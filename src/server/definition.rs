use std::{
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

use nix::sys::memfd::{MFdFlags, memfd_create};
use wayland_server::{
    Dispatch, GlobalDispatch, Resource,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{
        wl_buffer::WlBuffer, wl_callback::WlCallback, wl_compositor::WlCompositor,
        wl_data_device::WlDataDevice, wl_data_device_manager::WlDataDeviceManager,
        wl_data_source::WlDataSource, wl_keyboard::WlKeyboard, wl_output::WlOutput,
        wl_pointer::WlPointer, wl_region::WlRegion, wl_seat::WlSeat, wl_shm::WlShm,
        wl_shm_pool::WlShmPool, wl_subcompositor::WlSubcompositor, wl_subsurface::WlSubsurface,
        wl_surface::WlSurface,
    },
};

use wayland_protocols::{
    wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags,
    xdg::shell::server::{
        xdg_positioner::{self, XdgPositioner},
        xdg_surface::{self, XdgSurface},
        xdg_toplevel::{self, XdgToplevel},
        xdg_wm_base::{self, XdgWmBase},
    },
};

use wayland_protocols::wp::linux_dmabuf::zv1::server::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
};

use ash::vk;
use std::collections::HashMap;
use wayland_server::backend::ObjectId;

use crate::vulkan::{SurfaceTexture, VulkanContext};

#[derive(Clone)]
pub struct ShmBuffer {
    pub pool_id: ObjectId,
    pub offset: i32,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
}

pub struct DmabufData {
    pub fd: OwnedFd,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
    pub modifier: u64,
}

pub struct ServerState {
    pub surfaces: Vec<WlSurface>,
    pub windows: Vec<XdgToplevel>,

    pub vkctx: Rc<VulkanContext>,

    pub input_focus: Option<WlSurface>,
    mode: drm::control::Mode,

    pub pools: HashMap<ObjectId, (OwnedFd, memmap2::MmapMut)>,
    pub buffers: HashMap<ObjectId, ShmBuffer>,

    // Maps Surface ID -> WlBuffer
    pub surface_buffers: HashMap<ObjectId, wayland_server::protocol::wl_buffer::WlBuffer>,
    pub surface_textures: HashMap<ObjectId, SurfaceTexture>,
    pub cursor_surface: Option<(WlSurface, i32, i32)>,

    pub window_surfaces: Vec<WlSurface>,
    pub wm: Box<dyn crate::wm::WindowManager>,
    pub cursor_pos: (f64, f64),

    pub frame_callbacks: Vec<wayland_server::protocol::wl_callback::WlCallback>,

    pub keyboards: Vec<wayland_server::protocol::wl_keyboard::WlKeyboard>,
    pub keymap_fd: OwnedFd,
    pub keymap_size: u32,
    pub xkb_state: xkbcommon::xkb::State,

    pub pointers: Vec<wayland_server::protocol::wl_pointer::WlPointer>,
    pub pointer_focus: Option<WlSurface>,

    pub serial: u32,
    pub super_held: bool,

    pub pending_dmabufs: HashMap<ObjectId, DmabufData>,
    pub dmabuffers: HashMap<ObjectId, DmabufData>,

    pub gpu_dev_t: u64,
    pub dmabuf_table_fd: std::os::unix::io::OwnedFd,

    pub xdg_to_surface: HashMap<wayland_server::backend::ObjectId, WlSurface>,
}

impl ServerState {
    pub fn new(
        vkctx: Rc<VulkanContext>,
        mode: drm::control::Mode,
        gpu_dev_t: u64,
        dmabuf_table_fd: std::os::unix::io::OwnedFd,
    ) -> Self {
        use nix::unistd::{ftruncate, write};
        use xkbcommon::xkb;

        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "evdev",
            "pc105",
            "be",
            "oss",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("Failed to create XKB Keymap");

        let keymap_string = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let keymap_bytes = keymap_string.as_bytes();
        let keymap_size = keymap_bytes.len() as u32 + 1; // NOTE: +1 for the null terminator

        let keymap_fd = memfd_create(
            "pattern-keymap",
            MFdFlags::MFD_CLOEXEC | MFdFlags::MFD_ALLOW_SEALING,
        )
        .unwrap();

        ftruncate(keymap_fd.as_fd(), keymap_size as i64).unwrap();
        write(keymap_fd.as_fd(), keymap_bytes).unwrap();
        write(keymap_fd.as_fd(), &[0]).unwrap();

        let xkb_state = xkb::State::new(&keymap);

        Self {
            surfaces: Vec::new(),
            windows: Vec::new(),
            vkctx,
            input_focus: None,
            mode,

            pools: HashMap::new(),
            buffers: HashMap::new(),
            surface_buffers: HashMap::new(),
            surface_textures: HashMap::new(),
            cursor_surface: None,

            window_surfaces: Vec::new(),
            wm: Box::new(crate::wm::FloatingWm::new()),
            cursor_pos: (0., 0.),
            xdg_to_surface: HashMap::new(),

            frame_callbacks: Vec::new(),

            keyboards: Vec::new(),
            keymap_fd,
            keymap_size,
            xkb_state,

            pointers: Vec::new(),
            pointer_focus: None,

            serial: 1,
            super_held: false,

            pending_dmabufs: HashMap::new(),
            dmabuffers: HashMap::new(),
            gpu_dev_t,
            dmabuf_table_fd,
        }
    }
}

pub struct ClientState;

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl GlobalDispatch<WlCompositor, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlCompositor>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlCompositor, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlCompositor,
        request: wayland_server::protocol::wl_compositor::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_compositor::Request::CreateSurface { id } => {
                let surface = data_init.init(id, ());
                println!("[pattern]: Client requested a new surface!");
                state.surfaces.push(surface);
            }
            wayland_server::protocol::wl_compositor::Request::CreateRegion { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

// Shared Memory (SHM) Global
impl GlobalDispatch<WlShm, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlShm>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let shm = data_init.init(resource, ());

        shm.format(wayland_server::protocol::wl_shm::Format::Xrgb8888);
        shm.format(wayland_server::protocol::wl_shm::Format::Argb8888);
    }
}

impl Dispatch<WlShm, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlShm,
        request: wayland_server::protocol::wl_shm::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_shm::Request::CreatePool { id, size, fd, .. } = request
        {
            println!(
                "[pattern]: Client offered a Shared Memory Pool of size {}",
                size
            );
            let mmap = unsafe {
                memmap2::MmapOptions::new()
                    .len(size as usize)
                    .map_mut(&fd)
                    .unwrap()
            };
            let pool = data_init.init(id, ());
            state.pools.insert(pool.id(), (fd, mmap));
        }
    }
}

// Surface Handler
impl Dispatch<WlSurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        surface: &WlSurface,
        request: wayland_server::protocol::wl_surface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_surface::Request::Attach { buffer, .. } => {
                if let Some(buf) = buffer {
                    state.surface_buffers.insert(surface.id(), buf.clone());
                }
            }
            wayland_server::protocol::wl_surface::Request::Commit => {
                // Finding the buffer attached to this surface
                if let Some(buffer) = state.surface_buffers.get(&surface.id()) {
                    if let Some(buffer_info) = state.buffers.get(&buffer.id()) {
                        if let Some((_, mmap)) = state.pools.get(&buffer_info.pool_id) {
                            let start = buffer_info.offset as usize;
                            let len = (buffer_info.height * buffer_info.stride) as usize;
                            let pixels = &mmap[start..start + len];

                            unsafe {
                                // Cleanup old texture if this surface is resizing
                                if let Some(old) = state.surface_textures.remove(&surface.id()) {
                                    state.vkctx.device.destroy_sampler(old.samp, None);
                                    state.vkctx.device.destroy_image_view(old.view, None);
                                    state.vkctx.device.destroy_image(old.img, None);
                                    state.vkctx.device.free_memory(old.mem, None);
                                    state.vkctx.device.destroy_descriptor_pool(old.pool, None);
                                }

                                // Upload the new pixels
                                let (img, mem, view, samp) = state.vkctx.upload_texture(
                                    buffer_info.width as u32,
                                    buffer_info.height as u32,
                                    pixels,
                                );

                                // Bind it to a Descriptor Set
                                let (pool, set) = state.vkctx.create_descriptor_set(
                                    state.vkctx.descriptor_set_layout,
                                    view,
                                    samp,
                                );

                                #[rustfmt::skip]
                                state.surface_textures.insert(
                                    surface.id(),
                                    SurfaceTexture {
                                        img, mem, view, samp, pool, set,
                                        w: buffer_info.width as f32,
                                        h: buffer_info.height as f32,
                                    },
                                );
                            }
                        }
                    } else if let Some(dmabuf) = state.dmabuffers.get(&buffer.id()) {
                        unsafe {
                            // Cleanup old texture
                            if let Some(old) = state.surface_textures.remove(&surface.id()) {
                                state.vkctx.device.destroy_sampler(old.samp, None);
                                state.vkctx.device.destroy_image_view(old.view, None);
                                state.vkctx.device.destroy_image(old.img, None);
                                state.vkctx.device.free_memory(old.mem, None);
                                state.vkctx.device.destroy_descriptor_pool(old.pool, None);
                            }

                            let (img, mem) = state.vkctx.import_dmabuf(
                                &dmabuf.fd,
                                dmabuf.width,
                                dmabuf.height,
                                dmabuf.stride,
                                dmabuf.modifier,
                            );

                            let view_info = vk::ImageViewCreateInfo::default()
                                .image(img)
                                .view_type(vk::ImageViewType::TYPE_2D)
                                .format(vk::Format::B8G8R8A8_UNORM)
                                .subresource_range(
                                    vk::ImageSubresourceRange::default()
                                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                                        .level_count(1)
                                        .layer_count(1),
                                );

                            let view = state
                                .vkctx
                                .device
                                .create_image_view(&view_info, None)
                                .expect("Failed to create Image View for DMA-BUF");

                            let sampler_info = vk::SamplerCreateInfo::default()
                                .mag_filter(vk::Filter::LINEAR)
                                .min_filter(vk::Filter::LINEAR)
                                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);

                            let samp = state
                                .vkctx
                                .device
                                .create_sampler(&sampler_info, None)
                                .expect("Failed to create Sampler for DMA-BUF");

                            let (pool, set) = state.vkctx.create_descriptor_set(
                                state.vkctx.descriptor_set_layout,
                                view,
                                samp,
                            );

                            state.surface_textures.insert(
                                surface.id(),
                                SurfaceTexture {
                                    img,
                                    mem,
                                    view,
                                    samp,
                                    pool,
                                    set,
                                    w: dmabuf.width as f32,
                                    h: dmabuf.height as f32,
                                },
                            );
                        }
                    }

                    buffer.release();
                }
            }
            wayland_server::protocol::wl_surface::Request::Frame { callback } => {
                let cb = data_init.init(callback, ());
                state.frame_callbacks.push(cb);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCallback, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlCallback,
        _request: wayland_server::protocol::wl_callback::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

// Dummy handler for Window Regions (Hitboxes)
impl Dispatch<WlRegion, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlRegion,
        _request: wayland_server::protocol::wl_region::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlShmPool,
        request: wayland_server::protocol::wl_shm_pool::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_shm_pool::Request::CreateBuffer {
                id,
                offset,
                width,
                height,
                stride,
                ..
            } => {
                println!("[pattern]: Client carved a WlBuffer out of the SHM Pool");
                let buffer = data_init.init(id, ());
                state.buffers.insert(
                    buffer.id(),
                    ShmBuffer {
                        pool_id: resource.id(),
                        offset,
                        width,
                        height,
                        stride,
                    },
                );
            }

            wayland_server::protocol::wl_shm_pool::Request::Resize { size } => {
                println!("[pattern]: Client resized the SHM pool to {}", size);
                if let Some((fd, mmap)) = state.pools.get_mut(&resource.id()) {
                    *mmap = unsafe {
                        memmap2::MmapOptions::new()
                            .len(size as usize)
                            .map_mut(&fd.as_fd())
                            .unwrap()
                    };
                }
            }

            wayland_server::protocol::wl_shm_pool::Request::Destroy => {}

            _ => {}
        }
    }
}

// Dummy handler for the actual pixel buffer
impl Dispatch<WlBuffer, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlBuffer,
        _request: wayland_server::protocol::wl_buffer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        // Clients can send a "Destroy" request for buffers, we can safely ignore it for now.
    }
}

// The XDG Base Manager (The Global)
impl GlobalDispatch<XdgWmBase, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<XdgWmBase>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<XdgWmBase, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &XdgWmBase,
        request: xdg_wm_base::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_wm_base::Request::GetXdgSurface { id, surface } => {
                println!("[pattern]: Client upgraded a WlSurface to an XdgSurface");

                let xdg_surface = data_init.init(id, ());

                state.wm.map_window(surface.clone());
                state.xdg_to_surface.insert(xdg_surface.id(), surface);
            }
            xdg_wm_base::Request::CreatePositioner { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

// The XDG Surface
impl Dispatch<XdgSurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgSurface,
        request: xdg_surface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_surface::Request::GetToplevel { id } => {
                let toplevel = data_init.init(id, ());

                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.assign_toplevel(&surface.id(), toplevel.clone());
                }

                let state_val = xdg_toplevel::State::Activated as u32;
                let states_bytes = state_val.to_ne_bytes().to_vec();

                toplevel.configure(800, 600, states_bytes);

                resource.configure(1);
            }
            // xdg_surface::Request::GetPopup { id, .. } => {
            //     data_init.init(id, ());
            // }
            xdg_surface::Request::AckConfigure { .. } => {}
            _ => {}
        }
    }
}

// The XDG Toplevel
impl Dispatch<XdgToplevel, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgToplevel,
        request: xdg_toplevel::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_toplevel::Request::SetTitle { title } => {
                println!("[pattern]: Window title set to: {}", title);
                state.windows.push(resource.clone());
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
                println!("[pattern]: App ID set to: {}", app_id);
            }
            xdg_toplevel::Request::Move { seat: _, serial: _ } => {
                state.wm.begin_interactive_move(
                    &resource.id(),
                    state.cursor_pos.0,
                    state.cursor_pos.1,
                );
            }
            _ => {}
        }

        state.windows.push(resource.clone());
    }
}

// Dummy Handler for Positioner
impl Dispatch<XdgPositioner, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &XdgPositioner,
        _request: xdg_positioner::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<WlOutput, ()> for ServerState {
    fn bind(
        state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlOutput>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let output = data_init.init(resource, ());

        // Tell the client the screen size and refresh rate
        output.geometry(
            0,
            0, // X, Y offset
            state.mode.size().0 as i32,
            state.mode.size().1 as i32,
            wayland_server::protocol::wl_output::Subpixel::Unknown,
            "Pattern".to_string(),         // Make
            "Virtual Display".to_string(), // Model
            wayland_server::protocol::wl_output::Transform::Normal,
        );
        output.mode(
            wayland_server::protocol::wl_output::Mode::Current,
            state.mode.size().0 as i32,
            state.mode.size().1 as i32,
            (state.mode.vrefresh() * 1000) as i32,
        );

        // If the client bound to version 2 or higher,
        // we MUST send the scale and done events!
        if output.version() >= 2 {
            output.scale(1);
            output.done();
        }
    }
}

impl Dispatch<WlOutput, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlOutput,
        _request: wayland_server::protocol::wl_output::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

// The Seat (Input Hub)
impl GlobalDispatch<WlSeat, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlSeat>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let seat = data_init.init(resource, ());

        seat.capabilities(
            wayland_server::protocol::wl_seat::Capability::Pointer
                | wayland_server::protocol::wl_seat::Capability::Keyboard,
        );
        seat.name("pattern-seat".to_string());
    }
}

impl Dispatch<WlSeat, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlSeat,
        request: wayland_server::protocol::wl_seat::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_seat::Request::GetPointer { id } => {
                let pointer = data_init.init(id, ());
                state.pointers.push(pointer);
            }
            wayland_server::protocol::wl_seat::Request::GetKeyboard { id } => {
                let keyboard = data_init.init(id, ());
                let fd = state.keymap_fd.as_fd();
                keyboard.keymap(
                    wayland_server::protocol::wl_keyboard::KeymapFormat::XkbV1,
                    fd,
                    state.keymap_size,
                );

                if keyboard.version() >= 4 {
                    keyboard.repeat_info(35, 300);
                }

                // If this client already has focus, send an enter event immediately
                if let Some(focused_surface) = &state.input_focus {
                    if let Some(focused_client) = focused_surface.client() {
                        if focused_client.id() == _client.id() {
                            state.serial += 1;
                            keyboard.enter(state.serial, focused_surface, Vec::new());
                            keyboard.modifiers(state.serial, 0, 0, 0, 0);
                        }
                    }
                }

                state.keyboards.push(keyboard);
            }
            _ => {}
        }
    }
}

// Dummy handlers for the actual Pointer and Keyboard objects
impl Dispatch<WlPointer, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlPointer,
        request: wayland_server::protocol::wl_pointer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_pointer::Request::SetCursor {
            surface,
            hotspot_x,
            hotspot_y,
            ..
        } = request
        {
            if let Some(surf) = surface {
                state.cursor_surface = Some((surf, hotspot_x, hotspot_y));
            } else {
                state.cursor_surface = None;
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlKeyboard,
        _request: wayland_server::protocol::wl_keyboard::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

// THE SUBCOMPOSITOR (Required by GLFW/GTK)

impl GlobalDispatch<WlSubcompositor, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlSubcompositor>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlSubcompositor, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlSubcompositor,
        request: wayland_server::protocol::wl_subcompositor::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_subcompositor::Request::GetSubsurface { id, .. } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSubsurface, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlSubsurface,
        _request: wayland_server::protocol::wl_subsurface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<WlDataDeviceManager, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlDataDeviceManager>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlDataDeviceManager, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlDataDeviceManager,
        request: wayland_server::protocol::wl_data_device_manager::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_data_device_manager::Request::GetDataDevice {
                id, ..
            } => {
                data_init.init(id, ());
            }
            wayland_server::protocol::wl_data_device_manager::Request::CreateDataSource { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

// Dummy handler for the actual device
impl Dispatch<WlDataDevice, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlDataDevice,
        _request: wayland_server::protocol::wl_data_device::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

// Dummy handler for the data source
impl Dispatch<WlDataSource, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlDataSource,
        _request: wayland_server::protocol::wl_data_source::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

// DMA-BUF / HARDWARE ACCELERATION

impl GlobalDispatch<ZwpLinuxDmabufV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpLinuxDmabufV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let dmabuf = data_init.init(resource, ());

        if dmabuf.version() < 4 {
            // ARGB8888
            dmabuf.modifier(0x34325241, 0, 0);
            // XRGB8888
            dmabuf.modifier(0x34325258, 0, 0);
        }
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpLinuxDmabufV1,
        request: zwp_linux_dmabuf_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_linux_dmabuf_v1::Request::CreateParams { params_id } => {
                data_init.init(params_id, ());
            }
            zwp_linux_dmabuf_v1::Request::GetDefaultFeedback { id }
            | zwp_linux_dmabuf_v1::Request::GetSurfaceFeedback { id, .. } => {
                let feedback = data_init.init(id, ());

                // Send the sealed 32-byte format table first
                feedback.format_table(state.dmabuf_table_fd.as_fd(), 32);

                // Identify the compositor's core GPU
                let dev_bytes = state.gpu_dev_t.to_ne_bytes().to_vec();
                feedback.main_device(dev_bytes.clone());

                // Define the optimal format tranche
                feedback.tranche_target_device(dev_bytes);
                feedback.tranche_flags(TrancheFlags::empty());

                // Tell Mesa to look at both entries in our table
                let indices: [u16; 2] = [0, 1];
                let indices_bytes =
                    unsafe { std::slice::from_raw_parts(indices.as_ptr() as *const u8, 4) };

                feedback.tranche_formats(indices_bytes.to_vec());
                feedback.tranche_done();

                // Conclude the transaction
                feedback.done();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpLinuxDmabufFeedbackV1,
        _request: zwp_linux_dmabuf_feedback_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZwpLinuxBufferParamsV1,
        request: zwp_linux_buffer_params_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwp_linux_buffer_params_v1::Request::Add {
                fd,
                stride,
                modifier_hi,
                modifier_lo,
                ..
            } => {
                // The client gave us the raw GPU File Descriptor
                let modifier = ((modifier_hi as u64) << 32) | (modifier_lo as u64);

                // Store it temporarily in our pending map
                state.pending_dmabufs.insert(
                    resource.id(),
                    DmabufData {
                        fd,
                        width: 0,
                        height: 0, // Set in CreateImmed
                        stride,
                        format: 0,
                        modifier,
                    },
                );
            }
            zwp_linux_buffer_params_v1::Request::CreateImmed {
                buffer_id,
                width,
                height,
                format,
                ..
            } => {
                // The client finished defining the buffer. Give them a WlBuffer
                let wl_buffer = data_init.init(buffer_id, ());

                if let Some(mut data) = state.pending_dmabufs.remove(&resource.id()) {
                    data.width = width as u32;
                    data.height = height as u32;
                    data.format = format;
                    println!(
                        "[pattern]: Client successfully created a Hardware Accelerated DMA-BUF"
                    );
                    state.dmabuffers.insert(wl_buffer.id(), data);
                }
            }
            zwp_linux_buffer_params_v1::Request::Destroy => {
                state.pending_dmabufs.remove(&resource.id());
            }
            _ => {}
        }
    }
}

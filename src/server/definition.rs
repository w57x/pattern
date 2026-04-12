use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

use ash::vk;
use nix::sys::memfd::{MFdFlags, memfd_create};
use wayland_server::{
    Dispatch, GlobalDispatch, Resource, WEnum,
    backend::{ClientData, ClientId, DisconnectReason, ObjectId},
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
    wp::{
        linux_dmabuf::zv1::server::{
            zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
            zwp_linux_dmabuf_feedback_v1::{self, TrancheFlags, ZwpLinuxDmabufFeedbackV1},
            zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
        },
        viewporter::server::{
            wp_viewport::{self, WpViewport},
            wp_viewporter::{self, WpViewporter},
        },
    },
    xdg::{
        decoration::zv1::server::{
            zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
            zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        },
        shell::server::{
            xdg_popup::{self, XdgPopup},
            xdg_positioner::{self, XdgPositioner},
            xdg_surface::{self, XdgSurface},
            xdg_toplevel::{self, XdgToplevel},
            xdg_wm_base::{self, XdgWmBase},
        },
        xdg_output::zv1::server::{
            zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
            zxdg_output_v1,
        },
    },
};

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

#[derive(Clone)]
pub struct PositionerData {
    pub size: (i32, i32),
    pub anchor_rect: (i32, i32, i32, i32),
    pub offset: (i32, i32),
    pub anchor: WEnum<xdg_positioner::Anchor>,
    pub gravity: WEnum<xdg_positioner::Gravity>,
    pub constraint_adjustment: WEnum<xdg_positioner::ConstraintAdjustment>,
}

impl Default for PositionerData {
    fn default() -> Self {
        Self {
            size: (0, 0),
            anchor_rect: (0, 0, 0, 0),
            offset: (0, 0),
            anchor: WEnum::Value(xdg_positioner::Anchor::None),
            gravity: WEnum::Value(xdg_positioner::Gravity::None),
            constraint_adjustment: WEnum::Value(xdg_positioner::ConstraintAdjustment::None),
        }
    }
}

pub struct ServerState {
    pub surfaces: Vec<WlSurface>,
    pub windows: Vec<XdgToplevel>,

    pub vkctx: Rc<VulkanContext>,

    pub input_focus: Option<WlSurface>,
    pub mode: drm::control::Mode,

    pub pools: HashMap<ObjectId, (OwnedFd, memmap2::MmapMut)>,
    pub buffers: HashMap<ObjectId, ShmBuffer>,

    // Maps Surface ID -> WlBuffer
    pub surface_buffers: HashMap<ObjectId, wayland_server::protocol::wl_buffer::WlBuffer>,
    pub surface_textures: HashMap<ObjectId, SurfaceTexture>,
    pub cursor_surface: Option<(WlSurface, i32, i32)>,

    pub window_surfaces: Vec<WlSurface>,
    pub wm: Box<dyn crate::wm::WindowManager>,
    pub styler: Box<dyn crate::styler::Styler>,
    pub cursor_pos: (f64, f64),

    pub frame_callbacks: Vec<wayland_server::protocol::wl_callback::WlCallback>,

    pub keyboards: Vec<wayland_server::protocol::wl_keyboard::WlKeyboard>,
    pub keymap_fd: OwnedFd,
    pub keymap_size: u32,
    pub xkb_state: xkbcommon::xkb::State,

    pub pointers: Vec<wayland_server::protocol::wl_pointer::WlPointer>,
    pub pointer_focus: Option<WlSurface>,

    pub outputs: Vec<wayland_server::protocol::wl_output::WlOutput>,

    pub serial: u32,
    pub super_held: bool,

    pub pending_dmabufs: HashMap<ObjectId, DmabufData>,
    pub dmabuffers: HashMap<ObjectId, DmabufData>,

    pub gpu_dev_t: u64,
    pub dmabuf_table_fd: std::os::unix::io::OwnedFd,

    pub xdg_to_surface: HashMap<wayland_server::backend::ObjectId, WlSurface>,
    pub pending_positioners: HashMap<ObjectId, PositionerData>,
    pub subsurfaces: Vec<SubsurfaceData>,
    pub decoration_to_toplevel: HashMap<ObjectId, ObjectId>,
    pub pending_scales: HashMap<ObjectId, i32>,
    pub viewports: HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
    pub surface_to_viewport: HashMap<ObjectId, ObjectId>,
}

#[derive(Clone)]
pub struct SubsurfaceData {
    pub id: ObjectId,
    pub surface: WlSurface,
    pub parent: WlSurface,
    pub x: i32,
    pub y: i32,
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
            styler: Box::new(crate::styler::DefaultStyler::new()),
            cursor_pos: (0., 0.),

            frame_callbacks: Vec::new(),

            keyboards: Vec::new(),
            keymap_fd,
            keymap_size,
            xkb_state,

            pointers: Vec::new(),
            pointer_focus: None,

            outputs: Vec::new(),

            serial: 1,
            super_held: false,

            pending_dmabufs: HashMap::new(),
            dmabuffers: HashMap::new(),
            gpu_dev_t,
            dmabuf_table_fd,

            xdg_to_surface: HashMap::new(),
            pending_positioners: HashMap::new(),
            subsurfaces: Vec::new(),
            decoration_to_toplevel: HashMap::new(),
            pending_scales: HashMap::new(),
            viewports: HashMap::new(),
            surface_to_viewport: HashMap::new(),
        }
    }

    pub fn set_input_focus(&mut self, surface: WlSurface) {
        if self.input_focus.as_ref() == Some(&surface) {
            return;
        }

        if let Some(old_focus) = self.input_focus.take() {
            if let Some(old_client) = old_focus.client() {
                self.serial += 1;
                for keyboard in self
                    .keyboards
                    .iter()
                    .filter(|k| k.client().map(|c| c.id()) == Some(old_client.id()))
                {
                    keyboard.leave(self.serial, &old_focus);
                }
            }
        }

        self.input_focus = Some(surface.clone());
        if let Some(client) = surface.client() {
            self.serial += 1;
            for keyboard in self
                .keyboards
                .iter()
                .filter(|k| k.client().map(|c| c.id()) == Some(client.id()))
            {
                keyboard.enter(self.serial, &surface, Vec::new());

                let depressed = self
                    .xkb_state
                    .serialize_mods(xkbcommon::xkb::STATE_MODS_DEPRESSED);
                let latched = self
                    .xkb_state
                    .serialize_mods(xkbcommon::xkb::STATE_MODS_LATCHED);
                let locked = self
                    .xkb_state
                    .serialize_mods(xkbcommon::xkb::STATE_MODS_LOCKED);
                let group = self
                    .xkb_state
                    .serialize_layout(xkbcommon::xkb::STATE_LAYOUT_EFFECTIVE);
                keyboard.modifiers(self.serial, depressed, latched, locked, group);
            }
        }
    }

    pub fn set_pointer_focus(
        &mut self,
        surface: Option<WlSurface>,
        local_x: f64,
        local_y: f64,
        time: u32,
    ) {
        if self.pointer_focus == surface {
            if let Some(surf) = &self.pointer_focus {
                if let Some(client) = surf.client() {
                    for pointer in self
                        .pointers
                        .iter()
                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                    {
                        pointer.motion(time, local_x, local_y);
                        pointer.frame();
                    }
                }
            }
            return;
        }

        if let Some(old_focus) = self.pointer_focus.take() {
            if let Some(old_client) = old_focus.client() {
                self.serial += 1;
                for pointer in self
                    .pointers
                    .iter()
                    .filter(|p| p.client().map(|c| c.id()) == Some(old_client.id()))
                {
                    pointer.leave(self.serial, &old_focus);
                    pointer.frame();
                }
            }
        }

        self.pointer_focus = surface.clone();
        if let Some(surf) = surface {
            if let Some(client) = surf.client() {
                self.serial += 1;
                for pointer in self
                    .pointers
                    .iter()
                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                {
                    pointer.enter(self.serial, &surf, local_x, local_y);
                    pointer.frame();
                }
            }
        }
    }
}

pub struct ClientState;

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
        // We can't easily access state here because ClientData doesn't have it.
        // Usually, cleanup is done via Resource::Destroy handlers or in the main loop.
        // Since we already have a reaping loop in main.rs, it will handle is_alive() surfaces.
    }
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

                for output in &state.outputs {
                    if surface.client().map(|c| c.id()) == output.client().map(|c| c.id()) {
                        surface.enter(output);
                    }
                }

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
                                        scale: *state.pending_scales.get(&surface.id()).unwrap_or(&1),
                                    },
                                );
                                state.wm.refresh_window_dimensions(
                                    &surface.id(),
                                    buffer_info.width,
                                    buffer_info.height,
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
                                    scale: *state.pending_scales.get(&surface.id()).unwrap_or(&1),
                                },
                            );
                            state.wm.refresh_window_dimensions(
                                &surface.id(),
                                dmabuf.width as i32,
                                dmabuf.height as i32,
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
            wayland_server::protocol::wl_surface::Request::SetBufferScale { scale } => {
                state.pending_scales.insert(surface.id(), scale);
            }
            wayland_server::protocol::wl_surface::Request::Destroy => {
                let is_focused = state.input_focus.as_ref().map(|s| s.id()) == Some(surface.id());
                if is_focused {
                    state.input_focus = None;
                }
                if state.pointer_focus.as_ref().map(|s| s.id()) == Some(surface.id()) {
                    state.pointer_focus = None;
                }
                state.wm.unmap_window(&surface.id());
                state.wm.unmap_popup(&surface.id());
                state.surfaces.retain(|s| s.id() != surface.id());
                if let Some(vp_id) = state.surface_to_viewport.remove(&surface.id()) {
                    state.viewports.remove(&vp_id);
                }

                // If we lost focus, try to focus the next best thing
                if is_focused {
                    if let Some(next_window) = state.wm.get_render_list().last() {
                        state.set_input_focus(next_window.surface.clone());
                    }
                }
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

            wayland_server::protocol::wl_shm_pool::Request::Destroy => {
                state.pools.remove(&resource.id());
            }

            _ => {}
        }
    }
}

impl Dispatch<WlBuffer, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlBuffer,
        request: wayland_server::protocol::wl_buffer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_buffer::Request::Destroy => {
                state.buffers.remove(&resource.id());
                state.dmabuffers.remove(&resource.id());
            }
            _ => {}
        }
    }
}

// The XDG Decoration Manager
impl GlobalDispatch<ZxdgDecorationManagerV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgDecorationManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZxdgDecorationManagerV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZxdgDecorationManagerV1,
        request: wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::Request::GetToplevelDecoration { id, toplevel } => {
                let decoration: ZxdgToplevelDecorationV1 = data_init.init(id, ());
                state
                    .decoration_to_toplevel
                    .insert(decoration.id(), toplevel.id());

                if state.styler.supports_ssd() {
                    decoration.configure(wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ServerSide);
                } else {
                    decoration.configure(wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ClientSide);
                }
            }
            wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZxdgToplevelDecorationV1,
        request: wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Request::SetMode { mode } => {
                if let Some(toplevel_id) = state.decoration_to_toplevel.get(&resource.id()).cloned()
                {
                    let enabled = match mode {
                        wayland_server::WEnum::Value(
                            wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ServerSide,
                        ) => true,
                        _ => false,
                    };
                    state.wm.set_window_ssd(&toplevel_id, enabled);
                }
            }
            wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Request::UnsetMode => {
                if let Some(toplevel_id) = state.decoration_to_toplevel.get(&resource.id()).cloned()
                {
                    state.wm.set_window_ssd(&toplevel_id, false);
                }
            }
            wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Request::Destroy => {
                state.decoration_to_toplevel.remove(&resource.id());
            }
            _ => {}
        }
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

                state.xdg_to_surface.insert(xdg_surface.id(), surface);
            }
            xdg_wm_base::Request::CreatePositioner { id } => {
                data_init.init(id, ());
            }
            xdg_wm_base::Request::Destroy => {}
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

                let surface = state.xdg_to_surface.get(&resource.id()).cloned();

                if let Some(surface) = surface {
                    state.wm.map_window(surface.clone());
                    state
                        .wm
                        .assign_toplevel(&surface.id(), toplevel.clone(), resource.clone());
                    state.wm.focus_window(&surface.id());
                    state.set_input_focus(surface.clone());

                    // Update pointer focus immediately in case the new window is under the cursor
                    let (cx, cy) = state.cursor_pos;
                    let hit = state.styler.hit_test(
                        cx,
                        cy,
                        &state.wm.get_render_list(),
                        &state.wm.get_popups(),
                        &state.subsurfaces,
                        &state.surface_textures,
                        &state.viewports,
                        &state.surface_to_viewport,
                        state.wm.as_ref(),
                    );
                    state.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, 0);
                }

                let state_val = xdg_toplevel::State::Activated as u32;
                let states_bytes = state_val.to_ne_bytes().to_vec();

                toplevel.configure(800, 600, states_bytes);

                resource.configure(1);
            }
            xdg_surface::Request::GetPopup {
                id,
                parent,
                positioner,
            } => {
                let popup = data_init.init(id, ());
                let positioner_data = state
                    .pending_positioners
                    .get(&positioner.id())
                    .cloned()
                    .unwrap_or_default();

                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    if let Some(parent_xdg) = parent {
                        if let Some(parent_surface) = state.xdg_to_surface.get(&parent_xdg.id()) {
                            // Find parent geometry
                            let parent_geom = state
                                .wm
                                .get_render_list()
                                .iter()
                                .find(|w| w.surface.id() == parent_surface.id())
                                .map(|w| w.geometry)
                                .unwrap_or_default();

                            // Calculate position based on anchor_rect, anchor, gravity, and offset
                            // xdg_popup spec: anchor_rect is relative to parent window geometry
                            let mut x = parent_geom.x
                                + positioner_data.anchor_rect.0
                                + positioner_data.offset.0;
                            let mut y = parent_geom.y
                                + positioner_data.anchor_rect.1
                                + positioner_data.offset.1;

                            use xdg_positioner::{Anchor, Gravity};
                            match positioner_data.anchor {
                                wayland_server::WEnum::Value(Anchor::TopRight)
                                | wayland_server::WEnum::Value(Anchor::Right)
                                | wayland_server::WEnum::Value(Anchor::BottomRight) => {
                                    x += positioner_data.anchor_rect.2;
                                }
                                wayland_server::WEnum::Value(Anchor::Top)
                                | wayland_server::WEnum::Value(Anchor::Bottom) => {
                                    x += positioner_data.anchor_rect.2 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.anchor {
                                wayland_server::WEnum::Value(Anchor::BottomLeft)
                                | wayland_server::WEnum::Value(Anchor::Bottom)
                                | wayland_server::WEnum::Value(Anchor::BottomRight) => {
                                    y += positioner_data.anchor_rect.3;
                                }
                                wayland_server::WEnum::Value(Anchor::Left)
                                | wayland_server::WEnum::Value(Anchor::Right) => {
                                    y += positioner_data.anchor_rect.3 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.gravity {
                                wayland_server::WEnum::Value(Gravity::TopRight)
                                | wayland_server::WEnum::Value(Gravity::Right)
                                | wayland_server::WEnum::Value(Gravity::BottomRight) => {
                                    x -= positioner_data.size.0;
                                }
                                wayland_server::WEnum::Value(Gravity::Top)
                                | wayland_server::WEnum::Value(Gravity::Bottom) => {
                                    x -= positioner_data.size.0 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.gravity {
                                wayland_server::WEnum::Value(Gravity::BottomLeft)
                                | wayland_server::WEnum::Value(Gravity::Bottom)
                                | wayland_server::WEnum::Value(Gravity::BottomRight) => {
                                    y -= positioner_data.size.1;
                                }
                                wayland_server::WEnum::Value(Gravity::Left)
                                | wayland_server::WEnum::Value(Gravity::Right) => {
                                    y -= positioner_data.size.1 / 2;
                                }
                                _ => {}
                            }

                            // Clamp popup position to screen boundaries
                            let (sw, sh) = state.mode.size();
                            let (px, py) = state.wm.get_absolute_position(&parent_surface.id());
                            let abs_x = px + x as f64;
                            let abs_y = py + y as f64;

                            if abs_x + positioner_data.size.0 as f64 > sw as f64 {
                                x -= (abs_x + positioner_data.size.0 as f64 - sw as f64) as i32;
                            }
                            if abs_x < 0.0 {
                                x -= abs_x as i32;
                            }
                            if abs_y + positioner_data.size.1 as f64 > sh as f64 {
                                y -= (abs_y + positioner_data.size.1 as f64 - sh as f64) as i32;
                            }
                            if abs_y < 0.0 {
                                y -= abs_y as i32;
                            }

                            state.wm.map_popup(crate::wm::PopupState {
                                surface: surface.clone(),
                                xdg_surface: resource.clone(),
                                xdg_popup: popup.clone(),
                                parent_surface_id: parent_surface.id(),
                                x,
                                y,
                            });

                            state.serial += 1;
                            popup.configure(x, y, positioner_data.size.0, positioner_data.size.1);
                            resource.configure(state.serial);
                        }
                    }
                }
            }
            xdg_surface::Request::SetWindowGeometry {
                x,
                y,
                width,
                height,
            } => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.set_window_geometry(
                        &surface.id(),
                        crate::wm::Rect {
                            x,
                            y,
                            w: width,
                            h: height,
                        },
                    );
                }
            }
            xdg_surface::Request::AckConfigure { .. } => {}
            xdg_surface::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_window(&surface.id());
                    state.wm.unmap_popup(&surface.id());
                }
                state.xdg_to_surface.remove(&resource.id());
            }
            _ => {}
        }
    }
}

// The XDG Popup
impl Dispatch<XdgPopup, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgPopup,
        request: xdg_popup::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_popup::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_popup(&surface.id());
                }
            }
            xdg_popup::Request::Grab { .. } => {}
            xdg_popup::Request::Reposition { .. } => {}
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
                println!("[wm]: Window title set to: {}", title);
                state.wm.set_window_title(&resource.id(), title);
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
                println!("[wm]: App ID set to: {}", app_id);
                state.wm.set_window_app_id(&resource.id(), app_id);
            }
            xdg_toplevel::Request::SetParent { parent } => {
                let parent_id = parent.map(|p| p.id());
                state.wm.set_window_parent(&resource.id(), parent_id);
            }
            xdg_toplevel::Request::Move { seat: _, serial: _ } => {
                state.wm.begin_interactive_move(
                    &resource.id(),
                    state.cursor_pos.0,
                    state.cursor_pos.1,
                );
            }
            xdg_toplevel::Request::Resize {
                seat: _,
                serial: _,
                edges,
            } => {
                println!("[wm]: Resize - {edges:?}");
                state.wm.begin_interactive_resize(
                    &resource.id(),
                    edges.into(),
                    state.cursor_pos.0,
                    state.cursor_pos.1,
                );
            }
            xdg_toplevel::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_window(&surface.id());
                }
                state.windows.retain(|w| w.id() != resource.id());
                return;
            }
            _ => {}
        }

        if !state.windows.iter().any(|w| w.id() == resource.id()) {
            state.windows.push(resource.clone());
        }
    }
}

// Dummy Handler for Positioner
impl Dispatch<XdgPositioner, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgPositioner,
        request: xdg_positioner::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let entry = state.pending_positioners.entry(resource.id()).or_default();
        match request {
            xdg_positioner::Request::SetSize { width, height } => {
                entry.size = (width, height);
            }
            xdg_positioner::Request::SetAnchorRect {
                x,
                y,
                width,
                height,
            } => {
                entry.anchor_rect = (x, y, width, height);
            }
            xdg_positioner::Request::SetAnchor { anchor } => {
                entry.anchor = anchor;
            }
            xdg_positioner::Request::SetGravity { gravity } => {
                entry.gravity = gravity;
            }
            xdg_positioner::Request::SetConstraintAdjustment {
                constraint_adjustment,
            } => {
                entry.constraint_adjustment = constraint_adjustment;
            }
            xdg_positioner::Request::SetOffset { x, y } => {
                entry.offset = (x, y);
            }
            xdg_positioner::Request::Destroy => {
                state.pending_positioners.remove(&resource.id());
            }
            _ => {}
        }
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

        if output.version() >= 2 {
            output.scale(1);
        }
        if output.version() >= 4 {
            output.name("WL-1".to_string());
            output.description("Pattern Display".to_string());
        }
        if output.version() >= 2 {
            output.done();
        }

        // Send enter to all existing surfaces for this new client output bind
        for surface in &state.surfaces {
            if surface.client().map(|c| c.id()) == Some(_client.id()) {
                surface.enter(&output);
            }
        }

        state.outputs.push(output);
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
        resource: &WlPointer,
        request: wayland_server::protocol::wl_pointer::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_pointer::Request::SetCursor {
                surface,
                hotspot_x,
                hotspot_y,
                ..
            } => {
                if let Some(surf) = surface {
                    state.cursor_surface = Some((surf, hotspot_x, hotspot_y));
                } else {
                    state.cursor_surface = None;
                }
            }
            wayland_server::protocol::wl_pointer::Request::Release => {
                state.pointers.retain(|p| p.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlKeyboard,
        request: wayland_server::protocol::wl_keyboard::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_keyboard::Request::Release = request {
            state.keyboards.retain(|k| k.id() != resource.id());
        }
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
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlSubcompositor,
        request: wayland_server::protocol::wl_subcompositor::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_subcompositor::Request::GetSubsurface {
                id,
                surface,
                parent,
            } => {
                let subsurface = data_init.init(id, ());
                state.subsurfaces.push(SubsurfaceData {
                    id: subsurface.id(),
                    surface: surface.clone(),
                    parent: parent.clone(),
                    x: 0,
                    y: 0,
                });
            }
            wayland_server::protocol::wl_subcompositor::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<WlSubsurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlSubsurface,
        request: wayland_server::protocol::wl_subsurface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_subsurface::Request::SetPosition { x, y } => {
                if let Some(sub) = state.subsurfaces.iter_mut().find(|s| s.id == resource.id()) {
                    sub.x = x;
                    sub.y = y;
                }
            }
            wayland_server::protocol::wl_subsurface::Request::PlaceAbove { sibling } => {
                let idx = state.subsurfaces.iter().position(|s| s.id == resource.id());
                let sibling_idx = state
                    .subsurfaces
                    .iter()
                    .position(|s| s.surface.id() == sibling.id());
                if let (Some(idx), Some(s_idx)) = (idx, sibling_idx) {
                    let sub = state.subsurfaces.remove(idx);
                    // If sibling was at s_idx, and we removed idx which was < s_idx, new s_idx is s_idx - 1.
                    // But we want to be ABOVE sibling, so we insert at new_s_idx + 1.
                    let new_idx = if idx < s_idx { s_idx } else { s_idx + 1 };
                    state.subsurfaces.insert(new_idx, sub);
                }
            }
            wayland_server::protocol::wl_subsurface::Request::PlaceBelow { sibling } => {
                let idx = state.subsurfaces.iter().position(|s| s.id == resource.id());
                let sibling_idx = state
                    .subsurfaces
                    .iter()
                    .position(|s| s.surface.id() == sibling.id());
                if let (Some(idx), Some(s_idx)) = (idx, sibling_idx) {
                    let sub = state.subsurfaces.remove(idx);
                    let new_idx = if idx < s_idx { s_idx - 1 } else { s_idx };
                    state.subsurfaces.insert(new_idx, sub);
                }
            }
            wayland_server::protocol::wl_subsurface::Request::SetSync => {}
            wayland_server::protocol::wl_subsurface::Request::SetDesync => {}
            wayland_server::protocol::wl_subsurface::Request::Destroy => {
                state.subsurfaces.retain(|s| s.id != resource.id());
            }
            _ => {}
        }
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

// Viewporter
impl GlobalDispatch<WpViewporter, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpViewporter>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpViewporter, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WpViewporter,
        request: wp_viewporter::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_viewporter::Request::GetViewport { id, surface } => {
                let viewport = data_init.init(id, ());
                state
                    .surface_to_viewport
                    .insert(surface.id(), viewport.id());
            }
            wp_viewporter::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<WpViewport, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WpViewport,
        request: wp_viewport::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let entry = state.viewports.entry(resource.id()).or_insert((None, None));
        match request {
            wp_viewport::Request::SetSource {
                x,
                y,
                width,
                height,
            } => {
                if x == -1.0 {
                    entry.0 = None;
                } else {
                    entry.0 = Some((x, y, width, height));
                }
            }
            wp_viewport::Request::SetDestination { width, height } => {
                if width == -1 {
                    entry.1 = None;
                } else {
                    entry.1 = Some((width, height));
                }
            }
            wp_viewport::Request::Destroy => {
                state.viewports.remove(&resource.id());
            }
            _ => {}
        }
    }
}

// XDG Output
impl GlobalDispatch<ZxdgOutputManagerV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgOutputManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zxdg_output_manager_v1::Request::GetXdgOutput { id, output } => {
                let xdg_output = data_init.init(id, output.clone());
                let (w, h) = state.mode.size();
                xdg_output.logical_position(0, 0);
                xdg_output.logical_size(w as i32, h as i32);
                if xdg_output.version() >= 2 {
                    xdg_output.name("PatternDisplay".to_string());
                    xdg_output.description("Pattern Virtual Output".to_string());
                }
                xdg_output.done();
            }
            zxdg_output_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, WlOutput> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zxdg_output_v1::ZxdgOutputV1,
        request: zxdg_output_v1::Request,
        _output: &WlOutput,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let zxdg_output_v1::Request::Destroy = request {}
    }
}

impl GlobalDispatch<zxdg_output_v1::ZxdgOutputV1, WlOutput> for ServerState {
    fn bind(
        state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<zxdg_output_v1::ZxdgOutputV1>,
        output: &WlOutput,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let xdg_output = data_init.init(resource, output.clone());
        let (w, h) = state.mode.size();
        xdg_output.logical_position(0, 0);
        xdg_output.logical_size(w as i32, h as i32);
        if xdg_output.version() >= 2 {
            xdg_output.name("PatternDisplay".to_string());
            xdg_output.description("Pattern Virtual Output".to_string());
        }
        xdg_output.done();
    }
}

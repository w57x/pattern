pub mod proto;

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

use nix::sys::memfd::{MFdFlags, memfd_create};
use wayland_server::{
    Resource, WEnum,
    backend::{ClientData, ClientId, DisconnectReason, ObjectId},
    protocol::{wl_data_device, wl_data_source, wl_surface::WlSurface},
};

use wayland_protocols::{
    wp::primary_selection::zv1::server::{
        zwp_primary_selection_device_v1, zwp_primary_selection_source_v1,
    },
    xdg::shell::server::{xdg_positioner, xdg_toplevel::XdgToplevel},
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
    pub offset: u32,
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
    pub active_dmabufs: HashMap<ObjectId, wayland_server::protocol::wl_buffer::WlBuffer>,
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

    pub data_sources: HashMap<ObjectId, (wl_data_source::WlDataSource, Vec<String>)>,
    pub selection: Option<wl_data_source::WlDataSource>,
    pub data_devices: Vec<wl_data_device::WlDataDevice>,

    pub primary_selection_sources: HashMap<
        ObjectId,
        (
            zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
            Vec<String>,
        ),
    >,
    pub primary_selection: Option<zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1>,
    pub primary_selection_devices:
        Vec<zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1>,

    pub regions: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub pending_damage: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub pending_input_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub pending_opaque_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub surface_input_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub surface_opaque_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
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
            active_dmabufs: HashMap::new(),
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

            data_sources: HashMap::new(),
            selection: None,
            data_devices: Vec::new(),

            primary_selection_sources: HashMap::new(),
            primary_selection: None,
            primary_selection_devices: Vec::new(),

            regions: HashMap::new(),
            pending_damage: HashMap::new(),
            pending_input_region: HashMap::new(),
            pending_opaque_region: HashMap::new(),
            surface_input_region: HashMap::new(),
            surface_opaque_region: HashMap::new(),
        }
    }

    pub fn set_input_focus(&mut self, surface: WlSurface, dh: &wayland_server::DisplayHandle) {
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

            // Send selection offer to the newly focused client
            if let Some(source) = &self.selection {
                for device in &self.data_devices {
                    if device.client().map(|c| c.id()) == Some(client.id()) {
                        use wayland_server::protocol::wl_data_offer::WlDataOffer;
                        let offer = client
                            .create_resource::<WlDataOffer, (), Self>(dh, device.version(), ())
                            .expect("Failed to create WlDataOffer");
                        device.data_offer(&offer);

                        if let Some((_, mime_types)) = self.data_sources.get(&source.id()) {
                            for mime in mime_types {
                                offer.offer(mime.clone());
                            }
                        }
                        device.selection(Some(&offer));
                    }
                }
            } else {
                for device in &self.data_devices {
                    if device.client().map(|c| c.id()) == Some(client.id()) {
                        device.selection(None);
                    }
                }
            }

            // Send primary selection offer to the newly focused client
            if let Some(source) = &self.primary_selection {
                for device in &self.primary_selection_devices {
                    if device.client().map(|c| c.id()) == Some(client.id()) {
                        use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1;
                        let offer = client
                            .create_resource::<ZwpPrimarySelectionOfferV1, (), Self>(
                                dh,
                                device.version(),
                                (),
                            )
                            .expect("Failed to create ZwpPrimarySelectionOfferV1");
                        device.data_offer(&offer);

                        if let Some((_, mime_types)) =
                            self.primary_selection_sources.get(&source.id())
                        {
                            for mime in mime_types {
                                offer.offer(mime.clone());
                            }
                        }
                        device.selection(Some(&offer));
                    }
                }
            } else {
                for device in &self.primary_selection_devices {
                    if device.client().map(|c| c.id()) == Some(client.id()) {
                        device.selection(None);
                    }
                }
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
        // Reap dead resources
    }
}

pub mod cursor_loader;
pub mod proto;

use std::{
    collections::{HashMap, HashSet},
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

use ash::vk;
use nix::sys::memfd::{MFdFlags, memfd_create};
use rand::prelude::*;
use tracing::error;
use wayland_server::{
    Resource, WEnum,
    backend::{ClientData, ClientId, DisconnectReason, ObjectId},
    protocol::{
        wl_buffer::WlBuffer, wl_callback::WlCallback, wl_data_device, wl_data_offer::WlDataOffer,
        wl_data_source, wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_surface::WlSurface,
    },
};

use wayland_protocols::{
    wp::{
        cursor_shape::v1::server::wp_cursor_shape_device_v1,
        pointer_constraints::zv1::server::{zwp_confined_pointer_v1, zwp_locked_pointer_v1},
        pointer_gestures::zv1::server::{
            zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1,
        },
        presentation_time::server::wp_presentation_feedback::WpPresentationFeedback,
        primary_selection::zv1::server::{
            zwp_primary_selection_device_v1,
            zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
            zwp_primary_selection_source_v1,
        },
        relative_pointer::zv1::server::zwp_relative_pointer_v1,
    },
    xdg::shell::server::{xdg_positioner, xdg_toplevel::XdgToplevel},
};

use crate::{
    gpu::CardInfo,
    server::cursor_loader::CursorManager,
    vulkan::{SurfaceTexture, VulkanContext},
};

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

pub struct Composer {
    rng: ThreadRng,

    pub surfaces: Vec<WlSurface>,
    pub windows: Vec<XdgToplevel>,

    pub vkctx: Rc<VulkanContext>,

    pub input_focus: Option<WlSurface>,
    pub mode: drm::control::Mode,
    pub card_info: CardInfo,

    pub pools: HashMap<ObjectId, (OwnedFd, memmap2::MmapMut)>,
    pub buffers: HashMap<ObjectId, ShmBuffer>,

    // Maps Surface ID -> WlBuffer
    pub surface_buffers: HashMap<ObjectId, WlBuffer>,
    pub active_dmabufs: HashMap<ObjectId, WlBuffer>,
    pub surface_textures: HashMap<ObjectId, SurfaceTexture>,
    pub cursor_manager: CursorManager,
    pub cursor_surface: Option<(WlSurface, i32, i32)>,
    pub cursor_shape: Option<wp_cursor_shape_device_v1::Shape>,

    pub window_surfaces: Vec<WlSurface>,
    pub wm: Box<dyn crate::wm::WindowManager>,
    pub styler: Box<dyn crate::styler::Styler>,
    pub cursor_pos: (f64, f64),

    pub pending_frame_callbacks: HashMap<ObjectId, Vec<WlCallback>>,
    pub active_frame_callbacks: Vec<WlCallback>,

    pub keyboards: Vec<WlKeyboard>,
    pub keymap_fd: OwnedFd,
    pub keymap_size: u32,
    pub xkb_state: xkbcommon::xkb::State,

    pub pointers: Vec<WlPointer>,
    pub pointer_focus: Option<WlSurface>,
    pub pointer_grab: Option<WlSurface>,

    pub relative_pointers: Vec<zwp_relative_pointer_v1::ZwpRelativePointerV1>,
    pub pointer_lock: Option<zwp_locked_pointer_v1::ZwpLockedPointerV1>,
    pub pointer_confine: Option<zwp_confined_pointer_v1::ZwpConfinedPointerV1>,
    pub cursor_pos_hint: Option<(f64, f64)>,

    pub swipe_gestures:
        HashMap<ObjectId, Vec<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1>>,
    pub pinch_gestures:
        HashMap<ObjectId, Vec<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1>>,
    pub hold_gestures: HashMap<ObjectId, Vec<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1>>,

    pub outputs: Vec<wayland_server::protocol::wl_output::WlOutput>,

    pub pending_syncobj_state:
        HashMap<ObjectId, crate::server::proto::linux_drm_syncobj::SurfaceSyncObjState>,
    pub syncobj_state:
        HashMap<ObjectId, crate::server::proto::linux_drm_syncobj::SurfaceSyncObjState>,
    pub syncobj_timelines: HashMap<ObjectId, ash::vk::Semaphore>,
    pub explicit_sync_surfaces: HashSet<ObjectId>,
    pub buffer_textures: HashMap<ObjectId, crate::vulkan::SurfaceTexture>,
    pub dead_semaphores: Vec<vk::Semaphore>,
    pub needs_redraw: bool,

    pub pending_presentation_feedbacks: HashMap<ObjectId, Vec<WpPresentationFeedback>>,
    pub surface_presentation_feedbacks: HashMap<ObjectId, Vec<WpPresentationFeedback>>,
    pub feedbacks_to_present: Vec<WpPresentationFeedback>,

    pub serial: u32,

    pub pending_dmabufs: HashMap<ObjectId, DmabufData>,
    pub dmabuffers: HashMap<ObjectId, DmabufData>,

    pub gpu_dev_t: u64,
    pub dmabuf_table_fd: std::os::unix::io::OwnedFd,

    pub xdg_to_surface: HashMap<wayland_server::backend::ObjectId, WlSurface>,
    pub pending_positioners: HashMap<ObjectId, PositionerData>,
    pub subsurfaces: Vec<SubsurfaceData>,
    pub decoration_to_toplevel: HashMap<ObjectId, ObjectId>,
    pub dialog_to_toplevel: HashMap<ObjectId, ObjectId>,
    pub activation_tokens: HashSet<String>,
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

    pub last_enter_serial: HashMap<ClientId, u32>,

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

impl Composer {
    pub fn new(
        vkctx: Rc<VulkanContext>,
        mode: drm::control::Mode,
        card_info: CardInfo,
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
            rng: ThreadRng::default(),
            surfaces: Vec::new(),
            windows: Vec::new(),
            vkctx,
            input_focus: None,
            mode,
            card_info,

            pools: HashMap::new(),
            buffers: HashMap::new(),
            surface_buffers: HashMap::new(),
            active_dmabufs: HashMap::new(),
            surface_textures: HashMap::new(),
            cursor_manager: CursorManager::new("Adwaita", 24),
            cursor_surface: None,
            cursor_shape: None,

            window_surfaces: Vec::new(),
            wm: Box::new(crate::wm::FloatingWm::new()),
            styler: Box::new(crate::styler::DefaultStyler::new()),
            cursor_pos: (0., 0.),

            pending_frame_callbacks: HashMap::new(),
            active_frame_callbacks: Vec::new(),

            keyboards: Vec::new(),
            keymap_fd,
            keymap_size,
            xkb_state,

            pointers: Vec::new(),
            swipe_gestures: HashMap::new(),
            pinch_gestures: HashMap::new(),
            hold_gestures: HashMap::new(),
            pointer_focus: None,
            pointer_grab: None,

            relative_pointers: Vec::new(),
            pointer_lock: None,
            pointer_confine: None,
            cursor_pos_hint: None,

            outputs: Vec::new(),

            pending_syncobj_state: HashMap::new(),
            syncobj_state: HashMap::new(),
            syncobj_timelines: HashMap::new(),
            explicit_sync_surfaces: HashSet::new(),
            buffer_textures: HashMap::new(),
            dead_semaphores: vec![],
            needs_redraw: true,

            pending_presentation_feedbacks: HashMap::new(),
            surface_presentation_feedbacks: HashMap::new(),
            feedbacks_to_present: Vec::new(),

            serial: 1,

            pending_dmabufs: HashMap::new(),
            dmabuffers: HashMap::new(),
            gpu_dev_t,
            dmabuf_table_fd,

            xdg_to_surface: HashMap::new(),
            pending_positioners: HashMap::new(),
            subsurfaces: Vec::new(),
            decoration_to_toplevel: HashMap::new(),
            dialog_to_toplevel: HashMap::new(),
            activation_tokens: std::collections::HashSet::new(),
            pending_scales: HashMap::new(),
            viewports: HashMap::new(),
            surface_to_viewport: HashMap::new(),

            data_sources: HashMap::new(),
            selection: None,
            data_devices: Vec::new(),

            primary_selection_sources: HashMap::new(),
            primary_selection: None,
            primary_selection_devices: Vec::new(),

            last_enter_serial: HashMap::new(),

            regions: HashMap::new(),
            pending_damage: HashMap::new(),
            pending_input_region: HashMap::new(),
            pending_opaque_region: HashMap::new(),
            surface_input_region: HashMap::new(),
            surface_opaque_region: HashMap::new(),
        }
    }

    pub fn load_cursor_shape(&mut self, shape: wp_cursor_shape_device_v1::Shape) {
        self.cursor_manager.get_or_load(shape, &self.vkctx)
    }

    pub fn get_surface_position(&self, surface_id: &ObjectId) -> Option<(f64, f64)> {
        // 1. Check if it's a toplevel or popup managed by WM
        if let Some((wm_x, wm_y)) = self.wm.get_surface_position(surface_id) {
            return Some((wm_x, wm_y));
        }

        // 2. Check if it's a subsurface
        if let Some(sub) = self
            .subsurfaces
            .iter()
            .find(|s| s.surface.id() == *surface_id)
        {
            if let Some((px, py)) = self.get_surface_position(&sub.parent.id()) {
                return Some((px + sub.x as f64, py + sub.y as f64));
            }
        }

        None
    }

    pub fn get_or_import_timeline_semaphore(
        &mut self,
        timeline_id: &ObjectId,
        dh: &wayland_server::DisplayHandle,
    ) -> Option<vk::Semaphore> {
        if let Some(sem) = self.syncobj_timelines.get(timeline_id) {
            return Some(*sem);
        }

        let timeline = match wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_timeline_v1::WpLinuxDrmSyncobjTimelineV1::from_id(dh, timeline_id.clone()) {
            Ok(t) => t,
            Err(_) => {
                error!("Failed to create resource from id for {:?}", timeline_id);
                return None;
            }
        };

        let timeline_data =
            match timeline.data::<crate::server::proto::linux_drm_syncobj::Timeline>() {
                Some(data) => data,
                None => {
                    error!("Failed to get timeline data for {:?}", timeline_id);
                    return None;
                }
            };

        let fd_dup = match timeline_data.fd.try_clone() {
            Ok(fd) => fd,
            Err(e) => {
                error!("try_clone failed for {:?}: {:?}", timeline_id, e);
                return None;
            }
        };
        let sem = match unsafe { self.vkctx.import_syncobj_as_semaphore(fd_dup) } {
            Ok(sem) => sem,
            Err(e) => {
                error!(
                    "import_syncobj_as_semaphore failed for {:?}: {:?}",
                    timeline_id, e
                );
                return None;
            }
        };
        self.syncobj_timelines.insert(timeline_id.clone(), sem);
        Some(sem)
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

                // Send NULL selection events to the client losing focus
                self.clear_selection(&old_client);
                self.clear_primary_selection(&old_client);
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

            // Send selection offers to the newly focused client
            self.send_selection_offer(&client, dh);
            self.send_primary_selection_offer(&client, dh);
        }
    }

    pub fn cleanup_surface(&mut self, id: &ObjectId) {
        self.wm.unmap_window(id);
        self.wm.unmap_popup(id);
        self.xdg_to_surface.remove(id);
        self.surface_textures.remove(id);
        self.active_dmabufs.remove(id);
        self.pending_syncobj_state.remove(id);
        self.syncobj_state.remove(id);
        self.surface_buffers.remove(id);
        self.explicit_sync_surfaces.remove(id);

        if self.input_focus.as_ref().map(|s| s.id()) == Some(id.clone()) {
            self.input_focus = None;
        }
        if self.pointer_focus.as_ref().map(|s| s.id()) == Some(id.clone()) {
            self.pointer_focus = None;
        }
        if self.pointer_grab.as_ref().map(|s| s.id()) == Some(id.clone()) {
            let mut shifted = false;
            if let Some(sub) = self.subsurfaces.iter().find(|s| s.surface.id() == *id) {
                if sub.parent.is_alive() {
                    self.pointer_grab = Some(sub.parent.clone());
                    shifted = true;
                }
            }
            if !shifted {
                self.pointer_grab = None;
            }
        }

        self.surfaces.retain(|s| &s.id() != id);
        self.subsurfaces.retain(|s| &s.surface.id() != id);
    }

    pub fn send_selection_offer(
        &self,
        client: &wayland_server::Client,
        dh: &wayland_server::DisplayHandle,
    ) {
        if let Some(source) = &self.selection {
            for device in &self.data_devices {
                if device.client().map(|c| c.id()) == Some(client.id()) {
                    // Don't send the selection to the client that owns it
                    if source.client().map(|c| c.id()) == Some(client.id()) {
                        continue;
                    }

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
            self.clear_selection(client);
        }
    }

    pub fn send_primary_selection_offer(
        &self,
        client: &wayland_server::Client,
        dh: &wayland_server::DisplayHandle,
    ) {
        if let Some(source) = &self.primary_selection {
            for device in &self.primary_selection_devices {
                if device.client().map(|c| c.id()) == Some(client.id()) {
                    // Don't send the selection to the client that owns it
                    if source.client().map(|c| c.id()) == Some(client.id()) {
                        continue;
                    }

                    let offer = client
                        .create_resource::<ZwpPrimarySelectionOfferV1, (), Self>(
                            dh,
                            device.version(),
                            (),
                        )
                        .expect("Failed to create ZwpPrimarySelectionOfferV1");
                    device.data_offer(&offer);

                    if let Some((_, mime_types)) = self.primary_selection_sources.get(&source.id())
                    {
                        for mime in mime_types {
                            offer.offer(mime.clone());
                        }
                    }
                    device.selection(Some(&offer));
                }
            }
        } else {
            self.clear_primary_selection(client);
        }
    }

    pub fn clear_selection(&self, client: &wayland_server::Client) {
        for device in &self.data_devices {
            if device.client().map(|c| c.id()) == Some(client.id()) {
                device.selection(None);
            }
        }
    }

    pub fn clear_primary_selection(&self, client: &wayland_server::Client) {
        for device in &self.primary_selection_devices {
            if device.client().map(|c| c.id()) == Some(client.id()) {
                device.selection(None);
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

                // Only clear cursor if moving to a different client
                let new_client_id = surface.as_ref().and_then(|s| s.client().map(|c| c.id()));
                if new_client_id != Some(old_client.id()) {
                    self.cursor_surface = None;
                    self.cursor_shape = None;
                }
            } else {
                self.cursor_surface = None;
                self.cursor_shape = None;
            }
        }

        self.pointer_focus = surface.clone();
        if let Some(surf) = surface {
            if let Some(client) = surf.client() {
                self.serial += 1;
                self.last_enter_serial.insert(client.id(), self.serial);
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

    pub unsafe fn drop_semaphores(&mut self) {
        for sem in self.dead_semaphores.drain(..) {
            unsafe {
                self.vkctx.device.destroy_semaphore(sem, None);
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

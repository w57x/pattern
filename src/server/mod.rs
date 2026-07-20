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
use wayland_protocols_wlr::data_control::v1::server::{
    zwlr_data_control_device_v1, zwlr_data_control_offer_v1, zwlr_data_control_source_v1,
};
use wayland_server::{
    Client, Resource,
    backend::{ClientData, ClientId, DisconnectReason, ObjectId},
    protocol::{
        wl_buffer::WlBuffer, wl_callback::WlCallback, wl_data_device, wl_data_offer::WlDataOffer,
        wl_data_source, wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
};

use wayland_protocols::{
    wp::{
        cursor_shape::v1::server::wp_cursor_shape_device_v1,
        linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_timeline_v1::WpLinuxDrmSyncobjTimelineV1,
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
        text_input::zv3::server::zwp_text_input_v3::ZwpTextInputV3,
    },
    xdg::shell::server::{
        xdg_popup::XdgPopup, xdg_positioner, xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel,
    },
};

use wayland_protocols_misc::zwp_input_method_v2::server::{
    zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
    zwp_input_method_v2::ZwpInputMethodV2, zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
};

use crate::{
    config::CompositorCommand,
    gpu::CardInfo,
    server::{
        cursor_loader::CursorManager,
        proto::{
            linux_drm_syncobj::{self, SurfaceSyncObjState},
            session_lock::SessionLockState,
            text_input::TextInputState,
        },
    },
    vulkan::{SurfaceTexture, VulkanContext},
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ShmBuffer {
    pub pool_id: ObjectId,
    pub offset: i32,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub mmap: Arc<Mutex<memmap2::MmapMut>>,
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
    pub anchor: xdg_positioner::Anchor,
    pub gravity: xdg_positioner::Gravity,
    pub constraint_adjustment: xdg_positioner::ConstraintAdjustment,
}

impl Default for PositionerData {
    fn default() -> Self {
        Self {
            size: (0, 0),
            anchor_rect: (0, 0, 0, 0),
            offset: (0, 0),
            anchor: xdg_positioner::Anchor::None,
            gravity: xdg_positioner::Gravity::None,
            constraint_adjustment: xdg_positioner::ConstraintAdjustment::None,
        }
    }
}

#[derive(Clone)]
pub enum SelectionSource {
    Standard(wl_data_source::WlDataSource),
    Primary(zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1),
    DataControl(zwlr_data_control_source_v1::ZwlrDataControlSourceV1),
}

impl SelectionSource {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Standard(s) => s.id(),
            Self::Primary(s) => s.id(),
            Self::DataControl(s) => s.id(),
        }
    }

    pub fn client(&self) -> Option<Client> {
        match self {
            Self::Standard(s) => s.client(),
            Self::Primary(s) => s.client(),
            Self::DataControl(s) => s.client(),
        }
    }

    pub fn cancelled(&self) {
        match self {
            Self::Standard(s) => s.cancelled(),
            Self::Primary(s) => s.cancelled(),
            Self::DataControl(s) => s.cancelled(),
        }
    }

    pub fn send(&self, mime_type: String, fd: std::os::unix::io::BorrowedFd<'_>) {
        match self {
            Self::Standard(s) => s.send(mime_type, fd),
            Self::Primary(s) => s.send(mime_type, fd),
            Self::DataControl(s) => s.send(mime_type, fd),
        }
    }

    pub fn target(&self, mime_type: Option<String>) {
        match self {
            Self::Standard(s) => s.target(mime_type),
            Self::Primary(_) => {
                // Primary selection does not have a target event
            }
            Self::DataControl(_) => {
                // Data control sources don't support explicit target feedback
            }
        }
    }
}

pub struct LayerState {
    pub size: Option<(u32, u32)>,
    pub anchor: Option<u32>,
    pub zone: Option<i32>,
    pub margin: Option<(i32, i32, i32, i32)>,
    pub interactivity: Option<u32>,
}

pub struct Composer {
    rng: ThreadRng,

    pub surfaces: Vec<WlSurface>,
    pub windows: Vec<XdgToplevel>,

    pub vkctx: Rc<VulkanContext>,

    pub input_focus: Option<WlSurface>,
    pub mode: drm::control::Mode,
    pub card_info: CardInfo,
    pub outputs_info: Vec<crate::gpu::OutputLayoutInfo>,

    pub pools: HashMap<ObjectId, (OwnedFd, Arc<Mutex<memmap2::MmapMut>>)>,
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
    pub config_manager: crate::config::ConfigManager,

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
    pub wl_output_globals: Vec<wayland_server::backend::GlobalId>,

    pub pending_syncobj_state: HashMap<ObjectId, SurfaceSyncObjState>,
    pub syncobj_state: HashMap<ObjectId, SurfaceSyncObjState>,
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
    pub pending_gamma: Option<Vec<crate::gpu::DrmColorLut>>,

    pub data_sources: HashMap<ObjectId, (wl_data_source::WlDataSource, Vec<String>)>,
    pub data_control_sources: HashMap<
        ObjectId,
        (
            zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
            Vec<String>,
        ),
    >,
    pub selection: Option<SelectionSource>,
    pub data_devices: Vec<wl_data_device::WlDataDevice>,

    pub primary_selection_sources: HashMap<
        ObjectId,
        (
            zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
            Vec<String>,
        ),
    >,
    pub primary_selection: Option<SelectionSource>,
    pub primary_selection_devices:
        Vec<zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1>,
    pub data_control_devices: Vec<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1>,

    pub text_inputs: Vec<(ZwpTextInputV3, WlSeat, TextInputState)>,

    pub input_methods: Vec<(ZwpInputMethodV2, WlSeat)>,
    pub input_popups: Vec<(ZwpInputPopupSurfaceV2, WlSurface, ZwpInputMethodV2)>,
    pub input_method_grabs: Vec<(ZwpInputMethodKeyboardGrabV2, ZwpInputMethodV2)>,

    pub unparented_popups: HashMap<ObjectId, (WlSurface, XdgSurface, XdgPopup, PositionerData)>,

    pub last_enter_serial: HashMap<ClientId, u32>,

    pub regions: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub pending_damage: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub pending_input_region: HashMap<ObjectId, Option<Vec<crate::wm::Rect>>>,
    pub pending_opaque_region: HashMap<ObjectId, Option<Vec<crate::wm::Rect>>>,
    pub pending_geometry: HashMap<ObjectId, crate::wm::Rect>,
    pub pending_subsurface_positions: HashMap<ObjectId, (i32, i32)>,
    pub pending_popup_positions: HashMap<ObjectId, (i32, i32)>,
    pub pending_layer_state: HashMap<ObjectId, LayerState>,
    pub surface_input_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub surface_opaque_region: HashMap<ObjectId, Vec<crate::wm::Rect>>,
    pub session_lock: Option<SessionLockState>,
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
    pub fn init(
        dh: &wayland_server::DisplayHandle,
        vkctx: Rc<VulkanContext>,
        outputs_info: Vec<crate::gpu::OutputLayoutInfo>,
        gpu_dev_t: u64,
        dmabuf_table_fd: std::os::unix::io::OwnedFd,
        mut wm: Box<dyn crate::wm::WindowManager>,
        styler: Box<dyn crate::styler::Styler>,
        config_manager: crate::config::ConfigManager,
    ) -> Self {
        use nix::unistd::{ftruncate, write};
        use xkbcommon::xkb;

        let (layout, variant, model, options, rules) = {
            let cfg = config_manager.config.lock().unwrap();
            (
                cfg.input.kb_layout.clone(),
                cfg.input.kb_variant.clone(),
                cfg.input.kb_model.clone(),
                cfg.input.kb_options.clone(),
                cfg.input.kb_rules.clone(),
            )
        };

        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            if rules.is_empty() { "evdev" } else { &rules },
            if model.is_empty() { "pc105" } else { &model },
            &layout,
            &variant,
            if options.is_empty() {
                None
            } else {
                Some(options)
            },
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

        let mode = outputs_info[0].card_info.mode;
        let card_info = outputs_info[0].card_info.clone();

        wm.set_outputs(outputs_info.clone());

        let mut wl_output_globals = Vec::new();
        for (i, _) in outputs_info.iter().enumerate() {
            let global_id = dh
                .create_global::<Composer, wayland_server::protocol::wl_output::WlOutput, MonitorData>(
                    4, MonitorData(i,
                ));
            wl_output_globals.push(global_id);
        }

        Self {
            rng: ThreadRng::default(),
            surfaces: Vec::new(),
            windows: Vec::new(),
            vkctx,
            input_focus: None,
            mode,
            card_info,
            outputs_info,

            pools: HashMap::new(),
            buffers: HashMap::new(),
            surface_buffers: HashMap::new(),
            active_dmabufs: HashMap::new(),
            surface_textures: HashMap::new(),
            cursor_manager: CursorManager::new("Adwaita", 24),
            cursor_surface: None,
            cursor_shape: None,

            window_surfaces: Vec::new(),
            wm,
            styler,
            cursor_pos: (mode.size().0 as f64 / 2.0, mode.size().1 as f64 / 2.0),

            pending_frame_callbacks: HashMap::new(),
            active_frame_callbacks: Vec::new(),

            keyboards: Vec::new(),
            keymap_fd,
            keymap_size,
            xkb_state,
            config_manager,

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
            wl_output_globals,

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
            pending_gamma: None,

            data_sources: HashMap::new(),
            data_control_sources: HashMap::new(),
            selection: None,
            data_devices: Vec::new(),

            primary_selection_sources: HashMap::new(),
            primary_selection: None,
            primary_selection_devices: Vec::new(),
            data_control_devices: Vec::new(),

            text_inputs: Vec::new(),
            input_methods: Vec::new(),
            input_popups: Vec::new(),
            input_method_grabs: Vec::new(),
            unparented_popups: HashMap::new(),

            last_enter_serial: HashMap::new(),

            regions: HashMap::new(),
            pending_damage: HashMap::new(),
            pending_input_region: HashMap::new(),
            pending_opaque_region: HashMap::new(),
            pending_geometry: HashMap::new(),
            pending_subsurface_positions: HashMap::new(),
            pending_popup_positions: HashMap::new(),
            pending_layer_state: HashMap::new(),
            surface_input_region: HashMap::new(),
            surface_opaque_region: HashMap::new(),
            session_lock: None,
        }
    }
}

impl Composer {
    pub fn update_outputs(
        &mut self,
        dh: &wayland_server::DisplayHandle,
        new_outputs: Vec<crate::gpu::OutputLayoutInfo>,
    ) {
        tracing::info!("Updating composer outputs: {} displays", new_outputs.len());

        for global_id in std::mem::take(&mut self.wl_output_globals) {
            dh.disable_global::<Composer>(global_id.clone());
            dh.remove_global::<Composer>(global_id);
        }

        self.outputs_info = new_outputs;
        self.wm.set_outputs(self.outputs_info.clone());

        for (i, _) in self.outputs_info.iter().enumerate() {
            let global_id = dh
                .create_global::<Composer, wayland_server::protocol::wl_output::WlOutput, MonitorData>(
                    4, MonitorData(i,
                ));
            self.wl_output_globals.push(global_id);
        }

        self.needs_redraw = true;
    }

    pub fn update_keymap(
        &mut self,
        layout: &str,
        variant: &str,
        model: &str,
        options: &str,
        rules: &str,
    ) {
        use nix::unistd::{ftruncate, write};
        use std::os::fd::AsFd;
        use xkbcommon::xkb;

        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            if rules.is_empty() { "evdev" } else { rules },
            if model.is_empty() { "pc105" } else { model },
            layout,
            variant,
            if options.is_empty() {
                None
            } else {
                Some(options.to_string())
            },
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap_or_else(|| {
            xkb::Keymap::new_from_names(
                &context,
                "evdev",
                "pc105",
                "us",
                "",
                None,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            )
            .expect("Failed to create fallback US keymap")
        });

        let keymap_string = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let keymap_bytes = keymap_string.as_bytes();
        let keymap_size = keymap_bytes.len() as u32 + 1;

        let keymap_fd = memfd_create(
            "pattern-keymap",
            MFdFlags::MFD_CLOEXEC | MFdFlags::MFD_ALLOW_SEALING,
        )
        .unwrap();

        ftruncate(keymap_fd.as_fd(), keymap_size as i64).unwrap();
        write(keymap_fd.as_fd(), keymap_bytes).unwrap();
        write(keymap_fd.as_fd(), &[0]).unwrap();

        self.keymap_fd = keymap_fd;
        self.keymap_size = keymap_size;
        self.xkb_state = xkb::State::new(&keymap);

        // Notify active keyboards
        for keyboard in &self.keyboards {
            keyboard.keymap(
                wayland_server::protocol::wl_keyboard::KeymapFormat::XkbV1,
                self.keymap_fd.as_fd(),
                self.keymap_size,
            );
        }
    }

    pub fn process_config_commands(&mut self, dh: &wayland_server::DisplayHandle) {
        let commands = {
            let mut queue = self.config_manager.pending_commands.lock().unwrap();
            std::mem::take(&mut *queue)
        };

        for cmd in commands {
            match cmd {
                CompositorCommand::Quit => {
                    std::process::exit(0);
                }
                CompositorCommand::Exec { full_sh_cmd } => {
                    let _ = std::process::Command::new("sh")
                        .args(["-c", &full_sh_cmd])
                        .spawn();
                }
                CompositorCommand::CloseWindow { id } => {
                    if let Some(win_id) = id {
                        if let Some(window) = self
                            .wm
                            .all_windows()
                            .into_iter()
                            .find(|w| w.surface.id().protocol_id() == win_id)
                            && let Some(toplevel) = &window.toplevel
                        {
                            toplevel.close();
                        }
                    } else {
                        self.request_closing_active_client();
                    }
                }
                CompositorCommand::FullscreenWindow { id, toggle, value } => {
                    self.request_fullscreen_window(id, toggle, value);
                }
                CompositorCommand::FocusWorkspace { id, next, previous } => {
                    let success = if next {
                        self.wm.focus_after_workspace()
                    } else if previous {
                        self.wm.focus_before_workspace()
                    } else if let Some(ws_id) = id {
                        self.wm.focus_workspace(ws_id)
                    } else {
                        false
                    };

                    if success {
                        self.needs_redraw = true;
                        self.set_input_focus(self.wm.get_focused_window(), dh);
                        self.update_pointer_focus(0);
                    }
                }
                CompositorCommand::MoveWindowToWorkspace { id, workspace } => {
                    let target_id = if let Some(win_id) = id {
                        self.wm
                            .all_windows()
                            .into_iter()
                            .find(|w| w.surface.id().protocol_id() == win_id)
                            .map(|w| w.surface.id())
                    } else {
                        self.wm.get_focused_window().map(|s| s.id())
                    };

                    if let Some(surface_id) = target_id {
                        self.wm.move_window_to_workspace(&surface_id, 0, workspace);
                        self.wm.focus_workspace(workspace);
                        self.needs_redraw = true;
                        self.set_input_focus(self.wm.get_focused_window(), dh);
                        self.update_pointer_focus(0);
                    }
                }
                CompositorCommand::DragWindow | CompositorCommand::ResizeWindow => {}
            }
        }
    }

    pub fn request_fullscreen_window(&mut self, id: Option<u32>, toggle: bool, force_val: bool) {
        let toplevel_id = if let Some(win_id) = id {
            self.wm
                .all_windows()
                .into_iter()
                .find(|w| w.surface.id().protocol_id() == win_id)
                .and_then(|w| w.toplevel.as_ref().map(|t| t.id()))
        } else {
            // Find active focused window
            if let Some(focused_surface) = &self.input_focus {
                let focused_id = focused_surface.id();
                self.wm
                    .all_windows()
                    .into_iter()
                    .find(|w| w.surface.id() == focused_id)
                    .and_then(|w| w.toplevel.as_ref().map(|t| t.id()))
            } else {
                None
            }
        };

        if let Some(toplevel_id) = toplevel_id {
            let current_fullscreen = self
                .wm
                .all_windows()
                .into_iter()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
                .map(|w| w.fullscreen)
                .unwrap_or(false);

            let target_fullscreen = if toggle {
                !current_fullscreen
            } else {
                force_val
            };

            self.serial += 1;
            self.wm.set_fullscreen(
                &toplevel_id,
                target_fullscreen,
                self.mode.size(),
                self.serial,
            );
            self.needs_redraw = true;
        }
    }

    pub fn load_cursor_shape(&mut self, shape: wp_cursor_shape_device_v1::Shape) {
        self.cursor_manager.get_or_load(shape, &self.vkctx)
    }

    pub fn is_input_popup(&self, surface_id: &ObjectId) -> bool {
        for (_, popup_surf, _) in &self.input_popups {
            if &popup_surf.id() == surface_id {
                return true;
            }
            if self.is_child_of(surface_id, &popup_surf.id()) {
                return true;
            }
        }
        false
    }

    fn is_child_of(&self, child_id: &ObjectId, parent_id: &ObjectId) -> bool {
        if let Some(sub) = self
            .subsurfaces
            .iter()
            .find(|s| &s.surface.id() == child_id)
        {
            if &sub.parent.id() == parent_id {
                return true;
            }
            return self.is_child_of(&sub.parent.id(), parent_id);
        }
        false
    }

    pub fn get_input_popup_surfaces(&self) -> Vec<(WlSurface, f64, f64)> {
        let mut surfaces = Vec::new();

        // we find the active text input for the currently focused surface
        let active_ti = if let Some(focus) = &self.input_focus {
            self.text_inputs.iter().find(|(_, _, ti_state)| {
                ti_state.active && ti_state.surface.as_ref().map(|s| s.id()) == Some(focus.id())
            })
        } else {
            None
        };

        if let Some((_, _ti_seat, ti_state)) = active_ti
            && let Some(focused_surf) = &ti_state.surface
            && let Some((px, py)) = self.get_surface_position(&focused_surf.id())
        {
            // we find any popups associated with an input method
            for (_, popup_surf, _im) in &self.input_popups {
                if popup_surf.is_alive() {
                    let x = px + ti_state.cursor_rect.0 as f64;
                    let y = py + ti_state.cursor_rect.1 as f64 + ti_state.cursor_rect.3 as f64;

                    surfaces.push((popup_surf.clone(), x, y));
                }
            }
        }
        surfaces
    }

    pub fn get_surface_position(&self, surface_id: &ObjectId) -> Option<(f64, f64)> {
        // we check if it's a toplevel or popup managed by WM
        if let Some((wm_x, wm_y)) = self.wm.get_surface_position(surface_id) {
            return Some((wm_x, wm_y));
        }

        // we check if it's a subsurface
        if let Some(sub) = self
            .subsurfaces
            .iter()
            .find(|s| s.surface.id() == *surface_id)
            && let Some((px, py)) = self.get_surface_position(&sub.parent.id())
        {
            return Some((px + sub.x as f64, py + sub.y as f64));
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

        let timeline = match WpLinuxDrmSyncobjTimelineV1::from_id(dh, timeline_id.clone()) {
            Ok(t) => t,
            Err(_) => {
                error!("Failed to create resource from id for {:?}", timeline_id);
                return None;
            }
        };

        let timeline_data = match timeline.data::<linux_drm_syncobj::Timeline>() {
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

    pub fn set_input_focus(
        &mut self,
        mut surface: Option<WlSurface>,
        dh: &wayland_server::DisplayHandle,
    ) {
        if let Some(surf) = &surface
            && let Some(win) = self
                .wm
                .all_windows()
                .iter()
                .find(|w| w.surface.id() == surf.id())
            && win.layer_surface.is_some()
            && win.keyboard_interactivity == 0
        {
            surface = None;
        }

        if self.input_focus.as_ref().map(|s| s.id()) == surface.as_ref().map(|s| s.id()) {
            return;
        }

        use wayland_protocols::xdg::shell::server::xdg_toplevel::State;

        if let Some(old_focus) = self.input_focus.take() {
            if let Some(win) = self
                .wm
                .all_windows()
                .iter()
                .find(|w| w.surface.id() == old_focus.id())
                && let (Some(toplevel), Some(xdg_surface)) = (&win.toplevel, &win.xdg_surface)
            {
                let mut states = Vec::new();
                // NOTE: Intentionally omitting State::Activated
                if win.maximized {
                    states.extend_from_slice(&(State::Maximized.0).to_ne_bytes());
                }
                if win.fullscreen {
                    states.extend_from_slice(&(State::Fullscreen.0).to_ne_bytes());
                }

                // Only enforcing size if maximized or fullscreen
                let (cfg_w, cfg_h) = if win.maximized || win.fullscreen {
                    (win.w, win.h)
                } else {
                    (0, 0)
                };

                toplevel.configure(cfg_w, cfg_h, states);
                xdg_surface.configure(self.serial);
            }

            if let Some(old_client) = old_focus.client() {
                self.serial += 1;
                for keyboard in self
                    .keyboards
                    .iter()
                    .filter(|k| k.client().map(|c| c.id()) == Some(old_client.id()))
                {
                    keyboard.leave(self.serial, &old_focus);
                }

                // Text Input Leave
                for (ti, _, state) in self.text_inputs.iter_mut() {
                    if ti.client().map(|c| c.id()) == Some(old_client.id()) {
                        ti.leave(&old_focus);
                        state.surface = None;
                    }
                }

                // we send nil selection events to the client losing focus
                self.clear_selection(&old_client);
                self.clear_primary_selection(&old_client);
            }
        }

        self.input_focus = surface;

        if let Some(new_focus) = &self.input_focus {
            if let Some(win) = self
                .wm
                .all_windows()
                .iter()
                .find(|w| w.surface.id() == new_focus.id())
                && let (Some(toplevel), Some(xdg_surface)) = (&win.toplevel, &win.xdg_surface)
            {
                let mut states = Vec::new();
                states.extend_from_slice(&(State::Activated.0).to_ne_bytes());
                if win.maximized {
                    states.extend_from_slice(&(State::Maximized.0).to_ne_bytes());
                }
                if win.fullscreen {
                    states.extend_from_slice(&(State::Fullscreen.0).to_ne_bytes());
                }

                let (cfg_w, cfg_h) = if win.maximized || win.fullscreen {
                    (win.w, win.h)
                } else {
                    (0, 0)
                };

                toplevel.configure(cfg_w, cfg_h, states);
                xdg_surface.configure(self.serial);
            }

            if let Some(client) = new_focus.client() {
                self.serial += 1;

                for (ti, _, state) in self.text_inputs.iter_mut() {
                    if ti.client().map(|c| c.id()) == Some(client.id()) {
                        ti.enter(new_focus);
                        state.surface = Some(new_focus.clone());
                    }
                }

                self.send_selection_offer(&client, dh);
                self.send_primary_selection_offer(&client, dh);

                for keyboard in self
                    .keyboards
                    .iter()
                    .filter(|k| k.client().map(|c| c.id()) == Some(client.id()))
                {
                    keyboard.enter(self.serial, new_focus, Vec::new());

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
    }

    pub fn cleanup_surface(&mut self, id: &ObjectId, dh: &wayland_server::DisplayHandle) {
        let is_layer_surface = self
            .wm
            .all_windows()
            .iter()
            .find(|w| &w.surface.id() == id)
            .map(|w| w.layer_surface.is_some())
            .unwrap_or(false);

        self.wm.unmap_window(id);
        self.wm.unmap_popup(id);
        self.xdg_to_surface.retain(|_, surface| surface.id() != *id);
        self.surface_textures.remove(id);
        if let Some(active_buf) = self.active_dmabufs.remove(id) {
            active_buf.release();
        }
        if let Some(vp_id) = self.surface_to_viewport.remove(id) {
            self.viewports.remove(&vp_id);
        }
        self.pending_syncobj_state.remove(id);
        self.syncobj_state.remove(id);
        self.surface_buffers.remove(id);
        self.explicit_sync_surfaces.remove(id);

        if self.input_focus.as_ref().map(|s| s.id()) == Some(id.clone()) {
            self.input_focus = None;
        }

        if self.pointer_focus.as_ref().map(|s| s.id()) == Some(id.clone()) {
            self.set_pointer_focus(None, 0.0, 0.0, 0);
            self.update_pointer_focus(0);
        }

        if self.input_focus.is_none() {
            let fallback_focus = self.wm.get_focused_window();
            self.set_input_focus(fallback_focus.clone(), dh);
        }

        if self.pointer_grab.as_ref().map(|s| s.id()) == Some(id.clone()) {
            let mut shifted = false;
            if let Some(sub) = self.subsurfaces.iter().find(|s| s.surface.id() == *id)
                && sub.parent.is_alive()
            {
                self.pointer_grab = Some(sub.parent.clone());
                shifted = true;
            }
            if !shifted {
                self.pointer_grab = None;
            }
        }

        self.surfaces.retain(|s| &s.id() != id);
        self.subsurfaces.retain(|s| &s.surface.id() != id);
        self.input_popups.retain(|(_, s, _)| &s.id() != id);

        if is_layer_surface {
            self.serial += 1;
            let size = self.mode.size();
            self.wm.recalculate_layer_layout(size, self.serial);
        }
    }

    pub fn broadcast_selection_offer(&self, dh: &wayland_server::DisplayHandle) {
        // we send to the focused client (standard wayland)
        if let Some(focus) = &self.input_focus
            && let Some(client) = focus.client()
        {
            self.send_selection_offer(&client, dh);
        }

        // we send to all data control devices (privileged clipboard listeners)
        // NOTE: we can't easily iterate over all clients here, but we can iterate over devices.
        // Each device knows its client.
        for device in &self.data_control_devices {
            if let Some(client) = device.client() {
                self.send_selection_offer(&client, dh);
            }
        }
    }

    pub fn broadcast_primary_selection_offer(&self, dh: &wayland_server::DisplayHandle) {
        if let Some(focus) = &self.input_focus
            && let Some(client) = focus.client()
        {
            self.send_primary_selection_offer(&client, dh);
        }

        for device in &self.data_control_devices {
            if let Some(client) = device.client() {
                self.send_primary_selection_offer(&client, dh);
            }
        }
    }

    pub fn send_selection_offer(
        &self,
        client: &wayland_server::Client,
        dh: &wayland_server::DisplayHandle,
    ) {
        if let Some(source) = &self.selection {
            for device in &self.data_devices {
                if device.client().map(|c| c.id()) == Some(client.id()) {
                    let offer = client
                        .create_resource::<WlDataOffer, ClientState, Composer>(
                            dh,
                            device.version(),
                            ClientState,
                        )
                        .expect("Failed to create WlDataOffer");
                    device.data_offer(&offer);

                    let mime_types = match source {
                        SelectionSource::Standard(_) => {
                            self.data_sources.get(&source.id()).map(|(_, m)| m)
                        }
                        SelectionSource::Primary(_) => self
                            .primary_selection_sources
                            .get(&source.id())
                            .map(|(_, m)| m),
                        SelectionSource::DataControl(_) => {
                            self.data_control_sources.get(&source.id()).map(|(_, m)| m)
                        }
                    };

                    if let Some(mime_types) = mime_types {
                        for mime in mime_types {
                            offer.offer(mime.clone());
                        }
                    }
                    device.selection(Some(&offer));
                }
            }

            for device in &self.data_control_devices {
                if device.client().map(|c| c.id()) == Some(client.id()) {
                    let offer = client
                        .create_resource::<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ClientState, Composer>(dh, device.version(), ClientState);

                    if offer.is_err() {
                        error!("Failed to create ZwlrDataControlOfferV1");
                        continue;
                    }

                    let offer = offer.unwrap();
                    device.data_offer(&offer);

                    let mime_types = match source {
                        SelectionSource::Standard(_) => {
                            self.data_sources.get(&source.id()).map(|(_, m)| m)
                        }
                        SelectionSource::Primary(_) => self
                            .primary_selection_sources
                            .get(&source.id())
                            .map(|(_, m)| m),
                        SelectionSource::DataControl(_) => {
                            self.data_control_sources.get(&source.id()).map(|(_, m)| m)
                        }
                    };

                    if let Some(mime_types) = mime_types {
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
                    let offer = client
                        .create_resource::<ZwpPrimarySelectionOfferV1, ClientState, Composer>(
                            dh,
                            device.version(),
                            ClientState,
                        );

                    if offer.is_err() {
                        error!("Failed to create ZwpPrimarySelectionOfferV1");
                        continue;
                    }

                    let offer = offer.unwrap();

                    device.data_offer(&offer);

                    let mime_types = match source {
                        SelectionSource::Standard(_) => {
                            self.data_sources.get(&source.id()).map(|(_, m)| m)
                        }
                        SelectionSource::Primary(_) => self
                            .primary_selection_sources
                            .get(&source.id())
                            .map(|(_, m)| m),
                        SelectionSource::DataControl(_) => {
                            self.data_control_sources.get(&source.id()).map(|(_, m)| m)
                        }
                    };

                    if let Some(mime_types) = mime_types {
                        for mime in mime_types {
                            offer.offer(mime.clone());
                        }
                    }
                    device.selection(Some(&offer));
                }
            }

            for device in &self.data_control_devices {
                if device.client().map(|c| c.id()) == Some(client.id()) {
                    let offer = client
                        .create_resource::<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ClientState, Composer>(dh, device.version(), ClientState);

                    if offer.is_err() {
                        error!("Failed to create ZwlrDataControlOfferV1");
                        continue;
                    }

                    let offer = offer.unwrap();

                    device.data_offer(&offer);

                    let mime_types = match source {
                        SelectionSource::Standard(_) => {
                            self.data_sources.get(&source.id()).map(|(_, m)| m)
                        }
                        SelectionSource::Primary(_) => self
                            .primary_selection_sources
                            .get(&source.id())
                            .map(|(_, m)| m),
                        SelectionSource::DataControl(_) => {
                            self.data_control_sources.get(&source.id()).map(|(_, m)| m)
                        }
                    };

                    if let Some(mime_types) = mime_types {
                        for mime in mime_types {
                            offer.offer(mime.clone());
                        }
                    }
                    device.primary_selection(Some(&offer));
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
        for device in &self.data_control_devices {
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
        for device in &self.data_control_devices {
            if device.client().map(|c| c.id()) == Some(client.id()) {
                device.primary_selection(None);
            }
        }
    }

    pub fn update_pointer_focus(&mut self, time: u32) {
        if self.wm.is_resizing() || self.wm.is_dragging() {
            self.set_pointer_focus(None, 0.0, 0.0, time);
            return;
        }

        if let Some(grabbed_surface) = self.pointer_grab.clone()
            && let Some((abs_x, abs_y)) = self.get_surface_position(&grabbed_surface.id())
        {
            let (cx, cy) = self.cursor_pos;
            let local_x = cx - abs_x;
            let local_y = cy - abs_y;
            self.set_pointer_focus(Some(grabbed_surface), local_x, local_y, time);
            return;
        }

        let (cx, cy) = self.cursor_pos;
        let mut extra_surfaces = self.get_input_popup_surfaces();

        if let Some(lock) = self.session_lock.as_ref() {
            for (_, lock_surface, out_id) in &lock.surfaces {
                if let Some(wl_out) = self.outputs.iter().find(|o| o.id() == *out_id) {
                    if let Some(monitor_data) = wl_out.data::<MonitorData>() {
                        let output_idx = monitor_data.0;
                        if let Some(out_info) = self.outputs_info.get(output_idx) {
                            extra_surfaces.push((
                                lock_surface.clone(),
                                out_info.x as f64,
                                out_info.y as f64,
                            ));
                        }
                    }
                }
            }
        }

        let hit = self.styler.hit_test(
            cx,
            cy,
            &self.subsurfaces,
            &self.surface_textures,
            &self.viewports,
            &self.surface_to_viewport,
            &self.surface_input_region,
            self.wm.as_ref(),
            &extra_surfaces,
        );
        self.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, time);
    }

    pub fn set_pointer_focus(
        &mut self,
        surface: Option<WlSurface>,
        local_x: f64,
        local_y: f64,
        time: u32,
    ) {
        if self.pointer_focus == surface {
            if let Some(surf) = &self.pointer_focus
                && let Some(client) = surf.client()
            {
                for pointer in self
                    .pointers
                    .iter()
                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                {
                    pointer.motion(time, local_x, local_y);
                    pointer.frame();
                }
            }
            return;
        }

        let old_focus = self.pointer_focus.take();
        self.pointer_focus = surface.clone();

        self.serial += 1;
        let serial = self.serial;

        let old_client_id = old_focus.as_ref().and_then(|s| s.client().map(|c| c.id()));
        let new_client_id = self
            .pointer_focus
            .as_ref()
            .and_then(|s| s.client().map(|c| c.id()));

        // Gathering all affected clients to send frames
        let mut affected_clients = HashSet::new();
        if let Some(id) = &old_client_id {
            affected_clients.insert(id.clone());
        }
        if let Some(id) = &new_client_id {
            affected_clients.insert(id.clone());
        }

        for client_id in affected_clients {
            for pointer in self
                .pointers
                .iter()
                .filter(|p| p.client().map(|c| c.id()) == Some(client_id.clone()))
            {
                // we send Leave if this client owned the old focus
                if let Some(old_surf) = &old_focus
                    && old_client_id == Some(client_id.clone())
                {
                    pointer.leave(serial, old_surf);
                }

                // we send Enter if this client owns the new focus
                if let Some(new_surf) = &self.pointer_focus
                    && new_client_id == Some(client_id.clone())
                {
                    self.last_enter_serial.insert(client_id.clone(), serial);
                    pointer.enter(serial, new_surf, local_x, local_y);
                }

                pointer.frame();
            }
        }

        // cleaning up cursor if needed
        if old_client_id != new_client_id {
            self.cursor_surface = None;
            self.cursor_shape = None;
        }
    }

    pub unsafe fn drop_semaphores(&mut self) {
        for sem in self.dead_semaphores.drain(..) {
            unsafe {
                self.vkctx.device.destroy_semaphore(sem, None);
            }
        }
    }

    pub fn request_closing_active_client(&mut self) -> bool {
        if let Some(focused_surface) = &self.input_focus {
            let focused_id = focused_surface.id();

            let active_window = self
                .wm
                .all_windows()
                .into_iter()
                .find(|w| w.surface.id() == focused_id);

            if let Some(window) = active_window
                && let Some(toplevel) = window.toplevel
            {
                toplevel.close();
                return true;
            }
        }

        false
    }
}

impl Drop for Composer {
    fn drop(&mut self) {
        unsafe {
            self.drop_semaphores();
            for (_, sem) in self.syncobj_timelines.drain() {
                self.vkctx.device.destroy_semaphore(sem, None);
            }
        }
    }
}

pub struct ClientState;
pub struct GlobalState;

pub struct MonitorData(pub usize);
pub struct SyncobjSurfaceData(pub wayland_server::backend::ObjectId);

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
        // Reap dead resources
    }
}

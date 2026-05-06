use input::event::gesture::GestureHoldEvent;
use input::event::keyboard::KeyboardEventTrait;
use input::event::pointer::PointerEventTrait;
use input::event::{EventTrait, pointer};
use input::{Libinput, LibinputInterface};
use libseat::Seat;
use nix::unistd::dup;
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::io::OwnedFd;
use std::path::Path;
use std::rc::Rc;
use tracing::{debug, warn};
use wayland_server::Resource;
use wayland_server::protocol::{wl_keyboard, wl_pointer};
use xkbcommon::xkb::Keymap;

use crate::animation::math::Vector2;
use crate::server::Composer;

mod bindings;
use bindings::{BindingAction, handle_keybinding};

pub struct SeatInterface {
    pub seat: Rc<RefCell<Seat>>,
    /// Maps the OS File Descriptor to the libseat Device ID
    pub devices: HashMap<RawFd, libseat::Device>,
}

impl LibinputInterface for SeatInterface {
    fn open_restricted(&mut self, path: &Path, _flags: i32) -> Result<OwnedFd, i32> {
        let mut seat = self.seat.borrow_mut();
        match seat.open_device(&path) {
            Ok(device) => {
                let dup_fd = dup(&device).map_err(|_| libc::EMFILE)?;
                self.devices.insert(dup_fd.as_raw_fd(), device);

                Ok(dup_fd)
            }
            Err(_) => Err(libc::EACCES),
        }
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        let dup_fd = fd.as_raw_fd();
        if let Some(device) = self.devices.remove(&dup_fd) {
            let mut seat = self.seat.borrow_mut();
            let _ = seat.close_device(device);
        }
    }
}

pub struct Input {
    pub context: Libinput,
    pub cursor: Vector2,
    pub absolute_offset: Vector2,
    pub dimension: Vector2,
    pub natural_scroll: bool,

    pub swipe_fingers: u32,
    pub swipe_dx: f64,
    pub swipe_triggered: bool,
}

pub struct Mods {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub mod4: bool,
}

impl Mods {
    #[rustfmt::skip]
    pub fn new(keymap: Keymap, depressed: u32) -> Self {
        let ctrl_index = keymap.mod_get_index(xkbcommon::xkb::MOD_NAME_CTRL); // "Control"
        let shift_index = keymap.mod_get_index(xkbcommon::xkb::MOD_NAME_SHIFT); // "Shift"
        let alt_index = keymap.mod_get_index(xkbcommon::xkb::MOD_NAME_ALT); // Usually "Mod1"
        let super_index = keymap.mod_get_index(xkbcommon::xkb::MOD_NAME_LOGO); // Usually "Mod4"

        let ctrl  = (depressed & (1 << ctrl_index))  != 0;
        let shift = (depressed & (1 << shift_index)) != 0;
        let alt   = (depressed & (1 << alt_index))   != 0;
        let mod4  = (depressed & (1 << super_index)) != 0;

        Self { alt, ctrl, shift, mod4 }
    }
}

impl Input {
    pub fn new(seat: Rc<RefCell<Seat>>, width: f64, height: f64) -> Self {
        let interface = SeatInterface {
            seat: seat.clone(),
            devices: HashMap::new(),
        };

        let mut input = Libinput::new_with_udev(interface);

        input.udev_assign_seat("seat0").unwrap();
        Self {
            context: input,
            cursor: Vector2 {
                x: width / 2.0,
                y: height / 2.0,
            },
            absolute_offset: Vector2::default(),
            dimension: Vector2 {
                x: width,
                y: height,
            },
            natural_scroll: true,

            swipe_fingers: 0,
            swipe_dx: 0.0,
            swipe_triggered: false,
        }
    }

    pub fn dispatch(&mut self, state: &mut Composer, dh: &wayland_server::DisplayHandle) -> bool {
        // Synchronize with potential external warps (e.g. wp_pointer_warp)
        if (self.cursor.x - state.cursor_pos.0).abs() > 0.1
            || (self.cursor.y - state.cursor_pos.1).abs() > 0.1
        {
            self.absolute_offset.x += state.cursor_pos.0 - self.cursor.x;
            self.absolute_offset.y += state.cursor_pos.1 - self.cursor.y;
            self.cursor.x = state.cursor_pos.0;
            self.cursor.y = state.cursor_pos.1;
        }

        self.context.dispatch().unwrap();

        let mut should_exit = false;

        for event in &mut self.context {
            match event {
                input::Event::Device(input::event::DeviceEvent::Added(evt)) => {
                    let mut device = evt.device();

                    if device.config_dwt_is_available() {
                        debug!("Disabling DWT (Palm Rejection) for device");
                        let _ = device.config_dwt_set_enabled(false);
                    }

                    if device.config_tap_finger_count() > 0 {
                        debug!("Touchpad detected. Enabling Tap-to-Click and Two-Finger Scroll");
                        let _ = device.config_tap_set_enabled(true);
                        let _ = device.config_scroll_set_method(input::ScrollMethod::TwoFinger);
                    }

                    if device
                        .config_scroll_set_natural_scroll_enabled(self.natural_scroll)
                        .is_ok()
                    {
                        debug!("Natural scroll set to: {}", self.natural_scroll);
                    }
                }
                input::Event::Device(_) => {}
                input::Event::Keyboard(input::event::keyboard::KeyboardEvent::Key(k)) => {
                    let key = k.key();
                    let time = k.time();

                    let key_state = if k.key_state() == input::event::keyboard::KeyState::Pressed {
                        wl_keyboard::KeyState::Pressed
                    } else {
                        wl_keyboard::KeyState::Released
                    };

                    let xkb_keycode = key + 8;
                    let keysym = state.xkb_state.key_get_one_sym(xkb_keycode.into());

                    let direction =
                        if key_state == wayland_server::protocol::wl_keyboard::KeyState::Pressed {
                            xkbcommon::xkb::KeyDirection::Down
                        } else {
                            xkbcommon::xkb::KeyDirection::Up
                        };

                    state.xkb_state.update_key(xkb_keycode.into(), direction);

                    let depressed = state
                        .xkb_state
                        .serialize_mods(xkbcommon::xkb::STATE_MODS_DEPRESSED);
                    let latched = state
                        .xkb_state
                        .serialize_mods(xkbcommon::xkb::STATE_MODS_LATCHED);
                    let locked = state
                        .xkb_state
                        .serialize_mods(xkbcommon::xkb::STATE_MODS_LOCKED);
                    let group = state
                        .xkb_state
                        .serialize_layout(xkbcommon::xkb::STATE_LAYOUT_EFFECTIVE);

                    match handle_keybinding(
                        state,
                        dh,
                        key,
                        key_state,
                        keysym,
                        Mods::new(state.xkb_state.get_keymap(), depressed),
                    ) {
                        BindingAction::Handled => continue,
                        BindingAction::Exit => {
                            should_exit = true;
                            continue;
                        }
                        BindingAction::None => {}
                    }

                    // Forward to IME grabs
                    let mut grabbed = false;
                    for (grab, _) in &state.input_method_grabs {
                        state.serial += 1;
                        grab.key(state.serial, time, key, key_state);
                        grab.modifiers(state.serial, depressed, latched, locked, group);
                        grabbed = true;
                    }

                    if grabbed {
                        continue;
                    }

                    if let Some(focused_surface) = &state.input_focus {
                        if let Some(client) = focused_surface.client() {
                            state.serial += 1;

                            for keyboard in state
                                .keyboards
                                .iter()
                                .filter(|kbd| kbd.client().map(|c| c.id()) == Some(client.id()))
                            {
                                keyboard.key(state.serial, time, key, key_state);
                                keyboard.modifiers(state.serial, depressed, latched, locked, group);
                            }
                        }
                    }
                }
                input::Event::Keyboard(_) => {}
                input::Event::Pointer(ev) => match ev {
                    input::event::PointerEvent::Motion(m) => {
                        let dx = m.dx();
                        let dy = m.dy();

                        let mut is_locked = false;

                        if let Some(focused) = &state.pointer_focus {
                            if let Some(client) = focused.client() {
                                if let Some(lock) = &state.pointer_lock {
                                    if lock.client().map(|c| c.id()) == Some(client.id()) {
                                        is_locked = true;
                                    }
                                }

                                for rp in &state.relative_pointers {
                                    if rp.client().map(|c| c.id()) == Some(client.id()) {
                                        let unaccel_dx = m.dx_unaccelerated();
                                        let unaccel_dy = m.dy_unaccelerated();
                                        let time_us = m.time_usec();
                                        let time_us_high = (time_us >> 32) as u32;
                                        let time_us_low = (time_us & 0xFFFFFFFF) as u32;

                                        rp.relative_motion(
                                            time_us_high,
                                            time_us_low,
                                            dx,
                                            dy,
                                            unaccel_dx,
                                            unaccel_dy,
                                        );
                                    }
                                }
                            }
                        }

                        if !is_locked {
                            self.cursor.x = (self.cursor.x + dx).clamp(0.0, self.dimension.x);
                            self.cursor.y = (self.cursor.y + dy).clamp(0.0, self.dimension.y);

                            state.cursor_pos = (self.cursor.x, self.cursor.y);
                            state.needs_redraw = true;
                            state.wm.update_drag(self.cursor.x, self.cursor.y);
                            state
                                .wm
                                .update_resize(self.cursor.x, self.cursor.y, state.serial);
                            Self::route_pointer_motion(self.cursor, state, m.time());
                        }
                    }

                    input::event::PointerEvent::MotionAbsolute(m) => {
                        let abs_x = m.absolute_x_transformed(self.dimension.x as u32);
                        let abs_y = m.absolute_y_transformed(self.dimension.y as u32);

                        let mut is_locked = false;
                        if let Some(focused) = &state.pointer_focus {
                            if let Some(client) = focused.client() {
                                if let Some(lock) = &state.pointer_lock {
                                    if lock.client().map(|c| c.id()) == Some(client.id()) {
                                        is_locked = true;
                                    }
                                }
                            }
                        }

                        if !is_locked {
                            self.cursor.x =
                                (abs_x + self.absolute_offset.x).clamp(0.0, self.dimension.x);
                            self.cursor.y =
                                (abs_y + self.absolute_offset.y).clamp(0.0, self.dimension.y);

                            state.cursor_pos = (self.cursor.x, self.cursor.y);
                            state.needs_redraw = true;
                            state.wm.update_drag(self.cursor.x, self.cursor.y);
                            state
                                .wm
                                .update_resize(self.cursor.x, self.cursor.y, state.serial);
                            Self::route_pointer_motion(self.cursor, state, m.time());
                        }
                    }

                    input::event::PointerEvent::Button(b) => {
                        use input::event::pointer::ButtonState as LibinputButtonState;
                        use wayland_server::protocol::wl_pointer::ButtonState as WlButtonState;

                        let button = b.button();
                        let state_val = if b.button_state() == LibinputButtonState::Pressed {
                            WlButtonState::Pressed
                        } else {
                            WlButtonState::Released
                        };

                        let is_left_click = button == 272;
                        let is_pressed = state_val == WlButtonState::Pressed;

                        state.needs_redraw = true;

                        let super_mod = state.xkb_state.mod_name_is_active(
                            &xkbcommon::xkb::MOD_NAME_LOGO,
                            xkbcommon::xkb::STATE_MODS_EFFECTIVE,
                        );

                        let extra_surfaces = state.get_input_popup_surfaces();
                        let hit = state.styler.hit_test(
                            self.cursor.x,
                            self.cursor.y,
                            &state.subsurfaces,
                            &state.surface_textures,
                            &state.viewports,
                            &state.surface_to_viewport,
                            &state.surface_input_region,
                            state.wm.as_ref(),
                            &extra_surfaces,
                        );
                        let hit_surface = hit.surface;

                        if is_left_click && is_pressed {
                            if let Some(surf) = &hit_surface {
                                if !state.is_input_popup(&surf.id()) {
                                    let focused_id = state.wm.focus_window(&surf.id());
                                    let target_surf = state
                                        .surfaces
                                        .iter()
                                        .find(|s| s.id() == focused_id)
                                        .cloned()
                                        .unwrap_or_else(|| surf.clone());

                                    state.set_input_focus(Some(target_surf.clone()), dh);

                                    if super_mod {
                                        state.wm.begin_drag(
                                            &target_surf.id(),
                                            self.cursor.x,
                                            self.cursor.y,
                                            state.mode.size(),
                                        );
                                    }
                                }
                                state.pointer_grab = Some(surf.clone());
                            }
                        } else if is_left_click && !is_pressed {
                            state.wm.end_drag();
                            state.wm.end_resize();
                            state.pointer_grab = None;
                        }

                        if !super_mod {
                            if let Some(focused) = &state.pointer_focus {
                                if let Some(client) = focused.client() {
                                    state.serial += 1;
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        pointer.button(state.serial, b.time(), button, state_val);
                                        pointer.frame();
                                    }
                                }
                            }
                        }
                    }

                    input::event::PointerEvent::ScrollWheel(a) => {
                        use input::event::pointer::Axis as LibinputAxis;
                        use input::event::pointer::PointerScrollEvent;
                        use wayland_server::protocol::wl_pointer::{Axis as WlAxis, AxisSource};

                        if let Some(focused) = &state.pointer_focus {
                            if let Some(client) = focused.client() {
                                for pointer in state
                                    .pointers
                                    .iter()
                                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                {
                                    pointer.axis_source(AxisSource::Wheel);

                                    if a.has_axis(LibinputAxis::Vertical) {
                                        let value = a.scroll_value(LibinputAxis::Vertical);
                                        let v120 = a.scroll_value_v120(LibinputAxis::Vertical);
                                        if value == 0.0 {
                                            pointer.axis_stop(a.time(), WlAxis::VerticalScroll);
                                        } else {
                                            pointer.axis_discrete(
                                                WlAxis::VerticalScroll,
                                                (v120 / 120.0).round() as i32,
                                            );
                                            pointer.axis(a.time(), WlAxis::VerticalScroll, value);
                                        }
                                    }
                                    if a.has_axis(LibinputAxis::Horizontal) {
                                        let value = a.scroll_value(LibinputAxis::Horizontal);
                                        let v120 = a.scroll_value_v120(LibinputAxis::Horizontal);
                                        if value == 0.0 {
                                            pointer.axis_stop(a.time(), WlAxis::HorizontalScroll);
                                        } else {
                                            pointer.axis_discrete(
                                                WlAxis::HorizontalScroll,
                                                (v120 / 120.0).round() as i32,
                                            );
                                            pointer.axis(a.time(), WlAxis::HorizontalScroll, value);
                                        }
                                    }
                                    pointer.frame();
                                }
                            }
                        }
                    }
                    input::event::PointerEvent::ScrollFinger(a) => {
                        Self::handle_scroll(
                            a,
                            wayland_server::protocol::wl_pointer::AxisSource::Finger,
                            state,
                        );
                    }
                    input::event::PointerEvent::ScrollContinuous(a) => {
                        Self::handle_scroll(
                            a,
                            wayland_server::protocol::wl_pointer::AxisSource::Continuous,
                            state,
                        );
                    }

                    _ => {}
                },
                input::Event::Touch(_) => {}
                input::Event::Tablet(_) => {}
                input::Event::TabletPad(_) => {}
                input::Event::Gesture(g) => {
                    use input::event::gesture::GestureEvent;
                    use input::event::gesture::{
                        GestureEndEvent, GestureEventCoordinates, GestureEventTrait,
                        GesturePinchEventTrait,
                    };
                    use input::event::gesture::{GesturePinchEvent, GestureSwipeEvent};

                    let serial = state.serial;

                    if let Some(focused) = &state.pointer_focus {
                        let focused_clone = focused.clone();
                        if let Some(client) = focused.client() {
                            match g {
                                GestureEvent::Swipe(GestureSwipeEvent::Begin(e)) => {
                                    self.swipe_fingers = e.finger_count() as u32;
                                    self.swipe_dx = 0.0;
                                    self.swipe_triggered = false;

                                    if self.swipe_fingers == 3 {
                                        continue;
                                    }

                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(swipes) =
                                            state.swipe_gestures.get(&pointer.id())
                                        {
                                            for swipe in swipes {
                                                swipe.begin(
                                                    serial,
                                                    e.time(),
                                                    &focused_clone,
                                                    e.finger_count() as u32,
                                                );
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Swipe(GestureSwipeEvent::Update(e)) => {
                                    if self.swipe_fingers == 3 {
                                        if !self.swipe_triggered {
                                            self.swipe_dx += e.dx();

                                            // Sensitivity threshold. Lower is more sensitive.
                                            let threshold = 80.0;

                                            // Note: You may need to invert these > < signs
                                            // depending on your natural_scroll preference
                                            if self.swipe_dx > threshold {
                                                if state.wm.focus_before_workspace() {
                                                    state.needs_redraw = true;
                                                }
                                                self.swipe_triggered = true;
                                            } else if self.swipe_dx < -threshold {
                                                if state.wm.focus_after_workspace() {
                                                    state.needs_redraw = true;
                                                }
                                                self.swipe_triggered = true;
                                            }
                                        }
                                        continue;
                                    }

                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(swipes) =
                                            state.swipe_gestures.get(&pointer.id())
                                        {
                                            for swipe in swipes {
                                                swipe.update(e.time(), e.dx(), e.dy());
                                                pointer.frame();
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Swipe(GestureSwipeEvent::End(e)) => {
                                    if self.swipe_fingers == 3 {
                                        self.swipe_fingers = 0;
                                        self.swipe_dx = 0.0;
                                        self.swipe_triggered = false;
                                        continue;
                                    }

                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(swipes) =
                                            state.swipe_gestures.get(&pointer.id())
                                        {
                                            for swipe in swipes {
                                                swipe.end(
                                                    serial,
                                                    e.time(),
                                                    if e.cancelled() { 1 } else { 0 },
                                                );
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Pinch(GesturePinchEvent::Begin(e)) => {
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(pinches) =
                                            state.pinch_gestures.get(&pointer.id())
                                        {
                                            for pinch in pinches {
                                                pinch.begin(
                                                    serial,
                                                    e.time(),
                                                    &focused_clone,
                                                    e.finger_count() as u32,
                                                );
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Pinch(GesturePinchEvent::Update(e)) => {
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(pinches) =
                                            state.pinch_gestures.get(&pointer.id())
                                        {
                                            for pinch in pinches {
                                                pinch.update(
                                                    e.time(),
                                                    e.dx(),
                                                    e.dy(),
                                                    e.scale(),
                                                    e.angle_delta(),
                                                );
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Pinch(GesturePinchEvent::End(e)) => {
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(pinches) =
                                            state.pinch_gestures.get(&pointer.id())
                                        {
                                            for pinch in pinches {
                                                pinch.end(
                                                    serial,
                                                    e.time(),
                                                    if e.cancelled() { 1 } else { 0 },
                                                );
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Hold(GestureHoldEvent::Begin(e)) => {
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(holds) = state.hold_gestures.get(&pointer.id())
                                        {
                                            for hold in holds {
                                                hold.begin(
                                                    serial,
                                                    e.time(),
                                                    &focused_clone,
                                                    e.finger_count() as u32,
                                                );
                                                pointer.frame();
                                            }
                                        }
                                    }
                                }
                                GestureEvent::Hold(GestureHoldEvent::End(e)) => {
                                    for pointer in state
                                        .pointers
                                        .iter()
                                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                    {
                                        if let Some(holds) = state.hold_gestures.get(&pointer.id())
                                        {
                                            for hold in holds {
                                                hold.end(
                                                    serial,
                                                    e.time(),
                                                    if e.cancelled() { 1 } else { 0 },
                                                );
                                                pointer.frame();
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                input::Event::Switch(_) => {}
                _ => {
                    warn!("Unhandled libinput event: {:?}", event);
                }
            }
        }

        return should_exit;
    }

    fn handle_scroll<E: pointer::PointerScrollEvent + pointer::PointerEventTrait>(
        event: E,
        source: wl_pointer::AxisSource,
        state: &mut Composer,
    ) {
        use pointer::Axis as LibinputAxis;
        use wl_pointer::Axis as WlAxis;

        state.needs_redraw = true;

        if let Some(focused) = &state.pointer_focus {
            if let Some(client) = focused.client() {
                for pointer in state
                    .pointers
                    .iter()
                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                {
                    pointer.axis_source(source);

                    if event.has_axis(LibinputAxis::Vertical) {
                        let value = event.scroll_value(LibinputAxis::Vertical);
                        if value == 0.0 {
                            pointer.axis_stop(event.time(), WlAxis::VerticalScroll);
                        } else {
                            pointer.axis(event.time(), WlAxis::VerticalScroll, value);
                        }
                    }
                    if event.has_axis(LibinputAxis::Horizontal) {
                        let value = event.scroll_value(LibinputAxis::Horizontal);
                        if value == 0.0 {
                            pointer.axis_stop(event.time(), WlAxis::HorizontalScroll);
                        } else {
                            pointer.axis(event.time(), WlAxis::HorizontalScroll, value);
                        }
                    }
                    pointer.frame();
                }
            }
        }
    }

    fn route_pointer_motion(cursor: Vector2, state: &mut Composer, time: u32) {
        if let Some(grabbed_surface) = state.pointer_grab.clone() {
            if let Some((abs_x, abs_y)) = state.get_surface_position(&grabbed_surface.id()) {
                let local_x = cursor.x - abs_x;
                let local_y = cursor.y - abs_y;
                state.set_pointer_focus(Some(grabbed_surface), local_x, local_y, time);
                return;
            }
        }

        let extra_surfaces = state.get_input_popup_surfaces();
        let hit = state.styler.hit_test(
            cursor.x,
            cursor.y,
            &state.subsurfaces,
            &state.surface_textures,
            &state.viewports,
            &state.surface_to_viewport,
            &state.surface_input_region,
            state.wm.as_ref(),
            &extra_surfaces,
        );

        state.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, time);
    }
}

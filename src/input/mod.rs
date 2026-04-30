use input::event::EventTrait;
use input::event::gesture::GestureHoldEvent;
use input::event::keyboard::KeyboardEventTrait;
use input::event::pointer::PointerEventTrait;
use input::{Libinput, LibinputInterface};
use libseat::Seat;
use nix::unistd::dup;
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::io::OwnedFd;
use std::path::Path;
use std::rc::Rc;
use tracing::debug;
use wayland_server::Resource;

use crate::server::ServerState;

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

#[derive(Clone, Copy)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl std::fmt::Display for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.x, self.y)
    }
}

pub struct Input {
    pub context: Libinput,
    pub cursor: Vec2,
    pub dimension: Vec2,
    pub natural_scroll: bool,
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
            cursor: Vec2 {
                x: width / 2.0,
                y: height / 2.0,
            },
            dimension: Vec2 {
                x: width,
                y: height,
            },
            natural_scroll: true,
        }
    }

    pub fn dispatch(
        &mut self,
        state: &mut ServerState,
        dh: &wayland_server::DisplayHandle,
    ) -> bool {
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
                        wayland_server::protocol::wl_keyboard::KeyState::Pressed
                    } else {
                        wayland_server::protocol::wl_keyboard::KeyState::Released
                    };

                    let xkb_keycode = key + 8;
                    let keysym = state.xkb_state.key_get_one_sym(xkb_keycode.into());

                    match handle_keybinding(state, key, key_state, keysym) {
                        BindingAction::Handled => continue,
                        BindingAction::Exit => {
                            should_exit = true;
                            continue;
                        }
                        BindingAction::None => {}
                    }

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
                        self.cursor.x = (self.cursor.x + m.dx()).clamp(0.0, self.dimension.x);
                        self.cursor.y = (self.cursor.y + m.dy()).clamp(0.0, self.dimension.y);

                        state.cursor_pos = (self.cursor.x, self.cursor.y);
                        state.wm.update_drag(self.cursor.x, self.cursor.y);
                        state
                            .wm
                            .update_resize(self.cursor.x, self.cursor.y, state.serial);
                        Self::route_pointer_motion(self.cursor, state, m.time());
                    }

                    input::event::PointerEvent::MotionAbsolute(m) => {
                        self.cursor.x = m.absolute_x_transformed(self.dimension.x as u32);
                        self.cursor.y = m.absolute_y_transformed(self.dimension.y as u32);

                        state.cursor_pos = (self.cursor.x, self.cursor.y);
                        state.wm.update_drag(self.cursor.x, self.cursor.y);
                        state
                            .wm
                            .update_resize(self.cursor.x, self.cursor.y, state.serial);
                        Self::route_pointer_motion(self.cursor, state, m.time());
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

                        let super_mod = state.xkb_state.mod_name_is_active(
                            &xkbcommon::xkb::MOD_NAME_LOGO,
                            xkbcommon::xkb::STATE_MODS_EFFECTIVE,
                        );

                        let hit = state.styler.hit_test(
                            self.cursor.x,
                            self.cursor.y,
                            &state.subsurfaces,
                            &state.surface_textures,
                            &state.viewports,
                            &state.surface_to_viewport,
                            &state.surface_input_region,
                            state.wm.as_ref(),
                        );
                        let hit_surface = hit.surface;

                        if is_left_click && is_pressed {
                            if let Some(surf) = &hit_surface {
                                let focused_id = state.wm.focus_window(&surf.id());
                                let target_surf = state
                                    .surfaces
                                    .iter()
                                    .find(|s| s.id() == focused_id)
                                    .cloned()
                                    .unwrap_or_else(|| surf.clone());

                                state.set_input_focus(target_surf.clone(), dh);

                                if super_mod {
                                    state.wm.begin_drag(
                                        &target_surf.id(),
                                        self.cursor.x,
                                        self.cursor.y,
                                        state.mode.size(),
                                    );
                                }
                            }
                        } else if is_left_click && !is_pressed {
                            state.wm.end_drag();
                            state.wm.end_resize();
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
                _ => todo!(),
            }
        }

        return should_exit;
    }

    fn handle_scroll<
        E: input::event::pointer::PointerScrollEvent + input::event::pointer::PointerEventTrait,
    >(
        event: E,
        source: wayland_server::protocol::wl_pointer::AxisSource,
        state: &mut ServerState,
    ) {
        use input::event::pointer::Axis as LibinputAxis;
        use wayland_server::protocol::wl_pointer::Axis as WlAxis;

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

    fn route_pointer_motion(cursor: Vec2, state: &mut ServerState, time: u32) {
        let hit = state.styler.hit_test(
            cursor.x,
            cursor.y,
            &state.subsurfaces,
            &state.surface_textures,
            &state.viewports,
            &state.surface_to_viewport,
            &state.surface_input_region,
            state.wm.as_ref(),
        );

        state.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, time);
    }
}

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
use wayland_server::Resource;

use crate::server::definition::ServerState;

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

        // OpenOptions::new()
        //     .custom_flags(flags)
        //     .read(true)
        //     .write(true)
        //     .open(path)
        //     .map(|file| file.into())
        //     .map_err(|err| err.raw_os_error().unwrap_or(libc::EACCES))
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        let dup_fd = fd.as_raw_fd();
        if let Some(device) = self.devices.remove(&dup_fd) {
            let mut seat = self.seat.borrow_mut();
            let _ = seat.close_device(device);
        }
        // drop(File::from(fd));
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
        }
    }

    pub fn dispatch(&mut self, state: &mut ServerState) -> bool {
        self.context.dispatch().unwrap();

        let mut should_exit = false;

        for event in &mut self.context {
            match event {
                input::Event::Device(_) => {}
                input::Event::Keyboard(input::event::keyboard::KeyboardEvent::Key(k)) => {
                    let key = k.key();
                    let time = k.time();

                    let key_state = if k.key_state() == input::event::keyboard::KeyState::Pressed {
                        wayland_server::protocol::wl_keyboard::KeyState::Pressed
                    } else {
                        wayland_server::protocol::wl_keyboard::KeyState::Released
                    };

                    if key == 1
                        && key_state == wayland_server::protocol::wl_keyboard::KeyState::Pressed
                    {
                        if let Some(active_window) = state.windows.last() {
                            println!("[pattern]: ESC pressed. Asking window to close...");
                            active_window.close();
                        } else {
                            println!(
                                "[pattern]: ESC pressed with no windows open. Shutting down substrate..."
                            );
                            should_exit = true;
                        }
                    }

                    if let Some(focused_surface) = &state.input_focus {
                        if let Some(client) = focused_surface.client() {
                            state.serial += 1;

                            let xkb_keycode = key + 8;
                            let direction = if key_state
                                == wayland_server::protocol::wl_keyboard::KeyState::Pressed
                            {
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
                        Self::route_pointer_motion(self.cursor, state, m.time());
                    }

                    input::event::PointerEvent::MotionAbsolute(m) => {
                        self.cursor.x = m.absolute_x_transformed(self.dimension.x as u32);
                        self.cursor.y = m.absolute_y_transformed(self.dimension.y as u32);
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

                        if let Some(focused) = &state.pointer_focus {
                            if let Some(client) = focused.client() {
                                state.serial += 1;
                                for pointer in state
                                    .pointers
                                    .iter()
                                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                                {
                                    pointer.button(state.serial, b.time(), button, state_val);
                                    pointer.frame(); // Sync
                                }
                            }
                        }
                    }

                    _ => {}
                },
                input::Event::Touch(_) => {}
                input::Event::Tablet(_) => {}
                input::Event::TabletPad(_) => {}
                input::Event::Gesture(_) => {}
                input::Event::Switch(_) => {}
                _ => todo!(),
            }
        }

        return should_exit;
    }

    fn route_pointer_motion(cursor: Vec2, state: &mut ServerState, time: u32) {
        let mut target_surface = None;
        let mut local_x = 0.0;
        let mut local_y = 0.0;

        for window_surf in &state.window_surfaces {
            if let Some(tex) = state.surface_textures.get(&window_surf.id()) {
                let win_x = state.window_loc.0;
                let win_y = state.window_loc.1;

                if cursor.x >= win_x
                    && cursor.x <= win_x + (tex.w as f64)
                    && cursor.y >= win_y
                    && cursor.y <= win_y + (tex.h as f64)
                {
                    target_surface = Some(window_surf.clone());
                    local_x = cursor.x - win_x;
                    local_y = cursor.y - win_y;
                    break; // Stop at the first window we hit
                }
            }
        }

        if target_surface.as_ref() != state.pointer_focus.as_ref() {
            if let Some(old_focus) = &state.pointer_focus {
                if let Some(client) = old_focus.client() {
                    state.serial += 1;
                    for pointer in state
                        .pointers
                        .iter()
                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                    {
                        pointer.leave(state.serial, old_focus);
                        pointer.frame();
                    }
                }
            }

            if let Some(new_focus) = &target_surface {
                if let Some(client) = new_focus.client() {
                    state.serial += 1;
                    for pointer in state
                        .pointers
                        .iter()
                        .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                    {
                        pointer.enter(state.serial, new_focus, local_x, local_y);
                        pointer.frame();
                    }
                }
            }

            state.pointer_focus = target_surface.clone();
        }

        // MOTION: Move the mouse inside the window
        if let Some(focused) = &state.pointer_focus {
            if let Some(client) = focused.client() {
                for pointer in state
                    .pointers
                    .iter()
                    .filter(|p| p.client().map(|c| c.id()) == Some(client.id()))
                {
                    pointer.motion(time, local_x, local_y);
                    pointer.frame();
                }
            }
        }
    }
}

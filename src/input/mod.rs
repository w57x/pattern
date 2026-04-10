use input::event::keyboard::KeyboardEventTrait;
use input::{Libinput, LibinputInterface};
use libseat::Seat;
use nix::unistd::dup;
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::io::OwnedFd;
use std::path::Path;
use std::rc::Rc;

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

    pub fn dispatch(&mut self) -> bool {
        self.context.dispatch().unwrap();

        let mut should_exit = false;

        for event in &mut self.context {
            match event {
                input::Event::Device(_) => {}
                input::Event::Keyboard(input::event::keyboard::KeyboardEvent::Key(k)) => {
                    if k.key() == 1 && k.key_state() == input::event::keyboard::KeyState::Pressed {
                        println!("[pattern]: ESC pressed. Shutting down substrate...");
                        should_exit = true;
                    }
                }
                input::Event::Keyboard(_) => {}
                input::Event::Pointer(ev) => match ev {
                    input::event::PointerEvent::Motion(m) => {
                        self.cursor.x = (self.cursor.x + m.dx()).clamp(0.0, self.dimension.x);
                        self.cursor.y = (self.cursor.y + m.dy()).clamp(0.0, self.dimension.y);
                    }

                    input::event::PointerEvent::MotionAbsolute(m) => {
                        self.cursor.x = m.absolute_x_transformed(self.dimension.x as u32);
                        self.cursor.y = m.absolute_y_transformed(self.dimension.y as u32);
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
}

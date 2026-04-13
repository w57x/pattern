use crate::server::ServerState;
use std::os::fd::AsFd;
use wayland_server::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::WlDataDeviceManager,
    wl_data_source::WlDataSource, wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_seat::WlSeat,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

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

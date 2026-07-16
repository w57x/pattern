use crate::server::Composer;
use wayland_protocols::wp::cursor_shape::v1::server::{
    wp_cursor_shape_device_v1::{self, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::{self, WpCursorShapeManagerV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WpCursorShapeManagerV1, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpCursorShapeManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpCursorShapeManagerV1, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WpCursorShapeManagerV1,
        request: <WpCursorShapeManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_cursor_shape_manager_v1::Request::Destroy => {
                // Nothing to do
            }
            wp_cursor_shape_manager_v1::Request::GetPointer {
                cursor_shape_device,
                pointer: _,
            } => {
                data_init.init(cursor_shape_device, ());
            }
            wp_cursor_shape_manager_v1::Request::GetTabletToolV2 {
                cursor_shape_device,
                tablet_tool: _,
            } => {
                data_init.init(cursor_shape_device, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<WpCursorShapeDeviceV1, ()> for Composer {
    fn request(
        state: &mut Self,
        client: &wayland_server::Client,
        _resource: &WpCursorShapeDeviceV1,
        request: <WpCursorShapeDeviceV1 as Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_cursor_shape_device_v1::Request::Destroy => {
                // Nothing to do
            }
            wp_cursor_shape_device_v1::Request::SetShape { serial, shape } => {
                let client_id = client.id();
                let last_serial = state.last_enter_serial.get(&client_id).copied();

                if last_serial == Some(serial)
                    && let Some(focus) = &state.pointer_focus
                    && focus.client().map(|c| c.id()) == Some(client_id)
                {
                    match shape {
                        wayland_server::WEnum::Value(s) => {
                            state.cursor_shape = Some(s);
                            // NOTE: Invalidate wl_pointer.set_cursor if any
                            state.cursor_surface = None;
                        }
                        wayland_server::WEnum::Unknown(_) => {}
                    }
                }
            }
            _ => {}
        }
    }
}

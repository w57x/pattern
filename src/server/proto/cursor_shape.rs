use wayland_protocols::wp::cursor_shape::v1::server::{
    wp_cursor_shape_device_v1::{self, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::{self, WpCursorShapeManagerV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<WpCursorShapeManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpCursorShapeManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<WpCursorShapeManagerV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WpCursorShapeManagerV1,
        request: <WpCursorShapeManagerV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wp_cursor_shape_manager_v1::Request::Destroy => {
                // Nothing to do
            }
            wp_cursor_shape_manager_v1::Request::GetPointer {
                cursor_shape_device,
                pointer: _,
            } => {
                data_init.init(cursor_shape_device, ClientState);
            }
            wp_cursor_shape_manager_v1::Request::GetTabletToolV2 {
                cursor_shape_device,
                tablet_tool: _,
            } => {
                data_init.init(cursor_shape_device, ClientState);
            }
            _ => {}
        }
    }
}

impl Dispatch<WpCursorShapeDeviceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        client: &wayland_server::Client,
        _resource: &WpCursorShapeDeviceV1,
        request: <WpCursorShapeDeviceV1 as Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
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
                        _ => {
                            state.cursor_shape = Some(shape);
                            // NOTE: Invalidate wl_pointer.set_cursor if any
                            state.cursor_surface = None;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

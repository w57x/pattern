use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::{self, WpPointerWarpV1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::ServerState;

impl GlobalDispatch<WpPointerWarpV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpPointerWarpV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpPointerWarpV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        client: &wayland_server::Client,
        _resource: &WpPointerWarpV1,
        request: <WpPointerWarpV1 as wayland_server::Resource>::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_pointer_warp_v1::Request::Destroy => {}
            wp_pointer_warp_v1::Request::WarpPointer {
                surface,
                pointer: _,
                x,
                y,
                serial,
            } => {
                let client_id = client.id();
                let last_serial = state.last_enter_serial.get(&client_id).copied();

                // Protocol: honor it if the surface has pointer focus and the serial matches
                if let Some(focus) = &state.pointer_focus {
                    if focus.id() == surface.id() && last_serial == Some(serial) {
                        if let Some(texture) = state.surface_textures.get(&surface.id()) {
                            // Protocol: reject it if the requested position is outside of the surface
                            if x >= 0. && x < texture.w as f64 && y >= 0. && y < texture.h as f64 {
                                // Translate to global coordinates
                                let (abs_x, abs_y) = state.wm.get_absolute_position(&surface.id());
                                state.cursor_pos = (abs_x + x, abs_y + y);

                                // Notify the client of the new position (sends wl_pointer.motion)
                                state.set_pointer_focus(Some(surface.clone()), x, y, 0);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::{self, WpPointerWarpV1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::{
    server::{ClientState, Composer, GlobalState},
    utils::time::gettime,
};

impl GlobalDispatch<WpPointerWarpV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WpPointerWarpV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<WpPointerWarpV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        client: &wayland_server::Client,
        _resource: &WpPointerWarpV1,
        request: <WpPointerWarpV1 as wayland_server::Resource>::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
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

                let has_focus = state
                    .pointer_focus
                    .as_ref()
                    .and_then(|f| f.client())
                    .map(|c| c.id() == client_id)
                    .unwrap_or(false);

                let has_grab = state
                    .pointer_grab
                    .as_ref()
                    .and_then(|f| f.client())
                    .map(|c| c.id() == client_id)
                    .unwrap_or(false);

                // Protocol: honor it if the client has pointer focus (or grab) and the serial matches.
                if (has_focus || has_grab)
                    && last_serial == Some(serial)
                    && let Some(texture) = state.surface_textures.get(&surface.id())
                {
                    let logical_w = (texture.w / texture.scale) as f64;
                    let logical_h = (texture.h / texture.scale) as f64;

                    // Protocol: reject it if the requested position is outside of the surface
                    if x >= 0.
                        && x < logical_w
                        && y >= 0.
                        && y < logical_h
                        && let Some((abs_x, abs_y)) = state.get_surface_position(&surface.id())
                    {
                        state.cursor_pos = (abs_x + x, abs_y + y);

                        // We must send a synthetic motion event to the client so its internal state
                        // matches the new warped position.
                        if let Some(grabbed) = state.pointer_grab.clone() {
                            if let Some((grab_x, grab_y)) =
                                state.get_surface_position(&grabbed.id())
                            {
                                state.set_pointer_focus(
                                    Some(grabbed),
                                    state.cursor_pos.0 - grab_x,
                                    state.cursor_pos.1 - grab_y,
                                    gettime(),
                                );
                            }
                        } else {
                            state.set_pointer_focus(Some(surface.clone()), x, y, gettime());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

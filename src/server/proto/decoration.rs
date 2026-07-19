use wayland_protocols::xdg::decoration::zv1::server::{
    zxdg_decoration_manager_v1::{self, ZxdgDecorationManagerV1},
    zxdg_toplevel_decoration_v1::{self, ZxdgToplevelDecorationV1},
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<ZxdgDecorationManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgDecorationManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZxdgDecorationManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZxdgDecorationManagerV1,
        request: zxdg_decoration_manager_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zxdg_decoration_manager_v1::Request::GetToplevelDecoration { id, toplevel } => {
                let decoration: ZxdgToplevelDecorationV1 = data_init.init(id, ClientState);
                state
                    .decoration_to_toplevel
                    .insert(decoration.id(), toplevel.id());

                if state.styler.supports_ssd() {
                    decoration.configure(zxdg_toplevel_decoration_v1::Mode::ServerSide);
                } else {
                    decoration.configure(zxdg_toplevel_decoration_v1::Mode::ClientSide);
                }
            }
            zxdg_decoration_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZxdgToplevelDecorationV1,
        request: zxdg_toplevel_decoration_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zxdg_toplevel_decoration_v1::Request::SetMode { mode } => {
                let mode_enum = mode;
                let final_mode = if state.styler.supports_ssd() {
                    mode_enum
                } else {
                    zxdg_toplevel_decoration_v1::Mode::ClientSide
                };

                if let Some(toplevel_id) = state.decoration_to_toplevel.get(&resource.id()).cloned()
                {
                    state.wm.set_window_ssd(
                        &toplevel_id,
                        final_mode == zxdg_toplevel_decoration_v1::Mode::ServerSide,
                    );
                }
                resource.configure(final_mode);
            }
            zxdg_toplevel_decoration_v1::Request::UnsetMode => {
                let final_mode = if state.styler.supports_ssd() {
                    zxdg_toplevel_decoration_v1::Mode::ServerSide
                } else {
                    zxdg_toplevel_decoration_v1::Mode::ClientSide
                };

                if let Some(toplevel_id) = state.decoration_to_toplevel.get(&resource.id()).cloned()
                {
                    state.wm.set_window_ssd(
                        &toplevel_id,
                        final_mode == zxdg_toplevel_decoration_v1::Mode::ServerSide,
                    );
                }
                resource.configure(final_mode);
            }
            zxdg_toplevel_decoration_v1::Request::Destroy => {
                state.decoration_to_toplevel.remove(&resource.id());
            }
            _ => {}
        }
    }
}

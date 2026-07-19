use rand::Rng;
use wayland_protocols::xdg::activation::v1::server::{xdg_activation_token_v1, xdg_activation_v1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<xdg_activation_v1::XdgActivationV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<xdg_activation_v1::XdgActivationV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &xdg_activation_v1::XdgActivationV1,
        request: xdg_activation_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            xdg_activation_v1::Request::GetActivationToken { id } => {
                data_init.init(id, ClientState);
            }
            xdg_activation_v1::Request::Activate { token, surface }
                if state.activation_tokens.remove(&token) =>
            {
                state.wm.focus_window(&surface.id());
            }
            xdg_activation_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &xdg_activation_token_v1::XdgActivationTokenV1,
        request: xdg_activation_token_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            xdg_activation_token_v1::Request::SetSerial { serial: _, seat: _ } => {}
            xdg_activation_token_v1::Request::SetAppId { app_id: _ } => {}
            xdg_activation_token_v1::Request::SetSurface { surface: _ } => {}
            xdg_activation_token_v1::Request::Commit => {
                let token = format!("pattern-token-{}-{:x}", state.serial, state.rng.next_u64());
                state.serial += 1;
                state.activation_tokens.insert(token.clone());
                resource.done(token);
            }
            xdg_activation_token_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

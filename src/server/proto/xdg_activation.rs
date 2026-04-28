use crate::server::ServerState;
use wayland_protocols::xdg::activation::v1::server::{xdg_activation_token_v1, xdg_activation_v1};
use wayland_server::{Dispatch, GlobalDispatch};

impl GlobalDispatch<xdg_activation_v1::XdgActivationV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<xdg_activation_v1::XdgActivationV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &xdg_activation_v1::XdgActivationV1,
        request: xdg_activation_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_activation_v1::Request::GetActivationToken { id } => {
                _data_init.init(id, ());
            }
            xdg_activation_v1::Request::Activate {
                token: _,
                surface: _,
            } => {}
            xdg_activation_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &xdg_activation_token_v1::XdgActivationTokenV1,
        request: xdg_activation_token_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_activation_token_v1::Request::SetSerial { serial: _, seat: _ } => {}
            xdg_activation_token_v1::Request::SetAppId { app_id: _ } => {}
            xdg_activation_token_v1::Request::SetSurface { surface: _ } => {}
            xdg_activation_token_v1::Request::Commit => {}
            xdg_activation_token_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

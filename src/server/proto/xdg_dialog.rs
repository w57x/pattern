use crate::server::ServerState;
use wayland_protocols::xdg::dialog::v1::server::{xdg_dialog_v1, xdg_wm_dialog_v1};
use wayland_server::{Dispatch, GlobalDispatch};

impl GlobalDispatch<xdg_wm_dialog_v1::XdgWmDialogV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<xdg_wm_dialog_v1::XdgWmDialogV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<xdg_wm_dialog_v1::XdgWmDialogV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &xdg_wm_dialog_v1::XdgWmDialogV1,
        request: xdg_wm_dialog_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_wm_dialog_v1::Request::GetXdgDialog { id, toplevel: _ } => {
                _data_init.init(id, ());
            }
            xdg_wm_dialog_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<xdg_dialog_v1::XdgDialogV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &xdg_dialog_v1::XdgDialogV1,
        request: xdg_dialog_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_dialog_v1::Request::SetModal => {}
            xdg_dialog_v1::Request::UnsetModal => {}
            xdg_dialog_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

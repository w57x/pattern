use wayland_protocols::xdg::dialog::v1::server::{xdg_dialog_v1, xdg_wm_dialog_v1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<xdg_wm_dialog_v1::XdgWmDialogV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<xdg_wm_dialog_v1::XdgWmDialogV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<xdg_wm_dialog_v1::XdgWmDialogV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &xdg_wm_dialog_v1::XdgWmDialogV1,
        request: xdg_wm_dialog_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            xdg_wm_dialog_v1::Request::GetXdgDialog { id, toplevel } => {
                let dialog = data_init.init(id, ClientState);
                _state.dialog_to_toplevel.insert(dialog.id(), toplevel.id());
            }
            xdg_wm_dialog_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<xdg_dialog_v1::XdgDialogV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &xdg_dialog_v1::XdgDialogV1,
        request: xdg_dialog_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let toplevel_id = if let Some(id) = state.dialog_to_toplevel.get(&resource.id()) {
            id.clone()
        } else {
            return;
        };

        match request {
            xdg_dialog_v1::Request::SetModal => {
                state.wm.set_modal(&toplevel_id, true);
            }
            xdg_dialog_v1::Request::UnsetModal => {
                state.wm.set_modal(&toplevel_id, false);
            }
            xdg_dialog_v1::Request::Destroy => {
                state.wm.set_modal(&toplevel_id, false);
                state.dialog_to_toplevel.remove(&resource.id());
            }
            _ => {}
        }
    }
}

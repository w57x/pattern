use wayland_protocols::ext::workspace::v1::server::{
    ext_workspace_group_handle_v1, ext_workspace_handle_v1, ext_workspace_manager_v1,
};
use wayland_server::{Dispatch, GlobalDispatch};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ext_workspace_manager_v1::ExtWorkspaceManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_manager_v1::ExtWorkspaceManagerV1,
        request: ext_workspace_manager_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            ext_workspace_manager_v1::Request::Commit => {}
            ext_workspace_manager_v1::Request::Stop => {}
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
        request: ext_workspace_group_handle_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            ext_workspace_group_handle_v1::Request::CreateWorkspace { workspace: _ } => {}
            ext_workspace_group_handle_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_handle_v1::ExtWorkspaceHandleV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_handle_v1::ExtWorkspaceHandleV1,
        request: ext_workspace_handle_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            ext_workspace_handle_v1::Request::Activate => {}
            ext_workspace_handle_v1::Request::Deactivate => {}
            ext_workspace_handle_v1::Request::Assign { workspace_group: _ } => {}
            ext_workspace_handle_v1::Request::Remove => {}
            ext_workspace_handle_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

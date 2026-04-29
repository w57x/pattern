use crate::server::ServerState;
use wayland_protocols::ext::workspace::v1::server::{
    ext_workspace_group_handle_v1, ext_workspace_handle_v1, ext_workspace_manager_v1,
};
use wayland_server::{Dispatch, GlobalDispatch};

impl GlobalDispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ext_workspace_manager_v1::ExtWorkspaceManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_manager_v1::ExtWorkspaceManagerV1,
        request: ext_workspace_manager_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            ext_workspace_manager_v1::Request::Commit => {}
            ext_workspace_manager_v1::Request::Stop => {}
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
        request: ext_workspace_group_handle_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            ext_workspace_group_handle_v1::Request::CreateWorkspace { workspace: _ } => {}
            ext_workspace_group_handle_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ext_workspace_handle_v1::ExtWorkspaceHandleV1,
        request: ext_workspace_handle_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
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

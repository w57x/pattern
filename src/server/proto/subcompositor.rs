use crate::server::{ServerState, SubsurfaceData};
use wayland_server::protocol::{wl_subcompositor::WlSubcompositor, wl_subsurface::WlSubsurface};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlSubcompositor, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlSubcompositor>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlSubcompositor, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlSubcompositor,
        request: wayland_server::protocol::wl_subcompositor::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_subcompositor::Request::GetSubsurface {
                id,
                surface,
                parent,
            } => {
                let subsurface = data_init.init(id, ());
                state.subsurfaces.push(SubsurfaceData {
                    id: subsurface.id(),
                    surface: surface.clone(),
                    parent: parent.clone(),
                    x: 0,
                    y: 0,
                });
            }
            wayland_server::protocol::wl_subcompositor::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<WlSubsurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &WlSubsurface,
        request: wayland_server::protocol::wl_subsurface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_server::protocol::wl_subsurface::Request::SetPosition { x, y } => {
                if let Some(sub) = state.subsurfaces.iter_mut().find(|s| s.id == resource.id()) {
                    sub.x = x;
                    sub.y = y;
                }
            }
            wayland_server::protocol::wl_subsurface::Request::PlaceAbove { sibling } => {
                let idx = state.subsurfaces.iter().position(|s| s.id == resource.id());
                let sibling_idx = state
                    .subsurfaces
                    .iter()
                    .position(|s| s.surface.id() == sibling.id());
                if let (Some(idx), Some(s_idx)) = (idx, sibling_idx) {
                    let sub = state.subsurfaces.remove(idx);
                    let new_idx = if idx < s_idx { s_idx } else { s_idx + 1 };
                    state.subsurfaces.insert(new_idx, sub);
                }
            }
            wayland_server::protocol::wl_subsurface::Request::PlaceBelow { sibling } => {
                let idx = state.subsurfaces.iter().position(|s| s.id == resource.id());
                let sibling_idx = state
                    .subsurfaces
                    .iter()
                    .position(|s| s.surface.id() == sibling.id());
                if let (Some(idx), Some(s_idx)) = (idx, sibling_idx) {
                    let sub = state.subsurfaces.remove(idx);
                    let new_idx = if idx < s_idx { s_idx - 1 } else { s_idx };
                    state.subsurfaces.insert(new_idx, sub);
                }
            }
            wayland_server::protocol::wl_subsurface::Request::SetSync => {}
            wayland_server::protocol::wl_subsurface::Request::SetDesync => {}
            wayland_server::protocol::wl_subsurface::Request::Destroy => {
                state.subsurfaces.retain(|s| s.id != resource.id());
            }
            _ => {}
        }
    }
}

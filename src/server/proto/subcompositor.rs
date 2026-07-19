use crate::server::{ClientState, Composer, GlobalState, SubsurfaceData};
use wayland_server::protocol::{wl_subcompositor::WlSubcompositor, wl_subsurface::WlSubsurface};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlSubcompositor, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlSubcompositor>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<WlSubcompositor, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WlSubcompositor,
        request: wayland_server::protocol::wl_subcompositor::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_server::protocol::wl_subcompositor::Request::GetSubsurface {
                id,
                surface,
                parent,
            } => {
                let subsurface = data_init.init(id, ClientState);
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

impl Dispatch<WlSubsurface, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &WlSubsurface,
        request: wayland_server::protocol::wl_subsurface::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_server::protocol::wl_subsurface::Request::SetPosition { x, y } => {
                state
                    .pending_subsurface_positions
                    .insert(resource.id(), (x, y));
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

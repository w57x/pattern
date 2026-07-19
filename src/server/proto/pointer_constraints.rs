use wayland_protocols::wp::pointer_constraints::zv1::server::{
    zwp_confined_pointer_v1::ZwpConfinedPointerV1, zwp_locked_pointer_v1::ZwpLockedPointerV1,
    zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<ZwpPointerConstraintsV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpPointerConstraintsV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwpPointerConstraintsV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpPointerConstraintsV1,
        request: wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::Request::Destroy => {}
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::Request::LockPointer {
                id,
                surface: _,
                pointer: _,
                region: _,
                lifetime: _,
            } => {
                let lock = data_init.init(id, ClientState);
                state.pointer_lock = Some(lock.clone());
                lock.locked();
            }
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::Request::ConfinePointer {
                id,
                surface: _,
                pointer: _,
                region: _,
                lifetime: _,
            } => {
                let confine = data_init.init(id, ClientState);
                state.pointer_confine = Some(confine.clone());
                confine.confined();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLockedPointerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpLockedPointerV1,
        request: wayland_protocols::wp::pointer_constraints::zv1::server::zwp_locked_pointer_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_locked_pointer_v1::Request::Destroy
                if state.pointer_lock.as_ref().map(|l| l.id()) == Some(resource.id()) => {
                    state.pointer_lock = None;
                }
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_locked_pointer_v1::Request::SetCursorPositionHint { surface_x, surface_y } => {
                state.cursor_pos_hint = Some((surface_x, surface_y));
            }
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_locked_pointer_v1::Request::SetRegion { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpConfinedPointerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpConfinedPointerV1,
        request: wayland_protocols::wp::pointer_constraints::zv1::server::zwp_confined_pointer_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_confined_pointer_v1::Request::Destroy
                if state.pointer_confine.as_ref().map(|c| c.id()) == Some(resource.id()) => {
                    state.pointer_confine = None;
                }
            wayland_protocols::wp::pointer_constraints::zv1::server::zwp_confined_pointer_v1::Request::SetRegion { .. } => {}
            _ => {}
        }
    }
}

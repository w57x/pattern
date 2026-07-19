use wayland_protocols::wp::relative_pointer::zv1::server::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::ZwpRelativePointerV1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<ZwpRelativePointerManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpRelativePointerManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwpRelativePointerManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwpRelativePointerManagerV1,
        request: wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request::Destroy => {}
            wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request::GetRelativePointer {
                id,
                pointer: _,
            } => {
                let rp = data_init.init(id, ClientState);
                state.relative_pointers.push(rp);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpRelativePointerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwpRelativePointerV1,
        request: wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_v1::Request::Destroy = request {
            state.relative_pointers.retain(|p| p.id() != resource.id());
        }
    }
}

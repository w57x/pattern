use crate::server::Composer;
use wayland_protocols::wp::relative_pointer::zv1::server::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::ZwpRelativePointerV1,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<ZwpRelativePointerManagerV1, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwpRelativePointerManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpRelativePointerManagerV1, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZwpRelativePointerManagerV1,
        request: wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request::Destroy => {}
            wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::Request::GetRelativePointer {
                id,
                pointer: _,
            } => {
                let rp = data_init.init(id, ());
                state.relative_pointers.push(rp);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpRelativePointerV1, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &ZwpRelativePointerV1,
        request: wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_v1::Request::Destroy = request {
            state.relative_pointers.retain(|p| p.id() != resource.id());
        }
    }
}

use crate::server::Composer;
use wayland_protocols::wp::fifo::v1::server::{wp_fifo_manager_v1, wp_fifo_v1};
use wayland_server::{Dispatch, DisplayHandle, GlobalDispatch};

impl GlobalDispatch<wp_fifo_manager_v1::WpFifoManagerV1, ()> for Composer {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<wp_fifo_manager_v1::WpFifoManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<wp_fifo_manager_v1::WpFifoManagerV1, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &wp_fifo_manager_v1::WpFifoManagerV1,
        request: wp_fifo_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_fifo_manager_v1::Request::GetFifo { id, surface: _ } => {
                data_init.init(id, ());
            }
            wp_fifo_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<wp_fifo_v1::WpFifoV1, ()> for Composer {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &wp_fifo_v1::WpFifoV1,
        request: wp_fifo_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wp_fifo_v1::Request::SetBarrier => {}
            wp_fifo_v1::Request::WaitBarrier => {}
            wp_fifo_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

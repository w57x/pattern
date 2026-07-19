use wayland_protocols::wp::fifo::v1::server::{wp_fifo_manager_v1, wp_fifo_v1};
use wayland_server::{Dispatch, DisplayHandle, GlobalDispatch};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<wp_fifo_manager_v1::WpFifoManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<wp_fifo_manager_v1::WpFifoManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<wp_fifo_manager_v1::WpFifoManagerV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &wp_fifo_manager_v1::WpFifoManagerV1,
        request: wp_fifo_manager_v1::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wp_fifo_manager_v1::Request::GetFifo { id, surface: _ } => {
                data_init.init(id, ClientState);
            }
            wp_fifo_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<wp_fifo_v1::WpFifoV1, Composer> for ClientState {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &wp_fifo_v1::WpFifoV1,
        request: wp_fifo_v1::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            wp_fifo_v1::Request::SetBarrier => {}
            wp_fifo_v1::Request::WaitBarrier => {}
            wp_fifo_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

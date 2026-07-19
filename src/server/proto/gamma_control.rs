use crate::{
    gpu::DrmColorLut,
    server::{ClientState, Composer, GlobalState},
};
use wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use wayland_server::{Dispatch, GlobalDispatch};

impl GlobalDispatch<ZwlrGammaControlManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZwlrGammaControlManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZwlrGammaControlManagerV1,
        request: zwlr_gamma_control_manager_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_gamma_control_manager_v1::Request::GetGammaControl {
                id,
                output: _output,
            } => {
                let gamma_control = data_init.init(id, ClientState);
                let size = state.card_info.gamma_size;
                gamma_control.gamma_size(size);
            }
            zwlr_gamma_control_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &ZwlrGammaControlV1,
        request: zwlr_gamma_control_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_gamma_control_v1::Request::SetGamma { fd } => {
                let size = state.card_info.gamma_size as usize;
                if size == 0 {
                    resource.failed();
                    return;
                }

                let mmap = unsafe {
                    match memmap2::MmapOptions::new().len(3 * size * 2).map(&fd) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::error!("Failed to mmap gamma control fd: {:?}", e);
                            resource.failed();
                            return;
                        }
                    }
                };

                let mut lut = Vec::with_capacity(size);
                for i in 0..size {
                    let r = u16::from_ne_bytes([mmap[i * 2], mmap[i * 2 + 1]]);
                    let g = u16::from_ne_bytes([mmap[(size + i) * 2], mmap[(size + i) * 2 + 1]]);
                    let b = u16::from_ne_bytes([
                        mmap[(2 * size + i) * 2],
                        mmap[(2 * size + i) * 2 + 1],
                    ]);
                    lut.push(DrmColorLut {
                        red: r,
                        green: g,
                        blue: b,
                        reserved: 0,
                    });
                }

                state.pending_gamma = Some(lut);
                state.needs_redraw = true;
            }
            zwlr_gamma_control_v1::Request::Destroy => {
                state.pending_gamma = Some(Vec::new());
                state.needs_redraw = true;
            }
            _ => {}
        }
    }

    fn destroyed(
        &self,
        state: &mut Composer,
        _client: wayland_server::backend::ClientId,
        _resource: &ZwlrGammaControlV1,
    ) {
        state.pending_gamma = Some(Vec::new());
        state.needs_redraw = true;
    }
}

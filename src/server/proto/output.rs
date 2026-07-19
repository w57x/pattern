use wayland_protocols::xdg::xdg_output::zv1::server::{
    zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
    zxdg_output_v1::{self, ZxdgOutputV1},
};
use wayland_server::protocol::wl_output::WlOutput;
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState, MonitorData};

impl GlobalDispatch<WlOutput, Composer> for MonitorData {
    fn bind(
        &self,
        state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlOutput>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let output_idx = self.0;

        let output = data_init.init(resource, MonitorData(output_idx));

        if let Some(out) = state.outputs_info.get(output_idx) {
            output.geometry(
                out.x,
                out.y,
                out.width,
                out.height,
                wayland_server::protocol::wl_output::Subpixel::Unknown,
                "Pattern".to_string(),
                out.card_info.name.clone(),
                wayland_server::protocol::wl_output::Transform::Normal,
            );
            output.mode(
                wayland_server::protocol::wl_output::Mode::Current,
                out.width,
                out.height,
                (out.card_info.mode.vrefresh() * 1000) as i32,
            );

            if output.version() >= 2 {
                output.scale(1);
            }
            if output.version() >= 4 {
                output.name(out.card_info.name.clone());
                output.description(out.card_info.description.clone());
            }
            if output.version() >= 2 {
                output.done();
            }
        }

        for surface in &state.surfaces {
            if surface.client().map(|c| c.id()) == Some(_client.id()) {
                surface.enter(&output);
            }
        }

        state.outputs.push(output);
    }
}

impl Dispatch<WlOutput, Composer> for MonitorData {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &WlOutput,
        _request: wayland_server::protocol::wl_output::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
    }
}

impl GlobalDispatch<ZxdgOutputManagerV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgOutputManagerV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<ZxdgOutputManagerV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zxdg_output_manager_v1::Request::GetXdgOutput { id, output } => {
                let xdg_output = data_init.init(id, output.clone());
                let output_idx = *output.data::<MonitorData>().map(|m| &m.0).unwrap_or(&0);
                if let Some(out) = state.outputs_info.get(output_idx) {
                    xdg_output.logical_position(out.x, out.y);
                    xdg_output.logical_size(out.width, out.height);
                    if xdg_output.version() >= 2 {
                        xdg_output.name(out.card_info.name.clone());
                        xdg_output.description(out.card_info.description.clone());
                    }
                    xdg_output.done();
                }
            }
            zxdg_output_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, Composer> for WlOutput {
    fn request(
        &self,
        _state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &ZxdgOutputV1,
        request: zxdg_output_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        if let zxdg_output_v1::Request::Destroy = request {}
    }
}

impl GlobalDispatch<ZxdgOutputV1, Composer> for WlOutput {
    fn bind(
        &self,
        state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgOutputV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let output = self;
        let xdg_output = data_init.init(resource, output.clone());
        let output_idx = *output.data::<MonitorData>().map(|m| &m.0).unwrap_or(&0);
        if let Some(out) = state.outputs_info.get(output_idx) {
            xdg_output.logical_position(out.x, out.y);
            xdg_output.logical_size(out.width, out.height);
            if xdg_output.version() >= 2 {
                xdg_output.name(out.card_info.name.clone());
                xdg_output.description(out.card_info.description.clone());
            }
            xdg_output.done();
        }
    }
}

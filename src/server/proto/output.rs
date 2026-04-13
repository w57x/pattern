use crate::server::ServerState;
use wayland_protocols::xdg::xdg_output::zv1::server::{
    zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
    zxdg_output_v1::{self, ZxdgOutputV1},
};
use wayland_server::protocol::wl_output::WlOutput;
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<WlOutput, ()> for ServerState {
    fn bind(
        state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<WlOutput>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let output = data_init.init(resource, ());

        output.geometry(
            0,
            0,
            state.mode.size().0 as i32,
            state.mode.size().1 as i32,
            wayland_server::protocol::wl_output::Subpixel::Unknown,
            "Pattern".to_string(),
            "Virtual Display".to_string(),
            wayland_server::protocol::wl_output::Transform::Normal,
        );
        output.mode(
            wayland_server::protocol::wl_output::Mode::Current,
            state.mode.size().0 as i32,
            state.mode.size().1 as i32,
            (state.mode.vrefresh() * 1000) as i32,
        );

        if output.version() >= 2 {
            output.scale(1);
        }
        if output.version() >= 4 {
            output.name("WL-1".to_string());
            output.description("Pattern Display".to_string());
        }
        if output.version() >= 2 {
            output.done();
        }

        for surface in &state.surfaces {
            if surface.client().map(|c| c.id()) == Some(_client.id()) {
                surface.enter(&output);
            }
        }

        state.outputs.push(output);
    }
}

impl Dispatch<WlOutput, ()> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &WlOutput,
        _request: wayland_server::protocol::wl_output::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<ZxdgOutputManagerV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgOutputManagerV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zxdg_output_manager_v1::Request::GetXdgOutput { id, output } => {
                let xdg_output = data_init.init(id, output.clone());
                let (w, h) = state.mode.size();
                xdg_output.logical_position(0, 0);
                xdg_output.logical_size(w as i32, h as i32);
                if xdg_output.version() >= 2 {
                    xdg_output.name("PatternDisplay".to_string());
                    xdg_output.description("Pattern Virtual Output".to_string());
                }
                xdg_output.done();
            }
            zxdg_output_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, WlOutput> for ServerState {
    fn request(
        _state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &ZxdgOutputV1,
        request: zxdg_output_v1::Request,
        _output: &WlOutput,
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        if let zxdg_output_v1::Request::Destroy = request {}
    }
}

impl GlobalDispatch<ZxdgOutputV1, WlOutput> for ServerState {
    fn bind(
        state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<ZxdgOutputV1>,
        output: &WlOutput,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let xdg_output = data_init.init(resource, output.clone());
        let (w, h) = state.mode.size();
        xdg_output.logical_position(0, 0);
        xdg_output.logical_size(w as i32, h as i32);
        if xdg_output.version() >= 2 {
            xdg_output.name("PatternDisplay".to_string());
            xdg_output.description("Pattern Virtual Output".to_string());
        }
        xdg_output.done();
    }
}

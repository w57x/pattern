use crate::server::ServerState;
use wayland_protocols_wlr::layer_shell::v1::server::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        request: zwlr_layer_shell_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            zwlr_layer_shell_v1::Request::GetLayerSurface {
                id,
                surface,
                output: _,
                layer,
                namespace,
            } => {
                println!(
                    "[pattern]: Client requested a layer surface (namespace: {})",
                    namespace
                );
                let layer_surface = data_init.init(id, ());

                let layer_val = match layer {
                    wayland_server::WEnum::Value(zwlr_layer_shell_v1::Layer::Background) => 0,
                    wayland_server::WEnum::Value(zwlr_layer_shell_v1::Layer::Bottom) => 1,
                    wayland_server::WEnum::Value(zwlr_layer_shell_v1::Layer::Top) => 2,
                    wayland_server::WEnum::Value(zwlr_layer_shell_v1::Layer::Overlay) => 3,
                    _ => 2,
                };

                state
                    .wm
                    .map_layer_surface(surface.clone(), layer_surface.clone(), layer_val);
                state.xdg_to_surface.insert(layer_surface.id(), surface);
            }
            zwlr_layer_shell_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        request: zwlr_layer_surface_v1::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let surface_id = if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
            surface.id()
        } else {
            return;
        };

        match request {
            zwlr_layer_surface_v1::Request::SetSize { width, height } => {
                state.wm.set_layer_surface_size(&surface_id, width, height);
            }
            zwlr_layer_surface_v1::Request::SetAnchor { anchor } => {
                let anchor_val = match anchor {
                    wayland_server::WEnum::Value(a) => a.bits(),
                    _ => 0,
                };
                state.wm.set_layer_surface_anchor(&surface_id, anchor_val);
            }
            zwlr_layer_surface_v1::Request::SetExclusiveZone { zone } => {
                state.wm.set_layer_surface_zone(&surface_id, zone);
                state.wm.recalculate_layer_layout(state.mode.size());
            }
            zwlr_layer_surface_v1::Request::SetMargin {
                top,
                right,
                bottom,
                left,
            } => {
                state
                    .wm
                    .set_layer_surface_margin(&surface_id, top, right, bottom, left);
            }
            zwlr_layer_surface_v1::Request::SetKeyboardInteractivity {
                keyboard_interactivity,
            } => {
                let inter_val = match keyboard_interactivity {
                    wayland_server::WEnum::Value(i) => i as u32,
                    _ => 0,
                };
                state
                    .wm
                    .set_layer_keyboard_interactivity(&surface_id, inter_val);
            }
            zwlr_layer_surface_v1::Request::GetPopup { popup: _ } => {}
            zwlr_layer_surface_v1::Request::AckConfigure { serial: _ } => {}
            zwlr_layer_surface_v1::Request::Destroy => {
                state.wm.unmap_window(&surface_id);
                state.xdg_to_surface.remove(&resource.id());
                state.wm.recalculate_layer_layout(state.mode.size());
            }
            _ => {}
        }
    }
}

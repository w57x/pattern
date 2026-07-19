use crate::server::{LayerState, proto::xdg_shell::compute_popup_position};
use wayland_protocols_wlr::layer_shell::v1::server::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

use crate::server::{ClientState, Composer, GlobalState};

impl GlobalDispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, Composer> for GlobalState {
    fn bind(
        &self,
        _state: &mut Composer,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        data_init.init(resource, ClientState);
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        _resource: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        request: zwlr_layer_shell_v1::Request,
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        match request {
            zwlr_layer_shell_v1::Request::GetLayerSurface {
                id,
                surface,
                output,
                layer,
                namespace,
            } => {
                let layer_surface = data_init.init(id, ClientState);

                let layer_val = match layer {
                    zwlr_layer_shell_v1::Layer::Background => 0,
                    zwlr_layer_shell_v1::Layer::Bottom => 1,
                    zwlr_layer_shell_v1::Layer::Top => 2,
                    zwlr_layer_shell_v1::Layer::Overlay => 3,
                    _ => 2,
                };

                let output_id = output.as_ref().and_then(|o| o.data::<usize>().copied());

                state.wm.map_layer_surface(
                    surface.clone(),
                    layer_surface.clone(),
                    layer_val,
                    namespace,
                    output_id,
                );
                state.xdg_to_surface.insert(layer_surface.id(), surface);
            }
            zwlr_layer_shell_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, Composer> for ClientState {
    fn request(
        &self,
        state: &mut Composer,
        _client: &wayland_server::Client,
        resource: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        request: zwlr_layer_surface_v1::Request,
        dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Composer>,
    ) {
        let surface_id = if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
            surface.id()
        } else {
            return;
        };

        match request {
            zwlr_layer_surface_v1::Request::SetSize { width, height } => {
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.size = Some((width, height));
            }
            zwlr_layer_surface_v1::Request::SetAnchor { anchor } => {
                let anchor_val = anchor.bits();
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.anchor = Some(anchor_val);
            }
            zwlr_layer_surface_v1::Request::SetExclusiveZone { zone } => {
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.zone = Some(zone);
            }
            zwlr_layer_surface_v1::Request::SetMargin {
                top,
                right,
                bottom,
                left,
            } => {
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.margin = Some((top, right, bottom, left));
            }
            zwlr_layer_surface_v1::Request::SetKeyboardInteractivity {
                keyboard_interactivity,
            } => {
                let inter_val = u32::from(keyboard_interactivity);
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.interactivity = Some(inter_val);
            }
            zwlr_layer_surface_v1::Request::GetPopup { popup } => {
                if let Some((surf, xdg_surf, xdg_pop, pos_data)) =
                    state.unparented_popups.remove(&popup.id())
                {
                    let (x, y) = compute_popup_position(state, &surface_id, &pos_data);

                    state.wm.map_popup(crate::wm::PopupState {
                        surface: surf,
                        xdg_surface: xdg_surf.clone(),
                        xdg_popup: xdg_pop.clone(),
                        parent_surface_id: surface_id.clone(),
                        x,
                        y,
                        geometry: crate::wm::Rect::default(),
                    });

                    state.serial += 1;
                    xdg_pop.configure(x, y, pos_data.size.0, pos_data.size.1);
                    // Important: also configure the underlying xdg_surface
                    xdg_surf.configure(state.serial);
                }
            }
            zwlr_layer_surface_v1::Request::AckConfigure { serial: _ } => {}
            zwlr_layer_surface_v1::Request::Destroy => {
                state.cleanup_surface(&surface_id, dhandle);
                state.serial += 1;
                state
                    .wm
                    .recalculate_layer_layout(state.mode.size(), state.serial);
            }
            _ => {}
        }
    }
}

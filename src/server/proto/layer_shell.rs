use crate::server::Composer;
use crate::server::proto::xdg_shell::compute_popup_position;
use wayland_protocols_wlr::layer_shell::v1::server::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use wayland_server::{Dispatch, GlobalDispatch, Resource, WEnum};

impl GlobalDispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for Composer {
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

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for Composer {
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
                let layer_surface = data_init.init(id, ());

                let layer_val = match layer {
                    WEnum::Value(zwlr_layer_shell_v1::Layer::Background) => 0,
                    WEnum::Value(zwlr_layer_shell_v1::Layer::Bottom) => 1,
                    WEnum::Value(zwlr_layer_shell_v1::Layer::Top) => 2,
                    WEnum::Value(zwlr_layer_shell_v1::Layer::Overlay) => 3,
                    _ => 2,
                };

                state.wm.map_layer_surface(
                    surface.clone(),
                    layer_surface.clone(),
                    layer_val,
                    namespace,
                );
                state.xdg_to_surface.insert(layer_surface.id(), surface);
            }
            zwlr_layer_shell_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        request: zwlr_layer_surface_v1::Request,
        _data: &(),
        dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
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
                    .or_insert(crate::server::LayerState {
                        size: None,
                        anchor: None,
                        zone: None,
                        margin: None,
                        interactivity: None,
                    });
                state.size = Some((width, height));
            }
            zwlr_layer_surface_v1::Request::SetAnchor { anchor } => {
                let anchor_val = match anchor {
                    wayland_server::WEnum::Value(a) => a.bits(),
                    _ => 0,
                };
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(crate::server::LayerState {
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
                    .or_insert(crate::server::LayerState {
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
                    .or_insert(crate::server::LayerState {
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
                let inter_val = match keyboard_interactivity {
                    wayland_server::WEnum::Value(i) => i as u32,
                    _ => 0,
                };
                let state = state
                    .pending_layer_state
                    .entry(surface_id.clone())
                    .or_insert(crate::server::LayerState {
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
                state.wm.recalculate_layer_layout(state.mode.size());
            }
            _ => {}
        }
    }
}

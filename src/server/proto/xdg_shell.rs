use crate::server::ServerState;
use wayland_protocols::xdg::shell::server::{
    xdg_popup, xdg_popup::XdgPopup, xdg_positioner, xdg_positioner::XdgPositioner, xdg_surface,
    xdg_surface::XdgSurface, xdg_toplevel, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<XdgWmBase, ()> for ServerState {
    fn bind(
        _state: &mut Self,
        _handle: &wayland_server::DisplayHandle,
        _client: &wayland_server::Client,
        resource: wayland_server::New<XdgWmBase>,
        _global_data: &(),
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<XdgWmBase, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        _resource: &XdgWmBase,
        request: wayland_protocols::xdg::shell::server::xdg_wm_base::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::GetXdgSurface {
                id,
                surface,
            } => {
                println!("[pattern]: Client upgraded a WlSurface to an XdgSurface");
                let xdg_surface = data_init.init(id, ());
                state.xdg_to_surface.insert(xdg_surface.id(), surface);
            }
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::CreatePositioner {
                id,
            } => {
                data_init.init(id, ());
            }
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<XdgSurface, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgSurface,
        request: xdg_surface::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_surface::Request::GetToplevel { id } => {
                let toplevel = data_init.init(id, ());
                let surface = state.xdg_to_surface.get(&resource.id()).cloned();

                if let Some(surface) = surface {
                    state.wm.map_window(surface.clone());
                    state
                        .wm
                        .assign_toplevel(&surface.id(), toplevel.clone(), resource.clone());
                    state.wm.focus_window(&surface.id());
                    state.set_input_focus(surface.clone());

                    let (cx, cy) = state.cursor_pos;
                    let hit = state.styler.hit_test(
                        cx,
                        cy,
                        &state.wm.get_render_list(),
                        &state.wm.get_popups(),
                        &state.subsurfaces,
                        &state.surface_textures,
                        &state.viewports,
                        &state.surface_to_viewport,
                        state.wm.as_ref(),
                    );
                    state.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, 0);
                }

                let state_val = xdg_toplevel::State::Activated as u32;
                let states_bytes = state_val.to_ne_bytes().to_vec();
                toplevel.configure(800, 600, states_bytes);
                resource.configure(1);
            }
            xdg_surface::Request::GetPopup {
                id,
                parent,
                positioner,
            } => {
                let popup = data_init.init(id, ());
                let positioner_data = state
                    .pending_positioners
                    .get(&positioner.id())
                    .cloned()
                    .unwrap_or_default();

                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    if let Some(parent_xdg) = parent {
                        if let Some(parent_surface) = state.xdg_to_surface.get(&parent_xdg.id()) {
                            let parent_geom = state
                                .wm
                                .get_render_list()
                                .iter()
                                .find(|w| w.surface.id() == parent_surface.id())
                                .map(|w| w.geometry)
                                .unwrap_or_default();

                            let mut x = parent_geom.x
                                + positioner_data.anchor_rect.0
                                + positioner_data.offset.0;
                            let mut y = parent_geom.y
                                + positioner_data.anchor_rect.1
                                + positioner_data.offset.1;

                            use xdg_positioner::{Anchor, Gravity};
                            match positioner_data.anchor {
                                wayland_server::WEnum::Value(Anchor::TopRight)
                                | wayland_server::WEnum::Value(Anchor::Right)
                                | wayland_server::WEnum::Value(Anchor::BottomRight) => {
                                    x += positioner_data.anchor_rect.2;
                                }
                                wayland_server::WEnum::Value(Anchor::Top)
                                | wayland_server::WEnum::Value(Anchor::Bottom) => {
                                    x += positioner_data.anchor_rect.2 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.anchor {
                                wayland_server::WEnum::Value(Anchor::BottomLeft)
                                | wayland_server::WEnum::Value(Anchor::Bottom)
                                | wayland_server::WEnum::Value(Anchor::BottomRight) => {
                                    y += positioner_data.anchor_rect.3;
                                }
                                wayland_server::WEnum::Value(Anchor::Left)
                                | wayland_server::WEnum::Value(Anchor::Right) => {
                                    y += positioner_data.anchor_rect.3 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.gravity {
                                wayland_server::WEnum::Value(Gravity::TopRight)
                                | wayland_server::WEnum::Value(Gravity::Right)
                                | wayland_server::WEnum::Value(Gravity::BottomRight) => {
                                    x -= positioner_data.size.0;
                                }
                                wayland_server::WEnum::Value(Gravity::Top)
                                | wayland_server::WEnum::Value(Gravity::Bottom) => {
                                    x -= positioner_data.size.0 / 2;
                                }
                                _ => {}
                            }

                            match positioner_data.gravity {
                                wayland_server::WEnum::Value(Gravity::BottomLeft)
                                | wayland_server::WEnum::Value(Gravity::Bottom)
                                | wayland_server::WEnum::Value(Gravity::BottomRight) => {
                                    y -= positioner_data.size.1;
                                }
                                wayland_server::WEnum::Value(Gravity::Left)
                                | wayland_server::WEnum::Value(Gravity::Right) => {
                                    y -= positioner_data.size.1 / 2;
                                }
                                _ => {}
                            }

                            let (sw, sh) = state.mode.size();
                            let (px, py) = state.wm.get_absolute_position(&parent_surface.id());
                            let abs_x = px + x as f64;
                            let abs_y = py + y as f64;

                            if abs_x + positioner_data.size.0 as f64 > sw as f64 {
                                x -= (abs_x + positioner_data.size.0 as f64 - sw as f64) as i32;
                            }
                            if abs_x < 0.0 {
                                x -= abs_x as i32;
                            }
                            if abs_y + positioner_data.size.1 as f64 > sh as f64 {
                                y -= (abs_y + positioner_data.size.1 as f64 - sh as f64) as i32;
                            }
                            if abs_y < 0.0 {
                                y -= abs_y as i32;
                            }

                            state.wm.map_popup(crate::wm::PopupState {
                                surface: surface.clone(),
                                xdg_surface: resource.clone(),
                                xdg_popup: popup.clone(),
                                parent_surface_id: parent_surface.id(),
                                x,
                                y,
                            });

                            state.serial += 1;
                            popup.configure(x, y, positioner_data.size.0, positioner_data.size.1);
                            resource.configure(state.serial);
                        }
                    }
                }
            }
            xdg_surface::Request::SetWindowGeometry {
                x,
                y,
                width,
                height,
            } => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.set_window_geometry(
                        &surface.id(),
                        crate::wm::Rect {
                            x,
                            y,
                            w: width,
                            h: height,
                        },
                    );
                }
            }
            xdg_surface::Request::AckConfigure { .. } => {}
            xdg_surface::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_window(&surface.id());
                    state.wm.unmap_popup(&surface.id());
                }
                state.xdg_to_surface.remove(&resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgPopup, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgPopup,
        request: xdg_popup::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_popup::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_popup(&surface.id());
                }
            }
            xdg_popup::Request::Grab { .. } => {}
            xdg_popup::Request::Reposition { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<XdgToplevel, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgToplevel,
        request: xdg_toplevel::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        match request {
            xdg_toplevel::Request::SetTitle { title } => {
                println!("[wm]: Window title set to: {}", title);
                state.wm.set_window_title(&resource.id(), title);
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
                println!("[wm]: App ID set to: {}", app_id);
                state.wm.set_window_app_id(&resource.id(), app_id);
            }
            xdg_toplevel::Request::SetParent { parent } => {
                let parent_id = parent.map(|p| p.id());
                state.wm.set_window_parent(&resource.id(), parent_id);
            }
            xdg_toplevel::Request::Move { seat: _, serial: _ } => {
                state.wm.begin_interactive_move(
                    &resource.id(),
                    state.cursor_pos.0,
                    state.cursor_pos.1,
                );
            }
            xdg_toplevel::Request::Resize {
                seat: _,
                serial: _,
                edges,
            } => {
                state.wm.begin_interactive_resize(
                    &resource.id(),
                    edges.into(),
                    state.cursor_pos.0,
                    state.cursor_pos.1,
                );
            }
            xdg_toplevel::Request::Destroy => {
                if let Some(surface) = state.xdg_to_surface.get(&resource.id()) {
                    state.wm.unmap_window(&surface.id());
                }
                state.windows.retain(|w| w.id() != resource.id());
                return;
            }
            _ => {}
        }

        if !state.windows.iter().any(|w| w.id() == resource.id()) {
            state.windows.push(resource.clone());
        }
    }
}

impl Dispatch<XdgPositioner, ()> for ServerState {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgPositioner,
        request: xdg_positioner::Request,
        _data: &(),
        _dhandle: &wayland_server::DisplayHandle,
        _data_init: &mut wayland_server::DataInit<'_, Self>,
    ) {
        let entry = state.pending_positioners.entry(resource.id()).or_default();
        match request {
            xdg_positioner::Request::SetSize { width, height } => {
                entry.size = (width, height);
            }
            xdg_positioner::Request::SetAnchorRect {
                x,
                y,
                width,
                height,
            } => {
                entry.anchor_rect = (x, y, width, height);
            }
            xdg_positioner::Request::SetAnchor { anchor } => {
                entry.anchor = anchor;
            }
            xdg_positioner::Request::SetGravity { gravity } => {
                entry.gravity = gravity;
            }
            xdg_positioner::Request::SetConstraintAdjustment {
                constraint_adjustment,
            } => {
                entry.constraint_adjustment = constraint_adjustment;
            }
            xdg_positioner::Request::SetOffset { x, y } => {
                entry.offset = (x, y);
            }
            xdg_positioner::Request::Destroy => {
                state.pending_positioners.remove(&resource.id());
            }
            _ => {}
        }
    }
}

use crate::server::Composer;
use wayland_protocols::xdg::shell::server::{
    xdg_popup, xdg_popup::XdgPopup, xdg_positioner, xdg_positioner::XdgPositioner, xdg_surface,
    xdg_surface::XdgSurface, xdg_toplevel, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase,
};
use wayland_server::{Dispatch, GlobalDispatch, Resource};

impl GlobalDispatch<XdgWmBase, ()> for Composer {
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

impl Dispatch<XdgWmBase, ()> for Composer {
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
                let xdg_surface = data_init.init(id, ());
                state.xdg_to_surface.insert(xdg_surface.id(), surface);
            }
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::CreatePositioner {
                id,
            } => {
                data_init.init(id, ());
            }
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::Pong { serial: _ } => {}
            wayland_protocols::xdg::shell::server::xdg_wm_base::Request::Destroy => {}
            _ => {}
        }
    }
}

pub fn compute_popup_position(
    state: &Composer,
    parent_surface_id: &wayland_server::backend::ObjectId,
    positioner_data: &crate::server::PositionerData,
) -> (i32, i32) {
    use xdg_positioner::{Anchor, Gravity};

    let mut x = positioner_data.anchor_rect.0 + positioner_data.offset.0;
    let mut y = positioner_data.anchor_rect.1 + positioner_data.offset.1;

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
        wayland_server::WEnum::Value(Gravity::TopLeft)
        | wayland_server::WEnum::Value(Gravity::Left)
        | wayland_server::WEnum::Value(Gravity::BottomLeft) => {
            x -= positioner_data.size.0;
        }
        wayland_server::WEnum::Value(Gravity::None)
        | wayland_server::WEnum::Value(Gravity::Top)
        | wayland_server::WEnum::Value(Gravity::Bottom) => {
            x -= positioner_data.size.0 / 2;
        }
        _ => {}
    }

    match positioner_data.gravity {
        wayland_server::WEnum::Value(Gravity::TopLeft)
        | wayland_server::WEnum::Value(Gravity::Top)
        | wayland_server::WEnum::Value(Gravity::TopRight) => {
            y -= positioner_data.size.1;
        }
        wayland_server::WEnum::Value(Gravity::None)
        | wayland_server::WEnum::Value(Gravity::Left)
        | wayland_server::WEnum::Value(Gravity::Right) => {
            y -= positioner_data.size.1 / 2;
        }
        _ => {}
    }

    let (sw, sh) = state.mode.size();
    let (px, py) = state.wm.get_absolute_position(parent_surface_id);
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

    (x, y)
}

impl Dispatch<XdgSurface, ()> for Composer {
    fn request(
        state: &mut Self,
        _client: &wayland_server::Client,
        resource: &XdgSurface,
        request: xdg_surface::Request,
        _data: &(),
        dhandle: &wayland_server::DisplayHandle,
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
                    state.set_input_focus(surface.clone(), dhandle);

                    let (cx, cy) = state.cursor_pos;
                    let extra_surfaces = state.get_input_popup_surfaces();
                    let hit = state.styler.hit_test(
                        cx,
                        cy,
                        &state.subsurfaces,
                        &state.surface_textures,
                        &state.viewports,
                        &state.surface_to_viewport,
                        &state.surface_input_region,
                        state.wm.as_ref(),
                        &extra_surfaces,
                    );
                    state.set_pointer_focus(hit.surface, hit.local_x, hit.local_y, 0);
                }

                let state_val = xdg_toplevel::State::Activated as u32;
                let states_bytes = state_val.to_ne_bytes().to_vec();

                state.serial += 1;
                toplevel.configure(0, 0, states_bytes);
                resource.configure(state.serial);
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
                            let (x, y) = compute_popup_position(
                                state,
                                &parent_surface.id(),
                                &positioner_data,
                            );

                            state.wm.map_popup(crate::wm::PopupState {
                                surface: surface.clone(),
                                xdg_surface: resource.clone(),
                                xdg_popup: popup.clone(),
                                parent_surface_id: parent_surface.id(),
                                x,
                                y,
                                geometry: crate::wm::Rect::default(),
                            });

                            state.serial += 1;
                            popup.configure(x, y, positioner_data.size.0, positioner_data.size.1);
                            resource.configure(state.serial);
                        }
                    } else {
                        // we store it for layer-shell to claim
                        state.unparented_popups.insert(
                            popup.id(),
                            (
                                surface.clone(),
                                resource.clone(),
                                popup.clone(),
                                positioner_data,
                            ),
                        );
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
                    state.pending_geometry.insert(
                        surface.id(),
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

impl Dispatch<XdgPopup, ()> for Composer {
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
            xdg_popup::Request::Reposition { positioner, token } => {
                let positioner_data = state
                    .pending_positioners
                    .get(&positioner.id())
                    .cloned()
                    .unwrap_or_default();

                let mut popup_to_update = None;
                for popup in state.wm.get_popups() {
                    if popup.xdg_popup.id() == resource.id() {
                        let (x, y) = compute_popup_position(
                            state,
                            &popup.parent_surface_id,
                            &positioner_data,
                        );
                        popup_to_update = Some((popup.surface.id(), x, y));
                        break;
                    }
                }

                if let Some((id, x, y)) = popup_to_update {
                    state.pending_popup_positions.insert(id.clone(), (x, y));
                    resource.repositioned(token);
                    resource.configure(x, y, positioner_data.size.0, positioner_data.size.1);
                    // Also need to configure the underlying xdg_surface
                    if let Some(popup) = state.wm.get_popups().iter().find(|p| p.surface.id() == id)
                    {
                        state.serial += 1;
                        popup.xdg_surface.configure(state.serial);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgToplevel, ()> for Composer {
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
                state.wm.set_window_title(&resource.id(), title);
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
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
                    state.mode.size(),
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
                    state.mode.size(),
                );
            }
            xdg_toplevel::Request::SetMaximized => {
                state
                    .wm
                    .set_maximized(&resource.id(), true, state.mode.size());
            }
            xdg_toplevel::Request::UnsetMaximized => {
                state
                    .wm
                    .set_maximized(&resource.id(), false, state.mode.size());
            }
            xdg_toplevel::Request::SetFullscreen { output: _ } => {
                state
                    .wm
                    .set_fullscreen(&resource.id(), true, state.mode.size());
            }
            xdg_toplevel::Request::UnsetFullscreen => {
                state
                    .wm
                    .set_fullscreen(&resource.id(), false, state.mode.size());
            }
            xdg_toplevel::Request::SetMinimized => {
                state.wm.set_minimized(&resource.id());
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

impl Dispatch<XdgPositioner, ()> for Composer {
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

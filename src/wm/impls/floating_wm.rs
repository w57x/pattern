use crate::wm::*;
use tracing::debug;
use wayland_server::Resource;

pub struct Wm {
    pub outputs: Vec<OutputState>,
    pub popups: Vec<PopupState>,
    pub drag_state: Option<(ObjectId, f64, f64)>,
    pub resize_state: Option<(ObjectId, u32, f64, f64, f64, f64, i32, i32, i32, i32)>,
}

impl Wm {
    pub fn new() -> Self {
        Self {
            outputs: vec![OutputState::new(0)],
            popups: Vec::new(),
            drag_state: None,
            resize_state: None,
        }
    }

    fn find_window_mut(&mut self, id: &ObjectId) -> Option<&mut WindowState> {
        for output in &mut self.outputs {
            if let Some(w) = output
                .background
                .windows
                .iter_mut()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
            if let Some(w) = output
                .bottom
                .windows
                .iter_mut()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
            for ws in output.workspaces.flatten_mut() {
                if let Some(w) = ws.windows.iter_mut().find(|w| &w.surface.id() == id) {
                    return Some(w);
                }
            }
            if let Some(w) = output
                .top
                .windows
                .iter_mut()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
            if let Some(w) = output
                .overlay
                .windows
                .iter_mut()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
        }
        None
    }

    fn find_window(&self, id: &ObjectId) -> Option<&WindowState> {
        for output in &self.outputs {
            if let Some(w) = output
                .background
                .windows
                .iter()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
            if let Some(w) = output.bottom.windows.iter().find(|w| &w.surface.id() == id) {
                return Some(w);
            }
            for ws in output.workspaces.flatten() {
                if let Some(w) = ws.windows.iter().find(|w| &w.surface.id() == id) {
                    return Some(w);
                }
            }
            if let Some(w) = output.top.windows.iter().find(|w| &w.surface.id() == id) {
                return Some(w);
            }
            if let Some(w) = output
                .overlay
                .windows
                .iter()
                .find(|w| &w.surface.id() == id)
            {
                return Some(w);
            }
        }
        None
    }

    fn find_window_by_toplevel_mut(&mut self, id: &ObjectId) -> Option<&mut WindowState> {
        for output in &mut self.outputs {
            if let Some(w) = output
                .background
                .windows
                .iter_mut()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(id.clone()))
            {
                return Some(w);
            }
            if let Some(w) = output
                .bottom
                .windows
                .iter_mut()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(id.clone()))
            {
                return Some(w);
            }
            for ws in output.workspaces.flatten_mut() {
                if let Some(w) = ws
                    .windows
                    .iter_mut()
                    .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(id.clone()))
                {
                    return Some(w);
                }
            }
            if let Some(w) = output
                .top
                .windows
                .iter_mut()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(id.clone()))
            {
                return Some(w);
            }
            if let Some(w) = output
                .overlay
                .windows
                .iter_mut()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(id.clone()))
            {
                return Some(w);
            }
        }
        None
    }
}

impl WindowManager for Wm {
    fn all_windows(&self) -> Vec<WindowState> {
        let mut list = Vec::new();
        for output in &self.outputs {
            list.extend(output.background.windows.clone());
            list.extend(output.bottom.windows.clone());
            for ws in output.workspaces.flatten() {
                list.extend(ws.windows.clone());
            }
            list.extend(output.top.windows.clone());
            list.extend(output.overlay.windows.clone());
        }

        let drag_id = self.drag_state.as_ref().map(|(id, _, _)| id.clone());
        let resize_id = self.resize_state.as_ref().map(|(id, ..)| id.clone());

        for win in &mut list {
            if Some(win.surface.id()) == drag_id || Some(win.surface.id()) == resize_id {
                win.is_interacting = true;
            }
        }

        list
    }

    fn map_window(&mut self, surface: WlSurface) {
        if self.outputs.is_empty() {
            return;
        }
        let out = &mut self.outputs[0];
        let ws_slot = out.workspaces.get_mut(out.active_workspace).unwrap();
        let ws = ws_slot.unwrap_mut();
        let offset = (ws.windows.len() * 30) as f64;

        ws.windows.push(WindowState {
            surface,
            xdg_surface: None,
            toplevel: None,
            layer_surface: None,
            parent_id: None,
            x: 100.0 + offset,
            y: 100.0 + offset,
            w: 800,
            h: 600,
            geometry: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            title: None,
            app_id: None,
            ssd: false,
            maximized: false,
            fullscreen: false,
            minimized: false,
            modal: false,
            saved_geometry: None,
            layer: 0,
            anchor: 0,
            exclusive_zone: 0,
            margin: (0, 0, 0, 0),
            keyboard_interactivity: 0,
            is_interacting: false,
        });
    }

    fn unmap_window(&mut self, surface_id: &ObjectId) {
        for out in &mut self.outputs {
            out.background
                .windows
                .retain(|w| &w.surface.id() != surface_id);
            out.bottom.windows.retain(|w| &w.surface.id() != surface_id);
            for ws in out.workspaces.flatten_mut() {
                ws.windows.retain(|w| &w.surface.id() != surface_id);
            }
            out.top.windows.retain(|w| &w.surface.id() != surface_id);
            out.overlay
                .windows
                .retain(|w| &w.surface.id() != surface_id);
        }
        self.popups.retain(|p| &p.surface.id() != surface_id);
        if let Some((drag_id, _, _)) = &self.drag_state {
            if drag_id == surface_id {
                self.drag_state = None;
            }
        }
        if let Some((resize_id, ..)) = &self.resize_state {
            if resize_id == surface_id {
                self.resize_state = None;
            }
        }
    }

    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId {
        let mut target_id = surface_id.clone();
        while let Some(popup) = self.popups.iter().find(|p| p.surface.id() == target_id) {
            target_id = popup.parent_surface_id.clone();
        }

        let has_transient_child = self
            .all_windows()
            .iter()
            .any(|w| w.parent_id.as_ref() == Some(&target_id));
        if has_transient_child {
            if let Some(child_id) = self.all_windows().iter().find_map(|w| {
                if w.parent_id.as_ref() == Some(&target_id) {
                    Some(w.surface.id())
                } else {
                    None
                }
            }) {
                return self.focus_window(&child_id);
            }
        }

        for out in &mut self.outputs {
            if let Some(idx) = out
                .background
                .windows
                .iter()
                .position(|w| w.surface.id() == target_id)
            {
                let w = out.background.windows.remove(idx);
                out.background.windows.push(w);
                return target_id;
            }

            if let Some(idx) = out
                .bottom
                .windows
                .iter()
                .position(|w| w.surface.id() == target_id)
            {
                let w = out.bottom.windows.remove(idx);
                out.bottom.windows.push(w);
                return target_id;
            }

            for ws in out.workspaces.flatten_mut() {
                if let Some(idx) = ws.windows.iter().position(|w| w.surface.id() == target_id) {
                    let w = ws.windows.remove(idx);
                    ws.windows.push(w);
                    return target_id;
                }
            }

            if let Some(idx) = out
                .top
                .windows
                .iter()
                .position(|w| w.surface.id() == target_id)
            {
                let w = out.top.windows.remove(idx);
                out.top.windows.push(w);
                return target_id;
            }

            if let Some(idx) = out
                .overlay
                .windows
                .iter()
                .position(|w| w.surface.id() == target_id)
            {
                let w = out.overlay.windows.remove(idx);
                out.overlay.windows.push(w);
                return target_id;
            }
        }
        target_id
    }

    fn assign_toplevel(
        &mut self,
        surface_id: &ObjectId,
        toplevel: XdgToplevel,
        xdg_surface: XdgSurface,
    ) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.toplevel = Some(toplevel);
            window.xdg_surface = Some(xdg_surface);
        }
    }

    fn map_popup(&mut self, popup: PopupState) {
        self.popups.push(popup);
    }

    fn unmap_popup(&mut self, popup_surface_id: &ObjectId) {
        self.popups.retain(|p| &p.surface.id() != popup_surface_id);
    }

    fn update_popup_position(&mut self, popup_surface_id: &ObjectId, x: i32, y: i32) {
        if let Some(popup) = self
            .popups
            .iter_mut()
            .find(|p| &p.surface.id() == popup_surface_id)
        {
            popup.x = x;
            popup.y = y;
        }
    }

    fn set_window_title(&mut self, toplevel_id: &ObjectId, title: String) {
        if let Some(window) = self.find_window_by_toplevel_mut(toplevel_id) {
            window.title = Some(title);
        }
    }

    fn set_window_app_id(&mut self, toplevel_id: &ObjectId, app_id: String) {
        if let Some(window) = self.find_window_by_toplevel_mut(toplevel_id) {
            window.app_id = Some(app_id);
        }
    }

    fn set_window_parent(&mut self, toplevel_id: &ObjectId, parent_id: Option<ObjectId>) {
        let mut parent_info = None;
        if let Some(pid) = &parent_id {
            if let Some(w) = self
                .all_windows()
                .iter()
                .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(pid.clone()))
            {
                parent_info = Some(((w.x, w.y), (w.w, w.h), w.geometry));
            }
        }

        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        if let Some(cid) = child_id {
            if let Some(child) = self.find_window_mut(&cid) {
                child.parent_id = parent_id;
                if let Some((pos, _dim, geom)) = parent_info {
                    let lpx = pos.0 + geom.x as f64 + (geom.w as f64) / 2.0;
                    let lpy = pos.1 + geom.y as f64 + (geom.h as f64) / 2.0;
                    let child_geom = child.geometry;
                    child.x = lpx - (child_geom.x as f64) - (child_geom.w as f64) / 2.0;
                    child.y = lpy - (child_geom.y as f64) - (child_geom.h as f64) / 2.0;
                }
            }
        }
    }

    fn set_window_ssd(&mut self, toplevel_id: &ObjectId, enabled: bool) {
        if let Some(window) = self.find_window_by_toplevel_mut(toplevel_id) {
            window.ssd = enabled;
        }
    }

    fn set_window_geometry(&mut self, surface_id: &ObjectId, geometry: Rect) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.x += (window.geometry.x - geometry.x) as f64;
            window.y += (window.geometry.y - geometry.y) as f64;
            window.geometry = geometry;
        }

        if let Some(popup) = self
            .popups
            .iter_mut()
            .find(|p| &p.surface.id() == surface_id)
        {
            popup.geometry = geometry;
        }
    }

    fn set_maximized(&mut self, toplevel_id: &ObjectId, maximized: bool, screen_size: (u16, u16)) {
        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        if let Some(id) = child_id {
            if let Some(window) = self.find_window_mut(&id) {
                if window.maximized == maximized {
                    return;
                }

                let (target_w, target_h) = if maximized {
                    (screen_size.0 as i32, screen_size.1 as i32)
                } else {
                    if let Some((_, _, w, h)) = window.saved_geometry {
                        (w, h)
                    } else {
                        (window.w, window.h)
                    }
                };

                if maximized && !window.maximized {
                    window.saved_geometry = Some((window.x, window.y, window.w, window.h));
                    window.x = -window.geometry.x as f64;
                    window.y = -window.geometry.y as f64;
                    window.maximized = true;
                } else if !maximized && window.maximized {
                    if let Some((x, y, w, h)) = window.saved_geometry.take() {
                        window.x = x;
                        window.y = y;
                        window.w = w;
                        window.h = h;
                    }
                    window.maximized = false;
                }

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    let mut states = Vec::new();
                    states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                    if window.maximized {
                        states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                    }
                    if window.fullscreen {
                        states.extend_from_slice(&(State::Fullscreen as u32).to_ne_bytes());
                    }
                    toplevel.configure(target_w, target_h, states);
                    xdg_surface.configure(0);
                }
            }
        }
    }

    fn set_fullscreen(
        &mut self,
        toplevel_id: &ObjectId,
        fullscreen: bool,
        screen_size: (u16, u16),
    ) {
        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        if let Some(id) = child_id {
            if let Some(window) = self.find_window_mut(&id) {
                if window.fullscreen == fullscreen {
                    return;
                }

                let (target_w, target_h) = if fullscreen {
                    (screen_size.0 as i32, screen_size.1 as i32)
                } else {
                    if let Some((_, _, w, h)) = window.saved_geometry {
                        (w, h)
                    } else {
                        (window.w, window.h)
                    }
                };

                if fullscreen && !window.fullscreen {
                    window.saved_geometry = Some((window.x, window.y, window.w, window.h));
                    window.x = -window.geometry.x as f64;
                    window.y = -window.geometry.y as f64;
                    window.fullscreen = true;
                } else if !fullscreen && window.fullscreen {
                    if let Some((x, y, w, h)) = window.saved_geometry.take() {
                        window.x = x;
                        window.y = y;
                        window.w = w;
                        window.h = h;
                    }
                    window.fullscreen = false;
                }

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    let mut states = Vec::new();
                    states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                    if window.maximized {
                        states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                    }
                    if window.fullscreen {
                        states.extend_from_slice(&(State::Fullscreen as u32).to_ne_bytes());
                    }
                    toplevel.configure(target_w, target_h, states);
                    xdg_surface.configure(0);
                }
            }
        }
    }

    fn set_minimized(&mut self, toplevel_id: &ObjectId) {
        if let Some(window) = self.find_window_by_toplevel_mut(toplevel_id) {
            window.minimized = true;
        }
    }

    fn set_modal(&mut self, toplevel_id: &ObjectId, modal: bool) {
        if let Some(window) = self.find_window_by_toplevel_mut(toplevel_id) {
            window.modal = modal;
        }
    }

    fn begin_interactive_move(
        &mut self,
        toplevel_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    ) {
        let target_surface_id = self.all_windows().iter().find_map(|w| {
            if let Some(top) = &w.toplevel {
                if &top.id() == toplevel_id {
                    return Some(w.surface.id());
                }
            }
            None
        });

        if let Some(surface_id) = target_surface_id {
            self.set_fullscreen(toplevel_id, false, screen_size);
            self.set_maximized(toplevel_id, false, screen_size);
            self.begin_drag(&surface_id, cursor_x, cursor_y, screen_size);
        }
    }

    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    ) {
        self.set_fullscreen(toplevel_id, false, screen_size);
        self.set_maximized(toplevel_id, false, screen_size);

        let window_info = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| {
                (
                    w.surface.id(),
                    w.x,
                    w.y,
                    w.geometry.w,
                    w.geometry.h,
                    w.geometry.x,
                    w.geometry.y,
                )
            });

        if let Some((id, x, y, gw, gh, gx, gy)) = window_info {
            self.resize_state = Some((id.clone(), edges, cursor_x, cursor_y, x, y, gw, gh, gx, gy));
            self.drag_state = None;
            self.focus_window(&id);
        }
    }

    fn begin_drag(
        &mut self,
        surface_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    ) {
        let toplevel_id = self
            .find_window(surface_id)
            .and_then(|w| w.toplevel.as_ref().map(|t| t.id()));
        if let Some(id) = toplevel_id {
            self.set_fullscreen(&id, false, screen_size);
            self.set_maximized(&id, false, screen_size);
        }

        if let Some(window) = self.find_window(surface_id) {
            let offset_x = cursor_x - window.x;
            let offset_y = cursor_y - window.y;
            self.drag_state = Some((surface_id.clone(), offset_x, offset_y));
            self.resize_state = None;
            self.focus_window(surface_id);
        }
    }

    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64) {
        if let Some((drag_id, off_x, off_y)) = self.drag_state.clone() {
            if let Some(window) = self.find_window_mut(&drag_id) {
                window.x = cursor_x - off_x;
                window.y = cursor_y - off_y;
            }
        }
    }

    fn end_drag(&mut self) {
        self.drag_state = None;
    }

    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32) {
        if let Some((
            id,
            edges,
            start_cx,
            start_cy,
            start_sx,
            start_sy,
            start_gw,
            start_gh,
            start_gx,
            start_gy,
        )) = self.resize_state.clone()
        {
            if let Some(window) = self.find_window_mut(&id) {
                let dx = cursor_x - start_cx;
                let dy = cursor_y - start_cy;

                let mut new_gx = start_sx + start_gx as f64;
                let mut new_gy = start_sy + start_gy as f64;
                let mut new_gw = start_gw as f64;
                let mut new_gh = start_gh as f64;

                if (edges & 4) != 0 {
                    new_gx += dx;
                    new_gw -= dx;
                } else if (edges & 8) != 0 {
                    new_gw += dx;
                }

                if (edges & 1) != 0 {
                    new_gy += dy;
                    new_gh -= dy;
                } else if (edges & 2) != 0 {
                    new_gh += dy;
                }

                if new_gw < 100.0 {
                    new_gw = 100.0;
                }
                if new_gh < 100.0 {
                    new_gh = 100.0;
                }

                window.x = new_gx - window.geometry.x as f64;
                window.y = new_gy - window.geometry.y as f64;

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    let state_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Resizing as u32;
                    let mut states_bytes = state_val.to_ne_bytes().to_vec();
                    let act_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated
                            as u32;
                    states_bytes.extend_from_slice(&act_val.to_ne_bytes());

                    toplevel.configure(new_gw as i32, new_gh as i32, states_bytes);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn end_resize(&mut self) {
        if let Some((id, ..)) = self.resize_state.take() {
            if let Some(window) = self.find_window_mut(&id) {
                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    let act_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated
                            as u32;
                    let states_bytes = act_val.to_ne_bytes().to_vec();
                    toplevel.configure(0, 0, states_bytes);
                    xdg_surface.configure(1);
                }
            }
        }
    }

    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.w = w;
            window.h = h;
        }
    }

    fn map_layer_surface(
        &mut self,
        surface: WlSurface,
        layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        layer: u32,
        namespace: String,
    ) {
        debug!(
            "Mapping layer surface: id={:?}, layer={} namespace={namespace}",
            surface.id(),
            layer
        );
        if self.outputs.is_empty() {
            return;
        }
        let out = &mut self.outputs[0];

        let win = WindowState {
            surface,
            xdg_surface: None,
            toplevel: None,
            layer_surface: Some(layer_surface),
            parent_id: None,
            x: 0.0,
            y: 0.0,
            w: 0,
            h: 0,
            geometry: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            title: None,
            app_id: None,
            ssd: false,
            maximized: false,
            fullscreen: false,
            minimized: false,
            modal: false,
            saved_geometry: None,
            layer,
            anchor: 0,
            exclusive_zone: 0,
            margin: (0, 0, 0, 0),
            keyboard_interactivity: 0,
            is_interacting: false,
        };

        match layer {
            0 => out.background.windows.push(win),
            1 => out.bottom.windows.push(win),
            2 => out.top.windows.push(win),
            3 => out.overlay.windows.push(win),
            _ => out.top.windows.push(win),
        }
    }

    fn set_layer_surface_size(&mut self, surface_id: &ObjectId, w: u32, h: u32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.w = w as i32;
            window.h = h as i32;
            window.geometry.w = w as i32;
            window.geometry.h = h as i32;
        }
    }

    fn set_layer_surface_anchor(&mut self, surface_id: &ObjectId, anchor: u32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.anchor = anchor;
        }
    }

    fn set_layer_surface_zone(&mut self, surface_id: &ObjectId, zone: i32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.exclusive_zone = zone;
        }
    }

    fn set_layer_surface_margin(
        &mut self,
        surface_id: &ObjectId,
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    ) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.margin = (top, right, bottom, left);
        }
    }

    fn set_layer_keyboard_interactivity(&mut self, surface_id: &ObjectId, interactivity: u32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.keyboard_interactivity = interactivity;
        }
    }

    fn recalculate_layer_layout(&mut self, screen_size: (u16, u16)) {
        if self.outputs.is_empty() {
            return;
        }

        // Very basic layout calculation
        // A complete implementation would handle exclusive zones, anchors, and margins properly
        let out = &mut self.outputs[0];

        let layers = vec![
            &mut out.background.windows,
            &mut out.bottom.windows,
            &mut out.top.windows,
            &mut out.overlay.windows,
        ];

        let screen_w = screen_size.0 as i32;
        let screen_h = screen_size.1 as i32;

        for layer_list in layers {
            for win in layer_list.iter_mut() {
                if win.layer_surface.is_none() {
                    continue;
                }

                // Simple layout based on anchor
                // Top=1, Bottom=2, Left=4, Right=8
                let mut x = win.margin.3 as f64; // Left margin
                let mut y = win.margin.0 as f64; // Top margin

                if (win.anchor & 1) != 0 && (win.anchor & 2) != 0 {
                    // Top and Bottom anchored
                    win.h = screen_h - win.margin.0 - win.margin.2;
                    win.geometry.h = win.h;
                } else if (win.anchor & 2) != 0 {
                    // Bottom anchored
                    y = (screen_h - win.h - win.margin.2) as f64;
                }

                if (win.anchor & 4) != 0 && (win.anchor & 8) != 0 {
                    // Left and Right anchored
                    win.w = screen_w - win.margin.1 - win.margin.3;
                    win.geometry.w = win.w;
                } else if (win.anchor & 8) != 0 {
                    // Right anchored
                    x = (screen_w - win.w - win.margin.1) as f64;
                }

                win.x = x;
                win.y = y;

                // Configure surface
                if let Some(ls) = &win.layer_surface {
                    ls.configure(0, win.w as u32, win.h as u32);
                }
            }
        }
    }

    fn get_background(&self) -> Vec<WindowState> {
        self.outputs
            .first()
            .map(|o| o.background.windows.clone())
            .unwrap_or_default()
    }

    fn get_bottom(&self) -> Vec<WindowState> {
        self.outputs
            .first()
            .map(|o| o.bottom.windows.clone())
            .unwrap_or_default()
    }

    fn get_workspace_windows(&self) -> Vec<WindowState> {
        self.outputs
            .first()
            .and_then(|o| {
                o.workspaces
                    .get(o.active_workspace)
                    .map(|ws| ws.unwrap_ref().windows.clone())
            })
            .unwrap_or_default()
    }

    fn get_top(&self) -> Vec<WindowState> {
        self.outputs
            .first()
            .map(|o| o.top.windows.clone())
            .unwrap_or_default()
    }

    fn get_overlay(&self) -> Vec<WindowState> {
        self.outputs
            .first()
            .map(|o| o.overlay.windows.clone())
            .unwrap_or_default()
    }

    fn get_popups(&self) -> Vec<PopupState> {
        self.popups.clone()
    }

    fn get_focused_window(&self) -> Option<WlSurface> {
        if self.outputs.is_empty() {
            return None;
        }

        let out = &self.outputs[0];

        if let Some(w) = out.overlay.windows.last() {
            return Some(w.surface.clone());
        }

        if let Some(w) = out.top.windows.last() {
            return Some(w.surface.clone());
        }
        if let Some(Slot::Occupied(ws)) = out.workspaces.get(out.active_workspace) {
            if let Some(w) = ws.windows.last() {
                return Some(w.surface.clone());
            }
        }

        if let Some(w) = out.bottom.windows.last() {
            return Some(w.surface.clone());
        }

        if let Some(w) = out.background.windows.last() {
            return Some(w.surface.clone());
        }

        None
    }

    fn get_surface_position(&self, surface_id: &ObjectId) -> Option<(f64, f64)> {
        if let Some(win) = self.find_window(surface_id) {
            return Some((win.x, win.y));
        }

        if let Some(popup) = self.popups.iter().find(|p| &p.surface.id() == surface_id) {
            // Popup x/y are relative to parent surface's window geometry
            let (parent_abs_x, parent_abs_y) = self.get_absolute_position(&popup.parent_surface_id);
            let surf_x = parent_abs_x + popup.x as f64 - popup.geometry.x as f64;
            let surf_y = parent_abs_y + popup.y as f64 - popup.geometry.y as f64;
            return Some((surf_x, surf_y));
        }

        None
    }

    fn get_absolute_position(&self, surface_id: &ObjectId) -> (f64, f64) {
        if let Some(win) = self.find_window(surface_id) {
            return (win.x + win.geometry.x as f64, win.y + win.geometry.y as f64);
        }

        if let Some(popup) = self.popups.iter().find(|p| &p.surface.id() == surface_id) {
            let (parent_abs_x, parent_abs_y) = self.get_absolute_position(&popup.parent_surface_id);
            return (parent_abs_x + popup.x as f64, parent_abs_y + popup.y as f64);
        }

        (0.0, 0.0)
    }

    fn create_workspace(
        &mut self,
        output_id: usize,
        insert_position: WorkspaceInsertPosition,
    ) -> Option<usize> {
        let output = self.outputs.iter_mut().find(|o| o.id == output_id)?;

        match insert_position {
            WorkspaceInsertPosition::After(i) => {
                if output.workspaces.get(i).is_some()
                    && output.workspaces.insert_after(i, Workspace::new(i + 1))
                {
                    Some(i + 1)
                } else {
                    None
                }
            }
            WorkspaceInsertPosition::Before(i) => {
                let target_index = if i > 0 { i - 1 } else { return None };
                if output.workspaces.get(i).is_some()
                    && output
                        .workspaces
                        .insert_before(i, Workspace::new(target_index))
                {
                    Some(target_index)
                } else {
                    None
                }
            }
        }
    }

    fn delete_workspace(&mut self, output_id: usize, id: usize) -> bool {
        if let Some(output) = self.outputs.iter_mut().find(|o| o.id == output_id) {
            output.workspaces.remove(id)
        } else {
            false
        }
    }

    fn move_window_to_workspace(
        &mut self,
        surface_id: &ObjectId,
        output_id: usize,
        workspace_id: usize,
    ) -> bool {
        // 1. we find and remove the window from its current location
        let mut target_window = None;
        for out in &mut self.outputs {
            // Check background
            if let Some(idx) = out
                .background
                .windows
                .iter()
                .position(|w| &w.surface.id() == surface_id)
            {
                target_window = Some(out.background.windows.remove(idx));
                break;
            }
            // Check bottom
            if let Some(idx) = out
                .bottom
                .windows
                .iter()
                .position(|w| &w.surface.id() == surface_id)
            {
                target_window = Some(out.bottom.windows.remove(idx));
                break;
            }
            // Check workspaces
            let mut found = false;
            for ws in out.workspaces.flatten_mut() {
                if let Some(idx) = ws
                    .windows
                    .iter()
                    .position(|w| &w.surface.id() == surface_id)
                {
                    target_window = Some(ws.windows.remove(idx));
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
            // Check top
            if let Some(idx) = out
                .top
                .windows
                .iter()
                .position(|w| &w.surface.id() == surface_id)
            {
                target_window = Some(out.top.windows.remove(idx));
                break;
            }
            // Check overlay
            if let Some(idx) = out
                .overlay
                .windows
                .iter()
                .position(|w| &w.surface.id() == surface_id)
            {
                target_window = Some(out.overlay.windows.remove(idx));
                break;
            }
        }

        // 2. If the window was found, insert it into the target workspace
        if let Some(window) = target_window {
            if let Some(output) = self.outputs.iter_mut().find(|o| o.id == output_id) {
                if let Some(slot) = output.workspaces.get_mut(workspace_id) {
                    if let Slot::Occupied(ws) = slot {
                        ws.windows.push(window);
                        return true;
                    }
                }
            }
            // Fallback: If target workspace/output doesn't exist, put it back in the current active workspace
            // so we don't lose the window.
            if !self.outputs.is_empty() {
                let out = &mut self.outputs[0];
                if let Some(Slot::Occupied(ws)) = out.workspaces.get_mut(out.active_workspace) {
                    ws.windows.push(window);
                }
            }
        }
        false
    }

    fn focus_before_workspace(&mut self) -> bool {
        if self.outputs.is_empty() {
            return false;
        }

        let out = &mut self.outputs[0];
        let mut target = out.active_workspace;

        debug!("[wm] trying to go before from (T{target})");

        while target > 0 {
            target -= 1;
            if let Some(slot) = out.workspaces.get(target) {
                if !slot.is_empty() {
                    out.active_workspace = target;
                    debug!("[wm] focused before (T{target})");
                    return true;
                }
            }
        }

        false
    }

    fn focus_after_workspace(&mut self) -> bool {
        if self.outputs.is_empty() {
            return false;
        }

        let output_id = self.outputs[0].id;
        let current_idx = self.outputs[0].active_workspace;
        debug!("[wm] trying to go after (T{current_idx})");

        let current_is_empty =
            if let Some(Slot::Occupied(ws)) = self.outputs[0].workspaces.get(current_idx) {
                ws.windows.is_empty()
            } else {
                true
            };

        if current_is_empty {
            return false;
        }

        let target_idx = current_idx + 1;

        let next_is_occupied = matches!(
            self.outputs[0].workspaces.get(target_idx),
            Some(Slot::Occupied(_))
        );

        if next_is_occupied {
            self.outputs[0].active_workspace = target_idx;
            debug!("[wm] focused after (T{target_idx})");
            return true;
        } else {
            if self
                .create_workspace(output_id, WorkspaceInsertPosition::After(current_idx))
                .is_some()
            {
                self.outputs[0].active_workspace = target_idx;
                debug!("[wm] focused after (T{target_idx})");
                return true;
            }
        }

        false
    }
}

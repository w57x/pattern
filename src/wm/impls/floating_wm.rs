use std::cell::Cell;

use crate::wm::*;
use tracing::debug;
use wayland_server::Resource;

pub struct Wm {
    pub outputs: Vec<OutputState>,
    pub popups: Vec<PopupState>,
    pub drag_state: Option<(ObjectId, f64, f64)>,
    pub resize_state: Option<(ObjectId, u32, f64, f64, f64, f64, i32, i32, i32, i32)>,
    pub workspace_swipe_offset: f64,
    pub is_swiping: bool,
    pub compaction_occurred: Cell<bool>,
}

impl Wm {
    pub fn new() -> Self {
        Self {
            outputs: vec![OutputState::new(0)],
            popups: Vec::new(),
            drag_state: None,
            resize_state: None,
            workspace_swipe_offset: 0.0,
            is_swiping: false,
            compaction_occurred: Cell::new(false),
        }
    }

    fn ensure_workspace(&mut self, ws_id: usize) {
        if self.outputs.is_empty() {
            return;
        }
        let out = &mut self.outputs[0];
        for i in 0..=ws_id {
            let slot = out.workspaces.get_mut(i).unwrap();
            if slot.is_empty() {
                *slot = Slot::Occupied(Workspace::new(i));
            }
        }
    }

    fn compact_workspaces(&mut self) {
        if self.outputs.is_empty() {
            return;
        }
        let out = &mut self.outputs[0];

        let ws_count = out.workspaces.inner.len();
        let mut new_workspaces = vec![Slot::Empty; ws_count];
        let mut write_idx = 0;
        let mut active_ws_shifted = out.active_workspace;

        for read_idx in 0..ws_count {
            if let Some(Slot::Occupied(ws)) = out.workspaces.get(read_idx) {
                if !ws.windows.is_empty() || read_idx == out.active_workspace {
                    let mut compacted_ws = ws.clone();
                    compacted_ws.id = write_idx;
                    new_workspaces[write_idx] = Slot::Occupied(compacted_ws);
                    if read_idx == out.active_workspace {
                        active_ws_shifted = write_idx;
                    }
                    write_idx += 1;
                }
            }
        }

        // Fill remaining slots with empty workspaces
        for idx in write_idx..ws_count {
            new_workspaces[idx] = Slot::Occupied(Workspace::new(idx));
        }

        out.workspaces.inner = new_workspaces;
        if out.active_workspace != active_ws_shifted {
            out.active_workspace = active_ws_shifted;
            self.compaction_occurred.set(true);
        }
    }

    fn max_allowed_workspace(&self) -> usize {
        let mut max_occupied = None;
        if !self.outputs.is_empty() {
            let out = &self.outputs[0];
            for ws in out.workspaces.flatten() {
                if !ws.windows.is_empty() {
                    if max_occupied.map_or(true, |max| ws.id > max) {
                        max_occupied = Some(ws.id);
                    }
                }
            }
        }
        match max_occupied {
            Some(idx) => idx + 1,
            None => 0,
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

    fn get_root_parent_id(&self, surface_id: &ObjectId) -> ObjectId {
        let mut current_id = surface_id.clone();
        while let Some(win) = self.find_window(&current_id) {
            if let Some(pid) = &win.parent_id {
                if let Some(parent_win) = self
                    .all_windows()
                    .iter()
                    .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(pid.clone()))
                {
                    current_id = parent_win.surface.id();
                    continue;
                }
            }
            break;
        }
        current_id
    }

    fn get_descendants_ordered(&self, surface_id: &ObjectId) -> Vec<ObjectId> {
        let mut ordered = vec![surface_id.clone()];
        let mut index = 0;
        while index < ordered.len() {
            let current_id = &ordered[index];
            if let Some(win) = self.find_window(current_id) {
                if let Some(toplevel) = &win.toplevel {
                    let tid = toplevel.id();
                    for w in self.all_windows() {
                        if w.parent_id.as_ref() == Some(&tid) {
                            let child_surf_id = w.surface.id();
                            if !ordered.contains(&child_surf_id) {
                                ordered.push(child_surf_id);
                            }
                        }
                    }
                }
            }
            index += 1;
        }
        ordered
    }

    fn get_transient_group(&self, surface_id: &ObjectId) -> Vec<ObjectId> {
        let mut group = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(surface_id.clone());
        group.push(surface_id.clone());

        while let Some(current_id) = queue.pop_front() {
            if let Some(win) = self.find_window(&current_id) {
                if let Some(pid) = &win.parent_id {
                    if let Some(parent_win) = self
                        .all_windows()
                        .iter()
                        .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(pid.clone()))
                    {
                        let parent_surf_id = parent_win.surface.id();
                        if !group.contains(&parent_surf_id) {
                            group.push(parent_surf_id.clone());
                            queue.push_back(parent_surf_id);
                        }
                    }
                }
                if let Some(toplevel) = &win.toplevel {
                    let tid = toplevel.id();
                    for w in self.all_windows() {
                        if w.parent_id.as_ref() == Some(&tid) {
                            let child_surf_id = w.surface.id();
                            if !group.contains(&child_surf_id) {
                                group.push(child_surf_id.clone());
                                queue.push_back(child_surf_id);
                            }
                        }
                    }
                }
            }
        }
        group
    }

    fn restack_window_group(&mut self, surface_id: &ObjectId) {
        let root_id = self.get_root_parent_id(surface_id);
        let ordered_ids = self.get_descendants_ordered(&root_id);
        for id in ordered_ids {
            for out in &mut self.outputs {
                if let Some(idx) = out
                    .background
                    .windows
                    .iter()
                    .position(|w| w.surface.id() == id)
                {
                    let w = out.background.windows.remove(idx);
                    out.background.windows.push(w);
                }
                if let Some(idx) = out.bottom.windows.iter().position(|w| w.surface.id() == id) {
                    let w = out.bottom.windows.remove(idx);
                    out.bottom.windows.push(w);
                }
                for ws in out.workspaces.flatten_mut() {
                    if let Some(idx) = ws.windows.iter().position(|w| w.surface.id() == id) {
                        let w = ws.windows.remove(idx);
                        ws.windows.push(w);
                    }
                }
                if let Some(idx) = out.top.windows.iter().position(|w| w.surface.id() == id) {
                    let w = out.top.windows.remove(idx);
                    out.top.windows.push(w);
                }
                if let Some(idx) = out
                    .overlay
                    .windows
                    .iter()
                    .position(|w| w.surface.id() == id)
                {
                    let w = out.overlay.windows.remove(idx);
                    out.overlay.windows.push(w);
                }
            }
        }
    }

    fn maintain_transient_constraints(&mut self) {
        let mut updates = Vec::new();
        let windows = self.all_windows();
        for w in &windows {
            if let Some(ref parent_tid) = w.parent_id {
                if let Some(parent) = windows
                    .iter()
                    .find(|pw| pw.toplevel.as_ref().map(|t| t.id()) == Some(parent_tid.clone()))
                {
                    let child_w = if w.geometry.w > 0 { w.geometry.w } else { w.w };
                    let child_h = if w.geometry.h > 0 { w.geometry.h } else { w.h };
                    let parent_w = if parent.geometry.w > 0 {
                        parent.geometry.w
                    } else {
                        parent.w
                    };
                    let parent_h = if parent.geometry.h > 0 {
                        parent.geometry.h
                    } else {
                        parent.h
                    };

                    let target_center_x =
                        parent.x + parent.geometry.x as f64 + (parent_w as f64) / 2.0;
                    let target_center_y =
                        parent.y + parent.geometry.y as f64 + (parent_h as f64) / 2.0;

                    let new_x = target_center_x - (w.geometry.x as f64) - (child_w as f64) / 2.0;
                    let new_y = target_center_y - (w.geometry.y as f64) - (child_h as f64) / 2.0;
                    updates.push((w.surface.id(), new_x, new_y));
                }
            }
        }
        for (sid, x, y) in updates {
            if let Some(w) = self.find_window_mut(&sid) {
                w.x = x;
                w.y = y;
            }
        }
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

        let (usable_x, usable_y) = (out.usable_area.x as f64, out.usable_area.y as f64);

        ws.windows.push(WindowState {
            surface,
            xdg_surface: None,
            toplevel: None,
            layer_surface: None,
            parent_id: None,
            x: usable_x + 100.0 + offset,
            y: usable_y + 100.0 + offset,
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
            sent_configures: Vec::new(),
            acknowledged_serial: None,
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
        self.compact_workspaces();
    }

    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId {
        let mut target_id = surface_id.clone();
        while let Some(popup) = self.popups.iter().find(|p| p.surface.id() == target_id) {
            target_id = popup.parent_surface_id.clone();
        }

        let target_toplevel_id = self
            .find_window(&target_id)
            .and_then(|w| w.toplevel.as_ref().map(|t| t.id()));
        let has_transient_child = if let Some(ref tid) = target_toplevel_id {
            self.all_windows()
                .iter()
                .any(|w| w.parent_id.as_ref() == Some(tid))
        } else {
            false
        };
        if has_transient_child {
            if let Some(child_id) = self.all_windows().iter().find_map(|w| {
                if w.parent_id.as_ref() == target_toplevel_id.as_ref() {
                    Some(w.surface.id())
                } else {
                    None
                }
            }) {
                return self.focus_window(&child_id);
            }
        }

        self.restack_window_group(&target_id);
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
        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        if let Some(cid) = child_id {
            if let Some(child) = self.find_window_mut(&cid) {
                child.parent_id = parent_id;
            }
        }
        self.maintain_transient_constraints();
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
        self.maintain_transient_constraints();
    }

    fn set_maximized(
        &mut self,
        toplevel_id: &ObjectId,
        maximized: bool,
        screen_size: (u16, u16),
        serial: u32,
    ) {
        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        let usable = self.outputs.first().map(|o| o.usable_area).unwrap_or(Rect {
            x: 0,
            y: 0,
            w: screen_size.0 as i32,
            h: screen_size.1 as i32,
        });

        if let Some(id) = child_id {
            if let Some(window) = self.find_window_mut(&id) {
                if window.maximized == maximized {
                    return;
                }

                let (target_w, target_h) = if maximized {
                    (usable.w, usable.h)
                } else {
                    if let Some((_, _, w, h)) = window.saved_geometry {
                        (w, h)
                    } else {
                        (window.w, window.h)
                    }
                };

                let config = if let Some(existing) = window
                    .sent_configures
                    .iter_mut()
                    .find(|c| c.serial == serial)
                {
                    existing.maximized = maximized;
                    existing.w = target_w;
                    existing.h = target_h;
                    existing.clone()
                } else {
                    let c = ConfigureState {
                        serial,
                        maximized,
                        fullscreen: window.fullscreen,
                        resizing: false,
                        edges: 0,
                        w: target_w,
                        h: target_h,
                        x: None,
                        y: None,
                    };
                    window.sent_configures.push(c.clone());
                    c
                };

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    let mut states = Vec::new();
                    states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                    if config.maximized {
                        states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                    }
                    if config.fullscreen {
                        states.extend_from_slice(&(State::Fullscreen as u32).to_ne_bytes());
                    }
                    toplevel.configure(config.w, config.h, states);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn set_fullscreen(
        &mut self,
        toplevel_id: &ObjectId,
        fullscreen: bool,
        screen_size: (u16, u16),
        serial: u32,
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

                let config = if let Some(existing) = window
                    .sent_configures
                    .iter_mut()
                    .find(|c| c.serial == serial)
                {
                    existing.fullscreen = fullscreen;
                    existing.w = target_w;
                    existing.h = target_h;
                    existing.clone()
                } else {
                    let c = ConfigureState {
                        serial,
                        maximized: window.maximized,
                        fullscreen,
                        resizing: false,
                        edges: 0,
                        w: target_w,
                        h: target_h,
                        x: None,
                        y: None,
                    };
                    window.sent_configures.push(c.clone());
                    c
                };

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    let mut states = Vec::new();
                    states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                    if window.maximized {
                        states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                    }
                    if config.fullscreen {
                        states.extend_from_slice(&(State::Fullscreen as u32).to_ne_bytes());
                    }
                    toplevel.configure(config.w, config.h, states);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn set_minimized(&mut self, toplevel_id: &ObjectId) -> Option<ObjectId> {
        let child_id = self
            .all_windows()
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
            .map(|w| w.surface.id());
        if let Some(id) = child_id {
            if let Some(window) = self.find_window_mut(&id) {
                window.minimized = true;
            }
            // If the minimized window had focus, refocus the next top window in the same workspace
            let active_ws = self.outputs[0].active_workspace;
            let mut target_to_focus = None;
            if let Some(slot) = self.outputs[0].workspaces.get(active_ws) {
                if let Slot::Occupied(ws) = slot {
                    if let Some(next_focus) = ws
                        .windows
                        .iter()
                        .rev()
                        .find(|w| !w.minimized && w.surface.id() != id)
                    {
                        target_to_focus = Some(next_focus.surface.id());
                    }
                }
            }
            if let Some(refocus_id) = target_to_focus {
                self.focus_window(&refocus_id);
                return Some(refocus_id);
            }
        }
        None
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
        serial: u32,
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
            self.set_fullscreen(toplevel_id, false, screen_size, serial);
            self.set_maximized(toplevel_id, false, screen_size, serial);
            self.begin_drag(&surface_id, cursor_x, cursor_y, screen_size, serial);
        }
    }

    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
        serial: u32,
    ) {
        self.set_fullscreen(toplevel_id, false, screen_size, serial);
        self.set_maximized(toplevel_id, false, screen_size, serial);

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
        serial: u32,
    ) {
        let toplevel_id = self
            .find_window(surface_id)
            .and_then(|w| w.toplevel.as_ref().map(|t| t.id()));
        if let Some(id) = toplevel_id {
            self.set_fullscreen(&id, false, screen_size, serial);
            self.set_maximized(&id, false, screen_size, serial);
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
            if let Some(window) = self.find_window(&drag_id) {
                let target_x = cursor_x - off_x;
                let target_y = cursor_y - off_y;
                let dx = target_x - window.x;
                let dy = target_y - window.y;

                if dx != 0.0 || dy != 0.0 {
                    let group = self.get_transient_group(&drag_id);
                    for id in group {
                        if let Some(w) = self.find_window_mut(&id) {
                            w.x += dx;
                            w.y += dy;
                        }
                    }
                }
            }
        }
        self.maintain_transient_constraints();
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

                let target_x = new_gx - window.geometry.x as f64;
                let target_y = new_gy - window.geometry.y as f64;

                let config = ConfigureState {
                    serial,
                    maximized: window.maximized,
                    fullscreen: window.fullscreen,
                    resizing: true,
                    edges,
                    w: new_gw as i32,
                    h: new_gh as i32,
                    x: Some(target_x),
                    y: Some(target_y),
                };
                window.sent_configures.push(config);

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                    let mut states = Vec::new();
                    states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                    states.extend_from_slice(&(State::Resizing as u32).to_ne_bytes());
                    if window.maximized {
                        states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                    }
                    if window.fullscreen {
                        states.extend_from_slice(&(State::Fullscreen as u32).to_ne_bytes());
                    }

                    toplevel.configure(new_gw as i32, new_gh as i32, states);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn end_resize(&mut self, serial: u32) {
        if let Some((id, ..)) = self.resize_state.take() {
            if let Some(window) = self.find_window_mut(&id) {
                let config = ConfigureState {
                    serial,
                    maximized: window.maximized,
                    fullscreen: window.fullscreen,
                    resizing: false,
                    edges: 0,
                    w: window.w,
                    h: window.h,
                    x: None,
                    y: None,
                };
                window.sent_configures.push(config);

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
                    toplevel.configure(0, 0, states);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.w = w;
            window.h = h;
        }
        self.maintain_transient_constraints();
    }

    fn ack_configure(&mut self, surface_id: &ObjectId, serial: u32) {
        if let Some(window) = self.find_window_mut(surface_id) {
            window.acknowledged_serial = Some(serial);
        }
    }

    fn apply_committed_configure(&mut self, surface_id: &ObjectId, actual_w: i32, actual_h: i32) {
        let (usable_x, usable_y) = self
            .outputs
            .first()
            .map(|o| (o.usable_area.x, o.usable_area.y))
            .unwrap_or((0, 0));
        let mut target_config = None;
        if let Some(window) = self.find_window_mut(surface_id) {
            if let Some(serial) = window.acknowledged_serial.take() {
                if let Some(idx) = window
                    .sent_configures
                    .iter()
                    .position(|c| c.serial == serial)
                {
                    target_config = Some(window.sent_configures[idx].clone());
                    window.sent_configures.drain(0..=idx);
                }
            }
        }

        if let Some(config) = target_config {
            if let Some(window) = self.find_window_mut(surface_id) {
                // Apply fullscreen
                if config.fullscreen && !window.fullscreen {
                    window.saved_geometry = Some((window.x, window.y, window.w, window.h));
                    window.x = -window.geometry.x as f64;
                    window.y = -window.geometry.y as f64;
                    window.w = config.w;
                    window.h = config.h;
                    window.fullscreen = true;
                    window.maximized = false;
                } else if !config.fullscreen && window.fullscreen {
                    if let Some((x, y, w, h)) = window.saved_geometry.take() {
                        window.x = x;
                        window.y = y;
                        window.w = w;
                        window.h = h;
                    }
                    window.fullscreen = false;
                }

                // Apply maximized
                if !window.fullscreen {
                    if config.maximized && !window.maximized {
                        window.saved_geometry = Some((window.x, window.y, window.w, window.h));
                        window.x = usable_x as f64 - window.geometry.x as f64;
                        window.y = usable_y as f64 - window.geometry.y as f64;
                        window.w = config.w;
                        window.h = config.h;
                        window.maximized = true;
                    } else if !config.maximized && window.maximized {
                        if let Some((x, y, w, h)) = window.saved_geometry.take() {
                            window.x = x;
                            window.y = y;
                            window.w = w;
                            window.h = h;
                        }
                        window.maximized = false;
                    } else if window.maximized {
                        window.x = usable_x as f64 - window.geometry.x as f64;
                        window.y = usable_y as f64 - window.geometry.y as f64;
                        window.w = config.w;
                        window.h = config.h;
                    }
                }

                // Apply resizing updates with exact edge adjustments
                if config.resizing && !window.maximized && !window.fullscreen {
                    if config.edges & 4 != 0 {
                        // Resizing left
                        let old_right_edge = window.x + window.geometry.x as f64 + window.w as f64;
                        window.x = old_right_edge - window.geometry.x as f64 - actual_w as f64;
                    }
                    if config.edges & 1 != 0 {
                        // Resizing top
                        let old_bottom_edge = window.y + window.geometry.y as f64 + window.h as f64;
                        window.y = old_bottom_edge - window.geometry.y as f64 - actual_h as f64;
                    }
                    window.w = actual_w;
                    window.h = actual_h;
                }
            }
        }
        self.maintain_transient_constraints();
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
            sent_configures: Vec::new(),
            acknowledged_serial: None,
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

    fn recalculate_layer_layout(&mut self, screen_size: (u16, u16), serial: u32) {
        if self.outputs.is_empty() {
            return;
        }

        let out = &mut self.outputs[0];

        let screen_w = screen_size.0 as i32;
        let screen_h = screen_size.1 as i32;

        let mut u_top = 0;
        let mut u_bottom = screen_h;
        let mut u_left = 0;
        let mut u_right = screen_w;

        let layers = vec![
            &mut out.background.windows,
            &mut out.bottom.windows,
            &mut out.top.windows,
            &mut out.overlay.windows,
        ];

        for layer_list in layers {
            for win in layer_list.iter_mut() {
                if win.layer_surface.is_none() {
                    continue;
                }

                let (box_top, box_bottom, box_left, box_right) = if win.exclusive_zone == -1 {
                    (0, screen_h, 0, screen_w)
                } else {
                    (u_top, u_bottom, u_left, u_right)
                };

                let x;
                let y;

                // Vertical positioning and height
                if (win.anchor & 1) != 0 && (win.anchor & 2) != 0 {
                    // Top and Bottom anchored
                    win.h = (box_bottom - box_top - win.margin.0 - win.margin.2).max(0);
                    win.geometry.h = win.h;
                    y = (box_top + win.margin.0) as f64;
                } else if (win.anchor & 1) != 0 {
                    // Top anchored only
                    y = (box_top + win.margin.0) as f64;
                } else if (win.anchor & 2) != 0 {
                    // Bottom anchored only
                    y = (box_bottom - win.h - win.margin.2) as f64;
                } else {
                    // Centered vertically
                    y = (box_top + (box_bottom - box_top - win.h) / 2) as f64;
                }

                // Horizontal positioning and width
                if (win.anchor & 4) != 0 && (win.anchor & 8) != 0 {
                    // Left and Right anchored
                    win.w = (box_right - box_left - win.margin.3 - win.margin.1).max(0);
                    win.geometry.w = win.w;
                    x = (box_left + win.margin.3) as f64;
                } else if (win.anchor & 4) != 0 {
                    // Left anchored only
                    x = (box_left + win.margin.3) as f64;
                } else if (win.anchor & 8) != 0 {
                    // Right anchored only
                    x = (box_right - win.w - win.margin.1) as f64;
                } else {
                    // Centered horizontally
                    x = (box_left + (box_right - box_left - win.w) / 2) as f64;
                }

                win.x = x;
                win.y = y;

                // Configure surface
                if let Some(ls) = &win.layer_surface {
                    ls.configure(0, win.w as u32, win.h as u32);
                }

                // Update usable area bounds if this window has a valid exclusive zone
                let set_bits = win.anchor.count_ones();
                if (set_bits == 1 || set_bits == 3) && win.exclusive_zone > 0 {
                    if (win.anchor & 1) != 0 && (win.anchor & 2) == 0 {
                        u_top = box_top + win.exclusive_zone;
                    } else if (win.anchor & 2) != 0 && (win.anchor & 1) == 0 {
                        u_bottom = box_bottom - win.exclusive_zone;
                    } else if (win.anchor & 4) != 0 && (win.anchor & 8) == 0 {
                        u_left = box_left + win.exclusive_zone;
                    } else if (win.anchor & 8) != 0 && (win.anchor & 4) == 0 {
                        u_right = box_right - win.exclusive_zone;
                    }
                }
            }
        }

        // Store the calculated usable area
        out.usable_area = Rect {
            x: u_left,
            y: u_top,
            w: (u_right - u_left).max(0),
            h: (u_bottom - u_top).max(0),
        };

        // Update maximized windows to fit the new usable area
        for out in &mut self.outputs {
            let usable = out.usable_area;
            for ws in out.workspaces.flatten_mut() {
                for win in &mut ws.windows {
                    if win.maximized && !win.fullscreen {
                        win.w = usable.w;
                        win.h = usable.h;
                        win.x = usable.x as f64 - win.geometry.x as f64;
                        win.y = usable.y as f64 - win.geometry.y as f64;

                        if let (Some(toplevel), Some(xdg_surface)) =
                            (&win.toplevel, &win.xdg_surface)
                        {
                            use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                            let mut states = Vec::new();
                            states.extend_from_slice(&(State::Activated as u32).to_ne_bytes());
                            states.extend_from_slice(&(State::Maximized as u32).to_ne_bytes());
                            toplevel.configure(win.w, win.h, states);
                            xdg_surface.configure(serial);
                        }
                    }
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

        let focusable =
            |w: &&WindowState| w.layer_surface.is_none() || w.keyboard_interactivity > 0;

        if let Some(w) = out.overlay.windows.iter().filter(focusable).last() {
            return Some(w.surface.clone());
        }

        if let Some(w) = out.top.windows.iter().filter(focusable).last() {
            return Some(w.surface.clone());
        }
        if let Some(Slot::Occupied(ws)) = out.workspaces.get(out.active_workspace) {
            if let Some(w) = ws.windows.iter().filter(focusable).last() {
                return Some(w.surface.clone());
            }
        }

        if let Some(w) = out.bottom.windows.iter().filter(focusable).last() {
            return Some(w.surface.clone());
        }

        if let Some(w) = out.background.windows.iter().filter(focusable).last() {
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
            self.ensure_workspace(workspace_id);
            if let Some(output) = self.outputs.iter_mut().find(|o| o.id == output_id) {
                if let Some(Slot::Occupied(ws)) = output.workspaces.get_mut(workspace_id) {
                    ws.windows.push(window);
                    self.compact_workspaces();
                    return true;
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
            self.compact_workspaces();
        }
        false
    }

    fn focus_before_workspace(&mut self) -> bool {
        if self.outputs.is_empty() {
            return false;
        }

        let current_ws = self.outputs[0].active_workspace;
        if current_ws > 0 {
            let target_ws = current_ws - 1;
            self.ensure_workspace(target_ws);
            self.outputs[0].active_workspace = target_ws;
            debug!("[wm] focused before (T{})", target_ws);
            true
        } else {
            false
        }
    }

    fn focus_after_workspace(&mut self) -> bool {
        if self.outputs.is_empty() {
            return false;
        }

        let current_ws = self.outputs[0].active_workspace;
        if current_ws < usize::MAX {
            let target_ws = current_ws + 1;
            if target_ws > self.max_allowed_workspace() {
                return false;
            }
            self.ensure_workspace(target_ws);
            self.outputs[0].active_workspace = target_ws;
            debug!("[wm] focused after (T{})", target_ws);
            true
        } else {
            false
        }
    }

    fn focus_workspace(&mut self, id: usize) -> bool {
        if self.outputs.is_empty() {
            return false;
        }
        if id > self.max_allowed_workspace() {
            return false;
        }
        self.ensure_workspace(id);
        self.outputs[0].active_workspace = id;
        debug!("[wm] focused workspace {}", id);
        true
    }

    fn begin_workspace_swipe(&mut self) {
        self.workspace_swipe_offset = 0.0;
        self.is_swiping = true;
    }

    fn update_workspace_swipe(&mut self, dx: f64) {
        if self.is_swiping {
            let active_ws = self.get_active_workspace();
            let new_offset = self.workspace_swipe_offset + dx * 3.0;
            if active_ws == 0 && new_offset > 0.0 {
                self.workspace_swipe_offset = 0.0;
            } else if active_ws == usize::MAX && new_offset < 0.0 {
                self.workspace_swipe_offset = 0.0;
            } else {
                let mut allowed_offset = new_offset;
                if new_offset < 0.0 && active_ws < usize::MAX {
                    let target_ws = active_ws + 1;
                    if target_ws > self.max_allowed_workspace() {
                        allowed_offset = 0.0;
                    } else {
                        self.ensure_workspace(target_ws);
                    }
                }
                self.workspace_swipe_offset = allowed_offset;
            }
        }
    }

    fn end_workspace_swipe(&mut self, threshold: f64) {
        if !self.is_swiping {
            return;
        }
        self.is_swiping = false;

        let offset = self.workspace_swipe_offset;
        if offset > threshold {
            self.focus_before_workspace();
        } else if offset < -threshold {
            self.focus_after_workspace();
        }
    }

    fn get_workspace_offset(&self) -> f64 {
        if self.is_swiping {
            self.workspace_swipe_offset
        } else {
            0.0
        }
    }

    fn is_workspace_swiping(&self) -> bool {
        self.is_swiping
    }

    fn get_active_workspace(&self) -> usize {
        if self.outputs.is_empty() {
            0
        } else {
            self.outputs[0].active_workspace
        }
    }

    fn get_workspace_windows_by_id(&self, workspace_id: usize) -> Vec<WindowState> {
        if self.outputs.is_empty() {
            return Vec::new();
        }
        let out = &self.outputs[0];
        if let Some(Slot::Occupied(ws)) = out.workspaces.get(workspace_id) {
            return ws.windows.clone();
        }
        Vec::new()
    }

    fn get_workspace_id_for_window(&self, surface_id: &ObjectId) -> Option<usize> {
        if self.outputs.is_empty() {
            return None;
        }
        for output in &self.outputs {
            for ws in output.workspaces.flatten() {
                if ws.windows.iter().any(|w| &w.surface.id() == surface_id) {
                    return Some(ws.id);
                }
            }
        }
        None
    }

    fn is_resizing(&self) -> bool {
        self.resize_state.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.drag_state.is_some()
    }

    fn take_compaction_occurred(&self) -> bool {
        self.compaction_occurred.take()
    }
}

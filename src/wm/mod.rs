use wayland_protocols::xdg::shell::server::xdg_popup::XdgPopup;
use wayland_protocols::xdg::shell::server::xdg_surface::XdgSurface;
use wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone)]
pub struct WindowState {
    pub surface: WlSurface,
    pub xdg_surface: Option<XdgSurface>,
    pub toplevel: Option<XdgToplevel>,
    pub parent_id: Option<ObjectId>,
    pub x: f64,
    pub y: f64,
    pub w: i32,
    pub h: i32,
    pub geometry: Rect,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub ssd: bool,
}

#[derive(Clone)]
pub struct PopupState {
    pub surface: WlSurface,
    pub xdg_surface: XdgSurface,
    pub xdg_popup: XdgPopup,
    pub parent_surface_id: ObjectId,
    pub x: i32,
    pub y: i32,
}

pub trait WindowManager {
    /// Called when a new XDG Toplevel is created
    fn map_window(&mut self, surface: WlSurface);

    /// Called when a window is destroyed
    fn unmap_window(&mut self, surface_id: &ObjectId);

    /// Brings a window to the front and grants it focus. Returns the ID of the focused window (may be different if redirected).
    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId;

    fn assign_toplevel(
        &mut self,
        surface_id: &ObjectId,
        toplevel: XdgToplevel,
        xdg_surface: XdgSurface,
    );

    fn map_popup(&mut self, popup: PopupState);
    fn unmap_popup(&mut self, popup_surface_id: &ObjectId);

    fn set_window_title(&mut self, toplevel_id: &ObjectId, title: String);
    fn set_window_app_id(&mut self, toplevel_id: &ObjectId, app_id: String);
    fn set_window_parent(&mut self, toplevel_id: &ObjectId, parent_id: Option<ObjectId>);
    fn set_window_ssd(&mut self, toplevel_id: &ObjectId, enabled: bool);
    fn set_window_geometry(&mut self, surface_id: &ObjectId, geometry: Rect);

    fn begin_interactive_move(&mut self, toplevel_id: &ObjectId, cursor_x: f64, cursor_y: f64);
    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
    );

    /// Starts dragging a specific window
    fn begin_drag(&mut self, surface_id: &ObjectId, cursor_x: f64, cursor_y: f64);

    /// Updates the dragged window's position
    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64);

    /// Drops the window
    fn end_drag(&mut self);

    /// Updates the resized window's dimensions
    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32);
    fn end_resize(&mut self);

    /// Updates the window's dimensions based on the actual buffer size committed by the client
    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32);

    /// Returns the windows in back-to-front drawing order
    fn get_render_list(&self) -> Vec<WindowState>;

    /// Returns all active popups
    fn get_popups(&self) -> Vec<PopupState>;

    /// Returns the currently focused window (the one on top)
    fn get_focused_window(&self) -> Option<WlSurface>;

    fn get_absolute_position(&self, surface_id: &ObjectId) -> (f64, f64);
}

pub struct FloatingWm {
    // Windows are ordered back-to-front. The last element is the top/focused window.
    pub windows: Vec<WindowState>,

    pub popups: Vec<PopupState>,

    // Tracks: (Window ID, Grab Offset X, Grab Offset Y)
    pub drag_state: Option<(ObjectId, f64, f64)>,

    // Tracks: (Window ID, Edges, Start Cursor X, Start Cursor Y, Start Win X, Start Win Y, Start Win W, Start Win H)
    pub resize_state: Option<(ObjectId, u32, f64, f64, f64, f64, i32, i32)>,
}

impl FloatingWm {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            popups: Vec::new(),
            drag_state: None,
            resize_state: None,
        }
    }
}

impl WindowManager for FloatingWm {
    fn map_window(&mut self, surface: WlSurface) {
        let offset = (self.windows.len() * 30) as f64;
        self.windows.push(WindowState {
            surface,
            xdg_surface: None,
            toplevel: None,
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
        });
    }

    fn unmap_window(&mut self, surface_id: &ObjectId) {
        self.windows.retain(|w| &w.surface.id() != surface_id);
        self.popups.retain(|p| &p.surface.id() != surface_id);
        if let Some((drag_id, _, _)) = &self.drag_state {
            if drag_id == surface_id {
                self.drag_state = None;
            }
        }
        if let Some((resize_id, _, _, _, _, _, _, _)) = &self.resize_state {
            if resize_id == surface_id {
                self.resize_state = None;
            }
        }
    }

    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId {
        // Check if this window is a parent of an existing window
        let has_transient_child = self
            .windows
            .iter()
            .any(|w| w.parent_id.as_ref() == Some(surface_id));

        if has_transient_child {
            // If it has a child, find the child and focus IT instead
            if let Some(child_id) = self.windows.iter().find_map(|w| {
                if w.parent_id.as_ref() == Some(surface_id) {
                    Some(w.surface.id())
                } else {
                    None
                }
            }) {
                // Tail-call to focus the child (handles nested transients)
                return self.focus_window(&child_id);
            }
        }

        if let Some(index) = self
            .windows
            .iter()
            .position(|w| &w.surface.id() == surface_id)
        {
            let window = self.windows.remove(index);
            self.windows.push(window);
        }
        surface_id.clone()
    }

    fn assign_toplevel(
        &mut self,
        surface_id: &ObjectId,
        toplevel: XdgToplevel,
        xdg_surface: XdgSurface,
    ) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| &w.surface.id() == surface_id)
        {
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

    fn set_window_title(&mut self, toplevel_id: &ObjectId, title: String) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            window.title = Some(title);
        }
    }

    fn set_window_app_id(&mut self, toplevel_id: &ObjectId, app_id: String) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            window.app_id = Some(app_id);
        }
    }

    fn set_window_parent(&mut self, toplevel_id: &ObjectId, parent_id: Option<ObjectId>) {
        let (window_idx, parent_pos, parent_dim, parent_geom) = if let Some(idx) = self
            .windows
            .iter()
            .position(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            let parent_info = if let Some(pid) = &parent_id {
                self.windows
                    .iter()
                    .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(pid.clone()))
                    .map(|w| ((w.x, w.y), (w.w, w.h), w.geometry))
            } else {
                None
            };
            (
                Some(idx),
                parent_info.map(|p| p.0),
                parent_info.map(|p| p.1),
                parent_info.map(|p| p.2),
            )
        } else {
            (None, None, None, None)
        };

        if let Some(idx) = window_idx {
            self.windows[idx].parent_id = parent_id;
            // Centering logic relative to parent window geometry
            if let (Some(pos), Some(_dim), Some(geom)) = (parent_pos, parent_dim, parent_geom) {
                // Logical parent center
                let lpx = pos.0 + geom.x as f64 + (geom.w as f64) / 2.0;
                let lpy = pos.1 + geom.y as f64 + (geom.h as f64) / 2.0;

                // Position child so its logical center is at logical parent center
                let child_geom = self.windows[idx].geometry;
                self.windows[idx].x = lpx - (child_geom.x as f64) - (child_geom.w as f64) / 2.0;
                self.windows[idx].y = lpy - (child_geom.y as f64) - (child_geom.h as f64) / 2.0;
            }
        }
    }

    fn set_window_ssd(&mut self, toplevel_id: &ObjectId, enabled: bool) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            window.ssd = enabled;
        }
    }

    fn set_window_geometry(&mut self, surface_id: &ObjectId, geometry: Rect) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| &w.surface.id() == surface_id)
        {
            window.geometry = geometry;
        }
    }

    fn begin_interactive_move(&mut self, toplevel_id: &ObjectId, cursor_x: f64, cursor_y: f64) {
        let target_surface_id = self.windows.iter().find_map(|w| {
            if let Some(top) = &w.toplevel {
                if &top.id() == toplevel_id {
                    return Some(w.surface.id());
                }
            }
            None
        });

        if let Some(surface_id) = target_surface_id {
            self.begin_drag(&surface_id, cursor_x, cursor_y);
        }
    }

    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
    ) {
        if let Some(window) = self
            .windows
            .iter()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            self.resize_state = Some((
                window.surface.id(),
                edges,
                cursor_x,
                cursor_y,
                window.x,
                window.y,
                window.w,
                window.h,
            ));
            self.focus_window(&window.surface.id());
        }
    }

    fn begin_drag(&mut self, surface_id: &ObjectId, cursor_x: f64, cursor_y: f64) {
        if let Some(window) = self.windows.iter().find(|w| &w.surface.id() == surface_id) {
            let offset_x = cursor_x - window.x;
            let offset_y = cursor_y - window.y;
            self.drag_state = Some((surface_id.clone(), offset_x, offset_y));
            self.focus_window(surface_id);
        }
    }

    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64) {
        if let Some((drag_id, off_x, off_y)) = &self.drag_state {
            if let Some(window) = self.windows.iter_mut().find(|w| &w.surface.id() == drag_id) {
                window.x = cursor_x - off_x;
                window.y = cursor_y - off_y;
            }
        }
    }

    fn end_drag(&mut self) {
        self.drag_state = None;
    }

    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32) {
        if let Some((id, edges, start_cx, start_cy, start_x, start_y, start_w, start_h)) =
            self.resize_state.clone()
        {
            if let Some(window) = self.windows.iter_mut().find(|w| w.surface.id() == id) {
                let dx = cursor_x - start_cx;
                let dy = cursor_y - start_cy;

                let mut new_x = start_x;
                let mut new_y = start_y;
                let mut new_w = start_w as f64;
                let mut new_h = start_h as f64;

                if (edges & 4) != 0 {
                    // Left
                    new_x += dx;
                    new_w -= dx;
                } else if (edges & 8) != 0 {
                    // Right
                    new_w += dx;
                }

                if (edges & 1) != 0 {
                    // Top
                    new_y += dy;
                    new_h -= dy;
                } else if (edges & 2) != 0 {
                    // Bottom
                    new_h += dy;
                }

                if new_w < 100.0 {
                    new_w = 100.0;
                }
                if new_h < 100.0 {
                    new_h = 100.0;
                }

                window.x = new_x;
                window.y = new_y;
                window.w = new_w as i32;
                window.h = new_h as i32;

                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    let state_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Resizing as u32;
                    let mut states_bytes = state_val.to_ne_bytes().to_vec();
                    let act_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated
                            as u32;
                    states_bytes.extend_from_slice(&act_val.to_ne_bytes());

                    toplevel.configure(window.w, window.h, states_bytes);
                    xdg_surface.configure(serial);
                }
            }
        }
    }

    fn end_resize(&mut self) {
        if let Some((id, _, _, _, _, _, _, _)) = self.resize_state.take() {
            if let Some(window) = self.windows.iter_mut().find(|w| w.surface.id() == id) {
                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    let act_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated
                            as u32;
                    let states_bytes = act_val.to_ne_bytes().to_vec();
                    toplevel.configure(window.w, window.h, states_bytes);
                    // Just a dummy serial for end_resize, real one handled by client response
                    xdg_surface.configure(1);
                }
            }
        }
    }

    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| &w.surface.id() == surface_id)
        {
            window.w = w;
            window.h = h;
        }
    }

    fn get_render_list(&self) -> Vec<WindowState> {
        self.windows.clone()
    }

    fn get_popups(&self) -> Vec<PopupState> {
        self.popups.clone()
    }

    fn get_focused_window(&self) -> Option<WlSurface> {
        self.windows.last().map(|w| w.surface.clone())
    }

    fn get_absolute_position(&self, surface_id: &ObjectId) -> (f64, f64) {
        if let Some(win) = self.windows.iter().find(|w| &w.surface.id() == surface_id) {
            return (win.x, win.y);
        }

        if let Some(popup) = self.popups.iter().find(|p| &p.surface.id() == surface_id) {
            let (parent_x, parent_y) = self.get_absolute_position(&popup.parent_surface_id);
            return (parent_x + popup.x as f64, parent_y + popup.y as f64);
        }

        (0.0, 0.0)
    }
}

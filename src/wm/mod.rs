use wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

#[derive(Clone)]
pub struct WindowState {
    pub surface: WlSurface,
    pub toplevel: Option<XdgToplevel>,
    pub x: f64,
    pub y: f64,
}

pub trait WindowManager {
    /// Called when a new XDG Toplevel is created
    fn map_window(&mut self, surface: WlSurface);

    /// Called when a window is destroyed
    fn unmap_window(&mut self, surface_id: &ObjectId);

    /// Brings a window to the front and grants it focus
    fn focus_window(&mut self, surface_id: &ObjectId);

    fn assign_toplevel(&mut self, surface_id: &ObjectId, toplevel: XdgToplevel);
    fn begin_interactive_move(&mut self, toplevel_id: &ObjectId, cursor_x: f64, cursor_y: f64);

    /// Starts dragging a specific window
    fn begin_drag(&mut self, surface_id: &ObjectId, cursor_x: f64, cursor_y: f64);

    /// Updates the dragged window's position
    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64);

    /// Drops the window
    fn end_drag(&mut self);

    /// Returns the windows in back-to-front drawing order
    fn get_render_list(&self) -> Vec<WindowState>;

    /// Returns the currently focused window (the one on top)
    fn get_focused_window(&self) -> Option<WlSurface>;
}

pub struct FloatingWm {
    // Windows are ordered back-to-front. The last element is the top/focused window.
    pub windows: Vec<WindowState>,

    // Tracks: (Window ID, Grab Offset X, Grab Offset Y)
    pub drag_state: Option<(ObjectId, f64, f64)>,
}

impl FloatingWm {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            drag_state: None,
        }
    }
}

impl WindowManager for FloatingWm {
    fn map_window(&mut self, surface: WlSurface) {
        let offset = (self.windows.len() * 30) as f64;
        self.windows.push(WindowState {
            surface,
            toplevel: None,
            x: 100.0 + offset,
            y: 100.0 + offset,
        });
    }

    fn unmap_window(&mut self, surface_id: &ObjectId) {
        self.windows.retain(|w| &w.surface.id() != surface_id);
        if let Some((drag_id, _, _)) = &self.drag_state {
            if drag_id == surface_id {
                self.drag_state = None;
            }
        }
    }

    fn focus_window(&mut self, surface_id: &ObjectId) {
        // Find the window, remove it, and push it to the end of the Vec (Top of Z-index)

        if let Some(index) = self
            .windows
            .iter()
            .position(|w| &w.surface.id() == surface_id)
        {
            let window = self.windows.remove(index);
            self.windows.push(window);
        }
    }

    fn assign_toplevel(&mut self, surface_id: &ObjectId, toplevel: XdgToplevel) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| &w.surface.id() == surface_id)
        {
            window.toplevel = Some(toplevel);
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

    fn begin_drag(&mut self, surface_id: &ObjectId, cursor_x: f64, cursor_y: f64) {
        if let Some(window) = self.windows.iter().find(|w| &w.surface.id() == surface_id) {
            // Calculate exact grab offset relative to the specific window's X/Y
            let offset_x = cursor_x - window.x;
            let offset_y = cursor_y - window.y;
            self.drag_state = Some((surface_id.clone(), offset_x, offset_y));

            // Dragging a window should immediately focus it and bring it to the front
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

    fn get_render_list(&self) -> Vec<WindowState> {
        self.windows.clone()
    }

    fn get_focused_window(&self) -> Option<WlSurface> {
        self.windows.last().map(|w| w.surface.clone())
    }
}

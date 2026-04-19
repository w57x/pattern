use wayland_protocols::xdg::shell::server::xdg_popup::XdgPopup;
use wayland_protocols::xdg::shell::server::xdg_surface::XdgSurface;
use wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

/// A simple rectangle representing a region in 2D space.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    /// X-coordinate of the top-left corner.
    pub x: i32,
    /// Y-coordinate of the top-left corner.
    pub y: i32,
    /// Width of the rectangle.
    pub w: i32,
    /// Height of the rectangle.
    pub h: i32,
}

/// Represents the complete state of a top-level window.
///
/// This struct tracks both the Wayland protocol objects and the compositor-side
/// state (position, size, various flags) for each mapped window.
#[derive(Clone)]
pub struct WindowState {
    /// The base Wayland surface.
    pub surface: WlSurface,
    /// The XDG surface associated with this window, if any.
    pub xdg_surface: Option<XdgSurface>,
    /// The XDG toplevel handle, providing window management controls.
    pub toplevel: Option<XdgToplevel>,
    /// The ID of the parent window, if this is a transient child (e.g., a dialog).
    pub parent_id: Option<ObjectId>,
    /// Current X position in global compositor coordinates.
    pub x: f64,
    /// Current Y position in global compositor coordinates.
    pub y: f64,
    /// Current width of the window surface.
    pub w: i32,
    /// Current height of the window surface.
    pub h: i32,
    /// The window geometry as defined by xdg-shell.
    /// This defines the "logical" window area, excluding shadows or other decorations.
    pub geometry: Rect,
    /// The window title string.
    pub title: Option<String>,
    /// The application identifier (used for grouping, icon selection, etc.).
    pub app_id: Option<String>,
    /// Whether server-side decorations (SSD) are enabled for this window.
    pub ssd: bool,

    /// Whether the window is currently in a maximized state.
    pub maximized: bool,
    /// Whether the window is currently in fullscreen mode.
    pub fullscreen: bool,
    /// Whether the window is currently minimized (hidden from view).
    pub minimized: bool,
    /// Stores the geometry (x, y, w, h) prior to maximization or fullscreen for restoration.
    pub saved_geometry: Option<(f64, f64, i32, i32)>,
}

/// Represents the state of a popup surface (e.g., context menus, tooltips).
///
/// Popups are always relative to a parent surface and follow xdg-popup semantics.
#[derive(Clone)]
pub struct PopupState {
    /// The base Wayland surface.
    pub surface: WlSurface,
    /// The XDG surface associated with this popup.
    pub xdg_surface: XdgSurface,
    /// The XDG popup handle.
    pub xdg_popup: XdgPopup,
    /// The ID of the parent surface this popup is positioned relative to.
    pub parent_surface_id: ObjectId,
    /// X position relative to the parent surface's top-left corner.
    pub x: i32,
    /// Y position relative to the parent surface's top-left corner.
    pub y: i32,
    /// The popup's geometry within its own surface.
    pub geometry: Rect,
}

/// Interface for a window management system.
///
/// This trait abstracts the logic for window layout, focus management,
/// and interactive operations (moving, resizing, tiling). It allows the compositor
/// to support different window management paradigms (e.g., floating vs. tiling).
pub trait WindowManager {
    /// Called when a new Wayland surface is first mapped.
    ///
    /// This initiates management of the surface and assigns initial positioning.
    fn map_window(&mut self, surface: WlSurface);

    /// Called when a managed surface is destroyed or unmapped.
    ///
    /// This removes the window from management and cleans up any associated resources.
    fn unmap_window(&mut self, surface_id: &ObjectId);

    /// Brings a window to the front and grants it focus.
    ///
    /// Returns the ID of the surface that actually received focus. This might be different
    /// if the target surface has transient children (e.g., focusing a parent redirects to its child dialog).
    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId;

    /// Associates an XDG toplevel interface with a previously mapped base surface.
    ///
    /// This transition allows the surface to support window management operations
    /// like maximization, fullscreen, and window titles.
    fn assign_toplevel(
        &mut self,
        surface_id: &ObjectId,
        toplevel: XdgToplevel,
        xdg_surface: XdgSurface,
    );

    /// Maps a new popup surface (e.g., a menu) to the display.
    fn map_popup(&mut self, popup: PopupState);

    /// Removes a popup surface from the display.
    fn unmap_popup(&mut self, popup_surface_id: &ObjectId);

    /// Updates the relative position of a popup surface.
    fn update_popup_position(&mut self, popup_surface_id: &ObjectId, x: i32, y: i32);

    /// Sets the human-readable title for a window.
    fn set_window_title(&mut self, toplevel_id: &ObjectId, title: String);

    /// Sets the application identifier for a window.
    fn set_window_app_id(&mut self, toplevel_id: &ObjectId, app_id: String);

    /// Sets the parent relationship for transient windows (e.g., dialogs).
    ///
    /// The window manager may use this to center the child over the parent
    /// or ensure the child always stays above the parent.
    fn set_window_parent(&mut self, toplevel_id: &ObjectId, parent_id: Option<ObjectId>);

    /// Enables or disables server-side decorations (SSD) for a window.
    fn set_window_ssd(&mut self, toplevel_id: &ObjectId, enabled: bool);

    /// Updates the logical geometry of a window as reported by the client.
    ///
    /// This adjustment ensures that the visible portion of the window remains stable
    /// even if the client changes the offsets of its internal buffers (e.g., for shadows).
    fn set_window_geometry(&mut self, surface_id: &ObjectId, geometry: Rect);

    /// Toggles the maximization state of a window.
    ///
    /// If maximized, the window manager should expand the window to fill the provided screen size.
    fn set_maximized(&mut self, toplevel_id: &ObjectId, maximized: bool, screen_size: (u16, u16));

    /// Toggles the fullscreen state of a window.
    fn set_fullscreen(&mut self, toplevel_id: &ObjectId, fullscreen: bool, screen_size: (u16, u16));

    /// Requests that a window be minimized (typically hidden from the workspace).
    fn set_minimized(&mut self, toplevel_id: &ObjectId);

    /// Initiates an interactive move operation (e.g., when the user drags the title bar).
    fn begin_interactive_move(
        &mut self,
        toplevel_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Initiates an interactive resize operation (e.g., dragging a window edge).
    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Low-level method to start a drag operation for a specific surface.
    fn begin_drag(
        &mut self,
        surface_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Updates the position of a window currently being dragged based on cursor movement.
    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64);

    /// Terminates the current drag operation, dropping the window at its current position.
    fn end_drag(&mut self);

    /// Updates the dimensions of a window currently being resized.
    ///
    /// Sends a configuration event to the client with the new suggested size.
    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32);

    /// Terminates the current resize operation.
    fn end_resize(&mut self);

    /// Informs the window manager that a client has committed new buffer dimensions.
    ///
    /// This should be called when the client acknowledges a configure event and commits its buffer.
    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32);

    /// Returns a list of all visible windows in back-to-front drawing order.
    fn get_render_list(&self) -> Vec<WindowState>;

    /// Returns a list of all active popup surfaces.
    fn get_popups(&self) -> Vec<PopupState>;

    /// Returns the surface that currently has input focus (the one on top).
    fn get_focused_window(&self) -> Option<WlSurface>;

    /// Calculates the absolute screen-space position of a surface, accounting for parent hierarchies.
    fn get_absolute_position(&self, surface_id: &ObjectId) -> (f64, f64);
}

/// A basic floating window manager implementation.
///
/// This window manager allows windows to be positioned anywhere on the screen,
/// supports overlapping windows with a back-to-front Z-order, and provides
/// interactive moving and resizing.
pub struct FloatingWm {
    /// List of managed windows, ordered from back to front.
    /// The last element is considered the top-most and focused window.
    pub windows: Vec<WindowState>,

    /// List of active popup surfaces.
    pub popups: Vec<PopupState>,

    /// Tracks the state of a window currently being moved.
    /// Stores: (Surface ID, Cursor Offset X, Cursor Offset Y).
    pub drag_state: Option<(ObjectId, f64, f64)>,

    /// Tracks the state of a window currently being resized.
    /// Stores: (Surface ID, Resize Edges, Start Cursor X, Start Cursor Y,
    /// Start Surface X, Start Surface Y, Start Geometry W, Start Geometry H,
    /// Start Geometry X, Start Geometry Y).
    pub resize_state: Option<(ObjectId, u32, f64, f64, f64, f64, i32, i32, i32, i32)>,
}

impl FloatingWm {
    /// Creates a new instance of the floating window manager with an empty window list.
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            popups: Vec::new(),
            drag_state: None,
            resize_state: None,
        }
    }
}

/// Implementation of the WindowManager trait for a floating layout.
impl WindowManager for FloatingWm {
    /// Maps a new window with a cascading default position.
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
            maximized: false,
            fullscreen: false,
            minimized: false,
            saved_geometry: None,
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
        if let Some((resize_id, ..)) = &self.resize_state {
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
            // According to xdg-shell spec, if the geometry offset changes,
            // we must adjust the surface position to keep the window geometry top-left corner stable.
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
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
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

            if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface) {
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

    fn set_fullscreen(
        &mut self,
        toplevel_id: &ObjectId,
        fullscreen: bool,
        screen_size: (u16, u16),
    ) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
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

            if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface) {
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

    fn set_minimized(&mut self, toplevel_id: &ObjectId) {
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            window.minimized = true;
        }
    }

    fn begin_interactive_move(
        &mut self,
        toplevel_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    ) {
        let target_surface_id = self.windows.iter().find_map(|w| {
            if let Some(top) = &w.toplevel {
                if &top.id() == toplevel_id {
                    return Some(w.surface.id());
                }
            }
            None
        });

        if let Some(surface_id) = target_surface_id {
            // Un-fullscreen and un-maximize if needed when moving
            self.set_fullscreen(&toplevel_id, false, screen_size);
            self.set_maximized(&toplevel_id, false, screen_size);

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
        if let Some(_window) = self
            .windows
            .iter_mut()
            .find(|w| w.toplevel.as_ref().map(|t| t.id()) == Some(toplevel_id.clone()))
        {
            // If the window was maximized or fullscreen, we MUST un-maximize it before we start resizing
            // otherwise our initial dimensions are the screen size, and if we then restore later
            // we'll lose our resize progress.
            self.set_fullscreen(&toplevel_id, false, screen_size);
            self.set_maximized(&toplevel_id, false, screen_size);
        }

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
                window.geometry.w,
                window.geometry.h,
                window.geometry.x,
                window.geometry.y,
            ));
            self.drag_state = None; // Mutually exclusive with resize
            self.focus_window(&window.surface.id());
        }
    }

    fn begin_drag(
        &mut self,
        surface_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    ) {
        if let Some(window) = self.windows.iter().find(|w| &w.surface.id() == surface_id) {
            if let Some(toplevel) = &window.toplevel {
                let toplevel_id = toplevel.id();
                self.set_fullscreen(&toplevel_id, false, screen_size);
                self.set_maximized(&toplevel_id, false, screen_size);
            }
        }

        if let Some(window) = self.windows.iter().find(|w| &w.surface.id() == surface_id) {
            let offset_x = cursor_x - window.x;
            let offset_y = cursor_y - window.y;
            self.drag_state = Some((surface_id.clone(), offset_x, offset_y));
            self.resize_state = None; // Mutually exclusive with drag
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
            if let Some(window) = self.windows.iter_mut().find(|w| w.surface.id() == id) {
                let dx = cursor_x - start_cx;
                let dy = cursor_y - start_cy;

                // new_gx and new_gy are the new geometry top-left in screen space
                let mut new_gx = start_sx + start_gx as f64;
                let mut new_gy = start_sy + start_gy as f64;
                let mut new_gw = start_gw as f64;
                let mut new_gh = start_gh as f64;

                if (edges & 4) != 0 {
                    // Left
                    new_gx += dx;
                    new_gw -= dx;
                } else if (edges & 8) != 0 {
                    // Right
                    new_gw += dx;
                }

                if (edges & 1) != 0 {
                    // Top
                    new_gy += dy;
                    new_gh -= dy;
                } else if (edges & 2) != 0 {
                    // Bottom
                    new_gh += dy;
                }

                if new_gw < 100.0 {
                    new_gw = 100.0;
                }
                if new_gh < 100.0 {
                    new_gh = 100.0;
                }

                // Update surface position based on new geometry position and CURRENT geometry offset
                window.x = new_gx - window.geometry.x as f64;
                window.y = new_gy - window.geometry.y as f64;

                // Configure is in window geometry coordinates
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
            if let Some(window) = self.windows.iter_mut().find(|w| w.surface.id() == id) {
                if let (Some(toplevel), Some(xdg_surface)) = (&window.toplevel, &window.xdg_surface)
                {
                    let act_val =
                        wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated
                            as u32;
                    let states_bytes = act_val.to_ne_bytes().to_vec();
                    toplevel.configure(0, 0, states_bytes);
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
            return (win.x + win.geometry.x as f64, win.y + win.geometry.y as f64);
        }

        if let Some(popup) = self.popups.iter().find(|p| &p.surface.id() == surface_id) {
            let (parent_abs_x, parent_abs_y) = self.get_absolute_position(&popup.parent_surface_id);
            return (parent_abs_x + popup.x as f64, parent_abs_y + popup.y as f64);
        }

        (0.0, 0.0)
    }
}

use wayland_protocols::xdg::shell::server::xdg_popup::XdgPopup;
use wayland_protocols::xdg::shell::server::xdg_surface::XdgSurface;
use wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Debug)]
pub struct ConfigureState {
    pub serial: u32,
    pub maximized: bool,
    pub fullscreen: bool,
    pub resizing: bool,
    pub edges: u32,
    pub w: i32,
    pub h: i32,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

/// Represents the state of a single window (toplevel or layer shell surface) in the compositor.
#[derive(Clone)]
pub struct WindowState {
    pub surface: WlSurface,
    pub xdg_surface: Option<XdgSurface>,
    pub toplevel: Option<XdgToplevel>,
    pub layer_surface: Option<
        wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    >,
    pub parent_id: Option<ObjectId>,
    pub x: f64,
    pub y: f64,
    pub w: i32,
    pub h: i32,
    pub geometry: Rect,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub ssd: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub minimized: bool,
    pub modal: bool,
    pub saved_geometry: Option<(f64, f64, i32, i32)>,

    // Layer Shell properties
    pub layer: u32,
    pub anchor: u32,
    pub exclusive_zone: i32,
    pub margin: (i32, i32, i32, i32), // top, right, bottom, left
    pub keyboard_interactivity: u32,

    pub is_interacting: bool,

    pub sent_configures: Vec<ConfigureState>,
    pub acknowledged_serial: Option<u32>,
}

#[derive(Clone)]
pub struct PopupState {
    pub surface: WlSurface,
    pub xdg_surface: XdgSurface,
    pub xdg_popup: XdgPopup,
    pub parent_surface_id: ObjectId,
    pub x: i32,
    pub y: i32,
    pub geometry: Rect,
}

#[derive(Clone)]
pub struct DisplayLayerState {
    pub windows: Vec<WindowState>,
}

impl DisplayLayerState {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Workspace {
    pub id: usize,
    pub windows: Vec<WindowState>,
}

impl Workspace {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            windows: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct OutputState {
    pub id: usize,
    pub workspaces: SlotVec<Workspace>,
    pub active_workspace: usize,
    pub background: DisplayLayerState,
    pub bottom: DisplayLayerState,
    pub top: DisplayLayerState,
    pub overlay: DisplayLayerState,
    pub usable_area: Rect,
}

impl OutputState {
    pub fn new(id: usize) -> Self {
        let mut wx = SlotVec::new(10);
        for i in 0..10 {
            *wx.get_mut(i).unwrap() = Slot::Occupied(Workspace::new(i));
        }
        Self {
            id,
            workspaces: wx,
            active_workspace: 0,
            background: DisplayLayerState::new(),
            bottom: DisplayLayerState::new(),
            top: DisplayLayerState::new(),
            overlay: DisplayLayerState::new(),
            usable_area: Rect::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum WorkspaceInsertPosition {
    After(usize),
    Before(usize),
}

/// A trait defining the core operations a window manager must provide to the compositor.
/// This includes mapping/unmapping surfaces, focusing, resizing, and scene graph queries.
pub trait WindowManager {
    /// Maps a new regular window (toplevel) into the scene graph.
    fn map_window(&mut self, surface: WlSurface);

    /// Unmaps and removes a window from the scene graph.
    fn unmap_window(&mut self, surface_id: &ObjectId);

    /// Focuses the specified window, bringing it to the front of its layer if applicable.
    /// Returns the ID of the surface that actually received focus (which might be a child).
    fn focus_window(&mut self, surface_id: &ObjectId) -> ObjectId;

    /// Assigns XDG toplevel role information to an already mapped window.
    fn assign_toplevel(
        &mut self,
        surface_id: &ObjectId,
        toplevel: XdgToplevel,
        xdg_surface: XdgSurface,
    );

    /// Maps a new popup surface (e.g., context menu, tooltip) into the scene graph.
    fn map_popup(&mut self, popup: PopupState);

    /// Unmaps and removes a popup surface from the scene graph.
    fn unmap_popup(&mut self, popup_surface_id: &ObjectId);

    /// Updates the position of an existing popup surface relative to its parent.
    fn update_popup_position(&mut self, popup_surface_id: &ObjectId, x: i32, y: i32);

    /// Sets the window title for the specified toplevel.
    fn set_window_title(&mut self, toplevel_id: &ObjectId, title: String);

    /// Sets the application ID for the specified toplevel.
    fn set_window_app_id(&mut self, toplevel_id: &ObjectId, app_id: String);

    /// Sets the parent of the specified toplevel.
    fn set_window_parent(&mut self, toplevel_id: &ObjectId, parent_id: Option<ObjectId>);

    /// Enables or disables server-side decorations (SSD) for the specified toplevel.
    fn set_window_ssd(&mut self, toplevel_id: &ObjectId, enabled: bool);

    /// Updates the logical geometry (bounds) of the specified surface.
    fn set_window_geometry(&mut self, surface_id: &ObjectId, geometry: Rect);

    /// Sets the maximized state for the specified toplevel.
    fn set_maximized(
        &mut self,
        toplevel_id: &ObjectId,
        maximized: bool,
        screen_size: (u16, u16),
        serial: u32,
    );

    /// Sets the fullscreen state for the specified toplevel.
    fn set_fullscreen(
        &mut self,
        toplevel_id: &ObjectId,
        fullscreen: bool,
        screen_size: (u16, u16),
        serial: u32,
    );

    /// Minimizes the specified toplevel window.
    fn set_minimized(&mut self, toplevel_id: &ObjectId) -> Option<ObjectId>;

    /// Sets or unsets the modal state for the specified toplevel.
    fn set_modal(&mut self, toplevel_id: &ObjectId, modal: bool);

    /// Begins an interactive move operation for the specified toplevel.
    fn begin_interactive_move(
        &mut self,
        toplevel_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
        serial: u32,
    );

    /// Begins an interactive resize operation for the specified toplevel.
    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
        serial: u32,
    );

    /// Begins a drag operation for the specified surface.
    fn begin_drag(
        &mut self,
        surface_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
        serial: u32,
    );

    /// Updates the current drag position.
    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64);

    /// Ends the current drag operation.
    fn end_drag(&mut self);

    /// Updates the current resize operation with the new cursor position.
    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32);

    /// Ends the current resize operation.
    fn end_resize(&mut self, serial: u32);

    /// Refreshes the internal window dimensions, usually in response to a client configure ack.
    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32);

    /// Acknowledges a configuration serial for a window.
    fn ack_configure(&mut self, surface_id: &ObjectId, serial: u32);

    /// Applies the acknowledged configure state on surface commit.
    fn apply_committed_configure(&mut self, surface_id: &ObjectId, actual_w: i32, actual_h: i32);

    // Layer Shell Management

    /// Maps a layer shell surface into the specified Z-order layer.
    fn map_layer_surface(
        &mut self,
        surface: WlSurface,
        layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        layer: u32,
        namesapce: String,
    );

    /// Sets the size of a layer shell surface.
    fn set_layer_surface_size(&mut self, surface_id: &ObjectId, w: u32, h: u32);

    /// Sets the anchor (edge alignments) for a layer shell surface.
    fn set_layer_surface_anchor(&mut self, surface_id: &ObjectId, anchor: u32);

    /// Sets the exclusive zone for a layer shell surface, indicating reserved screen edge space.
    fn set_layer_surface_zone(&mut self, surface_id: &ObjectId, zone: i32);

    /// Sets the margins for a layer shell surface.
    fn set_layer_surface_margin(
        &mut self,
        surface_id: &ObjectId,
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    );

    /// Sets the keyboard interactivity mode for a layer shell surface.
    fn set_layer_keyboard_interactivity(&mut self, surface_id: &ObjectId, interactivity: u32);

    /// Recalculates the layout of all layer surfaces on the screen based on their anchors, margins, and exclusive zones.
    fn recalculate_layer_layout(&mut self, screen_size: (u16, u16), serial: u32);

    // Scene Graph Queries

    /// Retrieves all windows currently managed, flattened into a single list.
    fn all_windows(&self) -> Vec<WindowState>;

    /// Retrieves the windows in the Background layer.
    fn get_background(&self) -> Vec<WindowState>;

    /// Retrieves the windows in the Bottom layer.
    fn get_bottom(&self) -> Vec<WindowState>;

    /// Retrieves the windows in the active workspace.
    fn get_workspace_windows(&self) -> Vec<WindowState>;

    /// Retrieves the windows in the Top layer.
    fn get_top(&self) -> Vec<WindowState>;

    /// Retrieves the windows in the Overlay layer.
    fn get_overlay(&self) -> Vec<WindowState>;

    /// Retrieves all mapped popups.
    fn get_popups(&self) -> Vec<PopupState>;

    /// Retrieves the currently focused surface, if any.
    fn get_focused_window(&self) -> Option<WlSurface>;

    /// Calculates and returns the absolute global position (x, y) of the specified surface's origin.
    fn get_surface_position(&self, surface_id: &ObjectId) -> Option<(f64, f64)>;

    /// Calculates and returns the absolute global position (x, y) of the specified surface's logical window origin.
    fn get_absolute_position(&self, surface_id: &ObjectId) -> (f64, f64);

    // Workspaces

    /// Create a new workspace by specifying where to insert it
    fn create_workspace(
        &mut self,
        output_id: usize,
        insert_position: WorkspaceInsertPosition,
    ) -> Option<usize>;

    /// Delete a workspace
    /// Return a boolean to confirm the deletion success
    fn delete_workspace(&mut self, output_id: usize, id: usize) -> bool;

    /// Move a window to a specific workspace on a specific output.
    /// Returns true if successful, false otherwise.
    fn move_window_to_workspace(
        &mut self,
        surface_id: &ObjectId,
        output_id: usize,
        workspace_id: usize,
    ) -> bool;

    /// Focus the workspace before the current workspace
    fn focus_before_workspace(&mut self) -> bool;
    /// Focus the workspace after the current workspace
    fn focus_after_workspace(&mut self) -> bool;
    /// Focus a specific workspace by ID
    fn focus_workspace(&mut self, id: usize) -> bool;

    /// Begin workspace swiping gesture
    fn begin_workspace_swipe(&mut self);
    /// Update workspace swiping gesture progress
    fn update_workspace_swipe(&mut self, dx: f64);
    /// End workspace swiping gesture
    fn end_workspace_swipe(&mut self, threshold: f64);
    /// Get the current horizontal offset of the workspace in pixels
    fn get_workspace_offset(&self) -> f64;
    /// Check if workspace is currently swiping
    fn is_workspace_swiping(&self) -> bool;
    /// Get the active workspace index
    fn get_active_workspace(&self) -> usize;
    /// Get all windows of a specific workspace
    fn get_workspace_windows_by_id(&self, workspace_id: usize) -> Vec<WindowState>;
    /// Get the workspace ID of a window
    fn get_workspace_id_for_window(&self, surface_id: &ObjectId) -> Option<usize>;
    /// Check if resizing is in progress
    fn is_resizing(&self) -> bool;
    /// Check if dragging is in progress
    fn is_dragging(&self) -> bool;

    /// Check if workspace compaction shifted the active workspace in this frame
    fn take_compaction_occurred(&self) -> bool {
        false
    }
}

#[derive(Clone, PartialEq)]
pub enum Slot<T> {
    Empty,
    Occupied(T),
}

impl<T> Slot<T> {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn unwrap_ref(&self) -> &T {
        match self {
            Slot::Empty => panic!("Unwrap on a empty slot"),
            Slot::Occupied(v) => v,
        }
    }

    pub fn unwrap_mut(&mut self) -> &mut T {
        match self {
            Slot::Empty => panic!("Unwrap on a empty slot"),
            Slot::Occupied(v) => v,
        }
    }
}

#[derive(Clone)]
pub struct SlotVec<T> {
    inner: Vec<Slot<T>>,
}

impl<T> SlotVec<T>
where
    T: Clone,
{
    pub fn new(slot_count: usize) -> Self {
        Self {
            inner: vec![Slot::Empty::<T>; slot_count],
        }
    }

    pub fn get(&self, index: usize) -> Option<&Slot<T>> {
        if index <= self.inner.len() - 1 {
            return Some(&self.inner[index]);
        }

        return None;
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Slot<T>> {
        if index <= self.inner.len() - 1 {
            return Some(&mut self.inner[index]);
        }

        if index == self.inner.len() {
            self.inner.push(Slot::Empty);
            return Some(&mut self.inner[index]);
        }

        if index > self.inner.len() {
            let extended_with = vec![Slot::Empty::<T>; index - self.inner.len() + 1];
            self.inner.extend(extended_with);
            return Some(&mut self.inner[index]);
        }

        return None;
    }

    pub fn insert_after(&mut self, index: usize, val: T) -> bool {
        if self.get_mut(index + 1).unwrap().is_empty() {
            self.inner[index + 1] = Slot::Occupied(val);
            return true;
        } else {
            return false;
        }
    }

    pub fn insert_before(&mut self, index: usize, val: T) -> bool {
        if self.get_mut(index - 1).unwrap().is_empty() {
            self.inner[index - 1] = Slot::Occupied(val);
            return true;
        } else {
            return false;
        }
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if !self.get(index).is_some() {
            self.inner[index] = Slot::Empty;
            return true;
        }

        return false;
    }

    pub fn flatten(&self) -> Vec<&T> {
        self.inner
            .iter()
            .filter_map(|x| match x {
                Slot::Empty => None,
                Slot::Occupied(val) => Some(val),
            })
            .collect()
    }

    pub fn flatten_mut(&mut self) -> Vec<&mut T> {
        self.inner
            .iter_mut()
            .filter_map(|x| match x {
                Slot::Empty => None,
                Slot::Occupied(val) => Some(val),
            })
            .collect()
    }
}

pub mod impls;

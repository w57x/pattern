use tracing::debug;
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
pub struct LayerState {
    pub windows: Vec<WindowState>,
}

impl LayerState {
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
    pub background: LayerState,
    pub bottom: LayerState,
    pub top: LayerState,
    pub overlay: LayerState,
}

impl OutputState {
    pub fn new(id: usize) -> Self {
        let mut wx = SlotVec::new(10);
        wx.insert_before(1, Workspace::new(0));
        Self {
            id,
            workspaces: wx,
            active_workspace: 0,
            background: LayerState::new(),
            bottom: LayerState::new(),
            top: LayerState::new(),
            overlay: LayerState::new(),
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
    fn set_maximized(&mut self, toplevel_id: &ObjectId, maximized: bool, screen_size: (u16, u16));

    /// Sets the fullscreen state for the specified toplevel.
    fn set_fullscreen(&mut self, toplevel_id: &ObjectId, fullscreen: bool, screen_size: (u16, u16));

    /// Minimizes the specified toplevel window.
    fn set_minimized(&mut self, toplevel_id: &ObjectId);

    /// Sets or unsets the modal state for the specified toplevel.
    fn set_modal(&mut self, toplevel_id: &ObjectId, modal: bool);

    /// Begins an interactive move operation for the specified toplevel.
    fn begin_interactive_move(
        &mut self,
        toplevel_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Begins an interactive resize operation for the specified toplevel.
    fn begin_interactive_resize(
        &mut self,
        toplevel_id: &ObjectId,
        edges: u32,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Begins a drag operation for the specified surface.
    fn begin_drag(
        &mut self,
        surface_id: &ObjectId,
        cursor_x: f64,
        cursor_y: f64,
        screen_size: (u16, u16),
    );

    /// Updates the current drag position.
    fn update_drag(&mut self, cursor_x: f64, cursor_y: f64);

    /// Ends the current drag operation.
    fn end_drag(&mut self);

    /// Updates the current resize operation with the new cursor position.
    fn update_resize(&mut self, cursor_x: f64, cursor_y: f64, serial: u32);

    /// Ends the current resize operation.
    fn end_resize(&mut self);

    /// Refreshes the internal window dimensions, usually in response to a client configure ack.
    fn refresh_window_dimensions(&mut self, surface_id: &ObjectId, w: i32, h: i32);

    // Layer Shell Management

    /// Maps a layer shell surface into the specified Z-order layer.
    fn map_layer_surface(
        &mut self,
        surface: WlSurface,
        layer_surface: wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        layer: u32,
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
    fn recalculate_layer_layout(&mut self, screen_size: (u16, u16));

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
            let extended_with = vec![Slot::Empty::<T>; index - self.inner.len()];
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

pub struct FloatingWm {
    pub outputs: Vec<OutputState>,
    pub popups: Vec<PopupState>,
    pub drag_state: Option<(ObjectId, f64, f64)>,
    pub resize_state: Option<(ObjectId, u32, f64, f64, f64, f64, i32, i32, i32, i32)>,
}

impl FloatingWm {
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
        list
    }
}

impl WindowManager for FloatingWm {
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
        list
    }

    /// Maps a new window with a cascading default position.
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
        layer_surface: wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        layer: u32,
    ) {
        debug!(
            "Mapping layer surface: id={:?}, layer={}",
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
        self.all_windows().last().map(|w| w.surface.clone())
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
        // 1. Find and remove the window from its current location
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
}

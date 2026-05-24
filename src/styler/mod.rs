use crate::server::SubsurfaceData;
use crate::vulkan::{DrawCommand, RenderQuad, SurfaceTexture};
use std::collections::HashMap;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;
pub mod style;
use crate::animation::{AnimatedValue, tree::AnimationTree};
use crate::styler::style::StyleConfig;

pub struct HitResult {
    pub surface: Option<WlSurface>,
    pub local_x: f64,
    pub local_y: f64,
}

pub struct AnimatedWindow {
    pub surface_id: ObjectId,
    pub x: AnimatedValue,
    pub y: AnimatedValue,
    pub w: AnimatedValue,
    pub h: AnimatedValue,
    pub alpha: AnimatedValue,
    pub scale: AnimatedValue,
    pub is_closing: bool,
    pub last_seen: u64,
    pub is_ssd: bool,
    pub texture_snapshot: Option<SurfaceTexture>,
    pub render_layer: u32,
    pub workspace_id: Option<usize>,
}

impl AnimatedWindow {
    pub fn new(
        id: ObjectId,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        is_ssd: bool,
        render_layer: u32,
    ) -> Self {
        Self {
            surface_id: id,
            x: AnimatedValue::new(x),
            y: AnimatedValue::new(y),
            w: AnimatedValue::new(w),
            h: AnimatedValue::new(h),
            alpha: AnimatedValue::new(0.0), // Start transparent
            scale: AnimatedValue::new(0.9), // Start slightly scaled down
            is_closing: false,
            last_seen: 0,
            is_ssd,
            texture_snapshot: None,
            render_layer,
            workspace_id: None,
        }
    }
}

/// A trait that defines how surfaces are rendered and how hit-testing is performed.
///
/// The `Styler` is responsible for translating the window manager's state into
/// concrete drawing commands for the renderer, and for mapping global cursor
/// coordinates back to specific surfaces.
pub trait Styler {
    /// Advance animations and synchronize logical state with visual state
    fn tick(
        &mut self,
        now_ms: f64,
        wm: &dyn crate::wm::WindowManager,
        textures: &HashMap<ObjectId, SurfaceTexture>,
        screen_size: (u16, u16),
    ) -> bool;

    /// Generates a list of draw commands for the current frame.
    ///
    /// This method takes the complete state of the compositor and produces
    /// the necessary commands to render the entire scene.
    fn generate_draw_list(
        &self,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        opaque_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
        screen_size: (u16, u16),
    ) -> Vec<DrawCommand>;

    /// Performs a hit-test to find the surface under the cursor.
    ///
    /// This should account for window Z-order, subsurfaces, and input regions.
    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        input_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
        extra_hit_surfaces: &[(WlSurface, f64, f64)],
    ) -> HitResult;

    /// Draws a surface and all its subsurfaces at the given global position.
    fn draw_surface_tree(
        &self,
        surface: &WlSurface,
        abs_x: f64,
        abs_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        draw_list: &mut Vec<DrawCommand>,
    );

    /// Returns whether this styler supports server-side decorations (SSD).
    fn supports_ssd(&self) -> bool {
        false
    }

    /// Returns the number of Kawase passes configured.
    fn blur_passes(&self) -> u32 {
        0
    }

    /// Returns the workspace offset for a given surface.
    fn get_workspace_offset_for_surface(
        &self,
        _surface_id: &ObjectId,
        _wm: &dyn crate::wm::WindowManager,
    ) -> f64 {
        0.0
    }
}

pub struct DefaultStyler {
    pub windows: HashMap<ObjectId, AnimatedWindow>,
    pub frame_counter: u64,
    pub config: AnimationTree,
    pub style: StyleConfig,
    pub workspace_offset: crate::animation::AnimatedValue,
    pub active_workspace: usize,
    pub prev_active_workspace: Option<usize>,
    pub is_swiping: bool,
    pub screen_size: (u16, u16),
}

impl DefaultStyler {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            frame_counter: 0,
            config: AnimationTree::default(),
            style: StyleConfig::default(),
            workspace_offset: crate::animation::AnimatedValue::new(0.0),
            active_workspace: 0,
            prev_active_workspace: None,
            is_swiping: false,
            screen_size: (1920, 1080),
        }
    }

    fn get_surface_size(
        &self,
        surface_id: &ObjectId,
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
    ) -> (f64, f64) {
        if let Some(vp_id) = surface_to_viewport.get(surface_id) {
            if let Some((_, Some(dst))) = viewports.get(vp_id) {
                return (dst.0 as f64, dst.1 as f64);
            }
        }
        if let Some(tex) = textures.get(surface_id) {
            return (
                tex.w as f64 / tex.scale as f64,
                tex.h as f64 / tex.scale as f64,
            );
        }
        (0.0, 0.0)
    }

    fn get_workspace_offset_for_surface(
        &self,
        surface_id: &ObjectId,
        wm: &dyn crate::wm::WindowManager,
    ) -> f64 {
        let mut target_id = surface_id.clone();
        let screen_w = self.screen_size.0 as f64;
        loop {
            if let Some(anim_win) = self.windows.get(&target_id) {
                if let Some(ws_id) = anim_win.workspace_id {
                    let active_ws = wm.get_active_workspace();
                    let ws_offset = self.workspace_offset.current;
                    let factor = 1.3; // Parallax factor to prevent adjacent workspaces peeking in
                    if ws_id == active_ws {
                        return ws_offset;
                    } else if ws_id < active_ws {
                        return (ws_offset - screen_w * (active_ws - ws_id) as f64) * factor;
                    } else {
                        return (ws_offset + screen_w * (ws_id - active_ws) as f64) * factor;
                    }
                }
                return 0.0;
            }
            if let Some(popup) = wm
                .get_popups()
                .iter()
                .find(|p| &p.surface.id() == &target_id)
            {
                target_id = popup.parent_surface_id.clone();
            } else {
                break;
            }
        }
        0.0
    }

    fn draw_surface_recursive(
        &self,
        surface: &WlSurface,
        abs_x: f64,
        abs_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        draw_list: &mut Vec<DrawCommand>,
        border_radius: f32,
        alpha: f32,
        scale: f32,
        is_base: bool, // newly added parameter
        is_ssd: bool,  // parameter to prevent shadowing CSD windows
    ) {
        let (mut lw, mut lh) =
            self.get_surface_size(&surface.id(), textures, viewports, surface_to_viewport);

        lw *= scale as f64;
        lh *= scale as f64;

        if let Some(tex) = textures.get(&surface.id()) {
            let mut src_x = 0.0;
            let mut src_y = 0.0;
            let mut src_w = 1.0;
            let mut src_h = 1.0;

            if let Some(vp_id) = surface_to_viewport.get(&surface.id()) {
                if let Some((Some(src), _)) = viewports.get(vp_id) {
                    src_x = (src.0 / tex.w as f64) as f32;
                    src_y = (src.1 / tex.h as f64) as f32;
                    src_w = (src.2 / tex.w as f64) as f32;
                    src_h = (src.3 / tex.h as f64) as f32;
                }
            }

            if is_base && self.style.blur.enabled && is_ssd {
                draw_list.push(DrawCommand::BlurCapture);
                draw_list.push(DrawCommand::Blur(RenderQuad {
                    set: tex.set, // unused by blur pipeline
                    x: abs_x.round() as f32,
                    y: abs_y.round() as f32,
                    w: lw.round() as f32,
                    h: lh.round() as f32,
                    src_x,
                    src_y,
                    src_w,
                    src_h,
                    border_radius,
                    alpha,
                }));
            }

            if is_base && is_ssd && self.style.shadow.enabled {
                let shadow_size = self.style.shadow.range as f32 * scale;
                // Offset the shadow based on config and apply scale
                let shadow_x =
                    abs_x as f32 + (self.style.shadow.offset.0 as f32 * scale) - shadow_size;
                let shadow_y =
                    abs_y as f32 + (self.style.shadow.offset.1 as f32 * scale) - shadow_size;

                let shadow_w = lw.round() as f32 + (shadow_size * 2.0);
                let shadow_h = lh.round() as f32 + (shadow_size * 2.0);

                draw_list.push(DrawCommand::Shadow(crate::vulkan::ShadowQuad {
                    x: shadow_x.round(),
                    y: shadow_y.round(),
                    w: shadow_w.round(),
                    h: shadow_h.round(),
                    border_radius,
                    spread: shadow_size,
                    power: self.style.shadow.render_power as f32,
                    alpha,
                    color: self.style.shadow.color,
                }));
            }

            draw_list.push(DrawCommand::Texture(RenderQuad {
                set: tex.set,
                x: abs_x.round() as f32,
                y: abs_y.round() as f32,
                w: lw.round() as f32,
                h: lh.round() as f32,
                src_x,
                src_y,
                src_w,
                src_h,
                border_radius,
                alpha,
            }));
        }

        for sub in subsurfaces {
            if sub.parent.id() == surface.id() {
                self.draw_surface_recursive(
                    &sub.surface,
                    abs_x + sub.x as f64 * scale as f64,
                    abs_y + sub.y as f64 * scale as f64,
                    subsurfaces,
                    textures,
                    viewports,
                    surface_to_viewport,
                    draw_list,
                    0.0,
                    alpha,
                    scale,
                    false, // subsurfaces are not base
                    false, // subsurfaces are not ssd
                );
            }
        }
    }

    /// Recursively checks if the cursor is over a surface or any of its subsurfaces.
    fn hit_test_recursive(
        &self,
        surface: &WlSurface,
        abs_x: f64,
        abs_y: f64,
        cursor_x: f64,
        cursor_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        input_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
    ) -> Option<HitResult> {
        for sub in subsurfaces.iter().rev() {
            if sub.parent.id() == surface.id() {
                if let Some(hit) = self.hit_test_recursive(
                    &sub.surface,
                    abs_x + sub.x as f64,
                    abs_y + sub.y as f64,
                    cursor_x,
                    cursor_y,
                    subsurfaces,
                    textures,
                    viewports,
                    surface_to_viewport,
                    input_regions,
                ) {
                    return Some(hit);
                }
            }
        }

        let (lw, lh) =
            self.get_surface_size(&surface.id(), textures, viewports, surface_to_viewport);

        if cursor_x >= abs_x
            && cursor_x <= abs_x + lw
            && cursor_y >= abs_y
            && cursor_y <= abs_y + lh
        {
            let local_x = cursor_x - abs_x;
            let local_y = cursor_y - abs_y;

            if let Some(rects) = input_regions.get(&surface.id()) {
                let hit_region = rects.iter().any(|r| {
                    local_x >= r.x as f64
                        && local_x <= (r.x + r.w) as f64
                        && local_y >= r.y as f64
                        && local_y <= (r.y + r.h) as f64
                });
                if !hit_region {
                    return None;
                }
            }

            return Some(HitResult {
                surface: Some(surface.clone()),
                local_x,
                local_y,
            });
        }
        None
    }

    fn get_visible_logical_windows(
        &self,
        wm: &dyn crate::wm::WindowManager,
    ) -> Vec<crate::wm::WindowState> {
        let mut visible_ids = std::collections::HashSet::new();

        for w in wm.get_background() {
            visible_ids.insert(w.surface.id());
        }
        for w in wm.get_bottom() {
            visible_ids.insert(w.surface.id());
        }

        let active_ws = wm.get_active_workspace();
        for w in wm.get_workspace_windows_by_id(active_ws) {
            visible_ids.insert(w.surface.id());
        }

        // Only load and render adjacent workspaces if we are swiping or transitioning
        if self.is_swiping || self.workspace_offset.current.abs() > 0.001 {
            if active_ws > 0 {
                for w in wm.get_workspace_windows_by_id(active_ws - 1) {
                    visible_ids.insert(w.surface.id());
                }
            }
            for w in wm.get_workspace_windows_by_id(active_ws + 1) {
                visible_ids.insert(w.surface.id());
            }
        }

        for w in wm.get_top() {
            visible_ids.insert(w.surface.id());
        }
        for w in wm.get_overlay() {
            visible_ids.insert(w.surface.id());
        }

        // We filter the complete list to preserve the `is_interacting` flag set by all_windows()
        wm.all_windows()
            .into_iter()
            .filter(|w| visible_ids.contains(&w.surface.id()) && !w.minimized)
            .collect()
    }
}

impl Styler for DefaultStyler {
    fn get_workspace_offset_for_surface(
        &self,
        surface_id: &ObjectId,
        wm: &dyn crate::wm::WindowManager,
    ) -> f64 {
        self.get_workspace_offset_for_surface(surface_id, wm)
    }

    fn tick(
        &mut self,
        now_ms: f64,
        wm: &dyn crate::wm::WindowManager,
        textures: &HashMap<ObjectId, SurfaceTexture>,
        screen_size: (u16, u16),
    ) -> bool {
        let mut animating = false;
        self.frame_counter += 1;
        self.screen_size = screen_size;

        // 0. Update workspace transition animations
        let wm_active_ws = wm.get_active_workspace();
        let wm_is_swiping = wm.is_workspace_swiping();
        let screen_w = screen_size.0 as f64;

        if wm_is_swiping {
            self.workspace_offset.current = wm.get_workspace_offset();
            self.workspace_offset.target = wm.get_workspace_offset();
            self.workspace_offset.duration = 0.0;
            self.is_swiping = true;

            // Show adjacent workspace during swipe
            if self.workspace_offset.current > 0.0 {
                self.prev_active_workspace = if self.active_workspace > 0 {
                    Some(self.active_workspace - 1)
                } else {
                    None
                };
            } else if self.workspace_offset.current < 0.0 {
                self.prev_active_workspace = Some(self.active_workspace + 1);
            } else {
                self.prev_active_workspace = None;
            }
        } else if self.is_swiping {
            self.is_swiping = false;

            if wm_active_ws != self.active_workspace {
                if wm_active_ws > self.active_workspace {
                    self.workspace_offset.current = screen_w + self.workspace_offset.current;
                } else {
                    self.workspace_offset.current = -screen_w + self.workspace_offset.current;
                }
                self.prev_active_workspace = Some(self.active_workspace);
                self.active_workspace = wm_active_ws;

                self.workspace_offset.start = self.workspace_offset.current;
                self.workspace_offset.set_target(
                    0.0,
                    now_ms,
                    self.config.workspaces_in.duration,
                    self.config.workspaces_in.curve,
                );
            } else {
                self.workspace_offset.start = self.workspace_offset.current;
                self.workspace_offset.set_target(
                    0.0,
                    now_ms,
                    self.config.workspaces_out.duration,
                    self.config.workspaces_out.curve,
                );
            }
        } else {
            if self.active_workspace != wm_active_ws {
                let slide_in_offset = if wm_active_ws > self.active_workspace {
                    screen_w
                } else {
                    -screen_w
                };

                self.prev_active_workspace = Some(self.active_workspace);
                self.active_workspace = wm_active_ws;

                self.workspace_offset.current = slide_in_offset;
                self.workspace_offset.start = slide_in_offset;
                self.workspace_offset.set_target(
                    0.0,
                    now_ms,
                    self.config.workspaces_in.duration,
                    self.config.workspaces_in.curve,
                );
            }
        }

        if self.workspace_offset.tick(now_ms) {
            animating = true;
        } else {
            if !self.is_swiping {
                self.prev_active_workspace = None;
            }
        }

        let all_logical_windows = self.get_visible_logical_windows(wm);

        // 1. Reconciliation: Map WM state into Styler state
        for logical_win in all_logical_windows {
            let id = logical_win.surface.id();
            let is_ssd = logical_win.ssd && logical_win.layer_surface.is_none();
            let render_layer = if logical_win.layer_surface.is_none() {
                2 // workspace
            } else {
                match logical_win.layer {
                    0 => 0, // background
                    1 => 1, // bottom
                    2 => 3, // top
                    3 => 4, // overlay
                    _ => 3,
                }
            };

            if let Some(anim_win) = self.windows.get_mut(&id) {
                anim_win.last_seen = self.frame_counter;
                anim_win.is_ssd = is_ssd;
                anim_win.render_layer = render_layer;

                if render_layer == 2 {
                    if let Some(ws_id) = wm.get_workspace_id_for_window(&id) {
                        anim_win.workspace_id = Some(ws_id);
                    }
                }

                if let Some(tex) = textures.get(&id) {
                    anim_win.texture_snapshot = Some(tex.clone());
                }

                let move_cfg = &self.config.windows_move;
                if move_cfg.enabled && !logical_win.is_interacting {
                    anim_win
                        .x
                        .set_target(logical_win.x, now_ms, move_cfg.duration, move_cfg.curve);
                    anim_win
                        .y
                        .set_target(logical_win.y, now_ms, move_cfg.duration, move_cfg.curve);
                    anim_win.w.set_target(
                        logical_win.w as f64,
                        now_ms,
                        move_cfg.duration,
                        move_cfg.curve,
                    );
                    anim_win.h.set_target(
                        logical_win.h as f64,
                        now_ms,
                        move_cfg.duration,
                        move_cfg.curve,
                    );
                } else {
                    anim_win
                        .x
                        .set_target(logical_win.x, now_ms, 0.0, move_cfg.curve);
                    anim_win
                        .y
                        .set_target(logical_win.y, now_ms, 0.0, move_cfg.curve);
                    anim_win
                        .w
                        .set_target(logical_win.w as f64, now_ms, 0.0, move_cfg.curve);
                    anim_win
                        .h
                        .set_target(logical_win.h as f64, now_ms, 0.0, move_cfg.curve);
                }

                // If it was closing but reappeared (rare but possible), cancel close
                if anim_win.is_closing {
                    anim_win.is_closing = false;
                    let in_cfg = &self.config.windows_in;
                    if in_cfg.enabled {
                        anim_win.alpha.set_target(
                            1.0,
                            now_ms,
                            self.config.fade_in.duration,
                            self.config.fade_in.curve,
                        );
                        anim_win
                            .scale
                            .set_target(1.0, now_ms, in_cfg.duration, in_cfg.curve);
                    }
                }
            } else {
                // New window! Pop-in animation.
                let mut new_anim_win = AnimatedWindow::new(
                    id.clone(),
                    logical_win.x,
                    logical_win.y,
                    logical_win.w as f64,
                    logical_win.h as f64,
                    is_ssd,
                    render_layer,
                );
                new_anim_win.last_seen = self.frame_counter;
                if let Some(tex) = textures.get(&id) {
                    new_anim_win.texture_snapshot = Some(tex.clone());
                }
                if render_layer == 2 {
                    new_anim_win.workspace_id = wm.get_workspace_id_for_window(&id);
                }

                let in_cfg = &self.config.windows_in;
                if in_cfg.enabled {
                    if in_cfg.style_name == "popin" {
                        new_anim_win.scale.current = 0.8;
                        new_anim_win.scale.start = 0.8;
                    }
                    new_anim_win.alpha.set_target(
                        1.0,
                        now_ms,
                        self.config.fade_in.duration,
                        self.config.fade_in.curve,
                    );
                    new_anim_win
                        .scale
                        .set_target(1.0, now_ms, in_cfg.duration, in_cfg.curve);
                } else {
                    new_anim_win.alpha.current = 1.0;
                    new_anim_win.alpha.target = 1.0;
                    new_anim_win.scale.current = 1.0;
                    new_anim_win.scale.target = 1.0;
                }

                self.windows.insert(id, new_anim_win);
            }
        }

        // 2. Identify closed windows and trigger fade-out
        for anim_win in self.windows.values_mut() {
            if anim_win.last_seen != self.frame_counter && !anim_win.is_closing {
                anim_win.is_closing = true;
                let out_cfg = &self.config.windows_out;

                if out_cfg.enabled {
                    anim_win.alpha.set_target(
                        0.0,
                        now_ms,
                        self.config.fade_out.duration,
                        self.config.fade_out.curve,
                    );
                    if out_cfg.style_name == "popin" {
                        anim_win
                            .scale
                            .set_target(0.8, now_ms, out_cfg.duration, out_cfg.curve);
                    }
                } else {
                    anim_win
                        .alpha
                        .set_target(0.0, now_ms, 0.0, self.config.fade_out.curve);
                }
            }
        }

        // 3. Tick all animations
        self.windows.retain(|_, anim_win| {
            let mut keep_alive = true;

            animating |= anim_win.x.tick(now_ms);
            animating |= anim_win.y.tick(now_ms);
            animating |= anim_win.w.tick(now_ms);
            animating |= anim_win.h.tick(now_ms);
            animating |= anim_win.alpha.tick(now_ms);
            animating |= anim_win.scale.tick(now_ms);

            if anim_win.is_closing && !animating && anim_win.alpha.current <= 0.01 {
                keep_alive = false; // Dead and invisible
            }

            keep_alive
        });

        animating
    }

    fn generate_draw_list(
        &self,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        _opaque_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
        _screen_size: (u16, u16),
    ) -> Vec<DrawCommand> {
        let mut draw_list = Vec::new();

        // 1. Draw all active and closing windows from our animated state
        let all_logical = self.get_visible_logical_windows(wm);
        let mut render_order: Vec<ObjectId> = all_logical.iter().map(|w| w.surface.id()).collect();

        // Append closing windows
        for id in self.windows.keys() {
            if !render_order.contains(id) {
                render_order.push(id.clone());
            }
        }

        let get_sort_key = |id: &ObjectId| -> u32 {
            if let Some(win) = all_logical.iter().find(|w| &w.surface.id() == id) {
                let is_fullscreen = win.fullscreen && win.layer_surface.is_none();
                let base_layer = if win.layer_surface.is_none() {
                    2
                } else {
                    match win.layer {
                        0 => 0,
                        1 => 1,
                        2 => 3,
                        3 => 4,
                        _ => 3,
                    }
                };
                match base_layer {
                    2 if is_fullscreen => 4,
                    4 => 5,
                    other => other,
                }
            } else if let Some(anim_win) = self.windows.get(id) {
                match anim_win.render_layer {
                    4 => 5,
                    other => other,
                }
            } else {
                2
            }
        };

        // Sort stably by render_layer so we don't break the WM's internal focus order,
        // but we ensure layers (background, workspace, overlay) are respected.
        render_order.sort_by_key(|id| get_sort_key(id));

        for id in render_order {
            if let Some(anim_win) = self.windows.get(&id) {
                let win_offset = self.get_workspace_offset_for_surface(&id, wm);

                let surface = all_logical
                    .iter()
                    .find(|w| w.surface.id() == id)
                    .map(|w| w.surface.clone());
                let radius = if anim_win.is_ssd {
                    self.style.rounding
                } else {
                    0.0
                };

                if let Some(surf) = surface {
                    // Adjust position based on scale to keep window centered during animation
                    let (sw, sh) =
                        self.get_surface_size(&id, textures, viewports, surface_to_viewport);
                    let x_offset = (sw - sw * anim_win.scale.current) / 2.0;
                    let y_offset = (sh - sh * anim_win.scale.current) / 2.0;

                    self.draw_surface_recursive(
                        &surf,
                        anim_win.x.current + x_offset + win_offset,
                        anim_win.y.current + y_offset,
                        subsurfaces,
                        textures,
                        viewports,
                        surface_to_viewport,
                        &mut draw_list,
                        radius,
                        anim_win.alpha.current as f32,
                        anim_win.scale.current as f32,
                        true,
                        anim_win.is_ssd,
                    );
                } else if let Some(tex) = &anim_win.texture_snapshot {
                    let lw = (tex.w as f64 / tex.scale as f64) * anim_win.scale.current;
                    let lh = (tex.h as f64 / tex.scale as f64) * anim_win.scale.current;

                    let sw = tex.w as f64 / tex.scale as f64;
                    let sh = tex.h as f64 / tex.scale as f64;
                    let x_offset = (sw - lw) / 2.0;
                    let y_offset = (sh - lh) / 2.0;

                    let final_x = anim_win.x.current + x_offset + win_offset;
                    let final_y = anim_win.y.current + y_offset;

                    if self.style.blur.enabled && anim_win.is_ssd {
                        draw_list.push(DrawCommand::BlurCapture);
                        draw_list.push(DrawCommand::Blur(RenderQuad {
                            set: tex.set, // unused by blur pipeline
                            x: final_x.round() as f32,
                            y: final_y.round() as f32,
                            w: lw.round() as f32,
                            h: lh.round() as f32,
                            src_x: 0.0,
                            src_y: 0.0,
                            src_w: 1.0,
                            src_h: 1.0,
                            border_radius: radius,
                            alpha: anim_win.alpha.current as f32,
                        }));
                    }

                    if anim_win.is_ssd && self.style.shadow.enabled {
                        let scale = anim_win.scale.current as f32;
                        let shadow_size = self.style.shadow.range as f32 * scale;

                        let shadow_x = final_x as f32 + (self.style.shadow.offset.0 as f32 * scale)
                            - shadow_size;
                        let shadow_y = final_y as f32 + (self.style.shadow.offset.1 as f32 * scale)
                            - shadow_size;

                        let shadow_w = lw.round() as f32 + (shadow_size * 2.0);
                        let shadow_h = lh.round() as f32 + (shadow_size * 2.0);

                        draw_list.push(DrawCommand::Shadow(crate::vulkan::ShadowQuad {
                            x: shadow_x.round(),
                            y: shadow_y.round(),
                            w: shadow_w.round(),
                            h: shadow_h.round(),
                            border_radius: radius,
                            spread: shadow_size,
                            power: self.style.shadow.render_power as f32,
                            alpha: anim_win.alpha.current as f32,
                            color: self.style.shadow.color,
                        }));
                    }

                    draw_list.push(DrawCommand::Texture(RenderQuad {
                        set: tex.set,
                        x: final_x.round() as f32,
                        y: final_y.round() as f32,
                        w: lw.round() as f32,
                        h: lh.round() as f32,
                        src_x: 0.0,
                        src_y: 0.0,
                        src_w: 1.0,
                        src_h: 1.0,
                        border_radius: radius,
                        alpha: anim_win.alpha.current as f32,
                    }));
                }
            }
        }

        // 2. Draw popups (on top of windows)
        for popup in wm.get_popups() {
            let (abs_x, abs_y) = wm.get_absolute_position(&popup.surface.id());
            let win_offset = self.get_workspace_offset_for_surface(&popup.surface.id(), wm);
            let surf_x = abs_x - popup.geometry.x as f64 + win_offset;
            let surf_y = abs_y - popup.geometry.y as f64;

            self.draw_surface_recursive(
                &popup.surface,
                surf_x,
                surf_y,
                subsurfaces,
                textures,
                viewports,
                surface_to_viewport,
                &mut draw_list,
                0.0,
                1.0,
                1.0,
                true,
                false, // Popups are not SSD
            );
        }

        draw_list
    }

    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        input_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
        extra_hit_surfaces: &[(wayland_server::protocol::wl_surface::WlSurface, f64, f64)],
    ) -> HitResult {
        // we check extra hit surfaces (like IME popups) first, as they are drawn on top
        for (surface, surf_x, surf_y) in extra_hit_surfaces.iter().rev() {
            if let Some(hit) = self.hit_test_recursive(
                surface,
                *surf_x,
                *surf_y,
                cursor_x,
                cursor_y,
                subsurfaces,
                textures,
                viewports,
                surface_to_viewport,
                input_regions,
            ) {
                return hit;
            }
        }

        for popup in wm.get_popups().iter().rev() {
            let (abs_x, abs_y) = wm.get_absolute_position(&popup.surface.id());
            let win_offset = self.get_workspace_offset_for_surface(&popup.surface.id(), wm);
            let surf_x = abs_x - popup.geometry.x as f64 + win_offset;
            let surf_y = abs_y - popup.geometry.y as f64;

            if let Some(hit) = self.hit_test_recursive(
                &popup.surface,
                surf_x,
                surf_y,
                cursor_x,
                cursor_y,
                subsurfaces,
                textures,
                viewports,
                surface_to_viewport,
                input_regions,
            ) {
                return hit;
            }
        }

        // Check layers in reverse order for hit testing
        let mut all_windows = wm.get_background();
        all_windows.extend(wm.get_bottom());
        all_windows.extend(wm.get_workspace_windows());
        all_windows.extend(wm.get_top());
        all_windows.extend(wm.get_overlay());

        let get_sort_key = |win: &crate::wm::WindowState| -> u32 {
            let is_fullscreen = win.fullscreen && win.layer_surface.is_none();
            let base_layer = if win.layer_surface.is_none() {
                2
            } else {
                match win.layer {
                    0 => 0,
                    1 => 1,
                    2 => 3,
                    3 => 4,
                    _ => 3,
                }
            };
            match base_layer {
                2 if is_fullscreen => 4,
                4 => 5,
                other => other,
            }
        };
        all_windows.sort_by_key(|win| get_sort_key(win));

        let all_windows_cloned = all_windows.clone();

        for win in all_windows.into_iter().rev() {
            let has_transient_child = all_windows_cloned
                .iter()
                .any(|w| w.parent_id.as_ref() == Some(&win.surface.id()));

            let win_offset = self.get_workspace_offset_for_surface(&win.surface.id(), wm);

            if let Some(hit) = self.hit_test_recursive(
                &win.surface,
                win.x + win_offset,
                win.y,
                cursor_x,
                cursor_y,
                subsurfaces,
                textures,
                viewports,
                surface_to_viewport,
                input_regions,
            ) {
                if has_transient_child {
                    return HitResult {
                        surface: None,
                        local_x: 0.0,
                        local_y: 0.0,
                    };
                }
                return hit;
            }
        }

        HitResult {
            surface: None,
            local_x: 0.0,
            local_y: 0.0,
        }
    }

    fn supports_ssd(&self) -> bool {
        true
    }

    fn blur_passes(&self) -> u32 {
        if self.style.blur.enabled {
            self.style.blur.passes
        } else {
            0
        }
    }

    fn draw_surface_tree(
        &self,
        surface: &WlSurface,
        abs_x: f64,
        abs_y: f64,
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        draw_list: &mut Vec<DrawCommand>,
    ) {
        self.draw_surface_recursive(
            surface,
            abs_x,
            abs_y,
            subsurfaces,
            textures,
            viewports,
            surface_to_viewport,
            draw_list,
            0.0,
            1.0,
            1.0,
            false,
            false,
        );
    }
}

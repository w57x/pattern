use crate::server::SubsurfaceData;
use crate::vulkan::{DrawCommand, RenderQuad, SurfaceTexture};
use crate::wm::{PopupState, WindowState};
use std::collections::HashMap;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

/// The result of a hit-test operation.
pub struct HitResult {
    /// The surface that was hit, if any.
    pub surface: Option<WlSurface>,
    /// The X coordinate relative to the surface's top-left corner.
    pub local_x: f64,
    /// The Y coordinate relative to the surface's top-left corner.
    pub local_y: f64,
}

/// A trait that defines how surfaces are rendered and how hit-testing is performed.
///
/// The `Styler` is responsible for translating the window manager's state into
/// concrete drawing commands for the renderer, and for mapping global cursor
/// coordinates back to specific surfaces.
pub trait Styler {
    /// Generates a list of draw commands for the current frame.
    ///
    /// This method takes the complete state of the compositor and produces
    /// the necessary commands to render the entire scene.
    fn generate_draw_list(
        &self,
        windows: &[WindowState],
        popups: &[PopupState],
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        opaque_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
    ) -> Vec<DrawCommand>;

    /// Performs a hit-test to find the surface under the cursor.
    ///
    /// This should account for window Z-order, subsurfaces, and input regions.
    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        windows: &[WindowState],
        popups: &[PopupState],
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        input_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
    ) -> HitResult;

    /// Returns whether this styler supports server-side decorations (SSD).
    fn supports_ssd(&self) -> bool {
        false
    }
}

/// The default implementation of the `Styler` trait.
///
/// It provides a simple 2D rendering of surfaces and standard Wayland hit-testing.
pub struct DefaultStyler;

impl DefaultStyler {
    /// Creates a new `DefaultStyler`.
    pub fn new() -> Self {
        Self
    }

    /// Calculates the logical size of a surface, accounting for viewports and buffer scales.
    fn get_surface_size(
        &self,
        surface_id: &ObjectId,
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
    ) -> (f64, f64) {
        // If viewport destination size is set, use it.
        if let Some(vp_id) = surface_to_viewport.get(surface_id) {
            if let Some((_, Some(dst))) = viewports.get(vp_id) {
                return (dst.0 as f64, dst.1 as f64);
            }
        }

        // Otherwise use buffer size / scale
        if let Some(tex) = textures.get(surface_id) {
            return (
                tex.w as f64 / tex.scale as f64,
                tex.h as f64 / tex.scale as f64,
            );
        }

        (0.0, 0.0)
    }

    /// Recursively generates draw commands for a surface and all its subsurfaces.
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
    ) {
        let (lw, lh) =
            self.get_surface_size(&surface.id(), textures, viewports, surface_to_viewport);

        if let Some(tex) = textures.get(&surface.id()) {
            let mut src_x = 0.0;
            let mut src_y = 0.0;
            let mut src_w = 1.0;
            let mut src_h = 1.0;

            // Handle viewport source (cropping)
            if let Some(vp_id) = surface_to_viewport.get(&surface.id()) {
                if let Some((Some(src), _)) = viewports.get(vp_id) {
                    src_x = (src.0 / tex.w as f64) as f32;
                    src_y = (src.1 / tex.h as f64) as f32;
                    src_w = (src.2 / tex.w as f64) as f32;
                    src_h = (src.3 / tex.h as f64) as f32;
                }
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
            }));
        }

        for sub in subsurfaces {
            if sub.parent.id() == surface.id() {
                self.draw_surface_recursive(
                    &sub.surface,
                    abs_x + sub.x as f64,
                    abs_y + sub.y as f64,
                    subsurfaces,
                    textures,
                    viewports,
                    surface_to_viewport,
                    draw_list,
                    0.0,
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

            // If an input region is defined, the hit must be inside one of its rects
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
}

impl Styler for DefaultStyler {
    fn generate_draw_list(
        &self,
        windows: &[WindowState],
        popups: &[PopupState],
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        _opaque_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
    ) -> Vec<DrawCommand> {
        let mut draw_list = Vec::new();

        for win_state in windows {
            let radius = if win_state.ssd { 20.0 } else { 0.0 };
            self.draw_surface_recursive(
                &win_state.surface,
                win_state.x,
                win_state.y,
                subsurfaces,
                textures,
                viewports,
                surface_to_viewport,
                &mut draw_list,
                radius,
            );
        }

        for popup in popups {
            let (abs_x, abs_y) = wm.get_absolute_position(&popup.surface.id());
            // abs_x/y is the origin of the GEOMETRY.
            // draw_surface_recursive needs the origin of the SURFACE.
            // Surface origin = geometry origin - geometry offset.
            let surf_x = abs_x - popup.geometry.x as f64;
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
            );
        }

        draw_list
    }

    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        windows: &[WindowState],
        popups: &[PopupState],
        subsurfaces: &[SubsurfaceData],
        textures: &HashMap<ObjectId, SurfaceTexture>,
        viewports: &HashMap<ObjectId, (Option<(f64, f64, f64, f64)>, Option<(i32, i32)>)>,
        surface_to_viewport: &HashMap<ObjectId, ObjectId>,
        input_regions: &HashMap<ObjectId, Vec<crate::wm::Rect>>,
        wm: &dyn crate::wm::WindowManager,
    ) -> HitResult {
        for popup in popups.iter().rev() {
            let (abs_x, abs_y) = wm.get_absolute_position(&popup.surface.id());
            let surf_x = abs_x - popup.geometry.x as f64;
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

        for win in windows.iter().rev() {
            let has_transient_child = windows
                .iter()
                .any(|w| w.parent_id.as_ref() == Some(&win.surface.id()));

            if let Some(hit) = self.hit_test_recursive(
                &win.surface,
                win.x,
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
}

use crate::vulkan::{ColorQuad, DrawCommand, RenderQuad, SurfaceTexture};
use crate::wm::WindowState;
use std::collections::HashMap;
use wayland_server::Resource;
use wayland_server::backend::ObjectId;
use wayland_server::protocol::wl_surface::WlSurface;

pub struct HitResult {
    pub surface: Option<WlSurface>,
    pub local_x: f64,
    pub local_y: f64,
}

pub trait Styler {
    fn generate_draw_list(
        &self,
        windows: &[WindowState],
        textures: &HashMap<ObjectId, SurfaceTexture>,
    ) -> Vec<DrawCommand>;

    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        windows: &[WindowState],
        textures: &HashMap<ObjectId, SurfaceTexture>,
    ) -> HitResult;
}

pub struct DefaultStyler;

impl DefaultStyler {
    pub fn new() -> Self {
        Self
    }
}

impl Styler for DefaultStyler {
    fn generate_draw_list(
        &self,
        windows: &[WindowState],
        textures: &HashMap<ObjectId, SurfaceTexture>,
    ) -> Vec<DrawCommand> {
        let mut draw_list = Vec::new();

        let focused_surface = windows.last().map(|w| w.surface.id());

        for win_state in windows {
            if let Some(tex) = textures.get(&win_state.surface.id()) {
                let is_focused = Some(win_state.surface.id()) == focused_surface;

                if is_focused {
                    let border_size = 2.0;
                    draw_list.push(DrawCommand::Color(ColorQuad {
                        color: [0.8, 0.4, 0.4, 1.0], // Red-ish border
                        x: win_state.x as f32 - border_size,
                        y: win_state.y as f32 - border_size,
                        w: tex.w + border_size * 2.0,
                        h: tex.h + border_size * 2.0,
                    }));
                }

                draw_list.push(DrawCommand::Texture(RenderQuad {
                    set: tex.set,
                    x: win_state.x as f32,
                    y: win_state.y as f32,
                    w: tex.w,
                    h: tex.h,
                }));
            }
        }
        draw_list
    }

    fn hit_test(
        &self,
        cursor_x: f64,
        cursor_y: f64,
        windows: &[WindowState],
        textures: &HashMap<ObjectId, SurfaceTexture>,
    ) -> HitResult {
        let mut target_surface = None;
        let mut local_x = 0.0;
        let mut local_y = 0.0;

        for win in windows.iter().rev() {
            if let Some(tex) = textures.get(&win.surface.id()) {
                if cursor_x >= win.x
                    && cursor_x <= win.x + (tex.w as f64)
                    && cursor_y >= win.y
                    && cursor_y <= win.y + (tex.h as f64)
                {
                    target_surface = Some(win.surface.clone());
                    local_x = cursor_x - win.x;
                    local_y = cursor_y - win.y;
                    break;
                }
            }
        }

        HitResult {
            surface: target_surface,
            local_x,
            local_y,
        }
    }
}

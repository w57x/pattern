use crate::server::proto::cursor_shape_utils::shape_to_name;
use crate::vulkan::{SurfaceTexture, VulkanContext, VulkanTextureInner};
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_device_v1::Shape;
use xcursor::{
    CursorTheme,
    parser::{Image, parse_xcursor},
};

pub struct CursorFrame {
    pub texture: SurfaceTexture,
    pub hotspot: (f32, f32),
    pub delay: u32, // in milliseconds
}

pub struct CursorAnimation {
    pub frames: Vec<CursorFrame>,
    pub total_delay: u32,
}

pub struct CursorManager {
    pub theme: String,
    pub size: u32,
    pub animations: HashMap<Shape, CursorAnimation>,
}

impl CursorManager {
    pub fn new(theme: &str, size: u32) -> Self {
        Self {
            theme: theme.to_string(),
            size,
            animations: HashMap::new(),
        }
    }

    pub fn set_theme(&mut self, theme: &str, vkctx: &VulkanContext) {
        if self.theme != theme {
            self.clear(vkctx);
            self.theme = theme.to_string();
        }
    }

    pub fn set_size(&mut self, size: u32, vkctx: &VulkanContext) {
        if self.size != size {
            self.clear(vkctx);
            self.size = size;
        }
    }

    pub fn clear(&mut self, _vkctx: &VulkanContext) {
        // Resources are now automatically cleared by Arc<VulkanTextureInner> drop
        self.animations.clear();
    }

    pub fn get_or_load(&mut self, shape: Shape, vkctx: &VulkanContext) {
        if self.animations.contains_key(&shape) {
            return;
        }

        let name = shape_to_name(shape);
        let Some(images) = load_icon(&self.theme, name) else {
            return;
        };

        let nearest_size = images
            .iter()
            .min_by_key(|image| (self.size as i32 - image.size as i32).abs())
            .map(|img| img.size)
            .unwrap_or(self.size);

        let mut frames = Vec::new();
        let mut total_delay = 0;

        for image in images.iter().filter(|img| img.size == nearest_size) {
            let (cursor_vk_img, cursor_vk_mem, cursor_view, cursor_sampler) = unsafe {
                vkctx.upload_texture(
                    image.width,
                    image.height,
                    image.width * 4,
                    &image.pixels_rgba,
                )
            };

            let (desc_pool, desc_set) = unsafe {
                vkctx.create_descriptor_set(
                    vkctx.descriptor_set_layout,
                    cursor_view,
                    cursor_sampler,
                )
            };

            let inner = Arc::new(VulkanTextureInner {
                device: vkctx.device.clone(),
                img: cursor_vk_img,
                mem: cursor_vk_mem,
                view: cursor_view,
                samp: cursor_sampler,
                pool: desc_pool,
                garbage_queue: vkctx.texture_garbage_queue.clone(),
            });

            let tex = SurfaceTexture {
                inner,
                set: desc_set,
                w: image.width as f32,
                h: image.height as f32,
                scale: 1.0,
            };

            frames.push(CursorFrame {
                texture: tex,
                hotspot: (image.xhot as f32, image.yhot as f32),
                delay: image.delay,
            });
            total_delay += image.delay;
        }

        if !frames.is_empty() {
            self.animations.insert(
                shape,
                CursorAnimation {
                    frames,
                    total_delay,
                },
            );
        }
    }

    pub fn get_frame(&self, shape: Shape, mut millis: u32) -> Option<&CursorFrame> {
        let anim = self.animations.get(&shape)?;
        if anim.total_delay == 0 {
            return anim.frames.first();
        }

        millis %= anim.total_delay;

        for frame in &anim.frames {
            if millis < frame.delay {
                return Some(frame);
            }
            millis -= frame.delay;
        }

        anim.frames.first()
    }
}

fn load_icon(theme_name: &str, cursor_name: &str) -> Option<Vec<Image>> {
    let theme = CursorTheme::load(theme_name);
    let icon_path = theme.load_icon(cursor_name)?;

    let mut cursor_file = std::fs::File::open(icon_path).ok()?;
    let mut cursor_data = Vec::new();
    cursor_file.read_to_end(&mut cursor_data).ok()?;

    parse_xcursor(&cursor_data)
}

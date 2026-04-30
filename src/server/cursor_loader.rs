use crate::server::proto::cursor_shape_utils::shape_to_name;
use crate::vulkan::{SurfaceTexture, VulkanContext};
use ash::vk;
use std::collections::HashMap;
use std::io::Read;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_device_v1::Shape;
use xcursor::{
    CursorTheme,
    parser::{Image, parse_xcursor},
};

pub struct CursorManager {
    pub theme: String,
    pub size: u32,
    pub textures: HashMap<Shape, SurfaceTexture>,
    pub hotspots: HashMap<Shape, (f32, f32)>,
}

impl CursorManager {
    pub fn new(theme: &str, size: u32) -> Self {
        Self {
            theme: theme.to_string(),
            size,
            textures: HashMap::new(),
            hotspots: HashMap::new(),
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

    pub fn clear(&mut self, vkctx: &VulkanContext) {
        for tex in self.textures.values() {
            unsafe {
                vkctx.device.destroy_sampler(tex.samp, None);
                vkctx.device.destroy_image_view(tex.view, None);
                vkctx.device.destroy_image(tex.img, None);
                vkctx.device.free_memory(tex.mem, None);
                vkctx.device.destroy_descriptor_pool(tex.pool, None);
            }
        }
        self.textures.clear();
        self.hotspots.clear();
    }

    pub fn get_or_load(
        &mut self,
        shape: Shape,
        vkctx: &VulkanContext,
    ) -> Option<vk::DescriptorSet> {
        if let Some(tex) = self.textures.get(&shape) {
            return Some(tex.set);
        }

        let name = shape_to_name(shape);
        let images = load_icon(&self.theme, name)?;
        let image = select_nearest_image(&images, self.size);

        let (cursor_vk_img, cursor_vk_mem, cursor_view, cursor_sampler) = unsafe {
            vkctx.upload_texture(
                image.width,
                image.height,
                image.width * 4,
                &image.pixels_rgba,
            )
        };

        let (desc_pool, desc_set) = unsafe {
            vkctx.create_descriptor_set(vkctx.descriptor_set_layout, cursor_view, cursor_sampler)
        };

        let tex = SurfaceTexture {
            img: cursor_vk_img,
            mem: cursor_vk_mem,
            view: cursor_view,
            samp: cursor_sampler,
            pool: desc_pool,
            set: desc_set,
            w: image.width as f32,
            h: image.height as f32,
            scale: 1.0,
        };

        self.hotspots
            .insert(shape, (image.xhot as f32, image.yhot as f32));
        self.textures.insert(shape, tex);

        Some(desc_set)
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

fn select_nearest_image(images: &[Image], target_size: u32) -> &Image {
    images
        .iter()
        .min_by_key(|image| (target_size as i32 - image.size as i32).abs())
        .unwrap_or(&images[0])
}

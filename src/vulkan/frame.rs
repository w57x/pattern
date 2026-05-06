use ash::vk;
use drm::control::Device as _;

use crate::gpu::{Card, buffer::Buffer};
use crate::vulkan::BlurChain;

pub struct VulkanFrame {
    #[allow(unused)]
    pub bo: Buffer<()>,

    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub fb_handle: drm::control::framebuffer::Handle,

    pub vk_view: vk::ImageView,
    pub vk_fb: vk::Framebuffer,

    pub blur_target: Option<BlurChain>,
}

impl VulkanFrame {
    pub unsafe fn destroy(&self, device: &ash::Device, card: &Card) {
        unsafe {
            if let Some(target) = &self.blur_target {
                // Manually clean up blur target resources
                for t in &target.targets {
                    device.destroy_framebuffer(t.framebuffer, None);
                    device.destroy_image_view(t.view, None);
                    device.destroy_image(t.image, None);
                    device.free_memory(t.memory, None);
                }
                device.destroy_sampler(target.sampler, None);
                device.destroy_descriptor_pool(target.pool, None);
                device.destroy_render_pass(target.render_pass, None);
            }
            device.destroy_framebuffer(self.vk_fb, None);
            device.destroy_image_view(self.vk_view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
        card.destroy_framebuffer(self.fb_handle).unwrap();
    }
}

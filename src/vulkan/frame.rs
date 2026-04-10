use ash::vk;
use drm::control::Device as _;

use crate::gpu::{Card, buffer::Buffer};

pub struct VulkanFrame {
    #[allow(unused)]
    pub bo: Buffer<()>,

    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub fb_handle: drm::control::framebuffer::Handle,

    pub vk_view: vk::ImageView,
    pub vk_fb: vk::Framebuffer,
}

impl VulkanFrame {
    pub unsafe fn destroy(&self, device: &ash::Device, card: &Card) {
        unsafe {
            device.destroy_framebuffer(self.vk_fb, None);
            device.destroy_image_view(self.vk_view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
        card.destroy_framebuffer(self.fb_handle).unwrap();
    }
}

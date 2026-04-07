use ash::{Device, Entry, Instance, vk};
use drm::buffer::Buffer as _;
use std::ffi::CStr;
use std::os::fd::IntoRawFd;

use crate::gpu::buffer::Buffer;

pub struct VulkanContext {
    #[allow(unused)]
    pub entry: Entry,
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: Device,
    pub queue: vk::Queue,
    pub queue_family_index: u32,
    pub command_pool: vk::CommandPool,
    pub fence: vk::Fence,
}

impl std::fmt::Display for VulkanContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VkContext(.queue_family_index = {})",
            self.queue_family_index
        )
    }
}

impl VulkanContext {
    pub fn new() -> Self {
        let entry = unsafe { Entry::load().expect("Failed to load Vulkan driver") };

        let app_info = vk::ApplicationInfo::default()
            .application_name(CStr::from_bytes_with_nul(b"Pattern Engine\0").unwrap())
            .api_version(vk::make_api_version(0, 1, 2, 0)); // Vulkan 1.2

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Failed to create Vulkan Instance")
        };

        let physical_devices = unsafe { instance.enumerate_physical_devices().unwrap() };
        let physical_device = physical_devices[0];

        let queue_family_properties =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

        let queue_family_index = queue_family_properties
            .iter()
            .position(|info| info.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .expect("No Graphics Queue found") as u32;

        let device_extensions = [
            ash::khr::external_memory_fd::NAME.as_ptr(),
            ash::khr::external_memory::NAME.as_ptr(),
            ash::ext::external_memory_dma_buf::NAME.as_ptr(),
            ash::ext::image_drm_format_modifier::NAME.as_ptr(),
        ];

        let queue_priorities = [1.0];

        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extensions);

        let device = unsafe {
            instance
                .create_device(physical_device, &device_create_info, None)
                .expect("Failed to create Vulkan Logical Device")
        };

        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        let command_pool = unsafe {
            device
                .create_command_pool(&pool_create_info, None)
                .expect("Failed to create Command Pool")
        };

        // NOTE: Creating a fence in the signaled state so we can immediately use it on the first frame
        let fence_create_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let fence = unsafe {
            device
                .create_fence(&fence_create_info, None)
                .expect("Failed to create Fence")
        };

        Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family_index,
            command_pool,
            fence,
        }
    }

    pub unsafe fn clear_image_and_wait(&self, image: vk::Image, r: f32, g: f32, b: f32, a: f32) {
        unsafe {
            self.device.reset_fences(&[self.fence]).unwrap();
        }

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let cmd_buffer = unsafe { self.device.allocate_command_buffers(&alloc_info).unwrap()[0] };

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.device
                .begin_command_buffer(cmd_buffer, &begin_info)
                .unwrap();
        }

        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        // NOTE: Transition Image: UNDEFINED -> TRANSFER_DST
        let barrier_to_clear = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .image(image)
            .subresource_range(subresource_range);

        unsafe {
            // NOTE: Insert a memory dependency
            self.device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier_to_clear),
            );
        }

        let clear_color = vk::ClearColorValue {
            float32: [r, g, b, a],
        };

        unsafe {
            self.device.cmd_clear_color_image(
                cmd_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear_color,
                std::slice::from_ref(&subresource_range),
            );
        }

        // NOTE: Transition Image: TRANSFER_DST -> GENERAL (For DRM display)
        let barrier_to_present = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ) // DRM will read this
            .image(image)
            .subresource_range(subresource_range);

        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                std::slice::from_ref(&barrier_to_present),
            );
        }

        // SUBMITINNNNG :)

        unsafe {
            self.device.end_command_buffer(cmd_buffer).unwrap();
        }

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd_buffer));

        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit_info), self.fence)
                .expect("Failed to submit command buffer");

            self.device
                .wait_for_fences(&[self.fence], true, std::u64::MAX)
                .unwrap();

            self.device
                .free_command_buffers(self.command_pool, &[cmd_buffer]);
        }
    }

    pub unsafe fn import_gbm_buffer(
        &self,
        b: &Buffer<()>,
        width: u32,
        height: u32,
    ) -> (vk::Image, vk::DeviceMemory) {
        let fd = b.to_owned_fd();
        let bo = b.raw();
        let modifier = bo.modifier();
        let stride = b.pitch();
        let offset = bo.offset(0) as u64;

        let format = vk::Format::B8G8R8A8_UNORM; // NOTE: Format::Xrgb8888

        let subresource_layout = vk::SubresourceLayout::default()
            .offset(offset)
            .size(0) // NOTE: 0 means let Vulkan figure out the total size from stride/height
            .row_pitch(stride as u64)
            .array_pitch(0)
            .depth_pitch(0);

        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier.into())
            .plane_layouts(std::slice::from_ref(&subresource_layout));

        let mut external_memory_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let mut image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        image_create_info = image_create_info.push_next(&mut external_memory_info);
        image_create_info = image_create_info.push_next(&mut modifier_info);

        let image = unsafe {
            self.device
                .create_image(&image_create_info, None)
                .expect("Failed to create Vulkan Image from GBM constraints")
        };

        let mem_reqs = unsafe { self.device.get_image_memory_requirements(image) };

        let raw_dup_fd = nix::unistd::dup(fd).unwrap().into_raw_fd();
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(raw_dup_fd);

        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            // NOTE: Memory type index is highly hardware dependent.
            // For DMA-BUFs, we must query the valid memory types.
            .memory_type_index(self.find_memory_type_index(mem_reqs.memory_type_bits))
            .push_next(&mut import_info);

        let memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .expect("Failed to import DMA-BUF memory into Vulkan")
        };

        unsafe {
            self.device
                .bind_image_memory(image, memory, 0)
                .expect("Failed to bind DMA-BUF memory to Vulkan Image");
        }

        (image, memory)
    }

    fn find_memory_type_index(&self, type_filter: u32) -> u32 {
        let mem_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };
        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0 {
                return i;
            }
        }

        panic!("Failed to find suitable memory type for DMA-BUF import");
    }
}

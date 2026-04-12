use ash::{Device, Entry, Instance, vk};
use drm::buffer::Buffer as _;
use std::ffi::CStr;
use std::os::fd::{IntoRawFd, OwnedFd};

use crate::gpu::buffer::Buffer;
pub mod frame;

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

    pub render_pass: vk::RenderPass,
    pub pipeline_layout: vk::PipelineLayout,
    pub color_pipeline_layout: vk::PipelineLayout,
    pub graphics_pipeline: vk::Pipeline,
    pub color_pipeline: vk::Pipeline,

    pub descriptor_set_layout: vk::DescriptorSetLayout,
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

        let render_pass = unsafe { Self::create_render_pass(&device) };
        let (
            descriptor_set_layout,
            pipeline_layout,
            color_pipeline_layout,
            graphics_pipeline,
            color_pipeline,
        ) = unsafe { Self::create_graphics_pipeline(&device, render_pass) };

        Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_family_index,
            command_pool,
            fence,
            render_pass,
            pipeline_layout,
            color_pipeline_layout,
            graphics_pipeline,
            color_pipeline,
            descriptor_set_layout,
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
            .memory_type_index(self.find_memory_type_index(
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ))
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

    fn find_memory_type_index(&self, type_filter: u32, properties: vk::MemoryPropertyFlags) -> u32 {
        let mem_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };
        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && (mem_properties.memory_types[i as usize].property_flags & properties)
                    == properties
            {
                return i;
            }
        }
        panic!("Failed to find suitable memory type");
    }

    pub unsafe fn create_render_pass(device: &ash::Device) -> vk::RenderPass {
        // NOTE: XRGB8888 -> BGRA8_UNORM
        let format = vk::Format::B8G8R8A8_UNORM;

        let color_attachment = vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::GENERAL);

        let color_attachment_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&color_attachment_ref));

        let dependency = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&color_attachment))
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(std::slice::from_ref(&dependency));

        unsafe {
            device
                .create_render_pass(&render_pass_info, None)
                .expect("Failed to create Render Pass")
        }
    }

    unsafe fn create_shader_module(device: &ash::Device, code: &[u8]) -> vk::ShaderModule {
        let mut cursor = std::io::Cursor::new(code);
        let code_u32 = ash::util::read_spv(&mut cursor).expect("Failed to parse SPIR-V bytecode");
        let create_info = vk::ShaderModuleCreateInfo::default().code(&code_u32);
        unsafe {
            device
                .create_shader_module(&create_info, None)
                .expect("Failed to create Shader Module")
        }
    }

    pub unsafe fn create_graphics_pipeline(
        device: &ash::Device,
        render_pass: vk::RenderPass,
    ) -> (
        vk::DescriptorSetLayout,
        vk::PipelineLayout,
        vk::PipelineLayout,
        vk::Pipeline,
        vk::Pipeline,
    ) {
        // Loading the embedded shaders from the Cargo output directory
        let vert_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/quad.vert.spv"));
        let frag_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/quad.frag.spv"));
        let solid_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/solid.frag.spv"));

        let vert_shader_module = unsafe { Self::create_shader_module(device, vert_shader_code) };
        let frag_shader_module = unsafe { Self::create_shader_module(device, frag_shader_code) };
        let solid_shader_module = unsafe { Self::create_shader_module(device, solid_shader_code) };

        let main_function_name =
            unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0") };

        let mut shader_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_shader_module)
                .name(main_function_name),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_shader_module)
                .name(main_function_name),
        ];

        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default();

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding));

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .expect("Failed to create Descriptor Set Layout")
        };

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        // Viewport State (Set to dynamic so we can update it in the command buffer)
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        // Rasterizer (Turns math triangles into pixel fragments)
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        // Color Blending (How transparent windows/cursors mix with the background)
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(std::slice::from_ref(&color_blend_attachment));

        // Push Constants Layout
        let push_constant_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<PushConstants>() as u32);

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant_range));

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .expect("Failed to create Pipeline Layout")
        };

        let color_pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(std::slice::from_ref(&push_constant_range));

        let color_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&color_pipeline_layout_info, None)
                .expect("Failed to create Color Pipeline Layout")
        };

        // Finally, baking the Pipeline object
        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state_info)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let graphics_pipeline = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .expect("Failed to create Graphics Pipeline")[0]
        };

        shader_stages[1] = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(solid_shader_module)
            .name(main_function_name);

        let solid_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state_info)
            .layout(color_pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        let color_pipeline = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&solid_pipeline_info),
                    None,
                )
                .expect("Failed to create Solid Graphics Pipeline")[0]
        };

        // Cleanup the temporary shader modules now that the pipeline has digested them
        unsafe {
            device.destroy_shader_module(vert_shader_module, None);
            device.destroy_shader_module(frag_shader_module, None);
            device.destroy_shader_module(solid_shader_module, None);
        }

        (
            descriptor_set_layout,
            pipeline_layout,
            color_pipeline_layout,
            graphics_pipeline,
            color_pipeline,
        )
    }

    pub unsafe fn create_vk_framebuffer(
        &self,
        image: vk::Image,
        width: u32,
        height: u32,
    ) -> (vk::ImageView, vk::Framebuffer) {
        // Create the View
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );

        let image_view = unsafe {
            self.device
                .create_image_view(&view_info, None)
                .expect("Failed to create Image View")
        };

        // Link the View to the Render Pass via a Framebuffer
        let fb_info = vk::FramebufferCreateInfo::default()
            .render_pass(self.render_pass)
            .attachments(std::slice::from_ref(&image_view))
            .width(width)
            .height(height)
            .layers(1);

        let framebuffer = unsafe {
            self.device
                .create_framebuffer(&fb_info, None)
                .expect("Failed to create Vulkan Framebuffer")
        };

        (image_view, framebuffer)
    }

    pub unsafe fn draw_frame(
        &self,
        vk_fb: vk::Framebuffer,
        screen_w: u32,
        screen_h: u32,
        quads: &[DrawCommand],
    ) {
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

        // Begin the Render Pass
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.1, 0.1, 0.12, 1.0],
            },
        }];

        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(vk_fb)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: screen_w,
                    height: screen_h,
                },
            })
            .clear_values(&clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                cmd_buffer,
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            );
        }

        // Set dynamic viewport/scissor
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: screen_w as f32,
            height: screen_h as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        unsafe {
            self.device
                .cmd_set_viewport(cmd_buffer, 0, std::slice::from_ref(&viewport));
        }

        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: screen_w,
                height: screen_h,
            },
        };

        unsafe {
            self.device
                .cmd_set_scissor(cmd_buffer, 0, std::slice::from_ref(&scissor));
        }

        // PAINTER LOGIC
        let mut current_pipeline = vk::Pipeline::null();

        for cmd in quads {
            match cmd {
                DrawCommand::Texture(quad) => {
                    if current_pipeline != self.graphics_pipeline {
                        unsafe {
                            self.device.cmd_bind_pipeline(
                                cmd_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.graphics_pipeline,
                            );
                        }
                        current_pipeline = self.graphics_pipeline;
                    }

                    unsafe {
                        self.device.cmd_bind_descriptor_sets(
                            cmd_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.pipeline_layout,
                            0,
                            std::slice::from_ref(&quad.set),
                            &[],
                        );
                    }

                    let push_constants = PushConstants {
                        pos: [quad.x, quad.y],
                        screen_size: [screen_w as f32, screen_h as f32],
                        quad_size: [quad.w, quad.h],
                        src_offset: [quad.src_x, quad.src_y],
                        src_size: [quad.src_w, quad.src_h],
                        border_radius: quad.border_radius,
                        _padding: 0.0,
                        color: [1.0, 1.0, 1.0, 1.0], // Default color, unused by quad.frag
                    };

                    let push_bytes = unsafe {
                        std::slice::from_raw_parts(
                            &push_constants as *const _ as *const u8,
                            std::mem::size_of::<PushConstants>(),
                        )
                    };

                    unsafe {
                        self.device.cmd_push_constants(
                            cmd_buffer,
                            self.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            push_bytes,
                        );

                        self.device.cmd_draw(cmd_buffer, 6, 1, 0, 0);
                    }
                }
                DrawCommand::Color(quad) => {
                    if current_pipeline != self.color_pipeline {
                        unsafe {
                            self.device.cmd_bind_pipeline(
                                cmd_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.color_pipeline,
                            );
                        }
                        current_pipeline = self.color_pipeline;
                    }

                    let push_constants = PushConstants {
                        pos: [quad.x, quad.y],
                        screen_size: [screen_w as f32, screen_h as f32],
                        quad_size: [quad.w, quad.h],
                        src_offset: [0.0, 0.0],
                        src_size: [1.0, 1.0],
                        border_radius: quad.border_radius,
                        _padding: 0.0,
                        color: quad.color,
                    };

                    let push_bytes = unsafe {
                        std::slice::from_raw_parts(
                            &push_constants as *const _ as *const u8,
                            std::mem::size_of::<PushConstants>(),
                        )
                    };

                    unsafe {
                        self.device.cmd_push_constants(
                            cmd_buffer,
                            self.color_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            push_bytes,
                        );

                        self.device.cmd_draw(cmd_buffer, 6, 1, 0, 0);
                    }
                }
            }
        }

        // SUBMITINNNNG :)

        unsafe {
            self.device.cmd_end_render_pass(cmd_buffer);
            self.device.end_command_buffer(cmd_buffer).unwrap();
        }

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd_buffer));

        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit_info), self.fence)
                .expect("Failed to submit command buffer");

            // Wait for the GPU to finish painting
            self.device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .unwrap();
            self.device
                .free_command_buffers(self.command_pool, &[cmd_buffer]);
        }
    }

    unsafe fn execute_one_time_commands<F: FnOnce(vk::CommandBuffer)>(&self, f: F) {
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

        f(cmd_buffer);

        unsafe {
            self.device.end_command_buffer(cmd_buffer).unwrap();
        }

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd_buffer));

        unsafe {
            self.device
                .queue_submit(self.queue, std::slice::from_ref(&submit_info), self.fence)
                .expect("Failed to submit one-time command");

            self.device
                .wait_for_fences(&[self.fence], true, std::u64::MAX)
                .unwrap();
            self.device
                .free_command_buffers(self.command_pool, &[cmd_buffer]);
        }
    }

    pub unsafe fn upload_texture(
        &self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> (vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler) {
        let image_size = (width * height * 4) as u64; // RGBA = 4 bytes per pixel

        // Create Staging Buffer (CPU Visible)
        let buffer_info = vk::BufferCreateInfo::default()
            .size(image_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe { self.device.create_buffer(&buffer_info, None).unwrap() };

        let mem_reqs = unsafe { self.device.get_buffer_memory_requirements(staging_buffer) };
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(self.find_memory_type_index(
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ));

        let staging_memory = unsafe { self.device.allocate_memory(&alloc_info, None).unwrap() };

        unsafe {
            self.device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
                .unwrap();
        }

        // Copy pixels from Rust &[u8] into the Staging Buffer

        let data_ptr = unsafe {
            self.device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .unwrap()
        };

        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), data_ptr as *mut u8, pixels.len());
            self.device.unmap_memory(staging_memory);
        }

        // Create the actual Vulkan Image (GPU Only)
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::B8G8R8A8_UNORM) // Xcursor pixels are usually sRGB
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let image = unsafe { self.device.create_image(&image_info, None).unwrap() };
        let mem_reqs = unsafe { self.device.get_image_memory_requirements(image) };
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(self.find_memory_type_index(
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ));

        let image_memory = unsafe { self.device.allocate_memory(&alloc_info, None).unwrap() };

        unsafe {
            self.device
                .bind_image_memory(image, image_memory, 0)
                .unwrap();

            // Record commands to transfer the data
            self.execute_one_time_commands(|cmd| {
                let subresource_range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);

                // Transition: UNDEFINED -> TRANSFER_DST
                let barrier1 = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .image(image)
                    .subresource_range(subresource_range);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&barrier1),
                );

                // Copy Buffer to Image
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                    .image_extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    });

                self.device.cmd_copy_buffer_to_image(
                    cmd,
                    staging_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    std::slice::from_ref(&region),
                );

                // Transition: TRANSFER_DST -> SHADER_READ_ONLY
                let barrier2 = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(image)
                    .subresource_range(subresource_range);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&barrier2),
                );
            });
        }

        // Cleanup Staging Buffer
        unsafe {
            self.device.destroy_buffer(staging_buffer, None);
            self.device.free_memory(staging_memory, None);
        }

        // Create Image View and Sampler
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );
        let image_view = unsafe { self.device.create_image_view(&view_info, None).unwrap() };

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false)
            .unnormalized_coordinates(false);
        let sampler = unsafe { self.device.create_sampler(&sampler_info, None).unwrap() };

        (image, image_memory, image_view, sampler)
    }

    pub unsafe fn create_descriptor_set(
        &self,
        layout: vk::DescriptorSetLayout,
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> (vk::DescriptorPool, vk::DescriptorSet) {
        // Create a Pool that can hold 1 Image Sampler
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            self.device
                .create_descriptor_pool(&pool_info, None)
                .expect("Failed to create Descriptor Pool")
        };

        // Allocate the Set using the Layout we built in the pipeline
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&layout));

        let descriptor_set = unsafe {
            self.device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate Descriptor Set")[0]
        };

        // Write our specific texture into the allocated Set
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler);

        let write_descriptor_set = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe {
            self.device
                .update_descriptor_sets(std::slice::from_ref(&write_descriptor_set), &[]);
        }

        (descriptor_pool, descriptor_set)
    }

    pub unsafe fn import_dmabuf(
        &self,
        ofd: &OwnedFd,
        width: u32,
        height: u32,
        stride: u32,
        modifier: u64,
    ) -> (vk::Image, vk::DeviceMemory) {
        let dup_fd = ofd.try_clone().expect("Failed to duplicate DMA-BUF FD");
        let raw_fd = dup_fd.into_raw_fd();

        let format = vk::Format::B8G8R8A8_UNORM;

        let subresource_layout = vk::SubresourceLayout::default()
            .offset(0)
            .size(0)
            .row_pitch(stride as u64)
            .array_pitch(0)
            .depth_pitch(0);

        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier)
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
            // Use DRM modifier tiling for direct zero-copy access
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED) // We will sample this directly
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        image_create_info = image_create_info.push_next(&mut external_memory_info);
        image_create_info = image_create_info.push_next(&mut modifier_info);

        let image = unsafe {
            self.device
                .create_image(&image_create_info, None)
                .expect("Failed to create Vulkan Image from DMA-BUF")
        };

        let mem_reqs = unsafe { self.device.get_image_memory_requirements(image) };
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(raw_fd);

        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(self.find_memory_type_index(
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ))
            .push_next(&mut import_info);

        let memory = match unsafe { self.device.allocate_memory(&allocate_info, None) } {
            Ok(m) => m,
            Err(e) => {
                // If allocation fails, Vulkan did not take ownership. We must close the FD.
                let _ = unsafe { libc::close(raw_fd) };
                panic!("Failed to import DMA-BUF memory into Vulkan: {:?}", e);
            }
        };

        unsafe {
            self.device.bind_image_memory(image, memory, 0).unwrap();

            self.execute_one_time_commands(|cmd| {
                let barrier = vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(image)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            });
        }

        (image, memory)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PushConstants {
    pub pos: [f32; 2],
    pub screen_size: [f32; 2],
    pub quad_size: [f32; 2],
    pub src_offset: [f32; 2],
    pub src_size: [f32; 2],
    pub border_radius: f32,
    pub _padding: f32,
    pub color: [f32; 4],
}

#[derive(Default, Debug)]
pub struct RenderQuad {
    pub set: ash::vk::DescriptorSet,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub src_x: f32,
    pub src_y: f32,
    pub src_w: f32,
    pub src_h: f32,
    pub border_radius: f32,
}

#[derive(Default, Debug)]
pub struct ColorQuad {
    pub color: [f32; 4],
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub border_radius: f32,
}

#[derive(Debug)]
pub enum DrawCommand {
    Texture(RenderQuad),
    Color(ColorQuad),
}

impl RenderQuad {
    pub fn from(set: ash::vk::DescriptorSet) -> Self {
        Self {
            set,
            ..Self::default()
        }
    }

    pub fn xy(self, x: f32, y: f32) -> Self {
        Self { x, y, ..self }
    }

    pub fn dim(self, w: f32, h: f32) -> Self {
        Self { w, h, ..self }
    }
}

pub struct SurfaceTexture {
    pub img: vk::Image,
    pub mem: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub samp: vk::Sampler,
    pub pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
    pub w: f32,
    pub h: f32,
    pub scale: i32,
}

use ash::vk;
use std::rc::Rc;
use vulkan::{Pipeline, device::SharedDeviceRef};

use crate::CameraUBO;

#[allow(dead_code)]
pub struct RenderContext {
    swapchain: vulkan::Swapchain,
    device: SharedDeviceRef,
    command_buffer_executed: Box<[vk::Fence]>,
    image_acquired: Box<[vk::Semaphore]>,
    render_complete: Box<[vk::Semaphore]>,
    command_infos: Box<[(vk::CommandPool, vk::CommandBuffer)]>,
    depth_images: Box<[vulkan::Image]>,
    pipeline: Rc<vulkan::Pipeline>,
    pub per_frame_buffer_element_size: u32,
    per_frame_buffer: vulkan::Buffer,
    pub index: usize,
}

pub const MAX_FRAME_COUNT: usize = 3;

impl RenderContext {
    pub fn new(
        device: SharedDeviceRef,
        pipeline_layout: Rc<vulkan::PipelineLayout>,
        window: &winit::window::Window,
        per_frame_ds: vk::DescriptorSet,
    ) -> crate::Result<RenderContext> {
        let swapchain = vulkan::Swapchain::new(device.clone(), window)
            .inspect_err(|e| tracing::error!("{e}"))?;

        let command_buffer_executed = {
            let mut fences: Vec<vk::Fence> = Vec::with_capacity(MAX_FRAME_COUNT);
            for _ in 0..MAX_FRAME_COUNT {
                let fence_create_info = ash::vk::FenceCreateInfo {
                    flags: vk::FenceCreateFlags::SIGNALED,
                    ..Default::default()
                };
                let fence =
                    unsafe { device.create_fence(&fence_create_info) }.inspect_err(|e| {
                        tracing::error!("{e}");
                        unsafe {
                            for f in fences.iter() {
                                device.destroy_fence(*f);
                            }
                        }
                    })?;
                fences.push(fence);
            }

            fences.into_boxed_slice()
        };

        let (per_frame_buffer, per_frame_buffer_element_size) = {
            let element_size = {
                let struct_size = std::mem::size_of::<CameraUBO>();

                let properties = unsafe { device.get_physical_device_properties() };

                struct_size.next_multiple_of(
                    properties.limits.min_uniform_buffer_offset_alignment as usize,
                )
            };

            let buffer = {
                let buffer_size = element_size * MAX_FRAME_COUNT;

                let buffer_create_info = vulkan::BufferCreateInfo {
                    size: buffer_size as u64,
                    usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                    memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                        | vk::MemoryPropertyFlags::HOST_VISIBLE,
                };

                vulkan::Buffer::new(device.clone(), &buffer_create_info)?
            };

            let buffer_info = vk::DescriptorBufferInfo {
                buffer: buffer.handle,
                offset: 0,
                range: element_size as u64,
            };

            let writes = [vk::WriteDescriptorSet {
                dst_set: per_frame_ds,
                dst_binding: 0,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                p_buffer_info: &buffer_info,
                ..Default::default()
            }];

            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }

            (buffer, element_size)
        };

        let (image_acquired, render_complete) = {
            let mut semaphores = Vec::with_capacity(swapchain.get_image_count() + MAX_FRAME_COUNT);

            for _ in 0..(swapchain.get_image_count() + MAX_FRAME_COUNT) {
                let semaphore_create_info = vk::SemaphoreCreateInfo {
                    ..Default::default()
                };
                let semaphore = unsafe { device.create_semaphore(&semaphore_create_info) }
                    .inspect_err(|e| {
                        tracing::error!("{}", e);
                        unsafe {
                            for s in semaphores.iter() {
                                device.destroy_semaphore(*s);
                            }
                            for fence in command_buffer_executed.iter() {
                                device.destroy_fence(*fence);
                            }
                        }
                    })?;
                semaphores.push(semaphore);
            }

            let completed = semaphores.split_off(MAX_FRAME_COUNT).into_boxed_slice();

            (semaphores.into_boxed_slice(), completed)
        };

        let command_infos = {
            let mut infos = Vec::with_capacity(MAX_FRAME_COUNT);

            for _ in 0..MAX_FRAME_COUNT {
                let pool = {
                    let pool_create_info = vk::CommandPoolCreateInfo {
                        flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                        queue_family_index: device.get_queue_family_index(),
                        ..Default::default()
                    };

                    unsafe { device.create_command_pool(&pool_create_info) }.inspect_err(|e| {
                        tracing::error!("{}", e);
                        unsafe {
                            for semaphore in image_acquired.iter() {
                                device.destroy_semaphore(*semaphore);
                            }
                            for semaphore in render_complete.iter() {
                                device.destroy_semaphore(*semaphore);
                            }
                            for fence in command_buffer_executed.iter() {
                                device.destroy_fence(*fence);
                            }
                        }
                    })?
                };
                let buffer = {
                    let buffer_allocate_info = ash::vk::CommandBufferAllocateInfo {
                        command_pool: pool,
                        command_buffer_count: 1,
                        level: vk::CommandBufferLevel::PRIMARY,
                        ..Default::default()
                    };

                    let buffers = unsafe { device.allocate_command_buffers(&buffer_allocate_info) }
                        .inspect_err(|e| {
                            tracing::error!("{}", e);
                            unsafe {
                                device.destroy_command_pool(pool);
                                for (pool, buffer) in infos.iter() {
                                    device.free_command_buffers(*pool, &[*buffer]);
                                    device.destroy_command_pool(*pool);
                                }
                                for semaphore in image_acquired.iter() {
                                    device.destroy_semaphore(*semaphore);
                                }
                                for semaphore in render_complete.iter() {
                                    device.destroy_semaphore(*semaphore);
                                }
                                for fence in command_buffer_executed.iter() {
                                    device.destroy_fence(*fence);
                                }
                            }
                        })?;

                    buffers[0]
                };

                infos.push((pool, buffer));
            }

            infos.into_boxed_slice()
        };

        let depth_stencil_format = device
            .find_viable_depth_stencil_format()
            .ok_or(vulkan::result::Error::CouldNotDetermineFormat)
            .inspect_err(|e| tracing::error!("{}", e))?;

        let depth_images = {
            let mut images = Vec::with_capacity(swapchain.get_image_count());

            let depth_image_create_info = vulkan::image::ImageCreateInfo {
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                mip_levels: 1,
                image_type: vk::ImageType::TYPE_2D,
                format: depth_stencil_format,
                width: swapchain.get_extent().width,
                height: swapchain.get_extent().height,
                depth: 1,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                array_layers: 1,
            };

            for _ in 0..swapchain.get_image_count() {
                let image = vulkan::image::Image::new(device.clone(), &depth_image_create_info)
                    .inspect_err(|e| {
                        tracing::error!("{}", e);
                        unsafe {
                            for (pool, buffer) in command_infos.iter() {
                                device.free_command_buffers(*pool, &[*buffer]);
                                device.destroy_command_pool(*pool);
                            }
                            for semaphore in image_acquired.iter() {
                                device.destroy_semaphore(*semaphore);
                            }
                            for semaphore in render_complete.iter() {
                                device.destroy_semaphore(*semaphore);
                            }
                            for fence in command_buffer_executed.iter() {
                                device.destroy_fence(*fence);
                            }
                        }
                    })?;
                images.push(image);
            }

            images.into_boxed_slice()
        };

        let pipeline: Rc<vulkan::Pipeline> = {
            let vert_entry_point_name =
                std::ffi::CString::new(crate::ENTRY_POINT_NAME_SHADER_VERT).unwrap();
            let frag_entry_point_name =
                std::ffi::CString::new(crate::ENTRY_POINT_NAME_SHADER_FRAG).unwrap();

            // TODO: convert crate::VERT_SHADER_PATH and crate::FRAG_SHADER_PATH into macros?
            const COMPILED_VERT_SHADER: &[u8] = include_bytes!("../shaders/shader.vert.spv");
            const COMPILED_FRAG_SHADER: &[u8] = include_bytes!("../shaders/shader.frag.spv");

            let vert_shader_module =
                vulkan::ShaderModule::from_compiled_spv(COMPILED_VERT_SHADER, device.clone())?;
            let frag_shader_module =
                vulkan::ShaderModule::from_compiled_spv(COMPILED_FRAG_SHADER, device.clone())?;

            let stages = {
                let vert_stage = vk::PipelineShaderStageCreateInfo {
                    stage: vk::ShaderStageFlags::VERTEX,
                    module: unsafe { *vert_shader_module.raw() },
                    p_name: vert_entry_point_name.as_ptr(),
                    ..Default::default()
                };
                let frag_stage = vk::PipelineShaderStageCreateInfo {
                    stage: vk::ShaderStageFlags::FRAGMENT,
                    module: unsafe { *frag_shader_module.raw() },
                    p_name: frag_entry_point_name.as_ptr(),
                    ..Default::default()
                };
                [vert_stage, frag_stage]
            };

            let (vertex_input_attributes, vertex_input_bindings) = {
                let vk_input_attributes = [
                    vk::VertexInputAttributeDescription {
                        location: 0,
                        binding: 0,
                        format: vk::Format::R32G32B32_SFLOAT,
                        offset: std::mem::offset_of!(crate::ShaderVertVertex, position) as u32,
                    },
                    vk::VertexInputAttributeDescription {
                        location: 1,
                        binding: 0,
                        format: vk::Format::R32G32_SFLOAT,
                        offset: std::mem::offset_of!(crate::ShaderVertVertex, tex_coord) as u32,
                    },
                    vk::VertexInputAttributeDescription {
                        location: 2,
                        binding: 0,
                        format: vk::Format::R32G32B32_SFLOAT,
                        offset: std::mem::offset_of!(crate::ShaderVertVertex, normal) as u32,
                    },
                ];

                let vk_binding_descriptions = [vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: std::mem::size_of::<crate::ShaderVertVertex>() as u32,
                    input_rate: vk::VertexInputRate::VERTEX,
                }];

                (vk_input_attributes, vk_binding_descriptions)
            };
            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo {
                vertex_binding_description_count: vertex_input_bindings.len() as u32,
                p_vertex_binding_descriptions: vertex_input_bindings.as_ptr(),
                vertex_attribute_description_count: vertex_input_attributes.len() as u32,
                p_vertex_attribute_descriptions: vertex_input_attributes.as_ptr(),
                ..Default::default()
            };
            let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo {
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                primitive_restart_enable: vk::FALSE,
                ..Default::default()
            };
            let viewport_state = vk::PipelineViewportStateCreateInfo {
                viewport_count: 1,
                p_viewports: std::ptr::null(), // Since dynamic viewports is enabled this can be null
                scissor_count: 1,
                p_scissors: std::ptr::null(), // this is also be dynamic
                ..Default::default()
            };
            let rasterization_state = vk::PipelineRasterizationStateCreateInfo {
                depth_clamp_enable: vk::FALSE,
                rasterizer_discard_enable: vk::FALSE,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                depth_bias_enable: vk::FALSE,
                depth_bias_constant_factor: 0.0,
                depth_bias_clamp: 0.0,
                depth_bias_slope_factor: 0.0,
                line_width: 1.0, // dyamic states is on and VK_DYNAMIC_STATE_LINE_WIDTH is not
                ..Default::default()
            };
            let multisample_state = vk::PipelineMultisampleStateCreateInfo {
                rasterization_samples: vk::SampleCountFlags::TYPE_1,
                sample_shading_enable: vk::FALSE,
                ..Default::default()
            };
            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo {
                depth_test_enable: vk::TRUE,
                depth_write_enable: vk::TRUE,
                depth_compare_op: vk::CompareOp::LESS,
                depth_bounds_test_enable: vk::FALSE,
                stencil_test_enable: vk::FALSE,
                min_depth_bounds: 0.0,
                max_depth_bounds: 1.0,
                ..Default::default()
            };
            let attachments = [vk::PipelineColorBlendAttachmentState {
                blend_enable: vk::TRUE,
                src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
                dst_color_blend_factor: vk::BlendFactor::ZERO,
                color_blend_op: vk::BlendOp::ADD,
                src_alpha_blend_factor: vk::BlendFactor::ZERO,
                dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                alpha_blend_op: vk::BlendOp::ADD,
                color_write_mask: vk::ColorComponentFlags::RGBA,
            }];
            let color_blend_state = vk::PipelineColorBlendStateCreateInfo {
                logic_op_enable: vk::FALSE,
                logic_op: vk::LogicOp::COPY,
                attachment_count: attachments.len() as u32,
                p_attachments: attachments.as_ptr(),
                blend_constants: [0.0, 0.0, 0.0, 0.0],
                ..Default::default()
            };
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic_state = vk::PipelineDynamicStateCreateInfo {
                dynamic_state_count: dynamic_states.len() as u32,
                p_dynamic_states: dynamic_states.as_ptr(),
                ..Default::default()
            };
            let color_formats = [swapchain.get_format()];
            let pipeline_rendering_info = vk::PipelineRenderingCreateInfo {
                color_attachment_count: color_formats.len() as u32,
                p_color_attachment_formats: color_formats.as_ptr(),
                depth_attachment_format: depth_stencil_format,
                stencil_attachment_format: depth_stencil_format,
                ..Default::default()
            };
            let pipeline_create_info = vk::GraphicsPipelineCreateInfo {
                p_next: &pipeline_rendering_info as *const _ as *const std::ffi::c_void,
                stage_count: stages.len() as u32,
                p_stages: stages.as_ptr(),
                p_vertex_input_state: &vertex_input_state,
                p_input_assembly_state: &input_assembly_state,
                p_tessellation_state: std::ptr::null(),
                p_viewport_state: &viewport_state,
                p_rasterization_state: &rasterization_state,
                p_multisample_state: &multisample_state,
                p_depth_stencil_state: &depth_stencil_state,
                p_color_blend_state: &color_blend_state,
                p_dynamic_state: &dynamic_state,
                layout: pipeline_layout.handle,
                render_pass: vk::RenderPass::null(), // dynamic rendering is enabled
                subpass: 0,
                ..Default::default()
            };

            Rc::new(Pipeline::new_graphics(
                device.clone(),
                pipeline_layout,
                &pipeline_create_info,
            )?)
        };

        Ok(RenderContext {
            device,
            swapchain,
            command_buffer_executed,
            image_acquired,
            render_complete,
            command_infos,
            depth_images,
            pipeline,
            per_frame_buffer_element_size: per_frame_buffer_element_size as u32,
            per_frame_buffer,
            index: 0,
        })
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            for (pool, buffer) in self.command_infos.iter_mut() {
                self.device.free_command_buffers(*pool, &[*buffer]);
                self.device.destroy_command_pool(*pool);
            }
            for semaphore in self.render_complete.iter_mut() {
                self.device.destroy_semaphore(*semaphore);
            }
            for semaphore in self.image_acquired.iter_mut() {
                self.device.destroy_semaphore(*semaphore);
            }
            for fence in self.command_buffer_executed.iter_mut() {
                self.device.destroy_fence(*fence);
            }
        }
    }
}

impl RenderContext {
    pub fn get_pipeline(&self) -> Rc<vulkan::Pipeline> {
        self.pipeline.clone()
    }
    pub fn update_camera(&self, camera_ubo: crate::CameraUBO) -> crate::Result<()> {
        let element_size = {
            let struct_size = std::mem::size_of::<CameraUBO>() as vk::DeviceSize;

            let properties = unsafe { self.device.get_physical_device_properties() };

            struct_size.next_multiple_of(
                properties.limits.min_uniform_buffer_offset_alignment as vk::DeviceSize,
            )
        };

        let offset = self.index as vk::DeviceSize * element_size;

        unsafe {
            let dst = self.per_frame_buffer.map_memory(offset, element_size)? as *mut CameraUBO;

            *dst = camera_ubo;

            self.per_frame_buffer.unmap();
        }

        Ok(())
    }
    pub unsafe fn draw<F>(&mut self, record_draw_commands: F) -> vulkan::result::Result<()>
    where
        F: FnOnce(vk::CommandBuffer),
    {
        // Acquire image
        let (swapchain_image_index, swapchain_image_view) = {
            unsafe {
                self.device.wait_for_fences(
                    &[self.command_buffer_executed[self.index]],
                    true,
                    u64::MAX,
                )?
            };

            let (image_index, _) = unsafe {
                self.swapchain
                    .acquire_next_image(self.image_acquired[self.index], vk::Fence::null())?
            };

            unsafe {
                self.device
                    .reset_fences(&[self.command_buffer_executed[self.index]])?
            };
            (
                image_index as usize,
                self.swapchain.get_image_view(image_index as usize).unwrap(),
            )
        };

        let (_, command_buffer) = self.command_infos.get(self.index).unwrap();

        // Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo {
            flags: ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            ..Default::default()
        };

        unsafe {
            // Reset the command buffer (requires pool/reset capability)
            self.device
                .reset_command_buffer(*command_buffer, vk::CommandBufferResetFlags::empty())?;

            self.device
                .begin_command_buffer(*command_buffer, &begin_info)?;
        }

        {
            let color_barrier = ash::vk::ImageMemoryBarrier2 {
                src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                src_access_mask: vk::AccessFlags2::empty(),
                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                image: *self.swapchain.get_image(swapchain_image_index).unwrap(),
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                ..Default::default()
            };
            let depth_barrier = vk::ImageMemoryBarrier2 {
                src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                src_access_mask: vk::AccessFlags2::empty(),
                dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                image: self.depth_images.get(swapchain_image_index).unwrap().handle,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                ..Default::default()
            };

            let dependencies = [color_barrier, depth_barrier];
            let dependency_info = vk::DependencyInfo {
                image_memory_barrier_count: dependencies.len() as u32,
                p_image_memory_barriers: dependencies.as_ptr(),
                ..Default::default()
            };
            unsafe {
                self.device
                    .cmd_pipeline_barrier2(*command_buffer, &dependency_info)
            };
        }

        // begin dynamic rendering
        {
            let color_attachment_info = vk::RenderingAttachmentInfo {
                image_view: *swapchain_image_view,
                image_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 0.0],
                    },
                },
                ..Default::default()
            };

            let depth_image = self.depth_images.get(swapchain_image_index).unwrap();
            let depth_attachment_info = ash::vk::RenderingAttachmentInfo {
                image_view: depth_image.view,
                image_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                clear_value: vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
                ..Default::default()
            };

            let rendering_info = ash::vk::RenderingInfo {
                render_area: vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: *self.swapchain.get_extent(),
                },
                layer_count: 1,
                view_mask: 0,
                color_attachment_count: 1,
                p_color_attachments: &color_attachment_info,
                p_depth_attachment: &depth_attachment_info,
                ..Default::default()
            };

            let viewport = ash::vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain.get_extent().width as f32,
                height: self.swapchain.get_extent().height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: *self.swapchain.get_extent(),
            };
            unsafe {
                self.device
                    .cmd_begin_rendering(*command_buffer, &rendering_info);

                self.device
                    .cmd_set_viewport(*command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(*command_buffer, 0, &[scissor]);
            };
        }

        record_draw_commands(*command_buffer);

        // End rendering & end command buffer
        unsafe {
            self.device.cmd_end_rendering(*command_buffer);
        }

        // Barrier to transition for pres
        {
            let dependencies = [vk::ImageMemoryBarrier2 {
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                dst_access_mask: vk::AccessFlags2::empty(),
                old_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                new_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                image: *self.swapchain.get_image(swapchain_image_index).unwrap(),
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                ..Default::default()
            }];
            let dependency_info = vk::DependencyInfo {
                image_memory_barrier_count: dependencies.len() as u32,
                p_image_memory_barriers: dependencies.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device
                    .cmd_pipeline_barrier2(*command_buffer, &dependency_info)
            };
        }

        unsafe {
            self.device
                .end_command_buffer(*command_buffer)
                .inspect_err(|e| tracing::error!("{}", e))?;
        }

        // Submit
        {
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let wait_semaphores = [self.image_acquired[self.index]];
            let signal_semaphores = [self.render_complete[self.index]];
            let command_buffers = [*command_buffer];

            let submit_info = vk::SubmitInfo {
                wait_semaphore_count: wait_semaphores.len() as u32,
                p_wait_semaphores: wait_semaphores.as_ptr(),
                p_wait_dst_stage_mask: wait_stages.as_ptr(),
                command_buffer_count: command_buffers.len() as u32,
                p_command_buffers: command_buffers.as_ptr(),
                signal_semaphore_count: signal_semaphores.len() as u32,
                p_signal_semaphores: signal_semaphores.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device.queue_submit(
                    self.device.queue,
                    &[submit_info],
                    *self.command_buffer_executed.get(self.index).unwrap(),
                )?
            };

            let present_wait_semaphores = signal_semaphores;
            let present_info = vk::PresentInfoKHR {
                wait_semaphore_count: present_wait_semaphores.len() as u32,
                p_wait_semaphores: present_wait_semaphores.as_ptr(),
                swapchain_count: 1,
                p_swapchains: unsafe { self.swapchain.get_swapchain_ptr() },
                p_image_indices: &(swapchain_image_index as u32),
                ..Default::default()
            };
            unsafe { self.device.queue_present(&present_info)? };
        }

        self.index += 1;
        let max_frames = match self.swapchain.get_present_mode() {
            vk::PresentModeKHR::MAILBOX => 3,
            _ => 2,
        };
        self.index %= max_frames;

        Ok(())
    }
}

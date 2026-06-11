use ash::vk;
use vulkan::device::SharedDeviceRef;

use crate::{
    CameraUBO, InstanceData, PipelineLayoutResourceHandle, PipelineLayoutResourceManager,
    PipelineResourceHandle, PipelineResourceManager, Result, ShaderModuleDescription,
    ShaderModuleResourceManager,
};

pub const MAX_FRAME_COUNT: u64 = 3;
pub const MAX_CAMERA_DATA_COUNT: u64 = 32;
pub const MAX_INSTANCE_DATA_COUNT: u64 = 128;
pub const MAX_INDIRECT_COMMAND_DATA_COUNT: u64 = MAX_INSTANCE_DATA_COUNT * 4;

#[allow(dead_code)]
pub struct FrameAllocator {
    uniform_buffer: vulkan::Buffer,
    uniform_buffer_offset: u64,
    storage_buffer: vulkan::Buffer,
    storage_buffer_offset: u64,
    indirect_buffer: vulkan::Buffer,
    indirect_buffer_offset: u64,
}

#[allow(dead_code)]
impl FrameAllocator {
    pub fn new(
        device: SharedDeviceRef,
        uniform_buffer_capcity: u64,
        storage_buffer_capacity: u64,
        indirect_buffer_capacity: u64,
    ) -> Result<Self> {
        let uniform_buffer = {
            let create_info = vulkan::BufferCreateInfo {
                size: uniform_buffer_capcity,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::HOST_VISIBLE,
            };

            vulkan::Buffer::new(device.clone(), &create_info)?
        };

        let storage_buffer = {
            let create_info = vulkan::BufferCreateInfo {
                size: storage_buffer_capacity,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::HOST_VISIBLE,
            };

            vulkan::Buffer::new(device.clone(), &create_info)?
        };

        let indirect_buffer = {
            let create_info = vulkan::BufferCreateInfo {
                size: indirect_buffer_capacity,
                usage: vk::BufferUsageFlags::INDIRECT_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::HOST_VISIBLE,
            };

            vulkan::Buffer::new(device, &create_info)?
        };

        Ok(Self {
            uniform_buffer,
            uniform_buffer_offset: 0,
            storage_buffer,
            storage_buffer_offset: 0,
            indirect_buffer,
            indirect_buffer_offset: 0,
        })
    }
    pub fn upload_uniform_data<T>(&mut self, data: &[T], alignment: u64) -> Result<u64> {
        let (buffer, offset) = (&self.uniform_buffer, &mut self.uniform_buffer_offset);
        let res = *offset;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;
        unsafe {
            let dst = buffer.map_memory(*offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }
        *offset += size;
        *offset = offset.next_multiple_of(alignment);

        Ok(res)
    }
    pub fn upload_storage_data<T>(&mut self, data: &[T], alignment: u64) -> Result<u64> {
        let (buffer, offset) = (&self.storage_buffer, &mut self.storage_buffer_offset);
        let res = *offset;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;
        unsafe {
            let dst = buffer.map_memory(*offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }
        *offset += size;
        *offset = offset.next_multiple_of(alignment);

        Ok(res)
    }
    pub fn upload_indirect_data<T>(&mut self, data: &[T], alignment: u64) -> Result<u64> {
        let (buffer, offset) = (&self.indirect_buffer, &mut self.indirect_buffer_offset);
        let res = *offset;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;
        unsafe {
            let dst = buffer.map_memory(*offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }
        *offset += size;
        *offset = offset.next_multiple_of(alignment);

        Ok(res)
    }
    #[inline]
    pub fn reset(&mut self) {
        self.uniform_buffer_offset = 0;
        self.storage_buffer_offset = 0;
        self.indirect_buffer_offset = 0;
    }
    #[inline]
    pub fn uniform_buffer_raw(&self) -> vk::Buffer {
        self.uniform_buffer.handle
    }
    #[inline]
    pub fn storage_buffer_raw(&self) -> vk::Buffer {
        self.storage_buffer.handle
    }
    #[inline]
    pub fn indirect_buffer_raw(&self) -> vk::Buffer {
        self.indirect_buffer.handle
    }
}

#[allow(dead_code)]
pub struct FrameData {
    device: SharedDeviceRef,
    command_buffer_executed: vk::Fence,
    image_acquired: vk::Semaphore,
    render_complete: vk::Semaphore,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    main_per_frame_descriptor_set: vk::DescriptorSet,
    grid_per_frame_descriptor_set: vk::DescriptorSet,
    camera_data_element_size: u64,
    instance_data_element_size: u64,
    indirect_command_data_element_size: u64,
    allocator: FrameAllocator,
}

#[allow(unused)]
impl FrameData {
    pub fn new(
        device: SharedDeviceRef,
        main_per_frame_descriptor_set: vk::DescriptorSet,
        grid_per_frame_descriptor_set: vk::DescriptorSet,
    ) -> Result<Self> {
        let camera_data_element_size = {
            let size = std::mem::size_of::<CameraUBO>() as u64;
            let properties = unsafe { device.get_physical_device_properties() };

            size.next_multiple_of(properties.limits.min_uniform_buffer_offset_alignment)
        };

        let instance_data_element_size = {
            let size = std::mem::size_of::<InstanceData>() as u64;
            let properties = unsafe { device.get_physical_device_properties() };

            size.next_multiple_of(properties.limits.min_storage_buffer_offset_alignment)
        };

        let indirect_command_data_element_size =
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;

        let allocator = FrameAllocator::new(
            device.clone(),
            camera_data_element_size * MAX_CAMERA_DATA_COUNT,
            instance_data_element_size * MAX_INSTANCE_DATA_COUNT,
            indirect_command_data_element_size * MAX_INDIRECT_COMMAND_DATA_COUNT,
        )?;

        {
            let main_camera_buffer_info = [vk::DescriptorBufferInfo {
                buffer: allocator.uniform_buffer_raw(),
                offset: 0,
                range: camera_data_element_size,
            }];
            let instance_buffer_info = [vk::DescriptorBufferInfo {
                buffer: allocator.storage_buffer_raw(),
                offset: 0,
                range: instance_data_element_size * MAX_INSTANCE_DATA_COUNT,
            }];
            let grid_camera_buffer_info = [vk::DescriptorBufferInfo {
                buffer: allocator.uniform_buffer_raw(),
                offset: 0,
                range: camera_data_element_size,
            }];
            let writes = [
                vk::WriteDescriptorSet {
                    dst_set: main_per_frame_descriptor_set,
                    dst_binding: 0,
                    dst_array_element: 0,
                    descriptor_count: 1,
                    p_buffer_info: main_camera_buffer_info.as_ptr(),
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: main_per_frame_descriptor_set,
                    dst_binding: 1,
                    dst_array_element: 0,
                    descriptor_count: 1,
                    p_buffer_info: instance_buffer_info.as_ptr(),
                    descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: grid_per_frame_descriptor_set,
                    dst_binding: 0,
                    dst_array_element: 0,
                    descriptor_count: 1,
                    p_buffer_info: grid_camera_buffer_info.as_ptr(),
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                    ..Default::default()
                },
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        let command_buffer_executed = {
            let create_info = vk::FenceCreateInfo {
                flags: vk::FenceCreateFlags::SIGNALED,
                ..Default::default()
            };
            unsafe { device.create_fence(&create_info) }?
        };

        let image_acquired = {
            let create_info = vk::SemaphoreCreateInfo {
                ..Default::default()
            };
            unsafe { device.create_semaphore(&create_info) }.inspect_err(|_| unsafe {
                device.destroy_fence(command_buffer_executed);
            })?
        };

        let render_complete = {
            let create_info = vk::SemaphoreCreateInfo {
                ..Default::default()
            };
            unsafe { device.create_semaphore(&create_info) }.inspect_err(|_| unsafe {
                device.destroy_semaphore(image_acquired);
                device.destroy_fence(command_buffer_executed);
            })?
        };

        let (command_pool, command_buffer) = {
            let command_pool = {
                let create_info = vk::CommandPoolCreateInfo {
                    queue_family_index: device.get_queue_family_index(),
                    flags: vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                    ..Default::default()
                };

                unsafe { device.create_command_pool(&create_info) }.inspect_err(|_| unsafe {
                    device.destroy_semaphore(render_complete);
                    device.destroy_semaphore(image_acquired);
                    device.destroy_fence(command_buffer_executed);
                })?
            };

            let command_buffer = {
                let allocate_info = vk::CommandBufferAllocateInfo {
                    command_pool,
                    command_buffer_count: 1,
                    level: vk::CommandBufferLevel::PRIMARY,
                    ..Default::default()
                };

                let buffers = unsafe { device.allocate_command_buffers(&allocate_info) }
                    .inspect_err(|_| unsafe {
                        device.destroy_command_pool(command_pool);
                        device.destroy_semaphore(render_complete);
                        device.destroy_semaphore(image_acquired);
                        device.destroy_fence(command_buffer_executed);
                    })?;
                buffers[0]
            };

            (command_pool, command_buffer)
        };

        Ok(Self {
            device,
            command_buffer_executed,
            image_acquired,
            render_complete,
            command_pool,
            command_buffer,
            main_per_frame_descriptor_set,
            grid_per_frame_descriptor_set,
            camera_data_element_size,
            instance_data_element_size,
            indirect_command_data_element_size,
            allocator,
        })
    }
    #[inline]
    pub fn allocator_mut(&mut self) -> &mut FrameAllocator {
        &mut self.allocator
    }
    #[inline]
    pub fn camera_data_element_size(&self) -> u64 {
        self.camera_data_element_size
    }
    #[inline]
    pub fn instance_data_element_size(&self) -> u64 {
        self.instance_data_element_size
    }
    #[inline]
    pub fn indirect_command_data_element_size(&self) -> u64 {
        self.indirect_command_data_element_size
    }
    #[inline]
    pub fn main_per_frame_descriptor_set(&self) -> vk::DescriptorSet {
        self.main_per_frame_descriptor_set
    }
    #[inline]
    pub fn grid_per_frame_descriptor_set(&self) -> vk::DescriptorSet {
        self.grid_per_frame_descriptor_set
    }
}

impl Drop for FrameData {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool);
            self.device.destroy_fence(self.command_buffer_executed);
            self.device.destroy_semaphore(self.image_acquired);
            self.device.destroy_semaphore(self.render_complete);
        }
    }
}

#[allow(dead_code)]
pub struct FrameContext {
    device: SharedDeviceRef,
    swapchain: vulkan::Swapchain,
    depth_images: Box<[vulkan::Image]>,
    main_pipeline: PipelineResourceHandle,
    grid_pipeline: PipelineResourceHandle,
    descriptor_pool: vk::DescriptorPool,
    frames: Box<[FrameData]>,
    pub index: usize,
}

impl FrameContext {
    pub fn new(
        device: SharedDeviceRef,
        window: &winit::window::Window,
        main_per_frame_ds_layout: vk::DescriptorSetLayout,
        main_pipeline_layout: PipelineLayoutResourceHandle,
        main_vert_shader_desc: ShaderModuleDescription,
        main_frag_shader_desc: ShaderModuleDescription,

        grid_per_frame_ds_layout: vk::DescriptorSetLayout,
        grid_pipeline_layout: PipelineLayoutResourceHandle,
        grid_vert_shader_desc: ShaderModuleDescription,
        grid_frag_shader_desc: ShaderModuleDescription,

        shader_modules: &mut ShaderModuleResourceManager,
        pipeline_layouts: &mut PipelineLayoutResourceManager,
        pipelines: &mut PipelineResourceManager,
    ) -> Result<Self> {
        let swapchain = vulkan::Swapchain::new(device.clone(), window)
            .inspect_err(|e| tracing::error!("{e}"))?;

        let depth_stencil_format = device
            .find_viable_depth_stencil_format()
            .ok_or(vulkan::result::Error::CouldNotDetermineFormat)?;

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
                    .inspect_err(|e| tracing::error!("{}", e))?;
                images.push(image);
            }

            images.into_boxed_slice()
        };

        let main_pipeline = {
            let vert_shader = shader_modules.access_or_create(main_vert_shader_desc)?;
            let frag_shader = shader_modules.access_or_create(main_frag_shader_desc)?;
            let desc = crate::PipelineDescription::DynamicGraphics {
                pipeline_layout: main_pipeline_layout,
                vert_shader,
                frag_shader,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                color_format: swapchain.get_format(),
                depth_format: depth_stencil_format,
                samples: vk::SampleCountFlags::TYPE_1,
            };
            pipelines.access_or_create(desc, pipeline_layouts, shader_modules)?
        };

        let grid_pipeline = {
            let vert_shader = shader_modules.access_or_create(grid_vert_shader_desc)?;
            let frag_shader = shader_modules.access_or_create(grid_frag_shader_desc)?;
            let desc = crate::PipelineDescription::DynamicGraphics {
                pipeline_layout: grid_pipeline_layout,
                vert_shader,
                frag_shader,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                color_format: swapchain.get_format(),
                depth_format: depth_stencil_format,
                samples: vk::SampleCountFlags::TYPE_1,
            };
            pipelines.access_or_create(desc, pipeline_layouts, shader_modules)?
        };

        let (descriptor_pool, main_descriptor_sets, grid_descriptor_sets) = {
            let descriptor_pool = {
                let pool_sizes = [
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                        descriptor_count: MAX_FRAME_COUNT as u32 * 2,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: MAX_FRAME_COUNT as u32,
                    },
                ];
                let create_info = vk::DescriptorPoolCreateInfo {
                    max_sets: MAX_FRAME_COUNT as u32 * 2,
                    p_pool_sizes: pool_sizes.as_ptr(),
                    pool_size_count: pool_sizes.len() as u32,
                    ..Default::default()
                };

                unsafe { device.create_descriptor_pool(&create_info) }?
            };

            let main_descriptor_sets = {
                let set_layouts = [main_per_frame_ds_layout; MAX_FRAME_COUNT as usize];
                let allocate_info = vk::DescriptorSetAllocateInfo {
                    descriptor_pool,
                    descriptor_set_count: set_layouts.len() as u32,
                    p_set_layouts: set_layouts.as_ptr(),
                    ..Default::default()
                };

                unsafe { device.allocate_descriptor_sets(&allocate_info) }.inspect_err(|e| {
                    tracing::error!("{e}");
                    unsafe {
                        device.destroy_descriptor_pool(descriptor_pool);
                    }
                })?
            };

            let grid_descriptor_sets = {
                let set_layouts = [grid_per_frame_ds_layout; MAX_FRAME_COUNT as usize];
                let allocate_info = vk::DescriptorSetAllocateInfo {
                    descriptor_pool,
                    descriptor_set_count: set_layouts.len() as u32,
                    p_set_layouts: set_layouts.as_ptr(),
                    ..Default::default()
                };

                unsafe { device.allocate_descriptor_sets(&allocate_info) }.inspect_err(|e| {
                    tracing::error!("{e}");
                    unsafe {
                        device.destroy_descriptor_pool(descriptor_pool);
                    }
                })?
            };

            (descriptor_pool, main_descriptor_sets, grid_descriptor_sets)
        };

        let mut frames = Vec::<FrameData>::with_capacity(MAX_FRAME_COUNT as usize);
        for (main_descriptor_set, grid_descriptor_set) in
            main_descriptor_sets.iter().zip(grid_descriptor_sets.iter())
        {
            let frame = FrameData::new(device.clone(), *main_descriptor_set, *grid_descriptor_set)
                .inspect_err(|_| unsafe {
                    device.destroy_descriptor_pool(descriptor_pool);
                })?;
            frames.push(frame);
        }

        Ok(Self {
            device,
            swapchain,
            frames: frames.into_boxed_slice(),
            depth_images,
            main_pipeline,
            grid_pipeline,
            descriptor_pool,
            index: 0,
        })
    }
}

impl Drop for FrameContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

impl FrameContext {
    pub fn get_main_pipeline(&self) -> PipelineResourceHandle {
        self.main_pipeline
    }
    pub fn get_grid_pipeline(&self) -> PipelineResourceHandle {
        self.grid_pipeline
    }
    pub fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.index]
    }
    pub fn get_current_frame_mut(&mut self) -> &mut FrameData {
        &mut self.frames[self.index]
    }
    pub fn swapchain_extent(&self) -> vk::Extent2D {
        *self.swapchain.get_extent()
    }
    pub unsafe fn draw<F>(&mut self, record_draw_commands: F) -> vulkan::result::Result<()>
    where
        F: FnOnce(vk::CommandBuffer),
    {
        let frame = self.get_current_frame();

        // Acquire image
        let (swapchain_image_index, swapchain_image_view) = {
            unsafe {
                self.device
                    .wait_for_fences(&[frame.command_buffer_executed], true, u64::MAX)?
            };

            let (image_index, _) = unsafe {
                self.swapchain
                    .acquire_next_image(frame.image_acquired, vk::Fence::null())?
            };

            unsafe { self.device.reset_fences(&[frame.command_buffer_executed])? };
            (
                image_index as usize,
                self.swapchain.get_image_view(image_index as usize).unwrap(),
            )
        };

        // Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo {
            flags: ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            ..Default::default()
        };

        unsafe {
            // Reset the command buffer (requires pool/reset capability)
            self.device
                .reset_command_buffer(frame.command_buffer, vk::CommandBufferResetFlags::empty())?;

            self.device
                .begin_command_buffer(frame.command_buffer, &begin_info)?;
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
                    .cmd_pipeline_barrier2(frame.command_buffer, &dependency_info)
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

            unsafe {
                self.device
                    .cmd_begin_rendering(frame.command_buffer, &rendering_info);
            };
        }

        record_draw_commands(frame.command_buffer);

        // End rendering & end command buffer
        unsafe {
            self.device.cmd_end_rendering(frame.command_buffer);
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
                    .cmd_pipeline_barrier2(frame.command_buffer, &dependency_info)
            };
        }

        unsafe {
            self.device
                .end_command_buffer(frame.command_buffer)
                .inspect_err(|e| tracing::error!("{}", e))?;
        }

        // Submit
        {
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let wait_semaphores = [frame.image_acquired];
            let signal_semaphores = [frame.render_complete];
            let command_buffers = [frame.command_buffer];

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
                    frame.command_buffer_executed,
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

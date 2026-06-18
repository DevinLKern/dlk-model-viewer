use ash::vk;
use vulkan::device::SharedDeviceRef;

use crate::{CameraUBO, InstanceData, Result};

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
    #[inline]
    pub fn storage_buffer_offset(&self) -> u64 {
        self.storage_buffer_offset
    }
}

pub struct FrameData {
    device: SharedDeviceRef,
    command_buffer_executed: vk::Fence,
    image_acquired: vk::Semaphore,
    render_complete: vk::Semaphore,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    allocator: FrameAllocator,
}

impl std::fmt::Debug for FrameData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FrameData")
    }
}

#[allow(unused)]
impl FrameData {
    pub fn new(device: SharedDeviceRef) -> Result<Self> {
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
            allocator,
        })
    }
    #[inline]
    pub fn allocator(&self) -> &FrameAllocator {
        &self.allocator
    }
    #[inline]
    pub fn allocator_mut(&mut self) -> &mut FrameAllocator {
        &mut self.allocator
    }
    #[inline]
    pub fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
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
    depth_format: vk::Format,
    frames: [FrameData; MAX_FRAME_COUNT as usize],
    pub index: usize,
}

impl FrameContext {
    pub fn new(device: SharedDeviceRef, window: &winit::window::Window) -> Result<Self> {
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

        let mut frames = Vec::<FrameData>::with_capacity(MAX_FRAME_COUNT as usize);
        for _ in 0..MAX_FRAME_COUNT {
            let frame = FrameData::new(device.clone())?;
            frames.push(frame);
        }
        let frames: [FrameData; MAX_FRAME_COUNT as usize] =
            frames.try_into().expect("Incorrect number of frames");

        Ok(Self {
            device,
            swapchain,
            frames,
            depth_images,
            depth_format: depth_stencil_format,
            index: 0,
        })
    }
}

impl Drop for FrameContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }
}

impl FrameContext {
    #[inline]
    pub fn get_color_format(&self) -> vk::Format {
        self.swapchain.get_format()
    }
    #[inline]
    pub fn depth_format(&self) -> vk::Format {
        self.depth_format
    }
    pub fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.index]
    }
    #[inline]
    pub(crate) fn frames(&self) -> &[FrameData] {
        &self.frames
    }
    pub fn get_current_frame_mut(&mut self) -> &mut FrameData {
        &mut self.frames[self.index]
    }
    pub fn swapchain_extent(&self) -> vk::Extent2D {
        *self.swapchain.get_extent()
    }
    pub unsafe fn draw<F>(&mut self, record_draw_commands: F) -> Result<()>
    where
        F: FnOnce(&mut FrameContext) -> Result<()>,
    {
        // Acquire image
        let (swapchain_image_index, swapchain_image_view) = {
            let frame = self.get_current_frame();

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
            let frame = self.get_current_frame();
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
                let frame = self.get_current_frame();
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
                let frame = self.get_current_frame();
                self.device
                    .cmd_begin_rendering(frame.command_buffer, &rendering_info);
            };
        }

        record_draw_commands(self)?;
        let frame = self.get_current_frame();

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
                    .cmd_pipeline_barrier2(frame.command_buffer(), &dependency_info)
            };
        }

        unsafe {
            self.device
                .end_command_buffer(self.get_current_frame().command_buffer)
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

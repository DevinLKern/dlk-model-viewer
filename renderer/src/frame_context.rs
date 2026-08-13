use ash::vk;
use vulkan::device::SharedDeviceRef;

use crate::{CameraUBO, Error, ImageHandle, InstanceData, Renderer, Result};

pub const MAX_FRAME_COUNT: u64 = 3;
pub const MAX_CAMERA_DATA_COUNT: u64 = 32;
pub const MAX_INSTANCE_DATA_COUNT: u64 = 128;
pub const MAX_INDIRECT_COMMAND_DATA_COUNT: u64 = MAX_INSTANCE_DATA_COUNT * 4;

#[allow(dead_code)]
pub struct Attachment {
    pub img: ImageHandle,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub clear_val: vk::ClearValue,
    pub final_layout: vk::ImageLayout,
    pub resolve_img: Option<ImageHandle>,
}

#[allow(dead_code)]
pub struct RenderTarget {
    pub color: Box<[Attachment]>,
    pub depth: Option<Attachment>,
    pub render_area: vk::Rect2D,
    // pub(crate) stencil: Option<Attachment>,
}

impl RenderTarget {
    pub fn begin_rendering(&self, renderer: &mut Renderer, cmd: vk::CommandBuffer) -> Result<()> {
        let mut barriers = Vec::with_capacity(self.color.len() + 2);
        let mut color_attachments = Vec::with_capacity(self.color.len());
        let mut depth_attachment = vk::RenderingAttachmentInfo::default();

        for attachment in self.color.iter() {
            let (resolve_mode, resolve_image_view, resolve_image_layout) =
                match attachment.resolve_img {
                    Some(handle) => {
                        let img = renderer
                            .get_image_mut(handle)
                            .ok_or(Error::ResourceMissing)?;
                        let new_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
                        if img.layout != new_layout {
                            barriers.push(vk::ImageMemoryBarrier2 {
                                src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                                src_access_mask: vk::AccessFlags2::empty(),
                                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                old_layout: img.layout,
                                new_layout,
                                image: img.handle,
                                subresource_range: vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR,
                                    base_mip_level: 0,
                                    level_count: img.mip_level_count,
                                    base_array_layer: 0,
                                    layer_count: img.layer_count,
                                },
                                ..Default::default()
                            });
                            img.layout = new_layout;
                        }
                        (vk::ResolveModeFlags::AVERAGE, img.view, img.layout)
                    }
                    None => (
                        vk::ResolveModeFlags::default(),
                        vk::ImageView::default(),
                        vk::ImageLayout::default(),
                    ),
                };
            let img = renderer
                .get_image_mut(attachment.img)
                .ok_or(Error::ResourceMissing)?;

            let new_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
            if img.layout != new_layout {
                barriers.push(vk::ImageMemoryBarrier2 {
                    // NOTE: TOP_OF_PIPE should not be hard coded.
                    // In the future, multiple passes will be used and TOP_OF_PIPE will not always be correct
                    src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    old_layout: img.layout,
                    new_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = new_layout;
            }

            color_attachments.push(vk::RenderingAttachmentInfo {
                image_view: img.view,
                image_layout: img.layout,
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_value: attachment.clear_val,
                resolve_mode,
                resolve_image_view,
                resolve_image_layout,
                ..Default::default()
            });
        }
        if let Some(attachment) = &self.depth {
            let img = renderer
                .get_image_mut(attachment.img)
                .ok_or(Error::ResourceMissing)?;
            let new_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;

            if img.layout != new_layout {
                let aspect_mask = if matches!(
                    img.format,
                    ash::vk::Format::D32_SFLOAT_S8_UINT
                        | ash::vk::Format::D24_UNORM_S8_UINT
                        | ash::vk::Format::D16_UNORM_S8_UINT
                ) {
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
                } else {
                    vk::ImageAspectFlags::DEPTH
                };
                // NOTE: This renderer currently only uses combined depth stencil images.
                barriers.push(vk::ImageMemoryBarrier2 {
                    src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    old_layout: img.layout,
                    new_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = new_layout;
            }

            depth_attachment = vk::RenderingAttachmentInfo {
                image_view: img.view,
                image_layout: img.layout,
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_value: attachment.clear_val,
                ..Default::default()
            };
        }
        let dependency_info = vk::DependencyInfo {
            image_memory_barrier_count: barriers.len() as u32,
            p_image_memory_barriers: barriers.as_ptr(),
            ..Default::default()
        };
        unsafe { renderer.device.cmd_pipeline_barrier2(cmd, &dependency_info) };

        // begin dynamic rendering
        let rendering_info = ash::vk::RenderingInfo {
            render_area: self.render_area,
            layer_count: 1,
            view_mask: 0,
            color_attachment_count: color_attachments.len() as u32,
            p_color_attachments: color_attachments.as_ptr(),
            p_depth_attachment: if self.depth.is_some() {
                &depth_attachment
            } else {
                std::ptr::null()
            },
            ..Default::default()
        };

        unsafe {
            renderer.device.cmd_begin_rendering(cmd, &rendering_info);
        };

        Ok(())
    }

    pub fn end_rendering(&self, renderer: &mut Renderer, cmd: vk::CommandBuffer) -> Result<()> {
        // end rendering
        unsafe {
            renderer.device.cmd_end_rendering(cmd);
        }

        let mut barriers = Vec::with_capacity(self.color.len() + 2);
        for attachment in self.color.iter() {
            let img = match attachment.resolve_img {
                Some(img) => img,
                None => attachment.img,
            };
            let img = renderer.get_image_mut(img).ok_or(Error::ResourceMissing)?;

            if img.layout == attachment.final_layout {
                continue;
            }

            barriers.push(vk::ImageMemoryBarrier2 {
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                dst_access_mask: vk::AccessFlags2::empty(),
                old_layout: img.layout,
                new_layout: attachment.final_layout,
                image: img.handle,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: img.mip_level_count,
                    base_array_layer: 0,
                    layer_count: img.layer_count,
                },
                ..Default::default()
            });
            img.layout = attachment.final_layout;
        }

        if let Some(attachment) = &self.depth {
            let img = renderer
                .get_image_mut(attachment.img)
                .ok_or(Error::ResourceMissing)?;
            let aspect_mask = if matches!(
                img.format,
                ash::vk::Format::D32_SFLOAT_S8_UINT
                    | ash::vk::Format::D24_UNORM_S8_UINT
                    | ash::vk::Format::D16_UNORM_S8_UINT
            ) {
                vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
            } else {
                vk::ImageAspectFlags::DEPTH
            };
            if img.layout != attachment.final_layout {
                // NOTE: This renderer currently only uses combined depth stencil images.
                barriers.push(vk::ImageMemoryBarrier2 {
                    src_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dst_stage_mask: vk::PipelineStageFlags2::ALL_GRAPHICS,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                    old_layout: img.layout,
                    new_layout: attachment.final_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = attachment.final_layout;
            }
        }

        let dependency_info = vk::DependencyInfo {
            image_memory_barrier_count: barriers.len() as u32,
            p_image_memory_barriers: barriers.as_ptr(),
            ..Default::default()
        };

        unsafe { renderer.device.cmd_pipeline_barrier2(cmd, &dependency_info) };

        Ok(())
    }
}

#[derive(PartialEq)]
pub struct AllocationRange {
    pub offset: u64,
    pub size: u64,
}

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
    #[inline]
    pub fn can_reserve_uniform_data(
        &self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        let offset = self.uniform_buffer_offset.next_multiple_of(alignment);
        if offset + byte_count > self.uniform_buffer.size {
            return None;
        }

        Some(AllocationRange {
            offset,
            size: byte_count,
        })
    }
    pub unsafe fn reserve_uniform_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        if self
            .can_reserve_uniform_data(byte_count, alignment)
            .is_none()
        {
            return None;
        }

        self.uniform_buffer_offset = self.uniform_buffer_offset.next_multiple_of(alignment);

        let res = self.uniform_buffer_offset;
        self.uniform_buffer_offset += byte_count;
        return Some(AllocationRange {
            offset: res,
            size: byte_count,
        });
    }
    pub unsafe fn upload_uniform_data<T>(&mut self, offset: u64, data: &[T]) -> Result<()> {
        debug_assert!(std::mem::size_of::<T>() != 0);

        let buffer = &self.uniform_buffer;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;

        // TODO: replace with error?
        debug_assert!(offset + size <= buffer.size);

        unsafe {
            let dst = buffer.map_memory(offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }

        Ok(())
    }
    #[inline]
    pub fn can_reserve_storage_data(
        &self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        let offset = self.storage_buffer_offset.next_multiple_of(alignment);
        if offset + byte_count > self.storage_buffer.size {
            return None;
        }

        Some(AllocationRange {
            offset,
            size: byte_count,
        })
    }
    pub unsafe fn reserve_storage_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        if self
            .can_reserve_storage_data(byte_count, alignment)
            .is_none()
        {
            return None;
        }

        self.storage_buffer_offset = self.storage_buffer_offset.next_multiple_of(alignment);

        let res = self.storage_buffer_offset;
        self.storage_buffer_offset += byte_count;
        return Some(AllocationRange {
            offset: res,
            size: byte_count,
        });
    }
    pub unsafe fn upload_storage_data<T>(&mut self, offset: u64, data: &[T]) -> Result<()> {
        debug_assert!(std::mem::size_of::<T>() != 0);

        let buffer = &self.storage_buffer;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;

        // TODO: replace with error?
        debug_assert!(offset + size <= buffer.size);

        unsafe {
            let dst = buffer.map_memory(offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }

        Ok(())
    }
    pub unsafe fn upload_indirect_data<T>(&mut self, data: &[T], alignment: u64) -> Result<u64> {
        debug_assert!(std::mem::size_of::<T>() != 0);

        self.indirect_buffer_offset = self.indirect_buffer_offset.next_multiple_of(alignment);

        let (buffer, offset) = (&self.indirect_buffer, &mut self.indirect_buffer_offset);
        let res = *offset;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;

        // TODO: replace with error?
        debug_assert!(*offset + size <= buffer.size);

        unsafe {
            let dst = buffer.map_memory(*offset, size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }

        self.indirect_buffer_offset += size;

        Ok(res)
    }
    #[inline]
    pub fn reset_indirect(&mut self) {
        self.indirect_buffer_offset = 0;
    }
    #[inline]
    pub fn reset_all(&mut self) {
        self.indirect_buffer_offset = 0;
        self.uniform_buffer_offset = 0;
        self.storage_buffer_offset = 0;
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
    images: Vec<ImageHandle>,
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
            // TODO: calculate the capacity in a more intelligent way
            (instance_data_element_size * MAX_INSTANCE_DATA_COUNT) + 20000,
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
            images: Vec::new(),
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
    #[inline]
    pub fn reset(&mut self, renderer: &mut Renderer) {
        self.allocator.reset_all();
        while let Some(handle) = self.images.pop() {
            renderer.destroy_image(handle);
        }
    }
    #[inline]
    pub fn get_image(&self, index: usize) -> Option<ImageHandle> {
        self.images.get(index).copied()
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
    depth_format: vk::Format,
    straight_to_resolve: bool,
    // (swapchain, depth, color)
    images: Vec<(ImageHandle, ImageHandle, ImageHandle)>,
    frames: [FrameData; MAX_FRAME_COUNT as usize],
    pub frame_index: usize,
    swapchain_image_index: usize,
}

impl FrameContext {
    pub fn reserve_uniform_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        let mut last_range = None;

        for allocator in self.frames.iter().map(|f| f.allocator()) {
            let cur_range =
                if let Some(range) = allocator.can_reserve_uniform_data(byte_count, alignment) {
                    range
                } else {
                    return None;
                };

            if let Some(range) = last_range {
                if range != cur_range {
                    return None;
                }
            }
            last_range = Some(cur_range);
        }

        let mut last_range = last_range.unwrap();

        for allocator in self.frames.iter_mut().map(|f| f.allocator_mut()) {
            let cur_range =
                unsafe { allocator.reserve_uniform_data(byte_count, alignment) }.unwrap();

            if cur_range != last_range {
                return None;
            }

            last_range = cur_range;
        }

        return Some(last_range);
    }
    pub fn reserve_storage_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<AllocationRange> {
        let mut last_range = None;

        for allocator in self.frames.iter().map(|f| f.allocator()) {
            let cur_range =
                if let Some(range) = allocator.can_reserve_storage_data(byte_count, alignment) {
                    range
                } else {
                    return None;
                };

            if let Some(range) = last_range {
                if range != cur_range {
                    return None;
                }
            }
            last_range = Some(cur_range);
        }

        let mut last_range = last_range.unwrap();

        for allocator in self.frames.iter_mut().map(|f| f.allocator_mut()) {
            let cur_range =
                unsafe { allocator.reserve_storage_data(byte_count, alignment) }.unwrap();

            if cur_range != last_range {
                return None;
            }

            last_range = cur_range;
        }

        return Some(last_range);
    }
    pub fn create_image(
        &mut self,
        image_create_info: &vulkan::ImageCreateInfo,
        renderer: &mut Renderer,
    ) -> Result<usize> {
        let mut index = None;
        let mut cleanup = 0;
        let mut error = None;
        for (i, frame) in self.frames.iter_mut().enumerate() {
            let cur_idx = frame.images.len();
            if let Some(idx) = index {
                if idx != cur_idx {
                    cleanup = i;
                    break;
                }
            }
            let imgage_handle = match renderer.create_image(image_create_info) {
                Ok(img) => img,
                Err(e) => {
                    error = Some(e);
                    cleanup = i;
                    break;
                }
            };
            frame.images.push(imgage_handle);
            index = Some(cur_idx);
        }

        for i in 0..cleanup {
            if let Some(image_handle) = self.frames[i].images.pop() {
                renderer.destroy_image(image_handle);
            }
        }

        if cleanup != 0 && error.is_none() {
            panic!("This should never happen");
        } else if let Some(e) = error {
            return Err(e);
        }

        Ok(index.unwrap())
    }
    pub fn destroy_images(&mut self, renderer: &mut Renderer) {
        for frame in self.frames.iter_mut() {
            while let Some(image_handle) = frame.images.pop() {
                renderer.destroy_image(image_handle);
            }
        }
        while let Some((swapchain, depth, color)) = self.images.pop() {
            renderer.destroy_image(color);
            renderer.destroy_image(swapchain);
            renderer.destroy_image(depth);
        }
    }
    // this is unsafe because the handles to images need to get released with renderer.destroy_image
    pub unsafe fn new(renderer: &mut Renderer, window: &winit::window::Window) -> Result<Self> {
        let device = renderer.device.clone();

        let mut frames = Vec::<FrameData>::with_capacity(MAX_FRAME_COUNT as usize);
        for _ in 0..MAX_FRAME_COUNT {
            let frame = FrameData::new(device.clone())?;
            frames.push(frame);
        }
        let frames: [FrameData; MAX_FRAME_COUNT as usize] =
            frames.try_into().expect("Incorrect number of frames");

        let swapchain = vulkan::Swapchain::new(device.clone(), window)
            .inspect_err(|e| tracing::error!("{e}"))?;

        let mut swapchain_images = {
            let raw_images = unsafe { swapchain.get_images() }?;
            let mut images = Vec::with_capacity(raw_images.len());
            for vk_img in raw_images {
                let img = unsafe { renderer.create_swapchain_image(vk_img, &swapchain) }
                    .inspect_err(|_| {
                        while let Some(img) = images.pop() {
                            renderer.destroy_image(img);
                        }
                    })?;
                images.push(img);
            }
            images
        };

        let depth_format = device.find_viable_depth_stencil_format().ok_or_else(|| {
            while let Some(img) = swapchain_images.pop() {
                renderer.destroy_image(img);
            }
            vulkan::result::Error::CouldNotDetermineFormat
        })?;

        let mut depth_images = {
            let mut images = Vec::with_capacity(swapchain_images.len());

            let depth_image_create_info = vulkan::image::ImageCreateInfo {
                memory_property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                mip_level_count: 1,
                image_type: vk::ImageType::TYPE_2D,
                format: depth_format,
                width: swapchain.extent().width,
                height: swapchain.extent().height,
                depth: 1,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                layer_count: 1,
                level_count: 1,
                samples: renderer.samples(),
            };

            for _ in 0..swapchain_images.len() {
                let img = renderer
                    .create_image(&depth_image_create_info)
                    .inspect_err(|_| {
                        while let Some(img) = images.pop() {
                            renderer.destroy_image(img);
                        }
                        while let Some(img) = swapchain_images.pop() {
                            renderer.destroy_image(img);
                        }
                    })?;
                images.push(img);
            }

            images
        };

        let color_images = {
            let mut images = Vec::with_capacity(swapchain_images.len());

            // TODO: At some point in the future, the code here that creates images should determine if
            // it can use the preferred image flags. This is fine for now though.
            let _preferred_memory_flags =
                vk::MemoryPropertyFlags::LAZILY_ALLOCATED | vk::MemoryPropertyFlags::DEVICE_LOCAL;
            let fallback_memory_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
            let usage_flags =
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT;
            let color_image_create_info = {
                vulkan::image::ImageCreateInfo {
                    memory_property_flags: fallback_memory_flags,
                    mip_level_count: 1,
                    image_type: vk::ImageType::TYPE_2D,
                    format: swapchain.format(),
                    width: swapchain.extent().width,
                    height: swapchain.extent().height,
                    depth: 1,
                    usage: usage_flags,
                    layer_count: 1,
                    level_count: 1,
                    samples: renderer.samples(),
                }
            };

            for _ in 0..swapchain_images.len() {
                let img = renderer
                    .create_image(&color_image_create_info)
                    .inspect_err(|_| {
                        while let Some(img) = images.pop() {
                            renderer.destroy_image(img);
                        }
                        while let Some(img) = depth_images.pop() {
                            renderer.destroy_image(img);
                        }
                        while let Some(img) = swapchain_images.pop() {
                            renderer.destroy_image(img);
                        }
                    })?;
                images.push(img);
            }

            images
        };

        let images = swapchain_images
            .into_iter()
            .zip(depth_images.into_iter())
            .zip(color_images.into_iter())
            .map(|((swapchain, depth), color)| (swapchain, depth, color));

        Ok(Self {
            device,
            swapchain,
            frames,
            depth_format,
            straight_to_resolve: renderer.samples() == vk::SampleCountFlags::TYPE_1,
            images: images.collect(),
            frame_index: 0,
            swapchain_image_index: 0,
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
        self.swapchain.format()
    }
    #[inline]
    pub fn depth_format(&self) -> vk::Format {
        self.depth_format
    }
    pub fn get_current_frame(&self) -> &FrameData {
        &self.frames[self.frame_index]
    }
    #[inline]
    pub(crate) fn frames(&self) -> &[FrameData] {
        &self.frames
    }
    pub fn get_current_frame_mut(&mut self) -> &mut FrameData {
        &mut self.frames[self.frame_index]
    }
    pub fn swapchain_extent(&self) -> vk::Extent2D {
        *self.swapchain.extent()
    }
    pub fn get_swapchain_render_target(&mut self) -> Result<RenderTarget> {
        let (swapchain_image_index, swapchain_image, depth_image, color_image) = {
            let frame = self.get_current_frame();

            unsafe {
                self.device
                    .wait_for_fences(&[frame.command_buffer_executed], true, u64::MAX)?
            };

            let (image_index, _) = unsafe {
                self.swapchain
                    .acquire_next_image(frame.image_acquired, vk::Fence::null())?
            };
            let (swapchain_image, depth_image, color_image) = self.images[image_index as usize];

            unsafe { self.device.reset_fences(&[frame.command_buffer_executed])? };

            (
                image_index as usize,
                swapchain_image,
                depth_image,
                color_image,
            )
        };

        self.swapchain_image_index = swapchain_image_index;

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

        let (img, resolve_img) = if self.straight_to_resolve {
            (swapchain_image, None)
        } else {
            (color_image, Some(swapchain_image))
        };

        Ok(RenderTarget {
            color: Box::new([Attachment {
                img,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                clear_val: vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.0; 4] },
                },
                resolve_img,
            }]),
            depth: Some(Attachment {
                img: depth_image,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                clear_val: vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
                resolve_img: None,
            }),
            render_area: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: *self.swapchain.extent(),
            },
        })
    }
    pub fn submit(&mut self) -> Result<()> {
        let frame = self.get_current_frame();

        unsafe {
            self.device.end_command_buffer(frame.command_buffer)?;
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
                p_image_indices: &(self.swapchain_image_index as u32),
                ..Default::default()
            };
            unsafe { self.device.queue_present(&present_info)? };
        }

        self.frame_index += 1;
        let max_frames = match self.swapchain.present_mode() {
            vk::PresentModeKHR::MAILBOX => 3,
            _ => 2,
        };
        self.frame_index %= max_frames;

        Ok(())
    }
}

use std::sync::atomic::{AtomicU32, Ordering};

use ash::vk;
use vulkan::device::SharedDeviceRef;

use crate::{CameraUBO, InstanceUBO, RenderStorage, RenderTarget, Renderer, Result, TargetImage};

pub const MAX_FRAME_COUNT: u64 = 3;
pub const MAX_CAMERA_DATA_COUNT: u64 = 32;
pub const MAX_INSTANCE_DATA_COUNT: u64 = 128;
pub const MAX_INDIRECT_COMMAND_DATA_COUNT: u64 = MAX_INSTANCE_DATA_COUNT * 4;

#[allow(dead_code)]
pub struct FrameAllocator {
    uniform_allocator: vulkan::StackAllocator,
    storage_allocator: vulkan::StackAllocator,
    indirect_allocator: vulkan::StackAllocator,
}

#[allow(dead_code)]
impl FrameAllocator {
    pub fn new(
        device: SharedDeviceRef,
        uniform_buffer_capcity: u64,
        storage_buffer_capacity: u64,
        indirect_buffer_capacity: u64,
    ) -> Result<Self> {
        let uniform_allocator =
            vulkan::StackAllocator::new_uniform(device.clone(), uniform_buffer_capcity)?;
        let storage_allocator =
            vulkan::StackAllocator::new_storage(device.clone(), storage_buffer_capacity)?;
        let indirect_allocator =
            vulkan::StackAllocator::new_indirect(device, indirect_buffer_capacity)?;

        Ok(Self {
            uniform_allocator,
            storage_allocator,
            indirect_allocator,
        })
    }
    #[inline]
    pub fn uniform_allocator(&self) -> &vulkan::StackAllocator {
        &self.uniform_allocator
    }
    #[inline]
    pub fn uniform_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.uniform_allocator
    }
    #[inline]
    pub fn storage_allocator(&self) -> &vulkan::StackAllocator {
        &self.storage_allocator
    }
    #[inline]
    pub fn storage_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.storage_allocator
    }
    #[inline]
    pub fn indirect_allocator(&self) -> &vulkan::StackAllocator {
        &self.indirect_allocator
    }
    #[inline]
    pub fn indirect_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.indirect_allocator
    }
    #[inline]
    pub fn reset(&mut self) {
        self.indirect_allocator.reset();
        self.uniform_allocator.reset();
        self.storage_allocator.reset();
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
    images: Vec<vulkan::Image>,
    // TODO: static resources should get aded to this struct
    #[allow(dyn_drop)]
    persistent_resources_in_use: Vec<std::sync::Arc<dyn Drop>>,
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
            let size = std::mem::size_of::<InstanceUBO>() as u64;
            let properties = unsafe { device.get_physical_device_properties() };

            size.next_multiple_of(properties.limits.min_storage_buffer_offset_alignment)
        };

        let indirect_command_data_element_size =
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;

        let allocator = FrameAllocator::new(
            device.clone(),
            camera_data_element_size * MAX_CAMERA_DATA_COUNT,
            (instance_data_element_size * MAX_INSTANCE_DATA_COUNT) + 200000,
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
            images: Vec::with_capacity(16),
            persistent_resources_in_use: Vec::with_capacity(16),
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
        self.allocator.reset();
        self.images.clear();
        self.persistent_resources_in_use.clear();
    }
    #[inline]
    pub fn create_image(&mut self, create_info: vulkan::ImageCreateInfo) -> Result<u32> {
        let image = vulkan::Image::new(self.device.clone(), &create_info)?;
        let index = self.images.len() as u32;
        self.images.push(image);
        Ok(index)
    }
    #[inline]
    pub(crate) fn get_image(&self, index: u32) -> Option<&vulkan::Image> {
        self.images.get(index as usize)
    }
    #[inline]
    pub fn get_image_mut(&mut self, index: u32) -> Option<&mut vulkan::Image> {
        self.images.get_mut(index as usize)
    }
    #[inline]
    #[allow(dyn_drop)]
    pub fn attach_persistent_resources(&mut self, resources: std::sync::Arc<dyn Drop>) {
        self.persistent_resources_in_use.push(resources);
    }
    #[inline]
    pub fn clear_persistent_resources(&mut self) {
        self.persistent_resources_in_use.clear();
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

#[derive(Copy, Clone, Debug)]
pub enum FrameContextImageHandle {
    Swapchain { id: u32, index: u32 },
    Depth { id: u32, index: u32 },
    Resolve { id: u32, index: u32 },
    Frame { id: u32, index: u32 },
}

impl FrameContextImageHandle {
    pub fn index(&self) -> u32 {
        match self {
            &Self::Swapchain { index, .. } => index,
            &Self::Depth { index, .. } => index,
            &Self::Resolve { index, .. } => index,
            &Self::Frame { index, .. } => index,
        }
    }
}

impl Default for FrameContextImageHandle {
    fn default() -> Self {
        Self::Frame {
            id: u32::MAX,
            index: u32::MAX,
        }
    }
}

struct SwapchainImage {
    swapchain: vulkan::Image,
    depth: vulkan::Image,
    resolve: vulkan::Image,
}

#[allow(dead_code)]
pub struct FrameContext {
    id: u32,
    device: SharedDeviceRef,
    swapchain: vulkan::Swapchain,
    depth_format: vk::Format,
    straight_to_resolve: bool,
    swapchain_images: Vec<SwapchainImage>,
    frames: [FrameData; MAX_FRAME_COUNT as usize],
    pub frame_index: usize,
    swapchain_image_index: usize,
}

static NEXT_FRAME_CONTEXT_ID: AtomicU32 = AtomicU32::new(0);
impl FrameContext {
    fn get_id() -> u32 {
        NEXT_FRAME_CONTEXT_ID.fetch_add(1, Ordering::Relaxed)
    }
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

        let swapchain_images = {
            let raw_images = unsafe { swapchain.get_images() }?;
            let mut images = Vec::with_capacity(raw_images.len());
            for vk_img in raw_images {
                let image = unsafe {
                    vulkan::Image::new_swapchain_image(
                        renderer.device.clone(),
                        vk_img,
                        swapchain.format(),
                        vk::ImageLayout::UNDEFINED,
                        swapchain.extent().width,
                        swapchain.extent().height,
                    )
                }?;
                images.push(image);
            }
            images
        };

        let depth_format = device
            .find_viable_depth_stencil_format()
            .ok_or(vulkan::result::Error::CouldNotDetermineFormat)?;

        let depth_images = {
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
                let image = vulkan::Image::new(renderer.device.clone(), &depth_image_create_info)?;
                images.push(image);
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
                let image = vulkan::Image::new(renderer.device.clone(), &color_image_create_info)?;
                images.push(image);
            }

            images
        };

        let images = swapchain_images
            .into_iter()
            .zip(depth_images.into_iter())
            .zip(color_images.into_iter())
            .map(|((swapchain, depth), color)| SwapchainImage {
                swapchain,
                depth,
                resolve: color,
            });

        Ok(Self {
            id: Self::get_id(),
            device,
            swapchain,
            frames,
            depth_format,
            straight_to_resolve: renderer.samples() == vk::SampleCountFlags::TYPE_1,
            swapchain_images: images.collect(),
            frame_index: 0,
            swapchain_image_index: 0,
        })
    }
    fn reserve_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
        select_allocator: fn(&mut FrameAllocator) -> &mut vulkan::StackAllocator,
    ) -> Option<vulkan::AllocationRange> {
        let mut last_range = None;

        for allocator in self
            .frames
            .iter_mut()
            .map(|f| select_allocator(f.allocator_mut()))
        {
            let cur_range = if let Some(range) = allocator.can_reserve(byte_count, alignment) {
                range
            } else {
                return None;
            };

            if let Some(range) = last_range
                && range != cur_range
            {
                return None;
            }
            last_range = Some(cur_range);
        }

        let mut last_range = last_range.unwrap();

        for allocator in self
            .frames
            .iter_mut()
            .map(|f| select_allocator(f.allocator_mut()))
        {
            let cur_range = allocator.reserve_data(byte_count, alignment).unwrap();

            if cur_range != last_range {
                return None;
            }

            last_range = cur_range;
        }

        return Some(last_range);
    }
    #[inline]
    fn select_uniform_allocator(allocator: &mut FrameAllocator) -> &mut vulkan::StackAllocator {
        &mut allocator.uniform_allocator
    }
    #[inline]
    fn select_storage_allocator(allocator: &mut FrameAllocator) -> &mut vulkan::StackAllocator {
        &mut allocator.storage_allocator
    }
    #[inline]
    fn select_indirect_allocator(allocator: &mut FrameAllocator) -> &mut vulkan::StackAllocator {
        &mut allocator.indirect_allocator
    }
    pub fn reserve_uniform_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<vulkan::AllocationRange> {
        self.reserve_data(byte_count, alignment, Self::select_uniform_allocator)
    }
    pub fn reserve_storage_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<vulkan::AllocationRange> {
        self.reserve_data(byte_count, alignment, Self::select_storage_allocator)
    }
    pub fn reserve_indirect_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<vulkan::AllocationRange> {
        self.reserve_data(byte_count, alignment, Self::select_indirect_allocator)
    }
    pub fn reset_frames(&mut self, renderer: &mut Renderer) {
        for frame in self.frames.iter_mut() {
            frame.reset(renderer);
        }
    }
    pub fn create_image(
        &mut self,
        image_create_info: &vulkan::ImageCreateInfo,
        renderer: &mut Renderer,
    ) -> Result<FrameContextImageHandle> {
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
            let image = match vulkan::Image::new(renderer.device.clone(), image_create_info) {
                Ok(img) => img,
                Err(e) => {
                    error = Some(e);
                    cleanup = i;
                    break;
                }
            };
            frame.images.push(image);
            index = Some(cur_idx);
        }

        if cleanup != 0 && error.is_none() {
            unreachable!("This should never panic");
        } else if let Some(e) = error {
            return Err(crate::Error::VulkanError(e));
        }

        Ok(FrameContextImageHandle::Frame {
            id: self.id,
            index: index.unwrap().try_into().expect("usize exceeds u32"),
        })
    }
    pub fn destroy_images(&mut self) {
        for frame in self.frames.iter_mut() {
            frame.images.clear();
        }
        self.swapchain_images.clear();
    }
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
    pub fn get_swapchain_render_target(&mut self) -> Result<FrameContextRenderTarget> {
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

            unsafe { self.device.reset_fences(&[frame.command_buffer_executed])? };

            (
                image_index as usize,
                FrameContextImageHandle::Swapchain {
                    id: self.id,
                    index: image_index,
                },
                FrameContextImageHandle::Depth {
                    id: self.id,
                    index: image_index,
                },
                FrameContextImageHandle::Resolve {
                    id: self.id,
                    index: image_index,
                },
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

        let (image, resolve_image) = if self.straight_to_resolve {
            (swapchain_image, None)
        } else {
            (color_image, Some(swapchain_image))
        };

        Ok(FrameContextRenderTarget {
            color_images: Box::new([TargetImage {
                handle: image,
                resolve_handle: resolve_image,
            }]),
            depth_image: Some(depth_image),
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

impl RenderStorage for FrameContext {
    type ImageHandle = FrameContextImageHandle;
    fn get_image(&self, image_handle: FrameContextImageHandle) -> Option<&vulkan::Image> {
        match image_handle {
            FrameContextImageHandle::Swapchain { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get(index as usize)
                    .and_then(|image| Some(&image.swapchain))
            }
            FrameContextImageHandle::Depth { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get(index as usize)
                    .and_then(|image| Some(&image.depth))
            }
            FrameContextImageHandle::Resolve { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get(index as usize)
                    .and_then(|image| Some(&image.resolve))
            }
            FrameContextImageHandle::Frame { id, index } => {
                if id != self.id {
                    return None;
                }
                self.get_current_frame().get_image(index)
            }
        }
    }
    fn get_image_mut(
        &mut self,
        image_handle: FrameContextImageHandle,
    ) -> Option<&mut vulkan::Image> {
        match image_handle {
            FrameContextImageHandle::Swapchain { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get_mut(index as usize)
                    .and_then(|image| Some(&mut image.swapchain))
            }
            FrameContextImageHandle::Depth { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get_mut(index as usize)
                    .and_then(|image| Some(&mut image.depth))
            }
            FrameContextImageHandle::Resolve { id, index } => {
                if id != self.id {
                    return None;
                }
                self.swapchain_images
                    .get_mut(index as usize)
                    .and_then(|image| Some(&mut image.resolve))
            }
            FrameContextImageHandle::Frame { id, index } => {
                if id != self.id {
                    return None;
                }
                self.get_current_frame_mut().get_image_mut(index)
            }
        }
    }
}

impl Drop for FrameContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }
}

pub struct FrameContextRenderTarget {
    pub render_area: vk::Rect2D,
    pub color_images: Box<[TargetImage<FrameContextImageHandle>]>,
    pub depth_image: Option<FrameContextImageHandle>,
}

impl RenderTarget for FrameContextRenderTarget {
    type ImageHandle = FrameContextImageHandle;
    fn get_default_scissor_and_viewport(&self) -> (vk::Rect2D, vk::Viewport) {
        let scissor = self.render_area;
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: scissor.extent.width as f32,
            height: scissor.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        return (scissor, viewport);
    }
    fn render_area(&self) -> vk::Rect2D {
        self.render_area
    }
    fn color_images(&self) -> &[TargetImage<Self::ImageHandle>] {
        &self.color_images
    }
    fn depth_image(&self) -> Option<Self::ImageHandle> {
        self.depth_image
    }
}

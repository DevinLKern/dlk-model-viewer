use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU32, Ordering},
};

use ash::vk;
use vulkan::device::SharedDeviceRef;

use crate::{
    CameraUBO, GlobalLightUBO, GridInstanceUBO, Image, ImageCreateInfo, InstanceUBO, PointLightUBO,
    PointLightsUBO, RenderStorage, RenderTarget, Renderer, ResolvableTargetImage, Result,
    TargetImage,
};

pub const MAX_FRAME_COUNT: u64 = 3;
pub const MAX_CAMERA_DATA_COUNT: u64 = 32;
pub const MAX_INSTANCE_DATA_COUNT: u64 = 128;
pub const MAX_INDIRECT_COMMAND_DATA_COUNT: u64 = MAX_INSTANCE_DATA_COUNT * 4;

pub struct FrameData {
    device: SharedDeviceRef,
    command_buffer_executed: vk::Fence,
    image_acquired: vk::Semaphore,
    render_complete: vk::Semaphore,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    uniform_allocator: vulkan::StackAllocator,
    storage_allocator: vulkan::StackAllocator,
    indirect_allocator: vulkan::StackAllocator,
    images: Vec<Image>,
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
        let properties = unsafe { device.get_physical_device_properties() };

        let camera_data_element_size = {
            let size = std::mem::size_of::<CameraUBO>() as u64;

            size.next_multiple_of(properties.limits.min_uniform_buffer_offset_alignment)
        };

        let instance_data_size = {
            let size = std::mem::size_of::<InstanceUBO>() as u64 * MAX_INSTANCE_DATA_COUNT;
            size.next_multiple_of(properties.limits.min_storage_buffer_offset_alignment)
        };

        let grid_instance_data_size = {
            let size = std::mem::size_of::<GridInstanceUBO>() as u64 * MAX_INSTANCE_DATA_COUNT;
            size.next_multiple_of(properties.limits.min_storage_buffer_offset_alignment)
        };

        let indirect_command_data_element_size =
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;

        let uniform_allocator = vulkan::StackAllocator::new_uniform(
            device.clone(),
            camera_data_element_size * MAX_CAMERA_DATA_COUNT,
        )?;
        let storage_allocator = vulkan::StackAllocator::new_storage(
            device.clone(),
            instance_data_size
                + grid_instance_data_size
                // TODO: This should refer to MAX_POINT_LIGHT_COUNT in application.rs not 32
                + (32 + std::mem::size_of::<PointLightUBO>() as u64)
                + std::mem::size_of::<PointLightsUBO>() as u64
                + std::mem::size_of::<GlobalLightUBO>() as u64
                + 1024,
        )?;
        let indirect_allocator = vulkan::StackAllocator::new_indirect(
            device.clone(),
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
            uniform_allocator,
            storage_allocator,
            indirect_allocator,
            images: Vec::with_capacity(16),
            persistent_resources_in_use: Vec::with_capacity(16),
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
    pub fn command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }
    #[inline]
    pub fn reset(&mut self, renderer: &mut Renderer) {
        self.indirect_allocator.reset();
        self.uniform_allocator.reset();
        self.storage_allocator.reset();
        self.images.clear();
        self.persistent_resources_in_use.clear();
    }
    #[inline]
    pub fn create_image(&mut self, create_info: &ImageCreateInfo) -> Result<u32> {
        let image = Image::new_owned(self.device.clone(), &create_info)?;
        let index = self.images.len() as u32;
        self.images.push(image);
        Ok(index)
    }
    #[inline]
    pub(crate) fn get_image(&self, index: u32) -> Option<&Image> {
        self.images.get(index as usize)
    }
    #[inline]
    pub fn get_image_mut(&mut self, index: u32) -> Option<&mut Image> {
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
    swapchain: Image,
    depth: Image,
    resolve: Image,
}

#[derive(Default, Copy, Clone)]
pub struct FrameContextRange<T: Default + Copy + Clone> {
    pub offset: u64,
    pub size: u64,
    _marker: PhantomData<T>,
}

impl<T: Default + Copy + Clone> From<FrameContextRange<T>> for vulkan::AllocationRange {
    fn from(value: FrameContextRange<T>) -> Self {
        Self {
            offset: value.offset,
            size: value.size,
        }
    }
}

impl<T: Default + Copy + Clone> From<vulkan::AllocationRange> for FrameContextRange<T> {
    fn from(value: vulkan::AllocationRange) -> Self {
        Self {
            offset: value.offset,
            size: value.size,
            _marker: PhantomData::default(),
        }
    }
}

impl<T: Default + Copy + Clone> FrameContextRange<T> {
    #[allow(unused)]
    #[inline]
    pub fn new(offset: u64, size: u64) -> Self {
        Self {
            offset,
            size,
            _marker: PhantomData::default(),
        }
    }
}

#[derive(Default, Copy, Clone)]
pub struct Uniform;
#[derive(Default, Copy, Clone)]
pub struct Storage;

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

// CanResolveBindingValue was here

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
                let image = Image::new_swapchain(
                    renderer.device.clone(),
                    vk_img,
                    swapchain.extent().width,
                    swapchain.extent().height,
                    swapchain.format(),
                )?;
                images.push(image);
            }
            images
        };

        let depth_format = device
            .find_viable_depth_stencil_format()
            .ok_or(vulkan::result::Error::CouldNotDetermineFormat)?;

        let depth_images = {
            let mut images = Vec::with_capacity(swapchain_images.len());

            let depth_image_create_info = ImageCreateInfo {
                memory_property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                mip_levels: 1,
                image_type: vk::ImageType::TYPE_2D,
                format: depth_format,
                width: swapchain.extent().width,
                height: swapchain.extent().height,
                depth: 1,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                array_layers: 1,
                samples: renderer.samples(),
            };

            for _ in 0..swapchain_images.len() {
                let image = Image::new_owned(renderer.device.clone(), &depth_image_create_info)?;
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
                ImageCreateInfo {
                    memory_property_flags: fallback_memory_flags,
                    mip_levels: 1,
                    image_type: vk::ImageType::TYPE_2D,
                    format: swapchain.format(),
                    width: swapchain.extent().width,
                    height: swapchain.extent().height,
                    depth: 1,
                    usage: usage_flags,
                    array_layers: 1,
                    samples: renderer.samples(),
                }
            };

            for _ in 0..swapchain_images.len() {
                let image = Image::new_owned(renderer.device.clone(), &color_image_create_info)?;
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
        select_allocator: fn(&mut FrameData) -> &mut vulkan::StackAllocator,
    ) -> Option<vulkan::AllocationRange> {
        let mut last_range = None;

        for allocator in self
            .frames
            .iter_mut()
            .map(|frame_data| select_allocator(frame_data))
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
            .map(|frame_data| select_allocator(frame_data))
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
    fn select_uniform_allocator(frame_data: &mut FrameData) -> &mut vulkan::StackAllocator {
        &mut frame_data.uniform_allocator
    }
    #[inline]
    fn select_storage_allocator(frame_data: &mut FrameData) -> &mut vulkan::StackAllocator {
        &mut frame_data.storage_allocator
    }
    #[inline]
    fn select_indirect_allocator(frame_data: &mut FrameData) -> &mut vulkan::StackAllocator {
        &mut frame_data.indirect_allocator
    }
    pub fn reserve_uniform_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<FrameContextRange<Uniform>> {
        self.reserve_data(byte_count, alignment, Self::select_uniform_allocator)
            .and_then(|range| Some(range.into()))
    }
    pub fn reserve_storage_data(
        &mut self,
        byte_count: u64,
        alignment: u64,
    ) -> Option<FrameContextRange<Storage>> {
        self.reserve_data(byte_count, alignment, Self::select_storage_allocator)
            .and_then(|range| Some(range.into()))
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
        image_create_info: &ImageCreateInfo,
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
            let image = match Image::new_owned(renderer.device.clone(), image_create_info) {
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
            unreachable!();
        } else if let Some(e) = error {
            return Err(e);
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
    #[inline]
    pub(crate) fn frames_mut(&mut self) -> &mut [FrameData] {
        &mut self.frames
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
            let image_index = image_index as usize;

            unsafe { self.device.reset_fences(&[frame.command_buffer_executed])? };

            // Begin command buffer
            let begin_info = vk::CommandBufferBeginInfo {
                flags: ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                ..Default::default()
            };

            unsafe {
                let frame = self.get_current_frame();

                // Reset the command buffer (requires pool/reset capability)
                self.device.reset_command_buffer(
                    frame.command_buffer,
                    vk::CommandBufferResetFlags::empty(),
                )?;

                self.device
                    .begin_command_buffer(frame.command_buffer, &begin_info)?;
            }

            let cmd = frame.command_buffer();
            let image = &mut self.swapchain_images[image_index];

            {
                let mut barriers = [vk::ImageMemoryBarrier2::default(); 3];
                let mut barrier_count = 0;

                const NEW_SWAPCHAIN_IMAGE_LAYOUT: vk::ImageLayout =
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
                if image.swapchain.layout() != NEW_SWAPCHAIN_IMAGE_LAYOUT {
                    barriers[barrier_count as usize] = vk::ImageMemoryBarrier2 {
                        src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                        src_access_mask: vk::AccessFlags2::empty(),
                        dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        old_layout: image.swapchain.layout(),
                        new_layout: NEW_SWAPCHAIN_IMAGE_LAYOUT,
                        image: image.swapchain.handle(),
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: image.swapchain.mip_levels(),
                            base_array_layer: 0,
                            layer_count: image.swapchain.array_layers(),
                        },
                        ..Default::default()
                    };
                    *image.swapchain.layout_mut() = NEW_SWAPCHAIN_IMAGE_LAYOUT;
                    barrier_count += 1;
                }

                const NEW_COLOR_IMAGE_LAYOUT: vk::ImageLayout =
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
                if image.resolve.layout() != NEW_COLOR_IMAGE_LAYOUT {
                    barriers[barrier_count as usize] = vk::ImageMemoryBarrier2 {
                        src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                        src_access_mask: vk::AccessFlags2::empty(),
                        dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        old_layout: image.resolve.layout(),
                        new_layout: NEW_COLOR_IMAGE_LAYOUT,
                        image: image.resolve.handle(),
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: image.resolve.mip_levels(),
                            base_array_layer: 0,
                            layer_count: image.resolve.array_layers(),
                        },
                        ..Default::default()
                    };
                    *image.resolve.layout_mut() = NEW_COLOR_IMAGE_LAYOUT;
                    barrier_count += 1;
                }

                const NEW_DEPTH_IMAGE_LAYOUT: vk::ImageLayout =
                    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
                if image.depth.layout() != NEW_DEPTH_IMAGE_LAYOUT {
                    let aspect_mask = if matches!(
                        image.depth.format(),
                        vk::Format::D32_SFLOAT_S8_UINT
                            | vk::Format::D24_UNORM_S8_UINT
                            | vk::Format::D16_UNORM_S8_UINT
                    ) {
                        vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
                    } else {
                        vk::ImageAspectFlags::DEPTH
                    };
                    barriers[barrier_count as usize] = vk::ImageMemoryBarrier2 {
                        src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                        src_access_mask: vk::AccessFlags2::empty(),
                        dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                        dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        old_layout: image.depth.layout(),
                        new_layout: NEW_DEPTH_IMAGE_LAYOUT,
                        image: image.depth.handle(),
                        subresource_range: vk::ImageSubresourceRange {
                            aspect_mask,
                            base_mip_level: 0,
                            level_count: image.depth.mip_levels(),
                            base_array_layer: 0,
                            layer_count: image.depth.array_layers(),
                        },
                        ..Default::default()
                    };
                    *image.depth.layout_mut() = NEW_DEPTH_IMAGE_LAYOUT;
                    barrier_count += 1;
                }

                let info = vk::DependencyInfo {
                    image_memory_barrier_count: barrier_count,
                    p_image_memory_barriers: barriers.as_ptr(),
                    ..Default::default()
                };

                unsafe {
                    self.device.cmd_pipeline_barrier2(cmd, &info);
                }
            }

            (
                image_index,
                &mut image.swapchain,
                &mut image.depth,
                &mut image.resolve,
            )
        };

        self.swapchain_image_index = swapchain_image_index;

        // TODO: Depth should have associated resolve image
        let depth_image = {
            let image_view_desc = depth_image.get_default_image_view_desc(
                vk::ImageAspectFlags::DEPTH,
                vk::ImageViewType::TYPE_2D,
            );
            Some(ResolvableTargetImage {
                main: TargetImage::new(depth_image, image_view_desc)?,
                resolve: None,
            })
        };

        let (main, resolve) = if self.straight_to_resolve {
            let swapchain_image_view_desc = swapchain_image.get_default_image_view_desc(
                vk::ImageAspectFlags::COLOR,
                vk::ImageViewType::TYPE_2D,
            );
            let target = TargetImage::new(swapchain_image, swapchain_image_view_desc)?;

            (target, None)
        } else {
            let swapchain_image_view_desc = swapchain_image.get_default_image_view_desc(
                vk::ImageAspectFlags::COLOR,
                vk::ImageViewType::TYPE_2D,
            );
            let resolve = TargetImage::new(swapchain_image, swapchain_image_view_desc)?;

            let color_image_view_desc = color_image.get_default_image_view_desc(
                vk::ImageAspectFlags::COLOR,
                vk::ImageViewType::TYPE_2D,
            );
            let target = TargetImage::new(color_image, color_image_view_desc)?;

            (target, Some(resolve))
        };

        let target = RenderTarget {
            color_images: Box::new([ResolvableTargetImage { main, resolve }]),
            depth_image,
            render_area: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: *self.swapchain.extent(),
            },
        };

        Ok(target)
    }
    pub fn submit(&mut self) -> Result<()> {
        let cmd = self.get_current_frame().command_buffer();

        {
            let image = &mut self.swapchain_images[self.swapchain_image_index];

            let mut barriers = [vk::ImageMemoryBarrier2::default(); 1];
            let mut barrier_count = 0;

            const NEW_SWAPCHAIN_IMAGE_LAYOUT: vk::ImageLayout = vk::ImageLayout::PRESENT_SRC_KHR;
            if image.swapchain.layout() != NEW_SWAPCHAIN_IMAGE_LAYOUT {
                barriers[barrier_count as usize] = vk::ImageMemoryBarrier2 {
                    src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: vk::AccessFlags2::NONE,
                    dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                    dst_access_mask: vk::AccessFlags2::NONE,
                    old_layout: image.swapchain.layout(),
                    new_layout: NEW_SWAPCHAIN_IMAGE_LAYOUT,
                    image: image.swapchain.handle(),
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: image.swapchain.mip_levels(),
                        base_array_layer: 0,
                        layer_count: image.swapchain.array_layers(),
                    },
                    ..Default::default()
                };
                *image.swapchain.layout_mut() = NEW_SWAPCHAIN_IMAGE_LAYOUT;
                barrier_count += 1;
            }

            let info = vk::DependencyInfo {
                image_memory_barrier_count: barrier_count,
                p_image_memory_barriers: barriers.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device.cmd_pipeline_barrier2(cmd, &info);
            }
        }

        let frame = self.get_current_frame();

        unsafe {
            self.device.end_command_buffer(cmd)?;
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
    fn get_image(&self, image_handle: FrameContextImageHandle) -> Option<&Image> {
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
    fn get_image_mut(&mut self, image_handle: FrameContextImageHandle) -> Option<&mut Image> {
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

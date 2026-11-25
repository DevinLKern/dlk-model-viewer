use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use ash::vk;
use vulkan::SharedDeviceRef;

use crate::{Error, Result};

#[derive(Copy, Clone)]
pub struct ImageViewDesc {
    pub subresource_range: vk::ImageSubresourceRange,
    pub ty: vk::ImageViewType,
    pub swizzle: vk::ComponentMapping,
}

impl PartialEq for ImageViewDesc {
    fn eq(&self, other: &Self) -> bool {
        let sub_resouce_equals = self.subresource_range.aspect_mask
            == other.subresource_range.aspect_mask
            && self.subresource_range.base_mip_level == other.subresource_range.base_mip_level
            && self.subresource_range.level_count == other.subresource_range.level_count
            && self.subresource_range.base_array_layer == other.subresource_range.base_array_layer
            && self.subresource_range.layer_count == other.subresource_range.layer_count;
        let swizzle_equals = self.swizzle.r == other.swizzle.r
            && self.swizzle.g == other.swizzle.g
            && self.swizzle.b == other.swizzle.b
            && self.swizzle.a == other.swizzle.a;
        let type_equals = self.ty == other.ty;

        sub_resouce_equals && swizzle_equals && type_equals
    }
    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}
impl Eq for ImageViewDesc {
    //
}
impl Hash for ImageViewDesc {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.subresource_range.aspect_mask.hash(state);
        self.subresource_range.base_mip_level.hash(state);
        self.subresource_range.level_count.hash(state);
        self.subresource_range.base_array_layer.hash(state);
        self.subresource_range.layer_count.hash(state);

        self.swizzle.r.hash(state);
        self.swizzle.g.hash(state);
        self.swizzle.b.hash(state);
        self.swizzle.a.hash(state);

        self.ty.hash(state);
    }
}

pub(crate) enum ImageMemory {
    Swapchain,
    Owned(vk::DeviceMemory),
}

pub struct ImageCreateInfo {
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: vk::SampleCountFlags,
    pub usage: vk::ImageUsageFlags,
    pub image_type: vk::ImageType,
    pub memory_property_flags: vk::MemoryPropertyFlags,
}

pub struct Image {
    device: SharedDeviceRef,
    handle: vk::Image,
    memory: ImageMemory,
    width: u32,
    height: u32,
    depth: u32,
    mip_levels: u32,
    array_layers: u32,
    layout: vk::ImageLayout,
    format: vk::Format,
    _ty: vk::ImageType,
    views: HashMap<ImageViewDesc, vk::ImageView>,
}

impl Image {
    pub(crate) fn new_swapchain(
        device: SharedDeviceRef,
        image: vk::Image,
        width: u32,
        height: u32,
        format: vk::Format,
    ) -> Result<Self> {
        let create_info = ImageCreateInfo {
            format,
            width,
            height,
            depth: 1,
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            usage: vk::ImageUsageFlags::empty(),
            image_type: vk::ImageType::TYPE_2D,
            memory_property_flags: vk::MemoryPropertyFlags::empty(),
        };
        Self::new_inner(device, &create_info, Some(image))
    }
    pub fn new_owned(device: SharedDeviceRef, create_info: &ImageCreateInfo) -> Result<Self> {
        Self::new_inner(device, create_info, None)
    }
    fn new_inner(
        device: SharedDeviceRef,
        create_info: &ImageCreateInfo,
        swapchain_image: Option<vk::Image>,
    ) -> Result<Self> {
        let handle = if let Some(image_handle) = swapchain_image {
            image_handle
        } else {
            let tiling = {
                let format_properties =
                    unsafe { device.get_physical_device_format_properties(create_info.format) };
                let features = {
                    let mut f = vk::FormatFeatureFlags::empty();
                    if create_info.usage.contains(vk::ImageUsageFlags::SAMPLED) {
                        f |= vk::FormatFeatureFlags::SAMPLED_IMAGE;
                    }
                    if create_info
                        .usage
                        .contains(ash::vk::ImageUsageFlags::COLOR_ATTACHMENT)
                    {
                        f |= vk::FormatFeatureFlags::COLOR_ATTACHMENT;
                    }
                    if create_info
                        .usage
                        .contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                    {
                        f |= vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
                    }
                    if create_info
                        .usage
                        .contains(vk::ImageUsageFlags::TRANSFER_SRC)
                    {
                        f |= vk::FormatFeatureFlags::TRANSFER_SRC;
                    }
                    if create_info
                        .usage
                        .contains(vk::ImageUsageFlags::TRANSFER_DST)
                    {
                        f |= vk::FormatFeatureFlags::TRANSFER_DST;
                    }
                    if create_info.usage.contains(vk::ImageUsageFlags::STORAGE) {
                        f |= vk::FormatFeatureFlags::STORAGE_IMAGE;
                    }
                    f
                };

                if format_properties.optimal_tiling_features.contains(features) {
                    vk::ImageTiling::OPTIMAL
                } else if format_properties.linear_tiling_features.contains(features) {
                    vk::ImageTiling::LINEAR
                } else {
                    return Err(Error::VulkanError(vulkan::Error::ImageFeaturesNotSupported));
                }
            };
            let queue_family_indices = [device.get_queue_family_index()];
            let image_create_info = vk::ImageCreateInfo {
                image_type: create_info.image_type,
                format: create_info.format,
                extent: vk::Extent3D {
                    width: create_info.width,
                    height: create_info.height,
                    depth: create_info.depth,
                },
                mip_levels: create_info.mip_levels,
                array_layers: create_info.array_layers,
                samples: create_info.samples,
                tiling,
                usage: create_info.usage,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                p_queue_family_indices: queue_family_indices.as_ptr(),
                queue_family_index_count: queue_family_indices.len() as u32,
                ..Default::default()
            };

            unsafe { device.create_image(&image_create_info) }?
        };

        let memory = if swapchain_image.is_none() {
            let (memory_properties, memory_requirements) = unsafe {
                (
                    device.get_physical_device_memory_properties(),
                    device.get_image_memory_requirements(handle),
                )
            };
            let memory_type_index = vulkan::find_memory_index(
                memory_properties,
                memory_requirements,
                create_info.memory_property_flags,
            )
            .ok_or_else(|| {
                unsafe {
                    device.destroy_image(handle);
                }
                Error::VulkanError(vulkan::Error::CouldNotFindViableMemoryIndex)
            })?;

            let memory_allocate_info = vk::MemoryAllocateInfo {
                allocation_size: memory_requirements.size,
                memory_type_index,
                ..Default::default()
            };
            let memory = unsafe { device.allocate_memory(&memory_allocate_info) }.inspect_err(
                |_| unsafe {
                    device.destroy_image(handle);
                },
            )?;

            unsafe { device.bind_image_memory(handle, memory, 0) }.inspect_err(|_| unsafe {
                device.destroy_image(handle);
                device.free_memory(memory);
            })?;

            ImageMemory::Owned(memory)
        } else {
            ImageMemory::Swapchain
        };

        Ok(Self {
            device,
            handle,
            memory,
            width: create_info.width,
            height: create_info.height,
            depth: create_info.depth,
            mip_levels: create_info.mip_levels,
            array_layers: create_info.array_layers,
            layout: vk::ImageLayout::UNDEFINED,
            format: create_info.format,
            _ty: create_info.image_type,
            views: HashMap::new(),
        })
    }
    #[inline]
    pub fn get_default_image_view_desc(
        &self,
        aspect_mask: vk::ImageAspectFlags,
        view_type: vk::ImageViewType,
    ) -> ImageViewDesc {
        ImageViewDesc {
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: self.mip_levels,
                base_array_layer: 0,
                layer_count: self.array_layers,
            },
            swizzle: vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            },
            ty: view_type,
        }
    }
    pub fn create_image_view(&mut self, image_view_desc: ImageViewDesc) -> Result<vk::ImageView> {
        if let Some(view) = self.views.get(&image_view_desc) {
            return Ok(*view);
        }

        let view = {
            let image_view_create_info = vk::ImageViewCreateInfo {
                image: self.handle,
                view_type: image_view_desc.ty,
                format: self.format,
                components: image_view_desc.swizzle,
                subresource_range: image_view_desc.subresource_range,
                ..Default::default()
            };

            unsafe { self.device.create_image_view(&image_view_create_info) }?
        };

        self.views.insert(image_view_desc, view);
        return Ok(view);
    }
    #[inline]
    pub fn get_image_view(&self, image_view_desc: ImageViewDesc) -> Option<vk::ImageView> {
        self.views.get(&image_view_desc).copied()
    }
    #[inline]
    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
    }
    #[inline]
    pub fn get_width(&self, mip_level: u32) -> u32 {
        debug_assert!(mip_level < self.mip_levels);
        (self.width >> mip_level).max(1)
    }
    #[inline]
    pub fn get_height(&self, mip_level: u32) -> u32 {
        debug_assert!(mip_level < self.mip_levels);
        (self.height >> mip_level).max(1)
    }
    #[inline]
    pub fn get_depth(&self, mip_level: u32) -> u32 {
        debug_assert!(mip_level < self.mip_levels);
        (self.depth >> mip_level).max(1)
    }
    #[inline]
    pub fn format(&self) -> vk::Format {
        self.format
    }
    #[inline]
    pub fn array_layers(&self) -> u32 {
        self.array_layers
    }
    #[inline]
    pub fn layout(&self) -> vk::ImageLayout {
        self.layout
    }
    #[inline]
    pub fn layout_mut(&mut self) -> &mut vk::ImageLayout {
        &mut self.layout
    }
    #[inline]
    pub unsafe fn clear_image_views(&mut self) {
        for (_, view) in self.views.drain() {
            unsafe {
                self.device.destroy_image_view(view);
            }
        }
    }
    #[inline]
    pub fn handle(&self) -> vk::Image {
        self.handle
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.clear_image_views();
            if let ImageMemory::Owned(memory) = self.memory {
                self.device.destroy_image(self.handle);
                self.device.free_memory(memory);
            }
        }
    }
}

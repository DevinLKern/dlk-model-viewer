use crate::allocator::find_memory_index;
use crate::device::SharedDeviceRef;
use crate::result::{Error, Result};

use ash::vk;

pub struct Image {
    device: SharedDeviceRef,
    pub handle: ash::vk::Image,
    pub view: ash::vk::ImageView,
    pub memory: ash::vk::DeviceMemory,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[allow(dead_code)]
pub struct ImageCreateInfo {
    pub memory_property_flags: ash::vk::MemoryPropertyFlags,
    pub mip_levels: u32,
    pub image_type: ash::vk::ImageType,
    pub format: ash::vk::Format,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub usage: ash::vk::ImageUsageFlags,
    pub array_layers: u32,
}

fn is_depth_format(format: ash::vk::Format) -> bool {
    matches!(
        format,
        ash::vk::Format::D16_UNORM
            | ash::vk::Format::X8_D24_UNORM_PACK32
            | ash::vk::Format::S8_UINT
            | ash::vk::Format::D16_UNORM_S8_UINT
            | ash::vk::Format::D24_UNORM_S8_UINT
            | ash::vk::Format::D32_SFLOAT_S8_UINT
    )
}
fn is_stencil_format(format: ash::vk::Format) -> bool {
    matches!(
        format,
        ash::vk::Format::S8_UINT
            | ash::vk::Format::D16_UNORM_S8_UINT
            | ash::vk::Format::D24_UNORM_S8_UINT
            | ash::vk::Format::D32_SFLOAT_S8_UINT
    )
}

#[allow(dead_code)]
impl Image {
    pub fn new(device: SharedDeviceRef, create_info: &ImageCreateInfo) -> Result<Self> {
        let tiling = {
            let format_properties =
                unsafe { device.get_physical_device_format_properties(create_info.format) };
            let features = {
                let mut f = ash::vk::FormatFeatureFlags::empty();
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::SAMPLED)
                {
                    f |= ash::vk::FormatFeatureFlags::SAMPLED_IMAGE;
                }
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::COLOR_ATTACHMENT)
                {
                    f |= ash::vk::FormatFeatureFlags::COLOR_ATTACHMENT;
                }
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                {
                    f |= ash::vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
                }
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::TRANSFER_SRC)
                {
                    f |= ash::vk::FormatFeatureFlags::TRANSFER_SRC;
                }
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::TRANSFER_DST)
                {
                    f |= ash::vk::FormatFeatureFlags::TRANSFER_DST;
                }
                if create_info
                    .usage
                    .contains(ash::vk::ImageUsageFlags::STORAGE)
                {
                    f |= ash::vk::FormatFeatureFlags::STORAGE_IMAGE;
                }
                f
            };

            if format_properties.optimal_tiling_features.contains(features) {
                ash::vk::ImageTiling::OPTIMAL
            } else if format_properties.linear_tiling_features.contains(features) {
                ash::vk::ImageTiling::LINEAR
            } else {
                return Err(Error::NotImplemented); // TODO: add error type?
            }
        };

        let image_create_info = ash::vk::ImageCreateInfo {
            image_type: create_info.image_type,
            format: create_info.format,
            mip_levels: create_info.mip_levels,
            extent: ash::vk::Extent3D {
                width: create_info.width,
                height: create_info.height,
                depth: create_info.depth,
            },
            usage: create_info.usage,
            array_layers: create_info.array_layers,
            samples: ash::vk::SampleCountFlags::TYPE_1,
            tiling,
            sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
            initial_layout: ash::vk::ImageLayout::UNDEFINED,
            ..Default::default()
        };

        let image = unsafe { device.create_image(&image_create_info) }?;

        let image_view_create_info = ash::vk::ImageViewCreateInfo {
            image,
            view_type: match create_info.image_type {
                vk::ImageType::TYPE_1D => {
                    if create_info.array_layers > 1 {
                        vk::ImageViewType::TYPE_1D_ARRAY
                    } else {
                        vk::ImageViewType::TYPE_1D
                    }
                }
                vk::ImageType::TYPE_2D => {
                    if create_info.array_layers > 1 {
                        vk::ImageViewType::TYPE_2D_ARRAY
                    } else {
                        vk::ImageViewType::TYPE_2D
                    }
                }
                vk::ImageType::TYPE_3D => vk::ImageViewType::TYPE_3D,
                _ => vk::ImageViewType::TYPE_1D,
            },
            format: create_info.format,
            components: vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            },
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: {
                    let mut mask = ash::vk::ImageAspectFlags::empty();
                    if is_depth_format(create_info.format) {
                        mask |= ash::vk::ImageAspectFlags::DEPTH;
                    }
                    if is_stencil_format(create_info.format) {
                        mask |= ash::vk::ImageAspectFlags::STENCIL;
                    }
                    if mask == ash::vk::ImageAspectFlags::empty() {
                        mask = ash::vk::ImageAspectFlags::COLOR;
                    }
                    mask
                },
                base_mip_level: 0,
                level_count: create_info.mip_levels,
                base_array_layer: 0,
                layer_count: create_info.array_layers,
            },
            ..Default::default()
        };

        let allocate_info = {
            let memory_properties = unsafe { device.get_physical_device_memory_properties() };
            let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
            let memory_property_flags = create_info.memory_property_flags;
            let memory_type_index = find_memory_index(
                memory_properties,
                memory_requirements,
                memory_property_flags,
            )
            .ok_or_else(|| {
                unsafe {
                    device.destroy_image(image);
                }
                Error::NotImplemented
            })?;
            ash::vk::MemoryAllocateInfo {
                allocation_size: memory_requirements.size,
                memory_type_index,
                ..Default::default()
            }
        };
        let memory = unsafe { device.allocate_memory(&allocate_info) }.inspect_err(|_| unsafe {
            device.destroy_image(image);
        })?;

        unsafe { device.bind_image_memory(image, memory, 0) }.inspect_err(|_| unsafe {
            device.free_memory(memory);
            device.destroy_image(image);
        })?;

        let image_view =
            unsafe { device.create_image_view(&image_view_create_info) }.inspect_err(|_| {
                unsafe {
                    device.free_memory(memory);
                    device.destroy_image(image)
                };
            })?;
        Ok(Image {
            device,
            handle: image,
            view: image_view,
            memory,
            width: create_info.width,
            height: create_info.height,
            depth: create_info.depth,
        })
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory);
            self.device.destroy_image_view(self.view);
            self.device.destroy_image(self.handle);
        }
    }
}

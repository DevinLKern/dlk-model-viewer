use crate::allocator::find_memory_index;
use crate::device::SharedDeviceRef;
use crate::result::{Error, Result};

use ash::vk;
use std::rc::Rc;

pub struct BufferCreateInfo {
    pub size: vk::DeviceSize,
    pub usage: vk::BufferUsageFlags,
    pub memory_property_flags: vk::MemoryPropertyFlags,
}

pub struct Buffer {
    device: SharedDeviceRef,
    pub handle: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub size: vk::DeviceSize,
    pub offset: vk::DeviceSize,
}

impl Buffer {
    pub fn new(device: SharedDeviceRef, create_info: &BufferCreateInfo) -> Result<Self> {
        let buffer_create_info = vk::BufferCreateInfo {
            size: create_info.size,
            usage: create_info.usage,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };

        let buffer = unsafe { device.create_buffer(&buffer_create_info) }?;

        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_properties = unsafe { device.get_physical_device_memory_properties() };
        let memory_type_index = find_memory_index(
            memory_properties,
            memory_requirements,
            create_info.memory_property_flags,
        )
        .ok_or(Error::CouldNotFindMemoryTypeIndex(
            create_info.memory_property_flags,
        ))
        .inspect_err(|_| unsafe {
            device.destroy_buffer(buffer);
        })?;

        let allocate_info = vk::MemoryAllocateInfo {
            allocation_size: memory_requirements.size,
            memory_type_index,
            ..Default::default()
        };
        let memory = unsafe { device.allocate_memory(&allocate_info) }.inspect_err(|_| unsafe {
            device.destroy_buffer(buffer);
        })?;

        let offset = 0;

        unsafe { device.bind_buffer_memory(buffer, memory, offset) }.inspect_err(|_| unsafe {
            device.destroy_buffer(buffer);
            device.free_memory(memory);
        })?;

        Ok(Buffer {
            device,
            handle: buffer,
            memory,
            size: create_info.size,
            offset,
        })
    }

    #[inline]
    pub unsafe fn map_memory(
        &self,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> ash::prelude::VkResult<*mut std::ffi::c_void> {
        unsafe {
            self.device
                .map_memory(self.memory, offset, size, vk::MemoryMapFlags::empty())
        }
    }

    #[inline]
    pub unsafe fn unmap(&self) {
        unsafe { self.device.unmap_memory(self.memory) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory);
            self.device.destroy_buffer(self.handle);
        }
    }
}

impl std::fmt::Display for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Buffer{{ size: {}, offset: {} }}",
            self.size, self.offset
        )
    }
}

pub struct UniformBV {
    pub buffer: Rc<Buffer>,
    pub offset: vk::DeviceSize,
    pub size: vk::DeviceSize,
}

pub struct DynamicUniformBV {
    pub buffer: Rc<Buffer>,
    pub offset: vk::DeviceSize,
    pub size: vk::DeviceSize,
}

impl std::fmt::Display for DynamicUniformBV {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DynamicUniformBV{{ buffer: {}, size: {}, offset: {} }}",
            self.buffer, self.size, self.offset
        )
    }
}

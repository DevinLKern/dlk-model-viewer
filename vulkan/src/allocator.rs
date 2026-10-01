use crate::{Buffer, Result, SharedDeviceRef};

use ash::vk;

pub fn find_memory_index(
    memory_properties: ash::vk::PhysicalDeviceMemoryProperties,
    memory_requirements: ash::vk::MemoryRequirements,
    required_properties: ash::vk::MemoryPropertyFlags,
) -> Option<u32> {
    for i in 0..memory_properties.memory_type_count {
        let mem_type = memory_properties.memory_types[i as usize];
        let type_supported = (memory_requirements.memory_type_bits & (1 << i)) != 0;
        let properties_match = mem_type.property_flags.contains(required_properties);

        if type_supported && properties_match {
            return Some(i);
        }
    }
    return None;
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub struct AllocationRange {
    pub offset: u64,
    pub size: u64,
}

impl Default for AllocationRange {
    fn default() -> Self {
        Self { offset: 0, size: 0 }
    }
}

pub struct StackAllocator {
    buffer: Buffer,
    offset: u64,
}

impl StackAllocator {
    pub fn new_uniform(device: SharedDeviceRef, size: u64) -> Result<Self> {
        let create_info = crate::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
        };

        let buffer = crate::Buffer::new(device, &create_info)?;

        Ok(Self { buffer, offset: 0 })
    }
    pub fn new_storage(device: SharedDeviceRef, size: u64) -> Result<Self> {
        let create_info = crate::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::STORAGE_BUFFER,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
        };

        let buffer = crate::Buffer::new(device, &create_info)?;

        Ok(Self { buffer, offset: 0 })
    }
    pub fn new_indirect(device: SharedDeviceRef, size: u64) -> Result<Self> {
        let create_info = crate::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::INDIRECT_BUFFER,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
        };

        let buffer = crate::Buffer::new(device, &create_info)?;

        Ok(Self { buffer, offset: 0 })
    }
    pub fn new_vertex(device: SharedDeviceRef, size: u64) -> Result<Self> {
        let create_info = crate::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
        };

        let buffer = crate::Buffer::new(device, &create_info)?;

        Ok(Self { buffer, offset: 0 })
    }
    pub fn new_index(device: SharedDeviceRef, size: u64) -> Result<Self> {
        let create_info = crate::BufferCreateInfo {
            size,
            usage: vk::BufferUsageFlags::INDEX_BUFFER,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
        };

        let buffer = crate::Buffer::new(device, &create_info)?;

        Ok(Self { buffer, offset: 0 })
    }
    pub fn can_reserve(&self, byte_count: u64, alignment: u64) -> Option<AllocationRange> {
        let offset = self.offset.next_multiple_of(alignment);
        if offset + byte_count > self.buffer.size {
            return None;
        }

        Some(AllocationRange {
            offset,
            size: byte_count,
        })
    }
    pub fn reserve_data(&mut self, byte_count: u64, alignment: u64) -> Option<AllocationRange> {
        if self.can_reserve(byte_count, alignment).is_none() {
            return None;
        }

        self.offset = self.offset.next_multiple_of(alignment);

        let res = self.offset;
        self.offset += byte_count;

        Some(AllocationRange {
            offset: res,
            size: byte_count,
        })
    }
    pub unsafe fn upload_data<T>(&mut self, allocation: AllocationRange, data: &[T]) -> Result<()> {
        debug_assert!(std::mem::size_of::<T>() != 0);
        debug_assert!(allocation.size != 0);
        debug_assert!(allocation.offset + allocation.size <= self.buffer.size);

        let buffer = &self.buffer;

        let size = (data.len() * std::mem::size_of::<T>()) as u64;
        if allocation.size < size {
            return Err(crate::Error::OutsideAllocationRange);
        }

        unsafe {
            let dst = buffer.map_memory(allocation.offset, allocation.size)? as *mut T;
            dst.copy_from_nonoverlapping(data.as_ptr(), data.len());
            buffer.unmap();
        }

        Ok(())
    }
    #[inline]
    pub fn reset(&mut self) {
        self.offset = 0;
    }
    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.handle
    }
    #[inline]
    pub fn offset(&self) -> u64 {
        self.offset
    }
    #[inline]
    pub fn into_buffer(self) -> Buffer {
        self.buffer
    }
}

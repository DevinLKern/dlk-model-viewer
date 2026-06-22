mod frame_context;
mod render_pass;
mod resource_manager;
mod result;
mod scene;

include!(concat!(env!("OUT_DIR"), "/variable_types.rs"));
include!(concat!(env!("OUT_DIR"), "/shader_paths.rs"));
include!(concat!(env!("OUT_DIR"), "/entry_points.rs"));

pub use frame_context::*;
pub use render_pass::*;
pub(crate) use resource_manager::*;
pub use result::Error;
pub use result::Result;
pub use scene::*;

use ash::vk;
use std::rc::Rc;
use std::u64;
use vulkan::SharedDeviceRef;

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = unsafe { *p_callback_data };
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        std::borrow::Cow::from("")
    } else {
        unsafe { std::ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy() }
    };

    let message = if callback_data.p_message.is_null() {
        std::borrow::Cow::from("")
    } else {
        unsafe { std::ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy() }
    };

    let message = format!("{message_type:?} [{message_id_name} ({message_id_number})] : {message}");

    if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!(message);
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        tracing::warn!(message);
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        tracing::info!(message);
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE) {
        tracing::trace!(message);
    }

    vk::FALSE
}

#[allow(unused)]
pub struct Renderer {
    pub device: SharedDeviceRef,
    command_pool: vk::CommandPool,
    shader_modules: ShaderModuleResourceManager,
    descriptor_set_layouts: DescriptorSetLayoutResourceManager,
    pipeline_layouts: PipelineLayoutResourceManager,
    pipelines: PipelineResourceManager,
    repeat_sampler: vk::Sampler,
    mesh_arenas: slotmap::DenseSlotMap<MeshArenaHandle, MeshArena>,
}

impl Renderer {
    pub fn new(
        debug_enabled: bool,
        display_handle: &winit::raw_window_handle::DisplayHandle,
    ) -> result::Result<Renderer> {
        let instance = vulkan::Instance::new(debug_enabled, display_handle)?;
        let device = vulkan::Device::new(instance, Some(vulkan_debug_callback))?;

        let command_pool = {
            let command_pool_create_info = vk::CommandPoolCreateInfo {
                queue_family_index: device.get_queue_family_index(),
                ..Default::default()
            };

            unsafe { device.create_command_pool(&command_pool_create_info) }
                .inspect_err(|e| tracing::error!("{e}"))?
        };

        let shader_modules = crate::ShaderModuleResourceManager::new(device.clone());
        let descriptor_set_layouts = crate::DescriptorSetLayoutResourceManager::new(device.clone());
        let pipeline_layouts = crate::PipelineLayoutResourceManager::new(device.clone());
        let pipelines = crate::PipelineResourceManager::new(device.clone());

        let repeat_sampler = {
            let properties = unsafe { device.get_physical_device_properties() };
            let sampler_create_info = vk::SamplerCreateInfo {
                mag_filter: vk::Filter::LINEAR,
                min_filter: vk::Filter::LINEAR,
                mipmap_mode: vk::SamplerMipmapMode::LINEAR,
                address_mode_u: vk::SamplerAddressMode::REPEAT,
                address_mode_v: vk::SamplerAddressMode::REPEAT,
                address_mode_w: vk::SamplerAddressMode::REPEAT,
                mip_lod_bias: 0.0,
                anisotropy_enable: vk::TRUE,
                max_anisotropy: properties.limits.max_sampler_anisotropy,
                compare_enable: vk::FALSE,
                compare_op: vk::CompareOp::ALWAYS,
                ..Default::default()
            };

            unsafe { device.create_sampler(&sampler_create_info) }.inspect_err(|e| {
                tracing::error!("{e}");
                unsafe {
                    device.destroy_command_pool(command_pool);
                }
            })?
        };

        Ok(Self {
            device,
            command_pool,
            shader_modules,
            descriptor_set_layouts,
            pipeline_layouts,
            pipelines,
            mesh_arenas: slotmap::DenseSlotMap::with_key(),
            repeat_sampler,
        })
    }
    // TODO: Add RenderPass trait
    #[inline]
    pub fn render_main_scene(
        &mut self,
        ctx: &mut FrameContext,
        scene: &Scene,
        pass: &MainRenderPass,
        camera_data: CameraUBO,
    ) -> Result<()> {
        pass.render(
            ctx,
            &mut self.pipelines,
            &mut self.pipeline_layouts,
            &mut self.shader_modules,
            &self.mesh_arenas,
            scene,
            camera_data,
        )
    }
    #[inline]
    pub fn render_grid_scene(
        &mut self,
        ctx: &mut FrameContext,
        scene: &Scene,
        pass: &GridRenderPass,
        camera_data: CameraUBO,
    ) -> Result<()> {
        pass.render(
            ctx,
            &mut self.pipelines,
            &mut self.pipeline_layouts,
            &mut self.shader_modules,
            &self.mesh_arenas,
            scene,
            camera_data,
        )
    }
    pub fn access_or_create_pipeline_layout(
        &mut self,
        desc: PipelineLayoutDescription,
    ) -> Result<PipelineLayoutResourceHandle> {
        self.pipeline_layouts
            .access_or_create(desc, &self.descriptor_set_layouts)
    }
    pub fn mesh_arenas_mut(&mut self) -> &mut slotmap::DenseSlotMap<MeshArenaHandle, MeshArena> {
        &mut self.mesh_arenas
    }
    pub fn pipeline_layouts_mut(&mut self) -> &mut PipelineLayoutResourceManager {
        &mut self.pipeline_layouts
    }
    pub fn descriptor_set_layouts(&self) -> &DescriptorSetLayoutResourceManager {
        &self.descriptor_set_layouts
    }
    pub fn descriptor_set_layouts_mut(&mut self) -> &mut DescriptorSetLayoutResourceManager {
        &mut self.descriptor_set_layouts
    }
    pub fn shader_modules_mut(&mut self) -> &mut ShaderModuleResourceManager {
        &mut self.shader_modules
    }
    pub fn piplines_mut(&mut self) -> &mut PipelineResourceManager {
        &mut self.pipelines
    }
    fn create_transfer_buffer(&self, size: u64) -> result::Result<vulkan::Buffer> {
        let create_info = vulkan::BufferCreateInfo {
            size: size,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
        };

        let buffer = vulkan::Buffer::new(self.device.clone(), &create_info)
            .inspect_err(|e| tracing::error!("{}", e))?;

        Ok(buffer)
    }
    fn get_command_buffer(&self) -> Result<vk::CommandBuffer> {
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo {
            command_pool: self.command_pool,
            command_buffer_count: 1,
            level: vk::CommandBufferLevel::PRIMARY,
            ..Default::default()
        };

        let command_buffers = unsafe {
            self.device
                .allocate_command_buffers(&command_buffer_allocate_info)
        }?;

        Ok(*command_buffers.get(0).unwrap())
    }
    pub fn create_uniform_buffers(
        &self,
        size: u64,
        count: u64,
    ) -> result::Result<Box<[vulkan::UniformBV]>> {
        let buffer = {
            let buffer_create_info = vulkan::BufferCreateInfo {
                size: size * count,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &buffer_create_info)?
        };

        let buffer = Rc::new(buffer);

        let views: Box<[vulkan::UniformBV]> = (0..count)
            .map(|i| vulkan::UniformBV {
                buffer: buffer.clone(),
                offset: i * size,
                size: size,
            })
            .collect();

        Ok(views)
    }
    pub fn update_uniform_buffer(
        &self,
        data: *const u8,
        byte_count: usize,
        uniform_bv: &vulkan::UniformBV,
    ) -> result::Result<()> {
        unsafe {
            let dst = uniform_bv
                .buffer
                .map_memory(uniform_bv.offset, uniform_bv.size)?;
            std::ptr::copy_nonoverlapping(data, dst as *mut u8, byte_count);
            Ok(uniform_bv.buffer.unmap())
        }
    }
    pub fn create_dynamic_uniform_buffer(&self, size: u64) -> Result<vulkan::Buffer> {
        let buffer = {
            let create_info = vulkan::BufferCreateInfo {
                size: size,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &create_info)
                .inspect_err(|e| tracing::error!("{}", e))?
        };

        Ok(buffer)
    }
    pub fn create_indirect_buffer(&self, size: u64) -> Result<vulkan::Buffer> {
        let buffer = {
            let create_info = vulkan::BufferCreateInfo {
                size,
                usage: vk::BufferUsageFlags::INDIRECT_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &create_info)
                .inspect_err(|e| tracing::error!("{}", e))?
        };

        Ok(buffer)
    }
    pub fn update_dynamic_uniform_buffer(
        &self,
        data: *const u8,
        byte_count: usize,
        uniform_bv: &vulkan::DynamicUniformBV,
    ) -> result::Result<()> {
        unsafe {
            let dst = uniform_bv
                .buffer
                .map_memory(uniform_bv.offset, uniform_bv.size)?;

            std::ptr::copy_nonoverlapping(data, dst as *mut u8, byte_count);

            Ok(uniform_bv.buffer.unmap())
        }
    }
    pub fn create_image(
        &mut self,
        image_data: image::DynamicImage,
    ) -> result::Result<vulkan::Image> {
        use image::GenericImageView;

        let (width, height) = image_data.dimensions();
        let rgba = image_data.into_rgba8();
        let data = rgba.as_raw();
        let size = data.len() as u64;

        let image = {
            let image_create_info = vulkan::ImageCreateInfo {
                memory_property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                mip_levels: 1,
                image_type: vk::ImageType::TYPE_2D,
                format: vk::Format::R8G8B8A8_SRGB,
                width,
                height,
                depth: 1,
                usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                array_layers: 1,
            };

            vulkan::Image::new(self.device.clone(), &image_create_info)?
        };

        let transfer_buffer = self.create_transfer_buffer(size)?;

        unsafe {
            let dst = transfer_buffer.map_memory(0, size)?;

            std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, size as usize);

            transfer_buffer.unmap();
        }

        let command_buffer = self.get_command_buffer()?;

        {
            let begin_info = vk::CommandBufferBeginInfo {
                flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                ..Default::default()
            };
            unsafe {
                self.device
                    .begin_command_buffer(command_buffer, &begin_info)
            }?;
        }

        // transfer commands here
        {
            // to I need the stage mask here?
            let barriers = [vk::ImageMemoryBarrier2 {
                image: image.handle,
                old_layout: vk::ImageLayout::UNDEFINED,
                new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                src_access_mask: vk::AccessFlags2::NONE,
                dst_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                ..Default::default()
            }];

            let dependency_info = vk::DependencyInfo {
                image_memory_barrier_count: barriers.len() as u32,
                p_image_memory_barriers: barriers.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device
                    .cmd_pipeline_barrier2(command_buffer, &dependency_info)
            };

            let regions = [vk::BufferImageCopy2 {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: image.width,
                    height: image.height,
                    depth: image.depth,
                },
                ..Default::default()
            }];

            let copy_buffer_to_image_info = vk::CopyBufferToImageInfo2 {
                src_buffer: transfer_buffer.handle,
                dst_image: image.handle,
                dst_image_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                region_count: regions.len() as u32,
                p_regions: regions.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device
                    .cmd_copy_buffer_to_image2(command_buffer, &copy_buffer_to_image_info)
            };

            let barriers = [vk::ImageMemoryBarrier2 {
                image: image.handle,
                old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
                src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                dst_access_mask: vk::AccessFlags2::SHADER_READ,
                ..Default::default()
            }];

            let dependency_info = vk::DependencyInfo {
                image_memory_barrier_count: barriers.len() as u32,
                p_image_memory_barriers: barriers.as_ptr(),
                ..Default::default()
            };

            unsafe {
                self.device
                    .cmd_pipeline_barrier2(command_buffer, &dependency_info)
            };
        }

        unsafe {
            self.device.end_command_buffer(command_buffer)?;

            let submit_info = [vk::SubmitInfo {
                command_buffer_count: 1,
                p_command_buffers: &command_buffer,
                ..Default::default()
            }];

            self.device
                .queue_submit(self.device.queue, &submit_info, vk::Fence::null())?;
            self.device.device_wait_idle()?;
            self.device
                .free_command_buffers(self.command_pool, &[command_buffer]);
        }

        Ok(image)
    }
    pub fn create_mesh_arena(
        &mut self,
        vertices: &[u8],
        indices: &[u32],
    ) -> Result<MeshArenaHandle> {
        let buffer_size = vertices.len() as u64;

        let vertex_buffer = {
            let buffer_create_info = vulkan::BufferCreateInfo {
                size: buffer_size,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
                memory_property_flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &buffer_create_info)?
        };

        unsafe {
            let src = vertices.as_ptr();
            let dst = vertex_buffer.map_memory(0, buffer_size)? as *mut u8;

            std::ptr::copy_nonoverlapping(src, dst, buffer_size as usize);

            vertex_buffer.unmap();
        }

        let buffer_size = (indices.len() * std::mem::size_of::<u32>()) as u64;

        let index_buffer = {
            let buffer_create_info = vulkan::BufferCreateInfo {
                size: buffer_size,
                usage: vk::BufferUsageFlags::INDEX_BUFFER,
                memory_property_flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &buffer_create_info)?
        };

        unsafe {
            let src = indices.as_ptr();
            let dst = index_buffer.map_memory(0, buffer_size)? as *mut u32;

            std::ptr::copy_nonoverlapping(src, dst, indices.len());

            index_buffer.unmap();
        }

        let handle = self.mesh_arenas.insert(MeshArena {
            vertex_buffer,
            index_buffer,
        });

        Ok(handle)
    }
    #[inline]
    pub fn access_mesh_arena(&mut self, handle: MeshArenaHandle) -> Option<&MeshArena> {
        self.mesh_arenas.get(handle)
    }
    #[inline]
    pub fn destroy_mesh_arena(&mut self, handle: MeshArenaHandle) -> bool {
        self.mesh_arenas.remove(handle).is_some()
    }
    #[inline]
    pub fn bind_descriptor_sets(
        &self,
        cmd: vk::CommandBuffer,
        pipeline_layout: PipelineLayoutResourceHandle,
        first_set: u32,
        sets: &[vk::DescriptorSet],
        dynamic_offsets: &[u32],
    ) {
        if let Some(layout) = self.pipeline_layouts.get(pipeline_layout) {
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    layout.desc.bind_point,
                    layout.raw,
                    first_set,
                    sets,
                    dynamic_offsets,
                );
            }
        }
    }
    #[inline]
    pub fn repeat_sampler(&self) -> vk::Sampler {
        self.repeat_sampler
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            self.device.destroy_sampler(self.repeat_sampler);
            self.device.destroy_command_pool(self.command_pool);
        }
    }
}

mod render_context;
mod result;

include!(concat!(env!("OUT_DIR"), "/variable_types.rs"));
include!(concat!(env!("OUT_DIR"), "/shader_paths.rs"));
include!(concat!(env!("OUT_DIR"), "/entry_points.rs"));

pub use render_context::RenderContext;
pub use result::Error;
pub use result::Result;

use ash::vk;
use std::rc::Rc;
use vulkan::device::SharedDeviceRef;

use crate::render_context::MAX_FRAME_COUNT;

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

pub const MAX_CONTEXTS: u32 = 1;
pub const MAX_TEXTURES: u32 = 16;
pub const MAX_MATERIALS: u32 = 32;

// use crate::render_context::MAX_TEXTURES;

pub struct Renderer {
    pub device: SharedDeviceRef,
    pub pipeline_layout: Rc<vulkan::PipelineLayout>,
    command_pool: vk::CommandPool,
    descriptor_pool: vk::DescriptorPool,
    per_frame_ds_layout: vk::DescriptorSetLayout,
    per_obj_ds_layout: vk::DescriptorSetLayout,
    other_ds_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: Box<[vk::DescriptorSet]>,

    pub model_transform_buffer: vulkan::Buffer,
    model_transform_index: u64,
    model_transform_buffer_element_size: u64,

    global_light_buffer: vulkan::Buffer,

    textures: Vec<vulkan::Image>,

    material_buffer: vulkan::Buffer,
    material_buffer_offset: u32,
    pub material_buffer_element_size: u32,

    repeat_sampler: vk::Sampler,
}

impl Renderer {
    pub fn new(
        debug_enabled: bool,
        display_handle: &winit::raw_window_handle::DisplayHandle,
        model_transform_capacity: u64,
        texture_capacity: u64,
        material_capacity: u64,
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

        let textures = Vec::<vulkan::Image>::with_capacity(texture_capacity as usize);

        let (model_transform_buffer, model_transform_buffer_element_size) = {
            let element_size = {
                let ubo_size = std::mem::size_of::<crate::InstanceData>();

                // let properties = unsafe { device.get_physical_device_properties() };

                // ubo_size.next_multiple_of(
                //     properties.limits.min_storage_buffer_offset_alignment as usize,
                // );

                ubo_size
            };

            let model_transform_buffer_create_info = vulkan::BufferCreateInfo {
                size: element_size as u64 * model_transform_capacity,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            let buffer = vulkan::Buffer::new(device.clone(), &model_transform_buffer_create_info)?;

            (buffer, element_size)
        };

        let global_light_buffer = {
            let element_size = {
                let ubo_size = std::mem::size_of::<crate::GlobalLightUBO>();

                let properties = unsafe { device.get_physical_device_properties() };

                ubo_size.next_multiple_of(
                    properties.limits.min_uniform_buffer_offset_alignment as usize,
                )
            };

            let global_light_buffer_create_info = vulkan::BufferCreateInfo {
                size: element_size as u64,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(device.clone(), &global_light_buffer_create_info)?
        };

        let (material_buffer, material_buffer_element_size) = {
            let element_size = {
                let ubo_size = std::mem::size_of::<crate::MaterialUBO>();

                let properties = unsafe { device.get_physical_device_properties() };

                ubo_size.next_multiple_of(
                    properties.limits.min_storage_buffer_offset_alignment as usize,
                )
            };

            let buffer_create_info = vulkan::BufferCreateInfo {
                size: (element_size as u64 * material_capacity),
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            let buffer = vulkan::Buffer::new(device.clone(), &buffer_create_info)?;

            (buffer, element_size as u32)
        };

        let ds_layout_bindings: &[&[vk::DescriptorSetLayoutBinding]] = &[
            // SET 0 - per frame (update in render_context.rs)
            &[vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX,
                ..Default::default()
            }],
            // SET 1 - per obj
            &[vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                p_immutable_samplers: std::ptr::null(),
                _marker: std::marker::PhantomData {},
            }],
            // SET 2 - irregular updates
            &[
                //
                vk::DescriptorSetLayoutBinding {
                    binding: 0,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                    p_immutable_samplers: std::ptr::null(),
                    _marker: std::marker::PhantomData {},
                },
                // global_textures
                vk::DescriptorSetLayoutBinding {
                    binding: 1,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: texture_capacity as u32,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                    p_immutable_samplers: std::ptr::null(),
                    _marker: std::marker::PhantomData {},
                },
                // materials
                vk::DescriptorSetLayoutBinding {
                    binding: 2,
                    descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                    p_immutable_samplers: std::ptr::null(),
                    _marker: std::marker::PhantomData {},
                },
            ],
        ];

        let pipeline_layout = Rc::new(vulkan::PipelineLayout::new(
            device.clone(),
            ds_layout_bindings,
        )?);

        let descriptor_pool = {
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: MAX_MATERIALS,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: MAX_TEXTURES,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                    descriptor_count: 2,
                },
            ];
            let descrptor_pool_create_info = vk::DescriptorPoolCreateInfo {
                max_sets: crate::MAX_FRAME_COUNT as u32 + MAX_MATERIALS + 1,
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_pool(&descrptor_pool_create_info) }.inspect_err(
                |e| {
                    unsafe {
                        device.destroy_command_pool(command_pool);
                    }
                    tracing::error!("{e}")
                },
            )?
        };

        let per_frame_ds_layout = {
            // 0 == per frame index
            let ds_layout_create_info = vk::DescriptorSetLayoutCreateInfo {
                binding_count: ds_layout_bindings[0].len() as u32,
                p_bindings: ds_layout_bindings[0].as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_set_layout(&ds_layout_create_info) }.inspect_err(
                |e| {
                    tracing::error!("{e}");
                    unsafe {
                        device.destroy_command_pool(command_pool);
                        device.destroy_descriptor_pool(descriptor_pool);
                    }
                },
            )?
        };

        let per_obj_ds_layout = {
            // 1 == per obj index
            let ds_layout_create_info = vk::DescriptorSetLayoutCreateInfo {
                binding_count: ds_layout_bindings[1].len() as u32,
                p_bindings: ds_layout_bindings[1].as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_set_layout(&ds_layout_create_info) }.inspect_err(
                |e| {
                    tracing::error!("{e}");
                    unsafe {
                        device.destroy_descriptor_set_layout(per_frame_ds_layout);
                        device.destroy_command_pool(command_pool);
                        device.destroy_descriptor_pool(descriptor_pool);
                    }
                },
            )?
        };

        let other_ds_layout = {
            // 2 == other index
            let ds_layout_create_info = vk::DescriptorSetLayoutCreateInfo {
                binding_count: ds_layout_bindings[2].len() as u32,
                p_bindings: ds_layout_bindings[2].as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_set_layout(&ds_layout_create_info) }.inspect_err(
                |e| {
                    tracing::error!("{e}");
                    unsafe {
                        device.destroy_descriptor_set_layout(per_obj_ds_layout);
                        device.destroy_descriptor_set_layout(per_frame_ds_layout);
                        device.destroy_command_pool(command_pool);
                        device.destroy_descriptor_pool(descriptor_pool);
                    }
                },
            )?
        };

        let descriptor_sets = {
            let ds_layouts = [per_frame_ds_layout, per_obj_ds_layout, other_ds_layout];
            let ds_create_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool,
                descriptor_set_count: ds_layouts.len() as u32,
                p_set_layouts: ds_layouts.as_ptr(),
                ..Default::default()
            };

            unsafe { device.allocate_descriptor_sets(&ds_create_info) }.inspect_err(|e| {
                tracing::error!("{e}");
                unsafe {
                    device.destroy_descriptor_set_layout(per_obj_ds_layout);
                    device.destroy_descriptor_set_layout(per_frame_ds_layout);
                    device.destroy_descriptor_set_layout(other_ds_layout);
                    device.destroy_command_pool(command_pool);
                    device.destroy_descriptor_pool(descriptor_pool);
                }
            })?
        };

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
                    device.destroy_descriptor_set_layout(per_obj_ds_layout);
                    device.destroy_descriptor_set_layout(per_frame_ds_layout);
                    device.destroy_descriptor_set_layout(other_ds_layout);
                    device.destroy_command_pool(command_pool);
                    device.destroy_descriptor_pool(descriptor_pool);
                }
            })?
        };

        {
            let per_obj_descriptor_set_info = vk::DescriptorBufferInfo {
                buffer: model_transform_buffer.handle,
                offset: 0,
                range: model_transform_buffer_element_size as u64 * model_transform_capacity,
            };
            let global_light_buffer_info = vk::DescriptorBufferInfo {
                buffer: global_light_buffer.handle,
                offset: 0,
                range: global_light_buffer.size,
            };

            let material_buffer_info = vk::DescriptorBufferInfo {
                buffer: material_buffer.handle,
                offset: 0,
                range: material_buffer.size,
            };

            let writes = [
                // set 0 updated in render_context.rs
                vk::WriteDescriptorSet {
                    dst_set: descriptor_sets[1],
                    dst_binding: 0,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                    p_buffer_info: &per_obj_descriptor_set_info,
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: descriptor_sets[2],
                    dst_binding: 0,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    p_buffer_info: &global_light_buffer_info,
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: descriptor_sets[2],
                    dst_binding: 2,
                    descriptor_count: 1,
                    descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                    p_buffer_info: &material_buffer_info,
                    ..Default::default()
                },
            ];

            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        Ok(Renderer {
            device,
            pipeline_layout,
            command_pool,
            descriptor_pool,
            per_frame_ds_layout,
            per_obj_ds_layout,
            other_ds_layout,
            descriptor_sets: descriptor_sets.into_boxed_slice(),

            model_transform_buffer,
            model_transform_index: 0,
            model_transform_buffer_element_size: model_transform_buffer_element_size as u64,

            global_light_buffer,

            textures,

            material_buffer,
            material_buffer_offset: 0,
            material_buffer_element_size,
            repeat_sampler,
        })
    }
    pub fn create_render_context(&self, window: &winit::window::Window) -> Result<RenderContext> {
        RenderContext::new(
            self.device.clone(),
            self.pipeline_layout.clone(),
            window,
            self.descriptor_sets[0],
        )
    }
    pub fn update_world_light(
        &self,
        ambient: f32,
        dir: math::Vec3<f32>,
        color: math::Vec4<f32>,
    ) -> Result<()> {
        let ubo = crate::GlobalLightUBO {
            direction: dir.normalized().into_vec4().into_arr(),
            color: color.as_arr(),
            ambient,
        };

        unsafe {
            let dst = self
                .global_light_buffer
                .map_memory(0, std::mem::size_of::<crate::GlobalLightUBO>() as u64)?;
            let src = &ubo;

            std::ptr::copy_nonoverlapping(src, dst as *mut crate::GlobalLightUBO, 1);

            self.global_light_buffer.unmap();
        }

        Ok(())
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
    pub fn create_vertex_buffer(
        &self,
        data: &[u8],
        vertex_count: u32,
    ) -> vulkan::Result<vulkan::VertexBuffer> {
        let buffer = {
            let buffer_create_info = vulkan::BufferCreateInfo {
                size: data.len() as u64,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
                memory_property_flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &buffer_create_info)?
        };

        unsafe {
            let dst = buffer.map_memory(buffer.offset, buffer.size)?;

            std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, data.len());

            buffer.unmap();
        }

        let view = vulkan::VertexBuffer {
            buffer,
            vertex_count,
            instance_count: 1,
            first_binding: 0,
            offset: 0,
        };

        Ok(view)
    }
    pub fn create_index_buffer(
        &self,
        data: &[u8],
        index_type: vk::IndexType,
        index_count: u32,
        first_index: u32,
    ) -> result::Result<vulkan::IndexBuffer> {
        let buffer = {
            let buffer_create_info = vulkan::buffer::BufferCreateInfo {
                size: data.len() as u64,
                usage: vk::BufferUsageFlags::INDEX_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(self.device.clone(), &buffer_create_info)?
        };

        unsafe {
            let dst = buffer.map_memory(buffer.offset, buffer.size)?;

            std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, data.len());

            buffer.unmap();
        }

        let view = vulkan::IndexBuffer {
            buffer,
            offset: 0,
            index_count,
            instance_count: 1,
            first_index,
            vertex_offset: 0,
            first_instance: 0,
            index_type,
        };

        Ok(view)
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
    pub fn create_image(&mut self, image_data: image::DynamicImage) -> result::Result<usize> {
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

        let idx = self.textures.len();

        let image_infos = [vk::DescriptorImageInfo {
            sampler: self.repeat_sampler,
            image_view: image.view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let writes = [vk::WriteDescriptorSet {
            dst_set: self.descriptor_sets[2],
            dst_binding: 1,
            descriptor_count: image_infos.len() as u32,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            p_image_info: image_infos.as_ptr(),
            dst_array_element: idx as u32,
            ..Default::default()
        }];

        unsafe { self.device.update_descriptor_sets(&writes, &[]) }
        self.textures.push(image);
        Ok(idx)
    }
    pub fn add_material(&mut self, material_data: crate::MaterialUBO) -> Result<u32> {
        let data = material_data;
        let write_offset = self.material_buffer_offset as u64;
        let write_size = self.material_buffer_element_size as u64;
        let requested_end = write_offset + write_size;
        if requested_end > self.material_buffer.size {
            return Err(Error::BufferCapacityExceeded {
                buffer: "material_buffer",
                requested_end,
                capacity: self.material_buffer.size,
            });
        }

        unsafe {
            let dst = self.material_buffer.map_memory(write_offset, write_size)?;
            let src = &data;
            std::ptr::copy_nonoverlapping(src, dst as *mut crate::MaterialUBO, 1);
            self.material_buffer.unmap();
        }
        let res = self.material_buffer_offset;
        self.material_buffer_offset += self.material_buffer_element_size;
        Ok(res)
    }
    pub fn add_instance(
        &mut self,
        model_matrix: math::Mat4<f32>,
        material_index: u32,
    ) -> Result<u32> {
        let data = crate::InstanceData {
            model_matrix: model_matrix.as_2d_arr(),
            normal_matrix: model_matrix
                .as_mat3()
                .transposed()
                .inverse()
                .unwrap()
                .into_mat4(1.0)
                .as_2d_arr(),
            material_index,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let write_offset = self.model_transform_index * self.model_transform_buffer_element_size;
        let write_size = self.model_transform_buffer_element_size;
        let requested_end = write_offset + write_size;
        if requested_end > self.model_transform_buffer.size {
            return Err(Error::BufferCapacityExceeded {
                buffer: "model_transform_buffer",
                requested_end,
                capacity: self.model_transform_buffer.size,
            });
        }

        unsafe {
            let dst = self
                .model_transform_buffer
                .map_memory(write_offset, write_size)?;
            let src = &data;
            std::ptr::copy_nonoverlapping(src, dst as *mut crate::InstanceData, 1);
            self.model_transform_buffer.unmap();
        }
        let res = self.model_transform_index;
        self.model_transform_index += 1;
        Ok(res as u32)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_sampler(self.repeat_sampler);
            self.device
                .destroy_descriptor_set_layout(self.per_obj_ds_layout);
            self.device
                .destroy_descriptor_set_layout(self.per_frame_ds_layout);
            self.device
                .destroy_descriptor_set_layout(self.other_ds_layout);
            self.device.destroy_command_pool(self.command_pool);
            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

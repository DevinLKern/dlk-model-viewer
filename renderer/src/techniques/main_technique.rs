use std::{sync::Arc, sync::atomic::AtomicU32};

use ash::vk;

use crate::{
    CameraUBO, CanResolveBindingValue, DescriptorSetLayoutDescription,
    DescriptorSetLayoutResourceHandle, DirectionalLightUBO, Error, FrameContext,
    FrameContextImageHandle, FrameContextRange, GlobalLightUBO, ImageResourceHandle,
    InstanceBuffer, MAX_FRAME_COUNT, MainMaterialUBO, MaterialHandle, PointLightsUBO,
    RenderStorage, RenderTarget, RenderTechnique, Renderer, ResourceRegistry, Resources, Result,
    Storage, Uniform,
    techniques::{HasBindingValue, ShaderBinding},
};

const COMPILED_MAIN_VERT_SHADER: &[u8] = include_bytes!("../../shaders/shader.vert.spv");
const COMPILED_MAIN_FRAG_SHADER: &[u8] = include_bytes!("../../shaders/shader.frag.spv");

#[derive(Copy, Clone, Debug)]
pub struct MainImageHandle {
    id: u32,
    index: u32,
}

pub struct MainMaterialData {
    pub diffuse_base: math::Vec3<f32>,
    pub diffuse_texture: Option<MainImageHandle>,
    pub ambient_base: math::Vec3<f32>,
    pub ambient_texture: Option<MainImageHandle>,
    pub specular_base: math::Vec3<f32>,
    pub specular_texture: Option<MainImageHandle>,
    pub shininess: f32,
}

pub struct MainResourcesRegistry {
    id: u32,
    images: Vec<(ImageResourceHandle, vk::Sampler)>,
    materials: Vec<MainMaterialUBO>,
    global_light_ubo_data: GlobalLightUBO,
}

static NEXT_MAIN_NODE_ID: AtomicU32 = AtomicU32::new(0);
impl MainResourcesRegistry {
    fn get_id() -> u32 {
        NEXT_MAIN_NODE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
    #[inline]
    pub fn new() -> Self {
        Self {
            id: Self::get_id(),
            materials: Vec::with_capacity(16),
            images: Vec::new(),
            global_light_ubo_data: GlobalLightUBO {
                direction: [0.0; 4],
                color: [1.0; 4],
                ambient: 0.1,
            },
        }
    }
    #[inline]
    pub fn set_global_light_ubo_data(&mut self, data: GlobalLightUBO) {
        self.global_light_ubo_data = data;
    }
    pub fn add_image(
        &mut self,
        image_handle: ImageResourceHandle,
        sampler: vk::Sampler,
    ) -> MainImageHandle {
        let index: u32 = self.images.len().try_into().expect("usize exceeds u32");
        self.images.push((image_handle, sampler));
        MainImageHandle { id: self.id, index }
    }
    fn get_image(
        &self,
        image_handle: MainImageHandle,
    ) -> Option<&(ImageResourceHandle, vk::Sampler)> {
        if image_handle.id != self.id {
            return None;
        }
        self.images.get(image_handle.index as usize)
    }
    #[inline]
    pub fn add_material(
        &mut self,
        material_data: MainMaterialData,
        registry: &mut ResourceRegistry,
    ) -> Result<MaterialHandle<MainMaterialUBO>> {
        let index = self.materials.len() as u32;

        const MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT: u32 = 1 << 0;
        const MATERIAL_FLAG_AMBIENT_TEXTURE_BIT: u32 = 1 << 1;
        const MATERIAL_FLAG_SPECULAR_TEXTURE_BIT: u32 = 1 << 2;

        let mut flags: u32 = 0;

        let diffuse_texture_index = match material_data.diffuse_texture {
            Some(handle) => {
                let (image, _) = self.get_image(handle).ok_or(Error::ResourceMissing)?;
                registry.get_image(*image).ok_or(Error::ResourceMissing)?;

                flags |= MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT;
                image.index()
            }
            None => 0,
        };
        let ambient_texture_index = match material_data.ambient_texture {
            Some(handle) => {
                let (image, _) = self.get_image(handle).ok_or(Error::ResourceMissing)?;
                registry.get_image(*image).ok_or(Error::ResourceMissing)?;

                flags |= MATERIAL_FLAG_AMBIENT_TEXTURE_BIT;
                image.index()
            }
            None => 0,
        };
        let specular_texture_index = match material_data.specular_texture {
            Some(handle) => {
                let (image, _) = self.get_image(handle).ok_or(Error::ResourceMissing)?;
                registry.get_image(*image).ok_or(Error::ResourceMissing)?;

                flags |= MATERIAL_FLAG_SPECULAR_TEXTURE_BIT;
                image.index()
            }
            None => 0,
        };

        self.materials.push(MainMaterialUBO {
            diffuse_base: material_data.diffuse_base.as_arr(),
            diffuse_texture_index,
            ambient_base: material_data.ambient_base.as_arr(),
            ambient_texture_index,
            specular_base: material_data.specular_base.as_arr(),
            specular_texture_index,
            shininess: material_data.shininess,
            flags,
            _pad0: 0,
            _pad1: 0,
        });

        Ok(crate::MaterialHandle::new(self.id, index))
    }
    #[inline]
    pub fn register(self, registry: &mut ResourceRegistry) -> MainResourcesUploader {
        registry.add_storage_allocation(
            (self.materials.len() * std::mem::size_of::<MainMaterialUBO>()) as u64,
            std::mem::size_of::<MainMaterialUBO>() as u64,
        );
        registry.add_uniform_allocation(
            std::mem::size_of::<GlobalLightUBO>() as u64,
            std::mem::align_of::<GlobalLightUBO>() as u64,
        );

        registry.add_descriptors(vk::DescriptorType::UNIFORM_BUFFER, 1);
        registry.add_descriptors(vk::DescriptorType::STORAGE_BUFFER, 1);
        registry.add_sets(1);

        MainResourcesUploader {
            id: self.id,
            scene_id: registry.id(),
            images: self.images.into_boxed_slice(),
            materials: self.materials.into_boxed_slice(),
            global_light_ubo_data: self.global_light_ubo_data,
        }
    }
}

pub struct MainResourcesUploader {
    id: u32,
    scene_id: u32,
    images: Box<[(ImageResourceHandle, vk::Sampler)]>,
    materials: Box<[MainMaterialUBO]>,
    global_light_ubo_data: GlobalLightUBO,
}

impl MainResourcesUploader {
    #[inline]
    pub fn set_global_light_ubo_data(&mut self, ubo: GlobalLightUBO) {
        self.global_light_ubo_data = ubo;
    }
    pub fn upload(self, uploader: &mut crate::ResourceUploader) -> Result<MainResourcesPromise> {
        debug_assert!(self.scene_id == uploader.id());

        let materials_allocation_size =
            (self.materials.len() * std::mem::size_of::<MainMaterialUBO>()) as u64;
        let global_light_ubo_allocation_size = (std::mem::size_of::<GlobalLightUBO>()) as u64;

        // TODO: Add error type(s)?
        let _ = uploader
            .storage_allocator
            .can_reserve(
                materials_allocation_size,
                std::mem::size_of::<MainMaterialUBO>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;
        let _ = uploader
            .uniform_allocator
            .can_reserve(
                global_light_ubo_allocation_size,
                std::mem::align_of::<GlobalLightUBO>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;

        let material_ubos_range = uploader
            .storage_allocator
            .reserve_data(
                materials_allocation_size,
                std::mem::size_of::<MainMaterialUBO>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;
        let global_light_ubo_range = uploader
            .uniform_allocator
            .reserve_data(
                global_light_ubo_allocation_size,
                std::mem::align_of::<GlobalLightUBO>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;

        unsafe {
            uploader
                .storage_allocator
                .upload_data(material_ubos_range, &self.materials)?;
            uploader
                .uniform_allocator
                .upload_data(global_light_ubo_range, &[self.global_light_ubo_data])?;
        }

        Ok(MainResourcesPromise {
            id: self.id,
            images: self.images,
            scene_id: self.scene_id,
            material_ubos_range,
            global_light_ubo_range,
        })
    }
}

pub struct MainResourcesPromise {
    id: u32,
    scene_id: u32,
    images: Box<[(ImageResourceHandle, vk::Sampler)]>,
    material_ubos_range: vulkan::AllocationRange,
    global_light_ubo_range: vulkan::AllocationRange,
}

impl MainResourcesPromise {
    pub fn finalize(
        self,
        resources: Arc<Resources>,
        renderer: &mut Renderer,
    ) -> Result<MainResources> {
        debug_assert!(self.scene_id == resources.id());

        let other_descriptor_set_layout_handle = {
            let bindings: &[crate::DescriptorSetLayoutBindingInfo] = &[
                // global light
                crate::DescriptorSetLayoutBindingInfo {
                    binding: 0,
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
                // global_textures
                crate::DescriptorSetLayoutBindingInfo {
                    binding: 1,
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    count: self.images.len() as u32,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
                // materials
                crate::DescriptorSetLayoutBindingInfo {
                    binding: 2,
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
            ];
            let description = DescriptorSetLayoutDescription {
                bindings: bindings.into(),
            };
            renderer
                .descriptor_set_layouts_mut()
                .access_or_create(description)?
        };

        let other_descriptor_set = {
            let layout = *renderer
                .descriptor_set_layouts
                .get(other_descriptor_set_layout_handle)
                .ok_or(Error::ResourceMissing)?;
            let set_layouts = [layout];

            let alloc_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool: resources.descriptor_pool(),
                descriptor_set_count: 1,
                p_set_layouts: set_layouts.as_ptr(),
                ..Default::default()
            };

            let sets = unsafe { renderer.device.allocate_descriptor_sets(&alloc_info)? };
            sets[0]
        };

        {
            let mut image_info = Vec::with_capacity(self.images.len());
            for (image_handle, sampler) in &self.images {
                let image = resources
                    .get_image(*image_handle)
                    .ok_or(Error::ResourceMissing)?;

                image_info.push(vk::DescriptorImageInfo {
                    sampler: *sampler,
                    image_view: image.view,
                    image_layout: image.layout,
                });
            }

            let image_info: Box<[_]> = image_info.into_boxed_slice();

            let global_light_buffer_info = [vk::DescriptorBufferInfo {
                buffer: resources.uniform_buffer().handle,
                offset: self.global_light_ubo_range.offset,
                range: self.global_light_ubo_range.size,
            }];
            let material_buffer_info = [vk::DescriptorBufferInfo {
                buffer: resources.storage_buffer().handle,
                offset: self.material_ubos_range.offset,
                range: self.material_ubos_range.size,
            }];
            let writes = [
                vk::WriteDescriptorSet {
                    dst_set: other_descriptor_set,
                    dst_binding: 0,
                    dst_array_element: 0,
                    descriptor_count: global_light_buffer_info.len() as u32,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    p_buffer_info: global_light_buffer_info.as_ptr(),
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: other_descriptor_set,
                    dst_binding: 1,
                    dst_array_element: 0,
                    descriptor_count: image_info.len() as u32,
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    p_image_info: image_info.as_ptr(),
                    ..Default::default()
                },
                vk::WriteDescriptorSet {
                    dst_set: other_descriptor_set,
                    dst_binding: 2,
                    dst_array_element: 0,
                    descriptor_count: material_buffer_info.len() as u32,
                    descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                    p_buffer_info: material_buffer_info.as_ptr(),
                    ..Default::default()
                },
            ];
            unsafe { renderer.device.update_descriptor_sets(&writes, &[]) };
        }

        let descriptor_set_layout_bindings: &[crate::DescriptorSetLayoutBindingInfo] = &[
            crate::DescriptorSetLayoutBindingInfo {
                binding: 0,
                ty: vk::DescriptorType::STORAGE_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            },
            crate::DescriptorSetLayoutBindingInfo {
                binding: 1,
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            },
            crate::DescriptorSetLayoutBindingInfo {
                binding: 2,
                ty: vk::DescriptorType::STORAGE_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
            },
            crate::DescriptorSetLayoutBindingInfo {
                binding: 3,
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX,
            },
            crate::DescriptorSetLayoutBindingInfo {
                binding: 4,
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
            },
        ];

        let per_frame_descriptor_set_layout_desc = crate::DescriptorSetLayoutDescription {
            bindings: descriptor_set_layout_bindings.into(),
        };
        let per_frame_descriptor_set_layout = renderer
            .descriptor_set_layouts_mut()
            .access_or_create(per_frame_descriptor_set_layout_desc)?;

        let pipeline_layout_desc = crate::PipelineLayoutDescription {
            descriptor_set_layouts: Box::new([
                per_frame_descriptor_set_layout,
                other_descriptor_set_layout_handle,
            ]),
            bind_point: vk::PipelineBindPoint::GRAPHICS,
        };

        let pipeline_layout = renderer.access_or_create_pipeline_layout(pipeline_layout_desc)?;

        // TODO: it seems like this could be generated by build.rs or a macro?
        const VERTEX_ATTRIBUTE_DESCRIPTIONS: &[vk::VertexInputAttributeDescription] = &[
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::offset_of!(crate::ShaderVertVertex, position) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: std::mem::offset_of!(crate::ShaderVertVertex, tex_coord) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::offset_of!(crate::ShaderVertVertex, normal) as u32,
            },
        ];
        let vertex_input_bindings = &[vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<crate::ShaderVertVertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let vert_module_desc = crate::ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::VERTEX,
            spv: COMPILED_MAIN_VERT_SHADER,
            entry_point_name: crate::ENTRY_POINT_NAME_SHADER_VERT,
            vertex_attribute_descriptions: VERTEX_ATTRIBUTE_DESCRIPTIONS,
            vertex_input_bindings,
        };
        let vert_module = renderer
            .shader_modules_mut()
            .access_or_create(vert_module_desc)?;

        let frag_module_desc = crate::ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::FRAGMENT,
            spv: COMPILED_MAIN_FRAG_SHADER,
            entry_point_name: crate::ENTRY_POINT_NAME_SHADER_FRAG,
            vertex_attribute_descriptions: &[],
            vertex_input_bindings: &[],
        };
        let frag_module = renderer
            .shader_modules_mut()
            .access_or_create(frag_module_desc)?;

        let _other_descriptor_set_layout_handle = other_descriptor_set_layout_handle;

        Ok(MainResources {
            id: self.id,
            resources,
            _other_descriptor_set_layout_handle,
            other_descriptor_set,
            images: self.images,
            material_ubos_range: self.material_ubos_range,
            global_light_ubo_range: self.global_light_ubo_range,
            per_frame_descriptor_set_layout,
            pipeline_layout,
            vert_module,
            frag_module,
        })
    }
}

pub struct MainResources {
    id: u32,
    resources: Arc<Resources>,
    _other_descriptor_set_layout_handle: DescriptorSetLayoutResourceHandle,
    other_descriptor_set: vk::DescriptorSet,
    images: Box<[(ImageResourceHandle, vk::Sampler)]>,
    material_ubos_range: vulkan::AllocationRange,
    global_light_ubo_range: vulkan::AllocationRange,
    per_frame_descriptor_set_layout: crate::DescriptorSetLayoutResourceHandle,
    pipeline_layout: crate::PipelineLayoutResourceHandle,
    vert_module: crate::ShaderModuleResourceHandle,
    frag_module: crate::ShaderModuleResourceHandle,
}

impl MainResources {
    #[inline]
    pub fn images(&self) -> &[(ImageResourceHandle, vk::Sampler)] {
        &self.images
    }
    #[inline]
    pub fn global_light_ubo_range(&self) -> &vulkan::AllocationRange {
        &self.global_light_ubo_range
    }
    #[inline]
    pub fn material_ubos_range(&self) -> &vulkan::AllocationRange {
        &self.material_ubos_range
    }
    #[inline]
    pub fn resources(&self) -> &Resources {
        &self.resources
    }
    #[inline]
    pub fn get_image(
        &self,
        image_handle: ImageResourceHandle,
    ) -> Option<&(ImageResourceHandle, vk::Sampler)> {
        debug_assert!(self.id == image_handle.id());
        self.images.get(image_handle.index() as usize)
    }
    #[inline]
    pub fn bind<Target, Storage, Handle>(
        &self,
        cmd: vk::CommandBuffer,
        renderer: &mut Renderer,
        target: &Target,
        storage: &Storage,
    ) -> Result<()>
    where
        Handle: Copy + Clone,
        Target: RenderTarget<ImageHandle = Handle>,
        Storage: RenderStorage<ImageHandle = Handle>,
    {
        let (pipeline, layout) = {
            let layout = renderer
                .pipeline_layouts
                .get(self.pipeline_layout)
                .ok_or(crate::Error::ResourceMissing)?
                .raw;

            let mut color_formats: Vec<vk::Format> = Vec::with_capacity(8);
            for target_image in target.color_images().iter() {
                let img = storage
                    .get_image(target_image.handle)
                    .ok_or(crate::Error::ResourceMissing)?;
                color_formats.push(img.format);
            }

            let depth_format = if let Some(img_handle) = target.depth_image() {
                let img = storage
                    .get_image(img_handle)
                    .ok_or(crate::Error::ResourceMissing)?;
                Some(img.format)
            } else {
                None
            };

            let pipeline_desc = crate::PipelineDescription::DynamicGraphics {
                pipeline_layout: self.pipeline_layout,
                vert_shader: self.vert_module,
                frag_shader: self.frag_module,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                color_formats: color_formats.into_boxed_slice(),
                depth_format,
                stencil_format: None,
                samples: renderer.samples(),
            };
            let pipeline_handle = renderer.pipelines.access_or_create(
                pipeline_desc,
                &mut renderer.pipeline_layouts,
                &mut renderer.shader_modules,
            )?;
            let pipeline = renderer
                .pipelines
                .get(pipeline_handle)
                .ok_or(Error::ResourceMissing)?;

            (*pipeline, layout)
        };
        let sets = [self.other_descriptor_set];
        unsafe {
            renderer
                .device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);

            renderer.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                1,
                &sets,
                &[],
            );
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub struct MainTechnique {
    device: vulkan::SharedDeviceRef,
    main_resource_id: u32,
    descriptor_pool: vk::DescriptorPool,
    per_frame_descriptor_sets: [vk::DescriptorSet; crate::MAX_FRAME_COUNT as usize],
}

impl Drop for MainTechnique {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

// TODO: generate impmlementation or a macro that implements ShaderBinding<T> with build.rs.
impl ShaderBinding<InstanceBuffer> for MainTechnique {
    const BINDING: u32 = 0;
    const SET: u32 = 0;
}
impl ShaderBinding<CameraUBO> for MainTechnique {
    const BINDING: u32 = 1;
    const SET: u32 = 0;
}
impl ShaderBinding<PointLightsUBO> for MainTechnique {
    const BINDING: u32 = 2;
    const SET: u32 = 0;
}
impl ShaderBinding<DirectionalLightUBO> for MainTechnique {
    const BINDING: u32 = 3;
    const SET: u32 = 0;
}
pub struct DepthImage;
impl ShaderBinding<DepthImage> for MainTechnique {
    const BINDING: u32 = 4;
    const SET: u32 = 0;
}

impl MainTechnique {
    pub fn new(renderer: &mut crate::Renderer, resources: &MainResources) -> crate::Result<Self> {
        let device = renderer.device.clone();

        // per frame only
        let descriptor_pool = {
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: MAX_FRAME_COUNT as u32 * 3,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: MAX_FRAME_COUNT as u32 * 2,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: MAX_FRAME_COUNT as u32,
                },
            ];
            let create_info = vk::DescriptorPoolCreateInfo {
                max_sets: pool_sizes.iter().map(|s| s.descriptor_count).sum(),
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_pool(&create_info) }?
        };

        let per_frame_descriptor_sets: [vk::DescriptorSet; crate::MAX_FRAME_COUNT as usize] = {
            let per_frame_set_layout = *renderer
                .descriptor_set_layouts_mut()
                .get(resources.per_frame_descriptor_set_layout)
                .unwrap();
            let set_layouts = [per_frame_set_layout; crate::MAX_FRAME_COUNT as usize];
            let alloc_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool,
                descriptor_set_count: set_layouts.len() as u32,
                p_set_layouts: set_layouts.as_ptr(),
                ..Default::default()
            };
            let sets = unsafe { device.allocate_descriptor_sets(&alloc_info) }?;

            sets.try_into()
                .expect("Incorrect number of descriptor sets")
        };

        Ok(Self {
            device,
            main_resource_id: resources.id,
            descriptor_pool,
            per_frame_descriptor_sets,
        })
    }
    pub fn update_context<T>(
        &mut self,
        ctx: &mut FrameContext,
        ctx_state: &T,
        renderer: &Renderer,
    ) -> crate::Result<()>
    where
        Self: ShaderBinding<CameraUBO>
            + ShaderBinding<InstanceBuffer>
            + ShaderBinding<PointLightsUBO>
            + ShaderBinding<DirectionalLightUBO>
            + ShaderBinding<DepthImage>,
        T: HasBindingValue<CameraUBO, Value = FrameContextRange<Uniform>>
            + HasBindingValue<InstanceBuffer, Value = FrameContextRange<Storage>>
            + HasBindingValue<PointLightsUBO, Value = FrameContextRange<Storage>>
            + HasBindingValue<DirectionalLightUBO, Value = FrameContextRange<Uniform>>
            + HasBindingValue<DepthImage, Value = FrameContextImageHandle>,
    {
        let camera_data_range = <T as HasBindingValue<CameraUBO>>::get(ctx_state);
        let camera_infos = ctx.resolve(camera_data_range, renderer)?;

        let instance_data_range = <T as HasBindingValue<InstanceBuffer>>::get(ctx_state);
        let instance_infos = ctx.resolve(instance_data_range, renderer)?;

        let point_lights_data_range = <T as HasBindingValue<PointLightsUBO>>::get(ctx_state);
        let point_light_infos = ctx.resolve(point_lights_data_range, renderer)?;

        let directional_light_data_range =
            <T as HasBindingValue<DirectionalLightUBO>>::get(ctx_state);
        let directional_light_infos = ctx.resolve(directional_light_data_range, renderer)?;

        let depth_image_handle = <T as HasBindingValue<DepthImage>>::get(&ctx_state);
        let depth_image_infos = ctx.resolve(depth_image_handle, renderer)?;

        let writes: Box<[vk::WriteDescriptorSet]> = (0..crate::MAX_FRAME_COUNT as usize)
            .flat_map(|i| {
                [
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<InstanceBuffer>>::BINDING,
                        descriptor_count: 1,
                        p_buffer_info: &instance_infos[i],
                        descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                        ..Default::default()
                    },
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<CameraUBO>>::BINDING,
                        descriptor_count: 1,
                        p_buffer_info: &camera_infos[i],
                        descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                        ..Default::default()
                    },
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<PointLightsUBO>>::BINDING,
                        descriptor_count: 1,
                        p_buffer_info: &point_light_infos[i],
                        descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                        ..Default::default()
                    },
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<DirectionalLightUBO>>::BINDING,
                        descriptor_count: 1,
                        p_buffer_info: &directional_light_infos[i],
                        descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                        ..Default::default()
                    },
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<DepthImage>>::BINDING,
                        descriptor_count: 1,
                        p_image_info: &depth_image_infos[i],
                        descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        ..Default::default()
                    },
                ]
                .into_iter()
            })
            .collect();

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };

        Ok(())
    }
}

impl RenderTechnique for MainTechnique {
    type TechniqueResources = MainResources;
    fn bind(
        &self,
        cmd: vk::CommandBuffer,
        ctx: &FrameContext,
        resources: &Self::TechniqueResources,
        renderer: &Renderer,
    ) -> crate::Result<()> {
        debug_assert!(self.main_resource_id == resources.id);

        let current_frame_index = ctx.frame_index;

        let layout = renderer
            .pipeline_layouts
            .get(resources.pipeline_layout)
            .ok_or(crate::Error::ResourceMissing)?
            .raw;

        unsafe {
            // bind per frame ds
            let sets = &[self.per_frame_descriptor_sets[current_frame_index]];
            let dynamic_offsets = &[];
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                sets,
                dynamic_offsets,
            );
        }

        Ok(())
    }
}

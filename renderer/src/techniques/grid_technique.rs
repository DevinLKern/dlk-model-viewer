use std::sync::{Arc, atomic::AtomicU32};

use crate::{
    CameraUBO, CanResolveBindingValue, DescriptorSetLayoutBindingInfo,
    DescriptorSetLayoutDescription, DescriptorSetLayoutResourceHandle, ENTRY_POINT_NAME_GRID_FRAG,
    ENTRY_POINT_NAME_GRID_VERT, Error, FrameContext, FrameContextRange, GridInstanceBuffer,
    GridMaterialUBO, GridVertVertex, HasBindingValue, MAX_FRAME_COUNT, MaterialHandle,
    PipelineLayoutDescription, PipelineLayoutResourceHandle, RenderStorage, RenderTarget,
    RenderTechnique, Renderer, ResourceRegistry, ResourceUploader, Resources, Result,
    ShaderBinding, ShaderModuleDescription, ShaderModuleResourceHandle, Storage, Uniform,
};

use ash::vk;
use vulkan::SharedDeviceRef;

const COMPILED_GRID_VERT_SHADER: &[u8] = include_bytes!("../../shaders/grid.vert.spv");
const COMPILED_GRID_FRAG_SHADER: &[u8] = include_bytes!("../../shaders/grid.frag.spv");

pub struct GridMaterialData {
    pub base_color: math::Vec4<f32>,
    pub line_color: math::Vec4<f32>,
    pub line_width: math::Vec2<f32>,
    pub scale: math::Vec2<f32>,
}

pub struct GridResourcesRegistry {
    id: u32,
    materials: Vec<GridMaterialUBO>,
}

static NEXT_GRID_RESOURCES_ID: AtomicU32 = AtomicU32::new(0);
impl GridResourcesRegistry {
    #[inline]
    fn get_id() -> u32 {
        NEXT_GRID_RESOURCES_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
    #[inline]
    pub fn new() -> Self {
        Self {
            id: Self::get_id(),
            materials: Vec::new(),
        }
    }
    #[inline]
    pub fn add_material(
        &mut self,
        material_data: GridMaterialData,
    ) -> MaterialHandle<GridMaterialUBO> {
        let index = self.materials.len() as u32;

        self.materials.push(GridMaterialUBO {
            base_color: material_data.base_color.into_arr(),
            line_color: material_data.line_color.into_arr(),
            line_width: material_data.line_width.into_arr(),
            scale: material_data.scale.into_arr(),
        });

        MaterialHandle::new(self.id, index)
    }
    #[inline]
    pub fn register(self, registry: &mut ResourceRegistry) -> GridResourcesUploader {
        registry.add_storage_allocation(
            (self.materials.len() * std::mem::size_of::<GridMaterialUBO>()) as u64,
            std::mem::size_of::<GridMaterialUBO>() as u64,
        );

        registry.add_descriptors(vk::DescriptorType::STORAGE_BUFFER, 1);
        registry.add_sets(1);

        GridResourcesUploader {
            id: self.id,
            scene_id: registry.id(),
            materials: self.materials.into_boxed_slice(),
        }
    }
}

pub struct GridResourcesUploader {
    id: u32,
    scene_id: u32,
    materials: Box<[GridMaterialUBO]>,
}

impl GridResourcesUploader {
    pub fn upload(self, uploader: &mut ResourceUploader) -> Result<GridResourcesPromise> {
        debug_assert!(self.scene_id == uploader.id());

        let materials_allocation_size =
            (std::mem::size_of::<GridMaterialUBO>() * self.materials.len()) as u64;

        // TODO: Add error type

        let _ = uploader
            .storage_allocator_mut()
            .can_reserve(
                materials_allocation_size,
                std::mem::size_of::<GridMaterialUBO>() as u64,
            )
            .ok_or(Error::BufferCapacityExceeded)?;

        let material_ubos_range = uploader
            .storage_allocator_mut()
            .reserve_data(
                materials_allocation_size,
                std::mem::size_of::<GridMaterialUBO>() as u64,
            )
            .ok_or(Error::BufferCapacityExceeded)?;

        unsafe {
            uploader
                .storage_allocator_mut()
                .upload_data(material_ubos_range, &self.materials)?;
        }

        Ok(GridResourcesPromise {
            id: self.id,
            scene_id: self.scene_id,
            material_ubos_range,
        })
    }
}

pub struct GridResourcesPromise {
    id: u32,
    scene_id: u32,
    material_ubos_range: vulkan::AllocationRange,
}

impl GridResourcesPromise {
    pub fn finalize(
        self,
        resources: Arc<Resources>,
        renderer: &mut Renderer,
    ) -> Result<GridResources> {
        debug_assert!(self.scene_id == resources.id());

        let other_descriptor_set_layout_handle = {
            let bindings: &[crate::DescriptorSetLayoutBindingInfo] =
                &[crate::DescriptorSetLayoutBindingInfo {
                    binding: 0,
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                }];
            let description = crate::DescriptorSetLayoutDescription {
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
            let material_buffer_info = [vk::DescriptorBufferInfo {
                buffer: resources.storage_buffer().handle,
                offset: self.material_ubos_range.offset,
                range: self.material_ubos_range.size,
            }];
            let writes = [vk::WriteDescriptorSet {
                dst_set: other_descriptor_set,
                dst_binding: 0,
                dst_array_element: 0,
                descriptor_count: material_buffer_info.len() as u32,
                descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                p_buffer_info: material_buffer_info.as_ptr(),
                ..Default::default()
            }];
            unsafe { renderer.device.update_descriptor_sets(&writes, &[]) };
        }

        let descriptor_set_layout_bindings: &[DescriptorSetLayoutBindingInfo] = &[
            DescriptorSetLayoutBindingInfo {
                binding: 0,
                ty: vk::DescriptorType::STORAGE_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX,
            },
            DescriptorSetLayoutBindingInfo {
                binding: 1,
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                count: 1,
                stage_flags: vk::ShaderStageFlags::VERTEX,
            },
        ];

        let per_frame_descriptor_set_layout_desc = DescriptorSetLayoutDescription {
            bindings: descriptor_set_layout_bindings.into(),
        };
        let per_frame_descriptor_set_layout_handle = renderer
            .descriptor_set_layouts_mut()
            .access_or_create(per_frame_descriptor_set_layout_desc)?;

        let pipeline_layout_desc = PipelineLayoutDescription {
            descriptor_set_layouts: Box::new([
                per_frame_descriptor_set_layout_handle,
                other_descriptor_set_layout_handle,
            ]),
            bind_point: vk::PipelineBindPoint::GRAPHICS,
        };

        let pipeline_layout_handle =
            renderer.access_or_create_pipeline_layout(pipeline_layout_desc)?;

        const VERTEX_ATTRIBUTE_DESCRIPTIONS: &[vk::VertexInputAttributeDescription] =
            &[vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::offset_of!(GridVertVertex, position) as u32,
            }];
        let vertex_input_bindings = &[vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<GridVertVertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let vert_module_desc = ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::VERTEX,
            spv: COMPILED_GRID_VERT_SHADER,
            entry_point_name: ENTRY_POINT_NAME_GRID_VERT,
            vertex_attribute_descriptions: VERTEX_ATTRIBUTE_DESCRIPTIONS,
            vertex_input_bindings,
        };
        let vert_module = renderer
            .shader_modules_mut()
            .access_or_create(vert_module_desc)?;

        let frag_module_desc = ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::FRAGMENT,
            spv: COMPILED_GRID_FRAG_SHADER,
            entry_point_name: ENTRY_POINT_NAME_GRID_FRAG,
            vertex_attribute_descriptions: &[],
            vertex_input_bindings: &[],
        };
        let frag_module = renderer
            .shader_modules_mut()
            .access_or_create(frag_module_desc)?;

        Ok(GridResources {
            id: self.id,
            resources,
            material_ubos_range: self.material_ubos_range,
            per_frame_descriptor_set_layout_handle,
            other_descriptor_set_layout_handle,
            other_descriptor_set,
            pipeline_layout_handle,
            vert_module,
            frag_module,
        })
    }
}

#[allow(dead_code)]
pub struct GridResources {
    id: u32,
    resources: Arc<Resources>,
    material_ubos_range: vulkan::AllocationRange,
    per_frame_descriptor_set_layout_handle: DescriptorSetLayoutResourceHandle,
    other_descriptor_set_layout_handle: DescriptorSetLayoutResourceHandle,
    other_descriptor_set: vk::DescriptorSet,
    pipeline_layout_handle: PipelineLayoutResourceHandle,
    vert_module: ShaderModuleResourceHandle,
    frag_module: ShaderModuleResourceHandle,
}

impl GridResources {
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
                .get(self.pipeline_layout_handle)
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
                pipeline_layout: self.pipeline_layout_handle,
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
pub struct GridTechnique {
    device: SharedDeviceRef,
    grid_resources_id: u32,
    descriptor_pool: vk::DescriptorPool,
    per_frame_descriptor_sets: [vk::DescriptorSet; MAX_FRAME_COUNT as usize],
}

impl Drop for GridTechnique {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

impl ShaderBinding<GridInstanceBuffer> for GridTechnique {
    const BINDING: u32 = 0;
    const SET: u32 = 0;
}
impl ShaderBinding<CameraUBO> for GridTechnique {
    const BINDING: u32 = 1;
    const SET: u32 = 0;
}

#[allow(dead_code)]
impl GridTechnique {
    pub fn new(renderer: &mut Renderer, resources: &GridResources) -> Result<Self> {
        let device = renderer.device.clone();

        let descriptor_pool = {
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: MAX_FRAME_COUNT as u32,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: MAX_FRAME_COUNT as u32,
                },
            ];
            let create_info = vk::DescriptorPoolCreateInfo {
                max_sets: MAX_FRAME_COUNT as u32,
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_pool(&create_info) }?
        };

        let per_frame_descriptor_sets: [vk::DescriptorSet; MAX_FRAME_COUNT as usize] = {
            let per_frame_set_layout = *renderer
                .descriptor_set_layouts_mut()
                .get(resources.per_frame_descriptor_set_layout_handle)
                .unwrap();
            let set_layouts = [per_frame_set_layout; MAX_FRAME_COUNT as usize];
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
            grid_resources_id: resources.id,
            descriptor_pool,
            per_frame_descriptor_sets,
        })
    }
    pub fn update_context<T>(
        &mut self,
        ctx: &mut FrameContext,
        ctx_state: &T,
        renderer: &crate::Renderer,
    ) -> crate::Result<()>
    where
        Self: ShaderBinding<CameraUBO> + ShaderBinding<GridInstanceBuffer>,
        T: HasBindingValue<CameraUBO, Value = FrameContextRange<Uniform>>
            + HasBindingValue<GridInstanceBuffer, Value = FrameContextRange<Storage>>,
    {
        let camera_data_range = <T as HasBindingValue<CameraUBO>>::get(&ctx_state);
        let camera_infos = ctx.resolve(camera_data_range, renderer)?;

        let instance_data_range = <T as HasBindingValue<GridInstanceBuffer>>::get(&ctx_state);
        let instance_infos = ctx.resolve(instance_data_range, renderer)?;

        let writes: Box<[vk::WriteDescriptorSet]> = (0..MAX_FRAME_COUNT as usize)
            .flat_map(|i| {
                [
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: <Self as ShaderBinding<GridInstanceBuffer>>::BINDING,
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
                ]
                .into_iter()
            })
            .collect();

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };

        Ok(())
    }
}

impl RenderTechnique for GridTechnique {
    type TechniqueResources = GridResources;
    fn bind(
        &self,
        cmd: vk::CommandBuffer,
        ctx: &FrameContext,
        resources: &Self::TechniqueResources,
        renderer: &Renderer,
    ) -> crate::Result<()> {
        debug_assert!(self.grid_resources_id == resources.id);

        let current_frame_index = ctx.frame_index;

        let layout = renderer
            .pipeline_layouts
            .get(resources.pipeline_layout_handle)
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

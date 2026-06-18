use ash::vk;
use math::Zero;
use vulkan::SharedDeviceRef;

use crate::{
    COMPILED_MAIN_FRAG_SHADER, COMPILED_MAIN_VERT_SHADER, CameraUBO,
    DescriptorSetLayoutBindingInfo, DescriptorSetLayoutDescription,
    DescriptorSetLayoutResourceHandle, ENTRY_POINT_NAME_SHADER_FRAG, ENTRY_POINT_NAME_SHADER_VERT,
    Error, FrameContext, GlobalLightUBO, InstanceData, MAX_FRAME_COUNT, MAX_INSTANCE_DATA_COUNT,
    MaterialUBO, MeshArena, MeshArenaHandle, PipelineDescription, PipelineLayoutDescription,
    PipelineLayoutResourceHandle, PipelineLayoutResourceManager, PipelineResourceManager, Renderer,
    Result, ShaderModuleDescription, ShaderModuleResourceHandle, ShaderModuleResourceManager,
    ShaderVertVertex,
};

slotmap::new_key_type! { pub struct InstanceDataHandle; }

#[allow(dead_code)]
pub struct MainSceneBuilder {
    pub(crate) vertices: Vec<ShaderVertVertex>,
    pub(crate) indices: Vec<u32>,
    // first_index, index_count
    pub(crate) images: Vec<(vk::Sampler, vulkan::Image)>,
    // base_color, texture_index, is_unlit
    pub(crate) materials: Vec<(math::Vec4<f32>, Option<usize>, bool)>,
    pub(crate) light_direction: math::Vec3<f32>,
    pub(crate) light_color: math::Vec4<f32>,
    pub(crate) ambient_light_intensity: f32,
}

#[allow(dead_code)]
impl MainSceneBuilder {
    pub fn new() -> Self {
        const INITIAL_CAPCITY: usize = 32;
        Self {
            vertices: Vec::with_capacity(INITIAL_CAPCITY),
            indices: Vec::with_capacity(INITIAL_CAPCITY),
            materials: Vec::with_capacity(INITIAL_CAPCITY),
            images: Vec::with_capacity(INITIAL_CAPCITY),
            light_direction: math::Vec3::ZERO,
            light_color: math::Vec4::ZERO,
            ambient_light_intensity: 0.1,
        }
    }
    #[inline]
    pub fn set_light_direction(&mut self, dir: math::Vec3<f32>) {
        self.light_direction = dir;
    }
    #[inline]
    pub fn set_light_color(&mut self, color: math::Vec4<f32>) {
        self.light_color = color;
    }
    #[inline]
    pub fn set_ambient_light_intensity(&mut self, ambient: f32) {
        self.ambient_light_intensity = ambient;
    }
    #[inline]
    pub fn add_vertices(
        &mut self,
        verts: impl Iterator<Item = ShaderVertVertex>,
    ) -> (usize, usize) {
        let before = self.vertices.len();
        self.vertices.extend(verts);
        let after = self.vertices.len();
        return (before, after - before);
    }
    #[inline]
    pub fn add_indices(&mut self, indices: impl Iterator<Item = u32>) -> (usize, usize) {
        let before = self.indices.len();
        self.indices.extend(indices);
        let after = self.indices.len();
        return (before, after - before);
    }
    #[inline]
    pub fn add_image(&mut self, sampler: vk::Sampler, image: vulkan::Image) -> usize {
        let res = self.images.len();
        self.images.push((sampler, image));
        return res;
    }
    #[inline]
    pub fn add_material(
        &mut self,
        base_color: math::Vec4<f32>,
        texture: Option<usize>,
        unlit: bool,
    ) -> usize {
        let res = self.materials.len();
        self.materials.push((base_color, texture, unlit));
        return res;
    }
    #[allow(unused)]
    pub fn build(
        self,
        device: SharedDeviceRef,
        mesh_arenas: &mut slotmap::DenseSlotMap<MeshArenaHandle, MeshArena>,
    ) -> Result<MainScene> {
        let mesh_arena = {
            let vertex_buffer = {
                let create_info = vulkan::BufferCreateInfo {
                    size: (self.vertices.len() * std::mem::size_of::<ShaderVertVertex>()) as u64,
                    usage: vk::BufferUsageFlags::VERTEX_BUFFER,
                    memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                        | vk::MemoryPropertyFlags::HOST_COHERENT,
                };

                vulkan::Buffer::new(device.clone(), &create_info)?
            };
            let index_buffer = {
                let create_info = vulkan::BufferCreateInfo {
                    size: (self.indices.len() * std::mem::size_of::<u32>()) as u64,
                    usage: vk::BufferUsageFlags::INDEX_BUFFER,
                    memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                        | vk::MemoryPropertyFlags::HOST_COHERENT,
                };

                vulkan::Buffer::new(device.clone(), &create_info)?
            };

            unsafe {
                let dst = vertex_buffer.map_memory(0, vertex_buffer.size)?;
                let dst = dst as *mut ShaderVertVertex;
                dst.copy_from(self.vertices.as_ptr(), self.vertices.len());
                vertex_buffer.unmap();

                let dst = index_buffer.map_memory(0, index_buffer.size)?;
                let dst = dst as *mut u32;
                dst.copy_from(self.indices.as_ptr(), self.indices.len());
                index_buffer.unmap();
            }

            MeshArena {
                vertex_buffer,
                index_buffer,
            }
        };
        let mesh_arena_handle = mesh_arenas.insert(mesh_arena);

        let mut uniform_buffer_offset = 0;
        let uniform_buffer = {
            let size = std::mem::size_of::<GlobalLightUBO>() as u64;
            let create_info = vulkan::BufferCreateInfo {
                size,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(device.clone(), &create_info)?
        };

        unsafe {
            let dst = uniform_buffer.map_memory(
                uniform_buffer_offset,
                std::mem::size_of::<GlobalLightUBO>() as u64,
            )?;
            let dst = dst as *mut GlobalLightUBO;
            *dst = GlobalLightUBO {
                direction: self.light_direction.as_vec4(0.0).as_arr(),
                color: self.light_color.into_arr(),
                ambient: self.ambient_light_intensity,
            };
            uniform_buffer.unmap();
        }
        let global_light_offset = uniform_buffer_offset;
        uniform_buffer_offset += std::mem::size_of::<GlobalLightUBO>() as u64;
        let global_light_range = (global_light_offset, uniform_buffer_offset);

        let storage_buffer_offset = 0;
        let storage_buffer = {
            let size = (self.materials.len() * std::mem::size_of::<MaterialUBO>()) as u64;
            let create_info = vulkan::BufferCreateInfo {
                size,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(device, &create_info)?
        };

        let material_data: Box<[MaterialUBO]> = self
            .materials
            .into_iter()
            .map(|(base_color, texture_index, unlit)| {
                const MATERIAL_FLAG_TEXTURED_BIT: u32 = (1 << 0);
                const MATERIAL_FLAG_UNLIT_BIT: u32 = (1 << 1);
                let mut flags: u32 = 0;
                if texture_index.is_some() {
                    flags |= MATERIAL_FLAG_TEXTURED_BIT;
                }
                if unlit {
                    flags |= MATERIAL_FLAG_UNLIT_BIT;
                }
                MaterialUBO {
                    flags,
                    texture_index: texture_index.unwrap_or(0) as u32,
                    _pad2: [0; 8],
                    base_color: base_color.as_arr(),
                }
            })
            .collect();

        let mut storage_buffer_offset = 0;
        let size = (material_data.len() * std::mem::size_of::<MaterialUBO>()) as u64;
        unsafe {
            let dst = storage_buffer.map_memory(storage_buffer_offset, size)?;
            let dst = dst as *mut MaterialUBO;
            dst.copy_from(material_data.as_ptr(), material_data.len());
            storage_buffer.unmap();
        }
        storage_buffer_offset += size;

        Ok(MainScene {
            mesh_arena_handle,
            images: self.images,
            global_light_range,
            uniform_buffer,
            storage_buffer,
            submeshes: Vec::new(),
            instances: Vec::new(),
            draws: Vec::new(),
        })
    }
}

pub struct MainScene {
    pub(crate) mesh_arena_handle: MeshArenaHandle,
    pub(crate) images: Vec<(vk::Sampler, vulkan::Image)>,
    pub(crate) global_light_range: (u64, u64),
    pub(crate) uniform_buffer: vulkan::Buffer,
    pub(crate) storage_buffer: vulkan::Buffer,
    // (first_index, index_count)
    pub(crate) submeshes: Vec<(usize, usize)>,
    // (model_transform, material_index)
    pub(crate) instances: Vec<(math::Mat4<f32>, usize)>,
    // (instance_index, submesh_index)
    pub(crate) draws: Vec<(usize, usize)>,
}

impl MainScene {
    #[inline]
    pub fn add_instance(&mut self, transform: math::Mat4<f32>, material_index: usize) -> usize {
        let res = self.instances.len();
        self.instances.push((transform, material_index));
        return res;
    }
    #[inline]
    pub fn add_draw(&mut self, instance_index: usize, submesh_index: usize) {
        self.draws.push((instance_index, submesh_index));
    }
    #[inline]
    pub fn add_submesh(&mut self, first_index: usize, index_count: usize) -> usize {
        let res = self.submeshes.len();
        self.submeshes.push((first_index, index_count));
        return res;
    }
    #[inline]
    pub fn reset_draws(&mut self) {
        self.draws.clear();
    }
    #[inline]
    pub fn reset(&mut self) {
        self.instances.clear();
        self.draws.clear();
        self.submeshes.clear();
    }
}

#[allow(dead_code)]
pub struct MainRenderPass {
    device: SharedDeviceRef,
    per_frame_descriptor_set_layout: DescriptorSetLayoutResourceHandle,
    other_descriptor_set_layout: DescriptorSetLayoutResourceHandle,
    descriptor_pool: vk::DescriptorPool,
    per_frame_descriptor_sets: [vk::DescriptorSet; MAX_FRAME_COUNT as usize],
    other_descriptor_set: vk::DescriptorSet,
    pipeline_layout: PipelineLayoutResourceHandle,
    vert_module: ShaderModuleResourceHandle,
    frag_module: ShaderModuleResourceHandle,
}

impl Drop for MainRenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

#[allow(dead_code)]
impl MainRenderPass {
    pub fn new(
        device: SharedDeviceRef,
        scene: &MainScene,
        renderer: &mut Renderer,
    ) -> Result<Self> {
        let descriptor_set_layout_bindings: &[&[DescriptorSetLayoutBindingInfo]] = &[
            // SET 0 - per frame
            &[
                DescriptorSetLayoutBindingInfo {
                    binding: 0,
                    ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::VERTEX,
                },
                DescriptorSetLayoutBindingInfo {
                    binding: 1,
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                },
            ],
            // SET 1 - other
            &[
                // global light
                DescriptorSetLayoutBindingInfo {
                    binding: 0,
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
                // global_textures
                DescriptorSetLayoutBindingInfo {
                    binding: 1,
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    count: scene.images.len() as u32,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
                // materials
                DescriptorSetLayoutBindingInfo {
                    binding: 2,
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    count: 1,
                    stage_flags: vk::ShaderStageFlags::FRAGMENT,
                },
            ],
        ];

        let per_frame_descriptor_set_layout_desc = DescriptorSetLayoutDescription {
            bindings: descriptor_set_layout_bindings[0].into(),
        };
        let per_frame_descriptor_set_layout = renderer
            .descriptor_set_layouts_mut()
            .access_or_create(per_frame_descriptor_set_layout_desc)?;
        let other_descriptor_set_layout_desc = DescriptorSetLayoutDescription {
            bindings: descriptor_set_layout_bindings[1].into(),
        };
        let other_descriptor_set_layout = renderer
            .descriptor_set_layouts_mut()
            .access_or_create(other_descriptor_set_layout_desc)?;

        let pipeline_layout_desc = PipelineLayoutDescription {
            descriptor_set_layouts: Box::new([
                per_frame_descriptor_set_layout,
                other_descriptor_set_layout,
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
            stride: std::mem::size_of::<ShaderVertVertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let vert_module_desc = ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::VERTEX,
            spv: COMPILED_MAIN_VERT_SHADER,
            entry_point_name: ENTRY_POINT_NAME_SHADER_VERT,
            vertex_attribute_descriptions: VERTEX_ATTRIBUTE_DESCRIPTIONS,
            vertex_input_bindings,
        };
        let vert_module = renderer
            .shader_modules_mut()
            .access_or_create(vert_module_desc)?;

        let frag_module_desc = ShaderModuleDescription::Internal {
            stage: vk::ShaderStageFlags::FRAGMENT,
            spv: COMPILED_MAIN_FRAG_SHADER,
            entry_point_name: ENTRY_POINT_NAME_SHADER_FRAG,
            vertex_attribute_descriptions: &[],
            vertex_input_bindings: &[],
        };
        let frag_module = renderer
            .shader_modules_mut()
            .access_or_create(frag_module_desc)?;

        let descriptor_pool = {
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                    descriptor_count: MAX_FRAME_COUNT as u32,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_BUFFER,
                    descriptor_count: MAX_FRAME_COUNT as u32 + 1,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: scene.images.len() as u32,
                },
            ];
            let create_info = vk::DescriptorPoolCreateInfo {
                max_sets: MAX_FRAME_COUNT as u32 + 3,
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
                ..Default::default()
            };

            unsafe { device.create_descriptor_pool(&create_info) }?
        };

        let per_frame_descriptor_sets: [vk::DescriptorSet; MAX_FRAME_COUNT as usize] = {
            let per_frame_set_layout = *renderer
                .descriptor_set_layouts_mut()
                .get(per_frame_descriptor_set_layout)
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

        let other_descriptor_set = {
            let other_set_layout = *renderer
                .descriptor_set_layouts_mut()
                .get(other_descriptor_set_layout)
                .unwrap();
            let set_layouts = [other_set_layout];
            let alloc_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool,
                descriptor_set_count: set_layouts.len() as u32,
                p_set_layouts: set_layouts.as_ptr(),
                ..Default::default()
            };
            let sets = unsafe { device.allocate_descriptor_sets(&alloc_info) }?;
            sets[0]
        };

        {
            let image_info: Box<[vk::DescriptorImageInfo]> = scene
                .images
                .iter()
                .map(|(sampler, img)| vk::DescriptorImageInfo {
                    sampler: *sampler,
                    image_view: img.view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                })
                .collect();

            let (global_light_offset, global_light_size) = scene.global_light_range;
            let global_light_buffer_info = [vk::DescriptorBufferInfo {
                buffer: scene.uniform_buffer.handle,
                offset: global_light_offset,
                range: global_light_size,
            }];
            let material_buffer_info = [vk::DescriptorBufferInfo {
                buffer: scene.storage_buffer.handle,
                offset: 0,
                range: vk::WHOLE_SIZE,
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
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        Ok(Self {
            device,
            per_frame_descriptor_set_layout,
            other_descriptor_set_layout,
            descriptor_pool,
            per_frame_descriptor_sets,
            other_descriptor_set,
            pipeline_layout,
            vert_module,
            frag_module,
        })
    }
    pub fn update_context(&self, ctx: &FrameContext) {
        const CAMERA_SIZE: u64 = std::mem::size_of::<CameraUBO>() as u64;
        const INSTANCE_SIZE: u64 = std::mem::size_of::<InstanceData>() as u64;

        let camera_infos: Box<[vk::DescriptorBufferInfo]> = (0..MAX_FRAME_COUNT as usize)
            .map(|i| vk::DescriptorBufferInfo {
                buffer: ctx.frames()[i].allocator().uniform_buffer_raw(),
                offset: 0,
                range: CAMERA_SIZE,
            })
            .collect();

        let instance_infos: Box<[vk::DescriptorBufferInfo]> = (0..MAX_FRAME_COUNT as usize)
            .map(|i| vk::DescriptorBufferInfo {
                buffer: ctx.frames()[i].allocator().storage_buffer_raw(),
                offset: 0,
                range: INSTANCE_SIZE * MAX_INSTANCE_DATA_COUNT,
            })
            .collect();

        let writes: Box<[vk::WriteDescriptorSet]> = (0..MAX_FRAME_COUNT as usize)
            .flat_map(|i| {
                [
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: 0,
                        descriptor_count: 1,
                        p_buffer_info: &camera_infos[i],
                        descriptor_type: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                        ..Default::default()
                    },
                    vk::WriteDescriptorSet {
                        dst_set: self.per_frame_descriptor_sets[i],
                        dst_binding: 1,
                        descriptor_count: 1,
                        p_buffer_info: &instance_infos[i],
                        descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
                        ..Default::default()
                    },
                ]
                .into_iter()
            })
            .collect();

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
    }
    pub fn render(
        &self,
        ctx: &mut FrameContext,
        pipelines: &mut PipelineResourceManager,
        pipeline_layouts: &mut PipelineLayoutResourceManager,
        shader_modules: &mut ShaderModuleResourceManager,
        mesh_arenas: &slotmap::DenseSlotMap<MeshArenaHandle, MeshArena>,
        scene: &MainScene,
        camera_data: CameraUBO,
    ) -> Result<()> {
        let (pipeline, layout) = {
            let layout = pipeline_layouts
                .get(self.pipeline_layout)
                .ok_or(Error::ResourceMissing)?
                .raw;
            let pipeline_desc = PipelineDescription::DynamicGraphics {
                pipeline_layout: self.pipeline_layout,
                vert_shader: self.vert_module,
                frag_shader: self.frag_module,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                color_format: ctx.get_color_format(),
                depth_format: ctx.depth_format(),
                samples: vk::SampleCountFlags::TYPE_1,
            };
            let pipeline_handle =
                pipelines.access_or_create(pipeline_desc, pipeline_layouts, shader_modules)?;
            let pipeline = *pipelines
                .get(pipeline_handle)
                .ok_or(Error::ResourceMissing)?;

            (pipeline, layout)
        };

        let current_frame_index = ctx.index;
        let frame = ctx.get_current_frame_mut();

        let camera_size_aligned = self.device.get_uniform_buffer_min_size::<CameraUBO>();
        let camera_offset = frame
            .allocator_mut()
            .upload_uniform_data(&[camera_data], camera_size_aligned)?
            as u32;

        let cmd = frame.command_buffer();

        let mut indirect_command_data = Vec::<vk::DrawIndexedIndirectCommand>::with_capacity(64);
        let mut instance_data = Vec::<InstanceData>::with_capacity(64);
        let stride = {
            let size = std::mem::size_of::<InstanceData>();
            let align = std::mem::align_of::<InstanceData>();

            size.next_multiple_of(align) as u64
        };
        let first_instance_offset = frame
            .allocator_mut()
            .storage_buffer_offset()
            .next_multiple_of(stride)
            / stride;
        for (instance_index, submesh_index) in scene.draws.iter() {
            let (first_index, index_count) = scene.submeshes.get(*submesh_index).unwrap();
            let (transform, material_index) = scene.instances.get(*instance_index).unwrap();

            indirect_command_data.push(vk::DrawIndexedIndirectCommand {
                index_count: *index_count as u32,
                instance_count: 1,
                first_index: *first_index as u32,
                vertex_offset: 0,
                first_instance: instance_data.len() as u32 + first_instance_offset as u32,
            });
            let model_matrix = transform;
            let normal_matrix = model_matrix
                .as_mat3()
                .transposed()
                .inverse()
                .unwrap()
                .into_mat4(1.0);

            instance_data.push(InstanceData {
                model_matrix: model_matrix.as_2d_arr(),
                normal_matrix: normal_matrix.as_2d_arr(),
                material_index: *material_index as u32,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            });
        }
        let _instance_offset = frame
            .allocator_mut()
            .upload_storage_data(&instance_data, std::mem::size_of::<InstanceData>() as u64)?;
        let indirect_offset = frame.allocator_mut().upload_indirect_data(
            &indirect_command_data,
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64,
        )?;

        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);

            // bind per frame ds
            let sets = &[self.per_frame_descriptor_sets[current_frame_index]];
            let dynamic_offsets = &[camera_offset];
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                sets,
                dynamic_offsets,
            );

            // bind other ds
            let sets = &[self.other_descriptor_set];
            let dynamic_offsets = &[];
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                1,
                sets,
                dynamic_offsets,
            );

            let mesh_arena = mesh_arenas.get(scene.mesh_arena_handle).unwrap();

            let (vb, ib) = (
                mesh_arena.vertex_buffer.handle,
                mesh_arena.index_buffer.handle,
            );
            self.device.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
            self.device
                .cmd_bind_index_buffer(cmd, ib, 0, vk::IndexType::UINT32);

            self.device.cmd_draw_indexed_indirect(
                cmd,
                frame.allocator_mut().indirect_buffer_raw(),
                indirect_offset,
                indirect_command_data.len() as u32,
                std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32,
            );
        };

        Ok(())
    }
}

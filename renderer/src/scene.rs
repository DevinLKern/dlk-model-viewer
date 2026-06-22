use crate::{GlobalLightUBO, GridData, MaterialUBO, Result, ShaderVertVertex};

use math::{Identity, Mat4, Zero};
use vulkan::SharedDeviceRef;

use ash::vk;

pub(crate) const MAX_SCENE_IMAGE_COUNT: u32 = 32;

#[allow(dead_code)]
pub struct MeshArena {
    pub vertex_buffer: vulkan::Buffer,
    pub index_buffer: vulkan::Buffer,
}
slotmap::new_key_type! { pub struct MeshArenaHandle; }

#[allow(dead_code)]
pub struct SubMesh {
    pub geometry: MeshArenaHandle,
    pub first_index: u32,
    pub index_count: u32,
}
slotmap::new_key_type! { pub struct SubMeshHandle; }

#[allow(dead_code)]
pub struct SceneBuilder {
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
impl SceneBuilder {
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
    ) -> Result<Scene> {
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

        let alignment = device.get_uniform_buffer_min_offset_alignment();

        let mut uniform_buffer_offset = 0;

        let global_light_offset = uniform_buffer_offset;
        uniform_buffer_offset += std::mem::size_of::<GlobalLightUBO>() as u64;
        uniform_buffer_offset = uniform_buffer_offset.next_multiple_of(alignment);
        let global_light_range = (
            global_light_offset,
            uniform_buffer_offset - global_light_offset,
        );

        let grid_data_offset = uniform_buffer_offset;
        uniform_buffer_offset += std::mem::size_of::<GridData>() as u64;
        uniform_buffer_offset = uniform_buffer_offset.next_multiple_of(alignment);
        let grid_data_range = (grid_data_offset, uniform_buffer_offset - grid_data_offset);

        let uniform_buffer = {
            let size = uniform_buffer_offset;
            let create_info = vulkan::BufferCreateInfo {
                size,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
            };

            vulkan::Buffer::new(device.clone(), &create_info)?
        };

        unsafe {
            let dst = uniform_buffer.map_memory(global_light_offset, global_light_range.1)?;
            let dst = dst as *mut GlobalLightUBO;
            *dst = GlobalLightUBO {
                direction: self.light_direction.as_vec4(0.0).as_arr(),
                color: self.light_color.into_arr(),
                ambient: self.ambient_light_intensity,
            };
            uniform_buffer.unmap();
        }

        unsafe {
            let dst = uniform_buffer.map_memory(grid_data_offset, grid_data_range.1)?;
            let dst = dst as *mut GridData;
            *dst = GridData {
                model_matrix: Mat4::IDENTITY.into_2d_arr(),
                scale: [1.0, 1.0],
                _pad2: [0; 8],
                base_color: [0.0; 4],
                line_color: [1.0; 4],
            };
            uniform_buffer.unmap();
        }

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

        Ok(Scene {
            mesh_arena_handle,
            images: self.images,
            global_light_range,
            grid_data_range,
            uniform_buffer,
            storage_buffer,
            submeshes: Vec::new(),
            instances: Vec::new(),
            draws: Vec::new(),
        })
    }
}

pub struct Scene {
    pub(crate) mesh_arena_handle: MeshArenaHandle,
    pub(crate) images: Vec<(vk::Sampler, vulkan::Image)>,
    pub(crate) global_light_range: (u64, u64),
    pub(crate) grid_data_range: (u64, u64),
    pub(crate) uniform_buffer: vulkan::Buffer,
    pub(crate) storage_buffer: vulkan::Buffer,
    // (first_index, index_count)
    pub(crate) submeshes: Vec<(usize, usize)>,
    // (model_transform, material_index)
    pub(crate) instances: Vec<(math::Mat4<f32>, usize)>,
    // (instance_index, submesh_index)
    pub(crate) draws: Vec<(usize, usize)>,
}

impl Scene {
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

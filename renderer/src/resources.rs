use vulkan::SharedDeviceRef;

use crate::{Error, Renderer, Result};

use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use ash::vk;

pub type Image = vulkan::Image;

#[derive(Copy, Clone, Debug, Default)]
pub struct ImageResourceHandle {
    id: u32,
    index: u32,
}

impl ImageResourceHandle {
    #[inline]
    pub(crate) fn new(id: u32, index: u32) -> Self {
        Self { id, index }
    }
    #[inline]
    pub(crate) fn id(&self) -> u32 {
        self.id
    }
    #[inline]
    pub(crate) fn index(&self) -> u32 {
        self.index
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, Default)]
pub struct MaterialHandle<Material> {
    id: u32,
    index: u32,
    phantom_material: PhantomData<Material>,
}

impl<Material> MaterialHandle<Material> {
    pub(crate) fn new(id: u32, index: u32) -> Self {
        Self {
            id,
            index,
            phantom_material: PhantomData::default(),
        }
    }
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn id(&self) -> u32 {
        self.id
    }
    #[inline]
    pub fn index(&self) -> u32 {
        self.index
    }
}

pub struct ResourceRegistry {
    id: u32,
    device: vulkan::SharedDeviceRef,
    vertex_buffer_size: u64,
    index_buffer_size: u64,
    uniform_buffer_size: u64,
    storage_buffer_size: u64,
    max_set_count: u32,
    images: Vec<vulkan::Image>,
    descriptor_counts: HashMap<vk::DescriptorType, u32>,
}

static NEXT_SCENE_ID: AtomicU32 = AtomicU32::new(0);
impl ResourceRegistry {
    #[inline]
    fn get_id() -> u32 {
        NEXT_SCENE_ID.fetch_add(1, Ordering::Relaxed)
    }
    #[inline]
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn new(renderer: &Renderer) -> Self {
        Self {
            id: Self::get_id(),
            device: renderer.device.clone(),
            vertex_buffer_size: 0,
            index_buffer_size: 0,
            uniform_buffer_size: 0,
            storage_buffer_size: 0,
            max_set_count: 0,
            images: Vec::new(),
            descriptor_counts: HashMap::new(),
        }
    }
    #[inline]
    pub fn add_uniform_allocation(&mut self, byte_count: u64, alignment: u64) {
        debug_assert!(alignment != 0);
        self.uniform_buffer_size =
            self.uniform_buffer_size.next_multiple_of(alignment) + byte_count;
    }
    #[inline]
    pub fn add_storage_allocation(&mut self, byte_count: u64, alignment: u64) {
        debug_assert!(alignment != 0);
        self.storage_buffer_size =
            self.storage_buffer_size.next_multiple_of(alignment) + byte_count;
    }
    #[inline]
    pub fn add_vertex_allocation(&mut self, byte_count: u64, alignment: u64) {
        debug_assert!(alignment != 0);
        self.vertex_buffer_size = self.vertex_buffer_size.next_multiple_of(alignment) + byte_count;
    }
    #[inline]
    pub fn add_index_allocation(&mut self, byte_count: u64, alignment: u64) {
        debug_assert!(alignment != 0);
        self.index_buffer_size = self.index_buffer_size.next_multiple_of(alignment) + byte_count;
    }
    #[inline]
    pub fn add_image(&mut self, image: vulkan::Image) -> crate::ImageResourceHandle {
        let index = self.images.len() as u32;
        self.images.push(image);
        ImageResourceHandle::new(self.id, index)
    }
    #[inline]
    pub(crate) fn add_descriptors(&mut self, ty: vk::DescriptorType, count: u32) {
        let entry = self.descriptor_counts.entry(ty).or_insert(0);
        *entry += count;
    }
    #[inline]
    pub(crate) fn get_image(
        &mut self,
        image_handle: ImageResourceHandle,
    ) -> Option<&mut vulkan::Image> {
        if image_handle.id() != self.id() {
            return None;
        }
        self.images.get_mut(image_handle.index() as usize)
    }
    #[inline]
    pub(crate) fn add_sets(&mut self, count: u32) {
        self.max_set_count += count;
    }
    pub fn register(mut self, renderer: &Renderer) -> Result<ResourceUploader> {
        {
            let entry = self
                .descriptor_counts
                .entry(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .or_default();
            *entry = self.images.len() as u32;
        }

        let descriptor_pool = {
            let pool_sizes: Box<[_]> = self
                .descriptor_counts
                .into_iter()
                .map(|(ty, count)| vk::DescriptorPoolSize {
                    ty,
                    descriptor_count: count,
                })
                .collect();

            let create_info = vk::DescriptorPoolCreateInfo {
                max_sets: self.max_set_count,
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
                ..Default::default()
            };

            unsafe { renderer.device.create_descriptor_pool(&create_info)? }
        };
        Ok(ResourceUploader {
            id: self.id,
            device: renderer.device.clone(),
            descriptor_pool: descriptor_pool,
            images: self.images.into_boxed_slice(),
            vertex_allocator: vulkan::StackAllocator::new_vertex(
                self.device.clone(),
                self.vertex_buffer_size,
            )?,
            index_allocator: vulkan::StackAllocator::new_index(
                self.device.clone(),
                self.index_buffer_size,
            )?,
            uniform_allocator: vulkan::StackAllocator::new_uniform(
                self.device.clone(),
                self.uniform_buffer_size,
            )?,
            storage_allocator: vulkan::StackAllocator::new_storage(
                self.device,
                self.storage_buffer_size,
            )?,
        })
    }
}

pub struct ResourceUploader {
    id: u32,
    descriptor_pool: vk::DescriptorPool,
    device: SharedDeviceRef,
    images: Box<[vulkan::Image]>,
    pub(crate) vertex_allocator: vulkan::StackAllocator,
    pub(crate) index_allocator: vulkan::StackAllocator,
    pub(crate) uniform_allocator: vulkan::StackAllocator,
    pub(crate) storage_allocator: vulkan::StackAllocator,
}

impl ResourceUploader {
    #[inline]
    pub fn id(&self) -> u32 {
        self.id
    }
    #[inline]
    pub fn vertex_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.vertex_allocator
    }
    #[inline]
    pub fn index_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.index_allocator
    }
    #[inline]
    pub fn uniform_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.uniform_allocator
    }
    #[inline]
    pub fn storage_allocator_mut(&mut self) -> &mut vulkan::StackAllocator {
        &mut self.storage_allocator
    }
    pub fn upload(self) -> Resources {
        Resources {
            id: self.id,
            device: self.device,
            descriptor_pool: self.descriptor_pool,
            vertex_buffer: self.vertex_allocator.into_buffer(),
            index_buffer: self.index_allocator.into_buffer(),
            uniform_buffer: self.uniform_allocator.into_buffer(),
            storage_buffer: self.storage_allocator.into_buffer(),
            images: self.images,
        }
    }
}

pub struct Resources {
    id: u32,
    device: SharedDeviceRef,
    descriptor_pool: vk::DescriptorPool,
    vertex_buffer: vulkan::Buffer,
    index_buffer: vulkan::Buffer,
    uniform_buffer: vulkan::Buffer,
    storage_buffer: vulkan::Buffer,
    images: Box<[vulkan::Image]>,
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool);
        }
    }
}

impl Resources {
    #[inline]
    pub(crate) fn uniform_buffer(&self) -> &vulkan::Buffer {
        &self.uniform_buffer
    }
    #[inline]
    pub(crate) fn storage_buffer(&self) -> &vulkan::Buffer {
        &self.storage_buffer
    }
    #[inline]
    pub(crate) fn vertex_buffer(&self) -> &vulkan::Buffer {
        &self.vertex_buffer
    }
    #[inline]
    pub(crate) fn index_buffer(&self) -> &vulkan::Buffer {
        &self.index_buffer
    }
    #[inline]
    pub(crate) fn id(&self) -> u32 {
        self.id
    }
    #[inline]
    pub(crate) fn descriptor_pool(&self) -> vk::DescriptorPool {
        self.descriptor_pool
    }
    #[inline]
    pub(crate) fn get_image(&self, image_handle: ImageResourceHandle) -> Option<&vulkan::Image> {
        if image_handle.id() != self.id() {
            return None;
        }
        self.images.get(image_handle.index() as usize)
    }
}

pub struct MeshNodeRegistry<Vertex, Index> {
    id: u32,
    vertices: Vec<Vertex>,
    indices: Vec<Index>,
    submeshes: Vec<TypedSubMesh<Vertex, Index>>,
}

pub struct TypedSubMesh<Vertex, Index> {
    first_index: u32,
    index_count: u32,
    phatom_vertex: PhantomData<Vertex>,
    phantom_index: PhantomData<Index>,
}

impl<Vertex, Index> TypedSubMesh<Vertex, Index> {
    #[inline]
    pub fn first_index(&self) -> u32 {
        self.first_index
    }
    #[inline]
    pub fn index_count(&self) -> u32 {
        self.index_count
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TypedSubMeshHandle<Vertex, Index> {
    id: u32,
    index: u32,
    phatom_vertex: PhantomData<Vertex>,
    phantom_index: PhantomData<Index>,
}

pub struct MeshNodeUploader<Vertex, Index> {
    id: u32,
    scene_id: u32,
    vertices: Box<[Vertex]>,
    indices: Box<[Index]>,
    submeshes: Box<[TypedSubMesh<Vertex, Index>]>,
}

static NEXT_MESHNODE_REGISTER_ID: AtomicU32 = AtomicU32::new(0);
impl<Vertex, Index> MeshNodeRegistry<Vertex, Index> {
    fn get_id() -> u32 {
        NEXT_MESHNODE_REGISTER_ID.fetch_add(1, Ordering::Relaxed)
    }
    pub fn new() -> Self {
        Self {
            id: Self::get_id(),
            vertices: Vec::with_capacity(512),
            indices: Vec::with_capacity(512),
            submeshes: Vec::with_capacity(32),
        }
    }
    // #[inline]
    // pub fn add_vertices(&mut self, vertices: impl Iterator<Item = V>) -> (usize, usize) {
    //     let vertex_count_pre = self.vertices.len();
    //     self.vertices.extend(vertices);
    //     debug_assert!(self.vertices.len() <= self.max_vertex_count);
    //     let vertex_count_post = self.vertices.len();
    //     (vertex_count_pre, vertex_count_post)
    // }
    // #[inline]
    // pub fn add_indices(&mut self, indices: impl Iterator<Item = Index>) -> (usize, usize) {
    //     let index_count_pre = self.indices.len();
    //     self.indices.extend(indices);
    //     debug_assert!(self.indices.len() <= self.max_index_count);
    //     let index_count_post = self.indices.len();
    //     (index_count_pre, index_count_post)
    // }
    #[inline]
    pub fn add_submesh(
        &mut self,
        indices: impl Iterator<Item = Index>,
        vertices: impl Iterator<Item = Vertex>,
    ) -> TypedSubMeshHandle<Vertex, Index> {
        let first_index = self.indices.len();
        self.vertices.extend(vertices);
        self.indices.extend(indices);
        let index_count = self.indices.len() - first_index;

        let index = self.submeshes.len();

        self.submeshes.push(TypedSubMesh {
            first_index: first_index.try_into().expect("first_index exceeds u32"),
            index_count: index_count.try_into().expect("index_count exceeds u32"),
            phatom_vertex: PhantomData::default(),
            phantom_index: PhantomData::default(),
        });

        TypedSubMeshHandle {
            id: self.id,
            index: index.try_into().expect("index exceeds u32"),
            phatom_vertex: PhantomData::default(),
            phantom_index: PhantomData::default(),
        }
    }
    #[inline]
    pub fn register(
        self,
        registry: &mut crate::ResourceRegistry,
    ) -> MeshNodeUploader<Vertex, Index> {
        registry.add_vertex_allocation(
            (std::mem::size_of::<Vertex>() * self.vertices.len()) as u64,
            std::mem::size_of::<Vertex>() as u64,
        );
        registry.add_index_allocation(
            (std::mem::size_of::<Index>() * self.indices.len()) as u64,
            std::mem::size_of::<Index>() as u64,
        );

        MeshNodeUploader {
            id: self.id,
            scene_id: registry.id(),
            vertices: self.vertices.into_boxed_slice(),
            indices: self.indices.into_boxed_slice(),
            submeshes: self.submeshes.into_boxed_slice(),
        }
    }
}

impl<Vertex, Index> MeshNodeUploader<Vertex, Index> {
    pub fn upload(
        self,
        uploader: &mut crate::ResourceUploader,
    ) -> Result<MeshDataPromise<Vertex, Index>> {
        debug_assert!(self.scene_id == uploader.id());

        let vertices_allocation_size = (self.vertices.len() * std::mem::size_of::<Vertex>()) as u64;
        let indices_allocation_size = (self.indices.len() * std::mem::size_of::<Index>()) as u64;

        let _ = uploader
            .vertex_allocator
            .can_reserve(
                vertices_allocation_size,
                std::mem::size_of::<Vertex>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;
        let _ = uploader
            .index_allocator
            .can_reserve(indices_allocation_size, std::mem::size_of::<Index>() as u64)
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;

        let vertices_allocation = uploader
            .vertex_allocator
            .reserve_data(
                vertices_allocation_size,
                std::mem::size_of::<Vertex>() as u64,
            )
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;
        let indices_allocation = uploader
            .index_allocator
            .reserve_data(indices_allocation_size, std::mem::size_of::<Index>() as u64)
            .ok_or(Error::VulkanError(vulkan::Error::NotImplemented))?;

        unsafe {
            uploader
                .vertex_allocator
                .upload_data(vertices_allocation, &self.vertices)?;
            uploader
                .index_allocator
                .upload_data(indices_allocation, &self.indices)?;
        }

        Ok(MeshDataPromise {
            id: self.id,
            scene_id: self.scene_id,
            submeshes: self.submeshes,
            indices_allocation,
            vertices_allocation,
        })
    }
}

pub struct MeshDataPromise<Vertex, Index> {
    id: u32,
    scene_id: u32,
    submeshes: Box<[TypedSubMesh<Vertex, Index>]>,
    vertices_allocation: vulkan::AllocationRange,
    indices_allocation: vulkan::AllocationRange,
}

pub struct MeshData<Vertex, Index> {
    id: u32,
    resources: Arc<Resources>,
    submeshes: Box<[TypedSubMesh<Vertex, Index>]>,
    vertices_allocation: vulkan::AllocationRange,
    indices_allocation: vulkan::AllocationRange,
}

pub trait VkIndexType {
    const INDEX_TYPE: vk::IndexType;
}
impl VkIndexType for u16 {
    const INDEX_TYPE: vk::IndexType = vk::IndexType::UINT16;
}
impl VkIndexType for u32 {
    const INDEX_TYPE: vk::IndexType = vk::IndexType::UINT32;
}

impl<Vertex, Index: VkIndexType> MeshDataPromise<Vertex, Index> {
    pub fn finalize(self, resources: Arc<Resources>) -> MeshData<Vertex, Index> {
        debug_assert!(self.scene_id == resources.id());
        MeshData {
            id: self.id,
            resources,
            submeshes: self.submeshes,
            vertices_allocation: self.vertices_allocation,
            indices_allocation: self.indices_allocation,
        }
    }
}

impl<Vertex, Index: VkIndexType> MeshData<Vertex, Index> {
    pub fn get_submesh(
        &self,
        handle: TypedSubMeshHandle<Vertex, Index>,
    ) -> Option<&TypedSubMesh<Vertex, Index>> {
        debug_assert!(handle.id == self.id);
        self.submeshes.get(handle.index as usize)
    }
    pub fn bind(&self, cmd: vk::CommandBuffer, renderer: &Renderer) {
        unsafe {
            renderer.device.cmd_bind_index_buffer(
                cmd,
                self.resources.index_buffer().handle,
                self.indices_allocation.offset,
                Index::INDEX_TYPE,
            );
            renderer.device.cmd_bind_vertex_buffers(
                cmd,
                0,
                &[self.resources.vertex_buffer().handle],
                &[self.vertices_allocation.offset],
            );
        }
    }
}

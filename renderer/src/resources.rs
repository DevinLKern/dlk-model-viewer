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

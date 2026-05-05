#version 450

const uint MATERIAL_FLAG_TEXTURED_BIT = (1 << 0);

// per frame
layout(std140, set = 0, binding = 0) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;


// per object
struct InstanceData {
    mat4 model_matrix;
    mat4 normal_matrix;
    uint material_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

layout(std430, set = 1, binding = 0) buffer InstanceBuffer {
    InstanceData arr [];
} instances;

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coord;
layout(location = 2) in vec3 normal;

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) out vec3 v_normal_world_space;
layout(location = 2) flat out uint v_material_index;

void main() {
    InstanceData data = instances.arr[gl_InstanceIndex];
    gl_Position = camera.proj_matrix * camera.view_matrix * data.model_matrix * vec4(position, 1);
    v_tex_coord = tex_coord;
    // v_normal = normal;
    // v_normal_matrix = data.normal_matrix;
    v_material_index = data.material_index;
    v_normal_world_space = normalize(mat3(data.normal_matrix) * normal);
}

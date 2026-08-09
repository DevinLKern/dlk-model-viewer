#version 450


const uint MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT = (1 << 0);
const uint MATERIAL_FLAG_AMBIENT_TEXTURE_BIT = (1 << 1);
const uint MATERIAL_FLAG_SPECULAR_TEXTURE_BIT = (1 << 2);

// per frame

struct InstanceData {
    mat4 model_matrix;
    mat4 normal_matrix;
    uint material_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

layout(std430, set = 0, binding = 0) buffer InstanceBuffer {
    InstanceData arr [];
} instances;

layout(std140, set = 0, binding = 1) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;

layout(std140, set = 0, binding = 3) uniform DirectionalLightUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} light;

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coord;
layout(location = 2) in vec3 normal;

layout(location = 0) out vec3 v_pos;
layout(location = 1) out vec2 v_tex_coord;
layout(location = 2) out vec3 v_normal_world_space;
layout(location = 3) flat out uint v_material_index;
layout(location = 4) out vec4 v_pos_light_space;

void main() {
    InstanceData data = instances.arr[gl_InstanceIndex];
    gl_Position = camera.proj_matrix * camera.view_matrix * data.model_matrix * vec4(position, 1);

    v_pos = vec3(data.model_matrix * vec4(position, 1));
    v_tex_coord = tex_coord;
    v_material_index = data.material_index;
    v_normal_world_space = normalize(mat3(data.normal_matrix) * normal);
    v_pos_light_space = light.proj_matrix * light.view_matrix * data.model_matrix * vec4(position, 1);
}

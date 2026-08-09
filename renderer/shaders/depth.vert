#version 450

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

// irregular

layout(std140, set = 0, binding = 1) uniform DirectionalLightUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} light;

layout(location = 0) in vec3 position;

void main() {
    InstanceData data = instances.arr[gl_InstanceIndex];
    gl_Position = light.proj_matrix * light.view_matrix * data.model_matrix * vec4(position, 1);
}

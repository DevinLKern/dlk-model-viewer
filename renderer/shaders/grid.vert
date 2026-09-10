#version 450


// per frame
struct GridInstanceUBO {
    mat4 model_matrix;
    mat4 _pad;
    uint material_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

layout(std430, set = 0, binding = 0) buffer GridInstanceBuffer {
    GridInstanceUBO arr [];
} instances;

layout(std140, set = 0, binding = 1) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;

layout(location = 0) in vec3 position;

layout(location = 0) out vec2 v_uv;
layout(location = 1) flat out uint v_material_index;

void main() {
    GridInstanceUBO data = instances.arr[gl_InstanceIndex];
    gl_Position = camera.proj_matrix * camera.view_matrix * data.model_matrix * vec4(position, 1);

    vec3 world_pos = (data.model_matrix * vec4(position, 1.0)).xyz;
    v_uv = world_pos.xz;
    v_material_index = data.material_index;

    gl_Position =
        camera.proj_matrix *
        camera.view_matrix *
        data.model_matrix  *
        vec4(position, 1.0);
}

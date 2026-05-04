#version 450

const uint MATERIAL_FLAG_TEXTURED_BIT = (1 << 0);

// set 0 is for objects that are updated every frame
layout(std140, set = 0, binding = 0) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;

// set 1 is for objects that are update every object
layout(std140, set = 1, binding = 0) uniform MeshUBO {
    mat4 model_matrix;
    mat4 normal_matrix;
    uint material_index;
} mesh;

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coord;
layout(location = 2) in vec3 normal;

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) out vec3 v_normal;

void main() {
    gl_Position = camera.proj_matrix * camera.view_matrix * mesh.model_matrix * vec4(position, 1);
    v_tex_coord = tex_coord;
    v_normal = normal;
}

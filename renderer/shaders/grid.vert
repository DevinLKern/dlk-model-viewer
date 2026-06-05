#version 450


// per frame
layout(std140, set = 0, binding = 0) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;

// other
layout(set = 1, binding = 0) uniform GridData {
    mat4 model_matrix;
    vec2 scale;
    vec4 base_color;
    vec4 line_color;
} grid;

layout(location = 0) in vec3 position;
layout(location = 0) out vec2 v_uv;

void main()
{
    vec3 world_pos = (grid.model_matrix * vec4(position, 1.0)).xyz;
    v_uv = world_pos.xz;

    gl_Position =
        camera.proj_matrix *
        camera.view_matrix *
        grid.model_matrix *
        vec4(position, 1.0);
}

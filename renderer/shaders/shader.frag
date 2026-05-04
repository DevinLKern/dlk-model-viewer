#version 450

#extension GL_EXT_nonuniform_qualifier : enable

const uint MATERIAL_FLAG_TEXTURED_BIT = (1 << 0);

// set 1 is for objects that are update every object
layout(std140, set = 1, binding = 0) uniform MeshUBO {
    mat4 model_matrix;
    mat4 normal_matrix;
    uint material_index;
} mesh;

// set 2 is for objects that are updated irregularly
layout(std140, set = 2, binding = 0) uniform GlobalLightUBO {
    vec3 direction;
    vec4 color;
    float ambient;
} world_light;

layout (set = 2, binding = 1) uniform sampler2D global_textures[];

struct MaterialUBO {
    uint flags;
    uint texture_index;
    vec4 base_color;
};

#define MAX_MATERIALS 32

layout(std140, set = 2, binding = 2) buffer MaterialsUBO {
    MaterialUBO materials [MAX_MATERIALS];
};

// layout (location = 0) in vec3 vColor;
layout (location = 0) in vec2 v_tex_coord;
layout (location = 1) in vec3 v_normal;

layout (location = 0) out vec4 f_color;


void main() {
    // mat3 normal_matrix = transpose(inverse(mat3(mesh.model_matrix)));
    vec3 normal_world_space = normalize(mat3(mesh.normal_matrix) * v_normal);
    float light_intensity = world_light.ambient + max(0.0, dot(normal_world_space, -world_light.direction));
    
    MaterialUBO mat = materials[nonuniformEXT(mesh.material_index)];
    
    if ((mat.flags & MATERIAL_FLAG_TEXTURED_BIT) != 0) {
        f_color = texture(global_textures[nonuniformEXT(mat.texture_index)], v_tex_coord) * light_intensity;
    } else {
        f_color = mat.base_color * light_intensity;
    }
}

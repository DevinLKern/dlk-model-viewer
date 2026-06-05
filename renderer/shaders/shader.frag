#version 450

#extension GL_EXT_nonuniform_qualifier : enable

const uint MATERIAL_FLAG_TEXTURED_BIT = (1 << 0);
const uint MATERIAL_FLAG_UNLIT_BIT = (1 << 1);

struct InstanceData {
    mat4 model_matrix;
    mat4 normal_matrix;
    uint material_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

layout(std430, set = 0, binding = 1) buffer InstanceBuffer {
    InstanceData arr [];
} instances;

// irregular
layout(std140, set = 1, binding = 0) uniform GlobalLightUBO {
    vec4 direction;
    vec4 color;
    float ambient;
} world_light;

layout (set = 1, binding = 1) uniform sampler2D global_textures[];

struct MaterialUBO {
    uint flags;
    uint texture_index;
    vec4 base_color;
};

layout(std140, set = 1, binding = 2) buffer MaterialsUBO {
    MaterialUBO arr [];
} materials;

// layout (location = 0) in vec3 vColor;
layout (location = 0) in vec2 v_tex_coord;
layout (location = 1) in vec3 v_normal_world_space;
layout (location = 2) flat in uint v_material_index;

layout (location = 0) out vec4 f_color;


void main() {
    // mat3 normal_matrix = transpose(inverse(mat3(data.model_matrix)));
    vec3 L = vec3(world_light.direction);
    float light_intensity = world_light.ambient + max(0.0, dot(v_normal_world_space, -normalize(L)));
    
    MaterialUBO mat = materials.arr[nonuniformEXT(v_material_index)];

    if ((mat.flags & MATERIAL_FLAG_UNLIT_BIT) != 0) {
        light_intensity = 1.0;
    }
    
    if ((mat.flags & MATERIAL_FLAG_TEXTURED_BIT) != 0) {
        vec4 color = texture(global_textures[nonuniformEXT(mat.texture_index)], v_tex_coord);
        f_color = vec4(color.xyz * light_intensity, color.w);
    } else {
        f_color = vec4(mat.base_color.xyz * light_intensity, mat.base_color.w);
    }
}

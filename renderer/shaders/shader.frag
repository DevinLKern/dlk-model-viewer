#version 450

#extension GL_EXT_nonuniform_qualifier : enable

const uint MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT = (1 << 0);
const uint MATERIAL_FLAG_AMBIENT_TEXTURE_BIT = (1 << 1);
const uint MATERIAL_FLAG_SPECULAR_TEXTURE_BIT = (1 << 2);

layout(std140, set = 0, binding = 0) uniform CameraUBO {
    mat4 view_matrix;
    mat4 proj_matrix;
} camera;

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

struct PointLightData {
    // w is intensity / brightness
    vec4 color;
    vec3 position;
    int _pad;
};
layout(std430, set = 0, binding = 2) buffer PointLightsUBO {
    uint count;
    uint _pad0;
    uint _pad1;
    uint _pad2;
    
    PointLightData arr [];
} point_lights;

// irregular
layout(std140, set = 1, binding = 0) uniform GlobalLightUBO {
    vec4 direction;
    vec4 color;
    float ambient;
} world_light;

layout(set = 1, binding = 1) uniform sampler2D global_textures[];

struct MaterialUBO {
    vec3 diffuse_base;
    uint diffuse_texture_index;
    vec3 ambient_base;
    uint ambient_texture_index;
    vec3 specular_base;
    uint specular_texture_index;

    float shininess;
    uint flags;
    uint _pad0;
    uint _pad1;
};

layout(std140, set = 1, binding = 2) buffer MaterialsUBO {
    MaterialUBO arr [];
} materials;

layout (location = 0) in vec3 v_pos;
layout (location = 1) in vec2 v_tex_coord;
layout (location = 2) in vec3 v_normal_world_space;
layout (location = 3) flat in uint v_material_index;

layout (location = 0) out vec4 f_color;


vec3 calc_point_light(PointLightData light, MaterialUBO mat, vec3 view_dir) {
    vec3 light_dir = normalize(light.position - v_pos);
    vec3 reflect_dir = reflect(-light_dir, v_normal_world_space);

    float diff = max(dot(v_normal_world_space, light_dir), 0.0);
    float spec = pow(max(dot(view_dir, reflect_dir), 0.0), mat.shininess);

    vec3 diffuse = mat.diffuse_base * diff * light.color.xyz;
    if ((mat.flags & MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT) != 0) {
        diffuse *= texture(global_textures[nonuniformEXT(mat.diffuse_texture_index)], v_tex_coord).rgb;
    }

    vec3 specular = mat.specular_base * spec;
    if ((mat.flags & MATERIAL_FLAG_SPECULAR_TEXTURE_BIT) != 0) {
        specular *= texture(global_textures[nonuniformEXT(mat.specular_texture_index)], v_tex_coord).rgb;
    }

    float distance = length(v_pos - light.position);
    diffuse /= distance * distance;
    specular /= distance * distance;

    return (diffuse + specular) * light.color.a;
}

void main() {
    MaterialUBO mat = materials.arr[nonuniformEXT(v_material_index)];

    vec3 ambient = world_light.color.xyz * mat.ambient_base;
    if ((mat.flags & MATERIAL_FLAG_AMBIENT_TEXTURE_BIT) != 0) {
        ambient *= texture(global_textures[nonuniformEXT(mat.ambient_texture_index)], v_tex_coord).rgb;
    }
    float ambient_strength = 0.01;
    ambient *= ambient_strength;
    
    vec3 world_light_dir = normalize(vec3(world_light.direction));
    float light_intensity = world_light.ambient + max(0.0, dot(v_normal_world_space, -world_light_dir));
    
    vec3 diffuse = mat.diffuse_base;
    if ((mat.flags & MATERIAL_FLAG_DIFFUSE_TEXTURE_BIT) != 0) {
        diffuse *= texture(global_textures[nonuniformEXT(mat.diffuse_texture_index)], v_tex_coord).rgb;
    }
    diffuse *= light_intensity;
    diffuse *= world_light.color.rgb;

    float specular_strength = 1.0;
    vec3 specular = specular_strength * mat.specular_base;

    // Maybe pass in camera_pos as part of the CameraUBO struct.
    // I suspect inverse(mat4) is an expensive funtion.
    vec3 camera_pos = inverse(camera.view_matrix)[3].xyz;
    vec3 view_dir = normalize(camera_pos - v_pos);
    vec3 reflect_dir = reflect(world_light_dir, normalize(v_normal_world_space));
    float spec = pow(max(dot(view_dir, reflect_dir), 0.0), mat.shininess);
    specular *= spec;
    
    if ((mat.flags & MATERIAL_FLAG_SPECULAR_TEXTURE_BIT) != 0) {
        specular *= texture(global_textures[nonuniformEXT(mat.specular_texture_index)], v_tex_coord).rgb;
    }

    vec3 res = diffuse + specular + ambient;
        
    for (int i = 0; i < point_lights.count; i++) {
        PointLightData light = point_lights.arr[i];
        res += calc_point_light(light, mat, view_dir);
    }

    f_color = vec4(res, 1.0);
}

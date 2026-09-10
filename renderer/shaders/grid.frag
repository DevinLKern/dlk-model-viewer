#version 450

// DISCLAIMER: I used the shader described in the article below.
// https://bgolus.medium.com/the-best-darn-grid-shader-yet-727f9278b9d8

#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) in vec2 v_uv;
layout(location = 1) flat in uint v_material_index;

layout(location = 0) out vec4 f_color;

// other
struct GridMaterialUBO {
    vec4 base_color;
    vec4 line_color;
    vec2 line_width;
    vec2 scale;
};

layout(set = 1, binding = 0) buffer GridMaterialsUBO {
    GridMaterialUBO arr [];
} materials;

float infinite_grid(vec2 uv, vec2 line_width) {
    vec2 ddx = dFdx(uv);
    vec2 ddy = dFdy(uv);

    vec2 uv_deriv = vec2(length(vec2(ddx.x, ddy.x)), length(vec2(ddx.y, ddy.y)));

    bvec2 invert_line = bvec2(line_width.x > 0.5, line_width.y > 0.5);
    vec2 target_width = vec2(
        invert_line.x ? 1.0 - line_width.x : line_width.x,
        invert_line.y ? 1.0 - line_width.y : line_width.y
    );

    vec2 draw_width = vec2(
        clamp(target_width.x, uv_deriv.x, 0.5),
        clamp(target_width.y, uv_deriv.y, 0.5)
    );

    vec2 line_aa = max(uv_deriv, vec2(0.000001)) * 0.5;
    vec2 grid_uv = abs(fract(uv) * 2.0 - 1.0);

    grid_uv.x = invert_line.x ? 1.0 - grid_uv.x : grid_uv.x;
    grid_uv.y = invert_line.y ? 1.0 - grid_uv.y : grid_uv.y;

    vec2 grid2 = smoothstep(draw_width + line_aa, draw_width - line_aa, grid_uv);

    grid2 *= clamp(target_width / draw_width, 0.0, 1.0);
    grid2 = mix(grid2, target_width, clamp(uv_deriv * 2.0 - 1.0, 0.0, 1.0));
    grid2.x = invert_line.x ? 1.0 - grid2.x : grid2.x;
    grid2.y = invert_line.y ? 1.0 - grid2.y : grid2.y;

    return mix(grid2.x, 1.0, grid2.y);
}

void main() {
    GridMaterialUBO mat = materials.arr[nonuniformEXT(v_material_index)];

    float grid_mul = infinite_grid(v_uv * mat.scale, mat.line_width);

    f_color = mix(
        mat.base_color,
        mat.line_color,
        grid_mul * mat.line_color.a
    );
}

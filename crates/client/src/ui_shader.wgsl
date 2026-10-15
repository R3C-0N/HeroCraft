struct UiUniform {
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> ui_uniform: UiUniform;

@group(0) @binding(1)
var ui_texture: texture_2d<f32>;

@group(0) @binding(2)
var ui_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>, // En pixels écran (0,0 en haut à gauche)
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_ui(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.uv = in.uv;

    // Conversion coordonnées écran (pixels) -> NDC [-1.0, 1.0]
    let ndc_x = (in.position.x / ui_uniform.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / ui_uniform.screen_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_ui(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(ui_texture, ui_sampler, in.uv);
}

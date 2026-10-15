struct CameraUniform {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_pos = in.position;
    out.normal = in.normal;
    out.color = in.color;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calcul de la normale par dérivées d'écran pour un aspect facetté / pixelisé net (style voxel)
    let dpx = dpdx(in.world_pos);
    let dpy = dpdy(in.world_pos);
    let cross_norm = normalize(cross(dpx, dpy));
    let n = select(cross_norm, -cross_norm, dot(cross_norm, in.normal) < 0.0);

    let l = normalize(camera.light_dir);

    // Éclairage diffus doux (Half-Lambert)
    let ndotl = dot(n, l);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.3);
    let ambient = 0.22;
    let light_intensity = ambient + (1.0 - ambient) * half_lambert;

    // Lumière rasante atmosphérique (Rim lighting / lueur d'horizon planétaire)
    let view_dir = normalize(-in.world_pos);
    let rim = 1.0 - clamp(dot(n, view_dir), 0.0, 1.0);
    let rim_light = pow(rim, 3.5) * 0.45;
    let atmosphere_color = vec3<f32>(0.25, 0.65, 1.0); // Bleu atmosphérique

    let lit_rgb = in.color.rgb * light_intensity + atmosphere_color * rim_light;

    return vec4<f32>(lit_rgb, in.color.a);
}

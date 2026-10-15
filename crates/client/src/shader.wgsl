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
    let dpx = dpdx(in.world_pos);
    let dpy = dpdy(in.world_pos);
    let cross_norm = normalize(cross(dpx, dpy));
    let geom_n = select(cross_norm, -cross_norm, dot(cross_norm, in.normal) < 0.0);

    // Calcul de l'altitude réelle par pixel à partir de la distance au centre de la planète
    let radial_dist = length(in.world_pos);
    let base_radius = 25.0;
    let altitude = radial_dist - base_radius;

    // Dérivation du gradient d'altitude (pente et micro-relief per-pixel)
    let dh_dx = dpdx(radial_dist);
    let dh_dy = dpdy(radial_dist);

    // Perturbation de la normale selon la pente pour sculpter les crêtes de montagne
    let bump_offset = (dpx * dh_dx + dpy * dh_dy) * 45.0;
    let bumped = normalize(geom_n - bump_offset);
    let n = select(geom_n, bumped, altitude > 0.05);

    let l = normalize(camera.light_dir);

    // Éclairage diffus doux (Half-Lambert) réagissant au relief sculpté
    let ndotl = dot(n, l);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.25);
    let ambient = 0.20;
    let light_intensity = ambient + (1.0 - ambient) * half_lambert;

    // 1. Ombrage hypsométrique : met en valeur les sommets et donne du contraste aux dépressions
    let alt_factor = clamp(altitude / 0.85, 0.0, 1.0);
    let hypsometric = 0.88 + 0.24 * alt_factor;

    // 2. Micro-ombrage des marches de blocs voxels (hauteur ~0.03835)
    let block_h = 0.03835;
    let layer_frac = fract(altitude / block_h);
    let voxel_step_shade = select(1.0, 0.88 + 0.12 * smoothstep(0.08, 0.35, layer_frac), altitude > 0.05);

    // Lumière rasante atmosphérique (Rim lighting / lueur d'horizon planétaire)
    let view_dir = normalize(-in.world_pos);
    let rim = 1.0 - clamp(dot(geom_n, view_dir), 0.0, 1.0);
    let rim_light = pow(rim, 3.5) * 0.45;
    let atmosphere_color = vec3<f32>(0.25, 0.65, 1.0); // Bleu atmosphérique

    let shaded_color = in.color.rgb * light_intensity * hypsometric * voxel_step_shade;
    let lit_rgb = shaded_color + atmosphere_color * rim_light;

    return vec4<f32>(lit_rgb, in.color.a);
}

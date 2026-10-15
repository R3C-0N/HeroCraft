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

// Matrice de tramage Bayer 4x4 normalisée [0.0, 1.0] pour transition douce stochastique
fn bayer4x4(pos: vec2<u32>) -> f32 {
    let bayer = array<f32, 16>(
        0.0 / 16.0,  8.0 / 16.0,  2.0 / 16.0, 10.0 / 16.0,
        12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0,  6.0 / 16.0,
        3.0 / 16.0, 11.0 / 16.0,  1.0 / 16.0,  9.0 / 16.0,
        15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0,  5.0 / 16.0
    );
    let x = pos.x % 4u;
    let y = pos.y % 4u;
    return bayer[y * 4u + x];
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Transition douce hybride par tramage stochastique (Dithered Cross-Fade)
    let alpha = in.color.a;
    if (abs(alpha) < 0.999) {
        let screen_coord = vec2<u32>(in.clip_position.xy);
        let dither = bayer4x4(screen_coord);
        if (alpha >= 0.0) {
            // Fade-in : on conserve les pixels où dither <= alpha
            if (dither > alpha) {
                discard;
            }
        } else {
            // Fade-out : on conserve les pixels où dither > (1.0 - abs(alpha))
            let opacity = -alpha;
            if (dither <= (1.0 - opacity)) {
                discard;
            }
        }
    }

    // Normale géométrique pure de la facette (style Low-Poly / Voxel franc et net)
    let dpx = dpdx(in.world_pos);
    let dpy = dpdy(in.world_pos);
    let cross_norm = normalize(cross(dpx, dpy));
    let n = select(cross_norm, -cross_norm, dot(cross_norm, in.normal) < 0.0);

    let l = normalize(camera.light_dir);

    // Éclairage directionnel net sans dégradés flous ni artefacts
    let ndotl = dot(n, l);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.2);
    let ambient = 0.28;
    let light_intensity = ambient + (1.0 - ambient) * half_lambert;

    // Légère lueur atmosphérique rasante sur le limbe planétaire
    let view_dir = normalize(-in.world_pos);
    let rim = 1.0 - clamp(dot(n, view_dir), 0.0, 1.0);
    let rim_light = pow(rim, 4.0) * 0.35;
    let atmosphere_color = vec3<f32>(0.25, 0.65, 1.0);

    let lit_rgb = in.color.rgb * light_intensity + atmosphere_color * rim_light;

    return vec4<f32>(lit_rgb, 1.0);
}

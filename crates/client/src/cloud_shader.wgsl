// =========================================================================
// HeroCraft - 3D Cloud Mesh Shader (WGSL)
// - Rendu 3D temps réel d'objets géométriques physiques dans le monde
// - Dérive douce du vent autour de l'axe de rotation planétaire
// - Biseaux et congés arrondis nets (style Minecraft Complementary Reimagined)
// - Éclairage dynamique : soleil direct doré, crépuscule lavande, nuit lunaire
// - Deux faces visibles (cull_mode: None) permettant de traverser les nuages
// =========================================================================

struct CameraUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    planet_radius: f32,
    light_dir: vec3<f32>,
    time: f32,
    lighting_params: vec4<f32>,
    lighting_params2: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>, // x: u, y: v, z: bevel_mask, w: face_type
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bevel_mask: f32,
    @location(4) face_type: f32,
};

@vertex
fn vs_cloud(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Dérive douce du vent autour de l'axe planétaire (Y)
    let wind_speed = 0.0035; // ~30 minutes pour une révolution planétaire complète
    let angle = camera.time * wind_speed;
    let cos_a = cos(angle);
    let sin_a = sin(angle);

    // Rotation autour de l'axe Y
    let pos_rot = vec3<f32>(
        in.position.x * cos_a - in.position.z * sin_a,
        in.position.y,
        in.position.x * sin_a + in.position.z * cos_a
    );
    let norm_rot = vec3<f32>(
        in.normal.x * cos_a - in.normal.z * sin_a,
        in.normal.y,
        in.normal.x * sin_a + in.normal.z * cos_a
    );

    out.world_pos = pos_rot;
    out.normal = normalize(norm_rot);
    out.uv = in.color.xy;
    out.bevel_mask = in.color.z;
    out.face_type = in.color.w;
    out.clip_position = camera.view_proj * vec4<f32>(pos_rot, 1.0);
    return out;
}

struct MoonData {
    dir: vec3<f32>,
    phase: f32,
};

fn get_moon_data(time: f32, light_dir: vec3<f32>) -> MoonData {
    let l_sun = normalize(light_dir);
    let moon_tilt = 0.497;
    let u_moon = vec3<f32>(cos(moon_tilt), sin(moon_tilt), 0.0);
    let v_moon = vec3<f32>(0.0, 0.0, 1.0);
    let moon_period = 1260.0;
    let moon_angle = (time / moon_period) * 6.2831853 + 3.69;
    let moon_dir = normalize(u_moon * cos(moon_angle) + v_moon * sin(moon_angle));
    let sun_moon_dot = dot(moon_dir, l_sun);
    let moon_phase = clamp((-sun_moon_dot + 1.0) * 0.5, 0.0, 1.0);
    var data: MoonData;
    data.dir = moon_dir;
    data.phase = moon_phase;
    return data;
}

@fragment
fn fs_cloud(in: VertexOutput) -> @location(0) vec4<f32> {
    // Multiplicateur de densité / visibilité des nuages (0.0 en vue satellite)
    let cloud_slider = camera.lighting_params2.z;
    if (cloud_slider < 0.01) {
        discard;
    }

    let mask = u32(in.bevel_mask + 0.5);
    // Bits 0..3: Découpe des coins extérieurs réels du quad (aucun arrondi sur les jonctions intérieures)
    let c00 = (mask & 1u) != 0u; // Coin (u=0, v=0)
    let c10 = (mask & 2u) != 0u; // Coin (u=1, v=0)
    let c11 = (mask & 4u) != 0u; // Coin (u=1, v=1)
    let c01 = (mask & 8u) != 0u; // Coin (u=0, v=1)

    let uv = in.uv;
    let r_bevel = 0.14; // Rayon de chanfrein arrondi sur les arêtes et coins extérieurs

    // Découpe chirurgicale des 4 coins extérieurs réels de chaque face
    if (c00 && uv.x < r_bevel && uv.y < r_bevel) {
        if (length(vec2<f32>(r_bevel - uv.x, r_bevel - uv.y)) > r_bevel) {
            discard;
        }
    }
    if (c10 && (1.0 - uv.x) < r_bevel && uv.y < r_bevel) {
        if (length(vec2<f32>(r_bevel - (1.0 - uv.x), r_bevel - uv.y)) > r_bevel) {
            discard;
        }
    }
    if (c11 && (1.0 - uv.x) < r_bevel && (1.0 - uv.y) < r_bevel) {
        if (length(vec2<f32>(r_bevel - (1.0 - uv.x), r_bevel - (1.0 - uv.y))) > r_bevel) {
            discard;
        }
    }
    if (c01 && uv.x < r_bevel && (1.0 - uv.y) < r_bevel) {
        if (length(vec2<f32>(r_bevel - uv.x, r_bevel - (1.0 - uv.y))) > r_bevel) {
            discard;
        }
    }

    // Bits 4..7: Lissage des normales sur les bordures extérieures réelles
    let edge_u_min = (mask & 16u) != 0u; // Bord u=0
    let edge_u_max = (mask & 32u) != 0u; // Bord u=1
    let edge_v_min = (mask & 64u) != 0u; // Bord v=0
    let edge_v_max = (mask & 128u) != 0u; // Bord v=1

    var norm = in.normal;
    var bevel_offset = vec2<f32>(0.0);
    if (edge_u_min && uv.x < r_bevel) {
        bevel_offset.x -= (r_bevel - uv.x) / r_bevel;
    }
    if (edge_u_max && (1.0 - uv.x) < r_bevel) {
        bevel_offset.x += (r_bevel - (1.0 - uv.x)) / r_bevel;
    }
    if (edge_v_min && uv.y < r_bevel) {
        bevel_offset.y -= (r_bevel - uv.y) / r_bevel;
    }
    if (edge_v_max && (1.0 - uv.y) < r_bevel) {
        bevel_offset.y += (r_bevel - (1.0 - uv.y)) / r_bevel;
    }

    if (length(bevel_offset) > 0.01) {
        let dp1 = dpdx(in.world_pos);
        let dp2 = dpdy(in.world_pos);
        let duv1 = dpdx(in.uv);
        let duv2 = dpdy(in.uv);
        let det = duv1.x * duv2.y - duv1.y * duv2.x;
        if (abs(det) > 1e-6) {
            let tan_u = normalize((dp1 * duv2.y - dp2 * duv1.y) / det);
            let tan_v = normalize((dp2 * duv1.x - dp1 * duv2.x) / det);
            norm = normalize(norm + (tan_u * bevel_offset.x + tan_v * bevel_offset.y) * 0.50);
        }
    }

    // Paramètres d'éclairage interactifs
    let moon_slider = camera.lighting_params.x;
    let sun_slider = camera.lighting_params.y;
    let night_amb_slider = camera.lighting_params.z;
    let day_amb_slider = camera.lighting_params.w;

    let up = normalize(in.world_pos);
    let l_sun = normalize(camera.light_dir);
    let sun_up = dot(l_sun, up);
    let daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);

    let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
    let sunset_factor = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);

    let moon = get_moon_data(camera.time, camera.light_dir);

    // Éclairage direct du soleil
    let ndotl = max(dot(norm, l_sun), 0.0);
    let direct_sun = ndotl * (1.20 + 0.40 * sunset_factor);

    let view_dir = normalize(camera.camera_pos - in.world_pos);
    let forward = max(dot(-view_dir, l_sun), 0.0);
    let rim = pow(forward, 6.0) * (0.65 + 0.90 * sunset_factor);

    let sun_highlight = mix(vec3<f32>(1.0, 0.98, 0.92), vec3<f32>(1.0, 0.82, 0.48), sunset_factor);
    let direct_light = sun_highlight * (direct_sun + rim * 0.55) * daylight * sun_slider;

    // Ambiance du ciel (teinte lavande / violet crépusculaire de la photo)
    let ambient_day = vec3<f32>(0.74, 0.84, 0.96);
    let ambient_sunset = vec3<f32>(0.64, 0.48, 0.68);
    let cloud_ambient = mix(ambient_day, ambient_sunset, sunset_factor) * daylight * day_amb_slider;
    let ndotup = dot(norm, up);
    let ambient_facing = 0.72 + 0.28 * clamp(ndotup * 0.5 + 0.5, 0.0, 1.0);
    let ambient_light = cloud_ambient * ambient_facing;

    // Éclairage lunaire de nuit
    let ndotm = max(dot(norm, moon.dir), 0.0);
    let moon_direct = ndotm * moon.phase * moon_slider * (1.0 - daylight);
    let moon_light = vec3<f32>(0.70, 0.84, 1.0) * (moon_direct * 0.70);
    let night_ambient = vec3<f32>(0.04, 0.07, 0.15) * night_amb_slider * (1.0 - daylight);

    // Ombrage ambiant doux
    let ao = 0.78 + 0.22 * clamp(ndotup * 0.5 + 0.5, 0.0, 1.0);
    let cloud_color = (direct_light + ambient_light + moon_light + night_ambient) * ao;

    // Opacité solide en jeu, estompage fluide en dézoomant vers le mode satellite
    let alpha = clamp(cloud_slider, 0.0, 1.0);

    return vec4<f32>(cloud_color, alpha);
}

// =========================================================================
// HeroCraft - Terrain & Voxel Surface Shader (WGSL)
// - Éclairage Half-Lambert & normales géométriques nettes
// - Interpolation douce des couleurs et de l'Ambient Occlusion (AO)
// - Reflets spéculaires intenses de l'eau (Blinn-Phong + Fresnel) et scintillement de neige
// - Lueur atmosphérique rasante sur le limbe planétaire en vue orbitale
// - Brouillard atmosphérique volumétrique de distance au sol
// - Dithered Cross-Fade (tramage stochastique Bayer 4x4) pour transitions LOD
// =========================================================================

struct CameraUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    planet_radius: f32,
    light_dir: vec3<f32>,
    time: f32,
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
    @location(1) color: vec4<f32>, // Interpolation douce pour l'AO per-vertex
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
    // 1. Transition douce hybride par tramage stochastique (Dithered Cross-Fade)
    let alpha = in.color.a;
    if (abs(alpha) < 0.999) {
        let screen_coord = vec2<u32>(in.clip_position.xy);
        let dither = bayer4x4(screen_coord);
        if (alpha >= 0.0) {
            if (dither > alpha) {
                discard;
            }
        } else {
            let opacity = -alpha;
            if (dither <= (1.0 - opacity)) {
                discard;
            }
        }
    }

    // 2. Normale géométrique pure de la facette (style Voxel et Low-Poly franc et net)
    let dpx = dpdx(in.world_pos);
    let dpy = dpdy(in.world_pos);
    let cross_norm = normalize(cross(dpx, dpy));
    let n = select(cross_norm, -cross_norm, dot(cross_norm, in.normal) < 0.0);

    let l = normalize(camera.light_dir);
    let view_dir = normalize(camera.camera_pos - in.world_pos);
    let h = normalize(l + view_dir);

    // 3. Éclairage Half-Lambert doux et contrasté
    let ndotl = dot(n, l);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.25);
    let ambient = 0.26;
    let light_intensity = ambient + (1.0 - ambient) * half_lambert;

    // Détection des types de matériaux selon la couleur du bloc/terrain
    // Eau : teinte bleutée à dominante bleue
    let is_water = in.color.b > 0.45 && in.color.r < 0.30;
    // Neige / Glace : haute luminance et équilibre des canaux
    let is_snow = in.color.r > 0.82 && in.color.g > 0.84 && in.color.b > 0.88;

    // 4. Reflets spéculaires d'eau et de glace
    var specular_light = vec3<f32>(0.0);
    let ndoth = max(dot(n, h), 0.0);
    let ndotv = max(dot(n, view_dir), 0.0);

    if (is_water) {
        // Reflet spéculaire Blinn-Phong tranchant pour l'océan
        let sun_spec = pow(ndoth, 64.0) * 1.6;
        // Effet Fresnel rasante
        let fresnel = pow(1.0 - ndotv, 5.0) * 0.45 + 0.05;
        specular_light += vec3<f32>(1.0, 0.95, 0.85) * (sun_spec * clamp(ndotl * 2.0, 0.0, 1.0) + fresnel * 0.35);
    } else if (is_snow) {
        // Scintillement cristallin diffus sur les névés et glaciers
        let snow_glint = pow(ndoth, 24.0) * 0.40 * clamp(ndotl * 2.0, 0.0, 1.0);
        specular_light += vec3<f32>(0.92, 0.95, 1.0) * snow_glint;
    }

    // 5. Lueur atmosphérique rasante sur le limbe planétaire en vue orbitale
    let rim = 1.0 - clamp(dot(n, view_dir), 0.0, 1.0);
    let rim_light = pow(rim, 4.0) * 0.45 * clamp(ndotl * 0.5 + 0.5, 0.0, 1.0);
    let atmosphere_color = vec3<f32>(0.25, 0.65, 1.0);

    // Couleur éclairée de base avec Ambient Occlusion (AO) per-vertex interpolé
    var lit_rgb = in.color.rgb * light_intensity + specular_light + atmosphere_color * rim_light;

    // 6. Brouillard atmosphérique de distance au sol (Ground Distance Fog)
    let dist = length(camera.camera_pos - in.world_pos);
    let cam_alt = length(camera.camera_pos) - camera.planet_radius;
    let ground_proximity = clamp(1.0 - cam_alt / 8.0, 0.0, 1.0);

    if (ground_proximity > 0.001) {
        // Début du brouillard à 1.5 unités (~40 blocs), saturation douce à 7.5 unités (~200 blocs)
        let fog_factor = clamp((dist - 1.5) / 6.0, 0.0, 1.0) * ground_proximity;

        // Diffusion dorée vers le soleil (Mie forward scattering)
        let forward_scatter = pow(max(dot(view_dir, -l), 0.0), 3.0);
        let fog_ambient = vec3<f32>(0.62, 0.76, 0.94);
        let fog_sun = vec3<f32>(1.0, 0.82, 0.55);
        let fog_color = mix(fog_ambient, fog_sun, forward_scatter * 0.55);

        lit_rgb = mix(lit_rgb, fog_color, fog_factor * 0.80);
    }

    return vec4<f32>(lit_rgb, 1.0);
}

// =========================================================================
// HeroCraft - Rayons crépusculaires (ombres volumétriques dans l'air)
// Le ciel, le terrain et les nuages comptent déjà la lumière diffusée par l'air entre la caméra
// et eux, comme si tout l'air était au soleil. Cette passe marche le long de chaque pixel
// jusqu'à la surface vue, lit les cartes d'ombre du relief et des nuages, et retire la lumière
// que l'air à l'ombre n'a pas pu diffuser (mélange soustractif sur la cible HDR).
// Les traînées claires et sombres qui en résultent sont les rayons crépusculaires.
// =========================================================================

#import herocraft::common::camera
#import herocraft::atmosphere::{
    atmosphere, sample_medium, rayleigh_phase, mie_phase, ray_sphere, transmittance_to_top, planet_shadow, PI,
    SKY_RADIANCE_GAIN
}
#import herocraft::shadow::{shadow, relief_light_visibility, cloud_opacity}

@group(2) @binding(0) var scene_depth_ms: texture_depth_multisampled_2d;
@group(2) @binding(1) var scene_depth: texture_depth_2d;

struct FullscreenOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullscreenOutput {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullscreenOutput;
    out.position = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// Bruit de gradient entrelacé (Jimenez 2014) : décale le départ de la marche d'un pixel à
// l'autre, les bandes régulières deviennent un grain fin
fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn god_rays(pixel: vec2<f32>, depth: f32, size: vec2<f32>) -> vec4<f32> {
    let uv = pixel / size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let world_h = camera.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    let world = world_h.xyz / world_h.w;
    let origin = camera.camera_pos;
    let to_surface = world - origin;
    let dir = normalize(to_surface);
    // Le ciel (profondeur 1) n'a pas de surface : la marche s'arrête à la portée des cartes
    let surface_dist = select(length(to_surface), 1e9, depth >= 1.0);

    let sun_dir = normalize(camera.light_dir);
    let top = ray_sphere(origin, dir, atmosphere.top_radius);
    let t_start = max(top.x, 0.0);
    // Arrêt un peu avant la surface : sa propre profondeur dans la carte d'ombre la marquerait
    // à moitié à l'ombre (points sombres sur l'eau vue en rasant)
    let margin = 0.03 * camera.planet_radius / 25.0 + 0.03 * surface_dist;
    let t_end = min(min(surface_dist - margin, atmosphere.params.z), max(top.y, 0.0));
    if (t_end <= t_start) {
        return vec4<f32>(0.0);
    }

    let cos_theta = dot(dir, sun_dir);
    let phase_r = rayleigh_phase(cos_theta);
    let phase_m = mie_phase(cos_theta);
    let cloud_base = atmosphere.params.w;
    // Surfaces : même densité réduite que leur perspective aérienne (atmosphere.wgsl)
    let density = select(atmosphere.sky.z, 1.0, depth >= 1.0);

    let steps = 40;
    let dt = (t_end - t_start) / f32(steps);
    let jitter = interleaved_gradient_noise(pixel);
    var trans = vec3<f32>(1.0);
    var lost = vec3<f32>(0.0);
    var lit = vec3<f32>(0.0);
    for (var i = 0; i < steps; i = i + 1) {
        let p = origin + dir * (t_start + (f32(i) + jitter) * dt);
        let r = length(p);
        let h = r - atmosphere.bottom_radius;
        let mu_s = dot(p / r, sun_dir);
        let medium = sample_medium(h);

        // Ombre du relief, puis des nuages pour l'air sous leur couche
        var vis = mix(1.0, relief_light_visibility(p), shadow.params.x);
        if (h < cloud_base) {
            vis *= 1.0 - cloud_opacity(p) * shadow.params.y;
        }

        let sun_t = transmittance_to_top(h, mu_s) * planet_shadow(h, mu_s);
        let single = (medium.rayleigh * phase_r + medium.mie * phase_m) * sun_t * density;
        let extinction = medium.extinction * density;
        let step_t = exp(-extinction * dt);
        let s_int = single * (1.0 - step_t) / max(extinction, vec3<f32>(1e-7));
        lit += trans * s_int;
        lost += trans * s_int * (1.0 - vis);
        trans *= step_t;
    }
    // Une intensité au-dessus de 1 accentue les rayons, sans retirer plus que la lumière que ce
    // tronçon d'air diffusait (l'image deviendrait noire)
    // Même facteur sur les trois canaux pour garder la teinte de l'air (sinon franges colorées)
    let headroom = lit / max(lost, vec3<f32>(1e-9));
    let gain = min(atmosphere.params.y, min(headroom.r, min(headroom.g, headroom.b)));
    let loss = lost * gain * atmosphere.sun_illuminance * (PI * SKY_RADIANCE_GAIN * atmosphere.sky.x);
    return vec4<f32>(max(loss, vec3<f32>(0.0)), 0.0);
}

@fragment
fn fs_god_rays_ms(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let depth = textureLoad(scene_depth_ms, pixel, 0);
    return god_rays(in.position.xy, depth, vec2<f32>(textureDimensions(scene_depth_ms)));
}

@fragment
fn fs_god_rays(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let depth = textureLoad(scene_depth, pixel, 0);
    return god_rays(in.position.xy, depth, vec2<f32>(textureDimensions(scene_depth)));
}

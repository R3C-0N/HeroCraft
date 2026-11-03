// =========================================================================
// HeroCraft - Rayons crépusculaires (ombres volumétriques dans l'air)
// Le ciel, le terrain et les nuages comptent déjà la lumière diffusée par l'air entre la caméra
// et eux, comme si tout l'air était au soleil. Cette passe marche le long de chaque pixel
// jusqu'à la surface vue, lit les cartes d'ombre du relief et des nuages, et retire la lumière
// que l'air à l'ombre n'a pas pu diffuser (mélange soustractif sur la cible HDR).
// Les traînées claires et sombres qui en résultent sont les rayons crépusculaires.
// La marche se fait à résolution réduite, puis une remontée bilatérale guidée par la profondeur
// ramène le résultat en pleine résolution sans baver sur les bords. Rayons fins : demi-résolution
// (4 fois moins de pixels) et 28 pas ; allégés : quart de résolution (16 fois moins) et 14 pas.
// =========================================================================

#import herocraft::common::camera
#import herocraft::atmosphere::{
    atmosphere, sample_medium, rayleigh_phase, mie_phase, ray_sphere, transmittance_to_top, planet_shadow, PI,
    SKY_RADIANCE_GAIN
}
#import herocraft::shadow::{shadow, relief_light_visibility, cloud_opacity}

@group(2) @binding(0) var scene_depth_ms: texture_depth_multisampled_2d;
@group(2) @binding(1) var scene_depth: texture_depth_2d;
// Lumière perdue (rgb) et distance de la surface (a), à résolution réduite
@group(3) @binding(0) var half_rays: texture_2d<f32>;

// Rapport entre l'image et la cible des rayons (2 ou 4, atmosphere.rs `god_rays_divisor`)
fn ray_divisor() -> i32 {
    return i32(atmosphere.sky.w + 0.5);
}

// Distance stockée pour le ciel (sous le maximum d'un flottant 16 bits)
const SKY_DISTANCE: f32 = 60000.0;

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

// Distance caméra-surface d'un pixel (SKY_DISTANCE pour le ciel)
fn view_distance(pixel: vec2<f32>, depth: f32, size: vec2<f32>) -> f32 {
    if (depth >= 1.0) {
        return SKY_DISTANCE;
    }
    let uv = pixel / size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let world_h = camera.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    return min(length(world_h.xyz / world_h.w - camera.camera_pos), SKY_DISTANCE);
}

// `pixel` : position en pleine résolution, `noise_pixel` : pixel qui choisit le décalage du bruit
fn god_rays(pixel: vec2<f32>, noise_pixel: vec2<f32>, depth: f32, size: vec2<f32>) -> vec4<f32> {
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

    // Moitié moins de pas pour les rayons allégés : le flou de la remontée lisse le grain
    let steps = select(28, 14, ray_divisor() > 2);
    let dt = (t_end - t_start) / f32(steps);
    let jitter = interleaved_gradient_noise(noise_pixel);
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

// Résolution réduite : chaque texel couvre f x f pixels (f = 2 ou 4). Il prend la profondeur la
// plus proche ou la plus lointaine des quatre coins du bloc, en damier, pour que les deux côtés
// d'un bord aient des texels.
fn trace_half(half_pixel: vec2<i32>, d: vec4<f32>, size: vec2<f32>) -> vec4<f32> {
    let near = min(min(d.x, d.y), min(d.z, d.w));
    let far = max(max(d.x, d.y), max(d.z, d.w));
    let depth = select(near, far, ((half_pixel.x + half_pixel.y) & 1) == 1);
    let f = ray_divisor();
    let pixel = vec2<f32>(half_pixel * f) + vec2<f32>(f32(f) * 0.5);
    let loss = god_rays(pixel, vec2<f32>(half_pixel), depth, size);
    return vec4<f32>(loss.rgb, view_distance(pixel, depth, size));
}

@fragment
fn fs_trace_ms(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let h = vec2<i32>(in.position.xy);
    let size = vec2<i32>(textureDimensions(scene_depth_ms));
    let f = ray_divisor();
    let p = min(h * f, size - 1);
    let q = min(h * f + f - 1, size - 1);
    let d = vec4<f32>(
        textureLoad(scene_depth_ms, p, 0),
        textureLoad(scene_depth_ms, vec2<i32>(q.x, p.y), 0),
        textureLoad(scene_depth_ms, vec2<i32>(p.x, q.y), 0),
        textureLoad(scene_depth_ms, q, 0),
    );
    return trace_half(h, d, vec2<f32>(size));
}

@fragment
fn fs_trace(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let h = vec2<i32>(in.position.xy);
    let size = vec2<i32>(textureDimensions(scene_depth));
    let f = ray_divisor();
    let p = min(h * f, size - 1);
    let q = min(h * f + f - 1, size - 1);
    let d = vec4<f32>(
        textureLoad(scene_depth, p, 0),
        textureLoad(scene_depth, vec2<i32>(q.x, p.y), 0),
        textureLoad(scene_depth, vec2<i32>(p.x, q.y), 0),
        textureLoad(scene_depth, q, 0),
    );
    return trace_half(h, d, vec2<f32>(size));
}

// Remontée bilatérale : 3x3 texels autour du pixel, pondérés par la distance à l'écran et par
// l'écart de profondeur (un texel d'une autre surface ne compte presque pas). Le flou 3x3
// lisse aussi le grain du bruit de départ de la marche.
fn upsample(pixel: vec2<f32>, depth: f32, size: vec2<f32>) -> vec4<f32> {
    let dist = view_distance(pixel, depth, size);
    let half_size = vec2<i32>(textureDimensions(half_rays));
    // Le texel i est centré sur le pixel plein f i + f / 2
    let f = f32(ray_divisor());
    let h = (pixel - vec2<f32>(f * 0.5)) / f;
    let base = vec2<i32>(round(h));
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let t = clamp(base + vec2<i32>(x, y), vec2<i32>(0), half_size - 1);
            let s = textureLoad(half_rays, t, 0);
            // Poids en tente (bilinéaire élargi au voisinage 3x3)
            let off = abs(vec2<f32>(t) - h);
            let tent = max(vec2<f32>(1.5) - off, vec2<f32>(0.0));
            let w_space = tent.x * tent.y;
            let rel = abs(s.a - dist) / max(dist, 1e-3);
            let w_depth = 1.0 / (1e-4 + rel * rel * 400.0);
            let w = w_space * w_depth;
            sum += s.rgb * w;
            weight += w;
        }
    }
    return vec4<f32>(sum / max(weight, 1e-12), 0.0);
}

@fragment
fn fs_upsample_ms(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    return upsample(in.position.xy, textureLoad(scene_depth_ms, pixel, 0), vec2<f32>(textureDimensions(scene_depth_ms)));
}

@fragment
fn fs_upsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    return upsample(in.position.xy, textureLoad(scene_depth, pixel, 0), vec2<f32>(textureDimensions(scene_depth)));
}

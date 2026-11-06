// =========================================================================
// HeroCraft - Pluie et neige (precipitation.rs)
// Une instance par colonne de blocs autour du joueur, `drops` gouttes par instance (6 sommets
// chacune). La chute est calculée ici à partir du temps : chaque goutte tombe en boucle du haut
// du volume jusqu'au dessus de sa colonne (toit, feuillage, sol), où elle disparaît.
// Pluie : traits fins étirés dans le sens de la chute ; neige : flocons qui ondulent en tombant.
// =========================================================================

#import herocraft::common::{camera, flatten_point}
#import herocraft::atmosphere::sky_light

struct Precip {
    // Verticale au joueur (xyz), temps en secondes (w)
    up: vec4<f32>,
    // Est local (xyz), force de l'averse (w)
    east: vec4<f32>,
    // Nord local (xyz), part de neige (w)
    north: vec4<f32>,
    // x : taille d'un bloc, y : rayon des pieds, z : vent (dérive par bloc de chute), w : gouttes
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> precip: Precip;

const ABOVE_BLOCKS: f32 = 16.0;
const BELOW_BLOCKS: f32 = 10.0;
const RADIUS_BLOCKS: f32 = 20.0;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    // Opacité (fondu au bord du volume et près de l'œil)
    @location(1) alpha: f32,
    // Coordonnées dans le quadrilatère (-1 à 1)
    @location(2) quad: vec2<f32>,
};

fn hash(n: u32) -> f32 {
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    x = (x >> 22u) ^ x;
    return f32(x) / 4294967295.0;
}

@vertex
fn vs_precip(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32, @location(0) column: vec4<f32>) -> VertexOutput {
    var out: VertexOutput;
    let drop = vi / 6u;
    let corner = vi % 6u;
    let block = precip.params.x;
    let feet_r = precip.params.y;
    let snow = precip.north.w;
    let seed = ii * 16u + drop;
    let r0 = hash(seed * 3u);
    let r1 = hash(seed * 3u + 1u);
    let r2 = hash(seed * 3u + 2u);

    // Chute en boucle dans le volume : du haut (au-dessus des pieds) au bas (en dessous)
    let top_r = feet_r + ABOVE_BLOCKS * block;
    let span = (ABOVE_BLOCKS + BELOW_BLOCKS) * block;
    let speed_blocks = mix(13.0 + 4.0 * r2, 1.5 + 0.8 * r2, snow);
    let t = precip.up.w;
    let fall = fract(r0 + t * speed_blocks * block / span);
    let r = top_r - fall * span;
    let bottom = max(column.w, top_r - span);
    let dir = column.xyz;
    // Arrêtée par le dessus de la colonne, ou hors du volume : rien
    let hidden = r < bottom || column.w >= top_r;

    // Place dans la colonne, dérive du vent depuis le haut, ondulation des flocons
    let fallen_blocks = (top_r - r) / block;
    let sway = snow * 0.35 * sin(t * (1.1 + r1) + r0 * 40.0);
    let offset_e = (r1 - 0.5) + precip.params.z * (fallen_blocks - ABOVE_BLOCKS) + sway;
    let offset_n = (r2 - 0.5) + snow * 0.25 * cos(t * (0.9 + r2) + r1 * 30.0);
    let center = dir * r + (precip.east.xyz * offset_e + precip.north.xyz * offset_n) * block;

    // Fondu au bord du carré de colonnes, et tout près de l'œil (un trait énorme sinon)
    let to_eye = camera.camera_pos - center;
    let eye_dist = length(to_eye);
    let horizontal = length(to_eye - precip.up.xyz * dot(to_eye, precip.up.xyz)) / block;
    out.alpha = (1.0 - smoothstep(RADIUS_BLOCKS - 6.0, RADIUS_BLOCKS - 0.5, horizontal)) * smoothstep(0.25 * block, 1.2 * block, eye_dist);
    // Moins de gouttes visibles entre deux paliers : la force fait varier la transparence
    out.alpha *= clamp(precip.params.w - f32(drop), 0.0, 1.0);

    // Quadrilatère : un trait le long de la chute (pluie), un carré face à l'œil (neige)
    let fall_axis = normalize(precip.up.xyz - precip.east.xyz * precip.params.z * (1.0 - snow));
    let view = to_eye / max(eye_dist, 1e-6);
    let across = normalize(cross(fall_axis, view) + vec3<f32>(1e-6));
    let flake_up = normalize(cross(view, across));
    let half_len = mix(0.42, 0.045, snow) * block;
    let half_wid = mix(0.014, 0.045, snow) * block;
    let along_axis = mix(fall_axis, flake_up, snow);
    var q = vec2<f32>(-1.0, -1.0);
    switch corner {
        case 1u: { q = vec2<f32>(1.0, -1.0); }
        case 2u: { q = vec2<f32>(1.0, 1.0); }
        case 3u: { q = vec2<f32>(-1.0, -1.0); }
        case 4u: { q = vec2<f32>(1.0, 1.0); }
        case 5u: { q = vec2<f32>(-1.0, 1.0); }
        default: {}
    }
    let p = center + across * (q.x * half_wid) + along_axis * (q.y * half_len);
    out.world_pos = p;
    out.quad = q;
    out.clip_position = camera.view_proj * vec4<f32>(flatten_point(p).pos, 1.0);
    if (hidden || out.alpha <= 0.001) {
        out.clip_position = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    }
    return out;
}

@fragment
fn fs_precip(in: VertexOutput) -> @location(0) vec4<f32> {
    let snow = precip.north.w;
    let up = normalize(in.world_pos);
    let sun = normalize(camera.light_dir);
    // Éclairées par le ciel (gris sous l'averse), un peu par la Lune la nuit, et par les éclairs
    let sky = sky_light(in.world_pos, up, sun);
    let night = vec3<f32>(0.035, 0.045, 0.07) * camera.lighting_params.z;
    let flash = vec3<f32>(1.6, 1.7, 2.0) * camera.weather.w;
    // Gouttes et flocons renvoient la lumière diffuse du ciel, sans teinte propre
    let sky_level = dot(sky, vec3<f32>(0.3, 0.5, 0.2));
    let light = vec3<f32>(sky_level * 1.1) + night + flash;
    // Flocon rond, trait de pluie plus dense au milieu
    let d = length(in.quad);
    let shape = mix(1.0 - abs(in.quad.x), 1.0 - smoothstep(0.4, 1.0, d), snow);
    let color = mix(vec3<f32>(0.62, 0.68, 0.78), vec3<f32>(1.0, 1.0, 1.0), snow) * light;
    let alpha = in.alpha * shape * mix(0.5, 0.9, snow);
    return vec4<f32>(color, alpha);
}

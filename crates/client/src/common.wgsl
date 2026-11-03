// =========================================================================
// HeroCraft - Module WGSL partagé (naga_oil)
// Uniforme caméra commun au ciel, au terrain et aux nuages. La Lune y est calculée une fois par
// image (renderer.rs, `compute_moon_data`) : sa position reste la même partout.
// =========================================================================
#define_import_path herocraft::common

struct CameraUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    planet_radius: f32,
    light_dir: vec3<f32>,
    time: f32,
    lighting_params: vec4<f32>,
    lighting_params_ext: vec4<f32>,
    cloud_params: vec4<f32>,
    // x : surcroît de rayon du globe apparent, (k - 1) × rayon de la mer (0 = vraie courbure), y : k,
    // z et w : distances au sol depuis la caméra où l'aplatissement commence et devient complet
    flatten: vec4<f32>,
    // Direction de la Lune (xyz) et sa phase éclairée (w, 0 à 1)
    moon: vec4<f32>,
    // x, y : inverse de la taille de l'image en pixels ; z : amplitude relative du tramage avant
    // l'écriture dans une cible HDR à mantisse courte (0 en Rgba16Float) ; w : part de vue
    // orbitale (0 au sol, 1 en orbite haute)
    screen: vec4<f32>,
    // x : échelle des dimensions mesurées en blocs (rayon / 25 sur le prototype de 1 024 blocs par
    // face, 2 aujourd'hui), y : taille d'un bloc en unités du monde
    scales: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Position à l'écran (NDC) d'un pixel de la passe 3D, depuis @builtin(position)
fn screen_ndc(pixel: vec2<f32>) -> vec2<f32> {
    let uv = pixel * camera.screen.xy;
    return vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
}

// Tramage avant l'écriture dans la cible HDR : en Rg11b10Ufloat, la mantisse n'a que 6 bits (5
// pour le bleu) et les dégradés lisses (ciel, brume) feraient des bandes. Un bruit de gradient
// entrelacé (Jimenez 2014), d'un pas de mantisse d'amplitude, les change en grain invisible.
// Sans effet en Rgba16Float (amplitude nulle).
fn quantization_dither(color: vec3<f32>, pixel: vec2<f32>) -> vec3<f32> {
    let noise = fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715)))) - 0.5;
    return color * (1.0 + noise * camera.screen.z * vec3<f32>(1.0, 1.0, 2.0));
}

// --- Aplatissement de la planète vue du sol (renderer.rs, `planet_flattening`) ---

struct FlatPoint {
    // Position où le point est dessiné
    pos: vec3<f32>,
    // Rotation (axe × angle) qui amène le repère local vrai du point sur son repère dessiné
    rot: vec3<f32>,
};

// Redessine `p` sur un globe k fois plus grand, tangent à la vraie planète sous la caméra.
// L'angle au centre est réduit de façon à garder les distances le long du sol à toutes les
// altitudes, et l'altitude est conservée : seule la courbure au loin diminue. Le déplacement
// n'entre en jeu qu'au-delà de la portée de visée (flatten.z), où il fait encore moins d'un
// dixième de bloc : à portée de bras le dessin est exactement la vraie géométrie, celle de
// l'éclairage, des ombres, de la visée et des collisions.
fn flatten_point(p: vec3<f32>) -> FlatPoint {
    var out: FlatPoint;
    out.pos = p;
    out.rot = vec3<f32>(0.0);
    let extra = camera.flatten.x;
    if (extra <= 0.0) {
        return out;
    }
    let up = normalize(camera.camera_pos);
    let r = length(p);
    let d = p / max(r, 1e-6);
    let c = dot(up, d);
    let side = d - up * c;
    let s = length(side);
    if (s < 1e-7) {
        return out;
    }
    let t = side / s;
    let theta = atan2(s, c);
    let rho = r + extra;
    // Autour des antipodes, que le grand globe ne laisse jamais voir, les grands triangles du
    // relief lointain se retourneraient : ils sont rabattus au centre du grand globe, sous sa
    // surface
    if (theta > 0.8 * 3.14159265) {
        out.pos = -up * extra;
        return out;
    }
    let theta2 = theta * r / rho;
    // Centre du grand globe en -extra le long de `up` : ρ cos θ' - extra = r - 2ρ sin²(θ'/2),
    // écrit sans soustraire deux grands nombres
    let half = sin(0.5 * theta2);
    let flat = up * (r - 2.0 * rho * half * half) + t * (rho * sin(theta2));
    let w = smoothstep(camera.flatten.z, camera.flatten.w, theta * r);
    out.pos = mix(p, flat, w);
    out.rot = cross(up, t) * ((theta2 - theta) * w);
    return out;
}

// Tourne `v` de la rotation `rv` (axe × angle)
fn rotate_by(v: vec3<f32>, rv: vec3<f32>) -> vec3<f32> {
    let a = length(rv);
    if (a < 1e-9) {
        return v;
    }
    let k = rv / a;
    let ca = cos(a);
    return v * ca + cross(k, v) * sin(a) + k * (dot(k, v) * (1.0 - ca));
}

// Le ciel, la lumière du soleil et l'ambiance du jour viennent de l'atmosphère physique
// (atmosphere.wgsl). La scène est rendue en Rgba16Float puis convertie par le tonemapper (AgX par
// défaut) : seul le disque solaire, assez intense pour un bloom marqué, garde une échelle à part.
const HDR_SUN_DISK: f32 = 6.0;

// Clarté du jour et facteur de crépuscule vus depuis le sol, selon la hauteur du soleil
struct SkyFactors {
    daylight: f32,
    sunset: f32,
};

fn sky_factors(sun_up: f32) -> SkyFactors {
    var f: SkyFactors;
    // Transition naturelle vers la nuit dès que le soleil passe sous l'horizon
    f.daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);
    // Soleil rasant l'horizon (-0.05 à +0.20)
    let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
    f.sunset = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);
    return f;
}

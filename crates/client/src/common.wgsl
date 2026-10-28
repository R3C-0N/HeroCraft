// =========================================================================
// HeroCraft - Module WGSL partagé (naga_oil)
// Uniforme caméra et orbite lunaire communs au ciel, au terrain et aux nuages : une seule
// définition garantit que la position de la Lune reste cohérente.
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
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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

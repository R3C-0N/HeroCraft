// =========================================================================
// HeroCraft - Atmosphère physique (module WGSL partagé, naga_oil)
// Modèle de Hillaire (« A Scalable and Production Ready Sky and Atmosphere Rendering
// Technique », EGSR 2020) : diffusion de Rayleigh (air), de Mie (brume) et absorption de
// l'ozone, avec trois tables précalculées (atmosphere.rs) :
// - transmittance du point vers le haut de l'atmosphère, selon l'altitude et l'angle ;
// - diffusion multiple (lumière rediffusée plusieurs fois, approximée comme isotrope) ;
// - irradiance du ciel reçue par une surface (dessus, flanc côté soleil, flanc opposé).
// Les distances sont en unités du monde, centre de la planète à l'origine.
//
// Courbure virtuelle : sur une planète aussi petite, le trajet rasant de la lumière du soleil
// couchant serait trop court pour rougir. Les tables sont donc calculées pour une planète plus
// grande (même atmosphère, rayon `sky.y`) et lues selon l'altitude et l'angle local du soleil.
// Les rayons de vue suivent, eux, la vraie courbure (limbe correct depuis l'orbite).
//
// Unité des couleurs : celle du shader du terrain, où une surface blanche éclairée de face
// par un soleil d'éclairement E vaut E. Une radiance physique L y vaut donc π L.
// =========================================================================
#define_import_path herocraft::atmosphere

struct Atmosphere {
    rayleigh_scattering: vec3<f32>,
    bottom_radius: f32,
    mie_scattering: vec3<f32>,
    top_radius: f32,
    mie_extinction: vec3<f32>,
    // Inverse de la hauteur d'échelle de Rayleigh (densité exp(-h * k))
    rayleigh_exp_scale: f32,
    absorption_extinction: vec3<f32>,
    mie_exp_scale: f32,
    ground_albedo: vec3<f32>,
    mie_g: f32,
    // Éclairement du soleil, blanc au zénith au niveau de la mer (curseur Soleil inclus)
    sun_illuminance: vec3<f32>,
    ozone_center: f32,
    // x : demi-épaisseur de la couche d'ozone, y : intensité des rayons crépusculaires,
    // z : distance de marche des rayons crépusculaires, w : altitude du bas des nuages
    params: vec4<f32>,
    // x : gain de la lumière du ciel (curseur Jour), y : rayon de la planète virtuelle des tables,
    // z : densité de l'air pour la perspective aérienne des surfaces, w : inutilisé
    sky: vec4<f32>,
};

@group(0) @binding(1) var<uniform> atmosphere: Atmosphere;
@group(0) @binding(2) var transmittance_lut: texture_2d<f32>;
@group(0) @binding(3) var multiscatter_lut: texture_2d<f32>;
@group(0) @binding(4) var irradiance_lut: texture_2d<f32>;
@group(0) @binding(5) var atmosphere_sampler: sampler;

const PI: f32 = 3.14159265;
// Éclairement de référence du soleil (atmosphere.rs) : le disque solaire est réglé pour lui
const SUN_REFERENCE: f32 = 2.5;
// Gain de la lumière diffusée par l'air (ciel et perspective aérienne) : ciel plus lumineux que
// le seul modèle physique, AgX assombrissant les bleus saturés
const SKY_RADIANCE_GAIN: f32 = 1.35;
const TRANSMITTANCE_LUT_SIZE: vec2<f32> = vec2<f32>(256.0, 64.0);
const MULTISCATTER_LUT_SIZE: f32 = 32.0;
const IRRADIANCE_LUT_SIZE: f32 = 32.0;

fn thickness() -> f32 {
    return atmosphere.top_radius - atmosphere.bottom_radius;
}

// Rayons du sol et du sommet de l'atmosphère de la planète virtuelle des tables
fn lut_bottom() -> f32 {
    return atmosphere.sky.y;
}

fn lut_top() -> f32 {
    return atmosphere.sky.y + thickness();
}

struct Medium {
    rayleigh: vec3<f32>,
    mie: vec3<f32>,
    extinction: vec3<f32>,
};

// Coefficients de l'air à l'altitude `h` au-dessus du niveau de la mer
fn sample_medium(h_in: f32) -> Medium {
    let h = max(h_in, 0.0);
    let rayleigh_density = exp(-h * atmosphere.rayleigh_exp_scale);
    let mie_density = exp(-h * atmosphere.mie_exp_scale);
    let ozone_density = max(0.0, 1.0 - abs(h - atmosphere.ozone_center) / atmosphere.params.x);
    var m: Medium;
    m.rayleigh = atmosphere.rayleigh_scattering * rayleigh_density;
    m.mie = atmosphere.mie_scattering * mie_density;
    m.extinction = m.rayleigh + atmosphere.mie_extinction * mie_density + atmosphere.absorption_extinction * ozone_density;
    return m;
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

// Cornette-Shanks : Henyey-Greenstein corrigé, pic avant marqué autour du soleil
fn mie_phase(cos_theta: f32) -> f32 {
    let g = atmosphere.mie_g;
    let g2 = g * g;
    let denom = max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4);
    return 3.0 / (8.0 * PI) * (1.0 - g2) * (1.0 + cos_theta * cos_theta) / ((2.0 + g2) * denom * sqrt(denom));
}

// Intersections d'un rayon avec la sphère de rayon `radius` centrée à l'origine (t_near, t_far),
// (-1, -1) sans intersection
fn ray_sphere(origin: vec3<f32>, dir: vec3<f32>, radius: f32) -> vec2<f32> {
    let b = dot(origin, dir);
    let c = dot(origin, origin) - radius * radius;
    let disc = b * b - c;
    if (disc < 0.0) {
        return vec2<f32>(-1.0);
    }
    let s = sqrt(disc);
    return vec2<f32>(-b - s, -b + s);
}

// --- Table de transmittance (paramétrage de Bruneton, planète virtuelle) ---

fn unit_to_texel(x: f32, size: f32) -> f32 {
    return 0.5 / size + x * (1.0 - 1.0 / size);
}

fn texel_to_unit(u: f32, size: f32) -> f32 {
    return (u - 0.5 / size) / (1.0 - 1.0 / size);
}

fn transmittance_uv(h: f32, mu: f32) -> vec2<f32> {
    let rb = lut_bottom();
    let rt = lut_top();
    let r = rb + clamp(h, 0.0, thickness());
    let big_h = sqrt(max(rt * rt - rb * rb, 0.0));
    let rho = sqrt(max(r * r - rb * rb, 0.0));
    let disc = r * r * (mu * mu - 1.0) + rt * rt;
    let d = max(-r * mu + sqrt(max(disc, 0.0)), 0.0);
    let d_min = rt - r;
    let d_max = rho + big_h;
    let x_mu = (d - d_min) / max(d_max - d_min, 1e-6);
    let x_r = rho / big_h;
    return vec2<f32>(unit_to_texel(x_mu, TRANSMITTANCE_LUT_SIZE.x), unit_to_texel(x_r, TRANSMITTANCE_LUT_SIZE.y));
}

// Inverse de transmittance_uv : (rayon sur la planète virtuelle, mu) du texel
fn transmittance_params(uv: vec2<f32>) -> vec2<f32> {
    let rb = lut_bottom();
    let rt = lut_top();
    let x_mu = texel_to_unit(uv.x, TRANSMITTANCE_LUT_SIZE.x);
    let x_r = texel_to_unit(uv.y, TRANSMITTANCE_LUT_SIZE.y);
    let big_h = sqrt(max(rt * rt - rb * rb, 0.0));
    let rho = big_h * x_r;
    let r = sqrt(rho * rho + rb * rb);
    let d_min = rt - r;
    let d_max = rho + big_h;
    let d = d_min + x_mu * (d_max - d_min);
    var mu = 1.0;
    if (d > 0.0) {
        mu = clamp((big_h * big_h - rho * rho - d * d) / (2.0 * r * d), -1.0, 1.0);
    }
    return vec2<f32>(r, mu);
}

// Transmittance du point (altitude h, cosinus zénithal mu) jusqu'au haut de l'atmosphère
fn transmittance_to_top(h: f32, mu: f32) -> vec3<f32> {
    return textureSampleLevel(transmittance_lut, atmosphere_sampler, transmittance_uv(h, mu), 0.0).rgb;
}

// Part du disque solaire au-dessus de l'horizon du point (ombre de la planète virtuelle)
fn planet_shadow(h: f32, mu_s: f32) -> f32 {
    let sin_h = lut_bottom() / (lut_bottom() + max(h, 0.0));
    let mu_horizon = -sqrt(max(1.0 - sin_h * sin_h, 0.0));
    return smoothstep(-0.01, 0.01, mu_s - mu_horizon);
}

// Lumière du soleil qui atteint le point `p`, sans les ombres du relief (unité : éclairement)
fn sun_light_at(p: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let r = length(p);
    let h = r - atmosphere.bottom_radius;
    let mu_s = dot(p / max(r, 1e-6), sun_dir);
    return atmosphere.sun_illuminance * transmittance_to_top(h, mu_s) * planet_shadow(h, mu_s);
}

// --- Tables de diffusion multiple et d'irradiance : (cosinus zénithal du soleil, altitude) ---

fn altitude_uv(h: f32, mu_s: f32, size: f32) -> vec2<f32> {
    let x_r = clamp(h / thickness(), 0.0, 1.0);
    return vec2<f32>(unit_to_texel(mu_s * 0.5 + 0.5, size), unit_to_texel(x_r, size));
}

fn multiscatter_at(h: f32, mu_s: f32) -> vec3<f32> {
    return textureSampleLevel(multiscatter_lut, atmosphere_sampler, altitude_uv(h, mu_s, MULTISCATTER_LUT_SIZE), 0.0).rgb;
}

// Irradiance du ciel (soleil exclu) reçue par une surface de normale `n` au point `p`, dans
// l'unité du shader (éclairement du soleil et gain du ciel inclus)
fn sky_light(p: vec3<f32>, n: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    return sky_irradiance(p, n, sun_dir) * atmosphere.sun_illuminance * atmosphere.sky.x;
}

// Irradiance du ciel (soleil exclu) reçue par une surface de normale `n` au point `p`, pour un
// éclairement solaire unitaire. Trois orientations précalculées : dessus, flanc tourné vers le
// soleil, flanc opposé. Les faces tournées vers le bas reçoivent le rebond du sol.
fn sky_irradiance(p: vec3<f32>, n: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let r = length(p);
    let up = p / max(r, 1e-6);
    let mu_s = dot(up, sun_dir);
    let uv = altitude_uv(r - atmosphere.bottom_radius, mu_s, IRRADIANCE_LUT_SIZE);
    // Les trois orientations sont rangées côte à côte (largeur 3 x 32)
    let u = vec3<f32>(uv.x, uv.x + 1.0, uv.x + 2.0) / 3.0;
    let e_up = textureSampleLevel(irradiance_lut, atmosphere_sampler, vec2<f32>(u.x, uv.y), 0.0).rgb;
    let e_sun = textureSampleLevel(irradiance_lut, atmosphere_sampler, vec2<f32>(u.y, uv.y), 0.0).rgb;
    let e_away = textureSampleLevel(irradiance_lut, atmosphere_sampler, vec2<f32>(u.z, uv.y), 0.0).rgb;

    let n_up = dot(n, up);
    let sun_h = sun_dir - up * mu_s;
    let n_h = n - up * n_up;
    let side_t = select(0.5, dot(normalize(n_h), normalize(sun_h)) * 0.5 + 0.5, length(sun_h) > 1e-4 && length(n_h) > 1e-4);
    let side = mix(e_away, e_sun, side_t);
    let down = (e_up + e_sun + e_away) * (atmosphere.ground_albedo * 0.25);
    return select(mix(side, down, -n_up), mix(side, e_up, n_up), n_up >= 0.0);
}

// --- Diffusion le long d'un rayon ---

struct Scattering {
    // Lumière diffusée vers l'observateur (unité du shader, éclairement solaire inclus)
    inscatter: vec3<f32>,
    // Transmittance de l'observateur au bout du segment
    transmittance: vec3<f32>,
};

// Diffusion simple du soleil et diffusion multiple (table) entre `origin` et
// `origin + dir * t_max` dans la vraie géométrie, coupée par l'atmosphère et le niveau de la
// mer, pour un éclairement solaire unitaire (radiance physique). `density` multiplie la densité
// de l'air. Pas plus serrés près de l'observateur (répartition quadratique), où la densité
// varie le plus vite.
fn integrate_scattering_unit(origin: vec3<f32>, dir: vec3<f32>, t_max: f32, sun_dir: vec3<f32>, steps: i32, density: f32) -> Scattering {
    var result: Scattering;
    result.inscatter = vec3<f32>(0.0);
    result.transmittance = vec3<f32>(1.0);

    let top = ray_sphere(origin, dir, atmosphere.top_radius);
    if (top.y <= 0.0) {
        return result;
    }
    let t_start = max(top.x, 0.0);
    var t_end = min(top.y, t_max);
    let ground = ray_sphere(origin, dir, atmosphere.bottom_radius);
    if (ground.x > 0.0) {
        t_end = min(t_end, ground.x);
    }
    if (t_end <= t_start) {
        return result;
    }

    let cos_theta = dot(dir, sun_dir);
    let phase_r = rayleigh_phase(cos_theta);
    let phase_m = mie_phase(cos_theta);
    let length_total = t_end - t_start;
    let n = f32(steps);

    var t_prev = 0.0;
    for (var i = 0; i < steps; i = i + 1) {
        let x1 = (f32(i) + 1.0) / n;
        let t_next = x1 * x1 * length_total;
        let dt = t_next - t_prev;
        let p = origin + dir * (t_start + (t_prev + t_next) * 0.5);
        t_prev = t_next;

        let r = length(p);
        let h = r - atmosphere.bottom_radius;
        let mu_s = dot(p / r, sun_dir);
        let medium = sample_medium(h);
        let sun_t = transmittance_to_top(h, mu_s) * planet_shadow(h, mu_s);
        let ms = multiscatter_at(h, mu_s);
        let s = (medium.rayleigh * (phase_r * sun_t + ms) + medium.mie * (phase_m * sun_t + ms)) * density;
        let extinction = medium.extinction * density;

        // Intégration analytique sur le pas (Hillaire) : stable même pour un pas épais
        let step_t = exp(-extinction * dt);
        let s_int = (s - s * step_t) / max(extinction, vec3<f32>(1e-7));
        result.inscatter += result.transmittance * s_int;
        result.transmittance *= step_t;
    }
    return result;
}

// Ciel : même calcul dans l'unité du shader, éclairement du soleil inclus
fn integrate_scattering(origin: vec3<f32>, dir: vec3<f32>, t_max: f32, sun_dir: vec3<f32>, steps: i32) -> Scattering {
    var result = integrate_scattering_unit(origin, dir, t_max, sun_dir, steps, 1.0);
    result.inscatter *= atmosphere.sun_illuminance * (PI * SKY_RADIANCE_GAIN * atmosphere.sky.x);
    return result;
}

// Perspective aérienne d'une surface à `dist` de la caméra. Densité d'air réduite (comme
// l'échelle de distance de la perspective aérienne d'Unreal) : sur une planète aussi petite,
// le relief à quelques blocs serait sinon déjà noyé dans la brume.
fn aerial_perspective(origin: vec3<f32>, dir: vec3<f32>, dist: f32, sun_dir: vec3<f32>, steps: i32) -> Scattering {
    var result = integrate_scattering_unit(origin, dir, dist, sun_dir, steps, atmosphere.sky.z);
    result.inscatter *= atmosphere.sun_illuminance * (PI * SKY_RADIANCE_GAIN * atmosphere.sky.x);
    return result;
}

// Applique la perspective aérienne à une couleur vue à travers `scattering`
fn apply_aerial_perspective(color: vec3<f32>, scattering: Scattering) -> vec3<f32> {
    return color * scattering.transmittance + scattering.inscatter;
}

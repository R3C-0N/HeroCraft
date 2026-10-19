// =========================================================================
// HeroCraft - Sky & Space Atmosphere Shader (WGSL)
// - Ciel étoilé 3D procédural avec scintillement temporel
// - Soleil éclatant avec disque solaire, assombrissement de limbe et couronne
// - Halo atmosphérique externe planétaire (limbe spatial)
// - Diffusion atmosphérique au sol (dôme céleste azur et horizon doré)
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

struct SkyVertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyVertexOutput {
    var out: SkyVertexOutput;
    // Triangle plein écran en coordonnées NDC couvrant tout l'écran
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    let p = pos[vertex_index];
    out.clip_pos = vec4<f32>(p.x, p.y, 1.0, 1.0); // Z = 1.0 (fond infini)
    out.ndc = p;
    return out;
}

// Fonction de hachage 3D pour la répartition spatiale pseudo-aléatoire des étoiles
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += vec3<f32>(dot(q, q.yxz + vec3<f32>(33.33)));
    return fract((q.xxy + q.yxx) * q.zyx);
}

// Générateur de champ d'étoiles 3D avec scintillement
fn render_starfield(ray_dir: vec3<f32>, time: f32) -> vec3<f32> {
    var stars = vec3<f32>(0.0);

    // Couche 1 : Étoiles principales lumineuses
    let scale1 = 180.0;
    let p1 = ray_dir * scale1;
    let cell1 = floor(p1);
    let f1 = fract(p1);
    let h1 = hash33(cell1);
    if (h1.x > 0.88) {
        let center = h1 * 0.75 + vec3<f32>(0.125);
        let d = length(f1 - center);
        let brightness = (h1.x - 0.88) / 0.12;
        let twinkle = sin(time * 3.5 + h1.y * 50.0) * 0.35 + 0.65;
        let intensity = exp(-d * 32.0) * brightness * twinkle;
        let star_color = mix(vec3<f32>(0.75, 0.90, 1.0), vec3<f32>(1.0, 0.88, 0.72), h1.z);
        stars += star_color * intensity * 2.2;
    }

    // Couche 2 : Voile dense d'étoiles lointaines plus fines
    let scale2 = 280.0;
    let p2 = ray_dir * scale2;
    let cell2 = floor(p2);
    let f2 = fract(p2);
    let h2 = hash33(cell2);
    if (h2.x > 0.92) {
        let center2 = h2 * 0.7 + vec3<f32>(0.15);
        let d2 = length(f2 - center2);
        let intensity2 = exp(-d2 * 45.0) * (h2.x - 0.92) / 0.08;
        stars += vec3<f32>(0.85, 0.92, 1.0) * intensity2 * 1.2;
    }

    // Nébuleuse spatiale subtile (poussière stellaire galactique)
    let galaxy = pow(clamp(1.0 - abs(ray_dir.y * 0.75 + ray_dir.x * 0.4), 0.0, 1.0), 3.5);
    stars += vec3<f32>(0.02, 0.035, 0.08) * galaxy;

    return stars;
}

// Structure représentant la position et la phase de la Lune
struct MoonData {
    dir: vec3<f32>,     // Vecteur unitaire orienté vers la Lune
    phase: f32,         // 0.0 = Nouvelle Lune, 0.5 = Quartier, 1.0 = Pleine Lune
};

// Calcule la position orbitale et la phase de la Lune en temps réel
// - Période orbitale : 21 minutes (1260 secondes)
// - Inclinaison orbitale lunaire : ~28.5° (0.497 rad)
// - Cycle synodique complet (retour à la même phase) : 21 jours de jeu (420 minutes)
// - Déphasage initial (+3.69 rad) pour garantir une pleine lune majestueuse dès la première nuit
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

// Matrice de tramage Bayer 4x4 pour supprimer l'effet de bandes (banding) du raymarching volumétrique
fn bayer4x4_sky(coord: vec2<u32>) -> f32 {
    let m = array<f32, 16>(
        0.0 / 16.0,  8.0 / 16.0,  2.0 / 16.0, 10.0 / 16.0,
       12.0 / 16.0,  4.0 / 16.0, 14.0 / 16.0,  6.0 / 16.0,
        3.0 / 16.0, 11.0 / 16.0,  1.0 / 16.0,  9.0 / 16.0,
       15.0 / 16.0,  7.0 / 16.0, 13.0 / 16.0,  5.0 / 16.0
    );
    let index = (coord.y % 4u) * 4u + (coord.x % 4u);
    return m[index];
}

// Générateur pseudo-aléatoire 2D à haute entropie sans singularité en (0,0)
fn hash21_cloud(p: vec2<f32>) -> f32 {
    let p_off = p + vec2<f32>(157.1, 311.7);
    var p3 = fract(vec3<f32>(p_off.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Bruit de valeur 2D avec interpolation cubique (Hermite smoothstep)
fn value_noise_cloud(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21_cloud(i);
    let b = hash21_cloud(i + vec2<f32>(1.0, 0.0));
    let c = hash21_cloud(i + vec2<f32>(0.0, 1.0));
    let d = hash21_cloud(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Détermine si une cellule de la grille voxel contient un bloc de nuage solide
fn is_cell_cloud(cell: vec2<f32>, threshold: f32) -> bool {
    let n1 = value_noise_cloud(cell * 0.16 + vec2<f32>(12.3, 45.7));
    let n2 = value_noise_cloud(cell * 0.36 + vec2<f32>(73.1, 19.8));
    let n_val = n1 * 0.65 + n2 * 0.35;
    return n_val > threshold;
}

// Fonction de distance signée (SDF) 3D d'un nuage solide style Voxel / Minecraft Reimagined :
// - Blocs solides avec faces planes et arêtes franches
// - Biseaux arrondis nets sur les coins verticaux et les bords horizontaux (rounded cuboids)
// - Retourne > 0.0 si le point est à l'intérieur du bloc solide, < 0.0 si dans l'air
fn cloud_solid_sdf(
    p: vec3<f32>,
    east: vec3<f32>,
    north: vec3<f32>,
    up: vec3<f32>,
    time: f32,
    cloud_slider: f32
) -> f32 {
    let r = length(p);
    let r_bot = 34.0;
    let r_top = 35.4;
    if (r < r_bot || r > r_top) {
        return -1.0;
    }
    let h_norm = (r - r_bot) / (r_top - r_bot);

    let x_local = dot(p, east);
    let z_local = dot(p, north);

    // Ancrage géographique sur le globe planétaire (permet de marcher sous les nuages)
    let lat = asin(clamp(up.y, -0.999, 0.999));
    let lon = atan2(up.x, up.z);
    let geo_anchor = vec2<f32>(lon * 34.5 * cos(lat), lat * 34.5);

    // Dérive douce du vent dans le ciel
    let wind = vec2<f32>(time * 0.016, time * 0.005);
    let cell_size = 2.4;
    let uv = (vec2<f32>(x_local, z_local) + geo_anchor) / cell_size + wind;

    let cell = floor(uv);
    let f = fract(uv);

    // Seuil de couverture nuageuse équilibré (~35% de couverture, présent au zénith comme à l'horizon)
    let threshold = mix(0.50, 0.32, clamp(cloud_slider * 0.5, 0.0, 1.0));

    if (!is_cell_cloud(cell, threshold)) {
        return -1.0;
    }

    // Détection des parois extérieures adjacentes
    let wall_w = !is_cell_cloud(cell + vec2<f32>(-1.0, 0.0), threshold);
    let wall_e = !is_cell_cloud(cell + vec2<f32>( 1.0, 0.0), threshold);
    let wall_s = !is_cell_cloud(cell + vec2<f32>(0.0, -1.0), threshold);
    let wall_n = !is_cell_cloud(cell + vec2<f32>(0.0,  1.0), threshold);

    let r_bevel = 0.12; // Rayon d'arrondi net et précis sur les arêtes verticales

    // 1. Découpe des 4 coins verticaux arrondis
    if (wall_w && wall_s) {
        if (f.x < r_bevel && f.y < r_bevel) {
            let d = length(vec2<f32>(r_bevel - f.x, r_bevel - f.y));
            if (d > r_bevel) {
                return -1.0;
            }
        }
    }
    if (wall_e && wall_s) {
        if ((1.0 - f.x) < r_bevel && f.y < r_bevel) {
            let d = length(vec2<f32>(r_bevel - (1.0 - f.x), r_bevel - f.y));
            if (d > r_bevel) {
                return -1.0;
            }
        }
    }
    if (wall_w && wall_n) {
        if (f.x < r_bevel && (1.0 - f.y) < r_bevel) {
            let d = length(vec2<f32>(r_bevel - f.x, r_bevel - (1.0 - f.y)));
            if (d > r_bevel) {
                return -1.0;
            }
        }
    }
    if (wall_e && wall_n) {
        if ((1.0 - f.x) < r_bevel && (1.0 - f.y) < r_bevel) {
            let d = length(vec2<f32>(r_bevel - (1.0 - f.x), r_bevel - (1.0 - f.y)));
            if (d > r_bevel) {
                return -1.0;
            }
        }
    }

    // 2. Découpe des arêtes horizontales arrondies (dessus et dessous du nuage)
    let r_y = 0.12; // Rayon d'arrondi sur les bords horizontaux
    var d_edge = 1.0;
    if (wall_w) { d_edge = min(d_edge, f.x); }
    if (wall_e) { d_edge = min(d_edge, 1.0 - f.x); }
    if (wall_s) { d_edge = min(d_edge, f.y); }
    if (wall_n) { d_edge = min(d_edge, 1.0 - f.y); }

    if (d_edge < r_y) {
        // Bord inférieur
        if (h_norm < r_y) {
            let d = length(vec2<f32>(r_y - d_edge, r_y - h_norm));
            if (d > r_y) {
                return -1.0;
            }
        }
        // Bord supérieur
        if ((1.0 - h_norm) < r_y) {
            let d = length(vec2<f32>(r_y - d_edge, r_y - (1.0 - h_norm)));
            if (d > r_y) {
                return -1.0;
            }
        }
    }

    return 1.0;
}

// Calcule la normale extérieure à la surface solide du nuage par gradient numérique
fn get_solid_cloud_normal(
    p: vec3<f32>,
    east: vec3<f32>,
    north: vec3<f32>,
    up: vec3<f32>,
    time: f32,
    cloud_slider: f32
) -> vec3<f32> {
    let eps = 0.05;
    let dx = cloud_solid_sdf(p + east * eps, east, north, up, time, cloud_slider)
           - cloud_solid_sdf(p - east * eps, east, north, up, time, cloud_slider);
    let dz = cloud_solid_sdf(p + north * eps, east, north, up, time, cloud_slider)
           - cloud_solid_sdf(p - north * eps, east, north, up, time, cloud_slider);
    let dy = cloud_solid_sdf(p + up * eps, east, north, up, time, cloud_slider)
           - cloud_solid_sdf(p - up * eps, east, north, up, time, cloud_slider);
    let grad = east * dx + north * dz + up * dy;
    let len = length(grad);
    return select(up, -grad / len, len > 1e-4);
}

// Raymarching de surface solide pour des nuages cubiques nets et bien définis
fn render_volumetric_clouds(
    ray_dir: vec3<f32>,
    o: vec3<f32>,
    up: vec3<f32>,
    l_sun: vec3<f32>,
    time: f32,
    cloud_slider: f32,
    sunset_factor: f32,
    daylight: f32,
    moon: MoonData,
    moon_slider: f32,
    sun_slider: f32,
    day_amb_slider: f32,
    night_amb_slider: f32,
    screen_coord: vec2<u32>
) -> vec4<f32> {
    if (cloud_slider < 0.02) {
        return vec4<f32>(0.0);
    }

    let ray_up = dot(ray_dir, up);
    if (ray_up < 0.005) {
        return vec4<f32>(0.0);
    }

    let r_bot = 34.0;
    let r_top = 35.4;
    let r_cam = length(o);

    // Calcul de l'intersection du rayon avec les sphères délimitant la couche nuageuse
    let b = dot(o, ray_dir);
    let d2_bot = b * b - (r_cam * r_cam - r_bot * r_bot);
    let d2_top = b * b - (r_cam * r_cam - r_top * r_top);

    if (d2_top <= 0.0) {
        return vec4<f32>(0.0);
    }

    var t_enter = 0.0;
    if (d2_bot > 0.0) {
        t_enter = max(-b + sqrt(d2_bot), 0.0);
    }
    let t_exit = -b + sqrt(d2_top);

    if (t_exit <= t_enter || t_exit <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Base orthonormale sur le plan tangentiel local de la planète
    let ref_axis = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(up.y) > 0.9);
    let east = normalize(cross(up, ref_axis));
    let north = cross(up, east);

    let num_steps = 22u;
    let dt = (t_exit - t_enter) / f32(num_steps);
    let dither = bayer4x4_sky(screen_coord);
    var t = t_enter + dt * (dither * 0.75 + 0.125);

    var hit = false;
    var hit_t = 0.0;

    for (var i = 0u; i < num_steps; i = i + 1u) {
        let p = o + ray_dir * t;
        if (cloud_solid_sdf(p, east, north, up, time, cloud_slider) > 0.0) {
            // Impact détecté sur la surface solide du nuage !
            // Raffinement dichotomique précis en 4 sous-étapes pour un contour net et chirurgical
            var t_a = t - dt;
            var t_b = t;
            for (var j = 0u; j < 4u; j = j + 1u) {
                let t_m = (t_a + t_b) * 0.5;
                if (cloud_solid_sdf(o + ray_dir * t_m, east, north, up, time, cloud_slider) > 0.0) {
                    t_b = t_m;
                } else {
                    t_a = t_m;
                }
            }
            hit_t = t_b;
            hit = true;
            break;
        }
        t += dt;
    }

    if (!hit) {
        return vec4<f32>(0.0);
    }

    let hit_pos = o + ray_dir * hit_t;
    let norm = get_solid_cloud_normal(hit_pos, east, north, up, time, cloud_slider);

    // Éclairage direct du soleil sur la surface du bloc solide
    let ndotl = max(dot(norm, l_sun), 0.0);
    let direct_sun = ndotl * (1.20 + 0.40 * sunset_factor);

    // Halo d'éblouissement / contre-jour doré sur les arêtes (Mie rim)
    let forward = max(dot(ray_dir, l_sun), 0.0);
    let rim = pow(forward, 6.0) * (0.65 + 0.90 * sunset_factor);

    let sun_highlight = mix(vec3<f32>(1.0, 0.98, 0.92), vec3<f32>(1.0, 0.82, 0.48), sunset_factor);
    let direct_light = sun_highlight * (direct_sun + rim * 0.55) * daylight * sun_slider;

    // Ambiance du ciel sur les faces ombragées (teinte lavande / violet crépusculaire de la photo)
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

    // Ombrage ambiant doux (faces inférieures bien éclairées par l'ambiance du ciel)
    let ao = 0.78 + 0.22 * clamp(ndotup * 0.5 + 0.5, 0.0, 1.0);
    let cloud_color = (direct_light + ambient_light + moon_light + night_ambient) * ao;

    // Estompage doux à l'extrême horizon uniquement
    let horizon_fade = smoothstep(0.015, 0.08, ray_up);

    // Le nuage est 100% SOLIDE et opaque (alpha = 1.0) !
    return vec4<f32>(cloud_color, horizon_fade);
}

@fragment
fn fs_sky(in: SkyVertexOutput) -> @location(0) vec4<f32> {
    // Calcul de la direction du rayon dans le repère monde depuis la matrice inverse
    let world_far_h = camera.inv_view_proj * vec4<f32>(in.ndc.x, in.ndc.y, 1.0, 1.0);
    let world_far = world_far_h.xyz / world_far_h.w;
    let ray_dir = normalize(world_far - camera.camera_pos);

    let l = normalize(camera.light_dir);
    let cos_sun = dot(ray_dir, l);

    // Multiplicateurs d'intensité interactifs pilotés par les sliders de l'interface
    let moon_slider = camera.lighting_params.x;      // Multiplicateur Lune [0.0 .. 3.0]
    let sun_slider = camera.lighting_params.y;       // Multiplicateur Soleil [0.0 .. 3.0]
    let night_amb_slider = camera.lighting_params.z; // Multiplicateur Nuit / Étoiles [0.0 .. 3.0]
    let day_amb_slider = camera.lighting_params.w;   // Multiplicateur Jour / Ciel [0.0 .. 3.0]
    let cloud_slider = camera.lighting_params2.z;    // Multiplicateur Nuages Volumétriques [0.0 .. 2.0]

    // Données de la Lune orbitale
    let moon = get_moon_data(camera.time, camera.light_dir);
    let cos_moon = dot(ray_dir, moon.dir);
    let moon_radius_cos = 0.9988; // ~2.8° de diamètre apparent
    let is_moon_disk = cos_moon > moon_radius_cos;

    // 1. Soleil (Disque lumineux + Couronne d'éblouissement)
    var sun_color = vec3<f32>(0.0);
    let sun_radius_cos = 0.9992; // ~2.3° de diamètre apparent
    if (cos_sun > sun_radius_cos && !is_moon_disk) {
        // Disque solaire avec assombrissement de limbe (occultable lors d'une éclipse par la Lune)
        let limb = clamp((cos_sun - sun_radius_cos) / (1.0 - sun_radius_cos), 0.0, 1.0);
        let sun_core = mix(vec3<f32>(1.0, 0.85, 0.5), vec3<f32>(1.0, 1.0, 0.95), pow(limb, 0.5));
        sun_color += sun_core * 4.5;
    }
    // Couronne solaire chaude et rayonnement externe
    let corona = pow(clamp(cos_sun, 0.0, 1.0), 128.0) * 1.8;
    let wide_glow = pow(clamp(cos_sun, 0.0, 1.0), 16.0) * 0.35;
    sun_color = (sun_color + vec3<f32>(1.0, 0.82, 0.55) * corona + vec3<f32>(0.8, 0.5, 0.25) * wide_glow) * sun_slider;

    // 2. Lune (Disque sphérique 3D, phases lumineuses, cratères, mers et lumière cendrée)
    var moon_disk_color = vec3<f32>(0.0);
    if (is_moon_disk) {
        let rho = sqrt(clamp((1.0 - cos_moon) / (1.0 - moon_radius_cos), 0.0, 1.0));
        let z_local = sqrt(max(1.0 - rho * rho, 0.0));
        let v_tangent = ray_dir - moon.dir * cos_moon;
        let len_tangent = length(v_tangent);
        let d_tangent = select(vec3<f32>(0.0), v_tangent / max(len_tangent, 1e-6), len_tangent > 1e-6);

        // Normale sphérique extérieure de la surface de la Lune
        let n_moon = normalize(d_tangent * rho - moon.dir * z_local);

        // Éclairage direct par le Soleil sur la surface de la Lune
        let sun_incidence = dot(n_moon, l);
        let smooth_terminator = smoothstep(-0.04, 0.08, sun_incidence);
        let direct_lunar_light = smooth_terminator * pow(clamp(sun_incidence + 0.08, 0.0, 1.0), 0.6);

        // Lumière cendrée (Earthshine) éclairant la face non-ensoleillée de la Lune
        let earthshine = 0.040 * (1.0 - moon.phase) * (1.0 - smooth_terminator);
        let lunar_illumination = direct_lunar_light + earthshine;

        // Texture procédurale : Mers lunaires (Maria) et cratères
        let p_crater = n_moon * 4.2;
        let h_crater = fract(sin(dot(p_crater, vec3<f32>(12.9898, 78.233, 45.164))) * 43758.5453);
        let maria = smoothstep(0.30, 0.65, sin(n_moon.x * 2.8 + 0.4) * cos(n_moon.y * 3.2 + 0.3) + sin(n_moon.z * 3.0) * 0.25);
        let maria_albedo = mix(0.62, 0.98, maria);
        let crater_albedo = smoothstep(0.72, 0.92, h_crater) * 0.12;
        let moon_albedo = (maria_albedo + crater_albedo) * vec3<f32>(0.92, 0.95, 1.0);

        let limb_darkening = pow(z_local, 0.25);
        moon_disk_color = moon_albedo * lunar_illumination * (0.85 + 0.15 * limb_darkening) * 3.0;
    }

    // Halo / aura argentée autour de la Lune
    let moon_corona = pow(clamp(cos_moon, 0.0, 1.0), 128.0) * 0.25 * moon.phase;
    let moon_wide_glow = pow(clamp(cos_moon, 0.0, 1.0), 24.0) * 0.06 * moon.phase;
    let moon_aura = vec3<f32>(0.70, 0.82, 1.0) * (moon_corona + moon_wide_glow);
    let moon_space_color = (select(vec3<f32>(0.0), moon_disk_color, is_moon_disk) + moon_aura) * moon_slider;

    // 3. Ciel étoilé dans le vide spatial (étoiles masquées par le disque opaque de la Lune)
    let stars = render_starfield(ray_dir, camera.time) * night_amb_slider;
    let visible_stars = select(stars, vec3<f32>(0.0), is_moon_disk);
    var space_color = visible_stars + sun_color + moon_space_color;

    // Facteur d'altitude : transition fluide entre vue au sol (< 38.0) et orbite spatiale (> 58.0)
    let cam_dist = length(camera.camera_pos);
    let ground_factor = clamp((58.0 - cam_dist) / 20.0, 0.0, 1.0);
    let orbit_factor = 1.0 - ground_factor;

    // 4. Halo atmosphérique externe planétaire (vu depuis l'orbite spatiale uniquement)
    if (orbit_factor > 0.001) {
        let r_atm_inner = 28.5; // Surface moyenne océanique / continentale
        let r_atm_outer = 39.5; // Sommet de l'atmosphère visible depuis l'espace

        let o = camera.camera_pos;
        let b = dot(o, ray_dir);

        if (b < 0.0) { // Le rayon se dirige vers la planète
            let closest_dist = length(o - ray_dir * b);
            if (closest_dist > r_atm_inner && closest_dist < r_atm_outer) {
                let t_atm = (closest_dist - r_atm_inner) / (r_atm_outer - r_atm_inner);
                let density = pow(1.0 - t_atm, 2.2);

                let p_limb = normalize(o - ray_dir * b);
                let limb_sun = clamp(dot(p_limb, l) * 1.5 + 0.3, 0.0, 1.0);
                let terminator = clamp(1.0 - abs(dot(p_limb, l)) * 2.5, 0.0, 1.0);

                let atm_hue = mix(vec3<f32>(0.22, 0.65, 1.0), vec3<f32>(1.0, 0.55, 0.22), terminator * 0.85);
                let limb_glow = atm_hue * (density * limb_sun * 2.5);

                space_color += limb_glow * orbit_factor;
            }
        }
    }

    // 5. Diffusion atmosphérique au sol (Dôme céleste bleu azur en journée, crépuscule doré, nuit étoilée)
    if (ground_factor > 0.001) {
        let cam_up = normalize(camera.camera_pos);
        let ray_up = dot(ray_dir, cam_up);
        let sun_up = dot(l, cam_up);
        let moon_up = dot(moon.dir, cam_up);

        // Clarté du jour : transition naturelle vers la nuit noire dès que le soleil passe sous l'horizon
        let daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);

        // Facteur de crépuscule / coucher de soleil (soleil rasant l'horizon : -0.05 à +0.20)
        let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
        let sunset_factor = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);

        // Dégradé de couleurs du zénith
        let zenith_day = vec3<f32>(0.15, 0.45, 0.88); // Bleu azur pur éclatant
        let zenith_sunset = vec3<f32>(0.07, 0.13, 0.38); // Bleu crépusculaire profond / saphir
        let zenith_color = mix(zenith_day, zenith_sunset, sunset_factor);

        // Dégradé de couleurs de l'horizon
        let horizon_day = vec3<f32>(0.65, 0.80, 0.96); // Horizon clair estompé en journée

        // Horizon au lever/coucher :
        // Vers le soleil : or ambré, jaune incandescent et orange flamboyant
        // À l'opposé du soleil : l'Arche Crépusculaire / Ceinture de Vénus (rose/magenta doux et ombre pourpre)
        let sun_azimuth = dot(ray_dir, l); // -1.0 à l'opposé, +1.0 face au soleil
        let forward_sun = clamp(sun_azimuth * 0.5 + 0.5, 0.0, 1.0);
        let sunset_facing_sun = mix(vec3<f32>(1.0, 0.58, 0.15), vec3<f32>(1.0, 0.82, 0.35), pow(forward_sun, 3.0));
        let sunset_opposite_sun = vec3<f32>(0.65, 0.32, 0.48); // Ceinture de Vénus rosée/violacée
        let sunset_horizon = mix(sunset_opposite_sun, sunset_facing_sun, pow(forward_sun, 1.5));

        let horizon_color = mix(horizon_day, sunset_horizon, sunset_factor);

        // Dégradé vertical du dôme céleste (zénith -> horizon)
        let ray_h = max(ray_up, 0.0);
        let horizon_blend = pow(1.0 - ray_h, 2.5);
        let day_sky = mix(zenith_color, horizon_color, horizon_blend) * day_amb_slider;

        // Éclaircissement et halo solaire diffus au coucher/lever de soleil
        let sunset_glow_color = vec3<f32>(1.0, 0.52, 0.15);
        let day_glow_color = vec3<f32>(0.40, 0.35, 0.22);
        let active_glow_color = mix(day_glow_color, sunset_glow_color, sunset_factor);
        let ground_sun_glow = pow(clamp(cos_sun, 0.0, 1.0), 6.0) * active_glow_color * (1.0 + sunset_factor * 1.8) * sun_slider;

        // Visibilité du soleil au sol : masqué dès qu'il passe sous l'horizon planétaire
        let sun_visibility = clamp(sun_up * 15.0 + 0.5, 0.0, 1.0);
        let sun_disk_tint = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 0.65, 0.25), sunset_factor);

        // Visibilité de la Lune au sol :
        // - Occultée par l'horizon planétaire sous les pieds du joueur
        // - Atténuée en plein jour par la luminosité de l'atmosphère bleue, étincelante la nuit
        let ray_moon_up = dot(ray_dir, cam_up);
        let moon_ground_vis = clamp((moon_up + 0.03) / 0.08, 0.0, 1.0) * clamp((ray_moon_up + 0.015) / 0.035, 0.0, 1.0);
        let daytime_moon_tint = mix(vec3<f32>(1.0), vec3<f32>(0.75, 0.88, 1.0), daylight);
        let ground_moon = moon_space_color * moon_ground_vis * mix(1.0, 0.40, daylight) * daytime_moon_tint;

        // Ciel au sol : nuit étoilée douce et atmosphérique (starlight navy) vs ciel bleu diurne
        let night_sky = visible_stars + vec3<f32>(0.008, 0.012, 0.024) * night_amb_slider;
        let ground_sky_base = mix(night_sky, day_sky, daylight) + sun_color * sun_disk_tint * sun_visibility + ground_sun_glow * daylight + ground_moon;

        // Rendu des nuages volumétriques cubiques arrondis (Style Minecraft / Complementary Reimagined)
        // Les nuages disparaissent totalement en vue satellite (cloud_slider == 0 et ground_factor == 0)
        let clouds = render_volumetric_clouds(
            ray_dir,
            camera.camera_pos,
            cam_up,
            l,
            camera.time,
            cloud_slider * ground_factor,
            sunset_factor,
            daylight,
            moon,
            moon_slider,
            sun_slider,
            day_amb_slider,
            night_amb_slider,
            vec2<u32>(in.clip_pos.xy)
        );

        let ground_sky = mix(ground_sky_base, clouds.rgb, clouds.a);

        let final_sky = mix(space_color, ground_sky, ground_factor);
        return vec4<f32>(final_sky, 1.0);
    }

    return vec4<f32>(space_color, 1.0);
}

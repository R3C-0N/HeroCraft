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

@fragment
fn fs_sky(in: SkyVertexOutput) -> @location(0) vec4<f32> {
    // Calcul de la direction du rayon dans le repère monde depuis la matrice inverse
    let world_far_h = camera.inv_view_proj * vec4<f32>(in.ndc.x, in.ndc.y, 1.0, 1.0);
    let world_far = world_far_h.xyz / world_far_h.w;
    let ray_dir = normalize(world_far - camera.camera_pos);

    let l = normalize(camera.light_dir);
    let cos_sun = dot(ray_dir, l);

    // 1. Soleil (Disque lumineux + Couronne d'éblouissement)
    var sun_color = vec3<f32>(0.0);
    let sun_radius_cos = 0.9992; // ~2.3° de diamètre apparent
    if (cos_sun > sun_radius_cos) {
        // Disque solaire avec assombrissement de limbe
        let limb = clamp((cos_sun - sun_radius_cos) / (1.0 - sun_radius_cos), 0.0, 1.0);
        let sun_core = mix(vec3<f32>(1.0, 0.85, 0.5), vec3<f32>(1.0, 1.0, 0.95), pow(limb, 0.5));
        sun_color += sun_core * 4.5;
    }
    // Couronne solaire chaude et rayonnement externe
    let corona = pow(clamp(cos_sun, 0.0, 1.0), 128.0) * 1.8;
    let wide_glow = pow(clamp(cos_sun, 0.0, 1.0), 16.0) * 0.35;
    sun_color += vec3<f32>(1.0, 0.82, 0.55) * corona + vec3<f32>(0.8, 0.5, 0.25) * wide_glow;

    // 2. Ciel étoilé dans le vide spatial
    let stars = render_starfield(ray_dir, camera.time);
    var space_color = stars + sun_color;

    // Facteur d'altitude : transition fluide entre vue au sol (< 38.0) et orbite spatiale (> 58.0)
    // Le terrain réel avec relief et mers se situe entre les rayons 28.8 (océan) et 38.5 (hauts sommets)
    let cam_dist = length(camera.camera_pos);
    let ground_factor = clamp((58.0 - cam_dist) / 20.0, 0.0, 1.0);
    let orbit_factor = 1.0 - ground_factor;

    // 3. Halo atmosphérique externe planétaire (vu depuis l'orbite spatiale uniquement)
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

    // 4. Diffusion atmosphérique au sol (Dôme céleste bleu azur en journée, crépuscule doré, nuit étoilée)
    if (ground_factor > 0.001) {
        let cam_up = normalize(camera.camera_pos);
        let ray_up = dot(ray_dir, cam_up);
        let sun_up = dot(l, cam_up);

        // Clarté du jour : transition naturelle vers la nuit noire dès que le soleil passe sous l'horizon
        let daylight = clamp((sun_up + 0.05) / 0.16, 0.0, 1.0);

        // Facteur de crépuscule / coucher de soleil (soleil rasant l'horizon)
        let sunset_factor = clamp(1.0 - abs(sun_up) / 0.18, 0.0, 1.0) * daylight;

        // Dégradé de couleurs du zénith
        let zenith_day = vec3<f32>(0.15, 0.45, 0.88); // Bleu azur pur éclatant
        let zenith_sunset = vec3<f32>(0.08, 0.16, 0.42); // Bleu crépusculaire profond
        let zenith_color = mix(zenith_day, zenith_sunset, sunset_factor);

        // Dégradé de couleurs de l'horizon
        let horizon_day = vec3<f32>(0.65, 0.80, 0.96); // Horizon clair estompé
        let sun_azimuth = max(dot(ray_dir, l), 0.0);
        let sunset_warm = mix(vec3<f32>(0.65, 0.42, 0.55), vec3<f32>(1.0, 0.62, 0.22), pow(sun_azimuth, 2.0));
        let horizon_color = mix(horizon_day, sunset_warm, sunset_factor);

        // Dégradé vertical du dôme céleste (zénith -> horizon)
        let ray_h = max(ray_up, 0.0);
        let horizon_blend = pow(1.0 - ray_h, 2.5);
        let day_sky = mix(zenith_color, horizon_color, horizon_blend);

        // Éclaircissement et halo solaire diffus dans l'atmosphère au sol
        let ground_sun_glow = pow(clamp(cos_sun, 0.0, 1.0), 8.0) * vec3<f32>(0.40, 0.35, 0.22);

        // Visibilité du soleil au sol : masqué dès qu'il passe sous l'horizon planétaire
        let sun_visibility = clamp(sun_up * 15.0 + 0.5, 0.0, 1.0);

        // Ciel au sol : nuit étoilée profonde et sombre vs ciel bleu diurne
        let ground_sky = mix(stars, day_sky, daylight) + sun_color * sun_visibility + ground_sun_glow * daylight;

        let final_sky = mix(space_color, ground_sky, ground_factor);
        return vec4<f32>(final_sky, 1.0);
    }

    return vec4<f32>(space_color, 1.0);
}

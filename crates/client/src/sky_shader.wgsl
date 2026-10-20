// =========================================================================
// HeroCraft - Sky & Space Atmosphere Shader (WGSL)
// - Ciel étoilé 3D procédural avec scintillement temporel
// - Soleil éclatant avec disque solaire, assombrissement de limbe et couronne
// - Halo atmosphérique externe planétaire (limbe spatial)
// - Diffusion atmosphérique au sol (dôme céleste azur et horizon doré)
// =========================================================================

#import herocraft::common::{camera, get_moon_data, sky_factors, sky_dome}

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

// Calcule la position orbitale et la phase de la Lune en temps réel
// - Période orbitale : 21 minutes (1260 secondes)
// - Inclinaison orbitale lunaire : ~28.5° (0.497 rad)
// - Cycle synodique complet (retour à la même phase) : 21 jours de jeu (420 minutes)
// - Déphasage initial (+3.69 rad) pour garantir une pleine lune majestueuse dès la première nuit

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
    let cloud_slider = camera.lighting_params_ext.z;    // Multiplicateur Nuages Volumétriques [0.0 .. 2.0]

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

    // Facteur d'altitude : transition fluide entre vue au sol et orbite spatiale
    let scale = camera.planet_radius / 25.0;
    let cam_dist = length(camera.camera_pos);
    let ground_max = 58.0 * scale;
    let ground_fade = 20.0 * scale;
    let alt_ground_factor = clamp((ground_max - cam_dist) / ground_fade, 0.0, 1.0);
    // Utilise en priorité le ground_factor calculé dynamiquement depuis la caméra (lighting_params_ext.w)
    // avec repli sur l'altitude de la caméra pour garantir la visibilité du ciel
    let ground_factor = max(alt_ground_factor, camera.lighting_params_ext.w);
    let orbit_factor = 1.0 - ground_factor;

    // 4. Halo atmosphérique externe planétaire (vu depuis l'orbite spatiale uniquement)
    if (orbit_factor > 0.001) {
        let r_atm_inner = 28.5 * scale; // Surface moyenne océanique / continentale
        let r_atm_outer = 39.5 * scale; // Sommet de l'atmosphère visible depuis l'espace

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

        // Clarté du jour, crépuscule et dôme céleste : formules partagées avec les nuages (common.wgsl)
        let factors = sky_factors(sun_up);
        let daylight = factors.daylight;
        let sunset_factor = factors.sunset;
        let day_sky = sky_dome(ray_dir, cam_up, l, sunset_factor);

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
        let ground_sky_base = mix(night_sky, day_sky, daylight) + sun_color * sun_disk_tint * sun_visibility + ground_moon;

        let final_sky = mix(space_color, ground_sky_base, ground_factor);
        return vec4<f32>(final_sky, 1.0);
    }

    return vec4<f32>(space_color, 1.0);
}

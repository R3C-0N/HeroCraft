// =========================================================================
// HeroCraft - Sky & Space Atmosphere Shader (WGSL)
// - Ciel étoilé 3D procédural avec scintillement temporel
// - Soleil éclatant avec disque solaire, assombrissement de limbe et couronne
// - Atmosphère physique (atmosphere.wgsl) : même calcul au sol et depuis l'orbite, le bleu du
//   jour, le coucher de soleil et le limbe de la planète en découlent
// =========================================================================

#import herocraft::common::{camera, get_moon_data, HDR_SUN_DISK}
#import herocraft::atmosphere::{atmosphere, integrate_scattering, ray_sphere, SUN_REFERENCE}

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
    let night_amb_slider = camera.lighting_params.z; // Multiplicateur Nuit / Étoiles [0.0 .. 3.0]

    // Données de la Lune orbitale
    let moon = get_moon_data(camera.time, camera.light_dir);
    let cos_moon = dot(ray_dir, moon.dir);
    let moon_radius_cos = 0.9988; // ~2.8° de diamètre apparent
    let is_moon_disk = cos_moon > moon_radius_cos;

    // 1. Soleil (disque avec assombrissement du limbe et couronne). Le halo large autour du
    // disque vient de la diffusion de Mie de l'atmosphère.
    var sun_color = vec3<f32>(0.0);
    let sun_radius_cos = 0.9992; // ~2.3° de diamètre apparent
    if (cos_sun > sun_radius_cos && !is_moon_disk) {
        // Disque solaire avec assombrissement de limbe (occultable lors d'une éclipse par la Lune)
        let limb = clamp((cos_sun - sun_radius_cos) / (1.0 - sun_radius_cos), 0.0, 1.0);
        let sun_core = mix(vec3<f32>(1.0, 0.85, 0.5), vec3<f32>(1.0, 1.0, 0.95), pow(limb, 0.5));
        sun_color += sun_core * 4.5;
    }
    let corona = pow(clamp(cos_sun, 0.0, 1.0), 128.0) * 1.8;
    // Éclairement hors atmosphère : le disque est blanc à midi et rougit avec la transmittance
    sun_color = (sun_color + vec3<f32>(1.0, 0.82, 0.55) * corona) * (atmosphere.sun_illuminance / SUN_REFERENCE) * HDR_SUN_DISK;

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
    let moon_color = (select(vec3<f32>(0.0), moon_disk_color, is_moon_disk) + moon_aura) * moon_slider;

    // 3. Atmosphère : lumière diffusée le long du rayon et transmittance vers l'espace
    let scattering = integrate_scattering(camera.camera_pos, ray_dir, 1e9, l, 32);
    // Ce qui est derrière l'atmosphère (soleil, Lune, étoiles) est caché par la planète
    let hits_planet = ray_sphere(camera.camera_pos, ray_dir, atmosphere.bottom_radius).x > 0.0;
    let behind = select(scattering.transmittance, vec3<f32>(0.0), hits_planet);

    // 4. Étoiles : noyées par un ciel lumineux, comme l'œil qui ne les distingue plus de jour
    let sky_luma = dot(scattering.inscatter, vec3<f32>(0.2126, 0.7152, 0.0722));
    let stars = render_starfield(ray_dir, camera.time) * night_amb_slider * exp(-sky_luma * 12.0);
    let visible_stars = select(stars, vec3<f32>(0.0), is_moon_disk);

    // 5. Lueur nocturne de l'air (bleu nuit), proportionnelle à l'épaisseur d'air traversée
    let air_path = clamp((1.0 - scattering.transmittance.b) * 2.5, 0.0, 1.0);
    let night_glow = vec3<f32>(0.008, 0.012, 0.024) * night_amb_slider * air_path;

    let sky = scattering.inscatter + night_glow + (sun_color + moon_color + visible_stars) * behind;
    return vec4<f32>(sky, 1.0);
}

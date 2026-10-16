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
    q += dot(q, q.yxz + 33.33);
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
        let center = h1.yz * 0.75 + 0.125;
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
        let center2 = h2.yz * 0.7 + 0.15;
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
    var space_color = render_starfield(ray_dir, camera.time) + sun_color;

    // 3. Halo atmosphérique externe planétaire (vu depuis l'orbite / espace)
    let cam_dist = length(camera.camera_pos);
    let r_planet = camera.planet_radius;
    let r_atm = r_planet * 1.055; // Épaisseur atmosphérique ~5.5% du rayon planétaire

    // Test d'intersection rayon / atmosphère planétaire
    let o = camera.camera_pos;
    let b = dot(o, ray_dir);

    if (b < 0.0) { // Le rayon se dirige vers l'avant de la caméra
        let closest_dist = length(o - ray_dir * b);
        if (closest_dist > r_planet && closest_dist < r_atm) {
            // Le rayon rase le limbe planétaire à travers l'atmosphère
            let t_atm = (closest_dist - r_planet) / (r_atm - r_planet);
            let density = pow(1.0 - t_atm, 2.2);

            // Point le plus proche sur le limbe
            let p_limb = normalize(o - ray_dir * b) * r_planet;
            let limb_sun = clamp(dot(normalize(p_limb), l) * 2.0 + 0.2, 0.0, 1.0);
            let terminator = clamp(1.0 - abs(dot(normalize(p_limb), l)) * 2.5, 0.0, 1.0);

            // Teinte bleue Rayleigh le jour, halo crépusculaire doré/orangé au terminateur
            let atm_hue = mix(vec3<f32>(0.22, 0.65, 1.0), vec3<f32>(1.0, 0.55, 0.22), terminator * 0.85);
            let limb_glow = atm_hue * (density * limb_sun * 2.5);

            // Atténuation si la caméra est très proche du sol
            let orbit_factor = clamp((cam_dist - r_planet) / 10.0, 0.0, 1.0);
            space_color += limb_glow * orbit_factor;
        }
    }

    // 4. Diffusion atmosphérique au sol (Dôme céleste bleu diurne + Horizon)
    let cam_alt = cam_dist - r_planet;
    let ground_factor = clamp(1.0 - cam_alt / 8.0, 0.0, 1.0);

    if (ground_factor > 0.001) {
        let cam_up = normalize(camera.camera_pos);
        let ray_up = dot(ray_dir, cam_up);
        let sun_up = dot(l, cam_up);
        let daylight = clamp(sun_up * 1.8 + 0.3, 0.0, 1.0);

        // Dégradé zénith azur vers horizon clair
        let horizon_blend = pow(1.0 - max(ray_up, 0.0), 2.8);
        let zenith_color = vec3<f32>(0.18, 0.48, 0.90);
        let horizon_color = mix(vec3<f32>(0.68, 0.80, 0.95), vec3<f32>(0.98, 0.65, 0.35), clamp(1.0 - sun_up * 3.0, 0.0, 1.0));
        let day_sky = mix(zenith_color, horizon_color, horizon_blend) * daylight;

        // Éclaircissement autour du soleil dans l'atmosphère au sol
        let ground_sun_glow = pow(clamp(cos_sun, 0.0, 1.0), 8.0) * vec3<f32>(0.35, 0.30, 0.20) * daylight;

        let final_sky = mix(space_color, day_sky + ground_sun_glow, ground_factor * daylight);
        return vec4<f32>(final_sky, 1.0);
    }

    return vec4<f32>(space_color, 1.0);
}

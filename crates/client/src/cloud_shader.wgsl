// =========================================================================
// HeroCraft - 3D Marshmallow Cloud Shader (Boîtes Englobantes + SDF Analytique)
// - Boîtes englobantes 3D physiques réelles traversables dans le monde
// - Sphere tracing ultra-rapide (8 à 16 pas) sur SDF analytique exacte
// - Biseaux 3D arrondis sur TOUTES les arêtes (dessus, dessous, coins verticaux, coins sphériques)
// - Angles concaves intérieurs adoucis sans aucune encoche sur les parois partagées
// - Ambiance tirée du dôme céleste (même formule que sky_shader.wgsl) : teinte du ciel partagée
// - Dessus clair, flancs intermédiaires, dessous plus sombre et légèrement mauve
// - Perspective atmosphérique vers le ciel et compression douce des hautes lumières
// - Écriture précise de la profondeur dans le Z-buffer (@builtin(frag_depth))
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
    cloud_params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) frame_u: vec3<f32>,
    @location(2) frame_v: vec3<f32>,
    @location(3) frame_w: vec3<f32>,
    @location(4) cloud_center: vec3<f32>,
    @location(5) box1_bounds: vec4<f32>,
    @location(6) box2_bounds: vec4<f32>,
    @location(7) config: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) frame_u: vec3<f32>,
    @location(2) frame_v: vec3<f32>,
    @location(3) frame_w: vec3<f32>,
    @location(4) cloud_center: vec3<f32>,
    @location(5) @interpolate(flat) box1_bounds: vec4<f32>,
    @location(6) @interpolate(flat) box2_bounds: vec4<f32>,
    @location(7) @interpolate(flat) config: vec4<f32>,
};

@vertex
fn vs_cloud(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Dérive douce du vent autour de l'axe planétaire (Y)
    let wind_speed = 0.0035; // ~30 minutes pour une révolution planétaire complète
    let angle = camera.time * wind_speed;
    let cos_a = cos(angle);
    let sin_a = sin(angle);

    // Rotation autour de l'axe Y
    let pos_rot = vec3<f32>(
        in.position.x * cos_a - in.position.z * sin_a,
        in.position.y,
        in.position.x * sin_a + in.position.z * cos_a
    );

    let center_rot = vec3<f32>(
        in.cloud_center.x * cos_a - in.cloud_center.z * sin_a,
        in.cloud_center.y,
        in.cloud_center.x * sin_a + in.cloud_center.z * cos_a
    );

    let u_rot = vec3<f32>(
        in.frame_u.x * cos_a - in.frame_u.z * sin_a,
        in.frame_u.y,
        in.frame_u.x * sin_a + in.frame_u.z * cos_a
    );

    let v_rot = vec3<f32>(
        in.frame_v.x * cos_a - in.frame_v.z * sin_a,
        in.frame_v.y,
        in.frame_v.x * sin_a + in.frame_v.z * cos_a
    );

    let w_rot = vec3<f32>(
        in.frame_w.x * cos_a - in.frame_w.z * sin_a,
        in.frame_w.y,
        in.frame_w.x * sin_a + in.frame_w.z * cos_a
    );

    out.clip_position = camera.view_proj * vec4<f32>(pos_rot, 1.0);
    out.world_pos = pos_rot;
    out.frame_u = u_rot;
    out.frame_v = v_rot;
    out.frame_w = w_rot;
    out.cloud_center = center_rot;
    out.box1_bounds = in.box1_bounds;
    out.box2_bounds = in.box2_bounds;
    out.config = in.config;

    return out;
}

struct MoonData {
    dir: vec3<f32>,
    phase: f32,
};

fn get_moon_data(time: f32, light_dir: vec3<f32>) -> MoonData {
    let t_day = 1200.0;
    let t_moon = 1260.0;
    let omega_moon = 6.28318530718 / t_moon;

    let moon_tilt: f32 = 0.497;
    let u_moon = vec3<f32>(cos(moon_tilt), sin(moon_tilt), 0.0);
    let v_moon = vec3<f32>(0.0, 0.0, 1.0);

    let moon_angle = time * omega_moon + 3.14159265;
    let moon_dir = normalize(u_moon * cos(moon_angle) + v_moon * sin(moon_angle));

    let sun_moon_dot = dot(normalize(light_dir), moon_dir);
    let moon_phase = clamp((-sun_moon_dot + 1.0) * 0.5, 0.0, 1.0);
    var data: MoonData;
    data.dir = moon_dir;
    data.phase = moon_phase;
    return data;
}

// Distance 2D signée exacte à un rectangle orthogonal net (sans pré-arrondi)
fn sd_box_2d(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// Direction du gradient de sd_box_2d (normale extérieure de la boîte la plus proche)
fn sd_box_2d_dir(p: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let d = abs(p) - b;
    let s = select(vec2<f32>(-1.0), vec2<f32>(1.0), p >= vec2<f32>(0.0));
    if (max(d.x, d.y) > 0.0) {
        return normalize(max(d, vec2<f32>(0.0))) * s;
    }
    return select(vec2<f32>(0.0, s.y), vec2<f32>(s.x, 0.0), d.x > d.y);
}

// Minimum lissé polynomial (congé de rayon ~k entre deux distances)
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// Distance 3D avec biseau marshmallow uniforme sur TOUTES les arêtes et coins sphériques
// Utilise l'érosion analytique exacte des sous-boîtes pour garantir une surface 100% lisse
// sans aucune ride sur les faces planes et sans hernie sur les arêtes extérieures
fn sd_cloud_3d(p: vec3<f32>, b1: vec4<f32>, b2: vec4<f32>, num_boxes: f32, half_depth: f32, r: f32) -> f32 {
    let core1 = max(b1.zw - vec2<f32>(r), vec2<f32>(0.001));
    let d1 = sd_box_2d(p.xy - b1.xy, core1);

    var d_core = d1;
    if (num_boxes >= 1.5) {
        let core2 = max(b2.zw - vec2<f32>(r), vec2<f32>(0.001));
        let d2 = sd_box_2d(p.xy - b2.xy, core2);
        // Union lissée uniquement dans les angles rentrants (normales des deux boîtes divergentes) :
        // sur les arêtes alignées des L / T, les deux distances sont égales et un smin
        // uniforme y ferait un bourrelet terminé par une pointe
        let g1 = sd_box_2d_dir(p.xy - b1.xy, core1);
        let g2 = sd_box_2d_dir(p.xy - b2.xy, core2);
        let k = r * 1.6 * clamp(1.0 - dot(g1, g2), 0.0, 1.0);
        d_core = smin(d1, d2, max(k, 1e-4));
    }

    let d_z = abs(p.z) - (half_depth - r);
    let w = max(vec2<f32>(d_core, d_z), vec2<f32>(0.0));
    let inside = min(max(d_core, d_z), 0.0);
    return length(w) + inside - r;
}

// Normale par gradient analytique fini
fn get_cloud_normal(p: vec3<f32>, b1: vec4<f32>, b2: vec4<f32>, num_boxes: f32, half_depth: f32, r: f32) -> vec3<f32> {
    let eps = 0.004;
    let dx = sd_cloud_3d(p + vec3<f32>(eps, 0.0, 0.0), b1, b2, num_boxes, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(eps, 0.0, 0.0), b1, b2, num_boxes, half_depth, r);
    let dy = sd_cloud_3d(p + vec3<f32>(0.0, eps, 0.0), b1, b2, num_boxes, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(0.0, eps, 0.0), b1, b2, num_boxes, half_depth, r);
    let dz = sd_cloud_3d(p + vec3<f32>(0.0, 0.0, eps), b1, b2, num_boxes, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(0.0, 0.0, eps), b1, b2, num_boxes, half_depth, r);
    return normalize(vec3<f32>(dx, dy, dz));
}

// Couleur diurne du dôme céleste dans la direction `ray_dir` (sans disque solaire ni étoiles).
// Doit rester identique à la diffusion au sol de sky_shader.wgsl.
fn sky_dome(ray_dir: vec3<f32>, cam_up: vec3<f32>, l: vec3<f32>, sunset_factor: f32) -> vec3<f32> {
    let sun_slider = camera.lighting_params.y;
    let day_amb_slider = camera.lighting_params.w;

    let zenith_day = vec3<f32>(0.15, 0.45, 0.88);
    let zenith_sunset = vec3<f32>(0.07, 0.13, 0.38);
    let zenith_color = mix(zenith_day, zenith_sunset, sunset_factor);

    let horizon_day = vec3<f32>(0.65, 0.80, 0.96);
    let forward_sun = clamp(dot(ray_dir, l) * 0.5 + 0.5, 0.0, 1.0);
    let sunset_facing_sun = mix(vec3<f32>(1.0, 0.58, 0.15), vec3<f32>(1.0, 0.82, 0.35), pow(forward_sun, 3.0));
    let sunset_opposite_sun = vec3<f32>(0.65, 0.32, 0.48);
    let sunset_horizon = mix(sunset_opposite_sun, sunset_facing_sun, pow(forward_sun, 1.5));
    let horizon_color = mix(horizon_day, sunset_horizon, sunset_factor);

    // clamp (et non max) : dot(cam_up, cam_up) peut valoir 1.0000001 et pow(négatif) donne NaN (pixels noirs)
    let ray_h = clamp(dot(ray_dir, cam_up), 0.0, 1.0);
    let horizon_blend = pow(1.0 - ray_h, 2.5);
    let day_sky = mix(zenith_color, horizon_color, horizon_blend) * day_amb_slider;

    let active_glow_color = mix(vec3<f32>(0.40, 0.35, 0.22), vec3<f32>(1.0, 0.52, 0.15), sunset_factor);
    let glow = pow(clamp(dot(ray_dir, l), 0.0, 1.0), 6.0) * active_glow_color * (1.0 + sunset_factor * 1.8) * sun_slider;

    return day_sky + glow;
}

// Compression douce des hautes lumières au-delà de 0.78 (asymptote à 1.0)
fn soft_shoulder(c: vec3<f32>) -> vec3<f32> {
    let knee = 0.78;
    let over = max(c - vec3<f32>(knee), vec3<f32>(0.0));
    let compressed = vec3<f32>(knee) + (1.0 - knee) * (vec3<f32>(1.0) - exp(-over / (1.0 - knee)));
    return select(c, compressed, c > vec3<f32>(knee));
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_cloud(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // Multiplicateur de densité / visibilité des nuages (0.0 en vue satellite)
    let cloud_slider = camera.lighting_params2.z;
    if (cloud_slider < 0.01) {
        discard;
    }

    let view_dir = normalize(camera.camera_pos - in.world_pos);
    let ray_dir = -view_dir;

    // Passage dans le repère local de la boîte englobante
    let v_world = in.world_pos - in.cloud_center;
    let p0_surface = vec3<f32>(
        dot(v_world, in.frame_u),
        dot(v_world, in.frame_v),
        dot(v_world, in.frame_w)
    );

    let cam_v_world = camera.camera_pos - in.cloud_center;
    let p0_camera = vec3<f32>(
        dot(cam_v_world, in.frame_u),
        dot(cam_v_world, in.frame_v),
        dot(cam_v_world, in.frame_w)
    );

    let half_depth = in.config.y;
    let pad_xy = 0.65;
    let pad_z = 0.50;

    var c_min_x = in.box1_bounds.x - in.box1_bounds.z - pad_xy;
    var c_max_x = in.box1_bounds.x + in.box1_bounds.z + pad_xy;
    var c_min_y = in.box1_bounds.y - in.box1_bounds.w - pad_xy;
    var c_max_y = in.box1_bounds.y + in.box1_bounds.w + pad_xy;

    if (in.config.x >= 1.5) {
        c_min_x = min(c_min_x, in.box2_bounds.x - in.box2_bounds.z - pad_xy);
        c_max_x = max(c_max_x, in.box2_bounds.x + in.box2_bounds.z + pad_xy);
        c_min_y = min(c_min_y, in.box2_bounds.y - in.box2_bounds.w - pad_xy);
        c_max_y = max(c_max_y, in.box2_bounds.y + in.box2_bounds.w + pad_xy);
    }

    let c_min_z = -half_depth - pad_z;
    let c_max_z = half_depth + pad_z;

    let cam_in_box = p0_camera.x >= c_min_x && p0_camera.x <= c_max_x &&
                     p0_camera.y >= c_min_y && p0_camera.y <= c_max_y &&
                     p0_camera.z >= c_min_z && p0_camera.z <= c_max_z;

    // Si la caméra est en dehors de la boîte, on ne rend que les faces avant (évite tout Z-fighting et découpe prématurée)
    if (!is_front && !cam_in_box) {
        discard;
    }

    // Point de départ du rayon : caméra si on est dans la boîte, surface d'entrée sinon
    let local_p0 = select(p0_surface, p0_camera, cam_in_box);

    let local_ray_dir = normalize(vec3<f32>(
        dot(ray_dir, in.frame_u),
        dot(ray_dir, in.frame_v),
        dot(ray_dir, in.frame_w)
    ));

    let roundness = clamp(camera.cloud_params.x, 0.05, min(0.55, half_depth - 0.04));
    let shadow_intensity = camera.cloud_params.y;
    let sunset_glow_slider = camera.cloud_params.z;
    let micro_noise_slider = camera.cloud_params.w;

    var t = 0.0;
    var hit = false;
    var hit_local_p = local_p0;

    // Test si le point de départ est déjà à l'intérieur du nuage
    let initial_dist = sd_cloud_3d(local_p0, in.box1_bounds, in.box2_bounds, in.config.x, half_depth, roundness);
    if (initial_dist <= 0.001) {
        hit = true;
        hit_local_p = local_p0;
    } else {
        // Sphere tracing borné par la sortie de la boîte englobante (intersection rayon / AABB locale).
        // Un rayon qui frôle un arrondi avance par pas minuscules : avec peu de pas il abandonnait
        // avant d'atteindre une autre branche du nuage située derrière (ciel visible à tort).
        let inv_d = 1.0 / select(local_ray_dir, vec3<f32>(1e-6), abs(local_ray_dir) < vec3<f32>(1e-6));
        let t0 = (vec3<f32>(c_min_x, c_min_y, c_min_z) - local_p0) * inv_d;
        let t1 = (vec3<f32>(c_max_x, c_max_y, c_max_z) - local_p0) * inv_d;
        let t_exit = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));
        for (var step = 0; step < 96; step = step + 1) {
            let p = local_p0 + local_ray_dir * t;
            let d = sd_cloud_3d(p, in.box1_bounds, in.box2_bounds, in.config.x, half_depth, roundness);
            if (d < 0.002) {
                hit = true;
                hit_local_p = p;
                break;
            }
            // Pas minimal pour ne pas s'enliser le long d'une surface rasée
            t += max(d, 0.01);
            if (t > t_exit) {
                break;
            }
        }
    }

    if (!hit) {
        discard;
    }

    // Calcul de la normale sur la surface marshmallow
    let norm_local = get_cloud_normal(hit_local_p, in.box1_bounds, in.box2_bounds, in.config.x, half_depth, roundness);
    let norm_world = normalize(
        norm_local.x * in.frame_u +
        norm_local.y * in.frame_v +
        norm_local.z * in.frame_w
    );

    // Position monde exacte de l'impact pour le Z-Buffer et l'éclairage
    let hit_world_pos = in.cloud_center +
        hit_local_p.x * in.frame_u +
        hit_local_p.y * in.frame_v +
        hit_local_p.z * in.frame_w;

    let clip_pos = camera.view_proj * vec4<f32>(hit_world_pos, 1.0);
    let frag_depth = clip_pos.z / clip_pos.w;

    // --- Éclairage : ambiance tirée du ciel, dessus clair, dessous sombre ---
    let moon_slider = camera.lighting_params.x;
    let sun_slider = camera.lighting_params.y;
    let night_amb_slider = camera.lighting_params.z;
    let fog_slider = camera.lighting_params2.y;

    // Même repère que le ciel vu par le joueur (sky_shader.wgsl) : tous les nuages partagent la teinte du ciel
    let cam_up = normalize(camera.camera_pos);
    let l_sun = normalize(camera.light_dir);
    let sun_up = dot(l_sun, cam_up);
    let daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);
    let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
    let sunset_factor = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);

    let up_planet = normalize(hit_world_pos);
    let ndotup = dot(norm_world, up_planet);
    let top = ndotup * 0.5 + 0.5; // 1 = dessus, 0.5 = flanc, 0 = dessous

    // 1. Ambiance du ciel : horizon dans la direction regardée, mêlé au zénith pour les faces hautes
    let horiz = ray_dir - cam_up * dot(ray_dir, cam_up);
    let horiz_dir = select(cam_up, normalize(horiz), length(horiz) > 1e-3);
    let sky_top = sky_dome(cam_up, cam_up, l_sun, sunset_factor);
    let sky_side = sky_dome(normalize(horiz_dir + cam_up * 0.15), cam_up, l_sun, sunset_factor);
    let sky_ambient = mix(sky_side, sky_top, 0.30 * top);

    // 2. Soleil : wrap doux + halo argenté large et discret (pas de dégradé dur sur une même face)
    let sun_col = mix(vec3<f32>(1.0, 0.97, 0.92), vec3<f32>(1.0, 0.66, 0.46), sunset_factor);
    let sun_wrap = pow(clamp(dot(norm_world, l_sun) * 0.5 + 0.5, 0.0, 1.0), 1.5);
    let silver = pow(clamp(dot(ray_dir, l_sun), 0.0, 1.0), 6.0) * 0.18 * sunset_glow_slider;
    let sun_light = sun_col * (sun_wrap * 0.55 + silver) * sun_slider;

    // 3. Dessous plus sombre et légèrement mauve (réglé par le curseur Ombre)
    let under_dark = clamp(0.38 * shadow_intensity, 0.0, 0.75);
    let shade = mix(1.0 - under_dark, 1.0, smoothstep(0.0, 1.0, top));
    let under_tint = mix(vec3<f32>(0.92, 0.86, 0.90), vec3<f32>(1.0), top);
    let day_col = (sky_ambient * 0.85 + sun_light) * shade * under_tint;

    // 4. Nuit : ambiance sombre et clair de lune argenté
    let moon = get_moon_data(camera.time, camera.light_dir);
    let moon_up = clamp((dot(moon.dir, cam_up) + 0.04) / 0.18, 0.0, 1.0);
    let moon_wrap = clamp(dot(norm_world, moon.dir) * 0.5 + 0.5, 0.0, 1.0);
    let moon_light = vec3<f32>(0.62, 0.74, 0.92) * (moon_wrap * 0.20 + 0.04) * moon.phase * moon_up * moon_slider;
    let night_ambient = vec3<f32>(0.020, 0.028, 0.055) * night_amb_slider * (0.7 + 0.3 * top);

    var col = mix(night_ambient + moon_light, day_col, daylight);

    // 5. Micro-texture optionnelle (désactivée par défaut)
    if (micro_noise_slider > 0.01) {
        let p_grid = floor(hit_local_p * 18.0);
        let g_hash = fract(sin(dot(p_grid, vec3<f32>(12.9898, 78.233, 45.164))) * 43758.5453);
        col *= 1.0 + (g_hash - 0.5) * micro_noise_slider * 0.06;
    }

    // 6. Perspective atmosphérique : fondu vers la couleur du ciel derrière le nuage
    let dist = length(camera.camera_pos - hit_world_pos);
    let night_sky = vec3<f32>(0.008, 0.012, 0.024) * night_amb_slider;
    let sky_behind = mix(night_sky, sky_dome(ray_dir, cam_up, l_sun, sunset_factor), daylight);
    let fog = (1.0 - exp(-max(dist - 5.0, 0.0) * 0.06 * fog_slider)) * 0.90;
    col = mix(col, sky_behind, fog);

    // 7. Compression douce des hautes lumières (pas de blanc saturé « plastique »)
    let final_color = soft_shoulder(col);
    let final_alpha = clamp(cloud_slider, 0.0, 1.0);

    var out: FragmentOutput;
    out.color = vec4<f32>(final_color, final_alpha);
    out.depth = frag_depth;
    return out;
}

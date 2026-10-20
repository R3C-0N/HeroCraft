// =========================================================================
// HeroCraft - 3D Marshmallow Cloud Shader (Boîtes Englobantes + SDF Analytique)
// - Boîtes englobantes 3D physiques réelles traversables dans le monde
// - Sphere tracing ultra-rapide (8 à 16 pas) sur SDF analytique exacte
// - Forme décrite par un masque d'occupation 11x11 par groupe de nuages (contours découpés, encoches, trous)
// - Coins convexes arrondis, angles rentrants comblés, arêtes hautes et basses en coussin
// - Ambiance tirée du dôme céleste (même formule que sky_shader.wgsl) : teinte du ciel partagée
// - Dessus clair, flancs intermédiaires, dessous plus sombre et légèrement mauve
// - Perspective atmosphérique vers le ciel et compression douce des hautes lumières
// - Écriture précise de la profondeur dans le Z-buffer (@builtin(frag_depth))
// =========================================================================

#import herocraft::common::{camera, get_moon_data, sky_factors, sky_dome}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) frame_u: vec3<f32>,
    @location(2) frame_v: vec3<f32>,
    @location(3) frame_w: vec3<f32>,
    @location(4) cloud_center: vec3<f32>,
    @location(5) bbox: vec4<f32>,
    @location(6) mask: vec4<u32>,
    @location(7) config: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) frame_u: vec3<f32>,
    @location(2) frame_v: vec3<f32>,
    @location(3) frame_w: vec3<f32>,
    @location(4) cloud_center: vec3<f32>,
    @location(5) @interpolate(flat) bbox: vec4<f32>,
    @location(6) @interpolate(flat) mask: vec4<u32>,
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
    out.bbox = in.bbox;
    out.mask = in.mask;
    out.config = in.config;

    return out;
}

// Cellule (x, y) du masque 11x11 occupée ? (bit = y * 11 + x, sur 4 mots de 32 bits)
fn cell_on(m: vec4<u32>, x: i32, y: i32) -> bool {
    if (x < 0 || y < 0 || x > 10 || y > 10) {
        return false;
    }
    let bit = u32(y * 11 + x);
    let word = m[bit / 32u];
    return ((word >> (bit % 32u)) & 1u) == 1u;
}

// Distance 2D signée à la forme du masque, coins convexes arrondis et angles rentrants comblés
// (rayon rh). Exacte à moins d'une cellule de la surface, bornée à une cellule au-delà.
fn sd_mask_2d(p: vec2<f32>, m: vec4<u32>, cell: f32, rh: f32) -> f32 {
    let q = p / cell + vec2<f32>(5.5); // Coordonnées de grille (0..11)
    let c = vec2<i32>(floor(q));
    let inside = cell_on(m, c.x, c.y);

    // Distance à la cellule de statut opposé la plus proche dans le voisinage 3x3
    // (dehors : cellule pleine la plus proche ; dedans : cellule vide la plus proche)
    var d = cell;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let cc = c + vec2<i32>(dx, dy);
            if (cell_on(m, cc.x, cc.y) != inside) {
                let e = abs(q - (vec2<f32>(cc) + vec2<f32>(0.5))) - vec2<f32>(0.5);
                d = min(d, length(max(e, vec2<f32>(0.0))) * cell);
            }
        }
    }
    var sd = select(d, -d, inside);

    // Sommet de grille le plus proche : arrondi d'un coin convexe (1 cellule pleine sur 4)
    // ou congé d'un angle rentrant (3 cellules pleines sur 4)
    let vtx = round(q);
    let vi = vec2<i32>(vtx);
    let c00 = cell_on(m, vi.x - 1, vi.y - 1);
    let c10 = cell_on(m, vi.x, vi.y - 1);
    let c01 = cell_on(m, vi.x - 1, vi.y);
    let c11 = cell_on(m, vi.x, vi.y);
    let count = i32(c00) + i32(c10) + i32(c01) + i32(c11);
    let w0 = (q - vtx) * cell;
    if (count == 1) {
        // Signe vers la cellule pleine
        let s = vec2<f32>(select(-1.0, 1.0, c10 || c11), select(-1.0, 1.0, c01 || c11));
        let w = w0 * s;
        sd = max(sd, length(max(vec2<f32>(rh) - w, vec2<f32>(0.0))) - rh);
    } else if (count == 3) {
        // Signe vers la cellule vide
        let s = vec2<f32>(select(-1.0, 1.0, !c10 || !c11), select(-1.0, 1.0, !c01 || !c11));
        let w = w0 * s;
        if (w.x >= 0.0 && w.y >= 0.0 && w.x < rh && w.y < rh) {
            sd = min(sd, rh - length(w - vec2<f32>(rh)));
        }
    }
    return sd;
}

// Distance 3D : forme du masque extrudée sur l'épaisseur du nuage, arêtes hautes et basses
// arrondies (rayon r) par érosion puis dilatation, pour le profil en coussin
fn sd_cloud_3d(p: vec3<f32>, m: vec4<u32>, cell: f32, half_depth: f32, r: f32) -> f32 {
    let rh = clamp(max(r * 1.3, 0.35), 0.0, 0.48 * cell);
    let d_core = sd_mask_2d(p.xy, m, cell, rh) + r;
    // Dalle courbée pour suivre la sphère (les grands nuages ne remontent plus aux bords)
    let curv_r = camera.planet_radius + 8.8;
    let z = p.z + dot(p.xy, p.xy) / (2.0 * curv_r);
    let d_z = abs(z) - (half_depth - r);
    let w = max(vec2<f32>(d_core, d_z), vec2<f32>(0.0));
    let inside = min(max(d_core, d_z), 0.0);
    // Facteur 0.9 : marge de sécurité du sphere tracing (courbure et congés non exacts)
    return (length(w) + inside - r) * 0.9;
}

// Normale par gradient analytique fini
fn get_cloud_normal(p: vec3<f32>, m: vec4<u32>, cell: f32, half_depth: f32, r: f32) -> vec3<f32> {
    let eps = 0.004;
    let dx = sd_cloud_3d(p + vec3<f32>(eps, 0.0, 0.0), m, cell, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(eps, 0.0, 0.0), m, cell, half_depth, r);
    let dy = sd_cloud_3d(p + vec3<f32>(0.0, eps, 0.0), m, cell, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(0.0, eps, 0.0), m, cell, half_depth, r);
    let dz = sd_cloud_3d(p + vec3<f32>(0.0, 0.0, eps), m, cell, half_depth, r)
           - sd_cloud_3d(p - vec3<f32>(0.0, 0.0, eps), m, cell, half_depth, r);
    return normalize(vec3<f32>(dx, dy, dz));
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
    let cloud_slider = camera.lighting_params_ext.z;
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

    let c_min_x = in.bbox.x - pad_xy;
    let c_max_x = in.bbox.z + pad_xy;
    let c_min_y = in.bbox.y - pad_xy;
    let c_max_y = in.bbox.w + pad_xy;

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
    let initial_dist = sd_cloud_3d(local_p0, in.mask, in.config.x, half_depth, roundness);
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
            let d = sd_cloud_3d(p, in.mask, in.config.x, half_depth, roundness);
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
    let norm_local = get_cloud_normal(hit_local_p, in.mask, in.config.x, half_depth, roundness);
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
    let fog_slider = camera.lighting_params_ext.y;

    // Même repère que le ciel vu par le joueur (sky_shader.wgsl) : tous les nuages partagent la teinte du ciel
    let cam_up = normalize(camera.camera_pos);
    let l_sun = normalize(camera.light_dir);
    let factors = sky_factors(dot(l_sun, cam_up));
    let daylight = factors.daylight;
    let sunset_factor = factors.sunset;

    let up_planet = normalize(hit_world_pos);
    let ndotup = dot(norm_world, up_planet);
    // 1 = dessus, 0 = dessous : mélange de l'orientation de la normale et de la hauteur dans la dalle
    // (dégradé vertical continu sur les flancs, comme les nuages Complementary)
    let hit_z = hit_local_p.z + dot(hit_local_p.xy, hit_local_p.xy) / (2.0 * (camera.planet_radius + 8.8));
    let height01 = clamp((hit_z + half_depth) / (2.0 * half_depth), 0.0, 1.0);
    let top = mix(ndotup * 0.5 + 0.5, pow(height01, 0.8), 0.45);

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
    // Auto-ombrage : deux échantillons du SDF en direction du soleil (parties du nuage qui masquent le soleil)
    let l_local = vec3<f32>(dot(l_sun, in.frame_u), dot(l_sun, in.frame_v), dot(l_sun, in.frame_w));
    let occ_a = sd_cloud_3d(hit_local_p + l_local * 0.45, in.mask, in.config.x, half_depth, roundness);
    let occ_b = sd_cloud_3d(hit_local_p + l_local * 1.10, in.mask, in.config.x, half_depth, roundness);
    let self_shadow = clamp(-occ_a / 0.30, 0.0, 1.0) * 0.6 + clamp(-occ_b / 0.30, 0.0, 1.0) * 0.4;
    let sun_light = sun_col * (sun_wrap * 0.55 * (1.0 - 0.45 * self_shadow) + silver) * sun_slider;

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

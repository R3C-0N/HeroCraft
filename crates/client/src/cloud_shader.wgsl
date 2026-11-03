// =========================================================================
// HeroCraft - 3D Marshmallow Cloud Shader (Boîtes Englobantes + SDF Analytique)
// - Boîtes englobantes 3D physiques réelles traversables dans le monde
// - Sphere tracing ultra-rapide (8 à 16 pas) sur SDF analytique exacte
// - Forme décrite par un masque d'occupation 11x11 par groupe de nuages (contours découpés, encoches, trous)
// - Coins convexes arrondis, angles rentrants comblés, arêtes hautes et basses en coussin
// - Soleil et ambiance du ciel tirés de l'atmosphère physique (atmosphere.wgsl)
// - Dessus clair, flancs intermédiaires, dessous plus sombre et légèrement mauve
// - Perspective aérienne : même diffusion de l'air que le ciel et le relief
// - Écriture précise de la profondeur dans le Z-buffer (@builtin(frag_depth))
// =========================================================================

#import herocraft::common::{camera, sky_factors, flatten_point, rotate_by, quantization_dither}
#import herocraft::atmosphere::{sun_light_at, sky_light, aerial_perspective_screen, apply_aerial_perspective}

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
    // Planète aplatie (common.wgsl) : le nuage y est déplacé d'un bloc, centre dessiné et rotation
    // du repère vrai vers le repère dessiné. Le reste de la sortie est dans la vraie géométrie.
    @location(8) @interpolate(flat) flat_center: vec3<f32>,
    @location(9) @interpolate(flat) flat_rot: vec3<f32>,
};

// Passe d'ombre des nuages : projection orthographique depuis le soleil (shadow.rs)
struct ShadowPass {
    view_proj: mat4x4<f32>,
};

@group(1) @binding(0)
var<uniform> shadow_pass: ShadowPass;

@vertex
fn vs_cloud(in: VertexInput) -> VertexOutput {
    return cloud_vertex(in, camera.view_proj, true);
}

@vertex
fn vs_cloud_shadow(in: VertexInput) -> VertexOutput {
    return cloud_vertex(in, shadow_pass.view_proj, false);
}

// Dérive du vent appliquée à la boîte et à son repère, projetée par `view_proj` (sur la planète
// aplatie si `flatten`, tout le nuage suivant son centre)
fn cloud_vertex(in: VertexInput, view_proj: mat4x4<f32>, flatten: bool) -> VertexOutput {
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

    var drawn = pos_rot;
    out.flat_center = center_rot;
    out.flat_rot = vec3<f32>(0.0);
    if (flatten) {
        let flat = flatten_point(center_rot);
        out.flat_center = flat.pos;
        out.flat_rot = flat.rot;
        drawn = flat.pos + rotate_by(pos_rot - center_rot, flat.rot);
    }
    out.clip_position = view_proj * vec4<f32>(drawn, 1.0);
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

// Cellule du masque qui contient le point (coordonnées de grille 0..11)
fn mask_cell(p: vec2<f32>, cell: f32) -> vec2<i32> {
    return vec2<i32>(floor(p / cell + vec2<f32>(5.5)));
}

// Voisinage 3x3 de la cellule `c` (bit (dy + 1) * 3 + (dx + 1)). Il ne change qu'en passant
// d'une cellule à l'autre : les marches le gardent d'un pas au suivant.
fn mask_neighborhood(m: vec4<u32>, c: vec2<i32>) -> u32 {
    var nb = 0u;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            if (cell_on(m, c.x + dx, c.y + dy)) {
                nb = nb | (1u << u32((dy + 1) * 3 + dx + 1));
            }
        }
    }
    return nb;
}

// Distance 2D signée à la forme du masque, coins convexes arrondis et angles rentrants comblés
// (rayon rh). Exacte et continue à moins d'une cellule de la surface, bornée à une cellule au-delà.
// `nb` : voisinage 3x3 de la cellule du point (mask_neighborhood).
fn sd_mask_2d_nb(p: vec2<f32>, nb: u32, cell: f32, rh: f32) -> f32 {
    let q = p / cell + vec2<f32>(5.5); // Coordonnées de grille (0..11)
    let c = vec2<i32>(floor(q));
    let inside = (nb & 16u) != 0u;

    // Sommet de grille le plus proche : arrondi d'un coin convexe (1 cellule pleine sur 4)
    // ou congé d'un angle rentrant (3 cellules pleines sur 4).
    // Ses 4 cellules sont dans le voisinage 3x3 (round(q) vaut c ou c + 1 sur chaque axe)
    let vtx = round(q);
    let o = vec2<i32>(vtx) - c;
    let b = u32(o.y * 3 + o.x);
    let c00 = ((nb >> b) & 1u) == 1u;
    let c10 = ((nb >> (b + 1u)) & 1u) == 1u;
    let c01 = ((nb >> (b + 3u)) & 1u) == 1u;
    let c11 = ((nb >> (b + 4u)) & 1u) == 1u;
    let count = i32(c00) + i32(c10) + i32(c01) + i32(c11);
    let w0 = (q - vtx) * cell;

    // Angle rentrant : la cellule vide du sommet est remplacée par son quart de plan au coin
    // arrondi (distance exacte), des deux côtés de la surface. Sans cela le congé ne valait que
    // dans la cellule vide : distance discontinue au ras des parois (encoches et arêtes visibles)
    let s3 = vec2<f32>(select(-1.0, 1.0, !c10 || !c11), select(-1.0, 1.0, !c01 || !c11));
    var skip = vec2<i32>(99);
    if (count == 3) {
        skip = o + vec2<i32>(select(-1, 0, s3.x > 0.0), select(-1, 0, s3.y > 0.0));
    }

    // Distance à la cellule de statut opposé la plus proche dans le voisinage 3x3
    // (dehors : cellule pleine la plus proche ; dedans : cellule vide la plus proche)
    var d = cell;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let on = ((nb >> u32((dy + 1) * 3 + dx + 1)) & 1u) == 1u;
            if (on != inside && !(dx == skip.x && dy == skip.y)) {
                let cc = c + vec2<i32>(dx, dy);
                let e = abs(q - (vec2<f32>(cc) + vec2<f32>(0.5))) - vec2<f32>(0.5);
                d = min(d, length(max(e, vec2<f32>(0.0))) * cell);
            }
        }
    }
    if (count == 3) {
        // Distance signée au quart de plan vide arrondi (négative dedans)
        let e = vec2<f32>(rh) - w0 * s3;
        let sd_e = length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0) - rh;
        d = min(d, select(-sd_e, sd_e, inside));
    }
    var sd = select(d, -d, inside);

    if (count == 1) {
        // Signe vers la cellule pleine
        let s = vec2<f32>(select(-1.0, 1.0, c10 || c11), select(-1.0, 1.0, c01 || c11));
        // Quart de plan plein au coin arrondi, distance exacte aussi en profondeur (sinon un
        // palier à -rh, discontinu au milieu de la cellule)
        let e = vec2<f32>(rh) - w0 * s;
        sd = max(sd, length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0) - rh);
    }
    return sd;
}

// Distance 3D : forme du masque extrudée sur l'épaisseur du nuage, arêtes hautes et basses
// arrondies (rayon r) par érosion puis dilatation, pour le profil en coussin
fn sd_cloud_3d(p: vec3<f32>, m: vec4<u32>, cell: f32, half_depth: f32, r: f32) -> f32 {
    return sd_cloud_3d_nb(p, mask_neighborhood(m, mask_cell(p.xy, cell)), cell, half_depth, r);
}

// Même distance, voisinage 3x3 de la cellule du point déjà lu (mask_neighborhood)
fn sd_cloud_3d_nb(p: vec3<f32>, nb: u32, cell: f32, half_depth: f32, r: f32) -> f32 {
    let rh = clamp(max(r * 1.3, 0.35), 0.0, 0.48 * cell);
    let d_core = sd_mask_2d_nb(p.xy, nb, cell, rh) + r;
    // Dalle courbée pour suivre la sphère (les grands nuages ne remontent plus aux bords)
    let curv_r = camera.planet_radius + 8.8 * (camera.planet_radius / 25.0);
    let z = p.z + dot(p.xy, p.xy) / (2.0 * curv_r);
    let d_z = abs(z) - (half_depth - r);
    let w = max(vec2<f32>(d_core, d_z), vec2<f32>(0.0));
    let inside = min(max(d_core, d_z), 0.0);
    // Facteur 0.9 : marge de sécurité du sphere tracing (courbure et congés non exacts)
    return (length(w) + inside - r) * 0.9;
}

// sd_mask_2d_nb d'un point voisin : réutilise le voisinage `nb0` de la cellule `c0` s'il y est
fn sd_mask_2d_near(p: vec2<f32>, m: vec4<u32>, c0: vec2<i32>, nb0: u32, cell: f32, rh: f32) -> f32 {
    let c = mask_cell(p, cell);
    var nb = nb0;
    if (any(c != c0)) {
        nb = mask_neighborhood(m, c);
    }
    return sd_mask_2d_nb(p, nb, cell, rh);
}

// Normale : gradient de sd_cloud_3d par dérivation en chaîne. Seule la forme 2D du masque
// passe par des différences finies (4 évaluations au lieu de 6 évaluations du SDF 3D complet),
// l'épaisseur courbée se dérive analytiquement.
fn get_cloud_normal(p: vec3<f32>, m: vec4<u32>, cell: f32, half_depth: f32, r: f32) -> vec3<f32> {
    let eps = 0.004;
    let rh = clamp(max(r * 1.3, 0.35), 0.0, 0.48 * cell);
    // Les 5 points sont presque toujours dans la même cellule : un seul voisinage lu
    let c0 = mask_cell(p.xy, cell);
    let nb0 = mask_neighborhood(m, c0);
    let gx = sd_mask_2d_near(p.xy + vec2<f32>(eps, 0.0), m, c0, nb0, cell, rh) - sd_mask_2d_near(p.xy - vec2<f32>(eps, 0.0), m, c0, nb0, cell, rh);
    let gy = sd_mask_2d_near(p.xy + vec2<f32>(0.0, eps), m, c0, nb0, cell, rh) - sd_mask_2d_near(p.xy - vec2<f32>(0.0, eps), m, c0, nb0, cell, rh);
    let g_core = vec3<f32>(gx, gy, 0.0) / (2.0 * eps);
    let d_core = sd_mask_2d_nb(p.xy, nb0, cell, rh) + r;

    let curv_r = camera.planet_radius + 8.8 * (camera.planet_radius / 25.0);
    let z = p.z + dot(p.xy, p.xy) / (2.0 * curv_r);
    let d_z = abs(z) - (half_depth - r);
    let g_z = select(-1.0, 1.0, z >= 0.0) * vec3<f32>(p.xy / curv_r, 1.0);

    let w = max(vec2<f32>(d_core, d_z), vec2<f32>(0.0));
    let wl = length(w);
    var g = select(g_z, g_core, d_core > d_z);
    if (wl > 1e-6) {
        g = (w.x * g_core + w.y * g_z) / wl;
    }
    return normalize(g);
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

    // Caméra ramenée dans la vraie géométrie du nuage (planète aplatie)
    let eye = in.cloud_center + rotate_by(camera.camera_pos - in.flat_center, -in.flat_rot);
    let view_dir = normalize(eye - in.world_pos);
    let ray_dir = -view_dir;

    // Passage dans le repère local de la boîte englobante
    let v_world = in.world_pos - in.cloud_center;
    let p0_surface = vec3<f32>(
        dot(v_world, in.frame_u),
        dot(v_world, in.frame_v),
        dot(v_world, in.frame_w)
    );

    let cam_v_world = eye - in.cloud_center;
    let p0_camera = vec3<f32>(
        dot(cam_v_world, in.frame_u),
        dot(cam_v_world, in.frame_v),
        dot(cam_v_world, in.frame_w)
    );

    let half_depth = in.config.y;
    // Mêmes marges que la boîte du maillage (cloud_mesh.rs), à l'échelle de la planète
    let planet_scale = camera.planet_radius / 25.0;
    let pad_xy = 0.65 * planet_scale;
    let pad_z = 0.50 * planet_scale;

    let c_min_x = in.bbox.x - pad_xy;
    let c_max_x = in.bbox.z + pad_xy;
    let c_min_y = in.bbox.y - pad_xy;
    let c_max_y = in.bbox.w + pad_xy;

    // Affaissement des bords de la dalle courbée : majoré par le coin de l'emprise
    let rho_max = length(max(abs(vec2<f32>(c_min_x, c_min_y)), abs(vec2<f32>(c_max_x, c_max_y))));
    let sag = rho_max * rho_max / (2.0 * length(in.cloud_center));
    let c_min_z = -half_depth - pad_z - sag;
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
        // Sphere tracing borné à la boîte serrée autour de la forme (emprise des cellules,
        // épaisseur et affaissement de la dalle courbée) : la marge de la boîte du maillage
        // ne contient aucune surface, on la saute analytiquement au lieu d'y marcher.
        // Un rayon qui frôle un arrondi avance par pas minuscules : avec peu de pas il abandonnait
        // avant d'atteindre une autre branche du nuage située derrière (ciel visible à tort).
        let margin = 0.05 * planet_scale;
        let rho_shape = length(max(abs(in.bbox.xy), abs(in.bbox.zw)));
        let sag_shape = rho_shape * rho_shape / (2.0 * length(in.cloud_center));
        let tight_min = vec3<f32>(in.bbox.x, in.bbox.y, -half_depth - sag_shape) - vec3<f32>(margin);
        let tight_max = vec3<f32>(in.bbox.z, in.bbox.w, half_depth) + vec3<f32>(margin);
        let inv_d = 1.0 / select(local_ray_dir, vec3<f32>(1e-6), abs(local_ray_dir) < vec3<f32>(1e-6));
        let t0 = (tight_min - local_p0) * inv_d;
        let t1 = (tight_max - local_p0) * inv_d;
        let t_enter = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(t0.z, t1.z));
        let t_exit = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));
        // Seuil de contact proportionnel à la taille d'un pixel à cette distance : un rayon qui
        // rase la surface s'arrête dès qu'il en est à moins d'un demi-pixel au lieu de ramper
        let dist0 = select(length(eye - in.world_pos), 0.0, cam_in_box);
        let pixel_angle = 0.0006;
        t = max(t_enter, 0.0);
        // Rayon qui manque la boîte serrée : aucun pas
        let steps_max = select(0, 96, t <= t_exit);
        // Voisinage du masque relu seulement au changement de cellule
        var nb_cell = vec2<i32>(-100);
        var nb = 0u;
        for (var step = 0; step < steps_max; step = step + 1) {
            let p = local_p0 + local_ray_dir * t;
            let c = mask_cell(p.xy, in.config.x);
            if (any(c != nb_cell)) {
                nb_cell = c;
                nb = mask_neighborhood(in.mask, c);
            }
            let d = sd_cloud_3d_nb(p, nb, in.config.x, half_depth, roundness);
            let eps = max(0.002, (dist0 + t) * pixel_angle);
            if (d < eps) {
                hit = true;
                // Recalage sur la surface (avancer de d ne peut pas la traverser)
                hit_local_p = p + local_ray_dir * max(d, 0.0);
                break;
            }
            // Pas minimal pour ne pas s'enliser le long d'une surface rasée
            t += max(d, max(0.01, eps));
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

    let hit_drawn = in.flat_center + rotate_by(hit_world_pos - in.cloud_center, in.flat_rot);
    let clip_pos = camera.view_proj * vec4<f32>(hit_drawn, 1.0);
    let frag_depth = clip_pos.z / clip_pos.w;

    // --- Éclairage : ambiance tirée du ciel, dessus clair, dessous sombre ---
    let moon_slider = camera.lighting_params.x;
    let night_amb_slider = camera.lighting_params.z;

    // Même repère que le ciel vu par le joueur (sky_shader.wgsl) : tous les nuages partagent la teinte du ciel
    let cam_up = normalize(camera.camera_pos);
    let factors = sky_factors(dot(normalize(camera.light_dir), cam_up));
    // Soleil vu du nuage comme sur la planète aplatie (repère vrai du nuage)
    let l_sun = rotate_by(normalize(camera.light_dir), -in.flat_rot);
    let daylight = factors.daylight;

    let up_planet = normalize(hit_world_pos);
    let ndotup = dot(norm_world, up_planet);
    // 1 = dessus, 0 = dessous : mélange de l'orientation de la normale et de la hauteur dans la dalle
    // (dégradé vertical continu sur les flancs, comme les nuages Complementary)
    let hit_z = hit_local_p.z + dot(hit_local_p.xy, hit_local_p.xy) / (2.0 * length(in.cloud_center));
    let height01 = clamp((hit_z + half_depth) / (2.0 * half_depth), 0.0, 1.0);
    let top = mix(ndotup * 0.5 + 0.5, pow(height01, 0.8), 0.45);

    // 1. Ambiance : irradiance du ciel à l'altitude du nuage, du flanc (moyenne des côtés soleil
    // et opposé) au dessus selon `top`. Elle ne suit pas l'orientation horizontale de la normale :
    // au crépuscule, l'écart entre flanc éclairé et flanc opposé faisait ressortir les congés
    // des angles rentrants, où la normale tourne vite.
    let across_sun = cross(up_planet, l_sun);
    let side_dir = select(normalize(across_sun), normalize(cross(up_planet, vec3<f32>(1.0, 0.0, 0.0))), length(across_sun) < 1e-3);
    let sky_side = sky_light(hit_world_pos, side_dir, l_sun);
    let sky_top = sky_light(hit_world_pos, up_planet, l_sun);
    let sky_ambient = mix(sky_side, sky_top, top);

    // 2. Soleil arrivé à l'altitude du nuage à travers l'atmosphère : wrap doux + halo argenté
    // large et discret (pas de dégradé dur sur une même face)
    let sun_col = sun_light_at(hit_world_pos, l_sun);
    let sun_wrap = pow(clamp(dot(norm_world, l_sun) * 0.5 + 0.5, 0.0, 1.0), 1.5);
    let silver = pow(clamp(dot(ray_dir, l_sun), 0.0, 1.0), 6.0) * 0.18 * sunset_glow_slider;
    // Auto-ombrage : deux échantillons du SDF en direction du soleil (parties du nuage qui masquent le soleil)
    let l_local = vec3<f32>(dot(l_sun, in.frame_u), dot(l_sun, in.frame_v), dot(l_sun, in.frame_w));
    let occ_a = sd_cloud_3d(hit_local_p + l_local * 0.45, in.mask, in.config.x, half_depth, roundness);
    let occ_b = sd_cloud_3d(hit_local_p + l_local * 1.10, in.mask, in.config.x, half_depth, roundness);
    let self_shadow = clamp(-occ_a / 0.30, 0.0, 1.0) * 0.6 + clamp(-occ_b / 0.30, 0.0, 1.0) * 0.4;
    let sun_light = sun_col * (sun_wrap * 0.7 * (1.0 - 0.45 * self_shadow) + silver * 1.28);

    // 3. Dessous plus sombre et légèrement mauve (réglé par le curseur Ombre)
    let under_dark = clamp(0.38 * shadow_intensity, 0.0, 0.75);
    let shade = mix(1.0 - under_dark, 1.0, smoothstep(0.0, 1.0, top));
    let under_tint = mix(vec3<f32>(0.92, 0.86, 0.90), vec3<f32>(1.0), top);
    let day_col = (sky_ambient * 0.85 + sun_light) * shade * under_tint;

    // 4. Nuit : ambiance sombre et clair de lune argenté
    // Lune : direction et phase calculées une fois par image (renderer.rs)
    let moon_dir = camera.moon.xyz;
    let moon_up = clamp((dot(moon_dir, cam_up) + 0.04) / 0.18, 0.0, 1.0);
    let moon_wrap = clamp(dot(norm_world, rotate_by(moon_dir, -in.flat_rot)) * 0.5 + 0.5, 0.0, 1.0);
    let moon_light = vec3<f32>(0.62, 0.74, 0.92) * (moon_wrap * 0.20 + 0.04) * camera.moon.w * moon_up * moon_slider;
    let night_ambient = vec3<f32>(0.020, 0.028, 0.055) * night_amb_slider * (0.7 + 0.3 * top);

    // Le jour s'éteint de lui-même avec l'atmosphère : la nuit s'y ajoute quand il disparaît
    var col = (night_ambient + moon_light) * (1.0 - daylight) + day_col;

    // 5. Micro-texture optionnelle (désactivée par défaut)
    if (micro_noise_slider > 0.01) {
        let p_grid = floor(hit_local_p * 18.0);
        let g_hash = fract(sin(dot(p_grid, vec3<f32>(12.9898, 78.233, 45.164))) * 43758.5453);
        col *= 1.0 + (g_hash - 0.5) * micro_noise_slider * 0.06;
    }

    // 6. Perspective aérienne : diffusion et extinction de l'air entre la caméra et le nuage
    let dist = length(eye - hit_world_pos);
    let screen_dir = rotate_by(ray_dir, in.flat_rot);
    col = apply_aerial_perspective(col, aerial_perspective_screen(camera.camera_pos, screen_dir, clip_pos.xy / clip_pos.w, dist, normalize(camera.light_dir), 10));

    // 7. Les hautes lumières sont compressées par le tonemapper de la passe de composition
    let final_color = col;
    let final_alpha = clamp(cloud_slider, 0.0, 1.0);

    var out: FragmentOutput;
    out.color = vec4<f32>(quantization_dither(final_color, in.clip_position.xy), final_alpha);
    out.depth = frag_depth;
    return out;
}

// Ombre portée des nuages : le rayon part de la face de la boîte tournée vers le soleil et suit
// la lumière. Un impact écrit l'opacité du nuage dans la carte d'ombre (mélange par maximum).
@fragment
fn fs_cloud_shadow(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let cloud_slider = camera.lighting_params_ext.z;
    if (cloud_slider < 0.01 || !is_front) {
        discard;
    }

    let ray_dir = -normalize(camera.light_dir);
    let v_world = in.world_pos - in.cloud_center;
    let local_p0 = vec3<f32>(dot(v_world, in.frame_u), dot(v_world, in.frame_v), dot(v_world, in.frame_w));
    let local_ray_dir = normalize(vec3<f32>(dot(ray_dir, in.frame_u), dot(ray_dir, in.frame_v), dot(ray_dir, in.frame_w)));

    let half_depth = in.config.y;
    let roundness = clamp(camera.cloud_params.x, 0.05, min(0.55, half_depth - 0.04));
    let planet_scale = camera.planet_radius / 25.0;

    // Même boîte serrée que fs_cloud : la marge de la boîte du maillage ne contient aucune surface
    let margin = 0.05 * planet_scale;
    let rho_shape = length(max(abs(in.bbox.xy), abs(in.bbox.zw)));
    let sag_shape = rho_shape * rho_shape / (2.0 * length(in.cloud_center));
    let tight_min = vec3<f32>(in.bbox.x, in.bbox.y, -half_depth - sag_shape) - vec3<f32>(margin);
    let tight_max = vec3<f32>(in.bbox.z, in.bbox.w, half_depth) + vec3<f32>(margin);
    let inv_d = 1.0 / select(local_ray_dir, vec3<f32>(1e-6), abs(local_ray_dir) < vec3<f32>(1e-6));
    let t0 = (tight_min - local_p0) * inv_d;
    let t1 = (tight_max - local_p0) * inv_d;
    let t_enter = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(t0.z, t1.z));
    let t_exit = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));

    // Une ombre n'a pas besoin de la précision d'une surface vue : seuil de contact large
    let eps = 0.01 * planet_scale;
    var t = max(t_enter, 0.0);
    let steps_max = select(0, 48, t <= t_exit);
    var nb_cell = vec2<i32>(-100);
    var nb = 0u;
    for (var step = 0; step < steps_max; step = step + 1) {
        let p = local_p0 + local_ray_dir * t;
        let c = mask_cell(p.xy, in.config.x);
        if (any(c != nb_cell)) {
            nb_cell = c;
            nb = mask_neighborhood(in.mask, c);
        }
        let d = sd_cloud_3d_nb(p, nb, in.config.x, half_depth, roundness);
        if (d < eps) {
            return vec4<f32>(clamp(cloud_slider, 0.0, 1.0), 0.0, 0.0, 0.0);
        }
        t += max(d, eps);
        if (t > t_exit) {
            break;
        }
    }
    discard;
}

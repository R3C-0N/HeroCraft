// =========================================================================
// HeroCraft - Terrain & Voxel Surface Shader (WGSL)
// - Éclairage Half-Lambert & normales géométriques nettes
// - Interpolation douce des couleurs et de l'Ambient Occlusion (AO)
// - Reflets spéculaires intenses de l'eau (Blinn-Phong + Fresnel) et scintillement de neige
// - Lueur atmosphérique rasante sur le limbe planétaire en vue orbitale
// - Brouillard atmosphérique volumétrique de distance au sol
// - Dithered Cross-Fade (tramage stochastique Bayer 4x4) pour transitions LOD
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

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>, // Interpolation douce pour l'AO per-vertex
    @location(2) world_pos: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_pos = in.position;
    out.normal = in.normal;
    out.color = in.color;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

// Matrice de tramage Bayer 4x4 normalisée [0.0, 1.0] pour transition douce stochastique
fn bayer4x4(pos: vec2<u32>) -> f32 {
    let bayer = array<f32, 16>(
        0.0 / 16.0,  8.0 / 16.0,  2.0 / 16.0, 10.0 / 16.0,
        12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0,  6.0 / 16.0,
        3.0 / 16.0, 11.0 / 16.0,  1.0 / 16.0,  9.0 / 16.0,
        15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0,  5.0 / 16.0
    );
    let x = pos.x % 4u;
    let y = pos.y % 4u;
    return bayer[y * 4u + x];
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Transition douce hybride par tramage stochastique (Dithered Cross-Fade)
    let alpha = in.color.a;
    if (abs(alpha) < 0.999) {
        let screen_coord = vec2<u32>(in.clip_position.xy);
        let dither = bayer4x4(screen_coord);
        if (alpha >= 0.0) {
            if (dither > alpha) {
                discard;
            }
        } else {
            let opacity = -alpha;
            if (dither <= (1.0 - opacity)) {
                discard;
            }
        }
    }

    // 2. Détection des types de matériaux selon la couleur du bloc/terrain
    // Eau : bathymétrie complète (lagons azur, bleu océan moyen et fosses abyssales sombres)
    let is_water = in.color.b > in.color.r * 1.5 && in.color.r < 0.25;
    // Neige / Glace : haute luminance et équilibre des canaux
    let is_snow = in.color.r > 0.82 && in.color.g > 0.84 && in.color.b > 0.88;

    // Normale géométrique pure de la facette (style Voxel et Low-Poly franc et net pour le sol terrestre)
    let dpx = dpdx(in.world_pos);
    let dpy = dpdy(in.world_pos);
    let cross_norm = normalize(cross(dpx, dpy));
    let n_facet = select(cross_norm, -cross_norm, dot(cross_norm, in.normal) < 0.0);

    // Normale sphérique continue idéale pour la surface de l'océan
    let n_sphere = normalize(in.world_pos);

    // Facteur d'altitude orbitale : transition fluide entre vue au sol (alt < 3.0) et vue satellite (alt > 15.0)
    let cam_dist = length(camera.camera_pos);
    let cam_alt = cam_dist - camera.planet_radius;
    let orbit_factor = clamp((cam_alt - 3.0) / 12.0, 0.0, 1.0);

    // Pour l'eau : en vue satellite, on lisse les facettes des polygones en interpolant vers la normale sphérique,
    // garantissant un reflet spéculaire pur, doux et sans polygone visible.
    // Le sol terrestre (roche, herbe, terre, sable, neige) reste quant à lui 100% voxelisé et facetté (n_facet).
    let water_normal = normalize(mix(n_facet, n_sphere, orbit_factor));
    let n = select(n_facet, water_normal, is_water);

    let l = normalize(camera.light_dir);
    let view_dir = normalize(camera.camera_pos - in.world_pos);
    let h = normalize(l + view_dir);

    // 3. Détection des falaises et contremarches entre terrasses étagées (Approche B)
    // Sur un plateau horizontal, n_facet est presque colinéaire à n_sphere (verticality >= 0.88).
    // Sur un flanc de marche ou une falaise abrupte, n_facet s'incline fortement (verticality < 0.82).
    let verticality = dot(n_facet, n_sphere);
    let cliff_factor = clamp((0.88 - verticality) / 0.22, 0.0, 1.0) * orbit_factor;

    var base_color = in.color.rgb;

    // Si on est sur une falaise terrestre en vue orbitale :
    // On applique une roche minérale sombre avec fines strates rocheuses étagées
    if (!is_water && cliff_factor > 0.01) {
        let rock_strata = vec3<f32>(0.26, 0.24, 0.23); // Roche ardoise minérale sombre
        let alt_blocks = (length(in.world_pos) - camera.planet_radius) / (camera.planet_radius * 0.001534);
        // Micro-strates horizontales périodiques selon la hauteur des terrasses (6 blocs)
        let strata_band = sin(alt_blocks * (3.14159265 * 2.0 / 6.0));
        let strata_shading = 0.90 + strata_band * 0.12;
        let cliff_colored = rock_strata * strata_shading;

        // Ombrage de contact et accentuation de falaise
        base_color = mix(base_color, cliff_colored, cliff_factor * 0.85);
    }

    // 4. Éclairage solaire et nocturne (transition douce jour/nuit)
    let sun_elevation = dot(n_sphere, l);
    let day_factor = clamp(sun_elevation * 4.0 + 0.5, 0.0, 1.0);

    let ndotl = dot(n, l);
    let direct = max(ndotl, 0.0) * day_factor;
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.35) * day_factor;

    // Ambiance nocturne sombre et immersive (0.04 sous les étoiles vs 0.22 en plein jour)
    let ambient = mix(0.04, 0.22, day_factor);
    let light_intensity = ambient + (1.0 - ambient) * mix(half_lambert, direct, 0.40);

    // 5. Reflets spéculaires d'eau et de glace
    var specular_light = vec3<f32>(0.0);
    let ndoth = max(dot(n, h), 0.0);
    let ndotv = max(dot(n, view_dir), 0.0);

    if (is_water) {
        // Reflet spéculaire Blinn-Phong pour l'océan (fin et intense en vue satellite, atténué la nuit)
        let spec_power = mix(48.0, 96.0, orbit_factor);
        let sun_spec = pow(ndoth, spec_power) * 1.8;
        // Effet Fresnel rasante
        let fresnel = pow(1.0 - ndotv, 5.0) * 0.45 + 0.05;
        specular_light += vec3<f32>(1.0, 0.95, 0.85) * (sun_spec * clamp(ndotl * 2.0, 0.0, 1.0) + fresnel * 0.35) * day_factor;
    } else if (is_snow) {
        // Scintillement cristallin diffus sur les névés et glaciers
        let snow_glint = pow(ndoth, 24.0) * 0.40 * clamp(ndotl * 2.0, 0.0, 1.0);
        specular_light += vec3<f32>(0.92, 0.95, 1.0) * snow_glint * day_factor;
    }

    // 6. Lueur atmosphérique rasante sur le limbe planétaire en vue orbitale
    // Utilise la normale sphérique planétaire (n_sphere) et non la facette du bloc (n_facet),
    // afin que le halo atmosphérique soit strictement restreint au contour extérieur du disque planétaire contre l'espace,
    // sans jamais illuminer les contremarches, falaises ou blocs à 90° à l'intérieur des terres.
    let sphere_view = clamp(dot(n_sphere, view_dir), 0.0, 1.0);
    let rim = 1.0 - sphere_view;
    let sun_side = clamp(sun_elevation * 0.5 + 0.5, 0.0, 1.0);
    let rim_light = pow(rim, 8.0) * 0.35 * sun_side * orbit_factor;
    let atmosphere_color = vec3<f32>(0.25, 0.65, 1.0);

    // Couleur éclairée de base avec Ambient Occlusion (AO) per-vertex interpolé
    var lit_rgb = base_color * light_intensity + specular_light + atmosphere_color * rim_light;

    // 7. Brouillard atmosphérique de distance au sol (Ground Distance Fog)
    let dist = length(camera.camera_pos - in.world_pos);
    let ground_proximity = clamp(1.0 - cam_alt / 8.0, 0.0, 1.0);

    if (ground_proximity > 0.001) {
        // Début du brouillard à 2.5 unités (~65 blocs), saturation douce à 8.0 unités
        let fog_factor = clamp((dist - 2.5) / 5.5, 0.0, 1.0) * ground_proximity;

        // Diffusion dorée vers le soleil en journée, brume nocturne sombre la nuit
        let forward_scatter = pow(max(dot(view_dir, -l), 0.0), 3.0);
        let night_fog = vec3<f32>(0.015, 0.02, 0.04);
        let fog_ambient = mix(night_fog, vec3<f32>(0.72, 0.80, 0.90), day_factor);
        let fog_sun = mix(night_fog, vec3<f32>(1.0, 0.85, 0.60), day_factor);
        let fog_color = mix(fog_ambient, fog_sun, forward_scatter * 0.55 * day_factor);

        lit_rgb = mix(lit_rgb, fog_color, fog_factor * 0.75);
    }

    return vec4<f32>(lit_rgb, 1.0);
}

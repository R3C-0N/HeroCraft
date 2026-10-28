// =========================================================================
// HeroCraft - Terrain & Voxel Surface Shader (WGSL)
// - Éclairage Half-Lambert & normales géométriques nettes
// - Interpolation douce des couleurs et de l'Ambient Occlusion (AO)
// - Reflets spéculaires intenses de l'eau (Blinn-Phong + Fresnel) et scintillement de neige
// - Lumière du soleil et du ciel tirées de l'atmosphère physique (atmosphere.wgsl)
// - Perspective aérienne : diffusion et extinction de l'air entre la caméra et la surface
// - Dithered Cross-Fade (tramage stochastique Bayer 4x4) pour transitions LOD
// - Ombres portées du soleil (3 cascades, PCF) et ombres mouvantes des nuages
// =========================================================================

#import herocraft::common::{camera, get_moon_data, flatten_point, rotate_by}
#import herocraft::atmosphere::{sun_light_at, sky_light, aerial_perspective_screen, apply_aerial_perspective}
#import herocraft::shadow::{shadow, shadow_maps, shadow_sampler, cloud_shadow_map, cloud_sampler, to_shadow_uv}

// Part de la lumière du soleil qui atteint le point : 1 = plein soleil, 0 = ombre complète.
// `n` est la normale géométrique de la facette, `l` la direction du soleil.
fn sun_visibility(world_pos: vec3<f32>, n: vec3<f32>, l: vec3<f32>) -> f32 {
    if (shadow.params.z < 0.5) {
        return 1.0;
    }

    // Une face tournée à l'opposé du soleil est dans sa propre ombre : la carte n'est pas consultée.
    // Le décalage le long de la normale l'aurait sinon sortie du bloc, et le haut de la face
    // (près de l'arête) aurait vu le soleil par-dessus le bloc.
    let ndotl = dot(n, l);
    let facing = smoothstep(-0.02, 0.10, ndotl);
    if (facing <= 0.0) {
        return 0.0;
    }
    // Décalage seulement utile sur les faces rasantes (acné), nul face au soleil
    let slope = sqrt(clamp(1.0 - ndotl * ndotl, 0.0, 1.0));

    // Relief : première cascade qui contient le point (PCF 3x3, comparaisons filtrées)
    var relief = 1.0;
    let texel = shadow.extents.w;
    for (var i = 0; i < 3; i = i + 1) {
        let offset = n * (shadow.extents[i] * shadow.params.w * slope);
        let uvz = to_shadow_uv(shadow.cascades[i] * vec4<f32>(world_pos + offset, 1.0));
        let edge = max(abs(uvz.x - 0.5), abs(uvz.y - 0.5)) * 2.0;
        if (edge < 0.98 && uvz.z > 0.0 && uvz.z < 1.0) {
            var sum = 0.0;
            for (var y = -1; y <= 1; y = y + 1) {
                for (var x = -1; x <= 1; x = x + 1) {
                    let uv = uvz.xy + vec2<f32>(f32(x), f32(y)) * texel;
                    sum += textureSampleCompareLevel(shadow_maps, shadow_sampler, uv, i, uvz.z - 0.0002);
                }
            }
            relief = sum / 9.0;
            // Fondu au bord de la dernière cascade : pas de limite nette au loin
            if (i == 2) {
                relief = mix(relief, 1.0, smoothstep(0.80, 0.98, edge));
            }
            break;
        }
    }

    // Nuages : opacité filtrée sur 5 points pour des bords doux
    let cuv = to_shadow_uv(shadow.cloud_view_proj * vec4<f32>(world_pos, 1.0)).xy;
    var clouds = 0.0;
    if (all(cuv > vec2<f32>(0.0)) && all(cuv < vec2<f32>(1.0))) {
        let d = 1.5 / vec2<f32>(textureDimensions(cloud_shadow_map));
        clouds = textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv, 0.0).r * 0.4;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(d.x, d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(-d.x, d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(d.x, -d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(-d.x, -d.y), 0.0).r * 0.15;
    }

    return mix(1.0, relief * facing, shadow.params.x) * (1.0 - clouds * shadow.params.y);
}

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
    // Du point vers la caméra, dans le repère vrai du point (planète aplatie : common.wgsl)
    @location(3) to_eye: vec3<f32>,
    // Position à l'écran (clip) où le point est dessiné
    @location(4) screen_clip: vec4<f32>,
    // Rotation du repère vrai du point vers son repère dessiné (planète aplatie)
    @location(5) flat_rot: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Éclairage et ombres dans la vraie géométrie, dessin sur la planète aplatie
    let flat = flatten_point(in.position);
    out.world_pos = in.position;
    out.normal = in.normal;
    out.color = in.color;
    out.to_eye = rotate_by(camera.camera_pos - flat.pos, -flat.rot);
    out.clip_position = camera.view_proj * vec4<f32>(flat.pos, 1.0);
    out.screen_clip = out.clip_position;
    out.flat_rot = flat.rot;
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

// Structure représentant la position et la phase de la Lune

// Calcule la position orbitale et la phase de la Lune en temps réel (identique au sky shader et renderer.rs)
// - Période orbitale : 21 minutes (1260 secondes)
// - Inclinaison orbitale lunaire : ~28.5° (0.497 rad)
// - Déphasage initial (+3.69 rad) pour garantir une pleine lune majestueuse dès la première nuit

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
    // Orientée vers la caméra : la face visible d'un solide regarde toujours l'observateur. La normale
    // de sommet n'est pas fiable sur les contremarches (parfois inversée, la face s'éclairait alors
    // côté opposé au soleil). L'eau garde la normale de sommet pour rester orientée vers le ciel.
    let facing_ref = select(in.to_eye, in.normal, is_water);
    let n_facet = select(cross_norm, -cross_norm, dot(cross_norm, facing_ref) < 0.0);

    // Normale sphérique continue idéale pour la surface de l'océan
    let n_sphere = normalize(in.world_pos);

    // Facteur d'altitude orbitale : transition fluide entre vue au sol et vue satellite
    let scale = camera.planet_radius / 25.0;
    let cam_dist = length(camera.camera_pos);
    let cam_alt = cam_dist - camera.planet_radius;
    let alt_orbit_factor = clamp((cam_alt - 3.0 * scale) / (12.0 * scale), 0.0, 1.0);
    // Au sol (FPS/TPS), lighting_params_ext.w vaut 1.0, ce qui garantit qu'on n'est pas en orbite
    let orbit_factor = min(alt_orbit_factor, 1.0 - camera.lighting_params_ext.w);

    // Pour l'eau : en vue satellite, on lisse les facettes des polygones en interpolant vers la normale sphérique,
    // garantissant un reflet spéculaire pur, doux et sans polygone visible.
    // Le sol terrestre (roche, herbe, terre, sable, neige) reste quant à lui 100% voxelisé et facetté (n_facet).
    let water_normal = normalize(mix(n_facet, n_sphere, orbit_factor));
    let n = select(n_facet, water_normal, is_water);

    // Soleil et Lune vus du point comme sur la planète aplatie : le relief lointain est éclairé
    // comme celui d'un monde presque plat, pas comme l'autre côté d'une petite planète
    let l = rotate_by(normalize(camera.light_dir), -in.flat_rot);
    let view_dir = normalize(in.to_eye);
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

    // 4. Éclairage solaire et nocturne (avec teintes chaudes au coucher et clair de lune dynamique)
    let sun_elevation = dot(n_sphere, l);
    let day_factor = clamp((sun_elevation + 0.08) / 0.24, 0.0, 1.0);
    let night_factor = 1.0 - day_factor;

    // Lumière du soleil arrivée au point à travers l'atmosphère : blanche à midi, dorée puis
    // orangée quand le trajet dans l'air s'allonge, nulle sous l'horizon
    let sun_color = sun_light_at(in.world_pos, l);

    // Multiplicateurs d'intensité interactifs pilotés par les sliders de l'interface
    let moon_slider = camera.lighting_params.x;      // Multiplicateur Lune [0.0 .. 3.0]
    let night_amb_slider = camera.lighting_params.z; // Multiplicateur Nuit / Étoiles [0.0 .. 3.0]

    // Données orbitales de la Lune (partagées avec le sky shader)
    var moon = get_moon_data(camera.time, camera.light_dir);
    moon.dir = rotate_by(moon.dir, -in.flat_rot);
    let moon_elevation = dot(n_sphere, moon.dir);
    let moon_above_horizon = clamp((moon_elevation + 0.04) / 0.18, 0.0, 1.0);
    // Intensité effective du clair de lune : présent uniquement de nuit lorsque la Lune est levée
    let moon_intensity = moon_above_horizon * moon.phase * night_factor * moon_slider;

    // Lumière directe lunaire : faisceau directionnel argenté créant du relief et des ombres nocturnes
    let ndotm = dot(n, moon.dir);
    let moon_direct = max(ndotm, 0.0);
    let moon_half_lambert = pow(clamp(ndotm * 0.5 + 0.5, 0.0, 1.0), 1.4);
    let direct_moon_level = 0.16 * mix(moon_half_lambert, moon_direct, 0.45) * moon_intensity;
    let moon_direct_color = vec3<f32>(0.72, 0.85, 1.0) * direct_moon_level;

    // Lumière ambiante :
    // - Jour et crépuscule : irradiance du ciel calculée par l'atmosphère, plus bleue sur le
    //   dessus, plus chaude sur les flancs tournés vers le soleil couchant
    // - Nuit sans lune : obscurité stellaire (modulé par night_amb_slider)
    // - Nuit sous clair de lune : lueur bleutée argentée généreuse (modulée par moon_slider)
    // Le relief voxel masque une bonne part du ciel (marches, falaises voisines) : seule une
    // fraction de l'irradiance d'un plan dégagé atteint les faces
    let sun_ambient = sky_light(in.world_pos, n, l) * 0.6;

    let starlight_ambient = vec3<f32>(0.32, 0.42, 0.70) * (0.038 * night_amb_slider); // Nuit noire sans lune
    let moonlight_ambient = vec3<f32>(0.48, 0.62, 0.94) * (0.075 * moon_intensity); // Surcroît clair de lune
    let night_ambient_total = starlight_ambient + moonlight_ambient;

    let ndotl = dot(n, l);
    let direct = max(ndotl, 0.0);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.35);
    // Lambert majoritaire : en HDR, le contraste soleil / ombre est porté par le tonemapper
    // L'eau renvoie surtout le ciel (reflets plus bas) : peu de diffus pour garder un bleu profond
    let water_diffuse = select(1.0, 0.45, is_water);
    // Ombres portées : n'atténuent que la lumière directe, le ciel continue d'éclairer
    let sun_vis = select(1.0, sun_visibility(in.world_pos, n_facet, l), day_factor > 0.0);
    let direct_level = mix(half_lambert, direct, 0.65) * water_diffuse * sun_vis;

    // Éclairage combiné : ambiance (ciel / nuit étoilée / clair de lune) + soleil direct + lune directe
    let total_light = night_ambient_total * night_factor + sun_ambient + sun_color * direct_level + moon_direct_color;

    // 5. Reflets spéculaires d'eau et de glace
    var specular_light = vec3<f32>(0.0);
    let ndoth = max(dot(n, h), 0.0);
    let ndotv = clamp(dot(n, view_dir), 0.0, 1.0);

    if (is_water) {
        // Reflet spéculaire Blinn-Phong pour l'océan
        let spec_power = mix(48.0, 96.0, orbit_factor);
        let sun_spec = pow(ndoth, spec_power) * 1.8 * sun_vis;
        // Effet Fresnel rasante
        let fresnel = pow(1.0 - ndotv, 5.0) * 0.45 + 0.05;
        // Reflet du ciel : sa lumière moyenne (irradiance d'une surface horizontale)
        let sky_reflection = sky_light(in.world_pos, n_sphere, l);

        // Reflet spéculaire lunaire sur l'océan (traînée argentée miroitante sous la Lune la nuit)
        let h_moon = normalize(moon.dir + view_dir);
        let ndoth_moon = max(dot(n, h_moon), 0.0);
        let moon_spec = pow(ndoth_moon, spec_power) * 1.8 * clamp(ndotm * 2.0, 0.0, 1.0) * moon_intensity;
        let moon_spec_color = vec3<f32>(0.75, 0.88, 1.0);
        let night_water_glint = vec3<f32>(0.25, 0.35, 0.60) * (0.04 * night_amb_slider + 0.10 * moon_intensity);
        let night_water_spec = (moon_spec_color * moon_spec + night_water_glint * fresnel) * night_factor;

        let water_spec = sun_color * sun_spec * clamp(ndotl * 2.0, 0.0, 1.0) + sky_reflection * fresnel + night_water_spec;
        specular_light += water_spec;
    } else if (is_snow) {
        // Scintillement cristallin diffus sur les névés et glaciers (soleil de jour et lune la nuit)
        let snow_glint = pow(ndoth, 24.0) * 0.40 * clamp(ndotl * 2.0, 0.0, 1.0) * sun_vis;
        let snow_moon_glint = pow(max(dot(n, normalize(moon.dir + view_dir)), 0.0), 20.0) * 0.15 * moon_intensity;
        specular_light += sun_color * snow_glint + vec3<f32>(0.92, 0.95, 1.0) * snow_moon_glint;
    }

    // Couleur éclairée de base avec Ambient Occlusion (AO) per-vertex interpolé
    var lit_rgb = base_color * total_light + specular_light;

    // 6. Perspective aérienne : l'air entre la caméra et la surface diffuse la lumière du soleil
    // et du ciel (bleu au loin, doré face au soleil couchant) et atténue la surface. Le même
    // calcul donne la couleur du ciel à l'horizon : le relief lointain s'y fond. Dans
    // l'atmosphère, elle est lue dans le volume précalculé pour la caméra.
    let dist = length(in.to_eye);
    // Air vu depuis la caméra : dans son propre repère, sans rotation
    let aerial = aerial_perspective_screen(camera.camera_pos, -view_dir, in.screen_clip.xy / in.screen_clip.w, dist, normalize(camera.light_dir), 12);
    lit_rgb = apply_aerial_perspective(lit_rgb, aerial);

    // Brume nocturne : l'air n'est plus éclairé par le soleil, une lueur bleutée argentée
    // (vers la Lune) remplace la diffusion là où l'air atténue la surface
    let haze = 1.0 - dot(aerial.transmittance, vec3<f32>(1.0 / 3.0));
    let moon_scatter = pow(max(dot(view_dir, -moon.dir), 0.0), 3.0) * moon_intensity;
    let base_night_fog = vec3<f32>(0.018, 0.024, 0.045) * night_amb_slider;
    let moonlit_fog = vec3<f32>(0.035, 0.050, 0.085) + vec3<f32>(0.05, 0.07, 0.11) * moon_scatter;
    let night_fog = mix(base_night_fog, moonlit_fog, clamp(moon_intensity, 0.0, 1.0));
    lit_rgb += night_fog * haze * night_factor;

    return vec4<f32>(lit_rgb, 1.0);
}

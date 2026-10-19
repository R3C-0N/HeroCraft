// =========================================================================
// HeroCraft - 3D Multi-Mode Cloud Shader (WGSL)
// - Mode A : Raymarching Volumétrique SDF (Style Complementary Reimagined)
//   Boîte arrondie sdRoundBox, loi de Beer-Lambert, phase Henyey-Greenstein,
//   transitions crépusculaires douces (pêche / doré / lavande).
// - Mode B : Maillage Polyédrique Adouci (Smooth Beveled Mesh)
//   Normales lissées (smooth normals), faux Subsurface Scattering,
//   adoucissement de silhouette (Fresnel falloff).
// - Mode C : Proxy Mesh Raymarché
//   Volumes englobants optimisés, raymarching ultra-rapide haute performance.
// - Mode 0 : Split-Screen Côte à Côte (Les 3 tiers à l'écran)
//   Tiers G = Mode A, Tiers C = Mode B, Tiers D = Mode C avec lignes de séparation.
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

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>, // x: u, y: v, z: bevel_mask, w: face_type
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bevel_mask: f32,
    @location(4) face_type: f32,
    @location(5) local_pos: vec3<f32>,
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
    let norm_rot = vec3<f32>(
        in.normal.x * cos_a - in.normal.z * sin_a,
        in.normal.y,
        in.normal.x * sin_a + in.normal.z * cos_a
    );

    out.world_pos = pos_rot;
    out.normal = normalize(norm_rot);
    out.uv = in.color.xy;
    out.bevel_mask = in.color.z;
    out.face_type = in.color.w;
    out.local_pos = in.position;
    out.clip_position = camera.view_proj * vec4<f32>(pos_rot, 1.0);
    return out;
}

struct MoonData {
    dir: vec3<f32>,
    phase: f32,
};

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

// Fonction SDF analytique de boîte arrondie (Style Complementary Reimagined)
fn sd_round_box(p: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec3<f32>(r);
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

// Hachage 3D rapide pour bruit de volume cotonneux
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += vec3<f32>(dot(q, q.yxz + vec3<f32>(33.33)));
    return fract((q.xxy + q.yxx) * q.zyx);
}

// Bruit 3D fluide pour texture cotonneuse volumétrique
fn noise3d(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec3<f32>(3.0) - vec3<f32>(2.0) * f);

    let n000 = hash33(i).x;
    let n100 = hash33(i + vec3<f32>(1.0, 0.0, 0.0)).x;
    let n010 = hash33(i + vec3<f32>(0.0, 1.0, 0.0)).x;
    let n110 = hash33(i + vec3<f32>(1.0, 1.0, 0.0)).x;
    let n001 = hash33(i + vec3<f32>(0.0, 0.0, 1.0)).x;
    let n101 = hash33(i + vec3<f32>(1.0, 0.0, 1.0)).x;
    let n011 = hash33(i + vec3<f32>(0.0, 1.0, 1.0)).x;
    let n111 = hash33(i + vec3<f32>(1.0, 1.0, 1.0)).x;

    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);

    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);

    return mix(nxy0, nxy1, u.z);
}

fn fbm2(p: vec3<f32>) -> f32 {
    return noise3d(p) * 0.65 + noise3d(p * 2.2) * 0.35;
}

// Fonction de phase de Henyey-Greenstein (diffusion avant/arrière)
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = pow(max(1.0 + g2 - 2.0 * g * cos_theta, 0.001), 1.5);
    return (1.0 - g2) / (4.0 * 3.14159265 * denom);
}

@fragment
fn fs_cloud(in: VertexOutput) -> @location(0) vec4<f32> {
    // Multiplicateur de densité / visibilité des nuages (0.0 en vue satellite)
    let cloud_slider = camera.lighting_params2.z;
    if (cloud_slider < 0.01) {
        discard;
    }

    // Coordonnées écran normalisées [0.0, 1.0] pour le découpage en 3 tiers
    let ndc = camera.view_proj * vec4<f32>(in.world_pos, 1.0);
    let screen_u = (ndc.x / ndc.w) * 0.5 + 0.5;

    // Détermination du mode de rendu actif :
    // 0.0 = Split-Screen Côte à côte | 1.0 = Mode A | 2.0 = Mode B | 3.0 = Mode C
    let raw_mode = camera.lighting_params2.w;
    var effective_mode: u32 = 0u;
    var is_separator: bool = false;
    var sep_glow: f32 = 0.0;

    if (raw_mode > 0.5 && raw_mode < 1.5) {
        effective_mode = 1u; // Mode A (SDF Volumétrique)
    } else if (raw_mode >= 1.5 && raw_mode < 2.5) {
        effective_mode = 2u; // Mode B (Mesh Adouci)
    } else if (raw_mode >= 2.5) {
        effective_mode = 3u; // Mode C (Proxy)
    } else {
        // Mode 0 : Split-Screen 3 colonnes
        let d1 = abs(screen_u - 0.333333);
        let d2 = abs(screen_u - 0.666667);
        if (d1 < 0.0016 || d2 < 0.0016) {
            is_separator = true;
        } else if (d1 < 0.0035 || d2 < 0.0035) {
            sep_glow = max(1.0 - d1 / 0.0035, 1.0 - d2 / 0.0035);
        }

        if (screen_u < 0.333333) {
            effective_mode = 1u; // Tiers gauche : Mode A
        } else if (screen_u < 0.666667) {
            effective_mode = 2u; // Tiers central : Mode B
        } else {
            effective_mode = 3u; // Tiers droit : Mode C
        }
    }

    // Affichage des fines lignes de séparation dorées en Split-Screen
    if (is_separator) {
        return vec4<f32>(1.0, 0.88, 0.45, 0.95);
    }

    // --- Paramètres d'éclairage communs ---
    let moon_slider = camera.lighting_params.x;
    let sun_slider = camera.lighting_params.y;
    let night_amb_slider = camera.lighting_params.z;
    let day_amb_slider = camera.lighting_params.w;

    let up = normalize(in.world_pos);
    let l_sun = normalize(camera.light_dir);
    let sun_up = dot(l_sun, up);
    let daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);

    let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
    let sunset_factor = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);

    let moon = get_moon_data(camera.time, camera.light_dir);
    let view_dir = normalize(camera.camera_pos - in.world_pos);
    let ray_dir = -view_dir;

    let sun_highlight = mix(vec3<f32>(1.0, 0.98, 0.92), vec3<f32>(1.0, 0.72, 0.42), sunset_factor);
    let ambient_day = vec3<f32>(0.74, 0.84, 0.96);
    let ambient_sunset = vec3<f32>(0.64, 0.48, 0.68);
    let cloud_ambient = mix(ambient_day, ambient_sunset, sunset_factor) * daylight * day_amb_slider;
    let night_ambient = vec3<f32>(0.04, 0.07, 0.15) * night_amb_slider * (1.0 - daylight);

    let wind_speed = 0.0035;
    let angle = camera.time * wind_speed;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let ray_dir_local = vec3<f32>(
        ray_dir.x * cos_a + ray_dir.z * sin_a,
        ray_dir.y,
        -ray_dir.x * sin_a + ray_dir.z * cos_a
    );

    let r_bot = camera.planet_radius + 8.2;
    let r_top = camera.planet_radius + 9.4;

    var final_color = vec3<f32>(0.0);
    var final_alpha = 1.0;

    // =========================================================================
    // MODE A : Raymarching Volumétrique SDF (Style Complementary Reimagined)
    // =========================================================================
    if (effective_mode == 1u) {
        let num_steps = 12;
        let ds = 0.22;
        var opt_depth = 0.0;
        var accum_light = vec3<f32>(0.0);

        let cos_theta = dot(ray_dir, l_sun);
        let p_forward = henyey_greenstein(cos_theta, 0.65);
        let p_back = henyey_greenstein(cos_theta, -0.25);
        let phase = 0.75 * p_forward + 0.25 * p_back;

        // Transitions de couleurs douces : soleil direct doré / pêche crépusculaire
        let direct_sun_col = mix(vec3<f32>(1.0, 0.98, 0.90), vec3<f32>(1.0, 0.68, 0.42), sunset_factor) * sun_slider;
        let peach_rim = mix(direct_sun_col, vec3<f32>(1.0, 0.76, 0.52), sunset_factor) * phase * (0.8 + 1.2 * sunset_factor);
        let lavender_shadow = mix(vec3<f32>(0.72, 0.84, 0.98), vec3<f32>(0.56, 0.40, 0.68), sunset_factor) * daylight * day_amb_slider;

        let ndotm = max(dot(in.normal, moon.dir), 0.0);
        let moon_scatter = vec3<f32>(0.70, 0.84, 1.0) * ndotm * moon.phase * moon_slider * (1.0 - daylight);

        for (var i = 0; i < num_steps; i = i + 1) {
            let sample_pos = in.local_pos + ray_dir_local * (f32(i) + 0.5) * ds;
            let r = length(sample_pos);
            if (r >= r_bot && r <= r_top) {
                let h = (r - r_bot) / (r_top - r_bot);
                let v_profile = 4.0 * h * (1.0 - h);

                let cell_size = 1.6;
                let cell_p = fract(sample_pos / cell_size) - 0.5;
                let box_dist = sd_round_box(cell_p * cell_size, vec3<f32>(0.46 * cell_size), 0.20 * cell_size);

                let cotton_noise = fbm2(sample_pos * 1.1) * 0.30;
                let sdf_val = box_dist - cotton_noise;
                let density = clamp(-sdf_val / 0.16, 0.0, 1.0) * v_profile * 1.6;

                if (density > 0.01) {
                    let step_opt_depth = 2.4 * density * ds;
                    opt_depth += step_opt_depth;
                    let transmittance = exp(-opt_depth);

                    let step_light = (direct_sun_col * 0.55 + peach_rim * 0.95 + lavender_shadow + moon_scatter + night_ambient) * density * ds;
                    accum_light += step_light * transmittance;

                    if (transmittance < 0.02) {
                        break;
                    }
                }
            }
        }

        final_color = accum_light;
        final_alpha = (1.0 - exp(-opt_depth)) * clamp(cloud_slider, 0.0, 1.0);
    }
    // =========================================================================
    // MODE B : Maillage Polyédrique Adouci (Smooth Beveled Mesh)
    // =========================================================================
    else if (effective_mode == 2u) {
        let norm_b = normalize(in.normal);

        // Éclairage diffus doux (Half-Lambert wrap)
        let ndotl = dot(norm_b, l_sun);
        let wrap = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.25);
        let direct_b = sun_highlight * (wrap * 0.95) * daylight * sun_slider;

        // Faux Subsurface Scattering (lumière traversant les arêtes)
        let sss_dir = normalize(l_sun + norm_b * 0.35);
        let sss_forward = max(dot(-view_dir, sss_dir), 0.0);
        let sss_factor = pow(sss_forward, 3.2) * (0.85 + 0.65 * sunset_factor);
        let sss_color = mix(vec3<f32>(1.0, 0.95, 0.88), vec3<f32>(1.0, 0.65, 0.45), sunset_factor);
        let sss_light = sss_color * sss_factor * daylight * sun_slider;

        // Adoucissement de silhouette (Fresnel falloff)
        let ndotv = max(dot(norm_b, view_dir), 0.0);
        let silhouette_falloff = pow(1.0 - ndotv, 2.6);
        let silhouette_light = mix(ambient_day, ambient_sunset, sunset_factor) * silhouette_falloff * 0.48 * daylight;

        // Ambiance douce & éclairage lunaire
        let ndotup = dot(norm_b, up);
        let ao = 0.82 + 0.18 * clamp(ndotup * 0.5 + 0.5, 0.0, 1.0);
        let ambient_b = cloud_ambient * ao;

        let moon_direct_b = max(dot(norm_b, moon.dir), 0.0) * moon.phase * moon_slider * (1.0 - daylight);
        let moon_light_b = vec3<f32>(0.70, 0.84, 1.0) * (moon_direct_b * 0.70);

        final_color = direct_b + sss_light + silhouette_light + ambient_b + moon_light_b + night_ambient;
        final_alpha = clamp(cloud_slider * (0.84 + 0.16 * ndotv), 0.0, 1.0);
    }
    // =========================================================================
    // MODE C : Proxy Mesh Raymarché (Boîtes englobantes haute performance)
    // =========================================================================
    else {
        let ds_c = 0.32;
        var trans_c = 1.0;
        var accum_c = vec3<f32>(0.0);

        let forward_c = pow(max(dot(-view_dir, l_sun), 0.0), 4.0);
        let rim_c = sun_highlight * forward_c * (0.60 + 0.60 * sunset_factor);
        let ndotm = max(dot(in.normal, moon.dir), 0.0);
        let moon_light_c = vec3<f32>(0.70, 0.84, 1.0) * ndotm * moon.phase * moon_slider * (1.0 - daylight);

        for (var k = 0; k < 4; k = k + 1) {
            let p_c = in.local_pos + ray_dir_local * (f32(k) + 0.5) * ds_c;
            let r_c = length(p_c);
            let h_c = clamp((r_c - r_bot) / (r_top - r_bot), 0.0, 1.0);
            let v_c = 4.0 * h_c * (1.0 - h_c);

            let density_c = v_c * 1.35;
            let trans_step = exp(-1.7 * density_c * ds_c);
            trans_c *= trans_step;

            let light_step_c = (sun_highlight * 0.90 * daylight * sun_slider + cloud_ambient + rim_c + moon_light_c) * density_c * ds_c;
            accum_c += light_step_c * trans_c;
        }

        final_color = accum_c + night_ambient;
        final_alpha = clamp((1.0 - trans_c) * cloud_slider, 0.0, 1.0);
    }

    // Halo subtil à la limite des colonnes en split-screen
    if (sep_glow > 0.0) {
        final_color = mix(final_color, vec3<f32>(1.0, 0.88, 0.45), sep_glow * 0.40);
    }

    return vec4<f32>(final_color, final_alpha);
}

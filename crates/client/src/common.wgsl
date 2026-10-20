// =========================================================================
// HeroCraft - Module WGSL partagé (naga_oil)
// Uniforme caméra, orbite lunaire et dôme céleste communs au ciel, au terrain et aux nuages :
// une seule définition garantit que leurs couleurs et la position de la Lune restent cohérentes.
// =========================================================================
#define_import_path herocraft::common

struct CameraUniform {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    planet_radius: f32,
    light_dir: vec3<f32>,
    time: f32,
    lighting_params: vec4<f32>,
    lighting_params_ext: vec4<f32>,
    cloud_params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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

// Clarté du jour et facteur de crépuscule vus depuis le sol, selon la hauteur du soleil
struct SkyFactors {
    daylight: f32,
    sunset: f32,
};

fn sky_factors(sun_up: f32) -> SkyFactors {
    var f: SkyFactors;
    // Transition naturelle vers la nuit dès que le soleil passe sous l'horizon
    f.daylight = clamp((sun_up + 0.06) / 0.18, 0.0, 1.0);
    // Soleil rasant l'horizon (-0.05 à +0.20)
    let sunset_raw = clamp(1.0 - abs(sun_up - 0.03) / 0.18, 0.0, 1.0);
    f.sunset = sunset_raw * clamp((sun_up + 0.06) / 0.10, 0.0, 1.0);
    return f;
}

// Couleur diurne du dôme céleste dans la direction `ray_dir` (sans disque solaire, Lune ni étoiles)
fn sky_dome(ray_dir: vec3<f32>, cam_up: vec3<f32>, l: vec3<f32>, sunset_factor: f32) -> vec3<f32> {
    let sun_slider = camera.lighting_params.y;
    let day_amb_slider = camera.lighting_params.w;

    // Dégradé de couleurs du zénith : azur le jour, saphir profond au crépuscule
    let zenith_day = vec3<f32>(0.15, 0.45, 0.88);
    let zenith_sunset = vec3<f32>(0.07, 0.13, 0.38);
    let zenith_color = mix(zenith_day, zenith_sunset, sunset_factor);

    // Horizon : clair le jour ; au crépuscule, or ambré vers le soleil
    // et Ceinture de Vénus rosée/violacée à l'opposé
    let horizon_day = vec3<f32>(0.65, 0.80, 0.96);
    let forward_sun = clamp(dot(ray_dir, l) * 0.5 + 0.5, 0.0, 1.0);
    let sunset_facing_sun = mix(vec3<f32>(1.0, 0.58, 0.15), vec3<f32>(1.0, 0.82, 0.35), pow(forward_sun, 3.0));
    let sunset_opposite_sun = vec3<f32>(0.65, 0.32, 0.48);
    let sunset_horizon = mix(sunset_opposite_sun, sunset_facing_sun, pow(forward_sun, 1.5));
    let horizon_color = mix(horizon_day, sunset_horizon, sunset_factor);

    // Dégradé vertical zénith -> horizon. clamp (et non max) : dot(cam_up, cam_up) peut valoir
    // 1.0000001 et pow d'un nombre négatif donne NaN (pixels noirs)
    let ray_h = clamp(dot(ray_dir, cam_up), 0.0, 1.0);
    let horizon_blend = pow(1.0 - ray_h, 2.5);
    let day_sky = mix(zenith_color, horizon_color, horizon_blend) * day_amb_slider;

    // Halo solaire diffus, renforcé au lever/coucher
    let active_glow_color = mix(vec3<f32>(0.40, 0.35, 0.22), vec3<f32>(1.0, 0.52, 0.15), sunset_factor);
    let glow = pow(clamp(dot(ray_dir, l), 0.0, 1.0), 6.0) * active_glow_color * (1.0 + sunset_factor * 1.8) * sun_slider;

    return day_sky + glow;
}

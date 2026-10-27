// =========================================================================
// HeroCraft - Précalcul des tables de l'atmosphère (atmosphere.rs)
// Recalculées seulement quand la planète ou les réglages de l'atmosphère changent.
// =========================================================================

#import herocraft::atmosphere::{
    atmosphere, sample_medium, ray_sphere, transmittance_params, transmittance_to_top, planet_shadow,
    texel_to_unit, lut_bottom, lut_top, thickness, multiscatter_at, rayleigh_phase, mie_phase,
    PI, TRANSMITTANCE_LUT_SIZE, MULTISCATTER_LUT_SIZE, IRRADIANCE_LUT_SIZE
}

// Les tables sont calculées sur la planète virtuelle (lut_bottom, lut_top, voir atmosphere.wgsl) :
// une altitude y vaut |p| - lut_bottom()

struct FullscreenOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullscreenOutput {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullscreenOutput;
    out.position = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

// Transmittance : épaisseur optique le long du rayon jusqu'au haut de l'atmosphère
@fragment
fn fs_transmittance(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let params = transmittance_params(in.position.xy / TRANSMITTANCE_LUT_SIZE);
    let r = params.x;
    let mu = params.y;
    let origin = vec3<f32>(0.0, r, 0.0);
    let dir = vec3<f32>(sqrt(max(1.0 - mu * mu, 0.0)), mu, 0.0);
    let t_end = max(ray_sphere(origin, dir, lut_top()).y, 0.0);

    let steps = 40;
    let dt = t_end / f32(steps);
    var depth = vec3<f32>(0.0);
    for (var i = 0; i < steps; i = i + 1) {
        let p = origin + dir * ((f32(i) + 0.5) * dt);
        depth += sample_medium(length(p) - lut_bottom()).extinction * dt;
    }
    return vec4<f32>(exp(-depth), 1.0);
}

// Point et direction du soleil d'un texel indexé par (cosinus zénithal du soleil, altitude)
struct AltitudeTexel {
    origin: vec3<f32>,
    sun_dir: vec3<f32>,
};

fn altitude_texel(pixel: vec2<f32>, size: f32) -> AltitudeTexel {
    let mu_s = texel_to_unit(pixel.x / size, size) * 2.0 - 1.0;
    let x_r = texel_to_unit(pixel.y / size, size);
    let r = lut_bottom() + mix(1e-3, thickness() - 1e-3, clamp(x_r, 0.0, 1.0));
    var t: AltitudeTexel;
    t.origin = vec3<f32>(0.0, r, 0.0);
    t.sun_dir = vec3<f32>(sqrt(max(1.0 - mu_s * mu_s, 0.0)), mu_s, 0.0);
    return t;
}

// Direction n°i parmi `count` réparties uniformément sur la sphère (spirale de Fibonacci)
fn sphere_direction(i: i32, count: i32) -> vec3<f32> {
    let y = 1.0 - 2.0 * (f32(i) + 0.5) / f32(count);
    let ring = sqrt(max(1.0 - y * y, 0.0));
    let phi = f32(i) * 2.39996323;
    return vec3<f32>(cos(phi) * ring, y, sin(phi) * ring);
}

// Diffusion multiple (Hillaire 2020, §5.5) : lumière du second ordre L2 et part f_ms rediffusée
// à chaque ordre, somme géométrique L2 / (1 - f_ms) des ordres suivants
@fragment
fn fs_multiscatter(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let texel = altitude_texel(in.position.xy, MULTISCATTER_LUT_SIZE);
    let dir_count = 64;
    let steps = 20;
    let isotropic = 1.0 / (4.0 * PI);
    var l2 = vec3<f32>(0.0);
    var f_ms = vec3<f32>(0.0);

    for (var d = 0; d < dir_count; d = d + 1) {
        let dir = sphere_direction(d, dir_count);
        let top = ray_sphere(texel.origin, dir, lut_top());
        let ground = ray_sphere(texel.origin, dir, lut_bottom());
        let hits_ground = ground.x > 0.0;
        let t_end = select(max(top.y, 0.0), ground.x, hits_ground);
        let dt = t_end / f32(steps);

        var lum = vec3<f32>(0.0);
        var fms = vec3<f32>(0.0);
        var trans = vec3<f32>(1.0);
        for (var i = 0; i < steps; i = i + 1) {
            let p = texel.origin + dir * ((f32(i) + 0.5) * dt);
            let r = length(p);
            let h = r - lut_bottom();
            let mu_s = dot(p / r, texel.sun_dir);
            let medium = sample_medium(h);
            let scattering = medium.rayleigh + medium.mie;
            let sun_t = transmittance_to_top(h, mu_s) * planet_shadow(h, mu_s);
            let step_t = exp(-medium.extinction * dt);
            let inv_ext = 1.0 / max(medium.extinction, vec3<f32>(1e-7));
            let s = scattering * isotropic * sun_t;
            lum += trans * (s - s * step_t) * inv_ext;
            fms += trans * (scattering - scattering * step_t) * inv_ext;
            trans *= step_t;
        }
        // Sol lambertien éclairé par le soleil
        if (hits_ground) {
            let p = texel.origin + dir * t_end;
            let mu_s = dot(normalize(p), texel.sun_dir);
            let sun_t = transmittance_to_top(0.0, mu_s) * planet_shadow(0.0, mu_s);
            lum += trans * sun_t * max(mu_s, 0.0) * atmosphere.ground_albedo / PI;
        }
        // Phase isotrope et angle solide 4π / dir_count
        l2 += lum * (4.0 * PI / f32(dir_count)) * isotropic;
        f_ms += fms * (4.0 * PI / f32(dir_count)) * isotropic;
    }
    let psi = l2 / max(vec3<f32>(1.0) - f_ms, vec3<f32>(1e-3));
    return vec4<f32>(psi, 1.0);
}

// Diffusion simple et multiple le long d'un rayon de la planète virtuelle (radiance pour un
// soleil unitaire), avec le rebond du sol lambertien si le rayon le touche
fn sky_radiance(origin: vec3<f32>, dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let top = ray_sphere(origin, dir, lut_top());
    let ground = ray_sphere(origin, dir, lut_bottom());
    let hits_ground = ground.x > 0.0;
    let t_end = select(max(top.y, 0.0), ground.x, hits_ground);
    let steps = 16;
    let dt = t_end / f32(steps);
    let cos_theta = dot(dir, sun_dir);
    let phase_r = rayleigh_phase(cos_theta);
    let phase_m = mie_phase(cos_theta);
    var lum = vec3<f32>(0.0);
    var trans = vec3<f32>(1.0);
    for (var i = 0; i < steps; i = i + 1) {
        let p = origin + dir * ((f32(i) + 0.5) * dt);
        let r = length(p);
        let h = r - lut_bottom();
        let mu_s = dot(p / r, sun_dir);
        let medium = sample_medium(h);
        let sun_t = transmittance_to_top(h, mu_s) * planet_shadow(h, mu_s);
        let ms = multiscatter_at(h, mu_s);
        let s = medium.rayleigh * (phase_r * sun_t + ms) + medium.mie * (phase_m * sun_t + ms);
        let step_t = exp(-medium.extinction * dt);
        lum += trans * (s - s * step_t) / max(medium.extinction, vec3<f32>(1e-7));
        trans *= step_t;
    }
    if (hits_ground) {
        let mu_s = dot(normalize(origin + dir * t_end), sun_dir);
        let sun_t = transmittance_to_top(0.0, mu_s) * planet_shadow(0.0, mu_s);
        lum += trans * sun_t * max(mu_s, 0.0) * atmosphere.ground_albedo / PI;
    }
    return lum;
}

// Irradiance du ciel pour trois normales (dessus, flanc vers le soleil, flanc opposé),
// rangées côte à côte. Le rebond du sol est compté pour les directions sous l'horizon.
@fragment
fn fs_irradiance(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let slot = floor(in.position.x / IRRADIANCE_LUT_SIZE);
    let pixel = vec2<f32>(in.position.x - slot * IRRADIANCE_LUT_SIZE, in.position.y);
    let texel = altitude_texel(pixel, IRRADIANCE_LUT_SIZE);
    let sun_h = vec3<f32>(1.0, 0.0, 0.0);
    var normal = vec3<f32>(0.0, 1.0, 0.0);
    if (slot > 1.5) {
        normal = -sun_h;
    } else if (slot > 0.5) {
        normal = sun_h;
    }

    let dir_count = 96;
    var irradiance = vec3<f32>(0.0);
    for (var d = 0; d < dir_count; d = d + 1) {
        let dir = sphere_direction(d, dir_count);
        let cos_n = dot(dir, normal);
        if (cos_n > 0.0) {
            irradiance += sky_radiance(texel.origin, dir, texel.sun_dir) * cos_n;
        }
    }
    return vec4<f32>(irradiance * (4.0 * PI / f32(dir_count)), 1.0);
}

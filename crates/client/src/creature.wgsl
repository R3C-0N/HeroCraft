// =========================================================================
// HeroCraft - Créatures
// Modèles texturés (texels nets, sans filtrage), éclairés comme le relief : soleil et ciel tirés
// de l'atmosphère, ombres du relief et des nuages, ambiance de nuit, perspective aérienne.
// Les os sont déjà appliqués (creatures.rs) : les sommets arrivent dans le repère du monde.
// =========================================================================

#import herocraft::common::{camera, flatten_point, rotate_by, screen_ndc}
#import herocraft::atmosphere::{sun_light_at, sky_light, aerial_perspective_screen, apply_aerial_perspective}
#import herocraft::shadow::sun_visibility

@group(2) @binding(0) var creature_texture: texture_2d<f32>;
@group(2) @binding(1) var creature_sampler: sampler;

// Mêmes tampons que le relief (planet_mesh.rs) : position, puis attributs tassés sur 8 bits
// (la normale n'est pas lue : celle de la facette sert), puis coordonnées de texture
struct VertexInput {
    @location(0) position: vec3<f32>,
    // Racine carrée de la teinte multipliée à la texture (blanc, ou gris pour une espèce sans modèle)
    @location(2) color: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
    // Du point vers la caméra, dans le repère vrai du point (planète aplatie : common.wgsl)
    @location(3) to_eye: vec3<f32>,
    // Direction du Soleil vue du point comme sur la planète aplatie, tournée par sommet
    @location(4) light_dir: vec3<f32>,
};

@vertex
fn vs_creature(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let flat = flatten_point(in.position);
    out.world_pos = in.position;
    out.color = vec4<f32>(in.color.rgb * in.color.rgb, 1.0);
    out.uv = in.uv;
    out.to_eye = rotate_by(camera.camera_pos - flat.pos, -flat.rot);
    out.clip_position = camera.view_proj * vec4<f32>(flat.pos, 1.0);
    out.light_dir = rotate_by(normalize(camera.light_dir), -flat.rot);
    return out;
}

@fragment
fn fs_creature(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(creature_texture, creature_sampler, in.uv);
    // Pièces découpées dans la texture (seuil des matériaux du bestiaire)
    if (texel.a < 0.5) {
        discard;
    }
    let albedo = texel.rgb * in.color.rgb;

    // Normale de la facette, tournée vers l'œil : des pavés nets, comme les blocs
    let cross_norm = normalize(cross(dpdx(in.world_pos), dpdy(in.world_pos)));
    let n = select(cross_norm, -cross_norm, dot(cross_norm, in.to_eye) < 0.0);
    let n_sphere = normalize(in.world_pos);
    let l = normalize(in.light_dir);
    let view_dir = normalize(in.to_eye);

    // Jour et nuit, comme le relief
    let day_factor = clamp((dot(n_sphere, l) + 0.08) / 0.24, 0.0, 1.0);
    let night_factor = 1.0 - day_factor;
    let night_ambient = vec3<f32>(0.32, 0.42, 0.70) * (0.038 * camera.lighting_params.z);

    let ndotl = dot(n, l);
    let half_lambert = pow(clamp(ndotl * 0.5 + 0.5, 0.0, 1.0), 1.35);
    let sun_vis = select(1.0, sun_visibility(in.world_pos, n, l), day_factor > 0.0);
    let direct_level = mix(half_lambert, max(ndotl, 0.0), 0.65) * sun_vis;
    let light = night_ambient * night_factor + sky_light(in.world_pos, n, l) * 0.6 + sun_light_at(in.world_pos, l) * direct_level;
    var lit_rgb = albedo * light;

    let dist = length(in.to_eye);
    let aerial = aerial_perspective_screen(camera.camera_pos, -view_dir, screen_ndc(in.clip_position.xy), dist, normalize(camera.light_dir), 12);
    lit_rgb = apply_aerial_perspective(lit_rgb, aerial);
    return vec4<f32>(lit_rgb, 1.0);
}

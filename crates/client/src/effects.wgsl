// =========================================================================
// HeroCraft - Effets des compétences (effects.rs)
// Un quadrilatère par instance (6 sommets), calculé sur le CPU : centre et deux demi-axes.
// La forme se dessine ici : point texturé, anneau, arc balayé, dôme, faisceau, disque.
// Couleur prémultipliée : une couche qui luit s'ajoute à la lumière (alpha 0), un voile occulte.
// =========================================================================

#import herocraft::common::{camera, flatten_point}

@group(1) @binding(0) var textures: texture_2d_array<f32>;
@group(1) @binding(1) var texture_sampler: sampler;

const KIND_SPRITE: u32 = 0u;
const KIND_RING: u32 = 1u;
const KIND_ARC: u32 = 2u;
const KIND_DOME: u32 = 3u;
const KIND_BEAM: u32 = 4u;
const KIND_DISC: u32 = 5u;
const NO_TEXTURE: u32 = 255u;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // Coordonnées dans le quadrilatère (-1 à 1)
    @location(0) quad: vec2<f32>,
    @location(1) color: vec4<f32>,
    // x : forme, y : couche de texture
    @location(2) @interpolate(flat) kind: vec2<u32>,
    // x : paramètre 0, y : paramètre 1, z : part ajoutée, w : rotation
    @location(3) params: vec4<f32>,
    // Demi-angle d'un arc
    @location(4) half_angle: f32,
};

@vertex
fn vs_effect(
    @builtin(vertex_index) vi: u32,
    @location(0) center: vec4<f32>,
    @location(1) axis_u: vec4<f32>,
    @location(2) axis_v: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) extra: vec4<f32>,
) -> VertexOutput {
    var q = vec2<f32>(-1.0, -1.0);
    switch vi {
        case 1u: { q = vec2<f32>(1.0, -1.0); }
        case 2u: { q = vec2<f32>(1.0, 1.0); }
        case 3u: { q = vec2<f32>(-1.0, -1.0); }
        case 4u: { q = vec2<f32>(1.0, 1.0); }
        case 5u: { q = vec2<f32>(-1.0, 1.0); }
        default: {}
    }
    let p = center.xyz + axis_u.xyz * q.x + axis_v.xyz * q.y;
    var out: VertexOutput;
    out.quad = q;
    out.color = color;
    out.kind = vec2<u32>(u32(center.w + 0.5), u32(extra.x + 0.5));
    out.params = vec4<f32>(axis_u.w, axis_v.w, extra.y, extra.z);
    out.half_angle = extra.z;
    out.clip_position = camera.view_proj * vec4<f32>(flatten_point(p).pos, 1.0);
    return out;
}

fn sample_layer(layer: u32, q: vec2<f32>, rotation: f32) -> vec4<f32> {
    let c = cos(rotation);
    let s = sin(rotation);
    let r = vec2<f32>(c * q.x - s * q.y, s * q.x + c * q.y);
    let uv = clamp(r * 0.5 + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    return textureSampleLevel(textures, texture_sampler, uv, i32(layer), 0.0);
}

@fragment
fn fs_effect(in: VertexOutput) -> @location(0) vec4<f32> {
    let q = in.quad;
    let d = length(q);
    let kind = in.kind.x;
    let layer = in.kind.y;
    var shape = 0.0;
    var tint = vec3<f32>(1.0);
    switch kind {
        case KIND_SPRITE: {
            if (layer == NO_TEXTURE) {
                shape = 1.0 - smoothstep(0.2, 1.0, d);
            } else {
                // Les arêtes du carré s'effacent : la texture ne doit pas montrer son bord
                let t = sample_layer(layer, q, in.params.w);
                shape = t.a * (1.0 - smoothstep(0.92, 1.0, max(abs(q.x), abs(q.y))));
                tint = t.rgb;
            }
        }
        case KIND_RING: {
            let inner = in.params.x;
            let mid = (inner + 1.0) * 0.5;
            let half = max((1.0 - inner) * 0.5, 0.01);
            shape = pow(max(1.0 - abs(d - mid) / half, 0.0), 1.5);
            if (layer != NO_TEXTURE) {
                shape *= sample_layer(layer, q, in.params.w).a;
            }
        }
        case KIND_ARC: {
            let inner = in.params.x;
            let mid = (inner + 1.0) * 0.5;
            let half = max((1.0 - inner) * 0.5, 0.01);
            let band = pow(max(1.0 - abs(d - mid) / half, 0.0), 1.2);
            // 0 devant, positif à droite ; le coup avance de la droite vers la gauche
            let a = atan2(q.x, q.y);
            let edge = -in.params.y;
            let inside = step(edge, a) * step(abs(a), in.half_angle);
            // Plus vif au bord qui avance, effacé aux extrémités de l'arc
            let lead = 1.0 - clamp((a - edge) / max(2.0 * in.half_angle, 1e-3), 0.0, 1.0);
            let ends = 1.0 - smoothstep(in.half_angle * 0.8, in.half_angle, abs(a));
            shape = band * inside * mix(0.45, 1.0, lead) * mix(0.3, 1.0, ends);
        }
        case KIND_DOME: {
            // Une bulle : transparente au milieu, vive au bord
            let rim = pow(clamp(d, 0.0, 1.0), 3.0);
            shape = mix(0.18, 1.0, rim) * (1.0 - smoothstep(0.94, 1.0, d));
        }
        case KIND_BEAM: {
            let across = pow(max(1.0 - abs(q.x), 0.0), 1.5);
            let along = 1.0 - smoothstep(0.75, 1.0, abs(q.y));
            shape = across * along;
        }
        case KIND_DISC: {
            shape = 1.0 - smoothstep(0.7, 1.0, d);
            if (layer != NO_TEXTURE) {
                let t = sample_layer(layer, q, in.params.w);
                shape *= t.a;
                tint = t.rgb;
            }
        }
        default: {}
    }
    let a = clamp(in.color.a * shape, 0.0, 1.0);
    if (a <= 0.002) {
        discard;
    }
    let rgb = in.color.rgb * tint * a;
    return vec4<f32>(rgb, a * (1.0 - in.params.z));
}

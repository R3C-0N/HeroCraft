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
// Pierre du Néant posée : le quadrilatère est tourné vers l'œil par le vertex shader, et le
// fragment retrouve le cristal par lancer de rayon dans le repère du bloc
const KIND_VOID: u32 = 6u;
// Rayon de la sphère qui enveloppe le cristal, en hauteurs de bloc
const STONE_RADIUS: f32 = 0.6;
// Demi-largeur et demi-hauteur de la fenêtre, en blocs (le cristal fait 5/16 et 8/16)
const STONE_HALF_U: f32 = 0.3125;
const STONE_HALF_V: f32 = 0.5;
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
    // Position dans le monde (avant aplatissement), pour la fenêtre sur le néant
    @location(5) world: vec3<f32>,
    // Pierre du Néant : coin du bloc et lignes de l'inverse de son repère (arêtes du bloc)
    @location(6) @interpolate(flat) stone_origin: vec3<f32>,
    @location(7) @interpolate(flat) stone_r0: vec3<f32>,
    @location(8) @interpolate(flat) stone_r1: vec3<f32>,
    @location(9) @interpolate(flat) stone_r2: vec3<f32>,
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
    var p = center.xyz + axis_u.xyz * q.x + axis_v.xyz * q.y;
    var out: VertexOutput;
    out.stone_origin = vec3<f32>(0.0);
    out.stone_r0 = vec3<f32>(0.0);
    out.stone_r1 = vec3<f32>(0.0);
    out.stone_r2 = vec3<f32>(0.0);
    if (u32(center.w + 0.5) == KIND_VOID) {
        // Arêtes du bloc : x et z dans les demi-axes, le haut dans la couleur
        let ax = axis_u.xyz;
        let ay = color.xyz;
        let az = axis_v.xyz;
        let c = center.xyz;
        let to_c = c - camera.camera_pos;
        let dist = length(to_c);
        let f = to_c / max(dist, 1e-6);
        let radius = STONE_RADIUS * length(ay);
        var v = normalize(ay) - f * dot(normalize(ay), f);
        if (length(v) < 1e-4) {
            v = normalize(ax);
        }
        v = normalize(v);
        let u = cross(v, f);
        // Devant le cristal, assez grand pour couvrir sa silhouette ; rien si l'œil est dedans
        let size = select(0.0, radius, dist > radius * 1.2);
        p = c - f * radius + (u * q.x + v * q.y) * size;
        let det = dot(ax, cross(ay, az));
        out.stone_origin = c - (ax + ay + az) * 0.5;
        out.stone_r0 = cross(ay, az) / det;
        out.stone_r1 = cross(az, ax) / det;
        out.stone_r2 = cross(ax, ay) / det;
    }
    out.quad = q;
    out.color = color;
    out.kind = vec2<u32>(u32(center.w + 0.5), u32(extra.x + 0.5));
    out.params = vec4<f32>(axis_u.w, axis_v.w, extra.y, extra.z);
    out.half_angle = extra.z;
    out.world = p;
    out.clip_position = camera.view_proj * vec4<f32>(flatten_point(p).pos, 1.0);
    return out;
}

fn void_hash(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yxz + 33.33);
    return fract((r.x + r.y) * r.z);
}

fn void_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(void_hash(i), void_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x), mix(void_hash(i + vec3<f32>(0.0, 1.0, 0.0)), void_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(void_hash(i + vec3<f32>(0.0, 0.0, 1.0)), void_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x), mix(void_hash(i + vec3<f32>(0.0, 1.0, 1.0)), void_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

// Le néant vu dans la direction `dir` : un noir profond, des brumes violettes qui dérivent et
// des étoiles qui scintillent. Ne dépend que de la direction : il reste fixe dans le monde, et
// on le voit défiler derrière la Pierre en tournant autour, comme par une fenêtre.
fn void_sky(dir: vec3<f32>, time: f32) -> vec3<f32> {
    var c = vec3<f32>(0.004, 0.0, 0.012);
    let drift = vec3<f32>(time * 0.03, time * 0.017, -time * 0.021);
    let mist = void_noise(dir * 3.0 + drift) * 0.6 + void_noise(dir * 7.0 - drift) * 0.4;
    c += vec3<f32>(0.10, 0.02, 0.22) * pow(mist, 3.0);
    for (var k = 0; k < 2; k++) {
        let scale = select(90.0, 45.0, k == 0);
        let p = dir * scale;
        let cell = floor(p);
        let h = void_hash(cell + f32(k) * 17.0);
        if (h > 0.93) {
            let star = cell + 0.5 + (vec3<f32>(void_hash(cell + 3.1), void_hash(cell + 7.7), void_hash(cell + 11.3)) - 0.5) * 0.5;
            let d = length(p - star);
            let twinkle = 0.6 + 0.4 * sin(time * (2.0 + h * 5.0) + h * 40.0);
            let tint = mix(vec3<f32>(0.75, 0.6, 1.0), vec3<f32>(1.0), h);
            c += tint * (1.0 - smoothstep(0.0, 0.22, d)) * twinkle * select(1.6, 3.0, k == 0);
        }
    }
    return c;
}

// La fenêtre sur le néant d'une Pierre posée, comme dans la maquette validée : le rayon de l'œil
// retrouve la face du cristal qu'il touche ; chaque texel de cette face prend sa part d'un
// losange tourné vers l'œil. Dedans, le néant ; autour, une ombre qui boit la lumière du cristal
// et s'efface en fondu, au bord qui ondule. La coque elle-même vient du terrain, dessous.
fn stone_window(in: VertexOutput) -> vec4<f32> {
    let time = in.params.x;
    let dir = normalize(in.world - camera.camera_pos);
    let o = camera.camera_pos - in.stone_origin;
    // Repère du bloc, en seizièmes : le cristal y est fait de boîtes alignées
    let ro = vec3<f32>(dot(in.stone_r0, o), dot(in.stone_r1, o), dot(in.stone_r2, o)) * 16.0;
    let rd = vec3<f32>(dot(in.stone_r0, dir), dot(in.stone_r1, dir), dot(in.stone_r2, dir)) * 16.0;
    // Les boîtes de `assets/blocks/voidStonePlaced.json`
    var lo = array<vec3<f32>, 8>(
        vec3<f32>(6.0, 0.0, 6.0), vec3<f32>(5.0, 2.0, 5.0), vec3<f32>(4.0, 4.0, 4.0), vec3<f32>(3.0, 6.0, 3.0),
        vec3<f32>(3.0, 8.0, 3.0), vec3<f32>(4.0, 10.0, 4.0), vec3<f32>(5.0, 12.0, 5.0), vec3<f32>(6.0, 14.0, 6.0),
    );
    var hi = array<vec3<f32>, 8>(
        vec3<f32>(10.0, 2.0, 10.0), vec3<f32>(11.0, 4.0, 11.0), vec3<f32>(12.0, 6.0, 12.0), vec3<f32>(13.0, 8.0, 13.0),
        vec3<f32>(13.0, 10.0, 13.0), vec3<f32>(12.0, 12.0, 12.0), vec3<f32>(11.0, 14.0, 11.0), vec3<f32>(10.0, 16.0, 10.0),
    );
    let inv = 1.0 / select(rd, vec3<f32>(1e-6), abs(rd) < vec3<f32>(1e-6));
    var best = 1e30;
    var axis = -1;
    for (var i = 0; i < 8; i++) {
        let t0 = (lo[i] - ro) * inv;
        let t1 = (hi[i] - ro) * inv;
        let mn = min(t0, t1);
        let mx = max(t0, t1);
        let tn = max(max(mn.x, mn.y), mn.z);
        let tf = min(min(mx.x, mx.y), mx.z);
        if (tn < tf && tn > 0.0 && tn < best) {
            best = tn;
            axis = select(select(2, 1, mn.y >= mn.z), 0, mn.x >= mn.y && mn.x >= mn.z);
        }
    }
    if (axis < 0) {
        discard;
    }
    // Le point touché, ramené au centre de son texel sur la face
    let hit = ro + rd * best;
    var snapped = floor(hit + 1e-3) + 0.5;
    if (axis == 0) {
        snapped.x = hit.x;
    } else if (axis == 1) {
        snapped.y = hit.y;
    } else {
        snapped.z = hit.z;
    }
    let rel = snapped / 16.0 - vec3<f32>(0.5);
    let to_eye = normalize(-(ro / 16.0 - vec3<f32>(0.5)));
    // Losange face à l'œil, droit sur le haut du bloc
    var v = vec3<f32>(0.0, 1.0, 0.0) - to_eye * to_eye.y;
    if (length(v) < 1e-4) {
        v = vec3<f32>(0.0, 0.0, 1.0);
    }
    v = normalize(v);
    let u = cross(v, -to_eye);
    let qu = dot(rel, u) / STONE_HALF_U;
    let qv = dot(rel, v) / STONE_HALF_V;
    let m = abs(qu) + abs(qv);
    let ang = atan2(qu, qv);
    let edge = 0.66 + 0.16 * void_noise(vec3<f32>(cos(ang) * 3.0, sin(ang) * 3.0, time * 0.8));
    if (m < edge) {
        // Une lueur violette sourde monte du fond vers le bord, par paliers
        let deep = pow(clamp(floor((m - 0.15) / max(edge - 0.15, 0.01) * 4.0) / 4.0, 0.0, 1.0), 4.0);
        return vec4<f32>(void_sky(dir, time) + vec3<f32>(0.12, 0.03, 0.30) * deep, 1.0);
    }
    let x = clamp((m - edge) / 0.34, 0.0, 1.0);
    let shadow = 0.92 * (1.0 - x * x * (3.0 - 2.0 * x));
    if (shadow <= 0.002) {
        discard;
    }
    return vec4<f32>(vec3<f32>(0.01, 0.0, 0.03) * shadow, shadow);
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
    if (kind == KIND_VOID) {
        return stone_window(in);
    }
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

// =========================================================================
// HeroCraft - Cartes de papier découpé des cinématiques (paper_cards.rs)
// Une carte par instance : une grille de 8 × 8 cases, gondolée sur le GPU. La découpe vient d'une
// page de l'atlas ; un champ de distances autour d'elle dessine le liseré blanc. Le papier a son
// grain, ses fibres et son épaisseur sombre au bord ; la lumière est celle de la scène.
// =========================================================================

struct CardUniform {
    view_proj: mat4x4<f32>,
    // Direction vers la lumière (xyz), part de lumière ambiante (w)
    light_dir: vec4<f32>,
    // Couleur de la lumière, intensité comprise (rgb), temps (w)
    light_color: vec4<f32>,
    // x : taille d'un bloc, y : œil (pour retourner les cartes vues de dos) en z, w libres
    params: vec4<f32>,
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: CardUniform;
@group(0) @binding(1) var cut_outs: texture_2d_array<f32>;
@group(0) @binding(2) var outlines: texture_2d_array<f32>;
@group(0) @binding(3) var card_sampler: sampler;

const GRID: u32 = 8u;
const NO_TEXTURE: u32 = 65535u;
// Distance (en pixels de l'atlas) que couvre le champ du liseré, voir paper_cards.rs
const OUTLINE_RANGE: f32 = 16.0;
// Lumière du jour du jeu : un papier blanc au soleil y prend la clarté du sol éclairé
const LIGHT_SCALE: f32 = 2.2;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) quad: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    // x : page, y : liseré (en pixels de l'atlas), z : lueur, w : feuille de fond
    @location(4) @interpolate(flat) look: vec4<f32>,
    // Taille de la carte en blocs (grain du papier)
    @location(5) size_blocks: vec2<f32>,
    // Coordonnées dans la page de l'atlas
    @location(6) uv: vec2<f32>,
};

@vertex
fn vs_card(
    @builtin(vertex_index) vi: u32,
    @location(0) center: vec4<f32>,
    @location(1) axis_u: vec4<f32>,
    @location(2) axis_v: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) extra: vec4<f32>,
    @location(5) uv_rect: vec4<f32>,
) -> VertexOutput {
    // Case de la grille et coin de la case (deux triangles)
    let cell = vi / 6u;
    let corner = vi % 6u;
    var c = vec2<u32>(0u, 0u);
    switch corner {
        case 1u: { c = vec2<u32>(1u, 0u); }
        case 2u: { c = vec2<u32>(1u, 1u); }
        case 3u: { c = vec2<u32>(0u, 0u); }
        case 4u: { c = vec2<u32>(1u, 1u); }
        case 5u: { c = vec2<u32>(0u, 1u); }
        default: {}
    }
    let g = vec2<f32>(f32(cell % GRID + c.x), f32(cell / GRID + c.y)) / f32(GRID);
    let q = g * 2.0 - 1.0;

    let u = axis_u.xyz;
    let v = axis_v.xyz;
    let half_w = length(u);
    let n = normalize(cross(u, v));
    // Gondolement : la feuille se creuse le long de sa largeur, un peu le long de sa hauteur
    let curl = axis_u.w;
    let bend = curl * half_w * (q.x * q.x - 0.33) + curl * 0.35 * half_w * (q.y * q.y - 0.33);
    let p = center.xyz + u * q.x + v * q.y + n * bend;
    let du = u + n * (curl * half_w * 2.0 * q.x);
    let dv = v + n * (curl * 0.35 * half_w * 2.0 * q.y);

    var out: VertexOutput;
    out.quad = q;
    out.uv = mix(uv_rect.xy, uv_rect.zw, vec2<f32>(g.x, 1.0 - g.y));
    out.world_pos = p;
    out.normal = normalize(cross(du, dv));
    out.color = color;
    out.look = vec4<f32>(center.w, axis_v.w, extra.x, extra.y);
    out.size_blocks = vec2<f32>(half_w, length(v)) * 2.0 / max(scene.params.x, 1e-6);
    out.clip_position = scene.view_proj * vec4<f32>(p, 1.0);
    return out;
}

fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

// Grain du papier : un velouté fin et quelques fibres étirées
fn paper_grain(p: vec2<f32>) -> f32 {
    let fine = value_noise(p * 9.0) * 0.5 + value_noise(p * 23.0) * 0.3 + value_noise(p * 51.0) * 0.2;
    let fibres = value_noise(vec2<f32>(p.x * 3.0, p.y * 40.0) + vec2<f32>(value_noise(p * 2.0) * 4.0, 0.0));
    return (fine - 0.5) * 0.10 + smoothstep(0.82, 0.97, fibres) * 0.05;
}

// Tramage 4 × 4 : une carte qui apparaît se dessine point par point, sans tri des transparences
fn bayer(pixel: vec2<f32>) -> f32 {
    let x = u32(pixel.x) % 4u;
    let y = u32(pixel.y) % 4u;
    let m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[y * 4u + x] + 0.5) / 16.0;
}

@fragment
fn fs_card(in: VertexOutput) -> @location(0) vec4<f32> {
    let q = in.quad;
    let uv = in.uv;
    let layer = u32(in.look.x + 0.5);
    let backdrop = in.look.w > 0.5;

    var albedo = in.color.rgb;
    // Bord de la découpe : 1 au cœur, 0 hors du liseré
    var edge = 1.0;
    if (layer != NO_TEXTURE) {
        let t = textureSampleLevel(cut_outs, card_sampler, uv, i32(layer), 0.0);
        // Distance au dessin, en pixels de l'atlas
        let outside = textureSampleLevel(outlines, card_sampler, uv, i32(layer), 0.0).r * OUTLINE_RANGE;
        let border = in.look.y;
        if (t.a < 0.5) {
            if (outside > border || border <= 0.0) {
                discard;
            }
            // Le liseré : du papier blanc cassé autour de la découpe
            albedo = vec3<f32>(0.93, 0.90, 0.83);
            edge = 1.0 - outside / max(border, 1e-4);
        } else {
            albedo = t.rgb * in.color.rgb;
        }
    }

    // Une carte qui apparaît ou s'efface
    if (in.color.a < 0.999 && in.color.a <= bayer(in.clip_position.xy)) {
        discard;
    }

    let grain = paper_grain(in.quad * in.size_blocks * 0.5);
    albedo = albedo * (1.0 + grain);

    // Lumière vive : la carte brille et ne prend pas d'ombre
    let glow = in.look.z;
    if (glow > 1.0) {
        return vec4<f32>(albedo * glow * LIGHT_SCALE * 0.6, 1.0);
    }

    if (backdrop) {
        // Le fond : un papier éclairé de face, assombri vers les bords comme sous une lampe
        let vignette = 1.0 - 0.35 * smoothstep(0.35, 1.25, length(q * vec2<f32>(0.8, 1.0)));
        let light = scene.light_color.rgb * (0.55 + 0.45 * scene.light_dir.w);
        return vec4<f32>(albedo * light * LIGHT_SCALE * 0.7 * vignette, 1.0);
    }

    // Deux faces : une carte vue de dos est éclairée comme son recto
    var n = normalize(in.normal);
    if (dot(n, scene.eye.xyz - in.world_pos) < 0.0) {
        n = -n;
    }
    let l = normalize(scene.light_dir.xyz);
    let ambient = scene.light_dir.w;
    let diffuse = max(dot(n, l), 0.0);
    // L'épaisseur du papier : la tranche de la découpe un peu plus sombre
    let rim = mix(0.82, 1.0, smoothstep(0.0, 0.35, edge));
    let light = scene.light_color.rgb * (ambient + (1.0 - ambient) * diffuse);
    return vec4<f32>(albedo * light * LIGHT_SCALE * rim, 1.0);
}

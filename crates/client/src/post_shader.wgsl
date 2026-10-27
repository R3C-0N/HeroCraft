// =========================================================================
// HeroCraft - Post-traitement HDR (WGSL)
// - Bloom à la Call of Duty (Jimenez 2014) : chaîne de downsample 13 taps puis upsample en tente 3x3
// - Seuil doux (knee) et moyenne de Karis sur le premier niveau contre les lucioles
// - Composition : exposition, tonemapper (écrêtage, épaule douce, ACES, AgX) et tramage 8 bits
// =========================================================================

struct PostUniform {
    // x : exposition linéaire (2^EV), y : intensité du bloom, z : seuil, w : largeur du genou
    params: vec4<f32>,
    // x : tonemapper (0 écrêtage, 1 épaule douce, 2 ACES, 3 AgX), y : 1 si la swapchain n'encode pas en sRGB
    mode: vec4<u32>,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> post: PostUniform;
// Niveau 0 du bloom, lu seulement par la composition
@group(0) @binding(3) var bloom_tex: texture_2d<f32>;

struct FullscreenOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> FullscreenOutput {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    let p = pos[vertex_index];
    var out: FullscreenOutput;
    out.clip_pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return out;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Écarte NaN, infinis et valeurs négatives : un seul pixel invalide se propagerait à tout le bloom
fn sanitize(c: vec3<f32>) -> vec3<f32> {
    let finite = all(c == c) && all(abs(c) < vec3<f32>(65000.0));
    return select(vec3<f32>(0.0), clamp(c, vec3<f32>(0.0), vec3<f32>(256.0)), finite);
}

// Downsample 13 taps : 4 carrés de 4 texels qui se recouvrent plus un central (sans crénelage)
fn downsample13(uv: vec2<f32>, texel: vec2<f32>) -> array<vec3<f32>, 13> {
    var s: array<vec3<f32>, 13>;
    s[0] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>(-2.0, -2.0), 0.0).rgb;
    s[1] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 0.0, -2.0), 0.0).rgb;
    s[2] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 2.0, -2.0), 0.0).rgb;
    s[3] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>(-2.0,  0.0), 0.0).rgb;
    s[4] = textureSampleLevel(src_tex, src_sampler, uv, 0.0).rgb;
    s[5] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 2.0,  0.0), 0.0).rgb;
    s[6] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>(-2.0,  2.0), 0.0).rgb;
    s[7] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 0.0,  2.0), 0.0).rgb;
    s[8] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 2.0,  2.0), 0.0).rgb;
    s[9] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    s[10] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 1.0, -1.0), 0.0).rgb;
    s[11] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>(-1.0,  1.0), 0.0).rgb;
    s[12] = textureSampleLevel(src_tex, src_sampler, uv + texel * vec2<f32>( 1.0,  1.0), 0.0).rgb;
    return s;
}

fn src_texel() -> vec2<f32> {
    return 1.0 / vec2<f32>(textureDimensions(src_tex, 0));
}

// Seuil doux : seule la part au-dessus du seuil alimente le bloom, avec une transition quadratique
fn soft_threshold(c: vec3<f32>) -> vec3<f32> {
    let threshold = post.params.z;
    let knee = max(post.params.w, 1e-4);
    let brightness = max(c.r, max(c.g, c.b));
    var soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee);
    let contribution = max(soft, brightness - threshold) / max(brightness, 1e-4);
    return c * contribution;
}

// Premier niveau : seuil puis moyenne de Karis par groupe (pondérée par 1 / (1 + luminance))
@fragment
fn fs_prefilter(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let s = downsample13(in.uv, src_texel());
    var t: array<vec3<f32>, 13>;
    for (var i = 0; i < 13; i++) {
        t[i] = soft_threshold(sanitize(s[i]));
    }
    let g0 = (t[9] + t[10] + t[11] + t[12]) * 0.25;
    let g1 = (t[0] + t[1] + t[3] + t[4]) * 0.25;
    let g2 = (t[1] + t[2] + t[4] + t[5]) * 0.25;
    let g3 = (t[3] + t[4] + t[6] + t[7]) * 0.25;
    let g4 = (t[4] + t[5] + t[7] + t[8]) * 0.25;
    let w0 = 0.5 / (1.0 + luminance(g0));
    let w1 = 0.125 / (1.0 + luminance(g1));
    let w2 = 0.125 / (1.0 + luminance(g2));
    let w3 = 0.125 / (1.0 + luminance(g3));
    let w4 = 0.125 / (1.0 + luminance(g4));
    let c = (g0 * w0 + g1 * w1 + g2 * w2 + g3 * w3 + g4 * w4) / (w0 + w1 + w2 + w3 + w4);
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_downsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let s = downsample13(in.uv, src_texel());
    var c = (s[9] + s[10] + s[11] + s[12]) * 0.125;
    c += (s[0] + s[2] + s[6] + s[8]) * 0.03125;
    c += (s[1] + s[3] + s[5] + s[7]) * 0.0625;
    c += s[4] * 0.125;
    return vec4<f32>(c, 1.0);
}

// Upsample en tente 3x3, ajouté (blend additif) au niveau plus fin déjà rempli par le downsample
@fragment
fn fs_upsample(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let d = src_texel();
    var c = textureSampleLevel(src_tex, src_sampler, in.uv, 0.0).rgb * 4.0;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>(-d.x, 0.0), 0.0).rgb * 2.0;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>( d.x, 0.0), 0.0).rgb * 2.0;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>(0.0, -d.y), 0.0).rgb * 2.0;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>(0.0,  d.y), 0.0).rgb * 2.0;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>(-d.x, -d.y), 0.0).rgb;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>( d.x, -d.y), 0.0).rgb;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>(-d.x,  d.y), 0.0).rgb;
    c += textureSampleLevel(src_tex, src_sampler, in.uv + vec2<f32>( d.x,  d.y), 0.0).rgb;
    return vec4<f32>(c / 16.0, 1.0);
}

// --- Tonemappers (entrée et sortie en sRGB linéaire) ---

// Identique à l'ancien rendu sous `knee`, puis compression exponentielle vers 1.0
fn tonemap_soft_shoulder(c: vec3<f32>) -> vec3<f32> {
    let knee = 0.8;
    let over = max(c - vec3<f32>(knee), vec3<f32>(0.0));
    let compressed = knee + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));
    return select(c, vec3<f32>(compressed), c > vec3<f32>(knee));
}

// ACES ajusté par Stephen Hill (RRT + ODT), espace de travail sRGB linéaire
fn tonemap_aces(c: vec3<f32>) -> vec3<f32> {
    let input_mat = mat3x3<f32>(
        vec3<f32>(0.59719, 0.07600, 0.02840),
        vec3<f32>(0.35458, 0.90834, 0.13383),
        vec3<f32>(0.04823, 0.01566, 0.83777),
    );
    let output_mat = mat3x3<f32>(
        vec3<f32>( 1.60475, -0.10208, -0.00327),
        vec3<f32>(-0.53108,  1.10813, -0.07276),
        vec3<f32>(-0.07367, -0.00605,  1.07602),
    );
    let v = input_mat * c;
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return clamp(output_mat * (a / b), vec3<f32>(0.0), vec3<f32>(1.0));
}

// AgX (Troy Sobotka), approximation polynomiale de Benjamin Wrensch, look « punchy »
fn agx_contrast(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

fn tonemap_agx(c: vec3<f32>) -> vec3<f32> {
    let agx_mat = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let agx_mat_inv = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let min_ev = -12.47393;
    let max_ev = 4.026069;
    var v = agx_mat * max(c, vec3<f32>(1e-10));
    v = clamp(log2(v), vec3<f32>(min_ev), vec3<f32>(max_ev));
    v = (v - min_ev) / (max_ev - min_ev);
    v = agx_contrast(v);
    // Look « punchy » : saturation légèrement relevée
    let luma = luminance(v);
    v = luma + 1.4 * (v - luma);
    v = agx_mat_inv * v;
    // La courbe sort des valeurs encodées pour l'affichage : retour en linéaire pour la swapchain sRGB
    return pow(clamp(v, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Bruit de gradient entrelacé (Jimenez) : casse les bandes des dégradés du ciel en 8 bits
fn interleaved_gradient_noise(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fs_composite(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let hdr = sanitize(textureSampleLevel(src_tex, src_sampler, in.uv, 0.0).rgb);
    let bloom = textureSampleLevel(bloom_tex, src_sampler, in.uv, 0.0).rgb;
    let c = (hdr + bloom * post.params.y) * post.params.x;

    var mapped: vec3<f32>;
    switch post.mode.x {
        case 0u: { mapped = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)); }
        case 2u: { mapped = tonemap_aces(c); }
        case 3u: { mapped = tonemap_agx(c); }
        default: { mapped = tonemap_soft_shoulder(c); }
    }

    // Tramage d'un demi-pas de quantification, appliqué dans l'espace encodé
    let noise = interleaved_gradient_noise(in.clip_pos.xy) - 0.5;
    let encoded = clamp(linear_to_srgb(mapped) + noise / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
    let out_rgb = select(srgb_to_linear(encoded), encoded, post.mode.y == 1u);
    return vec4<f32>(out_rgb, 1.0);
}

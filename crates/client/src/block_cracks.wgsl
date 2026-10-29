// =========================================================================
// HeroCraft - Fissures du bloc en train de casser
// Les faces visibles du bloc, à peine décollées de lui, reçoivent des fissures sombres sur une
// grille de 16 × 16 pixels (le grain des blocs). Elles partent de quelques points de la face et
// s'étendent avec l'avancement de la casse ; le bloc s'assombrit un peu en même temps.
// =========================================================================

#import herocraft::common::{camera, flatten_point}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) face_uv: vec4<f32>,
};

@vertex
fn vs_cracks(@location(0) position: vec3<f32>, @location(1) face_uv: vec4<f32>) -> VertexOutput {
    var out: VertexOutput;
    // Même aplatissement que le relief : les fissures restent collées au bloc
    out.clip = camera.view_proj * vec4<f32>(flatten_point(position).pos, 1.0);
    out.face_uv = face_uv;
    return out;
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

@fragment
fn fs_cracks(in: VertexOutput) -> @location(0) vec4<f32> {
    let progress = clamp(in.face_uv.w, 0.0, 1.0);
    let face = in.face_uv.z;
    // Centre du pixel de bloc (16 × 16 par face)
    let pixel = (floor(clamp(in.face_uv.xy, vec2<f32>(0.0), vec2<f32>(0.999)) * 16.0) + 0.5) / 16.0;

    // Bords des cellules de Voronoï (3 × 3 cellules par face) : le tracé des fissures
    let cell_scale = 3.0;
    let p = pixel * cell_scale;
    let cell = floor(p);
    var f1 = 8.0;
    var f2 = 8.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let c = cell + vec2<f32>(f32(i), f32(j));
            let seed = c + hash2(c + vec2<f32>(face * 7.3, face * 3.1));
            let d = distance(p, seed);
            if (d < f1) {
                f2 = f1;
                f1 = d;
            } else if (d < f2) {
                f2 = d;
            }
        }
    }
    let on_crack = f2 - f1 < 0.16;

    // Les fissures gagnent la face depuis son centre, avec un peu d'irrégularité
    let wobble = hash2(floor(pixel * 16.0) + vec2<f32>(face)).x * 0.15;
    let reach = progress * 0.95 + 0.05;
    let shown = on_crack && distance(pixel, vec2<f32>(0.5)) + wobble < reach;

    let shade = 0.12 * progress;
    if (shown) {
        return vec4<f32>(0.02, 0.018, 0.015, 0.55 + 0.3 * progress);
    }
    if (shade < 0.01) {
        discard;
    }
    return vec4<f32>(0.0, 0.0, 0.0, shade);
}

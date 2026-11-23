// =========================================================================
// HeroCraft - Os des créatures appliqués sur le GPU
// Chaque fil place un sommet d'une bête : mélange de ses quatre os, puis repère de la bête dans le
// monde. Il écrit dans les tampons de sommets que lit la passe 3D (position, attributs tassés,
// coordonnées de texture), comme le faisait `creatures::build_frame` sur le CPU (`Skinning::Cpu`,
// la référence de ce calcul).
// =========================================================================

struct ModelVertex {
    // Position de liaison (xyz) et u (w)
    position: vec4<f32>,
    // Normale de liaison (xyz) et v (w)
    normal: vec4<f32>,
    weights: vec4<f32>,
    joints: vec4<u32>,
};

// Miroir de `creatures::SkinnedInstance`
struct Instance {
    left: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    // Pieds (xyz) et hauteur des pieds du modèle (w)
    feet: vec4<f32>,
    tint: vec4<f32>,
    // Premier sommet du modèle de l'espèce, nombre de sommets, première matrice, premier sommet de sortie
    model_start: u32,
    vertex_count: u32,
    first_matrix: u32,
    first_output: u32,
};

@group(0) @binding(0) var<storage, read> model_vertices: array<ModelVertex>;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> bones: array<mat4x4<f32>>;
// Sorties : positions (3 f32 par sommet), attributs tassés (3 u32), coordonnées de texture (2 f32)
@group(0) @binding(3) var<storage, read_write> out_positions: array<f32>;
@group(0) @binding(4) var<storage, read_write> out_attributes: array<u32>;
@group(0) @binding(5) var<storage, read_write> out_uvs: array<f32>;
// Premier sommet de sortie dans les tampons (après les sommets écrits par le CPU)
struct Params {
    output_base: u32,
    untextured: u32,
    _pad0: u32,
    _pad1: u32,
};
@group(0) @binding(6) var<uniform> params: Params;

// Comme `PlanetVertex::packed_attributes` : normale et opacité en snorm8, racine de la couleur en unorm8
fn snorm8(x: f32) -> u32 {
    return u32(i32(floor((clamp(x, -1.0, 1.0) + 1.0) * 127.0 + 0.5)) - 127) & 0xffu;
}

fn unorm8(x: f32) -> u32 {
    return u32(floor(sqrt(clamp(x, 0.0, 1.0)) * 255.0 + 0.5)) & 0xffu;
}

@compute @workgroup_size(64)
fn cs_skin(@builtin(global_invocation_id) id: vec3<u32>) {
    let inst = instances[id.y];
    let k = id.x;
    if (k >= inst.vertex_count) {
        return;
    }
    let v = model_vertices[inst.model_start + k];
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var j = 0u; j < 4u; j = j + 1u) {
        let w = v.weights[j];
        if (w > 0.0) {
            let m = bones[inst.first_matrix + v.joints[j]];
            p = p + (m * vec4<f32>(v.position.xyz, 1.0)).xyz * w;
            n = n + (m * vec4<f32>(v.normal.xyz, 0.0)).xyz * w;
        }
    }
    p.y = p.y + inst.feet.w;
    let world = inst.feet.xyz + inst.left.xyz * p.x + inst.up.xyz * p.y + inst.forward.xyz * p.z;
    var normal = vec3<f32>(0.0);
    if (dot(n, n) > 0.0) {
        normal = normalize(n);
    }
    var turned = inst.left.xyz * normal.x + inst.up.xyz * normal.y + inst.forward.xyz * normal.z;
    if (dot(turned, turned) > 0.0) {
        turned = normalize(turned);
    }

    let o = params.output_base + inst.first_output + k;
    out_positions[o * 3u] = world.x;
    out_positions[o * 3u + 1u] = world.y;
    out_positions[o * 3u + 2u] = world.z;
    out_attributes[o * 3u] = snorm8(turned.x) | (snorm8(turned.y) << 8u) | (snorm8(turned.z) << 16u) | (snorm8(1.0) << 24u);
    out_attributes[o * 3u + 1u] = unorm8(inst.tint.x) | (unorm8(inst.tint.y) << 8u) | (unorm8(inst.tint.z) << 16u);
    out_attributes[o * 3u + 2u] = params.untextured;
    out_uvs[o * 2u] = v.position.w;
    out_uvs[o * 2u + 1u] = v.normal.w;
}

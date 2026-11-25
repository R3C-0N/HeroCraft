// =========================================================================
// HeroCraft - Tri des morceaux du relief sur le GPU
// Chaque fil regarde un morceau de la table du relief (`TerrainPool::set_draw_table`) pour une
// passe (la passe 3D ou une cascade d'ombre) et écrit son dessin indirect : le morceau garde une
// instance s'il peut se voir dans cette passe, aucune sinon. Mêmes tests que sur le CPU
// (`TerrainCull::is_visible`, `ShadowRenderer::touches`), qui restent la référence.
// =========================================================================

struct Record {
    // Centre (xyz) et rayon de la sphère englobante (w)
    sphere: vec4<f32>,
    // Plus grande distance d'un sommet au centre de la planète
    max_radius: f32,
    index_count: u32,
    first_index: u32,
    base_vertex: i32,
};

// Miroir de `terrain_pool::CullPass`
struct Pass {
    // 0 : passe 3D (champ de vision et horizon), 1 : cascade d'ombre, 2 : tout dessiner
    kind: u32,
    // Premier dessin de la passe dans le tampon indirect
    first_draw: u32,
    _pad0: u32,
    _pad1: u32,
    // Passe 3D : plans gauche, droite, bas, haut (normale vers l'intérieur, distance)
    planes: array<vec4<f32>, 4>,
    // Œil (xyz), niveau de la mer (w)
    eye: vec4<f32>,
    // Verticale de l'œil (xyz), rayon du grand globe (w)
    up: vec4<f32>,
    // `camera.flatten` du shader
    flatten: vec4<f32>,
    // Hauteur minimale de l'œil, marge des sphères, marge de l'horizon, angle des points rabattus
    margins: vec4<f32>,
    // Cascade : centre (xyz) et demi-côté (w), puis les deux axes du plan de la lumière
    cascade: vec4<f32>,
    right: vec4<f32>,
    light_up: vec4<f32>,
};

struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
};

struct Params {
    record_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> records: array<Record>;
@group(0) @binding(1) var<storage, read> passes: array<Pass>;
@group(0) @binding(2) var<storage, read_write> draws: array<DrawArgs>;
@group(0) @binding(3) var<uniform> params: Params;

// Copie de `flatten_point` (common.wgsl) pour la caméra de la passe
fn flatten_point(pass_: Pass, p: vec3<f32>) -> vec3<f32> {
    let extra = pass_.flatten.x;
    if (extra <= 0.0) {
        return p;
    }
    let up = pass_.up.xyz;
    let r = length(p);
    let d = p / max(r, 1e-6);
    let c = dot(up, d);
    let side = d - up * c;
    let s = length(side);
    if (s < 1e-7) {
        return p;
    }
    let t = side / s;
    let theta = atan2(s, c);
    let rho = r + extra;
    let theta2 = theta * r / rho;
    let half_angle = sin(0.5 * theta2);
    let flat_p = up * (r - 2.0 * rho * half_angle * half_angle) + t * (rho * sin(theta2));
    let a = pass_.flatten.z;
    let b = pass_.flatten.w;
    let x = clamp((theta * r - a) / max(b - a, 1e-6), 0.0, 1.0);
    let w = x * x * (3.0 - 2.0 * x);
    return mix(p, flat_p, w);
}

// `TerrainCull::is_visible`
fn camera_sees(pass_: Pass, center: vec3<f32>, radius: f32, max_radius: f32) -> bool {
    let dist = length(center);
    let up = pass_.up.xyz;
    if (dist < 1e-6 || dot(up, up) == 0.0) {
        return true;
    }
    let theta = acos(clamp(dot(up, center / dist), -1.0, 1.0));
    var spread = 3.14159265;
    if (radius < dist) {
        spread = asin(radius / dist);
    }
    let theta_min = max(theta - spread, 0.0);

    // 1. Horizon
    let sea = pass_.eye.w;
    let rho = pass_.up.w;
    let h = max(length(pass_.eye.xyz) - sea, pass_.margins.x);
    let peak = max(max_radius - sea, 0.0);
    let reach = sqrt(2.0 * rho * h + h * h) + sqrt(2.0 * rho * peak + peak * peak);
    if (theta_min * sea > reach * pass_.margins.z) {
        return false;
    }

    // 2. Champ de vision
    if (theta + spread > pass_.margins.w) {
        return true;
    }
    let drawn = flatten_point(pass_, center);
    let r = radius * pass_.margins.y;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let plane = pass_.planes[i];
        if (dot(plane.xyz, drawn) + plane.w < -r) {
            return false;
        }
    }
    return true;
}

// `ShadowRenderer::touches`
fn cascade_touches(pass_: Pass, center: vec3<f32>, radius: f32) -> bool {
    let d = center - pass_.cascade.xyz;
    let reach = pass_.cascade.w + radius;
    return abs(dot(d, pass_.right.xyz)) <= reach && abs(dot(d, pass_.light_up.xyz)) <= reach;
}

@compute @workgroup_size(64)
fn cs_cull(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = id.x;
    if (k >= params.record_count) {
        return;
    }
    let pass_ = passes[id.y];
    let record = records[k];
    var visible = true;
    if (pass_.kind == 0u) {
        visible = camera_sees(pass_, record.sphere.xyz, record.sphere.w, record.max_radius);
    } else if (pass_.kind == 1u) {
        visible = cascade_touches(pass_, record.sphere.xyz, record.sphere.w);
    }
    var args: DrawArgs;
    args.index_count = record.index_count;
    args.instance_count = select(0u, 1u, visible);
    args.first_index = record.first_index;
    args.base_vertex = record.base_vertex;
    args.first_instance = 0u;
    draws[pass_.first_draw + k] = args;
}

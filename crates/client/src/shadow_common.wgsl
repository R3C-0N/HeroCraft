// =========================================================================
// HeroCraft - Cartes d'ombre du soleil (module WGSL partagé, naga_oil)
// Cascades de profondeur du relief et carte d'opacité des nuages vues depuis le soleil
// (shadow.rs), lues par le terrain et par les rayons crépusculaires.
// =========================================================================
#define_import_path herocraft::shadow

struct ShadowUniform {
    cascades: array<mat4x4<f32>, 3>,
    cloud_view_proj: mat4x4<f32>,
    // xyz : demi-largeur monde de chaque cascade, w : taille d'un texel de cascade en uv
    extents: vec4<f32>,
    // x : force des ombres du relief, y : force des ombres des nuages, z : 1 si actives,
    // w : décalage maximal le long de la normale (face rasante), en fraction de la demi-largeur de la cascade
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> shadow: ShadowUniform;
@group(1) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(1) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(3) var cloud_shadow_map: texture_2d<f32>;
@group(1) @binding(4) var cloud_sampler: sampler;

fn to_shadow_uv(clip: vec4<f32>) -> vec3<f32> {
    let ndc = clip.xyz / clip.w;
    return vec3<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z);
}

// Le point voit-il le soleil par-dessus le relief ? Un seul test filtré (2x2) dans la première
// cascade qui le contient, 1 hors des cascades. Pour les points de l'air (rayons crépusculaires).
fn relief_light_visibility(p: vec3<f32>) -> f32 {
    for (var i = 0; i < 3; i = i + 1) {
        let uvz = to_shadow_uv(shadow.cascades[i] * vec4<f32>(p, 1.0));
        if (all(uvz.xy > vec2<f32>(0.0)) && all(uvz.xy < vec2<f32>(1.0)) && uvz.z > 0.0 && uvz.z < 1.0) {
            return textureSampleCompareLevel(shadow_maps, shadow_sampler, uvz.xy, i, uvz.z - 0.0002);
        }
    }
    return 1.0;
}

// Opacité des nuages entre le point et le soleil (0 hors de la carte)
fn cloud_opacity(p: vec3<f32>) -> f32 {
    let cuv = to_shadow_uv(shadow.cloud_view_proj * vec4<f32>(p, 1.0)).xy;
    if (all(cuv > vec2<f32>(0.0)) && all(cuv < vec2<f32>(1.0))) {
        return textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv, 0.0).r;
    }
    return 0.0;
}

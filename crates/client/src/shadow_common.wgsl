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

// Part de la lumière du soleil qui atteint le point (relief et créatures) : 1 = plein soleil, 0 = ombre complète.
// `n` est la normale géométrique de la facette, `l` la direction du soleil.
fn sun_visibility(world_pos: vec3<f32>, n: vec3<f32>, l: vec3<f32>) -> f32 {
    if (shadow.params.z < 0.5) {
        return 1.0;
    }

    // Une face tournée à l'opposé du soleil est dans sa propre ombre : la carte n'est pas consultée.
    // Le décalage le long de la normale l'aurait sinon sortie du bloc, et le haut de la face
    // (près de l'arête) aurait vu le soleil par-dessus le bloc.
    let ndotl = dot(n, l);
    let facing = smoothstep(-0.02, 0.10, ndotl);
    if (facing <= 0.0) {
        return 0.0;
    }
    // Décalage seulement utile sur les faces rasantes (acné), nul face au soleil
    let slope = sqrt(clamp(1.0 - ndotl * ndotl, 0.0, 1.0));

    // Relief : première cascade qui contient le point (PCF 3x3, comparaisons filtrées)
    var relief = 1.0;
    let texel = shadow.extents.w;
    for (var i = 0; i < 3; i = i + 1) {
        let offset = n * (shadow.extents[i] * shadow.params.w * slope);
        let uvz = to_shadow_uv(shadow.cascades[i] * vec4<f32>(world_pos + offset, 1.0));
        let edge = max(abs(uvz.x - 0.5), abs(uvz.y - 0.5)) * 2.0;
        if (edge < 0.98 && uvz.z > 0.0 && uvz.z < 1.0) {
            var sum = 0.0;
            for (var y = -1; y <= 1; y = y + 1) {
                for (var x = -1; x <= 1; x = x + 1) {
                    let uv = uvz.xy + vec2<f32>(f32(x), f32(y)) * texel;
                    sum += textureSampleCompareLevel(shadow_maps, shadow_sampler, uv, i, uvz.z - 0.0002);
                }
            }
            relief = sum / 9.0;
            // Fondu au bord de la dernière cascade : pas de limite nette au loin
            if (i == 2) {
                relief = mix(relief, 1.0, smoothstep(0.80, 0.98, edge));
            }
            break;
        }
    }

    // Nuages : opacité filtrée sur 5 points pour des bords doux
    let cuv = to_shadow_uv(shadow.cloud_view_proj * vec4<f32>(world_pos, 1.0)).xy;
    var clouds = 0.0;
    if (all(cuv > vec2<f32>(0.0)) && all(cuv < vec2<f32>(1.0))) {
        let d = 1.5 / vec2<f32>(textureDimensions(cloud_shadow_map));
        clouds = textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv, 0.0).r * 0.4;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(d.x, d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(-d.x, d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(d.x, -d.y), 0.0).r * 0.15;
        clouds += textureSampleLevel(cloud_shadow_map, cloud_sampler, cuv + vec2<f32>(-d.x, -d.y), 0.0).r * 0.15;
    }

    return mix(1.0, relief * facing, shadow.params.x) * (1.0 - clouds * shadow.params.y);
}

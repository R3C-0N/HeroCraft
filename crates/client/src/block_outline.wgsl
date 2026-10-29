// =========================================================================
// HeroCraft - Contour du bloc visé
// Les 12 arêtes du bloc sous le réticule, en rubans tournés vers la caméra (épaisseur constante à
// l'écran, calculée côté CPU). Testées en profondeur sans l'écrire : les arêtes cachées par le
// bloc lui-même ou par le relief ne se voient pas. Couleur au-delà du blanc dans la scène HDR :
// le tonemapper la rend blanche et le bloom la fait légèrement briller. Les mêmes rubans dessinent
// la silhouette des autres joueurs et des créatures.
// =========================================================================

#import herocraft::common::{camera, flatten_point}

struct OutlineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_outline(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> OutlineOut {
    var out: OutlineOut;
    // Même aplatissement que le relief : le contour reste collé au bloc
    out.clip = camera.view_proj * vec4<f32>(flatten_point(position).pos, 1.0);
    out.color = color;
    return out;
}

// Couleur de la boîte : blanc pour le bloc visé, doré pour les autres joueurs, celle de leur espèce
// pour les créatures
@fragment
fn fs_outline(in: OutlineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

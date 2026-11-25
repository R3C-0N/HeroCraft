// =========================================================================
// HeroCraft - Contour du bloc visé
// Les 12 arêtes du bloc sous le réticule, en rubans tournés vers la caméra (épaisseur constante à
// l'écran, calculée côté CPU). Testées en profondeur sans l'écrire : les arêtes cachées par le
// bloc lui-même ou par le relief ne se voient pas. Couleur au-delà du blanc dans la scène HDR :
// le tonemapper la rend blanche et le bloom la fait légèrement briller.
// =========================================================================

#import herocraft::common::{camera, flatten_point}

@vertex
fn vs_outline(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    // Même aplatissement que le relief : le contour reste collé au bloc
    return camera.view_proj * vec4<f32>(flatten_point(position).pos, 1.0);
}

@fragment
fn fs_outline() -> @location(0) vec4<f32> {
    return vec4<f32>(2.2, 2.2, 2.0, 1.0);
}

// Silhouette d'un autre joueur en ligne : dorée, au-delà du blanc elle aussi
@fragment
fn fs_player() -> @location(0) vec4<f32> {
    return vec4<f32>(2.4, 1.7, 0.4, 1.0);
}

// =========================================================================
// HeroCraft - Passe d'ombre du relief (WGSL)
// Profondeur seule vue depuis le soleil, une passe par cascade (shadow.rs)
// =========================================================================

struct ShadowPass {
    view_proj: mat4x4<f32>,
};

@group(1) @binding(0)
var<uniform> shadow_pass: ShadowPass;

@vertex
fn vs_shadow(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return shadow_pass.view_proj * vec4<f32>(position, 1.0);
}

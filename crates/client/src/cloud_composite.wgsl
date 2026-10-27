// =========================================================================
// HeroCraft - Composition des nuages calculés en demi-résolution
// - Couleur prémultipliée agrandie par filtrage bilinéaire (bords doux)
// - Profondeur la plus proche des 4 texels du filtrage, écrite dans le Z-buffer
//   pleine résolution : le relief masque toujours correctement les nuages
// =========================================================================

@group(0) @binding(0) var cloud_color: texture_2d<f32>;
@group(0) @binding(1) var cloud_depth: texture_2d<f32>;
@group(0) @binding(2) var cloud_sampler: sampler;

struct CompositeOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@vertex
fn vs_composite(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // Triangle plein écran
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );
    return vec4<f32>(pos[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_composite(@builtin(position) frag_coord: vec4<f32>) -> CompositeOutput {
    let dims = vec2<f32>(textureDimensions(cloud_color));
    // Un texel demi-résolution couvre 2x2 pixels
    let half_pos = frag_coord.xy * 0.5;
    let color = textureSampleLevel(cloud_color, cloud_sampler, half_pos / dims, 0.0);
    if (color.a < 0.002) {
        discard;
    }

    // Texels lus par le filtrage bilinéaire : on garde l'impact le plus proche
    let max_texel = vec2<i32>(textureDimensions(cloud_depth)) - vec2<i32>(1);
    let t0 = clamp(vec2<i32>(floor(half_pos - vec2<f32>(0.5))), vec2<i32>(0), max_texel);
    let t1 = min(t0 + vec2<i32>(1), max_texel);
    let depth = min(
        min(textureLoad(cloud_depth, t0, 0).r, textureLoad(cloud_depth, vec2<i32>(t1.x, t0.y), 0).r),
        min(textureLoad(cloud_depth, vec2<i32>(t0.x, t1.y), 0).r, textureLoad(cloud_depth, t1, 0).r),
    );

    var out: CompositeOutput;
    out.color = color;
    out.depth = depth;
    return out;
}

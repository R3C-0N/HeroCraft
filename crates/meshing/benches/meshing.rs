//! Mesure du maillage des chunks : `cargo bench -p herocraft_meshing`.
//!
//! Toujours la même colonne de chunks (graine 2026, face +Z, chunk 10, 12) pour comparer un
//! changement à l'autre. Rapports HTML de criterion dans `target/criterion/`.

use std::sync::Arc;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use herocraft_common::PlanetFace;
use herocraft_meshing::voxel_mesh::VoxelMeshBuilder;
use herocraft_world::generator::PlanetWorldGenerator;
use herocraft_world::{PlanetDescriptor, TerrainEdits, VisualizationFilter};

const SEED: u64 = 2026;
const FACE: PlanetFace = PlanetFace::PosZ;
const CHUNK: (i32, i32) = (10, 12);
const RADIUS: f32 = 50.0;

fn generator() -> PlanetWorldGenerator {
    PlanetWorldGenerator::new(PlanetDescriptor { seed: SEED, ..Default::default() })
}

fn meshing(c: &mut Criterion) {
    let generator = generator();
    let (cx, cy) = CHUNK;
    let edits = TerrainEdits::new(None);
    // Les textures des blocs sont chargées une fois pour toutes, hors mesure
    let _ = VoxelMeshBuilder::block_color(herocraft_common::BlockId::STONE, 2);

    let mut group = c.benchmark_group("maillage");
    group.sample_size(20);

    // Relief, biomes, grottes et arbres d'une colonne : ce que le maillage lit ensuite
    group.bench_function("echantillonnage_colonne", |b| {
        b.iter(|| VoxelMeshBuilder::sample_column_source(&generator, FACE, cx, cy, RADIUS, VisualizationFilter::Standard))
    });

    let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, FACE, cx, cy, RADIUS, VisualizationFilter::Standard));

    // Toutes les couches d'altitude de la colonne : génération des blocs, lumière, faces visibles
    group.bench_function("colonne_complete", |b| b.iter(|| VoxelMeshBuilder::build_column(&generator, &edits, source.clone())));

    // Remaillage après un bloc cassé ou posé : la couche touchée et ses deux voisines
    let mut column = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
    let surface = (0..528).rev().find(|&y| !generator.block_at(FACE, cx * 32 + 9, y, cy * 32 + 9).is_air()).unwrap_or(0);
    let layer = surface / 32;
    group.bench_function("remaillage_trois_couches", |b| {
        b.iter(|| VoxelMeshBuilder::rebuild_layers(&generator, &edits, &mut column, black_box(&[layer - 1, layer, layer + 1])))
    });

    // Chunk lointain simplifié (LOD), sans grottes ni lumière
    group.bench_function("chunk_lod_rapide", |b| {
        b.iter(|| VoxelMeshBuilder::build_incurved_single_chunk_fast(&generator, FACE, cx, cy, RADIUS, VisualizationFilter::Standard))
    });

    group.finish();
}

criterion_group!(benches, meshing);
criterion_main!(benches);

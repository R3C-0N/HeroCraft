//! Mesure de la génération du terrain : `cargo bench -p herocraft_world`.
//!
//! Toujours la même colonne de chunks (graine 2026, face +Z, chunk 10, 12) pour comparer un
//! changement à l'autre. Rapports HTML de criterion dans `target/criterion/`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use herocraft_common::{PlanetChunkCoord, PlanetFace};
use herocraft_voxel_engine::chunk::CHUNK_SIZE;
use herocraft_world::generator::PlanetWorldGenerator;
use herocraft_world::PlanetDescriptor;

const SEED: u64 = 2026;
const FACE: PlanetFace = PlanetFace::PosZ;
const CHUNK: (i32, i32) = (10, 12);
/// Bordure de colonnes autour du chunk, comme pour le maillage (faces et lumière des voisins)
const BORDER: i32 = 1;

fn generator() -> PlanetWorldGenerator {
    PlanetWorldGenerator::new(PlanetDescriptor { seed: SEED, ..Default::default() })
}

fn generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation");
    group.sample_size(10);

    // Création du générateur : tectonique et courants océaniques de la planète
    group.bench_function("nouveau_generateur", |b| b.iter(generator));

    let generator = generator();
    let (cx, cy) = CHUNK;
    group.sample_size(30);

    // Une seule colonne : relief et biome en un point
    group.bench_function("echantillon_colonne", |b| b.iter(|| generator.sample_column(FACE, black_box(0.31), black_box(-0.42))));

    // Les 34 x 34 colonnes d'un chunk et de sa bordure
    group.bench_function("colonnes_chunk", |b| b.iter(|| generator.sample_chunk_columns(FACE, cx, cy, BORDER)));

    // Blocs du chunk qui contient la surface, colonnes déjà échantillonnées (grottes et arbres compris)
    let columns = generator.sample_chunk_columns(FACE, cx, cy, BORDER);
    let side = CHUNK_SIZE as i32 + 2 * BORDER;
    let center = columns[((CHUNK_SIZE as i32 / 2 + BORDER) * side + CHUNK_SIZE as i32 / 2 + BORDER) as usize].elevation;
    let surface = PlanetChunkCoord::new(FACE, cx, cy, (center as i32).div_euclid(CHUNK_SIZE as i32));
    group.bench_function("chunk_surface", |b| b.iter(|| generator.generate_chunk_from_columns(&surface, &columns, BORDER)));

    // Chunk sous la surface : roche et grottes, sans arbres
    let deep = PlanetChunkCoord { altitude_layer: surface.altitude_layer - 2, ..surface };
    group.bench_function("chunk_souterrain", |b| b.iter(|| generator.generate_chunk_from_columns(&deep, &columns, BORDER)));

    // Chunk complet, échantillonnage des colonnes compris
    group.bench_function("chunk_complet", |b| b.iter(|| generator.generate_chunk(&surface)));

    group.finish();
}

criterion_group!(benches, generation);
criterion_main!(benches);

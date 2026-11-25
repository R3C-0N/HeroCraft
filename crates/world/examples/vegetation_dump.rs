//! Liste les blocs visibles d'un carré de terrain (arbres, herbes, fleurs compris), pour en tirer
//! une vue d'ensemble hors du jeu.
//!
//! Usage : cargo run -p herocraft_world --release --example vegetation_dump -- <biome> [graine] [côté]
//! (`Forest`, `Plains`, `Jungle`…). Écrit une ligne par bloc : `x y z #dessus #côté plante`.
use herocraft_common::{PlanetChunkCoord, PlanetFace, blocks};
use herocraft_world::{BiomeType, PlanetDescriptor, PlanetWorldGenerator};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let biome = args.first().and_then(|b| BiomeType::from_name(b)).unwrap_or(BiomeType::Forest);
    let seed = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let side: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(48);
    let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed, ..Default::default() });

    // Premier carré dont les quatre coins et le centre sont du biome demandé
    let mut origin = None;
    'search: for face in [PlanetFace::PosX, PlanetFace::NegZ, PlanetFace::PosZ, PlanetFace::NegX, PlanetFace::PosY] {
        for z in (64..960 - side).step_by(37) {
            for x in (64..960 - side).step_by(37) {
                let ok = [(0, 0), (side, 0), (0, side), (side, side), (side / 2, side / 2)]
                    .iter()
                    .all(|&(dx, dz)| generator.sample_block_column(face, x + dx, z + dz).biome == biome);
                if ok {
                    origin = Some((face, x, z));
                    break 'search;
                }
            }
        }
    }
    let Some((face, x0, z0)) = origin else {
        eprintln!("Aucun carré de {biome:?} trouvé");
        return;
    };
    eprintln!("{biome:?} : face {face:?}, x {x0}, z {z0}");

    let hex = |c: [f32; 4]| format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
    let mut chunks = std::collections::HashMap::new();
    let block_at = |chunks: &mut std::collections::HashMap<PlanetChunkCoord, herocraft_voxel_engine::ChunkData>, x: i32, y: i32, z: i32| {
        let coord = PlanetChunkCoord::new(face, x.div_euclid(32), z.div_euclid(32), y.div_euclid(32));
        let chunk = chunks.entry(coord).or_insert_with(|| generator.generate_chunk(&coord));
        chunk.get_voxel(x.rem_euclid(32) as u8, y.rem_euclid(32) as u8, z.rem_euclid(32) as u8)
    };
    let grounds: Vec<i32> =
        (0..side * side).map(|i| generator.sample_block_column(face, x0 + i % side, z0 + i / side).elevation.floor() as i32).collect();
    let (low, high) = (grounds.iter().min().unwrap() - 3, grounds.iter().max().unwrap() + 24);
    for z in z0..z0 + side {
        for x in x0..x0 + side {
            for y in low..high {
                let block = block_at(&mut chunks, x, y, z);
                if block.is_air() {
                    continue;
                }
                // Seulement les blocs dont une face touche l'air ou une plante
                let open = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, -1)].iter().any(|&(dx, dy, dz)| {
                    let n = block_at(&mut chunks, x + dx, y + dy, z + dz);
                    n.is_air() || n.is_plant() || !n.is_opaque()
                });
                if !open {
                    continue;
                }
                let Some(sheet) = blocks().get(block) else { continue };
                println!(
                    "{} {} {} {} {} {}",
                    x - x0,
                    y - low,
                    z - z0,
                    hex(sheet.top_color()),
                    hex(sheet.side_color()),
                    block.is_plant() as u8
                );
            }
        }
    }
}

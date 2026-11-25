//! Rend des coupes du terrain (PPM) pour inspecter les grottes : une coupe verticale le long d'une
//! ligne de blocs et une vue de dessus des grottes, plus quelques statistiques et le coût du champ.
//! Usage : cargo run -p herocraft_world --release --example cave_slice -- [seed] [face 0-5] [x] [z] [préfixe]
//! ou `-- [seed] scan` pour lister les régions de terre
use herocraft_common::{PlanetChunkCoord, PlanetFace};
use herocraft_world::{CAVE_DEPTH, PlanetDescriptor, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};
use std::io::Write;
use std::time::Instant;

fn write_ppm(path: &str, w: usize, h: usize, pixels: &[u8]) {
    let mut f = std::fs::File::create(path).expect("création du fichier");
    write!(f, "P6\n{} {}\n255\n", w, h).unwrap();
    f.write_all(pixels).unwrap();
    println!("{} écrit", path);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let generator = PlanetWorldGenerator::new(desc);
    if args.get(2).map(String::as_str) == Some("scan") {
        // Proportion de terre de chaque quart de face, pour choisir où couper
        for (f, &face) in PlanetFace::ALL.iter().enumerate() {
            for q in 0..16 {
                let (qx, qz) = (q % 4 * 256 + 128, q / 4 * 256 + 128);
                let land = (0..64)
                    .filter(|k| generator.sample_block_column(face, qx - 96 + k % 8 * 24, qz - 96 + k / 8 * 24).elevation > 130.0)
                    .count();
                if land > 40 {
                    println!("face {} centre ({}, {}) : {}/64 colonnes au-dessus de 130", f, qx, qz, land);
                }
            }
        }
        return;
    }
    let face = PlanetFace::ALL[args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4usize)];
    let x_center: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(512);
    let z_line: i32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(512);
    let prefix = args.get(5).cloned().unwrap_or_else(|| "grottes".to_string());

    // 1. Coupe verticale : x en abscisse, altitude en ordonnée
    let (w, h) = (512usize, 360usize);
    let x_start = (x_center - 256).clamp(0, 512);
    let mut pixels = vec![0u8; w * h * 3];
    for i in 0..w as i32 {
        let x = x_start + i;
        let column = generator.sample_block_column(face, x, z_line);
        for y in 0..h as i32 {
            let block = generator.column_block(&column, face, x, y, z_line);
            let solid_natural = (y as f64) < column.elevation.floor();
            let rgb = if block.is_air() && solid_natural {
                [230, 60, 40] // grotte
            } else if block.is_air() {
                [150, 190, 235]
            } else if block == herocraft_common::BlockId::WATER {
                [40, 90, 180]
            } else if block == herocraft_common::BlockId::GRASS {
                [70, 150, 60]
            } else if block == herocraft_common::BlockId::DIRT {
                [120, 85, 55]
            } else if block == herocraft_common::BlockId::SAND {
                [220, 200, 140]
            } else if block == herocraft_common::BlockId::SNOW {
                [245, 245, 250]
            } else {
                [95, 95, 100]
            };
            let row = h - 1 - y as usize;
            pixels[(row * w + i as usize) * 3..][..3].copy_from_slice(&rgb);
        }
    }
    write_ppm(&format!("{}_coupe.ppm", prefix), w, h, &pixels);

    // 2. Vue de dessus : pour chaque colonne, rouge si une grotte la traverse (plus sombre en
    // profondeur), jaune si elle débouche à la surface
    let side = 512usize;
    let (x0, z0) = (x_start, (z_line - 256).clamp(0, 512));
    let mut top = vec![0u8; side * side * 3];
    let (mut band, mut carved, mut openings, mut land) = (0usize, 0usize, 0usize, 0usize);
    for j in 0..side as i32 {
        for i in 0..side as i32 {
            let (x, z) = (x0 + i, z0 + j);
            let column = generator.sample_block_column(face, x, z);
            let ground = column.elevation.floor() as i32;
            let mut shallowest = None;
            for y in (ground - CAVE_DEPTH as i32).max(0)..ground {
                band += 1;
                if generator.is_cave(&column, face, x, y, z) {
                    carved += 1;
                    shallowest = Some(y);
                }
            }
            let land_column = column.elevation > SEA_LEVEL_BLOCKS;
            land += land_column as usize;
            let rgb = match shallowest {
                Some(y) if y == ground - 1 => {
                    openings += 1;
                    [250, 220, 40]
                }
                Some(y) => {
                    let t = ((ground - y) as f64 / CAVE_DEPTH).clamp(0.0, 1.0);
                    [(240.0 - 150.0 * t) as u8, (70.0 - 50.0 * t) as u8, 40]
                }
                None if land_column => [70, 120, 60],
                None => [40, 70, 140],
            };
            top[((j as usize) * side + i as usize) * 3..][..3].copy_from_slice(&rgb);
        }
    }
    write_ppm(&format!("{}_dessus.ppm", prefix), side, side, &top);
    println!(
        "Bande des grottes : {:.2}% creusée, ouvertures en surface : {:.3}% des colonnes de terre",
        carved as f64 / band.max(1) as f64 * 100.0,
        openings as f64 / land.max(1) as f64 * 100.0
    );

    // 3. Coût : génération de chunks avec et sans grottes sur une colonne de couches
    let coords: Vec<PlanetChunkCoord> =
        (0..8).flat_map(|i| (2..7).map(move |layer| PlanetChunkCoord::new(face, 8 + i, 17, layer))).collect();
    let columns: Vec<_> = (0..8).map(|i| generator.sample_chunk_columns(face, 8 + i, 17, 1)).collect();
    // Meilleur temps sur 10 passes, la machine n'étant pas au repos
    let best = |f: &dyn Fn()| {
        (0..10)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed()
            })
            .min()
            .unwrap()
    };
    let without = best(&|| {
        for (k, coord) in coords.iter().enumerate() {
            let _ = generator.generate_chunk_with_caves(coord, &columns[k / 5], 1, None);
        }
    });
    let with = best(&|| {
        for (k, coord) in coords.iter().enumerate() {
            let _ = generator.generate_chunk_from_columns(coord, &columns[k / 5], 1);
        }
    });
    let fields = best(&|| {
        for (k, coord) in coords.iter().enumerate() {
            let _ = generator.chunk_cave_field(coord, &columns[k / 5], 1);
        }
    });
    let sampling = best(&|| {
        for i in 0..8 {
            let _ = generator.sample_chunk_columns(face, 8 + i, 17, 1);
        }
    });
    println!(
        "Échantillonnage des colonnes (8 colonnes de chunks) : {:.2} ms ; champs de grottes seuls : {:.2} ms",
        sampling.as_secs_f64() * 1e3,
        fields.as_secs_f64() * 1e3
    );
    println!(
        "Génération de {} chunks : {:.2} ms sans grottes, {:.2} ms avec ({:.3} ms par chunk en plus)",
        coords.len(),
        without.as_secs_f64() * 1e3,
        with.as_secs_f64() * 1e3,
        (with - without).as_secs_f64() * 1e3 / coords.len() as f64
    );
}

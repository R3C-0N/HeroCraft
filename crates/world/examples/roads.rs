//! Trace les chemins entre villes et villages d'une face et dessine la carte vue de dessus (PPM).
//! Usage : cargo run -p herocraft_world --release --example roads -- [graine] [face 0..6] [sortie.ppm] [blocs par pixel] [n° de chemin]
//! Avec un n° de chemin, dessine plutôt, vue de dessus, les abords du départ de ce chemin
//! (`blocs par pixel` vaut alors 1 : bloc par bloc ; au-delà, une vue plus large, centrée sur le chemin entier).
use herocraft_common::PlanetFace;
use herocraft_world::edits::FACE_BLOCKS;
use herocraft_world::generator::{BiomeType, SEA_LEVEL_BLOCKS};
use herocraft_world::{PlanetDescriptor, PlanetWorldGenerator};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let face = PlanetFace::ALL[args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0)];
    let generator = PlanetWorldGenerator::new(desc);
    let start = std::time::Instant::now();
    for cz in 0..herocraft_world::structures::CELLS_PER_FACE {
        for cx in 0..herocraft_world::structures::CELLS_PER_FACE {
            generator.structures.cell(&generator, face, cx, cz);
        }
    }
    println!("établissements dessinés en {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
    let start = std::time::Instant::now();
    let roads = generator.roads.all_on_face(&generator, face);
    println!("{} chemins tracés en {:.0} ms", roads.len(), start.elapsed().as_secs_f64() * 1000.0);
    for road in &roads {
        let straight = (((road.from.0 - road.to.0) as f64).powi(2) + ((road.from.1 - road.to.1) as f64).powi(2)).sqrt();
        println!(
            "  {:?} → {:?} : {:.0} blocs ({:.0} à vol d'oiseau), {} colonnes",
            road.from,
            road.to,
            road.length(),
            straight,
            road.columns()
        );
    }
    let Some(out) = args.get(3) else {
        return;
    };
    let scale: i32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(8);
    if let Some(road) = args.get(5).and_then(|n| n.parse::<usize>().ok()).and_then(|n| roads.get(n)) {
        close_up(&generator, face, road, out, scale);
        return;
    }
    let size = FACE_BLOCKS / scale;
    let mut img = vec![0u8; (size * size * 3) as usize];
    for pz in 0..size {
        for px in 0..size {
            let (x, z) = (px * scale + scale / 2, pz * scale + scale / 2);
            let c = generator.sample_block_column(face, x, z);
            let h = c.elevation - SEA_LEVEL_BLOCKS;
            let rgb = if h <= 0.0 {
                [30, 60, (120.0 + h.max(-60.0)) as u8]
            } else {
                let base: [f64; 3] = match c.biome {
                    BiomeType::Desert | BiomeType::Beach => [200.0, 185.0, 130.0],
                    BiomeType::Savanna => [170.0, 165.0, 90.0],
                    BiomeType::SnowPeaks | BiomeType::Tundra => [225.0, 225.0, 230.0],
                    BiomeType::Mountains => [130.0, 125.0, 120.0],
                    _ => [80.0, 140.0, 70.0],
                };
                let shade = (0.75 + h / 300.0).clamp(0.6, 1.3);
                [(base[0] * shade).min(255.0) as u8, (base[1] * shade).min(255.0) as u8, (base[2] * shade).min(255.0) as u8]
            };
            let i = ((pz * size + px) * 3) as usize;
            img[i..i + 3].copy_from_slice(&rgb);
        }
    }
    let mut put = |x: i32, z: i32, rgb: [u8; 3]| {
        let (px, pz) = (x / scale, z / scale);
        if (0..size).contains(&px) && (0..size).contains(&pz) {
            let i = ((pz * size + px) * 3) as usize;
            img[i..i + 3].copy_from_slice(&rgb);
        }
    };
    for road in &roads {
        for w in road.points.windows(2) {
            let n = ((w[1].0 - w[0].0).abs().max((w[1].1 - w[0].1).abs()) as i32).max(1);
            for k in 0..=n {
                let t = k as f64 / n as f64;
                put((w[0].0 + (w[1].0 - w[0].0) * t) as i32, (w[0].1 + (w[1].1 - w[0].1) * t) as i32, [120, 60, 20]);
            }
        }
    }
    for cz in 0..herocraft_world::structures::CELLS_PER_FACE {
        for cx in 0..herocraft_world::structures::CELLS_PER_FACE {
            if let Some(s) = generator.structures.cell(&generator, face, cx, cz) {
                let rgb = match s.kind {
                    herocraft_world::SettlementKind::Town => [220, 30, 30],
                    herocraft_world::SettlementKind::Village => [250, 200, 0],
                    herocraft_world::SettlementKind::Ruins => [90, 90, 90],
                };
                for z in s.bounds.z0..s.bounds.z1 {
                    for x in s.bounds.x0..s.bounds.x1 {
                        put(x, z, rgb);
                    }
                }
            }
        }
    }
    let mut file = std::fs::File::create(out).expect("fichier de sortie");
    write!(file, "P6\n{size} {size}\n255\n").unwrap();
    file.write_all(&img).unwrap();
    println!("carte écrite dans {out}");
}

/// Vue de dessus, un pixel par bloc, du plus haut bloc de chaque colonne autour du départ du chemin
fn close_up(generator: &PlanetWorldGenerator, face: PlanetFace, road: &herocraft_world::Road, out: &str, scale: i32) {
    use herocraft_common::BlockId;
    let scale = scale.max(1);
    let (size, center) = if scale == 1 {
        (320, (road.points[0].0 as i32, road.points[0].1 as i32))
    } else {
        let b = road.bounds;
        ((b.width().max(b.depth()) + 160) / scale, b.center())
    };
    let (x0, z0) = (center.0 - size * scale / 2, center.1 - size * scale / 2);
    let top = generator.settlement_at(face, road.from.0, road.from.1).map_or(0, |s| s.top);
    let mut img = vec![0u8; (size * size * 3) as usize];
    let mut heights = vec![0i32; (size * size) as usize];
    for pz in 0..size {
        for px in 0..size {
            let (x, z) = (x0 + px * scale, z0 + pz * scale);
            let column = generator.sample_block_column(face, x, z);
            let ground = column.elevation.floor() as i32;
            let mut y = if generator.settlement_at(face, x, z).is_some() { top.max(ground + 24) } else { ground + 14 };
            let mut block = BlockId::AIR;
            while y >= ground - 2 {
                let b = generator.column_block(&column, face, x, y, z);
                if !b.is_air() {
                    block = b;
                    break;
                }
                y -= 1;
            }
            let rgb: [u8; 3] = match block.0 {
                2 => [120, 85, 55],
                3 => [90, 150, 60],
                4 => [215, 200, 140],
                5 => [240, 240, 245],
                6 => [50, 90, 180],
                1 => [125, 125, 125],
                8 => [100, 70, 40],
                9 => [40, 100, 40],
                123 => [165, 160, 150],
                120 => [110, 110, 115],
                n => {
                    let h = (n as u32).wrapping_mul(2654435761);
                    [(h >> 24) as u8 / 2 + 100, (h >> 16) as u8 / 2 + 60, (h >> 8) as u8 / 2 + 40]
                }
            };
            let i = (pz * size + px) as usize;
            heights[i] = y;
            img[i * 3..i * 3 + 3].copy_from_slice(&rgb);
        }
    }
    // Ombrage : la lumière vient du nord-ouest
    for pz in 1..size {
        for px in 1..size {
            let i = (pz * size + px) as usize;
            let d = heights[i] - heights[i - 1 - size as usize];
            let k = (1.0 + d as f64 * 0.12).clamp(0.55, 1.35);
            for c in 0..3 {
                img[i * 3 + c] = (img[i * 3 + c] as f64 * k).min(255.0) as u8;
            }
        }
    }
    let mut file = std::fs::File::create(out).expect("fichier de sortie");
    write!(file, "P6\n{size} {size}\n255\n").unwrap();
    file.write_all(&img).unwrap();
    println!("abords du chemin écrits dans {out}");
}

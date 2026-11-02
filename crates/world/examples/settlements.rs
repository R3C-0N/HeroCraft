//! Liste les villes, villages et ruines d'une planète, et dessine le plan vu de dessus de l'un d'eux (PPM).
//! Usage : cargo run -p herocraft_world --release --example settlements -- [graine] [n° à dessiner] [sortie.ppm]
use herocraft_common::{blocks, PlanetFace};
use herocraft_world::structures::CELLS_PER_FACE;
use herocraft_world::{PlanetDescriptor, PlanetWorldGenerator};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let generator = PlanetWorldGenerator::new(desc);
    let start = std::time::Instant::now();
    let mut all = Vec::new();
    for face in PlanetFace::ALL {
        for cz in 0..CELLS_PER_FACE {
            for cx in 0..CELLS_PER_FACE {
                if let Some(s) = generator.structures.cell(&generator, face, cx, cz) {
                    all.push(s);
                }
            }
        }
    }
    println!("{} établissements dessinés en {:.0} ms", all.len(), start.elapsed().as_secs_f64() * 1000.0);
    for (i, s) in all.iter().enumerate() {
        println!(
            "{i:3} {:8} face {:?} centre ({}, {}) {:?}, {} pièces, {}×{} blocs",
            s.kind.name(),
            s.face,
            s.center.0,
            s.center.1,
            s.biome,
            s.pieces.len(),
            s.bounds.width(),
            s.bounds.depth()
        );
    }

    let Some(s) = args.get(2).and_then(|n| n.parse::<usize>().ok()).and_then(|n| all.get(n)) else {
        return;
    };
    let out = args.get(3).cloned().unwrap_or_else(|| "settlement.ppm".to_string());
    let area = s.bounds.expand(6);
    let (w, h) = (area.width() as usize, area.depth() as usize);
    let scale = 4;
    let mut pixels = vec![0u8; w * h * 3 * scale * scale];
    for z in 0..h {
        for x in 0..w {
            let (bx, bz) = (area.x0 + x as i32, area.z0 + z as i32);
            let column = generator.sample_block_column(s.face, bx, bz);
            let top = (0..400).rev().map(|y| (y, generator.column_block(&column, s.face, bx, y, bz))).find(|(_, b)| !b.is_air());
            let rgb = match top {
                Some((y, block)) => {
                    let c = blocks().get(block).map(|sheet| sheet.top_color()).unwrap_or([0.5, 0.5, 0.5, 1.0]);
                    // Ombrage selon l'altitude
                    let shade = (0.6 + (y as f32 - column.elevation as f32) * 0.05).clamp(0.4, 1.3);
                    [c[0] * shade, c[1] * shade, c[2] * shade].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
                }
                None => [0, 0, 0],
            };
            for dz in 0..scale {
                for dx in 0..scale {
                    let i = ((z * scale + dz) * w * scale + x * scale + dx) * 3;
                    pixels[i..i + 3].copy_from_slice(&rgb);
                }
            }
        }
    }
    let mut f = std::fs::File::create(&out).expect("création du fichier");
    write!(f, "P6\n{} {}\n255\n", w * scale, h * scale).unwrap();
    f.write_all(&pixels).unwrap();
    println!("{out} écrit");
}

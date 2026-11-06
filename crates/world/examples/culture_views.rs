//! Vue isométrique d'une ville ou d'un village de chaque culture (blocs aux couleurs de leurs
//! fiches), pour comparer les styles. Une image PPM par établissement.
//! Usage : cargo run -p herocraft_world --release --example culture_views -- [graine] [ville|village] [dossier] [demi-côté]
use herocraft_common::{blocks, BlockId, PlanetFace};
use herocraft_world::structures::CELLS_PER_FACE;
use herocraft_world::{Culture, PlanetDescriptor, PlanetWorldGenerator, Settlement};
use std::io::Write;

/// Côté d'un bloc à l'écran, en pixels
const S: i32 = 6;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let kind = args.get(2).cloned().unwrap_or_else(|| "ville".into());
    let dir = args.get(3).cloned().unwrap_or_else(|| ".".into());
    let half: Option<i32> = args.get(4).and_then(|s| s.parse().ok());
    let generator = PlanetWorldGenerator::new(desc);
    let mut all: Vec<&Settlement> = Vec::new();
    for face in PlanetFace::ALL {
        for cz in 0..CELLS_PER_FACE {
            for cx in 0..CELLS_PER_FACE {
                if let Some(s) = generator.structures.cell(&generator, face, cx, cz) {
                    all.push(s);
                }
            }
        }
    }
    for culture in Culture::ALL {
        let Some(s) = all.iter().find(|s| s.kind.name() == kind && s.culture == Some(culture)) else {
            println!("{kind} {:?} : aucune", culture);
            continue;
        };
        let out = format!("{dir}/{kind}-{}.ppm", culture.espece_id());
        render(&generator, s, half, &out);
        println!("{} ({:?}, centre {:?}) → {out}", culture.name(), s.biome, s.center);
    }
}

fn color(block: BlockId, top: bool) -> [f32; 3] {
    let c = blocks().get(BlockId(block.0 & !(0x7 << BlockId::ROTATION_SHIFT))).map(|b| if top { b.top_color() } else { b.side_color() }).unwrap_or([1.0, 0.0, 1.0, 1.0]);
    [c[0], c[1], c[2]]
}

fn render(generator: &PlanetWorldGenerator, s: &Settlement, half: Option<i32>, out: &str) {
    let area = match half {
        Some(h) => herocraft_world::structures::Rect::around(s.center.0, s.center.1, h),
        None => s.bounds.expand(3),
    };
    let ground = generator.sample_block_column(s.face, s.center.0, s.center.1).elevation.floor() as i32;
    let (y0, y1) = (ground - 12, s.top.max(ground + 10) + 2);
    let (w, d, h) = (area.width(), area.depth(), y1 - y0);
    // Grille des blocs
    let mut grid = vec![BlockId::AIR; (w * d * h) as usize];
    let idx = |x: i32, y: i32, z: i32| ((y * d + z) * w + x) as usize;
    for z in 0..d {
        for x in 0..w {
            let (bx, bz) = (area.x0 + x, area.z0 + z);
            let column = generator.sample_block_column(s.face, bx, bz);
            for y in 0..h {
                let b = generator.column_block(&column, s.face, bx, y0 + y, bz);
                // Les eaux et le terrain sous le relief restent pleins
                grid[idx(x, y, z)] = b;
            }
        }
    }
    let solid = |x: i32, y: i32, z: i32| {
        if x < 0 || z < 0 || y < 0 || x >= w || z >= d || y >= h {
            return false;
        }
        !grid[idx(x, y, z)].is_air()
    };
    // Projection : x vers la droite-bas, z vers la gauche-bas, y vers le haut
    let width = (w + d) * S;
    let height = (w + d) * S / 2 + h * S + S;
    let mut img = vec![[18u8, 22, 30]; (width * height) as usize];
    let mut depth = vec![i32::MIN; (width * height) as usize];
    let origin = (d * S, h * S);
    for y in 0..h {
        for z in 0..d {
            for x in 0..w {
                let b = grid[idx(x, y, z)];
                if b.is_air() || (solid(x, y + 1, z) && solid(x + 1, y, z) && solid(x, y, z + 1)) {
                    continue;
                }
                let px = origin.0 + (x - z) * S;
                let py = origin.1 + (x + z) * S / 2 - y * S;
                let key = x + z + y;
                let top = color(b, true);
                let side = color(b, false);
                // Ombre douce selon la hauteur
                let lift = 0.75 + 0.25 * (y as f32 / h as f32);
                for dy in 0..2 * S {
                    for dx in 0..2 * S {
                        // Hexagone du cube : sommet en (0, 0), base en (0, 2S), côtés à ±S
                        let u = (dx - S) as f32 + 0.5;
                        let v = dy as f32 + 0.5;
                        let half = u.abs() / 2.0;
                        if v < half || v > 2.0 * S as f32 - half {
                            continue;
                        }
                        let (c, k) = if v < S as f32 - half {
                            (top, 1.0)
                        } else if u < 0.0 {
                            (side, 0.8)
                        } else {
                            (side, 0.62)
                        };
                        let (sx, sy) = (px + dx - S, py + dy - S / 2);
                        if sx < 0 || sy < 0 || sx >= width || sy >= height {
                            continue;
                        }
                        let i = (sy * width + sx) as usize;
                        if key < depth[i] {
                            continue;
                        }
                        depth[i] = key;
                        img[i] = [c[0], c[1], c[2]].map(|v| ((v * k * lift).clamp(0.0, 1.0) * 255.0) as u8);
                    }
                }
            }
        }
    }
    let mut f = std::fs::File::create(out).expect("création du fichier");
    write!(f, "P6\n{width} {height}\n255\n").unwrap();
    for p in img {
        f.write_all(&p).unwrap();
    }
}

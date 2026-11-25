//! Vues de la ville-souvenir d'un temple : plan vu de dessus dans le passé (le rassemblement de
//! l'armée en bleu, les deux portes en jaune (grande porte devant, porte de la ville haute au fond), les places des mini-boss en orange, les postes des ennemis en rouge) et au présent, et
//! deux vues isométriques du passé, depuis l'entrée du temple et depuis le fond. Une image PPM par
//! vue.
//! Usage : cargo run -p herocraft_world --release --example memory_city_views -- [graine] [dossier] [peuple du joueur]
use herocraft_common::{BlockId, PlanetFace, blocks};
use herocraft_world::{Culture, Epoch, MemoryCity, PlanetDescriptor, PlanetWorldGenerator, TempleSite};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let player = args.get(3).and_then(|n| Culture::ALL.into_iter().find(|c| format!("{c:?}") == *n)).unwrap_or(Culture::Humains);
    let generator = PlanetWorldGenerator::new(desc);
    let face = PlanetFace::ALL[0];
    let mid = herocraft_world::FACE_BLOCKS / 2;
    let site = generator.nearest_temple(face, mid, mid, herocraft_world::FACE_BLOCKS).expect("un temple sur la face");
    let holder = site.holder_for(player);
    let past = generator.with_epoch(Epoch::Past(holder));
    let city = past.memory_city(&site, holder).expect("la ville-souvenir").clone();
    println!(
        "Temple en {:?} ({}, {}), entrée {:?}, tenu par les {} ; rempart {}, muraille intérieure {}, parvis {}, {} postes, {} chunks à régénérer",
        site.face,
        site.center().0,
        site.center().1,
        site.door(),
        holder.name(),
        city.outer,
        city.inner,
        city.court,
        city.posts.len(),
        past.vision_chunks(&site, holder).len()
    );
    let view = View::new(&site, &city);
    view.plan(&past, Some(&city), &format!("{dir}/ville-passe-plan.ppm"));
    view.plan(&generator, None, &format!("{dir}/ville-present-plan.ppm"));
    view.iso(&past, false, &format!("{dir}/ville-passe-entree.ppm"));
    view.iso(&past, true, &format!("{dir}/ville-passe-fond.ppm"));
}

struct View {
    face: PlanetFace,
    center: (i32, i32),
    door: (i32, i32),
    half: i32,
}

fn color(block: BlockId, top: bool) -> [f32; 3] {
    let c = blocks()
        .get(BlockId(block.0 & !(0x7 << BlockId::ROTATION_SHIFT)))
        .map(|b| if top { b.top_color() } else { b.side_color() })
        .unwrap_or([1.0, 0.0, 1.0, 1.0]);
    [c[0], c[1], c[2]]
}

fn save(out: &str, w: i32, h: i32, img: &[[u8; 3]]) {
    let mut f = std::fs::File::create(out).expect("création du fichier");
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    for p in img {
        f.write_all(p).unwrap();
    }
}

impl View {
    fn new(site: &TempleSite, city: &MemoryCity) -> View {
        View { face: site.face, center: site.center(), door: site.door().delta(), half: city.outer + 26 }
    }

    /// Colonne du monde du point (i, j) de l'image : l'entrée du temple vers le bas (ou vers le
    /// haut si `back`)
    fn world(&self, i: i32, j: i32, back: bool) -> (i32, i32) {
        let (u, v) = (i - self.half, self.half - j);
        let (u, v) = if back { (-u, -v) } else { (u, v) };
        let (dx, dz) = self.door;
        (self.center.0 - dx * v + dz * u, self.center.1 - dz * v - dx * u)
    }

    fn plan(&self, generator: &PlanetWorldGenerator, city: Option<&MemoryCity>, out: &str) {
        const P: i32 = 6;
        let side = 2 * self.half + 1;
        let mut img = vec![[18u8, 22, 30]; (side * P * side * P) as usize];
        let base = generator.sample_block_column(self.face, self.center.0, self.center.1).elevation.floor() as i32;
        let mut marks: Vec<((i32, i32), [u8; 3])> = Vec::new();
        if let Some(c) = city {
            marks.extend(c.posts.iter().map(|p| (p.0, [220, 40, 40])));
            marks.push((c.muster, [80, 150, 255]));
            marks.push((c.gate, [255, 220, 60]));
            marks.extend(c.arenas.iter().map(|&a| (a, [255, 140, 0])));
            marks.push((c.inner_gate, [255, 220, 60]));
        }
        for j in 0..side {
            for i in 0..side {
                let (x, z) = self.world(i, j, false);
                let column = generator.sample_block_column(self.face, x, z);
                let ground = column.elevation.floor() as i32;
                let Some((y, b)) = (ground - 4..ground + 30)
                    .rev()
                    .map(|y| (y, generator.column_block(&column, self.face, x, y, z)))
                    .find(|(_, b)| !b.is_air())
                else {
                    continue;
                };
                let shade = (0.75 + 0.025 * (y - base) as f32).clamp(0.4, 1.25);
                let px = color(b, true).map(|v| ((v * shade).clamp(0.0, 1.0) * 255.0) as u8);
                for py in 0..P {
                    for qx in 0..P {
                        img[((j * P + py) * side * P + i * P + qx) as usize] = px;
                    }
                }
            }
        }
        // Les marques, en ronds pleins
        for j in 0..side {
            for i in 0..side {
                let (x, z) = self.world(i, j, false);
                for &((mx, mz), c) in &marks {
                    if (mx, mz) == (x, z) {
                        let r = if c[0] == 220 { 4 } else { 9 };
                        for py in -r..=r {
                            for qx in -r..=r {
                                if py * py + qx * qx <= r * r {
                                    let (sx, sy) = (i * P + P / 2 + qx, j * P + P / 2 + py);
                                    if sx >= 0 && sy >= 0 && sx < side * P && sy < side * P {
                                        img[(sy * side * P + sx) as usize] = c;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        save(out, side * P, side * P, &img);
    }

    fn iso(&self, generator: &PlanetWorldGenerator, back: bool, out: &str) {
        const S: i32 = 4;
        let n = 2 * self.half + 1;
        let mut lo = i32::MAX;
        let mut columns = Vec::with_capacity((n * n) as usize);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = self.world(i, j, back);
                let c = generator.sample_block_column(self.face, x, z);
                lo = lo.min(c.elevation.floor() as i32);
                columns.push((x, z, c));
            }
        }
        let (y0, y1) = (lo - 2, lo + 48);
        let h = y1 - y0;
        let mut grid = vec![BlockId::AIR; (n * n * h) as usize];
        let idx = |i: i32, y: i32, j: i32| ((y * n + j) * n + i) as usize;
        for j in 0..n {
            for i in 0..n {
                let (x, z, c) = &columns[(j * n + i) as usize];
                for y in 0..h {
                    grid[idx(i, y, j)] = generator.column_block(c, self.face, *x, y0 + y, *z);
                }
            }
        }
        let solid = |i: i32, y: i32, j: i32| i >= 0 && j >= 0 && y >= 0 && i < n && j < n && y < h && !grid[idx(i, y, j)].is_air();
        let width = 2 * n * S;
        let height = n * S + h * S + S;
        let mut img = vec![[18u8, 22, 30]; (width * height) as usize];
        let mut depth = vec![i32::MIN; (width * height) as usize];
        let origin = (n * S, h * S);
        for y in 0..h {
            for j in 0..n {
                for i in 0..n {
                    let b = grid[idx(i, y, j)];
                    if b.is_air() || (solid(i, y + 1, j) && solid(i + 1, y, j) && solid(i, y, j + 1)) {
                        continue;
                    }
                    let px = origin.0 + (i - j) * S;
                    let py = origin.1 + (i + j) * S / 2 - y * S;
                    let key = i + j + y;
                    let (top, side) = (color(b, true), color(b, false));
                    for dy in 0..2 * S {
                        for dx in 0..2 * S {
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
                            let p = (sy * width + sx) as usize;
                            if key < depth[p] {
                                continue;
                            }
                            depth[p] = key;
                            img[p] = c.map(|v| ((v * k).clamp(0.0, 1.0) * 255.0) as u8);
                        }
                    }
                }
            }
        }
        save(out, width, height, &img);
    }
}

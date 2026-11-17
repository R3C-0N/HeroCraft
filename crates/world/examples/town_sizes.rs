//! Aperçus des quatre tailles de villes (bourg, petite ville, grande ville, capitale), dessinées au
//! même endroit : un plan vu de dessus (un pixel par bloc, même échelle pour toutes) et une vue
//! isométrique de chacune, plus un gros plan du cœur de la capitale. Images PPM.
//! Usage : cargo run -p herocraft_world --release --example town_sizes -- [graine] [dossier] [culture]
use herocraft_common::{blocks, BlockId, PlanetFace};
use herocraft_world::structures::{preview_town, Rect, TownSize};
use herocraft_world::{VEGETATION_SPAN, BiomeType, Culture, PlanetDescriptor, PlanetWorldGenerator, Settlement};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let culture = args
        .get(3)
        .and_then(|name| Culture::ALL.into_iter().find(|c| c.espece_id() == name || c.name() == name))
        .unwrap_or(Culture::Humains);
    let generator = PlanetWorldGenerator::new(desc);
    let (face, center) = match (args.get(4), args.get(5), args.get(6)) {
        (Some(f), Some(x), Some(z)) => (PlanetFace::ALL[f.parse::<usize>().unwrap()], (x.parse().unwrap(), z.parse().unwrap())),
        _ => find_site(&generator).expect("aucun site assez grand pour une capitale"),
    };
    println!("site : {face:?} {center:?} ({:?})", generator.sample_block_column(face, center.0, center.1).biome);
    let mut towns = Vec::new();
    for size in TownSize::ALL {
        let town = (0..20u64).find_map(|k| preview_town(&generator, face, center, size, culture, k)).expect("plan refusé");
        let houses = town.pieces.iter().filter(|p| matches!(p, herocraft_world::structures::Piece::House(_))).count();
        println!("{} : {} blocs de côté, {} pièces, {} bâtiments", size.name(), town.bounds.width(), town.pieces.len(), houses);
        towns.push((size, town));
    }
    // Plan commun : le site de la capitale, chaque ville dessinée seule
    let capital = towns.last().unwrap().1.bounds.expand(6);
    for (size, town) in &towns {
        let scene = Scene::build(&generator, face, town, capital);
        scene.plan(&format!("{dir}/plan-{}.ppm", size.name()));
        let s = match size {
            TownSize::Bourg => 6,
            TownSize::PetiteVille => 4,
            TownSize::GrandeVille => 3,
            TownSize::Capitale => 2,
        };
        let iso = Scene::build(&generator, face, town, town.bounds.expand(4));
        iso.iso(s, &format!("{dir}/iso-{}.ppm", size.name()));
        println!("{} rendue", size.name());
    }
    // Gros plan : la citadelle et ses premiers quartiers
    let (_, town) = towns.last().unwrap();
    let zoom = Scene::build(&generator, face, town, Rect::around(center.0, center.1, 72));
    zoom.iso(6, &format!("{dir}/iso-capitale-coeur.ppm"));
    // Gros plan d'un quartier et de ses remparts, dans un coin de la capitale
    let half = TownSize::Capitale.half();
    let corner = Scene::build(&generator, face, town, Rect::new(center.0 + half - 110, center.1 + half - 110, center.0 + half + 8, center.1 + half + 8));
    corner.iso(6, &format!("{dir}/iso-capitale-rempart.ppm"));
}

fn accepts(biome: BiomeType) -> bool {
    !matches!(biome, BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea | BiomeType::SnowPeaks | BiomeType::Mountains | BiomeType::Beach)
}

/// Un site de terre ferme assez plat pour une capitale (le plus plat d'un balayage de la planète)
fn find_site(generator: &PlanetWorldGenerator) -> Option<(PlanetFace, (i32, i32))> {
    let half = TownSize::Capitale.half() + 8;
    let face_blocks = herocraft_world::FACE_BLOCKS;
    let mut best: Option<(i32, PlanetFace, (i32, i32))> = None;
    for face in PlanetFace::ALL {
        let mut cz = half + 64;
        while cz < face_blocks - half - 64 {
            let mut cx = half + 64;
            'site: while cx < face_blocks - half - 64 {
                let (mut lo, mut hi) = (i32::MAX, i32::MIN);
                let mut dz = -half;
                while dz <= half {
                    let mut dx = -half;
                    while dx <= half {
                        let c = generator.sample_block_column(face, cx + dx, cz + dz);
                        let g = c.elevation.floor() as i32;
                        if !accepts(c.biome) || (g as f64) <= herocraft_world::SEA_LEVEL_BLOCKS + 2.0 {
                            cx += 256;
                            continue 'site;
                        }
                        lo = lo.min(g);
                        hi = hi.max(g);
                        if hi - lo > best.map_or(40, |b| b.0) {
                            cx += 256;
                            continue 'site;
                        }
                        dx += 24;
                    }
                    dz += 24;
                }
                if best.map_or(true, |b| hi - lo < b.0) {
                    best = Some((hi - lo, face, (cx, cz)));
                    if hi - lo <= 14 {
                        return Some((face, (cx, cz)));
                    }
                }
                cx += 256;
            }
            cz += 256;
        }
    }
    best.map(|b| (b.1, b.2))
}

/// Les blocs d'une zone : le terrain nu, la ville par-dessus, des arbres hors de ses murs
struct Scene {
    w: i32,
    d: i32,
    h: i32,
    grid: Vec<BlockId>,
}

impl Scene {
    fn build(generator: &PlanetWorldGenerator, face: PlanetFace, town: &Settlement, area: Rect) -> Scene {
        // Pièces de la ville rangées par carreaux de 32 colonnes (une ville a des milliers de pièces)
        const TILE: i32 = 32;
        let tiles_x = (area.width() + TILE - 1) / TILE;
        let tiles_z = (area.depth() + TILE - 1) / TILE;
        let mut tiles = Vec::new();
        for tz in 0..tiles_z {
            for tx in 0..tiles_x {
                let r = Rect::sized(area.x0 + tx * TILE, area.z0 + tz * TILE, TILE, TILE);
                let mut t = town.clone();
                t.pieces.retain(|p| p.rect().intersects(&r));
                tiles.push(t);
            }
        }
        let mut lo = i32::MAX;
        let mut columns = Vec::with_capacity((area.width() * area.depth()) as usize);
        for z in area.z0..area.z1 {
            for x in area.x0..area.x1 {
                let c = generator.sample_block_column(face, x, z);
                lo = lo.min(c.elevation.floor() as i32);
                columns.push(c);
            }
        }
        let y0 = lo - 4;
        let y1 = town.top.max(lo + 20) + 24;
        let (w, d, h) = (area.width(), area.depth(), y1 - y0);
        let walls = town.bounds.expand(2);
        // Une rangée de colonnes par fil, ses blocs de bas en haut
        let rows: Vec<Vec<BlockId>> = std::thread::scope(|scope| {
            let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as i32;
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let (columns, tiles) = (&columns, &tiles);
                    scope.spawn(move || {
                        let mut trees: std::collections::HashMap<(i32, i32), Option<herocraft_world::Tree>> = Default::default();
                        let mut out = Vec::new();
                        let mut z = t;
                        while z < d {
                            let mut row = vec![BlockId::AIR; (w * h) as usize];
                            for x in 0..w {
                                let (bx, bz) = (area.x0 + x, area.z0 + z);
                                let column = &columns[(z * w + x) as usize];
                                let ground = column.elevation.floor() as i32;
                                let tile = &tiles[((z / TILE) * tiles_x + x / TILE) as usize];
                                let span = tile.span(bx, bz, ground);
                                let wild = !walls.contains(bx, bz);
                                for y in 0..h {
                                    let gy = y0 + y;
                                    let mut b = if span.contains(&gy) { tile.block(bx, gy, bz, ground) } else { None };
                                    if b.is_none() && gy < ground - 3 {
                                        b = Some(BlockId::STONE);
                                    }
                                    let mut b = b.unwrap_or_else(|| generator.natural_block(column, face, bx, gy, bz));
                                    if b.is_air() && wild && gy >= ground && gy < ground + VEGETATION_SPAN + 2 {
                                        b = generator.vegetation_block(column, face, bx, gy, bz, |cx, cz| *trees.entry((cx, cz)).or_insert_with(|| generator.tree_in_cell(face, cx, cz)));
                                    }
                                    row[(y * w + x) as usize] = b;
                                }
                            }
                            out.push((z, row));
                            z += threads;
                        }
                        out
                    })
                })
                .collect();
            let mut rows = vec![Vec::new(); d as usize];
            for h in handles {
                for (z, row) in h.join().unwrap() {
                    rows[z as usize] = row;
                }
            }
            rows
        });
        let mut grid = vec![BlockId::AIR; (w * d * h) as usize];
        for z in 0..d {
            for y in 0..h {
                for x in 0..w {
                    grid[((y * d + z) * w + x) as usize] = rows[z as usize][(y * w + x) as usize];
                }
            }
        }
        Scene { w, d, h, grid }
    }

    fn at(&self, x: i32, y: i32, z: i32) -> BlockId {
        if x < 0 || z < 0 || y < 0 || x >= self.w || z >= self.d || y >= self.h {
            return BlockId::AIR;
        }
        self.grid[((y * self.d + z) * self.w + x) as usize]
    }

    /// Vue de dessus, un pixel par colonne, ombrée par la pente
    fn plan(&self, out: &str) {
        let mut tops = vec![0i32; (self.w * self.d) as usize];
        let mut img = vec![[0u8; 3]; (self.w * self.d) as usize];
        for z in 0..self.d {
            for x in 0..self.w {
                let mut y = self.h - 1;
                while y > 0 && self.at(x, y, z).is_air() {
                    y -= 1;
                }
                tops[(z * self.w + x) as usize] = y;
            }
        }
        for z in 0..self.d {
            for x in 0..self.w {
                let y = tops[(z * self.w + x) as usize];
                let west = if x > 0 { tops[(z * self.w + x - 1) as usize] } else { y };
                let north = if z > 0 { tops[((z - 1) * self.w + x) as usize] } else { y };
                let shade = (1.0 + 0.12 * ((y - west) + (y - north)) as f32).clamp(0.55, 1.35);
                let c = color(self.at(x, y, z), true);
                img[(z * self.w + x) as usize] = c.map(|v| ((v * shade).clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        write_ppm(out, self.w, self.d, &img);
    }

    /// Vue isométrique, `s` pixels par demi-arête de bloc
    fn iso(&self, s: i32, out: &str) {
        let (w, d, h) = (self.w, self.d, self.h);
        let solid = |x: i32, y: i32, z: i32| !self.at(x, y, z).is_air();
        let width = (w + d) * s;
        let height = (w + d) * s / 2 + h * s + s;
        let mut img = vec![[18u8, 22, 30]; (width * height) as usize];
        let mut depth = vec![i32::MIN; (width * height) as usize];
        let origin = (d * s, h * s);
        for y in 0..h {
            for z in 0..d {
                for x in 0..w {
                    let b = self.at(x, y, z);
                    if b.is_air() || (solid(x, y + 1, z) && solid(x + 1, y, z) && solid(x, y, z + 1)) {
                        continue;
                    }
                    let px = origin.0 + (x - z) * s;
                    let py = origin.1 + (x + z) * s / 2 - y * s;
                    let key = x + z + y;
                    let top = color(b, true);
                    let side = color(b, false);
                    let lift = 0.75 + 0.25 * (y as f32 / h as f32);
                    for dy in 0..2 * s {
                        for dx in 0..2 * s {
                            let u = (dx - s) as f32 + 0.5;
                            let v = dy as f32 + 0.5;
                            let half = u.abs() / 2.0;
                            if v < half || v > 2.0 * s as f32 - half {
                                continue;
                            }
                            let (c, k) = if v < s as f32 - half {
                                (top, 1.0)
                            } else if u < 0.0 {
                                (side, 0.8)
                            } else {
                                (side, 0.62)
                            };
                            let (sx, sy) = (px + dx - s, py + dy - s / 2);
                            if sx < 0 || sy < 0 || sx >= width || sy >= height {
                                continue;
                            }
                            let i = (sy * width + sx) as usize;
                            if key < depth[i] {
                                continue;
                            }
                            depth[i] = key;
                            img[i] = c.map(|v| ((v * k * lift).clamp(0.0, 1.0) * 255.0) as u8);
                        }
                    }
                }
            }
        }
        write_ppm(out, width, height, &img);
    }
}

fn color(block: BlockId, top: bool) -> [f32; 3] {
    let c = blocks().get(BlockId(block.0 & !(0x7 << BlockId::ROTATION_SHIFT))).map(|b| if top { b.top_color() } else { b.side_color() }).unwrap_or([1.0, 0.0, 1.0, 1.0]);
    [c[0], c[1], c[2]]
}

fn write_ppm(out: &str, w: i32, h: i32, img: &[[u8; 3]]) {
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).expect("création du fichier"));
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    for p in img {
        f.write_all(p).unwrap();
    }
}

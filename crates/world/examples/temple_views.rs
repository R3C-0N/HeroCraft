//! Vues isométriques d'un temple à ses deux époques : la ruine du présent et le temple intact du
//! passé (entier, puis toit ôté pour voir le sanctuaire). L'emplacement de la Pierre du Néant est
//! marqué d'un bloc violet. La vue est tournée pour montrer l'entrée. Une image PPM par vue.
//! Usage : cargo run -p herocraft_world --release --example temple_views -- [graine] [dossier] [peuple du joueur]
use herocraft_common::{blocks, BlockId, PlanetFace};
use herocraft_world::structures::Side;
use herocraft_world::{Culture, Epoch, PlanetDescriptor, PlanetWorldGenerator, TempleSite};
use std::io::Write;

/// Côté d'un bloc à l'écran, en pixels
const S: i32 = 10;
/// Demi-côté de la vue autour de l'autel
const HALF: i32 = 17;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let dir = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let player = args
        .get(3)
        .and_then(|n| Culture::ALL.into_iter().find(|c| format!("{c:?}") == *n))
        .unwrap_or(Culture::Humains);
    let generator = PlanetWorldGenerator::new(desc);
    let mid = herocraft_world::FACE_BLOCKS / 2;
    // Le plus grand temple trouvé près du milieu de chaque face
    let site = PlanetFace::ALL
        .into_iter()
        .filter_map(|f| generator.nearest_temple(f, mid, mid, herocraft_world::FACE_BLOCKS))
        .max_by_key(|t| t.foot().width())
        .expect("un temple sur la planète");
    let holder = site.holder_for(player);
    println!(
        "Temple en {:?} ({}, {}), {} de côté, entrée {:?}, tenu dans le passé par les {} (joueur : {})",
        site.face,
        site.center().0,
        site.center().1,
        site.foot().width(),
        site.door(),
        holder.name(),
        player.name()
    );
    let past = generator.with_epoch(Epoch::Past(holder));
    render(
        &generator,
        &site,
        Epoch::Present,
        false,
        &format!("{dir}/temple-present.ppm"),
    );
    render(
        &past,
        &site,
        past.epoch,
        false,
        &format!("{dir}/temple-passe.ppm"),
    );
    render(
        &past,
        &site,
        past.epoch,
        true,
        &format!("{dir}/temple-passe-coupe.ppm"),
    );
    render(
        &generator,
        &site,
        Epoch::Present,
        true,
        &format!("{dir}/temple-present-coupe.ppm"),
    );
    for c in [
        Culture::Nains,
        Culture::HautsElfes,
        Culture::DemiOrques,
        Culture::ElfesDesBois,
    ] {
        let g = generator.with_epoch(Epoch::Past(c));
        render(
            &g,
            &site,
            g.epoch,
            false,
            &format!("{dir}/temple-passe-{c:?}.ppm"),
        );
    }
}

fn color(block: BlockId, top: bool) -> [f32; 3] {
    let c = blocks()
        .get(BlockId(block.0 & !(0x7 << BlockId::ROTATION_SHIFT)))
        .map(|b| if top { b.top_color() } else { b.side_color() })
        .unwrap_or([1.0, 0.0, 1.0, 1.0]);
    [c[0], c[1], c[2]]
}

/// Marque de l'emplacement de la Pierre
const SLOT: BlockId = BlockId(u16::MAX);

fn render(generator: &PlanetWorldGenerator, site: &TempleSite, epoch: Epoch, cut: bool, out: &str) {
    let (cx, cz) = site.rect().center();
    let face = site.face;
    let (y0, y1) = (site.floor() - 7, site.top() + 1);
    let (w, d, h) = (2 * HALF + 1, 2 * HALF + 1, y1 - y0);
    let slot = site.stone_slot(epoch);
    // Tourne la vue pour que l'entrée regarde vers +Z (face gauche visible)
    let world = |x: i32, z: i32| -> (i32, i32) {
        let (u, v) = (x - HALF, z - HALF);
        let (a, b) = match site.door() {
            Side::South => (u, v),
            Side::North => (-u, -v),
            Side::East => (v, -u),
            Side::West => (-v, u),
        };
        (cx + a, cz + b)
    };
    let mut grid = vec![BlockId::AIR; (w * d * h) as usize];
    let idx = |x: i32, y: i32, z: i32| ((y * d + z) * w + x) as usize;
    for z in 0..d {
        for x in 0..w {
            let (bx, bz) = world(x, z);
            let column = generator.sample_block_column(face, bx, bz);
            for y in 0..h {
                let wy = y0 + y;
                let mut b = generator.column_block(&column, face, bx, wy, bz);
                if cut && wy >= site.floor() + 2 {
                    b = BlockId::AIR;
                }
                // Les arbres autour masquent le temple : la vue les ôte
                let plain = BlockId(b.0 & !(0x7 << BlockId::ROTATION_SHIFT));
                let id = blocks()
                    .get(plain)
                    .map(|s| s.id.clone())
                    .unwrap_or_default();
                let plant = ["Leaf", "tallGrass", "Flower", "Bush", "Mushroom"]
                    .iter()
                    .any(|k| id.contains(k))
                    || [BlockId::WOOD, BlockId(14), BlockId(17)].contains(&plain);
                if plant && !site.rect().contains(bx, bz) {
                    b = BlockId::AIR;
                }
                if (bx, wy, bz) == (slot.x, slot.y, slot.z) {
                    b = SLOT;
                }
                grid[idx(x, y, z)] = b;
            }
        }
    }
    let solid = |x: i32, y: i32, z: i32| {
        if x < 0 || z < 0 || y < 0 || x >= w || z >= d || y >= h {
            return false;
        }
        let b = grid[idx(x, y, z)];
        !b.is_air() && b != SLOT
    };
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
                let (top, side) = if b == SLOT {
                    ([0.75, 0.2, 1.0], [0.55, 0.1, 0.85])
                } else {
                    (color(b, true), color(b, false))
                };
                let lift = 0.75 + 0.25 * (y as f32 / h as f32);
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
                        let i = (sy * width + sx) as usize;
                        if key < depth[i] {
                            continue;
                        }
                        depth[i] = key;
                        img[i] = [c[0], c[1], c[2]]
                            .map(|v| ((v * k * lift).clamp(0.0, 1.0) * 255.0) as u8);
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

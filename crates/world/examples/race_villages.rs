//! Aperçus des villages des races qui n'en ont pas encore (Demi-orques, Revenants, Sylphes, Gnomes,
//! Aasimars, Hommes-bêtes) : une petite scène bâtie à la main avec les vrais blocs du jeu, vue en
//! isométrique comme `culture_views`. Ce n'est pas encore le générateur, seulement de quoi choisir
//! un style avant de l'écrire. Une image PPM par race.
//! Usage : cargo run -p herocraft_world --release --example race_villages -- [dossier]
use herocraft_common::{blocks, BlockId};
use std::io::Write;

/// Côté d'un bloc à l'écran, en pixels
const S: i32 = 8;
const W: i32 = 44;
const D: i32 = 44;
const H: i32 = 30;
/// Hauteur du sol
const G: i32 = 6;

struct Scene {
    grid: Vec<BlockId>,
}

fn b(name: &str) -> BlockId {
    blocks().find(name).unwrap_or_else(|| panic!("bloc inconnu : {name}"))
}

fn hash(x: i32, z: i32, k: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (z as u32).wrapping_mul(0x85EB_CA77) ^ k.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

impl Scene {
    fn new() -> Self {
        Scene { grid: vec![BlockId::AIR; (W * D * H) as usize] }
    }
    fn idx(x: i32, y: i32, z: i32) -> Option<usize> {
        (x >= 0 && z >= 0 && y >= 0 && x < W && z < D && y < H).then(|| ((y * D + z) * W + x) as usize)
    }
    fn set(&mut self, x: i32, y: i32, z: i32, block: BlockId) {
        if let Some(i) = Self::idx(x, y, z) {
            self.grid[i] = block;
        }
    }
    fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        Self::idx(x, y, z).map(|i| self.grid[i]).unwrap_or(BlockId::AIR)
    }
    fn fill(&mut self, x: (i32, i32), y: (i32, i32), z: (i32, i32), block: BlockId) {
        for yy in y.0..=y.1 {
            for zz in z.0..=z.1 {
                for xx in x.0..=x.1 {
                    self.set(xx, yy, zz, block);
                }
            }
        }
    }
    /// Sol plat : `top` en surface, `under` dessous, avec des taches de `patch`
    fn ground(&mut self, top: BlockId, under: BlockId, patch: Option<(BlockId, f32)>) {
        for z in 0..D {
            for x in 0..W {
                self.fill((x, x), (0, G - 2), (z, z), under);
                let t = match patch {
                    Some((p, chance)) if hash(x / 2, z / 2, 7) < chance => p,
                    _ => top,
                };
                self.set(x, G - 1, z, t);
            }
        }
    }
    /// Maison : murs, angles, plancher, porte vers +x et fenêtres de verre, toit à deux pans
    #[allow(clippy::too_many_arguments)]
    fn house(&mut self, x0: i32, y0: i32, z0: i32, w: i32, d: i32, h: i32, wall: BlockId, corner: BlockId, roof: BlockId, ridge: BlockId) {
        let (x1, z1) = (x0 + w - 1, z0 + d - 1);
        self.fill((x0, x1), (y0 - 1, y0 - 1), (z0, z1), corner);
        for y in y0..y0 + h {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let edge_x = x == x0 || x == x1;
                    let edge_z = z == z0 || z == z1;
                    if edge_x && edge_z {
                        self.set(x, y, z, corner);
                    } else if edge_x || edge_z {
                        let window = y == y0 + 1 && h > 2 && ((x - x0) % 3 == 1 && edge_z || (z - z0) % 3 == 1 && edge_x);
                        self.set(x, y, z, if window { b("glass") } else { wall });
                    } else {
                        self.set(x, y, z, BlockId::AIR);
                    }
                }
            }
        }
        // Porte sur la face +x
        let zc = z0 + d / 2;
        self.fill((x1, x1), (y0, y0 + 1), (zc, zc), BlockId::AIR);
        // Toit à deux pans le long de x
        let mut k = 0;
        loop {
            let (za, zb) = (z0 - 1 + k, z1 + 1 - k);
            if za > zb {
                break;
            }
            let y = y0 + h + k;
            if za >= zb - 1 {
                self.fill((x0 - 1, x1 + 1), (y, y), (za, zb), ridge);
                break;
            }
            self.fill((x0 - 1, x1 + 1), (y, y), (za, za), roof);
            self.fill((x0 - 1, x1 + 1), (y, y), (zb, zb), roof);
            // Pignons
            for z in za + 1..zb {
                self.set(x0, y, z, wall);
                self.set(x1, y, z, wall);
            }
            k += 1;
        }
    }
    /// Hutte ronde : murs en cercle, dôme
    fn hut(&mut self, cx: i32, y0: i32, cz: i32, r: i32, h: i32, wall: BlockId, roof: BlockId) {
        let rr = r as f32 + 0.5;
        for z in cz - r - 1..=cz + r + 1 {
            for x in cx - r - 1..=cx + r + 1 {
                let dd = (((x - cx) * (x - cx) + (z - cz) * (z - cz)) as f32).sqrt();
                if dd <= rr {
                    for y in y0..y0 + h {
                        self.set(x, y, z, if dd > rr - 1.0 { wall } else { BlockId::AIR });
                    }
                }
                // Dôme
                for k in 0..=r + 1 {
                    let rk = rr + 0.3 - k as f32 * rr / (r as f32 + 1.0);
                    if dd <= rk && dd > rk - 1.4 {
                        self.set(x, y0 + h + k, z, roof);
                    }
                }
            }
        }
        self.fill((cx + r, cx + r), (y0, y0 + 1), (cz, cz), BlockId::AIR);
    }
    fn tree(&mut self, x: i32, y0: i32, z: i32, h: i32, log: BlockId, leaf: BlockId, r: i32) {
        for y in y0..y0 + h {
            self.set(x, y, z, log);
        }
        let top = y0 + h;
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy * 2 + dz * dz <= r * r + 1 && self.get(x + dx, top + dy, z + dz).is_air() {
                        self.set(x + dx, top + dy, z + dz, leaf);
                    }
                }
            }
        }
    }
    fn pine(&mut self, x: i32, y0: i32, z: i32, h: i32) {
        let log = b("pineTrunk");
        let leaf = b("darkLeaf");
        for y in y0..y0 + h {
            self.set(x, y, z, log);
        }
        for k in 0..h - 2 {
            let r = (h - 2 - k) / 3 + if k % 2 == 0 { 1 } else { 0 };
            let y = y0 + 2 + k;
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() + dz.abs() <= r && !(dx == 0 && dz == 0) {
                        self.set(x + dx, y, z + dz, leaf);
                    }
                }
            }
        }
        self.set(x, y0 + h, z, leaf);
    }
    fn scatter(&mut self, names: &[&str], chance: f32, k: u32) {
        let flora: Vec<BlockId> = names.iter().map(|n| b(n)).collect();
        for z in 1..D - 1 {
            for x in 1..W - 1 {
                if hash(x, z, k) < chance {
                    let mut y = H - 1;
                    while y > 0 && self.get(x, y, z).is_air() {
                        y -= 1;
                    }
                    let below = self.get(x, y, z);
                    if below == b("grass") || below == b("dirt") || below == b("snow") || below == b("sand") {
                        self.set(x, y + 1, z, flora[(hash(x, z, k + 1) * flora.len() as f32) as usize % flora.len()]);
                    }
                }
            }
        }
    }
}

/// Demi-orques : camp de clan en savane, palissade de pieux, huttes basses de basalte, forge et arène
fn demi_orques() -> Scene {
    let mut s = Scene::new();
    s.ground(b("grass"), b("dirt"), Some((b("ash"), 0.18)));
    let (cx, cz) = (22, 22);
    // Palissade de pieux taillés en pointe, ouverte vers +x
    for a in 0..360 {
        let t = (a as f32).to_radians();
        let (x, z) = ((cx as f32 + 19.0 * t.cos()).round() as i32, (cz as f32 + 19.0 * t.sin()).round() as i32);
        if (x - cx) > 15 && (z - cz).abs() < 3 {
            continue;
        }
        let h = 4 + ((x + z) & 1);
        s.fill((x, x), (G, G + h - 1), (z, z), b("pineTrunk"));
    }
    // Totems à l'entrée : pieux coiffés de fer
    for z in [cz - 3, cz + 3] {
        s.fill((cx + 19, cx + 19), (G, G + 6), (z, z), b("pineTrunk"));
        s.set(cx + 19, G + 7, z, b("ironOrnate"));
        s.set(cx + 19, G + 5, z + if z < cz { -1 } else { 1 }, b("ironOrnate"));
    }
    // Huttes basses
    let basalt = b("basalt");
    let pine = b("pineTrunk");
    let peat = b("peat");
    s.house(5, G, 6, 8, 7, 3, basalt, pine, peat, b("ironPlate"));
    s.house(6, G, 27, 7, 8, 3, basalt, pine, peat, b("ironPlate"));
    s.house(27, G, 5, 7, 6, 3, basalt, pine, peat, b("ironPlate"));
    s.house(28, G, 31, 6, 6, 3, basalt, pine, peat, b("ironPlate"));
    // Arène au centre : anneau de pierre dure et sable
    for z in cz - 6..=cz + 6 {
        for x in cx - 6..=cx + 6 {
            let dd = (((x - cx) * (x - cx) + (z - cz) * (z - cz)) as f32).sqrt();
            if dd <= 6.4 {
                s.set(x, G - 1, z, b("sand"));
            }
            if dd > 5.4 && dd <= 6.4 && !((x - cx) > 4 && (z - cz).abs() < 2) {
                s.fill((x, x), (G, G), (z, z), b("hardStone"));
            }
        }
    }
    // Forge : fourneau, enclume de fer et fosse de lave
    s.set(cx - 2, G, cz - 9, b("furnace"));
    s.set(cx, G, cz - 9, b("ironPlate"));
    s.set(cx + 2, G - 1, cz - 9, b("lava"));
    s.fill((cx - 3, cx + 3), (G + 3, G + 3), (cz - 10, cz - 8), b("plankSlab"));
    for (x, z) in [(cx - 3, cz - 10), (cx + 3, cz - 10), (cx - 3, cz - 8), (cx + 3, cz - 8)] {
        s.fill((x, x), (G, G + 2), (z, z), pine);
    }
    s.scatter(&["deadBush"], 0.03, 3);
    s
}

/// Revenants : hameau sur pilotis au bord d'un lac, pierre sombre et bois flotté, lanternes bleues,
/// une maison à moitié sous l'eau et un ossuaire sur la rive
fn revenants() -> Scene {
    let mut s = Scene::new();
    s.ground(b("sand"), b("sand"), Some((b("gravel"), 0.15)));
    // Lac sur la moitié avant, fond de boue
    for z in 0..D {
        for x in 0..W {
            if x + z > 40 {
                s.fill((x, x), (1, 1), (z, z), b("mud"));
                s.fill((x, x), (2, G - 1), (z, z), b("water"));
            }
        }
    }
    let log = b("oakTrunk");
    let stone = b("hardStone");
    let roof = b("flintBrick");
    // Pilotis et passerelles à G + 2
    let deck = G + 2;
    for &(x0, z0, w, d) in &[(24, 8, 8, 7), (8, 26, 8, 7), (26, 24, 7, 7)] {
        for (x, z) in [(x0, z0), (x0 + w - 1, z0), (x0, z0 + d - 1), (x0 + w - 1, z0 + d - 1)] {
            s.fill((x, x), (2, deck - 1), (z, z), log);
        }
        s.fill((x0, x0 + w - 1), (deck - 1, deck - 1), (z0, z0 + d - 1), b("plank"));
        s.house(x0 + 1, deck, z0 + 1, w - 2, d - 2, 3, stone, log, roof, b("voidStone"));
    }
    // Passerelles
    s.fill((16, 23), (deck - 1, deck - 1), (29, 30), b("plankSlab"));
    s.fill((28, 29), (deck - 1, deck - 1), (15, 23), b("plankSlab"));
    s.fill((16, 21), (deck - 1, deck - 1), (13, 14), b("plankSlab"));
    // Maison à moitié noyée : plancher sous l'eau
    s.house(32, 2, 33, 6, 6, 6, stone, log, roof, b("voidStone"));
    for y in 2..G {
        for z in 34..38 {
            for x in 33..37 {
                s.set(x, y, z, b("water"));
            }
        }
    }
    // Lanternes bleues sur poteaux
    for &(x, z) in &[(22, 16), (18, 28), (30, 21), (12, 12), (25, 33)] {
        let y0 = if x + z > 40 { deck } else { G };
        s.fill((x, x), (y0, y0 + 2), (z, z), log);
        s.set(x, y0 + 3, z, b("glowbellBloom"));
    }
    // Ossuaire sur la rive, à la place du puits
    let (ox, oz) = (12, 12);
    s.fill((ox - 2, ox + 2), (G, G), (oz - 2, oz + 2), b("hardStoneCut"));
    s.fill((ox - 1, ox + 1), (G + 1, G + 3), (oz - 1, oz + 1), b("voidStone"));
    s.fill((ox, ox), (G + 1, G + 2), (oz + 1, oz + 1), BlockId::AIR);
    s.fill((ox - 1, ox + 1), (G + 4, G + 4), (oz - 1, oz + 1), b("hardStoneCut"));
    s.set(ox, G + 5, oz, b("glowbellBloom"));
    s.tree(4, G, 20, 5, b("birchTrunk"), b("lightLeaf"), 2);
    s.scatter(&["deadBush", "brownShroom"], 0.05, 9);
    s
}

/// Sylphes : maisons hautes et légères au bord d'une falaise enneigée, verre et cristal, passerelles
fn sylphes() -> Scene {
    let mut s = Scene::new();
    s.ground(b("snow"), b("stone"), Some((b("stone"), 0.2)));
    // Falaise : un plateau haut à l'arrière (petits x + z)
    let top = G + 9;
    for z in 0..D {
        for x in 0..W {
            let edge = 26 + (hash(x / 3, z / 3, 4) * 4.0) as i32;
            if x + z < edge {
                s.fill((x, x), (G - 1, top - 2), (z, z), b("stone"));
                s.set(x, top - 1, z, b("snow"));
            }
        }
    }
    let birch = b("birchTrunk");
    let white = b("brickWhitewashed");
    let crystal = b("cristalTaille");
    // Tours fines perchées sur la falaise, flèches de cristal
    for &(x0, z0, h) in &[(3, 4, 7), (11, 2, 9), (3, 13, 8)] {
        s.house(x0, top, z0, 5, 5, h, white, birch, b("glass"), crystal);
        s.set(x0 + 2, top + h + 4, z0 + 2, b("crystal"));
    }
    // Une tour sur pilier au-dessus du vide
    let (px, pz) = (22, 18);
    s.fill((px + 1, px + 3), (G, top - 1), (pz + 1, pz + 3), b("hardStoneCut"));
    s.house(px, top, pz, 5, 5, 6, white, birch, b("glass"), crystal);
    // Passerelles de planches vers la tour
    s.fill((14, px - 1), (top - 1, top - 1), (pz + 2, pz + 2), b("plankSlab"));
    // Carillons de mithril au bord
    for &(x, z) in &[(9, 16), (16, 8), (14, 14)] {
        s.fill((x, x), (top, top + 3), (z, z), birch);
        s.set(x, top + 4, z, b("ornementMithril"));
    }
    // En contrebas, sol de neige et quelques sapins
    s.pine(34, G, 10, 8);
    s.pine(38, G, 24, 7);
    s.pine(14, G, 36, 9);
    s.pine(30, G, 36, 6);
    s.scatter(&["snowball", "tallGrass1"], 0.02, 5);
    s
}

/// Gnomes : maisons à moitié enterrées dans des buttes, toits d'herbe, portes d'un bloc, ateliers
/// et cheminées
fn gnomes() -> Scene {
    let mut s = Scene::new();
    s.ground(b("grass"), b("dirt"), None);
    let bronze = b("ornementBronze");
    // Buttes : demi-sphères de terre coiffées d'herbe, une porte ronde et une fenêtre ronde vers +x
    for &(cx, cz, r, big) in &[(10, 10, 6, false), (10, 30, 5, false), (28, 9, 5, true), (31, 28, 6, false)] {
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                let dd2 = (x - cx) * (x - cx) + (z - cz) * (z - cz);
                if dd2 > r * r {
                    continue;
                }
                let hgt = (((r * r - dd2) as f32).sqrt() * 0.9) as i32;
                s.fill((x, x), (G, G + hgt), (z, z), b("grass"));
            }
        }
        // Porte basse d'un bloc pour les gnomes, encadrée de bronze ; grande porte pour les autres
        let fx = cx + r - 1;
        let door_h = if big { 2 } else { 1 };
        s.fill((cx, fx + 1), (G, G + door_h - 1), (cz, cz), BlockId::AIR);
        s.set(fx + 1, G + door_h, cz, bronze);
        s.set(fx + 1, G, cz - 1, bronze);
        s.set(fx + 1, G, cz + 1, bronze);
        // Fenêtre ronde
        s.set(fx, G + 2, cz + 2, b("glass"));
        s.set(cx + 2, G + 2, cz + r - 1, b("glass"));
        // Cheminée d'atelier
        s.fill((cx - 2, cx - 2), (G, G + r + 2), (cz - 1, cz - 1), b("brick"));
        s.set(cx - 2, G + r + 3, cz - 1, b("ironPlate"));
    }
    // Ateliers en plein air : établi, fourneau, alambics de cuivre
    s.set(20, G, 20, b("workbench"));
    s.set(21, G, 20, b("furnace"));
    s.set(20, G, 22, b("copperOrnate"));
    s.set(22, G, 22, b("copperPlate"));
    s.set(22, G + 1, 22, b("glass"));
    // Lanternes partout
    for &(x, z) in &[(18, 16), (24, 24), (17, 26), (25, 15), (36, 18), (6, 20)] {
        s.fill((x, x), (G, G + 1), (z, z), b("barriereBronze"));
        s.set(x, G + 2, z, b("lanterne"));
    }
    s.tree(38, G, 6, 5, b("oakTrunk"), b("greenLeaf"), 3);
    s.tree(4, G, 39, 4, b("birchTrunk"), b("lightLeaf"), 2);
    s.scatter(&["redFlower", "yellowFlower", "tulip"], 0.03, 11);
    s
}

/// Aasimars : sanctuaire de pierre blanche et d'or sur un plateau, temple lumineux au centre et
/// terrasses hautes qu'on n'atteint qu'en volant
fn aasimars() -> Scene {
    let mut s = Scene::new();
    s.ground(b("grass"), b("stone"), Some((b("stone"), 0.12)));
    let white = b("brickWhitewashed");
    let cut = b("hardStoneCut");
    let gold = b("ornementOr");
    // Parvis
    s.fill((12, 31), (G - 1, G - 1), (12, 31), cut);
    // Temple : colonnade, toit plat doré, lumière au sommet
    let (x0, z0, x1, z1) = (15, 15, 28, 28);
    for x in (x0..=x1).step_by(3) {
        for z in [z0, z1] {
            s.fill((x, x), (G, G + 6), (z, z), cut);
        }
    }
    for z in (z0..=z1).step_by(3) {
        for x in [x0, x1] {
            s.fill((x, x), (G, G + 6), (z, z), cut);
        }
    }
    s.fill((x0, x1), (G + 7, G + 7), (z0, z1), white);
    s.fill((x0, x1), (G + 8, G + 8), (z0, z0), gold);
    s.fill((x0, x1), (G + 8, G + 8), (z1, z1), gold);
    s.fill((x0, x0), (G + 8, G + 8), (z0, z1), gold);
    s.fill((x1, x1), (G + 8, G + 8), (z0, z1), gold);
    s.fill((19, 24), (G + 8, G + 11), (19, 24), white);
    s.fill((20, 23), (G + 12, G + 13), (20, 23), gold);
    s.fill((21, 22), (G + 14, G + 15), (21, 22), b("crystal"));
    s.fill((21, 22), (G, G), (21, 22), b("glowbellBloom"));
    // Terrasses hautes sur colonnes, sans escalier
    for &(x, z, h) in &[(5, 6, 12), (34, 5, 14), (6, 35, 10)] {
        s.fill((x, x + 1), (G, G + h - 1), (z, z + 1), cut);
        s.fill((x - 2, x + 3), (G + h, G + h), (z - 2, z + 3), white);
        s.fill((x - 2, x + 3), (G + h + 1, G + h + 1), (z - 2, z - 2), gold);
        s.set(x, G + h + 1, z + 1, b("crystal"));
    }
    // Maisons blanches au faîte d'or
    s.house(33, G, 16, 6, 6, 4, white, cut, b("cobbleStoneStairs"), gold);
    s.house(16, G, 34, 7, 6, 4, white, cut, b("cobbleStoneStairs"), gold);
    s.house(34, G, 31, 6, 6, 4, white, cut, b("cobbleStoneStairs"), gold);
    s.scatter(&["lavender", "iris"], 0.03, 13);
    s
}

/// Hommes-bêtes : huttes de rondins et de feuillage sur des plateformes à deux blocs, sans escalier,
/// mâts totems et enclos de chasse, entre de grands arbres
fn hommes_betes() -> Scene {
    let mut s = Scene::new();
    s.ground(b("grass"), b("dirt"), Some((b("mud"), 0.1)));
    let oak = b("oakTrunk");
    let plank = b("plank");
    // Plateformes : le dessus à G + 2, deux blocs de saut
    for &(cx, cz) in &[(9, 9), (9, 27), (25, 8), (27, 26)] {
        let r = 4;
        for (x, z) in [(cx - r, cz - r), (cx + r, cz - r), (cx - r, cz + r), (cx + r, cz + r)] {
            s.fill((x, x), (G, G + 1), (z, z), oak);
        }
        s.fill((cx - r, cx + r), (G + 1, G + 1), (cz - r, cz + r), plank);
        s.hut(cx, G + 2, cz, 3, 3, oak, b("peat"));
    }
    // Mâts totems de couleurs
    for &(x, z, top) in &[(19, 18, "copperOrnate"), (19, 22, "ornementOr"), (37, 18, "redLeaf")] {
        s.fill((x, x), (G, G + 7), (z, z), oak);
        s.set(x, G + 8, z, b(top));
        s.set(x + 1, G + 6, z, b(top));
        s.set(x - 1, G + 6, z, b(top));
    }
    // Enclos de chasse
    for x in 30..=40 {
        s.set(x, G, 35, b("barriereChene"));
        s.set(x, G, 42, b("barriereChene"));
    }
    for z in 35..=42 {
        s.set(30, G, z, b("barriereChene"));
        s.set(40, G, z, b("barriereChene"));
    }
    // Grands arbres de jungle
    s.tree(3, G, 18, 9, oak, b("greenLeaf"), 4);
    s.tree(38, G, 4, 10, oak, b("darkLeaf"), 4);
    s.tree(18, G, 39, 8, oak, b("greenLeaf"), 3);
    s.scatter(&["tallGrass2", "redFlower"], 0.04, 17);
    s
}

fn color(block: BlockId, top: bool) -> [f32; 3] {
    let c = blocks().get(BlockId(block.0 & !(0x7 << BlockId::ROTATION_SHIFT))).map(|b| if top { b.top_color() } else { b.side_color() }).unwrap_or([1.0, 0.0, 1.0, 1.0]);
    [c[0], c[1], c[2]]
}

fn render(s: &Scene, out: &str) {
    let (w, d, h) = (W, D, H);
    let water = b("water");
    let solid = |x: i32, y: i32, z: i32| {
        let bl = s.get(x, y, z);
        !bl.is_air() && bl != water
    };
    let width = (w + d) * S;
    let height = (w + d) * S / 2 + h * S + S;
    let mut img = vec![[24u8, 28, 36]; (width * height) as usize];
    let mut depth = vec![i32::MIN; (width * height) as usize];
    let origin = (d * S, h * S);
    for y in 0..h {
        for z in 0..d {
            for x in 0..w {
                let bl = s.get(x, y, z);
                if bl.is_air() || (bl == water && s.get(x, y + 1, z) == water) {
                    continue;
                }
                if bl != water && solid(x, y + 1, z) && solid(x + 1, y, z) && solid(x, y, z + 1) {
                    continue;
                }
                let px = origin.0 + (x - z) * S;
                let py = origin.1 + (x + z) * S / 2 - y * S;
                let key = x + z + y;
                let top = color(bl, true);
                let side = color(bl, false);
                let lift = 0.78 + 0.22 * (y as f32 / h as f32);
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
                        // Fines arêtes sombres pour lire les blocs
                        let rim = if dx == 0 || dy == 0 || (v - half).abs() < 0.8 { 0.88 } else { 1.0 };
                        img[i] = [c[0], c[1], c[2]].map(|v| ((v * k * lift * rim).clamp(0.0, 1.0) * 255.0) as u8);
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

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let scenes: [(&str, fn() -> Scene); 6] = [
        ("demiOrque", demi_orques),
        ("revenant", revenants),
        ("sylphe", sylphes),
        ("gnome", gnomes),
        ("aasimar", aasimars),
        ("hommeBete", hommes_betes),
    ];
    for (id, build) in scenes {
        let out = format!("{dir}/village-{id}.ppm");
        render(&build(), &out);
        println!("{id} → {out}");
    }
}

//! Carte du monde (touche M) et boussole du HUD.
//!
//! La carte est vue de dessus, le nord en haut, centrée sur le joueur à l'ouverture. Elle suit la
//! courbure de la planète (projection azimutale équidistante autour du centre de la carte) : les
//! distances depuis le centre sont justes et la carte passe d'une face du cube à l'autre sans
//! couture. Le relief vient du générateur, colonne par colonne, ombré par la pente ; les villes,
//! villages et ruines, leurs noms et les chemins s'y ajoutent. La molette zoome (de 1 à 64 blocs
//! par pixel), un glissé déplace la carte, un clic droit pose ou retire un repère.
//!
//! La boussole, en haut de l'écran, montre le cap du regard, les établissements des environs et le
//! repère, avec la distance de celui que l'on vise.
//!
//! Tout le calcul (relief, plans des établissements, tracé des chemins) se fait dans un fil à part :
//! la carte s'affiche dès que le relief est prêt, les chemins arrivent ensuite.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use glam::{DVec2, DVec3};
use herocraft_common::PlanetFace;
use herocraft_world::edits::FACE_BLOCKS;
use herocraft_world::generator::{BiomeType, SEA_LEVEL_BLOCKS};
use herocraft_world::structures::{Rect, CELL, REGIONS_PER_FACE, REGION_BLOCKS};
use herocraft_world::{PlanetWorldGenerator, SettlementKind};
use rayon::prelude::*;

use crate::theme::{GOLD, GOLD_BRIGHT, TEXT_DIM, TEXT_SOFT};

/// Côté de l'image du relief, en pixels de carte
pub const MAP_PIXELS: usize = 320;
/// Échelles de la carte, en blocs par pixel de carte
pub const ZOOM_LEVELS: [f64; 7] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];
/// Échelle à l'ouverture : 4 blocs par pixel, soit 1 280 blocs de côté
const DEFAULT_ZOOM: usize = 2;
/// Au-delà de cette échelle, les plans des établissements ne sont plus dessinés dans le relief
const FOOTPRINT_MAX_BPP: f64 = 4.0;
/// Rayon des environs que la boussole connaît, en blocs
pub const COMPASS_RANGE: f64 = 1600.0;
/// Le joueur doit s'être déplacé d'autant (en blocs) pour que les environs soient relus
const NEARBY_REFRESH: f64 = 200.0;
/// Délai minimal entre deux demandes de relief pendant un glissé
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);
/// Chemins tracés par lot entre deux vérifications d'une demande plus récente
const ROAD_BATCH: usize = 4;

/// Rayon de la planète en blocs : un bloc au centre d'une face a la largeur d'un pas d'angle de π/2
/// divisé par le nombre de blocs de la face
pub fn planet_radius_blocks() -> f64 {
    FACE_BLOCKS as f64 * 2.0 / PI
}

/// Repère local (vers le haut, l'est et le nord) en un point de la sphère, le même que celui de la
/// caméra : le nord pointe vers le pôle +Y
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub up: DVec3,
    pub east: DVec3,
    pub north: DVec3,
}

impl Frame {
    pub fn at(up: DVec3) -> Self {
        let up = up.normalize();
        if up.y.abs() < 0.999 {
            let north = (DVec3::Y - up * up.y).normalize();
            Self { up, east: north.cross(up).normalize(), north }
        } else {
            let sign = if up.y >= 0.0 { 1.0 } else { -1.0 };
            Self { up, east: DVec3::X, north: DVec3::new(0.0, 0.0, -sign) }
        }
    }

    /// Direction sur la sphère d'un point de la carte, donné en blocs vers l'est et le nord
    pub fn unproject(&self, offset: DVec2) -> DVec3 {
        let dist = offset.length();
        if dist < 1e-9 {
            return self.up;
        }
        let angle = dist / planet_radius_blocks();
        let tangent = (self.east * offset.x + self.north * offset.y) / dist;
        (self.up * angle.cos() + tangent * angle.sin()).normalize()
    }

    /// Place d'une direction sur la carte, en blocs vers l'est et le nord (inverse de `unproject`)
    pub fn project(&self, dir: DVec3) -> DVec2 {
        let dir = dir.normalize();
        let cos = self.up.dot(dir).clamp(-1.0, 1.0);
        let tangent = dir - self.up * cos;
        let len = tangent.length();
        if len < 1e-12 {
            return DVec2::ZERO;
        }
        let dist = cos.acos() * planet_radius_blocks();
        DVec2::new(tangent.dot(self.east), tangent.dot(self.north)) / len * dist
    }

    /// Cap (en radians, 0 au nord, dans le sens des aiguilles d'une montre) et distance en blocs
    /// d'une direction
    pub fn bearing_distance(&self, dir: DVec3) -> (f64, f64) {
        let p = self.project(dir);
        (p.x.atan2(p.y).rem_euclid(TAU), p.length())
    }
}

/// Une ville, un village ou des ruines, tels que la carte et la boussole les montrent
#[derive(Debug, Clone)]
pub struct Place {
    pub kind: SettlementKind,
    pub name: String,
    pub dir: DVec3,
}

/// Clé d'un établissement : sa face et la colonne de son centre
type PlaceKey = (usize, (i32, i32));
/// Clé d'un chemin : sa face et les centres des deux établissements reliés
type RoadKey = (usize, (i32, i32), (i32, i32));

/// Où est le joueur et où il regarde, donné par le jeu à chaque image (au sol)
#[derive(Debug, Clone, Copy)]
pub struct Navigation {
    /// Direction du joueur depuis le centre de la planète
    pub up: DVec3,
    /// Cap du regard (0 au nord, sens des aiguilles d'une montre)
    pub yaw: f64,
}

// ---------------------------------------------------------------------------
// Fil de calcul
// ---------------------------------------------------------------------------

enum Job {
    Map { seed: u64, generator: Box<PlanetWorldGenerator>, center: DVec3, bpp: f64 },
    Nearby { seed: u64, generator: Box<PlanetWorldGenerator>, center: DVec3 },
}

enum Reply {
    Terrain { seed: u64, center: DVec3, bpp: f64, image: egui::ColorImage },
    Places { seed: u64, places: Vec<(PlaceKey, Place)> },
    Roads { seed: u64, roads: Vec<(RoadKey, Vec<DVec3>)> },
}

/// Colonne d'une face sous une direction
fn column_of(dir: DVec3) -> (PlanetFace, i32, i32) {
    let (face, u, v) = PlanetFace::sphere_direction_to_face_uv(dir);
    let to_block = |t: f64| (((t + 1.0) * 0.5 * FACE_BLOCKS as f64).floor() as i32).clamp(0, FACE_BLOCKS - 1);
    (face, to_block(u), to_block(v))
}

/// Direction du centre de la colonne (x, z) d'une face (coordonnées continues de blocs)
pub fn dir_of(face: PlanetFace, x: f64, z: f64) -> DVec3 {
    let span = 2.0 / FACE_BLOCKS as f64;
    face.uv_to_sphere_direction(-1.0 + x * span, -1.0 + z * span)
}

fn face_index(face: PlanetFace) -> usize {
    PlanetFace::ALL.iter().position(|f| *f == face).unwrap_or(0)
}

/// Nom d'un établissement, tiré de la graine et de sa place : le même pour tous les joueurs
pub fn place_name(seed: u64, face: PlanetFace, center: (i32, i32), kind: SettlementKind) -> String {
    const HEADS: [&str; 40] = [
        "Bel", "Mont", "Roche", "Val", "Brume", "Haute", "Clair", "Fonte", "Gris", "Bois", "Lune", "Pierre", "Cendre", "Saule",
        "Givre", "Ronce", "Orme", "Aube", "Corbe", "Fer", "Sombre", "Vieux", "Ambre", "Rive", "Chêne", "Loup", "Blanche", "Noire",
        "Grand", "Beau", "Frêne", "Bruy", "Mousse", "Rouge", "Sable", "Houx", "Crête", "Vert", "Saint-Aure", "Hêtre",
    ];
    const TAILS: [&str; 32] = [
        "val", "mont", "fort", "lac", "bourg", "combe", "ville", "pierre", "roc", "mer", "lieu", "champ", "court", "pré", "fond",
        "garde", "brune", "mare", "vent", "aigue", "castel", "ferme", "font", "lande", "moulin", "rive", "brande", "haie", "marche",
        "bois", "sauge", "colline",
    ];
    let mut h = seed ^ 0x9e37_79b9_7f4a_7c15;
    for v in [face_index(face) as u64, center.0 as u64, center.1 as u64] {
        h = (h ^ v).wrapping_mul(0x0100_0000_01b3).rotate_left(23).wrapping_add(0x632b_e59b_d9b4_e019);
    }
    h ^= h >> 31;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 29;
    let head = HEADS[(h % HEADS.len() as u64) as usize];
    let mut tail = TAILS[((h >> 16) % TAILS.len() as u64) as usize];
    if head.to_lowercase() == tail || head.to_lowercase().ends_with(tail) {
        tail = TAILS[((h >> 16).wrapping_add(7) % TAILS.len() as u64) as usize];
    }
    let name = format!("{head}{tail}");
    match kind {
        SettlementKind::Ruins => format!("Ruines de {name}"),
        _ => name,
    }
}

/// Établissements des cases données (cases déjà dessinées ou dessinées à cette lecture)
fn places_in_cells(generator: &PlanetWorldGenerator, seed: u64, cells: &HashSet<(usize, i32, i32)>) -> Vec<(PlaceKey, Place)> {
    let cells: Vec<_> = cells.iter().copied().collect();
    cells
        .par_iter()
        .filter_map(|&(f, cx, cz)| {
            let face = PlanetFace::ALL[f];
            let s = generator.structures.cell(generator, face, cx, cz)?;
            let dir = dir_of(face, s.center.0 as f64 + 0.5, s.center.1 as f64 + 0.5);
            Some(((f, s.center), Place { kind: s.kind, name: place_name(seed, face, s.center, s.kind), dir }))
        })
        .collect()
}

/// Établissements à moins de `COMPASS_RANGE` blocs de `center`
fn nearby_places(generator: &PlanetWorldGenerator, seed: u64, center: DVec3) -> Vec<(PlaceKey, Place)> {
    let frame = Frame::at(center);
    let step = CELL as f64 / 2.0;
    let n = (COMPASS_RANGE / step).ceil() as i32;
    let mut cells = HashSet::new();
    for j in -n..=n {
        for i in -n..=n {
            let offset = DVec2::new(i as f64, j as f64) * step;
            if offset.length() <= COMPASS_RANGE + CELL as f64 {
                let (face, x, z) = column_of(frame.unproject(offset));
                cells.insert((face_index(face), x.div_euclid(CELL), z.div_euclid(CELL)));
            }
        }
    }
    places_in_cells(generator, seed, &cells)
}

/// Lavis d'une colonne, comme peint à l'aquarelle sur le parchemin (avant les traits d'encre)
fn ground_color(biome: BiomeType, elevation: f64) -> [f32; 3] {
    let h = elevation - SEA_LEVEL_BLOCKS;
    if h < 0.0 {
        // Eau : lavis bleu-vert, un peu plus soutenu au large
        let t = (-h / 80.0).clamp(0.0, 1.0) as f32;
        let shallow = [0.66_f32, 0.77, 0.74];
        let deep = [0.47_f32, 0.60, 0.65];
        return std::array::from_fn(|i| shallow[i] * (1.0 - t) + deep[i] * t);
    }
    let base: [f32; 3] = match biome {
        BiomeType::Beach => [0.91, 0.84, 0.64],
        BiomeType::Plains => [0.80, 0.82, 0.58],
        BiomeType::Forest => [0.60, 0.68, 0.45],
        BiomeType::Jungle => [0.50, 0.64, 0.40],
        BiomeType::Taiga => [0.55, 0.62, 0.50],
        BiomeType::Savanna => [0.87, 0.80, 0.55],
        BiomeType::Desert => [0.93, 0.82, 0.58],
        BiomeType::Mountains => [0.73, 0.67, 0.57],
        BiomeType::SnowPeaks => [0.96, 0.95, 0.91],
        BiomeType::Tundra => [0.82, 0.81, 0.71],
        BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea => [0.91, 0.84, 0.64],
    };
    // Les hauteurs pâlissent, comme sur une carte en relief
    let lift = ((h / 300.0).clamp(0.0, 1.0) * 0.30) as f32;
    std::array::from_fn(|i| base[i] * (1.0 - lift) + 0.94 * lift)
}

/// Couleur d'une colonne bâtie, selon le type d'établissement
fn footprint_color(kind: SettlementKind) -> [f32; 3] {
    match kind {
        SettlementKind::Town => [0.66, 0.38, 0.28],
        SettlementKind::Village => [0.72, 0.52, 0.34],
        SettlementKind::Ruins => [0.58, 0.54, 0.48],
    }
}

/// Encre brune des traits de la carte
const INK: [f32; 3] = [0.29, 0.20, 0.13];

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t)
}

/// Bruit déterministe d'une colonne (pointillés des forêts)
fn column_hash(f: usize, x: i32, z: i32) -> u32 {
    let mut h = (f as u32).wrapping_mul(0x9e37_79b1) ^ (x as u32).wrapping_mul(0x85eb_ca6b) ^ (z as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Résultat du relief : l'image, et les cases et régions qu'elle couvre
struct Terrain {
    image: egui::ColorImage,
    cells: HashSet<(usize, i32, i32)>,
    regions: Vec<(usize, i32, i32)>,
}

fn render_terrain(generator: &PlanetWorldGenerator, center: DVec3, bpp: f64) -> Terrain {
    herocraft_common::prof_scope!("world_map::render_terrain");
    let frame = Frame::at(center);
    let n = MAP_PIXELS;
    let half = n as f64 / 2.0;
    // Altitude et couleur de chaque pixel, ligne par ligne en parallèle
    type Pixel = (f64, [f32; 3], bool, BiomeType, (usize, i32, i32));
    let rows: Vec<Vec<Pixel>> = (0..n)
        .into_par_iter()
        .map(|py| {
            (0..n)
                .map(|px| {
                    let offset = DVec2::new((px as f64 + 0.5 - half) * bpp, (half - py as f64 - 0.5) * bpp);
                    let (face, x, z) = column_of(frame.unproject(offset));
                    let column = generator.sample_block_column(face, x, z);
                    let mut color = ground_color(column.biome, column.elevation);
                    let mut built = false;
                    if bpp <= FOOTPRINT_MAX_BPP {
                        if let Some(s) = generator.settlement_at(face, x, z) {
                            if s.occupies(x, z, 0) {
                                color = footprint_color(s.kind);
                                built = true;
                            }
                        }
                    }
                    (column.elevation, color, built, column.biome, (face_index(face), x, z))
                })
                .collect()
        })
        .collect();
    let at = |px: usize, py: usize| &rows[py][px];
    let is_water = |px: usize, py: usize| rows[py][px].0 < SEA_LEVEL_BLOCKS;
    // Distance au rivage (en pixels) des pixels d'eau, pour les lignes d'eau parallèles à la côte
    const RIPPLES: [u16; 3] = [3, 6, 10];
    let mut shore = vec![u16::MAX; n * n];
    let mut queue = std::collections::VecDeque::new();
    let neighbours = |px: usize, py: usize| {
        [(px.wrapping_sub(1), py), (px + 1, py), (px, py.wrapping_sub(1)), (px, py + 1)].into_iter().filter(move |&(x, y)| x < n && y < n)
    };
    for py in 0..n {
        for px in 0..n {
            if is_water(px, py) && neighbours(px, py).any(|(x, y)| !is_water(x, y)) {
                shore[py * n + px] = 1;
                queue.push_back((px, py));
            }
        }
    }
    while let Some((px, py)) = queue.pop_front() {
        let d = shore[py * n + px];
        if d >= RIPPLES[2] {
            continue;
        }
        for (x, y) in neighbours(px, py) {
            if is_water(x, y) && shore[y * n + x] == u16::MAX {
                shore[y * n + x] = d + 1;
                queue.push_back((x, y));
            }
        }
    }
    // Courbes de niveau : un trait d'encre fin tous les `step` blocs d'altitude
    let step = (bpp * 5.0).clamp(10.0, 160.0);
    let level = |h: f64| ((h - SEA_LEVEL_BLOCKS) / step).floor() as i64;
    let mut pixels = Vec::with_capacity(n * n);
    let mut cells = HashSet::new();
    let mut regions = HashSet::new();
    for py in 0..n {
        for px in 0..n {
            let &(elevation, color, built, biome, (f, x, z)) = at(px, py);
            cells.insert((f, x.div_euclid(CELL), z.div_euclid(CELL)));
            regions.insert((f, x.div_euclid(REGION_BLOCKS), z.div_euclid(REGION_BLOCKS)));
            let water = elevation < SEA_LEVEL_BLOCKS;
            let mut c = color;
            if water {
                let d = shore[py * n + px];
                if d == 1 {
                    // Bord de l'eau : un liseré plus soutenu
                    c = mix(c, [0.36, 0.48, 0.52], 0.45);
                } else if RIPPLES.contains(&d) {
                    c = mix(c, [0.36, 0.48, 0.52], if d == RIPPLES[2] { 0.18 } else { 0.30 });
                }
            } else if !built {
                // Ombrage adouci : la lumière vient du nord-ouest
                if px > 0 && py > 0 {
                    let nw = at(px - 1, py - 1).0.max(SEA_LEVEL_BLOCKS);
                    let slope = (elevation - nw) / (bpp * std::f64::consts::SQRT_2);
                    let lit = (1.0 + slope * 0.5).clamp(0.80, 1.14) as f32;
                    c = c.map(|v| v * lit);
                }
                // Trait de côte
                if neighbours(px, py).any(|(x, y)| is_water(x, y)) {
                    c = mix(c, INK, 0.85);
                } else {
                    // Courbes de niveau
                    let here = level(elevation);
                    let crosses = (px + 1 < n && level(at(px + 1, py).0.max(SEA_LEVEL_BLOCKS)) != here)
                        || (py + 1 < n && level(at(px, py + 1).0.max(SEA_LEVEL_BLOCKS)) != here);
                    if crosses {
                        c = mix(c, INK, 0.28);
                    } else if matches!(biome, BiomeType::Forest | BiomeType::Jungle | BiomeType::Taiga) && column_hash(f, x, z).is_multiple_of(9) {
                        // Forêts : pointillés d'encre verte
                        c = mix(c, [0.22, 0.32, 0.18], 0.45);
                    }
                }
            }
            let c = c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
            pixels.push(egui::Color32::from_rgb(c[0], c[1], c[2]));
        }
    }
    // Régions du centre d'abord : les chemins les plus proches arrivent les premiers
    let mut regions: Vec<_> = regions.into_iter().collect();
    let region_dist = |&(f, rx, rz): &(usize, i32, i32)| {
        let c = dir_of(PlanetFace::ALL[f], (rx as f64 + 0.5) * REGION_BLOCKS as f64, (rz as f64 + 0.5) * REGION_BLOCKS as f64);
        -c.dot(center)
    };
    regions.sort_by(|a, b| region_dist(a).total_cmp(&region_dist(b)));
    Terrain { image: egui::ColorImage::new([n, n], pixels), cells, regions }
}

/// Chemins qui passent par une région
fn roads_in_region(generator: &PlanetWorldGenerator, (f, rx, rz): (usize, i32, i32)) -> Vec<(RoadKey, Vec<DVec3>)> {
    if !(0..REGIONS_PER_FACE).contains(&rx) || !(0..REGIONS_PER_FACE).contains(&rz) {
        return Vec::new();
    }
    let face = PlanetFace::ALL[f];
    let area = Rect::new(rx * REGION_BLOCKS, rz * REGION_BLOCKS, (rx + 1) * REGION_BLOCKS, (rz + 1) * REGION_BLOCKS);
    generator
        .roads
        .touching(generator, face, area)
        .into_iter()
        .map(|road| {
            let points = road.points.iter().map(|&(x, z)| dir_of(face, x + 0.5, z + 0.5)).collect();
            ((f, road.from, road.to), points)
        })
        .collect()
}

fn worker(jobs: Receiver<Job>, replies: Sender<Reply>) {
    let mut pending: Option<Job> = None;
    loop {
        let first = match pending.take() {
            Some(job) => job,
            None => match jobs.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        // Seule la dernière demande de chaque sorte compte
        let (mut map, mut nearby) = (None, None);
        let take = |job: Job, map: &mut Option<Job>, nearby: &mut Option<Job>| match job {
            Job::Map { .. } => *map = Some(job),
            Job::Nearby { .. } => *nearby = Some(job),
        };
        take(first, &mut map, &mut nearby);
        loop {
            match jobs.try_recv() {
                Ok(job) => take(job, &mut map, &mut nearby),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some(Job::Nearby { seed, generator, center }) = nearby {
            let places = nearby_places(&generator, seed, center);
            if replies.send(Reply::Places { seed, places }).is_err() {
                return;
            }
        }
        let Some(Job::Map { seed, generator, center, bpp }) = map else { continue };
        let terrain = render_terrain(&generator, center, bpp);
        if replies.send(Reply::Terrain { seed, center, bpp, image: terrain.image }).is_err() {
            return;
        }
        let places = places_in_cells(&generator, seed, &terrain.cells);
        if replies.send(Reply::Places { seed, places }).is_err() {
            return;
        }
        // Chemins, par lots : une nouvelle demande interrompt le tracé (ce qui est tracé reste en
        // mémoire dans le générateur et ne sera pas refait)
        let mut sent: HashSet<RoadKey> = HashSet::new();
        for batch in terrain.regions.chunks(ROAD_BATCH) {
            match jobs.try_recv() {
                Ok(job) => {
                    pending = Some(job);
                    break;
                }
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => {}
            }
            let found: Vec<_> = batch.par_iter().flat_map_iter(|&r| roads_in_region(&generator, r)).collect();
            let roads: Vec<_> = found.into_iter().filter(|(key, _)| sent.insert(*key)).collect();
            if !roads.is_empty() && replies.send(Reply::Roads { seed, roads }).is_err() {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// État de la carte côté interface
// ---------------------------------------------------------------------------

/// Relief reçu du fil de calcul, prêt à dessiner
struct MapTexture {
    texture: egui::TextureHandle,
    center: DVec3,
    bpp: f64,
}

/// Ce que la carte et la boussole savent du monde, et l'état de l'écran de carte
pub struct WorldMap {
    jobs: Sender<Job>,
    replies: Receiver<Reply>,
    /// Graine du monde dont viennent les données ; elles sont oubliées quand elle change
    seed: Option<u64>,
    places: HashMap<PlaceKey, Place>,
    roads: HashMap<RoadKey, Vec<DVec3>>,
    texture: Option<MapTexture>,
    /// Centre et échelle de la dernière demande de relief
    requested: Option<(DVec3, f64)>,
    last_request: Option<Instant>,
    /// Point d'où les environs de la boussole ont été lus
    nearby_from: Option<DVec3>,
    /// Le relief demandé n'est pas encore arrivé
    pub loading: bool,
    /// Centre de la carte affichée
    pub center: DVec3,
    /// Rang de l'échelle dans `ZOOM_LEVELS`
    pub zoom: usize,
    /// Défilement de molette pas encore converti en cran de zoom
    scroll: f32,
    /// Repère posé d'un clic droit
    pub waypoint: Option<DVec3>,
    /// Position du joueur, mise à jour par le jeu au sol
    pub navigation: Option<Navigation>,
    /// Parchemin du fond de la carte, et le même en cadre dont le centre est transparent
    parchment: Option<(egui::TextureHandle, egui::TextureHandle)>,
}

impl Default for WorldMap {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldMap {
    pub fn new() -> Self {
        let (jobs, job_rx) = mpsc::channel();
        let (reply_tx, replies) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("carte".into()).spawn(move || {
            herocraft_common::threads::lower_current_thread_priority();
            worker(job_rx, reply_tx)
        });
        if let Err(e) = spawned {
            log::warn!("Fil de la carte impossible à lancer : {e}");
        }
        Self {
            jobs,
            replies,
            seed: None,
            places: HashMap::new(),
            roads: HashMap::new(),
            texture: None,
            requested: None,
            last_request: None,
            nearby_from: None,
            loading: false,
            center: DVec3::Y,
            zoom: DEFAULT_ZOOM,
            scroll: 0.0,
            waypoint: None,
            navigation: None,
            parchment: None,
        }
    }

    /// Échelle affichée, en blocs par pixel de carte
    pub fn bpp(&self) -> f64 {
        ZOOM_LEVELS[self.zoom]
    }

    /// Recentre la carte sur le joueur
    pub fn recenter(&mut self) {
        if let Some(nav) = self.navigation {
            self.center = nav.up;
        }
    }

    /// Établissements connus de la carte
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn places(&self) -> impl Iterator<Item = &Place> {
        self.places.values()
    }

    /// Nombre de chemins connus de la carte
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn road_count(&self) -> usize {
        self.roads.len()
    }

    /// Le relief de la vue demandée est arrivé
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn terrain_ready(&self) -> bool {
        self.texture.is_some() && !self.loading
    }

    /// À chaque image : envoie les demandes au fil de calcul et range ses réponses. `map_open` :
    /// l'écran de carte est ouvert et veut son relief.
    pub fn pump(&mut self, ctx: &egui::Context, generator: &PlanetWorldGenerator, map_open: bool) {
        let seed = generator.planet.seed;
        if self.seed != Some(seed) {
            self.seed = Some(seed);
            self.places.clear();
            self.roads.clear();
            self.texture = None;
            self.requested = None;
            self.nearby_from = None;
            self.waypoint = None;
            self.loading = false;
        }
        while let Ok(reply) = self.replies.try_recv() {
            match reply {
                Reply::Terrain { seed: s, center, bpp, image } if s == seed => {
                    let texture = ctx.load_texture("carte", image, egui::TextureOptions::LINEAR);
                    self.texture = Some(MapTexture { texture, center, bpp });
                    if self.requested.is_some_and(|(c, b)| c == center && b == bpp) {
                        self.loading = false;
                    }
                }
                Reply::Places { seed: s, places } if s == seed => self.places.extend(places),
                Reply::Roads { seed: s, roads } if s == seed => self.roads.extend(roads),
                _ => {}
            }
        }
        // Environs de la boussole
        if let Some(nav) = self.navigation {
            let moved = self.nearby_from.map_or(f64::INFINITY, |from| from.angle_between(nav.up) * planet_radius_blocks());
            if moved > NEARBY_REFRESH {
                self.nearby_from = Some(nav.up);
                let _ = self.jobs.send(Job::Nearby { seed, generator: Box::new(generator.clone()), center: nav.up });
            }
        }
        // Relief de la carte
        let wanted = (self.center, self.bpp());
        if map_open && self.requested != Some(wanted) && self.last_request.is_none_or(|t| t.elapsed() >= REQUEST_INTERVAL) {
            self.requested = Some(wanted);
            self.last_request = Some(Instant::now());
            self.loading = true;
            let _ = self.jobs.send(Job::Map { seed, generator: Box::new(generator.clone()), center: wanted.0, bpp: wanted.1 });
        }
    }

    // -----------------------------------------------------------------------
    // Écran de carte
    // -----------------------------------------------------------------------

    /// Dessine l'écran de carte : une feuille de parchemin entière, titre, carte et légende à l'encre
    pub fn draw_screen(&mut self, ctx: &egui::Context, close_hint: &str) {
        let screen = ctx.content_rect();
        let side = ((screen.height() - 24.0) / SHEET_H).min((screen.width() - 24.0) / SHEET_W).clamp(220.0, 760.0);
        if self.parchment.is_none() {
            let w = (SHEET_W * SHEET_TEXELS) as usize;
            let h = (SHEET_H * SHEET_TEXELS) as usize;
            let load = |name: &str, hole: bool| ctx.load_texture(name, sheet_image(w, h, hole), egui::TextureOptions::LINEAR);
            self.parchment = Some((load("carte_feuille", false), load("carte_feuille_ajouree", true)));
        }
        let ink = egui::Color32::from_rgb(74, 50, 32);
        let faded = egui::Color32::from_rgb(120, 92, 62);
        egui::Area::new(egui::Id::unique("map_screen"))
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let (sheet, _) = ui.allocate_exact_size(egui::vec2(SHEET_W * side, SHEET_H * side), egui::Sense::hover());
                let painter = ui.painter();
                let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                if let Some((paper, _)) = &self.parchment {
                    // Ombre portée, puis la feuille
                    painter.image(paper.id(), sheet.translate(egui::vec2(5.0, 8.0)), full_uv, egui::Color32::from_black_alpha(110));
                    painter.image(paper.id(), sheet, full_uv, egui::Color32::WHITE);
                }
                let u = side;
                let map_rect = egui::Rect::from_min_size(sheet.min + egui::vec2(SHEET_MARGIN * u, SHEET_HEADER * u), egui::vec2(u, u));
                // En-tête : titre calligraphié, fermeture discrète, filet orné
                let title_y = sheet.top() + SHEET_HEADER * u * 0.52;
                ink_text(painter, egui::pos2(sheet.center().x, title_y), egui::Align2::CENTER_CENTER, "Carte du monde", crate::ui_skin::title_font((u * 0.052).clamp(18.0, 30.0)), ink);
                let flourish_y = title_y + (u * 0.032).clamp(12.0, 20.0);
                paint_flourish(painter, egui::pos2(sheet.center().x, flourish_y), (u * 0.22).clamp(90.0, 170.0), ink);
                painter.text(
                    egui::pos2(map_rect.right(), sheet.top() + SHEET_HEADER * u * 0.34),
                    egui::Align2::RIGHT_CENTER,
                    format!("[{close_hint}] ou [Échap]"),
                    egui::FontId::proportional(11.0),
                    faded,
                );

                // Carte
                let response = ui.interact(map_rect, egui::Id::unique("map_canvas"), egui::Sense::click_and_drag());
                self.map_input(ui, map_rect, &response);
                self.paint_map(ui, map_rect, sheet, response.hover_pos());

                // Pied de page : échelle, position, recentrer, aide
                let row_y = map_rect.bottom() + SHEET_FOOTER * u * 0.30;
                self.scale_bar(painter, egui::pos2(map_rect.left() + 4.0, row_y), u, ink);
                let recenter = painter.layout_no_wrap("⌖ Recentrer".to_string(), crate::ui_skin::strong_font(13.0), ink);
                let r = egui::Rect::from_min_size(egui::pos2(map_rect.right() - recenter.size().x, row_y - recenter.size().y / 2.0), recenter.size()).expand(3.0);
                let button = ui.interact(r, egui::Id::unique("map_recenter"), egui::Sense::click());
                let color = if button.hovered() { egui::Color32::from_rgb(150, 44, 32) } else { ink };
                ink_text(painter, r.center(), egui::Align2::CENTER_CENTER, "⌖ Recentrer", crate::ui_skin::strong_font(13.0), color);
                if button.hovered() {
                    painter.line_segment([r.left_bottom() + egui::vec2(3.0, -2.0), r.right_bottom() - egui::vec2(3.0, 2.0)], egui::Stroke::new(1.0_f32, color));
                }
                if button.clicked() {
                    self.recenter();
                }
                if let Some(nav) = self.navigation {
                    let (lat, lon) = lat_lon(nav.up);
                    painter.text(
                        egui::pos2(r.left() - 16.0, row_y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{lat}   {lon}"),
                        egui::FontId::proportional(12.5),
                        ink,
                    );
                }
                painter.text(
                    egui::pos2(sheet.center().x, map_rect.bottom() + SHEET_FOOTER * u * 0.68),
                    egui::Align2::CENTER_CENTER,
                    "Molette : zoom  ·  Glisser : déplacer  ·  Clic droit : poser ou retirer un repère",
                    egui::FontId::proportional(11.0),
                    faded,
                );
            });
    }

    /// Pixels d'écran par bloc, pour une carte de côté `side`
    fn screen_scale(&self, side: f32) -> f64 {
        side as f64 / (MAP_PIXELS as f64 * self.bpp())
    }

    /// Point d'écran d'une direction, sur la carte `rect`
    fn to_screen(&self, frame: &Frame, rect: egui::Rect, dir: DVec3) -> egui::Pos2 {
        let p = frame.project(dir) * self.screen_scale(rect.width());
        rect.center() + egui::vec2(p.x as f32, -p.y as f32)
    }

    /// Direction sous un point d'écran de la carte `rect`
    fn dir_at_screen(&self, frame: &Frame, rect: egui::Rect, pos: egui::Pos2) -> DVec3 {
        let d = pos - rect.center();
        let scale = self.screen_scale(rect.width());
        frame.unproject(DVec2::new(d.x as f64, -d.y as f64) / scale)
    }

    fn map_input(&mut self, ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
        let frame = Frame::at(self.center);
        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            if d != egui::Vec2::ZERO {
                let scale = self.screen_scale(rect.width());
                self.center = frame.unproject(DVec2::new(-d.x as f64, d.y as f64) / scale);
            }
        }
        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let near_waypoint = self.waypoint.is_some_and(|w| self.to_screen(&frame, rect, w).distance(pos) < 12.0);
                self.waypoint = if near_waypoint { None } else { Some(self.dir_at_screen(&frame, rect, pos)) };
            }
        }
        if response.hovered() {
            self.scroll += ui.input(|i| crate::theme::raw_scroll_delta(i).y);
            let (plus, minus) = ui.input(|i| {
                (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals), i.key_pressed(egui::Key::Minus))
            });
            let mut step = 0_i32;
            if self.scroll >= 40.0 || plus {
                step = -1;
            } else if self.scroll <= -40.0 || minus {
                step = 1;
            }
            if step != 0 {
                self.scroll = 0.0;
                let zoom = (self.zoom as i32 + step).clamp(0, ZOOM_LEVELS.len() as i32 - 1) as usize;
                if zoom != self.zoom {
                    // Le point sous le pointeur reste sous le pointeur
                    if let Some(pos) = response.hover_pos() {
                        let anchor = self.dir_at_screen(&frame, rect, pos);
                        self.zoom = zoom;
                        let d = pos - rect.center();
                        let offset = DVec2::new(d.x as f64, -d.y as f64) / self.screen_scale(rect.width());
                        // Nouveau centre : à `offset` du point visé, dans l'autre sens
                        self.center = Frame::at(anchor).unproject(-offset);
                    } else {
                        self.zoom = zoom;
                    }
                }
            }
        } else {
            self.scroll = 0.0;
        }
    }

    fn paint_map(&self, ui: &egui::Ui, rect: egui::Rect, sheet: egui::Rect, hover: Option<egui::Pos2>) {
        let painter = ui.painter_at(rect);
        let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let frame = Frame::at(self.center);
        let scale = self.screen_scale(rect.width());
        // Relief : l'image reçue, placée et mise à l'échelle sur la vue en cours (pendant un glissé
        // ou un zoom, en attendant la suivante). Un peu transparente : le grain du papier transparaît.
        if let Some(tex) = &self.texture {
            let c = self.to_screen(&frame, rect, tex.center);
            let half = (MAP_PIXELS as f64 * tex.bpp * scale / 2.0) as f32;
            let r = egui::Rect::from_center_size(c, egui::vec2(half * 2.0, half * 2.0));
            painter.image(tex.texture.id(), r, full_uv, egui::Color32::from_white_alpha(212));
        }
        // Vue d'au plus un demi-tour de planète : au-delà, la projection replie le monde sur lui-même
        let view_radius = (rect.width() as f64 / scale) * 0.75;
        let visible = |dir: DVec3| frame.up.angle_between(dir) * planet_radius_blocks() < view_radius;
        let ink = egui::Color32::from_rgb(74, 50, 32);

        // Chemins : pointillés d'encre
        let road_width = (0.8 + 1.5 * scale as f32).clamp(1.3, 3.5);
        for points in self.roads.values() {
            if !points.iter().step_by(8).any(|&d| visible(d)) {
                continue;
            }
            let line: Vec<egui::Pos2> = points.iter().map(|&d| self.to_screen(&frame, rect, d)).collect();
            let dash = road_width * 3.0;
            painter.extend(egui::Shape::dashed_line(&line, egui::Stroke::new(road_width, egui::Color32::from_rgb(120, 70, 38)), dash, dash * 0.6));
        }

        // Établissements, des ruines aux villes (les villes par-dessus)
        let mut places: Vec<&Place> = self.places.values().filter(|p| visible(p.dir)).collect();
        places.sort_by_key(|p| match p.kind {
            SettlementKind::Ruins => 0,
            SettlementKind::Village => 1,
            SettlementKind::Town => 2,
        });
        let bpp = self.bpp();
        let mut hovered: Option<&Place> = None;
        for place in &places {
            let pos = self.to_screen(&frame, rect, place.dir);
            if !rect.expand(20.0).contains(pos) || (place.kind == SettlementKind::Ruins && bpp > 16.0) {
                continue;
            }
            paint_place_icon(&painter, pos, place.kind, 1.0, true);
            if hover.is_some_and(|h| h.distance(pos) < 10.0) {
                hovered = Some(place);
            }
            let labelled = match place.kind {
                SettlementKind::Town => bpp <= 64.0,
                SettlementKind::Village => bpp <= 32.0,
                SettlementKind::Ruins => bpp <= 8.0,
            };
            if labelled {
                let font = match place.kind {
                    SettlementKind::Town => crate::ui_skin::title_font(15.0),
                    SettlementKind::Village => crate::ui_skin::strong_font(12.5),
                    SettlementKind::Ruins => egui::FontId::proportional(11.5),
                };
                ink_text(&painter, pos + egui::vec2(0.0, 9.0), egui::Align2::CENTER_TOP, &place.name, font, ink);
            }
        }

        // La feuille ajourée par-dessus : la carte se fond dans le parchemin sur ses bords
        if let Some((_, holed)) = &self.parchment {
            painter.image(holed.id(), sheet, full_uv, egui::Color32::WHITE);
        }

        // Repère
        if let Some(w) = self.waypoint {
            let pos = self.to_screen(&frame, rect, w);
            paint_waypoint(&painter, pos, 7.0, true);
        }

        // Joueur : flèche dans le sens du regard
        if let Some(nav) = self.navigation {
            let pos = self.to_screen(&frame, rect, nav.up);
            if rect.shrink(6.0).contains(pos) {
                paint_player_arrow(&painter, pos, nav.yaw as f32, 11.0);
            } else {
                // Hors de la carte : une flèche au bord indique où il est
                let d = (pos - rect.center()).normalized();
                let edge = rect.center() + d * (rect.width() / 2.0 - 16.0);
                let angle = d.x.atan2(-d.y);
                paint_player_arrow(&painter, edge, angle, 9.0);
            }
        }

        // Rose des vents, à l'encre
        paint_compass_rose(&painter, rect.right_top() + egui::vec2(-42.0, 46.0), 22.0, ink);

        // Chargement
        if self.loading {
            ink_text(&painter, rect.left_bottom() + egui::vec2(24.0, -22.0), egui::Align2::LEFT_BOTTOM, "Relevé du terrain…", egui::FontId::proportional(12.5), ink);
        }

        // Fiche du lieu survolé, ou distance au point survolé
        if let Some(h) = hover {
            let target = hovered.map(|p| p.dir).unwrap_or_else(|| self.dir_at_screen(&frame, rect, h));
            let distance = self.navigation.map(|nav| Frame::at(nav.up).bearing_distance(target));
            let mut text = match hovered {
                Some(p) => format!("{} ({})", p.name, kind_label(p.kind)),
                None => String::new(),
            };
            if let Some((bearing, dist)) = distance {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!("{} · {}", format_distance(dist), cardinal(bearing)));
            }
            if !text.is_empty() {
                let ink = egui::Color32::from_rgb(74, 50, 32);
                let galley = painter.layout(text, egui::FontId::proportional(12.5), ink, 260.0);
                let mut r = egui::Rect::from_min_size(h + egui::vec2(14.0, 12.0), galley.size()).expand(5.0);
                if r.right() > rect.right() {
                    r = r.translate(egui::vec2(-r.width() - 28.0, 0.0));
                }
                if r.bottom() > rect.bottom() {
                    r = r.translate(egui::vec2(0.0, -r.height() - 24.0));
                }
                painter.rect_filled(r.translate(egui::vec2(2.0, 3.0)), 2.0, egui::Color32::from_black_alpha(60));
                painter.rect_filled(r, 2.0, egui::Color32::from_rgb(243, 232, 204));
                painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, ink), egui::StrokeKind::Middle);
                painter.galley(r.min + egui::vec2(5.0, 5.0), galley, ink);
            }
        }
    }

    /// Barre d'échelle à l'encre : une longueur ronde de blocs, en cases alternées
    fn scale_bar(&self, painter: &egui::Painter, left: egui::Pos2, side: f32, ink: egui::Color32) {
        let px_per_block = self.screen_scale(side) as f32;
        let target = 110.0 / px_per_block;
        let nice = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0]
            .into_iter()
            .rev()
            .find(|&n| n <= target)
            .unwrap_or(10.0);
        let width = nice * px_per_block;
        let stroke = egui::Stroke::new(1.0_f32, ink);
        let bar = egui::Rect::from_min_size(left - egui::vec2(0.0, 2.5), egui::vec2(width, 5.0));
        for k in 0..4 {
            let cell = egui::Rect::from_min_size(bar.min + egui::vec2(width * k as f32 / 4.0, 0.0), egui::vec2(width / 4.0, 5.0));
            painter.rect_filled(cell, 0.0, if k % 2 == 0 { ink } else { egui::Color32::from_rgb(238, 226, 196) });
        }
        painter.rect_stroke(bar, 0.0, stroke, egui::StrokeKind::Middle);
        painter.text(bar.right_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, format_distance(nice as f64), egui::FontId::proportional(12.5), ink);
    }

    // -----------------------------------------------------------------------
    // Boussole
    // -----------------------------------------------------------------------

    /// Boussole en haut de l'écran : cap du regard, établissements des environs et repère
    pub fn draw_compass(&self, ctx: &egui::Context, top: f32) {
        let Some(nav) = self.navigation else { return };
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.4).clamp(260.0, 440.0);
        let rect = egui::Rect::from_center_size(egui::pos2(screen.center().x, top + 14.0), egui::vec2(width, 28.0));
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("compass")));
        painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(140));
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0_f32, GOLD.gamma_multiply(0.55)), egui::StrokeKind::Middle);
        let clip = painter.with_clip_rect(rect.shrink(1.0));
        // Demi-champ de la boussole : 90° de part et d'autre du regard
        let half_span = PI / 2.0;
        let x_of = |bearing: f64| -> Option<f32> {
            let rel = (bearing - nav.yaw + PI).rem_euclid(TAU) - PI;
            (rel.abs() <= half_span).then(|| rect.center().x + (rel / half_span) as f32 * width / 2.0)
        };
        const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SO", "O", "NO"];
        for step in 0..24 {
            let bearing = step as f64 * TAU / 24.0;
            let Some(x) = x_of(bearing) else { continue };
            if step % 3 == 0 {
                let label = POINTS[step / 3];
                let color = if label == "N" { egui::Color32::from_rgb(232, 86, 66) } else if label.len() == 1 { TEXT_SOFT } else { TEXT_DIM };
                let size = if label.len() == 1 { 14.0 } else { 11.5 };
                clip.text(egui::pos2(x, rect.center().y + 1.0), egui::Align2::CENTER_CENTER, label, crate::ui_skin::strong_font(size), color);
            } else {
                clip.line_segment([egui::pos2(x, rect.bottom() - 7.0), egui::pos2(x, rect.bottom() - 2.0)], egui::Stroke::new(1.0_f32, TEXT_DIM));
            }
        }
        // Lieux des environs, puis le repère, au-dessus
        let frame = Frame::at(nav.up);
        let mut aimed: Option<(f64, String, f64)> = None;
        let mut consider = |rel: f64, label: String, dist: f64| {
            if rel.abs() < 0.12 && aimed.as_ref().is_none_or(|(r, _, _)| rel.abs() < r.abs()) {
                aimed = Some((rel, label, dist));
            }
        };
        for place in self.places.values() {
            let (bearing, dist) = frame.bearing_distance(place.dir);
            if !(12.0..=COMPASS_RANGE).contains(&dist) {
                continue;
            }
            if let Some(x) = x_of(bearing) {
                paint_place_icon(&clip, egui::pos2(x, rect.top() + 8.0), place.kind, 0.7, false);
                consider((bearing - nav.yaw + PI).rem_euclid(TAU) - PI, place.name.clone(), dist);
            }
        }
        if let Some(w) = self.waypoint {
            let (bearing, dist) = frame.bearing_distance(w);
            match x_of(bearing) {
                Some(x) => {
                    paint_waypoint(&clip, egui::pos2(x, rect.top() + 8.0), 4.5, false);
                    consider((bearing - nav.yaw + PI).rem_euclid(TAU) - PI, "Repère".to_string(), dist);
                }
                None => {
                    // Derrière le joueur : une pointe au bord du côté où tourner
                    let rel = (bearing - nav.yaw + PI).rem_euclid(TAU) - PI;
                    let x = if rel > 0.0 { rect.right() - 7.0 } else { rect.left() + 7.0 };
                    paint_waypoint(&clip, egui::pos2(x, rect.center().y), 4.0, false);
                }
            }
        }
        // Repère du regard
        let c = rect.center().x;
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(c, rect.bottom() - 5.0), egui::pos2(c + 5.0, rect.bottom() + 2.0), egui::pos2(c - 5.0, rect.bottom() + 2.0)],
            GOLD_BRIGHT,
            egui::Stroke::NONE,
        ));
        let below = egui::pos2(c, rect.bottom() + 5.0);
        let heading = format!("{}°", (nav.yaw.to_degrees().rem_euclid(360.0)).round() as i32 % 360);
        let text = match aimed {
            Some((_, label, dist)) => format!("{heading} · {label} · {}", format_distance(dist)),
            None => heading,
        };
        shadowed_text(&painter, below, egui::Align2::CENTER_TOP, &text, 12.0, TEXT_SOFT);
    }
}

fn kind_label(kind: SettlementKind) -> &'static str {
    match kind {
        SettlementKind::Town => "ville",
        SettlementKind::Village => "village",
        SettlementKind::Ruins => "ruines",
    }
}

/// Distance lisible : en mètres (un bloc fait un mètre), en kilomètres au-delà de 1 000
pub fn format_distance(blocks: f64) -> String {
    if blocks >= 1000.0 {
        format!("{:.1} km", blocks / 1000.0).replace('.', ",")
    } else {
        format!("{:.0} m", blocks)
    }
}

/// Point cardinal le plus proche d'un cap
pub fn cardinal(bearing: f64) -> &'static str {
    const POINTS: [&str; 8] = ["nord", "nord-est", "est", "sud-est", "sud", "sud-ouest", "ouest", "nord-ouest"];
    POINTS[((bearing.rem_euclid(TAU) / (TAU / 8.0)).round() as usize) % 8]
}

/// Latitude et longitude lisibles d'une direction
fn lat_lon(dir: DVec3) -> (String, String) {
    let (lat, lon) = herocraft_world::unit_sphere_to_lat_lon(dir.normalize());
    let (lat, lon) = (lat.to_degrees(), lon.to_degrees());
    (
        format!("{:.1}° {}", lat.abs(), if lat >= 0.0 { "N" } else { "S" }).replace('.', ","),
        format!("{:.1}° {}", lon.abs(), if lon >= 0.0 { "E" } else { "O" }).replace('.', ","),
    )
}

fn shadowed_text(painter: &egui::Painter, pos: egui::Pos2, align: egui::Align2, text: &str, size: f32, color: egui::Color32) {
    let font = crate::ui_skin::strong_font(size);
    painter.text(pos + egui::vec2(1.0, 1.0), align, text, font.clone(), egui::Color32::from_black_alpha(220));
    painter.text(pos, align, text, font, color);
}

/// Encre des icônes
const ICON_INK: egui::Color32 = egui::Color32::from_rgb(58, 38, 22);

/// Contour fermé à l'encre
fn ink_outline(painter: &egui::Painter, points: &[egui::Pos2], width: f32, color: egui::Color32) {
    let mut closed = points.to_vec();
    closed.push(points[0]);
    painter.add(egui::Shape::line(closed, egui::Stroke::new(width, color)));
}

/// Lavis d'une forme convexe, puis son contour à l'encre
fn washed(painter: &egui::Painter, points: Vec<egui::Pos2>, fill: egui::Color32, width: f32) {
    painter.add(egui::Shape::convex_polygon(points.clone(), fill, egui::Stroke::NONE));
    ink_outline(painter, &points, width, ICON_INK);
}

/// Icône d'un établissement, à la plume comme sur une carte ancienne : château à deux tours pour
/// une ville, deux maisons pour un village, arche brisée pour des ruines. `halo` : un voile de
/// papier clair dessous, pour la lire sur le relief.
fn paint_place_icon(painter: &egui::Painter, pos: egui::Pos2, kind: SettlementKind, scale: f32, halo: bool) {
    let s = scale;
    let p = |x: f32, y: f32| pos + egui::vec2(x * s, y * s);
    let w = (1.1 * s).max(0.8);
    if halo {
        painter.circle_filled(pos + egui::vec2(0.0, 0.5 * s), 9.5 * s, egui::Color32::from_rgba_unmultiplied(243, 232, 204, 55));
    }
    match kind {
        SettlementKind::Town => {
            let wall = egui::Color32::from_rgb(214, 186, 140);
            let roof = egui::Color32::from_rgb(170, 62, 44);
            // Courtine entre deux tours, porte en arc
            washed(painter, vec![p(-5.0, -1.0), p(5.0, -1.0), p(5.0, 7.0), p(-5.0, 7.0)], wall, w);
            for x in [-3.0, 0.0, 3.0] {
                washed(painter, vec![p(x - 0.9, -2.8), p(x + 0.9, -2.8), p(x + 0.9, -1.0), p(x - 0.9, -1.0)], wall, w * 0.8);
            }
            for x in [-7.5, 7.5] {
                washed(painter, vec![p(x - 2.6, -3.0), p(x + 2.6, -3.0), p(x + 2.6, 7.0), p(x - 2.6, 7.0)], wall, w);
                washed(painter, vec![p(x - 3.3, -3.0), p(x, -9.5), p(x + 3.3, -3.0)], roof, w);
                painter.line_segment([p(x, -9.5), p(x, -11.5)], egui::Stroke::new(w * 0.8, ICON_INK));
                painter.add(egui::Shape::convex_polygon(vec![p(x, -11.5), p(x + 2.4, -10.9), p(x, -10.3)], roof, egui::Stroke::NONE));
            }
            let gate: Vec<egui::Pos2> = (0..=8)
                .map(|k| {
                    let a = std::f32::consts::PI * k as f32 / 8.0;
                    p(-1.8 * a.cos(), 3.2 - 1.8 * a.sin())
                })
                .chain([p(1.8, 7.0), p(-1.8, 7.0)])
                .collect();
            painter.add(egui::Shape::convex_polygon(gate.clone(), ICON_INK, egui::Stroke::NONE));
        }
        SettlementKind::Village => {
            let wall = egui::Color32::from_rgb(226, 200, 150);
            let roof = egui::Color32::from_rgb(186, 96, 54);
            // Maison du fond, plus petite
            washed(painter, vec![p(0.5, -1.5), p(7.5, -1.5), p(7.5, 4.0), p(0.5, 4.0)], wall, w * 0.9);
            washed(painter, vec![p(-0.5, -1.5), p(4.0, -6.0), p(8.5, -1.5)], roof, w * 0.9);
            // Maison de devant, avec sa porte
            washed(painter, vec![p(-7.0, 0.0), p(1.5, 0.0), p(1.5, 6.5), p(-7.0, 6.5)], wall, w);
            washed(painter, vec![p(-8.3, 0.0), p(-2.75, -6.0), p(2.8, 0.0)], roof, w);
            painter.rect_filled(egui::Rect::from_min_max(p(-3.8, 3.0), p(-1.8, 6.5)), 0.0, ICON_INK);
            painter.line_segment([p(5.5, -4.0), p(5.5, -6.5)], egui::Stroke::new(w * 1.4, ICON_INK));
        }
        SettlementKind::Ruins => {
            let stone = egui::Color32::from_rgb(196, 186, 168);
            // Une arche dont il ne reste qu'un pilier entier, et un tronçon
            washed(painter, vec![p(-6.0, -5.0), p(-2.5, -5.0), p(-2.5, 6.0), p(-6.0, 6.0)], stone, w);
            washed(painter, vec![p(-6.5, -6.6), p(-2.0, -6.6), p(-2.0, -5.0), p(-6.5, -5.0)], stone, w);
            washed(painter, vec![p(2.5, 1.0), p(6.0, 0.0), p(6.0, 6.0), p(2.5, 6.0)], stone, w);
            let arch: Vec<egui::Pos2> = (0..=5)
                .map(|k| {
                    let a = std::f32::consts::PI * (1.0 - k as f32 / 10.0);
                    p(2.0 + 4.3 * a.cos(), -2.0 - 3.2 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(arch, egui::Stroke::new(w * 1.6, ICON_INK)));
            // Moellons tombés
            painter.circle_filled(p(0.2, 5.6), 0.9 * s, ICON_INK);
            painter.circle_filled(p(8.0, 5.4), 0.8 * s, ICON_INK);
        }
    }
}

/// Repère : une croix rouge à la plume, comme sur une carte au trésor. `halo` : sur la carte.
fn paint_waypoint(painter: &egui::Painter, pos: egui::Pos2, r: f32, halo: bool) {
    let red = egui::Color32::from_rgb(178, 40, 30);
    if halo {
        painter.circle_filled(pos, r * 1.5, egui::Color32::from_rgba_unmultiplied(243, 232, 204, 140));
        painter.circle_stroke(pos, r * 1.35, egui::Stroke::new(0.8_f32, red.gamma_multiply(0.7)));
    }
    let width = (r * 0.42).max(1.6);
    painter.line_segment([pos + egui::vec2(-r, -r), pos + egui::vec2(r, r)], egui::Stroke::new(width, red));
    painter.line_segment([pos + egui::vec2(r * 0.9, -r * 1.05), pos + egui::vec2(-r * 0.95, r * 0.95)], egui::Stroke::new(width, red));
}

/// Joueur : une aiguille de boussole, moitié rouge moitié papier, pointée selon `angle`
/// (0 : vers le haut de la carte, sens horaire)
fn paint_player_arrow(painter: &egui::Painter, pos: egui::Pos2, angle: f32, r: f32) {
    let rot = |x: f32, y: f32| {
        let (s, c) = angle.sin_cos();
        pos + egui::vec2(x * c - y * s, x * s + y * c)
    };
    let tip = rot(0.0, -r * 1.25);
    let tail = rot(0.0, r * 0.8);
    let left = rot(-r * 0.48, r * 0.05);
    let right = rot(r * 0.48, r * 0.05);
    let red = egui::Color32::from_rgb(182, 44, 32);
    let pale = egui::Color32::from_rgb(240, 226, 192);
    painter.add(egui::Shape::convex_polygon(vec![tip, right, pos], red, egui::Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![tip, pos, left], egui::Color32::from_rgb(140, 30, 22), egui::Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![left, pos, tail], pale, egui::Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![pos, right, tail], egui::Color32::from_rgb(214, 196, 158), egui::Stroke::NONE));
    ink_outline(painter, &[tip, right, tail, left], 1.2, ICON_INK);
    painter.circle_filled(pos, r * 0.16, ICON_INK);
}

/// Filet orné sous le titre : un trait qui s'affine, un losange au centre
fn paint_flourish(painter: &egui::Painter, c: egui::Pos2, half: f32, ink: egui::Color32) {
    for side in [-1.0_f32, 1.0] {
        let points: Vec<egui::Pos2> = (0..=12)
            .map(|k| {
                let t = k as f32 / 12.0;
                c + egui::vec2(side * (8.0 + t * half), (t * std::f32::consts::TAU).sin() * 1.6 * (1.0 - t))
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.2_f32, ink)));
        painter.circle_filled(c + egui::vec2(side * (half + 10.0), 0.0), 1.4, ink);
    }
    let d = 4.0;
    painter.add(egui::Shape::convex_polygon(vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)], ink, egui::Stroke::NONE));
}

/// Feuille de l'écran de carte, en côtés de carte : marges, en-tête et pied de page autour d'elle
const SHEET_MARGIN: f32 = 0.06;
const SHEET_HEADER: f32 = 0.12;
const SHEET_FOOTER: f32 = 0.11;
const SHEET_W: f32 = 1.0 + 2.0 * SHEET_MARGIN;
const SHEET_H: f32 = SHEET_HEADER + 1.0 + SHEET_FOOTER;
/// Texels de l'image de la feuille par côté de carte
const SHEET_TEXELS: f32 = 420.0;

fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Bruit de valeur lissé
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let a = lattice(ix, iy, seed) + (lattice(ix + 1, iy, seed) - lattice(ix, iy, seed)) * sx;
    let b = lattice(ix, iy + 1, seed) + (lattice(ix + 1, iy + 1, seed) - lattice(ix, iy + 1, seed)) * sx;
    a + (b - a) * sy
}

fn fbm2(x: f32, y: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for octave in 0..5 {
        sum += value_noise(x * freq, y * freq, seed + octave) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Couleur du parchemin au point (x, y) de la feuille (en côtés de carte) : papier crème,
/// grain, taches, bords roussis
fn parchment_color(x: f32, y: f32) -> [f32; 3] {
    let grain = fbm2(x * 40.0, y * 40.0, 11);
    let cloud = fbm2(x * 4.0, y * 4.0, 23);
    let stain = fbm2(x * 2.5 + 3.0, y * 2.5 + 7.0, 37);
    let mut c = mix([0.86, 0.77, 0.58], [0.95, 0.89, 0.73], cloud);
    c = c.map(|v| v * (0.94 + 0.08 * grain));
    c = mix(c, [0.74, 0.58, 0.38], smoothstep(0.58, 0.78, stain) * 0.35);
    let d = sheet_edge_distance(x, y);
    let wobble = (fbm2(x * 9.0, y * 9.0, 51) - 0.5) * 0.04;
    let burn = 1.0 - smoothstep(0.0, 0.05 + wobble, d);
    mix(c, [0.52, 0.36, 0.20], burn * 0.6)
}

/// Distance d'un point de la feuille à son bord, bord rongé par le bruit
fn sheet_edge_distance(x: f32, y: f32) -> f32 {
    let d = x.min(SHEET_W - x).min(y).min(SHEET_H - y);
    d + (fbm2(x * 14.0, y * 14.0, 63) - 0.5) * 0.022
}

/// Image de la feuille. `hole` : la même, ajourée à l'emplacement de la carte avec un fondu
/// irrégulier, à poser par-dessus la carte pour qu'elle se perde dans le papier.
fn sheet_image(w: usize, h: usize, hole: bool) -> egui::ColorImage {
    let rows: Vec<Vec<egui::Color32>> = (0..h)
        .into_par_iter()
        .map(|py| {
            (0..w)
                .map(|px| {
                    let (x, y) = ((px as f32 + 0.5) / w as f32 * SHEET_W, (py as f32 + 0.5) / h as f32 * SHEET_H);
                    let c = parchment_color(x, y).map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
                    // Bord de la feuille, déchiré
                    let mut alpha = smoothstep(0.0, 0.008, sheet_edge_distance(x, y));
                    if hole {
                        let (mx, my) = (x - SHEET_MARGIN, y - SHEET_HEADER);
                        let inside = mx.min(1.0 - mx).min(my).min(1.0 - my);
                        if inside > 0.0 {
                            let wobble = (fbm2(mx * 6.0, my * 6.0, 77) - 0.5) * 0.07;
                            alpha *= 1.0 - smoothstep(0.012 + wobble * 0.4, 0.085 + wobble, inside);
                        }
                    }
                    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (alpha * 255.0) as u8)
                })
                .collect()
        })
        .collect();
    egui::ColorImage::new([w, h], rows.into_iter().flatten().collect())
}

/// Texte à l'encre, détouré de papier clair pour rester lisible sur le relief
fn ink_text(painter: &egui::Painter, pos: egui::Pos2, align: egui::Align2, text: &str, font: egui::FontId, color: egui::Color32) {
    let halo = egui::Color32::from_rgba_unmultiplied(244, 232, 200, 150);
    for d in [egui::vec2(-1.0, 0.0), egui::vec2(1.0, 0.0), egui::vec2(0.0, -1.0), egui::vec2(0.0, 1.0)] {
        painter.text(pos + d, align, text, font.clone(), halo);
    }
    painter.text(pos, align, text, font, color);
}

/// Rose des vents à l'encre : étoile à huit branches, le nord en rouge
fn paint_compass_rose(painter: &egui::Painter, c: egui::Pos2, r: f32, ink: egui::Color32) {
    let stroke = egui::Stroke::new(1.0_f32, ink);
    painter.circle_stroke(c, r * 0.62, stroke);
    painter.circle_stroke(c, r * 0.56, egui::Stroke::new(0.6_f32, ink));
    let point = |angle: f32, len: f32| c + egui::vec2(angle.sin(), -angle.cos()) * len;
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let len = if k % 2 == 0 { r } else { r * 0.55 };
        let w = if k % 2 == 0 { 0.16 } else { 0.22 };
        let tip = point(a, len);
        let left = point(a - std::f32::consts::FRAC_PI_2, r * w);
        let right = point(a + std::f32::consts::FRAC_PI_2, r * w);
        let dark = if k == 0 { egui::Color32::from_rgb(170, 52, 38) } else { ink };
        let light = egui::Color32::from_rgb(238, 226, 196);
        // Chaque branche : une moitié pleine, une moitié claire, comme gravée
        painter.add(egui::Shape::convex_polygon(vec![c, left, tip], dark, egui::Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(vec![c, tip, right], light, stroke));
    }
    painter.text(point(0.0, r + 2.0), egui::Align2::CENTER_BOTTOM, "N", crate::ui_skin::title_font(13.0), egui::Color32::from_rgb(150, 44, 32));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_aller_retour() {
        let frame = Frame::at(DVec3::new(0.3, 0.5, -0.8));
        for offset in [DVec2::new(0.0, 0.0), DVec2::new(120.0, -40.0), DVec2::new(-3000.0, 2500.0)] {
            let back = frame.project(frame.unproject(offset));
            assert!((back - offset).length() < 1e-6, "{offset:?} → {back:?}");
        }
    }

    #[test]
    fn le_nord_est_vers_le_pole() {
        let frame = Frame::at(DVec3::new(1.0, 0.2, 0.1));
        let (bearing, _) = frame.bearing_distance(frame.unproject(DVec2::new(0.0, 100.0)));
        assert!(bearing.abs() < 1e-9 || (bearing - TAU).abs() < 1e-9);
        // Vers le pôle +Y : cap nord
        let toward_pole = (frame.up + DVec3::Y * 0.01).normalize();
        let (bearing, _) = frame.bearing_distance(toward_pole);
        assert!(bearing < 1e-6 || TAU - bearing < 1e-6, "cap {bearing}");
        // À l'est : 90°
        let (bearing, dist) = frame.bearing_distance(frame.unproject(DVec2::new(250.0, 0.0)));
        assert!((bearing - PI / 2.0).abs() < 1e-9);
        assert!((dist - 250.0).abs() < 1e-6);
        assert_eq!(cardinal(bearing), "est");
    }

    #[test]
    fn la_distance_suit_les_blocs() {
        // Deux colonnes voisines au centre d'une face sont à un bloc l'une de l'autre
        let half = FACE_BLOCKS as f64 / 2.0;
        let a = dir_of(PlanetFace::PosX, half, half);
        let b = dir_of(PlanetFace::PosX, half + 100.0, half);
        let (_, dist) = Frame::at(a).bearing_distance(b);
        assert!((dist - 100.0).abs() < 0.5, "{dist}");
        assert_eq!(column_of(a), (PlanetFace::PosX, half as i32, half as i32));
    }

    #[test]
    fn noms_stables() {
        let a = place_name(42, PlanetFace::PosY, (805, 1370), SettlementKind::Town);
        assert_eq!(a, place_name(42, PlanetFace::PosY, (805, 1370), SettlementKind::Town));
        assert!(place_name(42, PlanetFace::PosY, (805, 1370), SettlementKind::Ruins).starts_with("Ruines de "));
        let names: HashSet<String> = (0..200).map(|i| place_name(7, PlanetFace::NegZ, (i * 37, i * 11), SettlementKind::Village)).collect();
        assert!(names.len() > 120, "noms trop souvent répétés : {}", names.len());
    }

    #[test]
    fn distances_lisibles() {
        assert_eq!(format_distance(640.4), "640 m");
        assert_eq!(format_distance(1840.0), "1,8 km");
    }

    #[test]
    fn la_carte_trouve_les_villes_et_les_chemins() {
        // Graine 42, face 0 : un chemin part d'un village en (812, 1355)
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 42, ..Default::default() });
        let center = dir_of(PlanetFace::ALL[0], 812.5, 1355.5);
        let places = nearby_places(&generator, 42, center);
        assert!(places.iter().any(|(_, p)| p.kind != SettlementKind::Ruins && p.dir.angle_between(center) * planet_radius_blocks() < 2.0));
        let terrain = render_terrain(&generator, center, 8.0);
        assert_eq!(terrain.image.pixels.len(), MAP_PIXELS * MAP_PIXELS);
        let roads: Vec<_> = terrain.regions.iter().take(1).flat_map(|&r| roads_in_region(&generator, r)).collect();
        assert!(!roads.is_empty(), "aucun chemin autour de l'établissement");
    }
}

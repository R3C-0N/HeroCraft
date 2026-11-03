//! Chemins de terre et de gravier entre les villes et les villages.
//!
//! Chaque région de structures (voir `structures`) accueille au plus une ville ou un village. Deux
//! régions voisines par un côté sont reliées par un chemin quand toutes deux ont le leur ; deux
//! régions voisines par un coin le sont aussi quand les deux autres régions du carré n'en ont pas
//! (sans cela, un établissement isolé derrière une région vide resterait sans route).
//!
//! Le tracé part du bout d'une rue de village ou d'une porte de ville, celle qui regarde le mieux
//! vers l'autre établissement, puis cherche (A* sur une grille de `STEP` blocs) le chemin le moins
//! pénible : il ne passe jamais sous la mer ni dans les neiges éternelles, refuse les pentes raides,
//! préfère faire un détour que gravir une colline, et contourne les autres villes, villages et
//! ruines. Le tracé brut est arrondi (Chaikin), puis rastérisé : un cœur de gravier de trois blocs,
//! des bords mêlés de terre.
//!
//! Le chemin reste dans les régions qu'il relie : une colonne n'a donc à interroger que les quelques
//! liaisons qui touchent sa région. Comme les plans, un chemin est calculé à la première lecture et ne
//! dépend que de la graine : le client et le serveur, chunk après chunk, posent le même.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::OnceLock;

use herocraft_common::{BlockId, PlanetFace};

use crate::edits::FACE_BLOCKS;
use crate::generator::{BiomeType, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};
use crate::structures::{block::GRAVEL, face_index, hash2, mix, unit, Rect, Settlement, REGIONS_PER_FACE, REGION_BLOCKS};

/// Pas de la grille de recherche du tracé, en blocs
const STEP: i32 = 16;
/// Nœuds que la recherche peut développer avant d'abandonner (le détour serait trop long)
const MAX_EXPANSIONS: usize = 80_000;
/// Pente (dénivelé par bloc) au-delà de laquelle un chemin ne passe pas
const MAX_SLOPE: f64 = 0.8;
/// Demi-largeur du cœur de gravier, et du chemin bords compris (en blocs, depuis l'axe)
const CORE_HALF: f64 = 1.05;
const EDGE_HALF: f64 = 1.75;
/// Les chemins restent à tant de blocs des autres établissements
const SETTLEMENT_CLEARANCE: i32 = 3;

/// Liaison entre deux régions voisines, repérée par la région (rx, rz) du coin haut-gauche
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    /// (rx, rz) – (rx + 1, rz)
    East,
    /// (rx, rz) – (rx, rz + 1)
    South,
    /// (rx, rz) – (rx + 1, rz + 1)
    Down,
    /// (rx + 1, rz) – (rx, rz + 1)
    Up,
}

impl Link {
    const ALL: [Link; 4] = [Link::East, Link::South, Link::Down, Link::Up];

    /// Les deux régions reliées
    fn ends(self, rx: i32, rz: i32) -> [(i32, i32); 2] {
        match self {
            Link::East => [(rx, rz), (rx + 1, rz)],
            Link::South => [(rx, rz), (rx, rz + 1)],
            Link::Down => [(rx, rz), (rx + 1, rz + 1)],
            Link::Up => [(rx + 1, rz), (rx, rz + 1)],
        }
    }

    /// Pour une liaison en diagonale, les deux autres régions du carré
    fn others(self, rx: i32, rz: i32) -> Option<[(i32, i32); 2]> {
        match self {
            Link::East | Link::South => None,
            Link::Down => Some([(rx + 1, rz), (rx, rz + 1)]),
            Link::Up => Some([(rx, rz), (rx + 1, rz + 1)]),
        }
    }

    /// Colonnes où le chemin reste : les régions qu'il relie (le carré entier pour une diagonale)
    fn area(self, rx: i32, rz: i32) -> Rect {
        let (w, d) = match self {
            Link::East => (2, 1),
            Link::South => (1, 2),
            Link::Down | Link::Up => (2, 2),
        };
        let r = REGION_BLOCKS;
        Rect::new(rx * r, rz * r, ((rx + w) * r).min(FACE_BLOCKS), ((rz + d) * r).min(FACE_BLOCKS))
    }

    /// Liaisons (région du coin, sorte) dont la zone contient la région (rx, rz)
    fn around(rx: i32, rz: i32) -> [(i32, i32, Link); 12] {
        [
            (rx - 1, rz, Link::East),
            (rx, rz, Link::East),
            (rx, rz - 1, Link::South),
            (rx, rz, Link::South),
            (rx - 1, rz - 1, Link::Down),
            (rx, rz - 1, Link::Down),
            (rx - 1, rz, Link::Down),
            (rx, rz, Link::Down),
            (rx - 1, rz - 1, Link::Up),
            (rx, rz - 1, Link::Up),
            (rx - 1, rz, Link::Up),
            (rx, rz, Link::Up),
        ]
    }
}

/// Un chemin entre deux établissements
#[derive(Debug, Clone)]
pub struct Road {
    pub face: PlanetFace,
    /// Centres des deux établissements reliés
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// Axe du chemin, d'un bout de rue à l'autre
    pub points: Vec<(f64, f64)>,
    /// Toutes les colonnes du chemin
    pub bounds: Rect,
    /// Bloc du dessus du sol de chaque colonne du chemin
    columns: HashMap<(i32, i32), BlockId>,
}

impl Road {
    /// Bloc qui remplace le dessus du sol de la colonne (x, z), si le chemin y passe
    #[inline]
    pub fn surface(&self, x: i32, z: i32) -> Option<BlockId> {
        if !self.bounds.contains(x, z) {
            return None;
        }
        self.columns.get(&(x, z)).copied()
    }

    /// Le chemin passe à moins de `margin` colonnes de (x, z)
    pub fn near(&self, x: i32, z: i32, margin: i32) -> bool {
        if !self.bounds.expand(margin).contains(x, z) {
            return false;
        }
        (z - margin..=z + margin).any(|cz| (x - margin..=x + margin).any(|cx| self.columns.contains_key(&(cx, cz))))
    }

    /// Longueur de l'axe, en blocs
    pub fn length(&self) -> f64 {
        self.points.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum()
    }

    /// Nombre de colonnes du chemin
    pub fn columns(&self) -> usize {
        self.columns.len()
    }
}

/// Les chemins d'une planète, liaison par liaison, tracés à la première lecture
#[derive(Debug)]
pub struct RoadMap {
    links: Vec<OnceLock<Option<Road>>>,
}

impl Default for RoadMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RoadMap {
    pub fn new() -> Self {
        Self { links: (0..6 * REGIONS_PER_FACE * REGIONS_PER_FACE * Link::ALL.len() as i32).map(|_| OnceLock::new()).collect() }
    }

    fn link(&self, generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<&Road> {
        if !(0..REGIONS_PER_FACE).contains(&rx) || !(0..REGIONS_PER_FACE).contains(&rz) {
            return None;
        }
        let slot = Link::ALL.iter().position(|l| *l == link)? as i32;
        let index = ((face_index(face) * REGIONS_PER_FACE + rz) * REGIONS_PER_FACE + rx) * Link::ALL.len() as i32 + slot;
        self.links[index as usize].get_or_init(|| plan_road(generator, face, rx, rz, link)).as_ref()
    }

    /// Les chemins qui peuvent passer dans la région (rx, rz)
    fn in_region<'a>(&'a self, generator: &'a PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32) -> impl Iterator<Item = &'a Road> + 'a {
        Link::around(rx, rz).into_iter().filter_map(move |(lx, lz, link)| self.link(generator, face, lx, lz, link))
    }

    /// Bloc du dessus du sol de la colonne (x, z) d'une face, si un chemin y passe
    pub fn surface(&self, generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32) -> Option<BlockId> {
        if !(0..FACE_BLOCKS).contains(&x) || !(0..FACE_BLOCKS).contains(&z) {
            return None;
        }
        let (rx, rz) = (x.div_euclid(REGION_BLOCKS), z.div_euclid(REGION_BLOCKS));
        self.in_region(generator, face, rx, rz).find_map(|road| road.surface(x, z))
    }

    /// Les chemins qui touchent le rectangle de colonnes `area` d'une face
    pub fn touching<'a>(&'a self, generator: &'a PlanetWorldGenerator, face: PlanetFace, area: Rect) -> Vec<&'a Road> {
        let mut found: Vec<&'a Road> = Vec::new();
        let last = REGIONS_PER_FACE - 1;
        for rz in area.z0.div_euclid(REGION_BLOCKS).max(0)..=(area.z1 - 1).div_euclid(REGION_BLOCKS).min(last) {
            for rx in area.x0.div_euclid(REGION_BLOCKS).max(0)..=(area.x1 - 1).div_euclid(REGION_BLOCKS).min(last) {
                for road in self.in_region(generator, face, rx, rz) {
                    if road.bounds.intersects(&area) && !found.iter().any(|r| std::ptr::eq(*r, road)) {
                        found.push(road);
                    }
                }
            }
        }
        found
    }

    /// Un chemin passe à moins de `margin` colonnes de (x, z)
    pub fn near(&self, generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32, margin: i32) -> bool {
        if margin == 0 {
            return self.surface(generator, face, x, z).is_some();
        }
        self.touching(generator, face, Rect::around(x, z, margin)).iter().any(|road| road.near(x, z, margin))
    }

    /// Tous les chemins d'une face (les trace tous : à réserver aux outils et aux tests)
    pub fn all_on_face(&self, generator: &PlanetWorldGenerator, face: PlanetFace) -> Vec<&Road> {
        let mut found = Vec::new();
        for rz in 0..REGIONS_PER_FACE {
            for rx in 0..REGIONS_PER_FACE {
                for link in Link::ALL {
                    if let Some(road) = self.link(generator, face, rx, rz, link) {
                        found.push(road);
                    }
                }
            }
        }
        found
    }
}

/// Bloc posé par un chemin en (x, y, z), `ground` étant le premier bloc d'air au-dessus du relief :
/// le chemin remplace le dessus du sol, sauf sous la mer
#[inline]
pub fn road_block(surface: BlockId, y: i32, ground: i32) -> Option<BlockId> {
    (y == ground - 1 && ground as f64 > SEA_LEVEL_BLOCKS).then_some(surface)
}

// ---------------------------------------------------------------------------
// Tracé
// ---------------------------------------------------------------------------

/// Le bout de rue de `s` d'où partir vers `target` : la colonne de la sortie, et le point où le
/// chemin, parti tout droit, a quitté les abords de l'établissement
fn best_exit(s: &Settlement, target: (i32, i32)) -> Option<((i32, i32), (i32, i32))> {
    let clear = s.bounds.expand(6);
    s.exits
        .iter()
        .map(|&(at, side)| {
            let (dx, dz) = side.delta();
            let mut launch = at;
            for _ in 0..256 {
                if !clear.contains(launch.0, launch.1) {
                    break;
                }
                launch = (launch.0 + dx, launch.1 + dz);
            }
            let d = ((launch.0 - target.0) as i64).pow(2) + ((launch.1 - target.1) as i64).pow(2);
            (d, at, launch)
        })
        .min_by_key(|&(d, _, _)| d)
        .map(|(_, at, launch)| (at, launch))
}

fn plan_road(generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<Road> {
    let [ra, rb] = link.ends(rx, rz);
    let structures = &generator.structures;
    let a = structures.region_settlement(generator, face, ra.0, ra.1)?;
    let b = structures.region_settlement(generator, face, rb.0, rb.1)?;
    if let Some(others) = link.others(rx, rz) {
        if others.iter().any(|&(ox, oz)| structures.region_settlement(generator, face, ox, oz).is_some()) {
            return None;
        }
    }
    herocraft_common::prof_scope!("world::roads::plan_road");
    let area = link.area(rx, rz);
    let (exit_a, launch_a) = best_exit(a, b.center)?;
    let (exit_b, launch_b) = best_exit(b, a.center)?;
    // Les établissements de la zone (ceux qu'on relie compris) : le chemin les contourne
    let obstacles: Vec<Rect> = structures.touching(generator, face, area).iter().map(|s| s.bounds.expand(SETTLEMENT_CLEARANCE)).collect();
    let seed = mix(generator.planet.seed ^ 0x40AD_5000_0000_0000 ^ ((face_index(face) as u64) << 40) ^ hash2(0x40AD, rx * 4 + link as i32, rz));
    let mut search = Search { generator, face, area: area.expand(-4), obstacles, seed, grounds: HashMap::new() };
    let start = search.snap(launch_a)?;
    let goal = search.snap(launch_b)?;
    // Étapes intermédiaires écartées de la ligne droite, une tous les 600 blocs environ : le chemin
    // décrit de grandes courbes même en plaine, puis le A* et les ondulations font le reste
    let (dx, dz) = ((goal.0 - start.0) as f64, (goal.1 - start.1) as f64);
    let dist = (dx * dx + dz * dz).sqrt();
    let stops = (dist * STEP as f64 / 600.0).floor() as i32;
    let mut waypoints = Vec::new();
    let start_ground = search.ground_any(start)? as f64;
    let goal_ground = search.ground_any(goal)? as f64;
    let mut side = if unit(hash2(seed, 0x57, 0)) < 0.5 { 1.0 } else { -1.0 };
    for k in 1..=stops {
        let r = |n: i32| unit(hash2(seed, k, n));
        let t = (k as f64 + (r(1) - 0.5) * 0.4) / (stops + 1) as f64;
        // Le plus souvent de l'autre côté que l'étape précédente : un S plutôt qu'un arc
        if r(2) < 0.75 {
            side = -side;
        }
        let offset = side * dist * (0.07 + 0.11 * r(3));
        let (nx, nz) = (-dz / dist.max(1.0), dx / dist.max(1.0));
        // Repli vers la ligne droite si l'étape tombe dans l'eau ou sur un obstacle
        for shrink in [1.0, 0.6, 0.3] {
            let w = ((start.0 as f64 + dx * t + nx * offset * shrink).round() as i32, (start.1 as f64 + dz * t + nz * offset * shrink).round() as i32);
            // Pas d'étape perchée : elle ferait grimper le chemin pour rien
            let expected = start_ground + (goal_ground - start_ground) * t;
            if search.ground(w).is_some_and(|g| (g as f64 - expected).abs() < 24.0) {
                waypoints.push(w);
                break;
            }
        }
    }
    let mut nodes = vec![start];
    for leg in waypoints.iter().copied().chain([goal]).collect::<Vec<_>>() {
        let from = *nodes.last()?;
        let mut part = match search.run(from, leg) {
            Some(part) => part,
            // L'étape est enclavée : on la saute
            None if leg != goal => continue,
            None => return None,
        };
        nodes.extend(part.drain(1..));
    }

    let center = |p: (i32, i32)| (p.0 as f64 + 0.5, p.1 as f64 + 0.5);
    let mut points = vec![center(exit_a), center(launch_a)];
    points.extend(nodes.iter().map(|&(i, j)| ((i * STEP) as f64 + 0.5, (j * STEP) as f64 + 0.5)));
    points.push(center(launch_b));
    points.push(center(exit_b));
    points.dedup();
    // Arrondit les angles du tracé, en gardant les deux bouts de rue, puis le fait onduler : un
    // chemin tiré au cordeau sur des centaines de blocs ne ressemble à rien
    for _ in 0..2 {
        points = chaikin(&points);
    }
    points = relax(generator, face, &points);
    points = meander(generator, face, &points, seed);
    for _ in 0..2 {
        points = chaikin(&points);
    }
    let columns = rasterize(&points, &area, generator.planet.seed);
    let bounds = columns.keys().fold(Rect::new(i32::MAX, i32::MAX, i32::MIN, i32::MIN), |r, &(x, z)| Rect::new(r.x0.min(x), r.z0.min(z), r.x1.max(x + 1), r.z1.max(z + 1)));
    Some(Road { face, from: a.center, to: b.center, points, bounds, columns })
}

/// Bruit de valeur à une dimension, lissé, dans [-1, 1]
fn noise_1d(seed: u64, t: f64) -> f64 {
    let i = t.floor();
    let f = t - i;
    let at = |k: f64| unit(hash2(seed, k as i32, 0x3EA)) * 2.0 - 1.0;
    let w = f * f * (3.0 - 2.0 * f);
    at(i) + (at(i + 1.0) - at(i)) * w
}

/// Bruit de valeur à deux dimensions, lissé, dans [0, 1], de maille `scale` blocs
fn noise_2d(seed: u64, x: f64, z: f64, scale: f64) -> f64 {
    let (x, z) = (x / scale, z / scale);
    let (i, j) = (x.floor(), z.floor());
    let (fx, fz) = (x - i, z - j);
    let at = |a: f64, b: f64| unit(hash2(seed, a as i32, b as i32));
    let (wx, wz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let top = at(i, j) + (at(i + 1.0, j) - at(i, j)) * wx;
    let bottom = at(i, j + 1.0) + (at(i + 1.0, j + 1.0) - at(i, j + 1.0)) * wx;
    top + (bottom - top) * wz
}

/// Pas de reprise de l'axe, en blocs
const RESAMPLE: f64 = 4.0;

/// L'axe repris à pas régulier (`RESAMPLE` blocs), bouts compris
fn resample(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut even = vec![points[0]];
    let mut carry = 0.0;
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let len = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
        let mut t = RESAMPLE - carry;
        while t < len {
            even.push((p.0 + (q.0 - p.0) * t / len, p.1 + (q.1 - p.1) * t / len));
            t += RESAMPLE;
        }
        carry = len - (t - RESAMPLE);
    }
    even.push(points[points.len() - 1]);
    even
}

/// 0 à moins de `keep` blocs d'un bout du chemin, 1 au-delà de `keep + ramp`, en douceur entre les deux
fn fade_near_ends(s: f64, total: f64, keep: f64, ramp: f64) -> f64 {
    let e = ((s.min(total - s) - keep) / ramp).clamp(0.0, 1.0);
    e * e * (3.0 - 2.0 * e)
}

/// Lisse l'axe à grande échelle (moyenne glissante sur ±`SMOOTH_RADIUS` blocs) : les longues
/// lignes droites et les coudes du tracé sur grille deviennent de larges courbes. Un point que le
/// lissage mettrait dans l'eau, ou sur une pente bien plus raide que le tracé d'origine, reste où il
/// était ; les abords des bouts de rue ne bougent pas.
fn relax(generator: &PlanetWorldGenerator, face: PlanetFace, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    const SMOOTH_RADIUS: f64 = 64.0;
    let even = resample(points);
    let n = even.len();
    let radius = (SMOOTH_RADIUS / RESAMPLE) as usize;
    if n < 2 * radius + 2 {
        return points.to_vec();
    }
    let total = (n - 1) as f64 * RESAMPLE;
    let ground = |p: (f64, f64)| generator.sample_block_column(face, p.0.floor() as i32, p.1.floor() as i32).elevation.floor();
    let mut out = even.clone();
    for k in 1..n - 1 {
        let weight = fade_near_ends(k as f64 * RESAMPLE, total, 16.0, 64.0);
        if weight <= 0.0 {
            continue;
        }
        let (lo, hi) = (k.saturating_sub(radius), (k + radius).min(n - 1));
        let (mut sx, mut sz, mut sw) = (0.0, 0.0, 0.0);
        for (j, p) in even.iter().enumerate().take(hi + 1).skip(lo) {
            // Poids en triangle, symétrique : la fenêtre se resserre près des bouts
            let w = (radius + 1 - k.abs_diff(j).min(radius)) as f64;
            sx += p.0 * w;
            sz += p.1 * w;
            sw += w;
        }
        let p = even[k];
        let moved = (p.0 + (sx / sw - p.0) * weight, p.1 + (sz / sw - p.1) * weight);
        if (moved.0 - p.0).abs() + (moved.1 - p.1).abs() < 1.0 {
            out[k] = moved;
            continue;
        }
        let g = ground(moved);
        if g <= SEA_LEVEL_BLOCKS + 1.0 || (g - ground(p)).abs() > 6.0 {
            continue;
        }
        out[k] = moved;
    }
    out
}

/// Écart latéral maximal des ondulations du chemin, en blocs
const MEANDER: f64 = 12.0;

/// Fait onduler l'axe : repris tous les 4 blocs, chaque point est écarté sur le côté d'une
/// quantité qui varie doucement le long du chemin (deux longueurs d'onde mêlées). Les ondulations
/// s'éteignent près des deux bouts de rue, et ne poussent jamais le chemin dans l'eau.
fn meander(generator: &PlanetWorldGenerator, face: PlanetFace, points: &[(f64, f64)], seed: u64) -> Vec<(f64, f64)> {
    let even = resample(points);
    let n = even.len();
    if n < 4 {
        return points.to_vec();
    }
    let fade = |s: f64| fade_near_ends(s, (n - 1) as f64 * RESAMPLE, 12.0, 48.0);
    let mut out = Vec::with_capacity(n);
    out.push(even[0]);
    for k in 1..n - 1 {
        let s = k as f64 * 4.0;
        let (prev, next) = (even[k - 1], even[k + 1]);
        let (tx, tz) = (next.0 - prev.0, next.1 - prev.1);
        let tl = (tx * tx + tz * tz).sqrt().max(1e-9);
        let (nx, nz) = (-tz / tl, tx / tl);
        let offset = MEANDER * fade(s) * (0.75 * noise_1d(seed ^ 0x3EA1, s / 140.0) + 0.25 * noise_1d(seed ^ 0x3EA2, s / 45.0));
        let p = even[k];
        let moved = (p.0 + nx * offset, p.1 + nz * offset);
        let wet = offset.abs() > 0.5 && {
            let column = generator.sample_block_column(face, moved.0 as i32, moved.1 as i32);
            column.elevation.floor() <= SEA_LEVEL_BLOCKS + 1.0
        };
        out.push(if wet { p } else { moved });
    }
    out.push(even[n - 1]);
    out
}

/// Une passe de Chaikin : chaque segment est remplacé par ses points au quart et aux trois quarts,
/// les deux bouts restent
fn chaikin(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        out.push((0.75 * p.0 + 0.25 * q.0, 0.75 * p.1 + 0.25 * q.1));
        out.push((0.25 * p.0 + 0.75 * q.0, 0.25 * p.1 + 0.75 * q.1));
    }
    out.push(points[points.len() - 1]);
    out
}

/// Colonnes à moins de `EDGE_HALF` de l'axe : gravier au cœur, gravier ou terre sur les bords
fn rasterize(points: &[(f64, f64)], area: &Rect, seed: u64) -> HashMap<(i32, i32), BlockId> {
    let mut distance: HashMap<(i32, i32), f64> = HashMap::new();
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let (x0, x1) = ((p.0.min(q.0) - EDGE_HALF).floor() as i32 - 1, (p.0.max(q.0) + EDGE_HALF).ceil() as i32 + 1);
        let (z0, z1) = ((p.1.min(q.1) - EDGE_HALF).floor() as i32 - 1, (p.1.max(q.1) + EDGE_HALF).ceil() as i32 + 1);
        let (vx, vz) = (q.0 - p.0, q.1 - p.1);
        let len2 = vx * vx + vz * vz;
        for z in z0..=z1 {
            for x in x0..=x1 {
                if !area.contains(x, z) {
                    continue;
                }
                let (cx, cz) = (x as f64 + 0.5, z as f64 + 0.5);
                let t = if len2 > 0.0 { (((cx - p.0) * vx + (cz - p.1) * vz) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let d = ((cx - p.0 - t * vx).powi(2) + (cz - p.1 - t * vz).powi(2)).sqrt();
                if d <= EDGE_HALF {
                    let e = distance.entry((x, z)).or_insert(f64::INFINITY);
                    *e = e.min(d);
                }
            }
        }
    }
    distance
        .into_iter()
        .map(|((x, z), d)| {
            let block = if d <= CORE_HALF || unit(hash2(seed ^ 0x40AD_E06E, x, z)) < 0.55 { GRAVEL } else { BlockId::DIRT };
            ((x, z), block)
        })
        .collect()
}

/// Recherche A* du tracé sur la grille de pas `STEP`
struct Search<'a> {
    generator: &'a PlanetWorldGenerator,
    face: PlanetFace,
    /// Colonnes où le tracé reste
    area: Rect,
    obstacles: Vec<Rect>,
    seed: u64,
    /// Sol des nœuds déjà relevés (`None` : infranchissable)
    grounds: HashMap<(i32, i32), Option<i32>>,
}

impl Search<'_> {
    /// Le nœud de la grille le plus proche de la colonne, dans la zone
    fn snap(&self, at: (i32, i32)) -> Option<(i32, i32)> {
        let lo = |v: i32| (v + STEP - 1).div_euclid(STEP);
        let hi = |v: i32| (v - 1).div_euclid(STEP);
        let (i0, i1, j0, j1) = (lo(self.area.x0), hi(self.area.x1), lo(self.area.z0), hi(self.area.z1));
        if i0 > i1 || j0 > j1 {
            return None;
        }
        let round = |v: i32| (v + STEP / 2).div_euclid(STEP);
        Some((round(at.0).clamp(i0, i1), round(at.1).clamp(j0, j1)))
    }

    /// Sol du nœud (i, j), `None` s'il est infranchissable (mer, glace, hors zone, établissement)
    fn ground(&mut self, node: (i32, i32)) -> Option<i32> {
        if let Some(&g) = self.grounds.get(&node) {
            return g;
        }
        let (x, z) = (node.0 * STEP, node.1 * STEP);
        let g = if !self.area.contains(x, z) || self.obstacles.iter().any(|o| o.contains(x, z)) {
            None
        } else {
            let column = self.generator.sample_block_column(self.face, x, z);
            let ground = column.elevation.floor() as i32;
            let passable = (ground as f64) > SEA_LEVEL_BLOCKS + 1.0
                && !matches!(column.biome, BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea | BiomeType::SnowPeaks);
            passable.then_some(ground)
        };
        self.grounds.insert(node, g);
        g
    }

    fn run(&mut self, start: (i32, i32), goal: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        // Les deux bouts sont hors des obstacles par construction ; leur sol doit être relevé quand même
        let start_ground = self.forced_ground(start)?;
        let goal_ground = self.forced_ground(goal)?;
        self.grounds.insert(start, Some(start_ground));
        self.grounds.insert(goal, Some(goal_ground));

        // Coûts en seizièmes de bloc, entiers : l'ordre de la recherche ne dépend d'aucun arrondi
        let heuristic = |n: (i32, i32)| -> i64 {
            let (dx, dz) = ((n.0 - goal.0).abs() as i64, (n.1 - goal.1).abs() as i64);
            let (lo, hi) = (dx.min(dz), dx.max(dz));
            // Un peu gourmande (× 1,2) : le tracé reste proche du meilleur, la recherche est bien plus courte
            ((hi - lo) * STEP as i64 * 16 + lo * (STEP as f64 * std::f64::consts::SQRT_2 * 16.0) as i64) * 6 / 5
        };
        let mut best: HashMap<(i32, i32), (i64, (i32, i32))> = HashMap::new();
        let mut open = BinaryHeap::new();
        best.insert(start, (0, start));
        open.push(Reverse((heuristic(start), 0i64, start.0, start.1)));
        let mut expansions = 0;
        while let Some(Reverse((_, cost, i, j))) = open.pop() {
            let node = (i, j);
            if best.get(&node).is_some_and(|&(c, _)| c < cost) {
                continue;
            }
            if node == goal {
                let mut path = vec![goal];
                let mut at = goal;
                while at != start {
                    at = best[&at].1;
                    path.push(at);
                }
                path.reverse();
                return Some(path);
            }
            expansions += 1;
            if expansions > MAX_EXPANSIONS {
                return None;
            }
            let here = self.grounds[&node]?;
            for (di, dj) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let next = (i + di, j + dj);
                let Some(there) = self.ground(next) else {
                    continue;
                };
                let len = if di != 0 && dj != 0 { STEP as f64 * std::f64::consts::SQRT_2 } else { STEP as f64 };
                let slope = (there - here).abs() as f64 / len;
                if slope > MAX_SLOPE {
                    continue;
                }
                // Une montée coûte vite plus cher qu'un détour. Un bruit doux, à grande maille, rend
                // certains terrains un peu moins commodes que d'autres : le chemin les contourne en
                // larges courbes plutôt que de filer tout droit
                let (wx, wz) = ((next.0 * STEP) as f64, (next.1 * STEP) as f64);
                let wander = 0.7 * noise_2d(self.seed, wx, wz, 240.0) + 0.3 * noise_2d(self.seed ^ 0x5EED, wx, wz, 90.0);
                let step = len * (1.0 + 14.0 * slope * slope + wander);
                let total = cost + (step * 16.0) as i64;
                if best.get(&next).is_none_or(|&(c, _)| total < c) {
                    best.insert(next, (total, node));
                    open.push(Reverse((total + heuristic(next), total, next.0, next.1)));
                }
            }
        }
        None
    }

    /// Sol d'un nœud, obstacles compris
    fn ground_any(&self, node: (i32, i32)) -> Option<i32> {
        Some(self.generator.sample_block_column(self.face, node.0 * STEP, node.1 * STEP).elevation.floor() as i32)
    }

    /// Sol d'un bout du tracé, sans tenir compte des obstacles (il touche son établissement)
    fn forced_ground(&self, node: (i32, i32)) -> Option<i32> {
        let column = self.generator.sample_block_column(self.face, node.0 * STEP, node.1 * STEP);
        let ground = column.elevation.floor() as i32;
        ((ground as f64) > SEA_LEVEL_BLOCKS + 1.0).then_some(ground)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::structures::SettlementKind;

    fn generator(seed: u64) -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed, ..Default::default() })
    }

    #[test]
    fn test_settlements_are_linked_by_roads() {
        let generator = generator(2026);
        let mut roads = 0;
        let mut linked = Vec::new();
        for face in PlanetFace::ALL {
            for road in generator.roads.all_on_face(&generator, face) {
                roads += 1;
                // Le chemin part d'un établissement et arrive à l'autre
                let ends = [road.points[0], road.points[road.points.len() - 1]];
                for (end, center) in ends.iter().zip([road.from, road.to]) {
                    let s = generator.settlement_at(face, center.0, center.1).expect("établissement au bout du chemin");
                    assert!(s.kind != SettlementKind::Ruins);
                    assert!(s.bounds.expand(2).contains(end.0 as i32, end.1 as i32), "bout {end:?} loin de {:?}", s.bounds);
                }
                let straight = (((road.from.0 - road.to.0) as f64).powi(2) + ((road.from.1 - road.to.1) as f64).powi(2)).sqrt();
                assert!(road.length() < straight * 2.5, "chemin de {:.0} blocs pour {straight:.0} à vol d'oiseau", road.length());
                linked.push((face, road.from));
                linked.push((face, road.to));
            }
        }
        linked.sort_by_key(|&(f, c)| (face_index(f), c));
        linked.dedup();
        println!("{roads} chemins, {} établissements reliés", linked.len());
        assert!(roads >= 15, "seulement {roads} chemins");
    }

    #[test]
    fn test_roads_stay_on_dry_and_gentle_ground() {
        let generator = generator(2026);
        let face_roads: Vec<&Road> = PlanetFace::ALL.iter().flat_map(|&f| generator.roads.all_on_face(&generator, f)).take(6).collect();
        assert!(!face_roads.is_empty());
        for road in face_roads {
            let mut wet = 0;
            let mut total = 0;
            for (i, &(x, z)) in road.columns.keys().enumerate() {
                if i % 7 != 0 {
                    continue;
                }
                total += 1;
                let column = generator.sample_block_column(road.face, x, z);
                if column.elevation.floor() <= SEA_LEVEL_BLOCKS {
                    wet += 1;
                }
            }
            assert!(wet * 100 <= total, "{wet} colonnes sous la mer sur {total}");
        }
    }

    #[test]
    fn test_road_blocks_agree_between_chunks_and_single_blocks() {
        use herocraft_common::PlanetChunkCoord;
        let generator = generator(2026);
        let road = PlanetFace::ALL.iter().flat_map(|&f| generator.roads.all_on_face(&generator, f)).next().expect("un chemin");
        let face = road.face;
        // Un point au milieu du chemin, loin des établissements
        let mid = road.points[road.points.len() / 2];
        let (bx, bz) = (mid.0 as i32, mid.1 as i32);
        assert_eq!(generator.settlement_at(face, bx, bz).map(|s| s.kind), None);
        let ground = generator.sample_block_column(face, bx, bz).elevation.floor() as i32;
        let (cx, cz) = (bx.div_euclid(32), bz.div_euclid(32));
        let mut gravel = 0;
        for layer in [(ground - 1).div_euclid(32), ground.div_euclid(32)] {
            let chunk = generator.generate_chunk(&PlanetChunkCoord::new(face, cx, cz, layer));
            for z in 0..32 {
                for x in 0..32 {
                    for y in 0..32 {
                        let (gx, gy, gz) = (cx * 32 + x, layer * 32 + y, cz * 32 + z);
                        let block = generator.block_at(face, gx, gy, gz);
                        assert_eq!(chunk.get_voxel(x as u8, y as u8, z as u8), block, "({gx}, {gy}, {gz})");
                        gravel += (block == GRAVEL) as i32;
                    }
                }
            }
        }
        assert!(gravel > 20, "seulement {gravel} blocs de gravier");
        // Ni herbe haute, ni arbre sur le chemin
        assert!(generator.is_built(face, bx, bz, 0));
        let column = generator.sample_block_column(face, bx, bz);
        assert_eq!(generator.ground_plant(&column, face, bx, bz), None);
    }

    #[test]
    fn test_roads_do_not_depend_on_reading_order() {
        let a = generator(77);
        let b = generator(77);
        let face = PlanetFace::ALL[2];
        let forward = a.roads.all_on_face(&a, face);
        // Dans l'autre ordre : les liaisons de la dernière région d'abord
        for rz in (0..REGIONS_PER_FACE).rev() {
            for rx in (0..REGIONS_PER_FACE).rev() {
                for link in Link::ALL.into_iter().rev() {
                    b.roads.link(&b, face, rx, rz, link);
                }
            }
        }
        let backward = b.roads.all_on_face(&b, face);
        assert_eq!(forward.len(), backward.len());
        for (r, s) in forward.iter().zip(&backward) {
            assert_eq!(r.points, s.points);
            assert_eq!(r.columns, s.columns);
        }
    }
}

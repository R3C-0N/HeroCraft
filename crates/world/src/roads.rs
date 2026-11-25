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

    /// Chemin d'essai réduit à son axe
    #[cfg(test)]
    pub(crate) fn for_tests(face: PlanetFace, from: (i32, i32), to: (i32, i32), points: Vec<(f64, f64)>) -> Self {
        Road { face, from, to, points, bounds: Rect::new(0, 0, 0, 0), columns: HashMap::new() }
    }
}

/// Les chemins d'une planète, liaison par liaison, tracés à la première lecture
#[derive(Debug)]
pub struct RoadMap {
    links: Vec<OnceLock<Option<Road>>>,
    guides: Vec<OnceLock<Option<Guide>>>,
}

impl Default for RoadMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RoadMap {
    pub fn new() -> Self {
        let count = 6 * REGIONS_PER_FACE * REGIONS_PER_FACE * Link::ALL.len() as i32;
        Self { links: (0..count).map(|_| OnceLock::new()).collect(), guides: (0..count).map(|_| OnceLock::new()).collect() }
    }

    fn index(face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<usize> {
        if !(0..REGIONS_PER_FACE).contains(&rx) || !(0..REGIONS_PER_FACE).contains(&rz) {
            return None;
        }
        let slot = Link::ALL.iter().position(|l| *l == link)? as i32;
        Some((((face_index(face) * REGIONS_PER_FACE + rz) * REGIONS_PER_FACE + rx) * Link::ALL.len() as i32 + slot) as usize)
    }

    fn link(&self, generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<&Road> {
        let index = Self::index(face, rx, rz, link)?;
        self.links[index].get_or_init(|| plan_road(generator, face, rx, rz, link)).as_ref()
    }

    fn guide(&self, generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<&Guide> {
        let index = Self::index(face, rx, rz, link)?;
        self.guides[index].get_or_init(|| plan_guide(generator, face, rx, rz, link)).as_ref()
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
pub(crate) fn road_block(surface: BlockId, y: i32, ground: i32) -> Option<BlockId> {
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

/// Tracé brut d'une liaison, d'un bout de rue à l'autre (courbe lisse, sans ondulations ni tronc
/// commun) : sert de guide au chemin, et dit dans quelle direction il quitte chaque établissement
#[derive(Debug, Clone)]
struct Guide {
    points: Vec<(f64, f64)>,
}

fn plan_guide(generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<Guide> {
    let [ra, rb] = link.ends(rx, rz);
    let structures = &generator.structures;
    let a = structures.region_settlement(generator, face, ra.0, ra.1)?;
    let b = structures.region_settlement(generator, face, rb.0, rb.1)?;
    if let Some(others) = link.others(rx, rz)
        && others.iter().any(|&(ox, oz)| structures.region_settlement(generator, face, ox, oz).is_some()) {
        return None;
    }
    herocraft_common::prof_scope!("world::roads::plan_guide");
    let (exit_a, launch_a) = best_exit(a, b.center)?;
    let (exit_b, launch_b) = best_exit(b, a.center)?;
    let center = |p: (i32, i32)| (p.0 as f64 + 0.5, p.1 as f64 + 0.5);
    let mut search = Search::for_area(generator, face, link.area(rx, rz), link_seed(generator, face, rx, rz, link));
    let start = search.snap(launch_a)?;
    let goal = search.snap(launch_b)?;
    let nodes = search.with_waypoints(start, goal)?;
    let grid = grid_points(&nodes);
    Some(Guide { points: smooth_curve(generator, face, [center(exit_a), center(launch_a)], &grid, [center(launch_b), center(exit_b)]) })
}

fn link_seed(generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> u64 {
    mix(generator.planet.seed ^ 0x40AD_5000_0000_0000 ^ ((face_index(face) as u64) << 40) ^ hash2(0x40AD, rx * 4 + link as i32, rz))
}

fn grid_points(nodes: &[(i32, i32)]) -> Vec<(f64, f64)> {
    nodes.iter().map(|&(i, j)| ((i * STEP) as f64 + 0.5, (j * STEP) as f64 + 0.5)).collect()
}

/// La liaison entre les régions `from` et `to` (voisines), et si elle va de `to` vers `from`
fn link_between(from: (i32, i32), to: (i32, i32)) -> Option<(i32, i32, Link, bool)> {
    let (x, z) = from;
    Some(match (to.0 - x, to.1 - z) {
        (1, 0) => (x, z, Link::East, false),
        (-1, 0) => (x - 1, z, Link::East, true),
        (0, 1) => (x, z, Link::South, false),
        (0, -1) => (x, z - 1, Link::South, true),
        (1, 1) => (x, z, Link::Down, false),
        (-1, -1) => (x - 1, z - 1, Link::Down, true),
        (-1, 1) => (x - 1, z, Link::Up, false),
        (1, -1) => (x, z - 1, Link::Up, true),
        _ => return None,
    })
}

/// Angle sous lequel deux chemins quittent ensemble un établissement (en radians, environ 40°)
const SHARED_ANGLE: f64 = 0.7;
/// Distance, en blocs depuis le centre, à laquelle on mesure la direction de départ d'un chemin
const DEPARTURE: f64 = 320.0;
/// Deux guides plus proches que cela suivent le même tronc
const SHARED_GAP: f64 = 28.0;
/// Le tronc commun s'arrête au plus loin à tant de blocs (le long du chemin) de l'établissement
const MAX_TRUNK: f64 = 900.0;

/// Tronc commun aux chemins qui quittent un établissement dans la même direction : la partie du
/// guide de référence jusqu'au carrefour où les guides se séparent
#[derive(Debug, Clone)]
struct Trunk {
    /// Du bout de rue au carrefour
    points: Vec<(f64, f64)>,
    /// Direction du tronc au carrefour (unitaire)
    dir: (f64, f64),
}

impl Trunk {
    fn junction(&self) -> (f64, f64) {
        self.points[self.points.len() - 1]
    }

    /// Un peu au-delà du carrefour : les branches partent dans le prolongement du tronc
    fn ahead(&self) -> (f64, f64) {
        let j = self.junction();
        (j.0 + self.dir.0 * 64.0, j.1 + self.dir.1 * 64.0)
    }
}

/// Le tronc que le chemin de la région `region` vers la région `target` partage avec les autres
/// chemins qui partent du même établissement dans la même direction. Tout se décide sur les guides
/// (qui ne dépendent d'aucun tronc) : chaque chemin du groupe trouve le même tronc.
fn shared_trunk(generator: &PlanetWorldGenerator, face: PlanetFace, s: &Settlement, region: (i32, i32), target: (i32, i32)) -> Option<Trunk> {
    use std::f64::consts::TAU;
    let c = (s.center.0 as f64 + 0.5, s.center.1 as f64 + 0.5);
    // Les guides des liaisons de l'établissement, orientés depuis lui, et leur direction de départ
    // (angle de départ, région d'arrivée, guide orienté depuis l'établissement)
    type Departure = (f64, (i32, i32), Vec<(f64, f64)>);
    let mut guides: Vec<Departure> = Vec::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let other = (region.0 + dx, region.1 + dz);
            let Some((lx, lz, link, reversed)) = link_between(region, other) else {
                continue;
            };
            let Some(guide) = generator.roads.guide(generator, face, lx, lz, link) else {
                continue;
            };
            let mut points = resample(&guide.points);
            if reversed {
                points.reverse();
            }
            let at = points.iter().copied().find(|p| (p.0 - c.0).hypot(p.1 - c.1) >= DEPARTURE).unwrap_or(points[points.len() - 1]);
            guides.push(((at.1 - c.1).atan2(at.0 - c.0).rem_euclid(TAU), other, points));
        }
    }
    if guides.len() < 2 {
        return None;
    }
    guides.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    // On coupe le cercle au plus grand écart, puis on enchaîne les départs proches
    let n = guides.len();
    let gap = |i: usize| (guides[(i + 1) % n].0 - guides[i].0).rem_euclid(TAU);
    let cut = (0..n).max_by(|&i, &j| gap(i).total_cmp(&gap(j)).then(j.cmp(&i)))?;
    let order: Vec<usize> = (1..=n).map(|k| (cut + k) % n).collect();
    let mut groups: Vec<Vec<usize>> = vec![vec![order[0]]];
    for w in order.windows(2) {
        if gap(w[0]) < SHARED_ANGLE {
            groups.last_mut()?.push(w[1]);
        } else {
            groups.push(vec![w[1]]);
        }
    }
    let group = groups.into_iter().find(|g| g.iter().any(|&i| guides[i].1 == target))?;
    if group.len() < 2 {
        return None;
    }
    // Le guide de référence : celui de la région de plus petit numéro ; le carrefour est le dernier
    // point de son début que tous les autres guides du groupe longent encore
    let reference = *group.iter().min_by_key(|&&i| guides[i].1)?;
    let main = &guides[reference].2;
    let r = REGION_BLOCKS;
    let inside = Rect::new(region.0 * r, region.1 * r, ((region.0 + 1) * r).min(FACE_BLOCKS), ((region.1 + 1) * r).min(FACE_BLOCKS)).expand(-32);
    let clear = s.bounds.expand(24);
    let max = ((MAX_TRUNK / RESAMPLE) as usize).min(main.len() * 2 / 5);
    // Les guides peuvent partir de portes différentes avant de se rejoindre : on garde le dernier
    // point que tous longent encore, ou à défaut un carrefour à quelque distance de l'établissement
    let mut last = None;
    let mut fallback = None;
    for (k, &p) in main.iter().enumerate().take(max) {
        if !inside.contains(p.0 as i32, p.1 as i32) {
            break;
        }
        if clear.contains(p.0 as i32, p.1 as i32) {
            continue;
        }
        if fallback.is_none() && (p.0 - c.0).hypot(p.1 - c.1) >= 220.0 {
            fallback = Some(k);
        }
        if group.iter().filter(|&&i| i != reference).all(|&i| guides[i].2.iter().any(|q| (q.0 - p.0).hypot(q.1 - p.1) < SHARED_GAP)) {
            last = Some(k);
        }
    }
    let k = last.filter(|&k| k as f64 * RESAMPLE >= 120.0).or(fallback)?.max(2);
    let (p, q) = (main[k - 2], main[k]);
    let len = (q.0 - p.0).hypot(q.1 - p.1).max(1e-6);
    Some(Trunk { points: main[..=k].to_vec(), dir: ((q.0 - p.0) / len, (q.1 - p.1) / len) })
}

fn plan_road(generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32, link: Link) -> Option<Road> {
    let guide = generator.roads.guide(generator, face, rx, rz, link)?;
    let [ra, rb] = link.ends(rx, rz);
    let structures = &generator.structures;
    let a = structures.region_settlement(generator, face, ra.0, ra.1)?;
    let b = structures.region_settlement(generator, face, rb.0, rb.1)?;
    herocraft_common::prof_scope!("world::roads::plan_road");
    let area = link.area(rx, rz);
    let seed = link_seed(generator, face, rx, rz, link);
    let trunk_a = shared_trunk(generator, face, a, ra, rb);
    let trunk_b = shared_trunk(generator, face, b, rb, ra);

    let middle = if trunk_a.is_none() && trunk_b.is_none() {
        guide.points.clone()
    } else {
        // Entre les carrefours (ou les bouts de rue), une nouvelle branche
        let center = |p: (i32, i32)| (p.0 as f64 + 0.5, p.1 as f64 + 0.5);
        let head = match &trunk_a {
            Some(t) => [t.junction(), t.ahead()],
            None => {
                let (exit, launch) = best_exit(a, b.center)?;
                [center(exit), center(launch)]
            }
        };
        let tail = match &trunk_b {
            Some(t) => [t.ahead(), t.junction()],
            None => {
                let (exit, launch) = best_exit(b, a.center)?;
                [center(launch), center(exit)]
            }
        };
        let mut search = Search::for_area(generator, face, area, seed);
        let start = search.snap((head[1].0 as i32, head[1].1 as i32))?;
        let goal = search.snap((tail[0].0 as i32, tail[0].1 as i32))?;
        let nodes = search.with_waypoints(start, goal)?;
        smooth_curve(generator, face, head, &grid_points(&nodes), tail)
    };
    let mut points = Vec::new();
    // Le tronc ondule selon sa propre graine : il est identique pour tous les chemins qui le suivent
    let trunk_seed = |t: &Trunk| mix(generator.planet.seed ^ 0x7A0C ^ hash2(0x7A0C, t.junction().0 as i32, t.junction().1 as i32));
    // Le tronc serpente dans la région de son établissement seule : sa forme ne dépend pas de la liaison
    let region_rect = |r: (i32, i32)| {
        let size = REGION_BLOCKS;
        Rect::new(r.0 * size, r.1 * size, ((r.0 + 1) * size).min(FACE_BLOCKS), ((r.1 + 1) * size).min(FACE_BLOCKS))
    };
    if let Some(t) = &trunk_a {
        let wavy = meander(generator, face, &t.points, trunk_seed(t), &region_rect(ra));
        points.extend_from_slice(&wavy[..wavy.len() - 1]);
    }
    points.extend(meander(generator, face, &middle, seed, &area));
    if let Some(t) = &trunk_b {
        let wavy = meander(generator, face, &t.points, trunk_seed(t), &region_rect(rb));
        points.extend(wavy.iter().rev().skip(1));
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

/// Espacement, en blocs, des points du guide par lesquels passe la courbe du chemin
const CONTROL_SPACING: usize = 120;

/// Courbe de Catmull-Rom centripète par les points de contrôle, échantillonnée tous les blocs ou presque
fn catmull_rom(controls: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    let n = controls.len();
    let at = |i: isize| -> (f64, f64) {
        if i < 0 {
            // Points fantômes : prolongent les bouts en ligne droite
            let (a, b) = (controls[0], controls[1]);
            (2.0 * a.0 - b.0, 2.0 * a.1 - b.1)
        } else if i as usize >= n {
            let (a, b) = (controls[n - 1], controls[n - 2]);
            (2.0 * a.0 - b.0, 2.0 * a.1 - b.1)
        } else {
            controls[i as usize]
        }
    };
    let knot = |p: (f64, f64), q: (f64, f64)| ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt().sqrt().max(1e-6);
    (0..n - 1)
        .map(|i| {
            let i = i as isize;
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let (t1, t2, t3) = (knot(p0, p1), knot(p1, p2), knot(p2, p3));
            let (t1, t2, t3) = (t1, t1 + t2, t1 + t2 + t3);
            let len = ((p2.0 - p1.0).powi(2) + (p2.1 - p1.1).powi(2)).sqrt();
            let steps = (len / 1.5).ceil().max(1.0) as usize;
            let lerp = |a: (f64, f64), b: (f64, f64), ta: f64, tb: f64, t: f64| {
                let w = if tb - ta == 0.0 { 0.0 } else { (t - ta) / (tb - ta) };
                (a.0 + (b.0 - a.0) * w, a.1 + (b.1 - a.1) * w)
            };
            (0..steps)
                .map(|k| {
                    let t = t1 + (t2 - t1) * k as f64 / steps as f64;
                    let a1 = lerp(p0, p1, 0.0, t1, t);
                    let a2 = lerp(p1, p2, t1, t2, t);
                    let a3 = lerp(p2, p3, t2, t3, t);
                    let b1 = lerp(a1, a2, 0.0, t2, t);
                    let b2 = lerp(a2, a3, t1, t3, t);
                    lerp(b1, b2, t1, t2, t)
                })
                .collect()
        })
        .collect()
}

/// Le chemin, courbe lisse qui part tout droit des deux bouts de rue (`head`, `tail` : la sortie et
/// le point où l'on quitte les abords) et suit le guide `grid` du A* par des points espacés de
/// `CONTROL_SPACING` blocs. Là où la courbe couperait par l'eau, un point du guide de plus la
/// ramène vers lui.
fn smooth_curve(generator: &PlanetWorldGenerator, face: PlanetFace, head: [(f64, f64); 2], grid: &[(f64, f64)], tail: [(f64, f64); 2]) -> Vec<(f64, f64)> {
    let guide = if grid.len() >= 2 { resample(grid) } else { grid.to_vec() };
    let spacing = CONTROL_SPACING / RESAMPLE as usize;
    // Indices des points du guide retenus (ni trop près des bouts, où la courbe rejoint les rues)
    let margin = (40.0 / RESAMPLE) as usize;
    let mut picks: Vec<usize> = Vec::new();
    if guide.len() > 2 * margin {
        let count = ((guide.len() - 2 * margin) / spacing).max(1);
        for k in 0..=count {
            picks.push(margin + (guide.len() - 1 - 2 * margin) * k / count);
        }
    }
    let wet = |p: (f64, f64)| generator.sample_block_column(face, p.0.floor() as i32, p.1.floor() as i32).elevation.floor() <= SEA_LEVEL_BLOCKS + 1.0;
    for _ in 0..6 {
        let controls: Vec<(f64, f64)> = head.iter().copied().chain(picks.iter().map(|&i| guide[i])).chain(tail).collect();
        let segments = catmull_rom(&controls);
        // Segments (entre deux points du guide) qui coupent par l'eau : un point de plus au milieu
        let mut extra = Vec::new();
        for (s, segment) in segments.iter().enumerate() {
            if s < 2 || s >= 2 + picks.len().saturating_sub(1) {
                continue;
            }
            let (i, j) = (picks[s - 2], picks[s - 1]);
            if j > i + 1 && segment.iter().step_by(3).any(|&p| wet(p)) {
                extra.push((i + j) / 2);
            }
        }
        if extra.is_empty() {
            let mut points: Vec<(f64, f64)> = segments.into_iter().flatten().collect();
            points.push(tail[1]);
            return points;
        }
        picks.extend(extra);
        picks.sort_unstable();
        picks.dedup();
    }
    let controls: Vec<(f64, f64)> = head.iter().copied().chain(picks.iter().map(|&i| guide[i])).chain(tail).collect();
    let mut points: Vec<(f64, f64)> = catmull_rom(&controls).into_iter().flatten().collect();
    points.push(tail[1]);
    points
}

/// Écart latéral maximal des ondulations du chemin, en blocs
const MEANDER: f64 = 55.0;

/// Fait serpenter l'axe : repris tous les 4 blocs, chaque point est écarté sur le côté d'une
/// quantité qui varie doucement le long du chemin (grandes courbes de 300 blocs environ, plus
/// petites par-dessus) : le chemin tourne sans cesse, sans jamais faire de coude. Les courbes
/// s'éteignent près des deux bouts, se resserrent là où elles pousseraient le chemin dans l'eau, sur
/// une pente ou hors de sa zone, et ce resserrement est lui-même lissé.
fn meander(generator: &PlanetWorldGenerator, face: PlanetFace, points: &[(f64, f64)], seed: u64, area: &Rect) -> Vec<(f64, f64)> {
    let even = resample(points);
    let n = even.len();
    if n < 4 {
        return points.to_vec();
    }
    let total = (n - 1) as f64 * RESAMPLE;
    let inside = area.expand(-6);
    let ground = |p: (f64, f64)| generator.sample_block_column(face, p.0.floor() as i32, p.1.floor() as i32).elevation.floor();
    // Écart voulu et direction de chaque point
    let mut wanted = vec![(0.0, (0.0, 0.0)); n];
    for (k, w) in wanted.iter_mut().enumerate().take(n - 1).skip(1) {
        let s = k as f64 * RESAMPLE;
        let (prev, next) = (even[k - 1], even[k + 1]);
        let (tx, tz) = (next.0 - prev.0, next.1 - prev.1);
        let tl = (tx * tx + tz * tz).sqrt().max(1e-9);
        let swing = 0.7 * noise_1d(seed ^ 0x3EA1, s / 320.0) + 0.3 * noise_1d(seed ^ 0x3EA2, s / 130.0);
        // Le bruit reste souvent loin de ses extrêmes : on l'étire un peu
        let swing = (swing * 1.6).clamp(-1.0, 1.0);
        *w = (MEANDER * fade_near_ends(s, total, 12.0, 140.0) * swing, (-tz / tl, tx / tl));
    }
    // Part de l'écart que le terrain permet, testée tous les 4 points
    let mut allowed = vec![1.0f64; n];
    for k in (1..n - 1).step_by(4) {
        let (offset, (nx, nz)) = wanted[k];
        if offset.abs() < 2.0 {
            continue;
        }
        let p = even[k];
        let base = ground(p);
        allowed[k] = [1.0, 0.6, 0.3, 0.0]
            .into_iter()
            .find(|&f| {
                let q = (p.0 + nx * offset * f, p.1 + nz * offset * f);
                if f == 0.0 {
                    return true;
                }
                let g = ground(q);
                inside.contains(q.0 as i32, q.1 as i32) && g > SEA_LEVEL_BLOCKS + 1.0 && (g - base).abs() <= 8.0
            })
            .unwrap_or(0.0);
        for j in k + 1..(k + 4).min(n - 1) {
            allowed[j] = allowed[k];
        }
    }
    // Lissage du resserrement : un minimum glissant (on reste prudent autour d'un obstacle), puis une moyenne
    let window = 16usize;
    let eroded: Vec<f64> = (0..n).map(|k| allowed[k.saturating_sub(window)..(k + window + 1).min(n)].iter().copied().fold(1.0, f64::min)).collect();
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let slice = &eroded[k.saturating_sub(window)..(k + window + 1).min(n)];
        let f = slice.iter().sum::<f64>() / slice.len() as f64;
        let (offset, (nx, nz)) = wanted[k];
        let p = even[k];
        out.push((p.0 + nx * offset * f, p.1 + nz * offset * f));
    }
    out[0] = even[0];
    out[n - 1] = even[n - 1];
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

impl<'a> Search<'a> {
    /// Recherche dans la zone `area`, qui contourne les établissements qui la touchent
    fn for_area(generator: &'a PlanetWorldGenerator, face: PlanetFace, area: Rect, seed: u64) -> Self {
        let obstacles: Vec<Rect> = generator.structures.touching(generator, face, area).iter().map(|s| s.bounds.expand(SETTLEMENT_CLEARANCE)).collect();
        Search { generator, face, area: area.expand(-4), obstacles, seed, grounds: HashMap::new() }
    }

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

    /// Tracé de `start` à `goal` par des étapes écartées de la ligne droite, une tous les 600 blocs
    /// environ : le chemin décrit de grandes courbes même en plaine. Une étape enclavée est sautée.
    fn with_waypoints(&mut self, start: (i32, i32), goal: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        let seed = self.seed;
        let (dx, dz) = ((goal.0 - start.0) as f64, (goal.1 - start.1) as f64);
        let dist = (dx * dx + dz * dz).sqrt();
        let stops = (dist * STEP as f64 / 600.0).floor() as i32;
        let mut waypoints = Vec::new();
        let start_ground = self.ground_any(start)? as f64;
        let goal_ground = self.ground_any(goal)? as f64;
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
                if self.ground(w).is_some_and(|g| (g as f64 - expected).abs() < 24.0) {
                    waypoints.push(w);
                    break;
                }
            }
        }
        let mut nodes = vec![start];
        for leg in waypoints.into_iter().chain([goal]) {
            let from = *nodes.last()?;
            match self.run(from, leg) {
                Some(mut part) => nodes.extend(part.drain(1..)),
                None if leg != goal => continue,
                None => return None,
            }
        }
        Some(nodes)
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
    fn test_roads_leaving_together_share_a_trunk() {
        let generator = generator(42);
        let roads: Vec<&Road> = PlanetFace::ALL.iter().flat_map(|&f| generator.roads.all_on_face(&generator, f)).collect();
        // Deux chemins d'un même établissement qui partagent un long tronc : les mêmes colonnes
        let mut shared_pairs = 0;
        for (i, r) in roads.iter().enumerate() {
            for s in &roads[i + 1..] {
                if r.face != s.face || ![r.from, r.to].iter().any(|c| *c == s.from || *c == s.to) {
                    continue;
                }
                let common = r.columns.keys().filter(|k| s.columns.contains_key(k)).count();
                if common > 400 {
                    shared_pairs += 1;
                }
            }
        }
        println!("{shared_pairs} paires de chemins avec un tronc commun");
        assert!(shared_pairs >= 3, "seulement {shared_pairs} troncs communs");
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

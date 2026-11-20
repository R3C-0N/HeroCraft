//! La ville-souvenir : la ville en guerre que la vision du tutoriel dessine autour d'un temple
//! (bible, section 8).
//!
//! Dans le passé seulement (`Epoch::Past`), chaque temple se dresse au cœur d'une ville fermée,
//! posée sur le terrain réel, au même endroit que la ruine du présent. Au présent, il n'en reste
//! rien : seul le temple en ruine demeure (choix de Mat, question 38 des notes).
//!
//! La ville est ronde et se traverse en assaut (bible, section 8, « L'ouverture de la vision ») :
//! - **dehors, devant la grande porte**, l'armée se masse (`MemoryCity::muster`) sur un terrain
//!   dégagé (`MemoryCity::field`). La porte du rempart (`MemoryCity::gate`) s'ouvre dans l'axe de
//!   l'entrée du temple : de là, on voit le rayon violet monter du centre. Le jeu la referme
//!   derrière le joueur (écran de fumée) ; le générateur la laisse ouverte. Le rempart est plus
//!   haut que tout le reste, sans autre porte ni échelle ;
//! - **les faubourgs** puis **le cercle des casernes et du marché** : des barricades ennemies
//!   coupent la grande rue de l'axe à plusieurs endroits et forcent les détours par les rues en
//!   anneau et les ruelles. Des places dégagées accueillent les mini-boss
//!   (`MemoryCity::arenas`), où les soldats forment le cercle. Des maisons ont brûlé ;
//! - **la citadelle**, une enceinte crénelée autour du temple, dont la seule porte
//!   (`MemoryCity::citadel_gate`) s'ouvre dans l'axe de l'entrée du temple, sur l'esplanade et le
//!   parvis, puis l'arène du temple où attend le boss du souvenir.
//!
//! Le plan donne aussi les colonnes où poster les ennemis (`MemoryCity::posts`), cercle par cercle.

use super::cell::*;
use super::*;

/// Distance minimale entre le rempart et le bord de la face (le plan reste sur une seule face)
const FACE_MARGIN: i32 = 4;
/// Rayon du rempart extérieur, au plus et au moins
const OUTER_MAX: i32 = 62;
const OUTER_MIN: i32 = 42;
/// Espace entre le parvis du temple et le mur de la citadelle
const ESPLANADE: i32 = 4;

/// Profondeur et demi-largeur du terrain dégagé devant la grande porte, où l'armée se masse
const FIELD_DEPTH: i32 = 22;
const FIELD_HALF: i32 = 16;
/// Rayon des places des mini-boss
const ARENA: i32 = 6;
/// Les deux rangées du mur de la citadelle
const KEEP_WALL: u8 = 21;
const KEEP_WALK: u8 = 22;
/// Intérieur de la citadelle (esplanade et temple)
const COURT: u8 = 23;
/// Barricade de rue
const BARRICADE: u8 = 24;

/// Cercle de la ville où se trouve une colonne
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ring {
    /// Entre le rempart et la palissade
    Outskirts,
    /// Entre la palissade et la citadelle
    Barracks,
    /// Dans la citadelle, autour du temple
    Citadel,
}

/// La ville-souvenir d'un temple, telle que la vision la dessine pour un peuple qui le tient
#[derive(Debug, Clone)]
pub struct MemoryCity {
    /// Le plan, comme celui de toute ville (le temple intact en fait partie)
    pub settlement: Settlement,
    /// Centre de la ville : le centre de l'enceinte du temple
    pub center: (i32, i32),
    /// Rayon du rempart extérieur
    pub outer: i32,
    /// Rayon de la limite entre les faubourgs et le cercle des casernes (une rue en anneau la suit)
    pub middle: i32,
    /// Rayon du mur de la citadelle
    pub citadel: i32,
    /// Où l'armée se masse avant l'assaut, dehors, face à la grande porte
    pub muster: (i32, i32),
    /// Milieu de la grande porte du rempart, dans l'axe de l'entrée du temple
    pub gate: (i32, i32),
    /// Le passage de la grande porte : les colonnes à boucher quand elle se referme
    pub gate_rect: Rect,
    /// Le terrain dégagé devant la grande porte
    pub field: Rect,
    /// Centres des places des mini-boss, de la porte vers le temple
    pub arenas: Vec<(i32, i32)>,
    /// Milieu de la porte de la citadelle, dans l'axe de l'entrée du temple
    pub citadel_gate: (i32, i32),
    /// Colonnes où poster des ennemis (rues, places, chemin de ronde), avec leur cercle
    pub posts: Vec<((i32, i32), Ring)>,
}

impl MemoryCity {
    /// Toutes les colonnes que la ville prend : rien d'autre (établissement, chemin, arbre) ne s'y
    /// dessine dans le passé
    pub fn covers(&self, x: i32, z: i32) -> bool {
        let (dx, dz) = ((x - self.center.0) as f64, (z - self.center.1) as f64);
        (dx * dx + dz * dz).sqrt() <= self.outer as f64 + 1.5 || self.field.contains(x, z)
    }

    /// Cercle de la colonne (x, z), `None` hors du rempart
    pub fn ring(&self, x: i32, z: i32) -> Option<Ring> {
        let d = dist(self.center, x, z);
        if d <= self.citadel as f64 {
            Some(Ring::Citadel)
        } else if d <= self.middle as f64 {
            Some(Ring::Barracks)
        } else if d <= self.outer as f64 {
            Some(Ring::Outskirts)
        } else {
            None
        }
    }
}

fn dist(c: (i32, i32), x: i32, z: i32) -> f64 {
    let (dx, dz) = ((x - c.0) as f64, (z - c.1) as f64);
    (dx * dx + dz * dz).sqrt()
}

/// Rayon du rempart : le plus grand qui reste sur la face et dont tout le tour est au-dessus de
/// l'eau (sinon on passerait le rempart à la nage)
fn outer_radius(generator: &PlanetWorldGenerator, face: PlanetFace, c: (i32, i32)) -> i32 {
    let to_edge = c.0.min(c.1).min(crate::edits::FACE_BLOCKS - 1 - c.0).min(crate::edits::FACE_BLOCKS - 1 - c.1) - FACE_MARGIN - 2;
    let mut r = OUTER_MAX.min(to_edge);
    while r > OUTER_MIN {
        let dry = (0..96).all(|i| {
            let t = std::f64::consts::TAU * i as f64 / 96.0;
            let (x, z) = (c.0 + (r as f64 * t.cos()).round() as i32, c.1 + (r as f64 * t.sin()).round() as i32);
            ground_at(generator, face, x, z).0 as f64 > SEA_LEVEL_BLOCKS + 1.0
        });
        if dry {
            break;
        }
        r -= 4;
    }
    r.max(OUTER_MIN.min(to_edge))
}

/// Dessine la ville-souvenir du temple `temple` (d'un établissement de ruines de `face`), tenue
/// par le peuple `holder`
pub(crate) fn plan_memory_city(generator: &PlanetWorldGenerator, face: PlanetFace, temple: Temple, biome: BiomeType, holder: Culture) -> Option<MemoryCity> {
    use std::f64::consts::{PI, TAU};
    let c = temple.center;
    let (fx, fz) = (c.0 as f64 + 0.5, c.1 as f64 + 0.5);
    let mut dice = Dice(mix(temple.seed ^ 0x5E7E_C17E ^ hash2(0x3E3, Culture::ALL.iter().position(|&k| k == holder).unwrap_or(0) as i32, 0)));
    let outer = outer_radius(generator, face, c);
    // Le parvis descend jusqu'à `radius + STEPS_MAX + PARVIS_DEPTH` du centre
    let citadel = temple.radius + Temple::STEPS_MAX + Temple::PARVIS_DEPTH + ESPLANADE + 2;
    let middle = ((outer as f64 * 0.68).round() as i32).max(citadel + 10);
    let (habits, masonry) = (Habits::of(holder), Masonry::of(holder));
    let street = street_block(holder, biome, true);
    // Le rempart dépasse les toits les plus hauts ; la citadelle est crénelée à la façon du peuple
    let wall = Masonry { height: masonry.height.max(5) + 4, tower_height: masonry.tower_height + 4, palisade: false, ..masonry };
    let keep = Masonry { height: masonry.height + 1, tower_height: masonry.tower_height + 1, ..masonry };
    // Barricades de pieux, trop hautes pour être sautées
    let barricade = Masonry { height: 2, palisade: true, light: masonry.light, ..Masonry::of(Culture::ElfesDesBois) };

    // `front` sort par l'entrée du temple, vers la grande porte
    let (dx, dz) = temple.door().delta();
    let front = (dx as f64, dz as f64);
    let at = |dir: (f64, f64), r: f64| ((fx + dir.0 * r).floor() as i32, (fz + dir.1 * r).floor() as i32);
    let angle = |dir: (f64, f64)| dir.1.atan2(dir.0);
    let a_front = angle(front);
    let off = |t: f64, a: f64| ((t - a + PI).rem_euclid(TAU) - PI).abs();

    let ext = outer + FIELD_DEPTH + 3;
    let n = 2 * ext + 1;
    let mut m = TownMask { x0: c.0 - ext, z0: c.1 - ext, n, cells: vec![OUT; (n * n) as usize] };
    let inside = |r: i32, x: i32, z: i32| dist(c, x, z) <= r as f64;
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    // Le bord d'un disque de rayon r : dedans, avec un voisin dehors
    let edge = |r: i32, x: i32, z: i32| inside(r, x, z) && dirs.iter().any(|&(ox, oz)| !inside(r, x + ox, z + oz));
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            let v = if edge(outer, x, z) {
                WALL
            } else if edge(outer - 1, x, z) {
                WALK
            } else if edge(citadel, x, z) {
                KEEP_WALL
            } else if edge(citadel - 1, x, z) {
                KEEP_WALK
            } else if inside(citadel, x, z) {
                COURT
            } else if inside(outer, x, z) {
                IN
            } else {
                OUT
            };
            m.set(x, z, v);
        }
    }

    // Rues : huit grandes rues de la citadelle au rempart, une rue en anneau de chaque côté de la
    // palissade, une au pied du rempart et une au pied de la citadelle
    // Celles de l'avant, du fond et des côtés restent droites dans l'axe (la porte de la
    // citadelle, le réveil, la brèche) ; les quatre autres dévient un peu
    let mut avenues: Vec<f64> = (0..8).map(|i| a_front + TAU * i as f64 / 8.0).collect();
    avenues.iter_mut().skip(1).step_by(2).for_each(|a| *a += (unit(dice.next()) - 0.5) * 0.18);
    for &a in &avenues {
        let dir = (a.cos(), a.sin());
        let line = wobbly((fx + dir.0 * (citadel as f64 + 1.0), fz + dir.1 * (citadel as f64 + 1.0)), (fx + dir.0 * (outer as f64 - 2.0), fz + dir.1 * (outer as f64 - 2.0)), 3.0, &mut dice);
        m.stroke(&line, 1.6, |v| (v == IN).then_some(ROAD));
    }
    for (r, half) in [(outer as f64 - 4.0, 1.5), (middle as f64 + 2.5, 1.5), (middle as f64 - 2.5, 1.5), (citadel as f64 + 2.5, 1.6)] {
        let steps = (r * TAU / 3.0) as i32;
        let line: Vec<(f64, f64)> = (0..=steps).map(|i| {
            let t = TAU * i as f64 / steps as f64;
            (fx + r * t.cos(), fz + r * t.sin())
        }).collect();
        m.stroke(&line, half, |v| (v == IN).then_some(ROAD));
    }
    // Ruelles entre les anneaux
    for (a, b) in [(citadel as f64 + 3.0, middle as f64 - 3.0), (middle as f64 + 3.0, outer as f64 - 4.0)] {
        let count = ((a + b) * 0.5 * TAU / 30.0).round() as i32;
        for _ in 0..count {
            let t = unit(dice.next()) * TAU;
            if avenues.iter().any(|&g| off(t, g) < 0.12) {
                continue;
            }
            let t2 = t + (unit(dice.next()) - 0.5) * 0.2;
            let line = wobbly((fx + a * t.cos(), fz + a * t.sin()), (fx + b * t2.cos(), fz + b * t2.sin()), 2.5, &mut dice);
            m.stroke(&line, 1.1, |v| (v == IN).then_some(ROAD));
        }
    }

    // La grande porte du rempart, dans l'axe de l'entrée du temple : un passage de cinq de large,
    // et la rue qui en sort vers le terrain où l'armée se masse
    let gate = at(front, outer as f64 - 0.5);
    let outer_gate = Rect::new(gate.0 - 2, gate.1 - 2, gate.0 + 3, gate.1 + 3);
    for z in outer_gate.z0..outer_gate.z1 {
        for x in outer_gate.x0..outer_gate.x1 {
            m.set(x, z, GATEHOUSE);
        }
    }
    let gate_way = wobbly(at(front, outer as f64 - 5.0).into_f(), at(front, outer as f64 + 6.0).into_f(), 0.0, &mut dice);
    m.stroke(&gate_way, 2.4, |v| matches!(v, IN | OUT | ROAD).then_some(ROAD));
    let muster = at(front, (outer + FIELD_DEPTH / 2 + 2) as f64);
    let (fa, fb) = (at(front, outer as f64 + 2.0), at(front, (outer + FIELD_DEPTH) as f64));
    let (px, pz) = (-dz, dx);
    let corners = [(fa.0 + px * FIELD_HALF, fa.1 + pz * FIELD_HALF), (fa.0 - px * FIELD_HALF, fa.1 - pz * FIELD_HALF), (fb.0 + px * FIELD_HALF, fb.1 + pz * FIELD_HALF), (fb.0 - px * FIELD_HALF, fb.1 - pz * FIELD_HALF)];
    let field = Rect::new(
        corners.iter().map(|c| c.0).min().unwrap_or(0),
        corners.iter().map(|c| c.1).min().unwrap_or(0),
        corners.iter().map(|c| c.0).max().unwrap_or(0) + 1,
        corners.iter().map(|c| c.1).max().unwrap_or(0) + 1,
    );

    // La porte de la citadelle, dans l'axe de l'entrée du temple : un passage voûté de trois de large
    let citadel_gate = at(front, citadel as f64 - 0.5);
    let gate_rect = if dx != 0 {
        Rect::new(citadel_gate.0 - 2, citadel_gate.1 - 1, citadel_gate.0 + 3, citadel_gate.1 + 2)
    } else {
        Rect::new(citadel_gate.0 - 1, citadel_gate.1 - 2, citadel_gate.0 + 2, citadel_gate.1 + 3)
    };
    for z in gate_rect.z0..gate_rect.z1 {
        for x in gate_rect.x0..gate_rect.x1 {
            m.set(x, z, GATEHOUSE);
        }
    }
    let approach = wobbly(at(front, citadel as f64 + 1.0).into_f(), at(front, middle as f64 - 2.0).into_f(), 0.0, &mut dice);
    m.stroke(&approach, 1.6, |v| (v == IN).then_some(ROAD));

    // Places des mini-boss : sur les détours, de part et d'autre de l'axe
    let side_sign = if dice.chance(0.5) { 1.0 } else { -1.0 };
    let arenas: Vec<(i32, i32)> = [((outer + middle) as f64 * 0.5, a_front + side_sign * 0.55), (middle as f64 - 2.5, a_front - side_sign * 0.75), ((middle + citadel) as f64 * 0.5 + 1.0, a_front + side_sign * 0.9)]
        .iter()
        .map(|&(r, t)| ((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32))
        .collect();
    for &(ax, az) in &arenas {
        for z in az - ARENA..=az + ARENA {
            for x in ax - ARENA..=ax + ARENA {
                if dist((ax, az), x, z) <= ARENA as f64 && matches!(m.get(x, z), IN | ROAD) {
                    m.set(x, z, SQUARE);
                }
            }
        }
    }
    // Barricades ennemies : la grande rue de l'axe est coupée net en trois endroits (on fait le
    // détour par les rues en anneau) ; les autres grandes rues gardent un passage d'un bloc
    let cut = |m: &mut TownMask, r: f64, a: f64, half: f64, gap: bool| {
        for z in c.1 - ext..=c.1 + ext {
            for x in c.0 - ext..=c.0 + ext {
                let (ux, uz) = (x as f64 + 0.5 - fx, z as f64 + 0.5 - fz);
                let along = ux * a.cos() + uz * a.sin();
                let across = -ux * a.sin() + uz * a.cos();
                if (along - r).abs() <= 0.6 && across.abs() <= half && !(gap && across.abs() < 0.6) && m.get(x, z) == ROAD {
                    m.set(x, z, BARRICADE);
                }
            }
        }
    };
    for r in [outer as f64 - 9.0, middle as f64 + 0.5, citadel as f64 + 7.5] {
        cut(&mut m, r, a_front, 6.0, false);
    }
    for &a in avenues.iter().skip(1) {
        if dice.chance(0.6) {
            cut(&mut m, (middle + outer) as f64 * 0.5 + 2.0, a, 3.5, true);
        }
    }

    let mut p = Planner { generator, face, area: Rect::around(c.0, c.1, ext + 2), pieces: Vec::new(), taken: Vec::new(), exits: Vec::new(), culture: Some(holder) };
    // Le temple, au centre, et tout ce que la citadelle enferme
    let temple_rect = temple.rect();
    p.take(Piece::Temple(temple), 1);
    // L'esplanade, pavée jusqu'au pied du temple : le temple passe avant elle là où il pose
    // quelque chose (il vient le premier dans le plan)
    for rect in m.runs(|v| v == COURT) {
        p.pieces.push(Piece::Paving { rect, block: street });
    }
    // Tours : sur le rempart, hors de la grande porte ; de part et d'autre de la porte de la citadelle
    let a_back = a_front + PI;
    let towers = ((TAU * outer as f64) / 34.0).round() as i32;
    for i in 0..towers {
        let t = a_back + TAU * (i as f64 + 0.5) / towers as f64;
        if off(t, a_front) < 6.0 / outer as f64 {
            continue;
        }
        let r = outer as f64 - 1.0;
        let rect = Rect::around((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32, 2);
        if let Some(floor) = floor_for(generator, face, &rect, 8) {
            p.take(Piece::Tower { rect, floor, masonry: wall }, 0);
        }
    }
    for s in [-1.0, 1.0] {
        let t = a_front + s * 5.0 / citadel as f64;
        let r = citadel as f64 - 1.0;
        let rect = Rect::around((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32, 2);
        if let Some(floor) = floor_for(generator, face, &rect, 8) {
            p.take(Piece::Tower { rect, floor, masonry: keep }, 0);
        }
    }
    p.pieces.push(Piece::Gate { rect: gate_rect, masonry: keep, walk: false });
    for rect in m.runs(|v| v == WALL) {
        p.pieces.push(Piece::Rampart { rect, masonry: wall, walk: false });
    }
    for rect in m.runs(|v| v == WALK) {
        p.pieces.push(Piece::Rampart { rect, masonry: wall, walk: true });
    }
    for rect in m.runs(|v| v == KEEP_WALL) {
        p.pieces.push(Piece::Rampart { rect, masonry: keep, walk: false });
    }
    for rect in m.runs(|v| v == KEEP_WALK) {
        p.pieces.push(Piece::Rampart { rect, masonry: keep, walk: true });
    }
    p.pieces.push(Piece::Gate { rect: outer_gate, masonry: wall, walk: false });
    for rect in m.runs(|v| v == BARRICADE) {
        p.pieces.push(Piece::Rampart { rect, masonry: barricade, walk: false });
    }
    for rect in m.runs(|v| v == ROAD || v == GATEHOUSE || v == SQUARE) {
        p.pieces.push(Piece::Paving { rect, block: street });
    }
    // Les colonnes prises jusqu'ici ne reçoivent pas de maison
    for piece in &p.pieces {
        let r = piece.rect();
        if let Piece::Tower { .. } = piece {
            for z in r.z0..r.z1 {
                for x in r.x0..r.x1 {
                    m.set(x, z, BUILT);
                }
            }
        }
    }

    // Maisons alignées sur les rues, la porte sur la rue. Dans les faubourgs, une sur trois a brûlé
    // et s'est effondrée ; une sur huit dans le cercle du milieu
    let mut fronts: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            if m.get(x, z) != ROAD {
                continue;
            }
            for &(ox, oz) in &dirs {
                if m.get(x + ox, z + oz) == IN {
                    fronts.push(((x, z), (ox, oz)));
                }
            }
        }
    }
    shuffle(&mut dice, &mut fronts);
    let mut houses = 0;
    for ((rx, rz), (ox, oz)) in fronts {
        if m.get(rx + ox, rz + oz) != IN {
            continue;
        }
        let door = match (ox, oz) {
            (1, _) => Side::West,
            (-1, _) => Side::East,
            (_, 1) => Side::North,
            _ => Side::South,
        };
        let (w, d) = (dice.range(5, 9), dice.range(4, 8));
        let shift = dice.range(-1, 2);
        let foot = match door {
            Side::West => Rect::sized(rx + 1, rz - w / 2 + shift, d, w),
            Side::East => Rect::sized(rx - d, rz - w / 2 + shift, d, w),
            Side::North => Rect::sized(rx - w / 2 + shift, rz + 1, w, d),
            Side::South => Rect::sized(rx - w / 2 + shift, rz - d, w, d),
        };
        let fits = (foot.z0..foot.z1).all(|z| (foot.x0..foot.x1).all(|x| m.get(x, z) == IN));
        let clear = (foot.z0 - 1..foot.z1 + 1).all(|z| (foot.x0 - 1..foot.x1 + 1).all(|x| matches!(m.get(x, z), IN | ROAD | BUILT | EAVES)));
        if !fits || !clear {
            continue;
        }
        let (hx, hz) = foot.center();
        let outskirts = dist(c, hx, hz) > middle as f64;
        let burnt = dice.chance(if outskirts { 0.33 } else { 0.12 });
        let used = if burnt {
            let Some(floor) = floor_for(generator, face, &foot, 5) else { continue };
            let ruin = RuinHouse { foot, floor, height: dice.range(2, 5), seed: dice.next() };
            p.take(Piece::RuinHouse(ruin), 0);
            foot
        } else {
            let style = Style::town(holder, biome, &mut dice);
            let storeys = if dice.chance(habits.upstairs.1) { 2 } else { 1 };
            let house = House { foot, door, ..random_home(&mut dice, storeys, style, 0.0, habits.hip) };
            let before = p.pieces.len();
            if !p.try_building_spaced(house, 0) {
                continue;
            }
            p.pieces[before].rect()
        };
        for z in used.z0..used.z1 {
            for x in used.x0..used.x1 {
                if foot.contains(x, z) {
                    m.set(x, z, BUILT);
                } else if m.get(x, z) == IN {
                    m.set(x, z, EAVES);
                }
            }
        }
        houses += 1;
    }
    // Arrière-cours : de petites maisons derrière les premières, la porte vers la rue la plus proche
    let mut spots: Vec<(i32, i32)> = Vec::new();
    for z in (c.1 - ext..=c.1 + ext).step_by(3) {
        for x in (c.0 - ext..=c.0 + ext).step_by(3) {
            if m.get(x, z) == IN {
                spots.push((x, z));
            }
        }
    }
    shuffle(&mut dice, &mut spots);
    for (x, z) in spots {
        if m.get(x, z) != IN {
            continue;
        }
        let Some((ox, oz)) = dirs.iter().filter_map(|&(ox, oz)| (1..12).find(|&k| m.get(x + ox * k, z + oz * k) == ROAD).map(|k| (k, (ox, oz)))).min_by_key(|e| e.0).map(|e| e.1) else {
            continue;
        };
        let door = match (ox, oz) {
            (1, _) => Side::East,
            (-1, _) => Side::West,
            (_, 1) => Side::South,
            _ => Side::North,
        };
        let (w, d) = (dice.range(4, 7), dice.range(4, 7));
        let foot = Rect::sized(x - w / 2, z - d / 2, w, d);
        let fits = (foot.z0..foot.z1).all(|z| (foot.x0..foot.x1).all(|x| m.get(x, z) == IN));
        let clear = (foot.z0 - 1..foot.z1 + 1).all(|z| (foot.x0 - 1..foot.x1 + 1).all(|x| matches!(m.get(x, z), IN | ROAD | BUILT | EAVES)));
        if !fits || !clear {
            continue;
        }
        let (hx, hz) = foot.center();
        let burnt = dice.chance(if dist(c, hx, hz) > middle as f64 { 0.33 } else { 0.12 });
        let used = if burnt {
            let Some(floor) = floor_for(generator, face, &foot, 5) else { continue };
            p.take(Piece::RuinHouse(RuinHouse { foot, floor, height: dice.range(2, 5), seed: dice.next() }), 0);
            foot
        } else {
            let style = Style::town(holder, biome, &mut dice);
            let house = House { foot, door, ..random_home(&mut dice, 1, style, 0.0, habits.hip) };
            let before = p.pieces.len();
            if !p.try_building_spaced(house, 0) {
                continue;
            }
            p.pieces[before].rect()
        };
        for z in used.z0..used.z1 {
            for x in used.x0..used.x1 {
                if foot.contains(x, z) {
                    m.set(x, z, BUILT);
                } else if m.get(x, z) == IN {
                    m.set(x, z, EAVES);
                }
            }
        }
        houses += 1;
    }
    if houses < 12 {
        return None;
    }
    // Lampadaires de loin en loin, seulement dans le cercle du milieu (les faubourgs sont dans le noir)
    let seed = dice.next();
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            if m.get(x, z) == ROAD && dist(c, x, z) < middle as f64 - 1.0 && hash2(seed, x, z).is_multiple_of(36) && dirs.iter().any(|&(ox, oz)| matches!(m.get(x + ox, z + oz), IN | BUILT)) {
                p.pieces.push(Piece::Lamp { x, z, masonry });
            }
        }
    }

    // Postes des ennemis : dans les rues, de loin en loin, et sur les chemins de ronde
    let mut posts = Vec::new();
    let seed = dice.next();
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            let v = m.get(x, z);
            let d = dist(c, x, z);
            let ring = if d <= citadel as f64 { Ring::Citadel } else if d <= middle as f64 { Ring::Barracks } else { Ring::Outskirts };
            let spot = match v {
                ROAD | SQUARE => dist(c, x, z) < outer as f64 - 1.5 && hash2(seed, x, z).is_multiple_of(70),
                COURT => !temple_rect.contains(x, z) && hash2(seed, x, z).is_multiple_of(55),
                WALK | KEEP_WALK => hash2(seed ^ 1, x, z).is_multiple_of(24),
                _ => false,
            };
            // Personne juste derrière la grande porte : l'armée doit pouvoir y entrer
            if spot && dist(gate, x, z) > 10.0 {
                posts.push(((x, z), ring));
            }
        }
    }

    // Le plus haut bloc : celui d'un bâtiment, ou le haut du rempart, qui suit le terrain
    let mut high = i32::MIN;
    for z in (c.1 - outer..=c.1 + outer).step_by(5) {
        for x in (c.0 - outer..=c.0 + outer).step_by(5) {
            if dist(c, x, z) <= outer as f64 + 1.0 {
                high = high.max(ground_at(generator, face, x, z).0);
            }
        }
    }
    let wall_top = high + wall.height + 4;
    let mut settlement = p.finish(SettlementKind::Ruins, c, biome)?;
    settlement.top = settlement.top.max(wall_top);
    // Le rempart borne le plan : les rues qui le touchent n'en sortent pas
    settlement.bounds = Rect::around(c.0, c.1, outer + 2).union(&settlement.bounds).union(&field);
    Some(MemoryCity { settlement, center: c, outer, middle, citadel, muster, gate, gate_rect: outer_gate, field, arenas, citadel_gate, posts })
}

trait IntoF {
    fn into_f(self) -> (f64, f64);
}

impl IntoF for (i32, i32) {
    fn into_f(self) -> (f64, f64) {
        (self.0 as f64 + 0.5, self.1 as f64 + 0.5)
    }
}

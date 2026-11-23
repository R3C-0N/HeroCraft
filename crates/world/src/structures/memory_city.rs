//! La ville-souvenir : la ville en guerre que la vision du tutoriel dessine autour d'un temple
//! (bible, section 8).
//!
//! Dans le passé seulement (`Epoch::Past`), chaque temple se dresse au cœur d'une grande ville
//! fermée, posée sur le terrain réel, au même endroit que la ruine du présent. Au présent, il n'en
//! reste rien : seul le temple en ruine demeure (choix de Mat, question 38 des notes).
//!
//! La ville est ronde, serrée, et se traverse en assaut (bible, section 8, « L'ouverture de la
//! vision ») :
//! - **dehors, devant la grande porte**, l'armée se masse (`MemoryCity::muster`) sur un terrain
//!   dégagé (`MemoryCity::field`). La porte du rempart (`MemoryCity::gate`) s'ouvre dans l'axe de
//!   l'entrée du temple. Le jeu la ferme, le bélier de la charge l'enfonce et une herse tombe
//!   derrière le joueur (`vision_charge`, côté client) ; le générateur la laisse ouverte. Le rempart est plus haut que tout le reste, sans autre porte ni échelle ;
//! - **la ville basse**, entre le rempart et une seconde muraille : des maisons pauvres, petites,
//!   serrées les unes contre les autres, beaucoup brûlées ;
//! - **la seconde muraille** est fermée du côté de la grande porte : sa seule porte
//!   (`MemoryCity::inner_gate`) est à l'opposé. Le chemin est ainsi tracé naturellement : de la
//!   grande porte, on fait le tour de la muraille par la rue qui la longe ;
//! - **la ville haute**, derrière la seconde muraille : des maisons riches, plus grandes, à étage,
//!   jusqu'à l'esplanade du temple (sans mur) puis le parvis et l'arène du temple, où attend le
//!   boss du souvenir.
//!
//! Des barricades ennemies coupent les rues et forcent les détours ; des places dégagées
//! accueillent les mini-boss (`MemoryCity::arenas`). Le plan donne aussi les colonnes où poster
//! les ennemis (`MemoryCity::posts`), quartier par quartier.

use super::cell::*;
use super::*;

/// Distance minimale entre le rempart et le bord de la face (le plan reste sur une seule face)
const FACE_MARGIN: i32 = 4;
/// Rayon du rempart extérieur, au plus et au moins
const OUTER_MAX: i32 = 90;
const OUTER_MIN: i32 = 50;
/// Espace entre le parvis du temple et le bord de l'esplanade
const ESPLANADE: i32 = 4;
/// Profondeur et demi-largeur du terrain dégagé devant la grande porte, où l'armée se masse
const FIELD_DEPTH: i32 = 22;
const FIELD_HALF: i32 = 16;
/// Rayon des places des mini-boss
const ARENA: i32 = 6;

/// Les deux rangées de la seconde muraille
const INNER_WALL: u8 = 21;
const INNER_WALK: u8 = 22;
/// L'esplanade du temple
const COURT: u8 = 23;
/// Barricade de rue
const BARRICADE: u8 = 24;

/// Quartier de la ville où se trouve une colonne
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ring {
    /// La ville basse, entre le rempart et la seconde muraille : les maisons pauvres
    Lower,
    /// La ville haute, derrière la seconde muraille : les maisons riches
    Upper,
    /// L'esplanade du temple
    Temple,
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
    /// Rayon de la seconde muraille, entre la ville basse et la ville haute
    pub inner: i32,
    /// Rayon de l'esplanade du temple
    pub court: i32,
    /// Où l'armée se masse avant l'assaut, dehors, face à la grande porte
    pub muster: (i32, i32),
    /// Milieu de la grande porte du rempart, dans l'axe de l'entrée du temple
    pub gate: (i32, i32),
    /// Le passage de la grande porte : les colonnes à boucher quand elle se referme
    pub gate_rect: Rect,
    /// Le terrain dégagé devant la grande porte
    pub field: Rect,
    /// Milieu de la seule porte de la seconde muraille, à l'opposé de la grande porte
    pub inner_gate: (i32, i32),
    /// Centres des places des mini-boss, de la grande porte vers le temple
    pub arenas: Vec<(i32, i32)>,
    /// Colonnes où poster des ennemis (rues, places, chemins de ronde), avec leur quartier
    pub posts: Vec<((i32, i32), Ring)>,
}

impl MemoryCity {
    /// Toutes les colonnes que la ville prend : rien d'autre (établissement, chemin, arbre) ne s'y
    /// dessine dans le passé
    pub fn covers(&self, x: i32, z: i32) -> bool {
        dist(self.center, x, z) <= self.outer as f64 + 1.5 || self.field.contains(x, z)
    }

    /// Quartier de la colonne (x, z), `None` hors du rempart
    pub fn ring(&self, x: i32, z: i32) -> Option<Ring> {
        let d = dist(self.center, x, z);
        if d <= self.court as f64 {
            Some(Ring::Temple)
        } else if d <= self.inner as f64 {
            Some(Ring::Upper)
        } else if d <= self.outer as f64 {
            Some(Ring::Lower)
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
    let to_edge = c.0.min(c.1).min(crate::edits::FACE_BLOCKS - 1 - c.0).min(crate::edits::FACE_BLOCKS - 1 - c.1) - FACE_MARGIN - FIELD_DEPTH - 2;
    let mut r = OUTER_MAX.min(to_edge);
    while r > OUTER_MIN {
        let dry = (0..128).all(|i| {
            let t = std::f64::consts::TAU * i as f64 / 128.0;
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
    let court = temple.radius + Temple::STEPS_MAX + Temple::PARVIS_DEPTH + ESPLANADE;
    let inner = ((outer as f64 * 0.58).round() as i32).max(court + 18);
    let (habits, masonry) = (Habits::of(holder), Masonry::of(holder));
    let street = street_block(holder, biome, true);
    // Le rempart dépasse les toits les plus hauts ; la seconde muraille est crénelée à la façon du peuple
    let wall = Masonry { height: masonry.height.max(5) + 5, tower_height: masonry.tower_height + 5, palisade: false, ..masonry };
    let keep = Masonry { height: masonry.height.max(5) + 3, tower_height: masonry.tower_height + 3, palisade: false, ..masonry };
    // Barricades de pieux, trop hautes pour être sautées
    let barricade = Masonry { height: 2, palisade: true, light: masonry.light, ..Masonry::of(Culture::ElfesDesBois) };

    // `front` sort par l'entrée du temple, vers la grande porte
    let (dx, dz) = temple.door().delta();
    let front = (dx as f64, dz as f64);
    let at = |dir: (f64, f64), r: f64| ((fx + dir.0 * r).floor() as i32, (fz + dir.1 * r).floor() as i32);
    let a_front = front.1.atan2(front.0);
    let a_back = a_front + PI;
    let off = |t: f64, a: f64| ((t - a + PI).rem_euclid(TAU) - PI).abs();
    let polar = |r: f64, t: f64| ((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32);

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
            } else if edge(inner, x, z) {
                INNER_WALL
            } else if edge(inner - 1, x, z) {
                INNER_WALK
            } else if inside(court, x, z) {
                COURT
            } else if inside(outer, x, z) {
                IN
            } else {
                OUT
            };
            m.set(x, z, v);
        }
    }

    // Rues en anneau : au pied du rempart, de chaque côté de la seconde muraille (celle du dehors
    // est le chemin de l'assaut) et autour de l'esplanade
    for (r, half) in [(outer as f64 - 4.0, 1.5), (inner as f64 + 3.0, 2.0), (inner as f64 - 3.5, 1.5), (court as f64 + 2.0, 1.6)] {
        let steps = (r * TAU / 3.0) as i32;
        let line: Vec<(f64, f64)> = (0..=steps).map(|i| {
            let t = TAU * i as f64 / steps as f64;
            (fx + r * t.cos(), fz + r * t.sin())
        }).collect();
        m.stroke(&line, half, |v| (v == IN).then_some(ROAD));
    }
    // Grandes rues : dix dans la ville basse, de la seconde muraille au rempart (celle de l'axe part
    // de la grande porte) ; six dans la ville haute, de l'esplanade à la muraille (celle du fond
    // part de la porte de la seconde muraille)
    let mut lower: Vec<f64> = (0..10).map(|i| a_front + TAU * i as f64 / 10.0).collect();
    lower.iter_mut().skip(1).for_each(|a| *a += (unit(dice.next()) - 0.5) * 0.12);
    for &a in &lower {
        let line = wobbly(polar(inner as f64 + 3.0, a).into_f(), polar(outer as f64 - 2.0, a).into_f(), if a == a_front { 0.0 } else { 2.5 }, &mut dice);
        m.stroke(&line, if a == a_front { 2.2 } else { 1.4 }, |v| (v == IN).then_some(ROAD));
    }
    let mut upper: Vec<f64> = (0..6).map(|i| a_back + TAU * i as f64 / 6.0).collect();
    upper.iter_mut().skip(1).for_each(|a| *a += (unit(dice.next()) - 0.5) * 0.15);
    for &a in &upper {
        let line = wobbly(polar(court as f64 + 1.0, a).into_f(), polar(inner as f64 - 2.0, a).into_f(), if a == a_back { 0.0 } else { 2.0 }, &mut dice);
        m.stroke(&line, if a == a_back { 2.0 } else { 1.4 }, |v| (v == IN).then_some(ROAD));
    }
    // Ruelles étroites dans la ville basse
    let (a, b) = (inner as f64 + 5.0, outer as f64 - 6.0);
    for _ in 0..((a + b) * 0.5 * TAU / 26.0).round() as i32 {
        let t = unit(dice.next()) * TAU;
        if lower.iter().any(|&g| off(t, g) < 0.1) {
            continue;
        }
        let t2 = t + (unit(dice.next()) - 0.5) * 0.25;
        let line = wobbly(polar(a, t).into_f(), polar(b, t2).into_f(), 2.0, &mut dice);
        m.stroke(&line, 1.0, |v| (v == IN).then_some(ROAD));
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

    // La porte de la seconde muraille, à l'opposé de la grande porte : un passage voûté de trois de large
    let back = (-front.0, -front.1);
    let inner_gate = at(back, inner as f64 - 0.5);
    let inner_rect = if dx != 0 {
        Rect::new(inner_gate.0 - 2, inner_gate.1 - 1, inner_gate.0 + 3, inner_gate.1 + 2)
    } else {
        Rect::new(inner_gate.0 - 1, inner_gate.1 - 2, inner_gate.0 + 2, inner_gate.1 + 3)
    };
    for z in inner_rect.z0..inner_rect.z1 {
        for x in inner_rect.x0..inner_rect.x1 {
            m.set(x, z, GATEHOUSE);
        }
    }
    let through = wobbly(at(back, inner as f64 + 3.0).into_f(), at(back, inner as f64 - 4.0).into_f(), 0.0, &mut dice);
    m.stroke(&through, 1.6, |v| matches!(v, IN | ROAD).then_some(ROAD));

    // Places des mini-boss : sur la rue qui longe la seconde muraille, d'un côté du tour ; devant la
    // porte de la seconde muraille ; dans la ville haute, avant l'esplanade
    let side_sign = if dice.chance(0.5) { 1.0 } else { -1.0 };
    let arenas: Vec<(i32, i32)> = [
        polar(inner as f64 + 3.0 + ARENA as f64 - 1.0, a_front + side_sign * 1.4),
        polar(inner as f64 + 3.0 + ARENA as f64 - 1.0, a_back - side_sign * 0.35),
        polar((inner + court) as f64 * 0.5, a_front + side_sign * 0.6),
    ]
    .to_vec();
    for &(ax, az) in &arenas {
        for z in az - ARENA..=az + ARENA {
            for x in ax - ARENA..=ax + ARENA {
                if dist((ax, az), x, z) <= ARENA as f64 && matches!(m.get(x, z), IN | ROAD) {
                    m.set(x, z, SQUARE);
                }
            }
        }
    }
    // Barricades ennemies, coupées net en travers des rues
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
    // La grande rue de l'axe : barrée avant la seconde muraille (on prend la rue qui la longe)
    cut(&mut m, (outer + inner) as f64 * 0.5, a_front, 6.0, false);
    // Les autres grandes rues gardent un passage d'un bloc
    for &a in lower.iter().skip(1) {
        if dice.chance(0.5) {
            cut(&mut m, (inner + outer) as f64 * 0.5 + 3.0, a, 3.5, true);
        }
    }

    let mut p = Planner { generator, face, area: Rect::around(c.0, c.1, ext + 2), pieces: Vec::new(), taken: Vec::new(), exits: Vec::new(), culture: Some(holder) };
    // Le temple, au centre ; l'esplanade, pavée jusqu'à son pied : le temple passe avant elle là
    // où il pose quelque chose (il vient le premier dans le plan)
    let temple_rect = temple.rect();
    p.take(Piece::Temple(temple), 1);
    for rect in m.runs(|v| v == COURT) {
        p.pieces.push(Piece::Paving { rect, block: street });
    }
    // Tours : sur le rempart, hors de la grande porte ; de part et d'autre des deux portes
    let towers = ((TAU * outer as f64) / 34.0).round() as i32;
    for i in 0..towers {
        let t = a_back + TAU * (i as f64 + 0.5) / towers as f64;
        if off(t, a_front) < 7.0 / outer as f64 {
            continue;
        }
        let rect = Rect::around(polar(outer as f64 - 1.0, t).0, polar(outer as f64 - 1.0, t).1, 2);
        if let Some(floor) = floor_for(generator, face, &rect, 8) {
            p.take(Piece::Tower { rect, floor, masonry: wall }, 0);
        }
    }
    for (r, a, m_) in [(outer as f64 - 1.0, a_front, wall), (inner as f64 - 1.0, a_back, keep)] {
        for s in [-1.0, 1.0] {
            let (x, z) = polar(r, a + s * 5.5 / r);
            let rect = Rect::around(x, z, 2);
            if let Some(floor) = floor_for(generator, face, &rect, 8) {
                p.take(Piece::Tower { rect, floor, masonry: m_ }, 0);
            }
        }
    }
    p.pieces.push(Piece::Gate { rect: outer_gate, masonry: wall, walk: false });
    p.pieces.push(Piece::Gate { rect: inner_rect, masonry: keep, walk: false });
    for rect in m.runs(|v| v == WALL) {
        p.pieces.push(Piece::Rampart { rect, masonry: wall, walk: false });
    }
    for rect in m.runs(|v| v == WALK) {
        p.pieces.push(Piece::Rampart { rect, masonry: wall, walk: true });
    }
    for rect in m.runs(|v| v == INNER_WALL) {
        p.pieces.push(Piece::Rampart { rect, masonry: keep, walk: false });
    }
    for rect in m.runs(|v| v == INNER_WALK) {
        p.pieces.push(Piece::Rampart { rect, masonry: keep, walk: true });
    }
    for rect in m.runs(|v| v == BARRICADE) {
        p.pieces.push(Piece::Rampart { rect, masonry: barricade, walk: false });
    }
    for rect in m.runs(|v| v == ROAD || v == GATEHOUSE || v == SQUARE) {
        p.pieces.push(Piece::Paving { rect, block: street });
    }
    for piece in &p.pieces {
        if let Piece::Tower { rect: r, .. } = piece {
            for z in r.z0..r.z1 {
                for x in r.x0..r.x1 {
                    m.set(x, z, BUILT);
                }
            }
        }
    }

    // Maisons, serrées les unes contre les autres, la porte sur la rue : petites et pauvres dans la
    // ville basse (une sur trois brûlée), grandes et riches, à étage, dans la ville haute
    let mut houses = 0;
    let mut place = |m: &mut TownMask, p: &mut Planner, dice: &mut Dice, foot: Rect, door: Side| -> bool {
        let fits = (foot.z0..foot.z1).all(|z| (foot.x0..foot.x1).all(|x| m.get(x, z) == IN));
        let clear = (foot.z0 - 1..foot.z1 + 1).all(|z| (foot.x0 - 1..foot.x1 + 1).all(|x| matches!(m.get(x, z), IN | ROAD | SQUARE | BUILT | EAVES)));
        if !fits || !clear {
            return false;
        }
        let (hx, hz) = foot.center();
        let rich = dist(c, hx, hz) < inner as f64;
        let used = if dice.chance(if rich { 0.02 } else { 0.05 }) {
            let Some(floor) = floor_for(generator, face, &foot, 5) else { return false };
            p.take(Piece::RuinHouse(RuinHouse { foot, floor, height: dice.range(2, 5), seed: dice.next() }), 0);
            foot
        } else {
            let (style, storeys) = if rich {
                (Style::town(holder, biome, dice), if dice.chance(0.8) { 2 } else { 1 })
            } else {
                (Style::village(holder, biome, dice), 1)
            };
            let house = House { foot, door, ..random_home(dice, storeys, style, 0.0, habits.hip) };
            let before = p.pieces.len();
            if !p.try_building_spaced(house, 0) {
                return false;
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
        true
    };
    // D'abord le long des rues
    let mut fronts: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            if !matches!(m.get(x, z), ROAD | SQUARE) {
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
        let rich = dist(c, rx, rz) < inner as f64;
        let (w, d) = if rich { (dice.range(6, 11), dice.range(6, 10)) } else { (dice.range(4, 7), dice.range(4, 7)) };
        let shift = dice.range(-1, 2);
        let foot = match door {
            Side::West => Rect::sized(rx + 1, rz - w / 2 + shift, d, w),
            Side::East => Rect::sized(rx - d, rz - w / 2 + shift, d, w),
            Side::North => Rect::sized(rx - w / 2 + shift, rz + 1, w, d),
            Side::South => Rect::sized(rx - w / 2 + shift, rz - d, w, d),
        };
        place(&mut m, &mut p, &mut dice, foot, door);
    }
    // Puis les arrière-cours, jusqu'à remplir les îlots : la porte vers la rue la plus proche
    let mut spots: Vec<(i32, i32)> = Vec::new();
    for z in (c.1 - ext..=c.1 + ext).step_by(2) {
        for x in (c.0 - ext..=c.0 + ext).step_by(2) {
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
        let Some((ox, oz)) = dirs.iter().filter_map(|&(ox, oz)| (1..12).find(|&k| matches!(m.get(x + ox * k, z + oz * k), ROAD | SQUARE)).map(|k| (k, (ox, oz)))).min_by_key(|e| e.0).map(|e| e.1) else {
            continue;
        };
        let door = match (ox, oz) {
            (1, _) => Side::East,
            (-1, _) => Side::West,
            (_, 1) => Side::South,
            _ => Side::North,
        };
        let rich = dist(c, x, z) < inner as f64;
        for (w, d) in if rich { [(8, 7), (6, 6), (5, 5)] } else { [(5, 5), (4, 5), (4, 4)] } {
            if place(&mut m, &mut p, &mut dice, Rect::sized(x - w / 2, z - d / 2, w, d), door) {
                break;
            }
        }
    }
    if houses < 40 {
        return None;
    }
    // Lampadaires de loin en loin, dans la ville haute seulement (la ville basse est dans le noir)
    let seed = dice.next();
    for z in c.1 - ext..=c.1 + ext {
        for x in c.0 - ext..=c.0 + ext {
            if m.get(x, z) == ROAD && dist(c, x, z) < inner as f64 - 1.0 && hash2(seed, x, z).is_multiple_of(30) && dirs.iter().any(|&(ox, oz)| matches!(m.get(x + ox, z + oz), IN | BUILT | EAVES)) {
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
            let ring = if d <= court as f64 { Ring::Temple } else if d <= inner as f64 { Ring::Upper } else { Ring::Lower };
            let spot = match v {
                ROAD | SQUARE => d < outer as f64 - 1.5 && hash2(seed, x, z).is_multiple_of(80),
                COURT => !temple_rect.contains(x, z) && hash2(seed, x, z).is_multiple_of(55),
                WALK | INNER_WALK => hash2(seed ^ 1, x, z).is_multiple_of(28),
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
    let wall_top = high + wall.tower_height + 6;
    let mut settlement = p.finish(SettlementKind::Ruins, c, biome)?;
    settlement.top = settlement.top.max(wall_top);
    // Le rempart borne le plan, avec le terrain de l'assaut
    settlement.bounds = Rect::around(c.0, c.1, outer + 2).union(&settlement.bounds).union(&field);
    Some(MemoryCity { settlement, center: c, outer, inner, court, muster, gate, gate_rect: outer_gate, field, inner_gate, arenas, posts })
}

trait IntoF {
    fn into_f(self) -> (f64, f64);
}

impl IntoF for (i32, i32) {
    fn into_f(self) -> (f64, f64) {
        (self.0 as f64 + 0.5, self.1 as f64 + 0.5)
    }
}

//! La charge de la vision (bible, section 8, « La charge ») : après l'éveil, le temps repart sur
//! le cri du capitaine et l'armée court vers la grande porte, fermée. Un bélier, sous son abri de
//! planches, la frappe jusqu'à ce qu'elle vole en éclats ; un énorme rayon violet s'élève alors du
//! centre de la ville. Le joueur entre avec les soldats, par ses propres pas (aucun écran de
//! fumée : on le garde pour la sortie du souvenir), et une herse s'abat derrière lui : la ville
//! ne se quitte plus.
//!
//! Tout se joue dans les blocs de la vision, chez le joueur (la porte, le bélier, la herse) ; en
//! solo, les soldats du tutoriel courent avec lui. Ce module ne touche à rien : il dit à la boucle
//! du jeu ce qui arrive (`ChargeEvent`), qu'elle applique.

use glam::Vec3;
use herocraft_common::{BlockId, PlanetFace};
use herocraft_world::{BlockGeometry, BlockPos, MemoryCity, PlanetWorldGenerator};

/// Demi-largeur du passage de la grande porte (cinq blocs de large et de profondeur)
const GATE_HALF: i32 = 2;
/// Hauteur du passage, en blocs
const GATE_HEIGHT: i32 = 3;
/// Le bélier commence à frapper quand le joueur est à moins de tant de blocs de la porte…
const RAM_START_BLOCKS: f32 = 18.0;
/// … ou au plus tard tant de secondes après le cri de la charge
const RAM_START_SECONDS: f32 = 12.0;
/// Coups de bélier qu'il faut à la porte
const RAM_HITS: u32 = 4;
/// Secondes entre deux coups
const RAM_INTERVAL: f32 = 1.9;
/// Secondes que le bélier reste contre la porte après un coup
const RAM_CONTACT: f32 = 0.35;
/// Le joueur est entré dans la ville quand il a passé la porte de tant de blocs
const INSIDE_BLOCKS: i32 = 4;
/// Secondes d'affichage de la consigne de la herse
const SEALED_HINT_SECONDS: f32 = 7.0;
/// Allure des soldats qui chargent, par rapport à leur marche
const CHARGE_PACE: f32 = 2.2;

/// Ce que la boucle du jeu doit faire
#[derive(Debug, Clone, PartialEq)]
pub enum ChargeEvent {
    /// Changer ces blocs de la vision
    Blocks(Vec<(BlockPos, BlockId)>),
    /// Envoyer ces soldats (solo) vers ces points (direction depuis le centre de la planète), à
    /// cette allure
    March(Vec<(herocraft_common::CreatureId, Vec3, f32)>),
    /// Un coup de bélier contre la porte (son, secousse, poussière), au point de l'impact
    RamHit(Vec3),
    /// La porte vole en éclats, en ce point
    GateBroken(Vec3),
    /// Le rayon violet s'élève du centre de la ville, en ce point (au sol)
    Beam(Vec3),
    /// La herse s'abat derrière le joueur, en ce point
    Sealed(Vec3),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Le tutoriel de l'éclat n'est pas fini : la porte attend, fermée
    Waiting,
    /// L'armée court vers la porte
    Charging,
    /// Le bélier frappe
    Ramming,
    /// La porte a cédé : on entre
    Breached,
    /// Le joueur est dans la ville, la herse est tombée
    Sealed,
}

/// Le plan de l'assaut, tiré de la ville-souvenir : la porte, l'axe qui sort de la ville, le sol
#[derive(Debug, Clone, Copy)]
pub struct Siege {
    face: PlanetFace,
    gate: (i32, i32),
    /// Axe qui sort de la ville par la porte (un des quatre axes de la face)
    out: (i32, i32),
    /// Premier niveau d'air du passage
    floor: i32,
    center: (i32, i32),
    center_floor: i32,
}

impl Siege {
    pub fn of(city: &MemoryCity, face: PlanetFace, generator: &PlanetWorldGenerator) -> Self {
        let (gx, gz) = city.gate;
        let (dx, dz) = (city.muster.0 - gx, city.muster.1 - gz);
        let out = if dx.abs() >= dz.abs() { (dx.signum(), 0) } else { (0, dz.signum()) };
        let floor_at = |x: i32, z: i32| {
            let ground = generator.sample_block_column(face, x, z).elevation.floor() as i32;
            (ground - 4..ground + 40).find(|&y| generator.block_at(face, x, y - 1, z).is_solid() && !generator.block_at(face, x, y, z).is_solid()).unwrap_or(ground + 1)
        };
        Self { face, gate: (gx, gz), out, floor: floor_at(gx, gz), center: city.center, center_floor: floor_at(city.center.0, city.center.1) }
    }

    /// Colonne à `along` blocs de la porte vers l'extérieur et `side` blocs sur le côté
    fn column(&self, along: i32, side: i32) -> (i32, i32) {
        let (ox, oz) = self.out;
        let (lx, lz) = (-oz, ox);
        (self.gate.0 + ox * along + lx * side, self.gate.1 + oz * along + lz * side)
    }

    fn pos(&self, along: i32, side: i32, level: i32) -> BlockPos {
        let (x, z) = self.column(along, side);
        BlockPos::new(self.face, x, self.floor + level, z)
    }

    /// Où en est la colonne (x, z) le long de l'axe de la porte (positif dehors) et sur le côté
    fn local(&self, x: i32, z: i32) -> (i32, i32) {
        let (ox, oz) = self.out;
        let (dx, dz) = (x - self.gate.0, z - self.gate.1);
        (dx * ox + dz * oz, dx * -oz + dz * ox)
    }

    fn point(&self, geometry: &BlockGeometry, along: f32, side: f32, level: f32) -> Vec3 {
        let (ox, oz) = (self.out.0 as f32, self.out.1 as f32);
        let (lx, lz) = (-oz, ox);
        let x = self.gate.0 as f32 + 0.5 + ox * along + lx * side;
        let z = self.gate.1 as f32 + 0.5 + oz * along + lz * side;
        geometry.corner(self.face, x as f64, (self.floor as f32 + level) as f64, z as f64).as_vec3()
    }
}

fn block(id: &str) -> BlockId {
    herocraft_common::blocks().find(id).unwrap_or(BlockId::AIR)
}

pub struct VisionCharge {
    siege: Siege,
    geometry: BlockGeometry,
    phase: Phase,
    /// Secondes depuis le début de la phase
    timer: f32,
    hits: u32,
    /// Le bélier est contre la porte
    ram_forward: bool,
    soldiers: Vec<herocraft_common::CreatureId>,
}

impl VisionCharge {
    pub fn new(siege: Siege, geometry: BlockGeometry) -> Self {
        Self { siege, geometry, phase: Phase::Waiting, timer: 0.0, hits: 0, ram_forward: false, soldiers: Vec::new() }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Cap (0 : nord, π/2 : est, comme celui des créatures) qui regarde la porte depuis `from`
    pub fn facing_gate(&self, from: Vec3) -> f32 {
        let up = from.normalize_or_zero();
        let (east, north) = herocraft_common::tangent_frame(up);
        let to = self.siege.point(&self.geometry, GATE_HALF as f32, 0.0, 0.0) - from;
        to.dot(east).atan2(to.dot(north)).rem_euclid(std::f32::consts::TAU)
    }

    /// La porte fermée (des planches dans tout le passage) et le bélier qui l'attend, sous son abri
    pub fn initial_blocks(&self) -> Vec<(BlockPos, BlockId)> {
        let s = &self.siege;
        let (plank, post) = (block("plank"), block("oakTrunk"));
        let mut blocks = Vec::new();
        for along in -GATE_HALF..=GATE_HALF {
            for side in -GATE_HALF..=GATE_HALF {
                for level in 0..GATE_HEIGHT {
                    blocks.push((s.pos(along, side, level), plank));
                }
            }
        }
        // L'abri : quatre poteaux et un toit de planches, à l'aplomb du bélier
        for along in [GATE_HALF + 2, GATE_HALF + 5] {
            for side in [-GATE_HALF, GATE_HALF] {
                for level in 0..GATE_HEIGHT {
                    blocks.push((s.pos(along, side, level), post));
                }
            }
        }
        for along in GATE_HALF + 1..=GATE_HALF + 6 {
            for side in -GATE_HALF..=GATE_HALF {
                blocks.push((s.pos(along, side, GATE_HEIGHT), plank));
            }
        }
        blocks.extend(self.ram(false));
        blocks
    }

    /// Le tronc du bélier : quatre blocs dans l'axe, à mi-hauteur, contre la porte (`forward`) ou
    /// reculé d'un bloc ; le bloc qu'il quitte redevient de l'air
    fn ram(&self, forward: bool) -> Vec<(BlockPos, BlockId)> {
        let s = &self.siege;
        let trunk = block("oakTrunk");
        let start = GATE_HALF + if forward { 1 } else { 2 };
        let mut blocks: Vec<_> = (start..start + 4).map(|along| (s.pos(along, 0, 1), trunk)).collect();
        let freed = if forward { start + 4 } else { start - 1 };
        blocks.push((s.pos(freed, 0, 1), BlockId::AIR));
        blocks
    }

    /// La herse, dans le dernier rang du passage côté ville
    fn portcullis(&self) -> Vec<(BlockPos, BlockId)> {
        let bars = block("barriereFer");
        (-GATE_HALF..=GATE_HALF).flat_map(|side| (0..GATE_HEIGHT).map(move |level| (side, level))).map(|(side, level)| (self.siege.pos(-GATE_HALF, side, level), bars)).collect()
    }

    /// La porte, le bélier, son abri et la herse ne se cassent pas à la pioche
    pub fn protects(&self, pos: BlockPos) -> bool {
        let s = &self.siege;
        if pos.face != s.face || pos.y < s.floor || pos.y > s.floor + GATE_HEIGHT {
            return false;
        }
        let (along, side) = s.local(pos.x, pos.z);
        side.abs() <= GATE_HALF && (-GATE_HALF..=GATE_HALF + 7).contains(&along)
    }

    /// Le cri de la charge : les soldats `soldiers` (solo, vide en ligne) courent vers la porte
    pub fn start(&mut self, soldiers: &[herocraft_common::CreatureId], out: &mut Vec<ChargeEvent>) {
        if self.phase != Phase::Waiting {
            return;
        }
        self.phase = Phase::Charging;
        self.timer = 0.0;
        self.soldiers = soldiers.to_vec();
        // Une foule serrée devant l'abri du bélier
        let spots = |i: usize| {
            let row = (i / 7) as f32;
            let col = (i % 7) as f32 - 3.0;
            (GATE_HALF as f32 + 9.0 + row * 1.6, col * 1.8 + if (i / 7) % 2 == 1 { 0.9 } else { 0.0 })
        };
        let march = self.soldiers.iter().enumerate().map(|(i, &id)| {
            let (along, side) = spots(i);
            (id, self.siege.point(&self.geometry, along, side, 0.0).normalize_or_zero(), CHARGE_PACE)
        });
        out.push(ChargeEvent::March(march.collect()));
    }

    /// Avance l'assaut : `player` (pieds du joueur)
    pub fn update(&mut self, dt: f32, player: Vec3, out: &mut Vec<ChargeEvent>) {
        self.timer += dt;
        let s = self.siege;
        let at = self.geometry.block_at_point(player.as_dvec3()).filter(|p| p.face == s.face);
        let (along, side) = at.map_or((i32::MAX, i32::MAX), |p| s.local(p.x, p.z));
        let gate_point = s.point(&self.geometry, GATE_HALF as f32 + 0.5, 0.0, 1.5);
        match self.phase {
            Phase::Waiting | Phase::Sealed => {}
            Phase::Charging => {
                let near = (player.distance(gate_point) / self.geometry.block_height as f32) < RAM_START_BLOCKS;
                if near || self.timer >= RAM_START_SECONDS {
                    self.phase = Phase::Ramming;
                    // Premier coup tout de suite
                    self.timer = RAM_INTERVAL;
                }
            }
            Phase::Ramming => {
                if self.ram_forward && self.timer >= RAM_CONTACT {
                    self.ram_forward = false;
                    out.push(ChargeEvent::Blocks(self.ram(false)));
                }
                if self.timer >= RAM_INTERVAL {
                    self.timer = 0.0;
                    self.hits += 1;
                    if self.hits >= RAM_HITS {
                        self.breach(out);
                    } else {
                        self.ram_forward = true;
                        out.push(ChargeEvent::Blocks(self.ram(true)));
                        out.push(ChargeEvent::RamHit(gate_point));
                    }
                }
            }
            Phase::Breached => {
                // Il a passé la porte : la herse tombe derrière lui
                let inside = along <= -GATE_HALF - INSIDE_BLOCKS && side.abs() <= GATE_HALF + 8;
                if inside {
                    self.phase = Phase::Sealed;
                    self.timer = 0.0;
                    out.push(ChargeEvent::Blocks(self.portcullis()));
                    out.push(ChargeEvent::Sealed(s.point(&self.geometry, -GATE_HALF as f32, 0.0, 1.0)));
                }
            }
        }
    }

    /// La porte vole en éclats : le passage s'ouvre, le tronc du bélier tombe avec elle, le rayon
    /// s'élève, et les soldats s'engouffrent dans la ville
    fn breach(&mut self, out: &mut Vec<ChargeEvent>) {
        let s = self.siege;
        self.phase = Phase::Breached;
        self.timer = 0.0;
        let mut blocks = Vec::new();
        for along in -GATE_HALF..=GATE_HALF {
            for side in -GATE_HALF..=GATE_HALF {
                for level in 0..GATE_HEIGHT {
                    blocks.push((s.pos(along, side, level), BlockId::AIR));
                }
            }
        }
        for along in GATE_HALF + 1..=GATE_HALF + 6 {
            blocks.push((s.pos(along, 0, 1), BlockId::AIR));
        }
        out.push(ChargeEvent::Blocks(blocks));
        out.push(ChargeEvent::GateBroken(s.point(&self.geometry, GATE_HALF as f32, 0.0, 1.5)));
        let (cx, cz) = s.center;
        out.push(ChargeEvent::Beam(self.geometry.corner(s.face, cx as f64 + 0.5, s.center_floor as f64, cz as f64 + 0.5).as_vec3()));
        // Dans la ville, en colonne derrière la porte
        let march = self.soldiers.iter().enumerate().map(|(i, &id)| {
            let along = -(GATE_HALF as f32) - 6.0 - (i / 3) as f32 * 1.5;
            let side = (i % 3) as f32 - 1.0;
            (id, s.point(&self.geometry, along, side * 1.4, 0.0).normalize_or_zero(), CHARGE_PACE)
        });
        out.push(ChargeEvent::March(march.collect()));
    }

    /// La consigne de la phase
    pub fn hint(&self) -> Option<String> {
        Some(match self.phase {
            Phase::Waiting => return None,
            Phase::Charging | Phase::Ramming => "À la charge ! Cours vers la grande porte avec l'armée.".to_string(),
            Phase::Breached => "La porte a cédé ! Entre dans la ville avec les soldats.".to_string(),
            Phase::Sealed if self.timer < SEALED_HINT_SECONDS => "Une herse s'abat derrière toi : plus de retour possible. Le temple t'attend au cœur de la ville, sous le rayon violet.".to_string(),
            Phase::Sealed => return None,
        })
    }

    /// Avance l'horloge de la phase scellée (la consigne s'efface)
    pub fn tick_hint(&mut self, dt: f32) {
        if self.phase == Phase::Sealed {
            self.timer += dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge() -> (VisionCharge, BlockGeometry) {
        let geometry = BlockGeometry::new(crate::camera::DEFAULT_RADIUS as f64);
        let siege = Siege { face: PlanetFace::PosY, gate: (500, 500), out: (0, 1), floor: 120, center: (500, 440), center_floor: 125 };
        (VisionCharge::new(siege, geometry.clone()), geometry)
    }

    fn feet(c: &VisionCharge, g: &BlockGeometry, along: f32) -> Vec3 {
        c.siege.point(g, along, 0.0, 0.0)
    }

    #[test]
    fn the_ram_breaks_the_gate_then_the_portcullis_falls_behind_the_hero() {
        let (mut c, g) = charge();
        let initial = c.initial_blocks();
        let plank = block("plank");
        assert!(initial.iter().any(|&(p, b)| p == c.siege.pos(0, 0, 1) && b == plank), "la porte est fermée");
        assert!(c.protects(c.siege.pos(0, 0, 1)) && c.protects(c.siege.pos(GATE_HALF + 3, 0, 1)));
        assert!(!c.protects(c.siege.pos(-20, 0, 1)));
        let mut out = Vec::new();
        // Rien ne bouge avant l'éveil
        c.update(1.0, feet(&c, &g, 30.0), &mut out);
        assert!(out.is_empty() && c.phase() == Phase::Waiting);
        c.start(&[7, 8, 9].map(herocraft_common::CreatureId), &mut out);
        assert!(matches!(&out[0], ChargeEvent::March(m) if m.len() == 3));
        out.clear();
        // Le joueur court vers la porte : le bélier frappe
        c.update(0.1, feet(&c, &g, 10.0), &mut out);
        assert_eq!(c.phase(), Phase::Ramming);
        let mut hits = 0;
        let mut broken = false;
        for _ in 0..200 {
            out.clear();
            c.update(0.05, feet(&c, &g, 10.0), &mut out);
            hits += out.iter().filter(|e| matches!(e, ChargeEvent::RamHit(_))).count();
            broken |= out.iter().any(|e| matches!(e, ChargeEvent::GateBroken(_)));
            if broken {
                assert!(out.iter().any(|e| matches!(e, ChargeEvent::Beam(_))), "le rayon s'élève");
                assert!(out.iter().any(|e| matches!(e, ChargeEvent::Blocks(b) if b.iter().any(|&(p, id)| p == c.siege.pos(0, 0, 1) && id == BlockId::AIR))));
                break;
            }
        }
        assert!(broken && hits as u32 == RAM_HITS - 1, "{hits}");
        assert_eq!(c.phase(), Phase::Breached);
        // Dans le passage, rien ; une fois dans la ville, la herse tombe
        out.clear();
        c.update(0.1, feet(&c, &g, 0.0), &mut out);
        assert!(out.is_empty());
        c.update(0.1, feet(&c, &g, -(GATE_HALF + INSIDE_BLOCKS) as f32 - 1.0), &mut out);
        assert_eq!(c.phase(), Phase::Sealed);
        assert!(out.iter().any(|e| matches!(e, ChargeEvent::Sealed(_))));
        assert!(c.hint().is_some());
        c.tick_hint(SEALED_HINT_SECONDS + 1.0);
        assert!(c.hint().is_none());
    }

    #[test]
    fn the_gate_plug_fills_the_real_passage_of_a_memory_city() {
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 4242, ..Default::default() });
        let mid = herocraft_world::FACE_BLOCKS / 2;
        let site = generator.nearest_temple(PlanetFace::PosX, mid, mid, herocraft_world::FACE_BLOCKS).expect("un temple");
        let holder = site.holder_for(herocraft_world::Culture::Humains);
        let past = generator.with_epoch(herocraft_world::Epoch::Past(holder));
        let city = past.memory_city(&site, holder).expect("la ville-souvenir");
        let siege = Siege::of(city, site.face, &past);
        // Le passage est de l'air au niveau du sol, entre deux pans de rempart
        for along in -GATE_HALF..=GATE_HALF {
            for side in -GATE_HALF..=GATE_HALF {
                let p = siege.pos(along, side, 0);
                assert!(!past.block_at(p.face, p.x, p.y, p.z).is_solid(), "{along} {side}");
            }
        }
        let wall = siege.pos(0, GATE_HALF + 1, 1);
        assert!(past.block_at(wall.face, wall.x, wall.y, wall.z).is_solid(), "le rempart borde la porte");
        // Dehors, le terrain de l'armée
        let (x, z) = siege.column(GATE_HALF + 10, 0);
        assert!(city.field.contains(x, z));
    }

    #[test]
    fn the_ram_starts_alone_if_the_hero_lingers() {
        let (mut c, g) = charge();
        let mut out = Vec::new();
        c.start(&[], &mut out);
        c.update(RAM_START_SECONDS + 0.1, feet(&c, &g, 60.0), &mut out);
        assert_eq!(c.phase(), Phase::Ramming);
    }
}

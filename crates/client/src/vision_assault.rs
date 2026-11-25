//! La traversée de la ville-souvenir, jusqu'à la sortie du souvenir (bible, section 8) : ce qui
//! suit la charge, une fois la herse tombée derrière le héros.
//!
//! 4. **Les soldats en nombre.** Les défenseurs tiennent leurs postes dans les rues et sur les
//!    remparts (`MemoryCity::posts`) : ils n'apparaissent qu'à l'approche du héros, pour en
//!    montrer beaucoup sans tous les simuler. L'armée le suit et se bat contre eux ; les
//!    barricades du plan forcent les détours, et beaucoup d'alliés tombent.
//! 5. **Les arènes.** Sur chacune des trois places (`MemoryCity::arenas`), alliés et ennemis
//!    s'écartent et forment un cercle qui repousse tout le monde vers le centre, héros et
//!    mini-boss : on ne sort du cercle qu'une fois le mini-boss tombé.
//! 6. **Le boss final.** Presque seul, le héros arrive dans l'arène du temple : un héraut l'y
//!    attend, seul. Dans une explosion qui tue les derniers alliés, il prend la forme d'un
//!    monstre du Néant.
//! 7. **La sortie.** Le monstre vaincu, la Pierre du Néant s'éveille sur l'autel du sanctuaire et
//!    appelle le héros (le murmure revient). Il la brise, la ramasse : `fin-de-la-vision` se joue,
//!    puis il revient au présent avec la puissance de la Pierre et un vrai éclat de cristal.
//!
//! Le souvenir se vit en solo : ce module mène lui-même les créatures du monde solo.

use glam::Vec3;
use herocraft_common::{BlockId, PlanetFace, StoneEvent, StoneKind};
use herocraft_world::{BlockGeometry, BlockPos, CreatureWorld, Epoch, MemoryCity, TempleSite};

/// Les mini-boss, une espèce par place, de la grande porte vers le temple
const MINI_BOSSES: [&str; 3] = ["capitaineEnnemi", "bruteDeGuerre", "championVille"];
const ENEMY: &str = "soldatEnnemi";
const RING: &str = "soldatCercle";
const HERALD: &str = "herautDuNeant";
const MONSTER: &str = "monstreDuNeant";
/// Bloc de la Pierre posée sur l'autel
const STONE_BLOCK: &str = "voidStonePlaced";

/// Un poste s'éveille quand le héros passe à moins de tant de blocs…
const POST_WAKE_BLOCKS: f32 = 38.0;
/// … mais pas sous ses yeux
const POST_MIN_BLOCKS: f32 = 11.0;
/// Au plus tant de défenseurs debout à la fois
const MAX_ENEMIES: usize = 26;
/// Au plus tant de postes éveillés par image
const WAKES_PER_FRAME: usize = 3;
/// Secondes entre deux ordres donnés à l'armée
const ORDER_INTERVAL: f32 = 2.0;
/// Le cercle d'une place se forme quand le héros entre à moins de tant de blocs de son centre
const ARENA_TRIGGER_BLOCKS: f32 = 6.5;
/// Rayon du cercle des soldats, en blocs
const RING_RADIUS_BLOCKS: f32 = 8.5;
/// Soldats du cercle
const RING_SOLDIERS: usize = 20;
/// Au-delà de tant de blocs du centre, le cercle repousse héros et mini-boss
const RING_PUSH_BLOCKS: f32 = 7.3;
/// Le héraut apparaît quand le héros est à moins de tant de blocs de l'arène du temple…
const HERALD_SHOW_BLOCKS: f32 = 45.0;
/// … et se transforme à moins de tant
const HERALD_TRIGGER_BLOCKS: f32 = 10.0;
/// L'explosion de la transformation tue les alliés à moins de tant de blocs
const BLAST_BLOCKS: f32 = 30.0;
/// Le monstre ne quitte pas l'arène du temple : il y est ramené au-delà de tant de blocs
const MONSTER_LEASH_BLOCKS: f32 = 16.0;
/// Durée d'une annonce (un mini-boss tombé…), en secondes
const NOTICE_SECONDS: f32 = 5.0;

/// Ce que la boucle du jeu doit faire
#[derive(Debug, Clone, PartialEq)]
pub enum AssaultEvent {
    /// Un effet visuel en ce point, tenu tant de secondes s'il boucle
    Effect(&'static str, Vec3, Option<f32>),
    /// Un événement de cinématique, en ce point
    Notify(&'static str, Vec3),
    /// Changer ces blocs de la vision
    Blocks(Vec<(BlockPos, BlockId)>),
    /// Déplacer le héros de tant (repère du monde) : le cercle le repousse
    Push(Vec3),
    /// Secousse de la vue, de 0 à 1
    Shake(f32),
    /// Grondement (explosion, cercle qui se forme), de 0 à 1
    Rumble(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// La charge n'est pas finie
    Waiting,
    /// Dans les rues, avec l'armée
    Streets,
    /// Le cercle d'une place s'est formé autour de son mini-boss
    Arena(usize),
    /// Le héraut attend au centre de l'arène du temple
    Herald,
    /// Le monstre du Néant
    Monster,
    /// La Pierre appelle le héros, sur l'autel
    Crystal,
    /// La Pierre est ramassée : le souvenir s'achève
    Done,
}

#[derive(Debug, Clone)]
struct Arena {
    center: (i32, i32),
    species: &'static str,
    done: bool,
}

pub struct VisionAssault {
    face: PlanetFace,
    geometry: BlockGeometry,
    posts: Vec<((i32, i32), bool)>,
    arenas: Vec<Arena>,
    /// Centre de l'arène du temple, au niveau de son sol
    temple_arena: BlockPos,
    /// Où la Pierre s'éveille : sur l'autel du sanctuaire
    stone: BlockPos,
    /// Juste derrière la grande porte, dans la ville : premier point de reprise
    inside: (i32, i32),
    phase: Phase,
    allies: Vec<u64>,
    enemies: Vec<u64>,
    ring: Vec<u64>,
    boss: Option<u64>,
    herald: Option<u64>,
    order_in: f32,
    /// Où reprendre si le héros tombe (colonne)
    checkpoint: Option<(i32, i32)>,
    notice: Option<(String, f32)>,
    spawned: u32,
}

impl VisionAssault {
    pub fn new(city: &MemoryCity, site: &TempleSite, geometry: BlockGeometry) -> Self {
        let face = site.face;
        let (gx, gz) = city.gate;
        let (cx, cz) = city.center;
        let len = (((cx - gx).pow(2) + (cz - gz).pow(2)) as f32).sqrt().max(1.0);
        let inside = (gx + ((cx - gx) as f32 / len * 9.0).round() as i32, gz + ((cz - gz) as f32 / len * 9.0).round() as i32);
        Self {
            face,
            geometry,
            posts: city.posts.iter().map(|&(p, _)| (p, false)).collect(),
            arenas: city.arenas.iter().zip(MINI_BOSSES).map(|(&center, species)| Arena { center, species, done: false }).collect(),
            temple_arena: site.arena_center(),
            stone: site.stone_slot(Epoch::Past(herocraft_world::Culture::Humains)),
            inside,
            phase: Phase::Waiting,
            allies: Vec::new(),
            enemies: Vec::new(),
            ring: Vec::new(),
            boss: None,
            herald: None,
            order_in: 0.0,
            checkpoint: None,
            notice: None,
            spawned: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    fn block(&self) -> f32 {
        self.geometry.block_height as f32
    }

    /// Direction (depuis le centre de la planète) de la colonne (x, z)
    fn dir(&self, (x, z): (i32, i32)) -> Vec3 {
        self.geometry.corner(self.face, x as f64 + 0.5, 0.0, z as f64 + 0.5).as_vec3().normalize_or_zero()
    }

    fn at_floor(&self, pos: BlockPos) -> Vec3 {
        self.geometry.corner(pos.face, pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5).as_vec3()
    }

    /// Distance en blocs, à plat, du héros à la colonne
    fn flat_blocks(&self, player: Vec3, column: (i32, i32)) -> f32 {
        let up = player.normalize_or_zero();
        let d = self.dir(column) * player.length() - player;
        (d - up * d.dot(up)).length() / self.block()
    }

    /// La herse est tombée : la traversée commence, avec les soldats qui ont passé la porte
    pub fn start(&mut self, allies: &[u64]) {
        if self.phase == Phase::Waiting {
            self.phase = Phase::Streets;
            self.allies = allies.to_vec();
            self.checkpoint = Some(self.inside);
        }
    }

    /// Le héros tombé reprend ici (direction depuis le centre de la planète)
    pub fn checkpoint(&self) -> Option<Vec3> {
        self.checkpoint.map(|c| self.dir(c))
    }

    /// Où la Pierre appelle le héros (le murmure en vient), une fois le monstre vaincu
    pub fn calling(&self) -> Option<Vec3> {
        (self.phase == Phase::Crystal).then(|| self.at_floor(self.stone) + self.at_floor(self.stone).normalize_or_zero() * 0.5 * self.block())
    }

    /// La Pierre de l'autel est ramassée : le souvenir s'achève (`fin-de-la-vision` suit)
    pub fn on_stone(&mut self, event: &StoneEvent) {
        if self.phase == Phase::Crystal && matches!(event, StoneEvent::Obtained { kind: StoneKind::Whole, .. }) {
            self.phase = Phase::Done;
        }
    }

    /// Le souvenir est allé au bout
    pub fn completed(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Les créatures du souvenir, à retirer au retour
    pub fn creatures(&self) -> impl Iterator<Item = u64> + '_ {
        self.allies.iter().chain(&self.enemies).chain(&self.ring).chain(&self.boss).chain(&self.herald).copied()
    }

    fn alive(world: &CreatureWorld, id: u64) -> bool {
        world.creatures().iter().any(|c| c.id == id && c.is_alive())
    }

    fn position(world: &CreatureWorld, id: u64) -> Option<Vec3> {
        world.creatures().iter().find(|c| c.id == id && c.is_alive()).map(|c| c.position)
    }

    /// Cap (0 : nord, π/2 : est) de `from` vers `to`
    fn yaw(from: Vec3, to: Vec3) -> f32 {
        let up = from.normalize_or_zero();
        let (east, north) = herocraft_common::tangent_frame(up);
        let d = to - from;
        d.dot(east).atan2(d.dot(north)).rem_euclid(std::f32::consts::TAU)
    }

    fn spawn(world: &mut CreatureWorld, species: &str, feet: Vec3, yaw: f32) -> Option<u64> {
        let index = world.registry().find(species)?;
        Some(world.spawn_led(index, feet, yaw))
    }

    /// Avance la traversée. `ground(dir)` : les pieds posés au sol dans cette direction.
    pub fn update(&mut self, dt: f32, player: Vec3, world: &mut CreatureWorld, ground: &dyn Fn(Vec3) -> Vec3, out: &mut Vec<AssaultEvent>) {
        if let Some((_, left)) = self.notice.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                self.notice = None;
            }
        }
        self.allies.retain(|&id| Self::alive(world, id));
        self.enemies.retain(|&id| Self::alive(world, id));
        match self.phase {
            Phase::Waiting | Phase::Done => {}
            Phase::Streets => {
                self.lead_allies(dt, player, world, ground);
                self.wake_posts(player, world, ground);
                if let Some(i) = self.arenas.iter().position(|a| !a.done && self.flat_blocks(player, a.center) <= ARENA_TRIGGER_BLOCKS) {
                    self.begin_arena(i, player, world, ground, out);
                } else if self.flat_blocks(player, (self.temple_arena.x, self.temple_arena.z)) <= HERALD_SHOW_BLOCKS {
                    let at = self.at_floor(self.temple_arena);
                    self.herald = Self::spawn(world, HERALD, at, Self::yaw(at, player));
                    self.phase = Phase::Herald;
                }
            }
            Phase::Arena(i) => self.hold_arena(i, dt, player, world, out),
            Phase::Herald => {
                self.lead_allies(dt, player, world, ground);
                self.wake_posts(player, world, ground);
                if self.flat_blocks(player, (self.temple_arena.x, self.temple_arena.z)) <= HERALD_TRIGGER_BLOCKS {
                    self.transform(player, world, out);
                }
            }
            Phase::Monster => {
                let Some(boss) = self.boss else { return };
                match Self::position(world, boss) {
                    Some(at) => {
                        // Il ne quitte pas l'arène du temple
                        let center = self.at_floor(self.temple_arena);
                        let away = at - center;
                        let up = at.normalize_or_zero();
                        let flat = away - up * away.dot(up);
                        let d = flat.length() / self.block();
                        if d > MONSTER_LEASH_BLOCKS {
                            world.slide(boss, -flat.normalize_or_zero() * (d - MONSTER_LEASH_BLOCKS) * self.block());
                        }
                    }
                    None => self.monster_slain(out),
                }
            }
            Phase::Crystal => {}
        }
    }

    /// L'armée suit le héros, en grappe autour de lui (elle se bat quand un ennemi est en vue)
    fn lead_allies(&mut self, dt: f32, player: Vec3, world: &mut CreatureWorld, _ground: &dyn Fn(Vec3) -> Vec3) {
        self.order_in -= dt;
        if self.order_in > 0.0 {
            return;
        }
        self.order_in = ORDER_INTERVAL;
        let up = player.normalize_or_zero();
        let (t1, t2) = herocraft_common::tangent_frame(up);
        let block = self.block();
        for (i, &id) in self.allies.iter().enumerate() {
            let angle = i as f32 * 2.399;
            let radius = (2.5 + (i % 5) as f32 * 1.1) * block;
            let target = player + (t1 * angle.cos() + t2 * angle.sin()) * radius;
            world.send_to(id, target.normalize_or_zero(), 1.5);
        }
    }

    /// Les défenseurs des postes proches prennent place
    fn wake_posts(&mut self, player: Vec3, world: &mut CreatureWorld, ground: &dyn Fn(Vec3) -> Vec3) {
        let mut woken = 0;
        for k in 0..self.posts.len() {
            if self.enemies.len() >= MAX_ENEMIES || woken >= WAKES_PER_FRAME {
                break;
            }
            let (post, done) = self.posts[k];
            if done {
                continue;
            }
            let d = self.flat_blocks(player, post);
            if !(POST_MIN_BLOCKS..=POST_WAKE_BLOCKS).contains(&d) {
                continue;
            }
            self.posts[k].1 = true;
            woken += 1;
            // Un ou deux par poste
            let count = 1 + self.spawned.is_multiple_of(3) as u32;
            for j in 0..count {
                let up = self.dir(post);
                let (t1, _) = herocraft_common::tangent_frame(up);
                let feet = ground((up + t1 * j as f32 * 1.2 * self.block() / player.length()).normalize_or_zero());
                if let Some(id) = Self::spawn(world, ENEMY, feet, Self::yaw(feet, player)) {
                    self.enemies.push(id);
                    self.spawned += 1;
                }
            }
        }
    }

    /// Le cercle se forme sur la place `i` : les combattants s'écartent, les soldats du cercle
    /// l'entourent, le mini-boss attend au centre
    fn begin_arena(&mut self, i: usize, player: Vec3, world: &mut CreatureWorld, ground: &dyn Fn(Vec3) -> Vec3, out: &mut Vec<AssaultEvent>) {
        let center_dir = self.dir(self.arenas[i].center);
        let center = ground(center_dir);
        let up = center_dir;
        let (t1, t2) = herocraft_common::tangent_frame(up);
        let block = self.block();
        // Ceux qui sont dans le cercle s'en écartent
        for &id in self.allies.iter().chain(&self.enemies) {
            let Some(at) = Self::position(world, id) else { continue };
            let away = at - center;
            let flat = away - up * away.dot(up);
            let d = flat.length() / block;
            if d < RING_RADIUS_BLOCKS + 1.5 {
                let dir = if flat.length_squared() > 1e-12 { flat.normalize() } else { t1 };
                world.slide(id, dir * (RING_RADIUS_BLOCKS + 2.0 - d) * block);
            }
        }
        for k in 0..RING_SOLDIERS {
            let a = k as f32 / RING_SOLDIERS as f32 * std::f32::consts::TAU;
            let feet = ground((center + (t1 * a.cos() + t2 * a.sin()) * RING_RADIUS_BLOCKS * block).normalize_or_zero());
            if let Some(id) = Self::spawn(world, RING, feet, Self::yaw(feet, center)) {
                self.ring.push(id);
            }
        }
        // Le mini-boss, de l'autre côté du centre
        let away = center - player;
        let flat = away - up * away.dot(up);
        let side = if flat.length_squared() > 1e-12 { flat.normalize() } else { t2 };
        let feet = ground((center + side * 3.0 * block).normalize_or_zero());
        self.boss = Self::spawn(world, self.arenas[i].species, feet, Self::yaw(feet, player));
        self.phase = Phase::Arena(i);
        out.push(AssaultEvent::Effect("shockRing", center, None));
        out.push(AssaultEvent::Rumble(0.35));
        out.push(AssaultEvent::Notify("arena_formed", center));
    }

    fn hold_arena(&mut self, i: usize, dt: f32, player: Vec3, world: &mut CreatureWorld, out: &mut Vec<AssaultEvent>) {
        let block = self.block();
        let center_dir = self.dir(self.arenas[i].center);
        let center = center_dir * player.length();
        let push_back = |at: Vec3| {
            let up = at.normalize_or_zero();
            let to = center_dir * at.length() - at;
            let flat = to - up * to.dot(up);
            let d = flat.length() / block;
            (d > RING_PUSH_BLOCKS).then(|| flat.normalize_or_zero() * (d - RING_PUSH_BLOCKS) * block)
        };
        if let Some(delta) = push_back(player) {
            out.push(AssaultEvent::Push(delta * (dt * 6.0).min(1.0)));
        }
        let boss = self.boss.filter(|&id| Self::alive(world, id));
        match boss {
            Some(id) => {
                if let Some(delta) = Self::position(world, id).and_then(push_back) {
                    world.slide(id, delta);
                }
            }
            None => {
                // Le mini-boss est tombé : le cercle se défait, la traversée reprend
                let name = world.registry().find(self.arenas[i].species).and_then(|s| world.registry().get(s)).map(|s| s.name.clone()).unwrap_or_default();
                for id in self.ring.drain(..) {
                    world.despawn(id);
                }
                self.arenas[i].done = true;
                self.boss = None;
                self.checkpoint = Some(self.arenas[i].center);
                self.phase = Phase::Streets;
                self.order_in = 0.0;
                let left = self.arenas.iter().filter(|a| !a.done).count();
                let next = if left == 0 { "Le chemin du temple est ouvert.".to_string() } else { format!("Encore {left} à abattre avant le temple.") };
                self.notice = Some((format!("{name} est tombé ! {next}"), NOTICE_SECONDS));
                out.push(AssaultEvent::Effect("dustPuff", center, None));
                out.push(AssaultEvent::Notify("arena_won", center));
            }
        }
    }

    /// Le héraut prend la forme du monstre du Néant : l'explosion tue les derniers alliés
    fn transform(&mut self, player: Vec3, world: &mut CreatureWorld, out: &mut Vec<AssaultEvent>) {
        let at = self.at_floor(self.temple_arena);
        let block = self.block();
        if let Some(id) = self.herald.take() {
            world.despawn(id);
        }
        for &id in &self.allies {
            if Self::position(world, id).is_some_and(|p| p.distance(at) / block < BLAST_BLOCKS) {
                world.wound(id, at, 0, 10_000.0);
            }
        }
        let up = at.normalize_or_zero();
        let away = player - at;
        let flat = away - up * away.dot(up);
        out.push(AssaultEvent::Push(flat.normalize_or_zero() * 3.0 * block));
        out.push(AssaultEvent::Effect("shapeshiftBurst", at, None));
        out.push(AssaultEvent::Effect("fireExplosion", at, None));
        out.push(AssaultEvent::Effect("soulBlast", at, None));
        out.push(AssaultEvent::Effect("shockRing", at, None));
        out.push(AssaultEvent::Shake(1.0));
        out.push(AssaultEvent::Rumble(1.0));
        out.push(AssaultEvent::Notify("void_monster", at));
        self.boss = Self::spawn(world, MONSTER, at, Self::yaw(at, player));
        self.phase = Phase::Monster;
        // Tombé contre le monstre, il reprend au bord de l'arène
        let back = (at + flat.normalize_or_zero() * 13.0 * block).normalize_or_zero();
        let column = self.geometry.block_at_point((back * at.length()).as_dvec3()).map(|p| (p.x, p.z));
        self.checkpoint = column.or(self.checkpoint);
    }

    /// Le monstre est vaincu : la Pierre s'éveille sur l'autel et appelle le héros
    fn monster_slain(&mut self, out: &mut Vec<AssaultEvent>) {
        self.boss = None;
        self.phase = Phase::Crystal;
        let at = self.at_floor(self.temple_arena);
        let stone = self.at_floor(self.stone);
        if let Some(block) = herocraft_common::blocks().find(STONE_BLOCK) {
            out.push(AssaultEvent::Blocks(vec![(self.stone, block)]));
        }
        out.push(AssaultEvent::Effect("holyBeam", stone, None));
        out.push(AssaultEvent::Effect("shockRing", at, None));
        out.push(AssaultEvent::Notify("monster_slain", stone));
        self.checkpoint = Some((self.temple_arena.x, self.temple_arena.z));
    }

    /// La consigne de la phase
    pub fn hint(&self, world: &CreatureWorld) -> Option<String> {
        if let Some((notice, _)) = &self.notice {
            return Some(notice.clone());
        }
        Some(match self.phase {
            Phase::Waiting | Phase::Done => return None,
            Phase::Streets => {
                let left = self.arenas.iter().filter(|a| !a.done).count();
                if left > 0 {
                    "Traverse la ville vers le temple, sous le rayon violet. Les défenseurs tiennent les rues : l'armée te suit.".to_string()
                } else {
                    "Le temple est tout proche, sous le rayon violet.".to_string()
                }
            }
            Phase::Arena(i) => {
                let name = world.registry().find(self.arenas[i].species).and_then(|s| world.registry().get(s)).map(|s| s.name.clone()).unwrap_or_default();
                format!("Le cercle s'est refermé : on n'en sort pas. Abats {name} !")
            }
            Phase::Herald => "Dans l'arène du temple, quelqu'un t'attend, seul.".to_string(),
            Phase::Monster => "Le héraut a pris la forme d'un monstre du Néant ! Abats-le.".to_string(),
            Phase::Crystal => "Le monstre est vaincu. La Pierre du Néant t'appelle, sur l'autel du sanctuaire : brise-la et prends-la.".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_world::PlanetWorldGenerator;

    struct Setup {
        assault: VisionAssault,
        world: CreatureWorld,
        city: MemoryCity,
        site: TempleSite,
        generator: PlanetWorldGenerator,
        geometry: BlockGeometry,
    }

    fn setup() -> Setup {
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 4242, ..Default::default() });
        let mid = herocraft_world::FACE_BLOCKS / 2;
        let site = generator.nearest_temple(PlanetFace::PosX, mid, mid, herocraft_world::FACE_BLOCKS).expect("un temple");
        let holder = site.holder_for(herocraft_world::Culture::Humains);
        let past = generator.with_epoch(Epoch::Past(holder));
        let city = past.memory_city(&site, holder).expect("la ville-souvenir").clone();
        let geometry = BlockGeometry::new(past.base_radius());
        let registry = std::sync::Arc::new(herocraft_common::CreatureRegistry::load_default());
        let world = CreatureWorld::new(past.base_radius(), 4242, registry);
        let assault = VisionAssault::new(&city, &site, geometry);
        Setup { assault, world, city, site, generator: past, geometry }
    }

    fn feet_at(s: &Setup, (x, z): (i32, i32)) -> Vec3 {
        let ground = s.generator.sample_block_column(s.site.face, x, z).elevation.floor() + 1.0;
        s.geometry.corner(s.site.face, x as f64 + 0.5, ground, z as f64 + 0.5).as_vec3()
    }

    fn alive(world: &CreatureWorld, species: &str) -> Vec<u64> {
        let index = world.registry().find(species).unwrap();
        world.creatures().iter().filter(|c| c.species == index && c.is_alive()).map(|c| c.id).collect()
    }

    #[test]
    fn the_whole_memory_from_the_streets_to_the_stone() {
        let mut s = setup();
        let mut out = Vec::new();
        let radius = s.geometry.corner(PlanetFace::PosX, 0.0, 0.0, 0.0).as_vec3().length();
        let ground = move |dir: Vec3| dir * radius;
        let ground = &ground;
        // Avant la herse, rien
        s.assault.update(0.1, feet_at(&s, s.city.gate), &mut s.world, ground, &mut out);
        assert_eq!(s.assault.phase(), Phase::Waiting);
        let inside = feet_at(&s, s.assault.inside);
        let allies: Vec<u64> = (0..6).filter_map(|_| VisionAssault::spawn(&mut s.world, "soldatAllie", inside, 0.0)).collect();
        s.assault.start(&allies);
        assert!(s.assault.checkpoint().is_some());
        // Dans les rues : les défenseurs prennent leurs postes à l'approche
        s.assault.update(0.1, feet_at(&s, s.assault.inside), &mut s.world, ground, &mut out);
        for _ in 0..20 {
            s.assault.update(0.1, feet_at(&s, s.assault.inside), &mut s.world, ground, &mut out);
        }
        assert!(!alive(&s.world, ENEMY).is_empty(), "des défenseurs aux postes");
        assert!(s.assault.hint(&s.world).is_some());
        // Les trois places : le cercle, le mini-boss, puis le cercle se défait
        for (i, &boss_kind) in MINI_BOSSES.iter().enumerate() {
            let center = s.city.arenas[i];
            s.assault.update(0.1, feet_at(&s, center), &mut s.world, ground, &mut out);
            assert_eq!(s.assault.phase(), Phase::Arena(i));
            assert_eq!(alive(&s.world, RING).len(), RING_SOLDIERS);
            let boss = alive(&s.world, boss_kind);
            assert_eq!(boss.len(), 1);
            // Le héros qui s'éloigne est repoussé vers le centre
            out.clear();
            let far = (center.0 + 12, center.1);
            s.assault.update(0.1, feet_at(&s, far), &mut s.world, ground, &mut out);
            assert!(out.iter().any(|e| matches!(e, AssaultEvent::Push(_))));
            s.world.wound(boss[0], feet_at(&s, center), 0, 100_000.0);
            s.assault.update(0.1, feet_at(&s, center), &mut s.world, ground, &mut out);
            assert_eq!(s.assault.phase(), Phase::Streets);
            assert!(alive(&s.world, RING).is_empty());
        }
        // Le temple : le héraut, puis le monstre
        let arena = (s.site.arena_center().x, s.site.arena_center().z);
        s.assault.update(0.1, feet_at(&s, (arena.0 + 30, arena.1)), &mut s.world, ground, &mut out);
        assert_eq!(s.assault.phase(), Phase::Herald);
        assert_eq!(alive(&s.world, HERALD).len(), 1);
        out.clear();
        s.assault.update(0.1, feet_at(&s, (arena.0 + 5, arena.1)), &mut s.world, ground, &mut out);
        assert_eq!(s.assault.phase(), Phase::Monster);
        assert!(out.iter().any(|e| matches!(e, AssaultEvent::Notify("void_monster", _))));
        assert!(alive(&s.world, HERALD).is_empty());
        let monster = alive(&s.world, MONSTER);
        assert_eq!(monster.len(), 1);
        s.world.wound(monster[0], feet_at(&s, arena), 0, 100_000.0);
        out.clear();
        s.assault.update(0.1, feet_at(&s, arena), &mut s.world, ground, &mut out);
        assert_eq!(s.assault.phase(), Phase::Crystal);
        assert!(s.assault.calling().is_some());
        let stone = herocraft_common::blocks().find(STONE_BLOCK).unwrap();
        assert!(out.iter().any(|e| matches!(e, AssaultEvent::Blocks(b) if b.iter().any(|&(_, id)| id == stone))));
        // La Pierre ramassée : le souvenir s'achève
        let item = StoneKind::Whole.item().unwrap();
        s.assault.on_stone(&StoneEvent::Obtained { kind: StoneKind::Whole, item, position: [0.0; 3] });
        assert!(s.assault.completed());
        assert!(s.assault.creatures().count() > 0);
    }
}

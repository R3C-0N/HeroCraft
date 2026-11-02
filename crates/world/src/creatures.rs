//! Créatures : elles apparaissent sur le relief autour des joueurs, s'y promènent au hasard et
//! disparaissent quand plus aucun joueur n'est près d'elles.
//!
//! La même simulation tourne sur le serveur, qui fait autorité et diffuse les positions, et dans la
//! partie solo du client. Elle lit le terrain bloc par bloc (générateur + modifications) : une
//! créature marche sur les blocs posés, tombe dans les trous creusés, regarde quelques pas devant
//! elle pour contourner murs et falaises plutôt que de faire demi-tour, préfère la terre ferme et,
//! à l'eau, nage vers la rive la plus proche.
//! Corps, allure et biomes de chaque espèce viennent de sa description (`CreatureRegistry`).
//!
//! Une créature se tourne peu à peu vers son nouveau cap, saute pour monter sur un bloc, et, frappée
//! par un joueur, rougit, recule, puis s'enfuit ; sans vie, elle bascule sur le flanc et disparaît.
//!
//! Le tempérament de l'espèce règle sa réaction au joueur le plus proche : une bête craintive fuit
//! dès qu'il approche (de moins loin s'il est accroupi), une placide s'arrête pour le regarder, une curieuse le suit à distance.
//!
//! Chaque espèce vit à son rythme (`Activity`) : selon la hauteur du soleil là où elle se trouve, elle
//! est éveillée ou s'endort sur place, une bête après l'autre, jusqu'à ce que son heure revienne ou
//! qu'un coup la réveille. Une espèce n'apparaît qu'aux heures où elle est éveillée.
//! Les bêtes apparues ensemble forment un troupeau : chacune revient vers ses voisines quand elle
//! s'en écarte et prend volontiers le cap de celles qui marchent. Quand l'une est frappée, les autres
//! s'affolent aussi, mais moins : elles courent moins vite et se calment plus tôt que la victime.

use std::collections::HashMap;
use std::sync::Arc;
use std::f32::consts::{PI, TAU};

use glam::Vec3;
use herocraft_common::{tangent_frame, BlockId, CreatureRegistry, CreatureSpecies, CreatureState, Temperament, PlanetFace, CREATURE_DEATH_TIME, CREATURE_HURT_TIME, PLAYER_HIT_DAMAGE};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::edits::{BlockGeometry, BlockPos, TerrainEdits, TerrainQuery, WORLD_HEIGHT};
use crate::generator::{BiomeType, ColumnSample, PlanetWorldGenerator};

/// Créatures simulées au plus, tous joueurs confondus
pub const MAX_CREATURES: usize = 64;
/// Une créature plus loin que ça de tous les joueurs disparaît, en blocs. C'est aussi la distance
/// à laquelle le serveur montre une créature à un joueur.
pub const CREATURE_RANGE_BLOCKS: f32 = 96.0;
/// Créatures au plus dans le voisinage d'un joueur avant qu'il n'en apparaisse d'autres
const MAX_NEAR_PLAYER: usize = 10;
/// Distance d'apparition autour d'un joueur, en blocs : hors de sa vue immédiate, mais pas trop loin
const SPAWN_MIN_BLOCKS: f32 = 24.0;
const SPAWN_MAX_BLOCKS: f32 = 56.0;
/// Intervalle entre deux tentatives d'apparition autour de chaque joueur, en secondes
const SPAWN_INTERVAL: f32 = 1.0;
/// Chute, en blocs par seconde²
const GRAVITY: f32 = 20.0;
/// Hauteur de marche franchie d'un saut, en blocs
const STEP_BLOCKS: f32 = 1.0;
/// Élan d'un saut, en blocs par seconde : de quoi monter un peu plus haut qu'une marche
const JUMP_SPEED: f32 = 7.2;
/// Vitesse à laquelle une créature se tourne, en radians par seconde
const TURN_SPEED: f32 = 4.0;
/// Écart au cap visé sous lequel elle le considère atteint, en radians
const FACING_TOLERANCE: f32 = 0.1;
/// Recul d'un coup : vitesse dans le plan, en blocs par seconde, et petit bond
const KNOCKBACK_SPEED: f32 = 6.0;
const KNOCKBACK_HOP: f32 = 4.5;
/// Freinage du recul, par seconde
const KNOCKBACK_DAMPING: f32 = 5.0;
/// Fuite après un coup : durée, en secondes, et allure par rapport à la marche
const PANIC_TIME: f32 = 4.0;
const PANIC_SPEED: f32 = 1.8;
/// Une bête craintive fuit un joueur plus près que ça, en blocs, pendant `FLEE_TIME` secondes avant
/// de jeter un nouveau coup d'œil
const FLEE_RADIUS_BLOCKS: f32 = 8.0;
/// Rayon de fuite face à un joueur accroupi, qui s'approche sans bruit, en blocs
const FLEE_RADIUS_CROUCHING_BLOCKS: f32 = 3.0;
const FLEE_TIME: f32 = 1.5;
/// Une bête placide regarde un joueur plus près que ça, en blocs
const WATCH_RADIUS_BLOCKS: f32 = 6.0;
/// Une bête curieuse suit un joueur plus près que `FOLLOW_RADIUS_BLOCKS` et s'arrête à
/// `FOLLOW_STOP_BLOCKS` de lui ; elle repart quand il s'éloigne au-delà de `FOLLOW_RESUME_BLOCKS`
const FOLLOW_RADIUS_BLOCKS: f32 = 16.0;
const FOLLOW_STOP_BLOCKS: f32 = 3.0;
const FOLLOW_RESUME_BLOCKS: f32 = 4.5;
/// Intervalle entre deux réveils possibles d'une créature endormie, en secondes : les bêtes d'un
/// troupeau ne se lèvent pas toutes ensemble
const SLEEP_CHECK: (f32, f32) = (3.0, 12.0);
/// Fuite des voisines d'une bête frappée, qui n'ont pas reçu le coup : plus courte et moins rapide
const ALARM_TIME: f32 = 1.8;
const ALARM_SPEED: f32 = 1.35;
/// Portée de l'alerte autour de la bête frappée, en blocs
const ALARM_RADIUS_BLOCKS: f32 = 16.0;
/// Voisines du troupeau prises en compte, en blocs
const HERD_RADIUS_BLOCKS: f32 = 20.0;
/// Écart au centre du troupeau au-delà duquel une bête y revient plutôt que d'errer, en blocs
const HERD_LOOSE_BLOCKS: f32 = 4.0;
/// Écart au-delà duquel elle ne s'arrête plus avant d'avoir rejoint les autres, en blocs
const HERD_STRAY_BLOCKS: f32 = 7.0;
/// Chance qu'une bête au repos emboîte le pas de voisines qui marchent, à chaque changement d'humeur
const HERD_FOLLOW_CHANCE: f64 = 0.6;
/// Une créature ne descend pas volontairement plus bas que ça, en blocs : au-delà, c'est une falaise
const MAX_DROP_BLOCKS: f32 = 3.0;
/// Regard devant elle : nombre de points sondés sur son chemin, et écart entre eux, en blocs
const LOOKAHEAD_STEPS: usize = 5;
const PROBE_SPACING: f32 = 0.5;
/// Écarts au cap essayés pour contourner un obstacle, en radians, du plus petit au plus grand
const DETOUR_ANGLES: [f32; 6] = [0.4, 0.8, 1.2, 1.6, 2.1, 2.6];
/// Nage : allure par rapport à la marche, et part du corps sous la surface (au plus un bloc)
const SWIM_SPEED: f32 = 0.6;
const SWIM_SINK: f32 = 0.5;
/// Recherche de la rive : portée, en blocs, directions sondées et intervalle, en secondes
const SHORE_RANGE_BLOCKS: usize = 12;
const SHORE_DIRECTIONS: usize = 16;
const SHORE_SEARCH_INTERVAL: f32 = 1.0;
/// Pas de simulation le plus long, en secondes
const MAX_STEP: f32 = 0.1;
/// Au-delà, le cache de colonnes est vidé
const MAX_CACHED_COLUMNS: usize = 4096;

pub struct Creature {
    pub id: u64,
    /// Rang de son espèce dans le registre
    pub species: usize,
    /// Troupeau : les bêtes apparues ensemble partagent ce numéro (celui de la première)
    pub herd: u64,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est)
    pub yaw: f32,
    /// Cap vers lequel elle se tourne
    target_yaw: f32,
    /// Vitesse verticale, en blocs par seconde (positive vers le haut)
    vertical_speed: f32,
    /// Recul d'un coup reçu, dans le plan tangent, en blocs par seconde
    push: Vec3,
    walking: bool,
    /// Temps restant avant de changer d'humeur (s'arrêter, repartir), en secondes
    timer: f32,
    health: f32,
    /// Temps restant de la teinte rouge d'un coup, en secondes
    hurt: f32,
    /// Temps restant de fuite après un coup, en secondes
    panic: f32,
    /// Allure de la fuite par rapport à la marche : plus vive pour la bête frappée que pour ses voisines
    panic_speed: f32,
    /// Temps écoulé depuis sa mort (`None` : vivante), en secondes
    dead_for: Option<f32>,
    /// Endormie : elle ne bouge plus jusqu'au retour de son heure, ou jusqu'à un coup
    sleeping: bool,
    /// Côté par lequel elle contourne les obstacles (1 : à droite, -1 : à gauche), gardé d'un
    /// obstacle à l'autre pour longer un mur sans hésiter
    detour_side: f32,
    /// Elle nage : ses pieds sont à la surface de l'eau
    swimming: bool,
    /// Enfoncement du corps dans l'eau, en unités du monde
    sink: f32,
}

impl Creature {
    pub fn state(&self) -> CreatureState {
        CreatureState {
            creature_id: self.id,
            // Elle nage le corps à moitié sous l'eau
            position: self.position - self.position.normalize_or_zero() * self.sink,
            yaw: self.yaw,
            hurt: (self.hurt / CREATURE_HURT_TIME).clamp(0.0, 1.0),
            death: self.dead_for.map_or(0.0, |t| (t / CREATURE_DEATH_TIME).clamp(1e-3, 1.0)),
        }
    }

    pub fn is_alive(&self) -> bool {
        self.dead_for.is_none()
    }

    pub fn is_sleeping(&self) -> bool {
        self.sleeping
    }
}

/// Un joueur tel que les créatures le perçoivent
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPresence {
    /// Pieds (repère du centre de la planète)
    pub feet: Vec3,
    /// Accroupi : il approche sans bruit, les bêtes craintives le remarquent de moins loin
    pub crouching: bool,
}

impl PlayerPresence {
    pub fn standing(feet: Vec3) -> Self {
        Self { feet, crouching: false }
    }
}

/// Espèce candidate dans un biome : rang, poids, chance d'apparaître vraiment
type SpawnCandidate = (usize, f32, f32);

pub struct CreatureWorld {
    registry: Arc<CreatureRegistry>,
    /// Espèces qui apparaissent dans chaque biome
    spawn_table: HashMap<BiomeType, Vec<SpawnCandidate>>,
    geometry: BlockGeometry,
    creatures: Vec<Creature>,
    next_id: u64,
    rng: StdRng,
    spawn_timer: f32,
    /// Direction du soleil (repère du centre de la planète), d'où l'heure en chaque lieu. `None` :
    /// pas d'horloge, toutes les espèces restent éveillées.
    sun_dir: Option<Vec3>,
    /// Colonnes du générateur déjà échantillonnées : les créatures restent longtemps sur les mêmes
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl CreatureWorld {
    pub fn new(base_radius: f64, seed: u64, registry: Arc<CreatureRegistry>) -> Self {
        let mut spawn_table: HashMap<BiomeType, Vec<SpawnCandidate>> = HashMap::new();
        for (index, species) in registry.iter().enumerate() {
            for rule in &species.spawn {
                match BiomeType::from_name(&rule.biome) {
                    Some(biome) if rule.weight > 0.0 => spawn_table.entry(biome).or_default().push((index, rule.weight, rule.chance)),
                    Some(_) => {}
                    None => log::warn!("Créature {} : biome « {} » inconnu.", species.id, rule.biome),
                }
            }
        }
        Self {
            registry,
            spawn_table,
            geometry: BlockGeometry::new(base_radius),
            creatures: Vec::new(),
            next_id: 1,
            rng: StdRng::seed_from_u64(seed),
            spawn_timer: 0.0,
            sun_dir: None,
            columns: HashMap::new(),
        }
    }

    pub fn registry(&self) -> &Arc<CreatureRegistry> {
        &self.registry
    }

    pub fn creatures(&self) -> &[Creature] {
        &self.creatures
    }

    /// Règle l'heure : la direction du soleil, vue du centre de la planète
    pub fn set_sun_dir(&mut self, sun_dir: Vec3) {
        self.sun_dir = Some(sun_dir.normalize_or_zero());
    }

    /// Hauteur du soleil en `p` (sinus de son élévation), ou `None` sans horloge
    fn sun_height(&self, p: Vec3) -> Option<f32> {
        sun_height(self.sun_dir, p)
    }

    pub fn clear(&mut self) {
        self.creatures.clear();
        self.columns.clear();
    }

    /// Fait apparaître une créature de l'espèce de rang `species` aux pieds `feet`, et renvoie son
    /// identifiant
    pub fn spawn(&mut self, species: usize, feet: Vec3) -> u64 {
        self.spawn_in_herd(species, feet, None)
    }

    /// Comme `spawn`, mais dans le troupeau `herd` (`None` : elle en commence un nouveau)
    pub fn spawn_in_herd(&mut self, species: usize, feet: Vec3, herd: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let yaw = self.rng.gen_range(0.0..TAU);
        let timer = self.rng.gen_range(0.5..3.0);
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        self.creatures.push(Creature {
            id,
            species,
            herd: herd.unwrap_or(id),
            position: feet,
            yaw,
            target_yaw: yaw,
            vertical_speed: 0.0,
            push: Vec3::ZERO,
            walking: false,
            timer,
            health,
            hurt: 0.0,
            panic: 0.0,
            panic_speed: PANIC_SPEED,
            dead_for: None,
            sleeping: false,
            detour_side: if self.rng.gen_bool(0.5) { 1.0 } else { -1.0 },
            swimming: false,
            sink: 0.0,
        });
        id
    }

    /// Un joueur dont la tête est en `from` frappe la créature `id` : elle perd de la vie, est
    /// repoussée à l'opposé, puis s'enfuit ; les bêtes de son troupeau proches fuient aussi, moins
    /// longtemps et moins vite. Renvoie faux si elle n'existe pas ou est déjà morte.
    pub fn hit(&mut self, id: u64, from: Vec3) -> bool {
        let Some(index) = self.creatures.iter().position(|c| c.id == id && c.is_alive()) else { return false };
        let c = &mut self.creatures[index];
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let away = c.position - from;
        let away = (away - up * away.dot(up)).normalize_or_zero();
        // Frappée d'aplomb : elle recule droit devant elle
        let away = if away == Vec3::ZERO { north * c.yaw.cos() + east * c.yaw.sin() } else { away };
        c.push = away * KNOCKBACK_SPEED;
        c.vertical_speed = c.vertical_speed.max(KNOCKBACK_HOP);
        c.hurt = CREATURE_HURT_TIME;
        c.health -= PLAYER_HIT_DAMAGE;
        c.sleeping = false;
        if c.health <= 0.0 {
            c.dead_for = Some(0.0);
            c.walking = false;
        } else {
            c.panic = PANIC_TIME;
            c.panic_speed = PANIC_SPEED;
            c.walking = true;
            c.target_yaw = away.dot(east).atan2(away.dot(north)).rem_euclid(TAU);
        }
        let (herd, center) = (c.herd, c.position);
        self.alarm_herd(id, herd, center, from);
        true
    }

    /// Les bêtes du troupeau `herd` proches de `center` (sauf la victime `victim`) s'enfuient loin
    /// de l'attaquant en `from`, sans prendre de coup : fuite courte et modérée
    fn alarm_herd(&mut self, victim: u64, herd: u64, center: Vec3, from: Vec3) {
        let radius = ALARM_RADIUS_BLOCKS * self.geometry.block_height as f32;
        for c in &mut self.creatures {
            if c.id == victim || c.herd != herd || !c.is_alive() || c.position.distance(center) > radius {
                continue;
            }
            // Déjà en fuite plus longue (frappée elle-même, ou déjà alertée) : elle la garde
            if c.panic >= ALARM_TIME {
                continue;
            }
            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let away = c.position - from;
            let away = (away - up * away.dot(up)).normalize_or_zero();
            if away != Vec3::ZERO {
                let jitter = self.rng.gen_range(-0.35..0.35);
                c.target_yaw = (away.dot(east).atan2(away.dot(north)) + jitter).rem_euclid(TAU);
            }
            c.panic = ALARM_TIME;
            c.panic_speed = ALARM_SPEED;
            c.walking = true;
        }
    }

    /// Avance la simulation de `dt` secondes autour des joueurs debout dont les pieds sont `players`
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, players: &[Vec3]) {
        let presences: Vec<PlayerPresence> = players.iter().copied().map(PlayerPresence::standing).collect();
        self.tick_with_presences(dt, generator, edits, &presences);
    }

    /// Avance la simulation de `dt` secondes autour des joueurs `players`, en tenant compte de leur
    /// posture (accroupi ou debout)
    pub fn tick_with_presences(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, presences: &[PlayerPresence]) {
        let players: Vec<Vec3> = presences.iter().map(|p| p.feet).collect();
        let players = players.as_slice();
        // Une image très longue (fenêtre en arrière-plan) ne fait pas traverser les murs
        let dt = dt.min(MAX_STEP);
        let range = CREATURE_RANGE_BLOCKS * self.geometry.block_height as f32;
        self.creatures.retain(|c| c.dead_for.is_none_or(|t| t < CREATURE_DEATH_TIME) && players.iter().any(|p| p.distance(c.position) <= range));

        let columns = std::mem::take(&mut self.columns);
        let mut query = TerrainQuery::with_columns(generator, edits, columns);
        let herds = herd_pulls(&self.creatures, self.geometry.block_height as f32);
        let mut walker = Walker { geometry: self.geometry, query: &mut query, sun_dir: self.sun_dir };
        for (creature, herd) in self.creatures.iter_mut().zip(herds) {
            if let Some(species) = self.registry.get(creature.species) {
                let nearest = presences.iter().copied().min_by(|a, b| a.feet.distance_squared(creature.position).total_cmp(&b.feet.distance_squared(creature.position)));
                walker.step(creature, species, nearest, herd, &mut self.rng, dt);
            }
        }

        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = SPAWN_INTERVAL;
            for &player in players {
                self.try_spawn_near(&mut query, player);
            }
        }

        let mut columns = query.into_columns();
        if columns.len() > MAX_CACHED_COLUMNS {
            columns.clear();
        }
        self.columns = columns;
    }

    /// Tente de faire apparaître un groupe sur le relief autour d'un joueur, selon le biome
    fn try_spawn_near(&mut self, query: &mut TerrainQuery, player: Vec3) {
        let block = self.geometry.block_height as f32;
        let near = self.creatures.iter().filter(|c| c.position.distance(player) <= CREATURE_RANGE_BLOCKS * block).count();
        if near >= MAX_NEAR_PLAYER || self.creatures.len() >= MAX_CREATURES || !self.rng.gen_bool(0.5) {
            return;
        }
        let up = player.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let angle = self.rng.gen_range(0.0..TAU);
        let distance = self.rng.gen_range(SPAWN_MIN_BLOCKS..SPAWN_MAX_BLOCKS) * block;
        let center = player + (north * angle.cos() + east * angle.sin()) * distance;
        let Some(column) = self.geometry.block_at_point(center.as_dvec3()) else { return };
        if surface_feet(&self.geometry, query, column.face, column.x, column.z).is_none() {
            return;
        }
        let biome = query.column_sample(column.face, column.x, column.z).biome;
        let Some(candidates) = self.spawn_table.get(&biome) else { return };
        // Seules les espèces éveillées à cette heure-ci, à cet endroit, apparaissent
        let candidates: Vec<SpawnCandidate> = match self.sun_height(center) {
            Some(height) => candidates
                .iter()
                .copied()
                .filter(|&(species, _, _)| self.registry.get(species).is_some_and(|s| s.activity.is_awake(height)))
                .collect(),
            None => candidates.clone(),
        };
        let Some(species) = pick_species(&candidates, &mut self.rng) else { return };
        let Some((min_group, max_group)) = self.registry.get(species).map(|s| (s.min_group, s.max_group)) else { return };

        let group = self.rng.gen_range(min_group..=max_group);
        let mut herd = None;
        for _ in 0..group {
            if self.creatures.len() >= MAX_CREATURES {
                break;
            }
            let (dx, dz) = (self.rng.gen_range(-3..=3), self.rng.gen_range(-3..=3));
            let pos = BlockPos::new(column.face, column.x + dx, 0, column.z + dz);
            if !pos.is_valid() {
                continue;
            }
            if let Some(feet) = surface_feet(&self.geometry, query, pos.face, pos.x, pos.z) {
                let id = self.spawn_in_herd(species, feet, herd);
                herd.get_or_insert(id);
            }
        }
    }
}

/// Hauteur du soleil `sun_dir` vue de `p` (sinus de son élévation), ou `None` sans horloge
fn sun_height(sun_dir: Option<Vec3>, p: Vec3) -> Option<f32> {
    sun_dir.map(|sun| sun.dot(p.normalize_or_zero()))
}

/// Pieds d'une créature posée au milieu du dessus de la colonne (x, z), si ce dessus est un sol où
/// elle peut vivre (herbe, terre, sable, neige : ni eau, ni feuillage, ni roche nue)
fn surface_feet(geometry: &BlockGeometry, query: &mut TerrainQuery, face: PlanetFace, x: i32, z: i32) -> Option<Vec3> {
    let top = query.support_top(face, x, z, WORLD_HEIGHT - 1);
    if top <= 0 || top >= WORLD_HEIGHT - 2 {
        return None;
    }
    let surface = query.block(BlockPos::new(face, x, top - 1, z));
    if ![BlockId::GRASS, BlockId::DIRT, BlockId::SAND, BlockId::SNOW].contains(&surface) {
        return None;
    }
    Some(geometry.corner(face, x as f64 + 0.5, top as f64, z as f64 + 0.5).as_vec3())
}

/// Espèce tirée au poids parmi les candidates d'un biome, ou aucune si elle n'a pas eu sa chance
fn pick_species(candidates: &[SpawnCandidate], rng: &mut StdRng) -> Option<usize> {
    let total: f32 = candidates.iter().map(|&(_, weight, _)| weight).sum();
    if total <= 0.0 {
        return None;
    }
    let mut roll = rng.gen_range(0.0..total);
    let &(species, _, chance) = candidates.iter().find(|&&(_, weight, _)| {
        if roll < weight {
            true
        } else {
            roll -= weight;
            false
        }
    })?;
    (chance >= 1.0 || rng.gen_bool(chance.clamp(0.0, 1.0) as f64)).then_some(species)
}

/// Ce qu'une bête sait de son troupeau autour d'elle
#[derive(Clone, Copy, Default)]
struct HerdPull {
    /// Cap vers le centre de ses voisines, et distance à ce centre en blocs
    toward: Option<(f32, f32)>,
    /// Cap moyen des voisines qui marchent
    heading: Option<f32>,
}

/// Pour chaque créature, la position et l'allure des autres bêtes vivantes de son troupeau
fn herd_pulls(creatures: &[Creature], block: f32) -> Vec<HerdPull> {
    let radius = HERD_RADIUS_BLOCKS * block;
    let heading_of = |c: &Creature| {
        let (east, north) = tangent_frame(c.position.normalize_or_zero());
        north * c.yaw.cos() + east * c.yaw.sin()
    };
    creatures
        .iter()
        .map(|c| {
            if !c.is_alive() {
                return HerdPull::default();
            }
            let (mut sum, mut count, mut heading, mut walkers) = (Vec3::ZERO, 0, Vec3::ZERO, 0);
            for o in creatures {
                if o.id == c.id || o.herd != c.herd || !o.is_alive() || o.position.distance(c.position) > radius {
                    continue;
                }
                sum += o.position;
                count += 1;
                if o.walking && o.panic <= 0.0 {
                    heading += heading_of(o);
                    walkers += 1;
                }
            }
            if count == 0 {
                return HerdPull::default();
            }
            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let yaw_of = |v: Vec3| {
                let v = v - up * v.dot(up);
                (v.length_squared() > 1e-12).then(|| v.dot(east).atan2(v.dot(north)).rem_euclid(TAU))
            };
            let offset = sum / count as f32 - c.position;
            let distance = (offset - up * offset.dot(up)).length() / block;
            HerdPull {
                toward: yaw_of(offset).map(|yaw| (yaw, distance)),
                heading: if walkers > 0 { yaw_of(heading) } else { None },
            }
        })
        .collect()
}

/// Sol sous un point du chemin
#[derive(Clone, Copy, PartialEq, Debug)]
enum Footing {
    Dry,
    Water,
}

/// Ce qu'une créature voit devant elle dans une direction
#[derive(Clone, Copy, Debug)]
struct Path {
    /// Points sondés franchissables d'affilée, de 0 (bloquée tout de suite) à `LOOKAHEAD_STEPS`
    clear: usize,
    /// Une partie du chemin franchissable passe dans l'eau
    wet: bool,
    /// Altitude du sol juste devant le museau, s'il est franchissable
    first: Option<f32>,
}

impl Path {
    /// Note d'un chemin : le plus long d'abord, puis au sec si `avoid_water`
    fn score(&self, avoid_water: bool) -> usize {
        self.clear * 2 + usize::from(!avoid_water || !self.wet)
    }
}

/// Déplacement d'une créature sur le terrain
struct Walker<'q, 'a> {
    geometry: BlockGeometry,
    query: &'q mut TerrainQuery<'a>,
    sun_dir: Option<Vec3>,
}

impl Walker<'_, '_> {
    fn block(&self) -> f32 {
        self.geometry.block_height as f32
    }

    /// Altitude des pieds, en blocs depuis le fond du monde
    fn altitude(&self, p: Vec3) -> f32 {
        ((p.length() as f64 - self.geometry.base_radius) / self.geometry.block_height) as f32
    }

    /// Altitude du sol sous `p` pour des pieds à `altitude` : dessus du plus haut bloc porteur qui ne
    /// dépasse pas les pieds de plus d'une marche, et la colonne où il est
    fn ground(&mut self, p: Vec3, altitude: f32) -> Option<(BlockPos, f32)> {
        let column = self.geometry.block_at_point(p.as_dvec3())?;
        let from_y = (altitude + STEP_BLOCKS).floor() as i32 - 1;
        let top = self.query.support_top(column.face, column.x, column.z, from_y);
        Some((BlockPos { y: top, ..column }, top as f32))
    }

    /// Rien de solide dans la colonne `column` entre les altitudes `altitude` et `altitude + height`
    fn fits(&mut self, column: BlockPos, altitude: f32, height: f32) -> bool {
        let (bottom, top) = ((altitude + 1e-3).floor() as i32, (altitude + height - 1e-3).floor() as i32);
        (bottom..=top).all(|y| {
            let pos = BlockPos { y, ..column };
            !pos.is_valid() || !self.query.block(pos).is_solid()
        })
    }

    /// Réaction de la créature au joueur le plus proche, selon son tempérament. Renvoie vrai si elle
    /// s'en occupe, et laisse alors son humeur de côté.
    fn react(&self, c: &mut Creature, temperament: Temperament, player: PlayerPresence, rng: &mut StdRng) -> bool {
        let flee_radius = if player.crouching { FLEE_RADIUS_CROUCHING_BLOCKS } else { FLEE_RADIUS_BLOCKS };
        let player = player.feet;
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let towards = player - c.position;
        let towards = towards - up * towards.dot(up);
        let distance = towards.length() / self.block();
        // Joueur juste au-dessus ou au-dessous d'elle : pas de direction où regarder ni fuir
        if distance < 0.25 {
            return false;
        }
        let yaw_to_player = towards.dot(east).atan2(towards.dot(north)).rem_euclid(TAU);
        match temperament {
            Temperament::Indifferent => false,
            Temperament::Craintif if distance < flee_radius => {
                // À l'opposé du joueur, un peu de travers pour ne pas filer toutes en ligne
                c.panic = FLEE_TIME;
                c.panic_speed = PANIC_SPEED;
                c.walking = true;
                c.target_yaw = (yaw_to_player + PI + rng.gen_range(-0.4..0.4)).rem_euclid(TAU);
                true
            }
            Temperament::Placide if distance < WATCH_RADIUS_BLOCKS => {
                c.walking = false;
                c.target_yaw = yaw_to_player;
                // Une fois le joueur parti, elle reprend vite sa promenade
                c.timer = c.timer.min(1.5);
                true
            }
            Temperament::Curieux if distance < FOLLOW_RADIUS_BLOCKS => {
                // Elle s'arrête près de lui, et ne repart que s'il s'éloigne franchement
                let stop = if c.walking { FOLLOW_STOP_BLOCKS } else { FOLLOW_RESUME_BLOCKS };
                c.walking = distance > stop;
                c.target_yaw = yaw_to_player;
                c.timer = c.timer.min(1.5);
                true
            }
            _ => false,
        }
    }

    /// Où poser les pieds à la place de `p`, en venant de pieds à `altitude` : rien si un mur
    /// barre la route, si c'est une falaise, de la lave, ou si le corps n'y tient pas
    fn footing(&mut self, p: Vec3, altitude: f32, height: f32) -> Option<(f32, Footing)> {
        let (column, ground) = self.ground(p, altitude)?;
        if ground < altitude - MAX_DROP_BLOCKS {
            return None;
        }
        let below = self.query.block(BlockPos { y: column.y - 1, ..column });
        if below == BlockId::LAVA || !self.fits(column, ground, height) {
            return None;
        }
        Some((ground, if below.is_liquid() { Footing::Water } else { Footing::Dry }))
    }

    /// Sonde le chemin depuis `from` (pieds à `altitude`) dans la direction `heading`, à partir de
    /// `reach` blocs devant le centre du corps
    fn path(&mut self, from: Vec3, altitude: f32, heading: Vec3, reach: f32, height: f32) -> Path {
        let block = self.block();
        let mut path = Path { clear: 0, wet: false, first: None };
        let mut alt = altitude;
        for k in 0..LOOKAHEAD_STEPS {
            let p = from + heading * (reach + k as f32 * PROBE_SPACING) * block;
            let Some((ground, footing)) = self.footing(p, alt, height) else { break };
            if k == 0 {
                path.first = Some(ground);
            }
            alt = ground;
            path.wet |= footing == Footing::Water;
            path.clear += 1;
        }
        path
    }

    /// Nouveau cap pour contourner ce qui gêne sur le chemin `current` : le plus petit écart qui
    /// mène plus loin (ou au sec), du côté habituel d'abord. Bloquée de partout, elle fait demi-tour.
    #[allow(clippy::too_many_arguments)]
    fn detour(&mut self, c: &mut Creature, altitude: f32, reach: f32, height: f32, current: Path, avoid_water: bool, rng: &mut StdRng) -> Option<f32> {
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let perfect = LOOKAHEAD_STEPS * 2 + 1;
        // (écart signé, note) du meilleur cap trouvé : il doit faire mieux que le cap actuel
        let mut best: Option<(f32, usize)> = None;
        'search: for angle in DETOUR_ANGLES {
            for side in [c.detour_side, -c.detour_side] {
                let yaw = c.yaw + side * angle;
                let score = self.path(c.position, altitude, north * yaw.cos() + east * yaw.sin(), reach, height).score(avoid_water);
                if score > best.map_or(current.score(avoid_water), |(_, s)| s) {
                    best = Some((side * angle, score));
                    if score == perfect {
                        break 'search;
                    }
                }
            }
        }
        match best {
            Some((offset, _)) => {
                c.detour_side = offset.signum();
                Some((c.yaw + offset).rem_euclid(TAU))
            }
            // Cul-de-sac : demi-tour, un peu de travers, et elle essaiera l'autre côté la prochaine fois
            None if current.clear == 0 => {
                c.detour_side = -c.detour_side;
                Some((c.yaw + PI + rng.gen_range(-0.8..0.8)).rem_euclid(TAU))
            }
            None => None,
        }
    }

    /// Cap vers la rive la plus proche où elle peut sortir de l'eau, depuis des pieds à `altitude`
    fn shore_yaw(&mut self, from: Vec3, altitude: f32, height: f32) -> Option<f32> {
        let up = from.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let block = self.block();
        for r in 1..=SHORE_RANGE_BLOCKS {
            for i in 0..SHORE_DIRECTIONS {
                let yaw = i as f32 * TAU / SHORE_DIRECTIONS as f32;
                let p = from + (north * yaw.cos() + east * yaw.sin()) * r as f32 * block;
                if matches!(self.footing(p, altitude, height), Some((_, Footing::Dry))) {
                    return Some(yaw);
                }
            }
        }
        None
    }

    fn step(&mut self, c: &mut Creature, species: &CreatureSpecies, nearest_player: Option<PlayerPresence>, herd: HerdPull, rng: &mut StdRng, dt: f32) {
        let block = self.block();
        let base_radius = self.geometry.base_radius as f32;
        let radius_of = move |altitude: f32| base_radius + altitude * block;

        c.hurt = (c.hurt - dt).max(0.0);
        if let Some(t) = c.dead_for.as_mut() {
            *t += dt;
        } else if c.panic > 0.0 {
            // Fuite : elle court droit devant, jusqu'à se calmer
            c.panic -= dt;
            if c.panic <= 0.0 {
                c.walking = false;
                c.timer = rng.gen_range(2.0..6.0);
            }
        } else if c.swimming {
            // À l'eau : elle nage vers la rive la plus proche, qu'elle cherche de temps en temps (on ne
            // dort pas en nageant)
            c.sleeping = false;
            c.walking = true;
            c.timer -= dt;
            if c.timer <= 0.0 {
                c.timer = SHORE_SEARCH_INTERVAL;
                let altitude = self.altitude(c.position);
                if let Some(yaw) = self.shore_yaw(c.position, altitude, species.body.height) {
                    c.target_yaw = yaw;
                }
            }
        } else if !c.sleeping && nearest_player.is_some_and(|player| self.react(c, species.temperament, player, rng)) {
            // Éveillée, elle s'occupe du joueur ; endormie, elle ne le remarque pas (un coup la réveille)
        } else {
            // Humeur : marcher un moment dans une direction, puis s'arrêter brouter. Au sein d'un
            // troupeau, elle revient vers les autres si elle s'en écarte, et suit celles qui marchent.
            // Hors de ses heures, elle s'endort là où elle est, et au retour de ses heures se réveille.
            // Chacune le fait à la fin de son humeur du moment : un troupeau s'endort et s'éveille peu
            // à peu.
            let gap = herd.toward.map_or(0.0, |(_, distance)| distance);
            let straying = gap > HERD_STRAY_BLOCKS && !c.sleeping;
            c.timer -= dt;
            // Seule loin des autres, elle n'attend pas la fin de sa pause pour les rejoindre
            if straying && !c.walking {
                c.timer = c.timer.min(0.5);
            }
            if c.timer <= 0.0 {
                let awake = sun_height(self.sun_dir, c.position).is_none_or(|height| species.activity.is_awake(height));
                // Au repos, elle emboîte volontiers le pas des voisines qui marchent
                let follow = herd.heading.filter(|_| awake && !c.sleeping && !c.walking && rng.gen_bool(HERD_FOLLOW_CHANCE));
                if !awake {
                    c.sleeping = true;
                    c.walking = false;
                    c.target_yaw = c.yaw;
                    c.timer = rng.gen_range(SLEEP_CHECK.0..SLEEP_CHECK.1);
                } else if c.sleeping {
                    // Elle s'étire un moment avant de repartir
                    c.sleeping = false;
                    c.walking = false;
                    c.timer = rng.gen_range(1.0..3.0);
                } else if !straying && follow.is_none() && (c.walking || rng.gen_bool(0.4)) {
                    c.walking = false;
                    c.timer = rng.gen_range(2.0..6.0);
                    if rng.gen_bool(0.5) {
                        c.target_yaw = c.yaw + rng.gen_range(-1.0..1.0);
                    }
                } else {
                    c.walking = true;
                    c.timer = rng.gen_range(1.0..4.0);
                    c.target_yaw = match (herd.toward, follow) {
                        // Écartée : droit vers le centre du troupeau
                        (Some((toward, distance)), _) if distance > HERD_LOOSE_BLOCKS => toward + rng.gen_range(-0.3..0.3),
                        // Dans le troupeau : le cap de celles qui marchent
                        (_, Some(heading)) => heading + rng.gen_range(-0.4..0.4),
                        _ => c.yaw + rng.gen_range(-PI / 2.0..PI / 2.0),
                    };
                }
                c.target_yaw = c.target_yaw.rem_euclid(TAU);
            }
        }

        // Elle se tourne peu à peu vers le cap visé, par le plus court
        let turn = angle_between(c.yaw, c.target_yaw);
        let max_turn = TURN_SPEED * dt;
        c.yaw = (c.yaw + turn.clamp(-max_turn, max_turn)).rem_euclid(TAU);
        let turn = angle_between(c.yaw, c.target_yaw);

        // Pesanteur : posée sur le sol, poussée dessus si un bloc l'a recouverte, sinon elle saute ou tombe
        let up = c.position.normalize_or_zero();
        let mut altitude = self.altitude(c.position);
        let Some((column, ground)) = self.ground(c.position, altitude) else { return };
        if altitude <= ground + 1e-4 && c.vertical_speed <= 0.0 {
            altitude = ground;
            c.vertical_speed = 0.0;
        } else {
            c.vertical_speed -= GRAVITY * dt;
            let next = altitude + c.vertical_speed * dt;
            if c.vertical_speed > 0.0 && !self.fits(column, next, species.body.height) {
                // La tête cogne un plafond
                c.vertical_speed = 0.0;
            } else {
                altitude = next.max(ground);
            }
            if altitude <= ground {
                c.vertical_speed = 0.0;
            }
        }
        c.position = up * radius_of(altitude);
        let on_ground = altitude <= ground + 1e-4 && c.vertical_speed <= 0.0;
        let swimming = on_ground && self.query.block(BlockPos { y: column.y - 1, ..column }).is_liquid();
        if swimming && !c.swimming {
            // Tout juste à l'eau : elle cherche la rive sans attendre
            c.timer = 0.0;
        }
        c.swimming = swimming;
        c.sink = if swimming { (species.body.height * SWIM_SINK).min(1.0) * block } else { 0.0 };

        // Déplacement dans le plan : recul d'un coup, et marche (ralentie le temps de se tourner)
        let (east, north) = tangent_frame(up);
        c.push = (c.push - up * c.push.dot(up)) * (-KNOCKBACK_DAMPING * dt).exp();
        if c.push.length_squared() < 1e-4 {
            c.push = Vec3::ZERO;
        }
        let mut velocity = c.push;
        let pace = if c.panic > 0.0 { c.panic_speed } else { 1.0 } * if c.swimming { SWIM_SPEED } else { 1.0 };
        let speed = species.walk_speed * pace * turn.cos().max(0.0);
        if c.walking && c.dead_for.is_none() && speed > 0.0 {
            // Elle regarde quelques pas devant elle : le sol juste devant le museau décide si elle
            // avance, la suite si elle doit déjà obliquer pour contourner un mur, une falaise ou l'eau
            let heading = north * c.yaw.cos() + east * c.yaw.sin();
            let reach = speed * dt + species.body.half_length;
            let path = self.path(c.position, altitude, heading, reach, species.body.height);
            // Au calme et à pied sec, elle longe l'eau plutôt que d'y entrer, si elle le peut
            let avoid_water = c.panic <= 0.0 && !c.swimming;
            if turn.abs() < FACING_TOLERANCE && path.score(avoid_water) < LOOKAHEAD_STEPS * 2 + 1 {
                if let Some(yaw) = self.detour(c, altitude, reach, species.body.height, path, avoid_water, rng) {
                    c.target_yaw = yaw;
                }
            }
            if let Some(ahead) = path.first {
                // Une marche devant : elle saute dessus (et sort ainsi de l'eau)
                if on_ground && ahead > altitude + 1e-3 {
                    c.vertical_speed = JUMP_SPEED;
                }
                velocity += heading * speed;
            }
        }
        if velocity == Vec3::ZERO {
            return;
        }
        // Elle n'entre dans une colonne que si son corps y passe à son altitude : une marche se
        // franchit en sautant par-dessus
        let target = (c.position + velocity * dt * block).normalize_or_zero() * radius_of(altitude);
        let free = match self.ground(target, altitude) {
            Some((column, ground)) => {
                let below = self.query.block(BlockPos { y: column.y - 1, ..column });
                ground <= altitude + 1e-3 && below != BlockId::LAVA && self.fits(column, altitude, species.body.height)
            }
            None => false,
        };
        if free {
            c.position = target;
        } else {
            c.push = Vec3::ZERO;
        }
    }
}

/// Angle signé pour aller du cap `from` au cap `to` par le plus court, dans [-π, π]
fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::edits::FACE_BLOCKS;

    fn registry() -> Arc<CreatureRegistry> {
        Arc::new(CreatureRegistry::load_default())
    }

    fn generator() -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed: 4242, ..Default::default() })
    }

    /// Une colonne d'herbe de la face +Z, et les pieds d'une créature posée dessus
    fn grass_column(generator: &PlanetWorldGenerator, edits: &TerrainEdits) -> (BlockPos, Vec3) {
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(generator, edits);
        (16..FACE_BLOCKS - 16)
            .step_by(23)
            .flat_map(|x| (16..FACE_BLOCKS - 16).step_by(23).map(move |z| (x, z)))
            .find_map(|(x, z)| {
                let feet = surface_feet(&geometry, &mut query, PlanetFace::PosZ, x, z)?;
                let top = query.support_top(PlanetFace::PosZ, x, z, WORLD_HEIGHT - 1);
                // Terrain plat autour : les voisines à la même hauteur
                let flat = [(1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2)]
                    .iter()
                    .all(|(dx, dz)| query.support_top(PlanetFace::PosZ, x + dx, z + dz, WORLD_HEIGHT - 1) == top);
                flat.then_some((BlockPos::new(PlanetFace::PosZ, x, top, z), feet))
            })
            .expect("une colonne d'herbe sur terrain plat")
    }

    fn altitude(geometry: &BlockGeometry, p: Vec3) -> f32 {
        ((p.length() as f64 - geometry.base_radius) / geometry.block_height) as f32
    }

    #[test]
    fn creatures_appear_around_a_player_and_walk_on_the_ground() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 7, registry());
        // Une minute de jeu à 20 images par seconde
        let mut starts = HashMap::new();
        for _ in 0..1200 {
            world.tick(0.05, &generator, &edits, &[player]);
            for c in world.creatures() {
                starts.entry(c.id).or_insert(c.position);
            }
        }
        assert!(!world.creatures().is_empty(), "aucune créature n'est apparue");
        assert!(world.creatures().len() <= MAX_NEAR_PLAYER + 4);
        let mut query = TerrainQuery::new(&generator, &edits);
        let mut moved = 0;
        for c in world.creatures() {
            let block = c.position.distance(player) / geometry.block_height as f32;
            assert!(block <= CREATURE_RANGE_BLOCKS, "{} à {block} blocs", c.species);
            // Debout sur le sol de sa colonne (ou en train d'y tomber), jamais dedans
            let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
            let alt = altitude(&geometry, c.position);
            let ground = query.support_top(column.face, column.x, column.z, (alt + 1.0).floor() as i32 - 1) as f32;
            assert!(alt >= ground - 1e-3, "{} enfoncée dans le sol ({alt} < {ground})", c.species);
            assert!(!query.block(BlockPos { y: (alt + 1e-2).floor() as i32, ..column }).is_solid(), "{} dans un bloc ({alt})", c.species);
            if starts[&c.id].distance(c.position) > geometry.block_height as f32 {
                moved += 1;
            }
        }
        assert!(moved > 0, "aucune créature n'a bougé");
    }

    #[test]
    fn creatures_disappear_far_from_every_player() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 1, registry());
        let sheep = world.registry().find("vache").unwrap();
        world.spawn(sheep, feet);
        world.tick(0.05, &generator, &edits, &[feet]);
        assert_eq!(world.creatures().len(), 1);
        world.tick(0.05, &generator, &edits, &[-feet]);
        assert!(world.creatures().is_empty());
    }

    #[test]
    fn a_wall_stops_a_creature_and_a_hole_makes_it_fall() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        let boar = world.registry().find("sanglier").unwrap();
        world.spawn(boar, feet);

        // Mur de deux blocs tout autour : le sanglier ne passe pas, quel que soit son cap
        for (dx, dz) in [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)] {
            for dy in 0..2 {
                assert!(edits.set_block(&generator, BlockPos { x: ground.x + dx, y: ground.y + dy, z: ground.z + dz, ..ground }, BlockId::STONE));
            }
        }
        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let c = &world.creatures()[0];
        assert_eq!(geometry.block_at_point(c.position.as_dvec3()).map(|p| (p.x, p.z)), Some((ground.x, ground.z)));

        // Le sol creusé sous lui : il tombe d'un bloc
        assert!(edits.set_block(&generator, BlockPos { y: ground.y - 1, ..ground }, BlockId::AIR));
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let alt = altitude(&geometry, world.creatures()[0].position);
        assert!((alt - (ground.y - 1) as f32).abs() < 1e-2, "altitude {alt} au lieu de {}", ground.y - 1);
    }

    /// Cap qui mène du centre de la colonne `from` vers celui de `to`
    fn yaw_towards(geometry: &BlockGeometry, from: Vec3, to: BlockPos) -> f32 {
        let target = geometry.corner(to.face, to.x as f64 + 0.5, to.y as f64, to.z as f64 + 0.5).as_vec3();
        let up = from.normalize();
        let (east, north) = tangent_frame(up);
        let d = target - from;
        d.dot(east).atan2(d.dot(north)).rem_euclid(TAU)
    }

    #[test]
    fn a_creature_turns_progressively() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        let cow = world.registry().find("vache").unwrap();
        world.spawn(cow, feet);
        let c = &mut world.creatures[0];
        (c.yaw, c.target_yaw, c.timer) = (0.2, 0.2 + PI * 0.9, 100.0);

        let mut yaws = vec![0.2];
        for _ in 0..30 {
            world.tick(0.05, &generator, &edits, &[feet]);
            yaws.push(world.creatures()[0].yaw);
        }
        for pair in yaws.windows(2) {
            let turn = angle_between(pair[0], pair[1]);
            assert!((0.0..=TURN_SPEED * 0.05 + 1e-4).contains(&turn), "demi-tour d'un coup : {turn} rad en une image");
        }
        assert!(angle_between(*yaws.last().unwrap(), 0.2 + PI * 0.9).abs() < 1e-3, "cap jamais atteint");
    }

    #[test]
    fn a_creature_jumps_onto_a_step() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let boar = world.registry().find("sanglier").unwrap();
        world.spawn(boar, feet);
        // Une marche d'un bloc devant (un mur derrière en ferait un cul-de-sac, qu'il contournerait)
        let step = BlockPos { x: ground.x + 1, ..ground };
        assert!(edits.set_block(&generator, step, BlockId::STONE));
        let c = &mut world.creatures[0];
        c.yaw = yaw_towards(&geometry, feet, step);
        (c.target_yaw, c.walking, c.timer) = (c.yaw, true, 100.0);

        let base = ground.y as f32;
        let (mut highest, mut midair, mut on_step) = (base, 0, false);
        for _ in 0..80 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let position = world.creatures()[0].position;
            let alt = altitude(&geometry, position);
            highest = highest.max(alt);
            if alt > base + 0.05 && (alt - base - 1.0).abs() > 0.05 {
                midair += 1;
            }
            let column = geometry.block_at_point(position.as_dvec3()).unwrap();
            on_step |= (column.x, column.z) == (step.x, step.z) && (alt - (base + 1.0)).abs() < 1e-2;
        }
        assert!(on_step, "il n'est pas monté sur la marche");
        assert!(highest > base + 1.05, "pas de saut au-dessus de la marche ({highest})");
        assert!(midair >= 3, "montée d'un coup, sans saut ({midair} images en l'air)");
    }

    #[test]
    fn a_hit_hurts_pushes_and_finally_kills() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 11, registry());
        let rabbit = world.registry().find("lapin").unwrap();
        let id = world.spawn(rabbit, feet);
        assert!(!world.hit(id + 1, feet), "pas de créature à toucher");

        // Frappée depuis l'ouest : elle rougit et part vers l'est
        let (east, _) = tangent_frame(feet.normalize());
        let attacker = feet - east * 2.0 * geometry.block_height as f32;
        assert!(world.hit(id, attacker));
        assert_eq!(world.creatures()[0].state().hurt, 1.0);
        for _ in 0..10 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let c = &world.creatures()[0];
        let moved = (c.position - feet).dot(east) / geometry.block_height as f32;
        assert!(moved > 0.5, "recul de {moved} bloc seulement");
        assert!(c.state().hurt < 1.0 && c.state().death == 0.0);

        // Assez de coups : elle bascule, puis disparaît
        let health = world.registry().get(rabbit).unwrap().health;
        let hits = (health / PLAYER_HIT_DAMAGE).ceil() as usize;
        for _ in 1..hits {
            assert!(world.hit(id, attacker));
        }
        assert!(world.creatures()[0].state().death > 0.0);
        assert!(!world.hit(id, attacker), "une bête morte ne se frappe plus");
        for _ in 0..((CREATURE_DEATH_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        assert!(world.creatures().is_empty());
    }

    /// Distance dans le plan tangent entre deux pieds, en blocs
    fn planar_blocks(geometry: &BlockGeometry, a: Vec3, b: Vec3) -> f32 {
        let up = a.normalize();
        let d = b - a;
        (d - up * d.dot(up)).length() / geometry.block_height as f32
    }

    /// Une créature de l'espèce `id` posée sur l'herbe, et un joueur à `blocks` blocs à l'est d'elle
    fn creature_and_player(id: &str, blocks: f32, seed: u64) -> (PlanetWorldGenerator, TerrainEdits, BlockGeometry, CreatureWorld, Vec3) {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), seed, registry());
        let species = world.registry().find(id).unwrap();
        world.spawn(species, feet);
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet + east * blocks * geometry.block_height as f32;
        (generator, edits, geometry, world, player)
    }

    #[test]
    fn bundled_species_have_their_temperament() {
        let registry = registry();
        let temperament = |id: &str| registry.get(registry.find(id).unwrap()).unwrap().temperament;
        for id in ["lapin", "cerf", "faisan"] {
            assert_eq!(temperament(id), Temperament::Craintif, "{id}");
        }
        assert_eq!(temperament("vache"), Temperament::Placide);
    }

    #[test]
    fn a_shy_creature_flees_an_approaching_player() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("lapin", 4.0, 13);
        let start = planar_blocks(&geometry, world.creatures()[0].position, player);
        let mut fled = false;
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
            fled |= world.creatures()[0].walking && world.creatures()[0].panic > 0.0;
        }
        let end = planar_blocks(&geometry, world.creatures()[0].position, player);
        assert!(fled, "le lapin n'a pas détalé");
        assert!(end > start + 1.5, "le lapin ne s'est pas éloigné ({start} → {end} blocs)");
    }

    #[test]
    fn a_crouching_player_can_approach_a_shy_creature() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("lapin", 5.0, 29);
        let c = &mut world.creatures[0];
        (c.walking, c.timer) = (false, 100.0);
        let start = world.creatures()[0].position;
        let crouching = PlayerPresence { feet: player, crouching: true };
        for _ in 0..40 {
            world.tick_with_presences(0.05, &generator, &edits, &[crouching]);
        }
        let c = &world.creatures()[0];
        assert!(c.panic == 0.0 && !c.walking, "le lapin a remarqué le joueur accroupi à 5 blocs");
        assert!(planar_blocks(&geometry, start, c.position) < 0.1);

        // Accroupi tout près, il finit par le remarquer
        let (east, _) = tangent_frame(c.position.normalize());
        let close = PlayerPresence { feet: c.position + east * 2.0 * geometry.block_height as f32, crouching: true };
        world.tick_with_presences(0.05, &generator, &edits, &[close]);
        assert!(world.creatures()[0].panic > 0.0, "le lapin ne fuit pas un joueur accroupi à 2 blocs");
    }

    #[test]
    fn a_placid_creature_stops_and_watches_the_player() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("vache", 4.0, 17);
        let c = &mut world.creatures[0];
        (c.yaw, c.target_yaw, c.walking, c.timer) = (PI, PI, true, 100.0);
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let c = &world.creatures()[0];
        assert!(!c.walking, "la vache continue sa promenade");
        let up = c.position.normalize();
        let (east, north) = tangent_frame(up);
        let d = player - c.position;
        let towards = d.dot(east).atan2(d.dot(north));
        assert!(angle_between(c.yaw, towards).abs() < 0.05, "la vache ne regarde pas le joueur");
        assert!(planar_blocks(&geometry, c.position, player) > 3.0, "la vache est venue vers le joueur");
    }

    #[test]
    fn a_curious_creature_follows_the_player_and_keeps_its_distance() {
        // Aucune espèce livrée n'est curieuse : le chien l'est le temps du test
        let species = registry()
            .iter()
            .cloned()
            .map(|mut s| {
                if s.id == "chien" {
                    s.temperament = Temperament::Curieux;
                }
                s
            })
            .collect();
        let registry = Arc::new(CreatureRegistry::from_species(species, Default::default()));
        let (generator, edits, geometry, _, player) = creature_and_player("chien", 10.0, 19);
        let mut world = CreatureWorld::new(generator.base_radius(), 19, registry);
        let (_, feet) = grass_column(&generator, &edits);
        world.spawn(world.registry().find("chien").unwrap(), feet);
        let mut closest = f32::MAX;
        for _ in 0..120 {
            world.tick(0.05, &generator, &edits, &[player]);
            closest = closest.min(planar_blocks(&geometry, world.creatures()[0].position, player));
        }
        let end = planar_blocks(&geometry, world.creatures()[0].position, player);
        assert!(end < 6.0, "le chien n'a pas suivi le joueur ({end} blocs)");
        assert!(closest > 2.0, "le chien colle le joueur ({closest} blocs)");
        assert!(!world.creatures()[0].walking, "le chien ne s'arrête pas près du joueur");
    }

    #[test]
    fn an_indifferent_creature_ignores_the_player() {
        let (generator, edits, _, mut world, player) = creature_and_player("sanglier", 2.0, 23);
        let c = &mut world.creatures[0];
        (c.walking, c.timer) = (false, 100.0);
        for _ in 0..20 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let c = &world.creatures()[0];
        assert!(!c.walking && c.panic == 0.0, "le sanglier réagit au joueur");
    }

    #[test]
    fn animals_sleep_outside_their_hours_and_wake_up_when_their_time_comes() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let up = feet.normalize();
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        let cow = world.registry().find("vache").unwrap();
        let wolf = world.registry().find("loup").unwrap();
        let cow_id = world.spawn(cow, feet);
        let wolf_id = world.spawn(wolf, feet);
        let get = |world: &CreatureWorld, id| world.creatures().iter().find(|c| c.id == id).map(|c| (c.is_sleeping(), c.position)).unwrap();

        // Minuit : le soleil sous les pieds. La vache s'endort et ne bouge plus, le loup veille.
        world.set_sun_dir(-up);
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let (cow_sleeping, at) = get(&world, cow_id);
        assert!(cow_sleeping, "la vache ne dort pas la nuit");
        assert!(!get(&world, wolf_id).0, "le loup dort la nuit");
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        assert!(get(&world, cow_id).1.distance(at) < 1e-3, "la vache bouge en dormant");

        // Un coup la réveille
        world.hit(cow_id, feet + up * 10.0);
        assert!(!get(&world, cow_id).0, "frappée, la vache dort encore");

        // Midi : la vache est réveillée, le loup dort
        world.set_sun_dir(up);
        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        assert!(!get(&world, cow_id).0, "la vache dort en plein jour");
        assert!(get(&world, wolf_id).0, "le loup ne dort pas en plein jour");
    }

    #[test]
    fn only_awake_species_appear() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let up = player.normalize();
        for (sun, hour) in [(up, "midi"), (-up, "minuit")] {
            let mut world = CreatureWorld::new(generator.base_radius(), 11, registry());
            world.set_sun_dir(sun);
            for _ in 0..1200 {
                world.tick(0.05, &generator, &edits, &[player]);
            }
            assert!(!world.creatures().is_empty(), "aucune créature à {hour}");
            for c in world.creatures() {
                let species = world.registry().get(c.species).unwrap();
                // Apparues loin du joueur : le soleil n'y est pas tout à fait à la même hauteur
                let height = sun.dot(c.position.normalize());
                assert!(species.activity.is_awake(height), "{} apparu(e) à {hour}", species.id);
            }
        }
    }

    /// Plus grand écart au centre d'un troupeau, en blocs
    fn herd_spread(world: &CreatureWorld, geometry: &BlockGeometry, herd: u64) -> f32 {
        let members: Vec<Vec3> = world.creatures().iter().filter(|c| c.herd == herd).map(|c| c.position).collect();
        let center = members.iter().copied().sum::<Vec3>() / members.len() as f32;
        members.iter().map(|p| p.distance(center) / geometry.block_height as f32).fold(0.0, f32::max)
    }

    #[test]
    fn a_herd_stays_together() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        let sheep = world.registry().find("mouflon").unwrap();
        let herd = world.spawn_in_herd(sheep, feet, None);
        for _ in 0..4 {
            world.spawn_in_herd(sheep, feet, Some(herd));
        }
        // Trois minutes : sans cohésion, leurs errances au hasard les auraient dispersées
        let mut worst: f32 = 0.0;
        for frame in 0..3600 {
            world.tick(0.05, &generator, &edits, &[feet]);
            if frame >= 600 {
                worst = worst.max(herd_spread(&world, &geometry, herd));
            }
        }
        assert_eq!(world.creatures().iter().filter(|c| c.herd == herd).count(), 5);
        assert!(worst < 16.0, "troupeau dispersé : une bête à {worst} blocs du centre");
    }

    #[test]
    fn a_hit_alarms_the_herd_less_than_the_victim() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let sheep = world.registry().find("mouflon").unwrap();
        let victim = world.spawn_in_herd(sheep, feet, None);
        let mates = [world.spawn_in_herd(sheep, feet, Some(victim)), world.spawn_in_herd(sheep, feet, Some(victim))];
        let stranger = world.spawn(sheep, feet);
        let (east, _) = tangent_frame(feet.normalize());
        assert!(world.hit(victim, feet - east * 2.0 * geometry.block_height as f32));

        let get = |world: &CreatureWorld, id: u64| world.creatures().iter().find(|c| c.id == id).map(|c| (c.panic, c.panic_speed)).unwrap();
        assert_eq!(get(&world, victim), (PANIC_TIME, PANIC_SPEED));
        for &mate in &mates {
            let (panic, speed) = get(&world, mate);
            assert!(panic > 0.0 && panic < PANIC_TIME && speed < PANIC_SPEED, "voisine : fuite {panic} s à {speed}");
        }
        assert_eq!(get(&world, stranger).0, 0.0, "une bête d'un autre troupeau ne s'affole pas");

        // Les voisines se calment avant la victime
        for _ in 0..((ALARM_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        assert!(mates.iter().all(|&m| get(&world, m).0 <= 0.0), "les voisines fuient encore");
        assert!(get(&world, victim).0 > 0.0, "la victime s'est calmée trop tôt");
    }

    /// Plateau de pierre en plein ciel au-dessus d'une colonne d'herbe, de `dx` blocs de part et
    /// d'autre en x et `dz` en z : ses bords sont des falaises. Renvoie le bloc central du plancher.
    fn sky_platform(generator: &PlanetWorldGenerator, edits: &TerrainEdits, dx: i32, dz: i32) -> BlockPos {
        let (ground, _) = grass_column(generator, edits);
        let floor = BlockPos { y: ground.y + 24, ..ground };
        for x in -dx..=dx {
            for z in -dz..=dz {
                assert!(edits.set_block(generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::STONE));
            }
        }
        floor
    }

    /// Pieds posés sur le plancher `floor`, décalés de (x, z) colonnes
    fn feet_on(geometry: &BlockGeometry, floor: BlockPos, x: i32, z: i32) -> Vec3 {
        geometry.corner(floor.face, (floor.x + x) as f64 + 0.5, (floor.y + 1) as f64, (floor.z + z) as f64 + 0.5).as_vec3()
    }

    /// Fait marcher la créature `0` droit vers la colonne `to`, sans changer d'humeur
    fn walk_towards(world: &mut CreatureWorld, geometry: &BlockGeometry, to: BlockPos) {
        let c = &mut world.creatures[0];
        c.yaw = yaw_towards(geometry, c.position, to);
        (c.target_yaw, c.walking, c.timer) = (c.yaw, true, 1000.0);
    }

    #[test]
    fn a_creature_walks_around_a_wall() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 6, 4);
        // Mur de cinq blocs de large et deux de haut en travers, avec un passage de chaque côté
        for z in -2..=2 {
            for y in 1..=2 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + 2, y: floor.y + y, z: floor.z + z, ..floor }, BlockId::STONE));
            }
        }
        let feet = feet_on(&geometry, floor, -3, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 13, registry());
        let boar = world.registry().find("sanglier").unwrap();
        world.spawn(boar, feet);
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x + 6, ..floor });

        let mut passed = false;
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let position = world.creatures()[0].position;
            let alt = altitude(&geometry, position);
            assert!((alt - (floor.y + 1) as f32).abs() < 0.6, "tombée du plateau ({alt})");
            let column = geometry.block_at_point(position.as_dvec3()).unwrap();
            passed |= column.x >= floor.x + 3 && (alt - (floor.y + 1) as f32).abs() < 1e-2;
        }
        assert!(passed, "le sanglier n'a pas contourné le mur");
    }

    #[test]
    fn creatures_never_walk_off_a_cliff() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 4, 4);
        let feet = feet_on(&geometry, floor, 0, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 17, registry());
        let mut ids = Vec::new();
        for (name, x, z) in [("vache", -2, 0), ("lapin", 2, 2), ("sanglier", 0, -2)] {
            let species = world.registry().find(name).unwrap();
            ids.push(world.spawn(species, feet_on(&geometry, floor, x, z)));
        }
        // Deux minutes de promenade, toujours droit vers le bord quand elles repartent
        for i in 0..2400 {
            if i % 100 == 0 {
                for c in world.creatures.iter_mut().filter(|c| ids.contains(&c.id)) {
                    (c.walking, c.timer) = (true, 4.0);
                }
            }
            world.tick(0.05, &generator, &edits, &[feet]);
            // Celles qui apparaissent d'elles-mêmes sur le sol, plus bas, ne comptent pas
            for c in world.creatures().iter().filter(|c| ids.contains(&c.id)) {
                let alt = altitude(&geometry, c.position);
                assert!((alt - (floor.y + 1) as f32).abs() < 0.6, "{} tombée du plateau ({alt})", c.species);
            }
        }
    }

    #[test]
    fn a_fleeing_creature_swims_across_water() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 7, 3);
        // Un ruisseau de trois blocs en travers du plateau
        for x in 0..=2 {
            for z in -3..=3 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::WATER));
            }
        }
        let feet = feet_on(&geometry, floor, -4, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 19, registry());
        let cow = world.registry().find("vache").unwrap();
        world.spawn(cow, feet);
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x + 7, ..floor });
        world.creatures[0].panic = 100.0;

        let (mut swam, mut across) = (false, false);
        for _ in 0..300 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let c = &world.creatures()[0];
            let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
            if c.swimming {
                swam = true;
                // Le corps est à moitié dans l'eau, les pieds restent à la surface
                assert!(c.state().position.length() < c.position.length());
            }
            across |= column.x >= floor.x + 3 && !c.swimming;
        }
        assert!(swam, "la vache n'est pas entrée dans l'eau");
        assert!(across, "la vache n'a pas traversé le ruisseau");
    }

    #[test]
    fn a_creature_in_water_swims_to_the_shore() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 7, 4);
        // Un bassin sur tout le plateau sauf sa bordure est
        for x in -7..=4 {
            for z in -4..=4 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::WATER));
            }
        }
        let feet = feet_on(&geometry, floor, -5, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 23, registry());
        let sheep = world.registry().find("vache").unwrap();
        world.spawn(sheep, feet);
        // Tournée vers l'ouest, loin de la rive
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x - 7, ..floor });
        world.creatures[0].timer = 3.0;

        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let c = &world.creatures()[0];
        let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
        assert!(column.x >= floor.x + 5 && !c.swimming, "le mouton est resté à l'eau (colonne {})", column.x - floor.x);
        assert!((altitude(&geometry, c.position) - (floor.y + 1) as f32).abs() < 1e-2);
    }
}

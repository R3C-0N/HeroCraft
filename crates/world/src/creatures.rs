//! Créatures : elles apparaissent sur le relief autour des joueurs, s'y promènent au hasard et
//! disparaissent quand plus aucun joueur n'est près d'elles.
//!
//! La même simulation tourne sur le serveur, qui fait autorité et diffuse les positions, et dans la
//! partie solo du client. Elle lit le terrain bloc par bloc (générateur + modifications) : une
//! créature marche sur les blocs posés, tombe dans les trous creusés, contourne les murs et l'eau.
//! Corps, allure et biomes de chaque espèce viennent de sa description (`CreatureRegistry`).
//!
//! Une créature se tourne peu à peu vers son nouveau cap, saute pour monter sur un bloc, et, frappée
//! par un joueur, rougit, recule, puis s'enfuit ; sans vie, elle bascule sur le flanc et disparaît.
//!
//! Chaque espèce vit à son rythme (`Activity`) : selon la hauteur du soleil là où elle se trouve, elle
//! est éveillée ou s'endort sur place, une bête après l'autre, jusqu'à ce que son heure revienne ou
//! qu'un coup la réveille. Une espèce n'apparaît qu'aux heures où elle est éveillée.

use std::collections::HashMap;
use std::sync::Arc;
use std::f32::consts::{PI, TAU};

use glam::Vec3;
use herocraft_common::{tangent_frame, BlockId, CreatureRegistry, CreatureSpecies, CreatureState, PlanetFace, CREATURE_DEATH_TIME, CREATURE_HURT_TIME, PLAYER_HIT_DAMAGE};
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
/// Intervalle entre deux réveils possibles d'une créature endormie, en secondes : les bêtes d'un
/// troupeau ne se lèvent pas toutes ensemble
const SLEEP_CHECK: (f32, f32) = (3.0, 12.0);
/// Une créature ne descend pas volontairement plus bas que ça, en blocs
const MAX_DROP_BLOCKS: f32 = 3.0;
/// Pas de simulation le plus long, en secondes
const MAX_STEP: f32 = 0.1;
/// Au-delà, le cache de colonnes est vidé
const MAX_CACHED_COLUMNS: usize = 4096;

pub struct Creature {
    pub id: u64,
    /// Rang de son espèce dans le registre
    pub species: usize,
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
    /// Temps écoulé depuis sa mort (`None` : vivante), en secondes
    dead_for: Option<f32>,
    /// Endormie : elle ne bouge plus jusqu'au retour de son heure, ou jusqu'à un coup
    sleeping: bool,
}

impl Creature {
    pub fn state(&self) -> CreatureState {
        CreatureState {
            creature_id: self.id,
            position: self.position,
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
        let id = self.next_id;
        self.next_id += 1;
        let yaw = self.rng.gen_range(0.0..TAU);
        let timer = self.rng.gen_range(0.5..3.0);
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        self.creatures.push(Creature {
            id,
            species,
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
            dead_for: None,
            sleeping: false,
        });
        id
    }

    /// Un joueur dont la tête est en `from` frappe la créature `id` : elle perd de la vie, est
    /// repoussée à l'opposé, puis s'enfuit. Renvoie faux si elle n'existe pas ou est déjà morte.
    pub fn hit(&mut self, id: u64, from: Vec3) -> bool {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) else { return false };
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
            c.walking = true;
            c.target_yaw = away.dot(east).atan2(away.dot(north)).rem_euclid(TAU);
        }
        true
    }

    /// Avance la simulation de `dt` secondes autour des joueurs dont les pieds sont `players`
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, players: &[Vec3]) {
        // Une image très longue (fenêtre en arrière-plan) ne fait pas traverser les murs
        let dt = dt.min(MAX_STEP);
        let range = CREATURE_RANGE_BLOCKS * self.geometry.block_height as f32;
        self.creatures.retain(|c| c.dead_for.is_none_or(|t| t < CREATURE_DEATH_TIME) && players.iter().any(|p| p.distance(c.position) <= range));

        let columns = std::mem::take(&mut self.columns);
        let mut query = TerrainQuery::with_columns(generator, edits, columns);
        let mut walker = Walker { geometry: self.geometry, query: &mut query, sun_dir: self.sun_dir };
        for creature in &mut self.creatures {
            if let Some(species) = self.registry.get(creature.species) {
                walker.step(creature, species, &mut self.rng, dt);
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
                self.spawn(species, feet);
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

    /// Le corps tient debout sur ce sol : il y a la place, et le sol n'est pas liquide
    fn can_stand(&mut self, ground: BlockPos, height: f32) -> bool {
        let below = self.query.block(BlockPos { y: ground.y - 1, ..ground });
        below != BlockId::WATER && below != BlockId::LAVA && self.fits(ground, ground.y as f32, height)
    }

    /// Rien de solide dans la colonne `column` entre les altitudes `altitude` et `altitude + height`
    fn fits(&mut self, column: BlockPos, altitude: f32, height: f32) -> bool {
        let (bottom, top) = ((altitude + 1e-3).floor() as i32, (altitude + height - 1e-3).floor() as i32);
        (bottom..=top).all(|y| {
            let pos = BlockPos { y, ..column };
            !pos.is_valid() || !self.query.block(pos).is_solid()
        })
    }

    fn step(&mut self, c: &mut Creature, species: &CreatureSpecies, rng: &mut StdRng, dt: f32) {
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
        } else {
            // Humeur : marcher un moment dans une direction, puis s'arrêter brouter ; hors de ses
            // heures, s'endormir là où elle est, et au retour de ses heures, se réveiller. Chacune le
            // fait à la fin de son humeur du moment : un troupeau s'endort et s'éveille peu à peu.
            c.timer -= dt;
            if c.timer <= 0.0 {
                let awake = sun_height(self.sun_dir, c.position).is_none_or(|height| species.activity.is_awake(height));
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
                } else if c.walking || rng.gen_bool(0.4) {
                    c.walking = false;
                    c.timer = rng.gen_range(2.0..6.0);
                    if rng.gen_bool(0.5) {
                        c.target_yaw = c.yaw + rng.gen_range(-1.0..1.0);
                    }
                } else {
                    c.walking = true;
                    c.timer = rng.gen_range(1.0..4.0);
                    c.target_yaw = c.yaw + rng.gen_range(-PI / 2.0..PI / 2.0);
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

        // Déplacement dans le plan : recul d'un coup, et marche (ralentie le temps de se tourner)
        let (east, north) = tangent_frame(up);
        c.push = (c.push - up * c.push.dot(up)) * (-KNOCKBACK_DAMPING * dt).exp();
        if c.push.length_squared() < 1e-4 {
            c.push = Vec3::ZERO;
        }
        let mut velocity = c.push;
        let speed = species.walk_speed * if c.panic > 0.0 { PANIC_SPEED } else { 1.0 } * turn.cos().max(0.0);
        if c.walking && c.dead_for.is_none() && speed > 0.0 {
            // La colonne devant le museau décide si la voie est libre
            let heading = north * c.yaw.cos() + east * c.yaw.sin();
            let probe = c.position + heading * (speed * dt + species.body.half_length) * block;
            let ahead = match self.ground(probe, altitude) {
                Some((column, ahead)) if ahead >= altitude - MAX_DROP_BLOCKS && self.can_stand(column, species.body.height) => Some(ahead),
                _ => None,
            };
            match ahead {
                Some(ahead) => {
                    // Une marche devant : elle saute dessus
                    if on_ground && ahead > altitude + 1e-3 {
                        c.vertical_speed = JUMP_SPEED;
                    }
                    velocity += heading * speed;
                }
                // Voie barrée, une fois tournée vers son cap : demi-tour, un peu de travers
                None if turn.abs() < FACING_TOLERANCE => {
                    c.target_yaw = (c.yaw + PI + rng.gen_range(-0.8..0.8)).rem_euclid(TAU);
                }
                None => {}
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
                ground <= altitude + 1e-3 && below != BlockId::WATER && below != BlockId::LAVA && self.fits(column, altitude, species.body.height)
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
        // Une marche d'un bloc devant, et un mur de deux blocs derrière elle pour qu'il s'y arrête
        let step = BlockPos { x: ground.x + 1, ..ground };
        assert!(edits.set_block(&generator, step, BlockId::STONE));
        for dz in -2..=2 {
            for dy in 0..3 {
                edits.set_block(&generator, BlockPos { x: ground.x + 2, y: ground.y + dy, z: ground.z + dz, ..ground }, BlockId::STONE);
            }
        }
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
}

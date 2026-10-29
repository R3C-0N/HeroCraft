//! Créatures : elles apparaissent sur le relief autour des joueurs, s'y promènent au hasard et
//! disparaissent quand plus aucun joueur n'est près d'elles.
//!
//! La même simulation tourne sur le serveur, qui fait autorité et diffuse les positions, et dans la
//! partie solo du client. Elle lit le terrain bloc par bloc (générateur + modifications) : une
//! créature marche sur les blocs posés, tombe dans les trous creusés, contourne les murs et l'eau.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use glam::Vec3;
use herocraft_common::{tangent_frame, BlockId, CreatureKind, CreatureState, PlanetFace};
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
/// Hauteur de marche franchie d'un coup, en blocs
const STEP_BLOCKS: f32 = 1.0;
/// Une créature ne descend pas volontairement plus bas que ça, en blocs
const MAX_DROP_BLOCKS: f32 = 3.0;
/// Pas de simulation le plus long, en secondes
const MAX_STEP: f32 = 0.1;
/// Au-delà, le cache de colonnes est vidé
const MAX_CACHED_COLUMNS: usize = 4096;

pub struct Creature {
    pub id: u64,
    pub kind: CreatureKind,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est)
    pub yaw: f32,
    /// Vitesse de chute, en blocs par seconde
    fall_speed: f32,
    walking: bool,
    /// Temps restant avant de changer d'humeur (s'arrêter, repartir), en secondes
    timer: f32,
}

impl Creature {
    pub fn state(&self) -> CreatureState {
        CreatureState { creature_id: self.id, position: self.position, yaw: self.yaw }
    }
}

pub struct CreatureWorld {
    geometry: BlockGeometry,
    creatures: Vec<Creature>,
    next_id: u64,
    rng: StdRng,
    spawn_timer: f32,
    /// Colonnes du générateur déjà échantillonnées : les créatures restent longtemps sur les mêmes
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl CreatureWorld {
    pub fn new(base_radius: f64, seed: u64) -> Self {
        Self {
            geometry: BlockGeometry::new(base_radius),
            creatures: Vec::new(),
            next_id: 1,
            rng: StdRng::seed_from_u64(seed),
            spawn_timer: 0.0,
            columns: HashMap::new(),
        }
    }

    pub fn creatures(&self) -> &[Creature] {
        &self.creatures
    }

    pub fn clear(&mut self) {
        self.creatures.clear();
        self.columns.clear();
    }

    /// Fait apparaître une créature aux pieds `feet`, et renvoie son identifiant
    pub fn spawn(&mut self, kind: CreatureKind, feet: Vec3) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let yaw = self.rng.gen_range(0.0..TAU);
        let timer = self.rng.gen_range(0.5..3.0);
        self.creatures.push(Creature { id, kind, position: feet, yaw, fall_speed: 0.0, walking: false, timer });
        id
    }

    /// Avance la simulation de `dt` secondes autour des joueurs dont les pieds sont `players`
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, players: &[Vec3]) {
        // Une image très longue (fenêtre en arrière-plan) ne fait pas traverser les murs
        let dt = dt.min(MAX_STEP);
        let range = CREATURE_RANGE_BLOCKS * self.geometry.block_height as f32;
        self.creatures.retain(|c| players.iter().any(|p| p.distance(c.position) <= range));

        let columns = std::mem::take(&mut self.columns);
        let mut query = TerrainQuery::with_columns(generator, edits, columns);
        let mut walker = Walker { geometry: self.geometry, query: &mut query };
        for creature in &mut self.creatures {
            walker.step(creature, &mut self.rng, dt);
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
        let Some(kind) = pick_kind(biome, &mut self.rng) else { return };

        let group = self.rng.gen_range(1..=kind.max_group());
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
                self.spawn(kind, feet);
            }
        }
    }
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

/// Espèce tirée selon le biome, ou aucune (biome inhabité, ou désert la plupart du temps)
fn pick_kind(biome: BiomeType, rng: &mut StdRng) -> Option<CreatureKind> {
    use CreatureKind::*;
    let table: &[(CreatureKind, u32)] = match biome {
        BiomeType::Plains => &[(Sheep, 5), (Rabbit, 4), (Boar, 1)],
        BiomeType::Forest => &[(Boar, 4), (Rabbit, 4), (Sheep, 2)],
        BiomeType::Taiga => &[(Boar, 1), (Rabbit, 1)],
        BiomeType::Tundra | BiomeType::SnowPeaks => &[(Rabbit, 1)],
        BiomeType::Mountains => &[(Sheep, 7), (Rabbit, 3)],
        BiomeType::Savanna => &[(Boar, 5), (Rabbit, 3), (Sheep, 2)],
        BiomeType::Jungle => &[(Boar, 7), (Rabbit, 3)],
        BiomeType::Desert if rng.gen_bool(0.3) => &[(Rabbit, 1)],
        _ => &[],
    };
    let total: u32 = table.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.gen_range(0..total);
    table.iter().find_map(|&(kind, weight)| {
        if roll < weight {
            Some(kind)
        } else {
            roll -= weight;
            None
        }
    })
}

/// Déplacement d'une créature sur le terrain
struct Walker<'q, 'a> {
    geometry: BlockGeometry,
    query: &'q mut TerrainQuery<'a>,
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

    /// Le corps tient debout sur ce sol : rien de solide sur sa hauteur, et le sol n'est pas liquide
    fn can_stand(&mut self, ground: BlockPos, height: f32) -> bool {
        let below = self.query.block(BlockPos { y: ground.y - 1, ..ground });
        if below == BlockId::WATER || below == BlockId::LAVA {
            return false;
        }
        (0..height.ceil() as i32).all(|dy| {
            let pos = BlockPos { y: ground.y + dy, ..ground };
            !pos.is_valid() || !self.query.block(pos).is_solid()
        })
    }

    fn step(&mut self, c: &mut Creature, rng: &mut StdRng, dt: f32) {
        let block = self.block();
        let base_radius = self.geometry.base_radius as f32;
        let radius_of = move |altitude: f32| base_radius + altitude * block;

        // Humeur : marcher un moment dans une direction, puis s'arrêter brouter
        c.timer -= dt;
        if c.timer <= 0.0 {
            if c.walking || rng.gen_bool(0.4) {
                c.walking = false;
                c.timer = rng.gen_range(2.0..6.0);
                if rng.gen_bool(0.5) {
                    c.yaw += rng.gen_range(-1.0..1.0);
                }
            } else {
                c.walking = true;
                c.timer = rng.gen_range(1.0..4.0);
                c.yaw += rng.gen_range(-PI / 2.0..PI / 2.0);
            }
            c.yaw = c.yaw.rem_euclid(TAU);
        }

        // Pesanteur : posée sur le sol, poussée dessus si un bloc l'a recouverte, sinon elle tombe
        let up = c.position.normalize_or_zero();
        let mut altitude = self.altitude(c.position);
        let Some((_, ground)) = self.ground(c.position, altitude) else { return };
        if ground >= altitude {
            altitude = ground;
            c.fall_speed = 0.0;
        } else {
            c.fall_speed += GRAVITY * dt;
            altitude = (altitude - c.fall_speed * dt).max(ground);
            if altitude <= ground {
                c.fall_speed = 0.0;
            }
        }
        c.position = up * radius_of(altitude);
        if !c.walking || c.fall_speed > 0.0 {
            return;
        }

        // Marche : la colonne devant le museau décide si la voie est libre
        let (east, north) = tangent_frame(up);
        let heading = north * c.yaw.cos() + east * c.yaw.sin();
        let distance = c.kind.walk_speed() * dt * block;
        let probe = c.position + heading * (distance + c.kind.half_length() * block);
        let blocked = match self.ground(probe, altitude) {
            Some((column, ahead)) => ahead < altitude - MAX_DROP_BLOCKS || !self.can_stand(column, c.kind.height()),
            None => true,
        };
        if blocked {
            // Demi-tour, un peu de travers
            c.yaw = (c.yaw + PI + rng.gen_range(-0.8..0.8)).rem_euclid(TAU);
            return;
        }
        let dir = (c.position + heading * distance).normalize_or_zero();
        // Une marche devant : elle saute dessus
        let ahead = self.ground(dir * radius_of(altitude), altitude).map_or(altitude, |(_, g)| g);
        c.position = dir * radius_of(altitude.max(ahead));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::edits::FACE_BLOCKS;

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
        let mut world = CreatureWorld::new(generator.base_radius(), 7);
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
            assert!(block <= CREATURE_RANGE_BLOCKS, "{} à {block} blocs", c.kind.name());
            // Debout sur le sol de sa colonne (ou en train d'y tomber), jamais dedans
            let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
            let alt = altitude(&geometry, c.position);
            let ground = query.support_top(column.face, column.x, column.z, (alt + 1.0).floor() as i32 - 1) as f32;
            assert!(alt >= ground - 1e-3, "{} enfoncée dans le sol ({alt} < {ground})", c.kind.name());
            assert!(!query.block(BlockPos { y: (alt + 1e-2).floor() as i32, ..column }).is_solid(), "{} dans un bloc ({alt})", c.kind.name());
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
        let mut world = CreatureWorld::new(generator.base_radius(), 1);
        world.spawn(CreatureKind::Sheep, feet);
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
        let mut world = CreatureWorld::new(generator.base_radius(), 3);
        world.spawn(CreatureKind::Boar, feet);

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
}

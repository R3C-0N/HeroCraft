//! Créatures : chaque zone de la planète a sa population, tirée de la graine du monde la première
//! fois qu'un joueur s'en approche, puis plus rien n'apparaît. Les bêtes s'y promènent au hasard,
//! s'endorment (ne sont plus simulées, mais gardées telles quelles) quand plus aucun joueur n'est
//! près, et se réveillent où elles étaient quand un joueur revient. Une bête tuée ne revient pas.
//! Voir `population` pour les zones et leur enregistrement.
//!
//! La même simulation tourne sur le serveur, qui fait autorité et diffuse les positions, et dans la
//! partie solo du client. Elle lit le terrain bloc par bloc (générateur + modifications) : une
//! créature marche sur les blocs posés, tombe dans les trous creusés, contourne les murs et l'eau.
//! Corps, allure et biomes de chaque espèce viennent de sa description (`CreatureRegistry`).
//!
//! Une créature se tourne peu à peu vers son nouveau cap, saute pour monter sur un bloc, et, frappée
//! par un joueur, rougit, recule, puis s'enfuit ; sans vie, elle bascule sur le flanc et disparaît.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::f32::consts::{PI, TAU};

use glam::Vec3;
use herocraft_common::{tangent_frame, BlockId, CreatureRegistry, CreatureSpecies, CreatureState, PlanetFace, CREATURE_DEATH_TIME, CREATURE_HURT_TIME, PLAYER_HIT_DAMAGE};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::edits::{BlockGeometry, BlockPos, TerrainEdits, TerrainQuery, WORLD_HEIGHT};
use crate::generator::{BiomeType, ColumnSample, PlanetWorldGenerator};
use crate::population::{PopulationSave, SleepingCreature, ZoneKey, ZONE_BLOCKS};

/// Distance à laquelle le serveur montre une créature à un joueur, en blocs. Les zones dont le
/// centre est à moins de cette distance (plus une demi-diagonale de zone) sont éveillées.
pub const CREATURE_RANGE_BLOCKS: f32 = 96.0;
/// Demi-diagonale d'une zone, en blocs : une zone s'éveille dès qu'une partie peut être à portée
const ZONE_HALF_DIAGONAL: f32 = ZONE_BLOCKS as f32 * std::f32::consts::FRAC_1_SQRT_2;
/// Marge avant qu'une zone éveillée ne se rendorme, en blocs : un joueur qui fait les cent pas à la
/// limite ne la réveille et ne l'endort pas sans cesse
const SLEEP_MARGIN_BLOCKS: f32 = 24.0;
/// Chance qu'une zone reçoive un groupe à sa création (l'espèce tirée a encore sa propre chance)
const ZONE_GROUP_CHANCE: f64 = 0.25;
/// Écart des bêtes d'un groupe autour de son centre, en blocs
const GROUP_SPREAD: i32 = 3;
/// Premier identifiant des bêtes posées à la main : celles des zones sont numérotées par zone en dessous
const MANUAL_ID_BASE: u64 = 1 << 48;
/// Nom du fichier de la population dans le dossier de sauvegarde d'un monde
pub const POPULATION_FILE: &str = "creatures.json";
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
}

/// Espèce candidate dans un biome : rang, poids, chance d'apparaître vraiment
type SpawnCandidate = (usize, f32, f32);

pub struct CreatureWorld {
    registry: Arc<CreatureRegistry>,
    /// Espèces qui peuplent chaque biome
    spawn_table: HashMap<BiomeType, Vec<SpawnCandidate>>,
    geometry: BlockGeometry,
    /// Graine du monde : elle décide de la population de chaque zone
    seed: u64,
    /// Bêtes des zones éveillées, simulées
    creatures: Vec<Creature>,
    /// Zones éveillées : peuplées, leurs bêtes sont dans `creatures`
    awake: HashSet<ZoneKey>,
    /// Zones peuplées endormies, avec leurs bêtes. Une zone ni éveillée ni ici n'a encore jamais été
    /// peuplée.
    sleeping: HashMap<ZoneKey, Vec<SleepingCreature>>,
    /// Direction du centre de chaque zone depuis le centre de la planète
    zone_centers: Vec<(ZoneKey, Vec3)>,
    /// Prochain identifiant d'une bête posée à la main
    next_id: u64,
    rng: StdRng,
    /// Faux : les zones restent vides (tests d'une bête seule)
    populate: bool,
    /// Fichier où la population est enregistrée, ou `None` pour ne rien enregistrer
    save_path: Option<PathBuf>,
    /// Colonnes du générateur déjà échantillonnées : les créatures restent longtemps sur les mêmes
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl CreatureWorld {
    /// Monde sans sauvegarde : chaque zone est peuplée d'après `seed`, la graine de la planète
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
        let geometry = BlockGeometry::new(base_radius);
        let half = ZONE_BLOCKS as f64 / 2.0;
        let zone_centers = ZoneKey::all()
            .map(|zone| {
                let (x, z) = ((zone.x * ZONE_BLOCKS) as f64 + half, (zone.z * ZONE_BLOCKS) as f64 + half);
                (zone, geometry.corner(zone.face, x, 0.0, z).normalize().as_vec3())
            })
            .collect();
        Self {
            registry,
            spawn_table,
            geometry,
            seed,
            creatures: Vec::new(),
            awake: HashSet::new(),
            sleeping: HashMap::new(),
            zone_centers,
            next_id: MANUAL_ID_BASE,
            rng: StdRng::seed_from_u64(seed),
            populate: true,
            save_path: None,
            columns: HashMap::new(),
        }
    }

    /// Monde enregistré dans le dossier de sauvegarde `dir` : reprend les zones déjà peuplées (bêtes
    /// tuées comprises, qui ne reviennent pas) et y enregistrera la population avec `save`
    pub fn with_save_dir(base_radius: f64, seed: u64, registry: Arc<CreatureRegistry>, dir: impl AsRef<Path>) -> Self {
        let mut world = Self::new(base_radius, seed, registry);
        let path = dir.as_ref().join(POPULATION_FILE);
        match PopulationSave::load(&path) {
            Ok(Some(save)) => {
                world.next_id = save.next_id.max(MANUAL_ID_BASE);
                world.sleeping = save.into_map();
                world.save_path = Some(path);
            }
            Ok(None) => world.save_path = Some(path),
            // Fichier illisible : on ne l'écrase pas, la population sera retirée sans être gardée
            Err(e) => log::warn!("Population de {} illisible ({e}) : elle ne sera pas enregistrée.", path.display()),
        }
        world
    }

    pub fn registry(&self) -> &Arc<CreatureRegistry> {
        &self.registry
    }

    /// Les bêtes éveillées (celles des zones proches d'un joueur)
    pub fn creatures(&self) -> &[Creature] {
        &self.creatures
    }

    /// Enregistre la population : zones déjà peuplées, bêtes endormies et éveillées. Sans effet pour
    /// un monde sans sauvegarde.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.save_path else { return Ok(()) };
        let mut zones = self.sleeping.clone();
        for zone in &self.awake {
            zones.entry(*zone).or_default();
        }
        for c in self.creatures.iter().filter(|c| c.is_alive()) {
            if let (Some(zone), Some(sleeping)) = (self.zone_of(c.position), self.sleeping_creature(c)) {
                zones.entry(zone).or_default().push(sleeping);
            }
        }
        PopulationSave { next_id: self.next_id, zones: zones.into_iter().collect() }.save(path)
    }

    /// Fait apparaître une créature de l'espèce de rang `species` aux pieds `feet`, et renvoie son
    /// identifiant. Elle s'endort avec sa zone si aucun joueur n'est près.
    pub fn spawn(&mut self, species: usize, feet: Vec3) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        let yaw = self.rng.gen_range(0.0..TAU);
        self.wake_creature(id, species, feet, yaw, health);
        id
    }

    fn wake_creature(&mut self, id: u64, species: usize, feet: Vec3, yaw: f32, health: f32) {
        let timer = self.rng.gen_range(0.5..3.0);
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
        });
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

    /// Avance la simulation de `dt` secondes autour des joueurs dont les pieds sont `players` :
    /// éveille les zones qui s'en approchent (en les peuplant la première fois), endort les autres
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, players: &[Vec3]) {
        // Une image très longue (fenêtre en arrière-plan) ne fait pas traverser les murs
        let dt = dt.min(MAX_STEP);
        self.creatures.retain(|c| c.dead_for.is_none_or(|t| t < CREATURE_DEATH_TIME));

        let columns = std::mem::take(&mut self.columns);
        let mut query = TerrainQuery::with_columns(generator, edits, columns);
        self.update_zones(&mut query, players);

        let mut walker = Walker { geometry: self.geometry, query: &mut query };
        for creature in &mut self.creatures {
            if let Some(species) = self.registry.get(creature.species) {
                walker.step(creature, species, &mut self.rng, dt);
            }
        }

        let mut columns = query.into_columns();
        if columns.len() > MAX_CACHED_COLUMNS {
            columns.clear();
        }
        self.columns = columns;
    }

    /// La zone où se trouve ce point
    fn zone_of(&self, p: Vec3) -> Option<ZoneKey> {
        self.geometry.block_at_point(p.as_dvec3()).map(|b| ZoneKey::of_column(b.face, b.x, b.z))
    }

    /// Éveille les zones proches d'un joueur, endort les autres avec leurs bêtes, et range dans sa
    /// zone endormie toute bête qui est sortie des zones éveillées
    fn update_zones(&mut self, query: &mut TerrainQuery, players: &[Vec3]) {
        let block = self.geometry.block_height as f32;
        let wake = (CREATURE_RANGE_BLOCKS + ZONE_HALF_DIAGONAL) * block;
        let sleep = wake + SLEEP_MARGIN_BLOCKS * block;
        // Distance du joueur le plus proche au centre de chaque zone, à l'altitude du joueur
        let mut to_wake = Vec::new();
        let mut keep = HashSet::new();
        for &(zone, center) in &self.zone_centers {
            let nearest = players.iter().map(|p| (center * p.length()).distance(*p)).fold(f32::INFINITY, f32::min);
            if nearest <= sleep && self.awake.contains(&zone) {
                keep.insert(zone);
            } else if nearest <= wake {
                to_wake.push(zone);
            }
        }
        // Une zone qui s'endort reste peuplée, même vide : elle ne sera plus jamais tirée
        for zone in self.awake.difference(&keep) {
            self.sleeping.entry(*zone).or_default();
        }
        self.awake = keep;
        for zone in to_wake {
            let creatures = match self.sleeping.remove(&zone) {
                Some(creatures) => creatures,
                None => self.populate_zone(zone, query),
            };
            for c in creatures {
                match self.registry.find(&c.species) {
                    Some(species) => self.wake_creature(c.id, species, c.position, c.yaw, c.health),
                    None => log::warn!("Créature {} : espèce « {} » inconnue, laissée de côté.", c.id, c.species),
                }
            }
            self.awake.insert(zone);
        }

        // Les bêtes hors des zones éveillées s'endorment là où elles sont (les mourantes disparaissent)
        let mut index = 0;
        while index < self.creatures.len() {
            let c = &self.creatures[index];
            let zone = self.zone_of(c.position);
            if zone.is_some_and(|z| self.awake.contains(&z)) {
                index += 1;
                continue;
            }
            let c = self.creatures.swap_remove(index);
            let (Some(zone), Some(sleeping), true) = (zone, self.sleeping_creature(&c), c.is_alive()) else { continue };
            if !self.sleeping.contains_key(&zone) {
                let population = self.populate_zone(zone, query);
                self.sleeping.insert(zone, population);
            }
            self.sleeping.get_mut(&zone).unwrap().push(sleeping);
        }
    }

    /// La bête telle qu'elle sera gardée endormie
    fn sleeping_creature(&self, c: &Creature) -> Option<SleepingCreature> {
        let species = self.registry.get(c.species)?.id.clone();
        Some(SleepingCreature { id: c.id, species, position: c.position, yaw: c.yaw, health: c.health })
    }

    /// Bêtes que la graine du monde destine à cette zone : au plus un groupe, d'une espèce de son
    /// biome, posé sur le relief. Le tirage ne dépend que de la graine et de la zone.
    fn populate_zone(&self, zone: ZoneKey, query: &mut TerrainQuery) -> Vec<SleepingCreature> {
        let mut population = Vec::new();
        if !self.populate {
            return population;
        }
        let mut rng = StdRng::seed_from_u64(zone.seed(self.seed));
        if !rng.gen_bool(ZONE_GROUP_CHANCE) {
            return population;
        }
        // Centre du groupe assez loin du bord pour que tout le groupe reste dans la zone
        let span = GROUP_SPREAD..ZONE_BLOCKS - GROUP_SPREAD;
        let (cx, cz) = (zone.x * ZONE_BLOCKS + rng.gen_range(span.clone()), zone.z * ZONE_BLOCKS + rng.gen_range(span));
        if surface_feet(&self.geometry, query, zone.face, cx, cz).is_none() {
            return population;
        }
        let biome = query.column_sample(zone.face, cx, cz).biome;
        let Some(candidates) = self.spawn_table.get(&biome) else { return population };
        let Some(species) = pick_species(candidates, &mut rng) else { return population };
        let Some(info) = self.registry.get(species) else { return population };

        let group = rng.gen_range(info.min_group..=info.max_group);
        for n in 0..group {
            let (dx, dz) = (rng.gen_range(-GROUP_SPREAD..=GROUP_SPREAD), rng.gen_range(-GROUP_SPREAD..=GROUP_SPREAD));
            let yaw = rng.gen_range(0.0..TAU);
            if let Some(feet) = surface_feet(&self.geometry, query, zone.face, cx + dx, cz + dz) {
                let id = (zone.index() << 16) | n as u64;
                population.push(SleepingCreature { id, species: info.id.clone(), position: feet, yaw, health: info.health });
            }
        }
        population
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
            // Humeur : marcher un moment dans une direction, puis s'arrêter brouter
            c.timer -= dt;
            if c.timer <= 0.0 {
                if c.walking || rng.gen_bool(0.4) {
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
    use std::collections::HashSet;

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
    fn the_zones_around_a_player_are_populated_and_creatures_walk_on_the_ground() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        // Une minute de jeu à 20 images par seconde
        let mut starts = HashMap::new();
        for _ in 0..1200 {
            world.tick(0.05, &generator, &edits, &[player]);
            for c in world.creatures() {
                starts.entry(c.id).or_insert(c.position);
            }
        }
        assert!(!world.creatures().is_empty(), "aucune créature n'est apparue");
        let mut query = TerrainQuery::new(&generator, &edits);
        let mut moved = 0;
        for c in world.creatures() {
            let block = c.position.distance(player) / geometry.block_height as f32;
            assert!(block <= CREATURE_RANGE_BLOCKS + ZONE_BLOCKS as f32 * 2.0 + SLEEP_MARGIN_BLOCKS, "{} à {block} blocs", c.species);
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
    fn creatures_fall_asleep_far_from_every_player() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 1, registry());
        world.populate = false;
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
        world.populate = false;
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
        world.populate = false;
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
        world.populate = false;
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
        world.populate = false;
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

    /// Identifiant, espèce et position de chaque bête éveillée, triés
    fn snapshot(world: &CreatureWorld) -> Vec<(u64, usize, Vec3)> {
        let mut v: Vec<_> = world.creatures().iter().map(|c| (c.id, c.species, c.position)).collect();
        v.sort_by_key(|c| c.0);
        v
    }

    /// Un point de la face +Z, à `blocks` blocs du centre le long de x, sur le relief
    fn feet_at(generator: &PlanetWorldGenerator, edits: &TerrainEdits, x: i32, z: i32) -> Vec3 {
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(generator, edits);
        let top = query.support_top(PlanetFace::PosZ, x, z, WORLD_HEIGHT - 1);
        geometry.corner(PlanetFace::PosZ, x as f64 + 0.5, top as f64, z as f64 + 0.5).as_vec3()
    }

    #[test]
    fn the_population_depends_only_on_the_world_seed() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut a = CreatureWorld::new(generator.base_radius(), 77, registry());
        let mut b = CreatureWorld::new(generator.base_radius(), 77, registry());
        a.tick(0.0, &generator, &edits, &[player]);
        b.tick(0.0, &generator, &edits, &[player]);
        assert!(!a.creatures().is_empty());
        assert_eq!(snapshot(&a), snapshot(&b));
        let mut c = CreatureWorld::new(generator.base_radius(), 78, registry());
        c.tick(0.0, &generator, &edits, &[player]);
        assert_ne!(snapshot(&a), snapshot(&c), "une autre graine, une autre population");
    }

    #[test]
    fn nothing_appears_after_the_first_population() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        world.tick(0.05, &generator, &edits, &[player]);
        let first: HashSet<u64> = world.creatures().iter().map(|c| c.id).collect();
        assert!(!first.is_empty());
        // Une minute plus tard, aucune nouvelle bête
        for _ in 0..1200 {
            world.tick(0.05, &generator, &edits, &[player]);
            assert!(world.creatures().iter().all(|c| first.contains(&c.id)), "une bête est apparue");
        }
        // Toutes tuées : le voisinage reste vide, même après un aller-retour au loin
        let ids: Vec<u64> = world.creatures().iter().map(|c| c.id).collect();
        for id in ids {
            while world.hit(id, player) {}
        }
        for _ in 0..((CREATURE_DEATH_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        assert!(world.creatures().is_empty());
        world.tick(0.05, &generator, &edits, &[-player]);
        world.tick(0.05, &generator, &edits, &[player]);
        assert!(world.creatures().is_empty(), "une bête est revenue");
        for _ in 0..200 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        assert!(world.creatures().is_empty());
    }

    #[test]
    fn sleeping_creatures_wake_up_where_they_were() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        for _ in 0..200 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let before = snapshot(&world);
        assert!(!before.is_empty());
        // Le joueur part à l'autre bout de la planète : tout s'endort ; il revient : tout se réveille
        world.tick(0.0, &generator, &edits, &[-player]);
        assert!(world.creatures().iter().all(|c| c.position.distance(-player) < player.length()));
        world.tick(0.0, &generator, &edits, &[player]);
        let after = snapshot(&world);
        for bête in &before {
            assert!(after.iter().any(|a| a.0 == bête.0 && a.2.distance(bête.2) < 1e-3), "bête {} perdue ou déplacée", bête.0);
        }
    }

    #[test]
    fn a_creature_that_wanders_off_sleeps_in_its_new_zone() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        world.populate = false;
        let cow = world.registry().find("vache").unwrap();
        let id = world.spawn(cow, feet);
        world.tick(0.0, &generator, &edits, &[feet]);
        // Elle s'éloigne hors des zones éveillées : elle s'endort dans la sienne, pas ailleurs
        let far = (CREATURE_RANGE_BLOCKS + ZONE_BLOCKS as f32 * 3.0 + SLEEP_MARGIN_BLOCKS) as i32;
        let x = if ground.x + far < crate::edits::FACE_BLOCKS { ground.x + far } else { ground.x - far };
        let away = feet_at(&generator, &edits, x, ground.z);
        world.creatures[0].position = away;
        world.tick(0.0, &generator, &edits, &[feet]);
        assert!(world.creatures().is_empty());
        let zone = ZoneKey::of_column(PlanetFace::PosZ, x, ground.z);
        assert!(world.sleeping[&zone].iter().any(|c| c.id == id));
        // Le joueur la rejoint : elle est là
        world.tick(0.0, &generator, &edits, &[away]);
        let c = world.creatures().iter().find(|c| c.id == id).expect("la vache a disparu");
        assert!(c.position.distance(away) < geometry.block_height as f32 * 1e-3);
    }

    #[test]
    fn the_population_is_saved_and_reloaded() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let dir = std::env::temp_dir().join(format!("herocraft_creatures_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut world = CreatureWorld::with_save_dir(generator.base_radius(), 77, registry(), &dir);
        for _ in 0..100 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        // Une bête tuée avant l'enregistrement
        let killed = world.creatures()[0].id;
        while world.hit(killed, player) {}
        for _ in 0..((CREATURE_DEATH_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let before = snapshot(&world);
        world.save().unwrap();

        let mut reloaded = CreatureWorld::with_save_dir(generator.base_radius(), 77, registry(), &dir);
        reloaded.tick(0.0, &generator, &edits, &[player]);
        let after = snapshot(&reloaded);
        assert!(after.iter().all(|c| c.0 != killed), "la bête tuée est revenue");
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!((b.0, b.1), (a.0, a.1));
            assert!(b.2.distance(a.2) < 1e-3);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

//! Serveur de jeu dédié : il écoute les connexions TCP, fait autorité sur les blocs de la planète
//! (validés, appliqués, enregistrés puis diffusés à tous) et sur les créatures (qu'il fait vivre et
//! montre à chaque joueur proche), et relaie les positions des joueurs.
//!
//! Le terrain est procédural : le serveur n'envoie que la graine et les chunks modifiés, chaque
//! client génère le reste lui-même.

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec3;
use herocraft_common::lan::{LanAnnounce, LanAnnouncer};
use herocraft_common::net::{encode_frame, Connection};
use herocraft_common::{
    fall_damage, BlockId, BlockInteractionAction, CharacterStats, ClientPacket, CreatureRegistry, CreatureState, PlayerClass, PlayerState,
    ServerPacket, SkillWheel, PROTOCOL_VERSION,
};
use herocraft_voxel_engine::ChunkStorage;
use herocraft_world::{BlockGeometry, BlockPos, CreatureWorld, PlanetDescriptor, PlanetWorldGenerator, TerrainEdits, CREATURE_RANGE_BLOCKS};
use log::{info, warn};

/// Ticks de simulation par seconde
pub const TICK_RATE: u32 = 60;
/// Les positions des joueurs sont diffusées tous les N ticks (30 fois par seconde)
const STATE_BROADCAST_TICKS: u64 = 2;
/// Les positions des créatures sont diffusées tous les N ticks (10 fois par seconde : elles vont
/// au pas, le client interpole entre deux positions)
const CREATURE_BROADCAST_TICKS: u64 = 6;
/// Les statistiques RPG sont envoyées tous les N ticks (une fois par seconde)
const STATS_TICKS: u64 = TICK_RATE as u64;
/// Les blocs modifiés sont enregistrés tous les N ticks (toutes les 5 secondes)
const AUTOSAVE_TICKS: u64 = 5 * TICK_RATE as u64;
/// La partie est annoncée sur le réseau local tous les N ticks (toutes les secondes)
const LAN_ANNOUNCE_TICKS: u64 = TICK_RATE as u64;
/// Délai pour envoyer `Connect` après l'ouverture de la connexion
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Distance maximale entre les pieds d'un joueur et le centre d'un bloc qu'il modifie, en blocs
/// (6 blocs de portée depuis la tête, plus la marge de la caméra à la 3e personne et du décalage
/// entre deux envois de position)
const MAX_REACH_BLOCKS: f64 = 12.0;
/// Longueur maximale d'un nom de joueur et d'un message de chat, en caractères
const MAX_NAME_CHARS: usize = 24;
const MAX_CHAT_CHARS: usize = 256;
/// `/summon` : créatures au plus par commande, et distance maximale au joueur, en blocs
const MAX_SUMMON: u32 = 20;
const MAX_SUMMON_BLOCKS: f64 = 32.0;
/// `/xp` : points au plus par commande
const MAX_EXPERIENCE: u64 = 1_000_000;

type ClientConnection = Connection<ServerPacket, ClientPacket>;

pub struct ServerConfig {
    /// Adresse d'écoute (ex. `0.0.0.0:7777`)
    pub bind: String,
    pub seed: u64,
    /// Dossier de la sauvegarde du monde, ou `None` pour ne rien enregistrer (tests)
    pub save_dir: Option<PathBuf>,
}

impl ServerConfig {
    pub fn new(bind: impl Into<String>, seed: u64) -> Self {
        Self { bind: bind.into(), seed, save_dir: Some(PathBuf::from(herocraft_world::save_dir_name("serveur_monde", seed))) }
    }
}

pub struct ConnectedPlayer {
    pub id: u64,
    pub name: String,
    pub stats: CharacterStats,
    /// Pieds du joueur, `None` tant qu'il n'a pas envoyé sa première position
    pub position: Option<Vec3>,
    pub yaw: f32,
    pub pitch: f32,
    /// Sprint en cours, d'après le dernier `PlayerMove`
    pub sprinting: bool,
    /// Accroupi, d'après le dernier `PlayerMove`
    pub crouching: bool,
    /// Créatures que son client affiche (celles qui lui ont été annoncées)
    known_creatures: HashSet<u64>,
    connection: ClientConnection,
}

/// Connexion ouverte qui n'a pas encore envoyé `Connect`
struct PendingClient {
    connection: ClientConnection,
    opened: Instant,
}

pub struct Server {
    listener: TcpListener,
    seed: u64,
    generator: PlanetWorldGenerator,
    geometry: BlockGeometry,
    edits: TerrainEdits,
    creatures: CreatureWorld,
    pending: Vec<PendingClient>,
    players: HashMap<u64, ConnectedPlayer>,
    /// Dernière position connue de chaque joueur passé sur le serveur (par nom), pour le replacer
    last_positions: HashMap<String, Vec3>,
    /// Statistiques (niveau, points et compétences) de chaque joueur parti, par nom, rendues s'il
    /// revient pendant la vie du serveur
    last_stats: HashMap<String, CharacterStats>,
    /// Roue des compétences, juge des points que les joueurs investissent
    skill_wheel: SkillWheel,
    /// Tirage du butin des bêtes tuées
    loot_rng: herocraft_common::LootRng,
    next_player_id: u64,
    tick_counter: u64,
    /// Annonce de la partie sur le réseau local, avec son nom
    lan: Option<(LanAnnouncer, String)>,
    /// Identifiant de cette partie dans les annonces
    instance: u64,
    /// Lignes tapées dans la console du serveur dédié (`set_console`)
    console: Option<Receiver<String>>,
}

impl Server {
    pub fn bind(config: ServerConfig) -> io::Result<Self> {
        let listener = TcpListener::bind(&config.bind)?;
        listener.set_nonblocking(true)?;

        let planet = PlanetDescriptor { seed: config.seed, ..Default::default() };
        let generator = PlanetWorldGenerator::new(planet);
        let geometry = BlockGeometry::new(generator.base_radius());
        let storage = match &config.save_dir {
            Some(dir) => Some(ChunkStorage::new(dir)?),
            None => None,
        };
        let edits = TerrainEdits::new(storage);
        let instance = random_instance();
        // La population des zones vient de la graine du monde, et reprend la sauvegarde
        let registry = Arc::new(CreatureRegistry::load_default());
        let creatures = match &config.save_dir {
            Some(dir) => CreatureWorld::with_save_dir(generator.base_radius(), config.seed, registry, dir),
            None => CreatureWorld::new(generator.base_radius(), config.seed, registry),
        };

        Ok(Self {
            listener,
            seed: config.seed,
            generator,
            geometry,
            edits,
            creatures,
            pending: Vec::new(),
            players: HashMap::new(),
            last_positions: HashMap::new(),
            last_stats: HashMap::new(),
            skill_wheel: SkillWheel::standard(),
            loot_rng: herocraft_common::LootRng::from_clock(),
            next_player_id: 1,
            tick_counter: 0,
            lan: None,
            instance,
            console: None,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Annonce la partie sous ce nom aux clients du réseau local qui cherchent des parties
    pub fn enable_lan(&mut self, name: impl Into<String>) -> io::Result<()> {
        self.lan = Some((LanAnnouncer::new()?, name.into()));
        self.announce_lan();
        Ok(())
    }

    fn announce_lan(&self) {
        let (Some((announcer, name)), Ok(addr)) = (&self.lan, self.listener.local_addr()) else { return };
        announcer.announce(&LanAnnounce::new(self.instance, name.clone(), addr.port(), self.players.len() as u32, self.seed));
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn players(&self) -> impl Iterator<Item = &ConnectedPlayer> {
        self.players.values()
    }

    /// Les créatures en vie
    pub fn creatures(&self) -> &CreatureWorld {
        &self.creatures
    }

    /// Fait apparaître une créature de l'espèce `species` (nom de son fichier) aux pieds `feet` ;
    /// elle s'endormira avec sa zone si aucun joueur n'est près. `None` si l'espèce est inconnue.
    pub fn spawn_creature(&mut self, species: &str, feet: Vec3) -> Option<u64> {
        let species = self.creatures.registry().find(species)?;
        Some(self.creatures.spawn(species, feet))
    }

    /// Le bloc à cette position, modifications des joueurs comprises
    pub fn block(&self, pos: BlockPos) -> BlockId {
        self.edits.block(&self.generator, pos)
    }

    /// Boucle de jeu à `TICK_RATE` ticks par seconde, jusqu'à ce que `running` passe à `false`
    pub fn run(&mut self, running: &AtomicBool) {
        let tick_duration = Duration::from_secs_f64(1.0 / TICK_RATE as f64);
        let mut next_tick = Instant::now();
        while running.load(Ordering::Relaxed) {
            self.tick(1.0 / TICK_RATE as f32);
            next_tick += tick_duration;
            let now = Instant::now();
            if next_tick > now {
                std::thread::sleep(next_tick - now);
            } else if now - next_tick > tick_duration * 30 {
                // Plus d'une demi-seconde de retard : on ne rattrape pas les ticks perdus
                warn!("Serveur en retard de {:.0} ms, ticks sautés.", (now - next_tick).as_secs_f32() * 1000.0);
                next_tick = now;
            }
        }
        self.shutdown("Arrêt du serveur");
    }

    /// Les lignes reçues sur `console` sont exécutées comme des commandes de la console (`console_command`)
    pub fn set_console(&mut self, console: Receiver<String>) {
        self.console = Some(console);
    }

    /// Commande tapée dans la console du serveur : renvoie ce qu'elle affiche
    pub fn console_command(&mut self, line: &str) -> Vec<String> {
        let line = line.trim();
        let (name, rest) = line.split_once(' ').map_or((line, ""), |(n, r)| (n, r.trim()));
        match name.to_lowercase().as_str() {
            "help" | "?" => vec![
                "Commands: help, list, seed, say <message>, stop".to_string(),
            ],
            "list" | "who" => {
                let mut names: Vec<&str> = self.players.values().map(|p| p.name.as_str()).collect();
                names.sort_unstable();
                vec![format!("{} player(s) online: {}", names.len(), names.join(", "))]
            }
            "seed" => vec![format!("Seed: {}", self.seed)],
            "say" if !rest.is_empty() => {
                let text: String = rest.chars().filter(|c| !c.is_control()).take(MAX_CHAT_CHARS).collect();
                self.broadcast(&ServerPacket::ChatMessage { sender: "Server".to_string(), message: text.clone() });
                vec![format!("<Server> {text}")]
            }
            "say" => vec!["Usage: say <message>".to_string()],
            "" => Vec::new(),
            other => vec![format!("Unknown command: {other} (type help)")],
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.tick_counter += 1;
        let lines: Vec<String> = self.console.as_ref().map(|c| c.try_iter().collect()).unwrap_or_default();
        for line in lines {
            for reply in self.console_command(&line) {
                info!("{reply}");
            }
        }
        self.accept_connections();
        self.process_handshakes();
        self.process_players();

        for player in self.players.values_mut() {
            player.stats.regenerate(dt, player.sprinting);
        }

        let (ids, presences): (Vec<u64>, Vec<herocraft_world::PlayerPresence>) =
            self.players.values().filter_map(|p| p.position.map(|feet| (p.id, herocraft_world::PlayerPresence { feet, crouching: p.crouching }))).unzip();
        for hit in self.creatures.tick_with_presences(dt, &self.generator, &self.edits, &presences) {
            self.creature_attack(ids[hit.player], hit.species, hit.damage);
        }

        if self.tick_counter % STATE_BROADCAST_TICKS == 0 {
            self.broadcast_player_states();
        }
        if self.tick_counter % CREATURE_BROADCAST_TICKS == 0 {
            self.send_creatures();
        }
        if self.tick_counter % LAN_ANNOUNCE_TICKS == 0 {
            self.announce_lan();
        }
        if self.tick_counter % STATS_TICKS == 0 {
            for player in self.players.values() {
                player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
            }
        }
        if self.tick_counter % AUTOSAVE_TICKS == 0 {
            self.save();
        }
        if self.tick_counter % (TICK_RATE as u64 * 60) == 0 {
            info!(
                "Tick #{} : {} joueur(s) en ligne, {} créature(s), {} chunks modifiés.",
                self.tick_counter,
                self.players.len(),
                self.creatures.creatures().len(),
                self.edits.len()
            );
        }
    }

    /// Enregistre les chunks modifiés depuis le dernier enregistrement, et la population de créatures
    pub fn save(&self) {
        match self.edits.flush() {
            Ok(0) => {}
            Ok(n) => info!("{n} chunks modifiés enregistrés."),
            Err(e) => warn!("Échec de l'enregistrement des blocs modifiés : {e}"),
        }
        if let Err(e) = self.creatures.save() {
            warn!("Échec de l'enregistrement des créatures : {e}");
        }
    }

    /// Prévient les joueurs, les déconnecte et enregistre le monde
    pub fn shutdown(&mut self, reason: &str) {
        let frame = encode_frame(&ServerPacket::Disconnect(reason.to_string()));
        for player in self.players.values() {
            player.connection.send_frame(frame.clone());
        }
        self.players.clear();
        self.pending.clear();
        self.save();
    }

    fn accept_connections(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => match ClientConnection::from_stream(stream) {
                    Ok(connection) => {
                        info!("Connexion entrante de {addr}.");
                        self.pending.push(PendingClient { connection, opened: Instant::now() });
                    }
                    Err(e) => warn!("Connexion de {addr} refusée : {e}"),
                },
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    warn!("Erreur d'écoute : {e}");
                    break;
                }
            }
        }
    }

    fn process_handshakes(&mut self) {
        for pending in std::mem::take(&mut self.pending) {
            let packet = match pending.connection.try_recv() {
                Ok(Some(packet)) => packet,
                Ok(None) if pending.opened.elapsed() < HANDSHAKE_TIMEOUT => {
                    self.pending.push(pending);
                    continue;
                }
                Ok(None) => {
                    reject(&pending.connection, "Délai de connexion dépassé");
                    continue;
                }
                Err(_) => continue,
            };
            let ClientPacket::Connect { player_name, protocol_version } = packet else {
                reject(&pending.connection, "Le premier paquet doit être Connect");
                continue;
            };
            if protocol_version != PROTOCOL_VERSION {
                reject(
                    &pending.connection,
                    &format!("Version du protocole incompatible (client {protocol_version}, serveur {PROTOCOL_VERSION})"),
                );
                continue;
            }
            let name: String = player_name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
            if name.is_empty() {
                reject(&pending.connection, "Nom de joueur vide");
                continue;
            }
            if self.players.values().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                reject(&pending.connection, &format!("Le nom « {name} » est déjà en jeu"));
                continue;
            }
            self.join(pending.connection, name);
        }
    }

    /// Accueille un joueur : identifiant, graine, chunks modifiés et joueurs déjà là, puis annonce
    /// son arrivée aux autres. Tout part dans le même tick : aucun bloc modifié entre-temps n'est
    /// manqué.
    fn join(&mut self, connection: ClientConnection, name: String) {
        let id = self.next_player_id;
        self.next_player_id += 1;

        let chunks = self.edits.snapshot();
        connection.send(&ServerPacket::ConnectAck {
            player_id: id,
            world_seed: self.seed,
            spawn_position: self.last_positions.get(&name).copied(),
            server_tickrate: TICK_RATE,
            edited_chunks: chunks.len() as u32,
        });
        for (coord, chunk) in chunks {
            connection.send(&ServerPacket::ChunkData { coord, compressed_voxels: chunk.compress_rle() });
        }
        for other in self.players.values() {
            connection.send(&ServerPacket::PlayerJoined { player_id: other.id, name: other.name.clone() });
        }

        info!("{name} a rejoint la partie (joueur #{id}, {}).", connection.peer_addr());
        self.broadcast(&ServerPacket::PlayerJoined { player_id: id, name: name.clone() });
        self.broadcast(&ServerPacket::ChatMessage { sender: String::new(), message: format!("{name} a rejoint la partie") });
        let stats = self.last_stats.remove(&name).unwrap_or_else(|| CharacterStats::new(PlayerClass::Adventurer));
        connection.send(&ServerPacket::StatsUpdate(stats.clone()));
        self.players.insert(
            id,
            ConnectedPlayer {
                id,
                name,
                stats,
                position: None,
                yaw: 0.0,
                pitch: 0.0,
                sprinting: false,
                crouching: false,
                known_creatures: HashSet::new(),
                connection,
            },
        );
    }

    fn process_players(&mut self) {
        let ids: Vec<u64> = self.players.keys().copied().collect();
        for id in ids {
            loop {
                let Some(player) = self.players.get(&id) else { break };
                match player.connection.try_recv() {
                    Ok(Some(packet)) => self.handle_packet(id, packet),
                    Ok(None) => break,
                    Err(reason) => {
                        self.leave(id, &reason);
                        break;
                    }
                }
            }
        }
    }

    fn handle_packet(&mut self, id: u64, packet: ClientPacket) {
        match packet {
            ClientPacket::Connect { .. } => {}
            ClientPacket::PlayerMove { position, yaw, pitch, sprinting, crouching } => {
                if !(position.is_finite() && yaw.is_finite() && pitch.is_finite()) {
                    return;
                }
                if let Some(player) = self.players.get_mut(&id) {
                    player.position = Some(position);
                    player.yaw = yaw;
                    player.pitch = pitch;
                    player.sprinting = sprinting;
                    player.crouching = crouching;
                }
            }
            ClientPacket::Fall { blocks } => self.fall(id, blocks),
            ClientPacket::InteractBlock { block, action } => self.interact_block(id, block.into(), action),
            ClientPacket::HitCreature { creature_id } => self.hit_creature(id, creature_id),
            ClientPacket::LearnSkill { id: skill } => self.learn_skill(id, &skill),
            ClientPacket::Legend(legend) => self.legend(id, *legend),
            ClientPacket::ChatMessage(text) => {
                let text: String = text.trim().chars().filter(|c| !c.is_control()).take(MAX_CHAT_CHARS).collect();
                if text.is_empty() {
                    return;
                }
                let Some(player) = self.players.get(&id) else { return };
                info!("<{}> {}", player.name, text);
                let sender = player.name.clone();
                self.broadcast(&ServerPacket::ChatMessage { sender, message: text });
            }
            ClientPacket::Summon { species, count, feet } => self.summon(id, &species, count, feet),
            ClientPacket::Heal => {
                let Some(player) = self.players.get_mut(&id) else { return };
                player.stats.revive();
                player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
                tell(player, "Healed");
            }
            ClientPacket::GainExperience(amount) => self.grant_experience(id, amount.min(MAX_EXPERIENCE)),
            ClientPacket::Disconnect => self.leave(id, "déconnexion"),
        }
    }

    /// Légende d'un héros : refusée si un choix sort de l'atlas, sinon sa voie devient celle du
    /// joueur, qui reçoit aussitôt ses jauges
    fn legend(&mut self, id: u64, mut legend: herocraft_common::hero::Legend) {
        let Some(player) = self.players.get_mut(&id) else { return };
        if let Err(reason) = legend.verify() {
            warn!("Légende de {} refusée : {}", player.name, reason);
            return;
        }
        info!("{} est {} ({}, {}).", player.name, legend.appearance.nom, legend.appearance.species().nom, legend.class.label());
        player.stats.class = legend.class;
        player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
    }

    /// `/xp` : le joueur gagne `amount` points d'expérience, ses nouvelles jauges lui sont envoyées
    pub fn grant_experience(&mut self, id: u64, amount: u64) {
        let Some(player) = self.players.get_mut(&id) else { return };
        let level = player.stats.level;
        player.stats.add_experience(amount);
        player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
        let gained = player.stats.level.saturating_sub(level);
        let message = match gained {
            0 => format!("+{amount} XP"),
            n => format!("+{amount} XP, level {} (+{n})", player.stats.level),
        };
        tell(player, &message);
    }

    /// `/summon` : `count` créatures de l'espèce `species` aux pieds `feet`, s'ils sont près du joueur
    fn summon(&mut self, id: u64, species: &str, count: u32, feet: Vec3) {
        let Some(player) = self.players.get(&id) else { return };
        let near = player.position.is_some_and(|p| p.distance(feet) as f64 <= MAX_SUMMON_BLOCKS * self.geometry.block_height);
        let reply = match self.creatures.registry().find(species) {
            _ if !feet.is_finite() || !near => "Too far away to summon there".to_string(),
            None => format!("Unknown creature: {species}"),
            Some(index) => {
                let count = count.clamp(1, MAX_SUMMON);
                for _ in 0..count {
                    self.creatures.spawn(index, feet);
                }
                let name = self.creatures.registry().get(index).map_or(species.to_string(), |s| s.name.clone());
                info!("{} a fait apparaître {count} × {name}.", player.name);
                format!("Summoned {count} × {name}")
            }
        };
        if let Some(player) = self.players.get(&id) {
            tell(player, &reply);
        }
    }

    /// Dommages d'une chute ; le joueur tué revient à la vie (son client le replace au point
    /// d'apparition). Ses nouvelles jauges lui sont envoyées aussitôt.
    fn fall(&mut self, id: u64, blocks: f32) {
        let damage = fall_damage(blocks);
        let Some(player) = self.players.get_mut(&id) else { return };
        if damage <= 0.0 {
            return;
        }
        let died = player.stats.take_damage(damage);
        if died {
            player.stats.revive();
        }
        player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
        if died {
            let name = player.name.clone();
            info!("{name} est mort d'une chute de {blocks:.0} blocs.");
            self.broadcast(&ServerPacket::ChatMessage { sender: String::new(), message: format!("{name} s'est écrasé au sol") });
        }
    }

    /// Coup d'une créature de l'espèce de rang `species` sur un joueur ; comme une chute, celui qu'il
    /// tue revient à la vie et ses nouvelles jauges lui sont envoyées aussitôt
    fn creature_attack(&mut self, id: u64, species: usize, damage: f32) {
        let cause = self.creatures.registry().get(species).map_or_else(|| "Une créature".to_string(), |s| s.name.clone());
        let Some(player) = self.players.get_mut(&id) else { return };
        let died = player.stats.take_damage(damage);
        if died {
            player.stats.revive();
        }
        player.connection.send(&ServerPacket::PlayerHurt { damage, cause: cause.clone() });
        player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
        if died {
            let name = player.name.clone();
            info!("{name} a été tué par : {cause}.");
            self.broadcast(&ServerPacket::ChatMessage { sender: String::new(), message: format!("{name} a été tué par : {cause}") });
        }
    }

    /// Valide et applique une action sur un bloc, puis diffuse le résultat. Une action refusée
    /// renvoie à son auteur le vrai bloc, qui annule ce que son client avait déjà affiché.
    fn interact_block(&mut self, id: u64, pos: BlockPos, action: BlockInteractionAction) {
        let Some(player) = self.players.get(&id) else { return };
        if !pos.is_valid() {
            return;
        }
        let current = self.edits.block(&self.generator, pos);
        let target = match action {
            BlockInteractionAction::Break if current.is_targetable() => Some(BlockId::AIR),
            // Seul un bloc décrit par une fiche se pose, à la place de l'air ou d'un liquide
            BlockInteractionAction::Place(block)
                if herocraft_common::blocks().is_known(BlockId(block))
                    && current.is_replaceable()
                    && (!BlockId(block).is_solid() || !self.overlaps_other_player(id, pos)) =>
            {
                Some(BlockId(block))
            }
            _ => None,
        };
        let in_reach = player.position.is_some_and(|feet| {
            let center = self.geometry.corner(pos.face, pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
            center.distance(feet.as_dvec3()) <= MAX_REACH_BLOCKS * self.geometry.block_height
        });

        match target {
            Some(block) if in_reach && self.edits.set_block(&self.generator, pos, block) => {
                self.broadcast(&ServerPacket::VoxelUpdate { block: pos.into(), block_id: block.0 });
            }
            _ => {
                player.connection.send(&ServerPacket::VoxelUpdate { block: pos.into(), block_id: current.0 });
            }
        }
    }

    /// Coup d'un joueur sur une créature à sa portée : la simulation en tire dégâts, recul et fuite,
    /// que les joueurs voient avec les prochaines positions
    fn hit_creature(&mut self, id: u64, creature_id: u64) {
        let Some(feet) = self.players.get(&id).and_then(|p| p.position) else { return };
        let reach = MAX_REACH_BLOCKS as f32 * self.geometry.block_height as f32;
        if !self.creatures.creatures().iter().any(|c| c.id == creature_id && c.position.distance(feet) <= reach) {
            return;
        }
        // Tuée : son butin tombe chez celui qui l'a frappée (les objets au sol sont propres à chacun)
        let Some((species, position)) = self.creatures.hit_to_kill(creature_id, feet) else { return };
        let loot = herocraft_common::loot::creature_loot(self.creatures.registry(), species).roll(&mut self.loot_rng);
        if let (Some(player), false) = (self.players.get(&id), loot.is_empty()) {
            let items = loot.into_iter().map(|(item, count)| (item.0, count)).collect();
            player.connection.send(&ServerPacket::CreatureLoot { position, items });
        }
    }

    /// Un bloc posé là enfermerait un autre joueur (bloc des pieds ou de la tête). L'auteur de
    /// l'action n'est pas testé : son client le fait déjà, avec sa position exacte.
    fn overlaps_other_player(&self, id: u64, pos: BlockPos) -> bool {
        self.players.values().filter(|p| p.id != id).filter_map(|p| p.position).any(|feet| {
            let Some(feet_block) = self.geometry.block_at_point(feet.as_dvec3()) else { return false };
            feet_block.face == pos.face && feet_block.x == pos.x && feet_block.z == pos.z && (pos.y == feet_block.y || pos.y == feet_block.y + 1)
        })
    }

    /// Investit un point du joueur dans une compétence, si la roue l'accepte, puis lui renvoie
    /// ses statistiques (inchangées en cas de refus, ce qui défait ce que son client a affiché)
    fn learn_skill(&mut self, id: u64, skill: &str) {
        let Some(player) = self.players.get_mut(&id) else { return };
        if let Err(error) = self.skill_wheel.learn(&mut player.stats, skill) {
            warn!("{} n'apprend pas {skill} : {}.", player.name, error.message());
        }
        player.connection.send(&ServerPacket::StatsUpdate(player.stats.clone()));
    }

    fn leave(&mut self, id: u64, reason: &str) {
        let Some(player) = self.players.remove(&id) else { return };
        info!("{} a quitté la partie ({reason}).", player.name);
        if let Some(position) = player.position {
            self.last_positions.insert(player.name.clone(), position);
        }
        self.last_stats.insert(player.name.clone(), player.stats);
        self.broadcast(&ServerPacket::PlayerLeft { player_id: id });
        self.broadcast(&ServerPacket::ChatMessage { sender: String::new(), message: format!("{} a quitté la partie", player.name) });
    }

    fn broadcast_player_states(&self) {
        if self.players.len() < 2 {
            return;
        }
        let states: Vec<PlayerState> = self
            .players
            .values()
            .filter_map(|p| p.position.map(|position| PlayerState { player_id: p.id, position, yaw: p.yaw, pitch: p.pitch }))
            .collect();
        // Chacun reçoit les autres, pas lui-même
        for player in self.players.values() {
            let others: Vec<PlayerState> = states.iter().filter(|s| s.player_id != player.id).copied().collect();
            if !others.is_empty() {
                player.connection.send(&ServerPacket::PlayerStates(others));
            }
        }
    }

    /// Montre à chaque joueur les créatures de son voisinage : celles qui y entrent sont annoncées,
    /// celles qui en sortent (ou ont disparu) retirées, les autres ont leur position à jour
    fn send_creatures(&mut self) {
        let range = CREATURE_RANGE_BLOCKS * self.geometry.block_height as f32;
        let (creatures, registry) = (self.creatures.creatures(), self.creatures.registry());
        for player in self.players.values_mut() {
            let Some(feet) = player.position else { continue };
            let mut visible = HashSet::new();
            let mut states: Vec<CreatureState> = Vec::new();
            for creature in creatures.iter().filter(|c| c.position.distance(feet) <= range) {
                visible.insert(creature.id);
                if player.known_creatures.contains(&creature.id) {
                    states.push(creature.state());
                } else {
                    let species = registry.get(creature.species).map(|s| s.id.clone()).unwrap_or_default();
                    player.connection.send(&ServerPacket::CreatureSpawned { species, state: creature.state() });
                }
            }
            let gone: Vec<u64> = player.known_creatures.difference(&visible).copied().collect();
            if !gone.is_empty() {
                player.connection.send(&ServerPacket::CreaturesDespawned(gone));
            }
            if !states.is_empty() {
                player.connection.send(&ServerPacket::CreatureStates(states));
            }
            player.known_creatures = visible;
        }
    }

    /// Envoie un paquet à tous les joueurs en ligne
    fn broadcast(&self, packet: &ServerPacket) {
        let frame = encode_frame(packet);
        for player in self.players.values() {
            player.connection.send_frame(frame.clone());
        }
    }
}

/// Réponse d'une commande, au seul joueur qui l'a tapée
fn tell(player: &ConnectedPlayer, message: &str) {
    player.connection.send(&ServerPacket::ChatMessage { sender: String::new(), message: message.to_string() });
}

fn reject(connection: &ClientConnection, reason: &str) {
    warn!("Connexion de {} refusée : {reason}", connection.peer_addr());
    connection.send(&ServerPacket::Disconnect(reason.to_string()));
}

/// Identifiant de partie tiré au hasard (clés aléatoires de `RandomState`)
fn random_instance() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    hasher.finish()
}

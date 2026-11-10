//! Partie en ligne : connexion au serveur dédié, envoi de la position et des blocs modifiés,
//! réception des blocs des autres, de leurs positions et des créatures proches.
//!
//! La partie se rejoint depuis le menu Multijoueur (parties du réseau local ou adresse saisie), ou
//! dès le lancement avec `HEROCRAFT_SERVER` / `--connect`. Le même menu ouvre la partie solo au
//! réseau local en lançant un serveur dans le jeu.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use glam::Vec3;
use herocraft_common::net::Connection;
use herocraft_server::{Server, ServerConfig};
use herocraft_common::{
    BlockId, BlockInteractionAction, CharacterStats, ClientPacket, ItemId, CreatureAction, CreatureRegistry, CreatureState, PlanetChunkCoord, CREATURE_DEATH_TIME, CREATURE_HURT_TIME, ServerPacket, PROTOCOL_VERSION,
};
use herocraft_voxel_engine::ChunkData;
use herocraft_world::{BlockGeometry, BlockPos};
use log::{info, warn};

use crate::camera::{CameraViewMode, OrbitCamera, PLAYER_HALF_WIDTH};
use crate::creatures::CreatureView;

/// Délai de connexion et d'accueil (graine et chunks modifiés)
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Intervalle minimal entre deux envois de position (30 par seconde)
const MOVE_SEND_INTERVAL: Duration = Duration::from_millis(33);
/// Les autres joueurs sont affichés avec ce retard sur leurs positions reçues : il y a presque
/// toujours deux positions encadrant l'instant affiché, entre lesquelles le mouvement est
/// interpolé (deux intervalles d'envoi du serveur)
const INTERPOLATION_DELAY: Duration = Duration::from_millis(70);
/// Retard d'affichage des créatures, dont les positions arrivent moins souvent (10 par seconde)
const CREATURE_INTERPOLATION_DELAY: Duration = Duration::from_millis(220);
/// Sans position récente, le mouvement est prolongé au plus de cette durée, puis s'arrête
const MAX_EXTRAPOLATION: Duration = Duration::from_millis(150);
/// Positions gardées par joueur
const MAX_SNAPSHOTS: usize = 32;
/// Hauteur de la silhouette d'un autre joueur, en blocs
const REMOTE_PLAYER_HEIGHT: f64 = 1.8;
/// Au-delà de cette distance (en blocs), le nom d'un autre joueur n'est plus affiché
const NAME_TAG_MAX_BLOCKS: f32 = 96.0;
/// Nombre maximal de silhouettes dessinées
pub const MAX_REMOTE_PLAYERS: usize = 32;

type ServerConnection = Connection<ClientPacket, ServerPacket>;

/// Serveur demandé au lancement (`--connect ADRESSE`, sinon `HEROCRAFT_SERVER`, `--solo` pour
/// l'ignorer), nom du joueur (`--name NOM`, sinon `HEROCRAFT_PSEUDO`, sinon le nom de session) et
/// graine imposée à la planète solo (`--seed GRAINE`, sinon `HEROCRAFT_SEED`, sinon tirée au hasard)
pub struct LaunchOptions {
    pub server: Option<String>,
    pub pseudo: String,
    pub seed: Option<u64>,
}

pub fn launch_options() -> LaunchOptions {
    let mut server = std::env::var("HEROCRAFT_SERVER").ok().filter(|s| !s.trim().is_empty());
    let mut pseudo = std::env::var("HEROCRAFT_PSEUDO").ok().filter(|s| !s.trim().is_empty());
    let mut seed = std::env::var("HEROCRAFT_SEED").ok();
    let mut solo = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--connect" => server = args.next(),
            "--name" => pseudo = args.next(),
            "--solo" => solo = true,
            "--seed" => seed = args.next(),
            _ => {}
        }
    }
    let pseudo = pseudo
        .or_else(|| std::env::var("USERNAME").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "Héros".to_string());
    let seed = seed.and_then(|s| {
        let parsed = s.trim().parse().ok();
        if parsed.is_none() {
            warn!("Graine « {s} » invalide (nombre entier attendu), graine aléatoire utilisée.");
        }
        parsed
    });
    LaunchOptions { server: server.filter(|_| !solo).map(|s| server_address(&s)), pseudo, seed }
}

/// Adresse saisie, complétée du port par défaut s'il manque
pub fn server_address(input: &str) -> String {
    let input = input.trim();
    if input.contains(':') {
        input.to_string()
    } else {
        format!("{input}:{}", herocraft_common::DEFAULT_PORT)
    }
}

/// Connexion menée en tâche de fond : le jeu continue de s'afficher pendant l'accueil
pub struct PendingJoin {
    pub addr: String,
    result: Receiver<Result<JoinedWorld, String>>,
}

impl PendingJoin {
    pub fn start(addr: String, name: String) -> Self {
        let (tx, result) = mpsc::channel();
        let thread_addr = addr.clone();
        std::thread::spawn(move || {
            let _ = tx.send(join(&thread_addr, &name));
        });
        Self { addr, result }
    }

    /// Le résultat, une fois la connexion aboutie ou échouée
    pub fn poll(&self) -> Option<Result<JoinedWorld, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("connexion interrompue".into())),
        }
    }
}

/// Serveur lancé dans le jeu pour ouvrir la partie solo au réseau local : il reprend la
/// sauvegarde du monde solo et l'enregistre au même endroit
pub struct HostedServer {
    pub port: u16,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HostedServer {
    pub fn start(seed: u64, save_dir: PathBuf, pseudo: &str) -> Result<Self, String> {
        let config = |bind: String| ServerConfig { bind, seed, save_dir: Some(save_dir.clone()) };
        // Port habituel s'il est libre, sinon un port choisi par le système
        let mut server = Server::bind(config(format!("0.0.0.0:{}", herocraft_common::DEFAULT_PORT)))
            .or_else(|_| Server::bind(config("0.0.0.0:0".into())))
            .map_err(|e| format!("impossible d'ouvrir la partie : {e}"))?;
        let port = server.local_addr().map_err(|e| e.to_string())?.port();
        if let Err(e) = server.enable_lan(format!("Monde de {pseudo}")) {
            warn!("Annonce de la partie sur le réseau local impossible : {e}");
        }
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = running.clone();
        let thread = std::thread::Builder::new()
            .name("serveur-integre".into())
            .spawn(move || server.run(&thread_running))
            .map_err(|e| e.to_string())?;
        info!("Partie ouverte au réseau local sur le port {port}.");
        Ok(Self { port, running, thread: Some(thread) })
    }
}

impl Drop for HostedServer {
    /// Arrête le serveur, qui prévient ses joueurs et enregistre le monde
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        info!("Partie fermée au réseau local.");
    }
}

/// Ce que le serveur envoie à la connexion
pub struct JoinedWorld {
    pub client: NetClient,
    pub seed: u64,
    pub spawn_position: Option<Vec3>,
    pub edited_chunks: Vec<(PlanetChunkCoord, ChunkData)>,
}

/// Se connecte et attend l'accueil du serveur (bloquant)
pub fn join(addr: &str, name: &str) -> Result<JoinedWorld, String> {
    let connection = ServerConnection::connect(addr, CONNECT_TIMEOUT).map_err(|e| format!("connexion à {addr} impossible : {e}"))?;
    connection.send(&ClientPacket::Connect { player_name: name.to_string(), protocol_version: PROTOCOL_VERSION, skills: herocraft_common::skills().fingerprint() });

    let (player_id, seed, spawn_position, chunk_count) = match connection.recv_timeout(CONNECT_TIMEOUT)? {
        ServerPacket::ConnectAck { player_id, world_seed, spawn_position, edited_chunks, .. } => {
            (player_id, world_seed, spawn_position, edited_chunks)
        }
        ServerPacket::Disconnect(reason) => return Err(format!("refusé par le serveur : {reason}")),
        other => return Err(format!("réponse inattendue du serveur : {other:?}")),
    };

    let mut client = NetClient {
        connection,
        player_id,
        players: HashMap::new(),
        creatures: HashMap::new(),
        villager_cards: HashMap::new(),
        last_move_sent: None,
        last_move: None,
    };
    let mut edited_chunks = Vec::with_capacity(chunk_count as usize);
    while edited_chunks.len() < chunk_count as usize {
        match client.connection.recv_timeout(CONNECT_TIMEOUT)? {
            ServerPacket::ChunkData { coord, compressed_voxels } => match ChunkData::decompress_rle(&compressed_voxels) {
                Ok(chunk) => edited_chunks.push((coord, chunk)),
                Err(e) => return Err(format!("chunk {coord:?} illisible : {e}")),
            },
            other => {
                // Rien d'autre n'est attendu avant la fin des chunks, mais un paquet reste un paquet
                client.handle(Instant::now(), other);
            }
        }
    }
    info!("Connecté à {addr} : joueur #{player_id}, planète #{seed}, {} chunks modifiés reçus.", edited_chunks.len());
    Ok(JoinedWorld { client, seed, spawn_position, edited_chunks })
}

/// Positions reçues d'un joueur ou d'une créature (pieds et cap), datées à leur arrivée, de la plus
/// ancienne à la plus récente
#[derive(Default)]
struct Track {
    snapshots: VecDeque<(Instant, Vec3, f32)>,
}

impl Track {
    fn push(&mut self, at: Instant, position: Vec3, yaw: f32) {
        if self.snapshots.back().is_some_and(|&(last, _, _)| at < last) {
            return;
        }
        self.snapshots.push_back((at, position, yaw));
        while self.snapshots.len() > MAX_SNAPSHOTS {
            self.snapshots.pop_front();
        }
    }

    /// Pieds et cap à afficher à l'instant `now` : interpolés entre les deux positions qui encadrent
    /// `now - delay`, ou prolongés un court instant au-delà de la dernière.
    /// Calculé sur l'horloge, pas image par image : une fenêtre qui dessine peu d'images (en
    /// arrière-plan) montre quand même le joueur là où il est.
    fn sample(&self, now: Instant, delay: Duration) -> Option<(Vec3, f32)> {
        let render = now.checked_sub(delay).unwrap_or(now);
        let &(first_t, first_p, first_yaw) = self.snapshots.front()?;
        if render <= first_t {
            return Some((first_p, first_yaw));
        }
        let mut previous = (first_t, first_p, first_yaw);
        for &(t, p, yaw) in self.snapshots.iter().skip(1) {
            if render <= t {
                let span = (t - previous.0).as_secs_f32();
                let k = if span > 0.0 { (render - previous.0).as_secs_f32() / span } else { 1.0 };
                // Le cap tourne par le plus court, sans à-coup d'une position à l'autre
                let turn = (yaw - previous.2 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                return Some((previous.1.lerp(p, k), previous.2 + turn * k));
            }
            previous = (t, p, yaw);
        }
        // Au-delà de la dernière position : on prolonge la dernière vitesse, brièvement
        let &(last_t, last_p, last_yaw) = self.snapshots.back()?;
        let n = self.snapshots.len();
        if n < 2 {
            return Some((last_p, last_yaw));
        }
        let (prev_t, prev_p, _) = self.snapshots[n - 2];
        let span = (last_t - prev_t).as_secs_f32();
        if span <= 0.0 {
            return Some((last_p, last_yaw));
        }
        let ahead = (render - last_t).min(MAX_EXTRAPOLATION).as_secs_f32();
        Some((last_p + (last_p - prev_p) * (ahead / span), last_yaw))
    }
}

/// Un autre joueur, tel qu'affiché
pub struct RemotePlayer {
    pub name: String,
    /// Pieds affichés, recalculés à chaque frame par `poll`
    pub position: Option<Vec3>,
    track: Track,
}

impl RemotePlayer {
    fn new(name: String) -> Self {
        Self { name, position: None, track: Track::default() }
    }
}

/// Une créature du voisinage, telle qu'affichée
pub struct RemoteCreature {
    /// Identifiant de son espèce (nom de son fichier)
    pub species: String,
    /// Pieds et cap affichés, recalculés à chaque frame par `poll`
    pub position: Option<(Vec3, f32)>,
    /// Coup reçu et mort affichés (`CreatureState::hurt`, `death`), recalculés avec la position
    pub hurt: f32,
    pub death: f32,
    /// Ce qu'elle fait (`CreatureState::action`), au même rythme que la position
    pub action: CreatureAction,
    /// Taille et humeur d'élevage (`CreatureState::scale`, `mood`), les dernières reçues
    pub scale: f32,
    pub mood: u8,
    /// Invocateur et allure copiée (`CreatureState::summoner`, `copy`)
    pub summoner: Option<u64>,
    pub copy: Option<u32>,
    track: Track,
    /// Derniers coups, morts et actions reçus, avec leur instant, pour les montrer au rythme des
    /// positions
    effects: VecDeque<(Instant, f32, f32, CreatureAction)>,
}

impl RemoteCreature {
    fn new(species: String) -> Self {
        Self { species, position: None, hurt: 0.0, death: 0.0, action: CreatureAction::Repos, scale: 1.0, mood: 0, summoner: None, copy: None, track: Track::default(), effects: VecDeque::new() }
    }

    fn push(&mut self, at: Instant, state: &CreatureState) {
        self.track.push(at, state.position, state.yaw);
        (self.scale, self.mood, self.summoner, self.copy) = (state.scale, state.mood, state.summoner, state.copy);
        self.effects.push_back((at, state.hurt, state.death, state.action));
        while self.effects.len() > MAX_SNAPSHOTS {
            self.effects.pop_front();
        }
    }

    /// Coup, mort et action à l'instant `now - delay` : le dernier état reçu avant, avancé du temps
    /// passé depuis (la rougeur s'efface, la bête morte continue de basculer)
    fn sample_effects(&self, now: Instant, delay: Duration) -> (f32, f32, CreatureAction) {
        let render = now.checked_sub(delay).unwrap_or(now);
        let Some(&(at, hurt, death, action)) = self.effects.iter().rev().find(|(at, ..)| *at <= render) else { return (0.0, 0.0, CreatureAction::Repos) };
        let elapsed = (render - at).as_secs_f32();
        let death = if death > 0.0 { (death + elapsed / CREATURE_DEATH_TIME).min(1.0) } else { 0.0 };
        ((hurt - elapsed / CREATURE_HURT_TIME).max(0.0), death, action)
    }
}

/// Ce qui arrive du serveur et que la boucle de jeu applique
pub enum NetEvent {
    /// Bloc changé (par un autre joueur, ou correction d'une de nos actions refusée)
    BlockChanged(BlockPos, BlockId),
    /// Statistiques RPG du joueur, qui font foi
    Stats(CharacterStats),
    /// Un habitant proche parle (quelqu'un, peut-être nous, lui a adressé la parole)
    VillagerSays { speaker: String, line: String },
    /// Le joueur vient d'être frappé par une créature (`cause` : son nom)
    Hurt { damage: f32, cause: String },
    /// Le joueur a tué une créature : son butin tombe en `position`
    Loot { position: Vec3, items: Vec<(ItemId, u16)> },
    /// Œuf pondu par une bête aux pieds `position`
    Laid { position: Vec3, item: ItemId },
    /// Message du chat (`sender` vide : annonce du serveur, ou réponse d'une commande)
    Chat { sender: String, message: String },
    /// Horloge du ciel du serveur et temps imposé, qui font foi
    Clock { time: f64, forced: Option<herocraft_common::WeatherKind> },
    /// Un joueur (peut-être nous) lance une compétence
    SkillCast { player_id: u64, skill: String, rank: u8, aim: Vec3, aimed: Option<Vec3> },
    /// Le serveur refuse notre lancement de `skill`
    SkillRefused { skill: String, reason: String },
    /// Une compétence frappe : centre de sa zone et cibles touchées
    SkillHit { player_id: u64, skill: String, rank: u8, center: Vec3, creatures: Vec<u64>, players: Vec<u64> },
    /// Des créatures tirent (tourelles, bastion, liche)
    CreatureShots(Vec<herocraft_common::CreatureShotEvent>),
    /// Connexion perdue ou fermée par le serveur
    Disconnected(String),
}

pub struct NetClient {
    connection: ServerConnection,
    player_id: u64,
    players: HashMap<u64, RemotePlayer>,
    creatures: HashMap<u64, RemoteCreature>,
    /// Fiches des habitants et des voyageurs du voisinage, par créature
    villager_cards: HashMap<u64, std::sync::Arc<herocraft_common::VillagerCard>>,
    last_move_sent: Option<Instant>,
    last_move: Option<(Vec3, f32, f32, bool, bool, Option<u16>)>,
}

impl NetClient {
    /// Envoie la position du joueur et ce qu'il tient en main (au plus 20 fois par seconde, et
    /// seulement si elle a changé)
    pub fn send_move(&mut self, camera: &OrbitCamera, holding: Option<herocraft_common::ItemId>) {
        let state = (camera.player_pos, camera.player_yaw, camera.player_pitch, camera.is_running(), camera.is_crouching(), holding.map(|i| i.0));
        if self.last_move == Some(state) || self.last_move_sent.is_some_and(|t| t.elapsed() < MOVE_SEND_INTERVAL) {
            return;
        }
        self.last_move = Some(state);
        self.last_move_sent = Some(Instant::now());
        self.connection.send(&ClientPacket::PlayerMove { position: state.0, yaw: state.1, pitch: state.2, sprinting: state.3, crouching: state.4, holding: state.5 });
    }

    /// Légende du héros : le serveur en vérifie chaque choix et en prend la voie
    pub fn send_legend(&mut self, legend: &herocraft_common::hero::Legend) {
        self.connection.send(&ClientPacket::Legend(Box::new(legend.clone())));
    }

    /// Signale au serveur une chute de `blocks` blocs, qui en tire les dommages
    pub fn send_fall(&mut self, blocks: f32) {
        self.connection.send(&ClientPacket::Fall { blocks });
    }

    /// Donne à manger à une créature (l'objet est déjà retiré de l'inventaire)
    pub fn send_feed_creature(&mut self, creature_id: u64, item: herocraft_common::ItemId) {
        self.connection.send(&ClientPacket::FeedCreature { creature_id, item: item.0 });
    }

    /// Signale au serveur un bloc changé par le joueur (déjà appliqué localement)
    pub fn send_block_change(&mut self, pos: BlockPos, block: BlockId) {
        let action = if block.is_air() { BlockInteractionAction::Break } else { BlockInteractionAction::Place(block.0) };
        self.connection.send(&ClientPacket::InteractBlock { block: pos.into(), action });
    }

    /// Parle à l'habitant `creature_id` (sans rien à échanger) : le serveur renvoie sa réplique à tous
    /// les joueurs proches de lui
    pub fn send_talk_to_villager(&mut self, creature_id: u64) {
        self.connection.send(&ClientPacket::TalkToVillager { creature_id });
    }

    /// Signale au serveur un coup porté à une créature
    pub fn send_hit_creature(&mut self, creature_id: u64) {
        self.connection.send(&ClientPacket::HitCreature { creature_id });
    }

    /// Dépèce la bête morte `creature_id` (le serveur renvoie son butin)
    pub fn send_skin_creature(&mut self, creature_id: u64) {
        self.connection.send(&ClientPacket::SkinCreature { creature_id });
    }

    /// Notre identifiant de joueur sur le serveur
    pub fn player_id(&self) -> u64 {
        self.player_id
    }

    /// Demande au serveur de lancer une compétence
    pub fn send_cast_skill(&mut self, skill: &str, aim: Vec3, aimed: Option<Vec3>) {
        self.connection.send(&ClientPacket::CastSkill { skill: skill.to_string(), aim, aimed });
    }

    /// Demande au serveur d'investir un point dans une compétence (il renvoie les statistiques)
    pub fn send_learn_skill(&mut self, id: String) {
        self.connection.send(&ClientPacket::LearnSkill { id });
    }

    /// Envoie un message dans le chat ; le serveur le renvoie à tous, nous compris
    pub fn send_chat(&mut self, message: &str) {
        self.connection.send(&ClientPacket::ChatMessage(message.to_string()));
    }

    /// Commandes que le serveur exécute : `/summon`, `/heal`, `/xp`
    pub fn send_summon(&mut self, species: &str, count: u32, feet: Vec3) {
        self.connection.send(&ClientPacket::Summon { species: species.to_string(), count, feet });
    }

    /// Le joueur mange l'aliment `item` (le serveur lui renvoie ses jauges)
    pub fn send_eat(&mut self, item: herocraft_common::ItemId) {
        self.connection.send(&ClientPacket::Eat { item: item.0 });
    }

    pub fn send_heal(&mut self) {
        self.connection.send(&ClientPacket::Heal);
    }

    /// `/time set`, `/season`, `/weather` : le ciel du serveur change pour tous
    pub fn send_sky(&mut self, time: Option<f64>, forced: Option<Option<herocraft_common::WeatherKind>>) {
        self.connection.send(&ClientPacket::SetSky { time, forced });
    }

    pub fn send_experience(&mut self, amount: u64) {
        self.connection.send(&ClientPacket::GainExperience(amount));
    }

    /// Noms des autres joueurs en ligne
    pub fn player_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.players.values().map(|p| p.name.clone()).collect();
        names.sort_unstable_by_key(|n| n.to_lowercase());
        names
    }

    /// Pieds affichés de l'autre joueur nommé `name` (sans tenir compte de la casse)
    pub fn player_position(&self, name: &str) -> Option<(String, Option<Vec3>)> {
        self.players.values().find(|p| p.name.eq_ignore_ascii_case(name)).map(|p| (p.name.clone(), p.position))
    }

    /// Prévient le serveur du départ du joueur
    pub fn disconnect(&mut self) {
        self.connection.send(&ClientPacket::Disconnect);
    }

    /// Relève les paquets reçus depuis la dernière frame et fait avancer l'affichage des autres
    pub fn poll(&mut self) -> Vec<NetEvent> {
        let mut events = Vec::new();
        loop {
            match self.connection.try_recv_timed() {
                Ok(Some((at, packet))) => events.extend(self.handle(at, packet)),
                Ok(None) => break,
                Err(reason) => {
                    events.push(NetEvent::Disconnected(reason));
                    break;
                }
            }
        }
        let now = Instant::now();
        for player in self.players.values_mut() {
            player.position = player.track.sample(now, INTERPOLATION_DELAY).map(|(p, _)| p);
        }
        for creature in self.creatures.values_mut() {
            creature.position = creature.track.sample(now, CREATURE_INTERPOLATION_DELAY);
            (creature.hurt, creature.death, creature.action) = creature.sample_effects(now, CREATURE_INTERPOLATION_DELAY);
        }
        events
    }

    fn handle(&mut self, at: Instant, packet: ServerPacket) -> Option<NetEvent> {
        match packet {
            ServerPacket::VoxelUpdate { block, block_id } => return Some(NetEvent::BlockChanged(block.into(), BlockId(block_id))),
            ServerPacket::PlayerJoined { player_id, name } if player_id != self.player_id => {
                self.players.insert(player_id, RemotePlayer::new(name));
            }
            ServerPacket::PlayerLeft { player_id } => {
                self.players.remove(&player_id);
            }
            ServerPacket::PlayerStates(states) => {
                for state in states {
                    if let Some(player) = self.players.get_mut(&state.player_id) {
                        player.track.push(at, state.position, state.yaw);
                    }
                }
            }
            ServerPacket::CreatureSpawned { species, state } => {
                let mut creature = RemoteCreature::new(species);
                creature.push(at, &state);
                self.creatures.insert(state.creature_id, creature);
            }
            ServerPacket::VillagerCard { creature_id, card } => {
                self.villager_cards.insert(creature_id, std::sync::Arc::new(*card));
            }
            ServerPacket::CreaturesDespawned(ids) => {
                for id in ids {
                    self.creatures.remove(&id);
                    self.villager_cards.remove(&id);
                }
            }
            ServerPacket::CreatureStates(states) => {
                for state in states {
                    if let Some(creature) = self.creatures.get_mut(&state.creature_id) {
                        creature.push(at, &state);
                    }
                }
            }
            ServerPacket::CreatureShots(shots) => return Some(NetEvent::CreatureShots(shots)),
            ServerPacket::ChatMessage { sender, message } => {
                if sender.is_empty() {
                    info!("[Serveur] {message}");
                } else {
                    info!("<{sender}> {message}");
                }
                return Some(NetEvent::Chat { sender, message });
            }
            ServerPacket::VillagerSays { speaker, line } => return Some(NetEvent::VillagerSays { speaker, line }),
            ServerPacket::Disconnect(reason) => return Some(NetEvent::Disconnected(reason)),
            ServerPacket::Clock { time, forced } => return Some(NetEvent::Clock { time, forced }),
            ServerPacket::ChunkData { coord, .. } => warn!("Chunk {coord:?} reçu hors de la connexion, ignoré."),
            ServerPacket::StatsUpdate(stats) => return Some(NetEvent::Stats(stats)),
            ServerPacket::PlayerHurt { damage, cause } => return Some(NetEvent::Hurt { damage, cause }),
            ServerPacket::Laid { position, item } if ItemId(item).is_known() => return Some(NetEvent::Laid { position, item: ItemId(item) }),
            ServerPacket::Laid { .. } => {}
            ServerPacket::CreatureLoot { position, items } => {
                let items = items.into_iter().map(|(item, count)| (ItemId(item), count)).filter(|(item, _)| item.is_known()).collect();
                return Some(NetEvent::Loot { position, items });
            }
            ServerPacket::SkillCast { player_id, skill, rank, aim, aimed } => return Some(NetEvent::SkillCast { player_id, skill, rank, aim, aimed }),
            ServerPacket::SkillRefused { skill, reason } => return Some(NetEvent::SkillRefused { skill, reason }),
            ServerPacket::SkillHit { player_id, skill, rank, center, creatures, players } => return Some(NetEvent::SkillHit { player_id, skill, rank, center, creatures, players }),
            ServerPacket::ConnectAck { .. } | ServerPacket::PlayerJoined { .. } => {}
        }
        None
    }

    /// Noms des autres joueurs proches et devant la caméra, avec leur position à l'écran en
    /// pixels (`width` × `height`)
    pub fn name_tags(&self, camera: &OrbitCamera, width: f32, height: f32) -> Vec<(egui::Pos2, String)> {
        if camera.view_mode() == CameraViewMode::SatelliteOrbit {
            return Vec::new();
        }
        let block = BlockGeometry::new(camera.base_radius as f64).block_height as f32;
        let view_proj = camera.build_view_projection_matrix();
        let eye = camera.eye_position();
        self.players
            .values()
            .filter_map(|player| {
                let feet = player.position?;
                let head = feet + feet.normalize_or_zero() * (REMOTE_PLAYER_HEIGHT as f32 + 0.4) * block;
                if head.distance(eye) > NAME_TAG_MAX_BLOCKS * block {
                    return None;
                }
                let clip = view_proj * head.extend(1.0);
                if clip.w <= 0.0 {
                    return None;
                }
                let ndc = clip.truncate() / clip.w;
                if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
                    return None;
                }
                let screen = egui::pos2((ndc.x * 0.5 + 0.5) * width, (0.5 - ndc.y * 0.5) * height);
                Some((screen, player.name.clone()))
            })
            .collect()
    }

    /// Boîtes (8 coins, dans l'ordre de `BlockGeometry::block_corners`) des autres joueurs visibles
    /// Pieds affichés d'un autre joueur
    pub fn remote_player_feet(&self, id: u64) -> Option<Vec3> {
        self.players.get(&id).and_then(|p| p.position)
    }

    pub fn remote_player_boxes(&self, base_radius: f32) -> Vec<[Vec3; 8]> {
        let block = BlockGeometry::new(base_radius as f64).block_height as f32;
        self.players
            .values()
            .filter_map(|p| p.position)
            .take(MAX_REMOTE_PLAYERS)
            .map(|feet| player_box(feet, block))
            .collect()
    }

    /// Créatures du voisinage à dessiner, leur espèce cherchée dans `registry`
    /// Fiche de l'habitant ou du voyageur qu'incarne la créature `id`
    pub fn villager_card(&self, id: u64) -> Option<&std::sync::Arc<herocraft_common::VillagerCard>> {
        self.villager_cards.get(&id)
    }

    pub fn creature_views(&self, registry: &CreatureRegistry) -> Vec<CreatureView> {
        self.creatures
            .iter()
            .filter_map(|(&id, c)| {
                let (feet, yaw) = c.position?;
                Some(CreatureView { id, species: registry.find(&c.species), feet, yaw, hurt: c.hurt, death: c.death, action: c.action, scale: c.scale, mood: c.mood, summoner: c.summoner, copy: c.copy, tint: None })
            })
            .collect()
    }
}

/// Coins de la silhouette d'un joueur debout aux pieds `feet` : bit 0 = est, bit 1 = haut, bit 2 = nord
fn player_box(feet: Vec3, block: f32) -> [Vec3; 8] {
    let up = feet.normalize_or_zero();
    let (east, north) = crate::camera::tangent_frame(up);
    let (e, n, h) = (east * PLAYER_HALF_WIDTH * block, north * PLAYER_HALF_WIDTH * block, up * REMOTE_PLAYER_HEIGHT as f32 * block);
    std::array::from_fn(|i| {
        let sx = if i & 1 == 0 { -1.0 } else { 1.0 };
        let sz = if i & 4 == 0 { -1.0 } else { 1.0 };
        let y = if i & 2 == 0 { Vec3::ZERO } else { h };
        feet + e * sx + y + n * sz
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ouvrir sa partie au réseau local : le serveur intégré démarre sur la sauvegarde du monde et
    /// le joueur s'y connecte en tâche de fond
    #[test]
    fn test_hosted_server_accepts_its_player() {
        let save_dir = std::env::temp_dir().join(format!("herocraft_test_heberge_{}", std::process::id()));
        let host = HostedServer::start(4242, save_dir.clone(), "Mat").unwrap();
        let pending = PendingJoin::start(format!("127.0.0.1:{}", host.port), "Mat".into());
        let deadline = Instant::now() + Duration::from_secs(10);
        let world = loop {
            if let Some(result) = pending.poll() {
                break result.unwrap();
            }
            assert!(Instant::now() < deadline, "connexion trop longue");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(world.seed, 4242);
        assert!(world.edited_chunks.is_empty());
        drop(world);
        drop(host);
        let _ = std::fs::remove_dir_all(save_dir);
    }

    #[test]
    fn test_remote_player_is_interpolated_on_the_clock() {
        let mut player = Track::default();
        let t0 = Instant::now();
        let step = Duration::from_millis(33);
        for i in 0..4 {
            player.push(t0 + step * i, Vec3::new(i as f32, 0.0, 0.0), 0.0);
        }
        let position_at = |t: Instant| player.sample(t, INTERPOLATION_DELAY).map(|(p, _)| p);
        // Entre deux positions reçues : interpolé
        let p = position_at(t0 + step + step / 2 + INTERPOLATION_DELAY).unwrap();
        assert!((p.x - 1.5).abs() < 1e-3, "{p}");
        // Juste après la dernière : prolongé à la même vitesse
        let p = position_at(t0 + step * 3 + step + INTERPOLATION_DELAY).unwrap();
        assert!((p.x - 4.0).abs() < 1e-3, "{p}");
        // Longtemps après : arrêté au bout de MAX_EXTRAPOLATION
        let p = position_at(t0 + step * 3 + Duration::from_secs(5)).unwrap();
        let max = 3.0 + MAX_EXTRAPOLATION.as_secs_f32() / step.as_secs_f32();
        assert!((p.x - max).abs() < 1e-2, "{p}");
    }

    #[test]
    fn test_player_box_stands_on_the_feet() {
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let corners = player_box(feet, 1.0);
        let bottom = (corners[0] + corners[1] + corners[4] + corners[5]) / 4.0;
        let top = (corners[2] + corners[3] + corners[6] + corners[7]) / 4.0;
        assert!(bottom.distance(feet) < 1e-3);
        assert!((top.length() - feet.length() - REMOTE_PLAYER_HEIGHT as f32).abs() < 1e-3);
        assert!((corners[0].distance(corners[1]) - 2.0 * PLAYER_HALF_WIDTH).abs() < 1e-3);
    }
}

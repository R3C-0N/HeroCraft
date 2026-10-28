//! Partie en ligne : connexion au serveur dédié, envoi de la position et des blocs modifiés,
//! réception des blocs des autres et de leurs positions.
//!
//! La partie se rejoint depuis le menu Multijoueur (parties du réseau local ou adresse saisie), ou
//! dès le lancement avec `HEROCRAFT_SERVER` / `--connect`. Le même menu ouvre la partie solo au
//! réseau local en lançant un serveur dans le jeu.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use glam::Vec3;
use herocraft_common::net::Connection;
use herocraft_server::{Server, ServerConfig};
use herocraft_common::{BlockId, BlockInteractionAction, ClientPacket, PlanetChunkCoord, ServerPacket, PROTOCOL_VERSION};
use herocraft_voxel_engine::ChunkData;
use herocraft_world::{BlockGeometry, BlockPos};
use log::{info, warn};

use crate::camera::{CameraViewMode, OrbitCamera, PLAYER_HALF_WIDTH};

/// Délai de connexion et d'accueil (graine et chunks modifiés)
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Intervalle minimal entre deux envois de position (20 par seconde)
const MOVE_SEND_INTERVAL: Duration = Duration::from_millis(50);
/// Vitesse de rattrapage de la position affichée d'un autre joueur vers la dernière reçue (1/s)
const REMOTE_SMOOTHING: f32 = 15.0;
/// Hauteur de la silhouette d'un autre joueur, en blocs
const REMOTE_PLAYER_HEIGHT: f64 = 1.8;
/// Au-delà de cette distance (en blocs), le nom d'un autre joueur n'est plus affiché
const NAME_TAG_MAX_BLOCKS: f32 = 96.0;
/// Nombre maximal de silhouettes dessinées
pub const MAX_REMOTE_PLAYERS: usize = 32;

type ServerConnection = Connection<ClientPacket, ServerPacket>;

/// Serveur demandé au lancement (`--connect ADRESSE`, sinon `HEROCRAFT_SERVER`, `--solo` pour
/// l'ignorer) et nom du joueur (`--name NOM`, sinon `HEROCRAFT_PSEUDO`, sinon le nom de session)
pub struct LaunchOptions {
    pub server: Option<String>,
    pub pseudo: String,
}

pub fn launch_options() -> LaunchOptions {
    let mut server = std::env::var("HEROCRAFT_SERVER").ok().filter(|s| !s.trim().is_empty());
    let mut pseudo = std::env::var("HEROCRAFT_PSEUDO").ok().filter(|s| !s.trim().is_empty());
    let mut solo = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--connect" => server = args.next(),
            "--name" => pseudo = args.next(),
            "--solo" => solo = true,
            _ => {}
        }
    }
    let pseudo = pseudo
        .or_else(|| std::env::var("USERNAME").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "Héros".to_string());
    LaunchOptions { server: server.filter(|_| !solo).map(|s| server_address(&s)), pseudo }
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
    connection.send(&ClientPacket::Connect { player_name: name.to_string(), protocol_version: PROTOCOL_VERSION });

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
                client.handle(other);
            }
        }
    }
    info!("Connecté à {addr} : joueur #{player_id}, planète #{seed}, {} chunks modifiés reçus.", edited_chunks.len());
    Ok(JoinedWorld { client, seed, spawn_position, edited_chunks })
}

/// Un autre joueur, tel qu'affiché
pub struct RemotePlayer {
    pub name: String,
    /// Pieds affichés, qui rattrapent en douceur la dernière position reçue
    pub position: Option<Vec3>,
    target: Option<Vec3>,
}

/// Ce qui arrive du serveur et que la boucle de jeu applique
pub enum NetEvent {
    /// Bloc changé (par un autre joueur, ou correction d'une de nos actions refusée)
    BlockChanged(BlockPos, BlockId),
    /// Connexion perdue ou fermée par le serveur
    Disconnected(String),
}

pub struct NetClient {
    connection: ServerConnection,
    player_id: u64,
    players: HashMap<u64, RemotePlayer>,
    last_move_sent: Option<Instant>,
    last_move: Option<(Vec3, f32, f32)>,
}

impl NetClient {
    /// Envoie la position du joueur (au plus 20 fois par seconde, et seulement si elle a changé)
    pub fn send_move(&mut self, camera: &OrbitCamera) {
        let state = (camera.player_pos, camera.player_yaw, camera.player_pitch);
        if self.last_move == Some(state) || self.last_move_sent.is_some_and(|t| t.elapsed() < MOVE_SEND_INTERVAL) {
            return;
        }
        self.last_move = Some(state);
        self.last_move_sent = Some(Instant::now());
        self.connection.send(&ClientPacket::PlayerMove { position: state.0, yaw: state.1, pitch: state.2 });
    }

    /// Signale au serveur un bloc changé par le joueur (déjà appliqué localement)
    pub fn send_block_change(&mut self, pos: BlockPos, block: BlockId) {
        let action = if block.is_air() { BlockInteractionAction::Break } else { BlockInteractionAction::Place(block.0) };
        self.connection.send(&ClientPacket::InteractBlock { block: pos.into(), action });
    }

    /// Prévient le serveur du départ du joueur
    pub fn disconnect(&mut self) {
        self.connection.send(&ClientPacket::Disconnect);
    }

    /// Relève les paquets reçus depuis la dernière frame et fait avancer l'affichage des autres
    pub fn poll(&mut self, dt: f32) -> Vec<NetEvent> {
        let mut events = Vec::new();
        loop {
            match self.connection.try_recv() {
                Ok(Some(packet)) => events.extend(self.handle(packet)),
                Ok(None) => break,
                Err(reason) => {
                    events.push(NetEvent::Disconnected(reason));
                    break;
                }
            }
        }
        let t = 1.0 - (-REMOTE_SMOOTHING * dt).exp();
        for player in self.players.values_mut() {
            player.position = match (player.position, player.target) {
                // Saut de plus de 50 unités (téléportation, nouvelle apparition) : pas de glissade
                (Some(shown), Some(target)) if shown.distance(target) < 50.0 => Some(shown.lerp(target, t)),
                (_, target) => target,
            };
        }
        events
    }

    fn handle(&mut self, packet: ServerPacket) -> Option<NetEvent> {
        match packet {
            ServerPacket::VoxelUpdate { block, block_id } => return Some(NetEvent::BlockChanged(block.into(), BlockId(block_id))),
            ServerPacket::PlayerJoined { player_id, name } if player_id != self.player_id => {
                self.players.insert(player_id, RemotePlayer { name, position: None, target: None });
            }
            ServerPacket::PlayerLeft { player_id } => {
                self.players.remove(&player_id);
            }
            ServerPacket::PlayerStates(states) => {
                for state in states {
                    if let Some(player) = self.players.get_mut(&state.player_id) {
                        player.target = Some(state.position);
                    }
                }
            }
            ServerPacket::ChatMessage { sender, message } => {
                if sender.is_empty() {
                    info!("[Serveur] {message}");
                } else {
                    info!("<{sender}> {message}");
                }
            }
            ServerPacket::Disconnect(reason) => return Some(NetEvent::Disconnected(reason)),
            ServerPacket::ChunkData { coord, .. } => warn!("Chunk {coord:?} reçu hors de la connexion, ignoré."),
            ServerPacket::StatsUpdate(_) | ServerPacket::ConnectAck { .. } | ServerPacket::PlayerJoined { .. } => {}
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
    pub fn remote_player_boxes(&self, base_radius: f32) -> Vec<[Vec3; 8]> {
        let block = BlockGeometry::new(base_radius as f64).block_height as f32;
        self.players
            .values()
            .filter_map(|p| p.position)
            .take(MAX_REMOTE_PLAYERS)
            .map(|feet| player_box(feet, block))
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

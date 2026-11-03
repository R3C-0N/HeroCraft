use crate::character::CharacterStats;
use crate::creature::CreatureState;
use crate::math::{PlanetChunkCoord, PlanetFace};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Version du protocole : un client et un serveur de versions différentes refusent de se parler
pub const PROTOCOL_VERSION: u32 = 9;
/// Port d'écoute du serveur par défaut
pub const DEFAULT_PORT: u16 = 7777;

/// Position d'un bloc sur la planète : face du cube, colonne (x le long de u, z le long de v) et
/// altitude y en blocs depuis le fond du monde (même repère que `herocraft_world::BlockPos`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockAddress {
    pub face: PlanetFace,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Messages envoyés du Client vers le Serveur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientPacket {
    /// Demande de connexion initiale avec nom du joueur et version
    Connect {
        player_name: String,
        protocol_version: u32,
    },
    /// Position des pieds du joueur (repère du centre de la planète), orientation du regard et
    /// sprint en cours (il consomme l'endurance), posture accroupie (les bêtes craintives le
    /// remarquent de moins loin)
    PlayerMove {
        position: Vec3,
        yaw: f32,
        pitch: f32,
        sprinting: bool,
        crouching: bool,
    },
    /// Le joueur vient de retomber au sol après une chute de `blocks` blocs
    Fall {
        blocks: f32,
    },
    /// Action de minage ou de pose d'un bloc
    InteractBlock {
        block: BlockAddress,
        action: BlockInteractionAction,
    },
    /// Le joueur frappe une créature (dégâts, recul et fuite décidés par le serveur)
    HitCreature {
        creature_id: u64,
    },
    /// Le joueur investit un point dans une compétence de la roue (le serveur vérifie points et
    /// prérequis, puis renvoie ses statistiques)
    LearnSkill {
        id: String,
    },
    /// Légende du héros, envoyée à la connexion et au baptême : le serveur vérifie chaque choix
    /// contre l'atlas et en prend la voie (ses jauges et ses gains de niveau en dépendent)
    Legend(Box<crate::hero::Legend>),
    /// Le joueur mange un aliment (numéro d'objet) : le serveur en lit la satiété et renvoie ses
    /// statistiques
    Eat {
        item: u16,
    },
    /// Envoi d'un message dans le chat
    ChatMessage(String),
    /// Commande `/summon` : `count` créatures de l'espèce `species` (nom de son fichier) aux pieds
    /// `feet`, près du joueur
    Summon {
        species: String,
        count: u32,
        feet: Vec3,
    },
    /// Commande `/heal` : jauges pleines
    Heal,
    /// Commande `/xp` : points d'expérience gagnés
    GainExperience(u64),
    /// Le joueur quitte la partie
    Disconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockInteractionAction {
    Break,
    Place(u16), // BlockId
}

/// État d'un joueur tel que les autres le voient
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub player_id: u64,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Messages envoyés du Serveur vers le Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerPacket {
    /// Acceptation de la connexion. Suivie des `edited_chunks` chunks modifiés du monde
    /// (`ChunkData`), puis des joueurs déjà en ligne (`PlayerJoined`).
    ConnectAck {
        player_id: u64,
        /// Graine de la planète : le client génère le même terrain que le serveur
        world_seed: u64,
        /// Dernière position connue du joueur sur ce serveur, ou `None` pour un premier passage
        spawn_position: Option<Vec3>,
        server_tickrate: u32,
        edited_chunks: u32,
    },
    /// Chunk modifié par les joueurs, compressé en RLE (`ChunkData::compress_rle`)
    ChunkData {
        coord: PlanetChunkCoord,
        compressed_voxels: Vec<u8>,
    },
    /// Mise à jour d'un voxel individuel
    VoxelUpdate {
        block: BlockAddress,
        block_id: u16,
    },
    /// Un joueur est en ligne (à la connexion : un message par joueur déjà présent)
    PlayerJoined {
        player_id: u64,
        name: String,
    },
    /// Un joueur a quitté la partie
    PlayerLeft {
        player_id: u64,
    },
    /// Positions des autres joueurs, envoyées à intervalle régulier
    PlayerStates(Vec<PlayerState>),
    /// Une créature entre dans le voisinage du joueur (elle vient d'apparaître, ou il s'en approche)
    CreatureSpawned {
        /// Identifiant de l'espèce (nom de son fichier dans `assets/creatures/`)
        species: String,
        state: CreatureState,
    },
    /// Créatures sorties du voisinage du joueur, ou disparues
    CreaturesDespawned(Vec<u64>),
    /// Positions des créatures du voisinage, envoyées à intervalle régulier
    CreatureStates(Vec<CreatureState>),
    /// Mise à jour des statistiques RPG du joueur (chaque seconde, et aussitôt après une chute)
    StatsUpdate(CharacterStats),
    /// Le joueur vient d'être mordu ou chargé par une créature (suivi de ses nouvelles jauges)
    PlayerHurt {
        damage: f32,
        /// Nom de l'espèce qui l'a frappé, pour l'écran de mort
        cause: String,
    },
    /// Message de chat ou notification système
    ChatMessage {
        sender: String,
        message: String,
    },
    /// Erreur ou déconnexion
    Disconnect(String),
}

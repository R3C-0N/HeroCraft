use crate::character::CharacterStats;
use crate::math::PlanetChunkCoord;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Messages envoyés du Client vers le Serveur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientPacket {
    /// Demande de connexion initiale avec nom du joueur et version
    Connect {
        player_name: String,
        protocol_version: u32,
    },
    /// Déplacement et orientation du joueur
    PlayerMove {
        position: Vec3,
        yaw: f32,
        pitch: f32,
    },
    /// Action de minage ou de pose d'un bloc
    InteractBlock {
        chunk_coord: PlanetChunkCoord,
        voxel_x: u8,
        voxel_y: u8,
        voxel_z: u8,
        action: BlockInteractionAction,
    },
    /// Envoi d'un message dans le chat
    ChatMessage(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockInteractionAction {
    Break,
    Place(u16), // BlockId
}

/// Messages envoyés du Serveur vers le Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerPacket {
    /// Acceptation de la connexion avec l'identifiant du joueur attribué
    ConnectAck {
        player_id: u64,
        spawn_position: Vec3,
        server_tickrate: u32,
    },
    /// Synchronisation d'un chunk de voxel compressé
    ChunkData {
        coord: PlanetChunkCoord,
        compressed_voxels: Vec<u8>,
    },
    /// Mise à jour d'un voxel individuel
    VoxelUpdate {
        coord: PlanetChunkCoord,
        x: u8,
        y: u8,
        z: u8,
        block_id: u16,
    },
    /// Mise à jour des statistiques RPG du joueur
    StatsUpdate(CharacterStats),
    /// Message de chat ou notification système
    ChatMessage {
        sender: String,
        message: String,
    },
    /// Erreur ou déconnexion
    Disconnect(String),
}

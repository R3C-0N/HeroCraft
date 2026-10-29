//! Créatures de la planète : les espèces et l'état d'une créature tel que le serveur le diffuse.
//!
//! La simulation (apparition, déplacement) est dans `herocraft_world::creatures`.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Espèce d'une créature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureKind {
    Rabbit,
    Sheep,
    Boar,
}

impl CreatureKind {
    pub const ALL: [CreatureKind; 3] = [CreatureKind::Rabbit, CreatureKind::Sheep, CreatureKind::Boar];

    pub fn name(&self) -> &'static str {
        match self {
            CreatureKind::Rabbit => "Lapin",
            CreatureKind::Sheep => "Mouton",
            CreatureKind::Boar => "Sanglier",
        }
    }

    /// Demi-largeur du corps (de côté), en blocs
    pub fn half_width(&self) -> f32 {
        match self {
            CreatureKind::Rabbit => 0.2,
            CreatureKind::Sheep => 0.35,
            CreatureKind::Boar => 0.35,
        }
    }

    /// Demi-longueur du corps (de la tête à la queue), en blocs
    pub fn half_length(&self) -> f32 {
        match self {
            CreatureKind::Rabbit => 0.25,
            CreatureKind::Sheep => 0.55,
            CreatureKind::Boar => 0.6,
        }
    }

    /// Hauteur du corps, en blocs
    pub fn height(&self) -> f32 {
        match self {
            CreatureKind::Rabbit => 0.45,
            CreatureKind::Sheep => 1.2,
            CreatureKind::Boar => 0.9,
        }
    }

    /// Vitesse de marche, en blocs par seconde
    pub fn walk_speed(&self) -> f32 {
        match self {
            CreatureKind::Rabbit => 2.2,
            CreatureKind::Sheep => 1.0,
            CreatureKind::Boar => 1.4,
        }
    }

    /// Taille maximale d'un groupe qui apparaît ensemble
    pub fn max_group(&self) -> u32 {
        match self {
            CreatureKind::Rabbit => 2,
            CreatureKind::Sheep => 4,
            CreatureKind::Boar => 3,
        }
    }

    /// Couleur de la silhouette dans la scène HDR (au-delà de 1 : elle brille un peu)
    pub fn color(&self) -> [f32; 3] {
        match self {
            CreatureKind::Rabbit => [1.9, 1.6, 1.3],
            CreatureKind::Sheep => [2.2, 2.2, 2.2],
            CreatureKind::Boar => [1.6, 0.8, 0.45],
        }
    }
}

/// Position d'une créature telle que les joueurs la voient
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureState {
    pub creature_id: u64,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est, comme `tangent_frame`)
    pub yaw: f32,
}

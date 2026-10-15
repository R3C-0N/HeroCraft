use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Les 6 faces du cube projeté sur la sphère planétaire
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlanetFace {
    PosX, // 0: Est (+X)
    NegX, // 1: Ouest (-X)
    PosY, // 2: Pôle Nord (+Y)
    NegY, // 3: Pôle Sud (-Y)
    PosZ, // 4: Front (+Z)
    NegZ, // 5: Back (-Z)
}

impl PlanetFace {
    pub const ALL: [PlanetFace; 6] = [
        PlanetFace::PosX,
        PlanetFace::NegX,
        PlanetFace::PosY,
        PlanetFace::NegY,
        PlanetFace::PosZ,
        PlanetFace::NegZ,
    ];

    /// Projette des coordonnées 2D (u, v) normalisées dans [-1.0, 1.0] sur la face
    /// vers une direction unitaire 3D sur la sphère.
    /// Utilise la projection tangentielle normalisée pour minimiser la distorsion aux angles.
    pub fn uv_to_sphere_direction(&self, u: f64, v: f64) -> DVec3 {
        // Projection équiangulaire tangentielle (Ronchi et al.) :
        // Remplace l'échantillonnage linéaire par la tangente de l'angle pour que chaque
        // subdivision ait une dimension angulaire uniforme sur la sphère.
        // Élimine l'effet de distorsion, de gonflement au centre de l'écran et d'oscillation à la rotation.
        let u_tan = (u * std::f64::consts::FRAC_PI_4).tan();
        let v_tan = (v * std::f64::consts::FRAC_PI_4).tan();

        let (x, y, z) = match self {
            PlanetFace::PosX => (1.0, -v_tan, -u_tan),
            PlanetFace::NegX => (-1.0, -v_tan, u_tan),
            PlanetFace::PosY => (u_tan, 1.0, v_tan),
            PlanetFace::NegY => (u_tan, -1.0, -v_tan),
            PlanetFace::PosZ => (u_tan, -v_tan, 1.0),
            PlanetFace::NegZ => (-u_tan, -v_tan, -1.0),
        };
        DVec3::new(x, y, z).normalize()
    }
}

/// Coordonnée d'un chunk sur la planète
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanetChunkCoord {
    pub face: PlanetFace,
    pub x: i32,
    pub y: i32,
    pub altitude_layer: i32,
}

impl PlanetChunkCoord {
    pub fn new(face: PlanetFace, x: i32, y: i32, altitude_layer: i32) -> Self {
        Self {
            face,
            x,
            y,
            altitude_layer,
        }
    }
}

/// Coordonnée locale d'un voxel à l'intérieur d'un chunk (généralement 0..32)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocalVoxelCoord {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl LocalVoxelCoord {
    pub const CHUNK_SIZE: usize = 32;

    pub fn new(x: u8, y: u8, z: u8) -> Self {
        debug_assert!((x as usize) < Self::CHUNK_SIZE);
        debug_assert!((y as usize) < Self::CHUNK_SIZE);
        debug_assert!((z as usize) < Self::CHUNK_SIZE);
        Self { x, y, z }
    }

    #[inline(always)]
    pub fn to_linear_index(&self) -> usize {
        (self.x as usize)
            + (self.z as usize) * Self::CHUNK_SIZE
            + (self.y as usize) * Self::CHUNK_SIZE * Self::CHUNK_SIZE
    }

    #[inline(always)]
    pub fn from_linear_index(idx: usize) -> Self {
        let y = idx / (Self::CHUNK_SIZE * Self::CHUNK_SIZE);
        let rem = idx % (Self::CHUNK_SIZE * Self::CHUNK_SIZE);
        let z = rem / Self::CHUNK_SIZE;
        let x = rem % Self::CHUNK_SIZE;
        Self {
            x: x as u8,
            y: y as u8,
            z: z as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_projection_normalization() {
        for face in PlanetFace::ALL.iter() {
            let dir = face.uv_to_sphere_direction(0.5, -0.3);
            let length = dir.length();
            assert!((length - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_linear_indexing() {
        let coord = LocalVoxelCoord::new(14, 28, 3);
        let idx = coord.to_linear_index();
        let recovered = LocalVoxelCoord::from_linear_index(idx);
        assert_eq!(coord, recovered);
    }
}

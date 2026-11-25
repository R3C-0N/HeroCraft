use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

/// Axes est et nord du plan tangent à la sphère sous la direction `up`. Un cap `yaw` (en radians,
/// 0 = nord, π/2 = est) y donne la direction `nord · cos(yaw) + est · sin(yaw)`.
pub fn tangent_frame(up: Vec3) -> (Vec3, Vec3) {
    if up.y.abs() < 0.999 {
        let north = (Vec3::Y - up * up.y).normalize();
        (north.cross(up).normalize(), north)
    } else {
        let sign = if up.y >= 0.0 { 1.0 } else { -1.0 };
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -sign))
    }
}

/// Direction du cap `yaw` au point `at` de la sphère, le long du sol
pub fn yaw_direction(at: Vec3, yaw: f32) -> Vec3 {
    let (east, north) = tangent_frame(at.normalize_or(Vec3::Y));
    north * yaw.cos() + east * yaw.sin()
}

/// Ce qui est en `from` se trouve devant celui qui est en `at` et regarde vers `facing`, dans un
/// angle de `arc` degrés (360 : de tous les côtés). Seul compte l'écart le long du sol.
pub fn in_front(facing: Vec3, at: Vec3, from: Vec3, arc: f32) -> bool {
    if arc >= 360.0 {
        return true;
    }
    let up = at.normalize_or(Vec3::Y);
    let flat = |v: Vec3| (v - up * v.dot(up)).normalize_or_zero();
    let (toward, facing) = (flat(from - at), flat(facing));
    if toward == Vec3::ZERO || facing == Vec3::ZERO {
        return true;
    }
    toward.dot(facing) >= (arc.to_radians() * 0.5).cos() - 1e-4
}

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
    pub const ALL: [PlanetFace; 6] =
        [PlanetFace::PosX, PlanetFace::NegX, PlanetFace::PosY, PlanetFace::NegY, PlanetFace::PosZ, PlanetFace::NegZ];

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

    /// Détermine la face et les coordonnées (u, v) d'une direction 3D sur la sphère (inverse de uv_to_sphere_direction)
    pub fn sphere_direction_to_face_uv(dir: DVec3) -> (PlanetFace, f64, f64) {
        let abs_x = dir.x.abs();
        let abs_y = dir.y.abs();
        let abs_z = dir.z.abs();

        let (face, u_tan, v_tan) = if abs_x >= abs_y && abs_x >= abs_z {
            if dir.x > 0.0 { (PlanetFace::PosX, -dir.z / dir.x, -dir.y / dir.x) } else { (PlanetFace::NegX, -dir.z / dir.x, dir.y / dir.x) }
        } else if abs_y >= abs_x && abs_y >= abs_z {
            if dir.y > 0.0 { (PlanetFace::PosY, dir.x / dir.y, dir.z / dir.y) } else { (PlanetFace::NegY, -dir.x / dir.y, dir.z / dir.y) }
        } else {
            if dir.z > 0.0 { (PlanetFace::PosZ, dir.x / dir.z, -dir.y / dir.z) } else { (PlanetFace::NegZ, dir.x / dir.z, dir.y / dir.z) }
        };

        let inv_pi_4 = 4.0 / std::f64::consts::PI;
        let u = (u_tan.atan() * inv_pi_4).clamp(-1.0, 1.0);
        let v = (v_tan.atan() * inv_pi_4).clamp(-1.0, 1.0);
        (face, u, v)
    }

    /// Projette une direction 3D quelconque sur le plan de cette face et renvoie ses coordonnées (u, v).
    /// Renvoie `None` si la direction pointe dans le demi-espace arrière opposé à la normale de la face.
    pub fn project_to_face_uv(&self, dir: DVec3) -> Option<(f64, f64)> {
        let (u_tan, v_tan) = match self {
            PlanetFace::PosX => {
                if dir.x <= 1e-6 {
                    return None;
                }
                (-dir.z / dir.x, -dir.y / dir.x)
            }
            PlanetFace::NegX => {
                if dir.x >= -1e-6 {
                    return None;
                }
                (-dir.z / dir.x, dir.y / dir.x)
            }
            PlanetFace::PosY => {
                if dir.y <= 1e-6 {
                    return None;
                }
                (dir.x / dir.y, dir.z / dir.y)
            }
            PlanetFace::NegY => {
                if dir.y >= -1e-6 {
                    return None;
                }
                (-dir.x / dir.y, dir.z / dir.y)
            }
            PlanetFace::PosZ => {
                if dir.z <= 1e-6 {
                    return None;
                }
                (dir.x / dir.z, -dir.y / dir.z)
            }
            PlanetFace::NegZ => {
                if dir.z >= -1e-6 {
                    return None;
                }
                (dir.x / dir.z, dir.y / dir.z)
            }
        };

        let inv_pi_4 = 4.0 / std::f64::consts::PI;
        let u = u_tan.atan() * inv_pi_4;
        let v = v_tan.atan() * inv_pi_4;
        Some((u, v))
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
        Self { face, x, y, altitude_layer }
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
        (self.x as usize) + (self.z as usize) * Self::CHUNK_SIZE + (self.y as usize) * Self::CHUNK_SIZE * Self::CHUNK_SIZE
    }

    #[inline(always)]
    pub fn from_linear_index(idx: usize) -> Self {
        let y = idx / (Self::CHUNK_SIZE * Self::CHUNK_SIZE);
        let rem = idx % (Self::CHUNK_SIZE * Self::CHUNK_SIZE);
        let z = rem / Self::CHUNK_SIZE;
        let x = rem % Self::CHUNK_SIZE;
        Self { x: x as u8, y: y as u8, z: z as u8 }
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

    #[test]
    fn test_sphere_direction_to_face_uv_roundtrip() {
        for &face in &PlanetFace::ALL {
            for &u in &[-0.8, -0.2, 0.0, 0.4, 0.9] {
                for &v in &[-0.7, -0.1, 0.0, 0.5, 0.85] {
                    let dir = face.uv_to_sphere_direction(u, v);
                    let (rec_face, rec_u, rec_v) = PlanetFace::sphere_direction_to_face_uv(dir);
                    assert_eq!(face, rec_face, "Face mismatch for u={}, v={}", u, v);
                    assert!((u - rec_u).abs() < 1e-5, "u mismatch: {} vs {}", u, rec_u);
                    assert!((v - rec_v).abs() < 1e-5, "v mismatch: {} vs {}", v, rec_v);
                }
            }
        }
    }

    #[test]
    fn test_project_to_face_uv() {
        for &face in &PlanetFace::ALL {
            for &u in &[-0.8, -0.2, 0.0, 0.4, 0.9] {
                for &v in &[-0.7, -0.1, 0.0, 0.5, 0.85] {
                    let dir = face.uv_to_sphere_direction(u, v);
                    let res = face.project_to_face_uv(dir);
                    assert!(res.is_some());
                    let (rec_u, rec_v) = res.unwrap();
                    assert!((u - rec_u).abs() < 1e-5, "u mismatch: {} vs {}", u, rec_u);
                    assert!((v - rec_v).abs() < 1e-5, "v mismatch: {} vs {}", v, rec_v);
                }
            }
        }
    }
}

use glam::DVec3;
use herocraft_common::{PlanetChunkCoord, PlanetFace};
use serde::{Deserialize, Serialize};

/// Identifiant unique d'un nœud dans le QuadTree de la Cubed-Sphere
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuadNodeKey {
    pub face: PlanetFace,
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

impl QuadNodeKey {
    pub fn new(face: PlanetFace, level: u8, x: u32, y: u32) -> Self {
        Self { face, level, x, y }
    }

    /// Limites (u_min, u_max, v_min, v_max) dans le repère [-1.0, 1.0] de la face
    pub fn uv_bounds(&self) -> (f64, f64, f64, f64) {
        let count = (1u32 << self.level) as f64;
        let span = 2.0 / count;
        let u_min = -1.0 + (self.x as f64) * span;
        let u_max = u_min + span;
        let v_min = -1.0 + (self.y as f64) * span;
        let v_max = v_min + span;
        (u_min, u_max, v_min, v_max)
    }

    /// Direction unitaire 3D du centre du nœud sur la sphère
    pub fn center_direction(&self) -> DVec3 {
        let (u_min, u_max, v_min, v_max) = self.uv_bounds();
        let u_mid = (u_min + u_max) * 0.5;
        let v_mid = (v_min + v_max) * 0.5;
        self.face.uv_to_sphere_direction(u_mid, v_mid)
    }

    /// Convertit un nœud de niveau 5 (feuille voxel maximale) en coordonnée de chunk voxel
    pub fn to_chunk_coord(&self, altitude_layer: i32) -> Option<PlanetChunkCoord> {
        if self.level == 5 {
            Some(PlanetChunkCoord::new(
                self.face,
                self.x as i32,
                self.y as i32,
                altitude_layer,
            ))
        } else {
            None
        }
    }

    /// Les 4 nœuds enfants
    pub fn children(&self) -> [QuadNodeKey; 4] {
        let next_level = self.level + 1;
        let nx = self.x * 2;
        let ny = self.y * 2;
        [
            QuadNodeKey::new(self.face, next_level, nx, ny),
            QuadNodeKey::new(self.face, next_level, nx + 1, ny),
            QuadNodeKey::new(self.face, next_level, nx, ny + 1),
            QuadNodeKey::new(self.face, next_level, nx + 1, ny + 1),
        ]
    }
}

/// Nœud feuille résultant de l'évaluation du QuadTree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuadLeafKind {
    /// Nœud basse/moyenne altitude (LOD 0 à 4) rendu par un patch de terrain polygonal
    LodPatch,
    /// Nœud haute résolution au sol (LOD 5) rendu par de vrais voxels incurvés
    VoxelChunk,
}

#[derive(Debug, Clone, Copy)]
pub struct QuadLeaf {
    pub key: QuadNodeKey,
    pub kind: QuadLeafKind,
}

/// Évaluateur de QuadTree planétaire pour Cubed-Sphere
pub struct PlanetQuadTree {
    pub base_radius: f64,
    pub min_level: u8,
    pub max_level: u8,
    /// Seuils de distance caméra pour subdiviser chaque niveau (index = level 0..max_level)
    pub split_distances: Vec<f64>,
}

impl PlanetQuadTree {
    /// Crée un QuadTree configuré pour un rayon de planète donné
    pub fn new(base_radius: f64) -> Self {
        // On conserve tous les niveaux du QuadTree (Level 0 à 5),
        // mais avec une finesse de maillage adaptative par niveau (finesse élevée dès l'orbite).
        let min_level = 0;
        let max_level = 5;

        // Seuils de distance caméra-nœud pour déclencher la subdivision :
        // Level 0 (face globale) : split si distance < 95.0
        // Level 1 (quadrant)     : split si distance < 60.0
        // Level 2 (région)       : split si distance < 38.0
        // Level 3 (zone)         : split si distance < 20.0
        // Level 4 (local)        : split si distance < 7.5 (passage aux voxels 3D Level 5)
        let split_distances = vec![
            95.0,
            60.0,
            38.0,
            20.0,
            7.5,
        ];

        Self {
            base_radius,
            min_level,
            max_level,
            split_distances,
        }
    }

    /// Évalue l'arbre complet depuis la position de la caméra et renvoie la liste des feuilles visibles
    pub fn evaluate(&self, camera_pos: DVec3) -> Vec<QuadLeaf> {
        let mut leaves = Vec::with_capacity(256);

        // Toutes les 6 faces de la planète sont toujours conservées pour garantir
        // une planète 3D fermée et complète à 360° sans aucune face manquante.
        for &face in &PlanetFace::ALL {
            let root = QuadNodeKey::new(face, 0, 0, 0);
            self.subdivide_recursive(root, camera_pos, &mut leaves);
        }

        leaves
    }

    fn subdivide_recursive(
        &self,
        node: QuadNodeKey,
        camera_pos: DVec3,
        leaves: &mut Vec<QuadLeaf>,
    ) {
        let center_dir = node.center_direction();
        let center_pos = center_dir * self.base_radius;
        let dist_to_node = (camera_pos - center_pos).length();

        let should_split = node.level < self.min_level
            || ((node.level as usize) < self.split_distances.len()
                && node.level < self.max_level
                && dist_to_node < self.split_distances[node.level as usize]);

        if should_split {
            for child in node.children() {
                self.subdivide_recursive(child, camera_pos, leaves);
            }
        } else {
            let kind = if node.level == self.max_level {
                QuadLeafKind::VoxelChunk
            } else {
                QuadLeafKind::LodPatch
            };
            leaves.push(QuadLeaf { key: node, kind });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quadnode_uv_bounds() {
        let root = QuadNodeKey::new(PlanetFace::PosZ, 0, 0, 0);
        let (u0, u1, v0, v1) = root.uv_bounds();
        assert_eq!((u0, u1, v0, v1), (-1.0, 1.0, -1.0, 1.0));

        let child = QuadNodeKey::new(PlanetFace::PosZ, 1, 0, 0);
        let (u0, u1, v0, v1) = child.uv_bounds();
        assert_eq!((u0, u1, v0, v1), (-1.0, 0.0, -1.0, 0.0));

        let child_top_right = QuadNodeKey::new(PlanetFace::PosZ, 1, 1, 1);
        let (u0, u1, v0, v1) = child_top_right.uv_bounds();
        assert_eq!((u0, u1, v0, v1), (0.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn test_level_5_maps_to_chunk_coord() {
        let node_lvl5 = QuadNodeKey::new(PlanetFace::PosX, 5, 12, 18);
        let coord = node_lvl5.to_chunk_coord(0).unwrap();
        assert_eq!(coord.face, PlanetFace::PosX);
        assert_eq!(coord.x, 12);
        assert_eq!(coord.y, 18);
        assert_eq!(coord.altitude_layer, 0);

        let node_lvl4 = QuadNodeKey::new(PlanetFace::PosX, 4, 6, 9);
        assert!(node_lvl4.to_chunk_coord(0).is_none());
    }

    #[test]
    fn test_quadtree_evaluation_distant_camera() {
        let qt = PlanetQuadTree::new(25.0);
        // Caméra très éloignée dans l'espace (distance 150)
        let cam_pos = DVec3::new(0.0, 0.0, 150.0);
        let leaves = qt.evaluate(cam_pos);

        // Toutes les 6 faces doivent être présentes au niveau racine (Level 0)
        assert_eq!(leaves.len(), 6);
        for leaf in &leaves {
            assert_eq!(leaf.kind, QuadLeafKind::LodPatch);
            assert_eq!(leaf.key.level, 0);
        }
    }

    #[test]
    fn test_quadtree_evaluation_close_surface_camera() {
        let qt = PlanetQuadTree::new(25.0);
        // Caméra très proche de la surface sur l'axe +Z (rayon 25.0 + 1.5 = 26.5)
        let cam_pos = DVec3::new(0.0, 0.0, 26.5);
        let leaves = qt.evaluate(cam_pos);

        // Il doit y avoir des feuilles VoxelChunk (level 5) directement sous la caméra
        let voxel_leaves: Vec<_> = leaves
            .iter()
            .filter(|l| l.kind == QuadLeafKind::VoxelChunk)
            .collect();
        assert!(!voxel_leaves.is_empty());

        // Et des feuilles LodPatch plus loin vers l'horizon
        let lod_leaves: Vec<_> = leaves
            .iter()
            .filter(|l| l.kind == QuadLeafKind::LodPatch)
            .collect();
        assert!(!lod_leaves.is_empty());
    }
}

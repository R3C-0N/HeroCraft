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
    pub max_level: u8,
    /// Seuils de distance caméra pour subdiviser chaque niveau (index = level 0..max_level)
    pub split_distances: Vec<f64>,
}

impl PlanetQuadTree {
    /// Crée un QuadTree configuré pour un rayon de planète donné
    pub fn new(base_radius: f64) -> Self {
        // Seuils de subdivision calculés pour assurer une transition fluide
        // Rayon ~25.0 :
        // Seuils de distance caméra-nœud calibrés selon la taille réelle des cellules :
        // Rayon 25.0 (périmètre ~157.0) :
        // Level 0 (taille ~40) : split si distance < 80.0
        // Level 1 (taille ~20) : split si distance < 45.0
        // Level 2 (taille ~10) : split si distance < 25.0
        // Level 3 (taille ~5.0): split si distance < 12.0
        // Level 4 (taille ~2.5): split vers Level 5 (voxels) uniquement si distance < 4.5
        // Cela garantit qu'au sol, seuls 4 à 9 chunks voxels locaux sont activés (pas de freeze !)
        let split_distances = vec![
            80.0,
            45.0,
            25.0,
            12.0,
            4.5,
        ];

        Self {
            base_radius,
            max_level: 5,
            split_distances,
        }
    }

    /// Évalue l'arbre complet depuis la position de la caméra et renvoie la liste des feuilles visibles
    pub fn evaluate(&self, camera_pos: DVec3) -> Vec<QuadLeaf> {
        let mut leaves = Vec::with_capacity(128);
        let cam_dist = camera_pos.length();
        let cam_dir = if cam_dist > 1e-4 {
            camera_pos / cam_dist
        } else {
            DVec3::new(0.0, 0.0, 1.0)
        };

        for &face in &PlanetFace::ALL {
            let root = QuadNodeKey::new(face, 0, 0, 0);
            self.subdivide_recursive(root, camera_pos, cam_dir, &mut leaves);
        }

        leaves
    }

    fn subdivide_recursive(
        &self,
        node: QuadNodeKey,
        camera_pos: DVec3,
        cam_dir: DVec3,
        leaves: &mut Vec<QuadLeaf>,
    ) {
        let center_dir = node.center_direction();
        let center_pos = center_dir * self.base_radius;

        // Culling sécurisé des faces arrière :
        // Si dot < -0.35, le centre du nœud est à plus de 110° par rapport à la caméra
        // donc complètement sur la face cachée de la planète.
        let dot = center_dir.dot(cam_dir);
        if dot < -0.35 {
            return;
        }

        let dist_to_node = (camera_pos - center_pos).length();

        let should_split = (node.level as usize) < self.split_distances.len()
            && node.level < self.max_level
            && dist_to_node < self.split_distances[node.level as usize];

        if should_split {
            for child in node.children() {
                self.subdivide_recursive(child, camera_pos, cam_dir, leaves);
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

        // Toutes les feuilles doivent être de faible niveau (LOD patchs)
        assert!(!leaves.is_empty());
        for leaf in &leaves {
            assert_eq!(leaf.kind, QuadLeafKind::LodPatch);
            assert!(leaf.key.level <= 1);
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

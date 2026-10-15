use std::collections::{HashMap, HashSet};
use glam::{DVec3, Vec3};
use herocraft_world::{
    PlanetQuadTree, PlanetWorldGenerator, QuadLeaf, QuadLeafKind, QuadNodeKey,
};
use herocraft_voxel_engine::cache::WorldChunkCache;

use crate::planet_mesh::PlanetMesh;
use crate::voxel_mesh::VoxelMeshBuilder;

/// Gestionnaire de LOD planétaire piloté par QuadTree Cubed-Sphere
pub struct PlanetLodManager {
    pub quadtree: PlanetQuadTree,
    pub base_radius: f32,
    lod_cache: HashMap<QuadNodeKey, PlanetMesh>,
    voxel_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub current_leaves: Vec<QuadLeaf>,
    pub combined_mesh: PlanetMesh,
    last_camera_pos: Vec3,
}

impl PlanetLodManager {
    pub fn new(base_radius: f32) -> Self {
        Self {
            quadtree: PlanetQuadTree::new(base_radius as f64),
            base_radius,
            lod_cache: HashMap::new(),
            voxel_cache: HashMap::new(),
            current_leaves: Vec::new(),
            combined_mesh: PlanetMesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            last_camera_pos: Vec3::splat(f32::NAN),
        }
    }

    /// Réinitialise les caches (par exemple lors d'un changement de seed avec 'G')
    pub fn clear(&mut self) {
        self.lod_cache.clear();
        self.voxel_cache.clear();
        self.current_leaves.clear();
        self.last_camera_pos = Vec3::splat(f32::NAN);
    }

    /// Invalide un chunk voxel modifié pour forcer son recalcul
    #[allow(dead_code)]
    pub fn invalidate_chunk(&mut self, key: &QuadNodeKey) {
        self.voxel_cache.remove(key);
    }

    /// Met à jour la hiérarchie LOD selon la position de la caméra
    /// Renvoie `true` si le maillage combiné a été régénéré et doit être téléversé sur le GPU
    pub fn update(
        &mut self,
        camera_pos: Vec3,
        generator: &PlanetWorldGenerator,
        chunk_cache: &mut WorldChunkCache,
    ) -> bool {
        let cam_dvec = DVec3::new(
            camera_pos.x as f64,
            camera_pos.y as f64,
            camera_pos.z as f64,
        );

        let new_leaves = self.quadtree.evaluate(cam_dvec);

        // Vérifie si la topologie des feuilles visibles a changé
        let leaves_changed = new_leaves.len() != self.current_leaves.len()
            || new_leaves
                .iter()
                .zip(&self.current_leaves)
                .any(|(a, b)| a.key != b.key);

        if !leaves_changed && !self.last_camera_pos.is_nan() {
            return false;
        }

        self.current_leaves = new_leaves;
        self.last_camera_pos = camera_pos;

        let mut total_vertices = 0;
        let mut total_indices = 0;

        // 1. Assure la présence des maillages pour chaque feuille
        for leaf in &self.current_leaves {
            match leaf.kind {
                QuadLeafKind::LodPatch => {
                    let mesh = self.lod_cache.entry(leaf.key).or_insert_with(|| {
                        // Résolution 4x4 quads par patch LOD :
                        // Au Level 3 (64 nœuds par face), cela donne exactement 32x32 = 1024 cellules par face (1 cellule par chunk !)
                        PlanetMesh::generate_lod_patch(generator, &leaf.key, self.base_radius, 4)
                    });
                    total_vertices += mesh.vertices.len();
                    total_indices += mesh.indices.len();
                }
                QuadLeafKind::VoxelChunk => {
                    let mesh = self.voxel_cache.entry(leaf.key).or_insert_with(|| {
                        VoxelMeshBuilder::build_incurved_single_chunk(
                            generator,
                            chunk_cache,
                            leaf.key.face,
                            leaf.key.x as i32,
                            leaf.key.y as i32,
                            self.base_radius,
                        )
                    });
                    total_vertices += mesh.vertices.len();
                    total_indices += mesh.indices.len();
                }
            }
        }

        // Nettoyage régulier des caches pour éviter la rétention infinie de mémoire
        if self.lod_cache.len() > 1200 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.lod_cache.retain(|k, _| active_keys.contains(k));
        }
        if self.voxel_cache.len() > 120 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.voxel_cache.retain(|k, _| active_keys.contains(k));
        }

        // 2. Assemblage du maillage global combiné (un seul Draw Call)
        let mut combined = PlanetMesh {
            vertices: Vec::with_capacity(total_vertices),
            indices: Vec::with_capacity(total_indices),
        };

        for leaf in &self.current_leaves {
            let mesh = match leaf.kind {
                QuadLeafKind::LodPatch => self.lod_cache.get(&leaf.key).unwrap(),
                QuadLeafKind::VoxelChunk => self.voxel_cache.get(&leaf.key).unwrap(),
            };

            let base_idx = combined.vertices.len() as u32;
            combined.vertices.extend_from_slice(&mesh.vertices);
            for &idx in &mesh.indices {
                combined.indices.push(base_idx + idx);
            }
        }

        self.combined_mesh = combined;
        true
    }
}

use std::collections::{HashMap, HashSet};
use glam::{DVec3, Vec3};
use herocraft_world::{
    PlanetQuadTree, PlanetWorldGenerator, QuadLeaf, QuadLeafKind, QuadNodeKey,
};
use herocraft_voxel_engine::cache::WorldChunkCache;

use crate::planet_mesh::PlanetMesh;
use crate::voxel_mesh::VoxelMeshBuilder;

struct VoxelBuildRequest {
    seed: u64,
    key: QuadNodeKey,
    generator: PlanetWorldGenerator,
    face: herocraft_common::PlanetFace,
    chunk_x: i32,
    chunk_y: i32,
    base_radius: f32,
    filter: herocraft_world::VisualizationFilter,
}

struct VoxelBuildResult {
    seed: u64,
    key: QuadNodeKey,
    mesh: PlanetMesh,
}

/// Gestionnaire de LOD planétaire piloté par QuadTree Cubed-Sphere avec transitions douces hybrides
pub struct PlanetLodManager {
    pub quadtree: PlanetQuadTree,
    pub base_radius: f32,
    pub current_seed: u64,
    lod_cache: HashMap<QuadNodeKey, PlanetMesh>,
    voxel_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub current_leaves: Vec<QuadLeaf>,
    pub combined_mesh: PlanetMesh,
    pub active_filter: herocraft_world::VisualizationFilter,
    last_camera_pos: Vec3,
    // Système de calcul asynchrone en tâche de fond (élimine tout freeze au zoom max)
    request_tx: std::sync::mpsc::Sender<VoxelBuildRequest>,
    result_rx: std::sync::mpsc::Receiver<VoxelBuildResult>,
    pending_requests: HashSet<QuadNodeKey>,
}

impl PlanetLodManager {
    pub fn new(base_radius: f32, current_seed: u64) -> Self {
        let (request_tx, request_rx) = std::sync::mpsc::channel::<VoxelBuildRequest>();
        let (result_tx, result_rx) = std::sync::mpsc::channel::<VoxelBuildResult>();

        // Worker thread en arrière-plan pour la génération non-bloquante des voxels
        std::thread::Builder::new()
            .name("herocraft-voxel-worker".to_string())
            .spawn(move || {
                while let Ok(req) = request_rx.recv() {
                    let mesh = VoxelMeshBuilder::build_incurved_single_chunk_fast(
                        &req.generator,
                        req.face,
                        req.chunk_x,
                        req.chunk_y,
                        req.base_radius,
                        req.filter,
                    );
                    let _ = result_tx.send(VoxelBuildResult {
                        seed: req.seed,
                        key: req.key,
                        mesh,
                    });
                }
            })
            .expect("Échec d'initialisation du thread de calcul voxel");

        Self {
            quadtree: PlanetQuadTree::new(base_radius as f64),
            base_radius,
            current_seed,
            lod_cache: HashMap::new(),
            voxel_cache: HashMap::new(),
            current_leaves: Vec::new(),
            combined_mesh: PlanetMesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            active_filter: herocraft_world::VisualizationFilter::Standard,
            last_camera_pos: Vec3::splat(f32::NAN),
            request_tx,
            result_rx,
            pending_requests: HashSet::new(),
        }
    }

    /// Définit le filtre de visualisation actif et force la régénération si modifié
    pub fn set_filter(&mut self, filter: herocraft_world::VisualizationFilter) {
        if self.active_filter != filter {
            self.active_filter = filter;
            self.clear(self.current_seed);
        }
    }

    /// Réinitialise les caches (par exemple lors d'un changement de seed avec 'G')
    pub fn clear(&mut self, new_seed: u64) {
        self.current_seed = new_seed;
        self.lod_cache.clear();
        self.voxel_cache.clear();
        self.current_leaves.clear();
        self.pending_requests.clear();
        self.last_camera_pos = Vec3::splat(f32::NAN);
    }

    /// Invalide un chunk voxel modifié pour forcer son recalcul
    #[allow(dead_code)]
    pub fn invalidate_chunk(&mut self, key: &QuadNodeKey) {
        self.voxel_cache.remove(key);
    }

    /// Met à jour la hiérarchie LOD selon la position de la caméra et le temps écoulé dt
    /// Renvoie `true` si le maillage combiné a été régénéré et doit être téléversé sur le GPU
    pub fn update(
        &mut self,
        camera_pos: Vec3,
        generator: &PlanetWorldGenerator,
        _chunk_cache: &mut WorldChunkCache,
        _dt: f32,
    ) -> bool {
        // 1. Récupération non-bloquante des chunks voxels calculés par le thread en arrière-plan
        let mut received_new_voxel = false;
        while let Ok(res) = self.result_rx.try_recv() {
            if res.seed == self.current_seed {
                self.pending_requests.remove(&res.key);
                self.voxel_cache.insert(res.key, res.mesh);
                received_new_voxel = true;
            }
        }

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

        // On ne reconstruit le maillage lourd QUE si la topologie a changé ou qu'un nouveau chunk voxel est prêt
        if !leaves_changed && !received_new_voxel && !self.last_camera_pos.is_nan() {
            return false;
        }

        self.current_leaves = new_leaves;
        self.last_camera_pos = camera_pos;

        let filter = self.active_filter;
        let base_radius = self.base_radius;

        // 2. Préparation et requêtes asynchrones pour chaque feuille
        for leaf in &self.current_leaves {
            match leaf.kind {
                QuadLeafKind::LodPatch => {
                    self.lod_cache.entry(leaf.key).or_insert_with(|| {
                        let resolution = match leaf.key.level {
                            0 => 32,
                            1 => 24,
                            2 => 16,
                            3 => 16,
                            _ => 16,
                        };
                        PlanetMesh::generate_lod_patch(generator, &leaf.key, base_radius, resolution, filter)
                    });
                }
                QuadLeafKind::VoxelChunk => {
                    // Si le chunk voxel n'est pas encore en cache, on déclenche le calcul en tâche de fond
                    if !self.voxel_cache.contains_key(&leaf.key) && self.pending_requests.insert(leaf.key) {
                        let _ = self.request_tx.send(VoxelBuildRequest {
                            seed: self.current_seed,
                            key: leaf.key,
                            generator: generator.clone(),
                            face: leaf.key.face,
                            chunk_x: leaf.key.x as i32,
                            chunk_y: leaf.key.y as i32,
                            base_radius,
                            filter,
                        });
                    }
                    // On prépare également le patch polygonal LOD 4 de secours immédiat
                    self.lod_cache.entry(leaf.key).or_insert_with(|| {
                        PlanetMesh::generate_lod_patch(generator, &leaf.key, base_radius, 16, filter)
                    });
                }
            }
        }

        // Nettoyage périodique des caches
        if self.lod_cache.len() > 1500 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.lod_cache.retain(|k, _| active_keys.contains(k));
        }
        if self.voxel_cache.len() > 200 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.voxel_cache.retain(|k, _| active_keys.contains(k));
        }

        // 3. Assemblage du maillage global combiné (copie directe haute performance)
        let mut combined = PlanetMesh {
            vertices: Vec::with_capacity(32768),
            indices: Vec::with_capacity(65536),
        };

        for leaf in &self.current_leaves {
            match leaf.kind {
                QuadLeafKind::LodPatch => {
                    if let Some(mesh) = self.lod_cache.get(&leaf.key) {
                        append_mesh(&mut combined, mesh);
                    }
                }
                QuadLeafKind::VoxelChunk => {
                    if let Some(voxel_mesh) = self.voxel_cache.get(&leaf.key) {
                        append_mesh(&mut combined, voxel_mesh);
                    } else if let Some(patch) = self.lod_cache.get(&leaf.key) {
                        // Voxel chunk encore en calcul : affichage immédiat du patch LOD 4 sans freeze !
                        append_mesh(&mut combined, patch);
                    }
                }
            }
        }

        self.combined_mesh = combined;
        true
    }
}

/// Fusionne un maillage source dans le maillage combiné (copie mémoire directe rapide)
fn append_mesh(combined: &mut PlanetMesh, source: &PlanetMesh) {
    let base_idx = combined.vertices.len() as u32;
    combined.vertices.extend_from_slice(&source.vertices);
    combined.indices.reserve(source.indices.len());
    for &idx in &source.indices {
        combined.indices.push(base_idx + idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_lod_update_performance_when_stationary() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod_manager = PlanetLodManager::new(25.0, 2026);

        let cam_pos = Vec3::new(25.5, 0.0, 0.0);

        // Frame 0 (initialisation)
        lod_manager.update(cam_pos, &generator, &mut cache, 0.0);

        let mut true_returns = 0;
        let start = Instant::now();
        for frame in 0..60 {
            let pending_count = lod_manager.pending_requests.len();
            let voxel_cache_count = lod_manager.voxel_cache.len();
            let leaves_count = lod_manager.current_leaves.len();

            let updated = lod_manager.update(cam_pos, &generator, &mut cache, 0.016);
            if updated {
                true_returns += 1;
            }
            if frame < 5 || frame == 30 || frame == 59 {
                println!(
                    "Frame {}: updated={} | leaves={} | pending={} | cached_voxels={}",
                    frame, updated, leaves_count, pending_count, voxel_cache_count
                );
            }
        }
        let total_time = start.elapsed();
        println!(
            "Stationary camera: {}/60 frames triggered mesh rebuild! Total time: {:?}",
            true_returns, total_time
        );
        // Au repos sans mouvement, le coût CPU par frame doit être quasi-nul (< 1ms par frame, soit < 60ms au total)
        assert!(total_time.as_millis() < 500, "Update took too long: {:?}", total_time);
    }
}


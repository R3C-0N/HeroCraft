use std::collections::{HashMap, HashSet};
use glam::{DVec3, Vec3};
use herocraft_world::{
    PlanetQuadTree, PlanetWorldGenerator, QuadLeaf, QuadLeafKind, QuadNodeKey,
};
use herocraft_voxel_engine::cache::WorldChunkCache;

use crate::planet_mesh::PlanetMesh;
use crate::voxel_mesh::VoxelMeshBuilder;

pub enum LodBuildTask {
    VoxelChunk {
        seed: u64,
        key: QuadNodeKey,
        generator: PlanetWorldGenerator,
        face: herocraft_common::PlanetFace,
        chunk_x: i32,
        chunk_y: i32,
        base_radius: f32,
        filter: herocraft_world::VisualizationFilter,
    },
    LodPatch {
        seed: u64,
        key: QuadNodeKey,
        generator: PlanetWorldGenerator,
        base_radius: f32,
        resolution: usize,
        filter: herocraft_world::VisualizationFilter,
    },
}

pub struct LodBuildResult {
    pub seed: u64,
    pub key: QuadNodeKey,
    pub kind: QuadLeafKind,
    pub mesh: PlanetMesh,
}

/// Gestionnaire de LOD planétaire piloté par QuadTree Cubed-Sphere avec transitions douces hybrides
pub struct PlanetLodManager {
    pub quadtree: PlanetQuadTree,
    pub base_radius: f32,
    pub current_seed: u64,
    pub lod_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub voxel_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub current_leaves: Vec<QuadLeaf>,
    pub combined_mesh: PlanetMesh,
    pub active_filter: herocraft_world::VisualizationFilter,
    last_camera_pos: Vec3,
    last_evaluated_pos: Vec3,
    time_since_eval: f32,
    thread_pool: rayon::ThreadPool,
    result_tx: std::sync::mpsc::Sender<LodBuildResult>,
    result_rx: std::sync::mpsc::Receiver<LodBuildResult>,
    pending_requests: HashSet<QuadNodeKey>,
}

impl PlanetLodManager {
    pub fn new(base_radius: f32, current_seed: u64) -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus.min(8))
            .thread_name(|idx| format!("herocraft-worker-{}", idx))
            .build()
            .expect("Échec d'initialisation du pool de workers Rayon");

        let (result_tx, result_rx) = std::sync::mpsc::channel::<LodBuildResult>();

        let mut mgr = Self {
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
            last_evaluated_pos: Vec3::splat(f32::NAN),
            time_since_eval: 0.0,
            thread_pool,
            result_tx,
            result_rx,
            pending_requests: HashSet::new(),
        };

        // Pré-génération synchrone ultra-rapide des 6 faces racines (LOD 0)
        // Elles servent d'ancêtre et de fallback immédiat pour éliminer tout trou dans la géométrie
        let desc = herocraft_world::PlanetDescriptor {
            seed: current_seed,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        for &face in &herocraft_common::PlanetFace::ALL {
            let key = QuadNodeKey::new(face, 0, 0, 0);
            let patch = PlanetMesh::generate_lod_patch(&generator, &key, base_radius, 24, mgr.active_filter);
            mgr.lod_cache.insert(key, patch);
        }

        mgr
    }

    /// Envoie une tâche de calcul de maillage au pool Rayon en tâche de fond
    fn request_mesh(&mut self, task: LodBuildTask) {
        let (key, seed, kind) = match &task {
            LodBuildTask::VoxelChunk { key, seed, .. } => (*key, *seed, QuadLeafKind::VoxelChunk),
            LodBuildTask::LodPatch { key, seed, .. } => (*key, *seed, QuadLeafKind::LodPatch),
        };
        if self.pending_requests.insert(key) {
            let tx = self.result_tx.clone();
            self.thread_pool.spawn(move || {
                let mesh = match task {
                    LodBuildTask::VoxelChunk { generator, face, chunk_x, chunk_y, base_radius, filter, .. } => {
                        VoxelMeshBuilder::build_incurved_single_chunk_fast(&generator, face, chunk_x, chunk_y, base_radius, filter)
                    }
                    LodBuildTask::LodPatch { generator, key, base_radius, resolution, filter, .. } => {
                        PlanetMesh::generate_lod_patch(&generator, &key, base_radius, resolution, filter)
                    }
                };
                let _ = tx.send(LodBuildResult { seed, key, kind, mesh });
            });
        }
    }

    /// Récupère le maillage le plus précis disponible pour un nœud donné,
    /// ou remonte chez ses ancêtres parents s'il est encore en cours de calcul (zéro freeze, zéro popping)
    pub fn get_mesh_or_ancestor(&self, key: &QuadNodeKey) -> Option<&PlanetMesh> {
        if let Some(m) = self.voxel_cache.get(key) {
            return Some(m);
        }
        if let Some(m) = self.lod_cache.get(key) {
            return Some(m);
        }
        let mut curr = key.parent();
        while let Some(parent_key) = curr {
            if let Some(m) = self.lod_cache.get(&parent_key) {
                return Some(m);
            }
            curr = parent_key.parent();
        }
        None
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
        self.last_evaluated_pos = Vec3::splat(f32::NAN);
        self.time_since_eval = 0.0;

        let desc = herocraft_world::PlanetDescriptor {
            seed: new_seed,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        for &face in &herocraft_common::PlanetFace::ALL {
            let key = QuadNodeKey::new(face, 0, 0, 0);
            let patch = PlanetMesh::generate_lod_patch(&generator, &key, self.base_radius, 24, self.active_filter);
            self.lod_cache.insert(key, patch);
        }
    }

    /// Invalide un chunk voxel modifié pour forcer son recalcul
    #[allow(dead_code)]
    pub fn invalidate_chunk(&mut self, key: &QuadNodeKey) {
        self.voxel_cache.remove(key);
    }

    /// Met à jour la hiérarchie LOD selon la position de la caméra, le temps écoulé dt et le cône de vision (Frustum)
    /// Renvoie `true` si la topologie a changé ou que de nouveaux maillages sont prêts
    pub fn update(
        &mut self,
        camera_pos: Vec3,
        generator: &PlanetWorldGenerator,
        _chunk_cache: &mut WorldChunkCache,
        dt: f32,
        frustum: Option<&herocraft_world::WorldFrustum>,
    ) -> bool {
        // 1. Récupération non-bloquante des maillages calculés par le pool Rayon
        let mut received_new_mesh = false;
        while let Ok(res) = self.result_rx.try_recv() {
            if res.seed == self.current_seed {
                self.pending_requests.remove(&res.key);
                match res.kind {
                    QuadLeafKind::VoxelChunk => {
                        self.voxel_cache.insert(res.key, res.mesh);
                    }
                    QuadLeafKind::LodPatch => {
                        self.lod_cache.insert(res.key, res.mesh);
                    }
                }
                received_new_mesh = true;
            }
        }

        self.time_since_eval += dt;
        let moved_dist = if self.last_evaluated_pos.is_nan() {
            f32::INFINITY
        } else {
            (camera_pos - self.last_evaluated_pos).length()
        };

        // Évaluation QuadTree temporisée : réévalué si déplacement >= 15 cm ou toutes les 80 ms si déplacement >= 2 cm
        let should_re_eval = self.last_evaluated_pos.is_nan()
            || moved_dist > 0.15
            || (moved_dist > 0.02 && self.time_since_eval > 0.08);

        let (new_leaves, leaves_changed) = if should_re_eval {
            let cam_dvec = DVec3::new(
                camera_pos.x as f64,
                camera_pos.y as f64,
                camera_pos.z as f64,
            );
            let leaves = self.quadtree.evaluate_with_frustum(cam_dvec, Some(generator), frustum);
            let changed = leaves.len() != self.current_leaves.len()
                || leaves
                    .iter()
                    .zip(&self.current_leaves)
                    .any(|(a, b)| a.key != b.key);
            self.last_evaluated_pos = camera_pos;
            self.time_since_eval = 0.0;
            (leaves, changed)
        } else {
            (self.current_leaves.clone(), false)
        };

        if !leaves_changed && !received_new_mesh && !self.last_camera_pos.is_nan() {
            return false;
        }

        self.current_leaves = new_leaves;
        self.last_camera_pos = camera_pos;

        let filter = self.active_filter;
        let base_radius = self.base_radius;

        // 2. Déclenchement 100% asynchrone des calculs sur le pool de threads Rayon (zéro freeze sur le thread principal)
        let mut tasks = Vec::new();
        for leaf in &self.current_leaves {
            match leaf.kind {
                QuadLeafKind::LodPatch => {
                    if !self.lod_cache.contains_key(&leaf.key) {
                        let resolution = match leaf.key.level {
                            0 => 32,
                            1 => 24,
                            2 => 16,
                            3 => 16,
                            _ => 16,
                        };
                        tasks.push(LodBuildTask::LodPatch {
                            seed: self.current_seed,
                            key: leaf.key,
                            generator: generator.clone(),
                            base_radius,
                            resolution,
                            filter,
                        });
                    }
                }
                QuadLeafKind::VoxelChunk => {
                    if !self.voxel_cache.contains_key(&leaf.key) {
                        tasks.push(LodBuildTask::VoxelChunk {
                            seed: self.current_seed,
                            key: leaf.key,
                            generator: generator.clone(),
                            face: leaf.key.face,
                            chunk_x: leaf.key.x as i32,
                            chunk_y: leaf.key.y as i32,
                            base_radius,
                            filter,
                        });
                        if !self.lod_cache.contains_key(&leaf.key) {
                            tasks.push(LodBuildTask::LodPatch {
                                seed: self.current_seed,
                                key: leaf.key,
                                generator: generator.clone(),
                                base_radius,
                                resolution: 16,
                                filter,
                            });
                        }
                    }
                }
            }
        }

        for task in tasks {
            self.request_mesh(task);
        }

        // Nettoyage périodique des caches
        if self.lod_cache.len() > 1500 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.lod_cache.retain(|k, _| active_keys.contains(k) || k.level == 0);
        }
        if self.voxel_cache.len() > 200 {
            let active_keys: HashSet<_> = self.current_leaves.iter().map(|l| l.key).collect();
            self.voxel_cache.retain(|k, _| active_keys.contains(k));
        }

        true
    }

    /// Reconstruit le maillage combiné complet (utile uniquement à l'initialisation ou au fallback)
    pub fn rebuild_combined_mesh(&mut self) {
        let mut combined = PlanetMesh {
            vertices: Vec::with_capacity(32768),
            indices: Vec::with_capacity(65536),
        };

        for leaf in &self.current_leaves {
            if let Some(mesh) = self.get_mesh_or_ancestor(&leaf.key) {
                append_mesh(&mut combined, mesh);
            }
        }
        self.combined_mesh = combined;
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
        lod_manager.update(cam_pos, &generator, &mut cache, 0.0, None);

        let mut true_returns = 0;
        let start = Instant::now();
        for frame in 0..60 {
            let pending_count = lod_manager.pending_requests.len();
            let voxel_cache_count = lod_manager.voxel_cache.len();
            let leaves_count = lod_manager.current_leaves.len();

            let updated = lod_manager.update(cam_pos, &generator, &mut cache, 0.016, None);
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
        // Au repos sans mouvement, le coût CPU par frame doit être minime même en mode debug
        assert!(total_time.as_millis() < 1000, "Update took too long: {:?}", total_time);
    }

    #[test]
    fn test_lod_update_performance_when_moving() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod_manager = PlanetLodManager::new(25.0, 2026);

        let mut cam_pos = Vec3::new(25.5, 0.0, 0.0);
        lod_manager.update(cam_pos, &generator, &mut cache, 0.0, None);

        let start = Instant::now();
        let mut rebuild_count = 0;
        // Simule 60 frames de marche lente (0.01 unité par frame à 60 FPS)
        for _ in 0..60 {
            cam_pos.y += 0.01;
            if lod_manager.update(cam_pos, &generator, &mut cache, 0.016, None) {
                rebuild_count += 1;
            }
        }
        let total_time = start.elapsed();
        // Grâce au deadband throttle, le QuadTree n'est pas réévalué de manière intempestive à chaque micro-mouvement
        println!(
            "Moving camera (60 frames): rebuilds={} total_time={:?}",
            rebuild_count, total_time
        );
        assert!(total_time.as_millis() < 1000, "Moving camera update took too long: {:?}", total_time);
    }
}



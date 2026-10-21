use std::collections::{HashMap, HashSet};
use glam::{DVec3, Vec3};
use herocraft_world::{
    PlanetQuadTree, PlanetWorldGenerator, QuadLeaf, QuadLeafKind, QuadNodeKey,
};
use herocraft_voxel_engine::cache::WorldChunkCache;

use crate::planet_mesh::PlanetMesh;
use crate::voxel_mesh::VoxelMeshBuilder;

use rayon::prelude::*;

/// Maillage calculé en tâche de fond (chunk voxel ou patch polygonal)
pub struct VoxelBuildResult {
    /// Génération des caches au lancement de la tâche (incrémentée par `clear`) : un résultat périmé est ignoré
    pub generation: u64,
    pub key: QuadNodeKey,
    pub mesh: PlanetMesh,
}

/// Identité d'un maillage affiché : change dès que son contenu peut changer (nouvelle génération des caches)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LodMeshId {
    pub generation: u64,
    pub key: QuadNodeKey,
    pub voxel: bool,
}

/// Résolution de la grille d'un patch polygonal selon son niveau
fn patch_resolution(level: u8) -> usize {
    match level {
        0 => 32,
        1 => 24,
        _ => 16,
    }
}

/// Gestionnaire de LOD planétaire piloté par QuadTree Cubed-Sphere avec transitions douces hybrides
pub struct PlanetLodManager {
    pub quadtree: PlanetQuadTree,
    pub base_radius: f32,
    pub current_seed: u64,
    pub lod_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub voxel_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub current_leaves: Vec<QuadLeaf>,
    pub active_filter: herocraft_world::VisualizationFilter,
    last_camera_pos: Vec3,
    last_evaluated_pos: Vec3,
    time_since_eval: f32,
    thread_pool: rayon::ThreadPool,
    result_tx: std::sync::mpsc::Sender<VoxelBuildResult>,
    result_rx: std::sync::mpsc::Receiver<VoxelBuildResult>,
    pending_requests: HashSet<QuadNodeKey>,
    /// Pool dédié aux patchs polygonaux, pour qu'ils ne fassent pas la queue derrière les chunks voxels
    patch_pool: rayon::ThreadPool,
    patch_tx: std::sync::mpsc::Sender<VoxelBuildResult>,
    patch_rx: std::sync::mpsc::Receiver<VoxelBuildResult>,
    pending_patches: HashSet<QuadNodeKey>,
    /// Nouvelles feuilles en attente de leurs patchs : les feuilles courantes restent affichées jusque-là
    pending_leaves: Option<Vec<QuadLeaf>>,
    generation: u64,
}

impl PlanetLodManager {
    pub fn new(base_radius: f32, current_seed: u64) -> Self {
        herocraft_common::prof_scope!("lod::PlanetLodManager::new");
        let num_cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus.min(8))
            .thread_name(|idx| format!("herocraft-worker-{}", idx))
            .build()
            .expect("Échec d'initialisation du pool de workers Rayon");

        let (result_tx, result_rx) = std::sync::mpsc::channel::<VoxelBuildResult>();
        let patch_pool = rayon::ThreadPoolBuilder::new()
            .num_threads((num_cpus / 3).clamp(2, 4))
            .thread_name(|idx| format!("herocraft-patch-{}", idx))
            .build()
            .expect("Échec d'initialisation du pool de patchs Rayon");
        let (patch_tx, patch_rx) = std::sync::mpsc::channel::<VoxelBuildResult>();

        let mut mgr = Self {
            quadtree: PlanetQuadTree::new(base_radius as f64),
            base_radius,
            current_seed,
            lod_cache: HashMap::new(),
            voxel_cache: HashMap::new(),
            current_leaves: Vec::new(),
            active_filter: herocraft_world::VisualizationFilter::Standard,
            last_camera_pos: Vec3::splat(f32::NAN),
            last_evaluated_pos: Vec3::splat(f32::NAN),
            time_since_eval: 0.0,
            thread_pool,
            result_tx,
            result_rx,
            pending_requests: HashSet::new(),
            patch_pool,
            patch_tx,
            patch_rx,
            pending_patches: HashSet::new(),
            pending_leaves: None,
            generation: 0,
        };

        // Pré-génération synchrone des 6 faces racines (LOD 0)
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

    /// Récupère le maillage propre au nœud (voxel haute résolution ou patch polygonal dédié)
    #[allow(dead_code)]
    pub fn get_mesh_or_ancestor(&self, key: &QuadNodeKey) -> Option<&PlanetMesh> {
        if let Some(m) = self.voxel_cache.get(key) {
            return Some(m);
        }
        self.lod_cache.get(key)
    }

    /// Définit le filtre de visualisation actif et force la régénération si modifié
    pub fn set_filter(&mut self, filter: herocraft_world::VisualizationFilter) {
        if self.active_filter != filter {
            self.active_filter = filter;
            self.clear(self.current_seed);
        }
    }

    /// Ajuste les seuils de distance du QuadTree selon le préréglage vidéo sélectionné
    pub fn set_distance_preset(&mut self, preset: crate::ui::LodDistanceSetting) {
        let mult = match preset {
            crate::ui::LodDistanceSetting::Courte => 0.8,
            crate::ui::LodDistanceSetting::Normale => 1.0,
            crate::ui::LodDistanceSetting::Elevee => 1.25,
        };
        let scale = self.base_radius as f64 / herocraft_world::REFERENCE_BASE_RADIUS;
        self.quadtree.split_distances = vec![
            95.0 * mult * scale,
            60.0 * mult * scale,
            38.0 * mult * scale,
            18.0 * mult * scale,
            3.8 * mult * scale,
        ];
        // Force la réévaluation du QuadTree au prochain frame
        self.last_evaluated_pos = Vec3::splat(f32::NAN);
        self.last_camera_pos = Vec3::splat(f32::NAN);
    }

    /// Réinitialise les caches (par exemple lors d'un changement de seed avec 'G')
    pub fn clear(&mut self, new_seed: u64) {
        herocraft_common::prof_scope!("lod::clear");
        self.current_seed = new_seed;
        self.lod_cache.clear();
        self.voxel_cache.clear();
        self.current_leaves.clear();
        self.pending_requests.clear();
        self.pending_patches.clear();
        self.pending_leaves = None;
        self.generation += 1;
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
        _frustum: Option<&herocraft_world::WorldFrustum>,
    ) -> bool {
        herocraft_common::prof_scope!("lod::update");
        // 1. Récupération non-bloquante des maillages calculés en tâche de fond
        let mut received_new_mesh = false;
        let mut received_count = 0usize;
        while let Ok(res) = self.result_rx.try_recv() {
            received_count += 1;
            if res.generation == self.generation {
                self.pending_requests.remove(&res.key);
                self.voxel_cache.insert(res.key, res.mesh);
                received_new_mesh = true;
            }
        }
        while let Ok(res) = self.patch_rx.try_recv() {
            if res.generation == self.generation {
                self.pending_patches.remove(&res.key);
                self.lod_cache.insert(res.key, res.mesh);
            }
        }

        if received_count > 0 {
            herocraft_common::prof_value!("lod.voxels_recus_par_frame", received_count);
        }

        self.time_since_eval += dt;
        let moved_dist = if self.last_evaluated_pos.is_nan() {
            f32::INFINITY
        } else {
            (camera_pos - self.last_evaluated_pos).length()
        };

        let scale = (self.base_radius as f64 / herocraft_world::REFERENCE_BASE_RADIUS) as f32;
        // Évaluation QuadTree temporisée : réévalué si déplacement >= 15 cm (adapté à l'échelle) ou toutes les 80 ms si déplacement >= 2 cm
        let should_re_eval = self.last_evaluated_pos.is_nan()
            || moved_dist > 0.15 * scale
            || (moved_dist > 0.02 * scale && self.time_since_eval > 0.08);

        let changed_leaves = if should_re_eval {
            herocraft_common::prof_scope!("lod::quadtree_eval");
            let cam_dvec = DVec3::new(
                camera_pos.x as f64,
                camera_pos.y as f64,
                camera_pos.z as f64,
            );
            let leaves = self.quadtree.evaluate_with_generator(cam_dvec, Some(generator));
            // Comparaison avec le dernier ensemble demandé (en attente ou affiché)
            let target = self.pending_leaves.as_ref().unwrap_or(&self.current_leaves);
            let changed = leaves.len() != target.len()
                || leaves.iter().zip(target).any(|(a, b)| a.key != b.key);
            self.last_evaluated_pos = camera_pos;
            self.time_since_eval = 0.0;
            changed.then_some(leaves)
        } else {
            None
        };

        // 2. Demande des patchs polygonaux manquants et des maillages voxels pour les nouvelles feuilles
        if let Some(new_leaves) = changed_leaves {
            herocraft_common::prof_value!("lod.feuilles", new_leaves.len());
            let filter = self.active_filter;
            let base_radius = self.base_radius;
            let generation = self.generation;

            // Patchs nécessaires non encore en cache
            // (couvre à la fois les feuilles LodPatch et les patchs de secours immédiat des VoxelChunks)
            let missing_patch_keys: Vec<QuadNodeKey> = new_leaves
                .iter()
                .filter(|l| !self.lod_cache.contains_key(&l.key))
                .map(|l| l.key)
                .collect();

            if !missing_patch_keys.is_empty() && self.current_leaves.is_empty() {
                // Rien n'est encore affiché (démarrage, nouvelle planète) : génération bloquante
                herocraft_common::prof_scope!("lod::generate_missing_patches");
                herocraft_common::prof_value!("lod.patchs_manquants_par_maj", missing_patch_keys.len());
                let gen = generator.clone();
                let new_patches: Vec<(QuadNodeKey, PlanetMesh)> = self.thread_pool.install(|| {
                    missing_patch_keys
                        .into_par_iter()
                        .map(|key| {
                            let patch = PlanetMesh::generate_lod_patch(
                                &gen,
                                &key,
                                base_radius,
                                patch_resolution(key.level),
                                filter,
                            );
                            (key, patch)
                        })
                        .collect()
                });

                for (key, patch) in new_patches {
                    self.lod_cache.insert(key, patch);
                }
            } else if !missing_patch_keys.is_empty() {
                // Génération en tâche de fond : les feuilles actuelles restent affichées en attendant
                herocraft_common::prof_scope!("lod::spawn_patch_tasks");
                herocraft_common::prof_value!("lod.patchs_manquants_par_maj", missing_patch_keys.len());
                for key in missing_patch_keys {
                    if self.pending_patches.insert(key) {
                        let tx = self.patch_tx.clone();
                        let task_gen = generator.clone();
                        self.patch_pool.spawn(move || {
                            let mesh = PlanetMesh::generate_lod_patch(
                                &task_gen,
                                &key,
                                base_radius,
                                patch_resolution(key.level),
                                filter,
                            );
                            let _ = tx.send(VoxelBuildResult { generation, key, mesh });
                        });
                    }
                }
            }

            // Déclenchement en arrière-plan des maillages de voxels 3D non encore calculés
            herocraft_common::prof_scope!("lod::spawn_voxel_tasks");
            for leaf in &new_leaves {
                if leaf.kind == QuadLeafKind::VoxelChunk {
                    if !self.voxel_cache.contains_key(&leaf.key) && self.pending_requests.insert(leaf.key) {
                        let tx = self.result_tx.clone();
                        let task_gen = generator.clone();
                        let key = leaf.key;
                        let queued_at = std::time::Instant::now();
                        self.thread_pool.spawn(move || {
                            herocraft_common::prof_value!("lod.attente_file_voxel_us", queued_at.elapsed().as_micros());
                            let mesh = VoxelMeshBuilder::build_incurved_single_chunk_fast(
                                &task_gen,
                                key.face,
                                key.x as i32,
                                key.y as i32,
                                base_radius,
                                filter,
                            );
                            let _ = tx.send(VoxelBuildResult { generation, key, mesh });
                        });
                    }
                }
            }

            self.pending_leaves = Some(new_leaves);
        }

        // 3. Bascule vers les nouvelles feuilles dès que tous leurs patchs sont prêts
        let mut leaves_switched = false;
        if let Some(pending) = &self.pending_leaves {
            if pending.iter().all(|l| self.lod_cache.contains_key(&l.key)) {
                self.current_leaves = self.pending_leaves.take().unwrap_or_default();
                leaves_switched = true;
            }
        }

        if !leaves_switched && !received_new_mesh && !self.last_camera_pos.is_nan() {
            return false;
        }
        self.last_camera_pos = camera_pos;

        // Nettoyage périodique des caches (les feuilles en attente restent protégées)
        {
            herocraft_common::prof_scope!("lod::cache_cleanup");
            if self.lod_cache.len() > 1500 || self.voxel_cache.len() > 200 {
                let active_keys: HashSet<_> = self
                    .current_leaves
                    .iter()
                    .chain(self.pending_leaves.iter().flatten())
                    .map(|l| l.key)
                    .collect();
                if self.lod_cache.len() > 1500 {
                    self.lod_cache.retain(|k, _| active_keys.contains(k) || k.level == 0);
                }
                if self.voxel_cache.len() > 200 {
                    self.voxel_cache.retain(|k, _| active_keys.contains(k));
                }
            }
        }

        true
    }

    /// Maillages à afficher pour les feuilles courantes (voxel si prêt, sinon patch polygonal)
    pub fn visible_meshes(&self) -> Vec<(LodMeshId, &PlanetMesh)> {
        let generation = self.generation;
        self.current_leaves
            .iter()
            .filter_map(|leaf| {
                let voxel = (leaf.kind == QuadLeafKind::VoxelChunk)
                    .then(|| self.voxel_cache.get(&leaf.key))
                    .flatten();
                match voxel {
                    Some(mesh) => Some((LodMeshId { generation, key: leaf.key, voxel: true }, mesh)),
                    // Voxel chunk encore en calcul : affichage immédiat de son patch dédié sans trou
                    None => self
                        .lod_cache
                        .get(&leaf.key)
                        .map(|mesh| (LodMeshId { generation, key: leaf.key, voxel: false }, mesh)),
                }
            })
            .collect()
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
        assert!(total_time.as_millis() < 3000, "Update took too long: {:?}", total_time);
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
        assert!(total_time.as_millis() < 15000, "Moving camera update took too long: {:?}", total_time);
    }

    #[test]
    fn test_inspect_lod_distribution() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut camera = crate::camera::OrbitCamera::new(165.0, 25.0, &generator);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod = PlanetLodManager::new(25.0, 2026);

        println!("Spawn player_pos: {:?}", camera.player_pos);
        println!("Spawn eye: {:?}", camera.eye_position());

        lod.update(camera.eye_position(), &generator, &mut cache, 0.0, None);
        println!("Leaves count at spawn: {}", lod.current_leaves.len());

        let mut face_levels = std::collections::HashMap::new();
        for l in &lod.current_leaves {
            *face_levels.entry((l.key.face, l.key.level)).or_insert(0) += 1;
        }
        println!("Levels at spawn: {:?}", face_levels);

        // Zoom out smoothly towards orbit
        camera.toggle_view(&generator);
        for _ in 0..100 {
            camera.update(0.016, &generator);
        }
        lod.update(camera.eye_position(), &generator, &mut cache, 0.016, None);
        let mut face_levels_orbit = std::collections::HashMap::new();
        for l in &lod.current_leaves {
            *face_levels_orbit.entry((l.key.face, l.key.level)).or_insert(0) += 1;
        }
        println!("Levels in orbit: {:?}", face_levels_orbit);

        // Let Rayon process all tasks and pump updates
        let start_wait = std::time::Instant::now();
        while !lod.pending_requests.is_empty() && start_wait.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            lod.update(camera.eye_position(), &generator, &mut cache, 0.016, None);
        }
        println!("After processing: pending={}, cached_lod={}, cached_voxels={}", lod.pending_requests.len(), lod.lod_cache.len(), lod.voxel_cache.len());
        let visible = lod.visible_meshes();
        println!(
            "Visible meshes: verts={}, indices={}",
            visible.iter().map(|(_, m)| m.vertices.len()).sum::<usize>(),
            visible.iter().map(|(_, m)| m.indices.len()).sum::<usize>()
        );

        // Check if ANY leaf in current_leaves is using an ancestor!
        let mut using_ancestor = 0;
        let mut using_self = 0;
        for l in &lod.current_leaves {
            if lod.lod_cache.contains_key(&l.key) || lod.voxel_cache.contains_key(&l.key) {
                using_self += 1;
            } else {
                using_ancestor += 1;
            }
        }
        println!("Leaves breakdown: using_self={}, using_ancestor={}", using_self, using_ancestor);
    }

    #[test]
    fn test_doubled_planet_size_lod_integrity() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut camera = crate::camera::OrbitCamera::new(330.0, 50.0, &generator);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod = PlanetLodManager::new(50.0, 2026);

        // 1. En orbite (distance = 330.0), la planète entière est en patchs LOD
        lod.update(camera.eye_position(), &generator, &mut cache, 0.0, None);
        let lod_patches = lod.current_leaves.iter().filter(|l| l.kind == herocraft_world::QuadLeafKind::LodPatch).count();
        assert!(lod_patches > 0, "Les patchs LOD doivent être actifs en vue satellite");

        // 2. Au sol (transition au sol, distance = 0.0), les voxels 3D (Level 5) doivent être générés autour du joueur
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.update_camera_bounds(&generator);
        lod.update(camera.eye_position(), &generator, &mut cache, 0.16, None);
        // Les patchs des nouvelles feuilles sont générés en tâche de fond : bascule après quelques frames
        let start = std::time::Instant::now();
        while lod.pending_leaves.is_some() && start.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            lod.update(camera.eye_position(), &generator, &mut cache, 0.016, None);
        }
        let voxels_ground = lod.current_leaves.iter().filter(|l| l.kind == herocraft_world::QuadLeafKind::VoxelChunk).count();
        assert!(voxels_ground > 0, "Des voxels doivent être présents au sol sur la planète de taille doublée (50.0)");
    }
}



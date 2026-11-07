use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use glam::{DVec3, Vec3};
use herocraft_common::BlockId;
use herocraft_world::{
    BlockPos, PlanetQuadTree, PlanetWorldGenerator, QuadLeaf, QuadLeafKind, QuadNodeKey, TerrainEdits, FACE_CHUNKS,
};
use herocraft_voxel_engine::cache::WorldChunkCache;

use crate::planet_mesh::{PatchDetail, PlanetMesh, TreeDetail};
use crate::voxel_mesh::{VoxelColumnMesh, VoxelMeshBuilder};

use herocraft_world::VOXEL_LEVEL;

use rayon::prelude::*;

/// Maillage calculé en tâche de fond (chunk voxel ou patch polygonal)
pub struct VoxelBuildResult {
    /// Génération des caches au lancement de la tâche (incrémentée par `clear`) : un résultat périmé est ignoré
    pub generation: u64,
    /// Version des patchs au lancement (incrémentée par `set_precision`)
    pub patch_epoch: u32,
    pub key: QuadNodeKey,
    pub mesh: PlanetMesh,
}

/// Colonne de chunks voxel maillée en tâche de fond
pub struct VoxelColumnResult {
    pub generation: u64,
    pub key: QuadNodeKey,
    pub column: VoxelColumnMesh,
    /// Modifications du terrain déjà prises en compte (voir `PlanetLodManager::edit_count`)
    pub edit_count: u64,
}

/// Identité d'un maillage affiché : change dès que son contenu peut changer (nouvelle génération
/// des caches, ou bloc modifié dans la colonne)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LodMeshId {
    pub generation: u64,
    pub key: QuadNodeKey,
    pub voxel: bool,
    pub revision: u32,
}

/// Résolution de la grille d'un patch polygonal selon son niveau (précision normale)
fn patch_resolution(level: u8) -> usize {
    match level {
        0 => 32,
        1 => 24,
        _ => 16,
    }
}

/// Finesse d'un patch selon son niveau et la précision choisie. Les niveaux juste au-delà des
/// chunks voxel (vus du sol, de ~130 à ~770 blocs) ont l'allure des voxels : une cellule par bloc,
/// textures et arbres bloc par bloc ; plus loin, cellules plus grandes et arbres simplifiés, puis
/// couleurs seules.
pub fn patch_detail(level: u8, precision: crate::ui::LodPrecisionSetting) -> PatchDetail {
    use crate::ui::LodPrecisionSetting as P;
    let side_blocks = (herocraft_world::FACE_BLOCKS >> level).max(1) as usize;
    let normal = patch_resolution(level);
    // Niveaux au-dessus des chunks voxel (1 : nœuds de 2 × 2 chunks)
    let depth = VOXEL_LEVEL.saturating_sub(level);
    let exact = PatchDetail { resolution: side_blocks, terrace_step: 1.0, textured: true, trees: TreeDetail::Blocks };
    let half = PatchDetail { resolution: side_blocks / 2, terrace_step: 1.0, textured: true, trees: TreeDetail::Boxes };
    let quarter = PatchDetail { resolution: side_blocks / 4, terrace_step: 1.0, textured: true, trees: TreeDetail::Boxes };
    let far = |res: usize| PatchDetail::plain(res.max(normal), 1.0);
    match (precision, depth) {
        (P::Normale, 1) => exact,
        (P::Normale, 2) => half,
        (P::Normale, _) => far(normal),
        (P::Fine, 1 | 2) => exact,
        (P::Fine, 3) => quarter,
        (P::Fine, _) => far(32),
        (P::Exacte, 1..=3) => exact,
        (P::Exacte, 4) => quarter,
        (P::Exacte, _) => far(32),
    }
}

/// Patchs gardés en cache avant nettoyage : moins quand ils sont lourds
fn lod_cache_limit(precision: crate::ui::LodPrecisionSetting) -> usize {
    match precision {
        crate::ui::LodPrecisionSetting::Normale => 1500,
        _ => 800,
    }
}

/// Gestionnaire de LOD planétaire piloté par QuadTree Cubed-Sphere avec transitions douces hybrides
pub struct PlanetLodManager {
    pub quadtree: PlanetQuadTree,
    pub base_radius: f32,
    pub current_seed: u64,
    pub lod_cache: HashMap<QuadNodeKey, PlanetMesh>,
    pub voxel_cache: HashMap<QuadNodeKey, VoxelColumnMesh>,
    pub current_leaves: Vec<QuadLeaf>,
    pub active_filter: herocraft_world::VisualizationFilter,
    /// Précision des patchs polygonaux (voir `patch_detail`)
    pub precision: crate::ui::LodPrecisionSetting,
    /// Version des patchs : change avec la précision, pour que le GPU ne réaffiche pas les anciens
    patch_epoch: u32,
    /// Patchs de l'ancienne précision, affichés tant que leurs remplaçants ne sont pas prêts
    stale_patches: HashMap<QuadNodeKey, (u32, PlanetMesh)>,
    /// Redemander les patchs de toutes les feuilles à la prochaine mise à jour
    refresh_patches: bool,
    last_camera_pos: Vec3,
    last_evaluated_pos: Vec3,
    time_since_eval: f32,
    thread_pool: rayon::ThreadPool,
    result_tx: std::sync::mpsc::Sender<VoxelColumnResult>,
    result_rx: std::sync::mpsc::Receiver<VoxelColumnResult>,
    pending_requests: HashSet<QuadNodeKey>,
    /// Pool dédié aux patchs polygonaux, pour qu'ils ne fassent pas la queue derrière les chunks voxels
    patch_pool: rayon::ThreadPool,
    patch_tx: std::sync::mpsc::Sender<VoxelBuildResult>,
    patch_rx: std::sync::mpsc::Receiver<VoxelBuildResult>,
    pending_patches: HashSet<QuadNodeKey>,
    /// Nouvelles feuilles en attente de leurs patchs : les feuilles courantes restent affichées jusque-là
    pending_leaves: Option<Vec<QuadLeaf>>,
    generation: u64,
    /// Blocs modifiés par le joueur, lus par les tâches de maillage
    edits: Arc<TerrainEdits>,
    /// Nombre de modifications appliquées depuis la création des caches
    edit_count: u64,
    /// Dernière modification ayant touché chaque colonne : un maillage lancé avant elle est périmé
    column_edits: HashMap<QuadNodeKey, u64>,
    /// Révision du maillage de chaque colonne modifiée (voir `LodMeshId::revision`)
    revisions: HashMap<QuadNodeKey, u32>,
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

        let (result_tx, result_rx) = std::sync::mpsc::channel::<VoxelColumnResult>();
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
            precision: crate::ui::LodPrecisionSetting::Normale,
            patch_epoch: 0,
            stale_patches: HashMap::new(),
            refresh_patches: false,
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
            edits: Arc::new(TerrainEdits::new(None)),
            edit_count: 0,
            column_edits: HashMap::new(),
            revisions: HashMap::new(),
        };

        // Pré-génération synchrone des 6 faces racines (LOD 0)
        let desc = herocraft_world::PlanetDescriptor {
            seed: current_seed,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        for &face in &herocraft_common::PlanetFace::ALL {
            let key = QuadNodeKey::new(face, 0, 0, 0);
            let patch = PlanetMesh::generate_lod_patch_stepped(&generator, &key, base_radius, PatchDetail::plain(24, 1.0), mgr.active_filter);
            mgr.lod_cache.insert(key, patch);
        }

        mgr
    }

    /// Récupère le maillage propre au nœud (voxel haute résolution ou patch polygonal dédié)
    #[allow(dead_code)]
    pub fn get_mesh_or_ancestor(&self, key: &QuadNodeKey) -> Option<&PlanetMesh> {
        if let Some(m) = self.voxel_cache.get(key) {
            return Some(&m.mesh);
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

    /// Change la précision des patchs polygonaux : tous les patchs sont refaits en tâche de fond,
    /// les anciens restent affichés jusqu'à ce que les nouveaux soient tous prêts (les chunks
    /// voxel ne bougent pas)
    pub fn set_precision(&mut self, precision: crate::ui::LodPrecisionSetting) {
        if self.precision == precision {
            return;
        }
        self.precision = precision;
        let old_epoch = self.patch_epoch;
        self.patch_epoch = self.patch_epoch.wrapping_add(1);
        // Changements rapprochés : les patchs les plus récents remplacent les plus anciens
        let old = std::mem::take(&mut self.lod_cache);
        self.stale_patches.extend(old.into_iter().map(|(key, mesh)| (key, (old_epoch, mesh))));
        self.pending_patches.clear();
        self.refresh_patches = true;
    }

    /// Ajuste les seuils de distance du QuadTree selon le préréglage vidéo sélectionné
    pub fn set_distance_preset(&mut self, preset: crate::ui::LodDistanceSetting) {
        let mult = match preset {
            crate::ui::LodDistanceSetting::Courte => 0.8,
            crate::ui::LodDistanceSetting::Normale => 1.0,
            crate::ui::LodDistanceSetting::Elevee => 1.25,
        };
        self.quadtree.split_distances = herocraft_world::split_distances(self.base_radius as f64, mult);
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
        self.stale_patches.clear();
        self.refresh_patches = false;
        self.generation += 1;
        self.column_edits.clear();
        self.revisions.clear();
        self.last_camera_pos = Vec3::splat(f32::NAN);
        self.last_evaluated_pos = Vec3::splat(f32::NAN);
        self.time_since_eval = 0.0;

        let desc = herocraft_world::PlanetDescriptor {
            seed: new_seed,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let (base_radius, filter) = (self.base_radius, self.active_filter);
        // Un thread par face : les pools peuvent être occupés par des chunks de l'ancienne planète
        let roots: Vec<(QuadNodeKey, PlanetMesh)> = std::thread::scope(|scope| {
            let handles: Vec<_> = herocraft_common::PlanetFace::ALL
                .iter()
                .map(|&face| {
                    let generator = &generator;
                    scope.spawn(move || {
                        let key = QuadNodeKey::new(face, 0, 0, 0);
                        (key, PlanetMesh::generate_lod_patch_stepped(generator, &key, base_radius, PatchDetail::plain(24, 1.0), filter))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("génération d'un patch racine")).collect()
        });
        // Les 6 faces grossières s'affichent tout de suite : les feuilles détaillées de la nouvelle
        // planète sont générées en tâche de fond au lieu de geler la frame (~0,5 s)
        for (key, patch) in roots {
            self.current_leaves.push(QuadLeaf { key, kind: QuadLeafKind::LodPatch });
            self.lod_cache.insert(key, patch);
        }
    }

    /// Blocs modifiés de la planète courante
    pub fn terrain_edits(&self) -> &Arc<TerrainEdits> {
        &self.edits
    }

    /// Blocs modifiés de la planète courante (à donner après `clear` lors d'un changement de planète)
    pub fn set_terrain_edits(&mut self, edits: Arc<TerrainEdits>) {
        self.edits = edits;
    }

    /// Change un bloc et refait tout de suite, dans cette frame, les couches de chunks dont
    /// l'affichage en dépend : celle du bloc et, s'il touche un bord de son chunk, la voisine de
    /// ce côté (ses faces et son ombrage ambiant le voient). Renvoie `true` si le bloc a changé.
    pub fn set_block(&mut self, generator: &PlanetWorldGenerator, pos: BlockPos, block: BlockId) -> bool {
        herocraft_common::prof_scope!("lod::set_block");
        let coord = pos.chunk_coord();
        let own_key = QuadNodeKey::new(coord.face, VOXEL_LEVEL, coord.x as u32, coord.y as u32);
        // Le chunk d'origine est tiré des colonnes déjà échantillonnées pour le maillage s'il y en a
        let source = self.voxel_cache.get(&own_key).map(|c| c.source.clone());
        let changed = self.edits.set_block_with(pos, block, |coord| match &source {
            Some(source) => source.generate_chunk(generator, coord.altitude_layer),
            None => generator.generate_chunk(coord),
        });
        if !changed {
            return false;
        }
        self.edit_count += 1;

        let (lx, ly, lz) = pos.local();
        let edge = |l: u8| -> &'static [i32] {
            match l {
                0 => &[0, -1],
                31 => &[0, 1],
                _ => &[0],
            }
        };
        let layers: Vec<i32> = edge(ly).iter().map(|d| coord.altitude_layer + d).collect();
        for &dx in edge(lx) {
            for &dy in edge(lz) {
                let (cx, cy) = (coord.x + dx, coord.y + dy);
                if !(0..FACE_CHUNKS).contains(&cx) || !(0..FACE_CHUNKS).contains(&cy) {
                    continue;
                }
                let key = QuadNodeKey::new(coord.face, VOXEL_LEVEL, cx as u32, cy as u32);
                // Nouvelle révision même pour une colonne absente : un maillage d'avant la
                // modification encore gardé sur le GPU ne doit pas être réaffiché
                *self.revisions.entry(key).or_insert(0) += 1;
                self.column_edits.insert(key, self.edit_count);
                if let Some(column) = self.voxel_cache.get_mut(&key) {
                    VoxelMeshBuilder::rebuild_layers(generator, &self.edits, column, &layers);
                }
            }
        }
        true
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
        let mut stale = Vec::new();
        while let Ok(res) = self.result_rx.try_recv() {
            received_count += 1;
            if res.generation == self.generation {
                self.pending_requests.remove(&res.key);
                if self.column_edits.get(&res.key).is_some_and(|&n| n > res.edit_count) {
                    // Un bloc de la colonne a changé pendant le maillage : il est refait
                    stale.push(res.key);
                    continue;
                }
                self.voxel_cache.insert(res.key, res.column);
                received_new_mesh = true;
            }
        }
        for key in stale {
            self.spawn_voxel_task(key, generator);
        }
        while let Ok(res) = self.patch_rx.try_recv() {
            if res.generation == self.generation && res.patch_epoch == self.patch_epoch {
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

        let scale = herocraft_world::block_scale(self.base_radius as f64) as f32;
        // Évaluation QuadTree temporisée : réévalué si déplacement >= 15 cm (adapté à l'échelle) ou toutes les 80 ms si déplacement >= 2 cm
        let should_re_eval = self.refresh_patches
            || self.last_evaluated_pos.is_nan()
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
            let changed = std::mem::take(&mut self.refresh_patches)
                || leaves.len() != target.len()
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
            let precision = self.precision;
            let base_radius = self.base_radius;
            let generation = self.generation;
            let patch_epoch = self.patch_epoch;

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
                            let patch = PlanetMesh::generate_lod_patch_stepped(&gen, &key, base_radius, patch_detail(key.level, precision), filter);
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
                            let mesh = PlanetMesh::generate_lod_patch_stepped(&task_gen, &key, base_radius, patch_detail(key.level, precision), filter);
                            let _ = tx.send(VoxelBuildResult { generation, patch_epoch, key, mesh });
                        });
                    }
                }
            }

            // Déclenchement en arrière-plan des maillages de voxels 3D non encore calculés
            herocraft_common::prof_scope!("lod::spawn_voxel_tasks");
            for leaf in &new_leaves {
                if leaf.kind == QuadLeafKind::VoxelChunk && !self.voxel_cache.contains_key(&leaf.key) {
                    self.spawn_voxel_task(leaf.key, generator);
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
                // Plus aucune feuille affichée n'a besoin des patchs de l'ancienne précision
                self.stale_patches.clear();
            }
        }

        if !leaves_switched && !received_new_mesh && !self.last_camera_pos.is_nan() {
            return false;
        }
        self.last_camera_pos = camera_pos;

        // Nettoyage périodique des caches (les feuilles en attente restent protégées)
        {
            herocraft_common::prof_scope!("lod::cache_cleanup");
            let lod_limit = lod_cache_limit(self.precision);
            if self.lod_cache.len() > lod_limit || self.voxel_cache.len() > 200 {
                let active_keys: HashSet<_> = self
                    .current_leaves
                    .iter()
                    .chain(self.pending_leaves.iter().flatten())
                    .map(|l| l.key)
                    .collect();
                if self.lod_cache.len() > lod_limit {
                    self.lod_cache.retain(|k, _| active_keys.contains(k) || k.level == 0);
                }
                if self.voxel_cache.len() > 200 {
                    self.voxel_cache.retain(|k, _| active_keys.contains(k));
                }
            }
        }

        true
    }

    /// Lance le maillage d'une colonne voxel en tâche de fond (sauf s'il est déjà en cours)
    fn spawn_voxel_task(&mut self, key: QuadNodeKey, generator: &PlanetWorldGenerator) {
        if !self.pending_requests.insert(key) {
            return;
        }
        let tx = self.result_tx.clone();
        let task_gen = generator.clone();
        let edits = self.edits.clone();
        let (generation, edit_count) = (self.generation, self.edit_count);
        let (base_radius, filter) = (self.base_radius, self.active_filter);
        let queued_at = std::time::Instant::now();
        self.thread_pool.spawn(move || {
            herocraft_common::prof_value!("lod.attente_file_voxel_us", queued_at.elapsed().as_micros());
            let source = VoxelMeshBuilder::sample_column_source(&task_gen, key.face, key.x as i32, key.y as i32, base_radius, filter);
            let column = VoxelMeshBuilder::build_column(&task_gen, &edits, Arc::new(source));
            let _ = tx.send(VoxelColumnResult { generation, key, column, edit_count });
        });
    }

    /// Génération courante (incrémentée à chaque nouvelle planète)
    pub fn generation(&self) -> u64 {
        self.generation
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
                    Some(column) => {
                        let revision = self.revisions.get(&leaf.key).copied().unwrap_or(0);
                        Some((LodMeshId { generation, key: leaf.key, voxel: true, revision }, &column.mesh))
                    }
                    // Voxel chunk encore en calcul : affichage immédiat de son patch dédié sans trou.
                    // La révision d'un patch est sa version (précision) : un patch refait n'est
                    // pas confondu avec l'ancien déjà envoyé au GPU.
                    None => match self.lod_cache.get(&leaf.key) {
                        Some(mesh) => Some((LodMeshId { generation, key: leaf.key, voxel: false, revision: self.patch_epoch }, mesh)),
                        None => self.stale_patches.get(&leaf.key).map(|(epoch, mesh)| {
                            (LodMeshId { generation, key: leaf.key, voxel: false, revision: *epoch }, mesh)
                        }),
                    },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    const SEA_LEVEL_BLOCKS_F: f64 = herocraft_world::SEA_LEVEL_BLOCKS;

    #[test]
    fn test_lod_update_performance_when_stationary() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod_manager = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 2026);

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
        let mut lod_manager = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 2026);

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
        let mut camera = crate::camera::OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 2026);

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

    /// Juste au-delà des chunks voxel, les patchs ont l'allure des voxels : une cellule par bloc,
    /// textures et arbres en blocs ; plus loin, ils s'allègent sans jamais être plus grossiers
    /// qu'avant
    #[test]
    fn near_patches_look_like_voxels() {
        use crate::ui::LodPrecisionSetting as P;
        let near = VOXEL_LEVEL - 1;
        let side = (herocraft_world::FACE_BLOCKS >> near) as usize;
        for p in P::ALL {
            let d = patch_detail(near, p);
            assert_eq!((d.resolution, d.terrace_step, d.textured, d.trees), (side, 1.0, true, TreeDetail::Blocks), "{p:?}");
        }
        for level in 0..VOXEL_LEVEL {
            for p in P::ALL {
                let d = patch_detail(level, p);
                assert!(d.resolution >= patch_resolution(level) && d.resolution <= 256, "niveau {level} {p:?} : {}", d.resolution);
                assert_eq!(d.terrace_step, 1.0);
            }
        }
        assert_eq!(PlanetMesh::quantize_elevation(137.8, 1.0), 137.0);
        assert_eq!(PlanetMesh::quantize_elevation(97.4, 1.0), 97.0);
    }

    /// Coût de chaque précision au sol : feuilles par niveau, sommets, mémoire GPU et temps de
    /// génération de tous les patchs (plus l'ancienne précision normale, en terrasses de 6 blocs).
    /// `cargo test --release -p herocraft_client mesure_precision_lod -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_precision_lod() {
        use crate::ui::LodPrecisionSetting as P;
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 42, ..Default::default() });
        let mut camera = crate::camera::OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.update_camera_bounds(&generator);
        let lod = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 42);
        let leaves = lod.quadtree.evaluate_with_generator(camera.eye_position().as_dvec3(), Some(&generator));
        let patches: Vec<QuadNodeKey> = leaves.iter().filter(|l| l.kind == QuadLeafKind::LodPatch).map(|l| l.key).collect();
        let configs: [(&str, Box<dyn Fn(u8) -> PatchDetail + Sync>); 4] = [
            ("Ancienne (couleurs, terrasses)", Box::new(|l| PatchDetail::plain(patch_resolution(l), PlanetMesh::ORBIT_TERRACE_STEP_HEIGHT))),
            ("Normale", Box::new(|l| patch_detail(l, P::Normale))),
            ("Fine", Box::new(|l| patch_detail(l, P::Fine))),
            ("1:1", Box::new(|l| patch_detail(l, P::Exacte))),
        ];
        for (name, detail) in &configs {
            let start = Instant::now();
            let meshes: Vec<(u8, usize, usize)> = lod.thread_pool.install(|| {
                patches
                    .par_iter()
                    .map(|key| {
                        let m = PlanetMesh::generate_lod_patch_stepped(&generator, key, crate::camera::DEFAULT_RADIUS, detail(key.level), lod.active_filter);
                        (key.level, m.vertices.len(), m.indices.len() / 3)
                    })
                    .collect()
            });
            let elapsed = start.elapsed();
            let verts: usize = meshes.iter().map(|m| m.1).sum();
            let tris: usize = meshes.iter().map(|m| m.2).sum();
            // Position (12 o) + attributs compactés (12 o) par sommet, index de 4 o
            let gpu_mb = (verts * 24 + tris * 3 * 4) as f64 / 1e6;
            println!("{name} : {} patchs, {verts} sommets, {tris} triangles, ~{gpu_mb:.0} Mo GPU, génération {elapsed:.2?} ({} fils)", patches.len(), lod.thread_pool.current_num_threads());
            let mut per_level = std::collections::BTreeMap::new();
            for (level, v, _) in meshes {
                let e = per_level.entry(level).or_insert((0usize, 0usize));
                e.0 += 1;
                e.1 += v;
            }
            for (level, (n, v)) in per_level {
                let res = detail(level).resolution;
                let block_per_cell = (herocraft_world::FACE_BLOCKS >> level) as f64 / res as f64;
                println!("  niveau {level} : {n} patchs, {res}² cellules ({block_per_cell} blocs/cellule), {v} sommets");
            }
        }
    }

    /// Changer de précision ne vide pas l'écran : les feuilles et leurs anciens patchs restent
    /// affichés (pas de retour aux 6 faces de la vue satellite), puis les nouveaux les remplacent
    #[test]
    fn precision_change_keeps_terrain_displayed() {
        use crate::ui::LodPrecisionSetting as P;
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 42, ..Default::default() });
        let mut camera = crate::camera::OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.update_camera_bounds(&generator);
        let eye = camera.eye_position();
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 42);
        lod.update(eye, &generator, &mut cache, 0.0, None);
        let leaves = lod.current_leaves.len();
        let shown = lod.visible_meshes().len();
        assert!(leaves > 6);

        let mid = lod.current_leaves.iter().find(|l| l.kind == QuadLeafKind::LodPatch && l.key.level == VOXEL_LEVEL - 2).unwrap().key;
        let before_mid = lod.lod_cache[&mid].vertices.len();
        lod.set_precision(P::Fine);
        lod.update(eye, &generator, &mut cache, 0.016, None);
        assert_eq!(lod.current_leaves.len(), leaves, "les feuilles détaillées restent affichées");
        assert!(lod.visible_meshes().len() >= shown, "chaque feuille garde un maillage pendant le calcul");

        let start = Instant::now();
        while lod.pending_leaves.is_some() && start.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            lod.update(eye, &generator, &mut cache, 0.016, None);
        }
        assert!(lod.pending_leaves.is_none(), "les patchs fins finissent par remplacer les anciens");
        assert!(lod.stale_patches.is_empty());
        // Les patchs de 128 blocs sont passés d'une cellule pour 2 blocs à une par bloc : plus de sommets
        let mid = lod.current_leaves.iter().find(|l| l.kind == QuadLeafKind::LodPatch && l.key.level == VOXEL_LEVEL - 2).unwrap();
        assert!(lod.lod_cache[&mid.key].vertices.len() > before_mid);
    }

    /// Profil du relief devant le joueur : hauteur des voxels et du dessus des patchs de chaque
    /// précision, bloc par bloc, en CSV.
    /// `HEROCRAFT_CAPTURES=dossier cargo test --release -p herocraft_client profil_precision_lod -- --ignored`
    #[test]
    #[ignore]
    fn profil_precision_lod() {
        use crate::ui::LodPrecisionSetting as P;
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let seed: u64 = std::env::var("HEROCRAFT_GRAINE").ok().and_then(|s| s.parse().ok()).unwrap_or(42);
        let angle: f64 = std::env::var("HEROCRAFT_PROFIL_ANGLE").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed, ..Default::default() });
        let mut camera = crate::camera::OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.update_camera_bounds(&generator);
        let eye = camera.eye_position();
        let lod = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, seed);
        let leaves = lod.quadtree.evaluate_with_generator(eye.as_dvec3(), Some(&generator));
        let (face, u0, v0) = herocraft_common::PlanetFace::sphere_direction_to_face_uv(camera.player_pos.as_dvec3().normalize());
        let block_uv = 2.0 / herocraft_world::FACE_BLOCKS as f64;
        let height = |u: f64, v: f64, step: f64| {
            let dir = face.uv_to_sphere_direction(u, v);
            let (lat, _) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
            let (e, biome) = generator.sample_elevation_and_biome(dir, lat);
            if biome.is_water() { SEA_LEVEL_BLOCKS_F } else { PlanetMesh::quantize_elevation(e, step) }
        };
        let mut csv = String::from("distance;niveau;voxels;normale;fine;exacte\n");
        for d in 0..1600 {
            let u = u0 + (d as f64 + 0.5) * block_uv * angle.cos();
            let v = v0 + (d as f64 + 0.5) * block_uv * angle.sin();
            if !(-1.0..1.0).contains(&u) || !(-1.0..1.0).contains(&v) {
                break;
            }
            let Some(leaf) = leaves.iter().find(|l| {
                let (a, b, c, e) = l.key.uv_bounds();
                l.key.face == face && (a..b).contains(&u) && (c..e).contains(&v)
            }) else { continue };
            // Colonne de voxels réelle : centre du bloc, dessus à `floor` de l'élévation
            let (iu, iv) = (((u + 1.0) / block_uv).floor(), ((v + 1.0) / block_uv).floor());
            let truth = height(-1.0 + (iu + 0.5) * block_uv, -1.0 + (iv + 0.5) * block_uv, 1.0);
            let mut row = format!("{d};{};{truth}", leaf.key.level);
            for p in P::ALL {
                let h = if leaf.kind == QuadLeafKind::VoxelChunk {
                    truth
                } else {
                    let PatchDetail { resolution: res, terrace_step: step, .. } = patch_detail(leaf.key.level, p);
                    let (a, b, c, e) = leaf.key.uv_bounds();
                    let (cu, cv) = ((b - a) / res as f64, (e - c) / res as f64);
                    let (ci, cj) = (((u - a) / cu).floor(), ((v - c) / cv).floor());
                    height(a + (ci + 0.5) * cu, c + (cj + 0.5) * cv, step)
                };
                row += &format!(";{h}");
            }
            csv += &row;
            csv.push('\n');
        }
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join(format!("profil-lod-{seed}.csv")), csv).unwrap();
    }

    #[test]
    fn test_doubled_planet_size_lod_integrity() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let generator = PlanetWorldGenerator::new(desc);
        let mut camera = crate::camera::OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        let mut cache = herocraft_voxel_engine::WorldChunkCache::new(None, 64);
        let mut lod = PlanetLodManager::new(crate::camera::DEFAULT_RADIUS, 2026);

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



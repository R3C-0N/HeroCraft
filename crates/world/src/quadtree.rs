use std::collections::HashMap;
use std::sync::Mutex;

use glam::DVec3;
use herocraft_common::{PlanetChunkCoord, PlanetFace};
use serde::{Deserialize, Serialize};

use crate::cubed_sphere::unit_sphere_to_lat_lon;
use crate::generator::{PlanetWorldGenerator, SEA_LEVEL_BLOCKS};

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

    /// Nœud parent (niveau immédiatement supérieur)
    pub fn parent(&self) -> Option<QuadNodeKey> {
        if self.level == 0 {
            None
        } else {
            Some(QuadNodeKey::new(
                self.face,
                self.level - 1,
                self.x / 2,
                self.y / 2,
            ))
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

/// Plan 3D défini par son vecteur normal unitaire et sa distance à l'origine
#[derive(Debug, Clone, Copy)]
pub struct FrustumPlane {
    pub normal: DVec3,
    pub distance: f64,
}

impl FrustumPlane {
    pub fn distance_to_point(&self, p: DVec3) -> f64 {
        self.normal.dot(p) + self.distance
    }
}

/// Frustum de vue à 6 plans pour le culling spatial
#[derive(Debug, Clone, Copy)]
pub struct WorldFrustum {
    pub planes: [FrustumPlane; 6],
}

impl WorldFrustum {
    pub fn from_view_projection(vp: glam::Mat4) -> Self {
        let cols = vp.to_cols_array_2d();
        let row = |i: usize| -> [f64; 4] {
            [
                cols[0][i] as f64,
                cols[1][i] as f64,
                cols[2][i] as f64,
                cols[3][i] as f64,
            ]
        };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let make_plane = |nx: f64, ny: f64, nz: f64, d: f64| -> FrustumPlane {
            let normal = DVec3::new(nx, ny, nz);
            let len = normal.length();
            if len > 1e-6 {
                FrustumPlane {
                    normal: normal / len,
                    distance: d / len,
                }
            } else {
                FrustumPlane {
                    normal: DVec3::ZERO,
                    distance: 0.0,
                }
            }
        };

        // Méthode de Gribb-Hartmann adaptée à WGPU (profondeur [0, 1])
        let left = make_plane(r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]);
        let right = make_plane(r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]);
        let bottom = make_plane(r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]);
        let top = make_plane(r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]);
        let near = make_plane(r2[0], r2[1], r2[2], r2[3]);
        let far = make_plane(r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]);

        Self {
            planes: [left, right, bottom, top, near, far],
        }
    }

    /// Teste si une sphère englobante intersecte ou est à l'intérieur du frustum
    pub fn intersects_sphere(&self, center: DVec3, radius: f64) -> bool {
        for plane in &self.planes {
            if plane.distance_to_point(center) < -radius {
                return false;
            }
        }
        true
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
    /// Élévations déjà échantillonnées, par cellule de la grille voxel de chaque face
    elevation_cache: Mutex<ElevationCache>,
}

/// Résolution de la grille de cache d'élévation (une cellule par colonne de voxels)
const ELEVATION_CACHE_GRID: f64 = 1024.0;
const ELEVATION_CACHE_MAX_ENTRIES: usize = 1 << 18;

#[derive(Default)]
struct ElevationCache {
    seed: Option<u64>,
    values: HashMap<(PlanetFace, i32, i32), f64>,
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
        // Level 3 (zone)         : split si distance < 18.0
        // Level 4 (local)        : split si distance < 3.8 (passage aux voxels 3D Level 5 locaux au sol)
        // Échelle proportionnelle au rayon de la planète par rapport au rayon de référence
        let scale = base_radius / crate::cubed_sphere::REFERENCE_BASE_RADIUS;
        let split_distances = vec![
            95.0 * scale,
            60.0 * scale,
            38.0 * scale,
            18.0 * scale,
            3.8 * scale,
        ];

        Self {
            base_radius,
            min_level,
            max_level,
            split_distances,
            elevation_cache: Mutex::new(ElevationCache::default()),
        }
    }

    /// Élévation du relief au point (u, v) d'une face, arrondie à la colonne de voxels la plus proche.
    /// Le relief étant déterministe, chaque colonne n'est échantillonnée qu'une fois entre deux évaluations.
    fn cached_elevation(&self, gen: &PlanetWorldGenerator, face: PlanetFace, u: f64, v: f64) -> f64 {
        let iu = ((u + 1.0) * 0.5 * ELEVATION_CACHE_GRID).round() as i32;
        let iv = ((v + 1.0) * 0.5 * ELEVATION_CACHE_GRID).round() as i32;
        let mut cache = self.elevation_cache.lock().unwrap();
        if cache.seed != Some(gen.planet.seed) || cache.values.len() > ELEVATION_CACHE_MAX_ENTRIES {
            cache.seed = Some(gen.planet.seed);
            cache.values.clear();
        }
        *cache.values.entry((face, iu, iv)).or_insert_with(|| {
            let snapped_u = iu as f64 / ELEVATION_CACHE_GRID * 2.0 - 1.0;
            let snapped_v = iv as f64 / ELEVATION_CACHE_GRID * 2.0 - 1.0;
            let dir = face.uv_to_sphere_direction(snapped_u, snapped_v);
            let (lat, lon) = unit_sphere_to_lat_lon(dir);
            gen.sample_elevation(dir, lat, lon)
        })
    }

    /// Évalue l'arbre complet depuis la position de la caméra et renvoie la liste des feuilles visibles
    pub fn evaluate(&self, camera_pos: DVec3) -> Vec<QuadLeaf> {
        self.evaluate_with_generator(camera_pos, None)
    }

    /// Évalue l'arbre en tenant compte de l'élévation réelle du relief planétaire pour un niveau de détail parfait
    pub fn evaluate_with_generator(
        &self,
        camera_pos: DVec3,
        generator: Option<&PlanetWorldGenerator>,
    ) -> Vec<QuadLeaf> {
        self.evaluate_with_frustum(camera_pos, generator, None)
    }

    /// Évalue l'arbre en appliquant le Frustum Culling (cône de vue caméra) et l'élévation réelle
    pub fn evaluate_with_frustum(
        &self,
        camera_pos: DVec3,
        generator: Option<&PlanetWorldGenerator>,
        frustum: Option<&WorldFrustum>,
    ) -> Vec<QuadLeaf> {
        herocraft_common::prof_scope!("world::quadtree::evaluate");
        let mut leaves = Vec::with_capacity(256);

        // Toutes les 6 faces de la planète sont toujours initialisées
        for &face in &PlanetFace::ALL {
            let root = QuadNodeKey::new(face, 0, 0, 0);
            self.subdivide_recursive(root, camera_pos, generator, frustum, &mut leaves);
        }

        leaves
    }

    fn subdivide_recursive(
        &self,
        node: QuadNodeKey,
        camera_pos: DVec3,
        generator: Option<&PlanetWorldGenerator>,
        frustum: Option<&WorldFrustum>,
        leaves: &mut Vec<QuadLeaf>,
    ) {
        let center_dir = node.center_direction();
        let altitude_scale = self.base_radius * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0);

        let cam_r = camera_pos.length();
        let cam_dir = if cam_r > 1e-6 {
            camera_pos / cam_r
        } else {
            DVec3::Y
        };

        // Calcule le point de surface le plus proche sur le nœud par rapport à la caméra.
        // Mesurer la distance au point le plus proche (au lieu d'un centre unique) évite
        // qu'un dénivelé montagneux vertical important (jusqu'à 10 m de rayon supplémentaire)
        // n'interrompe prématurément la subdivision en niveau 5 voxels et ne crée un trou béant.
        let (u_min, u_max, v_min, v_max) = node.uv_bounds();
        let (closest_u, closest_v) = if let Some((cam_u, cam_v)) = node.face.project_to_face_uv(cam_dir) {
            (cam_u.clamp(u_min, u_max), cam_v.clamp(v_min, v_max))
        } else {
            ((u_min + u_max) * 0.5, (v_min + v_max) * 0.5)
        };
        let closest_dir = node.face.uv_to_sphere_direction(closest_u, closest_v);

        // Calcule le rayon réel de surface au point le plus proche (tenant compte du relief montagneux ou de la mer)
        let surface_radius = if let Some(gen) = generator {
            let elev = self.cached_elevation(gen, node.face, closest_u, closest_v);
            let eff_elev = elev.max(SEA_LEVEL_BLOCKS);
            self.base_radius + eff_elev * altitude_scale
        } else {
            let sea_r = self.base_radius + 100.0 * altitude_scale;
            if cam_r >= self.base_radius && cam_r <= self.base_radius + 400.0 * altitude_scale {
                cam_r
            } else {
                sea_r
            }
        };

        let closest_surface_pos = closest_dir * surface_radius;
        let dist_to_node = (camera_pos - closest_surface_pos).length();

        // 1. Culling d'horizon planétaire : si le nœud se trouve sur l'hémisphère arrière masqué
        // par la courbure de la planète, on ne le subdivise pas au-delà du niveau 1
        let is_behind_horizon = if cam_r < self.base_radius * 2.2 && node.level >= 1 {
            closest_dir.dot(cam_dir) < -0.22
        } else {
            false
        };

        // 2. Frustum Culling (Cône de vision) :
        // Rayon de la sphère englobant le nœud avec marge pour le relief montagneux maximal
        let max_mountain_r = self.base_radius + 360.0 * altitude_scale;
        let center_pos = center_dir * surface_radius;
        let bounding_radius = max_mountain_r * (1.85 / (1 << node.level) as f64) + 1.5;
        let is_outside_frustum = if let Some(frust) = frustum {
            node.level >= 1 && !frust.intersects_sphere(center_pos, bounding_radius)
        } else {
            false
        };

        let should_split = !is_behind_horizon
            && !is_outside_frustum
            && (node.level < self.min_level
                || ((node.level as usize) < self.split_distances.len()
                    && node.level < self.max_level
                    && dist_to_node < self.split_distances[node.level as usize]));

        if should_split {
            for child in node.children() {
                self.subdivide_recursive(child, camera_pos, generator, frustum, leaves);
            }
        } else {
            // Nœuds hors du champ de vision : exclus de l'affichage pour économiser le GPU
            if is_outside_frustum && node.level >= 1 {
                return;
            }
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

    #[test]
    fn test_quadtree_evaluation_real_ground_elevation() {
        let qt = PlanetQuadTree::new(25.0);
        // Altitude de la mer (100 blocs) : rayon ~28.84
        let cam_sea = DVec3::new(0.0, 0.0, 28.9);
        let leaves_sea = qt.evaluate(cam_sea);
        let voxels_sea = leaves_sea.iter().filter(|l| l.kind == QuadLeafKind::VoxelChunk).count();
        println!("Leaves at sea (r=28.9): total={}, voxels={}", leaves_sea.len(), voxels_sea);

        // Altitude d'une colline (180 blocs) : rayon ~31.90
        let cam_hill = DVec3::new(0.0, 0.0, 32.0);
        let leaves_hill = qt.evaluate(cam_hill);
        let voxels_hill = leaves_hill.iter().filter(|l| l.kind == QuadLeafKind::VoxelChunk).count();
        println!("Leaves on hill (r=32.0): total={}, voxels={}", leaves_hill.len(), voxels_hill);

        // Altitude d'une montagne (280 blocs) : rayon ~35.74
        let cam_mountain = DVec3::new(0.0, 0.0, 35.8);
        let leaves_mountain = qt.evaluate(cam_mountain);
        let voxels_mountain = leaves_mountain.iter().filter(|l| l.kind == QuadLeafKind::VoxelChunk).count();
        println!("Leaves on mountain (r=35.8): total={}, voxels={}", leaves_mountain.len(), voxels_mountain);

        assert!(voxels_sea > 0, "Les voxels doivent apparaître au niveau de la mer");
        assert!(voxels_hill > 0, "Les voxels doivent apparaître sur les collines");
        assert!(voxels_mountain > 0, "Les voxels doivent apparaître sur les montagnes");

        let desc = crate::PlanetDescriptor::default();
        let gen = PlanetWorldGenerator::new(desc);
        let dir = DVec3::new(0.0, 0.0, 1.0);
        let (lat, lon) = unit_sphere_to_lat_lon(dir);
        let elev = gen.sample_elevation(dir, lat, lon).max(SEA_LEVEL_BLOCKS);
        let altitude_scale = qt.base_radius * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0);
        let ground_r = qt.base_radius + elev * altitude_scale;
        let cam_ground = dir * (ground_r + 0.065);
        let leaves_gen = qt.evaluate_with_generator(cam_ground, Some(&gen));
        let voxels_gen = leaves_gen.iter().filter(|l| l.kind == QuadLeafKind::VoxelChunk).count();
        assert!(voxels_gen > 0, "Les voxels doivent être générés au sol avec le générateur planétaire");
    }

    #[test]
    fn test_high_mountain_subdivision_without_holes() {
        let qt = PlanetQuadTree::new(25.0);
        let desc = crate::PlanetDescriptor::default();
        let gen = PlanetWorldGenerator::new(desc);

        // Recherche d'un point culminant (haute montagne > 300 blocs)
        let mut highest_elev = 0.0;
        let mut peak_dir = DVec3::Z;

        for step_u in -10..=10 {
            for step_v in -10..=10 {
                let u = (step_u as f64) * 0.08;
                let v = (step_v as f64) * 0.08;
                let dir = PlanetFace::PosZ.uv_to_sphere_direction(u, v);
                let (lat, lon) = unit_sphere_to_lat_lon(dir);
                let e = gen.sample_elevation(dir, lat, lon);
                if e > highest_elev {
                    highest_elev = e;
                    peak_dir = dir;
                }
            }
        }

        println!("Highest found elevation on PosZ: {:.1} blocks", highest_elev);
        let altitude_scale = qt.base_radius * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0);
        let peak_r = qt.base_radius + highest_elev * altitude_scale;
        let cam_peak = peak_dir * (peak_r + 0.065);

        let leaves = qt.evaluate_with_generator(cam_peak, Some(&gen));
        let voxel_leaves: Vec<_> = leaves.iter().filter(|l| l.kind == QuadLeafKind::VoxelChunk).collect();
        assert!(!voxel_leaves.is_empty(), "Au sommet de la montagne, des VoxelChunks doivent être présents");

        // Vérifier que le chunk sous les pieds de la caméra est bien un VoxelChunk
        let (cam_face, cam_u, cam_v) = PlanetFace::sphere_direction_to_face_uv(peak_dir);
        let found_under_camera = voxel_leaves.iter().any(|l| {
            if l.key.face != cam_face {
                return false;
            }
            let (u0, u1, v0, v1) = l.key.uv_bounds();
            cam_u >= u0 && cam_u <= u1 && cam_v >= v0 && cam_v <= v1
        });
        assert!(found_under_camera, "Le chunk directement sous le joueur au sommet de la montagne doit être maillé en voxels (pas de trou)");
    }
}

//! Collisions du joueur avec les blocs : le sol sous les pieds, les murs et les plafonds sont lus
//! bloc par bloc dans le terrain (générateur + modifications), là où le maillage voxel les dessine.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use glam::{DVec3, Vec3};
use herocraft_common::PlanetFace;
use herocraft_world::generator::ColumnSample;
use herocraft_world::{BlockGeometry, PlanetWorldGenerator, TerrainEdits, TerrainQuery, WORLD_HEIGHT};

/// Au-delà, le cache de colonnes est vidé (le joueur ne touche qu'une poignée de colonnes par image)
const MAX_CACHED_COLUMNS: usize = 4096;

pub struct VoxelCollider {
    edits: Arc<TerrainEdits>,
    geometry: BlockGeometry,
    /// Colonnes du générateur déjà échantillonnées : le bruit coûte cher, le joueur reste sur place
    columns: RefCell<HashMap<(PlanetFace, i32, i32), ColumnSample>>,
}

impl VoxelCollider {
    pub fn new(edits: Arc<TerrainEdits>, base_radius: f32) -> Self {
        Self { edits, geometry: BlockGeometry::new(base_radius as f64), columns: RefCell::new(HashMap::new()) }
    }

    /// Hauteur d'un bloc en unités du monde
    pub fn block_height(&self) -> f32 {
        self.geometry.block_height as f32
    }

    fn with_query<R>(&self, generator: &PlanetWorldGenerator, f: impl FnOnce(&mut TerrainQuery) -> R) -> R {
        let columns = std::mem::take(&mut *self.columns.borrow_mut());
        let mut query = TerrainQuery::with_columns(generator, &self.edits, columns);
        let result = f(&mut query);
        let mut columns = query.into_columns();
        if columns.len() > MAX_CACHED_COLUMNS {
            columns.clear();
        }
        *self.columns.borrow_mut() = columns;
        result
    }

    fn radius_of_altitude(&self, y: i32) -> f32 {
        (self.geometry.base_radius + y as f64 * self.geometry.block_height) as f32
    }

    /// Rayon du sol sous le point `p` : dessus du plus haut bloc porteur de sa colonne qui ne dépasse
    /// pas les pieds de plus de `step` blocs (une marche franchissable).
    pub fn ground_radius(&self, generator: &PlanetWorldGenerator, p: Vec3, step: f32) -> f32 {
        let Some(pos) = self.geometry.block_at_point(p.as_dvec3()) else {
            return self.geometry.base_radius as f32;
        };
        let altitude = (p.length() as f64 - self.geometry.base_radius) / self.geometry.block_height;
        let from_y = (altitude + step as f64).floor() as i32 - 1;
        let top = self.with_query(generator, |q| q.support_top(pos.face, pos.x, pos.z, from_y));
        self.radius_of_altitude(top)
    }

    /// Rayon du dessus de la colonne entière sous une direction (plus haut bloc porteur du monde)
    pub fn column_top_radius(&self, generator: &PlanetWorldGenerator, dir: Vec3) -> f32 {
        let Some(pos) = self.geometry.block_at_point(dir.normalize().as_dvec3() * self.geometry.base_radius) else {
            return self.geometry.base_radius as f32;
        };
        let top = self.with_query(generator, |q| q.support_top(pos.face, pos.x, pos.z, WORLD_HEIGHT - 1));
        self.radius_of_altitude(top)
    }

    /// Rayon du sol pour un point libre (caméra) : s'il est dans un bloc porteur, le dessus de la pile
    /// de blocs qui le contient, sinon le dessus du premier bloc porteur en dessous.
    pub fn point_ground_radius(&self, generator: &PlanetWorldGenerator, p: Vec3) -> f32 {
        let Some(pos) = self.geometry.block_at_point(p.as_dvec3()) else {
            return self.geometry.base_radius as f32;
        };
        let top = self.with_query(generator, |q| {
            let mut y = pos.y.max(0);
            while y < WORLD_HEIGHT && q.support_top(pos.face, pos.x, pos.z, y) == y + 1 {
                y += 1;
            }
            q.support_top(pos.face, pos.x, pos.z, y)
        });
        self.radius_of_altitude(top)
    }

    /// Un des points est dans un bloc qui arrête le corps (ni air, ni eau)
    pub fn any_solid(&self, generator: &PlanetWorldGenerator, points: &[Vec3]) -> bool {
        self.with_query(generator, |q| {
            points.iter().any(|p| {
                self.geometry
                    .block_at_point(DVec3::new(p.x as f64, p.y as f64, p.z as f64))
                    .is_some_and(|pos| pos.is_valid() && q.block(pos).is_solid())
            })
        })
    }
}

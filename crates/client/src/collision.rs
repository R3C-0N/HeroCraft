//! Collisions du joueur avec les blocs : le sol sous les pieds, les murs et les plafonds sont lus
//! bloc par bloc dans le terrain (générateur + modifications), là où le maillage voxel les dessine.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use glam::{DVec3, Vec3};
use herocraft_common::{BlockId, PlanetFace};
use herocraft_world::generator::ColumnSample;
use herocraft_world::{BlockGeometry, BlockPos, PlanetWorldGenerator, TerrainEdits, TerrainQuery, WORLD_HEIGHT};

/// Au-delà, le cache de colonnes est vidé (le joueur ne touche qu'une poignée de colonnes par image)
const MAX_CACHED_COLUMNS: usize = 8192;

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
    /// pas les pieds de plus de `step` blocs (une marche franchissable). Une dalle, un escalier
    /// portent au dessus de leur pavé sous le point ; là où leurs pavés laissent un vide, le sol est
    /// plus bas.
    pub fn ground_radius(&self, generator: &PlanetWorldGenerator, p: Vec3, step: f32) -> f32 {
        let Some((pos, local)) = self.geometry.locate(p.as_dvec3()) else {
            return self.geometry.base_radius as f32;
        };
        let altitude = (p.length() as f64 - self.geometry.base_radius) / self.geometry.block_height;
        let limit = altitude + step as f64;
        let top = self.with_query(generator, |q| {
            let mut from_y = limit.floor() as i32;
            loop {
                let top = q.support_top(pos.face, pos.x, pos.z, from_y);
                if top <= 0 {
                    return 0.0;
                }
                let y = top - 1;
                let block = q.block(BlockPos { y, ..pos });
                let height = match block.shape() {
                    Some(shape) => shape.top_at(block.shape_variant(), local[0], local[2], limit - y as f64),
                    None => Some(1.0),
                };
                match height {
                    Some(h) if y as f64 + h <= limit + 1e-9 => return y as f64 + h,
                    _ => from_y = y - 1,
                }
            }
        });
        (self.geometry.base_radius + top * self.geometry.block_height) as f32
    }

    /// Rayon du dessus de la colonne entière sous une direction (plus haut bloc porteur du monde)
    pub fn column_top_radius(&self, generator: &PlanetWorldGenerator, dir: Vec3) -> f32 {
        let Some(pos) = self.geometry.block_at_point(dir.normalize().as_dvec3() * self.geometry.base_radius) else {
            return self.geometry.base_radius as f32;
        };
        let top = self.with_query(generator, |q| q.support_top(pos.face, pos.x, pos.z, WORLD_HEIGHT - 1));
        self.radius_of_altitude(top)
    }

    /// Rayon du dessus du plus haut bloc porteur de chaque colonne `(x, z)` de la face `face`
    /// (là où s'arrête la pluie), en une seule lecture du terrain
    pub fn support_tops(&self, generator: &PlanetWorldGenerator, face: PlanetFace, columns: &[(i32, i32)]) -> Vec<f32> {
        let tops: Vec<i32> = self.with_query(generator, |q| columns.iter().map(|&(x, z)| q.support_top(face, x, z, WORLD_HEIGHT - 1)).collect());
        tops.into_iter().map(|y| self.radius_of_altitude(y)).collect()
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

    /// Bloc qui contient le point `p` (l'air hors de la grille)
    pub fn block_at(&self, generator: &PlanetWorldGenerator, p: Vec3) -> BlockId {
        match self.geometry.block_at_point(p.as_dvec3()) {
            Some(pos) if pos.is_valid() => self.with_query(generator, |q| q.block(pos)),
            _ => BlockId::AIR,
        }
    }

    /// Un des points est dans la matière d'un bloc qui arrête le corps (ni air, ni eau ; les
    /// pavés seuls d'une dalle ou d'un escalier)
    pub fn any_solid(&self, generator: &PlanetWorldGenerator, points: &[Vec3]) -> bool {
        self.with_query(generator, |q| {
            points.iter().any(|p| {
                self.geometry.locate(DVec3::new(p.x as f64, p.y as f64, p.z as f64)).is_some_and(|(pos, local)| {
                    let block = if pos.is_valid() { q.block(pos) } else { BlockId::AIR };
                    block.is_solid() && block.contains(local)
                })
            })
        })
    }
}

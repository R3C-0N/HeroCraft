//! Modifications du terrain par le joueur (blocs détruits ou posés).
//!
//! Le terrain est entièrement procédural : un chunk qu'aucun joueur n'a touché n'existe que dans le
//! générateur, et le maillage le recalcule à la demande. Un chunk modifié est copié une fois depuis le
//! générateur, puis ses blocs sont changés en place : il remplace alors le générateur partout où le
//! client lit le terrain (maillage, visée), et il est enregistré dans la sauvegarde du monde.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::sync::{Arc, RwLock};

use glam::DVec3;
use herocraft_common::{BlockId, PlanetChunkCoord, PlanetFace};
use herocraft_voxel_engine::{ChunkData, ChunkStorage, CHUNK_SIZE};

use crate::generator::{ColumnSample, PlanetWorldGenerator};

/// Chunks par arête de face du cube
pub const FACE_CHUNKS: i32 = 32;
/// Blocs par arête de face du cube
pub const FACE_BLOCKS: i32 = FACE_CHUNKS * CHUNK_SIZE as i32;
/// Couches de chunks en altitude (celles que le maillage sait afficher)
pub const ALTITUDE_LAYERS: i32 = 17;
/// Hauteur du monde en blocs : aucun bloc n'est posé au-delà
pub const WORLD_HEIGHT: i32 = ALTITUDE_LAYERS * CHUNK_SIZE as i32;

/// Position d'un bloc sur la planète : face du cube, colonne (x le long de u, z le long de v,
/// de 0 à `FACE_BLOCKS`) et altitude y en blocs depuis le fond du monde
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub face: PlanetFace,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(face: PlanetFace, x: i32, y: i32, z: i32) -> Self {
        Self { face, x, y, z }
    }

    /// Le bloc est sur la face et entre le fond et le plafond du monde
    pub fn is_valid(&self) -> bool {
        (0..FACE_BLOCKS).contains(&self.x) && (0..FACE_BLOCKS).contains(&self.z) && (0..WORLD_HEIGHT).contains(&self.y)
    }

    pub fn chunk_coord(&self) -> PlanetChunkCoord {
        let size = CHUNK_SIZE as i32;
        PlanetChunkCoord::new(self.face, self.x.div_euclid(size), self.z.div_euclid(size), self.y.div_euclid(size))
    }

    /// Coordonnées dans le chunk, dans l'ordre de `ChunkData::get_voxel` (x, y, z)
    pub fn local(&self) -> (u8, u8, u8) {
        let size = CHUNK_SIZE as i32;
        (self.x.rem_euclid(size) as u8, self.y.rem_euclid(size) as u8, self.z.rem_euclid(size) as u8)
    }
}

#[derive(Default)]
struct EditsInner {
    chunks: HashMap<PlanetChunkCoord, Arc<ChunkData>>,
    /// Couches modifiées de chaque colonne de chunks (face, x, y) : le maillage d'une colonne les lit
    layers: HashMap<(PlanetFace, i32, i32), BTreeSet<i32>>,
    /// Chunks modifiés depuis le dernier enregistrement
    dirty: HashSet<PlanetChunkCoord>,
}

/// Les chunks modifiés d'une planète, partagés entre le fil principal (qui modifie) et les fils de
/// maillage (qui lisent). Un chunk est un `Arc` : un maillage en cours garde la version qu'il a lue,
/// une modification en fait une copie sans attendre qu'il ait fini.
pub struct TerrainEdits {
    inner: RwLock<EditsInner>,
    storage: Option<ChunkStorage>,
}

impl TerrainEdits {
    /// Reprend les chunks modifiés enregistrés dans la sauvegarde
    pub fn new(storage: Option<ChunkStorage>) -> Self {
        let mut inner = EditsInner::default();
        if let Some(storage) = &storage {
            match storage.list_chunks() {
                Ok(coords) => {
                    for coord in coords {
                        match storage.load_chunk(&coord) {
                            Ok(Some(chunk)) => {
                                inner.layers.entry((coord.face, coord.x, coord.y)).or_default().insert(coord.altitude_layer);
                                inner.chunks.insert(coord, Arc::new(chunk));
                            }
                            Ok(None) => {}
                            Err(e) => log::warn!("Chunk modifié illisible {:?} : {}", coord, e),
                        }
                    }
                }
                Err(e) => log::warn!("Liste des chunks modifiés illisible : {}", e),
            }
            if !inner.chunks.is_empty() {
                log::info!("{} chunks modifiés repris de la sauvegarde.", inner.chunks.len());
            }
        }
        Self { inner: RwLock::new(inner), storage }
    }

    /// Nombre de chunks modifiés
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Le chunk modifié à cette coordonnée, s'il y en a un
    pub fn chunk(&self, coord: &PlanetChunkCoord) -> Option<Arc<ChunkData>> {
        self.inner.read().unwrap().chunks.get(coord).cloned()
    }

    /// Couches modifiées d'une colonne de chunks, de bas en haut
    pub fn edited_layers(&self, face: PlanetFace, chunk_x: i32, chunk_y: i32) -> Vec<i32> {
        self.inner
            .read()
            .unwrap()
            .layers
            .get(&(face, chunk_x, chunk_y))
            .map(|layers| layers.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Le bloc à cette position : celui du joueur s'il l'a modifié, sinon celui du générateur
    pub fn block(&self, generator: &PlanetWorldGenerator, pos: BlockPos) -> BlockId {
        if let Some(chunk) = self.chunk(&pos.chunk_coord()) {
            let (x, y, z) = pos.local();
            return chunk.get_voxel(x, y, z);
        }
        generator.block_at(pos.face, pos.x, pos.y, pos.z)
    }

    /// Remplace un bloc. Renvoie `false` sans rien changer si la position est hors du monde ou si le
    /// bloc y est déjà.
    pub fn set_block(&self, generator: &PlanetWorldGenerator, pos: BlockPos, block: BlockId) -> bool {
        self.set_block_with(pos, block, |coord| generator.generate_chunk(coord))
    }

    /// Comme `set_block`, le chunk d'origine étant fourni par `generate` s'il n'est pas encore
    /// modifié (le client le tire des colonnes qu'il a déjà échantillonnées pour le maillage)
    pub fn set_block_with<G>(&self, pos: BlockPos, block: BlockId, generate: G) -> bool
    where
        G: FnOnce(&PlanetChunkCoord) -> ChunkData,
    {
        if !pos.is_valid() {
            return false;
        }
        let coord = pos.chunk_coord();
        let (x, y, z) = pos.local();
        // Le chunk est généré hors du verrou : les fils de maillage continuent de lire pendant ce temps
        let generated = if self.chunk(&coord).is_none() { Some(generate(&coord)) } else { None };

        let mut inner = self.inner.write().unwrap();
        let chunk = match (inner.chunks.contains_key(&coord), generated) {
            (true, _) => inner.chunks.get_mut(&coord).unwrap(),
            (false, Some(chunk)) => inner.chunks.entry(coord).or_insert(Arc::new(chunk)),
            // Seul le fil principal modifie : un chunk présent à la lecture l'est encore ici
            (false, None) => return false,
        };
        if chunk.get_voxel(x, y, z) == block {
            // Un chunk tout juste copié du générateur sans changement n'est pas gardé
            if !inner.layers.get(&(coord.face, coord.x, coord.y)).is_some_and(|l| l.contains(&coord.altitude_layer)) {
                inner.chunks.remove(&coord);
            }
            return false;
        }
        Arc::make_mut(chunk).set_voxel(x, y, z, block);
        inner.layers.entry((coord.face, coord.x, coord.y)).or_default().insert(coord.altitude_layer);
        inner.dirty.insert(coord);
        true
    }

    /// Enregistre les chunks modifiés depuis le dernier appel. Renvoie leur nombre.
    pub fn flush(&self) -> io::Result<usize> {
        let Some(storage) = &self.storage else {
            return Ok(0);
        };
        let pending: Vec<(PlanetChunkCoord, Arc<ChunkData>)> = {
            let mut inner = self.inner.write().unwrap();
            let dirty: Vec<PlanetChunkCoord> = inner.dirty.drain().collect();
            dirty.into_iter().filter_map(|c| inner.chunks.get(&c).map(|chunk| (c, chunk.clone()))).collect()
        };
        for (coord, chunk) in &pending {
            if let Err(e) = storage.save_chunk(coord, chunk) {
                // Rien n'est perdu : le chunk sera retenté au prochain enregistrement
                self.inner.write().unwrap().dirty.insert(*coord);
                return Err(e);
            }
        }
        Ok(pending.len())
    }
}

/// Lecture du terrain bloc par bloc, pour la visée : les colonnes du générateur déjà échantillonnées
/// sont gardées, un rayon traversant une dizaine de blocs n'en échantillonne qu'une poignée.
pub struct TerrainQuery<'a> {
    generator: &'a PlanetWorldGenerator,
    edits: &'a TerrainEdits,
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl<'a> TerrainQuery<'a> {
    pub fn new(generator: &'a PlanetWorldGenerator, edits: &'a TerrainEdits) -> Self {
        Self { generator, edits, columns: HashMap::new() }
    }

    pub fn block(&mut self, pos: BlockPos) -> BlockId {
        if let Some(chunk) = self.edits.chunk(&pos.chunk_coord()) {
            let (x, y, z) = pos.local();
            return chunk.get_voxel(x, y, z);
        }
        let generator = self.generator;
        let column = self
            .columns
            .entry((pos.face, pos.x, pos.z))
            .or_insert_with(|| generator.sample_block_column(pos.face, pos.x, pos.z));
        generator.column_block(column, pos.y)
    }
}

/// Géométrie des blocs sur la sphère : un bloc (x, y, z) d'une face occupe les directions de la
/// face entre u = -1 + x·pas et u + pas (de même en v avec z), entre les rayons R + y·h et R + (y+1)·h,
/// h étant la largeur d'un bloc au centre de la face. C'est exactement le volume que dessine le
/// maillage voxel.
#[derive(Debug, Clone, Copy)]
pub struct BlockGeometry {
    pub base_radius: f64,
    pub block_height: f64,
}

impl BlockGeometry {
    pub fn new(base_radius: f64) -> Self {
        let voxel_span_uv = 2.0 / FACE_BLOCKS as f64;
        Self { base_radius, block_height: base_radius * std::f64::consts::FRAC_PI_4 * voxel_span_uv }
    }

    /// Coin (x, y, z) de la grille d'une face, en coordonnées continues de blocs
    pub fn corner(&self, face: PlanetFace, x: f64, y: f64, z: f64) -> DVec3 {
        let voxel_span_uv = 2.0 / FACE_BLOCKS as f64;
        let dir = face.uv_to_sphere_direction(-1.0 + x * voxel_span_uv, -1.0 + z * voxel_span_uv);
        dir * (self.base_radius + y * self.block_height)
    }

    /// Les 8 coins d'un bloc : bit 0 = x + 1, bit 1 = y + 1, bit 2 = z + 1
    pub fn block_corners(&self, pos: BlockPos) -> [DVec3; 8] {
        std::array::from_fn(|i| {
            let (dx, dy, dz) = ((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
            self.corner(pos.face, pos.x as f64 + dx, pos.y as f64 + dy, pos.z as f64 + dz)
        })
    }

    /// Le bloc qui contient un point de l'espace (`None` au centre de la planète)
    pub fn block_at_point(&self, p: DVec3) -> Option<BlockPos> {
        let r = p.length();
        if r < 1e-9 {
            return None;
        }
        let (face, u, v) = PlanetFace::sphere_direction_to_face_uv(p / r);
        let to_block = |t: f64| (((t + 1.0) * 0.5 * FACE_BLOCKS as f64).floor() as i32).clamp(0, FACE_BLOCKS - 1);
        let y = ((r - self.base_radius) / self.block_height).floor() as i32;
        Some(BlockPos::new(face, to_block(u), y, to_block(v)))
    }
}

/// Bloc touché par un rayon
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Le bloc solide touché
    pub block: BlockPos,
    /// Le bloc vide juste avant, contre la face touchée : là où un bloc posé irait
    pub before: Option<BlockPos>,
    /// Distance de l'origine au point d'entrée dans le bloc
    pub distance: f64,
}

/// Lance un rayon depuis `origin` dans la direction `dir` (unitaire) et renvoie le premier bloc pour
/// lequel `is_solid` est vrai, à moins de `max_distance`.
///
/// Les blocs sont courbes (leurs faces latérales sont des plans passant par le centre de la
/// planète, leurs dessus des calottes) : pas de traversée de grille exacte possible, le rayon est
/// donc avancé par pas d'un dixième de bloc, puis le point d'entrée est affiné par dichotomie.
pub fn raycast_blocks<F>(geometry: &BlockGeometry, origin: DVec3, dir: DVec3, max_distance: f64, mut is_solid: F) -> Option<RayHit>
where
    F: FnMut(BlockPos) -> bool,
{
    let step = geometry.block_height * 0.1;
    let mut solid_cache: HashMap<BlockPos, bool> = HashMap::new();
    let mut solid_at = |t: f64| -> Option<(BlockPos, bool)> {
        let pos = geometry.block_at_point(origin + dir * t)?;
        let solid = *solid_cache.entry(pos).or_insert_with(|| pos.is_valid() && is_solid(pos));
        Some((pos, solid))
    };

    // Départ dans un bloc solide (caméra dans la roche) : il est visé tel quel
    if let Some((start, true)) = solid_at(0.0) {
        return Some(RayHit { block: start, before: None, distance: 0.0 });
    }

    let mut previous_t = 0.0;
    let mut t = step;
    while t <= max_distance + step {
        let t_clamped = t.min(max_distance);
        if let Some((_, true)) = solid_at(t_clamped) {
            // Entrée entre previous_t (vide) et t (solide) : dichotomie jusqu'au millième de bloc
            let (mut lo, mut hi) = (previous_t, t_clamped);
            while hi - lo > geometry.block_height * 1e-3 {
                let mid = 0.5 * (lo + hi);
                match solid_at(mid) {
                    Some((_, true)) => hi = mid,
                    _ => lo = mid,
                }
            }
            let (block, _) = solid_at(hi)?;
            let before = solid_at(lo).filter(|(_, solid)| !solid).map(|(pos, _)| pos).filter(|pos| *pos != block);
            return Some(RayHit { block, before, distance: hi });
        }
        if t_clamped >= max_distance {
            break;
        }
        previous_t = t_clamped;
        t += step;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;

    fn generator() -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() })
    }

    #[test]
    fn block_at_matches_generated_chunk() {
        let generator = generator();
        let coord = PlanetChunkCoord::new(PlanetFace::PosZ, 13, 17, 3);
        let chunk = generator.generate_chunk(&coord);
        for &(x, y, z) in &[(0u8, 0u8, 0u8), (5, 4, 9), (31, 31, 31), (16, 12, 3), (2, 20, 30)] {
            let pos = BlockPos::new(coord.face, coord.x * 32 + x as i32, coord.altitude_layer * 32 + y as i32, coord.y * 32 + z as i32);
            assert_eq!(pos.chunk_coord(), coord);
            assert_eq!(pos.local(), (x, y, z));
            assert_eq!(generator.block_at(pos.face, pos.x, pos.y, pos.z), chunk.get_voxel(x, y, z), "{:?}", pos);
        }
    }

    #[test]
    fn set_block_copies_the_chunk_then_changes_one_block() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let pos = BlockPos::new(PlanetFace::PosX, 500, 140, 600);
        let neighbour = BlockPos::new(PlanetFace::PosX, 501, 140, 600);
        let before = edits.block(&generator, neighbour);

        assert!(edits.set_block(&generator, pos, BlockId::STONE) || generator.block_at(pos.face, pos.x, pos.y, pos.z) == BlockId::STONE);
        assert_eq!(edits.block(&generator, pos), BlockId::STONE);
        assert_eq!(edits.block(&generator, neighbour), before, "le reste du chunk est celui du générateur");
        assert!(!edits.set_block(&generator, pos, BlockId::STONE), "rien ne change, rien n'est signalé");
        assert!(edits.set_block(&generator, pos, BlockId::AIR));
        assert_eq!(edits.edited_layers(PlanetFace::PosX, 15, 18), vec![4]);
        assert!(!edits.set_block(&generator, BlockPos::new(PlanetFace::PosX, 0, WORLD_HEIGHT, 0), BlockId::STONE));
    }

    #[test]
    fn edits_survive_a_save_and_reload() {
        let dir = std::env::temp_dir().join("herocraft_test_terrain_edits");
        let _ = std::fs::remove_dir_all(&dir);
        let generator = generator();
        let pos = BlockPos::new(PlanetFace::NegY, 100, 300, 900);
        {
            let edits = TerrainEdits::new(Some(ChunkStorage::new(&dir).unwrap()));
            edits.set_block(&generator, pos, BlockId::STONE);
            assert_eq!(edits.flush().unwrap(), 1);
            assert_eq!(edits.flush().unwrap(), 0);
        }
        let edits = TerrainEdits::new(Some(ChunkStorage::new(&dir).unwrap()));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits.block(&generator, pos), BlockId::STONE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn block_at_point_inverts_corners() {
        let geometry = BlockGeometry::new(50.0);
        for face in PlanetFace::ALL {
            for &(x, y, z) in &[(0, 0, 0), (511, 120, 7), (1023, 300, 1023), (40, 99, 800)] {
                let pos = BlockPos::new(face, x, y, z);
                let corners = geometry.block_corners(pos);
                let centre = corners.iter().fold(DVec3::ZERO, |a, c| a + *c) / 8.0;
                assert_eq!(geometry.block_at_point(centre), Some(pos));
            }
        }
    }

    #[test]
    fn raycast_hits_the_first_solid_block_and_the_face_before_it() {
        let geometry = BlockGeometry::new(50.0);
        let face = PlanetFace::PosY;
        // Sol plat à y < 100 ; le rayon part de trois blocs au-dessus et vise droit vers le bas
        let column = BlockPos::new(face, 300, 0, 400);
        let corners = geometry.block_corners(BlockPos { y: 103, ..column });
        let origin = corners.iter().fold(DVec3::ZERO, |a, c| a + *c) / 8.0;
        let hit = raycast_blocks(&geometry, origin, -origin.normalize(), 10.0 * geometry.block_height, |p| p.y < 100).unwrap();
        assert_eq!(hit.block, BlockPos { y: 99, ..column });
        assert_eq!(hit.before, Some(BlockPos { y: 100, ..column }));
        assert!((hit.distance - 3.5 * geometry.block_height).abs() < 0.01 * geometry.block_height);

        // Trop loin : rien
        assert!(raycast_blocks(&geometry, origin, -origin.normalize(), 2.0 * geometry.block_height, |p| p.y < 100).is_none());

        // Un mur à x = 305 : visé de côté, le bloc d'avant est son voisin en x
        let side = geometry.corner(face, 302.5, 100.5, 400.5);
        let target = geometry.corner(face, 310.5, 100.5, 400.5);
        let hit = raycast_blocks(&geometry, side, (target - side).normalize(), 10.0 * geometry.block_height, |p| p.x >= 305).unwrap();
        assert_eq!(hit.block, BlockPos::new(face, 305, 100, 400));
        assert_eq!(hit.before, Some(BlockPos::new(face, 304, 100, 400)));
    }
}

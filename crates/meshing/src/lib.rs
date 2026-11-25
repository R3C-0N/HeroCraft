//! Maillage du terrain voxel de HeroCraft : blocs des colonnes de chunks, relief lointain (LOD),
//! lumière des blocs et textures. Sorti du client pour être testé et mesuré sans fenêtre ni GPU
//! (`cargo bench -p herocraft_meshing`).

pub mod block_light;
pub mod block_textures;
pub mod planet_mesh;
pub mod texture;
pub mod voxel_mesh;

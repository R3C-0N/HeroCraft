//! Passes de rendu GPU de HeroCraft (wgpu) : assemblage des shaders, post-traitement HDR, ombres,
//! atmosphère, nuages, précipitations, fissures et contours des blocs, tampons et tri du relief,
//! modèles animés. Les réglages graphiques (`settings`) sont ceux que modifie le menu Options.
//! La boucle de rendu qui les assemble (`State`) reste dans la crate du client.

// Maillage du terrain (crate herocraft_meshing), lu par les tampons du relief et les ombres
use herocraft_meshing::planet_mesh;

pub mod atmosphere;
pub mod block_cracks;
pub mod block_outline;
pub mod cloud_mesh;
pub mod flattening;
pub mod post;
pub mod precipitation;
pub mod settings;
pub mod shaders;
pub mod shadow;
pub mod skinned_model;
pub mod terrain_cull;
pub mod terrain_pool;

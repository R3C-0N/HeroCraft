use glam::{DVec3, Vec3};
use herocraft_common::{BlockId, PlanetFace};
use herocraft_world::{
    cubed_sphere::unit_sphere_to_lat_lon,
    generator::{BiomeType, PlanetWorldGenerator, SEA_LEVEL_BLOCKS},
};

/// Sommet pour le rendu de la planète
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlanetVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    /// Texture du bloc : rang dans la liste des textures (16 bits bas, `NO_TEXTURE` : couleur
    /// seule), puis u et v en seizièmes de la texture (8 bits chacun)
    pub texture: u32,
    /// Lumière des torches et autres blocs qui éclairent reçue par le sommet, de 0 à 1
    /// (`block_light::intensity`)
    pub light: f32,
}

impl PlanetVertex {
    /// Sans texture : la couleur seule
    pub const UNTEXTURED: u32 = herocraft_common::NO_TEXTURE as u32;

    /// Texture `layer` lue en (u, v), en seizièmes
    pub fn pack_texture(layer: u16, u: u8, v: u8) -> u32 {
        layer as u32 | (u as u32) << 16 | (v as u32) << 24
    }

    /// Premier tampon envoyé au GPU : la position seule (12 octets), la seule donnée que lisent
    /// les passes d'ombre
    pub fn position_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }

    /// Second tampon : normale et opacité, couleur, texture, tassées en 12 octets
    /// (`PackedAttributes`). La texture est à l'emplacement 8, loin de ceux qu'ajoutent les
    /// créatures dans un troisième tampon.
    pub fn attribute_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![1 => Snorm8x4, 2 => Unorm8x4, 8 => Uint32];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PackedAttributes>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }

    /// Données du second tampon pour ce sommet. Arrondis faits à la main (`+ 0.5` puis
    /// troncature) : `f32::round` appelle une fonction de la libm, trois fois plus lent ici.
    pub fn packed_attributes(&self) -> PackedAttributes {
        let snorm = |x: f32| (((x.clamp(-1.0, 1.0) + 1.0) * 127.0 + 0.5) as i32 - 127) as i8;
        // Racine de la couleur : la précision des 8 bits va aux tons sombres (le shader l'élève
        // au carré), où un pas linéaire de 1/255 se verrait
        let unorm = |x: f32| (x.clamp(0.0, 1.0).sqrt() * 255.0 + 0.5) as u8;
        let [nx, ny, nz] = self.normal;
        let [r, g, b, a] = self.color;
        PackedAttributes {
            normal_alpha: [snorm(nx), snorm(ny), snorm(nz), snorm(a)],
            color: [unorm(r), unorm(g), unorm(b), (self.light.clamp(0.0, 1.0) * 255.0 + 0.5) as u8],
            texture: self.texture,
        }
    }
}

/// Attributs d'un sommet tels que le GPU les lit (`PlanetVertex::attribute_layout`) : 12 octets au
/// lieu de 32. La normale ne sert qu'à orienter l'eau (le shader du terrain prend la normale de la
/// facette) : 8 bits lui suffisent.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PackedAttributes {
    /// Normale (xyz) et opacité du sommet (w, négative pour un fondu sortant), de -127 à 127
    pub normal_alpha: [i8; 4],
    /// Racine carrée de la couleur (rgb), de 0 à 255, puis la lumière des blocs (a)
    pub color: [u8; 4],
    pub texture: u32,
}

/// Sommets d'un maillage dans les deux tampons du GPU : positions, puis attributs tassés
pub fn split_vertices(vertices: &[PlanetVertex]) -> (Vec<[f32; 3]>, Vec<PackedAttributes>) {
    (vertices.iter().map(|v| v.position).collect(), vertices.iter().map(PlanetVertex::packed_attributes).collect())
}

/// Indices d'un maillage tels que le GPU les lit : sur 16 bits quand il a au plus 65 536 sommets
/// (deux fois moins de données à lire), sur 32 bits sinon
pub enum PackedIndices {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl PackedIndices {
    pub fn new(indices: &[u32], vertex_count: usize) -> Self {
        if vertex_count <= 1 << 16 {
            Self::U16(indices.iter().map(|&i| i as u16).collect())
        } else {
            Self::U32(indices.to_vec())
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::U16(i) => bytemuck::cast_slice(i),
            Self::U32(i) => bytemuck::cast_slice(i),
        }
    }

    pub fn format(&self) -> wgpu::IndexFormat {
        match self {
            Self::U16(_) => wgpu::IndexFormat::Uint16,
            Self::U32(_) => wgpu::IndexFormat::Uint32,
        }
    }
}

/// Maillage prêt à envoyer au GPU (`PlanetVertex::position_layout` et `attribute_layout`) :
/// 24 octets par sommet au lieu de 44, et des indices courts quand c'est possible
pub struct GpuMeshData {
    pub positions: Vec<[f32; 3]>,
    pub attributes: Vec<PackedAttributes>,
    pub indices: PackedIndices,
    pub num_indices: u32,
}

impl GpuMeshData {
    pub fn new(mesh: &PlanetMesh) -> Self {
        let (positions, attributes) = split_vertices(&mesh.vertices);
        let indices = PackedIndices::new(&mesh.indices, mesh.vertices.len());
        Self { positions, attributes, indices, num_indices: mesh.indices.len() as u32 }
    }

    /// Octets envoyés au GPU pour ce maillage, sans le tasser
    pub fn upload_size(mesh: &PlanetMesh) -> usize {
        let index_size = if mesh.vertices.len() <= 1 << 16 { 2 } else { 4 };
        mesh.vertices.len() * (std::mem::size_of::<[f32; 3]>() + std::mem::size_of::<PackedAttributes>()) + mesh.indices.len() * index_size
    }
}

/// Arbres d'un patch de relief lointain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeDetail {
    /// Aucun
    None,
    /// Un pavé de feuillage et un tronc par arbre, sans texture
    Boxes,
    /// Bloc par bloc et texturés, comme dans les chunks voxel
    Blocks,
}

/// Finesse d'un patch de relief lointain
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatchDetail {
    /// Cellules par côté
    pub resolution: usize,
    /// Hauteur des terrasses en blocs (1 : relief au bloc près)
    pub terrace_step: f64,
    /// Textures des blocs sur le dessus et les flancs (relief au bloc près seulement)
    pub textured: bool,
    pub trees: TreeDetail,
}

impl PatchDetail {
    /// Couleurs seules, sans arbres
    pub const fn plain(resolution: usize, terrace_step: f64) -> Self {
        Self { resolution, terrace_step, textured: false, trees: TreeDetail::None }
    }
}

/// Une cellule de patch : hauteur de son dessus, couleur, et matières pour les textures
#[derive(Debug, Clone, Copy)]
struct LodCell {
    height: f64,
    water: bool,
    color: [f32; 4],
    /// Bloc du dessus (`None` : couleur seule)
    top: Option<BlockId>,
    /// Bloc sous le dessus, pour le bas des falaises
    under: Option<BlockId>,
}

/// Matière sous un bloc de surface, visible au flanc des falaises
fn subsoil(surface: BlockId) -> BlockId {
    match surface {
        BlockId::GRASS | BlockId::DIRT => BlockId::DIRT,
        BlockId::SAND => BlockId::SAND,
        _ => BlockId::STONE,
    }
}

/// Bornes d'un arbre (dx, dy, dz relatifs au pied du tronc) : tous ses blocs, puis son feuillage
fn tree_extent(tree: &herocraft_world::Tree) -> Vec<(i32, i32, i32, BlockId, bool)> {
    let r = tree.reach();
    let mut blocks = Vec::new();
    for y in tree.ground..tree.top() {
        for dz in -r..=r {
            for dx in -r..=r {
                if let Some((block, trunk)) = tree.block(tree.x + dx, y, tree.z + dz) {
                    blocks.push((dx, y - tree.ground, dz, block, trunk));
                }
            }
        }
    }
    blocks
}

/// Arbre bloc par bloc : les faces de ses blocs que l'arbre ne cache pas lui-même, texturées comme
/// dans les chunks voxel. `lift` : décalage vertical (en blocs) vers le dessus de la cellule.
fn emit_tree_blocks(vertices: &mut Vec<PlanetVertex>, indices: &mut Vec<u32>, tree: &herocraft_world::Tree, lift: f64, corner: &impl Fn(f64, f64, f64) -> Vec3) {
    let blocks = tree_extent(tree);
    let occupied: std::collections::HashSet<(i32, i32, i32)> = blocks.iter().map(|b| (b.0, b.1, b.2)).collect();
    for &(dx, dy, dz, block, _) in &blocks {
        for (dir, face) in crate::voxel_mesh::CUBE_FACES.iter().enumerate() {
            let d = herocraft_common::FACE_DIRS[dir];
            if occupied.contains(&(dx + d.0, dy + d.1, dz + d.2)) {
                continue;
            }
            let layer = herocraft_common::blocks().face_texture(block, dir);
            let shade = if dir == 2 { 1.0 } else { 0.78 };
            let c = crate::voxel_mesh::VoxelMeshBuilder::block_color(block, dir);
            let color = [c[0] * shade, c[1] * shade, c[2] * shade, c[3]];
            let p = face.corners.map(|(cx, cy, cz)| {
                corner(
                    (tree.x + dx) as f64 + cx as f64,
                    (tree.ground + dy) as f64 + lift + cy as f64,
                    (tree.z + dz) as f64 + cz as f64,
                )
            });
            let texture = face.corners.map(|(cx, cy, cz)| {
                let (u, v) = crate::block_textures::face_uv(dir, [cx * 16, cy * 16, cz * 16]);
                PlanetVertex::pack_texture(layer, u, v)
            });
            crate::voxel_mesh::VoxelMeshBuilder::add_quad(vertices, indices, p[0], p[1], p[2], p[3], color, [1.0; 4], [0.0; 4], texture);
        }
    }
}

/// Arbre simplifié : un pavé de feuillage (ses bornes) sur un tronc d'un bloc de section
fn emit_tree_boxes(vertices: &mut Vec<PlanetVertex>, indices: &mut Vec<u32>, tree: &herocraft_world::Tree, lift: f64, corner: &impl Fn(f64, f64, f64) -> Vec3) {
    let blocks = tree_extent(tree);
    let mut boxes: Vec<(BlockId, [i32; 6])> = Vec::with_capacity(2);
    for want_trunk in [true, false] {
        let mut bounds: Option<(BlockId, [i32; 6])> = None;
        for &(dx, dy, dz, block, trunk) in &blocks {
            if trunk != want_trunk {
                continue;
            }
            let b = bounds.get_or_insert((block, [dx, dy, dz, dx + 1, dy + 1, dz + 1]));
            b.1 = [b.1[0].min(dx), b.1[1].min(dy), b.1[2].min(dz), b.1[3].max(dx + 1), b.1[4].max(dy + 1), b.1[5].max(dz + 1)];
        }
        boxes.extend(bounds);
    }
    for (block, [ax, ay, az, bx, by, bz]) in boxes {
        for (dir, face) in crate::voxel_mesh::CUBE_FACES.iter().enumerate() {
            let shade = if dir == 2 { 1.0 } else { 0.78 };
            let c = crate::voxel_mesh::VoxelMeshBuilder::block_color(block, dir);
            let color = [c[0] * shade, c[1] * shade, c[2] * shade, 1.0];
            let p = face.corners.map(|(cx, cy, cz)| {
                let pick = |lo: i32, hi: i32, c: u8| if c == 0 { lo } else { hi } as f64;
                corner(
                    tree.x as f64 + pick(ax, bx, cx),
                    tree.ground as f64 + lift + pick(ay, by, cy),
                    tree.z as f64 + pick(az, bz, cz),
                )
            });
            crate::voxel_mesh::VoxelMeshBuilder::add_quad(vertices, indices, p[0], p[1], p[2], p[3], color, [1.0; 4], [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
        }
    }
}

/// Plus longue rangée (en blocs) de dessus fusionnés en un quad : la texture y est répétée, en
/// seizièmes sur 8 bits (15 × 16 = 240)
const MAX_MERGED_BLOCKS: f64 = 15.0;

/// Plus grande cellule de patch (en blocs de côté) où les villes et villages sont dessinés
const MAX_SETTLEMENT_CELL_BLOCKS: f64 = 4.0;
/// Plus grande cellule de patch (en blocs de côté) où les chemins sont dessinés
const MAX_ROAD_CELL_BLOCKS: f64 = 2.0;

/// Maillage indexé prêt pour WGPU
#[derive(Clone, Default)]
pub struct PlanetMesh {
    pub vertices: Vec<PlanetVertex>,
    pub indices: Vec<u32>,
}

impl PlanetMesh {
    /// Hauteur d'un palier pour le relief orbital étagé en strates / terrasses (en blocs)
    pub const ORBIT_TERRACE_STEP_HEIGHT: f64 = 6.0;

    /// Discrétise une altitude en paliers de `step` blocs au-dessus de la mer. Avec un pas d'un
    /// bloc, le dessus d'une cellule est exactement celui des voxels (`floor` de l'élévation).
    #[inline]
    pub fn quantize_elevation(elevation: f64, step: f64) -> f64 {
        if elevation <= SEA_LEVEL_BLOCKS {
            if step <= 1.0 { elevation.floor() } else { elevation }
        } else {
            let diff = elevation - SEA_LEVEL_BLOCKS;
            SEA_LEVEL_BLOCKS + (diff / step).floor() * step
        }
    }

    /// Applique un facteur d'opacité/fondu alpha à tous les sommets du maillage
    #[allow(dead_code)]
    pub fn set_alpha(&mut self, alpha: f32) {
        for v in &mut self.vertices {
            v.color[3] = alpha;
        }
    }
    /// Construit le maillage complet de la planète Cubed-Sphere avec relief en colonnes voxels à 90° et biomes
    #[allow(dead_code)]
    pub fn generate_planet(
        generator: &PlanetWorldGenerator,
        base_radius: f32,
        grid_resolution: usize,
        filter: herocraft_world::VisualizationFilter,
    ) -> Self {
        let mut combined_mesh = PlanetMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };

        for &face in &PlanetFace::ALL {
            let key = herocraft_world::QuadNodeKey::new(face, 0, 0, 0);
            let patch = Self::generate_lod_patch(generator, &key, base_radius, grid_resolution, filter);
            let base_idx = combined_mesh.vertices.len() as u32;
            combined_mesh.vertices.extend(patch.vertices);
            combined_mesh.indices.extend(patch.indices.into_iter().map(|idx| idx + base_idx));
        }

        combined_mesh
    }

    /// Palette de couleurs enrichie avec gradation altimétrique et bathymétrique
    /// Fait ressortir les fosses océaniques (bleu nuit abyssal très contrasté) et les massifs/super-chaînes de montagnes (ardoise sombre -> granite -> cimes éclatantes)
    pub fn relief_biome_color(biome: BiomeType, raw_elevation: f64) -> [f32; 4] {
        // Enneigement strict au-dessus de 300 blocs pour n'importe quel relief continental
        if !biome.is_water() && raw_elevation >= 300.0 {
            let t = ((raw_elevation - 300.0) / 50.0).clamp(0.0, 1.0) as f32;
            let c_snowline = [0.88, 0.93, 0.99, 1.0]; // Ligne des neiges / séracs bleutés à 300
            let c_summit = [1.00, 1.00, 1.00, 1.0];   // Sommet éclatant de pureté à 350
            return [
                c_snowline[0] * (1.0 - t) + c_summit[0] * t,
                c_snowline[1] * (1.0 - t) + c_summit[1] * t,
                c_snowline[2] * (1.0 - t) + c_summit[2] * t,
                1.0,
            ];
        }

        if biome.is_water() {
            // Dégradé bathymétrique sous-marin :
            // raw_elevation va de ~15.0 (fosses océaniques et subductions profondes) à 100.0 (niveau de la mer)
            let depth = (SEA_LEVEL_BLOCKS - raw_elevation).max(0.0);
            let t = (depth / 80.0).clamp(0.0, 1.0) as f32;

            // 3 paliers d'interpolation :
            // t = 0.0 : Lagons et mers côtières peu profondes (azur / cyan lumineux)
            // t = 0.40 : Bassins océaniques moyens (bleu roi profond)
            // t = 1.0 : Fosses abyssales et zones de subduction (bleu nuit / noir abyssal très sombre et contrasté)
            let c_shallow = [0.08, 0.48, 0.78, 1.0];
            let c_mid = [0.04, 0.22, 0.62, 1.0];
            let c_trench = [0.006, 0.025, 0.16, 1.0];


            if t < 0.40 {
                let k = t / 0.40;
                [
                    c_shallow[0] * (1.0 - k) + c_mid[0] * k,
                    c_shallow[1] * (1.0 - k) + c_mid[1] * k,
                    c_shallow[2] * (1.0 - k) + c_mid[2] * k,
                    1.0,
                ]
            } else {
                let k = (t - 0.40) / 0.60;
                [
                    c_mid[0] * (1.0 - k) + c_trench[0] * k,
                    c_mid[1] * (1.0 - k) + c_trench[1] * k,
                    c_mid[2] * (1.0 - k) + c_trench[2] * k,
                    1.0,
                ]
            }
        } else {
            match biome {
                BiomeType::Beach => [0.88, 0.82, 0.56, 1.0],
                BiomeType::Desert => [0.90, 0.72, 0.38, 1.0],
                BiomeType::Forest => {
                    // Piémont forestier : fonce légèrement avec l'altitude en approchant des montagnes
                    if raw_elevation > 140.0 {
                        let k = ((raw_elevation - 140.0) / 30.0).clamp(0.0, 1.0) as f32;
                        let base = [0.14, 0.44, 0.16, 1.0];
                        let rock = [0.25, 0.35, 0.22, 1.0];
                        [
                            base[0] * (1.0 - k) + rock[0] * k,
                            base[1] * (1.0 - k) + rock[1] * k,
                            base[2] * (1.0 - k) + rock[2] * k,
                            1.0,
                        ]
                    } else {
                        [0.14, 0.44, 0.16, 1.0]
                    }
                }
                BiomeType::Plains => {
                    // Piémont des plaines : transition douce vers la roche sombre
                    if raw_elevation > 140.0 {
                        let k = ((raw_elevation - 140.0) / 30.0).clamp(0.0, 1.0) as f32;
                        let base = [0.28, 0.64, 0.22, 1.0];
                        let rock = [0.35, 0.42, 0.26, 1.0];
                        [
                            base[0] * (1.0 - k) + rock[0] * k,
                            base[1] * (1.0 - k) + rock[1] * k,
                            base[2] * (1.0 - k) + rock[2] * k,
                            1.0,
                        ]
                    } else {
                        [0.28, 0.64, 0.22, 1.0]
                    }
                }
                BiomeType::Mountains => {
                    // Massifs montagneux rocailleux (170.0 -> 300.0) :
                    // - Piémont et alpages (170 -> 215) : transition d'alpages montagnards vers la roche
                    // - Versants rocailleux et crêtes (215 -> 300) : roches minérales rugueuses (ardoise, basalte, granite)
                    let t = ((raw_elevation - 170.0) / 130.0).clamp(0.0, 1.0) as f32;
                    if t < 0.35 {
                        let k = t / 0.35;
                        let c_alpage = [0.26, 0.48, 0.22, 1.0]; // Vert alpage montagnard
                        let c_rock_base = [0.36, 0.34, 0.33, 1.0]; // Roche ardoise sombre
                        [
                            c_alpage[0] * (1.0 - k) + c_rock_base[0] * k,
                            c_alpage[1] * (1.0 - k) + c_rock_base[1] * k,
                            c_alpage[2] * (1.0 - k) + c_rock_base[2] * k,
                            1.0,
                        ]
                    } else {
                        let k = (t - 0.35) / 0.65;
                        let c_rock_base = [0.36, 0.34, 0.33, 1.0]; // Ardoise / basalte sombre
                        let c_granite = [0.60, 0.58, 0.56, 1.0];   // Granite minéral clair
                        [
                            c_rock_base[0] * (1.0 - k) + c_granite[0] * k,
                            c_rock_base[1] * (1.0 - k) + c_granite[1] * k,
                            c_rock_base[2] * (1.0 - k) + c_granite[2] * k,
                            1.0,
                        ]
                    }
                }
                BiomeType::SnowPeaks => {
                    // Calotte polaire affleurante au niveau de la mer (~101) vs Hauts sommets
                    if raw_elevation < SEA_LEVEL_BLOCKS + 5.0 {
                        [0.92, 0.96, 0.99, 1.0] // Banquise polaire immaculée
                    } else {
                        // Enneigement au-dessus de 300.0 jusqu'aux sommets (300.0 -> 350.0)
                        let t = ((raw_elevation - 300.0) / 50.0).clamp(0.0, 1.0) as f32;
                        let c_snowline = [0.88, 0.93, 0.99, 1.0]; // Ligne des neiges / séracs bleutés
                        let c_summit = [1.00, 1.00, 1.00, 1.0];   // Sommet éclatant de pureté
                        [
                            c_snowline[0] * (1.0 - t) + c_summit[0] * t,
                            c_snowline[1] * (1.0 - t) + c_summit[1] * t,
                            c_snowline[2] * (1.0 - t) + c_summit[2] * t,
                            1.0,
                        ]
                    }
                }
                BiomeType::Tundra | BiomeType::Taiga | BiomeType::Savanna | BiomeType::Jungle => {
                    // Piémont : même transition douce vers la roche que les plaines et les forêts
                    let base = Self::biome_color(biome);
                    if raw_elevation > 140.0 {
                        let k = ((raw_elevation - 140.0) / 30.0).clamp(0.0, 1.0) as f32;
                        let rock = [0.34, 0.38, 0.28, 1.0];
                        [
                            base[0] * (1.0 - k) + rock[0] * k,
                            base[1] * (1.0 - k) + rock[1] * k,
                            base[2] * (1.0 - k) + rock[2] * k,
                            1.0,
                        ]
                    } else {
                        base
                    }
                }
                _ => Self::biome_color(biome),
            }
        }
    }

    /// Palette de couleurs de base selon le biome (utilisée comme repli)
    pub fn biome_color(biome: BiomeType) -> [f32; 4] {
        match biome {
            // Zones maritimes (Océans et Mers)
            BiomeType::DeepOcean => [0.03, 0.10, 0.38, 1.0], // Bleu abyssal profond
            BiomeType::Ocean => [0.07, 0.28, 0.68, 1.0],     // Bleu océan intense
            BiomeType::Sea => [0.10, 0.54, 0.82, 1.0],       // Bleu lagon / azur mer intérieure
            // Zones côtières et terrestres
            BiomeType::Beach => [0.88, 0.82, 0.56, 1.0],      // Sable doré des plages
            BiomeType::Plains => [0.28, 0.64, 0.22, 1.0],     // Plaines et prairies verdoyantes
            BiomeType::Forest => [0.14, 0.44, 0.16, 1.0],     // Forêts et forêts boréales
            BiomeType::Desert => [0.90, 0.72, 0.38, 1.0],     // Dunes et déserts chauds
            BiomeType::Mountains => [0.52, 0.50, 0.48, 1.0],  // Roches grises des massifs
            BiomeType::SnowPeaks => [0.96, 0.97, 1.00, 1.0],  // Sommets enneigés et calottes polaires
            BiomeType::Tundra => [0.56, 0.60, 0.48, 1.0],     // Toundra rase, vert-de-gris givré
            BiomeType::Taiga => [0.12, 0.33, 0.22, 1.0],      // Conifères vert sombre bleuté
            BiomeType::Savanna => [0.68, 0.62, 0.30, 1.0],    // Herbes sèches jaune paille
            BiomeType::Jungle => [0.04, 0.47, 0.20, 1.0],     // Canopée tropicale vert émeraude
        }
    }

    /// Calcule la couleur avec transparence pour le filtre tectonique au-dessus du relief naturel
    pub fn blend_tectonic_color(base_color: [f32; 4], sample: &herocraft_world::TectonicSample) -> [f32; 4] {
        // Sur les lignes de failles (< 0.12), l'accent est marqué (80%),
        // au cœur des plaques, transparence douce (35%) pour laisser voir les continents, mers et biomes
        let alpha = if sample.dist_to_boundary < 0.12 {
            0.80_f32
        } else {
            0.35_f32
        };
        [
            base_color[0] * (1.0 - alpha) + sample.debug_color[0] * alpha,
            base_color[1] * (1.0 - alpha) + sample.debug_color[1] * alpha,
            base_color[2] * (1.0 - alpha) + sample.debug_color[2] * alpha,
            1.0,
        ]
    }

    /// Construit le maillage d'un patch LOD sous forme de colonnes voxels avec falaises à 90°
    pub fn generate_lod_patch(
        generator: &PlanetWorldGenerator,
        key: &herocraft_world::QuadNodeKey,
        base_radius: f32,
        grid_resolution: usize,
        filter: herocraft_world::VisualizationFilter,
    ) -> Self {
        Self::generate_lod_patch_stepped(generator, key, base_radius, PatchDetail::plain(grid_resolution, Self::ORBIT_TERRACE_STEP_HEIGHT), filter)
    }

    /// Plus haut bloc posé par une ville, un village, des ruines (si `settlements`) ou un chemin
    /// (si `roads`) dans la colonne (x, z) d'une face : altitude de son dessus et couleur de sa
    /// face du haut. `None` : le terrain naturel reste.
    fn structure_top(
        generator: &PlanetWorldGenerator,
        face: PlanetFace,
        x: i32,
        z: i32,
        elevation: f64,
        settlements: bool,
        roads: bool,
    ) -> Option<(i32, BlockId)> {
        let ground = elevation.floor() as i32;
        if settlements {
            if let Some(s) = generator.settlement_at(face, x, z) {
                let span = s.span(x, z, ground);
                if !span.is_empty() {
                    for y in span.rev() {
                        match s.block(x, y, z, ground) {
                            Some(b) if !b.is_air() => {
                                return Some((y + 1, b));
                            }
                            _ => {}
                        }
                    }
                    // Le plan ne fait que creuser cette colonne (une cour, une rue) : sol naturel
                    return None;
                }
            }
        }
        if roads {
            if let Some(b) = generator.roads.surface(generator, face, x, z) {
                return Some((ground, b));
            }
        }
        None
    }

    /// Construit le maillage d'un patch selon `detail` : grille, terrasses, textures des blocs et
    /// arbres. Au plus fin (une cellule par bloc, textures, arbres en blocs), un patch a l'allure
    /// des chunks voxel, sans grottes ni plantes.
    pub fn generate_lod_patch_stepped(
        generator: &PlanetWorldGenerator,
        key: &herocraft_world::QuadNodeKey,
        base_radius: f32,
        detail: PatchDetail,
        filter: herocraft_world::VisualizationFilter,
    ) -> Self {
        herocraft_common::prof_scope!("mesh::generate_lod_patch");
        let n = detail.resolution.max(2);
        let terrace_step = detail.terrace_step;
        let mut vertices: Vec<PlanetVertex> = Vec::with_capacity(n * n * 8);
        let mut indices: Vec<u32> = Vec::with_capacity(n * n * 12);

        let (u_min, u_max, v_min, v_max) = key.uv_bounds();
        let du = (u_max - u_min) / (n as f64);
        let dv = (v_max - v_min) / (n as f64);
        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f64)) as f32;
        let block_uv = 2.0 / herocraft_world::FACE_BLOCKS as f64;
        let standard = filter == herocraft_world::VisualizationFilter::Standard;
        let precise = terrace_step <= 1.0 && standard;
        let textured = detail.textured && precise;

        // Côté d'une cellule en blocs : les villes n'apparaissent que si elle est assez fine pour
        // que leurs bâtiments (5 à 10 blocs) ne deviennent pas des pavés, les chemins (1 à 3 blocs)
        // seulement près du bloc
        let cell_blocks = du / block_uv;
        let show_settlements = precise && cell_blocks <= MAX_SETTLEMENT_CELL_BLOCKS;
        let show_roads = precise && cell_blocks <= MAX_ROAD_CELL_BLOCKS;

        let sample_cell = |ci: i32, cj: i32| -> LodCell {
            let u_mid = u_min + ((ci as f64) + 0.5) * du;
            let v_mid = v_min + ((cj as f64) + 0.5) * dv;
            let sphere_dir: DVec3 = key.face.uv_to_sphere_direction(u_mid, v_mid);
            let (lat, _lon) = unit_sphere_to_lat_lon(sphere_dir);
            let (raw_elevation, biome) = generator.sample_elevation_and_biome(sphere_dir, lat);
            let is_water = biome.is_water();
            let stepped = if is_water {
                SEA_LEVEL_BLOCKS
            } else {
                Self::quantize_elevation(raw_elevation, terrace_step)
            };
            let to_block = |t: f64| ((t + 1.0) / block_uv).floor() as i32;
            let (bx, bz) = (to_block(u_mid), to_block(v_mid));
            // Ville, village ou chemin sous le centre de la cellule : dessus et matière du plus
            // haut bloc qu'ils posent dans cette colonne
            if (show_settlements || show_roads) && !is_water {
                if let Some((top, block)) = Self::structure_top(generator, key.face, bx, bz, raw_elevation, show_settlements, show_roads) {
                    let color = crate::voxel_mesh::VoxelMeshBuilder::block_color(block, 2);
                    let material = textured.then_some(block);
                    return LodCell { height: top as f64, water: false, color, top: material, under: material };
                }
            }
            let color = match filter {
                herocraft_world::VisualizationFilter::Standard => Self::relief_biome_color(biome, stepped),
                herocraft_world::VisualizationFilter::Tectonics => {
                    let base_color = Self::relief_biome_color(biome, stepped);
                    let sample = generator.tectonics.sample(sphere_dir);
                    Self::blend_tectonic_color(base_color, &sample)
                }
                herocraft_world::VisualizationFilter::OceanCurrents => {
                    let continental = generator.sample_continental_value(sphere_dir);
                    generator.ocean_currents.sample(sphere_dir, is_water, continental).color
                }
            };
            // Matière du dessus du sol (herbe, sable, neige, roche) et celle qu'elle recouvre,
            // pour les textures
            let (top, under) = if textured && !is_water {
                let column = herocraft_world::ColumnSample { dir: sphere_dir, lat, elevation: raw_elevation, biome };
                match generator.surface_block(&column, key.face, bx, bz) {
                    Some(b) => (Some(b), Some(subsoil(b))),
                    None => (None, None),
                }
            } else {
                (None, None)
            };
            LodCell { height: stepped, water: is_water, color, top, under }
        };

        // Pré-échantillonnage de la grille n x n
        let mut grid_cells = Vec::with_capacity(n * n);
        {
            herocraft_common::prof_scope!("mesh::lod_patch::sampling");
            for j in 0..n {
                for i in 0..n {
                    grid_cells.push(sample_cell(i as i32, j as i32));
                }
            }
        }
        herocraft_common::prof_scope!("mesh::lod_patch::geometry");

        // Helper pour ajouter un quad (4 sommets + 2 triangles CCW)
        let add_quad = |verts: &mut Vec<PlanetVertex>, inds: &mut Vec<u32>,
                        p: [Vec3; 4],
                        normal: Vec3, color: [f32; 4], texture: [u32; 4]| {
            let base_idx = verts.len() as u32;
            let norm_arr = normal.to_array();
            for k in 0..4 {
                verts.push(PlanetVertex { position: p[k].to_array(), normal: norm_arr, color, texture: texture[k], light: 0.0 });
            }
            inds.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
        };
        let untextured = [PlanetVertex::UNTEXTURED; 4];
        // Texture d'un mur : u le long du bord, v vers le bas
        let wall_texture = |block: BlockId, dir: usize| {
            let layer = herocraft_common::blocks().face_texture(block, dir);
            [(0, 0), (0, 16), (16, 16), (16, 0)].map(|(u, v)| PlanetVertex::pack_texture(layer, u, v))
        };
        let radius = |h: f64| base_radius + (h as f32) * altitude_scale;
        // Mur vertical entre les directions a et b (unitaires), du haut `h_top` au bas `h_bot` (en
        // blocs) ; `dir` : sa direction dans `FACE_DIRS`. Avec textures : un bloc de la matière du
        // dessus (le flanc de l'herbe), puis celle d'en dessous.
        let emit_wall = |verts: &mut Vec<PlanetVertex>, inds: &mut Vec<u32>, a: Vec3, b: Vec3, h_top: f64, h_bot: f64, cell: &LodCell, dir: usize| {
            let quad = |hi: f64, lo: f64| [a * radius(hi), a * radius(lo), b * radius(lo), b * radius(hi)];
            let top = quad(h_top, h_bot);
            let normal = (top[3] - top[0]).cross(top[0] - top[1]).normalize_or_zero();
            match (cell.top, cell.under) {
                (Some(top_block), Some(under_block)) => {
                    let shade = |block: BlockId| {
                        let c = crate::voxel_mesh::VoxelMeshBuilder::block_color(block, dir);
                        [c[0] * 0.78, c[1] * 0.78, c[2] * 0.78, 1.0]
                    };
                    let split = (h_top - 1.0).max(h_bot);
                    add_quad(verts, inds, quad(h_top, split), normal, shade(top_block), wall_texture(top_block, dir));
                    if split > h_bot {
                        add_quad(verts, inds, quad(split, h_bot), normal, shade(under_block), wall_texture(under_block, dir));
                    }
                }
                _ => {
                    let c = cell.color;
                    add_quad(verts, inds, top, normal, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.55, c[3]], untextured);
                }
            }
        };

        let skirt_depth = (base_radius * 0.015).max(0.25);

        for j in 0..n {
            let v0 = v_min + (j as f64) * dv;
            let v1 = v_min + ((j + 1) as f64) * dv;

            for i in 0..n {
                let u0 = u_min + (i as f64) * du;
                let u1 = u_min + ((i + 1) as f64) * du;

                let cell = grid_cells[j * n + i];
                let stepped = cell.height;
                let color = cell.color;

                // 4 directions des coins de la cellule sur la sphère
                let to_vec = |d: DVec3| Vec3::new(d.x as f32, d.y as f32, d.z as f32);
                let v00 = to_vec(key.face.uv_to_sphere_direction(u0, v0));
                let v10 = to_vec(key.face.uv_to_sphere_direction(u1, v0));
                let v11 = to_vec(key.face.uv_to_sphere_direction(u1, v1));
                let v01 = to_vec(key.face.uv_to_sphere_direction(u0, v1));

                // 1. Le dessus de la colonne est posé plus bas, par rangées (voir `MAX_MERGED_TOPS`)
                let cliff_color = [color[0] * 0.55, color[1] * 0.55, color[2] * 0.55, color[3]];

                // 2. Murs verticaux à 90° vers les voisins plus bas (+U, -U, +V, -V)
                let neighbor = |di: i32, dj: i32| -> f64 {
                    let (ni, nj) = (i as i32 + di, j as i32 + dj);
                    if (0..n as i32).contains(&ni) && (0..n as i32).contains(&nj) {
                        grid_cells[nj as usize * n + ni as usize].height
                    } else {
                        sample_cell(ni, nj).height
                    }
                };
                let h_east = neighbor(1, 0);
                let h_west = neighbor(-1, 0);
                let h_north = neighbor(0, 1);
                let h_south = neighbor(0, -1);
                if stepped > h_east {
                    emit_wall(&mut vertices, &mut indices, v10, v11, stepped, h_east, &cell, 0);
                }
                if stepped > h_west {
                    emit_wall(&mut vertices, &mut indices, v01, v00, stepped, h_west, &cell, 1);
                }
                if stepped > h_north {
                    emit_wall(&mut vertices, &mut indices, v11, v01, stepped, h_north, &cell, 4);
                }
                if stepped > h_south {
                    emit_wall(&mut vertices, &mut indices, v00, v10, stepped, h_south, &cell, 5);
                }

                // Jupes de bordure extérieure (Skirts) pour garantir l'absence de fissure entre niveaux de LOD
                let mut skirt = |a: Vec3, b: Vec3, h_other: f64| {
                    let r_base = radius(stepped.min(h_other));
                    let q = [a * r_base, a * (r_base - skirt_depth), b * (r_base - skirt_depth), b * r_base];
                    let normal = (q[3] - q[0]).cross(q[0] - q[1]).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, q, normal, cliff_color, untextured);
                };
                if i == 0 {
                    skirt(v01, v00, h_west);
                }
                if i == n - 1 {
                    skirt(v10, v11, h_east);
                }
                if j == 0 {
                    skirt(v00, v10, h_south);
                }
                if j == n - 1 {
                    skirt(v11, v01, h_north);
                }
            }
        }

        // Dessus des colonnes : les cellules voisines d'une rangée de même hauteur et même matière
        // forment un seul quad, qui répète la texture une fois par cellule
        {
            herocraft_common::prof_scope!("mesh::lod_patch::tops");
            let to_vec = |d: DVec3| Vec3::new(d.x as f32, d.y as f32, d.z as f32);
            // Au plus 15 blocs : la texture répétée tient sur 8 bits, et le quad plat ne s'écarte
            // de la courbure que d'un centième de bloc
            let max_run = ((MAX_MERGED_BLOCKS / cell_blocks) as usize).clamp(1, MAX_MERGED_BLOCKS as usize);
            let same = |a: &LodCell, b: &LodCell| a.height == b.height && a.water == b.water && a.top == b.top && a.color == b.color;
            for j in 0..n {
                let v0 = v_min + (j as f64) * dv;
                let v1 = v_min + ((j + 1) as f64) * dv;
                let mut i = 0;
                while i < n {
                    let start = i;
                    let cell = grid_cells[j * n + i];
                    let max_run = max_run;
                    while i + 1 < n && i + 1 - start < max_run && same(&grid_cells[j * n + i + 1], &cell) {
                        i += 1;
                    }
                    let run = i + 1 - start;
                    let (u0, u1) = (u_min + start as f64 * du, u_min + (i + 1) as f64 * du);
                    let d = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)].map(|(u, v)| to_vec(key.face.uv_to_sphere_direction(u, v)));
                    let r_top = radius(cell.height);
                    let texture = match cell.top {
                        Some(block) => {
                            let layer = herocraft_common::blocks().face_texture(block, 2);
                            let len = (16 * run) as u8;
                            [(0, 0), (len, 0), (len, 16), (0, 16)].map(|(u, v)| PlanetVertex::pack_texture(layer, u, v))
                        }
                        None => untextured,
                    };
                    let normal = (d[0] + d[1] + d[2] + d[3]).normalize();
                    add_quad(&mut vertices, &mut indices, d.map(|x| x * r_top), normal, cell.color, texture);
                    i += 1;
                }
            }
        }

        // 3. Arbres dont le tronc est dans le patch, posés sur la cellule sous leur tronc
        if detail.trees != TreeDetail::None && standard {
            herocraft_common::prof_scope!("mesh::lod_patch::trees");
            let to_block = |t: f64| ((t + 1.0) / block_uv).round() as i32;
            let (x0, x1, z0, z1) = (to_block(u_min), to_block(u_max), to_block(v_min), to_block(v_max));
            let field = generator.vegetation_field(key.face, x0..x1, z0..z1, None);
            let corner = |x: f64, y: f64, z: f64| -> Vec3 {
                let d = key.face.uv_to_sphere_direction(-1.0 + x * block_uv, -1.0 + z * block_uv);
                Vec3::new(d.x as f32, d.y as f32, d.z as f32) * radius(y)
            };
            for tree in field.trees() {
                if !(x0..x1).contains(&tree.x) || !(z0..z1).contains(&tree.z) {
                    continue;
                }
                let ci = (((tree.x - x0) as f64 / cell_blocks) as usize).min(n - 1);
                let cj = (((tree.z - z0) as f64 / cell_blocks) as usize).min(n - 1);
                let cell = &grid_cells[cj * n + ci];
                if cell.water || cell.top.is_none() && textured {
                    continue;
                }
                let lift = cell.height - tree.ground as f64;
                match detail.trees {
                    TreeDetail::Blocks => emit_tree_blocks(&mut vertices, &mut indices, tree, lift, &corner),
                    _ => emit_tree_boxes(&mut vertices, &mut indices, tree, lift, &corner),
                }
            }
        }

        PlanetMesh { vertices, indices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les patchs fins montrent les villes : une colonne de rempart ou de maison dépasse le sol
    #[test]
    fn structure_top_sees_town_buildings() {
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 42, ..Default::default() });
        let face = PlanetFace::PosZ;
        let regions = herocraft_world::FACE_BLOCKS / 2048;
        let town = (0..regions * regions)
            .filter_map(|r| generator.structures.region_settlement(&generator, face, r % regions, r / regions))
            .next()
            .expect("un établissement sur la face");
        let b = town.bounds;
        let raised = (b.z0..b.z1).step_by(2).flat_map(|z| (b.x0..b.x1).step_by(2).map(move |x| (x, z))).any(|(x, z)| {
            let column = generator.sample_block_column(face, x, z);
            let ground = column.elevation.floor() as i32;
            PlanetMesh::structure_top(&generator, face, x, z, column.elevation, true, false).is_some_and(|(top, _)| top > ground)
        });
        assert!(raised, "au moins un bâtiment doit dépasser du sol");
    }

    /// Vue de dessus des patchs au-dessus d'une ville : ancienne précision normale, fine et 1:1.
    /// `HEROCRAFT_CAPTURES=dossier cargo test --release -p herocraft_client apercu_villes_lod -- --ignored`
    #[test]
    #[ignore]
    fn apercu_villes_lod() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let seed: u64 = std::env::var("HEROCRAFT_GRAINE").ok().and_then(|s| s.parse().ok()).unwrap_or(42);
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed, ..Default::default() });
        let face = PlanetFace::PosZ;
        let regions = herocraft_world::FACE_BLOCKS / 2048;
        let town = (0..regions * regions)
            .filter_map(|r| generator.structures.region_settlement(&generator, face, r % regions, r / regions))
            .find(|s| s.kind == herocraft_world::SettlementKind::Town)
            .expect("une ville sur la face");
        // Nœud de 128 blocs de côté centré au mieux sur la ville
        let level = herocraft_world::VOXEL_LEVEL - 2;
        let side = herocraft_world::FACE_BLOCKS >> level;
        let key = herocraft_world::QuadNodeKey::new(face, level, (town.center.0 / side) as u32, (town.center.1 / side) as u32);
        let configs = [PatchDetail::plain(16, PlanetMesh::ORBIT_TERRACE_STEP_HEIGHT), PatchDetail { textured: true, trees: TreeDetail::Boxes, ..PatchDetail::plain(side as usize / 2, 1.0) }, PatchDetail { textured: true, trees: TreeDetail::Blocks, ..PatchDetail::plain(side as usize, 1.0) }];
        let px = 4usize;
        let panel = side as usize * px;
        let (w, h) = (panel * configs.len() + 8 * (configs.len() - 1), panel);
        let mut img = vec![30u8; w * h * 3];
        let (u0, u1, v0, v1) = key.uv_bounds();
        for (n, &detail) in configs.iter().enumerate() {
            let mesh = PlanetMesh::generate_lod_patch_stepped(&generator, &key, 1.0, detail, herocraft_world::VisualizationFilter::Standard);
            let ox = n * (panel + 8);
            let base = 1.0f32;
            for quad in mesh.vertices.chunks(4) {
                let c = (Vec3::from(quad[0].position) + Vec3::from(quad[2].position)) * 0.5;
                let up = c.normalize();
                if Vec3::from(quad[0].normal).dot(up) < 0.95 {
                    continue;
                }
                let (Some((a, b)), Some((e, f))) = (
                    face.project_to_face_uv(Vec3::from(quad[0].position).as_dvec3().normalize()),
                    face.project_to_face_uv(Vec3::from(quad[2].position).as_dvec3().normalize()),
                ) else { continue };
                let to_px = |t: f64, lo: f64, hi: f64| (((t - lo) / (hi - lo)) * panel as f64).round().clamp(0.0, panel as f64) as usize;
                let (x0, x1) = (to_px(a.min(e), u0, u1), to_px(a.max(e), u0, u1));
                let (y0, y1) = (to_px(b.min(f), v0, v1), to_px(b.max(f), v0, v1));
                let height = (c.length() - base) / (base * std::f32::consts::FRAC_PI_4 * 2.0 / herocraft_world::FACE_BLOCKS as f32);
                let shade = (0.55 + (height - 100.0) / 120.0).clamp(0.45, 1.15);
                let col = quad[0].color;
                let rgb = [0, 1, 2].map(|k| ((col[k] * shade).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = ((h - 1 - y) * w + ox + x) * 3;
                        img[i..i + 3].copy_from_slice(&rgb);
                    }
                }
            }
        }
        std::fs::create_dir_all(&dir).unwrap();
        let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("villes-lod-{seed}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }

    #[test]
    fn test_packed_vertex_round_trip() {
        assert_eq!(std::mem::size_of::<PackedAttributes>(), 12);
        let v = PlanetVertex { position: [1.0, 2.0, 3.0], normal: [0.6, -0.8, 0.0], color: [0.02, 0.5, 1.0, -0.5], texture: 7, light: 0.0 };
        let p = v.packed_attributes();
        // Décodage du GPU : snorm8 = c / 127, unorm8 = c / 255 (puis au carré pour la couleur)
        let n: Vec<f32> = p.normal_alpha.iter().map(|&c| (c as f32 / 127.0).max(-1.0)).collect();
        let c: Vec<f32> = p.color.iter().map(|&c| (c as f32 / 255.0).powi(2)).collect();
        assert!((n[0] - 0.6).abs() < 0.005 && (n[1] + 0.8).abs() < 0.005 && n[2] == 0.0);
        assert!((n[3] + 0.5).abs() < 0.01, "opacité négative du fondu sortant gardée : {}", n[3]);
        // Ton sombre : erreur relative faible grâce à la racine
        assert!((c[0] - 0.02).abs() / 0.02 < 0.05, "{}", c[0]);
        assert!((c[1] - 0.5).abs() < 0.005 && c[2] == 1.0);
        assert_eq!(p.texture, 7);
        // Opaque : exactement 1, que le shader distingue d'un fondu (seuil 0.999)
        let opaque = PlanetVertex { color: [0.3, 0.3, 0.3, 1.0], ..v }.packed_attributes();
        assert_eq!(opaque.normal_alpha[3], 127);
    }

    #[test]
    fn test_short_indices_when_possible() {
        let short = PackedIndices::new(&[0, 1, 65535], 65536);
        assert_eq!((short.bytes().len(), short.format()), (6, wgpu::IndexFormat::Uint16));
        assert_eq!(bytemuck::cast_slice::<u8, u16>(short.bytes()), &[0, 1, 65535]);
        let long = PackedIndices::new(&[0, 70000], 70001);
        assert_eq!((long.bytes().len(), long.format()), (8, wgpu::IndexFormat::Uint32));
        // La taille annoncée avant le tassage est celle envoyée
        let vertex = PlanetVertex { position: [0.0; 3], normal: [0.0, 1.0, 0.0], color: [1.0; 4], texture: 0, light: 0.0 };
        let mesh = PlanetMesh { vertices: vec![vertex; 4], indices: vec![0, 1, 2, 2, 3, 0] };
        let data = GpuMeshData::new(&mesh);
        let sent = data.positions.len() * 12 + data.attributes.len() * 12 + data.indices.bytes().len();
        assert_eq!((sent, GpuMeshData::upload_size(&mesh)), (4 * 24 + 6 * 2, 4 * 24 + 6 * 2));
    }

    #[test]
    fn test_relief_biome_color_trench_vs_shallow_water() {
        let shallow = PlanetMesh::relief_biome_color(BiomeType::Sea, 98.0);
        let mid_ocean = PlanetMesh::relief_biome_color(BiomeType::Ocean, 65.0);
        let trench = PlanetMesh::relief_biome_color(BiomeType::DeepOcean, 20.0);

        // La fosse abyssale doit être nettement plus sombre que le lagon côtier
        let brightness_shallow = shallow[0] * 0.299 + shallow[1] * 0.587 + shallow[2] * 0.114;
        let brightness_trench = trench[0] * 0.299 + trench[1] * 0.587 + trench[2] * 0.114;
        assert!(
            brightness_trench < brightness_shallow * 0.35,
            "La fosse abyssale ({:?}) doit être profondément plus sombre que l'eau côtière ({:?})",
            trench,
            shallow
        );

        // Décroissance continue de clarté
        assert!(shallow[1] > mid_ocean[1]);
        assert!(mid_ocean[2] > trench[2]);
    }

    #[test]
    fn test_relief_biome_color_mountains_progression() {
        let foothills = PlanetMesh::relief_biome_color(BiomeType::Mountains, 185.0);
        let high_ridge = PlanetMesh::relief_biome_color(BiomeType::Mountains, 265.0);
        let snow_summit = PlanetMesh::relief_biome_color(BiomeType::SnowPeaks, 345.0);

        // La base de la montagne est sombre (ardoise), la crête est en granite clair, le sommet est blanc éclatant
        assert!(foothills[0] < high_ridge[0]);
        assert!(high_ridge[0] < snow_summit[0]);
        assert!(snow_summit[0] > 0.95 && snow_summit[1] > 0.95 && snow_summit[2] > 0.95);
    }

    #[test]
    fn test_snowline_at_300() {
        let just_below = PlanetMesh::relief_biome_color(BiomeType::Mountains, 298.0);
        let at_snow = PlanetMesh::relief_biome_color(BiomeType::Mountains, 305.0);

        // Sous 300 : granite / crête rocheuse (< 0.70 de composante rouge/vert)
        assert!(just_below[0] < 0.70);
        // Au-dessus de 300 : enneigement automatique (> 0.85 de clarté éclatante)
        assert!(at_snow[0] > 0.85 && at_snow[1] > 0.85 && at_snow[2] > 0.85);
    }

    #[test]
    fn test_vertical_walls_are_orthogonal() {
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        });
        let key = herocraft_world::QuadNodeKey::new(PlanetFace::PosZ, 2, 1, 1);
        let mesh = PlanetMesh::generate_lod_patch(&generator, &key, 50.0, 8, herocraft_world::VisualizationFilter::Standard);

        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);

        // Vérifier que chaque triangle a une normale valide et que les parois de falaises
        // sont orthogonales au vecteur radial (verticalité proche de 0.0)
        let mut found_vertical_cliff = false;
        for chunk in mesh.indices.chunks_exact(3) {
            let v0 = Vec3::from(mesh.vertices[chunk[0] as usize].position);
            let v1 = Vec3::from(mesh.vertices[chunk[1] as usize].position);
            let v2 = Vec3::from(mesh.vertices[chunk[2] as usize].position);
            let radial = ((v0 + v1 + v2) / 3.0).normalize();
            let face_normal = (v1 - v0).cross(v2 - v0).normalize_or_zero();

            // Sur une paroi verticale à 90°, face_normal est perpendiculaire au vecteur radial (dot ≈ 0)
            let verticality = face_normal.dot(radial).abs();
            if verticality < 0.15 {
                found_vertical_cliff = true;
                assert!(verticality < 0.10, "Une paroi verticale doit être à ~90° du rayon");
            }
        }
        assert!(found_vertical_cliff, "Le maillage doit contenir des falaises verticales à 90°");
    }
}

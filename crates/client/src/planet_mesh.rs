use glam::{DVec3, Vec3};
use herocraft_common::PlanetFace;
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

/// Maillage indexé prêt pour WGPU
#[derive(Clone, Default)]
pub struct PlanetMesh {
    pub vertices: Vec<PlanetVertex>,
    pub indices: Vec<u32>,
}

impl PlanetMesh {
    /// Hauteur d'un palier pour le relief orbital étagé en strates / terrasses (en blocs)
    pub const ORBIT_TERRACE_STEP_HEIGHT: f64 = 6.0;

    /// Discrétise une altitude continue en terrasses géologiques étagées pour la vue orbitale
    #[inline]
    pub fn quantize_terrace_elevation(elevation: f64) -> f64 {
        if elevation <= SEA_LEVEL_BLOCKS {
            elevation
        } else {
            let diff = elevation - SEA_LEVEL_BLOCKS;
            let step = (diff / Self::ORBIT_TERRACE_STEP_HEIGHT).floor() * Self::ORBIT_TERRACE_STEP_HEIGHT;
            SEA_LEVEL_BLOCKS + step
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
        herocraft_common::prof_scope!("mesh::generate_lod_patch");
        let n = grid_resolution.max(2);
        let mut vertices: Vec<PlanetVertex> = Vec::with_capacity(n * n * 8);
        let mut indices: Vec<u32> = Vec::with_capacity(n * n * 12);

        let (u_min, u_max, v_min, v_max) = key.uv_bounds();
        let du = (u_max - u_min) / (n as f64);
        let dv = (v_max - v_min) / (n as f64);
        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f64)) as f32;

        let sample_cell = |ci: i32, cj: i32| -> (f64, bool, [f32; 4]) {
            let u_mid = u_min + ((ci as f64) + 0.5) * du;
            let v_mid = v_min + ((cj as f64) + 0.5) * dv;
            let sphere_dir: DVec3 = key.face.uv_to_sphere_direction(u_mid, v_mid);
            let (lat, _lon) = unit_sphere_to_lat_lon(sphere_dir);
            let (raw_elevation, biome) = generator.sample_elevation_and_biome(sphere_dir, lat);
            let is_water = biome.is_water();
            let stepped = if is_water {
                SEA_LEVEL_BLOCKS
            } else {
                Self::quantize_terrace_elevation(raw_elevation)
            };
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
            (stepped, is_water, color)
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
                        p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3,
                        normal: Vec3, color: [f32; 4]| {
            let base_idx = verts.len() as u32;
            let norm_arr = normal.to_array();
            verts.push(PlanetVertex { position: p0.to_array(), normal: norm_arr, color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            verts.push(PlanetVertex { position: p1.to_array(), normal: norm_arr, color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            verts.push(PlanetVertex { position: p2.to_array(), normal: norm_arr, color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });
            verts.push(PlanetVertex { position: p3.to_array(), normal: norm_arr, color, texture: PlanetVertex::UNTEXTURED, light: 0.0 });

            // Triangle 1 CCW: (0, 1, 2)
            inds.push(base_idx);
            inds.push(base_idx + 1);
            inds.push(base_idx + 2);

            // Triangle 2 CCW: (0, 2, 3)
            inds.push(base_idx);
            inds.push(base_idx + 2);
            inds.push(base_idx + 3);
        };

        let skirt_depth = (base_radius * 0.015).max(0.25);

        for j in 0..n {
            let v0 = v_min + (j as f64) * dv;
            let v1 = v_min + ((j + 1) as f64) * dv;

            for i in 0..n {
                let u0 = u_min + (i as f64) * du;
                let u1 = u_min + ((i + 1) as f64) * du;

                let (stepped, _is_water, color) = grid_cells[j * n + i];
                let r_top = base_radius + (stepped as f32) * altitude_scale;

                // 4 directions des coins de la cellule sur la sphère
                let d00 = key.face.uv_to_sphere_direction(u0, v0);
                let d10 = key.face.uv_to_sphere_direction(u1, v0);
                let d11 = key.face.uv_to_sphere_direction(u1, v1);
                let d01 = key.face.uv_to_sphere_direction(u0, v1);

                let v00 = Vec3::new(d00.x as f32, d00.y as f32, d00.z as f32);
                let v10 = Vec3::new(d10.x as f32, d10.y as f32, d10.z as f32);
                let v11 = Vec3::new(d11.x as f32, d11.y as f32, d11.z as f32);
                let v01 = Vec3::new(d01.x as f32, d01.y as f32, d01.z as f32);

                let v_center = (v00 + v10 + v11 + v01).normalize();

                // 1. Face supérieure horizontale de la colonne (plateau plat à hauteur H)
                let p00_top = v00 * r_top;
                let p10_top = v10 * r_top;
                let p11_top = v11 * r_top;
                let p01_top = v01 * r_top;

                add_quad(&mut vertices, &mut indices, p00_top, p10_top, p11_top, p01_top, v_center, color);

                // Couleur de roche / falaise pour les murs verticaux
                let cliff_color = [color[0] * 0.55, color[1] * 0.55, color[2] * 0.55, color[3]];

                // 2. Murs verticaux à 90° (les falaises perpendiculaires aux plateaux)

                // Voisin Est (+U)
                let (h_east, _, _) = if i + 1 < n {
                    grid_cells[j * n + (i + 1)]
                } else {
                    sample_cell(i as i32 + 1, j as i32)
                };
                if stepped > h_east {
                    let r_bot = base_radius + (h_east as f32) * altitude_scale;
                    let p10_bot = v10 * r_bot;
                    let p11_bot = v11 * r_bot;
                    let wall_norm = (p11_top - p10_top).cross(p10_top - p10_bot).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p10_top, p10_bot, p11_bot, p11_top, wall_norm, cliff_color);
                }

                // Voisin Ouest (-U)
                let (h_west, _, _) = if i > 0 {
                    grid_cells[j * n + (i - 1)]
                } else {
                    sample_cell(i as i32 - 1, j as i32)
                };
                if stepped > h_west {
                    let r_bot = base_radius + (h_west as f32) * altitude_scale;
                    let p01_bot = v01 * r_bot;
                    let p00_bot = v00 * r_bot;
                    let wall_norm = (p00_top - p01_top).cross(p01_top - p01_bot).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p01_top, p01_bot, p00_bot, p00_top, wall_norm, cliff_color);
                }

                // Voisin Nord (+V)
                let (h_north, _, _) = if j + 1 < n {
                    grid_cells[(j + 1) * n + i]
                } else {
                    sample_cell(i as i32, j as i32 + 1)
                };
                if stepped > h_north {
                    let r_bot = base_radius + (h_north as f32) * altitude_scale;
                    let p11_bot = v11 * r_bot;
                    let p01_bot = v01 * r_bot;
                    let wall_norm = (p01_top - p11_top).cross(p11_top - p11_bot).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p11_top, p11_bot, p01_bot, p01_top, wall_norm, cliff_color);
                }

                // Voisin Sud (-V)
                let (h_south, _, _) = if j > 0 {
                    grid_cells[(j - 1) * n + i]
                } else {
                    sample_cell(i as i32, j as i32 - 1)
                };
                if stepped > h_south {
                    let r_bot = base_radius + (h_south as f32) * altitude_scale;
                    let p00_bot = v00 * r_bot;
                    let p10_bot = v10 * r_bot;
                    let wall_norm = (p10_top - p00_top).cross(p00_top - p00_bot).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p00_top, p00_bot, p10_bot, p10_top, wall_norm, cliff_color);
                }

                // Jupes de bordure extérieure (Skirts) pour garantir l'absence de fissure entre niveaux de LOD
                if i == 0 {
                    let r_base_west = base_radius + (stepped.min(h_west) as f32) * altitude_scale;
                    let p_top_a = v01 * r_base_west;
                    let p_top_b = v00 * r_base_west;
                    let p_bot_a = v01 * (r_base_west - skirt_depth);
                    let p_bot_b = v00 * (r_base_west - skirt_depth);
                    let skirt_norm = (p_top_b - p_top_a).cross(p_top_a - p_bot_a).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p_top_a, p_bot_a, p_bot_b, p_top_b, skirt_norm, cliff_color);
                }
                if i == n - 1 {
                    let r_base_east = base_radius + (stepped.min(h_east) as f32) * altitude_scale;
                    let p_top_a = v10 * r_base_east;
                    let p_top_b = v11 * r_base_east;
                    let p_bot_a = v10 * (r_base_east - skirt_depth);
                    let p_bot_b = v11 * (r_base_east - skirt_depth);
                    let skirt_norm = (p_top_b - p_top_a).cross(p_top_a - p_bot_a).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p_top_a, p_bot_a, p_bot_b, p_top_b, skirt_norm, cliff_color);
                }
                if j == 0 {
                    let r_base_south = base_radius + (stepped.min(h_south) as f32) * altitude_scale;
                    let p_top_a = v00 * r_base_south;
                    let p_top_b = v10 * r_base_south;
                    let p_bot_a = v00 * (r_base_south - skirt_depth);
                    let p_bot_b = v10 * (r_base_south - skirt_depth);
                    let skirt_norm = (p_top_b - p_top_a).cross(p_top_a - p_bot_a).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p_top_a, p_bot_a, p_bot_b, p_top_b, skirt_norm, cliff_color);
                }
                if j == n - 1 {
                    let r_base_north = base_radius + (stepped.min(h_north) as f32) * altitude_scale;
                    let p_top_a = v11 * r_base_north;
                    let p_top_b = v01 * r_base_north;
                    let p_bot_a = v11 * (r_base_north - skirt_depth);
                    let p_bot_b = v01 * (r_base_north - skirt_depth);
                    let skirt_norm = (p_top_b - p_top_a).cross(p_top_a - p_bot_a).normalize_or_zero();
                    add_quad(&mut vertices, &mut indices, p_top_a, p_bot_a, p_bot_b, p_top_b, skirt_norm, cliff_color);
                }
            }
        }

        PlanetMesh { vertices, indices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

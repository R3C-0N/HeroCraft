//! Textures des blocs du terrain : chaque PNG cité par une fiche devient une couche d'un tableau de
//! textures de 16 x 16 texels, avec ses niveaux de détail (mipmaps).
//!
//! Une couche ne garde pas les couleurs du PNG mais leur rapport à sa couleur moyenne : le shader
//! multiplie la couleur du sommet par ce rapport. Un sommet qui porte la couleur moyenne de la
//! texture la montre telle quelle ; un sommet qui porte la couleur du biome (dessus du relief
//! naturel) la montre teintée par le biome. De loin, la face a la couleur du sommet, la même que
//! celle du relief lointain sans texture : pas de saut de teinte au fondu entre les deux.

use std::sync::OnceLock;

use herocraft_common::{blocks, items, BlockId, ItemId};

use crate::skinned_model::Texture;

/// Côté d'une texture de bloc, en texels
pub const TEXTURE_SIZE: u32 = 16;
/// Niveaux de détail : 16, 8, 4, 2 et 1 texels
const MIP_LEVELS: u32 = 5;
/// Rapport le plus fort à la couleur moyenne que garde une couche (un octet code de 0 à 4)
const RATIO_RANGE: f32 = 4.0;

/// Une texture prête à envoyer : ses niveaux de détail et sa couleur moyenne
struct Layer {
    mips: Vec<Vec<u8>>,
    mean: [f32; 3],
}

/// Toutes les textures des blocs, dans l'ordre de `BlockRegistry::texture_files`, suivies des
/// variantes des textures connectées
pub struct BlockTextureSet {
    layers: Vec<Layer>,
    /// Pour chaque texture d'un bloc connecté (verre) : la première de ses 16 variantes, une par
    /// combinaison de bords reliés (`CONNECTED_*`)
    connected: Vec<Option<u16>>,
    /// Icônes des objets qui peuvent tomber au sol (`loot::droppable_items`), ajoutées en dernier :
    /// l'objet et sa couche
    icons: Vec<(ItemId, u16)>,
}

/// Bords d'une face reliés à un bloc du même genre (bits du masque d'une texture connectée), dans
/// le repère de la texture : gauche (u = 0), droite, haut (v = 0), bas
pub const CONNECTED_LEFT: u8 = 1;
pub const CONNECTED_RIGHT: u8 = 2;
pub const CONNECTED_TOP: u8 = 4;
pub const CONNECTED_BOTTOM: u8 = 8;
/// Cadre d'une texture connectée : un liseré sombre au bord, un clair juste dedans
const FRAME_OUTER: [f32; 3] = [0.40, 0.45, 0.50];
const FRAME_INNER: [f32; 3] = [0.80, 0.86, 0.90];

static TEXTURES: OnceLock<BlockTextureSet> = OnceLock::new();

/// Les textures des blocs, lues au premier appel (le maillage lit leurs couleurs moyennes)
pub fn block_textures() -> &'static BlockTextureSet {
    TEXTURES.get_or_init(BlockTextureSet::load)
}

impl BlockTextureSet {
    fn load() -> Self {
        let registry = blocks();
        let images: Vec<Option<Texture>> = registry
            .texture_files()
            .iter()
            .map(|file| {
                let path = registry.asset_path(file);
                Texture::load(&path).map_err(|e| log::warn!("Texture {} illisible : {e}", path.display())).ok()
            })
            .collect();
        let mut layers: Vec<Layer> = images.iter().map(|image| image.as_ref().map_or_else(Layer::flat, Layer::from_image)).collect();
        // Variantes des textures connectées, ajoutées après les textures des fiches
        let mut connected = vec![None; layers.len()];
        for sheet in registry.iter().filter(|s| s.connected) {
            for dir in 0..6 {
                let base = registry.face_texture(BlockId(sheet.numeric_id), dir);
                let Some(Some(image)) = images.get(base as usize) else { continue };
                if connected[base as usize].is_some() {
                    continue;
                }
                connected[base as usize] = Some(layers.len() as u16);
                let mean = layers[base as usize].mean;
                layers.extend((0..16).map(|mask| Layer::connected(image, mean, mask)));
            }
        }
        // Icônes des objets qui tombent au sol (butin), dessinées sur une plaque tournante
        let mut icons = Vec::new();
        for &item in herocraft_common::loot::droppable_items() {
            let Some(sheet) = items().get(item) else { continue };
            let path = items().asset_path(&sheet.icon);
            match Texture::load(&path) {
                Ok(image) => {
                    icons.push((item, layers.len() as u16));
                    layers.push(Layer::from_image(&image));
                }
                Err(e) => log::warn!("Icône {} illisible : {e}", path.display()),
            }
        }
        Self { layers, connected, icons }
    }

    /// Couche de l'icône d'un objet qui peut tomber au sol
    pub fn icon_layer(&self, item: ItemId) -> Option<u16> {
        self.icons.iter().find(|(i, _)| *i == item).map(|&(_, layer)| layer)
    }

    /// Couche à montrer pour la texture `layer` d'un bloc connecté dont les bords `mask` touchent un
    /// bloc du même genre (la texture telle quelle si elle n'a pas de variantes)
    pub fn connected_layer(&self, layer: u16, mask: u8) -> u16 {
        match self.connected.get(layer as usize) {
            Some(Some(first)) => first + (mask & 15) as u16,
            _ => layer,
        }
    }

    /// Couleur moyenne (texels visibles) de la couche
    pub fn mean(&self, layer: u16) -> Option<[f32; 3]> {
        self.layers.get(layer as usize).map(|l| l.mean)
    }

    /// Couleur d'une face (`FACE_DIRS`) qui montre sa texture telle quelle : la moyenne de la
    /// texture, teinte comprise, avec la transparence de la fiche. `None` sans texture.
    pub fn face_color(&self, block: BlockId, dir: usize) -> Option<[f32; 4]> {
        let layer = blocks().face_texture(block, dir);
        let sheet = blocks().get(block)?;
        let mean = self.mean(layer)?;
        let tint = match sheet.texture_tint {
            Some(tint) if sheet.tinted_face(dir) => tint.0,
            _ => [1.0; 4],
        };
        Some([mean[0] * tint[0], mean[1] * tint[1], mean[2] * tint[2], sheet.side_color()[3]])
    }

    /// Envoie les couches à la carte graphique : le groupe de liaison 2 du shader du terrain
    /// Genre de végétation de chaque couche, d'après le nom de sa texture : feuillus, conifères,
    /// herbe ou autre (`Foliage`). Les variantes des textures connectées (verre) sont « autre ».
    pub fn foliage_classes(&self) -> Vec<Foliage> {
        let files = blocks().texture_files();
        (0..self.layers.len()).map(|i| files.get(i).map_or(Foliage::Other, |f| Foliage::of_texture(f))).collect()
    }

    /// `foliage_classes` tassés pour le shader : 16 couches par mot de 32 bits, `FOLIAGE_WORDS` mots
    fn foliage_words(&self) -> Vec<u32> {
        let mut words = vec![0u32; FOLIAGE_WORDS];
        for (i, class) in self.foliage_classes().into_iter().enumerate().take(FOLIAGE_WORDS * 16) {
            words[i / 16] |= (class as u32) << ((i % 16) * 2);
        }
        words
    }

    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> BlockTextureBinding {
        // Au moins une couche : le shader lit le tableau même quand aucun bloc n'a de texture
        let fallback = [Layer::flat()];
        let layers: &[Layer] = if self.layers.is_empty() { &fallback } else { &self.layers };
        let size = wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: layers.len() as u32 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Block Textures"),
            size,
            mip_level_count: MIP_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (index, layer) in layers.iter().enumerate() {
            for (level, pixels) in layer.mips.iter().enumerate() {
                let side = TEXTURE_SIZE >> level;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: index as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * side), rows_per_image: Some(side) },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                );
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // Texels nets de près, fondus de loin (sans scintillement). Répétée : le dessus d'une rangée
        // de blocs semblables du relief lointain est un seul quad qui répète la texture.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Block Texture Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        // Genre de végétation de chaque couche, pour les saisons (2 bits par couche)
        let classes = self.foliage_words();
        let foliage = wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor {
            label: Some("Block Foliage Classes"),
            contents: bytemuck::cast_slice(&classes),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Block Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Block Texture Bind Group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: foliage.as_entire_binding() },
            ],
        });
        BlockTextureBinding { layout, bind_group }
    }
}

/// Mots du tableau des genres de végétation (shader.wgsl, `foliage` : 64 vec4<u32>, 4 096 couches)
const FOLIAGE_WORDS: usize = 256;

/// Genre de végétation d'une texture, que les saisons colorent (shader.wgsl, `season_color`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Foliage {
    Other = 0,
    /// Feuillus : roux à l'automne, dépouillés l'hiver
    Deciduous = 1,
    /// Conifères : restent verts, un peu plus sombres l'hiver
    Conifer = 2,
    /// Herbe : jaunit à l'automne, brunit l'hiver
    Grass = 3,
}

impl Foliage {
    /// D'après le nom du fichier de la texture
    pub fn of_texture(file: &str) -> Foliage {
        let name = file.rsplit('/').next().unwrap_or(file).to_lowercase();
        if name.contains("darkgreenleaf") || name.contains("needle") || name.contains("spruce") {
            Foliage::Conifer
        } else if name.contains("leaf") && !name.contains("redleaf") {
            Foliage::Deciduous
        } else if name.starts_with("grass.") || name.starts_with("grasssides") || name.starts_with("tallgrass") {
            Foliage::Grass
        } else {
            Foliage::Other
        }
    }
}

/// Textures des blocs sur la carte graphique
pub struct BlockTextureBinding {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl Layer {
    /// Couche neutre : la couleur du sommet seule
    fn flat() -> Self {
        let neutral = (255.0 / RATIO_RANGE).round() as u8;
        let mips = (0..MIP_LEVELS).map(|level| [neutral, neutral, neutral, 255].repeat(((TEXTURE_SIZE >> level).pow(2)) as usize)).collect();
        Self { mips, mean: [1.0; 3] }
    }

    /// Les 16 x 16 texels d'une image (la première d'une texture animée, bande d'images carrées)
    fn texels(image: &Texture) -> Vec<[f32; 4]> {
        let frame = image.width.min(image.height).max(1);
        let n = TEXTURE_SIZE as usize;
        let texel = |x: usize, y: usize| -> [f32; 4] {
            let (sx, sy) = (x as u32 * frame / TEXTURE_SIZE, y as u32 * frame / TEXTURE_SIZE);
            let i = ((sy * image.width + sx) * 4) as usize;
            std::array::from_fn(|c| image.rgba.get(i + c).copied().unwrap_or(255) as f32 / 255.0)
        };
        (0..n * n).map(|i| texel(i % n, i / n)).collect()
    }

    fn from_image(image: &Texture) -> Self {
        let texels = Self::texels(image);
        let visible: Vec<&[f32; 4]> = texels.iter().filter(|t| t[3] >= 0.5).collect();
        let mean = if visible.is_empty() {
            [1.0; 3]
        } else {
            std::array::from_fn(|c| (visible.iter().map(|t| t[c]).sum::<f32>() / visible.len() as f32).max(1.0 / 255.0))
        };
        Self::from_texels(&texels, mean)
    }

    /// Variante connectée d'une texture (verre) : le dedans transparent, sauf ses reflets, et un
    /// cadre de deux texels sur chaque bord qui ne touche pas un bloc du même genre (`mask`). Le
    /// coin d'un cadre reste là où l'un de ses deux bords en a un.
    fn connected(image: &Texture, mean: [f32; 3], mask: u8) -> Self {
        let n = TEXTURE_SIZE as usize;
        let source = Self::texels(image);
        let framed = |bit: u8| mask & bit == 0;
        let texels: Vec<[f32; 4]> = (0..n * n)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                // Distance au bord encadré le plus proche (0 : liseré extérieur, 1 : intérieur)
                let edges = [(x, CONNECTED_LEFT), (n - 1 - x, CONNECTED_RIGHT), (y, CONNECTED_TOP), (n - 1 - y, CONNECTED_BOTTOM)];
                let depth = edges.iter().filter(|(d, bit)| *d < 2 && framed(*bit)).map(|(d, _)| *d).min();
                match depth {
                    Some(0) => [FRAME_OUTER[0], FRAME_OUTER[1], FRAME_OUTER[2], 1.0],
                    Some(_) => [FRAME_INNER[0], FRAME_INNER[1], FRAME_INNER[2], 1.0],
                    None => {
                        // Reflets : les texels du dedans plus marqués que la vitre autour
                        let t = source[i];
                        let interior = (2..n - 2).contains(&x) && (2..n - 2).contains(&y);
                        if interior && t[3] > 0.15 { [t[0], t[1], t[2], 1.0] } else { [0.0; 4] }
                    }
                }
            })
            .collect();
        Self::from_texels(&texels, mean)
    }

    /// Couche tirée de 16 x 16 texels, ses couleurs rapportées à `mean`
    fn from_texels(texels: &[[f32; 4]], mean: [f32; 3]) -> Self {
        let n = TEXTURE_SIZE as usize;
        // Découpée (fleurs, feuillages, échelle) : des texels entièrement transparents. Sinon (glace)
        // la transparence vient de la couleur de la fiche, la texture reste pleine.
        let cutout = texels.iter().any(|t| t[3] < 0.5);
        // Rapports à la moyenne et couverture (1 : texel visible), en flottants pour les mipmaps
        let mut level: Vec<[f32; 4]> = texels
            .iter()
            .map(|t| {
                let covered = if cutout { (t[3] >= 0.5) as u8 as f32 } else { 1.0 };
                [t[0] / mean[0], t[1] / mean[1], t[2] / mean[2], covered]
            })
            .collect();
        let mut mips = Vec::with_capacity(MIP_LEVELS as usize);
        let mut side = n;
        loop {
            mips.push(
                level
                    .iter()
                    .flat_map(|t| {
                        let ratio = |v: f32| ((v / RATIO_RANGE).clamp(0.0, 1.0) * 255.0).round() as u8;
                        [ratio(t[0]), ratio(t[1]), ratio(t[2]), (t[3].clamp(0.0, 1.0) * 255.0).round() as u8]
                    })
                    .collect(),
            );
            if mips.len() == MIP_LEVELS as usize {
                break;
            }
            // Niveau suivant : moyenne de 2 x 2 texels, les couleurs pondérées par leur couverture
            let half = side / 2;
            level = (0..half * half)
                .map(|i| {
                    let (x, y) = (2 * (i % half), 2 * (i / half));
                    let four = [level[y * side + x], level[y * side + x + 1], level[(y + 1) * side + x], level[(y + 1) * side + x + 1]];
                    let cover: f32 = four.iter().map(|t| t[3]).sum();
                    let color = |c: usize| if cover > 0.0 { four.iter().map(|t| t[c] * t[3]).sum::<f32>() / cover } else { 1.0 };
                    [color(0), color(1), color(2), cover / 4.0]
                })
                .collect();
            side = half;
        }
        Self { mips, mean }
    }
}

/// Coordonnées de texture (u, v en seizièmes) d'un coin de face, depuis sa place dans le bloc (en
/// seizièmes) : le haut de l'image en haut des côtés, une forme montre le morceau de texture
/// qu'elle couvre
pub fn face_uv(dir: usize, corner: [u8; 3]) -> (u8, u8) {
    let [x, y, z] = corner;
    match dir {
        0 => (16 - z, 16 - y),
        1 => (z, 16 - y),
        2 | 3 => (x, z),
        4 => (x, 16 - y),
        _ => (16 - x, 16 - y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_follows_the_texture_names() {
        assert_eq!(Foliage::of_texture("textures/GreenLeaf.png"), Foliage::Deciduous);
        assert_eq!(Foliage::of_texture("textures/LightGreenLeaf.png"), Foliage::Deciduous);
        assert_eq!(Foliage::of_texture("textures/DarkGreenLeaf.png"), Foliage::Conifer);
        assert_eq!(Foliage::of_texture("textures/RedLeaf.png"), Foliage::Other);
        assert_eq!(Foliage::of_texture("textures/Grass.png"), Foliage::Grass);
        assert_eq!(Foliage::of_texture("textures/TallGrass2.png"), Foliage::Grass);
        assert_eq!(Foliage::of_texture("textures/GrassBottom.png"), Foliage::Other);
        assert_eq!(Foliage::of_texture("textures/Stone.png"), Foliage::Other);
        let set = block_textures();
        let classes = set.foliage_classes();
        assert!(classes.contains(&Foliage::Deciduous) && classes.contains(&Foliage::Grass));
        assert!(set.foliage_words().iter().any(|&w| w != 0));
    }
    use herocraft_common::NO_TEXTURE;

    #[test]
    fn test_every_block_texture_loads() {
        let set = block_textures();
        let variants = set.connected.iter().flatten().count() * 16;
        assert_eq!(set.layers.len(), blocks().texture_files().len() + variants + set.icons.len());
        // Le tableau de textures tient dans la limite par défaut des cartes graphiques
        assert!(set.layers.len() <= 256, "{} couches", set.layers.len());
        let fibre = items().find("fibre").unwrap();
        assert!(set.icon_layer(fibre).is_some(), "la fibre tombe des herbes hautes : son icône est chargée");
        assert!(set.layers.len() > 100);
        for layer in &set.layers {
            assert_eq!(layer.mips.len(), MIP_LEVELS as usize);
            assert_eq!(layer.mips[0].len(), (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
            assert_eq!(layer.mips[4].len(), 4);
        }
        // La pierre montrée telle quelle : sa couleur moyenne, grise
        let stone = set.face_color(BlockId::STONE, 2).unwrap();
        assert!((stone[0] - stone[2]).abs() < 0.1 && stone[0] > 0.3);
        // L'eau garde sa couleur : pas de texture
        assert_eq!(blocks().face_texture(BlockId::WATER, 2), NO_TEXTURE);
        // L'herbe : dessus teinté, côtés (terre bordée de vert) tels quels
        let grass = blocks().get(BlockId::GRASS).unwrap();
        assert!(grass.tinted_face(2) && !grass.tinted_face(0) && !grass.tinted_face(3));
    }

    #[test]
    fn test_cutout_texture_keeps_its_holes() {
        // Damier 16 x 16 : un texel sur deux transparent
        let rgba = (0..256).flat_map(|i| if (i + i / 16) % 2 == 0 { [200, 100, 50, 255] } else { [0, 0, 0, 0] }).collect();
        let layer = Layer::from_image(&Texture { width: 16, height: 16, rgba });
        assert_eq!(layer.mips[0][3], 255);
        assert_eq!(layer.mips[0][7], 0);
        // La moyenne ne compte que les texels visibles : leur rapport vaut 1 (64 sur 255)
        assert!((layer.mean[0] - 200.0 / 255.0).abs() < 1e-3);
        assert_eq!(layer.mips[0][0], 64);
        // Plus petit niveau : moitié couvert, couleur des seuls texels visibles
        assert_eq!(layer.mips[4], vec![64, 64, 64, 128]);
    }

    #[test]
    fn test_glass_frame_follows_its_neighbours() {
        let set = block_textures();
        let glass = blocks().find("glass").unwrap();
        let base = blocks().face_texture(glass, 0);
        let alone = set.connected_layer(base, 0);
        assert_ne!(alone, base, "le verre a ses variantes connectées");
        let covered = |layer: u16, x: usize, y: usize| set.layers[layer as usize].mips[0][(y * 16 + x) * 4 + 3] > 127;
        // Seul : cadre sur les quatre bords, vitre transparente au milieu
        assert!(covered(alone, 0, 8) && covered(alone, 15, 8) && covered(alone, 8, 0) && covered(alone, 8, 15));
        assert!(!covered(alone, 4, 4));
        // Relié à droite : plus de cadre à droite, toujours à gauche
        let right = set.connected_layer(base, CONNECTED_RIGHT);
        assert!(covered(right, 0, 8) && !covered(right, 15, 8) && covered(right, 15, 0));
        // Relié de tous côtés : aucun cadre
        let all = set.connected_layer(base, 15);
        assert!(!covered(all, 0, 8) && !covered(all, 8, 0) && !covered(all, 15, 15));
        // Un bloc qui n'est pas connecté garde sa texture
        let stone = blocks().face_texture(BlockId::STONE, 0);
        assert_eq!(set.connected_layer(stone, 3), stone);
    }
}

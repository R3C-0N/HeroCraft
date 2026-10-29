//! Textures des blocs du terrain : chaque PNG cité par une fiche devient une couche d'un tableau de
//! textures de 16 x 16 texels, avec ses niveaux de détail (mipmaps).
//!
//! Une couche ne garde pas les couleurs du PNG mais leur rapport à sa couleur moyenne : le shader
//! multiplie la couleur du sommet par ce rapport. Un sommet qui porte la couleur moyenne de la
//! texture la montre telle quelle ; un sommet qui porte la couleur du biome (dessus du relief
//! naturel) la montre teintée par le biome. De loin, la face a la couleur du sommet, la même que
//! celle du relief lointain sans texture : pas de saut de teinte au fondu entre les deux.

use std::sync::OnceLock;

use herocraft_common::{blocks, BlockId};

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

/// Toutes les textures des blocs, dans l'ordre de `BlockRegistry::texture_files`
pub struct BlockTextureSet {
    layers: Vec<Layer>,
}

static TEXTURES: OnceLock<BlockTextureSet> = OnceLock::new();

/// Les textures des blocs, lues au premier appel (le maillage lit leurs couleurs moyennes)
pub fn block_textures() -> &'static BlockTextureSet {
    TEXTURES.get_or_init(BlockTextureSet::load)
}

impl BlockTextureSet {
    fn load() -> Self {
        let registry = blocks();
        let layers = registry
            .texture_files()
            .iter()
            .map(|file| {
                let path = registry.asset_path(file);
                match Texture::load(&path) {
                    Ok(image) => Layer::from_image(&image),
                    Err(e) => {
                        log::warn!("Texture {} illisible : {e}", path.display());
                        Layer::flat()
                    }
                }
            })
            .collect();
        Self { layers }
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
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: index as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * side), rows_per_image: Some(side) },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                );
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // Texels nets de près, fondus de loin (sans scintillement)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Block Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
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
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Block Texture Bind Group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        BlockTextureBinding { layout, bind_group }
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

    fn from_image(image: &Texture) -> Self {
        // Première image d'une texture animée (bande d'images carrées), ramenée à 16 x 16
        let frame = image.width.min(image.height).max(1);
        let n = TEXTURE_SIZE as usize;
        let texel = |x: usize, y: usize| -> [f32; 4] {
            let (sx, sy) = (x as u32 * frame / TEXTURE_SIZE, y as u32 * frame / TEXTURE_SIZE);
            let i = ((sy * image.width + sx) * 4) as usize;
            std::array::from_fn(|c| image.rgba.get(i + c).copied().unwrap_or(255) as f32 / 255.0)
        };
        let texels: Vec<[f32; 4]> = (0..n * n).map(|i| texel(i % n, i / n)).collect();
        // Découpée (fleurs, feuillages, échelle) : des texels entièrement transparents. Sinon (verre,
        // glace) la transparence vient de la couleur de la fiche, la texture reste pleine.
        let cutout = texels.iter().any(|t| t[3] < 0.5);
        let visible: Vec<&[f32; 4]> = texels.iter().filter(|t| t[3] >= 0.5).collect();
        let mean = if visible.is_empty() {
            [1.0; 3]
        } else {
            std::array::from_fn(|c| (visible.iter().map(|t| t[c]).sum::<f32>() / visible.len() as f32).max(1.0 / 255.0))
        };
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
    use herocraft_common::NO_TEXTURE;

    #[test]
    fn test_every_block_texture_loads() {
        let set = block_textures();
        assert_eq!(set.layers.len(), blocks().texture_files().len());
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
}

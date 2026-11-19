//! Cartes de papier découpé des cinématiques (`cards` des fichiers de `assets/cinematics/`) : des
//! feuilles posées dans la scène 3D, gondolées, avec le grain du papier, un liseré blanc autour de
//! la découpe et la lumière de la scène. Une cinématique sur fond de papier (`stage.backdrop`) y
//! tend en plus une grande feuille derrière tout ce que voit la caméra.
//!
//! Les découpes sont les PNG à transparence de `assets/cinematics/cards/`, rangées en couches d'une
//! texture. Autour de chaque découpe, un champ de distances (calculé au chargement) dessine le
//! liseré à la largeur que demande chaque carte.

use std::collections::HashMap;
use std::path::Path;

use crate::post;

/// Côté d'une couche de texture, en pixels
pub const TEXTURE_SIZE: u32 = 512;
/// Distance au dessin, en pixels, que couvre le champ du liseré (voir `paper_cards.wgsl`)
pub const OUTLINE_RANGE: f32 = 16.0;
/// Couche « sans image » (feuille unie)
pub const NO_TEXTURE: u32 = 65535;
/// Cartes dessinées au plus à la fois
pub const MAX_CARDS: usize = 256;
/// Cases de la grille d'une carte, de côté (voir le shader)
const GRID: u32 = 8;

/// Dossier des découpes
pub const CARDS_DIR: &str = "assets/cinematics/cards";

/// Une carte à dessiner : centre et demi-axes dans le monde
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardInstance {
    /// Centre (xyz), couche de texture (w, `NO_TEXTURE` sans image)
    pub center: [f32; 4],
    /// Demi-largeur (xyz), gondolement (w, en part de la demi-largeur)
    pub axis_u: [f32; 4],
    /// Demi-hauteur (xyz), liseré (w, en part de la largeur de la texture)
    pub axis_v: [f32; 4],
    /// Teinte linéaire (rgb), opacité (a)
    pub color: [f32; 4],
    /// x : lueur (au-dessus de 1, la carte brille), y : 1 pour la feuille de fond
    pub extra: [f32; 4],
}

/// Lumière et caméra de la scène
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardUniform {
    pub view_proj: [[f32; 4]; 4],
    /// Vers la lumière (xyz), part ambiante (w)
    pub light_dir: [f32; 4],
    /// Couleur et intensité de la lumière (rgb), temps (w)
    pub light_color: [f32; 4],
    /// x : taille d'un bloc
    pub params: [f32; 4],
    /// Œil de la caméra (xyz)
    pub eye: [f32; 4],
}

impl Default for CardUniform {
    fn default() -> Self {
        Self { view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(), light_dir: [0.0, 1.0, 0.0, 0.5], light_color: [1.0; 4], params: [1.0, 0.0, 0.0, 0.0], eye: [0.0; 4] }
    }
}

/// Les découpes chargées : couche et proportions (largeur / hauteur) de chacune
#[derive(Debug, Clone, Default)]
pub struct CardLayers {
    layers: HashMap<String, (u32, f32)>,
}

impl CardLayers {
    /// Couche et proportions d'une découpe (`None` : image inconnue)
    pub fn get(&self, image: &str) -> Option<(u32, f32)> {
        self.layers.get(image).copied()
    }
}


/// Les découpes de `dir`, prêtes pour la texture : couleurs, champ du liseré, couches
pub struct CardImages {
    pub layers: CardLayers,
    pub count: u32,
    pub pixels: Vec<u8>,
    pub outlines: Vec<u8>,
}

impl CardImages {
    pub fn load_default() -> Self {
        match herocraft_common::skills::asset_dir(CARDS_DIR) {
            Some(dir) => Self::load_dir(&dir),
            None => Self::load_dir(Path::new("")),
        }
    }

    /// Chaque PNG de `dir` devient une couche ; une image illisible est signalée et ignorée. Il
    /// reste toujours une couche (vide), la texture ne pouvant pas en avoir zéro.
    pub fn load_dir(dir: &Path) -> Self {
        let side = TEXTURE_SIZE as usize;
        let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))).collect();
        files.sort();
        let mut layers = HashMap::new();
        let mut pixels = Vec::new();
        let mut outlines = Vec::new();
        for path in files {
            let Some((w, h, rgba)) = crate::effects::read_png(&path) else {
                log::warn!("Découpe illisible : {}", path.display());
                continue;
            };
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let layer = (pixels.len() / (side * side * 4)) as u32;
            let resized = resize(w, h, &rgba, side);
            outlines.extend(outline_field(&resized, side));
            pixels.extend(resized);
            layers.insert(name, (layer, w as f32 / h.max(1) as f32));
        }
        if pixels.is_empty() {
            pixels = vec![0; side * side * 4];
            outlines = vec![255; side * side];
        }
        let count = (pixels.len() / (side * side * 4)) as u32;
        log::info!("{} découpe(s) de cinématique chargée(s).", layers.len());
        Self { layers: CardLayers { layers }, count, pixels, outlines }
    }
}

/// Ramène une image à `side` × `side` : chaque pixel est la moyenne de la zone qu'il couvre, les
/// couleurs pondérées par leur opacité (pas de frange sombre autour de la découpe)
fn resize(w: usize, h: usize, rgba: &[u8], side: usize) -> Vec<u8> {
    let mut out = vec![0u8; side * side * 4];
    for y in 0..side {
        let (y0, y1) = (y * h / side, ((y + 1) * h / side).max(y * h / side + 1).min(h));
        for x in 0..side {
            let (x0, x1) = (x * w / side, ((x + 1) * w / side).max(x * w / side + 1).min(w));
            let (mut c, mut a, mut n) = ([0f32; 3], 0f32, 0f32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &rgba[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4];
                    let alpha = p[3] as f32 / 255.0;
                    for i in 0..3 {
                        c[i] += p[i] as f32 * alpha;
                    }
                    a += alpha;
                    n += 1.0;
                }
            }
            let dst = &mut out[(y * side + x) * 4..(y * side + x) * 4 + 4];
            for i in 0..3 {
                dst[i] = if a > 0.0 { (c[i] / a).round() as u8 } else { 0 };
            }
            dst[3] = (a / n.max(1.0) * 255.0).round() as u8;
        }
    }
    out
}

/// Distance de chaque pixel au dessin (opacité d'au moins 1/2), en pixels, ramenée de 0 à
/// `OUTLINE_RANGE` sur 0 à 255 (distance du chanfrein 3-4, en deux passes)
fn outline_field(rgba: &[u8], side: usize) -> Vec<u8> {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = (0..side * side).map(|i| if rgba[i * 4 + 3] >= 128 { 0 } else { FAR }).collect();
    let at = |x: usize, y: usize| y * side + x;
    for y in 0..side {
        for x in 0..side {
            let mut v = d[at(x, y)];
            if x > 0 {
                v = v.min(d[at(x - 1, y)] + 3);
            }
            if y > 0 {
                v = v.min(d[at(x, y - 1)] + 3);
                if x > 0 {
                    v = v.min(d[at(x - 1, y - 1)] + 4);
                }
                if x + 1 < side {
                    v = v.min(d[at(x + 1, y - 1)] + 4);
                }
            }
            d[at(x, y)] = v;
        }
    }
    for y in (0..side).rev() {
        for x in (0..side).rev() {
            let mut v = d[at(x, y)];
            if x + 1 < side {
                v = v.min(d[at(x + 1, y)] + 3);
            }
            if y + 1 < side {
                v = v.min(d[at(x, y + 1)] + 3);
                if x + 1 < side {
                    v = v.min(d[at(x + 1, y + 1)] + 4);
                }
                if x > 0 {
                    v = v.min(d[at(x - 1, y + 1)] + 4);
                }
            }
            d[at(x, y)] = v;
        }
    }
    d.into_iter().map(|v| ((v as f32 / 3.0) / OUTLINE_RANGE * 255.0).min(255.0) as u8).collect()
}

pub struct CardRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    count: u32,
    pub layers: CardLayers,
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Paper Cards Shader"), source: wgpu::ShaderSource::Wgsl(include_str!("paper_cards.wgsl").into()) });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Paper Cards Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_card"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CardInstance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_card"),
            targets: &[Some(wgpu::ColorTargetState { format: target.format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        // Opaques (la découpe et l'apparition se font en jetant des pixels) : elles s'ordonnent
        // entre elles et avec le monde par la profondeur
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: target.sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    })
}

fn texture_array(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, format: wgpu::TextureFormat, bytes_per_pixel: u32, layers: u32, data: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: layers };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(TEXTURE_SIZE * bytes_per_pixel), rows_per_image: Some(TEXTURE_SIZE) },
        size,
    );
    texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

impl CardRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: post::SceneTarget, images: CardImages) -> Self {
        let cut_outs = texture_array(device, queue, "Paper Cards", wgpu::TextureFormat::Rgba8UnormSrgb, 4, images.count, &images.pixels);
        let outlines = texture_array(device, queue, "Paper Card Outlines", wgpu::TextureFormat::R8Unorm, 1, images.count, &images.outlines);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Paper Cards Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Paper Cards Uniform"),
            size: std::mem::size_of::<CardUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Paper Cards Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                texture_entry(1),
                texture_entry(2),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Paper Cards Bind Group"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&cut_outs) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&outlines) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Paper Cards Pipeline Layout"), bind_group_layouts: &[&bind_layout], push_constant_ranges: &[] });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Paper Cards Instances"),
            size: (MAX_CARDS * std::mem::size_of::<CardInstance>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline: create_pipeline(device, &layout, target), layout, bind_group, uniform, instances, count: 0, layers: images.layers }
    }

    pub fn set_target(&mut self, device: &wgpu::Device, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, &self.layout, target);
    }

    /// Les cartes de cette image (aucune hors des cinématiques de papier)
    pub fn update(&mut self, queue: &wgpu::Queue, uniform: &CardUniform, cards: &[CardInstance]) {
        let cards = &cards[..cards.len().min(MAX_CARDS)];
        self.count = cards.len() as u32;
        if !cards.is_empty() {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(cards));
        }
    }

    /// À appeler dans la passe 3D, avec les objets opaques
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..GRID * GRID * 6, 0..self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_is_valid() {
        let module = naga::front::wgsl::parse_str(include_str!("paper_cards.wgsl")).unwrap_or_else(|e| panic!("{e}"));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module).unwrap();
    }

    #[test]
    fn test_outline_field() {
        let side = 32;
        let mut rgba = vec![0u8; side * side * 4];
        // Un carré opaque au milieu
        for y in 12..20 {
            for x in 12..20 {
                rgba[(y * side + x) * 4 + 3] = 255;
            }
        }
        let field = outline_field(&rgba, side);
        assert_eq!(field[15 * side + 15], 0);
        // 4 pixels à gauche du carré
        let four = field[15 * side + 8] as f32 / 255.0 * OUTLINE_RANGE;
        assert!((four - 4.0).abs() < 0.2, "{four}");
        assert_eq!(field[0], 255);
    }

    #[test]
    fn test_resize_keeps_colours_at_the_edge() {
        // Moitié rouge opaque, moitié transparente noire : pas de frange sombre en réduisant
        let (w, h) = (4, 2);
        let mut rgba = Vec::new();
        for _ in 0..h {
            rgba.extend([255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0]);
        }
        let out = resize(w, h, &rgba, 1);
        assert_eq!(&out[..3], &[255, 0, 0]);
        assert_eq!(out[3], 128);
    }

    #[test]
    fn test_game_cards_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CARDS_DIR);
        let images = CardImages::load_dir(&dir);
        assert_eq!(images.pixels.len(), (images.count * TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize);
        assert_eq!(images.outlines.len(), (images.count * TEXTURE_SIZE * TEXTURE_SIZE) as usize);
        // Chaque image citée par une cinématique existe
        let (catalog, _) = herocraft_common::cinematics::CinematicCatalog::load_dir(&dir.join(".."));
        for id in catalog.ids() {
            for image in catalog.get(&id).unwrap().card_images() {
                assert!(images.layers.get(image).is_some(), "{id} : découpe {image} absente de {CARDS_DIR}");
            }
        }
    }
}

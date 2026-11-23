//! Cartes de papier découpé des cinématiques (`cards` des fichiers de `assets/cinematics/`) : des
//! feuilles posées dans la scène 3D, gondolées, avec le grain du papier, un liseré blanc autour de
//! la découpe et la lumière de la scène. Une cinématique sur fond de papier (`stage.backdrop`) y
//! tend en plus une grande feuille derrière tout ce que voit la caméra.
//!
//! Les découpes sont les PNG à transparence de `assets/cinematics/cards/` (sous-dossiers compris).
//! Plan après plan, celles que montre le plan sont rangées dans un atlas : quelques pages de texture
//! où chaque découpe garde sa taille, entourée d'une marge. L'atlas du plan suivant se prépare dans
//! un autre fil pendant que le plan se joue. Un champ de distances
//! (calculé au chargement) y dessine le liseré à la largeur que demande chaque carte.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::post;

/// Côté d'une page de l'atlas, en pixels
pub const PAGE_SIZE: u32 = 2048;
/// Plus grand côté d'une découpe dans l'atlas (les plus grandes sont réduites)
pub const MAX_IMAGE: u32 = 1024;
/// Distance au dessin, en pixels, que couvre le champ du liseré (voir `paper_cards.wgsl`)
pub const OUTLINE_RANGE: f32 = 16.0;
/// Marge autour de chaque découpe dans l'atlas : la place du liseré
pub const PAD: u32 = 16;
/// Couche « sans image » (feuille unie)
pub const NO_TEXTURE: u32 = 65535;
/// Mélanges d'une carte (`CardInstance::extra[2]`) : découpée et opaque, translucide, lumineuse
pub const BLEND_CUT: f32 = 0.0;
pub const BLEND_ALPHA: f32 = 1.0;
pub const BLEND_ADD: f32 = 2.0;

/// Cartes dessinées au plus à la fois
pub const MAX_CARDS: usize = 1024;
/// Cases de la grille d'une carte, de côté (voir le shader)
const GRID: u32 = 8;

/// Dossier des découpes
pub const CARDS_DIR: &str = "assets/cinematics/cards";

/// Une carte à dessiner : centre et demi-axes dans le monde
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CardInstance {
    /// Centre (xyz), page de l'atlas (w, `NO_TEXTURE` sans image)
    pub center: [f32; 4],
    /// Demi-largeur (xyz), gondolement (w, en part de la demi-largeur)
    pub axis_u: [f32; 4],
    /// Demi-hauteur (xyz), liseré (w, en pixels de l'atlas)
    pub axis_v: [f32; 4],
    /// Teinte linéaire (rgb), opacité (a)
    pub color: [f32; 4],
    /// x : lueur (au-dessus de 1, la carte brille), y : 1 pour la feuille de fond, z : mélange
    /// (`BLEND_CUT`, `BLEND_ALPHA`, `BLEND_ADD`)
    pub extra: [f32; 4],
    /// Coins de la découpe et de sa marge dans la page (u0, v0, u1, v1)
    pub uv: [f32; 4],
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
    /// x : taille d'un bloc, y : vignette, z : largeur / hauteur de l'image
    pub params: [f32; 4],
    /// Œil de la caméra (xyz)
    pub eye: [f32; 4],
}

impl Default for CardUniform {
    fn default() -> Self {
        Self { view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(), light_dir: [0.0, 1.0, 0.0, 0.5], light_color: [1.0; 4], params: [1.0, 0.0, 0.0, 0.0], eye: [0.0; 4] }
    }
}

/// Une découpe rangée dans l'atlas
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardSprite {
    pub page: u32,
    /// Coins de la découpe et de sa marge dans la page (u0, v0, u1, v1)
    pub uv: [f32; 4],
    /// Taille de la découpe dans l'atlas, en pixels, sans la marge
    pub size: [f32; 2],
}

impl CardSprite {
    /// Largeur / hauteur de la découpe
    pub fn aspect(&self) -> f32 {
        self.size[0] / self.size[1].max(1.0)
    }

    /// Ce que la marge ajoute de chaque côté, en part de la largeur et de la hauteur
    pub fn margin(&self) -> [f32; 2] {
        [PAD as f32 / self.size[0].max(1.0), PAD as f32 / self.size[1].max(1.0)]
    }
}

/// Les découpes de l'atlas, par nom (chemin dans `assets/cinematics/cards/`)
#[derive(Debug, Clone, Default)]
pub struct CardLayers {
    sprites: HashMap<String, CardSprite>,
}

impl CardLayers {
    /// Place d'une découpe dans l'atlas (`None` : image inconnue)
    pub fn get(&self, image: &str) -> Option<CardSprite> {
        self.sprites.get(image).copied()
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }
}

/// Les découpes d'un plan de cinématique : ce qui identifie l'atlas et les images à y ranger
#[derive(Debug, Clone, PartialEq)]
pub struct CardSet {
    pub key: String,
    pub images: Vec<String>,
}

/// Les découpes d'une cinématique, rangées en pages : couleurs, champ du liseré
pub struct CardImages {
    pub layers: CardLayers,
    pub pages: u32,
    pub pixels: Vec<u8>,
    pub outlines: Vec<u8>,
}

impl CardImages {
    /// Le dossier des découpes du jeu
    pub fn default_dir() -> PathBuf {
        herocraft_common::skills::asset_dir(CARDS_DIR).unwrap_or_default()
    }

    /// Un atlas vide (une page transparente, la texture ne pouvant pas en avoir zéro)
    pub fn empty() -> Self {
        Self::pack(Vec::new())
    }

    /// Les découpes `names` de `dir` ; une image absente ou illisible est signalée et ignorée
    pub fn load<'a>(dir: &Path, names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut images = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in names {
            if !seen.insert(name.to_string()) {
                continue;
            }
            let path = dir.join(name);
            let Some((w, h, rgba)) = crate::effects::read_png(&path) else {
                log::warn!("Découpe illisible : {}", path.display());
                continue;
            };
            let k = (MAX_IMAGE as f32 / w.max(h).max(1) as f32).min(1.0);
            let (nw, nh) = (((w as f32 * k).round() as usize).max(1), ((h as f32 * k).round() as usize).max(1));
            let rgba = if (nw, nh) == (w, h) { rgba } else { resize(w, h, &rgba, nw, nh) };
            images.push((name.to_string(), nw, nh, rgba));
        }
        let atlas = Self::pack(images);
        log::info!("{} découpe(s) de cinématique chargée(s) en {} page(s).", atlas.layers.len(), atlas.pages);
        atlas
    }

    /// Range les images en étagères, des plus hautes aux plus basses, page après page
    fn pack(mut images: Vec<(String, usize, usize, Vec<u8>)>) -> Self {
        let side = PAGE_SIZE as usize;
        let pad = PAD as usize;
        images.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
        let mut sprites = HashMap::new();
        let mut pixels: Vec<u8> = vec![0; side * side * 4];
        let (mut page, mut x, mut y, mut row) = (0usize, 0usize, 0usize, 0usize);
        for (name, w, h, rgba) in images {
            let (pw, ph) = (w + 2 * pad, h + 2 * pad);
            if x + pw > side {
                x = 0;
                y += row;
                row = 0;
            }
            if y + ph > side {
                page += 1;
                pixels.resize((page + 1) * side * side * 4, 0);
                (x, y, row) = (0, 0, 0);
            }
            let base = page * side * side * 4;
            for line in 0..h {
                let dst = base + ((y + pad + line) * side + x + pad) * 4;
                pixels[dst..dst + w * 4].copy_from_slice(&rgba[line * w * 4..(line + 1) * w * 4]);
            }
            let s = side as f32;
            sprites.insert(name, CardSprite { page: page as u32, uv: [x as f32 / s, y as f32 / s, (x + pw) as f32 / s, (y + ph) as f32 / s], size: [w as f32, h as f32] });
            x += pw;
            row = row.max(ph);
        }
        let pages = page + 1;
        let outlines = (0..pages).flat_map(|p| outline_field(&pixels[p * side * side * 4..(p + 1) * side * side * 4], side)).collect();
        Self { layers: CardLayers { sprites }, pages: pages as u32, pixels, outlines }
    }
}

/// Ramène une image à `nw` × `nh` : chaque pixel est la moyenne de la zone qu'il couvre, les
/// couleurs pondérées par leur opacité (pas de frange sombre autour de la découpe)
fn resize(w: usize, h: usize, rgba: &[u8], nw: usize, nh: usize) -> Vec<u8> {
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1).min(w));
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
            let dst = &mut out[(y * nw + x) * 4..(y * nw + x) * 4 + 4];
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
    /// Cartes translucides et lumineuses : mélangées à ce qui est derrière, sans écrire la profondeur
    blended: wgpu::RenderPipeline,
    /// Les cartes découpées viennent d'abord dans le tampon, puis les autres du fond vers l'avant
    opaque_count: u32,
    layout: wgpu::PipelineLayout,
    bind_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    count: u32,
    /// Le plan dont l'atlas est chargé
    loaded: Option<String>,
    /// L'atlas du plan suivant, rangé dans un autre fil
    pending: Option<(String, std::thread::JoinHandle<CardImages>)>,
    pub layers: CardLayers,
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget, blended: bool) -> wgpu::RenderPipeline {
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
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_card"),
            // Couleurs prémultipliées : une carte lumineuse a une opacité nulle et s'ajoute
            targets: &[Some(wgpu::ColorTargetState { format: target.format, blend: blended.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        // Opaques (la découpe et l'apparition se font en jetant des pixels) : elles s'ordonnent
        // entre elles et avec le monde par la profondeur
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: !blended,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: target.sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: crate::shaders::pipeline_cache(),
    })
}

fn texture_array(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, format: wgpu::TextureFormat, bytes_per_pixel: u32, layers: u32, data: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: PAGE_SIZE, height: PAGE_SIZE, depth_or_array_layers: layers };
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
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(PAGE_SIZE * bytes_per_pixel), rows_per_image: Some(PAGE_SIZE) },
        size,
    );
    texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

fn create_bind_group(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout, uniform: &wgpu::Buffer, sampler: &wgpu::Sampler, images: &CardImages) -> wgpu::BindGroup {
    let cut_outs = texture_array(device, queue, "Paper Cards", wgpu::TextureFormat::Rgba8UnormSrgb, 4, images.pages, &images.pixels);
    let outlines = texture_array(device, queue, "Paper Card Outlines", wgpu::TextureFormat::R8Unorm, 1, images.pages, &images.outlines);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Paper Cards Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&cut_outs) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&outlines) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

impl CardRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target: post::SceneTarget, images: CardImages) -> Self {
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
        let bind_group = create_bind_group(device, queue, &bind_layout, &uniform, &sampler, &images);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Paper Cards Pipeline Layout"), bind_group_layouts: &[&bind_layout], push_constant_ranges: &[] });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Paper Cards Instances"),
            size: (MAX_CARDS * std::mem::size_of::<CardInstance>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline: create_pipeline(device, &layout, target, false), blended: create_pipeline(device, &layout, target, true), opaque_count: 0, layout, bind_layout, sampler, bind_group, uniform, instances, count: 0, loaded: None, pending: None, layers: images.layers }
    }

    pub fn set_target(&mut self, device: &wgpu::Device, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, &self.layout, target, false);
        self.blended = create_pipeline(device, &self.layout, target, true);
    }

    /// Charge l'atlas du plan en cours s'il ne l'est pas déjà (`None` : le libère), et prépare
    /// en arrière-plan celui du plan suivant, prêt à la coupe
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, current: Option<&CardSet>, next: Option<&CardSet>) {
        if self.loaded.as_deref() != current.map(|c| c.key.as_str()) {
            let atlas = match current {
                None => {
                    self.pending = None;
                    CardImages::empty()
                }
                Some(set) => match self.pending.take() {
                    Some((key, handle)) if key == set.key => handle.join().unwrap_or_else(|_| CardImages::load(&CardImages::default_dir(), set.images.iter().map(String::as_str))),
                    // Un atlas préparé pour un autre plan est abandonné (son fil finit seul)
                    _ => CardImages::load(&CardImages::default_dir(), set.images.iter().map(String::as_str)),
                },
            };
            self.bind_group = create_bind_group(device, queue, &self.bind_layout, &self.uniform, &self.sampler, &atlas);
            self.layers = atlas.layers;
            self.loaded = current.map(|c| c.key.clone());
        }
        if let Some(next) = next {
            let ready = self.pending.as_ref().is_some_and(|(key, _)| *key == next.key);
            if !ready && self.loaded.as_deref() != Some(next.key.as_str()) {
                let images = next.images.clone();
                let handle = std::thread::spawn(move || CardImages::load(&CardImages::default_dir(), images.iter().map(String::as_str)));
                self.pending = Some((next.key.clone(), handle));
            }
        }
    }

    /// Les cartes de cette image (aucune hors des cinématiques de papier)
    pub fn update(&mut self, queue: &wgpu::Queue, uniform: &CardUniform, cards: &[CardInstance]) {
        let cards = order_cards(uniform, &cards[..cards.len().min(MAX_CARDS)]);
        self.count = cards.len() as u32;
        self.opaque_count = cards.iter().take_while(|c| c.extra[2] < 0.5).count() as u32;
        if !cards.is_empty() {
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&cards));
        }
    }

    /// À appeler dans la passe 3D, avec les objets opaques
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        if self.opaque_count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..GRID * GRID * 6, 0..self.opaque_count);
        }
        if self.count > self.opaque_count {
            pass.set_pipeline(&self.blended);
            pass.draw(0..GRID * GRID * 6, self.opaque_count..self.count);
        }
    }
}

/// Les cartes découpées d'abord, puis les autres de la plus lointaine à la plus proche (leur
/// profondeur vue de la caméra), pour qu'elles se mélangent dans l'ordre
fn order_cards(uniform: &CardUniform, cards: &[CardInstance]) -> Vec<CardInstance> {
    let view_proj = glam::Mat4::from_cols_array_2d(&uniform.view_proj);
    let depth = |c: &CardInstance| (view_proj * glam::Vec3::from_slice(&c.center[..3]).extend(1.0)).w;
    let (mut out, mut blended): (Vec<_>, Vec<_>) = cards.iter().copied().partition(|c| c.extra[2] < 0.5);
    blended.sort_by(|a, b| depth(b).total_cmp(&depth(a)));
    out.extend(blended);
    out
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
        let out = resize(w, h, &rgba, 1, 1);
        assert_eq!(&out[..3], &[255, 0, 0]);
        assert_eq!(out[3], 128);
    }

    #[test]
    fn test_blended_cards_come_last_far_to_near() {
        let uniform = CardUniform { view_proj: glam::Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0).to_cols_array_2d(), ..Default::default() };
        let card = |z: f32, blend: f32| CardInstance { center: [0.0, 0.0, z, 0.0], extra: [1.0, 0.0, blend, 0.0], ..bytemuck::Zeroable::zeroed() };
        let ordered = order_cards(&uniform, &[card(-2.0, BLEND_ALPHA), card(-9.0, BLEND_CUT), card(-5.0, BLEND_ADD), card(-1.0, BLEND_CUT)]);
        let z: Vec<f32> = ordered.iter().map(|c| c.center[2]).collect();
        assert_eq!(z, [-9.0, -1.0, -5.0, -2.0]);
    }

    #[test]
    fn test_pack_keeps_images_apart() {
        let side = PAGE_SIZE as usize;
        let images: Vec<_> = (0..40).map(|i| (format!("{i}.png"), 300 + i, 200 + 7 * i, vec![255u8; (300 + i) * (200 + 7 * i) * 4])).collect();
        let atlas = CardImages::pack(images);
        assert_eq!(atlas.layers.len(), 40);
        assert!(atlas.pages >= 2);
        assert_eq!(atlas.pixels.len(), atlas.pages as usize * side * side * 4);
        let sprites: Vec<_> = (0..40).map(|i| atlas.layers.get(&format!("{i}.png")).unwrap()).collect();
        for (i, a) in sprites.iter().enumerate() {
            assert!(a.uv[2] <= 1.0 && a.uv[3] <= 1.0);
            assert_eq!(a.size, [300.0 + i as f32, 200.0 + 7.0 * i as f32]);
            for b in &sprites[i + 1..] {
                let apart = a.page != b.page || a.uv[2] <= b.uv[0] || b.uv[2] <= a.uv[0] || a.uv[3] <= b.uv[1] || b.uv[3] <= a.uv[1];
                assert!(apart, "{a:?} {b:?}");
            }
        }
        // La marge reste transparente, l'image est opaque
        let a = sprites[0];
        let (x, y) = ((a.uv[0] * side as f32) as usize, (a.uv[1] * side as f32) as usize);
        let page = a.page as usize * side * side;
        assert_eq!(atlas.pixels[(page + (y + 2) * side + x + 2) * 4 + 3], 0);
        assert_eq!(atlas.pixels[(page + (y + PAD as usize) * side + x + PAD as usize) * 4 + 3], 255);
    }

    #[test]
    fn test_game_cards_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CARDS_DIR);
        // Chaque image citée par une cinématique existe
        let (catalog, _) = herocraft_common::cinematics::CinematicCatalog::load_dir(&dir.join(".."));
        for id in catalog.ids() {
            let cinematic = catalog.get(&id).unwrap();
            for image in cinematic.card_images() {
                assert!(dir.join(image).is_file(), "{id} : découpe {image} absente de {CARDS_DIR}");
            }
            // Plan par plan : mémoire raisonnable, au plus 6 pages (120 Mo)
            let mut t = 0.0;
            while t < cinematic.duration() {
                let (shot, start, end) = cinematic.shot_span_at(t);
                let wanted = cinematic.card_images_between(start, end);
                let images = CardImages::load(&dir, wanted.iter().copied());
                assert_eq!(images.pixels.len(), (images.pages * PAGE_SIZE * PAGE_SIZE * 4) as usize);
                assert_eq!(images.outlines.len(), (images.pages * PAGE_SIZE * PAGE_SIZE) as usize);
                assert!(images.pages <= 6, "{id}, plan {} : {} pages", shot + 1, images.pages);
                assert_eq!(images.layers.len(), wanted.len(), "{id}, plan {} : découpe illisible", shot + 1);
                t = end + 1e-3;
            }
        }
    }
}

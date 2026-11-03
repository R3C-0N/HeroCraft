//! Rendu des créatures : leur maillage de l'image (`creatures::build_frame`) dans des tampons
//! réécrits à chaque image, une texture par espèce, et un pipeline éclairé comme le relief.
//!
//! Les sommets ont le format du relief (`PlanetVertex`) dans un premier tampon, les coordonnées de
//! texture dans un second : la passe d'ombre du soleil lit le premier tel quel, et les créatures
//! portent leur ombre comme le terrain.

use std::ops::Range;

use glam::Vec3;

use crate::creatures::CreatureFrame;
use crate::planet_mesh::PlanetVertex;
use crate::post;
use crate::skinned_model::Texture;

pub struct CreatureRenderer {
    pipeline: wgpu::RenderPipeline,
    /// Gardé pour recréer le pipeline quand le MSAA change
    pipeline_layout: wgpu::PipelineLayout,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Texture de chaque espèce, dans l'ordre du registre (`None` : sans modèle)
    textures: Vec<Option<wgpu::BindGroup>>,
    /// Texture blanche des pavés gris
    plain: wgpu::BindGroup,
    /// Planche du héros vu à la troisième personne, avec la clé de son apparence
    hero: Option<(String, wgpu::BindGroup)>,
    buffers: Option<Buffers>,
    draws: Vec<(Option<usize>, Range<u32>)>,
    num_indices: u32,
    center: Vec3,
    radius: f32,
}

struct Buffers {
    vertices: wgpu::Buffer,
    uvs: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Sommets et indices qu'ils peuvent contenir
    capacity: (usize, usize),
}

/// Pipeline des créatures, qui dépend de l'échantillonnage de la passe 3D (MSAA ou non)
fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, sample_count: u32) -> wgpu::RenderPipeline {
    let shader = crate::shaders::create_shader_module(device, "Creature Shader", include_str!("creature.wgsl"), "creature.wgsl");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Creature Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_creature"),
            buffers: &[
                PlanetVertex::desc(),
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![3 => Float32x2],
                },
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_creature"),
            targets: &[Some(wgpu::ColorTargetState {
                format: post::HDR_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    })
}

impl CreatureRenderer {
    /// `scene_layout` et `shadow_layout` : groupes 0 et 1 du relief
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene_layout: &wgpu::BindGroupLayout, shadow_layout: &wgpu::BindGroupLayout, sample_count: u32) -> Self {
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Creature Texture Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Creature Pipeline Layout"),
            bind_group_layouts: &[scene_layout, shadow_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let pipeline = create_pipeline(device, &layout, sample_count);
        // Texels nets : la peinture du bestiaire est faite au texel près
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Creature Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let white = Texture { width: 1, height: 1, rgba: vec![255; 4] };
        let plain = texture_bind_group(device, queue, &texture_layout, &sampler, &white, "Creature Plain Texture");
        Self { pipeline, pipeline_layout: layout, texture_layout, sampler, textures: Vec::new(), plain, hero: None, buffers: None, draws: Vec::new(), num_indices: 0, center: Vec3::ZERO, radius: 0.0 }
    }

    /// MSAA coupé ou rétabli : le pipeline est recréé, textures et maillages sont gardés
    pub fn set_sample_count(&mut self, device: &wgpu::Device, sample_count: u32) {
        self.pipeline = create_pipeline(device, &self.pipeline_layout, sample_count);
    }

    /// Textures des espèces, dans l'ordre du registre
    pub fn set_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &[Option<Texture>]) {
        self.textures = textures
            .iter()
            .map(|t| t.as_ref().map(|t| texture_bind_group(device, queue, &self.texture_layout, &self.sampler, t, "Creature Texture")))
            .collect();
    }

    /// Planche du héros (`hero_model::HERO_TEXTURE`), repeinte seulement quand son apparence change
    pub fn set_hero_sheet(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, appearance: &herocraft_common::hero::Appearance) {
        let key = appearance.sheet_key();
        if self.hero.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let size = herocraft_common::hero::SHEET_SIZE as u32;
        let sheet = Texture { width: size, height: size, rgba: herocraft_common::hero::paint(appearance).rgba };
        let bind_group = texture_bind_group(device, queue, &self.texture_layout, &self.sampler, &sheet, "Hero Sheet");
        self.hero = Some((key, bind_group));
    }

    /// Envoie le maillage de l'image
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &CreatureFrame) {
        self.num_indices = frame.indices.len() as u32;
        self.draws = frame.draws.clone();
        if frame.indices.is_empty() {
            return;
        }
        let (vertices, indices) = (frame.vertices.len(), frame.indices.len());
        if self.buffers.as_ref().is_none_or(|b| vertices > b.capacity.0 || indices > b.capacity.1) {
            let capacity = (vertices.next_power_of_two().max(4096), indices.next_power_of_two().max(8192));
            let buffer = |label, size: usize, usage| {
                device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
            };
            self.buffers = Some(Buffers {
                vertices: buffer("Creature Vertex Buffer", capacity.0 * std::mem::size_of::<PlanetVertex>(), wgpu::BufferUsages::VERTEX),
                uvs: buffer("Creature UV Buffer", capacity.0 * std::mem::size_of::<[f32; 2]>(), wgpu::BufferUsages::VERTEX),
                indices: buffer("Creature Index Buffer", capacity.1 * 4, wgpu::BufferUsages::INDEX),
                capacity,
            });
        }
        if let Some(b) = &self.buffers {
            queue.write_buffer(&b.vertices, 0, bytemuck::cast_slice(&frame.vertices));
            queue.write_buffer(&b.uvs, 0, bytemuck::cast_slice(&frame.uvs));
            queue.write_buffer(&b.indices, 0, bytemuck::cast_slice(&frame.indices));
        }
        let (min, max) = frame.vertices.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            let p = Vec3::from(v.position);
            (lo.min(p), hi.max(p))
        });
        (self.center, self.radius) = ((min + max) * 0.5, (max - min).length() * 0.5);
    }

    /// (centre, rayon, sommets, indices, nombre d'indices) pour la passe d'ombre, s'il y a des créatures
    pub fn shadow_caster(&self) -> Option<(Vec3, f32, &wgpu::Buffer, &wgpu::Buffer, u32)> {
        let b = self.buffers.as_ref().filter(|_| self.num_indices > 0)?;
        Some((self.center, self.radius, &b.vertices, &b.indices, self.num_indices))
    }

    /// À appeler dans la passe 3D, après le relief
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup, shadow_bind_group: &wgpu::BindGroup) {
        let Some(b) = self.buffers.as_ref().filter(|_| self.num_indices > 0) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, shadow_bind_group, &[]);
        pass.set_vertex_buffer(0, b.vertices.slice(..));
        pass.set_vertex_buffer(1, b.uvs.slice(..));
        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (texture, range) in &self.draws {
            let bind_group = match texture {
                Some(crate::hero_model::HERO_TEXTURE) => self.hero.as_ref().map(|(_, b)| b),
                Some(t) => self.textures.get(*t).and_then(|b| b.as_ref()),
                None => None,
            }
            .unwrap_or(&self.plain);
            pass.set_bind_group(2, bind_group, &[]);
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
    }
}

fn texture_bind_group(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, texture: &Texture, label: &str) -> wgpu::BindGroup {
    let size = wgpu::Extent3d { width: texture.width, height: texture.height, depth_or_array_layers: 1 };
    let gpu = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Couleurs prises telles quelles, comme celles des blocs
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture { texture: &gpu, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &texture.rgba,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * texture.width), rows_per_image: Some(texture.height) },
        size,
    );
    let view = gpu.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

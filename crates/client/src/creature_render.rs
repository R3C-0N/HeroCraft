//! Rendu des créatures : leur maillage de l'image (`creatures::build_frame`) dans des tampons
//! réécrits à chaque image, une texture par espèce, et un pipeline éclairé comme le relief.
//!
//! Les sommets ont les tampons du relief (`PlanetVertex::position_layout` et `attribute_layout`),
//! les coordonnées de texture un troisième : la passe d'ombre du soleil lit les positions seules,
//! et les créatures proches portent leur ombre comme le terrain.

use std::ops::Range;

use glam::Vec3;

use crate::creatures::{CreatureFrame, SkinnedInstance, SKINNED_INDEX};
use crate::planet_mesh::{PackedAttributes, PlanetVertex};
use crate::post;
use crate::skinned_model::Texture;

/// Planches d'habitants gardées au plus avant d'oublier les plus anciennes
const MAX_VILLAGER_SHEETS: usize = 96;
/// Fils par groupe du compute shader des os (`@workgroup_size` de `skinning.wgsl`)
const SKINNING_GROUP: u32 = 64;

/// Tampons des bêtes et des matrices d'os, pour `capacity` (bêtes, matrices)
fn skinning_buffers(device: &wgpu::Device, capacity: (usize, usize)) -> (wgpu::Buffer, wgpu::Buffer, (usize, usize)) {
    let buffer = |label, size: usize| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let instances = buffer("Creature Skinning Instances", capacity.0 * std::mem::size_of::<SkinnedInstance>());
    let bones = buffer("Creature Bone Matrices", capacity.1 * 64);
    (instances, bones, capacity)
}

/// D'où vient la texture d'un humanoïde : une apparence à peindre, ou une image toute faite
#[derive(Clone, Copy)]
pub enum SheetSource<'a> {
    Painted(&'a herocraft_common::hero::Appearance),
    /// Clé unique et image
    Fixed(&'a str, &'a Texture),
}

pub struct CreatureRenderer {
    pipeline: wgpu::RenderPipeline,
    /// Gardé pour recréer le pipeline quand la cible de la passe 3D change
    pipeline_layout: wgpu::PipelineLayout,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Texture de chaque espèce, dans l'ordre du registre (`None` : sans modèle)
    textures: Vec<Option<wgpu::BindGroup>>,
    /// Texture blanche des pavés gris
    plain: wgpu::BindGroup,
    /// Planche du héros vu à la troisième personne, avec la clé de son apparence
    hero: Option<(String, wgpu::BindGroup)>,
    /// Planches des habitants et des voyageurs, par apparence, et l'image où chacune a servi pour
    /// la dernière fois
    villagers: Vec<(String, wgpu::BindGroup, u64)>,
    frame_number: u64,
    buffers: Option<Buffers>,
    draws: Vec<(Option<usize>, Range<u32>)>,
    num_indices: u32,
    center: Vec3,
    radius: f32,
    /// Os appliqués sur le GPU (`skinning.wgsl`)
    skinning: Option<GpuSkinning>,
}

/// Ce que demande le compute shader des os : modèles de liaison, bêtes et matrices de l'image
struct GpuSkinning {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    model_vertices: wgpu::Buffer,
    /// Premier sommet du modèle de chaque espèce dans `model_vertices`
    model_starts: Vec<u32>,
    /// Rayon de chaque modèle autour de ses pieds, en tailles de bête (pour la sphère englobante)
    model_reach: Vec<f32>,
    instances: wgpu::Buffer,
    bones: wgpu::Buffer,
    params: wgpu::Buffer,
    /// Bêtes et matrices que peuvent contenir les tampons
    capacity: (usize, usize),
    bind_group: Option<wgpu::BindGroup>,
    /// Travail de l'image : nombre de bêtes, plus grand modèle (en sommets)
    pending: (u32, u32),
}

struct Buffers {
    positions: wgpu::Buffer,
    attributes: wgpu::Buffer,
    uvs: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Sommets et indices qu'ils peuvent contenir
    capacity: (usize, usize),
}

/// Pipeline des créatures, qui dépend de la cible de la passe 3D (MSAA, format HDR)
fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> wgpu::RenderPipeline {
    let shader = crate::shaders::create_shader_module(device, "Creature Shader", include_str!("creature.wgsl"), "creature.wgsl");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Creature Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_creature"),
            buffers: &[
                Some(PlanetVertex::position_layout()),
                Some(PlanetVertex::attribute_layout()),
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![3 => Float32x2],
                }),
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_creature"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: target.sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview_mask: None,
        cache: crate::shaders::pipeline_cache(),
    })
}

impl CreatureRenderer {
    /// `scene_layout` et `shadow_layout` : groupes 0 et 1 du relief
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene_layout: &wgpu::BindGroupLayout, shadow_layout: &wgpu::BindGroupLayout, target: post::SceneTarget) -> Self {
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
            bind_group_layouts: &[Some(scene_layout), Some(shadow_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = create_pipeline(device, &layout, target);
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
        Self { pipeline, pipeline_layout: layout, texture_layout, sampler, textures: Vec::new(), plain, hero: None, villagers: Vec::new(), frame_number: 0, buffers: None, draws: Vec::new(), num_indices: 0, center: Vec3::ZERO, radius: 0.0, skinning: None }
    }

    /// Modèles des espèces, pour appliquer leurs os sur le GPU (`Skinning::Gpu`)
    pub fn set_models(&mut self, device: &wgpu::Device, models: &crate::creatures::CreatureModels) {
        self.skinning = Some(GpuSkinning::new(device, models));
    }

    /// À encoder avant les ombres et la passe 3D : le GPU place les sommets des bêtes de l'image
    pub fn encode_skinning(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(skin) = self.skinning.as_ref().filter(|_| self.num_indices > 0) {
            skin.encode(encoder);
        }
    }

    /// Cible de la passe 3D changée (MSAA coupé ou rétabli, format HDR) : le pipeline est
    /// recréé, textures et maillages sont gardés
    pub fn set_target(&mut self, device: &wgpu::Device, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, &self.pipeline_layout, target);
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

    /// Début d'une image : les planches d'habitants inutilisées depuis longtemps sont oubliées (les
    /// indices des autres changent, à demander de nouveau avec `villager_texture`)
    pub fn begin_villagers(&mut self) {
        self.frame_number += 1;
        if self.villagers.len() > MAX_VILLAGER_SHEETS {
            let now = self.frame_number;
            self.villagers.retain(|(_, _, used)| now - used < 600);
        }
    }

    /// Indice de texture de la planche d'un habitant (peinte la première fois qu'on la demande), ou
    /// d'une image fixe (planche d'une invocation, arme greffée)
    pub fn villager_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, source: SheetSource) -> usize {
        let key = match source {
            SheetSource::Painted(appearance) => appearance.sheet_key(),
            SheetSource::Fixed(key, _) => format!("fixe:{key}"),
        };
        let slot = match self.villagers.iter().position(|(k, _, _)| *k == key) {
            Some(slot) => slot,
            None => {
                let painted;
                let texture = match source {
                    SheetSource::Painted(appearance) => {
                        let size = herocraft_common::hero::SHEET_SIZE as u32;
                        painted = Texture { width: size, height: size, rgba: herocraft_common::hero::paint(appearance).rgba };
                        &painted
                    }
                    SheetSource::Fixed(_, texture) => texture,
                };
                let bind_group = texture_bind_group(device, queue, &self.texture_layout, &self.sampler, texture, "Villager Sheet");
                self.villagers.push((key, bind_group, 0));
                self.villagers.len() - 1
            }
        };
        self.villagers[slot].2 = self.frame_number;
        crate::hero_model::VILLAGER_TEXTURE_BASE + slot
    }

    /// Envoie le maillage de l'image
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &CreatureFrame) {
        self.num_indices = frame.indices.len() as u32;
        self.draws = frame.draws.clone();
        if let Some(skin) = self.skinning.as_mut() {
            skin.pending = (0, 0);
        }
        if frame.indices.is_empty() {
            return;
        }
        let cpu_vertices = frame.vertices.len();
        let (vertices, indices) = (cpu_vertices + frame.skinned_vertices as usize, frame.indices.len());
        let mut reallocated = false;
        if self.buffers.as_ref().is_none_or(|b| vertices > b.capacity.0 || indices > b.capacity.1) {
            reallocated = true;
            if let Some(skin) = self.skinning.as_mut() {
                skin.bind_group = None;
            }
            let capacity = (vertices.next_power_of_two().max(4096), indices.next_power_of_two().max(8192));
            let buffer = |label, size: usize, usage| {
                device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
            };
            self.buffers = Some(Buffers {
                positions: buffer("Creature Position Buffer", capacity.0 * std::mem::size_of::<[f32; 3]>(), wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE),
                attributes: buffer("Creature Attribute Buffer", capacity.0 * std::mem::size_of::<PackedAttributes>(), wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE),
                uvs: buffer("Creature UV Buffer", capacity.0 * std::mem::size_of::<[f32; 2]>(), wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE),
                indices: buffer("Creature Index Buffer", capacity.1 * 4, wgpu::BufferUsages::INDEX),
                capacity,
            });
        }
        // Sans compute shader des os, les bêtes animées sur le GPU ne sont pas dessinées
        let gpu = self.skinning.is_some() && !frame.skinned.is_empty();
        if let Some(b) = &self.buffers {
            if !frame.vertices.is_empty() {
                let (positions, attributes) = crate::planet_mesh::split_vertices(&frame.vertices);
                queue.write_buffer(&b.positions, 0, bytemuck::cast_slice(&positions));
                queue.write_buffer(&b.attributes, 0, bytemuck::cast_slice(&attributes));
                queue.write_buffer(&b.uvs, 0, bytemuck::cast_slice(&frame.uvs));
            }
            if frame.skinned.is_empty() {
                queue.write_buffer(&b.indices, 0, bytemuck::cast_slice(&frame.indices));
            } else {
                // Les sommets animés sur le GPU viennent après ceux du CPU
                let base = cpu_vertices as u32;
                let resolved: Vec<u32> = frame.indices.iter().map(|&i| if i & SKINNED_INDEX != 0 { if gpu { base + (i & !SKINNED_INDEX) } else { 0 } } else { i }).collect();
                queue.write_buffer(&b.indices, 0, bytemuck::cast_slice(&resolved));
            }
        }
        let (mut min, mut max) = frame.vertices.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            let p = Vec3::from(v.position);
            (lo.min(p), hi.max(p))
        });
        if gpu
            && let (Some(skin), Some(b)) = (self.skinning.as_mut(), self.buffers.as_ref()) {
            let (lo, hi) = skin.prepare(device, queue, frame, [&b.positions, &b.attributes, &b.uvs], cpu_vertices as u32, reallocated);
            (min, max) = (min.min(lo), max.max(hi));
        }
        (self.center, self.radius) = ((min + max) * 0.5, (max - min).length() * 0.5);
    }

    /// Maillage de l'image pour la passe d'ombre (cascade proche seulement), s'il y a des créatures
    pub fn shadow_caster(&self) -> Option<crate::shadow::Caster<'_>> {
        let b = self.buffers.as_ref().filter(|_| self.num_indices > 0)?;
        Some(crate::shadow::Caster {
            center: self.center,
            radius: self.radius,
            positions: &b.positions,
            indices: &b.indices,
            index_format: wgpu::IndexFormat::Uint32,
            num_indices: self.num_indices,
        })
    }

    /// À appeler dans la passe 3D, après le relief
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup, shadow_bind_group: &wgpu::BindGroup) {
        let Some(b) = self.buffers.as_ref().filter(|_| self.num_indices > 0) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, shadow_bind_group, &[]);
        pass.set_vertex_buffer(0, b.positions.slice(..));
        pass.set_vertex_buffer(1, b.attributes.slice(..));
        pass.set_vertex_buffer(2, b.uvs.slice(..));
        pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint32);
        for (texture, range) in &self.draws {
            let bind_group = match texture {
                Some(crate::hero_model::HERO_TEXTURE) => self.hero.as_ref().map(|(_, b)| b),
                Some(t) if *t >= crate::hero_model::VILLAGER_TEXTURE_BASE => self.villagers.get(t - crate::hero_model::VILLAGER_TEXTURE_BASE).map(|(_, b, _)| b),
                Some(t) => self.textures.get(*t).and_then(|b| b.as_ref()),
                None => None,
            }
            .unwrap_or(&self.plain);
            pass.set_bind_group(2, bind_group, &[]);
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
    }
}

impl GpuSkinning {
    fn new(device: &wgpu::Device, models: &crate::creatures::CreatureModels) -> Self {
        let (vertices, model_starts) = models.gpu_vertices();
        let model_reach = models.reaches();
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Creature Skinning Layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, true),
                storage(3, false),
                storage(4, false),
                storage(5, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Creature Skinning Pipeline Layout"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Creature Skinning Shader"), source: wgpu::ShaderSource::Wgsl(include_str!("skinning.wgsl").into()) });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Creature Skinning Pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_skin"),
            compilation_options: Default::default(),
            cache: crate::shaders::pipeline_cache(),
        });
        use wgpu::util::DeviceExt;
        let placeholder = [crate::creatures::GpuModelVertex { position: [0.0; 4], normal: [0.0; 4], weights: [0.0; 4], joints: [0; 4] }];
        let model_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Creature Model Vertices"),
            contents: bytemuck::cast_slice(if vertices.is_empty() { &placeholder[..] } else { &vertices[..] }),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor { label: Some("Creature Skinning Params"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (instances, bones, capacity) = skinning_buffers(device, (64, 2048));
        GpuSkinning { pipeline, layout, model_vertices, model_starts, model_reach, instances, bones, params, capacity, bind_group: None, pending: (0, 0) }
    }


    /// Envoie les bêtes et les matrices de l'image ; les sommets seront écrits dans `outputs`
    /// (positions, attributs, coordonnées de texture) à partir du sommet `output_base`.
    /// `rebind` : les tampons de sortie ont changé. Renvoie la boîte qui englobe les bêtes.
    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &CreatureFrame, outputs: [&wgpu::Buffer; 3], output_base: u32, rebind: bool) -> (Vec3, Vec3) {
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let instances: Vec<SkinnedInstance> = frame
            .skinned
            .iter()
            .map(|inst| SkinnedInstance { species: self.model_starts.get(inst.species as usize).copied().unwrap_or(0), ..*inst })
            .collect();
        for inst in &frame.skinned {
            let size = Vec3::from_slice(&inst.up[..3]).length();
            let reach = self.model_reach.get(inst.species as usize).copied().unwrap_or(2.0) * size;
            let feet = Vec3::from_slice(&inst.feet[..3]);
            min = min.min(feet - Vec3::splat(reach));
            max = max.max(feet + Vec3::splat(reach));
        }
        let mut rebind = rebind || self.bind_group.is_none();
        if instances.len() > self.capacity.0 || frame.bone_matrices.len() > self.capacity.1 {
            let wanted = (instances.len().next_power_of_two().max(self.capacity.0), frame.bone_matrices.len().next_power_of_two().max(self.capacity.1));
            (self.instances, self.bones, self.capacity) = skinning_buffers(device, wanted);
            rebind = true;
        }
        if rebind {
            fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
                wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }
            }
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Creature Skinning Bind Group"),
                layout: &self.layout,
                entries: &[
                    entry(0, &self.model_vertices),
                    entry(1, &self.instances),
                    entry(2, &self.bones),
                    entry(3, outputs[0]),
                    entry(4, outputs[1]),
                    entry(5, outputs[2]),
                    entry(6, &self.params),
                ],
            }));
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        queue.write_buffer(&self.bones, 0, bytemuck::cast_slice(&frame.bone_matrices));
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&[output_base, PlanetVertex::UNTEXTURED, 0, 0]));
        let largest = frame.skinned.iter().map(|i| i.vertex_count).max().unwrap_or(0);
        self.pending = (frame.skinned.len() as u32, largest);
        (min, max)
    }

    /// Le compute shader des os sur les bêtes envoyées par `prepare`
    fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(bind_group), (count, largest)) = (self.bind_group.as_ref(), self.pending) else { return };
        if count == 0 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("Creature Skinning Pass"), timestamp_writes: None });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(largest.div_ceil(SKINNING_GROUP), count, 1);
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
        wgpu::TexelCopyTextureInfo { texture: &gpu, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &texture.rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * texture.width), rows_per_image: Some(texture.height) },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::{build_frame, CreatureAnimator, CreatureModels, CreatureView, Skinning};
    use herocraft_common::{CreatureAction, CreatureRegistry};

    /// Le compute shader des os donne les mêmes sommets que le calcul du CPU. Demande un
    /// adaptateur (lavapipe suffit) : `cargo test --release -p herocraft_client skinning_gpu -- --ignored`
    #[test]
    #[ignore]
    fn skinning_gpu_matches_cpu() {
        let registry = CreatureRegistry::load_default();
        let (models, _) = CreatureModels::load(&registry);
        let species: Vec<usize> = (0..registry.len()).filter(|&s| models.gpu_vertices().1.get(s + 1).is_some_and(|&n| n > models.gpu_vertices().1[s])).collect();
        let views: Vec<CreatureView> = (0..24)
            .map(|i| {
                let feet = glam::Vec3::new(i as f32 * 3.0, 50.0, 1000.0);
                let action = [CreatureAction::Marche, CreatureAction::Repos, CreatureAction::Course][i % 3];
                CreatureView { id: herocraft_common::CreatureId(i as u64), species: Some(species[i % species.len()]), feet, yaw: i as f32 * 0.7, hurt: if i == 5 { 0.5 } else { 0.0 }, death: if i == 7 { 0.6 } else { 0.0 }, action, scale: if i == 3 { 0.5 } else { 1.0 }, mood: 0, summoner: None, copy: None, tint: None }
            })
            .collect();
        let mut animator = CreatureAnimator::default();
        let mut moving = views.clone();
        for _ in 0..20 {
            for v in moving.iter_mut() {
                v.feet.x += 0.05;
            }
            animator.update(&moving, &registry, &models, 1.0 / 60.0, 1.0);
        }
        let cpu = build_frame(&moving, &registry, &models, &animator, 1.0, Skinning::Cpu, None);
        let gpu = build_frame(&moving, &registry, &models, &animator, 1.0, Skinning::Gpu, None);
        assert!(gpu.vertices.is_empty());
        assert_eq!(gpu.skinned_vertices as usize, cpu.vertices.len());
        assert_eq!(gpu.indices.iter().map(|i| i & !SKINNED_INDEX).collect::<Vec<_>>(), cpu.indices);

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("aucun adaptateur graphique");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let mut skin = GpuSkinning::new(&device, &models);
        let n = cpu.vertices.len();
        let output = |size: usize| device.create_buffer(&wgpu::BufferDescriptor { label: None, size: size as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let outputs = [output(n * 12), output(n * 12), output(n * 8)];
        let (lo, hi) = skin.prepare(&device, &queue, &gpu, [&outputs[0], &outputs[1], &outputs[2]], 0, true);
        let mut encoder = device.create_command_encoder(&Default::default());
        skin.encode(&mut encoder);
        let readback: Vec<wgpu::Buffer> = outputs
            .iter()
            .map(|o| {
                let r = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: o.size(), usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                encoder.copy_buffer_to_buffer(o, 0, &r, 0, o.size());
                r
            })
            .collect();
        queue.submit([encoder.finish()]);
        let read = |b: &wgpu::Buffer| {
            b.slice(..).map_async(wgpu::MapMode::Read, |_| ());
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            b.slice(..).get_mapped_range().expect("lecture du tampon").to_vec()
        };
        let positions: Vec<[f32; 3]> = bytemuck::cast_slice(&read(&readback[0])).to_vec();
        let attributes: Vec<PackedAttributes> = bytemuck::cast_slice(&read(&readback[1])).to_vec();
        let uvs: Vec<[f32; 2]> = bytemuck::cast_slice(&read(&readback[2])).to_vec();
        let mut worst = 0.0_f32;
        for (k, v) in cpu.vertices.iter().enumerate() {
            let (p, q) = (glam::Vec3::from(v.position), glam::Vec3::from(positions[k]));
            worst = worst.max(p.distance(q));
            assert!(p.cmpge(lo).all() && p.cmple(hi).all(), "sommet {k} hors de la boîte englobante");
            let want = v.packed_attributes();
            assert_eq!(want.texture, attributes[k].texture);
            assert_eq!(want.color[..3], attributes[k].color[..3], "couleur du sommet {k}");
            for c in 0..4 {
                assert!((want.normal_alpha[c] as i32 - attributes[k].normal_alpha[c] as i32).abs() <= 2, "normale du sommet {k} : {:?} / {:?}", want.normal_alpha, attributes[k].normal_alpha);
            }
            assert_eq!(cpu.uvs[k], uvs[k]);
        }
        assert!(worst < 1e-3, "écart de position {worst}");
    }
}

//! Tampons partagés du relief : tous les maillages LOD et voxel vivent dans quelques grands
//! tampons (positions, attributs, indices courts, indices longs) au lieu d'un trio de tampons
//! chacun. La passe 3D et les ombres n'attachent ces tampons qu'une fois, puis dessinent chaque
//! morceau par sa plage (`base_vertex`, `first_index`) ; quand la carte le permet, tous les
//! morceaux visibles partent en un seul `multi_draw_indexed_indirect`.
//!
//! Tri sur le GPU (`terrain_cull.wgsl`) : la liste des morceaux à afficher est envoyée une fois,
//! quand elle change (`set_draw_table`) ; à chaque image, un compute shader écrit pour chaque passe
//! (la passe 3D, chaque cascade d'ombre redessinée) les dessins indirects de toute la table, sans
//! instance pour les morceaux que la passe ne voit pas. Le CPU ne trie plus rien et n'envoie plus
//! d'arguments de dessin.

use std::ops::Range;

use wgpu::util::DrawIndexedIndirectArgs;

use crate::planet_mesh::{GpuMeshData, PackedAttributes, PackedIndices};

/// Capacités de départ (sommets, indices courts, indices longs) ; chaque tampon double quand il
/// est plein
const START_VERTICES: u32 = 1 << 20;
const START_INDICES: u32 = 1 << 22;

/// Plages libres d'un tampon, en éléments : on prend la première assez grande, et une plage
/// rendue est recollée à ses voisines
#[derive(Debug)]
struct RangeAllocator {
    free: Vec<Range<u32>>,
    capacity: u32,
}

impl RangeAllocator {
    fn new(capacity: u32) -> Self {
        Self { free: vec![0..capacity], capacity }
    }

    fn alloc(&mut self, size: u32) -> Option<Range<u32>> {
        let slot = self.free.iter().position(|r| r.end - r.start >= size)?;
        let start = self.free[slot].start;
        self.free[slot].start += size;
        if self.free[slot].is_empty() {
            self.free.remove(slot);
        }
        Some(start..start + size)
    }

    fn release(&mut self, range: Range<u32>) {
        if range.is_empty() {
            return;
        }
        let at = self.free.partition_point(|r| r.start < range.start);
        self.free.insert(at, range);
        // Recolle avec la suivante, puis avec la précédente
        if at + 1 < self.free.len() && self.free[at].end == self.free[at + 1].start {
            self.free[at].end = self.free[at + 1].end;
            self.free.remove(at + 1);
        }
        if at > 0 && self.free[at - 1].end == self.free[at].start {
            self.free[at - 1].end = self.free[at].end;
            self.free.remove(at);
        }
    }

    /// Le tampon passe à `capacity` éléments : la fin ajoutée devient libre
    fn grow(&mut self, capacity: u32) {
        let added = self.capacity..capacity;
        self.capacity = capacity;
        self.release(added);
    }
}

/// Place d'un maillage dans les tampons partagés
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSlot {
    pub vertices: Range<u32>,
    pub indices: Range<u32>,
    /// Indices utiles (la plage d'un maillage aux indices courts peut en compter un de plus, pour
    /// l'alignement de 4 octets des copies)
    pub num_indices: u32,
    pub format: wgpu::IndexFormat,
}

pub struct TerrainPool {
    pub positions: wgpu::Buffer,
    pub attributes: wgpu::Buffer,
    indices16: wgpu::Buffer,
    indices32: wgpu::Buffer,
    vertex_alloc: RangeAllocator,
    index16_alloc: RangeAllocator,
    index32_alloc: RangeAllocator,
    /// Arguments des dessins indirects de l'image, et combien ce tampon en contient
    indirect: wgpu::Buffer,
    indirect_capacity: u32,
    /// Premier argument libre de l'image dans `indirect` (chaque passe y a sa plage)
    indirect_cursor: u32,
    /// La carte sait enchaîner les dessins indirects (`Features::MULTI_DRAW_INDIRECT`)
    multi_draw: bool,
    /// Tri des morceaux sur le GPU, quand la carte a des compute shaders
    gpu_cull: Option<GpuCull>,
}

/// Fils par groupe de `terrain_cull.wgsl`
const CULL_GROUP: u32 = 64;

/// Morceau de la table du relief, tel que le lit `terrain_cull.wgsl`
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CullRecord {
    /// Centre (xyz) et rayon de la sphère englobante (w)
    pub sphere: [f32; 4],
    pub max_radius: f32,
    pub index_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
}

/// Une passe vue par `terrain_cull.wgsl` : la passe 3D (`TerrainCull::gpu_pass`), une cascade
/// d'ombre (`CullPass::cascade`) ou tout (`CullPass::everything`)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CullPass {
    pub kind: u32,
    pub first_draw: u32,
    pub _pad: [u32; 2],
    pub planes: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub up: [f32; 4],
    pub flatten: [f32; 4],
    pub margins: [f32; 4],
    pub cascade: [f32; 4],
    pub right: [f32; 4],
    pub light_up: [f32; 4],
}

impl CullPass {
    pub const CAMERA: u32 = 0;
    pub const CASCADE: u32 = 1;
    pub const EVERYTHING: u32 = 2;

    pub fn everything() -> Self {
        Self { kind: Self::EVERYTHING, ..Default::default() }
    }

    /// Cascade de centre `center` et de demi-côté `extent`, dans le plan (`right`, `up`) de la lumière
    pub fn cascade(center: glam::Vec3, extent: f32, right: glam::Vec3, up: glam::Vec3) -> Self {
        Self { kind: Self::CASCADE, cascade: center.extend(extent).to_array(), right: right.extend(0.0).to_array(), light_up: up.extend(0.0).to_array(), ..Default::default() }
    }
}

/// Ce que demande le tri sur le GPU : la table des morceaux, les passes de l'image et leurs dessins
struct GpuCull {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    records: wgpu::Buffer,
    passes: wgpu::Buffer,
    params: wgpu::Buffer,
    /// Dessins indirects écrits par le shader, une plage de la taille de la table par passe
    draws: wgpu::Buffer,
    /// Morceaux, passes et dessins que peuvent contenir les tampons
    capacity: (u32, u32, u32),
    bind_group: Option<wgpu::BindGroup>,
    /// Morceaux de la table aux indices courts, puis aux indices longs (dans cet ordre)
    counts: [u32; 2],
}

fn cull_buffer(device: &wgpu::Device, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(16), usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

impl GpuCull {
    fn new(device: &wgpu::Device) -> Self {
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terrain Cull Layout"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Terrain Cull Pipeline Layout"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("Terrain Cull Shader"), source: wgpu::ShaderSource::Wgsl(include_str!("terrain_cull.wgsl").into()) });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Terrain Cull Pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_cull"),
            compilation_options: Default::default(),
            cache: crate::shaders::pipeline_cache(),
        });
        let capacity = (1024, 8, 8 * 1024);
        Self {
            pipeline,
            layout,
            records: cull_buffer(device, "Terrain Cull Records", capacity.0 as u64 * std::mem::size_of::<CullRecord>() as u64, wgpu::BufferUsages::STORAGE),
            passes: cull_buffer(device, "Terrain Cull Passes", capacity.1 as u64 * std::mem::size_of::<CullPass>() as u64, wgpu::BufferUsages::STORAGE),
            params: cull_buffer(device, "Terrain Cull Params", 16, wgpu::BufferUsages::UNIFORM),
            draws: cull_buffer(device, "Terrain Culled Draws", capacity.2 as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_SRC),
            capacity,
            bind_group: None,
            counts: [0, 0],
        }
    }

    fn total(&self) -> u32 {
        self.counts[0] + self.counts[1]
    }
}

#[cfg(test)]
impl TerrainPool {
    /// Relit les dessins de `draws` écrits par le tri sur le GPU (attend la fin du GPU)
    pub fn read_culled(&self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &PreparedDraws) -> Vec<DrawIndexedIndirectArgs> {
        let (PreparedDraws::Gpu { offset, counts }, Some(cull)) = (draws, self.gpu_cull.as_ref()) else { return Vec::new() };
        let size = (counts[0] + counts[1]) as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&cull.draws, *offset, &readback, 0, size);
        queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| ());
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = readback.slice(..).get_mapped_range().expect("lecture du tampon").to_vec();
        data.chunks_exact(20)
            .map(|c| {
                let u = |i: usize| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());
                DrawIndexedIndirectArgs { index_count: u(0), instance_count: u(1), first_index: u(2), base_vertex: u(3) as i32, first_instance: u(4) }
            })
            .collect()
    }
}

fn vertex_buffers(device: &wgpu::Device, vertices: u32) -> (wgpu::Buffer, wgpu::Buffer) {
    let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
    let buffer = |label, size: usize| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: (size * vertices as usize) as u64, usage, mapped_at_creation: false });
    (buffer("Terrain Pool Positions", std::mem::size_of::<[f32; 3]>()), buffer("Terrain Pool Attributes", std::mem::size_of::<PackedAttributes>()))
}

/// Octets d'un indice
fn index_size(format: wgpu::IndexFormat) -> u64 {
    match format {
        wgpu::IndexFormat::Uint16 => 2,
        wgpu::IndexFormat::Uint32 => 4,
    }
}

fn index_buffer(device: &wgpu::Device, indices: u32, format: wgpu::IndexFormat) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(if format == wgpu::IndexFormat::Uint16 { "Terrain Pool Indices 16" } else { "Terrain Pool Indices 32" }),
        size: indices as u64 * index_size(format),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn indirect_buffer(device: &wgpu::Device, draws: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Terrain Indirect Draws"),
        size: draws as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64,
        usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Copie `old` dans un tampon neuf plus grand (le contenu est gardé)
fn regrow(device: &wgpu::Device, queue: &wgpu::Queue, old: &wgpu::Buffer, new: wgpu::Buffer) -> wgpu::Buffer {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Terrain Pool Grow") });
    encoder.copy_buffer_to_buffer(old, 0, &new, 0, old.size());
    queue.submit([encoder.finish()]);
    new
}

impl TerrainPool {
    pub fn new(device: &wgpu::Device) -> Self {
        // Les dessins indirects enchaînés existent partout depuis wgpu 27 (rejoués un à un là où la carte ne les sait pas)
        let multi_draw = device.adapter_info().backend != wgpu::Backend::Gl;
        log::info!("Relief : tampons partagés, {}", if multi_draw { "dessins indirects enchaînés" } else { "un dessin par morceau" });
        let (positions, attributes) = vertex_buffers(device, START_VERTICES);
        Self {
            positions,
            attributes,
            indices16: index_buffer(device, START_INDICES, wgpu::IndexFormat::Uint16),
            indices32: index_buffer(device, START_INDICES, wgpu::IndexFormat::Uint32),
            vertex_alloc: RangeAllocator::new(START_VERTICES),
            index16_alloc: RangeAllocator::new(START_INDICES),
            index32_alloc: RangeAllocator::new(START_INDICES),
            indirect: indirect_buffer(device, 1024),
            indirect_capacity: 1024,
            indirect_cursor: 0,
            multi_draw,
            gpu_cull: {
                let gpu = multi_draw && device.limits().max_compute_workgroups_per_dimension > 0 && !cpu_culling_forced();
                log::info!("Relief : morceaux triés sur le {}", if gpu { "GPU" } else { "CPU" });
                gpu.then(|| GpuCull::new(device))
            },
        }
    }

    /// Les morceaux sont triés sur le GPU (`set_draw_table`, `cull`) plutôt que par `prepare_draws`
    pub fn gpu_culling(&self) -> bool {
        self.gpu_cull.is_some()
    }

    /// Nouvelle liste des morceaux à afficher (sphère englobante, plus grande distance au centre de
    /// la planète), du plus proche au plus lointain : la passe 3D les dessine dans cet ordre
    pub fn set_draw_table<'s>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, meshes: impl Iterator<Item = (&'s PoolSlot, glam::Vec3, f32, f32)>) {
        let Some(cull) = self.gpu_cull.as_mut() else { return };
        let mut groups: [Vec<CullRecord>; 2] = [Vec::new(), Vec::new()];
        for (slot, center, radius, max_radius) in meshes {
            let group = if slot.format == wgpu::IndexFormat::Uint16 { 0 } else { 1 };
            groups[group].push(CullRecord {
                sphere: center.extend(radius).to_array(),
                max_radius,
                index_count: slot.num_indices,
                first_index: slot.indices.start,
                base_vertex: slot.vertices.start as i32,
            });
        }
        cull.counts = [groups[0].len() as u32, groups[1].len() as u32];
        let total = cull.total();
        if total > cull.capacity.0 {
            cull.capacity.0 = total.next_power_of_two();
            cull.records = cull_buffer(device, "Terrain Cull Records", cull.capacity.0 as u64 * std::mem::size_of::<CullRecord>() as u64, wgpu::BufferUsages::STORAGE);
            cull.bind_group = None;
        }
        if total > 0 {
            let records: Vec<CullRecord> = groups.concat();
            queue.write_buffer(&cull.records, 0, bytemuck::cast_slice(&records));
        }
    }

    /// Trie la table pour chaque passe de `passes` sur le GPU (encodé dans `encoder`, avant les
    /// passes de rendu) ; rend les dessins de chaque passe, dans l'ordre
    pub fn cull(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, passes: &mut [CullPass]) -> Vec<PreparedDraws> {
        let Some(cull) = self.gpu_cull.as_mut() else { return Vec::new() };
        let total = cull.total();
        let empty = || PreparedDraws::Gpu { offset: 0, counts: [0, 0] };
        if total == 0 || passes.is_empty() {
            return passes.iter().map(|_| empty()).collect();
        }
        let pass_count = passes.len() as u32;
        if pass_count > cull.capacity.1 {
            cull.capacity.1 = pass_count.next_power_of_two();
            cull.passes = cull_buffer(device, "Terrain Cull Passes", cull.capacity.1 as u64 * std::mem::size_of::<CullPass>() as u64, wgpu::BufferUsages::STORAGE);
            cull.bind_group = None;
        }
        if pass_count * total > cull.capacity.2 {
            cull.capacity.2 = (pass_count * total).next_power_of_two();
            cull.draws = cull_buffer(device, "Terrain Culled Draws", cull.capacity.2 as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_SRC);
            cull.bind_group = None;
        }
        for (i, pass) in passes.iter_mut().enumerate() {
            pass.first_draw = i as u32 * total;
        }
        queue.write_buffer(&cull.passes, 0, bytemuck::cast_slice(passes));
        queue.write_buffer(&cull.params, 0, bytemuck::cast_slice(&[total, 0, 0, 0]));
        let bind_group = cull.bind_group.get_or_insert_with(|| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Terrain Cull Bind Group"),
                layout: &cull.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: cull.records.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: cull.passes.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: cull.draws.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: cull.params.as_entire_binding() },
                ],
            })
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("Terrain Cull"), timestamp_writes: None });
            pass.set_pipeline(&cull.pipeline);
            pass.set_bind_group(0, &*bind_group, &[]);
            pass.dispatch_workgroups(total.div_ceil(CULL_GROUP), pass_count, 1);
        }
        let stride = std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        passes.iter().map(|p| PreparedDraws::Gpu { offset: p.first_draw as u64 * stride, counts: cull.counts }).collect()
    }

    pub fn index_buffer(&self, format: wgpu::IndexFormat) -> &wgpu::Buffer {
        match format {
            wgpu::IndexFormat::Uint16 => &self.indices16,
            wgpu::IndexFormat::Uint32 => &self.indices32,
        }
    }

    /// Place un maillage dans les tampons (agrandis si besoin) et l'y copie
    pub fn insert(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &GpuMeshData) -> PoolSlot {
        let vertex_count = data.positions.len() as u32;
        let format = data.indices.format();
        // Indices courts par paires : chaque copie fait un multiple de 4 octets
        let index_count = match &data.indices {
            PackedIndices::U16(i) => (i.len() as u32).next_multiple_of(2),
            PackedIndices::U32(i) => i.len() as u32,
        };
        let vertices = loop {
            if let Some(r) = self.vertex_alloc.alloc(vertex_count) {
                break r;
            }
            let capacity = (self.vertex_alloc.capacity * 2).max(vertex_count.next_power_of_two());
            let (positions, attributes) = vertex_buffers(device, capacity);
            self.positions = regrow(device, queue, &self.positions, positions);
            self.attributes = regrow(device, queue, &self.attributes, attributes);
            self.vertex_alloc.grow(capacity);
        };
        let indices = loop {
            let alloc = if format == wgpu::IndexFormat::Uint16 { &mut self.index16_alloc } else { &mut self.index32_alloc };
            // Début pair pour les indices courts (alignement de 4 octets)
            if let Some(r) = alloc.alloc(index_count) {
                break r;
            }
            let capacity = (alloc.capacity * 2).max(index_count.next_power_of_two());
            alloc.grow(capacity);
            let old = if format == wgpu::IndexFormat::Uint16 { &mut self.indices16 } else { &mut self.indices32 };
            *old = regrow(device, queue, old, index_buffer(device, capacity, format));
        };
        queue.write_buffer(&self.positions, vertices.start as u64 * 12, bytemuck::cast_slice(&data.positions));
        queue.write_buffer(&self.attributes, vertices.start as u64 * 12, bytemuck::cast_slice(&data.attributes));
        let offset = indices.start as u64 * index_size(format);
        match &data.indices {
            PackedIndices::U16(i) if i.len() % 2 == 1 => {
                let mut padded = i.clone();
                padded.push(0);
                queue.write_buffer(&self.indices16, offset, bytemuck::cast_slice(&padded));
            }
            PackedIndices::U16(i) => queue.write_buffer(&self.indices16, offset, bytemuck::cast_slice(i)),
            PackedIndices::U32(i) => queue.write_buffer(&self.indices32, offset, bytemuck::cast_slice(i)),
        }
        PoolSlot { vertices, indices, num_indices: data.num_indices, format }
    }

    /// Rend la place d'un maillage retiré
    pub fn release(&mut self, slot: PoolSlot) {
        self.vertex_alloc.release(slot.vertices);
        match slot.format {
            wgpu::IndexFormat::Uint16 => self.index16_alloc.release(slot.indices),
            wgpu::IndexFormat::Uint32 => self.index32_alloc.release(slot.indices),
        }
    }

    /// Début d'une image : au plus `max_draws` dessins seront préparés (toutes passes comprises)
    pub fn begin_frame(&mut self, device: &wgpu::Device, max_draws: usize) {
        self.indirect_cursor = 0;
        if self.multi_draw && max_draws as u32 > self.indirect_capacity {
            self.indirect_capacity = (max_draws as u32).next_power_of_two();
            self.indirect = indirect_buffer(device, self.indirect_capacity);
        }
    }

    /// Prépare les dessins de `slots` (déjà triés) : arguments indirects envoyés au GPU quand la
    /// carte sait les enchaîner. À appeler avant d'encoder la passe, puis `draw` dans la passe.
    pub fn prepare_draws<'s>(&mut self, queue: &wgpu::Queue, slots: impl Iterator<Item = &'s PoolSlot>) -> PreparedDraws {
        let mut groups: [Vec<DrawIndexedIndirectArgs>; 2] = [Vec::new(), Vec::new()];
        for slot in slots {
            let group = if slot.format == wgpu::IndexFormat::Uint16 { 0 } else { 1 };
            groups[group].push(DrawIndexedIndirectArgs {
                index_count: slot.num_indices,
                instance_count: 1,
                first_index: slot.indices.start,
                base_vertex: slot.vertices.start as i32,
                first_instance: 0,
            });
        }
        let total = (groups[0].len() + groups[1].len()) as u32;
        let indirect = self.multi_draw && total > 0 && self.indirect_cursor + total <= self.indirect_capacity;
        let offset = self.indirect_cursor as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        if indirect {
            let bytes: Vec<u8> = groups.iter().flat_map(|g| g.iter().flat_map(|a| a.as_bytes().to_vec())).collect();
            queue.write_buffer(&self.indirect, offset, &bytes);
            self.indirect_cursor += total;
        }
        PreparedDraws::Cpu { groups, offset: indirect.then_some(offset) }
    }

    /// Dessine les morceaux préparés ; le pipeline et les groupes de liaison sont déjà en place.
    /// `with_attributes` : la passe lit aussi le second tampon (la passe 3D, pas les ombres).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, draws: &PreparedDraws, with_attributes: bool) {
        if draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, self.positions.slice(..));
        if with_attributes {
            pass.set_vertex_buffer(1, self.attributes.slice(..));
        }
        let (groups, offset) = match draws {
            PreparedDraws::Cpu { groups, offset } => (groups, offset),
            PreparedDraws::Gpu { offset, counts } => {
                let Some(cull) = self.gpu_cull.as_ref() else { return };
                let mut at = *offset;
                for (&count, format) in counts.iter().zip([wgpu::IndexFormat::Uint16, wgpu::IndexFormat::Uint32]) {
                    if count == 0 {
                        continue;
                    }
                    pass.set_index_buffer(self.index_buffer(format).slice(..), format);
                    pass.multi_draw_indexed_indirect(&cull.draws, at, count);
                    at += count as u64 * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
                }
                return;
            }
        };
        let mut offset = *offset;
        for (group, format) in groups.iter().zip([wgpu::IndexFormat::Uint16, wgpu::IndexFormat::Uint32]) {
            if group.is_empty() {
                continue;
            }
            pass.set_index_buffer(self.index_buffer(format).slice(..), format);
            if let Some(at) = offset.as_mut() {
                pass.multi_draw_indexed_indirect(&self.indirect, *at, group.len() as u32);
                *at += (group.len() * std::mem::size_of::<DrawIndexedIndirectArgs>()) as u64;
            } else {
                for a in group {
                    pass.draw_indexed(a.first_index..a.first_index + a.index_count, a.base_vertex, 0..1);
                }
            }
        }
    }

    /// Octets réservés sur le GPU (sommets et indices)
    pub fn reserved_bytes(&self) -> u64 {
        self.positions.size() + self.attributes.size() + self.indices16.size() + self.indices32.size()
    }
}

/// Dessins d'une passe, par format d'indices
pub enum PreparedDraws {
    /// Triés sur le CPU (`prepare_draws`)
    Cpu {
        groups: [Vec<DrawIndexedIndirectArgs>; 2],
        /// Place des arguments dans le tampon indirect (`None` : un `draw_indexed` par morceau)
        offset: Option<u64>,
    },
    /// Triés sur le GPU (`cull`) : toute la table, à partir de `offset` dans ses dessins
    Gpu { offset: u64, counts: [u32; 2] },
}

impl PreparedDraws {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Cpu { groups, .. } => groups.iter().all(Vec::is_empty),
            Self::Gpu { counts, .. } => counts == &[0, 0],
        }
    }
}

/// `HEROCRAFT_TRI=cpu` garde l'ancien tri des morceaux sur le CPU (comparaisons, cartes capricieuses)
fn cpu_culling_forced() -> bool {
    std::env::var("HEROCRAFT_TRI").is_ok_and(|v| v.eq_ignore_ascii_case("cpu"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freed_ranges_are_merged_and_reused() {
        let mut a = RangeAllocator::new(100);
        let x = a.alloc(30).unwrap();
        let y = a.alloc(30).unwrap();
        let z = a.alloc(30).unwrap();
        assert_eq!((x.clone(), y.clone(), z.clone()), (0..30, 30..60, 60..90));
        assert!(a.alloc(20).is_none());
        a.release(y);
        a.release(x);
        assert_eq!(a.free, vec![0..60, 90..100]);
        assert_eq!(a.alloc(50), Some(0..50));
        a.release(z);
        assert_eq!(a.free, vec![50..100]);
        a.grow(200);
        assert_eq!(a.free, vec![50..200]);
    }
}

//! Pluie et neige autour du joueur : des gouttes (traits fins) ou des flocons qui tombent dans
//! chaque colonne de blocs d'un carré centré sur lui, et s'arrêtent sur le premier bloc porteur
//! qu'elles rencontrent. Sous un toit, sous un arbre ou dans une grotte, il ne pleut pas.
//!
//! Tout le mouvement est calculé par le shader de sommets à partir du temps : le processeur ne
//! fournit que le dessus de chaque colonne (`ColumnTops`), relu quand le joueur change de bloc.
//! Une colonne dessine `drops` gouttes ; au plus ~6 700 quadrilatères à pleine averse.

use glam::Vec3;

use crate::post;

/// Demi-côté du carré de colonnes autour du joueur, en blocs : 41 × 41 colonnes
pub const RADIUS_BLOCKS: i32 = 20;
pub const SIDE: usize = (2 * RADIUS_BLOCKS + 1) as usize;
/// Gouttes par colonne à pleine averse (pluie, neige), et sur une puce intégrée
const MAX_RAIN_DROPS: u32 = 4;
const MAX_SNOW_FLAKES: u32 = 3;
const MAX_DROPS_INTEGRATED: u32 = 2;
// Le volume où tombe la pluie va de 16 blocs au-dessus des pieds à 10 blocs en dessous
// (precipitation.wgsl, `ABOVE_BLOCKS` et `BELOW_BLOCKS`)

/// Une colonne : direction de son centre depuis le centre de la planète (xyz) et rayon du dessus
/// de son plus haut bloc porteur (w), où s'arrêtent les gouttes
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Column {
    pub dir: [f32; 3],
    pub top_radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PrecipUniform {
    /// Verticale au joueur (xyz), temps de l'averse en secondes (w)
    up: [f32; 4],
    /// Est local (xyz), force de l'averse (w, 0 à 1)
    east: [f32; 4],
    /// Nord local (xyz), part de neige (w, 0 : pluie, 1 : neige)
    north: [f32; 4],
    /// x : taille d'un bloc ; y : rayon des pieds du joueur ; z : vent (blocs de dérive vers l'est
    /// par bloc de chute) ; w : gouttes par colonne à cette force
    /// (fractionnaire : la dernière goutte est en partie transparente)
    params: [f32; 4],
}

/// Ce que l'averse doit savoir à chaque image
#[derive(Debug, Clone, Copy)]
pub struct PrecipFrame {
    pub feet: Vec3,
    pub block: f32,
    /// Force de 0 à 1 (0 : rien n'est dessiné)
    pub intensity: f32,
    /// Part de neige, de 0 à 1
    pub snow: f32,
    /// Vent, de 0 à 1
    pub wind: f32,
    /// Temps en secondes (réel : l'averse ne suit pas la vitesse du ciel)
    pub time: f32,
}

pub struct Precipitation {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    columns: wgpu::Buffer,
    column_count: u32,
    drops: u32,
    integrated: bool,
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> wgpu::RenderPipeline {
    let shader =
        crate::shaders::create_shader_module(device, "Precipitation Shader", include_str!("precipitation.wgsl"), "precipitation.wgsl");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Precipitation Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_precip"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Column>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_precip"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: target.sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview_mask: None,
        cache: crate::shaders::pipeline_cache(),
    })
}

impl Precipitation {
    pub fn new(device: &wgpu::Device, scene_layout: &wgpu::BindGroupLayout, target: post::SceneTarget, integrated: bool) -> Self {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Precipitation Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Precipitation Pipeline Layout"),
            bind_group_layouts: &[Some(scene_layout), Some(&uniform_layout)],
            immediate_size: 0,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Precipitation Uniform"),
            size: std::mem::size_of::<PrecipUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Precipitation Bind Group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let columns = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Precipitation Columns"),
            size: (SIDE * SIDE * std::mem::size_of::<Column>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: create_pipeline(device, &layout, target),
            layout,
            uniform,
            bind_group,
            columns,
            column_count: 0,
            drops: 0,
            integrated,
        }
    }

    pub fn set_target(&mut self, device: &wgpu::Device, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, &self.layout, target);
    }

    /// Nouveaux dessus de colonnes (au plus `SIDE` × `SIDE`)
    pub fn set_columns(&mut self, queue: &wgpu::Queue, columns: &[Column]) {
        let columns = &columns[..columns.len().min(SIDE * SIDE)];
        self.column_count = columns.len() as u32;
        if !columns.is_empty() {
            queue.write_buffer(&self.columns, 0, bytemuck::cast_slice(columns));
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, frame: Option<PrecipFrame>) {
        let Some(f) = frame.filter(|f| f.intensity > 0.01) else {
            self.drops = 0;
            return;
        };
        let max = if f.snow > 0.5 { MAX_SNOW_FLAKES } else { MAX_RAIN_DROPS };
        let max = if self.integrated { max.min(MAX_DROPS_INTEGRATED) } else { max };
        self.drops = ((f.intensity * max as f32).ceil() as u32).clamp(1, max);
        let up = f.feet.normalize_or_zero();
        let (east, north) = herocraft_common::tangent_frame(up);
        // L'averse se densifie aussi entre deux paliers de gouttes : transparence
        let uniform = PrecipUniform {
            up: [up.x, up.y, up.z, f.time],
            east: [east.x, east.y, east.z, f.intensity],
            north: [north.x, north.y, north.z, f.snow],
            params: [f.block, f.feet.length(), f.wind * 0.35, f.intensity * max as f32],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// À appeler dans la passe 3D, après le relief, le ciel et les nuages
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup) {
        if self.drops == 0 || self.column_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.columns.slice(..));
        pass.draw(0..self.drops * 6, 0..self.column_count);
    }
}

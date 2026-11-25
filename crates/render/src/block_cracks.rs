//! Fissures du bloc en train de casser : ses faces tournées vers l'œil, à peine décollées du
//! bloc, sur lesquelles le shader dessine des fissures pixelisées qui s'étendent avec l'avancement.

use glam::Vec3;

use crate::post;

/// Les 6 faces d'un bloc, par indices de coins (bit 0 = x + 1, bit 1 = y + 1, bit 2 = z + 1),
/// dans l'ordre du tour de la face
const FACES: [[usize; 4]; 6] = [
    [0, 2, 6, 4], // x -
    [1, 3, 7, 5], // x +
    [0, 1, 5, 4], // y -
    [2, 3, 7, 6], // y +
    [0, 1, 3, 2], // z -
    [4, 5, 7, 6], // z +
];
const VERTICES: usize = FACES.len() * 6;
/// Les faces sont décollées du bloc de cette fraction de bloc, vers l'extérieur et vers l'œil
const LIFT: f32 = 0.004;
const TOWARD_EYE: f32 = 0.01;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct CrackVertex {
    position: [f32; 3],
    /// Coordonnées sur la face (0 à 1), numéro de la face et avancement de la casse
    face_uv: [f32; 4],
}

pub struct BlockCracks {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    count: u32,
}

/// Pipeline des fissures, qui dépend de la cible de la passe 3D (MSAA, format HDR)
fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> wgpu::RenderPipeline {
    let shader = crate::shaders::create_shader_module(device, "Block Cracks Shader", include_str!("block_cracks.wgsl"), "block_cracks.wgsl");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Block Cracks Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_cracks"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<CrackVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_cracks"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: target.sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: crate::shaders::pipeline_cache(),
    })
}

impl BlockCracks {
    /// `layout` : celui du ciel et des nuages (groupe 0 = scène, dont la caméra)
    pub fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> Self {
        let pipeline = create_pipeline(device, layout, target);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Block Cracks Vertex Buffer"),
            size: (VERTICES * std::mem::size_of::<CrackVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, vertex_buffer, count: 0 }
    }

    /// Cible de la passe 3D changée (MSAA coupé ou rétabli, format HDR) : le pipeline est recréé
    pub fn set_target(&mut self, device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, layout, target);
    }

    /// Bloc de coins `corners` cassé à `progress` (0 à 1), vu de `eye` (aucun : rien n'est dessiné)
    pub fn update(&mut self, queue: &wgpu::Queue, cracks: Option<([Vec3; 8], f32)>, eye: Vec3) {
        let Some((corners, progress)) = cracks else {
            self.count = 0;
            return;
        };
        let vertices = faces(&corners, progress, eye);
        self.count = vertices.len() as u32;
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
    }

    /// À appeler dans la passe 3D, après le relief (dont il lit la profondeur)
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

/// Deux triangles par face tournée vers l'œil, décollés du bloc
fn faces(corners: &[Vec3; 8], progress: f32, eye: Vec3) -> Vec<CrackVertex> {
    let block_size = corners[0].distance(corners[1]);
    let centre = corners.iter().copied().sum::<Vec3>() / 8.0;
    const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut out = Vec::with_capacity(VERTICES);
    for (f, face) in FACES.iter().enumerate() {
        let face_centre = face.iter().map(|&i| corners[i]).sum::<Vec3>() / 4.0;
        let outward = (face_centre - centre).normalize_or_zero();
        if outward.dot(eye - face_centre) <= 0.0 {
            continue;
        }
        for k in [0, 1, 2, 0, 2, 3] {
            let p = corners[face[k]] + outward * LIFT * block_size;
            let to_eye = eye - p;
            let p = p + to_eye.normalize_or_zero() * (TOWARD_EYE * block_size).min(0.5 * to_eye.length());
            out.push(CrackVertex { position: p.to_array(), face_uv: [UV[k][0], UV[k][1], f as f32, progress] });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_faces_turned_to_the_eye_are_drawn() {
        let corners: [Vec3; 8] = std::array::from_fn(|i| Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32));
        // Vu en biais : trois faces visibles (x +, y +, z +)
        let vertices = faces(&corners, 0.5, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(vertices.len(), 3 * 6);
        let mut seen: Vec<u32> = vertices.iter().map(|v| v.face_uv[2] as u32).collect();
        seen.dedup();
        assert_eq!(seen, vec![1, 3, 5]);
        for v in &vertices {
            let p = Vec3::from(v.position);
            assert!(p.cmpge(Vec3::splat(-0.02)).all() && p.cmple(Vec3::splat(1.02)).all(), "{p} déborde du bloc");
            assert_eq!(v.face_uv[3], 0.5);
        }
        // Vu de face : une seule
        assert_eq!(faces(&corners, 0.5, Vec3::new(0.5, 5.0, 0.5)).len(), 6);
    }
}

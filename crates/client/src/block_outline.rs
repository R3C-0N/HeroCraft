//! Contour en surbrillance du bloc visé : 12 rubans le long de ses arêtes, reconstruits à chaque
//! frame face à la caméra. Le même tracé dessine la silhouette des autres joueurs en ligne et celle
//! des créatures, chaque boîte de sa couleur.

use glam::Vec3;

use crate::post;

/// Les 12 arêtes d'un bloc, par indices de coins (bit 0 = x + 1, bit 1 = y + 1, bit 2 = z + 1)
const EDGES: [(usize, usize); 12] = [
    (0, 1), (2, 3), (4, 5), (6, 7), // le long de x
    (0, 2), (1, 3), (4, 6), (5, 7), // le long de y
    (0, 4), (1, 5), (2, 6), (3, 7), // le long de z
];
const VERTICES: usize = EDGES.len() * 6;

/// Demi-largeur d'un ruban, en fraction de sa distance à l'œil (~2,5 px de large en 720p à 70°)
const HALF_WIDTH_PER_DISTANCE: f32 = 0.0018;
/// Les rubans sont avancés vers l'œil de cette fraction de bloc : une arête dans un coin rentrant
/// (contre un bloc voisin ou le sol) passerait sinon à moitié derrière la face qui la longe, alors
/// que les arêtes du dos du bloc, un bloc entier plus loin, restent cachées
const TOWARD_EYE: f32 = 0.04;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct OutlineVertex {
    position: [f32; 3],
    /// Couleur dans la scène HDR (au-delà de 1 : le bloom la fait briller)
    color: [f32; 3],
}

/// Boîte à cerner : ses 8 coins et sa couleur
pub type ColoredBox = ([Vec3; 8], [f32; 3]);

/// Contour du bloc visé : blanc
const OUTLINE_COLOR: [f32; 3] = [2.2, 2.2, 2.0];
/// Silhouette des autres joueurs : dorée
const PLAYER_COLOR: [f32; 3] = [2.4, 1.7, 0.4];

pub struct BlockOutline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    max_boxes: usize,
    /// Couleur des boîtes passées sans couleur à `update`
    color: [f32; 3],
    /// Boîtes dessinées
    count: usize,
    /// Contenu actuel du tampon : il n'est réécrit que s'il change
    uploaded: Option<(Vec<ColoredBox>, Vec3)>,
}

impl BlockOutline {
    /// Contour du bloc visé (une boîte, blanche)
    pub fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, sample_count: u32) -> Self {
        Self::with_style(device, layout, sample_count, OUTLINE_COLOR, 1)
    }

    /// Silhouettes des autres joueurs (jusqu'à `max_boxes`, dorées)
    pub fn players(device: &wgpu::Device, layout: &wgpu::PipelineLayout, sample_count: u32, max_boxes: usize) -> Self {
        Self::with_style(device, layout, sample_count, PLAYER_COLOR, max_boxes)
    }

    /// Silhouettes des créatures (jusqu'à `max_boxes`, de la couleur de leur espèce)
    pub fn creatures(device: &wgpu::Device, layout: &wgpu::PipelineLayout, sample_count: u32, max_boxes: usize) -> Self {
        Self::with_style(device, layout, sample_count, PLAYER_COLOR, max_boxes)
    }

    /// `layout` : celui du ciel et des nuages (groupe 0 = scène, dont la caméra) ; `color` : celle
    /// des boîtes passées sans couleur
    fn with_style(device: &wgpu::Device, layout: &wgpu::PipelineLayout, sample_count: u32, color: [f32; 3], max_boxes: usize) -> Self {
        let shader = crate::shaders::create_shader_module(device, "Block Outline Shader", include_str!("block_outline.wgsl"), "block_outline.wgsl");
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Block Outline Pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_outline"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<OutlineVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_outline"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: post::HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
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
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Block Outline Vertex Buffer"),
            size: (max_boxes * VERTICES * std::mem::size_of::<OutlineVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, vertex_buffer, max_boxes, color, count: 0, uploaded: None }
    }

    /// Contour des boîtes de coins `boxes` vues de `eye`, de la couleur du style (aucune : rien n'est
    /// dessiné)
    pub fn update(&mut self, queue: &wgpu::Queue, boxes: &[[Vec3; 8]], eye: Vec3) {
        let colored: Vec<ColoredBox> = boxes.iter().map(|&corners| (corners, self.color)).collect();
        self.update_colored(queue, &colored, eye);
    }

    /// Contour des boîtes `(coins, couleur)` vues de `eye`
    pub fn update_colored(&mut self, queue: &wgpu::Queue, boxes: &[ColoredBox], eye: Vec3) {
        let boxes = &boxes[..boxes.len().min(self.max_boxes)];
        self.count = boxes.len();
        if boxes.is_empty() || self.uploaded.as_ref().is_some_and(|(b, e)| b == boxes && *e == eye) {
            return;
        }
        self.uploaded = Some((boxes.to_vec(), eye));
        let vertices: Vec<OutlineVertex> = boxes.iter().flat_map(|(corners, color)| ribbons(corners, *color, eye)).collect();
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// À appeler dans la passe 3D, après le relief (dont il lit la profondeur)
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..(self.count * VERTICES) as u32, 0..1);
    }
}

/// Deux triangles par arête, larges perpendiculairement à l'arête et au regard
fn ribbons(corners: &[Vec3; 8], color: [f32; 3], eye: Vec3) -> [OutlineVertex; VERTICES] {
    let block_size = corners[0].distance(corners[1]);
    let mut out = [OutlineVertex { position: [0.0; 3], color }; VERTICES];
    for (i, &(a, b)) in EDGES.iter().enumerate() {
        let (a, b) = (corners[a], corners[b]);
        let along = (b - a).normalize_or_zero();
        let to_eye = eye - (a + b) * 0.5;
        let half_width = to_eye.length() * HALF_WIDTH_PER_DISTANCE;
        let side = along.cross(to_eye).normalize_or_zero() * half_width;
        // Les rubans dépassent d'une demi-largeur aux coins pour se rejoindre sans encoche
        let (a, b) = (a - along * half_width, b + along * half_width);
        let quad = [a - side, a + side, b + side, a - side, b + side, b - side];
        for (j, &p) in quad.iter().enumerate() {
            let to_eye = eye - p;
            let p = p + to_eye.normalize_or_zero() * (TOWARD_EYE * block_size).min(0.5 * to_eye.length());
            out[i * 6 + j] = OutlineVertex { position: p.to_array(), color };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ribbons_follow_every_edge_and_face_the_eye() {
        let corners: [Vec3; 8] = std::array::from_fn(|i| Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32));
        let eye = Vec3::new(3.0, 4.0, 5.0);
        let vertices = ribbons(&corners, OUTLINE_COLOR, eye);
        for (i, &(a, b)) in EDGES.iter().enumerate() {
            let quad: Vec<Vec3> = vertices[i * 6..i * 6 + 6].iter().map(|v| Vec3::from(v.position)).collect();
            let mid = quad.iter().copied().sum::<Vec3>() / 6.0;
            let expected = (corners[a] + corners[b]) * 0.5;
            assert!(mid.distance(expected) < 0.2, "arête {i} : {mid} au lieu de {expected}");
            // Le ruban longe l'arête et fait face à l'œil autant que possible
            let normal = (quad[1] - quad[0]).cross(quad[2] - quad[0]).normalize();
            assert!(normal.dot((corners[b] - corners[a]).normalize()).abs() < 0.05);
            assert!(normal.dot((eye - expected).normalize()).abs() > 0.5);
            // Il est avancé vers l'œil
            assert!(eye.distance(mid) < eye.distance(expected));
        }
    }
}

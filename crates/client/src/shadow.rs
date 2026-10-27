//! Ombres portées du soleil : trois cascades de profondeur centrées sur le joueur pour le relief,
//! et une carte d'opacité des nuages, toutes vues depuis le soleil en projection orthographique.
//! Le shader du terrain les lit au groupe 1 (`sun_visibility` dans shader.wgsl).

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::ui::ShadowSettings;

pub const CASCADE_COUNT: usize = 3;
const CASCADE_SIZE: u32 = 2048;
const CLOUD_MAP_SIZE: u32 = 1024;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLOUD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Demi-largeur monde de chaque cascade sur une planète de rayon 25 (multipliée par l'échelle).
/// Un bloc mesure environ 0.038 : 50 texels par bloc près du joueur, 5 au bord du brouillard.
const CASCADE_EXTENTS: [f32; CASCADE_COUNT] = [0.8, 2.6, 8.0];
/// Demi-largeur couverte par la carte des nuages (au-delà, le brouillard masque le sol)
const CLOUD_EXTENT: f32 = 10.0;
/// Distance de la caméra soleil au centre, et profondeur totale du volume capturé : contient
/// les sommets du relief et la couche de nuages (8.8 au-dessus du rayon)
const LIGHT_DISTANCE: f32 = 14.0;
const LIGHT_DEPTH: f32 = 28.0;
/// Décalage maximal le long de la normale à la lecture (faces rasantes), en fraction de la
/// demi-largeur : 1 texel. Plus grand, il sortait le haut des faces à l'ombre de leur bloc.
const NORMAL_OFFSET: f32 = 2.0 / CASCADE_SIZE as f32;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowPassUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowUniform {
    cascades: [[[f32; 4]; 4]; CASCADE_COUNT],
    cloud_view_proj: [[f32; 4]; 4],
    extents: [f32; 4],
    params: [f32; 4],
}

/// Une passe de rendu depuis le soleil : sa cible et la matrice qui la projette
struct CasterPass {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    view: wgpu::TextureView,
    center: Vec3,
    extent: f32,
}

pub struct Shadows {
    cascades: Vec<CasterPass>,
    cloud: CasterPass,
    terrain_pipeline: wgpu::RenderPipeline,
    cloud_pipeline: wgpu::RenderPipeline,
    receiver_buffer: wgpu::Buffer,
    pub receiver_layout: wgpu::BindGroupLayout,
    pub receiver_bind_group: wgpu::BindGroup,
    /// Ombres calculées cette frame (au sol, réglage actif, soleil levé)
    active: bool,
    clouds_active: bool,
    light_dir: Vec3,
}

/// Repère orthonormé de la caméra soleil (`right`, `up`, direction vers le soleil)
fn light_basis(light_dir: Vec3) -> (Vec3, Vec3) {
    let reference = if light_dir.y.abs() < 0.95 { Vec3::Y } else { Vec3::X };
    let right = reference.cross(light_dir).normalize();
    let up = light_dir.cross(right);
    (right, up)
}

/// Matrice vue-projection orthographique centrée sur `center`, recalée sur la grille de texels
/// pour que les bords des ombres ne scintillent pas quand le joueur se déplace
fn light_view_proj(center: Vec3, light_dir: Vec3, extent: f32, map_size: u32, scale: f32) -> Mat4 {
    let (right, up) = light_basis(light_dir);
    let texel = 2.0 * extent / map_size as f32;
    let snap = |v: f32| (v / texel).round() * texel;
    let snapped = right * snap(center.dot(right)) + up * snap(center.dot(up)) + light_dir * center.dot(light_dir);
    let eye = snapped + light_dir * LIGHT_DISTANCE * scale;
    let view = Mat4::look_at_rh(eye, snapped, up);
    let proj = Mat4::orthographic_rh(-extent, extent, -extent, extent, 0.0, LIGHT_DEPTH * scale);
    proj * view
}

impl Shadows {
    pub fn new(device: &wgpu::Device, camera_layout: &wgpu::BindGroupLayout) -> Self {
        let pass_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Pass Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let cascade_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Cascades"),
            size: wgpu::Extent3d { width: CASCADE_SIZE, height: CASCADE_SIZE, depth_or_array_layers: CASCADE_COUNT as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let cloud_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cloud Shadow Map"),
            size: wgpu::Extent3d { width: CLOUD_MAP_SIZE, height: CLOUD_MAP_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CLOUD_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let caster_pass = |label: &str, view: wgpu::TextureView, extent: f32| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(&[ShadowPassUniform { view_proj: Mat4::IDENTITY.to_cols_array_2d() }]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &pass_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
            });
            CasterPass { buffer, bind_group, view, center: Vec3::ZERO, extent }
        };
        let cascades = (0..CASCADE_COUNT)
            .map(|i| {
                let view = cascade_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Shadow Cascade Layer"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                });
                caster_pass("Shadow Cascade Pass", view, CASCADE_EXTENTS[i])
            })
            .collect();
        let cloud = caster_pass(
            "Cloud Shadow Pass",
            cloud_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            CLOUD_EXTENT,
        );

        // Pipelines des passes depuis le soleil
        let caster_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Caster Pipeline Layout"),
            bind_group_layouts: &[camera_layout, &pass_layout],
            push_constant_ranges: &[],
        });
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Caster Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow_caster.wgsl").into()),
        });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Shadow Pipeline"),
            layout: Some(&caster_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_shadow"),
                buffers: &[crate::planet_mesh::PlanetVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                // Biais proportionnel à la pente : supprime l'acné sur les faces rasantes
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let cloud_shader = crate::shaders::create_shader_module(device, "Cloud Shadow Shader", include_str!("cloud_shader.wgsl"), "cloud_shader.wgsl");
        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Shadow Pipeline"),
            layout: Some(&caster_layout),
            vertex: wgpu::VertexState {
                module: &cloud_shader,
                entry_point: Some("vs_cloud_shadow"),
                buffers: &[crate::cloud_mesh::CloudVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &cloud_shader,
                entry_point: Some("fs_cloud_shadow"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: CLOUD_FORMAT,
                    // Nuages qui se recouvrent : l'opacité la plus forte l'emporte
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Max,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::RED,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Lecture par le shader du terrain
        let receiver_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shadow Receiver Buffer"),
            contents: bytemuck::cast_slice(&[ShadowUniform::zeroed_disabled()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let fragment_entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let receiver_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Receiver Bind Group Layout"),
            entries: &[
                fragment_entry(0, wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                }),
                fragment_entry(1, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                }),
                fragment_entry(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
                fragment_entry(3, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }),
                fragment_entry(4, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let comparison_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Comparison Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let cascade_array_view = cascade_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Shadow Cascades Array"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let cloud_sample_view = cloud_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let receiver_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Receiver Bind Group"),
            layout: &receiver_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: receiver_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&cascade_array_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&comparison_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&cloud_sample_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&linear_sampler) },
            ],
        });

        Self {
            cascades,
            cloud,
            terrain_pipeline,
            cloud_pipeline,
            receiver_buffer,
            receiver_layout,
            receiver_bind_group,
            active: false,
            clouds_active: false,
            light_dir: Vec3::Y,
        }
    }

    /// Place les cascades autour du joueur (décalées vers l'avant du regard) et met à jour
    /// les matrices. `ground_factor` vaut 0 en orbite, où les ombres sont coupées.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        settings: &ShadowSettings,
        focus: Vec3,
        forward: Vec3,
        sun_dir: Vec3,
        planet_radius: f32,
        ground_factor: f32,
        clouds_visible: bool,
    ) {
        let scale = planet_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;
        let up = focus.normalize_or_zero();
        let sun_up = sun_dir.dot(up);
        self.light_dir = sun_dir.normalize();
        self.active = settings.enabled && ground_factor > 0.0 && sun_up > -0.1;
        self.clouds_active = self.active && settings.cloud_shadows && clouds_visible;
        if !self.active {
            let uniform = ShadowUniform::zeroed_disabled();
            queue.write_buffer(&self.receiver_buffer, 0, bytemuck::cast_slice(&[uniform]));
            return;
        }

        // Regard projeté sur le sol : la moitié de chaque cascade sert devant le joueur
        let flat_forward = (forward - up * forward.dot(up)).normalize_or_zero();
        let mut uniform = ShadowUniform::zeroed_disabled();
        for (i, pass) in self.cascades.iter_mut().enumerate() {
            let extent = CASCADE_EXTENTS[i] * scale;
            pass.extent = extent;
            pass.center = focus + flat_forward * extent * 0.5;
            let vp = light_view_proj(pass.center, self.light_dir, extent, CASCADE_SIZE, scale);
            queue.write_buffer(&pass.buffer, 0, bytemuck::cast_slice(&[ShadowPassUniform { view_proj: vp.to_cols_array_2d() }]));
            uniform.cascades[i] = vp.to_cols_array_2d();
            uniform.extents[i] = extent;
        }
        let cloud_extent = CLOUD_EXTENT * scale;
        self.cloud.extent = cloud_extent;
        self.cloud.center = focus;
        let cloud_vp = light_view_proj(focus, self.light_dir, cloud_extent, CLOUD_MAP_SIZE, scale);
        queue.write_buffer(&self.cloud.buffer, 0, bytemuck::cast_slice(&[ShadowPassUniform { view_proj: cloud_vp.to_cols_array_2d() }]));
        uniform.cloud_view_proj = cloud_vp.to_cols_array_2d();
        uniform.extents[3] = 1.0 / CASCADE_SIZE as f32;
        uniform.params = [
            settings.strength,
            if self.clouds_active { settings.cloud_strength } else { 0.0 },
            1.0,
            NORMAL_OFFSET,
        ];
        queue.write_buffer(&self.receiver_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Encode les passes depuis le soleil. `meshes` donne (centre, rayon, buffers, indices) des
    /// maillages du relief ; seuls ceux qui touchent une cascade y sont dessinés.
    pub fn encode<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        camera_bind_group: &wgpu::BindGroup,
        meshes: impl Iterator<Item = (Vec3, f32, &'a wgpu::Buffer, &'a wgpu::Buffer, u32)> + Clone,
        clouds: Option<(&wgpu::Buffer, &wgpu::Buffer, u32)>,
        timestamps: Option<(&wgpu::QuerySet, u32, u32)>,
    ) {
        let begin_ts = |query_set| wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: timestamps.map(|t| t.1),
            end_of_pass_write_index: None,
        };
        let end_ts = |query_set| wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: timestamps.map(|t| t.2),
        };

        if self.active {
            let (right, up) = light_basis(self.light_dir);
            for (i, pass) in self.cascades.iter().enumerate() {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Shadow Cascade Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &pass.view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: timestamps.filter(|_| i == 0).map(|t| begin_ts(t.0)),
                });
                render_pass.set_pipeline(&self.terrain_pipeline);
                render_pass.set_bind_group(0, camera_bind_group, &[]);
                render_pass.set_bind_group(1, &pass.bind_group, &[]);
                for (center, radius, vertex_buffer, index_buffer, num_indices) in meshes.clone() {
                    // Test dans le plan de la lumière : la profondeur couvre déjà tout le volume
                    let d = center - pass.center;
                    if d.dot(right).abs() > pass.extent + radius || d.dot(up).abs() > pass.extent + radius {
                        continue;
                    }
                    render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..num_indices, 0, 0..1);
                }
            }
        }

        // La carte des nuages est toujours effacée : sa passe porte aussi le timestamp de fin
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cloud Shadow Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.cloud.view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: timestamps.map(|t| {
                let mut writes = end_ts(t.0);
                if !self.active {
                    writes.beginning_of_pass_write_index = Some(t.1);
                }
                writes
            }),
        });
        if let Some((vertex_buffer, index_buffer, num_indices)) = clouds.filter(|_| self.clouds_active) {
            render_pass.set_pipeline(&self.cloud_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.cloud.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..num_indices, 0, 0..1);
        }
    }
}

impl ShadowUniform {
    fn zeroed_disabled() -> Self {
        Self {
            cascades: [Mat4::IDENTITY.to_cols_array_2d(); CASCADE_COUNT],
            cloud_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            extents: [0.0; 4],
            params: [0.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_uniform_layout() {
        // 4 matrices + 2 vec4 : doit correspondre à ShadowUniform dans shader.wgsl
        assert_eq!(std::mem::size_of::<ShadowUniform>(), 4 * 64 + 2 * 16);
    }

    #[test]
    fn test_light_projection_contains_center_and_snaps() {
        let sun = Vec3::new(0.3, 0.8, 0.5).normalize();
        let center = Vec3::new(10.0, 22.0, 5.0);
        let vp = light_view_proj(center, sun, 2.0, 2048, 1.0);
        let ndc = vp.project_point3(center);
        // Le recalage sur la grille ne déplace le centre que d'un texel au plus
        assert!(ndc.x.abs() < 2.0 / 2048.0 + 1e-4 && ndc.y.abs() < 2.0 / 2048.0 + 1e-4, "{ndc:?}");
        assert!(ndc.z > 0.0 && ndc.z < 1.0);
        // Un point plus proche du soleil est moins profond
        let closer = vp.project_point3(center + sun * 1.0);
        assert!(closer.z < ndc.z);
    }
}

use std::sync::Arc;
use winit::window::Window;

use crate::camera::OrbitCamera;
use crate::planet_mesh::{PlanetMesh, PlanetVertex};
use crate::ui::{LightingSettings, UiState};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub planet_radius: f32,
    pub light_dir: [f32; 3],
    pub time: f32,
    pub lighting_params: [f32; 4],
    pub lighting_params2: [f32; 4],
}

pub struct State {
    #[allow(dead_code)]
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    // Pipeline Ciel & Atmosphère 3D
    pub sky_pipeline: wgpu::RenderPipeline,

    // Pipeline Nuages 3D physiques réels (traversables et visibles à leur altitude)
    pub cloud_pipeline: wgpu::RenderPipeline,
    pub cloud_vertex_buffer: wgpu::Buffer,
    pub cloud_index_buffer: wgpu::Buffer,
    pub num_cloud_indices: u32,

    // Pipeline Planète & Voxels 3D
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub sample_count: u32,
    pub msaa_color_texture: Option<wgpu::Texture>,
    pub msaa_color_view: Option<wgpu::TextureView>,
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_buffer_capacity: usize,
    pub index_buffer: wgpu::Buffer,
    pub index_buffer_capacity: usize,
    pub num_indices: u32,

    // Rendu Interface Utilisateur egui
    pub egui_renderer: egui_wgpu::Renderer,

    // Paramètres Vidéo & Menu
    pub vsync_mode: wgpu::PresentMode,
    pub novsync_mode: wgpu::PresentMode,
    pub simulated_time: f32,
    pub last_uniform_time: std::time::Instant,
}

/// Calcule la direction unitaire vers le Soleil en temps réel
/// - Cycle diurne complet (jour / nuit) : 20 minutes (1200 secondes)
/// - Inclinaison de l'écliptique : ~23.4° (0.409 rad)
pub fn compute_sun_dir(time: f32) -> glam::Vec3 {
    let tilt: f32 = 0.409;
    let u_sun = glam::Vec3::new(tilt.cos(), tilt.sin(), 0.0);
    let v_sun = glam::Vec3::new(0.0, 0.0, 1.0);
    let sun_period = 1200.0_f32; // 20 minutes
    let sun_angle = (time / sun_period) * std::f32::consts::TAU + 0.40;
    (u_sun * sun_angle.cos() + v_sun * sun_angle.sin()).normalize()
}

/// Calcule la direction unitaire et la phase de la Lune en temps réel
/// - Période orbitale : 21 minutes (1260 secondes)
/// - Inclinaison orbitale lunaire : ~28.5° (0.497 rad)
/// - Cycle synodique complet (retour à la même phase) : 21 jours de jeu (420 minutes)
#[allow(dead_code)]
pub fn compute_moon_data(time: f32, sun_dir: glam::Vec3) -> (glam::Vec3, f32) {
    let l_sun = sun_dir.normalize();
    let moon_tilt: f32 = 0.497;
    let u_moon = glam::Vec3::new(moon_tilt.cos(), moon_tilt.sin(), 0.0);
    let v_moon = glam::Vec3::new(0.0, 0.0, 1.0);
    let moon_period = 1260.0_f32; // 21 minutes
    let moon_angle = (time / moon_period) * std::f32::consts::TAU + 3.69;
    let moon_dir = (u_moon * moon_angle.cos() + v_moon * moon_angle.sin()).normalize();
    let sun_moon_dot = moon_dir.dot(l_sun);
    let moon_phase = ((-sun_moon_dot + 1.0) * 0.5).clamp(0.0, 1.0);
    (moon_dir, moon_phase)
}

impl State {
    pub async fn new(window: Arc<Window>, planet_mesh: &PlanetMesh, _ui_state: &UiState) -> Self {
        let size = window.inner_size();

        // 1. Instance WGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // 2. Surface
        let surface = instance
            .create_surface(window.clone())
            .expect("Échec de création de la surface WGPU");

        // 3. Adaptateur GPU
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Aucun adaptateur GPU compatible trouvé");

        log::info!("Adaptateur WGPU sélectionné : {:?}", adapter.get_info().name);

        // 4. Device et Queue
        let adapter_limits = adapter.limits();
        let mut required_limits = if adapter_limits.max_compute_workgroups_per_dimension == 0 {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter_limits.clone())
        } else {
            wgpu::Limits::default()
        };
        // Débloque la taille maximale de buffer jusqu'à la capacité matérielle réelle du GPU
        required_limits.max_buffer_size = adapter_limits.max_buffer_size;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    label: Some("HeroCraft Device"),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("Échec d'obtention du device WGPU");

        // 5. Configuration de la surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        // Détection des modes de présentation pour V-Sync activé / désactivé
        let vsync_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::AutoVsync) {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::Fifo
        };

        let novsync_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::AutoNoVsync) {
            wgpu::PresentMode::AutoNoVsync
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            vsync_mode
        };

        let present_mode = vsync_mode;
        log::info!("Mode de présentation WGPU initial (V-Sync) : {:?}", present_mode);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // 6. Shader 3D Planète
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Planet Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        use wgpu::util::DeviceExt;

        // Uniform Camera
        let sun_dir = compute_sun_dir(0.0);
        let initial_uniform = CameraUniform {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 0.0, 165.0],
            planet_radius: 25.0,
            light_dir: sun_dir.to_array(),
            time: 0.0,
            lighting_params: [0.25, 1.0, 2.10, 1.0],
            lighting_params2: [1.0, 1.0, 0.0, 0.0],
        };

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[initial_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("camera_bind_group_layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Détection du support matériel MSAA 4x
        let msaa_flags = adapter.get_texture_format_features(config.format).flags;
        let sample_count = if msaa_flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4) {
            4
        } else {
            1
        };
        log::info!("Anti-Aliasing MSAA 4x : {} (sample_count = {})", sample_count == 4, sample_count);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &config, sample_count);
        let (msaa_color_texture, msaa_color_view) = match Self::create_msaa_color_texture(&device, &config, sample_count) {
            Some((t, v)) => (Some(t), Some(v)),
            None => (None, None),
        };

        // Pipeline Ciel & Atmosphère 3D (Étoiles, Soleil, Halo spatial, Ciel bleu)
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky_shader.wgsl").into()),
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
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

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Planet Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[PlanetVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
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

        let vertex_buffer_capacity = (planet_mesh.vertices.len() * 3 / 2).max(262144);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Planet Vertex Buffer"),
            size: (vertex_buffer_capacity * std::mem::size_of::<PlanetVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        if !planet_mesh.vertices.is_empty() {
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&planet_mesh.vertices));
        }

        let index_buffer_capacity = (planet_mesh.indices.len() * 3 / 2).max(524288);
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Planet Index Buffer"),
            size: (index_buffer_capacity * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        if !planet_mesh.indices.is_empty() {
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&planet_mesh.indices));
        }

        // -------------------------------------------------------------
        // Pipeline Nuages 3D physiques réels (traversables et visibles à leur altitude)
        // -------------------------------------------------------------
        let cloud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cloud_shader.wgsl").into()),
        });

        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &cloud_shader,
                entry_point: Some("vs_cloud"),
                buffers: &[PlanetVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &cloud_shader,
                entry_point: Some("fs_cloud"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Visible des deux côtés pour pouvoir entrer et traverser les nuages
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
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

        let cloud_mesh = crate::cloud_mesh::CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);
        let cloud_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Vertex Buffer"),
            contents: bytemuck::cast_slice(&cloud_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let cloud_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Index Buffer"),
            contents: bytemuck::cast_slice(&cloud_mesh.indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        let num_cloud_indices = cloud_mesh.indices.len() as u32;

        // -------------------------------------------------------------
        // Rendu Interface Utilisateur egui
        // -------------------------------------------------------------
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            None,
            1,
            false,
        );

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            sky_pipeline,
            cloud_pipeline,
            cloud_vertex_buffer,
            cloud_index_buffer,
            num_cloud_indices,
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            depth_texture,
            depth_view,
            sample_count,
            msaa_color_texture,
            msaa_color_view,
            vertex_buffer,
            vertex_buffer_capacity,
            index_buffer,
            index_buffer_capacity,
            num_indices: planet_mesh.indices.len() as u32,
            egui_renderer,
            vsync_mode,
            novsync_mode,
            simulated_time: 0.0,
            last_uniform_time: std::time::Instant::now(),
        }
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_msaa_color_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> Option<(wgpu::Texture, wgpu::TextureView)> {
        if sample_count <= 1 {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MSAA Color Texture"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some((texture, view))
    }


    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, &self.config, self.sample_count);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            let (msaa_texture, msaa_view) = match Self::create_msaa_color_texture(&self.device, &self.config, self.sample_count) {
                Some((t, v)) => (Some(t), Some(v)),
                None => (None, None),
            };
            self.msaa_color_texture = msaa_texture;
            self.msaa_color_view = msaa_view;
        }
    }

    pub fn set_vsync(&mut self, enabled: bool) {
        let target_mode = if enabled {
            self.vsync_mode
        } else {
            self.novsync_mode
        };
        if self.config.present_mode != target_mode {
            self.config.present_mode = target_mode;
            self.surface.configure(&self.device, &self.config);
            log::info!("Mode de présentation WGPU mis à jour : {:?}", target_mode);
        }
    }

    pub fn update_camera(&mut self, camera: &OrbitCamera, lighting: &LightingSettings) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_uniform_time).as_secs_f32();
        self.last_uniform_time = now;
        self.simulated_time += dt * lighting.moon_speed;

        let view_proj = camera.build_view_projection_matrix();
        let inv_view_proj = view_proj.inverse();
        let eye = camera.eye_position();
        let sun_dir = compute_sun_dir(self.simulated_time);

        // En vue satellite (ou lors de la transition sol -> satellite), les nuages disparaissent totalement
        let cloud_vis = if camera.view_mode() == crate::camera::CameraViewMode::SatelliteOrbit {
            0.0_f32
        } else {
            ((25.0 - camera.effective_distance) / 5.0).clamp(0.0, 1.0)
        };
        let effective_cloud_density = lighting.cloud_density * cloud_vis;

        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_pos: eye.to_array(),
            planet_radius: camera.base_radius,
            light_dir: sun_dir.to_array(),
            time: self.simulated_time,
            lighting_params: [
                lighting.moon_intensity,
                lighting.sun_intensity,
                lighting.night_ambient,
                lighting.day_ambient,
            ],
            lighting_params2: [
                lighting.moon_speed,
                lighting.fog_intensity,
                effective_cloud_density,
                lighting.cloud_mode as u32 as f32,
            ],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    #[allow(dead_code)]
    pub fn update_mesh(&mut self, mesh: &PlanetMesh) {
        let max_buf_size = self.device.limits().max_buffer_size;

        if mesh.vertices.len() > self.vertex_buffer_capacity {
            let target_cap = (mesh.vertices.len() * 3 / 2).max(262144);
            let target_bytes = (target_cap * std::mem::size_of::<PlanetVertex>()) as u64;
            let (final_cap, final_bytes) = if target_bytes <= max_buf_size {
                (target_cap, target_bytes)
            } else {
                let max_possible = (max_buf_size as usize) / std::mem::size_of::<PlanetVertex>();
                let clamped_cap = max_possible.min(mesh.vertices.len().max(self.vertex_buffer_capacity));
                (clamped_cap, (clamped_cap * std::mem::size_of::<PlanetVertex>()) as u64)
            };

            self.vertex_buffer_capacity = final_cap;
            self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Planet Vertex Buffer"),
                size: final_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }
        if !mesh.vertices.is_empty() && self.vertex_buffer_capacity > 0 {
            let copy_count = mesh.vertices.len().min(self.vertex_buffer_capacity);
            let vertex_bytes = bytemuck::cast_slice(&mesh.vertices[..copy_count]);
            self.queue.write_buffer(&self.vertex_buffer, 0, vertex_bytes);
        }

        if mesh.indices.len() > self.index_buffer_capacity {
            let target_cap = (mesh.indices.len() * 3 / 2).max(524288);
            let target_bytes = (target_cap * std::mem::size_of::<u32>()) as u64;
            let (final_cap, final_bytes) = if target_bytes <= max_buf_size {
                (target_cap, target_bytes)
            } else {
                let max_possible = (max_buf_size as usize) / std::mem::size_of::<u32>();
                let clamped_cap = max_possible.min(mesh.indices.len().max(self.index_buffer_capacity));
                (clamped_cap, (clamped_cap * std::mem::size_of::<u32>()) as u64)
            };

            self.index_buffer_capacity = final_cap;
            self.index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Planet Index Buffer"),
                size: final_bytes,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }
        if !mesh.indices.is_empty() && self.index_buffer_capacity > 0 {
            let copy_count = mesh.indices.len().min(self.index_buffer_capacity);
            let index_bytes = bytemuck::cast_slice(&mesh.indices[..copy_count]);
            self.queue.write_buffer(&self.index_buffer, 0, index_bytes);
            self.num_indices = copy_count as u32;
        } else {
            self.num_indices = 0;
        }
    }

    pub fn update_cloud_mesh(&mut self, base_radius: f32, seed: u64) {
        use wgpu::util::DeviceExt;
        let cloud_mesh = crate::cloud_mesh::CloudMeshGenerator::generate_cloud_mesh(base_radius, seed);
        self.cloud_vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Vertex Buffer"),
            contents: bytemuck::cast_slice(&cloud_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        self.cloud_index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Index Buffer"),
            contents: bytemuck::cast_slice(&cloud_mesh.indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        self.num_cloud_indices = cloud_mesh.indices.len() as u32;
    }

    pub fn render(
        &mut self,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        textures_delta: &egui::TexturesDelta,
    ) -> Result<(), wgpu::SurfaceError> {
        for (id, delta) in &textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let output = self.surface.get_current_texture()?;
        let swapchain_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // 1. Passe 3D (Ciel étoilé + Soleil + Halo atmosphérique + Planète & Voxels avec MSAA 4x si supporté)
        {
            let (color_view, resolve_target, store_op) = if self.sample_count > 1 {
                (
                    self.msaa_color_view.as_ref().unwrap(),
                    Some(&swapchain_view),
                    wgpu::StoreOp::Discard, // La cible résolue reçoit l'image antialiadée finale
                )
            } else {
                (
                    &swapchain_view,
                    None,
                    wgpu::StoreOp::Store,
                )
            };

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D World Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.01,
                            g: 0.015,
                            b: 0.03,
                            a: 1.0,
                        }),
                        store: store_op,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // 1.a. Ciel étoilé & Soleil & Halo atmosphérique
            render_pass.set_pipeline(&self.sky_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.draw(0..3, 0..1);

            // 1.b. Relief planétaire & Voxels
            if self.num_indices > 0 {
                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }

            // 1.c. Nuages 3D physiques réels (traversables et visibles à leur altitude)
            if self.num_cloud_indices > 0 {
                render_pass.set_pipeline(&self.cloud_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.cloud_vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.cloud_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.num_cloud_indices, 0, 0..1);
            }
        }

        // 2. Passe 2D Interface Utilisateur egui (sample_count = 1 sur la swapchain résolue)
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            paint_jobs,
            screen_descriptor,
        );

        {
            let mut ui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &swapchain_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Conserver le rendu 3D
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            }).forget_lifetime();

            self.egui_renderer.render(&mut ui_pass, paint_jobs, screen_descriptor);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_uniform_size_and_alignment() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 192);
        assert_eq!(std::mem::align_of::<CameraUniform>(), 16);
    }

    #[test]
    fn test_shaders_wgsl_validation() {
        let sky_shader = include_str!("sky_shader.wgsl");
        let terrain_shader = include_str!("shader.wgsl");
        let ui_shader = include_str!("ui_shader.wgsl");
        let cloud_shader = include_str!("cloud_shader.wgsl");

        for (name, source) in [
            ("sky_shader.wgsl", sky_shader),
            ("shader.wgsl", terrain_shader),
            ("ui_shader.wgsl", ui_shader),
            ("cloud_shader.wgsl", cloud_shader),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("Failed to parse {name}:\n{e}"));
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            validator
                .validate(&module)
                .unwrap_or_else(|e| panic!("Failed to validate {name}:\n{e}"));
        }
    }

    #[test]
    fn test_lunar_orbit_mechanics_and_phases() {
        let mut min_phase = 1.0_f32;
        let mut max_phase = 0.0_f32;

        // Période synodique complète : 25200s (21 jours de 1200s, soit 20 révolutions lunaires de 1260s)
        let synodic_period = 25200.0_f32;
        let steps = 420; // Échantillonnage toutes les 60 secondes

        for step in 0..=steps {
            let t = (step as f32) * (synodic_period / steps as f32);
            let sun_dir = compute_sun_dir(t);
            let (moon_dir, phase) = compute_moon_data(t, sun_dir);

            // Vérifie que les vecteurs sont toujours unitaires
            assert!((sun_dir.length() - 1.0).abs() < 1e-5);
            assert!((moon_dir.length() - 1.0).abs() < 1e-5);
            // Vérifie que la phase est bien bornée [0.0, 1.0]
            assert!(phase >= 0.0 && phase <= 1.0);

            if phase < min_phase {
                min_phase = phase;
            }
            if phase > max_phase {
                max_phase = phase;
            }
        }

        // Sur un cycle synodique complet de 21 jours, la Lune doit couvrir de la Nouvelle Lune (~0) à la Pleine Lune (~1)
        assert!(min_phase < 0.05, "La nouvelle lune doit approcher 0.0 (obtenu: {min_phase})");
        assert!(max_phase > 0.95, "La pleine lune doit approcher 1.0 (obtenu: {max_phase})");

        // Première nuit (t = 600s, 10 min de jeu) : pleine lune éclatante
        let sun_first_night = compute_sun_dir(600.0);
        let (_moon_dir_first_night, phase_first_night) = compute_moon_data(600.0, sun_first_night);
        assert!(phase_first_night > 0.95, "La première nuit doit avoir une pleine lune éclatante (obtenu: {phase_first_night})");

        // Nuit du 11ème jour (t = 10.5 jours * 1200s + 600s = 13200s) : nouvelle lune (nuit sans lune)
        let sun_new_moon_night = compute_sun_dir(13200.0);
        let (_moon_dir_new_moon, phase_new_moon) = compute_moon_data(13200.0, sun_new_moon_night);
        assert!(phase_new_moon < 0.05, "La nuit de nouvelle lune doit approcher 0.0 (obtenu: {phase_new_moon})");
    }

    #[test]
    fn test_clouds_disappear_in_satellite_view() {
        use crate::camera::{CameraViewMode, OrbitCamera};
        let desc = herocraft_world::PlanetDescriptor::default();
        let gen = herocraft_world::PlanetWorldGenerator::new(desc);

        // 1. Vue Satellite (distance par défaut = 165.0)
        let mut cam = OrbitCamera::new(165.0, 25.0, &gen);
        assert_eq!(cam.view_mode(), CameraViewMode::SatelliteOrbit);

        let cloud_vis_satellite = if cam.view_mode() == CameraViewMode::SatelliteOrbit {
            0.0_f32
        } else {
            ((25.0 - cam.effective_distance) / 5.0).clamp(0.0, 1.0)
        };
        assert_eq!(cloud_vis_satellite, 0.0, "Les nuages doivent être totalement invisibles en vue satellite");

        // 2. Vue Sol (FPS: distance = 0.0)
        cam.effective_distance = 0.0;
        assert_eq!(cam.view_mode(), CameraViewMode::FirstPerson);
        let cloud_vis_fps = if cam.view_mode() == CameraViewMode::SatelliteOrbit {
            0.0_f32
        } else {
            ((25.0 - cam.effective_distance) / 5.0).clamp(0.0, 1.0)
        };
        assert_eq!(cloud_vis_fps, 1.0, "Les nuages doivent être à 100% visibles au sol en FPS");

        // 3. Vue Sol (TPS: distance = 15.0)
        cam.effective_distance = 15.0;
        assert_eq!(cam.view_mode(), CameraViewMode::ThirdPerson);
        let cloud_vis_tps = if cam.view_mode() == CameraViewMode::SatelliteOrbit {
            0.0_f32
        } else {
            ((25.0 - cam.effective_distance) / 5.0).clamp(0.0, 1.0)
        };
        assert_eq!(cloud_vis_tps, 1.0, "Les nuages doivent être à 100% visibles au sol en TPS");
    }

    #[test]
    fn test_cloud_modes_uniform_transmission() {
        use crate::ui::{CloudRenderMode, LightingSettings};

        let mut lighting = LightingSettings::default();

        // 1. Mode Split-Screen par défaut (0.0)
        assert_eq!(lighting.cloud_mode, CloudRenderMode::SplitScreen);
        assert_eq!(lighting.cloud_mode as u32 as f32, 0.0);

        // 2. Mode A : SDF Volumétrique (1.0)
        lighting.cloud_mode = CloudRenderMode::ModeA;
        assert_eq!(lighting.cloud_mode as u32 as f32, 1.0);

        // 3. Mode B : Mesh Adouci (2.0)
        lighting.cloud_mode = CloudRenderMode::ModeB;
        assert_eq!(lighting.cloud_mode as u32 as f32, 2.0);

        // 4. Mode C : Proxy Raymarché (3.0)
        lighting.cloud_mode = CloudRenderMode::ModeC;
        assert_eq!(lighting.cloud_mode as u32 as f32, 3.0);
    }
}

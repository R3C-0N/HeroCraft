use std::sync::Arc;
use winit::window::Window;

use crate::camera::OrbitCamera;
use crate::planet_mesh::{PlanetMesh, PlanetVertex};
use crate::ui::UiState;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub planet_radius: f32,
    pub light_dir: [f32; 3],
    pub time: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl UiVertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiUniform {
    pub screen_size: [f32; 2],
    pub _pad: [f32; 2],
}

pub struct State {
    #[allow(dead_code)]
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    // Pipeline Planète & Voxels 3D
    pub render_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_buffer_capacity: usize,
    pub index_buffer: wgpu::Buffer,
    pub index_buffer_capacity: usize,
    pub num_indices: u32,

    // Pipeline Interface Utilisateur 2D
    pub ui_pipeline: wgpu::RenderPipeline,
    pub ui_texture: wgpu::Texture,
    pub ui_bind_group: wgpu::BindGroup,
    pub ui_uniform_buffer: wgpu::Buffer,
    pub ui_vertex_buffer: wgpu::Buffer,
    pub ui_index_buffer: wgpu::Buffer,

    // Paramètres Vidéo & Menu
    pub is_settings_open: bool,
    pub num_ui_indices: u32,
    pub vsync_mode: wgpu::PresentMode,
    pub novsync_mode: wgpu::PresentMode,
    pub start_time: std::time::Instant,
}

impl State {
    pub async fn new(window: Arc<Window>, planet_mesh: &PlanetMesh, ui_state: &UiState) -> Self {
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
        let initial_uniform = CameraUniform {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 0.0, 165.0],
            planet_radius: 25.0,
            light_dir: [0.6, 0.8, 0.5],
            time: 0.0,
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
        // Pipeline Interface Utilisateur 2D (Boutons, HUD)
        // -------------------------------------------------------------
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UI Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui_shader.wgsl").into()),
        });

        let ui_texture_desc = wgpu::TextureDescriptor {
            label: Some("UI Canvas Texture"),
            size: wgpu::Extent3d {
                width: ui_state.canvas_width as u32,
                height: ui_state.canvas_height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let ui_texture = device.create_texture(&ui_texture_desc);
        let ui_texture_view = ui_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let ui_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let initial_ui_uniform = UiUniform {
            screen_size: [size.width as f32, size.height as f32],
            _pad: [0.0, 0.0],
        };
        let ui_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UI Uniform Buffer"),
            contents: bytemuck::cast_slice(&[initial_ui_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let ui_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("ui_bind_group_layout"),
            });

        let ui_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &ui_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ui_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&ui_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&ui_sampler),
                },
            ],
            label: Some("ui_bind_group"),
        });

        let ui_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("UI Pipeline Layout"),
                bind_group_layouts: &[&ui_bind_group_layout],
                push_constant_ranges: &[],
            });

        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Render Pipeline"),
            layout: Some(&ui_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ui_shader,
                entry_point: Some("vs_ui"),
                buffers: &[UiVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ui_shader,
                entry_point: Some("fs_ui"),
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Quads 2D de l'UI (Panneau HUD ou Backdrop assombrissant + Compteur FPS + Menu Paramètres)
        let (ui_vertices, num_ui_indices) =
            Self::build_ui_vertices(size.width as f32, size.height as f32, false);

        let ui_indices: [u32; 18] = [
            0, 1, 2, 0, 2, 3,       // Quad 1 : Panneau principal HUD ou Backdrop
            4, 5, 6, 4, 6, 7,       // Quad 2 : Badge FPS en haut à droite
            8, 9, 10, 8, 10, 11,    // Quad 3 : Menu Paramètres Vidéo
        ];

        let ui_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UI Vertex Buffer"),
            contents: bytemuck::cast_slice(&ui_vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let ui_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UI Index Buffer"),
            contents: bytemuck::cast_slice(&ui_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut state = Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            camera_buffer,
            camera_bind_group,
            depth_texture,
            depth_view,
            vertex_buffer,
            vertex_buffer_capacity,
            index_buffer,
            index_buffer_capacity,
            num_indices: planet_mesh.indices.len() as u32,
            ui_pipeline,
            ui_texture,
            ui_bind_group,
            ui_uniform_buffer,
            ui_vertex_buffer,
            ui_index_buffer,
            is_settings_open: false,
            num_ui_indices,
            vsync_mode,
            novsync_mode,
            start_time: std::time::Instant::now(),
        };

        // Chargement initial des pixels de l'UI
        state.update_ui(ui_state);
        state
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
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
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Génère les sommets des quads de l'interface utilisateur 2D :
    /// - Mode Jeu : Quad 1 (Panneau HUD gauche) + Quad 2 (Badge FPS)
    /// - Mode Menu : Quad 1 (Backdrop plein écran sombre) + Quad 2 (Badge FPS) + Quad 3 (Fenêtre centrale Paramètres)
    pub fn build_ui_vertices(screen_w: f32, screen_h: f32, is_settings_open: bool) -> ([UiVertex; 12], u32) {
        let cw = 512.0_f32;
        let ch = 512.0_f32;

        let fps_w = 116.0_f32;
        let fps_h = 32.0_f32;
        let fps_x0 = (screen_w - fps_w - 16.0).max(16.0);
        let fps_y0 = 16.0_f32;

        if !is_settings_open {
            let panel_w = 320.0_f32;
            let panel_h = 310.0_f32;

            let vertices = [
                // Quad 1: Panneau principal HUD (Haut-Gauche)
                UiVertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
                UiVertex { position: [panel_w, 0.0], uv: [panel_w / cw, 0.0] },
                UiVertex { position: [panel_w, panel_h], uv: [panel_w / cw, panel_h / ch] },
                UiVertex { position: [0.0, panel_h], uv: [0.0, panel_h / ch] },

                // Quad 2: Badge FPS (Haut-Droite)
                UiVertex { position: [fps_x0, fps_y0], uv: [320.0 / cw, 0.0] },
                UiVertex { position: [fps_x0 + fps_w, fps_y0], uv: [(320.0 + fps_w) / cw, 0.0] },
                UiVertex { position: [fps_x0 + fps_w, fps_y0 + fps_h], uv: [(320.0 + fps_w) / cw, fps_h / ch] },
                UiVertex { position: [fps_x0, fps_y0 + fps_h], uv: [320.0 / cw, fps_h / ch] },

                // Quad 3: Inactif (zéroté)
                UiVertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
                UiVertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
                UiVertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
                UiVertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
            ];
            (vertices, 12)
        } else {
            // Backdrop foncé plein écran : UV pointe vers le pixel sombre en (511, 511)
            let dark_u = 511.0 / cw;
            let dark_v = 511.0 / ch;

            let menu_w = 380.0_f32;
            let menu_h = 400.0_f32;
            let menu_x = ((screen_w - menu_w) * 0.5).max(0.0);
            let menu_y = ((screen_h - menu_h) * 0.5).max(0.0);

            let vertices = [
                // Quad 1: Fond assombrissant plein écran (Backdrop)
                UiVertex { position: [0.0, 0.0], uv: [dark_u, dark_v] },
                UiVertex { position: [screen_w, 0.0], uv: [dark_u, dark_v] },
                UiVertex { position: [screen_w, screen_h], uv: [dark_u, dark_v] },
                UiVertex { position: [0.0, screen_h], uv: [dark_u, dark_v] },

                // Quad 2: Badge FPS (Haut-Droite)
                UiVertex { position: [fps_x0, fps_y0], uv: [320.0 / cw, 0.0] },
                UiVertex { position: [fps_x0 + fps_w, fps_y0], uv: [(320.0 + fps_w) / cw, 0.0] },
                UiVertex { position: [fps_x0 + fps_w, fps_y0 + fps_h], uv: [(320.0 + fps_w) / cw, fps_h / ch] },
                UiVertex { position: [fps_x0, fps_y0 + fps_h], uv: [320.0 / cw, fps_h / ch] },

                // Quad 3: Fenêtre centrale du Menu Paramètres Vidéo
                UiVertex { position: [menu_x, menu_y], uv: [0.0, 0.0] },
                UiVertex { position: [menu_x + menu_w, menu_y], uv: [menu_w / cw, 0.0] },
                UiVertex { position: [menu_x + menu_w, menu_y + menu_h], uv: [menu_w / cw, menu_h / ch] },
                UiVertex { position: [menu_x, menu_y + menu_h], uv: [0.0, menu_h / ch] },
            ];
            (vertices, 18)
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, &self.config);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            // Mise à jour de la taille écran pour l'UI
            let ui_uniform = UiUniform {
                screen_size: [new_size.width as f32, new_size.height as f32],
                _pad: [0.0, 0.0],
            };
            self.queue.write_buffer(
                &self.ui_uniform_buffer,
                0,
                bytemuck::cast_slice(&[ui_uniform]),
            );

            self.update_ui_geometry();
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

    pub fn set_settings_open(&mut self, is_open: bool) {
        if self.is_settings_open != is_open {
            self.is_settings_open = is_open;
            self.update_ui_geometry();
        }
    }

    pub fn update_ui_geometry(&mut self) {
        let (ui_vertices, num_indices) =
            Self::build_ui_vertices(self.size.width as f32, self.size.height as f32, self.is_settings_open);
        self.num_ui_indices = num_indices;
        self.queue.write_buffer(
            &self.ui_vertex_buffer,
            0,
            bytemuck::cast_slice(&ui_vertices),
        );
    }

    pub fn update_camera(&mut self, camera: &OrbitCamera) {
        let view_proj = camera.build_view_projection_matrix();
        let inv_view_proj = view_proj.inverse();
        let eye = camera.eye_position();
        let uniform = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_pos: eye.to_array(),
            planet_radius: camera.base_radius,
            light_dir: [0.6, 0.8, 0.5],
            time: self.start_time.elapsed().as_secs_f32(),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn update_ui(&mut self, ui_state: &UiState) {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.ui_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &ui_state.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(ui_state.canvas_width as u32 * 4),
                rows_per_image: Some(ui_state.canvas_height as u32),
            },
            wgpu::Extent3d {
                width: ui_state.canvas_width as u32,
                height: ui_state.canvas_height as u32,
                depth_or_array_layers: 1,
            },
        );
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

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // 1. Passe 3D Planète (Fond spatial sombre + test de profondeur)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Planet Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.03,
                            b: 0.06,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
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

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        // 2. Passe 2D Interface Utilisateur (Alpha Blending, sans test de profondeur)
        {
            let mut ui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Conserver le rendu 3D
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            ui_pass.set_pipeline(&self.ui_pipeline);
            ui_pass.set_bind_group(0, &self.ui_bind_group, &[]);
            ui_pass.set_vertex_buffer(0, self.ui_vertex_buffer.slice(..));
            ui_pass.set_index_buffer(self.ui_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            ui_pass.draw_indexed(0..self.num_ui_indices, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

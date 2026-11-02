use std::collections::HashMap;
use std::sync::Arc;
use winit::window::Window;

use crate::atmosphere::Atmosphere;
use crate::block_cracks::BlockCracks;
use crate::block_outline::BlockOutline;
use crate::camera::{CameraViewMode, OrbitCamera};
use crate::creature_render::CreatureRenderer;
use crate::gpu_timer::{self, GpuTimer};
use crate::lod::{LodMeshId, PlanetLodManager};
use crate::block_textures::BlockTextureBinding;
use crate::planet_mesh::PlanetVertex;
use crate::post::{self, PostProcess};
use crate::shadow::Shadows;
use crate::ui::{AtmosphereSettings, CloudSettings, LightingSettings, PostSettings, ShadowSettings, UiState};

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
    pub cloud_params: [f32; 4],
    /// Aplatissement de la planète vue du sol (common.wgsl, `flatten_point`) : x = surcroît de
    /// rayon du globe apparent, (k - 1) × rayon de la mer, y = k (0 et 1 : vraie courbure),
    /// z et w = distances au sol depuis la caméra où il commence et devient complet.
    pub flatten: [f32; 4],
}

/// Plus haut relief (en blocs au-dessus de la mer) pris en compte pour borner l'aplatissement
const FLATTEN_MOUNTAIN_BLOCKS: f32 = 150.0;

/// Distance au sol (en blocs) sur laquelle l'aplatissement passe de rien à complet
const FLATTEN_BLEND_BLOCKS: f32 = 16.0;

/// Altitudes de l'œil au-dessus de la mer (en blocs) entre lesquelles l'aplatissement s'efface :
/// complet en dessous, vraie courbure au-dessus. Vue de haut, la planète entière est visible et
/// n'a plus besoin d'être aplatie : le dézoom vers l'orbite la ramène doucement à sa forme.
const FLATTEN_FADE_BLOCKS: (f32, f32) = (40.0, 400.0);

/// Recul de la caméra en troisième personne (en blocs) pris en compte dans la zone laissée
/// intacte autour d'elle : au-delà, la zone grandirait avec le recul et l'aplatissement, alors
/// déjà fort à son bord, s'y ferait d'un coup (relief étiré en traînées)
const FLATTEN_ARM_BLOCKS: f32 = 4.0;

/// Rayon apparent de la planète vue du sol, en multiple de son vrai rayon (k >= 1).
///
/// Le relief est redessiné sur un globe k fois plus grand, tangent à la vraie planète sous la
/// caméra : l'horizon recule de √k. Toute la planète tient alors dans une calotte de ce grand
/// globe, dont le bord (les antipodes) ne doit jamais se voir : la distance de l'horizon depuis
/// l'œil, plus celle d'où l'on voit encore un sommet au-delà, reste sous 0,9 demi-tour de la vraie
/// planète. k décroît donc avec l'altitude, et s'efface tout à fait entre 40 et 400 blocs
/// au-dessus de la mer (`FLATTEN_FADE_BLOCKS`). `strength` (0..1) va de la vraie courbure (0) à
/// ce maximum (1), en progression géométrique.
pub fn planet_flattening(eye_radius: f32, base_radius: f32, strength: f32) -> f32 {
    let block = base_radius * std::f32::consts::FRAC_PI_4 * (2.0 / 1024.0);
    let sea_radius = base_radius + herocraft_world::SEA_LEVEL_BLOCKS as f32 * block;
    // Au moins la hauteur des yeux : k varierait sinon très vite au ras de l'eau
    let h = (eye_radius - sea_radius).max(2.0 * block);
    let reach = h.sqrt() + (FLATTEN_MOUNTAIN_BLOCKS * block).sqrt();
    // (0,9 π)² / 2 ≈ 4 : √(2 k R h) + √(2 k R H) <= 0,9 π R
    let k_max = (4.0 * sea_radius / (reach * reach)).max(1.0);
    let (fade_low, fade_high) = FLATTEN_FADE_BLOCKS;
    let t = ((h / block - fade_low) / (fade_high - fade_low)).clamp(0.0, 1.0);
    let fade = 1.0 - t * t * (3.0 - 2.0 * t);
    k_max.powf(strength.clamp(0.0, 1.0) * fade)
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    last_used_sync: u64,
    /// Centre de la boîte englobante, pour trier les draws du plus proche au plus lointain
    center: glam::Vec3,
    /// Demi-diagonale de la boîte englobante (tri des maillages par cascade d'ombre)
    radius: f32,
}

fn mesh_bounds(vertices: &[PlanetVertex]) -> (glam::Vec3, f32) {
    let (min, max) = vertices.iter().fold(
        (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)),
        |(lo, hi), v| {
            let p = glam::Vec3::from(v.position);
            (lo.min(p), hi.max(p))
        },
    );
    ((min + max) * 0.5, (max - min).length() * 0.5)
}

/// Octets envoyés au GPU au plus par synchronisation (un gros ensemble est étalé sur plusieurs frames)
const LOD_UPLOAD_BUDGET: usize = 6 * 1024 * 1024;

/// Nombre de synchronisations après lequel un maillage non affiché est libéré du GPU
const LOD_GPU_MESH_TTL: u64 = 120;

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
    /// Cible HDR de la passe 3D, bloom et composition vers la swapchain
    post: PostProcess,
    bloom_enabled: bool,
    /// Ombres portées du relief et des nuages, calculées depuis le soleil avant la passe 3D
    shadows: Shadows,
    /// Atmosphère physique (tables précalculées) et rayons crépusculaires
    atmosphere: Atmosphere,
    sun_dir: glam::Vec3,
    ground_factor: f32,
    clouds_visible: bool,
    /// Un couple de buffers par maillage de feuille LOD, envoyé une seule fois au GPU
    lod_gpu_meshes: HashMap<LodMeshId, GpuMesh>,
    /// Maillages à dessiner cette frame, dans l'ordre des feuilles
    lod_draw_list: Vec<LodMeshId>,
    camera_eye: glam::Vec3,
    lod_sync_count: u64,
    lod_uploads_pending: bool,
    /// Contour du bloc visé
    block_outline: BlockOutline,
    /// Fissures du bloc en train de casser
    block_cracks: BlockCracks,
    /// Silhouettes des autres joueurs en ligne
    player_outlines: BlockOutline,
    /// Créatures : maillage refait à chaque image, textures des espèces
    creatures: CreatureRenderer,
    /// Blocs tombés au sol : petits cubes dessinés avec le relief, refaits à chaque frame
    dropped_items: GrowableMesh,
    /// Textures des blocs, lues par le shader du terrain (groupe 2)
    block_textures: BlockTextureBinding,

    // Rendu Interface Utilisateur egui
    pub egui_renderer: egui_wgpu::Renderer,

    // Paramètres Vidéo & Menu
    pub vsync_mode: wgpu::PresentMode,
    pub novsync_mode: wgpu::PresentMode,
    pub simulated_time: f32,
    pub last_uniform_time: std::time::Instant,

    // Mesure du temps GPU par passe (feature `profiling` uniquement)
    gpu_timer: Option<GpuTimer>,
    view_mode: CameraViewMode,
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
    pub async fn new(window: Arc<Window>, _ui_state: &UiState) -> Self {
        herocraft_common::prof_scope!("render::State::new");
        let size = window.inner_size();
        crate::benchmark::set_meta("Résolution", format!("{} × {}", size.width, size.height));

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
        let adapter_info = adapter.get_info();
        herocraft_common::profiler::set_meta(
            "gpu",
            format!("{} ({:?}, {:?}, pilote {} {})", adapter_info.name, adapter_info.backend, adapter_info.device_type, adapter_info.driver, adapter_info.driver_info),
        );
        crate::benchmark::set_meta(
            "Carte graphique",
            format!("{} ({:?}, {:?}, pilote {} {})", adapter_info.name, adapter_info.backend, adapter_info.device_type, adapter_info.driver, adapter_info.driver_info),
        );

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
                    required_features: GpuTimer::wanted_features(adapter.features()),
                    required_limits,
                    label: Some("HeroCraft Device"),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("Échec d'obtention du device WGPU");
        let gpu_timer = GpuTimer::new(&device, &queue);

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
        herocraft_common::profiler::set_meta("present_mode_initial", format!("{:?}", present_mode));

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
        let shader = crate::shaders::create_shader_module(&device, "Planet Shader", include_str!("shader.wgsl"), "shader.wgsl");

        use wgpu::util::DeviceExt;

        // Uniform Camera
        let sun_dir = compute_sun_dir(0.0);
        let default_radius = (herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32;
        let initial_uniform = CameraUniform {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            inv_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0, 0.0, default_radius * 6.6],
            planet_radius: default_radius,
            light_dir: sun_dir.to_array(),
            time: 0.0,
            lighting_params: [0.25, 1.0, 2.10, 1.0],
            lighting_params2: [1.0, 1.0, 0.0, 0.0],
            cloud_params: [0.45, 1.0, 1.0, 0.0],
            flatten: [0.0, 1.0, 0.0, 0.0],
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

        // Détection du support matériel MSAA 4x sur le format HDR de la passe 3D
        let sample_count = PostProcess::sample_count(&adapter);
        log::info!("Anti-Aliasing MSAA 4x : {} (sample_count = {})", sample_count == 4, sample_count);
        herocraft_common::profiler::set_meta("msaa_sample_count", sample_count.to_string());

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &config, sample_count);
        let post = PostProcess::new(&device, &config, sample_count);

        // Groupe 0 du ciel, du terrain et des nuages : caméra et tables de l'atmosphère.
        // Le terrain lit en plus les cartes d'ombre (groupe 1).
        let shadows = Shadows::new(&device, &camera_bind_group_layout);
        let atmosphere = Atmosphere::new(&device, &camera_buffer, &shadows.receiver_layout, &depth_view, sample_count, (config.width, config.height));
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&atmosphere.scene_layout],
                push_constant_ranges: &[],
            });
        let creatures = CreatureRenderer::new(&device, &queue, &atmosphere.scene_layout, &shadows.receiver_layout, sample_count);
        // Le terrain lit aussi les textures des blocs (groupe 2)
        let block_textures = crate::block_textures::block_textures().upload(&device, &queue);
        let terrain_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Terrain Pipeline Layout"),
                bind_group_layouts: &[&atmosphere.scene_layout, &shadows.receiver_layout, &block_textures.layout],
                push_constant_ranges: &[],
            });

        // Pipeline Ciel & Atmosphère 3D (Étoiles, Soleil, Halo spatial, Ciel bleu)
        let sky_shader = crate::shaders::create_shader_module(&device, "Sky Shader", include_str!("sky_shader.wgsl"), "sky_shader.wgsl");

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
                    format: post::HDR_FORMAT,
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
            // Dessiné après le terrain, seulement sur les pixels restés à la profondeur du fond :
            // la marche dans l'atmosphère n'est pas calculée sous le relief
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

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Planet Render Pipeline"),
            layout: Some(&terrain_pipeline_layout),
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
                    format: post::HDR_FORMAT,
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

        // -------------------------------------------------------------
        // Pipeline Nuages 3D physiques réels (traversables et visibles à leur altitude)
        // -------------------------------------------------------------
        let cloud_shader = crate::shaders::create_shader_module(&device, "Cloud Shader", include_str!("cloud_shader.wgsl"), "cloud_shader.wgsl");

        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &cloud_shader,
                entry_point: Some("vs_cloud"),
                buffers: &[crate::cloud_mesh::CloudVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &cloud_shader,
                entry_point: Some("fs_cloud"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: post::HDR_FORMAT,
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

        let cloud_mesh = crate::cloud_mesh::CloudMeshGenerator::generate_cloud_mesh(default_radius, 2026);
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
        let block_outline = BlockOutline::new(&device, &render_pipeline_layout, sample_count);
        let block_cracks = BlockCracks::new(&device, &render_pipeline_layout, sample_count);
        let dropped_items = GrowableMesh::new(&device, "Dropped Items");
        let player_outlines =
            BlockOutline::players(&device, &render_pipeline_layout, sample_count, crate::network::MAX_REMOTE_PLAYERS);

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
            post,
            bloom_enabled: true,
            shadows,
            atmosphere,
            sun_dir,
            ground_factor: 0.0,
            clouds_visible: false,
            lod_gpu_meshes: HashMap::new(),
            lod_draw_list: Vec::new(),
            camera_eye: glam::Vec3::ZERO,
            lod_sync_count: 0,
            lod_uploads_pending: false,
            block_outline,
            block_cracks,
            player_outlines,
            creatures,
            dropped_items,
            block_textures,
            egui_renderer,
            vsync_mode,
            novsync_mode,
            simulated_time: 0.0,
            last_uniform_time: std::time::Instant::now(),
            gpu_timer,
            view_mode: CameraViewMode::SatelliteOrbit,
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

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        herocraft_common::prof_scope!("render::resize");
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            crate::benchmark::set_meta("Résolution", format!("{} × {}", new_size.width, new_size.height));
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, &self.config, self.sample_count);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;

            self.post.resize(&self.device, &self.config);
            self.atmosphere.resize(&self.device, &self.depth_view, (self.config.width, self.config.height));
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
            herocraft_common::profiler::set_meta("present_mode_final", format!("{:?}", target_mode));
            crate::benchmark::set_meta("Présentation", format!("{:?}", target_mode));
        }
    }

    pub fn update_camera(&mut self, camera: &OrbitCamera, lighting: &LightingSettings, clouds: &CloudSettings, flatten_strength: f32) {
        herocraft_common::prof_scope!("render::update_camera");
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_uniform_time).as_secs_f32();
        self.last_uniform_time = now;
        self.view_mode = camera.view_mode();
        self.simulated_time += dt * lighting.moon_speed;

        let view_proj = camera.build_view_projection_matrix();
        let inv_view_proj = view_proj.inverse();
        let eye = camera.eye_position();
        let sun_dir = compute_sun_dir(self.simulated_time);
        self.camera_eye = eye;

        // Facteur de présence au sol (1.0 = plein sol FPS/TPS, 0.0 = orbite spatiale profonde)
        let ground_factor = if camera.view_mode() != crate::camera::CameraViewMode::SatelliteOrbit {
            1.0_f32
        } else {
            let t_orbit = ((camera.effective_distance - camera.base_radius) / (camera.base_radius * 1.8)).clamp(0.0, 1.0);
            1.0 - t_orbit
        };

        // En vue satellite (ou lors de la transition sol -> satellite), les nuages disparaissent totalement
        let cloud_vis = if camera.view_mode() == crate::camera::CameraViewMode::SatelliteOrbit {
            0.0_f32
        } else {
            let fade_dist = (camera.base_radius * 0.2).max(1.0);
            ((camera.base_radius - camera.effective_distance) / fade_dist).clamp(0.0, 1.0)
        };
        let effective_cloud_density = clouds.density * cloud_vis;
        self.sun_dir = sun_dir;
        self.ground_factor = ground_factor;
        self.clouds_visible = effective_cloud_density > 0.01;

        let k = planet_flattening(eye.length(), camera.base_radius, flatten_strength);
        let block = camera.base_radius * std::f32::consts::FRAC_PI_4 * (2.0 / 1024.0);
        let sea_radius = camera.base_radius + herocraft_world::SEA_LEVEL_BLOCKS as f32 * block;
        // Rien n'est déplacé à portée de visée, comptée depuis la tête du joueur (la caméra est
        // derrière elle en troisième personne)
        let flatten_start = (crate::block_interaction::REACH_BLOCKS as f32 + 2.0) * block
            + camera.effective_distance.min(FLATTEN_ARM_BLOCKS * block);

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
                ground_factor,
            ],
            cloud_params: [
                clouds.roundness,
                clouds.shadow_intensity,
                clouds.sunset_glow,
                clouds.micro_noise,
            ],
            flatten: [(k - 1.0) * sea_radius, k, flatten_start, flatten_start + FLATTEN_BLEND_BLOCKS * block],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Cascades centrées sur le joueur, à appeler après `update_camera`
    pub fn update_shadows(&mut self, camera: &OrbitCamera, settings: &ShadowSettings) {
        let (_, _, _, forward) = camera.look_vectors();
        self.shadows.update(
            &self.queue,
            settings,
            camera.player_pos,
            forward,
            self.sun_dir,
            camera.base_radius,
            self.ground_factor,
            self.clouds_visible,
        );
    }

    /// Paramètres de l'atmosphère, à appeler après `update_shadows` (les rayons suivent les ombres)
    pub fn update_atmosphere(&mut self, camera: &OrbitCamera, settings: &AtmosphereSettings, lighting: &LightingSettings) {
        self.atmosphere.update(&self.queue, settings, lighting, camera.base_radius, self.shadows.is_active());
    }

    pub fn update_post(&mut self, settings: &PostSettings) {
        self.bloom_enabled = settings.bloom_enabled && settings.bloom_intensity > 0.0;
        self.post.update(&self.queue, settings);
    }

    /// Synchronise les buffers GPU avec les feuilles LOD affichées : seuls les nouveaux maillages
    /// sont envoyés, ceux qui ne sont plus affichés sont libérés après `LOD_GPU_MESH_TTL` synchronisations.
    /// Les envois sont limités à `LOD_UPLOAD_BUDGET` octets par appel : tant que le nouvel ensemble
    /// n'est pas entièrement sur le GPU, l'ancien reste affiché (voir `has_pending_lod_uploads`).
    pub fn sync_lod_meshes(&mut self, lod: &PlanetLodManager) {
        use wgpu::util::DeviceExt;
        herocraft_common::prof_scope!("render::sync_lod_meshes");
        self.lod_sync_count += 1;
        let sync = self.lod_sync_count;
        let mut uploaded_bytes = 0usize;
        // Premier affichage : rien à garder à l'écran, tout est envoyé d'un coup
        let budget = if self.lod_draw_list.is_empty() { usize::MAX } else { LOD_UPLOAD_BUDGET };
        let mut new_list = Vec::new();
        let mut complete = true;

        for (id, mesh) in lod.visible_meshes() {
            if mesh.indices.is_empty() {
                continue;
            }
            if let Some(gpu_mesh) = self.lod_gpu_meshes.get_mut(&id) {
                gpu_mesh.last_used_sync = sync;
            } else if uploaded_bytes < budget {
                uploaded_bytes += mesh.vertices.len() * std::mem::size_of::<PlanetVertex>() + mesh.indices.len() * 4;
                let (center, radius) = mesh_bounds(&mesh.vertices);
                let gpu_mesh = GpuMesh {
                    vertex_buffer: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("LOD Vertex Buffer"),
                        contents: bytemuck::cast_slice(&mesh.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    index_buffer: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("LOD Index Buffer"),
                        contents: bytemuck::cast_slice(&mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    num_indices: mesh.indices.len() as u32,
                    last_used_sync: sync,
                    center,
                    radius,
                };
                self.lod_gpu_meshes.insert(id, gpu_mesh);
            } else {
                complete = false;
            }
            new_list.push(id);
        }

        if complete {
            self.lod_draw_list = new_list;
        } else {
            // Ensemble incomplet : l'ancien reste affiché, ses maillages sont protégés
            for id in &self.lod_draw_list {
                if let Some(m) = self.lod_gpu_meshes.get_mut(id) {
                    m.last_used_sync = sync;
                }
            }
        }
        self.lod_uploads_pending = !complete;

        let generation = lod.generation();
        self.lod_gpu_meshes.retain(|id, m| {
            (id.generation == generation || m.last_used_sync == sync) && sync - m.last_used_sync <= LOD_GPU_MESH_TTL
        });
        herocraft_common::prof_value!("render.upload_octets", uploaded_bytes);
        herocraft_common::prof_value!("render.draws_terrain", self.lod_draw_list.len());
        herocraft_common::prof_value!("render.maillages_gpu_en_memoire", self.lod_gpu_meshes.len());
    }

    /// Coins du bloc visé à cerner (ou rien), à appeler après `update_camera`
    pub fn set_block_outline(&mut self, corners: Option<[glam::Vec3; 8]>) {
        self.block_outline.update(&self.queue, corners.as_slice(), self.camera_eye);
    }

    /// Bloc en train de casser (ses coins et l'avancement de 0 à 1), à appeler après `update_camera`
    pub fn set_block_cracks(&mut self, cracks: Option<([glam::Vec3; 8], f32)>) {
        self.block_cracks.update(&self.queue, cracks, self.camera_eye);
    }

    /// Silhouettes des autres joueurs (8 coins chacune), à appeler après `update_camera`
    pub fn set_player_outlines(&mut self, boxes: &[[glam::Vec3; 8]]) {
        self.player_outlines.update(&self.queue, boxes, self.camera_eye);
    }

    /// Textures des espèces de créatures, dans l'ordre du registre
    pub fn set_creature_textures(&mut self, textures: &[Option<crate::skinned_model::Texture>]) {
        self.creatures.set_textures(&self.device, &self.queue, textures);
    }

    /// Maillage des créatures de l'image (`creatures::build_frame`)
    pub fn set_creature_frame(&mut self, frame: &crate::creatures::CreatureFrame) {
        self.creatures.upload(&self.device, &self.queue, frame);
    }

    /// Cubes des blocs tombés au sol, dans le format du relief
    pub fn set_dropped_items(&mut self, mesh: &crate::planet_mesh::PlanetMesh) {
        self.dropped_items.upload(&self.device, &self.queue, mesh);
    }

    /// Des maillages LOD restent à envoyer : appeler `sync_lod_meshes` à la prochaine frame
    pub fn has_pending_lod_uploads(&self) -> bool {
        self.lod_uploads_pending
    }

    /// Direction du Soleil à la dernière image (jour ou nuit pour l'ambiance sonore)
    pub fn sun_dir(&self) -> glam::Vec3 {
        self.sun_dir
    }

    pub fn update_cloud_mesh(&mut self, base_radius: f32, seed: u64) {
        herocraft_common::prof_scope!("render::update_cloud_mesh");
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
        herocraft_common::prof_scope!("render::render");
        {
            herocraft_common::prof_scope!("render::egui_textures");
            for (id, delta) in &textures_delta.set {
                self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
            }
        }

        // Bloque si le GPU est en retard ou en attente de V-Sync
        let output = {
            herocraft_common::prof_scope!("render::acquire_frame");
            self.surface.get_current_texture()?
        };
        let swapchain_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        if let Some(timer) = self.gpu_timer.as_mut() {
            timer.begin_frame(&self.device, self.view_mode);
        }
        let timer_queries = self.gpu_timer.as_ref().and_then(|t| t.query_set());
        let timer_inside_passes = self.gpu_timer.as_ref().is_some_and(|t| t.inside_passes);

        // 0. Tables de l'atmosphère (seulement si la planète ou les réglages ont changé), puis
        // ombres : cascades du relief et carte des nuages, vues depuis le soleil
        self.atmosphere.encode_luts(&mut encoder);
        {
            herocraft_common::prof_scope!("render::encode_shadows");
            // Les créatures portent leur ombre comme le relief
            let meshes = self
                .lod_draw_list
                .iter()
                .filter_map(|id| self.lod_gpu_meshes.get(id))
                .map(|m| (m.center, m.radius, &m.vertex_buffer, &m.index_buffer, m.num_indices))
                .chain(self.creatures.shadow_caster());
            let clouds = (self.num_cloud_indices > 0)
                .then_some((&self.cloud_vertex_buffer, &self.cloud_index_buffer, self.num_cloud_indices));
            let timestamps = timer_queries.map(|q| (q, gpu_timer::TS_SHADOW_BEGIN, gpu_timer::TS_SHADOW_END));
            self.shadows.encode(&mut encoder, &self.camera_bind_group, meshes, clouds, timestamps);
        }

        // 1. Passe 3D HDR (Ciel étoilé + Soleil + Halo atmosphérique + Planète & Voxels avec MSAA 4x si supporté)
        {
            herocraft_common::prof_scope!("render::encode_3d_pass");
            let (color_view, resolve_target) = self.post.scene_attachment();
            // Avec MSAA, seule la cible HDR résolue est relue par le post-traitement
            let store_op = if resolve_target.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store };

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
                timestamp_writes: timer_queries.map(|query_set| wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(gpu_timer::TS_3D_BEGIN),
                    end_of_pass_write_index: Some(gpu_timer::TS_3D_END),
                }),
            });

            // 1.a. Relief planétaire & Voxels, du plus proche au plus lointain : le test de
            // profondeur précoce élimine les fragments cachés avant d'exécuter le shader
            let (meshes, eye) = (&self.lod_gpu_meshes, self.camera_eye);
            self.lod_draw_list.sort_by_cached_key(|id| {
                meshes.get(id).map_or(u32::MAX, |m| m.center.distance_squared(eye).to_bits())
            });
            if !self.lod_draw_list.is_empty() {
                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.atmosphere.scene_bind_group, &[]);
                render_pass.set_bind_group(1, &self.shadows.receiver_bind_group, &[]);
                render_pass.set_bind_group(2, &self.block_textures.bind_group, &[]);
                for id in &self.lod_draw_list {
                    if let Some(mesh) = self.lod_gpu_meshes.get(id) {
                        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
                    }
                }
            }
            // Créatures, éclairées comme le relief
            self.creatures.draw(&mut render_pass, &self.atmosphere.scene_bind_group, &self.shadows.receiver_bind_group);
            // Blocs tombés au sol : même pipeline que le relief (éclairage, ombres reçues)
            if self.dropped_items.num_indices > 0 {
                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.atmosphere.scene_bind_group, &[]);
                render_pass.set_bind_group(1, &self.shadows.receiver_bind_group, &[]);
                render_pass.set_bind_group(2, &self.block_textures.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.dropped_items.vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.dropped_items.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.dropped_items.num_indices, 0, 0..1);
            }
            // Fissures puis contour du bloc visé, sur la profondeur du relief
            self.block_cracks.draw(&mut render_pass, &self.atmosphere.scene_bind_group);
            self.block_outline.draw(&mut render_pass, &self.atmosphere.scene_bind_group);
            self.player_outlines.draw(&mut render_pass, &self.atmosphere.scene_bind_group);
            if let Some(query_set) = timer_queries.filter(|_| timer_inside_passes) {
                render_pass.write_timestamp(query_set, gpu_timer::TS_AFTER_TERRAIN);
            }

            // 1.b. Ciel : atmosphère, Soleil, Lune et étoiles sur les pixels non couverts
            render_pass.set_pipeline(&self.sky_pipeline);
            render_pass.set_bind_group(0, &self.atmosphere.scene_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
            if let Some(query_set) = timer_queries.filter(|_| timer_inside_passes) {
                render_pass.write_timestamp(query_set, gpu_timer::TS_AFTER_SKY);
            }

            // 1.c. Nuages 3D physiques réels (traversables et visibles à leur altitude)
            if self.num_cloud_indices > 0 {
                render_pass.set_pipeline(&self.cloud_pipeline);
                render_pass.set_bind_group(0, &self.atmosphere.scene_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.cloud_vertex_buffer.slice(..));
                render_pass.set_index_buffer(self.cloud_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.num_cloud_indices, 0, 0..1);
            }
        }

        // 1.d. Rayons crépusculaires : l'air à l'ombre du relief et des nuages ne diffuse pas le soleil
        {
            herocraft_common::prof_scope!("render::encode_god_rays");
            let timestamps = timer_queries.map(|q| (q, gpu_timer::TS_GOD_RAYS_BEGIN, gpu_timer::TS_GOD_RAYS_END));
            self.atmosphere.encode_god_rays(&mut encoder, self.post.hdr_view(), &self.shadows.receiver_bind_group, timestamps);
        }

        // 2. Bloom puis composition HDR (exposition + tonemapper) vers la swapchain
        {
            herocraft_common::prof_scope!("render::encode_post");
            let timestamps = timer_queries.map(|q| (q, gpu_timer::TS_POST_BEGIN, gpu_timer::TS_POST_END));
            self.post.encode(&mut encoder, &swapchain_view, self.bloom_enabled, timestamps);
        }

        // 3. Passe 2D Interface Utilisateur egui (sample_count = 1 sur la swapchain)
        {
            herocraft_common::prof_scope!("render::egui_update_buffers");
            self.egui_renderer.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                paint_jobs,
                screen_descriptor,
            );
        }

        {
            herocraft_common::prof_scope!("render::encode_egui_pass");
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
                timestamp_writes: timer_queries.map(|query_set| wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(gpu_timer::TS_UI_BEGIN),
                    end_of_pass_write_index: Some(gpu_timer::TS_UI_END),
                }),
            }).forget_lifetime();

            self.egui_renderer.render(&mut ui_pass, paint_jobs, screen_descriptor);
        }

        {
            herocraft_common::prof_scope!("render::submit");
            if let Some(timer) = self.gpu_timer.as_ref() {
                timer.resolve(&mut encoder);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.end_frame();
            }
        }
        {
            herocraft_common::prof_scope!("render::present");
            output.present();
        }

        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}

/// Petit maillage réécrit souvent (blocs tombés au sol) : ses buffers ne sont recréés que s'il
/// grossit au-delà de leur taille
struct GrowableMesh {
    label: &'static str,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    /// Sommets et indices que les buffers peuvent contenir
    capacity: (usize, usize),
    num_indices: u32,
}

impl GrowableMesh {
    fn new(device: &wgpu::Device, label: &'static str) -> Self {
        let (vertex_buffer, index_buffer) = Self::buffers(device, label, (1, 1));
        Self { label, vertex_buffer, index_buffer, capacity: (1, 1), num_indices: 0 }
    }

    fn buffers(device: &wgpu::Device, label: &str, capacity: (usize, usize)) -> (wgpu::Buffer, wgpu::Buffer) {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity.0 * std::mem::size_of::<PlanetVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity.1 * std::mem::size_of::<u32>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        (vertex_buffer, index_buffer)
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &crate::planet_mesh::PlanetMesh) {
        self.num_indices = mesh.indices.len() as u32;
        if mesh.indices.is_empty() {
            return;
        }
        if mesh.vertices.len() > self.capacity.0 || mesh.indices.len() > self.capacity.1 {
            self.capacity = (mesh.vertices.len().next_power_of_two(), mesh.indices.len().next_power_of_two());
            (self.vertex_buffer, self.index_buffer) = Self::buffers(device, self.label, self.capacity);
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&mesh.vertices));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_flattening_bounds() {
        let r = 50.0_f32;
        let block = r * std::f32::consts::FRAC_PI_4 * (2.0 / 1024.0);
        let sea = r + herocraft_world::SEA_LEVEL_BLOCKS as f32 * block;
        // Réglage à 0 : vraie courbure à toute altitude
        assert_eq!(planet_flattening(sea + 2.0 * block, r, 0.0), 1.0);
        // Au sol : le globe apparent est bien plus grand, l'horizon recule d'au moins 3 fois
        let k_ground = planet_flattening(sea + 1.7 * block, r, 1.0);
        assert!(k_ground > 9.0, "k au sol = {k_ground}");
        // Décroît avec l'altitude et revient à 1 bien avant l'orbite
        let k_high = planet_flattening(sea + 100.0 * block, r, 1.0);
        assert!(k_high > 1.0 && k_high < k_ground);
        assert_eq!(planet_flattening(sea + 400.0 * block, r, 1.0), 1.0);
        assert_eq!(planet_flattening(r * 6.6, r, 1.0), 1.0);
        // Sans saut pendant le dézoom
        let mut last = planet_flattening(sea, r, 1.0);
        for i in 0..=800 {
            let k = planet_flattening(sea + i as f32 * 0.5 * block, r, 1.0);
            assert!(k <= last + 1e-3 && last / k < 1.05, "saut de k à {} blocs : {last} -> {k}", i as f32 * 0.5);
            last = k;
        }
        // Les antipodes restent cachées : horizon + portée d'un sommet sous 0,9 demi-tour
        for h_blocks in [0.0_f32, 2.0, 30.0, 300.0] {
            let k = planet_flattening(sea + h_blocks * block, r, 1.0);
            let big = k * sea;
            let seen = (2.0 * big * h_blocks * block).sqrt() + (2.0 * big * FLATTEN_MOUNTAIN_BLOCKS * block).sqrt();
            assert!(seen <= 0.9 * std::f32::consts::PI * sea * 1.001, "h = {h_blocks} : {seen}");
        }
    }

    #[test]
    fn test_camera_uniform_size_and_alignment() {
        assert_eq!(std::mem::size_of::<CameraUniform>(), 224);
        assert_eq!(std::mem::align_of::<CameraUniform>(), 16);
    }

    #[test]
    fn test_shaders_wgsl_validation() {
        let sky_shader = include_str!("sky_shader.wgsl");
        let terrain_shader = include_str!("shader.wgsl");
        let ui_shader = include_str!("ui_shader.wgsl");
        let cloud_shader = include_str!("cloud_shader.wgsl");
        let post_shader = include_str!("post_shader.wgsl");
        let shadow_caster = include_str!("shadow_caster.wgsl");
        let atmosphere_lut = include_str!("atmosphere_lut.wgsl");
        let god_rays = include_str!("god_rays.wgsl");
        let block_outline = include_str!("block_outline.wgsl");
        let block_cracks = include_str!("block_cracks.wgsl");
        let creature = include_str!("creature.wgsl");

        for (name, source) in [
            ("sky_shader.wgsl", sky_shader),
            ("shader.wgsl", terrain_shader),
            ("ui_shader.wgsl", ui_shader),
            ("cloud_shader.wgsl", cloud_shader),
            ("post_shader.wgsl", post_shader),
            ("shadow_caster.wgsl", shadow_caster),
            ("atmosphere_lut.wgsl", atmosphere_lut),
            ("god_rays.wgsl", god_rays),
            ("block_outline.wgsl", block_outline),
            ("block_cracks.wgsl", block_cracks),
            ("creature.wgsl", creature),
        ] {
            // Assemblage naga_oil (imports de common.wgsl) puis validation complète
            let module = if matches!(name, "ui_shader.wgsl" | "post_shader.wgsl" | "shadow_caster.wgsl") {
                naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("Failed to parse {name}:\n{e}"))
            } else {
                crate::shaders::compose(source, name).unwrap_or_else(|e| panic!("Failed to compose {name}:\n{e}"))
            };
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

        let calc_vis = |cam: &OrbitCamera| {
            if cam.view_mode() == CameraViewMode::SatelliteOrbit {
                0.0_f32
            } else {
                let fade_dist = (cam.base_radius * 0.2).max(1.0);
                ((cam.base_radius - cam.effective_distance) / fade_dist).clamp(0.0, 1.0)
            }
        };

        let cloud_vis_satellite = calc_vis(&cam);
        assert_eq!(cloud_vis_satellite, 0.0, "Les nuages doivent être totalement invisibles en vue satellite");

        // 2. Vue Sol (FPS: distance = 0.0)
        cam.effective_distance = 0.0;
        assert_eq!(cam.view_mode(), CameraViewMode::FirstPerson);
        let cloud_vis_fps = calc_vis(&cam);
        assert_eq!(cloud_vis_fps, 1.0, "Les nuages doivent être à 100% visibles au sol en FPS");

        // 3. Vue Sol (TPS: distance = 15.0)
        cam.effective_distance = 15.0;
        assert_eq!(cam.view_mode(), CameraViewMode::ThirdPerson);
        let cloud_vis_tps = calc_vis(&cam);
        assert_eq!(cloud_vis_tps, 1.0, "Les nuages doivent être à 100% visibles au sol en TPS");
    }

    #[test]
    fn test_sky_visibility_at_ground_vs_satellite_across_scales() {
        use crate::camera::{CameraViewMode, OrbitCamera};
        let desc = herocraft_world::PlanetDescriptor::default();
        let gen = herocraft_world::PlanetWorldGenerator::new(desc);

        for &radius in &[25.0, 50.0, 100.0] {
            let scale = radius / 25.0;
            let mut cam = OrbitCamera::new(radius * 6.6, radius, &gen);

            let calc_ground_factor = |cam: &OrbitCamera| {
                if cam.view_mode() != CameraViewMode::SatelliteOrbit {
                    1.0_f32
                } else {
                    let t_orbit = ((cam.effective_distance - cam.base_radius) / (cam.base_radius * 1.8)).clamp(0.0, 1.0);
                    1.0 - t_orbit
                }
            };

            // 1. Vue Sol FPS : le ciel doit être à 100% visible
            cam.effective_distance = 0.0;
            assert_eq!(cam.view_mode(), CameraViewMode::FirstPerson);
            assert_eq!(calc_ground_factor(&cam), 1.0, "Ciel non visible au sol en FPS pour rayon {}", radius);

            // 2. Vue Sol TPS (ex: à mi-distance du rayon) : le ciel doit être à 100% visible
            cam.effective_distance = radius * 0.5;
            assert_eq!(cam.view_mode(), CameraViewMode::ThirdPerson);
            assert_eq!(calc_ground_factor(&cam), 1.0, "Ciel non visible au sol en TPS pour rayon {}", radius);

            // 3. Vue Satellite complète : transition vers l'espace profond
            cam.effective_distance = radius * 6.6;
            assert_eq!(cam.view_mode(), CameraViewMode::SatelliteOrbit);
            assert_eq!(calc_ground_factor(&cam), 0.0, "L'espace doit être actif en vue satellite pour rayon {}", radius);

            // 4. Formule du shader de ciel : vérifie la compatibilité de l'altitude au sol
            let ground_max = 58.0 * scale;
            let ground_fade = 20.0 * scale;
            // Même au sommet d'une montagne (ex: base_radius + 20.0 * scale)
            let mountain_cam_dist = radius + 20.0 * scale;
            let alt_ground_factor = ((ground_max - mountain_cam_dist) / ground_fade).clamp(0.0, 1.0);
            // Au sol, avec cam.effective_distance = 0.0
            cam.effective_distance = 0.0;
            let combined_ground = alt_ground_factor.max(calc_ground_factor(&cam));
            assert_eq!(combined_ground, 1.0, "Le ciel doit être 100% visible en montagne au sol pour rayon {}", radius);
        }
    }
}

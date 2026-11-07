//! Atmosphère physique (Hillaire 2020) : diffusion de Rayleigh, de Mie et absorption de l'ozone,
//! à l'échelle de la planète. Trois tables sont précalculées quand la planète ou les réglages
//! changent (transmittance, diffusion multiple, irradiance du ciel), puis le ciel, le terrain et
//! les nuages les lisent au groupe 0 (`atmosphere.wgsl`). Deux tables de plus suivent la caméra et
//! sont recalculées à chaque image : la vue du ciel et le volume de perspective aérienne, lus par
//! le ciel, le terrain et les nuages au lieu d'une marche par pixel. La passe des rayons crépusculaires
//! retire ensuite la lumière que l'air à l'ombre du relief et des nuages n'a pas diffusée.

use wgpu::util::DeviceExt;

use crate::ui::{AtmosphereSettings, LightingSettings};

const LUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const TRANSMITTANCE_SIZE: (u32, u32) = (256, 64);
const MULTISCATTER_SIZE: u32 = 32;
/// Trois orientations de normale côte à côte (dessus, flanc vers le soleil, flanc opposé)
const IRRADIANCE_SIZE: (u32, u32) = (3 * 32, 32);
/// Vue du ciel depuis la caméra (azimut par rapport au soleil, angle zénithal)
const SKY_VIEW_SIZE: (u32, u32) = (192, 108);
/// Volume de perspective aérienne : 32 tranches de distance de 32x32 directions, côte à côte
const AERIAL_SIZE: (u32, u32) = (32 * 32, 32);

/// Dimensions sur le prototype de rayon 25 (1 024 blocs par face), multipliées par l'échelle en
/// blocs (`herocraft_world::block_scale`). Le bas de l'atmosphère est
/// le niveau de la mer (3.8 au-dessus du rayon de base) ; les nuages flottent à 15 au-dessus du
/// rayon de base, au-dessus des plus hauts sommets (13.4).
const ATMOSPHERE_HEIGHT: f32 = 16.0;
const RAYLEIGH_SCALE_HEIGHT: f32 = 2.0;
const MIE_SCALE_HEIGHT: f32 = 1.0;
/// Rayon de la planète virtuelle des tables, en multiple du vrai : le trajet rasant du soleil
/// couchant s'allonge assez pour le rougir (voir atmosphere.wgsl)
const LUT_CURVATURE_SCALE: f32 = 5.0;
/// Densité de l'air pour la perspective aérienne du relief et des nuages
const AERIAL_PERSPECTIVE_DENSITY: f32 = 0.35;
/// Altitude du centre de la couche de nuages au-dessus du rayon de base (cloud_shader.wgsl)
const CLOUD_LAYER_HEIGHT: f32 = crate::cloud_mesh::CloudMeshGenerator::CLOUD_ALTITUDE_MID;
/// Portée de la marche des rayons crépusculaires (la carte des nuages couvre 10 autour du joueur)
const GOD_RAYS_DISTANCE: f32 = 12.0;

/// Rapport entre la taille de l'image et celle de la marche des rayons : 2 (demi-résolution,
/// rayons fins) ou 4 (quart de résolution et deux fois moins de pas, rayons allégés)
fn god_rays_divisor(settings: &AtmosphereSettings) -> u32 {
    if settings.god_rays_fine {
        2
    } else {
        4
    }
}

/// Épaisseurs optiques verticales de la Terre (Rayleigh à 680, 550 et 440 nm, ozone), amplifiées
/// pour un ciel d'un bleu plus profond (AgX désature les bleus) et des couchers plus rouges
const EARTH_RAYLEIGH_DEPTH: [f32; 3] = [0.0464, 0.1085, 0.2648];
const EARTH_OZONE_DEPTH: [f32; 3] = [0.00975, 0.0282, 0.0013];
const DENSITY_BOOST: f32 = 1.6;
/// Brume (Mie) : épaisseur optique verticale, albédo de diffusion simple
const MIE_DEPTH: f32 = 0.045;
const MIE_ALBEDO: f32 = 0.9;
const GROUND_ALBEDO: f32 = 0.3;
/// Éclairement du soleil au zénith, dans l'unité du shader du terrain (lumière directe)
const SUN_ILLUMINANCE: f32 = 2.5;
/// Part de la transmittance verticale compensée dans l'éclairement hors atmosphère : 1 donnerait
/// un soleil de midi parfaitement blanc, moins le laisse légèrement chaud
const NOON_WHITE_BALANCE: f32 = 0.6;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtmosphereUniform {
    rayleigh_scattering: [f32; 3],
    bottom_radius: f32,
    mie_scattering: [f32; 3],
    top_radius: f32,
    mie_extinction: [f32; 3],
    rayleigh_exp_scale: f32,
    absorption_extinction: [f32; 3],
    mie_exp_scale: f32,
    ground_albedo: [f32; 3],
    mie_g: f32,
    sun_illuminance: [f32; 3],
    ozone_center: f32,
    params: [f32; 4],
    sky: [f32; 4],
}

impl AtmosphereUniform {
    /// Paramètres physiques pour une planète de rayon `planet_radius`. L'éclairage fournit les
    /// curseurs Brume (Mie), Soleil et Jour (gain de la lumière du ciel).
    pub fn new(planet_radius: f32, settings: &AtmosphereSettings, lighting: &LightingSettings, god_rays: bool) -> Self {
        let haze = lighting.fog_intensity;
        // Dimensions mesurées en blocs : le ciel vu du sol reste celui du prototype quelle que soit
        // la taille de la planète (vue de l'espace, l'atmosphère est d'autant plus fine)
        let scale = herocraft_world::block_scale(planet_radius as f64) as f32;
        // Niveau de la mer : SEA_LEVEL_BLOCKS blocs de rayon × π/4 × 2/FACE_BLOCKS au-dessus du rayon de base
        let block = planet_radius * std::f32::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f32);
        let sea_level = herocraft_world::generator::SEA_LEVEL_BLOCKS as f32 * block;
        let bottom_radius = planet_radius + sea_level;
        let h_r = RAYLEIGH_SCALE_HEIGHT * scale;
        let h_m = MIE_SCALE_HEIGHT * scale;
        // Ozone : couche en triangle centrée à 25 km pour 8 km d'échelle de Rayleigh, 15 km de demi-épaisseur
        let ozone_center = h_r * 25.0 / 8.0;
        let ozone_half_width = h_r * 15.0 / 8.0;

        let air = DENSITY_BOOST * settings.air_density;
        let rayleigh = EARTH_RAYLEIGH_DEPTH.map(|d| d * air / h_r);
        let ozone = EARTH_OZONE_DEPTH.map(|d| d * air / ozone_half_width);
        let mie_scattering = MIE_DEPTH * haze / h_m;
        let mie_extinction = mie_scattering / MIE_ALBEDO;

        // Soleil presque blanc au zénith au niveau de la mer : l'éclairement hors atmosphère
        // compense en partie la transmittance verticale (sinon la lumière de midi serait jaune)
        let zenith_depth: [f32; 3] =
            std::array::from_fn(|i| rayleigh[i] * h_r + mie_extinction * h_m + ozone[i] * ozone_half_width);
        let sun = SUN_ILLUMINANCE * lighting.sun_intensity;

        Self {
            rayleigh_scattering: rayleigh,
            bottom_radius,
            mie_scattering: [mie_scattering; 3],
            top_radius: bottom_radius + ATMOSPHERE_HEIGHT * scale,
            mie_extinction: [mie_extinction; 3],
            rayleigh_exp_scale: 1.0 / h_r,
            absorption_extinction: ozone,
            mie_exp_scale: 1.0 / h_m,
            ground_albedo: [GROUND_ALBEDO; 3],
            mie_g: settings.sun_halo,
            sun_illuminance: zenith_depth.map(|d| sun * (d * NOON_WHITE_BALANCE).exp()),
            ozone_center,
            params: [
                ozone_half_width,
                if god_rays { settings.god_rays_intensity } else { 0.0 },
                GOD_RAYS_DISTANCE * scale,
                (CLOUD_LAYER_HEIGHT - 0.5) * scale + planet_radius - bottom_radius,
            ],
            sky: [lighting.day_ambient, bottom_radius * LUT_CURVATURE_SCALE, AERIAL_PERSPECTIVE_DENSITY, god_rays_divisor(settings) as f32],
        }
    }

    /// Partie qui détermine les tables précalculées (le soleil et les rayons n'y entrent pas)
    fn lut_key(&self) -> Self {
        Self { sun_illuminance: [0.0; 3], params: [self.params[0], 0.0, 0.0, 0.0], sky: [0.0, self.sky[1], 0.0, 0.0], ..*self }
    }
}

struct Lut {
    view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

/// Table de la caméra, recalculée à chaque image : lumière diffusée et transmittance
struct CameraLut {
    inscatter: wgpu::TextureView,
    transmittance: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
}

pub struct Atmosphere {
    uniform_buffer: wgpu::Buffer,
    transmittance: Lut,
    multiscatter: Lut,
    irradiance: Lut,
    sky_view: CameraLut,
    aerial: CameraLut,
    camera_lut_bind_group: wgpu::BindGroup,
    /// Paramètres des tables actuelles (None : jamais calculées) et paramètres voulus
    lut_key: Option<AtmosphereUniform>,
    wanted_key: AtmosphereUniform,
    /// Groupe 0 du ciel, du terrain, des nuages et des rayons : caméra + atmosphère
    pub scene_layout: wgpu::BindGroupLayout,
    pub scene_bind_group: wgpu::BindGroup,
    god_rays_trace_pipeline: wgpu::RenderPipeline,
    god_rays_upsample_pipeline: wgpu::RenderPipeline,
    depth_layout: wgpu::BindGroupLayout,
    depth_bind_group: wgpu::BindGroup,
    /// Rayons à demi-résolution : lumière perdue + distance, relue par la remontée bilatérale
    half_layout: wgpu::BindGroupLayout,
    half_view: wgpu::TextureView,
    half_bind_group: wgpu::BindGroup,
    multisampled: bool,
    /// Taille de la passe 3D, et rapport entre elle et la cible des rayons (2 : demi-résolution,
    /// rayons fins ; 4 : quart de résolution, rayons allégés)
    size: (u32, u32),
    ray_divisor: u32,
    god_rays_active: bool,
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

impl Atmosphere {
    pub fn new(
        device: &wgpu::Device,
        camera_buffer: &wgpu::Buffer,
        shadow_layout: &wgpu::BindGroupLayout,
        depth_view: &wgpu::TextureView,
        target: crate::post::SceneTarget,
        size: (u32, u32),
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Atmosphere Uniform Buffer"),
            contents: bytemuck::cast_slice(&[AtmosphereUniform::new(
                (herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32,
                &AtmosphereSettings::default(),
                &LightingSettings::default(),
                false,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atmosphere LUT Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let create_view = |label: &str, (width, height): (u32, u32)| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: LUT_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let transmittance_view = create_view("Atmosphere Transmittance LUT", TRANSMITTANCE_SIZE);
        let multiscatter_view = create_view("Atmosphere Multiscatter LUT", (MULTISCATTER_SIZE, MULTISCATTER_SIZE));
        let irradiance_view = create_view("Atmosphere Irradiance LUT", IRRADIANCE_SIZE);

        // Chaque table ne lie que ce qu'elle lit : une table en cours d'écriture ne peut pas
        // être liée en lecture dans la même passe
        let lut_shader = crate::shaders::create_shader_module(device, "Atmosphere LUT Shader", include_str!("atmosphere_lut.wgsl"), "atmosphere_lut.wgsl");
        let make_lut = |label: &str, entry_point: &str, view: wgpu::TextureView, inputs: &[(u32, &wgpu::TextureView)]| {
            let mut layout_entries = vec![uniform_entry(1)];
            let mut entries = vec![wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() }];
            for &(binding, input) in inputs {
                layout_entries.push(texture_entry(binding));
                entries.push(wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(input) });
            }
            if !inputs.is_empty() {
                layout_entries.push(sampler_entry(5));
                entries.push(wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&sampler) });
            }
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries: &layout_entries });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &layout, entries: &entries });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &lut_shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &lut_shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(wgpu::ColorTargetState { format: LUT_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            Lut { view, pipeline, bind_group }
        };
        let transmittance = make_lut("Atmosphere Transmittance", "fs_transmittance", transmittance_view, &[]);
        let multiscatter = make_lut("Atmosphere Multiscatter", "fs_multiscatter", multiscatter_view, &[(2, &transmittance.view)]);
        let irradiance = make_lut(
            "Atmosphere Irradiance",
            "fs_irradiance",
            irradiance_view,
            &[(2, &transmittance.view), (3, &multiscatter.view)],
        );

        // Tables de la caméra : caméra, atmosphère, transmittance et diffusion multiple
        let camera_lut_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Atmosphere Camera LUT Layout"),
            entries: &[uniform_entry(0), uniform_entry(1), texture_entry(2), texture_entry(3), sampler_entry(5)],
        });
        let camera_lut_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmosphere Camera LUT Bind Group"),
            layout: &camera_lut_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&transmittance.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&multiscatter.view) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let camera_lut_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Atmosphere Camera LUT Pipeline Layout"),
            bind_group_layouts: &[&camera_lut_layout],
            push_constant_ranges: &[],
        });
        let make_camera_lut = |label: &str, entry_point: &str, size: (u32, u32)| {
            let target = Some(wgpu::ColorTargetState { format: LUT_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&camera_lut_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &lut_shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &lut_shader,
                    entry_point: Some(entry_point),
                    targets: &[target.clone(), target],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            CameraLut {
                inscatter: create_view(&format!("{label} Inscatter"), size),
                transmittance: create_view(&format!("{label} Transmittance"), size),
                pipeline,
            }
        };
        let sky_view = make_camera_lut("Atmosphere Sky View LUT", "fs_sky_view", SKY_VIEW_SIZE);
        let aerial = make_camera_lut("Atmosphere Aerial Perspective LUT", "fs_aerial", AERIAL_SIZE);

        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene Bind Group Layout"),
            entries: &[
                uniform_entry(0),
                uniform_entry(1),
                texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                sampler_entry(5),
                texture_entry(6),
                texture_entry(7),
                texture_entry(8),
                texture_entry(9),
            ],
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene Bind Group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&transmittance.view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&multiscatter.view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&irradiance.view) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&sky_view.inscatter) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&sky_view.transmittance) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&aerial.inscatter) },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&aerial.transmittance) },
            ],
        });

        // Rayons crépusculaires : lit la profondeur de la passe 3D (échantillon 0 avec MSAA)
        let multisampled = target.sample_count > 1;
        let half_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("God Rays Half Layout"),
            entries: &[texture_entry(0)],
        });
        let ray_divisor = 2;
        let (half_view, half_bind_group) = Self::create_half_target(device, &half_layout, size, ray_divisor);
        let (depth_layout, god_rays_trace_pipeline, god_rays_upsample_pipeline) =
            Self::create_god_rays_pipelines(device, &scene_layout, shadow_layout, &half_layout, target);
        let depth_bind_group = Self::create_depth_bind_group(device, &depth_layout, depth_view, multisampled);

        Self {
            uniform_buffer,
            transmittance,
            multiscatter,
            irradiance,
            sky_view,
            aerial,
            camera_lut_bind_group,
            lut_key: None,
            wanted_key: AtmosphereUniform::new((herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32, &AtmosphereSettings::default(), &LightingSettings::default(), false).lut_key(),
            scene_layout,
            scene_bind_group,
            god_rays_trace_pipeline,
            god_rays_upsample_pipeline,
            depth_layout,
            depth_bind_group,
            half_layout,
            half_view,
            half_bind_group,
            multisampled,
            size,
            ray_divisor,
            god_rays_active: false,
        }
    }

    /// Groupe de la profondeur de la passe 3D et pipelines des rayons (marche, remontée), qui
    /// dépendent de la cible de la passe 3D : échantillonnage de sa profondeur (MSAA ou non) et
    /// format de l'image où la remontée retire la lumière
    fn create_god_rays_pipelines(
        device: &wgpu::Device,
        scene_layout: &wgpu::BindGroupLayout,
        shadow_layout: &wgpu::BindGroupLayout,
        half_layout: &wgpu::BindGroupLayout,
        target: crate::post::SceneTarget,
    ) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline, wgpu::RenderPipeline) {
        let multisampled = target.sample_count > 1;
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("God Rays Depth Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: if multisampled { 0 } else { 1 },
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled,
                },
                count: None,
            }],
        });
        let god_rays_shader = crate::shaders::create_shader_module(device, "God Rays Shader", include_str!("god_rays.wgsl"), "god_rays.wgsl");
        let fullscreen_pipeline = |label: &str, layouts: &[&wgpu::BindGroupLayout], entry_point: &str, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &god_rays_shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &god_rays_shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: if blend.is_some() { wgpu::ColorWrites::COLOR } else { wgpu::ColorWrites::ALL },
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let god_rays_trace_pipeline = fullscreen_pipeline(
            "God Rays Trace Pipeline",
            &[scene_layout, shadow_layout, &depth_layout],
            if multisampled { "fs_trace_ms" } else { "fs_trace" },
            // Cible des rayons : l'alpha garde la distance (remontée bilatérale)
            crate::post::HDR_FORMAT,
            None,
        );
        // Image - lumière perdue dans l'air à l'ombre
        let god_rays_upsample_pipeline = fullscreen_pipeline(
            "God Rays Upsample Pipeline",
            &[scene_layout, shadow_layout, &depth_layout, half_layout],
            if multisampled { "fs_upsample_ms" } else { "fs_upsample" },
            target.format,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::ReverseSubtract,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        );
        (depth_layout, god_rays_trace_pipeline, god_rays_upsample_pipeline)
    }

    fn create_depth_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        depth_view: &wgpu::TextureView,
        multisampled: bool,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("God Rays Depth Bind Group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: if multisampled { 0 } else { 1 },
                resource: wgpu::BindingResource::TextureView(depth_view),
            }],
        })
    }

    /// Cible des rayons à résolution réduite d'un facteur `divisor` (arrondie au-dessus)
    fn create_half_target(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        (width, height): (u32, u32),
        divisor: u32,
    ) -> (wgpu::TextureView, wgpu::BindGroup) {
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("God Rays Half Target"),
                size: wgpu::Extent3d {
                    width: width.div_ceil(divisor).max(1),
                    height: height.div_ceil(divisor).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::post::HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("God Rays Half Bind Group"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        (view, bind_group)
    }

    /// La texture de profondeur et la cible des rayons sont recréées au redimensionnement
    pub fn resize(&mut self, device: &wgpu::Device, depth_view: &wgpu::TextureView, size: (u32, u32)) {
        self.size = size;
        self.depth_bind_group = Self::create_depth_bind_group(device, &self.depth_layout, depth_view, self.multisampled);
        (self.half_view, self.half_bind_group) = Self::create_half_target(device, &self.half_layout, size, self.ray_divisor);
    }

    /// La cible de la passe 3D change (MSAA coupé ou rétabli, format HDR) : nouvelle profondeur,
    /// et pipelines des rayons qui la lisent et qui écrivent dans l'image
    pub fn set_target(&mut self, device: &wgpu::Device, shadow_layout: &wgpu::BindGroupLayout, depth_view: &wgpu::TextureView, target: crate::post::SceneTarget) {
        self.multisampled = target.sample_count > 1;
        (self.depth_layout, self.god_rays_trace_pipeline, self.god_rays_upsample_pipeline) =
            Self::create_god_rays_pipelines(device, &self.scene_layout, shadow_layout, &self.half_layout, target);
        self.depth_bind_group = Self::create_depth_bind_group(device, &self.depth_layout, depth_view, self.multisampled);
    }

    /// `god_rays` : rayons demandés et ombres du soleil calculées cette frame
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        settings: &AtmosphereSettings,
        lighting: &LightingSettings,
        planet_radius: f32,
        god_rays: bool,
    ) {
        self.god_rays_active = god_rays && settings.god_rays && settings.god_rays_intensity > 0.0;
        let divisor = god_rays_divisor(settings);
        if divisor != self.ray_divisor {
            self.ray_divisor = divisor;
            (self.half_view, self.half_bind_group) = Self::create_half_target(device, &self.half_layout, self.size, divisor);
        }
        let uniform = AtmosphereUniform::new(planet_radius, settings, lighting, self.god_rays_active);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
        self.wanted_key = uniform.lut_key();
    }

    /// Recalcule les tables si la planète ou les réglages ont changé, puis les tables de la
    /// caméra (à encoder avant la passe 3D, après la mise à jour de la caméra)
    pub fn encode_luts(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.lut_key != Some(self.wanted_key) {
            self.lut_key = Some(self.wanted_key);
            self.encode_static_luts(encoder);
        }
        for lut in [&self.sky_view, &self.aerial] {
            let attachment = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Atmosphere Camera LUT Pass"),
                color_attachments: &[attachment(&lut.inscatter), attachment(&lut.transmittance)],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&lut.pipeline);
            pass.set_bind_group(0, &self.camera_lut_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    fn encode_static_luts(&self, encoder: &mut wgpu::CommandEncoder) {
        for lut in [&self.transmittance, &self.multiscatter, &self.irradiance] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Atmosphere LUT Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &lut.view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&lut.pipeline);
            pass.set_bind_group(0, &lut.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Rayons crépusculaires sur la cible HDR résolue, après la passe 3D : marche à
    /// demi-résolution, puis remontée bilatérale soustraite de l'image
    pub fn encode_god_rays(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        hdr_view: &wgpu::TextureView,
        shadow_bind_group: &wgpu::BindGroup,
        timestamps: Option<(&wgpu::QuerySet, u32, u32)>,
    ) {
        // Début du chronométrage sur la marche, fin sur la remontée
        if self.god_rays_active {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("God Rays Trace Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.half_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: timestamps.map(|(query_set, begin, _)| wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(begin),
                    end_of_pass_write_index: None,
                }),
            });
            pass.set_pipeline(&self.god_rays_trace_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, shadow_bind_group, &[]);
            pass.set_bind_group(2, &self.depth_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("God Rays Upsample Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: hdr_view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: timestamps.map(|(query_set, begin, end)| wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: if self.god_rays_active { None } else { Some(begin) },
                end_of_pass_write_index: Some(end),
            }),
        });
        // Passe vide sans rayons : les timestamps restent valides
        if self.god_rays_active {
            pass.set_pipeline(&self.god_rays_upsample_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_bind_group(1, shadow_bind_group, &[]);
            pass.set_bind_group(2, &self.depth_bind_group, &[]);
            pass.set_bind_group(3, &self.half_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atmosphere_uniform_layout() {
        // 8 vec4 : doit correspondre à Atmosphere dans atmosphere.wgsl
        assert_eq!(std::mem::size_of::<AtmosphereUniform>(), 8 * 16);
    }

    #[test]
    fn test_atmosphere_scales_with_planet_and_whitens_noon_sun() {
        let settings = AtmosphereSettings::default();
        let lighting = LightingSettings::default();
        let r = crate::camera::DEFAULT_RADIUS;
        let small = AtmosphereUniform::new(r, &settings, &lighting, true);
        let big = AtmosphereUniform::new(2.0 * r, &settings, &lighting, true);
        // Le bas de l'atmosphère est au niveau de la mer (100 blocs au-dessus du rayon de base)
        let block = herocraft_world::block_size(r as f64) as f32;
        assert!((small.bottom_radius - r - 100.0 * block).abs() < 0.01, "{}", small.bottom_radius);
        // Épaisseur mesurée en blocs : celle du prototype (16 × 2 unités de 1/13 de bloc)
        assert!(((small.top_radius - small.bottom_radius) / block - 16.0 * 2.0 / 0.0767).abs() < 5.0);
        // Planète deux fois plus grande : atmosphère deux fois plus épaisse, coefficients deux fois plus faibles
        assert!(((big.top_radius - big.bottom_radius) / (small.top_radius - small.bottom_radius) - 2.0).abs() < 1e-4);
        assert!((small.rayleigh_scattering[2] / big.rayleigh_scattering[2] - 2.0).abs() < 1e-4);
        // L'éclairement hors atmosphère compense le bleu absorbé au zénith : plus fort en bleu
        assert!(small.sun_illuminance[2] > small.sun_illuminance[0]);
        // Les tables ne dépendent ni du soleil ni des rayons
        let brighter = AtmosphereUniform::new(crate::camera::DEFAULT_RADIUS, &settings, &LightingSettings { sun_intensity: 2.0, day_ambient: 1.5, ..lighting }, false);
        assert_eq!(small.lut_key(), brighter.lut_key());
        let hazier = AtmosphereUniform::new(crate::camera::DEFAULT_RADIUS, &settings, &LightingSettings { fog_intensity: 2.0, ..lighting }, true);
        assert_ne!(small.lut_key(), hazier.lut_key());
    }
}

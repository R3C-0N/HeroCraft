//! Ombres portées du soleil : trois cascades de profondeur centrées sur le joueur pour le relief,
//! et une carte d'opacité des nuages, toutes vues depuis le soleil en projection orthographique.
//! Le shader du terrain les lit au groupe 1 (`sun_visibility` dans shader.wgsl).

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::ui::ShadowSettings;

pub const CASCADE_COUNT: usize = 3;
/// Côté des cascades : ombres fines, ombres allégées (`ShadowSettings::fine`)
const CASCADE_SIZE_FINE: u32 = 2048;
const CASCADE_SIZE_LIGHT: u32 = 1024;
const CLOUD_MAP_SIZE: u32 = 1024;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLOUD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Demi-largeur monde de chaque cascade sur une planète de rayon 25 (multipliée par l'échelle).
/// Un bloc mesure environ 0.038 : en ombres fines, 50 texels par bloc près du joueur, 5 au bord
/// du brouillard (deux fois moins en ombres allégées).
const CASCADE_EXTENTS: [f32; CASCADE_COUNT] = [0.8, 2.6, 8.0];
/// Demi-largeur couverte par la carte des nuages (au-delà, le brouillard masque le sol)
const CLOUD_EXTENT: f32 = 10.0;
/// Distance de la caméra soleil au centre, et profondeur totale du volume capturé : contient
/// les sommets du relief
const LIGHT_DISTANCE: f32 = 14.0;
const LIGHT_DEPTH: f32 = 28.0;
/// Même chose pour la carte des nuages, dont la couche flotte à 15 au-dessus du rayon de base,
/// environ 11 au-dessus du joueur : le long d'un soleil bas (17° au-dessus de l'horizon), elle est
/// à près de 40 du sol qu'elle ombre
const CLOUD_LIGHT_DISTANCE: f32 = 40.0;
const CLOUD_LIGHT_DEPTH: f32 = 80.0;
/// Décalage maximal le long de la normale à la lecture (faces rasantes), en fraction de la
/// demi-largeur : 1 texel. Plus grand, il sortait le haut des faces à l'ombre de leur bloc.
fn normal_offset(cascade_size: u32) -> f32 {
    2.0 / cascade_size as f32
}

/// Cascades qui peuvent être redessinées à l'image `frame` (la proche à chaque image). Ombres
/// fines : les deux lointaines en alternance, une image sur deux. Allégées : celle du milieu une
/// image sur deux et la plus lointaine une sur quatre ; leurs texels couvrent assez de terrain pour
/// qu'un retard de quelques images ne se voie pas. Une cascade lointaine n'est redessinée à son
/// tour que si sa carte ne correspond plus à la scène (`FarCascadeState::is_stale`).
fn cascade_refresh(frame: u64, fine: bool) -> [bool; CASCADE_COUNT] {
    if fine {
        [true, frame % 2 == 0, frame % 2 == 1]
    } else {
        [true, frame % 2 == 0, frame % 4 == 1]
    }
}

/// La carte des nuages est redessinée à cette image : une sur deux (fines), une sur quatre
/// (allégées), en décalé des cascades lointaines
fn cloud_refresh(frame: u64, fine: bool) -> bool {
    if fine {
        frame % 2 == 0
    } else {
        frame % 4 == 3
    }
}

/// Hauteur d'ombre (en blocs) dont le bout ne doit pas se décaler de plus d'un demi-texel avant
/// qu'une cascade lointaine soit redessinée quand le soleil tourne : arbres, falaises, maisons
const SUN_LAG_CASTER_BLOCKS: f32 = 32.0;
/// Déplacement du centre d'une cascade lointaine (en fraction de sa demi-largeur) au-delà duquel
/// elle est redessinée : le bord de la carte gardée reste assez loin devant le joueur
const FAR_CASCADE_MAX_SHIFT: f32 = 1.0 / 16.0;

/// Ce qu'une cascade lointaine a dessiné : tant que la scène n'en est pas assez loin, sa carte est
/// gardée telle quelle (elle ne contient que le relief, qui ne bouge pas seul)
#[derive(Clone, Copy, Debug, PartialEq)]
struct FarCascadeState {
    center: Vec3,
    extent: f32,
    light_dir: Vec3,
    /// Empreinte des maillages du relief affichés (`Shadows::update`)
    terrain: u64,
}

impl FarCascadeState {
    /// La carte dessinée pour `self` ne convient plus à la scène `now`. `texel` : côté d'un texel
    /// de la cascade, `sun_up` : sinus de la hauteur du soleil, `block` : taille d'un bloc.
    ///
    /// Le bout de l'ombre d'un objet de hauteur H avance de H·Δθ / sin²(e) quand le soleil, à la
    /// hauteur e, tourne de Δθ : la carte est gardée tant que ce décalage reste sous un demi-texel
    /// pour H = `SUN_LAG_CASTER_BLOCKS` (soleil bas : redessinée à chaque tour, comme avant).
    fn is_stale(&self, now: &FarCascadeState, texel: f32, sun_up: f32, block: f32) -> bool {
        if self.terrain != now.terrain || (self.extent - now.extent).abs() > now.extent * 1e-4 {
            return true;
        }
        if self.center.distance(now.center) > now.extent * FAR_CASCADE_MAX_SHIFT {
            return true;
        }
        let sin_e = sun_up.clamp(0.05, 1.0);
        let max_angle = 0.5 * texel * sin_e * sin_e / (SUN_LAG_CASTER_BLOCKS * block);
        // Angle entre les deux directions, précis aux petits angles (acos ne l'est pas)
        let angle = 2.0 * (0.5 * (self.light_dir - now.light_dir).length()).min(1.0).asin();
        angle > max_angle
    }
}

/// Maillage qui porte une ombre : sa boîte englobante (centre, demi-diagonale) et ses tampons de
/// positions (`PlanetVertex::position_layout`) et d'indices
#[derive(Clone, Copy)]
pub struct Caster<'a> {
    pub center: Vec3,
    pub radius: f32,
    pub positions: &'a wgpu::Buffer,
    pub indices: &'a wgpu::Buffer,
    pub index_format: wgpu::IndexFormat,
    pub num_indices: u32,
}

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
    /// Cible de la carte des nuages (les cascades dessinent dans `CascadeMaps::layers`)
    view: Option<wgpu::TextureView>,
    center: Vec3,
    extent: f32,
    /// Redessinée cette frame (sinon la carte et sa matrice de la frame précédente restent)
    refresh: bool,
    /// Cascades lointaines : ce que la carte gardée a dessiné (aucune : à redessiner)
    drawn: Option<FarCascadeState>,
}

/// Textures des cascades d'une taille donnée
struct CascadeMaps {
    size: u32,
    /// Une vue par couche (cible des passes), et toutes les couches (lecture)
    layers: Vec<wgpu::TextureView>,
    array_view: wgpu::TextureView,
}

impl CascadeMaps {
    fn new(device: &wgpu::Device, size: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Cascades"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: CASCADE_COUNT as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layers = (0..CASCADE_COUNT)
            .map(|i| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Shadow Cascade Layer"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Shadow Cascades Array"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        Self { size, layers, array_view }
    }
}

pub struct Shadows {
    cascades: Vec<CasterPass>,
    maps: CascadeMaps,
    /// Ombres fines (sinon allégées) : taille des cascades, filtrage et rythme de redessin
    fine: bool,
    /// Tout redessiner à la prochaine image (cartes recréées)
    refresh_all: bool,
    comparison_sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,
    cloud_sample_view: wgpu::TextureView,
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
    /// Matrices des cartes telles qu'elles ont été dessinées (cascades lointaines en alternance)
    uniform: ShadowUniform,
    frame: u64,
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
    light_view_proj_depth(center, light_dir, extent, map_size, LIGHT_DISTANCE * scale, LIGHT_DEPTH * scale)
}

fn light_view_proj_depth(center: Vec3, light_dir: Vec3, extent: f32, map_size: u32, distance: f32, depth: f32) -> Mat4 {
    let (right, up) = light_basis(light_dir);
    let texel = 2.0 * extent / map_size as f32;
    let snap = |v: f32| (v / texel).round() * texel;
    let snapped = right * snap(center.dot(right)) + up * snap(center.dot(up)) + light_dir * center.dot(light_dir);
    let eye = snapped + light_dir * distance;
    let view = Mat4::look_at_rh(eye, snapped, up);
    let proj = Mat4::orthographic_rh(-extent, extent, -extent, extent, 0.0, depth);
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

        let maps = CascadeMaps::new(device, CASCADE_SIZE_FINE);
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

        let caster_pass = |label: &str, view: Option<wgpu::TextureView>, extent: f32| {
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
            CasterPass { buffer, bind_group, view, center: Vec3::ZERO, extent, refresh: true, drawn: None }
        };
        // La vue des cascades est celle de `maps` (recréée avec elles), celle des nuages est gardée
        let cascades = (0..CASCADE_COUNT).map(|i| caster_pass("Shadow Cascade Pass", None, CASCADE_EXTENTS[i])).collect();
        let cloud = caster_pass(
            "Cloud Shadow Pass",
            Some(cloud_texture.create_view(&wgpu::TextureViewDescriptor::default())),
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
                // Les positions seules : 12 octets lus par sommet
                buffers: &[crate::planet_mesh::PlanetVertex::position_layout()],
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
            cache: crate::shaders::pipeline_cache(),
        });

        let cloud_shader = crate::shaders::create_shader_module(device, "Cloud Shadow Shader", include_str!("cloud_shader.wgsl"), "cloud_shader.wgsl");
        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Shadow Pipeline"),
            layout: Some(&caster_layout),
            vertex: wgpu::VertexState {
                module: &cloud_shader,
                entry_point: Some("vs_cloud_shadow"),
                buffers: &[crate::cloud_mesh::CloudVertex::desc(), crate::cloud_mesh::CLOUD_WEATHER_LAYOUT],
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
            cache: crate::shaders::pipeline_cache(),
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
        let cloud_sample_view = cloud_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let receiver_bind_group = Self::create_receiver_bind_group(
            device,
            &receiver_layout,
            &receiver_buffer,
            &maps,
            &comparison_sampler,
            &cloud_sample_view,
            &linear_sampler,
        );

        Self {
            cascades,
            maps,
            fine: true,
            refresh_all: true,
            comparison_sampler,
            linear_sampler,
            cloud_sample_view,
            cloud,
            terrain_pipeline,
            cloud_pipeline,
            receiver_buffer,
            receiver_layout,
            receiver_bind_group,
            active: false,
            clouds_active: false,
            light_dir: Vec3::Y,
            uniform: ShadowUniform::zeroed_disabled(),
            frame: 0,
        }
    }

    fn create_receiver_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        maps: &CascadeMaps,
        comparison_sampler: &wgpu::Sampler,
        cloud_view: &wgpu::TextureView,
        linear_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Receiver Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&maps.array_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(comparison_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(cloud_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(linear_sampler) },
            ],
        })
    }

    /// Ombres calculées cette frame (au sol, réglage actif, soleil levé)
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Passe des ombres fines aux allégées ou l'inverse : les cascades sont recréées à leur
    /// nouvelle taille (le groupe lu par le terrain aussi) et tout est redessiné
    fn set_fine(&mut self, device: &wgpu::Device, fine: bool) {
        self.fine = fine;
        let size = if fine { CASCADE_SIZE_FINE } else { CASCADE_SIZE_LIGHT };
        if self.maps.size == size {
            return;
        }
        self.maps = CascadeMaps::new(device, size);
        self.receiver_bind_group = Self::create_receiver_bind_group(
            device,
            &self.receiver_layout,
            &self.receiver_buffer,
            &self.maps,
            &self.comparison_sampler,
            &self.cloud_sample_view,
            &self.linear_sampler,
        );
        self.refresh_all = true;
    }

    /// Place les cascades autour du joueur (décalées vers l'avant du regard) et met à jour
    /// les matrices. `ground_factor` vaut 0 en orbite, où les ombres sont coupées. `terrain` :
    /// empreinte des maillages du relief affichés, qui change quand l'un d'eux apparaît ou part.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        settings: &ShadowSettings,
        focus: Vec3,
        forward: Vec3,
        sun_dir: Vec3,
        planet_radius: f32,
        ground_factor: f32,
        clouds_visible: bool,
        terrain: u64,
    ) {
        self.set_fine(device, settings.fine);
        // Portées des ombres mesurées en blocs
        let scale = herocraft_world::block_scale(planet_radius as f64) as f32;
        let up = focus.normalize_or_zero();
        let sun_up = sun_dir.dot(up);
        self.light_dir = sun_dir.normalize();
        let was_active = self.active;
        let clouds_were_active = self.clouds_active;
        self.active = settings.enabled && ground_factor > 0.0 && sun_up > -0.1;
        self.clouds_active = self.active && settings.cloud_shadows && clouds_visible;
        if !self.active {
            self.uniform = ShadowUniform::zeroed_disabled();
            queue.write_buffer(&self.receiver_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
            return;
        }

        // La cascade proche, qui porte aussi les créatures, est redessinée à chaque frame. Les
        // lointaines, qui n'ont que le relief, le sont à leur tour (`cascade_refresh`) et seulement
        // si le soleil, le joueur ou le relief a assez bougé depuis leur dernier dessin. La carte
        // des nuages suit son rythme. Tout est redessiné à l'activation et quand les cartes
        // viennent d'être recréées.
        self.frame = self.frame.wrapping_add(1);
        let refresh_all = !was_active || std::mem::take(&mut self.refresh_all);
        let slots = cascade_refresh(self.frame, self.fine);
        let cascade_size = self.maps.size;
        let block = planet_radius * std::f32::consts::FRAC_PI_4 * (2.0 / 1024.0);

        // Regard projeté sur le sol : la moitié de chaque cascade sert devant le joueur
        let flat_forward = (forward - up * forward.dot(up)).normalize_or_zero();
        for (i, pass) in self.cascades.iter_mut().enumerate() {
            let extent = CASCADE_EXTENTS[i] * scale;
            let center = focus + flat_forward * extent * 0.5;
            let now = FarCascadeState { center, extent, light_dir: self.light_dir, terrain };
            let texel = 2.0 * extent / cascade_size as f32;
            let stale = i == 0 || pass.drawn.is_none_or(|drawn| drawn.is_stale(&now, texel, sun_up, block));
            pass.refresh = refresh_all || (slots[i] && stale);
            if !pass.refresh {
                continue;
            }
            pass.extent = extent;
            pass.center = center;
            pass.drawn = (i > 0).then_some(now);
            let vp = light_view_proj(pass.center, self.light_dir, extent, cascade_size, scale);
            queue.write_buffer(&pass.buffer, 0, bytemuck::cast_slice(&[ShadowPassUniform { view_proj: vp.to_cols_array_2d() }]));
            self.uniform.cascades[i] = vp.to_cols_array_2d();
            self.uniform.extents[i] = extent;
        }
        self.cloud.refresh = cloud_refresh(self.frame, self.fine) || refresh_all || !clouds_were_active;
        if self.cloud.refresh {
            let cloud_extent = CLOUD_EXTENT * scale;
            self.cloud.extent = cloud_extent;
            self.cloud.center = focus;
            let cloud_vp = light_view_proj_depth(
                focus,
                self.light_dir,
                cloud_extent,
                CLOUD_MAP_SIZE,
                CLOUD_LIGHT_DISTANCE * scale,
                CLOUD_LIGHT_DEPTH * scale,
            );
            queue.write_buffer(&self.cloud.buffer, 0, bytemuck::cast_slice(&[ShadowPassUniform { view_proj: cloud_vp.to_cols_array_2d() }]));
            self.uniform.cloud_view_proj = cloud_vp.to_cols_array_2d();
        }
        let uniform = &mut self.uniform;
        uniform.extents[3] = 1.0 / cascade_size as f32;
        uniform.params = [
            settings.strength,
            if self.clouds_active { settings.cloud_strength } else { 0.0 },
            // Filtrage du terrain : 1 = 3x3 (fines), 2 = 2x2 (allégées)
            if self.fine { 1.0 } else { 2.0 },
            normal_offset(cascade_size),
        ];
        queue.write_buffer(&self.receiver_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }

    /// Cascades redessinées à cette image (aucune sans ombres)
    pub fn cascades_to_draw(&self) -> Vec<usize> {
        if !self.active {
            return Vec::new();
        }
        self.cascades.iter().enumerate().filter(|(_, pass)| pass.refresh).map(|(i, _)| i).collect()
    }

    /// Nombre de cascades
    pub fn cascade_count(&self) -> usize {
        self.cascades.len()
    }

    /// Le maillage de centre `center` et de demi-diagonale `radius` touche la cascade `i` (test
    /// dans le plan de la lumière : la profondeur couvre déjà tout le volume)
    pub fn touches(&self, i: usize, center: Vec3, radius: f32) -> bool {
        let Some(pass) = self.cascades.get(i) else { return false };
        let (right, up) = light_basis(self.light_dir);
        let d = center - pass.center;
        let reach = pass.extent + radius;
        d.dot(right).abs() <= reach && d.dot(up).abs() <= reach
    }

    /// Encode les passes depuis le soleil. `terrain` : pour chaque cascade redessinée, les morceaux
    /// du relief qui la touchent (`touches`), tirés de `pool` ; `moving` : ce qui bouge seul
    /// (créatures), dans la cascade proche seulement, la seule redessinée à chaque image.
    pub fn encode<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        camera_bind_group: &wgpu::BindGroup,
        pool: &crate::terrain_pool::TerrainPool,
        terrain: &[Option<crate::terrain_pool::PreparedDraws>],
        moving: Option<Caster<'a>>,
        clouds: Option<(&wgpu::Buffer, &wgpu::Buffer, &wgpu::Buffer, u32)>,
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
            // La cascade 0, toujours redessinée, porte le timestamp de début
            for (i, pass) in self.cascades.iter().enumerate().filter(|(_, pass)| pass.refresh) {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Shadow Cascade Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.maps.layers[i],
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: timestamps.filter(|_| i == 0).map(|t| begin_ts(t.0)),
                });
                render_pass.set_pipeline(&self.terrain_pipeline);
                render_pass.set_bind_group(0, camera_bind_group, &[]);
                render_pass.set_bind_group(1, &pass.bind_group, &[]);
                // Le relief : les morceaux qui touchent la cascade, tirés des tampons partagés
                if let Some(Some(draws)) = terrain.get(i) {
                    pool.draw(&mut render_pass, draws, false);
                }
                for caster in moving.filter(|_| i == 0) {
                    // Test dans le plan de la lumière : la profondeur couvre déjà tout le volume
                    let d = caster.center - pass.center;
                    let reach = pass.extent + caster.radius;
                    if d.dot(right).abs() > reach || d.dot(up).abs() > reach {
                        continue;
                    }
                    render_pass.set_vertex_buffer(0, caster.positions.slice(..));
                    render_pass.set_index_buffer(caster.indices.slice(..), caster.index_format);
                    render_pass.draw_indexed(0..caster.num_indices, 0, 0..1);
                }
            }
        }

        // La passe de la carte des nuages a toujours lieu : elle porte le timestamp de fin. Sans
        // nouveau dessin, la carte de la frame précédente est gardée.
        let redraw_clouds = !self.active || self.cloud.refresh;
        let load = if redraw_clouds { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cloud Shadow Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.cloud.view.as_ref().expect("carte des nuages"),
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
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
        if let Some((vertex_buffer, weather_buffer, index_buffer, num_indices)) = clouds.filter(|_| self.clouds_active && redraw_clouds) {
            render_pass.set_pipeline(&self.cloud_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_bind_group(1, &self.cloud.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, weather_buffer.slice(..));
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
    fn test_far_cascades_refresh_less_often_when_light() {
        let count = |fine: bool| {
            let mut n = [0; CASCADE_COUNT];
            let mut clouds = 0;
            for frame in 0..8 {
                for (i, r) in cascade_refresh(frame, fine).iter().enumerate() {
                    n[i] += usize::from(*r);
                }
                clouds += usize::from(cloud_refresh(frame, fine));
            }
            (n, clouds)
        };
        assert_eq!(count(true), ([8, 4, 4], 4));
        assert_eq!(count(false), ([8, 4, 2], 2));
        // Allégées : jamais plus de deux cartes par image en plus de la proche
        for frame in 0..8 {
            let drawn = cascade_refresh(frame, false).iter().filter(|r| **r).count() + usize::from(cloud_refresh(frame, false));
            assert!(drawn <= 2, "image {frame} : {drawn} cartes");
        }
    }

    #[test]
    fn test_far_cascade_kept_while_scene_is_still() {
        let block = 25.0 * std::f32::consts::FRAC_PI_4 * (2.0 / 1024.0);
        let extent = CASCADE_EXTENTS[1];
        let texel = 2.0 * extent / CASCADE_SIZE_LIGHT as f32;
        let sun = Vec3::new(0.2, 0.9, 0.3).normalize();
        let drawn = FarCascadeState { center: Vec3::new(0.0, 25.0, 0.0), extent, light_dir: sun, terrain: 7 };
        // Rien n'a bougé : la carte est gardée
        assert!(!drawn.is_stale(&drawn, texel, 0.9, block));
        // Relief changé, ou joueur parti assez loin : redessinée
        assert!(drawn.is_stale(&FarCascadeState { terrain: 8, ..drawn }, texel, 0.9, block));
        assert!(drawn.is_stale(&FarCascadeState { center: drawn.center + Vec3::X * extent * 0.1, ..drawn }, texel, 0.9, block));
        assert!(!drawn.is_stale(&FarCascadeState { center: drawn.center + Vec3::X * extent * 0.01, ..drawn }, texel, 0.9, block));
        // Le soleil tourne de 0,02° : gardée soleil haut, redessinée soleil bas
        let turned = FarCascadeState { light_dir: glam::Quat::from_rotation_z(0.02_f32.to_radians()) * sun, ..drawn };
        assert!(!drawn.is_stale(&turned, texel, 0.9, block));
        assert!(drawn.is_stale(&turned, texel, 0.1, block));
        // Soleil haut : un tour de 0,5° se voit
        let far_turn = FarCascadeState { light_dir: glam::Quat::from_rotation_z(0.5_f32.to_radians()) * sun, ..drawn };
        assert!(drawn.is_stale(&far_turn, texel, 0.9, block));
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

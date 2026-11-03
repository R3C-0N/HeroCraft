//! Post-traitement HDR : la passe 3D écrit dans une cible `Rgba16Float`, puis le bloom
//! (chaîne de mips à demi-résolution) et la composition (exposition + tonemapper) produisent
//! l'image finale dans la swapchain, sur laquelle egui dessine ensuite.

use wgpu::util::DeviceExt;

use crate::ui::PostSettings;

/// Format de la cible HDR et des niveaux du bloom
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Nombre maximal de niveaux du bloom (le premier est à demi-résolution)
const MAX_BLOOM_LEVELS: u32 = 6;
/// Taille minimale (en pixels) du plus petit niveau
const MIN_BLOOM_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PostUniform {
    params: [f32; 4],
    mode: [u32; 4],
}

/// Textures dépendant de la taille de la fenêtre
struct Targets {
    hdr_view: wgpu::TextureView,
    hdr_msaa_view: Option<wgpu::TextureView>,
    bloom_views: Vec<wgpu::TextureView>,
    /// Downsample : source = HDR puis niveau i-1, cible = niveau i
    down_bind_groups: Vec<wgpu::BindGroup>,
    /// Upsample : source = niveau i+1, cible = niveau i
    up_bind_groups: Vec<wgpu::BindGroup>,
    composite_bind_group: wgpu::BindGroup,
}

pub struct PostProcess {
    sample_count: u32,
    output_is_srgb: bool,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    blur_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    prefilter_pipeline: wgpu::RenderPipeline,
    downsample_pipeline: wgpu::RenderPipeline,
    upsample_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    targets: Targets,
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

impl PostProcess {
    /// MSAA 4x si l'adaptateur le permet sur le format HDR
    pub fn sample_count(adapter: &wgpu::Adapter) -> u32 {
        let flags = adapter.get_texture_format_features(HDR_FORMAT).flags;
        if flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4) {
            4
        } else {
            1
        }
    }

    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Post Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post_shader.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Post Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Post Uniform Buffer"),
            contents: bytemuck::cast_slice(&[PostUniform { params: [1.0, 0.0, 1.0, 0.5], mode: [1, 0, 0, 0] }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let common_entries = [
            texture_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom Bind Group Layout"),
            entries: &common_entries,
        });
        let composite_entries = [common_entries[0], common_entries[1], common_entries[2], texture_entry(3)];
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &composite_entries,
        });

        let pipeline = |label: &str, layout: &wgpu::BindGroupLayout, entry: &str, format: wgpu::TextureFormat, blend: wgpu::BlendState| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_fullscreen"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
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
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let prefilter_pipeline = pipeline("Bloom Prefilter Pipeline", &blur_layout, "fs_prefilter", HDR_FORMAT, wgpu::BlendState::REPLACE);
        let downsample_pipeline = pipeline("Bloom Downsample Pipeline", &blur_layout, "fs_downsample", HDR_FORMAT, wgpu::BlendState::REPLACE);
        let upsample_pipeline = pipeline("Bloom Upsample Pipeline", &blur_layout, "fs_upsample", HDR_FORMAT, additive);
        let composite_pipeline = pipeline("Composite Pipeline", &composite_layout, "fs_composite", config.format, wgpu::BlendState::REPLACE);

        let targets = Self::create_targets(device, config, sample_count, &sampler, &uniform_buffer, &blur_layout, &composite_layout);

        Self {
            sample_count,
            output_is_srgb: config.format.is_srgb(),
            sampler,
            uniform_buffer,
            blur_layout,
            composite_layout,
            prefilter_pipeline,
            downsample_pipeline,
            upsample_pipeline,
            composite_pipeline,
            targets,
        }
    }

    fn bloom_level_count(width: u32, height: u32) -> u32 {
        let mut levels = 0;
        let (mut w, mut h) = ((width / 2).max(1), (height / 2).max(1));
        while levels < MAX_BLOOM_LEVELS && (levels == 0 || (w >= MIN_BLOOM_SIZE && h >= MIN_BLOOM_SIZE)) {
            levels += 1;
            w /= 2;
            h /= 2;
        }
        levels
    }

    fn create_targets(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
        sampler: &wgpu::Sampler,
        uniform_buffer: &wgpu::Buffer,
        blur_layout: &wgpu::BindGroupLayout,
        composite_layout: &wgpu::BindGroupLayout,
    ) -> Targets {
        let (width, height) = (config.width.max(1), config.height.max(1));
        let hdr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Color Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let hdr_view = hdr.create_view(&wgpu::TextureViewDescriptor::default());

        let hdr_msaa_view = (sample_count > 1).then(|| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("HDR MSAA Color Texture"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        });

        let levels = Self::bloom_level_count(width, height);
        let bloom = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Bloom Mip Chain"),
            size: wgpu::Extent3d { width: (width / 2).max(1), height: (height / 2).max(1), depth_or_array_layers: 1 },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bloom_views: Vec<_> = (0..levels)
            .map(|mip| {
                bloom.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Bloom Mip View"),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

        let blur_group = |source: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Bloom Bind Group"),
                layout: blur_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform_buffer.as_entire_binding() },
                ],
            })
        };
        let down_bind_groups = (0..levels as usize)
            .map(|i| blur_group(if i == 0 { &hdr_view } else { &bloom_views[i - 1] }))
            .collect();
        let up_bind_groups = (0..levels as usize - 1).map(|i| blur_group(&bloom_views[i + 1])).collect();

        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&hdr_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&bloom_views[0]) },
            ],
        });

        Targets { hdr_view, hdr_msaa_view, bloom_views, down_bind_groups, up_bind_groups, composite_bind_group }
    }

    pub fn resize(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        self.targets = Self::create_targets(
            device,
            config,
            self.sample_count,
            &self.sampler,
            &self.uniform_buffer,
            &self.blur_layout,
            &self.composite_layout,
        );
    }

    /// MSAA coupé ou rétabli : la cible HDR multiéchantillonnée est recréée ou retirée
    pub fn set_sample_count(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, sample_count: u32) {
        self.sample_count = sample_count;
        self.resize(device, config);
    }

    /// Cible couleur de la passe 3D : (attachement, cible de résolution MSAA éventuelle)
    pub fn scene_attachment(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        match &self.targets.hdr_msaa_view {
            Some(msaa) => (msaa, Some(&self.targets.hdr_view)),
            None => (&self.targets.hdr_view, None),
        }
    }

    /// Cible HDR résolue (relue par le bloom et la composition)
    pub fn hdr_view(&self) -> &wgpu::TextureView {
        &self.targets.hdr_view
    }

    pub fn update(&self, queue: &wgpu::Queue, settings: &PostSettings) {
        let bloom = if settings.bloom_enabled { settings.bloom_intensity } else { 0.0 };
        let uniform = PostUniform {
            params: [2f32.powf(settings.exposure_ev), bloom, settings.bloom_threshold, settings.bloom_knee],
            mode: [settings.tonemapper.shader_index(), u32::from(!self.output_is_srgb), 0, 0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Encode le bloom puis la composition dans `output`. `timestamps` encadre l'ensemble des passes.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        bloom_enabled: bool,
        timestamps: Option<(&wgpu::QuerySet, u32, u32)>,
    ) {
        let t = &self.targets;
        let fullscreen_pass = |encoder: &mut wgpu::CommandEncoder,
                               label: &str,
                               target: &wgpu::TextureView,
                               load: wgpu::LoadOp<wgpu::Color>,
                               pipeline: &wgpu::RenderPipeline,
                               bind_group: &wgpu::BindGroup,
                               timestamp_writes: Option<wgpu::RenderPassTimestampWrites>| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        };
        let begin_ts = timestamps.map(|(query_set, begin, _)| wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: Some(begin),
            end_of_pass_write_index: None,
        });
        let end_ts = timestamps.map(|(query_set, _, end)| wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(end),
        });

        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let levels = t.bloom_views.len();
        let mut composite_ts = end_ts;
        if bloom_enabled {
            for i in 0..levels {
                let pipeline = if i == 0 { &self.prefilter_pipeline } else { &self.downsample_pipeline };
                let ts = if i == 0 { begin_ts.clone() } else { None };
                fullscreen_pass(encoder, "Bloom Downsample Pass", &t.bloom_views[i], clear, pipeline, &t.down_bind_groups[i], ts);
            }
            for i in (0..levels - 1).rev() {
                fullscreen_pass(encoder, "Bloom Upsample Pass", &t.bloom_views[i], wgpu::LoadOp::Load, &self.upsample_pipeline, &t.up_bind_groups[i], None);
            }
        } else if let Some((query_set, begin, end)) = timestamps {
            // Sans bloom, la composition porte les deux timestamps
            composite_ts = Some(wgpu::RenderPassTimestampWrites {
                query_set,
                beginning_of_pass_write_index: Some(begin),
                end_of_pass_write_index: Some(end),
            });
        }
        fullscreen_pass(encoder, "Composite Pass", output, clear, &self.composite_pipeline, &t.composite_bind_group, composite_ts);
    }
}

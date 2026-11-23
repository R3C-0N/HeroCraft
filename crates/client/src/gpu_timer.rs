//! Mesure du temps GPU par passe via les timestamp queries WGPU (feature cargo `profiling`).
//!
//! Les résultats sont relus de façon asynchrone (anneau de buffers) pour ne jamais bloquer
//! le thread principal, puis envoyés au profileur dans le groupe `gpu`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::camera::CameraViewMode;

/// Points de mesure écrits dans une frame
pub const TS_SHADOW_BEGIN: u32 = 0;
pub const TS_SHADOW_END: u32 = 1;
pub const TS_3D_BEGIN: u32 = 2;
pub const TS_AFTER_TERRAIN: u32 = 3;
pub const TS_AFTER_SKY: u32 = 4;
pub const TS_3D_END: u32 = 5;
pub const TS_POST_BEGIN: u32 = 6;
pub const TS_POST_END: u32 = 7;
pub const TS_UI_BEGIN: u32 = 8;
pub const TS_UI_END: u32 = 9;
pub const TS_GOD_RAYS_BEGIN: u32 = 10;
pub const TS_GOD_RAYS_END: u32 = 11;
const TIMESTAMP_COUNT: u32 = 12;
const SLOT_COUNT: usize = 4;
const BUFFER_SIZE: u64 = TIMESTAMP_COUNT as u64 * 8;

struct Slot {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    in_flight: bool,
    mapped: Arc<AtomicBool>,
    view_mode: CameraViewMode,
}

pub struct GpuTimer {
    slots: Vec<Slot>,
    /// Slot utilisé par la frame en cours d'encodage
    current: Option<usize>,
    period_ns: f64,
    /// Timestamps à l'intérieur des passes (ciel / terrain / nuages séparés)
    pub inside_passes: bool,
}

impl GpuTimer {
    /// Fonctionnalités à demander au device (vide sans la feature `profiling`)
    pub fn wanted_features(adapter_features: wgpu::Features) -> wgpu::Features {
        if !herocraft_common::profiler::is_enabled() && !crate::benchmark::requested() {
            return wgpu::Features::empty();
        }
        adapter_features & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES)
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let features = device.features();
        let wanted = herocraft_common::profiler::is_enabled() || crate::benchmark::requested();
        if !wanted || !features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            if herocraft_common::profiler::is_enabled() {
                herocraft_common::profiler::set_meta("gpu_timestamps", "non supportés par l'adaptateur".into());
            }
            return None;
        }
        let inside_passes = features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
        herocraft_common::profiler::set_meta(
            "gpu_timestamps",
            if inside_passes { "par passe et par draw".into() } else { "par passe".into() },
        );

        let slots = (0..SLOT_COUNT)
            .map(|_| Slot {
                query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("GPU Timer Queries"),
                    ty: wgpu::QueryType::Timestamp,
                    count: TIMESTAMP_COUNT,
                }),
                resolve_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GPU Timer Resolve"),
                    size: BUFFER_SIZE,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GPU Timer Readback"),
                    size: BUFFER_SIZE,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                in_flight: false,
                mapped: Arc::new(AtomicBool::new(false)),
                view_mode: CameraViewMode::SatelliteOrbit,
            })
            .collect();

        Some(Self {
            slots,
            current: None,
            period_ns: queue.get_timestamp_period() as f64,
            inside_passes,
        })
    }

    /// Relit les résultats disponibles puis réserve un slot pour la frame (None si tous sont en vol)
    pub fn begin_frame(&mut self, device: &wgpu::Device, view_mode: CameraViewMode) -> Option<&wgpu::QuerySet> {
        let _ = device.poll(wgpu::PollType::Poll);
        for slot in &mut self.slots {
            if slot.in_flight && slot.mapped.load(Ordering::Acquire) {
                {
                    let data = slot.readback_buffer.slice(..).get_mapped_range().expect("lecture du tampon");
                    let ts: &[u64] = bytemuck::cast_slice(&data);
                    record(ts, self.period_ns, slot.view_mode, self.inside_passes);
                }
                slot.readback_buffer.unmap();
                slot.mapped.store(false, Ordering::Release);
                slot.in_flight = false;
            }
        }
        self.current = self.slots.iter().position(|s| !s.in_flight);
        let slot = &mut self.slots[self.current?];
        slot.view_mode = view_mode;
        Some(&slot.query_set)
    }

    pub fn query_set(&self) -> Option<&wgpu::QuerySet> {
        self.current.map(|i| &self.slots[i].query_set)
    }

    /// À appeler sur l'encodeur après la dernière passe
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(i) = self.current {
            let slot = &self.slots[i];
            encoder.resolve_query_set(&slot.query_set, 0..TIMESTAMP_COUNT, &slot.resolve_buffer, 0);
            encoder.copy_buffer_to_buffer(&slot.resolve_buffer, 0, &slot.readback_buffer, 0, BUFFER_SIZE);
        }
    }

    /// À appeler après `queue.submit`
    pub fn end_frame(&mut self) {
        if let Some(i) = self.current.take() {
            let slot = &mut self.slots[i];
            slot.in_flight = true;
            let mapped = slot.mapped.clone();
            slot.readback_buffer.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                if res.is_ok() {
                    mapped.store(true, Ordering::Release);
                }
            });
        }
    }
}

fn record(ts: &[u64], period_ns: f64, view_mode: CameraViewMode, inside_passes: bool) {
    let span = |a: u32, b: u32| -> u64 {
        let (a, b) = (ts[a as usize], ts[b as usize]);
        (b.saturating_sub(a) as f64 * period_ns) as u64
    };
    // Banc d'essai : mêmes temps, par passe, relevés pour la phase en cours
    let benchmark = [
        ("gpu::ombres", span(TS_SHADOW_BEGIN, TS_SHADOW_END)),
        ("gpu::passe_3d", span(TS_3D_BEGIN, TS_3D_END)),
        ("gpu::rayons", span(TS_GOD_RAYS_BEGIN, TS_GOD_RAYS_END)),
        ("gpu::post_bloom", span(TS_POST_BEGIN, TS_POST_END)),
        ("gpu::interface", span(TS_UI_BEGIN, TS_UI_END)),
        ("gpu::image", span(TS_SHADOW_BEGIN, TS_UI_END)),
    ];
    for (name, ns) in benchmark {
        crate::benchmark::record_gpu(name, ns);
    }
    if inside_passes {
        crate::benchmark::record_gpu("gpu::passe_3d::terrain", span(TS_3D_BEGIN, TS_AFTER_TERRAIN));
        crate::benchmark::record_gpu("gpu::passe_3d::ciel", span(TS_AFTER_TERRAIN, TS_AFTER_SKY));
        crate::benchmark::record_gpu("gpu::passe_3d::nuages_resolve", span(TS_AFTER_SKY, TS_3D_END));
    }
    herocraft_common::prof_gpu!("gpu::shadows", span(TS_SHADOW_BEGIN, TS_SHADOW_END));
    herocraft_common::prof_gpu!("gpu::pass_3d", span(TS_3D_BEGIN, TS_3D_END));
    if inside_passes {
        herocraft_common::prof_gpu!("gpu::pass_3d::terrain", span(TS_3D_BEGIN, TS_AFTER_TERRAIN));
        herocraft_common::prof_gpu!("gpu::pass_3d::sky", span(TS_AFTER_TERRAIN, TS_AFTER_SKY));
        herocraft_common::prof_gpu!("gpu::pass_3d::clouds_msaa_resolve", span(TS_AFTER_SKY, TS_3D_END));
    }
    herocraft_common::prof_gpu!("gpu::god_rays", span(TS_GOD_RAYS_BEGIN, TS_GOD_RAYS_END));
    herocraft_common::prof_gpu!("gpu::post_bloom_tonemap", span(TS_POST_BEGIN, TS_POST_END));
    herocraft_common::prof_gpu!("gpu::pass_egui", span(TS_UI_BEGIN, TS_UI_END));
    let frame = span(TS_SHADOW_BEGIN, TS_UI_END);
    match view_mode {
        CameraViewMode::FirstPerson => herocraft_common::prof_gpu!("gpu::frame [vue FPS]", frame),
        CameraViewMode::ThirdPerson => herocraft_common::prof_gpu!("gpu::frame [vue TPS]", frame),
        CameraViewMode::SatelliteOrbit => herocraft_common::prof_gpu!("gpu::frame [vue orbite]", frame),
    }
}

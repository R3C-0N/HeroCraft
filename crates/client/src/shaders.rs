//! Assemblage des shaders WGSL avec naga_oil : `common.wgsl`, `atmosphere.wgsl` et
//! `shadow_common.wgsl` sont des modules importables (`#import herocraft::common::{...}`)
//! partagés par le ciel, le terrain, les nuages et les passes de l'atmosphère.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use naga_oil::compose::{ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderLanguage, ShaderType};

const MODULES: [(&str, &str); 3] = [
    (include_str!("common.wgsl"), "common.wgsl"),
    (include_str!("atmosphere.wgsl"), "atmosphere.wgsl"),
    (include_str!("shadow_common.wgsl"), "shadow_common.wgsl"),
];

/// Modules déjà assemblés, par fichier : l'assemblage par naga_oil coûte des dizaines de
/// millisecondes, et les pipelines sont recréés quand la cible de la passe 3D change (MSAA)
static COMPOSED: OnceLock<Mutex<HashMap<(String, usize), naga::Module>>> = OnceLock::new();

/// Assemble un shader WGSL et ses imports en un module naga (message d'erreur lisible sinon)
pub fn compose(source: &str, file_path: &str) -> anyhow::Result<naga::Module> {
    let key = (file_path.to_string(), source.len());
    let composed = COMPOSED.get_or_init(Default::default);
    if let Some(module) = composed.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(module.clone());
    }
    let module = compose_uncached(source, file_path)?;
    composed.lock().unwrap_or_else(|e| e.into_inner()).insert(key, module.clone());
    Ok(module)
}

fn compose_uncached(source: &str, file_path: &str) -> anyhow::Result<naga::Module> {
    herocraft_common::prof_scope!("render::shaders::compose");
    let mut composer = Composer::default();
    for &(source, file_path) in MODULES.iter() {
        let added = composer.add_composable_module(ComposableModuleDescriptor {
            source,
            file_path,
            language: ShaderLanguage::Wgsl,
            ..Default::default()
        });
        if let Err(e) = added {
            return Err(anyhow::anyhow!(e.emit_to_string(&composer)));
        }
    }
    composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path,
            shader_type: ShaderType::Wgsl,
            ..Default::default()
        })
        .map_err(|e| anyhow::anyhow!(e.emit_to_string(&composer)))
}

/// Crée un module de shader wgpu à partir d'une source WGSL utilisant les imports partagés
pub fn create_shader_module(device: &wgpu::Device, label: &str, source: &str, file_path: &str) -> wgpu::ShaderModule {
    let module = compose(source, file_path).unwrap_or_else(|e| panic!("Échec d'assemblage de {file_path} :\n{e}"));
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
    })
}

/// Cache des pipelines du pilote (Vulkan), et le fichier où il est gardé d'une partie à l'autre :
/// les shaders déjà compilés par le pilote ne le sont plus au lancement ni au changement de MSAA
static PIPELINE_CACHE: OnceLock<(wgpu::PipelineCache, PathBuf)> = OnceLock::new();

/// Dossier des caches du jeu, à côté des sauvegardes
const CACHE_DIR: &str = "cache";

/// Ouvre le cache des pipelines du device du jeu, s'il en a la fonction (`Features::PIPELINE_CACHE`)
pub fn init_pipeline_cache(adapter_info: &wgpu::AdapterInfo, device: &wgpu::Device) {
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return;
    }
    let Some(key) = wgpu::util::pipeline_cache_key(adapter_info) else { return };
    let path = PathBuf::from(CACHE_DIR).join(key);
    let data = std::fs::read(&path).ok();
    // SAFETY: les données viennent de `PipelineCache::get_data` (fichier écrit par
    // `save_pipeline_cache`) ; un fichier d'un autre pilote ou abîmé est écarté (`fallback`)
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor { label: Some("HeroCraft Pipeline Cache"), data: data.as_deref(), fallback: true })
    };
    log::info!("Cache des pipelines : {} ({})", path.display(), if data.is_some() { "repris" } else { "nouveau" });
    let _ = PIPELINE_CACHE.set((cache, path));
}

/// Le cache à passer à chaque création de pipeline (`None` sans cache)
pub fn pipeline_cache() -> Option<&'static wgpu::PipelineCache> {
    PIPELINE_CACHE.get().map(|(cache, _)| cache)
}

/// Enregistre le cache des pipelines (à la fermeture du jeu)
pub fn save_pipeline_cache() {
    let Some((cache, path)) = PIPELINE_CACHE.get() else { return };
    let Some(data) = cache.get_data() else { return };
    let temp = path.with_extension("tmp");
    let written = std::fs::create_dir_all(CACHE_DIR).and_then(|_| std::fs::write(&temp, &data)).and_then(|_| std::fs::rename(&temp, path));
    if let Err(e) = written {
        log::warn!("Cache des pipelines non enregistré ({}) : {e}", path.display());
    }
}

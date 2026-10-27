//! Assemblage des shaders WGSL avec naga_oil : `common.wgsl`, `atmosphere.wgsl` et
//! `shadow_common.wgsl` sont des modules importables (`#import herocraft::common::{...}`)
//! partagés par le ciel, le terrain, les nuages et les passes de l'atmosphère.

use std::borrow::Cow;

use naga_oil::compose::{ComposableModuleDescriptor, Composer, ComposerError, NagaModuleDescriptor, ShaderLanguage, ShaderType};

const MODULES: [(&str, &str); 3] = [
    (include_str!("common.wgsl"), "common.wgsl"),
    (include_str!("atmosphere.wgsl"), "atmosphere.wgsl"),
    (include_str!("shadow_common.wgsl"), "shadow_common.wgsl"),
];

/// Assemble un shader WGSL et ses imports en un module naga (message d'erreur lisible sinon)
pub fn compose(source: &str, file_path: &str) -> Result<naga::Module, String> {
    herocraft_common::prof_scope!("render::shaders::compose");
    let mut composer = Composer::default();
    let result = MODULES
        .iter()
        .try_for_each(|&(source, file_path)| {
            composer
                .add_composable_module(ComposableModuleDescriptor {
                    source,
                    file_path,
                    language: ShaderLanguage::Wgsl,
                    ..Default::default()
                })
                .map(|_| ())
        })
        .and_then(|_| {
            composer.make_naga_module(NagaModuleDescriptor {
                source,
                file_path,
                shader_type: ShaderType::Wgsl,
                ..Default::default()
            })
        });
    result.map_err(|e: ComposerError| e.emit_to_string(&composer))
}

/// Crée un module de shader wgpu à partir d'une source WGSL utilisant les imports partagés
pub fn create_shader_module(device: &wgpu::Device, label: &str, source: &str, file_path: &str) -> wgpu::ShaderModule {
    let module = compose(source, file_path).unwrap_or_else(|e| panic!("Échec d'assemblage de {file_path} :\n{e}"));
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
    })
}

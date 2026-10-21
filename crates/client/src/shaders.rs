//! Assemblage des shaders WGSL avec naga_oil : `common.wgsl` est un module importable
//! (`#import herocraft::common::{...}`) partagé par le ciel, le terrain et les nuages.

use std::borrow::Cow;

use naga_oil::compose::{ComposableModuleDescriptor, Composer, ComposerError, NagaModuleDescriptor, ShaderLanguage, ShaderType};

const COMMON_WGSL: &str = include_str!("common.wgsl");

/// Assemble un shader WGSL et ses imports en un module naga (message d'erreur lisible sinon)
pub fn compose(source: &str, file_path: &str) -> Result<naga::Module, String> {
    herocraft_common::prof_scope!("render::shaders::compose");
    let mut composer = Composer::default();
    let result = composer
        .add_composable_module(ComposableModuleDescriptor {
            source: COMMON_WGSL,
            file_path: "common.wgsl",
            language: ShaderLanguage::Wgsl,
            ..Default::default()
        })
        .map(|_| ())
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

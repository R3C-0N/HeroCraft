//! Modèles à squelette des créatures, lus dans des fichiers glTF 2.0 (données embarquées en base64,
//! comme ceux du bestiaire) et leur texture PNG.
//!
//! Seul ce dont les créatures ont besoin est lu : un maillage d'une primitive (positions, normales,
//! coordonnées de texture, os et poids), le squelette (nœuds, matrices de liaison inverses) et les
//! animations de rotation, translation et échelle des os. Chaque sommet suit les quatre os que lui
//! donnent ses poids.

use anyhow::Context;
use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;

/// Un sommet du modèle, dans sa pose de liaison
#[derive(Debug, Clone, Copy)]
pub struct ModelVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    /// Os (rangs dans `joints`) et leurs poids
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    parent: Option<usize>,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Property {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone)]
struct Channel {
    node: usize,
    path: Property,
    times: Vec<f32>,
    /// 3 (translation, échelle) ou 4 (rotation) flottants par instant
    values: Vec<f32>,
    step: bool,
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub duration: f32,
    channels: Vec<Channel>,
}

#[derive(Debug, Clone)]
pub struct SkinnedModel {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    nodes: Vec<Node>,
    /// Nœuds des os, dans l'ordre de `JOINTS_0`
    joints: Vec<usize>,
    inverse_bind: Vec<Mat4>,
    animations: HashMap<String, Animation>,
    /// Hauteur sous l'origine du point le plus bas de la pose de repos : les pieds y sont posés
    pub feet_height: f32,
}

/// Texture RGBA 8 bits, partagée avec les textures des blocs
pub use herocraft_meshing::texture::Texture;

impl SkinnedModel {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let gltf: Gltf = serde_json::from_str(json)?;
        let buffers = gltf
            .buffers
            .iter()
            .map(|b| {
                let data = b.uri.as_deref().and_then(|uri| uri.split_once(";base64,")).map(|(_, data)| data).context("seuls les tampons embarqués en base64 sont lus")?;
                base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| anyhow::anyhow!("tampon illisible : {e}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let reader = Reader { gltf: &gltf, buffers: &buffers };

        let mesh = gltf.meshes.first().context("aucun maillage")?;
        let primitive = mesh.primitives.first().context("maillage vide")?;
        let attribute = |name: &str| primitive.attributes.get(name).copied().with_context(|| format!("attribut {name} manquant"));
        let positions = reader.floats(attribute("POSITION")?, 3)?;
        let normals = reader.floats(attribute("NORMAL")?, 3)?;
        let uvs = reader.floats(attribute("TEXCOORD_0")?, 2)?;
        let joints = reader.integers(attribute("JOINTS_0")?, 4)?;
        let weights = reader.floats(attribute("WEIGHTS_0")?, 4)?;
        let count = positions.len() / 3;
        if [normals.len() / 3, uvs.len() / 2, joints.len() / 4, weights.len() / 4].iter().any(|&n| n != count) {
            anyhow::bail!("attributs de tailles différentes");
        }
        let vertices = (0..count)
            .map(|i| ModelVertex {
                position: Vec3::from_slice(&positions[i * 3..]),
                normal: Vec3::from_slice(&normals[i * 3..]),
                uv: [uvs[i * 2], uvs[i * 2 + 1]],
                joints: std::array::from_fn(|k| joints[i * 4 + k] as u16),
                weights: std::array::from_fn(|k| weights[i * 4 + k]),
            })
            .collect::<Vec<_>>();
        let indices = match primitive.indices {
            Some(accessor) => reader.integers(accessor, 1)?,
            None => (0..count as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= count) {
            anyhow::bail!("indice hors du maillage");
        }

        let mut nodes: Vec<Node> = gltf
            .nodes
            .iter()
            .map(|n| Node {
                name: n.name.clone().unwrap_or_default(),
                parent: None,
                translation: n.translation.map_or(Vec3::ZERO, Vec3::from),
                rotation: n.rotation.map_or(Quat::IDENTITY, Quat::from_array),
                scale: n.scale.map_or(Vec3::ONE, Vec3::from),
            })
            .collect();
        for (i, n) in gltf.nodes.iter().enumerate() {
            for &child in &n.children {
                nodes.get_mut(child).context("nœud enfant inconnu")?.parent = Some(i);
            }
        }

        let skin = gltf.skins.first().context("aucun squelette")?;
        if skin.joints.iter().any(|&j| j >= nodes.len()) {
            anyhow::bail!("os inconnu");
        }
        let inverse_bind = match skin.inverse_bind_matrices {
            Some(accessor) => reader.floats(accessor, 16)?.chunks(16).map(Mat4::from_cols_slice).collect(),
            None => vec![Mat4::IDENTITY; skin.joints.len()],
        };
        if inverse_bind.len() != skin.joints.len() || vertices.iter().any(|v| v.joints.iter().any(|&j| j as usize >= skin.joints.len())) {
            anyhow::bail!("squelette incohérent");
        }

        let mut animations = HashMap::new();
        for (index, animation) in gltf.animations.iter().enumerate() {
            let mut channels = Vec::new();
            for channel in &animation.channels {
                let sampler = animation.samplers.get(channel.sampler).context("échantillonneur inconnu")?;
                let (path, width) = match channel.target.path.as_str() {
                    "translation" => (Property::Translation, 3),
                    "rotation" => (Property::Rotation, 4),
                    "scale" => (Property::Scale, 3),
                    _ => continue,
                };
                let Some(node) = channel.target.node.filter(|&n| n < nodes.len()) else { continue };
                let times = reader.floats(sampler.input, 1)?;
                let values = reader.floats(sampler.output, width)?;
                if times.is_empty() || values.len() != times.len() * width {
                    anyhow::bail!("canal d'animation incohérent");
                }
                channels.push(Channel { node, path, times, values, step: sampler.interpolation.as_deref() == Some("STEP") });
            }
            let duration = channels.iter().filter_map(|c| c.times.last().copied()).fold(0.0, f32::max);
            let name = animation.name.clone().unwrap_or_else(|| format!("animation{index}"));
            animations.insert(name, Animation { duration, channels });
        }

        let mut model = Self { vertices, indices, nodes, joints: skin.joints.clone(), inverse_bind, animations, feet_height: 0.0 };
        let rest = model.skin_matrices(None, 0.0);
        model.feet_height = -model.vertices.iter().map(|v| skin_point(&rest, v, v.position).y).fold(f32::MAX, f32::min);
        Ok(model)
    }

    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.get(name)
    }

    /// Nœuds nommés `pattern`, ou dont le nom commence par `pattern` sans son `*` final, dans
    /// l'ordre du fichier
    pub fn nodes_matching(&self, pattern: &str) -> Vec<usize> {
        let matches = |name: &str| match pattern.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == pattern,
        };
        self.nodes.iter().enumerate().filter(|(_, n)| matches(&n.name)).map(|(i, _)| i).collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Nom de l'os de rang `joint` (celui des `joints` d'un sommet). Les pièces greffées sur le
    /// héros (`assets/skills/models`) portent les noms de ses os.
    #[allow(dead_code)] // lu par les pièces greffées des compétences
    pub fn joint_name(&self, joint: u16) -> Option<&str> {
        self.joints.get(joint as usize).map(|&node| self.nodes[node].name.as_str())
    }

    /// Matrices de chaque os (pose de l'animation à l'instant `time`, ou de repos) : elles amènent
    /// un sommet de la pose de liaison à sa place dans le repère du modèle
    pub fn skin_matrices(&self, animation: Option<&Animation>, time: f32) -> Vec<Mat4> {
        self.posed_matrices(animation, time, &[])
    }

    /// Comme `skin_matrices`, avec en plus, pour chaque nœud (rang dans le glTF), une rotation
    /// appliquée après la sienne et un déplacement ajouté au sien (`offsets` vide : aucun)
    pub fn posed_matrices(&self, animation: Option<&Animation>, time: f32, offsets: &[(Quat, Vec3)]) -> Vec<Mat4> {
        let mut locals: Vec<(Vec3, Quat, Vec3)> = self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect();
        if let Some(animation) = animation {
            for channel in &animation.channels {
                let local = &mut locals[channel.node];
                match channel.path {
                    Property::Translation => local.0 = Vec3::from_slice(&channel.sample(time, 3)),
                    Property::Rotation => local.1 = Quat::from_slice(&channel.sample(time, 4)).normalize(),
                    Property::Scale => local.2 = Vec3::from_slice(&channel.sample(time, 3)),
                }
            }
        }
        for (local, (rotation, translation)) in locals.iter_mut().zip(offsets) {
            local.0 += *translation;
            local.1 = (local.1 * *rotation).normalize();
        }
        let mut globals: Vec<Option<Mat4>> = vec![None; self.nodes.len()];
        self.joints
            .iter()
            .zip(&self.inverse_bind)
            .map(|(&joint, inverse_bind)| self.global(joint, &locals, &mut globals) * *inverse_bind)
            .collect()
    }

    fn global(&self, node: usize, locals: &[(Vec3, Quat, Vec3)], globals: &mut [Option<Mat4>]) -> Mat4 {
        if let Some(m) = globals[node] {
            return m;
        }
        let (t, r, s) = locals[node];
        let local = Mat4::from_scale_rotation_translation(s, r, t);
        let m = match self.nodes[node].parent {
            Some(parent) => self.global(parent, locals, globals) * local,
            None => local,
        };
        globals[node] = Some(m);
        m
    }
}

/// Position d'un point du sommet `v` emporté par ses os
pub fn skin_point(matrices: &[Mat4], v: &ModelVertex, p: Vec3) -> Vec3 {
    let mut out = Vec3::ZERO;
    for k in 0..4 {
        if v.weights[k] > 0.0 {
            out += matrices[v.joints[k] as usize].transform_point3(p) * v.weights[k];
        }
    }
    out
}

/// Direction de la normale du sommet `v` emportée par ses os
pub fn skin_normal(matrices: &[Mat4], v: &ModelVertex) -> Vec3 {
    let mut out = Vec3::ZERO;
    for k in 0..4 {
        if v.weights[k] > 0.0 {
            out += matrices[v.joints[k] as usize].transform_vector3(v.normal) * v.weights[k];
        }
    }
    out.normalize_or_zero()
}

impl Channel {
    /// Valeur interpolée à l'instant `time` (bornée aux extrémités)
    fn sample(&self, time: f32, width: usize) -> Vec<f32> {
        let at = |i: usize| &self.values[i * width..(i + 1) * width];
        let next = self.times.partition_point(|&t| t <= time);
        if next == 0 {
            return at(0).to_vec();
        }
        if next >= self.times.len() || self.step {
            return at(next - 1).to_vec();
        }
        let (t0, t1) = (self.times[next - 1], self.times[next]);
        let k = if t1 > t0 { (time - t0) / (t1 - t0) } else { 0.0 };
        if width == 4 {
            let (a, b) = (Quat::from_slice(at(next - 1)), Quat::from_slice(at(next)));
            return a.slerp(b, k).to_array().to_vec();
        }
        at(next - 1).iter().zip(at(next)).map(|(a, b)| a + (b - a) * k).collect()
    }
}

// --- Lecture du glTF ---

#[derive(Deserialize)]
struct Gltf {
    #[serde(default)]
    nodes: Vec<GltfNode>,
    #[serde(default)]
    meshes: Vec<GltfMesh>,
    #[serde(default)]
    skins: Vec<GltfSkin>,
    #[serde(default)]
    accessors: Vec<GltfAccessor>,
    #[serde(rename = "bufferViews", default)]
    buffer_views: Vec<GltfBufferView>,
    #[serde(default)]
    buffers: Vec<GltfBuffer>,
    #[serde(default)]
    animations: Vec<GltfAnimation>,
}

#[derive(Deserialize)]
struct GltfNode {
    name: Option<String>,
    #[serde(default)]
    children: Vec<usize>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    scale: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct GltfMesh {
    primitives: Vec<GltfPrimitive>,
}

#[derive(Deserialize)]
struct GltfPrimitive {
    attributes: HashMap<String, usize>,
    indices: Option<usize>,
}

#[derive(Deserialize)]
struct GltfSkin {
    joints: Vec<usize>,
    #[serde(rename = "inverseBindMatrices")]
    inverse_bind_matrices: Option<usize>,
}

#[derive(Deserialize)]
struct GltfAccessor {
    #[serde(rename = "bufferView")]
    buffer_view: usize,
    #[serde(rename = "byteOffset", default)]
    byte_offset: usize,
    #[serde(rename = "componentType")]
    component_type: u32,
    count: usize,
}

#[derive(Deserialize)]
struct GltfBufferView {
    buffer: usize,
    #[serde(rename = "byteOffset", default)]
    byte_offset: usize,
    #[serde(rename = "byteLength")]
    byte_length: usize,
    #[serde(rename = "byteStride")]
    byte_stride: Option<usize>,
}

#[derive(Deserialize)]
struct GltfBuffer {
    uri: Option<String>,
}

#[derive(Deserialize)]
struct GltfAnimation {
    name: Option<String>,
    channels: Vec<GltfChannel>,
    samplers: Vec<GltfSampler>,
}

#[derive(Deserialize)]
struct GltfChannel {
    sampler: usize,
    target: GltfTarget,
}

#[derive(Deserialize)]
struct GltfTarget {
    node: Option<usize>,
    path: String,
}

#[derive(Deserialize)]
struct GltfSampler {
    input: usize,
    output: usize,
    interpolation: Option<String>,
}

struct Reader<'a> {
    gltf: &'a Gltf,
    buffers: &'a [Vec<u8>],
}

impl Reader<'_> {
    /// Octets de chaque élément de l'accesseur (`width` composantes de `size` octets)
    fn elements(&self, accessor: usize, width: usize, size: usize) -> anyhow::Result<(Vec<&[u8]>, u32)> {
        let a = self.gltf.accessors.get(accessor).context("accesseur inconnu")?;
        let view = self.gltf.buffer_views.get(a.buffer_view).context("vue de tampon inconnue")?;
        let buffer = self.buffers.get(view.buffer).context("tampon inconnu")?;
        let element = width * size;
        let stride = view.byte_stride.unwrap_or(element);
        let start = view.byte_offset + a.byte_offset;
        let end = view.byte_offset + view.byte_length;
        if a.count > 0 && (start + stride * (a.count - 1) + element > end || end > buffer.len()) {
            anyhow::bail!("accesseur hors de son tampon");
        }
        Ok(((0..a.count).map(|i| &buffer[start + i * stride..start + i * stride + element]).collect(), a.component_type))
    }

    fn floats(&self, accessor: usize, width: usize) -> anyhow::Result<Vec<f32>> {
        let (elements, kind) = self.elements(accessor, width, 4)?;
        if kind != 5126 {
            anyhow::bail!("flottants attendus (type {kind})");
        }
        Ok(elements.iter().flat_map(|e| e.chunks(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect())
    }

    fn integers(&self, accessor: usize, width: usize) -> anyhow::Result<Vec<u32>> {
        let kind = self.gltf.accessors.get(accessor).context("accesseur inconnu")?.component_type;
        let size = match kind {
            5121 => 1,
            5123 => 2,
            5125 => 4,
            _ => anyhow::bail!("entiers attendus (type {kind})"),
        };
        let (elements, _) = self.elements(accessor, width, size)?;
        Ok(elements
            .iter()
            .flat_map(|e| {
                e.chunks(size).map(|b| match size {
                    1 => b[0] as u32,
                    2 => u16::from_le_bytes([b[0], b[1]]) as u32,
                    _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::CreatureRegistry;

    #[test]
    fn test_bestiary_models_load() {
        let registry = CreatureRegistry::load_default();
        assert!(registry.len() >= 19);
        for species in registry.iter().filter(|s| !s.humanoid) {
            let model = SkinnedModel::load(&registry.asset_path(&species.model.mesh)).unwrap_or_else(|e| panic!("{} : {e}", species.id));
            // Sans animations dans sa fiche, la bête joue l'animation `repos` du glTF
            if species.model.animations.is_empty() {
                let animation = model.animation("repos").unwrap_or_else(|| panic!("{} : pas d'animation", species.id));
                assert!(animation.duration > 0.0);
            }
            // Un seul os porte tout le corps (`racine` dans le bestiaire, `root` dans les
            // créatures des compétences)
            assert_eq!(model.nodes_matching("racine").len() + model.nodes_matching("root").len(), 1, "{}", species.id);
            // Les pieds sont sous l'origine (ou dessus), à la hauteur du corps à peu près
            assert!(model.feet_height >= 0.0 && model.feet_height < species.body.height * 1.5, "{} : {}", species.id, model.feet_height);
            let texture = Texture::load(&registry.asset_path(&species.model.texture)).unwrap();
            assert_eq!(texture.rgba.len(), (texture.width * texture.height * 4) as usize);
        }
    }

    #[test]
    fn test_animation_moves_the_legs() {
        let registry = CreatureRegistry::load_default();
        let species = registry.get(registry.find("sanglier").unwrap()).unwrap();
        let model = SkinnedModel::load(&registry.asset_path(&species.model.mesh)).unwrap();
        let animation = model.animation("repos").unwrap();
        let at = |t: f32| {
            let m = model.skin_matrices(Some(animation), t);
            model.vertices.iter().map(|v| skin_point(&m, v, v.position)).collect::<Vec<_>>()
        };
        let (a, b) = (at(0.0), at(animation.duration * 0.25));
        let moved = a.iter().zip(&b).map(|(p, q)| p.distance(*q)).fold(0.0, f32::max);
        assert!(moved > 0.02, "rien ne bouge ({moved})");
        // Pose de repos = première image de l'animation, à peu près
        let rest = model.skin_matrices(None, 0.0);
        let rest: Vec<Vec3> = model.vertices.iter().map(|v| skin_point(&rest, v, v.position)).collect();
        assert!(rest.iter().zip(&a).all(|(p, q)| p.distance(*q) < 0.2));
    }
}

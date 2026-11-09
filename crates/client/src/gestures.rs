//! Gestes du héros joués par les compétences (`assets/hero/animations/*.json`, décrits dans le
//! README du dossier) : une suite de poses clés, interpolées entre elles.

use std::collections::HashMap;
use std::path::Path;

use glam::{Quat, Vec3};
use serde::Deserialize;

use crate::hero_model::{Bone, BonePose, FigurePose};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Ease {
    #[default]
    InOut,
    In,
    Out,
    Linear,
}

impl Ease {
    fn apply(self, s: f32) -> f32 {
        let s = s.clamp(0.0, 1.0);
        match self {
            Ease::InOut => s * s * (3.0 - 2.0 * s),
            Ease::In => s * s,
            Ease::Out => 1.0 - (1.0 - s) * (1.0 - s),
            Ease::Linear => s,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct BoneFile {
    #[serde(default)]
    rotation: Option<[f32; 3]>,
    #[serde(default)]
    translation: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyFile {
    t: f32,
    #[serde(default)]
    ease: Ease,
    #[serde(default)]
    bones: HashMap<String, BoneFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct GestureFile {
    duration: f32,
    legs: String,
    keys: Vec<KeyFile>,
}

/// Une pose clé : chaque os, en rotation et en déplacement (blocs), au repos s'il n'est pas cité
#[derive(Debug, Clone)]
struct Key {
    t: f32,
    ease: Ease,
    bones: [(Quat, Vec3); Bone::COUNT],
}

/// Un geste prêt à jouer
#[derive(Debug, Clone)]
pub struct Gesture {
    pub duration: f32,
    /// La marche s'arrête le temps du geste
    pub legs_locked: bool,
    keys: Vec<Key>,
}

impl Gesture {
    fn from_file(file: GestureFile) -> Result<Self, String> {
        if !(file.duration > 0.0) || file.keys.len() < 2 {
            return Err("durée positive et deux clés au moins".into());
        }
        let mut keys = Vec::new();
        for key in file.keys {
            let mut bones = [(Quat::IDENTITY, Vec3::ZERO); Bone::COUNT];
            for (name, pose) in key.bones {
                let bone = Bone::from_name(&name).ok_or_else(|| format!("os inconnu {name}"))?;
                if let Some(r) = pose.rotation {
                    bones[bone.index()].0 = bone.rotation_from_degrees(r);
                }
                if let Some(t) = pose.translation {
                    bones[bone.index()].1 = Vec3::from(t);
                }
            }
            keys.push(Key { t: key.t, ease: key.ease, bones });
        }
        Ok(Self { duration: file.duration, legs_locked: file.legs == "locked", keys })
    }

    /// La pose à l'instant `u` du geste (0 à 1), écrite dans `pose` ; `units_per_block` convertit
    /// les déplacements en unités du patron (`Figure::units_per_block`)
    pub fn pose(&self, u: f32, units_per_block: f32, pose: &mut FigurePose) {
        let u = u.clamp(0.0, 1.0);
        let next = self.keys.iter().position(|k| k.t >= u).unwrap_or(self.keys.len() - 1).max(1);
        let (a, b) = (&self.keys[next - 1], &self.keys[next]);
        let span = (b.t - a.t).max(1e-6);
        let s = b.ease.apply((u - a.t) / span);
        for bone in Bone::ALL {
            let i = bone.index();
            let rotation = a.bones[i].0.slerp(b.bones[i].0, s);
            let translation = a.bones[i].1.lerp(b.bones[i].1, s) * units_per_block;
            pose.bones[i] = BonePose { rotation, translation };
        }
    }
}

/// Tous les gestes, par nom
#[derive(Debug, Default)]
pub struct GestureLibrary {
    gestures: HashMap<String, Gesture>,
}

impl GestureLibrary {
    /// Les gestes de `assets/hero/animations/` ; un fichier invalide est signalé et ignoré
    pub fn load_default() -> Self {
        let Some(dir) = herocraft_common::skills::asset_dir(herocraft_common::skills::GESTURES_DIR) else {
            log::warn!("Dossier des gestes introuvable : le héros ne fera pas ses gestes.");
            return Self::default();
        };
        let (library, errors) = Self::load_dir(&dir);
        for e in errors {
            log::warn!("{e}");
        }
        library
    }

    fn load_dir(dir: &Path) -> (Self, Vec<String>) {
        let mut gestures = HashMap::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<GestureFile>(&t).map_err(|e| e.to_string()))
                .and_then(Gesture::from_file);
            match parsed {
                Ok(g) => {
                    gestures.insert(name, g);
                }
                Err(e) => errors.push(format!("geste {name} : {e}")),
            }
        }
        (Self { gestures }, errors)
    }

    pub fn get(&self, name: &str) -> Option<&Gesture> {
        self.gestures.get(name)
    }

    pub fn len(&self) -> usize {
        self.gestures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gesture_loads_and_starts_and_ends_at_rest() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/hero/animations");
        let (library, errors) = GestureLibrary::load_dir(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(library.len(), 15);
        let slam = library.get("groundSlam").unwrap();
        assert!(slam.legs_locked);
        let mut pose = FigurePose::default();
        slam.pose(0.0, 10.0, &mut pose);
        assert!(pose.bones.iter().all(|b| b.rotation.angle_between(Quat::IDENTITY) < 1e-4));
        // À l'impact, le bras droit est loin du repos
        slam.pose(0.35, 10.0, &mut pose);
        assert!(pose.bone(Bone::RightArm).rotation.angle_between(Quat::IDENTITY) > 0.5);
        assert!(pose.bone(Bone::Body).translation.y < 0.0, "le corps se baisse");
        slam.pose(1.0, 10.0, &mut pose);
        assert!(pose.bones.iter().all(|b| b.rotation.angle_between(Quat::IDENTITY) < 1e-4));
    }
}

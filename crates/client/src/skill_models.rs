//! Vérifie les modèles 3D des compétences (`assets/skills/models`, construits par
//! `tools/modeles_competences/construire.py`) et en tire des aperçus hors jeu :
//! - objets posés (tourelles, pièges, murs…) : un glTF et ses animations (`fire`, `snap`, `rise`…) ;
//! - pièces greffées sur le héros (transformations partielles du Druide, armes des invocations) :
//!   un glTF dont les os portent les noms des os du héros, en texels du patron × 0,05625 bloc ;
//! - invocations humanoïdes : une planche 64 × 64 au patron du héros (`humanoids.json`).

use std::path::{Path, PathBuf};

use glam::Vec3;
use herocraft_common::hero::{atlas, Appearance, Sheet, SHEET_SIZE};
use herocraft_common::tangent_frame;
use serde::Deserialize;

use crate::creatures::CreatureFrame;
use crate::hero_model::{figure, push_figure, Bone, Figure, FigurePose};
use crate::planet_mesh::PlanetVertex;
use crate::skinned_model::{SkinnedModel, Texture};

/// Taille d'un texel du patron du héros en blocs : l'unité des pièces greffées
const HERO_TEXEL: f32 = 0.05625;

#[derive(Deserialize)]
struct Humanoid {
    sheet: String,
    species: String,
    weapon: String,
}

#[derive(Deserialize)]
struct Tinted {
    tint: [f32; 3],
}

#[derive(Deserialize)]
struct Catalogue {
    humanoids: std::collections::BTreeMap<String, Humanoid>,
    tinted: std::collections::BTreeMap<String, Tinted>,
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/skills/models")
}

fn catalogue() -> Catalogue {
    serde_json::from_str(&std::fs::read_to_string(dir().join("humanoids.json")).unwrap()).unwrap()
}

fn sheet(file: &str) -> Sheet {
    let texture = Texture::load(&dir().join(file)).unwrap();
    assert_eq!((texture.width, texture.height), (SHEET_SIZE as u32, SHEET_SIZE as u32), "{file}");
    Sheet { rgba: texture.rgba }
}

fn appearance(species: &str) -> Appearance {
    let mut cfg = atlas().defaut(species);
    cfg.retouch(true);
    cfg
}

/// Ajoute une pièce greffée, posée comme la figure : chaque sommet suit l'os du héros dont il porte
/// le nom
#[allow(clippy::too_many_arguments)]
fn push_graft(frame: &mut CreatureFrame, model: &SkinnedModel, cfg: &Appearance, figure: &Figure, feet: Vec3, yaw: f32, pose: &FigurePose, texture: usize) {
    // Le même repère que `push_figure`
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let left = up.cross(forward);
    let unit = cfg.taille as f32 / figure.body_height;
    let frames = figure.bone_frames(pose);
    let to_world = |p: Vec3| feet + (left * p.x + up * p.y + forward * p.z) * unit;
    let start = frame.indices.len() as u32;
    let base = frame.vertices.len() as u32;
    for v in &model.vertices {
        let name = model.joint_name(v.joints[0]).unwrap();
        let bone = Bone::from_name(name).unwrap_or_else(|| panic!("os inconnu du héros : {name}"));
        let f = frames[bone.index()];
        let p = f.apply(v.position / HERO_TEXEL);
        let n = f.rotation * v.normal;
        frame.vertices.push(PlanetVertex {
            position: to_world(p).to_array(),
            normal: (left * n.x + up * n.y + forward * n.z).to_array(),
            color: [pose.tint[0], pose.tint[1], pose.tint[2], 1.0],
            texture: PlanetVertex::UNTEXTURED,
            light: 0.0,
        });
        frame.uvs.push(v.uv);
    }
    frame.indices.extend(model.indices.iter().map(|i| base + i));
    frame.draws.push((Some(texture), start..frame.indices.len() as u32));
}

#[test]
fn every_skill_model_loads_with_its_texture_and_clips() {
    let expected = [
        ("ballisticTurret", vec!["fire"]),
        ("flameTurret", vec!["fire"]),
        ("jawTrap", vec!["snap"]),
        ("magneticTrap", vec!["spin"]),
        ("shrapnelMine", vec!["blink"]),
        ("portableBarricade", vec!["unfold"]),
        ("etheriumBastion", vec!["rise", "fire"]),
        ("stoneWall", vec!["rise"]),
        ("boneWall", vec!["rise"]),
    ];
    for (id, clips) in expected {
        let model = SkinnedModel::load(&dir().join(format!("{id}.gltf"))).unwrap_or_else(|e| panic!("{id} : {e}"));
        Texture::load(&dir().join(format!("{id}.png"))).unwrap_or_else(|e| panic!("{id} : {e}"));
        assert!(!model.vertices.is_empty(), "{id}");
        for clip in clips {
            assert!(model.animation(clip).is_some_and(|a| a.duration > 0.0), "{id} : pas d'animation {clip}");
        }
        // Posé sur le sol : rien sous les pieds au repos
        assert!(model.feet_height.abs() < 0.05, "{id} : {}", model.feet_height);
    }
}

#[test]
fn grafts_and_weapons_hang_on_hero_bones() {
    let mut weapons: Vec<String> = catalogue().humanoids.into_values().map(|h| h.weapon).collect();
    weapons.extend(["wolfPaw", "bearHide", "elkLegs", "galeFeathers"].map(String::from));
    for id in weapons {
        let model = SkinnedModel::load(&dir().join(format!("{id}.gltf"))).unwrap_or_else(|e| panic!("{id} : {e}"));
        Texture::load(&dir().join(format!("{id}.png"))).unwrap();
        for v in &model.vertices {
            let name = model.joint_name(v.joints[0]).unwrap();
            assert!(Bone::from_name(name).is_some(), "{id} : {name} n'est pas un os du héros");
        }
    }
}

#[test]
fn humanoid_summons_are_hero_sheets_on_a_known_species() {
    let catalogue = catalogue();
    assert_eq!(catalogue.humanoids.len(), 3);
    for (id, h) in &catalogue.humanoids {
        assert!(atlas().especes().any(|e| e.id == h.species), "{id} : espèce {}", h.species);
        let f = figure(&appearance(&h.species), &sheet(&h.sheet));
        assert!(f.pieces.iter().map(|p| p.quads.len()).sum::<usize>() > 100, "{id}");
    }
    for (id, t) in &catalogue.tinted {
        assert!(t.tint.iter().all(|c| (0.0..=2.0).contains(c)), "{id}");
    }
}

/// Une ligne de héros : invocations humanoïdes armées, copies teintées, greffes du Druide
#[test]
#[ignore]
fn apercu_competences_heros() {
    use crate::creatures::apercu::{rasterize, save};

    let catalogue = catalogue();
    let cell: usize = std::env::var("APERCU_CASE").ok().and_then(|c| c.parse().ok()).unwrap_or(300);
    let feet = Vec3::new(0.0, 0.0, 1000.0);
    let human = appearance("humain");
    let hero_sheet = herocraft_common::hero::paint(&human);
    let hero_texture = Texture { width: SHEET_SIZE as u32, height: SHEET_SIZE as u32, rgba: hero_sheet.rgba.clone() };
    let hero = figure(&human, &hero_sheet);

    // (planche, apparence, figure, greffe, teinte, pose)
    type Cell = (Texture, Appearance, Figure, Option<String>, [f32; 3], FigurePose);
    let mut cells: Vec<Cell> = Vec::new();
    let rest = FigurePose::default();
    let mut ready = FigurePose::default();
    ready.raise_right_arm(0.5);
    for h in catalogue.humanoids.values() {
        let s = sheet(&h.sheet);
        let cfg = appearance(&h.species);
        let f = figure(&cfg, &s);
        cells.push((Texture { width: 64, height: 64, rgba: s.rgba }, cfg, f, Some(h.weapon.clone()), [1.0; 3], ready));
    }
    for t in catalogue.tinted.values() {
        cells.push((hero_texture.clone(), human.clone(), hero.clone(), None, t.tint, rest));
    }
    let mut flap = FigurePose::default();
    flap.bone_mut(Bone::RightArm).rotation = Bone::RightArm.rotation_from_degrees([0.0, 0.0, 80.0]);
    flap.bone_mut(Bone::LeftArm).rotation = Bone::LeftArm.rotation_from_degrees([0.0, 0.0, 80.0]);
    let mut claw = FigurePose::default();
    claw.raise_left_arm(1.2);
    claw.raise_right_arm(0.6);
    let mut stride = FigurePose::default();
    stride.bone_mut(Bone::RightLeg).rotation = Bone::RightLeg.rotation_from_degrees([25.0, 0.0, 0.0]);
    stride.bone_mut(Bone::LeftLeg).rotation = Bone::LeftLeg.rotation_from_degrees([-20.0, 0.0, 0.0]);
    stride.bone_mut(Bone::LeftShin).rotation = Bone::LeftShin.rotation_from_degrees([30.0, 0.0, 0.0]);
    for (graft, pose) in [("wolfPaw", claw), ("bearHide", rest), ("elkLegs", stride), ("galeFeathers", flap)] {
        cells.push((hero_texture.clone(), human.clone(), hero.clone(), Some(graft.into()), [1.0; 3], pose));
    }

    let (w, h) = (cells.len() * cell, cell);
    let mut img = vec![[205u8, 225, 238]; w * h];
    let mut depth = vec![f32::MAX; w * h];
    for (k, (texture, cfg, f, graft, tint, pose)) in cells.into_iter().enumerate() {
        let pose = FigurePose { tint, ..pose };
        // Face à la caméra (au sud), de trois quarts
        let yaw = std::f32::consts::PI + if k >= 5 { -0.5 } else { 0.5 };
        let mut frame = CreatureFrame::default();
        push_figure(&mut frame, &cfg, &f, feet, yaw, &pose, 0, 1.0);
        let scale = cell as f32 * 0.8 / 2.2;
        rasterize(&mut img, &mut depth, w, &frame, &texture, feet, (k * cell, 0, cell), scale);
        if let Some(graft) = graft {
            let model = SkinnedModel::load(&dir().join(format!("{graft}.gltf"))).unwrap();
            let gtex = Texture::load(&dir().join(format!("{graft}.png"))).unwrap();
            let mut frame = CreatureFrame::default();
            push_graft(&mut frame, &model, &cfg, &f, feet, yaw, &pose, 0);
            rasterize(&mut img, &mut depth, w, &frame, &gtex, feet, (k * cell, 0, cell), scale);
        }
    }
    save(&img, w, h);
}

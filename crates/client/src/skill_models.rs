//! Modèles 3D des compétences (`assets/skills/models`, construits par
//! `tools/modeles_competences/construire.py`) :
//! - objets posés (tourelles, pièges, murs…) : un glTF et ses animations (`fire`, `snap`, `rise`…),
//!   dessinés comme les créatures (leur fiche est dans `assets/creatures/`) ;
//! - pièces greffées sur le héros (transformations partielles du Druide, armes des invocations) :
//!   un glTF dont les os portent les noms des os du héros, en texels du patron × 0,05625 bloc ;
//! - invocations humanoïdes : une planche 64 × 64 au patron du héros (`humanoids.json`), portée
//!   par une créature humanoïde du même identifiant, l'arme greffée dans la main droite.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use glam::{Quat, Vec3, vec3};
use herocraft_common::hero::{Appearance, SHEET_SIZE, Sheet, atlas};
use herocraft_common::tangent_frame;
use serde::Deserialize;

use crate::creatures::{CreatureFrame, CreatureView};
use crate::hero_model::{Bone, Figure, FigurePose, figure};
use crate::planet_mesh::PlanetVertex;
use crate::skinned_model::{SkinnedModel, Texture};
use herocraft_common::{COPY_SUMMONER, CreatureRegistry};

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
    herocraft_common::skills::asset_dir("assets/skills/models")
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/skills/models"))
}

fn read_catalogue() -> anyhow::Result<Catalogue> {
    let path = dir().join("humanoids.json");
    let text = std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("{} : {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{} : {e}", path.display()))
}

fn load_sheet(file: &str) -> anyhow::Result<Sheet> {
    let texture = Texture::load(&dir().join(file)).map_err(|e| anyhow::anyhow!("{file} : {e}"))?;
    if (texture.width, texture.height) != (SHEET_SIZE as u32, SHEET_SIZE as u32) {
        anyhow::bail!("{file} : la planche doit faire {SHEET_SIZE} × {SHEET_SIZE}");
    }
    Ok(Sheet { rgba: texture.rgba })
}

fn appearance(species: &str) -> Appearance {
    let mut cfg = atlas().defaut(species);
    cfg.retouch(true);
    cfg
}

/// Une pièce greffée sur le héros : son maillage et sa texture
pub(crate) struct Graft {
    pub id: String,
    pub model: SkinnedModel,
    pub texture: Texture,
}

impl Graft {
    fn load(id: &str) -> anyhow::Result<Self> {
        let model = SkinnedModel::load(&dir().join(format!("{id}.gltf"))).map_err(|e| anyhow::anyhow!("{id} : {e}"))?;
        let texture = Texture::load(&dir().join(format!("{id}.png"))).map_err(|e| anyhow::anyhow!("{id} : {e}"))?;
        if let Some(name) = model.vertices.iter().filter_map(|v| model.joint_name(v.joints[0])).find(|n| Bone::from_name(n).is_none()) {
            anyhow::bail!("{id} : {name} n'est pas un os du héros");
        }
        Ok(Self { id: id.to_string(), model, texture })
    }
}

/// L'allure d'une invocation humanoïde : un corps de héros à sa planche, et son arme
pub(crate) struct SummonLook {
    pub id: String,
    pub appearance: Appearance,
    pub sheet: Texture,
    pub figure: Figure,
    pub weapon: Option<Graft>,
}

/// Les invocations humanoïdes de `humanoids.json`, par identifiant (celui de leur créature),
/// chargées une fois
fn summon_looks() -> &'static HashMap<String, SummonLook> {
    static LOOKS: OnceLock<HashMap<String, SummonLook>> = OnceLock::new();
    LOOKS.get_or_init(|| {
        let catalogue = match read_catalogue() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Invocations humanoïdes illisibles : {e}");
                return HashMap::new();
            }
        };
        let mut looks = HashMap::new();
        for (id, h) in catalogue.humanoids {
            if !atlas().especes().any(|e| e.id == h.species) {
                log::warn!("Invocation {id} : espèce {} inconnue", h.species);
                continue;
            }
            let sheet = match load_sheet(&h.sheet) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Invocation {id} : {e}");
                    continue;
                }
            };
            let cfg = appearance(&h.species);
            let figure = figure(&cfg, &sheet);
            let weapon = Graft::load(&h.weapon).map_err(|e| log::warn!("Invocation {id} : {e}")).ok();
            let size = SHEET_SIZE as u32;
            looks.insert(
                id.clone(),
                SummonLook { id, appearance: cfg, sheet: Texture { width: size, height: size, rgba: sheet.rgba }, figure, weapon },
            );
        }
        looks
    })
}

/// L'allure de l'invocation humanoïde `species` (identifiant de sa créature), s'il y en a une
pub(crate) fn summon_look(species: &str) -> Option<&'static SummonLook> {
    summon_looks().get(species)
}

/// La pièce greffée `id` (`assets/skills/models/<id>.gltf`), lue la première fois qu'on la demande
pub(crate) fn graft(id: &str) -> Option<&'static Graft> {
    static GRAFTS: std::sync::Mutex<Option<HashMap<String, Option<&'static Graft>>>> = std::sync::Mutex::new(None);
    let mut grafts = GRAFTS.lock().unwrap_or_else(|e| e.into_inner());
    *grafts.get_or_insert_with(HashMap::new).entry(id.to_string()).or_insert_with(|| match Graft::load(id) {
        Ok(g) => Some(Box::leak(Box::new(g))),
        Err(e) => {
            log::warn!("Pièce greffée illisible : {e}");
            None
        }
    })
}

/// Teinte des copies (leurres, ombres d'Étherium), par identifiant de leur créature
fn copy_tints() -> &'static HashMap<String, [f32; 3]> {
    static TINTS: OnceLock<HashMap<String, [f32; 3]>> = OnceLock::new();
    TINTS.get_or_init(|| read_catalogue().map(|c| c.tinted.into_iter().map(|(id, t)| (id, t.tint)).collect()).unwrap_or_default())
}

/// La créature telle qu'elle se dessine : une copie (`CreatureState::copy`) prend l'espèce
/// qu'elle imite et la teinte de son espèce à elle ; une copie de son invocateur reste humanoïde
pub(crate) fn drawn_as(view: &CreatureView, registry: &CreatureRegistry) -> CreatureView {
    let Some(copy) = view.copy else { return *view };
    let own = view.species.and_then(|s| registry.get(s));
    let tint = own.and_then(|s| copy_tints().get(&s.id).copied());
    let species = if copy == COPY_SUMMONER { view.species } else { registry.get(copy as usize).map(|_| copy as usize).or(view.species) };
    CreatureView { species, tint, ..*view }
}

/// Ajoute une pièce greffée, posée comme la figure (`push_figure`) : chaque sommet suit l'os du
/// héros dont il porte le nom
pub(crate) fn push_graft(
    frame: &mut CreatureFrame,
    model: &SkinnedModel,
    cfg: &Appearance,
    figure: &Figure,
    feet: Vec3,
    yaw: f32,
    pose: &FigurePose,
    texture: usize,
    block: f32,
) {
    // Le même repère que `push_figure`
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let left = up.cross(forward);
    let unit = cfg.taille as f32 / figure.body_height * block;
    let lying = pose.lying.clamp(0.0, 1.0);
    let lie = Quat::from_rotation_x(-lying * std::f32::consts::FRAC_PI_2);
    let lift = vec3(0.0, 2.0 * lying, 0.0);
    let frames = figure.bone_frames(pose);
    let to_world = |p: Vec3| feet + (left * p.x + up * p.y + forward * p.z) * unit;
    let start = frame.indices.len() as u32;
    let base = frame.vertices.len() as u32;
    for v in &model.vertices {
        let bone = model.joint_name(v.joints[0]).and_then(Bone::from_name).unwrap_or(Bone::Body);
        let f = frames[bone.index()];
        let p = lie * f.apply(v.position / HERO_TEXEL) + lift;
        let n = lie * (f.rotation * v.normal);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hero_model::push_figure;

    fn catalogue() -> Catalogue {
        read_catalogue().unwrap()
    }

    fn sheet(file: &str) -> Sheet {
        load_sheet(file).unwrap()
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
            ("furnaceBarricade", vec!["unfold"]),
            ("sacredRampart", vec!["rise"]),
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
        assert_eq!(catalogue.humanoids.len(), 2);
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
            let yaw = std::f32::consts::PI + if k >= 4 { -0.5 } else { 0.5 };
            let mut frame = CreatureFrame::default();
            push_figure(&mut frame, &cfg, &f, feet, yaw, &pose, 0, 1.0);
            let scale = cell as f32 * 0.8 / 2.2;
            rasterize(&mut img, &mut depth, w, &frame, &texture, feet, (k * cell, 0, cell), scale);
            if let Some(graft) = graft {
                let model = SkinnedModel::load(&dir().join(format!("{graft}.gltf"))).unwrap();
                let gtex = Texture::load(&dir().join(format!("{graft}.png"))).unwrap();
                let mut frame = CreatureFrame::default();
                push_graft(&mut frame, &model, &cfg, &f, feet, yaw, &pose, 0, 1.0);
                rasterize(&mut img, &mut depth, w, &frame, &gtex, feet, (k * cell, 0, cell), scale);
            }
        }
        save(&img, w, h);
    }

    #[test]
    fn every_humanoid_summon_has_its_look_and_weapon() {
        for id in ["deathKnight", "lichLord"] {
            let look = summon_look(id).unwrap_or_else(|| panic!("{id}"));
            assert!(look.weapon.is_some(), "{id} sans arme");
            let mut frame = CreatureFrame::default();
            let feet = Vec3::new(0.0, 1000.0, 0.0);
            let weapon = look.weapon.as_ref().unwrap();
            push_graft(&mut frame, &weapon.model, &look.appearance, &look.figure, feet, 0.0, &FigurePose::default(), 0, 1.0);
            // L'arme est à hauteur de main, près du corps
            let near = frame.vertices.iter().all(|v| (Vec3::from(v.position) - feet).length() < 3.0);
            assert!(near, "{id} : arme loin du corps");
        }
    }

    #[test]
    fn druid_partial_forms_graft_their_piece_while_they_last() {
        let catalog = herocraft_common::skills::SkillCatalog::load_default();
        for (skill, piece) in [("packClaws", "wolfPaw"), ("bearHide", "bearHide"), ("elkLegs", "elkLegs"), ("galeFeathers", "galeFeathers")]
        {
            let def = catalog.get(skill).unwrap_or_else(|| panic!("{skill}"));
            assert!(def.is_castable(), "{skill}");
            assert_eq!(def.graft.as_deref(), Some(piece), "{skill}");
            assert!(graft(piece).is_some(), "{piece}");
        }
    }
}

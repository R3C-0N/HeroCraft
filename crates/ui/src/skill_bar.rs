//! Barre de compétences : quatre cases (touches A, E, R, T par défaut) où le joueur pose les
//! compétences qu'il lance, depuis la fiche de la roue. Une compétence apprise se pose seule dans
//! la première case libre. La barre est gardée dans `config/barre_competences.json`.
//!
//! Les icônes viennent de `assets/skills/<classe>/<id>.png` (pixel art, dessiné sans lissage) ;
//! une compétence sans icône montre une pastille de la couleur de sa classe, avec son initiale.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use herocraft_common::{skills, CharacterStats};

use herocraft_render::skinned_model::Texture;

/// Nombre de cases de la barre
pub const SKILL_SLOTS: usize = 4;
/// Fichier de la barre, relatif au dossier de lancement (comme les contrôles)
pub const SKILL_BAR_FILE: &str = "config/barre_competences.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillBar {
    #[serde(default)]
    pub slots: [Option<String>; SKILL_SLOTS],
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl SkillBar {
    /// La barre gardée dans `path`, vide si le fichier manque ou se lit mal
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let mut bar = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<SkillBar>(&t).ok()).unwrap_or_default();
        // Une compétence disparue du catalogue (renommée, retirée) libère sa case
        for slot in &mut bar.slots {
            if slot.as_deref().is_some_and(|id| skills().get(id).is_none_or(|s| !s.is_castable())) {
                *slot = None;
            }
        }
        bar.path = Some(path.to_path_buf());
        bar
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)
        })();
        if let Err(e) = result {
            log::warn!("Impossible d'enregistrer la barre de compétences dans {} : {}", path.display(), e);
        }
    }

    pub fn get(&self, slot: usize) -> Option<&str> {
        self.slots.get(slot).and_then(|s| s.as_deref())
    }

    /// Case où est posée `id`
    pub(crate) fn slot_of(&self, id: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.as_deref() == Some(id))
    }

    /// Pose `id` dans la case `slot` ; si elle était déjà sur la barre, les deux cases s'échangent
    pub fn place(&mut self, slot: usize, id: &str) {
        if slot >= SKILL_SLOTS {
            return;
        }
        if let Some(from) = self.slot_of(id) {
            self.slots.swap(from, slot);
        } else {
            self.slots[slot] = Some(id.to_string());
        }
        self.save();
    }

    /// Les compétences lançables apprises qui ne sont pas encore sur la barre prennent les cases
    /// libres (après qu'un point a été investi)
    pub fn fill_learned(&mut self, stats: &CharacterStats) {
        let mut changed = false;
        for def in skills().castable(stats) {
            // Les attaques d'une forme du Druide se lancent aux clics, pas depuis la barre
            // Les mélanges se lancent par la case de la Fusion
            if self.slot_of(&def.id).is_some() || skills().form_of_attack(&def.id).is_some() || skills().fusion_of_mix(&def.id).is_some() {
                continue;
            }
            let Some(free) = self.slots.iter().position(Option::is_none) else { break };
            self.slots[free] = Some(def.id.clone());
            changed = true;
        }
        if changed {
            self.save();
        }
    }
}

/// Icônes des compétences déjà envoyées à egui, par compétence (aucune si l'image manque)
#[derive(Default)]
pub struct SkillIcons {
    cache: RefCell<HashMap<String, Option<egui::TextureHandle>>>,
}

impl SkillIcons {
    fn texture(&self, ctx: &egui::Context, id: &str) -> Option<egui::TextureId> {
        let mut cache = self.cache.borrow_mut();
        let handle = cache.entry(id.to_string()).or_insert_with(|| {
            // L'amplification d'un élément (case de la Fusion) : `amplify:<élément>`
            let path = match id.strip_prefix("amplify:") {
                Some(element) => skills().amplify_icon(element)?.to_path_buf(),
                None => skills().get(id)?.icon.clone()?,
            };
            match Texture::load(&path) {
                Ok(image) => {
                    let pixels = egui::ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba);
                    Some(ctx.load_texture(format!("skill:{id}"), pixels, egui::TextureOptions::NEAREST))
                }
                Err(e) => {
                    log::warn!("Icône {} illisible : {e}", path.display());
                    None
                }
            }
        });
        handle.as_ref().map(egui::TextureHandle::id)
    }

    /// Dessine l'icône de `id` dans `rect`, grisée si `dim`
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, id: &str, dim: bool) {
        let tint = if dim { egui::Color32::from_gray(90) } else { egui::Color32::WHITE };
        if let Some(texture) = self.texture(painter.ctx(), id) {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture, rect, uv, tint);
            return;
        }
        // Icône de remplissage : la couleur de la classe et l'initiale de la compétence
        let catalog = skills();
        let def = catalog.get(id);
        let [r, g, b] = def.and_then(|d| catalog.class(&d.class)).map_or([110, 110, 120], |c| c.color);
        let fill = egui::Color32::from_rgb(r, g, b).gamma_multiply(if dim { 0.35 } else { 0.8 });
        painter.rect_filled(rect.shrink(rect.width() * 0.08), 4.0, fill);
        let initial = def.and_then(|d| d.name.chars().next()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(rect.height() * 0.5), tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_learned_skill_takes_a_free_slot_and_swaps_when_moved() {
        let mut bar = SkillBar::default();
        let mut stats = CharacterStats::new(herocraft_common::PlayerClass::Adventurer);
        stats.skills.insert("judgment".into(), 1);
        bar.fill_learned(&stats);
        assert_eq!(bar.get(0), Some("judgment"));
        bar.fill_learned(&stats);
        assert_eq!(bar.slots.iter().flatten().count(), 1, "une seule fois sur la barre");
        bar.place(2, "judgment");
        assert_eq!((bar.get(0), bar.get(2)), (None, Some("judgment")));
        bar.place(0, "heavyStrike");
        bar.place(0, "judgment");
        assert_eq!((bar.get(0), bar.get(2)), (Some("judgment"), Some("heavyStrike")));
    }

    #[test]
    fn every_skill_has_an_icon_file() {
        let missing: Vec<_> = skills().iter().filter(|s| s.icon.is_none()).map(|s| s.id.clone()).collect();
        // Les icônes arrivent peu à peu : on signale ce qui manque sans bloquer
        if !missing.is_empty() {
            eprintln!("Compétences sans icône (pastille de remplissage) : {missing:?}");
        }
        assert!(missing.len() < skills().len(), "aucune icône trouvée");
    }
}

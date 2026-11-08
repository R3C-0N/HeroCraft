//! Estimation des FPS selon les réglages graphiques.
//!
//! Le banc d'essai (`--benchmark`) mesure ce que change chaque réglage par rapport à la référence
//! (qualité haute de la carte, tout activé) et l'enregistre dans `config/couts_graphiques.json`.
//! Le menu Options > Affichage relit ces coûts et additionne ceux des réglages choisis : la durée
//! estimée d'une image est celle de la référence, plus l'écart de chaque réglage qui en diffère.
//!
//! Le calcul suppose que les coûts s'additionnent. Le banc d'essai le vérifie sur les qualités
//! moyenne et basse, qui changent plusieurs réglages à la fois (section « Vérification de
//! l'estimation » du rapport). Il ne vaut que pour la résolution du banc d'essai, et ne tient pas
//! compte de la V-Sync ni de la limite d'images.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ui::{LodDistanceSetting, LodPrecisionSetting};

/// Coûts mesurés par le dernier banc d'essai
pub const COSTS_FILE: &str = "config/couts_graphiques.json";

/// Écart de durée d'image de chaque réglage par rapport à la référence, en ms (positif : plus lent)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CostProfile {
    /// Mesuré sur cette machine (faux : valeurs d'exemple intégrées au jeu)
    pub measured: bool,
    pub gpu: String,
    pub resolution: String,
    /// Date du banc d'essai (secondes depuis 1970)
    pub timestamp: u64,
    /// Puce intégrée : la référence a alors les ombres et les rayons allégés et le MSAA coupé
    pub integrated_gpu: bool,
    /// Durée d'une image avec les réglages de référence
    pub reference_ms: f32,
    /// Ombres du relief coupées (coupe aussi les ombres des nuages et les rayons)
    pub no_shadows: f32,
    pub no_cloud_shadows: f32,
    pub no_clouds: f32,
    pub no_god_rays: f32,
    pub no_bloom: f32,
    /// Finesse des ombres, des rayons ou MSAA inversés par rapport à la référence
    pub flip_fine_shadows: f32,
    pub flip_fine_god_rays: f32,
    pub flip_msaa: f32,
    pub lod_short: f32,
    pub lod_high: f32,
    pub precision_ultra: f32,
    pub precision_exact: f32,
}

impl Default for CostProfile {
    /// Valeurs d'exemple : banc d'essai du 7 novembre 2026 sur une GTX 1070 en 1280 × 720
    fn default() -> Self {
        Self {
            measured: false,
            gpu: "NVIDIA GeForce GTX 1070 (exemple)".into(),
            resolution: "1280 × 720".into(),
            timestamp: 1_794_075_252,
            integrated_gpu: false,
            reference_ms: 3.80,
            no_shadows: -0.23,
            no_cloud_shadows: 0.0,
            no_clouds: -0.11,
            no_god_rays: -0.13,
            no_bloom: -0.25,
            flip_fine_shadows: -0.11,
            flip_fine_god_rays: -0.13,
            flip_msaa: -0.09,
            lod_short: -0.22,
            lod_high: 0.14,
            precision_ultra: 0.57,
            precision_exact: 0.36,
        }
    }
}

/// Réglages qui comptent dans l'estimation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsChoice {
    pub shadows: bool,
    pub cloud_shadows: bool,
    pub clouds: bool,
    pub god_rays: bool,
    pub bloom: bool,
    pub fine_shadows: bool,
    pub fine_god_rays: bool,
    pub msaa: bool,
    pub lod_distance: LodDistanceSetting,
    pub lod_precision: LodPrecisionSetting,
}

/// Une ligne du détail de l'estimation
#[derive(Debug, Clone, PartialEq)]
pub struct CostLine {
    pub label: &'static str,
    pub ms: f32,
}

/// Résultat de l'estimation
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub frame_ms: f32,
    /// Écart de chaque réglage qui diffère de la référence
    pub lines: Vec<CostLine>,
}

impl Estimate {
    pub fn fps(&self) -> f32 {
        1000.0 / self.frame_ms.max(0.1)
    }
}

/// Un réglage coupé ne peut pas rendre l'image plus lente : un écart positif n'est que du bruit
fn saving(ms: f32) -> f32 {
    ms.min(0.0)
}

/// Un réglage ajouté ne peut pas rendre l'image plus rapide
fn extra(ms: f32) -> f32 {
    ms.max(0.0)
}

impl CostProfile {
    /// Coûts du dernier banc d'essai, ou valeurs d'exemple si le fichier est absent ou illisible
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("Coûts graphiques illisibles dans {} ({e}) : valeurs d'exemple.", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Réglages de la référence du banc d'essai sur cette carte
    pub fn reference(&self) -> GraphicsChoice {
        let fine = !self.integrated_gpu;
        GraphicsChoice {
            shadows: true,
            cloud_shadows: true,
            clouds: true,
            god_rays: true,
            bloom: true,
            fine_shadows: fine,
            fine_god_rays: fine,
            msaa: fine,
            lod_distance: LodDistanceSetting::Normale,
            lod_precision: LodPrecisionSetting::Normale,
        }
    }

    /// Durée d'image estimée avec ces réglages, et le détail de ce qui la change
    pub fn estimate(&self, c: &GraphicsChoice) -> Estimate {
        let r = self.reference();
        let mut lines = Vec::new();
        let mut add = |label, ms: f32| lines.push(CostLine { label, ms });
        if !c.shadows {
            // Les ombres des nuages et les rayons dépendent des ombres du relief
            add("Sans ombres du relief (ni des nuages, ni rayons)", saving(self.no_shadows));
        } else {
            if c.fine_shadows != r.fine_shadows {
                add(if c.fine_shadows { "Ombres fines" } else { "Ombres allégées" }, self.flip_fine_shadows);
            }
            if !c.cloud_shadows && c.clouds {
                add("Sans ombres des nuages", saving(self.no_cloud_shadows));
            }
            if !c.god_rays {
                add("Sans rayons crépusculaires", saving(self.no_god_rays));
            } else if c.fine_god_rays != r.fine_god_rays {
                add(if c.fine_god_rays { "Rayons fins" } else { "Rayons allégés" }, self.flip_fine_god_rays);
            }
        }
        if !c.clouds {
            add("Sans nuages", saving(self.no_clouds));
        }
        if !c.bloom {
            add("Sans bloom", saving(self.no_bloom));
        }
        if c.msaa != r.msaa {
            add(if c.msaa { "Avec MSAA 4×" } else { "Sans MSAA" }, self.flip_msaa);
        }
        match c.lod_distance {
            LodDistanceSetting::Courte => add("Distance de détail courte", saving(self.lod_short)),
            LodDistanceSetting::Normale => {}
            LodDistanceSetting::Elevee => add("Distance de détail élevée", extra(self.lod_high)),
        }
        match c.lod_precision {
            LodPrecisionSetting::Normale => {}
            LodPrecisionSetting::Ultra => add("Relief lointain ultra", extra(self.precision_ultra)),
            LodPrecisionSetting::Exacte => add("Relief lointain 1:1", extra(self.precision_exact)),
        }
        let frame_ms = (self.reference_ms + lines.iter().map(|l| l.ms).sum::<f32>()).max(0.1);
        Estimate { frame_ms, lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_estimates_reference() {
        let p = CostProfile::default();
        let e = p.estimate(&p.reference());
        assert!(e.lines.is_empty());
        assert!((e.frame_ms - p.reference_ms).abs() < 1e-6);
    }

    #[test]
    fn test_costs_add_up() {
        let p = CostProfile::default();
        let c = GraphicsChoice { bloom: false, lod_distance: LodDistanceSetting::Courte, ..p.reference() };
        let e = p.estimate(&c);
        assert!((e.frame_ms - (p.reference_ms + p.no_bloom + p.lod_short)).abs() < 1e-5);
    }

    #[test]
    fn test_shadows_off_hides_dependent_effects() {
        let p = CostProfile::default();
        let c = GraphicsChoice { shadows: false, god_rays: false, cloud_shadows: false, ..p.reference() };
        let e = p.estimate(&c);
        assert_eq!(e.lines.len(), 1, "les rayons et les ombres des nuages sont comptés avec les ombres du relief");
    }

    #[test]
    fn test_noise_never_inverts_a_cost() {
        let p = CostProfile { no_bloom: 0.2, lod_high: -0.3, ..CostProfile::default() };
        let r = p.reference();
        assert!(p.estimate(&GraphicsChoice { bloom: false, ..r }).frame_ms <= p.reference_ms);
        assert!(p.estimate(&GraphicsChoice { lod_distance: LodDistanceSetting::Elevee, ..r }).frame_ms >= p.reference_ms);
    }

    #[test]
    fn test_round_trip() {
        let dir = std::env::temp_dir().join(format!("herocraft_costs_{}", std::process::id()));
        let path = dir.join("couts.json");
        let p = CostProfile { measured: true, reference_ms: 5.0, ..CostProfile::default() };
        p.save(&path).unwrap();
        assert_eq!(CostProfile::load_or_default(&path), p);
        let _ = std::fs::remove_dir_all(dir);
    }
}

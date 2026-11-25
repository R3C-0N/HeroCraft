//! Statistiques du joueur côté client : vie, mana, endurance, classe et niveau affichés au HUD.
//! En solo, le jeu les fait évoluer lui-même ; en ligne, il applique les mêmes règles en attendant
//! le serveur, dont chaque `StatsUpdate` fait foi.

use std::path::{Path, PathBuf};
use std::time::Instant;

use herocraft_common::{CharacterStats, PlayerClass, fall_damage};

use crate::camera::OrbitCamera;

/// Après épuisement, le sprint revient quand l'endurance remonte à cette fraction du maximum
const SPRINT_RECOVERY: f32 = 0.25;
/// Temps passé sur l'écran de mort avant de pouvoir réapparaître, en secondes
pub(crate) const RESPAWN_DELAY: f32 = 2.0;

/// Mort du joueur, en attente de sa réapparition
#[derive(Debug, Clone)]
pub(crate) struct Death {
    pub at: Instant,
    /// Ce qui l'a tué, affiché sur l'écran de mort
    pub cause: String,
}

pub(crate) struct PlayerStats {
    pub stats: CharacterStats,
    /// Dernière blessure, pour le voile rouge du HUD
    pub hurt_at: Option<Instant>,
    /// Force de la dernière blessure, de 0,3 (égratignure) à 1 (un tiers de la vie ou plus)
    pub hurt_strength: f32,
    /// Le joueur est mort et attend de réapparaître
    pub death: Option<Death>,
    /// Fichier où la progression (niveau, expérience, points, compétences) du monde solo est
    /// gardée ; `None` en ligne, où le serveur la tient
    progress_path: Option<PathBuf>,
    /// Résurrections comptées au moment de sa mort : une de plus le relève sur place
    revived_seen: u32,
    /// Instant de la dernière réapparition : les jauges d'avant, encore en route, ne le retuent pas
    respawned_at: Option<Instant>,
    /// En solo, pendant une vision : les compétences du présent, mises de côté (le héros y vit en
    /// simple soldat) ; ce sont elles qu'on enregistre
    present_skills: Option<herocraft_common::vision::PresentSkills>,
}

/// Ce qui se garde d'une partie solo à l'autre
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedProgress {
    level: u32,
    experience: u64,
    #[serde(default)]
    skill_points: u32,
    #[serde(default)]
    skills: std::collections::BTreeMap<String, u8>,
    /// Jauge de faim au moment de quitter (pleine dans les sauvegardes d'avant la faim)
    #[serde(default)]
    satiety: Option<f32>,
}

impl Default for PlayerStats {
    /// Même personnage que celui que le serveur donne à chaque nouveau joueur
    fn default() -> Self {
        Self {
            stats: CharacterStats::new(PlayerClass::Adventurer),
            hurt_at: None,
            hurt_strength: 0.0,
            death: None,
            progress_path: None,
            revived_seen: 0,
            respawned_at: None,
            present_skills: None,
        }
    }
}

impl PlayerStats {
    /// Nouveau personnage dont la progression est reprise de `path` (s'il existe) et y sera
    /// enregistrée ; sans chemin (en ligne), un personnage neuf en attendant le serveur
    pub(crate) fn load_or_default(path: Option<PathBuf>) -> Self {
        let mut player = Self::default();
        if let Some(saved) = path.as_deref().and_then(read_progress) {
            let stats = &mut player.stats;
            // Les gains d'attributs se rejouent niveau par niveau, comme en jeu
            for _ in 1..saved.level.max(1) {
                stats.add_experience(stats.exp_for_next_level());
            }
            stats.experience = saved.experience;
            stats.skill_points = saved.skill_points;
            stats.skills = saved.skills;
            // Compétences d'avant le passage aux identifiants anglais : oubliées, points rendus
            herocraft_common::skills().forget_unknown(stats);
            stats.revive();
            if let Some(satiety) = saved.satiety.filter(|s| s.is_finite()) {
                stats.satiety = satiety.clamp(0.0, stats.max_satiety);
            }
        }
        player.progress_path = path;
        player
    }

    /// Enregistre la progression du monde solo (rien en ligne)
    pub(crate) fn save(&self) {
        let Some(path) = &self.progress_path else { return };
        let saved = SavedProgress {
            level: self.stats.level,
            experience: self.stats.experience,
            skill_points: self.present_skills.as_ref().map_or(self.stats.skill_points, |p| p.skill_points),
            skills: self.present_skills.as_ref().map_or_else(|| self.stats.skills.clone(), |p| p.skills.clone()),
            satiety: Some(self.stats.satiety),
        };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?)
        })();
        if let Err(e) = result {
            log::warn!("Impossible d'enregistrer la progression dans {} : {}", path.display(), e);
        }
    }

    /// En solo, le héros entre dans la vision en simple soldat : ses compétences sont mises de côté
    pub(crate) fn enter_vision(&mut self) {
        if self.present_skills.is_none() {
            self.present_skills = Some(herocraft_common::vision::PresentSkills::set_aside(&mut self.stats));
        }
    }

    /// En solo, il revient de la vision et retrouve ses compétences du présent
    pub(crate) fn leave_vision(&mut self) {
        if let Some(present) = self.present_skills.take() {
            present.restore(&mut self.stats);
        }
    }

    /// En solo, il revient d'une vision achevée : ses compétences du présent, plus celles que
    /// l'éclat a éveillées
    pub(crate) fn leave_vision_with_power(&mut self) {
        if let Some(present) = self.present_skills.take() {
            present.keep_awakened(&mut self.stats);
        }
    }

    /// Avance les jauges de `dt` secondes (le sprint consomme l'endurance) et coupe le sprint du
    /// joueur épuisé jusqu'à ce qu'il ait repris son souffle
    pub(crate) fn tick(&mut self, dt: f32, camera: &mut OrbitCamera) {
        if self.is_dead() {
            self.check_revived();
            return;
        }
        self.stats.regenerate(dt, camera.is_running());
        self.update_exhaustion(camera);
    }

    fn update_exhaustion(&self, camera: &mut OrbitCamera) {
        if self.stats.stamina <= 0.0 {
            camera.sprint_exhausted = true;
        } else if self.stats.stamina >= SPRINT_RECOVERY * self.stats.max_stamina {
            camera.sprint_exhausted = false;
        }
    }

    /// Mange un aliment qui rend `satiety` points : faux si le joueur est mort ou n'a plus faim
    /// (l'aliment reste alors dans l'inventaire)
    pub(crate) fn eat(&mut self, satiety: f32) -> bool {
        !self.is_dead() && self.stats.eat(satiety)
    }

    /// Dommages d'une chute de `blocks` blocs : renvoie la force de la blessure (0 si la chute
    /// est sans mal). S'il en meurt, le joueur reste mort jusqu'à `respawn`.
    pub(crate) fn fall(&mut self, blocks: f32) -> f32 {
        self.hurt(fall_damage(blocks), None, || format!("Chute de {:.0} blocs", blocks))
    }

    /// Dommages d'une créature (`cause` : son nom) : renvoie la force de la blessure
    /// (`kind` : le type du coup, que les boucliers typés distinguent)
    pub(crate) fn bitten(&mut self, damage: f32, kind: Option<herocraft_common::DamageKind>, cause: &str) -> f32 {
        self.hurt(damage, kind, || format!("Tué par : {cause}"))
    }

    /// Retire `damage` points de vie et renvoie la force de la blessure (0 sans dommage). S'il en
    /// meurt, le joueur reste mort jusqu'à `respawn`, avec la cause que donne `cause`.
    fn hurt(&mut self, damage: f32, kind: Option<herocraft_common::DamageKind>, cause: impl FnOnce() -> String) -> f32 {
        if damage <= 0.0 || self.is_dead() {
            return 0.0;
        }
        self.hurt_at = Some(Instant::now());
        self.hurt_strength = (3.0 * damage / self.stats.max_hp.max(1.0)).clamp(0.3, 1.0);
        if self.stats.take_hit(damage, kind) {
            self.hurt_strength = 1.0;
            self.death = Some(Death { at: Instant::now(), cause: cause() });
            self.stats.fallen = true;
            self.revived_seen = self.stats.revived;
        }
        self.hurt_strength
    }

    /// Tombé à zéro point de vie hors d'un coup (un saignement) : il reste mort jusqu'à `respawn`
    pub(crate) fn bled_out(&mut self) {
        if self.is_dead() {
            return;
        }
        self.stats.hp = 0.0;
        self.hurt_at = Some(Instant::now());
        self.hurt_strength = 1.0;
        self.death = Some(Death { at: Instant::now(), cause: "A succombé à ses blessures".to_string() });
        self.stats.fallen = true;
        self.revived_seen = self.stats.revived;
    }

    pub(crate) fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// Secondes restantes avant de pouvoir réapparaître (0 : c'est possible), `None` en vie
    pub(crate) fn respawn_in(&self) -> Option<f32> {
        self.death.as_ref().map(|d| (RESPAWN_DELAY - d.at.elapsed().as_secs_f32()).max(0.0))
    }

    /// Retour à la vie, jauges pleines (le joueur reste à replacer au point d'apparition)
    pub(crate) fn respawn(&mut self) {
        self.respawned_at = Some(Instant::now());
        self.death = None;
        self.hurt_at = None;
        self.stats.revive();
        self.stats.fallen = false;
        self.stats.stun_time = 0.0;
        self.stats.slow_time = 0.0;
        self.stats.tick_status(0.0);
    }

    /// Mort, mais relevé par une compétence (Intervention Suprême) : il revient à la vie sur place
    fn check_revived(&mut self) -> bool {
        if self.is_dead() && self.stats.revived > self.revived_seen {
            self.revived_seen = self.stats.revived;
            self.death = None;
            self.hurt_at = None;
            return true;
        }
        false
    }

    /// Statistiques reçues du serveur (un joueur mort le reste ici jusqu'à sa réapparition, même
    /// si le serveur l'a déjà ranimé)
    pub(crate) fn sync(&mut self, stats: CharacterStats, camera: &mut OrbitCamera) {
        self.stats = stats;
        // Le serveur le dit tombé sans qu'un coup l'ait annoncé : un saignement l'a vidé
        let fresh = self.respawned_at.is_some_and(|at| at.elapsed().as_secs_f32() < 2.0);
        if !self.is_dead() && self.stats.fallen && !fresh {
            self.bled_out();
        }
        if self.is_dead() && !self.check_revived() {
            self.stats.hp = 0.0;
        }
        self.update_exhaustion(camera);
    }
}

fn read_progress(path: &Path) -> Option<SavedProgress> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).map_err(|e| log::warn!("Progression illisible dans {} ({}), personnage neuf.", path.display(), e)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::SAFE_FALL_BLOCKS;

    #[test]
    fn test_solo_progress_is_saved_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("herocraft_progression_{}", std::process::id()));
        let path = dir.join("progression.json");
        let mut player = PlayerStats::load_or_default(Some(path.clone()));
        assert_eq!((player.stats.level, player.stats.skill_points), (1, 0));
        player.stats.add_experience(350);
        player.stats.satiety = 42.0;
        let wheel = herocraft_common::SkillWheel::standard();
        let id = wheel.nodes.iter().find(|n| n.requires.is_empty() && n.tier == 1).unwrap().id.clone();
        wheel.learn(&mut player.stats, &id).unwrap();
        player.save();

        let loaded = PlayerStats::load_or_default(Some(path));
        assert_eq!(loaded.stats.level, 3);
        assert_eq!(loaded.stats.experience, 50);
        assert_eq!(loaded.stats.skill_points, 1);
        assert_eq!(loaded.stats.skills.get(&id), Some(&1));
        assert_eq!(loaded.stats.max_hp, player.stats.max_hp, "les gains de niveau sont rejoués");
        assert_eq!(loaded.stats.hp, loaded.stats.max_hp);
        assert_eq!(loaded.stats.satiety, 42.0, "la faim est gardée d'une partie à l'autre");

        // En ligne, rien n'est écrit
        PlayerStats::load_or_default(None).save();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fall_hurts_then_kills_until_respawn() {
        let mut player = PlayerStats::default();
        assert_eq!(player.fall(SAFE_FALL_BLOCKS), 0.0);
        assert!(player.hurt_at.is_none());
        assert_eq!(player.stats.hp, player.stats.max_hp);

        let strength = player.fall(SAFE_FALL_BLOCKS + 2.0);
        assert!((0.3..1.0).contains(&strength), "petite chute : blessure de {strength}");
        assert!(player.fall(SAFE_FALL_BLOCKS + 8.0) > strength, "une plus grande chute fait plus mal");
        assert!(player.hurt_at.is_some());
        assert!(player.stats.hp < player.stats.max_hp);
        assert!(!player.is_dead());

        assert_eq!(player.fall(500.0), 1.0);
        assert!(player.is_dead());
        assert_eq!(player.stats.hp, 0.0);
        assert!(player.death.as_ref().unwrap().cause.contains("500"));
        assert!(player.respawn_in().unwrap() > 0.0, "l'écran de mort reste un moment");

        // Mort, il ne reprend pas de vie, ni des mains du serveur qui l'a déjà ranimé
        let mut camera = OrbitCamera::default();
        player.tick(10.0, &mut camera);
        assert_eq!(player.stats.hp, 0.0);
        player.sync(CharacterStats::new(PlayerClass::Adventurer), &mut camera);
        assert!(player.is_dead());
        assert_eq!(player.stats.hp, 0.0);

        player.respawn();
        assert!(!player.is_dead());
        assert_eq!(player.respawn_in(), None);
        assert_eq!(player.stats.hp, player.stats.max_hp);
    }

    #[test]
    fn test_a_fallen_player_is_raised_in_place_by_a_resurrection() {
        let mut player = PlayerStats::default();
        let mut camera = OrbitCamera::default();
        player.fall(500.0);
        assert!(player.is_dead() && player.stats.fallen);
        // Le serveur l'a ranimé mais le garde tombé : il reste sur son écran de mort
        let mut server = player.stats.clone();
        server.revive();
        player.sync(server.clone(), &mut camera);
        assert!(player.is_dead());
        // Puis un chapelain le relève
        assert!(server.resurrect(0.5));
        player.sync(server, &mut camera);
        assert!(!player.is_dead(), "relevé, il reste mort");
        assert_eq!(player.stats.hp, 0.5 * player.stats.max_hp);
    }

    #[test]
    fn test_exhausted_player_cannot_sprint_until_rested() {
        let mut player = PlayerStats::default();
        let mut camera = OrbitCamera::default();
        camera.effective_distance = 0.0;
        camera.is_sprinting = true;
        camera.move_forward = true;
        assert!(camera.is_running());

        player.tick(60.0, &mut camera);
        assert_eq!(player.stats.stamina, 0.0);
        assert!(camera.sprint_exhausted);
        assert!(!camera.is_running());

        // Un peu de repos ne suffit pas, il faut reprendre son souffle
        player.tick(0.5, &mut camera);
        assert!(camera.sprint_exhausted);
        player.tick(5.0, &mut camera);
        assert!(!camera.sprint_exhausted);
        assert!(camera.is_running());
    }
}

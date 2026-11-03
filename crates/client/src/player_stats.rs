//! Statistiques du joueur côté client : vie, mana, endurance, classe et niveau affichés au HUD.
//! En solo, le jeu les fait évoluer lui-même ; en ligne, il applique les mêmes règles en attendant
//! le serveur, dont chaque `StatsUpdate` fait foi.

use std::time::Instant;

use herocraft_common::{fall_damage, CharacterStats, PlayerClass};

use crate::camera::OrbitCamera;

/// Après épuisement, le sprint revient quand l'endurance remonte à cette fraction du maximum
const SPRINT_RECOVERY: f32 = 0.25;
/// Temps passé sur l'écran de mort avant de pouvoir réapparaître, en secondes
pub const RESPAWN_DELAY: f32 = 2.0;

/// Mort du joueur, en attente de sa réapparition
#[derive(Debug, Clone)]
pub struct Death {
    pub at: Instant,
    /// Ce qui l'a tué, affiché sur l'écran de mort
    pub cause: String,
}

pub struct PlayerStats {
    pub stats: CharacterStats,
    /// Dernière blessure, pour le voile rouge du HUD
    pub hurt_at: Option<Instant>,
    /// Force de la dernière blessure, de 0,3 (égratignure) à 1 (un tiers de la vie ou plus)
    pub hurt_strength: f32,
    /// Le joueur est mort et attend de réapparaître
    pub death: Option<Death>,
}

impl Default for PlayerStats {
    /// Même personnage que celui que le serveur donne à chaque nouveau joueur
    fn default() -> Self {
        Self { stats: CharacterStats::new(PlayerClass::Adventurer), hurt_at: None, hurt_strength: 0.0, death: None }
    }
}

impl PlayerStats {
    /// Avance les jauges de `dt` secondes (le sprint consomme l'endurance) et coupe le sprint du
    /// joueur épuisé jusqu'à ce qu'il ait repris son souffle
    pub fn tick(&mut self, dt: f32, camera: &mut OrbitCamera) {
        if self.is_dead() {
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

    /// Dommages d'une chute de `blocks` blocs : renvoie la force de la blessure (0 si la chute
    /// est sans mal). S'il en meurt, le joueur reste mort jusqu'à `respawn`.
    pub fn fall(&mut self, blocks: f32) -> f32 {
        self.hurt(fall_damage(blocks), || format!("Chute de {:.0} blocs", blocks))
    }

    /// Dommages d'une créature (`cause` : son nom) : renvoie la force de la blessure
    pub fn bitten(&mut self, damage: f32, cause: &str) -> f32 {
        self.hurt(damage, || format!("Tué par : {cause}"))
    }

    /// Retire `damage` points de vie et renvoie la force de la blessure (0 sans dommage). S'il en
    /// meurt, le joueur reste mort jusqu'à `respawn`, avec la cause que donne `cause`.
    fn hurt(&mut self, damage: f32, cause: impl FnOnce() -> String) -> f32 {
        if damage <= 0.0 || self.is_dead() {
            return 0.0;
        }
        self.hurt_at = Some(Instant::now());
        self.hurt_strength = (3.0 * damage / self.stats.max_hp.max(1.0)).clamp(0.3, 1.0);
        if self.stats.take_damage(damage) {
            self.hurt_strength = 1.0;
            self.death = Some(Death { at: Instant::now(), cause: cause() });
        }
        self.hurt_strength
    }

    pub fn is_dead(&self) -> bool {
        self.death.is_some()
    }

    /// Secondes restantes avant de pouvoir réapparaître (0 : c'est possible), `None` en vie
    pub fn respawn_in(&self) -> Option<f32> {
        self.death.as_ref().map(|d| (RESPAWN_DELAY - d.at.elapsed().as_secs_f32()).max(0.0))
    }

    /// Retour à la vie, jauges pleines (le joueur reste à replacer au point d'apparition)
    pub fn respawn(&mut self) {
        self.death = None;
        self.hurt_at = None;
        self.stats.revive();
    }

    /// Statistiques reçues du serveur (un joueur mort le reste ici jusqu'à sa réapparition, même
    /// si le serveur l'a déjà ranimé)
    pub fn sync(&mut self, stats: CharacterStats, camera: &mut OrbitCamera) {
        self.stats = stats;
        if self.is_dead() {
            self.stats.hp = 0.0;
        }
        self.update_exhaustion(camera);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::SAFE_FALL_BLOCKS;

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

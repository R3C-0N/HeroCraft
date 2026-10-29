//! Statistiques du joueur côté client : vie, mana, endurance, classe et niveau affichés au HUD.
//! En solo, le jeu les fait évoluer lui-même ; en ligne, il applique les mêmes règles en attendant
//! le serveur, dont chaque `StatsUpdate` fait foi.

use std::time::Instant;

use herocraft_common::{fall_damage, CharacterStats, PlayerClass};

use crate::camera::OrbitCamera;

/// Après épuisement, le sprint revient quand l'endurance remonte à cette fraction du maximum
const SPRINT_RECOVERY: f32 = 0.25;

pub struct PlayerStats {
    pub stats: CharacterStats,
    /// Dernière blessure, pour le voile rouge du HUD
    pub hurt_at: Option<Instant>,
}

impl Default for PlayerStats {
    /// Même personnage que celui que le serveur donne à chaque nouveau joueur
    fn default() -> Self {
        Self { stats: CharacterStats::new(PlayerClass::Warrior), hurt_at: None }
    }
}

impl PlayerStats {
    /// Avance les jauges de `dt` secondes (le sprint consomme l'endurance) et coupe le sprint du
    /// joueur épuisé jusqu'à ce qu'il ait repris son souffle
    pub fn tick(&mut self, dt: f32, camera: &mut OrbitCamera) {
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

    /// Dommages d'une chute de `blocks` blocs. Renvoie `true` si le joueur en meurt : il est
    /// aussitôt ranimé, jauges pleines, et reste à replacer au point d'apparition.
    pub fn fall(&mut self, blocks: f32) -> bool {
        let damage = fall_damage(blocks);
        if damage <= 0.0 {
            return false;
        }
        self.hurt_at = Some(Instant::now());
        let died = self.stats.take_damage(damage);
        if died {
            self.stats.revive();
        }
        died
    }

    /// Statistiques reçues du serveur
    pub fn sync(&mut self, stats: CharacterStats, camera: &mut OrbitCamera) {
        self.stats = stats;
        self.update_exhaustion(camera);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::SAFE_FALL_BLOCKS;

    #[test]
    fn test_fall_hurts_then_kills_and_revives() {
        let mut player = PlayerStats::default();
        assert!(!player.fall(SAFE_FALL_BLOCKS));
        assert!(player.hurt_at.is_none());
        assert_eq!(player.stats.hp, player.stats.max_hp);

        assert!(!player.fall(SAFE_FALL_BLOCKS + 5.0));
        assert!(player.hurt_at.is_some());
        assert!(player.stats.hp < player.stats.max_hp);

        assert!(player.fall(500.0));
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

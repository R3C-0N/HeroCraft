//! Sons du joueur déduits de son mouvement d'une image à l'autre : pas au rythme de la foulée,
//! saut au décollage, réception à l'atterrissage (plus forte après une longue chute).

use glam::Vec3;

use super::synth::Material;

/// Foulée en blocs entre deux pas, en marche et en sprint
const WALK_STRIDE: f32 = 2.4;
const SPRINT_STRIDE: f32 = 3.6;
/// Au-delà, le déplacement d'une image est une téléportation (apparition, autre planète) : pas de pas
const TELEPORT_BLOCKS: f32 = 3.0;
/// Vitesse de chute (blocs/s) sous laquelle une réception reste silencieuse
const SILENT_LANDING_SPEED: f32 = 4.0;
/// Vitesse de chute (blocs/s) de la réception la plus forte
const HARD_LANDING_SPEED: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerSound {
    Step(Material),
    Jump(Material),
    /// Réception, avec sa force dans ]0, 1]
    Land(Material, f32),
}

/// État du joueur lu après la mise à jour physique d'une image
#[derive(Debug, Clone, Copy)]
pub struct PlayerState {
    pub position: Vec3,
    pub grounded: bool,
    pub flying: bool,
    pub sprinting: bool,
    /// Vitesse verticale en blocs par seconde (positive vers le haut)
    pub vertical_speed: f32,
    /// Hauteur d'un bloc en unités du monde
    pub block_height: f32,
}

#[derive(Default)]
pub struct PlayerSoundTracker {
    last: Option<PlayerState>,
    /// Distance parcourue au sol depuis le dernier pas, en blocs
    stride: f32,
}

impl PlayerSoundTracker {
    /// Oublie l'état précédent (vue satellite, menu, nouvelle planète) : pas de faux saut ni de fausse
    /// réception au retour
    pub fn reset(&mut self) {
        self.last = None;
    }

    pub fn update(&mut self, now: PlayerState, surface: Material) -> Option<PlayerSound> {
        let Some(last) = self.last.replace(now) else {
            self.stride = 0.0;
            return None;
        };

        let up = now.position.normalize_or_zero();
        let delta = now.position - last.position;
        let horizontal = (delta - up * delta.dot(up)).length() / now.block_height.max(1e-6);
        if horizontal > TELEPORT_BLOCKS {
            self.stride = 0.0;
            return None;
        }

        // Décollage : au sol l'image d'avant, en l'air et montant maintenant, hors vol
        if last.grounded && !now.grounded && !now.flying && now.vertical_speed > 0.0 {
            self.stride = 0.0;
            return Some(PlayerSound::Jump(surface));
        }

        // Réception : la vitesse de chute de l'image d'avant fait la force du choc
        if !last.grounded && now.grounded {
            // Le prochain pas vient vite après une réception : la foulée reprend à mi-chemin
            self.stride = WALK_STRIDE * 0.5;
            let fall = -last.vertical_speed;
            if fall > SILENT_LANDING_SPEED {
                let strength = ((fall - SILENT_LANDING_SPEED) / (HARD_LANDING_SPEED - SILENT_LANDING_SPEED)).clamp(0.15, 1.0);
                return Some(PlayerSound::Land(surface, strength));
            }
            return None;
        }

        if !now.grounded || now.flying {
            return None;
        }
        if horizontal < 1e-4 {
            // À l'arrêt, le premier pas du prochain départ vient vite
            self.stride = self.stride.max(WALK_STRIDE * 0.6);
            return None;
        }
        self.stride += horizontal;
        let stride = if now.sprinting { SPRINT_STRIDE } else { WALK_STRIDE };
        if self.stride >= stride {
            self.stride = 0.0;
            return Some(PlayerSound::Step(surface));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f32 = 0.1;

    fn state(x_blocks: f32, grounded: bool, vertical_speed: f32) -> PlayerState {
        PlayerState {
            position: Vec3::new(x_blocks * H, 50.0, 0.0),
            grounded,
            flying: false,
            sprinting: false,
            vertical_speed,
            block_height: H,
        }
    }

    #[test]
    fn test_steps_follow_the_stride() {
        let mut tracker = PlayerSoundTracker::default();
        let mut steps = 0;
        // 20 blocs à 0.1 bloc par image
        for i in 0..=200 {
            if let Some(PlayerSound::Step(_)) = tracker.update(state(i as f32 * 0.1, true, 0.0), Material::Grass) {
                steps += 1;
            }
        }
        assert!((8..=9).contains(&steps), "{steps} pas sur 20 blocs");
    }

    #[test]
    fn test_standing_still_makes_no_step() {
        let mut tracker = PlayerSoundTracker::default();
        for _ in 0..100 {
            assert_eq!(tracker.update(state(0.0, true, 0.0), Material::Stone), None);
        }
    }

    #[test]
    fn test_jump_then_landing() {
        let mut tracker = PlayerSoundTracker::default();
        tracker.update(state(0.0, true, 0.0), Material::Dirt);
        assert_eq!(tracker.update(state(0.0, false, 12.0), Material::Dirt), Some(PlayerSound::Jump(Material::Dirt)));
        assert_eq!(tracker.update(state(0.0, false, -20.0), Material::Dirt), None);
        match tracker.update(state(0.0, true, 0.0), Material::Stone) {
            Some(PlayerSound::Land(Material::Stone, s)) => assert!(s > 0.5 && s <= 1.0),
            other => panic!("réception attendue, obtenu {other:?}"),
        }
    }

    #[test]
    fn test_small_drop_is_silent_and_flight_is_quiet() {
        let mut tracker = PlayerSoundTracker::default();
        tracker.update(state(0.0, false, -2.0), Material::Grass);
        assert_eq!(tracker.update(state(0.0, true, 0.0), Material::Grass), None, "descendre une marche ne fait pas de choc");

        let mut flying = state(0.0, false, 0.0);
        flying.flying = true;
        tracker.update(flying, Material::Grass);
        for i in 1..100 {
            flying.position.x = i as f32 * H;
            assert_eq!(tracker.update(flying, Material::Grass), None, "pas de pas en vol");
        }
    }

    #[test]
    fn test_teleport_is_silent() {
        let mut tracker = PlayerSoundTracker::default();
        tracker.update(state(0.0, true, 0.0), Material::Grass);
        assert_eq!(tracker.update(state(500.0, true, 0.0), Material::Grass), None);
    }
}

//! Horloge du ciel : course du Soleil selon le temps du ciel (secondes de jeu), partagée par le
//! client (rendu, heure locale) et le serveur (sommeil des créatures, météo).

use glam::Vec3;

/// Calcule la direction unitaire vers le Soleil en temps réel
/// - Cycle diurne complet (jour / nuit) : 20 minutes (1200 secondes)
/// - Inclinaison de l'écliptique : ~23.4° (0.409 rad)
pub fn compute_sun_dir(time: f32) -> Vec3 {
    let (u_sun, v_sun) = sun_plane();
    let sun_angle = (time / SUN_PERIOD) * std::f32::consts::TAU + SUN_PHASE;
    (u_sun * sun_angle.cos() + v_sun * sun_angle.sin()).normalize()
}

/// Durée d'un jour (20 minutes à vitesse céleste 1) et angle du Soleil au temps 0
pub const SUN_PERIOD: f32 = 1200.0;
const SUN_PHASE: f32 = 0.40;

/// Le Soleil tourne dans le plan de ces deux axes
fn sun_plane() -> (Vec3, Vec3) {
    let tilt: f32 = 0.409;
    (Vec3::new(tilt.cos(), tilt.sin(), 0.0), Vec3::new(0.0, 0.0, 1.0))
}

/// Temps du ciel où le Soleil passe au plus haut au-dessus de `up` (midi local), entre 0 et
/// `SUN_PERIOD` ; `None` si le Soleil ne s'y lève ni ne s'y couche (axe de sa course)
fn local_noon(up: Vec3) -> Option<f32> {
    let (u, v) = sun_plane();
    let (a, b) = (up.dot(u), up.dot(v));
    if a.hypot(b) < 1e-3 {
        return None;
    }
    let angle = b.atan2(a) - SUN_PHASE;
    Some((angle / std::f32::consts::TAU * SUN_PERIOD).rem_euclid(SUN_PERIOD))
}

/// Heure locale (0 à 24, midi quand le Soleil est au plus haut) au point de verticale `up`
pub fn local_hour(time: f32, up: Vec3) -> Option<f32> {
    let noon = local_noon(up)?;
    Some((12.0 + (time - noon) / SUN_PERIOD * 24.0).rem_euclid(24.0))
}

/// Prochain temps du ciel, à partir de `time`, où il est `hour` heures au point de verticale `up`
pub fn time_for_local_hour(time: f32, up: Vec3, hour: f32) -> Option<f32> {
    let noon = local_noon(up)?;
    let target = noon + (hour - 12.0) / 24.0 * SUN_PERIOD;
    Some(time + (target - time).rem_euclid(SUN_PERIOD))
}


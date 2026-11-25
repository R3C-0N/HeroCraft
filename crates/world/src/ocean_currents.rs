use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Résultat d'échantillonnage d'un courant marin
#[derive(Debug, Clone, Copy)]
pub struct OceanCurrentSample {
    pub velocity: DVec3,
    pub speed: f64,
    /// Anomalie de température : > 0 pour courant chaud, < 0 pour courant froid
    pub temp_anomaly: f64,
    /// Couleur de visualisation pour le filtre
    pub color: [f32; 4],
}

/// Simulateur des courants marins globaux et tourbillons océaniques (Gyres)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OceanicCurrentSimulation {
    pub seed: u64,
}

impl OceanicCurrentSimulation {
    pub fn new(seed: u64) -> Self {
        herocraft_common::prof_scope!("world::OceanicCurrentSimulation::new");
        Self { seed }
    }

    /// Évalue le vecteur et la température du courant marin en un point de la sphère
    pub fn sample(&self, dir: DVec3, is_water: bool, continental_value: f64) -> OceanCurrentSample {
        herocraft_common::prof_scope!("world::ocean_currents::sample");
        if !is_water {
            // Sur la terre ferme : teinte neutre sépia sombre pour mettre en valeur les océans
            return OceanCurrentSample {
                velocity: DVec3::ZERO,
                speed: 0.0,
                temp_anomaly: 0.0,
                color: [0.22, 0.22, 0.24, 1.0],
            };
        }

        let lat = dir.y.clamp(-1.0, 1.0).asin(); // [-pi/2, pi/2]
        let lon = dir.z.atan2(dir.x);          // [-pi, pi]

        // Vecteurs unitaires locaux de la sphère :
        // Est (longitude croissante) et Nord (latitude croissante)
        let east = DVec3::new(-dir.z, 0.0, dir.x).normalize_or_zero();
        let north = DVec3::new(-dir.y * dir.x, 1.0 - dir.y * dir.y, -dir.y * dir.z).normalize_or_zero();

        let deg_lat = lat.to_degrees();

        // 1. Vitesse zonale moyenne (alizés et vents d'ouest)
        // Hadley (< 30°) : vers l'Ouest (-east)
        // Ferrel (30° - 60°) : vers l'Est (+east)
        // Polaire (> 60°) : vers l'Ouest (-east)
        let zonal_speed = if deg_lat.abs() < 28.0 {
            -(1.0 - (deg_lat.abs() / 28.0).powi(2))
        } else if deg_lat.abs() < 62.0 {
            let mid_ratio = (deg_lat.abs() - 45.0) / 17.0;
            0.9 * (1.0 - mid_ratio.powi(2)).max(0.0)
        } else {
            -0.5 * ((deg_lat.abs() - 62.0) / 28.0).min(1.0)
        };

        // 2. Circulation méridonienne (Déviation de Coriolis & Gyres circulaires)
        // Dans les tourbillons subtropicaux (~25°-35°), l'eau remonte vers le pôle côté Ouest de l'océan
        // et redescend vers l'équateur côté Est
        let gyre_wave = (lon * 3.0 + (self.seed as f64 * 0.01)).sin();
        let is_northern = deg_lat > 0.0;

        let meridional_speed = if deg_lat.abs() > 10.0 && deg_lat.abs() < 55.0 {
            let gyre_strength = ((deg_lat.abs() - 10.0) / 45.0).sin();
            if is_northern {
                gyre_wave * gyre_strength * 0.8
            } else {
                -gyre_wave * gyre_strength * 0.8
            }
        } else {
            0.0
        };

        let velocity = (east * zonal_speed + north * meridional_speed).normalize_or_zero();
        let speed = (zonal_speed.hypot(meridional_speed)).min(1.2);

        // 3. Calcul de la température du courant :
        // Un courant qui se dirige vers les pôles (meridional > 0 au Nord, < 0 au Sud) transporte de l'eau CHAUDE
        // Un courant qui se dirige vers l'équateur transporte de l'eau FROIDE
        let poleward_component = if is_northern {
            velocity.dot(north)
        } else {
            -velocity.dot(north)
        };

        // Influence de la latitude de base : équateur = chaud (+0.4), pôles = froid (-0.5)
        let lat_base_temp = (deg_lat.abs() / 90.0).clamp(0.0, 1.0);
        let base_temp = 1.0 - 2.0 * lat_base_temp;

        let temp_anomaly = (base_temp * 0.5 + poleward_component * 0.6).clamp(-1.0, 1.0);

        // 4. Couleur du filtre thermique :
        // Chaud (> 0.25) : Dégradé du jaune d'or au rouge vif
        // Neutre (-0.25 à 0.25) : Cyan turquoise lumineux
        // Froid (< -0.25) : Bleu outremer profond à indigo arctique
        let color = if temp_anomaly > 0.15 {
            let t = ((temp_anomaly - 0.15) / 0.85).clamp(0.0, 1.0) as f32;
            // Transition Jaune d'or -> Rouge lave intense (Gulf Stream)
            [1.0, 0.85 * (1.0 - t * 0.6), 0.15 * (1.0 - t), 1.0]
        } else if temp_anomaly < -0.15 {
            let t = ((-temp_anomaly - 0.15) / 0.85).clamp(0.0, 1.0) as f32;
            // Transition Cyan -> Bleu abyssal arctique (Courant du Labrador / Humboldt)
            [0.10 * (1.0 - t), 0.55 * (1.0 - t * 0.5), 0.95, 1.0]
        } else {
            // Courant tempéré / neutre (Turquoise azur)
            [0.20, 0.75, 0.80, 1.0]
        };

        // Si très proche de la côte, on assombrit légèrement le tracé pour démarquer le rivage
        let shore_factor = if continental_value > -0.05 { 0.85 } else { 1.0 };
        let final_color = [
            color[0] * shore_factor as f32,
            color[1] * shore_factor as f32,
            color[2] * shore_factor as f32,
            1.0,
        ];

        OceanCurrentSample {
            velocity,
            speed,
            temp_anomaly,
            color: final_color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocean_currents_sampling() {
        let sim = OceanicCurrentSimulation::new(42);

        // Sur terre
        let sample_land = sim.sample(DVec3::new(1.0, 0.0, 0.0), false, 0.5);
        assert_eq!(sample_land.speed, 0.0);

        // Dans l'océan
        let sample_ocean = sim.sample(DVec3::new(1.0, 0.0, 0.0), true, -0.5);
        assert!(sample_ocean.speed > 0.0);
        assert!(sample_ocean.color[3] > 0.0);
    }
}

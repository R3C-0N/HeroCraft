use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Type de frontière tectonique
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryType {
    None,
    /// Collision de plaques (surrection de montagnes et fosses)
    Convergence,
    /// Écartement de plaques (rifts et dorsales médio-océaniques)
    Divergence,
    /// Coulissage latéral (failles transformantes)
    Transform,
}

/// Plaque tectonique sphérique
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TectonicPlate {
    pub id: usize,
    /// Centre de gravité initial de la plaque sur la sphère unitaire
    pub center: DVec3,
    /// Vraie plaque continentale (granitique) ou océanique (basaltique)
    pub is_oceanic: bool,
    /// Axe de rotation de la dérive de la plaque (Euler vector)
    pub rotation_axis: DVec3,
    /// Vitesse angulaire de dérive (radians/sec ou valeur relative)
    pub drift_speed: f64,
    /// Altitude de base de la croûte
    pub base_elevation: f64,
    /// Couleur de visualisation pour le filtre
    pub color: [f32; 4],
}

/// Résultat d'échantillonnage tectonique en un point précis de la planète
#[derive(Debug, Clone, Copy)]
pub struct TectonicSample {
    pub plate_id: usize,
    pub is_oceanic: bool,
    pub dist_to_boundary: f64,
    pub boundary_type: BoundaryType,
    pub relative_motion: f64,
    pub elevation_offset: f64,
    pub debug_color: [f32; 4],
}

/// Simulateur géologique des plaques tectoniques
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TectonicSimulation {
    pub seed: u64,
    pub plates: Vec<TectonicPlate>,
}

impl TectonicSimulation {
    /// Génère un système de plaques tectoniques réaliste (8 à 12 plaques)
    pub fn new(seed: u64, plate_count: usize) -> Self {
        let count = plate_count.clamp(6, 16);
        let mut plates = Vec::with_capacity(count);

        // Palette de teintes distinctes pour le mode de visualisation
        let palette: [[f32; 4]; 12] = [
            [0.85, 0.35, 0.35, 1.0], // Rouge corail
            [0.35, 0.75, 0.45, 1.0], // Vert émeraude
            [0.35, 0.55, 0.85, 1.0], // Bleu azur
            [0.85, 0.75, 0.35, 1.0], // Jaune ocre
            [0.75, 0.45, 0.85, 1.0], // Violet améthyste
            [0.35, 0.85, 0.85, 1.0], // Cyan turquoise
            [0.90, 0.55, 0.30, 1.0], // Orange terre de sienne
            [0.45, 0.80, 0.35, 1.0], // Vert mousse
            [0.60, 0.65, 0.75, 1.0], // Gris ardoise
            [0.85, 0.40, 0.65, 1.0], // Rose quartz
            [0.55, 0.40, 0.30, 1.0], // Brun bistre
            [0.30, 0.40, 0.60, 1.0], // Bleu nuit
        ];

        // Répartition sphérique quasi-uniforme (Fibonacci sphere)
        let phi = (1.0 + 5.0_f64.sqrt()) * 0.5; // Nombre d'or
        for i in 0..count {
            let y = 1.0 - (i as f64) / ((count - 1).max(1) as f64) * 2.0; // [-1.0, 1.0]
            let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;

            // Ajout d'une légère gigue pseudo-aléatoire selon la graine
            let jitter_seed = seed.wrapping_add((i as u64) * 987654321);
            let jx = ((jitter_seed.wrapping_mul(1103515245) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;
            let jy = ((jitter_seed.wrapping_mul(1664525) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;
            let jz = ((jitter_seed.wrapping_mul(214013) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;

            let raw_pos = DVec3::new(
                radius_at_y * theta.cos() + jx * 0.25,
                y + jy * 0.25,
                radius_at_y * theta.sin() + jz * 0.25,
            );
            let center = raw_pos.normalize();

            // ~55% plaques océaniques, ~45% continentales
            let is_oceanic = (jitter_seed % 100) < 55;

            // Axe de dérive (orthogonal au centre de la plaque)
            let raw_axis = DVec3::new(
                ((jitter_seed.wrapping_add(1).wrapping_mul(2246822519)) & 0xFFF) as f64 / 2048.0 - 1.0,
                ((jitter_seed.wrapping_add(2).wrapping_mul(3266489917)) & 0xFFF) as f64 / 2048.0 - 1.0,
                ((jitter_seed.wrapping_add(3).wrapping_mul(668265263)) & 0xFFF) as f64 / 2048.0 - 1.0,
            );
            let rotation_axis = raw_axis.normalize_or_zero();
            let drift_speed = 0.5 + (((jitter_seed >> 8) & 0xFF) as f64 / 255.0) * 0.5;

            let base_elevation = if is_oceanic { 5.0 } else { 15.0 };
            let color = palette[i % palette.len()];

            plates.push(TectonicPlate {
                id: i,
                center,
                is_oceanic,
                rotation_axis,
                drift_speed,
                base_elevation,
                color,
            });
        }

        Self { seed, plates }
    }

    /// Échantillonne l'état tectonique pour une direction donnée sur la planète
    pub fn sample(&self, dir: DVec3) -> TectonicSample {
        // Distorsion fractale pour donner aux frontières de plaques des contours organiques
        let warp_x = (dir.x * 3.5 + (self.seed as f64 * 0.001)).sin() * 0.08;
        let warp_y = (dir.y * 3.5 + 1.7).cos() * 0.08;
        let warp_z = (dir.z * 3.5 + 3.1).sin() * 0.08;
        let warped_dir = (dir + DVec3::new(warp_x, warp_y, warp_z)).normalize();

        // Trouver les 2 plaques les plus proches
        let mut best_id = 0;
        let mut second_id = 1;
        let mut best_dot = -2.0;
        let mut second_dot = -2.0;

        for plate in &self.plates {
            let dot = warped_dir.dot(plate.center);
            if dot > best_dot {
                second_dot = best_dot;
                second_id = best_id;
                best_dot = dot;
                best_id = plate.id;
            } else if dot > second_dot {
                second_dot = dot;
                second_id = plate.id;
            }
        }

        let p1 = &self.plates[best_id];
        let p2 = &self.plates[second_id];

        // Distance relative à la frontière (0 = sur la faille, 1 = au cœur de la plaque)
        // best_dot - second_dot est nul sur la frontière équidistante
        let margin = (best_dot - second_dot).max(0.0);
        let dist_to_boundary = (margin * 4.0).min(1.0);

        // Vitesse relative des deux plaques au point de contact
        // v = omega x dir
        let v1 = p1.rotation_axis.cross(warped_dir) * p1.drift_speed;
        let v2 = p2.rotation_axis.cross(warped_dir) * p2.drift_speed;
        let v_rel = v1 - v2;

        // Normale de la frontière : orientée de p1 vers p2
        let boundary_normal = (p2.center - p1.center).normalize_or_zero();
        let relative_motion = v_rel.dot(boundary_normal);

        let (boundary_type, elevation_offset) = if dist_to_boundary < 0.45 {
            let boundary_factor = (1.0 - (dist_to_boundary / 0.45)).max(0.0);

            if relative_motion > 0.08 {
                // Convergence : collision de plaques (Orogenèse / Chaîne de montagnes ou Fosse)
                let peak_height = if !p1.is_oceanic && !p2.is_oceanic {
                    // Collision continentale : chaînes de montagnes colossales (Himalaya)
                    18.0 * boundary_factor * (relative_motion * 3.0).min(1.5)
                } else if p1.is_oceanic && !p2.is_oceanic {
                    // Subduction océan sous continent : fosse marine profonde
                    -6.0 * boundary_factor
                } else if !p1.is_oceanic && p2.is_oceanic {
                    // Côté continental de la subduction : cordillère volcanique (Andes)
                    14.0 * boundary_factor
                } else {
                    // Collision océan/océan : fosse et arcs insulaires
                    -4.0 * boundary_factor
                };
                (BoundaryType::Convergence, peak_height)
            } else if relative_motion < -0.08 {
                // Divergence : écartement des plaques (Dorsale médio-océanique ou Rift)
                let rift = if p1.is_oceanic && p2.is_oceanic {
                    // Dorsale océanique (crête avec rift central)
                    let ridge = 3.5 * boundary_factor;
                    let central_groove = if dist_to_boundary < 0.08 { -2.0 } else { 0.0 };
                    ridge + central_groove
                } else {
                    // Rift continental : vallée effondrée
                    -4.5 * boundary_factor
                };
                (BoundaryType::Divergence, rift)
            } else {
                (BoundaryType::Transform, 0.0)
            }
        } else {
            (BoundaryType::None, 0.0)
        };

        // Couleur de debug pour la visualisation
        let mut debug_color = p1.color;
        if dist_to_boundary < 0.15 {
            // Surlignage vif des lignes de failles
            match boundary_type {
                BoundaryType::Convergence => {
                    // Rouge vif / lave pour les collisions montagneuses
                    debug_color = [1.0, 0.15, 0.15, 1.0];
                }
                BoundaryType::Divergence => {
                    // Bleu néon pour les dorsales d'écartement
                    debug_color = [0.15, 0.85, 1.0, 1.0];
                }
                BoundaryType::Transform => {
                    // Jaune d'or pour les failles coulissantes
                    debug_color = [1.0, 0.85, 0.15, 1.0];
                }
                BoundaryType::None => {
                    // Blanc neutre pour les frontières passives
                    debug_color = [0.9, 0.9, 0.9, 1.0];
                }
            }
        }

        TectonicSample {
            plate_id: best_id,
            is_oceanic: p1.is_oceanic,
            dist_to_boundary,
            boundary_type,
            relative_motion,
            elevation_offset,
            debug_color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tectonics_creation_and_sampling() {
        let sim = TectonicSimulation::new(42, 10);
        assert_eq!(sim.plates.len(), 10);

        let p = DVec3::new(1.0, 0.0, 0.0);
        let sample = sim.sample(p);
        assert!(sample.plate_id < 10);
        assert!(sample.dist_to_boundary >= 0.0 && sample.dist_to_boundary <= 1.0);
        assert!(sample.debug_color[3] > 0.0);
    }
}

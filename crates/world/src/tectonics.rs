use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Catégorie de taille de la plaque tectonique
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlateCategory {
    /// 4 Grandes plaques majeures (couvrent l'essentiel du globe et abritent les grands continents)
    Major,
    /// 3 Plaques moyennes (continents secondaires et bassins)
    Medium,
    /// 8 Petites plaques (plaques dynamiques insérées aux jonctions complexes)
    Minor,
}

/// Type de frontière tectonique
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryType {
    None,
    /// Collision de plaques (orogenèse, super-chaînes et fosses)
    Convergence,
    /// Écartement de plaques (rifts et dorsales médio-océaniques)
    Divergence,
    /// Coulissage latéral (failles transformantes)
    Transform,
}

/// Plaque tectonique sphérique hiérarchisée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TectonicPlate {
    pub id: usize,
    pub category: PlateCategory,
    /// Centre de gravité de la plaque sur la sphère unitaire
    pub center: DVec3,
    /// Poids d'attraction dans le Voronoi sphérique pondéré (1.45 majeur, 1.05 moyen, 0.65 mineur)
    pub weight: f64,
    /// Vraie plaque continentale (granitique) ou océanique (basaltique)
    pub is_oceanic: bool,
    /// Axe de rotation de la dérive de la plaque (Euler vector)
    pub rotation_axis: DVec3,
    /// Vitesse angulaire de dérive
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
    /// Intensité orogénique (0.0 à 1.5+) pour la formation de super-chaînes de montagnes
    pub orogeny_strength: f64,
    pub debug_color: [f32; 4],
}

/// Simulateur géologique des plaques tectoniques
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TectonicSimulation {
    pub seed: u64,
    pub plates: Vec<TectonicPlate>,
}

impl TectonicSimulation {
    /// Génère un système hiérarchisé de 15 plaques tectoniques :
    /// - 4 Grosses plaques
    /// - 3 Moyennes plaques
    /// - 8 Petites plaques
    pub fn new(seed: u64) -> Self {
        let count = 15;
        let mut plates = Vec::with_capacity(count);

        // Palette de 15 teintes distinctes pour chaque plaque
        let palette: [[f32; 4]; 15] = [
            // 4 Plaques Majeures
            [0.85, 0.32, 0.30, 1.0], // 0: Rouge rubis
            [0.25, 0.55, 0.85, 1.0], // 1: Bleu saphir
            [0.30, 0.75, 0.45, 1.0], // 2: Vert émeraude
            [0.88, 0.72, 0.25, 1.0], // 3: Jaune ambre
            // 3 Plaques Moyennes
            [0.70, 0.38, 0.82, 1.0], // 4: Violet améthyste
            [0.22, 0.82, 0.80, 1.0], // 5: Cyan turquoise
            [0.90, 0.52, 0.22, 1.0], // 6: Orange topaze
            // 8 Petites Plaques
            [0.45, 0.78, 0.30, 1.0], // 7: Vert pomme
            [0.85, 0.38, 0.65, 1.0], // 8: Rose fuchsia
            [0.55, 0.42, 0.28, 1.0], // 9: Brun caramel
            [0.30, 0.40, 0.65, 1.0], // 10: Bleu cobalt
            [0.65, 0.68, 0.75, 1.0], // 11: Gris argent
            [0.80, 0.82, 0.35, 1.0], // 12: Jaune chartreuse
            [0.35, 0.85, 0.65, 1.0], // 13: Menthe boréale
            [0.60, 0.30, 0.45, 1.0], // 14: Prune pourpre
        ];

        // Répartition sphérique des 15 centres (Fibonacci sphere)
        let phi = (1.0 + 5.0_f64.sqrt()) * 0.5;
        for i in 0..count {
            let y = 1.0 - (i as f64) / ((count - 1) as f64) * 2.0; // [-1.0, 1.0]
            let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;

            // Gigue pseudo-aléatoire selon la graine
            let jitter_seed = seed.wrapping_add((i as u64).wrapping_mul(987654321));
            let jx = ((jitter_seed.wrapping_mul(1103515245) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;
            let jy = ((jitter_seed.wrapping_mul(1664525) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;
            let jz = ((jitter_seed.wrapping_mul(214013) >> 16) & 0xFFF) as f64 / 4096.0 - 0.5;

            let raw_pos = DVec3::new(
                radius_at_y * theta.cos() + jx * 0.22,
                y + jy * 0.22,
                radius_at_y * theta.sin() + jz * 0.22,
            );
            let center = raw_pos.normalize();

            // Attribution équilibrée sur le globe :
            // 4 Grosses Plaques : indices 2, 5, 9, 12
            // 3 Moyennes Plaques : indices 4, 7, 10
            // 8 Petites Plaques : indices 0, 1, 3, 6, 8, 11, 13, 14
            let (category, weight, is_oceanic, drift_speed) = match i {
                // 4 Grandes plaques (w = 1.45)
                2 | 9 | 12 => (PlateCategory::Major, 1.45, false, 0.45), // 3 grands continents
                5 => (PlateCategory::Major, 1.45, true, 0.55),           // 1 méga-bassin océanique (Pacifique)
                // 3 Moyennes plaques (w = 1.05)
                4 | 10 => (PlateCategory::Medium, 1.05, false, 0.65),    // 2 continents secondaires
                7 => (PlateCategory::Medium, 1.05, true, 0.70),          // 1 océan intermédiaire
                // 8 Petites plaques (w = 0.65, rapides et dynamiques)
                1 | 13 => (PlateCategory::Minor, 0.65, false, 0.85),     // 2 micro-continents
                _ => (PlateCategory::Minor, 0.65, true, 1.05),           // 6 plaques océaniques de subduction
            };

            // Axe de dérive de rotation
            let raw_axis = DVec3::new(
                ((jitter_seed.wrapping_add(1).wrapping_mul(2246822519)) & 0xFFF) as f64 / 2048.0 - 1.0,
                ((jitter_seed.wrapping_add(2).wrapping_mul(3266489917)) & 0xFFF) as f64 / 2048.0 - 1.0,
                ((jitter_seed.wrapping_add(3).wrapping_mul(668265263)) & 0xFFF) as f64 / 2048.0 - 1.0,
            );
            let rotation_axis = raw_axis.normalize_or_zero();

            let base_elevation = if is_oceanic { 5.0 } else { 15.0 };
            let color = palette[i % palette.len()];

            plates.push(TectonicPlate {
                id: i,
                category,
                center,
                weight,
                is_oceanic,
                rotation_axis,
                drift_speed,
                base_elevation,
                color,
            });
        }

        Self { seed, plates }
    }

    /// Échantillonne l'état tectonique pour une direction donnée via Voronoi sphérique pondéré
    pub fn sample(&self, dir: DVec3) -> TectonicSample {
        // Distorsion fractale organique des frontières de plaques
        let warp_x = (dir.x * 3.5 + (self.seed as f64 * 0.001)).sin() * 0.08;
        let warp_y = (dir.y * 3.5 + 1.7).cos() * 0.08;
        let warp_z = (dir.z * 3.5 + 3.1).sin() * 0.08;
        let warped_dir = (dir + DVec3::new(warp_x, warp_y, warp_z)).normalize();

        // Trouver les 2 plaques les plus proches selon la métrique pondérée theta_i / w_i
        let mut best_id = 0;
        let mut second_id = 1;
        let mut min_dist = f64::MAX;
        let mut second_dist = f64::MAX;

        for plate in &self.plates {
            let dot = warped_dir.dot(plate.center).clamp(-1.0, 1.0);
            let angle = dot.acos();
            let d = angle / plate.weight;

            if d < min_dist {
                second_dist = min_dist;
                second_id = best_id;
                min_dist = d;
                best_id = plate.id;
            } else if d < second_dist {
                second_dist = d;
                second_id = plate.id;
            }
        }

        let p1 = &self.plates[best_id];
        let p2 = &self.plates[second_id];

        // Distance relative à la frontière (0 = sur la faille, 1 = au cœur de la plaque)
        let margin = (second_dist - min_dist).max(0.0);
        let dist_to_boundary = (margin * 3.5).min(1.0);

        // Vitesse relative des deux plaques au point de contact
        let v1 = p1.rotation_axis.cross(warped_dir) * p1.drift_speed;
        let v2 = p2.rotation_axis.cross(warped_dir) * p2.drift_speed;
        let v_rel = v1 - v2;

        // Normale de la frontière : orientée de p1 vers p2
        let boundary_normal = (p2.center - p1.center).normalize_or_zero();
        let relative_motion = v_rel.dot(boundary_normal);

        let (boundary_type, elevation_offset, orogeny_strength) = if dist_to_boundary < 0.40 {
            let boundary_factor = (1.0 - (dist_to_boundary / 0.40)).max(0.0);

            if relative_motion > 0.05 {
                // Convergence : collision de plaques (Orogenèse / Super-chaînes de montagnes ou Fosses)
                let collision_intensity = (relative_motion * 3.5).clamp(0.6, 2.0);

                let (peak_height, orogeny_mult) = if !p1.is_oceanic && !p2.is_oceanic {
                    // Collision continentale : Super-chaîne orogénique colossale (Himalaya)
                    (18.0 * boundary_factor * collision_intensity.min(1.5), 1.5)
                } else if p1.is_oceanic && !p2.is_oceanic {
                    // Subduction océan sous continent : fosse marine profonde
                    (-6.0 * boundary_factor, 0.4)
                } else if !p1.is_oceanic && p2.is_oceanic {
                    // Côté continental de la subduction : cordillère volcanique majeure (Andes)
                    (15.0 * boundary_factor * collision_intensity.min(1.4), 1.25)
                } else {
                    // Collision océan/océan : fosse et arcs insulaires
                    (-4.5 * boundary_factor, 0.3)
                };

                let orogeny = boundary_factor.powi(2) * collision_intensity * orogeny_mult;
                (BoundaryType::Convergence, peak_height, orogeny)
            } else if relative_motion < -0.05 {
                // Divergence : écartement des plaques (Dorsale médio-océanique ou Rift)
                let rift = if p1.is_oceanic && p2.is_oceanic {
                    // Dorsale océanique (crête sous-marine avec rift central)
                    let ridge = 3.5 * boundary_factor;
                    let central_groove = if dist_to_boundary < 0.08 { -2.0 } else { 0.0 };
                    ridge + central_groove
                } else {
                    // Rift continental : vallée effondrée
                    -4.5 * boundary_factor
                };
                (BoundaryType::Divergence, rift, 0.0)
            } else {
                (BoundaryType::Transform, 0.0, 0.0)
            }
        } else {
            (BoundaryType::None, 0.0, 0.0)
        };

        // Couleur de debug pour la visualisation
        let mut debug_color = p1.color;
        if dist_to_boundary < 0.12 {
            // Surlignage vif des lignes de failles
            match boundary_type {
                BoundaryType::Convergence => {
                    // Rouge vif / lave pour les collisions et super-chaînes
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
            orogeny_strength,
            debug_color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tectonics_creation_and_sampling() {
        let sim = TectonicSimulation::new(42);
        assert_eq!(sim.plates.len(), 15);

        let major_count = sim.plates.iter().filter(|p| p.category == PlateCategory::Major).count();
        let medium_count = sim.plates.iter().filter(|p| p.category == PlateCategory::Medium).count();
        let minor_count = sim.plates.iter().filter(|p| p.category == PlateCategory::Minor).count();

        assert_eq!(major_count, 4, "Il doit y avoir exactement 4 grosses plaques");
        assert_eq!(medium_count, 3, "Il doit y avoir exactement 3 moyennes plaques");
        assert_eq!(minor_count, 8, "Il doit y avoir exactement 8 petites plaques");

        let p = DVec3::new(1.0, 0.0, 0.0);
        let sample = sim.sample(p);
        assert!(sample.plate_id < 15);
        assert!(sample.dist_to_boundary >= 0.0 && sample.dist_to_boundary <= 1.0);
        assert!(sample.debug_color[3] > 0.0);
    }
}

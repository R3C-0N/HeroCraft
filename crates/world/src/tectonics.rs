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

/// Générateur pseudo-aléatoire rapide déterministe par graine
struct PlateRng(u64);

impl PlateRng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x517cc1b727220a95)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn next_range(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    fn next_vec3(&mut self) -> DVec3 {
        let x = self.next_range(-1.0, 1.0);
        let y = self.next_range(-1.0, 1.0);
        let z = self.next_range(-1.0, 1.0);
        DVec3::new(x, y, z).normalize_or_zero()
    }
}

fn rotate_vector(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    v * cos_a + axis.cross(v) * sin_a + axis * axis.dot(v) * (1.0 - cos_a)
}

#[derive(Clone, Copy)]
struct PlateRole {
    category: PlateCategory,
    weight: f64,
    is_oceanic: bool,
    drift_speed: f64,
}

impl TectonicSimulation {
    /// Génère un système hiérarchisé de 15 plaques tectoniques avec rotation 3D et brassage par graine :
    /// - 4 Grosses plaques
    /// - 3 Moyennes plaques
    /// - 8 Petites plaques
    pub fn new(seed: u64) -> Self {
        let count = 15;
        let mut plates = Vec::with_capacity(count);
        let mut rng = PlateRng::new(seed);

        // Palette de 15 teintes distinctes pour chaque plaque
        let palette: [[f32; 4]; 15] = [
            [0.85, 0.32, 0.30, 1.0], // 0: Rouge rubis
            [0.25, 0.55, 0.85, 1.0], // 1: Bleu saphir
            [0.30, 0.75, 0.45, 1.0], // 2: Vert émeraude
            [0.88, 0.72, 0.25, 1.0], // 3: Jaune ambre
            [0.70, 0.38, 0.82, 1.0], // 4: Violet améthyste
            [0.22, 0.82, 0.80, 1.0], // 5: Cyan turquoise
            [0.90, 0.52, 0.22, 1.0], // 6: Orange topaze
            [0.45, 0.78, 0.30, 1.0], // 7: Vert pomme
            [0.85, 0.38, 0.65, 1.0], // 8: Rose fuchsia
            [0.55, 0.42, 0.28, 1.0], // 9: Brun caramel
            [0.30, 0.40, 0.65, 1.0], // 10: Bleu cobalt
            [0.65, 0.68, 0.75, 1.0], // 11: Gris argent
            [0.80, 0.82, 0.35, 1.0], // 12: Jaune chartreuse
            [0.35, 0.85, 0.65, 1.0], // 13: Menthe boréale
            [0.60, 0.30, 0.45, 1.0], // 14: Prune pourpre
        ];

        // Rôles géologiques : exactement 4 majeures, 3 moyennes, 8 mineures
        let mut roles = [
            // 4 Grosses plaques (3 super-continents + 1 méga-océan Pacifique)
            PlateRole { category: PlateCategory::Major, weight: 1.45, is_oceanic: false, drift_speed: 0.45 },
            PlateRole { category: PlateCategory::Major, weight: 1.45, is_oceanic: false, drift_speed: 0.48 },
            PlateRole { category: PlateCategory::Major, weight: 1.45, is_oceanic: false, drift_speed: 0.52 },
            PlateRole { category: PlateCategory::Major, weight: 1.45, is_oceanic: true,  drift_speed: 0.56 },
            // 3 Moyennes plaques (2 continents moyens + 1 océan intermédiaire)
            PlateRole { category: PlateCategory::Medium, weight: 1.05, is_oceanic: false, drift_speed: 0.65 },
            PlateRole { category: PlateCategory::Medium, weight: 1.05, is_oceanic: false, drift_speed: 0.68 },
            PlateRole { category: PlateCategory::Medium, weight: 1.05, is_oceanic: true,  drift_speed: 0.72 },
            // 8 Petites plaques (2 micro-continents + 6 plaques de subduction rapides)
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: false, drift_speed: 0.85 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: false, drift_speed: 0.90 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.05 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.10 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.15 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.20 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.25 },
            PlateRole { category: PlateCategory::Minor, weight: 0.65, is_oceanic: true,  drift_speed: 1.30 },
        ];

        // Brassage aléatoire (Fisher-Yates) des rôles selon la graine
        for i in (1..roles.len()).rev() {
            let j = (rng.next_u64() as usize) % (i + 1);
            roles.swap(i, j);
        }

        // Rotation 3D globale aléatoire de toute la lithosphère selon la graine
        let global_rot_axis = rng.next_vec3();
        let global_rot_angle = rng.next_range(0.0, std::f64::consts::TAU);

        // Répartition sphérique des 15 centres (Fibonacci sphere) réorientée en 3D
        let phi = (1.0 + 5.0_f64.sqrt()) * 0.5;
        for i in 0..count {
            let y = 1.0 - (i as f64) / ((count - 1) as f64) * 2.0; // [-1.0, 1.0]
            let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;

            // Gigue pseudo-aléatoire
            let jx = rng.next_range(-0.25, 0.25);
            let jy = rng.next_range(-0.25, 0.25);
            let jz = rng.next_range(-0.25, 0.25);

            let raw_pos = DVec3::new(
                radius_at_y * theta.cos() + jx,
                y + jy,
                radius_at_y * theta.sin() + jz,
            ).normalize();

            // Application de la rotation 3D globale pour une disposition spatiale 100% unique par seed
            let center = rotate_vector(raw_pos, global_rot_axis, global_rot_angle).normalize();

            let role = roles[i];

            // Axe de dérive propre à chaque plaque
            let rotation_axis = rng.next_vec3();

            let base_elevation = if role.is_oceanic { 5.0 } else { 15.0 };
            let color = palette[i % palette.len()];

            plates.push(TectonicPlate {
                id: i,
                category: role.category,
                center,
                weight: role.weight,
                is_oceanic: role.is_oceanic,
                rotation_axis,
                drift_speed: role.drift_speed,
                base_elevation,
                color,
            });
        }

        Self { seed, plates }
    }

    /// Fraction de croûte continentale (0 = océanique, 1 = continentale) en un point, lissée entre plaques.
    ///
    /// Contrairement à `sample`, qui tranche net sur la frontière du Voronoi pondéré (des arcs de cercle,
    /// puisque la métrique theta_i / w_i produit des cercles d'Apollonius), chaque plaque contribue ici
    /// selon un noyau exponentiel de sa distance pondérée : le passage océan/continent devient un gradient
    /// large que le bruit continental peut déformer librement, au lieu d'imposer la géométrie des plaques au rivage.
    pub fn continental_crust(&self, dir: DVec3, softness: f64) -> f64 {
        let dists: Vec<f64> = self
            .plates
            .iter()
            .map(|p| dir.dot(p.center).clamp(-1.0, 1.0).acos() / p.weight)
            .collect();
        let min_dist = dists.iter().cloned().fold(f64::MAX, f64::min);

        let mut total = 0.0;
        let mut continental = 0.0;
        for (plate, d) in self.plates.iter().zip(dists) {
            let w = (-(d - min_dist) / softness).exp();
            total += w;
            if !plate.is_oceanic {
                continental += w;
            }
        }
        continental / total
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

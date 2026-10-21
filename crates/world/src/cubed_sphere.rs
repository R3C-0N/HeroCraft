use glam::DVec3;

/// Rayon de base de référence du monde (1.0 = prototype initial à rayon 25.0)
pub const REFERENCE_BASE_RADIUS: f64 = 25.0;

/// Facteur d'échelle global de la planète (modifiable en un seul point !)
/// - 1.0 : Rayon 25.0 (taille prototype initiale, circonférence ~157 m, ~4096 blocs)
/// - 2.0 : Rayon 50.0 (taille doublée, circonférence ~314 m)
/// - 4.0 : Rayon 100.0 (taille quadruplée, circonférence ~628 m)
/// Modifier cette constante met à l'échelle instantanément toute la planète, les LODs, la caméra,
/// les nuages et la physique du joueur.
pub const DEFAULT_PLANET_SCALE: f64 = 2.0;

/// Paramètres physiques d'une planète voxel
#[derive(Debug, Clone)]
pub struct PlanetDescriptor {
    pub name: String,
    pub radius_blocks: f64,
    pub max_mountain_height: f64,
    pub sea_level: f64,
    pub seed: u64,
    /// Facteur multiplicateur d'échelle globale (1.0 = base 25.0, 2.0 = base 50.0, etc.)
    pub scale: f64,
}

impl Default for PlanetDescriptor {
    fn default() -> Self {
        Self {
            name: "Aethelgard".to_string(),
            radius_blocks: 1024.0 * DEFAULT_PLANET_SCALE, // Rayon en voxels
            max_mountain_height: 120.0,
            sea_level: 0.0,
            seed: 42,
            scale: DEFAULT_PLANET_SCALE,
        }
    }
}

impl PlanetDescriptor {
    /// Rayon de base effectif de la planète dans l'espace 3D monde
    #[inline]
    pub fn base_radius(&self) -> f64 {
        REFERENCE_BASE_RADIUS * self.scale
    }

    /// Échelle d'altitude (largeur et hauteur d'un voxel en unités monde)
    #[inline]
    pub fn altitude_scale(&self) -> f64 {
        self.base_radius() * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0)
    }
}

/// Convertit un point 3D sur la sphère unitaire en latitude et longitude géographiques
pub fn unit_sphere_to_lat_lon(point: DVec3) -> (f64, f64) {
    let lat = point.y.clamp(-1.0, 1.0).asin(); // Latitude [-pi/2, pi/2]
    let lon = point.z.atan2(point.x);          // Longitude [-pi, pi]
    (lat, lon)
}

/// Évalue l'influence solaire (température de base selon la latitude)
pub fn base_solar_temperature(latitude_radians: f64) -> f64 {
    // 1.0 à l'équateur (lat = 0), 0.0 aux pôles (lat = +/- pi/2)
    latitude_radians.abs().cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_lat_lon() {
        let north_pole = DVec3::new(0.0, 1.0, 0.0);
        let (lat, _) = unit_sphere_to_lat_lon(north_pole);
        assert!((lat - std::f64::consts::FRAC_PI_2).abs() < 1e-5);

        let equator = DVec3::new(1.0, 0.0, 0.0);
        let (lat_eq, _) = unit_sphere_to_lat_lon(equator);
        assert!(lat_eq.abs() < 1e-5);
    }

    #[test]
    fn test_solar_temperature() {
        let equator_temp = base_solar_temperature(0.0);
        assert!((equator_temp - 1.0).abs() < 1e-5);

        let pole_temp = base_solar_temperature(std::f64::consts::FRAC_PI_2);
        assert!(pole_temp.abs() < 1e-5);
    }
}

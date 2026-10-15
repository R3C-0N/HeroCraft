use glam::DVec3;

/// Paramètres physiques d'une planète voxel
#[derive(Debug, Clone)]
pub struct PlanetDescriptor {
    pub name: String,
    pub radius_blocks: f64,
    pub max_mountain_height: f64,
    pub sea_level: f64,
    pub seed: u64,
}

impl Default for PlanetDescriptor {
    fn default() -> Self {
        Self {
            name: "Aethelgard".to_string(),
            radius_blocks: 1024.0, // Rayon en voxels pour le prototype
            max_mountain_height: 120.0,
            sea_level: 0.0,
            seed: 42,
        }
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

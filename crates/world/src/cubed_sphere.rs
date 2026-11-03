use glam::DVec3;

/// Rayon de base de référence du monde (1.0 = prototype initial à rayon 25.0)
pub const REFERENCE_BASE_RADIUS: f64 = 25.0;

/// Blocs par arête de face du prototype, pour lequel les dimensions du rendu (nuages, ombres,
/// caméra, LOD…) ont été réglées en unités du monde
pub const REFERENCE_FACE_BLOCKS: f64 = 1024.0;

/// Agrandissement de la planète par rapport au prototype, en blocs par face (8 pour 8 192 blocs).
/// Le bloc garde sa taille en unités du monde : c'est le rayon qui grandit.
pub const FACE_SCALE: f64 = crate::edits::FACE_BLOCKS as f64 / REFERENCE_FACE_BLOCKS;

/// Échelle des dimensions mesurées en blocs (hauteur des yeux, nuages, portée des ombres…) :
/// celle du prototype de rayon 50 et de 1 024 blocs par face, que la taille de la planète ne change pas
pub const BLOCK_SCALE: f64 = 2.0;

/// Facteur d'échelle global de la planète (rayon = `REFERENCE_BASE_RADIUS` × ce facteur) :
/// `BLOCK_SCALE` × `FACE_SCALE`, soit un rayon de 400 pour 8 192 blocs par face. Les dimensions qui
/// suivent la planète entière (orbite, atmosphère vue de l'espace, QuadTree) s'y rapportent ; celles
/// qui se mesurent en blocs se rapportent à `BLOCK_SCALE` (voir `block_scale`).
pub const DEFAULT_PLANET_SCALE: f64 = BLOCK_SCALE * FACE_SCALE;

/// Échelle « en blocs » d'une planète de rayon `base_radius` : le facteur par lequel multiplier
/// une dimension réglée sur le prototype de rayon 25 pour qu'elle garde sa taille en blocs
#[inline]
pub fn block_scale(base_radius: f64) -> f64 {
    base_radius / REFERENCE_BASE_RADIUS / FACE_SCALE
}

/// Taille d'un bloc (largeur au centre d'une face, et hauteur) en unités du monde, sur une planète
/// de rayon `base_radius`
#[inline]
pub fn block_size(base_radius: f64) -> f64 {
    base_radius * std::f64::consts::FRAC_PI_2 / crate::edits::FACE_BLOCKS as f64
}

/// Dossier de sauvegarde d'un monde : `saves/<prefixe>_<graine>`, suivi de la taille des faces
/// quand elle n'est pas celle du prototype (une sauvegarde d'une autre taille de planète n'y est
/// pas relue : ses blocs tomberaient ailleurs)
pub fn save_dir_name(prefix: &str, seed: u64) -> String {
    if crate::edits::FACE_BLOCKS == REFERENCE_FACE_BLOCKS as i32 {
        format!("saves/{prefix}_{seed}")
    } else {
        format!("saves/{prefix}_{seed}_f{}", crate::edits::FACE_BLOCKS)
    }
}

/// Graine tirée au hasard pour une nouvelle planète : entropie du système (clés aléatoires de
/// `RandomState`) mélangée à l'horloge, bornée à 10 chiffres pour rester facile à recopier
pub fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    hasher.write_u128(nanos);
    hasher.finish() % 10_000_000_000
}

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
            radius_blocks: crate::edits::FACE_BLOCKS as f64 / std::f64::consts::FRAC_PI_2, // Rayon en voxels
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
        self.base_radius() * std::f64::consts::FRAC_PI_4 * (2.0 / crate::edits::FACE_BLOCKS as f64)
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
    fn test_random_seed_varie() {
        let seeds: std::collections::HashSet<u64> = (0..64).map(|_| random_seed()).collect();
        assert!(seeds.len() > 60, "graines trop souvent identiques : {seeds:?}");
        assert!(seeds.iter().all(|&s| s < 10_000_000_000));
    }

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

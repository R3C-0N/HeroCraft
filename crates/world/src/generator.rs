use crate::cubed_sphere::{base_solar_temperature, unit_sphere_to_lat_lon, PlanetDescriptor};
use crate::ocean_currents::OceanicCurrentSimulation;
use crate::tectonics::TectonicSimulation;
use glam::DVec3;
use herocraft_common::{BlockId, PlanetChunkCoord};
use herocraft_voxel_engine::{ChunkData, CHUNK_SIZE};

/// Mode de filtre de visualisation pour la planète
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualizationFilter {
    #[default]
    Standard,
    Tectonics,
    OceanCurrents,
}

/// Niveau de la mer de référence en blocs (dans un chunk vertical de 32 blocs)
pub const SEA_LEVEL_BLOCKS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiomeType {
    /// Plaines et fosses océaniques abyssales
    DeepOcean,
    /// Grands océans ouverts
    Ocean,
    /// Mers intérieures, mers côtières, détroits et golfes
    Sea,
    /// Plages et bandes côtières de sable
    Beach,
    /// Prairies et plaines tempérées
    Plains,
    /// Forêts denses et bois
    Forest,
    /// Déserts de sable et canyons
    Desert,
    /// Massifs montagneux et crêtes rocheuses
    Mountains,
    /// Sommets enneigés, glaciers et calottes polaires
    SnowPeaks,
}

impl BiomeType {
    pub fn is_water(&self) -> bool {
        matches!(self, BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea)
    }
}

pub struct PlanetWorldGenerator {
    pub planet: PlanetDescriptor,
    pub tectonics: TectonicSimulation,
    pub ocean_currents: OceanicCurrentSimulation,
}

impl PlanetWorldGenerator {
    pub fn new(planet: PlanetDescriptor) -> Self {
        let tectonics = TectonicSimulation::new(planet.seed);
        let ocean_currents = OceanicCurrentSimulation::new(planet.seed);
        Self {
            planet,
            tectonics,
            ocean_currents,
        }
    }

    /// Génère un chunk voxel complet sur une face de la planète
    pub fn generate_chunk(&self, coord: &PlanetChunkCoord) -> ChunkData {
        let mut chunk = ChunkData::new_empty();

        let face_chunks_count = 32.0;
        let chunk_span_uv = 2.0 / face_chunks_count;

        let base_u = -1.0 + (coord.x as f64) * chunk_span_uv;
        let base_v = -1.0 + (coord.y as f64) * chunk_span_uv;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);

        let sea_level_y = (SEA_LEVEL_BLOCKS - (coord.altitude_layer as f64 * CHUNK_SIZE as f64))
            .clamp(0.0, CHUNK_SIZE as f64) as u8;

        for z in 0..CHUNK_SIZE as u8 {
            let v = base_v + (z as f64) * voxel_span_uv;
            for x in 0..CHUNK_SIZE as u8 {
                let u = base_u + (x as f64) * voxel_span_uv;

                let sphere_dir = coord.face.uv_to_sphere_direction(u, v);
                let (lat, lon) = unit_sphere_to_lat_lon(sphere_dir);

                let elevation = self.sample_elevation(sphere_dir, lat, lon);
                let biome = self.determine_biome(lat, elevation, sphere_dir);

                let ground_y = ((elevation - (coord.altitude_layer as f64 * CHUNK_SIZE as f64))
                    .clamp(0.0, CHUNK_SIZE as f64)) as u8;

                // 1. Remplissage du sol solide
                for y in 0..ground_y {
                    let block = if y + 1 == ground_y {
                        match biome {
                            BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea => BlockId::SAND,
                            BiomeType::Beach | BiomeType::Desert => BlockId::SAND,
                            BiomeType::SnowPeaks => BlockId::SNOW,
                            BiomeType::Mountains => BlockId::STONE,
                            _ => BlockId::GRASS,
                        }
                    } else if y + 4 >= ground_y {
                        BlockId::DIRT
                    } else {
                        BlockId::STONE
                    };
                    chunk.set_voxel(x, y, z, block);
                }

                // 2. Remplissage de l'eau jusqu'au niveau de la mer si sous l'eau
                if ground_y < sea_level_y {
                    for y in ground_y..sea_level_y {
                        let water_block = if biome == BiomeType::SnowPeaks {
                            // Aux pôles, la surface de l'eau est gelée en neige/glace
                            BlockId::SNOW
                        } else {
                            BlockId::WATER
                        };
                        chunk.set_voxel(x, y, z, water_block);
                    }
                }
            }
        }

        chunk
    }

    /// Calcule la valeur continentale brute sur la sphère
    /// Continents massifs ancrés au cœur des plaques continentales, entourés de leurs marges océaniques
    pub fn sample_continental_value(&self, dir: DVec3) -> f64 {
        let seed = self.planet.seed;

        // 1. Déformation de domaine douce pour des contours naturels sans micro-trous
        let warp_x = fbm_3d(dir * 1.1, 2, 2.0, 0.40, seed.wrapping_add(101));
        let warp_y = fbm_3d(dir * 1.1, 2, 2.0, 0.40, seed.wrapping_add(202));
        let warp_z = fbm_3d(dir * 1.1, 2, 2.0, 0.40, seed.wrapping_add(303));
        let warped_dir = (dir + DVec3::new(warp_x, warp_y, warp_z) * 0.22).normalize();

        // 2. Ancrage géologique réaliste sur les plaques tectoniques :
        // Les plaques continentales portent les continents en leur cœur (terres émergées entourées de marges océaniques).
        // Les plaques océaniques forment les vastes bassins maritimes.
        let tec = self.tectonics.sample(warped_dir);
        let plate = &self.tectonics.plates[tec.plate_id];

        let plate_bias = if !plate.is_oceanic {
            // Cœur de continent : dist_to_boundary va de 0 (frontière) à 1 (cœur)
            // Plus on est au centre de la plaque continentale, plus la masse de terre est compacte
            0.12 + tec.dist_to_boundary * 0.32
        } else {
            // Plaque océanique : bassin sous-marin profond
            -0.22 - (1.0 - tec.dist_to_boundary) * 0.12
        };

        // Bruit fractal macroscopique combiné à l'ancrage des plaques
        let macro_noise = fbm_3d(warped_dir * 0.95, 2, 2.0, 0.35, seed);
        let macro_continent = plate_bias * 0.60 + macro_noise * 0.40;

        // 3. Mers intérieures (type Méditerranée ou Mer Baltique) bien délimitées,
        // évitant de percer le cœur des terres
        let inland_sea = if macro_continent > 0.08 && macro_continent < 0.28 {
            let sea_dip = fbm_3d(dir * 2.6, 2, 2.0, 0.50, seed.wrapping_add(5555));
            if sea_dip < -0.32 {
                -0.18 // Creuse une mer intérieure nette
            } else {
                0.0
            }
        } else {
            0.0
        };

        // 4. Découpage côtier fin appliqué UNIQUEMENT sur la ligne de rivage (|val| < 0.14)
        // L'intérieur des continents et des océans reste ainsi 100% plein et solide
        let coastal_detail = if macro_continent.abs() < 0.14 {
            let factor = 1.0 - (macro_continent.abs() / 0.14);
            let fine = fbm_3d(dir * 4.2, 2, 2.0, 0.50, seed.wrapping_add(9999));
            fine * 0.07 * factor
        } else {
            0.0
        };

        macro_continent + inland_sea + coastal_detail
    }

    /// Fonction de relief procédurale planétaire garantissant au moins 40-50% d'océans et mers
    pub fn sample_elevation(&self, dir: DVec3, lat: f64, _lon: f64) -> f64 {
        let continental = self.sample_continental_value(dir);
        let sea_threshold = 0.01;
        let tectonic_sample = self.tectonics.sample(dir);

        // Calotte polaire resserrée (~73° de latitude) avec découpage fractal naturel
        let polar_noise = fbm_3d(dir * 3.2, 2, 2.0, 0.50, self.planet.seed.wrapping_add(3333)) * 0.08;
        let polar_boundary = 1.28 + polar_noise;
        let is_polar = lat.abs() > polar_boundary;

        if continental < sea_threshold {
            // Zone maritime
            if is_polar {
                // Banquise affleurante parfaitement alignée sur la sphère océanique (évite l'effet de plateau en profil)
                SEA_LEVEL_BLOCKS + 0.1
            } else {
                // Fond marin sous le niveau de la mer (avec fosses ou dorsales médio-océaniques)
                let depth_ratio = (sea_threshold - continental) / (1.0 + sea_threshold);
                (SEA_LEVEL_BLOCKS - depth_ratio * 7.0 + tectonic_sample.elevation_offset * 0.15)
                    .clamp(3.0, SEA_LEVEL_BLOCKS - 0.2)
            }
        } else {
            // Terre émergée : continents, collines, montagnes intérieures ET super-chaînes tectoniques
            let land_ratio = (continental - sea_threshold) / (1.0 - sea_threshold);

            // Dôme glaciaire naturel sur la terre polaire
            let polar_dome = if is_polar {
                (lat.abs() - polar_boundary).max(0.0) * 4.0
            } else {
                0.0
            };

            // 1. Montagnes ordinaires (pouvant apparaître n'importe où sur les terres émergées)
            let mountain_raw = fbm_3d(dir * 4.5, 4, 2.0, 0.50, self.planet.seed.wrapping_add(10101));
            let ridge = 1.0 - mountain_raw.abs();
            let standard_mountains = ridge * ridge * 7.5 * land_ratio.min(1.0);

            // 2. Collines douces
            let hills = fbm_3d(dir * 10.0, 3, 2.0, 0.50, self.planet.seed.wrapping_add(20202)) * 2.0;

            // 3. SUPER-CHAÎNES TECTONIQUES : Orogenèse colossale le long des collisions de plaques (Himalaya, Andes)
            let super_chain = if tectonic_sample.orogeny_strength > 0.02 {
                let super_noise = fbm_3d(dir * 8.5, 4, 2.0, 0.50, self.planet.seed.wrapping_add(88888));
                let super_ridge = (1.0 - super_noise.abs()).powi(2);
                super_ridge * 16.0 * tectonic_sample.orogeny_strength.min(1.5) * land_ratio.min(1.0)
            } else {
                0.0
            };

            (SEA_LEVEL_BLOCKS + land_ratio * 4.5 + standard_mountains + hills + polar_dome + super_chain)
                .max(SEA_LEVEL_BLOCKS + 0.2)
        }
    }

    /// Détermine le biome selon la latitude, l'altitude et la valeur continentale
    pub fn determine_biome(&self, lat: f64, elevation: f64, dir: DVec3) -> BiomeType {
        // 1. Calottes glaciaires et banquises permanentes aux pôles (Pôle Nord et Pôle Sud)
        // Calibrées à ~73° de latitude pour un équilibre parfait entre vue de profil et vue du dessus
        let polar_noise = fbm_3d(dir * 3.2, 2, 2.0, 0.50, self.planet.seed.wrapping_add(3333)) * 0.08;
        let polar_boundary = 1.28 + polar_noise;
        let is_polar = lat.abs() > polar_boundary;

        if is_polar {
            return BiomeType::SnowPeaks;
        }

        let continental = self.sample_continental_value(dir);
        let sea_threshold = 0.01;

        // 2. Biomes maritimes (Océans et Mers)
        if continental < -0.20 {
            BiomeType::DeepOcean // Plaines et fosses océaniques profondes
        } else if continental < -0.05 {
            BiomeType::Ocean // Grands océans ouverts
        } else if continental < sea_threshold {
            BiomeType::Sea // Mers intérieures, détroits et golfes
        } else if continental < sea_threshold + 0.03 {
            BiomeType::Beach // Rivages de sable côtier
        } else {
            // 3. Biomes continentaux
            let solar_temp = base_solar_temperature(lat);
            let altitude_cooling = (elevation - SEA_LEVEL_BLOCKS) / 26.0;
            let temp = (solar_temp - altitude_cooling).clamp(0.0, 1.0);

            // Humidité procédurale
            let moisture = fbm_3d(dir * 2.2, 3, 2.0, 0.50, self.planet.seed.wrapping_add(77777)) * 0.5 + 0.5;

            if temp < 0.22 || elevation > 21.0 {
                BiomeType::SnowPeaks // Sommets de haute montagne
            } else if elevation > 16.0 {
                BiomeType::Mountains // Crêtes rocheuses
            } else if temp > 0.65 && moisture < 0.38 {
                BiomeType::Desert // Déserts chauds
            } else if moisture > 0.52 {
                BiomeType::Forest // Forêts
            } else {
                BiomeType::Plains // Plaines et prairies
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fonctions mathématiques de bruit de gradient 3D (sans dépendance externe)
// ---------------------------------------------------------------------------

fn hash_u64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

fn grad3(x: i64, y: i64, z: i64, seed: u64) -> DVec3 {
    let h = hash_u64(
        (x as u64).wrapping_mul(73856093)
            ^ (y as u64).wrapping_mul(19349663)
            ^ (z as u64).wrapping_mul(83492791)
            ^ seed,
    );
    let theta = (h as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let phi = (((h >> 32) as u32) as f64 / u32::MAX as f64) * std::f64::consts::PI;
    DVec3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
}

pub fn gradient_noise_3d(p: DVec3, seed: u64) -> f64 {
    let x0 = p.x.floor() as i64;
    let y0 = p.y.floor() as i64;
    let z0 = p.z.floor() as i64;

    let fx = p.x - x0 as f64;
    let fy = p.y - y0 as f64;
    let fz = p.z - z0 as f64;

    let u = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let v = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let w = fz * fz * fz * (fz * (fz * 6.0 - 15.0) + 10.0);

    let mut result = 0.0;
    for dx in 0..=1 {
        for dy in 0..=1 {
            for dz in 0..=1 {
                let grad = grad3(x0 + dx, y0 + dy, z0 + dz, seed);
                let d = DVec3::new(fx - dx as f64, fy - dy as f64, fz - dz as f64);
                let dot = grad.dot(d);
                let wx = if dx == 0 { 1.0 - u } else { u };
                let wy = if dy == 0 { 1.0 - v } else { v };
                let wz = if dz == 0 { 1.0 - w } else { w };
                result += dot * wx * wy * wz;
            }
        }
    }
    result
}

pub fn fbm_3d(p: DVec3, octaves: usize, lacunarity: f64, gain: f64, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut max_amp = 0.0;

    for i in 0..octaves {
        sum += gradient_noise_3d(p * freq, seed.wrapping_add((i as u64).wrapping_mul(1013))) * amp;
        max_amp += amp;
        freq *= lacunarity;
        amp *= gain;
    }
    sum / max_amp
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::PlanetFace;

    #[test]
    fn test_generate_chunk_has_blocks() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());
        let coord = PlanetChunkCoord::new(PlanetFace::PosX, 0, 0, 0);
        let chunk = generator.generate_chunk(&coord);

        assert!(!chunk.get_voxel(0, 0, 0).is_air());
        assert!(chunk.get_voxel(0, 31, 0).is_air());
    }

    #[test]
    fn test_north_and_south_poles_exist() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());

        let north_pole = DVec3::new(0.0, 1.0, 0.0);
        let (lat_n, lon_n) = unit_sphere_to_lat_lon(north_pole);
        let elev_n = generator.sample_elevation(north_pole, lat_n, lon_n);
        let biome_n = generator.determine_biome(lat_n, elev_n, north_pole);

        assert_eq!(
            biome_n,
            BiomeType::SnowPeaks,
            "Le Pôle Nord doit obligatoirement être une calotte polaire (SnowPeaks)"
        );

        let south_pole = DVec3::new(0.0, -1.0, 0.0);
        let (lat_s, lon_s) = unit_sphere_to_lat_lon(south_pole);
        let elev_s = generator.sample_elevation(south_pole, lat_s, lon_s);
        let biome_s = generator.determine_biome(lat_s, elev_s, south_pole);

        assert_eq!(
            biome_s,
            BiomeType::SnowPeaks,
            "Le Pôle Sud doit obligatoirement être une calotte polaire (SnowPeaks)"
        );
    }

    #[test]
    fn test_ocean_coverage_is_at_least_40_percent() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());

        let sample_count = 2500;
        let mut water_points = 0;
        let mut sea_points = 0;
        let mut deep_ocean_points = 0;
        let mut land_points = 0;
        let mut polar_points = 0;

        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;

        for i in 0..sample_count {
            let y = 1.0 - (i as f64 / (sample_count - 1) as f64) * 2.0;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;

            let dir = DVec3::new(radius * theta.cos(), y, radius * theta.sin()).normalize();
            let (lat, lon) = unit_sphere_to_lat_lon(dir);

            let elevation = generator.sample_elevation(dir, lat, lon);
            let biome = generator.determine_biome(lat, elevation, dir);

            if biome.is_water() {
                water_points += 1;
                if biome == BiomeType::Sea {
                    sea_points += 1;
                } else if biome == BiomeType::DeepOcean {
                    deep_ocean_points += 1;
                }
            } else if biome == BiomeType::SnowPeaks && lat.abs() > 1.10 {
                polar_points += 1;
            } else {
                land_points += 1;
            }
        }

        let water_percentage = (water_points as f64 / sample_count as f64) * 100.0;
        println!(
            "Statistiques planétaires : Eau={:.1}% (Océans={}, Mers={}), Pôles={}, Terres émergées={}",
            water_percentage, deep_ocean_points, sea_points, polar_points, land_points
        );

        assert!(
            water_percentage >= 40.0,
            "La couverture maritime ({:.1}%) doit être d'au moins 40%",
            water_percentage
        );
        assert!(sea_points > 0, "Il doit y avoir des mers");
        assert!(deep_ocean_points > 0, "Il doit y avoir des océans profonds");
        assert!(land_points > 0, "Il doit y avoir des continents massifs");
        assert!(polar_points > 0, "Il doit y avoir des calottes polaires aux pôles");
    }
}

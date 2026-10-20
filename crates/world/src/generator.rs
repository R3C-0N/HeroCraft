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

/// Niveau de la mer de référence en blocs
pub const SEA_LEVEL_BLOCKS: f64 = 100.0;

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

#[derive(Debug, Clone)]
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

        let layer_base_y = coord.altitude_layer as f64 * CHUNK_SIZE as f64;

        let sea_level_y = (SEA_LEVEL_BLOCKS - layer_base_y)
            .clamp(0.0, CHUNK_SIZE as f64) as u8;

        for z in 0..CHUNK_SIZE as u8 {
            let v = base_v + (z as f64) * voxel_span_uv;
            for x in 0..CHUNK_SIZE as u8 {
                let u = base_u + (x as f64) * voxel_span_uv;

                let sphere_dir = coord.face.uv_to_sphere_direction(u, v);
                let (lat, lon) = unit_sphere_to_lat_lon(sphere_dir);

                let elevation = self.sample_elevation(sphere_dir, lat, lon);
                let biome = self.determine_biome(lat, elevation, sphere_dir);

                let ground_y = ((elevation - layer_base_y)
                    .clamp(0.0, CHUNK_SIZE as f64)) as u8;

                // 1. Remplissage du sol solide
                let elev_floor = elevation.floor();
                for y in 0..ground_y {
                    let global_y = layer_base_y + y as f64;
                    let is_surface = global_y + 1.0 >= elev_floor;

                    let block = if is_surface {
                        // Enneigement strict au-dessus de 300 blocs
                        if global_y >= 300.0 {
                            BlockId::SNOW
                        } else {
                            match biome {
                                BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea => BlockId::SAND,
                                BiomeType::Beach | BiomeType::Desert => BlockId::SAND,
                                BiomeType::SnowPeaks => BlockId::SNOW,
                                BiomeType::Mountains => {
                                    // Versants rocailleux des montagnes :
                                    // - Au-dessus de 220 : crêtes et parois de roche massive (BlockId::STONE)
                                    // - Entre 170 et 220 : alternance réaliste d'affleurements rocheux et d'alpages
                                    let rock_noise = fbm_3d(sphere_dir * 16.0, 2, 2.0, 0.5, self.planet.seed.wrapping_add(40404));
                                    if global_y > 220.0 || rock_noise > -0.05 {
                                        BlockId::STONE
                                    } else {
                                        BlockId::GRASS
                                    }
                                }
                                _ => BlockId::GRASS,
                            }
                        }
                    } else if global_y >= 295.0 {
                        // Socle rocheux sous les neiges d'altitude
                        BlockId::STONE
                    } else if global_y + 4.0 >= elev_floor {
                        if biome == BiomeType::Mountains {
                            let rock_noise = fbm_3d(sphere_dir * 16.0, 2, 2.0, 0.5, self.planet.seed.wrapping_add(40404));
                            if global_y > 215.0 || rock_noise > -0.05 {
                                BlockId::STONE
                            } else {
                                BlockId::DIRT
                            }
                        } else if biome == BiomeType::SnowPeaks {
                            BlockId::STONE
                        } else if biome == BiomeType::Desert || biome == BiomeType::Beach {
                            BlockId::SAND
                        } else {
                            BlockId::DIRT
                        }
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
                // Banquise affleurante parfaitement alignée sur la sphère océanique
                SEA_LEVEL_BLOCKS + 1.0
            } else {
                // Fond marin sous le niveau de la mer (avec fosses ou dorsales médio-océaniques)
                let depth_ratio = (sea_threshold - continental) / (1.0 + sea_threshold);
                (SEA_LEVEL_BLOCKS - depth_ratio * 65.0 + tectonic_sample.elevation_offset * 3.5)
                    .clamp(15.0, SEA_LEVEL_BLOCKS - 2.0)
            }
        } else {
            // Terre émergée : continents, collines, montagnes intérieures ET super-chaînes tectoniques
            let seed = self.planet.seed;

            // Pénétration dans les terres : 0 sur le rivage, 1 à l'intérieur du continent.
            // La valeur continentale plafonne vers 0.40 : la normaliser sur [0, 1] écrasait tout le relief d'un facteur ~5.
            let inland = smoothstep(0.0, 0.20, continental - sea_threshold);

            // Dôme glaciaire naturel sur la terre polaire
            let polar_dome = if is_polar {
                (lat.abs() - polar_boundary).max(0.0) * 40.0
            } else {
                0.0
            };

            // 1. Massifs montagneux : régions d'altitude basse fréquence (environ un tiers des terres)
            let massif_noise = fbm_3d(dir * 1.7, 3, 2.0, 0.50, seed.wrapping_add(10203)) / FBM_SPAN;
            let massif = smoothstep(-0.10, 0.45, massif_noise) * inland;

            // Crêtes : bruit ridged normalisé pour des pics aigus séparés par des vallées
            let ridge = ridged_fbm(dir * 4.5, 4, seed.wrapping_add(10101));
            let standard_mountains = ridge * (35.0 + 165.0 * massif) * massif;

            // 2. Plateaux continentaux et collines douces
            let plateau = inland * 22.0 + massif * 30.0;
            let hills = fbm_3d(dir * 10.0, 3, 2.0, 0.50, seed.wrapping_add(20202)) / FBM_SPAN * 9.0 * inland;

            // 3. SUPER-CHAÎNES TECTONIQUES : Orogenèse colossale le long des collisions de plaques (Himalaya, Andes)
            let super_chain = if tectonic_sample.orogeny_strength > 0.02 {
                let super_ridge = ridged_fbm(dir * 6.0, 4, seed.wrapping_add(88888));
                let strength = smoothstep(0.02, 0.6, tectonic_sample.orogeny_strength);
                (0.35 + super_ridge * 0.65) * 190.0 * strength * inland
            } else {
                0.0
            };

            (SEA_LEVEL_BLOCKS + 1.0 + plateau + hills + standard_mountains.max(super_chain) + polar_dome)
                .clamp(SEA_LEVEL_BLOCKS + 1.0, 350.0)
        }
    }

    /// Détermine le biome selon la latitude, l'altitude et la valeur continentale
    pub fn determine_biome(&self, lat: f64, elevation: f64, dir: DVec3) -> BiomeType {
        // 1. Calottes glaciaires et banquises permanentes aux pôles (Pôle Nord et Pôle Sud)
        // Calibrées à ~73° de latitude pour un équilibre parfait entre vue de profil et vue du dessus
        let polar_noise = fbm_3d(dir * 3.2, 2, 2.0, 0.50, self.planet.seed.wrapping_add(3333)) * 0.08;
        let polar_boundary = 1.28 + polar_noise;
        let is_polar = lat.abs() > polar_boundary;

        if is_polar || elevation >= 300.0 {
            return BiomeType::SnowPeaks;
        }

        let continental = self.sample_continental_value(dir);
        let sea_threshold = 0.01;

        // 2. Biomes maritimes (Océans et Mers)
        if (elevation <= SEA_LEVEL_BLOCKS || continental < sea_threshold) && elevation < 110.0 {
            if continental < -0.20 || elevation < 50.0 {
                BiomeType::DeepOcean // Plaines et fosses océaniques profondes
            } else if continental < -0.05 {
                BiomeType::Ocean // Grands océans ouverts
            } else {
                BiomeType::Sea // Mers intérieures, détroits et golfes
            }
        } else if continental < sea_threshold + 0.03 {
            BiomeType::Beach // Rivages de sable côtier
        } else {
            // 3. Biomes continentaux
            let solar_temp = base_solar_temperature(lat);
            let altitude_cooling = (elevation - SEA_LEVEL_BLOCKS) / 250.0;
            let temp = (solar_temp - altitude_cooling).clamp(0.0, 1.0);

            // Humidité procédurale
            let moisture = fbm_3d(dir * 2.2, 3, 2.0, 0.50, self.planet.seed.wrapping_add(77777)) * 0.5 + 0.5;

            // Enneigement strict au-dessus de 300 blocs
            if elevation >= 300.0 {
                BiomeType::SnowPeaks // Sommets enneigés et neiges éternelles
            } else if elevation > 170.0 {
                BiomeType::Mountains // Versants et massifs montagneux
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

/// Amplitude typique d'un `fbm_3d` (valeurs observées quasi toujours dans ±0.35) :
/// sert à ramener le bruit vers [-1, 1] avant de l'utiliser comme masque ou relief.
const FBM_SPAN: f64 = 0.35;

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bruit "ridged" multifractal dans [0, 1] : crêtes fines et pointues (1.0), larges vallées (0.0)
fn ridged_fbm(p: DVec3, octaves: usize, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut max_amp = 0.0;
    let mut weight = 1.0;

    for i in 0..octaves {
        let n = gradient_noise_3d(p * freq, seed.wrapping_add((i as u64).wrapping_mul(1013)));
        let r = (1.0 - (n.abs() / 0.5).min(1.0)).powi(2) * weight;
        weight = (r * 1.5).clamp(0.0, 1.0);
        sum += r * amp;
        max_amp += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    sum / max_amp
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
        let coord_base = PlanetChunkCoord::new(PlanetFace::PosX, 0, 0, 0);
        let chunk_base = generator.generate_chunk(&coord_base);
        assert!(!chunk_base.get_voxel(0, 0, 0).is_air());

        let coord_sky = PlanetChunkCoord::new(PlanetFace::PosX, 0, 0, 16);
        let chunk_sky = generator.generate_chunk(&coord_sky);
        assert!(chunk_sky.get_voxel(0, 31, 0).is_air());
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

    #[test]
    fn test_planets_diversity_across_seeds() {
        let mut desc1 = PlanetDescriptor::default();
        desc1.seed = 1001;
        let gen1 = PlanetWorldGenerator::new(desc1);

        let mut desc2 = PlanetDescriptor::default();
        desc2.seed = 2026;
        let gen2 = PlanetWorldGenerator::new(desc2);

        let mut diff_water = 0;
        let sample_count = 500;
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;

        for i in 0..sample_count {
            let y = 1.0 - (i as f64 / (sample_count - 1) as f64) * 2.0;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;
            let dir = DVec3::new(radius * theta.cos(), y, radius * theta.sin()).normalize();
            let (lat, lon) = unit_sphere_to_lat_lon(dir);

            let elev1 = gen1.sample_elevation(dir, lat, lon);
            let biome1 = gen1.determine_biome(lat, elev1, dir);

            let elev2 = gen2.sample_elevation(dir, lat, lon);
            let biome2 = gen2.determine_biome(lat, elev2, dir);

            if biome1.is_water() != biome2.is_water() {
                diff_water += 1;
            }
        }

        let diff_ratio = diff_water as f64 / sample_count as f64;
        println!("Différence Terre/Mer entre deux graines différentes : {:.1}%", diff_ratio * 100.0);
        assert!(
            diff_ratio > 0.25,
            "Deux graines différentes doivent générer des cartes fondamentalement distinctes (différence constatée : {:.1}%)",
            diff_ratio * 100.0
        );
    }

    #[test]
    fn test_mountains_reach_high_altitude() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());

        let sample_count = 20000;
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let mut land = 0;
        let mut mountains = 0;
        let mut highest: f64 = 0.0;

        for i in 0..sample_count {
            let y = 1.0 - (i as f64 / (sample_count - 1) as f64) * 2.0;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;
            let dir = DVec3::new(radius * theta.cos(), y, radius * theta.sin()).normalize();
            let (lat, lon) = unit_sphere_to_lat_lon(dir);
            if lat.abs() > 1.2 {
                continue;
            }

            let elevation = generator.sample_elevation(dir, lat, lon);
            if elevation > SEA_LEVEL_BLOCKS {
                land += 1;
                if elevation > 170.0 {
                    mountains += 1;
                }
                highest = highest.max(elevation);
            }
        }

        let mountain_ratio = mountains as f64 / land as f64;
        println!("Relief : sommet={:.0} blocs, terres > 170 = {:.1}%", highest, mountain_ratio * 100.0);
        assert!(highest > 280.0, "Les plus hauts sommets doivent dépasser 280 blocs (constaté : {:.0})", highest);
        assert!(
            mountain_ratio > 0.08,
            "Au moins 8% des terres doivent être montagneuses (constaté : {:.1}%)",
            mountain_ratio * 100.0
        );
    }

    #[test]
    fn test_snow_peaks_above_300_and_mountains() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());
        // Trouver un point continental tempéré (non-polaire)
        let mut land_dir = DVec3::new(1.0, 0.0, 0.0);
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        for i in 0..100 {
            let y = 1.0 - (i as f64 / 99.0) * 2.0;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;
            let dir = DVec3::new(radius * theta.cos(), y, radius * theta.sin()).normalize();
            let (lat, _) = unit_sphere_to_lat_lon(dir);
            if lat.abs() < 0.8 && generator.sample_continental_value(dir) > 0.10 {
                land_dir = dir;
                break;
            }
        }
        let (lat, _) = unit_sphere_to_lat_lon(land_dir);

        // Au-dessus de 300 : obligatoire SnowPeaks (neiges éternelles)
        let biome_snow = generator.determine_biome(lat, 310.0, land_dir);
        assert_eq!(biome_snow, BiomeType::SnowPeaks);

        // Entre 180 et 290 sur un continent : biome Montagnes
        let biome_mountain = generator.determine_biome(lat, 250.0, land_dir);
        assert_eq!(biome_mountain, BiomeType::Mountains);
    }
}

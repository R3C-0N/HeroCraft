use crate::caves::{block_carved, carvable_range, CaveField};
use crate::cubed_sphere::{base_solar_temperature, unit_sphere_to_lat_lon, PlanetDescriptor};
use crate::ocean_currents::OceanicCurrentSimulation;
use crate::tectonics::TectonicSimulation;
use glam::DVec3;
use herocraft_common::{BlockId, PlanetChunkCoord, PlanetFace};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Toundra : plaines froides et rases des hautes latitudes
    Tundra,
    /// Taïga : forêts de conifères des régions froides et humides
    Taiga,
    /// Savane : herbes sèches des régions chaudes
    Savanna,
    /// Jungle : forêts tropicales chaudes et très humides
    Jungle,
}

impl BiomeType {
    pub fn is_water(&self) -> bool {
        matches!(self, BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea)
    }
}

/// Échantillon de terrain d'une colonne de voxels (indépendant de la couche d'altitude)
#[derive(Debug, Clone, Copy)]
pub struct ColumnSample {
    pub dir: DVec3,
    pub lat: f64,
    pub elevation: f64,
    pub biome: BiomeType,
}

#[derive(Debug, Clone)]
pub struct PlanetWorldGenerator {
    pub planet: PlanetDescriptor,
    pub tectonics: TectonicSimulation,
    pub ocean_currents: OceanicCurrentSimulation,
}

impl PlanetWorldGenerator {
    pub fn new(planet: PlanetDescriptor) -> Self {
        herocraft_common::prof_scope!("world::PlanetWorldGenerator::new");
        let tectonics = TectonicSimulation::new(planet.seed);
        let ocean_currents = OceanicCurrentSimulation::new(planet.seed);
        Self {
            planet,
            tectonics,
            ocean_currents,
        }
    }

    /// Renvoie le rayon de base effectif de la planète
    #[inline]
    pub fn base_radius(&self) -> f64 {
        self.planet.base_radius()
    }

    /// Renvoie le facteur d'échelle global de la planète
    #[inline]
    pub fn scale(&self) -> f64 {
        self.planet.scale
    }

    /// Génère un chunk voxel complet sur une face de la planète
    pub fn generate_chunk(&self, coord: &PlanetChunkCoord) -> ChunkData {
        let columns = self.sample_chunk_columns(coord.face, coord.x, coord.y, 0);
        self.generate_chunk_from_columns(coord, &columns, 0)
    }

    /// Échantillonne l'élévation et le biome d'une colonne de voxels
    pub fn sample_column(&self, face: PlanetFace, u: f64, v: f64) -> ColumnSample {
        let dir = face.uv_to_sphere_direction(u, v);
        let (lat, _lon) = unit_sphere_to_lat_lon(dir);
        let (elevation, biome) = self.sample_elevation_and_biome(dir, lat);
        ColumnSample { dir, lat, elevation, biome }
    }

    /// Échantillonne les colonnes d'un chunk avec une bordure de `border` colonnes de chaque côté.
    /// Grille de côté `CHUNK_SIZE + 2 * border`, indexée par `(z + border) * côté + (x + border)`.
    /// Ces colonnes sont communes à toutes les couches d'altitude du chunk.
    pub fn sample_chunk_columns(&self, face: PlanetFace, chunk_x: i32, chunk_y: i32, border: i32) -> Vec<ColumnSample> {
        herocraft_common::prof_scope!("world::sample_chunk_columns");
        let chunk_span_uv = 2.0 / 32.0;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);
        let base_u = -1.0 + (chunk_x as f64) * chunk_span_uv;
        let base_v = -1.0 + (chunk_y as f64) * chunk_span_uv;
        let side = CHUNK_SIZE as i32 + 2 * border;
        let mut columns = Vec::with_capacity((side * side) as usize);
        for z in -border..CHUNK_SIZE as i32 + border {
            let v = base_v + (z as f64) * voxel_span_uv;
            for x in -border..CHUNK_SIZE as i32 + border {
                let u = base_u + (x as f64) * voxel_span_uv;
                columns.push(self.sample_column(face, u, v));
            }
        }
        columns
    }

    /// Bloc d'une cellule solide (sous le sol) : surface selon le biome, couche de terre, roche
    fn solid_block(&self, sphere_dir: DVec3, biome: BiomeType, global_y: f64, elev_floor: f64) -> BlockId {
        let is_surface = global_y + 1.0 >= elev_floor;
        if is_surface {
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
        }
    }

    /// Bloc généré en (x, y, z) d'une face, `column` étant sa colonne : exactement celui que pose
    /// `generate_chunk_from_columns`, sans générer le chunk entier
    pub fn column_block(&self, column: &ColumnSample, face: PlanetFace, x: i32, y: i32, z: i32) -> BlockId {
        if y < 0 {
            return BlockId::STONE;
        }
        let global_y = y as f64;
        let elev_floor = column.elevation.floor();
        if global_y < elev_floor {
            if block_carved(self.planet.seed, face, column, x, y, z) {
                BlockId::AIR
            } else {
                self.solid_block(column.dir, column.biome, global_y, elev_floor)
            }
        } else if global_y < SEA_LEVEL_BLOCKS {
            if column.biome == BiomeType::SnowPeaks {
                BlockId::SNOW
            } else {
                BlockId::WATER
            }
        } else {
            BlockId::AIR
        }
    }

    /// Le bloc plein (x, y, z) d'une face est creusé par une grotte (`column` étant sa colonne)
    pub fn is_cave(&self, column: &ColumnSample, face: PlanetFace, x: i32, y: i32, z: i32) -> bool {
        block_carved(self.planet.seed, face, column, x, y, z)
    }

    /// Bloc généré en (x, y, z) d'une face : x le long de u, z le long de v (0..1024), y l'altitude
    pub fn block_at(&self, face: PlanetFace, x: i32, y: i32, z: i32) -> BlockId {
        self.column_block(&self.sample_block_column(face, x, z), face, x, y, z)
    }

    /// Champ de grottes d'un chunk et de `border` blocs autour (colonnes de `sample_chunk_columns`
    /// avec la même bordure), réduit aux altitudes où ses colonnes peuvent en avoir. `None` si la
    /// couche n'en a aucune.
    pub fn chunk_cave_field(&self, coord: &PlanetChunkCoord, columns: &[ColumnSample], border: i32) -> Option<CaveField> {
        let layer_base = coord.altitude_layer * CHUNK_SIZE as i32;
        let (mut low, mut high) = (i32::MAX, i32::MIN);
        for column in columns {
            let range = carvable_range(column);
            if !range.is_empty() {
                low = low.min(range.start);
                high = high.max(range.end);
            }
        }
        let low = low.max(layer_base - border);
        let high = high.min(layer_base + CHUNK_SIZE as i32 + border);
        if low >= high {
            return None;
        }
        let size = CHUNK_SIZE as i32;
        let (x0, z0) = (coord.x * size, coord.y * size);
        Some(CaveField::new(
            self.planet.seed,
            coord.face,
            x0 - border..x0 + size + border,
            low..high,
            z0 - border..z0 + size + border,
        ))
    }

    /// Colonne de la grille voxel (x, z) d'une face, échantillonnée au même point que `sample_chunk_columns`
    pub fn sample_block_column(&self, face: PlanetFace, x: i32, z: i32) -> ColumnSample {
        let voxel_span_uv = 2.0 / (32.0 * CHUNK_SIZE as f64);
        self.sample_column(face, -1.0 + x as f64 * voxel_span_uv, -1.0 + z as f64 * voxel_span_uv)
    }

    /// Génère un chunk voxel à partir de colonnes précalculées par `sample_chunk_columns`
    pub fn generate_chunk_from_columns(&self, coord: &PlanetChunkCoord, columns: &[ColumnSample], border: i32) -> ChunkData {
        let caves = self.chunk_cave_field(coord, columns, border);
        self.generate_chunk_with_caves(coord, columns, border, caves.as_ref())
    }

    /// `generate_chunk_from_columns` avec le champ de grottes déjà calculé par `chunk_cave_field`
    pub fn generate_chunk_with_caves(
        &self,
        coord: &PlanetChunkCoord,
        columns: &[ColumnSample],
        border: i32,
        caves: Option<&CaveField>,
    ) -> ChunkData {
        herocraft_common::prof_scope!("world::generate_chunk");
        let mut chunk = ChunkData::new_empty();
        let side = CHUNK_SIZE + 2 * border as usize;

        let layer_base_y = coord.altitude_layer as f64 * CHUNK_SIZE as f64;

        let sea_level_y = (SEA_LEVEL_BLOCKS - layer_base_y)
            .clamp(0.0, CHUNK_SIZE as f64) as u8;

        for z in 0..CHUNK_SIZE as u8 {
            for x in 0..CHUNK_SIZE as u8 {
                let column = &columns[(z as usize + border as usize) * side + x as usize + border as usize];
                let sphere_dir = column.dir;
                let elevation = column.elevation;
                let biome = column.biome;

                let ground_y = ((elevation - layer_base_y)
                    .clamp(0.0, CHUNK_SIZE as f64)) as u8;

                // 1. Remplissage du sol solide, sauf les blocs creusés par les grottes
                let elev_floor = elevation.floor();
                let block_x = coord.x * CHUNK_SIZE as i32 + x as i32;
                let block_z = coord.y * CHUNK_SIZE as i32 + z as i32;
                for y in 0..ground_y {
                    let global_y = layer_base_y + y as f64;
                    chunk.set_voxel(x, y, z, self.solid_block(sphere_dir, biome, global_y, elev_floor));
                }
                if let Some(caves) = caves {
                    let base = layer_base_y as i32;
                    caves.for_each_carved(column, block_x, block_z, base..base + ground_y as i32, |y| {
                        chunk.set_voxel(x, (y - base) as u8, z, BlockId::AIR);
                    });
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
        herocraft_common::prof_scope!("world::sample_continental_value");
        let seed = self.planet.seed;

        // 1. Déformation de domaine multi-échelle : casse la géométrie sphérique des plaques
        // (grandes baies et péninsules à basse fréquence, découpes plus fines par-dessus)
        let warp = DVec3::new(
            fbm_3d(dir * 1.6, 4, 2.1, 0.50, seed.wrapping_add(101)),
            fbm_3d(dir * 1.6, 4, 2.1, 0.50, seed.wrapping_add(202)),
            fbm_3d(dir * 1.6, 4, 2.1, 0.50, seed.wrapping_add(303)),
        );
        let warped_dir = (dir + warp * 0.45).normalize();

        // 2. Ancrage géologique sur les plaques tectoniques : les plaques continentales portent les continents,
        // les plaques océaniques forment les bassins. La fraction de croûte est lissée entre plaques pour que
        // le rivage ne recopie pas la frontière du Voronoi pondéré (arcs de cercle et pointes aux jonctions).
        let crust = self.tectonics.continental_crust(warped_dir, 0.18);
        let plate_bias = (crust - 0.55) * 0.50;

        // Bruit fractal continental, de même poids que les plaques : c'est lui qui dessine le trait de côte
        let macro_noise = fbm_3d(warped_dir * 1.3, 5, 2.0, 0.50, seed);
        let macro_continent = plate_bias + macro_noise * 1.15;

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
        self.sample_elevation_and_continental(dir, lat).0
    }

    /// Élévation et biome d'un point, en ne calculant la valeur continentale qu'une seule fois
    pub fn sample_elevation_and_biome(&self, dir: DVec3, lat: f64) -> (f64, BiomeType) {
        let (elevation, continental) = self.sample_elevation_and_continental(dir, lat);
        (elevation, self.determine_biome_with_continental(lat, elevation, dir, || continental))
    }

    /// Élévation et valeur continentale (réutilisable par `determine_biome`)
    fn sample_elevation_and_continental(&self, dir: DVec3, lat: f64) -> (f64, f64) {
        herocraft_common::prof_scope!("world::sample_elevation");
        let continental = self.sample_continental_value(dir);
        let sea_threshold = 0.01;
        let tectonic_sample = self.tectonics.sample(dir);

        // Calotte polaire resserrée (~73° de latitude) avec découpage fractal naturel
        let polar_noise = fbm_3d(dir * 3.2, 2, 2.0, 0.50, self.planet.seed.wrapping_add(3333)) * 0.08;
        let polar_boundary = 1.28 + polar_noise;
        let is_polar = lat.abs() > polar_boundary;

        let elevation = if continental < sea_threshold {
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
        };
        (elevation, continental)
    }

    /// Détermine le biome selon la latitude, l'altitude et la valeur continentale
    pub fn determine_biome(&self, lat: f64, elevation: f64, dir: DVec3) -> BiomeType {
        self.determine_biome_with_continental(lat, elevation, dir, || self.sample_continental_value(dir))
    }

    /// `determine_biome` avec une valeur continentale fournie (calculée seulement si nécessaire)
    fn determine_biome_with_continental(
        &self,
        lat: f64,
        elevation: f64,
        dir: DVec3,
        continental: impl FnOnce() -> f64,
    ) -> BiomeType {
        herocraft_common::prof_scope!("world::determine_biome");
        // 1. Calottes glaciaires et banquises permanentes aux pôles (Pôle Nord et Pôle Sud)
        // Calibrées à ~73° de latitude pour un équilibre parfait entre vue de profil et vue du dessus
        let polar_noise = fbm_3d(dir * 3.2, 2, 2.0, 0.50, self.planet.seed.wrapping_add(3333)) * 0.08;
        let polar_boundary = 1.28 + polar_noise;
        let is_polar = lat.abs() > polar_boundary;

        if is_polar || elevation >= 300.0 {
            return BiomeType::SnowPeaks;
        }

        let continental = continental();
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

            // Humidité procédurale, ramenée sur [0, 1] (le bruit brut reste dans ±FBM_SPAN)
            let moisture_noise = fbm_3d(dir * 2.2, 3, 2.0, 0.50, self.planet.seed.wrapping_add(77777));
            let moisture = ((moisture_noise / FBM_SPAN).clamp(-1.0, 1.0) * 0.5 + 0.5).clamp(0.0, 1.0);

            // Enneigement strict au-dessus de 300 blocs
            if elevation >= 300.0 {
                BiomeType::SnowPeaks // Sommets enneigés et neiges éternelles
            } else if elevation > 170.0 {
                BiomeType::Mountains // Versants et massifs montagneux
            } else if temp < 0.50 {
                // Régions froides : forêts de conifères si l'air est humide, toundra rase sinon
                if moisture > 0.52 {
                    BiomeType::Taiga
                } else {
                    BiomeType::Tundra
                }
            } else if temp > 0.78 {
                // Régions chaudes, des déserts aux jungles selon l'humidité
                if moisture < 0.30 {
                    BiomeType::Desert
                } else if moisture < 0.47 {
                    BiomeType::Savanna
                } else if moisture > 0.60 {
                    BiomeType::Jungle
                } else {
                    BiomeType::Forest
                }
            } else if moisture < 0.24 && temp > 0.65 {
                BiomeType::Desert // Déserts continentaux
            } else if moisture > 0.56 {
                BiomeType::Forest // Forêts tempérées
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
    fn test_climate_biomes_all_present() {
        // Toundra et taïga dans le froid, savane et jungle dans le chaud, sans écraser les biomes d'origine
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());
        let sample_count = 20000;
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let mut counts = std::collections::HashMap::new();
        let mut land = 0;
        for i in 0..sample_count {
            let y = 1.0 - (i as f64 / (sample_count - 1) as f64) * 2.0;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / phi;
            let dir = DVec3::new(radius * theta.cos(), y, radius * theta.sin()).normalize();
            let (lat, _) = unit_sphere_to_lat_lon(dir);
            let (_, biome) = generator.sample_elevation_and_biome(dir, lat);
            if !biome.is_water() {
                land += 1;
                *counts.entry(biome).or_insert(0usize) += 1;
                if biome == BiomeType::Tundra || biome == BiomeType::Taiga {
                    assert!(lat.abs() > 0.5, "biome froid à {:.0}° de latitude", lat.to_degrees());
                }
                if biome == BiomeType::Jungle || biome == BiomeType::Savanna {
                    assert!(lat.abs() < 0.75, "biome chaud à {:.0}° de latitude", lat.to_degrees());
                }
            }
        }
        for biome in [
            BiomeType::Plains,
            BiomeType::Forest,
            BiomeType::Desert,
            BiomeType::Tundra,
            BiomeType::Taiga,
            BiomeType::Savanna,
            BiomeType::Jungle,
        ] {
            let share = *counts.get(&biome).unwrap_or(&0) as f64 / land as f64;
            assert!(share > 0.015, "{:?} ne couvre que {:.1}% des terres", biome, share * 100.0);
        }
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

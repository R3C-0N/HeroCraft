//! Météo vue par le joueur : le temps qu'il fait là où il est (`herocraft_world::weather`), lissé
//! pour que les changements se fassent en douceur, les éclairs et le tonnerre des orages, la
//! neige qui tient au sol et le sol mouillé, les couleurs des saisons, et ce que le rendu en tire
//! (lumière voilée, nuages, colonnes où tombe la pluie).

use glam::Vec3;
use herocraft_world::weather::{year_phase, SNOW_TEMPERATURE};
use herocraft_world::{BlockGeometry, PlanetWorldGenerator, WeatherModel, WeatherSample};

use crate::audio::synth::Noise;
use crate::collision::VoxelCollider;
use crate::precipitation::{self, Column};
use crate::ui::LightingSettings;

/// Temps entre deux lectures du temps qu'il fait au joueur, en secondes
const SAMPLE_PERIOD: f32 = 0.5;
/// Constante de temps du lissage des changements de temps, en secondes
const SMOOTHING: f32 = 6.0;
/// Durée de l'éclair, en secondes
const FLASH_TIME: f32 = 0.35;

/// Ce que le rendu et le son tirent de la météo, à chaque image
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeatherVisual {
    /// Couverture nuageuse, de 0 à 1
    pub cloud_cover: f32,
    /// Force de la pluie ou de la neige, de 0 à 1
    pub precipitation: f32,
    /// Part de neige dans ce qui tombe, de 0 à 1
    pub snow: f32,
    /// Force de l'orage, de 0 à 1
    pub storm: f32,
    /// Vent, de 0 à 1
    pub wind: f32,
    /// Sol mouillé, de 0 à 1 (monte sous la pluie, sèche ensuite)
    pub wetness: f32,
    /// Neige tombée sur le dessus des blocs, de 0 à 1 (monte sous la neige, fond au chaud)
    pub snow_cover: f32,
    /// Éclair en cours, de 0 à 1
    pub flash: f32,
    /// Saisons pour les feuillages et l'herbe : (automne, hiver) au nord puis au sud, de 0 à 1
    pub season: [f32; 4],
}

impl WeatherVisual {
    /// Ciel voilé : assombrit et grise le ciel, de 0 à 1
    pub fn overcast(&self) -> f32 {
        ((self.cloud_cover - 0.55).max(0.0) * 0.9 + self.precipitation * 0.45 + self.storm * 0.3).clamp(0.0, 1.0)
    }

    /// Pour l'uniforme caméra (common.wgsl, `camera.weather`) : voile, sol mouillé, neige au sol,
    /// éclair
    pub fn weather_uniform(&self) -> [f32; 4] {
        [self.overcast(), self.wetness, self.snow_cover, self.flash]
    }

    /// Éclairage sous ce temps : soleil voilé par les nuages, brume de l'averse, ciel éclairé par
    /// l'éclair. La brume entre dans les tables de l'atmosphère : elle change par paliers, pour ne
    /// pas les recalculer à chaque image.
    pub fn lighting(&self, base: &LightingSettings) -> LightingSettings {
        let overcast = self.overcast();
        let mut lighting = *base;
        lighting.sun_intensity *= 1.0 - 0.72 * overcast;
        lighting.day_ambient *= (1.0 - 0.35 * overcast) * (1.0 + 2.5 * self.flash);
        let haze = 1.0 + 2.0 * self.precipitation + 1.0 * self.snow * self.precipitation;
        lighting.fog_intensity *= (haze * 4.0).round() / 4.0;
        lighting
    }
}

/// Valeurs des saisons pour le shader, d'après le moment de l'année : rougeur d'automne et
/// dépouillement d'hiver des feuillus, au nord puis au sud
pub fn season_uniform(time: f64) -> [f32; 4] {
    let tint = |phase: f64| -> (f32, f32) {
        let s = |a: f64, b: f64, x: f64| {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        // Printemps 0 à 0,25, été à 0,5, automne à 0,75, hiver jusqu'à 1
        let autumn = s(0.47, 0.58, phase) * (1.0 - s(0.72, 0.80, phase));
        let winter = s(0.72, 0.80, phase).max(1.0 - s(0.0, 0.07, phase));
        (autumn as f32, winter as f32)
    };
    let phase = year_phase(time);
    let (na, nw) = tint(phase);
    let (sa, sw) = tint((phase + 0.5).rem_euclid(1.0));
    [na, nw, sa, sw]
}

/// Météo du joueur
pub struct ClientWeather {
    pub model: WeatherModel,
    /// Dernier temps lu au joueur
    pub sample: Option<WeatherSample>,
    visual: WeatherVisual,
    sample_in: f32,
    /// Temps avant le prochain éclair, en secondes
    next_flash: f32,
    flash_age: f32,
    /// Coups de tonnerre à venir : (délai, force)
    thunder: Vec<(f32, f32)>,
    rng: Noise,
    /// Le temps réel écoulé (mouvement de la pluie)
    pub clock: f32,
}

impl ClientWeather {
    pub fn new(seed: u64) -> Self {
        Self {
            model: WeatherModel::new(seed),
            sample: None,
            visual: WeatherVisual::default(),
            sample_in: 0.0,
            next_flash: 5.0,
            flash_age: FLASH_TIME,
            thunder: Vec::new(),
            rng: Noise::new(seed as u32 ^ 0x7E47),
            clock: 0.0,
        }
    }

    /// Nouvelle planète : la météo de sa graine, sans transition
    pub fn reset(&mut self, seed: u64) {
        let forced = self.model.forced;
        *self = Self::new(seed);
        self.model.forced = forced;
    }

    pub fn visual(&self) -> &WeatherVisual {
        &self.visual
    }

    /// Avance d'une image : `time` est le temps du ciel, `feet` les pieds du joueur. Au sol
    /// seulement (`ground`) : en orbite, ni pluie ni éclairs.
    pub fn update(&mut self, dt: f32, time: f64, feet: Vec3, base_radius: f32, ground: bool) {
        self.clock += dt;
        self.sample_in -= dt;
        let first = self.sample.is_none();
        if self.sample_in <= 0.0 || first {
            self.sample_in = SAMPLE_PERIOD;
            let block = BlockGeometry::new(base_radius as f64).block_height;
            let altitude = (feet.length() as f64 - base_radius as f64) / block - herocraft_world::generator::SEA_LEVEL_BLOCKS;
            self.sample = Some(self.model.sample(feet.as_dvec3(), altitude, time));
        }
        let Some(sample) = self.sample else { return };
        let v = &mut self.visual;
        let k = if first { 1.0 } else { 1.0 - (-dt / SMOOTHING).exp() };
        let toward = |value: &mut f32, target: f32| *value += (target - *value) * k;
        toward(&mut v.cloud_cover, sample.cloud_cover);
        toward(&mut v.precipitation, if ground { sample.precipitation } else { 0.0 });
        toward(&mut v.snow, if sample.snow { 1.0 } else { 0.0 });
        toward(&mut v.storm, sample.storm);
        toward(&mut v.wind, sample.wind);

        // Sol mouillé et neige au sol : ils viennent avec l'averse et partent lentement après
        let raining = sample.precipitation * (1.0 - v.snow);
        let snowing = sample.precipitation * v.snow;
        let thawing = sample.temperature > SNOW_TEMPERATURE + 0.03;
        if first {
            v.wetness = (raining > 0.05) as u8 as f32;
            v.snow_cover = (snowing > 0.05 || (sample.temperature < SNOW_TEMPERATURE - 0.05 && sample.cloud_cover > 0.5)) as u8 as f32;
        } else {
            v.wetness = (v.wetness + dt * (raining / 15.0 - 1.0 / 150.0 - v.snow_cover / 60.0)).clamp(0.0, 1.0);
            let melt = if thawing { 1.0 / 120.0 } else { 0.0 };
            v.snow_cover = (v.snow_cover + dt * (snowing / 40.0 - melt)).clamp(0.0, 1.0);
        }
        // Les tropiques n'ont presque pas de saisons : le shader module selon la latitude
        v.season = season_uniform(time);

        // Éclairs : d'autant plus fréquents que l'orage est fort, le tonnerre suit avec un délai
        // qui dit la distance de l'impact
        self.flash_age += dt;
        if ground && v.storm > 0.15 {
            self.next_flash -= dt * v.storm;
            if self.next_flash <= 0.0 {
                self.next_flash = 3.0 + 14.0 * self.rng.unit();
                self.flash_age = 0.0;
                let distance = self.rng.unit();
                self.thunder.push((0.4 + 4.0 * distance, (1.0 - 0.6 * distance) * v.storm.min(1.0)));
            }
        }
        // Éclair : un premier coup, un rebond plus faible
        let a = self.flash_age;
        v.flash = if a < FLASH_TIME {
            let first_strike = (1.0 - a / 0.12).max(0.0);
            let second = if a > 0.16 { (1.0 - (a - 0.16) / 0.19).max(0.0) * 0.6 } else { 0.0 };
            first_strike.max(second) * (1.0 - 0.5 * (1.0 - v.storm.min(1.0)))
        } else {
            0.0
        };
        for t in &mut self.thunder {
            t.0 -= dt;
        }
    }

    /// Coups de tonnerre arrivés à l'oreille du joueur à cette image (force de 0 à 1)
    pub fn take_thunder(&mut self) -> Vec<f32> {
        let arrived: Vec<f32> = self.thunder.iter().filter(|t| t.0 <= 0.0).map(|t| t.1).collect();
        self.thunder.retain(|t| t.0 > 0.0);
        arrived
    }
}

/// Dessus des colonnes autour du joueur, où s'arrêtent les gouttes : relu quand le joueur change
/// de colonne, et de temps en temps (blocs posés ou cassés)
pub struct ColumnTops {
    center: Option<(herocraft_common::PlanetFace, i32, i32)>,
    refresh_in: f32,
}

impl Default for ColumnTops {
    fn default() -> Self {
        Self { center: None, refresh_in: 0.0 }
    }
}

impl ColumnTops {
    /// Nouvelles colonnes s'il faut les relire, sinon `None`
    pub fn update(&mut self, dt: f32, collider: &VoxelCollider, generator: &PlanetWorldGenerator, feet: Vec3, base_radius: f32) -> Option<Vec<Column>> {
        self.refresh_in -= dt;
        let geometry = BlockGeometry::new(base_radius as f64);
        let at = geometry.block_at_point(feet.as_dvec3())?;
        let center = (at.face, at.x, at.z);
        if self.center == Some(center) && self.refresh_in > 0.0 {
            return None;
        }
        self.center = Some(center);
        self.refresh_in = 1.0;
        herocraft_common::prof_scope!("meteo::colonnes");
        let r = precipitation::RADIUS_BLOCKS;
        let face_blocks = herocraft_world::FACE_BLOCKS;
        let cells: Vec<(i32, i32)> = (-r..=r).flat_map(|dz| (-r..=r).map(move |dx| (at.x + dx, at.z + dz))).collect();
        let inside: Vec<(i32, i32)> = cells.iter().copied().filter(|&(x, z)| (0..face_blocks).contains(&x) && (0..face_blocks).contains(&z)).collect();
        let tops = collider.support_tops(generator, at.face, &inside);
        let mut tops = tops.into_iter();
        Some(
            cells
                .into_iter()
                .map(|(x, z)| {
                    let edge = !((0..face_blocks).contains(&x) && (0..face_blocks).contains(&z));
                    // Au-delà d'une arête du cube, la grille change de face : pas de pluie
                    let (dir, top) = if edge {
                        (Vec3::ZERO, f32::MAX)
                    } else {
                        let dir = geometry.corner(at.face, x as f64 + 0.5, 0.0, z as f64 + 0.5).normalize().as_vec3();
                        (dir, tops.next().unwrap_or(f32::MAX))
                    };
                    Column { dir: dir.to_array(), top_radius: top }
                })
                .collect(),
        )
    }
}

/// Temps qu'il fait sous chaque nuage, pour les montrer ou les cacher et les assombrir : quelques
/// nuages relus par image, le tampon envoyé quand ils ont tous été relus
pub struct CloudWeather {
    centers: Vec<Vec3>,
    ranks: Vec<f32>,
    values: Vec<[f32; 2]>,
    cursor: usize,
}

/// Nuages relus par image (environ 2 000 en tout : un tour par seconde à 60 images par seconde)
const CLOUDS_PER_FRAME: usize = 40;

impl CloudWeather {
    pub fn new(centers: Vec<Vec3>, ranks: Vec<f32>) -> Self {
        let values = ranks.iter().map(|&rank| [crate::cloud_mesh::cloud_visibility(rank, 0.7), 0.0]).collect();
        Self { centers, ranks, values, cursor: 0 }
    }

    /// Valeurs par sommet (8 par nuage) : visibilité et noirceur
    pub fn vertex_values(&self) -> Vec<[f32; 2]> {
        self.values.iter().flat_map(|v| [*v; 8]).collect()
    }

    /// Relit quelques nuages ; `true` quand le tour est fini (le tampon est à renvoyer). `time` :
    /// temps du ciel, d'où la dérive du vent (cloud_shader.wgsl, `cloud_vertex`).
    pub fn advance(&mut self, model: &WeatherModel, time: f64) -> bool {
        if self.centers.is_empty() {
            return false;
        }
        let angle = (time * 0.0035) as f32;
        let (sin, cos) = angle.sin_cos();
        for _ in 0..CLOUDS_PER_FRAME.min(self.centers.len()) {
            let i = self.cursor;
            let c = self.centers[i];
            let drifted = Vec3::new(c.x * cos - c.z * sin, c.y, c.x * sin + c.z * cos);
            let sample = model.sample(drifted.as_dvec3(), 0.0, time);
            let dark = (sample.precipitation * 0.75 + sample.storm * 0.35 + (sample.cloud_cover - 0.7).max(0.0)).min(1.0);
            self.values[i] = [crate::cloud_mesh::cloud_visibility(self.ranks[i], sample.cloud_cover), dark];
            self.cursor = (self.cursor + 1) % self.centers.len();
            if self.cursor == 0 {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_world::weather::SEASON_SECONDS;
    use herocraft_world::WeatherKind;

    #[test]
    fn seasons_colour_the_leaves_in_autumn_and_strip_them_in_winter() {
        let [na, nw, sa, sw] = season_uniform(SEASON_SECONDS * 2.5);
        assert!(na > 0.9 && nw < 0.1, "automne au nord");
        assert!(sa < 0.1 && sw < 0.1, "printemps au sud");
        let [na, nw, sa, _] = season_uniform(SEASON_SECONDS * 3.6);
        assert!(nw > 0.9 && na < 0.1 && sa < 0.1);
        let [na, nw, ..] = season_uniform(SEASON_SECONDS * 1.2);
        assert_eq!((na, nw), (0.0, 0.0), "l'été reste vert");
    }

    #[test]
    fn a_storm_brings_rain_lightning_and_thunder() {
        let mut weather = ClientWeather::new(5);
        weather.model.forced = Some(WeatherKind::Storm);
        let feet = Vec3::new(0.0, 0.0, 400.0);
        let (mut flashes, mut thunder) = (0, 0);
        for _ in 0..(60 * 120) {
            weather.update(1.0 / 60.0, 1000.0, feet, 400.0, true);
            flashes += (weather.visual().flash > 0.5) as usize;
            thunder += weather.take_thunder().len();
        }
        let v = weather.visual();
        assert!(v.precipitation > 0.9 && v.storm > 0.9 && v.wetness > 0.9 && v.snow < 0.1);
        assert!(flashes > 0 && thunder > 2, "{flashes} éclairs, {thunder} coups de tonnerre");
        let lit = v.lighting(&LightingSettings::default());
        assert!(lit.sun_intensity < LightingSettings::default().sun_intensity * 0.5);

        // Le beau temps revient : l'averse cesse en douceur, le sol sèche lentement
        weather.model.forced = Some(WeatherKind::Clear);
        for _ in 0..(60 * 5) {
            weather.update(1.0 / 60.0, 1000.0, feet, 400.0, true);
        }
        assert!(weather.visual().precipitation > 0.1, "l'averse s'arrête net");
        for _ in 0..(60 * 60) {
            weather.update(1.0 / 60.0, 1000.0, feet, 400.0, true);
        }
        assert!(weather.visual().precipitation < 0.01 && weather.visual().wetness > 0.2);
    }

    #[test]
    fn cloud_ranks_keep_the_usual_sky_and_fill_it_when_overcast() {
        let ranks: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let shown = |cover: f32| ranks.iter().filter(|&&r| crate::cloud_mesh::cloud_visibility(r, cover) > 0.5).count();
        assert!((75..=85).contains(&shown(0.7)), "{}", shown(0.7));
        assert_eq!(shown(1.0), 100);
        assert!(shown(0.0) < 35);
    }
}

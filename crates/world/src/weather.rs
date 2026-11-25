//! Météo et saisons, déduites de la graine du monde et du temps du ciel, sans rien enregistrer ni
//! envoyer : le client, le serveur et tous les joueurs en ligne voient la même pluie au même
//! endroit, tant qu'ils partagent l'horloge du ciel (`sky`).
//!
//! - **Saisons** : une année de quatre saisons de `DAYS_PER_SEASON` jours. L'hémisphère sud vit la
//!   saison opposée, et les tropiques n'ont presque pas de saisons (ni chaleur d'été, ni froid
//!   d'hiver). La saison change la température, donc la neige, et un peu la fréquence des pluies.
//! - **Météo** : un champ de bruit qui dérive lentement sur la planète dessine des zones de ciel
//!   couvert, de pluie et d'orage de quelques milliers de blocs. Il pleut plus souvent là où le
//!   climat est humide (jungle, forêts) que dans les déserts. Là où il fait froid, la pluie tombe en
//!   neige ; les orages ne naissent que dans l'air chaud.

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::cubed_sphere::{base_solar_temperature, FACE_SCALE};
use crate::generator::{fbm_3d, moisture_at, smoothstep, FBM_SPAN};
use crate::sky::SUN_PERIOD;

/// Jours de jeu (20 minutes chacun) par saison
pub const DAYS_PER_SEASON: f64 = 5.0;
/// Durée d'une saison, en secondes du ciel
pub const SEASON_SECONDS: f64 = DAYS_PER_SEASON * SUN_PERIOD as f64;
/// Durée d'une année (quatre saisons), en secondes du ciel
pub const YEAR_SECONDS: f64 = 4.0 * SEASON_SECONDS;

/// Sous cette température (échelle des biomes : 1 à l'équateur, 0 aux pôles), la pluie tombe en
/// neige. Les régions froides (taïga, toundra, sous 0,5) ont donc de la neige une bonne part de
/// l'année, les plaines tempérées en plein hiver seulement.
pub const SNOW_TEMPERATURE: f32 = 0.40;
/// Écart de température entre le cœur de l'été et le cœur de l'hiver, aux latitudes à saisons
const SEASON_SWING: f64 = 0.22;
/// Taille des zones de météo : un peu plus de 2 000 blocs d'une averse à l'autre
const WEATHER_FREQUENCY: f64 = 0.42 * FACE_SCALE;
/// Vitesse de dérive du champ de météo, en unités du bruit par seconde : une averse passe sur un
/// lieu en cinq à dix minutes de jeu
const WEATHER_DRIFT: f64 = 0.00045 * FACE_SCALE;

/// Saison, dans l'ordre de l'année
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

    /// Nom affiché
    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "Printemps",
            Season::Summer => "Été",
            Season::Autumn => "Automne",
            Season::Winter => "Hiver",
        }
    }

    /// Saison nommée en français ou en anglais (`printemps`, `ete`, `été`, `summer`…)
    pub fn parse(name: &str) -> Option<Season> {
        match name.to_lowercase().as_str() {
            "printemps" | "spring" => Some(Season::Spring),
            "ete" | "été" | "summer" => Some(Season::Summer),
            "automne" | "autumn" | "fall" => Some(Season::Autumn),
            "hiver" | "winter" => Some(Season::Winter),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Moment de l'année dans l'hémisphère nord : 0 au premier jour du printemps, 1 un an plus tard
pub fn year_phase(time: f64) -> f64 {
    (time / YEAR_SECONDS).rem_euclid(1.0)
}

/// Saison et avancée dans la saison (0 à 1) au moment `time`, pour l'hémisphère de latitude `lat`
/// (en radians, positive au nord)
pub fn season_at(time: f64, lat: f64) -> (Season, f32) {
    let phase = hemisphere_phase(year_phase(time), lat) * 4.0;
    let index = (phase.floor() as usize).min(3);
    (Season::ALL[index], phase.fract() as f32)
}

/// L'hémisphère sud a six mois de décalage
fn hemisphere_phase(phase: f64, lat: f64) -> f64 {
    if lat < 0.0 {
        (phase + 0.5).rem_euclid(1.0)
    } else {
        phase
    }
}

/// Force des saisons à une latitude : nulle sous les tropiques, entière aux latitudes tempérées
/// et froides
pub fn season_strength(lat: f64) -> f32 {
    smoothstep(0.20, 0.75, lat.abs()) as f32
}

/// Temps du ciel du premier jour de `season` (hémisphère nord) suivant `time`, à la même heure
/// du jour : l'année compte un nombre entier de jours
pub fn time_for_season(time: f64, season: Season) -> f64 {
    let year_start = (time / YEAR_SECONDS).floor() * YEAR_SECONDS;
    let day_time = time.rem_euclid(SUN_PERIOD as f64);
    let mut target = year_start + season.index() as f64 * SEASON_SECONDS + day_time;
    if target < time - SUN_PERIOD as f64 {
        target += YEAR_SECONDS;
    }
    target
}

pub use herocraft_common::WeatherKind;

/// Le temps qu'il fait en un lieu et à un moment
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherSample {
    pub season: Season,
    /// Avancée dans la saison, de 0 (premier jour) à 1
    pub season_progress: f32,
    /// Force des saisons à cette latitude (0 sous les tropiques, 1 aux latitudes tempérées)
    pub season_strength: f32,
    /// Température, sur l'échelle des biomes (1 à l'équateur, 0 aux pôles ; `SNOW_TEMPERATURE` :
    /// la pluie devient neige)
    pub temperature: f32,
    /// Part du ciel couverte de nuages, de 0 à 1
    pub cloud_cover: f32,
    /// Force de la pluie ou de la neige, de 0 (sec) à 1 (averse)
    pub precipitation: f32,
    /// Ce qui tombe est de la neige
    pub snow: bool,
    /// Force de l'orage (éclairs, tonnerre, vent), de 0 à 1
    pub storm: f32,
    /// Force du vent, de 0 (calme) à 1 (tempête)
    pub wind: f32,
}

impl WeatherSample {
    /// Le temps qu'il fait, en un mot
    pub fn kind(&self) -> WeatherKind {
        if self.precipitation > 0.05 {
            if self.snow {
                WeatherKind::Snow
            } else if self.storm > 0.2 {
                WeatherKind::Storm
            } else {
                WeatherKind::Rain
            }
        } else if self.cloud_cover > 0.6 {
            WeatherKind::Cloudy
        } else {
            WeatherKind::Clear
        }
    }

    /// Il pleut ou il neige
    pub(crate) fn is_wet(&self) -> bool {
        self.precipitation > 0.05
    }

    /// Remplace le temps par `kind`, en gardant la saison et la température
    fn forced(self, kind: WeatherKind) -> Self {
        let (cloud_cover, precipitation, storm, snow) = match kind {
            WeatherKind::Clear => (0.15, 0.0, 0.0, self.snow_at_temperature()),
            WeatherKind::Cloudy => (0.85, 0.0, 0.0, self.snow_at_temperature()),
            WeatherKind::Rain => (0.95, 0.7, 0.0, false),
            WeatherKind::Storm => (1.0, 1.0, 1.0, false),
            WeatherKind::Snow => (0.95, 0.7, 0.0, true),
        };
        Self { cloud_cover, precipitation, storm, snow, wind: wind(cloud_cover, storm), ..self }
    }

    fn snow_at_temperature(&self) -> bool {
        self.temperature < SNOW_TEMPERATURE
    }
}

/// Multiplicateur de la pousse des cultures : nulle sous la neige et dans le gel d'hiver, ralentie
/// en automne, entière au printemps et en été, un peu plus vive sous la pluie
pub fn crop_growth_factor(sample: &WeatherSample) -> f32 {
    if sample.snow && sample.is_wet() {
        return 0.0;
    }
    if sample.temperature < SNOW_TEMPERATURE {
        // Gel : rien ne pousse
        return 0.0;
    }
    let by_season = match sample.season {
        Season::Spring | Season::Summer => 1.0,
        Season::Autumn => 0.5,
        Season::Winter => 0.2,
    };
    // Sous les tropiques, les saisons comptent peu
    let seasonal = 1.0 + (by_season - 1.0) * sample.season_strength;
    seasonal * (1.0 + 0.2 * sample.precipitation.min(1.0))
}

/// Météo d'une planète : la graine fixe les zones de pluie, l'horloge du ciel les fait dériver
#[derive(Debug, Clone)]
pub struct WeatherModel {
    seed: u64,
    /// Temps choisi à la main, qui s'impose partout (`None` : le temps naturel)
    pub forced: Option<WeatherKind>,
}

impl WeatherModel {
    pub fn new(seed: u64) -> Self {
        Self { seed, forced: None }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Le temps qu'il fait au point `dir` de la sphère unité (repère du centre de la planète), à
    /// `altitude_blocks` blocs au-dessus du niveau de la mer, au temps du ciel `time`
    pub fn sample(&self, dir: DVec3, altitude_blocks: f64, time: f64) -> WeatherSample {
        let dir = dir.normalize_or_zero();
        let lat = dir.y.clamp(-1.0, 1.0).asin();
        let (season, season_progress) = season_at(time, lat);
        let strength = season_strength(lat);
        let temperature = self.temperature(lat, altitude_blocks, time);

        // Humidité du climat et du moment : un bruit qui dérive, des périodes sèches ou humides
        // plus vastes et plus lentes, et un peu plus de pluie aux saisons de transition
        let moisture = moisture_at(self.seed, dir);
        let drift = DVec3::new(0.62, 0.18, -0.76) * (time * WEATHER_DRIFT);
        let cells = fbm_3d(dir * WEATHER_FREQUENCY + drift, 3, 2.0, 0.5, self.seed ^ 0x5745_4154) / FBM_SPAN;
        let slow_drift = DVec3::new(-0.3, 0.1, 0.95) * (time * WEATHER_DRIFT * 0.35);
        let spells = fbm_3d(dir * (WEATHER_FREQUENCY * 0.3) + slow_drift, 2, 2.0, 0.5, self.seed ^ 0x5350_454C) / FBM_SPAN;
        let season_bias = match season {
            Season::Spring | Season::Autumn => 0.08,
            Season::Summer => -0.06,
            Season::Winter => 0.0,
        } * strength as f64;
        let wet = cells * 0.75 + spells * 0.45 + (moisture - 0.5) * 1.1 + season_bias;

        let cloud_cover = smoothstep(-0.45, 0.40, wet) as f32;
        let precipitation = smoothstep(0.30, 0.70, wet) as f32;
        let snow = temperature < SNOW_TEMPERATURE;
        // Orages : les averses les plus fortes, dans l'air chaud
        let storm = if snow { 0.0 } else { (smoothstep(0.62, 0.95, wet) * smoothstep(0.55, 0.72, temperature as f64)) as f32 };
        let sample = WeatherSample {
            season,
            season_progress,
            season_strength: strength,
            temperature,
            cloud_cover,
            precipitation,
            snow,
            storm,
            wind: wind(cloud_cover, storm),
        };
        match self.forced {
            Some(kind) => sample.forced(kind),
            None => sample,
        }
    }

    /// Température d'un lieu : celle du climat (latitude, altitude), plus chaude en été et plus
    /// froide en hiver hors des tropiques
    pub fn temperature(&self, lat: f64, altitude_blocks: f64, time: f64) -> f32 {
        let climate = base_solar_temperature(lat) - altitude_blocks.max(0.0) / 250.0;
        // Cœur de l'été au milieu de la deuxième saison, de l'hiver au milieu de la quatrième
        let phase = hemisphere_phase(year_phase(time), lat);
        let seasonal = (std::f64::consts::TAU * (phase - 0.125)).sin() * SEASON_SWING * season_strength(lat) as f64;
        (climate + seasonal) as f32
    }
}

/// Horloge du ciel d'un monde et temps imposé, enregistrés dans sa sauvegarde (`temps.json`) : on
/// retrouve l'heure, la saison et la météo là où on les avait laissées
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SkyClock {
    /// Temps du ciel, en secondes (`sky::compute_sun_dir`)
    pub time: f64,
    /// Temps imposé par `/weather` (`None` : le temps naturel)
    #[serde(default)]
    pub forced: Option<WeatherKind>,
}

/// Nom du fichier de l'horloge dans le dossier de sauvegarde d'un monde
pub(crate) const CLOCK_FILE: &str = "temps.json";

impl SkyClock {
    /// Horloge enregistrée dans le dossier `dir`, ou celle d'un monde neuf (fichier absent ou illisible)
    pub fn load(dir: impl AsRef<std::path::Path>) -> Self {
        let path = dir.as_ref().join(CLOCK_FILE);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| {
                serde_json::from_str::<SkyClock>(&text)
                    .map_err(|e| log::warn!("Horloge {} illisible ({e}) : le monde repart au premier jour.", path.display()))
                    .ok()
            })
            .filter(|c| c.time.is_finite())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(CLOCK_FILE), text)
    }
}

fn wind(cloud_cover: f32, storm: f32) -> f32 {
    (0.15 + 0.35 * cloud_cover + 0.6 * storm).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latitude_dir(lat_deg: f64, lon_deg: f64) -> DVec3 {
        let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
        DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
    }

    #[test]
    fn seasons_follow_the_year_and_the_hemisphere() {
        let north = 0.8;
        let south = -0.8;
        assert_eq!(season_at(0.0, north).0, Season::Spring);
        assert_eq!(season_at(SEASON_SECONDS * 1.5, north).0, Season::Summer);
        assert_eq!(season_at(SEASON_SECONDS * 2.5, north).0, Season::Autumn);
        assert_eq!(season_at(SEASON_SECONDS * 3.5, north).0, Season::Winter);
        assert_eq!(season_at(SEASON_SECONDS * 4.5, north).0, Season::Spring);
        // Six mois de décalage au sud
        assert_eq!(season_at(SEASON_SECONDS * 1.5, south).0, Season::Winter);
        assert_eq!(season_at(0.0, south).0, Season::Autumn);
        let (_, progress) = season_at(SEASON_SECONDS * 2.25, north);
        assert!((progress - 0.25).abs() < 1e-4);
    }

    #[test]
    fn summer_is_warmer_than_winter_except_in_the_tropics() {
        let model = WeatherModel::new(7);
        let summer = SEASON_SECONDS * 1.5;
        let winter = SEASON_SECONDS * 3.5;
        let lat = 50f64.to_radians();
        assert!(model.temperature(lat, 0.0, summer) > model.temperature(lat, 0.0, winter) + 0.3);
        // Au sud, c'est l'inverse
        assert!(model.temperature(-lat, 0.0, summer) < model.temperature(-lat, 0.0, winter) - 0.3);
        // À l'équateur, presque rien ne change
        assert!((model.temperature(0.0, 0.0, summer) - model.temperature(0.0, 0.0, winter)).abs() < 0.01);
        // L'altitude refroidit
        assert!(model.temperature(lat, 150.0, summer) < model.temperature(lat, 0.0, summer));
    }

    #[test]
    fn time_for_season_keeps_the_hour_and_moves_forward() {
        let time = SEASON_SECONDS * 2.0 + 333.0; // automne, 333 s après minuit... du temps du ciel
        for season in Season::ALL {
            let t = time_for_season(time, season);
            assert!(t >= time - SUN_PERIOD as f64, "{season:?}");
            assert!(t < time + YEAR_SECONDS);
            assert_eq!(season_at(t, 1.0).0, season);
            assert!(((t - time).rem_euclid(SUN_PERIOD as f64)).abs() < 1e-6);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_weather() {
        let (a, b) = (WeatherModel::new(42), WeatherModel::new(42));
        for i in 0..50 {
            let dir = latitude_dir(i as f64 * 3.0 - 70.0, i as f64 * 17.0);
            let t = i as f64 * 913.0;
            assert_eq!(a.sample(dir, 10.0, t), b.sample(dir, 10.0, t));
        }
    }

    /// Part du temps où il pleut ou neige, sur une année, en des lieux tirés au hasard
    fn wet_share(model: &WeatherModel, lats: std::ops::Range<i32>, filter: impl Fn(f64) -> bool) -> (f64, f64, f64) {
        let (mut wet, mut storm, mut total) = (0usize, 0usize, 0usize);
        let mut snow = 0usize;
        for lat in lats.step_by(3) {
            for lon in (0..360).step_by(9) {
                let dir = latitude_dir(lat as f64, lon as f64);
                if !filter(moisture_at(model.seed(), dir)) {
                    continue;
                }
                for step in 0..60 {
                    let t = step as f64 * YEAR_SECONDS / 60.0 + lon as f64 * 7.0;
                    let s = model.sample(dir, 5.0, t);
                    total += 1;
                    wet += s.is_wet() as usize;
                    storm += (s.storm > 0.2) as usize;
                    snow += (s.is_wet() && s.snow) as usize;
                }
            }
        }
        let n = total.max(1) as f64;
        (wet as f64 / n, storm as f64 / n, snow as f64 / n)
    }

    #[test]
    fn it_rains_now_and_then_more_where_the_air_is_humid() {
        let model = WeatherModel::new(2026);
        let (all, storms, _) = wet_share(&model, -60..61, |_| true);
        println!("pluie ou neige {all:.3}, orage {storms:.3}");
        assert!((0.12..0.40).contains(&all), "pluie {all}");
        assert!(storms > 0.005 && storms < all, "orages {storms}");
        let (dry, _, _) = wet_share(&model, -60..61, |m| m < 0.3);
        let (humid, _, _) = wet_share(&model, -60..61, |m| m > 0.6);
        println!("climat sec {dry:.3}, humide {humid:.3}");
        assert!(humid > dry * 2.0, "humide {humid}, sec {dry}");
    }

    #[test]
    fn it_snows_in_the_cold_and_never_at_the_equator() {
        let model = WeatherModel::new(2026);
        let (_, _, tropical_snow) = wet_share(&model, -15..16, |_| true);
        assert_eq!(tropical_snow, 0.0);
        let (polar_wet, _, polar_snow) = wet_share(&model, 65..80, |_| true);
        assert!(polar_snow > 0.0 && polar_snow > 0.6 * polar_wet, "neige {polar_snow}, pluie et neige {polar_wet}");
    }

    #[test]
    fn forced_weather_wins() {
        let mut model = WeatherModel::new(3);
        let dir = latitude_dir(5.0, 40.0);
        model.forced = Some(WeatherKind::Storm);
        let s = model.sample(dir, 0.0, 1000.0);
        assert_eq!(s.kind(), WeatherKind::Storm);
        model.forced = Some(WeatherKind::Clear);
        assert_eq!(model.sample(dir, 0.0, 1000.0).kind(), WeatherKind::Clear);
        model.forced = Some(WeatherKind::Snow);
        assert_eq!(model.sample(dir, 0.0, 1000.0).kind(), WeatherKind::Snow);
        for kind in WeatherKind::ALL {
            assert_eq!(WeatherKind::parse(kind.command_word()), Some(kind));
        }
    }

    #[test]
    fn the_clock_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("herocraft_horloge_{}", std::process::id()));
        assert_eq!(SkyClock::load(&dir), SkyClock::default());
        let clock = SkyClock { time: 12345.5, forced: Some(WeatherKind::Snow) };
        clock.save(&dir).unwrap();
        assert_eq!(SkyClock::load(&dir), clock);
        std::fs::write(dir.join(CLOCK_FILE), "pas du json").unwrap();
        assert_eq!(SkyClock::load(&dir), SkyClock::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn crops_stop_in_the_frost_and_grow_in_spring() {
        let model = WeatherModel::new(11);
        let dir = latitude_dir(50.0, 10.0);
        let spring = model.sample(dir, 0.0, SEASON_SECONDS * 0.5);
        let winter = model.sample(dir, 0.0, SEASON_SECONDS * 3.5);
        assert!(crop_growth_factor(&spring) > crop_growth_factor(&winter));
        let frozen = WeatherSample { temperature: 0.1, ..winter };
        assert_eq!(crop_growth_factor(&frozen), 0.0);
    }
}

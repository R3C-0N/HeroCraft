//! Temps qu'il fait, en un mot : partagé par le protocole (temps imposé par `/weather`) et la
//! météo du crate `herocraft_world`.

use serde::{Deserialize, Serialize};

/// Temps qu'il fait, tel que choisi à la main (commande `/weather`) : il s'impose partout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeatherKind {
    /// Ciel dégagé
    Clear,
    /// Ciel couvert, sans pluie
    Cloudy,
    Rain,
    /// Pluie d'orage, éclairs et tonnerre
    Storm,
    Snow,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 5] = [WeatherKind::Clear, WeatherKind::Cloudy, WeatherKind::Rain, WeatherKind::Storm, WeatherKind::Snow];

    pub fn name(self) -> &'static str {
        match self {
            WeatherKind::Clear => "Beau temps",
            WeatherKind::Cloudy => "Couvert",
            WeatherKind::Rain => "Pluie",
            WeatherKind::Storm => "Orage",
            WeatherKind::Snow => "Neige",
        }
    }

    /// Mot de la commande `/weather` (`clear`, `cloudy`, `rain`, `storm`, `snow`, ou en français)
    pub fn parse(name: &str) -> Option<WeatherKind> {
        match name.to_lowercase().as_str() {
            "clair" | "beau" | "soleil" | "clear" | "sun" => Some(WeatherKind::Clear),
            "couvert" | "nuages" | "cloudy" | "clouds" => Some(WeatherKind::Cloudy),
            "pluie" | "rain" => Some(WeatherKind::Rain),
            "orage" | "storm" | "thunder" => Some(WeatherKind::Storm),
            "neige" | "snow" => Some(WeatherKind::Snow),
            _ => None,
        }
    }

    /// Mot de la commande `/weather`
    pub fn command_word(self) -> &'static str {
        match self {
            WeatherKind::Clear => "clear",
            WeatherKind::Cloudy => "cloudy",
            WeatherKind::Rain => "rain",
            WeatherKind::Storm => "storm",
            WeatherKind::Snow => "snow",
        }
    }
}

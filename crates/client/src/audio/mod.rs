//! Son du jeu : bruitages du joueur et des blocs, ambiance (vent, oiseaux le jour, grillons la nuit).
//! Les sons sont synthétisés au lancement (`synth`) et mixés par rodio. Sans carte son (serveur,
//! WSL sans PulseAudio…), le jeu tourne normalement en silence.

pub mod events;
pub mod synth;

use std::num::NonZero;
use std::path::{Path, PathBuf};

use log::{info, warn};
use rodio::buffer::SamplesBuffer;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use serde::{Deserialize, Serialize};

use events::{PlayerSound, PlayerSoundTracker, PlayerState};
use synth::{Material, Noise, SAMPLE_RATE};

pub const AUDIO_FILE: &str = "config/audio.json";

/// Variantes synthétisées par son : tirées au hasard, jamais deux fois de suite la même
const VARIANTS: usize = 4;
/// Durée de la boucle de vent
const WIND_LOOP_SECONDS: f32 = 12.0;
/// Au-delà, un bloc changé par un autre joueur ne s'entend plus (en blocs)
const REMOTE_HEARING_BLOCKS: f32 = 32.0;

/// Volumes réglés dans Options > Son, enregistrés dans `config/audio.json`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {
    /// Volume général [0, 1]
    pub master: f32,
    /// Bruitages : pas, saut, blocs [0, 1]
    pub effects: f32,
    /// Ambiance : vent, oiseaux, grillons [0, 1]
    pub ambience: f32,
    pub muted: bool,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { master: 0.8, effects: 0.8, ambience: 0.6, muted: false, path: None }
    }
}

impl AudioSettings {
    /// Charge les volumes depuis `path`, ou les valeurs par défaut si le fichier est absent ou illisible.
    /// Les sauvegardes suivantes écrivent dans `path`.
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let mut settings = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<AudioSettings>(&text).unwrap_or_else(|e| {
                warn!("Réglages du son illisibles dans {} ({}), volumes par défaut.", path.display(), e);
                AudioSettings::default()
            }),
            Err(_) => AudioSettings::default(),
        };
        for v in [&mut settings.master, &mut settings.effects, &mut settings.ambience] {
            *v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 };
        }
        settings.path = Some(path.to_path_buf());
        settings
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            std::fs::write(path, text)
        })();
        if let Err(e) = result {
            warn!("Impossible d'enregistrer les réglages du son dans {} : {}", path.display(), e);
        }
    }

    /// Remet les volumes par défaut (le fichier d'enregistrement reste le même)
    pub fn reset(&mut self) {
        *self = Self { path: self.path.take(), ..Self::default() };
    }

    /// Gain final des bruitages. L'oreille perçoit le volume de façon logarithmique : la courbe au
    /// carré rend le curseur régulier.
    pub fn effects_gain(&self) -> f32 {
        if self.muted { 0.0 } else { (self.master * self.effects).powi(2) }
    }

    pub fn ambience_gain(&self) -> f32 {
        if self.muted { 0.0 } else { (self.master * self.ambience).powi(2) }
    }
}

/// Ce que l'ambiance doit savoir du joueur à chaque image
#[derive(Debug, Clone, Copy)]
pub struct AmbienceInput {
    /// Vue au sol ou à la 3e personne (pas la vue satellite)
    pub ground_view: bool,
    /// Menu de pause ouvert
    pub paused: bool,
    /// Hauteur au-dessus du relief, en blocs
    pub height_blocks: f32,
    /// Un plafond au-dessus de la tête (grotte, abri) : vent étouffé, pas d'animaux
    pub covered: bool,
    /// Le Soleil est levé au-dessus du joueur
    pub is_day: bool,
}

/// Sons synthétisés, par matière pour ceux des pas et des blocs
struct SoundBank {
    steps: Vec<Vec<SamplesBuffer>>,
    breaks: Vec<Vec<SamplesBuffer>>,
    places: Vec<Vec<SamplesBuffer>>,
    jumps: Vec<SamplesBuffer>,
    landings: Vec<SamplesBuffer>,
    hurts: Vec<SamplesBuffer>,
    birds: Vec<SamplesBuffer>,
    crickets: Vec<SamplesBuffer>,
}

fn buffer(samples: Vec<f32>) -> SamplesBuffer {
    SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(SAMPLE_RATE).expect("fréquence non nulle"), samples)
}

fn variants(make: impl Fn(u32) -> Vec<f32>) -> Vec<SamplesBuffer> {
    (0..VARIANTS as u32).map(|seed| buffer(make(seed))).collect()
}

impl SoundBank {
    fn synthesize() -> Self {
        herocraft_common::prof_scope!("audio::synthese");
        let per_material = |make: fn(Material, u32) -> Vec<f32>| -> Vec<Vec<SamplesBuffer>> {
            Material::ALL.iter().map(|&m| variants(|seed| make(m, seed))).collect()
        };
        Self {
            steps: per_material(synth::footstep),
            breaks: per_material(synth::block_break),
            places: per_material(synth::block_place),
            jumps: variants(synth::jump_whoosh),
            landings: variants(synth::landing_thud),
            hurts: variants(synth::hurt_grunt),
            birds: (0..8).map(|seed| buffer(synth::bird_song(seed))).collect(),
            crickets: variants(synth::cricket_chirps),
        }
    }
}

fn material_index(material: Material) -> usize {
    Material::ALL.iter().position(|&m| m == material).unwrap_or(0)
}

/// Sortie audio ouverte : la carte son, la boucle de vent et les sons préparés
struct Output {
    device: MixerDeviceSink,
    wind: Player,
    bank: SoundBank,
}

pub struct Audio {
    output: Option<Output>,
    rng: Noise,
    last_variant: usize,
    tracker: PlayerSoundTracker,
    /// Niveau du vent lissé (avant le volume de l'ambiance)
    wind_level: f32,
    /// Secondes avant le prochain chant d'oiseau ou de grillon
    critter_in: f32,
}

impl Audio {
    /// Ouvre la carte son par défaut et prépare les sons. En cas d'échec, le jeu reste muet.
    pub fn new() -> Self {
        let output = match DeviceSinkBuilder::open_default_sink() {
            Ok(mut device) => {
                device.log_on_drop(false);
                let started = std::time::Instant::now();
                let bank = SoundBank::synthesize();
                let wind = Player::connect_new(device.mixer());
                wind.set_volume(0.0);
                wind.append(buffer(synth::wind_loop(WIND_LOOP_SECONDS, 1)).repeat_infinite());
                info!("Son prêt ({} ms de synthèse).", started.elapsed().as_millis());
                Some(Output { device, wind, bank })
            }
            Err(e) => {
                warn!("Aucune sortie audio ({}) : le jeu continue sans le son.", e);
                None
            }
        };
        Self { output, rng: Noise::new(0xA0D1), last_variant: usize::MAX, tracker: PlayerSoundTracker::default(), wind_level: 0.0, critter_in: 4.0 }
    }

    /// Tire une variante, différente de la précédente quand il y en a plusieurs
    fn pick<'a>(&mut self, sounds: &'a [SamplesBuffer]) -> Option<&'a SamplesBuffer> {
        if sounds.is_empty() {
            return None;
        }
        let mut i = (self.rng.unit() * sounds.len() as f32) as usize % sounds.len();
        if sounds.len() > 1 && i == self.last_variant {
            i = (i + 1) % sounds.len();
        }
        self.last_variant = i;
        sounds.get(i)
    }

    /// Joue un son de la banque, avec une petite variation de hauteur
    fn play(&mut self, choose: impl Fn(&SoundBank) -> &[SamplesBuffer], gain: f32, pitch_spread: f32) {
        if gain <= 1e-4 {
            return;
        }
        let Some(output) = self.output.take() else { return };
        let pitch = 1.0 + pitch_spread * self.rng.next();
        if let Some(sound) = self.pick(choose(&output.bank)) {
            output.device.mixer().add(sound.clone().speed(pitch).amplify(gain));
        }
        self.output = Some(output);
    }

    /// Le joueur n'est plus suivi (vue satellite, menu, téléportation) : pas de faux pas au retour
    pub fn reset_player(&mut self) {
        self.tracker.reset();
    }

    /// Pas, saut et réception déduits du mouvement du joueur à cette image
    pub fn update_player(&mut self, state: PlayerState, surface: Material, settings: &AudioSettings) {
        let gain = settings.effects_gain();
        let m = material_index(surface);
        match self.tracker.update(state, surface) {
            Some(PlayerSound::Step(_)) => {
                let quiet = if state.sprinting { 0.55 } else { 0.4 };
                self.play(|b| &b.steps[m], gain * quiet, 0.08);
            }
            Some(PlayerSound::Jump(_)) => {
                self.play(|b| &b.steps[m], gain * 0.45, 0.05);
                self.play(|b| &b.jumps, gain * 0.25, 0.1);
            }
            Some(PlayerSound::Land(_, strength)) => {
                self.play(|b| &b.steps[m], gain * (0.4 + 0.3 * strength), 0.05);
                self.play(|b| &b.landings, gain * 0.7 * strength, 0.08);
            }
            None => {}
        }
    }

    /// Bloc cassé, à `distance_blocks` du joueur (0 pour les siens)
    pub fn block_broken(&mut self, material: Material, distance_blocks: f32, settings: &AudioSettings) {
        let m = material_index(material);
        self.play(|b| &b.breaks[m], settings.effects_gain() * 0.7 * attenuation(distance_blocks), 0.1);
    }

    /// Le joueur se blesse, avec une force de 0 à 1
    pub fn player_hurt(&mut self, strength: f32, settings: &AudioSettings) {
        let gain = settings.effects_gain();
        self.play(|b| &b.hurts, gain * (0.45 + 0.35 * strength), 0.06);
        self.play(|b| &b.landings, gain * 0.6 * strength, 0.08);
    }

    /// Coup porté au bloc que le joueur est en train de casser
    pub fn block_hit(&mut self, material: Material, settings: &AudioSettings) {
        let m = material_index(material);
        self.play(|b| &b.places[m], settings.effects_gain() * 0.3, 0.12);
    }

    /// Bloc posé, à `distance_blocks` du joueur (0 pour les siens)
    pub fn block_placed(&mut self, material: Material, distance_blocks: f32, settings: &AudioSettings) {
        let m = material_index(material);
        self.play(|b| &b.places[m], settings.effects_gain() * 0.6 * attenuation(distance_blocks), 0.06);
    }

    /// Son d'essai joué en réglant les volumes
    pub fn preview(&mut self, settings: &AudioSettings) {
        self.block_placed(Material::Stone, 0.0, settings);
    }

    /// Vent qui forcit avec l'altitude, oiseaux le jour et grillons la nuit, près du sol
    pub fn update_ambience(&mut self, dt: f32, input: AmbienceInput, settings: &AudioSettings) {
        if self.output.is_none() {
            return;
        }
        let mut target = if input.ground_view { 0.3 + 0.7 * (input.height_blocks / 40.0).clamp(0.0, 1.0) } else { 0.12 };
        if input.covered {
            target *= 0.3;
        }
        if input.paused {
            target *= 0.5;
        }
        self.wind_level += (target - self.wind_level) * (1.0 - (-1.5 * dt).exp());
        let gain = settings.ambience_gain();
        if let Some(output) = &self.output {
            output.wind.set_volume(self.wind_level * gain * 0.5);
        }

        let near_ground = input.ground_view && !input.paused && !input.covered && input.height_blocks < 12.0;
        if !near_ground {
            return;
        }
        self.critter_in -= dt;
        if self.critter_in > 0.0 {
            return;
        }
        let loudness = 0.4 + 0.6 * self.rng.unit();
        if input.is_day {
            self.play(|b| &b.birds, gain * 0.3 * loudness, 0.12);
            self.critter_in = 5.0 + 9.0 * self.rng.unit();
        } else {
            self.play(|b| &b.crickets, gain * 0.25 * loudness, 0.05);
            self.critter_in = 2.5 + 5.0 * self.rng.unit();
        }
    }
}

/// Atténuation avec la distance : pleine à côté du joueur, nulle au-delà de la portée d'écoute
fn attenuation(distance_blocks: f32) -> f32 {
    if distance_blocks >= REMOTE_HEARING_BLOCKS {
        return 0.0;
    }
    let fade = 1.0 - distance_blocks / REMOTE_HEARING_BLOCKS;
    fade * fade / (1.0 + distance_blocks / 6.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_gains() {
        let mut s = AudioSettings::default();
        assert!(s.effects_gain() > 0.0 && s.effects_gain() <= 1.0);
        s.master = 0.0;
        assert_eq!(s.effects_gain(), 0.0);
        s.master = 1.0;
        s.effects = 1.0;
        assert_eq!(s.effects_gain(), 1.0);
        s.muted = true;
        assert_eq!(s.effects_gain(), 0.0);
        assert_eq!(s.ambience_gain(), 0.0);
    }

    #[test]
    fn test_settings_round_trip_and_repair() {
        let dir = std::env::temp_dir().join(format!("herocraft_audio_{}", std::process::id()));
        let path = dir.join("audio.json");
        let mut s = AudioSettings::load_or_default(&path);
        assert_eq!(s.master, AudioSettings::default().master);
        s.master = 0.3;
        s.muted = true;
        s.save();
        let loaded = AudioSettings::load_or_default(&path);
        assert_eq!(loaded.master, 0.3);
        assert!(loaded.muted);

        // Fichier édité à la main : valeurs hors bornes ramenées, champs manquants par défaut
        std::fs::write(&path, r#"{ "master": 7.0 }"#).unwrap();
        let repaired = AudioSettings::load_or_default(&path);
        assert_eq!(repaired.master, 1.0);
        assert_eq!(repaired.ambience, AudioSettings::default().ambience);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_attenuation_fades_with_distance() {
        assert_eq!(attenuation(0.0), 1.0);
        assert!(attenuation(4.0) < attenuation(1.0));
        assert_eq!(attenuation(REMOTE_HEARING_BLOCKS), 0.0);
    }

    /// Sans carte son (intégration continue) le jeu reste muet ; avec, les sons partent sans erreur
    #[test]
    fn test_audio_never_blocks_the_game() {
        let mut audio = Audio::new();
        let muted = AudioSettings { muted: true, ..AudioSettings::default() };
        audio.block_broken(Material::Stone, 0.0, &muted);
        audio.preview(&muted);
        let input = AmbienceInput { ground_view: true, paused: false, height_blocks: 0.0, covered: false, is_day: true };
        audio.update_ambience(0.016, input, &muted);
    }

    /// Joue chaque son sur la sortie audio, pour les écouter :
    /// `cargo test -p herocraft_client ecouter_les_sons -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ecouter_les_sons() {
        let mut audio = Audio::new();
        let settings = AudioSettings { master: 1.0, effects: 1.0, ambience: 1.0, ..AudioSettings::default() };
        let pause = |s: f32| std::thread::sleep(std::time::Duration::from_secs_f32(s));
        for m in Material::ALL {
            println!("{m:?} : pas, bloc cassé, bloc posé");
            for _ in 0..4 {
                let m = material_index(m);
                audio.play(|b| &b.steps[m], 0.6, 0.08);
                pause(0.4);
            }
            audio.block_broken(m, 0.0, &settings);
            pause(0.6);
            audio.block_placed(m, 0.0, &settings);
            pause(0.6);
        }
        println!("Saut, réception, oiseau, grillons");
        audio.play(|b| &b.jumps, 0.5, 0.0);
        pause(0.6);
        audio.play(|b| &b.landings, 0.8, 0.0);
        pause(0.6);
        audio.play(|b| &b.birds, 0.5, 0.0);
        pause(1.5);
        audio.play(|b| &b.crickets, 0.5, 0.0);
        pause(1.5);
        println!("Vent");
        let input = AmbienceInput { ground_view: true, paused: false, height_blocks: 40.0, covered: false, is_day: true };
        for _ in 0..300 {
            audio.update_ambience(0.016, AmbienceInput { is_day: false, ..input }, &settings);
            pause(0.016);
        }
    }

    /// Écrit chaque son en WAV dans `target/sons/` :
    /// `cargo test -p herocraft_client exporter_les_sons -- --ignored`
    #[test]
    #[ignore]
    fn exporter_les_sons() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sons");
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, samples: Vec<f32>| {
            let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
            let data_len = (samples.len() * 2) as u32;
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
            bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
            bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
            bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.extend_from_slice(&16u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&data_len.to_le_bytes());
            for s in samples {
                bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
            }
            std::fs::write(dir.join(format!("{name}.wav")), bytes).unwrap();
        };
        for m in Material::ALL {
            let name = format!("{m:?}").to_lowercase();
            write(&format!("pas_{name}"), synth::footstep(m, 0));
            write(&format!("casse_{name}"), synth::block_break(m, 0));
            write(&format!("pose_{name}"), synth::block_place(m, 0));
        }
        write("saut", synth::jump_whoosh(0));
        write("reception", synth::landing_thud(0));
        write("oiseau", synth::bird_song(0));
        write("grillons", synth::cricket_chirps(0));
        write("vent", synth::wind_loop(WIND_LOOP_SECONDS, 1));
        println!("Sons écrits dans {}", dir.display());
    }

    #[test]
    fn test_reset_keeps_the_save_path() {
        let mut s = AudioSettings::load_or_default("config_test/audio.json");
        s.master = 0.1;
        s.reset();
        assert_eq!(s.master, AudioSettings::default().master);
        assert!(s.path.is_some());
    }
}

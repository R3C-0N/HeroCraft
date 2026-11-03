//! Banc d'essai automatique : `herocraft_client --benchmark`.
//!
//! Le jeu se lance sur une planète fixe et joue tout seul une suite de phases. Chaque phase change
//! un seul réglage, ou un seul mode de caméra ou de déplacement, par rapport à la phase de
//! référence (tout activé, vue à la première personne, joueur immobile qui tourne la tête). Après
//! un temps de chauffe, chaque phase mesure la durée des images, le temps CPU de chaque étape de
//! l'image et, si la carte le permet, le temps GPU de chaque passe. L'écart avec la référence donne
//! le coût de ce qui a été coupé. Le rapport est écrit dans `./benchmark/`, puis le jeu se ferme.
//!
//! La V-Sync et la limite de FPS sont coupées pendant le banc d'essai, le joueur ne peut pas être
//! blessé, rien n'est enregistré dans la sauvegarde, et la souris n'est pas capturée : la fenêtre
//! peut être déplacée sur un autre écran (sans être réduite) pendant la mesure.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use glam::Vec3;

use crate::camera::OrbitCamera;
use crate::ui::{GraphicsQuality, LodDistanceSetting, UiState};

/// Graine par défaut : la même planète d'un banc d'essai à l'autre
pub const DEFAULT_SEED: u64 = 2026;
/// Durées par défaut de la chauffe et de la mesure de chaque phase, en secondes
const WARMUP_SECS: f32 = 3.0;
const MEASURE_SECS: f32 = 8.0;
/// Mise en route avant la première mesure : relief, nuages et textures se chargent
const STARTUP_SECS: f32 = 20.0;
/// Bêtes ajoutées autour du joueur pour la phase « beaucoup de créatures »
pub const STRESS_CREATURES: usize = 60;

/// Le banc d'essai est demandé sur la ligne de commande
pub fn requested() -> bool {
    static REQUESTED: OnceLock<bool> = OnceLock::new();
    *REQUESTED.get_or_init(|| std::env::args().any(|a| a == "--benchmark"))
}

/// Ce qu'on fait des créatures pendant une phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureMode {
    /// Simulées, animées et dessinées (le jeu normal)
    Full,
    /// Simulées seulement : ni animation, ni maillage, ni dessin
    SimulationOnly,
    /// Ni simulées, ni dessinées
    Off,
    /// Comme `Full`, avec `STRESS_CREATURES` bêtes de plus autour du joueur
    Stress,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    FirstPerson,
    ThirdPerson,
    Orbit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    /// Immobile, la tête tourne d'un tour complet pendant la mesure
    Still,
    /// Marche en sprintant droit devant (en sautant pour franchir les marches)
    Walk,
    /// Vol sprinté droit devant (~29 blocs/s) : le relief se charge sans arrêt
    Fly,
}

/// Réglages d'une phase
#[derive(Debug, Clone, Copy)]
struct Setup {
    shadows: bool,
    cloud_shadows: bool,
    clouds: bool,
    god_rays: bool,
    bloom: bool,
    creatures: CreatureMode,
    lod: LodDistanceSetting,
    view: View,
    motion: Motion,
    /// Préréglage dont la phase prend la finesse des ombres et des rayons (qui dépend de la
    /// carte graphique) ; `None` : réglages par défaut de cette carte
    preset: Option<GraphicsQuality>,
    /// Inverse la finesse des ombres du relief, des rayons, ou le MSAA par rapport à la
    /// référence (allégés et sans MSAA par défaut sur une puce intégrée, fins et avec ailleurs)
    flip_fine_shadows: bool,
    flip_fine_god_rays: bool,
    flip_msaa: bool,
}

impl Setup {
    /// La référence, avec les effets d'un préréglage de qualité
    fn quality(quality: GraphicsQuality) -> Setup {
        // Seule la finesse dépend de la carte, elle est lue au début de la phase
        let e = quality.effects(false);
        Setup {
            shadows: e.shadows,
            cloud_shadows: e.cloud_shadows,
            god_rays: e.god_rays,
            bloom: e.bloom,
            lod: e.lod_distance,
            preset: Some(quality),
            ..Setup::REFERENCE
        }
    }

    const REFERENCE: Setup = Setup {
        shadows: true,
        cloud_shadows: true,
        clouds: true,
        god_rays: true,
        bloom: true,
        creatures: CreatureMode::Full,
        lod: LodDistanceSetting::Normale,
        view: View::FirstPerson,
        motion: Motion::Still,
        preset: None,
        flip_fine_shadows: false,
        flip_fine_god_rays: false,
        flip_msaa: false,
    };
}

struct Phase {
    name: &'static str,
    /// Ce que la phase coupe ou ajoute, pour la lecture du rapport
    what: &'static str,
    setup: Setup,
    /// Comparable à la référence : seule une chose change, au même endroit, même regard
    comparable: bool,
    /// Phase de référence (mesurée au début et à la fin : l'écart dit si la mesure a dérivé)
    reference: bool,
    /// Mise en route, non mesurée
    startup: bool,
}

fn phases() -> Vec<Phase> {
    let r = Setup::REFERENCE;
    let p = |name, what, setup, comparable| Phase { name, what, setup, comparable, reference: false, startup: false };
    vec![
        Phase { startup: true, ..p("Mise en route (non mesurée)", "", r, false) },
        Phase { reference: true, ..p("Référence (tout activé)", "", r, false) },
        p("Sans ombres du relief", "ombres portées du relief (coupe aussi les ombres des nuages et les rayons, qui en dépendent)", Setup { shadows: false, ..r }, true),
        p("Sans ombres des nuages", "ombres des nuages", Setup { cloud_shadows: false, ..r }, true),
        p("Sans nuages", "nuages", Setup { clouds: false, ..r }, true),
        p("Sans rayons crépusculaires", "rayons crépusculaires", Setup { god_rays: false, ..r }, true),
        p("Sans bloom", "bloom", Setup { bloom: false, ..r }, true),
        p(
            "Ombres fines ↔ allégées",
            "bascule des ombres du relief entre fines et allégées (sur puce intégrée, la référence est allégée : un coût négatif est ce qu'ajoutent les ombres fines)",
            Setup { flip_fine_shadows: true, ..r },
            true,
        ),
        p(
            "Rayons fins ↔ allégés",
            "bascule des rayons crépusculaires entre fins et allégés (sur puce intégrée, la référence est allégée : un coût négatif est ce qu'ajoutent les rayons fins)",
            Setup { flip_fine_god_rays: true, ..r },
            true,
        ),
        p(
            "MSAA basculé",
            "bascule du MSAA 4× (coupé par défaut sur puce intégrée : un coût négatif est ce qu'ajoute le MSAA)",
            Setup { flip_msaa: true, ..r },
            true,
        ),
        p("Créatures simulées, non dessinées", "animation et dessin des créatures", Setup { creatures: CreatureMode::SimulationOnly, ..r }, true),
        p("Sans créatures", "créatures (simulation, animation et dessin)", Setup { creatures: CreatureMode::Off, ..r }, true),
        p("Distance LOD courte", "passage de la distance LOD normale à courte", Setup { lod: LodDistanceSetting::Courte, ..r }, true),
        p("Distance LOD élevée", "passage de la distance LOD normale à élevée", Setup { lod: LodDistanceSetting::Elevee, ..r }, true),
        p(
            "Minimum (tout coupé)",
            "tous les effets et les créatures",
            Setup { shadows: false, cloud_shadows: false, clouds: false, god_rays: false, bloom: false, creatures: CreatureMode::Off, ..r },
            true,
        ),
        p("Qualité moyenne", "passage de la qualité haute à moyenne", Setup::quality(GraphicsQuality::Moyenne), true),
        p("Qualité basse", "passage de la qualité haute à basse", Setup::quality(GraphicsQuality::Basse), true),
        p("Beaucoup de créatures (+60)", "60 créatures de plus (coût négatif : ce qu'elles ajoutent)", Setup { creatures: CreatureMode::Stress, ..r }, true),
        p("Vue à la troisième personne", "", Setup { view: View::ThirdPerson, ..r }, false),
        p("Vue satellite", "", Setup { view: View::Orbit, ..r }, false),
        p("Marche (sprint)", "", Setup { motion: Motion::Walk, ..r }, false),
        p("Vol rapide", "", Setup { motion: Motion::Fly, ..r }, false),
        Phase { reference: true, ..p("Référence (contrôle, en fin de banc)", "", r, false) },
    ]
}

/// Ce que la boucle principale doit faire au début d'une phase
pub struct PhaseStart {
    /// Recréer la population de créatures (même graine : même point de départ à chaque phase)
    pub reset_creatures: bool,
    /// Bêtes à ajouter autour du joueur
    pub extra_creatures: usize,
}

/// Temps GPU par passe, lus de façon asynchrone par `gpu_timer` (en nanosecondes)
static GPU_SAMPLES: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());
/// Informations sur la machine pour l'en-tête du rapport
static META: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Enregistre le temps GPU d'une passe (appelé par `gpu_timer` quand une image est relue)
pub fn record_gpu(name: &'static str, ns: u64) {
    if requested() {
        if let Ok(mut samples) = GPU_SAMPLES.lock() {
            samples.push((name, ns));
        }
    }
}

/// Information affichée en tête du rapport (carte graphique, mode de présentation…)
pub fn set_meta(key: &str, value: String) {
    if let Ok(mut meta) = META.lock() {
        meta.retain(|(k, _)| k != key);
        meta.push((key.to_string(), value));
    }
}

/// Mesures d'une phase
#[derive(Default)]
struct Stats {
    frames: Vec<f32>,
    /// Temps CPU cumulé de chaque étape de l'image, en secondes
    sections: BTreeMap<&'static str, f64>,
    /// Temps GPU cumulé et nombre d'images relues, par passe
    gpu: BTreeMap<&'static str, (f64, u32)>,
    creatures: usize,
    vertices: usize,
}

impl Stats {
    fn mean_ms(&self) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        self.frames.iter().sum::<f32>() / self.frames.len() as f32 * 1000.0
    }

    fn percentile_ms(&self, p: f32) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        let mut sorted = self.frames.clone();
        sorted.sort_by(f32::total_cmp);
        sorted[((sorted.len() - 1) as f32 * p).round() as usize] * 1000.0
    }

    fn section_ms(&self, name: &str) -> f32 {
        self.sections.get(name).map_or(0.0, |t| (*t / self.frames.len().max(1) as f64 * 1000.0) as f32)
    }

    fn gpu_ms(&self, name: &str) -> Option<f32> {
        self.gpu.get(name).filter(|(_, n)| *n > 0).map(|(t, n)| (*t / *n as f64 / 1e6) as f32)
    }
}

/// Étapes de l'image mesurées côté CPU, dans l'ordre du rapport
pub const SECTIONS: [&str; 11] = [
    "caméra et joueur",
    "créatures : simulation",
    "créatures : animation et maillage",
    "LOD et envoi au GPU",
    "interface",
    "rendu : attente d'une image libre",
    "rendu : préparation",
    "rendu : envoi au GPU",
    "rendu : présentation",
    "reste de l'image",
    "hors image (système, fenêtre)",
];

/// Au-delà, une image est un à-coup : signalée dans la console et détaillée dans le rapport
pub const HITCH_MS: f32 = 100.0;

/// Chronométrage des étapes de chaque image, actif en permanence (quelques `Instant::now` par
/// image) : il explique les à-coups dans la console, et nourrit le banc d'essai
#[derive(Default)]
pub struct FrameProbe {
    sections: Vec<(&'static str, Duration)>,
    last_end: Option<Instant>,
}

/// Une image chronométrée
pub struct FrameRecord {
    /// Étapes de l'image, avec « reste de l'image » et « hors image » (temps passé depuis la fin
    /// de l'image précédente : événements de la fenêtre, système)
    pub sections: Vec<(&'static str, Duration)>,
}

impl FrameRecord {
    pub fn total(&self) -> Duration {
        self.sections.iter().map(|(_, d)| *d).sum()
    }

    /// Les étapes les plus longues, en clair
    pub fn describe(&self) -> String {
        let mut parts: Vec<&(&str, Duration)> = self.sections.iter().filter(|(_, d)| d.as_secs_f32() * 1000.0 >= 1.0).collect();
        parts.sort_by(|a, b| b.1.cmp(&a.1));
        parts.iter().map(|(n, d)| format!("{n} {:.0} ms", d.as_secs_f32() * 1000.0)).collect::<Vec<_>>().join(", ")
    }
}

impl FrameProbe {
    /// Ajoute le temps d'une étape de l'image en cours, commencée à `since`
    pub fn section(&mut self, name: &'static str, since: Instant) {
        self.sections.push((name, since.elapsed()));
    }

    /// Ajoute une étape déjà mesurée
    pub fn push(&mut self, name: &'static str, duration: Duration) {
        self.sections.push((name, duration));
    }

    /// Termine l'image commencée à `frame_start`
    pub fn finish(&mut self, frame_start: Instant) -> FrameRecord {
        let end = Instant::now();
        let mut sections = std::mem::take(&mut self.sections);
        let accounted: Duration = sections.iter().map(|(_, d)| *d).sum();
        sections.push(("reste de l'image", end.duration_since(frame_start).saturating_sub(accounted)));
        if let Some(last) = self.last_end.replace(end) {
            sections.push(("hors image (système, fenêtre)", frame_start.saturating_duration_since(last)));
        }
        FrameRecord { sections }
    }
}

/// Une des pires images du banc d'essai
struct Hitch {
    phase: &'static str,
    /// Secondes depuis le début de la phase
    at: f32,
    total_ms: f32,
    detail: String,
}

/// Pires images gardées pour le rapport
const WORST_KEPT: usize = 12;

pub struct Benchmark {
    phases: Vec<Phase>,
    index: usize,
    started_at: Instant,
    measuring: bool,
    stats: Vec<Stats>,
    /// Position de départ du joueur et heure (soleil au zénith au-dessus de lui)
    start_pos: Vec3,
    pub sun_time: f32,
    warmup: f32,
    measure: f32,
    /// Pires images de tout le banc d'essai (mise en route comprise)
    worst: Vec<Hitch>,
    pending_start: bool,
    pub seed: u64,
    began: std::time::SystemTime,
}

impl Benchmark {
    /// Banc d'essai prêt à partir de la position actuelle du joueur
    pub fn new(camera: &OrbitCamera, seed: u64) -> Self {
        let arg = |name: &str| {
            let args: Vec<String> = std::env::args().collect();
            args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|s| s.parse::<f32>().ok())
        };
        let up = camera.player_pos.normalize_or_zero();
        // Heure où le soleil est le plus haut au-dessus du point de départ
        let sun_time = (0..1200).map(|t| t as f32).max_by(|a, b| crate::renderer::compute_sun_dir(*a).dot(up).total_cmp(&crate::renderer::compute_sun_dir(*b).dot(up))).unwrap_or(0.0);
        let phases = phases();
        log::info!("BANC D'ESSAI : {} phases, environ {:.0} s. Ne touchez à rien.", phases.len(), STARTUP_SECS + (phases.len() - 1) as f32 * (WARMUP_SECS + MEASURE_SECS));
        Self {
            stats: phases.iter().map(|_| Stats::default()).collect(),
            phases,
            index: 0,
            started_at: Instant::now(),
            measuring: false,
            start_pos: camera.player_pos,
            sun_time,
            warmup: arg("--benchmark-chauffe").unwrap_or(WARMUP_SECS),
            measure: arg("--benchmark-mesure").unwrap_or(MEASURE_SECS),
            worst: Vec::new(),
            pending_start: true,
            seed,
            began: std::time::SystemTime::now(),
        }
    }

    fn setup(&self) -> Setup {
        self.phases[self.index].setup
    }

    pub fn creature_mode(&self) -> CreatureMode {
        self.setup().creatures
    }

    /// À appeler en début d'image, avant la mise à jour de la caméra : impose les réglages et les
    /// entrées de la phase. Renvoie de quoi préparer les créatures quand une phase commence.
    pub fn before_frame(&mut self, dt: f32, camera: &mut OrbitCamera, ui: &mut UiState, generator: &herocraft_world::PlanetWorldGenerator) -> Option<PhaseStart> {
        let setup = self.setup();
        // Réglages : imposés à chaque image (un clic dans le menu ne fausse pas la mesure)
        ui.video_settings.vsync = false;
        ui.video_settings.fps_limit = crate::ui::FpsLimitSetting::Unlimited;
        ui.video_settings.lod_distance = setup.lod;
        ui.shadow_settings.enabled = setup.shadows;
        ui.shadow_settings.cloud_shadows = setup.cloud_shadows;
        ui.cloud_settings.density = if setup.clouds { 1.0 } else { 0.0 };
        ui.atmosphere_settings.god_rays = setup.god_rays;
        ui.post_settings.bloom_enabled = setup.bloom;
        let integrated = ui.integrated_gpu;
        let (fine_shadows, fine_god_rays) = match setup.preset {
            Some(q) => {
                let e = q.effects(integrated);
                (e.fine_shadows, e.fine_god_rays)
            }
            None => (!integrated, !integrated),
        };
        ui.shadow_settings.fine = fine_shadows != setup.flip_fine_shadows;
        ui.atmosphere_settings.god_rays_fine = fine_god_rays != setup.flip_fine_god_rays;
        ui.video_settings.msaa = !integrated != setup.flip_msaa;
        ui.lighting_settings.moon_speed = 0.0;
        if ui.is_settings_open {
            ui.set_settings_open(false);
        }
        if ui.inventory_open {
            ui.set_inventory_open(false);
        }

        let start = std::mem::take(&mut self.pending_start).then(|| {
            log::info!("Banc d'essai, phase {}/{} : {}", self.index + 1, self.phases.len(), self.phases[self.index].name);
            // Chaque phase repart du même endroit, du même regard
            camera.release_inputs();
            if camera.is_flying {
                camera.toggle_flight();
            }
            camera.player_pos = self.start_pos;
            camera.vertical_velocity = 0.0;
            camera.player_yaw = 0.0;
            camera.player_pitch = -0.05;
            camera.sync_orbit_to_ground();
            let distance = match setup.view {
                View::FirstPerson => 0.0,
                View::ThirdPerson => 1.2,
                View::Orbit => camera.base_radius * 6.6,
            };
            camera.target_distance = distance;
            camera.distance = distance;
            if setup.motion == Motion::Fly {
                camera.toggle_flight();
            }
            camera.update_camera_bounds(generator);
            PhaseStart {
                reset_creatures: true,
                extra_creatures: if setup.creatures == CreatureMode::Stress { STRESS_CREATURES } else { 0 },
            }
        });

        // Entrées de la phase
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let turn = std::f32::consts::TAU / self.measure.max(0.1) * dt;
        match setup.motion {
            Motion::Still => match setup.view {
                View::Orbit => camera.orbit_yaw += turn * 0.25,
                _ => camera.look(turn, 0.0),
            },
            Motion::Walk => {
                camera.move_forward = true;
                camera.is_sprinting = true;
                camera.jump_requested = true;
            }
            Motion::Fly => {
                camera.move_forward = true;
                camera.is_sprinting = true;
                // Monte un peu au départ pour passer au-dessus des collines
                camera.ascend_held = elapsed < 1.5;
            }
        }
        // Le joueur ne s'épuise pas : la vitesse reste la même d'une phase à l'autre
        camera.sprint_exhausted = false;
        start
    }

    /// À appeler en fin d'image. Renvoie vrai quand le banc d'essai est terminé.
    pub fn after_frame(&mut self, dt: f32, record: &FrameRecord, creatures: usize, creature_vertices: usize) -> bool {
        let gpu: Vec<(&'static str, u64)> = GPU_SAMPLES.lock().map(|mut s| std::mem::take(&mut *s)).unwrap_or_default();
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let startup = self.phases[self.index].startup;
        let total_ms = record.total().as_secs_f32() * 1000.0;
        if self.worst.len() < WORST_KEPT || self.worst.last().is_some_and(|w| total_ms > w.total_ms) {
            self.worst.push(Hitch { phase: self.phases[self.index].name, at: elapsed, total_ms, detail: record.describe() });
            self.worst.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
            self.worst.truncate(WORST_KEPT);
        }
        let (warmup, measure) = if startup { (STARTUP_SECS.max(self.warmup), 0.0) } else { (self.warmup, self.measure) };
        if startup {
            // Mise en route : rien n'est mesuré
        } else if !self.measuring && elapsed >= warmup {
            self.measuring = true;
        } else if self.measuring {
            let stats = &mut self.stats[self.index];
            stats.frames.push(dt);
            for (name, d) in &record.sections {
                *stats.sections.entry(name).or_default() += d.as_secs_f64();
            }
            for (name, ns) in gpu {
                let entry = stats.gpu.entry(name).or_default();
                entry.0 += ns as f64;
                entry.1 += 1;
            }
            stats.creatures = stats.creatures.max(creatures);
            stats.vertices = stats.vertices.max(creature_vertices);
        }
        if elapsed >= warmup + measure {
            self.index += 1;
            self.measuring = false;
            self.pending_start = true;
            self.started_at = Instant::now();
            if self.index >= self.phases.len() {
                self.index = self.phases.len() - 1;
                self.write_report();
                return true;
            }
        }
        false
    }

    fn write_report(&self) {
        let stamp = self.began.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let dir = PathBuf::from("benchmark");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("Dossier {} impossible à créer : {e}", dir.display());
            return;
        }
        let md = dir.join(format!("rapport_{stamp}.md"));
        let csv = dir.join(format!("mesures_{stamp}.csv"));
        let (report, table) = (self.markdown(), self.csv());
        for (path, text) in [(&md, report), (&csv, table)] {
            match std::fs::write(path, text) {
                Ok(()) => log::info!("Banc d'essai : {} écrit.", path.display()),
                Err(e) => log::warn!("Banc d'essai : {} impossible à écrire : {e}", path.display()),
            }
        }
    }

    /// Phases mesurées, avec leurs mesures
    fn measured(&self) -> impl Iterator<Item = (&Phase, &Stats)> {
        self.phases.iter().zip(&self.stats).filter(|(p, _)| !p.startup)
    }

    fn gpu_passes(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.stats.iter().flat_map(|s| s.gpu.keys().copied()).collect();
        names.sort();
        names.dedup();
        names
    }

    fn markdown(&self) -> String {
        let mut out = String::new();
        let references: Vec<f32> = self.phases.iter().zip(&self.stats).filter(|(p, _)| p.reference).map(|(_, s)| s.mean_ms()).collect();
        let ref_ms = references.iter().sum::<f32>() / references.len().max(1) as f32;
        let drift = references.iter().copied().fold(f32::MIN, f32::max) - references.iter().copied().fold(f32::MAX, f32::min);
        let _ = writeln!(out, "# Banc d'essai HeroCraft\n");
        if cfg!(debug_assertions) {
            let _ = writeln!(
                out,
                "> **Attention : jeu compilé en debug.** Le code du jeu y tourne 5 à 10 fois plus lentement qu'en release. Relancer avec `cargo run --release --bin herocraft_client -- --benchmark` pour des chiffres qui veulent dire quelque chose.\n"
            );
        }
        let _ = writeln!(out, "- Compilation : {}", if cfg!(debug_assertions) { "debug" } else { "release" });
        for (k, v) in META.lock().map(|m| m.clone()).unwrap_or_default() {
            let _ = writeln!(out, "- {k} : {v}");
        }
        let _ = writeln!(out, "- Graine : {} ; chauffe {:.0} s puis mesure {:.0} s par phase", self.seed, self.warmup, self.measure);
        let _ = writeln!(out, "- V-Sync et limite de FPS coupées pendant la mesure\n");

        let _ = writeln!(out, "## Images par phase\n");
        let _ = writeln!(out, "| Phase | FPS | ms moy. | p50 | p95 | p99 | max | > 16,7 ms | créatures | Écart avec la référence |");
        let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
        for (phase, s) in self.measured() {
            let mean = s.mean_ms();
            let slow = s.frames.iter().filter(|&&f| f > 1.0 / 60.0).count() as f32 / s.frames.len().max(1) as f32 * 100.0;
            let delta = if phase.reference { "—".to_string() } else { format!("{:+.2} ms", mean - ref_ms) };
            let _ = writeln!(
                out,
                "| {} | {:.0} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} % | {} | {} |",
                phase.name,
                if mean > 0.0 { 1000.0 / mean } else { 0.0 },
                mean,
                s.percentile_ms(0.5),
                s.percentile_ms(0.95),
                s.percentile_ms(0.99),
                s.percentile_ms(1.0),
                slow,
                s.creatures,
                delta
            );
        }

        // Ce que coûte chaque fonction : temps gagné en la coupant
        let mut costs: Vec<(&str, f32)> = self.phases.iter().zip(&self.stats).filter(|(p, _)| p.comparable).map(|(p, s)| (p.what, ref_ms - s.mean_ms())).collect();
        costs.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = writeln!(out, "\n## Ce que coûte chaque fonction\n");
        let _ = writeln!(
            out,
            "Temps gagné par image en la coupant, par rapport à la moyenne des deux références ({ref_ms:.2} ms, {:.0} FPS). Les deux références, au début et à la fin, diffèrent de {drift:.2} ms : un écart plus petit que celui-là n'est pas significatif.\n",
            if ref_ms > 0.0 { 1000.0 / ref_ms } else { 0.0 }
        );
        for (what, cost) in costs {
            let share = if ref_ms > 0.0 { cost / ref_ms * 100.0 } else { 0.0 };
            let _ = writeln!(out, "- {what} : {cost:+.2} ms ({share:+.0} % de l'image)");
        }

        let _ = writeln!(out, "\n## Temps CPU par étape (ms par image)\n");
        let _ = writeln!(out, "| Phase | {} |", SECTIONS.join(" | "));
        let _ = writeln!(out, "|---|{}", "---:|".repeat(SECTIONS.len()));
        for (phase, s) in self.measured() {
            let cells: Vec<String> = SECTIONS.iter().map(|n| format!("{:.2}", s.section_ms(n))).collect();
            let _ = writeln!(out, "| {} | {} |", phase.name, cells.join(" | "));
        }
        let _ = writeln!(
            out,
            "\nQuand « attente d'une image libre » ou « présentation » domine, c'est le GPU qui limite. « hors image » est le temps passé entre deux images (événements de la fenêtre, système)."
        );

        let _ = writeln!(out, "\n## Pires images (à-coups)\n");
        let _ = writeln!(out, "Les {} images les plus longues de tout le banc d'essai, mise en route comprise, et leurs étapes de plus d'1 ms.\n", self.worst.len());
        let _ = writeln!(out, "| Durée | Phase | À | Détail |");
        let _ = writeln!(out, "|---:|---|---:|---|");
        for w in &self.worst {
            let _ = writeln!(out, "| {:.0} ms | {} | {:.1} s | {} |", w.total_ms, w.phase, w.at, w.detail);
        }

        let passes = self.gpu_passes();
        let _ = writeln!(out, "\n## Temps GPU par passe (ms par image)\n");
        if passes.is_empty() {
            let _ = writeln!(out, "La carte ou le pilote ne donne pas les horodatages GPU (timestamp queries) : temps GPU indisponible.");
        } else {
            let _ = writeln!(out, "| Phase | {} |", passes.iter().map(|p| p.trim_start_matches("gpu::")).collect::<Vec<_>>().join(" | "));
            let _ = writeln!(out, "|---|{}", "---:|".repeat(passes.len()));
            for (phase, s) in self.measured() {
                let cells: Vec<String> = passes.iter().map(|p| s.gpu_ms(p).map_or("—".into(), |v| format!("{v:.2}"))).collect();
                let _ = writeln!(out, "| {} | {} |", phase.name, cells.join(" | "));
            }
        }
        out
    }

    fn csv(&self) -> String {
        let passes = self.gpu_passes();
        let mut out = String::from("phase;images;ms_moy;p50;p95;p99;max;creatures;sommets_creatures");
        for s in SECTIONS {
            let _ = write!(out, ";cpu {s}");
        }
        for p in &passes {
            let _ = write!(out, ";{p}");
        }
        out.push('\n');
        for (phase, s) in self.measured() {
            let _ = write!(
                out,
                "{};{};{:.3};{:.3};{:.3};{:.3};{:.3};{};{}",
                phase.name,
                s.frames.len(),
                s.mean_ms(),
                s.percentile_ms(0.5),
                s.percentile_ms(0.95),
                s.percentile_ms(0.99),
                s.percentile_ms(1.0),
                s.creatures,
                s.vertices
            );
            for n in SECTIONS {
                let _ = write!(out, ";{:.3}", s.section_ms(n));
            }
            for p in &passes {
                let _ = write!(out, ";{}", s.gpu_ms(p).map_or(String::new(), |v| format!("{v:.3}")));
            }
            out.push('\n');
        }
        out
    }
}

/// Positions où poser `count` bêtes en cercles autour de `feet` (entre 5 et 20 blocs), un peu
/// au-dessus du sol pour qu'elles s'y posent en tombant
pub fn stress_positions(feet: Vec3, block: f32, count: usize) -> Vec<Vec3> {
    let up = feet.normalize_or_zero();
    let (east, north) = herocraft_common::tangent_frame(up);
    (0..count)
        .map(|i| {
            let angle = i as f32 * 2.399_963;
            let radius = 5.0 + 15.0 * (i as f32 / count.max(1) as f32);
            feet + (north * angle.cos() + east * angle.sin()) * radius * block + up * 2.0 * block
        })
        .collect()
}

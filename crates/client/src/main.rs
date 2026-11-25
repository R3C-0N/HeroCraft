use std::sync::Arc;
use std::time::Instant;

use log::{info, warn};
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

// Maillage du terrain : crate à part, mesurable avec `cargo bench -p herocraft_meshing`
use herocraft_meshing::{block_light, block_textures, planet_mesh, voxel_mesh};
// Passes de rendu GPU et réglages graphiques : crate herocraft_render
use herocraft_render::{
    atmosphere, block_cracks, block_outline, cloud_mesh, post, precipitation, shaders, shadow, skinned_model, terrain_cull,
    terrain_pool,
};
// Briques de l'interface egui : crate herocraft_ui
use herocraft_ui::{chat, commands, item_icons, skill_bar, skills_screen, ui_skin, world_map};
#[cfg(test)]
use herocraft_ui::ui_snapshot;
// Son synthétisé : crate herocraft_audio
use herocraft_audio as audio;
mod app;
mod benchmark;
mod block_interaction;
mod camera;
mod paper_cards;
mod cinematic;
mod collision;
mod creature_render;
mod creatures;
mod dropped_items;
mod effects;
mod gpu_timer;
mod hero_creator;
mod gestures;
mod hero_model;
mod villagers;
mod input;
mod inventory;
mod keyboard_hook;
mod lod;
mod monitor;
mod network;
mod player_stats;
mod renderer;
mod skill_models;
mod skill_play;
mod ui;
mod vision_assault;
mod vision_charge;
mod vision_tutorial;
mod void_stones;
mod weather;

use block_interaction::BlockInteraction;
use camera::OrbitCamera;
use input::Action;
use lod::PlanetLodManager;
use network::{HostedServer, NetClient, NetEvent, PendingJoin};
use herocraft_common::lan::LanBrowser;
use renderer::State;
use ui::{MenuPage, NetStatus, UiAction, UiState};


/// Allocateur mimalloc : les fils de génération et de maillage allouent beaucoup de petits `Vec`
/// en parallèle, cas où l'allocateur du système (surtout celui de Windows) est le plus lent
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    // Variables du fichier `.env` (sans écraser celles déjà définies dans l'environnement)
    let _ = dotenvy::dotenv();
    // Niveau `info` par défaut, `RUST_LOG` (ou le `.env`) le remplace s'il est défini
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    herocraft_common::profiler::init_main_thread();
    if cfg!(debug_assertions) {
        warn!("Jeu compilé en DEBUG : il tourne 5 à 10 fois plus lentement qu'en release. Pour jouer : cargo run --release --bin herocraft_client");
    }
    if herocraft_common::profiler::is_enabled() {
        info!("PROFILAGE ACTIF : rapport écrit dans ./profiling/ à la fermeture (ou avec [F9]).");
    }

    info!("==========================================================");
    info!("   HeroCraft — Moteur Planétaire QuadTree LOD (Rust / WGPU) ");
    info!("==========================================================");
    info!("Contrôles (modifiables dans Échap > Commandes) :");
    info!(" • Souris                             : Regarder autour (curseur capturé en jeu)");
    info!(" • Clic gauche / Clic droit           : Casser le bloc visé (il tombe, à ramasser) / Poser le bloc choisi");
    info!(" • [1-9] / Molette / [F]              : Choisir la case de la barre d'objets / Inventaire");
    info!(" • [A,E,R,T] (AZERTY) / [Q,E,R,T]     : Compétences de la barre (posées depuis la roue)");
    info!(" • [Z,Q,S,D] (AZERTY) / [W,A,S,D]     : Se déplacer");
    info!(" • [ESPACE] / double [ESPACE]         : Sauter / S'envoler (monter en vol)");
    info!(" • [MAJ] / [CTRL]                     : S'accroupir (descendre en vol) / Sprinter");
    info!(" • [ALT] + Molette                    : Zoom FPS (0m) -> TPS (1-25m) -> Satellite");
    info!(" • [V] / [B]                          : Vue sol <-> satellite / Réinitialiser la vue");
    info!(" • [C]                                : Roue des compétences");
    info!(" • [G] / [U] / [Y]                    : Nouvelle planète / Filtres tectonique, courants");
    info!(" • [ENTRÉE]                           : Chat (messages, commandes /help)");
    info!(" • [ÉCHAP]                            : Menu de pause (libère la souris)");
    info!("==========================================================");

    let wanted_monitor = monitor::requested();
    let event_loop = monitor::build_event_loop(wanted_monitor.is_some())?;
    // La fenêtre s'ouvre et la partie se prépare à la reprise de l'application (`resumed`)
    app::run(event_loop, wanted_monitor)
}

/// Ouvre la fenêtre du jeu, sur l'écran `wanted_monitor` s'il est demandé (`HEROCRAFT_MONITOR`)
fn create_window(event_loop: &ActiveEventLoop, wanted_monitor: Option<&str>) -> Window {
    let attributes = Window::default_attributes().with_title(WINDOW_TITLE).with_inner_size(LogicalSize::new(1280, 720));
    let window = event_loop.create_window(attributes).expect("Échec de création de la fenêtre Winit");
    if let Some(m) = wanted_monitor.and_then(|name| monitor::find(event_loop, name)) {
        window.set_outer_position(monitor::centered_position(&m, window.outer_size()));
        info!("Fenêtre ouverte sur l'écran {:?}.", m.name());
    }
    window
}

/// Titre de la fenêtre en solo
const WINDOW_TITLE: &str = "HeroCraft — Planète QuadTree LOD (Rust / WGPU)";

/// Son d'un bloc qui change : casse du bloc d'avant s'il devient de l'air, pose du nouveau sinon
fn block_sound(
    audio: &mut audio::Audio,
    previous: Option<herocraft_common::BlockId>,
    block: herocraft_common::BlockId,
    distance_blocks: f32,
    settings: &audio::AudioSettings,
) {
    use audio::synth::Material;
    if block.is_air() {
        if let Some(previous) = previous.filter(|b| !b.is_air()) {
            audio.block_broken(Material::of(previous), distance_blocks, settings);
        }
    } else {
        audio.block_placed(Material::of(block), distance_blocks, settings);
    }
}

/// Pas, saut et réception du joueur au sol, puis ambiance (vent, oiseaux, grillons)
fn update_sound(
    audio: &mut audio::Audio,
    camera: &OrbitCamera,
    generator: &herocraft_world::PlanetWorldGenerator,
    state: &State,
    ui_state: &UiState,
    dt: f32,
    sky_check_in: &mut f32,
    sky_covered: &mut bool,
    weather: &weather::WeatherVisual,
) {
    let settings = &ui_state.audio_settings;
    let ground_view = camera.distance <= camera.base_radius;
    let block_height = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    if ground_view && !ui_state.is_settings_open {
        let surface = camera.surface_block(generator).map_or(audio::synth::Material::Grass, audio::synth::Material::of);
        let player = audio::events::PlayerState {
            position: camera.player_pos,
            grounded: camera.is_grounded,
            flying: camera.is_flying,
            sprinting: camera.is_sprinting,
            vertical_speed: camera.vertical_velocity / block_height,
            block_height,
        };
        audio.update_player(player, surface, settings);
    } else {
        audio.reset_player();
    }

    *sky_check_in -= dt;
    if *sky_check_in <= 0.0 {
        *sky_check_in = 0.5;
        *sky_covered = ground_view && camera.sky_covered(generator);
    }
    let up = camera.player_pos.normalize_or_zero();
    let height_blocks = if ground_view && camera.is_flying {
        ((camera.player_pos.length() - camera.ground_radius_at(up, generator)) / block_height).max(0.0)
    } else {
        0.0
    };
    let input = audio::AmbienceInput {
        ground_view,
        paused: ui_state.is_settings_open,
        height_blocks,
        covered: *sky_covered,
        is_day: state.sun_dir().dot(up) > -0.05,
        rain: weather.precipitation * (1.0 - weather.snow),
        wind: weather.wind,
    };
    audio.update_ambience(dt, input, settings);
}

/// Horloge du ciel et temps imposé du monde solo, enregistrés avec lui
fn save_sky_clock(seed: u64, state: &State, weather: &weather::ClientWeather) {
    let clock = herocraft_world::SkyClock { time: state.simulated_time as f64, forced: weather.model.forced };
    if let Err(e) = clock.save(solo_save_dir(seed)) {
        warn!("Échec de l'enregistrement de l'horloge du ciel : {e}");
    }
}

/// Dossier de sauvegarde d'un monde solo (aussi celui du serveur qui l'ouvre au réseau local)
fn solo_save_dir(seed: u64) -> String {
    herocraft_world::save_dir_name("monde", seed)
}

/// Inventaire du joueur sur une planète : celui du monde solo, ou un inventaire à part pour la
/// planète d'un serveur rejoint
fn inventory_file(seed: u64, online: bool) -> std::path::PathBuf {
    let dir = if online { herocraft_world::save_dir_name("en_ligne", seed) } else { solo_save_dir(seed) };
    std::path::Path::new(&dir).join("inventaire.json")
}

/// Progression du héros (niveau, expérience, points, compétences) d'un monde solo
fn progress_file(seed: u64) -> std::path::PathBuf {
    std::path::Path::new(&solo_save_dir(seed)).join("progression.json")
}

/// Légende du héros sur une planète, à côté de son inventaire
fn legend_file(seed: u64, online: bool) -> std::path::PathBuf {
    inventory_file(seed, online).with_file_name("heros.json")
}

/// La légende enregistrée, si le héros a été baptisé dans ce monde (une légende qui ne passe plus
/// la vérification est ignorée : le créateur se rouvre)
fn load_legend(file: &std::path::Path) -> Option<herocraft_common::hero::Legend> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut legend: herocraft_common::hero::Legend = match serde_json::from_str(&text) {
        Ok(legend) => legend,
        Err(e) => {
            warn!("Légende illisible dans {} : {}", file.display(), e);
            return None;
        }
    };
    match legend.verify() {
        Ok(()) => Some(legend),
        Err(e) => {
            warn!("Légende de {} refusée : {}", file.display(), e);
            None
        }
    }
}

fn save_legend(file: &std::path::Path, legend: &herocraft_common::hero::Legend) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(legend).map_err(std::io::Error::other)?;
    std::fs::write(file, text)
}

/// Taille du héros en mètres (celle d'un aventurier sans légende par défaut)
fn hero_height(ui_state: &UiState) -> f32 {
    ui_state.legend.as_ref().map_or(camera::PLAYER_HEIGHT, |l| l.appearance.taille as f32)
}

/// Stockage d'un monde solo et ses blocs modifiés, repris de la sauvegarde
fn solo_terrain(seed: u64) -> (Option<herocraft_voxel_engine::ChunkStorage>, Arc<herocraft_world::TerrainEdits>) {
    let storage = herocraft_voxel_engine::ChunkStorage::new(solo_save_dir(seed)).ok();
    let edits = Arc::new(herocraft_world::TerrainEdits::new(storage.clone()));
    (storage, edits)
}

/// Le tutoriel de la vision commence aux pieds `feet` : le soldat revêt son paquetage, l'éclat
/// est posé dans sa roche, et en solo (`solo`) les compétences du présent sont mises de côté et
/// les soldats figés posés autour de lui (en ligne, le serveur tient les compétences)
fn begin_vision_tutorial(
    feet: glam::Vec3,
    camera: &OrbitCamera,
    generator: &herocraft_world::PlanetWorldGenerator,
    lod: &mut PlanetLodManager,
    ui_state: &mut UiState,
    solo: Option<(&mut herocraft_world::CreatureWorld, &herocraft_common::CreatureRegistry)>,
) -> Option<vision_tutorial::VisionTutorial> {
    let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
    let at = geometry.block_at_point(feet.as_dvec3())?;
    let entry = generator.vision_entry(at.face, at.x, at.z, VISION_TEMPLE_BLOCKS, herocraft_world::Culture::Humains)?;
    let mut tutorial = vision_tutorial::VisionTutorial::begin(entry.shard, &geometry, &mut ui_state.inventory);
    if let Some((pos, rock)) = tutorial.shard_rock() {
        lod.set_block(generator, pos, rock);
    }
    // La grande porte, fermée, et le bélier qui l'attend
    if let Some(city) = generator.memory_city(&entry.site, entry.holder) {
        let charge = vision_charge::VisionCharge::new(vision_charge::Siege::of(city, entry.site.face, generator), geometry);
        for (pos, block) in charge.initial_blocks() {
            lod.set_block(generator, pos, block);
        }
        tutorial.charge = Some(charge);
        tutorial.assault = Some(vision_assault::VisionAssault::new(city, &entry.site, geometry));
    }
    if let Some((world, registry)) = solo {
        ui_state.player.enter_vision();
        if let Some(species) = registry.find(vision_tutorial::VisionTutorial::soldier_species()) {
            let block = geometry.block_height as f32;
            let ids: Vec<herocraft_common::CreatureId> = tutorial.soldier_spots(feet, block).into_iter().map(|dir| {
                    let feet = dir * camera.ground_radius_at(dir, generator);
                    let yaw = tutorial.charge.as_ref().map_or(0.0, |c| c.facing_gate(feet));
                    world.spawn_led(species, feet, yaw)
                })
                .collect();
            info!("Tutoriel de la vision : {} soldats figés, l'éclat en {:?}.", ids.len(), entry.shard);
            tutorial.add_soldiers(ids);
        }
    }
    Some(tutorial)
}

/// Applique ce que la charge de la vision demande : blocs de la porte et du bélier, soldats qui
/// courent (solo), coups de bélier, porte qui vole en éclats, rayon violet et herse
fn apply_charge_events(
    events: Vec<vision_charge::ChargeEvent>,
    camera: &mut OrbitCamera,
    generator: &herocraft_world::PlanetWorldGenerator,
    lod: &mut PlanetLodManager,
    state: &mut State,
    solo: &mut herocraft_world::CreatureWorld,
    effects: &mut effects::Effects,
    audio: &mut audio::Audio,
    settings: &audio::AudioSettings,
    cinematics: &mut cinematic::Cinematics,
) {
    use vision_charge::ChargeEvent;
    if events.is_empty() {
        return;
    }
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let player = camera.player_pos;
    let distance = |p: glam::Vec3| player.distance(p) / block;
    let cue = |effect: &str, point: glam::Vec3, hold: Option<f32>| skill_play::VisualCue {
        effect: effect.into(),
        anchor: herocraft_common::skills::Anchor::Area,
        caster: None,
        point,
        creature: None,
        aim: glam::Vec3::ZERO,
        hold,
        area_blocks: 1.0,
        travel: None,
        delay: 0.0,
        projectile: None,
    };
    let mut remesh = false;
    for event in events {
        match event {
            ChargeEvent::Blocks(blocks) => {
                for (pos, id) in blocks {
                    remesh |= lod.set_block(generator, pos, id);
                }
            }
            ChargeEvent::March(orders) => {
                for (id, toward, pace) in orders {
                    solo.send_to(id, toward, pace);
                }
            }
            ChargeEvent::RamHit(at) => {
                audio.block_broken(audio::synth::Material::Wood, distance(at) * 0.5, settings);
                audio.block_hit(audio::synth::Material::Wood, settings);
                effects.start(vec![cue("dustPuff", at, None), cue("impactSpark", at, None)]);
                if distance(at) < 30.0 {
                    camera.hurt(0.35);
                }
            }
            ChargeEvent::GateBroken(at) => {
                for _ in 0..3 {
                    audio.block_broken(audio::synth::Material::Wood, distance(at) * 0.3, settings);
                }
                audio.thunder(0.5, settings);
                effects.start(vec![cue("groundBurst", at, None), cue("dustPuff", at, None), cue("armorShatter", at, None), cue("shockRing", at, None)]);
                camera.hurt(0.8);
                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::GATE_BROKEN).at(at));
            }
            ChargeEvent::Beam(at) => effects.start(vec![cue("rayonDuNeant", at, Some(3600.0))]),
            ChargeEvent::Sealed(at) => {
                audio.block_placed(audio::synth::Material::Stone, distance(at), settings);
                audio.thunder(0.25, settings);
                effects.start(vec![cue("dustPuff", at, None)]);
                camera.hurt(0.4);
            }
        }
    }
    if remesh {
        state.sync_lod_meshes(lod);
    }
}

/// Le tutoriel de la vision s'achève (retour au présent) : l'inventaire et les compétences du
/// présent reviennent, les soldats figés s'en vont
///
/// Rend vrai pour un souvenir achevé (la Pierre prise) : le héros garde alors la force que
/// l'éclat a éveillée et revient avec un éclat du cristal
fn end_vision_tutorial(tutorial: &mut Option<vision_tutorial::VisionTutorial>, ui_state: &mut UiState, solo: &mut herocraft_world::CreatureWorld) -> bool {
    let Some(t) = tutorial.take() else { return false };
    let completed = t.assault.as_ref().is_some_and(|a| a.completed());
    let fighters: Vec<herocraft_common::CreatureId> = t.assault.as_ref().map(|a| a.creatures().collect()).unwrap_or_default();
    for id in fighters.into_iter().chain(t.end(&mut ui_state.inventory)) {
        solo.despawn(id);
    }
    if completed {
        ui_state.player.leave_vision_with_power();
        if let Some(shard) = herocraft_common::StoneKind::Fragment.item() {
            ui_state.inventory.add(shard);
        }
    } else {
        ui_state.player.leave_vision();
    }
    ui_state.vision_hint = None;
    completed
}

/// Applique ce que l'assaut de la ville demande : effets, événements de cinématique, blocs
/// (la Pierre sur l'autel), héros repoussé vers le centre d'une place, secousses et grondements
fn apply_assault_events(
    events: Vec<vision_assault::AssaultEvent>,
    camera: &mut OrbitCamera,
    generator: &herocraft_world::PlanetWorldGenerator,
    lod: &mut PlanetLodManager,
    state: &mut State,
    effects: &mut effects::Effects,
    audio: &mut audio::Audio,
    settings: &audio::AudioSettings,
    cinematics: &mut cinematic::Cinematics,
) {
    use vision_assault::AssaultEvent;
    let mut remesh = false;
    for event in events {
        match event {
            AssaultEvent::Effect(effect, point, hold) => effects.start(vec![skill_play::VisualCue {
                effect: effect.into(),
                anchor: herocraft_common::skills::Anchor::Area,
                caster: None,
                point,
                creature: None,
                aim: glam::Vec3::ZERO,
                hold,
                area_blocks: 1.0,
                travel: None,
                delay: 0.0,
                projectile: None,
            }]),
            AssaultEvent::Notify(name, at) => {
                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(name).at(at));
            }
            AssaultEvent::Blocks(blocks) => {
                for (pos, id) in blocks {
                    remesh |= lod.set_block(generator, pos, id);
                }
            }
            AssaultEvent::Push(delta) => {
                let feet = camera.player_pos + delta;
                let dir = feet.normalize_or_zero();
                camera.player_pos = dir * camera.ground_radius_at(dir, generator).max(feet.length());
                camera.sync_orbit_to_ground();
            }
            AssaultEvent::Shake(amount) => camera.hurt(amount),
            AssaultEvent::Rumble(amount) => audio.thunder(amount, settings),
        }
    }
    if remesh {
        state.sync_lod_meshes(lod);
    }
}

/// Une vision de temple tient le monde du présent de côté
struct PresentWorld {
    /// Blocs modifiés du présent
    edits: Arc<herocraft_world::TerrainEdits>,
    /// En solo, là où le joueur se tenait avant la vision (en ligne, le serveur le sait)
    back: Option<glam::Vec3>,
}

/// Le temple de la vision est cherché à moins de tant de blocs du joueur
const VISION_TEMPLE_BLOCKS: i32 = 2048;

/// Entre dans la vision tenue par `holder` (`Some`), seul dans le passé avec des blocs modifiés
/// vierges, ou en revient (`None`) avec les blocs du présent mis de côté ; le terrain est refait et
/// le joueur posé en `feet`. `back` : en solo, là où revenir ; `storage` : sauvegarde des blocs du
/// présent, au retour en solo.
fn switch_epoch(
    holder: Option<herocraft_world::Culture>,
    feet: glam::Vec3,
    back: Option<glam::Vec3>,
    storage: Option<herocraft_voxel_engine::ChunkStorage>,
    vision: &mut Option<PresentWorld>,
    generator: &mut herocraft_world::PlanetWorldGenerator,
    chunk_cache: &mut herocraft_voxel_engine::WorldChunkCache,
    lod_manager: &mut PlanetLodManager,
    camera: &mut OrbitCamera,
    state: &mut State,
) {
    let edits = match holder {
        Some(holder) => {
            *generator = generator.with_epoch(herocraft_world::Epoch::Past(holder));
            let present = lod_manager.terrain_edits().clone();
            // Une vision dans une vision : on garde le vrai présent
            vision.get_or_insert(PresentWorld { edits: present, back });
            *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(None, 256);
            Arc::new(herocraft_world::TerrainEdits::new(None))
        }
        None => {
            *generator = generator.with_epoch(herocraft_world::Epoch::Present);
            *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
            match vision.take() {
                Some(present) => present.edits,
                None => lod_manager.terrain_edits().clone(),
            }
        }
    };
    // Le temps ne reste pas figé hors de la vision
    if holder.is_none() {
        state.time_frozen = false;
    }
    lod_manager.clear(generator.planet.seed);
    camera.set_terrain(edits.clone());
    lod_manager.set_terrain_edits(edits);
    camera.player_pos = feet;
    camera.sync_orbit_to_ground();
    lod_manager.update(camera.eye_position(), generator, chunk_cache, 0.0, None);
    state.sync_lod_meshes(lod_manager);
}

/// Remplace la planète affichée (nouvelle graine, partie en ligne ou retour au solo) : générateur,
/// blocs modifiés, caches, maillages, nuages et inventaire, puis pose le joueur au sol
fn switch_planet(
    seed: u64,
    online: bool,
    edits: Arc<herocraft_world::TerrainEdits>,
    storage: Option<herocraft_voxel_engine::ChunkStorage>,
    generator: &mut herocraft_world::PlanetWorldGenerator,
    chunk_cache: &mut herocraft_voxel_engine::WorldChunkCache,
    lod_manager: &mut PlanetLodManager,
    camera: &mut OrbitCamera,
    state: &mut State,
    ui_state: &mut UiState,
    weather: &mut weather::ClientWeather,
) {
    let desc = herocraft_world::PlanetDescriptor { seed, ..Default::default() };
    // Météo de la nouvelle planète ; en solo, son horloge reprend (en ligne, le serveur l'envoie)
    weather.reset(seed);
    if !online {
        let clock = herocraft_world::SkyClock::load(solo_save_dir(seed));
        state.simulated_time = clock.time as f32;
        weather.model.forced = clock.forced;
    }
    *generator = herocraft_world::PlanetWorldGenerator::new(desc);
    *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
    lod_manager.clear(seed);
    camera.set_terrain(edits.clone());
    lod_manager.set_terrain_edits(edits);
    camera.spawn_on_ground(generator);
    if ui_state.player.is_dead() {
        ui_state.player.respawn();
    }
    lod_manager.update(camera.lod_eye(), generator, chunk_cache, 0.0, None);
    state.sync_lod_meshes(lod_manager);
    state.update_cloud_mesh(camera.base_radius, seed);
    ui_state.set_seed(seed);
    ui_state.set_inventory_open(false);
    ui_state.inventory.save();
    ui_state.inventory = inventory::Inventory::load_or_default(inventory_file(seed, online));
    // En ligne, la progression est celle que tient le serveur
    ui_state.set_skills_open(false);
    ui_state.set_map_open(false);
    ui_state.player.save();
    ui_state.player = player_stats::PlayerStats::load_or_default((!online).then(|| progress_file(seed)));
    ui_state.set_legend(load_legend(&legend_file(seed, online)));
    camera.hero_height = hero_height(ui_state);
}

/// Touche attribuée à une action, en jeu (menu fermé) : lue par la fenêtre ou par le crochet clavier
fn game_key(
    action: Action,
    is_pressed: bool,
    first_press: bool,
    camera: &mut OrbitCamera,
    ui_state: &mut UiState,
    pending_actions: &mut Vec<UiAction>,
    jump_tap: &mut input::DoubleTap,
    zoom_modifier_held: &mut bool,
) {
    let is_ground_view = camera.target_distance <= camera.base_radius;
    if ui_state.player.is_dead() {
        // Mort : seul Saut sert, à réapparaître une fois l'écran de mort passé
        if action == Action::Jump && first_press && ui_state.player.respawn_in() == Some(0.0) {
            pending_actions.push(UiAction::Respawn);
        }
        return;
    }
    match action {
        Action::MoveForward => camera.move_forward = is_pressed,
        Action::MoveBackward => camera.move_backward = is_pressed,
        Action::MoveLeft => camera.move_left = is_pressed,
        Action::MoveRight => camera.move_right = is_pressed,
        Action::Sprint => camera.is_sprinting = is_pressed,
        Action::Descend => camera.descend_held = is_pressed,
        Action::ZoomModifier => *zoom_modifier_held = is_pressed,
        Action::Jump => {
            camera.ascend_held = is_pressed;
            if first_press {
                if !is_ground_view {
                    pending_actions.push(UiAction::ToggleAutoRotate);
                } else if jump_tap.press(Instant::now()) {
                    camera.toggle_flight();
                    info!("Double [Saut] : vol {}", if camera.is_flying { "ACTIVÉ" } else { "DÉSACTIVÉ" });
                } else {
                    camera.jump_requested = true;
                }
            }
        }
        Action::OpenChat if first_press => {
            camera.release_inputs();
            *zoom_modifier_held = false;
            ui_state.chat.open();
        }
        Action::OpenInventory if first_press && is_ground_view && ui_state.beast_attacks.is_none() => {
            camera.release_inputs();
            ui_state.set_inventory_open(true);
        }
        Action::OpenSkills if first_press && is_ground_view => {
            camera.release_inputs();
            ui_state.set_skills_open(true);
        }
        Action::OpenMap if first_press && is_ground_view => {
            camera.release_inputs();
            ui_state.set_map_open(true);
        }
        Action::ToggleView if first_press => pending_actions.push(UiAction::ToggleGroundCamera),
        Action::ResetView if first_press => pending_actions.push(UiAction::ResetCamera),
        Action::NewPlanet if first_press => pending_actions.push(UiAction::GenerateNewPlanet),
        Action::ToggleTectonics if first_press => {
            ui_state.show_tectonics = !ui_state.show_tectonics;
            if ui_state.show_tectonics {
                ui_state.show_ocean_currents = false;
            }
            pending_actions.push(UiAction::ToggleTectonics);
        }
        Action::ToggleOceanCurrents if first_press => {
            ui_state.show_ocean_currents = !ui_state.show_ocean_currents;
            if ui_state.show_ocean_currents {
                ui_state.show_tectonics = false;
            }
            pending_actions.push(UiAction::ToggleOceanCurrents);
        }
        Action::Skill1 | Action::Skill2 | Action::Skill3 | Action::Skill4 if is_ground_view && ui_state.beast_attacks.is_none() => {
            let slot = [Action::Skill1, Action::Skill2, Action::Skill3, Action::Skill4].iter().position(|&a| a == action).unwrap_or(0);
            match ui_state.skill_bar.get(slot).map(str::to_string) {
                // Une compétence qui se charge (Tir de Baliste) part au relâchement de sa touche
                Some(skill) if herocraft_common::skills().get(&skill).and_then(|d| d.cast.as_ref()).is_some_and(|c| c.charge.is_some()) => {
                    if first_press {
                        ui_state.skill_charges.push(skill.clone());
                        ui_state.skill_charging = Some(skill);
                    } else if !is_pressed && ui_state.skill_charging.as_deref() == Some(skill.as_str()) {
                        ui_state.skill_charging = None;
                        ui_state.skill_casts.push(skill);
                    }
                }
                Some(skill) if first_press => ui_state.skill_casts.push(skill),
                Some(_) => {}
                None if !first_press => {}
                None => {
                    let key = ui_state.controls.action_key_label(action);
                    let wheel = ui_state.controls.action_key_label(Action::OpenSkills);
                    ui_state.skill_notice = Some((format!("Rien sur [{key}] : posez une compétence depuis la roue [{wheel}]"), Instant::now()));
                }
            }
        }
        _ => {}
    }
}

/// Capture la souris (cachée, bloquée dans la fenêtre) ou la libère au centre de la fenêtre
fn set_cursor_captured(window: &Window, captured: bool) {
    if captured {
        // Locked n'existe pas partout (Windows, X11) : Confined garde le curseur dans la fenêtre,
        // et la vue lit de toute façon le mouvement brut de la souris
        let grab = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if let Err(e) = grab {
            warn!("Capture de la souris impossible : {}", e);
        }
        window.set_cursor_visible(false);
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        let size = window.inner_size();
        let _ = window.set_cursor_position(PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0));
        window.set_cursor_visible(true);
    }
}

/// Bête morte sous le réticule, à portée et devant le bloc visé : à dépecer
fn aimed_corpse(camera: &OrbitCamera, interaction: &BlockInteraction, views: &[creatures::CreatureView], registry: &herocraft_common::CreatureRegistry) -> Option<herocraft_common::CreatureId> {
    let (eye, dir, arm) = camera.view_ray()?;
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let reach = interaction.target().map_or(arm + block_interaction::REACH_BLOCKS as f32 * block, |hit| hit.distance as f32);
    creatures::aimed_corpse(views, registry, eye, dir, reach, block).map(|(id, _)| id)
}

/// Créature vivante sous le réticule, à portée et devant le bloc visé
fn aimed_creature(camera: &OrbitCamera, interaction: &BlockInteraction, views: &[creatures::CreatureView], registry: &herocraft_common::CreatureRegistry) -> Option<herocraft_common::CreatureId> {
    let (eye, dir, arm) = camera.view_ray()?;
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let reach = interaction.target().map_or(arm + block_interaction::REACH_BLOCKS as f32 * block, |hit| hit.distance as f32);
    creatures::aimed_creature(views, registry, eye, dir, reach, block).map(|(id, _)| id)
}

/// Donne à la bête visée ce que le joueur tient en main, si c'est sa nourriture et qu'elle en veut :
/// la simulation solo, ou le serveur en ligne, l'apprivoise, la rend amoureuse ou fait grandir le
/// petit. Renvoie vrai si elle a mangé (l'objet est retiré de l'inventaire).
fn feed_aimed_creature(
    camera: &OrbitCamera,
    interaction: &BlockInteraction,
    views: &[creatures::CreatureView],
    registry: &herocraft_common::CreatureRegistry,
    net: Option<&mut network::NetClient>,
    solo: &mut herocraft_world::CreatureWorld,
    ui_state: &mut UiState,
    audio: &mut audio::Audio,
) -> bool {
    let Some(item) = ui_state.inventory.selected_item() else { return false };
    let Some(id) = aimed_creature(camera, interaction, views, registry) else { return false };
    let Some(view) = views.iter().find(|v| v.id == id) else { return false };
    let Some(species) = view.species else { return false };
    // Un adulte apprivoisé déjà amoureux, ou qui vient d'avoir un petit, n'en veut pas
    let sated = view.mood & herocraft_common::MOOD_SATED != 0 && view.scale >= 1.0;
    if sated || !solo.food_of(species).contains(&item) {
        return false;
    }
    match net {
        Some(client) => client.send_feed_creature(id, item),
        None => {
            let feeding = solo.feed(id, item);
            if feeding == herocraft_world::Feeding::Refused {
                return false;
            }
            let name = registry.get(species).map_or("La bête", |s| s.name.as_str());
            if let Some(message) = herocraft_world::creatures::feeding_message(feeding, name) {
                ui_state.chat.info(message);
            }
        }
    }
    ui_state.inventory.consume_selected();
    audio.block_placed(audio::synth::Material::Grass, 0.0, &ui_state.audio_settings);
    true
}

/// Blessure du joueur de force `hurt` (0 : sans mal) : vue secouée, cri et, s'il en est mort,
/// écran de mort (`what` : ce qui l'a blessé, pour le journal)
fn show_wound(hurt: f32, what: &str, camera: &mut OrbitCamera, ui_state: &mut UiState, audio: &mut audio::Audio) {
    if hurt > 0.0 {
        camera.hurt(hurt);
        audio.player_hurt(hurt, &ui_state.audio_settings);
    }
    if ui_state.player.is_dead() && !camera.is_dead {
        info!("Mort : {what}.");
        camera.is_dead = true;
        ui_state.set_inventory_open(false);
    }
}

/// Frappe la créature `id` (la simulation solo, ou le serveur en ligne, en tire dégâts, recul et
/// fuite), sauf si le coup précédent est trop récent pour l'arme tenue.
/// Renvoie vrai si le coup a porté ici, en solo.
fn hit_creature(
    id: herocraft_common::CreatureId,
    skill_play: &mut skill_play::SkillPlay,
    camera: &OrbitCamera,
    net: Option<&mut NetClient>,
    solo: &mut herocraft_world::CreatureWorld,
    stats: &mut herocraft_common::CharacterStats,
    audio: &mut audio::Audio,
    settings: &audio::AudioSettings,
) -> bool {
    // Étourdi, il ne frappe pas
    if stats.is_stunned() {
        return false;
    }
    // Au rythme de l'arme tenue, de ses dégâts (le serveur en juge en ligne)
    let Ok(damage) = skill_play.strike() else { return false };
    // Invisible, son coup le dévoile et porte en critique
    let critical = stats.is_invisible();
    stats.invisible_time = 0.0;
    let alone = net.is_none();
    match net {
        Some(client) => client.send_hit_creature(id),
        None => {
            let factor = if critical { herocraft_world::CRITICAL_FACTOR } else { 1.0 };
            let boost = skill_play.solo_hit_factor(id, solo);
            solo.wound(id, camera.player_pos, skill_play::SOLO_PLAYER, damage * factor * boost);
        }
    }
    // Coup sourd
    audio.block_placed(audio::synth::Material::Dirt, 0.0, settings);
    alone
}

/// Le bloc cassé tombe au sol (s'il se ramasse : ni eau, ni lave), avec le butin de sa fiche
/// (fibre des herbes hautes, silex du gravier, récolte d'une plante mûre…)
fn drop_broken_block(dropped: &mut dropped_items::DroppedItems, camera: &OrbitCamera, pos: herocraft_world::BlockPos, block: herocraft_common::BlockId, previous: herocraft_common::BlockId) {
    if !block.is_air() {
        return;
    }
    let height = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let center = BlockInteraction::block_center(camera, pos);
    if let Some(item) = inventory::drop_for(previous) {
        dropped.spawn(item, center, height);
    }
    let loot = herocraft_common::loot::block_loot(previous).roll(&mut herocraft_common::LootRng::from_clock());
    for (item, count) in loot {
        for _ in 0..count {
            dropped.spawn(item, center, height);
        }
    }
}

/// Œuf pondu par une bête : posé à ses pieds, il y reste jusqu'à ce qu'on marche dessus
fn lay_egg(dropped: &mut dropped_items::DroppedItems, camera: &OrbitCamera, feet: glam::Vec3, item: herocraft_common::ItemId) {
    let height = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    dropped.lay(item, feet + feet.normalize_or_zero() * 0.2 * height);
}

/// Dépèce la bête morte `id` : en solo, son butin tombe aussitôt ; en ligne, le serveur le tire et
/// l'envoie (`NetEvent::Loot`)
fn skin_creature(id: herocraft_common::CreatureId, net: Option<&mut NetClient>, solo: &mut herocraft_world::CreatureWorld, dropped: &mut dropped_items::DroppedItems, camera: &OrbitCamera) {
    match net {
        Some(client) => client.send_skin_creature(id),
        None => {
            if let Some((species, feet)) = solo.skin(id) {
                let loot = herocraft_common::loot::creature_loot(solo.registry(), species).roll(&mut herocraft_common::LootRng::from_clock());
                drop_loot(dropped, camera, feet, &loot);
            }
        }
    }
}

/// Le butin d'une bête dépecée tombe à ses pieds (`feet`), un peu au-dessus du sol
fn drop_loot(dropped: &mut dropped_items::DroppedItems, camera: &OrbitCamera, feet: glam::Vec3, loot: &[(herocraft_common::ItemId, u16)]) {
    let height = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let center = feet + feet.normalize_or_zero() * 0.6 * height;
    for &(item, count) in loot {
        for _ in 0..count {
            dropped.spawn(item, center, height);
        }
    }
}

/// Enregistre les chunks modifiés par le joueur depuis le dernier enregistrement
fn save_terrain_edits(lod: &PlanetLodManager) {
    match lod.terrain_edits().flush() {
        Ok(0) => {}
        Ok(n) => info!("{} chunks modifiés enregistrés.", n),
        Err(e) => log::warn!("Échec de l'enregistrement des blocs modifiés : {}", e),
    }
}

/// Créatures d'un monde solo, reprises de sa sauvegarde
fn solo_creature_world(base_radius: f64, seed: u64, registry: &Arc<herocraft_common::CreatureRegistry>) -> herocraft_world::CreatureWorld {
    herocraft_world::CreatureWorld::with_save_dir(base_radius, seed, registry.clone(), solo_save_dir(seed))
}

/// Plantes semées d'un monde solo, reprises de sa sauvegarde
fn solo_fields_of(seed: u64) -> herocraft_world::Fields {
    herocraft_world::Fields::with_save_dir(solo_save_dir(seed))
}

/// Enregistre la population de créatures du monde solo (bêtes tuées comprises, qui ne reviennent
/// pas) et les plantes qui y poussent
fn save_solo_creatures(creatures: &herocraft_world::CreatureWorld, fields: &herocraft_world::Fields) {
    if let Err(e) = creatures.save() {
        log::warn!("Échec de l'enregistrement des créatures : {}", e);
    }
    if let Err(e) = fields.save() {
        log::warn!("Échec de l'enregistrement des cultures : {}", e);
    }
}

/// Écrit le rapport de profilage (no-op sans la feature cargo `profiling`)
fn write_profile_report() {
    if let Some(path) = herocraft_common::profiler::write_report() {
        info!("Rapport de profilage écrit : {}", path.display());
    }
}

/// Nom affiché d'une compétence (son identifiant si elle est inconnue)
fn skill_name(id: &str) -> String {
    herocraft_common::skills().get(id).map_or_else(|| id.to_string(), |s| s.name.clone())
}

/// Identifiants des blocs et objets que `/give` peut donner (ceux du catalogue de l'inventaire)
fn commands_items() -> Vec<String> {
    let blocks = herocraft_common::blocks().iter().filter(|s| inventory::can_hold(herocraft_common::BlockId(s.numeric_id).into())).map(|s| s.id.clone());
    let items = herocraft_common::items().iter().map(|s| s.id.clone());
    blocks.chain(items).collect()
}

/// Bloc ou objet de `/give`, par identifiant de fiche, sinon par nom affiché (sans tenir compte de
/// la casse)
fn find_item(name: &str) -> Option<herocraft_common::ItemId> {
    use herocraft_common::{blocks, items, BlockId, ItemId};
    let block = |s: &herocraft_common::BlockSheet| ItemId::from(BlockId(s.numeric_id));
    let holdable_blocks = || blocks().iter().filter(|s| inventory::can_hold(block(s)));
    holdable_blocks()
        .find(|s| s.id.eq_ignore_ascii_case(name))
        .map(block)
        .or_else(|| items().iter().find(|s| s.id.eq_ignore_ascii_case(name)).map(|s| s.item()))
        .or_else(|| holdable_blocks().find(|s| s.name.eq_ignore_ascii_case(name)).map(block))
        .or_else(|| items().iter().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.item()))
}

/// Ce que les commandes du chat peuvent changer
struct Game<'a> {
    camera: &'a mut OrbitCamera,
    generator: &'a herocraft_world::PlanetWorldGenerator,
    /// Le ciel (heure)
    sky: &'a mut State,
    ui: &'a mut UiState,
    /// Partie en ligne : `/summon`, `/heal` et `/xp` passent par le serveur
    net: Option<&'a mut NetClient>,
    /// Créatures de la partie solo
    creatures: &'a mut herocraft_world::CreatureWorld,
    registry: &'a herocraft_common::CreatureRegistry,
    seed: u64,
    pending_actions: &'a mut Vec<UiAction>,
    next_planet_seed: &'a mut Option<u64>,
    /// Météo du joueur (`/weather`, `/season`)
    weather: &'a mut weather::ClientWeather,
    /// Cinématiques (`/cinematic`, `/event`)
    cinematics: &'a mut cinematic::Cinematics,
    /// Objets au sol (`/drop`)
    dropped: &'a mut dropped_items::DroppedItems,
}

/// Température de l'échelle des biomes en degrés Celsius, à peu près : 0,4 (neige) vers 0 °C,
/// 1 (équateur) vers 30 °C
fn celsius(temperature: f32) -> i32 {
    ((temperature - herocraft_world::weather::SNOW_TEMPERATURE) * 50.0).round() as i32
}

fn season_english(season: herocraft_world::weather::Season) -> &'static str {
    match season {
        herocraft_world::weather::Season::Spring => "Spring",
        herocraft_world::weather::Season::Summer => "Summer",
        herocraft_world::weather::Season::Autumn => "Autumn",
        herocraft_world::weather::Season::Winter => "Winter",
    }
}

/// Exécute une commande du chat : renvoie les lignes de réponse, ou l'erreur à afficher
fn run_command(command: commands::Command, game: &mut Game) -> Result<Vec<String>, String> {
    use commands::{format_hour, Command};
    let block = herocraft_world::BlockGeometry::new(game.camera.base_radius as f64).block_height as f32;
    let online = game.ui.multiplayer.status != NetStatus::Solo;
    let moves = matches!(command, Command::TeleportLatLon { .. } | Command::TeleportPlayer(_) | Command::Spawn | Command::Fly);
    if moves && game.ui.player.is_dead() {
        return Err("You are dead".into());
    }
    let one = |line: String| Ok(vec![line]);
    match command {
        Command::Help(name) => commands::help(name.as_deref()),
        Command::TeleportLatLon { lat, lon, height } => {
            let feet = game.camera.ground_point(lat.to_radians(), lon.to_radians(), height, game.generator);
            game.camera.teleport(feet, game.generator);
            let above = if height > 0.0 { format!(", {height:.0} blocks above the ground") } else { String::new() };
            one(format!("Teleported to {lat:.2}°, {lon:.2}°{above}"))
        }
        Command::TeleportPlayer(name) => {
            let client = game.net.as_deref().ok_or("No other players in single player")?;
            let (name, feet) = client.player_position(&name).ok_or_else(|| format!("No player named {name}"))?;
            let feet = feet.ok_or_else(|| format!("{name} has not moved yet"))?;
            // Un bloc à côté de lui, pas dans lui
            let (east, _) = crate::camera::tangent_frame(feet.normalize_or_zero());
            game.camera.teleport(feet + east * block, game.generator);
            one(format!("Teleported to {name}"))
        }
        Command::Spawn => {
            game.camera.spawn_on_ground(game.generator);
            one("Back at the spawn point".into())
        }
        Command::Position => {
            let pos = game.camera.player_pos;
            let up = pos.normalize_or_zero();
            let lat = up.y.clamp(-1.0, 1.0).asin().to_degrees();
            let lon = up.z.atan2(up.x).to_degrees();
            let altitude = (pos.length() - game.camera.base_radius) / block - herocraft_world::generator::SEA_LEVEL_BLOCKS as f32;
            let geometry = herocraft_world::BlockGeometry::new(game.camera.base_radius as f64);
            let mut lines = vec![format!("Latitude {lat:.2}°, longitude {lon:.2}°, {altitude:.1} blocks above sea level")];
            if let Some(b) = geometry.block_at_point((pos + up * 0.5 * block).as_dvec3()) {
                lines.push(format!("Block: face {:?}, x {}, y {}, z {}", b.face, b.x, b.y, b.z));
            }
            Ok(lines)
        }
        Command::TimeQuery => {
            let up = game.camera.player_pos.normalize_or_zero();
            let hour = renderer::local_hour(game.sky.simulated_time, up).ok_or("The sun never rises nor sets here")?;
            let speed = game.ui.lighting_settings.moon_speed;
            let note = if online { " (your sky only)" } else { "" };
            one(format!("Local time {}, sky speed {speed}{note}", format_hour(hour)))
        }
        Command::TimeSet(hour) => {
            let up = game.camera.player_pos.normalize_or_zero();
            let time = renderer::time_for_local_hour(game.sky.simulated_time, up, hour).ok_or("The sun never rises nor sets here")?;
            // En ligne, le ciel est celui du serveur : l'heure change pour tous
            match game.net.as_deref_mut() {
                Some(client) => client.send_sky(Some(time as f64), None),
                None => game.sky.simulated_time = time,
            }
            let note = if online { " for everyone" } else { "" };
            one(format!("Time set to {}{note}", format_hour(hour)))
        }
        Command::TimeSpeed(_) if online => Err("Online, the sky follows the server's clock".into()),
        Command::TimeSpeed(speed) => {
            game.ui.lighting_settings.moon_speed = speed;
            one(if speed == 0.0 { "Sky frozen".into() } else { format!("Sky speed set to {speed}") })
        }
        Command::Seed => one(format!("Seed: {}", game.seed)),
        Command::Planet(_) if online => Err("The planet is the server's: leave the game to change it".into()),
        Command::Planet(seed) => {
            *game.next_planet_seed = seed;
            game.pending_actions.push(UiAction::GenerateNewPlanet);
            one(match seed {
                Some(seed) => format!("Generating planet {seed}..."),
                None => "Generating a new planet...".into(),
            })
        }
        Command::Give { item, count } => {
            let id = find_item(&item).ok_or_else(|| format!("Unknown item: {item} (Tab lists the names)"))?;
            let given = game.ui.inventory.add_many(id, count);
            match given {
                0 => Err("Your inventory is full".into()),
                n if n < count => one(format!("Gave {n} × {} (inventory full)", id.name())),
                n => one(format!("Gave {n} × {}", id.name())),
            }
        }
        Command::Drop { item, count } => {
            let id = find_item(&item).ok_or_else(|| format!("Unknown item: {item} (Tab lists the names)"))?;
            // Deux blocs devant le joueur, un peu au-dessus du sol : l'objet y tombe
            let (_, forward, _, _) = game.camera.look_vectors();
            let ahead = (game.camera.player_pos + forward * 2.0 * block).normalize_or_zero();
            let center = ahead * (game.camera.ground_radius_at(ahead, game.generator) + 0.6 * block);
            for _ in 0..count {
                game.dropped.place(id, center);
            }
            one(format!("Dropped {count} × {}", id.name()))
        }
        Command::Fly => {
            if game.camera.target_distance > game.camera.base_radius {
                return Err("Go back to the ground view first".into());
            }
            game.camera.toggle_flight();
            one(format!("Flight {}", if game.camera.is_flying { "on" } else { "off" }))
        }
        Command::Camera { distance, yaw, pitch } => {
            game.camera.set_view(distance, yaw, pitch);
            one(format!("Camera {distance} blocks behind"))
        }
        Command::Speed(factor) => {
            game.camera.speed_multiplier = factor;
            one(format!("Movement speed × {factor}"))
        }
        Command::Summon { species, count } => {
            let index = game
                .registry
                .iter()
                .position(|s| s.id.eq_ignore_ascii_case(&species) || s.name.eq_ignore_ascii_case(&species))
                .ok_or_else(|| format!("Unknown creature: {species} (Tab lists the names)"))?;
            let sheet = game.registry.get(index).ok_or("Unknown creature")?;
            // Trois blocs devant le joueur, posé au sol
            let (_, forward, _, _) = game.camera.look_vectors();
            let ahead = (game.camera.player_pos + forward * 3.0 * block).normalize_or_zero();
            let feet = ahead * game.camera.ground_radius_at(ahead, game.generator);
            match game.net.as_deref_mut() {
                Some(client) => {
                    client.send_summon(&sheet.id, count, feet);
                    Ok(Vec::new())
                }
                None => {
                    for _ in 0..count {
                        game.creatures.spawn(index, feet);
                    }
                    one(format!("Summoned {count} × {}", sheet.name))
                }
            }
        }
        Command::Cast(skill) => match herocraft_common::skills().get(&skill) {
            None => Err(format!("Unknown skill: {skill}")),
            Some(_) => {
                game.ui.skill_casts.push(skill);
                Ok(Vec::new())
            }
        },
        // En ligne, le serveur ouvre la vision ; en solo, le client change d'époque lui-même
        Command::Vision(enter) => match game.net.as_deref_mut() {
            Some(client) => {
                client.send_vision(enter);
                Ok(Vec::new())
            }
            None => {
                game.pending_actions.push(UiAction::Vision(enter));
                Ok(Vec::new())
            }
        },
        Command::Heal => match game.net.as_deref_mut() {
            Some(client) => {
                client.send_heal();
                Ok(Vec::new())
            }
            None if game.ui.player.is_dead() => Err("You are dead".into()),
            None => {
                game.ui.player.stats.revive();
                one("Healed".into())
            }
        },
        // /xp : en ligne, le serveur fait foi (`Server::grant_experience`)
        Command::Experience(amount) => match game.net.as_deref_mut() {
            Some(client) => {
                client.send_experience(amount);
                Ok(Vec::new())
            }
            None => {
                let stats = &mut game.ui.player.stats;
                let level = stats.level;
                stats.add_experience(amount);
                let gained = stats.level.saturating_sub(level);
                let line = if gained == 0 { format!("+{amount} XP") } else { format!("+{amount} XP, level {} (+{gained})", stats.level) };
                // La progression du solo est enregistrée aussitôt
                game.ui.player.save();
                one(line)
            }
        },
        Command::List => {
            let me = game.ui.multiplayer.pseudo.trim().to_string();
            let mut names = vec![format!("{me} (you)")];
            names.extend(game.net.as_deref().map(|c| c.player_names()).unwrap_or_default());
            one(format!("{} player(s) online: {}", names.len(), names.join(", ")))
        }
        Command::Weather(None) => {
            let sample = game.weather.sample.ok_or("The weather is not known yet")?;
            let forced = if game.weather.model.forced.is_some() { " (forced: /weather auto lets nature decide)" } else { "" };
            let what = match sample.kind() {
                herocraft_common::WeatherKind::Clear => "Clear sky",
                herocraft_common::WeatherKind::Cloudy => "Cloudy",
                herocraft_common::WeatherKind::Rain => "Rain",
                herocraft_common::WeatherKind::Storm => "Thunderstorm",
                herocraft_common::WeatherKind::Snow => "Snow",
            };
            one(format!("{what}, {}°C, wind {:.0}%{forced}", celsius(sample.temperature), sample.wind * 100.0))
        }
        Command::Weather(Some(forced)) => {
            match game.net.as_deref_mut() {
                Some(client) => client.send_sky(None, Some(forced)),
                None => game.weather.model.forced = forced,
            }
            let everyone = if online { " for everyone" } else { "" };
            one(match forced {
                Some(kind) => format!("Weather forced to {}{everyone}", kind.command_word()),
                None => format!("Natural weather is back{everyone}"),
            })
        }
        Command::Season(None) => {
            let up = game.camera.player_pos.normalize_or_zero();
            let lat = (up.y as f64).clamp(-1.0, 1.0).asin();
            let time = game.sky.simulated_time as f64;
            let (season, progress) = herocraft_world::weather::season_at(time, lat);
            let day = (progress as f64 * herocraft_world::weather::DAYS_PER_SEASON).floor() as u32 + 1;
            let year = (time / herocraft_world::weather::YEAR_SECONDS).floor() as u32 + 1;
            let mild = if herocraft_world::weather::season_strength(lat) < 0.3 { " (barely felt in the tropics)" } else { "" };
            one(format!("{}, day {day} of {}, year {year}{mild}", season_english(season), herocraft_world::weather::DAYS_PER_SEASON))
        }
        Command::Season(Some(season)) => {
            let time = herocraft_world::weather::time_for_season(game.sky.simulated_time as f64, season);
            match game.net.as_deref_mut() {
                Some(client) => client.send_sky(Some(time), None),
                None => game.sky.simulated_time = time as f32,
            }
            one(format!("{} begins in the north ({} in the south)", season_english(season), season_english(herocraft_world::weather::Season::ALL[(season as usize + 2) % 4])))
        }
        Command::ClearChat => {
            game.ui.chat.clear();
            Ok(Vec::new())
        }
        Command::Cinematic(None) => {
            let ids = game.cinematics.player.catalog.ids();
            if ids.is_empty() {
                return Ok(vec!["No cinematics".into()]);
            }
            Ok(vec![format!("Cinematics: {}", ids.join(", "))])
        }
        Command::Cinematic(Some(id)) => {
            game.cinematics.player.play(&id).map_err(|e| e.to_string())?;
            Ok(Vec::new())
        }
        Command::Event(text) => {
            let mut event = herocraft_common::cinematics::GameEvent::parse(&text).ok_or("Usage: /event <name> [key=value ...]")?;
            event.position = Some(game.camera.player_pos);
            game.sky.time_frozen = cinematic::frozen_after(&event, game.sky.time_frozen);
            let started = game.cinematics.notify(&event);
            Ok(vec![format!("Event {} sent: {started} cinematic(s) triggered", event.name)])
        }
    }
}

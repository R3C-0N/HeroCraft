use std::sync::Arc;
use std::time::Instant;

use log::{info, warn};
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceEvent, ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ControlFlow,
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{CursorGrabMode, Window, WindowBuilder},
};

mod atmosphere;
mod audio;
mod benchmark;
mod block_cracks;
mod block_interaction;
mod block_light;
mod block_outline;
mod block_textures;
mod camera;
mod chat;
mod paper_cards;
mod cinematic;
mod cloud_mesh;
mod collision;
mod commands;
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
mod item_icons;
mod keyboard_hook;
mod lod;
mod monitor;
mod network;
mod planet_mesh;
mod player_stats;
mod post;
mod precipitation;
mod renderer;
mod shaders;
mod shadow;
mod skills_screen;
mod skinned_model;
mod skill_models;
mod skill_bar;
mod skill_play;
mod terrain_cull;
mod terrain_pool;
mod threads;
mod ui;
mod ui_skin;
#[cfg(test)]
mod ui_snapshot;
mod vision_assault;
mod vision_charge;
mod vision_tutorial;
mod void_stones;
mod voxel_mesh;
mod weather;
mod world_map;

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

fn main() {
    // Variables du fichier `.env` (sans écraser celles déjà définies dans l'environnement)
    let _ = dotenvy::dotenv();
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
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
    let event_loop = monitor::build_event_loop(wanted_monitor.is_some());
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)
        .expect("Échec de création de la fenêtre Winit");
    if let Some(m) = wanted_monitor.as_deref().and_then(|name| monitor::find(&event_loop, name)) {
        window.set_outer_position(monitor::centered_position(&m, window.outer_size()));
        info!("Fenêtre ouverte sur l'écran {:?}.", m.name());
    }

    let window = Arc::new(window);

    // Initialisation du générateur procédural planétaire : graine imposée au lancement, sinon une
    // nouvelle planète à chaque partie
    let launch = network::launch_options();
    // Banc d'essai : toujours la même planète, sauf graine imposée
    let mut current_seed = launch.seed.or(benchmark::requested().then_some(benchmark::DEFAULT_SEED)).unwrap_or_else(herocraft_world::random_seed);
    info!("Planète de graine #{current_seed} (relancer avec --seed {current_seed} pour la retrouver).");
    let mut planet_desc = herocraft_world::PlanetDescriptor::default();
    planet_desc.seed = current_seed;

    info!("Initialisation de la planète et du gestionnaire QuadTree...");
    let mut generator = herocraft_world::PlanetWorldGenerator::new(planet_desc);

    // Initialisation du stockage et du cache de chunks voxels, et des blocs détruits ou posés par
    // le joueur, enregistrés dans la même sauvegarde
    let (storage, terrain_edits) = solo_terrain(current_seed);
    let mut chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);

    // Rayon de base de la planète déterminé par le descripteur (REFERENCE_BASE_RADIUS * scale)
    let base_radius = generator.base_radius() as f32;

    // Initialisation de la caméra multi-échelle unifiée (Sol FPS <-> TPS <-> Satellite)
    let mut camera = OrbitCamera::new(base_radius * 6.6, base_radius, &generator);
    let size = window.inner_size();
    camera.aspect = size.width as f32 / size.height.max(1) as f32;

    // Initialisation du gestionnaire de LOD QuadTree avec transitions douces hybrides
    let mut lod_manager = PlanetLodManager::new(base_radius, current_seed);
    camera.set_terrain(terrain_edits.clone());
    lod_manager.set_terrain_edits(terrain_edits);
    lod_manager.update(camera.lod_eye(), &generator, &mut chunk_cache, 0.0, None);
    let visible_meshes = lod_manager.visible_meshes();
    info!(
        "QuadTree initialisé : {} sommets, {} polygones ({} feuilles actives).",
        visible_meshes.iter().map(|(_, m)| m.vertices.len()).sum::<usize>(),
        visible_meshes.iter().map(|(_, m)| m.indices.len()).sum::<usize>() / 3,
        lod_manager.current_leaves.len()
    );

    // Initialisation de l'interface graphique (HUD avec boutons interactifs et menu paramètres)
    let mut ui_state = UiState::new(current_seed);
    ui_state.set_scale_factor(window.scale_factor() as f32);
    ui_state.set_screen_size(size.width as f32, size.height as f32);
    ui_state.controls = input::Controls::load_or_default(input::CONTROLS_FILE);
    ui_state.skill_bar = skill_bar::SkillBar::load_or_default(skill_bar::SKILL_BAR_FILE);
    ui_state.inventory = inventory::Inventory::load_or_default(inventory_file(current_seed, false));
    ui_state.player = player_stats::PlayerStats::load_or_default(Some(progress_file(current_seed)));
    ui_state.set_legend(load_legend(&legend_file(current_seed, false)));
    camera.hero_height = hero_height(&ui_state);
    ui_state.audio_settings = audio::AudioSettings::load_or_default(audio::AUDIO_FILE);
    // Son : bruitages du joueur et des blocs, ambiance (muet sans carte son)
    let mut audio = audio::Audio::new();
    // Le ciel au-dessus du joueur (grotte ou plein air) n'est relu que de temps en temps
    let mut sky_check_in = 0.0_f32;
    let mut sky_covered = false;

    // Partie en ligne : connexion en cours, connexion établie, partie ouverte au réseau local par
    // ce jeu, recherche des parties du réseau local (page Multijoueur ouverte)
    ui_state.multiplayer.pseudo = launch.pseudo.clone();
    let mut pending_join = launch.server.map(|addr| {
        info!("Connexion au serveur {} en tant que « {} »...", addr, launch.pseudo);
        ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
        PendingJoin::start(addr, launch.pseudo.clone())
    });
    let mut net: Option<NetClient> = None;
    // Vision de temple en cours : les blocs modifiés du présent, mis de côté, et en solo l'endroit
    // où le joueur revient
    let mut vision: Option<PresentWorld> = None;
    let mut hosted: Option<HostedServer> = None;
    let mut lan_browser: Option<LanBrowser> = None;
    let mut lan_retry_at = Instant::now();
    // Planète de la partie solo, retrouvée en quittant une partie en ligne
    let mut solo_seed = current_seed;
    // Créatures de la partie solo, simulées ici (en ligne, c'est le serveur qui les fait vivre), et
    // la planète où elles vivent. Les espèces sont décrites dans assets/creatures/ ; la population
    // de chaque zone est tirée une fois pour toutes et gardée dans la sauvegarde du monde.
    let creature_registry = Arc::new(herocraft_common::CreatureRegistry::load_default());
    let mut solo_creatures = solo_creature_world(base_radius as f64, current_seed, &creature_registry);
    // Plantes semées de la partie solo, qui poussent ici (en ligne, sur le serveur)
    let mut solo_fields = solo_fields_of(current_seed);
    // Chat : Tab complète les blocs et objets (`/give`) et les créatures (`/summon`)
    ui_state.chat.items = commands_items();
    ui_state.chat.creatures = creature_registry.iter().map(|s| s.id.clone()).collect();
    ui_state.chat.info(format!("Press [{}] to chat, /help for the commands", ui_state.controls.action_key_label(Action::OpenChat)));
    // Graine demandée par `/planet <graine>` pour la prochaine planète
    let mut next_planet_seed: Option<u64> = None;
    let mut creatures_seed = current_seed;
    let mut creature_animator = creatures::CreatureAnimator::default();
    let mut villager_animator = villagers::VillagerAnimator::default();
    let mut hero_gait = hero_model::HeroGait::default();
    // Démarche des héros des autres joueurs
    let mut remote_gaits: std::collections::HashMap<u64, hero_model::HeroGait> = std::collections::HashMap::new();
    // Pieds des héros changés en bête (formes du Druide) à l'image d'avant, pour leur allure
    let mut form_feet: std::collections::HashMap<u64, glam::Vec3> = std::collections::HashMap::new();
    let mut hero_figure = hero_model::FigureCache::default();
    // Compétences : lancements, recharges, gestes du héros et frappes à montrer
    let mut skill_play = skill_play::SkillPlay::default();
    let mut skill_effects = effects::Effects::default();
    // Cinématiques (assets/cinematics/) : déclenchées par les événements du jeu, ou par /cinematic
    let mut cinematics = cinematic::Cinematics::load();
    ui_state.chat.cinematics = cinematics.player.catalog.ids();
    let mut launch_announced = false;
    let mut effect_quads: Vec<effects::EffectQuad> = Vec::new();
    let (creature_models, creature_textures) = creatures::CreatureModels::load(&creature_registry);
    // Créatures dessinées à la dernière image, que le réticule peut viser, et délai avant le
    // prochain coup qu'on peut leur porter
    let mut creature_views: Vec<creatures::CreatureView> = Vec::new();
    // Humanoïde visé quand le clic gauche a été enfoncé : seul lui se frappe tant que le clic est tenu
    let mut strike_lock: Option<u64> = None;
    // En solo, combien de fois on a parlé à chaque habitant (il enchaîne ses répliques)
    let mut solo_talks: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
    // Puce intégrée : ombres et rayons allégés, MSAA coupé (réglables dans les options)
    ui_state.adapt_to_gpu(state.integrated_gpu());
    state.set_creature_textures(&creature_textures);
    state.set_creature_models(&creature_models);
    state.sync_lod_meshes(&lod_manager);

    // Banc d'essai (`--benchmark`) : le jeu joue seul une suite de phases et mesure chacune
    // Chronométrage des étapes de chaque image (à-coups, banc d'essai)
    let mut probe = benchmark::FrameProbe::default();
    let mut last_hitch_log: Option<Instant> = None;
    let mut bench = benchmark::requested().then(|| benchmark::Benchmark::new(&camera, current_seed));
    // Météo et saisons : l'horloge du ciel du monde solo reprend là où elle en était
    let mut client_weather = weather::ClientWeather::new(current_seed);
    let mut column_tops = weather::ColumnTops::default();
    let clock = herocraft_world::SkyClock::load(solo_save_dir(current_seed));
    state.simulated_time = clock.time as f32;
    client_weather.model.forced = clock.forced;
    if bench.is_some() {
        // Banc d'essai : beau temps, pour des mesures comparables
        client_weather.model.forced = Some(herocraft_world::WeatherKind::Clear);
        state.set_vsync(false);
        // Population tirée de la graine, sans sauvegarde : la même à chaque banc d'essai
        solo_creatures = herocraft_world::CreatureWorld::new(base_radius as f64, current_seed, creature_registry.clone());
    }

    // Gestion des entrées : souris capturée en jeu, glisser en vue satellite
    let mut cursor_captured = false;
    keyboard_hook::install(&window);
    let mut window_focused = true;
    let mut modifiers = ModifiersState::empty();
    let mut zoom_modifier_held = false;
    // Molette du pavé tactile (en pixels) cumulée jusqu'à un cran de la barre d'objets
    let mut hotbar_scroll = 0.0_f32;
    let mut jump_tap = input::DoubleTap::default();
    // Actions du clavier rejouées comme les boutons du HUD, au moment du rendu
    let mut pending_actions: Vec<UiAction> = Vec::new();
    let mut is_dragging = false;
    // Destruction (clic gauche) et pose (clic droit) du bloc visé par le réticule
    let mut block_interaction = BlockInteraction::new();
    // Blocs cassés tombés au sol, ramassés en passant à proximité
    let mut dropped_items = dropped_items::DroppedItems::new();
    // Pierres du Néant obtenues, touchées ou absorbées dans l'image (voir `void_stones::dispatch`)
    let mut stone_events = herocraft_common::StoneEvents::new();
    // La dernière Pierre entière obtenue, qu'absorbe la fin de sa vision (`vision_ended`)
    let mut vision_stone: Option<herocraft_common::ItemId> = None;
    // Le tutoriel de la vision (« L'éclat ») : le soldat, la roche à l'éclat et ses consignes
    let mut tutorial: Option<vision_tutorial::VisionTutorial> = None;
    // Fichier de commandes de test, lu à chaque image (`HEROCRAFT_COMMANDES`)
    let command_file = std::env::var_os("HEROCRAFT_COMMANDES").map(std::path::PathBuf::from);
    // Pierres du Néant posées près du joueur : leur fenêtre sur le néant et leurs poussières
    let mut void_sites = void_stones::VoidStoneSites::new();
    let game_start = Instant::now();
    let mut last_autosave = Instant::now();
    let mut last_mouse_pos = (0.0_f64, 0.0_f64);
    let mut last_frame_time = Instant::now();
    let mut fps_timer = Instant::now();
    let mut fps_frame_count = 0u32;

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, ref event } if window_id == window.id() => {
                // Hors image : le temps passé à traiter les événements de la fenêtre apparaît au rapport
                let _event_guard = (!matches!(event, WindowEvent::RedrawRequested)).then(|| {
                    herocraft_common::prof_scope!("boucle::evenement_fenetre" => guard);
                    guard
                });
                // Transmettre les événements de fenêtre à egui
                ui_state.handle_event(event);

                match event {
                    WindowEvent::CloseRequested => {
                        // En ligne, la population est celle du serveur : celle du solo est déjà enregistrée
                        if net.is_none() && hosted.is_none() {
                            save_solo_creatures(&solo_creatures, &solo_fields);
                            save_sky_clock(current_seed, &state, &client_weather);
                        }
                        if let Some(client) = net.as_mut() {
                            client.disconnect();
                        }
                        // Le serveur ouvert ici prévient ses joueurs et enregistre le monde
                        hosted = None;
                        let _ = chunk_cache.flush_dirty();
                        save_terrain_edits(&lod_manager);
                        ui_state.set_inventory_open(false);
                        ui_state.inventory.save();
                        ui_state.player.save();
                        write_profile_report();
                        crate::shaders::save_pipeline_cache();
                        target.exit();
                    }

                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                        ui_state.set_scale_factor(window.scale_factor() as f32);
                        ui_state.set_screen_size(physical_size.width as f32, physical_size.height as f32);
                        camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
                    }

                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        let physical_size = window.inner_size();
                        state.resize(physical_size);
                        ui_state.set_scale_factor(*scale_factor as f32);
                        ui_state.set_screen_size(physical_size.width as f32, physical_size.height as f32);
                        camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
                    }

                    WindowEvent::Focused(focused) => {
                        window_focused = *focused;
                        if !focused {
                            // Alt+Tab : les relâchements de touches n'arriveront pas
                            keyboard_hook::set_capture(None);
                            camera.release_inputs();
                            zoom_modifier_held = false;
                            is_dragging = false;
                        }
                    }

                    WindowEvent::ModifiersChanged(new_modifiers) => {
                        modifiers = new_modifiers.state();
                    }

                    // Clic souris : glisser pour tourner la planète en vue satellite (curseur visible).
                    // Souris capturée, les clics restent libres pour l'interaction avec les blocs.
                    WindowEvent::MouseInput { state: btn_state, button, .. } => {
                        // Changé en bête : le clic gauche et le clic droit lancent ses deux attaques
                        if cinematics.is_playing() {
                            // Cinématique : les clics ne touchent ni les blocs ni les bêtes
                        } else if let Some(attacks) = ui_state.beast_attacks.as_ref().filter(|_| cursor_captured) {
                            if *btn_state == ElementState::Pressed {
                                match button {
                                    MouseButton::Left => ui_state.skill_casts.push(attacks[0].clone()),
                                    MouseButton::Right => ui_state.skill_casts.push(attacks[1].clone()),
                                    _ => {}
                                }
                            }
                        } else if cursor_captured {
                            // Pose appliquée et remaillée sur-le-champ : le bloc change dans la frame qui
                            // suit le clic. Le clic gauche (casse, coup à une bête) avance à chaque frame.
                            let pressed = *btn_state == ElementState::Pressed;
                            // Clic droit sur un habitant ou un voyageur : ses échanges s'ouvrent
                            let villager = (pressed && *button == MouseButton::Right)
                                .then(|| aimed_creature(&camera, &block_interaction, &creature_views, &creature_registry))
                                .flatten()
                                .and_then(|id| match net.as_ref() {
                                    Some(client) => client.villager_card(id).cloned(),
                                    None => solo_creatures.villager_card(id).cloned(),
                                }.map(|card| (id, card)));
                            if let Some((id, card)) = villager.as_ref().filter(|(_, card)| card.chats_only()) {
                                // Rien à échanger : il répond dans le chat, que lisent les joueurs proches
                                match net.as_mut() {
                                    Some(client) => client.send_talk_to_villager(*id),
                                    None => {
                                        let turn = solo_talks.entry(*id).or_insert(0);
                                        ui_state.chat.villager(&card.speaker(), &card.line(*turn, herocraft_common::professions().lines_of(card)));
                                        *turn += 1;
                                    }
                                }
                            } else if let Some((id, card)) = villager {
                                camera.release_inputs();
                                block_interaction.reset();
                                ui_state.open_trade(id, card);
                            } else if let Some(action) = block_interaction.on_mouse_button(*button, pressed) {
                                block_interaction.update_target(&camera, &generator, &lod_manager.terrain_edits().clone());
                                // Clic droit sur un établi : il s'ouvre (accroupi, on pose contre lui)
                                let station = block_interaction.aimed_station().filter(|_| action == block_interaction::BlockAction::Place && !camera.is_crouching());
                                // Clic droit sur une bête morte, couteau ou dague en main : on la dépèce
                                let knife = action == block_interaction::BlockAction::Place && ui_state.inventory.slot(ui_state.inventory.selected()).is_some_and(|s| s.item.can_skin());
                                let corpse = if knife { aimed_corpse(&camera, &block_interaction, &creature_views, &creature_registry) } else { None };
                                // Clic droit sur une bête vivante avec sa nourriture en main : elle la mange
                                let fed = corpse.is_none()
                                    && action == block_interaction::BlockAction::Place
                                    && station.is_none()
                                    && feed_aimed_creature(&camera, &block_interaction, &creature_views, &creature_registry, net.as_mut(), &mut solo_creatures, &mut ui_state, &mut audio);
                                // Une Pierre du Néant en main : le joueur la touche
                                let stone = void_stones::held_stone(&ui_state.inventory).filter(|_| action == block_interaction::BlockAction::Place && station.is_none() && !fed && corpse.is_none());
                                // Sinon, un aliment en main : le joueur le mange
                                let food = ui_state.inventory.selected_food().filter(|_| action == block_interaction::BlockAction::Place && station.is_none() && !fed);
                                if let Some(item) = stone {
                                    void_stones::touch(item, &mut stone_events);
                                } else if let Some(id) = corpse {
                                    skin_creature(id, net.as_mut(), &mut solo_creatures, &mut dropped_items, &camera);
                                    audio.block_placed(audio::synth::Material::Dirt, 0.0, &ui_state.audio_settings);
                                } else if fed {
                                } else if let Some(at) = station {
                                    camera.release_inputs();
                                    block_interaction.reset();
                                    ui_state.open_station(at);
                                } else if let Some((item, satiety)) = food {
                                    if ui_state.player.eat(satiety) {
                                        ui_state.inventory.consume_selected();
                                        audio.eat(&ui_state.audio_settings);
                                        if let Some(client) = net.as_mut() {
                                            client.send_eat(item);
                                        }
                                    }
                                } else {
                                    let changes = block_interaction.apply(action, &camera, &generator, &mut lod_manager, &mut ui_state.inventory);
                                    for &(pos, block, previous) in &changes {
                                        block_sound(&mut audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                                        drop_broken_block(&mut dropped_items, &camera, pos, block, previous);
                                        match net.as_mut() {
                                            Some(client) => client.send_block_change(pos, block),
                                            None => solo_fields.observe(pos, block),
                                        }
                                    }
                                    if !changes.is_empty() {
                                        state.sync_lod_meshes(&lod_manager);
                                    }
                                }
                            }
                        }
                        if !cursor_captured && matches!(button, MouseButton::Left | MouseButton::Right) {
                            let is_over_ui = ui_state.is_settings_open
                                || ui_state.chat.open
                                || ui_state.ctx.is_pointer_over_egui()
                                || ui_state.ctx.egui_wants_pointer_input();
                            is_dragging = *btn_state == ElementState::Pressed && !is_over_ui;
                        }
                    }

                    // Déplacement curseur visible (survol des boutons UI et rotation de la planète)
                    WindowEvent::CursorMoved { position, .. } => {
                        if is_dragging && !cursor_captured && !ui_state.is_settings_open && !ui_state.screen_open() {
                            let dx = (position.x - last_mouse_pos.0) as f32;
                            let dy = (position.y - last_mouse_pos.1) as f32;
                            camera.orbit(dx, dy);
                        }
                        last_mouse_pos = (position.x, position.y);
                    }

                    // Molette : zoom continu FPS <-> TPS <-> Orbite avec la touche de zoom (Alt) maintenue,
                    // sinon case de la barre d'objets. En vue satellite, la molette seule zoome.
                    WindowEvent::MouseWheel { delta, .. } => {
                        let wants_zoom = if cursor_captured {
                            zoom_modifier_held
                        } else {
                            !(ui_state.is_settings_open
                                || ui_state.screen_open()
                                || ui_state.chat.open
                                || ui_state.ctx.is_pointer_over_egui()
                                || ui_state.ctx.egui_wants_pointer_input())
                        };

                        if cursor_captured && !zoom_modifier_held && ui_state.beast_attacks.is_none() {
                            // Molette vers le haut : case précédente (vers la gauche)
                            hotbar_scroll -= match delta {
                                MouseScrollDelta::LineDelta(_x, y) => *y,
                                MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                            };
                            let steps = hotbar_scroll.trunc();
                            if steps != 0.0 {
                                ui_state.inventory.scroll(steps as i32);
                                hotbar_scroll -= steps;
                            }
                        } else if wants_zoom {
                            let zoom_delta = match delta {
                                MouseScrollDelta::LineDelta(_x, y) => *y,
                                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.04,
                            };
                            camera.zoom(zoom_delta);
                        }
                    }

                    // Touches clavier : actions attribuées dans Échap > Commandes
                    WindowEvent::KeyboardInput {
                        event: KeyEvent {
                            state: key_state,
                            physical_key: PhysicalKey::Code(key_code),
                            logical_key,
                            repeat,
                            ..
                        },
                        ..
                    } => {
                        let is_pressed = *key_state == ElementState::Pressed;
                        let key_code = *key_code;

                        // Apprend le caractère de la touche selon la disposition du clavier (AZERTY…)
                        if is_pressed && !modifiers.intersects(ModifiersState::SHIFT | ModifiersState::CONTROL | ModifiersState::ALT) {
                            ui_state.controls.learn_label(key_code, logical_key);
                        }

                        if cinematics.is_playing() {
                            // Cinématique : le joueur ne fait que la regarder, ou la passer
                            let skip_key = matches!(key_code, KeyCode::Escape | KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter);
                            if skip_key && is_pressed && !*repeat {
                                cinematics.request_skip();
                            }
                        } else if key_code == KeyCode::Escape && is_pressed && ui_state.chat.open {
                            // Échap referme d'abord le chat
                            ui_state.chat.close();
                        } else if key_code == KeyCode::Escape && is_pressed {
                            // Touche Échap : annule une saisie de touche, ouvre le menu de pause,
                            // remonte d'un sous-menu ou referme le menu
                            let new_open = ui_state.handle_escape();

                            // Réinitialise les touches de déplacement pour éviter tout glissement fantôme
                            camera.release_inputs();
                            zoom_modifier_held = false;

                            info!("Touche [Échap] : Menu de pause {} ({:?})", if new_open { "OUVERT" } else { "FERMÉ" }, ui_state.menu_page);
                        } else if ui_state.rebinding.is_some() {
                            // Page Commandes : la touche pressée remplace celle de l'action choisie
                            if is_pressed {
                                ui_state.finish_rebind(key_code);
                            }
                        } else if key_code == KeyCode::F9 && is_pressed {
                            write_profile_report();
                        } else if ui_state.creator_open() {
                            // Créateur de légende : le clavier ne sert qu'à écrire le nom
                        } else if ui_state.chat.open {
                            // Chat ouvert : les touches vont à la saisie (egui), pas au joueur
                        } else if ui_state.trade.is_some() {
                            // Échanges avec un habitant : la touche de l'inventaire les referme aussi
                            if is_pressed && !*repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenInventory) {
                                ui_state.trade = None;
                            }
                        } else if ui_state.inventory_open {
                            // Écran d'inventaire : seule sa touche le referme, sauf pendant la saisie
                            // d'une recherche dans les recettes
                            let typing = ui_state.ctx.egui_wants_keyboard_input();
                            if is_pressed && !*repeat && !typing && ui_state.controls.action_for(key_code) == Some(Action::OpenInventory) {
                                ui_state.set_inventory_open(false);
                            }
                        } else if ui_state.skills_open {
                            // Roue des compétences : sa touche la referme
                            if is_pressed && !*repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenSkills) {
                                ui_state.set_skills_open(false);
                            }
                        } else if ui_state.map_open {
                            // Carte : sa touche la referme
                            if is_pressed && !*repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenMap) {
                                ui_state.set_map_open(false);
                            }
                        } else if !ui_state.is_settings_open {
                            // En jeu normal (menu fermé) : barre d'objets (1 à 9), déplacements et
                            // raccourcis selon les touches attribuées
                            if let Some(slot) = input::hotbar_slot_for(key_code) {
                                if is_pressed && ui_state.beast_attacks.is_none() {
                                    ui_state.inventory.select(slot);
                                }
                                return;
                            }
                            let Some(action) = ui_state.controls.action_for(key_code) else { return };
                            game_key(action, is_pressed, is_pressed && !*repeat, &mut camera, &mut ui_state, &mut pending_actions, &mut jump_tap, &mut zoom_modifier_held);
                        }
                    }

                    WindowEvent::RedrawRequested => {
                        herocraft_common::prof_scope!("frame" => frame_guard);
                        let now = Instant::now();
                        let dt = now.duration_since(last_frame_time).as_secs_f32();
                        last_frame_time = now;
                        // Sommets des créatures dessinées à cette image (rapport du banc d'essai)
                        let creature_vertices;

                        // Mesure et mise à jour du compteur FPS (rafraîchi toutes les 250 ms pour une lecture stable)
                        fps_frame_count += 1;
                        let fps_elapsed = now.duration_since(fps_timer).as_secs_f32();
                        if fps_elapsed >= 0.25 {
                            let current_fps = (fps_frame_count as f32 / fps_elapsed).round() as u32;
                            ui_state.set_fps(current_fps);
                            fps_frame_count = 0;
                            fps_timer = now;
                        }

                        // Banc d'essai : réglages et entrées de la phase en cours
                        if let Some(b) = bench.as_mut() {
                            if let Some(start) = b.before_frame(dt, &mut camera, &mut ui_state, &generator) {
                                lod_manager.set_distance_preset(ui_state.video_settings.lod_distance);
                                lod_manager.set_precision(ui_state.video_settings.lod_precision);
                                if start.reset_creatures {
                                    solo_creatures = herocraft_world::CreatureWorld::new(camera.base_radius as f64, current_seed, creature_registry.clone());
                                    creature_animator = creatures::CreatureAnimator::default();
                                }
                                let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                                for (i, feet) in benchmark::stress_positions(camera.player_pos, block, start.extra_creatures).into_iter().enumerate() {
                                    solo_creatures.spawn(i % creature_registry.len(), feet);
                                }
                            }
                            state.simulated_time = b.sun_time;
                        }

                        // Touches prises aux autres programmes par le crochet clavier depuis l'image précédente
                        for hooked in keyboard_hook::drain() {
                            let in_game = !ui_state.is_settings_open && !ui_state.screen_open() && !ui_state.chat.open && !cinematics.is_playing();
                            if let (true, Some(action)) = (in_game, ui_state.controls.action_for(hooked.key)) {
                                let first_press = hooked.pressed && !hooked.repeat;
                                game_key(action, hooked.pressed, first_press, &mut camera, &mut ui_state, &mut pending_actions, &mut jump_tap, &mut zoom_modifier_held);
                            }
                        }

                        // Souris capturée en jeu (au sol ou en 3e personne) ; le menu, la vue satellite
                        // et la perte du focus la libèrent
                        // L'inventaire ne s'ouvre qu'au sol
                        if ui_state.screen_open() && camera.target_distance > camera.base_radius {
                            ui_state.set_inventory_open(false);
                            ui_state.set_skills_open(false);
                            ui_state.trade = None;
                            ui_state.set_map_open(false);
                        }
                        // Première arrivée au sol d'un héros sans légende : le créateur s'ouvre
                        if bench.is_none() && ui_state.legend.is_none() && !ui_state.creator_open() && camera.target_distance <= camera.base_radius && !ui_state.player.is_dead() {
                            ui_state.open_creator();
                        }
                        // Mort : la souris est libérée pour le bouton de réapparition
                        // Banc d'essai : la souris reste libre (le jeu se pilote seul, la fenêtre peut
                        // être déplacée sur un autre écran pendant la mesure)
                        let want_capture = window_focused
                            && bench.is_none()
                            && !ui_state.player.is_dead()
                            && !ui_state.is_settings_open
                            && !ui_state.screen_open()
                            && !ui_state.chat.open
                            && camera.target_distance <= camera.base_radius;
                        if want_capture != cursor_captured {
                            cursor_captured = want_capture;
                            set_cursor_captured(&window, want_capture);
                            let controls = &ui_state.controls;
                            keyboard_hook::set_capture(
                                want_capture.then(|| keyboard_hook::interceptable(Action::ALL.map(|a| controls.key(a)))),
                            );
                            ui_state.set_pointer_captured(want_capture);
                            is_dragging = false;
                        }

                        // Cinématique : elle tient la caméra, ferme les écrans et renvoie ses
                        // événements au jeu (changer d'époque à la fin de la vision…)
                        cinematics.set_world(std::path::PathBuf::from(solo_save_dir(current_seed)));
                        // Le lancement du jeu, une fois le monde chargé : la Genèse s'y joue (pas pendant
                        // le banc d'essai)
                        if !launch_announced {
                            launch_announced = true;
                            // `--sans-genese` : directement dans le monde (scénarios de test)
                            if bench.is_none() && !std::env::args().any(|a| a == "--sans-genese") {
                                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::LAUNCH_EVENT));
                            }
                        }
                        let was_playing = cinematics.is_playing();
                        for event in cinematics.update(dt, &mut camera, &generator) {
                            info!("Événement de cinématique : {} {:?}", event.name, event.tags);
                            state.time_frozen = cinematic::frozen_after(&event, state.time_frozen);
                            if event.name == cinematic::VISION_ENDED {
                                if let Some(item) = vision_stone.take() {
                                    void_stones::absorb(item, &mut ui_state.inventory, &mut stone_events);
                                }
                                // Fin de la vision : retour au présent
                                match net.as_mut() {
                                    Some(client) => client.send_vision(false),
                                    None => pending_actions.push(UiAction::Vision(false)),
                                }
                            }
                        }
                        // Cartes de papier de la cinématique (aucune sinon), et l'atlas de ses découpes
                        state.prepare_cards(cinematics.card_sets());
                        let (card_uniform, card_instances) = cinematics.cards(&camera, state.card_layers());
                        state.update_cards(&card_uniform, &card_instances);
                        if cinematics.is_playing() && !was_playing {
                            ui_state.set_inventory_open(false);
                            ui_state.set_skills_open(false);
                            ui_state.set_map_open(false);
                            ui_state.trade = None;
                            ui_state.chat.close();
                            block_interaction.reset();
                        }

                        // Mise à jour de la caméra uniquement si le menu n'est pas ouvert
                        let section = Instant::now();
                        if !ui_state.is_settings_open {
                            herocraft_common::prof_scope!("frame::camera_update");
                            camera.obstacles = creatures::obstacles(&creature_views, &creature_registry);
                            // Compétences : allure renforcée, ruée ou bond qui commence
                            // Ralenti ou étourdi par une bête : moins vite, ou plus du tout, et sans sauter
                            let missing = 1.0 - ui_state.player.stats.hp / ui_state.player.stats.max_hp.max(1.0);
                            camera.buff_speed = skill_play.speed_factor(missing) * ui_state.player.stats.status_speed();
                            if ui_state.player.stats.is_stunned() {
                                camera.jump_requested = false;
                            }
                            if let Some((dash, leap, side, seconds)) = skill_play.take_motion() {
                                camera.launch(dash, leap, side, seconds);
                            }
                            for motion in skill_play.take_moves(Instant::now()) {
                                match motion {
                                    skill_play::Motion::Hover { height, duration } => camera.hover(height, duration),
                                    skill_play::Motion::Pull { to, speed, stop, hoist } => camera.pull(to, speed, stop, hoist),
                                    skill_play::Motion::Teleport { feet, look } => {
                                        camera.skill_teleport(feet, look, &generator);
                                    }
                                }
                            }
                            // Relevé sur place par une compétence (Intervention Suprême) : l'écran de mort se ferme
                            if camera.is_dead && !ui_state.player.is_dead() {
                                camera.is_dead = false;
                                audio.reset_player();
                                info!("Relevé sur place.");
                            }
                            camera.update(dt, &generator);
                            // Jauges du joueur, chute terminée : dommages (signalés au serveur en
                            // ligne), vue secouée et, si elle tue, écran de mort
                            ui_state.player.tick(dt, &mut camera);
                            if let Some(blocks) = camera.take_landing() {
                                if let Some(client) = net.as_mut() {
                                    client.send_fall(blocks);
                                }
                                // Banc d'essai : le joueur ne se blesse pas
                                if bench.is_none() {
                                    let hurt = ui_state.player.fall(blocks);
                                    show_wound(hurt, &format!("chute de {:.0} blocs", blocks), &mut camera, &mut ui_state, &mut audio);
                                }
                            }
                        }
                        probe.section("caméra et joueur", section);
                        {
                            herocraft_common::prof_scope!("son");
                            update_sound(&mut audio, &camera, &generator, &state, &ui_state, dt, &mut sky_check_in, &mut sky_covered, client_weather.visual());
                        }
                        // Météo au joueur : lumière voilée, nuages, pluie ou neige autour de lui
                        let ground = camera.distance <= camera.base_radius;
                        {
                            herocraft_common::prof_scope!("meteo");
                            let weather_dt = if ui_state.is_settings_open { 0.0 } else { dt };
                            client_weather.update(weather_dt, state.simulated_time as f64, camera.player_pos, camera.base_radius, ground);
                            for loudness in client_weather.take_thunder() {
                                audio.thunder(loudness, &ui_state.audio_settings);
                            }
                            state.update_cloud_weather(&client_weather.model);
                            let visual = *client_weather.visual();
                            let raining = ground && visual.precipitation > 0.01;
                            if let Some(collider) = camera.collider.as_ref().filter(|_| raining) {
                                if let Some(columns) = column_tops.update(dt, collider, &generator, camera.player_pos, camera.base_radius) {
                                    state.set_precipitation_columns(&columns);
                                }
                            }
                            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                            state.update_precipitation(raining.then_some(precipitation::PrecipFrame {
                                feet: camera.player_pos,
                                block,
                                intensity: visual.precipitation,
                                snow: visual.snow,
                                wind: visual.wind,
                                time: client_weather.clock,
                            }));
                        }
                        let visual = *client_weather.visual();
                        let mut lighting = visual.lighting(&ui_state.lighting_settings);
                        // En ligne, le ciel suit l'horloge du serveur
                        if net.is_some() {
                            lighting.moon_speed = 1.0;
                        }
                        state.update_camera(&camera, &lighting, &ui_state.cloud_settings, ui_state.video_settings.flattening, &visual);
                        state.update_post(&ui_state.post_settings, if ground { visual.overcast() } else { 0.0 });
                        state.set_msaa(ui_state.video_settings.msaa);
                        // Sous un ciel couvert, la lumière est diffuse : les ombres s'effacent, et leurs passes (avec
                        // les rayons crépusculaires) sont coupées sous un ciel tout gris
                        let mut shadow_settings = ui_state.shadow_settings;
                        let overcast = if ground { visual.overcast() } else { 0.0 };
                        shadow_settings.strength *= 1.0 - overcast;
                        shadow_settings.cloud_strength *= 1.0 - overcast;
                        shadow_settings.enabled &= overcast < 0.9;
                        state.update_shadows(&camera, &shadow_settings);
                        state.update_atmosphere(&camera, &ui_state.atmosphere_settings, &lighting);

                        // Bloc visé par le réticule (souris capturée seulement) et bouton tenu enfoncé
                        let mut terrain_edited = false;
                        // L'arme en main : sa cadence, son chargeur, ce qu'elle tire
                        skill_play.hold(ui_state.inventory.selected_item(), ui_state.player.stats.class);
                        ui_state.weapon_hud = skill_play.weapon_hud(ui_state.skill_charging.as_deref());
                        let mut aimed = None;
                        let mut looked_at = None;
                        if cursor_captured {
                            let edits = lod_manager.terrain_edits().clone();
                            block_interaction.update_target(&camera, &generator, &edits);
                            aimed = aimed_creature(&camera, &block_interaction, &creature_views, &creature_registry);
                            // L'habitant sous le réticule, sans bloc devant lui : on lit son nom
                            looked_at = block_interaction.sight(&camera, &generator, &edits, villagers::NAME_TAG_BLOCKS as f64).and_then(|sight| {
                                let (eye, dir, _) = camera.view_ray()?;
                                let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                                creatures::aimed_creature(&creature_views, &creature_registry, eye, dir, sight, block).map(|(id, _)| id)
                            });
                            // Une bête devant le bloc visé : c'est elle qu'on frappe, au rythme des
                            // coups, et le bloc derrière elle ne se fissure pas
                            // Un habitant, un garde ou un voyageur ne se frappe que si le clic a commencé
                            // sur lui : celui qui passe devant un bloc en train de casser n'est pas
                            // frappé par mégarde (les gardes s'en prendraient au joueur)
                            let fresh = block_interaction.take_fresh_press();
                            if fresh {
                                strike_lock = aimed;
                            }
                            // Un arc ou une arbalète, et de quoi le charger : le clic gauche tire (l'arc
                            // se bande tant qu'on tient le bouton et part au relâchement). Sans munition,
                            // il frappe au corps à corps, d'un coup de crosse.
                            let held = skill_play.held();
                            let ammo = held.ammo.and_then(|kind| ui_state.inventory.find_ammo(kind));
                            let shooting = held.is_ranged() && ammo.is_some();
                            if let Some(ammo) = ammo.filter(|_| shooting) {
                                block_interaction.pause_mining();
                                let bow = held.kind == Some(herocraft_common::WeaponKind::Bow);
                                if bow && fresh {
                                    skill_play.begin_charge(None);
                                    if let Some(client) = net.as_mut() {
                                        client.send_begin_charge(None);
                                    }
                                }
                                let release = if bow { skill_play.is_charging(None) && !block_interaction.is_breaking_held() } else { fresh };
                                if release {
                                    match skill_play.fire() {
                                        Ok(fired) => {
                                            ui_state.inventory.take_one(ammo);
                                            let (up, _, _, aim) = camera.look_vectors();
                                            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                                            audio.block_hit(audio::synth::Material::Wood, &ui_state.audio_settings);
                                            match net.as_mut() {
                                                Some(client) => client.send_shoot(aim, ammo),
                                                None => {
                                                    let damage = held.damage + herocraft_common::weapons::ammo_damage(ammo);
                                                    let hit = skill_play.solo_shoot(camera.player_pos, aim, damage, fired, &mut solo_creatures, &mut ui_state.player.stats);
                                                    skill_play.show_shot(None, camera.eye_position(), hit.to + up * 0.8 * block, hit.target, hit.special.as_deref(), block);
                                                    let name = ui_state.legend.as_ref().map(|l| l.appearance.nom.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Le joueur".to_string());
                                                    for murder in solo_creatures.take_murders() {
                                                        ui_state.chat.server(&murder.announce(&name));
                                                    }
                                                    for assault in solo_creatures.take_assaults() {
                                                        ui_state.chat.server(&assault.warning());
                                                    }
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            let weapon = held.item.map_or("Arme", |i| i.name());
                                            ui_state.skill_notice = Some((format!("{weapon} : {}", error.message()), Instant::now()));
                                        }
                                    }
                                }
                            }
                            if !block_interaction.is_breaking_held() {
                                strike_lock = None;
                            }
                            let humanoid = |id: u64| creature_views.iter().find(|v| v.id == id).is_some_and(|v| villagers::is_humanoid(v, &creature_registry));
                            let striking = aimed.filter(|_| !shooting && block_interaction.is_breaking_held()).filter(|&id| strike_lock == Some(id) || !humanoid(id));
                            if let Some(id) = striking {
                                block_interaction.pause_mining();
                                if hit_creature(id, &mut skill_play, &camera, net.as_mut(), &mut solo_creatures, &mut ui_state.player.stats, &mut audio, &ui_state.audio_settings) {
                                    skill_play.solo_weapon_hit(id, &mut solo_creatures, camera.player_pos, &mut ui_state.player.stats);
                                }
                                // Un habitant tué en solo : le chat l'annonce (en ligne, c'est le serveur)
                                let name = ui_state.legend.as_ref().map(|l| l.appearance.nom.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Le joueur".to_string());
                                for murder in solo_creatures.take_murders() {
                                    ui_state.chat.server(&murder.announce(&name));
                                }
                                for assault in solo_creatures.take_assaults() {
                                    ui_state.chat.server(&assault.warning());
                                }
                            }
                            // La roche à l'éclat ne cède qu'à la pioche
                            let bare_hands = tutorial.as_ref().is_some_and(|t| t.blocks_mining(block_interaction.target().map(|h| h.block), &ui_state.inventory));
                            if bare_hands {
                                block_interaction.pause_mining();
                            }
                            let action = if striking.is_some() || shooting || bare_hands { None } else { block_interaction.tick(dt) };
                            if let Some(block) = block_interaction.take_hit() {
                                audio.block_hit(audio::synth::Material::of(block), &ui_state.audio_settings);
                            }
                            if let Some(action) = action {
                                for (pos, block, previous) in block_interaction.apply(action, &camera, &generator, &mut lod_manager, &mut ui_state.inventory) {
                                    block_sound(&mut audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                                    drop_broken_block(&mut dropped_items, &camera, pos, block, previous);
                                    terrain_edited = true;
                                    match net.as_mut() {
                                        Some(client) => client.send_block_change(pos, block),
                                        None => solo_fields.observe(pos, block),
                                    }
                                }
                            }
                        } else {
                            block_interaction.reset();
                        }

                        // Partie en ligne : position envoyée, blocs des autres appliqués, autres joueurs affichés
                        if let Some(client) = net.as_mut() {
                            herocraft_common::prof_scope!("reseau");
                            client.send_move(&camera, ui_state.inventory.selected_item());
                            client.send_equipment(ui_state.inventory.worn().into_iter().map(|i| i.0).collect());
                            let mut lost = None;
                            for event in client.poll() {
                                match event {
                                    NetEvent::Vision { holder, feet } => {
                                        switch_epoch(holder, feet, None, None, &mut vision, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state);
                                        if holder.is_some() {
                                            if tutorial.is_none() {
                                                tutorial = begin_vision_tutorial(feet, &camera, &generator, &mut lod_manager, &mut ui_state, None);
                                                state.sync_lod_meshes(&lod_manager);
                                            }
                                            cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::VISION_ENTERED));
                                        } else {
                                            end_vision_tutorial(&mut tutorial, &mut ui_state, &mut solo_creatures);
                                        }
                                        block_interaction.invalidate();
                                        terrain_edited = true;
                                    }
                                    NetEvent::BlockChanged(pos, block) => {
                                        let previous = lod_manager.terrain_edits().block(&generator, pos);
                                        if lod_manager.set_block(&generator, pos, block) {
                                            // Bloc changé par un autre joueur : entendu s'il est proche
                                            let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
                                            let centre = geometry.block_corners(pos).iter().sum::<glam::DVec3>() / 8.0;
                                            let distance = (centre.as_vec3() - camera.player_pos).length() / geometry.block_height as f32;
                                            block_sound(&mut audio, Some(previous), block, distance, &ui_state.audio_settings);
                                            terrain_edited = true;
                                            block_interaction.invalidate();
                                        }
                                    }
                                    NetEvent::Stats(stats) => ui_state.player.sync(stats, &mut camera),
                                    NetEvent::Hurt { damage, cause } => {
                                        let hurt = ui_state.player.bitten(damage, None, &cause);
                                        show_wound(hurt, &cause, &mut camera, &mut ui_state, &mut audio);
                                    }
                                    NetEvent::Loot { position, items } => drop_loot(&mut dropped_items, &camera, position, &items),
                                    NetEvent::Laid { position, item } => lay_egg(&mut dropped_items, &camera, position, item),
                                    NetEvent::Chat { sender, message } if sender.is_empty() => ui_state.chat.server(&message),
                                    NetEvent::Chat { sender, message } => ui_state.chat.player(&sender, &message),
                                    NetEvent::VillagerSays { speaker, line } => ui_state.chat.villager(&speaker, &line),
                                    NetEvent::Clock { time, forced } => {
                                        state.simulated_time = time as f32;
                                        client_weather.model.forced = forced;
                                    }
                                    NetEvent::SkillCast { player_id, skill, rank, aim, aimed, fused, amplified } => skill_play.on_cast(player_id, client.player_id(), &skill, rank, aim, aimed, fused, amplified),
                                    NetEvent::SkillRefused { skill, reason } => {
                                        ui_state.skill_notice = Some((format!("{} : {reason}", skill_name(&skill)), Instant::now()));
                                        skill_play.on_refused(&skill, &reason);
                                    }
                                    NetEvent::SkillMove(m) => skill_play.on_move(m),
                                    NetEvent::SkillHit { player_id, skill, rank, center, creatures, players, parried, contact } => {
                                        let caster = (player_id != client.player_id()).then_some(player_id);
                                        skill_play.on_hit(skill_play::Shown { caster, skill, rank, center, creatures, players, parried, contact });
                                    }
                                    NetEvent::SkillCooldown { skill, seconds } => skill_play.extend_cooldown(&skill, seconds),
                                    NetEvent::Pushed { direction, speed } => camera.push(direction, speed),
                                    NetEvent::PlayerShot { player_id, from, to, target, special } => {
                                        let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                                        let up = from.normalize_or_zero();
                                        let mine = player_id == client.player_id();
                                        let start = if mine { camera.eye_position() } else { from + up * 1.5 * block };
                                        skill_play.show_shot((!mine).then_some(player_id), start, to + up * 0.8 * block, target, special.as_deref(), block);
                                    }
                                    NetEvent::CreatureShots(shots) => skill_play.on_shots(&shots, &creature_registry),
                                    NetEvent::Projectiles(events) => skill_play.on_projectiles(&events, &creature_registry),
                                    NetEvent::Disconnected(reason) => lost = Some(reason),
                                }
                            }
                            ui_state.chat.players = client.player_names();
                            if let Some(reason) = lost {
                                warn!("Déconnecté du serveur ({}) : retour à la partie solo.", reason);
                                ui_state.multiplayer.notice = Some(format!("Déconnecté : {}", reason));
                                pending_actions.push(UiAction::LeaveServer);
                            } else {
                                state.set_player_outlines(&client.remote_player_boxes(camera.base_radius));
                                ui_state.name_tags = client.name_tags(&camera, state.size.width as f32, state.size.height as f32);
                            }
                        }

                        // Créatures : celles que le serveur montre en ligne, sinon celles de la partie solo,
                        // en pause avec le menu. Dessinées d'après le modèle de leur espèce.
                        {
                            herocraft_common::prof_scope!("creatures");
                            let section = Instant::now();
                            let creature_mode = bench.as_ref().map_or(benchmark::CreatureMode::Full, |b| b.creature_mode());
                            creature_views = match net.as_ref() {
                                Some(client) => client.creature_views(&creature_registry),
                                None => {
                                    if creatures_seed != current_seed {
                                        solo_creatures = solo_creature_world(camera.base_radius as f64, current_seed, &creature_registry);
                                        solo_fields = solo_fields_of(current_seed);
                                        creatures_seed = current_seed;
                                    }
                                    if !ui_state.is_settings_open && creature_mode != benchmark::CreatureMode::Off {
                                        // L'heure du ciel règle le sommeil et les apparitions
                                        solo_creatures.set_sun_dir(state.sun_dir());
                                        // Sous l'orage les bêtes s'abritent, l'hiver les ours hibernent
                                        solo_creatures.set_climate(&client_weather.model, state.simulated_time as f64);
                                        let player = herocraft_world::PlayerPresence {
                                            feet: camera.player_pos,
                                            crouching: camera.is_crouching(),
                                            holding: ui_state.inventory.selected_item(),
                                            hidden: ui_state.player.stats.is_invisible() || ui_state.player.is_dead(),
                                        };
                                        // Les plantes semées poussent (le serveur s'en charge en ligne)
                                        for (pos, block) in solo_fields.tick_in_weather(dt, &generator, lod_manager.terrain_edits(), &client_weather.model, state.simulated_time as f64) {
                                            terrain_edited |= lod_manager.set_block(&generator, pos, block);
                                            block_interaction.invalidate();
                                        }
                                        // Morsures et charges des bêtes agressives
                                        // Le temps figé de la vision ralentit aussi les bêtes
                                        let hits = solo_creatures.tick_with_presences(dt * state.time_scale(), &generator, lod_manager.terrain_edits(), &[player]);
                                        // Banc d'essai : les bêtes ne blessent pas le joueur
                                        for hit in hits.into_iter().filter(|_| bench.is_none()) {
                                            // Une garde levée pare le coup qui arrive de face, et riposte
                                            let (_, facing, _, _) = camera.look_vectors();
                                            let (damage, parried) = skill_play.solo_parry(&mut solo_creatures, camera.player_pos, facing, &mut ui_state.player.stats, hit.creature_id, hit.damage, hit.kind);
                                            // Ralenti, étourdi, repoussé : le coup de certaines bêtes le laisse sonné
                                            if !ui_state.player.is_dead() && !parried {
                                                let ignores = skill_play.solo_immunities();
                                                solo_creatures.afflict(&hit, &mut ui_state.player.stats, &ignores);
                                                if let Some((direction, speed)) = solo_creatures.knockback(&hit, camera.player_pos).filter(|_| !ignores.contains(&herocraft_common::skills::Status::Knockback)) {
                                                    camera.push(direction, speed);
                                                }
                                            }
                                            let cause = creature_registry.get(hit.species).map_or("Une créature", |s| s.name.as_str());
                                            // L'armure portée, puis les renforcements de défense
                                            let damage = herocraft_common::through_armor(damage, hit.kind, ui_state.inventory.protection() as f32) * skill_play.solo_damage_factor(hit.kind);
                                            let hurt = ui_state.player.bitten(damage, Some(hit.kind), cause);
                                            // Tombé sous un effet à la mort : la nova part, il se relève
                                            if ui_state.player.is_dead() {
                                                skill_play.solo_last_stand(&mut solo_creatures, camera.player_pos, &mut ui_state.player.stats);
                                            }
                                            show_wound(hurt, cause, &mut camera, &mut ui_state, &mut audio);
                                        }
                                        // Tirs des tourelles, du bastion et de la liche
                                        let shots = solo_creatures.take_shots();
                                        if !shots.is_empty() {
                                            skill_play.on_shots(&shots, &creature_registry);
                                        }
                                        let projectiles = solo_creatures.take_projectile_events();
                                        if !projectiles.is_empty() {
                                            skill_play.on_projectiles(&projectiles, &creature_registry);
                                        }
                                        // Œufs pondus par les bêtes apprivoisées, laissés à leurs pieds
                                        for (feet, item) in solo_creatures.take_laid() {
                                            lay_egg(&mut dropped_items, &camera, feet, item);
                                        }
                                    }
                                    match creature_mode {
                                        benchmark::CreatureMode::Full | benchmark::CreatureMode::Stress => solo_creatures
                                            .creatures()
                                            .iter()
                                            .filter(|c| !c.is_hidden())
                                            .map(|c| creatures::CreatureView::new(Some(c.species), &c.state()))
                                            .collect(),
                                        benchmark::CreatureMode::SimulationOnly | benchmark::CreatureMode::Off => Vec::new(),
                                    }
                                }
                            };
                            // Les copies (leurres, ombres d'Étherium) se dessinent à l'allure qu'elles imitent
                            for view in creature_views.iter_mut().filter(|v| v.copy.is_some()) {
                                *view = skill_models::drawn_as(view, &creature_registry);
                            }
                            probe.section("créatures : simulation", section);
                            let section = Instant::now();
                            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                            let anim_dt = if ui_state.is_settings_open { 0.0 } else { dt };
                            // Les bêtes d'après leur modèle, les habitants et les voyageurs avec le corps des héros
                            let mut beasts: Vec<creatures::CreatureView> = creature_views.iter().filter(|v| !villagers::is_humanoid(v, &creature_registry)).copied().collect();
                            // Héros changés en bête (formes du Druide) : la bête à leur place
                            skill_play.me = net.as_ref().map_or(skill_play::SOLO_PLAYER, |c| c.player_id());
                            let third_person = camera.view_mode() == camera::CameraViewMode::ThirdPerson;
                            let local_form = skill_play.form_of(None).filter(|_| ui_state.legend.is_some());
                            // Changé en bête : les clics lancent ses attaques, l'inventaire se ferme
                            let attacks = skill_play.form_skill(None).and_then(|s| herocraft_common::skills().get(&s)).and_then(|d| <[String; 2]>::try_from(d.form_attacks.clone()).ok());
                            if attacks.is_some() && ui_state.beast_attacks.is_none() {
                                ui_state.set_inventory_open(false);
                                block_interaction.reset();
                            }
                            ui_state.beast_attacks = attacks;
                            let mut shapes: Vec<(u64, String, glam::Vec3, f32, bool)> = Vec::new();
                            if let Some(form) = local_form.clone().filter(|_| third_person) {
                                shapes.push((u64::MAX, form, camera.player_pos, camera.player_yaw, skill_play.form_attacking(None)));
                            }
                            if let Some(client) = net.as_ref() {
                                for (id, feet, yaw, _) in client.remote_heroes() {
                                    if let Some(form) = skill_play.form_of(Some(id)) {
                                        shapes.push((u64::MAX - 1 - id, form, feet, yaw, skill_play.form_attacking(Some(id))));
                                    }
                                }
                            }
                            form_feet.retain(|id, _| shapes.iter().any(|s| s.0 == *id));
                            for (id, form, feet, yaw, attacking) in &shapes {
                                let speed = form_feet.insert(*id, *feet).map_or(0.0, |last| last.distance(*feet) / block / dt.max(1e-3));
                                let action = if *attacking {
                                    herocraft_common::CreatureAction::Attaque
                                } else if speed > 4.0 { herocraft_common::CreatureAction::Course } else if speed > 0.3 { herocraft_common::CreatureAction::Marche } else { herocraft_common::CreatureAction::Repos };
                                beasts.push(creatures::CreatureView { id: *id, species: creature_registry.find(form), feet: *feet, yaw: *yaw, hurt: 0.0, death: 0.0, action, scale: 1.0, mood: 0, summoner: None, copy: None, tint: None });
                            }
                            creature_animator.update(&beasts, &creature_registry, &creature_models, anim_dt, block);
                            let mut frame = creatures::build_frame(&beasts, &creature_registry, &creature_models, &creature_animator, block, if state.gpu_skinning() { creatures::Skinning::Gpu } else { creatures::Skinning::Cpu }, Some(camera.eye_position()));
                            villager_animator.update(&creature_views, &creature_registry, anim_dt, block);
                            state.begin_villagers();
                            {
                                let card_of = |id: u64| match net.as_ref() {
                                    Some(client) => client.villager_card(id).cloned(),
                                    None => solo_creatures.villager_card(id).cloned(),
                                };
                                // Un leurre copie le héros de son lanceur, le nôtre ou celui d'un autre joueur
                                let me = net.as_ref().map_or(skill_play::SOLO_PLAYER, |c| c.player_id());
                                let my_look = ui_state.legend.as_ref().map(|l| l.appearance.clone());
                                let hero_of = |id: u64| if id == me { my_look.clone() } else { net.as_ref().and_then(|c| c.remote_appearance(id)).cloned() };
                                villagers::push_villagers(&mut frame, &creature_views, &creature_registry, card_of, &hero_of, &mut villager_animator, |a| state.villager_texture(a), block);
                                let mut tags = net.as_ref().map(|c| c.name_tags(&camera, state.size.width as f32, state.size.height as f32)).unwrap_or_default();
                                tags.extend(villagers::name_tags(&creature_views, card_of, looked_at, &camera, state.size.width as f32, state.size.height as f32));
                                ui_state.name_tags = tags;
                                // Les échanges se ferment quand l'habitant s'éloigne (ou disparaît)
                                if let Some(trade) = ui_state.trade.as_ref() {
                                    let near = villagers::feet_of(&creature_views, trade.creature_id).is_some_and(|f| f.distance(camera.player_pos) < 8.0 * block);
                                    if !near {
                                        ui_state.trade = None;
                                    }
                                }
                            }
                            // Compétences : en solo les règles tournent ici, sur nos créatures
                            let solo_skills = (net.is_none() && !ui_state.is_settings_open).then_some((&mut solo_creatures, camera.player_pos, &mut ui_state.player.stats));
                            skill_effects.start(skill_play.tick(anim_dt, solo_skills));
                            for id in skill_play.take_ended_projectiles() {
                                skill_effects.end_projectile(id);
                            }
                            if skill_play.take_solo_death() {
                                ui_state.player.bled_out();
                            }
                            if net.is_none() {
                                let name = ui_state.legend.as_ref().map(|l| l.appearance.nom.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Le joueur".to_string());
                                for murder in solo_creatures.take_murders() {
                                    ui_state.chat.server(&murder.announce(&name));
                                }
                                for assault in solo_creatures.take_assaults() {
                                    ui_state.chat.server(&assault.warning());
                                }
                            }
                            // Le héros, vu de dehors à la troisième personne, avec le geste de sa compétence
                            hero_gait.update(camera.player_pos, anim_dt, block);
                            let mut hero_anchors = hero_model::HeroAnchors::standing(camera.player_pos, camera.player_yaw, block);
                            if let Some(legend) = ui_state.legend.as_ref().filter(|_| camera.view_mode() == camera::CameraViewMode::ThirdPerson && local_form.is_none()) {
                                state.set_hero_sheet(&legend.appearance);
                                let figure = hero_figure.get(&legend.appearance);
                                let pose = skill_play.hero_pose(hero_gait.swing(), figure.units_per_block(&legend.appearance));
                                hero_model::push_hero_posed(&mut frame, &legend.appearance, figure, camera.player_pos, camera.player_yaw, &pose, block);
                                // Transformations partielles du Druide : patte, fourrure, pattes de cerf, plumes
                                for graft in skill_play.hero_grafts().iter().filter_map(|g| skill_models::graft(g)) {
                                    let slot = state.villager_texture(creature_render::SheetSource::Fixed(&graft.id, &graft.texture));
                                    skill_models::push_graft(&mut frame, &graft.model, &legend.appearance, figure, camera.player_pos, camera.player_yaw, &pose, slot, block);
                                }
                                hero_anchors = hero_model::HeroAnchors::posed(&legend.appearance, figure, camera.player_pos, camera.player_yaw, &pose, block);
                            }
                            // Les héros des autres joueurs, avec le geste de leur compétence et leurs greffes
                            if let Some(client) = net.as_ref() {
                                let heroes = client.remote_heroes();
                                remote_gaits.retain(|id, _| heroes.iter().any(|h| h.0 == *id));
                                for (id, feet, yaw, appearance) in heroes {
                                    if skill_play.form_of(Some(id)).is_some() {
                                        continue;
                                    }
                                    let gait = remote_gaits.entry(id).or_default();
                                    gait.update(feet, anim_dt, block);
                                    let figure = villager_animator.hero_figure(appearance);
                                    let pose = skill_play.pose_of(Some(id), gait.swing(), figure.units_per_block(appearance));
                                    let slot = state.villager_texture(creature_render::SheetSource::Painted(appearance));
                                    hero_model::push_figure(&mut frame, appearance, figure, feet, yaw, &pose, slot, block);
                                    for graft in skill_play.grafts_of(Some(id)).iter().filter_map(|g| skill_models::graft(g)) {
                                        let slot = state.villager_texture(creature_render::SheetSource::Fixed(&graft.id, &graft.texture));
                                        skill_models::push_graft(&mut frame, &graft.model, appearance, figure, feet, yaw, &pose, slot, block);
                                    }
                                }
                            }
                            // À la première personne, le temps d'un geste : les bras et l'arme devant l'œil
                            if let Some(legend) = ui_state.legend.as_ref().filter(|_| camera.view_mode() == camera::CameraViewMode::FirstPerson && skill_play.local_play().is_some()) {
                                state.set_hero_sheet(&legend.appearance);
                                let figure = hero_figure.get(&legend.appearance);
                                let pose = skill_play.hero_pose(0.0, figure.units_per_block(&legend.appearance));
                                let held = ui_state.inventory.slot(ui_state.inventory.selected()).map(|s| s.item);
                                // Un objet tenu (pas un bloc) fait l'arme ; sa couleur d'acier en attendant les vraies armes
                                let weapon = held.filter(|item| item.block().is_none()).map(|_| [0.72, 0.74, 0.78, 1.0]);
                                let (_, _, _, look) = camera.look_vectors();
                                let (right, left) = hero_model::push_first_person_arms(&mut frame, &legend.appearance, figure, camera.eye_position(), look, &pose, weapon, block);
                                hero_anchors.right_hand = right;
                                hero_anchors.left_hand = left;
                            }
                            // Les effets des compétences, accrochés au héros, aux joueurs et aux créatures
                            effect_quads.clear();
                            if !skill_effects.is_empty() {
                                let up = camera.player_pos.normalize_or_zero();
                                let (east, north) = herocraft_common::tangent_frame(up);
                                let hero_forward = north * camera.player_yaw.cos() + east * camera.player_yaw.sin();
                                let players = |id: u64| net.as_ref().and_then(|c| c.remote_player_feet(id)).map(|f| (f, 0.0));
                                let views = &creature_views;
                                let creature_feet = |id: u64| views.iter().find(|v| v.id == id).map(|v| v.feet);
                                let shots = &skill_play;
                                let projectile_at = |id: u64| shots.projectile_at(id);
                                let world = effects::AnchorWorld { hero: hero_anchors, hero_forward, players: &players, creatures: &creature_feet, projectiles: &projectile_at, eye: camera.eye_position(), block };
                                skill_effects.update(anim_dt, &world, &mut effect_quads);
                            }
                            {
                                let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
                                void_sites.update(anim_dt, lod_manager.terrain_edits(), &geometry, camera.player_pos);
                                void_sites.push_quads(game_start.elapsed().as_secs_f32(), camera.eye_position(), block, &mut effect_quads);
                                // Le tutoriel de la vision : ses étapes, sa consigne, le murmure et la lueur de l'éclat
                                ui_state.vision_hint = None;
                                match tutorial.as_mut() {
                                    Some(t) => {
                                        let outcome = t.update(anim_dt, camera.player_pos, block, cinematics.is_playing(), state.time_frozen, &ui_state.inventory, &generator, lod_manager.terrain_edits());
                                        if outcome.freeze {
                                            state.time_frozen = true;
                                        }
                                        let aiming_rock = block_interaction.target().is_some_and(|h| Some(h.block) == t.shard_rock().map(|(p, _)| p));
                                        if !cinematics.is_playing() {
                                            ui_state.vision_hint = t.hint(&ui_state.controls, aiming_rock);
                                        }
                                        audio.set_whisper(t.whisper(), &ui_state.audio_settings);
                                        t.push_quads(camera.eye_position(), block, &mut effect_quads);
                                        // La charge : au cri du capitaine (le temps reparti après l'éveil)
                                        let awakened = t.step() == vision_tutorial::Step::Awakened && !state.time_frozen;
                                        let soldiers = t.soldiers().to_vec();
                                        let mut charge_events = Vec::new();
                                        if let Some(charge) = t.charge.as_mut() {
                                            if awakened {
                                                charge.start(&soldiers, &mut charge_events);
                                            }
                                            charge.update(anim_dt, camera.player_pos, &mut charge_events);
                                            charge.tick_hint(anim_dt);
                                        }
                                        apply_charge_events(charge_events, &mut camera, &generator, &mut lod_manager, &mut state, &mut solo_creatures, &mut skill_effects, &mut audio, &ui_state.audio_settings, &mut cinematics);
                                        // Passé la herse : les rues, les trois places, le héraut, la Pierre
                                        let sealed = t.charge.as_ref().is_some_and(|c| c.phase() == vision_charge::Phase::Sealed);
                                        if let Some(assault) = t.assault.as_mut().filter(|_| sealed) {
                                            let mut assault_events = Vec::new();
                                            if assault.phase() == vision_assault::Phase::Waiting {
                                                let allies: Vec<u64> = solo_creatures.creatures().iter().filter(|c| c.is_alive() && soldiers.contains(&c.id)).map(|c| c.id).collect();
                                                assault.start(&allies);
                                            }
                                            let ground = |dir: glam::Vec3| dir * camera.ground_radius_at(dir, &generator);
                                            assault.update(anim_dt, camera.player_pos, &mut solo_creatures, &ground, &mut assault_events);
                                            if ui_state.vision_hint.is_none() && !cinematics.is_playing() {
                                                ui_state.vision_hint = assault.hint(&solo_creatures);
                                            }
                                            // Le cristal appelle : le murmure revient, de plus en plus fort près de l'autel
                                            if let Some(stone) = assault.calling() {
                                                let near = 1.0 - (camera.player_pos.distance(stone) / block / 60.0).clamp(0.0, 1.0);
                                                audio.set_whisper(0.25 + 0.75 * near, &ui_state.audio_settings);
                                            }
                                            apply_assault_events(assault_events, &mut camera, &generator, &mut lod_manager, &mut state, &mut skill_effects, &mut audio, &ui_state.audio_settings, &mut cinematics);
                                        }
                                    }
                                    None => audio.set_whisper(0.0, &ui_state.audio_settings),
                                }
                            }
                            state.update_effects(&effect_quads);
                            state.set_creature_frame(&frame);
                            creature_vertices = frame.vertices.len() + frame.skinned_vertices as usize;
                            probe.section("créatures : animation et maillage", section);
                        }

                        // Connexion menée en tâche de fond : à son terme, la planète du serveur
                        // remplace la partie solo
                        if let Some(result) = pending_join.as_ref().and_then(|p| p.poll()) {
                            let addr = pending_join.take().map(|p| p.addr).unwrap_or_default();
                            match result {
                                Ok(world) => {
                                    end_vision_tutorial(&mut tutorial, &mut ui_state, &mut solo_creatures);
                                    if vision.take().is_some() {
                                        generator = generator.with_epoch(herocraft_world::Epoch::Present);
                                        lod_manager.clear(current_seed);
                                    }
                                    let edits = Arc::new(herocraft_world::TerrainEdits::from_chunks(world.edited_chunks));
                                    if let Some(host) = hosted.as_ref().filter(|_| world.seed == current_seed) {
                                        // Notre propre partie : même planète et mêmes blocs, la vue et le
                                        // joueur restent en place, seuls les blocs passent par le serveur
                                        camera.set_terrain(edits.clone());
                                        lod_manager.set_terrain_edits(edits);
                                        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(None, 256);
                                        ui_state.multiplayer.status = NetStatus::Hosting { port: host.port };
                                        // Le serveur tient désormais la progression : celle du solo
                                        // est mise de côté jusqu'au retour
                                        ui_state.player.save();
                                        ui_state.player = player_stats::PlayerStats::load_or_default(None);
                                        // La voie vient de la légende, que la progression n'enregistre pas
                                        let legend = ui_state.legend.take();
                                        ui_state.set_legend(legend);
                                        window.set_title(&format!("HeroCraft — partie ouverte au réseau local (port {})", host.port));
                                    } else {
                                        current_seed = world.seed;
                                        switch_planet(current_seed, true, edits, None, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state, &mut client_weather);
                                        dropped_items.clear();
                                        // De retour sur ce serveur : là où le joueur l'avait quitté
                                        if let Some(spawn) = world.spawn_position {
                                            camera.player_pos = spawn;
                                            camera.sync_orbit_to_ground();
                                        }
                                        ui_state.multiplayer.status = NetStatus::Online(addr.clone());
                                        window.set_title(&format!("HeroCraft — en ligne sur {}", addr));
                                    }
                                    ui_state.multiplayer.notice = None;
                                    block_interaction.invalidate();
                                    terrain_edited = true;
                                    let mut client = world.client;
                                    // Le serveur apprend la voie du héros (ses jauges en dépendent)
                                    if let Some(legend) = &ui_state.legend {
                                        client.send_legend(legend);
                                    }
                                    camera.hero_height = hero_height(&ui_state);
                                    net = Some(client);
                                }
                                Err(e) => {
                                    warn!("Connexion à {} impossible : {}", addr, e);
                                    ui_state.multiplayer.notice = Some(format!("Connexion impossible : {}", e));
                                    ui_state.multiplayer.status = NetStatus::Solo;
                                    hosted = None;
                                }
                            }
                        }

                        // Page Multijoueur ouverte en solo : écoute des parties annoncées sur le réseau local
                        let browsing = ui_state.is_settings_open
                            && ui_state.menu_page == MenuPage::Multiplayer
                            && ui_state.multiplayer.status == NetStatus::Solo;
                        if browsing {
                            if lan_browser.is_none() && now >= lan_retry_at {
                                match LanBrowser::new() {
                                    Ok(browser) => lan_browser = Some(browser),
                                    Err(e) => {
                                        log::debug!("Recherche des parties du réseau local impossible : {}", e);
                                        lan_retry_at = now + std::time::Duration::from_secs(2);
                                    }
                                }
                                ui_state.multiplayer.lan_search_ok = lan_browser.is_some();
                            }
                            if let Some(browser) = lan_browser.as_mut() {
                                ui_state.multiplayer.lan_servers = browser
                                    .poll()
                                    .iter()
                                    .map(|s| ui::LanEntry { name: s.name.clone(), addr: s.addr.to_string(), players: s.players })
                                    .collect();
                            }
                        } else if lan_browser.is_some() {
                            lan_browser = None;
                            ui_state.multiplayer.lan_servers.clear();
                        }
                        // Blocs tombés au sol : chute, ramassage par le joueur au sol (menu fermé), dessin
                        if let Some(collider) = camera.collider.as_ref() {
                            let h = collider.block_height();
                            if !ui_state.is_settings_open {
                                let on_ground = camera.target_distance <= camera.base_radius;
                                let body = on_ground.then(|| camera.player_pos + camera.player_pos.normalize_or_zero() * 0.9 * h);
                                dropped_items.update(dt, &generator, collider, body, &mut ui_state.inventory);
                            }
                            for event in dropped_items.take_stone_events() {
                                stone_events.push(event);
                            }
                            if let Some(mesh) = dropped_items.take_mesh(h, game_start.elapsed().as_secs_f32()) {
                                state.set_dropped_items(&mesh);
                            }
                        }
                        for event in void_stones::dispatch(&mut stone_events, &mut ui_state.chat) {
                            // Une Pierre entière obtenue : la vision (`fin-de-la-vision`), puis
                            // `vision_ended` la fait absorber
                            if let herocraft_common::StoneEvent::Obtained { kind: herocraft_common::StoneKind::Whole, item, .. } = event {
                                vision_stone = Some(item);
                            }
                            // L'éclat du tutoriel ramassé : le soldat s'éveille en héros
                            if let Some(assault) = tutorial.as_mut().and_then(|t| t.assault.as_mut()) {
                                assault.on_stone(&event);
                            }
                            if tutorial.as_mut().is_some_and(|t| t.on_stone(&event)) {
                                match net.as_mut() {
                                    Some(client) => {
                                        client.send_vision_awaken();
                                        ui_state.chat.info("La force de l'éclat t'envahit : tes premières compétences s'ouvrent.");
                                    }
                                    None => {
                                        let names = herocraft_common::vision::awaken(&mut ui_state.player.stats);
                                        let stats = ui_state.player.stats.clone();
                                        ui_state.skill_bar.fill_learned(&stats);
                                        ui_state.chat.info(format!("La force de l'éclat t'envahit : {} s'ouvrent.", names.join(", ")));
                                    }
                                }
                                let (_, forward, _, _) = camera.look_vectors();
                                skill_effects.start(vec![skill_play::VisualCue {
                                    effect: "eclatEveil".into(),
                                    anchor: herocraft_common::skills::Anchor::Feet,
                                    caster: None,
                                    point: camera.player_pos,
                                    creature: None,
                                    aim: forward,
                                    hold: None,
                                    area_blocks: 1.0,
                                    travel: None,
                                    delay: 0.0,
                                    projectile: None,
                                }]);
                                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::VISION_AWAKENED).at(camera.player_pos));
                            }
                            if let Some(cinematic_event) = void_stones::cinematic_event(&event) {
                                cinematics.notify(&cinematic_event);
                            }
                        }
                        {
                            herocraft_common::prof_scope!("blocs::contour");
                            // Une bête visée cache le contour et les fissures du bloc derrière elle
                            state.set_block_outline(block_interaction.outline(&camera).filter(|_| aimed.is_none()));
                            state.set_block_cracks(block_interaction.cracks(&camera).filter(|_| aimed.is_none()));
                        }
                        if last_autosave.elapsed().as_secs() >= 30 && bench.is_none() {
                            save_terrain_edits(&lod_manager);
                            if net.is_none() && hosted.is_none() {
                                save_solo_creatures(&solo_creatures, &solo_fields);
                                save_sky_clock(current_seed, &state, &client_weather);
                            }
                            ui_state.inventory.save();
                            last_autosave = Instant::now();
                        }

                        // Mise à jour adaptative du QuadTree LOD
                        herocraft_common::prof_scope!("frame::lod_and_upload" => lod_guard);
                        let section = Instant::now();
                        if lod_manager.update(camera.lod_eye(), &generator, &mut chunk_cache, dt, None)
                            || terrain_edited
                            || state.has_pending_lod_uploads()
                        {
                            state.sync_lod_meshes(&lod_manager);
                        }
                        drop(lod_guard);
                        probe.section("LOD et envoi au GPU", section);

                        // 1. Mise à jour de l'interface egui
                        let ui_section = Instant::now();
                        let is_ground = camera.distance <= camera.base_radius;
                        // Carte et boussole : position du joueur, demandes au fil de la carte
                        ui_state.world_map.navigation = is_ground.then(|| world_map::Navigation {
                            up: camera.player_pos.as_dvec3().normalize(),
                            yaw: camera.player_yaw as f64,
                        });
                        let map_ctx = ui_state.ctx.clone();
                        ui_state.world_map.pump(&map_ctx, &generator, ui_state.map_open);
                        let cooldown_now = Instant::now();
                        for slot in 0..skill_bar::SKILL_SLOTS {
                            ui_state.fusion_face = skill_play.fusion_face(&ui_state.player.stats);
                            ui_state.skill_cooldowns[slot] = ui_state.skill_bar.get(slot).map_or((0.0, 0.0), |id| skill_play.cooldown(id, cooldown_now));
                        }
                        ui_state.cinematic = cinematics.overlay();
                        let (full_output, maybe_action) = ui_state.update_ui(is_ground);

                        // Roue des compétences : point investi et expérience d'essai. En ligne, le
                        // serveur juge et renvoie les statistiques ; en solo, le jeu applique les
                        // mêmes règles et enregistre la progression.
                        let skill_requests = std::mem::take(&mut ui_state.skill_requests);
                        if skill_requests.experience > 0 && net.is_none() {
                            let level = ui_state.player.stats.level;
                            ui_state.player.stats.add_experience(skill_requests.experience);
                            if ui_state.player.stats.level > level {
                                info!("Niveau {} atteint : {} point(s) à investir.", ui_state.player.stats.level, ui_state.player.stats.skill_points);
                            }
                            ui_state.player.save();
                        }
                        if let Some(id) = skill_requests.learn {
                            let wheel = &ui_state.skill_wheel;
                            match wheel.learn(&mut ui_state.player.stats, &id) {
                                Ok(rank) => {
                                    info!("Compétence {id} au rang {rank}.");
                                    let stats = ui_state.player.stats.clone();
                                    ui_state.skill_bar.fill_learned(&stats);
                                }
                                Err(e) => warn!("Compétence {id} : {}.", e.message()),
                            }
                            match net.as_mut() {
                                Some(client) => client.send_learn_skill(id),
                                None => ui_state.player.save(),
                            }
                        }

                        // Commandes de test (`HEROCRAFT_COMMANDES=fichier`) : les lignes déposées dans
                        // le fichier passent comme si elles avaient été tapées dans le chat
                        if let Some(file) = command_file.as_ref() {
                            if let Some(text) = std::fs::read_to_string(file).ok().filter(|t| !t.trim().is_empty()) {
                                let _ = std::fs::write(file, "");
                                for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                                    ui_state.chat.inject(line.to_string());
                                }
                            }
                        }
                        // Chat : message envoyé aux autres (en ligne) ou commande exécutée ici
                        for line in ui_state.chat.take_submitted() {
                            if line.starts_with('/') {
                                ui_state.chat.echo(&line);
                                let result = commands::parse(&line).and_then(|command| {
                                    let mut game = Game {
                                        camera: &mut camera,
                                        generator: &generator,
                                        sky: &mut state,
                                        ui: &mut ui_state,
                                        net: net.as_mut(),
                                        creatures: &mut solo_creatures,
                                        registry: &creature_registry,
                                        seed: current_seed,
                                        pending_actions: &mut pending_actions,
                                        next_planet_seed: &mut next_planet_seed,
                                        weather: &mut client_weather,
                                        cinematics: &mut cinematics,
                                        dropped: &mut dropped_items,
                                    };
                                    run_command(command, &mut game)
                                });
                                match result {
                                    Ok(lines) => lines.into_iter().for_each(|l| ui_state.chat.info(l)),
                                    Err(e) => ui_state.chat.error(e),
                                }
                            } else if let Some(client) = net.as_mut() {
                                // Le serveur renvoie le message à tous, nous compris
                                client.send_chat(&line);
                            } else {
                                let pseudo = ui_state.multiplayer.pseudo.trim().to_string();
                                ui_state.chat.player(&pseudo, &line);
                            }
                        }
                        if net.is_none() {
                            ui_state.chat.players.clear();
                        }

                        // Compétences qui se chargent : le serveur compte le temps tenu jusqu'au relâchement
                        for skill in std::mem::take(&mut ui_state.skill_charges) {
                            skill_play.begin_charge(Some(&skill));
                            if let Some(client) = net.as_mut() {
                                client.send_begin_charge(Some(&skill));
                            }
                        }
                        // Compétences lancées (touches de la barre, /cast) : visée du regard, et du
                        // bloc visé s'il y en a un
                        for skill in std::mem::take(&mut ui_state.skill_casts) {
                            let (_, _, _, aim) = camera.look_vectors();
                            // Un grappin vise plus loin que la main : jusqu'à sa portée
                            let aimed = match skill_play::grapple_of(herocraft_common::skills::skills(), &skill).filter(|g| g.to == herocraft_common::skills::GrappleTo::Block) {
                                Some(grapple) => {
                                    let rank = herocraft_common::skills::skills().rank(&ui_state.player.stats, &skill);
                                    block_interaction.hit_within(&camera, &generator, &lod_manager.terrain_edits().clone(), grapple.range.at(rank) as f64).map(|d| camera.eye_position() + aim * d)
                                }
                                None => block_interaction.target().map(|hit| camera.eye_position() + aim * hit.distance as f32),
                            };
                            let result = match net.as_mut() {
                                Some(client) => skill_play.request(&skill, &mut ui_state.player.stats, aim, aimed, Some(&mut |s: &str, aim, aimed| client.send_cast_skill(s, aim, aimed))),
                                None => skill_play.request(&skill, &mut ui_state.player.stats, aim, aimed, None),
                            };
                            if let Err(reason) = result {
                                ui_state.skill_notice = Some((format!("{} : {reason}", skill_name(&skill)), Instant::now()));
                            }
                        }

                        // 2. Traitement des actions déclenchées par les boutons egui et le clavier
                        pending_actions.extend(maybe_action);
                        for action in pending_actions.drain(..) {
                            herocraft_common::prof_scope!("frame::ui_action");
                            match action {
                                UiAction::PreviewSound => audio.preview(&ui_state.audio_settings),
                                UiAction::Vision(true) if vision.is_none() => {
                                    let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
                                    let entry = geometry
                                        .block_at_point(camera.player_pos.as_dvec3())
                                        .and_then(|at| generator.vision_entry(at.face, at.x, at.z, VISION_TEMPLE_BLOCKS, herocraft_world::Culture::Humains));
                                    match entry {
                                        Some(entry) => {
                                            info!("Vision du temple {:?} ({}, {}), tenu par les {}.", entry.site.face, entry.site.center().0, entry.site.center().1, entry.holder.name());
                                            // Les blocs du présent sont enregistrés avant de les mettre de côté
                                            let _ = chunk_cache.flush_dirty();
                                            save_terrain_edits(&lod_manager);
                                            let feet = geometry.corner(entry.feet.face, entry.feet.x as f64 + 0.5, entry.feet.y as f64, entry.feet.z as f64 + 0.5).as_vec3();
                                            let back = camera.player_pos;
                                            switch_epoch(Some(entry.holder), feet, Some(back), None, &mut vision, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state);
                                            if tutorial.is_none() {
                                                tutorial = begin_vision_tutorial(feet, &camera, &generator, &mut lod_manager, &mut ui_state, Some((&mut solo_creatures, &creature_registry)));
                                                state.sync_lod_meshes(&lod_manager);
                                            }
                                            cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::VISION_ENTERED));
                                            block_interaction.invalidate();
                                        }
                                        None => ui_state.chat.error("No temple nearby"),
                                    }
                                }
                                UiAction::Vision(false) => {
                                    if let Some(back) = vision.as_ref().and_then(|v| v.back) {
                                        let storage = solo_terrain(current_seed).0;
                                        switch_epoch(None, back, None, storage, &mut vision, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state);
                                        // Le souvenir achevé : il revient dans un nuage de fumée violette
                                        if end_vision_tutorial(&mut tutorial, &mut ui_state, &mut solo_creatures) {
                                            let feet = camera.player_pos;
                                            skill_effects.start(["shadowSmoke", "mistCloud"].into_iter().map(|effect| skill_play::VisualCue {
                                                effect: effect.into(),
                                                anchor: herocraft_common::skills::Anchor::Feet,
                                                caster: None,
                                                point: feet,
                                                creature: None,
                                                aim: glam::Vec3::ZERO,
                                                hold: None,
                                                area_blocks: 1.0,
                                                travel: None,
                                                delay: 0.0,
                                                projectile: None,
                                            }).collect());
                                            ui_state.chat.info("Tu reviens au présent sans souvenir de la bataille. Mais sa force est restée en toi, et un éclat du cristal brille dans ton sac.");
                                        }
                                        block_interaction.invalidate();
                                    }
                                }
                                UiAction::Vision(true) => {}
                                UiAction::LegendSealed => {
                                    let Some(legend) = ui_state.take_sealed_legend() else { continue };
                                    let online = ui_state.multiplayer.status != NetStatus::Solo && hosted.is_none();
                                    let file = legend_file(current_seed, online);
                                    if let Err(e) = save_legend(&file, &legend) {
                                        warn!("Légende non enregistrée dans {} : {}", file.display(), e);
                                    }
                                    if let Some(client) = net.as_mut() {
                                        client.send_legend(&legend);
                                    }
                                    info!(
                                        "Légende scellée : {} ({} · {}, {:.2} m).",
                                        legend.appearance.nom,
                                        legend.appearance.species().nom,
                                        legend.class.label(),
                                        legend.appearance.taille
                                    );
                                    // Le créateur reste ouvert sur « Entrer dans le monde »
                                    let creator = ui_state.creator.take();
                                    ui_state.set_legend(Some(legend));
                                    ui_state.creator = creator;
                                    camera.hero_height = hero_height(&ui_state);
                                }
                                UiAction::Respawn => {
                                    ui_state.player.respawn();
                                    if let Some(client) = net.as_mut() {
                                        client.send_respawn();
                                    }
                                    // Tombé dans le souvenir : il se relève au dernier point franchi
                                    let checkpoint = tutorial.as_ref().and_then(|t| t.assault.as_ref()).and_then(|a| a.checkpoint());
                                    match checkpoint {
                                        Some(dir) => {
                                            let dir = dir.normalize_or_zero();
                                            camera.player_pos = dir * camera.ground_radius_at(dir, &generator);
                                            camera.sync_orbit_to_ground();
                                        }
                                        None => camera.spawn_on_ground(&generator),
                                    }
                                    audio.reset_player();
                                    info!("Réapparition au point d'apparition.");
                                }
                                UiAction::ResumeGame => {
                                    ui_state.set_settings_open(false);
                                    info!("Bouton cliqué : Reprise de la partie.");
                                }
                                UiAction::QuitGame => {
                                    if net.is_none() && hosted.is_none() {
                                        save_solo_creatures(&solo_creatures, &solo_fields);
                                        save_sky_clock(current_seed, &state, &client_weather);
                                    }
                                    if let Some(client) = net.as_mut() {
                                        client.disconnect();
                                    }
                                    hosted = None;
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    ui_state.inventory.save();
                                    ui_state.player.save();
                                    info!("Bouton cliqué : Sauvegarde des chunks et fermeture du jeu.");
                                    write_profile_report();
                                    crate::shaders::save_pipeline_cache();
                                    target.exit();
                                }
                                UiAction::ToggleVsync => {
                                    state.set_vsync(ui_state.video_settings.vsync);
                                    info!("Paramètre V-Sync mis à jour : {}", if ui_state.video_settings.vsync { "ACTIVÉE" } else { "DÉSACTIVÉE" });
                                }
                                UiAction::ToggleFullscreen => {
                                    let is_fs = ui_state.video_settings.fullscreen;
                                    if is_fs {
                                        window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                                    } else {
                                        window.set_fullscreen(None);
                                    }
                                    info!("Paramètre Plein Écran mis à jour : {}", if is_fs { "ACTIVÉ" } else { "DÉSACTIVÉ" });
                                }
                                UiAction::SetLodDistance => {
                                    lod_manager.set_distance_preset(ui_state.video_settings.lod_distance);
                                    lod_manager.set_precision(ui_state.video_settings.lod_precision);
                                    info!("Paramètre Distance LOD mis à jour : {:?}", ui_state.video_settings.lod_distance);
                                }
                                UiAction::SetLodPrecision => {
                                    lod_manager.set_precision(ui_state.video_settings.lod_precision);
                                    info!("Paramètre Précision LOD mis à jour : {:?}", ui_state.video_settings.lod_precision);
                                }
                                UiAction::SetFpsLimit => {
                                    info!("Paramètre Limite FPS mis à jour : {:?}", ui_state.video_settings.fps_limit);
                                }
                                UiAction::GenerateNewPlanet if ui_state.multiplayer.status != NetStatus::Solo => {
                                    info!("Nouvelle planète impossible en ligne : la planète est celle du serveur.");
                                }
                                UiAction::OpenToLan | UiAction::JoinServer if ui_state.multiplayer.status != NetStatus::Solo => {}
                                UiAction::OpenToLan => {
                                    // Le serveur reprend la sauvegarde du monde : elle est d'abord mise à jour
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    save_solo_creatures(&solo_creatures, &solo_fields);
                                    save_sky_clock(current_seed, &state, &client_weather);
                                    solo_seed = current_seed;
                                    let pseudo = ui_state.multiplayer.pseudo.trim().to_string();
                                    let save_dir = std::path::PathBuf::from(solo_save_dir(current_seed));
                                    match HostedServer::start(current_seed, save_dir, &pseudo) {
                                        Ok(host) => {
                                            let addr = format!("127.0.0.1:{}", host.port);
                                            ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
                                            ui_state.multiplayer.notice = None;
                                            pending_join = Some(PendingJoin::start(addr, pseudo));
                                            hosted = Some(host);
                                        }
                                        Err(e) => {
                                            warn!("{}", e);
                                            ui_state.multiplayer.notice = Some(e);
                                        }
                                    }
                                }
                                UiAction::JoinServer => {
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    save_solo_creatures(&solo_creatures, &solo_fields);
                                    save_sky_clock(current_seed, &state, &client_weather);
                                    solo_seed = current_seed;
                                    let addr = network::server_address(&ui_state.multiplayer.join_target);
                                    let pseudo = ui_state.multiplayer.pseudo.trim().to_string();
                                    info!("Connexion au serveur {} en tant que « {} »...", addr, pseudo);
                                    ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
                                    ui_state.multiplayer.notice = None;
                                    pending_join = Some(PendingJoin::start(addr, pseudo));
                                }
                                UiAction::LeaveServer => {
                                    end_vision_tutorial(&mut tutorial, &mut ui_state, &mut solo_creatures);
                                    if vision.take().is_some() {
                                        generator = generator.with_epoch(herocraft_world::Epoch::Present);
                                        lod_manager.clear(current_seed);
                                    }
                                    if let Some(mut client) = net.take() {
                                        client.disconnect();
                                    }
                                    pending_join = None;
                                    // Le serveur ouvert ici s'arrête en enregistrant le monde solo
                                    let was_hosting = hosted.take().is_some();
                                    state.set_player_outlines(&[]);
                                    ui_state.name_tags.clear();
                                    let (storage, edits) = solo_terrain(solo_seed);
                                    // Le serveur ouvert ici a pu faire vivre (et tuer) les bêtes du monde
                                    // solo : elles sont reprises de la sauvegarde qu'il vient d'écrire
                                    solo_creatures = solo_creature_world(camera.base_radius as f64, solo_seed, &creature_registry);
                                    solo_fields = solo_fields_of(solo_seed);
                                    creatures_seed = solo_seed;
                                    if was_hosting && current_seed == solo_seed {
                                        // Même planète : le joueur reste où il est, avec les blocs des autres
                                        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
                                        ui_state.player = player_stats::PlayerStats::load_or_default(Some(progress_file(solo_seed)));
                                        // La voie vient de la légende, que la progression n'enregistre pas
                                        let legend = ui_state.legend.take();
                                        ui_state.set_legend(legend);
                                        camera.set_terrain(edits.clone());
                                        lod_manager.set_terrain_edits(edits);
                                        // Le serveur a enregistré son horloge dans le monde solo
                                        let clock = herocraft_world::SkyClock::load(solo_save_dir(solo_seed));
                                        state.simulated_time = clock.time as f32;
                                        client_weather.model.forced = clock.forced;
                                    } else {
                                        current_seed = solo_seed;
                                        switch_planet(current_seed, false, edits, storage, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state, &mut client_weather);
                                        dropped_items.clear();
                                    }
                                    block_interaction.invalidate();
                                    ui_state.multiplayer.status = NetStatus::Solo;
                                    window.set_title(WINDOW_TITLE);
                                    info!("Retour à la partie solo (planète #{}).", current_seed);
                                }
                                UiAction::GenerateNewPlanet => {
                                    end_vision_tutorial(&mut tutorial, &mut ui_state, &mut solo_creatures);
                                    vision = None;
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    save_solo_creatures(&solo_creatures, &solo_fields);
                                    save_sky_clock(current_seed, &state, &client_weather);
                                    current_seed = next_planet_seed.take().unwrap_or_else(herocraft_world::random_seed);
                                    let (storage, edits) = solo_terrain(current_seed);
                                    switch_planet(current_seed, false, edits, storage, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state, &mut client_weather);
                                    dropped_items.clear();
                                    info!("Nouvelle planète générée ! (Graine: #{})", current_seed);
                                }
                                UiAction::ToggleGroundCamera => {
                                    camera.toggle_view(&generator);
                                    info!(
                                        "Bouton cliqué : Transition vers {}",
                                        if camera.target_distance <= camera.base_radius { "Vue Première Personne (FPS)" } else { "Vue Satellite (Orbite)" }
                                    );
                                }
                                UiAction::ToggleAutoRotate => {
                                    camera.auto_rotate = !camera.auto_rotate;
                                    ui_state.set_auto_rotate(camera.auto_rotate);
                                    info!("Auto-rotation = {}", camera.auto_rotate);
                                }
                                UiAction::ResetCamera => {
                                    camera.reset();
                                    info!("Vue réinitialisée.");
                                }
                                UiAction::ToggleTectonics | UiAction::ToggleOceanCurrents => {
                                    let filter = ui_state.active_filter();
                                    lod_manager.set_filter(filter);
                                    lod_manager.update(camera.lod_eye(), &generator, &mut chunk_cache, 0.0, None);
                                    state.sync_lod_meshes(&lod_manager);
                                    info!("Filtre de visualisation sélectionné : {:?}", filter);
                                }
                            }
                        }

                        // 3. Tessellation egui
                        herocraft_common::prof_scope!("frame::egui_tessellate" => tess_guard);
                        let paint_jobs = ui_state.ctx.tessellate(full_output.shapes, ui_state.scale_factor);
                        drop(tess_guard);
                        probe.section("interface", ui_section);
                        let screen_descriptor = egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [state.size.width, state.size.height],
                            pixels_per_point: ui_state.scale_factor,
                        };

                        // 4. Rendu WGPU (3D + egui pass)
                        match state.render(&paint_jobs, &screen_descriptor, &full_output.textures_delta) {
                            Ok(_) => {}
                            Err(renderer::FrameError::Lost) => state.resize(state.size),
                            Err(e) => log::debug!("Image sautée : {:?}", e),
                        }
                        let timings = state.render_timings;
                        probe.push("rendu : attente d'une image libre", timings.acquire);
                        probe.push("rendu : préparation", timings.prepare);
                        probe.push("rendu : envoi au GPU", timings.submit);
                        probe.push("rendu : présentation", timings.present);

                        // Limiteur de framerate si V-Sync est désactivée
                        if !ui_state.video_settings.vsync {
                            let min_duration = match ui_state.video_settings.fps_limit {
                                crate::ui::FpsLimitSetting::Fps60 => Some(std::time::Duration::from_millis(16)),
                                crate::ui::FpsLimitSetting::Fps120 => Some(std::time::Duration::from_micros(8333)),
                                crate::ui::FpsLimitSetting::Unlimited => None,
                            };
                            if let Some(target_dur) = min_duration {
                                let frame_elapsed = last_frame_time.elapsed();
                                if frame_elapsed < target_dur {
                                    herocraft_common::prof_scope!("frame::fps_limiter_sleep");
                                    std::thread::sleep(target_dur - frame_elapsed);
                                }
                            }
                        }

                        // À-coup : signalé dans la console avec ses étapes les plus longues, et gardé
                        // quelques secondes à l'écran sous le compteur FPS
                        let record = probe.finish(now);
                        let total_ms = record.total().as_secs_f32() * 1000.0;
                        if total_ms >= benchmark::HITCH_MS {
                            // Au plus un message par seconde : une machine très lente n'inonde pas la console
                            if last_hitch_log.is_none_or(|t: Instant| t.elapsed().as_secs_f32() >= 1.0) {
                                warn!("À-coup : image de {:.0} ms ({}).", total_ms, record.describe());
                                last_hitch_log = Some(Instant::now());
                            }
                            ui_state.set_hitch(total_ms);
                        }
                        if let Some(b) = bench.as_mut() {
                            let shown = if net.is_none() { solo_creatures.creatures().len() } else { creature_views.len() };
                            if b.after_frame(dt, &record, shown, creature_vertices) {
                                info!("Banc d'essai terminé : rapport dans ./benchmark/.");
                                write_profile_report();
                                crate::shaders::save_pipeline_cache();
                                target.exit();
                            }
                        }

                        drop(frame_guard);
                        herocraft_common::profiler::frame_mark();
                    }

                    _ => {}
                }
            }

            // Mouvement brut de la souris : oriente le regard quand le curseur est capturé
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                herocraft_common::prof_scope!("boucle::mouvement_souris");
                if cursor_captured && !cinematics.is_playing() {
                    let controls = &ui_state.controls;
                    let step = input::LOOK_RADIANS_PER_COUNT * controls.mouse_sensitivity;
                    let pitch_sign = if controls.invert_y { 1.0 } else { -1.0 };
                    camera.look(delta.0 as f32 * step, delta.1 as f32 * step * pitch_sign);
                }
            }

            Event::AboutToWait => {
                window.request_redraw();
            }

            _ => {}
        }
    }).expect("Erreur dans la boucle d'événements");
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
#[allow(clippy::too_many_arguments)]
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
            let ids: Vec<u64> = tutorial.soldier_spots(feet, block).into_iter().map(|dir| {
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
#[allow(clippy::too_many_arguments)]
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
    let fighters: Vec<u64> = t.assault.as_ref().map(|a| a.creatures().collect()).unwrap_or_default();
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
fn aimed_corpse(camera: &OrbitCamera, interaction: &BlockInteraction, views: &[creatures::CreatureView], registry: &herocraft_common::CreatureRegistry) -> Option<u64> {
    let (eye, dir, arm) = camera.view_ray()?;
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let reach = interaction.target().map_or(arm + block_interaction::REACH_BLOCKS as f32 * block, |hit| hit.distance as f32);
    creatures::aimed_corpse(views, registry, eye, dir, reach, block).map(|(id, _)| id)
}

/// Créature vivante sous le réticule, à portée et devant le bloc visé
fn aimed_creature(camera: &OrbitCamera, interaction: &BlockInteraction, views: &[creatures::CreatureView], registry: &herocraft_common::CreatureRegistry) -> Option<u64> {
    let (eye, dir, arm) = camera.view_ray()?;
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let reach = interaction.target().map_or(arm + block_interaction::REACH_BLOCKS as f32 * block, |hit| hit.distance as f32);
    creatures::aimed_creature(views, registry, eye, dir, reach, block).map(|(id, _)| id)
}

/// Donne à la bête visée ce que le joueur tient en main, si c'est sa nourriture et qu'elle en veut :
/// la simulation solo, ou le serveur en ligne, l'apprivoise, la rend amoureuse ou fait grandir le
/// petit. Renvoie vrai si elle a mangé (l'objet est retiré de l'inventaire).
#[allow(clippy::too_many_arguments)]
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
    id: u64,
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
            solo.wound(id, camera.player_pos, 0, damage * factor * boost);
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
fn skin_creature(id: u64, net: Option<&mut NetClient>, solo: &mut herocraft_world::CreatureWorld, dropped: &mut dropped_items::DroppedItems, camera: &OrbitCamera) {
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
            game.cinematics.player.play(&id)?;
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

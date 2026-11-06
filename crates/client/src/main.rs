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
mod block_outline;
mod block_textures;
mod camera;
mod chat;
mod cloud_mesh;
mod collision;
mod commands;
mod creature_render;
mod creatures;
mod dropped_items;
mod gpu_timer;
mod hero_creator;
mod hero_model;
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
mod terrain_cull;
mod ui;
mod ui_skin;
#[cfg(test)]
mod ui_snapshot;
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
    info!(" • [1-9] / Molette / [E]              : Choisir la case de la barre d'objets / Inventaire");
    info!(" • [Z,Q,S,D] (AZERTY) / [W,A,S,D]     : Se déplacer");
    info!(" • [ESPACE] / double [ESPACE]         : Sauter / S'envoler (monter en vol)");
    info!(" • [MAJ] / [CTRL]                     : S'accroupir (descendre en vol) / Sprinter");
    info!(" • [ALT] + Molette                    : Zoom FPS (0m) -> TPS (1-25m) -> Satellite");
    info!(" • [F] / [R]                          : Vue sol <-> satellite / Réinitialiser la vue");
    info!(" • [C]                                : Roue des compétences");
    info!(" • [G] / [T] / [Y]                    : Nouvelle planète / Filtres tectonique, courants");
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
    lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
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
    // Chat : Tab complète les blocs et objets (`/give`) et les créatures (`/summon`)
    ui_state.chat.items = commands_items();
    ui_state.chat.creatures = creature_registry.iter().map(|s| s.id.clone()).collect();
    ui_state.chat.info(format!("Press [{}] to chat, /help for the commands", ui_state.controls.action_key_label(Action::OpenChat)));
    // Graine demandée par `/planet <graine>` pour la prochaine planète
    let mut next_planet_seed: Option<u64> = None;
    let mut creatures_seed = current_seed;
    let mut creature_animator = creatures::CreatureAnimator::default();
    let mut hero_gait = hero_model::HeroGait::default();
    let mut hero_figure = hero_model::FigureCache::default();
    let (creature_models, creature_textures) = creatures::CreatureModels::load(&creature_registry);
    // Créatures dessinées à la dernière image, que le réticule peut viser, et délai avant le
    // prochain coup qu'on peut leur porter
    let mut creature_views: Vec<creatures::CreatureView> = Vec::new();
    let mut attack_cooldown = 0.0_f32;

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
    // Puce intégrée : ombres et rayons allégés, MSAA coupé (réglables dans les options)
    ui_state.adapt_to_gpu(state.integrated_gpu());
    state.set_creature_textures(&creature_textures);
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
                            save_solo_creatures(&solo_creatures);
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
                        if cursor_captured {
                            // Pose appliquée et remaillée sur-le-champ : le bloc change dans la frame qui
                            // suit le clic. Le clic gauche (casse, coup à une bête) avance à chaque frame.
                            let pressed = *btn_state == ElementState::Pressed;
                            if let Some(action) = block_interaction.on_mouse_button(*button, pressed) {
                                block_interaction.update_target(&camera, &generator, &lod_manager.terrain_edits().clone());
                                // Clic droit sur un établi : il s'ouvre (accroupi, on pose contre lui)
                                let station = block_interaction.aimed_station().filter(|_| action == block_interaction::BlockAction::Place && !camera.is_crouching());
                                // Clic droit sur une bête morte, couteau ou dague en main : on la dépèce
                                let knife = action == block_interaction::BlockAction::Place && ui_state.inventory.slot(ui_state.inventory.selected()).is_some_and(|s| s.item.can_skin());
                                let corpse = if knife { aimed_corpse(&camera, &block_interaction, &creature_views, &creature_registry) } else { None };
                                // Clic droit, un aliment en main : le joueur le mange
                                let food = ui_state.inventory.selected_food().filter(|_| action == block_interaction::BlockAction::Place && station.is_none());
                                if let Some(id) = corpse {
                                    skin_creature(id, net.as_mut(), &mut solo_creatures, &mut dropped_items, &camera);
                                    audio.block_placed(audio::synth::Material::Dirt, 0.0, &ui_state.audio_settings);
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
                                } else if let Some((pos, block, previous)) = block_interaction.apply(action, &camera, &generator, &mut lod_manager, &mut ui_state.inventory) {
                                    block_sound(&mut audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                                    drop_broken_block(&mut dropped_items, &camera, pos, block, previous);
                                    state.sync_lod_meshes(&lod_manager);
                                    if let Some(client) = net.as_mut() {
                                        client.send_block_change(pos, block);
                                    }
                                }
                            }
                        }
                        if !cursor_captured && matches!(button, MouseButton::Left | MouseButton::Right) {
                            let is_over_ui = ui_state.is_settings_open
                                || ui_state.chat.open
                                || ui_state.ctx.is_pointer_over_area()
                                || ui_state.ctx.wants_pointer_input();
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
                                || ui_state.ctx.is_pointer_over_area()
                                || ui_state.ctx.wants_pointer_input())
                        };

                        if cursor_captured && !zoom_modifier_held {
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

                        if key_code == KeyCode::Escape && is_pressed && ui_state.chat.open {
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
                        } else if ui_state.inventory_open {
                            // Écran d'inventaire : seule sa touche le referme, sauf pendant la saisie
                            // d'une recherche dans les recettes
                            let typing = ui_state.ctx.wants_keyboard_input();
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
                                if is_pressed {
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
                            let in_game = !ui_state.is_settings_open && !ui_state.screen_open() && !ui_state.chat.open;
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

                        // Mise à jour de la caméra uniquement si le menu n'est pas ouvert
                        let section = Instant::now();
                        if !ui_state.is_settings_open {
                            herocraft_common::prof_scope!("frame::camera_update");
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
                        attack_cooldown = (attack_cooldown - dt).max(0.0);
                        let mut aimed = None;
                        if cursor_captured {
                            let edits = lod_manager.terrain_edits().clone();
                            block_interaction.update_target(&camera, &generator, &edits);
                            aimed = aimed_creature(&camera, &block_interaction, &creature_views, &creature_registry);
                            // Une bête devant le bloc visé : c'est elle qu'on frappe, au rythme des
                            // coups, et le bloc derrière elle ne se fissure pas
                            let striking = aimed.filter(|_| block_interaction.is_breaking_held());
                            if let Some(id) = striking {
                                block_interaction.pause_mining();
                                hit_creature(id, &mut attack_cooldown, &camera, net.as_mut(), &mut solo_creatures, &mut audio, &ui_state.audio_settings);
                            }
                            let action = if striking.is_some() { None } else { block_interaction.tick(dt) };
                            if let Some(block) = block_interaction.take_hit() {
                                audio.block_hit(audio::synth::Material::of(block), &ui_state.audio_settings);
                            }
                            if let Some(action) = action {
                                if let Some((pos, block, previous)) = block_interaction.apply(action, &camera, &generator, &mut lod_manager, &mut ui_state.inventory) {
                                    block_sound(&mut audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                                    drop_broken_block(&mut dropped_items, &camera, pos, block, previous);
                                    terrain_edited = true;
                                    if let Some(client) = net.as_mut() {
                                        client.send_block_change(pos, block);
                                    }
                                }
                            }
                        } else {
                            block_interaction.reset();
                        }

                        // Partie en ligne : position envoyée, blocs des autres appliqués, autres joueurs affichés
                        if let Some(client) = net.as_mut() {
                            herocraft_common::prof_scope!("reseau");
                            client.send_move(&camera);
                            let mut lost = None;
                            for event in client.poll() {
                                match event {
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
                                        let hurt = ui_state.player.bitten(damage, &cause);
                                        show_wound(hurt, &cause, &mut camera, &mut ui_state, &mut audio);
                                    }
                                    NetEvent::Loot { position, items } => drop_loot(&mut dropped_items, &camera, position, &items),
                                    NetEvent::Chat { sender, message } if sender.is_empty() => ui_state.chat.server(&message),
                                    NetEvent::Chat { sender, message } => ui_state.chat.player(&sender, &message),
                                    NetEvent::Clock { time, forced } => {
                                        state.simulated_time = time as f32;
                                        client_weather.model.forced = forced;
                                    }
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
                                        creatures_seed = current_seed;
                                    }
                                    if !ui_state.is_settings_open && creature_mode != benchmark::CreatureMode::Off {
                                        // L'heure du ciel règle le sommeil et les apparitions
                                        solo_creatures.set_sun_dir(state.sun_dir());
                                        // Sous l'orage les bêtes s'abritent, l'hiver les ours hibernent
                                        solo_creatures.set_climate(&client_weather.model, state.simulated_time as f64);
                                        let player = herocraft_world::PlayerPresence { feet: camera.player_pos, crouching: camera.is_crouching() };
                                        // Morsures et charges des bêtes agressives
                                        let hits = solo_creatures.tick_with_presences(dt, &generator, lod_manager.terrain_edits(), &[player]);
                                        // Banc d'essai : les bêtes ne blessent pas le joueur
                                        for hit in hits.into_iter().filter(|_| bench.is_none()) {
                                            let cause = creature_registry.get(hit.species).map_or("Une créature", |s| s.name.as_str());
                                            let hurt = ui_state.player.bitten(hit.damage, cause);
                                            show_wound(hurt, cause, &mut camera, &mut ui_state, &mut audio);
                                        }
                                    }
                                    match creature_mode {
                                        benchmark::CreatureMode::Full | benchmark::CreatureMode::Stress => solo_creatures
                                            .creatures()
                                            .iter()
                                            .map(|c| creatures::CreatureView::new(Some(c.species), &c.state()))
                                            .collect(),
                                        benchmark::CreatureMode::SimulationOnly | benchmark::CreatureMode::Off => Vec::new(),
                                    }
                                }
                            };
                            probe.section("créatures : simulation", section);
                            let section = Instant::now();
                            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                            let anim_dt = if ui_state.is_settings_open { 0.0 } else { dt };
                            creature_animator.update(&creature_views, &creature_registry, &creature_models, anim_dt, block);
                            let mut frame = creatures::build_frame(&creature_views, &creature_registry, &creature_models, &creature_animator, block);
                            // Le héros, vu de dehors à la troisième personne
                            hero_gait.update(camera.player_pos, anim_dt, block);
                            if let Some(legend) = ui_state.legend.as_ref().filter(|_| camera.view_mode() == camera::CameraViewMode::ThirdPerson) {
                                state.set_hero_sheet(&legend.appearance);
                                let figure = hero_figure.get(&legend.appearance);
                                hero_model::push_hero(&mut frame, &legend.appearance, figure, camera.player_pos, camera.player_yaw, hero_gait.swing(), block);
                            }
                            state.set_creature_frame(&frame);
                            creature_vertices = frame.vertices.len();
                            probe.section("créatures : animation et maillage", section);
                        }

                        // Connexion menée en tâche de fond : à son terme, la planète du serveur
                        // remplace la partie solo
                        if let Some(result) = pending_join.as_ref().and_then(|p| p.poll()) {
                            let addr = pending_join.take().map(|p| p.addr).unwrap_or_default();
                            match result {
                                Ok(world) => {
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
                            if let Some(mesh) = dropped_items.take_mesh(h, game_start.elapsed().as_secs_f32()) {
                                state.set_dropped_items(&mesh);
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
                                save_solo_creatures(&solo_creatures);
                                save_sky_clock(current_seed, &state, &client_weather);
                            }
                            ui_state.inventory.save();
                            last_autosave = Instant::now();
                        }

                        // Mise à jour adaptative du QuadTree LOD
                        herocraft_common::prof_scope!("frame::lod_and_upload" => lod_guard);
                        let section = Instant::now();
                        if lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, dt, None)
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
                                Ok(rank) => info!("Compétence {id} au rang {rank}."),
                                Err(e) => warn!("Compétence {id} : {}.", e.message()),
                            }
                            match net.as_mut() {
                                Some(client) => client.send_learn_skill(id),
                                None => ui_state.player.save(),
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

                        // 2. Traitement des actions déclenchées par les boutons egui et le clavier
                        pending_actions.extend(maybe_action);
                        for action in pending_actions.drain(..) {
                            herocraft_common::prof_scope!("frame::ui_action");
                            match action {
                                UiAction::PreviewSound => audio.preview(&ui_state.audio_settings),
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
                                    camera.spawn_on_ground(&generator);
                                    audio.reset_player();
                                    info!("Réapparition au point d'apparition.");
                                }
                                UiAction::ResumeGame => {
                                    ui_state.set_settings_open(false);
                                    info!("Bouton cliqué : Reprise de la partie.");
                                }
                                UiAction::QuitGame => {
                                    if net.is_none() && hosted.is_none() {
                                        save_solo_creatures(&solo_creatures);
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
                                    info!("Paramètre Distance LOD mis à jour : {:?}", ui_state.video_settings.lod_distance);
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
                                    save_solo_creatures(&solo_creatures);
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
                                    save_solo_creatures(&solo_creatures);
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
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    save_solo_creatures(&solo_creatures);
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
                                    lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
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
                            Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                            Err(e) => log::warn!("Erreur lors du rendu : {:?}", e),
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
                if cursor_captured {
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
    lod_manager.update(camera.eye_position(), generator, chunk_cache, 0.0, None);
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
        Action::OpenInventory if first_press && is_ground_view => {
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

/// Délai entre deux coups portés à une créature, en secondes
const ATTACK_COOLDOWN: f32 = 0.4;

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
/// fuite), sauf si le coup précédent est trop récent
fn hit_creature(
    id: u64,
    cooldown: &mut f32,
    camera: &OrbitCamera,
    net: Option<&mut NetClient>,
    solo: &mut herocraft_world::CreatureWorld,
    audio: &mut audio::Audio,
    settings: &audio::AudioSettings,
) {
    if *cooldown > 0.0 {
        return;
    }
    *cooldown = ATTACK_COOLDOWN;
    match net {
        Some(client) => client.send_hit_creature(id),
        None => {
            solo.hit(id, camera.player_pos);
        }
    }
    // Coup sourd
    audio.block_placed(audio::synth::Material::Dirt, 0.0, settings);
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

/// Enregistre la population de créatures du monde solo (bêtes tuées comprises, qui ne reviennent pas)
fn save_solo_creatures(creatures: &herocraft_world::CreatureWorld) {
    if let Err(e) = creatures.save() {
        log::warn!("Échec de l'enregistrement des créatures : {}", e);
    }
}

/// Écrit le rapport de profilage (no-op sans la feature cargo `profiling`)
fn write_profile_report() {
    if let Some(path) = herocraft_common::profiler::write_report() {
        info!("Rapport de profilage écrit : {}", path.display());
    }
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
        Command::Fly => {
            if game.camera.target_distance > game.camera.base_radius {
                return Err("Go back to the ground view first".into());
            }
            game.camera.toggle_flight();
            one(format!("Flight {}", if game.camera.is_flying { "on" } else { "off" }))
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
    }
}

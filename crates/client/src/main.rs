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
mod block_interaction;
mod block_outline;
mod camera;
mod cloud_mesh;
mod collision;
mod creature_render;
mod creatures;
mod gpu_timer;
mod input;
mod keyboard_hook;
mod lod;
mod monitor;
mod network;
mod planet_mesh;
mod player_stats;
mod post;
mod renderer;
mod shaders;
mod shadow;
mod skinned_model;
mod ui;
mod voxel_mesh;

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
    if herocraft_common::profiler::is_enabled() {
        info!("PROFILAGE ACTIF : rapport écrit dans ./profiling/ à la fermeture (ou avec [F9]).");
    }

    info!("==========================================================");
    info!("   HeroCraft — Moteur Planétaire QuadTree LOD (Rust / WGPU) ");
    info!("==========================================================");
    info!("Contrôles (modifiables dans Échap > Commandes) :");
    info!(" • Souris                             : Regarder autour (curseur capturé en jeu)");
    info!(" • Clic gauche / Clic droit           : Détruire le bloc visé / Poser un bloc de pierre");
    info!(" • [Z,Q,S,D] (AZERTY) / [W,A,S,D]     : Se déplacer");
    info!(" • [ESPACE] / double [ESPACE]         : Sauter / S'envoler (monter en vol)");
    info!(" • [MAJ] / [CTRL]                     : Descendre en vol / Sprinter");
    info!(" • [ALT] + Molette                    : Zoom FPS (0m) -> TPS (1-25m) -> Satellite");
    info!(" • [F] / [R]                          : Vue sol <-> satellite / Réinitialiser la vue");
    info!(" • [G] / [T] / [C]                    : Nouvelle planète / Filtres tectonique, courants");
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
    let mut current_seed = launch.seed.unwrap_or_else(herocraft_world::random_seed);
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
    // la planète où elles vivent. Les espèces sont décrites dans assets/creatures/.
    let creature_registry = Arc::new(herocraft_common::CreatureRegistry::load_default());
    let mut solo_creatures = herocraft_world::CreatureWorld::new(base_radius as f64, current_seed, creature_registry.clone());
    let mut creatures_seed = current_seed;
    let mut creature_animator = creatures::CreatureAnimator::default();
    let (creature_models, creature_textures) = creatures::CreatureModels::load(&creature_registry);

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
    state.set_creature_textures(&creature_textures);
    state.sync_lod_meshes(&lod_manager);

    // Gestion des entrées : souris capturée en jeu, glisser en vue satellite
    let mut cursor_captured = false;
    keyboard_hook::install(&window);
    let mut window_focused = true;
    let mut modifiers = ModifiersState::empty();
    let mut zoom_modifier_held = false;
    let mut jump_tap = input::DoubleTap::default();
    // Actions du clavier rejouées comme les boutons du HUD, au moment du rendu
    let mut pending_actions: Vec<UiAction> = Vec::new();
    let mut is_dragging = false;
    // Destruction (clic gauche) et pose (clic droit) du bloc visé par le réticule
    let mut block_interaction = BlockInteraction::new();
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
                        if let Some(client) = net.as_mut() {
                            client.disconnect();
                        }
                        // Le serveur ouvert ici prévient ses joueurs et enregistre le monde
                        hosted = None;
                        let _ = chunk_cache.flush_dirty();
                        save_terrain_edits(&lod_manager);
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
                            // Appliqué et remaillé sur-le-champ : le bloc change dans la frame qui suit le clic
                            let pressed = *btn_state == ElementState::Pressed;
                            if let Some(action) = block_interaction.on_mouse_button(*button, pressed) {
                                block_interaction.update_target(&camera, &generator, &lod_manager.terrain_edits().clone());
                                if let Some((pos, block)) = block_interaction.apply(action, &camera, &generator, &mut lod_manager) {
                                    state.sync_lod_meshes(&lod_manager);
                                    if let Some(client) = net.as_mut() {
                                        client.send_block_change(pos, block);
                                    }
                                }
                            }
                        }
                        if !cursor_captured && matches!(button, MouseButton::Left | MouseButton::Right) {
                            let is_over_ui = ui_state.is_settings_open
                                || ui_state.ctx.is_pointer_over_area()
                                || ui_state.ctx.wants_pointer_input();
                            is_dragging = *btn_state == ElementState::Pressed && !is_over_ui;
                        }
                    }

                    // Déplacement curseur visible (survol des boutons UI et rotation de la planète)
                    WindowEvent::CursorMoved { position, .. } => {
                        if is_dragging && !cursor_captured && !ui_state.is_settings_open {
                            let dx = (position.x - last_mouse_pos.0) as f32;
                            let dy = (position.y - last_mouse_pos.1) as f32;
                            camera.orbit(dx, dy);
                        }
                        last_mouse_pos = (position.x, position.y);
                    }

                    // Molette : zoom continu FPS <-> TPS <-> Orbite avec la touche de zoom (Alt) maintenue.
                    // En vue satellite, la molette seule zoome aussi.
                    WindowEvent::MouseWheel { delta, .. } => {
                        let wants_zoom = if cursor_captured {
                            zoom_modifier_held
                        } else {
                            !(ui_state.is_settings_open || ui_state.ctx.is_pointer_over_area() || ui_state.ctx.wants_pointer_input())
                        };

                        if wants_zoom {
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

                        if key_code == KeyCode::Escape && is_pressed {
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
                        } else if !ui_state.is_settings_open {
                            // En jeu normal (menu fermé) : déplacements et raccourcis selon les touches attribuées
                            let Some(action) = ui_state.controls.action_for(key_code) else { return };
                            game_key(action, is_pressed, is_pressed && !*repeat, &mut camera, &mut ui_state, &mut pending_actions, &mut jump_tap, &mut zoom_modifier_held);
                        }
                    }

                    WindowEvent::RedrawRequested => {
                        herocraft_common::prof_scope!("frame" => frame_guard);
                        let now = Instant::now();
                        let dt = now.duration_since(last_frame_time).as_secs_f32();
                        last_frame_time = now;

                        // Mesure et mise à jour du compteur FPS (rafraîchi toutes les 250 ms pour une lecture stable)
                        fps_frame_count += 1;
                        let fps_elapsed = now.duration_since(fps_timer).as_secs_f32();
                        if fps_elapsed >= 0.25 {
                            let current_fps = (fps_frame_count as f32 / fps_elapsed).round() as u32;
                            ui_state.set_fps(current_fps);
                            fps_frame_count = 0;
                            fps_timer = now;
                        }

                        // Touches prises aux autres programmes par le crochet clavier depuis l'image précédente
                        for hooked in keyboard_hook::drain() {
                            if let (false, Some(action)) = (ui_state.is_settings_open, ui_state.controls.action_for(hooked.key)) {
                                let first_press = hooked.pressed && !hooked.repeat;
                                game_key(action, hooked.pressed, first_press, &mut camera, &mut ui_state, &mut pending_actions, &mut jump_tap, &mut zoom_modifier_held);
                            }
                        }

                        // Souris capturée en jeu (au sol ou en 3e personne) ; le menu, la vue satellite
                        // et la perte du focus la libèrent
                        let want_capture = window_focused
                            && !ui_state.is_settings_open
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
                        if !ui_state.is_settings_open {
                            herocraft_common::prof_scope!("frame::camera_update");
                            camera.update(dt, &generator);
                            // Jauges du joueur, chute terminée : dommages (signalés au serveur en
                            // ligne) et, si elle tue, retour au point d'apparition
                            ui_state.player.tick(dt, &mut camera);
                            if let Some(blocks) = camera.take_landing() {
                                if let Some(client) = net.as_mut() {
                                    client.send_fall(blocks);
                                }
                                if ui_state.player.fall(blocks) {
                                    info!("Mort après une chute de {:.0} blocs : retour au point d'apparition.", blocks);
                                    camera.spawn_on_ground(&generator);
                                }
                            }
                        }
                        state.update_camera(&camera, &ui_state.lighting_settings, &ui_state.cloud_settings, ui_state.video_settings.flattening);
                        state.update_post(&ui_state.post_settings);
                        state.update_shadows(&camera, &ui_state.shadow_settings);
                        state.update_atmosphere(&camera, &ui_state.atmosphere_settings, &ui_state.lighting_settings);

                        // Bloc visé par le réticule (souris capturée seulement) et bouton tenu enfoncé
                        let mut terrain_edited = false;
                        if cursor_captured {
                            let edits = lod_manager.terrain_edits().clone();
                            block_interaction.update_target(&camera, &generator, &edits);
                            if let Some(action) = block_interaction.tick(dt) {
                                if let Some((pos, block)) = block_interaction.apply(action, &camera, &generator, &mut lod_manager) {
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
                                        if lod_manager.set_block(&generator, pos, block) {
                                            terrain_edited = true;
                                            block_interaction.invalidate();
                                        }
                                    }
                                    NetEvent::Stats(stats) => ui_state.player.sync(stats, &mut camera),
                                    NetEvent::Disconnected(reason) => lost = Some(reason),
                                }
                            }
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
                            let views: Vec<creatures::CreatureView> = match net.as_ref() {
                                Some(client) => client.creature_views(&creature_registry),
                                None => {
                                    if creatures_seed != current_seed {
                                        solo_creatures = herocraft_world::CreatureWorld::new(camera.base_radius as f64, current_seed, creature_registry.clone());
                                        creatures_seed = current_seed;
                                    }
                                    if !ui_state.is_settings_open {
                                        solo_creatures.tick(dt, &generator, lod_manager.terrain_edits(), &[camera.player_pos]);
                                    }
                                    solo_creatures
                                        .creatures()
                                        .iter()
                                        .map(|c| creatures::CreatureView { id: c.id, species: Some(c.species), feet: c.position, yaw: c.yaw })
                                        .collect()
                                }
                            };
                            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                            let anim_dt = if ui_state.is_settings_open { 0.0 } else { dt };
                            creature_animator.update(&views, &creature_registry, &creature_models, anim_dt, block);
                            let frame = creatures::build_frame(&views, &creature_registry, &creature_models, &creature_animator, block);
                            state.set_creature_frame(&frame);
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
                                        window.set_title(&format!("HeroCraft — partie ouverte au réseau local (port {})", host.port));
                                    } else {
                                        current_seed = world.seed;
                                        switch_planet(current_seed, edits, None, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state);
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
                                    net = Some(world.client);
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
                        {
                            herocraft_common::prof_scope!("blocs::contour");
                            state.set_block_outline(block_interaction.outline(&camera));
                        }
                        if last_autosave.elapsed().as_secs() >= 30 {
                            save_terrain_edits(&lod_manager);
                            last_autosave = Instant::now();
                        }

                        // Mise à jour adaptative du QuadTree LOD
                        herocraft_common::prof_scope!("frame::lod_and_upload" => lod_guard);
                        if lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, dt, None)
                            || terrain_edited
                            || state.has_pending_lod_uploads()
                        {
                            state.sync_lod_meshes(&lod_manager);
                        }
                        drop(lod_guard);

                        // 1. Mise à jour de l'interface egui
                        let is_ground = camera.distance <= camera.base_radius;
                        let (full_output, maybe_action) = ui_state.update_ui(camera.distance, is_ground);

                        // 2. Traitement des actions déclenchées par les boutons egui et le clavier
                        pending_actions.extend(maybe_action);
                        for action in pending_actions.drain(..) {
                            herocraft_common::prof_scope!("frame::ui_action");
                            match action {
                                UiAction::ResumeGame => {
                                    ui_state.set_settings_open(false);
                                    info!("Bouton cliqué : Reprise de la partie.");
                                }
                                UiAction::QuitGame => {
                                    if let Some(client) = net.as_mut() {
                                        client.disconnect();
                                    }
                                    hosted = None;
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
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
                                    if was_hosting && current_seed == solo_seed {
                                        // Même planète : le joueur reste où il est, avec les blocs des autres
                                        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
                                        camera.set_terrain(edits.clone());
                                        lod_manager.set_terrain_edits(edits);
                                    } else {
                                        current_seed = solo_seed;
                                        switch_planet(current_seed, edits, storage, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state);
                                    }
                                    block_interaction.invalidate();
                                    ui_state.multiplayer.status = NetStatus::Solo;
                                    window.set_title(WINDOW_TITLE);
                                    info!("Retour à la partie solo (planète #{}).", current_seed);
                                }
                                UiAction::GenerateNewPlanet => {
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    current_seed = herocraft_world::random_seed();
                                    let (storage, edits) = solo_terrain(current_seed);
                                    switch_planet(current_seed, edits, storage, &mut generator, &mut chunk_cache, &mut lod_manager, &mut camera, &mut state, &mut ui_state);
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

/// Dossier de sauvegarde d'un monde solo (aussi celui du serveur qui l'ouvre au réseau local)
fn solo_save_dir(seed: u64) -> String {
    format!("saves/monde_{}", seed)
}

/// Stockage d'un monde solo et ses blocs modifiés, repris de la sauvegarde
fn solo_terrain(seed: u64) -> (Option<herocraft_voxel_engine::ChunkStorage>, Arc<herocraft_world::TerrainEdits>) {
    let storage = herocraft_voxel_engine::ChunkStorage::new(solo_save_dir(seed)).ok();
    let edits = Arc::new(herocraft_world::TerrainEdits::new(storage.clone()));
    (storage, edits)
}

/// Remplace la planète affichée (nouvelle graine, partie en ligne ou retour au solo) : générateur,
/// blocs modifiés, caches, maillages et nuages, puis pose le joueur au sol
#[allow(clippy::too_many_arguments)]
fn switch_planet(
    seed: u64,
    edits: Arc<herocraft_world::TerrainEdits>,
    storage: Option<herocraft_voxel_engine::ChunkStorage>,
    generator: &mut herocraft_world::PlanetWorldGenerator,
    chunk_cache: &mut herocraft_voxel_engine::WorldChunkCache,
    lod_manager: &mut PlanetLodManager,
    camera: &mut OrbitCamera,
    state: &mut State,
    ui_state: &mut UiState,
) {
    let desc = herocraft_world::PlanetDescriptor { seed, ..Default::default() };
    *generator = herocraft_world::PlanetWorldGenerator::new(desc);
    *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
    lod_manager.clear(seed);
    camera.set_terrain(edits.clone());
    lod_manager.set_terrain_edits(edits);
    camera.spawn_on_ground(generator);
    lod_manager.update(camera.eye_position(), generator, chunk_cache, 0.0, None);
    state.sync_lod_meshes(lod_manager);
    state.update_cloud_mesh(camera.base_radius, seed);
    ui_state.set_seed(seed);
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

/// Enregistre les chunks modifiés par le joueur depuis le dernier enregistrement
fn save_terrain_edits(lod: &PlanetLodManager) {
    match lod.terrain_edits().flush() {
        Ok(0) => {}
        Ok(n) => info!("{} chunks modifiés enregistrés.", n),
        Err(e) => log::warn!("Échec de l'enregistrement des blocs modifiés : {}", e),
    }
}

/// Écrit le rapport de profilage (no-op sans la feature cargo `profiling`)
fn write_profile_report() {
    if let Some(path) = herocraft_common::profiler::write_report() {
        info!("Rapport de profilage écrit : {}", path.display());
    }
}

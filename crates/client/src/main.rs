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
mod gpu_timer;
mod input;
mod lod;
mod monitor;
mod network;
mod planet_mesh;
mod post;
mod renderer;
mod shaders;
mod shadow;
mod ui;
mod voxel_mesh;

use block_interaction::BlockInteraction;
use camera::OrbitCamera;
use input::Action;
use lod::PlanetLodManager;
use network::NetEvent;
use renderer::State;
use ui::{UiAction, UiState};

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
        .with_title("HeroCraft — Planète QuadTree LOD (Rust / WGPU)")
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)
        .expect("Échec de création de la fenêtre Winit");
    if let Some(m) = wanted_monitor.as_deref().and_then(|name| monitor::find(&event_loop, name)) {
        window.set_outer_position(monitor::centered_position(&m, window.outer_size()));
        info!("Fenêtre ouverte sur l'écran {:?}.", m.name());
    }

    let window = Arc::new(window);

    // Partie en ligne si un serveur est demandé : sa planète remplace celle du solo
    let mut joined = network::requested_server().and_then(|(addr, name)| {
        info!("Connexion au serveur {} en tant que « {} »...", addr, name);
        match network::join(&addr, &name) {
            Ok(world) => {
                window.set_title(&format!("HeroCraft — en ligne sur {} ({})", addr, name));
                Some(world)
            }
            Err(e) => {
                warn!("Partie en ligne impossible ({}) : lancement en solo.", e);
                None
            }
        }
    });

    // Initialisation du générateur procédural planétaire
    let mut current_seed = joined.as_ref().map_or(2026_u64, |world| world.seed);
    let mut planet_desc = herocraft_world::PlanetDescriptor::default();
    planet_desc.seed = current_seed;

    info!("Initialisation de la planète et du gestionnaire QuadTree...");
    let mut generator = herocraft_world::PlanetWorldGenerator::new(planet_desc);

    // Initialisation du stockage et du cache de chunks voxels. En ligne, le monde est enregistré
    // par le serveur : rien n'est écrit en local, les chunks modifiés viennent de lui.
    let terrain_edits;
    let mut chunk_cache;
    if let Some(world) = joined.as_mut() {
        terrain_edits = Arc::new(herocraft_world::TerrainEdits::from_chunks(std::mem::take(&mut world.edited_chunks)));
        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(None, 256);
    } else {
        let save_dir = format!("saves/monde_{}", current_seed);
        let storage = herocraft_voxel_engine::ChunkStorage::new(&save_dir).ok();
        // Blocs détruits ou posés par le joueur, enregistrés dans la même sauvegarde
        terrain_edits = Arc::new(herocraft_world::TerrainEdits::new(storage.clone()));
        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
    }

    // Rayon de base de la planète déterminé par le descripteur (REFERENCE_BASE_RADIUS * scale)
    let base_radius = generator.base_radius() as f32;

    // Initialisation de la caméra multi-échelle unifiée (Sol FPS <-> TPS <-> Satellite)
    let mut camera = OrbitCamera::new(base_radius * 6.6, base_radius, &generator);
    // De retour sur un serveur : là où le joueur l'avait quitté
    if let Some(spawn) = joined.as_ref().and_then(|world| world.spawn_position) {
        camera.player_pos = spawn;
        camera.sync_orbit_to_ground();
    }
    let mut net = joined.map(|world| world.client);
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

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
    state.sync_lod_meshes(&lod_manager);

    // Gestion des entrées : souris capturée en jeu, glisser en vue satellite
    let mut cursor_captured = false;
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
                            let is_ground_view = camera.target_distance <= camera.base_radius;
                            let first_press = is_pressed && !*repeat;
                            match action {
                                Action::MoveForward => camera.move_forward = is_pressed,
                                Action::MoveBackward => camera.move_backward = is_pressed,
                                Action::MoveLeft => camera.move_left = is_pressed,
                                Action::MoveRight => camera.move_right = is_pressed,
                                Action::Sprint => camera.is_sprinting = is_pressed,
                                Action::Descend => camera.descend_held = is_pressed,
                                Action::ZoomModifier => zoom_modifier_held = is_pressed,
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

                        // Souris capturée en jeu (au sol ou en 3e personne) ; le menu, la vue satellite
                        // et la perte du focus la libèrent
                        let want_capture = window_focused
                            && !ui_state.is_settings_open
                            && camera.target_distance <= camera.base_radius;
                        if want_capture != cursor_captured {
                            cursor_captured = want_capture;
                            set_cursor_captured(&window, want_capture);
                            ui_state.set_pointer_captured(want_capture);
                            is_dragging = false;
                        }

                        // Mise à jour de la caméra uniquement si le menu n'est pas ouvert
                        if !ui_state.is_settings_open {
                            herocraft_common::prof_scope!("frame::camera_update");
                            camera.update(dt, &generator);
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
                            for event in client.poll(dt) {
                                match event {
                                    NetEvent::BlockChanged(pos, block) => {
                                        if lod_manager.set_block(&generator, pos, block) {
                                            terrain_edited = true;
                                            block_interaction.invalidate();
                                        }
                                    }
                                    NetEvent::Disconnected(reason) => lost = Some(reason),
                                }
                            }
                            if let Some(reason) = lost {
                                warn!("Déconnecté du serveur ({}) : la partie continue hors ligne, sans sauvegarde.", reason);
                                window.set_title("HeroCraft — hors ligne (déconnecté du serveur)");
                                net = None;
                                state.set_player_outlines(&[]);
                                ui_state.name_tags.clear();
                            } else {
                                state.set_player_outlines(&client.remote_player_boxes(camera.base_radius));
                                ui_state.name_tags = client.name_tags(&camera, state.size.width as f32, state.size.height as f32);
                            }
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
                                UiAction::GenerateNewPlanet if net.is_some() => {
                                    info!("Nouvelle planète impossible en ligne : la planète est celle du serveur.");
                                }
                                UiAction::GenerateNewPlanet => {
                                    let _ = chunk_cache.flush_dirty();
                                    save_terrain_edits(&lod_manager);
                                    current_seed = current_seed.wrapping_add(1337);
                                    let mut new_desc = herocraft_world::PlanetDescriptor::default();
                                    new_desc.seed = current_seed;
                                    generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                                    let new_save_dir = format!("saves/monde_{}", current_seed);
                                    let new_storage = herocraft_voxel_engine::ChunkStorage::new(&new_save_dir).ok();
                                    let new_edits = herocraft_world::TerrainEdits::new(new_storage.clone());
                                    chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(new_storage, 256);
                                    lod_manager.clear(current_seed);
                                    let new_edits = Arc::new(new_edits);
                                    camera.set_terrain(new_edits.clone());
                                    lod_manager.set_terrain_edits(new_edits);
                                    camera.spawn_on_ground(&generator);
                                    lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
                                    state.sync_lod_meshes(&lod_manager);
                                    state.update_cloud_mesh(camera.base_radius, current_seed);
                                    ui_state.set_seed(current_seed);
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

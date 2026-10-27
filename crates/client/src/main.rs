use std::sync::Arc;
use std::time::Instant;

use log::info;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

mod camera;
mod cloud_mesh;
mod gpu_timer;
mod lod;
mod planet_mesh;
mod post;
mod renderer;
mod shaders;
mod shadow;
mod ui;
mod voxel_mesh;

use camera::OrbitCamera;
use lod::PlanetLodManager;
use renderer::State;
use ui::{UiAction, UiState};

fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    herocraft_common::profiler::init_main_thread();
    if herocraft_common::profiler::is_enabled() {
        info!("PROFILAGE ACTIF : rapport écrit dans ./profiling/ à la fermeture (ou avec [F9]).");
    }

    info!("==========================================================");
    info!("   HeroCraft — Moteur Planétaire QuadTree LOD (Rust / WGPU) ");
    info!("==========================================================");
    info!("Contrôles :");
    info!(" • [Z,Q,S,D] / [W,A,S,D]            : Marcher au sol sur les blocs (gravité sphérique)");
    info!(" • [SHIFT]                            : Courir / Sprinter");
    info!(" • [ESPACE]                           : Sauter (au sol) / Auto-rotation (en orbite)");
    info!(" • [F] ou Bouton HUD                  : Basculer instantanément Vue Sol (FPS) <-> Satellite");
    info!(" • Molette de la souris (Scroll)      : Zoom continu FPS (0m) -> TPS (1-25m) -> Satellite");
    info!(" • Clic Gauche / Droit (Drag & Drop)   : Orienter le regard (sol) / Tourner la planète (orbite)");
    info!(" • [R] ou Bouton HUD                  : Réinitialiser la vue en orbite");
    info!(" • [G] ou Bouton HUD                  : Générer une nouvelle planète procédurale");
    info!(" • [T] / [C]                          : Filtres plaques tectoniques / courants marins");
    info!(" • [ÉCHAP]                            : Quitter");
    info!("==========================================================");

    let event_loop = EventLoop::new().expect("Échec d'initialisation de l'EventLoop Winit");
    let window = WindowBuilder::new()
        .with_title("HeroCraft — Planète QuadTree LOD (Rust / WGPU)")
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)
        .expect("Échec de création de la fenêtre Winit");

    let window = Arc::new(window);

    // Initialisation du générateur procédural planétaire
    let mut current_seed = 2026_u64;
    let mut planet_desc = herocraft_world::PlanetDescriptor::default();
    planet_desc.seed = current_seed;

    info!("Initialisation de la planète et du gestionnaire QuadTree...");
    let mut generator = herocraft_world::PlanetWorldGenerator::new(planet_desc);

    // Initialisation du stockage et du cache de chunks voxels
    let save_dir = format!("saves/monde_{}", current_seed);
    let storage = herocraft_voxel_engine::ChunkStorage::new(&save_dir).ok();
    let mut chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);

    // Rayon de base de la planète déterminé par le descripteur (REFERENCE_BASE_RADIUS * scale)
    let base_radius = generator.base_radius() as f32;

    // Initialisation de la caméra multi-échelle unifiée (Sol FPS <-> TPS <-> Satellite)
    let mut camera = OrbitCamera::new(base_radius * 6.6, base_radius, &generator);
    let size = window.inner_size();
    camera.aspect = size.width as f32 / size.height.max(1) as f32;

    // Initialisation du gestionnaire de LOD QuadTree avec transitions douces hybrides
    let mut lod_manager = PlanetLodManager::new(base_radius, current_seed);
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

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
    state.sync_lod_meshes(&lod_manager);

    // Gestion des entrées souris
    let mut is_dragging = false;
    let mut last_mouse_pos = (0.0_f64, 0.0_f64);
    let mut last_frame_time = Instant::now();
    let mut fps_timer = Instant::now();
    let mut fps_frame_count = 0u32;

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, ref event } if window_id == window.id() => {
                // Transmettre les événements de fenêtre à egui
                ui_state.handle_event(&window, event);

                match event {
                    WindowEvent::CloseRequested => {
                        let _ = chunk_cache.flush_dirty();
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

                    // Clic souris (Bouton UI ou Drag & Drop planète)
                    WindowEvent::MouseInput { state: btn_state, button, .. } => {
                        let is_over_ui = ui_state.is_settings_open
                            || ui_state.ctx.is_pointer_over_area()
                            || ui_state.ctx.wants_pointer_input();

                        if *button == MouseButton::Left {
                            if *btn_state == ElementState::Pressed {
                                if !is_over_ui {
                                    is_dragging = true;
                                }
                            } else {
                                is_dragging = false;
                            }
                        } else if *button == MouseButton::Right {
                            is_dragging = *btn_state == ElementState::Pressed && !is_over_ui;
                        }
                    }

                    // Déplacement curseur (Survol des boutons UI et rotation de la vue)
                    WindowEvent::CursorMoved { position, .. } => {
                        let is_over_ui = ui_state.is_settings_open
                            || ui_state.ctx.is_pointer_over_area()
                            || ui_state.ctx.wants_pointer_input();

                        if is_dragging && !is_over_ui {
                            let dx = (position.x - last_mouse_pos.0) as f32;
                            let dy = (position.y - last_mouse_pos.1) as f32;
                            camera.orbit(dx, dy);
                        }
                        last_mouse_pos = (position.x, position.y);
                    }

                    // Molette de souris (Dézoom / Zoom continu FPS <-> TPS <-> Orbite)
                    WindowEvent::MouseWheel { delta, .. } => {
                        let is_over_ui = ui_state.is_settings_open
                            || ui_state.ctx.is_pointer_over_area()
                            || ui_state.ctx.wants_pointer_input();

                        if !is_over_ui {
                            let zoom_delta = match delta {
                                MouseScrollDelta::LineDelta(_x, y) => *y,
                                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.04,
                            };
                            camera.zoom(zoom_delta);
                        }
                    }

                    // Touches clavier (Déplacements ZQSD/WASD, Saut, Sprint, Mode caméra)
                    WindowEvent::KeyboardInput {
                        event: KeyEvent {
                            state: key_state,
                            physical_key: PhysicalKey::Code(key_code),
                            ..
                        },
                        ..
                    } => {
                        let is_pressed = *key_state == ElementState::Pressed;

                        if *key_code == KeyCode::Escape && is_pressed {
                            // Touche Échap : Ouvre ou ferme le Menu des Paramètres Vidéo
                            let new_open = ui_state.toggle_settings();

                            // Réinitialise les touches de déplacement pour éviter tout glissement fantôme
                            camera.move_forward = false;
                            camera.move_backward = false;
                            camera.move_left = false;
                            camera.move_right = false;
                            camera.is_sprinting = false;

                            info!("Touche [Échap] : Menu Paramètres {}", if new_open { "OUVERT" } else { "FERMÉ" });
                        } else if !ui_state.is_settings_open {
                            // En jeu normal (menu fermé) : traitement des commandes de déplacement et raccourcis
                            match key_code {
                                // Déplacements au sol (Support ZQSD et WASD)
                                KeyCode::KeyW | KeyCode::KeyZ => {
                                    camera.move_forward = is_pressed;
                                }
                                KeyCode::KeyS => {
                                    camera.move_backward = is_pressed;
                                }
                                KeyCode::KeyA | KeyCode::KeyQ => {
                                    camera.move_left = is_pressed;
                                }
                                KeyCode::KeyD => {
                                    camera.move_right = is_pressed;
                                }
                                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                                    camera.is_sprinting = is_pressed;
                                }

                                // Actions ponctuelles déclenchées à l'appui
                                _ if is_pressed => match key_code {
                                    KeyCode::Space => {
                                        if camera.distance <= camera.base_radius {
                                            camera.jump_requested = true;
                                        } else {
                                            camera.auto_rotate = !camera.auto_rotate;
                                            ui_state.set_auto_rotate(camera.auto_rotate);
                                            info!("Auto-rotation de la planète : {}", if camera.auto_rotate { "ACTIVÉE" } else { "DÉSACTIVÉE" });
                                        }
                                    }
                                    KeyCode::KeyF => {
                                        camera.toggle_view(&generator);
                                        info!(
                                            "Raccourci [F] : Transition vers {}",
                                            if camera.target_distance <= camera.base_radius { "Vue Première Personne (FPS)" } else { "Vue Satellite (Orbite)" }
                                        );
                                    }
                                    KeyCode::F9 => {
                                        write_profile_report();
                                    }
                                    KeyCode::KeyR => {
                                        camera.reset();
                                        info!("Vue de la caméra réinitialisée.");
                                    }
                                    KeyCode::KeyG => {
                                        let _ = chunk_cache.flush_dirty();
                                        current_seed = current_seed.wrapping_add(1337);
                                        let mut new_desc = herocraft_world::PlanetDescriptor::default();
                                        new_desc.seed = current_seed;
                                        generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                                        let new_save_dir = format!("saves/monde_{}", current_seed);
                                        let new_storage = herocraft_voxel_engine::ChunkStorage::new(&new_save_dir).ok();
                                        chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(new_storage, 256);
                                        lod_manager.clear(current_seed);
                                        camera.spawn_on_ground(&generator);
                                        lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
                                        state.sync_lod_meshes(&lod_manager);
                                        state.update_cloud_mesh(camera.base_radius, current_seed);
                                        ui_state.set_seed(current_seed);
                                        info!("Raccourci [G] : Nouvelle planète générée (Graine: #{})", current_seed);
                                    }
                                    KeyCode::KeyT => {
                                        ui_state.show_tectonics = !ui_state.show_tectonics;
                                        if ui_state.show_tectonics {
                                            ui_state.show_ocean_currents = false;
                                        }
                                        let filter = ui_state.active_filter();
                                        lod_manager.set_filter(filter);
                                        lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
                                        state.sync_lod_meshes(&lod_manager);
                                        info!("Raccourci [T] : Filtre plaques tectoniques = {}", ui_state.show_tectonics);
                                    }
                                    KeyCode::KeyC => {
                                        ui_state.show_ocean_currents = !ui_state.show_ocean_currents;
                                        if ui_state.show_ocean_currents {
                                            ui_state.show_tectonics = false;
                                        }
                                        let filter = ui_state.active_filter();
                                        lod_manager.set_filter(filter);
                                        lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
                                        state.sync_lod_meshes(&lod_manager);
                                        info!("Raccourci [C] : Filtre courants marins = {}", ui_state.show_ocean_currents);
                                    }
                                    _ => {}
                                },
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

                        // Mise à jour de la caméra uniquement si le menu n'est pas ouvert
                        if !ui_state.is_settings_open {
                            herocraft_common::prof_scope!("frame::camera_update");
                            camera.update(dt, &generator);
                        }
                        state.update_camera(&camera, &ui_state.lighting_settings, &ui_state.cloud_settings);
                        state.update_post(&ui_state.post_settings);
                        state.update_shadows(&camera, &ui_state.shadow_settings);

                        // Mise à jour adaptative du QuadTree LOD
                        herocraft_common::prof_scope!("frame::lod_and_upload" => lod_guard);
                        if lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, dt, None)
                            || state.has_pending_lod_uploads()
                        {
                            state.sync_lod_meshes(&lod_manager);
                        }
                        drop(lod_guard);

                        // 1. Mise à jour de l'interface egui
                        let is_ground = camera.distance <= camera.base_radius;
                        let (full_output, maybe_action) = ui_state.update_ui(camera.distance, is_ground);

                        // 2. Traitement des actions déclenchées par les boutons egui
                        if let Some(action) = maybe_action {
                            herocraft_common::prof_scope!("frame::ui_action");
                            match action {
                                UiAction::ResumeGame => {
                                    ui_state.set_settings_open(false);
                                    info!("Bouton cliqué : Reprise de la partie.");
                                }
                                UiAction::QuitGame => {
                                    let _ = chunk_cache.flush_dirty();
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
                                UiAction::CycleLodDistance => {
                                    lod_manager.set_distance_preset(ui_state.video_settings.lod_distance);
                                    info!("Paramètre Distance LOD mis à jour : {:?}", ui_state.video_settings.lod_distance);
                                }
                                UiAction::CycleFpsLimit => {
                                    info!("Paramètre Limite FPS mis à jour : {:?}", ui_state.video_settings.fps_limit);
                                }
                                UiAction::GenerateNewPlanet => {
                                    let _ = chunk_cache.flush_dirty();
                                    current_seed = current_seed.wrapping_add(1337);
                                    let mut new_desc = herocraft_world::PlanetDescriptor::default();
                                    new_desc.seed = current_seed;
                                    generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                                    let new_save_dir = format!("saves/monde_{}", current_seed);
                                    let new_storage = herocraft_voxel_engine::ChunkStorage::new(&new_save_dir).ok();
                                    chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(new_storage, 256);
                                    lod_manager.clear(current_seed);
                                    camera.spawn_on_ground(&generator);
                                    lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache, 0.0, None);
                                    state.sync_lod_meshes(&lod_manager);
                                    state.update_cloud_mesh(camera.base_radius, current_seed);
                                    ui_state.set_seed(current_seed);
                                    info!("Bouton cliqué : Nouvelle planète générée ! (Graine: #{})", current_seed);
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
                                    info!("Bouton cliqué : Auto-rotation = {}", camera.auto_rotate);
                                }
                                UiAction::ResetCamera => {
                                    camera.reset();
                                    info!("Bouton cliqué : Vue réinitialisée.");
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

            Event::AboutToWait => {
                window.request_redraw();
            }

            _ => {}
        }
    }).expect("Erreur dans la boucle d'événements");
}

/// Écrit le rapport de profilage (no-op sans la feature cargo `profiling`)
fn write_profile_report() {
    if let Some(path) = herocraft_common::profiler::write_report() {
        info!("Rapport de profilage écrit : {}", path.display());
    }
}

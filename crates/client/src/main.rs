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
mod lod;
mod planet_mesh;
mod renderer;
mod ui;
mod voxel_mesh;

use camera::OrbitCamera;
use lod::PlanetLodManager;
use renderer::State;
use ui::{UiAction, UiState};

fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    info!("==========================================================");
    info!("   HeroCraft — Moteur Planétaire QuadTree LOD (Rust / WGPU) ");
    info!("==========================================================");
    info!("Contrôles :");
    info!(" • [Bouton Écran]                   : Cliquer sur 'NOUVELLE PLANETE'");
    info!(" • Clic Gauche / Droit (Drag & Drop) : Tourner la planète");
    info!(" • Molette de la souris (Scroll)    : Zoomer / Dézoomer");
    info!(" • [ESPACE]                         : Activer/Désactiver l'auto-rotation");
    info!(" • [R]                              : Réinitialiser la caméra");
    info!(" • [Case Écran] / [T]        : Basculer le filtre des plaques tectoniques");
    info!(" • [Case Écran] / [C]        : Basculer le filtre des courants marins");
    info!(" • [ÉCHAP]                  : Quitter");
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

    // Initialisation de la caméra orbitale
    let mut camera = OrbitCamera::default();
    let size = window.inner_size();
    camera.aspect = size.width as f32 / size.height.max(1) as f32;

    // Initialisation du gestionnaire de LOD QuadTree
    let mut lod_manager = PlanetLodManager::new(25.0);
    lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
    info!(
        "QuadTree initialisé : {} sommets, {} polygones ({} feuilles actives).",
        lod_manager.combined_mesh.vertices.len(),
        lod_manager.combined_mesh.indices.len() / 3,
        lod_manager.current_leaves.len()
    );

    // Initialisation de l'interface graphique (HUD avec boutons interactifs)
    let mut ui_state = UiState::new(current_seed);

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &lod_manager.combined_mesh, &ui_state));

    // Gestion des entrées souris
    let mut is_dragging = false;
    let mut last_mouse_pos = (0.0_f64, 0.0_f64);
    let mut last_frame_time = Instant::now();

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, ref event } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => {
                        let _ = chunk_cache.flush_dirty();
                        target.exit();
                    }

                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                        camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
                    }

                    WindowEvent::ScaleFactorChanged { .. } => {
                        let physical_size = window.inner_size();
                        state.resize(physical_size);
                        camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
                    }

                    // Clic souris (Bouton UI ou Drag & Drop planète)
                    WindowEvent::MouseInput { state: btn_state, button, .. } => {
                        if *button == MouseButton::Left {
                            if *btn_state == ElementState::Pressed {
                                let mx = last_mouse_pos.0 as f32;
                                let my = last_mouse_pos.1 as f32;

                                if let Some(action) = ui_state.on_mouse_down(mx, my) {
                                    // Clic sur l'interface utilisateur
                                    is_dragging = false;
                                    match action {
                                        UiAction::GenerateNewPlanet => {
                                            let _ = chunk_cache.flush_dirty();
                                            current_seed = current_seed.wrapping_add(1337);
                                            let mut new_desc = herocraft_world::PlanetDescriptor::default();
                                            new_desc.seed = current_seed;
                                            generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                                            let new_save_dir = format!("saves/monde_{}", current_seed);
                                            let new_storage = herocraft_voxel_engine::ChunkStorage::new(&new_save_dir).ok();
                                            chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(new_storage, 256);
                                            lod_manager.clear();
                                            lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
                                            state.update_mesh(&lod_manager.combined_mesh);
                                            ui_state.set_seed(current_seed);
                                            info!("Bouton cliqué : Nouvelle planète générée ! (Graine: #{})", current_seed);
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
                                            lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
                                            state.update_mesh(&lod_manager.combined_mesh);
                                            info!("Filtre de visualisation sélectionné : {:?}", filter);
                                        }
                                    }
                                } else {
                                    // Clic dans la scène 3D pour faire tourner la planète
                                    is_dragging = true;
                                }
                            } else {
                                ui_state.on_mouse_up();
                                is_dragging = false;
                            }
                        } else if *button == MouseButton::Right {
                            is_dragging = *btn_state == ElementState::Pressed;
                        }
                    }

                    // Déplacement curseur (Survol des boutons UI et rotation de la planète)
                    WindowEvent::CursorMoved { position, .. } => {
                        let is_over_ui = ui_state.on_cursor_moved(position.x as f32, position.y as f32);

                        if is_dragging && !is_over_ui {
                            let dx = (position.x - last_mouse_pos.0) as f32;
                            let dy = (position.y - last_mouse_pos.1) as f32;
                            camera.orbit(dx, dy);
                        }
                        last_mouse_pos = (position.x, position.y);
                    }

                    // Molette de souris (Dézoom / Zoom)
                    WindowEvent::MouseWheel { delta, .. } => {
                        let zoom_delta = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y,
                            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.04,
                        };
                        camera.zoom(zoom_delta);
                    }

                    // Touches clavier (G, Espace, R, Échap)
                    WindowEvent::KeyboardInput {
                        event: KeyEvent {
                            state: ElementState::Pressed,
                            physical_key: PhysicalKey::Code(key_code),
                            ..
                        },
                        ..
                    } => match key_code {
                        KeyCode::Escape => {
                            let _ = chunk_cache.flush_dirty();
                            target.exit();
                        }
                        KeyCode::Space => {
                            camera.auto_rotate = !camera.auto_rotate;
                            ui_state.set_auto_rotate(camera.auto_rotate);
                            info!("Auto-rotation de la planète : {}", if camera.auto_rotate { "ACTIVÉE" } else { "DÉSACTIVÉE" });
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
                            lod_manager.clear();
                            lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
                            state.update_mesh(&lod_manager.combined_mesh);
                            ui_state.set_seed(current_seed);
                            info!("Raccourci [G] : Nouvelle planète générée (Graine: #{})", current_seed);
                        }
                        KeyCode::KeyT => {
                            ui_state.show_tectonics = !ui_state.show_tectonics;
                            if ui_state.show_tectonics {
                                ui_state.show_ocean_currents = false;
                            }
                            ui_state.redraw();
                            let filter = ui_state.active_filter();
                            lod_manager.set_filter(filter);
                            lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
                            state.update_mesh(&lod_manager.combined_mesh);
                            info!("Raccourci [T] : Filtre plaques tectoniques = {}", ui_state.show_tectonics);
                        }
                        KeyCode::KeyC => {
                            ui_state.show_ocean_currents = !ui_state.show_ocean_currents;
                            if ui_state.show_ocean_currents {
                                ui_state.show_tectonics = false;
                            }
                            ui_state.redraw();
                            let filter = ui_state.active_filter();
                            lod_manager.set_filter(filter);
                            lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache);
                            state.update_mesh(&lod_manager.combined_mesh);
                            info!("Raccourci [C] : Filtre courants marins = {}", ui_state.show_ocean_currents);
                        }
                        _ => {}
                    },

                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let dt = now.duration_since(last_frame_time).as_secs_f32();
                        last_frame_time = now;

                        camera.update(dt);
                        state.update_camera(&camera);

                        // Mise à jour adaptative du QuadTree LOD (planète entière visible + voxels proches)
                        if lod_manager.update(camera.eye_position(), &generator, &mut chunk_cache) {
                            state.update_mesh(&lod_manager.combined_mesh);
                        }

                        // Mise à jour de l'affichage UI si l'état des boutons ou la seed a changé
                        if ui_state.dirty {
                            state.update_ui(&ui_state);
                            ui_state.dirty = false;
                        }

                        match state.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                            Err(e) => log::warn!("Erreur lors du rendu : {:?}", e),
                        }
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

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
mod planet_mesh;
mod renderer;
mod ui;
mod voxel_mesh;

use camera::OrbitCamera;
use glam::DVec3;
use herocraft_common::PlanetFace;
use planet_mesh::PlanetMesh;
use renderer::State;
use ui::{UiAction, UiState};
use voxel_mesh::VoxelMeshBuilder;

fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    info!("==========================================================");
    info!("   HeroCraft — Prototype Moteur & Rendu Planétaire (WGPU) ");
    info!("==========================================================");
    info!("Contrôles :");
    info!(" • [Bouton Écran]                   : Cliquer sur 'NOUVELLE PLANETE'");
    info!(" • Clic Gauche / Droit (Drag & Drop) : Tourner la planète");
    info!(" • Molette de la souris (Scroll)    : Zoomer / Dézoomer");
    info!(" • [ESPACE]                         : Activer/Désactiver l'auto-rotation");
    info!(" • [R]                              : Réinitialiser la caméra");
    info!(" • [G]                              : Générer une nouvelle graine (seed)");
    info!(" • [ÉCHAP]                          : Quitter");
    info!("==========================================================");

    let event_loop = EventLoop::new().expect("Échec d'initialisation de l'EventLoop Winit");
    let window = WindowBuilder::new()
        .with_title("HeroCraft — Prototype Planétaire Voxel (Rust / WGPU)")
        .with_inner_size(LogicalSize::new(1280, 720))
        .build(&event_loop)
        .expect("Échec de création de la fenêtre Winit");

    let window = Arc::new(window);

    // Initialisation du générateur procédural planétaire
    let mut current_seed = 2026_u64;
    let mut planet_desc = herocraft_world::PlanetDescriptor::default();
    planet_desc.seed = current_seed;

    info!("Génération de la planète (Cubed-Sphere avec 6 faces et biomes)...");
    let mut generator = herocraft_world::PlanetWorldGenerator::new(planet_desc);
    let mut planet_mesh = PlanetMesh::generate_planet(&generator, 25.0, 48);
    info!(
        "Planète générée : {} sommets, {} polygones.",
        planet_mesh.vertices.len(),
        planet_mesh.indices.len() / 3
    );

    // Initialisation de l'interface graphique (HUD avec bouton interactif)
    let mut ui_state = UiState::new(current_seed);

    // Initialisation du moteur WGPU
    let mut state = pollster::block_on(State::new(window.clone(), &planet_mesh, &ui_state));

    // Caméra orbitale interactive
    let mut camera = OrbitCamera::default();
    let size = window.inner_size();
    camera.aspect = size.width as f32 / size.height.max(1) as f32;

    // Gestion des entrées souris
    let mut is_dragging = false;
    let mut last_mouse_pos = (0.0_f64, 0.0_f64);
    let mut last_frame_time = Instant::now();

    // État du streaming voxel (LOD dynamique)
    let mut is_voxel_mode = false;
    let mut current_voxel_chunk: Option<(PlanetFace, i32, i32)> = None;

    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, ref event } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => target.exit(),

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
                                            current_seed = current_seed.wrapping_add(1337);
                                            let mut new_desc = herocraft_world::PlanetDescriptor::default();
                                            new_desc.seed = current_seed;
                                            generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                                            planet_mesh = PlanetMesh::generate_planet(&generator, 25.0, 48);
                                            is_voxel_mode = false;
                                            current_voxel_chunk = None;
                                            state.update_mesh(&planet_mesh);
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
                        KeyCode::Escape => target.exit(),
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
                            current_seed = current_seed.wrapping_add(1337);
                            let mut new_desc = herocraft_world::PlanetDescriptor::default();
                            new_desc.seed = current_seed;
                            generator = herocraft_world::PlanetWorldGenerator::new(new_desc);
                            planet_mesh = PlanetMesh::generate_planet(&generator, 25.0, 48);
                            is_voxel_mode = false;
                            current_voxel_chunk = None;
                            state.update_mesh(&planet_mesh);
                            ui_state.set_seed(current_seed);
                            info!("Raccourci [G] : Nouvelle planète générée (Graine: #{})", current_seed);
                        }
                        _ => {}
                    },

                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let dt = now.duration_since(last_frame_time).as_secs_f32();
                        last_frame_time = now;

                        camera.update(dt);
                        state.update_camera(&camera);

                        // LOD Dynamique : Transition vers le terrain voxel en zoom rapproché (D <= 34.0)
                        let is_near_ground = camera.distance <= 34.0;

                        if is_near_ground {
                            let eye_dir = camera.eye_position().normalize();
                            let (face, u, v) = PlanetFace::sphere_direction_to_face_uv(
                                DVec3::new(eye_dir.x as f64, eye_dir.y as f64, eye_dir.z as f64),
                            );
                            let chunk_x = (((u + 1.0) * 0.5 * 32.0).floor() as i32).clamp(0, 31);
                            let chunk_y = (((v + 1.0) * 0.5 * 32.0).floor() as i32).clamp(0, 31);

                            let needs_update = !is_voxel_mode
                                || current_voxel_chunk != Some((face, chunk_x, chunk_y));

                            if needs_update {
                                if !is_voxel_mode {
                                    info!("LOD : Entrée dans l'atmosphère, affichage des voxels 3D au sol !");
                                }
                                is_voxel_mode = true;
                                current_voxel_chunk = Some((face, chunk_x, chunk_y));

                                let voxel_mesh = VoxelMeshBuilder::build_incurved_grid(
                                    &generator,
                                    face,
                                    chunk_x,
                                    chunk_y,
                                    25.0, // Rayon de base
                                );
                                state.update_mesh(&voxel_mesh);
                            }
                        } else if is_voxel_mode {
                            info!("LOD : Sortie en orbite haute, vue globale de la planète.");
                            is_voxel_mode = false;
                            current_voxel_chunk = None;
                            state.update_mesh(&planet_mesh);
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

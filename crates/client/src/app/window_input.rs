//! Entrées de la fenêtre : fermeture, taille, focus, souris, molette et clavier

use super::*;

impl App {
    /// Fermeture de la fenêtre : la partie est enregistrée, puis le jeu quitte
    pub(super) fn close(&mut self, event_loop: &EventLoopTarget) {
        self.save_before_exit();
        self.ui_state.set_inventory_open(false);
        self.ui_state.inventory.save();
        self.ui_state.player.save();
        write_profile_report();
        crate::shaders::save_pipeline_cache();
        event_loop.exit();
    }

    /// Fenêtre redimensionnée, ou passée sur un écran d'une autre densité de pixels
    pub(super) fn resize(&mut self, physical_size: PhysicalSize<u32>, scale_factor: f64) {
        self.state.resize(physical_size);
        self.ui_state.set_scale_factor(scale_factor as f32);
        self.ui_state.set_screen_size(physical_size.width as f32, physical_size.height as f32);
        self.camera.aspect = physical_size.width as f32 / physical_size.height.max(1) as f32;
    }

    pub(super) fn focus_changed(&mut self, focused: bool) {
        self.window_focused = focused;
        if !focused {
            // Alt+Tab : les relâchements de touches n'arriveront pas
            keyboard_hook::set_capture(None);
            self.camera.release_inputs();
            self.zoom_modifier_held = false;
            self.is_dragging = false;
        }
    }

    /// Clic souris : glisser pour tourner la planète en vue satellite (curseur visible).
    /// Souris capturée, les clics restent libres pour l'interaction avec les blocs.
    pub(super) fn mouse_input(&mut self, btn_state: ElementState, button: MouseButton) {
        // Changé en bête : le clic gauche et le clic droit lancent ses deux attaques
        if self.cinematics.is_playing() {
            // Cinématique : les clics ne touchent ni les blocs ni les bêtes
        } else if let Some(attacks) = self.ui_state.beast_attacks.as_ref().filter(|_| self.cursor_captured) {
            if btn_state == ElementState::Pressed {
                match button {
                    MouseButton::Left => self.ui_state.skill_casts.push(attacks[0].clone()),
                    MouseButton::Right => self.ui_state.skill_casts.push(attacks[1].clone()),
                    _ => {}
                }
            }
        } else if self.cursor_captured {
            self.click_in_world(button, btn_state == ElementState::Pressed);
        }
        if !self.cursor_captured && matches!(button, MouseButton::Left | MouseButton::Right) {
            let ui_state = &self.ui_state;
            let is_over_ui = ui_state.is_settings_open
                || ui_state.chat.open
                || ui_state.ctx.is_pointer_over_egui()
                || ui_state.ctx.egui_wants_pointer_input();
            self.is_dragging = btn_state == ElementState::Pressed && !is_over_ui;
        }
    }

    /// Clic souris capturée : un habitant à qui parler, un établi à ouvrir, une bête à dépecer ou à
    /// nourrir, une Pierre du Néant à toucher, un aliment à manger, ou le bloc visé à casser ou poser
    fn click_in_world(&mut self, button: MouseButton, pressed: bool) {
        let Self {
            camera,
            block_interaction,
            creature_views,
            creature_registry,
            net,
            solo_creatures,
            solo_talks,
            ui_state,
            generator,
            lod_manager,
            audio,
            stone_events,
            dropped_items,
            solo_fields,
            state,
            ..
        } = self;
        // Pose appliquée et remaillée sur-le-champ : le bloc change dans la frame qui
        // suit le clic. Le clic gauche (casse, coup à une bête) avance à chaque frame.
        // Clic droit sur un habitant ou un voyageur : ses échanges s'ouvrent
        let villager = (pressed && button == MouseButton::Right)
            .then(|| aimed_creature(camera, block_interaction, creature_views, creature_registry))
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
        } else if let Some(action) = block_interaction.on_mouse_button(button, pressed) {
            block_interaction.update_target(camera, generator, &lod_manager.terrain_edits().clone());
            // Clic droit sur un établi : il s'ouvre (accroupi, on pose contre lui)
            let station = block_interaction.aimed_station().filter(|_| action == block_interaction::BlockAction::Place && !camera.is_crouching());
            // Clic droit sur une bête morte, couteau ou dague en main : on la dépèce
            let knife = action == block_interaction::BlockAction::Place && ui_state.inventory.slot(ui_state.inventory.selected()).is_some_and(|s| s.item.can_skin());
            let corpse = if knife { aimed_corpse(camera, block_interaction, creature_views, creature_registry) } else { None };
            // Clic droit sur une bête vivante avec sa nourriture en main : elle la mange
            let fed = corpse.is_none()
                && action == block_interaction::BlockAction::Place
                && station.is_none()
                && feed_aimed_creature(camera, block_interaction, creature_views, creature_registry, net.as_mut(), solo_creatures, ui_state, audio);
            // Une Pierre du Néant en main : le joueur la touche
            let stone = void_stones::held_stone(&ui_state.inventory).filter(|_| action == block_interaction::BlockAction::Place && station.is_none() && !fed && corpse.is_none());
            // Sinon, un aliment en main : le joueur le mange
            let food = ui_state.inventory.selected_food().filter(|_| action == block_interaction::BlockAction::Place && station.is_none() && !fed);
            if let Some(item) = stone {
                void_stones::touch(item, stone_events);
            } else if let Some(id) = corpse {
                skin_creature(id, net.as_mut(), solo_creatures, dropped_items, camera);
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
                let changes = block_interaction.apply(action, camera, generator, lod_manager, &mut ui_state.inventory);
                for &(pos, block, previous) in &changes {
                    block_sound(audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                    drop_broken_block(dropped_items, camera, pos, block, previous);
                    match net.as_mut() {
                        Some(client) => client.send_block_change(pos, block),
                        None => solo_fields.observe(pos, block),
                    }
                }
                if !changes.is_empty() {
                    state.sync_lod_meshes(lod_manager);
                }
            }
        }
    }

    /// Déplacement curseur visible (survol des boutons UI et rotation de la planète)
    pub(super) fn cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        if self.is_dragging && !self.cursor_captured && !self.ui_state.is_settings_open && !self.ui_state.screen_open() {
            let dx = (position.x - self.last_mouse_pos.0) as f32;
            let dy = (position.y - self.last_mouse_pos.1) as f32;
            self.camera.orbit(dx, dy);
        }
        self.last_mouse_pos = (position.x, position.y);
    }

    /// Molette : zoom continu FPS <-> TPS <-> Orbite avec la touche de zoom (Alt) maintenue,
    /// sinon case de la barre d'objets. En vue satellite, la molette seule zoome.
    pub(super) fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let ui_state = &mut self.ui_state;
        let wants_zoom = if self.cursor_captured {
            self.zoom_modifier_held
        } else {
            !(ui_state.is_settings_open
                || ui_state.screen_open()
                || ui_state.chat.open
                || ui_state.ctx.is_pointer_over_egui()
                || ui_state.ctx.egui_wants_pointer_input())
        };

        if self.cursor_captured && !self.zoom_modifier_held && ui_state.beast_attacks.is_none() {
            // Molette vers le haut : case précédente (vers la gauche)
            self.hotbar_scroll -= match delta {
                MouseScrollDelta::LineDelta(_x, y) => y,
                MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
            };
            let steps = self.hotbar_scroll.trunc();
            if steps != 0.0 {
                ui_state.inventory.scroll(steps as i32);
                self.hotbar_scroll -= steps;
            }
        } else if wants_zoom {
            let zoom_delta = match delta {
                MouseScrollDelta::LineDelta(_x, y) => y,
                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.04,
            };
            self.camera.zoom(zoom_delta);
        }
    }

    /// Touches clavier : actions attribuées dans Échap > Commandes
    pub(super) fn keyboard_input(&mut self, key_state: ElementState, key_code: KeyCode, logical_key: &Key, repeat: bool) {
        let ui_state = &mut self.ui_state;
        let is_pressed = key_state == ElementState::Pressed;

        // Apprend le caractère de la touche selon la disposition du clavier (AZERTY…)
        if is_pressed && !self.modifiers.intersects(ModifiersState::SHIFT | ModifiersState::CONTROL | ModifiersState::ALT) {
            ui_state.controls.learn_label(key_code, logical_key);
        }

        if self.cinematics.is_playing() {
            // Cinématique : le joueur ne fait que la regarder, ou la passer
            let skip_key = matches!(key_code, KeyCode::Escape | KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter);
            if skip_key && is_pressed && !repeat {
                self.cinematics.request_skip();
            }
        } else if key_code == KeyCode::Escape && is_pressed && ui_state.chat.open {
            // Échap referme d'abord le chat
            ui_state.chat.close();
        } else if key_code == KeyCode::Escape && is_pressed {
            // Touche Échap : annule une saisie de touche, ouvre le menu de pause,
            // remonte d'un sous-menu ou referme le menu
            let new_open = ui_state.handle_escape();

            // Réinitialise les touches de déplacement pour éviter tout glissement fantôme
            self.camera.release_inputs();
            self.zoom_modifier_held = false;

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
            if is_pressed && !repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenInventory) {
                ui_state.trade = None;
            }
        } else if ui_state.inventory_open {
            // Écran d'inventaire : seule sa touche le referme, sauf pendant la saisie
            // d'une recherche dans les recettes
            let typing = ui_state.ctx.egui_wants_keyboard_input();
            if is_pressed && !repeat && !typing && ui_state.controls.action_for(key_code) == Some(Action::OpenInventory) {
                ui_state.set_inventory_open(false);
            }
        } else if ui_state.skills_open {
            // Roue des compétences : sa touche la referme
            if is_pressed && !repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenSkills) {
                ui_state.set_skills_open(false);
            }
        } else if ui_state.map_open {
            // Carte : sa touche la referme
            if is_pressed && !repeat && ui_state.controls.action_for(key_code) == Some(Action::OpenMap) {
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
            game_key(action, is_pressed, is_pressed && !repeat, &mut self.camera, ui_state, &mut self.pending_actions, &mut self.jump_tap, &mut self.zoom_modifier_held);
        }
    }

    /// Mouvement brut de la souris : oriente le regard quand le curseur est capturé
    pub(super) fn mouse_motion(&mut self, delta: (f64, f64)) {
        herocraft_common::prof_scope!("boucle::mouvement_souris");
        if self.cursor_captured && !self.cinematics.is_playing() {
            let controls = &self.ui_state.controls;
            let step = input::LOOK_RADIANS_PER_COUNT * controls.mouse_sensitivity;
            let pitch_sign = if controls.invert_y { 1.0 } else { -1.0 };
            self.camera.look(delta.0 as f32 * step, delta.1 as f32 * step * pitch_sign);
        }
    }
}

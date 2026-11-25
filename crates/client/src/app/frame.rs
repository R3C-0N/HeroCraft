//! Une image du jeu, étape par étape

use super::*;

/// Ce que le réticule vise à cette image
#[derive(Default)]
struct Aim {
    /// La créature sous le réticule, qu'on frapperait
    creature: Option<u64>,
    /// L'habitant sous le réticule, sans bloc devant lui : on lit son nom
    looked_at: Option<u64>,
}

impl App {
    /// Une image : simulation du jeu, interface, puis rendu
    pub(super) fn redraw(&mut self, event_loop: &EventLoopTarget) {
        herocraft_common::prof_scope!("frame" => frame_guard);
        let (now, dt) = self.tick_clock();

        self.drive_benchmark(dt);
        self.replay_hooked_keys();
        self.update_screens_and_cursor();
        self.update_cinematics(dt);
        self.update_player(dt);
        self.update_weather_and_sky(dt);

        // Bloc visé par le réticule (souris capturée seulement) et bouton tenu enfoncé
        let (aim, mut terrain_edited) = self.update_aim(dt);
        terrain_edited |= self.poll_network();
        // Sommets des créatures dessinées à cette image (rapport du banc d'essai)
        let (edited, creature_vertices) = self.update_creatures(dt, aim.looked_at);
        terrain_edited |= edited;
        terrain_edited |= self.poll_pending_join();
        self.update_lan_browser(now);
        self.update_dropped_items(dt);
        self.dispatch_stone_events();
        {
            herocraft_common::prof_scope!("blocs::contour");
            // Une bête visée cache le contour et les fissures du bloc derrière elle
            self.state.set_block_outline(self.block_interaction.outline(&self.camera).filter(|_| aim.creature.is_none()));
            self.state.set_block_cracks(self.block_interaction.cracks(&self.camera).filter(|_| aim.creature.is_none()));
        }
        self.autosave();
        self.update_lod(dt, terrain_edited);

        // 1. Mise à jour de l'interface egui
        let ui_section = Instant::now();
        let (full_output, maybe_action) = self.update_ui();
        self.apply_skill_requests();
        self.run_chat();
        self.cast_skills();

        // 2. Traitement des actions déclenchées par les boutons egui et le clavier
        self.pending_actions.extend(maybe_action);
        for action in std::mem::take(&mut self.pending_actions) {
            herocraft_common::prof_scope!("frame::ui_action");
            self.apply_action(action, event_loop);
        }

        // 3. Tessellation egui et 4. rendu WGPU (3D + egui pass)
        self.render(full_output, ui_section);
        self.limit_fps();
        self.finish_frame(now, dt, creature_vertices, event_loop);

        drop(frame_guard);
        herocraft_common::profiler::frame_mark();
    }

    /// Temps écoulé depuis l'image précédente, et compteur FPS
    fn tick_clock(&mut self) -> (Instant, f32) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        // Mesure et mise à jour du compteur FPS (rafraîchi toutes les 250 ms pour une lecture stable)
        self.fps_frame_count += 1;
        let fps_elapsed = now.duration_since(self.fps_timer).as_secs_f32();
        if fps_elapsed >= 0.25 {
            let current_fps = (self.fps_frame_count as f32 / fps_elapsed).round() as u32;
            self.ui_state.set_fps(current_fps);
            self.fps_frame_count = 0;
            self.fps_timer = now;
        }
        (now, dt)
    }

    /// Banc d'essai : réglages et entrées de la phase en cours
    fn drive_benchmark(&mut self, dt: f32) {
        let Self { bench, camera, ui_state, generator, lod_manager, solo_creatures, creature_animator, creature_registry, state, current_seed, .. } = self;
        if let Some(b) = bench.as_mut() {
            if let Some(start) = b.before_frame(dt, camera, ui_state, generator) {
                lod_manager.set_distance_preset(ui_state.video_settings.lod_distance);
                lod_manager.set_precision(ui_state.video_settings.lod_precision);
                if start.reset_creatures {
                    *solo_creatures = herocraft_world::CreatureWorld::new(camera.base_radius as f64, *current_seed, creature_registry.clone());
                    *creature_animator = creatures::CreatureAnimator::default();
                }
                let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
                for (i, feet) in benchmark::stress_positions(camera.player_pos, block, start.extra_creatures).into_iter().enumerate() {
                    solo_creatures.spawn(i % creature_registry.len(), feet);
                }
            }
            state.simulated_time = b.sun_time;
        }
    }

    /// Touches prises aux autres programmes par le crochet clavier depuis l'image précédente
    fn replay_hooked_keys(&mut self) {
        let Self { ui_state, cinematics, camera, pending_actions, jump_tap, zoom_modifier_held, .. } = self;
        for hooked in keyboard_hook::drain() {
            let in_game = !ui_state.is_settings_open && !ui_state.screen_open() && !ui_state.chat.open && !cinematics.is_playing();
            if let (true, Some(action)) = (in_game, ui_state.controls.action_for(hooked.key)) {
                let first_press = hooked.pressed && !hooked.repeat;
                game_key(action, hooked.pressed, first_press, camera, ui_state, pending_actions, jump_tap, zoom_modifier_held);
            }
        }
    }

    /// Écrans fermés hors du sol, créateur de légende à la première arrivée, souris capturée ou
    /// libérée
    fn update_screens_and_cursor(&mut self) {
        let Self { ui_state, camera, bench, window_focused, cursor_captured, window, is_dragging, .. } = self;
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
        let want_capture = *window_focused
            && bench.is_none()
            && !ui_state.player.is_dead()
            && !ui_state.is_settings_open
            && !ui_state.screen_open()
            && !ui_state.chat.open
            && camera.target_distance <= camera.base_radius;
        if want_capture != *cursor_captured {
            *cursor_captured = want_capture;
            set_cursor_captured(window, want_capture);
            let controls = &ui_state.controls;
            keyboard_hook::set_capture(
                want_capture.then(|| keyboard_hook::interceptable(Action::ALL.map(|a| controls.key(a)))),
            );
            ui_state.set_pointer_captured(want_capture);
            *is_dragging = false;
        }
    }

    /// Cinématique : elle tient la caméra, ferme les écrans et renvoie ses événements au jeu
    /// (changer d'époque à la fin de la vision…)
    fn update_cinematics(&mut self, dt: f32) {
        let Self {
            cinematics,
            current_seed,
            launch_announced,
            bench,
            camera,
            generator,
            state,
            vision_stone,
            ui_state,
            stone_events,
            net,
            pending_actions,
            block_interaction,
            ..
        } = self;
        cinematics.set_world(std::path::PathBuf::from(solo_save_dir(*current_seed)));
        // Le lancement du jeu, une fois le monde chargé : la Genèse s'y joue (pas pendant
        // le banc d'essai)
        if !*launch_announced {
            *launch_announced = true;
            // `--sans-genese` : directement dans le monde (scénarios de test)
            if bench.is_none() && !std::env::args().any(|a| a == "--sans-genese") {
                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::LAUNCH_EVENT));
            }
        }
        let was_playing = cinematics.is_playing();
        for event in cinematics.update(dt, camera, generator) {
            info!("Événement de cinématique : {} {:?}", event.name, event.tags);
            state.time_frozen = cinematic::frozen_after(&event, state.time_frozen);
            if event.name == cinematic::VISION_ENDED {
                if let Some(item) = vision_stone.take() {
                    void_stones::absorb(item, &mut ui_state.inventory, stone_events);
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
        let (card_uniform, card_instances) = cinematics.cards(camera, state.card_layers());
        state.update_cards(&card_uniform, &card_instances);
        if cinematics.is_playing() && !was_playing {
            ui_state.set_inventory_open(false);
            ui_state.set_skills_open(false);
            ui_state.set_map_open(false);
            ui_state.trade = None;
            ui_state.chat.close();
            block_interaction.reset();
        }
    }

    /// Caméra et joueur (compétences qui le déplacent, chute, jauges), puis son
    fn update_player(&mut self, dt: f32) {
        let Self {
            ui_state,
            camera,
            creature_views,
            creature_registry,
            skill_play,
            generator,
            audio,
            net,
            bench,
            probe,
            state,
            sky_check_in,
            sky_covered,
            client_weather,
            ..
        } = self;
        // Mise à jour de la caméra uniquement si le menu n'est pas ouvert
        let section = Instant::now();
        if !ui_state.is_settings_open {
            herocraft_common::prof_scope!("frame::camera_update");
            camera.obstacles = creatures::obstacles(creature_views, creature_registry);
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
                        camera.skill_teleport(feet, look, generator);
                    }
                }
            }
            // Relevé sur place par une compétence (Intervention Suprême) : l'écran de mort se ferme
            if camera.is_dead && !ui_state.player.is_dead() {
                camera.is_dead = false;
                audio.reset_player();
                info!("Relevé sur place.");
            }
            camera.update(dt, generator);
            // Jauges du joueur, chute terminée : dommages (signalés au serveur en
            // ligne), vue secouée et, si elle tue, écran de mort
            ui_state.player.tick(dt, camera);
            if let Some(blocks) = camera.take_landing() {
                if let Some(client) = net.as_mut() {
                    client.send_fall(blocks);
                }
                // Banc d'essai : le joueur ne se blesse pas
                if bench.is_none() {
                    let hurt = ui_state.player.fall(blocks);
                    show_wound(hurt, &format!("chute de {:.0} blocs", blocks), camera, ui_state, audio);
                }
            }
        }
        probe.section("caméra et joueur", section);
        {
            herocraft_common::prof_scope!("son");
            update_sound(audio, camera, generator, state, ui_state, dt, sky_check_in, sky_covered, client_weather.visual());
        }
    }

    /// Météo au joueur (lumière voilée, nuages, pluie ou neige autour de lui), puis ciel, ombres
    /// et atmosphère d'après la caméra
    fn update_weather_and_sky(&mut self, dt: f32) {
        let Self { camera, ui_state, client_weather, audio, state, column_tops, generator, net, .. } = self;
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
                if let Some(columns) = column_tops.update(dt, collider, generator, camera.player_pos, camera.base_radius) {
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
        state.update_camera(camera, &lighting, &ui_state.cloud_settings, ui_state.video_settings.flattening, &visual);
        state.update_post(&ui_state.post_settings, if ground { visual.overcast() } else { 0.0 });
        state.set_msaa(ui_state.video_settings.msaa);
        // Sous un ciel couvert, la lumière est diffuse : les ombres s'effacent, et leurs passes (avec
        // les rayons crépusculaires) sont coupées sous un ciel tout gris
        let mut shadow_settings = ui_state.shadow_settings;
        let overcast = if ground { visual.overcast() } else { 0.0 };
        shadow_settings.strength *= 1.0 - overcast;
        shadow_settings.cloud_strength *= 1.0 - overcast;
        shadow_settings.enabled &= overcast < 0.9;
        state.update_shadows(camera, &shadow_settings);
        state.update_atmosphere(camera, &ui_state.atmosphere_settings, &lighting);
    }

    /// L'arme en main, puis ce que vise le réticule (souris capturée seulement) : tir, coup à une
    /// créature ou bloc qui se casse. Rend ce qui est visé, et si des blocs ont changé.
    fn update_aim(&mut self, dt: f32) -> (Aim, bool) {
        let Self {
            skill_play,
            ui_state,
            cursor_captured,
            lod_manager,
            block_interaction,
            camera,
            generator,
            creature_views,
            creature_registry,
            strike_lock,
            net,
            audio,
            solo_creatures,
            tutorial,
            dropped_items,
            solo_fields,
            ..
        } = self;
        let mut terrain_edited = false;
        // L'arme en main : sa cadence, son chargeur, ce qu'elle tire
        skill_play.hold(ui_state.inventory.selected_item(), ui_state.player.stats.class);
        ui_state.weapon_hud = skill_play.weapon_hud(ui_state.skill_charging.as_deref());
        let mut aim = Aim::default();
        if !*cursor_captured {
            block_interaction.reset();
            return (aim, terrain_edited);
        }
        let edits = lod_manager.terrain_edits().clone();
        block_interaction.update_target(camera, generator, &edits);
        let aimed = aimed_creature(camera, block_interaction, creature_views, creature_registry);
        aim.creature = aimed;
        // L'habitant sous le réticule, sans bloc devant lui : on lit son nom
        aim.looked_at = block_interaction.sight(camera, generator, &edits, villagers::NAME_TAG_BLOCKS as f64).and_then(|sight| {
            let (eye, dir, _) = camera.view_ray()?;
            let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
            creatures::aimed_creature(creature_views, creature_registry, eye, dir, sight, block).map(|(id, _)| id)
        });
        // Une bête devant le bloc visé : c'est elle qu'on frappe, au rythme des
        // coups, et le bloc derrière elle ne se fissure pas
        // Un habitant, un garde ou un voyageur ne se frappe que si le clic a commencé
        // sur lui : celui qui passe devant un bloc en train de casser n'est pas
        // frappé par mégarde (les gardes s'en prendraient au joueur)
        let fresh = block_interaction.take_fresh_press();
        if fresh {
            *strike_lock = aimed;
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
                                let hit = skill_play.solo_shoot(camera.player_pos, aim, damage, fired, solo_creatures, &mut ui_state.player.stats);
                                skill_play.show_shot(None, camera.eye_position(), hit.to + up * 0.8 * block, hit.target, hit.special.as_deref(), block);
                                announce_solo_crimes(solo_creatures, ui_state);
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
            *strike_lock = None;
        }
        let humanoid = |id: u64| creature_views.iter().find(|v| v.id == id).is_some_and(|v| villagers::is_humanoid(v, creature_registry));
        let striking = aimed.filter(|_| !shooting && block_interaction.is_breaking_held()).filter(|&id| *strike_lock == Some(id) || !humanoid(id));
        if let Some(id) = striking {
            block_interaction.pause_mining();
            if hit_creature(id, skill_play, camera, net.as_mut(), solo_creatures, &mut ui_state.player.stats, audio, &ui_state.audio_settings) {
                skill_play.solo_weapon_hit(id, solo_creatures, camera.player_pos, &mut ui_state.player.stats);
            }
            // Un habitant tué en solo : le chat l'annonce (en ligne, c'est le serveur)
            announce_solo_crimes(solo_creatures, ui_state);
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
            for (pos, block, previous) in block_interaction.apply(action, camera, generator, lod_manager, &mut ui_state.inventory) {
                block_sound(audio, Some(previous), block, 0.0, &ui_state.audio_settings);
                drop_broken_block(dropped_items, camera, pos, block, previous);
                terrain_edited = true;
                match net.as_mut() {
                    Some(client) => client.send_block_change(pos, block),
                    None => solo_fields.observe(pos, block),
                }
            }
        }
        (aim, terrain_edited)
    }

    /// Partie en ligne : position envoyée, blocs des autres appliqués, autres joueurs affichés.
    /// Rend vrai si des blocs ont changé.
    fn poll_network(&mut self) -> bool {
        let Self {
            net,
            camera,
            ui_state,
            vision,
            generator,
            chunk_cache,
            lod_manager,
            state,
            tutorial,
            cinematics,
            solo_creatures,
            block_interaction,
            audio,
            dropped_items,
            client_weather,
            skill_play,
            creature_registry,
            pending_actions,
            ..
        } = self;
        let mut terrain_edited = false;
        let Some(client) = net.as_mut() else { return terrain_edited };
        herocraft_common::prof_scope!("reseau");
        client.send_move(camera, ui_state.inventory.selected_item());
        client.send_equipment(ui_state.inventory.worn().into_iter().map(|i| i.0).collect());
        let mut lost = None;
        for event in client.poll() {
            match event {
                NetEvent::Vision { holder, feet } => {
                    switch_epoch(holder, feet, None, None, vision, generator, chunk_cache, lod_manager, camera, state);
                    if holder.is_some() {
                        if tutorial.is_none() {
                            *tutorial = begin_vision_tutorial(feet, camera, generator, lod_manager, ui_state, None);
                            state.sync_lod_meshes(lod_manager);
                        }
                        cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::VISION_ENTERED));
                    } else {
                        end_vision_tutorial(tutorial, ui_state, solo_creatures);
                    }
                    block_interaction.invalidate();
                    terrain_edited = true;
                }
                NetEvent::BlockChanged(pos, block) => {
                    let previous = lod_manager.terrain_edits().block(generator, pos);
                    if lod_manager.set_block(generator, pos, block) {
                        // Bloc changé par un autre joueur : entendu s'il est proche
                        let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
                        let centre = geometry.block_corners(pos).iter().sum::<glam::DVec3>() / 8.0;
                        let distance = (centre.as_vec3() - camera.player_pos).length() / geometry.block_height as f32;
                        block_sound(audio, Some(previous), block, distance, &ui_state.audio_settings);
                        terrain_edited = true;
                        block_interaction.invalidate();
                    }
                }
                NetEvent::Stats(stats) => ui_state.player.sync(stats, camera),
                NetEvent::Hurt { damage, cause } => {
                    let hurt = ui_state.player.bitten(damage, None, &cause);
                    show_wound(hurt, &cause, camera, ui_state, audio);
                }
                NetEvent::Loot { position, items } => drop_loot(dropped_items, camera, position, &items),
                NetEvent::Laid { position, item } => lay_egg(dropped_items, camera, position, item),
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
                NetEvent::CreatureShots(shots) => skill_play.on_shots(&shots, creature_registry),
                NetEvent::Projectiles(events) => skill_play.on_projectiles(&events, creature_registry),
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
            ui_state.name_tags = client.name_tags(camera, state.size.width as f32, state.size.height as f32);
        }
        terrain_edited
    }

    /// Connexion menée en tâche de fond : à son terme, la planète du serveur remplace la partie
    /// solo. Rend vrai si le terrain a changé.
    fn poll_pending_join(&mut self) -> bool {
        let Some(result) = self.pending_join.as_ref().and_then(|p| p.poll()) else { return false };
        let addr = self.pending_join.take().map(|p| p.addr).unwrap_or_default();
        let Self {
            tutorial,
            ui_state,
            solo_creatures,
            vision,
            generator,
            lod_manager,
            current_seed,
            hosted,
            camera,
            chunk_cache,
            window,
            state,
            client_weather,
            dropped_items,
            block_interaction,
            net,
            ..
        } = self;
        match result {
            Ok(world) => {
                end_vision_tutorial(tutorial, ui_state, solo_creatures);
                if vision.take().is_some() {
                    *generator = generator.with_epoch(herocraft_world::Epoch::Present);
                    lod_manager.clear(*current_seed);
                }
                let edits = Arc::new(herocraft_world::TerrainEdits::from_chunks(world.edited_chunks));
                if let Some(host) = hosted.as_ref().filter(|_| world.seed == *current_seed) {
                    // Notre propre partie : même planète et mêmes blocs, la vue et le
                    // joueur restent en place, seuls les blocs passent par le serveur
                    camera.set_terrain(edits.clone());
                    lod_manager.set_terrain_edits(edits);
                    *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(None, 256);
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
                    *current_seed = world.seed;
                    switch_planet(*current_seed, true, edits, None, generator, chunk_cache, lod_manager, camera, state, ui_state, client_weather);
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
                let mut client = world.client;
                // Le serveur apprend la voie du héros (ses jauges en dépendent)
                if let Some(legend) = &ui_state.legend {
                    client.send_legend(legend);
                }
                camera.hero_height = hero_height(ui_state);
                *net = Some(client);
                true
            }
            Err(e) => {
                warn!("Connexion à {} impossible : {}", addr, e);
                ui_state.multiplayer.notice = Some(format!("Connexion impossible : {}", e));
                ui_state.multiplayer.status = NetStatus::Solo;
                *hosted = None;
                false
            }
        }
    }

    /// Page Multijoueur ouverte en solo : écoute des parties annoncées sur le réseau local
    fn update_lan_browser(&mut self, now: Instant) {
        let Self { ui_state, lan_browser, lan_retry_at, .. } = self;
        let browsing = ui_state.is_settings_open
            && ui_state.menu_page == MenuPage::Multiplayer
            && ui_state.multiplayer.status == NetStatus::Solo;
        if browsing {
            if lan_browser.is_none() && now >= *lan_retry_at {
                match LanBrowser::new() {
                    Ok(browser) => *lan_browser = Some(browser),
                    Err(e) => {
                        log::debug!("Recherche des parties du réseau local impossible : {}", e);
                        *lan_retry_at = now + std::time::Duration::from_secs(2);
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
            *lan_browser = None;
            ui_state.multiplayer.lan_servers.clear();
        }
    }

    /// Blocs tombés au sol : chute, ramassage par le joueur au sol (menu fermé), dessin
    fn update_dropped_items(&mut self, dt: f32) {
        let Self { camera, ui_state, dropped_items, generator, stone_events, state, game_start, .. } = self;
        if let Some(collider) = camera.collider.as_ref() {
            let h = collider.block_height();
            if !ui_state.is_settings_open {
                let on_ground = camera.target_distance <= camera.base_radius;
                let body = on_ground.then(|| camera.player_pos + camera.player_pos.normalize_or_zero() * 0.9 * h);
                dropped_items.update(dt, generator, collider, body, &mut ui_state.inventory);
            }
            for event in dropped_items.take_stone_events() {
                stone_events.push(event);
            }
            if let Some(mesh) = dropped_items.take_mesh(h, game_start.elapsed().as_secs_f32()) {
                state.set_dropped_items(&mesh);
            }
        }
    }

    /// Pierres du Néant obtenues, touchées ou absorbées : le chat, la vision, le tutoriel et les
    /// cinématiques en sont avertis
    fn dispatch_stone_events(&mut self) {
        let Self { stone_events, ui_state, vision_stone, tutorial, net, camera, skill_effects, cinematics, .. } = self;
        for event in void_stones::dispatch(stone_events, &mut ui_state.chat) {
            // Une Pierre entière obtenue : la vision (`fin-de-la-vision`), puis
            // `vision_ended` la fait absorber
            if let herocraft_common::StoneEvent::Obtained { kind: herocraft_common::StoneKind::Whole, item, .. } = event {
                *vision_stone = Some(item);
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
    }

    /// Sauvegarde automatique toutes les 30 secondes (pas pendant le banc d'essai)
    fn autosave(&mut self) {
        if self.last_autosave.elapsed().as_secs() >= 30 && self.bench.is_none() {
            save_terrain_edits(&self.lod_manager);
            if self.net.is_none() && self.hosted.is_none() {
                save_solo_creatures(&self.solo_creatures, &self.solo_fields);
                save_sky_clock(self.current_seed, &self.state, &self.client_weather);
            }
            self.ui_state.inventory.save();
            self.last_autosave = Instant::now();
        }
    }

    /// Mise à jour adaptative du QuadTree LOD, et envoi de ses maillages au GPU
    fn update_lod(&mut self, dt: f32, terrain_edited: bool) {
        herocraft_common::prof_scope!("frame::lod_and_upload" => lod_guard);
        let section = Instant::now();
        if self.lod_manager.update(self.camera.lod_eye(), &self.generator, &mut self.chunk_cache, dt, None)
            || terrain_edited
            || self.state.has_pending_lod_uploads()
        {
            self.state.sync_lod_meshes(&self.lod_manager);
        }
        drop(lod_guard);
        self.probe.section("LOD et envoi au GPU", section);
    }

    /// L'interface egui de l'image : carte, recharges des compétences, cinématique. Rend ce
    /// qu'egui a dessiné et l'action d'un bouton cliqué.
    fn update_ui(&mut self) -> (egui::FullOutput, Option<UiAction>) {
        let Self { camera, ui_state, generator, skill_play, cinematics, .. } = self;
        let is_ground = camera.distance <= camera.base_radius;
        // Carte et boussole : position du joueur, demandes au fil de la carte
        ui_state.world_map.navigation = is_ground.then(|| world_map::Navigation {
            up: camera.player_pos.as_dvec3().normalize(),
            yaw: camera.player_yaw as f64,
        });
        let map_ctx = ui_state.ctx.clone();
        ui_state.world_map.pump(&map_ctx, generator, ui_state.map_open);
        let cooldown_now = Instant::now();
        for slot in 0..skill_bar::SKILL_SLOTS {
            ui_state.fusion_face = skill_play.fusion_face(&ui_state.player.stats);
            ui_state.skill_cooldowns[slot] = ui_state.skill_bar.get(slot).map_or((0.0, 0.0), |id| skill_play.cooldown(id, cooldown_now));
        }
        ui_state.cinematic = cinematics.overlay();
        ui_state.update_ui(is_ground)
    }

    /// Roue des compétences : point investi et expérience d'essai. En ligne, le serveur juge et
    /// renvoie les statistiques ; en solo, le jeu applique les mêmes règles et enregistre la
    /// progression.
    fn apply_skill_requests(&mut self) {
        let Self { ui_state, net, .. } = self;
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
    }

    /// Chat : message envoyé aux autres (en ligne) ou commande exécutée ici
    fn run_chat(&mut self) {
        let Self {
            command_file,
            ui_state,
            camera,
            generator,
            state,
            net,
            solo_creatures,
            creature_registry,
            current_seed,
            pending_actions,
            next_planet_seed,
            client_weather,
            cinematics,
            dropped_items,
            ..
        } = self;
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
        for line in ui_state.chat.take_submitted() {
            if line.starts_with('/') {
                ui_state.chat.echo(&line);
                let result = commands::parse(&line).and_then(|command| {
                    let mut game = Game {
                        camera,
                        generator,
                        sky: state,
                        ui: ui_state,
                        net: net.as_mut(),
                        creatures: solo_creatures,
                        registry: creature_registry,
                        seed: *current_seed,
                        pending_actions,
                        next_planet_seed,
                        weather: client_weather,
                        cinematics,
                        dropped: dropped_items,
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
    }

    /// Compétences qui se chargent, puis compétences lancées (touches de la barre, /cast)
    fn cast_skills(&mut self) {
        let Self { ui_state, skill_play, net, camera, block_interaction, generator, lod_manager, .. } = self;
        // Compétences qui se chargent : le serveur compte le temps tenu jusqu'au relâchement
        for skill in std::mem::take(&mut ui_state.skill_charges) {
            skill_play.begin_charge(Some(&skill));
            if let Some(client) = net.as_mut() {
                client.send_begin_charge(Some(&skill));
            }
        }
        // Compétences lancées : visée du regard, et du bloc visé s'il y en a un
        for skill in std::mem::take(&mut ui_state.skill_casts) {
            let (_, _, _, aim) = camera.look_vectors();
            // Un grappin vise plus loin que la main : jusqu'à sa portée
            let aimed = match skill_play::grapple_of(herocraft_common::skills::skills(), &skill).filter(|g| g.to == herocraft_common::skills::GrappleTo::Block) {
                Some(grapple) => {
                    let rank = herocraft_common::skills::skills().rank(&ui_state.player.stats, &skill);
                    block_interaction.hit_within(camera, generator, &lod_manager.terrain_edits().clone(), grapple.range.at(rank) as f64).map(|d| camera.eye_position() + aim * d)
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
    }

    /// Tessellation egui, puis rendu WGPU (3D + egui pass)
    fn render(&mut self, full_output: egui::FullOutput, ui_section: Instant) {
        let Self { ui_state, state, probe, .. } = self;
        herocraft_common::prof_scope!("frame::egui_tessellate" => tess_guard);
        let paint_jobs = ui_state.ctx.tessellate(full_output.shapes, ui_state.scale_factor);
        drop(tess_guard);
        probe.section("interface", ui_section);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [state.size.width, state.size.height],
            pixels_per_point: ui_state.scale_factor,
        };

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
    }

    /// Limiteur de framerate si V-Sync est désactivée
    fn limit_fps(&self) {
        if !self.ui_state.video_settings.vsync {
            let min_duration = match self.ui_state.video_settings.fps_limit {
                crate::ui::FpsLimitSetting::Fps60 => Some(std::time::Duration::from_millis(16)),
                crate::ui::FpsLimitSetting::Fps120 => Some(std::time::Duration::from_micros(8333)),
                crate::ui::FpsLimitSetting::Unlimited => None,
            };
            if let Some(target_dur) = min_duration {
                let frame_elapsed = self.last_frame_time.elapsed();
                if frame_elapsed < target_dur {
                    herocraft_common::prof_scope!("frame::fps_limiter_sleep");
                    std::thread::sleep(target_dur - frame_elapsed);
                }
            }
        }
    }

    /// Fin de l'image : à-coup signalé, phase du banc d'essai mesurée (et le jeu quitte à la fin
    /// du banc)
    fn finish_frame(&mut self, now: Instant, dt: f32, creature_vertices: usize, event_loop: &EventLoopTarget) {
        // À-coup : signalé dans la console avec ses étapes les plus longues, et gardé
        // quelques secondes à l'écran sous le compteur FPS
        let record = self.probe.finish(now);
        let total_ms = record.total().as_secs_f32() * 1000.0;
        if total_ms >= benchmark::HITCH_MS {
            // Au plus un message par seconde : une machine très lente n'inonde pas la console
            if self.last_hitch_log.is_none_or(|t: Instant| t.elapsed().as_secs_f32() >= 1.0) {
                warn!("À-coup : image de {:.0} ms ({}).", total_ms, record.describe());
                self.last_hitch_log = Some(Instant::now());
            }
            self.ui_state.set_hitch(total_ms);
        }
        if let Some(b) = self.bench.as_mut() {
            let shown = if self.net.is_none() { self.solo_creatures.creatures().len() } else { self.creature_views.len() };
            if b.after_frame(dt, &record, shown, creature_vertices) {
                info!("Banc d'essai terminé : rapport dans ./benchmark/.");
                write_profile_report();
                crate::shaders::save_pipeline_cache();
                event_loop.exit();
            }
        }
    }
}

/// En solo, les habitants tués et les attaques de village sont annoncés dans le chat (en ligne,
/// c'est le serveur)
pub(super) fn announce_solo_crimes(solo_creatures: &mut herocraft_world::CreatureWorld, ui_state: &mut UiState) {
    let name = ui_state.legend.as_ref().map(|l| l.appearance.nom.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Le joueur".to_string());
    for murder in solo_creatures.take_murders() {
        ui_state.chat.server(&murder.announce(&name));
    }
    for assault in solo_creatures.take_assaults() {
        ui_state.chat.server(&assault.warning());
    }
}

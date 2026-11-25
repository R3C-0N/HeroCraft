//! Actions déclenchées par les boutons de l'interface et les raccourcis du clavier (`UiAction`)

use super::*;

impl App {
    /// Applique une action des boutons egui ou du clavier
    pub(super) fn apply_action(&mut self, action: UiAction, event_loop: &EventLoopTarget) {
        match action {
            UiAction::PreviewSound => self.audio.preview(&self.ui_state.audio_settings),
            UiAction::Vision(true) if self.vision.is_none() => self.enter_vision(),
            UiAction::Vision(false) => self.leave_vision(),
            UiAction::Vision(true) => {}
            UiAction::LegendSealed => self.seal_legend(),
            UiAction::Respawn => self.respawn(),
            UiAction::ResumeGame => {
                self.ui_state.set_settings_open(false);
                info!("Bouton cliqué : Reprise de la partie.");
            }
            UiAction::QuitGame => {
                self.save_before_exit();
                self.ui_state.inventory.save();
                self.ui_state.player.save();
                info!("Bouton cliqué : Sauvegarde des chunks et fermeture du jeu.");
                write_profile_report();
                crate::shaders::save_pipeline_cache();
                event_loop.exit();
            }
            UiAction::ToggleVsync => {
                let vsync = self.ui_state.video_settings.vsync;
                self.state.set_vsync(vsync);
                info!("Paramètre V-Sync mis à jour : {}", if vsync { "ACTIVÉE" } else { "DÉSACTIVÉE" });
            }
            UiAction::ToggleFullscreen => {
                let is_fs = self.ui_state.video_settings.fullscreen;
                if is_fs {
                    self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                } else {
                    self.window.set_fullscreen(None);
                }
                info!("Paramètre Plein Écran mis à jour : {}", if is_fs { "ACTIVÉ" } else { "DÉSACTIVÉ" });
            }
            UiAction::SetLodDistance => {
                self.lod_manager.set_distance_preset(self.ui_state.video_settings.lod_distance);
                self.lod_manager.set_precision(self.ui_state.video_settings.lod_precision);
                info!("Paramètre Distance LOD mis à jour : {:?}", self.ui_state.video_settings.lod_distance);
            }
            UiAction::SetLodPrecision => {
                self.lod_manager.set_precision(self.ui_state.video_settings.lod_precision);
                info!("Paramètre Précision LOD mis à jour : {:?}", self.ui_state.video_settings.lod_precision);
            }
            UiAction::SetFpsLimit => {
                info!("Paramètre Limite FPS mis à jour : {:?}", self.ui_state.video_settings.fps_limit);
            }
            UiAction::GenerateNewPlanet if self.ui_state.multiplayer.status != NetStatus::Solo => {
                info!("Nouvelle planète impossible en ligne : la planète est celle du serveur.");
            }
            UiAction::OpenToLan | UiAction::JoinServer if self.ui_state.multiplayer.status != NetStatus::Solo => {}
            UiAction::OpenToLan => self.open_to_lan(),
            UiAction::JoinServer => self.join_server(),
            UiAction::LeaveServer => self.leave_server(),
            UiAction::GenerateNewPlanet => self.generate_new_planet(),
            UiAction::ToggleGroundCamera => {
                self.camera.toggle_view(&self.generator);
                info!(
                    "Bouton cliqué : Transition vers {}",
                    if self.camera.target_distance <= self.camera.base_radius { "Vue Première Personne (FPS)" } else { "Vue Satellite (Orbite)" }
                );
            }
            UiAction::ToggleAutoRotate => {
                self.camera.auto_rotate = !self.camera.auto_rotate;
                self.ui_state.set_auto_rotate(self.camera.auto_rotate);
                info!("Auto-rotation = {}", self.camera.auto_rotate);
            }
            UiAction::ResetCamera => {
                self.camera.reset();
                info!("Vue réinitialisée.");
            }
            UiAction::ToggleTectonics | UiAction::ToggleOceanCurrents => {
                let filter = self.ui_state.active_filter();
                self.lod_manager.set_filter(filter);
                self.lod_manager.update(self.camera.lod_eye(), &self.generator, &mut self.chunk_cache, 0.0, None);
                self.state.sync_lod_meshes(&self.lod_manager);
                info!("Filtre de visualisation sélectionné : {:?}", filter);
            }
        }
    }

    /// Entre dans la vision du temple le plus proche, s'il y en a un
    fn enter_vision(&mut self) {
        let Self {
            camera,
            generator,
            chunk_cache,
            lod_manager,
            vision,
            state,
            tutorial,
            ui_state,
            solo_creatures,
            creature_registry,
            cinematics,
            block_interaction,
            ..
        } = self;
        let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
        let entry = geometry
            .block_at_point(camera.player_pos.as_dvec3())
            .and_then(|at| generator.vision_entry(at.face, at.x, at.z, VISION_TEMPLE_BLOCKS, herocraft_world::Culture::Humains));
        match entry {
            Some(entry) => {
                info!("Vision du temple {:?} ({}, {}), tenu par les {}.", entry.site.face, entry.site.center().0, entry.site.center().1, entry.holder.name());
                // Les blocs du présent sont enregistrés avant de les mettre de côté
                let _ = chunk_cache.flush_dirty();
                save_terrain_edits(lod_manager);
                let feet = geometry.corner(entry.feet.face, entry.feet.x as f64 + 0.5, entry.feet.y as f64, entry.feet.z as f64 + 0.5).as_vec3();
                let back = camera.player_pos;
                switch_epoch(Some(entry.holder), feet, Some(back), None, vision, generator, chunk_cache, lod_manager, camera, state);
                if tutorial.is_none() {
                    *tutorial = begin_vision_tutorial(feet, camera, generator, lod_manager, ui_state, Some((solo_creatures, creature_registry)));
                    state.sync_lod_meshes(lod_manager);
                }
                cinematics.notify(&herocraft_common::cinematics::GameEvent::new(cinematic::VISION_ENTERED));
                block_interaction.invalidate();
            }
            None => ui_state.chat.error("No temple nearby"),
        }
    }

    /// Fin de la vision : retour au présent, là où le joueur l'avait quitté
    fn leave_vision(&mut self) {
        let Self {
            vision,
            current_seed,
            generator,
            chunk_cache,
            lod_manager,
            camera,
            state,
            tutorial,
            ui_state,
            solo_creatures,
            skill_effects,
            block_interaction,
            ..
        } = self;
        if let Some(back) = vision.as_ref().and_then(|v| v.back) {
            let storage = solo_terrain(*current_seed).0;
            switch_epoch(None, back, None, storage, vision, generator, chunk_cache, lod_manager, camera, state);
            // Le souvenir achevé : il revient dans un nuage de fumée violette
            if end_vision_tutorial(tutorial, ui_state, solo_creatures) {
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

    /// Légende scellée dans le créateur : enregistrée, et apprise du serveur en ligne
    fn seal_legend(&mut self) {
        let Self { ui_state, hosted, current_seed, net, camera, .. } = self;
        let Some(legend) = ui_state.take_sealed_legend() else { return };
        let online = ui_state.multiplayer.status != NetStatus::Solo && hosted.is_none();
        let file = legend_file(*current_seed, online);
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
        camera.hero_height = hero_height(ui_state);
    }

    /// Réapparition après la mort : au point d'apparition, ou dans le souvenir au dernier point
    /// franchi
    fn respawn(&mut self) {
        let Self { ui_state, net, tutorial, camera, generator, audio, .. } = self;
        ui_state.player.respawn();
        if let Some(client) = net.as_mut() {
            client.send_respawn();
        }
        // Tombé dans le souvenir : il se relève au dernier point franchi
        let checkpoint = tutorial.as_ref().and_then(|t| t.assault.as_ref()).and_then(|a| a.checkpoint());
        match checkpoint {
            Some(dir) => {
                let dir = dir.normalize_or_zero();
                camera.player_pos = dir * camera.ground_radius_at(dir, generator);
                camera.sync_orbit_to_ground();
            }
            None => camera.spawn_on_ground(generator),
        }
        audio.reset_player();
        info!("Réapparition au point d'apparition.");
    }

    /// Ouvre la partie solo au réseau local : un serveur démarre ici sur le monde solo, et le jeu
    /// s'y connecte
    fn open_to_lan(&mut self) {
        // Le serveur reprend la sauvegarde du monde : elle est d'abord mise à jour
        self.save_solo_world();
        self.solo_seed = self.current_seed;
        let pseudo = self.ui_state.multiplayer.pseudo.trim().to_string();
        let save_dir = std::path::PathBuf::from(solo_save_dir(self.current_seed));
        match HostedServer::start(self.current_seed, save_dir, &pseudo) {
            Ok(host) => {
                let addr = format!("127.0.0.1:{}", host.port);
                self.ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
                self.ui_state.multiplayer.notice = None;
                self.pending_join = Some(PendingJoin::start(addr, pseudo));
                self.hosted = Some(host);
            }
            Err(e) => {
                warn!("{}", e);
                self.ui_state.multiplayer.notice = Some(e);
            }
        }
    }

    /// Rejoint le serveur choisi sur la page Multijoueur (connexion menée en tâche de fond)
    fn join_server(&mut self) {
        self.save_solo_world();
        self.solo_seed = self.current_seed;
        let addr = network::server_address(&self.ui_state.multiplayer.join_target);
        let pseudo = self.ui_state.multiplayer.pseudo.trim().to_string();
        info!("Connexion au serveur {} en tant que « {} »...", addr, pseudo);
        self.ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
        self.ui_state.multiplayer.notice = None;
        self.pending_join = Some(PendingJoin::start(addr, pseudo));
    }

    /// Quitte la partie en ligne (ou arrête celle ouverte ici) : retour à la partie solo
    fn leave_server(&mut self) {
        let Self {
            tutorial,
            ui_state,
            solo_creatures,
            vision,
            generator,
            lod_manager,
            current_seed,
            net,
            pending_join,
            hosted,
            state,
            solo_seed,
            camera,
            creature_registry,
            solo_fields,
            creatures_seed,
            chunk_cache,
            client_weather,
            dropped_items,
            block_interaction,
            window,
            ..
        } = self;
        end_vision_tutorial(tutorial, ui_state, solo_creatures);
        if vision.take().is_some() {
            *generator = generator.with_epoch(herocraft_world::Epoch::Present);
            lod_manager.clear(*current_seed);
        }
        if let Some(mut client) = net.take() {
            client.disconnect();
        }
        *pending_join = None;
        // Le serveur ouvert ici s'arrête en enregistrant le monde solo
        let was_hosting = hosted.take().is_some();
        state.set_player_outlines(&[]);
        ui_state.name_tags.clear();
        let (storage, edits) = solo_terrain(*solo_seed);
        // Le serveur ouvert ici a pu faire vivre (et tuer) les bêtes du monde
        // solo : elles sont reprises de la sauvegarde qu'il vient d'écrire
        *solo_creatures = solo_creature_world(camera.base_radius as f64, *solo_seed, creature_registry);
        *solo_fields = solo_fields_of(*solo_seed);
        *creatures_seed = *solo_seed;
        if was_hosting && *current_seed == *solo_seed {
            // Même planète : le joueur reste où il est, avec les blocs des autres
            *chunk_cache = herocraft_voxel_engine::WorldChunkCache::new(storage, 256);
            ui_state.player = player_stats::PlayerStats::load_or_default(Some(progress_file(*solo_seed)));
            // La voie vient de la légende, que la progression n'enregistre pas
            let legend = ui_state.legend.take();
            ui_state.set_legend(legend);
            camera.set_terrain(edits.clone());
            lod_manager.set_terrain_edits(edits);
            // Le serveur a enregistré son horloge dans le monde solo
            let clock = herocraft_world::SkyClock::load(solo_save_dir(*solo_seed));
            state.simulated_time = clock.time as f32;
            client_weather.model.forced = clock.forced;
        } else {
            *current_seed = *solo_seed;
            switch_planet(*current_seed, false, edits, storage, generator, chunk_cache, lod_manager, camera, state, ui_state, client_weather);
            dropped_items.clear();
        }
        block_interaction.invalidate();
        ui_state.multiplayer.status = NetStatus::Solo;
        window.set_title(WINDOW_TITLE);
        info!("Retour à la partie solo (planète #{}).", current_seed);
    }

    /// Nouvelle planète solo : celle demandée par `/planet <graine>`, sinon une graine au hasard
    fn generate_new_planet(&mut self) {
        end_vision_tutorial(&mut self.tutorial, &mut self.ui_state, &mut self.solo_creatures);
        self.vision = None;
        self.save_solo_world();
        self.current_seed = self.next_planet_seed.take().unwrap_or_else(herocraft_world::random_seed);
        let (storage, edits) = solo_terrain(self.current_seed);
        let Self { current_seed, generator, chunk_cache, lod_manager, camera, state, ui_state, client_weather, dropped_items, .. } = self;
        switch_planet(*current_seed, false, edits, storage, generator, chunk_cache, lod_manager, camera, state, ui_state, client_weather);
        dropped_items.clear();
        info!("Nouvelle planète générée ! (Graine: #{})", current_seed);
    }
}

//! Ce qui vit et se dessine dans le monde : créatures, habitants, héros, effets des compétences,
//! Pierres du Néant et tutoriel de la vision

use super::frame::announce_solo_crimes;
use super::*;

/// Une bête dessinée à la place d'un héros changé en bête (formes du Druide) : identifiant de
/// dessin, espèce, pieds, cap et attaque en cours
type BeastShape = (herocraft_common::CreatureId, String, glam::Vec3, f32, bool);

impl App {
    /// Créatures : celles que le serveur montre en ligne, sinon celles de la partie solo, en pause
    /// avec le menu. Dessinées d'après le modèle de leur espèce. Rend si des blocs ont changé (les
    /// plantes poussent) et le nombre de sommets dessinés.
    pub(super) fn update_creatures(&mut self, dt: f32, looked_at: Option<herocraft_common::CreatureId>) -> (bool, usize) {
        herocraft_common::prof_scope!("creatures");
        let section = Instant::now();
        let terrain_edited = self.simulate_creatures(dt);
        self.probe.section("créatures : simulation", section);

        let section = Instant::now();
        let block = herocraft_world::BlockGeometry::new(self.camera.base_radius as f64).block_height as f32;
        let anim_dt = if self.ui_state.is_settings_open { 0.0 } else { dt };
        let (mut frame, local_form) = self.build_beasts(dt, anim_dt, block);
        self.push_villagers(&mut frame, anim_dt, block, looked_at);
        self.tick_skills(anim_dt);
        let hero_anchors = self.push_heroes(&mut frame, anim_dt, block, local_form.is_none());
        self.update_skill_effects(anim_dt, block, hero_anchors);
        self.update_void_sites_and_tutorial(anim_dt, block);
        self.state.update_effects(&self.effect_quads);
        self.state.set_creature_frame(&frame);
        let creature_vertices = frame.vertices.len() + frame.skinned_vertices as usize;
        self.probe.section("créatures : animation et maillage", section);
        (terrain_edited, creature_vertices)
    }

    /// Les créatures de l'image : reçues du serveur en ligne ; en solo, simulées ici (plantes
    /// semées, morsures et charges, tirs, œufs). Rend vrai si des blocs ont changé.
    fn simulate_creatures(&mut self, dt: f32) -> bool {
        let Self {
            bench,
            creature_views,
            net,
            creature_registry,
            creatures_seed,
            current_seed,
            solo_creatures,
            solo_fields,
            camera,
            ui_state,
            state,
            client_weather,
            generator,
            lod_manager,
            block_interaction,
            skill_play,
            audio,
            dropped_items,
            ..
        } = self;
        let mut terrain_edited = false;
        let creature_mode = bench.as_ref().map_or(benchmark::CreatureMode::Full, |b| b.creature_mode());
        *creature_views = match net.as_ref() {
            Some(client) => client.creature_views(creature_registry),
            None => {
                if *creatures_seed != *current_seed {
                    *solo_creatures = solo_creature_world(camera.base_radius as f64, *current_seed, creature_registry);
                    *solo_fields = solo_fields_of(*current_seed);
                    *creatures_seed = *current_seed;
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
                    for (pos, block) in solo_fields.tick_in_weather(
                        dt,
                        generator,
                        lod_manager.terrain_edits(),
                        &client_weather.model,
                        state.simulated_time as f64,
                    ) {
                        terrain_edited |= lod_manager.set_block(generator, pos, block);
                        block_interaction.invalidate();
                    }
                    // Morsures et charges des bêtes agressives
                    // Le temps figé de la vision ralentit aussi les bêtes
                    let hits =
                        solo_creatures.tick_with_presences(dt * state.time_scale(), generator, lod_manager.terrain_edits(), &[player]);
                    // Banc d'essai : les bêtes ne blessent pas le joueur
                    for hit in hits.into_iter().filter(|_| bench.is_none()) {
                        // Une garde levée pare le coup qui arrive de face, et riposte
                        let (_, facing, _, _) = camera.look_vectors();
                        let (damage, parried) = skill_play.solo_parry(
                            solo_creatures,
                            camera.player_pos,
                            facing,
                            &mut ui_state.player.stats,
                            hit.creature_id,
                            hit.damage,
                            hit.kind,
                        );
                        // Ralenti, étourdi, repoussé : le coup de certaines bêtes le laisse sonné
                        if !ui_state.player.is_dead() && !parried {
                            let ignores = skill_play.solo_immunities();
                            solo_creatures.afflict(&hit, &mut ui_state.player.stats, &ignores);
                            if let Some((direction, speed)) = solo_creatures
                                .knockback(&hit, camera.player_pos)
                                .filter(|_| !ignores.contains(&herocraft_common::skills::Status::Knockback))
                            {
                                camera.push(direction, speed);
                            }
                        }
                        let cause = creature_registry.get(hit.species).map_or("Une créature", |s| s.name.as_str());
                        // L'armure portée, puis les renforcements de défense
                        let damage = herocraft_common::through_armor(damage, hit.kind, ui_state.inventory.protection() as f32)
                            * skill_play.solo_damage_factor(hit.kind);
                        let hurt = ui_state.player.bitten(damage, Some(hit.kind), cause);
                        // Tombé sous un effet à la mort : la nova part, il se relève
                        if ui_state.player.is_dead() {
                            skill_play.solo_last_stand(solo_creatures, camera.player_pos, &mut ui_state.player.stats);
                        }
                        show_wound(hurt, cause, camera, ui_state, audio);
                    }
                    // Tirs des tourelles, du bastion et de la liche
                    let shots = solo_creatures.take_shots();
                    if !shots.is_empty() {
                        skill_play.on_shots(&shots, creature_registry);
                    }
                    let projectiles = solo_creatures.take_projectile_events();
                    if !projectiles.is_empty() {
                        skill_play.on_projectiles(&projectiles, creature_registry);
                    }
                    // Œufs pondus par les bêtes apprivoisées, laissés à leurs pieds
                    for (feet, item) in solo_creatures.take_laid() {
                        lay_egg(dropped_items, camera, feet, item);
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
            *view = skill_models::drawn_as(view, creature_registry);
        }
        terrain_edited
    }

    /// Les bêtes d'après leur modèle, avec celles des héros changés en bête (formes du Druide).
    /// Rend l'image des créatures à compléter, et la forme de bête du joueur s'il en a pris une.
    fn build_beasts(&mut self, dt: f32, anim_dt: f32, block: f32) -> (creatures::CreatureFrame, Option<String>) {
        let Self {
            creature_views,
            creature_registry,
            skill_play,
            net,
            camera,
            ui_state,
            block_interaction,
            form_feet,
            creature_animator,
            creature_models,
            state,
            ..
        } = self;
        // Les habitants et les voyageurs se dessinent avec le corps des héros (`push_villagers`)
        let mut beasts: Vec<creatures::CreatureView> =
            creature_views.iter().filter(|v| !villagers::is_humanoid(v, creature_registry)).copied().collect();
        // Héros changés en bête (formes du Druide) : la bête à leur place
        skill_play.me = net.as_ref().map_or(skill_play::SOLO_PLAYER, |c| c.player_id());
        let third_person = camera.view_mode() == camera::CameraViewMode::ThirdPerson;
        let local_form = skill_play.form_of(None).filter(|_| ui_state.legend.is_some());
        // Changé en bête : les clics lancent ses attaques, l'inventaire se ferme
        let attacks = skill_play
            .form_skill(None)
            .and_then(|s| herocraft_common::skills().get(&s))
            .and_then(|d| <[String; 2]>::try_from(d.form_attacks.clone()).ok());
        if attacks.is_some() && ui_state.beast_attacks.is_none() {
            ui_state.set_inventory_open(false);
            block_interaction.reset();
        }
        ui_state.beast_attacks = attacks;
        let mut shapes: Vec<BeastShape> = Vec::new();
        if let Some(form) = local_form.clone().filter(|_| third_person) {
            shapes.push((
                herocraft_common::CreatureId(u64::MAX),
                form,
                camera.player_pos,
                camera.player_yaw,
                skill_play.form_attacking(None),
            ));
        }
        if let Some(client) = net.as_ref() {
            for (id, feet, yaw, _) in client.remote_heroes() {
                if let Some(form) = skill_play.form_of(Some(id)) {
                    shapes.push((herocraft_common::CreatureId(u64::MAX - 1 - id.0), form, feet, yaw, skill_play.form_attacking(Some(id))));
                }
            }
        }
        form_feet.retain(|id, _| shapes.iter().any(|s| s.0 == *id));
        for (id, form, feet, yaw, attacking) in &shapes {
            let speed = form_feet.insert(*id, *feet).map_or(0.0, |last| last.distance(*feet) / block / dt.max(1e-3));
            let action = if *attacking {
                herocraft_common::CreatureAction::Attaque
            } else if speed > 4.0 {
                herocraft_common::CreatureAction::Course
            } else if speed > 0.3 {
                herocraft_common::CreatureAction::Marche
            } else {
                herocraft_common::CreatureAction::Repos
            };
            beasts.push(creatures::CreatureView {
                id: *id,
                species: creature_registry.find(form),
                feet: *feet,
                yaw: *yaw,
                hurt: 0.0,
                death: 0.0,
                action,
                scale: 1.0,
                mood: 0,
                summoner: None,
                copy: None,
                tint: None,
            });
        }
        creature_animator.update(&beasts, creature_registry, creature_models, anim_dt, block);
        let frame = creatures::build_frame(
            &beasts,
            creature_registry,
            creature_models,
            creature_animator,
            block,
            if state.gpu_skinning() { creatures::Skinning::Gpu } else { creatures::Skinning::Cpu },
            Some(camera.eye_position()),
        );
        (frame, local_form)
    }

    /// Les habitants, les gardes et les voyageurs avec le corps des héros, et les noms à afficher
    /// au-dessus des têtes
    fn push_villagers(
        &mut self,
        frame: &mut creatures::CreatureFrame,
        anim_dt: f32,
        block: f32,
        looked_at: Option<herocraft_common::CreatureId>,
    ) {
        let Self { villager_animator, creature_views, creature_registry, state, net, solo_creatures, ui_state, camera, .. } = self;
        villager_animator.update(creature_views, creature_registry, anim_dt, block);
        state.begin_villagers();
        let card_of = |id: herocraft_common::CreatureId| match net.as_ref() {
            Some(client) => client.villager_card(id).cloned(),
            None => solo_creatures.villager_card(id).cloned(),
        };
        // Un leurre copie le héros de son lanceur, le nôtre ou celui d'un autre joueur
        let me = net.as_ref().map_or(skill_play::SOLO_PLAYER, |c| c.player_id());
        let my_look = ui_state.legend.as_ref().map(|l| l.appearance.clone());
        let hero_of = |id: herocraft_common::PlayerId| {
            if id == me { my_look.clone() } else { net.as_ref().and_then(|c| c.remote_appearance(id)).cloned() }
        };
        villagers::push_villagers(
            frame,
            creature_views,
            creature_registry,
            card_of,
            &hero_of,
            villager_animator,
            |a| state.villager_texture(a),
            block,
        );
        let mut tags = net.as_ref().map(|c| c.name_tags(camera, state.size.width as f32, state.size.height as f32)).unwrap_or_default();
        tags.extend(villagers::name_tags(creature_views, card_of, looked_at, camera, state.size.width as f32, state.size.height as f32));
        ui_state.name_tags = tags;
        // Les échanges se ferment quand l'habitant s'éloigne (ou disparaît)
        if let Some(trade) = ui_state.trade.as_ref() {
            let near = villagers::feet_of(creature_views, trade.creature_id).is_some_and(|f| f.distance(camera.player_pos) < 8.0 * block);
            if !near {
                ui_state.trade = None;
            }
        }
    }

    /// Compétences : en solo les règles tournent ici, sur nos créatures
    fn tick_skills(&mut self, anim_dt: f32) {
        let Self { net, ui_state, solo_creatures, camera, skill_effects, skill_play, .. } = self;
        let solo_skills =
            (net.is_none() && !ui_state.is_settings_open).then_some((&mut *solo_creatures, camera.player_pos, &mut ui_state.player.stats));
        skill_effects.start(skill_play.tick(anim_dt, solo_skills));
        for id in skill_play.take_ended_projectiles() {
            skill_effects.end_projectile(id);
        }
        if skill_play.take_solo_death() {
            ui_state.player.bled_out();
        }
        if net.is_none() {
            announce_solo_crimes(solo_creatures, ui_state);
        }
    }

    /// Le héros vu de dehors à la troisième personne (s'il n'a pas pris de forme de bête), les
    /// héros des autres joueurs, et à la première personne les bras le temps d'un geste. Rend les
    /// points du héros où s'accrochent les effets.
    fn push_heroes(
        &mut self,
        frame: &mut creatures::CreatureFrame,
        anim_dt: f32,
        block: f32,
        without_beast_form: bool,
    ) -> hero_model::HeroAnchors {
        let Self { hero_gait, camera, ui_state, state, hero_figure, skill_play, net, remote_gaits, villager_animator, .. } = self;
        // Le héros, vu de dehors à la troisième personne, avec le geste de sa compétence
        hero_gait.update(camera.player_pos, anim_dt, block);
        let mut hero_anchors = hero_model::HeroAnchors::standing(camera.player_pos, camera.player_yaw, block);
        if let Some(legend) =
            ui_state.legend.as_ref().filter(|_| camera.view_mode() == camera::CameraViewMode::ThirdPerson && without_beast_form)
        {
            state.set_hero_sheet(&legend.appearance);
            let figure = hero_figure.get(&legend.appearance);
            let pose = skill_play.hero_pose(hero_gait.swing(), figure.units_per_block(&legend.appearance));
            hero_model::push_hero_posed(frame, &legend.appearance, figure, camera.player_pos, camera.player_yaw, &pose, block);
            // Transformations partielles du Druide : patte, fourrure, pattes de cerf, plumes
            for graft in skill_play.hero_grafts().iter().filter_map(|g| skill_models::graft(g)) {
                let slot = state.villager_texture(creature_render::SheetSource::Fixed(&graft.id, &graft.texture));
                skill_models::push_graft(
                    frame,
                    &graft.model,
                    &legend.appearance,
                    figure,
                    camera.player_pos,
                    camera.player_yaw,
                    &pose,
                    slot,
                    block,
                );
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
                hero_model::push_figure(frame, appearance, figure, feet, yaw, &pose, slot, block);
                for graft in skill_play.grafts_of(Some(id)).iter().filter_map(|g| skill_models::graft(g)) {
                    let slot = state.villager_texture(creature_render::SheetSource::Fixed(&graft.id, &graft.texture));
                    skill_models::push_graft(frame, &graft.model, appearance, figure, feet, yaw, &pose, slot, block);
                }
            }
        }
        // À la première personne, le temps d'un geste : les bras et l'arme devant l'œil
        if let Some(legend) = ui_state
            .legend
            .as_ref()
            .filter(|_| camera.view_mode() == camera::CameraViewMode::FirstPerson && skill_play.local_play().is_some())
        {
            state.set_hero_sheet(&legend.appearance);
            let figure = hero_figure.get(&legend.appearance);
            let pose = skill_play.hero_pose(0.0, figure.units_per_block(&legend.appearance));
            let held = ui_state.inventory.slot(ui_state.inventory.selected()).map(|s| s.item);
            // Un objet tenu (pas un bloc) fait l'arme ; sa couleur d'acier en attendant les vraies armes
            let weapon = held.filter(|item| item.block().is_none()).map(|_| [0.72, 0.74, 0.78, 1.0]);
            let (_, _, _, look) = camera.look_vectors();
            let (right, left) =
                hero_model::push_first_person_arms(frame, &legend.appearance, figure, camera.eye_position(), look, &pose, weapon, block);
            hero_anchors.right_hand = right;
            hero_anchors.left_hand = left;
        }
        hero_anchors
    }

    /// Les effets des compétences, accrochés au héros, aux joueurs et aux créatures
    fn update_skill_effects(&mut self, anim_dt: f32, block: f32, hero_anchors: hero_model::HeroAnchors) {
        let Self { effect_quads, skill_effects, camera, net, creature_views, skill_play, .. } = self;
        effect_quads.clear();
        if !skill_effects.is_empty() {
            let up = camera.player_pos.normalize_or_zero();
            let (east, north) = herocraft_common::tangent_frame(up);
            let hero_forward = north * camera.player_yaw.cos() + east * camera.player_yaw.sin();
            let players = |id: herocraft_common::PlayerId| net.as_ref().and_then(|c| c.remote_player_feet(id)).map(|f| (f, 0.0));
            let views = &*creature_views;
            let creature_feet = |id: herocraft_common::CreatureId| views.iter().find(|v| v.id == id).map(|v| v.feet);
            let shots = &*skill_play;
            let projectile_at = |id: herocraft_common::ProjectileId| shots.projectile_at(id);
            let world = effects::AnchorWorld {
                hero: hero_anchors,
                hero_forward,
                players: &players,
                creatures: &creature_feet,
                projectiles: &projectile_at,
                eye: camera.eye_position(),
                block,
            };
            skill_effects.update(anim_dt, &world, effect_quads);
        }
    }

    /// Pierres du Néant posées près du joueur, puis le tutoriel de la vision : ses étapes, sa
    /// consigne, le murmure et la lueur de l'éclat, la charge et l'assaut
    fn update_void_sites_and_tutorial(&mut self, anim_dt: f32, block: f32) {
        let Self {
            camera,
            void_sites,
            lod_manager,
            game_start,
            effect_quads,
            ui_state,
            tutorial,
            cinematics,
            state,
            generator,
            block_interaction,
            audio,
            solo_creatures,
            skill_effects,
            ..
        } = self;
        let geometry = herocraft_world::BlockGeometry::new(camera.base_radius as f64);
        void_sites.update(anim_dt, lod_manager.terrain_edits(), &geometry, camera.player_pos);
        void_sites.push_quads(game_start.elapsed().as_secs_f32(), camera.eye_position(), block, effect_quads);
        // Le tutoriel de la vision : ses étapes, sa consigne, le murmure et la lueur de l'éclat
        ui_state.vision_hint = None;
        match tutorial.as_mut() {
            Some(t) => {
                let outcome = t.update(
                    anim_dt,
                    camera.player_pos,
                    block,
                    cinematics.is_playing(),
                    state.time_frozen,
                    &ui_state.inventory,
                    generator,
                    lod_manager.terrain_edits(),
                );
                if outcome.freeze {
                    state.time_frozen = true;
                }
                let aiming_rock = block_interaction.target().is_some_and(|h| Some(h.block) == t.shard_rock().map(|(p, _)| p));
                if !cinematics.is_playing() {
                    ui_state.vision_hint = t.hint(&ui_state.controls, aiming_rock);
                }
                audio.set_whisper(t.whisper(), &ui_state.audio_settings);
                t.push_quads(camera.eye_position(), block, effect_quads);
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
                apply_charge_events(
                    charge_events,
                    camera,
                    generator,
                    lod_manager,
                    state,
                    solo_creatures,
                    skill_effects,
                    audio,
                    &ui_state.audio_settings,
                    cinematics,
                );
                // Passé la herse : les rues, les trois places, le héraut, la Pierre
                let sealed = t.charge.as_ref().is_some_and(|c| c.phase() == vision_charge::Phase::Sealed);
                if let Some(assault) = t.assault.as_mut().filter(|_| sealed) {
                    let mut assault_events = Vec::new();
                    if assault.phase() == vision_assault::Phase::Waiting {
                        let allies: Vec<herocraft_common::CreatureId> =
                            solo_creatures.creatures().iter().filter(|c| c.is_alive() && soldiers.contains(&c.id)).map(|c| c.id).collect();
                        assault.start(&allies);
                    }
                    let ground = |dir: glam::Vec3| dir * camera.ground_radius_at(dir, generator);
                    assault.update(anim_dt, camera.player_pos, solo_creatures, &ground, &mut assault_events);
                    if ui_state.vision_hint.is_none() && !cinematics.is_playing() {
                        ui_state.vision_hint = assault.hint(solo_creatures);
                    }
                    // Le cristal appelle : le murmure revient, de plus en plus fort près de l'autel
                    if let Some(stone) = assault.calling() {
                        let near = 1.0 - (camera.player_pos.distance(stone) / block / 60.0).clamp(0.0, 1.0);
                        audio.set_whisper(0.25 + 0.75 * near, &ui_state.audio_settings);
                    }
                    apply_assault_events(
                        assault_events,
                        camera,
                        generator,
                        lod_manager,
                        state,
                        skill_effects,
                        audio,
                        &ui_state.audio_settings,
                        cinematics,
                    );
                }
            }
            None => audio.set_whisper(0.0, &ui_state.audio_settings),
        }
    }
}

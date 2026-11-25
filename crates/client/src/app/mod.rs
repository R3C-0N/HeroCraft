//! L'application du client : tout l'état du jeu d'une partie, et la réponse aux événements de la
//! fenêtre.
//!
//! - `mod.rs` : l'état (`App`), sa mise en place au lancement et l'aiguillage des événements ;
//! - `window_input` : souris, molette, clavier et fermeture de la fenêtre ;
//! - `frame` : l'image, étape par étape (horloge, caméra, météo, visée, réseau, interface, rendu) ;
//! - `scene` : les créatures, les héros, les effets des compétences et le tutoriel de la vision ;
//! - `actions` : les actions des boutons de l'interface et des raccourcis (`UiAction`).
//!
//! Les fonctions d'aide de `main.rs` (sauvegardes, changements de planète ou d'époque, combats,
//! commandes du chat) restent à la racine de la crate, partagées par ces modules.

mod actions;
mod frame;
mod scene;
mod window_input;

use super::*;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::keyboard::Key;

/// Tout l'état du jeu tenu par la boucle d'événements
pub(crate) struct App {
    window: Arc<Window>,
    /// Graine de la planète affichée (en ligne, celle du serveur)
    current_seed: u64,
    generator: herocraft_world::PlanetWorldGenerator,
    chunk_cache: herocraft_voxel_engine::WorldChunkCache,
    camera: OrbitCamera,
    lod_manager: PlanetLodManager,
    ui_state: UiState,
    /// Son : bruitages du joueur et des blocs, ambiance (muet sans carte son)
    audio: audio::Audio,
    /// Le ciel au-dessus du joueur (grotte ou plein air) n'est relu que de temps en temps
    sky_check_in: f32,
    sky_covered: bool,

    // Partie en ligne : connexion en cours, connexion établie, partie ouverte au réseau local par
    // ce jeu, recherche des parties du réseau local (page Multijoueur ouverte)
    pending_join: Option<PendingJoin>,
    net: Option<NetClient>,
    /// Vision de temple en cours : les blocs modifiés du présent, mis de côté, et en solo l'endroit
    /// où le joueur revient
    vision: Option<PresentWorld>,
    hosted: Option<HostedServer>,
    lan_browser: Option<LanBrowser>,
    lan_retry_at: Instant,
    /// Planète de la partie solo, retrouvée en quittant une partie en ligne
    solo_seed: u64,

    /// Espèces des créatures, décrites dans assets/creatures/
    creature_registry: Arc<herocraft_common::CreatureRegistry>,
    /// Créatures de la partie solo, simulées ici (en ligne, c'est le serveur qui les fait vivre).
    /// La population de chaque zone est tirée une fois pour toutes et gardée dans la sauvegarde du
    /// monde.
    solo_creatures: herocraft_world::CreatureWorld,
    /// Plantes semées de la partie solo, qui poussent ici (en ligne, sur le serveur)
    solo_fields: herocraft_world::Fields,
    /// Graine demandée par `/planet <graine>` pour la prochaine planète
    next_planet_seed: Option<u64>,
    /// Planète où vivent `solo_creatures` et `solo_fields`
    creatures_seed: u64,
    creature_animator: creatures::CreatureAnimator,
    villager_animator: villagers::VillagerAnimator,
    hero_gait: hero_model::HeroGait,
    /// Démarche des héros des autres joueurs
    remote_gaits: std::collections::HashMap<herocraft_common::PlayerId, hero_model::HeroGait>,
    /// Pieds des héros changés en bête (formes du Druide) à l'image d'avant, pour leur allure
    form_feet: std::collections::HashMap<herocraft_common::CreatureId, glam::Vec3>,
    hero_figure: hero_model::FigureCache,
    /// Compétences : lancements, recharges, gestes du héros et frappes à montrer
    skill_play: skill_play::SkillPlay,
    skill_effects: effects::Effects,
    /// Cinématiques (assets/cinematics/) : déclenchées par les événements du jeu, ou par /cinematic
    cinematics: cinematic::Cinematics,
    launch_announced: bool,
    effect_quads: Vec<effects::EffectQuad>,
    creature_models: creatures::CreatureModels,
    /// Créatures dessinées à la dernière image, que le réticule peut viser
    creature_views: Vec<creatures::CreatureView>,
    /// Humanoïde visé quand le clic gauche a été enfoncé : seul lui se frappe tant que le clic est tenu
    strike_lock: Option<herocraft_common::CreatureId>,
    /// En solo, combien de fois on a parlé à chaque habitant (il enchaîne ses répliques)
    solo_talks: std::collections::HashMap<herocraft_common::CreatureId, usize>,

    /// Moteur WGPU
    state: State,
    /// Chronométrage des étapes de chaque image (à-coups, banc d'essai)
    probe: benchmark::FrameProbe,
    last_hitch_log: Option<Instant>,
    /// Banc d'essai (`--benchmark`) : le jeu joue seul une suite de phases et mesure chacune
    bench: Option<benchmark::Benchmark>,
    /// Météo et saisons
    client_weather: weather::ClientWeather,
    column_tops: weather::ColumnTops,

    // Gestion des entrées : souris capturée en jeu, glisser en vue satellite
    cursor_captured: bool,
    window_focused: bool,
    modifiers: ModifiersState,
    zoom_modifier_held: bool,
    /// Molette du pavé tactile (en pixels) cumulée jusqu'à un cran de la barre d'objets
    hotbar_scroll: f32,
    jump_tap: input::DoubleTap,
    /// Actions du clavier rejouées comme les boutons du HUD, au moment du rendu
    pending_actions: Vec<UiAction>,
    is_dragging: bool,
    /// Destruction (clic gauche) et pose (clic droit) du bloc visé par le réticule
    block_interaction: BlockInteraction,
    /// Blocs cassés tombés au sol, ramassés en passant à proximité
    dropped_items: dropped_items::DroppedItems,
    /// Pierres du Néant obtenues, touchées ou absorbées dans l'image (voir `void_stones::dispatch`)
    stone_events: herocraft_common::StoneEvents,
    /// La dernière Pierre entière obtenue, qu'absorbe la fin de sa vision (`vision_ended`)
    vision_stone: Option<herocraft_common::ItemId>,
    /// Le tutoriel de la vision (« L'éclat ») : le soldat, la roche à l'éclat et ses consignes
    tutorial: Option<vision_tutorial::VisionTutorial>,
    /// Fichier de commandes de test, lu à chaque image (`HEROCRAFT_COMMANDES`)
    command_file: Option<std::path::PathBuf>,
    /// Pierres du Néant posées près du joueur : leur fenêtre sur le néant et leurs poussières
    void_sites: void_stones::VoidStoneSites,
    game_start: Instant,
    last_autosave: Instant,
    last_mouse_pos: (f64, f64),
    last_frame_time: Instant,
    fps_timer: Instant,
    fps_frame_count: u32,
}

impl App {
    /// Prépare la partie dans la fenêtre ouverte : planète, caméra, interface, son, créatures,
    /// moteur de rendu
    pub(crate) fn new(window: Arc<Window>) -> Self {
        // Initialisation du générateur procédural planétaire : graine imposée au lancement, sinon une
        // nouvelle planète à chaque partie
        let launch = network::launch_options();
        // Banc d'essai : toujours la même planète, sauf graine imposée
        let current_seed =
            launch.seed.or(benchmark::requested().then_some(benchmark::DEFAULT_SEED)).unwrap_or_else(herocraft_world::random_seed);
        info!("Planète de graine #{current_seed} (relancer avec --seed {current_seed} pour la retrouver).");
        let planet_desc = herocraft_world::PlanetDescriptor { seed: current_seed, ..Default::default() };

        info!("Initialisation de la planète et du gestionnaire QuadTree...");
        let generator = herocraft_world::PlanetWorldGenerator::new(planet_desc);

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
        lod_manager.update(camera.lod_eye(), &generator, &mut chunk_cache, 0.0, None);
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
        ui_state.skill_bar = skill_bar::SkillBar::load_or_default(skill_bar::SKILL_BAR_FILE);
        ui_state.inventory = inventory::Inventory::load_or_default(inventory_file(current_seed, false));
        ui_state.player = player_stats::PlayerStats::load_or_default(Some(progress_file(current_seed)));
        ui_state.set_legend(load_legend(&legend_file(current_seed, false)));
        camera.hero_height = hero_height(&ui_state);
        ui_state.audio_settings = audio::AudioSettings::load_or_default(audio::AUDIO_FILE);
        let audio = audio::Audio::new();

        ui_state.multiplayer.pseudo = launch.pseudo.clone();
        let pending_join = launch.server.map(|addr| {
            info!("Connexion au serveur {} en tant que « {} »...", addr, launch.pseudo);
            ui_state.multiplayer.status = NetStatus::Connecting(addr.clone());
            PendingJoin::start(addr, launch.pseudo.clone())
        });
        let creature_registry = Arc::new(herocraft_common::CreatureRegistry::load_default());
        let mut solo_creatures = solo_creature_world(base_radius as f64, current_seed, &creature_registry);
        let solo_fields = solo_fields_of(current_seed);
        // Chat : Tab complète les blocs et objets (`/give`) et les créatures (`/summon`)
        ui_state.chat.items = commands_items();
        ui_state.chat.creatures = creature_registry.iter().map(|s| s.id.clone()).collect();
        ui_state.chat.info(format!("Press [{}] to chat, /help for the commands", ui_state.controls.action_key_label(Action::OpenChat)));
        let cinematics = cinematic::Cinematics::load();
        ui_state.chat.cinematics = cinematics.player.catalog.ids();
        let (creature_models, creature_textures) = creatures::CreatureModels::load(&creature_registry);

        // Initialisation du moteur WGPU
        let mut state = pollster::block_on(State::new(window.clone(), &ui_state));
        // Puce intégrée : ombres et rayons allégés, MSAA coupé (réglables dans les options)
        ui_state.adapt_to_gpu(state.integrated_gpu());
        state.set_creature_textures(&creature_textures);
        state.set_creature_models(&creature_models);
        state.sync_lod_meshes(&lod_manager);

        let bench = benchmark::requested().then(|| benchmark::Benchmark::new(&camera, current_seed));
        // Météo et saisons : l'horloge du ciel du monde solo reprend là où elle en était
        let mut client_weather = weather::ClientWeather::new(current_seed);
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

        keyboard_hook::install(&window);

        Self {
            window,
            current_seed,
            generator,
            chunk_cache,
            camera,
            lod_manager,
            ui_state,
            audio,
            sky_check_in: 0.0,
            sky_covered: false,
            pending_join,
            net: None,
            vision: None,
            hosted: None,
            lan_browser: None,
            lan_retry_at: Instant::now(),
            solo_seed: current_seed,
            creature_registry,
            solo_creatures,
            solo_fields,
            next_planet_seed: None,
            creatures_seed: current_seed,
            creature_animator: creatures::CreatureAnimator::default(),
            villager_animator: villagers::VillagerAnimator::default(),
            hero_gait: hero_model::HeroGait::default(),
            remote_gaits: std::collections::HashMap::new(),
            form_feet: std::collections::HashMap::new(),
            hero_figure: hero_model::FigureCache::default(),
            skill_play: skill_play::SkillPlay::default(),
            skill_effects: effects::Effects::default(),
            cinematics,
            launch_announced: false,
            effect_quads: Vec::new(),
            creature_models,
            creature_views: Vec::new(),
            strike_lock: None,
            solo_talks: std::collections::HashMap::new(),
            state,
            probe: benchmark::FrameProbe::default(),
            last_hitch_log: None,
            bench,
            client_weather,
            column_tops: weather::ColumnTops::default(),
            cursor_captured: false,
            window_focused: true,
            modifiers: ModifiersState::empty(),
            zoom_modifier_held: false,
            hotbar_scroll: 0.0,
            jump_tap: input::DoubleTap::default(),
            pending_actions: Vec::new(),
            is_dragging: false,
            block_interaction: BlockInteraction::new(),
            dropped_items: dropped_items::DroppedItems::new(),
            stone_events: herocraft_common::StoneEvents::new(),
            vision_stone: None,
            tutorial: None,
            command_file: std::env::var_os("HEROCRAFT_COMMANDES").map(std::path::PathBuf::from),
            void_sites: void_stones::VoidStoneSites::new(),
            game_start: Instant::now(),
            last_autosave: Instant::now(),
            last_mouse_pos: (0.0, 0.0),
            last_frame_time: Instant::now(),
            fps_timer: Instant::now(),
            fps_frame_count: 0,
        }
    }

    /// Un événement de la fenêtre du jeu
    fn window_event(&mut self, event_loop: &EventLoopTarget, event: &WindowEvent) {
        // Hors image : le temps passé à traiter les événements de la fenêtre apparaît au rapport
        let _event_guard = (!matches!(event, WindowEvent::RedrawRequested)).then(|| {
            herocraft_common::prof_scope!("boucle::evenement_fenetre" => guard);
            guard
        });
        // Transmettre les événements de fenêtre à egui
        self.ui_state.handle_event(event);

        match event {
            WindowEvent::CloseRequested => self.close(event_loop),
            WindowEvent::Resized(physical_size) => self.resize(*physical_size, self.window.scale_factor()),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.resize(self.window.inner_size(), *scale_factor),
            WindowEvent::Focused(focused) => self.focus_changed(*focused),
            WindowEvent::ModifiersChanged(new_modifiers) => self.modifiers = new_modifiers.state(),
            WindowEvent::MouseInput { state, button, .. } => self.mouse_input(*state, *button),
            WindowEvent::CursorMoved { position, .. } => self.cursor_moved(*position),
            WindowEvent::MouseWheel { delta, .. } => self.mouse_wheel(*delta),
            WindowEvent::KeyboardInput {
                event: KeyEvent { state, physical_key: PhysicalKey::Code(key_code), logical_key, repeat, .. },
                ..
            } => self.keyboard_input(*state, *key_code, logical_key, *repeat),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    /// Enregistre ce qui doit l'être avant de fermer le jeu : la partie solo (en ligne, la
    /// population est celle du serveur : celle du solo est déjà enregistrée), les blocs, et quitte
    /// le serveur
    fn save_before_exit(&mut self) {
        if self.net.is_none() && self.hosted.is_none() {
            save_solo_creatures(&self.solo_creatures, &self.solo_fields);
            save_sky_clock(self.current_seed, &self.state, &self.client_weather);
        }
        if let Some(client) = self.net.as_mut() {
            client.disconnect();
        }
        // Le serveur ouvert ici prévient ses joueurs et enregistre le monde
        self.hosted = None;
        let _ = self.chunk_cache.flush_dirty();
        save_terrain_edits(&self.lod_manager);
    }

    /// Met à jour la sauvegarde du monde solo : blocs, créatures et plantes, horloge du ciel
    fn save_solo_world(&mut self) {
        let _ = self.chunk_cache.flush_dirty();
        save_terrain_edits(&self.lod_manager);
        save_solo_creatures(&self.solo_creatures, &self.solo_fields);
        save_sky_clock(self.current_seed, &self.state, &self.client_weather);
    }
}

/// La boucle d'événements telle que la voient les gestionnaires (pour quitter le jeu)
pub(crate) type EventLoopTarget = ActiveEventLoop;

/// L'application vue par winit : la partie (`App`) n'existe qu'une fois la fenêtre ouverte, à la
/// première reprise de l'application
struct Launcher {
    /// Écran demandé pour la fenêtre (`HEROCRAFT_MONITOR`)
    wanted_monitor: Option<String>,
    app: Option<App>,
}

impl ApplicationHandler for Launcher {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_none() {
            let window = create_window(event_loop, self.wanted_monitor.as_deref());
            self.app = Some(App::new(Arc::new(window)));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if let Some(app) = self.app.as_mut().filter(|app| app.window.id() == window_id) {
            app.window_event(event_loop, &event);
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent) {
        // Mouvement brut de la souris : oriente le regard quand le curseur est capturé
        if let (Some(app), DeviceEvent::MouseMotion { delta }) = (self.app.as_mut(), event) {
            app.mouse_motion(delta);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_ref() {
            app.window.request_redraw();
        }
    }
}

/// Fait tourner la boucle d'événements de winit jusqu'à la fermeture du jeu : la fenêtre s'ouvre
/// (sur l'écran `wanted_monitor` s'il est demandé) et la partie se prépare au démarrage
pub(crate) fn run(event_loop: EventLoop<()>, wanted_monitor: Option<String>) -> anyhow::Result<()> {
    // Le jeu dessine en continu
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut launcher = Launcher { wanted_monitor, app: None };
    anyhow::Context::context(event_loop.run_app(&mut launcher), "Erreur dans la boucle d'événements")
}

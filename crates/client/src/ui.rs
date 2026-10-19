use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::window::Window;

/// Actions déclenchées par l'interface utilisateur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    GenerateNewPlanet,
    ToggleGroundCamera,
    ToggleAutoRotate,
    ResetCamera,
    ToggleTectonics,
    ToggleOceanCurrents,
    // Actions Menu Paramètres Vidéo
    ToggleVsync,
    CycleLodDistance,
    ToggleFullscreen,
    CycleFpsLimit,
    ResumeGame,
    QuitGame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodDistanceSetting {
    Courte,
    Normale,
    Elevee,
}

impl LodDistanceSetting {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Courte => "Courte (Rapide)",
            Self::Normale => "Normale (Équilibrée)",
            Self::Elevee => "Élevée (Haute Fidélité)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsLimitSetting {
    Fps60,
    Fps120,
    Unlimited,
}

impl FpsLimitSetting {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Fps60 => "60 FPS",
            Self::Fps120 => "120 FPS",
            Self::Unlimited => "Illimité",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSettings {
    pub vsync: bool,
    pub fullscreen: bool,
    pub lod_distance: LodDistanceSetting,
    pub fps_limit: FpsLimitSetting,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            fullscreen: false,
            lod_distance: LodDistanceSetting::Normale,
            fps_limit: FpsLimitSetting::Fps60,
        }
    }
}

/// Réglages d'éclairage et d'ambiance céleste ajustables via sliders
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingSettings {
    pub moon_intensity: f32,     // Multiplicateur lumière de Lune (défaut: 1.0, [0.0 .. 3.0])
    pub sun_intensity: f32,      // Multiplicateur lumière du Soleil (défaut: 1.0, [0.0 .. 3.0])
    pub night_ambient: f32,      // Multiplicateur obscurité nocturne / étoiles (défaut: 1.0, [0.0 .. 3.0])
    pub day_ambient: f32,        // Multiplicateur clarté ambiante diurne (défaut: 1.0, [0.0 .. 3.0])
    pub moon_speed: f32,         // Vitesse orbitale de la Lune (défaut: 1.0, [0.0 .. 5.0], 0 = fige la lune)
    pub fog_intensity: f32,      // Multiplicateur brouillard de distance (défaut: 1.0, [0.0 .. 2.0])
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            moon_intensity: 0.25,
            sun_intensity: 1.0,
            night_ambient: 2.10,
            day_ambient: 1.0,
            moon_speed: 1.0,
            fog_intensity: 1.0,
        }
    }
}

pub struct UiState {
    pub ctx: egui::Context,
    pub raw_input: egui::RawInput,
    pub scale_factor: f32,
    pub pointer_pos: egui::Pos2,
    pub current_seed: u64,
    pub auto_rotate: bool,
    pub show_tectonics: bool,
    pub show_ocean_currents: bool,
    pub current_fps: u32,
    pub is_settings_open: bool,
    pub video_settings: VideoSettings,
    pub lighting_settings: LightingSettings,
    pub screen_width: f32,
    pub screen_height: f32,
}

impl UiState {
    pub fn new(initial_seed: u64) -> Self {
        let ctx = egui::Context::default();
        Self::apply_custom_theme(&ctx);

        Self {
            ctx,
            raw_input: egui::RawInput::default(),
            scale_factor: 1.0_f32,
            pointer_pos: egui::Pos2::ZERO,
            current_seed: initial_seed,
            auto_rotate: false,
            show_tectonics: false,
            show_ocean_currents: false,
            current_fps: 60,
            is_settings_open: false,
            video_settings: VideoSettings::default(),
            lighting_settings: LightingSettings::default(),
            screen_width: 1280.0_f32,
            screen_height: 720.0_f32,
        }
    }

    /// Applique un thème personnalisé sombre fantasy/voxel aux teintes obsidienne et or
    pub fn apply_custom_theme(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();

        visuals.window_rounding = egui::Rounding::same(8.0_f32);
        visuals.window_fill = egui::Color32::from_rgba_premultiplied(14, 18, 28, 240);
        visuals.window_stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(212, 175, 55));
        visuals.panel_fill = egui::Color32::from_rgba_premultiplied(10, 14, 22, 210);

        // Boutons inactifs
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgba_premultiplied(24, 32, 46, 220);
        visuals.widgets.inactive.rounding = egui::Rounding::same(5.0_f32);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(55, 70, 95));
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(220, 225, 235));

        // Boutons survolés
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgba_premultiplied(42, 54, 78, 235);
        visuals.widgets.hovered.rounding = egui::Rounding::same(5.0_f32);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(212, 175, 55));
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

        // Boutons cliqués / actifs
        visuals.widgets.active.bg_fill = egui::Color32::from_rgba_premultiplied(65, 85, 125, 245);
        visuals.widgets.active.rounding = egui::Rounding::same(5.0_f32);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 215, 0));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

        // Menu & fenêtres
        visuals.override_text_color = Some(egui::Color32::from_rgb(235, 240, 248));

        let mut style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0_f32, 8.0_f32);
        style.spacing.button_padding = egui::vec2(12.0_f32, 6.0_f32);

        ctx.set_style(style);
    }

    pub fn set_scale_factor(&mut self, factor: f32) {
        self.scale_factor = factor.max(0.1_f32);
    }

    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.current_seed = seed;
    }

    pub fn set_auto_rotate(&mut self, auto_rotate: bool) {
        self.auto_rotate = auto_rotate;
    }

    pub fn set_fps(&mut self, fps: u32) {
        self.current_fps = fps;
    }

    pub fn set_settings_open(&mut self, is_open: bool) {
        self.is_settings_open = is_open;
    }

    pub fn toggle_settings(&mut self) -> bool {
        self.is_settings_open = !self.is_settings_open;
        self.is_settings_open
    }

    pub fn active_filter(&self) -> herocraft_world::VisualizationFilter {
        if self.show_tectonics {
            herocraft_world::VisualizationFilter::Tectonics
        } else if self.show_ocean_currents {
            herocraft_world::VisualizationFilter::OceanCurrents
        } else {
            herocraft_world::VisualizationFilter::Standard
        }
    }

    /// Transmet les événements de fenêtre Winit à egui
    pub fn handle_event(&mut self, _window: &Window, event: &WindowEvent) {
        let ppp = self.scale_factor.max(0.1_f32);

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let pos = egui::pos2((position.x as f32) / ppp, (position.y as f32) / ppp);
                self.pointer_pos = pos;
                self.raw_input.events.push(egui::Event::PointerMoved(pos));
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let egui_btn = match button {
                    MouseButton::Left => Some(egui::PointerButton::Primary),
                    MouseButton::Right => Some(egui::PointerButton::Secondary),
                    MouseButton::Middle => Some(egui::PointerButton::Middle),
                    _ => None,
                };

                if let Some(btn) = egui_btn {
                    let pressed = *state == ElementState::Pressed;
                    self.raw_input.events.push(egui::Event::PointerButton {
                        pos: self.pointer_pos,
                        button: btn,
                        pressed,
                        modifiers: egui::Modifiers::default(),
                    });
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (*x * 20.0_f32, *y * 20.0_f32),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };

                self.raw_input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(dx, dy),
                    modifiers: egui::Modifiers::default(),
                });
            }

            _ => {}
        }
    }

    /// Construit l'interface graphique immédiate pour la frame courante
    pub fn update_ui(&mut self, camera_distance: f32, is_ground: bool) -> (egui::FullOutput, Option<UiAction>) {
        let mut raw_input = std::mem::take(&mut self.raw_input);
        let ppp = self.scale_factor.max(0.1_f32);
        let logical_width = self.screen_width / ppp;
        let logical_height = self.screen_height / ppp;

        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(logical_width, logical_height),
        ));
        self.ctx.set_pixels_per_point(ppp);

        let mut action: Option<UiAction> = None;

        let full_output = self.ctx.run(raw_input, |ctx| {
            // 1. Réticule de visée en mode FPS (quand le menu est fermé)
            if is_ground && !self.is_settings_open {
                let center = ctx.screen_rect().center();
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("crosshair")));
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(180));
                painter.line_segment([center - egui::vec2(6.0_f32, 0.0_f32), center + egui::vec2(6.0_f32, 0.0_f32)], stroke);
                painter.line_segment([center - egui::vec2(0.0_f32, 6.0_f32), center + egui::vec2(0.0_f32, 6.0_f32)], stroke);
                painter.circle_filled(center, 1.2_f32, egui::Color32::from_rgb(255, 215, 0));
            }

            // 2. Panneau HUD supérieur gauche (Planète, actions, filtres)
            egui::Area::new(egui::Id::new("top_left_hud"))
                .fixed_pos(egui::pos2(16.0_f32, 16.0_f32))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_premultiplied(12, 16, 24, 215))
                        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(180, 140, 45)))
                        .rounding(egui::Rounding::same(6.0_f32))
                        .inner_margin(egui::Margin::same(12.0_f32))
                        .show(ui, |ui| {
                            ui.set_width(260.0_f32);

                            // Titre & Graine du monde
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("HEROCRAFT")
                                        .strong()
                                        .color(egui::Color32::from_rgb(240, 200, 60))
                                        .size(16.0_f32),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("#{}", self.current_seed))
                                            .monospace()
                                            .color(egui::Color32::from_rgb(150, 165, 185))
                                            .size(11.0_f32),
                                    );
                                });
                            });

                            ui.add_space(4.0_f32);
                            ui.separator();
                            ui.add_space(4.0_f32);

                            // Boutons d'actions du monde
                            if ui.button("🎲  Nouvelle planète [G]").clicked() {
                                action = Some(UiAction::GenerateNewPlanet);
                            }

                            let view_label = if is_ground { "🛰  Vue Satellite [F]" } else { "🚶  Vue Sol (FPS) [F]" };
                            if ui.button(view_label).clicked() {
                                action = Some(UiAction::ToggleGroundCamera);
                            }

                            let rot_label = if self.auto_rotate { "⏸  Arrêter rotation [Espace]" } else { "▶  Auto-rotation [Espace]" };
                            if ui.button(rot_label).clicked() {
                                action = Some(UiAction::ToggleAutoRotate);
                            }

                            if ui.button("🔄  Réinitialiser vue [R]").clicked() {
                                action = Some(UiAction::ResetCamera);
                            }

                            ui.add_space(4.0_f32);
                            ui.separator();
                            ui.add_space(4.0_f32);

                            // Filtres cartographiques
                            ui.label(
                                egui::RichText::new("Filtres cartographiques :")
                                    .size(12.0_f32)
                                    .color(egui::Color32::from_rgb(180, 185, 200)),
                            );

                            if ui.checkbox(&mut self.show_tectonics, "Plaques tectoniques [T]").changed() {
                                if self.show_tectonics {
                                    self.show_ocean_currents = false;
                                }
                                action = Some(UiAction::ToggleTectonics);
                            }

                            if ui.checkbox(&mut self.show_ocean_currents, "Courants marins [C]").changed() {
                                if self.show_ocean_currents {
                                    self.show_tectonics = false;
                                }
                                action = Some(UiAction::ToggleOceanCurrents);
                            }

                            ui.add_space(4.0_f32);
                            ui.separator();
                            ui.add_space(4.0_f32);

                            // Section interactive d'éclairage et d'ambiance céleste
                            ui.collapsing(
                                egui::RichText::new("🌙  Éclairage & Ambiance")
                                    .size(12.0_f32)
                                    .color(egui::Color32::from_rgb(220, 205, 140))
                                    .strong(),
                                |ui| {
                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Lumière de Lune :").size(11.0_f32).color(egui::Color32::from_rgb(180, 210, 255)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.moon_intensity, 0.0..=3.0).text("Lune").show_value(true));

                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Lumière Solaire :").size(11.0_f32).color(egui::Color32::from_rgb(255, 230, 150)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.sun_intensity, 0.0..=3.0).text("Soleil").show_value(true));

                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Ambiance Nuit (Étoiles) :").size(11.0_f32).color(egui::Color32::from_rgb(140, 160, 220)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.night_ambient, 0.0..=4.0).text("Nuit").show_value(true));

                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Ambiance Jour (Ciel) :").size(11.0_f32).color(egui::Color32::from_rgb(160, 210, 240)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.day_ambient, 0.0..=3.0).text("Jour").show_value(true));

                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Vitesse Céleste (Jour 20m / Lune 21m) :").size(11.0_f32).color(egui::Color32::from_rgb(200, 200, 200)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.moon_speed, 0.0..=5.0).text("Vitesse").show_value(true));

                                    ui.add_space(2.0_f32);
                                    ui.label(egui::RichText::new("Brouillard Atmosphérique :").size(11.0_f32).color(egui::Color32::from_rgb(190, 190, 190)));
                                    ui.add(egui::Slider::new(&mut self.lighting_settings.fog_intensity, 0.0..=2.0).text("Brume").show_value(true));

                                    ui.add_space(4.0_f32);
                                    if ui.button("↺ Réinitialiser l'éclairage").clicked() {
                                        self.lighting_settings = LightingSettings::default();
                                    }
                                },
                            );
                        });
                });

            // 3. Panneau HUD supérieur droit (FPS, altitude, raccourci menu)
            egui::Area::new(egui::Id::new("top_right_hud"))
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(egui::pos2(logical_width - 16.0_f32, 16.0_f32))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_premultiplied(12, 16, 24, 215))
                        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(180, 140, 45)))
                        .rounding(egui::Rounding::same(6.0_f32))
                        .inner_margin(egui::Margin::same(10.0_f32))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let fps_color = if self.current_fps >= 55 {
                                    egui::Color32::from_rgb(80, 220, 100)
                                } else if self.current_fps >= 30 {
                                    egui::Color32::from_rgb(240, 190, 50)
                                } else {
                                    egui::Color32::from_rgb(240, 80, 80)
                                };
                                ui.label(egui::RichText::new(format!("{} FPS", self.current_fps)).strong().color(fps_color));

                                ui.separator();

                                let dist_label = if is_ground {
                                    "Sol (FPS)".to_string()
                                } else {
                                    format!("{:.0} m", camera_distance)
                                };
                                ui.label(egui::RichText::new(dist_label).color(egui::Color32::from_rgb(180, 205, 240)));

                                ui.separator();

                                if ui.button("⚙ Menu [Échap]").clicked() {
                                    self.is_settings_open = !self.is_settings_open;
                                }
                            });
                        });
                });

            // 4. Menu Paramètres Vidéo & Commandes (Modal centré lors de l'appui sur Échap)
            if self.is_settings_open {
                // Fond assombrissant l'arrière-plan 3D
                let screen_rect = ctx.screen_rect();
                ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("backdrop")))
                    .rect_filled(screen_rect, 0.0_f32, egui::Color32::from_rgba_premultiplied(0, 0, 0, 160));

                egui::Window::new("⚙  PARAMÈTRES — HEROCRAFT")
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0_f32, 0.0_f32))
                    .collapsible(false)
                    .resizable(false)
                    .default_width(400.0_f32)
                    .frame(
                        egui::Frame::window(&ctx.style())
                            .fill(egui::Color32::from_rgba_premultiplied(14, 18, 28, 252))
                            .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(212, 175, 55)))
                            .rounding(egui::Rounding::same(8.0_f32))
                            .inner_margin(egui::Margin::same(18.0_f32)),
                    )
                    .show(ctx, |ui| {
                        ui.heading(
                            egui::RichText::new("OPTIONS GRAPHIQUES")
                                .size(14.0_f32)
                                .color(egui::Color32::from_rgb(220, 185, 60)),
                        );
                        ui.add_space(4.0_f32);

                        // Synchronisation verticale
                        let vsync_text = if self.video_settings.vsync { "Activée" } else { "Désactivée" };
                        if ui.button(format!("Synchronisation Verticale (V-Sync) : {}", vsync_text)).clicked() {
                            self.video_settings.vsync = !self.video_settings.vsync;
                            action = Some(UiAction::ToggleVsync);
                        }

                        // Plein écran
                        let fs_text = if self.video_settings.fullscreen { "Plein Écran" } else { "Fenêtré" };
                        if ui.button(format!("Mode d'affichage : {}", fs_text)).clicked() {
                            self.video_settings.fullscreen = !self.video_settings.fullscreen;
                            action = Some(UiAction::ToggleFullscreen);
                        }

                        // Distance LOD
                        if ui.button(format!("Distance LOD : {}", self.video_settings.lod_distance.label())).clicked() {
                            self.video_settings.lod_distance = match self.video_settings.lod_distance {
                                LodDistanceSetting::Courte => LodDistanceSetting::Normale,
                                LodDistanceSetting::Normale => LodDistanceSetting::Elevee,
                                LodDistanceSetting::Elevee => LodDistanceSetting::Courte,
                            };
                            action = Some(UiAction::CycleLodDistance);
                        }

                        // Limite FPS
                        if ui.button(format!("Limite FPS : {}", self.video_settings.fps_limit.label())).clicked() {
                            self.video_settings.fps_limit = match self.video_settings.fps_limit {
                                FpsLimitSetting::Fps60 => FpsLimitSetting::Fps120,
                                FpsLimitSetting::Fps120 => FpsLimitSetting::Unlimited,
                                FpsLimitSetting::Unlimited => FpsLimitSetting::Fps60,
                            };
                            action = Some(UiAction::CycleFpsLimit);
                        }

                        ui.add_space(10.0_f32);
                        ui.separator();
                        ui.add_space(6.0_f32);

                        ui.heading(
                            egui::RichText::new("ÉCLAIRAGE & AMBIANCE DU MONDE")
                                .size(14.0_f32)
                                .color(egui::Color32::from_rgb(220, 185, 60)),
                        );
                        ui.add_space(4.0_f32);

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Lumière de Lune :").size(12.0_f32).color(egui::Color32::from_rgb(180, 210, 255)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.moon_intensity, 0.0..=3.0).show_value(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Lumière du Soleil :").size(12.0_f32).color(egui::Color32::from_rgb(255, 230, 150)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.sun_intensity, 0.0..=3.0).show_value(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Ambiance Nuit (Étoiles) :").size(12.0_f32).color(egui::Color32::from_rgb(140, 160, 220)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.night_ambient, 0.0..=4.0).show_value(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Ambiance Jour (Ciel) :").size(12.0_f32).color(egui::Color32::from_rgb(160, 210, 240)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.day_ambient, 0.0..=3.0).show_value(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Vitesse Céleste (Jour 20m / Lune 21m) :").size(12.0_f32).color(egui::Color32::from_rgb(200, 200, 200)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.moon_speed, 0.0..=5.0).show_value(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Brouillard Atmosphérique :").size(12.0_f32).color(egui::Color32::from_rgb(190, 190, 190)));
                            ui.add(egui::Slider::new(&mut self.lighting_settings.fog_intensity, 0.0..=2.0).show_value(true));
                        });

                        ui.add_space(4.0_f32);
                        if ui.button("↺ Réinitialiser l'éclairage par défaut").clicked() {
                            self.lighting_settings = LightingSettings::default();
                        }

                        ui.add_space(10.0_f32);
                        ui.separator();
                        ui.add_space(6.0_f32);

                        ui.heading(
                            egui::RichText::new("GUIDE DES CONTRÔLES")
                                .size(14.0_f32)
                                .color(egui::Color32::from_rgb(220, 185, 60)),
                        );
                        ui.add_space(4.0_f32);

                        egui::Grid::new("controls_grid")
                            .num_columns(2)
                            .spacing([14.0_f32, 5.0_f32])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("[Z,Q,S,D] / [W,A,S,D]").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Marcher sur les voxels (gravité sphérique)");
                                ui.end_row();

                                ui.label(egui::RichText::new("[SHIFT]").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Courir / Sprinter");
                                ui.end_row();

                                ui.label(egui::RichText::new("[ESPACE]").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Sauter (au sol) / Auto-rotation (orbite)");
                                ui.end_row();

                                ui.label(egui::RichText::new("[F]").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Basculer Vue Sol (FPS) <-> Orbite");
                                ui.end_row();

                                ui.label(egui::RichText::new("Molette Souris").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Zoom continu FPS -> TPS -> Orbite");
                                ui.end_row();

                                ui.label(egui::RichText::new("Clic Droit / Gauche").monospace().color(egui::Color32::from_rgb(200, 215, 235)));
                                ui.label("Orienter la vue / Tourner la planète");
                                ui.end_row();
                            });

                        ui.add_space(12.0_f32);
                        ui.separator();
                        ui.add_space(8.0_f32);

                        ui.horizontal(|ui| {
                            if ui.button(egui::RichText::new("▶  Reprendre la partie [Échap]").strong()).clicked() {
                                self.is_settings_open = false;
                                action = Some(UiAction::ResumeGame);
                            }
                            if ui.button(egui::RichText::new("🚪  Quitter le jeu").color(egui::Color32::from_rgb(245, 110, 110))).clicked() {
                                action = Some(UiAction::QuitGame);
                            }
                        });
                    });
            }
        });

        (full_output, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_initialization() {
        let ui = UiState::new(2026);
        assert_eq!(ui.current_seed, 2026);
        assert_eq!(ui.current_fps, 60);
        assert!(!ui.is_settings_open);
        assert!(!ui.auto_rotate);
        assert!(!ui.show_tectonics);
        assert!(!ui.show_ocean_currents);
        assert_eq!(ui.active_filter(), herocraft_world::VisualizationFilter::Standard);
    }

    #[test]
    fn test_video_settings_labels_and_defaults() {
        let settings = VideoSettings::default();
        assert!(settings.vsync);
        assert!(!settings.fullscreen);
        assert_eq!(settings.lod_distance, LodDistanceSetting::Normale);
        assert_eq!(settings.fps_limit, FpsLimitSetting::Fps60);

        assert_eq!(LodDistanceSetting::Courte.label(), "Courte (Rapide)");
        assert_eq!(LodDistanceSetting::Normale.label(), "Normale (Équilibrée)");
        assert_eq!(LodDistanceSetting::Elevee.label(), "Élevée (Haute Fidélité)");

        assert_eq!(FpsLimitSetting::Fps60.label(), "60 FPS");
        assert_eq!(FpsLimitSetting::Fps120.label(), "120 FPS");
        assert_eq!(FpsLimitSetting::Unlimited.label(), "Illimité");
    }

    #[test]
    fn test_ui_settings_toggle_and_filters() {
        let mut ui = UiState::new(42);

        assert!(!ui.is_settings_open);
        assert!(ui.toggle_settings());
        assert!(ui.is_settings_open);
        assert!(!ui.toggle_settings());
        assert!(!ui.is_settings_open);

        ui.show_tectonics = true;
        assert_eq!(ui.active_filter(), herocraft_world::VisualizationFilter::Tectonics);

        ui.show_tectonics = false;
        ui.show_ocean_currents = true;
        assert_eq!(ui.active_filter(), herocraft_world::VisualizationFilter::OceanCurrents);
    }

    #[test]
    fn test_ui_update_runs_cleanly() {
        let mut ui = UiState::new(100);
        ui.set_screen_size(1280.0, 720.0);

        // Vue orbite
        let (output_orbit, action_orbit) = ui.update_ui(200.0, false);
        assert!(action_orbit.is_none());
        assert!(!output_orbit.shapes.is_empty());

        // Vue sol avec menu paramètres ouvert
        ui.set_settings_open(true);
        let (output_ground, action_ground) = ui.update_ui(10.0, true);
        assert!(action_ground.is_none());
        assert!(!output_ground.shapes.is_empty());
    }

    #[test]
    fn test_lighting_settings_defaults_and_mutation() {
        let mut settings = LightingSettings::default();
        assert_eq!(settings.moon_intensity, 0.25);
        assert_eq!(settings.sun_intensity, 1.0);
        assert_eq!(settings.night_ambient, 2.10);
        assert_eq!(settings.day_ambient, 1.0);
        assert_eq!(settings.moon_speed, 1.0);
        assert_eq!(settings.fog_intensity, 1.0);

        settings.moon_intensity = 2.5;
        settings.moon_speed = 0.0; // Fige la lune
        assert_eq!(settings.moon_intensity, 2.5);
        assert_eq!(settings.moon_speed, 0.0);

        // Réinitialisation
        settings = LightingSettings::default();
        assert_eq!(settings.moon_intensity, 0.25);
        assert_eq!(settings.night_ambient, 2.10);
        assert_eq!(settings.moon_speed, 1.0);
    }
}

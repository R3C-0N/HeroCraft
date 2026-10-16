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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsLimitSetting {
    Fps60,
    Fps120,
    Unlimited,
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

#[derive(Debug, Clone, Copy)]
pub struct UiButton {
    pub id: UiAction,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: &'static str,
    pub is_checkbox: bool,
    pub is_hovered: bool,
    pub is_pressed: bool,
}

impl UiButton {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

pub struct UiState {
    pub canvas_width: usize,
    pub canvas_height: usize,
    pub pixels: Vec<u8>,
    pub buttons: Vec<UiButton>,
    pub settings_buttons: Vec<UiButton>,
    pub current_seed: u64,
    pub auto_rotate: bool,
    pub show_tectonics: bool,
    pub show_ocean_currents: bool,
    pub current_fps: u32,
    pub is_settings_open: bool,
    pub video_settings: VideoSettings,
    pub screen_width: f32,
    pub screen_height: f32,
    pub dirty: bool,
}

impl UiState {
    pub fn new(initial_seed: u64) -> Self {
        let canvas_width = 512;
        let canvas_height = 512;

        let buttons = vec![
            UiButton {
                id: UiAction::GenerateNewPlanet,
                x: 16.0,
                y: 64.0,
                w: 288.0,
                h: 32.0,
                label: "NOUVELLE PLANETE [G]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleGroundCamera,
                x: 16.0,
                y: 100.0,
                w: 288.0,
                h: 32.0,
                label: "VUE SOL (FPS) / ORBITE [F]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleAutoRotate,
                x: 16.0,
                y: 136.0,
                w: 288.0,
                h: 32.0,
                label: "AUTO-ROTATION [ESPACE]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ResetCamera,
                x: 16.0,
                y: 172.0,
                w: 288.0,
                h: 32.0,
                label: "REINITIALISER VUE [R]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleTectonics,
                x: 16.0,
                y: 212.0,
                w: 288.0,
                h: 32.0,
                label: "PLAQUES TECTONIQUES",
                is_checkbox: true,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleOceanCurrents,
                x: 16.0,
                y: 248.0,
                w: 288.0,
                h: 32.0,
                label: "COURANTS MARINS",
                is_checkbox: true,
                is_hovered: false,
                is_pressed: false,
            },
        ];

        let settings_buttons = vec![
            UiButton {
                id: UiAction::ToggleVsync,
                x: 24.0,
                y: 68.0,
                w: 332.0,
                h: 36.0,
                label: "V-SYNC",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::CycleLodDistance,
                x: 24.0,
                y: 110.0,
                w: 332.0,
                h: 36.0,
                label: "DISTANCE LOD",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleFullscreen,
                x: 24.0,
                y: 152.0,
                w: 332.0,
                h: 36.0,
                label: "PLEIN ECRAN",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::CycleFpsLimit,
                x: 24.0,
                y: 194.0,
                w: 332.0,
                h: 36.0,
                label: "LIMITE FPS",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ResumeGame,
                x: 24.0,
                y: 250.0,
                w: 332.0,
                h: 36.0,
                label: "REPRENDRE LA PARTIE [ECHAP]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::QuitGame,
                x: 24.0,
                y: 296.0,
                w: 332.0,
                h: 36.0,
                label: "QUITTER LE JEU",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
        ];

        let mut ui = Self {
            canvas_width,
            canvas_height,
            pixels: vec![0; canvas_width * canvas_height * 4],
            buttons,
            settings_buttons,
            current_seed: initial_seed,
            auto_rotate: false,
            show_tectonics: false,
            show_ocean_currents: false,
            current_fps: 60,
            is_settings_open: false,
            video_settings: VideoSettings::default(),
            screen_width: 1280.0,
            screen_height: 720.0,
            dirty: true,
        };

        ui.redraw();
        ui
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

    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn set_settings_open(&mut self, is_open: bool) {
        if self.is_settings_open != is_open {
            self.is_settings_open = is_open;
            self.redraw();
        }
    }

    pub fn toggle_settings(&mut self) -> bool {
        self.set_settings_open(!self.is_settings_open);
        self.is_settings_open
    }

    /// Vérifie si la souris survole l'un des boutons ou le panneau d'interface
    pub fn on_cursor_moved(&mut self, x: f32, y: f32) -> bool {
        let mut changed = false;

        if self.is_settings_open {
            let menu_w = 380.0_f32;
            let menu_h = 400.0_f32;
            let menu_x = ((self.screen_width - menu_w) * 0.5).max(0.0);
            let menu_y = ((self.screen_height - menu_h) * 0.5).max(0.0);

            let rel_x = x - menu_x;
            let rel_y = y - menu_y;

            for btn in &mut self.settings_buttons {
                let hovered = btn.contains(rel_x, rel_y);
                if btn.is_hovered != hovered {
                    btn.is_hovered = hovered;
                    changed = true;
                }
            }

            if changed {
                self.redraw();
            }

            // En mode menu des paramètres, tout déplacement souris est absorbé par l'UI
            true
        } else {
            let is_over_panel = x >= 12.0 && x <= 308.0 && y >= 12.0 && y <= 300.0;

            for btn in &mut self.buttons {
                let hovered = btn.contains(x, y);
                if btn.is_hovered != hovered {
                    btn.is_hovered = hovered;
                    changed = true;
                }
            }

            if changed {
                self.redraw();
            }

            is_over_panel
        }
    }

    /// Gère le clic de la souris
    pub fn on_mouse_down(&mut self, x: f32, y: f32) -> Option<UiAction> {
        let mut clicked_action = None;

        if self.is_settings_open {
            let menu_w = 380.0_f32;
            let menu_h = 400.0_f32;
            let menu_x = ((self.screen_width - menu_w) * 0.5).max(0.0);
            let menu_y = ((self.screen_height - menu_h) * 0.5).max(0.0);

            let rel_x = x - menu_x;
            let rel_y = y - menu_y;

            for btn in &mut self.settings_buttons {
                if btn.contains(rel_x, rel_y) {
                    btn.is_pressed = true;
                    clicked_action = Some(btn.id);

                    match btn.id {
                        UiAction::ToggleVsync => {
                            self.video_settings.vsync = !self.video_settings.vsync;
                        }
                        UiAction::CycleLodDistance => {
                            self.video_settings.lod_distance = match self.video_settings.lod_distance {
                                LodDistanceSetting::Courte => LodDistanceSetting::Normale,
                                LodDistanceSetting::Normale => LodDistanceSetting::Elevee,
                                LodDistanceSetting::Elevee => LodDistanceSetting::Courte,
                            };
                        }
                        UiAction::ToggleFullscreen => {
                            self.video_settings.fullscreen = !self.video_settings.fullscreen;
                        }
                        UiAction::CycleFpsLimit => {
                            self.video_settings.fps_limit = match self.video_settings.fps_limit {
                                FpsLimitSetting::Fps60 => FpsLimitSetting::Fps120,
                                FpsLimitSetting::Fps120 => FpsLimitSetting::Unlimited,
                                FpsLimitSetting::Unlimited => FpsLimitSetting::Fps60,
                            };
                        }
                        UiAction::ResumeGame => {
                            self.is_settings_open = false;
                        }
                        _ => {}
                    }
                }
            }

            if clicked_action.is_some() {
                self.redraw();
            }

            clicked_action
        } else {
            for btn in &mut self.buttons {
                if btn.contains(x, y) {
                    btn.is_pressed = true;
                    clicked_action = Some(btn.id);

                    match btn.id {
                        UiAction::ToggleTectonics => {
                            self.show_tectonics = !self.show_tectonics;
                            if self.show_tectonics {
                                self.show_ocean_currents = false;
                            }
                        }
                        UiAction::ToggleOceanCurrents => {
                            self.show_ocean_currents = !self.show_ocean_currents;
                            if self.show_ocean_currents {
                                self.show_tectonics = false;
                            }
                        }
                        _ => {}
                    }
                }
            }

            if clicked_action.is_some() {
                self.redraw();
            }

            clicked_action
        }
    }

    pub fn on_mouse_up(&mut self) {
        let mut changed = false;
        for btn in &mut self.buttons {
            if btn.is_pressed {
                btn.is_pressed = false;
                changed = true;
            }
        }
        for btn in &mut self.settings_buttons {
            if btn.is_pressed {
                btn.is_pressed = false;
                changed = true;
            }
        }
        if changed {
            self.redraw();
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.current_seed = seed;
        self.redraw();
    }

    pub fn set_auto_rotate(&mut self, enabled: bool) {
        self.auto_rotate = enabled;
        self.redraw();
    }

    /// Redessine le panneau d'interface dans le buffer de pixels
    pub fn redraw(&mut self) {
        self.pixels.fill(0);

        if self.is_settings_open {
            // Pixel assombrissant (Backdrop) tout en bas à droite du canvas
            let dark_idx = ((self.canvas_height - 1) * self.canvas_width + (self.canvas_width - 1)) * 4;
            self.pixels[dark_idx] = 8;
            self.pixels[dark_idx + 1] = 14;
            self.pixels[dark_idx + 2] = 26;
            self.pixels[dark_idx + 3] = 190;

            // Fenêtre centrale du Menu Paramètres Vidéo (380x400)
            self.draw_rounded_box(12, 12, 356, 376, [14, 22, 38, 245], [55, 95, 150, 255]);

            // Titre & Sous-titre
            self.draw_text_5x7(94, 24, "PARAMETRES VIDEO", [220, 240, 255, 255], 2);
            self.draw_text_5x7(90, 48, "[ECHAP] POUR REPRENDRE", [120, 165, 220, 255], 1);

            // Boutons de réglages
            let settings_buttons = self.settings_buttons.clone();
            for btn in settings_buttons {
                let bx = btn.x as usize;
                let by = btn.y as usize;
                let bw = btn.w as usize;
                let bh = btn.h as usize;

                let (label, bg_color, border_color, text_color) = match btn.id {
                    UiAction::ToggleVsync => {
                        let text = if self.video_settings.vsync {
                            "V-SYNC : [ACTIVE (60 FPS)]"
                        } else {
                            "V-SYNC : [DESACTIVE (ILLIMITE)]"
                        };
                        let (bg, border, tc) = if btn.is_pressed {
                            ([12, 85, 160, 255], [100, 200, 255, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([35, 55, 90, 240], [80, 160, 240, 255], [240, 248, 255, 255])
                        } else {
                            ([22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255])
                        };
                        (text, bg, border, tc)
                    }
                    UiAction::CycleLodDistance => {
                        let text = match self.video_settings.lod_distance {
                            LodDistanceSetting::Courte => "DISTANCE LOD : [COURTE]",
                            LodDistanceSetting::Normale => "DISTANCE LOD : [NORMALE]",
                            LodDistanceSetting::Elevee => "DISTANCE LOD : [MAXIMALE]",
                        };
                        let (bg, border, tc) = if btn.is_pressed {
                            ([12, 85, 160, 255], [100, 200, 255, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([35, 55, 90, 240], [80, 160, 240, 255], [240, 248, 255, 255])
                        } else {
                            ([22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255])
                        };
                        (text, bg, border, tc)
                    }
                    UiAction::ToggleFullscreen => {
                        let text = if self.video_settings.fullscreen {
                            "PLEIN ECRAN : [OUI]"
                        } else {
                            "PLEIN ECRAN : [NON]"
                        };
                        let (bg, border, tc) = if btn.is_pressed {
                            ([12, 85, 160, 255], [100, 200, 255, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([35, 55, 90, 240], [80, 160, 240, 255], [240, 248, 255, 255])
                        } else {
                            ([22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255])
                        };
                        (text, bg, border, tc)
                    }
                    UiAction::CycleFpsLimit => {
                        let text = match self.video_settings.fps_limit {
                            FpsLimitSetting::Fps60 => "LIMITE FPS : [60 FPS]",
                            FpsLimitSetting::Fps120 => "LIMITE FPS : [120 FPS]",
                            FpsLimitSetting::Unlimited => "LIMITE FPS : [ILLIMITE]",
                        };
                        let (bg, border, tc) = if btn.is_pressed {
                            ([12, 85, 160, 255], [100, 200, 255, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([35, 55, 90, 240], [80, 160, 240, 255], [240, 248, 255, 255])
                        } else {
                            ([22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255])
                        };
                        (text, bg, border, tc)
                    }
                    UiAction::ResumeGame => {
                        let (bg, border, tc) = if btn.is_pressed {
                            ([20, 95, 75, 255], [80, 225, 175, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([25, 80, 65, 245], [60, 200, 155, 255], [240, 255, 250, 255])
                        } else {
                            ([18, 55, 45, 220], [45, 150, 120, 220], [210, 245, 235, 255])
                        };
                        ("REPRENDRE LA PARTIE [ECHAP]", bg, border, tc)
                    }
                    UiAction::QuitGame => {
                        let (bg, border, tc) = if btn.is_pressed {
                            ([110, 30, 30, 255], [240, 90, 90, 255], [255, 255, 255, 255])
                        } else if btn.is_hovered {
                            ([85, 25, 25, 245], [210, 70, 70, 255], [255, 235, 235, 255])
                        } else {
                            ([55, 20, 20, 210], [150, 50, 50, 200], [240, 195, 195, 255])
                        };
                        ("QUITTER LE JEU", bg, border, tc)
                    }
                    _ => (btn.label, [22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255]),
                };

                self.draw_rounded_box(bx, by, bw, bh, bg_color, border_color);

                let text_pixel_width = label.len() * 6;
                let tx = bx + (bw.saturating_sub(text_pixel_width)) / 2;
                let ty = by + (bh.saturating_sub(8)) / 2;
                self.draw_text_5x7(tx, ty, label, text_color, 1);
            }

            self.draw_text_5x7(76, 350, "OPTIONS APPLIQUEES EN TEMPS REEL", [100, 135, 175, 255], 1);
        } else {
            // 1. Fond du panneau HUD classique translucide
            self.draw_rounded_box(12, 12, 296, 286, [14, 22, 38, 220], [45, 75, 115, 240]);

            // 2. Titre et informations planétaires
            self.draw_text_5x7(24, 24, "HEROCRAFT VOXEL RPG", [220, 235, 255, 255], 2);
            let seed_text = format!("GRAINE : #{}", self.current_seed);
            self.draw_text_5x7(24, 46, &seed_text, [130, 185, 240, 255], 1);
            self.draw_text_5x7(180, 46, "EAU : ~54%", [80, 210, 235, 255], 1);

            // 3. Dessin des boutons et checkboxes HUD
            let buttons = self.buttons.clone();
            for btn in buttons {
                let bx = btn.x as usize;
                let by = btn.y as usize;
                let bw = btn.w as usize;
                let bh = btn.h as usize;

                if btn.is_checkbox {
                    let is_checked = match btn.id {
                        UiAction::ToggleTectonics => self.show_tectonics,
                        UiAction::ToggleOceanCurrents => self.show_ocean_currents,
                        _ => false,
                    };

                    let (bg_color, border_color, box_color, text_color) = if is_checked {
                        if btn.id == UiAction::ToggleTectonics {
                            (
                                [40, 20, 24, 225],
                                [230, 75, 60, 255],
                                [240, 80, 60, 255],
                                [255, 230, 225, 255],
                            )
                        } else {
                            (
                                [16, 36, 48, 225],
                                [50, 185, 220, 255],
                                [45, 195, 235, 255],
                                [225, 245, 255, 255],
                            )
                        }
                    } else if btn.is_hovered {
                        (
                            [26, 38, 62, 220],
                            [75, 120, 175, 240],
                            [70, 110, 160, 255],
                            [230, 240, 255, 255],
                        )
                    } else {
                        (
                            [18, 28, 46, 190],
                            [40, 65, 100, 180],
                            [45, 75, 115, 200],
                            [175, 195, 220, 255],
                        )
                    };

                    self.draw_rounded_box(bx, by, bw, bh, bg_color, border_color);

                    let cbox_x = bx + 12;
                    let cbox_y = by + (bh - 16) / 2;
                    self.draw_rounded_box(cbox_x, cbox_y, 16, 16, [12, 18, 30, 255], box_color);

                    if is_checked {
                        for cy in 3..13 {
                            for cx in 3..13 {
                                let px = cbox_x + cx;
                                let py = cbox_y + cy;
                                if px < self.canvas_width && py < self.canvas_height {
                                    let idx = (py * self.canvas_width + px) * 4;
                                    self.pixels[idx] = box_color[0];
                                    self.pixels[idx + 1] = box_color[1];
                                    self.pixels[idx + 2] = box_color[2];
                                    self.pixels[idx + 3] = 255;
                                }
                            }
                        }
                        self.draw_text_5x7(cbox_x + 5, cbox_y + 4, "X", [255, 255, 255, 255], 1);
                    }

                    let text_x = bx + 38;
                    let text_y = by + (bh - 8) / 2;
                    self.draw_text_5x7(text_x, text_y, btn.label, text_color, 1);
                } else {
                    let (bg_color, border_color, text_color) = if btn.is_pressed {
                        ([12, 85, 160, 255], [100, 200, 255, 255], [255, 255, 255, 255])
                    } else if btn.is_hovered {
                        if btn.id == UiAction::GenerateNewPlanet {
                            ([25, 95, 180, 245], [90, 220, 255, 255], [255, 255, 255, 255])
                        } else {
                            ([35, 55, 90, 240], [80, 160, 240, 255], [240, 248, 255, 255])
                        }
                    } else {
                        if btn.id == UiAction::GenerateNewPlanet {
                            ([18, 65, 130, 230], [50, 150, 240, 230], [225, 240, 255, 255])
                        } else {
                            ([22, 34, 56, 210], [50, 80, 120, 200], [180, 205, 230, 255])
                        }
                    };

                    self.draw_rounded_box(bx, by, bw, bh, bg_color, border_color);

                    let text = if btn.id == UiAction::ToggleAutoRotate {
                        if self.auto_rotate {
                            "AUTO-ROTATION : [OUI]"
                        } else {
                            "AUTO-ROTATION : [NON]"
                        }
                    } else {
                        btn.label
                    };

                    let text_pixel_width = text.len() * 6;
                    let tx = bx + (bw.saturating_sub(text_pixel_width)) / 2;
                    let ty = by + (bh.saturating_sub(8)) / 2;
                    self.draw_text_5x7(tx, ty, text, text_color, 1);
                }
            }

            // 4. Instructions d'aide au bas du panneau
            self.draw_text_5x7(24, 278, "GLISSER : ORBITE | SCROLL : ZOOM", [100, 135, 175, 255], 1);
        }

        // 5. Badge Compteur FPS en haut à droite
        self.draw_fps_badge();

        self.dirty = true;
    }

    /// Dessine le badge du compteur de FPS dans la zone réservée (320..436, 0..32)
    pub fn draw_fps_badge(&mut self) {
        let bx = 320;
        let by = 0;
        let bw = 116;
        let bh = 32;

        // Efface la zone du badge FPS dans le buffer
        for dy in 0..bh {
            let py = by + dy;
            if py >= self.canvas_height {
                continue;
            }
            for dx in 0..bw {
                let px = bx + dx;
                if px >= self.canvas_width {
                    continue;
                }
                let idx = (py * self.canvas_width + px) * 4;
                self.pixels[idx] = 0;
                self.pixels[idx + 1] = 0;
                self.pixels[idx + 2] = 0;
                self.pixels[idx + 3] = 0;
            }
        }

        // Couleurs adaptatives selon le framerate (Vert > 55, Jaune > 30, Rouge < 30)
        let (bg, border, text_color) = if self.current_fps >= 55 {
            ([14, 22, 38, 220], [45, 140, 110, 240], [70, 235, 160, 255])
        } else if self.current_fps >= 30 {
            ([14, 22, 38, 220], [150, 130, 45, 240], [245, 210, 70, 255])
        } else {
            ([24, 14, 18, 220], [180, 50, 45, 240], [255, 90, 80, 255])
        };

        self.draw_rounded_box(bx, by, bw, bh, bg, border);

        // Texte au format "XXX FPS" avec centrage précis
        let fps_text = format!("{:>3} FPS", self.current_fps);
        // Police 5x7 mise à l'échelle x2 : chaque caractère fait 6*2 = 12px de large, hauteur = 7*2 = 14px
        let text_width = fps_text.len() * 12;
        let tx = bx + (bw.saturating_sub(text_width)) / 2;
        let ty = by + (bh.saturating_sub(14)) / 2;
        self.draw_text_5x7(tx, ty, &fps_text, text_color, 2);
    }

    /// Met à jour la valeur du compteur FPS
    pub fn set_fps(&mut self, fps: u32) {
        if self.current_fps != fps {
            self.current_fps = fps;
            self.draw_fps_badge();
            self.dirty = true;
        }
    }

    fn draw_rounded_box(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        bg: [u8; 4],
        border: [u8; 4],
    ) {
        for dy in 0..h {
            let py = y + dy;
            if py >= self.canvas_height {
                continue;
            }
            for dx in 0..w {
                let px = x + dx;
                if px >= self.canvas_width {
                    continue;
                }

                // Coins arrondis légers (rayon 3px)
                let is_corner = (dx < 3 && dy < 3 && (3 - dx) * (3 - dx) + (3 - dy) * (3 - dy) > 9)
                    || (dx >= w - 3 && dy < 3 && (dx - (w - 3)) * (dx - (w - 3)) + (3 - dy) * (3 - dy) > 9)
                    || (dx < 3 && dy >= h - 3 && (3 - dx) * (3 - dx) + (dy - (h - 3)) * (dy - (h - 3)) > 9)
                    || (dx >= w - 3 && dy >= h - 3 && (dx - (w - 3)) * (dx - (w - 3)) + (dy - (h - 3)) * (dy - (h - 3)) > 9);

                if is_corner {
                    continue;
                }

                let is_border = dx == 0 || dx == w - 1 || dy == 0 || dy == h - 1;
                let c = if is_border { border } else { bg };

                let idx = (py * self.canvas_width + px) * 4;
                self.pixels[idx] = c[0];
                self.pixels[idx + 1] = c[1];
                self.pixels[idx + 2] = c[2];
                self.pixels[idx + 3] = c[3];
            }
        }
    }

    /// Rendu de texte ASCII via police bitmap 5x7 intégrée
    fn draw_text_5x7(&mut self, x: usize, y: usize, text: &str, color: [u8; 4], scale: usize) {
        let mut cur_x = x;
        for ch in text.chars() {
            let glyph = get_glyph_5x7(ch);
            for row in 0..7 {
                for col in 0..5 {
                    if (glyph[row] & (1 << (4 - col))) != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = cur_x + col * scale + sx;
                                let py = y + row * scale + sy;
                                if px < self.canvas_width && py < self.canvas_height {
                                    let idx = (py * self.canvas_width + px) * 4;
                                    self.pixels[idx] = color[0];
                                    self.pixels[idx + 1] = color[1];
                                    self.pixels[idx + 2] = color[2];
                                    self.pixels[idx + 3] = color[3];
                                }
                            }
                        }
                    }
                }
            }
            cur_x += (5 + 1) * scale;
        }
    }
}

/// Définition 5x7 des caractères ASCII courants
fn get_glyph_5x7(c: char) -> [u8; 7] {
    match c {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10011, 0b01111],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10011, 0b10101, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '/' => [0b00001, 0b00010, 0b00100, 0b00100, 0b01000, 0b10000, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        '~' => [0b00000, 0b01001, 0b10101, 0b10010, 0b00000, 0b00000, 0b00000],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100],
        _ => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_initialization_and_fps_badge() {
        let mut ui = UiState::new(2026);
        assert_eq!(ui.canvas_width, 512);
        assert_eq!(ui.canvas_height, 512);
        assert_eq!(ui.current_fps, 60);

        // Vérification qu'il y a des pixels non nuls dans la zone du badge FPS (320..436, 0..32)
        let mut fps_pixels_found = false;
        for y in 0..32 {
            for x in 320..436 {
                let idx = (y * ui.canvas_width + x) * 4;
                if ui.pixels[idx + 3] > 0 {
                    fps_pixels_found = true;
                    break;
                }
            }
        }
        assert!(fps_pixels_found, "Le badge FPS doit contenir des pixels visibles dessinés");

        // Test de mise à jour du FPS
        ui.dirty = false;
        ui.set_fps(144);
        assert_eq!(ui.current_fps, 144);
        assert!(ui.dirty, "set_fps avec une nouvelle valeur doit marquer l'UI comme dirty");

        // Mettre à jour avec la même valeur ne doit pas marquer dirty
        ui.dirty = false;
        ui.set_fps(144);
        assert!(!ui.dirty, "set_fps avec la même valeur ne doit pas marquer dirty");
    }

    #[test]
    fn test_video_settings_menu_toggle_and_actions() {
        let mut ui = UiState::new(2026);
        ui.set_screen_size(1280.0, 720.0);
        assert!(!ui.is_settings_open);

        // Ouverture du menu
        ui.toggle_settings();
        assert!(ui.is_settings_open);
        assert!(ui.dirty);

        // En mode menu des paramètres, on_cursor_moved absorbe toujours l'événement
        assert!(ui.on_cursor_moved(10.0, 10.0));

        // Clic sur V-Sync (bouton 0: y=68..104, menu centré en x=(1280-380)/2=450, y=(720-400)/2=160)
        let vsync_click_x = 450.0 + 30.0;
        let vsync_click_y = 160.0 + 80.0;
        assert_eq!(ui.video_settings.vsync, true);
        let action = ui.on_mouse_down(vsync_click_x, vsync_click_y);
        assert_eq!(action, Some(UiAction::ToggleVsync));
        assert_eq!(ui.video_settings.vsync, false);

        // Deuxième clic réactive la V-Sync
        let action = ui.on_mouse_down(vsync_click_x, vsync_click_y);
        assert_eq!(action, Some(UiAction::ToggleVsync));
        assert_eq!(ui.video_settings.vsync, true);

        // Clic sur Distance LOD (bouton 1: y=110..146)
        let lod_click_y = 160.0 + 125.0;
        assert_eq!(ui.video_settings.lod_distance, LodDistanceSetting::Normale);
        let action = ui.on_mouse_down(vsync_click_x, lod_click_y);
        assert_eq!(action, Some(UiAction::CycleLodDistance));
        assert_eq!(ui.video_settings.lod_distance, LodDistanceSetting::Elevee);

        // Clic sur Bouton Reprendre (bouton 4: y=250..286)
        let resume_click_y = 160.0 + 265.0;
        let action = ui.on_mouse_down(vsync_click_x, resume_click_y);
        assert_eq!(action, Some(UiAction::ResumeGame));
        assert!(!ui.is_settings_open, "Le bouton Reprendre doit fermer le menu");
    }
}


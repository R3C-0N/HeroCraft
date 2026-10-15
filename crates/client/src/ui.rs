/// Actions déclenchées par l'interface utilisateur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    GenerateNewPlanet,
    ToggleAutoRotate,
    ResetCamera,
    ToggleTectonics,
    ToggleOceanCurrents,
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
    pub current_seed: u64,
    pub auto_rotate: bool,
    pub show_tectonics: bool,
    pub show_ocean_currents: bool,
    pub dirty: bool,
}

impl UiState {
    pub fn new(initial_seed: u64) -> Self {
        let canvas_width = 320;
        let canvas_height = 310;

        let buttons = vec![
            UiButton {
                id: UiAction::GenerateNewPlanet,
                x: 16.0,
                y: 66.0,
                w: 288.0,
                h: 36.0,
                label: "NOUVELLE PLANETE [G]",
                is_checkbox: false,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleAutoRotate,
                x: 16.0,
                y: 108.0,
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
                y: 146.0,
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
                y: 188.0,
                w: 288.0,
                h: 34.0,
                label: "PLAQUES TECTONIQUES",
                is_checkbox: true,
                is_hovered: false,
                is_pressed: false,
            },
            UiButton {
                id: UiAction::ToggleOceanCurrents,
                x: 16.0,
                y: 228.0,
                w: 288.0,
                h: 34.0,
                label: "COURANTS MARINS",
                is_checkbox: true,
                is_hovered: false,
                is_pressed: false,
            },
        ];

        let mut ui = Self {
            canvas_width,
            canvas_height,
            pixels: vec![0; canvas_width * canvas_height * 4],
            buttons,
            current_seed: initial_seed,
            auto_rotate: false,
            show_tectonics: false,
            show_ocean_currents: false,
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

    /// Vérifie si la souris survole l'un des boutons ou le panneau d'interface
    pub fn on_cursor_moved(&mut self, x: f32, y: f32) -> bool {
        let mut changed = false;
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

    /// Gère le clic de la souris
    pub fn on_mouse_down(&mut self, x: f32, y: f32) -> Option<UiAction> {
        let mut clicked_action = None;

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

    pub fn on_mouse_up(&mut self) {
        let mut changed = false;
        for btn in &mut self.buttons {
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

        // 1. Fond du panneau translucide façon "Glassmorphism"
        self.draw_rounded_box(12, 12, 296, 286, [14, 22, 38, 220], [45, 75, 115, 240]);

        // 2. Titre et informations planétaires
        self.draw_text_5x7(24, 24, "HEROCRAFT VOXEL RPG", [220, 235, 255, 255], 2);
        let seed_text = format!("GRAINE : #{}", self.current_seed);
        self.draw_text_5x7(24, 46, &seed_text, [130, 185, 240, 255], 1);
        self.draw_text_5x7(180, 46, "EAU : ~54%", [80, 210, 235, 255], 1);

        // 3. Dessin des boutons et checkboxes
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

                // Style du cadre de la checkbox
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

                // Boîte carrée de la checkbox (16x16)
                let cbox_x = bx + 12;
                let cbox_y = by + (bh - 16) / 2;
                self.draw_rounded_box(cbox_x, cbox_y, 16, 16, [12, 18, 30, 255], box_color);

                if is_checked {
                    // Cœur lumineux de la case cochée
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

                // Texte descriptif du filtre
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

                // Centrage du texte dans le bouton classique
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

        self.dirty = true;
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
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        '~' => [0b00000, 0b01001, 0b10101, 0b10010, 0b00000, 0b00000, 0b00000],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        _ => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
    }
}

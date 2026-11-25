//! Le créateur de légende : les trois étapes de la maquette dans le jeu. Le corps à la forge, la
//! voie au râtelier, le nom au baptême. Repris de l'écran `LegendeScreen` du module `HeroCraft`
//! de la branche `develop`.
//!
//! L'écran tient tout l'écran, sur un mur de planches, en deux panneaux côte à côte : à gauche le
//! héros qui tourne lentement dans son puits sombre, avec sa fiche dessous ; à droite l'étape en
//! cours. Rien n'est définitif avant le baptême, qui vérifie chaque choix contre l'atlas
//! ([`Legend::verify`]) et scelle le héros pour de bon.

use crate::hero_model::{Figure, FigureCache, FigurePose};
use herocraft_common::hero::{self, atlas, Appearance, ClassSheet, Dice, Family, Legend, CLASSES, NAME_MAX, SHEET_SIZE};
use herocraft_common::PlayerClass;


// Palette du créateur : bois sombre, parchemin et or
const GOLD: egui::Color32 = egui::Color32::from_rgb(212, 175, 55);
const GOLD_BRIGHT: egui::Color32 = egui::Color32::from_rgb(240, 200, 60);
const INK: egui::Color32 = egui::Color32::from_rgb(236, 226, 205);
const INK_SOFT: egui::Color32 = egui::Color32::from_rgb(200, 188, 165);
const INK_DIM: egui::Color32 = egui::Color32::from_rgb(150, 138, 118);
const DANGER: egui::Color32 = egui::Color32::from_rgb(245, 120, 105);
const PANEL: egui::Color32 = egui::Color32::from_rgba_premultiplied(18, 13, 10, 236);
const WELL: egui::Color32 = egui::Color32::from_rgb(10, 8, 7);
const CASE: egui::Color32 = egui::Color32::from_rgb(38, 29, 22);
const CASE_HOVER: egui::Color32 = egui::Color32::from_rgb(56, 42, 30);
const CASE_ON: egui::Color32 = egui::Color32::from_rgb(78, 60, 24);
const EDGE: egui::Color32 = egui::Color32::from_rgb(84, 64, 44);

const TITRES: [&str; 3] = ["La forge des âmes", "Le râtelier", "Le nom"];
const INTROS: [&str; 3] = [
    "Rien n’est encore écrit. Une silhouette attend sur l’enclume : donnez-lui une chair, une taille, un regard.",
    "Le maître d’armes ouvre le râtelier. Votre voie décide de ce que vos mains tiendront au premier matin.",
    "Tout est en place, sauf le nom. C’est lui que les tavernes répéteront, ou oublieront.",
];
const PANNEAUX: [&str; 3] = ["Modelage", "Les voies", "Baptême"];
const PAS: [&str; 3] = ["Le corps", "La voie", "Le nom"];

/// Onglets de la forge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Species,
    Body,
    Face,
    Hair,
    Outfit,
}

impl Tab {
    const ALL: [(Tab, &'static str); 5] =
        [(Tab::Species, "Espèce"), (Tab::Body, "Corps"), (Tab::Face, "Visage"), (Tab::Hair, "Pilosité"), (Tab::Outfit, "Tenue")];
}

/// Ce que le créateur demande au jeu
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CreatorEvent {
    /// Le baptême est accepté : la légende est à enregistrer et à envoyer au serveur
    Sealed(Box<Legend>),
    /// Le héros entre dans le monde : l'écran se referme
    Enter,
}

/// Le créateur ouvert
pub(crate) struct HeroCreator {
    step: usize,
    tab: Tab,
    cfg: Appearance,
    class: Option<PlayerClass>,
    name: String,
    dice: Dice,
    /// Raison du dernier refus du baptême
    refusal: Option<String>,
    /// La légende scellée : l'écran ne montre plus que l'entrée dans le monde
    sealed: Option<Legend>,
    preview: Preview,
}

impl Default for HeroCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl HeroCreator {
    pub(crate) fn new() -> Self {
        Self {
            step: 0,
            tab: Tab::Species,
            cfg: atlas().defaut("humain"),
            class: None,
            name: String::new(),
            dice: Dice::from_clock(),
            refusal: None,
            sealed: None,
            preview: Preview::default(),
        }
    }

    fn sheet(&self) -> Option<&'static ClassSheet> {
        self.class.and_then(hero::class_sheet)
    }

    fn go(&mut self, step: usize) {
        if self.sealed.is_some() {
            return;
        }
        let max = if self.class.is_some() { 2 } else { 1 };
        self.step = step.min(max);
        self.refusal = None;
    }

    /// Le dernier pas : la légende vérifiée contre l'atlas, puis scellée
    fn seal(&mut self) -> Option<CreatorEvent> {
        let class = self.class?;
        let mut appearance = self.cfg.clone();
        appearance.nom = self.name.clone();
        let mut legend = Legend { appearance, class };
        match legend.verify() {
            Ok(()) => {
                self.refusal = None;
                self.cfg = legend.appearance.clone();
                self.sealed = Some(legend.clone());
                Some(CreatorEvent::Sealed(Box::new(legend)))
            }
            Err(reason) => {
                self.refusal = Some(format!("Le sceau ne prend pas : {reason}."));
                None
            }
        }
    }

    /// Dessine l'écran sur tout l'écran ; renvoie ce que le joueur vient de décider
    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Option<CreatorEvent> {
        let mut event = None;
        let screen = ctx.content_rect();
        // Le temps réel écoulé (et non le pas prévu par egui) : la figure tourne à la même allure
        // quel que soit le nombre d'images par seconde
        let now = std::time::Instant::now();
        let dt = self.preview.last_time.map_or(0.0_f32, |t| now.duration_since(t).as_secs_f32().min(0.25_f32));
        self.preview.last_time = Some(now);
        self.preview.update(dt);
        ctx.request_repaint();

        egui::Area::new(egui::Id::unique("hero_creator"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                paint_planks(ui.painter(), screen);

                let margin = (screen.width() * 0.03_f32).clamp(12.0_f32, 40.0_f32);
                let inner = screen.shrink(margin);
                let gap = 18.0_f32;
                let left_w = (inner.width() * 0.4_f32).clamp(240.0_f32, 520.0_f32);
                let left = egui::Rect::from_min_size(inner.min, egui::vec2(left_w, inner.height()));
                let right = egui::Rect::from_min_max(egui::pos2(left.right() + gap, inner.top()), inner.max);

                ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.draw_preview_panel(ui, left));
                ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
                    event = self.draw_step_panel(ui, right);
                });
            });
        event
    }

    /* ---------------------------------------------------------------- panneau de gauche */

    fn draw_preview_panel(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        panel_frame(ui.painter(), rect);
        let pad = 16.0_f32;
        let content = rect.shrink(pad);
        let info_h = 150.0_f32.min(content.height() * 0.4_f32);
        let well = egui::Rect::from_min_max(content.min, egui::pos2(content.right(), content.bottom() - info_h - 10.0_f32));

        // Le puits sombre et le héros qui y tourne
        ui.painter().rect_filled(well, 6.0_f32, WELL);
        ui.painter().rect_stroke(well, 6.0_f32, egui::Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Middle);
        let response = ui.interact(well, egui::Id::unique("hero_preview"), egui::Sense::drag());
        if response.dragged() {
            let d = response.drag_delta();
            self.preview.angle += d.x * 0.0096_f32 * 1.6_f32;
            self.preview.tilt = (self.preview.tilt - d.y * 0.007_f32).clamp(-0.42_f32, 0.42_f32);
        }
        self.preview.held = response.dragged();
        let texture = self.preview.texture(ui.ctx(), &self.cfg);
        let figure = self.preview.figure.get(&self.cfg);
        draw_figure(ui.painter(), well, &texture, &self.cfg, figure, self.preview.angle, self.preview.tilt);
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        // Rotation : la case de la maquette, en haut à droite du puits
        let toggle = egui::Rect::from_min_size(egui::pos2(well.right() - 112.0_f32, well.top() + 8.0_f32), egui::vec2(104.0_f32, 24.0_f32));
        let toggle_resp = ui.interact(toggle, egui::Id::unique("hero_rotation"), egui::Sense::click());
        if toggle_resp.clicked() {
            self.preview.turning = !self.preview.turning;
        }
        paint_check(ui.painter(), toggle, "Rotation", self.preview.turning, toggle_resp.hovered());

        // La fiche, sous le puits
        let info = egui::Rect::from_min_max(egui::pos2(content.left(), well.bottom() + 10.0_f32), content.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(info), |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0_f32, 4.0_f32);
            let sp = self.cfg.species();
            let title = match self.sheet() {
                Some(sheet) => format!("{} · {}", sp.nom, sheet.name),
                None => sp.nom.clone(),
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).size(18.0_f32).strong().color(GOLD_BRIGHT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let size = format!("{} · {}", metres(self.cfg.taille), self.cfg.build_label().to_lowercase());
                    ui.label(egui::RichText::new(size).size(12.5_f32).color(INK_SOFT));
                });
            });
            if let Some(v) = self.cfg.variant() {
                ui.label(egui::RichText::new(&v.nom).size(13.0_f32).italics().color(INK_SOFT));
            }
            let text = match self.sheet() {
                Some(sheet) => sheet.role.to_string(),
                None => self.cfg.variant().and_then(|v| v.apparence.clone()).unwrap_or_else(|| sp.apparence.clone()),
            };
            ui.label(egui::RichText::new(text).size(12.0_f32).color(INK_DIM));
            ui.add_space(2.0_f32);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("MAINS").size(10.5_f32).strong().color(GOLD));
                let hands = match self.sheet() {
                    Some(sheet) => sheet.armament,
                    None => "Vides. Aucune voie n’a encore été choisie.",
                };
                ui.label(egui::RichText::new(hands).size(12.0_f32).color(INK_SOFT));
            });
        });
    }

    /* ---------------------------------------------------------------- panneau de droite */

    fn draw_step_panel(&mut self, ui: &mut egui::Ui, rect: egui::Rect) -> Option<CreatorEvent> {
        panel_frame(ui.painter(), rect);
        let mut event = None;
        let pad = 18.0_f32;
        let content = rect.shrink(pad);
        let footer_h = 48.0_f32;
        let body = egui::Rect::from_min_max(content.min, egui::pos2(content.right(), content.bottom() - footer_h));
        let footer = egui::Rect::from_min_max(egui::pos2(content.left(), body.bottom()), content.max);
        let frozen = self.sealed.is_some();

        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0_f32, 6.0_f32);
            // Les trois pas, en haut
            ui.horizontal(|ui| {
                for (i, pas) in PAS.iter().enumerate() {
                    let enabled = !frozen && (i < 2 || self.class.is_some());
                    let text = egui::RichText::new(format!("{}  {}", i + 1, pas.to_uppercase())).size(12.0_f32).strong();
                    let text = if i == self.step { text.color(GOLD_BRIGHT) } else { text.color(INK_DIM) };
                    let button = egui::Button::new(text).fill(if i == self.step { CASE_ON } else { egui::Color32::TRANSPARENT }).stroke(egui::Stroke::new(
                        1.0_f32,
                        if i == self.step { GOLD } else { EDGE },
                    ));
                    if ui.add_enabled(enabled, button).clicked() {
                        self.go(i);
                    }
                }
            });
            ui.add_space(4.0_f32);
            ui.label(egui::RichText::new(TITRES[self.step]).size(26.0_f32).strong().color(INK));
            ui.label(egui::RichText::new(INTROS[self.step]).size(13.0_f32).italics().color(INK_SOFT));
            ornament(ui, body.width());
            ui.label(egui::RichText::new(PANNEAUX[self.step].to_uppercase()).size(11.0_f32).strong().color(GOLD));

            egui::ScrollArea::vertical().id_salt(("hero_step", self.step)).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(ui.available_width() - 10.0_f32);
                match self.step {
                    0 => self.draw_forge(ui),
                    1 => self.draw_rack(ui),
                    _ => self.draw_baptism(ui),
                }
            });
        });

        // Pied : où en est le joueur, Retour et Suivant
        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            ui.painter().line_segment([footer.left_top(), footer.right_top()], egui::Stroke::new(1.0_f32, EDGE));
            ui.add_space(10.0_f32);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(self.footer_text()).size(12.5_f32).color(INK_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let next = match (frozen, self.step) {
                        (true, _) => "Entrer dans le monde",
                        (false, 0) => "Choisir sa voie",
                        (false, 1) => "Nommer le héros",
                        _ => "Sceller la légende",
                    };
                    let enabled = frozen || ((self.step == 0 || self.class.is_some()) && (self.step < 2 || !self.name.trim().is_empty()));
                    let button = egui::Button::new(egui::RichText::new(next).size(15.0_f32).strong().color(GOLD_BRIGHT))
                        .fill(CASE_ON)
                        .stroke(egui::Stroke::new(1.5_f32, GOLD))
                        .min_size(egui::vec2(190.0_f32, 34.0_f32));
                    if ui.add_enabled(enabled, button).clicked() {
                        if frozen {
                            event = Some(CreatorEvent::Enter);
                        } else if self.step < 2 {
                            self.go(self.step + 1);
                        } else {
                            event = self.seal();
                        }
                    }
                    let back = egui::Button::new(egui::RichText::new("Retour").size(14.0_f32).color(INK_SOFT))
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0_f32, EDGE))
                        .min_size(egui::vec2(100.0_f32, 34.0_f32));
                    if ui.add_enabled(self.step > 0 && !frozen, back).clicked() {
                        self.go(self.step - 1);
                    }
                });
            });
        });
        event
    }

    fn footer_text(&self) -> String {
        if self.sealed.is_some() {
            return "Fiche scellée.".into();
        }
        match self.step {
            0 => "Étape 1 sur 3 — le corps.".into(),
            1 => match self.sheet() {
                Some(sheet) => format!("Voie choisie : {}.", sheet.name),
                None => "Choisissez une voie pour continuer.".into(),
            },
            _ => "Un nom, et la partie commence.".into(),
        }
    }

    /* ---------------------------------------------------------------- étape 1 : la forge */

    fn draw_forge(&mut self, ui: &mut egui::Ui) {
        let a = atlas();
        ui.horizontal(|ui| {
            for (tab, label) in Tab::ALL {
                let on = tab == self.tab;
                let text = egui::RichText::new(label.to_uppercase()).size(11.5_f32).strong().color(if on { GOLD_BRIGHT } else { INK_DIM });
                let button = egui::Button::new(text).fill(if on { CASE_ON } else { CASE }).stroke(egui::Stroke::new(1.0_f32, if on { GOLD } else { EDGE }));
                if ui.add(button).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(4.0_f32);

        let traits = a.traits_de(&self.cfg);
        let mut changed = false;
        let mut souche = false;
        match self.tab {
            Tab::Species => {
                let mut picked = None;
                let especes: Vec<_> = a.especes().collect();
                grid(ui, 2, especes.len(), |ui, i, width| {
                    let e = especes[i];
                    if case(ui, width, &e.nom, Some(&metres(e.taille)), e.id == self.cfg.espece).clicked() {
                        picked = Some(e.id.clone());
                    }
                });
                if let Some(id) = picked {
                    self.cfg = a.defaut(&id);
                }
                let sp = self.cfg.species();
                let names: Vec<(String, String)> = sp.variantes.iter().enumerate().map(|(i, v)| (i.to_string(), v.nom.clone())).collect();
                let mut variante = self.cfg.variante.to_string();
                if selector(ui, "Variante", &names, &mut variante) {
                    self.cfg.variante = variante.parse().unwrap_or(0);
                    souche = true;
                }
                note(ui, "Chaque variante est une sous-race : silhouette, carnation et appendices lui sont propres.");
                ui.label(egui::RichText::new(&sp.attribut).size(12.0_f32).color(INK_SOFT));
            }
            Tab::Body => {
                let sp = self.cfg.species();
                let mut taille = self.cfg.taille as f32;
                let label = metres(self.cfg.taille);
                if slider(ui, "Taille", &label, &mut taille, sp.min as f32..=sp.max as f32) {
                    self.cfg.taille = (taille as f64 * 100.0).round() / 100.0;
                    changed = true;
                }
                note(ui, &format!("Entre {} et {} pour cette espèce.", metres(sp.min), metres(sp.max)));
                let mut corpulence = self.cfg.corpulence as f32;
                if slider(ui, "Corpulence", self.cfg.build_label(), &mut corpulence, 0.0..=1.0) {
                    self.cfg.corpulence = (corpulence as f64 * 50.0).round() / 50.0;
                    changed = true;
                }
                changed |= swatches(ui, "Couleur de peau", a.peaux_de(&self.cfg), &mut self.cfg.peau);
                changed |= selector(ui, "Forme de tête", &options(&a.FORMES_TETE), &mut self.cfg.forme_tete);
            }
            Tab::Face => {
                changed |= selector(ui, "Yeux", &options(&a.YEUX), &mut self.cfg.yeux);
                changed |= swatches(ui, "Couleur des yeux", &a.PAL_YEUX, &mut self.cfg.couleur_yeux);
                if traits.regard.as_deref() == Some("lumineux") {
                    note(ui, "Regard lumineux sans pupille : la teinte vient de la variante.");
                }
                changed |= selector(ui, "Bouche", &options(&a.BOUCHES), &mut self.cfg.bouche);
                if traits.canines.is_some() {
                    note(ui, "Canines inférieures saillantes ajoutées par l’espèce.");
                }
                changed |= selector(ui, "Marques", &options(&a.MARQUES), &mut self.cfg.marques);
            }
            Tab::Hair => {
                changed |= selector(ui, "Coiffure", &options(&a.COIFFURES), &mut self.cfg.coiffure);
                if traits.coiffure_forcee.is_some() {
                    note(ui, "Imposée par la variante.");
                }
                changed |= swatches(ui, "Couleur de cheveux", &a.PAL_CHEVEUX, &mut self.cfg.couleur_cheveux);
                changed |= selector(ui, "Barbe", &options(&a.BARBES), &mut self.cfg.barbe);
                if traits.barbe_forcee.is_some() {
                    note(ui, "Pilosité imposante : imposée par la variante.");
                }
            }
            Tab::Outfit => {
                changed |= selector(ui, "Vêtements de base", &options(&a.TENUES), &mut self.cfg.tenue);
                changed |= swatches(ui, "Couleur de tenue", &a.PAL_TENUE, &mut self.cfg.couleur_tenue);
            }
        }
        if changed || souche {
            self.cfg.retouch(souche);
        }

        ui.add_space(10.0_f32);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Aucun trait n’est définitif avant le baptême.").size(12.0_f32).color(INK_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, "Tirer au sort").clicked() {
                    let dice = &mut self.dice;
                    self.cfg = atlas().aleatoire(&mut || dice.roll(), None);
                }
            });
        });
    }

    /* ---------------------------------------------------------------- étape 2 : le râtelier */

    fn draw_rack(&mut self, ui: &mut egui::Ui) {
        for (family, title, line) in Family::ALL {
            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title.to_uppercase()).size(12.5_f32).strong().color(GOLD));
                ui.label(egui::RichText::new(line).size(12.0_f32).color(INK_DIM));
            });
            let classes: Vec<&ClassSheet> = CLASSES.iter().filter(|c| c.family == family).collect();
            grid(ui, 2, classes.len(), |ui, i, width| {
                let c = classes[i];
                if case(ui, width, c.name, None, self.class == Some(c.class)).clicked() {
                    self.class = Some(c.class);
                    self.refusal = None;
                }
            });
        }
        let Some(sheet) = self.sheet() else { return };
        ui.add_space(8.0_f32);
        well_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(sheet.name).size(20.0_f32).strong().color(GOLD_BRIGHT));
            ui.label(egui::RichText::new(format!("« {} »", sheet.role)).size(13.5_f32).italics().color(INK));
            ui.label(egui::RichText::new(sheet.armament).size(12.0_f32).color(INK_SOFT));
            ui.label(
                egui::RichText::new(format!("Mêlée {} · Distance {} · Magie {}", sheet.melee, sheet.ranged, sheet.magic)).size(12.0_f32).color(INK_DIM),
            );
        });
    }

    /* ---------------------------------------------------------------- étape 3 : le baptême */

    fn draw_baptism(&mut self, ui: &mut egui::Ui) {
        let frozen = self.sealed.is_some();
        rubric(ui, "Nom du héros");
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 150.0_f32).max(160.0_f32);
            let edit = egui::TextEdit::singleline(&mut self.name)
                .char_limit(NAME_MAX)
                .hint_text(egui::RichText::new("Votre nom").color(INK_DIM))
                .font(egui::FontId::proportional(18.0_f32))
                .desired_width(width);
            if ui.add_enabled(!frozen, edit).changed() {
                self.refusal = None;
            }
            if ui.add_enabled_ui(!frozen, |ui| ghost_button(ui, "Tirer un nom")).inner.clicked() {
                let dice = &mut self.dice;
                self.name = hero::random_name(&mut || dice.roll());
                self.refusal = None;
            }
        });
        note(ui, "Un nom se porte longtemps. Les crieurs le répéteront mal, les bardes l’allongeront, les registres de garnison l’abrégeront.");

        ui.add_space(6.0_f32);
        well_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            rubric(ui, "Feuille de départ");
            let sp = self.cfg.species();
            let variant = self.cfg.variant().map(|v| v.nom.as_str()).unwrap_or("");
            sheet_line(ui, "Espèce", &format!("{} · {}", sp.nom, variant));
            sheet_line(ui, "Taille", &format!("{} · corpulence {}", metres(self.cfg.taille), self.cfg.build_label().to_lowercase()));
            if let Some(sheet) = self.sheet() {
                sheet_line(ui, "Voie", sheet.name);
                sheet_line(ui, "Armement", sheet.armament);
            }
            sheet_line(ui, "Palier", "1 · première sortie");
        });

        if let Some(legend) = &self.sealed {
            ui.add_space(8.0_f32);
            ui.label(egui::RichText::new(format!("Ainsi commence la légende de {}.", legend.appearance.nom)).size(20.0_f32).strong().color(GOLD_BRIGHT));
            ui.label(
                egui::RichText::new("La garnison de Valbrume vous inscrit au palier 1. Le monde ne connaît pas encore votre nom : il l’apprendra, ou il l’oubliera.")
                    .size(12.5_f32)
                    .color(INK_SOFT),
            );
        }
        if let Some(refusal) = &self.refusal {
            ui.add_space(6.0_f32);
            ui.label(egui::RichText::new(refusal).size(13.0_f32).color(DANGER));
        }
    }
}

/* -------------------------------------------------------------------- aperçu tournant */

/// La figure du créateur : la planche en cours sur le corps du héros, de trois quarts, qui tourne
/// lentement ; la glisser la tourne à la main. L'écran d'inventaire en montre une aussi, au
/// milieu des cases d'équipement.
pub(crate) struct Preview {
    angle: f32,
    tilt: f32,
    turning: bool,
    held: bool,
    /// Heure de la dernière image dessinée
    last_time: Option<std::time::Instant>,
    /// Planche peinte et la clé de l'apparence qui l'a peinte : elle n'est repeinte que si ce qui
    /// s'y voit change
    texture: Option<(String, egui::TextureHandle)>,
    /// Le corps en relief de cette même planche
    figure: FigureCache,
}

impl Default for Preview {
    fn default() -> Self {
        Self { angle: 46.0_f32.to_radians(), tilt: (-4.0_f32).to_radians(), turning: true, held: false, last_time: None, texture: None, figure: FigureCache::default() }
    }
}

/// Tours par seconde quand on la laisse : la lente rotation de la maquette
const TURN_SPEED: f32 = 0.55;
/// Le plus grand héros que le cadre tient, en mètres : les tailles se comparent
const FRAME_METRES: f32 = 2.05;

impl Preview {
    /// Avance la rotation et dessine le héros `cfg` dans `frame` : la rotation suit l'horloge de
    /// l'interface, comme dans le créateur
    pub(crate) fn paint(&mut self, painter: &egui::Painter, frame: egui::Rect, cfg: &Appearance) {
        let now = std::time::Instant::now();
        let dt = self.last_time.map_or(0.0_f32, |t| now.duration_since(t).as_secs_f32().min(0.25_f32));
        self.last_time = Some(now);
        self.update(dt);
        let texture = self.texture(painter.ctx(), cfg);
        let (angle, tilt) = (self.angle, self.tilt);
        let figure = self.figure.get(cfg);
        draw_figure(painter, frame, &texture, cfg, figure, angle, tilt);
        painter.ctx().request_repaint();
    }

    fn update(&mut self, dt: f32) {
        if self.turning && !self.held {
            self.angle += dt * TURN_SPEED;
        }
    }

    fn texture(&mut self, ctx: &egui::Context, cfg: &Appearance) -> egui::TextureHandle {
        let key = cfg.sheet_key();
        if let Some((k, tex)) = &self.texture
            && *k == key {
            return tex.clone();
        }
        let sheet = hero::paint(cfg);
        let image = egui::ColorImage::from_rgba_unmultiplied([SHEET_SIZE, SHEET_SIZE], &sheet.rgba);
        let tex = ctx.load_texture("hero_sheet", image, egui::TextureOptions::NEAREST);
        self.texture = Some((key, tex.clone()));
        tex
    }
}

/// Dessine la figure tournée de `angle` et inclinée de `tilt` dans le cadre : à la taille de son
/// espèce contre un cadre de deux mètres (agrandi si cornes ou bois dépassent), pieds sur la même
/// ligne, élargie par sa carrure
fn draw_figure(painter: &egui::Painter, frame: egui::Rect, texture: &egui::TextureHandle, cfg: &Appearance, figure: &Figure, angle: f32, tilt: f32) {
    draw_posed_figure(painter, frame, texture, cfg, figure, &FigurePose::default(), angle, tilt);
}

/// Comme `draw_figure`, la figure dans la pose `pose`
fn draw_posed_figure(painter: &egui::Painter, frame: egui::Rect, texture: &egui::TextureHandle, cfg: &Appearance, figure: &Figure, pose: &FigurePose, angle: f32, tilt: f32) {
    let rotation = glam::Quat::from_rotation_x(-tilt) * glam::Quat::from_rotation_y(-angle);
    let width = cfg.width_scale();
    // Mètres par unité du patron : la taille va des pieds au sommet du crâne
    let metres_per_unit = cfg.taille as f32 / figure.body_height;
    let tallest = FRAME_METRES.max((figure.top + figure.hover) * metres_per_unit + 0.05_f32);
    let px_per_metre = frame.height() * 0.84_f32 / tallest;
    let units = px_per_metre * metres_per_unit;
    let feet = egui::pos2(frame.center().x, frame.bottom() - frame.height() * 0.07_f32);
    let light = glam::vec3(-0.45_f32, 0.75_f32, 0.55_f32).normalize();

    // Ombre au sol, plus pâle sous un héros qui flotte
    let shadow = egui::Rect::from_center_size(feet, egui::vec2(14.0_f32 * units * width, 3.5_f32 * units));
    let darkness = if figure.hover > 0.0 { 70 } else { 110 };
    painter.add(egui::Shape::ellipse_filled(shadow.center(), shadow.size() / 2.0_f32, egui::Color32::from_black_alpha(darkness)));

    // Faces tournées vers l'œil, de la plus lointaine à la plus proche : le relief se recouvre
    // comme il faut et les texels translucides (sylphe, ailes) laissent voir ce qui est derrière
    // Sans tampon de profondeur, l'ordre se joue face par face : une grande face de la couche 1
    // passerait devant les mèches en relief posées sur elle. Elle est donc découpée en texels,
    // à la taille des pavés.
    let mut quads: Vec<(f32, [glam::Vec3; 4], [f32; 4], f32)> = Vec::new();
    for q in figure.posed_with(pose, width) {
        let c = q.corners.map(|p| rotation * p);
        let normal = (c[1] - c[0]).cross(c[3] - c[0]).normalize_or_zero();
        if normal.z <= 1e-4 {
            continue;
        }
        let shade = 0.5_f32 + 0.5_f32 * normal.dot(light).max(0.0_f32);
        let [u, v, w, h] = q.uv;
        let (cols, rows) = (w.round().max(1.0_f32) as usize, h.round().max(1.0_f32) as usize);
        // Un membre plié au coude ou au genou donne des faces en éventail : chaque texel suit les
        // quatre coins de la face, pas seulement deux de ses côtés
        let at = |i: usize, j: usize| {
            let (a, b) = (i as f32 / cols as f32, j as f32 / rows as f32);
            c[0].lerp(c[1], a).lerp(c[3].lerp(c[2], a), b)
        };
        for j in 0..rows {
            for i in 0..cols {
                // Rang j compté depuis le bas de la face, le haut de la planche en v
                let cell = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
                let uv = [u + i as f32 * w / cols as f32, v + (rows - 1 - j) as f32 * h / rows as f32, w / cols as f32, h / rows as f32];
                let depth = (cell[0] + cell[1] + cell[2] + cell[3]).z * 0.25_f32;
                quads.push((depth, cell, uv, shade));
            }
        }
    }
    quads.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut mesh = egui::Mesh::with_texture(texture.id());
    let sheet = SHEET_SIZE as f32;
    let margin = 0.01_f32;
    for (_, c, [u, v, w, h], shade) in &quads {
        let g = (255.0_f32 * shade) as u8;
        let color = egui::Color32::from_rgb(g, g, g);
        let (u0, v0, u1, v1) = ((u + margin) / sheet, (v + margin) / sheet, (u + w - margin) / sheet, (v + h - margin) / sheet);
        let uvs = [egui::pos2(u0, v1), egui::pos2(u1, v1), egui::pos2(u1, v0), egui::pos2(u0, v0)];
        let base = mesh.vertices.len() as u32;
        for (p, uv) in c.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex { pos: egui::pos2(feet.x + p.x * units, feet.y - p.y * units), uv, color });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/* -------------------------------------------------------------------- petits éléments */

fn metres(m: f64) -> String {
    format!("{:.2} m", m).replace('.', ",")
}

fn options<T: hero::atlas::Choix>(liste: &[T]) -> Vec<(String, String)> {
    liste.iter().map(|o| (o.id().to_string(), o.nom().to_string())).collect()
}

/// Mur de planches derrière les panneaux
fn paint_planks(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 0.0_f32, egui::Color32::from_rgb(40, 28, 19));
    let plank = 46.0_f32;
    let mut y = rect.top();
    let mut i = 0u32;
    while y < rect.bottom() {
        let tint = [(46, 32, 22), (38, 26, 18), (43, 30, 20), (35, 24, 16)][(i % 4) as usize];
        let r = egui::Rect::from_min_max(egui::pos2(rect.left(), y), egui::pos2(rect.right(), (y + plank).min(rect.bottom())));
        painter.rect_filled(r, 0.0_f32, egui::Color32::from_rgb(tint.0, tint.1, tint.2));
        painter.line_segment([r.left_bottom(), r.right_bottom()], egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(22, 15, 10)));
        // Joints décalés d'une planche à l'autre
        let offset = if i.is_multiple_of(2) { 0.0_f32 } else { 170.0_f32 };
        let mut x = rect.left() + offset + 120.0_f32;
        while x < rect.right() {
            painter.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(26, 18, 12)));
            x += 340.0_f32;
        }
        y += plank;
        i += 1;
    }
    painter.rect_filled(rect, 0.0_f32, egui::Color32::from_black_alpha(70));
}

fn panel_frame(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 8.0_f32, PANEL);
    painter.rect_stroke(rect, 8.0_f32, egui::Stroke::new(1.5_f32, GOLD.gamma_multiply(0.7_f32)), egui::StrokeKind::Middle);
}

fn well_frame() -> egui::Frame {
    egui::Frame::NONE.fill(WELL.gamma_multiply(0.9_f32)).stroke(egui::Stroke::new(1.0_f32, EDGE)).corner_radius(6.0_f32).inner_margin(12.0_f32)
}

/// Filet doré orné d'un losange central
fn ornament(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0_f32), egui::Sense::hover());
    let painter = ui.painter();
    let (y, c) = (rect.center().y, rect.center().x);
    let line = egui::Stroke::new(1.0_f32, GOLD.gamma_multiply(0.6_f32));
    painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(c - 8.0_f32, y)], line);
    painter.line_segment([egui::pos2(c + 8.0_f32, y), egui::pos2(rect.right(), y)], line);
    let d = 4.0_f32;
    painter.add(egui::Shape::convex_polygon(
        vec![egui::pos2(c, y - d), egui::pos2(c + d, y), egui::pos2(c, y + d), egui::pos2(c - d, y)],
        GOLD_BRIGHT,
        egui::Stroke::NONE,
    ));
}

fn rubric(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0_f32);
    ui.label(egui::RichText::new(text.to_uppercase()).size(11.0_f32).strong().color(GOLD));
}

fn note(ui: &mut egui::Ui, text: &str) {
    if !text.is_empty() {
        ui.label(egui::RichText::new(text).size(11.5_f32).italics().color(INK_DIM));
    }
}

fn sheet_line(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0_f32, 16.0_f32), egui::Sense::hover());
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, key.to_uppercase(), egui::FontId::proportional(10.5_f32), INK_DIM);
        ui.add(egui::Label::new(egui::RichText::new(value).size(13.0_f32).color(INK)).wrap());
    });
}

/// En-tête d'un réglage : son nom à gauche, sa valeur à droite
fn header(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.add_space(4.0_f32);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label.to_uppercase()).size(11.0_f32).strong().color(GOLD));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).size(12.0_f32).color(INK_SOFT));
        });
    });
}

/// Liste parcourue avec ‹ et ›, le sélecteur de la maquette : le choix, et sa place dans la liste
fn selector(ui: &mut egui::Ui, label: &str, liste: &[(String, String)], current: &mut String) -> bool {
    if liste.is_empty() {
        return false;
    }
    let i = liste.iter().position(|(id, _)| id == current).unwrap_or(0);
    header(ui, label, &format!("{} / {}", i + 1, liste.len()));
    let mut next = None;
    ui.horizontal(|ui| {
        let arrow = |ui: &mut egui::Ui, text: &str| ui.add(egui::Button::new(egui::RichText::new(text).size(16.0_f32).strong().color(GOLD_BRIGHT)).fill(CASE).min_size(egui::vec2(34.0_f32, 30.0_f32)));
        if arrow(ui, "‹").clicked() {
            next = Some((i + liste.len() - 1) % liste.len());
        }
        let width = (ui.available_width() - 42.0_f32).max(60.0_f32);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0_f32), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0_f32, WELL);
        ui.painter().rect_stroke(rect, 4.0_f32, egui::Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Middle);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, &liste[i].1, egui::FontId::proportional(14.5_f32), INK);
        if arrow(ui, "›").clicked() {
            next = Some((i + 1) % liste.len());
        }
    });
    match next {
        Some(n) => {
            *current = liste[n].0.clone();
            true
        }
        None => false,
    }
}

fn slider(ui: &mut egui::Ui, label: &str, value_text: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {
    header(ui, label, value_text);
    ui.spacing_mut().slider_width = ui.available_width() - 8.0_f32;
    ui.add(egui::Slider::new(value, range).show_value(false)).changed()
}

/// Nuancier : une pastille par couleur, la choisie cerclée d'or
fn swatches(ui: &mut egui::Ui, label: &str, palette: &[String], current: &mut String) -> bool {
    header(ui, label, &current.to_uppercase());
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for hex in palette {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0_f32, 28.0_f32), egui::Sense::click());
            let c = hero::painter::rgb(hex);
            let fill = egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
            let on = hex.eq_ignore_ascii_case(current);
            ui.painter().rect_filled(rect.shrink(3.0_f32), 4.0_f32, fill);
            let stroke = if on {
                egui::Stroke::new(2.0_f32, GOLD_BRIGHT)
            } else if response.hovered() {
                egui::Stroke::new(1.0_f32, INK_SOFT)
            } else {
                egui::Stroke::new(1.0_f32, EDGE)
            };
            ui.painter().rect_stroke(rect, 5.0_f32, stroke, egui::StrokeKind::Middle);
            if response.clicked() && !on {
                *current = hex.clone();
                changed = true;
            }
        }
    });
    changed
}

/// Grille de `columns` colonnes de cases de même largeur
fn grid(ui: &mut egui::Ui, columns: usize, count: usize, mut cell: impl FnMut(&mut egui::Ui, usize, f32)) {
    let spacing = 8.0_f32;
    let width = ((ui.available_width() - spacing * (columns - 1) as f32) / columns as f32).max(80.0_f32);
    for row in 0..count.div_ceil(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for col in 0..columns {
                let i = row * columns + col;
                if i < count {
                    cell(ui, i, width);
                }
            }
        });
    }
}

/// Case cochable de la maquette : son nom, et un détail à droite
fn case(ui: &mut egui::Ui, width: f32, title: &str, detail: Option<&str>, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 34.0_f32), egui::Sense::click());
    let fill = if on {
        CASE_ON
    } else if response.hovered() {
        CASE_HOVER
    } else {
        CASE
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0_f32, fill);
    painter.rect_stroke(rect, 4.0_f32, egui::Stroke::new(if on { 1.5_f32 } else { 1.0_f32 }, if on { GOLD } else { EDGE }), egui::StrokeKind::Middle);
    let check = egui::Rect::from_center_size(egui::pos2(rect.left() + 16.0_f32, rect.center().y), egui::vec2(12.0_f32, 12.0_f32));
    painter.rect_stroke(check, 2.0_f32, egui::Stroke::new(1.0_f32, if on { GOLD_BRIGHT } else { INK_DIM }), egui::StrokeKind::Middle);
    if on {
        painter.rect_filled(check.shrink(3.0_f32), 1.0_f32, GOLD_BRIGHT);
    }
    painter.text(egui::pos2(rect.left() + 30.0_f32, rect.center().y), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(14.0_f32), if on { GOLD_BRIGHT } else { INK });
    if let Some(detail) = detail {
        painter.text(egui::pos2(rect.right() - 10.0_f32, rect.center().y), egui::Align2::RIGHT_CENTER, detail, egui::FontId::proportional(12.0_f32), INK_DIM);
    }
    response
}

/// Case à cocher dessinée (Rotation de l'aperçu)
fn paint_check(painter: &egui::Painter, rect: egui::Rect, label: &str, on: bool, hovered: bool) {
    painter.rect_filled(rect, 4.0_f32, if hovered { CASE_HOVER } else { CASE.gamma_multiply(0.8_f32) });
    painter.rect_stroke(rect, 4.0_f32, egui::Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Middle);
    let check = egui::Rect::from_center_size(egui::pos2(rect.left() + 13.0_f32, rect.center().y), egui::vec2(11.0_f32, 11.0_f32));
    painter.rect_stroke(check, 2.0_f32, egui::Stroke::new(1.0_f32, if on { GOLD_BRIGHT } else { INK_DIM }), egui::StrokeKind::Middle);
    if on {
        painter.rect_filled(check.shrink(2.5_f32), 1.0_f32, GOLD_BRIGHT);
    }
    painter.text(egui::pos2(rect.left() + 25.0_f32, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5_f32), INK_SOFT);
}

fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(text).size(13.0_f32).color(GOLD_BRIGHT)).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0_f32, GOLD.gamma_multiply(0.7_f32))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(creator: &mut HeroCreator, ctx: &egui::Context) -> Option<CreatorEvent> {
        let mut event = None;
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))), ..Default::default() };
        ctx.run_pass(input, |ctx| event = creator.show(ctx)).textures_delta.clear();
        event
    }

    #[test]
    fn creator_draws_every_step_and_tab() {
        let ctx = egui::Context::default();
        let mut creator = HeroCreator::new();
        for (tab, _) in Tab::ALL {
            creator.tab = tab;
            assert_eq!(run(&mut creator, &ctx), None);
        }
        creator.class = Some(PlayerClass::Soldier);
        creator.go(1);
        run(&mut creator, &ctx);
        creator.go(2);
        assert_eq!(creator.step, 2);
        run(&mut creator, &ctx);
    }

    #[test]
    fn the_name_step_waits_for_a_class() {
        let mut creator = HeroCreator::new();
        creator.go(2);
        assert_eq!(creator.step, 1);
    }

    #[test]
    fn sealing_checks_the_name_then_freezes_the_creator() {
        let mut creator = HeroCreator::new();
        creator.class = Some(PlayerClass::Berserker);
        assert_eq!(creator.seal(), None);
        assert!(creator.refusal.is_some());
        creator.name = "  Torgar le Rouge ".into();
        let Some(CreatorEvent::Sealed(legend)) = creator.seal() else { panic!("baptême refusé : {:?}", creator.refusal) };
        assert_eq!(legend.appearance.nom, "Torgar le Rouge");
        assert_eq!(legend.class, PlayerClass::Berserker);
        creator.go(0);
        assert_eq!(creator.step, 0, "une fiche scellée ne bouge plus");
    }

    /// Planche des héros en relief, rendue sur le processeur : une case par variante, de trois
    /// quarts puis de dos.
    /// `HEROCRAFT_CAPTURES=dossier cargo test -p herocraft_client snapshot_figures -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_figures() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let a = hero::atlas();
        let mut heroes = Vec::new();
        for id in &a.ORDRE_ESPECES {
            for v in 0..a.espece(id).variantes.len() {
                let mut cfg = a.defaut(id);
                cfg.variante = v as i32;
                cfg.retouch(true);
                heroes.push(cfg);
            }
        }
        let close: Vec<Appearance> = [("humain", 1), ("nain", 2), ("hautElfe", 3), ("hommeBete", 2), ("aasimar", 0)]
            .iter()
            .map(|(id, v)| heroes.iter().find(|c| c.espece == *id && c.variante == *v).cloned().expect("variante connue"))
            .collect();
        let all = heroes.clone();
        for (name, angle, heroes, cols, cell) in [
            ("figures-trois-quarts", 46.0_f32, &all, 10_usize, egui::vec2(150.0, 230.0)),
            ("figures-dos", 200.0, &all, 10, egui::vec2(150.0, 230.0)),
            ("figures-gros-plan", 30.0, &close, 5, egui::vec2(320.0, 480.0)),
            ("figures-gros-plan-dos", 215.0, &close, 5, egui::vec2(320.0, 480.0)),
            ("figures-profil-gauche", 90.0, &close, 5, egui::vec2(320.0, 480.0)),
            ("figures-profil-droit", 270.0, &close, 5, egui::vec2(320.0, 480.0)),
        ] {
            let rows = heroes.len().div_ceil(cols);
            let (width, height) = ((cols as f32 * cell.x) as usize, (rows as f32 * cell.y) as usize);
            let ctx = egui::Context::default();
            let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
            snapshot.set_background(None);
            let mut previews: Vec<Preview> = heroes.iter().map(|_| Preview::default()).collect();
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32))), ..Default::default() };
            let mut output = ctx.run_ui(input, |root| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
                    for (i, (cfg, preview)) in heroes.iter().zip(previews.iter_mut()).enumerate() {
                        let min = egui::pos2((i % cols) as f32 * cell.x, (i / cols) as f32 * cell.y);
                        let rect = egui::Rect::from_min_size(min, cell).shrink(4.0);
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(58, 46, 36));
                        let texture = preview.texture(ui.ctx(), cfg);
                        let figure = preview.figure.get(cfg);
                        draw_figure(ui.painter(), rect, &texture, cfg, figure, angle.to_radians(), (-4.0_f32).to_radians());
                    }
                });
            });
            snapshot.apply_textures(&mut output.textures_delta);
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            snapshot.paint(&primitives, output.pixels_per_point);
            snapshot.save(&dir.join(format!("{name}.png")));
        }
    }

    /// Un habitant de chaque race sans village encore, pour les aperçus de villages : une image
    /// par race, de trois quarts.
    /// `HEROCRAFT_CAPTURES=dossier cargo test -p herocraft_client snapshot_habitants_races -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_habitants_races() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let a = hero::atlas();
        let mut seed = 0x5EED_2026_u64;
        let mut rng = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for (id, v) in [("demiOrque", 0), ("revenant", 3), ("sylphe", 1), ("gnome", 0), ("aasimar", 0), ("hommeBete", 0)] {
            let mut cfg = a.aleatoire(&mut rng, Some(id));
            cfg.variante = v;
            cfg.retouch(true);
            let cell = egui::vec2(320.0, 480.0);
            let (width, height) = (cell.x as usize, cell.y as usize);
            let ctx = egui::Context::default();
            let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
            snapshot.set_background(None);
            let mut preview = Preview::default();
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, cell)), ..Default::default() };
            let mut output = ctx.run_ui(input, |root| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
                    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, cell).shrink(4.0);
                    let texture = preview.texture(ui.ctx(), &cfg);
                    let figure = preview.figure.get(&cfg);
                    draw_figure(ui.painter(), rect, &texture, &cfg, figure, 40.0_f32.to_radians(), (-4.0_f32).to_radians());
                });
            });
            snapshot.apply_textures(&mut output.textures_delta);
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            snapshot.paint(&primitives, output.pixels_per_point);
            snapshot.save(&dir.join(format!("habitant-{id}.png")));
        }
    }

    /// Planche des articulations : quelques héros dans des poses qui plient coudes, poignets et
    /// genoux, de profil puis de trois quarts.
    /// `HEROCRAFT_CAPTURES=dossier cargo test -p herocraft_client snapshot_joints -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_joints() {
        use crate::hero_model::Bone;
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let a = hero::atlas();
        let heroes: Vec<Appearance> = [("humain", 1), ("nain", 2), ("hommeBete", 2)]
            .iter()
            .map(|(id, v)| {
                let mut cfg = a.defaut(id);
                cfg.variante = *v;
                cfg.retouch(true);
                cfg
            })
            .collect();
        let pose = |bones: &[(Bone, [f32; 3])], down: f32| {
            let mut pose = FigurePose::default();
            for (bone, r) in bones {
                pose.bone_mut(*bone).rotation = bone.rotation_from_degrees(*r);
            }
            pose.bone_mut(Bone::Body).translation = glam::vec3(0.0, -down, 0.0);
            pose
        };
        let poses = [
            ("repos", pose(&[], 0.0)),
            ("estoc", pose(&[(Bone::RightArm, [40.0, 0.0, 0.0]), (Bone::RightForearm, [70.0, 0.0, 0.0]), (Bone::LeftArm, [20.0, 0.0, 10.0]), (Bone::LeftForearm, [100.0, 0.0, 0.0])], 0.0)),
            ("accroupi", pose(&[(Bone::RightLeg, [70.0, 0.0, 0.0]), (Bone::LeftLeg, [70.0, 0.0, 0.0]), (Bone::RightShin, [110.0, 0.0, 0.0]), (Bone::LeftShin, [110.0, 0.0, 0.0]), (Bone::Torso, [25.0, 0.0, 0.0]), (Bone::RightArm, [50.0, 0.0, 0.0]), (Bone::LeftArm, [50.0, 0.0, 0.0]), (Bone::RightForearm, [40.0, 0.0, 0.0]), (Bone::LeftForearm, [40.0, 0.0, 0.0])], 6.0)),
            ("frappe", pose(&[(Bone::RightArm, [160.0, 0.0, 10.0]), (Bone::RightForearm, [60.0, 0.0, 0.0]), (Bone::RightHand, [40.0, 0.0, 0.0]), (Bone::Torso, [-10.0, 25.0, 0.0]), (Bone::Head, [0.0, -20.0, 0.0]), (Bone::LeftLeg, [35.0, 0.0, 0.0]), (Bone::LeftShin, [45.0, 0.0, 0.0])], 0.0)),
            ("incantation", pose(&[(Bone::RightArm, [60.0, 0.0, 15.0]), (Bone::LeftArm, [60.0, 0.0, 15.0]), (Bone::RightForearm, [80.0, 30.0, 0.0]), (Bone::LeftForearm, [80.0, -30.0, 0.0]), (Bone::Head, [15.0, 0.0, 0.0])], 0.0)),
        ];
        let cell = egui::vec2(260.0, 400.0);
        for (name, angle) in [("articulations-profil", 90.0_f32), ("articulations-trois-quarts", 40.0)] {
            let cols = poses.len();
            let (width, height) = ((cols as f32 * cell.x) as usize, (heroes.len() as f32 * cell.y) as usize);
            let ctx = egui::Context::default();
            let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
            snapshot.set_background(None);
            let mut previews: Vec<Preview> = heroes.iter().map(|_| Preview::default()).collect();
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32))), ..Default::default() };
            let mut output = ctx.run_ui(input, |root| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
                    for (row, (cfg, preview)) in heroes.iter().zip(previews.iter_mut()).enumerate() {
                        let texture = preview.texture(ui.ctx(), cfg);
                        let figure = preview.figure.get(cfg).clone();
                        for (col, (label, pose)) in poses.iter().enumerate() {
                            let min = egui::pos2(col as f32 * cell.x, row as f32 * cell.y);
                            let rect = egui::Rect::from_min_size(min, cell).shrink(4.0);
                            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(58, 46, 36));
                            draw_posed_figure(ui.painter(), rect, &texture, cfg, &figure, pose, angle.to_radians(), (-4.0_f32).to_radians());
                            ui.painter().text(rect.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, *label, egui::FontId::proportional(18.0), egui::Color32::from_rgb(235, 220, 190));
                        }
                    }
                });
            });
            snapshot.apply_textures(&mut output.textures_delta);
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            snapshot.paint(&primitives, output.pixels_per_point);
            snapshot.save(&dir.join(format!("{name}.png")));
        }
    }
}

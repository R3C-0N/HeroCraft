//! Écran de la roue des compétences (touche C) : les quinze classes se partagent le cercle,
//! chacune de sa couleur (elles font le tour des teintes, de la magie à la distance puis à la
//! mêlée) ; leurs compétences sont posées sur quatre anneaux, du niveau 1 au
//! centre jusqu'à l'ultime au bord, reliées à celles qu'elles demandent.
//!
//! Cent quatre-vingts compétences ne se lisent pas d'un coup : la roue fait loupe sur une classe.
//! Celle-ci tourne en haut et prend près de la moitié du cercle, ses compétences grossies et
//! nommées ; les autres se resserrent en fines parts, où l'on voit encore ce qui est appris. Un
//! clic sur une part, la molette ou les flèches font tourner la roue ; un clic sur une compétence
//! ouvre sa fiche, à droite, d'où l'on y investit un point.

use std::f32::consts::{PI, TAU};

use herocraft_common::skills::{rank, SkillNode};
use herocraft_common::{CharacterStats, SkillWheel};

use crate::ui::{GOLD, GOLD_BRIGHT, MENU_PADDING, TEXT_DIM, TEXT_SOFT};

/// Diamètre de la roue, en points d'écran : la hauteur de l'écran moins l'en-tête, dans ces bornes
const WHEEL_MIN_SIZE: f32 = 380.0;
const WHEEL_MAX_SIZE: f32 = 760.0;
/// Rayon d'une compétence dans une part resserrée, et ce qu'elle gagne sous la loupe
/// (l'ultime est une fois et demie plus gros)
const NODE_RADIUS: f32 = 3.5;
const NODE_RADIUS_FOCUS: f32 = 7.5;
/// Angle de la classe sous la loupe ; les autres se partagent le reste
const FOCUS_SPAN: f32 = 150.0 * PI / 180.0;
/// Durée de la rotation de la roue, en secondes
const TURN_SECS: f32 = 0.25;
/// Distance au pointeur sous laquelle une compétence est survolée
const PICK_RADIUS: f32 = 9.0;
/// Défilement de molette (en points) pour passer à la classe voisine
const SCROLL_STEP: f32 = 40.0;
/// Largeur de la fiche à droite de la roue
const SHEET_WIDTH: f32 = 250.0;
/// Expérience que donne le bouton d'essai
pub const TEST_EXPERIENCE: u64 = 100;

/// Ce que le joueur a demandé sur l'écran, à appliquer par le jeu
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SkillScreenRequests {
    /// Investir un point dans cette compétence
    pub learn: Option<String>,
    /// Se donner de l'expérience (bouton d'essai du solo)
    pub experience: u64,
    /// Poser cette compétence dans cette case de la barre
    pub place: Option<(usize, String)>,
}

/// La barre de compétences, telle que la fiche la montre : ce qui est posé, et les touches
pub struct BarView<'a> {
    pub bar: &'a crate::skill_bar::SkillBar,
    pub icons: &'a crate::skill_bar::SkillIcons,
    pub keys: [String; crate::skill_bar::SKILL_SLOTS],
}

/// État propre à l'écran : la classe sous la loupe et la compétence dont la fiche est ouverte
#[derive(Debug, Default)]
pub struct SkillScreen {
    pub selected: Option<String>,
    /// Classe sous la loupe (rang dans la roue)
    pub focus: usize,
    /// Position de la loupe sans retour à zéro : la roue tourne par le plus court chemin, et
    /// d'un tour complet plutôt qu'à rebours quand on passe de la dernière classe à la première
    track: i32,
    /// Défilement de molette pas encore converti en rotation
    scroll: f32,
}

impl SkillScreen {
    /// Met la loupe sur la classe `index`, en tournant par le plus court chemin
    pub fn focus_on(&mut self, index: usize, count: usize) {
        if count == 0 {
            return;
        }
        let n = count as i32;
        let mut delta = (index as i32 - self.focus as i32).rem_euclid(n);
        if delta > n / 2 {
            delta -= n;
        }
        self.track += delta;
        self.focus = self.track.rem_euclid(n) as usize;
    }

    /// Tourne la roue de `steps` classes (positif : vers la classe suivante)
    pub fn turn(&mut self, steps: i32, count: usize) {
        if count == 0 {
            return;
        }
        self.track += steps;
        self.focus = self.track.rem_euclid(count as i32) as usize;
    }

    /// Dessine l'écran. `allow_test_experience` montre le bouton qui donne de l'expérience (solo).
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        wheel: &SkillWheel,
        stats: &CharacterStats,
        close_hint: &str,
        allow_test_experience: bool,
        bar: &BarView,
    ) -> SkillScreenRequests {
        let mut requests = SkillScreenRequests::default();
        if self.selected.as_deref().is_some_and(|id| wheel.node(id).is_none()) {
            self.selected = None;
        }
        let count = wheel.branches.len();
        let (left, right) = ctx.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
        if left {
            self.turn(-1, count);
        }
        if right {
            self.turn(1, count);
        }
        egui::Area::new(egui::Id::new("skills_screen"))
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_premultiplied(14, 18, 28, 245))
                    .stroke(egui::Stroke::new(1.5_f32, GOLD))
                    .rounding(egui::Rounding::same(10.0_f32))
                    .inner_margin(egui::Margin::same(MENU_PADDING))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("COMPÉTENCES").strong().size(16.0_f32).color(GOLD_BRIGHT));
                            ui.add_space(12.0_f32);
                            let points = stats.skill_points;
                            let color = if points > 0 { GOLD_BRIGHT } else { TEXT_DIM };
                            ui.label(egui::RichText::new(format!("{points} point{} à investir", if points > 1 { "s" } else { "" })).color(color));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("[{close_hint}] ou [Échap] pour fermer")).size(11.0_f32).color(TEXT_DIM));
                            });
                        });
                        ui.add_space(6.0_f32);
                        ui.horizontal_top(|ui| {
                            let size = (ctx.screen_rect().height() - 2.0 * MENU_PADDING - 60.0).clamp(WHEEL_MIN_SIZE, WHEEL_MAX_SIZE);
                            self.draw_wheel(ui, wheel, stats, size);
                            ui.add_space(12.0_f32);
                            ui.vertical(|ui| {
                                ui.set_width(SHEET_WIDTH);
                                draw_progress(ui, stats);
                                if allow_test_experience {
                                    let button = egui::Button::new(format!("+{TEST_EXPERIENCE} xp (essai)"));
                                    if ui.add(button).on_hover_text("Donne de l'expérience au héros, le temps que l'expérience se gagne en jeu").clicked() {
                                        requests.experience += TEST_EXPERIENCE;
                                    }
                                }
                                ui.add_space(10.0_f32);
                                ui.separator();
                                ui.add_space(6.0_f32);
                                self.draw_class_header(ui, wheel, stats);
                                ui.add_space(6.0_f32);
                                ui.separator();
                                ui.add_space(6.0_f32);
                                match self.selected.as_deref().and_then(|id| wheel.node(id)) {
                                    Some(node) => {
                                        let (learn, place) = draw_sheet(ui, wheel, stats, node, bar);
                                        requests.learn = learn;
                                        requests.place = place;
                                    }
                                    None => {
                                        ui.label(egui::RichText::new("Choisissez une compétence sur la roue pour voir sa fiche.").color(TEXT_DIM));
                                    }
                                }
                            });
                        });
                    });
            });
        requests
    }

    /// Classe sous la loupe : nom, famille, ce qui en est appris, et comment tourner la roue
    fn draw_class_header(&mut self, ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats) {
        let count = wheel.branches.len();
        let Some(branch) = wheel.branches.get(self.focus) else { return };
        let [r, g, b] = branch.color;
        ui.horizontal(|ui| {
            if ui.small_button("◀").on_hover_text("Classe précédente (flèche gauche)").clicked() {
                self.turn(-1, count);
            }
            ui.label(egui::RichText::new(&branch.name).strong().size(14.0_f32).color(egui::Color32::from_rgb(r, g, b).gamma_multiply(1.3)));
            if ui.small_button("▶").on_hover_text("Classe suivante (flèche droite)").clicked() {
                self.turn(1, count);
            }
        });
        let [melee, ranged, magic] = branch.mix;
        let nodes = wheel.nodes.iter().filter(|n| n.branch == branch.id);
        let (total, learned) = nodes.fold((0, 0), |(t, l), n| (t + 1, l + usize::from(rank(stats, &n.id) > 0)));
        ui.label(egui::RichText::new(format!("Mêlée {melee} · distance {ranged} · magie {magic}")).size(11.0_f32).color(TEXT_SOFT));
        ui.label(egui::RichText::new(format!("{learned}/{total} apprises")).size(11.0_f32).color(TEXT_DIM));
        ui.label(egui::RichText::new("Molette, flèches ou clic sur une part pour tourner la roue").size(10.5_f32).color(TEXT_DIM));
    }

    fn draw_wheel(&mut self, ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats, size: f32) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
        let count = wheel.branches.len();
        if count == 0 {
            return;
        }
        // Molette sur la roue : une classe par cran
        if response.hovered() {
            self.scroll += ui.input(|i| i.raw_scroll_delta.y + i.raw_scroll_delta.x);
            while self.scroll.abs() >= SCROLL_STEP {
                let step = self.scroll.signum();
                self.turn(-(step as i32), count);
                self.scroll -= step * SCROLL_STEP;
            }
        } else {
            self.scroll = 0.0;
        }

        let ctx = ui.ctx().clone();
        let weights: Vec<f32> = (0..count)
            .map(|i| ctx.animate_bool_with_time(egui::Id::new(("skills_focus", i)), i == self.focus, TURN_SECS))
            .collect();
        let track = ctx.animate_value_with_time(egui::Id::new("skills_track"), self.track as f32, TURN_SECS);
        let painter = ui.painter_at(rect);
        let layout = WheelLayout::new(rect.center(), size / 2.0 - 2.0, wheel.rings(), &weights, track);
        let (centre, outer, hub) = (layout.centre, layout.outer, layout.hub_radius());

        // Fond : une part par classe, teintée de sa famille, puis les anneaux des paliers
        painter.circle_filled(centre, outer, egui::Color32::from_rgb(20, 26, 38));
        for (b, branch) in wheel.branches.iter().enumerate() {
            let [r, g, bl] = branch.color;
            let w = weights[b];
            let alpha = if b % 2 == 0 { 30.0 } else { 18.0 } + 14.0 * w;
            let tint = egui::Color32::from_rgba_unmultiplied(r, g, bl, alpha as u8);
            let (start, end) = layout.sector(b);
            painter.add(sector_shape(centre, hub, outer, start, end, tint));
            painter.line_segment([layout.point(start, hub), layout.point(start, outer)], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(22)));
            // Nom de la classe le long du bord, jamais à l'envers ; sur deux lignes ou plus quand
            // sa part resserrée est trop étroite pour lui (« Sorcier élémentaire »)
            let mid = (start + end) / 2.0;
            let color = egui::Color32::from_rgb(r, g, bl).gamma_multiply(1.3);
            let font = egui::FontId::proportional(11.0 + 3.0 * w);
            let room = (end - start) * (outer - 12.0) * 0.9;
            let lines = wrap_words(&branch.name, room, |text| painter.layout_no_wrap(text.to_string(), font.clone(), color).size().x);
            let flipped = mid.sin() > 0.0;
            let angle = if flipped { mid - PI / 2.0 } else { mid + PI / 2.0 };
            let (sin, cos) = angle.sin_cos();
            let line_height = font.size + 2.0;
            for (i, line) in lines.iter().enumerate() {
                // La première ligne se lit en premier : au bord en haut de la roue, vers le centre
                // en bas, où le texte est retourné
                let step = if flipped { lines.len() - 1 - i } else { i };
                let galley = painter.layout_no_wrap(line.clone(), font.clone(), color);
                let half = galley.size() / 2.0;
                let at = layout.point(mid, outer - 12.0 - line_height * step as f32);
                let corner = at - egui::vec2(half.x * cos - half.y * sin, half.x * sin + half.y * cos);
                painter.add(egui::epaint::TextShape::new(corner, galley, color).with_angle(angle));
            }
        }
        for tier in 1..=4 {
            painter.circle_stroke(centre, layout.ring_radius(tier as f32), egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(12)));
        }
        painter.circle_stroke(centre, outer, egui::Stroke::new(1.5_f32, GOLD));

        // Moyeu : niveau du héros et points à investir
        painter.circle_filled(centre, hub, egui::Color32::from_rgb(28, 34, 48));
        painter.circle_stroke(centre, hub, egui::Stroke::new(1.5_f32, GOLD));
        painter.text(centre - egui::vec2(0.0, 9.0), egui::Align2::CENTER_CENTER, format!("Niv. {}", stats.level), egui::FontId::proportional(15.0_f32), GOLD_BRIGHT);
        let points = stats.skill_points;
        painter.text(
            centre + egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            format!("{points} point{}", if points > 1 { "s" } else { "" }),
            egui::FontId::proportional(11.0_f32),
            if points > 0 { GOLD_BRIGHT } else { TEXT_DIM },
        );

        // Position, rang de part et poids de loupe de chaque compétence
        let branch_of = |node: &SkillNode| wheel.branches.iter().position(|br| br.id == node.branch).unwrap_or(0);
        let position = |node: &SkillNode| {
            let (start, end) = layout.sector(branch_of(node));
            // Sous la loupe, les compétences se tiennent loin des bords de la part
            let w = weights[branch_of(node)];
            let margin = 0.06 * w;
            let slot = margin + (1.0 - 2.0 * margin) * node.slot.clamp(0.0, 1.0);
            layout.point(start + (end - start) * slot, layout.ring_radius(node.ring()))
        };
        let radius_of = |node: &SkillNode| {
            let r = NODE_RADIUS + (NODE_RADIUS_FOCUS - NODE_RADIUS) * weights[branch_of(node)];
            if node.is_ultimate() { r * 1.5 } else { r }
        };
        let selected = self.selected.as_deref().and_then(|id| wheel.node(id));

        // Liens des prérequis
        for node in &wheel.nodes {
            let w = weights[branch_of(node)];
            for required in node.requires.iter().filter_map(|r| wheel.node(r)) {
                let met = rank(stats, &required.id) > 0;
                let lit = selected.is_some_and(|s| s.id == node.id || s.id == required.id);
                let color = if lit {
                    GOLD_BRIGHT
                } else if met {
                    GOLD.gamma_multiply(0.8)
                } else {
                    egui::Color32::from_white_alpha((24.0 + 36.0 * w) as u8)
                };
                painter.line_segment([position(required), position(node)], egui::Stroke::new(if lit { 1.8_f32 } else { 1.0 + 0.5 * w }, color));
            }
        }

        // Compétence survolée : la plus proche du pointeur
        let pointer = response.hover_pos();
        let hovered = pointer.and_then(|p| {
            wheel
                .nodes
                .iter()
                .map(|n| (n, p.distance(position(n)) - radius_of(n)))
                .filter(|&(_, d)| d <= PICK_RADIUS - NODE_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(n, _)| n)
        });

        for node in &wheel.nodes {
            let at = position(node);
            let w = weights[branch_of(node)];
            let current = rank(stats, &node.id);
            let [r, g, b] = wheel.branch(&node.branch).map_or([180, 180, 180], |b| b.color);
            let family = egui::Color32::from_rgb(r, g, b);
            let radius = radius_of(node);
            let is_selected = selected.is_some_and(|s| s.id == node.id);
            let is_hovered = hovered.is_some_and(|h| h.id == node.id);
            let fill = if current > 0 {
                family
            } else if node.is_ultimate() {
                egui::Color32::from_rgb(30, 22, 48)
            } else {
                egui::Color32::from_rgb(26, 32, 44)
            };
            painter.circle_filled(at, radius, fill);
            let stroke = if is_selected || is_hovered {
                egui::Stroke::new(2.0_f32, GOLD_BRIGHT)
            } else if wheel.check(stats, &node.id).is_none() {
                egui::Stroke::new(1.2 + 0.6 * w, GOLD.gamma_multiply(0.5 + 0.5 * w))
            } else if current > 0 {
                egui::Stroke::new(1.2_f32, egui::Color32::WHITE.gamma_multiply(0.7))
            } else {
                egui::Stroke::new(1.0_f32, family.gamma_multiply(0.8))
            };
            painter.circle_stroke(at, radius, stroke);
            // Rangs au-delà du premier : un anneau de plus autour du point
            for extra in 1..current {
                painter.circle_stroke(at, radius + 2.0 * extra as f32, egui::Stroke::new(0.8_f32, family));
            }
            // Sous la loupe, le nom de la compétence sous son point
            if w > 0.3 {
                let alpha = ((w - 0.3) / 0.7).clamp(0.0, 1.0);
                let color = if current > 0 { egui::Color32::WHITE } else { TEXT_SOFT }.gamma_multiply(alpha);
                let galley = painter.layout(node.name.clone(), egui::FontId::proportional(10.0_f32), color, 84.0);
                let top = at + egui::vec2(-galley.size().x / 2.0, radius + 3.0 + 2.0 * current.saturating_sub(1) as f32);
                painter.galley(top, galley, color);
            }
        }

        // Survol d'une compétence : sa bulle ; d'une part resserrée : le nom de sa classe
        let hovered_sector = pointer.and_then(|p| layout.sector_at(p));
        if let Some(node) = hovered {
            let class = wheel.branch(&node.branch).map_or("", |b| b.name.as_str());
            response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(egui::RichText::new(&node.name).strong().color(GOLD_BRIGHT));
                ui.label(egui::RichText::new(format!("{class} · {} · rang {}/{}", node.tier_label(), rank(stats, &node.id), node.max_rank)).size(11.0_f32).color(TEXT_SOFT));
            });
        } else if let Some(b) = hovered_sector.filter(|&b| b != self.focus) {
            let branch = &wheel.branches[b];
            let learned = wheel.nodes.iter().filter(|n| n.branch == branch.id && rank(stats, &n.id) > 0).count();
            response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(egui::RichText::new(&branch.name).strong().color(GOLD_BRIGHT));
                ui.label(egui::RichText::new(format!("{learned} apprise{} · clic pour l'examiner", if learned > 1 { "s" } else { "" })).size(11.0_f32).color(TEXT_SOFT));
            });
        }
        if response.clicked() {
            match hovered {
                Some(node) => {
                    self.selected = Some(node.id.clone());
                    self.focus_on(branch_of(node), count);
                }
                None => {
                    if let Some(b) = hovered_sector {
                        self.focus_on(b, count);
                    }
                }
            }
        }
    }
}

/// Niveau, jauge d'expérience et points
fn draw_progress(ui: &mut egui::Ui, stats: &CharacterStats) {
    ui.label(egui::RichText::new(format!("Niveau {}", stats.level)).strong().color(TEXT_SOFT));
    let next = stats.exp_for_next_level().max(1);
    let fraction = (stats.experience as f32 / next as f32).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SHEET_WIDTH, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0_f32, egui::Color32::from_black_alpha(160));
    let mut filled = rect;
    filled.set_width(rect.width() * fraction);
    ui.painter().rect_filled(filled, 3.0_f32, GOLD);
    ui.label(egui::RichText::new(format!("{} / {} xp · un point par niveau", stats.experience, next)).size(11.0_f32).color(TEXT_DIM));
}

/// Fiche d'une compétence ; renvoie son identifiant si le joueur y investit un point
fn draw_sheet(ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats, node: &SkillNode, bar: &BarView) -> (Option<String>, Option<(usize, String)>) {
    let current = rank(stats, &node.id);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0_f32, 36.0_f32), egui::Sense::hover());
        bar.icons.paint(ui.painter(), rect, &node.id, current == 0);
        ui.label(egui::RichText::new(&node.name).strong().size(15.0_f32).color(GOLD_BRIGHT));
    });
    let branch = wheel.branch(&node.branch).map_or("", |b| b.name.as_str());
    ui.label(egui::RichText::new(format!("{branch} · {} · rang {current}/{}", node.tier_label(), node.max_rank)).size(11.0_f32).color(TEXT_DIM));
    ui.add_space(4.0_f32);
    if !node.description.is_empty() {
        ui.label(egui::RichText::new(&node.description).color(TEXT_SOFT));
    }
    if !node.requires.is_empty() {
        ui.add_space(4.0_f32);
        let names: Vec<String> = node
            .requires
            .iter()
            .map(|r| {
                let name = wheel.node(r).map_or(r.as_str(), |n| n.name.as_str());
                if rank(stats, r) > 0 { format!("✔ {name}") } else { format!("✖ {name}") }
            })
            .collect();
        ui.label(egui::RichText::new(format!("Demande : {}", names.join(", "))).size(11.0_f32).color(TEXT_DIM));
    }
    ui.add_space(8.0_f32);
    let error = wheel.check(stats, &node.id);
    let label = if current == 0 { "Apprendre (1 point)" } else { "Renforcer (1 point)" };
    let clicked = ui.add_enabled(error.is_none(), egui::Button::new(label)).clicked();
    if let Some(error) = error {
        let text = error.message();
        let mut chars = text.chars();
        let text = chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
        ui.label(egui::RichText::new(text).size(11.0_f32).color(TEXT_DIM));
    }
    let place = draw_placement(ui, stats, node, bar);
    (clicked.then(|| node.id.clone()), place)
}

/// Ce que le nœud fait lancer (lui-même, ou les compétences qu'il donne) et les boutons qui les
/// posent sur la barre ; une compétence sans règles est marquée « pas encore jouable »
fn draw_placement(ui: &mut egui::Ui, stats: &CharacterStats, node: &SkillNode, bar: &BarView) -> Option<(usize, String)> {
    let catalog = herocraft_common::skills();
    let ids: Vec<&str> = if node.grants.is_empty() { vec![node.id.as_str()] } else { node.grants.iter().map(String::as_str).collect() };
    let learned = rank(stats, &node.id) > 0;
    let mut place = None;
    ui.add_space(8.0_f32);
    for id in ids {
        let Some(def) = catalog.get(id) else { continue };
        ui.horizontal(|ui| {
            if !node.grants.is_empty() {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0_f32, 24.0_f32), egui::Sense::hover());
                bar.icons.paint(ui.painter(), rect, id, !learned);
                ui.label(egui::RichText::new(&def.name).size(12.0_f32).color(TEXT_SOFT)).on_hover_text(&def.description);
            }
            if !def.is_castable() {
                ui.label(egui::RichText::new("Pas encore jouable").italics().size(11.0_f32).color(TEXT_DIM))
                    .on_hover_text("Ses règles ne sont pas encore écrites : elle s'apprend, mais ne se lance pas encore.");
                return;
            }
            // Un mélange ne se pose pas : il se lance par la case de la Fusion
            if catalog.fusion_of_mix(id).is_some() {
                ui.label(egui::RichText::new("Se lance par la Fusion").italics().size(11.0_f32).color(TEXT_DIM))
                    .on_hover_text("Lance ses deux sorts élémentaires, puis la Fusion.");
                return;
            }
            if !learned {
                return;
            }
            ui.label(egui::RichText::new("Barre :").size(11.0_f32).color(TEXT_DIM));
            for (slot, key) in bar.keys.iter().enumerate() {
                let here = bar.bar.get(slot) == Some(id);
                let text = egui::RichText::new(key).size(11.0_f32).color(if here { GOLD_BRIGHT } else { TEXT_SOFT });
                let occupant = bar.bar.get(slot).and_then(|o| catalog.get(o)).map_or("vide".to_string(), |o| o.name.clone());
                if ui.add(egui::Button::new(text).selected(here).small()).on_hover_text(format!("Poser sur [{key}] (aujourd'hui : {occupant})")).clicked() {
                    place = Some((slot, id.to_string()));
                }
            }
        });
    }
    place
}

/// Coupe `text` en lignes de mots qui tiennent dans `room` (mesurées par `width`) ; un mot trop
/// long pour tenir seul garde sa ligne
fn wrap_words(text: &str, room: f32, width: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if width(&format!("{line} {word}")) <= room => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// Géométrie de la roue : la part de chaque classe (celle sous la loupe tournée en haut) et le
/// rayon des anneaux
struct WheelLayout {
    centre: egui::Pos2,
    outer: f32,
    rings: f32,
    /// Début et fin de chaque part, en radians
    sectors: Vec<(f32, f32)>,
}

impl WheelLayout {
    /// `weights` : la part de loupe de chaque classe (0 resserrée, 1 sous la loupe) ; `track` :
    /// la position de la loupe, fractionnaire pendant que la roue tourne
    fn new(centre: egui::Pos2, outer: f32, rings: f32, weights: &[f32], track: f32) -> Self {
        let n = weights.len().max(1);
        let thin = if n > 1 { (TAU - FOCUS_SPAN) / (n - 1) as f32 } else { TAU };
        let wide = if n > 1 { FOCUS_SPAN } else { TAU };
        let raw: Vec<f32> = weights.iter().map(|w| thin + (wide - thin) * w).collect();
        let scale = TAU / raw.iter().sum::<f32>().max(1e-6);
        let spans: Vec<f32> = raw.iter().map(|s| s * scale).collect();
        let mut starts = Vec::with_capacity(n);
        let mut at = 0.0;
        for span in &spans {
            starts.push(at);
            at += span;
        }
        // Milieu (sans retour à zéro) de la part de rang `k`, quel que soit le nombre de tours
        let middle = |k: i32| {
            let i = k.rem_euclid(n as i32) as usize;
            starts.get(i).map_or(0.0, |s| s + spans[i] / 2.0) + TAU * k.div_euclid(n as i32) as f32
        };
        let k = track.floor();
        let focus_middle = middle(k as i32) + (middle(k as i32 + 1) - middle(k as i32)) * (track - k);
        let rotation = -PI / 2.0 - focus_middle;
        let sectors = starts.iter().zip(&spans).map(|(s, span)| (s + rotation, s + rotation + span)).collect();
        Self { centre, outer, rings, sectors }
    }

    fn sector(&self, branch: usize) -> (f32, f32) {
        self.sectors.get(branch).copied().unwrap_or((0.0, TAU))
    }

    /// Part sous ce point (entre le moyeu et le bord)
    fn sector_at(&self, p: egui::Pos2) -> Option<usize> {
        let d = p - self.centre;
        if d.length() < self.hub_radius() || d.length() > self.outer {
            return None;
        }
        let angle = d.y.atan2(d.x);
        self.sectors.iter().position(|&(start, end)| {
            let rel = (angle - start).rem_euclid(TAU);
            rel < end - start
        })
    }

    fn hub_radius(&self) -> f32 {
        self.outer * 0.17
    }

    /// Les anneaux vont de 40 % du rayon jusqu'au nom des classes, au bord
    fn ring_radius(&self, ring: f32) -> f32 {
        let inner = self.outer * 0.40;
        let outer = self.outer - 44.0;
        if self.rings <= 1.0 {
            return (inner + outer) / 2.0;
        }
        inner + (outer - inner) * (ring.max(1.0) - 1.0) / (self.rings - 1.0)
    }

    fn point(&self, angle: f32, radius: f32) -> egui::Pos2 {
        self.centre + egui::vec2(angle.cos(), angle.sin()) * radius
    }
}

/// Part de couronne entre deux rayons et deux angles, en un seul maillage (sans liseré
/// d'anticrénelage entre les tranches)
fn sector_shape(centre: egui::Pos2, inner: f32, outer: f32, start: f32, end: f32, fill: egui::Color32) -> egui::Shape {
    let steps = ((end - start).abs() / 0.05).ceil().max(1.0) as u32;
    let mut mesh = egui::Mesh::default();
    for i in 0..=steps {
        let a = start + (end - start) * i as f32 / steps as f32;
        let dir = egui::vec2(a.cos(), a.sin());
        mesh.colored_vertex(centre + dir * inner, fill);
        mesh.colored_vertex(centre + dir * outer, fill);
    }
    for i in 0..steps {
        let k = 2 * i;
        mesh.add_triangle(k, k + 1, k + 3);
        mesh.add_triangle(k, k + 3, k + 2);
    }
    egui::Shape::mesh(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(focus: usize, n: usize) -> WheelLayout {
        let weights: Vec<f32> = (0..n).map(|i| if i == focus { 1.0 } else { 0.0 }).collect();
        WheelLayout::new(egui::Pos2::ZERO, 300.0, 4.0, &weights, focus as f32)
    }

    #[test]
    fn test_rings_stay_between_hub_and_labels() {
        let layout = layout(0, 15);
        assert!(layout.ring_radius(1.0) > layout.hub_radius() + NODE_RADIUS_FOCUS);
        assert!(layout.ring_radius(4.0) + 1.5 * NODE_RADIUS_FOCUS < 300.0 - 20.0, "l'ultime ne touche pas le nom de la classe");
        assert!(layout.ring_radius(1.0) < layout.ring_radius(2.0) && layout.ring_radius(2.0) < layout.ring_radius(4.0));
    }

    #[test]
    fn test_focused_class_is_wide_and_on_top() {
        for focus in [0, 7, 14] {
            let layout = layout(focus, 15);
            let (start, end) = layout.sector(focus);
            assert!((end - start - FOCUS_SPAN).abs() < 1e-4);
            assert!(((start + end) / 2.0 + PI / 2.0).rem_euclid(TAU).min((-(start + end) / 2.0 - PI / 2.0).rem_euclid(TAU)) < 1e-4, "en haut");
            let total: f32 = (0..15).map(|b| layout.sector(b)).map(|(s, e)| e - s).sum();
            assert!((total - TAU).abs() < 1e-4);
            // Le haut de la roue est dans la part sous la loupe
            assert_eq!(layout.sector_at(egui::pos2(0.0, -200.0)), Some(focus));
        }
    }

    #[test]
    fn test_long_class_names_wrap() {
        let width = |t: &str| t.chars().count() as f32 * 6.0;
        assert_eq!(wrap_words("Sorcier élémentaire", 70.0, width), vec!["Sorcier", "élémentaire"]);
        assert_eq!(wrap_words("Chasseur de monstre", 70.0, width), vec!["Chasseur de", "monstre"]);
        assert_eq!(wrap_words("Soldat", 70.0, width), vec!["Soldat"]);
        assert_eq!(wrap_words("Lame du vent", 200.0, width), vec!["Lame du vent"]);
    }

    #[test]
    fn test_wheel_turns_the_short_way() {
        let mut screen = SkillScreen::default();
        screen.focus_on(14, 15);
        assert_eq!((screen.focus, screen.track), (14, -1), "de la première à la dernière : un cran à rebours");
        screen.turn(1, 15);
        assert_eq!((screen.focus, screen.track), (0, 0));
        screen.focus_on(5, 15);
        assert_eq!((screen.focus, screen.track), (5, 5));
    }
}

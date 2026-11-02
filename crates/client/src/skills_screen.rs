//! Écran de la roue des compétences (touche C) : les quinze classes se partagent le cercle,
//! teintées de leur famille ; leurs compétences sont posées sur quatre anneaux, du niveau 1 au
//! centre jusqu'à l'ultime au bord, reliées à celles qu'elles demandent. Un clic choisit une
//! compétence ; sa fiche, à droite, permet d'y investir un point.

use std::f32::consts::{PI, TAU};

use herocraft_common::skills::{rank, SkillNode};
use herocraft_common::{CharacterStats, SkillWheel};

use crate::ui::{GOLD, GOLD_BRIGHT, MENU_PADDING, TEXT_DIM, TEXT_SOFT};

/// Diamètre de la roue, en points d'écran : la hauteur de l'écran moins l'en-tête, dans ces bornes
const WHEEL_MIN_SIZE: f32 = 380.0;
const WHEEL_MAX_SIZE: f32 = 760.0;
/// Rayon d'une compétence sur la roue (l'ultime est une fois et demie plus gros)
const NODE_RADIUS: f32 = 4.5;
/// Distance au pointeur sous laquelle une compétence est survolée
const PICK_RADIUS: f32 = 8.0;
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
}

/// État propre à l'écran : la compétence dont la fiche est ouverte
#[derive(Debug, Default)]
pub struct SkillScreen {
    pub selected: Option<String>,
}

impl SkillScreen {
    /// Dessine l'écran. `allow_test_experience` montre le bouton qui donne de l'expérience (solo).
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        wheel: &SkillWheel,
        stats: &CharacterStats,
        close_hint: &str,
        allow_test_experience: bool,
    ) -> SkillScreenRequests {
        let mut requests = SkillScreenRequests::default();
        if self.selected.as_deref().is_some_and(|id| wheel.node(id).is_none()) {
            self.selected = None;
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
                                match self.selected.as_deref().and_then(|id| wheel.node(id)) {
                                    Some(node) => {
                                        if let Some(id) = draw_sheet(ui, wheel, stats, node) {
                                            requests.learn = Some(id);
                                        }
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

    fn draw_wheel(&mut self, ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats, size: f32) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
        let painter = ui.painter_at(rect);
        let centre = rect.center();
        let outer = size / 2.0 - 2.0;
        let layout = WheelLayout { centre, outer, rings: wheel.rings(), branches: wheel.branches.len().max(1) };
        let hub = layout.hub_radius();

        // Fond : une part par classe, teintée de sa famille, puis les anneaux des paliers
        painter.circle_filled(centre, outer, egui::Color32::from_rgb(20, 26, 38));
        for (b, branch) in wheel.branches.iter().enumerate() {
            let [r, g, bl] = branch.color;
            // Une part sur deux un peu plus claire, pour distinguer les classes d'une même famille
            let tint = egui::Color32::from_rgba_unmultiplied(r, g, bl, if b % 2 == 0 { 30 } else { 18 });
            let (start, end) = layout.sector(b);
            painter.add(sector_shape(centre, hub, outer, start, end, tint));
            painter.line_segment([layout.point(start, hub), layout.point(start, outer)], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(22)));
            // Nom de la classe le long du bord, jamais à l'envers
            let mid = (start + end) / 2.0;
            let color = egui::Color32::from_rgb(r, g, bl).gamma_multiply(1.3);
            let galley = painter.layout_no_wrap(branch.name.clone(), egui::FontId::proportional(11.0_f32), color);
            let angle = if mid.sin() > 0.0 { mid - PI / 2.0 } else { mid + PI / 2.0 };
            let half = galley.size() / 2.0;
            let (sin, cos) = angle.sin_cos();
            let corner = layout.point(mid, outer - 11.0) - egui::vec2(half.x * cos - half.y * sin, half.x * sin + half.y * cos);
            painter.add(egui::epaint::TextShape::new(corner, galley, color).with_angle(angle));
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

        // Liens des prérequis, puis les compétences par-dessus
        let position = |node: &SkillNode| {
            let b = wheel.branches.iter().position(|br| br.id == node.branch).unwrap_or(0);
            let (start, end) = layout.sector(b);
            layout.point(start + (end - start) * node.slot.clamp(0.0, 1.0), layout.ring_radius(node.ring()))
        };
        let selected = self.selected.as_deref().and_then(|id| wheel.node(id));
        for node in &wheel.nodes {
            for required in node.requires.iter().filter_map(|r| wheel.node(r)) {
                let met = rank(stats, &required.id) > 0;
                let lit = selected.is_some_and(|s| s.id == node.id || s.id == required.id);
                let color = if lit {
                    GOLD_BRIGHT
                } else if met {
                    GOLD.gamma_multiply(0.8)
                } else {
                    egui::Color32::from_white_alpha(28)
                };
                painter.line_segment([position(required), position(node)], egui::Stroke::new(if lit { 1.8_f32 } else { 1.0_f32 }, color));
            }
        }
        let pointer = response.hover_pos();
        let mut hovered: Option<(&SkillNode, f32)> = None;
        for node in &wheel.nodes {
            let at = position(node);
            if let Some(d) = pointer.map(|p| p.distance(at)).filter(|&d| d <= PICK_RADIUS) {
                if hovered.is_none_or(|(_, best)| d < best) {
                    hovered = Some((node, d));
                }
            }
        }
        let hovered = hovered.map(|(n, _)| n);
        for node in &wheel.nodes {
            let at = position(node);
            let current = rank(stats, &node.id);
            let [r, g, b] = wheel.branch(&node.branch).map_or([180, 180, 180], |b| b.color);
            let family = egui::Color32::from_rgb(r, g, b);
            let radius = if node.is_ultimate() { NODE_RADIUS * 1.5 } else { NODE_RADIUS };
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
                egui::Stroke::new(1.5_f32, GOLD)
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
        }
        if let Some(node) = hovered {
            let class = wheel.branch(&node.branch).map_or("", |b| b.name.as_str());
            response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(egui::RichText::new(&node.name).strong().color(GOLD_BRIGHT));
                ui.label(egui::RichText::new(format!("{class} · {} · rang {}/{}", node.tier_label(), rank(stats, &node.id), node.max_rank)).size(11.0_f32).color(TEXT_SOFT));
            });
        }
        if response.clicked() {
            self.selected = hovered.map(|n| n.id.clone());
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
fn draw_sheet(ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats, node: &SkillNode) -> Option<String> {
    let current = rank(stats, &node.id);
    ui.label(egui::RichText::new(&node.name).strong().size(15.0_f32).color(GOLD_BRIGHT));
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
    clicked.then(|| node.id.clone())
}

/// Géométrie de la roue : angles des parts (la première commence en haut) et rayons des anneaux
struct WheelLayout {
    centre: egui::Pos2,
    outer: f32,
    rings: f32,
    branches: usize,
}

impl WheelLayout {
    fn sector(&self, branch: usize) -> (f32, f32) {
        let width = TAU / self.branches as f32;
        let start = -PI / 2.0 + branch as f32 * width;
        (start, start + width)
    }

    fn hub_radius(&self) -> f32 {
        self.outer * 0.17
    }

    /// Les anneaux vont de 45 % du rayon (assez de place pour quatre compétences par classe au
    /// niveau 1) jusqu'au nom des classes, au bord
    fn ring_radius(&self, ring: f32) -> f32 {
        let inner = self.outer * 0.45;
        let outer = self.outer - 34.0;
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

    #[test]
    fn test_rings_stay_between_hub_and_labels() {
        let layout = WheelLayout { centre: egui::Pos2::ZERO, outer: 300.0, rings: 4.0, branches: 3 };
        assert!(layout.ring_radius(1.0) > layout.hub_radius() + NODE_RADIUS);
        assert!(layout.ring_radius(4.0) + 1.5 * NODE_RADIUS < 300.0 - 20.0, "l'ultime ne touche pas le nom de la classe");
        assert!(layout.ring_radius(1.0) < layout.ring_radius(2.0) && layout.ring_radius(2.0) < layout.ring_radius(4.0));
        // La première part commence en haut
        let (start, end) = layout.sector(0);
        assert!((start + PI / 2.0).abs() < 1e-6 && (end - start - TAU / 3.0).abs() < 1e-6);
    }
}

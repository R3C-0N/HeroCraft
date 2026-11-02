//! Écran de la roue des compétences (touche C) : les branches se partagent le cercle, les
//! compétences sont posées sur des anneaux, du centre vers le bord. Un clic choisit une
//! compétence ; sa fiche, à droite, permet d'y investir un point.

use std::f32::consts::{PI, TAU};

use herocraft_common::skills::{rank, SkillNode};
use herocraft_common::{CharacterStats, SkillWheel};

use crate::ui::{GOLD, GOLD_BRIGHT, MENU_PADDING, TEXT_DIM, TEXT_SOFT};

/// Diamètre de la roue, en points d'écran
const WHEEL_SIZE: f32 = 440.0;
/// Rayon d'une compétence sur la roue
const NODE_RADIUS: f32 = 19.0;
/// Rayon du moyeu central
const HUB_RADIUS: f32 = 42.0;
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
                            self.draw_wheel(ui, wheel, stats);
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

    fn draw_wheel(&mut self, ui: &mut egui::Ui, wheel: &SkillWheel, stats: &CharacterStats) {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(WHEEL_SIZE, WHEEL_SIZE), egui::Sense::click());
        let painter = ui.painter_at(rect);
        let centre = rect.center();
        let outer = WHEEL_SIZE / 2.0 - 4.0;
        let layout = WheelLayout { centre, outer, rings: wheel.rings(), branches: wheel.branches.len().max(1) };

        // Fond : parts des branches, anneaux et rayons qui les séparent
        painter.circle_filled(centre, outer, egui::Color32::from_rgb(20, 26, 38));
        for (b, branch) in wheel.branches.iter().enumerate() {
            let [r, g, bl] = branch.color;
            let tint = egui::Color32::from_rgba_unmultiplied(r, g, bl, 26);
            let (start, end) = layout.sector(b);
            painter.add(sector_shape(centre, HUB_RADIUS, outer, start, end, tint));
            let edge = layout.point(start, outer);
            painter.line_segment([layout.point(start, HUB_RADIUS), edge], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(30)));
            // Nom de la branche le long du bord
            let label_at = layout.point((start + end) / 2.0, outer - 14.0);
            painter.text(label_at, egui::Align2::CENTER_CENTER, branch.name.to_uppercase(), egui::FontId::proportional(11.0_f32), egui::Color32::from_rgb(r, g, bl));
            if !wheel.nodes.iter().any(|n| n.branch == branch.id) {
                let hint_at = layout.point((start + end) / 2.0, (HUB_RADIUS + outer) / 2.0);
                painter.text(hint_at, egui::Align2::CENTER_CENTER, "à venir", egui::FontId::proportional(11.0_f32), TEXT_DIM);
            }
        }
        for ring in 1..=layout.rings {
            painter.circle_stroke(centre, layout.ring_radius(ring), egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(16)));
        }
        painter.circle_stroke(centre, outer, egui::Stroke::new(1.5_f32, GOLD));

        // Moyeu : niveau du héros
        painter.circle_filled(centre, HUB_RADIUS, egui::Color32::from_rgb(28, 34, 48));
        painter.circle_stroke(centre, HUB_RADIUS, egui::Stroke::new(1.5_f32, GOLD));
        painter.text(centre - egui::vec2(0.0, 7.0), egui::Align2::CENTER_CENTER, format!("Niv. {}", stats.level), egui::FontId::proportional(14.0_f32), GOLD_BRIGHT);
        painter.text(centre + egui::vec2(0.0, 10.0), egui::Align2::CENTER_CENTER, stats.class.label(), egui::FontId::proportional(10.0_f32), TEXT_DIM);

        // Liens des prérequis, puis les compétences par-dessus
        let position = |node: &SkillNode| {
            let b = wheel.branches.iter().position(|br| br.id == node.branch).unwrap_or(0);
            let (start, end) = layout.sector(b);
            layout.point(start + (end - start) * node.slot.clamp(0.0, 1.0), layout.ring_radius(node.ring))
        };
        for node in &wheel.nodes {
            for required in node.requires.iter().filter_map(|r| wheel.node(r)) {
                let met = rank(stats, &required.id) > 0;
                let color = if met { GOLD } else { egui::Color32::from_white_alpha(40) };
                painter.line_segment([position(required), position(node)], egui::Stroke::new(2.0_f32, color));
            }
        }
        let pointer = response.hover_pos();
        let mut hovered: Option<&SkillNode> = None;
        for node in &wheel.nodes {
            let at = position(node);
            let current = rank(stats, &node.id);
            let branch = wheel.branch(&node.branch).map_or([180, 180, 180], |b| b.color);
            let branch_color = egui::Color32::from_rgb(branch[0], branch[1], branch[2]);
            let is_hovered = pointer.is_some_and(|p| p.distance(at) <= NODE_RADIUS);
            if is_hovered {
                hovered = Some(node);
            }
            let selected = self.selected.as_deref() == Some(node.id.as_str());
            let learnable = wheel.check(stats, &node.id).is_none();
            let fill = if current > 0 { branch_color.gamma_multiply(0.85) } else { egui::Color32::from_rgb(34, 40, 54) };
            painter.circle_filled(at, NODE_RADIUS, fill);
            let ring_color = if selected || is_hovered {
                GOLD_BRIGHT
            } else if learnable {
                GOLD
            } else if current > 0 {
                branch_color
            } else {
                egui::Color32::from_white_alpha(50)
            };
            painter.circle_stroke(at, NODE_RADIUS, egui::Stroke::new(if selected { 3.0_f32 } else { 2.0_f32 }, ring_color));
            let initials: String = node.name.split([' ', '-']).filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
            let text_color = if current > 0 { egui::Color32::WHITE } else { TEXT_DIM };
            painter.text(at, egui::Align2::CENTER_CENTER, initials, egui::FontId::proportional(13.0_f32), text_color);
            // Rang sous la compétence
            painter.text(
                at + egui::vec2(0.0, NODE_RADIUS + 7.0),
                egui::Align2::CENTER_CENTER,
                format!("{current}/{}", node.max_rank),
                egui::FontId::proportional(10.0_f32),
                if current > 0 { GOLD_BRIGHT } else { TEXT_DIM },
            );
        }
        if let Some(node) = hovered {
            response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(egui::RichText::new(&node.name).strong().color(GOLD_BRIGHT));
                if !node.description.is_empty() {
                    ui.label(egui::RichText::new(&node.description).color(TEXT_SOFT));
                }
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
    ui.label(egui::RichText::new(format!("{branch} · anneau {} · rang {current}/{}", node.ring, node.max_rank)).size(11.0_f32).color(TEXT_DIM));
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
    rings: u8,
    branches: usize,
}

impl WheelLayout {
    fn sector(&self, branch: usize) -> (f32, f32) {
        let width = TAU / self.branches as f32;
        let start = -PI / 2.0 + branch as f32 * width;
        (start, start + width)
    }

    /// Les anneaux se partagent l'espace entre le moyeu et le nom des branches
    fn ring_radius(&self, ring: u8) -> f32 {
        let inner = HUB_RADIUS + NODE_RADIUS + 10.0;
        let outer = self.outer - NODE_RADIUS - 30.0;
        if self.rings <= 1 {
            return (inner + outer) / 2.0;
        }
        inner + (outer - inner) * (ring.max(1) - 1) as f32 / (self.rings - 1) as f32
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
        let layout = WheelLayout { centre: egui::Pos2::ZERO, outer: 200.0, rings: 3, branches: 3 };
        assert!(layout.ring_radius(1) > HUB_RADIUS + NODE_RADIUS);
        assert!(layout.ring_radius(3) + NODE_RADIUS < 200.0);
        assert!(layout.ring_radius(1) < layout.ring_radius(2) && layout.ring_radius(2) < layout.ring_radius(3));
        // La première part commence en haut
        let (start, end) = layout.sector(0);
        assert!((start + PI / 2.0).abs() < 1e-6 && (end - start - TAU / 3.0).abs() < 1e-6);
    }
}

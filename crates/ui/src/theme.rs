//! Palette bois, crème et or de l'ancien Terasology, partagée par le HUD et le menu, et aides
//! communes aux écrans egui

pub const GOLD: egui::Color32 = egui::Color32::from_rgb(245, 197, 66);
pub const GOLD_BRIGHT: egui::Color32 = egui::Color32::from_rgb(255, 224, 140);
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(180, 158, 124);
pub const TEXT_SOFT: egui::Color32 = egui::Color32::from_rgb(242, 226, 192);

/// Marge intérieure du cadre du menu de pause
pub(crate) const MENU_PADDING: f32 = 18.0;

/// Défilement de la molette reçu cette frame, sans le lissage d'egui
pub(crate) fn raw_scroll_delta(input: &egui::InputState) -> egui::Vec2 {
    input
        .raw
        .events
        .iter()
        .filter_map(|e| match e {
            egui::Event::MouseWheel { delta, .. } => Some(*delta),
            _ => None,
        })
        .fold(egui::Vec2::ZERO, |a, b| a + b)
}

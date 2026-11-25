//! Habillage des menus repris de l'ancien Terasology : panneaux de planches cerclés de fer,
//! boutons de bois, plaque de titre en métal, polices Grenze et Uncial Antiqua.
//!
//! Les textures sont du pixel art dessiné à l'échelle 3 (un pixel du dessin = 3 pixels de la
//! texture). Elles sont découpées en neuf (coins fixes, bords et centre répétés) et dessinées par
//! egui en filtrage « plus proche voisin », comme dans l'ancien jeu.

use std::sync::Arc;

/// Crème des textes, comme l'ancien thème (FFF3DC)
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(255, 243, 220);
/// Ombre portée sous les titres (17100C)
pub(crate) const TEXT_SHADOW: egui::Color32 = egui::Color32::from_rgb(23, 16, 12);
/// Texte gravé sur la plaque de métal (23282B)
pub(crate) const ENGRAVED: egui::Color32 = egui::Color32::from_rgb(35, 40, 43);
/// Texte d'un bouton enfoncé (FFE08C)
pub(crate) const PRESSED_TEXT: egui::Color32 = egui::Color32::from_rgb(255, 224, 140);
/// Texte désactivé (B49E7C)
pub(crate) const DISABLED_TEXT: egui::Color32 = egui::Color32::from_rgb(180, 158, 124);

/// Bois sombre des fonds (planches de l'ancien panneau)
pub const WOOD_DARK: egui::Color32 = egui::Color32::from_rgb(32, 18, 10);
pub const WOOD_DEEP: egui::Color32 = egui::Color32::from_rgb(61, 32, 13);
pub const WOOD_MID: egui::Color32 = egui::Color32::from_rgb(94, 51, 24);
pub const WOOD: egui::Color32 = egui::Color32::from_rgb(147, 90, 41);
pub const WOOD_LIGHT: egui::Color32 = egui::Color32::from_rgb(168, 103, 47);

/// Famille de police des titres (Uncial Antiqua)
pub(crate) const TITLE_FONT: &str = "titre";

macro_rules! skin_textures {
    ($($name:ident => $file:literal),* $(,)?) => {
        /// Textures de l'habillage
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[allow(dead_code)]
        pub enum Tex { $($name),* }

        const TEXTURE_FILES: &[(&str, &[u8])] = &[
            $(($file, include_bytes!(concat!("../assets/ui/", $file, ".png")))),*
        ];
    };
}

skin_textures! {
    PanelSlim => "panelSlim",
    Plaque => "plaque",
    Area => "area",
    Box => "box",
    BoxActive => "boxActive",
    Slot => "slot",
    SlotSelected => "slotSelected",
    ButtonMenu => "buttonMenu",
    ButtonMenuOver => "buttonMenuOver",
    ButtonMenuDown => "buttonMenuDown",
    ButtonMenuDanger => "buttonMenuDanger",
    ButtonMenuDangerOver => "buttonMenuDangerOver",
    ButtonMenuDangerDown => "buttonMenuDangerDown",
    ButtonMenuOff => "buttonMenuOff",
    Button => "button",
    ButtonOver => "buttonOver",
    ButtonDown => "buttonDown",
    ButtonDisabled => "buttonDisabled",
    ButtonSecondary => "buttonSecondary",
    ButtonSecondaryOver => "buttonSecondaryOver",
    ButtonSecondaryDown => "buttonSecondaryDown",
    ButtonDanger => "buttonDanger",
    ButtonDangerOver => "buttonDangerOver",
    ButtonDangerDown => "buttonDangerDown",
    ListItem => "listItem",
    ListItemHover => "listItemHover",
    ListItemSelected => "listItemSelected",
    Checkbox => "checkbox",
    CheckboxHover => "checkboxHover",
    CheckboxChecked => "checkboxChecked",
    CheckboxCheckedHover => "checkboxCheckedHover",
    CheckboxDisabled => "checkboxDisabled",
    CheckboxCheckedDisabled => "checkboxCheckedDisabled",
    OrnLeft => "ornLeft",
    OrnBar => "ornBar",
    OrnGem => "ornGem",
    OrnRight => "ornRight",
}

/// Textures chargées dans egui
pub struct Skin {
    textures: Vec<egui::TextureHandle>,
}

impl Skin {
    fn load(ctx: &egui::Context) -> Self {
        let textures = TEXTURE_FILES
            .iter()
            .map(|(name, bytes)| {
                let image = decode_png(bytes).unwrap_or_else(|e| panic!("texture d'interface {name} illisible : {e}"));
                ctx.load_texture(format!("skin:{name}"), image, egui::TextureOptions::NEAREST)
            })
            .collect();
        Self { textures }
    }

    pub fn texture(&self, tex: Tex) -> &egui::TextureHandle {
        &self.textures[tex as usize]
    }
}

fn decode_png(bytes: &[u8]) -> Result<egui::ColorImage, png::DecodingError> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let size = [info.width as usize, info.height as usize];
    let data = &buf[..info.buffer_size()];
    Ok(match info.color_type {
        png::ColorType::Rgba => egui::ColorImage::from_rgba_unmultiplied(size, data),
        png::ColorType::GrayscaleAlpha => {
            let rgba: Vec<u8> = data.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect();
            egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
        }
        _ => egui::ColorImage::from_rgb(size, data),
    })
}

fn skin_id() -> egui::Id {
    egui::Id::unique("herocraft_ui_skin")
}

/// Charge les textures et les polices de l'habillage dans ce contexte egui
pub fn install(ctx: &egui::Context) {
    let skin = Arc::new(Skin::load(ctx));
    ctx.data_mut(|d| d.insert_temp(skin_id(), skin));
    install_fonts(ctx);
}

/// Habillage du contexte (chargé à la volée s'il manque)
pub fn skin(ctx: &egui::Context) -> Arc<Skin> {
    if let Some(skin) = ctx.data(|d| d.get_temp::<Arc<Skin>>(skin_id())) {
        return skin;
    }
    let skin = Arc::new(Skin::load(ctx));
    ctx.data_mut(|d| d.insert_temp(skin_id(), skin.clone()));
    skin
}

/// Grenze pour le texte courant, Uncial Antiqua pour les titres ; les polices d'egui restent en
/// secours pour les symboles (▶ ⚙ 🌍…)
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Grenze est étroite et dessine petit : un léger agrandissement garde les tailles du jeu lisibles
    let grenze_tweak = egui::FontTweak { scale: 1.12_f32, y_offset_factor: 0.02_f32, ..Default::default() };
    fonts.font_data.insert(
        "Grenze".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../assets/ui/fonts/Grenze-Regular.ttf")).tweak(grenze_tweak.clone())),
    );
    fonts.font_data.insert(
        "Grenze-SemiBold".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../assets/ui/fonts/Grenze-SemiBold.ttf")).tweak(grenze_tweak.clone())),
    );
    fonts.font_data.insert(
        "Uncial".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../assets/ui/fonts/UncialAntiqua-Regular.ttf"))),
    );

    let fallbacks = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(egui::FontFamily::Proportional, std::iter::once("Grenze".to_owned()).chain(fallbacks.iter().cloned()).collect());
    fonts.families.insert(
        egui::FontFamily::Name(TITLE_FONT.into()),
        ["Uncial".to_owned(), "Grenze".to_owned()].into_iter().chain(fallbacks.iter().cloned()).collect(),
    );
    fonts.families.insert(
        egui::FontFamily::Name("fort".into()),
        std::iter::once("Grenze-SemiBold".to_owned()).chain(fallbacks.iter().cloned()).collect(),
    );
    ctx.set_fonts(fonts);
}

/// Police des titres à la taille donnée
pub fn title_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(TITLE_FONT.into()))
}

/// Police grasse (Grenze SemiBold) à la taille donnée
pub fn strong_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("fort".into()))
}

/// Découpe en neuf d'une texture : marges fixes, en pixels de texture (gauche, droite, haut, bas)
#[derive(Clone, Copy, Debug)]
pub struct Slice {
    pub tex: Tex,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Slice {
    pub const fn new(tex: Tex, left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self { tex, left, right, top, bottom }
    }

    pub const fn uniform(tex: Tex, border: f32) -> Self {
        Self::new(tex, border, border, border, border)
    }

    pub fn with_tex(self, tex: Tex) -> Self {
        Self { tex, ..self }
    }
}

/// Panneau principal des menus (planches cerclées de fer, liseré d'herbe en haut)
pub const PANEL: Slice = Slice::uniform(Tex::PanelSlim, 60.0);
/// Marge intérieure du panneau, à l'échelle 1
/// Marge en points flottants (celle d'egui est en entiers depuis la 0.31)
pub type Margin = egui::epaint::MarginF32;

pub(crate) const PANEL_MARGIN: Margin = Margin { left: 46.0, right: 46.0, top: 38.0, bottom: 46.0 };
/// Marge intérieure du panneau à l'échelle donnée
pub fn panel_margin(scale: f32) -> Margin {
    Margin {
        left: PANEL_MARGIN.left * scale,
        right: PANEL_MARGIN.right * scale,
        top: PANEL_MARGIN.top * scale,
        bottom: PANEL_MARGIN.bottom * scale,
    }
}

/// Cadre sombre des bulles et des petits panneaux du HUD
pub(crate) const AREA: Slice = Slice::uniform(Tex::Area, 9.0);
/// Encadré d'ardoise (touches, champs)
pub const BOX: Slice = Slice::uniform(Tex::Box, 12.0);
pub const SLOT: Slice = Slice::uniform(Tex::Slot, 12.0);
pub(crate) const PLAQUE: Slice = Slice::new(Tex::Plaque, 63.0, 63.0, 15.0, 15.0);

/// Dessine `slice` dans `rect`, chaque pixel de texture valant `scale` points. Les bords et le
/// centre sont répétés (pas étirés) pour garder le grain du bois.
pub fn paint_slice(painter: &egui::Painter, skin: &Skin, slice: Slice, rect: egui::Rect, scale: f32, tint: egui::Color32) {
    if !rect.is_positive() {
        return;
    }
    painter.add(slice_shape(skin, slice, rect, scale, tint));
}

pub(crate) fn slice_shape(skin: &Skin, slice: Slice, rect: egui::Rect, scale: f32, tint: egui::Color32) -> egui::Shape {
    let texture = skin.texture(slice.tex);
    let [tw, th] = texture.size().map(|v| v as f32);
    let mut mesh = egui::Mesh::with_texture(texture.id());

    // Les bords rétrécissent si le rectangle est plus petit qu'eux
    let fx = (rect.width() / ((slice.left + slice.right) * scale)).min(1.0_f32);
    let fy = (rect.height() / ((slice.top + slice.bottom) * scale)).min(1.0_f32);
    let (l, r) = (slice.left * scale * fx, slice.right * scale * fx);
    let (t, b) = (slice.top * scale * fy, slice.bottom * scale * fy);

    let xs = [rect.left(), rect.left() + l, rect.right() - r, rect.right()];
    let ys = [rect.top(), rect.top() + t, rect.bottom() - b, rect.bottom()];
    let us = [0.0_f32, slice.left, tw - slice.right, tw];
    let vs = [0.0_f32, slice.top, th - slice.bottom, th];

    for row in 0..3 {
        for col in 0..3 {
            let dst = egui::Rect::from_min_max(egui::pos2(xs[col], ys[row]), egui::pos2(xs[col + 1], ys[row + 1]));
            if !dst.is_positive() {
                continue;
            }
            let src = egui::Rect::from_min_max(egui::pos2(us[col], vs[row]), egui::pos2(us[col + 1], vs[row + 1]));
            if !src.is_positive() {
                continue;
            }
            // Tuile : taille d'origine (à l'échelle) pour les parties répétées, la cible sinon
            let tile_w = if col == 1 { src.width() * scale } else { dst.width() };
            let tile_h = if row == 1 { src.height() * scale } else { dst.height() };
            tile(&mut mesh, dst, src, tile_w, tile_h, [tw, th], tint);
        }
    }
    egui::Shape::mesh(mesh)
}

fn tile(mesh: &mut egui::Mesh, dst: egui::Rect, src: egui::Rect, tile_w: f32, tile_h: f32, tex_size: [f32; 2], tint: egui::Color32) {
    let tile_w = tile_w.max(1.0_f32);
    let tile_h = tile_h.max(1.0_f32);
    let mut y = dst.top();
    while y < dst.bottom() - 0.01_f32 {
        let h = tile_h.min(dst.bottom() - y);
        let v1 = src.top() + src.height() * (h / tile_h);
        let mut x = dst.left();
        while x < dst.right() - 0.01_f32 {
            let w = tile_w.min(dst.right() - x);
            let u1 = src.left() + src.width() * (w / tile_w);
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
            let uv = egui::Rect::from_min_max(
                egui::pos2(src.left() / tex_size[0], src.top() / tex_size[1]),
                egui::pos2(u1 / tex_size[0], v1 / tex_size[1]),
            );
            mesh.add_rect_with_uv(rect, uv, tint);
            x += w;
        }
        y += h;
    }
}

/// Cadre texturé autour d'un contenu, à la manière d'`egui::Frame`
pub struct SkinFrame {
    slice: Slice,
    scale: f32,
    margin: Margin,
    tint: egui::Color32,
}

impl SkinFrame {
    pub fn new(slice: Slice, scale: f32, margin: Margin) -> Self {
        Self { slice, scale, margin, tint: egui::Color32::WHITE }
    }

    /// Panneau de menu
    pub fn panel() -> Self {
        Self::panel_scaled(1.0_f32)
    }

    /// Panneau de menu réduit (petites fenêtres : bords et marges à `scale`)
    pub fn panel_scaled(scale: f32) -> Self {
        Self::new(PANEL, scale, panel_margin(scale))
    }

    /// Petit cadre sombre (HUD, bulles)
    pub fn area(margin: Margin) -> Self {
        Self::new(AREA, 1.0_f32, margin)
    }

    pub fn show<R>(self, ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
        let skin = skin(ui.ctx());
        let where_to_paint = ui.painter().add(egui::Shape::Noop);
        let outer_available = ui.available_rect_before_wrap();
        let inner_rect = egui::Rect::from_min_max(
            outer_available.min + egui::vec2(self.margin.left, self.margin.top),
            (outer_available.max - egui::vec2(self.margin.right, self.margin.bottom)).max(outer_available.min + egui::vec2(self.margin.left, self.margin.top)),
        );
        let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect).layout(*ui.layout()));
        let inner = add_contents(&mut content_ui);
        let content_rect = content_ui.min_rect();
        let outer_rect = egui::Rect::from_min_max(
            content_rect.min - egui::vec2(self.margin.left, self.margin.top),
            content_rect.max + egui::vec2(self.margin.right, self.margin.bottom),
        );
        ui.painter().set(where_to_paint, slice_shape(&skin, self.slice, outer_rect, self.scale, self.tint));
        let response = ui.allocate_rect(outer_rect, egui::Sense::hover());
        egui::InnerResponse::new(inner, response)
    }
}

/// Famille de bouton de l'ancien thème
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Grande entrée de menu en bois clair
    Menu,
    /// Entrée de menu rouge (quitter)
    MenuDanger,
    /// Bouton de bois clair
    Primary,
    /// Bouton de bois plus sombre (choix non retenu)
    Secondary,
    /// Élément de liste (onglets latéraux)
    ListItem,
}

/// Bouton texturé : remplace `egui::Button` dans les menus
pub struct SkinButton {
    text: egui::WidgetText,
    kind: ButtonKind,
    min_size: egui::Vec2,
    selected: bool,
    scale: f32,
}

impl SkinButton {
    pub fn new(text: impl Into<egui::WidgetText>) -> Self {
        Self { text: text.into(), kind: ButtonKind::Primary, min_size: egui::Vec2::ZERO, selected: false, scale: 1.0_f32 }
    }

    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn min_size(mut self, size: egui::Vec2) -> Self {
        self.min_size = size;
        self
    }

    /// Choix retenu : bois clair et texte doré
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Bouton compact (bordures à deux tiers)
    pub fn small(mut self) -> Self {
        self.scale = 2.0_f32 / 3.0_f32;
        self
    }

    /// Découpe, marges du texte (gauche, droite) et hauteur du rebord bas (en pixels de texture)
    fn layout(&self) -> (Slice, f32, f32, f32) {
        match self.kind {
            ButtonKind::Menu | ButtonKind::MenuDanger => (Slice::new(Tex::ButtonMenu, 9.0, 9.0, 9.0, 18.0), 20.0, 18.0, 15.0),
            ButtonKind::ListItem => (Slice::new(Tex::ListItem, 18.0, 6.0, 6.0, 6.0), 22.0, 10.0, 3.0),
            _ => (Slice::new(Tex::Button, 9.0, 9.0, 9.0, 18.0), 16.0, 16.0, 15.0),
        }
    }

    fn texture(&self, enabled: bool, hovered: bool, down: bool) -> Tex {
        use ButtonKind::*;
        let kind = match (self.kind, self.selected) {
            (Secondary, true) => Primary,
            (k, _) => k,
        };
        match kind {
            Menu if !enabled => Tex::ButtonMenuOff,
            Menu if down => Tex::ButtonMenuDown,
            Menu if hovered => Tex::ButtonMenuOver,
            Menu => Tex::ButtonMenu,
            MenuDanger if !enabled => Tex::ButtonMenuOff,
            MenuDanger if down => Tex::ButtonMenuDangerDown,
            MenuDanger if hovered => Tex::ButtonMenuDangerOver,
            MenuDanger => Tex::ButtonMenuDanger,
            ListItem if self.selected => Tex::ListItemSelected,
            ListItem if hovered => Tex::ListItemHover,
            ListItem => Tex::ListItem,
            _ if !enabled => Tex::ButtonDisabled,
            Primary if down => Tex::ButtonDown,
            Primary if hovered => Tex::ButtonOver,
            Primary => Tex::Button,
            Secondary if down => Tex::ButtonSecondaryDown,
            Secondary if hovered => Tex::ButtonSecondaryOver,
            Secondary => Tex::ButtonSecondary,
        }
    }
}

impl egui::Widget for SkinButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (slice, pad_left, pad_right, lip) = self.layout();
        let s = self.scale;
        let lip = lip * s;
        let wrap_width = ui.available_width() - (pad_left + pad_right) * s;
        let galley = self.text.clone().into_galley(ui, Some(egui::TextWrapMode::Extend), wrap_width, egui::TextStyle::Button);
        let natural_height = match self.kind {
            ButtonKind::Menu | ButtonKind::MenuDanger => 48.0_f32,
            ButtonKind::ListItem => galley.size().y + 12.0_f32,
            // Les boutons compacts serrent le texte sur la face
            _ => galley.size().y + if s < 1.0_f32 { 2.0_f32 } else { 10.0_f32 } + lip,
        };
        let size = egui::vec2(galley.size().x + (pad_left + pad_right) * s, natural_height).max(self.min_size);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), galley.text()));

        if ui.is_rect_visible(rect) {
            let enabled = ui.is_enabled();
            let down = enabled && response.is_pointer_button_down_on();
            let hovered = enabled && response.hovered();
            let skin = skin(ui.ctx());
            let tex = self.texture(enabled, hovered, down);
            let slice = slice.with_tex(tex);
            paint_slice(ui.painter(), &skin, slice, rect, s, egui::Color32::WHITE);

            // Texte centré sur la face du bouton ; il descend quand le bouton s'enfonce
            let face = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.bottom() - lip));
            let x = match self.kind {
                ButtonKind::Menu | ButtonKind::MenuDanger | ButtonKind::ListItem => rect.left() + pad_left * s,
                _ => face.center().x - galley.size().x * 0.5_f32,
            };
            let pressed_shift = if down && self.kind != ButtonKind::ListItem { 4.0_f32 * s } else { 0.0_f32 };
            let y = face.center().y - galley.size().y * 0.5_f32 + pressed_shift;
            let fallback = if !enabled {
                DISABLED_TEXT
            } else if down || (self.selected && self.kind != ButtonKind::ListItem) {
                PRESSED_TEXT
            } else {
                TEXT
            };
            let pos = egui::pos2(x, y);
            ui.painter().galley_with_override_text_color(pos + egui::vec2(1.0_f32, 1.0_f32), galley.clone(), TEXT_SHADOW.gamma_multiply(0.8_f32));
            ui.painter().galley(pos, galley, fallback);
        }
        response
    }
}

/// Case à cocher de l'ancien thème (24 points, deux points par pixel du dessin)
pub fn paint_checkbox(ui: &egui::Ui, rect: egui::Rect, on: bool, hovered: bool) {
    let skin = skin(ui.ctx());
    let tex = match (ui.is_enabled(), on, hovered) {
        (false, true, _) => Tex::CheckboxCheckedDisabled,
        (false, false, _) => Tex::CheckboxDisabled,
        (true, true, true) => Tex::CheckboxCheckedHover,
        (true, true, false) => Tex::CheckboxChecked,
        (true, false, true) => Tex::CheckboxHover,
        (true, false, false) => Tex::Checkbox,
    };
    let texture = skin.texture(tex);
    ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}

/// Filet de fer à pierre bleue, repris du bas du logo de l'ancien menu
pub fn ornament(ui: &mut egui::Ui, width: f32) {
    let skin = skin(ui.ctx());
    // Un tiers d'échelle : un pixel du dessin par point
    let scale = 1.0_f32 / 3.0_f32;
    let height = 48.0_f32 * scale;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let full = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    let cap = 48.0_f32 * scale;
    let left = egui::Rect::from_min_size(rect.min, egui::vec2(cap, height));
    let right = egui::Rect::from_min_size(egui::pos2(rect.right() - cap, rect.top()), egui::vec2(cap, height));
    let gem = egui::Rect::from_center_size(rect.center(), egui::vec2(cap, height));
    for bar in [
        egui::Rect::from_min_max(egui::pos2(left.right(), rect.top()), egui::pos2(gem.left(), rect.bottom())),
        egui::Rect::from_min_max(egui::pos2(gem.right(), rect.top()), egui::pos2(right.left(), rect.bottom())),
    ] {
        if bar.is_positive() {
            painter.image(skin.texture(Tex::OrnBar).id(), bar, full, egui::Color32::WHITE);
        }
    }
    painter.image(skin.texture(Tex::OrnLeft).id(), left, full, egui::Color32::WHITE);
    painter.image(skin.texture(Tex::OrnRight).id(), right, full, egui::Color32::WHITE);
    painter.image(skin.texture(Tex::OrnGem).id(), gem, full, egui::Color32::WHITE);
}

/// Plaque de métal gravée portant un titre (Grenze grasse : le H onciale se lit comme un D)
pub fn plaque_title(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    let skin = skin(ui.ctx());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), strong_font(size), ENGRAVED);
    let scale = 1.0_f32;
    let height = 60.0_f32 * scale;
    let width = galley.size().x + 2.0_f32 * 66.0_f32 * scale;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    paint_slice(ui.painter(), &skin, PLAQUE, rect, scale, egui::Color32::WHITE);
    // La face de la plaque s'arrête au-dessus de son ombre (dernier tiers du bas)
    let face_center = egui::pos2(rect.center().x, rect.top() + 25.0_f32 * scale);
    let pos = face_center - galley.size() * 0.5_f32;
    ui.painter().galley_with_override_text_color(pos + egui::vec2(0.0_f32, 1.0_f32), galley.clone(), egui::Color32::from_white_alpha(70));
    ui.painter().galley(pos, galley, ENGRAVED);
    response
}

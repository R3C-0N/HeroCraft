use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, KeyCode, NamedKey};

use crate::audio::AudioSettings;
use crate::input::{is_reserved_key, Action, Controls, ACTION_GROUPS};
use crate::inventory::{can_hold, Inventory, ItemStack, HOTBAR_SLOTS, TOTAL_SLOTS};
use crate::item_icons::ItemIcons;
use herocraft_common::{blocks, items, ItemId};
use crate::player_stats::PlayerStats;
use crate::hero_creator::{CreatorEvent, HeroCreator};
use herocraft_common::hero::Legend;
use crate::ui_skin::{ButtonKind, SkinButton};

/// Actions déclenchées par l'interface utilisateur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    GenerateNewPlanet,
    ToggleGroundCamera,
    ToggleAutoRotate,
    ResetCamera,
    ToggleTectonics,
    ToggleOceanCurrents,
    // Actions du sous-menu Options > Affichage
    ToggleVsync,
    SetLodDistance,
    ToggleFullscreen,
    SetFpsLimit,
    ResumeGame,
    QuitGame,
    // Actions de la page Multijoueur
    /// Ouvrir la partie solo au réseau local
    OpenToLan,
    /// Rejoindre la partie de `MultiplayerUi::join_target`
    JoinServer,
    /// Quitter la partie en ligne (ou la fermer si elle est ouverte ici) et revenir au solo
    LeaveServer,
    /// Options > Son : un volume vient d'être réglé, un son d'essai le fait entendre
    PreviewSound,
    /// Écran de mort : revenir à la vie au point d'apparition
    Respawn,
    /// Le créateur vient de sceller la légende du héros (`UiState::take_sealed_legend`) : elle est
    /// à enregistrer
    LegendSealed,
}

/// État de la partie en ligne, affiché dans la page Multijoueur
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetStatus {
    Solo,
    /// Connexion en cours à cette adresse
    Connecting(String),
    /// Partie solo ouverte au réseau local sur ce port
    Hosting { port: u16 },
    /// Connecté à cette adresse
    Online(String),
}

/// Partie trouvée sur le réseau local
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanEntry {
    pub name: String,
    pub addr: String,
    pub players: u32,
}

/// Page Multijoueur : nom du joueur, parties du réseau local, adresse saisie
pub struct MultiplayerUi {
    pub pseudo: String,
    /// Adresse saisie à la main
    pub address: String,
    /// Adresse à rejoindre quand `UiAction::JoinServer` est émise
    pub join_target: String,
    pub status: NetStatus,
    pub lan_servers: Vec<LanEntry>,
    /// La recherche sur le réseau local fonctionne (port d'écoute disponible)
    pub lan_search_ok: bool,
    /// Dernier message (erreur de connexion, déconnexion…)
    pub notice: Option<String>,
}

impl Default for MultiplayerUi {
    fn default() -> Self {
        Self {
            pseudo: "Héros".into(),
            address: String::new(),
            join_target: String::new(),
            status: NetStatus::Solo,
            lan_servers: Vec::new(),
            lan_search_ok: true,
            notice: None,
        }
    }
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

/// Durée d'affichage du dernier à-coup sous le compteur FPS, en secondes
const HITCH_DISPLAY_SECS: f32 = 5.0;

/// Préréglage de qualité graphique : règle d'un coup les effets les plus chers pour le GPU
/// (mesurés avec `--benchmark` sur un GPU intégré Iris Xe : ombres du relief ~4 ms par image,
/// rayons crépusculaires 1 à 4 ms, ombres des nuages ~1 ms, bloom ~0,5 ms)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsQuality {
    /// Ni ombres, ni rayons, ni bloom, distance de détail courte
    Basse,
    /// Ombres du relief, sans ombres des nuages ni rayons crépusculaires
    Moyenne,
    /// Tous les effets
    Haute,
}

/// Effets réglés par un préréglage de qualité
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityEffects {
    pub shadows: bool,
    pub cloud_shadows: bool,
    pub god_rays: bool,
    pub bloom: bool,
    pub lod_distance: LodDistanceSetting,
}

impl GraphicsQuality {
    pub const ALL: [GraphicsQuality; 3] = [GraphicsQuality::Basse, GraphicsQuality::Moyenne, GraphicsQuality::Haute];

    pub fn label(self) -> &'static str {
        match self {
            Self::Basse => "Basse",
            Self::Moyenne => "Moyenne",
            Self::Haute => "Haute",
        }
    }

    pub fn effects(self) -> QualityEffects {
        match self {
            Self::Basse => QualityEffects { shadows: false, cloud_shadows: false, god_rays: false, bloom: false, lod_distance: LodDistanceSetting::Courte },
            Self::Moyenne => QualityEffects { shadows: true, cloud_shadows: false, god_rays: false, bloom: true, lod_distance: LodDistanceSetting::Normale },
            Self::Haute => QualityEffects { shadows: true, cloud_shadows: true, god_rays: true, bloom: true, lod_distance: LodDistanceSetting::Normale },
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSettings {
    pub vsync: bool,
    pub fullscreen: bool,
    pub lod_distance: LodDistanceSetting,
    pub fps_limit: FpsLimitSetting,
    /// Aplatissement de la planète vue du sol : 0 = vraie courbure, 1 = horizon le plus lointain
    /// possible sans voir les antipodes (renderer.rs, `planet_flattening`)
    pub flattening: f32,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            fullscreen: false,
            lod_distance: LodDistanceSetting::Normale,
            fps_limit: FpsLimitSetting::Fps60,
            flattening: 1.0,
        }
    }
}

/// Réglages visuels des nuages 3D (Marshmallow SDF)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudSettings {
    pub density: f32,          // Densité / visibilité globale [0.0 .. 2.0] (défaut: 1.0)
    pub roundness: f32,        // Rayon d'arrondi des arêtes (Bevel) [0.05 .. 0.55] (défaut: 0.45)
    pub shadow_intensity: f32, // Ombrage de sous-face violacée [0.0 .. 2.0] (défaut: 1.0)
    pub sunset_glow: f32,      // Éclat chaud au coucher de soleil [0.0 .. 2.0] (défaut: 1.0)
    pub micro_noise: f32,      // Micro-texture / grain mat [0.0 .. 1.0] (défaut: 0.0)
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            density: 1.0,
            roundness: 0.45,
            shadow_intensity: 1.0,
            sunset_glow: 1.0,
            micro_noise: 0.0,
        }
    }
}

/// Courbe de conversion HDR -> écran appliquée par la passe de composition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tonemapper {
    /// Ancien rendu : valeurs écrêtées à 1.0
    Clip,
    /// Identique à l'ancien rendu dans les tons moyens, hautes lumières compressées
    SoftShoulder,
    Aces,
    AgX,
}

impl Tonemapper {
    pub const ALL: [Tonemapper; 4] = [Tonemapper::Clip, Tonemapper::SoftShoulder, Tonemapper::Aces, Tonemapper::AgX];

    pub fn label(self) -> &'static str {
        match self {
            Tonemapper::Clip => "Écrêtage (ancien rendu)",
            Tonemapper::SoftShoulder => "Épaule douce",
            Tonemapper::Aces => "ACES",
            Tonemapper::AgX => "AgX",
        }
    }

    /// Valeur de `post.mode.x` dans post_shader.wgsl
    pub fn shader_index(self) -> u32 {
        match self {
            Tonemapper::Clip => 0,
            Tonemapper::SoftShoulder => 1,
            Tonemapper::Aces => 2,
            Tonemapper::AgX => 3,
        }
    }
}

/// Réglages du post-traitement HDR (exposition, tonemapper, bloom)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostSettings {
    pub exposure_ev: f32,     // Exposition en EV [-3.0 .. 3.0] (0 = neutre)
    pub tonemapper: Tonemapper,
    pub bloom_enabled: bool,
    pub bloom_intensity: f32, // Part du halo ajoutée à l'image [0.0 .. 1.0]
    pub bloom_threshold: f32, // Luminance HDR à partir de laquelle un pixel diffuse [0.0 .. 8.0]
    pub bloom_knee: f32,      // Largeur de la transition douce autour du seuil [0.0 .. 3.0]
}

impl Default for PostSettings {
    fn default() -> Self {
        Self {
            exposure_ev: -0.3,
            tonemapper: Tonemapper::AgX,
            bloom_enabled: true,
            bloom_intensity: 0.25,
            bloom_threshold: 6.0,
            bloom_knee: 2.0,
        }
    }
}

/// Réglages des ombres portées du soleil et des nuages
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    pub strength: f32,       // Part de la lumière directe retirée à l'ombre du relief [0.0 .. 1.0]
    pub cloud_shadows: bool,
    pub cloud_strength: f32, // Part de la lumière directe retirée sous un nuage [0.0 .. 1.0]
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 1.0,
            cloud_shadows: true,
            cloud_strength: 0.7,
        }
    }
}

/// Réglages de l'atmosphère physique et des rayons crépusculaires (la brume est réglée par
/// le curseur Brume de l'éclairage)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereSettings {
    pub air_density: f32,        // Densité de l'air (diffusion de Rayleigh) [0.2 .. 3.0]
    pub sun_halo: f32,           // Anisotropie de la brume (g de Mie) : halo resserré autour du soleil [0.0 .. 0.95]
    pub god_rays: bool,
    pub god_rays_intensity: f32, // Accentuation de l'ombre de l'air (1 = physique) [0.0 .. 4.0]
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self {
            air_density: 1.0,
            sun_halo: 0.8,
            god_rays: true,
            god_rays_intensity: 2.5,
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
    pub cloud_density: f32,      // Densité des nuages volumétriques arrondis (défaut: 1.0, [0.0 .. 2.0])
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
            cloud_density: 1.0,
        }
    }
}

/// Page affichée dans le menu de pause (Échap)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuPage {
    /// Accueil : reprendre, options, commandes, quitter
    Main,
    /// Sous-menu des réglages, rangés par onglets
    Options,
    /// Touches modifiables et réglages de la souris
    Controls,
    /// Ouvrir sa partie au réseau local, rejoindre une partie
    Multiplayer,
}

/// Onglets du sous-menu Options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionsTab {
    Display,
    Lighting,
    Clouds,
    Hdr,
    Shadows,
    Atmosphere,
    Audio,
}

impl OptionsTab {
    pub const ALL: [OptionsTab; 7] = [
        OptionsTab::Display,
        OptionsTab::Lighting,
        OptionsTab::Clouds,
        OptionsTab::Hdr,
        OptionsTab::Shadows,
        OptionsTab::Atmosphere,
        OptionsTab::Audio,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            OptionsTab::Display => "🖥",
            OptionsTab::Lighting => "🌙",
            OptionsTab::Clouds => "☁",
            OptionsTab::Hdr => "✨",
            OptionsTab::Shadows => "🌓",
            OptionsTab::Atmosphere => "🌅",
            OptionsTab::Audio => "🔊",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OptionsTab::Display => "Affichage",
            OptionsTab::Lighting => "Éclairage",
            OptionsTab::Clouds => "Nuages",
            OptionsTab::Hdr => "Rendu HDR",
            OptionsTab::Shadows => "Ombres",
            OptionsTab::Atmosphere => "Atmosphère",
            OptionsTab::Audio => "Son",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            OptionsTab::Display => "Fenêtre, synchronisation et distance de détail",
            OptionsTab::Lighting => "Intensité du Soleil, de la Lune et du ciel",
            OptionsTab::Clouds => "Forme et teinte des nuages voxel",
            OptionsTab::Hdr => "Exposition, courbe de tons et halo lumineux",
            OptionsTab::Shadows => "Ombres portées du relief et des nuages",
            OptionsTab::Atmosphere => "Diffusion de l'air, brume et rayons crépusculaires",
            OptionsTab::Audio => "Volume général, bruitages et ambiance",
        }
    }
}

// Palette bois, crème et or de l'ancien Terasology, partagée par le HUD et le menu
const GOLD: egui::Color32 = egui::Color32::from_rgb(245, 197, 66);
const GOLD_BRIGHT: egui::Color32 = egui::Color32::from_rgb(255, 224, 140);
const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(180, 158, 124);
const TEXT_SOFT: egui::Color32 = egui::Color32::from_rgb(242, 226, 192);
const DANGER: egui::Color32 = egui::Color32::from_rgb(255, 128, 104);

/// Marge minimale entre un panneau et le bord de l'écran (points logiques)
const SCREEN_MARGIN: f32 = 16.0;
/// Largeur d'une jauge du HUD (vie, mana, endurance)
const GAUGE_WIDTH: f32 = 150.0;
/// Durée du voile rouge après une blessure, en secondes
const HURT_FLASH_SECS: f32 = 0.9;
/// Sous cette fraction de la vie, le bord de l'écran rougit et bat comme un cœur
const LOW_HP_FRACTION: f32 = 0.3;
/// En dessous de cette largeur, les onglets des options passent au-dessus du contenu
const OPTIONS_SIDEBAR_MIN_WIDTH: f32 = 560.0;
const OPTIONS_SIDEBAR_WIDTH: f32 = 150.0;

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
    /// Dernier à-coup (durée de l'image en ms) et quand il a eu lieu : le compteur FPS, moyenné,
    /// ne le montre pas
    hitch: Option<(f32, std::time::Instant)>,
    pub is_settings_open: bool,
    pub menu_page: MenuPage,
    pub options_tab: OptionsTab,
    pub video_settings: VideoSettings,
    pub lighting_settings: LightingSettings,
    pub cloud_settings: CloudSettings,
    pub post_settings: PostSettings,
    pub shadow_settings: ShadowSettings,
    pub atmosphere_settings: AtmosphereSettings,
    pub audio_settings: AudioSettings,
    pub controls: Controls,
    /// Action en attente d'une nouvelle touche dans la page Commandes
    pub rebinding: Option<Action>,
    /// Message affiché après un échange de touches entre deux actions
    pub rebind_notice: Option<String>,
    /// Curseur capturé par le jeu : egui ne reçoit plus la souris
    pub pointer_captured: bool,
    pub screen_width: f32,
    pub screen_height: f32,
    /// Noms des autres joueurs (en ligne), avec leur position à l'écran en pixels physiques
    pub name_tags: Vec<(egui::Pos2, String)>,
    pub multiplayer: MultiplayerUi,
    /// Blocs et objets du joueur : barre d'objets en bas de l'écran, sac dans l'écran d'inventaire
    pub inventory: Inventory,
    /// Tous les blocs et objets, à prendre dans l'écran d'inventaire
    pub catalog: Catalog,
    /// Icônes des blocs et des objets
    pub icons: ItemIcons,
    /// Écran d'inventaire ouvert (la souris est libérée pour déplacer les piles)
    pub inventory_open: bool,
    /// Jauges, classe et niveau du joueur
    pub player: PlayerStats,
    /// Légende du héros dans ce monde ; `None` tant qu'il n'est pas baptisé
    pub legend: Option<Legend>,
    /// Créateur de légende ouvert (à la première arrivée au sol d'un héros sans légende)
    pub creator: Option<HeroCreator>,
    /// Légende que le créateur vient de sceller, en attente du jeu
    sealed_legend: Option<Legend>,
    /// Chat et commandes (touche Entrée)
    pub chat: crate::chat::Chat,
}

impl UiState {
    pub fn new(initial_seed: u64) -> Self {
        herocraft_common::prof_scope!("ui::UiState::new");
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
            hitch: None,
            is_settings_open: false,
            menu_page: MenuPage::Main,
            options_tab: OptionsTab::Display,
            video_settings: VideoSettings::default(),
            lighting_settings: LightingSettings::default(),
            cloud_settings: CloudSettings::default(),
            post_settings: PostSettings::default(),
            shadow_settings: ShadowSettings::default(),
            atmosphere_settings: AtmosphereSettings::default(),
            audio_settings: AudioSettings::default(),
            controls: Controls::default(),
            rebinding: None,
            rebind_notice: None,
            pointer_captured: false,
            screen_width: 1280.0_f32,
            screen_height: 720.0_f32,
            name_tags: Vec::new(),
            multiplayer: MultiplayerUi::default(),
            inventory: Inventory::default(),
            catalog: Catalog::default(),
            icons: ItemIcons::default(),
            inventory_open: false,
            player: PlayerStats::default(),
            legend: None,
            creator: None,
            sealed_legend: None,
            chat: crate::chat::Chat::default(),
        }
    }

    /// Le créateur de légende tient l'écran
    pub fn creator_open(&self) -> bool {
        self.creator.is_some()
    }

    /// Ouvre le créateur, si le héros n'a pas encore de légende
    pub fn open_creator(&mut self) {
        if self.legend.is_none() && self.creator.is_none() {
            self.set_inventory_open(false);
            self.creator = Some(HeroCreator::new());
        }
    }

    /// Légende du héros dans le monde en cours : le créateur se referme quand elle est là
    pub fn set_legend(&mut self, legend: Option<Legend>) {
        if legend.is_some() {
            self.creator = None;
        }
        self.player.stats.class = legend.as_ref().map_or(herocraft_common::PlayerClass::Adventurer, |l| l.class);
        self.legend = legend;
    }

    /// La légende que le créateur vient de sceller (une seule fois)
    pub fn take_sealed_legend(&mut self) -> Option<Legend> {
        self.sealed_legend.take()
    }

    /// Applique le thème de l'ancien Terasology : textures de bois et de fer, polices Grenze et
    /// Uncial Antiqua, texte crème. Les widgets egui sans texture (curseurs, listes déroulantes,
    /// champs) reprennent les mêmes teintes de bois.
    pub fn apply_custom_theme(ctx: &egui::Context) {
        use crate::ui_skin::{TEXT, WOOD, WOOD_DARK, WOOD_DEEP, WOOD_LIGHT, WOOD_MID};
        crate::ui_skin::install(ctx);

        let mut visuals = egui::Visuals::dark();
        let edge = egui::Color32::from_rgb(23, 13, 8);

        visuals.window_rounding = egui::Rounding::ZERO;
        visuals.menu_rounding = egui::Rounding::ZERO;
        visuals.window_fill = egui::Color32::from_rgba_premultiplied(32, 18, 10, 245);
        visuals.window_stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(92, 65, 45));
        visuals.panel_fill = egui::Color32::from_rgba_premultiplied(32, 18, 10, 220);
        visuals.window_shadow = egui::epaint::Shadow { offset: egui::vec2(3.0_f32, 4.0_f32), blur: 0.0_f32, spread: 0.0_f32, color: egui::Color32::from_black_alpha(110) };
        visuals.popup_shadow = visuals.window_shadow;
        // Champs de saisie : creux de bois sombre
        visuals.extreme_bg_color = egui::Color32::from_rgb(23, 16, 12);
        visuals.faint_bg_color = WOOD_DEEP;
        visuals.text_cursor.stroke = egui::Stroke::new(2.0_f32, GOLD_BRIGHT);

        // Texte et filets (séparateurs)
        visuals.widgets.noninteractive.bg_fill = WOOD_DEEP;
        visuals.widgets.noninteractive.weak_bg_fill = WOOD_DEEP;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, TEXT_DIM.gamma_multiply(0.45_f32));
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
        visuals.widgets.noninteractive.rounding = egui::Rounding::ZERO;

        // Widgets au repos : bois, liseré sombre ; le rail des curseurs est un creux sombre
        visuals.widgets.inactive.bg_fill = WOOD_DARK;
        visuals.widgets.inactive.weak_bg_fill = WOOD;
        visuals.widgets.inactive.rounding = egui::Rounding::ZERO;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, edge);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);

        // Survol : bois clair, liseré doré
        visuals.widgets.hovered.bg_fill = WOOD_LIGHT;
        visuals.widgets.hovered.weak_bg_fill = WOOD_LIGHT;
        visuals.widgets.hovered.rounding = egui::Rounding::ZERO;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5_f32, GOLD);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, TEXT);

        // Clic : bois enfoncé, texte doré
        visuals.widgets.active.bg_fill = WOOD_LIGHT;
        visuals.widgets.active.weak_bg_fill = WOOD_MID;
        visuals.widgets.active.rounding = egui::Rounding::ZERO;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0_f32, GOLD_BRIGHT);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, GOLD_BRIGHT);

        visuals.widgets.open.bg_fill = WOOD_MID;
        visuals.widgets.open.weak_bg_fill = WOOD_MID;
        visuals.widgets.open.rounding = egui::Rounding::ZERO;
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, GOLD);
        visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);

        // Choix sélectionnés et partie remplie des curseurs : or
        visuals.selection.bg_fill = egui::Color32::from_rgb(150, 104, 28);
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, GOLD_BRIGHT);
        visuals.slider_trailing_fill = true;
        visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.6_f32 };

        let mut style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0_f32, 8.0_f32);
        style.spacing.button_padding = egui::vec2(12.0_f32, 6.0_f32);
        // Barres de défilement fines qui s'élargissent au survol
        style.spacing.scroll = egui::style::ScrollStyle::thin();

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

    /// Effets actuellement réglés, dans les termes des préréglages de qualité
    fn quality_effects(&self) -> QualityEffects {
        QualityEffects {
            shadows: self.shadow_settings.enabled,
            cloud_shadows: self.shadow_settings.cloud_shadows,
            god_rays: self.atmosphere_settings.god_rays,
            bloom: self.post_settings.bloom_enabled,
            lod_distance: self.video_settings.lod_distance,
        }
    }

    /// Préréglage de qualité qui correspond aux réglages actuels (`None` : réglages personnalisés)
    pub fn graphics_quality(&self) -> Option<GraphicsQuality> {
        let current = self.quality_effects();
        GraphicsQuality::ALL.into_iter().find(|q| q.effects() == current)
    }

    /// Applique un préréglage de qualité. Renvoie vrai si la distance de détail a changé (le LOD
    /// est alors à recalculer).
    pub fn apply_graphics_quality(&mut self, quality: GraphicsQuality) -> bool {
        let e = quality.effects();
        self.shadow_settings.enabled = e.shadows;
        self.shadow_settings.cloud_shadows = e.cloud_shadows;
        self.atmosphere_settings.god_rays = e.god_rays;
        self.post_settings.bloom_enabled = e.bloom;
        let lod_changed = self.video_settings.lod_distance != e.lod_distance;
        self.video_settings.lod_distance = e.lod_distance;
        lod_changed
    }

    /// Une image a duré `ms` millisecondes : affichée quelques secondes sous le compteur FPS
    pub fn set_hitch(&mut self, ms: f32) {
        self.hitch = Some((ms, std::time::Instant::now()));
    }

    pub fn set_fps(&mut self, fps: u32) {
        self.current_fps = fps;
    }

    pub fn set_settings_open(&mut self, is_open: bool) {
        self.is_settings_open = is_open;
        self.rebinding = None;
        if is_open {
            self.menu_page = MenuPage::Main;
            self.set_inventory_open(false);
        }
    }

    /// Ouvre ou referme l'écran d'inventaire. Refermé, la pile tenue à la souris retourne dans
    /// l'inventaire.
    pub fn set_inventory_open(&mut self, open: bool) {
        if !open {
            self.inventory.return_held();
        }
        self.inventory_open = open;
    }

    #[cfg(test)]
    pub fn toggle_settings(&mut self) -> bool {
        self.set_settings_open(!self.is_settings_open);
        self.is_settings_open
    }

    /// Ouvre le menu de pause directement sur une page
    #[cfg(test)]
    pub fn open_menu_at(&mut self, page: MenuPage) {
        self.set_settings_open(true);
        self.menu_page = page;
    }

    /// Touche Échap : ouvre le menu, remonte d'un sous-menu vers l'accueil, ou referme le menu
    /// depuis l'accueil. Renvoie l'état d'ouverture du menu.
    pub fn handle_escape(&mut self) -> bool {
        if self.inventory_open {
            self.set_inventory_open(false);
        } else if !self.is_settings_open {
            self.set_settings_open(true);
        } else if self.rebinding.is_some() {
            self.rebinding = None;
        } else if self.menu_page != MenuPage::Main {
            self.menu_page = MenuPage::Main;
        } else {
            self.set_settings_open(false);
        }
        self.is_settings_open
    }

    /// Attribue la touche pressée à l'action en attente de la page Commandes et enregistre
    pub fn finish_rebind(&mut self, key: KeyCode) {
        let Some(action) = self.rebinding else { return };
        if is_reserved_key(key) {
            return;
        }
        self.rebinding = None;
        let swapped = self.controls.rebind(action, key);
        self.rebind_notice = swapped.map(|other| {
            format!(
                "{} était utilisée par « {} », qui passe sur {}.",
                self.controls.key_label(key),
                other.label(),
                self.controls.action_key_label(other)
            )
        });
        self.controls.save();
    }

    /// Capture ou libère le pointeur. À la capture, egui oublie le survol en cours.
    pub fn set_pointer_captured(&mut self, captured: bool) {
        if captured && !self.pointer_captured {
            self.raw_input.events.push(egui::Event::PointerGone);
        }
        self.pointer_captured = captured;
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
    pub fn handle_event(&mut self, event: &WindowEvent) {
        herocraft_common::prof_scope!("ui::handle_event");
        let ppp = self.scale_factor.max(0.1_f32);

        // Souris capturée par le jeu : le curseur est caché et ne doit rien survoler ni cliquer
        if self.pointer_captured {
            return;
        }

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

            // Clavier : saisie dans les champs de texte (nom du joueur, adresse du serveur)
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.raw_input.modifiers = egui::Modifiers {
                    alt: state.alt_key(),
                    ctrl: state.control_key(),
                    shift: state.shift_key(),
                    mac_cmd: false,
                    command: state.control_key(),
                };
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let modifiers = self.raw_input.modifiers;
                if let Some(key) = egui_key(&event.logical_key) {
                    self.raw_input.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: event.repeat, modifiers });
                }
                if pressed && !modifiers.ctrl {
                    if let Some(text) = event.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control())) {
                        self.raw_input.events.push(egui::Event::Text(text.to_string()));
                    }
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
    pub fn update_ui(&mut self, is_ground: bool) -> (egui::FullOutput, Option<UiAction>) {
        herocraft_common::prof_scope!("ui::update_ui");
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

        // Le contexte egui est un Arc : le cloner libère `self` pour les méthodes de dessin
        let egui_ctx = self.ctx.clone();
        let full_output = egui_ctx.run(raw_input, |ctx| {
            // 1. Réticule de visée en mode FPS (quand le menu est fermé)
            if is_ground && !self.is_settings_open && !self.inventory_open && self.creator.is_none() && !self.player.is_dead() {
                let center = ctx.screen_rect().center();
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("crosshair")));
                let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(180));
                painter.line_segment([center - egui::vec2(6.0_f32, 0.0_f32), center + egui::vec2(6.0_f32, 0.0_f32)], stroke);
                painter.line_segment([center - egui::vec2(0.0_f32, 6.0_f32), center + egui::vec2(0.0_f32, 6.0_f32)], stroke);
                painter.circle_filled(center, 1.2_f32, egui::Color32::from_rgb(255, 215, 0));
            }

            // Noms des autres joueurs au-dessus de leur tête
            if !self.is_settings_open && !self.name_tags.is_empty() {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("name_tags")));
                for (pos, name) in &self.name_tags {
                    let pos = egui::pos2(pos.x / ppp, pos.y / ppp);
                    let galley = painter.layout_no_wrap(name.clone(), egui::FontId::proportional(14.0_f32), egui::Color32::WHITE);
                    let rect = egui::Rect::from_center_size(pos, galley.size()).expand(3.0_f32);
                    painter.rect_filled(rect, 3.0_f32, egui::Color32::from_black_alpha(140));
                    painter.galley(rect.min + egui::vec2(3.0_f32, 3.0_f32), galley, egui::Color32::WHITE);
                }
            }

            // Au sol : classe en bas à gauche, jauges en bas à droite, voile rouge après une
            // blessure, barre d'objets au centre et écran d'inventaire par-dessus le jeu
            // Créateur de légende : il tient tout l'écran, menu de pause excepté
            if self.creator.is_some() && !self.is_settings_open {
                let event = self.creator.as_mut().and_then(|creator| creator.show(ctx));
                match event {
                    Some(CreatorEvent::Sealed(legend)) => {
                        self.sealed_legend = Some(*legend);
                        action = Some(UiAction::LegendSealed);
                    }
                    Some(CreatorEvent::Enter) => self.creator = None,
                    None => {}
                }
            } else if is_ground && !self.is_settings_open {
                self.draw_vitals_hud(ctx);
                if self.player.is_dead() {
                    self.draw_death_screen(ctx, &mut action);
                } else if self.inventory_open {
                    self.draw_inventory_screen(ctx);
                } else {
                    self.draw_hotbar(ctx);
                }
            }

            // Chat : dernières lignes, ou saisie ouverte par Entrée
            if !self.is_settings_open && self.creator.is_none() {
                self.chat.draw(ctx);
            }

            // 3. Compteur FPS en haut à droite
            self.draw_status_hud(ctx);

            // 4. Menu de pause (Échap) : voile en fondu puis panneau centré
            let fade = ctx.animate_bool_with_time(egui::Id::new("pause_backdrop"), self.is_settings_open, 0.15_f32);
            if fade > 0.0_f32 {
                ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("backdrop")))
                    .rect_filled(ctx.screen_rect(), 0.0_f32, egui::Color32::from_black_alpha((170.0_f32 * fade) as u8));
            }
            if self.is_settings_open {
                self.draw_pause_menu(ctx, &mut action);
            }
        });

        (full_output, action)
    }

    /// Barre d'objets en bas de l'écran : la case choisie est cerclée d'or, le nom de son bloc
    /// s'affiche au-dessus
    fn draw_hotbar(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let bottom = screen.bottom() - SCREEN_MARGIN;
        egui::Area::new(egui::Id::new("hotbar"))
            .order(egui::Order::Background)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(egui::pos2(screen.center().x, bottom))
            .interactable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let selected = self.inventory.slot(self.inventory.selected());
                    let name = selected.map(|s| s.item.name()).unwrap_or("Main vide");
                    // Sur un fond clair (sable, neige), le nom reste lisible sur sa pastille sombre
                    let galley = ui.painter().layout_no_wrap(name.to_string(), egui::FontId::proportional(12.5_f32), TEXT_SOFT);
                    let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(16.0_f32, 4.0_f32), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0_f32, egui::Color32::from_black_alpha(150));
                    ui.painter().galley(rect.min + egui::vec2(8.0_f32, 2.0_f32), galley, TEXT_SOFT);
                    hud_frame(egui::Margin::same(6.0_f32)).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(3.0_f32, 0.0_f32);
                        ui.horizontal(|ui| {
                            for i in 0..HOTBAR_SLOTS {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(HOTBAR_SLOT_SIZE, HOTBAR_SLOT_SIZE), egui::Sense::hover());
                                paint_slot(ui.painter(), &self.icons, rect, self.inventory.slot(i), i == self.inventory.selected(), false);
                                ui.painter().text(
                                    rect.left_top() + egui::vec2(4.0_f32, 2.0_f32),
                                    egui::Align2::LEFT_TOP,
                                    (i + 1).to_string(),
                                    egui::FontId::proportional(9.5_f32),
                                    TEXT_DIM,
                                );
                            }
                        });
                    });
                });
            });
    }

    /// Écran d'inventaire : sac de 27 cases au-dessus de la barre d'objets, et à côté le catalogue
    /// de tous les blocs et objets. Clic gauche : prendre, poser, fusionner ou échanger une pile ;
    /// clic droit : prendre la moitié ou poser un exemplaire. Dans le catalogue, clic gauche : une
    /// pile entière, clic droit : un seul.
    fn draw_inventory_screen(&mut self, ctx: &egui::Context) {
        let open_key = self.controls.action_key_label(Action::OpenInventory);
        let mut clicked: Option<(usize, bool)> = None;
        let mut taken: Option<(ItemId, bool)> = None;
        let mut hovered: Option<(ItemId, Option<u16>)> = None;
        egui::Area::new(egui::Id::new("inventory_screen"))
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                crate::ui_skin::SkinFrame::panel()
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Inventaire").font(crate::ui_skin::title_font(20.0_f32)).color(GOLD_BRIGHT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("[{}] ou [Échap] pour fermer", open_key)).size(11.0_f32).color(TEXT_DIM));
                            });
                        });
                        ui.add_space(4.0_f32);
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                let mut slot_row = |ui: &mut egui::Ui, range: std::ops::Range<usize>| {
                                    ui.spacing_mut().item_spacing = egui::vec2(4.0_f32, 4.0_f32);
                                    ui.horizontal(|ui| {
                                        for i in range {
                                            let size = egui::vec2(INVENTORY_SLOT_SIZE, INVENTORY_SLOT_SIZE);
                                            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                                            let selected = i == self.inventory.selected();
                                            let stack = self.inventory.slot(i);
                                            paint_slot(ui.painter(), &self.icons, rect, stack, selected, response.hovered());
                                            if response.hovered() {
                                                hovered = stack.map(|s| (s.item, Some(s.count)));
                                            }
                                            if response.clicked() {
                                                clicked = Some((i, false));
                                            } else if response.secondary_clicked() {
                                                clicked = Some((i, true));
                                            }
                                        }
                                    });
                                };
                                caption(ui, "SAC");
                                for row in 0..TOTAL_SLOTS / HOTBAR_SLOTS - 1 {
                                    let start = HOTBAR_SLOTS + row * HOTBAR_SLOTS;
                                    slot_row(ui, start..start + HOTBAR_SLOTS);
                                }
                                ui.add_space(6.0_f32);
                                caption(ui, "BARRE D'OBJETS");
                                slot_row(ui, 0..HOTBAR_SLOTS);
                                ui.add_space(2.0_f32);
                                ui.label(
                                    egui::RichText::new("Clic gauche : prendre / poser une pile · Clic droit : moitié / un seul")
                                        .size(11.0_f32)
                                        .color(TEXT_DIM),
                                );
                            });
                            ui.add_space(10.0_f32);
                            ui.vertical(|ui| {
                                caption(ui, "CATALOGUE");
                                self.catalog.draw(ui, &self.icons, &mut taken, &mut hovered);
                            });
                        });
                    });
            });

        match clicked {
            Some((i, false)) => self.inventory.click(i),
            Some((i, true)) => self.inventory.click_secondary(i),
            None => {}
        }
        if let Some((item, one)) = taken {
            self.inventory.add_many(item, if one { 1 } else { item.max_stack() });
        }

        // Pile tenue sous le curseur, sinon fiche de l'objet survolé
        if let Some(held) = self.inventory.held() {
            if let Some(pos) = ctx.pointer_latest_pos() {
                let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("held_stack")));
                let rect = egui::Rect::from_center_size(pos, egui::vec2(INVENTORY_SLOT_SIZE, INVENTORY_SLOT_SIZE));
                paint_stack(&painter, &self.icons, rect, held);
            }
        } else if let Some((item, count)) = hovered {
            egui::show_tooltip_at_pointer(ctx, egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("inventory_tip")), egui::Id::new("inventory_tip"), |ui| {
                item_tooltip(ui, item, count);
            });
        }
    }

    /// Écran de mort : voile sombre et rouge, cause de la mort, puis bouton de réapparition une
    /// fois le délai passé (Saut fait de même)
    fn draw_death_screen(&mut self, ctx: &egui::Context, action: &mut Option<UiAction>) {
        let Some(death) = self.player.death.clone() else { return };
        let fade = (death.at.elapsed().as_secs_f32() / 0.8_f32).min(1.0_f32);
        ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("death_backdrop")))
            .rect_filled(ctx.screen_rect(), 0.0_f32, egui::Color32::from_rgba_unmultiplied(60, 0, 0, (170.0_f32 * fade) as u8));
        let wait = self.player.respawn_in().unwrap_or(0.0_f32);
        egui::Area::new(egui::Id::new("death_screen"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0_f32, -30.0_f32))
            .show(ctx, |ui| {
                ui.set_opacity(fade);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Vous êtes mort").font(crate::ui_skin::title_font(44.0_f32)).color(DANGER));
                    ui.add_space(4.0_f32);
                    ui.label(egui::RichText::new(&death.cause).size(16.0_f32).color(TEXT_SOFT));
                    ui.add_space(24.0_f32);
                    let label = if wait > 0.0_f32 { format!("Réapparaître ({:.0})", wait.ceil()) } else { "Réapparaître".to_string() };
                    let button = SkinButton::new(egui::RichText::new(label).size(18.0_f32)).min_size(egui::vec2(220.0_f32, 48.0_f32));
                    if ui.add_enabled(wait <= 0.0_f32, button).clicked() {
                        *action = Some(UiAction::Respawn);
                    }
                });
            });
    }

    /// HUD du personnage : classe, niveau, expérience et jauges de vie, de mana et d'endurance
    fn draw_vitals_hud(&mut self, ctx: &egui::Context) {
        let stats = &self.player.stats;
        // Blessure : éclair rouge sur tout l'écran et bords rougis qui s'estompent
        let mut flash = 0.0_f32;
        if let Some(hurt) = self.player.hurt_at {
            let t = hurt.elapsed().as_secs_f32() / HURT_FLASH_SECS;
            if t < 1.0_f32 {
                flash = (1.0_f32 - t).powf(1.5_f32) * (0.5_f32 + 0.5_f32 * self.player.hurt_strength);
            }
        }
        // Vie basse : les bords battent comme un cœur
        let hp_fraction = stats.hp / stats.max_hp.max(1.0_f32);
        let mut pulse = 0.0_f32;
        if hp_fraction < LOW_HP_FRACTION && !self.player.is_dead() {
            let beat = 0.6_f32 + 0.4_f32 * (ctx.input(|i| i.time) as f32 * 5.0_f32).sin();
            pulse = (1.0_f32 - hp_fraction / LOW_HP_FRACTION) * 0.7_f32 * beat;
        }
        let vignette = flash.max(pulse);
        if vignette > 0.0_f32 {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("hurt_flash")));
            painter.rect_filled(ctx.screen_rect(), 0.0_f32, egui::Color32::from_rgba_unmultiplied(200, 20, 20, (70.0_f32 * flash) as u8));
            painter.add(red_vignette(ctx.screen_rect(), (220.0_f32 * vignette) as u8));
        }

        let screen = ctx.screen_rect();
        // Classe, niveau et expérience en bas à gauche
        egui::Area::new(egui::Id::new("class_hud"))
            .order(egui::Order::Background)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(SCREEN_MARGIN, screen.bottom() - SCREEN_MARGIN))
            .interactable(false)
            .show(ctx, |ui| {
                hud_frame(egui::Margin::symmetric(12.0_f32, 8.0_f32)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0_f32, 4.0_f32);
                    if let Some(legend) = &self.legend {
                        ui.label(egui::RichText::new(&legend.appearance.nom).size(15.0_f32).strong().color(TEXT_SOFT));
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(stats.class.label()).strong().color(GOLD_BRIGHT));
                        ui.label(egui::RichText::new(format!("Niv. {}", stats.level)).color(TEXT_SOFT));
                    });
                    let next = stats.exp_for_next_level();
                    gauge(ui, 120.0_f32, 6.0_f32, stats.experience as f32 / next.max(1) as f32, GOLD, None);
                    ui.label(egui::RichText::new(format!("{} / {} XP", stats.experience, next)).size(11.0_f32).color(TEXT_DIM));
                });
            });

        // Vie, mana et endurance empilées en bas à droite, sans cadre
        egui::Area::new(egui::Id::new("vitals_hud"))
            .order(egui::Order::Background)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(egui::pos2(screen.right() - SCREEN_MARGIN, screen.bottom() - SCREEN_MARGIN))
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0_f32, 5.0_f32);
                let gauges = [
                    ("Vie", stats.hp, stats.max_hp, egui::Color32::from_rgb(200, 45, 45)),
                    ("Mana", stats.mana, stats.max_mana, egui::Color32::from_rgb(55, 105, 220)),
                    ("Endurance", stats.stamina, stats.max_stamina, egui::Color32::from_rgb(70, 175, 70)),
                ];
                for (name, value, max, color) in gauges {
                    let text = format!("{name}  {:.0} / {:.0}", value.max(0.0_f32).ceil(), max);
                    gauge(ui, GAUGE_WIDTH, 16.0_f32, value / max.max(1.0_f32), color, Some(&text));
                }
            });
    }

    /// Compteur FPS en haut à droite, seul élément permanent de l'écran
    fn draw_status_hud(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("top_right_hud"))
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(egui::pos2(screen.right() - SCREEN_MARGIN, SCREEN_MARGIN))
            .interactable(false)
            .show(ctx, |ui| {
                hud_frame(egui::Margin::symmetric(12.0_f32, 8.0_f32)).show(ui, |ui| {
                    let fps_color = if self.current_fps >= 55 {
                        egui::Color32::from_rgb(80, 220, 100)
                    } else if self.current_fps >= 30 {
                        egui::Color32::from_rgb(240, 190, 50)
                    } else {
                        egui::Color32::from_rgb(240, 80, 80)
                    };
                    ui.label(egui::RichText::new(format!("{} FPS", self.current_fps)).strong().color(fps_color));
                    // Le compteur est une moyenne : un gel d'une seconde y passe presque inaperçu
                    if let Some((ms, _)) = self.hitch.filter(|(_, at)| at.elapsed().as_secs_f32() < HITCH_DISPLAY_SECS) {
                        ui.label(egui::RichText::new(format!("à-coup {ms:.0} ms")).size(11.0_f32).color(egui::Color32::from_rgb(240, 80, 80)));
                    }
                });
            });
    }

    /// Menu de pause : panneau centré, borné à l'écran, dont le corps défile au besoin
    fn draw_pause_menu(&mut self, ctx: &egui::Context, action: &mut Option<UiAction>) {
        let screen = ctx.screen_rect();
        let page = self.menu_page;
        // Largeur du contenu ; le panneau de planches y ajoute ses bords
        let content_width = match page {
            MenuPage::Main => 304.0_f32,
            MenuPage::Options => 644.0_f32,
            MenuPage::Controls => 504.0_f32,
            MenuPage::Multiplayer => 484.0_f32,
        };
        // Sur une petite fenêtre, le cadre de planches rétrécit pour laisser la place au contenu
        let panel_scale = if screen.height() < 400.0_f32 || screen.width() < 400.0_f32 {
            1.0_f32 / 3.0_f32
        } else if screen.height() < 480.0_f32 || screen.width() < 560.0_f32 {
            2.0_f32 / 3.0_f32
        } else {
            1.0_f32
        };
        let margin = crate::ui_skin::panel_margin(panel_scale);
        let target_width = content_width + margin.left + margin.right;
        let width = target_width.min(screen.width() - 2.0_f32 * SCREEN_MARGIN).max(240.0_f32);
        let max_height = (screen.height() - 2.0_f32 * SCREEN_MARGIN).max(200.0_f32);
        let inner_width = width - margin.left - margin.right;
        let inner_max_height = max_height - margin.top - margin.bottom;
        // Hauteur du pied de page mesurée à la frame précédente, pour réserver sa place
        let footer_id = egui::Id::new(("pause_menu_footer", page));
        let footer_height = ctx.data(|d| d.get_temp::<f32>(footer_id)).unwrap_or(48.0_f32);

        egui::Area::new(egui::Id::new(("pause_menu", page)))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                crate::ui_skin::SkinFrame::panel_scaled(panel_scale)
                    .show(ui, |ui| {
                        ui.set_width(inner_width);
                        ui.set_max_width(inner_width);

                        let top = ui.cursor().top();
                        self.draw_menu_header(ui, inner_width);
                        let header_height = ui.cursor().top() - top;
                        let spacing = ui.spacing().item_spacing.y;
                        let body_height = (inner_max_height - header_height - footer_height - spacing).max(60.0_f32);

                        match page {
                            MenuPage::Main => self.draw_main_page(ui, body_height, action),
                            MenuPage::Options => self.draw_options_page(ui, inner_width, body_height, action),
                            MenuPage::Controls => self.draw_controls_page(ui, body_height),
                            MenuPage::Multiplayer => self.draw_multiplayer_page(ui, body_height, action),
                        }

                        let footer_top = ui.cursor().top();
                        self.draw_menu_footer(ui, page, action);
                        let measured = ui.min_rect().bottom() - footer_top;
                        ui.ctx().data_mut(|d| d.insert_temp(footer_id, measured));
                    });
            });
    }

    /// En-tête : titre doré sur l'accueil, fil d'Ariane et retour dans les sous-menus
    fn draw_menu_header(&mut self, ui: &mut egui::Ui, inner_width: f32) {
        match self.menu_page {
            MenuPage::Main => {
                ui.vertical_centered(|ui| {
                    crate::ui_skin::plaque_title(ui, "HEROCRAFT", 28.0_f32);
                    ui.add_space(-4.0_f32);
                    ui.label(egui::RichText::new("— PAUSE —").size(12.0_f32).color(TEXT_DIM));
                });
                ui.add_space(2.0_f32);
                ornament(ui, inner_width);
            }
            page => {
                let title = match page {
                    MenuPage::Options => "⚙  Options",
                    MenuPage::Multiplayer => "🌍  Multijoueur",
                    _ => "⌨  Commandes",
                };
                ui.horizontal(|ui| {
                    if ui.add(SkinButton::new("⬅ Retour").kind(ButtonKind::Secondary).small()).on_hover_text("Revenir au menu (Échap)").clicked() {
                        self.menu_page = MenuPage::Main;
                    }
                    ui.add_space(4.0_f32);
                    ui.label(egui::RichText::new("Pause  ›").size(13.0_f32).color(TEXT_DIM));
                    ui.label(egui::RichText::new(title).font(crate::ui_skin::title_font(19.0_f32)).color(GOLD_BRIGHT));
                });
                ornament(ui, inner_width);
            }
        }
    }

    /// Accueil du menu : grandes entrées de navigation
    fn draw_main_page(&mut self, ui: &mut egui::Ui, body_height: f32, action: &mut Option<UiAction>) {
        egui::ScrollArea::vertical()
            .id_salt("pause_main_scroll")
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let size = egui::vec2(ui.available_width(), 48.0_f32);
                ui.spacing_mut().item_spacing.y = 4.0_f32;

                if menu_entry(ui, size, "▶   Reprendre la partie", None).clicked() {
                    self.set_settings_open(false);
                    *action = Some(UiAction::ResumeGame);
                }
                if menu_entry(ui, size, "🌍   Multijoueur", Some("Ouvrir sa partie au réseau local, rejoindre une partie")).clicked() {
                    self.menu_page = MenuPage::Multiplayer;
                }
                if menu_entry(ui, size, "⚙   Options", Some("Affichage, éclairage, nuages, rendu…")).clicked() {
                    self.menu_page = MenuPage::Options;
                }
                if menu_entry(ui, size, "⌨   Commandes", Some("Touches modifiables et sensibilité de la souris")).clicked() {
                    self.menu_page = MenuPage::Controls;
                }

                ui.add_space(2.0_f32);
                ui.separator();
                ui.add_space(2.0_f32);

                // Quitter enregistre le monde et ferme le jeu, sans confirmation
                if ui
                    .add(SkinButton::new(egui::RichText::new("🚪   Quitter le jeu").size(16.0_f32)).kind(ButtonKind::MenuDanger).min_size(size))
                    .clicked()
                {
                    *action = Some(UiAction::QuitGame);
                }
            });
    }

    /// Page Multijoueur : état de la partie, nom du joueur, ouverture au réseau local, parties
    /// trouvées sur le réseau local et connexion par adresse
    fn draw_multiplayer_page(&mut self, ui: &mut egui::Ui, body_height: f32, action: &mut Option<UiAction>) {
        egui::ScrollArea::vertical()
            .id_salt("multiplayer_scroll")
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let mp = &mut self.multiplayer;
                let size = egui::vec2(ui.available_width(), 48.0_f32);

                // État de la partie
                let (status, color) = match &mp.status {
                    NetStatus::Solo => ("Partie solo".to_string(), TEXT_SOFT),
                    NetStatus::Connecting(addr) => (format!("Connexion à {addr}…"), GOLD_BRIGHT),
                    NetStatus::Hosting { port } => (format!("Partie ouverte au réseau local (port {port})"), GOLD_BRIGHT),
                    NetStatus::Online(addr) => (format!("En ligne sur {addr}"), GOLD_BRIGHT),
                };
                ui.horizontal(|ui| {
                    if matches!(mp.status, NetStatus::Connecting(_)) {
                        ui.spinner();
                    }
                    ui.label(egui::RichText::new(status).size(15.0_f32).strong().color(color));
                });
                if let NetStatus::Hosting { .. } = mp.status {
                    ui.label(
                        egui::RichText::new("Les joueurs du réseau local la voient dans leur page Multijoueur.").size(11.5_f32).color(TEXT_DIM),
                    );
                }
                if let Some(notice) = &mp.notice {
                    ui.label(egui::RichText::new(notice).size(12.0_f32).color(DANGER));
                }
                ui.add_space(4.0_f32);

                match mp.status {
                    NetStatus::Solo => {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Votre nom").color(TEXT_SOFT));
                            ui.add(egui::TextEdit::singleline(&mut mp.pseudo).char_limit(24).desired_width(180.0_f32));
                        });
                        ui.add_space(4.0_f32);
                        let can_play = !mp.pseudo.trim().is_empty();
                        let open = ui
                            .add_enabled(can_play, SkinButton::new(egui::RichText::new("🌍   Ouvrir au réseau local").size(16.0_f32)).kind(ButtonKind::Menu).min_size(size))
                            .on_hover_text("Lance un serveur sur ce monde : les joueurs du réseau local peuvent vous rejoindre");
                        if open.clicked() {
                            *action = Some(UiAction::OpenToLan);
                        }

                        ui.add_space(6.0_f32);
                        caption(ui, "PARTIES SUR LE RÉSEAU LOCAL");
                        if !mp.lan_search_ok {
                            ui.label(egui::RichText::new("Recherche impossible : le port 7778 est déjà utilisé sur cette machine.").size(12.0_f32).color(TEXT_DIM));
                        } else if mp.lan_servers.is_empty() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new("Recherche de parties…").size(12.0_f32).color(TEXT_DIM));
                            });
                        }
                        for server in &mp.lan_servers {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&server.name).strong().color(TEXT_SOFT));
                                    let players = if server.players == 1 { "1 joueur".to_string() } else { format!("{} joueurs", server.players) };
                                    ui.label(egui::RichText::new(format!("{}  ·  {players}", server.addr)).monospace().size(11.0_f32).color(TEXT_DIM));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add_enabled(can_play, SkinButton::new("▶ Rejoindre").small()).clicked() {
                                        mp.join_target = server.addr.clone();
                                        *action = Some(UiAction::JoinServer);
                                    }
                                });
                            });
                            ui.separator();
                        }

                        ui.add_space(6.0_f32);
                        caption(ui, "REJOINDRE PAR ADRESSE");
                        ui.horizontal(|ui| {
                            let field = ui.add(
                                egui::TextEdit::singleline(&mut mp.address)
                                    .hint_text(format!("ip:{}", herocraft_common::DEFAULT_PORT))
                                    .desired_width(220.0_f32),
                            );
                            let submit = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let can_join = can_play && !mp.address.trim().is_empty();
                            if (ui.add_enabled(can_join, SkinButton::new("▶ Se connecter").small()).clicked() || submit) && can_join {
                                mp.join_target = mp.address.trim().to_string();
                                *action = Some(UiAction::JoinServer);
                            }
                        });
                    }
                    NetStatus::Connecting(_) => {}
                    NetStatus::Hosting { .. } | NetStatus::Online(_) => {
                        let label = if matches!(mp.status, NetStatus::Hosting { .. }) {
                            "✖   Fermer la partie au réseau local"
                        } else {
                            "🚪   Se déconnecter"
                        };
                        let leave = ui
                            .add(SkinButton::new(egui::RichText::new(label).size(16.0_f32)).kind(ButtonKind::MenuDanger).min_size(size))
                            .on_hover_text("Revient à la partie solo");
                        if leave.clicked() {
                            *action = Some(UiAction::LeaveServer);
                        }
                    }
                }
            });
    }

    /// Sous-menu Options : onglets latéraux (ou en bandeau sur écran étroit) et réglages défilants
    fn draw_options_page(&mut self, ui: &mut egui::Ui, inner_width: f32, body_height: f32, action: &mut Option<UiAction>) {
        let wide = inner_width >= OPTIONS_SIDEBAR_MIN_WIDTH;

        if wide {
            let row = ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(OPTIONS_SIDEBAR_WIDTH, body_height),
                    egui::Layout::top_down_justified(egui::Align::LEFT),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("options_tabs_scroll")
                            .max_height(body_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0_f32;
                                for tab in OptionsTab::ALL {
                                    let text = egui::RichText::new(format!("{}  {}", tab.icon(), tab.label())).size(15.0_f32);
                                    let response = ui.add(
                                        SkinButton::new(text)
                                            .kind(ButtonKind::ListItem)
                                            .selected(self.options_tab == tab)
                                            .min_size(egui::vec2(ui.available_width(), 34.0_f32)),
                                    );
                                    if response.on_hover_text(tab.subtitle()).clicked() {
                                        self.options_tab = tab;
                                    }
                                }
                            });
                    },
                );
                // Espace pour le filet vertical, tracé après coup à la hauteur exacte de la rangée
                // (un Separator vertical prendrait toute la hauteur disponible et ferait grandir le menu)
                ui.add_space(12.0_f32);
                let content_width = ui.available_width();
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    self.draw_options_content(ui, content_width, body_height, action);
                });
            });
            let x = row.response.rect.left() + OPTIONS_SIDEBAR_WIDTH + 2.0_f32 + ui.spacing().item_spacing.x;
            ui.painter().vline(x, row.response.rect.y_range(), egui::Stroke::new(1.0_f32, GOLD.gamma_multiply(0.35_f32)));
        } else {
            let top = ui.cursor().top();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0_f32, 4.0_f32);
                for tab in OptionsTab::ALL {
                    let selected = self.options_tab == tab;
                    let text = if selected { format!("{} {}", tab.icon(), tab.label()) } else { tab.icon().to_string() };
                    if ui.add(SkinButton::new(text).kind(ButtonKind::Secondary).selected(selected).small()).on_hover_text(tab.label()).clicked() {
                        self.options_tab = tab;
                    }
                }
            });
            ui.separator();
            let remaining = (body_height - (ui.cursor().top() - top)).max(60.0_f32);
            self.draw_options_content(ui, inner_width, remaining, action);
        }
    }

    /// Réglages de l'onglet courant, dans une zone défilante
    fn draw_options_content(&mut self, ui: &mut egui::Ui, width: f32, height: f32, action: &mut Option<UiAction>) {
        let tab = self.options_tab;
        let top = ui.cursor().top();
        ui.label(egui::RichText::new(format!("{}  {}", tab.icon(), tab.label())).font(crate::ui_skin::title_font(18.0_f32)).color(GOLD_BRIGHT));
        ui.add_space(-6.0_f32);
        ui.label(egui::RichText::new(tab.subtitle()).size(11.0_f32).color(TEXT_DIM));
        ui.add_space(2.0_f32);

        let scroll_height = (height - (ui.cursor().top() - top)).max(60.0_f32);
        egui::ScrollArea::vertical()
            .id_salt(("options_content_scroll", tab))
            .max_height(scroll_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Curseurs aussi larges que possible sans déborder (libellés ~45 %, valeur ~60 pt)
                ui.spacing_mut().slider_width = (width * 0.55_f32 - 70.0_f32).clamp(80.0_f32, 240.0_f32);
                ui.add_space(4.0_f32);
                match tab {
                    OptionsTab::Display => self.draw_display_tab(ui, action),
                    OptionsTab::Lighting => self.draw_lighting_tab(ui),
                    OptionsTab::Clouds => self.draw_clouds_tab(ui),
                    OptionsTab::Hdr => self.draw_hdr_tab(ui),
                    OptionsTab::Shadows => self.draw_shadows_tab(ui),
                    OptionsTab::Atmosphere => self.draw_atmosphere_tab(ui),
                    OptionsTab::Audio => self.draw_audio_tab(ui, action),
                }
                ui.add_space(6.0_f32);
            });
    }

    fn draw_display_tab(&mut self, ui: &mut egui::Ui, action: &mut Option<UiAction>) {
        let quality = self.graphics_quality();
        let mut chosen = None;
        settings_grid(ui, "display_grid", |ui| {
            row_label(ui, "Qualité graphique", "Règle d'un coup les ombres, les rayons crépusculaires, le bloom et la distance de détail");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0_f32;
                for q in GraphicsQuality::ALL {
                    if ui.add(SkinButton::new(q.label()).kind(ButtonKind::Secondary).selected(quality == Some(q)).small()).clicked() && quality != Some(q) {
                        chosen = Some(q);
                    }
                }
                if quality.is_none() {
                    ui.label(egui::RichText::new("Personnalisée").size(11.0_f32).color(TEXT_DIM));
                }
            });
            ui.end_row();

            row_label(ui, "Synchronisation verticale", "Aligne les images sur l'écran pour éviter les déchirures");
            if toggle_switch(ui, &mut self.video_settings.vsync).changed() {
                *action = Some(UiAction::ToggleVsync);
            }
            ui.end_row();

            row_label(ui, "Plein écran", "Plein écran sans bordure sur le moniteur courant");
            if toggle_switch(ui, &mut self.video_settings.fullscreen).changed() {
                *action = Some(UiAction::ToggleFullscreen);
            }
            ui.end_row();

            row_label(ui, "Distance de détail (LOD)", "Distance à laquelle le terrain garde sa pleine résolution");
            let lod_choices = [
                (LodDistanceSetting::Courte, "Courte"),
                (LodDistanceSetting::Normale, "Normale"),
                (LodDistanceSetting::Elevee, "Élevée"),
            ];
            if choice_row(ui, &mut self.video_settings.lod_distance, &lod_choices) {
                *action = Some(UiAction::SetLodDistance);
            }
            ui.end_row();

            row_label(ui, "Limite d'images", "Plafond d'images par seconde (s'ajoute à la V-Sync)");
            let fps_choices = [FpsLimitSetting::Fps60, FpsLimitSetting::Fps120, FpsLimitSetting::Unlimited].map(|f| (f, f.label()));
            if choice_row(ui, &mut self.video_settings.fps_limit, &fps_choices) {
                *action = Some(UiAction::SetFpsLimit);
            }
            ui.end_row();

            slider_row(
                ui,
                "Aplatissement de l'horizon",
                &mut self.video_settings.flattening,
                0.0..=1.0,
                "Vue au sol : la planète est dessinée moins courbe pour voir plus loin (0 = vraie courbure)",
            );
        });
        if let Some(q) = chosen {
            if self.apply_graphics_quality(q) {
                *action = Some(UiAction::SetLodDistance);
            }
        }
        ui.add_space(4.0_f32);
        ui.label(egui::RichText::new(format!("Détail actuel : {}", self.video_settings.lod_distance.label())).size(11.0_f32).color(TEXT_DIM));
    }

    fn draw_lighting_tab(&mut self, ui: &mut egui::Ui) {
        let l = &mut self.lighting_settings;
        settings_grid(ui, "lighting_grid", |ui| {
            slider_row(ui, "Lumière du Soleil", &mut l.sun_intensity, 0.0..=3.0, "Multiplicateur de la lumière directe du Soleil");
            slider_row(ui, "Lumière de la Lune", &mut l.moon_intensity, 0.0..=3.0, "Clarté bleutée des nuits de pleine lune");
            slider_row(ui, "Ambiance de jour", &mut l.day_ambient, 0.0..=3.0, "Lumière diffuse du ciel en journée");
            slider_row(ui, "Ambiance de nuit", &mut l.night_ambient, 0.0..=4.0, "Obscurité nocturne et éclat des étoiles");
            slider_row(ui, "Vitesse céleste", &mut l.moon_speed, 0.0..=5.0, "Jour de 20 min, Lune de 21 min à 1.0 ; 0 fige le ciel");
        });
        if reset_button(ui, "Réinitialiser l'éclairage") {
            let fog = self.lighting_settings.fog_intensity;
            let clouds = self.lighting_settings.cloud_density;
            self.lighting_settings = LightingSettings::default();
            // La brume (onglet Atmosphère) et la densité des nuages ont leur propre onglet
            self.lighting_settings.fog_intensity = fog;
            self.lighting_settings.cloud_density = clouds;
        }
    }

    fn draw_clouds_tab(&mut self, ui: &mut egui::Ui) {
        let c = &mut self.cloud_settings;
        let mut density_changed = false;
        settings_grid(ui, "clouds_grid", |ui| {
            density_changed = slider_row(ui, "Densité", &mut c.density, 0.0..=2.0, "Couverture nuageuse ; 0 retire les nuages").changed();
            slider_row(ui, "Rondeur des arêtes", &mut c.roundness, 0.05..=0.55, "Arrondi des cubes de nuage (style marshmallow)");
            slider_row(ui, "Ombrage de sous-face", &mut c.shadow_intensity, 0.0..=2.0, "Teinte violacée sous les nuages");
            slider_row(ui, "Éclat au couchant", &mut c.sunset_glow, 0.0..=2.0, "Lueur chaude quand le Soleil est bas");
            slider_row(ui, "Grain mat", &mut c.micro_noise, 0.0..=1.0, "Micro-texture de surface");
        });
        if density_changed {
            self.lighting_settings.cloud_density = self.cloud_settings.density;
        }
        if reset_button(ui, "Réinitialiser les nuages") {
            self.cloud_settings = CloudSettings::default();
            self.lighting_settings.cloud_density = self.cloud_settings.density;
        }
    }

    fn draw_hdr_tab(&mut self, ui: &mut egui::Ui) {
        let p = &mut self.post_settings;
        settings_grid(ui, "hdr_grid", |ui| {
            slider_row(ui, "Exposition (EV)", &mut p.exposure_ev, -3.0..=3.0, "Éclaircit ou assombrit toute l'image");

            row_label(ui, "Courbe de tons", "Conversion des lumières HDR vers l'écran");
            egui::ComboBox::from_id_salt("tonemapper")
                .selected_text(p.tonemapper.label())
                .show_ui(ui, |ui| {
                    for t in Tonemapper::ALL {
                        ui.selectable_value(&mut p.tonemapper, t, t.label());
                    }
                });
            ui.end_row();

            row_label(ui, "Bloom", "Halo lumineux autour des sources très vives");
            toggle_switch(ui, &mut p.bloom_enabled);
            ui.end_row();

            let enabled = p.bloom_enabled;
            slider_row_enabled(ui, enabled, "   Intensité", &mut p.bloom_intensity, 0.0..=1.0, "Part du halo ajoutée à l'image");
            slider_row_enabled(ui, enabled, "   Seuil", &mut p.bloom_threshold, 0.0..=8.0, "Luminance à partir de laquelle un pixel diffuse");
            slider_row_enabled(ui, enabled, "   Douceur", &mut p.bloom_knee, 0.0..=3.0, "Largeur de la transition autour du seuil");
        });
        if reset_button(ui, "Réinitialiser le rendu HDR") {
            self.post_settings = PostSettings::default();
        }
    }

    fn draw_shadows_tab(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.shadow_settings;
        settings_grid(ui, "shadows_grid", |ui| {
            row_label(ui, "Ombres du soleil", "Ombres portées du relief");
            toggle_switch(ui, &mut s.enabled);
            ui.end_row();

            slider_row_enabled(ui, s.enabled, "   Force du relief", &mut s.strength, 0.0..=1.0, "Part de la lumière directe retirée à l'ombre");

            row_label(ui, "Ombres des nuages", "Les nuages assombrissent le sol");
            ui.add_enabled_ui(s.enabled, |ui| toggle_switch(ui, &mut s.cloud_shadows));
            ui.end_row();

            let clouds_on = s.enabled && s.cloud_shadows;
            slider_row_enabled(ui, clouds_on, "   Force sous les nuages", &mut s.cloud_strength, 0.0..=1.0, "Part de la lumière directe retirée sous un nuage");
        });
        if reset_button(ui, "Réinitialiser les ombres") {
            self.shadow_settings = ShadowSettings::default();
        }
    }

    fn draw_atmosphere_tab(&mut self, ui: &mut egui::Ui) {
        let a = &mut self.atmosphere_settings;
        let fog = &mut self.lighting_settings.fog_intensity;
        settings_grid(ui, "atmosphere_grid", |ui| {
            slider_row(ui, "Densité de l'air", &mut a.air_density, 0.2..=3.0, "Diffusion de Rayleigh : bleu du ciel, rougeoiement du couchant");
            slider_row(ui, "Halo solaire", &mut a.sun_halo, 0.0..=0.95, "Resserre la brume lumineuse autour du Soleil");
            slider_row(ui, "Brume", fog, 0.0..=2.0, "Voile de distance sur le relief lointain");

            row_label(ui, "Rayons crépusculaires", "Puits de lumière découpés par le relief et les nuages");
            toggle_switch(ui, &mut a.god_rays);
            ui.end_row();

            slider_row_enabled(ui, a.god_rays, "   Intensité", &mut a.god_rays_intensity, 0.0..=4.0, "1 = physique ; au-delà, rayons accentués");
        });
        if reset_button(ui, "Réinitialiser l'atmosphère") {
            self.atmosphere_settings = AtmosphereSettings::default();
            self.lighting_settings.fog_intensity = LightingSettings::default().fog_intensity;
        }
    }

    fn draw_audio_tab(&mut self, ui: &mut egui::Ui, action: &mut Option<UiAction>) {
        let a = &mut self.audio_settings;
        let mut changed = false;
        settings_grid(ui, "audio_grid", |ui| {
            row_label(ui, "Couper le son", "Silence complet, les volumes sont gardés");
            changed |= toggle_switch(ui, &mut a.muted).changed();
            ui.end_row();

            let enabled = !a.muted;
            changed |= volume_row(ui, enabled, "Volume général", &mut a.master, "Volume de tous les sons du jeu");
            changed |= volume_row(ui, enabled, "   Bruitages", &mut a.effects, "Pas, sauts, blocs cassés et posés");
            changed |= volume_row(ui, enabled, "   Ambiance", &mut a.ambience, "Vent, oiseaux le jour, grillons la nuit");
        });
        if reset_button(ui, "Réinitialiser le son") {
            self.audio_settings.reset();
            changed = true;
        }
        if changed {
            self.audio_settings.save();
            *action = Some(UiAction::PreviewSound);
        }
    }

    /// Pied de page : rappel de la touche Échap et action globale de la page
    fn draw_menu_footer(&mut self, ui: &mut egui::Ui, page: MenuPage, action: &mut Option<UiAction>) {
        ui.add_space(2.0_f32);
        ui.separator();
        ui.horizontal(|ui| {
            let hint = if page == MenuPage::Main { "reprendre" } else { "retour" };
            keycap(ui, "Échap");
            ui.label(egui::RichText::new(hint).size(11.0_f32).color(TEXT_DIM));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match page {
                MenuPage::Main => {
                    ui.label(egui::RichText::new(format!("Monde #{}", self.current_seed)).monospace().size(11.0_f32).color(TEXT_DIM));
                }
                MenuPage::Options => {
                    if ui
                        .add(SkinButton::new("↺ Tout le rendu par défaut").kind(ButtonKind::Secondary).small())
                        .on_hover_text("Éclairage, nuages, HDR, ombres et atmosphère (l'affichage est conservé)")
                        .clicked()
                    {
                        self.lighting_settings = LightingSettings::default();
                        self.cloud_settings = CloudSettings::default();
                        self.post_settings = PostSettings::default();
                        self.shadow_settings = ShadowSettings::default();
                        self.atmosphere_settings = AtmosphereSettings::default();
                    }
                }
                MenuPage::Multiplayer => {
                    ui.label(egui::RichText::new(format!("Monde #{}", self.current_seed)).monospace().size(11.0_f32).color(TEXT_DIM));
                }
                MenuPage::Controls => {
                    if ui.add(SkinButton::new("▶ Reprendre").small()).clicked() {
                        self.set_settings_open(false);
                        *action = Some(UiAction::ResumeGame);
                    }
                }
            });
        });
    }
}

/// Touches d'édition transmises à egui (le texte tapé passe à part, en `Event::Text`)
fn egui_key(key: &Key) -> Option<egui::Key> {
    Some(match key {
        Key::Named(NamedKey::Backspace) => egui::Key::Backspace,
        Key::Named(NamedKey::Delete) => egui::Key::Delete,
        Key::Named(NamedKey::Enter) => egui::Key::Enter,
        Key::Named(NamedKey::Tab) => egui::Key::Tab,
        Key::Named(NamedKey::ArrowLeft) => egui::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => egui::Key::ArrowRight,
        Key::Named(NamedKey::ArrowUp) => egui::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => egui::Key::ArrowDown,
        Key::Named(NamedKey::Home) => egui::Key::Home,
        Key::Named(NamedKey::End) => egui::Key::End,
        Key::Character(c) if c.eq_ignore_ascii_case("a") => egui::Key::A,
        _ => return None,
    })
}

/// Côté d'une case de la barre d'objets et de l'écran d'inventaire (points logiques)
const HOTBAR_SLOT_SIZE: f32 = 40.0;
const INVENTORY_SLOT_SIZE: f32 = 44.0;

/// Case d'inventaire : fond, liseré (or si choisie) et pile éventuelle
fn paint_slot(painter: &egui::Painter, icons: &ItemIcons, rect: egui::Rect, stack: Option<ItemStack>, selected: bool, hovered: bool) {
    let skin = crate::ui_skin::skin(painter.ctx());
    let slice = if selected { crate::ui_skin::SLOT.with_tex(crate::ui_skin::Tex::SlotSelected) } else { crate::ui_skin::SLOT };
    // Coins de fer à l'échelle du dessin (trois pixels de texture pour un du dessin, ramenés à deux)
    crate::ui_skin::paint_slice(painter, &skin, slice, rect, 2.0_f32 / 3.0_f32, egui::Color32::WHITE);
    if hovered {
        painter.rect_filled(rect.shrink(4.0_f32), 0.0_f32, egui::Color32::from_white_alpha(22));
    }
    if let Some(stack) = stack {
        paint_stack(painter, icons, rect, stack);
    }
}

/// Pile : icône du bloc ou de l'objet et nombre en bas à droite
fn paint_stack(painter: &egui::Painter, icons: &ItemIcons, rect: egui::Rect, stack: ItemStack) {
    icons.paint(painter, rect, stack.item);
    if stack.count > 1 {
        let pos = rect.right_bottom() - egui::vec2(3.0_f32, 1.0_f32);
        let font = egui::FontId::proportional(12.0_f32);
        let text = stack.count.to_string();
        painter.text(pos + egui::vec2(1.0_f32, 1.0_f32), egui::Align2::RIGHT_BOTTOM, &text, font.clone(), egui::Color32::BLACK);
        painter.text(pos, egui::Align2::RIGHT_BOTTOM, text, font, egui::Color32::WHITE);
    }
}

/// Fiche d'un bloc ou d'un objet survolé : nom (et nombre), rubrique, description, caractéristiques
fn item_tooltip(ui: &mut egui::Ui, item: ItemId, count: Option<u16>) {
    ui.set_min_width(180.0_f32);
    let title = match count {
        Some(count) => format!("{} × {}", item.name(), count),
        None => item.name().to_string(),
    };
    ui.label(egui::RichText::new(title).strong().color(GOLD_BRIGHT));
    let (category, description, stats) = match item.block() {
        Some(block) => match blocks().get(block) {
            Some(sheet) => (sheet.category.as_str(), sheet.description.as_str(), None),
            None => ("", "", None),
        },
        None => match items().get(item) {
            Some(sheet) => (sheet.category.as_str(), sheet.description.as_str(), Some(&sheet.stats)),
            None => ("", "", None),
        },
    };
    if !category.is_empty() {
        ui.label(egui::RichText::new(category).size(11.0_f32).color(TEXT_DIM));
    }
    if !description.is_empty() {
        ui.label(description);
    }
    for (name, value) in stats.into_iter().flatten() {
        ui.label(egui::RichText::new(format!("{name} : {value}")).size(12.0_f32).color(TEXT_SOFT));
    }
}

/// Une rubrique du catalogue
struct CatalogCategory {
    label: String,
    items: Vec<ItemId>,
}

/// Catalogue de l'écran d'inventaire : les blocs qui se ramassent et tous les objets, rangés par
/// rubrique (celle de leur fiche), avec une recherche par nom
pub struct Catalog {
    categories: Vec<CatalogCategory>,
    /// Rubrique montrée (`None` : toutes)
    pub category: Option<usize>,
    pub search: String,
}

/// Cases du catalogue par ligne
const CATALOG_COLUMNS: usize = 8;
const CATALOG_SLOT_SIZE: f32 = 36.0;

impl Default for Catalog {
    fn default() -> Self {
        let mut categories: Vec<CatalogCategory> = Vec::new();
        let mut add = |prefix: &str, category: &str, item: ItemId| {
            let label = format!("{prefix} · {}", if category.is_empty() { "Divers" } else { category });
            match categories.iter_mut().find(|c| c.label == label) {
                Some(c) => c.items.push(item),
                None => categories.push(CatalogCategory { label, items: vec![item] }),
            }
        };
        for sheet in blocks().iter() {
            let item = ItemId::from(herocraft_common::BlockId(sheet.numeric_id));
            if can_hold(item) {
                add("Blocs", &sheet.category, item);
            }
        }
        for sheet in items().iter() {
            add("Objets", &sheet.category, sheet.item());
        }
        for category in &mut categories {
            category.items.sort_by_key(|item| item.name());
        }
        Self { categories, category: None, search: String::new() }
    }
}

impl Catalog {
    /// Ce que montre le catalogue : la rubrique choisie, filtrée par la recherche
    pub fn visible(&self) -> Vec<ItemId> {
        let search = self.search.trim().to_lowercase();
        self.categories
            .iter()
            .enumerate()
            .filter(|(i, _)| self.category.is_none_or(|c| c == *i))
            .flat_map(|(_, c)| c.items.iter().copied())
            .filter(|item| search.is_empty() || item.name().to_lowercase().contains(&search))
            .collect()
    }

    fn draw(&mut self, ui: &mut egui::Ui, icons: &ItemIcons, taken: &mut Option<(ItemId, bool)>, hovered: &mut Option<(ItemId, Option<u16>)>) {
        let width = CATALOG_COLUMNS as f32 * (CATALOG_SLOT_SIZE + 4.0_f32) - 4.0_f32;
        ui.horizontal(|ui| {
            let label = self.category.map_or("Tout", |i| self.categories[i].label.as_str()).to_string();
            egui::ComboBox::from_id_salt("catalog_category").width(150.0_f32).selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.category, None, "Tout");
                for (i, category) in self.categories.iter().enumerate() {
                    ui.selectable_value(&mut self.category, Some(i), category.label.as_str());
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Rechercher…").desired_width(width - 160.0_f32));
        });
        ui.add_space(2.0_f32);
        let visible = self.visible();
        // Hauteur fixe : l'écran ne change pas de taille au fil de la recherche
        let height = 4.0_f32 * (INVENTORY_SLOT_SIZE + 4.0_f32) + 28.0_f32;
        egui::ScrollArea::vertical().id_salt("catalog").max_height(height).show(ui, |ui| {
            ui.set_width(width);
            ui.set_min_height(height);
            ui.spacing_mut().item_spacing = egui::vec2(4.0_f32, 4.0_f32);
            for row in visible.chunks(CATALOG_COLUMNS) {
                ui.horizontal(|ui| {
                    for &item in row {
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(CATALOG_SLOT_SIZE, CATALOG_SLOT_SIZE), egui::Sense::click());
                        paint_slot(ui.painter(), icons, rect, Some(ItemStack { item, count: 1 }), false, response.hovered());
                        if response.hovered() {
                            *hovered = Some((item, None));
                        }
                        if response.clicked() {
                            *taken = Some((item, false));
                        } else if response.secondary_clicked() {
                            *taken = Some((item, true));
                        }
                    }
                });
            }
            if visible.is_empty() {
                ui.label(egui::RichText::new("Rien ne correspond.").color(TEXT_DIM));
            }
        });
    }
}

/// Jauge horizontale remplie à `fraction`, avec son texte centré
fn gauge(ui: &mut egui::Ui, width: f32, height: f32, fraction: f32, color: egui::Color32, text: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let rounding = egui::Rounding::ZERO;
    painter.rect_filled(rect, rounding, egui::Color32::from_rgba_premultiplied(23, 13, 8, 210));
    let fraction = if fraction.is_finite() { fraction.clamp(0.0_f32, 1.0_f32) } else { 0.0_f32 };
    if fraction > 0.0_f32 {
        let filled = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fraction, rect.height()));
        painter.rect_filled(filled, rounding, color);
    }
    // Liseré de bois clair puis bord sombre, comme les cadres de l'ancien HUD
    painter.rect_stroke(rect, rounding, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(92, 65, 45)));
    painter.rect_stroke(rect.expand(1.0_f32), rounding, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(23, 13, 8)));
    if let Some(text) = text {
        let font = egui::FontId::proportional(11.5_f32);
        painter.text(rect.center() + egui::vec2(1.0_f32, 1.0_f32), egui::Align2::CENTER_CENTER, text, font.clone(), egui::Color32::from_black_alpha(200));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, font, egui::Color32::WHITE);
    }
}

/// Bords de l'écran rougis (opacité `alpha` au bord, nulle vers le centre)
fn red_vignette(screen: egui::Rect, alpha: u8) -> egui::Shape {
    let edge = egui::Color32::from_rgba_unmultiplied(170, 0, 0, alpha);
    let clear = egui::Color32::from_rgba_unmultiplied(170, 0, 0, 0);
    let inner = screen.shrink2(screen.size() * 0.3_f32);
    let mut mesh = egui::Mesh::default();
    for (pos, color) in [
        (screen.left_top(), edge),
        (screen.right_top(), edge),
        (screen.right_bottom(), edge),
        (screen.left_bottom(), edge),
        (inner.left_top(), clear),
        (inner.right_top(), clear),
        (inner.right_bottom(), clear),
        (inner.left_bottom(), clear),
    ] {
        mesh.colored_vertex(pos, color);
    }
    // Un trapèze par bord, entre le cadre de l'écran et le rectangle intérieur transparent
    for k in 0..4_u32 {
        let next = (k + 1) % 4;
        mesh.add_triangle(k, next, 4 + next);
        mesh.add_triangle(k, 4 + next, 4 + k);
    }
    egui::Shape::mesh(mesh)
}

/// Cadre commun des panneaux du HUD : bois sombre à liseré, comme les bulles de l'ancien jeu
fn hud_frame(margin: egui::Margin) -> crate::ui_skin::SkinFrame {
    crate::ui_skin::SkinFrame::area(margin)
}

/// Petit intitulé de section en capitales
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(10.5_f32).strong().color(TEXT_DIM));
}

/// Filet de fer à pierre bleue, sous les titres du menu
fn ornament(ui: &mut egui::Ui, width: f32) {
    crate::ui_skin::ornament(ui, width);
}

/// Grande entrée du menu d'accueil, avec une ligne d'explication au survol
fn menu_entry(ui: &mut egui::Ui, size: egui::Vec2, text: &str, hint: Option<&str>) -> egui::Response {
    let button = SkinButton::new(egui::RichText::new(text).size(16.0_f32)).kind(ButtonKind::Menu).min_size(size);
    let response = ui.add(button);
    match hint {
        Some(hint) => response.on_hover_text(hint),
        None => response,
    }
}

/// Touche dessinée comme un capuchon de clavier
fn keycap(ui: &mut egui::Ui, key: &str) {
    crate::ui_skin::SkinFrame::new(crate::ui_skin::BOX, 0.5_f32, egui::Margin::symmetric(9.0_f32, 5.0_f32)).show(ui, |ui| {
        ui.label(egui::RichText::new(key).monospace().size(11.5_f32).color(crate::ui_skin::TEXT));
    });
}

/// Grille à deux colonnes (libellé, réglage) commune aux onglets d'options
fn settings_grid(ui: &mut egui::Ui, id: &str, add_rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([16.0_f32, 10.0_f32])
        .min_col_width(110.0_f32)
        .show(ui, add_rows);
}

fn row_label(ui: &mut egui::Ui, text: &str, hint: &str) {
    ui.label(egui::RichText::new(text).color(TEXT_SOFT)).on_hover_text(hint);
}

fn slider_row(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, hint: &str) -> egui::Response {
    row_label(ui, label, hint);
    let response = ui.add(egui::Slider::new(value, range).fixed_decimals(2)).on_hover_text(hint);
    ui.end_row();
    response
}

fn slider_row_enabled(ui: &mut egui::Ui, enabled: bool, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, hint: &str) {
    ui.add_enabled_ui(enabled, |ui| row_label(ui, label, hint));
    ui.add_enabled(enabled, egui::Slider::new(value, range).fixed_decimals(2)).on_hover_text(hint);
    ui.end_row();
}

/// Curseur de volume en pour cent. Vrai quand un réglage est terminé (curseur relâché ou clic) :
/// c'est le moment de l'enregistrer et de faire entendre le nouveau volume.
fn volume_row(ui: &mut egui::Ui, enabled: bool, label: &str, value: &mut f32, hint: &str) -> bool {
    ui.add_enabled_ui(enabled, |ui| row_label(ui, label, hint));
    let slider = egui::Slider::new(value, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).custom_parser(|text| {
        text.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)
    });
    let response = ui.add_enabled(enabled, slider).on_hover_text(hint);
    ui.end_row();
    response.drag_stopped() || (response.changed() && !response.dragged())
}

/// Choix exclusif présenté comme un sélecteur segmenté ; renvoie vrai si la valeur a changé
fn choice_row<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, choices: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0_f32;
        for (choice, label) in choices {
            if ui.add(SkinButton::new(*label).kind(ButtonKind::Secondary).selected(*value == *choice).small()).clicked() && *value != *choice {
                *value = *choice;
                changed = true;
            }
        }
    });
    changed
}

/// Case à cocher de l'ancien thème (coche dorée sur bois sombre)
fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(24.0_f32, 24.0_f32);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        crate::ui_skin::paint_checkbox(ui, rect, *on, response.hovered());
    }
    response
}

/// Bouton discret de remise à zéro en bas d'un onglet ; renvoie vrai au clic
fn reset_button(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add_space(8.0_f32);
    ui.add(SkinButton::new(format!("↺  {}", text)).kind(ButtonKind::Secondary).small()).clicked()
}

/// Bouton en forme de capuchon de clavier, doré quand il attend une touche
fn keycap_button(ui: &mut egui::Ui, text: &str, listening: bool) -> egui::Response {
    ui.add(
        SkinButton::new(egui::RichText::new(text).monospace().size(11.5_f32))
            .kind(ButtonKind::Secondary)
            .selected(listening)
            .small()
            .min_size(egui::vec2(96.0_f32, 0.0_f32)),
    )
}

/// Intitulé de section sur toute une ligne de la grille des commandes
fn section_row(ui: &mut egui::Ui, title: &str) {
    ui.vertical(|ui| {
        ui.add_space(6.0_f32);
        caption(ui, title);
    });
    ui.end_row();
}

/// Ligne fixe de la page Commandes : touches non modifiables puis description
fn fixed_control_row(ui: &mut egui::Ui, keys: &[&str], description: &str) {
    ui.label(egui::RichText::new(description).color(TEXT_SOFT));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0_f32;
        for key in keys {
            keycap(ui, key);
        }
    });
    ui.end_row();
}

impl UiState {
    /// Page Commandes : touches modifiables par clic, réglages de la souris et raccourcis fixes
    fn draw_controls_page(&mut self, ui: &mut egui::Ui, body_height: f32) {
        egui::ScrollArea::vertical()
            .id_salt("controls_scroll")
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Cliquez sur une touche puis appuyez sur la nouvelle (Échap pour annuler).")
                        .size(11.5_f32)
                        .color(TEXT_DIM),
                );
                if let Some(notice) = &self.rebind_notice {
                    ui.label(egui::RichText::new(notice).size(11.5_f32).color(GOLD));
                }

                // Une seule grille pour toutes les sections : la colonne des touches reste alignée
                let zoom_key = self.controls.action_key_label(Action::ZoomModifier);
                let mut save = false;
                egui::Grid::new("controls_grid")
                    .num_columns(2)
                    .spacing([24.0_f32, 6.0_f32])
                    .min_col_width(130.0_f32)
                    .show(ui, |ui| {
                        for (title, actions) in ACTION_GROUPS {
                            section_row(ui, title);
                            for &action in actions {
                                ui.label(egui::RichText::new(action.label()).color(TEXT_SOFT));
                                let listening = self.rebinding == Some(action);
                                let text = if listening { "…".to_string() } else { self.controls.action_key_label(action) };
                                let response = keycap_button(ui, &text, listening)
                                    .on_hover_text(if listening { "Appuyez sur la nouvelle touche" } else { "Changer la touche" });
                                if response.clicked() {
                                    self.rebinding = if listening { None } else { Some(action) };
                                    self.rebind_notice = None;
                                }
                                ui.end_row();
                            }
                        }

                        section_row(ui, "BARRE D'OBJETS");
                        fixed_control_row(ui, &["1", "…", "9"], "Choisir une case");
                        fixed_control_row(ui, &["Molette"], "Case précédente / suivante");

                        section_row(ui, "SOURIS");
                        fixed_control_row(ui, &["Souris"], "Regarder autour (au sol)");
                        fixed_control_row(ui, &["Clic gauche"], "Casser le bloc visé (il tombe, à ramasser)");
                        fixed_control_row(ui, &["Clic droit"], "Poser le bloc de la case choisie");
                        fixed_control_row(ui, &[zoom_key.as_str(), "Molette"], "Zoom, du sol à l'orbite");
                        fixed_control_row(ui, &["Clic", "glisser"], "Tourner la planète (vue satellite)");

                        row_label(ui, "Sensibilité", "Vitesse de rotation de la vue à la souris");
                        let sensitivity = ui.add(egui::Slider::new(&mut self.controls.mouse_sensitivity, 0.1..=5.0).fixed_decimals(2));
                        ui.end_row();

                        row_label(ui, "Inverser l'axe vertical", "Pousser la souris vers l'avant baisse le regard");
                        let invert = toggle_switch(ui, &mut self.controls.invert_y);
                        ui.end_row();

                        // Le curseur n'enregistre qu'au relâchement, pas à chaque image du glisser
                        save = sensitivity.drag_stopped() || (sensitivity.changed() && !sensitivity.dragged()) || invert.changed();

                        section_row(ui, "SYSTÈME");
                        fixed_control_row(ui, &["Échap"], "Menu de pause / revenir en arrière");
                        fixed_control_row(ui, &["F9"], "Écrire le rapport de profilage");
                    });
                if save {
                    self.controls.save();
                }

                if reset_button(ui, "Touches et souris par défaut") {
                    self.controls.reset_bindings();
                    self.controls.mouse_sensitivity = 1.0;
                    self.controls.invert_y = false;
                    self.rebinding = None;
                    self.rebind_notice = None;
                    self.controls.save();
                }
            });
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
    fn test_graphics_quality_presets() {
        let mut ui = UiState::new(1);
        // Par défaut : tous les effets, c'est la qualité haute
        assert_eq!(ui.graphics_quality(), Some(GraphicsQuality::Haute));
        assert!(ui.apply_graphics_quality(GraphicsQuality::Basse), "la distance de détail passe à courte");
        assert_eq!(ui.graphics_quality(), Some(GraphicsQuality::Basse));
        assert!(!ui.shadow_settings.enabled && !ui.atmosphere_settings.god_rays && !ui.post_settings.bloom_enabled);
        assert!(ui.apply_graphics_quality(GraphicsQuality::Moyenne));
        assert!(ui.shadow_settings.enabled && !ui.shadow_settings.cloud_shadows && !ui.atmosphere_settings.god_rays);
        // Un effet réglé à la main : réglages personnalisés
        ui.atmosphere_settings.god_rays = true;
        assert_eq!(ui.graphics_quality(), None);
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
        let (output_orbit, action_orbit) = ui.update_ui(false);
        assert!(action_orbit.is_none());
        assert!(!output_orbit.shapes.is_empty());

        // Vue sol avec menu paramètres ouvert
        ui.set_settings_open(true);
        let (output_ground, action_ground) = ui.update_ui(true);
        assert!(action_ground.is_none());
        assert!(!output_ground.shapes.is_empty());
    }

    #[test]
    fn test_escape_navigates_menu_levels() {
        let mut ui = UiState::new(7);

        // Échap ouvre le menu sur l'accueil
        assert!(ui.handle_escape());
        assert_eq!(ui.menu_page, MenuPage::Main);

        // Depuis un sous-menu, Échap revient à l'accueil sans fermer
        ui.menu_page = MenuPage::Options;
        assert!(ui.handle_escape());
        assert_eq!(ui.menu_page, MenuPage::Main);

        // Depuis l'accueil, Échap referme
        assert!(!ui.handle_escape());

        // Le menu se rouvre toujours sur l'accueil
        ui.open_menu_at(MenuPage::Controls);
        ui.set_settings_open(false);
        assert!(ui.handle_escape());
        assert_eq!(ui.menu_page, MenuPage::Main);
    }

    #[test]
    fn test_rebinding_from_controls_page() {
        let mut ui = UiState::new(1);
        ui.open_menu_at(MenuPage::Controls);

        // Échap annule la saisie sans quitter la page
        ui.rebinding = Some(Action::Sprint);
        assert!(ui.handle_escape());
        assert_eq!(ui.rebinding, None);
        assert_eq!(ui.menu_page, MenuPage::Controls);

        // Touche déjà prise : échange et message
        ui.rebinding = Some(Action::Sprint);
        ui.finish_rebind(KeyCode::KeyW);
        assert_eq!(ui.rebinding, None);
        assert_eq!(ui.controls.key(Action::Sprint), KeyCode::KeyW);
        assert_eq!(ui.controls.key(Action::MoveForward), KeyCode::ControlLeft);
        assert!(ui.rebind_notice.is_some());

        // Touche réservée ignorée : la saisie continue
        ui.rebinding = Some(Action::Jump);
        ui.finish_rebind(KeyCode::F9);
        assert_eq!(ui.rebinding, Some(Action::Jump));

        // Fermer le menu abandonne la saisie
        ui.set_settings_open(false);
        assert_eq!(ui.rebinding, None);
    }

    #[test]
    fn test_captured_pointer_is_hidden_from_egui() {
        let mut ui = UiState::new(1);
        ui.set_pointer_captured(true);
        ui.raw_input.events.clear();
        let window_event = WindowEvent::MouseWheel {
            device_id: unsafe { winit::event::DeviceId::dummy() },
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: winit::event::TouchPhase::Moved,
        };
        ui.handle_event(&window_event);
        assert!(ui.raw_input.events.is_empty());

        ui.set_pointer_captured(false);
        ui.handle_event(&window_event);
        assert_eq!(ui.raw_input.events.len(), 1);
    }

    /// Chaque page et chaque onglet du menu tient dans l'écran, y compris sur de petites fenêtres
    #[test]
    fn test_pause_menu_fits_screen() {
        let pages = [
            (MenuPage::Main, None),
            (MenuPage::Controls, None),
            (MenuPage::Multiplayer, None),
        ]
        .into_iter()
        .chain(OptionsTab::ALL.into_iter().map(|t| (MenuPage::Options, Some(t))));

        for (page, tab) in pages {
            for (w, h, scale) in [(1920.0, 1080.0, 1.0), (1280.0, 720.0, 1.0), (800.0, 450.0, 1.0), (1280.0, 720.0, 2.0), (480.0, 320.0, 1.0)] {
                let mut ui = UiState::new(1);
                ui.set_screen_size(w, h);
                ui.set_scale_factor(scale);
                ui.open_menu_at(page);
                if let Some(tab) = tab {
                    ui.options_tab = tab;
                }
                // Plusieurs frames : egui mesure les zones à la première, et la taille doit rester stable
                for _ in 0..6 {
                    ui.update_ui(true);
                }

                let rect = ui
                    .ctx
                    .memory(|m| m.area_rect(egui::Id::new(("pause_menu", page))))
                    .expect("menu de pause dessiné");
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w / scale, h / scale));
                assert!(
                    screen.expand(0.5).contains_rect(rect),
                    "{:?}/{:?} dépasse l'écran {}x{}@{} : {:?}",
                    page,
                    tab,
                    w,
                    h,
                    scale,
                    rect
                );
            }
        }
    }

    /// Échap referme d'abord l'inventaire, sans ouvrir le menu ; ouvrir le menu referme l'inventaire
    #[test]
    fn test_escape_and_menu_close_inventory() {
        let mut ui = UiState::new(1);
        ui.inventory.add(herocraft_common::BlockId::STONE);
        ui.set_inventory_open(true);
        ui.inventory.click(0);
        assert!(ui.inventory.held().is_some());
        assert!(!ui.handle_escape(), "Échap ferme l'inventaire sans ouvrir le menu");
        assert!(!ui.inventory_open);
        assert_eq!(ui.inventory.held(), None, "la pile tenue retourne dans l'inventaire");
        assert_eq!(ui.inventory.count(herocraft_common::BlockId::STONE), 1);

        ui.set_inventory_open(true);
        ui.toggle_settings();
        assert!(ui.is_settings_open && !ui.inventory_open);
    }

    /// Barre d'objets et écran d'inventaire tiennent dans la fenêtre
    #[test]
    fn test_hotbar_and_inventory_fit_screen() {
        let mut ui = UiState::new(1);
        ui.set_screen_size(1280.0, 720.0);
        for _ in 0..3 {
            ui.inventory.add(herocraft_common::BlockId::DIRT);
        }
        for _ in 0..3 {
            ui.update_ui(true);
        }
        let hotbar = ui.ctx.memory(|m| m.area_rect(egui::Id::new("hotbar"))).expect("barre d'objets dessinée");
        assert!(hotbar.bottom() <= 720.0 && hotbar.left() >= 0.0 && hotbar.right() <= 1280.0, "{:?}", hotbar);
        // Ni les jauges (à droite) ni la classe (à gauche) ne chevauchent la barre d'objets
        for id in ["vitals_hud", "class_hud"] {
            let other = ui.ctx.memory(|m| m.area_rect(egui::Id::new(id))).expect("HUD du joueur dessiné");
            assert!(!other.intersects(hotbar), "{id} chevauche la barre d'objets : {:?} / {:?}", other, hotbar);
        }
        assert!(hotbar.top() > 360.0, "{:?}", hotbar);

        ui.set_inventory_open(true);
        for _ in 0..3 {
            ui.update_ui(true);
        }
        let screen = ui.ctx.memory(|m| m.area_rect(egui::Id::new("inventory_screen"))).expect("inventaire dessiné");
        assert!(screen.top() >= 0.0 && screen.bottom() <= 720.0 && screen.left() >= 0.0 && screen.right() <= 1280.0, "{:?}", screen);
    }

    /// Les jauges du joueur s'affichent au sol, en bas de l'écran, et pas en vue satellite
    #[test]
    fn test_vitals_hud_shown_on_ground_only() {
        let mut ui = UiState::new(1);
        ui.set_screen_size(1280.0, 720.0);
        ui.player.stats.hp = 0.5 * ui.player.stats.max_hp;
        for _ in 0..3 {
            ui.update_ui(true);
        }
        let rect = ui.ctx.memory(|m| m.area_rect(egui::Id::new("vitals_hud"))).expect("jauges dessinées");
        assert!(rect.bottom() <= 720.0 && rect.top() > 360.0, "{:?}", rect);
        assert!(rect.width() < 640.0, "{:?}", rect);
        assert!(rect.left() > 640.0, "jauges à droite : {:?}", rect);
        let class = ui.ctx.memory(|m| m.area_rect(egui::Id::new("class_hud"))).expect("classe dessinée");
        assert!(class.right() < 640.0 && class.bottom() <= 720.0, "classe à gauche : {:?}", class);
        // Plus de panneau en haut à gauche
        assert!(ui.ctx.memory(|m| m.area_rect(egui::Id::new("top_left_hud"))).is_none());

        let mut ui = UiState::new(1);
        for _ in 0..3 {
            ui.update_ui(false);
        }
        assert!(ui.ctx.memory(|m| m.area_rect(egui::Id::new("vitals_hud"))).is_none());
    }

    /// Les pictogrammes du menu existent dans les polices embarquées d'egui
    #[test]
    fn test_menu_icons_have_glyphs() {
        let mut ui = UiState::new(1);
        ui.update_ui(true);
        let font = egui::FontId::proportional(14.0);
        let icons = ["▶", "⚙", "⌨", "🚪", "⬅", "↺", "🎨", "🎲", "🌍", "🚶", "⏸", "🔄", "✖", "›", "—"]
            .into_iter()
            .chain(OptionsTab::ALL.iter().map(|t| t.icon()));
        for icon in icons {
            assert!(ui.ctx.fonts(|f| f.has_glyphs(&font, icon)), "glyphe manquant : {}", icon);
        }
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
        assert_eq!(settings.cloud_density, 1.0);

        settings.moon_intensity = 2.5;
        settings.moon_speed = 0.0; // Fige la lune
        settings.cloud_density = 1.8;
        assert_eq!(settings.moon_intensity, 2.5);
        assert_eq!(settings.moon_speed, 0.0);
        assert_eq!(settings.cloud_density, 1.8);

        // Réinitialisation
        settings = LightingSettings::default();
        assert_eq!(settings.moon_intensity, 0.25);
        assert_eq!(settings.night_ambient, 2.10);
        assert_eq!(settings.moon_speed, 1.0);
        assert_eq!(settings.cloud_density, 1.0);
    }

    #[test]
    fn test_cloud_settings_defaults_and_mutation() {
        let mut clouds = CloudSettings::default();
        assert_eq!(clouds.density, 1.0);
        assert_eq!(clouds.roundness, 0.45);
        assert_eq!(clouds.shadow_intensity, 1.0);
        assert_eq!(clouds.sunset_glow, 1.0);
        assert_eq!(clouds.micro_noise, 0.0);

        clouds.density = 1.5;
        clouds.roundness = 0.28;
        clouds.shadow_intensity = 1.8;
        clouds.sunset_glow = 0.8;
        clouds.micro_noise = 0.4;
        assert_eq!(clouds.density, 1.5);
        assert_eq!(clouds.roundness, 0.28);
        assert_eq!(clouds.shadow_intensity, 1.8);
        assert_eq!(clouds.sunset_glow, 0.8);
        assert_eq!(clouds.micro_noise, 0.4);

        clouds = CloudSettings::default();
        assert_eq!(clouds.density, 1.0);
        assert_eq!(clouds.roundness, 0.45);
    }

    /// Captures des menus en PNG, rendues sur le processeur :
    /// `HEROCRAFT_CAPTURES=dossier [HEROCRAFT_CAPTURES_FOND=image.png] cargo test -p herocraft_client snapshot_menus -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_menus() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let background = std::env::var("HEROCRAFT_CAPTURES_FOND").ok().map(std::path::PathBuf::from);
        // Taille de la fenêtre, « 1280x720 » par défaut
        let (width, height) = std::env::var("HEROCRAFT_CAPTURES_TAILLE")
            .ok()
            .and_then(|t| t.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
            .unwrap_or((1280_usize, 720_usize));

        let scenes: [(&str, fn(&mut UiState), bool, Option<egui::Pos2>); 7] = [
            ("menu", |ui| ui.open_menu_at(MenuPage::Main), true, None),
            ("menu_survol", |ui| ui.open_menu_at(MenuPage::Main), true, Some(egui::pos2(640.0, 372.0))),
            ("options", |ui| ui.open_menu_at(MenuPage::Options), true, None),
            ("options_ombres", |ui| {
                ui.open_menu_at(MenuPage::Options);
                ui.options_tab = OptionsTab::Shadows;
            }, true, None),
            ("commandes", |ui| ui.open_menu_at(MenuPage::Controls), true, None),
            ("multijoueur", |ui| ui.open_menu_at(MenuPage::Multiplayer), true, None),
            ("inventaire", |ui| ui.set_inventory_open(true), true, None),
        ];
        let mut all: Vec<(&str, fn(&mut UiState), bool, Option<egui::Pos2>)> = scenes.to_vec();
        all.push(("hud", |_| {}, true, None));

        for (name, setup, ground, pointer) in all {
            let mut ui = UiState::new(424242);
            ui.set_screen_size(width as f32, height as f32);
            setup(&mut ui);
            let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
            snapshot.set_background(background.as_deref());
            let mut last = None;
            // Plusieurs images : mesure des pieds de page, fondus et survols
            for _ in 0..12 {
                if let Some(pos) = pointer {
                    ui.raw_input.events.push(egui::Event::PointerMoved(pos));
                }
                ui.raw_input.time = Some(ui.raw_input.time.unwrap_or(0.0) + 0.5);
                ui.raw_input.predicted_dt = 0.5;
                let (output, _) = ui.update_ui(ground);
                snapshot.apply_textures(&output.textures_delta);
                last = Some(output);
            }
            let output = last.unwrap();
            let primitives = ui.ctx.tessellate(output.shapes, output.pixels_per_point);
            snapshot.paint(&primitives, output.pixels_per_point);
            snapshot.save(&dir.join(format!("{name}.png")));
        }
    }

}

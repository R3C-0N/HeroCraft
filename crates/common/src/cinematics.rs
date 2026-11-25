//! Cinématiques en jeu : séquences de plans de caméra, textes, fondus et événements, décrites par
//! les fichiers de `assets/cinematics/` (voir le README du dossier).
//!
//! Ce module ne dessine rien : il lit les fichiers, dit ce que montre une cinématique à chaque
//! instant (`Cinematic::sample`) et tient la lecture (`CinematicPlayer`) : quelle cinématique se
//! joue, laquelle se déclenche sur un événement du jeu (`GameEvent`), quels événements elle renvoie
//! au jeu en avançant. Le client place la caméra et peint l'écran à partir de ce qu'il en reçoit.
//!
//! Les positions d'une cinématique sont en blocs, dans le repère de son **ancre** : `x` vers la
//! droite, `y` vers le haut, `z` vers l'avant. L'ancre est le joueur (ses pieds et son regard au
//! moment où la cinématique commence) ou la position de l'événement qui l'a déclenchée.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Dossier des cinématiques
pub const CINEMATICS_DIR: &str = "assets/cinematics";

/// Événement envoyé par le jeu au lecteur (« Pierre obtenue », « temple atteint »…), et renvoyé
/// par une cinématique au jeu (`cues`, `on_end`). Un nom, des étiquettes, une position facultative.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    /// Point du monde où l'événement a lieu (une cinématique ancrée sur `event` s'y place)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Vec3>,
}

impl GameEvent {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }

    /// Ajoute une étiquette (`stone` = `voidStone`…)
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Situe l'événement dans le monde
    pub fn at(mut self, position: Vec3) -> Self {
        self.position = Some(position);
        self
    }

    /// Lit `nom clé=valeur clé=valeur` (commande `/event`)
    pub fn parse(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let mut event = Self::new(words.next()?);
        for word in words {
            let (key, value) = word.split_once('=')?;
            if key.is_empty() {
                return None;
            }
            event = event.tag(key, value);
        }
        Some(event)
    }
}

/// Une valeur constante, ou une paire `[début, fin]` parcourue pendant le plan
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Track<T> {
    Span([T; 2]),
    Fixed(T),
}

impl<T: Copy> Track<T> {
    pub fn start(&self) -> T {
        match self {
            Track::Fixed(v) => *v,
            Track::Span([a, _]) => *a,
        }
    }

    pub fn end(&self) -> T {
        match self {
            Track::Fixed(v) => *v,
            Track::Span([_, b]) => *b,
        }
    }
}

impl Track<f32> {
    pub fn at(&self, k: f32) -> f32 {
        let (a, b) = (self.start(), self.end());
        a + (b - a) * k
    }
}

impl Track<[f32; 3]> {
    pub fn at(&self, k: f32) -> Vec3 {
        Vec3::from(self.start()).lerp(Vec3::from(self.end()), k)
    }
}

/// Courbe d'avancement d'un plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    Linear,
    /// Démarre doucement
    In,
    /// S'arrête doucement
    Out,
    /// Démarre et s'arrête doucement (par défaut)
    #[default]
    InOut,
}

impl Ease {
    pub fn apply(self, k: f32) -> f32 {
        let k = k.clamp(0.0, 1.0);
        match self {
            Ease::Linear => k,
            Ease::In => k * k * k,
            Ease::Out => 1.0 - (1.0 - k).powi(3),
            Ease::InOut => k * k * (3.0 - 2.0 * k),
        }
    }
}

/// Caméra qui tourne autour de son point de visée
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orbit {
    /// Distance au point visé, en blocs (dans le plan du sol)
    pub radius: Track<f32>,
    /// Hauteur au-dessus du point visé, en blocs
    #[serde(default = "zero_track")]
    pub height: Track<f32>,
    /// Angle en degrés, compté depuis l'arrière de l'ancre (0 : derrière, 90 : à droite, 180 :
    /// face à elle)
    pub angle: Track<f32>,
}

fn zero_track() -> Track<f32> {
    Track::Fixed(0.0)
}

fn default_look_at() -> Track<[f32; 3]> {
    Track::Fixed([0.0, 1.6, 0.0])
}

fn default_fov() -> Track<f32> {
    Track::Fixed(50.0)
}

/// Un plan : la caméra va d'une position à une autre pendant `duration` secondes
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    /// Nom du plan, pour s'y retrouver dans le fichier
    #[serde(default)]
    pub name: String,
    pub duration: f32,
    /// Position de la caméra (`[x, y, z]`, ou `[[début], [fin]]` pour un travelling)
    #[serde(default)]
    pub position: Option<Track<[f32; 3]>>,
    /// Ou bien une caméra qui tourne autour du point visé
    #[serde(default)]
    pub orbit: Option<Orbit>,
    /// Point visé (par défaut, la tête de l'ancre)
    #[serde(default = "default_look_at")]
    pub look_at: Track<[f32; 3]>,
    /// Champ de vision vertical, en degrés
    #[serde(default = "default_fov")]
    pub fov: Track<f32>,
    /// Inclinaison de la caméra autour de son axe, en degrés
    #[serde(default = "zero_track")]
    pub roll: Track<f32>,
    /// Tremblement de la caméra, en blocs
    #[serde(default)]
    pub shake: f32,
    #[serde(default)]
    pub ease: Ease,
}

/// Façon d'afficher un texte
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionStyle {
    /// Sous-titre en bas de l'écran, au-dessus du bandeau noir (par défaut)
    #[default]
    Subtitle,
    /// Voix du conteur, en italique au milieu de l'écran
    Narration,
    /// Grand titre au centre (police des titres)
    Title,
    /// Lieu ou date, discret en haut à gauche
    Place,
}

/// Un texte affiché pendant la cinématique
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caption {
    /// Début, en secondes depuis le début de la cinématique
    pub at: f32,
    pub duration: f32,
    pub text: String,
    /// Qui parle (sous-titre)
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub style: CaptionStyle,
    /// Secondes d'apparition et de disparition en fondu
    #[serde(default = "default_caption_fade")]
    pub fade: f32,
}

fn default_caption_fade() -> f32 {
    0.5
}

/// Un fondu de l'écran vers une couleur (ou depuis elle)
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fade {
    pub at: f32,
    pub duration: f32,
    /// Opacité du voile au début et à la fin (0 : image nette, 1 : couleur pleine)
    pub from: f32,
    pub to: f32,
    /// `#RRGGBB` (noir par défaut)
    #[serde(default = "default_fade_color")]
    pub color: String,
}

fn default_fade_color() -> String {
    "#000000".into()
}

/// Un événement renvoyé au jeu à un instant de la cinématique (changer d'époque, jouer un son…)
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    pub at: f32,
    pub emit: String,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
}

/// Ce qui déclenche une cinématique : un événement du jeu dont les étiquettes valent `when`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub event: String,
    #[serde(default)]
    pub when: BTreeMap<String, String>,
}

impl Trigger {
    pub fn matches(&self, event: &GameEvent) -> bool {
        self.event == event.name && self.when.iter().all(|(k, v)| event.tags.get(k) == Some(v))
    }
}

/// Repère où se placent les positions de la cinématique
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Les pieds et le regard du joueur au début (par défaut)
    #[default]
    Player,
    /// La position de l'événement déclencheur, tournée comme le regard du joueur (le joueur à
    /// défaut de position)
    Event,
    /// La scène de papier : loin au-dessus du joueur, hors du monde, devant le fond de `stage`
    Stage,
}

/// Lumière de la scène de papier, dans le repère de l'ancre
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageLight {
    /// Direction d'où vient la lumière (`[x, y, z]`, vers la source)
    #[serde(default = "default_light_direction")]
    pub direction: [f32; 3],
    /// `#RRGGBB`
    #[serde(default = "default_light_color")]
    pub color: String,
    /// Force de la lumière directe
    #[serde(default = "one")]
    pub intensity: f32,
    /// Lumière ambiante, de 0 (ombres noires) à 1 (aucune ombre)
    #[serde(default = "default_ambient")]
    pub ambient: f32,
}

fn default_light_direction() -> [f32; 3] {
    [-0.4, 0.8, -0.5]
}

fn default_light_color() -> String {
    "#FFF4E0".into()
}

fn default_ambient() -> f32 {
    0.45
}

fn one() -> f32 {
    1.0
}

impl Default for StageLight {
    fn default() -> Self {
        Self { direction: default_light_direction(), color: default_light_color(), intensity: 1.0, ambient: default_ambient() }
    }
}

/// Réglages des cartes de papier : la lumière qui les éclaire, et le fond de la scène
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Couleur du papier de fond (`#RRGGBB`), tendu derrière tout ce que voit la caméra ; sans
    /// fond, les cartes se posent dans le monde
    #[serde(default)]
    pub backdrop: Option<String>,
    #[serde(default)]
    pub light: StageLight,
    /// Bords de l'image assombris, comme sous une lampe (0 : aucun, 0,55 : celui des maquettes)
    #[serde(default)]
    pub vignette: f32,
}

/// Une carte de papier découpé, posée dans la scène 3D
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    /// Nom pour s'y retrouver
    #[serde(default)]
    pub name: String,
    /// Découpe de `assets/cinematics/cards/` (PNG à transparence). Sans image : une feuille unie.
    #[serde(default)]
    pub image: Option<String>,
    /// Début et durée, en secondes (par défaut, toute la cinématique)
    #[serde(default)]
    pub at: f32,
    #[serde(default)]
    pub duration: Option<f32>,
    /// Secondes d'apparition et de disparition
    #[serde(default)]
    pub fade: f32,
    /// Opacité (0 à 1), en plus du fondu : la feuille se dessine point par point
    #[serde(default = "one_track")]
    pub opacity: Track<f32>,
    /// Centre de la carte, en blocs dans le repère de l'ancre (une paire : elle se déplace)
    #[serde(default = "origin_track")]
    pub position: Track<[f32; 3]>,
    /// Largeur en blocs ; la hauteur suit les proportions de l'image, ou `height`
    #[serde(default = "one_track")]
    pub width: Track<f32>,
    /// Trajet image par image : des étapes, en secondes depuis `at`, entre lesquelles la carte
    /// glisse en ligne droite. Elles remplacent `position`, `width` et `opacity`.
    #[serde(default)]
    pub keys: Vec<CardKey>,
    #[serde(default)]
    pub height: Option<Track<f32>>,
    /// Rotation autour de la verticale, en degrés (0 : la carte regarde vers l'arrière de
    /// l'ancre, là où se tient d'ordinaire la caméra)
    #[serde(default = "zero_track")]
    pub yaw: Track<f32>,
    /// Bascule vers l'avant ou l'arrière, en degrés (90 : couchée à plat, face au ciel)
    #[serde(default = "zero_track")]
    pub pitch: Track<f32>,
    /// Rotation dans son plan, en degrés
    #[serde(default = "zero_track")]
    pub roll: Track<f32>,
    /// Balancement, en degrés, comme une pièce tenue par un fil
    #[serde(default)]
    pub sway: f32,
    /// Gondolement du papier, en part de la largeur (0,03 : à peine, 0,15 : très courbé)
    #[serde(default = "default_curl")]
    pub curl: f32,
    /// Teinte multipliée à l'image (`#RRGGBB`), ou couleur d'une feuille unie
    #[serde(default)]
    pub color: Option<String>,
    /// Lueur : au-dessus de 1, la carte brille (lave, Pierre, soleil) et ne prend pas d'ombre
    #[serde(default = "one")]
    pub glow: f32,
    /// Liseré blanc autour de la découpe, en part de la largeur (0 : aucun)
    #[serde(default = "default_border")]
    pub border: f32,
    /// Comment la carte se pose sur ce qui est derrière : découpée (par défaut), translucide ou
    /// lumineuse (elle s'ajoute)
    #[serde(default)]
    pub blend: CardBlend,
    #[serde(default)]
    pub ease: Ease,
}

/// Mélange d'une carte avec ce qui est derrière elle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardBlend {
    /// Une découpe nette et opaque, qui cache ce qui est derrière (apparition par tramage)
    #[default]
    Cut,
    /// L'opacité de l'image compte : ombres douces, voiles, papier translucide
    Alpha,
    /// La carte s'ajoute à ce qui est derrière : lueurs, halos
    Add,
}

fn one_track() -> Track<f32> {
    Track::Fixed(1.0)
}

fn origin_track() -> Track<[f32; 3]> {
    Track::Fixed([0.0; 3])
}

/// Une étape du trajet d'une carte (`keys`)
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct CardKey {
    /// Secondes depuis le début de la carte
    pub t: f32,
    pub position: [f32; 3],
    pub width: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    /// Teinte multipliée à la couleur de la carte (rouge, vert, bleu linéaires ; 1 : inchangée)
    #[serde(default = "white")]
    pub tint: [f32; 3],
    /// Rotation dans son plan, en degrés (en plus de `roll`)
    #[serde(default)]
    pub roll: f32,
    /// Hauteur en blocs (sans : les proportions de l'image)
    #[serde(default)]
    pub height: Option<f32>,
}

fn white() -> [f32; 3] {
    [1.0; 3]
}

impl Card {
    /// Position, largeur, opacité et teinte le long des étapes, `dt` secondes après le début
    fn key_at(&self, dt: f32) -> Option<(Vec3, f32, f32, Vec3, f32, Option<f32>)> {
        let keys = &self.keys;
        let last = keys.last()?;
        let i = keys.partition_point(|k| k.t <= dt);
        let (a, b) = match i {
            0 => (keys[0], keys[0]),
            i if i >= keys.len() => (*last, *last),
            i => (keys[i - 1], keys[i]),
        };
        let k = if b.t > a.t { ((dt - a.t) / (b.t - a.t)).clamp(0.0, 1.0) } else { 0.0 };
        let height = match (a.height, b.height) {
            (Some(ha), Some(hb)) => Some(ha + (hb - ha) * k),
            (h, None) | (None, h) => h,
        };
        Some((Vec3::from(a.position).lerp(Vec3::from(b.position), k), a.width + (b.width - a.width) * k, a.opacity + (b.opacity - a.opacity) * k, Vec3::from(a.tint).lerp(Vec3::from(b.tint), k), a.roll + (b.roll - a.roll) * k, height))
    }
}

fn default_curl() -> f32 {
    0.04
}

fn default_border() -> f32 {
    0.012
}

/// Une carte à un instant, dans le repère de l'ancre
#[derive(Debug, Clone, PartialEq)]
pub struct CardSample<'a> {
    pub card: &'a Card,
    pub center: Vec3,
    pub width: f32,
    /// `None` : les proportions de l'image
    pub height: Option<f32>,
    /// Radians
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub opacity: f32,
    /// Teinte des étapes (`keys`), multipliée à la couleur de la carte
    pub tint: [f32; 3],
}

fn yes() -> bool {
    true
}

/// Une cinématique, telle que l'écrit son fichier
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cinematic {
    /// Identifiant : le nom du fichier, sans `.json`
    #[serde(skip)]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Le joueur peut la passer (deux appuis sur Échap, Espace ou Entrée)
    #[serde(default = "yes")]
    pub skippable: bool,
    /// Bandeaux noirs en haut et en bas
    #[serde(default = "yes")]
    pub letterbox: bool,
    /// Ne se joue qu'une fois par monde
    #[serde(default = "yes")]
    pub once: bool,
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    pub shots: Vec<Shot>,
    #[serde(default)]
    pub captions: Vec<Caption>,
    #[serde(default)]
    pub fades: Vec<Fade>,
    #[serde(default)]
    pub cues: Vec<Cue>,
    /// Cartes de papier découpé posées dans la scène 3D
    #[serde(default)]
    pub cards: Vec<Card>,
    /// Lumière des cartes, et fond de papier
    #[serde(default)]
    pub stage: Stage,
    /// Événements renvoyés au jeu à la fin, qu'elle soit allée au bout ou ait été passée
    #[serde(default)]
    pub on_end: Vec<String>,
    /// Cinématique enchaînée à la fin
    #[serde(default)]
    pub next: Option<String>,
}

/// Caméra d'un instant, dans le repère de l'ancre (en blocs)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSample {
    pub eye: Vec3,
    pub target: Vec3,
    /// Champ de vision vertical, en radians
    pub fov: f32,
    /// Inclinaison, en radians
    pub roll: f32,
}

/// Un texte à l'écran, et son opacité (0 à 1)
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionSample<'a> {
    pub caption: &'a Caption,
    pub alpha: f32,
}

/// Ce que montre une cinématique à un instant
#[derive(Debug, Clone, PartialEq)]
pub struct Frame<'a> {
    pub camera: CameraSample,
    /// Couleur du voile et son opacité (0 à 1)
    pub fade: ([u8; 3], f32),
    pub captions: Vec<CaptionSample<'a>>,
    /// Hauteur des bandeaux noirs, de 0 (absents) à 1 (entiers) : ils glissent au début et à la fin
    pub letterbox: f32,
}

/// Secondes pour que les bandeaux noirs entrent ou sortent
pub const LETTERBOX_SECS: f32 = 0.8;

/// Cinématique illisible ou qui ne se tient pas, avec son identifiant
#[derive(Debug, thiserror::Error)]
#[error("{id} : {issue}")]
pub struct CinematicError {
    pub id: String,
    pub issue: CinematicIssue,
}

/// Ce qui cloche dans une cinématique (`name` : « plan 2 », « carte « titre » »…)
#[derive(Debug, thiserror::Error)]
pub enum CinematicIssue {
    #[error(transparent)]
    Io(std::io::Error),
    #[error(transparent)]
    Json(serde_json::Error),
    #[error("aucun plan (`shots`)")]
    NoShots,
    #[error("{0} : durée nulle")]
    ShotDuration(String),
    #[error("{0} : ni `position` ni `orbit`")]
    NoPlacement(String),
    #[error("{0} : `position` et `orbit` à la fois")]
    TwoPlacements(String),
    #[error("{0} : champ de vision hors de 1 à 170 degrés")]
    Fov(String),
    #[error("texte « {0} » : début ou durée invalide")]
    CaptionTiming(String),
    #[error("fondu : début ou durée invalide")]
    FadeTiming,
    #[error("fondu : opacité hors de 0 à 1")]
    FadeOpacity,
    #[error("{what} : couleur « {color} » invalide")]
    Color { what: String, color: String },
    #[error("{0} : début, durée ou fondu invalide")]
    CardTiming(String),
    #[error("{0} : taille nulle")]
    CardSize(String),
    #[error("{0} : nom d'image invalide (un fichier de cards/)")]
    CardImage(String),
    #[error("{0} : étapes (`keys`) dans le désordre")]
    CardKeys(String),
    #[error("événement (`cues`) : début négatif ou nom vide")]
    Cue,
}

/// `/cinematic` d'une cinématique que le catalogue ne connaît pas
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Unknown cinematic \"{0}\"")]
pub struct UnknownCinematic(pub String);

impl Cinematic {
    /// Lit une cinématique et vérifie qu'elle se tient
    pub fn from_json(id: &str, text: &str) -> Result<Self, CinematicError> {
        let error = |issue| CinematicError { id: id.to_string(), issue };
        let mut cinematic: Cinematic = serde_json::from_str(text).map_err(|e| error(CinematicIssue::Json(e)))?;
        cinematic.id = id.to_string();
        cinematic.validate().map_err(error)?;
        Ok(cinematic)
    }

    fn validate(&self) -> Result<(), CinematicIssue> {
        use CinematicIssue as I;
        if self.shots.is_empty() {
            return Err(I::NoShots);
        }
        for (i, shot) in self.shots.iter().enumerate() {
            let name = if shot.name.is_empty() { format!("plan {}", i + 1) } else { format!("plan « {} »", shot.name) };
            if !(shot.duration > 0.0) {
                return Err(I::ShotDuration(name));
            }
            match (&shot.position, &shot.orbit) {
                (None, None) => return Err(I::NoPlacement(name)),
                (Some(_), Some(_)) => return Err(I::TwoPlacements(name)),
                _ => {}
            }
            if !(1.0..=170.0).contains(&shot.fov.start()) || !(1.0..=170.0).contains(&shot.fov.end()) {
                return Err(I::Fov(name));
            }
        }
        for caption in &self.captions {
            if caption.at < 0.0 || !(caption.duration > 0.0) {
                return Err(I::CaptionTiming(caption.text.clone()));
            }
        }
        for fade in &self.fades {
            if fade.at < 0.0 || fade.duration < 0.0 {
                return Err(I::FadeTiming);
            }
            if !(0.0..=1.0).contains(&fade.from) || !(0.0..=1.0).contains(&fade.to) {
                return Err(I::FadeOpacity);
            }
            parse_color(&fade.color).ok_or_else(|| I::Color { what: "fondu".into(), color: fade.color.clone() })?;
        }
        for (i, card) in self.cards.iter().enumerate() {
            let name = if card.name.is_empty() { format!("carte {}", i + 1) } else { format!("carte « {} »", card.name) };
            if card.at < 0.0 || card.duration.is_some_and(|d| !(d > 0.0)) || card.fade < 0.0 {
                return Err(I::CardTiming(name));
            }
            if !(card.width.start() > 0.0 && card.width.end() > 0.0) || card.height.is_some_and(|h| !(h.start() > 0.0 && h.end() > 0.0)) || card.keys.iter().any(|k| !(k.width > 0.0)) {
                return Err(I::CardSize(name));
            }
            if let Some(color) = &card.color {
                parse_color(color).ok_or_else(|| I::Color { what: name.clone(), color: color.clone() })?;
            }
            if card.image.as_deref().is_some_and(|i| i.is_empty() || i.contains("..") || i.starts_with('/') || i.contains('\\')) {
                return Err(I::CardImage(name));
            }
            if card.keys.windows(2).any(|w| !(w[1].t >= w[0].t)) || card.keys.first().is_some_and(|k| k.t < 0.0) {
                return Err(I::CardKeys(name));
            }
        }
        if let Some(color) = &self.stage.backdrop {
            parse_color(color).ok_or_else(|| I::Color { what: "fond".into(), color: color.clone() })?;
        }
        parse_color(&self.stage.light.color).ok_or_else(|| I::Color { what: "lumière".into(), color: self.stage.light.color.clone() })?;
        if self.cues.iter().any(|c| c.at < 0.0 || c.emit.is_empty()) {
            return Err(I::Cue);
        }
        Ok(())
    }

    /// Durée totale : le dernier plan, texte ou fondu qui se termine
    pub fn duration(&self) -> f32 {
        let shots: f32 = self.shots.iter().map(|s| s.duration).sum();
        let captions = self.captions.iter().map(|c| c.at + c.duration);
        let fades = self.fades.iter().map(|f| f.at + f.duration);
        let cues = self.cues.iter().map(|c| c.at);
        let cards = self.cards.iter().filter_map(|c| c.duration.map(|d| c.at + d));
        captions.chain(fades).chain(cues).chain(cards).fold(shots, f32::max)
    }

    /// Les cartes visibles à l'instant `t`
    pub fn cards_at(&self, t: f32) -> Vec<CardSample<'_>> {
        let end = self.duration();
        self.cards
            .iter()
            .filter_map(|card| {
                let stop = card.duration.map_or(end, |d| card.at + d);
                if t < card.at || t > stop {
                    return None;
                }
                let life = (stop - card.at).max(1e-3);
                let k = card.ease.apply((t - card.at) / life);
                let fade = if card.fade > 0.0 { ((t - card.at) / card.fade).min((stop - t) / card.fade).clamp(0.0, 1.0) } else { 1.0 };
                let keyed = card.key_at(t - card.at);
                let opacity = fade * keyed.map_or(card.opacity.at(k), |key| key.2).clamp(0.0, 1.0);
                // Balancement lent, décalé d'une carte à l'autre
                let phase = card.position.start()[0] * 1.7 + card.position.start()[2] * 0.9;
                let sway = (card.sway.to_radians()) * (t * 1.3 + phase).sin();
                Some(CardSample {
                    card,
                    center: keyed.map_or(card.position.at(k), |key| key.0),
                    width: keyed.map_or(card.width.at(k), |key| key.1),
                    tint: keyed.map_or([1.0; 3], |key| key.3.to_array()),
                    height: keyed.and_then(|key| key.5).or(card.height.map(|h| h.at(k))),
                    yaw: card.yaw.at(k).to_radians(),
                    pitch: card.pitch.at(k).to_radians(),
                    roll: (card.roll.at(k) + keyed.map_or(0.0, |key| key.4)).to_radians() + sway,
                    opacity,
                })
            })
            .collect()
    }

    /// Les images citées par les cartes, sans doublon
    pub fn card_images(&self) -> impl Iterator<Item = &str> {
        let mut seen = HashSet::new();
        self.cards.iter().filter_map(|c| c.image.as_deref()).filter(move |i| seen.insert(*i))
    }

    /// Le plan en cours à l'instant `t` : son rang, son début et sa fin (le dernier plan dure
    /// jusqu'à la fin de la cinématique)
    pub fn shot_span_at(&self, t: f32) -> (usize, f32, f32) {
        let mut start = 0.0;
        for (i, shot) in self.shots.iter().enumerate() {
            if t < start + shot.duration && i + 1 < self.shots.len() {
                return (i, start, start + shot.duration);
            }
            start += shot.duration;
        }
        let last = self.shots.len().saturating_sub(1);
        (last, start - self.shots.last().map_or(0.0, |s| s.duration), self.duration().max(start))
    }

    /// Les images des cartes visibles entre `t0` et `t1`, sans doublon : ce qu'il faut charger
    /// pour un plan (les découpes se chargent plan par plan)
    pub fn card_images_between(&self, t0: f32, t1: f32) -> Vec<&str> {
        let end = self.duration();
        let mut seen = HashSet::new();
        self.cards
            .iter()
            // Une carte qui commence à la fin du plan (la scène suivante) ou finit à son début n'y est pas
            .filter(|c| c.at < t1 && c.duration.map_or(end, |d| c.at + d) > t0)
            .filter_map(|c| c.image.as_deref())
            .filter(|i| seen.insert(*i))
            .collect()
    }

    /// Le plan en cours à l'instant `t` et son avancement (0 à 1, avant la courbe)
    fn shot_at(&self, t: f32) -> (&Shot, f32) {
        let mut start = 0.0;
        for shot in &self.shots {
            if t < start + shot.duration {
                return (shot, ((t - start) / shot.duration).max(0.0));
            }
            start += shot.duration;
        }
        // Après le dernier plan (un texte ou un fondu dure encore) : la caméra reste où elle est
        (self.shots.last().expect("au moins un plan, vérifié au chargement"), 1.0)
    }

    /// Ce que montre la cinématique à l'instant `t` (en secondes)
    pub fn sample(&self, t: f32) -> Frame<'_> {
        let (shot, k) = self.shot_at(t);
        let k = shot.ease.apply(k);
        let target = shot.look_at.at(k);
        let mut eye = match (&shot.position, &shot.orbit) {
            (Some(position), _) => position.at(k),
            (None, Some(orbit)) => {
                let angle = orbit.angle.at(k).to_radians();
                let radius = orbit.radius.at(k);
                // Angle 0 : derrière l'ancre (z négatif), 90 : à sa droite
                target + Vec3::new(angle.sin() * radius, orbit.height.at(k), -angle.cos() * radius)
            }
            (None, None) => target - Vec3::Z,
        };
        if shot.shake > 0.0 {
            // Tremblement régulier, sans hasard : le même à chaque lecture
            eye += Vec3::new((t * 23.0).sin() + (t * 37.0).sin() * 0.5, (t * 29.0).sin() * 0.7, (t * 31.0).cos() * 0.5) * shot.shake * 0.6;
        }
        let camera = CameraSample { eye, target, fov: shot.fov.at(k).to_radians(), roll: shot.roll.at(k).to_radians() };

        // Voile : le dernier fondu commencé décide, figé sur sa fin une fois terminé
        let mut fade = ([0, 0, 0], 0.0);
        for f in self.fades.iter().filter(|f| t >= f.at) {
            let k = if f.duration > 0.0 { ((t - f.at) / f.duration).clamp(0.0, 1.0) } else { 1.0 };
            fade = (parse_color(&f.color).unwrap_or([0, 0, 0]), f.from + (f.to - f.from) * Ease::InOut.apply(k));
        }

        let captions = self
            .captions
            .iter()
            .filter(|c| t >= c.at && t < c.at + c.duration)
            .map(|c| {
                let fade = c.fade.min(c.duration / 2.0).max(1e-3);
                let alpha = ((t - c.at) / fade).min((c.at + c.duration - t) / fade).clamp(0.0, 1.0);
                CaptionSample { caption: c, alpha }
            })
            .collect();

        let letterbox = if self.letterbox {
            let end = self.duration();
            (t / LETTERBOX_SECS).min((end - t) / LETTERBOX_SECS).clamp(0.0, 1.0)
        } else {
            0.0
        };

        Frame { camera, fade, captions, letterbox }
    }

    /// Événements des `cues` entre `from` (exclu) et `to` (inclus)
    fn cues_between(&self, from: f32, to: f32) -> impl Iterator<Item = GameEvent> + '_ {
        self.cues.iter().filter(move |c| c.at > from && c.at <= to).map(|c| GameEvent { name: c.emit.clone(), tags: c.tags.clone(), position: None })
    }
}

/// `#RRGGBB`
pub fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Toutes les cinématiques du jeu
#[derive(Debug, Clone, Default)]
pub struct CinematicCatalog {
    cinematics: HashMap<String, std::sync::Arc<Cinematic>>,
}

impl CinematicCatalog {
    /// Les cinématiques de `assets/cinematics/`. Un fichier invalide est signalé et ignoré.
    pub fn load_default() -> Self {
        let Some(dir) = crate::skills::asset_dir(CINEMATICS_DIR) else {
            log::warn!("Dossier {CINEMATICS_DIR}/ introuvable : aucune cinématique.");
            return Self::default();
        };
        let (catalog, errors) = Self::load_dir(&dir);
        for error in &errors {
            log::warn!("Cinématique ignorée : {error}");
        }
        log::info!("{} cinématique(s) chargée(s) depuis {}.", catalog.cinematics.len(), dir.display());
        catalog
    }

    /// Lit les cinématiques de `dir`, et renvoie aussi les erreurs (fichier illisible, cinématique
    /// enchaînée inconnue) : les fichiers fautifs sont ignorés
    pub fn load_dir(dir: &Path) -> (Self, Vec<String>) {
        let mut catalog = Self::default();
        let mut errors = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();
        for path in files {
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            match std::fs::read_to_string(&path).map_err(|e| CinematicError { id: id.clone(), issue: CinematicIssue::Io(e) }).and_then(|text| Cinematic::from_json(&id, &text)) {
                Ok(cinematic) => catalog.insert(cinematic),
                Err(e) => errors.push(e.to_string()),
            }
        }
        for cinematic in catalog.cinematics.values() {
            if let Some(next) = cinematic.next.as_ref().filter(|n| !catalog.cinematics.contains_key(*n)) {
                errors.push(format!("{} : cinématique enchaînée « {next} » inconnue", cinematic.id));
            }
        }
        (catalog, errors)
    }

    pub fn insert(&mut self, cinematic: Cinematic) {
        self.cinematics.insert(cinematic.id.clone(), std::sync::Arc::new(cinematic));
    }

    pub fn get(&self, id: &str) -> Option<&std::sync::Arc<Cinematic>> {
        self.cinematics.get(id)
    }

    /// Identifiants, dans l'ordre alphabétique
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.cinematics.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn len(&self) -> usize {
        self.cinematics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cinematics.is_empty()
    }

    /// Les cinématiques que déclenche cet événement, dans l'ordre alphabétique
    pub fn triggered_by<'a>(&'a self, event: &'a GameEvent) -> impl Iterator<Item = &'a std::sync::Arc<Cinematic>> + 'a {
        let mut found: Vec<_> = self.cinematics.values().filter(|c| c.triggers.iter().any(|t| t.matches(event))).collect();
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found.into_iter()
    }
}

/// Une cinématique en cours de lecture
#[derive(Debug, Clone)]
pub struct Playing {
    pub cinematic: std::sync::Arc<Cinematic>,
    /// Secondes écoulées
    pub time: f32,
    /// Événement qui l'a déclenchée (sa position sert d'ancre)
    pub event: Option<GameEvent>,
    /// Le joueur a demandé à passer une fois : un second appui passe pour de bon
    pub skip_armed: Option<f32>,
}

/// Secondes pendant lesquelles un premier appui attend le second pour passer
pub const SKIP_CONFIRM_SECS: f32 = 3.0;

/// Ce qu'a donné une demande de passer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipRequest {
    /// Rien ne se joue, ou la cinématique ne se passe pas
    Ignored,
    /// Premier appui : l'invitation à confirmer s'affiche
    Armed,
    /// Second appui : la cinématique est passée
    Skipped,
}

/// Le lecteur : la cinématique en cours, celles qui attendent, celles déjà vues dans ce monde
#[derive(Debug, Default)]
pub struct CinematicPlayer {
    pub catalog: CinematicCatalog,
    playing: Option<Playing>,
    queue: std::collections::VecDeque<(std::sync::Arc<Cinematic>, Option<GameEvent>)>,
    /// Cinématiques `once` déjà jouées (enregistrées avec le monde)
    pub seen: HashSet<String>,
    /// Événements renvoyés au jeu, en attente de `take_events`
    events: Vec<GameEvent>,
    /// Une cinématique vient de commencer : le client fixe son ancre
    started: bool,
}

/// Nom de l'événement renvoyé à la fin de chaque cinématique (étiquette `id`, et `skipped` =
/// `true` si elle a été passée)
pub const ENDED_EVENT: &str = "cinematic_ended";

impl CinematicPlayer {
    pub fn new(catalog: CinematicCatalog) -> Self {
        Self { catalog, ..Default::default() }
    }

    pub fn playing(&self) -> Option<&Playing> {
        self.playing.as_ref()
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Une cinématique a commencé depuis le dernier appel (une seule fois par cinématique)
    pub fn take_started(&mut self) -> bool {
        std::mem::take(&mut self.started)
    }

    /// Un événement du jeu : les cinématiques qu'il déclenche (et pas encore vues) se jouent à la
    /// suite. Renvoie combien ont été mises en lecture.
    pub fn notify(&mut self, event: &GameEvent) -> usize {
        let found: Vec<_> = self.catalog.triggered_by(event).filter(|c| !(c.once && self.seen.contains(&c.id))).cloned().collect();
        let count = found.len();
        for cinematic in found {
            self.enqueue(cinematic, Some(event.clone()));
        }
        count
    }

    /// Joue une cinématique par son identifiant, vue ou non (commande `/cinematic`)
    pub fn play(&mut self, id: &str) -> Result<(), UnknownCinematic> {
        let cinematic = self.catalog.get(id).cloned().ok_or_else(|| UnknownCinematic(id.to_string()))?;
        self.enqueue(cinematic, None);
        Ok(())
    }

    fn enqueue(&mut self, cinematic: std::sync::Arc<Cinematic>, event: Option<GameEvent>) {
        let already = self.playing.as_ref().is_some_and(|p| p.cinematic.id == cinematic.id) || self.queue.iter().any(|(c, _)| c.id == cinematic.id);
        if already {
            return;
        }
        self.queue.push_back((cinematic, event));
        if self.playing.is_none() {
            self.start_next();
        }
    }

    fn start_next(&mut self) {
        if let Some((cinematic, event)) = self.queue.pop_front() {
            if cinematic.once {
                self.seen.insert(cinematic.id.clone());
            }
            log::info!("Cinématique « {} » ({}).", cinematic.title, cinematic.id);
            self.playing = Some(Playing { cinematic, time: 0.0, event, skip_armed: None });
            self.started = true;
        }
    }

    /// Avance la lecture de `dt` secondes : renvoie les événements atteints au jeu (`take_events`)
    pub fn update(&mut self, dt: f32) {
        let Some(playing) = self.playing.as_mut() else { return };
        let before = playing.time;
        playing.time += dt.max(0.0);
        if let Some(left) = playing.skip_armed.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                playing.skip_armed = None;
            }
        }
        let (now, end) = (playing.time, playing.cinematic.duration());
        let cinematic = playing.cinematic.clone();
        self.events.extend(cinematic.cues_between(before, now.min(end)));
        if now >= end {
            self.finish(false);
        }
    }

    /// Le joueur demande à passer : un premier appui arme, un second passe. Les événements que la
    /// cinématique devait encore renvoyer partent quand même, pour que le jeu reste cohérent.
    pub fn request_skip(&mut self) -> SkipRequest {
        let Some(playing) = self.playing.as_mut().filter(|p| p.cinematic.skippable) else { return SkipRequest::Ignored };
        if playing.skip_armed.is_none() {
            playing.skip_armed = Some(SKIP_CONFIRM_SECS);
            return SkipRequest::Armed;
        }
        let cinematic = playing.cinematic.clone();
        let from = playing.time;
        self.events.extend(cinematic.cues_between(from, f32::INFINITY));
        self.finish(true);
        SkipRequest::Skipped
    }

    /// Arrête tout, sans rien renvoyer au jeu (changement de monde, déconnexion)
    pub fn stop(&mut self) {
        self.playing = None;
        self.queue.clear();
        self.started = false;
    }

    fn finish(&mut self, skipped: bool) {
        let Some(playing) = self.playing.take() else { return };
        let cinematic = playing.cinematic;
        for name in &cinematic.on_end {
            self.events.push(GameEvent::new(name.clone()));
        }
        let mut ended = GameEvent::new(ENDED_EVENT).tag("id", cinematic.id.clone());
        if skipped {
            ended = ended.tag("skipped", "true");
        }
        self.events.push(ended);
        // Enchaînée, la suivante passe avant celles qui attendaient
        if let Some(next) = cinematic.next.as_ref().and_then(|n| self.catalog.get(n)).cloned() {
            self.queue.push_front((next, playing.event));
        }
        self.start_next();
    }

    /// Les événements renvoyés au jeu depuis le dernier appel
    pub fn take_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    /// Ce que montre la cinématique en cours
    pub fn frame(&self) -> Option<Frame<'_>> {
        self.playing.as_ref().map(|p| p.cinematic.sample(p.time))
    }

    /// Lit la liste des cinématiques déjà vues (`cinematiques.json` de la sauvegarde)
    pub fn load_seen(&mut self, file: &Path) {
        self.seen = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()).unwrap_or_default().into_iter().collect();
    }

    /// Enregistre la liste des cinématiques déjà vues
    pub fn save_seen(&self, file: &Path) -> std::io::Result<()> {
        let mut seen: Vec<&String> = self.seen.iter().collect();
        seen.sort();
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, serde_json::to_string_pretty(&seen).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_json() -> &'static str {
        r##"{
            "title": "Essai",
            "triggers": [{"event": "stone_obtained", "when": {"stone": "void"}}],
            "shots": [
                {"duration": 2, "position": [[0, 2, -4], [0, 2, -2]], "ease": "linear"},
                {"duration": 2, "orbit": {"radius": 5, "angle": [0, 90]}, "look_at": [0, 0, 0], "ease": "linear"}
            ],
            "captions": [{"at": 1, "duration": 2, "text": "Bonjour", "fade": 0.5}],
            "fades": [
                {"at": 0, "duration": 1, "from": 1, "to": 0},
                {"at": 4, "duration": 1, "from": 0, "to": 1, "color": "#FFFFFF"}
            ],
            "cues": [{"at": 4.5, "emit": "vision_ended"}],
            "on_end": ["fin"]
        }"##
    }

    fn sample() -> Cinematic {
        Cinematic::from_json("essai", sample_json()).unwrap()
    }

    #[test]
    fn test_duration_and_shots() {
        let c = sample();
        assert_eq!(c.duration(), 5.0);
        let f = c.sample(1.0);
        assert!((f.camera.eye - Vec3::new(0.0, 2.0, -3.0)).length() < 1e-4);
        assert!((f.camera.target - Vec3::new(0.0, 1.6, 0.0)).length() < 1e-4);
        // Orbite : à la fin, un quart de tour, à droite de l'ancre
        let f = c.sample(3.999);
        assert!((f.camera.eye - Vec3::new(5.0, 0.0, 0.0)).length() < 0.05, "{:?}", f.camera.eye);
        // Après le dernier plan, la caméra reste en place
        assert!((c.sample(4.8).camera.eye - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn test_fades_captions_letterbox() {
        let c = sample();
        assert_eq!(c.sample(0.0).fade.1, 1.0);
        assert_eq!(c.sample(2.0).fade.1, 0.0);
        let late = c.sample(5.0).fade;
        assert_eq!(late, ([255, 255, 255], 1.0));
        assert!(c.sample(0.5).captions.is_empty());
        let caption = &c.sample(1.25).captions[0];
        assert_eq!(caption.caption.text, "Bonjour");
        assert!((caption.alpha - 0.5).abs() < 1e-4);
        assert_eq!(c.sample(2.0).captions[0].alpha, 1.0);
        assert_eq!(c.sample(0.0).letterbox, 0.0);
        assert_eq!(c.sample(2.5).letterbox, 1.0);
    }

    #[test]
    fn test_invalid_files_are_explained() {
        assert!(matches!(Cinematic::from_json("x", r#"{"title": "x", "shots": []}"#).unwrap_err().issue, CinematicIssue::NoShots));
        assert!(Cinematic::from_json("x", r#"{"title": "x", "shots": [{"duration": 1}]}"#).unwrap_err().to_string().contains("ni `position`"));
        let bad_color = r##"{"title": "x", "shots": [{"duration": 1, "position": [0,0,0]}], "fades": [{"at": 0, "duration": 1, "from": 0, "to": 1, "color": "rouge"}]}"##;
        assert!(matches!(Cinematic::from_json("x", bad_color).unwrap_err().issue, CinematicIssue::Color { .. }));
        assert!(Cinematic::from_json("x", r#"{"title": "x", "shots": [{"duration": 1, "position": [0,0,0]}], "typo": 1}"#).is_err());
    }

    fn player() -> CinematicPlayer {
        let mut catalog = CinematicCatalog::default();
        catalog.insert(sample());
        let mut suite = Cinematic::from_json("suite", r#"{"title": "Suite", "shots": [{"duration": 1, "position": [0,0,0]}]}"#).unwrap();
        suite.id = "suite".into();
        catalog.insert(suite);
        CinematicPlayer::new(catalog)
    }

    #[test]
    fn test_trigger_cues_and_end() {
        let mut p = player();
        assert_eq!(p.notify(&GameEvent::new("stone_obtained").tag("stone", "fire")), 0);
        assert!(!p.is_playing());
        assert_eq!(p.notify(&GameEvent::new("stone_obtained").tag("stone", "void").at(Vec3::ONE)), 1);
        assert!(p.take_started());
        assert_eq!(p.playing().unwrap().event.as_ref().unwrap().position, Some(Vec3::ONE));
        p.update(4.0);
        assert!(p.take_events().is_empty());
        p.update(0.6);
        assert_eq!(p.take_events(), vec![GameEvent::new("vision_ended")]);
        p.update(1.0);
        let events = p.take_events();
        assert_eq!(events[0], GameEvent::new("fin"));
        assert_eq!(events[1], GameEvent::new(ENDED_EVENT).tag("id", "essai"));
        assert!(!p.is_playing());
        // Déjà vue : ne se rejoue pas sur le même événement
        assert_eq!(p.notify(&GameEvent::new("stone_obtained").tag("stone", "void")), 0);
        assert!(p.seen.contains("essai"));
    }

    #[test]
    fn test_skip_needs_two_presses_and_still_emits_cues() {
        let mut p = player();
        p.play("essai").unwrap();
        p.update(1.0);
        assert_eq!(p.request_skip(), SkipRequest::Armed);
        p.update(SKIP_CONFIRM_SECS + 0.1);
        assert!(p.playing().unwrap().skip_armed.is_none(), "l'invitation retombe");
        assert_eq!(p.request_skip(), SkipRequest::Armed);
        assert_eq!(p.request_skip(), SkipRequest::Skipped);
        let names: Vec<String> = p.take_events().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["vision_ended", "fin", ENDED_EVENT]);
        assert!(!p.is_playing());
    }

    #[test]
    fn test_next_is_chained() {
        let mut p = player();
        let mut essai = sample();
        essai.next = Some("suite".into());
        p.catalog.insert(essai);
        p.play("essai").unwrap();
        p.take_started();
        p.update(10.0);
        assert_eq!(p.playing().unwrap().cinematic.id, "suite");
        assert!(p.take_started());
    }

    #[test]
    fn test_cards() {
        let json = r##"{
            "title": "Papier", "anchor": "stage",
            "stage": {"backdrop": "#E9DFC7", "light": {"ambient": 0.5}},
            "shots": [{"duration": 4, "position": [0, 2, -6]}],
            "cards": [
                {"image": "planete.png", "position": [[0, 2, 0], [0, 4, 0]], "width": 3, "ease": "linear"},
                {"at": 1, "duration": 2, "fade": 0.5, "position": [1, 0, 2], "width": 10, "height": 6, "color": "#C0A080"}
            ]
        }"##;
        let c = Cinematic::from_json("papier", json).unwrap();
        assert_eq!(c.anchor, Anchor::Stage);
        assert_eq!(c.stage.light.ambient, 0.5);
        assert_eq!(c.card_images().collect::<Vec<_>>(), ["planete.png"]);
        let cards = c.cards_at(0.5);
        assert_eq!(cards.len(), 1);
        let cards = c.cards_at(2.0);
        assert_eq!(cards.len(), 2);
        assert!((cards[0].center - Vec3::new(0.0, 3.0, 0.0)).length() < 1e-4);
        assert_eq!(cards[1].opacity, 1.0);
        assert_eq!(c.cards_at(1.25)[1].opacity, 0.5);
        assert!(Cinematic::from_json("x", &json.replace("planete.png", "../x.png")).is_err());
        assert!(Cinematic::from_json("x", &json.replace("\"width\": 3", "\"width\": 0")).is_err());
        assert!(Cinematic::from_json("x", &json.replace("planete.png", "genese/planete.png")).is_ok());
    }

    #[test]
    fn test_card_images_by_shot() {
        let json = r##"{
            "title": "Papier", "anchor": "stage",
            "shots": [{"duration": 4, "position": [0, 2, -6]}, {"duration": 3, "position": [0, 2, -6]}],
            "cards": [
                {"image": "a.png", "duration": 4, "position": [0, 0, 0], "width": 1},
                {"image": "b.png", "at": 3.5, "duration": 2, "position": [0, 0, 0], "width": 1},
                {"image": "c.png", "at": 5, "position": [0, 0, 0], "width": 1}
            ],
            "captions": [{"at": 6, "duration": 3, "text": "fin"}]
        }"##;
        let c = Cinematic::from_json("plans", json).unwrap();
        assert_eq!(c.shot_span_at(1.0), (0, 0.0, 4.0));
        assert_eq!(c.shot_span_at(4.0), (1, 4.0, 9.0));
        assert_eq!(c.shot_span_at(8.5), (1, 4.0, 9.0));
        assert_eq!(c.card_images_between(0.0, 4.0), ["a.png", "b.png"]);
        // Une carte qui finit au début du plan, ou commence à sa fin, n'y est pas
        assert_eq!(c.card_images_between(4.0, 9.0), ["b.png", "c.png"]);
        assert_eq!(c.card_images_between(0.0, 3.5), ["a.png"]);
        assert_eq!(c.card_images_between(4.5, 9.0), ["b.png", "c.png"]);
    }

    #[test]
    fn test_card_keys() {
        let json = r##"{
            "title": "Papier", "anchor": "stage",
            "shots": [{"duration": 4, "position": [0, 2, -6]}],
            "cards": [{"image": "a.png", "at": 1, "duration": 3, "keys": [
                {"t": 0, "position": [0, 0, 0], "width": 2},
                {"t": 1, "position": [2, 0, 0], "width": 4, "opacity": 0.5, "tint": [0.5, 1, 1]},
                {"t": 2, "position": [2, 2, 0], "width": 4}
            ]}]
        }"##;
        let c = Cinematic::from_json("cles", json).unwrap();
        let s = &c.cards_at(1.5)[0];
        assert!((s.center - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-4);
        assert!((s.width - 3.0).abs() < 1e-4 && (s.opacity - 0.75).abs() < 1e-4);
        assert!((s.tint[0] - 0.75).abs() < 1e-4 && s.tint[1] == 1.0);
        let s = &c.cards_at(3.5)[0];
        assert!((s.center - Vec3::new(2.0, 2.0, 0.0)).length() < 1e-4 && s.opacity == 1.0);
        assert!(Cinematic::from_json("x", &json.replace("\"t\": 2", "\"t\": 0.5")).is_err());
    }

    #[test]
    fn test_parse_event() {
        assert_eq!(GameEvent::parse("stone_obtained stone=void"), Some(GameEvent::new("stone_obtained").tag("stone", "void")));
        assert_eq!(GameEvent::parse("x =1"), None);
        assert_eq!(GameEvent::parse(""), None);
    }

    #[test]
    fn test_seen_round_trip() {
        let dir = std::env::temp_dir().join(format!("herocraft_cine_{}", std::process::id()));
        let file = dir.join("cinematiques.json");
        let mut p = player();
        p.seen.insert("essai".into());
        p.save_seen(&file).unwrap();
        let mut q = player();
        q.load_seen(&file);
        assert!(q.seen.contains("essai"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_game_cinematics_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CINEMATICS_DIR);
        let (catalog, errors) = CinematicCatalog::load_dir(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!catalog.is_empty());
        // La fin de la vision part quand la Pierre du Néant est obtenue, et renvoie au présent
        let event = GameEvent::new("stone_obtained").tag("stone", "voidStone").tag("kind", "whole");
        let vision: Vec<_> = catalog.triggered_by(&event).collect();
        assert_eq!(vision.len(), 1);
        assert!(vision[0].cues.iter().any(|c| c.emit == "vision_ended"));
    }
}

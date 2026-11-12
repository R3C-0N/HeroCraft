//! Créatures de la planète : les espèces, décrites chacune par un fichier JSON du dossier
//! `assets/creatures/` (corps, allure, biomes où elles apparaissent, modèle 3D glTF et sa texture),
//! et l'état d'une créature tel que le serveur le diffuse.
//!
//! Ajouter une bête, c'est ajouter un fichier : la simulation (`herocraft_world::creatures`) et le
//! rendu du client lisent tout dans ces descriptions. Le format est décrit dans
//! `assets/creatures/README.md`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Dossier des descriptions d'espèces, depuis le dossier de lancement
pub const CREATURES_DIR: &str = "assets/creatures";
/// Points de vie retirés à une créature par un coup de joueur
pub const PLAYER_HIT_DAMAGE: f32 = 2.5;
/// Durée de la teinte rouge d'une créature frappée, en secondes
pub const CREATURE_HURT_TIME: f32 = 0.4;
/// Durée de la chute sur le flanc d'une créature morte, avant qu'elle disparaisse, en secondes
pub const CREATURE_DEATH_TIME: f32 = 0.8;
/// Durée pendant laquelle une bête morte reste à terre, à dépecer, avant de disparaître, en secondes
pub const CORPSE_TIME: f32 = 600.0;

/// Une espèce de créature, telle que décrite par son fichier
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureSpecies {
    /// Identifiant : le nom du fichier sans `.json` (ex. `mouton`)
    #[serde(skip)]
    pub id: String,
    /// Nom affiché
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Familles de l'espèce, que certaines compétences visent (`shadow` : créature de l'ombre)
    #[serde(default)]
    pub tags: Vec<String>,
    /// Volume du corps pour les déplacements
    pub body: CreatureBody,
    /// Vitesse de marche, en blocs par seconde
    pub walk_speed: f32,
    /// Points de vie : chaque coup d'un joueur en retire `PLAYER_HIT_DAMAGE`
    #[serde(default = "default_health")]
    pub health: f32,
    /// Taille d'un groupe qui apparaît ensemble : au moins `min_group`, au plus `max_group`
    #[serde(default = "one")]
    pub min_group: u32,
    #[serde(default = "one")]
    pub max_group: u32,
    /// Biomes où l'espèce apparaît (aucun : elle n'apparaît pas d'elle-même)
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
    /// Réaction à l'approche d'un joueur (indifférente par défaut)
    #[serde(default)]
    pub temperament: Temperament,
    /// Moment de la journée où l'espèce est éveillée : elle dort le reste du temps
    #[serde(default)]
    pub activity: Activity,
    /// Comportement agressif (aucun : la bête est paisible et s'enfuit quand on la frappe)
    #[serde(default)]
    pub attack: Option<CreatureAttack>,
    /// Espèces qu'elle fuit dès qu'elle en voit une approcher (identifiants de leurs fichiers)
    #[serde(default)]
    pub afraid_of: Vec<String>,
    /// Humanoïde (habitant, voyageur) : dessiné avec le corps des héros et l'apparence que lui donne
    /// la simulation des villages, sans modèle glTF
    #[serde(default)]
    pub humanoid: bool,
    /// Elle se garde : les coups qui lui arrivent de face perdent une part de leurs dégâts, tant
    /// qu'elle n'est ni étourdie ni brisée (`break_guard`)
    #[serde(default)]
    pub guard: Option<CreatureGuard>,
    /// Élevage : ce qu'elle mange, ce qu'il faut pour l'apprivoiser, le temps que met un petit à
    /// grandir (aucun : elle ne s'élève pas)
    #[serde(default)]
    pub breeding: Option<Breeding>,
    /// Elle dort tout l'hiver (ours), là où les saisons sont marquées
    #[serde(default)]
    pub hibernation: bool,
    /// Ce que la bête tuée laisse tomber (voir `loot`)
    #[serde(default)]
    pub loot: Vec<crate::loot::LootEntry>,
    /// Modèle 3D (absent pour un humanoïde)
    #[serde(default)]
    pub model: CreatureModel,
    /// Provocation, en blocs : les bêtes agressives à cette distance d'elle s'en prennent à elle
    /// avant tout autre (Chevalier de la Mort)
    #[serde(default)]
    pub taunt: Option<f32>,
    /// Corps solide : forme, taille et résistance (sans : un cercle de la demi-largeur du corps, de
    /// résistance 0,5)
    #[serde(default)]
    pub collision: Option<CreatureCollision>,
    /// Elle attire les ennemis proches (Piège Magnétique)
    #[serde(default)]
    pub magnet: Option<CreatureMagnet>,
}

/// Forme du corps solide d'une espèce, vue de dessus
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollisionShape {
    /// Un disque (bête, humanoïde, tourelle)
    #[default]
    Circle,
    /// Un rectangle tourné avec la bête (mur, barricade)
    Box,
}

/// Corps solide d'une espèce, tel qu'écrit dans sa fiche
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureCollision {
    #[serde(default)]
    pub shape: CollisionShape,
    /// Demi-largeur et demi-longueur, en blocs (un cercle a pour rayon la première) ; sans : celles
    /// du corps (`body`)
    #[serde(default)]
    pub size: Option<[f32; 2]>,
    /// Hauteur, en blocs (sans : celle du corps)
    #[serde(default)]
    pub height: Option<f32>,
    /// De 0 à 1 : la part de l'enfoncement d'un autre corps qu'elle repousse à chaque instant. 0 :
    /// on la traverse (piège) ; 1 : rien ne la traverse (mur)
    #[serde(default = "half_f32")]
    pub resistance: f32,
}

fn half_f32() -> f32 {
    0.5
}

/// Corps solide d'une entité, en blocs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collider {
    pub shape: CollisionShape,
    pub half_width: f32,
    pub half_length: f32,
    pub height: f32,
    pub resistance: f32,
}

/// Le corps d'un joueur, pour les collisions avec les entités
pub const PLAYER_COLLIDER: Collider = Collider { shape: CollisionShape::Circle, half_width: 0.3, half_length: 0.3, height: 1.8, resistance: 0.5 };

impl CreatureSpecies {
    /// Son corps solide, à l'échelle 1
    pub fn collider(&self) -> Collider {
        let c = self.collision.unwrap_or(CreatureCollision { shape: CollisionShape::Circle, size: None, height: None, resistance: 0.5 });
        let [half_width, half_length] = c.size.unwrap_or([self.body.half_width, self.body.half_length]);
        Collider { shape: c.shape, half_width, half_length, height: c.height.unwrap_or(self.body.height), resistance: c.resistance.clamp(0.0, 1.0) }
    }
}

impl Collider {
    /// Rayon du disque qui le remplace quand il bouge contre un autre
    pub fn radius(&self) -> f32 {
        match self.shape {
            CollisionShape::Circle => self.half_width,
            CollisionShape::Box => self.half_width.min(self.half_length),
        }
    }

    /// Ce corps, posé les pieds en `feet` et tourné du cap `yaw`, est enfoncé par un disque de rayon
    /// `radius` (en blocs) aux pieds en `other` : le déplacement, dans le plan du sol, qui sort le
    /// disque de lui (`block` : taille d'un bloc). Aucun s'ils ne se touchent pas, ou si l'un passe
    /// au-dessus de l'autre (`other_height` : hauteur du disque).
    pub fn separation(&self, feet: Vec3, yaw: f32, block: f32, other: Vec3, radius: f32, other_height: f32) -> Option<Vec3> {
        let up = feet.normalize_or_zero();
        let rise = (other.length() - feet.length()) / block;
        if rise >= self.height || rise + other_height <= 0.0 {
            return None;
        }
        let (east, north) = crate::math::tangent_frame(up);
        let ahead = north * yaw.cos() + east * yaw.sin();
        let side = east * yaw.cos() - north * yaw.sin();
        let d = (other - feet) / block;
        let (f, s) = (d.dot(ahead), d.dot(side));
        let (along, across, depth) = match self.shape {
            CollisionShape::Circle => {
                let distance = (f * f + s * s).sqrt();
                let depth = self.half_width + radius - distance;
                if depth <= 0.0 {
                    return None;
                }
                if distance < 1e-4 { (1.0, 0.0, depth) } else { (f / distance, s / distance, depth) }
            }
            CollisionShape::Box => {
                let (cf, cs) = (f.clamp(-self.half_length, self.half_length), s.clamp(-self.half_width, self.half_width));
                let (df, ds) = (f - cf, s - cs);
                let outside = (df * df + ds * ds).sqrt();
                if outside > 1e-4 {
                    let depth = radius - outside;
                    if depth <= 0.0 {
                        return None;
                    }
                    (df / outside, ds / outside, depth)
                } else {
                    // Le centre est dedans : il sort par le côté le plus proche
                    let (pf, ps) = (self.half_length - f.abs() + radius, self.half_width - s.abs() + radius);
                    if pf < ps { (if f < 0.0 { -1.0 } else { 1.0 }, 0.0, pf) } else { (0.0, if s < 0.0 { -1.0 } else { 1.0 }, ps) }
                }
            }
        };
        Some((ahead * along + side * across) * depth * block)
    }
}

/// Élevage d'une espèce : nourrie de ce qu'elle aime, une bête sauvage finit par s'apprivoiser ;
/// apprivoisée et nourrie, elle cherche un partenaire apprivoisé de son espèce, et le couple donne
/// un petit, qui grandit
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breeding {
    /// Ce qu'elle mange (identifiants d'objets ou de blocs) : tenu en main, il l'attire
    pub food: Vec<String>,
    /// Chance qu'un repas apprivoise une bête sauvage (une sur trois par défaut)
    #[serde(default = "default_taming_chance")]
    pub taming_chance: f32,
    /// Temps qu'il faut à un petit pour devenir adulte, en minutes
    #[serde(default = "default_growth_minutes")]
    pub growth_minutes: f32,
    /// Ponte d'une bête apprivoisée et adulte (aucune : elle ne pond pas)
    #[serde(default)]
    pub lays: Option<Laying>,
}

/// Ce que pond une bête apprivoisée, et à quel rythme
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Laying {
    /// Objet pondu (identifiant de sa fiche)
    pub item: String,
    /// Temps entre deux pontes, au moins et au plus (tiré à chaque ponte), en minutes
    pub minutes: [f32; 2],
}

fn default_taming_chance() -> f32 {
    1.0 / 3.0
}

fn default_growth_minutes() -> f32 {
    10.0
}

/// Façon d'attaquer d'une bête agressive
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttackMode {
    /// Seule : elle poursuit sa cible et la mord dès qu'elle est à portée
    Solo,
    /// En meute : elle n'ose attaquer qu'entourée d'au moins `pack_size` bêtes de son espèce, qui se
    /// joignent toutes à la chasse et encerclent la cible ; trop peu nombreuses, elles fuient
    Pack,
    /// À la charge : elle s'arrête, vise, puis fonce en ligne droite et souffle un moment après
    Charge,
}

/// Comportement agressif : qui la bête chasse, comment, et ce que coûte sa morsure
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureAttack {
    pub mode: AttackMode,
    /// Points de vie retirés par coup (à un joueur ou à une proie)
    pub damage: f32,
    /// Temps entre deux coups, en secondes
    #[serde(default = "default_attack_cooldown")]
    pub cooldown: f32,
    /// Distance à laquelle elle repère une cible, en blocs
    #[serde(default = "default_sight")]
    pub sight: f32,
    /// Allonge d'un coup, en blocs devant son museau
    #[serde(default = "default_reach")]
    pub reach: f32,
    /// Allure de la poursuite (ou de la charge), par rapport à la marche
    #[serde(default = "default_chase_speed")]
    pub chase_speed: f32,
    /// Elle s'en prend aux joueurs qu'elle voit (sinon seulement à celui qui l'a frappée)
    #[serde(default = "yes")]
    pub hunts_players: bool,
    /// Elle n'attaque que frappée la première (ours, tréant…)
    #[serde(default)]
    pub provoked_only: bool,
    /// Espèces qu'elle chasse (identifiants de leurs fichiers)
    #[serde(default)]
    pub prey: Vec<String>,
    /// En meute : nombre de bêtes de son espèce, elle comprise, qu'il lui faut autour d'elle pour oser
    #[serde(default = "default_pack_size")]
    pub pack_size: u32,
    /// Elle défend son troupeau : quand un joueur frappe l'une des bêtes de son troupeau près
    /// d'elle, elle s'en prend à lui (les gardes d'un village)
    #[serde(default)]
    pub defends_herd: bool,
    /// Elle ne s'en prend qu'aux cibles à moins de tant de blocs d'elle (aucun : sa vue, `sight`)
    #[serde(default)]
    pub engage: Option<f32>,
    /// Elle lâche sa cible à tant de blocs de là où la poursuite a commencé, et y revient
    #[serde(default)]
    pub leash: Option<f32>,
    /// Tir à distance (tourelle, bastion, liche) : la bête vise de loin au lieu de s'approcher, et
    /// frappe au contact seulement si sa cible est à portée de `reach`
    #[serde(default)]
    pub shot: Option<CreatureShot>,
    /// Piège : la première cible à portée de `reach` le déclenche ; il explose sur tout ennemi
    /// proche, puis disparaît
    #[serde(default)]
    pub burst: Option<CreatureBurst>,
    /// Son coup ralentit le joueur touché : `[part de la vitesse perdue, secondes]`
    #[serde(default)]
    pub slow: Option<[f32; 2]>,
    /// Son coup étourdit le joueur touché tant de secondes
    #[serde(default)]
    pub stun: Option<f32>,
    /// Son coup repousse le joueur touché : vitesse de départ, en blocs par seconde
    #[serde(default)]
    pub knockback: Option<f32>,
}

/// La garde d'une espèce (`CreatureSpecies::guard`)
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureGuard {
    /// Part des dégâts retirée aux coups de face (0,6 : 60 %)
    pub block: f32,
    /// Angle couvert devant elle, en degrés
    #[serde(default = "default_guard_arc")]
    pub arc: f32,
}

fn default_guard_arc() -> f32 {
    120.0
}

/// L'explosion d'un piège
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureBurst {
    /// Rayon, en blocs : chaque ennemi dans ce rayon autour du piège prend les dégâts de l'attaque
    pub radius: f32,
    /// Secondes pendant lesquelles les ennemis touchés restent immobilisés
    #[serde(default)]
    pub stun: f32,
    /// Effet visuel de l'explosion (`assets/effects/`)
    #[serde(default)]
    pub effect: Option<String>,
    /// Saignement des ennemis touchés : points de vie perdus chaque seconde, pendant
    /// `bleed_seconds` secondes
    #[serde(default)]
    pub bleed: f32,
    #[serde(default)]
    pub bleed_seconds: f32,
    /// Ce que le piège fait apparaître en se déclenchant, autour de lui (les mines de
    /// l'Éradication Ciblée)
    #[serde(default)]
    pub spawn: Option<BurstSpawn>,
}

/// Les entités qu'un piège fait apparaître en se déclenchant (`CreatureBurst::spawn`) : elles sont
/// au même invocateur que lui
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BurstSpawn {
    /// Espèce (identifiant de son fichier)
    pub species: String,
    #[serde(default = "one")]
    pub count: u32,
    /// Secondes avant qu'elles disparaissent si rien ne les déclenche
    #[serde(default = "default_spawn_seconds")]
    pub seconds: f32,
}

fn default_spawn_seconds() -> f32 {
    10.0
}

/// Aimant (Piège Magnétique) : les ennemis à moins de `radius` blocs sont tirés vers lui
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureMagnet {
    pub radius: f32,
    /// Vitesse à laquelle il les tire, en blocs par seconde, au bord de son rayon (moins près de lui)
    pub speed: f32,
}

/// Un tir à distance : un projectile part de la bête vers sa cible, et touche à son arrivée
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureShot {
    /// Portée, en blocs
    pub range: f32,
    /// Points de vie retirés à chaque cible touchée (sinon ceux de l'attaque)
    #[serde(default)]
    pub damage: Option<f32>,
    /// Vitesse du projectile, en blocs par seconde (0 : tout de suite, comme un jet de flammes)
    #[serde(default)]
    pub speed: f32,
    /// Rayon de l'explosion, en blocs : chaque ennemi dans ce rayon autour de la cible est touché
    /// (0 : la cible seule)
    #[serde(default)]
    pub radius: f32,
    /// Hauteur d'où part le tir, en blocs au-dessus des pieds
    #[serde(default = "one_f32")]
    pub height: f32,
    /// Effets visuels (`assets/effects/`) : à la bouche du tir, tourné vers la cible ; le long du
    /// trajet ; à l'arrivée
    #[serde(default)]
    pub muzzle: Option<String>,
    #[serde(default)]
    pub projectile: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    /// Le tir traverse : chaque ennemi à moins de tant de blocs de sa trajectoire est touché
    /// (0 : il s'arrête sur sa cible)
    #[serde(default)]
    pub pierce: f32,
}

impl CreatureShot {
    /// Durée du trajet d'un tir de `from` à `to`, en secondes (`block` : taille d'un bloc)
    pub fn flight_seconds(&self, from: Vec3, to: Vec3, block: f32) -> f32 {
        if self.speed > 0.0 { from.distance(to) / (self.speed * block) } else { 0.0 }
    }
}

/// Un tir de créature, pour le montrer aux joueurs
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureShotEvent {
    pub creature_id: u64,
    /// Rang de l'espèce du tireur dans le registre
    pub species: u32,
    /// Bouche du tir et point d'arrivée
    pub from: Vec3,
    pub to: Vec3,
}

/// Volume du corps, en blocs
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureBody {
    /// Demi-largeur (de côté)
    pub half_width: f32,
    /// Demi-longueur (de la tête à la queue)
    pub half_length: f32,
    pub height: f32,
}

/// Réaction d'une espèce à un joueur qui s'approche, tant qu'il ne l'a pas frappée (un coup la fait
/// toujours fuir)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Temperament {
    /// Ne prête pas attention aux joueurs
    #[default]
    Indifferent,
    /// S'enfuit quand un joueur s'approche
    Craintif,
    /// S'arrête et regarde le joueur qui passe près d'elle
    Placide,
    /// Suit le joueur à distance, et le regarde une fois près de lui
    Curieux,
}

/// Rythme de vie d'une espèce, selon la hauteur du soleil là où se trouve la bête (la planète est
/// ronde : il fait jour d'un côté et nuit de l'autre)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// Éveillée le jour et au crépuscule, endormie en pleine nuit
    #[default]
    Diurnal,
    /// Éveillée la nuit et au crépuscule, endormie en plein jour
    Nocturnal,
    /// Éveillée à l'aube et au crépuscule, endormie en plein jour comme en pleine nuit
    Crepuscular,
    /// Toujours éveillée
    Always,
}

impl Activity {
    /// Au-delà de cette hauteur du soleil (sinus de son élévation), c'est le plein jour ; en deçà de
    /// son opposé, la pleine nuit. Entre les deux, l'aube ou le crépuscule.
    pub const FULL_LIGHT: f32 = 0.1;
    /// Hauteur du soleil, au-dessus comme en dessous de l'horizon, jusqu'où s'éveille une espèce
    /// crépusculaire
    pub const TWILIGHT: f32 = 0.5;

    /// L'espèce est éveillée quand le soleil est à cette hauteur (sinus de son élévation, de -1 sous
    /// les pieds à 1 au zénith)
    pub fn is_awake(self, sun_height: f32) -> bool {
        match self {
            Activity::Diurnal => sun_height > -Self::FULL_LIGHT,
            Activity::Nocturnal => sun_height < Self::FULL_LIGHT,
            Activity::Crepuscular => sun_height.abs() < Self::TWILIGHT,
            Activity::Always => true,
        }
    }
}

/// Apparition dans un biome
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRule {
    /// Nom du biome (`Plains`, `Forest`, `Desert`… : ceux de `herocraft_world::BiomeType`)
    pub biome: String,
    /// Poids face aux autres espèces du même biome
    pub weight: f32,
    /// Chance que le groupe tiré apparaisse vraiment (1 : toujours), pour une espèce rare
    #[serde(default = "always")]
    pub chance: f32,
}

/// Modèle 3D : un maillage glTF à squelette et sa texture, chemins depuis `assets/creatures/`.
/// Le modèle est en blocs, y vers le haut, la tête vers +z ; le point le plus bas de sa pose de repos
/// est posé sur les pieds de la créature.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureModel {
    /// Fichier `.gltf` (données embarquées), un seul maillage à squelette (absent : humanoïde)
    #[serde(default)]
    pub mesh: String,
    /// Texture PNG, lue au plus proche texel
    #[serde(default)]
    pub texture: String,
    /// Animations de la bête, par nom : `repos`, `marche`, `course`, `attaque`, `coup`, et ses
    /// inactivités (celles qui ont une `pause` : `broute`, `sommeil`…). Rangées par nom, le même
    /// ordre sur le serveur et le client.
    #[serde(default)]
    pub animations: BTreeMap<String, CreatureAnimation>,
}

/// Une animation, jouée en boucle : des os qui oscillent autour d'une pose, ou une animation du glTF
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureAnimation {
    /// Animation du glTF à jouer à la place des mouvements d'os
    #[serde(default)]
    pub clip: Option<String>,
    /// Durée d'un cycle, en secondes
    #[serde(default = "one_second")]
    pub cycle: f32,
    /// Distance parcourue en un cycle, en blocs : le cycle suit alors le chemin parcouru (marche,
    /// course), pour que les pattes ne glissent pas
    #[serde(default)]
    pub foulee: Option<f32>,
    /// Mouvement de chaque os, par nom de nœud du glTF. Un nom qui finit par `*` vise tous les os qui
    /// commencent ainsi.
    #[serde(default)]
    pub os: BTreeMap<String, BoneMotion>,
    /// Inactivité que la bête choisit quand elle s'arrête
    #[serde(default)]
    pub pause: Option<PauseRule>,
}

/// Mouvement d'un os : une pose fixe plus une oscillation sinusoïdale, par-dessus sa pose de repos.
/// Angles en degrés autour des axes du modèle (x : bascule en avant, le museau pique vers le bas ;
/// y : tourne vers la gauche ; z : penche sur la droite), déplacements en blocs.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoneMotion {
    /// Amplitude de l'oscillation des angles
    #[serde(default)]
    pub rotation: [f32; 3],
    /// Angles tenus pendant toute l'animation
    #[serde(default)]
    pub rotation_fixe: [f32; 3],
    /// Amplitude de l'oscillation de la position
    #[serde(default)]
    pub translation: [f32; 3],
    /// Déplacement tenu pendant toute l'animation
    #[serde(default)]
    pub translation_fixe: [f32; 3],
    /// Avance de l'oscillation, en fraction de cycle (0,5 : à contretemps)
    #[serde(default)]
    pub phase: f32,
    /// Oscillations par cycle (2 : deux fois plus vite que le cycle)
    #[serde(default = "one_f32")]
    pub frequence: f32,
    /// Avance de phase ajoutée à chaque os suivant du même nom (mille-pattes, queue en segments)
    #[serde(default)]
    pub decalage: f32,
}

/// Une inactivité : la bête arrêtée la choisit au poids parmi les autres, et la tient un moment
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseRule {
    pub poids: f32,
    /// Durée, en secondes : au moins la première valeur, au plus la seconde
    pub duree: [f32; 2],
}

/// Inactivité d'une espèce dont la fiche n'en décrit aucune : elle se tient au repos
pub const DEFAULT_PAUSE: PauseRule = PauseRule { poids: 1.0, duree: [2.0, 6.0] };

/// Ce que fait une créature, pour choisir son animation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatureAction {
    /// Arrêtée (ou morte)
    #[default]
    Repos,
    Marche,
    /// Fuite ou poursuite
    Course,
    Attaque,
    /// Inactivité : rang dans `CreatureSpecies::pauses`
    Pause(u8),
    /// Elle apparaît (entité invoquée qui se déplie ou surgit du sol) : avancée de 0 à 255 de son
    /// animation `apparition`, jouée une seule fois
    Apparition(u8),
}

impl CreatureAction {
    /// Nom de l'animation de l'action dans la fiche de l'espèce
    pub fn animation_name(self, species: &CreatureSpecies) -> &str {
        match self {
            Self::Repos => "repos",
            Self::Marche => "marche",
            Self::Course => "course",
            Self::Attaque => "attaque",
            Self::Pause(i) => species.pauses().nth(i as usize).map_or("repos", |(name, _)| name),
            Self::Apparition(_) => "apparition",
        }
    }
}

fn one_second() -> f32 {
    1.0
}

fn one_f32() -> f32 {
    1.0
}

fn default_health() -> f32 {
    10.0
}

fn default_attack_cooldown() -> f32 {
    1.2
}

fn default_sight() -> f32 {
    12.0
}

fn default_reach() -> f32 {
    0.8
}

fn default_chase_speed() -> f32 {
    1.6
}

fn default_pack_size() -> u32 {
    3
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

fn always() -> f32 {
    1.0
}

/// Toutes les espèces connues, rangées par identifiant (le même ordre sur le serveur et le client)
#[derive(Debug, Clone, Default)]
pub struct CreatureRegistry {
    species: Vec<CreatureSpecies>,
    /// Dossier des descriptions, d'où partent les chemins des modèles
    dir: PathBuf,
}

impl CreatureRegistry {
    /// Les espèces de `assets/creatures/` : depuis le dossier de lancement, sinon celles livrées
    /// avec les sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(CREATURES_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {CREATURES_DIR}/ introuvable : aucune créature.");
            return Self::default();
        };
        match Self::load_dir(dir) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                log::info!("{} espèce(s) de créatures chargée(s) depuis {}.", registry.len(), dir.display());
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::default()
            }
        }
    }

    /// Comme `load_default`, sans rien écrire dans la console (une espèce invalide est ignorée)
    pub fn load_quiet() -> Self {
        let candidates = [PathBuf::from(CREATURES_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR)];
        candidates.iter().find(|dir| dir.is_dir()).and_then(|dir| Self::load_dir(dir).ok()).map_or_else(Self::default, |(registry, _)| registry)
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les espèces valides et les erreurs des autres.
    pub fn load_dir(dir: &Path) -> io::Result<(Self, Vec<String>)> {
        let mut registry = Self { species: Vec::new(), dir: dir.to_path_buf() };
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| CreatureSpecies::parse(&id, &text));
            match result {
                Ok(species) => registry.species.push(species),
                Err(e) => errors.push(format!("Créature {} ignorée : {e}", path.display())),
            }
        }
        registry.species.sort_by(|a, b| a.id.cmp(&b.id));
        Ok((registry, errors))
    }

    pub fn from_species(mut species: Vec<CreatureSpecies>, dir: PathBuf) -> Self {
        species.sort_by(|a, b| a.id.cmp(&b.id));
        Self { species, dir }
    }

    /// Chemin d'un fichier cité par une description (modèle, texture)
    pub fn asset_path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    pub fn len(&self) -> usize {
        self.species.len()
    }

    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&CreatureSpecies> {
        self.species.get(index)
    }

    /// Rang de l'espèce d'identifiant `id`
    pub fn find(&self, id: &str) -> Option<usize> {
        self.species.iter().position(|s| s.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CreatureSpecies> {
        self.species.iter()
    }
}

impl CreatureSpecies {
    /// Lit la description JSON d'une espèce et vérifie qu'elle tient debout
    pub fn parse(id: &str, json: &str) -> Result<Self, String> {
        let mut species: CreatureSpecies = serde_json::from_str(json).map_err(|e| e.to_string())?;
        species.id = id.to_string();
        let body = species.body;
        if !(body.half_width > 0.0 && body.half_length > 0.0 && body.height > 0.0 && species.walk_speed >= 0.0 && species.health > 0.0) {
            return Err("corps, vitesse ou vie invalides (valeurs positives attendues)".into());
        }
        if !species.humanoid && (species.model.mesh.is_empty() || species.model.texture.is_empty()) {
            return Err("modèle manquant (`model.mesh` et `model.texture`)".into());
        }
        if species.min_group == 0 || species.max_group < species.min_group {
            return Err("taille de groupe invalide (1 ≤ min_group ≤ max_group)".into());
        }
        if species.spawn.iter().any(|rule| !(rule.weight >= 0.0 && (0.0..=1.0).contains(&rule.chance))) {
            return Err("apparition invalide (poids positif, chance entre 0 et 1)".into());
        }
        if let Some(a) = &species.attack {
            if !(a.damage > 0.0 && a.cooldown > 0.0 && a.sight > 0.0 && a.reach >= 0.0 && a.chase_speed > 0.0 && a.pack_size >= 1) {
                return Err("attaque invalide (dégâts, délai, vue et allure positifs, meute d'au moins 1)".into());
            }
        }
        if let Some(b) = &species.breeding {
            if b.food.is_empty() || !(b.taming_chance > 0.0 && b.taming_chance <= 1.0) || !(b.growth_minutes > 0.0) {
                return Err("élevage invalide (au moins une nourriture, chance d'apprivoiser entre 0 et 1, croissance positive)".into());
            }
            if let Some(Laying { minutes: [min, max], .. }) = b.lays {
                if !(min > 0.0 && max >= min) {
                    return Err("ponte invalide (0 < minutes min ≤ minutes max)".into());
                }
            }
        }
        for (name, animation) in &species.model.animations {
            if !(animation.cycle > 0.0 && animation.foulee.is_none_or(|f| f > 0.0)) {
                return Err(format!("animation {name} : cycle et foulée doivent être positifs"));
            }
            if let Some(PauseRule { poids, duree: [min, max] }) = animation.pause {
                if !(poids >= 0.0 && min > 0.0 && max >= min) {
                    return Err(format!("animation {name} : pause invalide (poids positif, 0 < durée min ≤ durée max)"));
                }
            }
            let values = animation.os.values().flat_map(|m| m.rotation.into_iter().chain(m.rotation_fixe).chain(m.translation).chain(m.translation_fixe).chain([m.phase, m.frequence, m.decalage]));
            if values.into_iter().any(|v| !v.is_finite()) {
                return Err(format!("animation {name} : valeur d'os invalide"));
            }
        }
        crate::loot::check_entries(&species.loot)?;
        if species.pauses().count() > u8::MAX as usize {
            return Err("trop d'inactivités".into());
        }
        Ok(species)
    }

    /// Les inactivités de l'espèce (animations qui ont une `pause`), rangées par nom
    pub fn pauses(&self) -> impl Iterator<Item = (&str, PauseRule)> {
        self.model.animations.iter().filter_map(|(name, a)| a.pause.map(|p| (name.as_str(), p)))
    }
}

/// Position d'une créature telle que les joueurs la voient
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureState {
    pub creature_id: u64,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est, comme `tangent_frame`)
    pub yaw: f32,
    /// Coup reçu à l'instant : 1 juste après, 0 une fois passé (la bête rougit)
    pub hurt: f32,
    /// Mort : 0 vivante, puis de 0 à 1 pendant qu'elle bascule sur le flanc avant de disparaître
    pub death: f32,
    /// Ce qu'elle fait, pour choisir son animation
    pub action: CreatureAction,
    /// Taille par rapport à un adulte : un petit grandit de 0,5 à 1
    pub scale: f32,
    /// Humeur d'élevage, en bits `MOOD_*`
    pub mood: u8,
    /// Joueur qui l'a invoquée (compétence)
    pub summoner: Option<u64>,
    /// Copie d'une autre allure : celle de son invocateur (`COPY_SUMMONER`) ou celle d'une espèce
    /// (son rang dans le registre), teinte comprise
    pub copy: Option<u32>,
}

/// `CreatureState::copy` : elle a l'allure de son invocateur
pub const COPY_SUMMONER: u32 = u32::MAX;

/// Des cœurs flottent au-dessus d'elle (amoureuse, ou tout juste apprivoisée)
pub const MOOD_HEARTS: u8 = 1;
/// Apprivoisée
pub const MOOD_TAMED: u8 = 2;
/// Elle ne mange pas pour l'instant (amoureuse, ou vient d'avoir un petit)
pub const MOOD_SATED: u8 = 4;

#[cfg(test)]
mod tests {
    #[test]
    fn test_activity_follows_the_sun() {
        use super::Activity::*;
        // Plein jour, crépuscule, pleine nuit
        let awake = |a: super::Activity| [0.9, 0.0, -0.9].map(|h| a.is_awake(h));
        assert_eq!(awake(Diurnal), [true, true, false]);
        assert_eq!(awake(Nocturnal), [false, true, true]);
        assert_eq!(awake(Crepuscular), [false, true, false]);
        assert_eq!(awake(Always), [true, true, true]);
    }

    use super::*;

    #[test]
    fn a_disc_is_pushed_out_of_a_wall_across_its_nearest_side() {
        let feet = Vec3::new(0.0, 0.0, 100.0);
        let (east, north) = crate::math::tangent_frame(feet.normalize());
        let wall = Collider { shape: CollisionShape::Box, half_width: 2.0, half_length: 0.5, height: 3.0, resistance: 1.0 };
        // Cap nord : le mur est large d'est en ouest, mince du nord au sud
        let out = wall.separation(feet, 0.0, 1.0, feet + north * 0.6 + east * 1.5, 0.3, 1.8).expect("il touche le mur");
        assert!((out.dot(north) - 0.2).abs() < 1e-3 && out.dot(east).abs() < 1e-3, "{out}");
        assert!(wall.separation(feet, 0.0, 1.0, feet + north * 0.9, 0.3, 1.8).is_none(), "trop loin");
        assert!(wall.separation(feet, 0.0, 1.0, feet + feet.normalize() * 3.5 + north * 0.6, 0.3, 1.8).is_none(), "au-dessus");
        // Tourné d'un quart de tour, c'est l'inverse
        assert!(wall.separation(feet, std::f32::consts::FRAC_PI_2, 1.0, feet + north * 1.5, 0.3, 1.8).is_some());
        let disc = Collider { shape: CollisionShape::Circle, half_width: 0.5, half_length: 0.5, height: 1.0, resistance: 0.5 };
        let out = disc.separation(feet, 0.0, 1.0, feet + east * 0.6, 0.3, 1.8).unwrap();
        assert!((out.dot(east) - 0.2).abs() < 1e-3);
    }

    #[test]
    fn test_bundled_species_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR);
        let (registry, errors) = CreatureRegistry::load_dir(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        for id in ["lapin", "sanglier", "vache", "loup"] {
            let species = registry.get(registry.find(id).unwrap_or_else(|| panic!("{id} manquant"))).unwrap();
            assert!(!species.spawn.is_empty());
        }
        let activity = |id| registry.get(registry.find(id).unwrap()).unwrap().activity;
        assert_eq!(activity("vache"), Activity::Diurnal);
        assert_eq!(activity("loup"), Activity::Nocturnal);
        for species in registry.iter().filter(|s| !s.humanoid) {
            assert!(registry.asset_path(&species.model.mesh).is_file(), "{} : modèle manquant", species.id);
            assert!(registry.asset_path(&species.model.texture).is_file(), "{} : texture manquante", species.id);
        }
    }

    #[test]
    fn test_invalid_descriptions_are_explained() {
        let ok = r#"{ "name": "Cube", "body": { "half_width": 0.5, "half_length": 0.5, "height": 1 }, "walk_speed": 1,
            "spawn": [ { "biome": "Plains", "weight": 0.5 } ], "model": { "mesh": "cube.gltf", "texture": "cube.png" } }"#;
        let cube = CreatureSpecies::parse("cube", ok).unwrap();
        assert_eq!((cube.min_group, cube.max_group), (1, 1));
        assert_eq!(cube.spawn[0].chance, 1.0);
        assert!(cube.model.animations.is_empty());
        assert_eq!(CreatureAction::Pause(0).animation_name(&cube), "repos");
        assert_eq!(cube.health, 10.0);
        assert_eq!(cube.temperament, Temperament::Indifferent);
        let shy = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "temperament": "craintif","#);
        assert_eq!(CreatureSpecies::parse("cube", &shy).unwrap().temperament, Temperament::Craintif);
        let unknown = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "temperament": "farouche","#);
        assert!(CreatureSpecies::parse("cube", &unknown).unwrap_err().contains("farouche"));

        let animated = ok.replace(r#""texture": "cube.png""#, r#""texture": "cube.png", "animations": {
            "marche": { "foulee": 0.5, "os": { "patte*": { "rotation": [20, 0, 0], "phase": 0.5, "decalage": 0.1 } } },
            "sommeil": { "cycle": 4, "pause": { "poids": 1, "duree": [10, 20] } },
            "broute": { "pause": { "poids": 2, "duree": [3, 6] } } }"#);
        let cube = CreatureSpecies::parse("cube", &animated).unwrap();
        let pauses: Vec<&str> = cube.pauses().map(|(name, _)| name).collect();
        assert_eq!(pauses, ["broute", "sommeil"]);
        assert_eq!(CreatureAction::Pause(1).animation_name(&cube), "sommeil");
        assert_eq!(CreatureAction::Course.animation_name(&cube), "course");
        assert_eq!(cube.model.animations["marche"].os["patte*"].frequence, 1.0);
        let bad_pause = animated.replace("[10, 20]", "[20, 10]");
        assert!(CreatureSpecies::parse("cube", &bad_pause).unwrap_err().contains("sommeil"));
        let bad_stride = animated.replace(r#""foulee": 0.5"#, r#""foulee": 0"#);
        assert!(CreatureSpecies::parse("cube", &bad_stride).is_err());

        let typo = ok.replace("walk_speed", "walk_sped");
        assert!(CreatureSpecies::parse("cube", &typo).unwrap_err().contains("walk_sped"));
        let flat = ok.replace(r#""height": 1"#, r#""height": 0"#);
        assert!(CreatureSpecies::parse("cube", &flat).is_err());
        let groups = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "min_group": 3, "max_group": 2,"#);
        assert!(CreatureSpecies::parse("cube", &groups).is_err());
        assert!(cube.attack.is_none(), "paisible par défaut");

        let wolf = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "attack": { "mode": "pack", "damage": 4, "prey": ["lapin"] },"#);
        let attack = CreatureSpecies::parse("cube", &wolf).unwrap().attack.unwrap();
        assert_eq!((attack.mode, attack.pack_size, attack.hunts_players, attack.provoked_only), (AttackMode::Pack, 3, true, false));
        assert_eq!(attack.prey, ["lapin"]);
        let unknown = wolf.replace("pack", "horde");
        assert!(CreatureSpecies::parse("cube", &unknown).is_err());
        let harmless = wolf.replace(r#""damage": 4"#, r#""damage": 0"#);
        assert!(CreatureSpecies::parse("cube", &harmless).is_err());
    }
}

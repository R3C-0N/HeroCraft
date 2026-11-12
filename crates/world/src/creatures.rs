//! Créatures : chaque zone de la planète a sa population, tirée de la graine du monde la première
//! fois qu'un joueur s'en approche, puis plus rien n'apparaît. Les bêtes s'y promènent au hasard,
//! s'endorment (ne sont plus simulées, mais gardées telles quelles) quand plus aucun joueur n'est
//! près, et se réveillent où elles étaient quand un joueur revient. Une bête tuée ne revient pas.
//! Voir `population` pour les zones et leur enregistrement.
//!
//! La même simulation tourne sur le serveur, qui fait autorité et diffuse les positions, et dans la
//! partie solo du client. Elle lit le terrain bloc par bloc (générateur + modifications) : une
//! créature marche sur les blocs posés, tombe dans les trous creusés, regarde quelques pas devant
//! elle pour contourner murs et falaises plutôt que de faire demi-tour, préfère la terre ferme et,
//! à l'eau, nage vers la rive la plus proche.
//! Corps, allure et biomes de chaque espèce viennent de sa description (`CreatureRegistry`).
//!
//! Une créature se tourne peu à peu vers son nouveau cap, saute pour monter sur un bloc, et, frappée
//! par un joueur, rougit, recule, puis s'enfuit ; sans vie, elle bascule sur le flanc et disparaît.
//!
//! Le tempérament de l'espèce règle sa réaction au joueur le plus proche : une bête craintive fuit
//! dès qu'il approche (de moins loin s'il est accroupi), une placide s'arrête pour le regarder, une curieuse le suit à distance.
//!
//! Chaque espèce vit à son rythme (`Activity`) : selon la hauteur du soleil là où elle se trouve, elle
//! est éveillée ou s'endort sur place, une bête après l'autre, jusqu'à ce que son heure revienne ou
//! qu'un coup la réveille.
//! Les bêtes apparues ensemble forment un troupeau : chacune revient vers ses voisines quand elle
//! s'en écarte et prend volontiers le cap de celles qui marchent. Quand l'une est frappée, les autres
//! s'affolent aussi, mais moins : elles courent moins vite et se calment plus tôt que la victime.
//!
//! Une bête dont la fiche décrit une attaque (`attack`) chasse : elle repère les joueurs et ses
//! proies à portée de vue, les poursuit et les frappe. Seule, elle mord dès qu'elle est au contact ;
//! en meute, elle n'ose qu'entourée de ses semblables, qui encerclent la cible avec elle ; à la
//! charge, elle vise, fonce en ligne droite, puis souffle. Frappée, elle riposte au lieu de fuir
//! (sauf une meute trop maigre). Les coups portés aux joueurs sont renvoyés par `tick`, qui laisse
//! au serveur ou à la partie solo le soin de les retirer des jauges de vie.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::f32::consts::{PI, TAU};

use glam::Vec3;
use herocraft_common::skills::Status;
use herocraft_common::{armor_factor, blocks, items, tangent_frame, AttackMode, DamageKind, BlockId, ItemId, MOOD_HEARTS, MOOD_SATED, MOOD_TAMED, CreatureAction, CreatureShotEvent, Collider, PLAYER_COLLIDER, CreatureRegistry, CreatureSpecies, CreatureState, Temperament, PlanetFace, CORPSE_TIME, CREATURE_DEATH_TIME, CREATURE_HURT_TIME, DEFAULT_PAUSE, PLAYER_HIT_DAMAGE};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::edits::{BlockGeometry, BlockPos, TerrainEdits, TerrainQuery, WORLD_HEIGHT};
use crate::generator::{BiomeType, ColumnSample, PlanetWorldGenerator};
use crate::population::{PopulationSave, SleepingCreature, ZoneKey, ZONE_BLOCKS};

/// Distance à laquelle le serveur montre une créature à un joueur, en blocs. Les zones dont le
/// centre est à moins de cette distance (plus une demi-diagonale de zone) sont éveillées.
pub const CREATURE_RANGE_BLOCKS: f32 = 96.0;
/// Demi-diagonale d'une zone, en blocs : une zone s'éveille dès qu'une partie peut être à portée
const ZONE_HALF_DIAGONAL: f32 = ZONE_BLOCKS as f32 * std::f32::consts::FRAC_1_SQRT_2;
/// Zones par côté d'un groupe de zones (recherche des zones proches des joueurs)
const ZONE_GROUP: i32 = 8;
/// Demi-diagonale d'un groupe de zones, en blocs
const ZONE_GROUP_HALF_DIAGONAL: f32 = (ZONE_GROUP * ZONE_BLOCKS) as f32 * std::f32::consts::FRAC_1_SQRT_2;
/// Marge avant qu'une zone éveillée ne se rendorme, en blocs : un joueur qui fait les cent pas à la
/// limite ne la réveille et ne l'endort pas sans cesse
const SLEEP_MARGIN_BLOCKS: f32 = 24.0;
/// Chance qu'une zone reçoive un groupe à sa création (l'espèce tirée a encore sa propre chance)
const ZONE_GROUP_CHANCE: f64 = 0.25;
/// Écart des bêtes d'un groupe autour de son centre, en blocs
const GROUP_SPREAD: i32 = 3;
/// Premier identifiant des bêtes posées à la main : celles des zones sont numérotées par zone en dessous
const MANUAL_ID_BASE: u64 = 1 << 48;
/// Nom du fichier de la population dans le dossier de sauvegarde d'un monde
pub const POPULATION_FILE: &str = "creatures.json";
/// Chute, en blocs par seconde²
const GRAVITY: f32 = 20.0;
/// Hauteur de marche franchie d'un saut, en blocs
const STEP_BLOCKS: f32 = 1.0;
/// Élan d'un saut, en blocs par seconde : de quoi monter un peu plus haut qu'une marche
const JUMP_SPEED: f32 = 7.2;
/// Vitesse à laquelle une créature se tourne, en radians par seconde
const TURN_SPEED: f32 = 4.0;
/// Écart au cap visé sous lequel elle le considère atteint, en radians
const FACING_TOLERANCE: f32 = 0.1;
/// Recul d'un coup : vitesse dans le plan, en blocs par seconde, et petit bond
const KNOCKBACK_SPEED: f32 = 6.0;
const KNOCKBACK_HOP: f32 = 4.5;
/// Freinage du recul, par seconde
const KNOCKBACK_DAMPING: f32 = 5.0;
/// Vitesse d'une bête rabattue au sol par une compétence (`slam`), en blocs par seconde
const SLAM_SPEED: f32 = 40.0;
/// Chute sans dommage d'une bête projetée en l'air (`launch`), en blocs, et dégâts par bloc au-delà
const LAUNCH_SAFE_FALL: f32 = 3.0;
const LAUNCH_FALL_DAMAGE: f32 = 3.0;
/// Fuite après un coup : durée, en secondes, et allure par rapport à la marche
const PANIC_TIME: f32 = 4.0;
const PANIC_SPEED: f32 = 1.8;
/// Une bête craintive fuit un joueur plus près que ça, en blocs, pendant `FLEE_TIME` secondes avant
/// de jeter un nouveau coup d'œil
const FLEE_RADIUS_BLOCKS: f32 = 8.0;
/// Rayon de fuite face à un joueur accroupi, qui s'approche sans bruit, en blocs
const FLEE_RADIUS_CROUCHING_BLOCKS: f32 = 3.0;
const FLEE_TIME: f32 = 1.5;
/// Une bête placide regarde un joueur plus près que ça, en blocs
const WATCH_RADIUS_BLOCKS: f32 = 6.0;
/// Une bête curieuse suit un joueur plus près que `FOLLOW_RADIUS_BLOCKS` et s'arrête à
/// `FOLLOW_STOP_BLOCKS` de lui ; elle repart quand il s'éloigne au-delà de `FOLLOW_RESUME_BLOCKS`
const FOLLOW_RADIUS_BLOCKS: f32 = 16.0;
const FOLLOW_STOP_BLOCKS: f32 = 3.0;
const FOLLOW_RESUME_BLOCKS: f32 = 4.5;
/// Intervalle entre deux réveils possibles d'une créature endormie, en secondes : les bêtes d'un
/// troupeau ne se lèvent pas toutes ensemble
const SLEEP_CHECK: (f32, f32) = (3.0, 12.0);
/// Fuite des voisines d'une bête frappée, qui n'ont pas reçu le coup : plus courte et moins rapide
const ALARM_TIME: f32 = 1.8;
const ALARM_SPEED: f32 = 1.35;
/// Portée de l'alerte autour de la bête frappée, en blocs
const ALARM_RADIUS_BLOCKS: f32 = 16.0;
/// Voisines du troupeau prises en compte, en blocs
const HERD_RADIUS_BLOCKS: f32 = 20.0;
/// Écart au centre du troupeau au-delà duquel une bête y revient plutôt que d'errer, en blocs
const HERD_LOOSE_BLOCKS: f32 = 4.0;
/// Écart au-delà duquel elle ne s'arrête plus avant d'avoir rejoint les autres, en blocs
const HERD_STRAY_BLOCKS: f32 = 7.0;
/// Chance qu'une bête au repos emboîte le pas de voisines qui marchent, à chaque changement d'humeur
const HERD_FOLLOW_CHANCE: f64 = 0.6;
/// Rayon dans lequel les bêtes d'une même espèce se comptent comme une meute, en blocs
const PACK_RADIUS: f32 = 16.0;
/// Colère d'une bête agressive frappée par un joueur, en secondes
const ANGER_TIME: f32 = 20.0;
/// Temps pendant lequel les gardes s'en prennent au joueur qui a tué un habitant, en secondes
const WANTED_TIME: f32 = 180.0;
/// Les gardes à moins de tant de blocs d'un habitant tué recherchent son meurtrier
const WANTED_RADIUS_BLOCKS: f32 = 48.0;
/// Temps pendant lequel une bête qui a lâché sa cible au bout de sa laisse ne cherche plus querelle
/// (elle regagne son poste), en secondes
const CALM_TIME: f32 = 8.0;
/// Une cible prise en chasse (ou signalée par la meute) se repère de plus loin, d'autant
const PURSUIT_FACTOR: f32 = 1.6;
/// Répit d'un prédateur qui vient de tuer une proie, en secondes
const SATED_TIME: f32 = 60.0;
/// Charge : visée, course au plus, souffle après, en secondes
const CHARGE_AIM: f32 = 0.7;
const CHARGE_RUN: f32 = 2.5;
const CHARGE_REST: f32 = 2.0;
/// Encerclement d'une meute : plus loin que ça de la cible, chaque bête vise un point à côté
/// d'elle, à cette distance, en blocs
const FLANK_DISTANCE: f32 = 5.0;
const FLANK_OFFSET: f32 = 2.5;
/// Demi-largeur d'une cible, ajoutée à l'allonge, en blocs
const TARGET_RADIUS: f32 = 0.3;
/// Distance des entités qu'un piège fait apparaître autour de lui, en blocs
const SPAWN_RING: f32 = 0.5;
/// Écart de hauteur au-delà duquel un coup ne porte pas, en blocs
const MAX_STRIKE_HEIGHT: f32 = 1.8;
/// Écart au cap de la cible au-delà duquel elle se tourne avant de frapper, en radians
const STRIKE_ANGLE: f32 = 0.6;
/// Durée de l'animation d'un coup porté, si la fiche n'a pas d'animation `attaque`, en secondes
const STRIKE_ANIMATION: f32 = 0.6;
/// Bond d'un coup : élan vers la cible, en blocs par seconde, et petit saut
const LUNGE_SPEED: f32 = 3.0;
const LUNGE_HOP: f32 = 3.0;
/// Une créature ne descend pas volontairement plus bas que ça, en blocs : au-delà, c'est une falaise
const MAX_DROP_BLOCKS: f32 = 3.0;
/// Regard devant elle : nombre de points sondés sur son chemin, et écart entre eux, en blocs
const LOOKAHEAD_STEPS: usize = 5;
const PROBE_SPACING: f32 = 0.5;
/// Écarts au cap essayés pour contourner un obstacle, en radians, du plus petit au plus grand
const DETOUR_ANGLES: [f32; 6] = [0.4, 0.8, 1.2, 1.6, 2.1, 2.6];
/// Nage : allure par rapport à la marche, et part du corps sous la surface (au plus un bloc)
const SWIM_SPEED: f32 = 0.6;
const SWIM_SINK: f32 = 0.5;
/// Recherche de la rive : portée, en blocs, directions sondées et intervalle, en secondes
const SHORE_RANGE_BLOCKS: usize = 12;
const SHORE_DIRECTIONS: usize = 16;
const SHORE_SEARCH_INTERVAL: f32 = 1.0;
/// Distance de son but à laquelle une créature menée s'arrête, en blocs
const GUIDE_ARRIVAL_BLOCKS: f32 = 0.6;
/// Temps entre deux visées d'une créature menée vers son but, en secondes (entre-temps, elle
/// contourne ce qui la gêne)
const GUIDE_AIM_INTERVAL: f32 = 0.3;
/// Distance à laquelle une créature remarque une bête qu'elle craint, en blocs
const FEAR_RADIUS_BLOCKS: f32 = 10.0;
/// Durée d'une fuite devant une bête crainte, renouvelée tant qu'elle est près
const FEAR_TIME: f32 = 2.5;
/// Premier identifiant des créatures menées par la simulation des villages
const MANAGED_ID_BASE: u64 = 1 << 56;
/// Pas de simulation le plus long, en secondes
const MAX_STEP: f32 = 0.1;
/// Une bête suit le joueur qui tient sa nourriture à moins de tant de blocs, jusqu'à tant de blocs de lui
const LURE_RADIUS_BLOCKS: f32 = 10.0;
const LURE_STOP_BLOCKS: f32 = 2.0;
/// Durée pendant laquelle une bête nourrie cherche un partenaire, en secondes
const LOVE_TIME: f32 = 30.0;
/// Délai avant qu'une bête qui vient d'avoir un petit puisse de nouveau aimer, en secondes
const BREED_COOLDOWN: f32 = 300.0;
/// Une bête amoureuse va vers le partenaire le plus proche à moins de tant de blocs ; à moins de
/// `MATE_DISTANCE_BLOCKS`, le petit naît
const MATE_RADIUS_BLOCKS: f32 = 8.0;
const MATE_DISTANCE_BLOCKS: f32 = 1.3;
/// Durée des cœurs d'une bête tout juste apprivoisée, en secondes
const HEARTS_TIME: f32 = 3.0;
/// Part de sa croissance qu'un petit gagne à chaque repas
const MEAL_GROWTH: f32 = 0.1;
/// Taille d'un nouveau-né par rapport à un adulte
const NEWBORN_SCALE: f32 = 0.5;
/// Au-delà, le cache de colonnes est vidé
const MAX_CACHED_COLUMNS: usize = 4096;

/// Le vol d'une bête projetée par une compétence
#[derive(Debug, Clone, Copy)]
struct Flight {
    /// Altitude la plus haute atteinte, en blocs
    peak: f32,
    /// Rabattue : par qui, et combien de dégâts à l'atterrissage
    slam: Option<(u64, f32)>,
}

pub struct Creature {
    pub id: u64,
    /// Rang de son espèce dans le registre
    pub species: usize,
    /// Troupeau : les bêtes apparues ensemble partagent ce numéro (celui de la première)
    pub herd: u64,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est)
    pub yaw: f32,
    /// Cap vers lequel elle se tourne
    target_yaw: f32,
    /// Vitesse verticale, en blocs par seconde (positive vers le haut)
    vertical_speed: f32,
    /// Recul d'un coup reçu, dans le plan tangent, en blocs par seconde
    push: Vec3,
    pub(crate) walking: bool,
    /// Inactivité choisie à l'arrêt (rang dans `CreatureSpecies::pauses`)
    pub(crate) pause: Option<u8>,
    /// Temps restant avant de changer d'humeur (s'arrêter, repartir), en secondes
    timer: f32,
    pub(crate) health: f32,
    /// Temps restant de la teinte rouge d'un coup, en secondes
    hurt: f32,
    /// Temps restant de fuite après un coup, en secondes
    pub(crate) panic: f32,
    /// Allure de la fuite par rapport à la marche : plus vive pour la bête frappée que pour ses voisines
    panic_speed: f32,
    /// Temps écoulé depuis sa mort (`None` : vivante), en secondes
    dead_for: Option<f32>,
    /// Endormie : elle ne bouge plus jusqu'au retour de son heure, ou jusqu'à un coup
    pub(crate) sleeping: bool,
    /// Côté par lequel elle contourne les obstacles (1 : à droite, -1 : à gauche), gardé d'un
    /// obstacle à l'autre pour longer un mur sans hésiter
    detour_side: f32,
    /// Elle nage : ses pieds sont à la surface de l'eau
    swimming: bool,
    /// Enfoncement du corps dans l'eau, en unités du monde
    sink: f32,
    /// Elle poursuit une cible : la promenade est suspendue
    pub(crate) hunting: bool,
    /// Allure de la poursuite, par rapport à la marche
    pace: f32,
    /// Temps restant de colère contre les joueurs, après un coup, en secondes
    anger: f32,
    /// Temps restant avant de pouvoir frapper de nouveau, en secondes
    attack_cooldown: f32,
    /// Temps restant de l'animation du coup qu'elle vient de porter, en secondes
    striking: f32,
    /// Temps restant de répit après avoir tué une proie, en secondes
    sated: f32,
    charge: Charge,
    /// Menée par la simulation des villages (`villagers`) : ni rangée dans une zone, ni enregistrée
    pub(crate) managed: bool,
    /// Rentrée chez elle : ni simulée, ni montrée aux joueurs
    pub(crate) hidden: bool,
    /// Où elle se rend (direction depuis le centre de la planète), en marchant à l'allure `guide_pace`
    pub(crate) guide: Option<Vec3>,
    pub(crate) guide_pace: f32,
    /// Arrivée au bout de son dernier trajet
    pub(crate) arrived: bool,
    /// Point qu'elle regarde, arrêtée (interlocuteur, établi, comptoir)
    pub(crate) look: Option<Vec3>,
    /// Apprivoisée par un joueur
    pub tamed: bool,
    /// Petit : secondes depuis sa naissance (`None` : adulte)
    age: Option<f32>,
    /// Temps qu'il faut à un petit de son espèce pour grandir, en secondes
    growth_seconds: f32,
    /// Temps restant avant sa prochaine ponte, en secondes (apprivoisée et adulte seulement)
    lay_in: f32,
    /// Temps restant amoureuse (elle cherche un partenaire)
    love: f32,
    /// Temps restant avant de pouvoir de nouveau aimer, après un petit
    breed_cooldown: f32,
    /// Temps restant des cœurs d'une bête tout juste apprivoisée
    hearts: f32,
    /// Partenaire vers qui elle marche
    mate: Option<Vec3>,
    /// Cible qu'elle poursuit, et d'où la poursuite est partie (bête tenue en laisse, `leash`)
    quarry: Option<Target>,
    chase_from: Option<Vec3>,
    /// Temps restant avant qu'une bête revenue de sa poursuite ne cherche de nouveau querelle
    calm: f32,
    /// Ralentie par une compétence : vitesse retirée (0 à 1) et temps restant, en secondes
    slow: f32,
    slow_time: f32,
    /// Étourdie par une compétence : temps restant sans bouger ni frapper, en secondes
    stun: f32,
    /// Projetée en l'air ou rabattue par une compétence : son vol, jusqu'à ce qu'elle retombe
    flight: Option<Flight>,
    /// Elle vient de retomber d'un vol : hauteur de la chute en blocs, et le coup qui l'a rabattue
    landed: Option<(f32, Option<(u64, f32)>)>,
    /// Garde brisée (Brise-Garde) : temps restant sans se garder, en secondes
    guard_broken: f32,
    /// Pressée par une compétence (Surcharge Mécanique) : cadence en plus (0,5 : +50 %) et temps restant
    haste: f32,
    haste_time: f32,
    /// Bouclier d'une compétence : points de dégâts qu'il absorbe encore, et temps restant
    shield: f32,
    shield_time: f32,
    /// Aveuglée : temps restant à ne voir qu'à `BLIND_SIGHT` blocs, en secondes
    blind: f32,
    /// Armure brisée par des compétences : part retirée et temps restant, en secondes, par bris
    shreds: Vec<(f32, f32)>,
    /// Provoquée : clé du joueur qu'elle poursuit, et temps restant, en secondes
    taunted: Option<(u64, f32)>,
    /// Invoquée par ce joueur (compétence) : alliée, jamais gardée endormie avec sa zone
    pub summoner: Option<u64>,
    /// Apparition d'une entité invoquée : temps restant et durée totale, en secondes ; elle ne
    /// bouge ni ne frappe avant d'être en place
    appearing: f32,
    appear_total: f32,
    /// Piège déclenché : secondes avant qu'il disparaisse
    vanish: Option<f32>,
    /// Saignement : points de vie perdus par seconde, secondes restantes, temps avant la prochaine perte
    bleed: f32,
    bleed_left: f32,
    bleed_next: f32,
    /// Allure copiée (`CreatureState::copy`)
    pub copy: Option<u32>,
}

/// Un joueur a tué un habitant ou un voyageur : de quoi l'annoncer
#[derive(Debug, Clone, PartialEq)]
pub struct Murder {
    /// Clé du joueur (celle de `hit_by`)
    pub player: u64,
    /// Nom et métier de la victime
    pub victim: String,
    pub profession: String,
    /// Ville ou village où c'est arrivé (`None` : sur les routes)
    pub town: Option<String>,
}

impl Murder {
    /// Phrase du chat : « Mat a tué Aude (Fermière) à Brumeval : les gardes sont à ses trousses ! »
    pub fn announce(&self, player: &str) -> String {
        let victim = if self.profession.is_empty() { self.victim.clone() } else { format!("{} ({})", self.victim, self.profession) };
        let place = self.town.as_ref().map_or_else(|| "sur les routes".to_string(), |t| format!("à {t}"));
        format!("{player} a tué {victim} {place} : les gardes sont à ses trousses !")
    }
}

/// Un joueur a frappé (sans le tuer) un habitant ou un voyageur : de quoi l'avertir, lui seul, que
/// les gardes vont s'en prendre à lui
#[derive(Debug, Clone, PartialEq)]
pub struct Assault {
    /// Clé du joueur (celle de `hit_by`)
    pub player: u64,
    pub victim: String,
    pub profession: String,
    pub town: Option<String>,
}

impl Assault {
    /// Phrase du chat, pour l'auteur du coup : « Tu as frappé Aude (Fermière) à Brumeval : les
    /// gardes s'en prennent à toi pendant 20 secondes ! »
    pub fn warning(&self) -> String {
        let victim = if self.profession.is_empty() { self.victim.clone() } else { format!("{} ({})", self.victim, self.profession) };
        match &self.town {
            Some(town) => format!("Tu as frappé {victim} à {town} : les gardes s'en prennent à toi pendant {:.0} secondes !", ANGER_TIME),
            None => format!("Tu as frappé {victim} sur les routes : gare à sa riposte !"),
        }
    }
}

/// Ce que fait un repas donné à une bête
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feeding {
    /// Elle n'en veut pas (pas sa nourriture, déjà amoureuse ou trop tôt après un petit) : l'objet
    /// n'est pas dépensé
    Refused,
    /// Sauvage : elle a mangé sans s'apprivoiser cette fois
    Taming,
    /// Elle vient d'être apprivoisée
    Tamed,
    /// Apprivoisée et adulte : elle cherche un partenaire
    InLove,
    /// Petit : il grandit plus vite
    Grew,
}

/// Ce qu'on dit au joueur après un repas donné à `name` (nom de l'espèce), s'il y a lieu
pub fn feeding_message(feeding: Feeding, name: &str) -> Option<String> {
    match feeding {
        Feeding::Tamed => Some(format!("Apprivoisé : {name}")),
        Feeding::InLove => Some(format!("{name} cherche un partenaire")),
        _ => None,
    }
}

/// Étape d'une charge
#[derive(Debug, Clone, Copy, PartialEq)]
enum Charge {
    Idle,
    /// Arrêtée, elle vise (temps restant)
    Aiming(f32),
    /// Elle fonce droit devant (temps restant)
    Running(f32),
    /// Elle souffle (temps restant)
    Resting(f32),
}

/// Coup porté à un joueur par une créature : à retirer de sa vie
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerHit {
    /// Rang du joueur dans la liste passée à `CreatureWorld::tick`
    pub player: usize,
    pub creature_id: u64,
    /// Rang de l'espèce de la créature dans le registre
    pub species: usize,
    pub damage: f32,
    /// Type du coup (`CreatureAttack::kind`)
    pub kind: DamageKind,
}

/// Un projectile en vol
#[derive(Debug, Clone)]
struct Flying {
    shooter: u64,
    /// Bouche du tir, et demi-largeur de ce qu'il traverse (0 : il s'arrête sur sa cible)
    from: Vec3,
    pierce: f32,
    /// Le tireur est une entité invoquée : il ne touche que les bêtes hostiles
    allied: bool,
    species: usize,
    target: Target,
    to: Vec3,
    damage: f32,
    kind: DamageKind,
    /// Rayon de l'explosion, en unités du monde
    radius: f32,
    left: f32,
}

/// Ce qu'une bête chasse
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    /// Rang du joueur
    Player(usize),
    /// Identifiant de la proie
    Creature(u64),
}

impl Creature {
    /// Pressée par une compétence en ce moment (Surcharge Mécanique)
    pub fn is_hasted(&self) -> bool {
        self.haste > 0.0
    }

    /// Protégée par le bouclier d'une compétence en ce moment
    pub fn has_shield(&self) -> bool {
        self.shield > 0.0
    }

    pub fn state(&self) -> CreatureState {
        CreatureState {
            creature_id: self.id,
            // Elle nage le corps à moitié sous l'eau
            position: self.position - self.position.normalize_or_zero() * self.sink,
            yaw: self.yaw,
            hurt: (self.hurt / CREATURE_HURT_TIME).clamp(0.0, 1.0),
            death: self.dead_for.map_or(0.0, |t| (t / CREATURE_DEATH_TIME).clamp(1e-3, 1.0)),
            action: self.action(),
            scale: self.scale(),
            mood: self.mood(),
            summoner: self.summoner,
            copy: self.copy,
        }
    }

    /// Taille par rapport à un adulte : un petit grandit de `NEWBORN_SCALE` à 1
    pub fn scale(&self) -> f32 {
        self.age.map_or(1.0, |age| NEWBORN_SCALE + (1.0 - NEWBORN_SCALE) * (age / self.growth_seconds).clamp(0.0, 1.0))
    }

    /// Humeur d'élevage (bits `MOOD_*`)
    pub fn mood(&self) -> u8 {
        let mut mood = 0;
        if self.is_alive() && (self.love > 0.0 || self.hearts > 0.0) {
            mood |= MOOD_HEARTS;
        }
        if self.tamed {
            mood |= MOOD_TAMED;
        }
        if self.love > 0.0 || self.breed_cooldown > 0.0 {
            mood |= MOOD_SATED;
        }
        mood
    }

    /// Petit qui n'a pas encore grandi
    pub fn is_young(&self) -> bool {
        self.age.is_some()
    }

    /// Ce qu'elle fait, pour choisir son animation
    pub fn action(&self) -> CreatureAction {
        if self.dead_for.is_some() {
            CreatureAction::Repos
        } else if self.appearing > 0.0 {
            let done = 1.0 - self.appearing / self.appear_total.max(1e-3);
            CreatureAction::Apparition((done.clamp(0.0, 1.0) * 255.0) as u8)
        } else if self.striking > 0.0 {
            CreatureAction::Attaque
        } else if self.panic > 0.0 || (self.hunting && self.walking && self.pace > 1.0) {
            // Fuite, poursuite ou charge
            CreatureAction::Course
        } else if self.walking {
            CreatureAction::Marche
        } else if self.hunting {
            // À l'arrêt pendant la chasse (elle vise, souffle ou se tourne vers sa cible)
            CreatureAction::Repos
        } else {
            self.pause.map_or(CreatureAction::Repos, CreatureAction::Pause)
        }
    }

    /// S'arrête et choisit au poids une inactivité de sa fiche (brouter, dormir…), qu'elle tient
    /// le temps que la fiche lui donne
    fn start_pause(&mut self, species: &CreatureSpecies, rng: &mut StdRng) {
        self.walking = false;
        let pauses: Vec<_> = species.pauses().map(|(_, rule)| rule).collect();
        let total: f32 = pauses.iter().map(|p| p.poids).sum();
        let (pause, rule) = if total > 0.0 {
            let mut roll = rng.gen_range(0.0..total);
            let index = pauses.iter().position(|p| {
                roll -= p.poids;
                roll < 0.0
            });
            let index = index.unwrap_or(pauses.len() - 1);
            (Some(index as u8), pauses[index])
        } else {
            (None, DEFAULT_PAUSE)
        };
        self.pause = pause;
        let [min, max] = rule.duree;
        self.timer = if max > min { rng.gen_range(min..max) } else { min };
    }

    pub fn is_alive(&self) -> bool {
        self.dead_for.is_none()
    }

    /// Elle poursuit une cible
    pub fn is_hunting(&self) -> bool {
        self.hunting
    }

    /// Lâche sa cible et reprend sa promenade un peu plus tard
    fn stop_hunting(&mut self, rng: &mut StdRng) {
        if self.hunting {
            self.hunting = false;
            self.walking = false;
            self.timer = rng.gen_range(1.0..3.0);
        }
        self.pace = 1.0;
        self.charge = Charge::Idle;
    }

    pub fn is_sleeping(&self) -> bool {
        self.sleeping
    }

    /// Rentrée chez elle : à ne pas montrer
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

/// Ce que voit encore une bête aveuglée, en blocs
pub const BLIND_SIGHT: f32 = 1.5;

/// Un joueur tel que les créatures le perçoivent
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPresence {
    /// Pieds (repère du centre de la planète)
    pub feet: Vec3,
    /// Accroupi : il approche sans bruit, les bêtes craintives le remarquent de moins loin
    pub crouching: bool,
    /// Ce qu'il tient en main : sa nourriture attire une bête qui s'élève
    pub holding: Option<ItemId>,
    /// Invisible (compétence) : les bêtes ne le voient ni ne le sentent
    pub hidden: bool,
}

impl PlayerPresence {
    pub fn standing(feet: Vec3) -> Self {
        Self { feet, crouching: false, holding: None, hidden: false }
    }
}

/// Espèce candidate dans un biome : rang, poids, chance d'apparaître vraiment
type SpawnCandidate = (usize, f32, f32);

pub struct CreatureWorld {
    registry: Arc<CreatureRegistry>,
    /// Espèces qui peuplent chaque biome
    spawn_table: HashMap<BiomeType, Vec<SpawnCandidate>>,
    /// Rangs des espèces que chaque espèce chasse
    prey_table: Vec<Vec<usize>>,
    /// Rangs des espèces que chaque espèce fuit
    fear_table: Vec<Vec<usize>>,
    /// Habitants des villes et des villages, voyageurs (voir `villagers`)
    pub(crate) villages: crate::villagers::Villages,
    /// Prochain identifiant d'une créature menée par les villages
    next_managed_id: u64,
    /// Ce que mange chaque espèce qui s'élève (vide : elle ne s'élève pas)
    food_table: Vec<Vec<ItemId>>,
    /// Ce que pond chaque espèce, et le temps entre deux pontes en minutes
    lay_table: Vec<Option<(ItemId, [f32; 2])>>,
    /// Œufs pondus pas encore ramassés par `take_laid`
    laid: Vec<(Vec3, ItemId)>,
    /// Joueurs recherchés par les gardes : clé du joueur, troupeau des gardes, temps restant
    wanted: Vec<(u64, u64, f32)>,
    /// Meurtres pas encore annoncés (voir `take_murders`)
    murders: Vec<Murder>,
    /// Tirs partis, pas encore montrés (voir `take_shots`)
    shots: Vec<CreatureShotEvent>,
    /// Projectiles en vol : ils touchent à leur arrivée
    flying: Vec<Flying>,
    /// Entités qu'un piège a fait apparaître, pas encore remises aux compétences (`take_spawned`)
    pub(crate) spawned: Vec<(u64, u64, f32)>,
    /// Coups portés aux habitants, pas encore signalés à leur auteur (voir `take_assaults`)
    assaults: Vec<Assault>,
    geometry: BlockGeometry,
    /// Graine du monde : elle décide de la population de chaque zone
    seed: u64,
    /// Bêtes des zones éveillées, simulées
    creatures: Vec<Creature>,
    /// Zones éveillées : peuplées, leurs bêtes sont dans `creatures`
    awake: HashSet<ZoneKey>,
    /// Zones peuplées endormies, avec leurs bêtes. Une zone ni éveillée ni ici n'a encore jamais été
    /// peuplée.
    sleeping: HashMap<ZoneKey, Vec<SleepingCreature>>,
    /// Direction du centre de chaque zone depuis le centre de la planète, par groupes de
    /// `ZONE_GROUP` × `ZONE_GROUP` zones (direction du centre du groupe, zones du groupe) : la
    /// recherche des zones proches d'un joueur écarte d'un coup les groupes lointains
    zone_groups: Vec<(Vec3, Vec<(ZoneKey, Vec3)>)>,
    /// Prochain identifiant d'une bête posée à la main
    next_id: u64,
    rng: StdRng,
    /// Direction du soleil (repère du centre de la planète), d'où l'heure en chaque lieu. `None` :
    /// pas d'horloge, toutes les espèces restent éveillées.
    sun_dir: Option<Vec3>,
    /// Météo et temps du ciel : sous l'orage les bêtes s'abritent, l'hiver les ours hibernent.
    /// `None` : pas de météo.
    climate: Option<(crate::weather::WeatherModel, f64)>,
    /// Faux : les zones restent vides (tests d'une bête seule)
    populate: bool,
    /// Fichier où la population est enregistrée, ou `None` pour ne rien enregistrer
    save_path: Option<PathBuf>,
    /// Colonnes du générateur déjà échantillonnées : les créatures restent longtemps sur les mêmes
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl CreatureWorld {
    /// Monde sans sauvegarde : chaque zone est peuplée d'après `seed`, la graine de la planète
    pub fn new(base_radius: f64, seed: u64, registry: Arc<CreatureRegistry>) -> Self {
        let mut spawn_table: HashMap<BiomeType, Vec<SpawnCandidate>> = HashMap::new();
        for (index, species) in registry.iter().enumerate() {
            for rule in &species.spawn {
                match BiomeType::from_name(&rule.biome) {
                    Some(biome) if rule.weight > 0.0 => spawn_table.entry(biome).or_default().push((index, rule.weight, rule.chance)),
                    Some(_) => {}
                    None => log::warn!("Créature {} : biome « {} » inconnu.", species.id, rule.biome),
                }
            }
        }
        let resolve = |species: &CreatureSpecies, ids: &[String], what: &str| -> Vec<usize> {
            ids.iter()
                .filter_map(|id| {
                    let found = registry.find(id);
                    if found.is_none() {
                        log::warn!("Créature {} : {what} « {id} » inconnue.", species.id);
                    }
                    found
                })
                .collect()
        };
        let prey_table = registry.iter().map(|species| resolve(species, species.attack.as_ref().map_or(&[][..], |a| &a.prey[..]), "proie")).collect();
        let fear_table = registry.iter().map(|species| resolve(species, &species.afraid_of, "bête crainte")).collect();
        let villages = crate::villagers::Villages::new(seed, &registry);
        let food_table = registry
            .iter()
            .map(|species| {
                let food = species.breeding.as_ref().map_or(&[][..], |b| &b.food[..]);
                food.iter()
                    .filter_map(|id| {
                        let found = items().find(id).or_else(|| blocks().find(id).map(ItemId::from));
                        if found.is_none() {
                            log::warn!("Créature {} : nourriture « {id} » inconnue.", species.id);
                        }
                        found
                    })
                    .collect()
            })
            .collect();
        let lay_table = registry
            .iter()
            .map(|species| {
                let lays = species.breeding.as_ref()?.lays.as_ref()?;
                let item = items().find(&lays.item).or_else(|| blocks().find(&lays.item).map(ItemId::from));
                if item.is_none() {
                    log::warn!("Créature {} : ponte « {} » inconnue.", species.id, lays.item);
                }
                Some((item?, lays.minutes))
            })
            .collect();
        let geometry = BlockGeometry::new(base_radius);
        let half = ZONE_BLOCKS as f64 / 2.0;
        let mut groups: HashMap<(PlanetFace, i32, i32), Vec<(ZoneKey, Vec3)>> = HashMap::new();
        for zone in ZoneKey::all() {
            let (x, z) = ((zone.x * ZONE_BLOCKS) as f64 + half, (zone.z * ZONE_BLOCKS) as f64 + half);
            let center = geometry.corner(zone.face, x, 0.0, z).normalize().as_vec3();
            groups.entry((zone.face, zone.x / ZONE_GROUP, zone.z / ZONE_GROUP)).or_default().push((zone, center));
        }
        let group_half = (ZONE_GROUP * ZONE_BLOCKS) as f64 / 2.0;
        let mut zone_groups: Vec<(Vec3, Vec<(ZoneKey, Vec3)>)> = groups
            .into_iter()
            .map(|((face, gx, gz), zones)| {
                let (x, z) = ((gx * ZONE_GROUP * ZONE_BLOCKS) as f64 + group_half, (gz * ZONE_GROUP * ZONE_BLOCKS) as f64 + group_half);
                (geometry.corner(face, x, 0.0, z).normalize().as_vec3(), zones)
            })
            .collect();
        // Ordre fixe (celui de `ZoneKey::all`) : les zones s'éveillent toujours dans le même ordre
        zone_groups.sort_by_key(|(_, zones)| zones[0].0.index());
        Self {
            registry,
            spawn_table,
            prey_table,
            fear_table,
            villages,
            next_managed_id: MANAGED_ID_BASE,
            food_table,
            lay_table,
            laid: Vec::new(),
            shots: Vec::new(),
            flying: Vec::new(),
            spawned: Vec::new(),
            wanted: Vec::new(),
            murders: Vec::new(),
            assaults: Vec::new(),
            geometry,
            seed,
            creatures: Vec::new(),
            awake: HashSet::new(),
            sleeping: HashMap::new(),
            zone_groups,
            next_id: MANUAL_ID_BASE,
            rng: StdRng::seed_from_u64(seed),
            sun_dir: None,
            climate: None,
            populate: true,
            save_path: None,
            columns: HashMap::new(),
        }
    }

    /// Monde enregistré dans le dossier de sauvegarde `dir` : reprend les zones déjà peuplées (bêtes
    /// tuées comprises, qui ne reviennent pas) et y enregistrera la population avec `save`
    pub fn with_save_dir(base_radius: f64, seed: u64, registry: Arc<CreatureRegistry>, dir: impl AsRef<std::path::Path>) -> Self {
        let mut world = Self::new(base_radius, seed, registry);
        let path = dir.as_ref().join(POPULATION_FILE);
        match PopulationSave::load(&path) {
            Ok(Some(save)) => {
                world.next_id = save.next_id.max(MANUAL_ID_BASE);
                world.sleeping = save.into_map();
                world.save_path = Some(path);
            }
            Ok(None) => world.save_path = Some(path),
            // Fichier illisible : on ne l'écrase pas, la population sera retirée sans être gardée
            Err(e) => log::warn!("Population de {} illisible ({e}) : elle ne sera pas enregistrée.", path.display()),
        }
        world
    }

    pub fn registry(&self) -> &Arc<CreatureRegistry> {
        &self.registry
    }

    /// Les bêtes éveillées (celles des zones proches d'un joueur)
    /// Taille d'un bloc, en unités du monde
    pub fn block_size(&self) -> f32 {
        self.geometry.block_height as f32
    }

    pub fn creatures(&self) -> &[Creature] {
        &self.creatures
    }

    /// Règle l'heure : la direction du soleil, vue du centre de la planète
    pub fn set_sun_dir(&mut self, sun_dir: Vec3) {
        self.sun_dir = Some(sun_dir.normalize_or_zero());
    }

    /// Règle la météo et le temps du ciel (saisons) : à appeler à chaque image avec `set_sun_dir`
    pub fn set_climate(&mut self, model: &crate::weather::WeatherModel, time: f64) {
        match &mut self.climate {
            Some((m, t)) => {
                m.forced = model.forced;
                *t = time;
            }
            None => self.climate = Some((model.clone(), time)),
        }
    }

    /// Enregistre la population : zones déjà peuplées, bêtes endormies et éveillées. Sans effet pour
    /// un monde sans sauvegarde.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.save_path else { return Ok(()) };
        let mut zones = self.sleeping.clone();
        for zone in &self.awake {
            zones.entry(*zone).or_default();
        }
        for c in self.creatures.iter().filter(|c| c.is_alive() && !c.managed) {
            if let (Some(zone), Some(sleeping)) = (self.zone_of(c.position), self.sleeping_creature(c)) {
                zones.entry(zone).or_default().push(sleeping);
            }
        }
        PopulationSave { next_id: self.next_id, zones: zones.into_iter().collect() }.save(path)
    }

    /// Fait apparaître une créature de l'espèce de rang `species` aux pieds `feet`, et renvoie son
    /// identifiant. Elle s'endort avec sa zone si aucun joueur n'est près.
    pub fn spawn(&mut self, species: usize, feet: Vec3) -> u64 {
        self.spawn_in_herd(species, feet, None)
    }

    /// Comme `spawn`, mais dans le troupeau `herd` (`None` : elle en commence un nouveau)
    pub fn spawn_in_herd(&mut self, species: usize, feet: Vec3, herd: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        let yaw = self.rng.gen_range(0.0..TAU);
        self.wake_creature(id, species, herd.unwrap_or(id), feet, yaw, health, false, None);
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn wake_creature(&mut self, id: u64, species: usize, herd: u64, feet: Vec3, yaw: f32, health: f32, tamed: bool, age: Option<f32>) {
        let timer = self.rng.gen_range(0.5..3.0);
        let breeding = self.registry.get(species).and_then(|s| s.breeding.as_ref());
        let growth_seconds = breeding.map_or(600.0, |b| b.growth_minutes * 60.0);
        let lay_in = self.next_laying(species);
        self.creatures.push(Creature {
            id,
            species,
            herd,
            position: feet,
            yaw,
            target_yaw: yaw,
            vertical_speed: 0.0,
            push: Vec3::ZERO,
            walking: false,
            pause: None,
            timer,
            health,
            hurt: 0.0,
            panic: 0.0,
            panic_speed: PANIC_SPEED,
            dead_for: None,
            sleeping: false,
            detour_side: if self.rng.gen_bool(0.5) { 1.0 } else { -1.0 },
            swimming: false,
            sink: 0.0,
            hunting: false,
            pace: 1.0,
            anger: 0.0,
            attack_cooldown: 0.0,
            striking: 0.0,
            sated: 0.0,
            charge: Charge::Idle,
            managed: false,
            hidden: false,
            guide: None,
            guide_pace: 1.0,
            arrived: false,
            look: None,
            tamed,
            age,
            growth_seconds,
            lay_in,
            love: 0.0,
            breed_cooldown: 0.0,
            hearts: 0.0,
            mate: None,
            quarry: None,
            chase_from: None,
            calm: 0.0,
            slow: 0.0,
            slow_time: 0.0,
            haste: 0.0,
            haste_time: 0.0,
            shield: 0.0,
            shield_time: 0.0,
            stun: 0.0,
            flight: None,
            landed: None,
            guard_broken: 0.0,
            blind: 0.0,
            shreds: Vec::new(),
            taunted: None,
            summoner: None,
            appearing: 0.0,
            appear_total: 0.0,
            vanish: None,
            bleed: 0.0,
            bleed_left: 0.0,
            bleed_next: 0.0,
            copy: None,
        });
    }

    /// Donne `item` à manger à la créature `id`. Une bête sauvage s'apprivoise une fois sur trois
    /// (`Breeding::taming_chance`) ; apprivoisée et adulte, elle devient amoureuse et cherche un partenaire apprivoisé de
    /// son espèce ; petite, elle grandit plus vite. Refusé si ce n'est pas sa nourriture, si elle est
    /// déjà amoureuse ou vient d'avoir un petit.
    pub fn feed(&mut self, id: u64, item: ItemId) -> Feeding {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) else { return Feeding::Refused };
        if !self.food_table.get(c.species).is_some_and(|food| food.contains(&item)) {
            return Feeding::Refused;
        }
        // Un repas la réveille et la calme
        c.sleeping = false;
        c.panic = 0.0;
        if let Some(age) = c.age.as_mut() {
            *age += c.growth_seconds * MEAL_GROWTH;
            c.hearts = HEARTS_TIME * 0.5;
            return Feeding::Grew;
        }
        if !c.tamed {
            let chance = self.registry.get(c.species).and_then(|s| s.breeding.as_ref()).map_or(1.0, |b| b.taming_chance);
            if !self.rng.gen_bool(chance.clamp(0.0, 1.0) as f64) {
                return Feeding::Taming;
            }
            c.tamed = true;
            c.hearts = HEARTS_TIME;
            return Feeding::Tamed;
        }
        if c.love > 0.0 || c.breed_cooldown > 0.0 {
            return Feeding::Refused;
        }
        c.love = LOVE_TIME;
        Feeding::InLove
    }

    /// Nom de l'espèce de la créature `id`
    pub fn species_name(&self, id: u64) -> Option<&str> {
        let c = self.creatures.iter().find(|c| c.id == id)?;
        self.registry.get(c.species).map(|s| s.name.as_str())
    }

    /// Délai avant la prochaine ponte d'une bête de l'espèce `species`, en secondes (infini si
    /// elle ne pond pas)
    fn next_laying(&mut self, species: usize) -> f32 {
        match self.lay_table.get(species).copied().flatten() {
            Some((_, [min, max])) if max > min => self.rng.gen_range(min..max) * 60.0,
            Some((_, [min, _])) => min * 60.0,
            None => f32::INFINITY,
        }
    }

    /// Œufs pondus depuis le dernier appel : où, et quel objet (à faire tomber au sol)
    pub fn take_laid(&mut self) -> Vec<(Vec3, ItemId)> {
        std::mem::take(&mut self.laid)
    }

    /// Ce que mange l'espèce de rang `species` (vide : elle ne s'élève pas)
    pub fn food_of(&self, species: usize) -> &[ItemId] {
        self.food_table.get(species).map_or(&[], |f| &f[..])
    }

    /// Avance l'âge des petits et l'humeur d'élevage ; chaque bête amoureuse va vers le partenaire
    /// amoureux de son espèce le plus proche, et le couple réuni donne un petit, apprivoisé, de leur
    /// troupeau
    fn rear(&mut self, dt: f32) {
        let block = self.geometry.block_height as f32;
        // Les bêtes apprivoisées et adultes qui pondent laissent leur œuf à leurs pieds
        let mut layers = Vec::new();
        for (i, c) in self.creatures.iter_mut().enumerate() {
            if c.tamed && c.age.is_none() && c.is_alive() && c.lay_in.is_finite() {
                c.lay_in -= dt;
                if c.lay_in <= 0.0 {
                    layers.push(i);
                }
            }
        }
        for i in layers {
            let (species, position) = (self.creatures[i].species, self.creatures[i].position);
            if let Some((item, _)) = self.lay_table[species] {
                self.laid.push((position, item));
            }
            self.creatures[i].lay_in = self.next_laying(species);
        }
        for c in self.creatures.iter_mut() {
            c.love = (c.love - dt).max(0.0);
            c.breed_cooldown = (c.breed_cooldown - dt).max(0.0);
            c.hearts = (c.hearts - dt).max(0.0);
            if let Some(age) = c.age.as_mut() {
                *age += dt;
                if *age >= c.growth_seconds {
                    c.age = None;
                }
            }
            c.mate = None;
        }
        let lovers: Vec<usize> = (0..self.creatures.len()).filter(|&i| self.creatures[i].love > 0.0 && self.creatures[i].is_alive()).collect();
        let mut births = Vec::new();
        for (k, &i) in lovers.iter().enumerate() {
            if self.creatures[i].love <= 0.0 {
                // Elle vient d'avoir un petit avec une autre
                continue;
            }
            let here = self.creatures[i].position;
            let partner = lovers[k + 1..]
                .iter()
                .chain(&lovers[..k])
                .copied()
                .filter(|&j| self.creatures[j].love > 0.0 && self.creatures[j].species == self.creatures[i].species)
                .map(|j| (j, self.creatures[j].position.distance(here) / block))
                .filter(|&(_, d)| d <= MATE_RADIUS_BLOCKS)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((j, distance)) = partner else { continue };
            if distance <= MATE_DISTANCE_BLOCKS {
                let there = self.creatures[j].position;
                for n in [i, j] {
                    let c = &mut self.creatures[n];
                    c.love = 0.0;
                    c.breed_cooldown = BREED_COOLDOWN;
                    c.hearts = HEARTS_TIME;
                }
                births.push((self.creatures[i].species, self.creatures[i].herd, (here + there) * 0.5));
            } else {
                self.creatures[i].mate = Some(self.creatures[j].position);
            }
        }
        for (species, herd, feet) in births {
            let id = self.next_id;
            self.next_id += 1;
            let health = self.registry.get(species).map_or(1.0, |s| s.health);
            let yaw = self.rng.gen_range(0.0..TAU);
            self.wake_creature(id, species, herd, feet, yaw, health, true, Some(0.0));
        }
    }

    /// Un joueur dont la tête est en `from` frappe la créature `id` : elle perd de la vie, se réveille
    /// et est repoussée à l'opposé. Une bête paisible (ou une meute trop maigre) s'enfuit, et les
    /// bêtes de son troupeau proches fuient aussi, moins longtemps et moins vite ; une bête agressive
    /// riposte, avec toute sa meute. Renvoie faux si elle n'existe pas ou est déjà morte.
    pub fn hit(&mut self, id: u64, from: Vec3) -> bool {
        self.hit_by(id, from, 0)
    }

    /// Comme `hit`, par le joueur de clé `player` (celle passée à `tick_players`) : s'il frappe un
    /// habitant, les gardes de sa ville s'en prennent à lui un moment ; s'il le tue, ceux des
    /// environs le recherchent longtemps
    pub fn hit_by(&mut self, id: u64, from: Vec3, player: u64) -> bool {
        self.wound(id, from, player, PLAYER_HIT_DAMAGE).is_some()
    }

    /// Comme `hit_by`, avec `damage` points de dégâts (une compétence) : `Some(vrai)` si le coup
    /// l'a tuée, `None` si elle n'existe pas ou est déjà morte
    pub fn wound(&mut self, id: u64, from: Vec3, player: u64, damage: f32) -> Option<bool> {
        self.wound_with(id, from, player, damage, DamageKind::Physical, 0.0)
    }

    /// Comme `wound`, pour un coup de type `kind` qui ignore la part `pierce` de l'armure
    pub fn wound_with(&mut self, id: u64, from: Vec3, player: u64, damage: f32, kind: DamageKind, pierce: f32) -> Option<bool> {
        let damage = damage * (1.0 - self.guard_against(id, from));
        let victim = self.creatures.iter().find(|c| c.id == id && c.is_alive()).map(|c| (c.herd, c.position, c.managed && self.registry.get(c.species).is_some_and(|s| s.humanoid)));
        let who = self.villages.card(id).map(|card| (card.name.clone(), card.profession.clone(), self.villages.town_of(id).map(str::to_string)));
        let killed = self.strike(id, from, damage, kind, pierce, true)?;
        if let Some((herd, position, true)) = victim {
            // Le premier coup (pas ceux qui suivent tant que les gardes lui en veulent) est signalé
            // à son auteur : il sait pourquoi on l'attaque
            let already = self.is_wanted(player, herd);
            match who {
                Some((victim, profession, town)) if killed => self.murders.push(Murder { player, victim, profession, town }),
                Some((victim, profession, town)) if !already => self.assaults.push(Assault { player, victim, profession, town }),
                _ => {}
            }
            let (time, radius) = if killed { (WANTED_TIME, WANTED_RADIUS_BLOCKS) } else { (ANGER_TIME, ALARM_RADIUS_BLOCKS * 2.0) };
            let radius = radius * self.geometry.block_height as f32;
            let registry = &self.registry;
            let mut herds: Vec<u64> = self
                .creatures
                .iter()
                .filter(|o| o.is_alive() && o.position.distance(position) <= radius && registry.get(o.species).and_then(|s| s.attack.as_ref()).is_some_and(|a| a.defends_herd))
                .map(|o| o.herd)
                .collect();
            herds.push(herd);
            herds.sort_unstable();
            herds.dedup();
            for herd in herds {
                match self.wanted.iter_mut().find(|w| w.0 == player && w.1 == herd) {
                    Some(w) => w.2 = w.2.max(time),
                    None => self.wanted.push((player, herd, time)),
                }
            }
        }
        Some(killed)
    }

    /// Ralentit la créature `id` de `strength` (0 à 1) pendant `duration` secondes ; un ralenti plus
    /// fort ou plus long que celui qu'elle subit le remplace
    /// La créature `id` frappe plus vite (`value` : 0,5 pour +50 %) pendant `duration` secondes
    pub fn haste(&mut self, id: u64, value: f32, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.haste = c.haste.max(value.max(0.0));
            c.haste_time = c.haste_time.max(duration);
        }
    }

    /// Un bouclier de `amount` points sur la créature `id` pendant `duration` secondes
    pub fn shield(&mut self, id: u64, amount: f32, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.shield = c.shield.max(amount.max(0.0));
            c.shield_time = c.shield_time.max(duration);
        }
    }

    /// Fait glisser la créature `id` de `delta` dans le plan du sol, à la même altitude (le mur
    /// du Rempart Tellurique qui avance)
    pub fn slide(&mut self, id: u64, delta: Vec3) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            let radius = c.position.length();
            let up = c.position.normalize_or_zero();
            let flat = delta - up * delta.dot(up);
            c.position = (c.position + flat).normalize_or_zero() * radius;
        }
    }

    /// Entités qu'un piège a fait apparaître depuis le dernier appel : invocateur, identifiant et
    /// secondes avant qu'elles disparaissent
    pub fn take_spawned(&mut self) -> Vec<(u64, u64, f32)> {
        std::mem::take(&mut self.spawned)
    }

    /// Rend `amount` points de vie à la créature `id`, sans dépasser ceux de son espèce
    pub fn heal(&mut self, id: u64, amount: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            let full = self.registry.get(c.species).map_or(c.health, |s| s.health);
            c.health = (c.health + amount.max(0.0)).min(full.max(c.health));
        }
    }

    pub fn slow(&mut self, id: u64, strength: f32, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.slow = c.slow.max(strength.clamp(0.0, 1.0));
            c.slow_time = c.slow_time.max(duration);
        }
    }

    /// Étourdit la créature `id` pendant `duration` secondes : elle ne bouge ni ne frappe
    pub fn stun(&mut self, id: u64, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.stun = c.stun.max(duration);
        }
    }

    /// Aveugle la créature `id` pendant `duration` secondes : elle ne voit plus qu'à `BLIND_SIGHT` blocs
    pub fn blind(&mut self, id: u64, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.blind = c.blind.max(duration);
        }
    }

    /// La créature `id` ne poursuit plus que le joueur de clé `key` pendant `duration` secondes
    pub fn taunt(&mut self, id: u64, key: u64, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.taunted = Some((key, duration));
            c.sleeping = false;
        }
    }

    /// Facteur des dégâts de type `kind` que laisse passer la créature de rang `index` : son armure
    /// (moins ce qui en est brisé, et la part `pierce` que le coup ignore) contre les coups
    /// physiques, sa résistance contre les coups magiques
    fn mitigation(&self, index: usize, kind: DamageKind, pierce: f32) -> f32 {
        let c = &self.creatures[index];
        let Some(species) = self.registry.get(c.species) else { return 1.0 };
        match kind {
            DamageKind::Physical => {
                let broken: f32 = c.shreds.iter().map(|s| s.0).sum();
                armor_factor(species.armor * (1.0 - broken.clamp(0.0, 1.0)) * (1.0 - pierce.clamp(0.0, 1.0)))
            }
            DamageKind::Magical => armor_factor(species.resist),
        }
    }

    /// Armure de la créature `id` en ce moment, bris déduits (0 si elle n'existe pas)
    pub fn armor(&self, id: u64) -> f32 {
        let Some(c) = self.creatures.iter().find(|c| c.id == id) else { return 0.0 };
        let broken: f32 = c.shreds.iter().map(|s| s.0).sum();
        self.registry.get(c.species).map_or(0.0, |s| s.armor * (1.0 - broken.clamp(0.0, 1.0)))
    }

    /// Brise l'armure de la créature `id` : `amount` en moins (1 : toute) pendant `seconds`
    /// secondes, sans dépasser `max` brisé en tout. Renvoie vrai si de l'armure a cédé.
    pub fn shred(&mut self, id: u64, amount: f32, seconds: f32, max: f32) -> bool {
        let Some(index) = self.creatures.iter().position(|c| c.id == id && c.is_alive()) else { return false };
        let armored = self.registry.get(self.creatures[index].species).is_some_and(|s| s.armor > 0.0);
        let c = &mut self.creatures[index];
        let broken: f32 = c.shreds.iter().map(|s| s.0).sum();
        let amount = amount.min(max.min(1.0) - broken).max(0.0);
        // Un nouveau bris prolonge ceux d'avant : l'armure rongée coup après coup reste rongée
        for s in &mut c.shreds {
            s.1 = s.1.max(seconds);
        }
        if amount <= 0.0 || seconds <= 0.0 {
            return false;
        }
        c.shreds.push((amount, seconds));
        armored
    }

    /// Ralenti et étourdissement que le coup `hit` laisse au joueur touché (`attack.slow`,
    /// `attack.stun`), sauf ceux dont il est immunisé (`ignores`)
    pub fn afflict(&self, hit: &PlayerHit, stats: &mut herocraft_common::CharacterStats, ignores: &[Status]) {
        let Some(attack) = self.registry.get(hit.species).and_then(|s| s.attack.as_ref()) else { return };
        if let Some([strength, seconds]) = attack.slow.filter(|_| !ignores.contains(&Status::Slow)) {
            stats.apply_slow(strength, seconds);
        }
        if let Some(seconds) = attack.stun.filter(|_| !ignores.contains(&Status::Stun)) {
            stats.apply_stun(seconds);
        }
    }

    /// Recul que le coup `hit` donne au joueur aux pieds `feet` : direction (le long du sol, à
    /// l'opposé de la bête) et vitesse en blocs par seconde (`attack.knockback`)
    pub fn knockback(&self, hit: &PlayerHit, feet: Vec3) -> Option<(Vec3, f32)> {
        let strength = self.registry.get(hit.species).and_then(|s| s.attack.as_ref()).and_then(|a| a.knockback).filter(|k| *k > 0.0)?;
        let from = self.position(hit.creature_id)?;
        let up = feet.normalize_or_zero();
        let away = feet - from;
        let away = (away - up * away.dot(up)).normalize_or_zero();
        (away != Vec3::ZERO).then_some((away, strength))
    }

    /// Position de la créature `id`, vivante ou non
    pub fn position(&self, id: u64) -> Option<Vec3> {
        self.creatures.iter().find(|c| c.id == id).map(|c| c.position)
    }

    /// La garde de la créature `id` tombe pendant `duration` secondes (Brise-Garde)
    pub fn break_guard(&mut self, id: u64, duration: f32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
            c.guard_broken = c.guard_broken.max(duration);
        }
    }

    /// Part des dégâts d'un coup venu de `from` que la garde de la créature `id` retire : celle de
    /// son espèce si le coup arrive de face, et qu'elle n'est ni étourdie ni brisée
    pub fn guard_against(&self, id: u64, from: Vec3) -> f32 {
        let Some(c) = self.creatures.iter().find(|c| c.id == id && c.is_alive()) else { return 0.0 };
        let Some(guard) = self.registry.get(c.species).and_then(|s| s.guard) else { return 0.0 };
        if c.stun > 0.0 || c.guard_broken > 0.0 {
            return 0.0;
        }
        if herocraft_common::in_front(herocraft_common::yaw_direction(c.position, c.yaw), c.position, from, guard.arc) {
            guard.block.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// La garde de la créature `id` tient en ce moment (elle n'est ni étourdie ni brisée)
    pub fn guarding(&self, id: u64) -> bool {
        self.creatures.iter().any(|c| c.id == id && c.is_alive() && c.stun <= 0.0 && c.guard_broken <= 0.0 && self.registry.get(c.species).is_some_and(|s| s.guard.is_some()))
    }

    /// Aveuglée en ce moment
    pub fn is_blind(&self, id: u64) -> bool {
        self.creatures.iter().any(|c| c.id == id && c.blind > 0.0)
    }

    /// La créature `id` perd `damage` points de vie par saignement, sans reculer ni fuir
    fn bleed_out(&mut self, id: u64, damage: f32) {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) else { return };
        c.hurt = CREATURE_HURT_TIME;
        c.health -= damage;
        if c.health <= 0.0 {
            c.dead_for = Some(0.0);
            c.walking = false;
            c.hunting = false;
        }
    }

    /// Repousse la créature `id` loin de `from` à `speed` blocs par seconde (négative : l'attire)
    pub fn shove(&mut self, id: u64, from: Vec3, speed: f32) {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) else { return };
        let up = c.position.normalize_or_zero();
        let away = c.position - from;
        let away = (away - up * away.dot(up)).normalize_or_zero();
        c.push = away * speed;
        c.vertical_speed = c.vertical_speed.max(KNOCKBACK_HOP * speed.abs().min(KNOCKBACK_SPEED) / KNOCKBACK_SPEED);
    }

    /// Projette la créature `id` à `height` blocs en l'air : elle reste étourdie tant qu'elle vole,
    /// et se blesse en retombant de haut
    pub fn launch(&mut self, id: u64, height: f32) {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive() && c.summoner.is_none()) else { return };
        c.vertical_speed = (2.0 * GRAVITY * height.max(0.0)).sqrt();
        c.push = Vec3::ZERO;
        let flight = c.flight.get_or_insert(Flight { peak: f32::MIN, slam: None });
        flight.slam = None;
    }

    /// Rabat la créature `id` au sol : à l'atterrissage, elle perd `damage` points de vie, portés par
    /// le joueur `player`
    pub fn slam(&mut self, id: u64, player: u64, damage: f32) {
        let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive() && c.summoner.is_none()) else { return };
        c.vertical_speed = -SLAM_SPEED;
        c.push = Vec3::ZERO;
        c.flight.get_or_insert(Flight { peak: f32::MIN, slam: None }).slam = Some((player, damage));
    }

    /// La créature `id` vole, projetée par une compétence
    pub fn is_airborne(&self, id: u64) -> bool {
        self.creatures.iter().any(|c| c.id == id && c.flight.is_some())
    }

    /// Les bêtes retombées d'un vol se blessent : la chute au-delà de `LAUNCH_SAFE_FALL` blocs, et le
    /// coup qui les a rabattues
    fn land(&mut self) {
        let block = self.geometry.block_height as f32;
        let landed: Vec<(u64, Vec3, f32, Option<(u64, f32)>)> = self.creatures.iter_mut().filter_map(|c| c.landed.take().map(|(fall, slam)| (c.id, c.position, fall, slam))).collect();
        for (id, at, fall, slam) in landed {
            let (player, hit) = slam.unwrap_or((0, 0.0));
            let damage = hit + (fall - LAUNCH_SAFE_FALL).max(0.0) * LAUNCH_FALL_DAMAGE;
            if damage > 0.0 {
                // Frappée d'en haut : elle ne recule pas
                self.wound(id, at + at.normalize_or_zero() * block, player, damage);
                if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id) {
                    c.push = Vec3::ZERO;
                }
            }
        }
    }

    /// Les meurtres d'habitants depuis le dernier appel, à annoncer dans le chat
    pub fn take_murders(&mut self) -> Vec<Murder> {
        std::mem::take(&mut self.murders)
    }

    /// Les coups portés aux habitants depuis le dernier appel, à signaler à leur auteur
    pub fn take_assaults(&mut self) -> Vec<Assault> {
        std::mem::take(&mut self.assaults)
    }

    /// Le joueur de clé `player` est recherché par les gardes du troupeau `herd`
    pub fn is_wanted(&self, player: u64, herd: u64) -> bool {
        self.wanted.iter().any(|w| w.0 == player && w.1 == herd)
    }

    /// Dépèce la bête morte `id` (encore à terre) : elle disparaît, et renvoie son espèce (rang
    /// dans le registre) et ses pieds, où tombe son butin. `None` si elle n'existe pas ou vit encore.
    pub fn skin(&mut self, id: u64) -> Option<(usize, Vec3)> {
        let index = self.creatures.iter().position(|c| c.id == id && !c.is_alive())?;
        let c = self.creatures.remove(index);
        Some((c.species, c.position))
    }

    /// Bêtes vivantes de l'espèce de la créature de rang `index` autour d'elle, elle comprise
    fn pack_count(&self, index: usize) -> usize {
        let c = &self.creatures[index];
        let radius = PACK_RADIUS * self.geometry.block_height as f32;
        self.creatures.iter().filter(|o| o.is_alive() && o.species == c.species && o.position.distance(c.position) <= radius).count()
    }

    /// Coup de `damage` sur la créature `id`, porté depuis `from` (par un joueur ou par un
    /// prédateur). Renvoie `Some(vrai)` s'il l'a tuée, `None` si elle n'existe pas ou est déjà morte.
    fn strike(&mut self, id: u64, from: Vec3, damage: f32, kind: DamageKind, pierce: f32, by_player: bool) -> Option<bool> {
        let index = self.creatures.iter().position(|c| c.id == id && c.is_alive())?;
        let damage = damage * self.mitigation(index, kind, pierce);
        let species = self.creatures[index].species;
        let attack = self.registry.get(species).and_then(|s| s.attack.as_ref());
        let fights_back = by_player
            && attack.is_some_and(|a| a.mode != AttackMode::Pack || self.pack_count(index) >= a.pack_size as usize);
        if fights_back && attack.is_some_and(|a| a.mode == AttackMode::Pack) {
            // Toute la meute prend la défense de la bête frappée
            let (position, radius) = (self.creatures[index].position, PACK_RADIUS * self.geometry.block_height as f32);
            for other in self.creatures.iter_mut().filter(|o| o.is_alive() && o.species == species && o.position.distance(position) <= radius) {
                other.anger = ANGER_TIME;
            }
        }

        let c = &mut self.creatures[index];
        // Le bouclier d'une compétence encaisse d'abord
        let absorbed = damage.min(c.shield).max(0.0);
        c.shield -= absorbed;
        let damage = damage - absorbed;
        // Une entité invoquée encaisse sans reculer ni fuir
        if c.summoner.is_some() {
            c.hurt = CREATURE_HURT_TIME;
            c.health -= damage;
            if c.health <= 0.0 {
                c.dead_for = Some(0.0);
                c.walking = false;
                c.hunting = false;
                return Some(true);
            }
            return Some(false);
        }
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let away = c.position - from;
        let away = (away - up * away.dot(up)).normalize_or_zero();
        // Frappée d'aplomb : elle recule droit devant elle
        let away = if away == Vec3::ZERO { north * c.yaw.cos() + east * c.yaw.sin() } else { away };
        c.push = away * KNOCKBACK_SPEED;
        c.vertical_speed = c.vertical_speed.max(KNOCKBACK_HOP);
        c.hurt = CREATURE_HURT_TIME;
        c.health -= damage;
        c.sleeping = false;
        if c.health <= 0.0 {
            c.dead_for = Some(0.0);
            c.walking = false;
            c.hunting = false;
            return Some(true);
        }
        if fights_back {
            c.anger = ANGER_TIME;
        } else {
            c.panic = PANIC_TIME;
            c.panic_speed = PANIC_SPEED;
            c.walking = true;
            c.hunting = false;
            c.pace = 1.0;
            c.charge = Charge::Idle;
            c.target_yaw = away.dot(east).atan2(away.dot(north)).rem_euclid(TAU);
        }
        if !fights_back {
            // Les bêtes de son troupeau s'affolent avec elle
            let (herd, center) = (c.herd, c.position);
            self.alarm_herd(id, herd, center, from);
        }
        Some(false)
    }

    /// Les bêtes du troupeau `herd` proches de `center` (sauf la victime `victim`) s'enfuient loin
    /// de l'attaquant en `from`, sans prendre de coup : fuite courte et modérée
    fn alarm_herd(&mut self, victim: u64, herd: u64, center: Vec3, from: Vec3) {
        let radius = ALARM_RADIUS_BLOCKS * self.geometry.block_height as f32;
        let registry = &self.registry;
        for c in &mut self.creatures {
            if c.id == victim || c.herd != herd || !c.is_alive() || c.hidden || c.position.distance(center) > radius {
                continue;
            }
            if c.anger > 0.0 && registry.get(c.species).is_some_and(|s| s.attack.is_some()) {
                continue;
            }
            // Déjà en fuite plus longue (frappée elle-même, ou déjà alertée) : elle la garde
            if c.panic >= ALARM_TIME {
                continue;
            }
            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let away = c.position - from;
            let away = (away - up * away.dot(up)).normalize_or_zero();
            if away != Vec3::ZERO {
                let jitter = self.rng.gen_range(-0.35..0.35);
                c.target_yaw = (away.dot(east).atan2(away.dot(north)) + jitter).rem_euclid(TAU);
            }
            c.panic = ALARM_TIME;
            c.panic_speed = ALARM_SPEED;
            c.walking = true;
        }
    }

    /// Les bêtes agressives choisissent leur cible, s'en approchent et frappent celles à portée.
    /// Renvoie les coups portés aux joueurs ; ceux portés aux proies sont appliqués ici.
    fn hunt(&mut self, dt: f32, players: &[Vec3], keys: &[u64], hidden: &[bool]) -> Vec<PlayerHit> {
        let block = self.geometry.block_height as f32;
        for w in &mut self.wanted {
            w.2 -= dt;
        }
        self.wanted.retain(|w| w.2 > 0.0);
        // Identifiant, espèce, pieds, vivante et visible, en chasse, apprivoisée, invoquée
        let snapshot: Vec<(u64, usize, Vec3, bool, bool, bool, bool)> =
            self.creatures.iter().map(|c| (c.id, c.species, c.position, c.is_alive() && !c.hidden, c.hunting, c.tamed, c.summoner.is_some())).collect();
        let mut player_hits = Vec::new();
        // Les projectiles arrivés touchent leur cible, ou tout ce qui est hostile au tireur dans le
        // rayon de leur explosion
        let mut blasts = Vec::new();
        let mut spawns: Vec<(herocraft_common::BurstSpawn, Vec3, u64)> = Vec::new();
        for f in &mut self.flying {
            f.left -= dt;
        }
        let (arrived, flying): (Vec<Flying>, Vec<Flying>) = std::mem::take(&mut self.flying).into_iter().partition(|f| f.left <= 0.0);
        self.flying = flying;
        for f in arrived {
            if f.pierce > 0.0 {
                // Il traverse : tout ennemi près de sa trajectoire, prolongée jusqu'au bout de la grappe
                let line = f.to - f.from;
                let length = line.length().max(1e-6);
                let along = line / length;
                let end = length + 4.0 * f.pierce;
                for &(id, _, position, alive, _, tamed, summoned) in &snapshot {
                    let foe = if f.allied { !tamed && !summoned } else { summoned };
                    let t = (position - f.from).dot(along);
                    let off = (position - f.from - along * t).length();
                    if alive && foe && id != f.shooter && t >= 0.0 && t <= end && off <= f.pierce + TARGET_RADIUS * block {
                        blasts.push((id, f.from, f.damage, f.kind));
                    }
                }
            } else if f.radius > 0.0 {
                for &(id, _, position, alive, _, tamed, summoned) in &snapshot {
                    let foe = if f.allied { !tamed } else { summoned };
                    if alive && foe && id != f.shooter && position.distance(f.to) <= f.radius {
                        blasts.push((id, f.to, f.damage, f.kind));
                    }
                }
                if !f.allied {
                    for (player, feet) in players.iter().enumerate() {
                        if feet.distance(f.to) <= f.radius {
                            player_hits.push(PlayerHit { player, creature_id: f.shooter, species: f.species, damage: f.damage, kind: f.kind });
                        }
                    }
                }
            } else {
                match f.target {
                    Target::Creature(id) => blasts.push((id, f.to, f.damage, f.kind)),
                    Target::Player(player) => player_hits.push(PlayerHit { player, creature_id: f.shooter, species: f.species, damage: f.damage, kind: f.kind }),
                }
            }
        }
        // Les aimants (Piège Magnétique) tirent à eux les ennemis proches
        let magnets: Vec<(u64, Vec3, f32, f32, bool)> = self
            .creatures
            .iter()
            .filter(|c| c.is_alive() && c.appearing <= 0.0)
            .filter_map(|c| self.registry.get(c.species)?.magnet.map(|m| (c.id, c.position, m.radius * block, m.speed, c.summoner.is_some())))
            .collect();
        for &(magnet, at, radius, speed, allied) in &magnets {
            for c in self.creatures.iter_mut().filter(|c| c.is_alive() && c.id != magnet) {
                let foe = if allied { !c.tamed } else { c.summoner.is_some() };
                let to = at - c.position;
                let up = c.position.normalize_or_zero();
                let flat = to - up * to.dot(up);
                let distance = flat.length();
                if foe && distance <= radius && distance > 0.3 * block {
                    c.push = flat / distance * speed * (distance / radius).clamp(0.3, 1.0);
                }
            }
        }
        // Coups portés aux proies : proie, d'où, dégâts, rang du prédateur
        let mut bites = Vec::new();
        // Pertes de sang de l'instant : qui, combien
        let mut bleeding = Vec::new();
        // Ennemis qu'un piège fait saigner : qui, par seconde, combien de temps
        let mut cuts = Vec::new();
        // Ennemis immobilisés par un piège, et pour combien de temps
        let mut stuns = Vec::new();
        let Self { creatures, registry, prey_table, rng, wanted, shots, flying, .. } = self;
        for (index, c) in creatures.iter_mut().enumerate() {
            if c.bleed_left > 0.0 && c.is_alive() {
                c.bleed_left -= dt;
                c.bleed_next -= dt;
                if c.bleed_next <= 0.0 {
                    c.bleed_next += 1.0;
                    bleeding.push((c.id, c.bleed));
                }
            }
            c.stun = (c.stun - dt).max(0.0);
            c.guard_broken = (c.guard_broken - dt).max(0.0);
            c.blind = (c.blind - dt).max(0.0);
            c.shreds.retain_mut(|s| {
                s.1 -= dt;
                s.1 > 0.0
            });
            if let Some(t) = c.taunted.as_mut() {
                t.1 -= dt;
            }
            c.taunted = c.taunted.filter(|t| t.1 > 0.0);
            c.slow_time = (c.slow_time - dt).max(0.0);
            if c.slow_time <= 0.0 {
                c.slow = 0.0;
            }
            c.haste_time = (c.haste_time - dt).max(0.0);
            if c.haste_time <= 0.0 {
                c.haste = 0.0;
            }
            c.shield_time = (c.shield_time - dt).max(0.0);
            if c.shield_time <= 0.0 {
                c.shield = 0.0;
            }
            // Étourdie, elle ne frappe pas ; pressée, elle recharge plus vite
            c.attack_cooldown = (c.attack_cooldown - dt * (1.0 + c.haste)).max(c.stun);
            c.striking = (c.striking - dt).max(0.0);
            c.anger = (c.anger - dt).max(0.0);
            c.sated = (c.sated - dt).max(0.0);
            // Un piège déclenché ne fait plus rien
            if let Some(t) = c.vanish.as_mut() {
                *t -= dt;
                c.hunting = false;
                continue;
            }
            // Une entité invoquée ne fait rien avant d'être en place
            if c.appearing > 0.0 {
                c.appearing = (c.appearing - dt).max(0.0);
                c.hunting = false;
                c.walking = false;
                continue;
            }
            let Some(species) = registry.get(c.species) else { continue };
            let Some(attack) = &species.attack else { continue };
            // Endormie, elle ne chasse pas, sauf si un coup l'a réveillée en colère
            if !c.is_alive() || c.hidden || c.panic > 0.0 || (c.sleeping && c.anger <= 0.0) {
                c.hunting = false;
                continue;
            }
            // Revenue du bout de sa laisse : elle regagne son poste sans chercher querelle
            let leashed = attack.engage.is_some() || attack.leash.is_some();
            if leashed && c.calm > 0.0 {
                c.calm = (c.calm - dt).max(0.0);
                c.stop_hunting(rng);
                continue;
            }
            c.sleeping = false;
            let (mut kin, mut alerted) = (0, false);
            for &(id, kind, position, alive, hunting, _, _) in &snapshot {
                if alive && kind == c.species && position.distance(c.position) <= PACK_RADIUS * block {
                    kin += 1;
                    alerted |= hunting && id != c.id;
                }
            }
            // Provoquée, elle est aussi en colère
            let angry = c.anger > 0.0 || c.taunted.is_some();
            let pack_mode = attack.mode == AttackMode::Pack;
            if (pack_mode && kin < attack.pack_size as usize) || (attack.provoked_only && !angry) {
                c.stop_hunting(rng);
                continue;
            }

            // La cible la plus proche à portée de vue : un joueur, ou une proie s'il a faim
            let range = attack.sight * block * if c.hunting || angry || (pack_mode && alerted) { PURSUIT_FACTOR } else { 1.0 };
            // Aveuglée, elle ne voit plus qu'à quelques pas
            let range = if c.blind > 0.0 { range.min(BLIND_SIGHT * block) } else { range };
            // Une bête tenue en laisse ne prend pour cible que ce qui l'approche (`engage`), mais
            // ne lâche pas celle qu'elle poursuit déjà
            let engage = attack.engage.map_or(range, |e| e * block);
            let quarry = c.quarry.filter(|_| c.hunting && leashed);
            let mut target: Option<(Target, Vec3, f32)> = None;
            let mut kept: Option<(Target, Vec3, f32)> = None;
            let mut consider = |candidate: Target, position: Vec3| {
                let distance = position.distance(c.position);
                if quarry == Some(candidate) && distance <= range {
                    kept = Some((candidate, position, distance));
                } else if distance <= range.min(engage) && target.is_none_or(|(_, _, best)| distance < best) {
                    target = Some((candidate, position, distance));
                }
            };
            let summoned = c.summoner.is_some();
            for (k, &player) in players.iter().enumerate() {
                // Une invocation ne s'en prend jamais aux joueurs
                if summoned {
                    break;
                }
                let key = keys.get(k).copied().unwrap_or(0);
                // Invisible, il passe sous son nez
                if hidden.get(k).copied().unwrap_or(false) {
                    continue;
                }
                // Un humanoïde (garde, voyageur armé) ne s'en prend qu'au joueur qui a frappé l'un des
                // siens (`wanted`), jamais à son voisin ; une bête en colère, à tous
                if attack.hunts_players || (angry && !species.humanoid) || wanted.iter().any(|w| w.0 == key && w.1 == c.herd) {
                    consider(Target::Player(k), player);
                }
            }
            if c.sated <= 0.0 && !angry {
                for &(id, kind, position, alive, _, tamed, _) in &snapshot {
                    if alive && prey_table[c.species].contains(&kind) && !(summoned && tamed) {
                        consider(Target::Creature(id), position);
                    }
                }
            }
            // Une invocation s'en prend aux bêtes agressives ; une bête agressive, aux invocations
            let hostile = |kind: usize| registry.get(kind).and_then(|s| s.attack.as_ref()).is_some_and(|a| a.hunts_players);
            for &(id, kind, position, alive, _, tamed, ally) in &snapshot {
                let foe = if summoned { !tamed && hostile(kind) } else { attack.hunts_players && ally };
                if alive && foe && id != c.id {
                    consider(Target::Creature(id), position);
                }
            }
            // Une invocation qui provoque (`taunt`) attire à elle les bêtes agressives proches
            let taunter = (attack.hunts_players && !summoned)
                .then(|| {
                    snapshot
                        .iter()
                        .filter(|e| e.3 && e.6)
                        .filter_map(|e| {
                            let reach = registry.get(e.1)?.taunt? * block;
                            let distance = e.2.distance(c.position);
                            (distance <= reach).then_some((Target::Creature(e.0), e.2, distance))
                        })
                        .min_by(|a, b| a.2.total_cmp(&b.2))
                })
                .flatten();
            // Provoquée par un joueur (compétence) : c'est lui qu'elle poursuit, où qu'il soit
            let provoker = c.taunted.filter(|_| !summoned).and_then(|(key, _)| {
                let k = (0..players.len()).find(|&k| keys.get(k).copied().unwrap_or(0) == key)?;
                (!hidden.get(k).copied().unwrap_or(false)).then(|| (Target::Player(k), players[k], players[k].distance(c.position)))
            });
            let Some((target, goal, _)) = provoker.or(taunter).or(kept).or(target) else {
                c.stop_hunting(rng);
                c.quarry = None;
                c.chase_from = None;
                continue;
            };
            if leashed {
                let from = *c.chase_from.get_or_insert(c.position);
                if attack.leash.is_some_and(|l| c.position.distance(from) > l * block) {
                    // Trop loin de là où elle a commencé : elle lâche et y revient
                    c.stop_hunting(rng);
                    c.quarry = None;
                    c.chase_from = None;
                    c.calm = CALM_TIME;
                    continue;
                }
                c.quarry = Some(target);
            }
            c.hunting = true;

            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let yaw_to = |v: Vec3| v.dot(east).atan2(v.dot(north)).rem_euclid(TAU);
            let to = goal - c.position;
            let flat = to - up * to.dot(up);
            let flat_distance = flat.length() / block;
            let toward = yaw_to(flat);
            let in_reach = flat_distance <= species.body.half_length + attack.reach + TARGET_RADIUS && (to.dot(up) / block).abs() <= MAX_STRIKE_HEIGHT;
            // Trop loin pour frapper, assez près pour tirer
            let shot = attack.shot.as_ref().filter(|s| !in_reach && flat_distance <= s.range);
            let mut strikes = false;
            match attack.mode {
                AttackMode::Charge => {
                    c.charge = match c.charge {
                        Charge::Idle if in_reach => {
                            // Au contact, elle frappe sans prendre d'élan
                            c.walking = false;
                            c.target_yaw = toward;
                            strikes = angle_between(c.yaw, toward).abs() < STRIKE_ANGLE;
                            Charge::Idle
                        }
                        Charge::Idle => {
                            c.walking = false;
                            c.target_yaw = toward;
                            Charge::Aiming(CHARGE_AIM)
                        }
                        Charge::Aiming(t) => {
                            c.walking = false;
                            c.target_yaw = toward;
                            if t > dt { Charge::Aiming(t - dt) } else { Charge::Running(CHARGE_RUN) }
                        }
                        // Lancée, elle ne corrige plus son cap
                        Charge::Running(t) if in_reach && c.attack_cooldown <= 0.0 => {
                            strikes = true;
                            Charge::Resting(CHARGE_REST + t.min(0.0))
                        }
                        Charge::Running(t) => {
                            c.walking = true;
                            c.pace = attack.chase_speed;
                            if t > dt { Charge::Running(t - dt) } else { Charge::Resting(CHARGE_REST) }
                        }
                        Charge::Resting(t) => {
                            c.walking = false;
                            c.pace = 1.0;
                            if t > dt { Charge::Resting(t - dt) } else { Charge::Idle }
                        }
                    };
                    if matches!(c.charge, Charge::Resting(_)) {
                        c.walking = false;
                    }
                }
                // Un piège se déclenche dès qu'on marche dessus, de quelque côté que ce soit
                AttackMode::Solo | AttackMode::Pack if attack.burst.is_some() => {
                    c.walking = false;
                    strikes = in_reach;
                }
                AttackMode::Solo | AttackMode::Pack => {
                    if in_reach || shot.is_some() {
                        c.walking = false;
                        c.target_yaw = toward;
                        strikes = angle_between(c.yaw, toward).abs() < STRIKE_ANGLE;
                    } else {
                        c.walking = true;
                        c.pace = attack.chase_speed;
                        c.target_yaw = if pack_mode && flat_distance > FLANK_DISTANCE {
                            // Chaque bête de la meute vise son propre côté de la cible
                            let angle = (c.id as f32 * 2.399_963).rem_euclid(TAU);
                            let (goal_east, goal_north) = tangent_frame(goal.normalize_or_zero());
                            yaw_to(goal + (goal_north * angle.cos() + goal_east * angle.sin()) * FLANK_OFFSET * block - c.position)
                        } else {
                            toward
                        };
                    }
                }
            }
            if strikes && c.attack_cooldown <= 0.0 {
                c.attack_cooldown = attack.cooldown;
                // Le temps d'un cycle de son animation d'attaque
                c.striking = species.model.animations.get("attaque").map_or(STRIKE_ANIMATION, |a| a.cycle);
                if let Some(burst) = &attack.burst {
                    // Il explose sur tout ennemi proche, puis disparaît le temps de son animation
                    let reach = burst.radius * block;
                    for &(id, _, position, alive, _, tamed, ally) in &snapshot {
                        let foe = if summoned { !tamed } else { ally };
                        if alive && foe && id != c.id && position.distance(c.position) <= reach {
                            blasts.push((id, c.position, attack.damage, attack.kind));
                            if burst.stun > 0.0 {
                                stuns.push((id, burst.stun));
                            }
                            if burst.bleed > 0.0 && burst.bleed_seconds > 0.0 {
                                cuts.push((id, burst.bleed, burst.bleed_seconds));
                            }
                        }
                    }
                    if !summoned {
                        for (player, feet) in players.iter().enumerate() {
                            if feet.distance(c.position) <= reach {
                                player_hits.push(PlayerHit { player, creature_id: c.id, species: c.species, damage: attack.damage, kind: attack.kind });
                            }
                        }
                    }
                    if let (Some(spawn), Some(summoner)) = (&burst.spawn, c.summoner) {
                        spawns.push((spawn.clone(), c.position, summoner));
                    }
                    shots.push(CreatureShotEvent { creature_id: c.id, species: c.species as u32, from: c.position, to: c.position });
                    c.vanish = Some(c.striking.max(0.1));
                    continue;
                }
                if let Some(shot) = shot {
                    let from = c.position + up * shot.height * block;
                    let aim = goal + up * TARGET_RADIUS * block;
                    shots.push(CreatureShotEvent { creature_id: c.id, species: c.species as u32, from, to: aim });
                    flying.push(Flying {
                        shooter: c.id,
                        allied: summoned,
                        species: c.species,
                        target,
                        from,
                        pierce: shot.pierce * block,
                        to: goal,
                        damage: shot.damage.unwrap_or(attack.damage),
                        kind: attack.kind,
                        radius: shot.radius * block,
                        left: shot.flight_seconds(from, aim, block),
                    });
                    continue;
                }
                // Un petit bond vers la cible
                if species.walk_speed > 0.0 {
                    c.push += flat.normalize_or_zero() * LUNGE_SPEED;
                    if c.vertical_speed == 0.0 {
                        c.vertical_speed = LUNGE_HOP;
                    }
                }
                match target {
                    Target::Player(player) => player_hits.push(PlayerHit { player, creature_id: c.id, species: c.species, damage: attack.damage, kind: attack.kind }),
                    Target::Creature(prey) => bites.push((prey, c.position, attack.damage, attack.kind, index)),
                }
            }
        }
        for (id, from, damage, kind) in blasts {
            self.strike(id, from, damage, kind, 0.0, false);
        }
        // Ce que les pièges déclenchés font apparaître, en cercle serré autour d'eux
        for (spawn, at, summoner) in spawns {
            let Some(kind) = self.registry.find(&spawn.species) else { continue };
            let (east, north) = tangent_frame(at.normalize_or_zero());
            for k in 0..spawn.count.max(1) {
                let angle = TAU * k as f32 / spawn.count.max(1) as f32;
                let spot = at + (north * angle.cos() + east * angle.sin()) * SPAWN_RING * block;
                let id = self.summon(kind, spot, angle, summoner);
                self.spawned.push((summoner, id, spawn.seconds));
            }
        }
        for (id, duration) in stuns {
            self.stun(id, duration);
        }
        for (id, per_second, seconds) in cuts {
            if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id && c.is_alive()) {
                (c.bleed, c.bleed_left, c.bleed_next) = (per_second, seconds, 1.0);
            }
        }
        for (id, damage) in bleeding {
            self.bleed_out(id, damage);
        }
        for (prey, from, damage, kind, hunter) in bites {
            // Un garde reste sur le qui-vive ; une bête repue laisse les proies un moment
            let defends = self.registry.get(self.creatures[hunter].species).and_then(|s| s.attack.as_ref()).is_some_and(|a| a.defends_herd || !a.hunts_players);
            if self.strike(prey, from, damage, kind, 0.0, false) == Some(true) && !defends {
                self.creatures[hunter].sated = SATED_TIME;
            }
        }
        player_hits
    }

    /// Avance la simulation de `dt` secondes autour des joueurs debout dont les pieds sont `players`
    /// (voir `tick_with_presences`), et renvoie les coups que les bêtes agressives leur ont portés
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, players: &[Vec3]) -> Vec<PlayerHit> {
        let presences: Vec<PlayerPresence> = players.iter().copied().map(PlayerPresence::standing).collect();
        self.tick_with_presences(dt, generator, edits, &presences)
    }

    /// Avance la simulation de `dt` secondes autour des joueurs `players`, en tenant compte de leur
    /// posture (accroupi ou debout) : éveille les zones qui s'en approchent (en les peuplant la
    /// première fois), endort les autres, et renvoie les coups que les bêtes agressives leur ont portés
    /// (`PlayerHit::player` : rang dans `presences`)
    pub fn tick_with_presences(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, presences: &[PlayerPresence]) -> Vec<PlayerHit> {
        self.tick_players(dt, generator, edits, presences, &[])
    }

    /// Comme `tick_with_presences`, chaque joueur reconnu par sa clé `keys[k]` (celle de `hit_by`) :
    /// les gardes poursuivent le joueur recherché, pas son voisin. Sans clé, un joueur a la clé 0.
    pub fn tick_players(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, presences: &[PlayerPresence], keys: &[u64]) -> Vec<PlayerHit> {
        let players: Vec<Vec3> = presences.iter().map(|p| p.feet).collect();
        let players = players.as_slice();
        // Une image très longue (fenêtre en arrière-plan) ne fait pas traverser les murs
        let dt = dt.min(MAX_STEP);
        // Une bête morte reste à terre le temps qu'on la dépèce
        // Un piège déclenché disparaît une fois son animation jouée
        self.creatures.retain(|c| c.dead_for.is_none_or(|t| t < CORPSE_TIME) && c.vanish.is_none_or(|t| t > 0.0));

        let columns = std::mem::take(&mut self.columns);
        let mut query = TerrainQuery::with_columns(generator, edits, columns);
        self.update_zones(&mut query, players);
        let mut villages = std::mem::take(&mut self.villages);
        villages.tick(self, &mut query, generator, players, dt);
        self.villages = villages;
        let hidden: Vec<bool> = presences.iter().map(|p| p.hidden).collect();
        let hits = self.hunt(dt, players, keys, &hidden);
        self.scare();
        self.rear(dt);

        let herds = herd_pulls(&self.creatures, self.geometry.block_height as f32);
        let mut walker = Walker { geometry: self.geometry, query: &mut query, sun_dir: self.sun_dir, climate: self.climate.as_ref() };
        for (creature, herd) in self.creatures.iter_mut().zip(herds) {
            if let Some(species) = self.registry.get(creature.species) {
                // Les joueurs invisibles passent inaperçus ; aveuglée, elle ne remarque que le plus proche
                let nearest = presences
                    .iter()
                    .copied()
                    .filter(|p| !p.hidden)
                    .filter(|p| creature.blind <= 0.0 || p.feet.distance(creature.position) <= BLIND_SIGHT * self.geometry.block_height as f32)
                    .min_by(|a, b| a.feet.distance_squared(creature.position).total_cmp(&b.feet.distance_squared(creature.position)));
                let food = self.food_table.get(creature.species).map_or(&[][..], |f| &f[..]);
                walker.step(creature, species, food, nearest, herd, &mut self.rng, dt);
            }
        }
        if dt > 0.0 {
            walker.collide(&mut self.creatures, &self.registry, players);
        }
        self.land();

        let mut columns = query.into_columns();
        if columns.len() > MAX_CACHED_COLUMNS {
            columns.clear();
        }
        self.columns = columns;
        hits
    }

    /// La zone où se trouve ce point
    fn zone_of(&self, p: Vec3) -> Option<ZoneKey> {
        self.geometry.block_at_point(p.as_dvec3()).map(|b| ZoneKey::of_column(b.face, b.x, b.z))
    }

    /// Éveille les zones proches d'un joueur, endort les autres avec leurs bêtes, et range dans sa
    /// zone endormie toute bête qui est sortie des zones éveillées
    fn update_zones(&mut self, query: &mut TerrainQuery, players: &[Vec3]) {
        let block = self.geometry.block_height as f32;
        let wake = (CREATURE_RANGE_BLOCKS + ZONE_HALF_DIAGONAL) * block;
        let sleep = wake + SLEEP_MARGIN_BLOCKS * block;
        // Distance du joueur le plus proche au centre de chaque zone, à l'altitude du joueur
        let mut to_wake = Vec::new();
        let mut keep = HashSet::new();
        // Un groupe dont le centre est loin de tous les joueurs n'a aucune zone assez proche (marge
        // de 1,5 pour l'étirement des cases vers les coins des faces)
        let group_reach = sleep + 1.5 * ZONE_GROUP_HALF_DIAGONAL * block;
        let near_groups = self.zone_groups.iter().filter(|(center, _)| {
            players.iter().any(|p| (*center * p.length()).distance(*p) <= group_reach)
        });
        for &(zone, center) in near_groups.flat_map(|(_, zones)| zones) {
            let nearest = players.iter().map(|p| (center * p.length()).distance(*p)).fold(f32::INFINITY, f32::min);
            if nearest <= sleep && self.awake.contains(&zone) {
                keep.insert(zone);
            } else if nearest <= wake {
                to_wake.push(zone);
            }
        }
        // Une zone qui s'endort reste peuplée, même vide : elle ne sera plus jamais tirée
        for zone in self.awake.difference(&keep) {
            self.sleeping.entry(*zone).or_default();
        }
        self.awake = keep;
        for zone in to_wake {
            let creatures = match self.sleeping.remove(&zone) {
                Some(creatures) => creatures,
                None => self.populate_zone(zone, query),
            };
            for c in creatures {
                match self.registry.find(&c.species) {
                    Some(species) => self.wake_creature(c.id, species, c.herd.unwrap_or(c.id), c.position, c.yaw, c.health, c.tamed, c.age),
                    None => log::warn!("Créature {} : espèce « {} » inconnue, laissée de côté.", c.id, c.species),
                }
            }
            self.awake.insert(zone);
        }

        // Les bêtes hors des zones éveillées s'endorment là où elles sont (les mourantes disparaissent)
        let mut index = 0;
        while index < self.creatures.len() {
            let c = &self.creatures[index];
            let zone = self.zone_of(c.position);
            if c.managed || zone.is_some_and(|z| self.awake.contains(&z)) {
                index += 1;
                continue;
            }
            let c = self.creatures.swap_remove(index);
            let (Some(zone), Some(sleeping), true) = (zone, self.sleeping_creature(&c), c.is_alive()) else { continue };
            if !self.sleeping.contains_key(&zone) {
                let population = self.populate_zone(zone, query);
                self.sleeping.insert(zone, population);
            }
            self.sleeping.get_mut(&zone).unwrap().push(sleeping);
        }
    }

    /// La bête telle qu'elle sera gardée endormie
    fn sleeping_creature(&self, c: &Creature) -> Option<SleepingCreature> {
        if c.summoner.is_some() {
            return None;
        }
        let species = self.registry.get(c.species)?.id.clone();
        Some(SleepingCreature { id: c.id, species, herd: Some(c.herd), position: c.position, yaw: c.yaw, health: c.health, tamed: c.tamed, age: c.age })
    }

    /// Bêtes que la graine du monde destine à cette zone : au plus un groupe, d'une espèce de son
    /// biome, posé sur le relief. Le tirage ne dépend que de la graine et de la zone.
    fn populate_zone(&self, zone: ZoneKey, query: &mut TerrainQuery) -> Vec<SleepingCreature> {
        let mut population = Vec::new();
        if !self.populate {
            return population;
        }
        let mut rng = StdRng::seed_from_u64(zone.seed(self.seed));
        if !rng.gen_bool(ZONE_GROUP_CHANCE) {
            return population;
        }
        // Centre du groupe assez loin du bord pour que tout le groupe reste dans la zone
        let span = GROUP_SPREAD..ZONE_BLOCKS - GROUP_SPREAD;
        let (cx, cz) = (zone.x * ZONE_BLOCKS + rng.gen_range(span.clone()), zone.z * ZONE_BLOCKS + rng.gen_range(span));
        if surface_feet(&self.geometry, query, zone.face, cx, cz).is_none() {
            return population;
        }
        let biome = query.column_sample(zone.face, cx, cz).biome;
        let Some(candidates) = self.spawn_table.get(&biome) else { return population };
        let Some(species) = pick_species(candidates, &mut rng) else { return population };
        let Some(info) = self.registry.get(species) else { return population };

        let group = rng.gen_range(info.min_group..=info.max_group);
        for n in 0..group {
            let (dx, dz) = (rng.gen_range(-GROUP_SPREAD..=GROUP_SPREAD), rng.gen_range(-GROUP_SPREAD..=GROUP_SPREAD));
            let yaw = rng.gen_range(0.0..TAU);
            if let Some(feet) = surface_feet(&self.geometry, query, zone.face, cx + dx, cz + dz) {
                let id = (zone.index() << 16) | n as u64;
                // Le groupe d'une zone forme un troupeau
                let herd = Some(zone.index() << 16);
                population.push(SleepingCreature { id, species: info.id.clone(), herd, position: feet, yaw, health: info.health, tamed: false, age: None });
            }
        }
        population
    }
}

impl CreatureWorld {
    /// Les créatures qui craignent une espèce s'enfuient quand une bête de cette espèce s'approche
    fn scare(&mut self) {
        let radius = FEAR_RADIUS_BLOCKS * self.geometry.block_height as f32;
        let threats: Vec<(usize, Vec3)> = self.creatures.iter().filter(|c| c.is_alive() && !c.hidden).map(|c| (c.species, c.position)).collect();
        for c in &mut self.creatures {
            let fears = &self.fear_table[c.species];
            if fears.is_empty() || !c.is_alive() || c.hidden || c.hunting {
                continue;
            }
            let nearest = threats
                .iter()
                .filter(|(species, p)| fears.contains(species) && p.distance(c.position) <= radius)
                .min_by(|a, b| a.1.distance_squared(c.position).total_cmp(&b.1.distance_squared(c.position)));
            let Some(&(_, threat)) = nearest else { continue };
            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let away = c.position - threat;
            let away = away - up * away.dot(up);
            if away.length_squared() > 1e-12 {
                c.target_yaw = away.dot(east).atan2(away.dot(north)).rem_euclid(TAU);
            }
            c.panic = FEAR_TIME;
            c.panic_speed = PANIC_SPEED;
            c.walking = true;
            c.sleeping = false;
            c.guide = None;
        }
    }

    /// Fait apparaître une créature menée par les villages, dans le troupeau `herd`
    pub(crate) fn spawn_managed(&mut self, species: usize, feet: Vec3, yaw: f32, herd: u64) -> u64 {
        let id = self.next_managed_id;
        self.next_managed_id += 1;
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        self.wake_creature(id, species, herd, feet, yaw, health, false, None);
        if let Some(c) = self.creatures.last_mut() {
            c.managed = true;
            c.timer = 0.0;
        }
        id
    }

    /// Fait apparaître une entité invoquée par le joueur `summoner` (compétence) : apprivoisée,
    /// donc alliée, et oubliée plutôt qu'endormie quand plus personne n'est près
    pub fn summon(&mut self, species: usize, feet: Vec3, yaw: f32, summoner: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let health = self.registry.get(species).map_or(1.0, |s| s.health);
        let herd = id;
        self.wake_creature(id, species, herd, feet, yaw, health, true, None);
        let appear = self.registry.get(species).and_then(|s| s.model.animations.get("apparition")).map_or(0.0, |a| a.cycle);
        if let Some(c) = self.creatures.last_mut() {
            c.summoner = Some(summoner);
            c.appearing = appear;
            c.appear_total = appear;
        }
        id
    }

    /// Donne à la créature invoquée `id` l'allure d'une autre (`CreatureState::copy`)
    pub fn set_copy(&mut self, id: u64, copy: u32) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == id) {
            c.copy = Some(copy);
        }
    }

    /// Tirs de créatures partis depuis le dernier appel, pour les montrer aux joueurs
    pub fn take_shots(&mut self) -> Vec<CreatureShotEvent> {
        std::mem::take(&mut self.shots)
    }

    /// Fait disparaître une entité invoquée (fin de sa durée, ou remplacée par une plus récente)
    pub fn dismiss(&mut self, id: u64) {
        self.creatures.retain(|c| !(c.id == id && c.summoner.is_some()));
    }

    /// Retire une créature (elle rentre chez elle pour de bon, ou s'éloigne de tous les joueurs)
    pub(crate) fn remove(&mut self, id: u64) {
        self.creatures.retain(|c| c.id != id);
    }

    pub(crate) fn creature_mut(&mut self, id: u64) -> Option<&mut Creature> {
        self.creatures.iter_mut().find(|c| c.id == id)
    }

    pub(crate) fn geometry(&self) -> &BlockGeometry {
        &self.geometry
    }

    pub(crate) fn sun_dir(&self) -> Option<Vec3> {
        self.sun_dir
    }

    /// Fiche de l'habitant ou du voyageur qu'incarne la créature `id`, si c'en est un
    pub fn villager_card(&self, id: u64) -> Option<&std::sync::Arc<herocraft_common::VillagerCard>> {
        self.villages.card(id)
    }
}

/// Tempérament d'une bête envers les joueurs : apprivoisée, une bête craintive ne fuit plus, elle
/// regarde
fn tamed_temperament(c: &Creature, temperament: Temperament) -> Temperament {
    match temperament {
        Temperament::Craintif if c.tamed => Temperament::Placide,
        t => t,
    }
}

/// Hauteur du soleil `sun_dir` vue de `p` (sinus de son élévation), ou `None` sans horloge
fn sun_height(sun_dir: Option<Vec3>, p: Vec3) -> Option<f32> {
    sun_dir.map(|sun| sun.dot(p.normalize_or_zero()))
}

/// Pieds d'une créature posée au milieu du dessus de la colonne (x, z), si ce dessus est un sol où
/// elle peut vivre (herbe, terre, sable, neige : ni eau, ni feuillage, ni roche nue)
fn surface_feet(geometry: &BlockGeometry, query: &mut TerrainQuery, face: PlanetFace, x: i32, z: i32) -> Option<Vec3> {
    let top = query.support_top(face, x, z, WORLD_HEIGHT - 1);
    if top <= 0 || top >= WORLD_HEIGHT - 2 {
        return None;
    }
    let surface = query.block(BlockPos::new(face, x, top - 1, z));
    if ![BlockId::GRASS, BlockId::DIRT, BlockId::SAND, BlockId::SNOW].contains(&surface) {
        return None;
    }
    Some(geometry.corner(face, x as f64 + 0.5, top as f64, z as f64 + 0.5).as_vec3())
}

/// Espèce tirée au poids parmi les candidates d'un biome, ou aucune si elle n'a pas eu sa chance
fn pick_species(candidates: &[SpawnCandidate], rng: &mut StdRng) -> Option<usize> {
    let total: f32 = candidates.iter().map(|&(_, weight, _)| weight).sum();
    if total <= 0.0 {
        return None;
    }
    let mut roll = rng.gen_range(0.0..total);
    let &(species, _, chance) = candidates.iter().find(|&&(_, weight, _)| {
        if roll < weight {
            true
        } else {
            roll -= weight;
            false
        }
    })?;
    (chance >= 1.0 || rng.gen_bool(chance.clamp(0.0, 1.0) as f64)).then_some(species)
}

/// Ce qu'une bête sait de son troupeau autour d'elle
#[derive(Clone, Copy, Default)]
struct HerdPull {
    /// Cap vers le centre de ses voisines, et distance à ce centre en blocs
    toward: Option<(f32, f32)>,
    /// Cap moyen des voisines qui marchent
    heading: Option<f32>,
}

/// Pour chaque créature, la position et l'allure des autres bêtes vivantes de son troupeau
fn herd_pulls(creatures: &[Creature], block: f32) -> Vec<HerdPull> {
    let radius = HERD_RADIUS_BLOCKS * block;
    let heading_of = |c: &Creature| {
        let (east, north) = tangent_frame(c.position.normalize_or_zero());
        north * c.yaw.cos() + east * c.yaw.sin()
    };
    creatures
        .iter()
        .map(|c| {
            if !c.is_alive() || c.managed {
                return HerdPull::default();
            }
            let (mut sum, mut count, mut heading, mut walkers) = (Vec3::ZERO, 0, Vec3::ZERO, 0);
            for o in creatures {
                if o.id == c.id || o.herd != c.herd || !o.is_alive() || o.position.distance(c.position) > radius {
                    continue;
                }
                sum += o.position;
                count += 1;
                if o.walking && o.panic <= 0.0 && !o.hunting {
                    heading += heading_of(o);
                    walkers += 1;
                }
            }
            if count == 0 {
                return HerdPull::default();
            }
            let up = c.position.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let yaw_of = |v: Vec3| {
                let v = v - up * v.dot(up);
                (v.length_squared() > 1e-12).then(|| v.dot(east).atan2(v.dot(north)).rem_euclid(TAU))
            };
            let offset = sum / count as f32 - c.position;
            let distance = (offset - up * offset.dot(up)).length() / block;
            HerdPull {
                toward: yaw_of(offset).map(|yaw| (yaw, distance)),
                heading: if walkers > 0 { yaw_of(heading) } else { None },
            }
        })
        .collect()
}

/// Sol sous un point du chemin
#[derive(Clone, Copy, PartialEq, Debug)]
enum Footing {
    Dry,
    Water,
}

/// Ce qu'une créature voit devant elle dans une direction
#[derive(Clone, Copy, Debug)]
struct Path {
    /// Points sondés franchissables d'affilée, de 0 (bloquée tout de suite) à `LOOKAHEAD_STEPS`
    clear: usize,
    /// Une partie du chemin franchissable passe dans l'eau
    wet: bool,
    /// Altitude du sol juste devant le museau, s'il est franchissable
    first: Option<f32>,
}

impl Path {
    /// Note d'un chemin : le plus long d'abord, puis au sec si `avoid_water`
    fn score(&self, avoid_water: bool) -> usize {
        self.clear * 2 + usize::from(!avoid_water || !self.wet)
    }
}

/// Déplacement d'une créature sur le terrain
struct Walker<'q, 'a> {
    geometry: BlockGeometry,
    query: &'q mut TerrainQuery<'a>,
    sun_dir: Option<Vec3>,
    climate: Option<&'q (crate::weather::WeatherModel, f64)>,
}

impl Walker<'_, '_> {
    fn block(&self) -> f32 {
        self.geometry.block_height as f32
    }

    /// Altitude des pieds, en blocs depuis le fond du monde
    fn altitude(&self, p: Vec3) -> f32 {
        ((p.length() as f64 - self.geometry.base_radius) / self.geometry.block_height) as f32
    }

    /// La météo garde la bête couchée : l'orage ou une forte averse (elle se tapit là où elle est,
    /// sauf les espèces toujours éveillées), ou l'hiver pour une espèce qui hiberne
    fn weather_keeps_asleep(&self, p: Vec3, species: &CreatureSpecies) -> bool {
        let Some((model, time)) = self.climate else { return false };
        let altitude = self.altitude(p) as f64 - crate::generator::SEA_LEVEL_BLOCKS;
        let sample = model.sample(p.as_dvec3(), altitude, *time);
        let hibernating = species.hibernation && sample.season == crate::weather::Season::Winter && sample.season_strength > 0.3;
        let sheltering = species.activity != herocraft_common::Activity::Always && (sample.storm > 0.3 || (sample.precipitation > 0.8 && !sample.snow));
        hibernating || sheltering
    }

    /// Altitude du sol sous `p` pour des pieds à `altitude` : dessus du plus haut bloc porteur qui ne
    /// dépasse pas les pieds de plus d'une marche, et la colonne où il est
    fn ground(&mut self, p: Vec3, altitude: f32) -> Option<(BlockPos, f32)> {
        let column = self.geometry.block_at_point(p.as_dvec3())?;
        let from_y = (altitude + STEP_BLOCKS).floor() as i32 - 1;
        let top = self.query.support_top(column.face, column.x, column.z, from_y);
        Some((BlockPos { y: top, ..column }, top as f32))
    }

    /// Rien de solide dans la colonne `column` entre les altitudes `altitude` et `altitude + height`
    /// Les corps solides se repoussent : chaque bête qui bouge sort de ce qu'elle enfonce (une autre
    /// bête, un mur, un joueur), de la part que lui donne la résistance de l'obstacle (entière contre
    /// un obstacle fixe, moitié contre un autre qui bouge)
    fn collide(&mut self, creatures: &mut [Creature], registry: &CreatureRegistry, players: &[Vec3]) {
        let block = self.block();
        // Pieds, cap, corps à l'échelle, fixe
        let bodies: Vec<Option<(Vec3, f32, Collider, bool)>> = creatures
            .iter()
            .map(|c| {
                let species = registry.get(c.species)?;
                if !c.is_alive() || c.hidden || c.vanish.is_some() {
                    return None;
                }
                let mut body = species.collider();
                let scale = c.scale();
                body.half_width *= scale;
                body.half_length *= scale;
                body.height *= scale;
                (body.resistance > 0.0).then_some((c.position, c.yaw, body, species.walk_speed <= 0.0 || c.sleeping))
            })
            .collect();
        for (i, c) in creatures.iter_mut().enumerate() {
            let Some((feet, _, me, fixed)) = bodies[i] else { continue };
            let Some(species) = registry.get(c.species) else { continue };
            // Une bête endormie ne se laisse pas bousculer : on la contourne
            if fixed || c.sleeping {
                continue;
            }
            let mut shift = Vec3::ZERO;
            for (j, other) in bodies.iter().enumerate() {
                let Some((at, yaw, body, anchored)) = *other else { continue };
                if j == i || at.distance(feet) > (body.half_width + body.half_length + me.radius()) * block {
                    continue;
                }
                if let Some(out) = body.separation(at, yaw, block, feet, me.radius(), me.height) {
                    shift += out * body.resistance * if anchored { 1.0 } else { 0.5 };
                }
            }
            for &player in players {
                if let Some(out) = PLAYER_COLLIDER.separation(player, 0.0, block, feet, me.radius(), me.height) {
                    shift += out * PLAYER_COLLIDER.resistance * 0.5;
                }
            }
            if shift != Vec3::ZERO {
                self.nudge(c, species, shift);
            }
        }
    }

    /// Déplace la bête de `offset` dans le plan du sol, si son corps y passe
    fn nudge(&mut self, c: &mut Creature, species: &CreatureSpecies, offset: Vec3) {
        let altitude = self.altitude(c.position);
        let target = (c.position + offset).normalize_or_zero() * c.position.length();
        let free = match self.ground(target, altitude) {
            Some((column, ground)) => ground <= altitude + 1e-3 && self.fits(column, altitude, species.body.height),
            None => false,
        };
        if free {
            c.position = target;
        }
    }

    fn fits(&mut self, column: BlockPos, altitude: f32, height: f32) -> bool {
        let (bottom, top) = ((altitude + 1e-3).floor() as i32, (altitude + height - 1e-3).floor() as i32);
        (bottom..=top).all(|y| {
            let pos = BlockPos { y, ..column };
            !pos.is_valid() || !self.query.block(pos).is_solid()
        })
    }

    /// Réaction de la créature au joueur le plus proche, selon son tempérament. Renvoie vrai si elle
    /// s'en occupe, et laisse alors son humeur de côté.
    fn react(&self, c: &mut Creature, temperament: Temperament, player: PlayerPresence, rng: &mut StdRng) -> bool {
        let flee_radius = if player.crouching { FLEE_RADIUS_CROUCHING_BLOCKS } else { FLEE_RADIUS_BLOCKS };
        let player = player.feet;
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let towards = player - c.position;
        let towards = towards - up * towards.dot(up);
        let distance = towards.length() / self.block();
        // Joueur juste au-dessus ou au-dessous d'elle : pas de direction où regarder ni fuir
        if distance < 0.25 {
            return false;
        }
        let yaw_to_player = towards.dot(east).atan2(towards.dot(north)).rem_euclid(TAU);
        match temperament {
            Temperament::Indifferent => false,
            Temperament::Craintif if distance < flee_radius => {
                // À l'opposé du joueur, un peu de travers pour ne pas filer toutes en ligne
                c.panic = FLEE_TIME;
                c.panic_speed = PANIC_SPEED;
                c.walking = true;
                c.target_yaw = (yaw_to_player + PI + rng.gen_range(-0.4..0.4)).rem_euclid(TAU);
                true
            }
            Temperament::Placide if distance < WATCH_RADIUS_BLOCKS => {
                c.walking = false;
                c.target_yaw = yaw_to_player;
                // Une fois le joueur parti, elle reprend vite sa promenade
                c.timer = c.timer.min(1.5);
                true
            }
            Temperament::Curieux if distance < FOLLOW_RADIUS_BLOCKS => {
                // Elle s'arrête près de lui, et ne repart que s'il s'éloigne franchement
                let stop = if c.walking { FOLLOW_STOP_BLOCKS } else { FOLLOW_RESUME_BLOCKS };
                c.walking = distance > stop;
                c.target_yaw = yaw_to_player;
                c.timer = c.timer.min(1.5);
                true
            }
            _ => false,
        }
    }

    /// Marche vers `goal` et s'arrête à `stop` blocs de lui (dans le plan tangent). Renvoie la
    /// distance en blocs, ou rien si `goal` est juste au-dessus ou au-dessous d'elle.
    fn approach(&self, c: &mut Creature, goal: Vec3, stop: f32) -> Option<f32> {
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let towards = goal - c.position;
        let towards = towards - up * towards.dot(up);
        let distance = towards.length() / self.block();
        if distance < 0.25 {
            c.walking = false;
            return None;
        }
        c.target_yaw = towards.dot(east).atan2(towards.dot(north)).rem_euclid(TAU);
        // Elle ne repart que si le but s'éloigne franchement
        let stop = if c.walking { stop } else { stop + 1.0 };
        c.walking = distance > stop;
        c.pause = None;
        c.timer = c.timer.min(1.5);
        Some(distance)
    }

    /// Où poser les pieds à la place de `p`, en venant de pieds à `altitude` : rien si un mur
    /// barre la route, si c'est une falaise, de la lave, ou si le corps n'y tient pas
    fn footing(&mut self, p: Vec3, altitude: f32, height: f32) -> Option<(f32, Footing)> {
        let (column, ground) = self.ground(p, altitude)?;
        if ground < altitude - MAX_DROP_BLOCKS {
            return None;
        }
        let below = self.query.block(BlockPos { y: column.y - 1, ..column });
        if below == BlockId::LAVA || !self.fits(column, ground, height) {
            return None;
        }
        Some((ground, if below.is_liquid() { Footing::Water } else { Footing::Dry }))
    }

    /// Sonde le chemin depuis `from` (pieds à `altitude`) dans la direction `heading`, à partir de
    /// `reach` blocs devant le centre du corps
    fn path(&mut self, from: Vec3, altitude: f32, heading: Vec3, reach: f32, height: f32) -> Path {
        let block = self.block();
        let mut path = Path { clear: 0, wet: false, first: None };
        let mut alt = altitude;
        for k in 0..LOOKAHEAD_STEPS {
            let p = from + heading * (reach + k as f32 * PROBE_SPACING) * block;
            let Some((ground, footing)) = self.footing(p, alt, height) else { break };
            if k == 0 {
                path.first = Some(ground);
            }
            alt = ground;
            path.wet |= footing == Footing::Water;
            path.clear += 1;
        }
        path
    }

    /// Nouveau cap pour contourner ce qui gêne sur le chemin `current` : le plus petit écart qui
    /// mène plus loin (ou au sec), du côté habituel d'abord. Bloquée de partout, elle fait demi-tour.
    #[allow(clippy::too_many_arguments)]
    fn detour(&mut self, c: &mut Creature, altitude: f32, reach: f32, height: f32, current: Path, avoid_water: bool, rng: &mut StdRng) -> Option<f32> {
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let perfect = LOOKAHEAD_STEPS * 2 + 1;
        // (écart signé, note) du meilleur cap trouvé : il doit faire mieux que le cap actuel
        let mut best: Option<(f32, usize)> = None;
        'search: for angle in DETOUR_ANGLES {
            for side in [c.detour_side, -c.detour_side] {
                let yaw = c.yaw + side * angle;
                let score = self.path(c.position, altitude, north * yaw.cos() + east * yaw.sin(), reach, height).score(avoid_water);
                if score > best.map_or(current.score(avoid_water), |(_, s)| s) {
                    best = Some((side * angle, score));
                    if score == perfect {
                        break 'search;
                    }
                }
            }
        }
        match best {
            Some((offset, _)) => {
                c.detour_side = offset.signum();
                Some((c.yaw + offset).rem_euclid(TAU))
            }
            // Cul-de-sac : demi-tour, un peu de travers, et elle essaiera l'autre côté la prochaine fois
            None if current.clear == 0 => {
                c.detour_side = -c.detour_side;
                Some((c.yaw + PI + rng.gen_range(-0.8..0.8)).rem_euclid(TAU))
            }
            None => None,
        }
    }

    /// Cap vers la rive la plus proche où elle peut sortir de l'eau, depuis des pieds à `altitude`
    fn shore_yaw(&mut self, from: Vec3, altitude: f32, height: f32) -> Option<f32> {
        let up = from.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let block = self.block();
        for r in 1..=SHORE_RANGE_BLOCKS {
            for i in 0..SHORE_DIRECTIONS {
                let yaw = i as f32 * TAU / SHORE_DIRECTIONS as f32;
                let p = from + (north * yaw.cos() + east * yaw.sin()) * r as f32 * block;
                if matches!(self.footing(p, altitude, height), Some((_, Footing::Dry))) {
                    return Some(yaw);
                }
            }
        }
        None
    }

    /// Une créature menée par les villages : elle marche vers son but, ou, arrêtée, regarde ce
    /// qu'on lui a dit de regarder (sinon le joueur qui passe, selon son tempérament)
    fn follow_guide(&self, c: &mut Creature, species: &CreatureSpecies, nearest_player: Option<PlayerPresence>, rng: &mut StdRng, dt: f32) {
        c.timer -= dt;
        let up = c.position.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let flat_to = |dir: Vec3| {
            let to = dir * c.position.length() - c.position;
            to - up * to.dot(up)
        };
        let yaw_of = |v: Vec3| v.dot(east).atan2(v.dot(north)).rem_euclid(TAU);
        if c.sleeping {
            c.walking = false;
            return;
        }
        if let Some(dir) = c.guide {
            let flat = flat_to(dir);
            if flat.length() / self.block() < GUIDE_ARRIVAL_BLOCKS {
                c.guide = None;
                c.arrived = true;
                c.walking = false;
                c.pace = 1.0;
            } else {
                let was_walking = c.walking;
                c.walking = true;
                c.pace = c.guide_pace;
                c.pause = None;
                if c.timer <= 0.0 || !was_walking {
                    c.target_yaw = yaw_of(flat);
                    c.timer = GUIDE_AIM_INTERVAL;
                }
            }
            return;
        }
        c.walking = false;
        match c.look {
            Some(dir) => {
                let flat = flat_to(dir.normalize_or_zero());
                if flat.length() / self.block() > 0.05 {
                    c.target_yaw = yaw_of(flat);
                }
            }
            None => {
                if let Some(player) = nearest_player {
                    self.react(c, species.temperament, player, rng);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn step(&mut self, c: &mut Creature, species: &CreatureSpecies, food: &[ItemId], nearest_player: Option<PlayerPresence>, herd: HerdPull, rng: &mut StdRng, dt: f32) {
        let block = self.block();
        let base_radius = self.geometry.base_radius as f32;
        let radius_of = move |altitude: f32| base_radius + altitude * block;

        if c.hidden {
            return;
        }
        c.hurt = (c.hurt - dt).max(0.0);
        if let Some(t) = c.dead_for.as_mut() {
            *t += dt;
        } else if c.hunting {
            // La chasse décide du cap et de l'allure
        } else if c.panic > 0.0 {
            // Fuite : elle court droit devant, jusqu'à se calmer
            c.panic -= dt;
            if c.panic <= 0.0 {
                c.start_pause(species, rng);
            }
        } else if c.swimming {
            // À l'eau : elle nage vers la rive la plus proche, qu'elle cherche de temps en temps (on ne
            // dort pas en nageant)
            c.sleeping = false;
            c.walking = true;
            c.timer -= dt;
            if c.timer <= 0.0 {
                c.timer = SHORE_SEARCH_INTERVAL;
                let altitude = self.altitude(c.position);
                if let Some(yaw) = self.shore_yaw(c.position, altitude, species.body.height) {
                    c.target_yaw = yaw;
                }
            }
        } else if c.managed {
            self.follow_guide(c, species, nearest_player, rng, dt);
        } else if let Some(mate) = c.mate.filter(|_| !c.sleeping) {
            // Amoureuse : elle va droit vers son partenaire
            self.approach(c, mate, 0.0);
        } else if let Some(player) = nearest_player.filter(|p| {
            !c.sleeping && p.holding.is_some_and(|item| food.contains(&item)) && p.feet.distance(c.position) < LURE_RADIUS_BLOCKS * block
        }) {
            // Le joueur tient sa nourriture : elle le suit au lieu de fuir
            self.approach(c, player.feet, LURE_STOP_BLOCKS);
        } else if !c.sleeping && nearest_player.is_some_and(|player| self.react(c, tamed_temperament(c, species.temperament), player, rng)) {
            // Éveillée, elle s'occupe du joueur ; endormie, elle ne le remarque pas (un coup la réveille)
        } else {
            // Humeur : marcher un moment dans une direction, puis s'arrêter brouter. Au sein d'un
            // troupeau, elle revient vers les autres si elle s'en écarte, et suit celles qui marchent.
            // Hors de ses heures, elle s'endort là où elle est, et au retour de ses heures se réveille.
            // Chacune le fait à la fin de son humeur du moment : un troupeau s'endort et s'éveille peu
            // à peu.
            let gap = herd.toward.map_or(0.0, |(_, distance)| distance);
            let straying = gap > HERD_STRAY_BLOCKS && !c.sleeping;
            c.timer -= dt;
            // Seule loin des autres, elle n'attend pas la fin de sa pause pour les rejoindre
            if straying && !c.walking {
                c.timer = c.timer.min(0.5);
            }
            if c.timer <= 0.0 {
                let awake = sun_height(self.sun_dir, c.position).is_none_or(|height| species.activity.is_awake(height))
                    && !self.weather_keeps_asleep(c.position, species);
                // Au repos, elle emboîte volontiers le pas des voisines qui marchent
                let follow = herd.heading.filter(|_| awake && !c.sleeping && !c.walking && rng.gen_bool(HERD_FOLLOW_CHANCE));
                if !awake {
                    c.sleeping = true;
                    c.walking = false;
                    // Couchée : l'animation `sommeil` de sa fiche, si elle en a une
                    c.pause = species.pauses().position(|(name, _)| name == "sommeil").map(|i| i as u8);
                    c.target_yaw = c.yaw;
                    c.timer = rng.gen_range(SLEEP_CHECK.0..SLEEP_CHECK.1);
                } else if c.sleeping {
                    // Elle s'étire un moment avant de repartir
                    c.sleeping = false;
                    c.walking = false;
                    c.pause = None;
                    c.timer = rng.gen_range(1.0..3.0);
                } else if !straying && follow.is_none() && (c.walking || rng.gen_bool(0.4)) {
                    c.start_pause(species, rng);
                    if rng.gen_bool(0.5) {
                        c.target_yaw = c.yaw + rng.gen_range(-1.0..1.0);
                    }
                } else {
                    c.walking = true;
                    c.timer = rng.gen_range(1.0..4.0);
                    c.target_yaw = match (herd.toward, follow) {
                        // Écartée : droit vers le centre du troupeau
                        (Some((toward, distance)), _) if distance > HERD_LOOSE_BLOCKS => toward + rng.gen_range(-0.3..0.3),
                        // Dans le troupeau : le cap de celles qui marchent
                        (_, Some(heading)) => heading + rng.gen_range(-0.4..0.4),
                        _ => c.yaw + rng.gen_range(-PI / 2.0..PI / 2.0),
                    };
                }
                c.target_yaw = c.target_yaw.rem_euclid(TAU);
            }
        }

        // Elle se tourne peu à peu vers le cap visé, par le plus court
        let turn = angle_between(c.yaw, c.target_yaw);
        let max_turn = TURN_SPEED * dt;
        c.yaw = (c.yaw + turn.clamp(-max_turn, max_turn)).rem_euclid(TAU);
        let turn = angle_between(c.yaw, c.target_yaw);

        // Pesanteur : posée sur le sol, poussée dessus si un bloc l'a recouverte, sinon elle saute ou tombe
        let up = c.position.normalize_or_zero();
        let mut altitude = self.altitude(c.position);
        let Some((column, ground)) = self.ground(c.position, altitude) else { return };
        if altitude <= ground + 1e-4 && c.vertical_speed <= 0.0 {
            altitude = ground;
            c.vertical_speed = 0.0;
            if let Some(flight) = c.flight.take() {
                c.landed = Some(((flight.peak - ground).max(0.0), flight.slam));
            }
        } else {
            c.vertical_speed -= GRAVITY * dt;
            let next = altitude + c.vertical_speed * dt;
            if c.vertical_speed > 0.0 && !self.fits(column, next, species.body.height) {
                // La tête cogne un plafond
                c.vertical_speed = 0.0;
            } else {
                altitude = next.max(ground);
            }
            if altitude <= ground {
                c.vertical_speed = 0.0;
            }
        }
        c.position = up * radius_of(altitude);
        if let Some(flight) = c.flight.as_mut() {
            // En vol, elle ne marche ni ne frappe
            flight.peak = flight.peak.max(altitude);
            c.stun = c.stun.max(0.1);
            if c.vertical_speed == 0.0 && altitude <= ground + 1e-4 {
                let flight = c.flight.take().unwrap();
                c.landed = Some(((flight.peak - ground).max(0.0), flight.slam));
            }
        }
        let on_ground = altitude <= ground + 1e-4 && c.vertical_speed <= 0.0;
        let swimming = on_ground && self.query.block(BlockPos { y: column.y - 1, ..column }).is_liquid();
        if swimming && !c.swimming {
            // Tout juste à l'eau : elle cherche la rive sans attendre
            c.timer = 0.0;
        }
        c.swimming = swimming;
        c.sink = if swimming { (species.body.height * SWIM_SINK).min(1.0) * block } else { 0.0 };

        // Déplacement dans le plan : recul d'un coup, et marche (ralentie le temps de se tourner)
        let (east, north) = tangent_frame(up);
        c.push = (c.push - up * c.push.dot(up)) * (-KNOCKBACK_DAMPING * dt).exp();
        if c.push.length_squared() < 1e-4 {
            c.push = Vec3::ZERO;
        }
        let mut velocity = c.push;
        let pace = if c.panic > 0.0 { c.panic_speed } else { c.pace } * if c.swimming { SWIM_SPEED } else { 1.0 };
        let status = if c.stun > 0.0 { 0.0 } else { 1.0 - c.slow };
        let speed = species.walk_speed * pace * status * turn.cos().max(0.0);
        if c.walking && c.dead_for.is_none() && speed > 0.0 {
            // Elle regarde quelques pas devant elle : le sol juste devant le museau décide si elle
            // avance, la suite si elle doit déjà obliquer pour contourner un mur, une falaise ou l'eau
            let heading = north * c.yaw.cos() + east * c.yaw.sin();
            let reach = speed * dt + species.body.half_length;
            let path = self.path(c.position, altitude, heading, reach, species.body.height);
            // Au calme et à pied sec, elle longe l'eau plutôt que d'y entrer, si elle le peut
            let avoid_water = c.panic <= 0.0 && !c.swimming;
            if turn.abs() < FACING_TOLERANCE && path.score(avoid_water) < LOOKAHEAD_STEPS * 2 + 1 {
                if let Some(yaw) = self.detour(c, altitude, reach, species.body.height, path, avoid_water, rng) {
                    c.target_yaw = yaw;
                }
            }
            if let Some(ahead) = path.first {
                // Une marche devant : elle saute dessus (et sort ainsi de l'eau)
                if on_ground && ahead > altitude + 1e-3 {
                    c.vertical_speed = JUMP_SPEED;
                }
                velocity += heading * speed;
            }
        }
        if velocity == Vec3::ZERO {
            return;
        }
        // Elle n'entre dans une colonne que si son corps y passe à son altitude : une marche se
        // franchit en sautant par-dessus
        let target = (c.position + velocity * dt * block).normalize_or_zero() * radius_of(altitude);
        let free = match self.ground(target, altitude) {
            Some((column, ground)) => {
                let below = self.query.block(BlockPos { y: column.y - 1, ..column });
                ground <= altitude + 1e-3 && below != BlockId::LAVA && self.fits(column, altitude, species.body.height)
            }
            None => false,
        };
        if free {
            c.position = target;
        } else {
            c.push = Vec3::ZERO;
        }
    }
}

/// Angle signé pour aller du cap `from` au cap `to` par le plus court, dans [-π, π]
fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::edits::FACE_BLOCKS;
    use std::collections::HashSet;

    fn registry() -> Arc<CreatureRegistry> {
        Arc::new(CreatureRegistry::load_default())
    }

    fn generator() -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed: 4242, ..Default::default() })
    }

    /// Une colonne d'herbe de la face +Z, et les pieds d'une créature posée dessus
    fn grass_column(generator: &PlanetWorldGenerator, edits: &TerrainEdits) -> (BlockPos, Vec3) {
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(generator, edits);
        // Dans les prairies et les forêts, où vivent les bêtes de tous les tests
        (16..FACE_BLOCKS - 16)
            .step_by(23)
            .flat_map(|x| (16..FACE_BLOCKS - 16).step_by(23).map(move |z| (x, z)))
            .filter(|&(x, z)| matches!(generator.sample_block_column(PlanetFace::PosZ, x, z).biome, BiomeType::Plains | BiomeType::Forest))
            .find_map(|(x, z)| {
                let feet = surface_feet(&geometry, &mut query, PlanetFace::PosZ, x, z)?;
                let top = query.support_top(PlanetFace::PosZ, x, z, WORLD_HEIGHT - 1);
                // Terrain plat autour : les voisines à la même hauteur
                let flat = [(1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2)]
                    .iter()
                    .all(|(dx, dz)| query.support_top(PlanetFace::PosZ, x + dx, z + dz, WORLD_HEIGHT - 1) == top);
                flat.then_some((BlockPos::new(PlanetFace::PosZ, x, top, z), feet))
            })
            .expect("une colonne d'herbe sur terrain plat")
    }

    fn altitude(geometry: &BlockGeometry, p: Vec3) -> f32 {
        ((p.length() as f64 - geometry.base_radius) / geometry.block_height) as f32
    }

    #[test]
    fn the_zones_around_a_player_are_populated_and_creatures_walk_on_the_ground() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        // Une minute de jeu à 20 images par seconde
        let mut starts = HashMap::new();
        for _ in 0..1200 {
            world.tick(0.05, &generator, &edits, &[player]);
            for c in world.creatures() {
                starts.entry(c.id).or_insert(c.position);
            }
        }
        assert!(!world.creatures().is_empty(), "aucune créature n'est apparue");
        let mut query = TerrainQuery::new(&generator, &edits);
        let mut moved = 0;
        for c in world.creatures() {
            let block = c.position.distance(player) / geometry.block_height as f32;
            assert!(block <= CREATURE_RANGE_BLOCKS + ZONE_BLOCKS as f32 * 2.0 + SLEEP_MARGIN_BLOCKS, "{} à {block} blocs", c.species);
            // Debout sur le sol de sa colonne (ou en train d'y tomber), jamais dedans
            let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
            let alt = altitude(&geometry, c.position);
            let ground = query.support_top(column.face, column.x, column.z, (alt + 1.0).floor() as i32 - 1) as f32;
            assert!(alt >= ground - 1e-3, "{} enfoncée dans le sol ({alt} < {ground})", c.species);
            assert!(!query.block(BlockPos { y: (alt + 1e-2).floor() as i32, ..column }).is_solid(), "{} dans un bloc ({alt})", c.species);
            if starts[&c.id].distance(c.position) > geometry.block_height as f32 {
                moved += 1;
            }
        }
        assert!(moved > 0, "aucune créature n'a bougé");
    }

    #[test]
    fn creatures_fall_asleep_far_from_every_player() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 1, registry());
        world.populate = false;
        let sheep = world.registry().find("vache").unwrap();
        world.spawn(sheep, feet);
        world.tick(0.05, &generator, &edits, &[feet]);
        assert_eq!(world.creatures().len(), 1);
        world.tick(0.05, &generator, &edits, &[-feet]);
        assert!(world.creatures().is_empty());
    }

    #[test]
    fn a_wall_stops_a_creature_and_a_hole_makes_it_fall() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        world.populate = false;
        let boar = world.registry().find("sanglier").unwrap();
        world.spawn(boar, feet);
        // Le joueur reste hors de la vue du sanglier, qui le chargerait
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 30.0 * geometry.block_height as f32;

        // Mur de deux blocs tout autour : le sanglier ne passe pas, quel que soit son cap
        for (dx, dz) in [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)] {
            for dy in 0..2 {
                assert!(edits.set_block(&generator, BlockPos { x: ground.x + dx, y: ground.y + dy, z: ground.z + dz, ..ground }, BlockId::STONE));
            }
        }
        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let c = &world.creatures()[0];
        assert_eq!(geometry.block_at_point(c.position.as_dvec3()).map(|p| (p.x, p.z)), Some((ground.x, ground.z)));

        // Le sol creusé sous lui : il tombe d'un bloc
        assert!(edits.set_block(&generator, BlockPos { y: ground.y - 1, ..ground }, BlockId::AIR));
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let alt = altitude(&geometry, world.creatures()[0].position);
        assert!((alt - (ground.y - 1) as f32).abs() < 1e-2, "altitude {alt} au lieu de {}", ground.y - 1);
    }

    #[test]
    fn a_launched_beast_flies_stunned_then_a_slam_hurts_it_on_landing() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        world.populate = false;
        let cow = world.registry().find("cow").or_else(|| world.registry().find("vache")).unwrap();
        world.spawn(cow, feet);
        let id = world.creatures()[0].id;
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 30.0 * geometry.block_height as f32;
        let health = |w: &CreatureWorld| w.creatures()[0].health;
        let full = health(&world);

        // Projetée à 4 blocs : elle monte, étourdie tant qu'elle vole
        world.launch(id, 4.0);
        let mut peak = 0.0f32;
        for _ in 0..12 {
            world.tick(0.05, &generator, &edits, &[player]);
            peak = peak.max(altitude(&geometry, world.creatures()[0].position) - ground.y as f32);
            assert!(world.creatures()[0].stun > 0.0, "étourdie en vol");
        }
        assert!((3.5..4.5).contains(&peak), "montée à {peak} blocs");
        assert!(world.is_airborne(id));

        // Rabattue : elle tombe aussitôt et prend le coup à l'atterrissage
        world.slam(id, 9, 20.0);
        for _ in 0..10 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        assert!(!world.is_airborne(id), "posée au sol");
        let alt = altitude(&geometry, world.creatures()[0].position);
        assert!((alt - ground.y as f32).abs() < 0.05, "altitude {alt}");
        let lost = full - health(&world);
        assert!(lost >= 20.0, "le coup rabattu blesse : {lost} points");
    }

    /// Cap qui mène du centre de la colonne `from` vers celui de `to`
    fn yaw_towards(geometry: &BlockGeometry, from: Vec3, to: BlockPos) -> f32 {
        let target = geometry.corner(to.face, to.x as f64 + 0.5, to.y as f64, to.z as f64 + 0.5).as_vec3();
        let up = from.normalize();
        let (east, north) = tangent_frame(up);
        let d = target - from;
        d.dot(east).atan2(d.dot(north)).rem_euclid(TAU)
    }

    #[test]
    fn a_creature_turns_progressively() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        // Le joueur assez loin pour qu'elles ne le remarquent pas, et ne le bousculent pas
        let far = feet + tangent_frame(feet.normalize()).0 * 20.0 * BlockGeometry::new(generator.base_radius()).block_height as f32;
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        world.populate = false;
        let cow = world.registry().find("vache").unwrap();
        world.spawn(cow, feet);
        let c = &mut world.creatures[0];
        (c.yaw, c.target_yaw, c.timer) = (0.2, 0.2 + PI * 0.9, 100.0);

        let mut yaws = vec![0.2];
        for _ in 0..30 {
            world.tick(0.05, &generator, &edits, &[far]);
            yaws.push(world.creatures()[0].yaw);
        }
        for pair in yaws.windows(2) {
            let turn = angle_between(pair[0], pair[1]);
            assert!((0.0..=TURN_SPEED * 0.05 + 1e-4).contains(&turn), "demi-tour d'un coup : {turn} rad en une image");
        }
        assert!(angle_between(*yaws.last().unwrap(), 0.2 + PI * 0.9).abs() < 1e-3, "cap jamais atteint");
    }

    #[test]
    fn a_creature_jumps_onto_a_step() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        world.populate = false;
        let boar = world.registry().find("sanglier").unwrap();
        world.spawn(boar, feet);
        // Une marche d'un bloc devant (un mur derrière en ferait un cul-de-sac, qu'il contournerait)
        let step = BlockPos { x: ground.x + 1, ..ground };
        assert!(edits.set_block(&generator, step, BlockId::STONE));
        let c = &mut world.creatures[0];
        c.yaw = yaw_towards(&geometry, feet, step);
        (c.target_yaw, c.walking, c.timer) = (c.yaw, true, 100.0);
        // Le joueur reste hors de la vue du sanglier, qui le chargerait
        let (east, _) = tangent_frame(feet.normalize());
        let feet = feet - east * 30.0 * geometry.block_height as f32;

        let base = ground.y as f32;
        let (mut highest, mut midair, mut on_step) = (base, 0, false);
        for _ in 0..80 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let position = world.creatures()[0].position;
            let alt = altitude(&geometry, position);
            highest = highest.max(alt);
            if alt > base + 0.05 && (alt - base - 1.0).abs() > 0.05 {
                midair += 1;
            }
            let column = geometry.block_at_point(position.as_dvec3()).unwrap();
            on_step |= (column.x, column.z) == (step.x, step.z) && (alt - (base + 1.0)).abs() < 1e-2;
        }
        assert!(on_step, "il n'est pas monté sur la marche");
        assert!(highest > base + 1.05, "pas de saut au-dessus de la marche ({highest})");
        assert!(midair >= 3, "montée d'un coup, sans saut ({midair} images en l'air)");
    }

    #[test]
    fn the_action_follows_what_the_creature_does() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 13, registry());
        let cow = world.registry().find("vache").unwrap();
        let species = world.registry().get(cow).unwrap().clone();
        let id = world.spawn(cow, feet);
        assert_eq!(world.creatures()[0].state().action, CreatureAction::Repos);

        // À l'arrêt, elle prend les inactivités de sa fiche, chacune le temps prévu. Le joueur se
        // tient loin : la vache, placide, s'arrêterait sinon pour le regarder.
        let (east, _) = tangent_frame(feet.normalize());
        let far = feet + east * 30.0 * BlockGeometry::new(generator.base_radius()).block_height as f32;
        let mut seen = std::collections::HashSet::new();
        let mut walked = false;
        for _ in 0..6000 {
            world.tick(0.05, &generator, &edits, &[far]);
            let c = &world.creatures()[0];
            match c.state().action {
                CreatureAction::Pause(i) => {
                    let (_, rule) = species.pauses().nth(i as usize).expect("inactivité de la fiche");
                    assert!(c.timer <= rule.duree[1] + 1e-3);
                    seen.insert(i);
                }
                CreatureAction::Marche => walked = true,
                other => assert_eq!(other, CreatureAction::Repos),
            }
        }
        assert!(walked, "elle n'a jamais marché");
        assert!(seen.len() >= 3, "trop peu d'inactivités ({seen:?})");

        // Frappée, elle court
        assert!(world.hit(id, world.creatures()[0].position - east));
        assert_eq!(world.creatures()[0].state().action, CreatureAction::Course);
    }

    #[test]
    fn a_hit_hurts_pushes_and_finally_kills() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 11, registry());
        world.populate = false;
        let rabbit = world.registry().find("lapin").unwrap();
        let id = world.spawn(rabbit, feet);
        assert!(!world.hit(id + 1, feet), "pas de créature à toucher");

        // Frappée depuis l'ouest : elle rougit et part vers l'est
        let (east, _) = tangent_frame(feet.normalize());
        let attacker = feet - east * 2.0 * geometry.block_height as f32;
        assert!(world.hit(id, attacker));
        assert_eq!(world.creatures()[0].state().hurt, 1.0);
        for _ in 0..10 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let c = &world.creatures()[0];
        let moved = (c.position - feet).dot(east) / geometry.block_height as f32;
        assert!(moved > 0.5, "recul de {moved} bloc seulement");
        assert!(c.state().hurt < 1.0 && c.state().death == 0.0);

        // Assez de coups : elle bascule, puis disparaît
        let health = world.registry().get(rabbit).unwrap().health;
        let hits = (health / PLAYER_HIT_DAMAGE).ceil() as usize;
        for _ in 1..hits - 1 {
            assert!(world.hit(id, attacker));
        }
        assert_eq!(world.skin(id), None, "une bête vivante ne se dépèce pas");
        assert!(world.hit(id, attacker));
        assert!(world.creatures()[0].state().death > 0.0);
        assert!(!world.hit(id, attacker), "une bête morte ne se frappe plus");
        for _ in 0..((CREATURE_DEATH_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        // Elle reste à terre, couchée, jusqu'à ce qu'on la dépèce
        assert_eq!(world.creatures()[0].state().death, 1.0);
        let at = world.creatures()[0].position;
        let (species, feet_at) = world.skin(id).expect("bête morte à dépecer");
        assert_eq!((species, feet_at), (rabbit, at));
        assert!(world.creatures().is_empty());
        assert_eq!(world.skin(id), None, "déjà dépecée");
    }

    /// Une bête morte qu'on ne dépèce pas disparaît au bout de `CORPSE_TIME`
    #[test]
    fn an_unskinned_corpse_disappears_after_a_while() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 11, registry());
        world.populate = false;
        let rabbit = world.registry().find("lapin").unwrap();
        let id = world.spawn(rabbit, feet);
        while world.hit(id, feet) {}
        let mut elapsed = 0.0;
        while elapsed < CORPSE_TIME - 1.0 {
            world.tick(MAX_STEP, &generator, &edits, &[feet]);
            elapsed += MAX_STEP;
        }
        assert_eq!(world.creatures().len(), 1, "encore à terre après {elapsed} s");
        for _ in 0..((2.0 / MAX_STEP) as usize + 2) {
            world.tick(MAX_STEP, &generator, &edits, &[feet]);
        }
        assert!(world.creatures().is_empty());
    }

    /// Distance dans le plan tangent entre deux pieds, en blocs
    fn planar_blocks(geometry: &BlockGeometry, a: Vec3, b: Vec3) -> f32 {
        let up = a.normalize();
        let d = b - a;
        (d - up * d.dot(up)).length() / geometry.block_height as f32
    }

    /// Une créature de l'espèce `id` posée sur l'herbe, et un joueur à `blocks` blocs à l'est d'elle
    fn creature_and_player(id: &str, blocks: f32, seed: u64) -> (PlanetWorldGenerator, TerrainEdits, BlockGeometry, CreatureWorld, Vec3) {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), seed, registry());
        let species = world.registry().find(id).unwrap();
        world.spawn(species, feet);
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet + east * blocks * geometry.block_height as f32;
        (generator, edits, geometry, world, player)
    }

    #[test]
    fn bundled_species_have_their_temperament() {
        let registry = registry();
        let temperament = |id: &str| registry.get(registry.find(id).unwrap()).unwrap().temperament;
        for id in ["lapin", "cerf", "faisan"] {
            assert_eq!(temperament(id), Temperament::Craintif, "{id}");
        }
        assert_eq!(temperament("vache"), Temperament::Placide);
    }

    #[test]
    fn a_shy_creature_flees_an_approaching_player() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("lapin", 4.0, 13);
        let start = planar_blocks(&geometry, world.creatures()[0].position, player);
        let mut fled = false;
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
            fled |= world.creatures()[0].walking && world.creatures()[0].panic > 0.0;
        }
        let end = planar_blocks(&geometry, world.creatures()[0].position, player);
        assert!(fled, "le lapin n'a pas détalé");
        assert!(end > start + 1.5, "le lapin ne s'est pas éloigné ({start} → {end} blocs)");
    }

    #[test]
    fn a_crouching_player_can_approach_a_shy_creature() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("lapin", 5.0, 29);
        let c = &mut world.creatures[0];
        (c.walking, c.timer) = (false, 100.0);
        let start = world.creatures()[0].position;
        let crouching = PlayerPresence { feet: player, crouching: true, holding: None, hidden: false };
        for _ in 0..40 {
            world.tick_with_presences(0.05, &generator, &edits, &[crouching]);
        }
        let c = &world.creatures()[0];
        assert!(c.panic == 0.0 && !c.walking, "le lapin a remarqué le joueur accroupi à 5 blocs");
        assert!(planar_blocks(&geometry, start, c.position) < 0.1);

        // Accroupi tout près, il finit par le remarquer
        let (east, _) = tangent_frame(c.position.normalize());
        let close = PlayerPresence { feet: c.position + east * 2.0 * geometry.block_height as f32, crouching: true, holding: None, hidden: false };
        world.tick_with_presences(0.05, &generator, &edits, &[close]);
        assert!(world.creatures()[0].panic > 0.0, "le lapin ne fuit pas un joueur accroupi à 2 blocs");
    }

    #[test]
    fn a_placid_creature_stops_and_watches_the_player() {
        let (generator, edits, geometry, mut world, player) = creature_and_player("vache", 4.0, 17);
        let c = &mut world.creatures[0];
        (c.yaw, c.target_yaw, c.walking, c.timer) = (PI, PI, true, 100.0);
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let c = &world.creatures()[0];
        assert!(!c.walking, "la vache continue sa promenade");
        let up = c.position.normalize();
        let (east, north) = tangent_frame(up);
        let d = player - c.position;
        let towards = d.dot(east).atan2(d.dot(north));
        assert!(angle_between(c.yaw, towards).abs() < 0.05, "la vache ne regarde pas le joueur");
        assert!(planar_blocks(&geometry, c.position, player) > 3.0, "la vache est venue vers le joueur");
    }

    #[test]
    fn a_curious_creature_follows_the_player_and_keeps_its_distance() {
        // Aucune espèce livrée n'est curieuse : le chien l'est le temps du test
        let species = registry()
            .iter()
            .cloned()
            .map(|mut s| {
                if s.id == "chien" {
                    s.temperament = Temperament::Curieux;
                }
                s
            })
            .collect();
        let registry = Arc::new(CreatureRegistry::from_species(species, Default::default()));
        let (generator, edits, geometry, _, player) = creature_and_player("chien", 10.0, 19);
        let mut world = CreatureWorld::new(generator.base_radius(), 19, registry);
        let (_, feet) = grass_column(&generator, &edits);
        world.spawn(world.registry().find("chien").unwrap(), feet);
        let mut closest = f32::MAX;
        for _ in 0..120 {
            world.tick(0.05, &generator, &edits, &[player]);
            closest = closest.min(planar_blocks(&geometry, world.creatures()[0].position, player));
        }
        let end = planar_blocks(&geometry, world.creatures()[0].position, player);
        assert!(end < 6.0, "le chien n'a pas suivi le joueur ({end} blocs)");
        assert!(closest > 2.0, "le chien colle le joueur ({closest} blocs)");
        assert!(!world.creatures()[0].walking, "le chien ne s'arrête pas près du joueur");
    }

    #[test]
    fn an_indifferent_creature_ignores_the_player() {
        let (generator, edits, _, mut world, player) = creature_and_player("lezard", 2.0, 23);
        let c = &mut world.creatures[0];
        (c.walking, c.timer) = (false, 100.0);
        for _ in 0..20 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let c = &world.creatures()[0];
        assert!(!c.walking && c.panic == 0.0, "le lézard réagit au joueur");
    }

    #[test]
    fn animals_sleep_outside_their_hours_and_wake_up_when_their_time_comes() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        // Le joueur assez loin pour qu'elles ne le remarquent pas, et ne le bousculent pas
        let far = feet + tangent_frame(feet.normalize()).0 * 20.0 * BlockGeometry::new(generator.base_radius()).block_height as f32;
        let up = feet.normalize();
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        let cow = world.registry().find("vache").unwrap();
        let wolf = world.registry().find("loup").unwrap();
        let cow_id = world.spawn(cow, feet);
        let wolf_id = world.spawn(wolf, feet);
        let get = |world: &CreatureWorld, id| world.creatures().iter().find(|c| c.id == id).map(|c| (c.is_sleeping(), c.position)).unwrap();

        // Minuit : le soleil sous les pieds. La vache s'endort et ne bouge plus, le loup veille.
        world.set_sun_dir(-up);
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[far]);
        }
        let (cow_sleeping, at) = get(&world, cow_id);
        assert!(cow_sleeping, "la vache ne dort pas la nuit");
        assert!(!get(&world, wolf_id).0, "le loup dort la nuit");
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[far]);
        }
        assert!(get(&world, cow_id).1.distance(at) < 1e-3, "la vache bouge en dormant");

        // Un coup la réveille
        world.hit(cow_id, feet + up * 10.0);
        assert!(!get(&world, cow_id).0, "frappée, la vache dort encore");

        // Midi : la vache est réveillée, le loup dort
        world.set_sun_dir(up);
        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[far]);
        }
        assert!(!get(&world, cow_id).0, "la vache dort en plein jour");
        assert!(get(&world, wolf_id).0, "le loup ne dort pas en plein jour");
    }

    #[test]
    fn animals_shelter_from_storms_and_bears_hibernate() {
        use crate::weather::{Season, WeatherKind, WeatherModel};
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        // Le joueur assez loin pour qu'elles ne le remarquent pas, et ne le bousculent pas
        let far = feet + tangent_frame(feet.normalize()).0 * 20.0 * BlockGeometry::new(generator.base_radius()).block_height as f32;
        let up = feet.normalize();
        let mut world = CreatureWorld::new(generator.base_radius(), 3, registry());
        let cow = world.spawn(world.registry().find("vache").unwrap(), feet);
        let bear = world.spawn(world.registry().find("ours").unwrap(), feet + up * 0.01);
        let sleeping = |world: &CreatureWorld, id| world.creatures().iter().find(|c| c.id == id).unwrap().is_sleeping();
        let run = |world: &mut CreatureWorld| {
            for _ in 0..600 {
                world.tick(0.05, &generator, &edits, &[far]);
            }
        };
        // Midi, en plein orage : la vache se couche
        world.set_sun_dir(up);
        let mut model = WeatherModel::new(3);
        model.forced = Some(WeatherKind::Storm);
        let lat = (up.y as f64).clamp(-1.0, 1.0).asin();
        // Saison chaude à cette latitude : l'ours ne dort que pour l'orage
        let summer = crate::weather::time_for_season(0.0, if lat >= 0.0 { Season::Summer } else { Season::Winter });
        world.set_climate(&model, summer);
        run(&mut world);
        assert!(sleeping(&world, cow), "la vache reste debout sous l'orage");
        // Beau temps : elle se relève
        model.forced = Some(WeatherKind::Clear);
        world.set_climate(&model, summer);
        run(&mut world);
        assert!(!sleeping(&world, cow), "la vache dort par beau temps");
        assert!(!sleeping(&world, bear), "l'ours dort en été");
        // L'hiver, l'ours hiberne là où les saisons sont marquées
        let winter = crate::weather::time_for_season(0.0, if lat >= 0.0 { Season::Winter } else { Season::Summer });
        world.set_climate(&model, winter);
        run(&mut world);
        if crate::weather::season_strength(lat) > 0.3 {
            assert!(sleeping(&world, bear), "l'ours ne dort pas l'hiver");
        }
        assert!(!sleeping(&world, cow), "la vache hiberne");
    }

    #[test]
    fn guards_go_after_the_killer_of_a_villager_and_no_one_else() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let block = geometry.block_height as f32;
        let (_, feet) = grass_column(&generator, &edits);
        let up = feet.normalize();
        let (east, north) = tangent_frame(up);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let (villager, guard) = (world.registry().find("villageois").unwrap(), world.registry().find("garde").unwrap());
        let herd = 77;
        let victim = world.spawn_managed(villager, feet + east * 2.0 * block, 0.0, herd);
        let guard_id = world.spawn_managed(guard, feet, 0.0, herd);
        // Le meurtrier (clé 1) est plus loin du garde que le passant (clé 2)
        let killer = feet + east * 3.0 * block;
        let bystander = feet - north * 2.0 * block;
        let presences = [PlayerPresence::standing(killer), PlayerPresence::standing(bystander)];
        let mut hits = Vec::new();
        for _ in 0..40 {
            hits.extend(world.tick_players(0.05, &generator, &edits, &presences, &[1, 2]));
        }
        assert!(hits.is_empty(), "le garde frappe sans raison");
        while world.hit_by(victim, killer, 1) {}
        assert!(world.is_wanted(1, herd) && !world.is_wanted(2, herd));
        // Hors des villages, la victime n'a pas de fiche : rien à annoncer
        assert!(world.take_murders().is_empty());
        for _ in 0..400 {
            hits.extend(world.tick_players(0.05, &generator, &edits, &presences, &[1, 2]));
        }
        assert!(hits.iter().any(|h| h.creature_id == guard_id && h.player == 0), "le garde laisse filer le meurtrier");
        assert!(hits.iter().all(|h| h.player == 0), "le garde frappe le passant");
    }

    #[test]
    fn a_hit_guard_strikes_back_at_the_striker_only() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let block = geometry.block_height as f32;
        let (_, feet) = grass_column(&generator, &edits);
        let (east, north) = tangent_frame(feet.normalize());
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let guard = world.registry().find("garde").unwrap();
        let herd = 79;
        let guard_id = world.spawn_managed(guard, feet, 0.0, herd);
        // Le passant (clé 2) est plus près du garde que celui qui le frappe (clé 1)
        let striker = feet + east * 3.0 * block;
        let bystander = feet - north * 1.5 * block;
        assert!(world.hit_by(guard_id, striker, 1));
        assert!(world.is_wanted(1, herd) && !world.is_wanted(2, herd));
        let presences = [PlayerPresence::standing(striker), PlayerPresence::standing(bystander)];
        let mut hits = Vec::new();
        for _ in 0..200 {
            hits.extend(world.tick_players(0.05, &generator, &edits, &presences, &[1, 2]));
        }
        assert!(hits.iter().any(|h| h.player == 0), "le garde ne riposte pas");
        assert!(hits.iter().all(|h| h.player == 0), "le garde frappe le passant");
    }

    #[test]
    fn guards_only_engage_close_and_give_up_after_thirty_blocks() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let block = geometry.block_height as f32;
        let (_, feet) = grass_column(&generator, &edits);
        let up = feet.normalize();
        let (east, _) = tangent_frame(up);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let (villager, guard) = (world.registry().find("villageois").unwrap(), world.registry().find("garde").unwrap());
        let herd = 78;
        let victim = world.spawn_managed(villager, feet, 0.0, herd);
        let guard_id = world.spawn_managed(guard, feet - east * 15.0 * block, 0.0, herd);
        while world.hit_by(victim, feet, 1) {}
        let hunting = |world: &CreatureWorld| world.creatures().iter().find(|c| c.id == guard_id).unwrap().is_hunting();
        // À quinze blocs, il ne bouge pas
        let far = [PlayerPresence::standing(feet)];
        for _ in 0..40 {
            world.tick_players(0.05, &generator, &edits, &far, &[1]);
        }
        assert!(!hunting(&world), "le garde attaque de trop loin");
        let start = world.creatures().iter().find(|c| c.id == guard_id).unwrap().position;
        // Le meurtrier passe à huit blocs puis s'enfuit bien plus vite que lui
        let mut player = start + east * 8.0 * block;
        let mut gave_up = None;
        for frame in 0..1200 {
            player += east * 2.6 * block * 0.05;
            world.tick_players(0.05, &generator, &edits, &[PlayerPresence::standing(player)], &[1]);
            let c = world.creatures().iter().find(|c| c.id == guard_id).unwrap();
            let gone = c.position.distance(start) / block;
            assert!(gone < 34.0, "le garde poursuit à {gone} blocs");
            if frame > 20 && !c.is_hunting() && gave_up.is_none() {
                gave_up = Some(gone);
            }
        }
        // Il lâche au bout de sa laisse (ou quand le meurtrier sort de sa vue)
        let gone = gave_up.expect("le garde n'a jamais lâché");
        assert!(gone > 5.0, "il a lâché à {gone} blocs");
    }

    /// Plus grand écart au centre d'un troupeau, en blocs
    fn herd_spread(world: &CreatureWorld, geometry: &BlockGeometry, herd: u64) -> f32 {
        let members: Vec<Vec3> = world.creatures().iter().filter(|c| c.herd == herd).map(|c| c.position).collect();
        let center = members.iter().copied().sum::<Vec3>() / members.len() as f32;
        members.iter().map(|p| p.distance(center) / geometry.block_height as f32).fold(0.0, f32::max)
    }

    #[test]
    fn a_herd_stays_together() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        let sheep = world.registry().find("mouflon").unwrap();
        let herd = world.spawn_in_herd(sheep, feet, None);
        for _ in 0..4 {
            world.spawn_in_herd(sheep, feet, Some(herd));
        }
        // Trois minutes : sans cohésion, leurs errances au hasard les auraient dispersées
        let mut worst: f32 = 0.0;
        for frame in 0..3600 {
            world.tick(0.05, &generator, &edits, &[feet]);
            if frame >= 600 {
                worst = worst.max(herd_spread(&world, &geometry, herd));
            }
        }
        assert_eq!(world.creatures().iter().filter(|c| c.herd == herd).count(), 5);
        assert!(worst < 16.0, "troupeau dispersé : une bête à {worst} blocs du centre");
    }

    #[test]
    fn a_hit_alarms_the_herd_less_than_the_victim() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        let sheep = world.registry().find("mouflon").unwrap();
        let victim = world.spawn_in_herd(sheep, feet, None);
        let mates = [world.spawn_in_herd(sheep, feet, Some(victim)), world.spawn_in_herd(sheep, feet, Some(victim))];
        let stranger = world.spawn(sheep, feet);
        let (east, _) = tangent_frame(feet.normalize());
        assert!(world.hit(victim, feet - east * 2.0 * geometry.block_height as f32));

        let get = |world: &CreatureWorld, id: u64| world.creatures().iter().find(|c| c.id == id).map(|c| (c.panic, c.panic_speed)).unwrap();
        assert_eq!(get(&world, victim), (PANIC_TIME, PANIC_SPEED));
        for &mate in &mates {
            let (panic, speed) = get(&world, mate);
            assert!(panic > 0.0 && panic < PANIC_TIME && speed < PANIC_SPEED, "voisine : fuite {panic} s à {speed}");
        }
        assert_eq!(get(&world, stranger).0, 0.0, "une bête d'un autre troupeau ne s'affole pas");

        // Les voisines se calment avant la victime
        for _ in 0..((ALARM_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        assert!(mates.iter().all(|&m| get(&world, m).0 <= 0.0), "les voisines fuient encore");
        assert!(get(&world, victim).0 > 0.0, "la victime s'est calmée trop tôt");
    }

    /// Plateau de pierre en plein ciel au-dessus d'une colonne d'herbe, de `dx` blocs de part et
    /// d'autre en x et `dz` en z : ses bords sont des falaises. Renvoie le bloc central du plancher.
    fn sky_platform(generator: &PlanetWorldGenerator, edits: &TerrainEdits, dx: i32, dz: i32) -> BlockPos {
        let (ground, _) = grass_column(generator, edits);
        let floor = BlockPos { y: ground.y + 24, ..ground };
        for x in -dx..=dx {
            for z in -dz..=dz {
                assert!(edits.set_block(generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::STONE));
            }
        }
        floor
    }

    /// Pieds posés sur le plancher `floor`, décalés de (x, z) colonnes
    fn feet_on(geometry: &BlockGeometry, floor: BlockPos, x: i32, z: i32) -> Vec3 {
        geometry.corner(floor.face, (floor.x + x) as f64 + 0.5, (floor.y + 1) as f64, (floor.z + z) as f64 + 0.5).as_vec3()
    }

    /// Fait marcher la créature `0` droit vers la colonne `to`, sans changer d'humeur
    fn walk_towards(world: &mut CreatureWorld, geometry: &BlockGeometry, to: BlockPos) {
        let c = &mut world.creatures[0];
        c.yaw = yaw_towards(geometry, c.position, to);
        (c.target_yaw, c.walking, c.timer) = (c.yaw, true, 1000.0);
    }

    #[test]
    fn a_creature_walks_around_a_wall() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 6, 4);
        // Mur de cinq blocs de large et deux de haut en travers, avec un passage de chaque côté
        for z in -2..=2 {
            for y in 1..=2 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + 2, y: floor.y + y, z: floor.z + z, ..floor }, BlockId::STONE));
            }
        }
        let feet = feet_on(&geometry, floor, -3, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 13, registry());
        let lizard = world.registry().find("lezard").unwrap();
        world.spawn(lizard, feet);
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x + 6, ..floor });

        let mut passed = false;
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let position = world.creatures()[0].position;
            let alt = altitude(&geometry, position);
            assert!((alt - (floor.y + 1) as f32).abs() < 0.6, "tombée du plateau ({alt})");
            let column = geometry.block_at_point(position.as_dvec3()).unwrap();
            passed |= column.x >= floor.x + 3 && (alt - (floor.y + 1) as f32).abs() < 1e-2;
        }
        assert!(passed, "le lézard n'a pas contourné le mur");
    }

    #[test]
    fn creatures_never_walk_off_a_cliff() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 4, 4);
        let feet = feet_on(&geometry, floor, 0, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 17, registry());
        let mut ids = Vec::new();
        for (name, x, z) in [("vache", -2, 0), ("lapin", 2, 2), ("sanglier", 0, -2)] {
            let species = world.registry().find(name).unwrap();
            ids.push(world.spawn(species, feet_on(&geometry, floor, x, z)));
        }
        // Deux minutes de promenade, toujours droit vers le bord quand elles repartent
        for i in 0..2400 {
            if i % 100 == 0 {
                for c in world.creatures.iter_mut().filter(|c| ids.contains(&c.id)) {
                    (c.walking, c.timer) = (true, 4.0);
                }
            }
            world.tick(0.05, &generator, &edits, &[feet]);
            // Celles qui apparaissent d'elles-mêmes sur le sol, plus bas, ne comptent pas
            for c in world.creatures().iter().filter(|c| ids.contains(&c.id)) {
                let alt = altitude(&geometry, c.position);
                assert!((alt - (floor.y + 1) as f32).abs() < 0.6, "{} tombée du plateau ({alt})", c.species);
            }
        }
    }

    #[test]
    fn a_fleeing_creature_swims_across_water() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 7, 3);
        // Un ruisseau de trois blocs en travers du plateau
        for x in 0..=2 {
            for z in -3..=3 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::WATER));
            }
        }
        let feet = feet_on(&geometry, floor, -4, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 19, registry());
        let cow = world.registry().find("vache").unwrap();
        world.spawn(cow, feet);
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x + 7, ..floor });
        world.creatures[0].panic = 100.0;

        let (mut swam, mut across) = (false, false);
        for _ in 0..300 {
            world.tick(0.05, &generator, &edits, &[feet]);
            let c = &world.creatures()[0];
            let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
            if c.swimming {
                swam = true;
                // Le corps est à moitié dans l'eau, les pieds restent à la surface
                assert!(c.state().position.length() < c.position.length());
            }
            across |= column.x >= floor.x + 3 && !c.swimming;
        }
        assert!(swam, "la vache n'est pas entrée dans l'eau");
        assert!(across, "la vache n'a pas traversé le ruisseau");
    }

    #[test]
    fn a_creature_in_water_swims_to_the_shore() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let floor = sky_platform(&generator, &edits, 7, 4);
        // Un bassin sur tout le plateau sauf sa bordure est
        for x in -7..=4 {
            for z in -4..=4 {
                assert!(edits.set_block(&generator, BlockPos { x: floor.x + x, z: floor.z + z, ..floor }, BlockId::WATER));
            }
        }
        let feet = feet_on(&geometry, floor, -5, 0);
        let mut world = CreatureWorld::new(generator.base_radius(), 23, registry());
        let sheep = world.registry().find("vache").unwrap();
        world.spawn(sheep, feet);
        // Tournée vers l'ouest, loin de la rive
        walk_towards(&mut world, &geometry, BlockPos { x: floor.x - 7, ..floor });
        world.creatures[0].timer = 3.0;

        for _ in 0..600 {
            world.tick(0.05, &generator, &edits, &[feet]);
        }
        let c = &world.creatures()[0];
        let column = geometry.block_at_point(c.position.as_dvec3()).unwrap();
        assert!(column.x >= floor.x + 5 && !c.swimming, "le mouton est resté à l'eau (colonne {})", column.x - floor.x);
        assert!((altitude(&geometry, c.position) - (floor.y + 1) as f32).abs() < 1e-2);
    }

    /// `count` bêtes de l'espèce `id` sur une colonne d'herbe plate, et un joueur à `blocks` blocs à
    /// l'est d'elles
    fn beasts_and_player(id: &str, count: usize, blocks: f32, seed: u64) -> (PlanetWorldGenerator, TerrainEdits, CreatureWorld, Vec3, Vec3) {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), seed, registry());
        let species = world.registry().find(id).unwrap();
        for _ in 0..count {
            world.spawn(species, feet);
        }
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet + east * blocks * geometry.block_height as f32;
        (generator, edits, world, feet, player)
    }

    /// Coups portés au joueur en `seconds` secondes de jeu
    fn hits_over(world: &mut CreatureWorld, generator: &PlanetWorldGenerator, edits: &TerrainEdits, player: Vec3, seconds: f32) -> Vec<PlayerHit> {
        (0..(seconds / 0.05) as usize).flat_map(|_| world.tick(0.05, generator, edits, &[player])).collect()
    }

    #[test]
    fn a_summoned_turret_appears_then_shoots_the_monster_in_range() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 1, 0.0, 31);
        let block = world.block_size();
        let (east, north) = tangent_frame(feet.normalize());
        // Le monstre est à huit blocs de la tourelle, le joueur loin des deux
        let turret_feet = feet + north * 8.0 * block;
        let player = feet - east * 40.0 * block;
        let monster = world.creatures()[0].id;
        let turret = world.summon(world.registry().find("ballisticTurret").unwrap(), turret_feet, 0.0, 9);
        let action = |w: &CreatureWorld, id| w.creatures().iter().find(|c| c.id == id).map(|c| c.action());
        assert!(matches!(action(&world, turret), Some(CreatureAction::Apparition(0))), "elle apparaît d'abord");
        let health = world.creatures().iter().find(|c| c.id == monster).unwrap().health;
        let mut shots = Vec::new();
        for _ in 0..80 {
            world.tick(0.05, &generator, &edits, &[player]);
            shots.extend(world.take_shots());
        }
        assert!(!matches!(action(&world, turret), Some(CreatureAction::Apparition(_))), "en place");
        assert!(shots.iter().any(|s| s.creature_id == turret), "la tourelle n'a pas tiré");
        let hurt = world.creatures().iter().find(|c| c.id == monster).is_none_or(|c| c.health < health);
        assert!(hurt, "le monstre n'a pas été touché");
    }

    #[test]
    fn a_jaw_trap_snaps_on_the_monster_that_steps_on_it_then_vanishes() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 1, 0.0, 33);
        let block = world.block_size();
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        let monster = world.creatures()[0].id;
        // Assez solide pour survivre aux mâchoires
        world.creature_mut(monster).unwrap().health = 100.0;
        let health = 100.0;
        let trap = world.summon(world.registry().find("jawTrap").unwrap(), feet, 0.0, 9);
        let mut shots = Vec::new();
        let mut stunned = false;
        for _ in 0..30 {
            world.tick(0.05, &generator, &edits, &[player]);
            shots.extend(world.take_shots());
            stunned |= world.creatures().iter().any(|c| c.id == monster && c.stun > 0.0);
        }
        assert!(shots.iter().any(|s| s.creature_id == trap), "le piège ne s'est pas déclenché");
        assert!(world.creatures().iter().all(|c| c.id != trap), "le piège déclenché est resté");
        assert!(world.creatures().iter().find(|c| c.id == monster).is_none_or(|c| c.health < health), "le monstre n'a rien pris");
        assert!(stunned, "le monstre n'a pas été immobilisé");
        // Puis il saigne, sans reculer
        let after = world.creatures().iter().find(|c| c.id == monster).unwrap().health;
        for _ in 0..60 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let bled = world.creatures().iter().find(|c| c.id == monster).unwrap().health;
        assert!(bled <= after - 10.0, "le monstre n'a pas saigné ({after} -> {bled})");
    }

    #[test]
    fn a_volcanic_rune_bursts_into_fire_when_a_monster_walks_on_it() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 1, 0.0, 41);
        let block = world.block_size();
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        let monster = world.creatures()[0].id;
        world.creature_mut(monster).unwrap().health = 100.0;
        let rune = world.summon(world.registry().find("volcanicRune").unwrap(), feet, 0.0, 9);
        let mut shots = Vec::new();
        for _ in 0..30 {
            world.tick(0.05, &generator, &edits, &[player]);
            shots.extend(world.take_shots());
        }
        assert!(shots.iter().any(|s| s.creature_id == rune), "la rune n'a pas éclaté");
        assert!(world.creatures().iter().all(|c| c.id != rune), "la rune éclatée est restée");
        assert!(world.creatures().iter().find(|c| c.id == monster).unwrap().health <= 100.0 - 12.0);
    }

    #[test]
    fn an_eradication_trap_drops_three_mines_around_its_prey() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 1, 0.0, 43);
        let block = world.block_size();
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        let monster = world.creatures()[0].id;
        world.creature_mut(monster).unwrap().health = 500.0;
        let trap = world.summon(world.registry().find("eradicationTrap").unwrap(), feet, 0.0, 9);
        let mine = world.registry().find("shrapnelMine").unwrap();
        let mut spawned = Vec::new();
        let mut shots = Vec::new();
        for _ in 0..30 {
            world.tick(0.05, &generator, &edits, &[player]);
            spawned.extend(world.take_spawned());
            shots.extend(world.take_shots());
        }
        assert!(shots.iter().any(|s| s.creature_id == trap), "le piège ne s'est pas refermé");
        assert_eq!(spawned.len(), 3, "trois mines larguées");
        assert!(spawned.iter().all(|&(summoner, _, seconds)| summoner == 9 && seconds > 0.0));
        let mines: Vec<u64> = spawned.iter().map(|s| s.1).collect();
        assert!(mines.iter().all(|id| world.creatures().iter().find(|c| c.id == *id).is_none_or(|c| c.species == mine)));
        // Le monstre immobilisé les déclenche
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
            shots.extend(world.take_shots());
        }
        assert!(mines.iter().any(|id| shots.iter().any(|s| s.creature_id == *id)), "aucune mine n'a explosé");
        assert!(world.creatures().iter().find(|c| c.id == monster).is_none_or(|c| c.health <= 500.0 - 12.0 - 25.0));
    }

    #[test]
    fn a_piercing_shot_hits_every_monster_along_its_line() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 0, 0.0, 47);
        let block = world.block_size();
        let (east, north) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        let rampant = world.registry().find("rampant").unwrap();
        let near = world.spawn(rampant, feet + north * 6.0 * block);
        let far = world.spawn(rampant, feet + north * 8.0 * block);
        for id in [near, far] {
            let c = world.creature_mut(id).unwrap();
            c.health = 1000.0;
            c.stun = 100.0;
        }
        world.summon(world.registry().find("executionTurret").unwrap(), feet, 0.0, 9);
        for _ in 0..60 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let hp = |w: &CreatureWorld, id| w.creatures().iter().find(|c| c.id == id).unwrap().health;
        assert!(hp(&world, near) < 1000.0 && hp(&world, far) < 1000.0, "les deux monstres alignés sont touchés");
    }

    #[test]
    fn a_hasted_turret_shoots_faster_and_a_shield_absorbs_its_wounds() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("rampant", 0, 0.0, 53);
        let block = world.block_size();
        let (east, north) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        // Pressée de 100 %, elle recharge deux fois plus vite
        let hasted = world.summon(world.registry().find("ballisticTurret").unwrap(), player + north * 3.0 * block, 0.0, 9);
        let plain = world.summon(world.registry().find("ballisticTurret").unwrap(), player - north * 3.0 * block, 0.0, 9);
        world.haste(hasted, 1.0, 100.0);
        for id in [hasted, plain] {
            let c = world.creature_mut(id).unwrap();
            c.appearing = 0.0;
            c.attack_cooldown = 1.0;
        }
        for _ in 0..5 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let cooldown = |w: &CreatureWorld, id| w.creatures().iter().find(|c| c.id == id).unwrap().attack_cooldown;
        assert!((cooldown(&world, plain) - 0.75).abs() < 1e-4, "{}", cooldown(&world, plain));
        assert!((cooldown(&world, hasted) - 0.5).abs() < 1e-4);
        // Le bouclier encaisse d'abord
        let turret = world.summon(world.registry().find("ballisticTurret").unwrap(), feet, 0.0, 9);
        world.shield(turret, 30.0, 10.0);
        let before = world.creatures().iter().find(|c| c.id == turret).unwrap().health;
        // (des coups qui percent toute son armure, pour compter juste)
        world.wound_with(turret, feet, 1, 20.0, DamageKind::Physical, 1.0);
        assert_eq!(world.creatures().iter().find(|c| c.id == turret).unwrap().health, before);
        world.wound_with(turret, feet, 1, 20.0, DamageKind::Physical, 1.0);
        assert_eq!(world.creatures().iter().find(|c| c.id == turret).unwrap().health, before - 10.0);
    }

    #[test]
    fn the_magnetic_trap_drags_beasts_to_its_center() {
        // Une vache, qui ne s'approcherait pas d'elle-même (un monstre, lui, irait mordre le piège)
        let (generator, edits, mut world, feet, _) = beasts_and_player("vache", 1, 0.0, 36);
        let block = world.block_size();
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * block;
        let monster = world.creatures()[0].id;
        let trap_feet = feet + east * 3.0 * block;
        world.summon(world.registry().find("magneticTrap").unwrap(), trap_feet, 0.0, 9);
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let distance = world.creatures().iter().find(|c| c.id == monster).unwrap().position.distance(trap_feet) / block;
        assert!(distance < 1.5, "la vache est restée à {distance} blocs du piège");
    }

    #[test]
    fn a_stone_wall_stops_the_monster_but_creatures_walk_over_a_trap() {
        // Le joueur à sept blocs à l'est, un mur de pierre en travers à mi-chemin
        let (generator, edits, mut world, feet, player) = beasts_and_player("rampant", 1, 7.0, 34);
        let block = world.block_size();
        let (east, north) = tangent_frame(feet.normalize());
        let monster = world.creatures()[0].id;
        world.summon(world.registry().find("stoneWall").unwrap(), feet + east * 3.5 * block, std::f32::consts::FRAC_PI_2, 9);
        let mut closest = f32::MIN;
        for _ in 0..80 {
            world.tick(0.05, &generator, &edits, &[player]);
            let Some(c) = world.creatures().iter().find(|c| c.id == monster) else { break };
            let d = (c.position - feet) / block;
            if d.dot(north).abs() < 2.4 {
                closest = closest.max(d.dot(east));
            }
        }
        assert!(closest > 1.5, "le monstre ne s'est pas approché du mur ({closest})");
        assert!(closest < 3.5 - 0.62 - 0.4, "le monstre a traversé le mur ({closest})");

        // Un piège (résistance 0) ne gêne personne
        let trap = world.registry().get(world.registry().find("jawTrap").unwrap()).unwrap().collider();
        assert_eq!(trap.resistance, 0.0);
    }

    #[test]
    fn two_beasts_on_the_same_spot_are_pushed_apart() {
        let (generator, edits, mut world, feet, _) = beasts_and_player("vache", 2, 0.0, 35);
        let block = world.block_size();
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 60.0 * block;
        for _ in 0..40 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let [a, b] = [0, 1].map(|i| world.creatures()[i].position);
        assert!(a.distance(b) / block > 0.5, "les deux vaches restent l'une dans l'autre ({})", a.distance(b) / block);
    }

    #[test]
    fn the_death_knight_draws_the_monster_away_from_the_player() {
        let (generator, edits, mut world, feet, player) = beasts_and_player("rampant", 1, 2.0, 32);
        let block = world.block_size();
        let (_, north) = tangent_frame(feet.normalize());
        let knight = world.summon(world.registry().find("deathKnight").unwrap(), feet + north * 4.5 * block, 0.0, 9);
        let health = world.creatures().iter().find(|c| c.id == knight).unwrap().health;
        let hits = hits_over(&mut world, &generator, &edits, player, 4.0);
        assert!(hits.is_empty(), "le monstre s'en est pris au joueur malgré la provocation");
        assert!(world.creatures().iter().find(|c| c.id == knight).is_none_or(|c| c.health < health), "le chevalier n'a pas été attaqué");
    }

    #[test]
    fn a_wolf_pack_attacks_the_player_but_a_lone_wolf_flees() {
        let (generator, edits, mut world, _, player) = beasts_and_player("loup", 3, 2.0, 21);
        let hits = hits_over(&mut world, &generator, &edits, player, 5.0);
        let wolf = world.registry().find("loup").unwrap();
        let damage = world.registry().get(wolf).unwrap().attack.as_ref().unwrap().damage;
        assert!(!hits.is_empty(), "la meute n'a pas attaqué");
        assert!(hits.iter().all(|h| h.player == 0 && h.species == wolf && h.damage == damage));
        // Les trois loups posés (d'autres bêtes ont pu apparaître plus loin entre-temps)
        assert!(world.creatures().iter().filter(|c| c.id <= 3).all(|c| c.is_hunting()));

        // Seul, il n'ose pas, et s'enfuit quand on le frappe
        let (generator, edits, mut world, feet, player) = beasts_and_player("loup", 1, 2.0, 22);
        assert!(hits_over(&mut world, &generator, &edits, player, 5.0).is_empty(), "un loup seul a attaqué");
        let id = world.creatures()[0].id;
        assert!(world.hit(id, feet));
        assert!(world.creatures()[0].panic > 0.0, "le loup seul ne fuit pas");
    }

    #[test]
    fn a_hit_wolf_rallies_its_pack() {
        let (generator, edits, mut world, feet, player) = beasts_and_player("loup", 3, 2.0, 23);
        let id = world.creatures()[0].id;
        assert!(world.hit(id, feet));
        assert!(world.creatures().iter().all(|c| c.anger > 0.0 && c.panic <= 0.0), "la meute ne riposte pas");
        assert!(!hits_over(&mut world, &generator, &edits, player, 5.0).is_empty());
    }

    #[test]
    fn a_bear_attacks_only_once_provoked() {
        let (generator, edits, mut world, feet, player) = beasts_and_player("ours", 1, 2.0, 24);
        assert!(hits_over(&mut world, &generator, &edits, player, 5.0).is_empty(), "l'ours a attaqué sans raison");
        let id = world.creatures()[0].id;
        assert!(world.hit(id, feet));
        assert_eq!(world.creatures()[0].panic, 0.0, "l'ours frappé fuit au lieu de riposter");
        assert!(!hits_over(&mut world, &generator, &edits, player, 5.0).is_empty(), "l'ours frappé ne riposte pas");
    }

    #[test]
    fn a_peaceful_creature_never_attacks() {
        let (generator, edits, mut world, feet, player) = beasts_and_player("vache", 3, 1.0, 25);
        let id = world.creatures()[0].id;
        world.hit(id, feet);
        assert!(hits_over(&mut world, &generator, &edits, player, 5.0).is_empty());
    }

    #[test]
    fn a_boar_aims_then_charges() {
        let (generator, edits, mut world, _, player) = beasts_and_player("sanglier", 1, 5.0, 26);
        world.tick(0.05, &generator, &edits, &[player]);
        assert!(matches!(world.creatures()[0].charge, Charge::Aiming(_)), "{:?}", world.creatures()[0].charge);
        assert!(!world.creatures()[0].walking, "il vise sans bouger");
        for _ in 0..((CHARGE_AIM / 0.05) as usize + 1) {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let charge = world.creatures()[0].charge;
        assert!(matches!(charge, Charge::Running(_) | Charge::Resting(_)), "pas de charge : {charge:?}");
    }

    #[test]
    fn wolves_hunt_their_prey() {
        // Joueur hors de vue : les loups s'occupent du lapin posé à côté d'eux
        let (generator, edits, mut world, feet, _) = beasts_and_player("loup", 3, 0.0, 27);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet - east * 40.0 * geometry.block_height as f32;
        let rabbit = world.registry().find("lapin").unwrap();
        let id = world.spawn(rabbit, feet);
        let health = world.registry().get(rabbit).unwrap().health;
        let mut bitten = false;
        for _ in 0..600 {
            assert!(world.tick(0.05, &generator, &edits, &[player]).is_empty(), "le joueur est trop loin pour être mordu");
            bitten |= world.creatures().iter().find(|c| c.id == id).is_none_or(|c| c.health < health);
        }
        assert!(bitten, "le lapin n'a jamais été mordu");
    }

    #[test]
    fn a_striking_creature_plays_its_attack_animation() {
        let (generator, edits, mut world, _, player) = beasts_and_player("loup", 3, 2.0, 21);
        let mut struck = false;
        for _ in 0..100 {
            let hits = world.tick(0.05, &generator, &edits, &[player]);
            for hit in hits {
                let c = world.creatures().iter().find(|c| c.id == hit.creature_id).unwrap();
                assert_eq!(c.state().action, CreatureAction::Attaque, "le loup mord sans animation d'attaque");
                struck = true;
            }
        }
        assert!(struck, "la meute n'a pas attaqué");
        // Les loups qui poursuivent courent
        let actions: Vec<_> = world.creatures().iter().filter(|c| c.is_hunting() && c.walking).map(|c| c.state().action).collect();
        assert!(actions.iter().all(|a| *a == CreatureAction::Course), "{actions:?}");
    }

    /// Identifiant, espèce et position de chaque bête éveillée, triés
    fn snapshot(world: &CreatureWorld) -> Vec<(u64, usize, Vec3)> {
        let mut v: Vec<_> = world.creatures().iter().map(|c| (c.id, c.species, c.position)).collect();
        v.sort_by_key(|c| c.0);
        v
    }

    /// Un point de la face +Z, à `blocks` blocs du centre le long de x, sur le relief
    fn feet_at(generator: &PlanetWorldGenerator, edits: &TerrainEdits, x: i32, z: i32) -> Vec3 {
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(generator, edits);
        let top = query.support_top(PlanetFace::PosZ, x, z, WORLD_HEIGHT - 1);
        geometry.corner(PlanetFace::PosZ, x as f64 + 0.5, top as f64, z as f64 + 0.5).as_vec3()
    }

    #[test]
    fn the_population_depends_only_on_the_world_seed() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut a = CreatureWorld::new(generator.base_radius(), 77, registry());
        let mut b = CreatureWorld::new(generator.base_radius(), 77, registry());
        a.tick(0.0, &generator, &edits, &[player]);
        b.tick(0.0, &generator, &edits, &[player]);
        assert!(!a.creatures().is_empty());
        assert_eq!(snapshot(&a), snapshot(&b));
        let mut c = CreatureWorld::new(generator.base_radius(), 78, registry());
        c.tick(0.0, &generator, &edits, &[player]);
        assert_ne!(snapshot(&a), snapshot(&c), "une autre graine, une autre population");
    }

    #[test]
    fn nothing_appears_after_the_first_population() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        world.tick(0.05, &generator, &edits, &[player]);
        let first: HashSet<u64> = world.creatures().iter().map(|c| c.id).collect();
        assert!(!first.is_empty());
        // Une minute plus tard, aucune nouvelle bête
        for _ in 0..1200 {
            world.tick(0.05, &generator, &edits, &[player]);
            assert!(world.creatures().iter().all(|c| first.contains(&c.id)), "une bête est apparue");
        }
        // Toutes tuées : le voisinage reste vide, même après un aller-retour au loin
        let ids: Vec<u64> = world.creatures().iter().map(|c| c.id).collect();
        for id in ids {
            while world.hit(id, player) {}
            assert!(world.skin(id).is_some());
        }
        assert!(world.creatures().is_empty());
        world.tick(0.05, &generator, &edits, &[-player]);
        world.tick(0.05, &generator, &edits, &[player]);
        assert!(world.creatures().is_empty(), "une bête est revenue");
        for _ in 0..200 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        assert!(world.creatures().is_empty());
    }

    #[test]
    fn sleeping_creatures_wake_up_where_they_were() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 77, registry());
        for _ in 0..200 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        let before = snapshot(&world);
        assert!(!before.is_empty());
        // Le joueur part à l'autre bout de la planète : tout s'endort ; il revient : tout se réveille
        world.tick(0.0, &generator, &edits, &[-player]);
        assert!(world.creatures().iter().all(|c| c.position.distance(-player) < player.length()));
        world.tick(0.0, &generator, &edits, &[player]);
        let after = snapshot(&world);
        for bête in &before {
            assert!(after.iter().any(|a| a.0 == bête.0 && a.2.distance(bête.2) < 1e-3), "bête {} perdue ou déplacée", bête.0);
        }
    }

    #[test]
    fn a_creature_that_wanders_off_sleeps_in_its_new_zone() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (ground, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 5, registry());
        world.populate = false;
        let cow = world.registry().find("vache").unwrap();
        let id = world.spawn(cow, feet);
        world.tick(0.0, &generator, &edits, &[feet]);
        // Elle s'éloigne hors des zones éveillées : elle s'endort dans la sienne, pas ailleurs
        let far = (CREATURE_RANGE_BLOCKS + ZONE_BLOCKS as f32 * 3.0 + SLEEP_MARGIN_BLOCKS) as i32;
        let x = if ground.x + far < crate::edits::FACE_BLOCKS { ground.x + far } else { ground.x - far };
        let away = feet_at(&generator, &edits, x, ground.z);
        world.creatures[0].position = away;
        world.tick(0.0, &generator, &edits, &[feet]);
        assert!(world.creatures().is_empty());
        let zone = ZoneKey::of_column(PlanetFace::PosZ, x, ground.z);
        assert!(world.sleeping[&zone].iter().any(|c| c.id == id));
        // Le joueur la rejoint : elle est là
        world.tick(0.0, &generator, &edits, &[away]);
        let c = world.creatures().iter().find(|c| c.id == id).expect("la vache a disparu");
        assert!(c.position.distance(away) < geometry.block_height as f32 * 1e-3);
    }

    #[test]
    fn the_population_is_saved_and_reloaded() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, player) = grass_column(&generator, &edits);
        let dir = std::env::temp_dir().join(format!("herocraft_creatures_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut world = CreatureWorld::with_save_dir(generator.base_radius(), 77, registry(), &dir);
        for _ in 0..100 {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        // Une bête tuée avant l'enregistrement
        let killed = world.creatures()[0].id;
        while world.hit(killed, player) {}
        for _ in 0..((CREATURE_DEATH_TIME / 0.05) as usize + 2) {
            world.tick(0.05, &generator, &edits, &[player]);
        }
        // Une bête morte encore à terre n'est pas enregistrée : elle ne revient pas
        let before: Vec<_> = snapshot(&world).into_iter().filter(|c| c.0 != killed).collect();
        world.save().unwrap();

        let mut reloaded = CreatureWorld::with_save_dir(generator.base_radius(), 77, registry(), &dir);
        reloaded.tick(0.0, &generator, &edits, &[player]);
        let after = snapshot(&reloaded);
        assert!(after.iter().all(|c| c.0 != killed), "la bête tuée est revenue");
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!((b.0, b.1), (a.0, a.1));
            assert!(b.2.distance(a.2) < 1e-3);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fed_animals_get_tamed_then_breed_a_growing_young() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 9, registry());
        world.populate = false;
        let cow = world.registry().find("vache").unwrap();
        let wheat = items().find("ble").unwrap();
        let (east, _) = tangent_frame(feet.normalize());
        let a = world.spawn(cow, feet);
        let b = world.spawn(cow, feet + east * 4.0 * geometry.block_height as f32);

        // Pas sa nourriture : refusé
        assert_eq!(world.feed(a, items().find("carotte").unwrap()), Feeding::Refused);
        // Quelques repas l'apprivoisent
        for id in [a, b] {
            let mut meals = 0;
            loop {
                meals += 1;
                match world.feed(id, wheat) {
                    Feeding::Taming => assert!(meals < 60, "une chance sur trois à chaque repas"),
                    Feeding::Tamed => break,
                    other => panic!("{other:?}"),
                }
            }
        }
        assert!(world.creatures().iter().all(|c| c.tamed && c.mood() & MOOD_TAMED != 0 && c.mood() & MOOD_HEARTS != 0));
        // Apprivoisées : un repas de plus les rend amoureuses, pas deux
        assert_eq!(world.feed(a, wheat), Feeding::InLove);
        assert_eq!(world.feed(a, wheat), Feeding::Refused);
        assert_eq!(world.feed(b, wheat), Feeding::InLove);

        // Elles se rejoignent et donnent un petit, petit et apprivoisé
        let far = feet - east * 30.0 * geometry.block_height as f32;
        for _ in 0..400 {
            world.tick(0.05, &generator, &edits, &[far]);
            if world.creatures().len() == 3 {
                break;
            }
        }
        assert_eq!(world.creatures().len(), 3, "aucun petit n'est né");
        let young = world.creatures().iter().find(|c| c.is_young()).expect("un petit");
        assert!(young.tamed && (young.scale() - NEWBORN_SCALE).abs() < 0.05);
        assert_eq!(young.state().scale, young.scale());
        let young = young.id;
        // Les parents ne veulent plus manger pour un moment
        assert_eq!(world.feed(a, wheat), Feeding::Refused);
        // Nourri, le petit grandit plus vite, puis devient adulte
        let before = world.creatures().iter().find(|c| c.id == young).unwrap().scale();
        assert_eq!(world.feed(young, wheat), Feeding::Grew);
        assert!(world.creatures().iter().find(|c| c.id == young).unwrap().scale() > before);
        world.creatures.iter_mut().find(|c| c.id == young).unwrap().age = Some(1e9);
        world.tick(0.05, &generator, &edits, &[far]);
        assert!(!world.creatures().iter().find(|c| c.id == young).unwrap().is_young());

        // Apprivoisement et âge sont gardés quand la zone s'endort
        let sleeping = world.sleeping_creature(&world.creatures()[0]).unwrap();
        assert!(sleeping.tamed);
    }

    #[test]
    fn holding_its_food_lures_a_shy_animal() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let geometry = BlockGeometry::new(generator.base_radius());
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 11, registry());
        world.populate = false;
        let rabbit = world.registry().find("lapin").unwrap();
        world.spawn(rabbit, feet);
        let (east, _) = tangent_frame(feet.normalize());
        let player = feet + east * 6.0 * geometry.block_height as f32;
        let distance = |w: &CreatureWorld| w.creatures()[0].position.distance(player) / geometry.block_height as f32;
        // Mains vides : le lapin fuit
        let empty = PlayerPresence::standing(player);
        for _ in 0..40 {
            world.tick_with_presences(0.05, &generator, &edits, &[empty]);
        }
        assert!(distance(&world) > 6.5, "le lapin devait fuir ({} blocs)", distance(&world));
        // Une carotte en main : il approche
        let start = distance(&world);
        let lure = PlayerPresence { holding: items().find("carotte"), ..empty };
        for _ in 0..200 {
            world.tick_with_presences(0.05, &generator, &edits, &[lure]);
        }
        assert!(distance(&world) < start.min(LURE_RADIUS_BLOCKS) - 1.0, "le lapin devait approcher ({start} puis {} blocs)", distance(&world));
    }

    #[test]
    fn a_meal_tames_one_time_in_three() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 13, registry());
        world.populate = false;
        let cow = world.registry().find("vache").unwrap();
        let wheat = items().find("ble").unwrap();
        let tamed = (0..600).filter(|_| {
            let id = world.spawn(cow, feet);
            world.feed(id, wheat) == Feeding::Tamed
        });
        let tamed = tamed.count();
        assert!((150..250).contains(&tamed), "{tamed} vaches apprivoisées sur 600 au premier repas");
    }

    #[test]
    fn a_tame_pheasant_lays_eggs_but_not_a_wild_one() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (_, feet) = grass_column(&generator, &edits);
        let mut world = CreatureWorld::new(generator.base_radius(), 17, registry());
        world.populate = false;
        let pheasant = world.registry().find("faisan").unwrap();
        let egg = items().find("oeufFaisan").expect("l'œuf de faisan");
        let wild = world.spawn(pheasant, feet);
        let tame = world.spawn(pheasant, feet);
        world.creatures.iter_mut().find(|c| c.id == tame).unwrap().tamed = true;
        // Pas d'œuf avant 5 minutes ; avant 10, un seul, de la bête apprivoisée, à ses pieds
        for c in world.creatures.iter_mut() {
            c.lay_in = c.lay_in.min(300.0);
        }
        world.creatures.iter_mut().find(|c| c.id == wild).unwrap().lay_in = 1.0;
        world.rear(299.0);
        assert!(world.take_laid().is_empty(), "rien avant l'heure, et rien de la bête sauvage");
        world.rear(2.0);
        let laid = world.take_laid();
        assert_eq!(laid.len(), 1);
        assert_eq!(laid[0].1, egg);
        let at = world.creatures().iter().find(|c| c.id == tame).unwrap().position;
        assert!(laid[0].0.distance(at) < 1e-3);
        assert!(world.take_laid().is_empty());
        // Elle repond plus tard, entre 5 et 10 minutes après
        let next = world.creatures().iter().find(|c| c.id == tame).unwrap().lay_in;
        assert!((300.0..=600.0).contains(&next), "{next}");
    }
    #[test]
    fn an_invisible_player_is_ignored_and_a_blind_monster_loses_him() {
        let (generator, edits, mut world, _, player) = beasts_and_player("rampant", 1, 3.0, 41);
        let hidden = PlayerPresence { hidden: true, ..PlayerPresence::standing(player) };
        let hits: Vec<PlayerHit> = (0..80).flat_map(|_| world.tick_with_presences(0.05, &generator, &edits, &[hidden])).collect();
        assert!(hits.is_empty(), "le monstre a frappé un joueur invisible");
        assert!(!world.creatures()[0].is_hunting(), "le monstre poursuit un joueur invisible");

        // Aveuglé, il ne voit pas le joueur à trois blocs
        let (generator, edits, mut world, _, player) = beasts_and_player("rampant", 1, 3.0, 42);
        let monster = world.creatures()[0].id;
        world.blind(monster, 10.0);
        assert!(world.is_blind(monster));
        world.tick(0.05, &generator, &edits, &[player]);
        assert!(!world.creatures()[0].is_hunting(), "le monstre aveuglé a vu le joueur");
    }

    #[test]
    fn a_taunted_monster_chases_its_provoker() {
        // Un ours ne s'en prend à personne sans raison ; provoqué par le joueur 7, il le poursuit
        let (generator, edits, mut world, _, player) = beasts_and_player("ours", 1, 2.0, 43);
        let bear = world.creatures()[0].id;
        world.taunt(bear, 7, 5.0);
        let presences = [PlayerPresence::standing(player)];
        let hits: Vec<PlayerHit> = (0..80).flat_map(|_| world.tick_players(0.05, &generator, &edits, &presences, &[7])).collect();
        assert!(!hits.is_empty(), "l'ours provoqué n'a pas attaqué");
    }

    #[test]
    fn some_beasts_slow_or_stun_the_player_they_hit() {
        let (generator, edits, mut world, _, player) = beasts_and_player("gorgone", 1, 1.0, 44);
        let hits = hits_over(&mut world, &generator, &edits, player, 5.0);
        let mut stats = herocraft_common::CharacterStats::new(herocraft_common::PlayerClass::Adventurer);
        world.afflict(hits.first().expect("la gorgone n'a pas frappé"), &mut stats, &[]);
        assert!(stats.slow > 0.0 && stats.slow_time > 0.0, "la gorgone ne ralentit pas");
        assert!(stats.status_speed() < 1.0);

        let brute = world.registry().find("brute").unwrap();
        let hit = PlayerHit { player: 0, creature_id: 0, species: brute, damage: 1.0, kind: DamageKind::Physical };
        world.afflict(&hit, &mut stats, &[]);
        assert!(stats.is_stunned() && stats.status_speed() == 0.0, "la brute n'étourdit pas");
        stats.tick_status(1.0);
        assert!(!stats.is_stunned(), "l'étourdissement ne passe pas");
    }

    #[test]
    fn a_charging_brute_knocks_the_player_back_unless_immune() {
        let (generator, edits, mut world, _, player) = beasts_and_player("brute", 1, 1.0, 44);
        let hits = hits_over(&mut world, &generator, &edits, player, 8.0);
        let hit = hits.first().expect("la brute n'a pas frappé");
        let (direction, speed) = world.knockback(hit, player).expect("la brute repousse");
        let from = world.position(hit.creature_id).unwrap();
        assert!(direction.dot(player - from) > 0.0 && speed > 0.0, "le recul éloigne de la bête");
        let mut stats = herocraft_common::CharacterStats::new(herocraft_common::PlayerClass::Adventurer);
        world.afflict(hit, &mut stats, &[Status::Stun]);
        assert!(!stats.is_stunned(), "immunisé, il n'est pas étourdi");
    }
}


//! Compétences : la roue des quinze classes et ce que fait chaque compétence quand on la lance.
//!
//! Chaque compétence est un fichier `assets/skills/<classe>/<id>.json` ; les classes sont dans
//! `assets/skills/classes.json`. Le fichier dit sa place sur la roue (`wheel`), ses règles
//! (`cast`, lues par le serveur) et ce que le client en montre (`visuals`, `sounds`). Une
//! compétence sans `cast` s'apprend mais ne se lance pas encore.
//!
//! Chaque niveau du héros donne un point de compétence ; un point apprend une compétence, de
//! n'importe quelle classe, ou la renforce d'un rang, pourvu que celles qu'elle demande soient
//! déjà apprises. Un nœud de la roue peut donner plusieurs compétences (`grants`) : elles ont
//! alors le rang du nœud.
//!
//! Les dossiers sont lus au premier appel de `skills()`, depuis le dossier de lancement, sinon
//! celui des sources ; un fichier invalide est signalé et ignoré. Serveur et client lisent les
//! mêmes fichiers ; `SkillCatalog::fingerprint` permet de vérifier qu'ils sont d'accord.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::CharacterStats;

/// Dossier des compétences, depuis la racine du projet
pub const SKILLS_DIR: &str = "assets/skills";
/// Dossier des gestes du héros (le serveur n'en lit que la durée et l'impact)
pub const GESTURES_DIR: &str = "assets/hero/animations";
/// Rang maximal d'une compétence, sauf mention contraire
pub const DEFAULT_MAX_RANK: u8 = 3;
/// Dégâts d'une compétence de puissance 1, en attendant les dégâts propres aux armes
pub const SKILL_BASE_DAMAGE: f32 = 5.0;

// ------------------------------------------------------------------------------------------------
// Valeurs selon le rang

/// Un nombre, ou un nombre par rang : `3` ou `[3, 3.5, 4]`. Au-delà de la liste, la dernière
/// valeur vaut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Ranked {
    One(f32),
    PerRank(Vec<f32>),
}

impl Default for Ranked {
    fn default() -> Self {
        Ranked::One(0.0)
    }
}

impl Ranked {
    /// La valeur au rang `rank` (1 pour le premier ; 0 compte comme 1)
    pub fn at(&self, rank: u8) -> f32 {
        match self {
            Ranked::One(v) => *v,
            Ranked::PerRank(values) => {
                let i = (rank.max(1) as usize - 1).min(values.len().saturating_sub(1));
                values.get(i).copied().unwrap_or(0.0)
            }
        }
    }

    fn is_finite(&self) -> bool {
        match self {
            Ranked::One(v) => v.is_finite(),
            Ranked::PerRank(values) => !values.is_empty() && values.iter().all(|v| v.is_finite()),
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Format des fichiers

/// Une classe : une part de la roue
#[derive(Debug, Clone, Deserialize)]
pub struct SkillBranch {
    pub id: String,
    pub name: String,
    /// Famille de la classe : `magic`, `ranged` ou `melee`
    #[serde(default)]
    pub family: String,
    /// Répartition de départ de la classe entre mêlée, distance et magie (sur 100)
    #[serde(default)]
    pub mix: [u8; 3],
    /// Couleur de la part, en RGB
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Deserialize)]
struct ClassesFile {
    classes: Vec<SkillBranch>,
}

/// Place d'une compétence sur la roue
#[derive(Debug, Clone, Deserialize)]
pub struct WheelPlace {
    /// Palier : 1 et 2 pour les niveaux 1 et 2, 3 pour les fusions, 4 pour l'ultime
    pub tier: u8,
    /// Distance au centre, en anneaux (1 : le plus proche) ; le palier par défaut
    #[serde(default)]
    pub ring: Option<f32>,
    /// Position dans la part de sa classe, de 0 (début) à 1 (fin)
    #[serde(default = "half")]
    pub slot: f32,
    /// Compétences à connaître avant de pouvoir l'apprendre (toutes)
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default = "default_max_rank")]
    pub max_rank: u8,
    /// Numéro de la fiche du carnet Planner qui la décrit
    #[serde(default)]
    pub planner: Option<u32>,
    /// Compétences que ce nœud donne au lieu de se lancer lui-même (les deux sorts d'un élément)
    #[serde(default)]
    pub grants: Vec<String>,
}

/// Ce qu'une compétence coûte
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Cost {
    #[serde(default)]
    pub mana: Ranked,
    #[serde(default)]
    pub stamina: Ranked,
}

/// Centre d'une zone circulaire
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Center {
    /// Les pieds du lanceur
    #[default]
    #[serde(rename = "self")]
    Caster,
    /// Le point visé au sol, à `distance` blocs au plus
    Aimed,
}

/// Où frappe une compétence
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    /// Disque autour du lanceur ou du point visé
    Circle {
        radius: Ranked,
        #[serde(default)]
        center: Center,
        /// Portée de la visée, pour `center: aimed`
        #[serde(default = "default_aim_distance")]
        distance: Ranked,
    },
    /// Secteur devant le lanceur : portée en blocs, ouverture en degrés
    Arc { range: Ranked, angle: Ranked },
    /// Rectangle droit devant le lanceur
    Line { length: Ranked, width: Ranked },
    /// Le lanceur seul
    #[serde(rename = "self")]
    Caster,
}

/// Déplacement du lanceur, en blocs : `dash` vers l'avant, `leap` (négatif : en arrière) en
/// bondissant, du lancement à l'impact ; `hover` dès le lancement ; `grapple` à l'impact ;
/// `teleport` quand la frappe touche
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Movement {
    #[serde(default)]
    pub dash: Option<Ranked>,
    #[serde(default)]
    pub leap: Option<Ranked>,
    /// Bond de côté, en blocs : vers la gauche si le joueur tient la touche de gauche, sinon vers
    /// la droite (Esquive tactique)
    #[serde(default)]
    pub side: Option<Ranked>,
    #[serde(default)]
    pub teleport: Option<Teleport>,
    #[serde(default)]
    pub grapple: Option<Grapple>,
    #[serde(default)]
    pub hover: Option<Hover>,
}

/// Téléportation près de l'ennemi touché le plus proche du lanceur. La place se compte dans le
/// repère de la cible vue du lanceur : `behind` blocs au-delà d'elle (négatif : entre elle et le
/// lanceur), `above` au-dessus, `side` sur sa droite. Le serveur la calcule et l'envoie au lanceur.
#[derive(Debug, Clone, Deserialize)]
pub struct Teleport {
    #[serde(default)]
    pub behind: Ranked,
    #[serde(default)]
    pub above: Ranked,
    #[serde(default)]
    pub side: Ranked,
    /// Où le lanceur regarde en arrivant
    #[serde(default)]
    pub face: Facing,
    /// Secondes entre la frappe et la téléportation
    #[serde(default)]
    pub delay: Ranked,
}

/// Regard du lanceur à l'arrivée d'une téléportation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    /// Vers la cible
    #[default]
    Target,
    /// Dans la direction qu'il visait au lancement
    Aim,
    /// À l'opposé de la cible
    Away,
}

/// Le lanceur est tiré en ligne droite à l'impact : vers le bloc visé (`block`, il s'y hisse s'il
/// en atteint le rebord) ou vers l'ennemi touché le plus proche (`target`)
#[derive(Debug, Clone, Deserialize)]
pub struct Grapple {
    #[serde(default)]
    pub to: GrappleTo,
    /// Portée, en blocs : un bloc plus loin n'est pas agrippé
    pub range: Ranked,
    /// Vitesse de la traction, en blocs par seconde
    #[serde(default = "grapple_speed")]
    pub speed: Ranked,
    /// Distance où la traction s'arrête avant le point d'accroche, en blocs
    #[serde(default = "grapple_stop")]
    pub stop: Ranked,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrappleTo {
    #[default]
    Block,
    Target,
}

fn grapple_speed() -> Ranked {
    Ranked::One(25.0)
}

fn grapple_stop() -> Ranked {
    Ranked::One(0.0)
}

/// Lévitation : le lanceur monte de `height` blocs et flotte `duration` secondes, puis redescend
/// sans se blesser
#[derive(Debug, Clone, Deserialize)]
pub struct Hover {
    pub height: Ranked,
    pub duration: Ranked,
}

/// Un déplacement que le serveur impose au lanceur (`SkillTick::moves`)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SkillMove {
    /// Il apparaît aux pieds `feet`, le regard dans la direction `look`
    Teleport { feet: Vec3, look: Vec3 },
    /// Il est tiré vers `to` à `speed` blocs par seconde, jusqu'à `stop` blocs de lui
    Pull { to: Vec3, speed: f32, stop: f32 },
}

/// Une seconde frappe, `delay` secondes après la première, sur les ennemis qu'elle a trouvés dans
/// sa zone
#[derive(Debug, Clone, Deserialize)]
pub struct FollowUp {
    pub delay: Ranked,
    pub effects: Vec<EffectEntry>,
}

/// Qui subit les effets
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Targets {
    #[default]
    Enemies,
    /// Les joueurs et les bêtes apprivoisées, lanceur compris
    Allies,
    #[serde(rename = "self")]
    Caster,
    All,
    /// Les entités invoquées par le lanceur
    Summons,
}

/// Ce que subit une cible
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// `power` × `SKILL_BASE_DAMAGE` points de vie retirés, de type `kind` (celui de la compétence,
    /// `Cast::damage_kind`, par défaut). `pierce` : part de l'armure de la cible que ce coup ignore
    /// (0,5 : la moitié ; 1 : toute)
    Damage {
        power: Ranked,
        #[serde(default)]
        kind: Option<crate::DamageKind>,
        #[serde(default)]
        pierce: Ranked,
    },
    /// Brise l'armure de la cible : `amount` de son armure en moins (1 : toute) pendant `duration`
    /// secondes, pour tous les coups. Les bris s'additionnent jusqu'à `max` (1 par défaut). Placé
    /// avant les dégâts, il compte dès le coup qui le porte.
    Shred {
        amount: Ranked,
        duration: Ranked,
        #[serde(default)]
        max: Option<Ranked>,
    },
    /// Points de vie rendus
    Heal { amount: Ranked },
    /// Vitesse réduite de `strength` (0 à 1) pendant `duration` secondes
    Slow { strength: Ranked, duration: Ranked },
    /// Plus de mouvement ni d'attaque pendant `duration` secondes
    Stun { duration: Ranked },
    /// Repoussée loin du lanceur (vitesse de départ, en blocs par seconde)
    Knockback { strength: Ranked },
    /// Attirée vers le lanceur
    Pull { strength: Ranked },
    /// Projetée à `height` blocs en l'air : étourdie tant qu'elle vole, elle se blesse en retombant
    Launch { height: Ranked },
    /// Rabattue au sol : à l'atterrissage, `power` × `SKILL_BASE_DAMAGE` points de vie retirés
    Slam { power: Ranked },
    /// Points de dégâts absorbés pendant `duration` secondes
    Shield {
        amount: Ranked,
        duration: Ranked,
        /// Ce que le bouclier rend à son porteur à mesure qu'il encaisse (`{ "mana": 0.5 }` : la
        /// moitié des dégâts absorbés, en mana)
        #[serde(default)]
        absorb_to: Option<Absorb>,
        /// Il n'absorbe que les coups de ce type (aucun : tous)
        #[serde(default)]
        absorbs: Option<crate::DamageKind>,
    },
    /// Statistique augmentée de `value` (0,3 : +30 %) pendant `duration` secondes
    Buff {
        stat: BuffStat,
        value: Ranked,
        duration: Ranked,
        /// Pour `defense` : ne réduit que les coups de ce type (aucun : tous)
        #[serde(default)]
        kind: Option<crate::DamageKind>,
        /// Dégâts en plus contre une famille de créatures (`tags` de l'espèce)
        #[serde(default)]
        against: Option<Against>,
        /// La valeur suit la vie qui manque au joueur : nulle à pleine vie, entière au bord de la mort
        #[serde(default)]
        per_missing_health: bool,
        /// Si le joueur tombe à zéro point de vie tant que le renforcement dure, ces effets partent
        /// autour de lui et il survit, le renforcement consumé
        #[serde(default)]
        on_death: Option<Around>,
        /// Tant que le renforcement dure, chaque attaque du joueur qui touche joue ces effets autour
        /// de ses cibles
        #[serde(default)]
        rider: Option<Around>,
    },
    /// Dégâts (`damage`) ou soins (`heal`) par seconde pendant `duration` secondes
    OverTime {
        #[serde(default)]
        damage: Option<Ranked>,
        #[serde(default)]
        heal: Option<Ranked>,
        duration: Ranked,
        /// Facteur des dégâts tant que la bête bouge (saignement qui s'aggrave : 3, trois fois plus)
        #[serde(default)]
        moving: Option<Ranked>,
    },
    /// Fait apparaître des entités alliées : créatures, tourelles, pièges, murs, invocations
    /// humanoïdes (une espèce de `assets/creatures/`)
    Summon {
        species: String,
        /// Combien à chaque lancement
        #[serde(default = "one")]
        count: Ranked,
        /// Où elles apparaissent : aux pieds du lanceur (`self`) ou au point visé (`aimed`)
        #[serde(default)]
        at: Center,
        /// Secondes avant qu'elles disparaissent (sans : jusqu'à leur destruction)
        #[serde(default)]
        duration: Option<Ranked>,
        /// Nombre au plus en même temps pour ce lanceur : les plus anciennes disparaissent
        #[serde(default)]
        limit: Option<Ranked>,
        /// Elles prennent une autre allure : celle du lanceur (`caster`, un leurre), ou celle d'un
        /// cadavre de la zone (`corpse` : une par cadavre, à sa place, qui le remplace)
        #[serde(default)]
        copies: Option<Copies>,
        /// Effets sur le lanceur quand chacune disparaît au bout de sa durée (les ombres de la
        /// Légion du Néant implosent en bouclier)
        #[serde(default)]
        on_end: Vec<EffectEntry>,
        /// Elles avancent droit devant, dans la direction visée, en repoussant les ennemis (le mur
        /// du Rempart Tellurique)
        #[serde(default)]
        advance: Option<Advance>,
    },
    /// La créature s'en prend au lanceur pendant `duration` secondes
    Taunt { duration: Ranked },
    /// Le joueur disparaît aux yeux des bêtes pendant `duration` secondes ; sa première attaque le
    /// dévoile, et porte un coup critique
    Invisible { duration: Ranked },
    /// La bête aveuglée ne voit plus qu'à un bloc et demi pendant `duration` secondes
    Blind { duration: Ranked },
    /// La bête marquée encaisse plus souvent les coups critiques du lanceur : `chance` en plus (1 :
    /// toujours) pendant `duration` secondes
    Mark { chance: Ranked, duration: Ranked },
    /// Lie l'allié au lanceur pendant `duration` secondes : `share` des dégâts qu'il subit (0,4 : 40 %)
    /// sont encaissés par le lanceur à sa place. Avec `hits`, le lien se défait après tant de coups.
    Link {
        share: Ranked,
        duration: Ranked,
        #[serde(default)]
        hits: Option<Ranked>,
    },
    /// Points de vie que perd le joueur (une frappe qui blesse son lanceur)
    Sacrifice { health: Ranked },
    /// Retire au joueur son ralenti, son étourdissement et ses dégâts sur la durée
    Cleanse {},
    /// Points de mana et d'endurance rendus
    Restore {
        #[serde(default)]
        mana: Ranked,
        #[serde(default)]
        stamina: Ranked,
    },
    /// Les alliés tombés dans la zone se relèvent sur place avec `health` de leur vie (0,5 : la moitié)
    Revive { health: Ranked },
    /// Posture de garde du joueur pendant `duration` secondes : les coups des bêtes qui arrivent
    /// de face sont bloqués, et chaque coup paré peut déclencher une riposte
    Guard(Guard),
    /// Le joueur ignore ces états pendant `duration` secondes ; ceux qu'il subit déjà tombent
    Immune { from: Vec<Status>, duration: Ranked },
    /// La garde de la bête (`guard` de son espèce) tombe pendant `duration` secondes
    BreakGuard { duration: Ranked },
    /// Pendant `duration` secondes, les tirs ennemis qui arrivent sur le joueur de face, dans un
    /// arc de `arc` degrés autour de la direction visée au lancement, s'arrêtent sur lui (Mur vivant)
    ShotGuard { arc: Ranked, duration: Ranked },
    /// Pendant `duration` secondes, les projectiles de cette taille (`small` : les petits ; `large` :
    /// tous) qui atteignent le joueur sont déviés (Manteau de Zéphyr)
    Deflect {
        #[serde(default)]
        size: crate::creature::ProjectileSize,
        duration: Ranked,
    },
    /// Le chargeur de l'arbalète tenue se remplit d'un coup (Esquive tactique, Roulade Évasive)
    Reload {},
    /// Munition spéciale : les `shots` prochains tirs du joueur (tirs d'arc ou d'arbalète et
    /// compétences de tir), dans les `duration` secondes, portent en plus `effects` à ce qu'ils
    /// touchent ; avec `radius`, à tous les ennemis dans ce rayon autour de l'impact (Balles à
    /// Fragmentation)
    LoadAmmo {
        shots: Ranked,
        duration: Ranked,
        #[serde(default)]
        radius: Option<Ranked>,
        effects: Vec<EffectEntry>,
    },
}

/// Une garde (`Effect::Guard`)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    /// Secondes que dure la fenêtre
    pub duration: Ranked,
    /// Part des dégâts bloqués (1 : tout)
    #[serde(default = "one")]
    pub block: Ranked,
    /// Angle couvert devant le joueur, en degrés (360 ou rien : de tous les côtés)
    #[serde(default)]
    pub arc: Option<f32>,
    /// Coups parés avant que la garde tombe (rien : jusqu'à la fin de la fenêtre)
    #[serde(default)]
    pub hits: Option<Ranked>,
    /// Types de dégâts bloqués (`physical`, `magical`) ; vide : tous. Les coups des bêtes sont
    /// physiques tant qu'elles n'ont pas de type de dégâts.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Riposte : des effets joués à chaque coup paré, autour de l'attaquant ou du joueur
    #[serde(default)]
    pub counter: Option<Counter>,
    /// Renvoie à l'attaquant tant de fois les dégâts bloqués (2 : le double)
    #[serde(default)]
    pub reflect: Option<Ranked>,
    /// Facteur de la recharge quand la fenêtre se ferme sans avoir rien paré (2 : deux fois plus
    /// longue). Par défaut 2 pour une garde qui riposte, 1 sinon.
    #[serde(default)]
    pub miss_cooldown: Option<f32>,
}

impl Guard {
    /// Elle riposte (`counter` ou `reflect`)
    pub fn ripostes(&self) -> bool {
        self.counter.is_some() || self.reflect.is_some()
    }

    /// Facteur de la recharge après une garde qui n'a rien paré
    pub fn miss_factor(&self) -> f32 {
        self.miss_cooldown.unwrap_or(if self.ripostes() { 2.0 } else { 1.0 })
    }

    /// Elle bloque les dégâts de type `kind` (`physical`, `magical`)
    pub fn blocks(&self, kind: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind)
    }
}

/// La riposte d'une garde (`Guard::counter`)
#[derive(Debug, Clone, Deserialize)]
pub struct Counter {
    /// Autour de qui elle part : l'attaquant (par défaut) ou le joueur
    #[serde(default)]
    pub at: CounterAt,
    #[serde(flatten)]
    pub around: Around,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CounterAt {
    #[default]
    Attacker,
    #[serde(rename = "self")]
    Caster,
}

/// Ce qu'une immunité fait ignorer (`Effect::Immune`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stun,
    Slow,
    Knockback,
    Pull,
    Damage,
}

/// Un vrai projectile (`Cast::projectile`) : le tir part de la main du lanceur dans la direction
/// visée (vers le point visé pour une zone `circle` visée), vole et touche ce qu'il rencontre. La
/// zone `line` donne sa portée (longueur) et son épaisseur (largeur).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projectile {
    /// Vitesse, en blocs par seconde
    #[serde(default = "projectile_speed")]
    pub speed: Ranked,
    #[serde(default)]
    pub size: crate::creature::ProjectileSize,
    /// Ennemis traversés avant de s'arrêter
    #[serde(default)]
    pub pierce: Pierce,
    /// Après chaque touche, il repart vers l'ennemi le plus proche pas encore touché
    #[serde(default)]
    pub bounce: Option<Bounce>,
    /// Ce qu'il traverse sans s'arrêter
    #[serde(default)]
    pub through: Vec<Through>,
}

fn projectile_speed() -> Ranked {
    Ranked::One(40.0)
}

/// Ennemis qu'un projectile traverse : un nombre, ou tous (`"all"`)
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Pierce {
    Count(Ranked),
    All(PierceAll),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PierceAll {
    All,
}

impl Default for Pierce {
    fn default() -> Self {
        Pierce::Count(Ranked::One(0.0))
    }
}

impl Pierce {
    /// Ennemis traversés au rang `rank` (`u32::MAX` : tous)
    pub fn at(&self, rank: u8) -> u32 {
        match self {
            Pierce::Count(n) => n.at(rank).max(0.0).round() as u32,
            Pierce::All(_) => u32::MAX,
        }
    }
}

/// Rebonds d'un projectile : combien, et jusqu'à quelle distance de la cible touchée, en blocs
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounce {
    pub count: Ranked,
    pub range: Ranked,
}

/// Ce qu'un projectile peut traverser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Through {
    /// Le relief
    Terrain,
    /// Les murs et les gardes
    Obstacles,
}

/// Une zone qui reste posée après la frappe (`Cast::zone`). Deux fois par seconde, elle rejoue les
/// effets qui durent sur ceux qui s'y trouvent : les dégâts et soins par seconde comptent tant
/// qu'on y est, ralenti, aveuglement, invisibilité et renforcements cessent quand on en sort. La
/// durée de ces effets est celle de la zone. Les effets d'un coup (dégâts, soin, recul,
/// étourdissement, invocation) ne jouent qu'à la frappe.
#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    /// Secondes que dure la zone
    pub duration: Ranked,
    /// Elle suit le lanceur (une aura) au lieu de rester où elle a été posée
    #[serde(default)]
    pub follow: bool,
}

/// Des effets joués dans un rayon autour d'un point : un cadavre (`Cast::on_kill`), les cibles d'une
/// attaque (`rider`), le joueur qui tombe (`on_death`). Chaque effet dit à qui il va (`to`) : les
/// ennemis par défaut.
#[derive(Debug, Clone, Deserialize)]
pub struct Around {
    /// Rayon, en blocs
    pub radius: Ranked,
    pub effects: Vec<EffectEntry>,
}

/// Part des dégâts absorbés par un bouclier qu'il rend en mana (`Effect::Shield::absorb_to`)
#[derive(Debug, Clone, Deserialize)]
pub struct Absorb {
    pub mana: Ranked,
}

/// Bonus de dégâts contre les créatures d'une famille (`Effect::Buff::against`)
#[derive(Debug, Clone, Deserialize)]
pub struct Against {
    /// Étiquette de l'espèce (`tags` dans sa fiche) : `shadow` pour les créatures de l'ombre
    pub tag: String,
    /// Dégâts en plus contre elles (0,5 : +50 %), ajoutés à `value`
    pub value: Ranked,
}

/// Une invocation qui avance (`Effect::Summon::advance`)
#[derive(Debug, Clone, Deserialize)]
pub struct Advance {
    /// Distance parcourue, en blocs
    pub blocks: Ranked,
    /// Vitesse, en blocs par seconde
    pub speed: Ranked,
}

/// L'allure que prend une invocation (`Effect::Summon::copies`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Copies {
    Caster,
    Corpse,
}

/// Statistique qu'un `buff` augmente
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuffStat {
    /// Dégâts des compétences et des coups
    Damage,
    /// Dégâts subis réduits
    Defense,
    /// Vitesse de déplacement
    Speed,
    /// Régénération de la vie
    Regeneration,
    /// Chance de coup critique (0,3 : 30 % ; un critique double les dégâts)
    Critical,
    /// Portée des compétences : distance de visée, longueur des lignes et des secteurs (0,3 : +30 %)
    Range,
    /// Vitesse d'attaque : coups d'arme, rechargement de l'arbalète, cadence de tir des
    /// invocations (0,5 : 50 % plus vite). `haste` est l'ancien nom.
    #[serde(alias = "haste")]
    AttackSpeed,
    /// Récupération du mana (0,3 : +30 % de la récupération au repos)
    ManaRegen,
    /// Récupération de l'endurance
    StaminaRegen,
}

/// Un effet, et à qui il s'applique s'il ne vise pas les cibles de la compétence (`to`)
#[derive(Debug, Clone, Deserialize)]
pub struct EffectEntry {
    #[serde(flatten)]
    pub effect: Effect,
    #[serde(default)]
    pub to: Option<Targets>,
}

/// Les règles d'une compétence, que le serveur applique
#[derive(Debug, Clone, Deserialize)]
pub struct Cast {
    /// Familles d'arme à tenir en main pour la lancer (vide : n'importe quoi, mains nues comprises)
    #[serde(default)]
    pub weapons: Vec<crate::weapons::WeaponKind>,
    /// Elle tire un carreau du chargeur de l'arbalète : refusée tant qu'elle recharge (Tir de Recul)
    #[serde(default)]
    pub uses_round: bool,
    /// Elle se charge : on tient la touche, elle part au relâchement (Tir de Baliste)
    #[serde(default)]
    pub charge: Option<Charge>,
    #[serde(default)]
    pub cost: Cost,
    /// Secondes avant de pouvoir la relancer
    #[serde(default)]
    pub cooldown: Ranked,
    /// Geste du héros (`assets/hero/animations/<nom>.json`)
    pub animation: String,
    /// Instant du geste (0 à 1) où la zone frappe ; celui du geste par défaut
    #[serde(default)]
    pub impact: Option<f32>,
    /// Le geste plus vite (2 : deux fois plus court) ou plus lentement
    #[serde(default = "one_f32")]
    pub speed: f32,
    pub area: Area,
    #[serde(default)]
    pub movement: Option<Movement>,
    #[serde(default)]
    pub targets: Targets,
    #[serde(default)]
    pub effects: Vec<EffectEntry>,
    /// Effets sur le lanceur, quelle que soit la zone
    #[serde(default)]
    pub self_effects: Vec<EffectEntry>,
    /// Le cercle visé se pose sur la bête marquée (`mark`) par le lanceur la plus proche du point visé,
    /// à portée
    #[serde(default)]
    pub homing: bool,
    /// La zone reste posée : ses effets valent tant qu'on s'y trouve, pour qui y entre après la
    /// frappe aussi
    #[serde(default)]
    pub zone: Option<Zone>,
    /// Effets sur le lanceur pour chaque ennemi que la frappe touche
    #[serde(default)]
    pub on_hit: Vec<EffectEntry>,
    /// Effets sur le lanceur si la frappe ne touche aucun ennemi
    #[serde(default)]
    pub on_miss: Vec<EffectEntry>,
    /// Effets autour de chaque ennemi que la frappe tue
    #[serde(default)]
    pub on_kill: Option<Around>,
    /// Un vrai projectile : la compétence ne frappe pas sa zone d'un coup, son tir vole et touche
    /// ce qu'il rencontre
    #[serde(default)]
    pub projectile: Option<Projectile>,
    /// Seconde frappe sur les ennemis trouvés par la première
    #[serde(default)]
    pub follow_up: Option<FollowUp>,
    /// Type de ses dégâts (coups, dégâts sur la durée, zones). Sans : magique si elle coûte de la
    /// mana, physique sinon (`damage_kind`)
    #[serde(default)]
    pub kind: Option<crate::DamageKind>,
}

impl Cast {
    /// Type des dégâts de la compétence : `kind`, sinon celui de sa ressource
    pub fn damage_kind(&self) -> crate::DamageKind {
        self.kind.unwrap_or(if self.cost.mana.at(1) > 0.0 { crate::DamageKind::Magical } else { crate::DamageKind::Physical })
    }
}

/// Une compétence qui se charge (`Cast::charge`) : relâchée avant `min` secondes, elle ne part pas
/// (et n'est pas payée) ; au-delà, sa puissance va de `power[0]` à `power[1]`, atteinte à `max`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub min: f32,
    pub max: f32,
    pub power: [f32; 2],
}

impl Charge {
    /// Puissance au bout de `held` secondes de charge (`None` : pas assez)
    pub fn power_after(&self, held: f32) -> Option<f32> {
        if held + 1e-3 < self.min {
            return None;
        }
        let t = if self.max > self.min { ((held - self.min) / (self.max - self.min)).clamp(0.0, 1.0) } else { 1.0 };
        Some(self.power[0] + (self.power[1] - self.power[0]) * t)
    }
}

/// Les règles de la Fusion : elle mêle les deux derniers sorts élémentaires lancés
#[derive(Debug, Clone, Deserialize)]
pub struct FusionRules {
    /// Deux éléments différents (nœuds de la roue qui donnent les sorts) et la compétence qu'ils font
    pub mixes: Vec<FusionMix>,
    #[serde(default)]
    pub amplify: Amplify,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FusionMix {
    pub elements: [String; 2],
    pub skill: String,
}

/// Deux sorts du même élément : le dernier est relancé amplifié
#[derive(Debug, Clone, Deserialize)]
pub struct Amplify {
    /// Facteur des dégâts, soins, boucliers et reculs
    #[serde(default = "amplify_power")]
    pub power: f32,
    /// Facteur de la taille de la zone
    #[serde(default = "amplify_area")]
    pub area: f32,
    /// Facteur du coût
    #[serde(default = "amplify_cost")]
    pub cost: f32,
    /// Visuels joués en plus de ceux du sort
    #[serde(default)]
    pub visuals: Vec<Visual>,
    /// Icône de l'amplification de chaque élément (fichier à côté de celui de la Fusion)
    #[serde(default)]
    pub icons: BTreeMap<String, String>,
}

impl Default for Amplify {
    fn default() -> Self {
        Self { power: amplify_power(), area: amplify_area(), cost: amplify_cost(), visuals: Vec::new(), icons: BTreeMap::new() }
    }
}

fn amplify_power() -> f32 {
    1.75
}

fn amplify_area() -> f32 {
    1.3
}

fn amplify_cost() -> f32 {
    1.5
}

/// Ce que la Fusion lance : un mélange de deux éléments, ou le dernier sort amplifié
#[derive(Debug, Clone, PartialEq)]
pub enum Fused {
    Mix(String),
    Amplified { spell: String, element: String },
}

impl Fused {
    /// La compétence lancée
    pub fn skill(&self) -> &str {
        match self {
            Fused::Mix(s) => s,
            Fused::Amplified { spell, .. } => spell,
        }
    }
}

/// Moment d'un visuel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Moment {
    /// Le geste commence
    Start,
    /// L'instant d'impact du geste
    Impact,
    /// Une cible est touchée
    Hit,
    /// Fin du geste
    End,
    /// Fin de l'effet sur la cible (ralenti, étourdi, bouclier…)
    EffectEnd,
    /// Le projectile s'arrête sur un mur, une garde ou le relief
    Blocked,
    /// Une garde pare un coup : la riposte part
    Parry,
}

/// Où un visuel se joue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    RightHand,
    LeftHand,
    Weapon,
    Head,
    Body,
    Feet,
    /// Le point visé au sol
    Aimed,
    /// Chaque créature touchée
    Target,
    /// Le centre de la zone de la compétence
    Area,
    /// Le projectile de la compétence, de son départ à son arrêt (`Cast::projectile`)
    Projectile,
}

/// Un effet visuel joué par la compétence
#[derive(Debug, Clone, Deserialize)]
pub struct Visual {
    /// Effet de `assets/effects/`
    pub effect: String,
    pub anchor: Anchor,
    pub from: Moment,
    /// Fin du visuel ; sans elle, il dure le temps de ses couches
    #[serde(default)]
    pub to: Option<Moment>,
}

#[derive(Debug, Clone, Deserialize)]
struct SkillFile {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    wheel: Option<WheelPlace>,
    #[serde(default)]
    cast: Option<Cast>,
    #[serde(default)]
    visuals: Vec<Visual>,
    #[serde(default)]
    sounds: BTreeMap<String, String>,
    #[serde(default)]
    special: Option<String>,
    #[serde(default)]
    graft: Option<String>,
    #[serde(default)]
    form: Option<String>,
    #[serde(default)]
    form_attacks: Vec<String>,
    #[serde(default)]
    fusion: Option<FusionRules>,
}

/// Une compétence lue dans son fichier
#[derive(Debug, Clone)]
pub struct SkillDef {
    pub id: String,
    /// Classe (nom du dossier)
    pub class: String,
    pub name: String,
    pub description: String,
    /// Chemin de l'icône, si le fichier existe
    pub icon: Option<PathBuf>,
    pub wheel: Option<WheelPlace>,
    pub cast: Option<Cast>,
    pub visuals: Vec<Visual>,
    pub sounds: BTreeMap<String, String>,
    /// Mécanique écrite en Rust, appelée par son nom (ultimes et cas uniques)
    pub special: Option<String>,
    /// Pièce greffée sur le lanceur tant que durent ses effets (`assets/skills/models/<id>.gltf` :
    /// patte de loup, fourrure d'ours… du Druide)
    pub graft: Option<String>,
    /// Forme complète prise par le lanceur tant que durent ses effets : une espèce de
    /// `assets/creatures/` dessinée à la place du héros (formes du Druide)
    pub form: Option<String>,
    /// Les deux attaques de la forme, au clic gauche puis au clic droit : des compétences sans
    /// place sur la roue, apprises avec la forme et jouables seulement sous elle
    pub form_attacks: Vec<String>,
    /// La Fusion : elle lance le mélange des deux derniers sorts élémentaires
    pub fusion: Option<FusionRules>,
}

impl SkillDef {
    /// Se lance : elle a des règles (ce qu'elle invoque est vérifié au chargement), ou c'est la Fusion
    pub fn is_castable(&self) -> bool {
        self.cast.is_some() || self.fusion.is_some()
    }

    /// Les apparitions de la compétence (`summon`), dans l'ordre du fichier
    pub fn summons(&self) -> impl Iterator<Item = &Effect> {
        self.cast.iter().flat_map(|c| c.effects.iter().chain(&c.self_effects)).map(|e| &e.effect).filter(|e| matches!(e, Effect::Summon { .. }))
    }
}

/// Durée et impact d'un geste, la seule partie des gestes que les règles lisent
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct GestureTiming {
    pub duration: f32,
    #[serde(default = "half")]
    pub impact: f32,
}

fn half() -> f32 {
    0.5
}

fn one() -> Ranked {
    Ranked::One(1.0)
}

fn one_f32() -> f32 {
    1.0
}

fn default_aim_distance() -> Ranked {
    Ranked::One(12.0)
}

fn default_max_rank() -> u8 {
    DEFAULT_MAX_RANK
}

// ------------------------------------------------------------------------------------------------
// Catalogue

/// Toutes les compétences, leurs classes et la durée des gestes
#[derive(Debug, Clone, Default)]
pub struct SkillCatalog {
    pub classes: Vec<SkillBranch>,
    skills: Vec<SkillDef>,
    index: HashMap<String, usize>,
    /// Nœud de la roue qui donne chaque compétence accordée (`grants`)
    granted_by: HashMap<String, String>,
    /// Attaque de forme -> la forme (compétence) qui la donne
    form_attack_of: HashMap<String, String>,
    /// Mélange -> la Fusion qui le lance
    mix_of: HashMap<String, String>,
    /// Icônes des amplifications, par élément
    amplify_icons: HashMap<String, PathBuf>,
    gestures: HashMap<String, GestureTiming>,
    fingerprint: u64,
}

static SKILLS: OnceLock<SkillCatalog> = OnceLock::new();

/// Les compétences du jeu, lues dans `assets/skills/` au premier appel
pub fn skills() -> &'static SkillCatalog {
    SKILLS.get_or_init(SkillCatalog::load_default)
}

/// Le dossier `dir` du projet : depuis le dossier de lancement, sinon celui des sources
pub fn asset_dir(dir: &str) -> Option<PathBuf> {
    [PathBuf::from(dir), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(dir)].into_iter().find(|d| d.is_dir())
}

/// Fichier JSON illisible
#[derive(Debug, thiserror::Error)]
enum FileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Ce qui cloche dans une fiche de compétence : nombres, références aux autres compétences, aux
/// gestes et aux espèces
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    #[error("palier de 1 à 4, anneau et rang maximal à partir de 1")]
    Wheel,
    #[error("compétence inconnue {0}")]
    UnknownSkill(String),
    #[error("fusion : compétence inconnue {0}")]
    FusionUnknownSkill(String),
    #[error("fusion : deux éléments différents par mélange (un même élément deux fois s'amplifie)")]
    FusionSameElement,
    #[error("fusion : facteurs d'amplification positifs")]
    FusionAmplify,
    #[error("geste inconnu {0}")]
    UnknownGesture(String),
    #[error("speed positive, impact de 0 à 1")]
    SpeedOrImpact,
    #[error("nombre invalide dans cast")]
    CastNumber,
    #[error("projectile : vitesse positive, nombres valides")]
    ProjectileNumbers,
    #[error("projectile : zone line ou circle (portée et point visé)")]
    ProjectileArea,
    #[error("grapple : range et speed positives")]
    Grapple,
    #[error("hover : height en nombre, duration positive")]
    Hover,
    #[error("teleport : nombres, delay positif ou nul")]
    Teleport,
    #[error("teleport et grapple vers la cible : une zone qui touche des ennemis")]
    MovementArea,
    #[error("follow_up : delay positif ou nul")]
    FollowUp,
    #[error("charge : 0 ≤ min ≤ max ≤ 10, puissances positives")]
    Charge,
    #[error("uses_round : une compétence d'arbalète (weapons)")]
    UsesRound,
    #[error("forme inconnue {0} (absente de assets/creatures/)")]
    UnknownForm(String),
    #[error("form_attacks : deux attaques (clic gauche, clic droit), avec une forme")]
    FormAttacks,
    #[error("attaque de forme inconnue {0}")]
    UnknownFormAttack(String),
    #[error("invocation inconnue {0} (absente de assets/creatures/)")]
    UnknownSummon(String),
    #[error("invocation {0} : count à partir de 1, duration et limit en nombres")]
    SummonNumbers(String),
    #[error("guard : nombres finis, arc de 0 à 360 degrés, miss_cooldown positif")]
    GuardNumbers,
    #[error("guard : type de dégâts inconnu {0} (physical, magical)")]
    GuardKind(String),
    #[error("immune : au moins un état dans from, et une durée")]
    Immune,
    #[error("damage : pierce de 0 à 1")]
    Pierce,
    #[error("shred : amount et max de 0 à 1, duration en secondes")]
    Shred,
}

impl SkillCatalog {
    /// Les compétences de `assets/skills/` et les gestes de `assets/hero/animations/`. Un fichier
    /// invalide est signalé et ignoré.
    pub fn load_default() -> Self {
        let Some(dir) = asset_dir(SKILLS_DIR) else {
            log::warn!("Dossier {SKILLS_DIR}/ introuvable : aucune compétence.");
            return Self::default();
        };
        let gestures = asset_dir(GESTURES_DIR).unwrap_or_default();
        let (catalog, errors) = Self::load_dirs(&dir, &gestures);
        for error in &errors {
            log::warn!("{error}");
        }
        log::info!("{} compétence(s) chargée(s) depuis {}.", catalog.skills.len(), dir.display());
        catalog
    }

    /// Lit les classes et les compétences de `dir`, les gestes de `gestures`, et renvoie aussi les
    /// erreurs trouvées (fichier illisible, référence inconnue) : les fichiers fautifs sont ignorés
    pub fn load_dirs(dir: &Path, gestures: &Path) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut hash = Fnv::default();
        let classes = match std::fs::read_to_string(dir.join("classes.json")) {
            Ok(text) => {
                hash.write(text.as_bytes());
                match serde_json::from_str::<ClassesFile>(&text) {
                    Ok(file) => file.classes,
                    Err(e) => {
                        errors.push(format!("classes.json : {e}"));
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                errors.push(format!("classes.json : {e}"));
                Vec::new()
            }
        };

        let mut timing = HashMap::new();
        for path in json_files(gestures) {
            let name = stem(&path);
            match std::fs::read_to_string(&path).map_err(FileError::from).and_then(|t| Ok(serde_json::from_str::<GestureTiming>(&t)?)) {
                Ok(t) if t.duration > 0.0 && t.duration.is_finite() => {
                    timing.insert(name, t);
                }
                Ok(_) => errors.push(format!("geste {name} : durée invalide")),
                Err(e) => errors.push(format!("geste {name} : {e}")),
            }
        }

        let mut skills = Vec::new();
        for class in &classes {
            for path in json_files(&dir.join(&class.id)) {
                let id = stem(&path);
                let text = match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(format!("{}/{id} : {e}", class.id));
                        continue;
                    }
                };
                match serde_json::from_str::<SkillFile>(&text) {
                    Ok(file) => {
                        hash.write(class.id.as_bytes());
                        hash.write(id.as_bytes());
                        hash.write(text.as_bytes());
                        let icon = file.icon.clone().unwrap_or_else(|| format!("{id}.png"));
                        let icon = Some(dir.join(&class.id).join(icon)).filter(|p| p.is_file());
                        skills.push(SkillDef {
                            id,
                            class: class.id.clone(),
                            name: file.name,
                            description: file.description,
                            icon,
                            wheel: file.wheel,
                            cast: file.cast,
                            visuals: file.visuals,
                            sounds: file.sounds,
                            special: file.special,
                            graft: file.graft,
                            form: file.form,
                            form_attacks: file.form_attacks,
                            fusion: file.fusion,
                        });
                    }
                    Err(e) => errors.push(format!("{}/{id}.json : {e}", class.id)),
                }
            }
        }

        // Ce qu'une compétence peut faire apparaître : les espèces de `assets/creatures/` (à côté
        // de `assets/skills/`)
        let creatures: HashSet<String> = dir.parent().map(|d| json_files(&d.join("creatures")).iter().map(|p| stem(p)).collect()).unwrap_or_default();

        let mut catalog = Self { classes, skills: Vec::new(), index: HashMap::new(), granted_by: HashMap::new(), form_attack_of: HashMap::new(), mix_of: HashMap::new(), amplify_icons: HashMap::new(), gestures: timing, fingerprint: 0 };
        // Les fichiers en double ou mal formés sont écartés, puis les références vérifiées
        let mut seen = HashSet::new();
        skills.retain(|s| {
            let fresh = seen.insert(s.id.clone());
            if !fresh {
                errors.push(format!("{} : compétence en double", s.id));
            }
            fresh
        });
        let ids: HashSet<String> = skills.iter().map(|s| s.id.clone()).collect();
        // Une pièce greffée sans modèle n'est pas dessinée
        for s in &mut skills {
            if let Some(graft) = s.graft.take() {
                if dir.join("models").join(format!("{graft}.gltf")).is_file() {
                    s.graft = Some(graft);
                } else {
                    errors.push(format!("{}/{} : pièce greffée inconnue {graft}", s.class, s.id));
                }
            }
        }
        skills.retain(|s| match catalog.check(s, &ids, &creatures) {
            Ok(()) => true,
            Err(e) => {
                errors.push(format!("{}/{} : {e}", s.class, s.id));
                false
            }
        });
        for s in &skills {
            for g in s.wheel.iter().flat_map(|w| &w.grants) {
                catalog.granted_by.insert(g.clone(), s.id.clone());
            }
            // Les attaques d'une forme s'apprennent avec elle
            for a in &s.form_attacks {
                catalog.granted_by.insert(a.clone(), s.id.clone());
                catalog.form_attack_of.insert(a.clone(), s.id.clone());
            }
            if let Some(fusion) = &s.fusion {
                for m in &fusion.mixes {
                    catalog.mix_of.insert(m.skill.clone(), s.id.clone());
                }
                for (element, icon) in &fusion.amplify.icons {
                    let path = dir.join(&s.class).join(icon);
                    if path.is_file() {
                        catalog.amplify_icons.insert(element.clone(), path);
                    } else {
                        errors.push(format!("{}/{} : icône d'amplification absente {icon}", s.class, s.id));
                    }
                }
            }
        }
        // Une compétence ni sur la roue ni accordée par un nœud ne s'apprendrait jamais
        for s in &skills {
            if s.wheel.is_none() && !catalog.granted_by.contains_key(&s.id) {
                errors.push(format!("{}/{} : ni sur la roue, ni donnée par un nœud", s.class, s.id));
            }
        }
        catalog.index = skills.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        catalog.skills = skills;
        catalog.fingerprint = hash.finish();
        (catalog, errors)
    }

    /// Ce qui cloche dans `skill` : nombres, références aux autres compétences et aux gestes
    fn check(&self, skill: &SkillDef, ids: &HashSet<String>, entities: &HashSet<String>) -> Result<(), SkillError> {
        if let Some(w) = &skill.wheel {
            if !(1..=4).contains(&w.tier) || w.max_rank == 0 || w.ring.unwrap_or(w.tier as f32) < 1.0 {
                return Err(SkillError::Wheel);
            }
            if let Some(missing) = w.requires.iter().chain(&w.grants).find(|r| !ids.contains(*r)) {
                return Err(SkillError::UnknownSkill(missing.clone()));
            }
        }
        if let Some(fusion) = &skill.fusion {
            if let Some(missing) = fusion.mixes.iter().flat_map(|m| m.elements.iter().chain([&m.skill])).find(|r| !ids.contains(*r)) {
                return Err(SkillError::FusionUnknownSkill(missing.clone()));
            }
            if fusion.mixes.iter().any(|m| m.elements[0] == m.elements[1]) {
                return Err(SkillError::FusionSameElement);
            }
            let a = &fusion.amplify;
            if ![a.power, a.area, a.cost].iter().all(|v| v.is_finite() && *v > 0.0) {
                return Err(SkillError::FusionAmplify);
            }
        }
        if let Some(cast) = &skill.cast {
            if !self.gestures.contains_key(&cast.animation) {
                return Err(SkillError::UnknownGesture(cast.animation.clone()));
            }
            if !(cast.speed > 0.0 && cast.speed.is_finite()) || cast.impact.is_some_and(|i| !(0.0..=1.0).contains(&i)) {
                return Err(SkillError::SpeedOrImpact);
            }
            let numbers: Vec<&Ranked> = match &cast.area {
                Area::Circle { radius, distance, .. } => vec![radius, distance],
                Area::Arc { range, angle } => vec![range, angle],
                Area::Line { length, width } => vec![length, width],
                Area::Caster => vec![],
            };
            if !numbers.iter().chain([&&cast.cooldown, &&cast.cost.mana, &&cast.cost.stamina]).all(|r| r.is_finite()) {
                return Err(SkillError::CastNumber);
            }
            if let Some(p) = &cast.projectile {
                let mut values = vec![&p.speed];
                if let Pierce::Count(n) = &p.pierce {
                    values.push(n);
                }
                if let Some(b) = &p.bounce {
                    values.extend([&b.count, &b.range]);
                }
                if !values.iter().all(|r| r.is_finite()) || p.speed.at(1) <= 0.0 {
                    return Err(SkillError::ProjectileNumbers);
                }
                if matches!(cast.area, Area::Arc { .. } | Area::Caster) {
                    return Err(SkillError::ProjectileArea);
                }
            }
            if let Some(m) = &cast.movement {
                if m.grapple.as_ref().is_some_and(|g| !(g.range.is_finite() && g.range.at(1) > 0.0 && g.speed.is_finite() && g.speed.at(1) > 0.0 && g.stop.is_finite())) {
                    return Err(SkillError::Grapple);
                }
                if m.hover.as_ref().is_some_and(|h| !(h.height.is_finite() && h.duration.is_finite() && h.duration.at(1) > 0.0)) {
                    return Err(SkillError::Hover);
                }
                if m.teleport.as_ref().is_some_and(|t| ![&t.behind, &t.above, &t.side, &t.delay].iter().all(|r| r.is_finite()) || t.delay.at(1) < 0.0) {
                    return Err(SkillError::Teleport);
                }
                if (m.teleport.is_some() || m.grapple.as_ref().is_some_and(|g| g.to == GrappleTo::Target)) && matches!(cast.area, Area::Caster) {
                    return Err(SkillError::MovementArea);
                }
            }
            if cast.follow_up.as_ref().is_some_and(|f| !f.delay.is_finite() || f.delay.at(1) < 0.0) {
                return Err(SkillError::FollowUp);
            }
            if let Some(c) = &cast.charge
                && !(c.min >= 0.0 && c.max >= c.min && c.max <= 10.0 && c.power.iter().all(|p| p.is_finite() && *p > 0.0)) {
                return Err(SkillError::Charge);
            }
            if cast.uses_round && !cast.weapons.contains(&crate::weapons::WeaponKind::Crossbow) {
                return Err(SkillError::UsesRound);
            }
            if let Some(form) = skill.form.as_ref().filter(|f| !entities.contains(*f)) {
                return Err(SkillError::UnknownForm(form.clone()));
            }
            if !skill.form_attacks.is_empty() && (skill.form.is_none() || skill.form_attacks.len() != 2) {
                return Err(SkillError::FormAttacks);
            }
            if let Some(missing) = skill.form_attacks.iter().find(|a| !ids.contains(*a)) {
                return Err(SkillError::UnknownFormAttack(missing.clone()));
            }
            for summon in skill.summons() {
                if let Effect::Summon { species, count, duration, limit, .. } = summon {
                    if !entities.contains(species) {
                        return Err(SkillError::UnknownSummon(species.clone()));
                    }
                    if !(count.is_finite() && count.at(1) >= 1.0) || duration.as_ref().is_some_and(|d| !d.is_finite()) || limit.as_ref().is_some_and(|l| !l.is_finite()) {
                        return Err(SkillError::SummonNumbers(species.clone()));
                    }
                }
            }
            // Percer et briser se comptent en parts de l'armure, de 0 à 1
            let share = |r: &Ranked| r.is_finite() && (0..=3).all(|rank| (0.0..=1.0).contains(&r.at(rank)));
            let mut entries: Vec<&EffectEntry> = cast.effects.iter().chain(&cast.self_effects).chain(&cast.on_hit).chain(&cast.on_miss).chain(cast.on_kill.iter().flat_map(|a| &a.effects)).collect();
            let mut i = 0;
            while i < entries.len() {
                if let Effect::Buff { rider, on_death, .. } = &entries[i].effect {
                    entries.extend(rider.iter().chain(on_death).flat_map(|a| &a.effects));
                }
                i += 1;
            }
            for entry in entries {
                match &entry.effect {
                    Effect::Guard(guard) => {
                        let numbers = [Some(&guard.duration), Some(&guard.block), guard.hits.as_ref(), guard.reflect.as_ref(), guard.counter.as_ref().map(|c| &c.around.radius)];
                        if !numbers.iter().flatten().all(|r| r.is_finite()) || guard.arc.is_some_and(|a| !(a > 0.0 && a <= 360.0)) || guard.miss_cooldown.is_some_and(|m| !(m.is_finite() && m > 0.0)) {
                            return Err(SkillError::GuardNumbers);
                        }
                        if let Some(kind) = guard.kinds.iter().find(|k| !matches!(k.as_str(), "physical" | "magical")) {
                            return Err(SkillError::GuardKind(kind.clone()));
                        }
                    }
                    Effect::Immune { from, duration } if from.is_empty() || !duration.is_finite() => {
                        return Err(SkillError::Immune);
                    }
                    Effect::Damage { pierce, .. } if !share(pierce) => return Err(SkillError::Pierce),
                    Effect::Shred { amount, duration, max } if !share(amount) || !duration.is_finite() || max.as_ref().is_some_and(|m| !share(m)) => {
                        return Err(SkillError::Shred);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Empreinte des fichiers lus : deux catalogues de même empreinte jugent de la même façon
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SkillDef> {
        self.skills.iter()
    }

    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.index.get(id).map(|&i| &self.skills[i])
    }

    pub fn class(&self, id: &str) -> Option<&SkillBranch> {
        self.classes.iter().find(|c| c.id == id)
    }

    /// Le nœud de la roue qui donne `id`, s'il en est un
    pub fn granted_by(&self, id: &str) -> Option<&str> {
        self.granted_by.get(id).map(String::as_str)
    }

    /// Rang de la compétence `id` chez ce personnage : celui de son nœud pour une compétence
    /// accordée (0 : pas apprise)
    pub fn rank(&self, stats: &CharacterStats, id: &str) -> u8 {
        rank(stats, self.granted_by(id).unwrap_or(id))
    }

    /// Les compétences que ce personnage peut lancer, dans l'ordre du catalogue
    pub fn castable<'a>(&'a self, stats: &'a CharacterStats) -> impl Iterator<Item = &'a SkillDef> + 'a {
        self.skills.iter().filter(move |s| s.is_castable() && self.rank(stats, &s.id) > 0)
    }

    /// La forme (compétence du Druide) dont `id` est l'une des deux attaques
    pub fn form_of_attack(&self, id: &str) -> Option<&SkillDef> {
        self.form_attack_of.get(id).and_then(|f| self.get(f))
    }

    /// La Fusion (compétence du Sorcier élémentaire qui mêle les sorts), s'il en est une
    pub fn fusion(&self) -> Option<&SkillDef> {
        self.skills.iter().find(|s| s.fusion.is_some())
    }

    /// La Fusion qui lance le mélange `id` (un mélange ne se lance pas directement)
    pub fn fusion_of_mix(&self, id: &str) -> Option<&SkillDef> {
        self.mix_of.get(id).and_then(|f| self.get(f))
    }

    /// L'élément (nœud de la roue) du sort `id`, s'il entre dans la Fusion
    pub fn element_of(&self, id: &str) -> Option<&str> {
        let node = self.granted_by(id)?;
        let rules = self.fusion()?.fusion.as_ref()?;
        rules.mixes.iter().any(|m| m.elements.iter().any(|e| e == node)).then_some(node)
    }

    /// Ce que la Fusion fait des sorts `first` puis `last` : leur mélange, ou `last` amplifié s'ils
    /// sont du même élément
    pub fn fuse(&self, first: &str, last: &str) -> Option<Fused> {
        let (a, b) = (self.element_of(first)?, self.element_of(last)?);
        if a == b {
            return Some(Fused::Amplified { spell: last.to_string(), element: a.to_string() });
        }
        let rules = self.fusion()?.fusion.as_ref()?;
        rules.mixes.iter().find(|m| (m.elements[0] == a && m.elements[1] == b) || (m.elements[0] == b && m.elements[1] == a)).map(|m| Fused::Mix(m.skill.clone()))
    }

    /// Icône de l'amplification de l'élément `element`
    pub fn amplify_icon(&self, element: &str) -> Option<&Path> {
        self.amplify_icons.get(element).map(PathBuf::as_path)
    }

    /// Durée et impact du geste `name`
    pub fn gesture(&self, name: &str) -> Option<GestureTiming> {
        self.gestures.get(name).copied()
    }

    /// Secondes entre le lancement de `cast` et sa frappe
    pub fn impact_delay(&self, cast: &Cast) -> f32 {
        let timing = self.gesture(&cast.animation).unwrap_or(GestureTiming { duration: 0.5, impact: 0.5 });
        timing.duration / cast.speed * cast.impact.unwrap_or(timing.impact)
    }

    /// La roue, telle que l'écran des compétences la dessine
    pub fn wheel(&self) -> SkillWheel {
        let nodes = self
            .skills
            .iter()
            .filter_map(|s| {
                let w = s.wheel.as_ref()?;
                Some(SkillNode {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    description: s.description.clone(),
                    branch: s.class.clone(),
                    tier: w.tier,
                    ring: w.ring,
                    slot: w.slot,
                    requires: w.requires.clone(),
                    max_rank: w.max_rank,
                    planner: w.planner,
                    grants: w.grants.clone(),
                })
            })
            .collect();
        SkillWheel { branches: self.classes.clone(), nodes }
    }

    /// Pourquoi ce personnage ne peut pas lancer `id` maintenant (`None` : il peut)
    pub fn cast_check(&self, stats: &CharacterStats, id: &str, cooldown_left: f32) -> Option<CastError> {
        let Some(skill) = self.get(id) else { return Some(CastError::Unknown) };
        let Some(cast) = skill.cast.as_ref() else { return Some(CastError::NotPlayable) };
        let rank = self.rank(stats, id);
        if rank == 0 {
            return Some(CastError::NotLearned);
        }
        if cooldown_left > 0.0 {
            return Some(CastError::Cooldown);
        }
        if stats.hp <= 0.0 {
            return Some(CastError::Dead);
        }
        if stats.mana < cast.cost.mana.at(rank) {
            return Some(CastError::NoMana);
        }
        if stats.stamina < cast.cost.stamina.at(rank) {
            return Some(CastError::NoStamina);
        }
        None
    }

    /// Retire de ce personnage les compétences inconnues du catalogue (d'anciennes sauvegardes) et
    /// lui rend les points qu'il y avait mis. Renvoie les identifiants retirés.
    pub fn forget_unknown(&self, stats: &mut CharacterStats) -> Vec<String> {
        let wheel_ids: HashSet<&str> = self.skills.iter().filter(|s| s.wheel.is_some()).map(|s| s.id.as_str()).collect();
        let unknown: Vec<String> = stats.skills.keys().filter(|id| !wheel_ids.contains(id.as_str())).cloned().collect();
        for id in &unknown {
            if let Some(rank) = stats.skills.remove(id) {
                stats.skill_points += rank as u32;
            }
        }
        unknown
    }
}

/// Pourquoi une compétence ne se lance pas
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.message())]
pub enum CastError {
    Unknown,
    NotPlayable,
    NotLearned,
    Cooldown,
    NoMana,
    NoStamina,
    Dead,
    /// Changé en bête : seules les attaques de la forme se lancent
    InForm,
    /// Une attaque de forme, hors de sa forme
    NotInForm,
    Stunned,
    /// Un mélange ne se lance que par la Fusion
    FusionOnly,
    /// La Fusion attend deux sorts élémentaires
    FusionEmpty,
    /// Ce mélange n'est pas encore appris
    FusionLocked,
    /// Un grappin sans bloc visé à portée
    NothingToGrab,
    /// Pas la bonne arme en main (`Cast::weapons`)
    NoWeapon,
    /// L'arbalète recharge (`Cast::uses_round`)
    Reloading,
    /// Relâchée trop tôt (`Cast::charge`)
    NotCharged,
}

impl CastError {
    pub fn message(self) -> &'static str {
        match self {
            CastError::Unknown => "compétence inconnue",
            CastError::NotPlayable => "pas encore jouable",
            CastError::NotLearned => "pas apprise",
            CastError::Cooldown => "pas encore rechargée",
            CastError::NoMana => "pas assez de mana",
            CastError::NoStamina => "pas assez d'endurance",
            CastError::Dead => "mort",
            CastError::InForm => "impossible sous cette forme",
            CastError::NotInForm => "seulement sous sa forme",
            CastError::Stunned => "étourdi",
            CastError::FusionOnly => "se lance par la Fusion",
            CastError::FusionEmpty => "lance d'abord deux sorts élémentaires",
            CastError::FusionLocked => "mélange pas encore appris",
            CastError::NothingToGrab => "rien à agripper",
            CastError::NoWeapon => "pas la bonne arme en main",
            CastError::Reloading => "l'arbalète recharge",
            CastError::NotCharged => "relâchée trop tôt",
        }
    }
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Hachage FNV-1a, stable d'une machine à l'autre (celui de la bibliothèque standard ne l'est pas)
#[derive(Debug)]
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        // Sépare les morceaux : « ab » + « c » ne vaut pas « a » + « bc »
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

// ------------------------------------------------------------------------------------------------
// Zones

/// Une zone posée dans le monde, au moment de la frappe. Les longueurs sont en unités du monde.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlacedArea {
    Circle { center: Vec3, radius: f32 },
    /// Secteur : sommet, direction (unitaire, dans le plan du sol), portée, demi-ouverture en radians
    Arc { apex: Vec3, forward: Vec3, range: f32, half_angle: f32 },
    /// Rectangle : départ, direction (unitaire, dans le plan du sol), longueur, demi-largeur
    Line { start: Vec3, forward: Vec3, length: f32, half_width: f32 },
    /// Le lanceur seul
    Caster,
}

/// Hauteur sous laquelle une zone attrape une cible au-dessus ou au-dessous du sol du lanceur, en
/// blocs
pub const AREA_HEIGHT_BLOCKS: f32 = 3.0;

impl Area {
    /// La zone posée aux pieds `feet` du lanceur, qui vise `aim` (direction quelconque) et
    /// `aimed` (point au sol, s'il en vise un). `block` : taille d'un bloc en unités du monde.
    pub fn place(&self, rank: u8, feet: Vec3, aim: Vec3, aimed: Option<Vec3>, block: f32) -> PlacedArea {
        self.place_scaled(rank, feet, aim, aimed, block, 1.0, 1.0)
    }

    /// Comme `place`, la zone agrandie de `size` (amplification) et sa portée allongée de `reach`
    /// (renforcement de portée)
    pub fn place_scaled(&self, rank: u8, feet: Vec3, aim: Vec3, aimed: Option<Vec3>, block: f32, size: f32, reach: f32) -> PlacedArea {
        let up = feet.normalize_or_zero();
        let flat = aim - up * aim.dot(up);
        let forward = if flat.length_squared() > 1e-8 { flat.normalize() } else { crate::tangent_frame(up).1 };
        match self {
            Area::Circle { radius, center, distance } => {
                let center = match (center, aimed) {
                    (Center::Aimed, Some(point)) => {
                        let range = distance.at(rank) * reach * block;
                        let offset = point - feet;
                        if offset.length() > range {
                            feet + offset.normalize() * range
                        } else {
                            point
                        }
                    }
                    (Center::Aimed, None) => feet + forward * distance.at(rank) * reach * block,
                    (Center::Caster, _) => feet,
                };
                PlacedArea::Circle { center, radius: radius.at(rank) * size * block }
            }
            Area::Arc { range, angle } => PlacedArea::Arc { apex: feet, forward, range: range.at(rank) * size * reach * block, half_angle: (angle.at(rank) * 0.5).to_radians() },
            Area::Line { length, width } => {
                PlacedArea::Line { start: feet, forward, length: length.at(rank) * size * reach * block, half_width: width.at(rank) * 0.5 * size * block }
            }
            Area::Caster => PlacedArea::Caster,
        }
    }

    /// Taille de la zone en blocs (rayon, portée ou longueur), à laquelle les effets visuels en
    /// `unit: area` se mesurent
    pub fn size_blocks(&self, rank: u8) -> f32 {
        match self {
            Area::Circle { radius, .. } => radius.at(rank),
            Area::Arc { range, .. } => range.at(rank),
            Area::Line { length, .. } => length.at(rank),
            Area::Caster => 1.0,
        }
    }
}

impl PlacedArea {
    /// La cible aux pieds `point` est dans la zone. `block` : taille d'un bloc.
    pub fn contains(&self, point: Vec3, block: f32) -> bool {
        let height = AREA_HEIGHT_BLOCKS * block;
        let split = |origin: Vec3| {
            let up = origin.normalize_or_zero();
            let d = point - origin;
            let vertical = d.dot(up);
            (d - up * vertical, vertical)
        };
        match *self {
            PlacedArea::Circle { center, radius } => {
                let (flat, vertical) = split(center);
                flat.length() <= radius && vertical.abs() <= height.max(radius * 0.5)
            }
            PlacedArea::Arc { apex, forward, range, half_angle } => {
                let (flat, vertical) = split(apex);
                let distance = flat.length();
                if distance > range || vertical.abs() > height {
                    return false;
                }
                // Tout contre le lanceur, elle est touchée quel que soit l'angle
                distance < 0.75 * block || flat.dot(forward) >= distance * half_angle.cos()
            }
            PlacedArea::Line { start, forward, length, half_width } => {
                let (flat, vertical) = split(start);
                let along = flat.dot(forward);
                (0.0..=length).contains(&along) && (flat - forward * along).length() <= half_width && vertical.abs() <= height
            }
            PlacedArea::Caster => false,
        }
    }

    /// Centre de la zone, où les visuels `area` se jouent
    pub fn center(&self, feet: Vec3) -> Vec3 {
        match *self {
            PlacedArea::Circle { center, .. } => center,
            PlacedArea::Arc { apex, forward, range, .. } => apex + forward * range * 0.5,
            PlacedArea::Line { start, forward, length, .. } => start + forward * length * 0.5,
            PlacedArea::Caster => feet,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// La roue

/// Une compétence de la roue
#[derive(Debug, Clone)]
pub struct SkillNode {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Identifiant de la classe qui la porte
    pub branch: String,
    /// Palier : 1 et 2 pour les niveaux 1 et 2, 3 pour les fusions, 4 pour l'ultime
    pub tier: u8,
    pub ring: Option<f32>,
    pub slot: f32,
    pub requires: Vec<String>,
    pub max_rank: u8,
    pub planner: Option<u32>,
    /// Compétences que ce nœud donne
    pub grants: Vec<String>,
}

impl SkillNode {
    /// Anneau où la dessiner
    pub fn ring(&self) -> f32 {
        self.ring.unwrap_or(self.tier as f32)
    }

    /// Nom du palier, pour sa fiche
    pub fn tier_label(&self) -> &'static str {
        match self.tier {
            1 => "Niveau 1",
            2 => "Niveau 2",
            3 => "Fusion",
            _ => "Ultime",
        }
    }

    pub fn is_ultimate(&self) -> bool {
        self.tier >= 4
    }
}

/// La roue entière
#[derive(Debug, Clone)]
pub struct SkillWheel {
    pub branches: Vec<SkillBranch>,
    pub nodes: Vec<SkillNode>,
}

/// Pourquoi une compétence ne peut pas être apprise
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.message())]
pub enum LearnError {
    Unknown,
    NoPoint,
    MaxRank,
    MissingPrerequisite,
}

impl LearnError {
    pub fn message(self) -> &'static str {
        match self {
            LearnError::Unknown => "compétence inconnue",
            LearnError::NoPoint => "aucun point à dépenser",
            LearnError::MaxRank => "déjà au rang maximal",
            LearnError::MissingPrerequisite => "prérequis manquant",
        }
    }
}

impl SkillWheel {
    /// La roue du jeu, lue dans `assets/skills/`
    pub fn standard() -> Self {
        skills().wheel()
    }

    pub fn node(&self, id: &str) -> Option<&SkillNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn branch(&self, id: &str) -> Option<&SkillBranch> {
        self.branches.iter().find(|b| b.id == id)
    }

    /// Anneau le plus éloigné du centre
    pub fn rings(&self) -> f32 {
        self.nodes.iter().map(SkillNode::ring).fold(1.0, f32::max)
    }

    /// Ce qui empêche d'apprendre `id` (ou de le renforcer), `None` si c'est possible
    pub fn check(&self, stats: &CharacterStats, id: &str) -> Option<LearnError> {
        let Some(node) = self.node(id) else { return Some(LearnError::Unknown) };
        if rank(stats, id) >= node.max_rank {
            return Some(LearnError::MaxRank);
        }
        if node.requires.iter().any(|r| rank(stats, r) == 0) {
            return Some(LearnError::MissingPrerequisite);
        }
        if stats.skill_points == 0 {
            return Some(LearnError::NoPoint);
        }
        None
    }

    /// Dépense un point pour apprendre `id`, ou le renforcer d'un rang : renvoie le nouveau rang
    pub fn learn(&self, stats: &mut CharacterStats, id: &str) -> Result<u8, LearnError> {
        if let Some(error) = self.check(stats, id) {
            return Err(error);
        }
        stats.skill_points -= 1;
        let rank = stats.skills.entry(id.to_string()).or_insert(0);
        *rank += 1;
        Ok(*rank)
    }
}

/// Rang du nœud `id` chez ce personnage (0 : pas appris)
pub fn rank(stats: &CharacterStats, id: &str) -> u8 {
    stats.skills.get(id).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerClass;

    fn catalog() -> SkillCatalog {
        let (catalog, errors) = SkillCatalog::load_dirs(&asset_dir(SKILLS_DIR).unwrap(), &asset_dir(GESTURES_DIR).unwrap());
        assert!(errors.is_empty(), "{errors:#?}");
        catalog
    }

    #[test]
    fn test_skill_files_are_valid() {
        let catalog = catalog();
        let wheel = catalog.wheel();
        // Les 180 compétences des 15 classes, douze chacune
        assert_eq!(wheel.branches.len(), 15);
        assert_eq!(wheel.nodes.len(), 180);
        for branch in &wheel.branches {
            let nodes: Vec<_> = wheel.nodes.iter().filter(|n| n.branch == branch.id).collect();
            assert_eq!(nodes.len(), 12, "{}", branch.name);
            assert_eq!(nodes.iter().filter(|n| n.is_ultimate()).count(), 1, "{} : un ultime", branch.name);
            assert!(["magic", "ranged", "melee"].contains(&branch.family.as_str()), "{}", branch.family);
            assert_eq!(branch.mix.iter().map(|&p| p as u32).sum::<u32>(), 100, "{}", branch.name);
        }
        // Une compétence ne demande que des compétences plus proches du centre
        for node in &wheel.nodes {
            for r in &node.requires {
                assert!(wheel.node(r).unwrap().ring() < node.ring(), "{} demande {}", node.id, r);
            }
        }
        assert_eq!(wheel.rings(), 4.0);
        let colors: HashSet<_> = wheel.branches.iter().map(|b| b.color).collect();
        assert_eq!(colors.len(), wheel.branches.len());
        // Les quatre éléments du Sorcier donnent chacun deux compétences
        assert_eq!(catalog.granted_by("igneousOrb"), Some("fire"));
        // ... et chacune des huit formes du Druide ses deux attaques
        assert_eq!(catalog.granted_by.len(), 8 + 16);
        assert_eq!(catalog.form_of_attack("wolfBite").map(|f| f.id.as_str()), Some("forestWolf"));
        // Identifiants en anglais : ni accent, ni nom de fichier hors de la convention
        for skill in catalog.iter() {
            assert!(skill.id.chars().all(|c| c.is_ascii_alphanumeric()) && skill.id.chars().next().unwrap().is_ascii_lowercase(), "{}", skill.id);
        }
        assert_ne!(catalog.fingerprint(), 0);
        // Toutes se jouent, sauf les nœuds qui donnent d'autres sorts (éléments)
        for skill in catalog.iter() {
            let container = skill.wheel.as_ref().is_some_and(|w| !w.grants.is_empty());
            assert!(container || skill.is_castable(), "{}/{} n'a pas de règles", skill.class, skill.id);
        }
    }

    #[test]
    fn test_fusion_mixes_two_elements_and_amplifies_one() {
        let catalog = catalog();
        assert_eq!(catalog.element_of("igneousOrb"), Some("fire"));
        assert_eq!(catalog.element_of("judgment"), None);
        assert_eq!(catalog.fuse("foamingWhip", "igneousOrb"), Some(Fused::Mix("scaldingTyphoon".into())));
        assert_eq!(catalog.fuse("seismicShard", "galeBlades"), Some(Fused::Mix("shrapnelStorm".into())));
        assert_eq!(catalog.fuse("igneousOrb", "volcanicRune"), Some(Fused::Amplified { spell: "volcanicRune".into(), element: "fire".into() }));
        // Les six paires d'éléments ont leur mélange, chaque élément son icône d'amplification
        let elements = ["fire", "water", "earth", "air"];
        let spell = |e: &str| catalog.iter().find(|s| catalog.granted_by(&s.id) == Some(e)).unwrap().id.clone();
        for a in elements {
            assert!(catalog.amplify_icon(a).is_some(), "{a}");
            for b in elements.iter().filter(|b| **b != a) {
                assert!(matches!(catalog.fuse(&spell(a), &spell(b)), Some(Fused::Mix(_))), "{a} + {b}");
            }
        }
        assert!(catalog.fusion_of_mix("magmaWall").is_some());
        assert!(catalog.fusion().unwrap().is_castable());
    }

    #[test]
    fn test_visuals_and_gestures_exist() {
        let catalog = catalog();
        let effects = asset_dir("assets/effects").unwrap();
        let amplify = catalog.fusion().and_then(|f| f.fusion.as_ref()).map(|f| f.amplify.visuals.clone()).unwrap_or_default();
        for visual in &amplify {
            assert!(effects.join(format!("{}.json", visual.effect)).is_file(), "amplification : effet inconnu {}", visual.effect);
        }
        for skill in catalog.iter() {
            for visual in &skill.visuals {
                assert!(effects.join(format!("{}.json", visual.effect)).is_file(), "{} : effet inconnu {}", skill.id, visual.effect);
            }
        }
        assert_eq!(catalog.gesture("groundSlam").map(|g| g.duration), Some(0.9));
    }

    #[test]
    fn test_points_come_from_levels_and_buy_ranks() {
        let wheel = catalog().wheel();
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Err(LearnError::NoPoint));

        // Niveau 4 : trois points
        hero.add_experience(600);
        assert_eq!(hero.skill_points, 3);
        assert_eq!(wheel.learn(&mut hero, "circularSweep"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "circularSweep"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Ok(2));
        assert_eq!(hero.skill_points, 0);
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Err(LearnError::NoPoint));
        assert_eq!(wheel.learn(&mut hero, "unknown"), Err(LearnError::Unknown));

        hero.skill_points = 10;
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Ok(3));
        assert_eq!(wheel.learn(&mut hero, "heavyStrike"), Err(LearnError::MaxRank));
        // Une fusion demande les deux formes du niveau 2 qu'elle unit
        assert_eq!(wheel.learn(&mut hero, "crushingTornado"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "frontalRush"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "divingStrike"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "crushingTornado"), Ok(1));
        // Les compétences des autres classes s'apprennent aussi (#171)
        assert_eq!(wheel.learn(&mut hero, "righteousStrike"), Ok(1));
        // Un sort accordé n'est pas un nœud : il prend le rang de son élément
        assert_eq!(wheel.learn(&mut hero, "igneousOrb"), Err(LearnError::Unknown));
        let catalog = catalog();
        assert_eq!(catalog.rank(&hero, "igneousOrb"), 0);
        wheel.learn(&mut hero, "fire").unwrap();
        assert_eq!((catalog.rank(&hero, "igneousOrb"), catalog.rank(&hero, "volcanicRune")), (1, 1));
    }

    #[test]
    fn test_old_saves_lose_unknown_skills_and_get_their_points_back() {
        let catalog = catalog();
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        hero.skills.insert("frappeLourde".into(), 2);
        hero.skills.insert("heavyStrike".into(), 1);
        assert_eq!(catalog.forget_unknown(&mut hero), vec!["frappeLourde".to_string()]);
        assert_eq!(hero.skill_points, 2);
        assert_eq!(hero.skills.len(), 1);
    }

    #[test]
    fn test_cast_rules() {
        let catalog = catalog();
        let mut hero = CharacterStats::new(PlayerClass::WarPriest);
        assert_eq!(catalog.cast_check(&hero, "judgment", 0.0), Some(CastError::NotLearned));
        hero.skills.insert("judgment".into(), 1);
        assert_eq!(catalog.cast_check(&hero, "judgment", 0.0), None);
        assert_eq!(catalog.cast_check(&hero, "judgment", 1.0), Some(CastError::Cooldown));
        hero.mana = 0.0;
        assert_eq!(catalog.cast_check(&hero, "judgment", 0.0), Some(CastError::NoMana));
        let cast = catalog.get("judgment").unwrap().cast.as_ref().unwrap();
        // Le maillet frappe le sol à 35 % d'un geste de 0,9 s
        assert!((catalog.impact_delay(cast) - 0.315).abs() < 1e-4);
    }

    #[test]
    fn test_ranked_values() {
        assert_eq!(Ranked::One(2.0).at(3), 2.0);
        let r = Ranked::PerRank(vec![3.0, 3.5, 4.0]);
        assert_eq!((r.at(0), r.at(1), r.at(2), r.at(3), r.at(5)), (3.0, 3.0, 3.5, 4.0, 4.0));
    }

    #[test]
    fn test_areas() {
        let block = 1.0;
        let feet = Vec3::new(0.0, 100.0, 0.0);
        let aim = Vec3::new(0.0, -0.3, -1.0);
        let arc = Area::Arc { range: Ranked::One(3.0), angle: Ranked::One(90.0) }.place(1, feet, aim, None, block);
        assert!(arc.contains(feet + Vec3::new(0.0, 0.0, -2.0), block), "devant");
        assert!(!arc.contains(feet + Vec3::new(0.0, 0.0, 2.0), block), "derrière");
        assert!(!arc.contains(feet + Vec3::new(0.0, 0.0, -4.0), block), "trop loin");
        assert!(!arc.contains(feet + Vec3::new(0.0, 5.0, -1.0), block), "trop haut");

        let line = Area::Line { length: Ranked::One(6.0), width: Ranked::One(2.0) }.place(1, feet, aim, None, block);
        assert!(line.contains(feet + Vec3::new(0.5, 0.0, -5.0), block));
        assert!(!line.contains(feet + Vec3::new(2.0, 0.0, -5.0), block));

        let circle = Area::Circle { radius: Ranked::PerRank(vec![3.0, 4.0]), center: Center::Caster, distance: Ranked::One(0.0) };
        assert!(!circle.place(1, feet, aim, None, block).contains(feet + Vec3::new(3.5, 0.0, 0.0), block));
        assert!(circle.place(2, feet, aim, None, block).contains(feet + Vec3::new(3.5, 0.0, 0.0), block));

        // Un point visé trop loin est ramené à la portée
        let aimed = Area::Circle { radius: Ranked::One(2.0), center: Center::Aimed, distance: Ranked::One(5.0) }.place(1, feet, aim, Some(feet + Vec3::new(0.0, 0.0, -20.0)), block);
        assert!(matches!(aimed, PlacedArea::Circle { center, .. } if (center - (feet + Vec3::new(0.0, 0.0, -5.0))).length() < 1e-3));
    }
}

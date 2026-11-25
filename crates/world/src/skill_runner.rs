//! Exécution des compétences : le même code juge et applique les compétences sur le serveur et dans
//! une partie solo, sur les créatures et les joueurs.
//!
//! Un lancement réussi (`SkillRunner::cast`) paie son coût, arme sa recharge et attend l'instant
//! d'impact de son geste ; à l'impact (`SkillRunner::tick`), la zone est posée devant le lanceur
//! et chaque cible subit les effets de la compétence. Les effets qui durent (soins et dégâts par
//! seconde, renforcements) sont suivis ici ; ralentis et étourdissements, par les créatures.

use std::collections::HashMap;

use glam::Vec3;
use herocraft_common::skills::{Around, BuffStat, CastError, Center, Copies, CounterAt, Effect, EffectEntry, Facing, Fused, GrappleTo, Guard, PlacedArea, SkillCatalog, SkillMove, Status, Targets, SKILL_BASE_DAMAGE};
use herocraft_common::{CharacterStats, CreatureId, DamageKind, PlayerId};

use crate::arsenal::{Arsenal, Fired, Special};
use crate::CreatureWorld;

/// Intervalle entre deux coups d'un effet par seconde, en secondes
const OVER_TIME_STEP: f32 = 0.5;

/// Les joueurs, tels que les compétences les voient
pub trait SkillPlayers {
    /// Identifiants des joueurs en jeu
    fn ids(&self) -> Vec<PlayerId>;
    /// Pieds du joueur, s'il est placé
    fn feet(&self, id: PlayerId) -> Option<Vec3>;
    fn stats_mut(&mut self, id: PlayerId) -> Option<&mut CharacterStats>;
}

/// Un seul joueur : la partie solo
pub struct SoloPlayer<'a> {
    pub id: PlayerId,
    pub feet: Vec3,
    pub stats: &'a mut CharacterStats,
}

impl SkillPlayers for SoloPlayer<'_> {
    fn ids(&self) -> Vec<PlayerId> {
        vec![self.id]
    }

    fn feet(&self, id: PlayerId) -> Option<Vec3> {
        (id == self.id).then_some(self.feet)
    }

    fn stats_mut(&mut self, id: PlayerId) -> Option<&mut CharacterStats> {
        (id == self.id).then_some(&mut *self.stats)
    }
}

/// Qui subit un effet qui dure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillTarget {
    Creature(CreatureId),
    Player(PlayerId),
}

/// Ce qu'a fait une compétence en frappant, pour l'annoncer aux joueurs
#[derive(Debug, Clone, PartialEq)]
pub struct SkillImpact {
    pub caster: PlayerId,
    pub skill: String,
    pub rank: u8,
    /// Centre de la zone
    pub center: Vec3,
    pub creatures: Vec<CreatureId>,
    pub players: Vec<PlayerId>,
    /// Entités que la frappe a fait apparaître (`summon`)
    pub summoned: Vec<CreatureId>,
    /// La touche d'un projectile en vol (seuls ses visuels « hit » jouent)
    pub contact: bool,
    /// La riposte d'une garde qui vient de parer un coup : le client n'en joue que les visuels
    /// de parade (`Moment::Parry`)
    pub parried: bool,
}

/// Ce qu'une garde fait d'un coup reçu (`SkillRunner::parry`)
#[derive(Debug, Clone, PartialEq)]
pub struct Parry {
    /// Dégâts qui passent encore
    pub damage: f32,
    /// Le coup a été paré : ni ralenti, ni étourdissement, ni recul
    pub parried: bool,
    /// La riposte, à annoncer
    pub impact: Option<SkillImpact>,
}

/// Ce que `tick` renvoie : les frappes du moment et les joueurs dont les jauges ont changé
#[derive(Debug, Default)]
pub struct SkillTick {
    pub impacts: Vec<SkillImpact>,
    /// Joueurs à qui renvoyer leurs statistiques
    pub changed: Vec<PlayerId>,
    /// Joueurs qu'un effet sur la durée vient de tuer
    pub died: Vec<PlayerId>,
    /// Déplacements imposés aux lanceurs (téléportation, traction vers une cible) : à qui, lequel
    pub moves: Vec<(PlayerId, SkillMove)>,
    /// Gardes fermées sans avoir rien paré : joueur, compétence, secondes de recharge en plus
    pub missed: Vec<(PlayerId, String, f32)>,
}

/// Ce qu'une frappe laisse pour plus tard, sur les ennemis qu'elle a trouvés
#[derive(Debug, Clone)]
enum Later {
    /// La seconde frappe (`follow_up`)
    FollowUp,
    /// La téléportation du lanceur près de la première cible (`movement.teleport`)
    Teleport,
}

#[derive(Debug, Clone)]
struct Delayed {
    pending: Pending,
    targets: Vec<CreatureId>,
    left: f32,
    what: Later,
}

/// Hauteur visée sur le corps d'une bête quand on se tourne vers elle, en blocs
const LOOK_AT_HEIGHT: f32 = 0.6;
/// Hauteur des yeux du héros au-dessus de ses pieds, en blocs
const EYE_HEIGHT: f32 = 1.6;

#[derive(Debug, Clone)]
struct Pending {
    caster: PlayerId,
    skill: String,
    rank: u8,
    aim: Vec3,
    aimed: Option<Vec3>,
    left: f32,
    /// Lancée depuis l'invisibilité : ses dégâts sont tous critiques
    crit: bool,
    /// Amplifiée par la Fusion : facteur de ses dégâts, soins, boucliers et reculs, et de sa zone
    power: f32,
    size: f32,
    /// Type de ses dégâts (`Cast::damage_kind`)
    kind: DamageKind,
}

/// Un lancement accepté : la compétence vraiment lancée (le mélange, pour la Fusion), son rang, et
/// si elle est amplifiée
#[derive(Debug, Clone, PartialEq)]
pub struct Casting {
    pub skill: String,
    pub rank: u8,
    pub amplified: bool,
}

#[derive(Debug, Clone)]
struct OverTime {
    caster: PlayerId,
    target: SkillTarget,
    /// Points par seconde : négatifs pour des dégâts, positifs pour des soins
    per_second: f32,
    left: f32,
    /// Temps avant le prochain coup
    next: f32,
    /// Facteur des dégâts tant que la bête bouge, et où elle était au coup précédent
    moving: f32,
    last: Option<Vec3>,
    /// Type de ses dégâts
    kind: DamageKind,
}

#[derive(Debug, Clone)]
struct Buff {
    player: PlayerId,
    stat: BuffStat,
    value: f32,
    left: f32,
    /// Dégâts en plus contre les créatures qui portent cette étiquette
    against: Option<(String, f32)>,
    /// La valeur suit la vie qui manque au joueur, et la part qui manque en ce moment (1 sinon)
    scaled: bool,
    scale: f32,
    /// Effets à la mort du joueur et à chacune de ses attaques, la compétence et son rang
    on_death: Option<Around>,
    rider: Option<Around>,
    skill: String,
    rank: u8,
    /// Pour la défense : le seul type de coups qu'elle réduit (aucun : tous)
    kind: Option<DamageKind>,
}

/// Une garde levée (`Effect::Guard`)
#[derive(Debug, Clone)]
struct Guarding {
    player: PlayerId,
    skill: String,
    rank: u8,
    guard: Guard,
    left: f32,
    /// Coups qu'elle pare encore (`None` : jusqu'à la fin de la fenêtre)
    hits: Option<u32>,
    /// Coups déjà parés
    parried: u32,
    /// Direction visée au lancement : celle de la garde quand on ne sait pas où regarde le joueur
    aim: Vec3,
}

/// Une immunité (`Effect::Immune`) : qui, à quoi, secondes restantes
#[derive(Debug, Clone)]
struct Immunity {
    player: PlayerId,
    from: Vec<Status>,
    left: f32,
}

/// Part de sa vie que garde un joueur sauvé par un effet à la mort (Abomination de l'Étherium)
const LAST_STAND_HEALTH: f32 = 0.1;

/// Un allié lié à un lanceur, qui encaisse une part de ses dégâts
#[derive(Debug, Clone)]
struct Link {
    caster: PlayerId,
    ally: PlayerId,
    share: f32,
    left: f32,
    /// Coups que le lien encaisse encore avant de se défaire (`None` : jusqu'à sa durée)
    hits: Option<u32>,
}

/// Une zone qui reste posée (`Cast::zone`) : son lancement, où était le lanceur, si elle le suit,
/// secondes restantes et temps avant de rejouer ses effets
#[derive(Debug, Clone)]
struct Zone {
    pending: Pending,
    feet: Vec3,
    follow: bool,
    left: f32,
    next: f32,
}

/// Intervalle entre deux passages d'une zone sur ceux qui s'y trouvent, en secondes
const ZONE_STEP: f32 = 0.5;
/// Durée des états qu'une zone donne (ralenti, invisibilité, renforcement…) : ils cessent dès qu'on
/// en sort, la zone les redonne à chaque passage
const ZONE_HOLD: f32 = 2.0 * ZONE_STEP;

/// L'effet `entry` tel qu'une zone qui reste le joue : à sa frappe (`first`) ou à un passage. Les
/// effets d'un coup ne jouent qu'à la frappe ; ceux qui durent sont raccourcis au passage suivant.
fn zone_entry(entry: &EffectEntry, first: bool) -> Option<EffectEntry> {
    use herocraft_common::skills::Ranked;
    let hold = Ranked::One(ZONE_HOLD);
    let effect = match &entry.effect {
        Effect::OverTime { damage, heal, moving, .. } => Effect::OverTime { damage: damage.clone(), heal: heal.clone(), duration: Ranked::One(ZONE_STEP), moving: moving.clone() },
        Effect::Slow { strength, .. } => Effect::Slow { strength: strength.clone(), duration: hold },
        Effect::Blind { .. } => Effect::Blind { duration: hold },
        Effect::Invisible { .. } => Effect::Invisible { duration: hold },
        Effect::Taunt { .. } => Effect::Taunt { duration: hold },
        Effect::Mark { chance, .. } => Effect::Mark { chance: chance.clone(), duration: hold },
        Effect::Buff { stat, value, against, per_missing_health, on_death, rider, kind, .. } => {
            Effect::Buff { stat: *stat, value: value.clone(), duration: hold, against: against.clone(), per_missing_health: *per_missing_health, on_death: on_death.clone(), rider: rider.clone(), kind: *kind }
        }
        // Chaque passage ronge un peu plus l'armure, et le bris cesse peu après la sortie
        Effect::Shred { amount, max, .. } => Effect::Shred { amount: amount.clone(), duration: hold, max: max.clone() },
        // Une zone repousse ou aspire à chaque passage (Bastion de Lumière, Tempête des Âmes)
        Effect::Knockback { .. } | Effect::Pull { .. } => entry.effect.clone(),
        other if first => other.clone(),
        _ => return None,
    };
    Some(EffectEntry { effect, to: entry.to })
}

/// Une entité invoquée, suivie jusqu'à sa disparition
#[derive(Debug, Clone)]
struct Summoned {
    caster: PlayerId,
    species: String,
    id: CreatureId,
    /// Secondes avant qu'elle disparaisse (`None` : jusqu'à sa destruction)
    left: Option<f32>,
    /// Effets sur le lanceur à la fin de sa durée, et le rang de la compétence
    on_end: Vec<EffectEntry>,
    rank: u8,
    /// Elle avance : direction (unitaire), blocs restants, vitesse en blocs par seconde
    advance: Option<(Vec3, f32, f32)>,
}

/// Un tir de compétence en vol : son lancement, ses touches, le facteur de ses dégâts
#[derive(Debug, Clone)]
struct Flight {
    pending: Pending,
    hits: u32,
    damage_factor: f32,
}

/// Une défense contre les tirs (Mur vivant, Manteau de Zéphyr)
#[derive(Debug, Clone)]
struct ShotWard {
    player: PlayerId,
    /// Garde frontale : direction visée au lancement, cosinus du demi-arc
    front: Option<(Vec3, f32)>,
    deflect: Option<herocraft_common::ProjectileSize>,
    left: f32,
}

/// Hauteur d'où part le tir d'une compétence, en blocs au-dessus des pieds
const SHOT_HEIGHT: f32 = 1.4;

/// Distance devant un mur qui avance où il pousse les ennemis, en blocs ; demi-longueur du mur
const ADVANCE_REACH: f32 = 1.5;
const ADVANCE_HALF_WIDTH: f32 = 3.0;

/// Points de vie rendus chaque seconde par un renforcement de régénération de 1 (100 %)
const REGENERATION_HP: f32 = 5.0;
/// Points de mana et d'endurance rendus chaque seconde par un renforcement de récupération de 1
/// (100 % de la récupération au repos)
const REGENERATION_MANA: f32 = 2.0;
const REGENERATION_STAMINA: f32 = 10.0;

/// Écart entre deux entités invoquées ensemble, en blocs
const SUMMON_SPACING: f32 = 1.5;

/// Lancements en attente d'impact, recharges et effets qui durent
#[derive(Debug, Default)]
pub struct SkillRunner {
    pending: Vec<Pending>,
    over_time: Vec<OverTime>,
    buffs: Vec<Buff>,
    /// Entités invoquées, de la plus ancienne à la plus récente
    summoned: Vec<Summoned>,
    /// Joueurs changés en bête (formes du Druide) : qui, par quelle compétence, secondes restantes
    forms: Vec<(PlayerId, String, f32)>,
    /// Recharge restante de chaque compétence de chaque joueur, en secondes
    cooldowns: HashMap<(PlayerId, String), f32>,
    /// Bêtes marquées : par qui, laquelle, chance de critique en plus, secondes restantes
    marks: Vec<(PlayerId, CreatureId, f32, f32)>,
    /// État du tirage des coups critiques
    luck: u64,
    /// Les deux derniers sorts élémentaires de chaque joueur, que la Fusion mêle
    recent: HashMap<PlayerId, Vec<String>>,
    /// Alliés liés à un lanceur (Marque du Martyr, Intervention Vertueuse)
    links: Vec<Link>,
    /// Zones qui restent posées après leur frappe
    zones: Vec<Zone>,
    /// Tirs de compétence en vol, par repère
    flights: HashMap<u64, Flight>,
    next_flight: u64,
    /// Défenses des joueurs contre les tirs (Mur vivant, Manteau de Zéphyr)
    wards: Vec<ShotWard>,
    /// Direction visée par chaque joueur à son dernier lancement
    aims: HashMap<PlayerId, Vec3>,
    /// Secondes frappes et téléportations en attente
    delayed: Vec<Delayed>,
    /// Déplacements des lanceurs décidés par les frappes, rendus par `tick`
    moves: Vec<(PlayerId, SkillMove)>,
    /// Gardes levées
    guards: Vec<Guarding>,
    /// Immunités en cours
    immunities: Vec<Immunity>,
    /// L'arme de chaque joueur : cadence, chargeur, charge, munitions spéciales
    arsenal: Arsenal,
}

/// Un tir d'arc ou d'arbalète arrivé : la bête touchée (la plus proche dans son couloir), où il
/// s'arrête, et la munition spéciale qu'il portait
#[derive(Debug, Clone, PartialEq)]
pub struct ShotHit {
    pub target: Option<CreatureId>,
    pub to: Vec3,
    pub special: Option<String>,
}

/// Facteur des dégâts d'un coup critique
pub const CRITICAL_FACTOR: f32 = 2.0;

impl SkillRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Les armes des joueurs
    pub fn arsenal(&self) -> &Arsenal {
        &self.arsenal
    }

    pub fn arsenal_mut(&mut self) -> &mut Arsenal {
        &mut self.arsenal
    }

    /// Le joueur `player`, de voie `class`, tient `item` en main (`None` : mains nues)
    pub fn hold(&mut self, player: PlayerId, item: Option<herocraft_common::ItemId>, class: herocraft_common::PlayerClass) {
        self.arsenal.hold(player, item, class);
    }

    /// Avance les armes de `dt` secondes, à la vitesse d'attaque de chacun (appelé par `tick` ; le
    /// client en ligne l'appelle seul pour prévoir son chargeur)
    pub fn tick_weapons(&mut self, dt: f32) {
        let speeds: HashMap<PlayerId, f32> = self.buffs.iter().filter(|b| b.stat == BuffStat::AttackSpeed).fold(HashMap::new(), |mut m, b| {
            *m.entry(b.player).or_insert(1.0) += b.value * b.scale;
            m
        });
        self.arsenal.tick(dt, |p| speeds.get(&p).copied().unwrap_or(1.0));
    }

    /// Le tir `fired` du joueur `caster` part de ses pieds `feet` dans la direction `aim`, avec
    /// `damage` points (arme et munition, avant renforcements) : il frappe la première bête ennemie
    /// de son couloir, et y porte sa munition spéciale
    pub fn shoot(&mut self, caster: PlayerId, feet: Vec3, aim: Vec3, damage: f32, fired: Fired, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> ShotHit {
        use herocraft_common::skills::{Area, Ranked};
        use herocraft_common::weapons::{SHOT_RANGE_BLOCKS, SHOT_WIDTH_BLOCKS};
        let block = creatures.block_size();
        let reach = 1.0 + self.buff(caster, BuffStat::Range);
        let line = Area::Line { length: Ranked::One(SHOT_RANGE_BLOCKS), width: Ranked::One(SHOT_WIDTH_BLOCKS) };
        let area = line.place_scaled(1, feet, aim, None, block, 1.0, reach);
        let target = creatures
            .creatures()
            .iter()
            .filter(|c| c.is_alive() && !c.tamed && c.summoner.is_none() && area.contains(c.position, block))
            .min_by(|a, b| a.position.distance(feet).total_cmp(&b.position.distance(feet)))
            .map(|c| (c.id, c.position));
        let up = feet.normalize_or_zero();
        let flat = (aim - up * aim.dot(up)).normalize_or_zero();
        let to = target.map_or(feet + flat * SHOT_RANGE_BLOCKS * reach * block, |t| t.1);
        let special = fired.special.as_ref().map(|s| s.skill.clone());
        if let Some((id, at)) = target {
            let chance = self.critical_chance(caster, id);
            let critical = chance > 0.0 && self.roll() < chance;
            let factor = if critical { CRITICAL_FACTOR } else { 1.0 };
            creatures.wound(id, feet, caster, damage * fired.power * self.hit_factor(caster, id, creatures) * factor);
            if let Some(special) = &fired.special {
                self.apply_special(special, caster, &[id], creatures, players);
            }
            self.ride(caster, &[at], creatures, players);
        }
        ShotHit { target: target.map(|t| t.0), to, special }
    }

    /// Les effets d'une munition spéciale sur les bêtes `targets` : sur elles seules, ou sur tous les
    /// ennemis dans son rayon autour de chacune
    fn apply_special(&mut self, special: &Special, caster: PlayerId, targets: &[CreatureId], creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) {
        use herocraft_common::skills::Ranked;
        let points: Vec<Vec3> = targets.iter().filter_map(|&id| creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position)).collect();
        match special.radius {
            Some(radius) => {
                let around = Around { radius: Ranked::One(radius), effects: special.effects.clone() };
                self.burst(&around, caster, special.rank, &special.skill, &points, creatures, players);
            }
            None => {
                let feet = players.feet(caster).unwrap_or(Vec3::ZERO);
                let p = Pending { caster, skill: special.skill.clone(), rank: special.rank, aim: Vec3::ZERO, aimed: None, left: 0.0, crit: false, power: 1.0, size: 1.0, kind: DamageKind::Physical };
                let damage_factor = 1.0 + self.buff(caster, BuffStat::Damage);
                for entry in &special.effects {
                    for &id in targets {
                        self.apply_to_creature(entry, &p, id, feet, damage_factor, creatures);
                    }
                }
            }
        }
    }

    /// Recharge restante de `skill` pour le joueur `player`, en secondes
    pub fn cooldown(&self, player: PlayerId, skill: &str) -> f32 {
        self.cooldowns.get(&(player, skill.to_string())).copied().unwrap_or(0.0)
    }

    /// Le joueur `caster` lance `skill` : refusé s'il ne l'a pas apprise, si elle n'est pas
    /// rechargée ou s'il n'a pas de quoi la payer ; sinon le coût est payé, la recharge armée, et
    /// la frappe attend l'impact du geste. Renvoie le rang de la compétence.
    pub fn cast(&mut self, catalog: &SkillCatalog, caster: PlayerId, stats: &mut CharacterStats, skill: &str, aim: Vec3, aimed: Option<Vec3>) -> Result<u8, CastError> {
        self.cast_as(catalog, caster, stats, skill, aim, aimed).map(|c| c.rank)
    }

    /// Comme `cast`, et dit ce qui est vraiment lancé : la Fusion lance le mélange des deux derniers
    /// sorts élémentaires du joueur, ou le dernier amplifié s'ils sont du même élément
    pub fn cast_as(&mut self, catalog: &SkillCatalog, caster: PlayerId, stats: &mut CharacterStats, requested: &str, aim: Vec3, aimed: Option<Vec3>) -> Result<Casting, CastError> {
        if !aim.is_finite() || aimed.is_some_and(|p| !p.is_finite()) {
            return Err(CastError::Unknown);
        }
        let asked = catalog.get(requested).ok_or(CastError::Unknown)?;
        let fused = match asked.fusion {
            Some(_) => {
                if catalog.rank(stats, requested) == 0 {
                    return Err(CastError::NotLearned);
                }
                Some(self.fusion_preview(catalog, caster, stats)?)
            }
            None if catalog.fusion_of_mix(requested).is_some() => return Err(CastError::FusionOnly),
            None => None,
        };
        let skill = fused.as_ref().map_or(requested, |f| f.skill());
        let amplified = matches!(fused, Some(Fused::Amplified { .. }));
        // La recharge est celle de la compétence demandée : la Fusion a la sienne
        if let Some(error) = catalog.cast_check(stats, skill, self.cooldown(caster, requested)) {
            return Err(error);
        }
        if stats.is_stunned() {
            return Err(CastError::Stunned);
        }
        let def = catalog.get(skill).ok_or(CastError::Unknown)?;
        let cast = def.cast.as_ref().ok_or(CastError::NotPlayable)?;
        // Changé en bête, on n'a que les deux attaques de la forme ; et elles seulement sous elle
        let form = self.form(caster);
        match (catalog.form_of_attack(skill), form) {
            (Some(of), Some(current)) if of.id == current => {}
            (Some(_), _) => return Err(CastError::NotInForm),
            (None, Some(_)) => return Err(CastError::InForm),
            (None, None) => {}
        }
        // Un grappin s'accroche au bloc visé : sans bloc, rien n'est payé
        if cast.movement.as_ref().and_then(|m| m.grapple.as_ref()).is_some_and(|g| g.to == GrappleTo::Block) && aimed.is_none() {
            return Err(CastError::NothingToGrab);
        }
        // L'arme en main : la bonne famille, un carreau prêt, une charge assez longue
        let held = self.arsenal.held(caster);
        if !cast.weapons.is_empty() && !held.kind.is_some_and(|k| cast.weapons.contains(&k)) {
            return Err(CastError::NoWeapon);
        }
        if cast.uses_round && self.arsenal.magazine(caster).is_none_or(|m| m.loaded == 0) {
            return Err(CastError::Reloading);
        }
        let charged = match &cast.charge {
            Some(charge) => charge.power_after(self.arsenal.charge_held(caster, Some(requested))).ok_or(CastError::NotCharged)?,
            None => 1.0,
        };
        let rank = catalog.rank(stats, skill);
        let amplify = catalog.fusion().and_then(|f| f.fusion.as_ref()).map(|f| f.amplify.clone()).unwrap_or_default();
        let (cost, power, size) = if amplified { (amplify.cost, amplify.power, amplify.area) } else { (1.0, 1.0, 1.0) };
        let (mana, stamina) = (cast.cost.mana.at(rank) * cost, cast.cost.stamina.at(rank) * cost);
        if stats.mana < mana {
            return Err(CastError::NoMana);
        }
        if stats.stamina < stamina {
            return Err(CastError::NoStamina);
        }
        if def.form.is_some() {
            let lasting = cast.self_effects.iter().chain(&cast.effects).filter_map(|e| match &e.effect {
                Effect::Buff { duration, .. } | Effect::Shield { duration, .. } | Effect::OverTime { duration, .. } => Some(duration.at(rank)),
                _ => None,
            });
            let seconds = catalog.impact_delay(cast) + lasting.fold(0.0, f32::max);
            self.forms.retain(|f| f.0 != caster);
            self.forms.push((caster, skill.to_string(), seconds));
        }
        stats.mana -= mana;
        stats.stamina -= stamina;
        if cast.charge.is_some() {
            self.arsenal.release_charge(caster, Some(requested));
        }
        if cast.uses_round {
            let _ = self.arsenal.use_round(caster);
        }
        let power = power * charged;
        self.cooldowns.insert((caster, requested.to_string()), cast.cooldown.at(rank));
        // Une attaque lancée invisible le dévoile, et frappe en critique
        let attacks = cast.effects.iter().any(|e| matches!(e.effect, Effect::Damage { .. } | Effect::OverTime { damage: Some(_), .. }));
        let crit = attacks && stats.is_invisible();
        if attacks {
            stats.invisible_time = 0.0;
        }
        self.remember(catalog, caster, requested);
        self.pending.push(Pending { caster, skill: skill.to_string(), rank, aim: aim.normalize_or_zero(), aimed, left: catalog.impact_delay(cast), crit, power, size, kind: cast.damage_kind() });
        Ok(Casting { skill: skill.to_string(), rank, amplified })
    }

    /// Ce que la Fusion du joueur `caster` lancerait maintenant : refusée sans deux sorts
    /// élémentaires, ou si leur mélange n'est pas appris
    pub fn fusion_preview(&self, catalog: &SkillCatalog, caster: PlayerId, stats: &CharacterStats) -> Result<Fused, CastError> {
        let recent = self.recent.get(&caster).map(Vec::as_slice).unwrap_or_default();
        let [first, last] = recent else { return Err(CastError::FusionEmpty) };
        let fused = catalog.fuse(first, last).ok_or(CastError::FusionEmpty)?;
        if catalog.rank(stats, fused.skill()) == 0 {
            return Err(CastError::FusionLocked);
        }
        Ok(fused)
    }

    /// Le joueur `caster` a lancé `skill` : un sort élémentaire entre dans ce que la Fusion mêlera,
    /// la Fusion consomme les deux. Appelé par `cast_as` ; le client en ligne l'appelle pour prévoir.
    pub fn remember(&mut self, catalog: &SkillCatalog, caster: PlayerId, skill: &str) {
        if catalog.get(skill).is_some_and(|d| d.fusion.is_some()) {
            self.recent.remove(&caster);
        } else if catalog.element_of(skill).is_some() {
            let recent = self.recent.entry(caster).or_default();
            recent.push(skill.to_string());
            if recent.len() > 2 {
                recent.remove(0);
            }
        }
    }

    /// Les sorts élémentaires que la Fusion du joueur `caster` mêlera (deux au plus, le dernier en fin)
    pub fn recent_spells(&self, caster: PlayerId) -> &[String] {
        self.recent.get(&caster).map(Vec::as_slice).unwrap_or_default()
    }

    /// Le joueur `player` subit `damage` points : la part qu'il garde, et celle qu'encaisse le
    /// lanceur qui l'a lié (Marque du Martyr), s'il en est un. Un lien compté en coups (Intervention
    /// Vertueuse) s'use, et se défait à son dernier.
    pub fn share_damage(&mut self, player: PlayerId, damage: f32) -> (f32, Option<(PlayerId, f32)>) {
        let Some(i) = self.links.iter().position(|l| l.ally == player) else { return (damage, None) };
        let link = &mut self.links[i];
        let (caster, share) = (link.caster, link.share);
        if let Some(hits) = &mut link.hits {
            *hits = hits.saturating_sub(1);
            if *hits == 0 {
                self.links.remove(i);
            }
        }
        (damage * (1.0 - share), Some((caster, damage * share)))
    }

    /// Le joueur `player` vient de tomber à zéro point de vie. S'il porte un renforcement à effet
    /// de mort (Abomination de l'Étherium), ses effets partent autour de lui, le renforcement et
    /// les saignements de sa compétence s'arrêtent, et il survit avec un peu de vie : renvoie alors
    /// la frappe à annoncer. `None` : il meurt.
    pub fn last_stand(&mut self, player: PlayerId, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> Option<SkillImpact> {
        let buff = self.buffs.iter().find(|b| b.player == player && b.on_death.is_some())?.clone();
        let around = buff.on_death.as_ref()?;
        let feet = players.feet(player)?;
        self.buffs.retain(|b| !(b.player == player && b.skill == buff.skill));
        self.over_time.retain(|o| !(o.caster == player && o.target == SkillTarget::Player(player) && o.per_second < 0.0));
        // Relevé sur place : le client qui l'a vu tomber le remet debout
        let stats = players.stats_mut(player)?;
        stats.fallen = true;
        stats.resurrect(LAST_STAND_HEALTH);
        let (touched, _) = self.burst(around, player, buff.rank, &buff.skill, &[feet], creatures, players);
        Some(SkillImpact { caster: player, skill: buff.skill, rank: buff.rank, center: feet, creatures: touched, players: Vec::new(), summoned: Vec::new(), parried: false, contact: false })
    }

    /// Joue les effets de `around` dans son rayon autour de chacun des `points`, une fois par cible :
    /// renvoie les créatures et les joueurs touchés
    fn burst(&mut self, around: &Around, caster: PlayerId, rank: u8, skill: &str, points: &[Vec3], creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> (Vec<CreatureId>, Vec<PlayerId>) {
        let radius = around.radius.at(rank) * creatures.block_size();
        let near = |at: Vec3| points.iter().any(|p| p.distance(at) <= radius);
        let beasts: Vec<(CreatureId, bool)> = creatures.creatures().iter().filter(|c| c.is_alive() && near(c.position)).map(|c| (c.id, c.tamed)).collect();
        let people: Vec<PlayerId> = players.ids().into_iter().filter(|&id| players.feet(id).is_some_and(near)).collect();
        let feet = players.feet(caster).or(points.first().copied()).unwrap_or(Vec3::ZERO);
        let p = Pending { caster, skill: skill.to_string(), rank, aim: Vec3::ZERO, aimed: None, left: 0.0, crit: false, power: 1.0, size: 1.0, kind: DamageKind::Physical };
        let damage_factor = 1.0 + self.buff(caster, BuffStat::Damage);
        let (mut touched, mut helped) = (Vec::new(), Vec::new());
        for entry in &around.effects {
            let to = entry.to.unwrap_or_default();
            for &(id, tamed) in &beasts {
                if matches!(to, Targets::All) || (to == Targets::Enemies && !tamed) || (to == Targets::Allies && tamed) {
                    self.apply_to_creature(entry, &p, id, feet, damage_factor, creatures);
                    touched.push(id);
                }
            }
            for &id in &people {
                if matches!(to, Targets::Allies | Targets::All) || (to == Targets::Caster && id == caster) {
                    self.apply_to_player(entry, caster, rank, 1.0, skill, id, players);
                    helped.push(id);
                }
            }
        }
        touched.sort_unstable();
        touched.dedup();
        helped.sort_unstable();
        helped.dedup();
        (touched, helped)
    }

    /// Une attaque du joueur `caster` a touché aux points `points` : ses renforcements qui portent
    /// des effets (Marteau Consacré) les jouent autour. Renvoie les joueurs dont les jauges changent.
    pub fn ride(&mut self, caster: PlayerId, points: &[Vec3], creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> Vec<PlayerId> {
        let riders: Vec<(Around, u8, String)> = self.buffs.iter().filter(|b| b.player == caster).filter_map(|b| b.rider.clone().map(|r| (r, b.rank, b.skill.clone()))).collect();
        let mut changed = Vec::new();
        if points.is_empty() {
            return changed;
        }
        for (around, rank, skill) in riders {
            changed.extend(self.burst(&around, caster, rank, &skill, points, creatures, players).1);
        }
        changed
    }

    /// Dégâts en plus du joueur `player` contre la créature `id`, selon la famille de son espèce
    /// (Arme de l'Aube contre les créatures de l'ombre)
    fn against(&self, player: PlayerId, id: CreatureId, creatures: &CreatureWorld) -> f32 {
        let Some(species) = creatures.creatures().iter().find(|c| c.id == id).and_then(|c| creatures.registry().get(c.species)) else { return 0.0 };
        self.buffs.iter().filter(|b| b.player == player && b.stat == BuffStat::Damage).filter_map(|b| b.against.as_ref()).filter(|(tag, _)| species.tags.contains(tag)).map(|(_, value)| value).sum()
    }

    /// Facteur des dégâts d'un coup d'arme du joueur `player` sur la créature `id` : ses renforcements
    /// de dégâts, famille de la créature comprise
    pub fn hit_factor(&self, player: PlayerId, id: CreatureId, creatures: &CreatureWorld) -> f32 {
        1.0 + self.buff(player, BuffStat::Damage) + self.against(player, id, creatures)
    }

    /// La forme (compétence du Druide) que le joueur `player` a prise en ce moment
    pub fn form(&self, player: PlayerId) -> Option<&str> {
        self.forms.iter().find(|f| f.0 == player).map(|f| f.1.as_str())
    }

    /// Renforcement de `stat` du joueur `player` (0 : aucun ; 0,3 : +30 %)
    pub fn buff(&self, player: PlayerId, stat: BuffStat) -> f32 {
        self.buffs.iter().filter(|b| b.player == player && b.stat == stat).map(|b| b.value * b.scale).sum()
    }

    /// Facteur des dégâts de type `kind` que subit le joueur `player` : ses renforcements de
    /// défense qui valent contre tous les coups, ou contre ce type
    pub fn damage_taken_factor(&self, player: PlayerId, kind: DamageKind) -> f32 {
        if self.immune(player, Status::Damage) {
            return 0.0;
        }
        let defense: f32 = self.buffs.iter().filter(|b| b.player == player && b.stat == BuffStat::Defense && b.kind.is_none_or(|k| k == kind)).map(|b| b.value * b.scale).sum();
        (1.0 - defense).clamp(0.1, 1.0)
    }

    /// Le joueur `player` est immunisé contre `status` en ce moment
    pub fn immune(&self, player: PlayerId, status: Status) -> bool {
        self.immunities.iter().any(|i| i.player == player && i.from.contains(&status))
    }

    /// Tout ce que le joueur `player` ignore en ce moment
    pub fn immunities(&self, player: PlayerId) -> Vec<Status> {
        let mut out: Vec<Status> = self.immunities.iter().filter(|i| i.player == player).flat_map(|i| i.from.iter().copied()).collect();
        out.sort_unstable_by_key(|s| *s as u8);
        out.dedup();
        out
    }

    /// Le joueur `player` garde en ce moment : la compétence de sa garde
    pub fn guarding(&self, player: PlayerId) -> Option<&str> {
        self.guards.iter().find(|g| g.player == player).map(|g| g.skill.as_str())
    }

    /// Le joueur `player`, qui regarde vers `facing` (s'il est connu), reçoit de la créature
    /// `attacker` un coup de `damage` points, de type `kind` (`physical`, `magical`). Sa garde, s'il
    /// en a levé une et que le coup arrive de face, en bloque sa part et lance sa riposte.
    pub fn parry(&mut self, player: PlayerId, facing: Option<Vec3>, attacker: CreatureId, damage: f32, kind: &str, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> Parry {
        let untouched = Parry { damage, parried: false, impact: None };
        let Some(i) = self.guards.iter().position(|g| g.player == player && g.guard.blocks(kind)) else { return untouched };
        let Some(feet) = players.feet(player) else { return untouched };
        let from = creatures.position(attacker);
        let g = &self.guards[i];
        let facing = facing.filter(|f| *f != Vec3::ZERO).unwrap_or(g.aim);
        let arc = g.guard.arc.unwrap_or(360.0);
        if from.is_some_and(|from| !herocraft_common::in_front(facing, feet, from, arc)) {
            return untouched;
        }
        let g = &mut self.guards[i];
        let blocked = damage * g.guard.block.at(g.rank).clamp(0.0, 1.0);
        g.parried += 1;
        if let Some(hits) = &mut g.hits {
            *hits = hits.saturating_sub(1);
        }
        let g = g.clone();
        if g.hits == Some(0) {
            self.guards.remove(i);
        }
        // La riposte part autour de l'attaquant, ou du joueur (un coup circulaire, un éclat)
        let point = match &g.guard.counter {
            Some(c) if c.at == CounterAt::Caster => feet,
            _ => from.unwrap_or(feet),
        };
        let (mut touched, mut helped) = (Vec::new(), Vec::new());
        if let Some(counter) = &g.guard.counter {
            (touched, helped) = self.burst(&counter.around, player, g.rank, &g.skill, &[point], creatures, players);
        }
        if let Some(reflect) = &g.guard.reflect {
            let back = blocked * reflect.at(g.rank) * (1.0 + self.buff(player, BuffStat::Damage));
            if back > 0.0 && creatures.wound(attacker, feet, player, back).is_some() && !touched.contains(&attacker) {
                touched.push(attacker);
            }
        }
        let impact = g.guard.ripostes().then(|| SkillImpact { caster: player, skill: g.skill.clone(), rank: g.rank, center: point, creatures: touched, players: helped, summoned: Vec::new(), parried: true, contact: false });
        Parry { damage: damage - blocked, parried: true, impact }
    }

    /// Entités invoquées par le joueur `player` encore présentes
    pub fn summoned_by(&self, player: PlayerId) -> impl Iterator<Item = CreatureId> + '_ {
        self.summoned.iter().filter(move |s| s.caster == player).map(|s| s.id)
    }

    /// Oublie le joueur `player` (il quitte la partie) ; ses invocations disparaissent avec lui
    pub fn forget_with(&mut self, player: PlayerId, creatures: &mut CreatureWorld) {
        for s in self.summoned.iter().filter(|s| s.caster == player) {
            creatures.dismiss(s.id);
        }
        self.forget(player);
    }

    /// Oublie le joueur `player` (il quitte la partie)
    pub fn forget(&mut self, player: PlayerId) {
        self.summoned.retain(|s| s.caster != player);
        self.pending.retain(|p| p.caster != player);
        self.over_time.retain(|o| o.caster != player && o.target != SkillTarget::Player(player));
        self.buffs.retain(|b| b.player != player);
        self.forms.retain(|f| f.0 != player);
        self.marks.retain(|m| m.0 != player);
        self.recent.remove(&player);
        self.links.retain(|l| l.caster != player && l.ally != player);
        self.zones.retain(|z| z.pending.caster != player);
        self.delayed.retain(|d| d.pending.caster != player);
        self.moves.retain(|m| m.0 != player);
        self.guards.retain(|g| g.player != player);
        self.immunities.retain(|i| i.player != player);
        self.cooldowns.retain(|(p, _), _| *p != player);
        self.arsenal.forget(player);
    }

    /// Avance de `dt` secondes : recharges, frappes arrivées à leur impact, effets qui durent
    pub fn tick(&mut self, dt: f32, catalog: &SkillCatalog, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> SkillTick {
        let mut out = SkillTick::default();
        self.tick_weapons(dt);
        for left in self.cooldowns.values_mut() {
            *left -= dt;
        }
        self.cooldowns.retain(|_, left| *left > 0.0);
        for form in &mut self.forms {
            form.2 -= dt;
        }
        self.forms.retain(|f| f.2 > 0.0);
        for mark in &mut self.marks {
            mark.3 -= dt;
        }
        self.marks.retain(|m| m.3 > 0.0);
        for link in &mut self.links {
            link.left -= dt;
        }
        self.links.retain(|l| l.left > 0.0);
        for immunity in &mut self.immunities {
            immunity.left -= dt;
        }
        self.immunities.retain(|i| i.left > 0.0);
        // Une garde qui se ferme sans avoir rien paré : la recharge de sa compétence s'allonge
        for g in &mut self.guards {
            g.left -= dt;
        }
        let (closed, open): (Vec<Guarding>, Vec<Guarding>) = std::mem::take(&mut self.guards).into_iter().partition(|g| g.left <= 0.0);
        self.guards = open;
        for g in closed.into_iter().filter(|g| g.parried == 0 && g.guard.miss_factor() > 1.0) {
            let Some(cooldown) = catalog.get(&g.skill).and_then(|d| d.cast.as_ref()).map(|c| c.cooldown.at(g.rank)) else { continue };
            let extra = cooldown * (g.guard.miss_factor() - 1.0);
            if extra > 0.0 {
                *self.cooldowns.entry((g.player, g.skill.clone())).or_insert(0.0) += extra;
                out.missed.push((g.player, g.skill, extra));
            }
        }
        for buff in &mut self.buffs {
            buff.left -= dt;
            if buff.scaled
                && let Some(stats) = players.stats_mut(buff.player) {
                buff.scale = missing_health(stats);
            }
            // Régénération : des points de vie rendus chaque instant
            let regenerates = matches!(buff.stat, BuffStat::Regeneration | BuffStat::ManaRegen | BuffStat::StaminaRegen);
            if regenerates && buff.value > 0.0
                && let Some(stats) = players.stats_mut(buff.player) {
                match buff.stat {
                    BuffStat::Regeneration => stats.hp = (stats.hp + buff.value * REGENERATION_HP * dt).min(stats.max_hp),
                    BuffStat::ManaRegen => stats.mana = (stats.mana + buff.value * REGENERATION_MANA * dt).min(stats.max_mana),
                    _ => stats.stamina = (stats.stamina + buff.value * REGENERATION_STAMINA * dt).min(stats.max_stamina),
                }
                out.changed.push(buff.player);
            }
        }
        self.buffs.retain(|b| b.left > 0.0);
        // Ce que les pièges ont fait apparaître (mines de l'Éradication Ciblée) disparaît à son heure
        for (caster, id, seconds) in creatures.take_spawned() {
            let species = creatures.creatures().iter().find(|c| c.id == id).and_then(|c| creatures.registry().get(c.species)).map(|s| s.id.clone()).unwrap_or_default();
            self.summoned.push(Summoned { caster, species, id, left: Some(seconds), on_end: Vec::new(), rank: 1, advance: None });
        }
        // Les murs qui avancent glissent droit devant, en repoussant les ennemis
        let block = creatures.block_size();
        for s in &mut self.summoned {
            let Some((dir, left, speed)) = &mut s.advance else { continue };
            let step = (*speed * dt).min(*left);
            *left -= step;
            creatures.slide(s.id, *dir * step * block);
            if let Some(at) = creatures.creatures().iter().find(|c| c.id == s.id).map(|c| c.position) {
                let ahead: Vec<CreatureId> = creatures
                    .creatures()
                    .iter()
                    .filter(|c| c.is_alive() && !c.tamed && c.summoner.is_none())
                    .filter(|c| {
                        let off = c.position - at;
                        let t = off.dot(*dir) / block;
                        (-0.5..=ADVANCE_REACH).contains(&t) && (off - *dir * off.dot(*dir)).length() / block <= ADVANCE_HALF_WIDTH
                    })
                    .map(|c| c.id)
                    .collect();
                for id in ahead {
                    let from = creatures.creatures().iter().find(|c| c.id == id).map_or(at, |c| c.position) - *dir * block;
                    creatures.shove(id, from, (*speed * 1.5).max(4.0));
                }
            }
            if *left <= 0.0 {
                s.advance = None;
            }
        }
        // Les invocations détruites sont oubliées ; celles dont la durée est passée disparaissent
        let alive: std::collections::HashSet<CreatureId> = creatures.creatures().iter().filter(|c| c.is_alive()).map(|c| c.id).collect();
        let mut ended = Vec::new();
        self.summoned.retain_mut(|s| {
            if !alive.contains(&s.id) {
                creatures.dismiss(s.id);
                return false;
            }
            if let Some(left) = &mut s.left {
                *left -= dt;
                if *left <= 0.0 {
                    creatures.dismiss(s.id);
                    ended.push((s.caster, s.rank, std::mem::take(&mut s.on_end)));
                    return false;
                }
            }
            true
        });
        for (caster, rank, effects) in ended {
            for entry in &effects {
                self.apply_to_player(entry, caster, rank, 1.0, "", caster, players);
            }
            if !effects.is_empty() {
                out.changed.push(caster);
            }
        }

        // Les défenses contre les tirs s'usent ; le monde des créatures les apprend
        for g in &mut self.wards {
            g.left -= dt;
        }
        self.wards.retain(|g| g.left > 0.0);
        let mut guards: Vec<crate::PlayerGuard> = Vec::new();
        for g in &self.wards {
            let at = match guards.iter().position(|o| o.key == g.player) {
                Some(i) => i,
                None => {
                    guards.push(crate::PlayerGuard { key: g.player, front: None, deflect: None });
                    guards.len() - 1
                }
            };
            guards[at].front = guards[at].front.or(g.front);
            guards[at].deflect = match (guards[at].deflect, g.deflect) {
                (Some(herocraft_common::ProjectileSize::Large), _) | (_, Some(herocraft_common::ProjectileSize::Large)) => Some(herocraft_common::ProjectileSize::Large),
                (a, b) => a.or(b),
            };
        }
        creatures.set_guards(guards);
        // Les tirs en vol ont touché ou se sont arrêtés
        for contact in creatures.take_contacts() {
            match contact {
                crate::SkillContact::Hit { tag, creature, at } => {
                    let Some(flight) = self.flights.get_mut(&tag) else { continue };
                    flight.hits += 1;
                    let (p, factor, first) = (flight.pending.clone(), flight.damage_factor, flight.hits == 1);
                    if let Some(impact) = self.touch(catalog, &p, creature, at, factor, first, creatures, players, &mut out.changed) {
                        out.impacts.push(impact);
                    }
                }
                crate::SkillContact::Over { tag, .. } => {
                    let Some(flight) = self.flights.remove(&tag) else { continue };
                    let p = flight.pending;
                    // Perdu sans rien toucher : ce que la compétence fait quand elle manque
                    if flight.hits == 0
                        && let Some(cast) = catalog.get(&p.skill).and_then(|d| d.cast.as_ref()) {
                        for entry in &cast.on_miss {
                            self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
                            out.changed.push(p.caster);
                        }
                    }
                }
            }
        }

        for p in &mut self.pending {
            p.left -= dt;
        }
        let (ready, waiting): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.pending).into_iter().partition(|p| p.left <= 0.0);
        self.pending = waiting;
        for p in ready {
            if let Some(impact) = self.strike(catalog, &p, creatures, players, &mut out.changed) {
                out.impacts.push(impact);
            }
        }

        for d in &mut self.delayed {
            d.left -= dt;
        }
        let (ready, waiting): (Vec<Delayed>, Vec<Delayed>) = std::mem::take(&mut self.delayed).into_iter().partition(|d| d.left <= 0.0);
        self.delayed = waiting;
        for d in ready {
            match d.what {
                Later::Teleport => self.teleport(catalog, &d, creatures, players),
                Later::FollowUp => {
                    if let Some(impact) = self.follow_up(catalog, &d, creatures, players, &mut out.changed) {
                        out.impacts.push(impact);
                    }
                }
            }
        }
        out.moves = std::mem::take(&mut self.moves);

        self.pulse_zones(dt, catalog, creatures, players, &mut out.changed);

        let block = creatures.block_size();
        let mut finished = Vec::new();
        let mut fell = Vec::new();
        for (i, o) in self.over_time.iter_mut().enumerate() {
            o.left -= dt;
            o.next -= dt;
            while o.next <= 0.0 && o.left > -OVER_TIME_STEP {
                o.next += OVER_TIME_STEP;
                let amount = o.per_second * OVER_TIME_STEP;
                match o.target {
                    SkillTarget::Creature(id) if amount < 0.0 => {
                        // Un saignement s'aggrave quand la bête bouge
                        let now = creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position);
                        let moved = matches!((o.last, now), (Some(a), Some(b)) if a.distance(b) > 0.3 * block);
                        o.last = now;
                        let amount = if moved { amount * o.moving } else { amount };
                        let from = players.feet(o.caster).unwrap_or_else(|| creatures.creatures().iter().find(|c| c.id == id).map_or(Vec3::ZERO, |c| c.position + Vec3::ONE * block));
                        creatures.wound_with(id, from, o.caster, -amount, o.kind, 0.0);
                    }
                    SkillTarget::Creature(_) => {}
                    SkillTarget::Player(id) => {
                        if let Some(stats) = players.stats_mut(id) {
                            if amount > 0.0 {
                                stats.hp = (stats.hp + amount).min(stats.max_hp);
                            } else if stats.hp > 0.0 && stats.take_damage(-amount) {
                                fell.push(id);
                            }
                            out.changed.push(id);
                        }
                    }
                }
            }
            if o.left <= 0.0 {
                finished.push(i);
            }
        }
        for i in finished.into_iter().rev() {
            self.over_time.swap_remove(i);
        }
        // Tombé d'un saignement : un effet à la mort le sauve, sinon il meurt
        fell.dedup();
        for id in fell {
            match self.last_stand(id, creatures, players) {
                Some(impact) => out.impacts.push(impact),
                None => out.died.push(id),
            }
        }
        out.changed.sort_unstable();
        out.changed.dedup();
        out
    }

    /// La frappe du lancement `p` : zone posée devant le lanceur, effets sur chaque cible
    fn strike(&mut self, catalog: &SkillCatalog, p: &Pending, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, changed: &mut Vec<PlayerId>) -> Option<SkillImpact> {
        let def = catalog.get(&p.skill)?;
        let cast = def.cast.as_ref()?;
        let feet = players.feet(p.caster)?;
        let block = creatures.block_size();
        let reach = 1.0 + self.buff(p.caster, BuffStat::Range);
        // Un tir à tête chercheuse se pose sur la bête marquée la plus proche du point visé
        let mut aimed = p.aimed;
        if cast.homing {
            let target = aimed.unwrap_or(feet + p.aim * 12.0 * block);
            let range = match &cast.area {
                herocraft_common::skills::Area::Circle { distance, .. } => distance.at(p.rank) * reach * block,
                _ => f32::MAX,
            };
            let marked: Vec<CreatureId> = self.marks.iter().filter(|m| m.0 == p.caster).map(|m| m.1).collect();
            aimed = creatures
                .creatures()
                .iter()
                .filter(|c| c.is_alive() && marked.contains(&c.id) && c.position.distance(feet) <= range)
                .min_by(|a, b| a.position.distance(target).total_cmp(&b.position.distance(target)))
                .map(|c| c.position)
                .or(aimed);
        }
        let area = cast.area.place_scaled(p.rank, feet, p.aim, aimed, block, p.size, reach);
        let center = area.center(feet);
        let damage_factor = (1.0 + self.buff(p.caster, BuffStat::Damage)) * p.power;
        self.aims.insert(p.caster, p.aim);

        // Un vrai projectile : le tir part, ses touches viendront en vol
        if let Some(projectile) = &cast.projectile {
            let up = feet.normalize_or_zero();
            let from = feet + up * SHOT_HEIGHT * block;
            let (direction, range, radius) = match area {
                herocraft_common::skills::PlacedArea::Line { forward, length, half_width, .. } => (forward, length / block, half_width / block),
                herocraft_common::skills::PlacedArea::Circle { center, .. } => {
                    let to = center + up * SHOT_HEIGHT * block - from;
                    (to.normalize_or_zero(), to.length() / block + 1.0, 0.5)
                }
                _ => (p.aim, 12.0, 0.5),
            };
            let tag = self.next_flight;
            self.next_flight += 1;
            let bounce = projectile.bounce.as_ref();
            creatures.launch_skill_shot(crate::SkillShot {
                caster: p.caster,
                tag,
                skill: p.skill.clone(),
                from,
                direction,
                speed: projectile.speed.at(p.rank),
                range,
                radius,
                pierce: projectile.pierce.at(p.rank),
                bounces: bounce.map_or(0, |b| b.count.at(p.rank).max(0.0).round() as u32),
                bounce_range: bounce.map_or(0.0, |b| b.range.at(p.rank)),
                through_terrain: projectile.through.contains(&herocraft_common::skills::Through::Terrain),
                through_obstacles: projectile.through.contains(&herocraft_common::skills::Through::Obstacles),
                size: projectile.size,
            });
            self.flights.insert(tag, Flight { pending: Pending { aimed, ..p.clone() }, hits: 0, damage_factor });
            for entry in &cast.self_effects {
                self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
                changed.push(p.caster);
            }
            let summoned = self.summon(def.summons(), p, feet, center, &area, creatures);
            return Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center: from, creatures: Vec::new(), players: Vec::new(), summoned, parried: false, contact: false });
        }

        // Dans une zone qui reste, les effets qui durent valent tant qu'on s'y trouve
        let staying: Vec<EffectEntry>;
        let effects = match &cast.zone {
            Some(zone) => {
                staying = cast.effects.iter().filter_map(|e| zone_entry(e, true)).collect();
                self.zones.push(Zone { pending: Pending { aimed, crit: false, ..p.clone() }, feet, follow: zone.follow, left: zone.duration.at(p.rank), next: ZONE_STEP });
                &staying
            }
            None => &cast.effects,
        };
        let (mut touched_creatures, mut touched_players, creature_hits) = self.sweep(cast.targets, effects, p, feet, &area, damage_factor, creatures, players);
        changed.extend(&touched_players);
        for entry in &cast.self_effects {
            self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
            changed.push(p.caster);
        }
        for g in self.guards.iter_mut().filter(|g| g.player == p.caster && g.skill == p.skill) {
            g.aim = p.aim;
        }
        let summoned = self.summon(def.summons(), p, feet, center, &area, creatures);
        touched_creatures.sort_unstable();
        touched_creatures.dedup();
        // Ce que la frappe déclenche : par ennemi touché, si elle manque, autour de ceux qu'elle tue
        let enemies: Vec<CreatureId> = touched_creatures.iter().copied().filter(|id| creature_hits.contains(&(*id, false))).collect();
        // Déplacements et seconde frappe : vers les ennemis de la zone, le plus proche d'abord
        let mut found: Vec<(CreatureId, f32)> = creature_hits
            .iter()
            .filter(|(_, tamed)| !tamed)
            .filter_map(|&(id, _)| creatures.creatures().iter().find(|c| c.id == id).map(|c| (id, c.position.distance(feet))))
            .collect();
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        let found: Vec<CreatureId> = found.into_iter().map(|f| f.0).collect();
        if let (Some(&first), Some(movement)) = (found.first(), cast.movement.as_ref()) {
            if let Some(teleport) = &movement.teleport {
                self.delayed.push(Delayed { pending: p.clone(), targets: vec![first], left: teleport.delay.at(p.rank), what: Later::Teleport });
            }
            if let Some(grapple) = movement.grapple.as_ref().filter(|g| g.to == GrappleTo::Target)
                && let Some(to) = creatures.creatures().iter().find(|c| c.id == first).map(|c| c.position) {
                self.moves.push((p.caster, SkillMove::Pull { to, speed: grapple.speed.at(p.rank), stop: grapple.stop.at(p.rank) }));
            }
        }
        if let Some(follow) = cast.follow_up.as_ref().filter(|_| !found.is_empty()) {
            self.delayed.push(Delayed { pending: p.clone(), targets: found.clone(), left: follow.delay.at(p.rank), what: Later::FollowUp });
        }
        let triggered = if enemies.is_empty() { &cast.on_miss } else { &cast.on_hit };
        for _ in 0..enemies.len().max(1) {
            for entry in triggered {
                self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
                changed.push(p.caster);
            }
        }
        let place = |creatures: &CreatureWorld, id: CreatureId| creatures.creatures().iter().find(|c| c.id == id).map(|c| (c.position, c.is_alive()));
        let spots: Vec<(Vec3, bool)> = enemies.iter().filter_map(|&id| place(creatures, id)).collect();
        if let Some(around) = &cast.on_kill {
            let corpses: Vec<Vec3> = spots.iter().filter(|s| !s.1).map(|s| s.0).collect();
            if !corpses.is_empty() {
                let (_, helped) = self.burst(around, p.caster, p.rank, &p.skill, &corpses, creatures, players);
                touched_players.extend(&helped);
                changed.extend(helped);
            }
        }
        let attacks = cast.effects.iter().any(|e| matches!(e.effect, Effect::Damage { .. }));
        // Une compétence de tir emporte la munition spéciale chargée
        if attacks && !enemies.is_empty() && cast.weapons.iter().any(|w| w.is_ranged())
            && let Some(special) = self.arsenal.take_special(p.caster) {
            self.apply_special(&special, p.caster, &enemies, creatures, players);
        }
        if attacks {
            let points: Vec<Vec3> = spots.iter().map(|s| s.0).collect();
            changed.extend(self.ride(p.caster, &points, creatures, players));
        }
        touched_players.sort_unstable();
        touched_players.dedup();
        Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center, creatures: touched_creatures, players: touched_players, summoned, parried: false, contact: false })
    }

    /// Le tir en vol du lancement `p` touche la créature `id` au point `at` : ses effets sur elle,
    /// ce qu'une touche déclenche pour le lanceur, ce que fait sa mort
    fn touch(&mut self, catalog: &SkillCatalog, p: &Pending, id: CreatureId, at: Vec3, damage_factor: f32, first: bool, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, changed: &mut Vec<PlayerId>) -> Option<SkillImpact> {
        let cast = catalog.get(&p.skill)?.cast.as_ref()?;
        let feet = players.feet(p.caster).unwrap_or(at);
        if creatures.creatures().iter().find(|c| c.id == id)?.tamed {
            return None;
        }
        for entry in &cast.effects {
            if matches!(entry.to.unwrap_or(cast.targets), Targets::Enemies | Targets::All) {
                self.apply_to_creature(entry, p, id, feet, damage_factor, creatures);
            }
        }
        for entry in &cast.on_hit {
            self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
            changed.push(p.caster);
        }
        // Le premier ennemi touché emporte le lanceur (téléportation, grappin) ; chacun reçoit la
        // seconde frappe
        if let (true, Some(movement)) = (first, cast.movement.as_ref()) {
            if let Some(teleport) = &movement.teleport {
                self.delayed.push(Delayed { pending: p.clone(), targets: vec![id], left: teleport.delay.at(p.rank), what: Later::Teleport });
            }
            if let Some(grapple) = movement.grapple.as_ref().filter(|g| g.to == GrappleTo::Target)
                && let Some(to) = creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position) {
                self.moves.push((p.caster, SkillMove::Pull { to, speed: grapple.speed.at(p.rank), stop: grapple.stop.at(p.rank) }));
            }
        }
        if let Some(follow) = &cast.follow_up {
            self.delayed.push(Delayed { pending: p.clone(), targets: vec![id], left: follow.delay.at(p.rank), what: Later::FollowUp });
        }
        let mut touched_players = Vec::new();
        let spot = creatures.creatures().iter().find(|c| c.id == id).map(|c| (c.position, c.is_alive()));
        if let (Some(around), Some((corpse, false))) = (&cast.on_kill, spot) {
            let (_, helped) = self.burst(around, p.caster, p.rank, &p.skill, &[corpse], creatures, players);
            touched_players.extend(&helped);
            changed.extend(helped);
        }
        let attacks = cast.effects.iter().any(|e| matches!(e.effect, Effect::Damage { .. }));
        // Le premier ennemi touché par un tir d'arme reçoit la munition spéciale chargée
        if attacks && first && cast.weapons.iter().any(|w| w.is_ranged())
            && let Some(special) = self.arsenal.take_special(p.caster) {
            self.apply_special(&special, p.caster, &[id], creatures, players);
        }
        if attacks {
            let points: Vec<Vec3> = spot.map(|s| s.0).into_iter().collect();
            changed.extend(self.ride(p.caster, &points, creatures, players));
        }
        Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center: at, creatures: vec![id], players: touched_players, summoned: Vec::new(), parried: false, contact: true })
    }

    /// Téléporte le lanceur près de sa cible (`movement.teleport`), dans le repère de la cible vue
    /// de lui
    fn teleport(&mut self, catalog: &SkillCatalog, d: &Delayed, creatures: &CreatureWorld, players: &mut dyn SkillPlayers) {
        let p = &d.pending;
        let Some(teleport) = catalog.get(&p.skill).and_then(|s| s.cast.as_ref()).and_then(|c| c.movement.as_ref()).and_then(|m| m.teleport.as_ref()) else { return };
        let (Some(feet), Some(&target)) = (players.feet(p.caster), d.targets.first()) else { return };
        let Some(at) = creatures.creatures().iter().find(|c| c.id == target).map(|c| c.position) else { return };
        let block = creatures.block_size();
        let (feet, look) = teleport_spot(teleport, p.rank, feet, at, p.aim, block);
        self.moves.push((p.caster, SkillMove::Teleport { feet, look }));
    }

    /// La seconde frappe (`follow_up`) sur les ennemis trouvés par la première, encore en vie
    fn follow_up(&mut self, catalog: &SkillCatalog, d: &Delayed, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, changed: &mut Vec<PlayerId>) -> Option<SkillImpact> {
        let p = &d.pending;
        let follow = catalog.get(&p.skill)?.cast.as_ref()?.follow_up.as_ref()?;
        let feet = players.feet(p.caster)?;
        let damage_factor = (1.0 + self.buff(p.caster, BuffStat::Damage)) * p.power;
        let alive: Vec<(CreatureId, Vec3)> = d.targets.iter().filter_map(|&id| creatures.creatures().iter().find(|c| c.id == id && c.is_alive()).map(|c| (id, c.position))).collect();
        let center = alive.first()?.1;
        for entry in &follow.effects {
            match entry.to.unwrap_or(Targets::Enemies) {
                Targets::Caster => {
                    self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, p.caster, players);
                    changed.push(p.caster);
                }
                Targets::Enemies | Targets::All => {
                    for &(id, _) in &alive {
                        self.apply_to_creature(entry, p, id, feet, damage_factor, creatures);
                    }
                }
                Targets::Allies | Targets::Summons => {}
            }
        }
        let touched: Vec<CreatureId> = alive.iter().map(|a| a.0).collect();
        Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center, creatures: touched, players: Vec::new(), summoned: Vec::new(), parried: false, contact: false })
    }

    /// Applique les effets `effects` à ceux qui se trouvent dans la zone `area` : les créatures et
    /// les joueurs touchés, et les créatures vivantes de la zone (identifiant, alliée ou non)
    #[allow(clippy::type_complexity)]
    fn sweep(&mut self, targets: Targets, effects: &[EffectEntry], p: &Pending, feet: Vec3, area: &PlacedArea, damage_factor: f32, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> (Vec<CreatureId>, Vec<PlayerId>, Vec<(CreatureId, bool)>) {
        let block = creatures.block_size();
        // Les cibles de la zone : créatures vivantes (les bêtes apprivoisées sont des alliées) et joueurs
        let in_area = |point: Vec3| area.contains(point, block);
        let found: Vec<(CreatureId, bool, bool)> = creatures.creatures().iter().filter(|c| c.is_alive() && in_area(c.position)).map(|c| (c.id, c.tamed, c.summoner == Some(p.caster))).collect();
        let player_hits: Vec<PlayerId> = players
            .ids()
            .into_iter()
            .filter(|&id| id == p.caster && matches!(area, PlacedArea::Caster) || players.feet(id).is_some_and(in_area))
            .collect();
        let mut touched_creatures = Vec::new();
        let mut touched_players = Vec::new();
        for entry in effects {
            let to = entry.to.unwrap_or(targets);
            for &(id, tamed, mine) in &found {
                let applies = match to {
                    Targets::All => true,
                    Targets::Enemies => !tamed,
                    Targets::Allies => tamed,
                    Targets::Summons => mine,
                    Targets::Caster => false,
                };
                if applies {
                    self.apply_to_creature(entry, p, id, feet, damage_factor, creatures);
                    touched_creatures.push(id);
                }
            }
            for &id in &player_hits {
                let applies = match to {
                    Targets::Allies | Targets::All => true,
                    Targets::Caster => id == p.caster,
                    Targets::Enemies | Targets::Summons => false,
                };
                if applies {
                    self.apply_to_player(entry, p.caster, p.rank, p.power, &p.skill, id, players);
                    touched_players.push(id);
                }
            }
        }
        (touched_creatures, touched_players, found.into_iter().map(|(id, tamed, _)| (id, tamed)).collect())
    }

    /// Les zones qui restent posées rejouent leurs effets sur ceux qui s'y trouvent
    fn pulse_zones(&mut self, dt: f32, catalog: &SkillCatalog, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, changed: &mut Vec<PlayerId>) {
        let block = creatures.block_size();
        let mut zones = std::mem::take(&mut self.zones);
        for zone in &mut zones {
            zone.left -= dt;
            zone.next -= dt;
            if zone.next > 0.0 || zone.left <= 0.0 {
                continue;
            }
            zone.next += ZONE_STEP;
            let p = &zone.pending;
            let Some(cast) = catalog.get(&p.skill).and_then(|d| d.cast.as_ref()) else { continue };
            // Une aura suit son lanceur ; sinon la zone reste où elle a été posée
            let feet = if zone.follow { players.feet(p.caster).unwrap_or(zone.feet) } else { zone.feet };
            let area = cast.area.place_scaled(p.rank, feet, p.aim, p.aimed, block, p.size, 1.0);
            let effects: Vec<EffectEntry> = cast.effects.iter().filter_map(|e| zone_entry(e, false)).collect();
            let damage_factor = (1.0 + self.buff(p.caster, BuffStat::Damage)) * p.power;
            let (_, touched, _) = self.sweep(cast.targets, &effects, p, feet, &area, damage_factor, creatures, players);
            changed.extend(touched);
        }
        zones.retain(|z| z.left > 0.0);
        zones.append(&mut self.zones);
        self.zones = zones;
    }

    /// Zones encore posées : compétence, lanceur et secondes restantes
    pub fn zones(&self) -> impl Iterator<Item = (&str, PlayerId, f32)> + '_ {
        self.zones.iter().map(|z| (z.pending.skill.as_str(), z.pending.caster, z.left))
    }

    /// Les apparitions d'une frappe, une fois par lancement : en cercle autour de leur point,
    /// les plus anciennes du lanceur disparaissant au-delà de la limite
    fn summon<'a>(&mut self, summons: impl Iterator<Item = &'a Effect>, p: &Pending, feet: Vec3, center: Vec3, area: &PlacedArea, creatures: &mut CreatureWorld) -> Vec<CreatureId> {
        let mut out = Vec::new();
        let block = creatures.block_size();
        for effect in summons {
            let Effect::Summon { species, count, at, duration, limit, copies, on_end, advance } = effect else { continue };
            let Some(index) = creatures.registry().find(species) else { continue };
            let left = duration.as_ref().map(|d| d.at(p.rank));
            // Une par cadavre de la zone, à sa place et à son allure ; le cadavre disparaît
            if *copies == Some(Copies::Corpse) {
                let corpses: Vec<(CreatureId, usize, Vec3, f32)> =
                    creatures.creatures().iter().filter(|c| !c.is_alive() && c.summoner.is_none() && area.contains(c.position, block)).map(|c| (c.id, c.species, c.position, c.yaw)).collect();
                for (corpse, kind, position, yaw) in corpses {
                    creatures.remove(corpse);
                    let id = creatures.summon(index, position, yaw, p.caster);
                    creatures.set_copy(id, kind as u32);
                    self.summoned.push(Summoned { caster: p.caster, species: species.clone(), id, left, on_end: on_end.clone(), rank: p.rank, advance: None });
                    out.push(id);
                }
                continue;
            }
            let origin = match at {
                Center::Caster => feet,
                Center::Aimed => center,
            };
            // Le plan du sol au point d'apparition, orienté vers la visée
            let up = origin.normalize_or(Vec3::Y);
            let forward = (p.aim - up * p.aim.dot(up)).normalize_or(up.any_orthonormal_vector());
            let right = forward.cross(up);
            // Tournée vers la visée : un mur se dresse en travers
            let (east, north) = herocraft_common::tangent_frame(up);
            let yaw = forward.dot(east).atan2(forward.dot(north));
            let marching = advance.as_ref().map(|a| (forward, a.blocks.at(p.rank), a.speed.at(p.rank)));
            let n = count.at(p.rank).round().max(1.0) as usize;
            let radius = if n > 1 { SUMMON_SPACING * block * n as f32 / std::f32::consts::TAU } else { 0.0 };
            for i in 0..n {
                let angle = std::f32::consts::TAU * i as f32 / n as f32;
                let spot = origin + (forward * angle.cos() + right * angle.sin()) * radius;
                let id = creatures.summon(index, spot, yaw, p.caster);
                if *copies == Some(Copies::Caster) {
                    creatures.set_copy(id, herocraft_common::COPY_SUMMONER);
                }
                self.summoned.push(Summoned { caster: p.caster, species: species.clone(), id, left, on_end: on_end.clone(), rank: p.rank, advance: marching });
                out.push(id);
            }
            if let Some(limit) = limit {
                let limit = limit.at(p.rank).round().max(1.0) as usize;
                let mine: Vec<CreatureId> = self.summoned.iter().filter(|s| s.caster == p.caster && s.species == *species).map(|s| s.id).collect();
                for &old in mine.iter().take(mine.len().saturating_sub(limit)) {
                    creatures.dismiss(old);
                    out.retain(|&id| id != old);
                }
                let gone = &mine[..mine.len().saturating_sub(limit)];
                self.summoned.retain(|s| !gone.contains(&s.id));
            }
        }
        out
    }

    /// Chance de coup critique du joueur `caster` sur la créature `id` : son renforcement, plus ses marques
    pub fn critical_chance(&self, caster: PlayerId, id: CreatureId) -> f32 {
        self.buff(caster, BuffStat::Critical) + self.marks.iter().filter(|m| m.0 == caster && m.1 == id).map(|m| m.2).sum::<f32>()
    }

    /// Un tirage entre 0 et 1 (splitmix64)
    fn roll(&mut self) -> f32 {
        self.luck = self.luck.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.luck;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    fn apply_to_creature(&mut self, entry: &EffectEntry, p: &Pending, id: CreatureId, feet: Vec3, damage_factor: f32, creatures: &mut CreatureWorld) {
        let rank = p.rank;
        let damage_factor = damage_factor + self.against(p.caster, id, creatures) * p.power;
        match &entry.effect {
            Effect::Damage { power, kind, pierce } => {
                let chance = if p.crit { 1.0 } else { self.critical_chance(p.caster, id) };
                let critical = chance > 0.0 && self.roll() < chance;
                let factor = if critical { CRITICAL_FACTOR } else { 1.0 };
                creatures.wound_with(id, feet, p.caster, power.at(rank) * SKILL_BASE_DAMAGE * damage_factor * factor, kind.unwrap_or(p.kind), pierce.at(rank));
            }
            Effect::Shred { amount, duration, max } => {
                creatures.shred(id, amount.at(rank), duration.at(rank), max.as_ref().map_or(1.0, |m| m.at(rank)));
            }
            Effect::Blind { duration } => creatures.blind(id, duration.at(rank)),
            Effect::Taunt { duration } => creatures.taunt(id, p.caster, duration.at(rank)),
            Effect::Mark { chance, duration } => {
                self.marks.retain(|m| !(m.0 == p.caster && m.1 == id));
                self.marks.push((p.caster, id, chance.at(rank), duration.at(rank)));
            }
            Effect::Heal { amount } => creatures.heal(id, amount.at(rank) * p.power),
            // Une invocation alliée se protège et se presse (Surcharge Mécanique)
            Effect::Shield { amount, duration, .. } => creatures.shield(id, amount.at(rank) * p.power, duration.at(rank)),
            Effect::Buff { stat: BuffStat::AttackSpeed, value, duration, .. } => creatures.haste(id, value.at(rank), duration.at(rank)),
            Effect::Slow { strength, duration } => creatures.slow(id, strength.at(rank), duration.at(rank)),
            Effect::Stun { duration } => creatures.stun(id, duration.at(rank)),
            Effect::Knockback { strength } => creatures.shove(id, feet, strength.at(rank) * p.power),
            Effect::Pull { strength } => creatures.shove(id, feet, -strength.at(rank) * p.power),
            Effect::Launch { height } => creatures.launch(id, height.at(rank)),
            Effect::Slam { power } => creatures.slam(id, p.caster, power.at(rank) * SKILL_BASE_DAMAGE * damage_factor),
            Effect::OverTime { damage, heal, duration, moving } => {
                // Un soin sur la durée d'une bête alliée : rendu d'un coup, par passage d'une zone ou
                // pour toute sa durée
                if let Some(heal) = heal {
                    creatures.heal(id, heal.at(rank) * duration.at(rank) * p.power);
                }
                if let Some(damage) = damage {
                    self.over_time.push(OverTime {
                        caster: p.caster,
                        target: SkillTarget::Creature(id),
                        per_second: -damage.at(rank) * damage_factor,
                        left: duration.at(rank),
                        next: OVER_TIME_STEP,
                        moving: moving.as_ref().map_or(1.0, |m| m.at(rank)),
                        last: Some(feet).and_then(|_| creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position)),
                        kind: p.kind,
                    });
                }
            }
            Effect::BreakGuard { duration } => creatures.break_guard(id, duration.at(rank)),
            // Sans effet sur une bête : les autres renforcements, invisibilité, résurrection, lien
            Effect::Guard(_) | Effect::Immune { .. } | Effect::Buff { .. } | Effect::Summon { .. } | Effect::Invisible { .. } | Effect::Revive { .. } | Effect::Link { .. } | Effect::Cleanse {} | Effect::Restore { .. } | Effect::Sacrifice { .. } | Effect::Reload {} | Effect::LoadAmmo { .. } | Effect::ShotGuard { .. } | Effect::Deflect { .. } => {}
        }
    }

    /// Effet `entry` de la compétence `skill` du lanceur `caster` sur le joueur `id`, ses soins et
    /// boucliers multipliés par `power`
    fn apply_to_player(&mut self, entry: &EffectEntry, caster: PlayerId, rank: u8, power: f32, skill: &str, id: PlayerId, players: &mut dyn SkillPlayers) {
        match &entry.effect {
            Effect::Heal { amount } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.hp = (stats.hp + amount.at(rank) * power).min(stats.max_hp);
                }
            }
            Effect::Link { share, duration, hits } => {
                // On ne se lie pas à soi-même
                if id != caster {
                    self.links.retain(|l| l.ally != id);
                    let hits = hits.as_ref().map(|h| h.at(rank).round().max(1.0) as u32);
                    self.links.push(Link { caster, ally: id, share: share.at(rank).clamp(0.0, 1.0), left: duration.at(rank), hits });
                }
            }
            Effect::Shield { amount, duration, absorb_to, absorbs } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.shield = stats.shield.max(amount.at(rank) * power);
                    stats.shield_kind = *absorbs;
                    stats.shield_time = stats.shield_time.max(duration.at(rank));
                    stats.shield_mana = absorb_to.as_ref().map_or(0.0, |a| a.mana.at(rank).max(0.0));
                }
            }
            Effect::Buff { stat, value, duration, against, per_missing_health, on_death, rider, kind } => {
                self.buffs.retain(|b| !(b.player == id && b.stat == *stat));
                let against = against.as_ref().map(|a| (a.tag.clone(), a.value.at(rank)));
                let scale = match players.stats_mut(id) {
                    Some(stats) if *per_missing_health => missing_health(stats),
                    _ => 1.0,
                };
                self.buffs.push(Buff {
                    player: id,
                    stat: *stat,
                    value: value.at(rank),
                    left: duration.at(rank),
                    against,
                    scaled: *per_missing_health,
                    scale,
                    on_death: on_death.clone(),
                    rider: rider.clone(),
                    skill: skill.to_string(),
                    rank,
                    kind: *kind,
                });
            }
            Effect::Sacrifice { health } => {
                // Jamais mortel : la frappe blesse, elle ne tue pas son lanceur
                if let Some(stats) = players.stats_mut(id) {
                    stats.hp = (stats.hp - health.at(rank)).max(1.0f32.min(stats.hp));
                }
            }
            Effect::Cleanse {} => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.cleanse();
                }
                self.over_time.retain(|o| !(o.target == SkillTarget::Player(id) && o.per_second < 0.0));
            }
            Effect::Restore { mana, stamina } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.mana = (stats.mana + mana.at(rank) * power).min(stats.max_mana);
                    stats.stamina = (stats.stamina + stamina.at(rank) * power).min(stats.max_stamina);
                }
            }
            Effect::OverTime { heal, damage, duration, .. } => {
                let per_second = heal.as_ref().map_or(0.0, |h| h.at(rank) * power) - damage.as_ref().map_or(0.0, |d| d.at(rank));
                if per_second != 0.0 {
                    self.over_time.push(OverTime { caster, target: SkillTarget::Player(id), per_second, left: duration.at(rank), next: OVER_TIME_STEP, moving: 1.0, last: None, kind: DamageKind::Physical });
                }
            }
            Effect::Invisible { duration } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.invisible_time = stats.invisible_time.max(duration.at(rank));
                }
            }
            Effect::Revive { health } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.resurrect(health.at(rank));
                }
            }
            Effect::ShotGuard { arc, duration } => {
                let feet = players.feet(id).unwrap_or(Vec3::Y);
                let up = feet.normalize_or_zero();
                let aim = self.aims.get(&id).copied().unwrap_or(Vec3::ZERO);
                let flat = aim - up * aim.dot(up);
                let facing = if flat.length_squared() > 1e-8 { flat.normalize() } else { herocraft_common::tangent_frame(up).1 };
                let cos = (arc.at(rank).clamp(0.0, 360.0) * 0.5).to_radians().cos();
                self.wards.push(ShotWard { player: id, front: Some((facing, cos)), deflect: None, left: duration.at(rank) });
            }
            Effect::Deflect { size, duration } => {
                self.wards.push(ShotWard { player: id, front: None, deflect: Some(*size), left: duration.at(rank) });
            }
            Effect::Guard(guard) => {
                self.guards.retain(|g| g.player != id);
                let hits = guard.hits.as_ref().map(|h| h.at(rank).round().max(1.0) as u32);
                self.guards.push(Guarding { player: id, skill: skill.to_string(), rank, guard: guard.clone(), left: guard.duration.at(rank), hits, parried: 0, aim: Vec3::ZERO });
            }
            Effect::Immune { from, duration } => {
                self.immunities.push(Immunity { player: id, from: from.clone(), left: duration.at(rank) });
                // Ce qu'il ignore désormais tombe tout de suite
                if let Some(stats) = players.stats_mut(id) {
                    if from.contains(&Status::Stun) {
                        stats.stun_time = 0.0;
                    }
                    if from.contains(&Status::Slow) {
                        stats.slow = 0.0;
                        stats.slow_time = 0.0;
                    }
                }
            }
            Effect::Reload {} => self.arsenal.reload_now(id),
            Effect::LoadAmmo { shots, duration, radius, effects } => {
                let shots = shots.at(rank).round().max(1.0) as u32;
                self.arsenal.load(id, Special { skill: skill.to_string(), rank, shots, left: duration.at(rank), radius: radius.as_ref().map(|r| r.at(rank)), effects: effects.clone() });
            }
            // Pas de combat entre joueurs : ni dégâts, ni ralenti, ni étourdissement d'un allié (ce
            // sont les bêtes qui ralentissent et étourdissent les joueurs, `CreatureWorld::afflict`)
            Effect::BreakGuard { .. } | Effect::Damage { .. } | Effect::Shred { .. } | Effect::Slow { .. } | Effect::Stun { .. } | Effect::Knockback { .. } | Effect::Pull { .. } | Effect::Launch { .. } | Effect::Slam { .. } | Effect::Summon { .. } | Effect::Taunt { .. } | Effect::Blind { .. } | Effect::Mark { .. } => {}
        }
    }
}

/// Où arrive une téléportation près d'une cible aux pieds `target`, pour un lanceur aux pieds
/// `feet` qui visait `aim` : les pieds d'arrivée et la direction du regard
pub fn teleport_spot(teleport: &herocraft_common::skills::Teleport, rank: u8, feet: Vec3, target: Vec3, aim: Vec3, block: f32) -> (Vec3, Vec3) {
    let up = target.normalize_or(Vec3::Y);
    let flat = |v: Vec3| v - up * v.dot(up);
    let forward = flat(target - feet).try_normalize().or_else(|| flat(aim).try_normalize()).unwrap_or_else(|| up.any_orthonormal_vector());
    let right = forward.cross(up);
    let offset = forward * teleport.behind.at(rank) + up * teleport.above.at(rank) + right * teleport.side.at(rank);
    let arrival = target + offset * block;
    let look = match teleport.face {
        Facing::Target => (target + up * LOOK_AT_HEIGHT * block - (arrival + up * EYE_HEIGHT * block)).try_normalize().unwrap_or(forward),
        Facing::Aim => aim.try_normalize().unwrap_or(forward),
        Facing::Away => flat(arrival - target).try_normalize().unwrap_or(forward),
    };
    (arrival, look)
}

/// Part de sa vie qui manque au joueur (0 : pleine vie, 1 : au bord de la mort)
fn missing_health(stats: &CharacterStats) -> f32 {
    (1.0 - stats.hp / stats.max_hp.max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::{CreatureRegistry, PlayerClass};
    use std::sync::Arc;

    /// Événements datés (l'instant en secondes, puis l'événement)
    type Timed<T> = Vec<(f32, T)>;

    fn world() -> (CreatureWorld, Vec3) {
        let registry = Arc::new(CreatureRegistry::load_default());
        let base_radius = 10_000.0;
        let mut world = CreatureWorld::new(base_radius, 1, registry.clone());
        let feet = Vec3::new(0.0, base_radius as f32, 0.0);
        let species = registry.find("cow").or_else(|| registry.find("vache")).unwrap_or(0);
        let block = world.block_size();
        world.spawn(species, feet + Vec3::new(0.0, 0.0, -2.0 * block));
        world.spawn(species, feet + Vec3::new(0.0, 0.0, 20.0 * block));
        (world, feet)
    }

    #[test]
    fn test_judgment_hits_what_is_in_its_circle_at_impact() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("judgment".into(), 1);
        let mana = stats.mana;
        let mut runner = SkillRunner::new();
        assert_eq!(runner.cast(&catalog, PlayerId(7), &mut stats, "judgment", Vec3::NEG_Z, None), Ok(1));
        assert!(stats.mana < mana, "le mana est payé");
        assert_eq!(runner.cast(&catalog, PlayerId(7), &mut stats, "judgment", Vec3::NEG_Z, None), Err(CastError::Cooldown));

        let near = creatures.creatures()[0].id;
        let health = |w: &CreatureWorld, id| w.creatures().iter().find(|c| c.id == id).map(|c| c.health).unwrap();
        let before = health(&creatures, near);
        // Rien avant l'impact du geste (0,315 s)
        let mut player = SoloPlayer { id: PlayerId(7), feet, stats: &mut stats };
        let tick = runner.tick(0.2, &catalog, &mut creatures, &mut player);
        assert!(tick.impacts.is_empty());
        let tick = runner.tick(0.2, &catalog, &mut creatures, &mut player);
        assert_eq!(tick.impacts.len(), 1);
        assert_eq!(tick.impacts[0].creatures, vec![near], "seule la bête proche est dans le cercle");
        assert!(health(&creatures, near) < before);

        // La recharge passe
        for _ in 0..80 {
            runner.tick(0.1, &catalog, &mut creatures, &mut player);
        }
        assert_eq!(runner.cooldown(PlayerId(7), "judgment"), 0.0);
    }

    #[test]
    fn test_self_effects_shield_and_heal_the_caster() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("faithArmor".into(), 2);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "faithArmor", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let tick = runner.tick(1.0, &catalog, &mut creatures, &mut player);
        assert_eq!(tick.changed, vec![PlayerId(1)]);
        assert_eq!(stats.shield, 30.0);
        let hp = stats.hp;
        stats.take_hit(10.0, Some(DamageKind::Physical));
        assert_eq!((stats.hp, stats.shield), (hp, 20.0), "le bouclier encaisse");
    }

    #[test]
    fn test_summons_appear_at_the_aimed_point_and_leave_over_the_limit() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::Engineer);
        stats.skills.insert("shrapnelMine".into(), 1);
        stats.stamina = 1000.0;
        let mut runner = SkillRunner::new();
        let mut mines = Vec::new();
        for _ in 0..4 {
            runner.cast(&catalog, PlayerId(3), &mut stats, "shrapnelMine", Vec3::NEG_Z, None).unwrap();
            let mut player = SoloPlayer { id: PlayerId(3), feet, stats: &mut stats };
            let mut summoned = Vec::new();
            for _ in 0..20 {
                summoned.extend(runner.tick(0.5, &catalog, &mut creatures, &mut player).impacts.into_iter().flat_map(|i| i.summoned));
            }
            assert_eq!(summoned.len(), 1, "une mine par lancement");
            mines.extend(summoned);
        }
        // Trois mines au plus au rang 1 : la plus ancienne a disparu
        let present: Vec<herocraft_common::CreatureId> = runner.summoned_by(PlayerId(3)).collect();
        assert_eq!(present, mines[1..].to_vec());
        assert!(creatures.creatures().iter().all(|c| c.id != mines[0]));
        let mine = creatures.creatures().iter().find(|c| c.id == mines[3]).unwrap();
        assert!(mine.tamed && mine.summoner == Some(PlayerId(3)), "alliée du lanceur");
        assert!(mine.position.distance(feet) > creatures.block_size(), "posée devant, au point visé");

        // En quittant la partie, l'ingénieur emporte ses mines
        runner.forget_with(PlayerId(3), &mut creatures);
        assert!(creatures.creatures().iter().all(|c| c.summoner.is_none()));
    }

    #[test]
    fn test_every_summon_skill_is_playable() {
        let catalog = SkillCatalog::load_default();
        for id in ["skeletonServant", "deathKnight", "fleshGolem", "lichLord", "boneBehemoth", "boneWall", "ballisticTurret", "etheriumBastion", "magmaWall"] {
            let def = catalog.get(id).unwrap();
            assert_eq!(def.summons().count(), 1, "{id}");
            assert!(def.is_castable(), "{id}");
        }
    }

    #[test]
    fn test_void_legion_turns_corpses_into_shades_and_the_decoy_copies_its_thief() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let near = creatures.creatures()[0].id;
        let cow = creatures.creatures()[0].species;
        creatures.wound(near, feet, PlayerId(5), 10_000.0);
        let mut stats = CharacterStats::new(PlayerClass::Necromancer);
        stats.skills.insert("voidLegion".into(), 1);
        stats.skills.insert("shadowDecoy".into(), 1);
        stats.mana = 1000.0;
        stats.stamina = 1000.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(5), &mut stats, "voidLegion", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(5), feet, stats: &mut stats };
        let summoned: Vec<herocraft_common::CreatureId> = (0..10).flat_map(|_| runner.tick(0.2, &catalog, &mut creatures, &mut player).impacts).flat_map(|i| i.summoned).collect();
        assert_eq!(summoned.len(), 1, "une ombre par cadavre");
        assert!(creatures.creatures().iter().all(|c| c.id != near), "le cadavre est devenu l'ombre");
        let shade = creatures.creatures().iter().find(|c| c.id == summoned[0]).unwrap();
        assert_eq!(shade.copy, Some(cow as u32));
        assert_eq!(stats.shield, 0.0, "pas de bouclier avant l'implosion");
        // Au bout de leur durée (8 s au rang 1), les ombres implosent en bouclier
        let mut player = SoloPlayer { id: PlayerId(5), feet, stats: &mut stats };
        for _ in 0..40 {
            runner.tick(0.2, &catalog, &mut creatures, &mut player);
        }
        assert!(creatures.creatures().iter().all(|c| c.id != summoned[0]), "l'ombre a implosé");
        assert!(stats.shield > 0.0, "le nécromancien reçoit son bouclier à l'implosion");

        runner.cast(&catalog, PlayerId(5), &mut stats, "shadowDecoy", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(5), feet, stats: &mut stats };
        let decoy: Vec<herocraft_common::CreatureId> = (0..10).flat_map(|_| runner.tick(0.2, &catalog, &mut creatures, &mut player).impacts).flat_map(|i| i.summoned).collect();
        let decoy = creatures.creatures().iter().find(|c| c.id == decoy[0]).unwrap();
        assert_eq!((decoy.copy, decoy.summoner), (Some(herocraft_common::COPY_SUMMONER), Some(PlayerId(5))));
    }

    #[test]
    fn a_druid_form_allows_only_its_two_attacks_and_regeneration_heals() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::Druid);
        for skill in ["ancientStag", "packClaws"] {
            stats.skills.insert(skill.into(), 1);
        }
        stats.mana = 1000.0;
        stats.stamina = 1000.0;
        let mut runner = SkillRunner::new();
        assert_eq!(runner.cast(&catalog, PlayerId(5), &mut stats, "stagAntlers", Vec3::NEG_Z, None), Err(CastError::NotInForm));
        runner.cast(&catalog, PlayerId(5), &mut stats, "ancientStag", Vec3::NEG_Z, None).unwrap();
        assert_eq!(runner.form(PlayerId(5)), Some("ancientStag"));
        assert_eq!(runner.cast(&catalog, PlayerId(5), &mut stats, "packClaws", Vec3::NEG_Z, None), Err(CastError::InForm));
        runner.cast(&catalog, PlayerId(5), &mut stats, "stagAntlers", Vec3::NEG_Z, None).unwrap();
        // Le cerf régénère : la vie remonte
        stats.hp = 10.0;
        let mut player = SoloPlayer { id: PlayerId(5), feet, stats: &mut stats };
        for _ in 0..20 {
            runner.tick(0.25, &catalog, &mut creatures, &mut player);
        }
        assert!(stats.hp > 12.0, "{}", stats.hp);
        // La forme passée, plus d'attaques de cerf
        let mut player = SoloPlayer { id: PlayerId(5), feet, stats: &mut stats };
        for _ in 0..100 {
            runner.tick(0.25, &catalog, &mut creatures, &mut player);
        }
        assert_eq!(runner.form(PlayerId(5)), None);
    }

    /// Deux joueurs côte à côte
    struct Pair<'a> {
        feet: Vec3,
        a: (PlayerId, &'a mut CharacterStats),
        b: (PlayerId, &'a mut CharacterStats),
    }

    impl SkillPlayers for Pair<'_> {
        fn ids(&self) -> Vec<PlayerId> {
            vec![self.a.0, self.b.0]
        }
        fn feet(&self, id: PlayerId) -> Option<Vec3> {
            (id == self.a.0 || id == self.b.0).then_some(self.feet)
        }
        fn stats_mut(&mut self, id: PlayerId) -> Option<&mut CharacterStats> {
            if id == self.a.0 {
                Some(&mut *self.a.1)
            } else if id == self.b.0 {
                Some(&mut *self.b.1)
            } else {
                None
            }
        }
    }

    /// Avance jusqu'à la frappe du lancement en attente
    fn until_impact(runner: &mut SkillRunner, catalog: &SkillCatalog, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) {
        for _ in 0..100 {
            let struck = !runner.tick(0.02, catalog, creatures, players).impacts.is_empty();
            fly(creatures, players, 0.02);
            if struck {
                // Un vrai projectile : il vole jusqu'à son arrêt
                for _ in 0..200 {
                    if runner.flights.is_empty() {
                        break;
                    }
                    runner.tick(0.02, catalog, creatures, players);
                    fly(creatures, players, 0.02);
                }
                return;
            }
        }
        panic!("pas de frappe");
    }

    /// Les projectiles avancent (sans relief), les joueurs `players` pour cibles
    fn fly(creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, dt: f32) {
        let feet: Vec<Vec3> = players.ids().into_iter().filter_map(|id| players.feet(id)).collect();
        creatures.advance_projectiles(dt, &feet);
    }

    fn health(w: &CreatureWorld, id: CreatureId) -> f32 {
        w.creatures().iter().find(|c| c.id == id).map(|c| c.health).unwrap()
    }

    /// Un monde de vaches en file devant le lanceur (direction visée : -Z), à ces distances en blocs
    fn cows_in_line(distances: &[f32]) -> (CreatureWorld, Vec3, Vec<CreatureId>) {
        let registry = Arc::new(CreatureRegistry::load_default());
        let base_radius = 10_000.0;
        let mut world = CreatureWorld::new(base_radius, 1, registry.clone());
        let feet = Vec3::new(0.0, base_radius as f32, 0.0);
        let cow = registry.find("vache").unwrap();
        let block = world.block_size();
        let ids = distances.iter().map(|d| world.spawn(cow, feet + Vec3::new(0.0, 0.0, -d * block))).collect();
        (world, feet, ids)
    }

    fn hurt(w: &CreatureWorld, id: CreatureId) -> bool {
        w.creatures().iter().find(|c| c.id == id).is_some_and(|c| c.health < w.registry().get(c.species).unwrap().health)
    }

    #[test]
    fn an_air_blade_flies_and_stops_on_the_first_enemy() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet, cows) = cows_in_line(&[4.0, 7.0]);
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "airBlade", None);
        assert!(hurt(&creatures, cows[0]), "la première vache est touchée");
        assert!(!hurt(&creatures, cows[1]), "la lame s'arrête sur elle");
    }

    #[test]
    fn a_piercing_arrow_goes_through_every_enemy_but_not_through_a_wall() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet, cows) = cows_in_line(&[4.0, 7.0, 10.0]);
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "piercingArrow", None);
        assert!(cows.iter().all(|&id| hurt(&creatures, id)), "les trois vaches alignées sont touchées");

        // Une barricade (sauvage : du camp d'en face) devant une quatrième vache
        let block = creatures.block_size();
        let cow = creatures.registry().find("vache").unwrap();
        let behind = creatures.spawn(cow, feet + Vec3::new(3.0 * block, 0.0, -9.0 * block));
        let wall = creatures.spawn(creatures.registry().find("portableBarricade").unwrap(), feet + Vec3::new(3.0 * block, 0.0, -5.0 * block));
        creatures.creature_mut(wall).unwrap().yaw = 0.0;
        let mut runner = SkillRunner::new();
        let aim = Vec3::new(3.0, 0.0, -9.0).normalize();
        stats.skills.insert("piercingArrow".into(), 1);
        stats.stamina = 100.0;
        runner.cast(&catalog, PlayerId(1), &mut stats, "piercingArrow", aim, None).unwrap();
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 2.0);
        assert!(!hurt(&creatures, behind), "le mur arrête la flèche perforante");
        // La Frappe d'Étherium traverse tout
        stats.skills.insert("etheriumStrike".into(), 1);
        stats.stamina = 100.0;
        runner.cast(&catalog, PlayerId(1), &mut stats, "etheriumStrike", aim, None).unwrap();
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 3.0);
        assert!(hurt(&creatures, behind), "la Frappe d'Étherium passe le mur");
    }

    #[test]
    fn a_shock_volley_bounces_from_enemy_to_enemy() {
        let catalog = SkillCatalog::load_default();
        // Une vache dans l'axe, trois autres à moins de trois blocs les unes des autres, une loin
        let (mut creatures, feet, _) = cows_in_line(&[]);
        let block = creatures.block_size();
        let cow = creatures.registry().find("vache").unwrap();
        let spots = [(0.0, -6.0), (2.0, -7.0), (4.0, -6.5), (6.0, -7.5), (20.0, -7.0)];
        let cows: Vec<herocraft_common::CreatureId> = spots.iter().map(|(x, z)| creatures.spawn(cow, feet + Vec3::new(x * block, 0.0, z * block))).collect();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "shockVolley", None);
        // Au rang 1 : la première touche, puis deux rebonds
        let touched: Vec<bool> = cows.iter().map(|&id| hurt(&creatures, id)).collect();
        assert_eq!(touched, vec![true, true, true, false, false], "trois vaches touchées de proche en proche");
    }

    #[test]
    fn walls_and_wards_against_shots() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet, _) = cows_in_line(&[]);
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Mur vivant : une garde de face, dans la direction visée
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "livingWall", None);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.2);
        let guard = creatures.guards().iter().find(|g| g.key == PlayerId(1)).copied().expect("une garde");
        let (facing, cos) = guard.front.expect("de face");
        assert!(facing.dot(Vec3::NEG_Z) > 0.99 && (cos - 0.5).abs() < 1e-3, "{facing} {cos}");
        // Manteau de Zéphyr : les petits projectiles sont déviés
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "zephyrMantle", None);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.2);
        let guard = creatures.guards().iter().find(|g| g.key == PlayerId(1)).copied().unwrap();
        assert_eq!(guard.deflect, Some(herocraft_common::ProjectileSize::Small));
        // Les deux cessent avec leur durée
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 9.0);
        assert!(creatures.guards().is_empty());
        // Rempart Sacré : un mur de lumière qui arrête les tirs, devant le Chapelain
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "sacredRampart", None);
        let rampart = creatures.registry().find("sacredRampart").unwrap();
        let wall = creatures.creatures().iter().find(|c| c.species == rampart).expect("le mur est là");
        let ahead = (wall.position - feet).dot(Vec3::NEG_Z) / creatures.block_size();
        assert!((ahead - 3.0).abs() < 0.2, "à trois blocs devant : {ahead}");
    }

    #[test]
    fn purification_heals_and_removes_slow_stun_and_damage_over_time() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("purification".into(), 1);
        stats.hp = 50.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "purification", Vec3::NEG_Z, None).unwrap();
        // Ralenti, étourdi et empoisonné après le lancement, avant la frappe
        stats.apply_slow(0.5, 20.0);
        stats.apply_stun(20.0);
        runner.over_time.push(OverTime { caster: PlayerId(9), target: SkillTarget::Player(PlayerId(1)), per_second: -4.0, left: 20.0, next: OVER_TIME_STEP, moving: 1.0, last: None, kind: DamageKind::Physical });
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert!(stats.hp > 50.0, "la Purification soigne");
        assert!(!stats.is_stunned() && stats.status_speed() == 1.0, "ralenti et étourdissement retirés");
        assert!(runner.over_time.is_empty(), "le poison est retiré");
    }

    #[test]
    fn virtuous_intervention_takes_the_next_hit_whole_then_unlinks() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut paladin = CharacterStats::new(PlayerClass::WarPriest);
        paladin.skills.insert("virtuousIntervention".into(), 1);
        paladin.stamina = 100.0;
        let mut ally = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut paladin, "virtuousIntervention", Vec3::NEG_Z, None).unwrap();
        let mut pair = Pair { feet, a: (PlayerId(1), &mut paladin), b: (PlayerId(2), &mut ally) };
        until_impact(&mut runner, &catalog, &mut creatures, &mut pair);
        assert_eq!(runner.share_damage(PlayerId(2), 20.0), (0.0, Some((PlayerId(1), 20.0))), "le paladin encaisse le coup en entier");
        assert_eq!(runner.share_damage(PlayerId(2), 20.0), (20.0, None), "le lien se défait après un coup");
    }

    #[test]
    fn glimmering_guard_turns_absorbed_damage_into_mana() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("glimmeringGuard".into(), 1);
        stats.stamina = 100.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "glimmeringGuard", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        stats.mana = 10.0;
        let hp = stats.hp;
        stats.take_hit(20.0, Some(DamageKind::Magical));
        assert_eq!((stats.hp, stats.mana), (hp, 20.0), "la moitié des dégâts absorbés revient en mana");
        // Un coup physique passe à travers
        stats.take_hit(5.0, Some(DamageKind::Physical));
        assert_eq!(stats.hp, hp - 5.0, "le bouclier n'absorbe que la magie");
        // Le bouclier fini, plus de mana rendu
        stats.regenerate(10.0, false);
        assert_eq!(stats.shield_mana, 0.0);
    }

    #[test]
    fn dawn_weapon_hits_shadow_creatures_harder() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        let rampant = creatures.registry().find("rampant").unwrap();
        let block = creatures.block_size();
        creatures.spawn(rampant, feet + Vec3::new(2.0 * block, 0.0, 0.0));
        let shadow = creatures.creatures().iter().find(|c| c.species == rampant).unwrap().id;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("dawnWeapon".into(), 1);
        let mut runner = SkillRunner::new();
        assert_eq!(runner.hit_factor(PlayerId(1), shadow, &creatures), 1.0);
        runner.cast(&catalog, PlayerId(1), &mut stats, "dawnWeapon", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert!((runner.hit_factor(PlayerId(1), cow, &creatures) - 1.4).abs() < 1e-5, "bonus de dégâts ordinaire");
        assert!((runner.hit_factor(PlayerId(1), shadow, &creatures) - 1.8).abs() < 1e-5, "doublé contre une créature de l'ombre");
    }

    #[test]
    fn frenzied_mutilation_heals_per_enemy_hit_and_wounds_on_a_miss() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("frenziedMutilation".into(), 1);
        stats.stamina = 100.0;
        stats.hp = 50.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "frenziedMutilation", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert_eq!(stats.hp, 55.0, "un ennemi touché : un soin");
        // Loin de tout, la rafale manque et blesse
        let far = feet + Vec3::new(0.0, 0.0, 8.0 * creatures.block_size());
        runner.cooldowns.clear();
        runner.cast(&catalog, PlayerId(1), &mut stats, "frenziedMutilation", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet: far, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert_eq!(stats.hp, 47.0, "aucun ennemi touché : 8 points perdus");
    }

    #[test]
    fn dying_frenzy_grows_with_the_missing_health() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("dyingFrenzy".into(), 1);
        stats.stamina = 100.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "dyingFrenzy", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert!(runner.buff(PlayerId(1), BuffStat::Damage) < 0.05, "presque rien à pleine vie");
        stats.hp = stats.max_hp * 0.25;
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        runner.tick(0.01, &catalog, &mut creatures, &mut player);
        assert!((runner.buff(PlayerId(1), BuffStat::Damage) - 0.6).abs() < 0.01, "les trois quarts du bonus aux trois quarts de vie perdus");
        assert!((runner.buff(PlayerId(1), BuffStat::AttackSpeed) - 0.3).abs() < 0.01, "sa vitesse d'attaque suit aussi");
        assert_eq!(runner.buff(PlayerId(1), BuffStat::Speed), 0.0, "plus de vitesse de déplacement");
    }

    #[test]
    fn etherium_abomination_explodes_instead_of_dying() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("etheriumAbomination".into(), 1);
        stats.stamina = 100.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut stats, "etheriumAbomination", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        let before = health(&creatures, cow);
        // Il se vide de son sang jusqu'à zéro : la nova part, il survit
        stats.hp = 1.0;
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let mut novas = 0;
        let mut died = false;
        for _ in 0..40 {
            let tick = runner.tick(0.1, &catalog, &mut creatures, &mut player);
            novas += tick.impacts.len();
            died |= !tick.died.is_empty();
        }
        assert_eq!((novas, died), (1, false), "une nova, et pas de mort");
        assert!(health(&creatures, cow) <= before - 6.0 * SKILL_BASE_DAMAGE, "la nova frappe autour");
        assert!(stats.hp > 0.0 && runner.buff(PlayerId(1), BuffStat::Damage) == 0.0, "il survit, l'Abomination consumée");
        // Sans elle, le même saignement tue
        let mut plain = CharacterStats::new(PlayerClass::WarPriest);
        plain.hp = 1.0;
        runner.over_time.push(OverTime { caster: PlayerId(2), target: SkillTarget::Player(PlayerId(2)), per_second: -4.0, left: 5.0, next: OVER_TIME_STEP, moving: 1.0, last: None, kind: DamageKind::Physical });
        let mut player = SoloPlayer { id: PlayerId(2), feet, stats: &mut plain };
        let died: Vec<herocraft_common::PlayerId> = (0..10).flat_map(|_| runner.tick(0.1, &catalog, &mut creatures, &mut player).died).collect();
        assert_eq!(died, vec![PlayerId(2)]);
    }

    #[test]
    fn condemnation_turns_a_kill_into_a_heal_and_hallowed_hammer_heals_on_each_attack() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("condemnation".into(), 1);
        stats.skills.insert("hallowedHammer".into(), 1);
        stats.stamina = 100.0;
        let mut runner = SkillRunner::new();
        // Le coup ne tue pas : pas de soin
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        stats.hp = 20.0;
        runner.cast(&catalog, PlayerId(1), &mut stats, "condemnation", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert!(stats.hp < 25.0, "la cible survit : pas d'explosion de soin");
        // Marteau Consacré : chaque attaque soigne autour de la cible
        runner.cast(&catalog, PlayerId(1), &mut stats, "hallowedHammer", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        stats.hp = 20.0;
        let at = creatures.creatures()[0].position;
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        assert_eq!(runner.ride(PlayerId(1), &[at], &mut creatures, &mut player), vec![PlayerId(1)]);
        assert_eq!(stats.hp, 26.0, "un coup d'arme soigne les alliés proches de la cible");
        // Le coup tue : le cadavre éclate en soin
        creatures.creature_mut(cow).unwrap().health = 1.0;
        runner.cooldowns.clear();
        runner.buffs.clear();
        stats.hp = 20.0;
        runner.cast(&catalog, PlayerId(1), &mut stats, "condemnation", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        until_impact(&mut runner, &catalog, &mut creatures, &mut player);
        assert!(stats.hp >= 50.0, "30 points rendus autour du cadavre");
    }

    /// Lance `skill` (rang 1) au point `aimed` et avance jusqu'à sa frappe
    fn cast_at(runner: &mut SkillRunner, catalog: &SkillCatalog, creatures: &mut CreatureWorld, stats: &mut CharacterStats, feet: Vec3, skill: &str, aimed: Option<Vec3>) {
        stats.skills.insert(skill.into(), 1);
        stats.mana = 100.0;
        stats.stamina = 100.0;
        runner.cast(catalog, PlayerId(1), stats, skill, Vec3::NEG_Z, aimed).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats };
        until_impact(runner, catalog, creatures, &mut player);
    }

    /// Avance de `seconds`, le joueur 1 aux pieds `feet`
    fn pass(runner: &mut SkillRunner, catalog: &SkillCatalog, creatures: &mut CreatureWorld, stats: &mut CharacterStats, feet: Vec3, seconds: f32) {
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats };
        for _ in 0..(seconds / 0.1).round() as usize {
            runner.tick(0.1, catalog, creatures, &mut player);
            fly(creatures, &mut player, 0.1);
        }
    }

    #[test]
    fn a_miasma_cloud_stays_and_catches_those_who_enter_later() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let spot = feet + Vec3::new(8.0 * block, 0.0, 0.0);
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Le nuage est posé loin de la bête : elle n'a rien
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "miasmaCloud", Some(spot));
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert_eq!(health(&creatures, cow), 1000.0);
        // Elle y entre : le nuage la ronge
        creatures.creature_mut(cow).unwrap().position = spot;
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        let inside = health(&creatures, cow);
        assert!(inside < 1000.0, "entrée après la frappe, elle est touchée");
        // Elle en sort : plus rien, alors que le nuage dure encore
        creatures.creature_mut(cow).unwrap().position = feet + Vec3::new(0.0, 0.0, -30.0 * block);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        let left = health(&creatures, cow);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 2.0);
        assert_eq!(health(&creatures, cow), left, "sortie du nuage, elle ne souffre plus");
        assert_eq!(runner.zones().count(), 1);
        // Le nuage se dissipe au bout de ses 8 secondes
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 4.0);
        assert_eq!(runner.zones().count(), 0);
    }

    #[test]
    fn a_sanctuary_burns_enemies_and_heals_allies_inside() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "sanctuary", Some(feet));
        stats.hp = 10.0;
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 2.0);
        assert!(stats.hp >= 13.0, "soigné dans le sanctuaire : {}", stats.hp);
        assert!(health(&creatures, cow) < 1000.0);
        // Sorti du sanctuaire, le soin cesse
        let away = feet + Vec3::new(30.0 * creatures.block_size(), 0.0, 0.0);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 1.0);
        let hp = stats.hp;
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 2.0);
        assert_eq!(stats.hp, hp);
    }

    #[test]
    fn a_soothing_mist_hides_its_caster_only_while_inside() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Le mélange se lance par la Fusion : ici la frappe est posée telle quelle
        runner.pending.push(Pending { caster: PlayerId(1), skill: "soothingMist".into(), rank: 1, aim: Vec3::NEG_Z, aimed: None, left: 0.0, crit: false, power: 1.0, size: 1.0, kind: DamageKind::Physical });
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.1);
        assert!(stats.is_invisible());
        // Le brouillard le recache tant qu'il y reste
        for _ in 0..30 {
            stats.tick_status(0.1);
            pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.1);
        }
        assert!(stats.is_invisible(), "caché tant qu'il est dans le brouillard");
        // Il en sort : le brouillard reste où il a été posé
        let away = feet + Vec3::new(30.0 * creatures.block_size(), 0.0, 0.0);
        for _ in 0..15 {
            stats.tick_status(0.1);
            pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 0.1);
        }
        assert!(!stats.is_invisible(), "dévoilé hors du brouillard");
    }

    #[test]
    fn a_vigor_aura_follows_its_caster_and_speeds_stamina_recovery() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "vigorAura", None);
        // L'aura suit le paladin, même loin d'où il l'a lancée
        let away = feet + Vec3::new(30.0 * creatures.block_size(), 0.0, 0.0);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 3.0);
        stats.stamina = 0.0;
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 2.0);
        assert!((stats.stamina - 6.0).abs() < 0.5, "+30 % de la récupération au repos : {}", stats.stamina);
        // L'aura finie, plus rien
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 12.0);
        stats.stamina = 0.0;
        pass(&mut runner, &catalog, &mut creatures, &mut stats, away, 1.0);
        assert_eq!(stats.stamina, 0.0);
    }

    #[test]
    fn a_soul_storm_heals_the_casters_summons_inside() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let skeleton = creatures.registry().find("skeletonServant").unwrap();
        let mine = creatures.summon(skeleton, feet + Vec3::new(block, 0.0, 0.0), 0.0, PlayerId(1));
        let theirs = creatures.summon(skeleton, feet + Vec3::new(-block, 0.0, 0.0), 0.0, PlayerId(2));
        creatures.creature_mut(mine).unwrap().health = 5.0;
        creatures.creature_mut(theirs).unwrap().health = 5.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "soulStorm", None);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert!(health(&creatures, mine) > 5.0, "l'invocation du lanceur est soignée");
        assert_eq!(health(&creatures, theirs), 5.0, "pas celle d'un autre");
    }

    #[test]
    fn a_magma_wall_leaves_a_lava_pool_that_burns_who_crosses_it() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let spot = feet + Vec3::new(5.0 * block, 0.0, 0.0);
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Le mélange se lance par la Fusion : ici la frappe est posée telle quelle
        runner.pending.push(Pending { caster: PlayerId(1), skill: "magmaWall".into(), rank: 1, aim: Vec3::NEG_Z, aimed: Some(spot), left: 0.0, crit: false, power: 1.0, size: 1.0, kind: DamageKind::Physical });
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.5);
        assert_eq!(runner.summoned_by(PlayerId(1)).count(), 1, "le mur est là");
        creatures.creature_mut(cow).unwrap().position = spot + Vec3::new(block, 0.0, 0.0);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert!(health(&creatures, cow) < 1000.0, "la lave brûle qui la traverse");
    }

    #[test]
    fn a_telluric_rampart_marches_forward_then_stops() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Donné par le nœud de la Terre : ici la frappe est posée telle quelle
        runner.pending.push(Pending { caster: PlayerId(1), skill: "telluricRampart".into(), rank: 1, aim: Vec3::NEG_Z, aimed: None, left: 0.0, crit: false, power: 1.0, size: 1.0, kind: DamageKind::Physical });
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 0.1);
        let wall = runner.summoned_by(PlayerId(1)).next().expect("le mur");
        let at = |c: &CreatureWorld| c.creatures().iter().find(|x| x.id == wall).unwrap().position;
        let start = at(&creatures);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 3.0);
        let moved = (at(&creatures) - start).dot(Vec3::NEG_Z) / block;
        assert!((moved - 6.0).abs() < 0.3, "le mur avance de six blocs vers la visée : {moved}");
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert!(((at(&creatures) - start).dot(Vec3::NEG_Z) / block - moved).abs() < 0.01, "puis il s'arrête");
    }

    #[test]
    fn mechanical_overload_heals_shields_and_hastes_only_the_casters_turrets() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let turret = creatures.registry().find("ballisticTurret").unwrap();
        let mine = creatures.summon(turret, feet + Vec3::new(3.0 * block, 0.0, 0.0), 0.0, PlayerId(1));
        let theirs = creatures.summon(turret, feet + Vec3::new(-3.0 * block, 0.0, 0.0), 0.0, PlayerId(2));
        creatures.creature_mut(mine).unwrap().health = 10.0;
        creatures.creature_mut(theirs).unwrap().health = 10.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.hp = 50.0;
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "mechanicalOverload", None);
        assert_eq!(health(&creatures, mine), 40.0, "la tourelle du lanceur est réparée");
        assert_eq!(health(&creatures, theirs), 10.0, "pas celle d'un autre");
        assert_eq!(stats.hp, 50.0, "plus de soin aux joueurs");
        let c = creatures.creatures().iter().find(|c| c.id == mine).unwrap();
        assert!(c.is_hasted() && c.has_shield(), "pressée et protégée");
    }

    #[test]
    fn traps_dropped_by_an_eradication_trap_vanish_if_nothing_triggers_them() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        let mine = creatures.registry().find("shrapnelMine").unwrap();
        let id = creatures.summon(mine, feet + Vec3::new(0.0, 0.0, 40.0 * creatures.block_size()), 0.0, PlayerId(1));
        creatures.spawned.push((PlayerId(1), id, 2.0));
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert_eq!(runner.summoned_by(PlayerId(1)).collect::<Vec<_>>(), vec![id]);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.5);
        assert_eq!(runner.summoned_by(PlayerId(1)).count(), 0);
    }

    #[test]
    fn the_first_attack_out_of_invisibility_is_critical_and_reveals() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::Rogue);
        stats.skills.insert("shadowVeil".into(), 1);
        stats.skills.insert("ambush".into(), 1);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(4), &mut stats, "shadowVeil", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(4), feet, stats: &mut stats });
        assert!(stats.is_invisible(), "le voile ne rend pas invisible");

        runner.cast(&catalog, PlayerId(4), &mut stats, "ambush", Vec3::NEG_Z, None).unwrap();
        assert!(!stats.is_invisible(), "l'attaque ne dévoile pas");
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(4), feet, stats: &mut stats });
        let expected = 2.4 * SKILL_BASE_DAMAGE * CRITICAL_FACTOR;
        assert!((1000.0 - health(&creatures, cow) - expected).abs() < 0.01, "pas de critique : {} dégâts", 1000.0 - health(&creatures, cow));
    }

    #[test]
    fn marks_and_critical_buffs_raise_the_critical_chance() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::Archer);
        stats.skills.insert("deadlyHunt".into(), 1);
        stats.skills.insert("hawkEye".into(), 1);
        let mut runner = SkillRunner::new();
        // Chasse mortelle : le point faible est toujours trouvé
        runner.cast(&catalog, PlayerId(2), &mut stats, "deadlyHunt", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(2), feet, stats: &mut stats });
        let expected = 1.8 * SKILL_BASE_DAMAGE * CRITICAL_FACTOR;
        assert!((1000.0 - health(&creatures, cow) - expected).abs() < 0.01, "pas de critique : {} dégâts", 1000.0 - health(&creatures, cow));

        runner.tick(1.0, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(2), feet, stats: &mut stats });
        assert_eq!(runner.critical_chance(PlayerId(2), cow), 0.0, "la marque de la chasse ne dure pas");
        runner.cast(&catalog, PlayerId(2), &mut stats, "hawkEye", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(2), feet, stats: &mut stats });
        assert!((runner.critical_chance(PlayerId(2), cow) - 0.3).abs() < 1e-6, "l'œil de faucon : {}", runner.critical_chance(PlayerId(2), cow));

        // Sur mille tirages à 30 %, à peu près trois cents critiques
        let crits = (0..1000).filter(|_| runner.roll() < 0.3).count();
        assert!((240..360).contains(&crits), "{crits} critiques");
    }

    #[test]
    fn a_tracer_round_marks_and_a_radiant_strike_blinds() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::Ranger);
        stats.skills.insert("tracerRound".into(), 1);
        stats.skills.insert("radiantStrike".into(), 1);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(3), &mut stats, "tracerRound", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(3), feet, stats: &mut stats });
        assert!(runner.critical_chance(PlayerId(3), cow) > 0.0, "la balle traçante ne marque pas");
        assert_eq!(runner.critical_chance(PlayerId(4), cow), 0.0, "la marque ne sert que son tireur");

        runner.cast(&catalog, PlayerId(3), &mut stats, "radiantStrike", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(3), feet, stats: &mut stats });
        assert!(creatures.is_blind(cow), "la frappe radieuse n'aveugle pas");
    }

    #[test]
    fn supreme_intervention_raises_the_fallen_ally_with_half_his_life() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut chaplain = CharacterStats::new(PlayerClass::Chaplain);
        chaplain.skills.insert("supremeIntervention".into(), 1);
        chaplain.mana = 1000.0;
        let mut ally = CharacterStats::new(PlayerClass::Soldier);
        // Tombé : le serveur l'a déjà ranimé, il attend sur son écran de mort
        ally.fallen = true;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut chaplain, "supremeIntervention", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut Pair { feet, a: (PlayerId(1), &mut chaplain), b: (PlayerId(2), &mut ally) });
        assert!(!ally.fallen && ally.revived == 1, "l'allié n'est pas relevé");
        assert!((ally.hp - 0.5 * ally.max_hp).abs() < 0.01, "relevé avec {} / {}", ally.hp, ally.max_hp);
        assert_eq!(chaplain.revived, 0, "le chapelain debout n'est pas « relevé »");
    }

    #[test]
    fn a_stunned_player_cannot_cast() {
        let catalog = SkillCatalog::load_default();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("judgment".into(), 1);
        stats.apply_stun(1.0);
        let mut runner = SkillRunner::new();
        assert_eq!(runner.cast(&catalog, PlayerId(7), &mut stats, "judgment", Vec3::NEG_Z, None), Err(CastError::Stunned));
        stats.tick_status(1.5);
        assert_eq!(runner.cast(&catalog, PlayerId(7), &mut stats, "judgment", Vec3::NEG_Z, None), Ok(1));
    }
    #[test]
    fn fusion_mixes_the_last_two_spells_or_amplifies_the_same_element() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        let at = creatures.creatures()[0].position;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::Sorcerer);
        for skill in ["fire", "water", "fusion"] {
            stats.skills.insert(skill.into(), 1);
        }
        stats.mana = 10_000.0;
        let mut runner = SkillRunner::new();
        let run = |runner: &mut SkillRunner, stats: &mut CharacterStats, creatures: &mut CreatureWorld| {
            until_impact(runner, &catalog, creatures, &mut SoloPlayer { id: PlayerId(6), feet, stats });
            runner.tick(6.0, &catalog, creatures, &mut SoloPlayer { id: PlayerId(6), feet, stats });
        };
        assert_eq!(runner.cast(&catalog, PlayerId(6), &mut stats, "fusion", Vec3::NEG_Z, Some(at)), Err(CastError::FusionEmpty));
        assert_eq!(runner.cast(&catalog, PlayerId(6), &mut stats, "scaldingTyphoon", Vec3::NEG_Z, Some(at)), Err(CastError::FusionOnly));

        // Feu puis eau : le Typhon Bouillant, s'il est appris
        runner.cast(&catalog, PlayerId(6), &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        runner.cast(&catalog, PlayerId(6), &mut stats, "foamingWhip", Vec3::NEG_Z, None).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        assert_eq!(runner.cast(&catalog, PlayerId(6), &mut stats, "fusion", Vec3::NEG_Z, Some(at)), Err(CastError::FusionLocked));
        stats.skills.insert("scaldingTyphoon".into(), 1);
        let casting = runner.cast_as(&catalog, PlayerId(6), &mut stats, "fusion", Vec3::NEG_Z, Some(at)).unwrap();
        assert_eq!((casting.skill.as_str(), casting.amplified), ("scaldingTyphoon", false));
        run(&mut runner, &mut stats, &mut creatures);
        assert!(runner.recent_spells(PlayerId(6)).is_empty(), "la Fusion consomme ses deux sorts");

        // Deux orbes : la seconde relancée amplifiée
        runner.cast(&catalog, PlayerId(6), &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        runner.cast(&catalog, PlayerId(6), &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        let before = health(&creatures, cow);
        let mana = stats.mana;
        let casting = runner.cast_as(&catalog, PlayerId(6), &mut stats, "fusion", Vec3::NEG_Z, Some(at)).unwrap();
        assert_eq!((casting.skill.as_str(), casting.amplified), ("igneousOrb", true));
        assert!((mana - stats.mana - 12.0 * 1.5).abs() < 0.01, "coût amplifié : {}", mana - stats.mana);
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(6), feet, stats: &mut stats });
        let expected = 1.3 * SKILL_BASE_DAMAGE * 1.75;
        assert!((before - health(&creatures, cow) - expected).abs() < 0.01, "dégâts amplifiés : {}", before - health(&creatures, cow));
        assert!(runner.cooldown(PlayerId(6), "fusion") > 0.0, "la Fusion a sa propre recharge");
    }

    #[test]
    fn martyrs_mark_shares_the_allys_damage_with_the_chaplain() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut chaplain = CharacterStats::new(PlayerClass::Chaplain);
        chaplain.skills.insert("martyrsMark".into(), 1);
        let mut ally = CharacterStats::new(PlayerClass::Soldier);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(1), &mut chaplain, "martyrsMark", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut Pair { feet, a: (PlayerId(1), &mut chaplain), b: (PlayerId(2), &mut ally) });
        let (kept, shared) = runner.share_damage(PlayerId(2), 10.0);
        assert!((kept - 6.0).abs() < 1e-4 && shared == Some((PlayerId(1), 4.0)), "{kept} {shared:?}");
        assert_eq!(runner.share_damage(PlayerId(1), 10.0), (10.0, None), "le chapelain ne se lie pas à lui-même");
    }

    #[test]
    fn a_detonating_mark_seeks_the_marked_beast_and_hawk_eye_reaches_further() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let block = creatures.block_size();
        let mut stats = CharacterStats::new(PlayerClass::Ranger);
        stats.skills.insert("tracerRound".into(), 1);
        stats.skills.insert("detonatingMark".into(), 1);
        stats.mana = 1000.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, PlayerId(3), &mut stats, "tracerRound", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(3), feet, stats: &mut stats });
        let before = health(&creatures, cow);
        // Visé loin de la vache, à dix blocs sur le côté : le tir la retrouve
        runner.cast(&catalog, PlayerId(3), &mut stats, "detonatingMark", Vec3::X, Some(feet + Vec3::X * 10.0 * block)).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(3), feet, stats: &mut stats });
        assert!(health(&creatures, cow) < before, "le tir n'a pas cherché la bête marquée");

        let mut archer = CharacterStats::new(PlayerClass::Archer);
        archer.skills.insert("hawkEye".into(), 1);
        runner.cast(&catalog, PlayerId(4), &mut archer, "hawkEye", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(4), feet, stats: &mut archer });
        assert!((runner.buff(PlayerId(4), BuffStat::Range) - 0.3).abs() < 1e-6, "l'œil de faucon n'allonge pas la portée");
    }

    #[test]
    fn deadly_hunt_bleeds_harder_while_the_beast_moves() {
        let catalog = SkillCatalog::load_default();
        let bled = |moves: bool| {
            let (mut creatures, feet) = world();
            let cow = creatures.creatures()[0].id;
            creatures.creature_mut(cow).unwrap().health = 1000.0;
            let block = creatures.block_size();
            let mut stats = CharacterStats::new(PlayerClass::Archer);
            stats.skills.insert("deadlyHunt".into(), 1);
            let mut runner = SkillRunner::new();
            runner.cast(&catalog, PlayerId(2), &mut stats, "deadlyHunt", Vec3::NEG_Z, None).unwrap();
            until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(2), feet, stats: &mut stats });
            let after_hit = health(&creatures, cow);
            for _ in 0..20 {
                if moves {
                    creatures.creature_mut(cow).unwrap().position += Vec3::X * 0.5 * block;
                }
                runner.tick(0.25, &catalog, &mut creatures, &mut SoloPlayer { id: PlayerId(2), feet, stats: &mut stats });
            }
            after_hit - health(&creatures, cow)
        };
        let (still, moving) = (bled(false), bled(true));
        assert!(still > 0.0 && moving > 2.0 * still, "immobile {still}, en mouvement {moving}");
    }

    /// Lance `skill` (rang 1) et avance de `seconds` par pas de 0,02 s : les frappes et les
    /// déplacements rendus, avec l'instant où ils arrivent
    fn run_moving(runner: &mut SkillRunner, catalog: &SkillCatalog, creatures: &mut CreatureWorld, stats: &mut CharacterStats, feet: Vec3, skill: &str, seconds: f32) -> (Timed<SkillImpact>, Timed<SkillMove>) {
        stats.skills.insert(skill.into(), 1);
        stats.mana = 100.0;
        stats.stamina = 100.0;
        runner.cast(catalog, PlayerId(1), stats, skill, Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats };
        let (mut impacts, mut moves) = (Vec::new(), Vec::new());
        for i in 1..=(seconds / 0.02).round() as usize {
            fly(creatures, &mut player, 0.02);
            let tick = runner.tick(0.02, catalog, creatures, &mut player);
            let t = i as f32 * 0.02;
            impacts.extend(tick.impacts.into_iter().map(|x| (t, x)));
            moves.extend(tick.moves.into_iter().map(|m| (t, m.1)));
        }
        (impacts, moves)
    }

    #[test]
    fn phantom_shot_teleports_behind_the_target_and_faces_it() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let cow = creatures.creatures()[0].position;
        let mut stats = CharacterStats::new(PlayerClass::Rogue);
        let mut runner = SkillRunner::new();
        let (impacts, moves) = run_moving(&mut runner, &catalog, &mut creatures, &mut stats, feet, "phantomShot", 1.0);
        // Le tir part, puis touche la vache
        let impacts: Vec<_> = impacts.into_iter().filter(|i| i.1.contact).collect();
        assert_eq!(impacts.len(), 1);
        let [(t, SkillMove::Teleport { feet: arrival, look })] = moves.as_slice() else { panic!("une téléportation : {moves:?}") };
        assert!((*t - impacts[0].0).abs() < 1e-3, "à l'impact");
        // 1,5 bloc au-delà de la vache, vue depuis le lanceur (droit devant, vers -Z)
        assert!(arrival.distance(cow + Vec3::NEG_Z * 1.5 * block) < 0.05 * block, "arrivée {arrival}, vache {cow}");
        assert!(look.z > 0.8, "regard vers la vache : {look}");

        // Rien de touché : pas de téléportation
        let (mut creatures, feet) = world();
        let mut runner = SkillRunner::new();
        stats.skills.insert("phantomShot".into(), 1);
        stats.stamina = 100.0;
        runner.cast(&catalog, PlayerId(1), &mut stats, "phantomShot", Vec3::X, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let moved = (0..50).any(|_| !runner.tick(0.02, &catalog, &mut creatures, &mut player).moves.is_empty());
        assert!(!moved);
    }

    #[test]
    fn a_grapple_bolt_needs_an_aimed_block_and_costs_nothing_without_one() {
        let catalog = SkillCatalog::load_default();
        let mut stats = CharacterStats::new(PlayerClass::Rogue);
        stats.skills.insert("grappleBolt".into(), 1);
        let stamina = stats.stamina;
        let mut runner = SkillRunner::new();
        assert_eq!(runner.cast(&catalog, PlayerId(1), &mut stats, "grappleBolt", Vec3::NEG_Z, None), Err(CastError::NothingToGrab));
        assert_eq!(stats.stamina, stamina, "rien n'est payé");
        assert_eq!(runner.cooldown(PlayerId(1), "grappleBolt"), 0.0);
        assert!(runner.cast(&catalog, PlayerId(1), &mut stats, "grappleBolt", Vec3::NEG_Z, Some(Vec3::new(0.0, 10_000.0, -50.0))).is_ok());
    }

    #[test]
    fn celestial_execution_launches_teleports_above_then_slams() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let id = creatures.creatures()[0].id;
        let cow = creatures.creatures()[0].position;
        let mut stats = CharacterStats::new(PlayerClass::WindBlade);
        let mut runner = SkillRunner::new();
        let (impacts, moves) = run_moving(&mut runner, &catalog, &mut creatures, &mut stats, feet, "celestialExecution", 1.5);
        assert!(creatures.is_airborne(id), "la vache est projetée");
        // Deux frappes : la projection, puis la seconde, 0,6 s plus tard, sur la même vache
        assert_eq!(impacts.len(), 2, "{impacts:?}");
        assert!((impacts[1].0 - impacts[0].0 - 0.6).abs() < 0.03);
        assert_eq!(impacts[1].1.creatures, vec![id]);
        // Téléporté 0,5 s après la frappe, un bloc au-dessus d'elle, le regard vers le bas
        let [(t, SkillMove::Teleport { feet: arrival, look })] = moves.as_slice() else { panic!("une téléportation : {moves:?}") };
        assert!((*t - impacts[0].0 - 0.5).abs() < 0.03);
        assert!(arrival.distance(cow + cow.normalize() * block) < 0.05 * block);
        assert!(look.dot(cow.normalize()) < -0.5, "regard vers la vache, en dessous : {look}");
    }

    #[test]
    fn lightning_assault_pulls_the_hunter_to_the_target_then_strikes() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let id = creatures.creatures()[0].id;
        let cow = creatures.creatures()[0].position;
        let before = health(&creatures, id);
        let mut stats = CharacterStats::new(PlayerClass::MonsterHunter);
        let mut runner = SkillRunner::new();
        let (impacts, moves) = run_moving(&mut runner, &catalog, &mut creatures, &mut stats, feet, "lightningAssault", 1.5);
        let [(_, SkillMove::Pull { to, speed, stop })] = moves.as_slice() else { panic!("une traction : {moves:?}") };
        assert_eq!((*to, *speed, *stop), (cow, 30.0, 1.2));
        // La frappe plongeante, 0,35 s après la traction : elle blesse et étourdit
        assert_eq!(impacts.len(), 2, "{impacts:?}");
        assert!((impacts[1].0 - impacts[0].0 - 0.35).abs() < 0.03);
        assert_eq!(impacts[1].1.creatures, vec![id]);
        assert!(health(&creatures, id) < before);
    }

    #[test]
    fn a_teleport_spot_follows_the_line_from_caster_to_target() {
        use herocraft_common::skills::{Ranked, Teleport};
        let up = Vec3::Y;
        let target = up * 100.0;
        let feet = target + Vec3::X * 5.0;
        let side = Teleport { behind: Ranked::One(-1.0), above: Ranked::One(0.0), side: Ranked::One(2.0), face: Facing::Away, delay: Ranked::One(0.0) };
        let (arrival, look) = teleport_spot(&side, 1, feet, target, Vec3::NEG_X, 1.0);
        // Entre la cible et le lanceur (un bloc vers lui), deux blocs sur la droite de la cible
        let right = Vec3::NEG_X.cross(up);
        assert!(arrival.distance(target + Vec3::X + right * 2.0) < 1e-4, "{arrival}");
        assert!(look.dot(arrival - target) > 0.0, "dos à la cible");
    }

    /// Lance `skill` au rang 1, regard vers `aim`, et avance jusqu'à sa frappe
    fn cast_and_strike(runner: &mut SkillRunner, catalog: &SkillCatalog, creatures: &mut CreatureWorld, feet: Vec3, stats: &mut CharacterStats, skill: &str, aim: Vec3) {
        stats.skills.insert(skill.into(), 1);
        stats.stamina = 1000.0;
        stats.mana = 1000.0;
        runner.cast(catalog, PlayerId(1), stats, skill, aim, None).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats };
        for _ in 0..40 {
            if !runner.tick(0.05, catalog, creatures, &mut player).impacts.is_empty() {
                return;
            }
        }
        panic!("{skill} n'a pas frappé");
    }

    #[test]
    fn a_riposte_parries_the_first_blow_from_the_front_and_strikes_back() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let near = creatures.creatures()[0].id;
        let toward = (creatures.position(near).unwrap() - feet).normalize();
        let mut stats = CharacterStats::new(PlayerClass::Champion);
        let mut runner = SkillRunner::new();
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "perfectRiposte", toward);
        assert_eq!(runner.guarding(PlayerId(1)), Some("perfectRiposte"));
        let health = |w: &CreatureWorld| w.creatures().iter().find(|c| c.id == near).map(|c| c.health).unwrap();
        let before = health(&creatures);

        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let parry = runner.parry(PlayerId(1), Some(toward), near, 10.0, "physical", &mut creatures, &mut player);
        assert!(parry.parried && parry.damage == 0.0, "le coup de face est paré en entier");
        let impact = parry.impact.expect("la riposte part");
        assert!(impact.parried && impact.creatures.contains(&near), "la riposte touche l'attaquant");
        assert!(health(&creatures) < before);
        // Un seul coup paré : le suivant passe
        let again = runner.parry(PlayerId(1), Some(toward), near, 10.0, "physical", &mut creatures, &mut player);
        assert!(!again.parried && again.damage == 10.0);
        assert_eq!(runner.guarding(PlayerId(1)), None);
    }

    #[test]
    fn a_frontal_guard_does_not_stop_a_blow_from_behind_nor_magic() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let near = creatures.creatures()[0].id;
        let toward = (creatures.position(near).unwrap() - feet).normalize();
        let mut stats = CharacterStats::new(PlayerClass::Champion);
        let mut runner = SkillRunner::new();
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "ironGuard", toward);
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let back = runner.parry(PlayerId(1), Some(-toward), near, 10.0, "physical", &mut creatures, &mut player);
        assert!(!back.parried && back.damage == 10.0, "le dos n'est pas gardé");
        let magic = runner.parry(PlayerId(1), Some(toward), near, 10.0, "magical", &mut creatures, &mut player);
        assert!(!magic.parried, "la Garde de fer ne bloque que le physique");
        let front = runner.parry(PlayerId(1), Some(toward), near, 10.0, "physical", &mut creatures, &mut player);
        assert!(front.parried && front.damage == 0.0 && front.impact.is_none(), "bloqué, sans riposte");
        assert_eq!(runner.guarding(PlayerId(1)), Some("ironGuard"), "elle pare jusqu'à la fin de sa fenêtre");
    }

    #[test]
    fn a_parry_that_parries_nothing_doubles_its_cooldown() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::Champion);
        let mut runner = SkillRunner::new();
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "perfectRiposte", Vec3::NEG_Z);
        let left = runner.cooldown(PlayerId(1), "perfectRiposte");
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let mut missed = Vec::new();
        for _ in 0..20 {
            missed.extend(runner.tick(0.1, &catalog, &mut creatures, &mut player).missed);
        }
        assert_eq!(missed, vec![(PlayerId(1), "perfectRiposte".to_string(), 10.0)]);
        let now = runner.cooldown(PlayerId(1), "perfectRiposte");
        assert!((now - (left - 2.0 + 10.0)).abs() < 0.01, "dix secondes de plus : {left} puis {now}");

        // La Garde de fer, qui ne riposte pas, garde sa recharge
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "ironGuard", Vec3::NEG_Z);
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let missed: Vec<_> = (0..50).flat_map(|_| runner.tick(0.1, &catalog, &mut creatures, &mut player).missed).collect();
        assert!(missed.is_empty());
    }

    #[test]
    fn immunities_lift_and_ignore_stun_knockback_and_damage() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::Champion);
        let mut runner = SkillRunner::new();
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "phalanx", Vec3::NEG_Z);
        assert!(runner.immune(PlayerId(1), Status::Stun) && runner.immune(PlayerId(1), Status::Knockback));
        assert!(!runner.immune(PlayerId(1), Status::Damage));

        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "divineAegis", Vec3::NEG_Z);
        assert_eq!(runner.damage_taken_factor(PlayerId(1), DamageKind::Physical), 0.0, "l'Égide rend insensible aux dégâts");
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        for _ in 0..30 {
            runner.tick(0.1, &catalog, &mut creatures, &mut player);
        }
        assert!(runner.damage_taken_factor(PlayerId(1), DamageKind::Physical) > 0.0, "puis l'immunité passe");

        // Inarrêtable : l'étourdissement en cours tombe
        stats.apply_stun(5.0);
        stats.skills.insert("voidWarmaster".into(), 1);
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        runner.apply_to_player(&EffectEntry { effect: Effect::Immune { from: vec![Status::Stun], duration: herocraft_common::skills::Ranked::One(3.0) }, to: None }, PlayerId(1), 1, 1.0, "voidWarmaster", PlayerId(1), &mut player);
        assert!(!stats.is_stunned());
        let immune = runner.immunities(PlayerId(1));
        assert!(immune.contains(&Status::Stun) && immune.contains(&Status::Knockback));
    }

    #[test]
    fn guard_breaker_breaks_the_guard_of_a_village_guard() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let species = creatures.registry().find("garde").expect("garde");
        let at = feet + Vec3::new(2.0 * block, 0.0, 0.0);
        creatures.spawn(species, at);
        let guard = creatures.creatures().iter().find(|c| c.species == species).map(|c| c.id).unwrap();
        assert!(creatures.guarding(guard));
        // De face, la garde retire sa part ; de dos, rien
        let facing = herocraft_common::yaw_direction(at, creatures.creatures().iter().find(|c| c.id == guard).unwrap().yaw);
        assert!((creatures.guard_against(guard, at + facing * block) - 0.6).abs() < 1e-4);
        assert_eq!(creatures.guard_against(guard, at - facing * block), 0.0);

        let mut stats = CharacterStats::new(PlayerClass::Soldier);
        let mut runner = SkillRunner::new();
        cast_and_strike(&mut runner, &catalog, &mut creatures, feet, &mut stats, "guardBreaker", Vec3::X);
        assert!(!creatures.guarding(guard), "Brise-Garde la brise");
        assert_eq!(creatures.guard_against(guard, at + facing * block), 0.0);
    }

    /// Une bête de l'espèce `species` aux pieds du joueur, très solide pour compter les dégâts
    fn armored(creatures: &mut CreatureWorld, feet: Vec3, species: &str) -> CreatureId {
        let index = creatures.registry().find(species).unwrap();
        let block = creatures.block_size();
        creatures.spawn(index, feet + Vec3::new(0.0, 0.0, -2.0 * block));
        let id = creatures.creatures().iter().rev().find(|c| c.species == index).unwrap().id;
        creatures.creature_mut(id).unwrap().health = 10_000.0;
        id
    }

    /// Dégâts que prend la bête `id` d'un coup de 10 points de type `kind` qui perce `pierce`
    fn blow(creatures: &mut CreatureWorld, id: CreatureId, feet: Vec3, kind: DamageKind, pierce: f32) -> f32 {
        let before = health(creatures, id);
        creatures.wound_with(id, feet, PlayerId(1), 10.0, kind, pierce);
        let after = health(creatures, id);
        creatures.creature_mut(id).unwrap().health = 10_000.0;
        before - after
    }

    #[test]
    fn armor_reduces_physical_blows_and_resistance_magical_ones() {
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        assert_eq!(blow(&mut creatures, cow, feet, DamageKind::Physical, 0.0), 10.0, "une vache n'a pas d'armure");
        // Chevalier de la mort : armure 80, résistance 20
        let knight = armored(&mut creatures, feet, "deathKnight");
        assert!((blow(&mut creatures, knight, feet, DamageKind::Physical, 0.0) - 1000.0 / 180.0).abs() < 1e-3);
        assert!((blow(&mut creatures, knight, feet, DamageKind::Magical, 0.0) - 1000.0 / 120.0).abs() < 1e-3);
        // Percer la moitié de l'armure : 40 au lieu de 80
        assert!((blow(&mut creatures, knight, feet, DamageKind::Physical, 0.5) - 1000.0 / 140.0).abs() < 1e-3);
        assert!((blow(&mut creatures, knight, feet, DamageKind::Physical, 1.0) - 10.0).abs() < 1e-3, "tout percer, c'est comme sans armure");
        assert!((blow(&mut creatures, knight, feet, DamageKind::Magical, 1.0) - 1000.0 / 120.0).abs() < 1e-3, "percer ne touche pas la résistance");
    }

    #[test]
    fn shreds_add_up_to_their_cap_then_wear_off() {
        let (mut creatures, feet) = world();
        let knight = armored(&mut creatures, feet, "deathKnight");
        assert!(creatures.shred(knight, 0.3, 2.0, 0.5));
        assert!((creatures.armor(knight) - 56.0).abs() < 1e-3);
        creatures.shred(knight, 0.3, 2.0, 0.5);
        assert!((creatures.armor(knight) - 40.0).abs() < 1e-3, "plafonné à la moitié");
        assert!(!creatures.shred(knight, 0.3, 2.0, 0.5), "rien de plus au plafond");
        let cow = creatures.creatures()[0].id;
        assert!(!creatures.shred(cow, 1.0, 2.0, 1.0), "rien à briser sur une vache");
        // Le temps du bris passe avec celui des créatures
        let generator = crate::PlanetWorldGenerator::new(crate::PlanetDescriptor { seed: 4242, ..Default::default() });
        let edits = crate::TerrainEdits::new(None);
        for _ in 0..10 {
            creatures.tick(0.1, &generator, &edits, &[feet]);
        }
        assert!((creatures.armor(knight) - 40.0).abs() < 1e-3, "le bris dure encore");
        for _ in 0..12 {
            creatures.tick(0.1, &generator, &edits, &[feet]);
        }
        assert!((creatures.armor(knight) - 80.0).abs() < 1e-3, "l'armure revient");
    }

    #[test]
    fn a_ballista_shot_breaks_the_armor_before_its_own_blow() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let knight = armored(&mut creatures, feet, "deathKnight");
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 10_000.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        // Un arc bandé à fond
        hold(&mut runner, "shortBow", PlayerClass::WarPriest);
        runner.arsenal_mut().begin_charge(PlayerId(1), Some("ballistaShot"));
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 2.0);
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "ballistaShot", None);
        assert_eq!(creatures.armor(knight), 0.0, "toute l'armure brisée");
        let (on_knight, on_cow) = (10_000.0 - health(&creatures, knight), 10_000.0 - health(&creatures, cow));
        assert!(on_cow > 0.0 && (on_knight - on_cow).abs() < 1e-3, "le chevalier prend le coup comme une bête sans armure");
    }

    #[test]
    fn a_monster_skewer_pierces_half_the_armor() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let knight = armored(&mut creatures, feet, "deathKnight");
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 10_000.0;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "monsterSkewer", None);
        let (on_knight, on_cow) = (10_000.0 - health(&creatures, knight), 10_000.0 - health(&creatures, cow));
        // Rang 1 : 40 % de l'armure ignorée, il en reste 48
        assert!((on_knight / on_cow - 100.0 / 148.0).abs() < 1e-3);
        assert_eq!(creatures.armor(knight), 80.0, "percer ne brise rien");
    }

    #[test]
    fn mana_skills_are_magical_and_stamina_skills_physical_unless_told_otherwise() {
        let catalog = SkillCatalog::load_default();
        let kind = |id: &str| catalog.get(id).and_then(|d| d.cast.as_ref()).map(|c| c.damage_kind()).unwrap();
        assert_eq!(kind("judgment"), DamageKind::Magical, "le Jugement coûte de la mana");
        assert_eq!(kind("righteousStrike"), DamageKind::Physical);
        assert_eq!(kind("twinShot"), DamageKind::Magical, "des balles magiques, malgré l'endurance");
        assert_eq!(kind("etheriumStrike"), DamageKind::Physical);
    }

    #[test]
    fn a_rallying_cry_only_guards_against_physical_blows() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "soldierRallyingCry", None);
        assert!((runner.damage_taken_factor(PlayerId(1), DamageKind::Physical) - 0.75).abs() < 1e-5);
        assert_eq!(runner.damage_taken_factor(PlayerId(1), DamageKind::Magical), 1.0);
    }

    #[test]
    fn faith_armor_lets_magic_through() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "faithArmor", None);
        let hp = stats.hp;
        stats.take_hit(10.0, Some(DamageKind::Physical));
        assert_eq!(stats.hp, hp, "le bouclier absorbe le coup physique");
        stats.take_hit(10.0, Some(DamageKind::Magical));
        assert_eq!(stats.hp, hp - 10.0, "le coup magique passe");
    }

    #[test]
    fn a_miasma_cloud_eats_the_armor_of_those_inside_up_to_its_cap() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let knight = armored(&mut creatures, feet, "deathKnight");
        let at = creatures.creatures().iter().find(|c| c.id == knight).unwrap().position;
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "miasmaCloud", Some(at));
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        let early = creatures.armor(knight);
        assert!(early < 80.0, "rongée dès les premiers passages");
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 5.0);
        assert!((creatures.armor(knight) - 32.0).abs() < 1e-3, "60 % au plus");
    }

    #[test]
    fn the_void_monolith_breaks_the_armor_of_each_target_it_strikes() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let knight = armored(&mut creatures, feet, "deathKnight");
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        let mut runner = SkillRunner::new();
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "voidMonolith", None);
        let at = creatures.creatures().iter().find(|c| c.id == knight).unwrap().position;
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        runner.ride(PlayerId(1), &[at], &mut creatures, &mut player);
        assert_eq!(creatures.armor(knight), 0.0, "un coup d'épée sous le Monolithe désintègre l'armure");
    }

    fn hold(runner: &mut SkillRunner, id: &str, class: PlayerClass) {
        runner.hold(PlayerId(1), Some(herocraft_common::items().find(id).unwrap()), class);
    }

    #[test]
    fn a_shooting_skill_needs_its_weapon_and_a_bolt() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::Ranger);
        stats.skills.insert("recoilShot".into(), 1);
        stats.stamina = 100.0;
        let mut runner = SkillRunner::new();
        assert_eq!(runner.cast(&catalog, PlayerId(1), &mut stats, "recoilShot", Vec3::NEG_Z, None), Err(CastError::NoWeapon), "mains nues");
        hold(&mut runner, "shortBow", PlayerClass::Ranger);
        assert_eq!(runner.cast(&catalog, PlayerId(1), &mut stats, "recoilShot", Vec3::NEG_Z, None), Err(CastError::NoWeapon), "un arc n'est pas une arbalète");
        hold(&mut runner, "handCrossbow", PlayerClass::Ranger);
        let stamina = stats.stamina;
        assert_eq!(runner.cast(&catalog, PlayerId(1), &mut stats, "recoilShot", Vec3::NEG_Z, None), Err(CastError::Reloading), "elle se charge à la prise en main");
        assert_eq!(stats.stamina, stamina, "un refus ne coûte rien");
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        assert_eq!(runner.cast(&catalog, PlayerId(1), &mut stats, "recoilShot", Vec3::NEG_Z, None), Ok(1));
        assert_eq!(runner.arsenal().magazine(PlayerId(1)).unwrap().loaded, 1, "le Rôdeur garde son second carreau");
    }

    #[test]
    fn evasive_moves_reload_the_crossbow() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::MonsterHunter);
        let mut runner = SkillRunner::new();
        hold(&mut runner, "handCrossbow", PlayerClass::MonsterHunter);
        assert_eq!(runner.arsenal().magazine(PlayerId(1)).unwrap().loaded, 0);
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "tacticalDodge", None);
        assert_eq!(runner.arsenal().magazine(PlayerId(1)).unwrap().loaded, 1, "rechargée d'un coup");
        assert!(catalog.get("tacticalDodge").unwrap().cast.as_ref().unwrap().movement.as_ref().unwrap().side.is_some(), "un bond de côté");
    }

    #[test]
    fn a_ballista_shot_hits_harder_when_fully_drawn() {
        let catalog = SkillCatalog::load_default();
        let dealt = |held: f32| {
            let (mut creatures, feet) = world();
            let cow = creatures.creatures()[0].id;
            creatures.creature_mut(cow).unwrap().health = 1000.0;
            let mut stats = CharacterStats::new(PlayerClass::Archer);
            stats.skills.insert("ballistaShot".into(), 1);
            stats.stamina = 100.0;
            let mut runner = SkillRunner::new();
            hold(&mut runner, "shortBow", PlayerClass::Archer);
            runner.arsenal_mut().begin_charge(PlayerId(1), Some("ballistaShot"));
            pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, held);
            runner.cast(&catalog, PlayerId(1), &mut stats, "ballistaShot", Vec3::NEG_Z, None)?;
            let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
            until_impact(&mut runner, &catalog, &mut creatures, &mut player);
            Ok(1000.0 - health(&creatures, cow))
        };
        assert_eq!(dealt(0.2), Err(CastError::NotCharged), "relâchée trop tôt");
        let (half, full) = (dealt(0.5).unwrap(), dealt(2.0).unwrap());
        assert!(half > 0.0 && (full / half - 2.0).abs() < 0.05, "à moitié {half}, pleine {full}");
    }

    #[test]
    fn poison_bolts_ride_the_next_three_shots() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let cow = creatures.creatures()[0].id;
        creatures.creature_mut(cow).unwrap().health = 1000.0;
        let mut stats = CharacterStats::new(PlayerClass::Rogue);
        let mut runner = SkillRunner::new();
        hold(&mut runner, "shortBow", PlayerClass::Rogue);
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "poisonBolt", None);
        assert_eq!(runner.arsenal().special(PlayerId(1)), Some(("poisonBolt", 3)));
        runner.arsenal_mut().begin_charge(PlayerId(1), None);
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 1.0);
        let fired = runner.arsenal_mut().fire(PlayerId(1)).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        let hit = runner.shoot(PlayerId(1), feet, Vec3::NEG_Z, 5.0, fired, &mut creatures, &mut player);
        assert_eq!((hit.target, hit.special.as_deref()), (Some(cow), Some("poisonBolt")));
        let after_shot = health(&creatures, cow);
        assert_eq!(after_shot, 995.0, "le tir lui-même : arc et flèche");
        pass(&mut runner, &catalog, &mut creatures, &mut stats, feet, 3.0);
        assert!(health(&creatures, cow) < after_shot, "le poison continue");
        assert_eq!(runner.arsenal().special(PlayerId(1)), Some(("poisonBolt", 2)));
    }

    #[test]
    fn frag_rounds_explode_around_the_target() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let block = creatures.block_size();
        let cow = creatures.creatures()[0].id;
        let species = creatures.creatures()[0].species;
        let beside = creatures.spawn(species, feet + Vec3::new(1.5 * block, 0.0, -2.0 * block));
        for id in [cow, beside] {
            creatures.creature_mut(id).unwrap().health = 1000.0;
        }
        let mut stats = CharacterStats::new(PlayerClass::Ranger);
        let mut runner = SkillRunner::new();
        hold(&mut runner, "handCrossbow", PlayerClass::Ranger);
        cast_at(&mut runner, &catalog, &mut creatures, &mut stats, feet, "fragRounds", None);
        runner.arsenal_mut().reload_now(PlayerId(1));
        let fired = runner.arsenal_mut().fire(PlayerId(1)).unwrap();
        let mut player = SoloPlayer { id: PlayerId(1), feet, stats: &mut stats };
        runner.shoot(PlayerId(1), feet, Vec3::NEG_Z, 5.0, fired, &mut creatures, &mut player);
        assert!(health(&creatures, beside) < 1000.0, "l'explosion touche la voisine");
    }
}


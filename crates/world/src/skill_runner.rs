//! Exécution des compétences : le même code juge et applique les compétences sur le serveur et dans
//! une partie solo, sur les créatures et les joueurs.
//!
//! Un lancement réussi (`SkillRunner::cast`) paie son coût, arme sa recharge et attend l'instant
//! d'impact de son geste ; à l'impact (`SkillRunner::tick`), la zone est posée devant le lanceur
//! et chaque cible subit les effets de la compétence. Les effets qui durent (soins et dégâts par
//! seconde, renforcements) sont suivis ici ; ralentis et étourdissements, par les créatures.

use std::collections::HashMap;

use glam::Vec3;
use herocraft_common::skills::{BuffStat, CastError, Center, Copies, Effect, EffectEntry, Fused, PlacedArea, SkillCatalog, Targets, SKILL_BASE_DAMAGE};
use herocraft_common::CharacterStats;

use crate::CreatureWorld;

/// Intervalle entre deux coups d'un effet par seconde, en secondes
const OVER_TIME_STEP: f32 = 0.5;

/// Les joueurs, tels que les compétences les voient
pub trait SkillPlayers {
    /// Identifiants des joueurs en jeu
    fn ids(&self) -> Vec<u64>;
    /// Pieds du joueur, s'il est placé
    fn feet(&self, id: u64) -> Option<Vec3>;
    fn stats_mut(&mut self, id: u64) -> Option<&mut CharacterStats>;
}

/// Un seul joueur : la partie solo
pub struct SoloPlayer<'a> {
    pub id: u64,
    pub feet: Vec3,
    pub stats: &'a mut CharacterStats,
}

impl SkillPlayers for SoloPlayer<'_> {
    fn ids(&self) -> Vec<u64> {
        vec![self.id]
    }

    fn feet(&self, id: u64) -> Option<Vec3> {
        (id == self.id).then_some(self.feet)
    }

    fn stats_mut(&mut self, id: u64) -> Option<&mut CharacterStats> {
        (id == self.id).then_some(&mut *self.stats)
    }
}

/// Qui subit un effet qui dure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillTarget {
    Creature(u64),
    Player(u64),
}

/// Ce qu'a fait une compétence en frappant, pour l'annoncer aux joueurs
#[derive(Debug, Clone, PartialEq)]
pub struct SkillImpact {
    pub caster: u64,
    pub skill: String,
    pub rank: u8,
    /// Centre de la zone
    pub center: Vec3,
    pub creatures: Vec<u64>,
    pub players: Vec<u64>,
    /// Entités que la frappe a fait apparaître (`summon`)
    pub summoned: Vec<u64>,
}

/// Ce que `tick` renvoie : les frappes du moment et les joueurs dont les jauges ont changé
#[derive(Debug, Default)]
pub struct SkillTick {
    pub impacts: Vec<SkillImpact>,
    /// Joueurs à qui renvoyer leurs statistiques
    pub changed: Vec<u64>,
}

#[derive(Debug, Clone)]
struct Pending {
    caster: u64,
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
    caster: u64,
    target: SkillTarget,
    /// Points par seconde : négatifs pour des dégâts, positifs pour des soins
    per_second: f32,
    left: f32,
    /// Temps avant le prochain coup
    next: f32,
    /// Facteur des dégâts tant que la bête bouge, et où elle était au coup précédent
    moving: f32,
    last: Option<Vec3>,
}

#[derive(Debug, Clone)]
struct Buff {
    player: u64,
    stat: BuffStat,
    value: f32,
    left: f32,
}

/// Une entité invoquée, suivie jusqu'à sa disparition
#[derive(Debug, Clone)]
struct Summoned {
    caster: u64,
    species: String,
    id: u64,
    /// Secondes avant qu'elle disparaisse (`None` : jusqu'à sa destruction)
    left: Option<f32>,
    /// Effets sur le lanceur à la fin de sa durée, et le rang de la compétence
    on_end: Vec<EffectEntry>,
    rank: u8,
}

/// Points de vie rendus chaque seconde par un renforcement de régénération de 1 (100 %)
const REGENERATION_HP: f32 = 5.0;

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
    forms: Vec<(u64, String, f32)>,
    /// Recharge restante de chaque compétence de chaque joueur, en secondes
    cooldowns: HashMap<(u64, String), f32>,
    /// Bêtes marquées : par qui, laquelle, chance de critique en plus, secondes restantes
    marks: Vec<(u64, u64, f32, f32)>,
    /// État du tirage des coups critiques
    luck: u64,
    /// Les deux derniers sorts élémentaires de chaque joueur, que la Fusion mêle
    recent: HashMap<u64, Vec<String>>,
    /// Alliés liés à un lanceur (Marque du Martyr) : lanceur, allié, part des dégâts, secondes restantes
    links: Vec<(u64, u64, f32, f32)>,
}

/// Facteur des dégâts d'un coup critique
pub const CRITICAL_FACTOR: f32 = 2.0;

impl SkillRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recharge restante de `skill` pour le joueur `player`, en secondes
    pub fn cooldown(&self, player: u64, skill: &str) -> f32 {
        self.cooldowns.get(&(player, skill.to_string())).copied().unwrap_or(0.0)
    }

    /// Le joueur `caster` lance `skill` : refusé s'il ne l'a pas apprise, si elle n'est pas
    /// rechargée ou s'il n'a pas de quoi la payer ; sinon le coût est payé, la recharge armée, et
    /// la frappe attend l'impact du geste. Renvoie le rang de la compétence.
    pub fn cast(&mut self, catalog: &SkillCatalog, caster: u64, stats: &mut CharacterStats, skill: &str, aim: Vec3, aimed: Option<Vec3>) -> Result<u8, CastError> {
        self.cast_as(catalog, caster, stats, skill, aim, aimed).map(|c| c.rank)
    }

    /// Comme `cast`, et dit ce qui est vraiment lancé : la Fusion lance le mélange des deux derniers
    /// sorts élémentaires du joueur, ou le dernier amplifié s'ils sont du même élément
    pub fn cast_as(&mut self, catalog: &SkillCatalog, caster: u64, stats: &mut CharacterStats, requested: &str, aim: Vec3, aimed: Option<Vec3>) -> Result<Casting, CastError> {
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
        self.cooldowns.insert((caster, requested.to_string()), cast.cooldown.at(rank));
        // Une attaque lancée invisible le dévoile, et frappe en critique
        let attacks = cast.effects.iter().any(|e| matches!(e.effect, Effect::Damage { .. } | Effect::OverTime { damage: Some(_), .. }));
        let crit = attacks && stats.is_invisible();
        if attacks {
            stats.invisible_time = 0.0;
        }
        self.remember(catalog, caster, requested);
        self.pending.push(Pending { caster, skill: skill.to_string(), rank, aim: aim.normalize_or_zero(), aimed, left: catalog.impact_delay(cast), crit, power, size });
        Ok(Casting { skill: skill.to_string(), rank, amplified })
    }

    /// Ce que la Fusion du joueur `caster` lancerait maintenant : refusée sans deux sorts
    /// élémentaires, ou si leur mélange n'est pas appris
    pub fn fusion_preview(&self, catalog: &SkillCatalog, caster: u64, stats: &CharacterStats) -> Result<Fused, CastError> {
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
    pub fn remember(&mut self, catalog: &SkillCatalog, caster: u64, skill: &str) {
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
    pub fn recent_spells(&self, caster: u64) -> &[String] {
        self.recent.get(&caster).map(Vec::as_slice).unwrap_or_default()
    }

    /// Le joueur `player` subit `damage` points : la part qu'il garde, et celle qu'encaisse le
    /// lanceur qui l'a lié (Marque du Martyr), s'il en est un
    pub fn share_damage(&self, player: u64, damage: f32) -> (f32, Option<(u64, f32)>) {
        match self.links.iter().find(|l| l.1 == player) {
            Some(&(caster, _, share, _)) => (damage * (1.0 - share), Some((caster, damage * share))),
            None => (damage, None),
        }
    }

    /// La forme (compétence du Druide) que le joueur `player` a prise en ce moment
    pub fn form(&self, player: u64) -> Option<&str> {
        self.forms.iter().find(|f| f.0 == player).map(|f| f.1.as_str())
    }

    /// Renforcement de `stat` du joueur `player` (0 : aucun ; 0,3 : +30 %)
    pub fn buff(&self, player: u64, stat: BuffStat) -> f32 {
        self.buffs.iter().filter(|b| b.player == player && b.stat == stat).map(|b| b.value).sum()
    }

    /// Facteur des dégâts que subit le joueur `player` (renforcement de défense compris)
    pub fn damage_taken_factor(&self, player: u64) -> f32 {
        (1.0 - self.buff(player, BuffStat::Defense)).clamp(0.1, 1.0)
    }

    /// Entités invoquées par le joueur `player` encore présentes
    pub fn summoned_by(&self, player: u64) -> impl Iterator<Item = u64> + '_ {
        self.summoned.iter().filter(move |s| s.caster == player).map(|s| s.id)
    }

    /// Oublie le joueur `player` (il quitte la partie) ; ses invocations disparaissent avec lui
    pub fn forget_with(&mut self, player: u64, creatures: &mut CreatureWorld) {
        for s in self.summoned.iter().filter(|s| s.caster == player) {
            creatures.dismiss(s.id);
        }
        self.forget(player);
    }

    /// Oublie le joueur `player` (il quitte la partie)
    pub fn forget(&mut self, player: u64) {
        self.summoned.retain(|s| s.caster != player);
        self.pending.retain(|p| p.caster != player);
        self.over_time.retain(|o| o.caster != player && o.target != SkillTarget::Player(player));
        self.buffs.retain(|b| b.player != player);
        self.forms.retain(|f| f.0 != player);
        self.marks.retain(|m| m.0 != player);
        self.recent.remove(&player);
        self.links.retain(|l| l.0 != player && l.1 != player);
        self.cooldowns.retain(|(p, _), _| *p != player);
    }

    /// Avance de `dt` secondes : recharges, frappes arrivées à leur impact, effets qui durent
    pub fn tick(&mut self, dt: f32, catalog: &SkillCatalog, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> SkillTick {
        let mut out = SkillTick::default();
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
            link.3 -= dt;
        }
        self.links.retain(|l| l.3 > 0.0);
        for buff in &mut self.buffs {
            buff.left -= dt;
            // Régénération : des points de vie rendus chaque instant
            if buff.stat == BuffStat::Regeneration && buff.value > 0.0 {
                if let Some(stats) = players.stats_mut(buff.player) {
                    stats.hp = (stats.hp + buff.value * REGENERATION_HP * dt).min(stats.max_hp);
                    out.changed.push(buff.player);
                }
            }
        }
        self.buffs.retain(|b| b.left > 0.0);
        // Les invocations détruites sont oubliées ; celles dont la durée est passée disparaissent
        let alive: std::collections::HashSet<u64> = creatures.creatures().iter().filter(|c| c.is_alive()).map(|c| c.id).collect();
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
                self.apply_to_player(entry, caster, rank, 1.0, caster, players);
            }
            if !effects.is_empty() {
                out.changed.push(caster);
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

        let block = creatures.block_size();
        let mut finished = Vec::new();
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
                        creatures.wound(id, from, o.caster, -amount);
                    }
                    SkillTarget::Creature(_) => {}
                    SkillTarget::Player(id) => {
                        if let Some(stats) = players.stats_mut(id) {
                            if amount > 0.0 {
                                stats.hp = (stats.hp + amount).min(stats.max_hp);
                            } else {
                                stats.take_damage(-amount);
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
        out.changed.sort_unstable();
        out.changed.dedup();
        out
    }

    /// La frappe du lancement `p` : zone posée devant le lanceur, effets sur chaque cible
    fn strike(&mut self, catalog: &SkillCatalog, p: &Pending, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers, changed: &mut Vec<u64>) -> Option<SkillImpact> {
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
            let marked: Vec<u64> = self.marks.iter().filter(|m| m.0 == p.caster).map(|m| m.1).collect();
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

        // Les cibles de la zone : créatures vivantes (les bêtes apprivoisées sont des alliées) et joueurs
        let in_area = |point: Vec3| area.contains(point, block);
        let creature_hits: Vec<(u64, bool)> = creatures.creatures().iter().filter(|c| c.is_alive() && in_area(c.position)).map(|c| (c.id, c.tamed)).collect();
        let player_hits: Vec<u64> = players
            .ids()
            .into_iter()
            .filter(|&id| id == p.caster && matches!(area, PlacedArea::Caster) || players.feet(id).is_some_and(in_area))
            .collect();

        let mut touched_creatures = Vec::new();
        let mut touched_players = Vec::new();
        let groups = [(cast.targets, &cast.effects)];
        for (targets, effects) in groups {
            for entry in effects.iter() {
                let to = entry.to.unwrap_or(targets);
                for &(id, tamed) in &creature_hits {
                    let hostile = !tamed;
                    if matches!(to, Targets::All) || (to == Targets::Enemies && hostile) || (to == Targets::Allies && !hostile) {
                        self.apply_to_creature(entry, p, id, feet, damage_factor, creatures);
                        touched_creatures.push(id);
                    }
                }
                for &id in &player_hits {
                    let applies = match to {
                        Targets::Allies | Targets::All => true,
                        Targets::Caster => id == p.caster,
                        Targets::Enemies => false,
                    };
                    if applies {
                        self.apply_to_player(entry, p.caster, p.rank, p.power, id, players);
                        touched_players.push(id);
                        changed.push(id);
                    }
                }
            }
        }
        for entry in &cast.self_effects {
            self.apply_to_player(entry, p.caster, p.rank, p.power, p.caster, players);
            changed.push(p.caster);
        }
        let summoned = self.summon(def.summons(), p, feet, center, &area, creatures);
        touched_creatures.sort_unstable();
        touched_creatures.dedup();
        touched_players.sort_unstable();
        touched_players.dedup();
        Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center, creatures: touched_creatures, players: touched_players, summoned })
    }

    /// Les apparitions d'une frappe, une fois par lancement : en cercle autour de leur point,
    /// les plus anciennes du lanceur disparaissant au-delà de la limite
    fn summon<'a>(&mut self, summons: impl Iterator<Item = &'a Effect>, p: &Pending, feet: Vec3, center: Vec3, area: &PlacedArea, creatures: &mut CreatureWorld) -> Vec<u64> {
        let mut out = Vec::new();
        let block = creatures.block_size();
        for effect in summons {
            let Effect::Summon { species, count, at, duration, limit, copies, on_end } = effect else { continue };
            let Some(index) = creatures.registry().find(species) else { continue };
            let left = duration.as_ref().map(|d| d.at(p.rank));
            // Une par cadavre de la zone, à sa place et à son allure ; le cadavre disparaît
            if *copies == Some(Copies::Corpse) {
                let corpses: Vec<(u64, usize, Vec3, f32)> =
                    creatures.creatures().iter().filter(|c| !c.is_alive() && c.summoner.is_none() && area.contains(c.position, block)).map(|c| (c.id, c.species, c.position, c.yaw)).collect();
                for (corpse, kind, position, yaw) in corpses {
                    creatures.remove(corpse);
                    let id = creatures.summon(index, position, yaw, p.caster);
                    creatures.set_copy(id, kind as u32);
                    self.summoned.push(Summoned { caster: p.caster, species: species.clone(), id, left, on_end: on_end.clone(), rank: p.rank });
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
            let n = count.at(p.rank).round().max(1.0) as usize;
            let radius = if n > 1 { SUMMON_SPACING * block * n as f32 / std::f32::consts::TAU } else { 0.0 };
            for i in 0..n {
                let angle = std::f32::consts::TAU * i as f32 / n as f32;
                let spot = origin + (forward * angle.cos() + right * angle.sin()) * radius;
                let id = creatures.summon(index, spot, 0.0, p.caster);
                if *copies == Some(Copies::Caster) {
                    creatures.set_copy(id, herocraft_common::COPY_SUMMONER);
                }
                self.summoned.push(Summoned { caster: p.caster, species: species.clone(), id, left, on_end: on_end.clone(), rank: p.rank });
                out.push(id);
            }
            if let Some(limit) = limit {
                let limit = limit.at(p.rank).round().max(1.0) as usize;
                let mine: Vec<u64> = self.summoned.iter().filter(|s| s.caster == p.caster && s.species == *species).map(|s| s.id).collect();
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
    pub fn critical_chance(&self, caster: u64, id: u64) -> f32 {
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

    fn apply_to_creature(&mut self, entry: &EffectEntry, p: &Pending, id: u64, feet: Vec3, damage_factor: f32, creatures: &mut CreatureWorld) {
        let rank = p.rank;
        match &entry.effect {
            Effect::Damage { power } => {
                let chance = if p.crit { 1.0 } else { self.critical_chance(p.caster, id) };
                let critical = chance > 0.0 && self.roll() < chance;
                let factor = if critical { CRITICAL_FACTOR } else { 1.0 };
                creatures.wound(id, feet, p.caster, power.at(rank) * SKILL_BASE_DAMAGE * damage_factor * factor);
            }
            Effect::Blind { duration } => creatures.blind(id, duration.at(rank)),
            Effect::Taunt { duration } => creatures.taunt(id, p.caster, duration.at(rank)),
            Effect::Mark { chance, duration } => {
                self.marks.retain(|m| !(m.0 == p.caster && m.1 == id));
                self.marks.push((p.caster, id, chance.at(rank), duration.at(rank)));
            }
            Effect::Slow { strength, duration } => creatures.slow(id, strength.at(rank), duration.at(rank)),
            Effect::Stun { duration } => creatures.stun(id, duration.at(rank)),
            Effect::Knockback { strength } => creatures.shove(id, feet, strength.at(rank) * p.power),
            Effect::Pull { strength } => creatures.shove(id, feet, -strength.at(rank) * p.power),
            Effect::OverTime { damage, duration, moving, .. } => {
                if let Some(damage) = damage {
                    self.over_time.push(OverTime {
                        caster: p.caster,
                        target: SkillTarget::Creature(id),
                        per_second: -damage.at(rank) * damage_factor,
                        left: duration.at(rank),
                        next: OVER_TIME_STEP,
                        moving: moving.as_ref().map_or(1.0, |m| m.at(rank)),
                        last: Some(feet).and_then(|_| creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position)),
                    });
                }
            }
            // Sans effet sur une bête : soins, boucliers, renforcements, invisibilité, résurrection, lien
            Effect::Heal { .. } | Effect::Shield { .. } | Effect::Buff { .. } | Effect::Summon { .. } | Effect::Invisible { .. } | Effect::Revive { .. } | Effect::Link { .. } => {}
        }
    }

    /// Effet `entry` du lanceur `caster` sur le joueur `id`, ses soins et boucliers multipliés par `power`
    fn apply_to_player(&mut self, entry: &EffectEntry, caster: u64, rank: u8, power: f32, id: u64, players: &mut dyn SkillPlayers) {
        match &entry.effect {
            Effect::Heal { amount } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.hp = (stats.hp + amount.at(rank) * power).min(stats.max_hp);
                }
            }
            Effect::Link { share, duration } => {
                // On ne se lie pas à soi-même
                if id != caster {
                    self.links.retain(|l| l.1 != id);
                    self.links.push((caster, id, share.at(rank).clamp(0.0, 1.0), duration.at(rank)));
                }
            }
            Effect::Shield { amount, duration } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.shield = stats.shield.max(amount.at(rank) * power);
                    stats.shield_time = stats.shield_time.max(duration.at(rank));
                }
            }
            Effect::Buff { stat, value, duration } => {
                self.buffs.retain(|b| !(b.player == id && b.stat == *stat));
                self.buffs.push(Buff { player: id, stat: *stat, value: value.at(rank), left: duration.at(rank) });
            }
            Effect::OverTime { heal, damage, duration, .. } => {
                let per_second = heal.as_ref().map_or(0.0, |h| h.at(rank) * power) - damage.as_ref().map_or(0.0, |d| d.at(rank));
                if per_second != 0.0 {
                    self.over_time.push(OverTime { caster, target: SkillTarget::Player(id), per_second, left: duration.at(rank), next: OVER_TIME_STEP, moving: 1.0, last: None });
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
            // Pas de combat entre joueurs : ni dégâts, ni ralenti, ni étourdissement d'un allié (ce
            // sont les bêtes qui ralentissent et étourdissent les joueurs, `CreatureWorld::afflict`)
            Effect::Damage { .. } | Effect::Slow { .. } | Effect::Stun { .. } | Effect::Knockback { .. } | Effect::Pull { .. } | Effect::Summon { .. } | Effect::Taunt { .. } | Effect::Blind { .. } | Effect::Mark { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::{CreatureRegistry, PlayerClass};
    use std::sync::Arc;

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
        assert_eq!(runner.cast(&catalog, 7, &mut stats, "judgment", Vec3::NEG_Z, None), Ok(1));
        assert!(stats.mana < mana, "le mana est payé");
        assert_eq!(runner.cast(&catalog, 7, &mut stats, "judgment", Vec3::NEG_Z, None), Err(CastError::Cooldown));

        let near = creatures.creatures()[0].id;
        let health = |w: &CreatureWorld, id| w.creatures().iter().find(|c| c.id == id).map(|c| c.health).unwrap();
        let before = health(&creatures, near);
        // Rien avant l'impact du geste (0,315 s)
        let mut player = SoloPlayer { id: 7, feet, stats: &mut stats };
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
        assert_eq!(runner.cooldown(7, "judgment"), 0.0);
    }

    #[test]
    fn test_self_effects_shield_and_heal_the_caster() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut stats = CharacterStats::new(PlayerClass::WarPriest);
        stats.skills.insert("faithArmor".into(), 2);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, 1, &mut stats, "faithArmor", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: 1, feet, stats: &mut stats };
        let tick = runner.tick(1.0, &catalog, &mut creatures, &mut player);
        assert_eq!(tick.changed, vec![1]);
        assert_eq!(stats.shield, 30.0);
        let hp = stats.hp;
        stats.take_damage(10.0);
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
            runner.cast(&catalog, 3, &mut stats, "shrapnelMine", Vec3::NEG_Z, None).unwrap();
            let mut player = SoloPlayer { id: 3, feet, stats: &mut stats };
            let mut summoned = Vec::new();
            for _ in 0..20 {
                summoned.extend(runner.tick(0.5, &catalog, &mut creatures, &mut player).impacts.into_iter().flat_map(|i| i.summoned));
            }
            assert_eq!(summoned.len(), 1, "une mine par lancement");
            mines.extend(summoned);
        }
        // Trois mines au plus au rang 1 : la plus ancienne a disparu
        let present: Vec<u64> = runner.summoned_by(3).collect();
        assert_eq!(present, mines[1..].to_vec());
        assert!(creatures.creatures().iter().all(|c| c.id != mines[0]));
        let mine = creatures.creatures().iter().find(|c| c.id == mines[3]).unwrap();
        assert!(mine.tamed && mine.summoner == Some(3), "alliée du lanceur");
        assert!(mine.position.distance(feet) > creatures.block_size(), "posée devant, au point visé");

        // En quittant la partie, l'ingénieur emporte ses mines
        runner.forget_with(3, &mut creatures);
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
        creatures.wound(near, feet, 5, 10_000.0);
        let mut stats = CharacterStats::new(PlayerClass::Necromancer);
        stats.skills.insert("voidLegion".into(), 1);
        stats.skills.insert("shadowDecoy".into(), 1);
        stats.mana = 1000.0;
        stats.stamina = 1000.0;
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, 5, &mut stats, "voidLegion", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: 5, feet, stats: &mut stats };
        let summoned: Vec<u64> = (0..10).flat_map(|_| runner.tick(0.2, &catalog, &mut creatures, &mut player).impacts).flat_map(|i| i.summoned).collect();
        assert_eq!(summoned.len(), 1, "une ombre par cadavre");
        assert!(creatures.creatures().iter().all(|c| c.id != near), "le cadavre est devenu l'ombre");
        let shade = creatures.creatures().iter().find(|c| c.id == summoned[0]).unwrap();
        assert_eq!(shade.copy, Some(cow as u32));
        assert_eq!(stats.shield, 0.0, "pas de bouclier avant l'implosion");
        // Au bout de leur durée (8 s au rang 1), les ombres implosent en bouclier
        let mut player = SoloPlayer { id: 5, feet, stats: &mut stats };
        for _ in 0..40 {
            runner.tick(0.2, &catalog, &mut creatures, &mut player);
        }
        assert!(creatures.creatures().iter().all(|c| c.id != summoned[0]), "l'ombre a implosé");
        assert!(stats.shield > 0.0, "le nécromancien reçoit son bouclier à l'implosion");

        runner.cast(&catalog, 5, &mut stats, "shadowDecoy", Vec3::NEG_Z, None).unwrap();
        let mut player = SoloPlayer { id: 5, feet, stats: &mut stats };
        let decoy: Vec<u64> = (0..10).flat_map(|_| runner.tick(0.2, &catalog, &mut creatures, &mut player).impacts).flat_map(|i| i.summoned).collect();
        let decoy = creatures.creatures().iter().find(|c| c.id == decoy[0]).unwrap();
        assert_eq!((decoy.copy, decoy.summoner), (Some(herocraft_common::COPY_SUMMONER), Some(5)));
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
        assert_eq!(runner.cast(&catalog, 5, &mut stats, "stagAntlers", Vec3::NEG_Z, None), Err(CastError::NotInForm));
        runner.cast(&catalog, 5, &mut stats, "ancientStag", Vec3::NEG_Z, None).unwrap();
        assert_eq!(runner.form(5), Some("ancientStag"));
        assert_eq!(runner.cast(&catalog, 5, &mut stats, "packClaws", Vec3::NEG_Z, None), Err(CastError::InForm));
        runner.cast(&catalog, 5, &mut stats, "stagAntlers", Vec3::NEG_Z, None).unwrap();
        // Le cerf régénère : la vie remonte
        stats.hp = 10.0;
        let mut player = SoloPlayer { id: 5, feet, stats: &mut stats };
        for _ in 0..20 {
            runner.tick(0.25, &catalog, &mut creatures, &mut player);
        }
        assert!(stats.hp > 12.0, "{}", stats.hp);
        // La forme passée, plus d'attaques de cerf
        let mut player = SoloPlayer { id: 5, feet, stats: &mut stats };
        for _ in 0..100 {
            runner.tick(0.25, &catalog, &mut creatures, &mut player);
        }
        assert_eq!(runner.form(5), None);
    }

    /// Deux joueurs côte à côte
    struct Pair<'a> {
        feet: Vec3,
        a: (u64, &'a mut CharacterStats),
        b: (u64, &'a mut CharacterStats),
    }

    impl SkillPlayers for Pair<'_> {
        fn ids(&self) -> Vec<u64> {
            vec![self.a.0, self.b.0]
        }
        fn feet(&self, id: u64) -> Option<Vec3> {
            (id == self.a.0 || id == self.b.0).then_some(self.feet)
        }
        fn stats_mut(&mut self, id: u64) -> Option<&mut CharacterStats> {
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
            if !runner.tick(0.02, catalog, creatures, players).impacts.is_empty() {
                return;
            }
        }
        panic!("pas de frappe");
    }

    fn health(w: &CreatureWorld, id: u64) -> f32 {
        w.creatures().iter().find(|c| c.id == id).map(|c| c.health).unwrap()
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
        runner.cast(&catalog, 4, &mut stats, "shadowVeil", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 4, feet, stats: &mut stats });
        assert!(stats.is_invisible(), "le voile ne rend pas invisible");

        runner.cast(&catalog, 4, &mut stats, "ambush", Vec3::NEG_Z, None).unwrap();
        assert!(!stats.is_invisible(), "l'attaque ne dévoile pas");
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 4, feet, stats: &mut stats });
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
        runner.cast(&catalog, 2, &mut stats, "deadlyHunt", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 2, feet, stats: &mut stats });
        let expected = 1.8 * SKILL_BASE_DAMAGE * CRITICAL_FACTOR;
        assert!((1000.0 - health(&creatures, cow) - expected).abs() < 0.01, "pas de critique : {} dégâts", 1000.0 - health(&creatures, cow));

        runner.tick(1.0, &catalog, &mut creatures, &mut SoloPlayer { id: 2, feet, stats: &mut stats });
        assert_eq!(runner.critical_chance(2, cow), 0.0, "la marque de la chasse ne dure pas");
        runner.cast(&catalog, 2, &mut stats, "hawkEye", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 2, feet, stats: &mut stats });
        assert!((runner.critical_chance(2, cow) - 0.3).abs() < 1e-6, "l'œil de faucon : {}", runner.critical_chance(2, cow));

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
        runner.cast(&catalog, 3, &mut stats, "tracerRound", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 3, feet, stats: &mut stats });
        assert!(runner.critical_chance(3, cow) > 0.0, "la balle traçante ne marque pas");
        assert_eq!(runner.critical_chance(4, cow), 0.0, "la marque ne sert que son tireur");

        runner.cast(&catalog, 3, &mut stats, "radiantStrike", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 3, feet, stats: &mut stats });
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
        runner.cast(&catalog, 1, &mut chaplain, "supremeIntervention", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut Pair { feet, a: (1, &mut chaplain), b: (2, &mut ally) });
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
        assert_eq!(runner.cast(&catalog, 7, &mut stats, "judgment", Vec3::NEG_Z, None), Err(CastError::Stunned));
        stats.tick_status(1.5);
        assert_eq!(runner.cast(&catalog, 7, &mut stats, "judgment", Vec3::NEG_Z, None), Ok(1));
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
            until_impact(runner, &catalog, creatures, &mut SoloPlayer { id: 6, feet, stats });
            runner.tick(6.0, &catalog, creatures, &mut SoloPlayer { id: 6, feet, stats });
        };
        assert_eq!(runner.cast(&catalog, 6, &mut stats, "fusion", Vec3::NEG_Z, Some(at)), Err(CastError::FusionEmpty));
        assert_eq!(runner.cast(&catalog, 6, &mut stats, "scaldingTyphoon", Vec3::NEG_Z, Some(at)), Err(CastError::FusionOnly));

        // Feu puis eau : le Typhon Bouillant, s'il est appris
        runner.cast(&catalog, 6, &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        runner.cast(&catalog, 6, &mut stats, "foamingWhip", Vec3::NEG_Z, None).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        assert_eq!(runner.cast(&catalog, 6, &mut stats, "fusion", Vec3::NEG_Z, Some(at)), Err(CastError::FusionLocked));
        stats.skills.insert("scaldingTyphoon".into(), 1);
        let casting = runner.cast_as(&catalog, 6, &mut stats, "fusion", Vec3::NEG_Z, Some(at)).unwrap();
        assert_eq!((casting.skill.as_str(), casting.amplified), ("scaldingTyphoon", false));
        run(&mut runner, &mut stats, &mut creatures);
        assert!(runner.recent_spells(6).is_empty(), "la Fusion consomme ses deux sorts");

        // Deux orbes : la seconde relancée amplifiée
        runner.cast(&catalog, 6, &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        runner.cast(&catalog, 6, &mut stats, "igneousOrb", Vec3::NEG_Z, Some(at)).unwrap();
        run(&mut runner, &mut stats, &mut creatures);
        let before = health(&creatures, cow);
        let mana = stats.mana;
        let casting = runner.cast_as(&catalog, 6, &mut stats, "fusion", Vec3::NEG_Z, Some(at)).unwrap();
        assert_eq!((casting.skill.as_str(), casting.amplified), ("igneousOrb", true));
        assert!((mana - stats.mana - 12.0 * 1.5).abs() < 0.01, "coût amplifié : {}", mana - stats.mana);
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 6, feet, stats: &mut stats });
        let expected = 1.3 * SKILL_BASE_DAMAGE * 1.75;
        assert!((before - health(&creatures, cow) - expected).abs() < 0.01, "dégâts amplifiés : {}", before - health(&creatures, cow));
        assert_eq!(runner.cooldown(6, "fusion") > 0.0, true, "la Fusion a sa propre recharge");
    }

    #[test]
    fn martyrs_mark_shares_the_allys_damage_with_the_chaplain() {
        let catalog = SkillCatalog::load_default();
        let (mut creatures, feet) = world();
        let mut chaplain = CharacterStats::new(PlayerClass::Chaplain);
        chaplain.skills.insert("martyrsMark".into(), 1);
        let mut ally = CharacterStats::new(PlayerClass::Soldier);
        let mut runner = SkillRunner::new();
        runner.cast(&catalog, 1, &mut chaplain, "martyrsMark", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut Pair { feet, a: (1, &mut chaplain), b: (2, &mut ally) });
        let (kept, shared) = runner.share_damage(2, 10.0);
        assert!((kept - 6.0).abs() < 1e-4 && shared == Some((1, 4.0)), "{kept} {shared:?}");
        assert_eq!(runner.share_damage(1, 10.0), (10.0, None), "le chapelain ne se lie pas à lui-même");
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
        runner.cast(&catalog, 3, &mut stats, "tracerRound", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 3, feet, stats: &mut stats });
        let before = health(&creatures, cow);
        // Visé loin de la vache, à dix blocs sur le côté : le tir la retrouve
        runner.cast(&catalog, 3, &mut stats, "detonatingMark", Vec3::X, Some(feet + Vec3::X * 10.0 * block)).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 3, feet, stats: &mut stats });
        assert!(health(&creatures, cow) < before, "le tir n'a pas cherché la bête marquée");

        let mut archer = CharacterStats::new(PlayerClass::Archer);
        archer.skills.insert("hawkEye".into(), 1);
        runner.cast(&catalog, 4, &mut archer, "hawkEye", Vec3::NEG_Z, None).unwrap();
        until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 4, feet, stats: &mut archer });
        assert!((runner.buff(4, BuffStat::Range) - 0.3).abs() < 1e-6, "l'œil de faucon n'allonge pas la portée");
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
            runner.cast(&catalog, 2, &mut stats, "deadlyHunt", Vec3::NEG_Z, None).unwrap();
            until_impact(&mut runner, &catalog, &mut creatures, &mut SoloPlayer { id: 2, feet, stats: &mut stats });
            let after_hit = health(&creatures, cow);
            for _ in 0..20 {
                if moves {
                    creatures.creature_mut(cow).unwrap().position += Vec3::X * 0.5 * block;
                }
                runner.tick(0.25, &catalog, &mut creatures, &mut SoloPlayer { id: 2, feet, stats: &mut stats });
            }
            after_hit - health(&creatures, cow)
        };
        let (still, moving) = (bled(false), bled(true));
        assert!(still > 0.0 && moving > 2.0 * still, "immobile {still}, en mouvement {moving}");
    }
}

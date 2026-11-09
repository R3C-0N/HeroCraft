//! Exécution des compétences : le même code juge et applique les compétences sur le serveur et dans
//! une partie solo, sur les créatures et les joueurs.
//!
//! Un lancement réussi (`SkillRunner::cast`) paie son coût, arme sa recharge et attend l'instant
//! d'impact de son geste ; à l'impact (`SkillRunner::tick`), la zone est posée devant le lanceur
//! et chaque cible subit les effets de la compétence. Les effets qui durent (soins et dégâts par
//! seconde, renforcements) sont suivis ici ; ralentis et étourdissements, par les créatures.

use std::collections::HashMap;

use glam::Vec3;
use herocraft_common::skills::{BuffStat, CastError, Center, Effect, EffectEntry, PlacedArea, SkillCatalog, Targets, SKILL_BASE_DAMAGE};
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
}

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
    /// Recharge restante de chaque compétence de chaque joueur, en secondes
    cooldowns: HashMap<(u64, String), f32>,
}

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
        if !aim.is_finite() || aimed.is_some_and(|p| !p.is_finite()) {
            return Err(CastError::Unknown);
        }
        if let Some(error) = catalog.cast_check(stats, skill, self.cooldown(caster, skill)) {
            return Err(error);
        }
        let def = catalog.get(skill).ok_or(CastError::Unknown)?;
        let cast = def.cast.as_ref().ok_or(CastError::NotPlayable)?;
        let rank = catalog.rank(stats, skill);
        stats.mana -= cast.cost.mana.at(rank);
        stats.stamina -= cast.cost.stamina.at(rank);
        self.cooldowns.insert((caster, skill.to_string()), cast.cooldown.at(rank));
        self.pending.push(Pending { caster, skill: skill.to_string(), rank, aim: aim.normalize_or_zero(), aimed, left: catalog.impact_delay(cast) });
        Ok(rank)
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
        self.cooldowns.retain(|(p, _), _| *p != player);
    }

    /// Avance de `dt` secondes : recharges, frappes arrivées à leur impact, effets qui durent
    pub fn tick(&mut self, dt: f32, catalog: &SkillCatalog, creatures: &mut CreatureWorld, players: &mut dyn SkillPlayers) -> SkillTick {
        let mut out = SkillTick::default();
        for left in self.cooldowns.values_mut() {
            *left -= dt;
        }
        self.cooldowns.retain(|_, left| *left > 0.0);
        for buff in &mut self.buffs {
            buff.left -= dt;
        }
        self.buffs.retain(|b| b.left > 0.0);
        // Les invocations détruites sont oubliées ; celles dont la durée est passée disparaissent
        let alive: std::collections::HashSet<u64> = creatures.creatures().iter().filter(|c| c.is_alive()).map(|c| c.id).collect();
        self.summoned.retain_mut(|s| {
            if !alive.contains(&s.id) {
                creatures.dismiss(s.id);
                return false;
            }
            if let Some(left) = &mut s.left {
                *left -= dt;
                if *left <= 0.0 {
                    creatures.dismiss(s.id);
                    return false;
                }
            }
            true
        });

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
        let area = cast.area.place(p.rank, feet, p.aim, p.aimed, block);
        let center = area.center(feet);
        let damage_factor = 1.0 + self.buff(p.caster, BuffStat::Damage);

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
                        self.apply_to_player(entry, p, id, players);
                        touched_players.push(id);
                        changed.push(id);
                    }
                }
            }
        }
        for entry in &cast.self_effects {
            self.apply_to_player(entry, p, p.caster, players);
            changed.push(p.caster);
        }
        let summoned = self.summon(def.summons(), p, feet, center, creatures);
        touched_creatures.sort_unstable();
        touched_creatures.dedup();
        touched_players.sort_unstable();
        touched_players.dedup();
        Some(SkillImpact { caster: p.caster, skill: p.skill.clone(), rank: p.rank, center, creatures: touched_creatures, players: touched_players, summoned })
    }

    /// Les apparitions d'une frappe, une fois par lancement : en cercle autour de leur point,
    /// les plus anciennes du lanceur disparaissant au-delà de la limite
    fn summon<'a>(&mut self, summons: impl Iterator<Item = &'a Effect>, p: &Pending, feet: Vec3, center: Vec3, creatures: &mut CreatureWorld) -> Vec<u64> {
        let mut out = Vec::new();
        let block = creatures.block_size();
        for effect in summons {
            let Effect::Summon { species, count, at, duration, limit } = effect else { continue };
            let Some(index) = creatures.registry().find(species) else { continue };
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
                self.summoned.push(Summoned { caster: p.caster, species: species.clone(), id, left: duration.as_ref().map(|d| d.at(p.rank)) });
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

    fn apply_to_creature(&mut self, entry: &EffectEntry, p: &Pending, id: u64, feet: Vec3, damage_factor: f32, creatures: &mut CreatureWorld) {
        let rank = p.rank;
        match &entry.effect {
            Effect::Damage { power } => {
                creatures.wound(id, feet, p.caster, power.at(rank) * SKILL_BASE_DAMAGE * damage_factor);
            }
            Effect::Slow { strength, duration } => creatures.slow(id, strength.at(rank), duration.at(rank)),
            Effect::Stun { duration } => creatures.stun(id, duration.at(rank)),
            Effect::Knockback { strength } => creatures.shove(id, feet, strength.at(rank)),
            Effect::Pull { strength } => creatures.shove(id, feet, -strength.at(rank)),
            Effect::OverTime { damage, duration, .. } => {
                if let Some(damage) = damage {
                    self.over_time.push(OverTime { caster: p.caster, target: SkillTarget::Creature(id), per_second: -damage.at(rank) * damage_factor, left: duration.at(rank), next: OVER_TIME_STEP });
                }
            }
            // Pas encore pour les créatures : soins, boucliers, renforcements, provocation
            Effect::Heal { .. } | Effect::Shield { .. } | Effect::Buff { .. } | Effect::Summon { .. } | Effect::Taunt { .. } => {}
        }
    }

    fn apply_to_player(&mut self, entry: &EffectEntry, p: &Pending, id: u64, players: &mut dyn SkillPlayers) {
        let rank = p.rank;
        match &entry.effect {
            Effect::Heal { amount } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.hp = (stats.hp + amount.at(rank)).min(stats.max_hp);
                }
            }
            Effect::Shield { amount, duration } => {
                if let Some(stats) = players.stats_mut(id) {
                    stats.shield = stats.shield.max(amount.at(rank));
                    stats.shield_time = stats.shield_time.max(duration.at(rank));
                }
            }
            Effect::Buff { stat, value, duration } => {
                self.buffs.retain(|b| !(b.player == id && b.stat == *stat));
                self.buffs.push(Buff { player: id, stat: *stat, value: value.at(rank), left: duration.at(rank) });
            }
            Effect::OverTime { heal, damage, duration } => {
                let per_second = heal.as_ref().map_or(0.0, |h| h.at(rank)) - damage.as_ref().map_or(0.0, |d| d.at(rank));
                if per_second != 0.0 {
                    self.over_time.push(OverTime { caster: p.caster, target: SkillTarget::Player(id), per_second, left: duration.at(rank), next: OVER_TIME_STEP });
                }
            }
            // Pas de dégâts entre joueurs ; ralentir ou repousser un joueur demanderait son client
            Effect::Damage { .. } | Effect::Slow { .. } | Effect::Stun { .. } | Effect::Knockback { .. } | Effect::Pull { .. } | Effect::Summon { .. } | Effect::Taunt { .. } => {}
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
    fn test_humanoid_summons_are_described_but_not_yet_playable() {
        let catalog = SkillCatalog::load_default();
        for id in ["deathKnight", "lichLord"] {
            let def = catalog.get(id).unwrap();
            assert_eq!(def.summons().count(), 1, "{id}");
            assert!(!def.is_castable(), "{id}");
        }
        for id in ["skeletonServant", "boneWall", "ballisticTurret", "etheriumBastion", "magmaWall"] {
            assert!(catalog.get(id).unwrap().is_castable(), "{id}");
        }
    }
}

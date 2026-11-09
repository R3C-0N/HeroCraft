//! Les compétences côté client : lancer, recharges affichées, geste du héros et frappes à montrer.
//!
//! En ligne, le serveur juge ; le client commence le geste aussitôt, sans attendre sa réponse, et
//! l'arrête si le lancement est refusé. En solo, le client exécute lui-même les règles
//! (`herocraft_world::SkillRunner`) sur ses propres créatures.

use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3;
use herocraft_common::skills::{skills, Anchor, Effect, Moment, SkillCatalog};
use herocraft_common::CharacterStats;
use herocraft_world::{CreatureWorld, SkillImpact, SkillRunner, SoloPlayer};

use crate::gestures::GestureLibrary;
use crate::hero_model::FigurePose;

/// Identifiant du joueur en solo
const SOLO_PLAYER: u64 = 0;

/// Une compétence qu'un joueur est en train de lancer (le geste et ses visuels). Rang et visée
/// serviront aux gestes des autres joueurs, quand ils seront dessinés en entier.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Play {
    /// `None` : le joueur local
    pub player: Option<u64>,
    pub skill: String,
    pub rank: u8,
    pub started: Instant,
    pub aim: Vec3,
    pub aimed: Option<Vec3>,
    /// Durée du geste, en secondes
    pub duration: f32,
}

impl Play {
    /// Avancement du geste, de 0 à 1
    pub fn progress(&self, now: Instant) -> f32 {
        (now.duration_since(self.started).as_secs_f32() / self.duration.max(1e-3)).min(1.0)
    }
}

/// Une frappe arrivée, à montrer : où, et qui elle a touché
#[derive(Debug, Clone)]
pub struct Shown {
    pub caster: Option<u64>,
    pub skill: String,
    pub rank: u8,
    pub center: Vec3,
    pub creatures: Vec<u64>,
    pub players: Vec<u64>,
}

/// Événement visuel d'une compétence, que le moteur d'effets joue
#[derive(Debug, Clone)]
pub struct VisualCue {
    pub effect: String,
    pub anchor: Anchor,
    /// Lanceur (`None` : le joueur local)
    pub caster: Option<u64>,
    /// Point de départ, pour les ancres fixes (zone, cible au moment du coup, point visé)
    pub point: Vec3,
    /// Créature suivie (ancre `target`)
    pub creature: Option<u64>,
    /// Direction visée
    pub aim: Vec3,
    /// Durée imposée (`to`), en secondes, sinon celle des couches de l'effet
    pub hold: Option<f32>,
    /// Taille de la zone de la compétence en blocs, pour les effets en `unit: area`
    pub area_blocks: f32,
}

/// L'état des compétences du joueur et de ce qui se joue autour de lui
pub struct SkillPlay {
    runner: SkillRunner,
    gestures: GestureLibrary,
    /// Fin de recharge de chaque compétence du joueur local
    ready_at: HashMap<String, Instant>,
    /// Recharge totale, pour dessiner la part écoulée
    cooldown_of: HashMap<String, f32>,
    plays: Vec<Play>,
    cues: Vec<VisualCue>,
    /// Dernier refus, à montrer au joueur
    pub last_refusal: Option<(String, Instant)>,
}

impl Default for SkillPlay {
    fn default() -> Self {
        Self {
            runner: SkillRunner::new(),
            gestures: GestureLibrary::load_default(),
            ready_at: HashMap::new(),
            cooldown_of: HashMap::new(),
            plays: Vec::new(),
            cues: Vec::new(),
            last_refusal: None,
        }
    }
}

impl SkillPlay {
    /// Recharge restante de `skill` (secondes) et sa durée totale
    pub fn cooldown(&self, skill: &str, now: Instant) -> (f32, f32) {
        let left = self.ready_at.get(skill).map_or(0.0, |t| t.saturating_duration_since(now).as_secs_f32());
        (left, self.cooldown_of.get(skill).copied().unwrap_or(0.0))
    }

    /// Le joueur local lance `skill`. En ligne (`online`), la demande part au serveur par
    /// `send` ; en solo, les règles s'appliquent ici. Renvoie l'erreur à afficher.
    pub fn request(&mut self, skill: &str, stats: &mut CharacterStats, aim: Vec3, aimed: Option<Vec3>, online: Option<&mut dyn FnMut(&str, Vec3, Option<Vec3>)>) -> Result<(), String> {
        let catalog = skills();
        let now = Instant::now();
        let (left, _) = self.cooldown(skill, now);
        if let Some(error) = catalog.cast_check(stats, skill, left) {
            return Err(error.message().to_string());
        }
        let rank = match online {
            Some(send) => {
                send(skill, aim, aimed);
                catalog.rank(stats, skill)
            }
            None => self.runner.cast(catalog, SOLO_PLAYER, stats, skill, aim, aimed).map_err(|e| e.message().to_string())?,
        };
        self.arm_cooldown(catalog, skill, rank, now);
        self.start(None, skill, rank, aim, aimed, now);
        Ok(())
    }

    fn arm_cooldown(&mut self, catalog: &SkillCatalog, skill: &str, rank: u8, now: Instant) {
        if let Some(cast) = catalog.get(skill).and_then(|s| s.cast.as_ref()) {
            let seconds = cast.cooldown.at(rank);
            self.cooldown_of.insert(skill.to_string(), seconds);
            self.ready_at.insert(skill.to_string(), now + std::time::Duration::from_secs_f32(seconds.max(0.0)));
        }
    }

    /// Le geste commence, et les visuels `start` avec lui
    fn start(&mut self, player: Option<u64>, skill: &str, rank: u8, aim: Vec3, aimed: Option<Vec3>, now: Instant) {
        let catalog = skills();
        let Some(def) = catalog.get(skill) else { return };
        let Some(cast) = &def.cast else { return };
        let duration = self.gestures.get(&cast.animation).map_or(0.6, |g| g.duration) / cast.speed;
        self.plays.retain(|p| p.player != player);
        self.plays.push(Play { player, skill: skill.to_string(), rank, started: now, aim, aimed, duration });
        let impact = catalog.impact_delay(cast);
        for visual in def.visuals.iter().filter(|v| v.from == Moment::Start) {
            let hold = match visual.to {
                Some(Moment::Impact) => Some(impact),
                Some(Moment::End) => Some(duration),
                Some(Moment::EffectEnd) => Some(effect_seconds(&cast.self_effects, rank).max(effect_seconds(&cast.effects, rank))),
                _ => None,
            };
            self.cues.push(VisualCue {
                effect: visual.effect.clone(),
                anchor: visual.anchor,
                caster: player,
                point: aimed.unwrap_or(Vec3::ZERO),
                creature: None,
                aim,
                hold,
                area_blocks: cast.area.size_blocks(rank),
            });
        }
    }

    /// Un joueur (peut-être nous) lance une compétence, d'après le serveur
    pub fn on_cast(&mut self, player: u64, me: u64, skill: &str, rank: u8, aim: Vec3, aimed: Option<Vec3>) {
        if player == me {
            // Notre geste a déjà commencé ; la recharge du serveur fait foi
            self.arm_cooldown(skills(), skill, rank, Instant::now());
            return;
        }
        self.start(Some(player), skill, rank, aim, aimed, Instant::now());
    }

    /// Le serveur refuse notre lancement : le geste s'arrête, la recharge aussi
    pub fn on_refused(&mut self, skill: &str, reason: &str) {
        self.plays.retain(|p| !(p.player.is_none() && p.skill == skill));
        self.ready_at.remove(skill);
        self.cues.retain(|c| c.caster.is_some());
        self.last_refusal = Some((reason.to_string(), Instant::now()));
    }

    /// Une compétence frappe (d'après le serveur, ou en solo d'après nos propres règles)
    pub fn on_hit(&mut self, shown: Shown) {
        let catalog = skills();
        let Some(def) = catalog.get(&shown.skill) else { return };
        let Some(cast) = &def.cast else { return };
        let effect_end = effect_seconds(&cast.effects, shown.rank);
        let self_end = effect_seconds(&cast.self_effects, shown.rank);
        for visual in &def.visuals {
            let hold = match visual.to {
                Some(Moment::EffectEnd) => Some(if visual.anchor == Anchor::Target { effect_end } else { self_end.max(effect_end) }),
                _ => None,
            };
            let base = VisualCue {
                effect: visual.effect.clone(),
                anchor: visual.anchor,
                caster: shown.caster,
                point: shown.center,
                creature: None,
                aim: Vec3::ZERO,
                hold,
                area_blocks: cast.area.size_blocks(shown.rank),
            };
            match visual.from {
                Moment::Impact => self.cues.push(base),
                Moment::Hit if visual.anchor == Anchor::Target => {
                    for &id in &shown.creatures {
                        self.cues.push(VisualCue { creature: Some(id), ..base.clone() });
                    }
                }
                // Une cible joueur : ses pieds, faute de mieux (les autres joueurs ne sont pas
                // encore dessinés en entier)
                Moment::Hit => {
                    if !shown.creatures.is_empty() || !shown.players.is_empty() {
                        self.cues.push(base);
                    }
                }
                _ => {}
            }
        }
    }

    /// Avance d'une image : en solo, les règles tournent ; renvoie les visuels à commencer
    pub fn tick(&mut self, dt: f32, solo: Option<(&mut CreatureWorld, Vec3, &mut CharacterStats)>) -> Vec<VisualCue> {
        let now = Instant::now();
        if let Some((creatures, feet, stats)) = solo {
            let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
            let tick = self.runner.tick(dt, skills(), creatures, &mut player);
            for SkillImpact { skill, rank, center, creatures, players, .. } in tick.impacts {
                self.on_hit(Shown { caster: None, skill, rank, center, creatures, players });
            }
        }
        self.plays.retain(|p| now.duration_since(p.started).as_secs_f32() < p.duration);
        self.ready_at.retain(|_, t| *t > now);
        std::mem::take(&mut self.cues)
    }

    /// Facteur des dégâts qu'encaisse le joueur solo (renforcement de défense)
    pub fn solo_damage_factor(&self) -> f32 {
        self.runner.damage_taken_factor(SOLO_PLAYER)
    }

    /// La pose du héros local : son geste en cours par-dessus la marche
    pub fn hero_pose(&self, swing: f32, units_per_block: f32) -> FigurePose {
        let now = Instant::now();
        let mut pose = FigurePose { swing, ..FigurePose::default() };
        let Some(play) = self.plays.iter().find(|p| p.player.is_none()) else { return pose };
        let Some(gesture) = skills().get(&play.skill).and_then(|s| s.cast.as_ref()).and_then(|c| self.gestures.get(&c.animation)) else { return pose };
        if gesture.legs_locked {
            pose.swing = 0.0;
        } else {
            // Les bras font le geste : seul un balancement léger reste
            pose.swing = swing * 0.3;
        }
        gesture.pose(play.progress(now), units_per_block, &mut pose);
        pose
    }

    /// Le geste du joueur local, s'il en fait un
    pub fn local_play(&self) -> Option<&Play> {
        self.plays.iter().find(|p| p.player.is_none())
    }
}

/// Durée la plus longue des effets qui durent (ralenti, étourdi, bouclier, renforcement…)
fn effect_seconds(effects: &[herocraft_common::skills::EffectEntry], rank: u8) -> f32 {
    effects
        .iter()
        .map(|e| match &e.effect {
            Effect::Slow { duration, .. } | Effect::Stun { duration } | Effect::Shield { duration, .. } | Effect::Buff { duration, .. } | Effect::OverTime { duration, .. } | Effect::Taunt { duration } => duration.at(rank),
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

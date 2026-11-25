//! Les compétences côté client : lancer, recharges affichées, geste du héros et frappes à montrer.
//!
//! En ligne, le serveur juge ; le client commence le geste aussitôt, sans attendre sa réponse, et
//! l'arrête si le lancement est refusé. En solo, le client exécute lui-même les règles
//! (`herocraft_world::SkillRunner`) sur ses propres créatures.

use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3;
use herocraft_common::skills::{skills, Anchor, Effect, GrappleTo, Moment, SkillCatalog, SkillMove};
use herocraft_common::{CastError, CharacterStats};
use herocraft_world::{CreatureWorld, SkillImpact, SkillRunner, SoloPlayer};

use crate::gestures::GestureLibrary;
use crate::hero_model::FigurePose;

/// Le grappin de la compétence `skill`, s'il en est un
pub(crate) fn grapple_of<'a>(catalog: &'a SkillCatalog, skill: &str) -> Option<&'a herocraft_common::skills::Grapple> {
    catalog.get(skill)?.cast.as_ref()?.movement.as_ref()?.grapple.as_ref()
}

/// Identifiant du joueur en solo
pub(crate) const SOLO_PLAYER: herocraft_common::PlayerId = herocraft_common::PlayerId(0);

/// Pourquoi le joueur local ne lance pas une compétence : le message est affiché tel quel
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CastRefusal {
    #[error(transparent)]
    Cast(#[from] CastError),
    /// Pas la bonne arme en main : les armes qui conviennent
    #[error("il te faut {}", .0.join(" ou "))]
    Weapon(Vec<&'static str>),
}

/// En ligne, l'envoi d'une demande de lancer au serveur (compétence, visée, cible visée)
pub(crate) type OnlineSend<'a> = &'a mut dyn FnMut(&str, Vec3, Option<Vec3>);

/// Une compétence qu'un joueur est en train de lancer (le geste et ses visuels). Rang et visée
/// serviront aux gestes des autres joueurs, quand ils seront dessinés en entier.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct Play {
    /// `None` : le joueur local
    pub player: Option<herocraft_common::PlayerId>,
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
    pub(crate) fn progress(&self, now: Instant) -> f32 {
        (now.duration_since(self.started).as_secs_f32() / self.duration.max(1e-3)).min(1.0)
    }
}

/// Une frappe arrivée, à montrer : où, et qui elle a touché
#[derive(Debug, Clone)]
pub(crate) struct Shown {
    pub caster: Option<herocraft_common::PlayerId>,
    pub skill: String,
    pub rank: u8,
    pub center: Vec3,
    pub creatures: Vec<herocraft_common::CreatureId>,
    pub players: Vec<herocraft_common::PlayerId>,
    /// La touche d'un projectile en vol : seuls les visuels « hit » jouent
    pub contact: bool,
    /// La riposte d'une garde : seuls les visuels de parade se jouent
    pub parried: bool,
}

/// Un projectile en vol vu du client : dernière position connue, vitesse, depuis quand
#[derive(Debug, Clone, Copy)]
struct LiveShot {
    at: Vec3,
    velocity: Vec3,
    since: Instant,
}

/// Secondes au plus qu'un visuel suit son projectile (il s'achève à son arrêt)
const PROJECTILE_HOLD: f32 = 8.0;

/// Événement visuel d'une compétence, que le moteur d'effets joue
#[derive(Debug, Clone)]
pub(crate) struct VisualCue {
    pub effect: String,
    pub anchor: Anchor,
    /// Lanceur (`None` : le joueur local)
    pub caster: Option<herocraft_common::PlayerId>,
    /// Point de départ, pour les ancres fixes (zone, cible au moment du coup, point visé)
    pub point: Vec3,
    /// Créature suivie (ancre `target`)
    pub creature: Option<herocraft_common::CreatureId>,
    /// Direction visée
    pub aim: Vec3,
    /// Durée imposée (`to`), en secondes, sinon celle des couches de l'effet
    pub hold: Option<f32>,
    /// Taille de la zone de la compétence en blocs, pour les effets en `unit: area`
    pub area_blocks: f32,
    /// Projectile : l'ancre file de `point` à ce point en tant de secondes
    pub travel: Option<(Vec3, f32)>,
    /// Secondes avant que l'effet commence
    pub delay: f32,
    /// Projectile suivi (ancre `projectile`)
    pub projectile: Option<herocraft_common::ProjectileId>,
}

/// Un déplacement de compétence que la caméra du joueur local joue
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Motion {
    /// Lévitation : monter de `height` blocs et flotter `duration` secondes
    Hover { height: f32, duration: f32 },
    /// Traction en ligne droite vers `to`, à `speed` blocs par seconde, jusqu'à `stop` blocs ; avec
    /// `hoist`, il se hisse sur le rebord qui l'arrête
    Pull { to: Vec3, speed: f32, stop: f32, hoist: bool },
    /// Téléportation : les pieds en `feet`, le regard vers `look`
    Teleport { feet: Vec3, look: Vec3 },
}

impl From<SkillMove> for Motion {
    fn from(m: SkillMove) -> Self {
        match m {
            SkillMove::Teleport { feet, look } => Motion::Teleport { feet, look },
            SkillMove::Pull { to, speed, stop } => Motion::Pull { to, speed, stop, hoist: false },
        }
    }
}

/// L'état des compétences du joueur et de ce qui se joue autour de lui
pub(crate) struct SkillPlay {
    runner: SkillRunner,
    /// Un saignement vient de tuer le joueur solo (`take_solo_death`)
    solo_died: bool,
    gestures: GestureLibrary,
    /// Fin de recharge de chaque compétence du joueur local
    ready_at: HashMap<String, Instant>,
    /// Recharge totale, pour dessiner la part écoulée
    cooldown_of: HashMap<String, f32>,
    plays: Vec<Play>,
    cues: Vec<VisualCue>,
    /// Pièces greffées : sur qui (`None` : le joueur local), laquelle, jusqu'à quand
    grafts: Vec<(Option<herocraft_common::PlayerId>, String, Instant)>,
    /// Formes complètes (`SkillDef::form`) : sur qui, par quelle compétence, jusqu'à quand
    forms: Vec<(Option<herocraft_common::PlayerId>, String, Instant)>,
    /// Lancements amplifiés par la Fusion, en attente de leur frappe : qui, quoi, jusqu'à quand
    amplified: Vec<(Option<herocraft_common::PlayerId>, String, Instant)>,
    /// Renforcements de vitesse du joueur local : de combien, jusqu'à quand
    speed_buffs: Vec<(f32, Instant, bool)>,
    /// Élan de la compétence locale qui commence : ruée, bond, bond de côté, en combien de secondes
    motion: Option<(f32, f32, f32, f32)>,
    /// Déplacements du joueur local à jouer, et à partir de quand : la compétence et son déplacement
    moves: Vec<(Instant, String, Motion)>,
    /// Identifiant du joueur local (en ligne, celui du serveur)
    pub me: herocraft_common::PlayerId,
    /// Dernier refus, à montrer au joueur
    pub last_refusal: Option<(String, Instant)>,
    /// Projectiles en vol, que le client fait voler entre deux nouvelles du serveur
    live: HashMap<herocraft_common::ProjectileId, LiveShot>,
    /// Projectiles arrêtés dont les visuels doivent s'achever (`take_ended_projectiles`)
    ended: Vec<herocraft_common::ProjectileId>,
    /// Qui a tiré chaque projectile en vol, et par quelle compétence
    shooters: HashMap<herocraft_common::ProjectileId, (herocraft_common::Shooter, String)>,
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
            grafts: Vec::new(),
            forms: Vec::new(),
            amplified: Vec::new(),
            speed_buffs: Vec::new(),
            solo_died: false,
            motion: None,
            moves: Vec::new(),
            me: SOLO_PLAYER,
            last_refusal: None,
            live: HashMap::new(),
            ended: Vec::new(),
            shooters: HashMap::new(),
        }
    }
}

impl SkillPlay {
    /// Recharge restante de `skill` (secondes) et sa durée totale
    pub(crate) fn cooldown(&self, skill: &str, now: Instant) -> (f32, f32) {
        let left = self.ready_at.get(skill).map_or(0.0, |t| t.saturating_duration_since(now).as_secs_f32());
        (left, self.cooldown_of.get(skill).copied().unwrap_or(0.0))
    }

    /// La recharge de `skill` dure `seconds` de plus (une parade ratée)
    pub(crate) fn extend_cooldown(&mut self, skill: &str, seconds: f32) {
        let now = Instant::now();
        let ready = self.ready_at.get(skill).copied().filter(|t| *t > now).unwrap_or(now);
        let left = ready.saturating_duration_since(now).as_secs_f32() + seconds.max(0.0);
        self.ready_at.insert(skill.to_string(), now + std::time::Duration::from_secs_f32(left));
        let total = self.cooldown_of.get(skill).copied().unwrap_or(0.0) + seconds.max(0.0);
        self.cooldown_of.insert(skill.to_string(), total.max(left));
    }

    /// Le joueur local lance `skill`. En ligne (`online`), la demande part au serveur par
    /// `send` ; en solo, les règles s'appliquent ici. Renvoie l'erreur à afficher.
    pub(crate) fn request(&mut self, skill: &str, stats: &mut CharacterStats, aim: Vec3, aimed: Option<Vec3>, online: Option<OnlineSend<'_>>) -> Result<(), CastRefusal> {
        let catalog = skills();
        let now = Instant::now();
        let requested = skill;
        // La Fusion lance le mélange des deux derniers sorts élémentaires (ou le dernier amplifié)
        let fused = match catalog.get(requested).and_then(|d| d.fusion.as_ref()) {
            Some(_) if catalog.rank(stats, requested) == 0 => return Err(CastError::NotLearned.into()),
            Some(_) => Some(self.runner.fusion_preview(catalog, SOLO_PLAYER, stats)?),
            None if catalog.fusion_of_mix(requested).is_some() => return Err(CastError::FusionOnly.into()),
            None => None,
        };
        let skill = fused.as_ref().map_or(requested, |f| f.skill());
        let amplified = matches!(fused, Some(herocraft_common::skills::Fused::Amplified { .. }));
        let (left, _) = self.cooldown(requested, now);
        if let Some(error) = catalog.cast_check(stats, skill, left) {
            return Err(error.into());
        }
        if stats.is_stunned() {
            return Err(CastError::Stunned.into());
        }
        if grapple_of(catalog, skill).is_some_and(|g| g.to == GrappleTo::Block) && aimed.is_none() {
            return Err(CastError::NothingToGrab.into());
        }
        // Changé en bête, seules ses deux attaques ; et elles seulement sous elle
        let form = self.form_skill(None);
        match (catalog.form_of_attack(skill), form) {
            (Some(of), Some(current)) if of.id == current => {}
            (Some(_), _) => return Err(CastError::NotInForm.into()),
            (None, Some(_)) => return Err(CastError::InForm.into()),
            (None, None) => {}
        }
        // L'arme en main (jugée aussi par le serveur ; ici pour répondre tout de suite)
        if let Some(cast) = catalog.get(skill).and_then(|d| d.cast.as_ref()) {
            let held = self.runner.arsenal().held(SOLO_PLAYER);
            if !cast.weapons.is_empty() && !held.kind.is_some_and(|k| cast.weapons.contains(&k)) {
                return Err(CastRefusal::Weapon(cast.weapons.iter().map(|w| w.name()).collect()));
            }
            if cast.uses_round && self.runner.arsenal().magazine(SOLO_PLAYER).is_none_or(|m| m.loaded == 0) {
                return Err(CastError::Reloading.into());
            }
            if let Some(charge) = &cast.charge
                && charge.power_after(self.runner.arsenal().charge_held(SOLO_PLAYER, Some(requested))).is_none() {
                self.runner.arsenal_mut().release_charge(SOLO_PLAYER, Some(requested));
                return Err(CastError::NotCharged.into());
            }
        }
        let rank = match online {
            Some(send) => {
                send(requested, aim, aimed);
                self.predict_weapon(catalog, requested, skill);
                // Le serveur tient le compte des sorts à mêler ; on le tient aussi pour prévoir
                self.runner.remember(catalog, SOLO_PLAYER, requested);
                catalog.rank(stats, skill)
            }
            None => self.runner.cast_as(catalog, SOLO_PLAYER, stats, requested, aim, aimed)?.rank,
        };
        self.arm_cooldown(catalog, requested, skill, rank, now);
        self.start(None, skill, rank, aim, aimed, now, amplified);
        Ok(())
    }

    /// En ligne, ce que notre lancement fait à notre arme, pour l'afficher sans attendre le serveur :
    /// charge relâchée, carreau tiré, chargeur rempli, munition spéciale chargée
    fn predict_weapon(&mut self, catalog: &SkillCatalog, requested: &str, skill: &str) {
        let Some(cast) = catalog.get(skill).and_then(|d| d.cast.as_ref()) else { return };
        let arsenal = self.runner.arsenal_mut();
        arsenal.release_charge(SOLO_PLAYER, Some(requested));
        if cast.uses_round {
            let _ = arsenal.use_round(SOLO_PLAYER);
        }
        for entry in &cast.self_effects {
            match &entry.effect {
                Effect::Reload {} => arsenal.reload_now(SOLO_PLAYER),
                Effect::LoadAmmo { shots, duration, radius, effects } => arsenal.load(
                    SOLO_PLAYER,
                    herocraft_world::Special { skill: skill.to_string(), rank: 1, shots: shots.at(1).round().max(1.0) as u32, left: duration.at(1), radius: radius.as_ref().map(|r| r.at(1)), effects: effects.clone() },
                ),
                _ => {}
            }
        }
    }

    /// Le joueur local tient `item` en main (`None` : mains nues)
    pub(crate) fn hold(&mut self, item: Option<herocraft_common::ItemId>, class: herocraft_common::PlayerClass) {
        self.runner.hold(SOLO_PLAYER, item, class);
    }

    /// Ce que vaut au combat l'objet tenu
    pub(crate) fn held(&self) -> herocraft_common::Held {
        self.runner.arsenal().held(SOLO_PLAYER)
    }

    /// Un coup au corps à corps du joueur local : ses dégâts de base, s'il n'est pas trop tôt
    pub(crate) fn strike(&mut self) -> Result<f32, CastError> {
        self.runner.arsenal_mut().strike(SOLO_PLAYER)
    }

    /// Le joueur local commence à bander son arc (`None`) ou à charger une compétence
    pub(crate) fn begin_charge(&mut self, skill: Option<&str>) {
        self.runner.arsenal_mut().begin_charge(SOLO_PLAYER, skill);
    }

    /// Il bande son arc, ou charge cette compétence
    pub(crate) fn is_charging(&self, skill: Option<&str>) -> bool {
        self.runner.arsenal().is_charging(SOLO_PLAYER, skill)
    }

    /// Le tir du joueur local part (arc relâché, arbalète) : refusé trop tôt, l'arbalète vide, l'arc
    /// pas assez bandé
    pub(crate) fn fire(&mut self) -> Result<herocraft_world::Fired, CastError> {
        self.runner.arsenal_mut().fire(SOLO_PLAYER)
    }

    /// En solo, le tir `fired` part des pieds `feet` dans la direction `aim` avec `damage` points
    pub(crate) fn solo_shoot(&mut self, feet: Vec3, aim: Vec3, damage: f32, fired: herocraft_world::Fired, creatures: &mut CreatureWorld, stats: &mut CharacterStats) -> herocraft_world::ShotHit {
        let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
        self.runner.shoot(SOLO_PLAYER, feet, aim, damage, fired, creatures, &mut player)
    }

    /// Un tir à montrer : la traînée qui file de `from` à `to`, l'étincelle sur la bête touchée, et
    /// ce que fait la munition spéciale à l'impact (nuage de poison, explosion)
    pub(crate) fn show_shot(&mut self, caster: Option<herocraft_common::PlayerId>, from: Vec3, to: Vec3, target: Option<herocraft_common::CreatureId>, special: Option<&str>, block: f32) {
        let flight = (from.distance(to) / block / 60.0).clamp(0.05, 0.5);
        let base = VisualCue { effect: "arrowTrail".into(), anchor: Anchor::Aimed, caster, point: from, creature: None, projectile: None, aim: to - from, hold: Some(flight), area_blocks: 1.0, travel: Some((to, flight)), delay: 0.0 };
        self.cues.push(base.clone());
        let Some(id) = target else { return };
        self.cues.push(VisualCue { effect: "impactSpark".into(), anchor: Anchor::Target, point: to, creature: Some(id), travel: None, hold: None, delay: flight, ..base.clone() });
        let Some(def) = special.and_then(|s| skills().get(s)) else { return };
        let radius = def.cast.as_ref().into_iter().flat_map(|c| &c.self_effects).find_map(|e| match &e.effect {
            Effect::LoadAmmo { radius, .. } => radius.as_ref().map(|r| r.at(1)),
            _ => None,
        });
        for visual in def.visuals.iter().filter(|v| v.from == Moment::Hit) {
            self.cues.push(VisualCue {
                effect: visual.effect.clone(),
                anchor: visual.anchor,
                point: to,
                creature: (visual.anchor == Anchor::Target).then_some(id),
                area_blocks: radius.unwrap_or(1.0),
                travel: None,
                hold: None,
                delay: flight,
                ..base.clone()
            });
        }
    }

    /// Ce que l'affichage montre de l'arme : chargeur, munition spéciale, arc bandé ou compétence chargée
    pub(crate) fn weapon_hud(&self, charging: Option<&str>) -> crate::ui::WeaponHud {
        let catalog = skills();
        let arsenal = self.runner.arsenal();
        let magazine = arsenal.magazine(SOLO_PLAYER).map(|m| (m.loaded, m.size, m.reload_left, m.reload_total));
        let special = arsenal.special(SOLO_PLAYER).map(|(skill, shots)| (catalog.get(skill).map_or(skill.to_string(), |d| d.name.clone()), shots));
        let charge = match charging {
            Some(skill) if arsenal.is_charging(SOLO_PLAYER, Some(skill)) => catalog.get(skill).and_then(|d| d.cast.as_ref().and_then(|c| c.charge.as_ref()).map(|c| (d, c))).map(|(def, c)| {
                let held = arsenal.charge_held(SOLO_PLAYER, Some(skill));
                (def.name.clone(), (held / c.max.max(1e-3)).min(1.0), c.power_after(held).is_some())
            }),
            _ if arsenal.is_charging(SOLO_PLAYER, None) => {
                let held = arsenal.charge_held(SOLO_PLAYER, None);
                Some(("Arc bandé".to_string(), (held / herocraft_common::weapons::BOW_FULL_DRAW).min(1.0), herocraft_common::weapons::bow_power(held).is_some()))
            }
            _ => None,
        };
        crate::ui::WeaponHud { magazine, special, charge }
    }

    /// La recharge de la case `key` (la Fusion, ou la compétence elle-même), celle de `skill`
    fn arm_cooldown(&mut self, catalog: &SkillCatalog, key: &str, skill: &str, rank: u8, now: Instant) {
        if let Some(cast) = catalog.get(skill).and_then(|s| s.cast.as_ref()) {
            let seconds = cast.cooldown.at(rank);
            self.cooldown_of.insert(key.to_string(), seconds);
            self.ready_at.insert(key.to_string(), now + std::time::Duration::from_secs_f32(seconds.max(0.0)));
        }
    }

    /// Ce que montre la case de la Fusion : l'icône du mélange qu'elle lancerait (`amplify:<élément>`
    /// pour une amplification), ou la sienne ; grisée sans mélange appris ou de quoi le payer
    pub(crate) fn fusion_face(&self, stats: &CharacterStats) -> Option<(String, bool)> {
        let catalog = skills();
        let fusion = catalog.fusion()?;
        let face = match self.runner.recent_spells(SOLO_PLAYER) {
            [first, last] => catalog.fuse(first, last),
            _ => None,
        };
        let unlearned = catalog.rank(stats, &fusion.id) == 0;
        Some(match face {
            Some(herocraft_common::skills::Fused::Amplified { element, spell }) => (format!("amplify:{element}"), unlearned || catalog.cast_check(stats, &spell, 0.0).is_some()),
            Some(herocraft_common::skills::Fused::Mix(mix)) => {
                let blocked = unlearned || catalog.cast_check(stats, &mix, 0.0).is_some();
                (mix, blocked)
            }
            None => (fusion.id.clone(), true),
        })
    }

    /// Le geste commence, et les visuels `start` avec lui (et ceux de l'amplification, `amplified`)
    fn start(&mut self, player: Option<herocraft_common::PlayerId>, skill: &str, rank: u8, aim: Vec3, aimed: Option<Vec3>, now: Instant, amplified: bool) {
        let catalog = skills();
        let Some(def) = catalog.get(skill) else { return };
        let Some(cast) = &def.cast else { return };
        let amplify = catalog.fusion().and_then(|f| f.fusion.as_ref()).map(|f| f.amplify.clone()).filter(|_| amplified);
        let size = amplify.as_ref().map_or(1.0, |a| a.area);
        if amplified {
            self.amplified.retain(|(who, _, _)| *who != player);
            self.amplified.push((player, skill.to_string(), now + std::time::Duration::from_secs_f32(5.0)));
        }
        let duration = self.gestures.get(&cast.animation).map_or(0.6, |g| g.duration) / cast.speed;
        self.plays.retain(|p| p.player != player);
        self.plays.push(Play { player, skill: skill.to_string(), rank, started: now, aim, aimed, duration });
        let impact = catalog.impact_delay(cast);
        // Transformation partielle du Druide : la pièce reste tant que durent ses effets
        if let Some(graft) = def.graft.as_ref() {
            let seconds = impact + effect_seconds(&cast.self_effects, rank).max(effect_seconds(&cast.effects, rank)).max(duration);
            self.grafts.retain(|(who, g, _)| !(*who == player && g == graft));
            self.grafts.push((player, graft.clone(), now + std::time::Duration::from_secs_f32(seconds)));
        }
        // Forme complète du Druide : la bête remplace le héros tant que durent ses effets
        if def.form.is_some() {
            let seconds = impact + lasting_seconds(cast, rank);
            self.forms.retain(|(who, _, _)| *who != player);
            self.forms.push((player, skill.to_string(), now + std::time::Duration::from_secs_f32(seconds)));
        }
        // Ruée ou bond du joueur local, jusqu'à l'impact
        if let Some(movement) = cast.movement.as_ref().filter(|_| player.is_none()) {
            let seconds = if impact >= 0.3 * duration { impact } else { 0.6 * duration };
            let at = |r: &Option<herocraft_common::skills::Ranked>| r.as_ref().map_or(0.0, |d| d.at(rank));
            self.motion = Some((at(&movement.dash), at(&movement.leap), at(&movement.side), seconds));
            if let Some(hover) = &movement.hover {
                self.moves.push((now, skill.to_string(), Motion::Hover { height: hover.height.at(rank), duration: hover.duration.at(rank) }));
            }
            // Le grappin part à l'impact du geste, vers le bloc visé
            if let (Some(grapple), Some(to)) = (movement.grapple.as_ref().filter(|g| g.to == GrappleTo::Block), aimed) {
                let at = now + std::time::Duration::from_secs_f32(impact.max(0.0));
                self.moves.push((at, skill.to_string(), Motion::Pull { to, speed: grapple.speed.at(rank), stop: grapple.stop.at(rank), hoist: true }));
            }
        }
        for visual in def.visuals.iter().chain(amplify.iter().flat_map(|a| &a.visuals)).filter(|v| v.from == Moment::Start) {
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
                area_blocks: cast.area.size_blocks(rank) * size,
                travel: None,
                delay: 0.0,
                projectile: None,
            });
        }
    }

    /// Un joueur (peut-être nous) lance une compétence, d'après le serveur
    pub(crate) fn on_cast(&mut self, player: herocraft_common::PlayerId, me: herocraft_common::PlayerId, skill: &str, rank: u8, aim: Vec3, aimed: Option<Vec3>, fused: bool, amplified: bool) {
        if player == me {
            // Notre geste a déjà commencé ; la recharge du serveur fait foi (celle de la Fusion pour un mélange)
            let catalog = skills();
            let key = if fused { catalog.fusion().map_or(skill, |f| f.id.as_str()) } else { skill };
            self.arm_cooldown(catalog, key, skill, rank, Instant::now());
            return;
        }
        self.start(Some(player), skill, rank, aim, aimed, Instant::now(), amplified);
    }

    /// Le serveur refuse notre lancement : le geste s'arrête, la recharge aussi
    pub(crate) fn on_refused(&mut self, skill: &str, reason: &str) {
        self.plays.retain(|p| !(p.player.is_none() && p.skill == skill));
        self.moves.retain(|m| m.1 != skill);
        self.ready_at.remove(skill);
        self.cues.retain(|c| c.caster.is_some());
        self.last_refusal = Some((reason.to_string(), Instant::now()));
    }

    /// Des créatures tirent : la bouche du tir, le projectile qui file, l'explosion à l'arrivée
    pub(crate) fn on_shots(&mut self, shots: &[herocraft_common::CreatureShotEvent], registry: &herocraft_common::CreatureRegistry) {
        for event in shots {
            let attack = registry.get(event.species as usize).and_then(|s| s.attack.as_ref());
            // Un piège qui explose
            if let Some(burst) = attack.and_then(|a| a.burst.as_ref()) {
                if let Some(effect) = &burst.effect {
                    self.cues.push(VisualCue {
                        effect: effect.clone(),
                        anchor: Anchor::Area,
                        caster: None,
                        point: event.to,
                        creature: None,
                        aim: Vec3::ZERO,
                        hold: None,
                        area_blocks: burst.radius.max(1.0),
                        travel: None,
                        delay: 0.0,
                        projectile: None,
                    });
                }
                continue;
            }
            let Some(shot) = attack.and_then(|a| a.shot.as_ref()) else { continue };
            let base = VisualCue {
                effect: String::new(),
                anchor: Anchor::Aimed,
                caster: None,
                point: event.from,
                creature: None,
                aim: event.to - event.from,
                hold: None,
                area_blocks: shot.radius.max(1.0),
                travel: None,
                delay: 0.0,
                projectile: None,
            };
            if let Some(effect) = &shot.muzzle {
                self.cues.push(VisualCue { effect: effect.clone(), hold: Some(0.5), ..base.clone() });
            }
            // Le projectile et son arrivée suivent les nouvelles du vol (`on_projectiles`)
        }
    }

    /// Projectiles partis, déviés ou arrêtés (joueurs et créatures)
    pub(crate) fn on_projectiles(&mut self, events: &[herocraft_common::ProjectileEvent], registry: &herocraft_common::CreatureRegistry) {
        use herocraft_common::{ProjectileEnd, ProjectileEvent, Shooter};
        let now = Instant::now();
        for event in events {
            match event {
                ProjectileEvent::Launch { id, shooter, skill, from, velocity } => {
                    self.live.insert(*id, LiveShot { at: *from, velocity: *velocity, since: now });
                    let base = VisualCue {
                        effect: String::new(),
                        anchor: Anchor::Projectile,
                        caster: None,
                        point: *from,
                        creature: None,
                        aim: *velocity,
                        hold: Some(PROJECTILE_HOLD),
                        area_blocks: 1.0,
                        travel: None,
                        delay: 0.0,
                        projectile: Some(*id),
                    };
                    match shooter {
                        Shooter::Creature { species, .. } => {
                            let shot = registry.get(*species as usize).and_then(|s| s.attack.as_ref()).and_then(|a| a.shot.as_ref());
                            if let Some(effect) = shot.and_then(|s| s.projectile.as_ref()).filter(|_| shot.is_some_and(|s| s.speed > 0.0)) {
                                self.cues.push(VisualCue { effect: effect.clone(), area_blocks: shot.map_or(1.0, |s| s.radius.max(1.0)), ..base });
                            }
                        }
                        Shooter::Player(player) => {
                            let caster = (*player != self.me).then_some(*player);
                            let Some(def) = skills().get(skill) else { continue };
                            for visual in def.visuals.iter().filter(|v| v.anchor == Anchor::Projectile && v.from == Moment::Impact) {
                                self.cues.push(VisualCue { effect: visual.effect.clone(), caster, ..base.clone() });
                            }
                        }
                    }
                }
                ProjectileEvent::Turn { id, at, velocity } => {
                    self.live.insert(*id, LiveShot { at: *at, velocity: *velocity, since: now });
                }
                ProjectileEvent::End { id, at, reason } => {
                    self.live.remove(id);
                    self.ended.push(*id);
                    let fixed = VisualCue {
                        effect: String::new(),
                        anchor: Anchor::Aimed,
                        caster: None,
                        point: *at,
                        creature: None,
                        aim: Vec3::ZERO,
                        hold: None,
                        area_blocks: 1.0,
                        travel: None,
                        delay: 0.0,
                        projectile: None,
                    };
                    // Ce qui a tiré : on le retrouve par le départ, gardé dans les visuels
                    let shooter = events.iter().find_map(|e| match e {
                        ProjectileEvent::Launch { id: launched, shooter, skill, .. } if launched == id => Some((*shooter, skill.clone())),
                        _ => None,
                    });
                    let shooter = shooter.or_else(|| self.shooters.get(id).cloned());
                    match shooter {
                        Some((Shooter::Creature { species, .. }, _)) => {
                            let shot = registry.get(species as usize).and_then(|s| s.attack.as_ref()).and_then(|a| a.shot.as_ref());
                            if let Some(effect) = shot.and_then(|s| s.impact.as_ref()) {
                                self.cues.push(VisualCue { effect: effect.clone(), area_blocks: shot.map_or(1.0, |s| s.radius.max(1.0)), ..fixed });
                            }
                        }
                        Some((Shooter::Player(_), skill)) if matches!(reason, ProjectileEnd::Blocked | ProjectileEnd::Terrain) => {
                            if let Some(def) = skills().get(&skill) {
                                for visual in def.visuals.iter().filter(|v| v.from == Moment::Blocked) {
                                    self.cues.push(VisualCue { effect: visual.effect.clone(), ..fixed.clone() });
                                }
                            }
                        }
                        _ => {}
                    }
                    self.shooters.remove(id);
                }
            }
            if let ProjectileEvent::Launch { id, shooter, skill, .. } = event {
                self.shooters.insert(*id, (*shooter, skill.clone()));
            }
        }
        // Un projectile dont on n'a plus de nouvelles depuis longtemps est oublié
        self.live.retain(|_, l| now.duration_since(l.since).as_secs_f32() < PROJECTILE_HOLD);
    }

    /// Où est le projectile `id` en ce moment, d'après ce que le serveur en a dit
    pub(crate) fn projectile_at(&self, id: herocraft_common::ProjectileId) -> Option<Vec3> {
        let l = self.live.get(&id)?;
        Some(l.at + l.velocity * Instant::now().duration_since(l.since).as_secs_f32())
    }

    /// Projectiles arrêtés depuis le dernier appel : leurs visuels s'achèvent
    pub(crate) fn take_ended_projectiles(&mut self) -> Vec<herocraft_common::ProjectileId> {
        std::mem::take(&mut self.ended)
    }

    /// Une compétence frappe (d'après le serveur, ou en solo d'après nos propres règles)
    pub(crate) fn on_hit(&mut self, shown: Shown) {
        let catalog = skills();
        let Some(def) = catalog.get(&shown.skill) else { return };
        let Some(cast) = &def.cast else { return };
        // La touche d'un projectile en vol : ses visuels « hit » seuls, sur la cible touchée
        if shown.contact {
            let effect_end = effect_seconds(&cast.effects, shown.rank);
            for visual in def.visuals.iter().filter(|v| v.from == Moment::Hit) {
                let hold = (visual.to == Some(Moment::EffectEnd)).then_some(effect_end);
                let base = VisualCue {
                    effect: visual.effect.clone(),
                    anchor: if visual.anchor == Anchor::Target { Anchor::Target } else { Anchor::Aimed },
                    caster: shown.caster,
                    point: shown.center,
                    creature: None,
                    aim: Vec3::ZERO,
                    hold,
                    area_blocks: cast.area.size_blocks(shown.rank),
                    travel: None,
                    delay: 0.0,
                    projectile: None,
                };
                if visual.anchor == Anchor::Target {
                    for &id in &shown.creatures {
                        self.cues.push(VisualCue { creature: Some(id), ..base.clone() });
                    }
                } else {
                    self.cues.push(base);
                }
            }
            return;
        }
        let now = Instant::now();
        self.amplified.retain(|(_, _, until)| *until > now);
        let who = shown.caster.filter(|&c| c != self.me);
        let amplified = self.amplified.iter().position(|(p, s, _)| *p == who && *s == shown.skill).map(|i| self.amplified.remove(i)).is_some();
        let amplify = catalog.fusion().and_then(|f| f.fusion.as_ref()).map(|f| f.amplify.clone()).filter(|_| amplified);
        let size = amplify.as_ref().map_or(1.0, |a| a.area);
        // Une zone qui reste se montre tant qu'elle dure
        let effect_end = effect_seconds(&cast.effects, shown.rank).max(cast.zone.as_ref().map_or(0.0, |z| z.duration.at(shown.rank)));
        let self_end = effect_seconds(&cast.self_effects, shown.rank);
        // Renforcements de vitesse qui nous touchent : les nôtres, et ceux d'un allié sur nous
        let mine = shown.caster.is_none_or(|c| c == self.me);
        let on_me = shown.players.contains(&self.me);
        let now = Instant::now();
        for (entry, applies) in cast.self_effects.iter().map(|e| (e, mine && !shown.parried)).chain(cast.effects.iter().map(|e| (e, on_me && e.to.unwrap_or(cast.targets) != herocraft_common::skills::Targets::Enemies))) {
            if let (true, Effect::Buff { stat: herocraft_common::skills::BuffStat::Speed, value, duration, per_missing_health, .. }) = (applies, &entry.effect) {
                self.speed_buffs.push((value.at(shown.rank), now + std::time::Duration::from_secs_f32(duration.at(shown.rank).max(0.0)), *per_missing_health));
            }
        }
        for visual in def.visuals.iter().chain(amplify.iter().flat_map(|a| &a.visuals)) {
            // Une riposte ne joue que les visuels de parade, une frappe tous les autres
            if shown.parried != (visual.from == Moment::Parry) {
                continue;
            }
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
                area_blocks: cast.area.size_blocks(shown.rank) * size,
                travel: None,
                delay: 0.0,
                projectile: None,
            };
            match visual.from {
                // Ceux du projectile partent avec lui (`on_projectiles`)
                Moment::Impact if visual.anchor == Anchor::Projectile => {}
                Moment::Impact => self.cues.push(base),
                Moment::Hit | Moment::Parry if visual.anchor == Anchor::Target => {
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
                Moment::Parry => self.cues.push(base),
                _ => {}
            }
        }
    }

    /// Avance d'une image : en solo, les règles tournent ; renvoie les visuels à commencer
    pub(crate) fn tick(&mut self, dt: f32, solo: Option<(&mut CreatureWorld, Vec3, &mut CharacterStats)>) -> Vec<VisualCue> {
        let now = Instant::now();
        let ran_rules = solo.is_some();
        if let Some((creatures, feet, stats)) = solo {
            let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
            let tick = self.runner.tick(dt, skills(), creatures, &mut player);
            self.solo_died |= !tick.died.is_empty();
            for (_, m) in tick.moves.into_iter().filter(|m| m.0 == SOLO_PLAYER) {
                self.on_move(m);
            }
            for (_, skill, seconds) in tick.missed.iter().filter(|m| m.0 == SOLO_PLAYER) {
                self.extend_cooldown(skill, *seconds);
            }
            for SkillImpact { skill, rank, center, creatures, players, parried, contact, .. } in tick.impacts {
                self.on_hit(Shown { caster: None, skill, rank, center, creatures, players, parried, contact });
            }
        }
        if !ran_rules {
            // En ligne : le serveur juge, l'arme avance ici pour l'affichage
            self.runner.tick_weapons(dt);
        }
        self.plays.retain(|p| now.duration_since(p.started).as_secs_f32() < p.duration);
        self.ready_at.retain(|_, t| *t > now);
        std::mem::take(&mut self.cues)
    }

    /// Vrai une fois si un saignement vient de tuer le joueur solo
    pub(crate) fn take_solo_death(&mut self) -> bool {
        std::mem::take(&mut self.solo_died)
    }

    /// Le joueur solo vient de tomber : un effet à la mort (Abomination de l'Étherium) le relève
    /// sur place dans une nova
    pub(crate) fn solo_last_stand(&mut self, creatures: &mut CreatureWorld, feet: Vec3, stats: &mut CharacterStats) {
        let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
        if let Some(SkillImpact { skill, rank, center, creatures, players, parried, .. }) = self.runner.last_stand(SOLO_PLAYER, creatures, &mut player) {
            self.on_hit(Shown { caster: None, skill, rank, center, creatures, players, parried, contact: false });
        }
    }

    /// Le joueur solo a frappé la créature `id` de son arme : ses renforcements qui portent des
    /// effets (Marteau Consacré) les jouent autour d'elle
    pub(crate) fn solo_weapon_hit(&mut self, id: herocraft_common::CreatureId, creatures: &mut CreatureWorld, feet: Vec3, stats: &mut CharacterStats) {
        let Some(at) = creatures.creatures().iter().find(|c| c.id == id).map(|c| c.position) else { return };
        let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
        self.runner.ride(SOLO_PLAYER, &[at], creatures, &mut player);
    }

    /// Facteur des dégâts d'un coup d'arme du joueur solo sur la créature `id` (renforcements de dégâts)
    pub(crate) fn solo_hit_factor(&self, id: herocraft_common::CreatureId, creatures: &herocraft_world::CreatureWorld) -> f32 {
        self.runner.hit_factor(SOLO_PLAYER, id, creatures)
    }

    /// Le coup `damage` de la créature `attacker` arrive sur le joueur solo, qui regarde vers
    /// `facing` : sa garde le pare peut-être, et sa riposte se montre. Renvoie les dégâts qui
    /// passent, et si le coup a été paré.
    pub(crate) fn solo_parry(&mut self, creatures: &mut CreatureWorld, feet: Vec3, facing: Vec3, stats: &mut CharacterStats, attacker: herocraft_common::CreatureId, damage: f32, kind: herocraft_common::DamageKind) -> (f32, bool) {
        let mut player = SoloPlayer { id: SOLO_PLAYER, feet, stats };
        let parry = self.runner.parry(SOLO_PLAYER, Some(facing), attacker, damage, kind.name(), creatures, &mut player);
        if let Some(SkillImpact { skill, rank, center, creatures, players, parried, .. }) = parry.impact {
            self.on_hit(Shown { caster: None, skill, rank, center, creatures, players, parried, contact: false });
        }
        (parry.damage, parry.parried)
    }

    /// Ce que le joueur solo ignore en ce moment (immunités)
    pub(crate) fn solo_immunities(&self) -> Vec<herocraft_common::skills::Status> {
        self.runner.immunities(SOLO_PLAYER)
    }

    /// Facteur des dégâts qu'encaisse le joueur solo (renforcement de défense)
    pub(crate) fn solo_damage_factor(&self, kind: herocraft_common::DamageKind) -> f32 {
        self.runner.damage_taken_factor(SOLO_PLAYER, kind)
    }

    /// La pose du héros local : son geste en cours par-dessus la marche
    /// Pièces greffées sur le héros local en ce moment
    /// Allure du joueur local donnée par ses renforcements de vitesse (1 : aucun, 0 : immobile)
    /// `missing` : la part de vie qui lui manque, que suivent certains renforcements (Frénésie Moribonde)
    pub(crate) fn speed_factor(&mut self, missing: f32) -> f32 {
        let now = Instant::now();
        self.speed_buffs.retain(|(_, until, _)| *until > now);
        (1.0 + self.speed_buffs.iter().map(|(v, _, scaled)| if *scaled { v * missing.clamp(0.0, 1.0) } else { *v }).sum::<f32>()).max(0.0)
    }

    /// Élan à donner au joueur local (ruée, bond, bond de côté : blocs ; secondes), une seule fois
    pub(crate) fn take_motion(&mut self) -> Option<(f32, f32, f32, f32)> {
        self.motion.take()
    }

    /// Le serveur (ou les règles en solo) déplace le joueur local
    pub(crate) fn on_move(&mut self, m: SkillMove) {
        self.moves.push((Instant::now(), String::new(), m.into()));
    }

    /// Les déplacements du joueur local dont l'heure est venue
    pub(crate) fn take_moves(&mut self, now: Instant) -> Vec<Motion> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.moves).into_iter().partition(|m| m.0 <= now);
        self.moves = later;
        due.into_iter().map(|m| m.2).collect()
    }

    /// Espèce dont le héros du joueur `player` (`None` : le joueur local) a pris la forme
    pub(crate) fn form_of(&mut self, player: Option<herocraft_common::PlayerId>) -> Option<String> {
        self.form_skill(player).and_then(|s| skills().get(&s)).and_then(|d| d.form.clone())
    }

    /// Le joueur `player` (`None` : le joueur local) joue l'une des attaques de sa forme
    pub(crate) fn form_attacking(&self, player: Option<herocraft_common::PlayerId>) -> bool {
        let now = Instant::now();
        self.plays.iter().any(|p| p.player == player && skills().form_of_attack(&p.skill).is_some() && now.duration_since(p.started).as_secs_f32() < p.duration)
    }

    /// La compétence de forme que le joueur `player` (`None` : le joueur local) a lancée, tant qu'elle dure
    pub(crate) fn form_skill(&mut self, player: Option<herocraft_common::PlayerId>) -> Option<String> {
        let now = Instant::now();
        self.forms.retain(|(_, _, until)| *until > now);
        self.forms.iter().find(|(who, _, _)| *who == player).map(|(_, f, _)| f.clone())
    }

    pub(crate) fn hero_grafts(&mut self) -> Vec<String> {
        self.grafts_of(None)
    }

    /// Pièces greffées sur le héros du joueur `player` (`None` : le joueur local) en ce moment
    pub(crate) fn grafts_of(&mut self, player: Option<herocraft_common::PlayerId>) -> Vec<String> {
        let now = Instant::now();
        self.grafts.retain(|(_, _, until)| *until > now);
        self.grafts.iter().filter(|(who, _, _)| *who == player).map(|(_, g, _)| g.clone()).collect()
    }

    pub(crate) fn hero_pose(&self, swing: f32, units_per_block: f32) -> FigurePose {
        self.pose_of(None, swing, units_per_block)
    }

    /// La pose du héros du joueur `player` (`None` : le joueur local) : son geste en cours par-dessus la marche
    pub(crate) fn pose_of(&self, player: Option<herocraft_common::PlayerId>, swing: f32, units_per_block: f32) -> FigurePose {
        let now = Instant::now();
        let mut pose = FigurePose { swing, ..FigurePose::default() };
        let Some(play) = self.plays.iter().find(|p| p.player == player) else { return pose };
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
    pub(crate) fn local_play(&self) -> Option<&Play> {
        self.plays.iter().find(|p| p.player.is_none())
    }
}

/// Durée d'une forme : celle de ses renforcements, boucliers et effets sur la durée (comme les
/// règles, `SkillRunner::cast`)
fn lasting_seconds(cast: &herocraft_common::skills::Cast, rank: u8) -> f32 {
    cast.self_effects
        .iter()
        .chain(&cast.effects)
        .filter_map(|e| match &e.effect {
            Effect::Buff { duration, .. } | Effect::Shield { duration, .. } | Effect::OverTime { duration, .. } => Some(duration.at(rank)),
            _ => None,
        })
        .fold(0.0, f32::max)
}

/// Durée la plus longue des effets qui durent (ralenti, étourdi, bouclier, renforcement…)
fn effect_seconds(effects: &[herocraft_common::skills::EffectEntry], rank: u8) -> f32 {
    effects
        .iter()
        .map(|e| match &e.effect {
            Effect::Slow { duration, .. }
            | Effect::Stun { duration }
            | Effect::Shield { duration, .. }
            | Effect::Buff { duration, .. }
            | Effect::OverTime { duration, .. }
            | Effect::Taunt { duration }
            | Effect::Invisible { duration }
            | Effect::Blind { duration }
            | Effect::Mark { duration, .. }
            | Effect::Link { duration, .. }
            | Effect::Immune { duration, .. }
            | Effect::BreakGuard { duration } => duration.at(rank),
            Effect::Guard(guard) => guard.duration.at(rank),
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::PlayerId;
    use herocraft_common::{CreatureRegistry, PlayerClass};
    use std::sync::Arc;

    #[test]
    fn a_local_cast_gives_its_dash_its_speed_and_its_form() {
        let mut play = SkillPlay::default();
        let mut stats = CharacterStats::new(PlayerClass::Druid);
        for skill in ["breezeStep", "zephyrMantle", "forestWolf"] {
            stats.skills.insert(skill.into(), 1);
        }
        stats.mana = 1000.0;
        stats.stamina = 1000.0;
        let mut creatures = CreatureWorld::new(1000.0, 1, Arc::new(CreatureRegistry::load_default()));
        let feet = Vec3::Z * 1000.0;

        // Pas de Brise : une ruée vers l'avant
        play.request("breezeStep", &mut stats, Vec3::NEG_Y, None, None).unwrap();
        let (dash, leap, side, seconds) = play.take_motion().expect("une ruée");
        assert!(dash > 0.0 && leap == 0.0 && side == 0.0 && seconds > 0.0);
        assert!(play.take_motion().is_none(), "une seule fois");

        // Manteau de Zéphyr : plus vite, une fois l'impact passé
        play.request("zephyrMantle", &mut stats, Vec3::NEG_Y, None, None).unwrap();
        assert_eq!(play.speed_factor(0.0), 1.0);
        for _ in 0..40 {
            play.tick(0.05, Some((&mut creatures, feet, &mut stats)));
        }
        assert!(play.speed_factor(0.0) > 1.3, "{}", play.speed_factor(0.0));

        // Loup des Bois : le héros devient le loup, et n'a plus que ses deux attaques
        assert_eq!(play.request("wolfBite", &mut stats, Vec3::NEG_Y, None, None), Err(CastError::NotInForm.into()));
        play.request("forestWolf", &mut stats, Vec3::NEG_Y, None, None).unwrap();
        assert_eq!(play.form_of(None).as_deref(), Some("forestWolf"));
        assert_eq!(play.form_of(Some(PlayerId(7))), None);
        play.request("wolfBite", &mut stats, Vec3::NEG_Y, None, None).unwrap();
        assert!(play.form_attacking(None));
        play.request("wolfPounce", &mut stats, Vec3::NEG_Y, None, None).unwrap();
        stats.skills.insert("packClaws".into(), 1);
        let refused = play.request("packClaws", &mut stats, Vec3::NEG_Y, None, None);
        assert_eq!(refused, Err(CastError::InForm.into()), "une autre compétence sous la forme");
    }
}

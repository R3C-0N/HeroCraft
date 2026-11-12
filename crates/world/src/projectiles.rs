//! Les projectiles en vol : tirs des créatures (tourelles, liche, bastion) et des compétences des
//! joueurs. Chaque instant, un tir avance d'un segment ; le premier obstacle rencontré sur ce
//! segment décide de la suite : le relief l'arrête, un mur ennemi l'arrête et encaisse, une garde le
//! bloque ou le dévie, une cible le prend, puis il s'arrête, la traverse ou rebondit.
//!
//! Module enfant de `creatures` : il lit directement les bêtes, le registre et la géométrie.

use glam::Vec3;
use herocraft_common::{ProjectileEnd, ProjectileEvent, ProjectileSize, Shooter};

use super::{CreatureWorld, PlayerHit, TARGET_RADIUS};
use crate::edits::{raycast_blocks, TerrainQuery};

/// Hauteur d'un joueur visé par un tir, en blocs
const PLAYER_HEIGHT: f32 = 1.8;
/// Angle dont un projectile dévié s'écarte, en degrés
const DEFLECT_ANGLE: f32 = 60.0;
/// Portée qui reste à un projectile dévié, en blocs
const DEFLECTED_RANGE: f32 = 12.0;
/// Marge en hauteur autour du corps d'une cible, en blocs
const VERTICAL_SLACK: f32 = 1.0;
/// Rebonds et déviations au plus par projectile et par instant
const MAX_TURNS_PER_STEP: u32 = 4;

/// Ce qu'un tir suit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Homing {
    Creature(u64),
    /// Un joueur, par sa clé
    Player(u64),
}

/// Ce que porte un projectile, et qui juge de ses touches
#[derive(Debug, Clone, Copy)]
pub(crate) enum Payload {
    /// Tir de créature : ses dégâts, appliqués ici, et le rayon de son explosion (unités du monde)
    Creature { species: usize, damage: f32, blast: f32 },
    /// Tir de compétence : `SkillRunner` applique ses effets à chaque touche (`take_contacts`)
    Skill { tag: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct Projectile {
    pub id: u64,
    pub shooter: Shooter,
    /// Tiré par le camp des joueurs (joueur, invocation) : il touche les bêtes hostiles
    pub allied: bool,
    pub position: Vec3,
    /// Direction (unitaire) et vitesse, en unités du monde par seconde (0 : il part et arrive dans
    /// le même instant, comme un jet de flammes)
    pub direction: Vec3,
    pub speed: f32,
    /// Demi-épaisseur, en unités du monde
    pub radius: f32,
    /// Distance qu'il peut encore parcourir, en unités du monde
    pub left: f32,
    pub homing: Option<Homing>,
    /// Ennemis qu'il traverse encore (`u32::MAX` : tous)
    pub pierce: u32,
    /// Rebonds qui lui restent et leur portée, en unités du monde
    pub bounces: u32,
    pub bounce_range: f32,
    pub through_terrain: bool,
    pub through_obstacles: bool,
    pub size: ProjectileSize,
    /// Créatures déjà touchées : il ne les touche qu'une fois
    pub hit: Vec<u64>,
    /// Joueurs (clés) qui l'ont dévié : il ne peut plus les toucher
    pub spared: Vec<u64>,
    pub payload: Payload,
}

/// Les défenses d'un joueur contre les tirs, que `SkillRunner` donne à chaque instant
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerGuard {
    pub key: u64,
    /// Garde frontale (Mur vivant) : direction (unitaire) et cosinus du demi-angle de l'arc
    pub front: Option<(Vec3, f32)>,
    /// Déviation (Manteau de Zéphyr) : la plus grande taille de projectile déviée
    pub deflect: Option<ProjectileSize>,
}

/// La touche d'un tir de compétence, ou sa fin
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillContact {
    /// Il touche la créature `creature` au point `at`
    Hit { tag: u64, creature: u64, at: Vec3 },
    /// Il s'arrête en `at`
    Over { tag: u64, at: Vec3, reason: ProjectileEnd },
}

/// Un tir de compétence à lancer (`CreatureWorld::launch_skill_shot`), longueurs en blocs
#[derive(Debug, Clone)]
pub struct SkillShot {
    pub caster: u64,
    /// Repère du tir chez `SkillRunner`, rendu à chaque touche
    pub tag: u64,
    pub skill: String,
    pub from: Vec3,
    pub direction: Vec3,
    /// Blocs par seconde
    pub speed: f32,
    pub range: f32,
    /// Demi-épaisseur
    pub radius: f32,
    pub pierce: u32,
    pub bounces: u32,
    pub bounce_range: f32,
    pub through_terrain: bool,
    pub through_obstacles: bool,
    pub size: ProjectileSize,
}

/// Ce que le segment d'un instant rencontre, à `t` unités de son départ
#[derive(Debug, Clone, Copy)]
enum Meet {
    Terrain,
    /// Un mur (identifiant)
    Obstacle(u64),
    Creature(u64),
    /// Un joueur (rang dans la liste de l'instant)
    Player(usize),
}

impl CreatureWorld {
    fn next_projectile_id(&mut self) -> u64 {
        self.next_projectile += 1;
        self.next_projectile
    }

    /// Lance le tir d'une créature
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn launch_creature_shot(&mut self, shooter: u64, species: usize, allied: bool, from: Vec3, to: Vec3, homing: Option<Homing>, shot: &herocraft_common::CreatureShot, damage: f32) {
        let block = self.geometry.block_height as f32;
        let line = to - from;
        let distance = line.length().max(1e-4);
        let direction = line / distance;
        let pierce = shot.pierce * block;
        let id = self.next_projectile_id();
        // Un tir perforant file droit au-delà de sa cible, jusqu'au bout de la grappe ; un tir guidé
        // a de la marge pour suivre une cible qui fuit
        let (left, homing, radius, pierce_count) = if pierce > 0.0 { (distance + 4.0 * pierce, None, pierce, u32::MAX) } else { (distance * 2.0 + 4.0 * block, homing, 0.0, 0) };
        let speed = shot.speed.max(0.0) * block;
        self.projectiles.push(Projectile {
            id,
            shooter: Shooter::Creature { id: shooter, species: species as u32 },
            allied,
            position: from,
            direction,
            speed,
            radius,
            left,
            homing,
            pierce: pierce_count,
            bounces: 0,
            bounce_range: 0.0,
            through_terrain: true,
            through_obstacles: false,
            size: shot.size(),
            hit: Vec::new(),
            spared: Vec::new(),
            payload: Payload::Creature { species, damage, blast: shot.radius * block },
        });
        let shown = if speed > 0.0 { speed } else { distance * 20.0 };
        self.projectile_events.push(ProjectileEvent::Launch { id, shooter: Shooter::Creature { id: shooter, species: species as u32 }, skill: String::new(), from, velocity: direction * shown });
    }

    /// Lance le tir d'une compétence ; ses touches sont rendues par `take_contacts`
    pub fn launch_skill_shot(&mut self, shot: SkillShot) -> u64 {
        let block = self.geometry.block_height as f32;
        let id = self.next_projectile_id();
        let direction = shot.direction.normalize_or_zero();
        self.projectiles.push(Projectile {
            id,
            shooter: Shooter::Player(shot.caster),
            allied: true,
            position: shot.from,
            direction,
            speed: shot.speed.max(0.1) * block,
            radius: shot.radius.max(0.0) * block,
            left: shot.range.max(0.0) * block,
            homing: None,
            pierce: shot.pierce,
            bounces: shot.bounces,
            bounce_range: shot.bounce_range * block,
            through_terrain: shot.through_terrain,
            through_obstacles: shot.through_obstacles,
            size: shot.size,
            hit: Vec::new(),
            spared: Vec::new(),
            payload: Payload::Skill { tag: shot.tag },
        });
        self.projectile_events.push(ProjectileEvent::Launch { id, shooter: Shooter::Player(shot.caster), skill: shot.skill, from: shot.from, velocity: direction * shot.speed * block });
        id
    }

    /// Les défenses des joueurs contre les tirs, pour les instants qui suivent
    pub fn set_guards(&mut self, guards: Vec<PlayerGuard>) {
        self.guards = guards;
    }

    /// Touches et fins des tirs de compétence depuis le dernier appel
    pub fn take_contacts(&mut self) -> Vec<SkillContact> {
        std::mem::take(&mut self.contacts)
    }

    /// Départs, déviations et arrêts de projectiles depuis le dernier appel, pour les montrer
    pub fn take_projectile_events(&mut self) -> Vec<ProjectileEvent> {
        std::mem::take(&mut self.projectile_events)
    }

    /// Projectiles en vol : identifiant, position
    pub fn projectiles(&self) -> impl Iterator<Item = (u64, Vec3)> + '_ {
        self.projectiles.iter().map(|p| (p.id, p.position))
    }

    /// Fait avancer les projectiles de `dt` secondes. `players` : pieds des joueurs, `keys` leurs
    /// clés (0 sans clé). Renvoie les coups portés aux joueurs.
    pub(crate) fn fly(&mut self, dt: f32, query: &mut TerrainQuery, players: &[Vec3], keys: &[u64]) -> Vec<PlayerHit> {
        self.fly_over(dt, Some(query), players, keys)
    }

    /// Fait avancer les projectiles sans relief (un monde sans planète, celui des tests) : rien ne
    /// les arrête que les murs et les cibles
    pub fn advance_projectiles(&mut self, dt: f32, players: &[Vec3]) -> Vec<PlayerHit> {
        self.fly_over(dt, None, players, &[])
    }

    fn fly_over(&mut self, dt: f32, mut query: Option<&mut TerrainQuery>, players: &[Vec3], keys: &[u64]) -> Vec<PlayerHit> {
        let block = self.geometry.block_height as f32;
        let key_of = |k: usize| keys.get(k).copied().unwrap_or(0);
        let mut player_hits = Vec::new();
        // Coups aux créatures : qui, d'où, combien
        let mut blasts: Vec<(u64, Vec3, f32)> = Vec::new();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        let mut ended: Vec<usize> = Vec::new();

        for (index, p) in projectiles.iter_mut().enumerate() {
            let mut budget = if p.speed > 0.0 { p.speed * dt } else { p.left };
            let mut turns = 0;
            let mut end: Option<(Vec3, ProjectileEnd)> = None;
            while budget > 1e-5 && end.is_none() {
                // Un tir guidé corrige sa course vers sa cible ; il s'arrête sur elle
                let mut goal_distance = f32::MAX;
                if let Some(homing) = p.homing {
                    let goal = match homing {
                        Homing::Creature(id) => self.creatures.iter().find(|c| c.id == id && c.is_alive()).map(|c| c.position),
                        Homing::Player(key) => (0..players.len()).find(|&k| key_of(k) == key).map(|k| players[k]),
                    };
                    match goal {
                        Some(goal) => {
                            let aim = goal + goal.normalize_or_zero() * TARGET_RADIUS * block;
                            let to = aim - p.position;
                            goal_distance = to.length();
                            if goal_distance > 1e-4 {
                                p.direction = to / goal_distance;
                            }
                        }
                        None => p.homing = None,
                    }
                }
                let step = budget.min(p.left).min(goal_distance + p.radius + TARGET_RADIUS * block);
                if step <= 1e-5 {
                    end = Some((p.position, ProjectileEnd::Range));
                    break;
                }
                let mut meets = self.meets(p, step, query.as_deref_mut(), players, &key_of);
                meets.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut advanced = step;
                let mut turned = false;
                for (t, meet) in meets {
                    let at = p.position + p.direction * t;
                    match meet {
                        Meet::Terrain => {
                            end = Some((at, ProjectileEnd::Terrain));
                        }
                        Meet::Obstacle(wall) => {
                            if let Payload::Creature { damage, .. } = p.payload {
                                blasts.push((wall, at, damage));
                            }
                            end = Some((at, ProjectileEnd::Blocked));
                        }
                        Meet::Creature(id) => {
                            if p.hit.contains(&id) {
                                continue;
                            }
                            p.hit.push(id);
                            match p.payload {
                                Payload::Creature { blast, .. } if blast > 0.0 => {
                                    end = Some((at, ProjectileEnd::Hit));
                                    break;
                                }
                                Payload::Creature { damage, .. } => blasts.push((id, p.position, damage)),
                                Payload::Skill { tag } => self.contacts.push(SkillContact::Hit { tag, creature: id, at }),
                            }
                            if p.bounces > 0 && turns < MAX_TURNS_PER_STEP {
                                // Il repart vers l'ennemi le plus proche qu'il n'a pas encore touché
                                let from = self.creatures.iter().find(|c| c.id == id).map_or(at, |c| c.position);
                                let next = self
                                    .creatures
                                    .iter()
                                    .filter(|c| c.is_alive() && !c.hidden && !p.hit.contains(&c.id) && self.is_foe(p, c))
                                    .map(|c| (c.id, c.position, c.position.distance(from)))
                                    .filter(|(_, _, d)| *d <= p.bounce_range)
                                    .min_by(|a, b| a.2.total_cmp(&b.2));
                                if let Some((target_id, target, distance)) = next {
                                    p.bounces -= 1;
                                    turns += 1;
                                    let up = target.normalize_or_zero();
                                    let aim = target + up * TARGET_RADIUS * block;
                                    p.position = at;
                                    p.direction = (aim - at).normalize_or_zero();
                                    p.left = distance * 1.5 + 2.0 * block;
                                    p.homing = Some(Homing::Creature(target_id));
                                    self.projectile_events.push(ProjectileEvent::Turn { id: p.id, at, velocity: p.direction * p.speed.max(1.0) });
                                    advanced = t;
                                    turned = true;
                                    break;
                                }
                                p.bounces = 0;
                            }
                            if p.pierce == 0 {
                                end = Some((at, ProjectileEnd::Hit));
                            } else if p.pierce != u32::MAX {
                                p.pierce -= 1;
                            }
                        }
                        Meet::Player(k) => {
                            let key = key_of(k);
                            let guard = self.guards.iter().find(|g| g.key == key).copied();
                            // De face, sur la garde du Mur vivant : il s'arrête net
                            if let Some((facing, cos)) = guard.and_then(|g| g.front) {
                                if (-p.direction).dot(facing) >= cos {
                                    end = Some((at, ProjectileEnd::Blocked));
                                    break;
                                }
                            }
                            // Le vent du Manteau de Zéphyr l'écarte
                            let deflects = guard.and_then(|g| g.deflect).is_some_and(|most| most == ProjectileSize::Large || p.size == ProjectileSize::Small);
                            if deflects && turns < MAX_TURNS_PER_STEP {
                                turns += 1;
                                let up = at.normalize_or_zero();
                                let side = if p.id % 2 == 0 { 1.0 } else { -1.0 };
                                let angle = side * DEFLECT_ANGLE.to_radians();
                                p.direction = (p.direction * angle.cos() + up.cross(p.direction) * angle.sin()).normalize_or_zero();
                                p.position = at;
                                p.homing = None;
                                p.spared.push(key);
                                p.left = p.left.min(DEFLECTED_RANGE * block);
                                self.projectile_events.push(ProjectileEvent::Turn { id: p.id, at, velocity: p.direction * p.speed.max(1.0) });
                                advanced = t;
                                turned = true;
                                break;
                            }
                            p.spared.push(key);
                            if let Payload::Creature { species, damage, blast } = p.payload {
                                if blast > 0.0 {
                                    end = Some((at, ProjectileEnd::Hit));
                                    break;
                                }
                                if let Shooter::Creature { id, .. } = p.shooter {
                                    player_hits.push(PlayerHit { player: k, creature_id: id, species, damage });
                                }
                            }
                            if p.pierce == 0 {
                                end = Some((at, ProjectileEnd::Hit));
                            } else if p.pierce != u32::MAX {
                                p.pierce -= 1;
                            }
                        }
                    }
                    if end.is_some() {
                        break;
                    }
                }
                if let Some((at, _)) = end {
                    p.position = at;
                    break;
                }
                if !turned {
                    p.position += p.direction * advanced;
                }
                p.left -= advanced;
                budget -= advanced;
                if p.left <= 1e-4 || (p.homing.is_some() && goal_distance <= advanced) {
                    end = Some((p.position, if p.homing.is_some() { ProjectileEnd::Hit } else { ProjectileEnd::Range }));
                }
            }
            if let Some((at, reason)) = end {
                // Un tir qui explose touche tout ennemi dans son rayon, où qu'il s'arrête
                if let Payload::Creature { species, damage, blast } = p.payload {
                    if blast > 0.0 {
                        for c in self.creatures.iter().filter(|c| c.is_alive() && !c.hidden && self.is_foe(p, c) && c.position.distance(at) <= blast) {
                            blasts.push((c.id, at, damage));
                        }
                        if !p.allied {
                            if let Shooter::Creature { id, .. } = p.shooter {
                                for (k, feet) in players.iter().enumerate() {
                                    if feet.distance(at) <= blast && !p.spared.contains(&key_of(k)) {
                                        player_hits.push(PlayerHit { player: k, creature_id: id, species, damage });
                                    }
                                }
                            }
                        }
                    } else if reason == ProjectileEnd::Hit {
                        // Un tir guidé arrivé sur sa cible sans l'avoir croisée (elle s'est déplacée
                        // dans l'instant) la touche quand même
                        match p.homing {
                            Some(Homing::Creature(id)) if !p.hit.contains(&id) => blasts.push((id, at, damage)),
                            Some(Homing::Player(key)) if !p.spared.contains(&key) => {
                                if let (Some(k), Shooter::Creature { id, .. }) = ((0..players.len()).find(|&k| key_of(k) == key), p.shooter) {
                                    player_hits.push(PlayerHit { player: k, creature_id: id, species, damage });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if let Payload::Skill { tag } = p.payload {
                    self.contacts.push(SkillContact::Over { tag, at, reason });
                }
                self.projectile_events.push(ProjectileEvent::End { id: p.id, at, reason });
                ended.push(index);
            }
        }
        for index in ended.into_iter().rev() {
            projectiles.swap_remove(index);
        }
        // Les tirs lancés pendant l'instant (aucun ici) s'ajoutent à ceux qui volent encore
        projectiles.append(&mut self.projectiles);
        self.projectiles = projectiles;
        for (id, from, damage) in blasts {
            self.strike(id, from, damage, false);
        }
        player_hits
    }

    /// `c` est-elle une cible pour le projectile `p` ?
    fn is_foe(&self, p: &Projectile, c: &super::Creature) -> bool {
        if matches!(p.shooter, Shooter::Creature { id, .. } if id == c.id) {
            return false;
        }
        if p.allied {
            !c.tamed && c.summoner.is_none()
        } else {
            c.summoner.is_some() || c.tamed
        }
    }

    /// Les rencontres du segment de longueur `step` que le projectile `p` parcourt depuis sa position
    fn meets(&self, p: &Projectile, step: f32, query: Option<&mut TerrainQuery>, players: &[Vec3], key_of: &dyn Fn(usize) -> u64) -> Vec<(f32, Meet)> {
        let block = self.geometry.block_height as f32;
        let mut meets = Vec::new();
        let (from, dir) = (p.position, p.direction);
        if let (false, Some(query)) = (p.through_terrain, query) {
            let hit = raycast_blocks(&self.geometry, from.as_dvec3(), dir.as_dvec3(), step as f64, |pos| Some(query.block(pos)).filter(|b| b.is_solid()));
            if let Some(hit) = hit {
                meets.push((hit.distance as f32, Meet::Terrain));
            }
        }
        for c in self.creatures.iter().filter(|c| c.is_alive() && !c.hidden && c.appearing <= 0.0) {
            let Some(species) = self.registry.get(c.species) else { continue };
            let wall = species.obstacle.is_some_and(|o| o.shots);
            if wall {
                // Un mur arrête les tirs du camp d'en face : un mur invoqué, ceux des bêtes hostiles
                let blocks = c.summoner.is_some() != p.allied;
                if blocks && !p.through_obstacles {
                    if let Some(t) = box_entry(from, dir, step, c.position, c.yaw, &species.collider(), c.scale() * block, p.radius) {
                        meets.push((t, Meet::Obstacle(c.id)));
                    }
                }
                continue;
            }
            // Un tir guidé ne touche que sa cible
            if p.homing.is_some_and(|h| h != Homing::Creature(c.id)) || !self.is_foe(p, c) {
                continue;
            }
            let scale = c.scale() * block;
            let reach = p.radius + species.body.half_width.max(TARGET_RADIUS) * scale;
            let up = c.position.normalize_or_zero();
            if let Some(t) = capsule_entry(from, dir, step, c.position, up, species.body.height.max(0.5) * scale, reach, VERTICAL_SLACK * block) {
                meets.push((t, Meet::Creature(c.id)));
            }
        }
        if !p.allied {
            for (k, feet) in players.iter().enumerate() {
                let key = key_of(k);
                if p.spared.contains(&key) || p.homing.is_some_and(|h| h != Homing::Player(key)) {
                    continue;
                }
                let up = feet.normalize_or_zero();
                if let Some(t) = capsule_entry(from, dir, step, *feet, up, PLAYER_HEIGHT * block, p.radius + TARGET_RADIUS * block, VERTICAL_SLACK * block) {
                    meets.push((t, Meet::Player(k)));
                }
            }
        }
        meets
    }
}

/// Où le segment `from + dir·t` (t de 0 à `length`) entre pour la première fois dans le cylindre
/// d'axe `base + up·u` (u de `-slack` à `height + slack`) et de rayon `reach`. La marge en hauteur
/// pardonne un tir à hauteur de main sur une petite bête, comme le faisaient les zones.
pub(crate) fn capsule_entry(from: Vec3, dir: Vec3, length: f32, base: Vec3, up: Vec3, height: f32, reach: f32, slack: f32) -> Option<f32> {
    // Par pas fins le long du segment : quelques dizaines de tests au plus, assez juste à l'échelle
    // d'un corps
    let samples = ((length / (reach * 0.5).max(1e-3)).ceil() as usize).clamp(1, 64);
    (0..=samples).map(|i| length * i as f32 / samples as f32).find(|&t| {
        let point = from + dir * t - base;
        let u = point.dot(up);
        (-slack..=height + slack).contains(&u) && (point - up * u).length() <= reach
    })
}

/// Où le segment `from + dir·t` (t de 0 à `length`) entre dans la boîte d'un corps solide posé en
/// `feet`, tourné du cap `yaw` (`scale` : taille d'un bloc, à l'échelle de la bête), grossie de
/// `margin`
pub(crate) fn box_entry(from: Vec3, dir: Vec3, length: f32, feet: Vec3, yaw: f32, collider: &herocraft_common::Collider, scale: f32, margin: f32) -> Option<f32> {
    let up = feet.normalize_or_zero();
    let (east, north) = herocraft_common::tangent_frame(up);
    let ahead = north * yaw.cos() + east * yaw.sin();
    let side = east * yaw.cos() - north * yaw.sin();
    let local = from - feet;
    let axes = [
        (ahead, collider.half_length * scale + margin, -collider.half_length * scale - margin),
        (side, collider.half_width * scale + margin, -collider.half_width * scale - margin),
        (up, collider.height * scale + margin, -margin),
    ];
    let (mut enter, mut exit) = (0.0f32, length);
    for (axis, high, low) in axes {
        let origin = local.dot(axis);
        let speed = dir.dot(axis);
        if speed.abs() < 1e-6 {
            if origin < low || origin > high {
                return None;
            }
            continue;
        }
        let (a, b) = ((low - origin) / speed, (high - origin) / speed);
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        enter = enter.max(a);
        exit = exit.min(b);
        if enter > exit {
            return None;
        }
    }
    Some(enter)
}

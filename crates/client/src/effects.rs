//! Effets visuels des compétences (`assets/effects/*.json`, décrits dans le README du dossier).
//!
//! Le processeur simule les couches de chaque effet en cours (formes et particules) et en tire
//! des quadrilatères ; le processeur graphique les dessine dans la cible HDR, avant le bloom, en
//! testant la profondeur sans l'écrire. Une couche qui luit (`glow` > 1) s'ajoute à la lumière
//! de la scène ; une couche mate (poussière, fumée) se pose dessus comme un voile.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use glam::Vec3;
use serde::Deserialize;
use serde_json::Value;

use herocraft_common::skills::Anchor;

use crate::hero_model::HeroAnchors;
use crate::post;
use crate::skill_play::VisualCue;

/// Quadrilatères au plus par image
pub const MAX_QUADS: usize = 4000;
/// Côté des textures, ramenées toutes à cette taille dans le tableau de textures
const TEXTURE_SIZE: u32 = 64;
/// Noms des textures, dans l'ordre des couches du tableau (`TEXTURES[i]` est la couche `i`)
const TEXTURES: [&str; 7] = ["soft.png", "spark.png", "flame.png", "smoke.png", "shard.png", "star.png", "rune.png"];
/// Aucune texture : tache ronde calculée par le shader
const NO_TEXTURE: u32 = 255;
/// Durée d'une couche sans durée ni boucle, en secondes
const DEFAULT_LAYER_SECONDS: f32 = 0.5;

// Formes, telles que le shader les lit (`kind`)
/// Secondes de trajet dont un sillage se souvient au plus
const TRAIL_MEMORY: f32 = 0.6;
const KIND_SPRITE: u32 = 0;
const KIND_RING: u32 = 1;
const KIND_ARC: u32 = 2;
const KIND_DOME: u32 = 3;
const KIND_BEAM: u32 = 4;
const KIND_DISC: u32 = 5;

// ------------------------------------------------------------------------------------------------
// Format

/// Un nombre ou une paire `[début, fin]`
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pair(f32, f32);

impl Pair {
    fn at(self, s: f32) -> f32 {
        self.0 + (self.1 - self.0) * s.clamp(0.0, 1.0)
    }

    /// Tirage entre les deux bornes
    fn pick(self, r: f32) -> f32 {
        self.at(r)
    }
}

fn pair(v: Option<&Value>, default: f32) -> Pair {
    match v {
        Some(Value::Number(n)) => {
            let x = n.as_f64().unwrap_or(default as f64) as f32;
            Pair(x, x)
        }
        Some(Value::Array(a)) if a.len() == 2 => Pair(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32),
        _ => Pair(default, default),
    }
}

/// Couleur linéaire et opacité
fn color(text: &str) -> [f32; 4] {
    let hex = text.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("FF"), 16).unwrap_or(255) as f32 / 255.0;
    let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let alpha = if hex.len() >= 8 { byte(6) } else { 1.0 };
    [linear(byte(0)), linear(byte(2)), linear(byte(4)), alpha]
}

fn colors(v: Option<&Value>) -> [[f32; 4]; 2] {
    match v.and_then(Value::as_array) {
        Some(a) if a.len() == 2 => [color(a[0].as_str().unwrap_or("#FFFFFF")), color(a[1].as_str().unwrap_or("#FFFFFF"))],
        _ => [[1.0; 4]; 2],
    }
}

fn mix4(a: [f32; 4], b: [f32; 4], s: f32) -> [f32; 4] {
    let s = s.clamp(0.0, 1.0);
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * s)
}

fn texture_layer(v: Option<&Value>) -> u32 {
    v.and_then(Value::as_str).and_then(|name| TEXTURES.iter().position(|t| *t == name)).map_or(NO_TEXTURE, |i| i as u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Ring,
    Disc,
    Arc,
    Streak,
    Dome,
    Column,
    Sprite,
    Trail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Point,
    Sphere,
    Ring,
    Disc,
    Cylinder,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Outward,
    Inward,
    Forward,
    Random,
    None,
}

#[derive(Debug, Clone)]
struct ShapeLayer {
    shape: Shape,
    radius: Pair,
    thickness: Pair,
    length: Pair,
    width: Pair,
    height: Pair,
    size: Pair,
    angle: f32,
    sweep: bool,
    spin: f32,
    pulse: f32,
    on_ground: bool,
    upright: bool,
    texture: u32,
    /// Sillage (`trail`) : secondes de trajet qu'il couvre derrière l'ancre
    life: f32,
}

#[derive(Debug, Clone)]
struct ParticleLayer {
    count: u32,
    rate: f32,
    origin: Origin,
    origin_radius: f32,
    origin_height: f32,
    origin_length: f32,
    upright: bool,
    direction: Direction,
    spread: f32,
    speed: Pair,
    life: Pair,
    gravity: f32,
    drag: f32,
    size: Pair,
    spin: Pair,
    orbit: f32,
    texture: u32,
}

#[derive(Debug, Clone)]
enum LayerKind {
    Shape(ShapeLayer),
    Particles(ParticleLayer),
}

#[derive(Debug, Clone)]
struct Layer {
    kind: LayerKind,
    delay: f32,
    duration: Option<f32>,
    looping: bool,
    fade_in: f32,
    fade_out: f32,
    color: [[f32; 4]; 2],
    glow: f32,
    offset: Vec3,
}

/// Un effet : ses couches, et l'échelle de ses longueurs
#[derive(Debug, Clone)]
pub struct EffectDef {
    area_unit: bool,
    layers: Vec<Layer>,
}

impl EffectDef {
    fn parse(json: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            unit: Option<String>,
            layers: Vec<serde_json::Map<String, Value>>,
        }
        let file: File = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut layers = Vec::new();
        for l in file.layers {
            let num = |k: &str, d: f32| l.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32);
            let flag = |k: &str| l.get(k).and_then(Value::as_bool).unwrap_or(false);
            let (kind, fields) = if let Some(Value::Object(p)) = l.get("particles") {
                let pn = |k: &str, d: f32| p.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32);
                let origin = match p.get("origin").and_then(Value::as_str).unwrap_or("point") {
                    "sphere" => Origin::Sphere,
                    "ring" => Origin::Ring,
                    "disc" => Origin::Disc,
                    "cylinder" => Origin::Cylinder,
                    "line" => Origin::Line,
                    _ => Origin::Point,
                };
                let direction = match p.get("direction").and_then(Value::as_str).unwrap_or("random") {
                    "up" => Direction::Up,
                    "outward" => Direction::Outward,
                    "inward" => Direction::Inward,
                    "forward" => Direction::Forward,
                    "none" => Direction::None,
                    _ => Direction::Random,
                };
                let layer = ParticleLayer {
                    count: pn("count", 0.0) as u32,
                    rate: pn("rate", 0.0),
                    origin,
                    origin_radius: pn("origin_radius", 0.3),
                    origin_height: pn("origin_height", 1.8),
                    origin_length: pn("origin_length", 1.0),
                    upright: p.get("upright").and_then(Value::as_bool).unwrap_or(false),
                    direction,
                    spread: pn("spread", 180.0),
                    speed: pair(p.get("speed"), 1.0),
                    life: pair(p.get("life"), 0.6),
                    gravity: pn("gravity", 0.0),
                    drag: pn("drag", 0.0),
                    size: pair(p.get("size"), 0.1),
                    spin: pair(p.get("spin"), 0.0),
                    orbit: pn("orbit", 0.0),
                    texture: texture_layer(p.get("texture")),
                };
                (LayerKind::Particles(layer), p.clone())
            } else {
                let shape = match l.get("shape").and_then(Value::as_str).ok_or("couche sans shape ni particles")? {
                    "ring" => Shape::Ring,
                    "disc" => Shape::Disc,
                    "arc" => Shape::Arc,
                    "streak" => Shape::Streak,
                    "dome" => Shape::Dome,
                    "column" => Shape::Column,
                    "sprite" => Shape::Sprite,
                    "trail" => Shape::Trail,
                    other => return Err(format!("forme inconnue {other}")),
                };
                let layer = ShapeLayer {
                    shape,
                    radius: pair(l.get("radius"), 1.0),
                    thickness: pair(l.get("thickness"), 0.2),
                    length: pair(l.get("length"), 1.0),
                    width: pair(l.get("width"), 0.2),
                    height: pair(l.get("height"), 3.0),
                    size: pair(l.get("size"), 0.5),
                    angle: num("angle", 90.0),
                    sweep: flag("sweep"),
                    spin: num("spin", 0.0),
                    pulse: num("pulse", 0.0),
                    on_ground: flag("on_ground"),
                    upright: flag("upright"),
                    texture: texture_layer(l.get("texture")),
                    life: num("life", 0.2),
                };
                (LayerKind::Shape(layer), l.clone())
            };
            let offset = fields.get("offset").and_then(Value::as_array).filter(|a| a.len() == 3).map_or(Vec3::ZERO, |a| Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32));
            layers.push(Layer {
                kind,
                delay: num("delay", 0.0),
                duration: l.get("duration").and_then(Value::as_f64).map(|v| v as f32),
                looping: flag("loop"),
                fade_in: num("fade_in", 0.0),
                fade_out: num("fade_out", 0.0),
                color: colors(fields.get("color")),
                glow: fields.get("glow").and_then(Value::as_f64).map_or(1.0, |v| v as f32),
                offset,
            });
        }
        Ok(Self { area_unit: file.unit.as_deref() == Some("area"), layers })
    }

    /// Durée de l'effet joué seul (sans durée imposée)
    fn natural_seconds(&self) -> f32 {
        self.layers
            .iter()
            .map(|l| {
                let (body, tail) = match &l.kind {
                    // Une gerbe part d'un coup : seule la vie des particules compte
                    LayerKind::Particles(p) if p.rate <= 0.0 => (l.duration.unwrap_or(0.0), p.life.0.max(p.life.1)),
                    LayerKind::Particles(p) => (l.duration.unwrap_or(DEFAULT_LAYER_SECONDS), p.life.0.max(p.life.1)),
                    LayerKind::Shape(_) => (l.duration.unwrap_or(DEFAULT_LAYER_SECONDS), 0.0),
                };
                l.delay + body + tail
            })
            .fold(0.0, f32::max)
    }
}

/// Tous les effets, par nom
#[derive(Debug, Default)]
pub struct EffectLibrary {
    effects: HashMap<String, EffectDef>,
}

impl EffectLibrary {
    pub fn load_default() -> Self {
        let Some(dir) = herocraft_common::skills::asset_dir("assets/effects") else {
            log::warn!("Dossier assets/effects/ introuvable : pas d'effets visuels.");
            return Self::default();
        };
        let (library, errors) = Self::load_dir(&dir);
        for e in errors {
            log::warn!("{e}");
        }
        library
    }

    fn load_dir(dir: &Path) -> (Self, Vec<String>) {
        let mut effects = HashMap::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| EffectDef::parse(&t)) {
                Ok(def) => {
                    effects.insert(name, def);
                }
                Err(e) => errors.push(format!("effet {name} : {e}")),
            }
        }
        (Self { effects }, errors)
    }
}

// ------------------------------------------------------------------------------------------------
// Simulation

/// Où sont les ancres à cette image
pub struct AnchorWorld<'a> {
    /// Le héros local
    pub hero: HeroAnchors,
    /// Cap du héros local
    pub hero_forward: Vec3,
    /// Les autres joueurs : pieds et cap
    pub players: &'a dyn Fn(u64) -> Option<(Vec3, f32)>,
    /// Pieds des créatures
    pub creatures: &'a dyn Fn(u64) -> Option<Vec3>,
    /// Position des projectiles en vol
    pub projectiles: &'a dyn Fn(u64) -> Option<Vec3>,
    pub eye: Vec3,
    pub block: f32,
}

#[derive(Debug, Clone)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    spin: f32,
    angle: f32,
    /// Angle d'orbite autour de l'origine, et rayon
    orbit: Option<(f32, f32)>,
}

#[derive(Debug)]
struct Running {
    def: EffectDef,
    cue: VisualCue,
    started: Instant,
    /// Durée totale
    seconds: f32,
    /// Particules de chaque couche, et particules à émettre en reste (émission continue)
    particles: Vec<Vec<Particle>>,
    emitted: Vec<bool>,
    carry: Vec<f32>,
    /// Dernière position de l'ancre
    last_anchor: Option<Vec3>,
    /// Positions récentes de l'ancre (instant, position), pour les sillages
    history: Vec<(f32, Vec3)>,
    seed: u32,
}

/// Un quadrilatère à dessiner, tel que le shader le lit
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EffectQuad {
    /// Centre (xyz), forme (w, `KIND_*` en flottant)
    pub center: [f32; 4],
    /// Demi-axe u (xyz), paramètre 0 (w)
    pub axis_u: [f32; 4],
    /// Demi-axe v (xyz), paramètre 1 (w)
    pub axis_v: [f32; 4],
    /// Couleur linéaire, déjà multipliée par la lueur (rgb), opacité (a)
    pub color: [f32; 4],
    /// x : couche de texture (255 : aucune), y : part ajoutée à la lumière (1 : lueur, 0 : voile),
    /// z : rotation de la texture (radians), w : libre
    pub extra: [f32; 4],
}

/// Les effets en cours
#[derive(Debug)]
pub struct Effects {
    library: EffectLibrary,
    running: Vec<Running>,
    next_seed: u32,
}

impl Default for Effects {
    fn default() -> Self {
        Self { library: EffectLibrary::load_default(), running: Vec::new(), next_seed: 1 }
    }
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    x as f32 / u32::MAX as f32
}

impl Effects {
    /// Commence les visuels annoncés
    pub fn start(&mut self, cues: Vec<VisualCue>) {
        for cue in cues {
            let Some(def) = self.library.effects.get(&cue.effect) else {
                log::debug!("Effet inconnu : {}", cue.effect);
                continue;
            };
            let seconds = cue.hold.map_or_else(|| def.natural_seconds(), |h| h.max(0.05));
            let n = def.layers.len();
            self.next_seed = self.next_seed.wrapping_add(7919);
            let started = Instant::now() + std::time::Duration::from_secs_f32(cue.delay.clamp(0.0, 30.0));
            self.running.push(Running { def: def.clone(), cue, started, seconds, particles: vec![Vec::new(); n], emitted: vec![false; n], carry: vec![0.0; n], last_anchor: None, history: Vec::new(), seed: self.next_seed });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// Le projectile `id` s'est arrêté : les visuels qui le suivaient s'achèvent
    pub fn end_projectile(&mut self, id: u64) {
        let now = Instant::now();
        for run in self.running.iter_mut().filter(|r| r.cue.projectile == Some(id)) {
            run.seconds = run.seconds.min(now.saturating_duration_since(run.started).as_secs_f32());
        }
    }

    /// Avance de `dt` secondes et écrit les quadrilatères de l'image dans `out`
    pub fn update(&mut self, dt: f32, world: &AnchorWorld, out: &mut Vec<EffectQuad>) {
        out.clear();
        let now = Instant::now();
        self.running.retain(|r| now.duration_since(r.started).as_secs_f32() < r.seconds + 2.0);
        for run in &mut self.running {
            // Pas encore commencé (`VisualCue::delay`)
            if now < run.started {
                continue;
            }
            let t = now.duration_since(run.started).as_secs_f32();
            let Some((anchor, forward)) = anchor_of(&run.cue, world, t) else { continue };
            let anchor = match anchor {
                Some(p) => {
                    run.last_anchor = Some(p);
                    p
                }
                None => match run.last_anchor {
                    Some(p) => p,
                    None => continue,
                },
            };
            let scale = world.block * if run.def.area_unit { run.cue.area_blocks.max(0.1) } else { 1.0 };
            run.history.push((t, anchor));
            run.history.retain(|h| h.0 >= t - TRAIL_MEMORY);
            simulate(run, t, dt, anchor, forward, scale, world, out);
            if out.len() >= MAX_QUADS {
                out.truncate(MAX_QUADS);
                break;
            }
        }
    }
}

/// L'ancre d'un visuel à cette image et la direction de la visée. `Some(None)` : l'ancre a
/// disparu (cible morte), l'effet reste où il était.
fn anchor_of(cue: &VisualCue, world: &AnchorWorld, t: f32) -> Option<(Option<Vec3>, Vec3)> {
    let (hero, forward) = match cue.caster {
        None => (world.hero, world.hero_forward),
        Some(id) => {
            let (feet, yaw) = (world.players)(id)?;
            let up = feet.normalize_or_zero();
            let (east, north) = herocraft_common::tangent_frame(up);
            (HeroAnchors::standing(feet, yaw, world.block), north * yaw.cos() + east * yaw.sin())
        }
    };
    let up = hero.feet.normalize_or_zero();
    let aim = cue.aim - up * cue.aim.dot(up);
    let forward = if aim.length_squared() > 1e-6 { aim.normalize() } else { forward };
    let point = match cue.anchor {
        Anchor::RightHand | Anchor::Weapon => Some(hero.right_hand),
        Anchor::LeftHand => Some(hero.left_hand),
        Anchor::Head => Some(hero.head),
        Anchor::Body => Some(hero.body),
        Anchor::Feet => Some(hero.feet),
        Anchor::Aimed | Anchor::Area => Some(match cue.travel {
            // Un projectile file vers son but
            Some((to, seconds)) => cue.point.lerp(to, (t / seconds.max(1e-3)).clamp(0.0, 1.0)),
            None => cue.point,
        }),
        Anchor::Target => cue.creature.and_then(|id| (world.creatures)(id)),
        // Le projectile en vol ; arrêté, l'effet reste où il était
        Anchor::Projectile => cue.projectile.and_then(|id| (world.projectiles)(id)).or(match cue.travel {
            Some((to, seconds)) if cue.projectile.is_none() => Some(cue.point.lerp(to, (t / seconds.max(1e-3)).clamp(0.0, 1.0))),
            _ if cue.projectile.is_none() => Some(cue.point),
            _ => None,
        }),
    };
    Some((point, forward))
}

#[allow(clippy::too_many_arguments)]
fn simulate(run: &mut Running, t: f32, dt: f32, anchor: Vec3, forward: Vec3, scale: f32, world: &AnchorWorld, out: &mut Vec<EffectQuad>) {
    let up = anchor.normalize_or_zero();
    let right = forward.cross(up).normalize_or_zero();
    let block = world.block;
    let total = run.seconds;
    for (i, layer) in run.def.layers.iter().enumerate() {
        let local = t - layer.delay;
        // Une couche en boucle dure tant que le visuel ; une couche datée, sa durée
        let span = if layer.looping { (total - layer.delay).max(0.05) } else { layer.duration.unwrap_or(DEFAULT_LAYER_SECONDS) };
        let origin = anchor + (right * -layer.offset.x + up * layer.offset.y + forward * layer.offset.z) * block;
        // Fondus d'une couche en boucle
        let fade = if layer.looping {
            let fin = if layer.fade_in > 0.0 { (local / layer.fade_in).clamp(0.0, 1.0) } else { 1.0 };
            let fout = if layer.fade_out > 0.0 { ((span - local) / layer.fade_out).clamp(0.0, 1.0) } else { 1.0 };
            fin * fout
        } else {
            1.0
        };
        let additive = if layer.glow > 1.0 { 1.0 } else { 0.0 };
        match &layer.kind {
            LayerKind::Shape(shape) => {
                if !(0.0..span).contains(&local) {
                    continue;
                }
                // Une forme en boucle garde sa valeur de début
                let s = if layer.looping { 0.0 } else { local / span };
                let mut c = mix4(layer.color[0], layer.color[1], s);
                if layer.looping {
                    c = layer.color[0];
                }
                let pulse = if shape.pulse > 0.0 { 0.85 + 0.15 * (local * shape.pulse * std::f32::consts::TAU).sin() } else { 1.0 };
                let rgb = [c[0] * layer.glow, c[1] * layer.glow, c[2] * layer.glow, c[3] * fade];
                // Le bout d'un sillage : là où était l'ancre il y a `life` secondes
                let offset = origin - anchor;
                let tail = run.history.iter().find(|h| h.0 >= t - shape.life).map_or(origin, |h| h.1 + offset);
                push_shape(shape, s, local, origin, tail, up, forward, right, scale * pulse, block, world.eye, rgb, additive, out);
            }
            LayerKind::Particles(p) => {
                let parts = &mut run.particles[i];
                // Émission : d'un coup au début, ou en continu pendant la couche
                if local >= 0.0 && !run.emitted[i] && p.count > 0 {
                    run.emitted[i] = true;
                    for k in 0..p.count.min(400) {
                        parts.push(spawn(p, run.seed.wrapping_add((i as u32) << 16).wrapping_add(k), origin, up, forward, right, scale, block));
                    }
                }
                if p.rate > 0.0 && (0.0..span).contains(&local) {
                    run.carry[i] += p.rate * dt * fade.max(0.2);
                    while run.carry[i] >= 1.0 && parts.len() < 600 {
                        run.carry[i] -= 1.0;
                        let k = run.seed.wrapping_add((i as u32) << 16).wrapping_add((local * 1000.0) as u32).wrapping_add(parts.len() as u32 * 31);
                        parts.push(spawn(p, k, origin, up, forward, right, scale, block));
                    }
                }
                for q in parts.iter_mut() {
                    q.age += dt;
                    if let Some((angle, radius)) = q.orbit.as_mut() {
                        *angle += p.orbit.to_radians() * dt;
                        q.pos = origin + (right * angle.cos() + forward * angle.sin()) * *radius + up * (q.pos - origin).dot(up);
                    } else {
                        q.vel -= up * p.gravity * block * dt;
                        q.vel *= (-p.drag * dt).exp();
                        q.pos += q.vel * dt;
                    }
                    q.angle += q.spin.to_radians() * dt;
                }
                parts.retain(|q| q.age < q.life);
                for q in parts.iter() {
                    if out.len() >= MAX_QUADS {
                        return;
                    }
                    let s = q.age / q.life.max(1e-3);
                    let c = mix4(layer.color[0], layer.color[1], s);
                    let size = p.size.at(s) * block * 0.5;
                    let (u, v) = billboard(q.pos, world.eye, up, size);
                    out.push(EffectQuad {
                        center: [q.pos.x, q.pos.y, q.pos.z, KIND_SPRITE as f32],
                        axis_u: [u.x, u.y, u.z, 0.0],
                        axis_v: [v.x, v.y, v.z, 0.0],
                        color: [c[0] * layer.glow, c[1] * layer.glow, c[2] * layer.glow, c[3] * fade],
                        extra: [p.texture as f32, additive, q.angle, 0.0],
                    });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(p: &ParticleLayer, seed: u32, origin: Vec3, up: Vec3, forward: Vec3, right: Vec3, scale: f32, block: f32) -> Particle {
    let r = |k: u32| hash(seed.wrapping_mul(13).wrapping_add(k));
    let theta = r(1) * std::f32::consts::TAU;
    let radius = p.origin_radius * scale;
    // Plan de l'origine : au sol, ou debout face à la visée
    let (pa, pb) = if p.upright { (right, up) } else { (right, forward) };
    let ring_dir = pa * theta.cos() + pb * theta.sin();
    let start = match p.origin {
        Origin::Point => origin,
        Origin::Sphere => {
            let z = r(2) * 2.0 - 1.0;
            let s = (1.0 - z * z).max(0.0).sqrt();
            origin + (right * (s * theta.cos()) + forward * (s * theta.sin()) + up * z) * radius * r(3).cbrt()
        }
        Origin::Ring => origin + ring_dir * radius,
        Origin::Disc => origin + ring_dir * radius * r(2).sqrt(),
        Origin::Cylinder => origin + ring_dir * radius + up * r(2) * p.origin_height * block,
        Origin::Line => origin + forward * (r(2) - 0.5) * p.origin_length * block,
    };
    let outward = (start - origin).normalize_or(up);
    let base = match p.direction {
        Direction::Up => up,
        Direction::Outward => outward,
        Direction::Inward => -outward,
        Direction::Forward => forward,
        Direction::Random | Direction::None => {
            let z = r(4) * 2.0 - 1.0;
            let s = (1.0 - z * z).max(0.0).sqrt();
            right * (s * (r(5) * std::f32::consts::TAU).cos()) + forward * (s * (r(5) * std::f32::consts::TAU).sin()) + up * z
        }
    };
    // Ouverture du cône autour de la direction
    let spread = p.spread.to_radians() * 0.5 * r(6);
    let side = base.any_orthonormal_vector();
    let side = glam::Quat::from_axis_angle(base, r(7) * std::f32::consts::TAU) * side;
    let dir = if p.direction == Direction::Random { base } else { (base * spread.cos() + side * spread.sin()).normalize_or(base) };
    let speed = if p.direction == Direction::None { 0.0 } else { p.speed.pick(r(8)) * block };
    let orbit = (p.orbit != 0.0).then(|| (theta, (start - origin).dot(pa).hypot((start - origin).dot(pb)).max(0.3 * block)));
    Particle { pos: start, vel: dir * speed, age: 0.0, life: p.life.pick(r(9)).max(0.05), spin: p.spin.pick(r(10)), angle: r(11) * std::f32::consts::TAU, orbit }
}

/// Demi-axes d'un carré face à l'œil
fn billboard(center: Vec3, eye: Vec3, up: Vec3, half: f32) -> (Vec3, Vec3) {
    let view = (eye - center).normalize_or(up);
    let u = up.cross(view).normalize_or(view.any_orthonormal_vector());
    let v = view.cross(u).normalize_or(up);
    (u * half, v * half)
}

#[allow(clippy::too_many_arguments)]
fn push_shape(shape: &ShapeLayer, s: f32, local: f32, origin: Vec3, tail: Vec3, up: Vec3, forward: Vec3, right: Vec3, scale: f32, block: f32, eye: Vec3, rgb: [f32; 4], additive: f32, out: &mut Vec<EffectQuad>) {
    let quad = |center: Vec3, u: Vec3, v: Vec3, kind: u32, p0: f32, p1: f32, texture: u32, rotation: f32| EffectQuad {
        center: [center.x, center.y, center.z, kind as f32],
        axis_u: [u.x, u.y, u.z, p0],
        axis_v: [v.x, v.y, v.z, p1],
        color: rgb,
        extra: [texture as f32, additive, rotation, 0.0],
    };
    // Plan d'une forme plate : au sol, debout face à la visée, ou à plat devant le héros
    let (pa, pb, lift) = if shape.on_ground {
        (right, forward, up * 0.06 * block)
    } else if shape.upright {
        (right, up, Vec3::ZERO)
    } else {
        (right, forward, Vec3::ZERO)
    };
    match shape.shape {
        Shape::Ring | Shape::Disc => {
            let radius = shape.radius.at(s) * scale;
            let thickness = (shape.thickness.at(s) * scale).min(radius);
            let inner = if radius > 0.0 { 1.0 - thickness / radius } else { 0.0 };
            let (kind, p0) = if shape.shape == Shape::Ring { (KIND_RING, inner) } else { (KIND_DISC, 0.0) };
            let rotation = shape.spin.to_radians() * local;
            out.push(quad(origin + lift, pa * radius, pb * radius, kind, p0, 0.0, shape.texture, rotation));
        }
        Shape::Arc => {
            let radius = shape.radius.at(s) * scale;
            let thickness = (shape.thickness.at(s) * scale).min(radius);
            let inner = 1.0 - thickness / radius.max(1e-4);
            let half = shape.angle.to_radians() * 0.5;
            // Le coup se dessine de droite à gauche ; sans `sweep`, l'arc entier dès le début
            let progress = if shape.sweep { (s * 2.5).min(1.0) } else { 1.0 };
            let center = origin + up * 0.1 * block;
            out.push(quad(center, pa * radius, pb * radius, KIND_ARC, inner, half * (2.0 * progress - 1.0) + 1e-3, NO_TEXTURE, half));
        }
        Shape::Streak => {
            let length = shape.length.at(s) * scale;
            let width = shape.width.at(s) * scale;
            let center = origin + forward * length * 0.5;
            let across = forward.cross((eye - center).normalize_or(up)).normalize_or(right);
            out.push(quad(center, across * width * 0.5, forward * length * 0.5, KIND_BEAM, 0.0, 0.0, NO_TEXTURE, 0.0));
        }
        Shape::Column => {
            let radius = shape.radius.at(s) * scale;
            let height = shape.height.at(s) * scale;
            let center = origin + up * height * 0.5;
            let across = up.cross((eye - center).normalize_or(forward)).normalize_or(right);
            out.push(quad(center, across * radius, up * height * 0.5, KIND_BEAM, 0.0, 0.0, NO_TEXTURE, 0.0));
        }
        Shape::Dome => {
            let radius = shape.radius.at(s) * scale;
            let (u, v) = billboard(origin, eye, up, radius);
            out.push(quad(origin, u, v, KIND_DOME, 0.0, 0.0, NO_TEXTURE, 0.0));
        }
        Shape::Sprite => {
            let half = shape.size.at(s) * scale * 0.5;
            let (u, v) = billboard(origin, eye, up, half);
            out.push(quad(origin, u, v, KIND_SPRITE, 0.0, 0.0, shape.texture, shape.spin.to_radians() * local));
        }
        // Le sillage d'un projectile : un ruban de l'ancre à là où elle était un instant plus tôt
        Shape::Trail => {
            let along = origin - tail;
            let length = along.length();
            if length > 1e-3 * block {
                let width = shape.width.at(0.0) * scale;
                let center = (origin + tail) * 0.5;
                let dir = along / length;
                let across = dir.cross((eye - center).normalize_or(up)).normalize_or(right);
                out.push(quad(center, across * width * 0.5, dir * length * 0.5, KIND_BEAM, 0.0, 0.0, NO_TEXTURE, 0.0));
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Rendu

pub struct EffectRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    bind_group: wgpu::BindGroup,
    quads: wgpu::Buffer,
    count: u32,
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, target: post::SceneTarget) -> wgpu::RenderPipeline {
    let shader = crate::shaders::create_shader_module(device, "Effects Shader", include_str!("effects.wgsl"), "effects.wgsl");
    // Couleur prémultipliée : une couche qui luit n'occulte rien (alpha 0), un voile occulte
    let blend = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Effects Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_effect"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<EffectQuad>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_effect"),
            targets: &[Some(wgpu::ColorTargetState { format: target.format, blend: Some(blend), write_mask: wgpu::ColorWrites::COLOR })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: target.sample_count, mask: !0, alpha_to_coverage_enabled: false },
        multiview: None,
        cache: None,
    })
}

/// Les textures de `assets/effects/textures/`, ramenées à `TEXTURE_SIZE` et rangées en couches.
/// Une texture absente reste une couche blanche.
fn load_textures() -> Vec<u8> {
    let side = TEXTURE_SIZE as usize;
    let mut pixels = vec![255u8; side * side * 4 * TEXTURES.len()];
    let dir = herocraft_common::skills::asset_dir("assets/effects/textures");
    for (layer, name) in TEXTURES.iter().enumerate() {
        let Some(path) = dir.as_ref().map(|d| d.join(name)) else { continue };
        let Some((w, h, rgba)) = read_png(&path) else {
            log::warn!("Texture d'effet illisible : {}", path.display());
            continue;
        };
        let base = layer * side * side * 4;
        for y in 0..side {
            for x in 0..side {
                let sx = x * w / side;
                let sy = y * h / side;
                let src = (sy * w + sx) * 4;
                let dst = base + (y * side + x) * 4;
                pixels[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
            }
        }
    }
    pixels
}

pub(crate) fn read_png(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::fs::File::open(path).ok()?);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((w, h, rgba))
}

impl EffectRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene_layout: &wgpu::BindGroupLayout, target: post::SceneTarget) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Effects Textures"),
            size: wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: TEXTURES.len() as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &load_textures(),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(TEXTURE_SIZE * 4), rows_per_image: Some(TEXTURE_SIZE) },
            wgpu::Extent3d { width: TEXTURE_SIZE, height: TEXTURE_SIZE, depth_or_array_layers: TEXTURES.len() as u32 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Effects Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Effects Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Effects Bind Group"),
            layout: &texture_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("Effects Pipeline Layout"), bind_group_layouts: &[scene_layout, &texture_layout], push_constant_ranges: &[] });
        let quads = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Effects Quads"),
            size: (MAX_QUADS * std::mem::size_of::<EffectQuad>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline: create_pipeline(device, &layout, target), layout, bind_group, quads, count: 0 }
    }

    pub fn set_target(&mut self, device: &wgpu::Device, target: post::SceneTarget) {
        self.pipeline = create_pipeline(device, &self.layout, target);
    }

    pub fn update(&mut self, queue: &wgpu::Queue, quads: &[EffectQuad]) {
        let quads = &quads[..quads.len().min(MAX_QUADS)];
        self.count = quads.len() as u32;
        if !quads.is_empty() {
            queue.write_buffer(&self.quads, 0, bytemuck::cast_slice(quads));
        }
    }

    /// À appeler dans la passe 3D, après les objets opaques
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, scene_bind_group: &wgpu::BindGroup) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_bind_group, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.quads.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_parses() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/effects");
        let (library, errors) = EffectLibrary::load_dir(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(library.effects.len(), 52);
        let ring = &library.effects["shockRing"];
        assert!(ring.area_unit);
        assert!((ring.natural_seconds() - 1.1).abs() < 1e-4, "{}", ring.natural_seconds());
    }

    #[test]
    fn a_running_effect_draws_quads_then_stops() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/effects");
        let (library, _) = EffectLibrary::load_dir(&dir);
        let mut effects = Effects { library, running: Vec::new(), next_seed: 1 };
        let feet = Vec3::new(0.0, 1000.0, 0.0);
        let cue = VisualCue { effect: "shockRing".into(), anchor: Anchor::Feet, caster: None, point: feet, creature: None, aim: Vec3::NEG_Z, hold: None, area_blocks: 3.0, travel: None, delay: 0.0, projectile: None };
        effects.start(vec![cue]);
        let no_player = |_: u64| None; let no_creature = |_: u64| None;
        let world = AnchorWorld { hero: HeroAnchors::standing(feet, 0.0, 1.0), hero_forward: Vec3::NEG_Z, players: &no_player, creatures: &no_creature, projectiles: &no_creature, eye: feet + Vec3::new(0.0, 3.0, 6.0), block: 1.0 };
        let mut out = Vec::new();
        effects.update(0.016, &world, &mut out);
        assert_eq!(out.len(), 1, "l'anneau seul avant les étincelles");
        assert_eq!(out[0].center[3] as u32, KIND_RING);
        // L'anneau s'étend vers le rayon de la zone (3 blocs)
        let radius = Vec3::from_slice(&out[0].axis_u[..3]).length();
        assert!(radius < 1.0, "{radius}");
        effects.running[0].started -= std::time::Duration::from_millis(350);
        effects.update(0.016, &world, &mut out);
        assert!(out.len() > 30, "les étincelles partent : {}", out.len());
        effects.running[0].started -= std::time::Duration::from_secs(5);
        effects.update(0.016, &world, &mut out);
        assert!(effects.is_empty());
    }

    #[test]
    fn a_trail_follows_its_projectile_and_ends_with_it() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/effects");
        let (library, _) = EffectLibrary::load_dir(&dir);
        let mut effects = Effects { library, running: Vec::new(), next_seed: 1 };
        let feet = Vec3::new(0.0, 1000.0, 0.0);
        let cue = VisualCue { effect: "arrowTrail".into(), anchor: Anchor::Projectile, caster: None, point: feet, creature: None, aim: Vec3::NEG_Z, hold: Some(8.0), area_blocks: 1.0, travel: None, delay: 0.0, projectile: Some(3) };
        effects.start(vec![cue]);
        let no_player = |_: u64| None;
        let no_creature = |_: u64| None;
        let mut out = Vec::new();
        // La flèche avance de 2 blocs entre deux images : le ruban va de l'ancienne position à la nouvelle
        for (k, z) in [0.0f32, -2.0].into_iter().enumerate() {
            let at = move |id: u64| (id == 3).then_some(feet + Vec3::new(0.0, 1.4, z));
            let world = AnchorWorld { hero: HeroAnchors::standing(feet, 0.0, 1.0), hero_forward: Vec3::NEG_Z, players: &no_player, creatures: &no_creature, projectiles: &at, eye: feet + Vec3::new(3.0, 3.0, 0.0), block: 1.0 };
            effects.running[0].started -= std::time::Duration::from_millis(50 * k as u64);
            effects.update(0.05, &world, &mut out);
        }
        assert_eq!(out.len(), 1, "un ruban");
        assert_eq!(out[0].center[3] as u32, KIND_BEAM);
        let half = Vec3::from_slice(&out[0].axis_v[..3]).length();
        assert!((half - 1.0).abs() < 0.05, "le ruban couvre les 2 blocs parcourus : {half}");
        // Le projectile s'arrête : le visuel s'achève
        effects.end_projectile(3);
        effects.running[0].started -= std::time::Duration::from_secs(3);
        let gone = |_: u64| None;
        let world = AnchorWorld { hero: HeroAnchors::standing(feet, 0.0, 1.0), hero_forward: Vec3::NEG_Z, players: &no_player, creatures: &no_creature, projectiles: &gone, eye: feet, block: 1.0 };
        effects.update(0.05, &world, &mut out);
        assert!(effects.is_empty());
    }
}

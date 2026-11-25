//! Lecture des cinématiques en jeu (`assets/cinematics/`, voir le README du dossier) : la caméra
//! suit les plans, l'écran se couvre de bandeaux noirs, de textes et de fondus, le joueur ne bouge
//! plus et peut passer (deux appuis sur Échap, Espace ou Entrée).
//!
//! Le minutage, les déclencheurs et les événements renvoyés au jeu sont tenus par
//! `herocraft_common::cinematics` ; ce module place la caméra dans le monde et peint l'écran.

use glam::Vec3;
use herocraft_common::cinematics::{CaptionStyle, CinematicCatalog, CinematicPlayer, GameEvent, SkipRequest};

use crate::camera::{CinematicView, OrbitCamera};
use crate::paper_cards::{CardInstance, CardSet, CardUniform};

/// Hauteur de la scène de papier au-dessus du joueur, en blocs
const STAGE_HEIGHT_BLOCKS: f32 = 1500.0;
/// Distance du fond de papier à la caméra, en blocs
const BACKDROP_BLOCKS: f32 = 120.0;

/// Couleur `#RRGGBB` en composantes linéaires
fn linear_color(text: &str) -> [f32; 3] {
    let srgb = herocraft_common::cinematics::parse_color(text).unwrap_or([255, 255, 255]);
    srgb.map(|c| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    })
}

/// Repère de la cinématique en cours, figé à son début : origine et axes du monde, taille d'un bloc
#[derive(Debug, Clone, Copy)]
struct AnchorFrame {
    origin: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    block: f32,
}

impl AnchorFrame {
    fn to_world(self, local: Vec3) -> Vec3 {
        self.origin + (self.right * local.x + self.up * local.y + self.forward * local.z) * self.block
    }
}

/// Le lecteur de cinématiques du client
pub(crate) struct Cinematics {
    pub player: CinematicPlayer,
    anchor: Option<AnchorFrame>,
    /// Monde dont `seen` est la liste des cinématiques déjà vues
    world: Option<std::path::PathBuf>,
}

/// Émis par `fin-de-la-vision` : la Pierre obtenue est absorbée
pub(crate) const VISION_ENDED: &str = "vision_ended";
/// Envoyé à l'entrée dans la vision d'un temple (`l-eclat` s'y joue)
pub(crate) const VISION_ENTERED: &str = "vision_entered";
/// Renvoyé par `l-eclat` : le temps s'arrête ; `TIME_RESUMED` le fait repartir
pub(crate) const TIME_FROZEN: &str = "time_frozen";
pub(crate) const TIME_RESUMED: &str = "time_resumed";
/// Le tutoriel de la vision : l'éclat est ramassé, le soldat s'éveille en héros (`l-eveil`)
pub(crate) const VISION_AWAKENED: &str = "vision_awakened";
/// La charge de la vision : le bélier a fait voler la grande porte en éclats (`la-porte-cede`)
pub(crate) const GATE_BROKEN: &str = "gate_broken";

/// Le temps est-il figé après l'événement `event`, s'il l'était (`frozen`) avant ?
pub(crate) fn frozen_after(event: &GameEvent, frozen: bool) -> bool {
    match event.name.as_str() {
        TIME_FROZEN => true,
        TIME_RESUMED | VISION_ENDED => false,
        _ => frozen,
    }
}

/// Événement envoyé une fois, à la première image du jeu (la Genèse s'y joue)
pub(crate) const LAUNCH_EVENT: &str = "game_launched";

/// Fichier des cinématiques déjà vues, dans la sauvegarde du monde
pub(crate) const SEEN_FILE: &str = "cinematiques.json";

impl Cinematics {
    pub(crate) fn load() -> Self {
        Self { player: CinematicPlayer::new(CinematicCatalog::load_default()), anchor: None, world: None }
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    /// Les découpes du plan en cours, et celles du plan suivant (à préparer d'avance)
    pub(crate) fn card_sets(&self) -> (Option<CardSet>, Option<CardSet>) {
        let Some(playing) = self.player.playing() else { return (None, None) };
        let cinematic = &playing.cinematic;
        let set = |t: f32| {
            let (shot, start, end) = cinematic.shot_span_at(t);
            let images: Vec<String> = cinematic.card_images_between(start, end).into_iter().map(str::to_string).collect();
            (CardSet { key: format!("{}#{shot}", cinematic.id), images }, end)
        };
        let (current, end) = set(playing.time);
        let next = (end < cinematic.duration()).then(|| set(end + 1e-3).0);
        (Some(current), next)
    }

    /// Change de monde (sauvegarde `dir`) : la lecture s'arrête et les cinématiques déjà vues sont
    /// celles de ce monde
    pub(crate) fn set_world(&mut self, dir: std::path::PathBuf) {
        if self.world.as_ref() == Some(&dir) {
            return;
        }
        self.player.stop();
        self.anchor = None;
        self.player.load_seen(&dir.join(SEEN_FILE));
        self.world = Some(dir);
    }

    /// Un événement du jeu (« Pierre obtenue »…) : les cinématiques qu'il déclenche se jouent
    pub(crate) fn notify(&mut self, event: &GameEvent) -> usize {
        self.player.notify(event)
    }

    /// Le joueur demande à passer
    pub(crate) fn request_skip(&mut self) -> SkipRequest {
        self.player.request_skip()
    }

    /// Avance la lecture et place la caméra. Renvoie les événements que la cinématique renvoie au
    /// jeu (changer d'époque, fin…), déjà renvoyés au lecteur lui-même pour qu'un événement puisse
    /// en déclencher une autre.
    pub(crate) fn update(&mut self, dt: f32, camera: &mut OrbitCamera, generator: &herocraft_world::PlanetWorldGenerator) -> Vec<GameEvent> {
        self.player.update(dt);
        let mut events = self.player.take_events();
        // Une cinématique renvoie un événement qui en déclenche une autre : elle se joue à la suite
        for event in &events {
            self.player.notify(event);
        }
        events.extend(self.player.take_events());

        if self.player.take_started() {
            if camera.target_distance > camera.base_radius {
                camera.transition_to_ground(generator);
            }
            self.anchor = Some(self.anchor_for(camera));
            if let Some(dir) = &self.world
                && let Err(e) = self.player.save_seen(&dir.join(SEEN_FILE)) {
                log::warn!("Cinématiques vues non enregistrées : {e}");
            }
        }

        match (self.player.frame(), self.anchor) {
            (Some(frame), Some(anchor)) => {
                // Le joueur reste immobile, les mains vides de toute entrée
                camera.release_inputs();
                camera.jump_requested = false;
                let shot = frame.camera;
                let mut eye = anchor.to_world(shot.eye);
                let target = anchor.to_world(shot.target);
                // Jamais sous le relief
                let ground = camera.ground_radius_at(eye, generator) + 0.2 * anchor.block;
                if eye.length() < ground {
                    eye = eye.normalize_or_zero() * ground;
                }
                let look = (target - eye).normalize_or(anchor.forward);
                let up = eye.normalize_or(anchor.up);
                let up = glam::Quat::from_axis_angle(look, shot.roll) * up;
                camera.cinematic = Some(CinematicView { eye, target, up, fov: shot.fov });
            }
            _ => {
                camera.cinematic = None;
                self.anchor = None;
            }
        }
        events
    }

    /// Repère de la cinématique qui commence : les pieds et le regard du joueur, ou la position
    /// de l'événement déclencheur
    fn anchor_for(&self, camera: &OrbitCamera) -> AnchorFrame {
        let (_, forward, right, _) = camera.look_vectors();
        let playing = self.player.playing();
        let at_event = playing
            .filter(|p| p.cinematic.anchor == herocraft_common::cinematics::Anchor::Event)
            .and_then(|p| p.event.as_ref())
            .and_then(|e| e.position);
        let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
        let mut origin = at_event.unwrap_or(camera.player_pos);
        let up = origin.normalize_or_zero();
        // La scène de papier : loin au-dessus du joueur, au-dessus des nuages et hors du monde
        if playing.is_some_and(|p| p.cinematic.anchor == herocraft_common::cinematics::Anchor::Stage) {
            origin += up * STAGE_HEIGHT_BLOCKS * block;
        }
        // Le regard du joueur, ramené dans le plan du sol de l'origine
        let forward = (forward - up * forward.dot(up)).normalize_or(forward);
        let right = forward.cross(up).normalize_or(right);
        AnchorFrame { origin, right, up, forward, block }
    }

    /// Les cartes de papier de la cinématique en cours, et la lumière qui les éclaire (aucune
    /// carte hors des cinématiques)
    pub(crate) fn cards(&self, camera: &OrbitCamera, layers: &crate::paper_cards::CardLayers) -> (CardUniform, Vec<CardInstance>) {
        let mut out = Vec::new();
        let (Some(playing), Some(anchor), Some(view)) = (self.player.playing(), self.anchor, camera.cinematic) else {
            return (CardUniform::default(), out);
        };
        let cinematic = &playing.cinematic;
        let stage = &cinematic.stage;
        let light = &stage.light;
        let to_world_dir = |v: Vec3| (anchor.right * v.x + anchor.up * v.y + anchor.forward * v.z).normalize_or(anchor.up);
        let [lr, lg, lb] = linear_color(&light.color);
        let uniform = CardUniform {
            view_proj: camera.build_view_projection_matrix().to_cols_array_2d(),
            light_dir: to_world_dir(Vec3::from(light.direction)).extend(light.ambient.clamp(0.0, 1.0)).to_array(),
            light_color: [lr * light.intensity, lg * light.intensity, lb * light.intensity, playing.time],
            params: [anchor.block, stage.vignette.clamp(0.0, 1.0), camera.aspect.max(0.1), 0.0],
            eye: view.eye.extend(1.0).to_array(),
        };

        // Le fond : une grande feuille face à la caméra, derrière tout le reste
        if let Some(color) = &stage.backdrop {
            let look = (view.target - view.eye).normalize_or(anchor.forward);
            let distance = BACKDROP_BLOCKS * anchor.block;
            let half_h = distance * (view.fov * 0.5).tan() * 1.15;
            let half_w = half_h * camera.aspect.max(0.1) * 1.1;
            let right = look.cross(view.up).normalize_or(anchor.right);
            let up = right.cross(look).normalize_or(anchor.up);
            let [r, g, b] = linear_color(color);
            out.push(CardInstance {
                center: (view.eye + look * distance).extend(crate::paper_cards::NO_TEXTURE as f32).to_array(),
                axis_u: (right * half_w).extend(0.0).to_array(),
                axis_v: (up * half_h).extend(0.0).to_array(),
                color: [r, g, b, 1.0],
                extra: [1.0, 1.0, 0.0, 0.0],
                uv: [0.0, 0.0, 1.0, 1.0],
            });
        }

        for sample in cinematic.cards_at(playing.time) {
            let card = sample.card;
            let sprite = card.image.as_deref().and_then(|image| layers.get(image));
            // Image absente : signalée au chargement par les tests, la carte reste unie
            let aspect = sprite.map_or(1.0, |s| s.aspect());
            let width = sample.width;
            let height = sample.height.unwrap_or(width / aspect.max(0.01));
            // La carte s'étend sur la marge de la découpe, où se dessine le liseré
            let [mx, my] = sprite.map_or([0.0; 2], |s| s.margin());
            // Repère de la carte dans celui de l'ancre : face vers l'arrière (-z), puis tournée
            let rotation = glam::Quat::from_rotation_y(-sample.yaw) * glam::Quat::from_rotation_x(sample.pitch) * glam::Quat::from_rotation_z(-sample.roll);
            let u = to_world_dir(rotation * Vec3::X) * width * (0.5 + mx) * anchor.block;
            let v = to_world_dir(rotation * Vec3::Y) * height * (0.5 + my) * anchor.block;
            let [r, g, b] = card.color.as_deref().map_or([1.0; 3], linear_color);
            let [r, g, b] = [r * sample.tint[0], g * sample.tint[1], b * sample.tint[2]];
            // Le liseré en pixels de l'atlas, au plus la marge
            let border = sprite.map_or(0.0, |s| (card.border.max(0.0) * s.size[0]).min(crate::paper_cards::PAD as f32 - 1.0));
            out.push(CardInstance {
                center: anchor.to_world(sample.center).extend(sprite.map_or(crate::paper_cards::NO_TEXTURE, |s| s.page) as f32).to_array(),
                axis_u: u.extend(card.curl.max(0.0)).to_array(),
                axis_v: v.extend(border).to_array(),
                color: [r, g, b, sample.opacity],
                extra: [
                    card.glow,
                    0.0,
                    match card.blend {
                        herocraft_common::cinematics::CardBlend::Cut => crate::paper_cards::BLEND_CUT,
                        herocraft_common::cinematics::CardBlend::Alpha => crate::paper_cards::BLEND_ALPHA,
                        herocraft_common::cinematics::CardBlend::Add => crate::paper_cards::BLEND_ADD,
                    },
                    0.0,
                ],
                uv: sprite.map_or([0.0, 0.0, 1.0, 1.0], |s| s.uv),
            });
        }
        (uniform, out)
    }

    /// Ce que l'interface doit peindre par-dessus le jeu, `skip_key` étant le nom de la touche
    pub(crate) fn overlay(&self) -> Option<Overlay> {
        let playing = self.player.playing()?;
        let frame = self.player.frame()?;
        Some(Overlay {
            letterbox: frame.letterbox,
            fade: frame.fade,
            captions: frame
                .captions
                .iter()
                .map(|c| OverlayCaption { text: c.caption.text.clone(), speaker: c.caption.speaker.clone(), style: c.caption.style, alpha: c.alpha })
                .collect(),
            skip_armed: playing.skip_armed.is_some(),
            skippable: playing.cinematic.skippable,
        })
    }
}

/// Un texte à peindre
#[derive(Debug, Clone)]
pub(crate) struct OverlayCaption {
    pub text: String,
    pub speaker: Option<String>,
    pub style: CaptionStyle,
    pub alpha: f32,
}

/// Ce que peint l'interface pendant une cinématique
#[derive(Debug, Clone)]
pub(crate) struct Overlay {
    pub letterbox: f32,
    pub fade: ([u8; 3], f32),
    pub captions: Vec<OverlayCaption>,
    pub skip_armed: bool,
    pub skippable: bool,
}

/// Part de la hauteur de l'écran prise par chaque bandeau noir
const LETTERBOX_SHARE: f32 = 0.11;

/// Couleur du texte : parchemin clair, comme les maquettes HTML
const INK: egui::Color32 = egui::Color32::from_rgb(244, 233, 210);
const GOLD: egui::Color32 = egui::Color32::from_rgb(226, 190, 120);

fn faded(color: egui::Color32, alpha: f32) -> egui::Color32 {
    let a = (alpha.clamp(0.0, 1.0) * color.a() as f32) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

/// Peint la cinématique par-dessus le jeu : bandeaux, voile, textes, invitation à passer
pub(crate) fn draw(ctx: &egui::Context, overlay: &Overlay) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("cinematic")));
    let band = screen.height() * LETTERBOX_SHARE * overlay.letterbox;

    // Voile du fondu, sous les bandeaux
    let ([r, g, b], alpha) = overlay.fade;
    if alpha > 0.0 {
        painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0) as u8));
    }
    if band > 0.5 {
        painter.rect_filled(egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, screen.min.y + band)), 0.0, egui::Color32::BLACK);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(screen.min.x, screen.max.y - band), screen.max), 0.0, egui::Color32::BLACK);
    }

    // Un texte clair sur un fond blanc se perd : il passe au sombre quand le voile est clair
    let light_veil = alpha > 0.5 && (r as u32 + g as u32 + b as u32) > 3 * 160;
    let ink = if light_veil { egui::Color32::from_rgb(52, 40, 28) } else { INK };
    let shadow = if light_veil { egui::Color32::from_white_alpha(120) } else { egui::Color32::from_black_alpha(200) };
    let wrap = (screen.width() * 0.7).min(900.0);

    for caption in &overlay.captions {
        let (font, color, anchor, pos) = match caption.style {
            CaptionStyle::Subtitle => (
                crate::ui_skin::strong_font(22.0),
                ink,
                egui::Align2::CENTER_BOTTOM,
                egui::pos2(screen.center().x, screen.max.y - band.max(screen.height() * 0.06) - 14.0),
            ),
            CaptionStyle::Narration => (egui::FontId::proportional(28.0), ink, egui::Align2::CENTER_CENTER, egui::pos2(screen.center().x, screen.min.y + screen.height() * 0.68)),
            CaptionStyle::Title => (crate::ui_skin::title_font(64.0), if light_veil { ink } else { GOLD }, egui::Align2::CENTER_CENTER, screen.center()),
            CaptionStyle::Place => (crate::ui_skin::title_font(20.0), if light_veil { ink } else { GOLD }, egui::Align2::LEFT_TOP, egui::pos2(screen.min.x + 36.0, screen.min.y + band + 22.0)),
        };
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap;
        job.halign = match anchor.x() {
            egui::Align::Min => egui::Align::Min,
            _ => egui::Align::Center,
        };
        if let Some(speaker) = caption.speaker.as_ref().filter(|_| caption.style == CaptionStyle::Subtitle) {
            job.append(&format!("{speaker} : "), 0.0, egui::TextFormat { font_id: font.clone(), color: faded(GOLD, caption.alpha), ..Default::default() });
        }
        let italics = caption.style == CaptionStyle::Narration;
        job.append(&caption.text, 0.0, egui::TextFormat { font_id: font.clone(), color: faded(color, caption.alpha), italics, ..Default::default() });
        let galley = painter.layout_job(job);
        // Un texte centré est mis en page autour de x = 0 : on le recale sur son coin
        let rect = anchor.anchor_size(pos, galley.size());
        let origin = rect.min - galley.rect.min.to_vec2();
        // Ombre portée : lisible sur un ciel clair comme sur la neige
        let mut shadow_job = (*galley.job).clone();
        for section in &mut shadow_job.sections {
            section.format.color = faded(shadow, caption.alpha);
        }
        let shadow_galley = painter.layout_job(shadow_job);
        painter.galley(origin + egui::vec2(1.5, 2.0), shadow_galley, egui::Color32::WHITE);
        painter.galley(origin, galley, egui::Color32::WHITE);
    }

    if overlay.skippable {
        let text = if overlay.skip_armed { "Appuyez encore pour passer" } else { "Échap · Passer" };
        let alpha = if overlay.skip_armed { 0.95 } else { 0.35 };
        let pos = egui::pos2(screen.max.x - 28.0, screen.max.y - (band - 26.0).max(0.0) / 2.0 - 14.0);
        let color = if light_veil { faded(egui::Color32::from_rgb(52, 40, 28), alpha) } else { faded(INK, alpha) };
        painter.text(pos, egui::Align2::RIGHT_CENTER, text, egui::FontId::proportional(15.0), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_axes() {
        let frame = AnchorFrame { origin: Vec3::new(0.0, 100.0, 0.0), right: Vec3::X, up: Vec3::Y, forward: Vec3::NEG_Z, block: 0.5 };
        assert_eq!(frame.to_world(Vec3::new(2.0, 4.0, 6.0)), Vec3::new(1.0, 102.0, -3.0));
    }

    #[test]
    fn test_overlay_draws_without_panicking() {
        let ctx = egui::Context::default();
        let overlay = Overlay {
            letterbox: 1.0,
            fade: ([255, 255, 255], 0.8),
            captions: [CaptionStyle::Subtitle, CaptionStyle::Narration, CaptionStyle::Title, CaptionStyle::Place]
                .into_iter()
                .map(|style| OverlayCaption { text: "Tu ouvres les yeux.".into(), speaker: Some("Le Roi".into()), style, alpha: 0.7 })
                .collect(),
            skip_armed: true,
            skippable: true,
        };
        crate::ui_skin::install(&ctx);
        // Les polices installées servent à partir de l'image suivante
        for _ in 0..2 {
            ctx.run_pass(egui::RawInput::default(), |ctx| draw(ctx, &overlay)).textures_delta.clear();
        }
    }

    /// Captures de l'écran des cinématiques du jeu à quelques instants, sans la 3D (fond au choix) :
    /// `HEROCRAFT_CAPTURES=captures [HEROCRAFT_CAPTURES_FOND=image.png] cargo test -p
    /// herocraft_client snapshot_cinematics -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_cinematics() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let background = std::env::var("HEROCRAFT_CAPTURES_FOND").ok().map(std::path::PathBuf::from);
        let (width, height) = (1280_usize, 720_usize);
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/cinematics");
        let (catalog, _) = CinematicCatalog::load_dir(&source);
        for id in catalog.ids() {
            let cinematic = catalog.get(&id).unwrap().clone();
            // Un instant au milieu de chaque texte
            for (i, caption) in cinematic.captions.iter().enumerate() {
                let mut player = CinematicPlayer::new(catalog.clone());
                player.play(&id).unwrap();
                player.update(caption.at + caption.duration / 2.0);
                let cinematics = Cinematics { player, anchor: None, world: None };
                let overlay = cinematics.overlay().unwrap();
                let ctx = egui::Context::default();
                crate::ui_skin::install(&ctx);
                let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
                snapshot.set_background(background.as_deref());
                let mut last = None;
                for _ in 0..2 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_pass(input, |ctx| draw(ctx, &overlay));
                    snapshot.apply_textures(&mut output.textures_delta);
                    last = Some(output);
                }
                let output = last.unwrap();
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                snapshot.paint(&primitives, output.pixels_per_point);
                snapshot.save(&dir.join(format!("{id}-{}.png", i + 1)));
            }
        }
    }

    /// Un demi-flottant IEEE en `f32`
    fn half_to_f32(h: u16) -> f32 {
        let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
        let exp = ((h >> 10) & 0x1f) as i32;
        let frac = (h & 0x3ff) as f32;
        sign * match exp {
            0 => frac / 1024.0 * 2f32.powi(-14),
            31 => f32::INFINITY,
            e => (1.0 + frac / 1024.0) * 2f32.powi(e - 15),
        }
    }

    /// Captures 3D de la scène de papier, rendues hors écran par le shader des cartes seul (sans
    /// ciel, monde ni textes ; tonemapping simplifié) : `HEROCRAFT_CAPTURES=captures cargo test -p
    /// herocraft_client snapshot_paper_stage -- --ignored` (cinématique au choix avec
    /// `HEROCRAFT_CINEMATIQUE`, `essai-papier` par défaut ; instants avec `HEROCRAFT_TEMPS`)
    #[test]
    #[ignore]
    fn snapshot_paper_stage() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let id = std::env::var("HEROCRAFT_CINEMATIQUE").unwrap_or_else(|_| "essai-papier".into());
        let (width, height) = (1280_u32, 720_u32);
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/cinematics");
        let (catalog, _) = CinematicCatalog::load_dir(&source);
        let duration = catalog.get(&id).expect("cinématique inconnue").duration();
        let images = crate::paper_cards::CardImages::empty();

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("aucun adaptateur graphique");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        // Demi-flottants : mélangeables, comme la cible HDR du jeu
        let format = wgpu::TextureFormat::Rgba16Float;
        let mut renderer = crate::paper_cards::CardRenderer::new(&device, &queue, crate::post::SceneTarget { sample_count: 1, format }, images);
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (width * height * 8) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });

        let generator = herocraft_world::PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 2026, ..Default::default() });
        let radius = crate::camera::DEFAULT_RADIUS;
        // Instants des captures (`HEROCRAFT_TEMPS=12,30.5`), six régulières par défaut
        let times: Vec<f32> = match std::env::var("HEROCRAFT_TEMPS") {
            Ok(list) => list.split(',').filter_map(|t| t.trim().parse().ok()).collect(),
            Err(_) => (0..6).map(|f| duration * (f as f32 + 0.5) / 6.0).collect(),
        };
        for (f, &t) in times.iter().enumerate() {
            let mut camera = OrbitCamera::new(0.0, radius, &generator);
            camera.aspect = width as f32 / height as f32;
            let mut cinematics = Cinematics { player: CinematicPlayer::new(catalog.clone()), anchor: None, world: None };
            cinematics.player.play(&id).unwrap();
            cinematics.update(0.0, &mut camera, &generator);
            cinematics.update(t, &mut camera, &generator);
            let (current, _) = cinematics.card_sets();
            renderer.prepare(&device, &queue, current.as_ref(), None);
            let (uniform, cards) = cinematics.cards(&camera, &renderer.layers);
            renderer.update(&queue, &uniform, &cards);
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let color_view = color.create_view(&Default::default());
                let depth_view = depth.create_view(&Default::default());
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { multiview_mask: None,
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { depth_slice: None, view: &color_view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                renderer.draw(&mut pass);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 8), rows_per_image: Some(height) } },
                size,
            );
            queue.submit([encoder.finish()]);
            readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let data: Vec<f32> = bytemuck::cast_slice::<u8, u16>(&readback.slice(..).get_mapped_range().expect("lecture du tampon")).iter().map(|&h| half_to_f32(h)).collect();
            readback.unmap();
            // Exposition du jeu (-0,3 EV), courbe filmique simple, puis sRGB ; voile de la cinématique
            let frame = cinematics.player.frame().unwrap();
            let ([fr, fg, fb], fade) = frame.fade;
            let veil = [fr, fg, fb].map(|c| c as f32 / 255.0);
            let mut rgb = Vec::with_capacity((width * height * 3) as usize);
            for px in data.chunks(4) {
                for (i, &c) in px[..3].iter().enumerate() {
                    let x = c * 2f32.powf(-0.3);
                    let mapped = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
                    let srgb = if mapped <= 0.0031308 { mapped * 12.92 } else { 1.055 * mapped.clamp(0.0, 1.0).powf(1.0 / 2.4) - 0.055 };
                    let v = srgb.clamp(0.0, 1.0) * (1.0 - fade) + veil[i] * fade;
                    rgb.push((v * 255.0).round() as u8);
                }
            }
            let file = std::fs::File::create(dir.join(format!("{id}-3d-{}.png", f + 1))).unwrap();
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&rgb).unwrap();
        }
    }
}

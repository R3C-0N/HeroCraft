//! Lecture des cinématiques en jeu (`assets/cinematics/`, voir le README du dossier) : la caméra
//! suit les plans, l'écran se couvre de bandeaux noirs, de textes et de fondus, le joueur ne bouge
//! plus et peut passer (deux appuis sur Échap, Espace ou Entrée).
//!
//! Le minutage, les déclencheurs et les événements renvoyés au jeu sont tenus par
//! `herocraft_common::cinematics` ; ce module place la caméra dans le monde et peint l'écran.

use glam::Vec3;
use herocraft_common::cinematics::{CaptionStyle, CinematicCatalog, CinematicPlayer, GameEvent, SkipRequest};

use crate::camera::{CinematicView, OrbitCamera};

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
    fn to_world(&self, local: Vec3) -> Vec3 {
        self.origin + (self.right * local.x + self.up * local.y + self.forward * local.z) * self.block
    }
}

/// Le lecteur de cinématiques du client
pub struct Cinematics {
    pub player: CinematicPlayer,
    anchor: Option<AnchorFrame>,
    /// Monde dont `seen` est la liste des cinématiques déjà vues
    world: Option<std::path::PathBuf>,
}

/// Fichier des cinématiques déjà vues, dans la sauvegarde du monde
pub const SEEN_FILE: &str = "cinematiques.json";

impl Cinematics {
    pub fn load() -> Self {
        Self { player: CinematicPlayer::new(CinematicCatalog::load_default()), anchor: None, world: None }
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    /// Change de monde (sauvegarde `dir`) : la lecture s'arrête et les cinématiques déjà vues sont
    /// celles de ce monde
    pub fn set_world(&mut self, dir: std::path::PathBuf) {
        if self.world.as_ref() == Some(&dir) {
            return;
        }
        self.player.stop();
        self.anchor = None;
        self.player.load_seen(&dir.join(SEEN_FILE));
        self.world = Some(dir);
    }

    /// Un événement du jeu (« Pierre obtenue »…) : les cinématiques qu'il déclenche se jouent
    pub fn notify(&mut self, event: &GameEvent) -> usize {
        self.player.notify(event)
    }

    /// Le joueur demande à passer
    pub fn request_skip(&mut self) -> SkipRequest {
        self.player.request_skip()
    }

    /// Avance la lecture et place la caméra. Renvoie les événements que la cinématique renvoie au
    /// jeu (changer d'époque, fin…), déjà renvoyés au lecteur lui-même pour qu'un événement puisse
    /// en déclencher une autre.
    pub fn update(&mut self, dt: f32, camera: &mut OrbitCamera, generator: &herocraft_world::PlanetWorldGenerator) -> Vec<GameEvent> {
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
            if let Some(dir) = &self.world {
                if let Err(e) = self.player.save_seen(&dir.join(SEEN_FILE)) {
                    log::warn!("Cinématiques vues non enregistrées : {e}");
                }
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
        let origin = at_event.unwrap_or(camera.player_pos);
        let up = origin.normalize_or_zero();
        // Le regard du joueur, ramené dans le plan du sol de l'origine
        let forward = (forward - up * forward.dot(up)).normalize_or(forward);
        let right = forward.cross(up).normalize_or(right);
        let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
        AnchorFrame { origin, right, up, forward, block }
    }

    /// Ce que l'interface doit peindre par-dessus le jeu, `skip_key` étant le nom de la touche
    pub fn overlay(&self) -> Option<Overlay> {
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
pub struct OverlayCaption {
    pub text: String,
    pub speaker: Option<String>,
    pub style: CaptionStyle,
    pub alpha: f32,
}

/// Ce que peint l'interface pendant une cinématique
#[derive(Debug, Clone)]
pub struct Overlay {
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
pub fn draw(ctx: &egui::Context, overlay: &Overlay) {
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("cinematic")));
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
            let _ = ctx.run(egui::RawInput::default(), |ctx| draw(ctx, &overlay));
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
                    let output = ctx.run(input, |ctx| draw(ctx, &overlay));
                    snapshot.apply_textures(&output.textures_delta);
                    last = Some(output);
                }
                let output = last.unwrap();
                let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                snapshot.paint(&primitives, output.pixels_per_point);
                snapshot.save(&dir.join(format!("{id}-{}.png", i + 1)));
            }
        }
    }
}

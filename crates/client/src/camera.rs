use std::sync::Arc;

use glam::{DVec3, Mat4, Vec3};
use herocraft_common::BlockId;
pub use herocraft_common::tangent_frame;
use herocraft_world::generator::PlanetWorldGenerator;
use herocraft_world::TerrainEdits;

use crate::collision::VoxelCollider;

/// Demi-largeur du joueur, en blocs (0.6 bloc de large)
pub const PLAYER_HALF_WIDTH: f32 = 0.3;
/// Hauteur du corps testée contre les murs et plafonds, en blocs (le joueur mesure 1.8 bloc)
pub const PLAYER_HEIGHT: f32 = 1.75;
/// Tolérance de contact avec le sol, en blocs : aucune marche n'est montée sans sauter
const GROUND_TOLERANCE: f32 = 0.05;
/// Marche que le joueur au sol monte sans sauter (une dalle), en blocs
const STEP_UP: f32 = 0.55;
/// Hauteur d'un saut sur place, en blocs : de quoi franchir une marche d'un bloc, pas deux
const JUMP_HEIGHT: f32 = 1.3;
/// Gravité, en blocs par seconde au carré : un saut reste en l'air ~0,65 s
const GRAVITY: f32 = 25.0;
/// Vitesse de chute maximale, en blocs par seconde (atteinte après ~30 blocs de chute)
const TERMINAL_FALL_SPEED: f32 = 40.0;
/// Coup reçu : la vue penche de côté puis se redresse, en tant de secondes
const HURT_SHAKE_SECS: f32 = 0.4;
/// Inclinaison de la vue au plus fort d'une blessure maximale, en radians (~9°)
const HURT_ROLL: f32 = 0.16;
/// Mort : la vue s'effondre au sol en tant de secondes, couchée sur le côté
const DEATH_FALL_SECS: f32 = 0.7;
const DEATH_ROLL: f32 = 1.25;
/// Accroupi : les yeux descendent de cette fraction de leur hauteur, en tant de secondes
const CROUCH_EYE_DROP: f32 = 0.2;
const CROUCH_SECS: f32 = 0.12;

/// Mode de vue actif selon le continuum de distance de zoom
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraViewMode {
    /// Vue à la Première Personne (les yeux à hauteur des blocs, 1.7 blocs au-dessus du sol)
    FirstPerson,
    /// Vue à la Troisième Personne (caméra reculée derrière le joueur)
    ThirdPerson,
    /// Vue Satellite / Orbite Planétaire (vue d'ensemble de la planète entière)
    SatelliteOrbit,
}

/// Caméra planétaire multi-échelle unifiée :
/// Passe sans couture de la Première Personne (FPS au sol) à la Troisième Personne (TPS)
/// puis à l'Orbite Planétaire globale (Satellite) par simple molette de souris.
/// Équipée d'un bras de collision dynamique (Spring-Arm) empêchant toute pénétration sous le relief.
pub struct OrbitCamera {
    /// Position 3D du joueur sur la sphère
    pub player_pos: Vec3,
    /// Azimut horizontal du regard (en radians sur le plan tangentiel local)
    pub player_yaw: f32,
    /// Inclinaison verticale du regard (tangage en radians, -85° à +85°)
    pub player_pitch: f32,
    /// Vitesse verticale pour la gravité et le saut
    pub vertical_velocity: f32,
    pub is_grounded: bool,

    /// Entrées de déplacement au sol (ZQSD / WASD)
    pub move_forward: bool,
    pub move_backward: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub is_sprinting: bool,
    /// Endurance épuisée : le sprint au sol est impossible
    pub sprint_exhausted: bool,
    pub jump_requested: bool,

    /// Vol libre (double appui sur Saut) : pas de gravité, Saut monte et Descendre descend
    pub is_flying: bool,
    /// Multiplicateur de la vitesse de déplacement (commande `/speed`)
    pub speed_multiplier: f32,
    pub ascend_held: bool,
    pub descend_held: bool,
    /// Accroupissement affiché, de 0 (debout) à 1 (accroupi) : les yeux descendent en douceur
    crouch_blend: f32,

    /// Distance actuelle de la caméra (interpolée avec fluidité vers target_distance)
    pub distance: f32,
    /// Distance cible souhaitée de la caméra (définie par la molette, touches ou boutons)
    pub target_distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,

    /// Distance effective après détection de collision avec le relief
    pub effective_distance: f32,
    /// Position 3D effective de l'œil de la caméra (garantie strictement au-dessus du sol)
    pub effective_eye: Vec3,

    /// Angles d'orientation pour la vue satellite orbitale
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,

    /// Paramètres de projection
    #[allow(dead_code)]
    pub fov_y: f32,
    pub aspect: f32,
    #[allow(dead_code)]
    pub z_near: f32,
    pub z_far: f32,

    /// Sensibilités
    pub rotate_sensitivity: f32,
    pub zoom_sensitivity: f32,

    /// Auto-rotation pour le mode vitrine en orbite
    pub auto_rotate: bool,
    pub auto_rotate_speed: f32,

    /// Rayon de base de la planète
    pub base_radius: f32,

    /// Collisions par blocs avec le terrain modifiable (sans lui : relief lisse du générateur)
    pub collider: Option<VoxelCollider>,

    /// Rayon le plus haut atteint depuis que le joueur a quitté le sol (hors vol)
    fall_peak: Option<f32>,
    /// Hauteur en blocs de la dernière chute terminée, en attente de `take_landing`
    landed_fall: Option<f32>,

    /// Secondes restantes de la secousse de la dernière blessure, et sa force (0 à 1)
    hurt_shake: f32,
    hurt_strength: f32,
    /// Joueur mort : plus aucune entrée de déplacement, la vue s'effondre au sol
    pub is_dead: bool,
    /// Avancement de cet effondrement, de 0 (debout) à 1 (couché)
    death_fall: f32,
}

/// Rayon de base de la planète par défaut
pub const DEFAULT_RADIUS: f32 = (herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32;

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            player_pos: Vec3::new(0.0, 52.0, 0.0),
            player_yaw: 0.0,
            player_pitch: 0.0,
            vertical_velocity: 0.0,
            is_grounded: true,

            move_forward: false,
            move_backward: false,
            move_left: false,
            move_right: false,
            is_sprinting: false,
            sprint_exhausted: false,
            jump_requested: false,

            is_flying: false,
            speed_multiplier: 1.0,
            ascend_held: false,
            descend_held: false,
            crouch_blend: 0.0,

            distance: 330.0,
            target_distance: 330.0,
            min_distance: 0.0,
            max_distance: 900.0,
            effective_distance: 330.0,
            effective_eye: Vec3::new(0.0, 330.0, 0.0),

            orbit_yaw: 0.5,
            orbit_pitch: 0.35,

            fov_y: 22.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
            z_near: 0.05,
            z_far: 5000.0,

            rotate_sensitivity: 0.004,
            zoom_sensitivity: 0.12,

            auto_rotate: false,
            auto_rotate_speed: 0.2,

            base_radius: DEFAULT_RADIUS,

            collider: None,

            fall_peak: None,
            landed_fall: None,

            hurt_shake: 0.0,
            hurt_strength: 0.0,
            is_dead: false,
            death_fall: 0.0,
        }
    }
}

impl OrbitCamera {
    pub fn new(distance: f32, base_radius: f32, generator: &PlanetWorldGenerator) -> Self {
        // Recul maximal de l'orbite et portée de la vue : à l'échelle de la planète entière
        let scale = base_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;
        let mut cam = Self {
            distance,
            target_distance: distance,
            effective_distance: distance,
            base_radius,
            max_distance: 450.0 * scale,
            z_far: 2500.0 * scale,
            ..Default::default()
        };
        cam.spawn_on_ground(generator);
        cam.distance = distance;
        cam.target_distance = distance;
        cam.update_camera_bounds(generator);
        cam
    }

    /// Active les collisions par blocs avec ce terrain (à refaire à chaque nouvelle planète)
    pub fn set_terrain(&mut self, edits: Arc<TerrainEdits>) {
        self.collider = Some(VoxelCollider::new(edits, self.base_radius));
    }

    /// Place le joueur directement sur la terre ferme au-dessus de la mer
    pub fn spawn_on_ground(&mut self, generator: &PlanetWorldGenerator) {
        herocraft_common::prof_scope!("camera::spawn_on_ground");
        let candidates = [
            Vec3::new(1.0, 0.2, 0.2).normalize(),
            Vec3::new(-1.0, 0.3, 0.1).normalize(),
            Vec3::new(0.3, 0.5, 0.8).normalize(),
            Vec3::new(0.2, -0.4, -0.9).normalize(),
            Vec3::new(0.8, -0.3, 0.5).normalize(),
            Vec3::new(0.0, 0.0, 1.0),
        ];

        let mut spawn_dir = candidates[0];
        let mut spawn_ground_r = self.base_radius + 5.0;

        for &dir in &candidates {
            let dir_dvec = DVec3::new(dir.x as f64, dir.y as f64, dir.z as f64);
            let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir_dvec);
            let elev = generator.sample_elevation(dir_dvec, lat, lon);
            let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
            if elev > sea + 2.0 {
                spawn_ground_r = self.ground_radius_at(dir, generator);
                spawn_dir = dir;
                break;
            }
        }

        self.player_pos = spawn_dir * spawn_ground_r;
        let visit_target = self.visit_settlement(generator);
        self.vertical_velocity = 0.0;
        self.is_grounded = true;
        self.is_flying = false;
        self.cancel_fall();
        // (Ré)apparition : debout, la vue droite
        self.is_dead = false;
        self.death_fall = 0.0;
        self.hurt_shake = 0.0;
        self.sync_orbit_to_ground();
        self.distance = 0.0;
        self.target_distance = 0.0;
        if let Some(target) = visit_target {
            // Le regard tourné vers le centre de l'établissement, un peu plongeant
            let (up, east, north) = self.local_frame();
            let toward = (target - up * target.dot(up)).normalize_or_zero();
            self.player_yaw = toward.dot(east).atan2(toward.dot(north));
            self.player_pitch = -0.25;
        }
        self.update_camera_bounds(generator);
    }

    /// `--visiter ville|village|ruines` : place le joueur au bord du premier établissement de ce
    /// genre que porte la planète. Renvoie la direction de son centre.
    fn visit_settlement(&mut self, generator: &PlanetWorldGenerator) -> Option<Vec3> {
        use herocraft_world::structures::CELLS_PER_FACE;
        let args: Vec<String> = std::env::args().collect();
        let wanted = args.iter().position(|a| a == "--visiter").and_then(|i| args.get(i + 1))?;
        let rank: usize = args.iter().position(|a| a == "--visiter").and_then(|i| args.get(i + 2)).and_then(|n| n.parse().ok()).unwrap_or(1);
        let mut found = herocraft_common::PlanetFace::ALL.into_iter().flat_map(|face| {
            (0..CELLS_PER_FACE * CELLS_PER_FACE).filter_map(move |i| generator.structures.cell(generator, face, i % CELLS_PER_FACE, i / CELLS_PER_FACE))
        });
        let Some(settlement) = found.find(|s| s.kind.name() == wanted.as_str()).and_then(|first| {
            if rank <= 1 { Some(first) } else { found.filter(|s| s.kind == first.kind).nth(rank - 2) }
        }) else {
            log::warn!("--visiter {wanted} : aucun établissement de ce genre sur cette planète (ville, village ou ruines).");
            return None;
        };
        let face = settlement.face;
        let to_dir = |x: i32, z: i32| {
            let span = 2.0 / herocraft_world::FACE_BLOCKS as f64;
            let d = face.uv_to_sphere_direction(-1.0 + (x as f64 + 0.5) * span, -1.0 + (z as f64 + 0.5) * span);
            Vec3::new(d.x as f32, d.y as f32, d.z as f32)
        };
        let (cx, cz) = settlement.center;
        let dir = to_dir(cx, settlement.bounds.z0 - 3).normalize();
        self.player_pos = dir * self.ground_radius_at(dir, generator);
        log::info!(
            "Visite : {} n° {rank} sur la face {:?}, centre en ({cx}, {cz}), {} pièces.",
            settlement.kind.name(),
            face,
            settlement.pieces.len()
        );
        Some(to_dir(cx, cz))
    }

    /// Mode de caméra déduit du continuum de distance effective
    pub fn view_mode(&self) -> CameraViewMode {
        if self.effective_distance <= 0.05 {
            CameraViewMode::FirstPerson
        } else if self.effective_distance <= self.base_radius {
            CameraViewMode::ThirdPerson
        } else {
            CameraViewMode::SatelliteOrbit
        }
    }

    /// Calcule le repère orthonormé local (Up, East, North) à la position actuelle du joueur
    pub fn local_frame(&self) -> (Vec3, Vec3, Vec3) {
        let player_len = self.player_pos.length().max(1.0);
        let up = self.player_pos / player_len;
        let (east, north) = tangent_frame(up);
        (up, east, north)
    }

    /// Direction tangentielle avant, droite et direction 3D du regard (look_dir)
    pub fn look_vectors(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let (up, east, north) = self.local_frame();
        let forward_tangent = (north * self.player_yaw.cos() + east * self.player_yaw.sin()).normalize();
        let right_tangent = forward_tangent.cross(up).normalize();
        let look_dir = (forward_tangent * self.player_pitch.cos() + up * self.player_pitch.sin()).normalize();
        (up, forward_tangent, right_tangent, look_dir)
    }

    /// Direction orbitale unitaire de la caméra par rapport au centre planétaire
    pub fn orbit_direction(&self) -> Vec3 {
        Vec3::new(
            self.orbit_pitch.cos() * self.orbit_yaw.sin(),
            self.orbit_pitch.sin(),
            self.orbit_pitch.cos() * self.orbit_yaw.cos(),
        )
        .normalize()
    }

    /// Calcule le rayon minimum de sécurité du sol sous une direction donnée. Avec les collisions par
    /// blocs, une direction unitaire donne le dessus de toute la colonne, un point de l'espace le sol
    /// sous ce point (ou le dessus des blocs qui le contiennent).
    pub fn ground_radius_at(&self, dir: Vec3, generator: &PlanetWorldGenerator) -> f32 {
        herocraft_common::prof_scope!("camera::ground_radius_at");
        if let Some(collider) = &self.collider {
            return if dir.length() < self.base_radius * 0.5 {
                collider.column_top_radius(generator, dir)
            } else {
                collider.point_ground_radius(generator, dir)
            };
        }
        let dir_norm = dir.normalize();
        let dir_dvec = DVec3::new(dir_norm.x as f64, dir_norm.y as f64, dir_norm.z as f64);
        let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir_dvec);
        let elevation = generator.sample_elevation(dir_dvec, lat, lon);
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
        let eff_elev = elevation.max(sea);
        let altitude_scale = (self.base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f64)) as f32;
        self.base_radius + (eff_elev as f32) * altitude_scale
    }

    /// Oriente la caméra ou le regard selon le glisser de souris
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        match self.view_mode() {
            CameraViewMode::FirstPerson | CameraViewMode::ThirdPerson => {
                // Rotation de la tête / regard du joueur
                self.player_yaw += dx * self.rotate_sensitivity * 1.3;
                self.player_pitch -= dy * self.rotate_sensitivity * 1.3;
                let limit = 1.48; // ~85 degrés (haut/bas)
                self.player_pitch = self.player_pitch.clamp(-limit, limit);
                self.sync_orbit_to_ground();
            }
            CameraViewMode::SatelliteOrbit => {
                // Rotation orbitale autour du globe
                let dist_factor = (self.distance / (self.base_radius * 6.6)).clamp(0.20, 1.0);
                self.orbit_yaw += dx * self.rotate_sensitivity * dist_factor;
                self.orbit_pitch -= dy * self.rotate_sensitivity * dist_factor;
                let limit = 1.52; // ~87 degrés
                self.orbit_pitch = self.orbit_pitch.clamp(-limit, limit);
            }
        }
    }

    /// Oriente le regard du joueur (souris capturée). `yaw` et `pitch` sont en radians ;
    /// un pitch positif lève le regard.
    pub fn look(&mut self, yaw: f32, pitch: f32) {
        self.player_yaw += yaw;
        self.player_pitch = (self.player_pitch + pitch).clamp(-1.48, 1.48); // ~85 degrés
        self.sync_orbit_to_ground();
    }

    /// Relâche toutes les entrées de déplacement maintenues (menu ouvert, perte du focus)
    pub fn release_inputs(&mut self) {
        self.move_forward = false;
        self.move_backward = false;
        self.move_left = false;
        self.move_right = false;
        self.is_sprinting = false;
        self.ascend_held = false;
        self.descend_held = false;
    }

    /// Décolle ou cesse de voler
    pub fn toggle_flight(&mut self) {
        self.is_flying = !self.is_flying;
        self.vertical_velocity = 0.0;
        self.jump_requested = false;
        // Une chute se compte depuis l'arrêt du vol, pas depuis le saut qui l'a précédé
        self.fall_peak = None;
        if self.is_flying {
            self.is_grounded = false;
        }
    }

    /// Hauteur d'un bloc dans le repère du monde
    fn block_height(&self) -> f32 {
        match &self.collider {
            Some(collider) => collider.block_height(),
            None => herocraft_world::BlockGeometry::new(self.base_radius as f64).block_height as f32,
        }
    }

    /// Blessure de force `strength` (0 à 1) : la vue penche brièvement
    pub fn hurt(&mut self, strength: f32) {
        self.hurt_shake = HURT_SHAKE_SECS;
        self.hurt_strength = strength.clamp(0.0, 1.0);
    }

    /// Inclinaison de la vue autour du regard : secousse d'une blessure, joueur mort couché
    fn view_roll(&self) -> f32 {
        let t = 1.0 - self.hurt_shake / HURT_SHAKE_SECS;
        let shake = if self.hurt_shake > 0.0 { HURT_ROLL * self.hurt_strength * (std::f32::consts::PI * t).sin() } else { 0.0 };
        let fall = self.death_fall * self.death_fall * (3.0 - 2.0 * self.death_fall);
        shake + DEATH_ROLL * fall
    }

    /// Oublie la chute en cours (téléportation, apparition)
    pub fn cancel_fall(&mut self) {
        self.fall_peak = None;
        self.landed_fall = None;
    }

    /// Hauteur en blocs de la chute qui vient de se terminer au sol, une seule fois
    pub fn take_landing(&mut self) -> Option<f32> {
        self.landed_fall.take()
    }

    /// Le joueur est accroupi : Descendre maintenu, au sol. Il marche lentement, ne sprinte pas, et
    /// les bêtes craintives le remarquent de moins loin.
    pub fn is_crouching(&self) -> bool {
        self.descend_held && !self.is_flying && !self.is_dead && self.view_mode() != CameraViewMode::SatelliteOrbit
    }

    /// Hauteur des yeux au-dessus des pieds, en unités du monde : ~1,7 bloc debout, ~1,35 accroupi
    fn eye_height(&self) -> f32 {
        let scale = herocraft_world::block_scale(self.base_radius as f64) as f32;
        0.065 * scale * (1.0 - CROUCH_EYE_DROP * self.crouch_blend)
    }

    /// Le joueur sprinte à pied (ce qui consomme son endurance)
    pub fn is_running(&self) -> bool {
        self.is_sprinting
            && !self.is_crouching()
            && !self.sprint_exhausted
            && !self.is_flying
            && (self.move_forward || self.move_backward || self.move_left || self.move_right)
            && self.view_mode() != CameraViewMode::SatelliteOrbit
    }

    /// Synchronise la position au sol du joueur avec la vue orbitale satellite
    pub fn sync_ground_to_orbit(&mut self, generator: &PlanetWorldGenerator) {
        let dir = self.orbit_direction();
        let ground_radius = self.ground_radius_at(dir, generator);

        self.player_pos = dir * ground_radius;
        self.vertical_velocity = 0.0;
        self.is_grounded = true;
        self.is_flying = false;
        self.cancel_fall();
        self.update_camera_bounds(generator);
    }

    /// Synchronise les angles orbitaux avec la position du joueur au sol
    pub fn sync_orbit_to_ground(&mut self) {
        let len = self.player_pos.length();
        if len > 0.001 {
            let up = self.player_pos / len;
            self.orbit_pitch = up.y.clamp(-0.99, 0.99).asin();
            self.orbit_yaw = up.x.atan2(up.z);
        }
    }

    /// Téléporte le joueur aux pieds `feet` (vue au sol) : en vol s'il n'est pas posé au sol, pour
    /// qu'il ne fasse pas de chute
    pub fn teleport(&mut self, feet: Vec3, generator: &PlanetWorldGenerator) {
        let ground = self.ground_radius_at(feet.normalize_or_zero(), generator);
        let block = self.block_height();
        self.player_pos = feet;
        self.vertical_velocity = 0.0;
        self.jump_requested = false;
        self.cancel_fall();
        self.is_flying = feet.length() > ground + 0.5 * block;
        self.is_grounded = !self.is_flying;
        self.sync_orbit_to_ground();
        self.distance = 0.0;
        self.target_distance = 0.0;
        self.update_camera_bounds(generator);
    }

    /// Pieds posés au sol à cette latitude et longitude (radians), `height_blocks` blocs au-dessus
    pub fn ground_point(&self, lat: f32, lon: f32, height_blocks: f32, generator: &PlanetWorldGenerator) -> Vec3 {
        let dir = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        dir * (self.ground_radius_at(dir, generator) + height_blocks * self.block_height())
    }

    /// Zoom continu via la molette : passe sans couture de Satellite à TPS puis à FPS
    pub fn zoom(&mut self, delta_y: f32) {
        if self.target_distance <= 0.05 && delta_y < 0.0 {
            // Sortie du mode FPS vers TPS
            self.target_distance = 1.2;
            return;
        }

        if self.target_distance <= 2.5 && delta_y > 0.0 {
            // Rentrée douce dans le mode FPS
            self.target_distance = (self.target_distance - delta_y * 0.40).max(0.0);
            return;
        }

        let factor = 1.0 - delta_y * self.zoom_sensitivity;
        self.target_distance = (self.target_distance * factor).clamp(self.min_distance, self.max_distance);

        if self.target_distance < 0.08 {
            self.target_distance = 0.0;
        }
    }

    /// Déclenche une transition fluide vers la vue satellite orbitale
    pub fn transition_to_orbit(&mut self) {
        self.sync_orbit_to_ground();
        self.target_distance = self.base_radius * 6.6;
    }

    /// Déclenche une transition fluide vers la vue au sol à la première personne (FPS)
    pub fn transition_to_ground(&mut self, generator: &PlanetWorldGenerator) {
        self.sync_ground_to_orbit(generator);
        self.target_distance = 0.0;
    }

    /// Bascule de manière fluide entre la vue au sol et la vue satellite orbitale
    pub fn toggle_view(&mut self, generator: &PlanetWorldGenerator) {
        if self.target_distance <= self.base_radius {
            self.transition_to_orbit();
        } else {
            self.transition_to_ground(generator);
        }
    }

    /// Calcule la position de la caméra sans aucune pénétration sous le relief (Spring-Arm anti-collision)
    pub fn update_camera_bounds(&mut self, generator: &PlanetWorldGenerator) {
        herocraft_common::prof_scope!("camera::update_camera_bounds");
        let scale = herocraft_world::block_scale(self.base_radius as f64) as f32;
        let (up, _, _, look_dir) = self.look_vectors();
        let eye_height = self.eye_height();
        let head = self.player_pos + up * eye_height;

        let margin = 0.035 * scale; // Marge de sécurité stricte au-dessus du sol (~0.9 bloc)

        if self.distance <= 0.05 {
            // Mode Première Personne (FPS) : positionné aux yeux de l'avatar
            self.effective_distance = 0.0;
            self.effective_eye = head;
        } else if self.distance <= self.base_radius {
            // Mode Troisième Personne (TPS) : Spring-Arm optimisé avec test d'extrémité rapide + dichotomie
            let max_d = self.distance;
            let mut safe_d = max_d;

            // 1. Test direct de la position finale souhaitée
            let full_pos = head - look_dir * max_d;
            let full_r = full_pos.length();
            let full_ground_r = self.ground_radius_at(full_pos, generator);

            if full_r < full_ground_r + margin {
                // Collision détectée à l'extrémité : recherche dichotomique rapide en 4 itérations
                let mut low = 0.0_f32;
                let mut high = max_d;
                for _ in 0..4 {
                    let mid = (low + high) * 0.5;
                    let mid_pos = head - look_dir * mid;
                    let mid_r = mid_pos.length();
                    let mid_ground_r = self.ground_radius_at(mid_pos, generator);
                    if mid_r < mid_ground_r + margin {
                        high = mid;
                    } else {
                        low = mid;
                    }
                }
                safe_d = (low - margin * 0.5).max(0.0);
            }

            self.effective_distance = safe_d;
            if self.effective_distance <= 0.05 {
                // Adossé à un mur ou relief : bascule fluide et transparente en FPS
                self.effective_eye = head;
            } else {
                let mut eye = head - look_dir * self.effective_distance;
                // Double sécurité : clamp strict au-dessus du relief sous l'œil
                let eye_ground_r = self.ground_radius_at(eye, generator);
                let eye_len = eye.length();
                if eye_len < eye_ground_r + margin {
                    eye = eye.normalize() * (eye_ground_r + margin);
                }
                self.effective_eye = eye;
            }
        } else {
            // Mode Satellite / Orbite globale
            let t_orbit = ((self.distance - self.base_radius) / (self.base_radius * 1.8)).clamp(0.0, 1.0);
            let target = head.lerp(Vec3::ZERO, t_orbit);

            let orbit_dir = self.orbit_direction();
            let orbit_eye = target + orbit_dir * self.distance;
            let tps_eye = head - look_dir * self.distance;

            let mut eye = tps_eye.lerp(orbit_eye, t_orbit);

            // Garantie d'altitude au-dessus du relief uniquement à basse altitude orbitale
            let eye_len = eye.length();
            if eye_len < self.base_radius + 0.80 * scale {
                let ground_r = self.ground_radius_at(eye, generator);
                if eye_len < ground_r + margin {
                    eye = eye.normalize() * (ground_r + margin);
                }
            }

            self.effective_distance = self.distance;
            self.effective_eye = eye;
        }
    }

    /// Mise à jour physique du joueur et de la caméra (déplacement ZQSD, gravité sphérique, détection de relief)
    pub fn update(&mut self, dt: f32, generator: &PlanetWorldGenerator) {
        herocraft_common::prof_scope!("camera::update");
        // 0. Transition fluide et sans à-coups de la distance de zoom (sans téléportation)
        if self.distance > self.base_radius && self.target_distance <= self.base_radius {
            self.sync_ground_to_orbit(generator);
        }

        let diff = self.target_distance - self.distance;
        if diff.abs() > 0.001 {
            let speed = if diff.abs() > 10.0 { 6.0 } else { 9.0 };
            let factor = 1.0 - (-speed * dt).exp();
            let step = diff * factor;
            let min_step = 1.5 * dt * diff.signum();
            if step.abs() < min_step.abs() && diff.abs() > 0.02 {
                self.distance += min_step;
            } else {
                self.distance += step;
            }
            if (self.distance - self.target_distance).abs() < 0.01 {
                self.distance = self.target_distance;
            }
        }

        if self.auto_rotate && self.view_mode() == CameraViewMode::SatelliteOrbit {
            self.orbit_yaw += self.auto_rotate_speed * dt;
        }

        self.hurt_shake = (self.hurt_shake - dt).max(0.0);
        let crouch_target = if self.is_crouching() { 1.0 } else { 0.0 };
        self.crouch_blend += (crouch_target - self.crouch_blend).clamp(-dt / CROUCH_SECS, dt / CROUCH_SECS);
        if self.is_dead {
            // Mort : plus de déplacement ni de vol, la vue s'effondre
            self.release_inputs();
            self.jump_requested = false;
            self.is_flying = false;
            self.death_fall = (self.death_fall + dt / DEATH_FALL_SECS).min(1.0);
        } else {
            self.death_fall = 0.0;
        }

        let (_up, forward_tangent, right_tangent, _) = self.look_vectors();
        let scale = herocraft_world::block_scale(self.base_radius as f64) as f32;

        // 1. Déplacement horizontal ZQSD / WASD
        let mut move_dir = Vec3::ZERO;
        if self.move_forward { move_dir += forward_tangent; }
        if self.move_backward { move_dir -= forward_tangent; }
        if self.move_left { move_dir -= right_tangent; }
        if self.move_right { move_dir += right_tangent; }

        if move_dir.length_squared() > 0.001 {
            move_dir = move_dir.normalize();
            // Vitesse adaptée à l'échelle : ~5.2 blocs/s en marche, ~11.7 blocs/s en sprint,
            // ~13 blocs/s en vol et ~29 blocs/s en vol sprinté
            let sprinting = self.is_sprinting && (self.is_flying || !self.sprint_exhausted);
            let speed = match (self.is_flying, sprinting) {
                // Accroupi : ~2 blocs/s, sans sprint
                (false, _) if self.is_crouching() => 0.08,
                (false, false) => 0.20,
                (false, true) => 0.45,
                (true, false) => 0.50,
                (true, true) => 1.10,
            } * scale * self.speed_multiplier;
            let delta = move_dir * speed * dt;
            match &self.collider {
                // Les murs arrêtent le joueur, qui glisse le long d'eux (axe est puis axe nord)
                Some(collider) => {
                    let (_, east, north) = self.local_frame();
                    let start = self.player_pos;
                    let step = if self.is_grounded && !self.is_flying { STEP_UP } else { GROUND_TOLERANCE };
                    let trapped = body_blocked(collider, generator, start, step);
                    let moved = [delta, east * delta.dot(east), north * delta.dot(north)].into_iter().find_map(|d| {
                        let candidate = (start + d).normalize() * start.length();
                        (trapped || !body_blocked(collider, generator, candidate, step)).then_some(candidate)
                    });
                    if let Some(pos) = moved {
                        self.player_pos = pos;
                    }
                }
                None => self.player_pos += delta,
            }
        }

        // 2. Gravité sphérique et hauteur du relief sous les pieds
        let new_up = self.player_pos.normalize();
        let ground_radius = match &self.collider {
            // Dessus des blocs sous l'empreinte du joueur, jamais plus haut que ses pieds
            // Au sol, le joueur monte de lui-même une demi-marche (dalle, escalier)
            Some(collider) => {
                let step = if self.is_grounded && !self.is_flying { STEP_UP } else { GROUND_TOLERANCE };
                footprint_ground(collider, generator, self.player_pos, step)
            }
            None => self.ground_radius_at(new_up, generator),
        };

        let previous_radius = self.player_pos.length();
        let mut current_radius = previous_radius;

        if self.is_flying {
            // Vol : vitesse verticale pilotée directement (~10 blocs/s), sans gravité
            let mut climb = 0.0;
            if self.ascend_held { climb += 0.40 * scale; }
            if self.descend_held { climb -= 0.40 * scale; }
            self.vertical_velocity = climb;
            self.jump_requested = false;
            current_radius += climb * dt;
            if current_radius <= ground_radius {
                current_radius = ground_radius;
                self.vertical_velocity = 0.0;
                self.is_grounded = true;
                // Se poser en descendant met fin au vol
                if climb < 0.0 {
                    self.is_flying = false;
                }
            } else {
                self.is_grounded = false;
            }
        } else {
            let block = self.block_height();
            // Saut : vitesse qui monte d'exactement `JUMP_HEIGHT` sous `GRAVITY`
            if self.jump_requested && self.is_grounded {
                self.vertical_velocity = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt() * block;
                self.is_grounded = false;
                self.jump_requested = false;
            }

            if self.is_grounded && current_radius <= ground_radius + 0.05 * scale {
                // Au sol : suit le relief et descend directement les marches (< 1.5 bloc)
                current_radius = ground_radius;
                self.vertical_velocity = 0.0;
            } else {
                // En l'air : gravité sphérique jusqu'au contact du relief, freinée par l'air
                self.vertical_velocity = (self.vertical_velocity - GRAVITY * block * dt).max(-TERMINAL_FALL_SPEED * block);
                current_radius += self.vertical_velocity * dt;
                if current_radius <= ground_radius {
                    current_radius = ground_radius;
                    self.vertical_velocity = 0.0;
                    self.is_grounded = true;
                } else {
                    self.is_grounded = false;
                }
            }
        }

        // Plafond : la tête qui monte dans un bloc s'y arrête
        if current_radius > previous_radius {
            if let Some(collider) = &self.collider {
                let h = collider.block_height();
                let head = new_up * (current_radius + PLAYER_HEIGHT * h);
                if collider.any_solid(generator, &footprint(head, h)) {
                    // Arrêt contre le dessous du bloc touché par la tête
                    let ceiling_altitude = ((head.length() - self.base_radius) / h).floor();
                    let ceiling_radius = self.base_radius + ceiling_altitude * h;
                    current_radius = (ceiling_radius - (PLAYER_HEIGHT + 0.01) * h).clamp(previous_radius, current_radius);
                    self.vertical_velocity = 0.0;
                }
            }
        }

        self.player_pos = new_up * current_radius;

        // Chute : du point le plus haut atteint en l'air jusqu'au contact du sol
        if self.is_flying {
            self.fall_peak = None;
        } else if self.is_grounded {
            if let Some(peak) = self.fall_peak.take() {
                let blocks = (peak - current_radius) / self.block_height();
                if blocks > 0.0 {
                    self.landed_fall = Some(blocks);
                }
            }
        } else {
            let peak = self.fall_peak.unwrap_or(previous_radius);
            self.fall_peak = Some(peak.max(current_radius));
        }

        // En mode sol, garder l'orbite alignée pour un dézoom fluide
        if self.view_mode() != CameraViewMode::SatelliteOrbit {
            self.sync_orbit_to_ground();
        }

        // Calcul anti-collision garanti au-dessus du relief
        self.update_camera_bounds(generator);
    }

    /// Bloc qui porte le joueur (sa matière donne le son des pas), ou l'eau quand il y a les pieds
    pub fn surface_block(&self, generator: &PlanetWorldGenerator) -> Option<BlockId> {
        let collider = self.collider.as_ref()?;
        let h = collider.block_height();
        let up = self.player_pos.normalize();
        let feet = collider.block_at(generator, self.player_pos + up * 0.3 * h);
        if feet == BlockId::WATER {
            return Some(feet);
        }
        Some(collider.block_at(generator, self.player_pos - up * 0.3 * h))
    }

    /// Un bloc au-dessus de la tête bouche le ciel (grotte, abri)
    pub fn sky_covered(&self, generator: &PlanetWorldGenerator) -> bool {
        let Some(collider) = &self.collider else { return false };
        let head = self.player_pos.length() + PLAYER_HEIGHT * collider.block_height();
        collider.column_top_radius(generator, self.player_pos) > head
    }

    /// Position 3D de l'œil de la caméra dans l'espace monde (garantie strictement au-dessus du sol)
    pub fn eye_position(&self) -> Vec3 {
        self.effective_eye
    }

    /// Matrice View-Projection avec FOV adaptatif selon le mode (FPS immersif -> Satellite téléobjectif)
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let eye = self.effective_eye;
        let (up, _, _, look_dir) = self.look_vectors();
        let eye_height = self.eye_height();
        let head = self.player_pos + up * eye_height;

        // Secousse d'une blessure et joueur mort couché : la vue tourne autour du regard
        let rolled_up = glam::Quat::from_axis_angle(look_dir, self.view_roll()) * up;
        let mut eye = eye;
        let (target, cam_up, fov, z_near) = if self.effective_distance <= 0.05 {
            // FPS : regard orienté selon le pitch/yaw du joueur ; mort, les yeux tombent au ras du sol
            let fall = self.death_fall * self.death_fall * (3.0 - 2.0 * self.death_fall);
            eye -= up * eye_height * 0.8 * fall;
            (eye + look_dir, rolled_up, 70.0_f32.to_radians(), 0.01_f32)
        } else if self.effective_distance <= self.base_radius {
            // TPS : regard centré sur la tête du joueur
            let t_fov = (self.effective_distance / self.base_radius).clamp(0.0, 1.0);
            let fov = (70.0 * (1.0 - t_fov) + 38.0 * t_fov).to_radians();
            (head, rolled_up, fov, 0.05_f32)
        } else {
            // Satellite / Orbite : transition douce de la cible vers le centre du monde (0,0,0)
            let t_orbit = ((self.effective_distance - self.base_radius) / (self.base_radius * 1.8)).clamp(0.0, 1.0);
            let target = head.lerp(Vec3::ZERO, t_orbit);
            let cam_up = up.lerp(Vec3::Y, t_orbit).normalize();
            let fov = (38.0 * (1.0 - t_orbit) + 22.0 * t_orbit).to_radians();
            (target, cam_up, fov, 0.1_f32)
        };

        let view = Mat4::look_at_rh(eye, target, cam_up);
        let proj = Mat4::perspective_rh(fov, self.aspect, z_near, self.z_far);
        proj * view
    }

    /// Réinitialise la caméra en vue satellite avec transition fluide
    pub fn reset(&mut self) {
        self.target_distance = self.base_radius * 6.6;
        self.orbit_yaw = 0.5;
        self.orbit_pitch = 0.35;
    }

    /// Rayon de visée du centre de l'écran (réticule) en jeu : origine à l'œil, direction du regard,
    /// et distance de l'œil à la tête du joueur (non nulle en troisième personne). `None` en vue
    /// satellite, où rien n'est visé.
    pub fn view_ray(&self) -> Option<(Vec3, Vec3, f32)> {
        if self.view_mode() == CameraViewMode::SatelliteOrbit {
            return None;
        }
        let (up, _, _, look_dir) = self.look_vectors();
        let scale = herocraft_world::block_scale(self.base_radius as f64) as f32;
        let head = self.player_pos + up * 0.065 * scale;
        Some((self.effective_eye, look_dir, (self.effective_eye - head).length()))
    }
}

/// Le centre et les 4 coins de l'empreinte carrée du joueur autour du point `p`
pub fn footprint(p: Vec3, block_height: f32) -> [Vec3; 5] {
    let (east, north) = tangent_frame(p.normalize());
    let (e, n) = (east * PLAYER_HALF_WIDTH * block_height, north * PLAYER_HALF_WIDTH * block_height);
    [p, p + e + n, p + e - n, p - e + n, p - e - n]
}

/// Rayon du sol sous l'empreinte du joueur : le plus haut des sols de ses coins, au plus `step`
/// blocs au-dessus des pieds
fn footprint_ground(collider: &VoxelCollider, generator: &PlanetWorldGenerator, feet: Vec3, step: f32) -> f32 {
    footprint(feet, collider.block_height())
        .iter()
        .map(|&p| collider.ground_radius(generator, p, step))
        .fold(f32::MIN, f32::max)
}

/// Le corps du joueur, posé sur le sol qu'il trouverait aux pieds `feet`, entre dans un bloc solide
fn body_blocked(collider: &VoxelCollider, generator: &PlanetWorldGenerator, feet: Vec3, step: f32) -> bool {
    let h = collider.block_height();
    let up = feet.normalize();
    let base = feet.length().max(footprint_ground(collider, generator, feet, step));
    let points: Vec<Vec3> = [GROUND_TOLERANCE, 0.6, 1.2, PLAYER_HEIGHT]
        .iter()
        .flat_map(|&height| footprint(up * (base + height * h), h))
        .collect();
    collider.any_solid(generator, &points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_mode_thresholds() {
        let mut cam = OrbitCamera::default();
        cam.effective_distance = 0.0;
        assert_eq!(cam.view_mode(), CameraViewMode::FirstPerson);

        cam.effective_distance = 0.04;
        assert_eq!(cam.view_mode(), CameraViewMode::FirstPerson);

        cam.effective_distance = 0.1;
        assert_eq!(cam.view_mode(), CameraViewMode::ThirdPerson);

        cam.effective_distance = cam.base_radius;
        assert_eq!(cam.view_mode(), CameraViewMode::ThirdPerson);

        cam.effective_distance = cam.base_radius + 0.1;
        assert_eq!(cam.view_mode(), CameraViewMode::SatelliteOrbit);

        cam.effective_distance = 6.6 * cam.base_radius;
        assert_eq!(cam.view_mode(), CameraViewMode::SatelliteOrbit);
    }

    #[test]
    fn test_local_frame_orthonormality() {
        let cam = OrbitCamera::default();
        let (up, east, north) = cam.local_frame();

        assert!((up.length() - 1.0).abs() < 1e-4);
        assert!((east.length() - 1.0).abs() < 1e-4);
        assert!((north.length() - 1.0).abs() < 1e-4);

        assert!(up.dot(east).abs() < 1e-4);
        assert!(up.dot(north).abs() < 1e-4);
        assert!(east.dot(north).abs() < 1e-4);
    }

    #[test]
    fn test_matrices_finite_in_all_modes() {
        let mut cam = OrbitCamera::default();
        let scale = herocraft_world::block_scale(cam.base_radius as f64) as f32;
        let eye_height = 0.065 * scale;
        for &dist in &[0.0, 0.05, 1.0, 10.0, 25.0, 50.0, 165.0, 330.0] {
            cam.distance = dist;
            cam.effective_distance = dist;
            cam.effective_eye = if dist <= 0.05 {
                cam.player_pos + Vec3::Y * eye_height
            } else {
                cam.player_pos + Vec3::Y * eye_height + Vec3::Z * dist
            };
            let mat = cam.build_view_projection_matrix();
            for col in 0..4 {
                for row in 0..4 {
                    assert!(!mat.col(col)[row].is_nan(), "NaN at distance {}", dist);
                    assert!(!mat.col(col)[row].is_infinite(), "Inf at distance {}", dist);
                }
            }
        }
    }

    #[test]
    fn test_camera_never_penetrates_ground() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let gen = PlanetWorldGenerator::new(desc);
        let mut cam = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &gen);

        // Test à différentes distances (FPS, TPS court, TPS long, Transition, Satellite)
        // et sous différents angles de tangage (regard en l'air, vers le sol, etc.)
        for &dist in &[0.0, 0.5, 2.0, 5.0, 15.0, 25.0, 45.0, 165.0] {
            for &pitch in &[-1.4, -0.7, 0.0, 0.7, 1.4] {
                cam.distance = dist;
                cam.player_pitch = pitch;
                cam.update_camera_bounds(&gen);

                let ground_r = cam.ground_radius_at(cam.effective_eye, &gen);
                assert!(
                    cam.effective_eye.length() >= ground_r + 0.02,
                    "Camera went underground! Eye radius {}, ground radius {} at dist {}, pitch {}",
                    cam.effective_eye.length(), ground_r, dist, pitch
                );
            }
        }
    }

    #[test]
    fn test_camera_smooth_zoom_transition() {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let gen = PlanetWorldGenerator::new(desc);
        let mut cam = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &gen);

        // Au départ en orbite
        let start = 6.6 * crate::camera::DEFAULT_RADIUS;
        assert_eq!(cam.target_distance, start);
        assert_eq!(cam.distance, start);

        // Déclenchement d'une transition vers le sol (FPS)
        cam.transition_to_ground(&gen);
        assert_eq!(cam.target_distance, 0.0);
        assert_eq!(cam.distance, start); // La distance ne se téléporte pas !

        // Simulation de quelques frames
        let mut intermediate_distances = Vec::new();
        for _ in 0..60 {
            cam.update(0.016, &gen);
            intermediate_distances.push(cam.distance);
        }

        // Vérification de la continuité et de la progression fluide
        assert!(intermediate_distances[0] < start, "Camera should start descending");
        assert!(intermediate_distances[10] < intermediate_distances[0]);
        assert!(intermediate_distances[30] < intermediate_distances[10]);
        let final_d = intermediate_distances.last().copied().unwrap();
        assert!(final_d < 0.01 * start, "Camera should have approached ground: final_d={}", final_d);
    }

    fn ground_test_camera() -> (OrbitCamera, PlanetWorldGenerator) {
        let desc = herocraft_world::PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        };
        let gen = PlanetWorldGenerator::new(desc);
        let mut cam = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &gen);
        cam.spawn_on_ground(&gen);
        cam.update(0.016, &gen);
        (cam, gen)
    }

    #[test]
    fn test_jump_leaves_ground_and_lands() {
        let (mut cam, gen) = ground_test_camera();
        let ground = cam.player_pos.length();
        assert!(cam.is_grounded);

        cam.jump_requested = true;
        let mut peak = ground;
        let mut airborne = 0;
        for _ in 0..120 {
            cam.update(0.016, &gen);
            peak = peak.max(cam.player_pos.length());
            airborne += usize::from(!cam.is_grounded);
        }
        assert!(peak > ground + 0.01, "le saut doit décoller du sol (pic {} sol {})", peak, ground);
        assert!(cam.is_grounded, "le joueur doit être retombé");
        // Un saut franchit un bloc sans en franchir deux, et dure un peu plus d'une demi-seconde
        let height = (peak - ground) / cam.block_height();
        assert!((1.1..1.6).contains(&height), "saut de {height} blocs");
        let seconds = airborne as f32 * 0.016;
        assert!((0.5..0.85).contains(&seconds), "{seconds} s en l'air");
    }

    #[test]
    fn test_fall_is_measured_from_the_peak() {
        let (mut cam, gen) = ground_test_camera();
        let block = herocraft_world::BlockGeometry::new(cam.base_radius as f64).block_height as f32;
        // Un saut sur place ne blesse pas
        cam.jump_requested = true;
        for _ in 0..240 {
            cam.update(0.016, &gen);
        }
        let jump = cam.take_landing().expect("le saut retombe au sol");
        assert!(jump < herocraft_common::SAFE_FALL_BLOCKS, "saut de {jump} blocs");
        assert_eq!(cam.take_landing(), None);

        // Lâché 12 blocs au-dessus du sol : chute d'environ 12 blocs
        let up = cam.player_pos.normalize();
        cam.player_pos += up * 12.0 * block;
        cam.is_grounded = false;
        for _ in 0..240 {
            cam.update(0.016, &gen);
        }
        let fall = cam.take_landing().expect("la chute se termine au sol");
        assert!((fall - 12.0).abs() < 0.5, "chute de {fall} blocs");

        // En vol, se poser n'est pas une chute
        cam.toggle_flight();
        cam.ascend_held = true;
        for _ in 0..60 {
            cam.update(0.016, &gen);
        }
        cam.ascend_held = false;
        cam.descend_held = true;
        for _ in 0..600 {
            cam.update(0.016, &gen);
        }
        assert!(cam.is_grounded && !cam.is_flying);
        assert_eq!(cam.take_landing(), None);
    }

    #[test]
    fn test_crouching_slows_walking_and_lowers_the_eyes() {
        let (mut cam, gen) = ground_test_camera();
        let walk = |cam: &mut OrbitCamera| {
            let start = cam.player_pos;
            cam.move_forward = true;
            for _ in 0..30 {
                cam.update(0.016, &gen);
            }
            cam.move_forward = false;
            cam.player_pos.distance(start)
        };
        let standing_eye = cam.eye_height();
        let standing = walk(&mut cam);

        cam.descend_held = true;
        cam.is_sprinting = true;
        assert!(cam.is_crouching());
        let crouching = walk(&mut cam);
        assert!(crouching < standing * 0.5, "accroupi, il avance presque aussi vite ({crouching} contre {standing})");
        assert!(cam.eye_height() < standing_eye * 0.9, "les yeux ne descendent pas");
        assert!(!cam.is_running(), "accroupi, il ne sprinte pas");

        // En vol, Descendre descend au lieu de s'accroupir
        cam.descend_held = false;
        cam.toggle_flight();
        cam.descend_held = true;
        assert!(!cam.is_crouching());
    }

    #[test]
    fn test_flight_climbs_hovers_and_lands() {
        let (mut cam, gen) = ground_test_camera();
        let ground = cam.player_pos.length();

        cam.toggle_flight();
        assert!(cam.is_flying);

        // Saut maintenu : montée sans gravité
        cam.ascend_held = true;
        for _ in 0..30 {
            cam.update(0.016, &gen);
        }
        cam.ascend_held = false;
        let altitude = cam.player_pos.length();
        assert!(altitude > ground + 0.05, "le vol doit monter");

        // Rien de maintenu : le joueur plane
        for _ in 0..30 {
            cam.update(0.016, &gen);
        }
        assert!((cam.player_pos.length() - altitude).abs() < 1e-3, "le vol ne doit pas retomber");

        // Descendre jusqu'au sol met fin au vol
        cam.descend_held = true;
        for _ in 0..300 {
            cam.update(0.016, &gen);
        }
        assert!(!cam.is_flying, "se poser doit terminer le vol");
        assert!(cam.is_grounded);
    }

    /// Joueur sur une aire plate de 9×9 blocs taillée autour de ses pieds, collisions par blocs actives.
    /// Renvoie aussi l'altitude (en blocs) du dessus de l'aire.
    fn voxel_test_camera() -> (OrbitCamera, PlanetWorldGenerator, Arc<TerrainEdits>, i32) {
        let gen = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut cam = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &gen);
        let edits = Arc::new(TerrainEdits::new(None));
        cam.set_terrain(edits.clone());
        cam.spawn_on_ground(&gen);
        let geometry = herocraft_world::BlockGeometry::new(cam.base_radius as f64);
        // Au milieu de son bloc : le trou de 3 × 3 blocs creusé autour couvre toute son empreinte
        let (pos, frac) = geometry.locate(cam.player_pos.as_dvec3()).unwrap();
        cam.player_pos = geometry.corner(pos.face, pos.x as f64 + 0.5, pos.y as f64 + frac[1], pos.z as f64 + 0.5).as_vec3();
        let floor = ((cam.player_pos.length() as f64 - geometry.base_radius) / geometry.block_height).round() as i32;
        for (dx, dz) in (-4..=4).flat_map(|i| (-4..=4).map(move |j| (i as f32, j as f32))) {
            let column = cell_at(&cam, dx, dz, 0);
            for y in floor - 3..floor {
                edits.set_block(&gen, herocraft_world::BlockPos { y, ..column }, herocraft_common::BlockId::STONE);
            }
            for y in floor..floor + 6 {
                edits.set_block(&gen, herocraft_world::BlockPos { y, ..column }, herocraft_common::BlockId::AIR);
            }
        }
        for _ in 0..30 {
            cam.update(0.016, &gen);
        }
        (cam, gen, edits, floor)
    }

    /// Bloc décalé de (avant, droite) blocs des pieds du joueur, à l'altitude `y`
    fn cell_at(cam: &OrbitCamera, forward: f32, right: f32, y: i32) -> herocraft_world::BlockPos {
        let geometry = herocraft_world::BlockGeometry::new(cam.base_radius as f64);
        let (_, f, r, _) = cam.look_vectors();
        let h = geometry.block_height as f32;
        let p = cam.player_pos + (f * forward + r * right) * h;
        herocraft_world::BlockPos { y, ..geometry.block_at_point(p.as_dvec3()).unwrap() }
    }

    fn altitude_blocks(cam: &OrbitCamera) -> f32 {
        let h = herocraft_world::BlockGeometry::new(cam.base_radius as f64).block_height as f32;
        (cam.player_pos.length() - cam.base_radius) / h
    }

    fn fill(cam: &OrbitCamera, gen: &PlanetWorldGenerator, edits: &TerrainEdits, forwards: &[f32], ys: &[i32], block: herocraft_common::BlockId) {
        for &f in forwards {
            for r in [-1.0, 0.0, 1.0] {
                for &y in ys {
                    edits.set_block(gen, cell_at(cam, f, r, y), block);
                }
            }
        }
    }

    fn walk(cam: &mut OrbitCamera, gen: &PlanetWorldGenerator, seconds: f32) {
        cam.move_forward = true;
        for _ in 0..(seconds / 0.016) as usize {
            cam.update(0.016, gen);
        }
        cam.move_forward = false;
        for _ in 0..60 {
            cam.update(0.016, gen);
        }
    }

    #[test]
    fn test_voxel_player_stands_on_block_tops() {
        let (cam, _, _, floor) = voxel_test_camera();
        assert!(cam.is_grounded);
        assert!((altitude_blocks(&cam) - floor as f32).abs() < 1e-2, "pieds à {} au lieu de {}", altitude_blocks(&cam), floor);
    }

    #[test]
    fn test_voxel_player_falls_into_a_dug_hole() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        fill(&cam, &gen, &edits, &[-1.0, 0.0, 1.0], &[floor - 1, floor - 2], herocraft_common::BlockId::AIR);
        for _ in 0..60 {
            cam.update(0.016, &gen);
        }
        assert!(cam.is_grounded);
        assert!((altitude_blocks(&cam) - (floor - 2) as f32).abs() < 1e-2, "pieds à {}", altitude_blocks(&cam));
    }

    #[test]
    fn test_voxel_one_block_step_needs_a_jump() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        fill(&cam, &gen, &edits, &[1.5, 2.5, 3.5], &[floor], herocraft_common::BlockId::STONE);
        let start = cam.player_pos;
        walk(&mut cam, &gen, 1.0);
        let h = herocraft_world::BlockGeometry::new(cam.base_radius as f64).block_height as f32;
        let travelled = (cam.player_pos.normalize() - start.normalize()).length() * cam.base_radius / h;
        assert!(travelled < 1.5, "la marche doit arrêter le joueur ({} blocs parcourus)", travelled);
        assert!((altitude_blocks(&cam) - floor as f32).abs() < 1e-2, "pas de montée automatique : pieds à {}", altitude_blocks(&cam));
    }

    #[test]
    fn test_voxel_slabs_are_climbed_without_jumping() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        let slab = herocraft_common::blocks().find("plankSlab").unwrap();
        fill(&cam, &gen, &edits, &[1.5, 2.5, 3.5], &[floor], slab);
        walk(&mut cam, &gen, 0.45);
        assert!(cam.is_grounded);
        assert!((altitude_blocks(&cam) - (floor as f32 + 0.5)).abs() < 1e-2, "debout sur la dalle : pieds à {}", altitude_blocks(&cam));
    }

    #[test]
    fn test_voxel_stairs_are_climbed_step_by_step() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        let stairs = herocraft_common::blocks().find("plankStairs").unwrap();
        // Escalier tourné vers le joueur (marche basse devant lui), puis palier d'un bloc
        for r in [-1.0, 0.0, 1.0] {
            let pos = cell_at(&cam, 1.5, r, floor);
            edits.set_block(&gen, pos, stairs.with_rotation(facing(&cam, pos)));
        }
        fill(&cam, &gen, &edits, &[2.5, 3.5], &[floor], herocraft_common::BlockId::STONE);
        walk(&mut cam, &gen, 0.55);
        assert!((altitude_blocks(&cam) - (floor + 1) as f32).abs() < 1e-2, "en haut de l'escalier : pieds à {}", altitude_blocks(&cam));
    }

    /// Quarts de tour qui tournent l'avant d'une forme vers la caméra (comme à la pose)
    fn facing(cam: &OrbitCamera, pos: herocraft_world::BlockPos) -> u8 {
        let geometry = herocraft_world::BlockGeometry::new(cam.base_radius as f64);
        let origin = geometry.corner(pos.face, pos.x as f64, pos.y as f64, pos.z as f64);
        let axis_x = (geometry.corner(pos.face, pos.x as f64 + 1.0, pos.y as f64, pos.z as f64) - origin).normalize();
        let axis_z = (geometry.corner(pos.face, pos.x as f64, pos.y as f64, pos.z as f64 + 1.0) - origin).normalize();
        let (_, forward, _, _) = cam.look_vectors();
        let (x, z) = (forward.as_dvec3().dot(axis_x), forward.as_dvec3().dot(axis_z));
        if z.abs() >= x.abs() {
            if z > 0.0 { 0 } else { 2 }
        } else if x < 0.0 {
            1
        } else {
            3
        }
    }

    #[test]
    fn test_voxel_jump_while_walking_climbs_a_step() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        fill(&cam, &gen, &edits, &[1.5, 2.5, 3.5], &[floor], herocraft_common::BlockId::STONE);
        // Avance jusqu'à toucher la marche, puis saute sans lâcher la touche d'avance
        cam.move_forward = true;
        for _ in 0..60 {
            cam.update(0.016, &gen);
        }
        cam.jump_requested = true;
        let mut peak = 0.0_f32;
        for frame in 0..60 {
            // Lâche l'avance une fois au-dessus de la marche pour rester dessus
            cam.move_forward = frame < 20;
            cam.update(0.016, &gen);
            peak = peak.max(altitude_blocks(&cam));
        }
        assert!(peak > floor as f32 + 1.2, "le saut en marchant doit décoller (pic à {})", peak);
        assert!((altitude_blocks(&cam) - (floor + 1) as f32).abs() < 1e-2, "pieds à {}", altitude_blocks(&cam));
    }

    #[test]
    fn test_voxel_jump_while_walking_on_flat_ground() {
        let (mut cam, gen, _, floor) = voxel_test_camera();
        cam.move_forward = true;
        cam.update(0.016, &gen);
        let start = cam.player_pos;
        cam.jump_requested = true;
        let mut peak = 0.0_f32;
        for _ in 0..20 {
            cam.update(0.016, &gen);
            peak = peak.max(altitude_blocks(&cam));
        }
        let h = herocraft_world::BlockGeometry::new(cam.base_radius as f64).block_height as f32;
        let travelled = (cam.player_pos.normalize() - start.normalize()).length() * cam.base_radius / h;
        assert!(peak > floor as f32 + 1.0, "le saut doit décoller (pic à {})", peak);
        assert!(travelled > 0.8, "le joueur doit avancer pendant le saut ({} blocs)", travelled);
    }

    #[test]
    fn test_voxel_wall_stops_the_player() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        let start = cam.player_pos;
        fill(&cam, &gen, &edits, &[2.0], &[floor, floor + 1], herocraft_common::BlockId::STONE);
        walk(&mut cam, &gen, 1.0);
        let h = herocraft_world::BlockGeometry::new(cam.base_radius as f64).block_height as f32;
        let travelled = (cam.player_pos.normalize() - start.normalize()).length() * cam.base_radius / h;
        assert!(travelled > 0.3 && travelled < 1.5, "le mur doit arrêter le joueur ({} blocs parcourus)", travelled);
        assert!((altitude_blocks(&cam) - floor as f32).abs() < 1e-2, "pieds à {}", altitude_blocks(&cam));
    }

    #[test]
    fn test_voxel_ceiling_stops_a_jump() {
        let (mut cam, gen, edits, floor) = voxel_test_camera();
        fill(&cam, &gen, &edits, &[-1.0, 0.0, 1.0], &[floor + 2], herocraft_common::BlockId::STONE);
        cam.jump_requested = true;
        let mut peak = 0.0_f32;
        for _ in 0..120 {
            cam.update(0.016, &gen);
            peak = peak.max(altitude_blocks(&cam));
        }
        assert!(peak > floor as f32 + 0.05 && peak < floor as f32 + 0.5, "pic du saut à {} sous un plafond à {}", peak, floor + 2);
        assert!(cam.is_grounded);
    }

    #[test]
    fn test_fall_accelerates_then_reaches_terminal_speed() {
        let (mut cam, gen) = ground_test_camera();
        let block = cam.block_height();
        let up = cam.player_pos.normalize();
        cam.player_pos += up * 200.0 * block;
        cam.is_grounded = false;
        let mut speeds = Vec::new();
        for _ in 0..150 {
            cam.update(0.016, &gen);
            speeds.push(-cam.vertical_velocity / block);
        }
        // La chute part de l'arrêt et accélère de `GRAVITY` blocs/s chaque seconde…
        assert!(speeds[0] < 1.0, "départ à {} blocs/s", speeds[0]);
        let gained = (speeds[30] - speeds[0]) / (30.0 * 0.016);
        assert!((gained - GRAVITY).abs() < 0.5, "accélération de {gained} blocs/s²");
        assert!(speeds.windows(2).take(90).all(|w| w[1] > w[0]), "la vitesse croît tant que la limite n'est pas atteinte");
        // …jusqu'à sa vitesse limite, qu'elle ne dépasse jamais
        assert!(speeds.iter().all(|&v| v <= TERMINAL_FALL_SPEED + 1e-3));
        assert!((speeds[149] - TERMINAL_FALL_SPEED).abs() < 1e-3, "vitesse finale {} blocs/s", speeds[149]);
    }

    #[test]
    fn test_hurt_shakes_then_death_lays_the_view_down() {
        let (mut cam, gen) = ground_test_camera();
        assert_eq!(cam.view_roll(), 0.0);
        cam.hurt(1.0);
        cam.update(HURT_SHAKE_SECS * 0.5, &gen);
        assert!(cam.view_roll() > 0.5 * HURT_ROLL, "la vue penche ({})", cam.view_roll());
        cam.update(HURT_SHAKE_SECS, &gen);
        assert_eq!(cam.view_roll(), 0.0, "puis se redresse");

        // Mort : les entrées sont ignorées, la vue se couche
        let start = cam.player_pos;
        cam.is_dead = true;
        for _ in 0..60 {
            cam.move_forward = true;
            cam.jump_requested = true;
            cam.update(0.016, &gen);
        }
        assert!((cam.player_pos - start).length() < 1e-4, "un mort ne bouge plus");
        assert!((cam.view_roll() - DEATH_ROLL).abs() < 1e-3);
        assert!(cam.build_view_projection_matrix().is_finite());

        // Réapparition : debout
        cam.spawn_on_ground(&gen);
        assert!(!cam.is_dead);
        assert_eq!(cam.view_roll(), 0.0);
    }

    #[test]
    fn test_look_clamps_pitch() {
        let mut cam = OrbitCamera::default();
        cam.look(0.5, 10.0);
        assert!((cam.player_yaw - 0.5).abs() < 1e-6);
        assert!((cam.player_pitch - 1.48).abs() < 1e-6);
        cam.look(0.0, -10.0);
        assert!((cam.player_pitch + 1.48).abs() < 1e-6);
    }
}

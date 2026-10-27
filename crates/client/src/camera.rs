use glam::{DVec3, Mat4, Vec3};
use herocraft_world::generator::PlanetWorldGenerator;

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
    pub jump_requested: bool,

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
}

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
            jump_requested: false,

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

            base_radius: (herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32,
        }
    }
}

impl OrbitCamera {
    pub fn new(distance: f32, base_radius: f32, generator: &PlanetWorldGenerator) -> Self {
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
                let altitude_scale = (self.base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0)) as f32;
                spawn_ground_r = self.base_radius + (elev as f32) * altitude_scale;
                spawn_dir = dir;
                break;
            }
        }

        self.player_pos = spawn_dir * spawn_ground_r;
        self.vertical_velocity = 0.0;
        self.is_grounded = true;
        self.sync_orbit_to_ground();
        self.distance = 0.0;
        self.target_distance = 0.0;
        self.update_camera_bounds(generator);
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

        let (north, east) = if up.y.abs() < 0.999 {
            let n = (Vec3::Y - up * up.y).normalize();
            let e = n.cross(up).normalize();
            (n, e)
        } else {
            let sign = if up.y >= 0.0 { 1.0 } else { -1.0 };
            let n = Vec3::new(0.0, 0.0, -sign);
            let e = Vec3::new(1.0, 0.0, 0.0);
            (n, e)
        };

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

    /// Calcule le rayon minimum de sécurité du sol sous une direction donnée
    pub fn ground_radius_at(&self, dir: Vec3, generator: &PlanetWorldGenerator) -> f32 {
        herocraft_common::prof_scope!("camera::ground_radius_at");
        let dir_norm = dir.normalize();
        let dir_dvec = DVec3::new(dir_norm.x as f64, dir_norm.y as f64, dir_norm.z as f64);
        let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir_dvec);
        let elevation = generator.sample_elevation(dir_dvec, lat, lon);
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
        let eff_elev = elevation.max(sea);
        let altitude_scale = (self.base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0)) as f32;
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

    /// Synchronise la position au sol du joueur avec la vue orbitale satellite
    pub fn sync_ground_to_orbit(&mut self, generator: &PlanetWorldGenerator) {
        let dir = self.orbit_direction();
        let ground_radius = self.ground_radius_at(dir, generator);

        self.player_pos = dir * ground_radius;
        self.vertical_velocity = 0.0;
        self.is_grounded = true;
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
        let scale = self.base_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;
        let (up, _, _, look_dir) = self.look_vectors();
        let eye_height = 0.065 * scale; // ~1.7 blocs au-dessus des pieds
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

        let (_up, forward_tangent, right_tangent, _) = self.look_vectors();
        let scale = self.base_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;

        // 1. Déplacement horizontal ZQSD / WASD
        let mut move_dir = Vec3::ZERO;
        if self.move_forward { move_dir += forward_tangent; }
        if self.move_backward { move_dir -= forward_tangent; }
        if self.move_left { move_dir -= right_tangent; }
        if self.move_right { move_dir += right_tangent; }

        if move_dir.length_squared() > 0.001 {
            move_dir = move_dir.normalize();
            // Vitesse adaptée à l'échelle : ~5.2 blocs/s en marche, ~11.7 blocs/s en sprint
            let speed = if self.is_sprinting { 0.45 * scale } else { 0.20 * scale };
            self.player_pos += move_dir * speed * dt;
        }

        // 2. Gravité sphérique et hauteur du relief sous les pieds
        let new_up = self.player_pos.normalize();
        let ground_radius = self.ground_radius_at(new_up, generator);

        let mut current_radius = self.player_pos.length();

        // Saut
        if self.jump_requested && self.is_grounded {
            self.vertical_velocity = 1.2 * scale;
            self.is_grounded = false;
            self.jump_requested = false;
        }

        // Gravité sphérique et franchissement doux des pentes
        if current_radius > ground_radius {
            // Descente douce si la marche au sol franchit une dénivellation légère (< 1.5 bloc)
            if self.is_grounded && (current_radius - ground_radius) < 0.05 * scale {
                current_radius = ground_radius;
                self.vertical_velocity = 0.0;
            } else {
                self.vertical_velocity -= 6.0 * scale * dt;
                current_radius += self.vertical_velocity * dt;
                if current_radius <= ground_radius {
                    current_radius = ground_radius;
                    self.vertical_velocity = 0.0;
                    self.is_grounded = true;
                } else {
                    self.is_grounded = false;
                }
            }
        } else {
            // Montée sur relief / contact direct
            current_radius = ground_radius;
            self.vertical_velocity = 0.0;
            self.is_grounded = true;
        }

        self.player_pos = new_up * current_radius;

        // En mode sol, garder l'orbite alignée pour un dézoom fluide
        if self.view_mode() != CameraViewMode::SatelliteOrbit {
            self.sync_orbit_to_ground();
        }

        // Calcul anti-collision garanti au-dessus du relief
        self.update_camera_bounds(generator);
    }

    /// Position 3D de l'œil de la caméra dans l'espace monde (garantie strictement au-dessus du sol)
    pub fn eye_position(&self) -> Vec3 {
        self.effective_eye
    }

    /// Matrice View-Projection avec FOV adaptatif selon le mode (FPS immersif -> Satellite téléobjectif)
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let eye = self.effective_eye;
        let (up, _, _, look_dir) = self.look_vectors();
        let scale = self.base_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;
        let eye_height = 0.065 * scale;
        let head = self.player_pos + up * eye_height;

        let (target, cam_up, fov, z_near) = if self.effective_distance <= 0.05 {
            // FPS : regard orienté selon le pitch/yaw du joueur
            (eye + look_dir, up, 70.0_f32.to_radians(), 0.01_f32)
        } else if self.effective_distance <= self.base_radius {
            // TPS : regard centré sur la tête du joueur
            let t_fov = (self.effective_distance / self.base_radius).clamp(0.0, 1.0);
            let fov = (70.0 * (1.0 - t_fov) + 38.0 * t_fov).to_radians();
            (head, up, fov, 0.05_f32)
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

        cam.effective_distance = 330.0;
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
        let scale = cam.base_radius / herocraft_world::REFERENCE_BASE_RADIUS as f32;
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
        let mut cam = OrbitCamera::new(165.0, 25.0, &gen);

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
        let mut cam = OrbitCamera::new(165.0, 25.0, &gen);

        // Au départ en orbite
        assert_eq!(cam.target_distance, 165.0);
        assert_eq!(cam.distance, 165.0);

        // Déclenchement d'une transition vers le sol (FPS)
        cam.transition_to_ground(&gen);
        assert_eq!(cam.target_distance, 0.0);
        assert_eq!(cam.distance, 165.0); // La distance ne se téléporte pas !

        // Simulation de quelques frames
        let mut intermediate_distances = Vec::new();
        for _ in 0..60 {
            cam.update(0.016, &gen);
            intermediate_distances.push(cam.distance);
        }

        // Vérification de la continuité et de la progression fluide
        assert!(intermediate_distances[0] < 165.0, "Camera should start descending");
        assert!(intermediate_distances[10] < intermediate_distances[0]);
        assert!(intermediate_distances[30] < intermediate_distances[10]);
        let final_d = intermediate_distances.last().copied().unwrap();
        assert!(final_d < 5.0, "Camera should have approached ground: final_d={}", final_d);
    }
}

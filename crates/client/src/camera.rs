use glam::{Mat4, Vec3};

/// Caméra orbitale 3D centrée sur la planète
pub struct OrbitCamera {
    /// Cible autour de laquelle orbite la caméra (centre de la planète)
    pub target: Vec3,
    /// Distance par rapport à la cible
    pub distance: f32,
    /// Angle horizontal (lacet / yaw en radians)
    pub yaw: f32,
    /// Angle vertical (tangage / pitch en radians)
    pub pitch: f32,

    /// Paramètres de projection
    pub fov_y: f32,
    pub aspect: f32,
    pub z_near: f32,
    pub z_far: f32,

    /// Sensibilité de rotation et de zoom
    pub rotate_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub min_distance: f32,
    pub max_distance: f32,

    /// Auto-rotation pour le mode vitrine
    pub auto_rotate: bool,
    pub auto_rotate_speed: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 165.0,
            yaw: 0.5,
            pitch: 0.35,
            fov_y: 22.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
            z_near: 0.5,
            z_far: 2000.0,
            rotate_sensitivity: 0.005,
            zoom_sensitivity: 0.08,
            min_distance: 28.5,
            max_distance: 450.0,
            auto_rotate: false,
            auto_rotate_speed: 0.2,
        }
    }
}

impl OrbitCamera {
    #[allow(dead_code)]
    pub fn new(distance: f32) -> Self {
        Self {
            distance,
            ..Default::default()
        }
    }

    /// Fait tourner la caméra selon le delta de la souris (drag & drop)
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        // Sensibilité adaptée à la distance : plus fine au ras du sol
        let dist_factor = (self.distance / 165.0).clamp(0.20, 1.0);
        self.yaw += dx * self.rotate_sensitivity * dist_factor;
        // Inversion du pitch pour que tirer vers le bas fasse pivoter la planète vers le bas
        self.pitch -= dy * self.rotate_sensitivity * dist_factor;

        // Limiter le pitch pour éviter le basculement gimbal lock aux pôles
        let limit = 1.52; // ~87 degrés
        self.pitch = self.pitch.clamp(-limit, limit);
    }

    /// Ajuste le zoom via la molette de la souris
    pub fn zoom(&mut self, delta_y: f32) {
        // Zoom proportionnel à la distance actuelle
        let factor = 1.0 - delta_y * self.zoom_sensitivity;
        self.distance = (self.distance * factor).clamp(self.min_distance, self.max_distance);
    }

    /// Mise à jour temporelle (ex: auto-rotation)
    pub fn update(&mut self, dt: f32) {
        if self.auto_rotate {
            self.yaw += self.auto_rotate_speed * dt;
        }
    }

    /// Position 3D de l'œil de la caméra dans le monde
    pub fn eye_position(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Matrice View-Projection pour le vertex shader (FOV fixe et stable à 22°)
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let eye = self.eye_position();
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.z_near, self.z_far);
        proj * view
    }

    /// Réinitialise la vue par défaut
    pub fn reset(&mut self) {
        self.distance = 165.0;
        self.yaw = 0.5;
        self.pitch = 0.35;
        self.target = Vec3::ZERO;
    }
}

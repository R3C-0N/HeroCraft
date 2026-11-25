//! Réglages graphiques : préréglages de qualité, distance et précision du relief lointain,
//! nuages, post-traitement HDR, ombres, atmosphère et éclairage. Le menu Options les modifie,
//! les passes de rendu les lisent.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodDistanceSetting {
    Courte,
    Normale,
    Elevee,
}

impl LodDistanceSetting {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Courte => "Courte (Rapide)",
            Self::Normale => "Normale (Équilibrée)",
            Self::Elevee => "Élevée (Haute Fidélité)",
        }
    }
}

/// Précision du relief lointain (patchs polygonaux au-delà des chunks voxel)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodPrecisionSetting {
    /// Couleurs seules, cellules de 4 blocs et plus (léger)
    Normale,
    /// Allure des voxels (blocs texturés, arbres) jusqu'à ~380 blocs, cellules de 2 blocs et
    /// arbres simplifiés jusqu'à ~770, couleurs au-delà (qualité ultra)
    Ultra,
    /// Test : allure des voxels jusqu'à ~1 300 blocs (très lourd)
    Exacte,
}

impl LodPrecisionSetting {
    pub const ALL: [LodPrecisionSetting; 3] = [Self::Normale, Self::Ultra, Self::Exacte];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Normale => "Normale",
            Self::Ultra => "Ultra",
            Self::Exacte => "1:1 (test)",
        }
    }
}

/// Préréglage de qualité graphique : règle d'un coup les effets les plus chers pour le GPU
/// (mesurés avec `--benchmark` sur un GPU intégré Iris Xe : ombres du relief ~4 ms par image,
/// rayons crépusculaires 1 à 4 ms, ombres des nuages ~1 ms, bloom ~0,5 ms ; sur une UHD 620 en
/// 1600 × 900 : ombres du relief ~15 ms, rayons ~21 ms, nuages ~9 ms). Sur une puce intégrée,
/// la qualité haute garde tous les effets mais en version allégée (ombres et rayons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsQuality {
    /// Ni ombres, ni rayons, ni bloom, distance de détail courte
    Basse,
    /// Ombres du relief allégées, sans ombres des nuages ni rayons crépusculaires
    Moyenne,
    /// Tous les effets
    Haute,
    /// Tous les effets, et le relief lointain vu du sol avec l'allure des blocs proches
    Ultra,
}

/// Effets réglés par un préréglage de qualité
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityEffects {
    pub shadows: bool,
    pub cloud_shadows: bool,
    pub god_rays: bool,
    pub bloom: bool,
    pub lod_distance: LodDistanceSetting,
    pub lod_precision: LodPrecisionSetting,
    /// Ombres du relief fines (`ShadowSettings::fine`)
    pub fine_shadows: bool,
    /// Rayons crépusculaires fins (`AtmosphereSettings::god_rays_fine`)
    pub fine_god_rays: bool,
}

impl GraphicsQuality {
    pub const ALL: [GraphicsQuality; 4] =
        [GraphicsQuality::Basse, GraphicsQuality::Moyenne, GraphicsQuality::Haute, GraphicsQuality::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            Self::Basse => "Basse",
            Self::Moyenne => "Moyenne",
            Self::Haute => "Haute",
            Self::Ultra => "Ultra",
        }
    }

    /// Effets du préréglage ; `integrated_gpu` : puce graphique intégrée, où même la qualité
    /// haute garde les ombres et les rayons allégés
    pub fn effects(self, integrated_gpu: bool) -> QualityEffects {
        let fine = !integrated_gpu;
        match self {
            Self::Basse => QualityEffects {
                shadows: false,
                cloud_shadows: false,
                god_rays: false,
                bloom: false,
                lod_distance: LodDistanceSetting::Courte,
                lod_precision: LodPrecisionSetting::Normale,
                fine_shadows: false,
                fine_god_rays: false,
            },
            Self::Moyenne => QualityEffects {
                shadows: true,
                cloud_shadows: false,
                god_rays: false,
                bloom: true,
                lod_distance: LodDistanceSetting::Normale,
                lod_precision: LodPrecisionSetting::Normale,
                fine_shadows: false,
                fine_god_rays: false,
            },
            Self::Haute => QualityEffects {
                shadows: true,
                cloud_shadows: true,
                god_rays: true,
                bloom: true,
                lod_distance: LodDistanceSetting::Normale,
                lod_precision: LodPrecisionSetting::Normale,
                fine_shadows: fine,
                fine_god_rays: fine,
            },
            Self::Ultra => QualityEffects { lod_precision: LodPrecisionSetting::Ultra, ..Self::Haute.effects(integrated_gpu) },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsLimitSetting {
    Fps60,
    Fps120,
    Unlimited,
}

impl FpsLimitSetting {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Fps60 => "60 FPS",
            Self::Fps120 => "120 FPS",
            Self::Unlimited => "Illimité",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSettings {
    pub vsync: bool,
    pub fullscreen: bool,
    pub lod_distance: LodDistanceSetting,
    /// Précision des patchs du relief lointain (`PlanetLodManager::set_precision`)
    pub lod_precision: LodPrecisionSetting,
    pub fps_limit: FpsLimitSetting,
    /// Aplatissement de la planète vue du sol : 0 = vraie courbure, 1 = horizon le plus lointain
    /// possible sans voir les antipodes (renderer.rs, `planet_flattening`)
    pub flattening: f32,
    /// Anticrénelage MSAA 4× de la passe 3D (si la carte le permet). Coupé par défaut sur les
    /// puces intégrées, où il coûte cher en bande passante (`UiState::adapt_to_gpu`).
    pub msaa: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            vsync: true,
            fullscreen: false,
            lod_distance: LodDistanceSetting::Normale,
            lod_precision: LodPrecisionSetting::Normale,
            fps_limit: FpsLimitSetting::Fps60,
            flattening: 1.0,
            msaa: true,
        }
    }
}

/// Réglages visuels des nuages 3D (Marshmallow SDF)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudSettings {
    pub density: f32,          // Densité / visibilité globale [0.0 .. 2.0] (défaut: 1.0)
    pub roundness: f32,        // Rayon d'arrondi des arêtes (Bevel) [0.05 .. 0.55] (défaut: 0.45)
    pub shadow_intensity: f32, // Ombrage de sous-face violacée [0.0 .. 2.0] (défaut: 1.0)
    pub sunset_glow: f32,      // Éclat chaud au coucher de soleil [0.0 .. 2.0] (défaut: 1.0)
    pub micro_noise: f32,      // Micro-texture / grain mat [0.0 .. 1.0] (défaut: 0.0)
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self { density: 1.0, roundness: 0.45, shadow_intensity: 1.0, sunset_glow: 1.0, micro_noise: 0.0 }
    }
}

/// Courbe de conversion HDR -> écran appliquée par la passe de composition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tonemapper {
    /// Ancien rendu : valeurs écrêtées à 1.0
    Clip,
    /// Identique à l'ancien rendu dans les tons moyens, hautes lumières compressées
    SoftShoulder,
    Aces,
    AgX,
}

impl Tonemapper {
    pub const ALL: [Tonemapper; 4] = [Tonemapper::Clip, Tonemapper::SoftShoulder, Tonemapper::Aces, Tonemapper::AgX];

    pub fn label(self) -> &'static str {
        match self {
            Tonemapper::Clip => "Écrêtage (ancien rendu)",
            Tonemapper::SoftShoulder => "Épaule douce",
            Tonemapper::Aces => "ACES",
            Tonemapper::AgX => "AgX",
        }
    }

    /// Valeur de `post.mode.x` dans post_shader.wgsl
    pub(crate) fn shader_index(self) -> u32 {
        match self {
            Tonemapper::Clip => 0,
            Tonemapper::SoftShoulder => 1,
            Tonemapper::Aces => 2,
            Tonemapper::AgX => 3,
        }
    }
}

/// Réglages du post-traitement HDR (exposition, tonemapper, bloom)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostSettings {
    pub exposure_ev: f32, // Exposition en EV [-3.0 .. 3.0] (0 = neutre)
    pub tonemapper: Tonemapper,
    pub bloom_enabled: bool,
    pub bloom_intensity: f32, // Part du halo ajoutée à l'image [0.0 .. 1.0]
    pub bloom_threshold: f32, // Luminance HDR à partir de laquelle un pixel diffuse [0.0 .. 8.0]
    pub bloom_knee: f32,      // Largeur de la transition douce autour du seuil [0.0 .. 3.0]
}

impl Default for PostSettings {
    fn default() -> Self {
        Self {
            exposure_ev: -0.3,
            tonemapper: Tonemapper::AgX,
            bloom_enabled: true,
            bloom_intensity: 0.25,
            bloom_threshold: 6.0,
            bloom_knee: 2.0,
        }
    }
}

/// Réglages des ombres portées du soleil et des nuages
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    pub strength: f32, // Part de la lumière directe retirée à l'ombre du relief [0.0 .. 1.0]
    pub cloud_shadows: bool,
    pub cloud_strength: f32, // Part de la lumière directe retirée sous un nuage [0.0 .. 1.0]
    /// Ombres fines : cascades de 2048 texels filtrées sur 3x3, cascades lointaines redessinées
    /// au plus une image sur deux. Sinon (allégées) : 1024 texels, filtrage 2x2, la cascade du
    /// milieu au plus une image sur deux et la plus lointaine une sur quatre. Dans les deux cas,
    /// une cascade lointaine est gardée tant que le soleil, le joueur et le relief ne bougent
    /// pas assez pour que ça se voie (shadow.rs).
    pub fine: bool,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self { enabled: true, strength: 1.0, cloud_shadows: true, cloud_strength: 0.7, fine: true }
    }
}

impl ShadowSettings {
    /// Réglages par défaut pour cette carte graphique (allégés sur une puce intégrée)
    pub fn for_gpu(integrated_gpu: bool) -> Self {
        Self { fine: !integrated_gpu, ..Self::default() }
    }
}

/// Réglages de l'atmosphère physique et des rayons crépusculaires (la brume est réglée par
/// le curseur Brume de l'éclairage)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereSettings {
    pub air_density: f32, // Densité de l'air (diffusion de Rayleigh) [0.2 .. 3.0]
    pub sun_halo: f32,    // Anisotropie de la brume (g de Mie) : halo resserré autour du soleil [0.0 .. 0.95]
    pub god_rays: bool,
    pub god_rays_intensity: f32, // Accentuation de l'ombre de l'air (1 = physique) [0.0 .. 4.0]
    /// Rayons fins : marche de 28 pas à demi-résolution. Sinon (allégés) : 14 pas au quart de
    /// la résolution, environ 8 fois moins de calcul (god_rays.wgsl).
    pub god_rays_fine: bool,
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self { air_density: 1.0, sun_halo: 0.8, god_rays: true, god_rays_intensity: 2.5, god_rays_fine: true }
    }
}

impl AtmosphereSettings {
    /// Réglages par défaut pour cette carte graphique (rayons allégés sur une puce intégrée)
    pub fn for_gpu(integrated_gpu: bool) -> Self {
        Self { god_rays_fine: !integrated_gpu, ..Self::default() }
    }
}

/// Réglages d'éclairage et d'ambiance céleste ajustables via sliders
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingSettings {
    pub moon_intensity: f32, // Multiplicateur lumière de Lune (défaut: 1.0, [0.0 .. 3.0])
    pub sun_intensity: f32,  // Multiplicateur lumière du Soleil (défaut: 1.0, [0.0 .. 3.0])
    pub night_ambient: f32,  // Multiplicateur obscurité nocturne / étoiles (défaut: 1.0, [0.0 .. 3.0])
    pub day_ambient: f32,    // Multiplicateur clarté ambiante diurne (défaut: 1.0, [0.0 .. 3.0])
    pub moon_speed: f32,     // Vitesse orbitale de la Lune (défaut: 1.0, [0.0 .. 5.0], 0 = fige la lune)
    pub fog_intensity: f32,  // Multiplicateur brouillard de distance (défaut: 1.0, [0.0 .. 2.0])
    pub cloud_density: f32,  // Densité des nuages volumétriques arrondis (défaut: 1.0, [0.0 .. 2.0])
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            moon_intensity: 0.25,
            sun_intensity: 1.0,
            night_ambient: 2.10,
            day_ambient: 1.0,
            moon_speed: 1.0,
            fog_intensity: 1.0,
            cloud_density: 1.0,
        }
    }
}

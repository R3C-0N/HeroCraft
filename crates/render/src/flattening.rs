//! Aplatissement de la planète vue du sol : le relief est redessiné sur un globe plus grand pour
//! reculer l'horizon (common.wgsl, `flatten_point`).

/// Plus haut relief (en blocs au-dessus de la mer) pris en compte pour borner l'aplatissement
pub(crate) const FLATTEN_MOUNTAIN_BLOCKS: f32 = 150.0;

/// Altitudes de l'œil au-dessus de la mer (en blocs) entre lesquelles l'aplatissement s'efface :
/// complet en dessous, vraie courbure au-dessus. Vue de haut, la planète entière est visible et
/// n'a plus besoin d'être aplatie : le dézoom vers l'orbite la ramène doucement à sa forme.
pub(crate) const FLATTEN_FADE_BLOCKS: (f32, f32) = (40.0, 400.0);

/// Rayon apparent de la planète vue du sol, en multiple de son vrai rayon (k >= 1).
///
/// Le relief est redessiné sur un globe k fois plus grand, tangent à la vraie planète sous la
/// caméra : l'horizon recule de √k. Toute la planète tient alors dans une calotte de ce grand
/// globe, dont le bord (les antipodes) ne doit jamais se voir : la distance de l'horizon depuis
/// l'œil, plus celle d'où l'on voit encore un sommet au-delà, reste sous 0,9 demi-tour de la vraie
/// planète. k décroît donc avec l'altitude, et s'efface tout à fait entre 40 et 400 blocs
/// au-dessus de la mer (`FLATTEN_FADE_BLOCKS`). `strength` (0..1) va de la vraie courbure (0) à
/// ce maximum (1), en progression géométrique.
pub fn planet_flattening(eye_radius: f32, base_radius: f32, strength: f32) -> f32 {
    let block = base_radius * std::f32::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f32);
    let sea_radius = base_radius + herocraft_world::SEA_LEVEL_BLOCKS as f32 * block;
    // Au moins la hauteur des yeux : k varierait sinon très vite au ras de l'eau
    let h = (eye_radius - sea_radius).max(2.0 * block);
    let reach = h.sqrt() + (FLATTEN_MOUNTAIN_BLOCKS * block).sqrt();
    // (0,9 π)² / 2 ≈ 4 : √(2 k R h) + √(2 k R H) <= 0,9 π R
    let k_max = (4.0 * sea_radius / (reach * reach)).max(1.0);
    let (fade_low, fade_high) = FLATTEN_FADE_BLOCKS;
    let t = ((h / block - fade_low) / (fade_high - fade_low)).clamp(0.0, 1.0);
    let fade = 1.0 - t * t * (3.0 - 2.0 * t);
    k_max.powf(strength.clamp(0.0, 1.0) * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_flattening_bounds() {
        let r = 50.0_f32;
        let block = r * std::f32::consts::FRAC_PI_4 * (2.0 / herocraft_world::FACE_BLOCKS as f32);
        let sea = r + herocraft_world::SEA_LEVEL_BLOCKS as f32 * block;
        // Réglage à 0 : vraie courbure à toute altitude
        assert_eq!(planet_flattening(sea + 2.0 * block, r, 0.0), 1.0);
        // Au sol : le globe apparent est bien plus grand, l'horizon recule d'au moins 3 fois
        let k_ground = planet_flattening(sea + 1.7 * block, r, 1.0);
        assert!(k_ground > 9.0, "k au sol = {k_ground}");
        // Décroît avec l'altitude et revient à 1 bien avant l'orbite
        let k_high = planet_flattening(sea + 100.0 * block, r, 1.0);
        assert!(k_high > 1.0 && k_high < k_ground);
        assert_eq!(planet_flattening(sea + 400.0 * block, r, 1.0), 1.0);
        assert_eq!(planet_flattening(r * 6.6, r, 1.0), 1.0);
        // Sans saut pendant le dézoom
        let mut last = planet_flattening(sea, r, 1.0);
        for i in 0..=800 {
            let k = planet_flattening(sea + i as f32 * 0.5 * block, r, 1.0);
            assert!(k <= last + 1e-3 && last / k < 1.05, "saut de k à {} blocs : {last} -> {k}", i as f32 * 0.5);
            last = k;
        }
        // Les antipodes restent cachées : horizon + portée d'un sommet sous 0,9 demi-tour
        for h_blocks in [0.0_f32, 2.0, 30.0, 300.0] {
            let k = planet_flattening(sea + h_blocks * block, r, 1.0);
            let big = k * sea;
            let seen = (2.0 * big * h_blocks * block).sqrt() + (2.0 * big * FLATTEN_MOUNTAIN_BLOCKS * block).sqrt();
            assert!(seen <= 0.9 * std::f32::consts::PI * sea * 1.001, "h = {h_blocks} : {seen}");
        }
    }
}

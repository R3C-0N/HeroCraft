//! Affichage des créatures : une silhouette colorée par espèce, orientée selon leur cap. En ligne,
//! elles viennent du serveur ; en solo, la simulation `CreatureWorld` tourne dans le jeu.

use glam::Vec3;
use herocraft_common::{tangent_frame, CreatureKind};
use herocraft_world::BlockGeometry;

use crate::block_outline::ColoredBox;

/// Nombre maximal de silhouettes de créatures dessinées
pub const MAX_CREATURE_BOXES: usize = herocraft_world::MAX_CREATURES;

/// Silhouettes et couleurs des créatures `(espèce, pieds, cap)`
pub fn boxes(creatures: impl Iterator<Item = (CreatureKind, Vec3, f32)>, base_radius: f32) -> Vec<ColoredBox> {
    let block = BlockGeometry::new(base_radius as f64).block_height as f32;
    creatures.take(MAX_CREATURE_BOXES).map(|(kind, feet, yaw)| (creature_box(kind, feet, yaw, block), kind.color())).collect()
}

/// Coins du corps d'une créature aux pieds `feet` : bit 0 = côté, bit 1 = haut, bit 2 = avant
fn creature_box(kind: CreatureKind, feet: Vec3, yaw: f32, block: f32) -> [Vec3; 8] {
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let side = forward.cross(up);
    let (s, f, h) = (side * kind.half_width() * block, forward * kind.half_length() * block, up * kind.height() * block);
    std::array::from_fn(|i| {
        let ss = if i & 1 == 0 { -1.0 } else { 1.0 };
        let sf = if i & 4 == 0 { -1.0 } else { 1.0 };
        let y = if i & 2 == 0 { Vec3::ZERO } else { h };
        feet + s * ss + y + f * sf
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creature_box_faces_its_heading() {
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (east, north) = tangent_frame(feet.normalize());
        for (yaw, heading) in [(0.0, north), (std::f32::consts::FRAC_PI_2, east)] {
            let c = creature_box(CreatureKind::Sheep, feet, yaw, 1.0);
            // Le long du cap : la longueur du corps ; en travers : sa largeur ; en haut : sa hauteur
            assert!(((c[4] - c[0]).dot(heading) - 2.0 * CreatureKind::Sheep.half_length()).abs() < 1e-3);
            assert!(((c[1] - c[0]).length() - 2.0 * CreatureKind::Sheep.half_width()).abs() < 1e-3);
            assert!(((c[2] - c[0]).length() - CreatureKind::Sheep.height()).abs() < 1e-3);
            let bottom = (c[0] + c[1] + c[4] + c[5]) / 4.0;
            assert!(bottom.distance(feet) < 1e-3);
        }
    }
}

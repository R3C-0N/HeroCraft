//! Lumière des blocs (torches, lampadaires, lave…) : chaque bloc qui éclaire répand sa lumière de
//! proche en proche, perdant un niveau par bloc, à travers tout ce qui n'est pas un bloc plein. Elle
//! est calculée au maillage d'un chunk, dans le chunk et sur `LIGHT_REACH` blocks autour (une
//! lumière voisine y entre), puis portée par chaque sommet jusqu'au shader du terrain.

use std::collections::VecDeque;

use herocraft_common::{FACE_DIRS, opposite_face};

/// Plus haute lumière d'un bloc
pub(crate) const MAX_LIGHT: u8 = 15;
/// Distance au-delà de laquelle une lumière n'éclaire plus rien (elle perd un niveau par bloc)
pub const LIGHT_REACH: i32 = MAX_LIGHT as i32 - 1;

/// Côté du champ : le chunk et sa marge de chaque côté
const SIDE: i32 = 32 + 2 * LIGHT_REACH;

/// Case déjà lue : bit 7, et les faces qu'elle couvre sur les 6 bits bas (`FACE_DIRS`)
const KNOWN: u8 = 0x80;
/// Bloc plein : toutes ses faces couvertes
pub(crate) const FULL: u8 = 0x3F;

/// Lumière de chaque case d'un chunk et de sa marge, coordonnées locales au chunk (de
/// `-LIGHT_REACH` à `32 + LIGHT_REACH`, exclu)
pub(crate) struct LightField {
    levels: Vec<u8>,
    /// Faces couvertes de chaque case lue (`KNOWN` en plus)
    covers: Vec<u8>,
}

#[inline(always)]
fn index(x: i32, y: i32, z: i32) -> Option<usize> {
    let (x, y, z) = (x + LIGHT_REACH, y + LIGHT_REACH, z + LIGHT_REACH);
    if (0..SIDE).contains(&x) && (0..SIDE).contains(&y) && (0..SIDE).contains(&z) {
        Some(((y * SIDE + z) * SIDE + x) as usize)
    } else {
        None
    }
}

impl LightField {
    /// Répand la lumière des `sources` (position locale, niveau). `covers(x, y, z)` donne les
    /// faces de la case que son bloc couvre entièrement (bit `dir` de `FACE_DIRS`, `FULL` pour un
    /// bloc plein) : la lumière n'entre ni ne sort par une face couverte (un toit d'escaliers, une
    /// dalle l'arrêtent). Chaque case n'est lue qu'une fois. `None` sans source qui atteigne le chunk.
    pub(crate) fn compute(sources: &[(i32, i32, i32, u8)], mut covers: impl FnMut(i32, i32, i32) -> u8) -> Option<Self> {
        // Une lumière qui n'atteint pas le chunk (ni la rangée de cases qui borde ses faces) ne sert à rien
        let reaches = |&&(x, y, z, level): &&(i32, i32, i32, u8)| {
            let gap = |v: i32| {
                if v < -1 {
                    -1 - v
                } else if v > 32 {
                    v - 32
                } else {
                    0
                }
            };
            level > 0 && gap(x) + gap(y) + gap(z) < level as i32
        };
        let sources: Vec<_> = sources.iter().filter(reaches).collect();
        if sources.is_empty() {
            return None;
        }
        let volume = (SIDE * SIDE * SIDE) as usize;
        let mut levels = vec![0u8; volume];
        let mut state = vec![0u8; volume];
        let mut queue = VecDeque::new();
        for &&(x, y, z, level) in &sources {
            if let Some(i) = index(x, y, z)
                && levels[i] < level
            {
                levels[i] = level.min(MAX_LIGHT);
                // Le bloc qui éclaire laisse sortir sa lumière de tous côtés
                state[i] = KNOWN;
                queue.push_back((x, y, z));
            }
        }
        while let Some((x, y, z)) = queue.pop_front() {
            let i = index(x, y, z).unwrap();
            let level = levels[i];
            if level <= 1 {
                continue;
            }
            for (dir, (dx, dy, dz)) in FACE_DIRS.into_iter().enumerate() {
                if state[i] & (1 << dir) != 0 {
                    continue;
                }
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                let Some(n) = index(nx, ny, nz) else { continue };
                if levels[n] >= level - 1 {
                    continue;
                }
                if state[n] & KNOWN == 0 {
                    state[n] = KNOWN | (covers(nx, ny, nz) & FULL);
                }
                if state[n] & (1 << opposite_face(dir)) != 0 {
                    continue;
                }
                levels[n] = level - 1;
                queue.push_back((nx, ny, nz));
            }
        }
        Some(Self { levels, covers: state })
    }

    /// La case est un bloc plein (lu pendant la propagation ; une case jamais atteinte compte comme vide)
    #[inline(always)]
    pub(crate) fn is_full(&self, x: i32, y: i32, z: i32) -> bool {
        index(x, y, z).is_some_and(|i| self.covers[i] & FULL == FULL)
    }

    /// Lumière de la case (0 hors du champ, et dans un bloc plein)
    #[inline(always)]
    pub(crate) fn level(&self, x: i32, y: i32, z: i32) -> u8 {
        index(x, y, z).map_or(0, |i| self.levels[i])
    }
}

/// Intensité portée par un sommet qui ne brille pas lui-même : au plus juste sous `EMISSIVE`
pub(crate) const LIT_MAX: f32 = 0.96;
/// Intensité des faces d'un bloc qui éclaire : le shader les fait briller (flamme, lanterne)
pub(crate) const EMISSIVE: f32 = 1.0;

/// Intensité rendue d'un niveau de lumière (moyen, d'où les fractions) : chaque niveau en moins
/// éclaire d'un cinquième de moins, et l'éclat tombe à zéro au niveau 0
pub fn intensity(level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    let fade = (level / 1.5).min(1.0);
    (0.8f32.powf(MAX_LIGHT as f32 - level) * fade).min(LIT_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_fades_by_one_per_block_and_stops_at_walls() {
        // Un mur plein en x = 4, de haut en bas
        let field = LightField::compute(&[(0, 5, 0, 15)], |x, _, _| if x == 4 { FULL } else { 0 }).unwrap();
        assert_eq!(field.level(0, 5, 0), 15);
        assert_eq!(field.level(1, 5, 0), 14);
        assert_eq!(field.level(0, 8, 2), 10);
        assert_eq!(field.level(4, 5, 0), 0, "le mur reste noir");
        // Derrière le mur, la lumière n'arrive qu'en le contournant… par nulle part ici
        assert_eq!(field.level(5, 5, 0), 0);
    }

    #[test]
    fn light_goes_around_obstacles() {
        // Un muret en x = 2, de y = 0 à 6 : la lumière passe par-dessus
        let field =
            LightField::compute(&[(0, 0, 0, 15)], |x, y, z| if x == 2 && (0..=6).contains(&y) && z == 0 { FULL } else { 0 }).unwrap();
        assert_eq!(field.level(3, 0, 0), 15 - 5, "contourné par z = ±1");
    }

    #[test]
    fn far_lights_are_ignored() {
        assert!(LightField::compute(&[(-20, 5, 5, 15)], |_, _, _| 0).is_none());
        assert!(LightField::compute(&[(-10, 5, 5, 15)], |_, _, _| 0).is_some());
    }

    #[test]
    fn covered_faces_stop_light() {
        // Une dalle (dessous couvert, bit 3) au-dessus de la lumière : elle ne passe pas vers le haut
        let field = LightField::compute(&[(0, 0, 0, 15)], |x, y, z| if (x, y, z) == (0, 1, 0) { 1 << 3 } else { 0 }).unwrap();
        assert_eq!(field.level(0, 1, 0), 15 - 3, "contournée par le côté");
        assert!(!field.is_full(0, 1, 0));
    }

    #[test]
    fn intensity_fades_out() {
        assert_eq!(intensity(0.0), 0.0);
        assert_eq!(intensity(15.0), LIT_MAX);
        assert!(intensity(15.0) < EMISSIVE);
        assert!(intensity(14.0) < intensity(15.0));
        assert!(intensity(0.5) < intensity(1.0));
    }
}

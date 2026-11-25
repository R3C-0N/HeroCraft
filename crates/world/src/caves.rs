//! Grottes : tunnels sinueux et cavernes creusés sous la surface.
//!
//! Le creusement est un champ de bruit 3D évalué sur un réseau de points espacés de `CAVE_CELL`
//! blocs à l'horizontale et `CAVE_CELL_Y` en altitude, aligné sur la grille de blocs de chaque face,
//! puis interpolé linéairement entre ces points. Un chunk entier n'évalue ainsi qu'un point sur 128,
//! et un bloc lu seul (visée, collisions) refait exactement le même calcul à partir des 8 points qui
//! l'entourent : les deux donnent toujours le même bloc.
//!
//! Les grottes restent dans une bande de `CAVE_DEPTH` blocs sous la surface : le maillage n'a ainsi
//! que les couches proches du relief à construire.

use std::ops::Range;

use glam::DVec3;
use herocraft_common::PlanetFace;

use crate::generator::{ColumnSample, SEA_LEVEL_BLOCKS};

/// Écart en blocs entre deux points du réseau de bruit, à l'horizontale
pub(crate) const CAVE_CELL: i32 = 4;
/// Écart en blocs entre deux points du réseau de bruit, en altitude (les bruits varient lentement :
/// une maille plus haute divise par deux le nombre de points sans changer la forme des grottes)
pub(crate) const CAVE_CELL_Y: i32 = 8;
/// Les blocs en dessous de cette altitude ne sont jamais creusés (fond du monde)
pub(crate) const CAVE_FLOOR: i32 = 4;
/// Profondeur maximale des grottes sous la surface, en blocs
pub const CAVE_DEPTH: f64 = 40.0;

/// Blocs par arête de face
const FACE_BLOCKS: f64 = crate::edits::FACE_BLOCKS as f64;
/// Rayon de la planète en blocs (projection équiangulaire : une face couvre un quart de tour)
const BLOCK_RADIUS: f64 = FACE_BLOCKS / std::f64::consts::FRAC_PI_2;

/// Taille des méandres des tunnels, en blocs
const TUNNEL_SCALE: f64 = 52.0;
/// Rayon des tunnels dans l'espace du bruit (environ 2 à 3 blocs)
const TUNNEL_RADIUS: f64 = 0.08;
/// Taille des régions avec ou sans réseau de tunnels, en blocs
const TUNNEL_REGION_SCALE: f64 = 220.0;
/// Taille des cavernes, en blocs
const CAVERN_SCALE: f64 = 44.0;
/// Seuil du bruit au-dessus duquel une caverne s'ouvre
const CAVERN_THRESHOLD: f64 = 0.40;
/// Les cavernes s'arrêtent à cette profondeur sous la surface (pas de cratère à ciel ouvert)
const CAVERN_MIN_DEPTH: f64 = 10.0;
/// Les colonnes basses (plages, côtes) gardent ce toit intact au-dessus des grottes
const LOW_COLUMN_ROOF: f64 = 5.0;
/// Les grottes rétrécissent sur cette hauteur au-dessus du fond de la bande
const DEPTH_FADE: f64 = 10.0;
/// Les tunnels rétrécissent sur cette profondeur sous la surface (sinon ils l'entaillent en tranchées)
const SURFACE_FADE: f64 = 7.0;
/// Densité de tunnels au-delà de laquelle ils débouchent quand même à la surface : les entrées
const ENTRANCE_REGION: f64 = 0.32;
/// Bruit des cavernes au-delà duquel les tunnels débouchent aussi : des entrées éparpillées partout
const ENTRANCE_SPOT: f64 = 0.15;
/// Élargissement des tunnels à leur sortie en surface (0,6 : bouche 60 % plus large)
const ENTRANCE_FUNNEL: f64 = 0.6;

/// Valeurs du champ en un point du réseau : deux bruits dont l'intersection des zéros trace les
/// tunnels, la densité des régions de tunnels et le bruit des cavernes
type CavePoint = [f32; 4];

/// Champ de grottes précalculé sur une boîte de blocs d'une face
pub struct CaveField {
    face: PlanetFace,
    /// Indice (x, y, z) du premier point du réseau
    origin: (i32, i32, i32),
    /// Nombre de points par axe
    size: (i32, i32, i32),
    points: Vec<CavePoint>,
    seed: u64,
}

impl CaveField {
    /// Champ couvrant les blocs `x` × `y` × `z` d'une face (coordonnées de blocs de la face)
    pub fn new(seed: u64, face: PlanetFace, x: Range<i32>, y: Range<i32>, z: Range<i32>) -> Self {
        herocraft_common::prof_scope!("world::caves::field");
        let lattice = |r: &Range<i32>, cell: i32| (r.start.div_euclid(cell), (r.end - 1).div_euclid(cell) + 2);
        let (x0, x1) = lattice(&x, CAVE_CELL);
        let (y0, y1) = lattice(&y, CAVE_CELL_Y);
        let (z0, z1) = lattice(&z, CAVE_CELL);
        let size = (x1 - x0, y1 - y0, z1 - z0);
        let mut points = Vec::with_capacity((size.0 * size.1 * size.2).max(0) as usize);
        for lz in z0..z1 {
            for lx in x0..x1 {
                let dir = lattice_direction(face, lx, lz);
                for ly in y0..y1 {
                    points.push(cave_point(seed, dir, ly));
                }
            }
        }
        Self { face, origin: (x0, y0, z0), size, points, seed }
    }

    /// Point du réseau, pris dans la boîte précalculée ou, à défaut, calculé
    #[inline]
    fn point(&self, lx: i32, ly: i32, lz: i32) -> CavePoint {
        let (x, y, z) = (lx - self.origin.0, ly - self.origin.1, lz - self.origin.2);
        if x < 0 || y < 0 || z < 0 || x >= self.size.0 || y >= self.size.1 || z >= self.size.2 {
            return cave_point(self.seed, lattice_direction(self.face, lx, lz), ly);
        }
        self.points[((z * self.size.0 + x) * self.size.1 + y) as usize]
    }

    /// Le bloc (x, y, z) de la face est creusé
    pub fn carved(&self, column: &ColumnSample, x: i32, y: i32, z: i32) -> bool {
        let Some(depth) = carvable_depth(column, y) else {
            return false;
        };
        let ly = y.div_euclid(CAVE_CELL_Y);
        let below = column_point(|lx, ly, lz| self.point(lx, ly, lz), x, ly, z);
        let above = column_point(|lx, ly, lz| self.point(lx, ly, lz), x, ly + 1, z);
        carved_between(&below, &above, y, depth)
    }

    /// Appelle `f` pour chaque bloc creusé de la colonne (x, z) entre les altitudes `ys` : le champ
    /// n'est interpolé en x et z qu'une fois par point du réseau de la colonne
    pub(crate) fn for_each_carved(&self, column: &ColumnSample, x: i32, z: i32, ys: Range<i32>, mut f: impl FnMut(i32)) {
        let range = carvable_range(column);
        let (start, end) = (ys.start.max(range.start), ys.end.min(range.end));
        let top = column.elevation.floor();
        let mut cached = (i32::MIN, [0.0; 4], [0.0; 4]);
        for y in start..end {
            let ly = y.div_euclid(CAVE_CELL_Y);
            if cached.0 != ly {
                let below = if cached.0 == ly - 1 { cached.2 } else { column_point(|a, b, c| self.point(a, b, c), x, ly, z) };
                cached = (ly, below, column_point(|a, b, c| self.point(a, b, c), x, ly + 1, z));
            }
            if carved_between(&cached.1, &cached.2, y, top - y as f64) {
                f(y);
            }
        }
    }
}

/// Le bloc (x, y, z) d'une face est creusé, calculé pour lui seul (8 points du réseau)
pub(crate) fn block_carved(seed: u64, face: PlanetFace, column: &ColumnSample, x: i32, y: i32, z: i32) -> bool {
    let Some(depth) = carvable_depth(column, y) else {
        return false;
    };
    let point = |lx: i32, ly: i32, lz: i32| cave_point(seed, lattice_direction(face, lx, lz), ly);
    let ly = y.div_euclid(CAVE_CELL_Y);
    carved_between(&column_point(point, x, ly, z), &column_point(point, x, ly + 1, z), y, depth)
}

/// Altitudes `[bas, haut)` où une colonne peut avoir des grottes (vide si aucune)
pub fn carvable_range(column: &ColumnSample) -> Range<i32> {
    // Pas de grottes sous la mer : invisibles et inaccessibles, elles ne coûteraient que du maillage
    if column.elevation <= SEA_LEVEL_BLOCKS + 1.0 {
        return 0..0;
    }
    let top = column.elevation.floor();
    let roof = if column.elevation < SEA_LEVEL_BLOCKS + 6.0 { top - LOW_COLUMN_ROOF } else { top };
    let bottom = (top - CAVE_DEPTH).max(CAVE_FLOOR as f64);
    bottom as i32..(roof as i32).max(bottom as i32)
}

/// Profondeur sous la surface d'un bloc qui peut être creusé, `None` s'il doit rester plein
#[inline]
fn carvable_depth(column: &ColumnSample, y: i32) -> Option<f64> {
    if carvable_range(column).contains(&y) {
        Some(column.elevation.floor() - y as f64)
    } else {
        None
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Position d'un bloc dans sa maille du réseau (centre du bloc)
#[inline]
fn cell_fraction(t: i32, cell: i32) -> f32 {
    (t.rem_euclid(cell) as f32 + 0.5) / cell as f32
}

/// Champ au niveau `ly` du réseau, interpolé en x puis z à la colonne (x, z). Le chunk entier et le
/// bloc seul passent tous deux par ici, dans le même ordre : leurs résultats sont identiques au bit près.
#[inline]
fn column_point(point: impl Fn(i32, i32, i32) -> CavePoint, x: i32, ly: i32, z: i32) -> CavePoint {
    let (lx, lz) = (x.div_euclid(CAVE_CELL), z.div_euclid(CAVE_CELL));
    let (fx, fz) = (cell_fraction(x, CAVE_CELL), cell_fraction(z, CAVE_CELL));
    let (p00, p10, p01, p11) = (point(lx, ly, lz), point(lx + 1, ly, lz), point(lx, ly, lz + 1), point(lx + 1, ly, lz + 1));
    std::array::from_fn(|k| lerp(lerp(p00[k], p10[k], fx), lerp(p01[k], p11[k], fx), fz))
}

/// Interpolation en altitude entre deux niveaux du réseau, puis décision tunnel / caverne
#[inline]
fn carved_between(below: &CavePoint, above: &CavePoint, y: i32, depth: f64) -> bool {
    let fy = cell_fraction(y, CAVE_CELL_Y);
    let a = lerp(below[0], above[0], fy) as f64;
    let b = lerp(below[1], above[1], fy) as f64;
    let cavern = lerp(below[3], above[3], fy) as f64;
    let tunnel_distance = a * a + b * b;
    let cavern_possible = depth > CAVERN_MIN_DEPTH && cavern > CAVERN_THRESHOLD;
    // Presque partout, ni tunnel ni caverne possibles : la suite (fondus) est sautée. Le rayon
    // maximal compte l'évasement des entrées.
    const MAX_RADIUS: f64 = TUNNEL_RADIUS * (1.0 + ENTRANCE_FUNNEL);
    if tunnel_distance >= MAX_RADIUS * MAX_RADIUS && !cavern_possible {
        return false;
    }
    let region = lerp(below[2], above[2], fy) as f64;

    // Les grottes se referment en approchant du fond de la bande (pas de sol plat à la limite)
    let fade = smoothstep(0.0, DEPTH_FADE, CAVE_DEPTH - depth);
    // Près de la surface aussi, sauf aux entrées : les régions les plus creusées et les points où une
    // caverne se forme plus bas (sommets du bruit des cavernes), réguliers sur tout le relief
    let shallow = smoothstep(1.0, SURFACE_FADE, depth);
    let cavern_spot = smoothstep(ENTRANCE_SPOT - 0.08, ENTRANCE_SPOT, cavern);
    let entrance = smoothstep(ENTRANCE_REGION - 0.08, ENTRANCE_REGION, region).max(cavern_spot);
    // Aux entrées, le tunnel s'évase en approchant de la surface : une bouche visible de loin
    let surface = shallow.max(entrance) * (1.0 + ENTRANCE_FUNNEL * entrance * (1.0 - shallow));

    // Tunnels : là où les deux bruits sont proches de zéro à la fois, un tube sinueux
    let radius = TUNNEL_RADIUS * smoothstep(-0.25, 0.15, region) * fade * surface;
    if tunnel_distance < radius * radius {
        return true;
    }
    // Cavernes : les sommets d'un troisième bruit, loin de la surface
    cavern_possible && cavern > CAVERN_THRESHOLD + (1.0 - fade) * 0.3
}

/// Direction sur la sphère d'un point du réseau (coin de bloc multiple de `CAVE_CELL`)
#[inline]
fn lattice_direction(face: PlanetFace, lx: i32, lz: i32) -> DVec3 {
    let span = 2.0 / FACE_BLOCKS;
    face.uv_to_sphere_direction(-1.0 + (lx * CAVE_CELL) as f64 * span, -1.0 + (lz * CAVE_CELL) as f64 * span)
}

/// Bruits d'un point du réseau, à partir de sa position 3D en blocs : les points communs à deux
/// faces (sur leur arête) ont donc les mêmes valeurs
fn cave_point(seed: u64, dir: DVec3, ly: i32) -> CavePoint {
    let p = dir * (BLOCK_RADIUS + (ly * CAVE_CELL_Y) as f64);
    let t = p / TUNNEL_SCALE;
    // Le second bruit est décalé et un peu plus fin : sur un même réseau, les deux s'annuleraient
    // ensemble le long de certaines arêtes, en tunnels parfaitement droits
    [
        noise3(t + DVec3::new(0.31, 0.57, 0.83), seed.wrapping_add(0xCA7E_0001)) as f32,
        noise3(t * 1.13 + DVec3::new(0.71, 0.19, 0.47), seed.wrapping_add(0xCA7E_0002)) as f32,
        noise3(p / TUNNEL_REGION_SCALE, seed.wrapping_add(0xCA7E_0003)) as f32,
        (noise3(p / CAVERN_SCALE, seed.wrapping_add(0xCA7E_0004)) * 0.75
            + noise3(p / (CAVERN_SCALE * 0.4), seed.wrapping_add(0xCA7E_0005)) * 0.25) as f32,
    ]
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bruit de gradient 3D (Perlin amélioré, 12 gradients sur les arêtes du cube), dans [-1, 1] :
/// sans trigonométrie, il coûte bien moins cher que `gradient_noise_3d` pour un champ dense
fn noise3(p: DVec3, seed: u64) -> f64 {
    let (x0, y0, z0) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (fx, fy, fz) = (p.x - x0, p.y - y0, p.z - z0);
    let (ix, iy, iz) = (x0 as i64, y0 as i64, z0 as i64);
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));

    let grad = |dx: i64, dy: i64, dz: i64| -> f64 {
        // Mélange complet (finaliseur de splitmix64) : un hachage plus faible laisse des alignements
        // visibles, des tunnels rectilignes le long des plans du réseau
        let mut h = ((ix + dx) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ ((iy + dy) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ ((iz + dz) as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
            ^ seed;
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        let h = h >> 40;
        let (x, y, z) = (fx - dx as f64, fy - dy as f64, fz - dz as f64);
        match h % 12 {
            0 => x + y,
            1 => -x + y,
            2 => x - y,
            3 => -x - y,
            4 => x + z,
            5 => -x + z,
            6 => x - z,
            7 => -x - z,
            8 => y + z,
            9 => -y + z,
            10 => y - z,
            _ => -y - z,
        }
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(grad(0, 0, 0), grad(1, 0, 0), u);
    let x10 = lerp(grad(0, 1, 0), grad(1, 1, 0), u);
    let x01 = lerp(grad(0, 0, 1), grad(1, 0, 1), u);
    let x11 = lerp(grad(0, 1, 1), grad(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::generator::PlanetWorldGenerator;

    #[test]
    fn field_and_single_block_agree() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());
        // Un chunk de la face -Z (graine 42) percé de tunnels entre les altitudes 60 et 200
        let face = PlanetFace::NegZ;
        let center = crate::edits::FACE_BLOCKS / 2;
        let (x0, z0) = (0..64)
            .flat_map(|i| (0..64).map(move |j| (center + (i - 32) * 32, center + (j - 32) * 32)))
            .find(|&(x, z)| {
                let column = generator.sample_block_column(face, x + 16, z + 16);
                (60..200).any(|y| (y as f64) < column.elevation && block_carved(generator.planet.seed, face, &column, x + 16, y, z + 16))
            })
            .expect("un chunk percé de grottes");
        let field = CaveField::new(generator.planet.seed, face, x0 - 1..x0 + 33, 60..200, z0 - 1..z0 + 33);
        let mut carved = 0;
        for z in z0 - 1..z0 + 33 {
            for x in x0 - 1..x0 + 33 {
                let column = generator.sample_block_column(face, x, z);
                let mut listed = Vec::new();
                field.for_each_carved(&column, x, z, 60..200, |y| listed.push(y));
                for y in 60..200 {
                    let a = field.carved(&column, x, y, z);
                    assert_eq!(a, listed.contains(&y), "({x}, {y}, {z})");
                    if (x + y + z) % 5 == 0 {
                        assert_eq!(a, block_carved(generator.planet.seed, face, &column, x, y, z), "({x}, {y}, {z})");
                    }
                    carved += a as usize;
                }
            }
        }
        assert!(carved > 0, "la zone testée doit contenir des grottes");
    }

    #[test]
    fn noise_stays_in_range() {
        let mut max: f64 = 0.0;
        for i in 0..20000 {
            let p = DVec3::new(i as f64 * 0.137, (i % 97) as f64 * 0.31, (i % 13) as f64 * 1.7);
            max = max.max(noise3(p, 7).abs());
        }
        assert!(max <= 1.0 && max > 0.4, "{max}");
    }
}

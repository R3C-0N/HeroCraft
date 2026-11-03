use glam::{DVec3, Vec3};
use herocraft_common::PlanetFace;

/// Sommet de boîte englobante pour le raymarching SDF analytique des nuages.
/// Chaque groupe de nuages est décrit par un masque d'occupation 11x11 (121 bits) dans son repère local.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudVertex {
    pub position: [f32; 3],     // Position 3D du sommet dans le monde
    pub _pad0: f32,
    pub frame_u: [f32; 3],      // Tangente locale U (alignée sur l'axe u de la face)
    pub _pad1: f32,
    pub frame_v: [f32; 3],      // Tangente locale V (alignée sur l'axe v de la face)
    pub _pad2: f32,
    pub frame_w: [f32; 3],      // Normale locale W (Up/Radiale)
    pub _pad3: f32,
    pub cloud_center: [f32; 3], // Centre du nuage dans l'espace planétaire
    pub _pad4: f32,
    pub bbox: [f32; 4],         // Emprise locale des cellules occupées [min_x, min_y, max_x, max_y]
    pub mask: [u32; 4],         // Masque 11x11 sur 128 bits (mot 0 = bits 0..31), bit = y * 11 + x
    pub config: [f32; 4],       // [taille de cellule, half_depth, id de secteur, 0.0]
}

impl CloudVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CloudVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3, // position
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3, // frame_u
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3, // frame_v
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x3, // frame_w
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x3, // cloud_center
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4, // bbox
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Uint32x4, // mask
                },
                wgpu::VertexAttribute {
                    offset: 112,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4, // config
                },
            ],
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct CloudMesh {
    pub vertices: Vec<CloudVertex>,
    pub indices: Vec<u32>,
}

/// Masque d'occupation 11x11 d'un groupe de nuages (bit = y * 11 + x)
pub type CloudMask = u128;

/// Générateur des nuages : boîtes englobantes contenant chacune un masque 11x11 de cellules,
/// rendu par sphere tracing d'un SDF arrondi (coussin) dans le shader.
pub struct CloudMeshGenerator;

impl CloudMeshGenerator {
    /// Rayon inférieur de la couche nuageuse (au-dessus du niveau de la mer et des collines)
    pub const CLOUD_ALTITUDE_BOT: f32 = 8.2;
    /// Rayon supérieur de la couche nuageuse
    pub const CLOUD_ALTITUDE_TOP: f32 = 9.4;

    /// Demi-épaisseur des nuages (dalle de 1.2)
    pub const CLOUD_HALF_DEPTH: f32 = 0.60;

    /// Nombre de secteurs par côté de face (au plus un groupe de nuages par secteur)
    /// 3 sur le prototype de 1 024 blocs par face. Sur une planète plus grande, la couche de nuages
    /// (à altitude fixe en blocs) est relativement plus proche du sol et les secteurs y rétrécissent :
    /// 3/4 de secteurs par bloc de face en plus gardent l'écart du prototype entre deux nuages
    /// (330 blocs environ). Vers les coins des faces, où les secteurs se resserrent encore, les
    /// nuages sont tirés plus petits (`sector_max_radius`) : deux nuages ne se touchent jamais.
    pub const SECTORS_PER_FACE: usize =
        if herocraft_world::FACE_SCALE <= 1.0 { 3 } else { (2.25 * herocraft_world::FACE_SCALE) as usize };
    /// Côté de la grille d'occupation d'un nuage
    pub const GRID: usize = 11;
    /// Taille d'une cellule de nuage
    pub const CELL: f32 = 1.0;
    /// Rayon maximal (coin de l'emprise) d'un groupe de nuages, pour l'espacement entre secteurs
    pub const MAX_RADIUS: f32 = 7.3;

    pub fn generate_cloud_mesh(base_radius: f32, seed: u64) -> CloudMesh {
        herocraft_common::prof_scope!("clouds::generate_cloud_mesh");
        // Altitude et taille des nuages mesurées en blocs
        let scale = herocraft_world::block_scale(base_radius as f64) as f32;
        let r_mid = base_radius + (Self::CLOUD_ALTITUDE_BOT + Self::CLOUD_ALTITUDE_TOP) * 0.5 * scale;
        let mut mesh = CloudMesh::default();

        for &face in &PlanetFace::ALL {
            Self::generate_face_clouds(&mut mesh, face, r_mid, scale, seed);
        }

        mesh
    }

    fn generate_face_clouds(mesh: &mut CloudMesh, face: PlanetFace, r_mid: f32, scale: f32, seed: u64) {
        let face_id = face as u64;
        let sectors = Self::SECTORS_PER_FACE;

        for sy in 0..sectors {
            for sx in 0..sectors {
                let sector_hash = Self::hash_sector(seed, face_id, sx as u64, sy as u64);
                // ~80% de chances d'avoir une formation nuageuse dans ce secteur
                if sector_hash % 100 >= 80 {
                    continue;
                }
                let mut rng = sector_hash;

                // Rayon maximal d'un groupe pour ne jamais toucher les secteurs voisins
                let max_radius = Self::sector_max_radius(face, sx, sy, r_mid, scale);
                let mut mask = Self::random_mask(&mut rng);
                for _ in 0..6 {
                    if Self::mask_radius(mask) <= max_radius {
                        break;
                    }
                    mask = Self::random_mask(&mut rng);
                }
                if mask == 0 || Self::mask_radius(mask) > max_radius {
                    continue;
                }

                // Centre du secteur dans [-0.9, 0.9] avec une légère gigue
                // (gigue réglée pour 3 secteurs par face, réduite d'autant pour des secteurs plus petits)
                let jitter = 0.006 * 3.0 / sectors as f64;
                let jitter_u = ((Self::next_rand(&mut rng) % 7) as f64 - 3.0) * jitter;
                let jitter_v = ((Self::next_rand(&mut rng) % 7) as f64 - 3.0) * jitter;
                let (u_c, v_c) = Self::sector_uv(sx, sy);
                let (u_c, v_c) = (u_c + jitter_u, v_c + jitter_v);

                // Repère local aligné sur les axes (u, v) de la face : tous les nuages d'une face
                // partagent la même orientation de grille, comme une couche nuageuse continue
                let w = Self::dir(face, u_c, v_c);
                let du = Self::dir(face, u_c + 1e-3, v_c) - Self::dir(face, u_c - 1e-3, v_c);
                let u = (du - w * du.dot(w)).normalize();
                let v = w.cross(u).normalize();
                let cloud_center = w * r_mid;

                let cell_size = Self::CELL * scale;
                let bbox = Self::mask_bbox_scaled(mask, cell_size);
                let pad_xy = 0.65_f32 * scale;
                let (min_x, min_y, max_x, max_y) = (bbox[0] - pad_xy, bbox[1] - pad_xy, bbox[2] + pad_xy, bbox[3] + pad_xy);

                let half_depth = Self::CLOUD_HALF_DEPTH * scale;
                let pad_z = 0.50_f32 * scale;
                // Le shader courbe la dalle pour suivre la sphère : les bords descendent de rho² / 2R
                let rho_max = Self::mask_radius(mask) * scale + pad_xy;
                let sag = rho_max * rho_max / (2.0 * r_mid);
                let min_z = -half_depth - pad_z - sag;
                let max_z = half_depth + pad_z;

                let mask_words = [mask as u32, (mask >> 32) as u32, (mask >> 64) as u32, (mask >> 96) as u32];
                let config = [cell_size, half_depth, (sx * sectors + sy) as f32, 0.0];

                let base_idx = mesh.vertices.len() as u32;

                // 8 coins de la boîte englobante (bit 0 = x, bit 1 = y, bit 2 = z)
                for &iz in &[min_z, max_z] {
                    for &iy in &[min_y, max_y] {
                        for &ix in &[min_x, max_x] {
                            let pos = cloud_center + u * ix + v * iy + w * iz;
                            mesh.vertices.push(CloudVertex {
                                position: pos.to_array(),
                                _pad0: 0.0,
                                frame_u: u.to_array(),
                                _pad1: 0.0,
                                frame_v: v.to_array(),
                                _pad2: 0.0,
                                frame_w: w.to_array(),
                                _pad3: 0.0,
                                cloud_center: cloud_center.to_array(),
                                _pad4: 0.0,
                                bbox,
                                mask: mask_words,
                                config,
                            });
                        }
                    }
                }

                #[rustfmt::skip]
                let box_indices = [
                    0, 2, 3,  0, 3, 1, // Bas (-Z)
                    4, 5, 7,  4, 7, 6, // Haut (+Z)
                    0, 1, 5,  0, 5, 4, // Sud (-Y)
                    2, 6, 7,  2, 7, 3, // Nord (+Y)
                    0, 4, 6,  0, 6, 2, // Ouest (-X)
                    1, 3, 7,  1, 7, 5, // Est (+X)
                ];

                for &idx in &box_indices {
                    mesh.indices.push(base_idx + idx);
                }
            }
        }
    }

    /// Centre (sans gigue) du secteur (sx, sy) d'une face, en coordonnées (u, v)
    fn sector_uv(sx: usize, sy: usize) -> (f64, f64) {
        let sectors = Self::SECTORS_PER_FACE as f64;
        (((sx as f64 + 0.5) / sectors * 2.0 - 1.0) * 0.90, ((sy as f64 + 0.5) / sectors * 2.0 - 1.0) * 0.90)
    }

    /// Plus grand rayon (en cellules) d'un nuage du secteur (sx, sy) : `MAX_RADIUS`, réduit là où
    /// les secteurs voisins de la face sont plus proches que deux fois ce rayon (moins la gigue)
    fn sector_max_radius(face: PlanetFace, sx: usize, sy: usize, r_mid: f32, scale: f32) -> f32 {
        let n = Self::SECTORS_PER_FACE as i64;
        let center = |sx: usize, sy: usize| {
            let (u, v) = Self::sector_uv(sx, sy);
            Self::dir(face, u, v) * r_mid
        };
        let here = center(sx, sy);
        let nearest = (-1..=1i64)
            .flat_map(|dx| (-1..=1i64).map(move |dy| (dx, dy)))
            .filter(|&d| d != (0, 0))
            .map(|(dx, dy)| (sx as i64 + dx, sy as i64 + dy))
            .filter(|&(x, y)| (0..n).contains(&x) && (0..n).contains(&y))
            .map(|(x, y)| center(x as usize, y as usize).distance(here))
            .fold(f32::INFINITY, f32::min);
        // Gigue d'au plus 3 pas de 0,018 / secteurs en u et en v, pour chacun des deux nuages
        let jitter = 2.0 * 3.0 * 0.018 / n as f32 * std::f32::consts::FRAC_PI_4 * r_mid * std::f32::consts::SQRT_2;
        let cell = Self::CELL * scale;
        Self::MAX_RADIUS.min((nearest - jitter) / (2.0 * cell))
    }

    fn dir(face: PlanetFace, u: f64, v: f64) -> Vec3 {
        let d: DVec3 = face.uv_to_sphere_direction(u, v).normalize();
        d.as_vec3()
    }

    #[inline]
    pub fn mask_get(mask: CloudMask, x: i32, y: i32) -> bool {
        let g = Self::GRID as i32;
        x >= 0 && y >= 0 && x < g && y < g && (mask >> (y * g + x)) & 1 == 1
    }

    #[inline]
    fn mask_set(mask: &mut CloudMask, x: i32, y: i32, on: bool) {
        let bit = 1u128 << (y * Self::GRID as i32 + x);
        if on {
            *mask |= bit;
        } else {
            *mask &= !bit;
        }
    }

    /// Distance du centre au coin le plus éloigné de l'emprise
    pub fn mask_radius(mask: CloudMask) -> f32 {
        let b = Self::mask_bbox(mask);
        (b[0].abs().max(b[2].abs()).powi(2) + b[1].abs().max(b[3].abs()).powi(2)).sqrt()
    }

    /// Emprise locale (repère centré sur la grille) des cellules occupées à une échelle donnée
    fn mask_bbox_scaled(mask: CloudMask, cell_size: f32) -> [f32; 4] {
        let g = Self::GRID as i32;
        let (mut x0, mut y0, mut x1, mut y1) = (g, g, -1, -1);
        for y in 0..g {
            for x in 0..g {
                if Self::mask_get(mask, x, y) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let half = g as f32 * 0.5;
        [
            (x0 as f32 - half) * cell_size,
            (y0 as f32 - half) * cell_size,
            (x1 as f32 + 1.0 - half) * cell_size,
            (y1 as f32 + 1.0 - half) * cell_size,
        ]
    }

    /// Emprise locale (repère centré sur la grille) des cellules occupées
    fn mask_bbox(mask: CloudMask) -> [f32; 4] {
        Self::mask_bbox_scaled(mask, Self::CELL)
    }

    /// Tire un masque 11x11 : un bruit de valeur lissé, modulé par une enveloppe elliptique
    /// de taille aléatoire (petits, moyens et grands nuages), puis nettoyé :
    /// pas de cellules isolées, pas de filaments d'une cellule, pas de contact uniquement diagonal.
    pub fn random_mask(rng: &mut u64) -> CloudMask {
        let g = Self::GRID as i32;

        // Enveloppe : union de 1 à 4 lobes elliptiques ; la taille globale varie
        // (~35% petits, ~40% moyens, ~25% grands groupes)
        let size_roll = Self::next_rand(rng) % 100;
        let (lobes, spread, lobe_r) = if size_roll < 35 {
            (1 + (Self::next_rand(rng) % 2) as usize, 1.0, 1.4)
        } else if size_roll < 75 {
            (2 + (Self::next_rand(rng) % 2) as usize, 2.6, 2.5)
        } else {
            (3 + (Self::next_rand(rng) % 2) as usize, 3.3, 2.9)
        };
        let rand01 = |rng: &mut u64| (Self::next_rand(rng) % 1000) as f32 / 1000.0;
        let mut ellipses = Vec::with_capacity(lobes);
        for _ in 0..lobes {
            let ex = 5.5 + (rand01(rng) * 2.0 - 1.0) * spread;
            let ey = 5.5 + (rand01(rng) * 2.0 - 1.0) * spread;
            let aspect = 0.6 + rand01(rng) * 0.8;
            let r = lobe_r * (0.75 + rand01(rng) * 0.5);
            ellipses.push((ex, ey, r * aspect, r / aspect));
        }

        // Bruit de valeur sur un réseau de 2.5 cellules : découpe les bords (encoches, bras, trous)
        let lattice: Vec<f32> = (0..36).map(|_| rand01(rng)).collect();
        let noise = |x: f32, y: f32| {
            let (fx, fy) = (x / 2.5, y / 2.5);
            let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
            let at = |i: usize, j: usize| lattice[j.min(5) * 6 + i.min(5)];
            let a = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * sx;
            let b = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * sx;
            a + (b - a) * sy
        };

        let mut mask: CloudMask = 0;
        for y in 0..g {
            for x in 0..g {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let env = ellipses
                    .iter()
                    .map(|&(ex, ey, rx, ry)| 1.0 - ((px - ex) / rx).powi(2) - ((py - ey) / ry).powi(2))
                    .fold(f32::MIN, f32::max);
                let value = env + (noise(px, py) - 0.5) * 1.1;
                if value > -0.05 {
                    Self::mask_set(&mut mask, x, y, true);
                }
            }
        }

        let mask = Self::clean_mask(mask);
        if mask.count_ones() < 2 {
            // Repli : petit nuage de 2x2 au centre
            let mut m = 0;
            for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
                Self::mask_set(&mut m, x, y, true);
            }
            return m;
        }
        mask
    }

    /// Ouverture 2x2 (une cellule n'est gardée que si elle appartient à un bloc 2x2 plein)
    /// puis comblement des contacts uniquement diagonaux
    pub fn clean_mask(raw: CloudMask) -> CloudMask {
        let g = Self::GRID as i32;
        let block = |x: i32, y: i32| {
            Self::mask_get(raw, x, y) && Self::mask_get(raw, x + 1, y) && Self::mask_get(raw, x, y + 1) && Self::mask_get(raw, x + 1, y + 1)
        };
        let mut mask: CloudMask = 0;
        for y in 0..g {
            for x in 0..g {
                let keep = Self::mask_get(raw, x, y) && (block(x, y) || block(x - 1, y) || block(x, y - 1) || block(x - 1, y - 1));
                Self::mask_set(&mut mask, x, y, keep);
            }
        }

        loop {
            let mut changed = false;
            for y in 0..g - 1 {
                for x in 0..g - 1 {
                    let a = Self::mask_get(mask, x, y);
                    let b = Self::mask_get(mask, x + 1, y);
                    let c = Self::mask_get(mask, x, y + 1);
                    let d = Self::mask_get(mask, x + 1, y + 1);
                    if a && d && !b && !c {
                        Self::mask_set(&mut mask, x + 1, y, true);
                        changed = true;
                    } else if b && c && !a && !d {
                        Self::mask_set(&mut mask, x, y, true);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        mask
    }

    /// Générateur pseudo-aléatoire SplitMix64 (déterministe par secteur)
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn hash_sector(seed: u64, face: u64, sx: u64, sy: u64) -> u64 {
        let mut h = seed ^ (face.wrapping_mul(0x9E3779B97F4A7C15));
        h = h.wrapping_add(sx.wrapping_mul(0xBF58476D1CE4E5B9));
        h = h ^ (h >> 30);
        h = h.wrapping_add(sy.wrapping_mul(0x94D049BB133111EB));
        h = h ^ (h >> 27);
        h = h.wrapping_mul(0x45D9F3B);
        h ^ (h >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rayon de la planète par défaut et échelle en blocs
    const R: f32 = (herocraft_world::REFERENCE_BASE_RADIUS * herocraft_world::DEFAULT_PLANET_SCALE) as f32;
    const S: f32 = herocraft_world::BLOCK_SCALE as f32;

    #[test]
    fn test_cloud_mesh_bounded_clusters_and_reasonable_sizes() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(R, 2026);

        assert!(!mesh.vertices.is_empty(), "Le maillage de nuages ne doit pas être vide");
        assert_eq!(mesh.indices.len() % 3, 0, "Les indices doivent former des triangles complets");

        // Boîtes englobantes : épaisseur + marges + courbure de la dalle
        let r_bot = R + (CloudMeshGenerator::CLOUD_ALTITUDE_BOT - 2.0) * S;
        let r_top = R + (CloudMeshGenerator::CLOUD_ALTITUDE_TOP + 1.5) * S;
        for v in &mesh.vertices {
            let r = Vec3::from(v.position).length();
            assert!(r >= r_bot && r <= r_top, "Sommet hors de la couche nuageuse : r = {r}");
        }

        assert_eq!(mesh.vertices.len() % 8, 0);
        assert_eq!(mesh.indices.len() % 36, 0);
    }

    #[test]
    fn test_cloud_vertex_layout_and_frames() {
        assert_eq!(std::mem::size_of::<CloudVertex>(), 128);
        let mesh = CloudMeshGenerator::generate_cloud_mesh(R, 2026);
        for v in &mesh.vertices {
            let u = Vec3::from(v.frame_u);
            let v_dir = Vec3::from(v.frame_v);
            let w = Vec3::from(v.frame_w);
            assert!((u.length() - 1.0).abs() < 1e-4 && (v_dir.length() - 1.0).abs() < 1e-4 && (w.length() - 1.0).abs() < 1e-4);
            assert!(u.dot(v_dir).abs() < 1e-4 && u.dot(w).abs() < 1e-4 && v_dir.dot(w).abs() < 1e-4);
            assert_eq!(v.config[1], CloudMeshGenerator::CLOUD_HALF_DEPTH * S);
            assert!(v.mask.iter().any(|&w| w != 0), "Chaque nuage doit avoir au moins une cellule");
            assert!(v.bbox[2] > v.bbox[0] && v.bbox[3] > v.bbox[1]);
        }
    }

    #[test]
    fn test_masks_clean_varied_and_spaced() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(R, 2026);
        let clouds: Vec<&CloudVertex> = mesh.vertices.iter().step_by(8).collect();
        assert!(clouds.len() >= 6 * 6, "Le ciel doit compter assez de nuages (obtenu {})", clouds.len());

        let mut sizes = std::collections::BTreeSet::new();
        let mut radii = Vec::new();
        for c in &clouds {
            let mask = c.mask.iter().enumerate().fold(0u128, |m, (i, &w)| m | ((w as u128) << (32 * i)));
            assert_eq!(CloudMeshGenerator::clean_mask(mask), mask, "Le masque doit déjà être nettoyé");
            // Pas de contact uniquement diagonal
            for y in 0..10 {
                for x in 0..10 {
                    let g = |dx, dy| CloudMeshGenerator::mask_get(mask, x + dx, y + dy);
                    assert!(!(g(0, 0) && g(1, 1) && !g(1, 0) && !g(0, 1)));
                    assert!(!(g(1, 0) && g(0, 1) && !g(0, 0) && !g(1, 1)));
                }
            }
            sizes.insert(mask.count_ones());
            radii.push(CloudMeshGenerator::mask_radius(mask));
        }
        assert!(sizes.len() >= 12, "Les tailles de nuages doivent varier (obtenu {sizes:?})");
        assert!(*sizes.iter().next().unwrap() <= 8, "Il doit y avoir de petits nuages");
        assert!(*sizes.iter().last().unwrap() >= 35, "Il doit y avoir de grands nuages");

        // Les groupes de nuages ne se chevauchent pas
        for i in 0..clouds.len() {
            for j in i + 1..clouds.len() {
                let d = (Vec3::from(clouds[i].cloud_center) - Vec3::from(clouds[j].cloud_center)).length();
                assert!(d > (radii[i] + radii[j]) * S, "Nuages {i} et {j} se chevauchent (d = {d})");
            }
        }
    }
}

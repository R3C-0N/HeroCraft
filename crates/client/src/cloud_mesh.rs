use glam::Vec3;
use herocraft_common::PlanetFace;

use crate::planet_mesh::{PlanetMesh, PlanetVertex};

/// Générateur de maillage 3D pour la couche nuageuse planétaire.
/// Contrairement à une simple projection 2D / shader céleste, les nuages sont ici des
/// objets 3D réels physiques dans le monde, avec des volumes cubiques délimités et traversables.
pub struct CloudMeshGenerator;

impl CloudMeshGenerator {
    /// Résolution de la grille sur chaque face du cubed-sphere (32x32)
    pub const GRID_SIZE: usize = 32;
    /// Taille d'un secteur de spawn (8x8 cellules) garantissant l'espacement et la taille bornée
    pub const SECTOR_SIZE: usize = 8;

    /// Rayon inférieur de la couche nuageuse (au-dessus du niveau de la mer et des collines)
    pub const CLOUD_ALTITUDE_BOT: f32 = 8.2;
    /// Rayon supérieur de la couche nuageuse (épaisseur fine de 1.2 bloc pour un ratio de dalle horizontale)
    pub const CLOUD_ALTITUDE_TOP: f32 = 9.4;

    /// Génère le maillage 3D complet de la couche nuageuse autour de la planète.
    /// Les nuages sont composés de grappes solides de taille raisonnable (strictement bornées par secteur)
    /// avec des faces supérieures, inférieures et des parois latérales nettes.
    pub fn generate_cloud_mesh(base_radius: f32, seed: u64) -> PlanetMesh {
        let r_bot = base_radius + Self::CLOUD_ALTITUDE_BOT;
        let r_top = base_radius + Self::CLOUD_ALTITUDE_TOP;

        let mut mesh = PlanetMesh::default();

        for &face in &PlanetFace::ALL {
            Self::generate_face_clouds(&mut mesh, face, r_bot, r_top, seed);
        }

        mesh
    }

    fn generate_face_clouds(
        mesh: &mut PlanetMesh,
        face: PlanetFace,
        r_bot: f32,
        r_top: f32,
        seed: u64,
    ) {
        let n = Self::GRID_SIZE;
        let mut occupied = [[false; Self::GRID_SIZE]; Self::GRID_SIZE];

        let sectors_per_dim = n / Self::SECTOR_SIZE;
        let face_id = face as u64;

        // 1. Détermination des amas de nuages bornés dans chaque secteur
        for sy in 0..sectors_per_dim {
            for sx in 0..sectors_per_dim {
                let sector_hash = Self::hash_sector(seed, face_id, sx as u64, sy as u64);
                // ~58% de chances d'avoir une formation nuageuse dans ce secteur (laisse de grands ciels bleus)
                if sector_hash % 100 < 58 {
                    // Centre borné au sein du secteur (évite les chevauchements infinis entre secteurs)
                    let cx = sx * Self::SECTOR_SIZE + 2 + ((sector_hash >> 4) % 4) as usize;
                    let cy = sy * Self::SECTOR_SIZE + 2 + ((sector_hash >> 8) % 4) as usize;

                    // Rayon strictement borné : entre 1 et 3 blocs
                    // La largeur totale d'un nuage ne dépassera JAMAIS 7 blocs !
                    let rx = 1 + ((sector_hash >> 12) % 3) as usize;
                    let ry = 1 + ((sector_hash >> 16) % 3) as usize;

                    let min_x = cx.saturating_sub(rx);
                    let max_x = (cx + rx).min(n - 1);
                    let min_y = cy.saturating_sub(ry);
                    let max_y = (cy + ry).min(n - 1);

                    for y in min_y..=max_y {
                        for x in min_x..=max_x {
                            let dx = x as f32 - cx as f32;
                            let dy = y as f32 - cy as f32;
                            let d_norm = (dx / (rx as f32 + 0.5)).powi(2)
                                + (dy / (ry as f32 + 0.5)).powi(2);
                            if d_norm <= 1.0 {
                                let cell_hash = Self::hash_cell(seed, face_id, x as u64, y as u64);
                                // Découpage en escalier pour un profil voxel naturel
                                if cell_hash % 100 < 80 {
                                    occupied[y][x] = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Génération des cellules de nuages avec biseaux 3D
        let r = Self::BEVEL_R;
        let n_f = n as f64;

        for y in 0..n {
            for x in 0..n {
                if !occupied[y][x] {
                    continue;
                }

                let x0 = x as f64;
                let x1 = (x + 1) as f64;
                let y0 = y as f64;
                let y1 = (y + 1) as f64;

                let has_w = x > 0 && occupied[y][x - 1];
                let has_e = x + 1 < n && occupied[y][x + 1];
                let has_s = y > 0 && occupied[y - 1][x];
                let has_n = y + 1 < n && occupied[y + 1][x];

                let ext_w = !has_w;
                let ext_e = !has_e;
                let ext_s = !has_s;
                let ext_n = !has_n;

                // Coins extérieurs convexes
                let conv_sw = ext_w && ext_s;
                let conv_se = ext_e && ext_s;
                let conv_ne = ext_e && ext_n;
                let conv_nw = ext_w && ext_n;

                // Construction de la liste ordonnée des sommets du polygone (CCW)
                let mut poly: Vec<[f64; 2]> = Vec::with_capacity(16);

                // Bord Sud
                if conv_sw {
                    poly.push([x0 + r, y0]);
                } else {
                    poly.push([x0, y0]);
                }
                if conv_se {
                    poly.push([x1 - r, y0]);
                    poly.push([x1 - 0.29289 * r, y0 + 0.29289 * r]);
                } else {
                    poly.push([x1, y0]);
                }

                // Bord Est
                if conv_se {
                    poly.push([x1, y0 + r]);
                }
                if conv_ne {
                    poly.push([x1, y1 - r]);
                    poly.push([x1 - 0.29289 * r, y1 - 0.29289 * r]);
                } else {
                    poly.push([x1, y1]);
                }

                // Bord Nord
                if conv_ne {
                    poly.push([x1 - r, y1]);
                }
                if conv_nw {
                    poly.push([x0 + r, y1]);
                    poly.push([x0 + 0.29289 * r, y1 - 0.29289 * r]);
                } else {
                    poly.push([x0, y1]);
                }

                // Bord Ouest
                if conv_nw {
                    poly.push([x0, y1 - r]);
                }
                if conv_sw {
                    poly.push([x0, y0 + r]);
                    poly.push([x0 + 0.29289 * r, y0 + 0.29289 * r]);
                }

                // Triangulation en éventail depuis le centre pour le dessus et le dessous
                let center = [(x0 + x1) * 0.5, (y0 + y1) * 0.5];
                let k = poly.len();
                for i in 0..k {
                    let next = (i + 1) % k;
                    Self::add_tri(mesh, face, center, poly[i], poly[next], r_bot, r_top, n_f);
                }

                // Parois extérieures et biseaux d'angle convexe multi-segments
                if ext_s {
                    let p_start = if conv_sw { [x0 + r, y0] } else { [x0, y0] };
                    let p_end = if conv_se { [x1 - r, y0] } else { [x1, y0] };
                    Self::add_wall(mesh, face, p_start, p_end, r_bot, r_top, n_f);
                }
                if conv_se {
                    let p_mid = [x1 - 0.29289 * r, y0 + 0.29289 * r];
                    Self::add_wall(mesh, face, [x1 - r, y0], p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, [x1, y0 + r], r_bot, r_top, n_f);
                }

                if ext_e {
                    let p_start = if conv_se { [x1, y0 + r] } else { [x1, y0] };
                    let p_end = if conv_ne { [x1, y1 - r] } else { [x1, y1] };
                    Self::add_wall(mesh, face, p_start, p_end, r_bot, r_top, n_f);
                }
                if conv_ne {
                    let p_mid = [x1 - 0.29289 * r, y1 - 0.29289 * r];
                    Self::add_wall(mesh, face, [x1, y1 - r], p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, [x1 - r, y1], r_bot, r_top, n_f);
                }

                if ext_n {
                    let p_start = if conv_ne { [x1 - r, y1] } else { [x1, y1] };
                    let p_end = if conv_nw { [x0 + r, y1] } else { [x0, y1] };
                    Self::add_wall(mesh, face, p_start, p_end, r_bot, r_top, n_f);
                }
                if conv_nw {
                    let p_mid = [x0 + 0.29289 * r, y1 - 0.29289 * r];
                    Self::add_wall(mesh, face, [x1 - r, y1], p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, [x0 + r, y1], r_bot, r_top, n_f);
                }

                if ext_w {
                    let p_start = if conv_nw { [x0, y1 - r] } else { [x0, y1] };
                    let p_end = if conv_sw { [x0, y0 + r] } else { [x0, y0] };
                    Self::add_wall(mesh, face, p_start, p_end, r_bot, r_top, n_f);
                }
                if conv_sw {
                    let p_mid = [x0 + 0.29289 * r, y0 + 0.29289 * r];
                    Self::add_wall(mesh, face, [x0, y1 - r], p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, [x0 + r, y0], r_bot, r_top, n_f);
                }
            }
        }

        // 3. Biseaux d'angle intérieur concave multi-segments (adoucissement des trous et coins intérieurs)
        for y in 0..=n {
            for x in 0..=n {
                let occ_sw = x > 0 && y > 0 && occupied[y - 1][x - 1];
                let occ_se = x < n && y > 0 && occupied[y - 1][x];
                let occ_nw = x > 0 && y < n && occupied[y][x - 1];
                let occ_ne = x < n && y < n && occupied[y][x];

                let pt = [x as f64, y as f64];

                // Coin intérieur tourné vers le Nord-Est (dans la cellule vide NE)
                if !occ_ne && occ_nw && occ_se {
                    let p_a = [pt[0] + r, pt[1]];
                    let p_mid = [pt[0] + 0.29289 * r, pt[1] + 0.29289 * r];
                    let p_b = [pt[0], pt[1] + r];
                    Self::add_wall(mesh, face, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, p_b, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_mid, p_b, r_bot, r_top, n_f);
                }
                // Coin intérieur tourné vers le Nord-Ouest (dans la cellule vide NW)
                if !occ_nw && occ_ne && occ_sw {
                    let p_a = [pt[0], pt[1] + r];
                    let p_mid = [pt[0] - 0.29289 * r, pt[1] + 0.29289 * r];
                    let p_b = [pt[0] - r, pt[1]];
                    Self::add_wall(mesh, face, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, p_b, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_mid, p_b, r_bot, r_top, n_f);
                }
                // Coin intérieur tourné vers le Sud-Ouest (dans la cellule vide SW)
                if !occ_sw && occ_nw && occ_se {
                    let p_a = [pt[0] - r, pt[1]];
                    let p_mid = [pt[0] - 0.29289 * r, pt[1] - 0.29289 * r];
                    let p_b = [pt[0], pt[1] - r];
                    Self::add_wall(mesh, face, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, p_b, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_mid, p_b, r_bot, r_top, n_f);
                }
                // Coin intérieur tourné vers le Sud-Est (dans la cellule vide SE)
                if !occ_se && occ_ne && occ_sw {
                    let p_a = [pt[0], pt[1] - r];
                    let p_mid = [pt[0] + 0.29289 * r, pt[1] - 0.29289 * r];
                    let p_b = [pt[0] + r, pt[1]];
                    Self::add_wall(mesh, face, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_wall(mesh, face, p_mid, p_b, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_a, p_mid, r_bot, r_top, n_f);
                    Self::add_tri(mesh, face, pt, p_mid, p_b, r_bot, r_top, n_f);
                }
            }
        }
    }

    /// Rayon du chanfrein 3D (en fractions de cellule de bloc)
    pub const BEVEL_R: f64 = 0.22;

    #[inline]
    fn grid_to_vec3(face: PlanetFace, gx: f64, gy: f64, n: f64, r: f32) -> Vec3 {
        let u = -1.0 + 2.0 * gx / n;
        let v = -1.0 + 2.0 * gy / n;
        let dir = face.uv_to_sphere_direction(u, v);
        (dir * (r as f64)).as_vec3()
    }

    /// Ajoute une paroi latérale avec congés verticaux multi-segments et normales lissées
    fn add_wall(
        mesh: &mut PlanetMesh,
        face: PlanetFace,
        p0: [f64; 2],
        p1: [f64; 2],
        r_bot: f32,
        r_top: f32,
        n: f64,
    ) {
        let h_bev = (r_top - r_bot) * 0.22;
        let r0 = r_bot;
        let r1 = r_bot + h_bev;
        let r2 = r_top - h_bev;
        let r3 = r_top;

        let p0_r0 = Self::grid_to_vec3(face, p0[0], p0[1], n, r0);
        let p1_r0 = Self::grid_to_vec3(face, p1[0], p1[1], n, r0);
        let p0_r1 = Self::grid_to_vec3(face, p0[0], p0[1], n, r1);
        let p1_r1 = Self::grid_to_vec3(face, p1[0], p1[1], n, r1);
        let p0_r2 = Self::grid_to_vec3(face, p0[0], p0[1], n, r2);
        let p1_r2 = Self::grid_to_vec3(face, p1[0], p1[1], n, r2);
        let p0_r3 = Self::grid_to_vec3(face, p0[0], p0[1], n, r3);
        let p1_r3 = Self::grid_to_vec3(face, p1[0], p1[1], n, r3);

        let d_horiz = p1_r1 - p0_r1;
        let d_vert = p0_r2 - p0_r1;
        let norm_wall = d_horiz.cross(d_vert).normalize();

        let up0 = p0_r0.normalize();
        let up1 = p1_r0.normalize();

        // Normales lissées avec adoucissement vertical (congés hauts et bas)
        let n0_bot = (norm_wall - up0 * 0.55).normalize().to_array();
        let n1_bot = (norm_wall - up1 * 0.55).normalize().to_array();
        let n0_mid = norm_wall.to_array();
        let n1_mid = norm_wall.to_array();
        let n0_top = (norm_wall + up0 * 0.55).normalize().to_array();
        let n1_top = (norm_wall + up1 * 0.55).normalize().to_array();

        // 1. Bande inférieure de congé (r0 -> r1)
        let idx0 = mesh.vertices.len() as u32;
        mesh.vertices.push(PlanetVertex { position: p0_r0.to_array(), normal: n0_bot, color: [0.0, 0.0, 1.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r0.to_array(), normal: n1_bot, color: [1.0, 0.0, 1.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r1.to_array(), normal: n1_mid, color: [1.0, 0.22, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p0_r1.to_array(), normal: n0_mid, color: [0.0, 0.22, 0.0, 0.5] });
        mesh.indices.extend_from_slice(&[idx0, idx0 + 1, idx0 + 2, idx0, idx0 + 2, idx0 + 3]);

        // 2. Bande centrale verticale (r1 -> r2)
        let idx1 = mesh.vertices.len() as u32;
        mesh.vertices.push(PlanetVertex { position: p0_r1.to_array(), normal: n0_mid, color: [0.0, 0.22, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r1.to_array(), normal: n1_mid, color: [1.0, 0.22, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r2.to_array(), normal: n1_mid, color: [1.0, 0.78, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p0_r2.to_array(), normal: n0_mid, color: [0.0, 0.78, 0.0, 0.5] });
        mesh.indices.extend_from_slice(&[idx1, idx1 + 1, idx1 + 2, idx1, idx1 + 2, idx1 + 3]);

        // 3. Bande supérieure de congé (r2 -> r3)
        let idx2 = mesh.vertices.len() as u32;
        mesh.vertices.push(PlanetVertex { position: p0_r2.to_array(), normal: n0_mid, color: [0.0, 0.78, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r2.to_array(), normal: n1_mid, color: [1.0, 0.78, 0.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p1_r3.to_array(), normal: n1_top, color: [1.0, 1.0, 1.0, 0.5] });
        mesh.vertices.push(PlanetVertex { position: p0_r3.to_array(), normal: n0_top, color: [0.0, 1.0, 1.0, 0.5] });
        mesh.indices.extend_from_slice(&[idx2, idx2 + 1, idx2 + 2, idx2, idx2 + 2, idx2 + 3]);
    }

    fn add_tri(
        mesh: &mut PlanetMesh,
        face: PlanetFace,
        p0: [f64; 2],
        p1: [f64; 2],
        p2: [f64; 2],
        r_bot: f32,
        r_top: f32,
        n: f64,
    ) {
        // Face supérieure (Top) à r_top avec normales lissées sur le bord
        let pt0 = Self::grid_to_vec3(face, p0[0], p0[1], n, r_top);
        let pt1 = Self::grid_to_vec3(face, p1[0], p1[1], n, r_top);
        let pt2 = Self::grid_to_vec3(face, p2[0], p2[1], n, r_top);
        let n_top = (pt1 - pt0).cross(pt2 - pt0).normalize();
        let base_top = mesh.vertices.len() as u32;

        let n_top0 = n_top.to_array();
        let n_top1 = (n_top + (pt1 - pt0).normalize() * 0.35).normalize().to_array();
        let n_top2 = (n_top + (pt2 - pt0).normalize() * 0.35).normalize().to_array();

        mesh.vertices.push(PlanetVertex {
            position: pt0.to_array(),
            normal: n_top0,
            color: [0.0, 0.0, 0.0, 1.0],
        });
        mesh.vertices.push(PlanetVertex {
            position: pt1.to_array(),
            normal: n_top1,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        mesh.vertices.push(PlanetVertex {
            position: pt2.to_array(),
            normal: n_top2,
            color: [0.0, 1.0, 0.0, 1.0],
        });
        mesh.indices.extend_from_slice(&[base_top, base_top + 1, base_top + 2]);

        // Face inférieure (Bottom) à r_bot avec normales lissées sur le bord
        let pb0 = Self::grid_to_vec3(face, p0[0], p0[1], n, r_bot);
        let pb1 = Self::grid_to_vec3(face, p1[0], p1[1], n, r_bot);
        let pb2 = Self::grid_to_vec3(face, p2[0], p2[1], n, r_bot);
        let n_bot = (pb2 - pb0).cross(pb1 - pb0).normalize();
        let base_bot = mesh.vertices.len() as u32;

        let n_bot0 = n_bot.to_array();
        let n_bot1 = (n_bot + (pb1 - pb0).normalize() * 0.35).normalize().to_array();
        let n_bot2 = (n_bot + (pb2 - pb0).normalize() * 0.35).normalize().to_array();

        mesh.vertices.push(PlanetVertex {
            position: pb0.to_array(),
            normal: n_bot0,
            color: [0.0, 0.0, 0.0, 0.0],
        });
        mesh.vertices.push(PlanetVertex {
            position: pb2.to_array(),
            normal: n_bot2,
            color: [0.0, 1.0, 0.0, 0.0],
        });
        mesh.vertices.push(PlanetVertex {
            position: pb1.to_array(),
            normal: n_bot1,
            color: [1.0, 0.0, 0.0, 0.0],
        });
        mesh.indices.extend_from_slice(&[base_bot, base_bot + 1, base_bot + 2]);
    }

    fn hash_sector(seed: u64, face: u64, sx: u64, sy: u64) -> u64 {
        let mut h = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(face.wrapping_mul(1442695040888963407))
            .wrapping_add(sx.wrapping_mul(2862933555777941757))
            .wrapping_add(sy.wrapping_mul(5832141555777941757));
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51afd7ed558ccd);
        h ^= h >> 33;
        h
    }

    fn hash_cell(seed: u64, face: u64, cx: u64, cy: u64) -> u64 {
        let mut h = seed
            .wrapping_mul(1442695040888963407)
            .wrapping_add(face.wrapping_mul(6364136223846793005))
            .wrapping_add(cx.wrapping_mul(5832141555777941757))
            .wrapping_add(cy.wrapping_mul(2862933555777941757));
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
        h ^= h >> 33;
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_mesh_bounded_clusters_and_reasonable_sizes() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        // Vérifie que le maillage est généré avec des sommets et indices valides
        assert!(!mesh.vertices.is_empty(), "Le maillage de nuages ne doit pas être vide");
        assert!(!mesh.indices.is_empty(), "Les indices de nuages ne doivent pas être vides");
        assert_eq!(mesh.indices.len() % 3, 0, "Les indices doivent former des triangles complets");

        // Vérifie les bornes d'altitude : tous les sommets doivent être compris entre r_bot et r_top
        let r_bot = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_BOT - 0.01;
        let r_top = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_TOP + 0.01;

        for v in &mesh.vertices {
            let pos = Vec3::from(v.position);
            let r = pos.length();
            assert!(
                r >= r_bot && r <= r_top,
                "Position de sommet hors de la couche nuageuse : r = {r}, attendu [{r_bot}, {r_top}]"
            );
        }

        // Vérifie que la taille totale reste raisonnable (quelques milliers de quads, pas un monde infini)
        let quads = mesh.indices.len() / 6;
        assert!(
            quads >= 500 && quads <= 15000,
            "Nombre de quads de nuages ({quads}) doit être raisonnable pour un rendu 3D fluide"
        );
    }

    #[test]
    fn test_cloud_mesh_3d_bevels_and_inner_corners() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        let mut has_wall = false;
        let mut has_top = false;
        let mut has_bottom = false;
        let mut has_diagonal_bevel = false;

        for v in &mesh.vertices {
            let ft = v.color[3];
            assert!(
                (ft - 0.0).abs() < 1e-4 || (ft - 0.5).abs() < 1e-4 || (ft - 1.0).abs() < 1e-4,
                "Type de face inconnu : {}",
                ft
            );
            if (ft - 0.0).abs() < 1e-4 {
                has_bottom = true;
            } else if (ft - 0.5).abs() < 1e-4 {
                has_wall = true;
                let n = Vec3::from(v.normal);
                // Si la normale a deux composantes significatives non nulles dans le plan tangent, c'est un biseau diagonal
                let proj_sq = n.x * n.x + n.y * n.y + n.z * n.z;
                if proj_sq > 0.8 {
                    has_diagonal_bevel = true;
                }
            } else if (ft - 1.0).abs() < 1e-4 {
                has_top = true;
            }
        }

        assert!(has_wall, "Le maillage doit contenir des parois 3D");
        assert!(has_top, "Le maillage doit contenir des faces supérieures");
        assert!(has_bottom, "Le maillage doit contenir des faces inférieures");
        assert!(has_diagonal_bevel, "Le maillage doit contenir des biseaux diagonaux 3D");
    }

    #[test]
    fn test_cloud_mesh_multi_segment_fillets_and_smooth_normals() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        let mut has_upward_fillet = false;
        let mut has_downward_fillet = false;
        let mut has_horizontal_wall = false;

        for v in &mesh.vertices {
            let n = Vec3::from(v.normal);
            let len = n.length();
            assert!((len - 1.0).abs() < 1e-4, "Normale de nuage non unitaire : len = {len}");

            let pos = Vec3::from(v.position);
            let up = pos.normalize();
            let radial_dot = n.dot(up);

            // Sommets de parois (face_type == 0.5)
            if (v.color[3] - 0.5).abs() < 1e-4 {
                let uv_y = v.color[1];
                if uv_y > 0.9 {
                    // Congé supérieur : normale orientée vers le haut
                    if radial_dot > 0.15 {
                        has_upward_fillet = true;
                    }
                } else if uv_y < 0.1 {
                    // Congé inférieur : normale orientée vers le bas
                    if radial_dot < -0.15 {
                        has_downward_fillet = true;
                    }
                } else if (uv_y - 0.5).abs() < 0.3 {
                    // Paroi centrale : normale quasi-horizontale
                    if radial_dot.abs() < 0.10 {
                        has_horizontal_wall = true;
                    }
                }
            }
        }

        assert!(has_upward_fillet, "Le maillage doit comporter des congés verticaux supérieurs");
        assert!(has_downward_fillet, "Le maillage doit comporter des congés verticaux inférieurs");
        assert!(has_horizontal_wall, "Le maillage doit comporter des parois centrales horizontales");
    }
}

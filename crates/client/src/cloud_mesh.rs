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

        // 2. Génération des voxels 3D pour chaque cellule occupée
        for y in 0..n {
            for x in 0..n {
                if !occupied[y][x] {
                    continue;
                }

                let u0 = -1.0 + 2.0 * (x as f64) / (n as f64);
                let u1 = -1.0 + 2.0 * ((x + 1) as f64) / (n as f64);
                let v0 = -1.0 + 2.0 * (y as f64) / (n as f64);
                let v1 = -1.0 + 2.0 * ((y + 1) as f64) / (n as f64);

                let d00 = face.uv_to_sphere_direction(u0, v0);
                let d10 = face.uv_to_sphere_direction(u1, v0);
                let d11 = face.uv_to_sphere_direction(u1, v1);
                let d01 = face.uv_to_sphere_direction(u0, v1);

                let d_center = face
                    .uv_to_sphere_direction((u0 + u1) * 0.5, (v0 + v1) * 0.5)
                    .as_vec3()
                    .normalize();

                let pb0 = (d00 * r_bot as f64).as_vec3();
                let pb1 = (d10 * r_bot as f64).as_vec3();
                let pb2 = (d11 * r_bot as f64).as_vec3();
                let pb3 = (d01 * r_bot as f64).as_vec3();

                let pt0 = (d00 * r_top as f64).as_vec3();
                let pt1 = (d10 * r_top as f64).as_vec3();
                let pt2 = (d11 * r_top as f64).as_vec3();
                let pt3 = (d01 * r_top as f64).as_vec3();

                let has_west = x > 0 && occupied[y][x - 1];
                let has_east = x + 1 < n && occupied[y][x + 1];
                let has_south = y > 0 && occupied[y - 1][x];
                let has_north = y + 1 < n && occupied[y + 1][x];

                let ext_w = !has_west;
                let ext_e = !has_east;
                let ext_s = !has_south;
                let ext_n = !has_north;

                // 2.a. Face inférieure (visible du sol en regardant vers le haut)
                // Sommets ordonnés : SW (0,0), NW (0,1), NE (1,1), SE (1,0)
                let bot_mask = ((if ext_w && ext_s { 1 } else { 0 })   // c00 (SW)
                    | (if ext_e && ext_s { 2 } else { 0 })             // c10 (SE)
                    | (if ext_e && ext_n { 4 } else { 0 })             // c11 (NE)
                    | (if ext_w && ext_n { 8 } else { 0 })             // c01 (NW)
                    | (if ext_w { 16 } else { 0 })                     // edge u=0 (W)
                    | (if ext_e { 32 } else { 0 })                     // edge u=1 (E)
                    | (if ext_s { 64 } else { 0 })                     // edge v=0 (S)
                    | (if ext_n { 128 } else { 0 })) as f32;           // edge v=1 (N)

                Self::add_quad(
                    mesh,
                    [
                        (pb0, [0.0, 0.0]),
                        (pb3, [0.0, 1.0]),
                        (pb2, [1.0, 1.0]),
                        (pb1, [1.0, 0.0]),
                    ],
                    -d_center,
                    bot_mask,
                    0.0, // bottom
                );

                // 2.b. Face supérieure (visible d'en haut / depuis l'espace)
                // Sommets ordonnés : SW (0,0), SE (1,0), NE (1,1), NW (0,1)
                let top_mask = ((if ext_w && ext_s { 1 } else { 0 })   // c00 (SW)
                    | (if ext_e && ext_s { 2 } else { 0 })             // c10 (SE)
                    | (if ext_e && ext_n { 4 } else { 0 })             // c11 (NE)
                    | (if ext_w && ext_n { 8 } else { 0 })             // c01 (NW)
                    | (if ext_w { 16 } else { 0 })                     // edge u=0 (W)
                    | (if ext_e { 32 } else { 0 })                     // edge u=1 (E)
                    | (if ext_s { 64 } else { 0 })                     // edge v=0 (S)
                    | (if ext_n { 128 } else { 0 })) as f32;           // edge v=1 (N)

                Self::add_quad(
                    mesh,
                    [
                        (pt0, [0.0, 0.0]),
                        (pt1, [1.0, 0.0]),
                        (pt2, [1.0, 1.0]),
                        (pt3, [0.0, 1.0]),
                    ],
                    d_center,
                    top_mask,
                    1.0, // top
                );

                // 2.c. Paroi Ouest (si bordure extérieure)
                if ext_w {
                    let n_west = ((pb0 - pb1) + (pb3 - pb2)).normalize();
                    // u: South -> North, v: Bottom -> Top
                    let west_mask = ((if ext_s { 1 } else { 0 })       // c00 (South, Bottom)
                        | (if ext_n { 2 } else { 0 })                 // c10 (North, Bottom)
                        | (if ext_n { 4 } else { 0 })                 // c11 (North, Top)
                        | (if ext_s { 8 } else { 0 })                 // c01 (South, Top)
                        | (if ext_s { 16 } else { 0 })                // edge u=0 (South)
                        | (if ext_n { 32 } else { 0 })                // edge u=1 (North)
                        | 64                                          // edge v=0 (Bottom)
                        | 128) as f32;                                // edge v=1 (Top)

                    Self::add_quad(
                        mesh,
                        [
                            (pb0, [0.0, 0.0]),
                            (pb3, [1.0, 0.0]),
                            (pt3, [1.0, 1.0]),
                            (pt0, [0.0, 1.0]),
                        ],
                        n_west,
                        west_mask,
                        0.5, // side
                    );
                }

                // 2.d. Paroi Est (si bordure extérieure)
                if ext_e {
                    let n_east = ((pb1 - pb0) + (pb2 - pb3)).normalize();
                    // u: South -> North, v: Bottom -> Top
                    let east_mask = ((if ext_s { 1 } else { 0 })       // c00 (South, Bottom)
                        | (if ext_n { 2 } else { 0 })                 // c10 (North, Bottom)
                        | (if ext_n { 4 } else { 0 })                 // c11 (North, Top)
                        | (if ext_s { 8 } else { 0 })                 // c01 (South, Top)
                        | (if ext_s { 16 } else { 0 })                // edge u=0 (South)
                        | (if ext_n { 32 } else { 0 })                // edge u=1 (North)
                        | 64                                          // edge v=0 (Bottom)
                        | 128) as f32;                                // edge v=1 (Top)

                    Self::add_quad(
                        mesh,
                        [
                            (pb1, [0.0, 0.0]),
                            (pb2, [1.0, 0.0]),
                            (pt2, [1.0, 1.0]),
                            (pt1, [0.0, 1.0]),
                        ],
                        n_east,
                        east_mask,
                        0.5, // side
                    );
                }

                // 2.e. Paroi Sud (si bordure extérieure)
                if ext_s {
                    let n_south = ((pb0 - pb3) + (pb1 - pb2)).normalize();
                    // u: West -> East, v: Bottom -> Top
                    let south_mask = ((if ext_w { 1 } else { 0 })      // c00 (West, Bottom)
                        | (if ext_e { 2 } else { 0 })                 // c10 (East, Bottom)
                        | (if ext_e { 4 } else { 0 })                 // c11 (East, Top)
                        | (if ext_w { 8 } else { 0 })                 // c01 (West, Top)
                        | (if ext_w { 16 } else { 0 })                // edge u=0 (West)
                        | (if ext_e { 32 } else { 0 })                // edge u=1 (East)
                        | 64                                          // edge v=0 (Bottom)
                        | 128) as f32;                                // edge v=1 (Top)

                    Self::add_quad(
                        mesh,
                        [
                            (pb0, [0.0, 0.0]),
                            (pb1, [1.0, 0.0]),
                            (pt1, [1.0, 1.0]),
                            (pt0, [0.0, 1.0]),
                        ],
                        n_south,
                        south_mask,
                        0.5, // side
                    );
                }

                // 2.f. Paroi Nord (si bordure extérieure)
                if ext_n {
                    let n_north = ((pb3 - pb0) + (pb2 - pb1)).normalize();
                    // u: West -> East, v: Bottom -> Top
                    let north_mask = ((if ext_w { 1 } else { 0 })      // c00 (West, Bottom)
                        | (if ext_e { 2 } else { 0 })                 // c10 (East, Bottom)
                        | (if ext_e { 4 } else { 0 })                 // c11 (East, Top)
                        | (if ext_w { 8 } else { 0 })                 // c01 (West, Top)
                        | (if ext_w { 16 } else { 0 })                // edge u=0 (West)
                        | (if ext_e { 32 } else { 0 })                // edge u=1 (East)
                        | 64                                          // edge v=0 (Bottom)
                        | 128) as f32;                                // edge v=1 (Top)

                    Self::add_quad(
                        mesh,
                        [
                            (pb3, [0.0, 0.0]),
                            (pb2, [1.0, 0.0]),
                            (pt2, [1.0, 1.0]),
                            (pt3, [0.0, 1.0]),
                        ],
                        n_north,
                        north_mask,
                        0.5, // side
                    );
                }
            }
        }
    }

    #[inline]
    fn add_quad(
        mesh: &mut PlanetMesh,
        pts_with_uv: [(Vec3, [f32; 2]); 4],
        normal: Vec3,
        bevel_mask: f32,
        face_type: f32,
    ) {
        let base_idx = mesh.vertices.len() as u32;
        let norm_arr = normal.to_array();

        for (pos, uv) in pts_with_uv {
            mesh.vertices.push(PlanetVertex {
                position: pos.to_array(),
                normal: norm_arr,
                color: [uv[0], uv[1], bevel_mask, face_type],
            });
        }

        // Deux triangles par quad (sens anti-horaire CCW)
        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 1);
        mesh.indices.push(base_idx + 2);

        mesh.indices.push(base_idx);
        mesh.indices.push(base_idx + 2);
        mesh.indices.push(base_idx + 3);
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
    fn test_cloud_mesh_per_quad_corner_and_edge_masks() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        // Vérifie que tous les masques sont des entiers valides dans l'intervalle [0, 255]
        for v in &mesh.vertices {
            let mask_f = v.color[2];
            let mask = mask_f.round() as u32;
            assert!(
                (mask_f - mask as f32).abs() < 1e-4,
                "Le masque de biseau doit être un entier exact encodé en f32 : {}",
                mask_f
            );
            assert!(mask <= 255, "Le masque de biseau doit tenir sur 8 bits : {}", mask);

            // Vérifie que face_type est 0.0 (bottom), 0.5 (side) ou 1.0 (top)
            let ft = v.color[3];
            assert!(
                (ft - 0.0).abs() < 1e-4 || (ft - 0.5).abs() < 1e-4 || (ft - 1.0).abs() < 1e-4,
                "Type de face inconnu : {}",
                ft
            );
        }
    }
}

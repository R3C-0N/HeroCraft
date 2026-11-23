//! Tri des morceaux de terrain à dessiner dans la passe 3D : ceux qui sortent du champ de vision,
//! ou qui restent cachés sous l'horizon de la planète aplatie, ne sont pas envoyés au GPU.
//! Les ombres gardent la liste complète (un relief hors champ peut ombrer ce qu'on voit).

use glam::{Mat4, Vec3, Vec4};

/// Marge relative ajoutée aux sphères englobantes : l'aplatissement déplace légèrement les
/// points d'un maillage les uns par rapport aux autres
const BOUNDS_MARGIN: f32 = 1.1;
/// Marge sur la portée de l'horizon, en fraction de la portée
const HORIZON_MARGIN: f32 = 1.05;
/// Au-delà de cet angle au centre (depuis la verticale de la caméra), `flatten_point` du shader
/// rabat les sommets : le maillage n'est pas testé contre le champ de vision
const FOLDED_ANGLE: f32 = 0.75 * std::f32::consts::PI;

/// Vue de la caméra telle que la passe 3D la dessine (voir `flatten_point` dans common.wgsl)
#[derive(Clone, Copy, Debug)]
pub struct TerrainCull {
    /// Plans gauche, droite, bas et haut du champ de vision (normale vers l'intérieur, distance)
    planes: [Vec4; 4],
    eye: Vec3,
    up: Vec3,
    sea_radius: f32,
    /// Rayon du grand globe sur lequel le relief est redessiné (k fois le niveau de la mer)
    big_radius: f32,
    /// `camera.flatten` du shader : surplus de rayon, k, début et fin du fondu (distance au sol)
    flatten: [f32; 4],
    /// Hauteur minimale de l'œil au-dessus de la mer prise en compte pour l'horizon
    min_eye_height: f32,
}

impl TerrainCull {
    pub fn new(view_proj: Mat4, eye: Vec3, sea_radius: f32, flatten: [f32; 4], block: f32) -> Self {
        let (r0, r1, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(3));
        let normalize = |p: Vec4| {
            let len = p.truncate().length();
            if len > 1e-12 { p / len } else { Vec4::ZERO }
        };
        Self {
            planes: [normalize(r3 + r0), normalize(r3 - r0), normalize(r3 + r1), normalize(r3 - r1)],
            eye,
            up: eye.normalize_or_zero(),
            sea_radius,
            big_radius: flatten[1].max(1.0) * sea_radius,
            flatten,
            min_eye_height: 2.0 * block,
        }
    }

    /// Le maillage de centre `center`, de sphère englobante `radius` et dont aucun sommet ne
    /// dépasse `max_radius` du centre de la planète peut-il se voir ?
    pub fn is_visible(&self, center: Vec3, radius: f32, max_radius: f32) -> bool {
        let dist = center.length();
        if dist < 1e-6 || self.up == Vec3::ZERO {
            return true;
        }
        // Angle au centre entre la verticale de la caméra et le point du maillage le plus proche
        let theta = self.up.dot(center / dist).clamp(-1.0, 1.0).acos();
        let spread = if radius < dist { (radius / dist).asin() } else { std::f32::consts::PI };
        let theta_min = (theta - spread).max(0.0);

        // 1. Horizon : un sommet de hauteur H (au-dessus de la mer) se voit d'un œil à la hauteur h
        // tant que la distance au sol reste sous la somme des deux tangentes au grand globe.
        // L'aplatissement garde les distances au sol et les altitudes.
        let rho = self.big_radius;
        let h = (self.eye.length() - self.sea_radius).max(self.min_eye_height);
        let peak = (max_radius - self.sea_radius).max(0.0);
        let reach = (2.0 * rho * h + h * h).sqrt() + (2.0 * rho * peak + peak * peak).sqrt();
        if theta_min * self.sea_radius > reach * HORIZON_MARGIN {
            return false;
        }

        // 2. Champ de vision, à la position où le shader dessine le centre
        if theta + spread > FOLDED_ANGLE {
            return true;
        }
        let drawn = self.flatten_point(center);
        let r = radius * BOUNDS_MARGIN;
        self.planes.iter().all(|p| p.truncate().dot(drawn) + p.w >= -r)
    }

    /// La même vue pour le tri sur le GPU (`terrain_cull.wgsl`)
    pub fn gpu_pass(&self) -> crate::terrain_pool::CullPass {
        crate::terrain_pool::CullPass {
            kind: crate::terrain_pool::CullPass::CAMERA,
            planes: self.planes.map(|p| p.to_array()),
            eye: self.eye.extend(self.sea_radius).to_array(),
            up: self.up.extend(self.big_radius).to_array(),
            flatten: self.flatten,
            margins: [self.min_eye_height, BOUNDS_MARGIN, HORIZON_MARGIN, FOLDED_ANGLE],
            ..Default::default()
        }
    }

    /// Copie de `flatten_point` (common.wgsl) : position où le shader dessine `p`
    fn flatten_point(&self, p: Vec3) -> Vec3 {
        let extra = self.flatten[0];
        if extra <= 0.0 {
            return p;
        }
        let r = p.length();
        let d = p / r.max(1e-6);
        let c = self.up.dot(d);
        let side = d - self.up * c;
        let s = side.length();
        if s < 1e-7 {
            return p;
        }
        let t = side / s;
        let theta = s.atan2(c);
        let rho = r + extra;
        let theta2 = theta * r / rho;
        let half = (0.5 * theta2).sin();
        let flat = self.up * (r - 2.0 * rho * half * half) + t * (rho * theta2.sin());
        let (a, b) = (self.flatten[2], self.flatten[3]);
        let x = ((theta * r - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
        let w = x * x * (3.0 - 2.0 * x);
        p.lerp(flat, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 400.0;
    const BLOCK: f32 = R * std::f32::consts::FRAC_PI_2 / 8192.0;

    /// Caméra à hauteur d'homme sur le pôle +Y, regard vers +Z, champ de vision de 70°
    fn ground_view(flatten: [f32; 4]) -> TerrainCull {
        let eye = Vec3::Y * (R + 2.0 * BLOCK);
        let view = Mat4::look_at_rh(eye, eye + Vec3::Z, Vec3::Y);
        let proj = Mat4::perspective_rh(70f32.to_radians(), 16.0 / 9.0, 0.001, 2000.0);
        TerrainCull::new(proj * view, eye, R, flatten, BLOCK)
    }

    fn at_ground(angle: f32, toward: Vec3, height_blocks: f32) -> Vec3 {
        (Vec3::Y * angle.cos() + toward * angle.sin()) * (R + height_blocks * BLOCK)
    }

    #[test]
    fn test_frustum_keeps_front_and_drops_back() {
        let cull = ground_view([0.0, 1.0, 0.0, 0.0]);
        let size = 32.0 * BLOCK;
        let front = at_ground(100.0 * BLOCK / R, Vec3::Z, 0.0);
        let back = at_ground(100.0 * BLOCK / R, -Vec3::Z, 0.0);
        assert!(cull.is_visible(front, size, R + 10.0 * BLOCK));
        assert!(!cull.is_visible(back, size, R + 10.0 * BLOCK));
        // Le morceau sous les pieds reste dessiné, même s'il déborde derrière
        assert!(cull.is_visible(Vec3::Y * R, size, R + 10.0 * BLOCK));
    }

    #[test]
    fn test_horizon_hides_low_far_terrain_but_not_mountains() {
        let cull = ground_view([0.0, 1.0, 0.0, 0.0]);
        let size = 64.0 * BLOCK;
        // Sans aplatissement, l'horizon d'un œil à 2 blocs est à environ 360 blocs
        let far = at_ground(2000.0 * BLOCK / R, Vec3::Z, 0.0);
        assert!(!cull.is_visible(far, size, R + 5.0 * BLOCK));
        assert!(cull.is_visible(far, size, R + 300.0 * BLOCK), "un sommet dépasse l'horizon");
    }

    #[test]
    fn test_flattening_pushes_horizon_back() {
        let k = crate::renderer::planet_flattening(R + 2.0 * BLOCK, R, 1.0);
        assert!(k > 2.0);
        let flatten = [(k - 1.0) * R, k, 6.0 * BLOCK, 22.0 * BLOCK];
        let flat = ground_view(flatten);
        let round = ground_view([0.0, 1.0, 0.0, 0.0]);
        let mid = at_ground(800.0 * BLOCK / R, Vec3::Z, 0.0);
        assert!(!round.is_visible(mid, 64.0 * BLOCK, R + 5.0 * BLOCK));
        assert!(flat.is_visible(mid, 64.0 * BLOCK, R + 5.0 * BLOCK));
    }

    #[test]
    fn test_flatten_point_matches_shader_rules() {
        let cull = ground_view([R, 2.0, 0.0, 10.0 * BLOCK]);
        // Sous la caméra : rien ne bouge
        let below = Vec3::Y * R;
        assert!((cull.flatten_point(below) - below).length() < 1e-4);
        // Au loin : l'altitude est conservée, le point remonte vers le plan tangent
        let far = at_ground(0.5, Vec3::Z, 0.0);
        let drawn = cull.flatten_point(far);
        assert!(drawn.y > far.y);
        let big_center = -Vec3::Y * R;
        assert!(((drawn - big_center).length() - 2.0 * R).abs() < 1e-2);
    }

    /// Le tri sur le GPU (`terrain_cull.wgsl`) garde les mêmes morceaux que le CPU, pour la passe 3D
    /// (avec et sans aplatissement) et pour une cascade d'ombre. Demande une carte graphique
    /// (lavapipe convient) : `cargo test -- --ignored gpu_cull`
    #[test]
    #[ignore]
    fn test_gpu_cull_matches_cpu() {
        use crate::terrain_pool::{CullPass, PoolSlot, TerrainPool};
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("aucun adaptateur graphique");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let mut pool = TerrainPool::new(&device);
        assert!(pool.gpu_culling());

        // Morceaux semés tout autour de la planète, à diverses hauteurs et tailles
        let mut seed = 12345u32;
        let mut rand = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32
        };
        let mut meshes = Vec::new();
        for k in 0..3000u32 {
            let dir = Vec3::new(rand() * 2.0 - 1.0, rand() * 2.0 - 1.0, rand() * 2.0 - 1.0).normalize_or(Vec3::Y);
            // La moitié tout près de la caméra (pôle +Y), où le champ de vision décide
            let dir = if k % 2 == 0 { (Vec3::Y + dir * 0.05).normalize() } else { dir };
            let height = rand() * 40.0 * BLOCK;
            let radius = (1.0 + rand() * 63.0) * BLOCK;
            let format = if k % 3 == 0 { wgpu::IndexFormat::Uint32 } else { wgpu::IndexFormat::Uint16 };
            let slot = PoolSlot { vertices: k * 10..k * 10 + 10, indices: k * 30..k * 30 + 30, num_indices: 30, format };
            meshes.push((slot, dir * (R + height), radius, R + height + radius));
        }
        pool.set_draw_table(&device, &queue, meshes.iter().map(|(s, c, r, m)| (s, *c, *r, *m)));
        // Ordre de la table : indices courts puis longs
        let table: Vec<_> = meshes.iter().filter(|m| m.0.format == wgpu::IndexFormat::Uint16).chain(meshes.iter().filter(|m| m.0.format == wgpu::IndexFormat::Uint32)).collect();

        let views = [ground_view([0.0, 1.0, 0.0, 0.0]), ground_view([R, 2.0, 0.0, 10.0 * BLOCK])];
        let (right, up) = (Vec3::X, Vec3::Z);
        let (cascade_center, extent) = (Vec3::Y * R + Vec3::X * 20.0 * BLOCK, 50.0 * BLOCK);
        let mut passes = vec![views[0].gpu_pass(), views[1].gpu_pass(), CullPass::cascade(cascade_center, extent, right, up), CullPass::everything()];
        let mut encoder = device.create_command_encoder(&Default::default());
        let draws = pool.cull(&device, &queue, &mut encoder, &mut passes);
        queue.submit([encoder.finish()]);

        for (p, prepared) in draws.iter().enumerate() {
            let args = pool.read_culled(&device, &queue, prepared);
            assert_eq!(args.len(), table.len());
            let mut mismatches = 0;
            let mut shown = 0;
            for (a, (slot, center, radius, max_radius)) in args.iter().zip(&table) {
                assert_eq!((a.first_index, a.base_vertex, a.index_count), (slot.indices.start, slot.vertices.start as i32, 30));
                let cpu = match p {
                    0 | 1 => views[p].is_visible(*center, *radius, *max_radius),
                    2 => {
                        let d = *center - cascade_center;
                        d.dot(right).abs() <= extent + radius && d.dot(up).abs() <= extent + radius
                    }
                    _ => true,
                };
                shown += cpu as usize;
                mismatches += (cpu != (a.instance_count == 1)) as usize;
            }
            eprintln!("passe {p} : {shown} visibles, {mismatches} écarts");
            // Quelques écarts d'arrondi au bord sont tolérés (aucun sur des milliers en pratique)
            assert!(mismatches <= 3, "passe {p} : {mismatches} morceaux triés autrement que sur le CPU");
            assert!(shown > 0 && (p == 3 || shown < table.len()), "passe {p} : test sans intérêt ({shown} visibles)");
        }
    }
}

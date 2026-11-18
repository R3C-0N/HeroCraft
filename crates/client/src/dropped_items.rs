//! Objets tombés au sol : un bloc cassé laisse un petit cube, un objet (butin d'une plante ou
//! d'une bête) une petite plaque à son icône. Il tombe, se pose sur le relief et tourne sur
//! lui-même. Le joueur qui passe à proximité l'attire puis le ramasse dans son
//! inventaire (s'il y a de la place). Un objet oublié disparaît au bout de quelques minutes,
//! sauf une Pierre du Néant : elle reste là où elle est, brille, et son ramassage est signalé
//! (`StoneEvent::Obtained`).

use glam::{Quat, Vec3};
use herocraft_common::{blocks, BlockId, ItemId, StoneEvent, StoneKind};
use herocraft_world::PlanetWorldGenerator;

use crate::block_textures::{block_textures, face_uv};
use crate::collision::VoxelCollider;
use crate::inventory::{item_colors, Inventory};
use crate::planet_mesh::{PlanetMesh, PlanetVertex};

/// Côté du cube dessiné, en blocs
const ITEM_SIZE: f32 = 0.28;
/// Gravité, en blocs par seconde²
const GRAVITY: f32 = 22.0;
/// Délai avant de pouvoir ramasser un objet qui vient de tomber
const PICKUP_DELAY: f32 = 0.4;
/// En deçà (en blocs, depuis le milieu du corps du joueur), l'objet est attiré vers le joueur
const ATTRACT_RADIUS: f32 = 2.5;
/// En deçà, l'objet entre dans l'inventaire
const PICKUP_RADIUS: f32 = 0.7;
/// Vitesse d'attraction, en blocs par seconde
const ATTRACT_SPEED: f32 = 9.0;
/// Durée de vie d'un objet au sol, en secondes
const LIFETIME: f32 = 300.0;
/// Au-delà, les plus anciens objets disparaissent
const MAX_ITEMS: usize = 512;
/// Taille de la plaque d'un objet, rapportée au cube d'un bloc
const ICON_SCALE: f32 = 1.4;
/// Vitesse de rotation, en radians par seconde
const SPIN_SPEED: f32 = 1.6;

#[derive(Debug, Clone, Copy)]
pub struct DroppedItem {
    pub item: ItemId,
    /// Centre du cube, dans le repère du monde
    pub position: Vec3,
    velocity: Vec3,
    age: f32,
    /// Phase de rotation et de flottement, pour que les objets voisins ne bougent pas en bloc
    phase: f32,
    /// Attiré par le joueur proche ; sinon (œuf pondu) il reste là où il est posé, et ne se
    /// ramasse qu'en marchant dessus
    attracted: bool,
}

#[derive(Default)]
pub struct DroppedItems {
    items: Vec<DroppedItem>,
    /// Pierres du Néant ramassées depuis le dernier `take_stone_events`
    stone_events: Vec<StoneEvent>,
    /// Les objets ont bougé depuis le dernier maillage
    dirty: bool,
}

impl DroppedItems {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn items(&self) -> &[DroppedItem] {
        &self.items
    }

    /// Plus aucun objet (changement de planète)
    pub fn clear(&mut self) {
        self.dirty |= !self.items.is_empty();
        self.items.clear();
    }

    /// Fait tomber `item` depuis `center` (le centre du bloc cassé, ou le flanc de la bête tuée),
    /// avec un petit saut vers le haut
    pub fn spawn(&mut self, item: impl Into<ItemId>, center: Vec3, block_height: f32) {
        let item = item.into();
        let up = center.normalize_or_zero();
        // Phase pseudo-aléatoire tirée de la position : deux objets ne tournent pas à l'unisson
        let phase = (center.x * 12.9898 + center.y * 78.233 + center.z * 37.719).sin().fract().abs() * std::f32::consts::TAU;
        let (t1, t2) = crate::camera::tangent_frame(up);
        let sideways = (t1 * phase.cos() + t2 * phase.sin()) * 0.8;
        self.items.push(DroppedItem {
            item,
            position: center,
            velocity: (up * 3.5 + sideways) * block_height,
            age: 0.0,
            phase,
            attracted: true,
        });
        self.evict_oldest();
        self.dirty = true;
    }

    /// Pose `item` sans saut en `center` (un œuf pondu, aux pieds de la bête) : il tombe droit au
    /// sol et y reste, sans être attiré par le joueur qui passe près (il se ramasse en marchant dessus)
    pub fn lay(&mut self, item: impl Into<ItemId>, center: Vec3) {
        let phase = (center.x * 12.9898 + center.y * 78.233 + center.z * 37.719).sin().fract().abs() * std::f32::consts::TAU;
        self.items.push(DroppedItem { item: item.into(), position: center, velocity: Vec3::ZERO, age: 0.0, phase, attracted: false });
        self.evict_oldest();
        self.dirty = true;
    }

    /// Pose `item` en `center`, sans saut (une Pierre du Néant sur l'autel d'un temple) : il
    /// tombe droit au sol et s'attire vers le joueur qui s'approche, comme un butin
    pub fn place(&mut self, item: impl Into<ItemId>, center: Vec3) {
        let phase = (center.x * 12.9898 + center.y * 78.233 + center.z * 37.719).sin().fract().abs() * std::f32::consts::TAU;
        self.items.push(DroppedItem { item: item.into(), position: center, velocity: Vec3::ZERO, age: 0.0, phase, attracted: true });
        self.evict_oldest();
        self.dirty = true;
    }

    /// Au-delà de `MAX_ITEMS`, le plus ancien objet disparaît (jamais une Pierre du Néant)
    fn evict_oldest(&mut self) {
        if self.items.len() > MAX_ITEMS {
            if let Some(oldest) = self.items.iter().position(|item| !item.item.is_void_stone()) {
                self.items.remove(oldest);
            }
        }
    }

    /// Les Pierres du Néant ramassées depuis le dernier appel (« Pierre obtenue »)
    pub fn take_stone_events(&mut self) -> Vec<StoneEvent> {
        std::mem::take(&mut self.stone_events)
    }

    /// Chute, pose sur le relief, attraction vers le joueur et ramassage. `player_center` est le
    /// milieu du corps du joueur, ou rien s'il ne peut pas ramasser (vue satellite). Renvoie le
    /// nombre d'objets ramassés.
    pub fn update(
        &mut self,
        dt: f32,
        generator: &PlanetWorldGenerator,
        collider: &VoxelCollider,
        player_center: Option<Vec3>,
        inventory: &mut Inventory,
    ) -> usize {
        if self.items.is_empty() {
            return 0;
        }
        herocraft_common::prof_scope!("objets_au_sol");
        let dt = dt.min(0.1);
        let h = collider.block_height();
        let half = ITEM_SIZE * 0.5 * h;
        let mut picked = 0;
        let stone_events = &mut self.stone_events;
        self.items.retain_mut(|item| {
            item.age += dt;
            // Une Pierre du Néant ne disparaît jamais d'elle-même
            if item.age > LIFETIME && !item.item.is_void_stone() {
                return false;
            }
            let up = item.position.normalize_or_zero();

            // Attiré par le joueur proche, à travers tout
            if let Some(center) = player_center.filter(|_| item.age >= PICKUP_DELAY) {
                let to_player = center - item.position;
                let distance = to_player.length() / h;
                if distance < PICKUP_RADIUS && inventory.add(item.item) {
                    picked += 1;
                    if let Some(kind) = StoneKind::of(item.item) {
                        stone_events.push(StoneEvent::Obtained { kind, item: item.item, position: item.position.to_array() });
                    }
                    return false;
                }
                if item.attracted && distance < ATTRACT_RADIUS && inventory.has_room_for(item.item) {
                    let step = (ATTRACT_SPEED * h * dt).min(to_player.length());
                    item.position += to_player.normalize_or_zero() * step;
                    item.velocity = Vec3::ZERO;
                    return true;
                }
            }

            // Chute vers le centre de la planète, jusqu'au dessus du bloc porteur
            item.velocity -= up * GRAVITY * h * dt;
            let next = item.position + item.velocity * dt;
            let ground = collider.ground_radius(generator, next, 0.5) + half;
            if next.length() <= ground {
                item.position = next.normalize_or_zero() * ground;
                // Posé : plus de vitesse verticale, la glissade s'amortit vite
                let radial = item.velocity.dot(up);
                item.velocity = (item.velocity - up * radial) * (1.0 - (8.0 * dt).min(1.0));
            } else {
                item.position = next;
            }
            true
        });
        // Les objets tournent sans cesse : le maillage est refait à chaque image tant qu'il y en a
        self.dirty = true;
        picked
    }

    /// Maillage des cubes à dessiner avec le relief (même éclairage et mêmes ombres), ou rien
    /// s'il n'a pas changé depuis le dernier appel
    pub fn take_mesh(&mut self, block_height: f32, time: f32) -> Option<PlanetMesh> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let mut mesh = PlanetMesh::default();
        let half = ITEM_SIZE * 0.5 * block_height;
        for item in &self.items {
            let up = item.position.normalize_or_zero();
            let (t1, t2) = crate::camera::tangent_frame(up);
            let angle = item.phase + time * SPIN_SPEED;
            let spin = Quat::from_axis_angle(up, angle);
            let (ax, az) = (spin * t1, spin * t2);
            // Léger flottement au-dessus du sol, comme un objet à ramasser
            let bob = (0.5 + 0.5 * (item.phase + time * 2.5).sin()) * 0.12 * block_height;
            let center = item.position + up * bob;
            match item.item.block() {
                Some(block) => push_cube(&mut mesh, center, [ax * half, up * half, az * half], block, 0.0),
                None => push_icon(&mut mesh, center, ax * half * ICON_SCALE, up * half * ICON_SCALE, item.item),
            }
        }
        Some(mesh)
    }
}

/// Plaque verticale de l'icône de l'objet `item`, centrée en `center`, de demi-largeur `right` et
/// de demi-hauteur `up`, dessinée des deux côtés. Un objet sans icône chargée reste un cube gris.
fn push_icon(mesh: &mut PlanetMesh, center: Vec3, right: Vec3, up: Vec3, item: ItemId) {
    // Une Pierre du Néant luit de sa propre lumière, même dans la nuit d'une crypte
    let light = if item.is_void_stone() { 1.0 } else { 0.0 };
    let Some(layer) = block_textures().icon_layer(item) else {
        push_cube(mesh, center, [right / ICON_SCALE, up / ICON_SCALE, right.cross(up).normalize_or_zero() * right.length() / ICON_SCALE], BlockId::AIR, light);
        return;
    };
    let mean = block_textures().mean(layer).unwrap_or([1.0; 3]);
    let normal = right.cross(up).normalize_or_zero();
    for side in [1.0f32, -1.0] {
        let base = mesh.vertices.len() as u32;
        // Coins dans l'ordre bas-gauche, bas-droite, haut-droite, haut-gauche, vus du côté `side`
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let corner = center + right * su * side + up * sv;
            let tu = if su * side > 0.0 { 16 } else { 0 };
            let tv = if sv > 0.0 { 0 } else { 16 };
            let texture = PlanetVertex::pack_texture(layer, tu, tv);
            mesh.vertices.push(PlanetVertex { position: corner.to_array(), normal: (normal * side).to_array(), color: [mean[0], mean[1], mean[2], 1.0], texture, light });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Cube du bloc `block` centré en `center`, de demi-axes `axes` (le deuxième est le haut),
/// couvert de ses textures comme dans le terrain, éclairé de `light` (`PlanetVertex::light`)
fn push_cube(mesh: &mut PlanetMesh, center: Vec3, axes: [Vec3; 3], block: BlockId, light: f32) {
    let (top, side) = item_colors(block);
    // Faces dans l'ordre de `FACE_DIRS` : +X, -X, +Y, -Y, +Z, -Z
    for (dir, (axis, sign)) in [(0, 1.0), (0, -1.0), (1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)].into_iter().enumerate() {
        let normal = axes[axis].normalize_or_zero() * sign;
        let (u, v) = (axes[(axis + 1) % 3], axes[(axis + 2) % 3]);
        let face = center + axes[axis] * sign;
        let color = block_textures().face_color(block, dir).unwrap_or(if dir == 2 { top } else { side });
        // Bloc connecté (verre) : la variante encadrée sur ses quatre bords
        let layer = block_textures().connected_layer(blocks().face_texture(block, dir), 0);
        let base = mesh.vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let corner = face + u * su + v * sv;
            // Place du coin dans le bloc (0 ou 16 sur chaque axe), d'où ses coordonnées de texture
            let mut local = [0u8; 3];
            for (k, s) in [(axis, sign), ((axis + 1) % 3, su), ((axis + 2) % 3, sv)] {
                local[k] = if s > 0.0 { 16 } else { 0 };
            }
            let (tu, tv) = face_uv(dir, local);
            let texture = PlanetVertex::pack_texture(layer, tu, tv);
            mesh.vertices.push(PlanetVertex { position: corner.to_array(), normal: normal.to_array(), color: [color[0], color[1], color[2], 1.0], texture, light });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::OrbitCamera;
    use herocraft_world::{BlockGeometry, PlanetDescriptor};

    /// Un bloc cassé tombe au sol, puis est ramassé quand le joueur s'approche
    #[test]
    fn test_item_falls_then_is_picked_up_nearby() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut camera = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.set_terrain(std::sync::Arc::new(herocraft_world::TerrainEdits::new(None)));
        let collider = camera.collider.as_ref().unwrap();
        let geometry = BlockGeometry::new(crate::camera::DEFAULT_RADIUS as f64);
        let h = geometry.block_height as f32;
        let feet = camera.player_pos;
        let up = feet.normalize();
        let (t1, _) = crate::camera::tangent_frame(up);

        // Lâché 3 blocs au-dessus du sol, à 6 blocs du joueur : il tombe sans être ramassé
        let mut drops = DroppedItems::new();
        let mut inventory = Inventory::default();
        let start = feet + t1 * 6.0 * h + up * 3.0 * h;
        drops.spawn(BlockId::STONE, start, h);
        for _ in 0..120 {
            drops.update(1.0 / 60.0, &generator, collider, Some(feet + up * 0.9 * h), &mut inventory);
        }
        let item = drops.items()[0];
        let ground = collider.ground_radius(&generator, item.position, 0.5);
        assert!((item.position.length() - ground - ITEM_SIZE * 0.5 * h).abs() < 0.01 * h, "posé sur le sol");
        assert!(item.position.length() < start.length());
        assert_eq!(inventory.selected_block(), None);
        assert!(drops.take_mesh(h, 0.0).is_some_and(|m| m.vertices.len() == 24 && m.indices.len() == 36));

        // Le joueur s'approche à 2 blocs : l'objet est attiré puis ramassé
        let near = item.position - t1 * 2.0 * h;
        let mut picked = 0;
        for _ in 0..60 {
            picked += drops.update(1.0 / 60.0, &generator, collider, Some(near), &mut inventory);
        }
        assert_eq!(picked, 1);
        assert!(drops.items().is_empty());
        assert_eq!(inventory.selected_block(), Some(BlockId::STONE));
    }

    /// Inventaire plein : l'objet reste au sol
    #[test]
    fn test_item_stays_when_inventory_is_full() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut camera = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.set_terrain(std::sync::Arc::new(herocraft_world::TerrainEdits::new(None)));
        let collider = camera.collider.as_ref().unwrap();
        let h = BlockGeometry::new(crate::camera::DEFAULT_RADIUS as f64).block_height as f32;
        let mut inventory = Inventory::default();
        while inventory.add(BlockId::SAND) {}
        let mut drops = DroppedItems::new();
        let center = camera.player_pos + camera.player_pos.normalize() * 0.9 * h;
        drops.spawn(BlockId::STONE, center, h);
        for _ in 0..60 {
            assert_eq!(drops.update(1.0 / 60.0, &generator, collider, Some(center), &mut inventory), 0);
        }
        assert_eq!(drops.items().len(), 1);
    }

    /// Une Pierre du Néant posée ne disparaît jamais, brille, et son ramassage est signalé
    #[test]
    fn test_void_stone_stays_glows_and_signals_its_pickup() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut camera = OrbitCamera::new(6.6 * crate::camera::DEFAULT_RADIUS, crate::camera::DEFAULT_RADIUS, &generator);
        camera.set_terrain(std::sync::Arc::new(herocraft_world::TerrainEdits::new(None)));
        let collider = camera.collider.as_ref().unwrap();
        let h = BlockGeometry::new(crate::camera::DEFAULT_RADIUS as f64).block_height as f32;
        let stone = StoneKind::Whole.item().unwrap();
        let feet = camera.player_pos;
        let up = feet.normalize();
        let (t1, _) = crate::camera::tangent_frame(up);
        let mut drops = DroppedItems::new();
        let mut inventory = Inventory::default();
        drops.place(stone, feet + t1 * 20.0 * h + up * 1.0 * h);
        drops.spawn(BlockId::STONE, feet + t1 * 20.0 * h + up * 1.0 * h, h);

        // Bien après la durée de vie d'un objet ordinaire, la Pierre est toujours là
        for _ in 0..((LIFETIME + 5.0) * 10.0) as usize {
            drops.update(0.1, &generator, collider, None, &mut inventory);
        }
        assert_eq!(drops.items().len(), 1);
        assert_eq!(drops.items()[0].item, stone);
        assert!(drops.take_stone_events().is_empty(), "rien de ramassé");
        let mesh = drops.take_mesh(h, 0.0).unwrap();
        assert!(mesh.vertices.iter().all(|v| v.light == 1.0), "la Pierre luit");

        // Le joueur s'approche : elle est ramassée, et « Pierre obtenue » est signalé une fois
        let near = drops.items()[0].position - t1 * 2.0 * h;
        for _ in 0..60 {
            drops.update(1.0 / 60.0, &generator, collider, Some(near), &mut inventory);
        }
        assert_eq!(inventory.count(stone), 1);
        let events = drops.take_stone_events();
        assert!(matches!(events.as_slice(), [StoneEvent::Obtained { kind: StoneKind::Whole, item, .. }] if *item == stone));
        assert!(drops.take_stone_events().is_empty());
    }

    /// Trop d'objets au sol : le plus ancien disparaît, mais jamais une Pierre
    #[test]
    fn test_void_stone_is_never_evicted() {
        let stone = StoneKind::Whole.item().unwrap();
        let mut drops = DroppedItems::new();
        drops.place(stone, Vec3::X);
        for _ in 0..MAX_ITEMS + 10 {
            drops.lay(BlockId::STONE, Vec3::X);
        }
        assert_eq!(drops.items().len(), MAX_ITEMS);
        assert_eq!(drops.items()[0].item, stone);
    }
}

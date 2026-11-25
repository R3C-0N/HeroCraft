//! Mesure le coût par image de la simulation des créatures (partie solo : appelée à chaque image
//! sur le fil du rendu). Un joueur marche ou vole en ligne droite sur la face +Z.
//! Usage : cargo run -p herocraft_world --release --example creature_bench -- [vitesse en blocs/s] [secondes] [x] [z] [bêtes en plus]
//! Les bêtes en plus sont posées autour du point de départ, comme la phase « Beaucoup de créatures »
//! du banc d'essai du client.
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use herocraft_common::{CreatureRegistry, PlanetFace};
use herocraft_world::{BlockGeometry, CreatureWorld, PlanetDescriptor, PlanetWorldGenerator, PlayerPresence, TerrainEdits, TerrainQuery};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let speed: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let seconds: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 4242, ..Default::default() });
    let edits = TerrainEdits::new(None);
    let geometry = BlockGeometry::new(generator.base_radius());
    let registry = Arc::new(CreatureRegistry::load_default());
    let mut world = CreatureWorld::new(generator.base_radius(), 4242, registry.clone());
    world.set_sun_dir(Vec3::new(0.3, 0.2, 1.0));
    let mut query = TerrainQuery::new(&generator, &edits);
    let dt = 1.0 / 60.0;
    let frames = (seconds / dt) as usize;
    let mut times = Vec::with_capacity(frames);
    let mut x: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300.0);
    let z: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(512.0);
    let mut max_creatures = 0;
    let extra: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    for i in 0..extra {
        let angle = i as f32 * 2.399_963;
        let dist = 4.0 + (i % 12) as f32 * 1.5;
        let (bx, bz) = (x + angle.cos() * dist, z + angle.sin() * dist);
        let top = query.support_top(PlanetFace::PosZ, bx as i32, bz as i32, 255);
        let feet = geometry.corner(PlanetFace::PosZ, bx as f64, top as f64, bz as f64).as_vec3();
        world.spawn(i % registry.len(), feet);
    }
    for _ in 0..frames {
        x += speed * dt;
        let top = query.support_top(PlanetFace::PosZ, x as i32, z as i32, 255);
        let feet = geometry.corner(PlanetFace::PosZ, x as f64, top as f64, z as f64).as_vec3();
        let t = Instant::now();
        world.tick_with_presences(dt, &generator, &edits, &[PlayerPresence::standing(feet)]);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        max_creatures = max_creatures.max(world.creatures().len());
    }
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
    let over = |ms: f64| times.iter().filter(|&&t| t > ms).count();
    println!(
        "{frames} images, jusqu'à {max_creatures} bêtes : moyenne {mean:.3} ms, p50 {:.3}, p95 {:.3}, p99 {:.3}, max {:.2} ms ; {} images > 4 ms, {} > 16 ms",
        pct(0.5),
        pct(0.95),
        pct(0.99),
        sorted[sorted.len() - 1],
        over(4.0),
        over(16.0)
    );
}

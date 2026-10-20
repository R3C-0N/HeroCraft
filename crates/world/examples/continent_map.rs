//! Rend une carte équirectangulaire terre/mer de la planète (PPM) pour inspecter la forme des continents.
//! Usage : cargo run -p herocraft_world --release --example continent_map -- [seed] [sortie.ppm]
use glam::DVec3;
use herocraft_world::{PlanetDescriptor, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut desc = PlanetDescriptor::default();
    if let Some(seed) = args.get(1).and_then(|s| s.parse().ok()) {
        desc.seed = seed;
    }
    let out = args.get(2).cloned().unwrap_or_else(|| "continent_map.ppm".to_string());
    let generator = PlanetWorldGenerator::new(desc);

    let (w, h) = (1024usize, 512usize);
    let mut pixels = Vec::with_capacity(w * h * 3);
    let mut water = 0usize;
    for j in 0..h {
        let lat = std::f64::consts::FRAC_PI_2 - (j as f64 + 0.5) / h as f64 * std::f64::consts::PI;
        for i in 0..w {
            let lon = (i as f64 + 0.5) / w as f64 * std::f64::consts::TAU - std::f64::consts::PI;
            let dir = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            let elevation = generator.sample_elevation(dir, lat, lon);
            let rgb = if elevation <= SEA_LEVEL_BLOCKS {
                water += 1;
                let t = ((SEA_LEVEL_BLOCKS - elevation) / 85.0).clamp(0.0, 1.0);
                [(40.0 - 30.0 * t) as u8, (110.0 - 70.0 * t) as u8, (190.0 - 80.0 * t) as u8]
            } else {
                let t = ((elevation - SEA_LEVEL_BLOCKS) / 200.0).clamp(0.0, 1.0);
                [(70.0 + 150.0 * t) as u8, (150.0 + 60.0 * t) as u8, (60.0 + 160.0 * t) as u8]
            };
            pixels.extend_from_slice(&rgb);
        }
    }
    let mut f = std::fs::File::create(&out).expect("création du fichier");
    write!(f, "P6\n{} {}\n255\n", w, h).unwrap();
    f.write_all(&pixels).unwrap();
    println!("{} écrit, eau = {:.1}%", out, water as f64 / (w * h) as f64 * 100.0);
}

//! Mesure des délais entre deux joueurs sur un serveur local : position relayée et bloc cassé.
//! cargo run --release -p herocraft_server --example latence

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use herocraft_common::net::Connection;
use herocraft_common::{BlockId, BlockInteractionAction, ClientPacket, PlanetFace, ServerPacket, PROTOCOL_VERSION};
use herocraft_server::{Server, ServerConfig};
use herocraft_world::{BlockGeometry, BlockPos, PlanetDescriptor, PlanetWorldGenerator, WORLD_HEIGHT};

type Client = Connection<ClientPacket, ServerPacket>;

fn connect(addr: &str, name: &str) -> Client {
    let c = Client::connect(addr, Duration::from_secs(5)).unwrap();
    c.send(&ClientPacket::Connect { player_name: name.into(), protocol_version: PROTOCOL_VERSION, skills: herocraft_common::skills().fingerprint() });
    c
}

fn main() {
    let mut config = ServerConfig::new("127.0.0.1:0", 2026);
    config.save_dir = None;
    let server = Server::bind(config).unwrap();
    let addr = server.local_addr().unwrap().to_string();
    let face = PlanetFace::PosZ;
    let (x, z, top) = (0..herocraft_world::FACE_BLOCKS).step_by(37).flat_map(|x| (0..herocraft_world::FACE_BLOCKS).step_by(37).map(move |z| (x, z))).find_map(|(x, z)| {
        let top = (0..WORLD_HEIGHT).rev().find(|&y| !server.block(BlockPos::new(face, x, y, z)).is_air())?;
        (server.block(BlockPos::new(face, x, top, z)) != BlockId::WATER).then_some((x, z, top))
    }).unwrap();
    let geometry = BlockGeometry::new(PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() }).base_radius());
    let feet = geometry.corner(face, x as f64 + 0.5, (top + 1) as f64, z as f64 + 0.5).as_vec3();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let t = std::thread::spawn(move || { let mut s = server; s.run(&r); });

    let a = connect(&addr, "A");
    let b = connect(&addr, "B");
    std::thread::sleep(Duration::from_millis(300));
    while a.try_recv().ok().flatten().is_some() {}
    while b.try_recv().ok().flatten().is_some() {}

    // Position : A bouge, combien de temps avant que B le voie ?
    let mut pos_delays = Vec::new();
    for i in 0..40 {
        let p = feet + glam::Vec3::splat(i as f32 * 0.01);
        let sent = Instant::now();
        a.send(&ClientPacket::PlayerMove { position: p, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false, holding: None });
        loop {
            match b.recv_timeout(Duration::from_secs(10)).unwrap() {
                ServerPacket::PlayerStates(s) if s.iter().any(|s| s.position == p) => break,
                _ => {}
            }
        }
        pos_delays.push(sent.elapsed());
        std::thread::sleep(Duration::from_millis(37));
    }
    // Blocs : A casse des blocs de la colonne, combien de temps avant que B le reçoive ?
    let mut block_delays = Vec::new();
    for (i, dx) in [0, 1, 2, 3, 40, 80].into_iter().enumerate() {
        let pos = BlockPos::new(face, x + dx, top - i as i32 % 2, z);
        let p = geometry.corner(face, (x + dx) as f64 + 0.5, (top + 1) as f64, z as f64 + 0.5).as_vec3();
        a.send(&ClientPacket::PlayerMove { position: p, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false, holding: None });
        std::thread::sleep(Duration::from_millis(100));
        let sent = Instant::now();
        a.send(&ClientPacket::InteractBlock { block: pos.into(), action: BlockInteractionAction::Break });
        loop {
            match b.recv_timeout(Duration::from_secs(30)).unwrap() {
                ServerPacket::VoxelUpdate { block, .. } if block == pos.into() => break,
                _ => {}
            }
        }
        block_delays.push(sent.elapsed());
    }
    running.store(false, Ordering::Relaxed);
    t.join().unwrap();
    let ms = |d: &Duration| d.as_secs_f64() * 1000.0;
    let mut p: Vec<f64> = pos_delays.iter().map(ms).collect();
    p.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("position : min {:.1} ms, médiane {:.1} ms, max {:.1} ms", p[0], p[p.len() / 2], p[p.len() - 1]);
    println!("blocs    : {:?} ms", block_delays.iter().map(|d| (ms(d) * 10.0).round() / 10.0).collect::<Vec<_>>());
}

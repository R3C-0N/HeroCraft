use herocraft_common::{CharacterStats, PlanetChunkCoord, PlanetFace, PlayerClass};
use herocraft_world::{PlanetDescriptor, PlanetWorldGenerator};
use log::info;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct ConnectedPlayer {
    pub id: u64,
    pub name: String,
    pub stats: CharacterStats,
    pub position: glam::Vec3,
}

pub struct ServerWorldState {
    pub generator: PlanetWorldGenerator,
    pub loaded_chunks: HashMap<PlanetChunkCoord, herocraft_voxel_engine::ChunkData>,
    pub players: HashMap<u64, ConnectedPlayer>,
    pub tick_counter: u64,
}

impl ServerWorldState {
    pub fn new(seed: u64) -> Self {
        let mut planet = PlanetDescriptor::default();
        planet.seed = seed;
        let generator = PlanetWorldGenerator::new(planet);

        Self {
            generator,
            loaded_chunks: HashMap::new(),
            players: HashMap::new(),
            tick_counter: 0,
        }
    }

    pub fn preload_spawn_area(&mut self) {
        info!("Préchargement des chunks de spawn sur la face PosZ...");
        for y in -1..=1 {
            for x in -1..=1 {
                let coord = PlanetChunkCoord::new(PlanetFace::PosZ, x, y, 0);
                let chunk = self.generator.generate_chunk(&coord);
                self.loaded_chunks.insert(coord, chunk);
            }
        }
        info!("Spawn prêt avec {} chunks chargés en mémoire.", self.loaded_chunks.len());
    }

    pub fn tick(&mut self, dt: f32) {
        self.tick_counter += 1;

        // Mise à jour de la régénération des stats des joueurs
        for player in self.players.values_mut() {
            // Régénération naturelle de stamina (+10 par sec) et de mana (+2 par sec)
            player.stats.stamina = (player.stats.stamina + 10.0 * dt).min(player.stats.max_stamina);
            player.stats.mana = (player.stats.mana + 2.0 * dt).min(player.stats.max_mana);
        }

        if self.tick_counter % 300 == 0 {
            info!(
                "Tick #{}: {} joueur(s) en ligne, {} chunks actifs.",
                self.tick_counter,
                self.players.len(),
                self.loaded_chunks.len()
            );
        }
    }
}

fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    info!("==========================================");
    info!("   HeroCraft - Serveur Dédié Voxel RPG    ");
    info!("==========================================");

    let mut server = ServerWorldState::new(2026);
    server.preload_spawn_area();

    // Ajout d'un joueur test pour la simulation
    let test_player = ConnectedPlayer {
        id: 1,
        name: "HérosTesteur".to_string(),
        stats: CharacterStats::new(PlayerClass::Warrior),
        position: glam::Vec3::new(0.0, 30.0, 0.0),
    };
    server.players.insert(test_player.id, test_player);

    info!("Démarrage de la boucle de jeu autoritaire à 60 TPS...");
    let tick_rate = Duration::from_micros(1_000_000 / 60);

    // Boucle de simulation (effectue quelques ticks d'initialisation en démo)
    let start_time = Instant::now();
    for _ in 0..120 {
        let tick_start = Instant::now();
        server.tick(1.0 / 60.0);

        let elapsed = tick_start.elapsed();
        if elapsed < tick_rate {
            std::thread::sleep(tick_rate - elapsed);
        }
    }

    info!(
        "Simulation de démarrage réussie (120 ticks en {:.2}s). Serveur prêt à recevoir des connexions !",
        start_time.elapsed().as_secs_f32()
    );
}

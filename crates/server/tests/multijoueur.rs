//! Clients sur un vrai serveur TCP local : accueil, blocs diffusés, actions refusées, positions,
//! créatures montrées aux joueurs proches et frappées.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use herocraft_common::net::Connection;
use glam::Vec3;
use herocraft_common::{BlockId, BlockInteractionAction, ClientPacket, PlanetFace, ServerPacket, PROTOCOL_VERSION};
use herocraft_server::{Server, ServerConfig};
use herocraft_world::{BlockGeometry, BlockPos, WORLD_HEIGHT};

type Client = Connection<ClientPacket, ServerPacket>;

const WAIT: Duration = Duration::from_secs(10);

fn connect(addr: &str, name: &str) -> (Client, u64, u32) {
    let client = Client::connect(addr, WAIT).unwrap();
    client.send(&ClientPacket::Connect { player_name: name.into(), protocol_version: PROTOCOL_VERSION });
    match client.recv_timeout(WAIT).unwrap() {
        ServerPacket::ConnectAck { player_id, world_seed, edited_chunks, .. } => {
            assert_eq!(world_seed, 4242);
            (client, player_id, edited_chunks)
        }
        other => panic!("ConnectAck attendu, reçu {other:?}"),
    }
}

/// Premier paquet qui satisfait `pick`, les autres (stats, chat…) étant ignorés
fn wait_for<T>(client: &Client, mut pick: impl FnMut(ServerPacket) -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let packet = client.recv_timeout(left).expect("paquet attendu non reçu");
        if let Some(found) = pick(packet) {
            return found;
        }
    }
}

fn test_server() -> Server {
    let mut config = ServerConfig::new("127.0.0.1:0", 4242);
    config.save_dir = None;
    Server::bind(config).unwrap()
}

/// Bloc de surface d'une colonne de terre ferme, et les pieds d'un joueur posé dessus
fn land(server: &Server) -> (BlockPos, Vec3) {
    let face = PlanetFace::PosZ;
    let (x, z, top) = (0..herocraft_world::FACE_BLOCKS)
        .step_by(37)
        .flat_map(|x| (0..herocraft_world::FACE_BLOCKS).step_by(37).map(move |z| (x, z)))
        .find_map(|(x, z)| {
            let top = (0..WORLD_HEIGHT).rev().find(|&y| !server.block(BlockPos::new(face, x, y, z)).is_air())?;
            (server.block(BlockPos::new(face, x, top, z)) != BlockId::WATER).then_some((x, z, top))
        })
        .expect("une colonne de terre ferme");
    let surface = BlockPos::new(face, x, top, z);
    let geometry = BlockGeometry::new(herocraft_world::PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 4242, ..Default::default() }).base_radius());
    let feet = geometry.corner(face, x as f64 + 0.5, (top + 1) as f64, z as f64 + 0.5).as_vec3();
    (surface, feet)
}

#[test]
fn deux_joueurs_partagent_les_blocs_et_les_positions() {
    let server = test_server();
    let addr = server.local_addr().unwrap().to_string();
    let (surface, feet) = land(&server);
    let (face, x, z) = (surface.face, surface.x, surface.z);

    let running = Arc::new(AtomicBool::new(true));
    let server_running = running.clone();
    let server_thread = std::thread::spawn(move || {
        let mut server = server;
        server.run(&server_running);
    });

    let (alice, alice_id, chunks) = connect(&addr, "Alice");
    assert_eq!(chunks, 0);
    let (bob, bob_id, _) = connect(&addr, "Bob");
    assert_ne!(alice_id, bob_id);
    // Bob apprend qu'Alice est là, Alice apprend l'arrivée de Bob
    wait_for(&bob, |p| matches!(p, ServerPacket::PlayerJoined { player_id, ref name } if player_id == alice_id && name == "Alice").then_some(()));
    wait_for(&alice, |p| matches!(p, ServerPacket::PlayerJoined { player_id, .. } if player_id == bob_id).then_some(()));

    // Un deuxième « Alice » est refusé
    let dup = Client::connect(&addr, WAIT).unwrap();
    dup.send(&ClientPacket::Connect { player_name: "alice".into(), protocol_version: PROTOCOL_VERSION });
    assert!(matches!(dup.recv_timeout(WAIT).unwrap(), ServerPacket::Disconnect(_)));

    // Sans position connue, Alice ne peut rien casser : le serveur lui renvoie le vrai bloc
    alice.send(&ClientPacket::InteractBlock { block: surface.into(), action: BlockInteractionAction::Break });
    let restored = wait_for(&alice, |p| match p {
        ServerPacket::VoxelUpdate { block, block_id } if block == surface.into() => Some(block_id),
        _ => None,
    });
    assert_ne!(restored, 0);

    // Posée sur la colonne, Alice casse le bloc sous ses pieds : les deux joueurs le voient
    alice.send(&ClientPacket::PlayerMove { position: feet, yaw: 0.3, pitch: -0.2, sprinting: false, crouching: false });
    alice.send(&ClientPacket::InteractBlock { block: surface.into(), action: BlockInteractionAction::Break });
    for client in [&alice, &bob] {
        let id = wait_for(client, |p| match p {
            ServerPacket::VoxelUpdate { block, block_id } if block == surface.into() => Some(block_id),
            _ => None,
        });
        assert_eq!(id, 0);
    }

    // Bob reçoit la position d'Alice
    let state = wait_for(&bob, |p| match p {
        ServerPacket::PlayerStates(states) => states.into_iter().find(|s| s.player_id == alice_id),
        _ => None,
    });
    assert!(state.position.distance(feet) < 1e-3);

    // Un bloc hors de portée (en plein ciel, loin de Bob) est refusé
    let far = BlockPos::new(face, (x + 200) % herocraft_world::FACE_BLOCKS, WORLD_HEIGHT - 1, z);
    bob.send(&ClientPacket::PlayerMove { position: feet, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false });
    bob.send(&ClientPacket::InteractBlock { block: far.into(), action: BlockInteractionAction::Place(1) });
    let id = wait_for(&bob, |p| match p {
        ServerPacket::VoxelUpdate { block, block_id } if block == far.into() => Some(block_id),
        _ => None,
    });
    assert_eq!(id, 0, "le bloc lointain ne doit pas être posé");

    // Un nouveau venu reçoit le chunk modifié
    let (_carol, _, chunks) = connect(&addr, "Carol");
    assert_eq!(chunks, 1);

    // Une chute blesse Alice, qui reçoit aussitôt ses jauges ; une chute mortelle la ranime
    alice.send(&ClientPacket::Fall { blocks: herocraft_common::SAFE_FALL_BLOCKS + 3.0 });
    let stats = wait_for(&alice, |p| match p {
        ServerPacket::StatsUpdate(stats) if stats.hp < stats.max_hp - 10.0 => Some(stats),
        _ => None,
    });
    assert!(stats.hp > 0.0);
    alice.send(&ClientPacket::Fall { blocks: 1000.0 });
    wait_for(&bob, |p| matches!(p, ServerPacket::ChatMessage { ref message, .. } if message == "Alice s'est écrasé au sol").then_some(()));

    // Alice part : Bob est prévenu
    alice.send(&ClientPacket::Disconnect);
    wait_for(&bob, |p| matches!(p, ServerPacket::PlayerLeft { player_id } if player_id == alice_id).then_some(()));

    running.store(false, Ordering::Relaxed);
    server_thread.join().unwrap();
    // Le serveur prévient les joueurs restants de son arrêt
    wait_for(&bob, |p| matches!(p, ServerPacket::Disconnect(_)).then_some(()));
}

/// Fait tourner le serveur à la main jusqu'à ce qu'un paquet reçu par `client` satisfasse `pick`
fn pump<T>(server: &mut Server, client: &Client, mut pick: impl FnMut(ServerPacket) -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        server.tick(1.0 / 60.0);
        while let Some(packet) = client.try_recv().expect("connexion fermée") {
            if let Some(found) = pick(packet) {
                return found;
            }
        }
        assert!(Instant::now() < deadline, "paquet attendu non reçu");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn les_creatures_proches_sont_montrees_au_joueur() {
    let mut server = test_server();
    let addr = server.local_addr().unwrap().to_string();
    let (_, feet) = land(&server);

    let alice = Client::connect(&addr, WAIT).unwrap();
    alice.send(&ClientPacket::Connect { player_name: "Alice".into(), protocol_version: PROTOCOL_VERSION });
    pump(&mut server, &alice, |p| matches!(p, ServerPacket::ConnectAck { .. }).then_some(()));
    alice.send(&ClientPacket::PlayerMove { position: feet, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false });
    let deadline = Instant::now() + WAIT;
    while server.players().all(|p| p.position.is_none()) {
        assert!(Instant::now() < deadline, "position d'Alice non reçue");
        server.tick(1.0 / 60.0);
        std::thread::sleep(Duration::from_millis(2));
    }

    // Une vache à ses pieds : il lui est annoncé, puis sa position suit
    let sheep = server.spawn_creature("vache", feet).expect("espèce vache");
    let (species, state) = pump(&mut server, &alice, |p| match p {
        ServerPacket::CreatureSpawned { species, state } if state.creature_id == sheep => Some((species, state)),
        _ => None,
    });
    assert_eq!(species, "vache");
    assert!(state.position.distance(feet) < 1e-3);
    pump(&mut server, &alice, |p| match p {
        ServerPacket::CreatureStates(states) => states.into_iter().find(|s| s.creature_id == sheep),
        _ => None,
    });

    // Alice la frappe : la vache rougit et recule
    alice.send(&ClientPacket::HitCreature { creature_id: sheep });
    let hurt = pump(&mut server, &alice, |p| match p {
        ServerPacket::CreatureStates(states) => states.into_iter().find(|s| s.creature_id == sheep && s.hurt > 0.0),
        _ => None,
    });
    assert_eq!(hurt.death, 0.0);
    pump(&mut server, &alice, |p| match p {
        ServerPacket::CreatureStates(states) => states.into_iter().find(|s| s.creature_id == sheep && s.position.distance(feet) > 0.1),
        _ => None,
    });

    // Alice part à l'autre bout de la planète : la vache, seule, disparaît de son affichage
    alice.send(&ClientPacket::PlayerMove { position: -feet, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false });
    pump(&mut server, &alice, |p| match p {
        ServerPacket::CreaturesDespawned(ids) => ids.contains(&sheep).then_some(()),
        _ => None,
    });
    assert!(server.creatures().creatures().iter().all(|c| c.id != sheep));
}

#[test]
fn l_experience_donne_des_points_que_la_roue_juge() {
    let mut server = test_server();
    let addr = server.local_addr().unwrap().to_string();
    let client = Client::connect(&addr, WAIT).unwrap();
    client.send(&ClientPacket::Connect { player_name: "Alice".into(), protocol_version: PROTOCOL_VERSION });
    pump(&mut server, &client, |p| matches!(p, ServerPacket::ConnectAck { .. }).then_some(()));

    // Sans point, la roue refuse : les statistiques reviennent inchangées
    client.send(&ClientPacket::LearnSkill { id: "frappeLourde".into() });
    let stats = pump(&mut server, &client, |p| match p {
        ServerPacket::StatsUpdate(stats) => Some(stats),
        _ => None,
    });
    assert!(stats.skills.is_empty(), "appris sans point : {:?}", stats.skills);

    // /xp : de quoi atteindre le niveau 3 (100 + 200), soit deux points
    client.send(&ClientPacket::GainExperience(300));
    pump(&mut server, &client, |p| matches!(p, ServerPacket::StatsUpdate(stats) if stats.skill_points == 2).then_some(()));

    client.send(&ClientPacket::LearnSkill { id: "frappeLourde".into() });
    let stats = pump(&mut server, &client, |p| match p {
        ServerPacket::StatsUpdate(stats) if stats.skill_points < 2 => Some(stats),
        _ => None,
    });
    assert_eq!(stats.skills.get("frappeLourde"), Some(&1));
    // Tornade écrasante est une fusion du niveau 2 : refusée, le point reste
    client.send(&ClientPacket::LearnSkill { id: "tornadeEcrasante".into() });
    server.tick(1.0 / 60.0);
    let stats = pump(&mut server, &client, |p| match p {
        ServerPacket::StatsUpdate(stats) => Some(stats),
        _ => None,
    });
    assert_eq!(stats.skill_points, 1);
    assert!(!stats.skills.contains_key("tornadeEcrasante"));
}

#[test]
fn la_legende_donne_sa_voie_et_une_legende_truquee_est_refusee() {
    use herocraft_common::hero::{atlas, Legend};
    use herocraft_common::PlayerClass;

    let server = test_server();
    let addr = server.local_addr().unwrap().to_string();
    let running = Arc::new(AtomicBool::new(true));
    let server_running = running.clone();
    let server_thread = std::thread::spawn(move || {
        let mut server = server;
        server.run(&server_running);
    });

    let (client, _, _) = connect(&addr, "Bran");
    let mut appearance = atlas().defaut("gnome");
    appearance.nom = "Brandin".into();
    // Un gnome de deux mètres n'existe pas : le serveur ne change rien
    let mut truquee = Legend { appearance: appearance.clone(), class: PlayerClass::Archer };
    truquee.appearance.taille = 2.0;
    client.send(&ClientPacket::Legend(Box::new(truquee)));
    let legend = Legend { appearance, class: PlayerClass::Druid };
    client.send(&ClientPacket::Legend(Box::new(legend)));
    let class = wait_for(&client, |p| match p {
        ServerPacket::StatsUpdate(stats) if stats.class != PlayerClass::Adventurer => Some(stats.class),
        _ => None,
    });
    assert_eq!(class, PlayerClass::Druid);

    running.store(false, Ordering::SeqCst);
    server_thread.join().unwrap();
}

#[test]
fn le_chat_et_les_commandes_passent_par_le_serveur() {
    let mut server = test_server();
    let addr = server.local_addr().unwrap().to_string();
    let (_, feet) = land(&server);

    let join = |server: &mut Server, name: &str| {
        let client = Client::connect(&addr, WAIT).unwrap();
        client.send(&ClientPacket::Connect { player_name: name.into(), protocol_version: PROTOCOL_VERSION });
        pump(server, &client, |p| matches!(p, ServerPacket::ConnectAck { .. }).then_some(()));
        client
    };
    let alice = join(&mut server, "Alice");
    let bob = join(&mut server, "Bob");
    alice.send(&ClientPacket::PlayerMove { position: feet, yaw: 0.0, pitch: 0.0, sprinting: false, crouching: false });

    // Un message d'Alice arrive à Bob, et à Alice elle-même
    alice.send(&ClientPacket::ChatMessage("  salut Bob  ".into()));
    let chat = |p: ServerPacket| match p {
        ServerPacket::ChatMessage { sender, message } if !sender.is_empty() => Some((sender, message)),
        _ => None,
    };
    assert_eq!(pump(&mut server, &bob, chat), ("Alice".to_string(), "salut Bob".to_string()));
    assert_eq!(pump(&mut server, &alice, chat), ("Alice".to_string(), "salut Bob".to_string()));

    // /summon : trois loups à ses pieds, la réponse ne va qu'à elle
    let before = server.creatures().creatures().len();
    alice.send(&ClientPacket::Summon { species: "loup".into(), count: 3, feet });
    let reply = |p: ServerPacket| match p {
        ServerPacket::ChatMessage { sender, message } if sender.is_empty() && !message.contains("rejoint") => Some(message),
        _ => None,
    };
    assert!(pump(&mut server, &alice, reply).starts_with("Summoned 3"));
    assert_eq!(server.creatures().creatures().len(), before + 3);
    // Trop loin d'elle : refusé
    alice.send(&ClientPacket::Summon { species: "loup".into(), count: 1, feet: feet * 2.0 });
    assert!(pump(&mut server, &alice, reply).contains("Too far"));

    // /xp : de l'expérience, et le niveau qui va avec
    alice.send(&ClientPacket::GainExperience(100_000));
    pump(&mut server, &alice, |p| matches!(p, ServerPacket::StatsUpdate(stats) if stats.level > 1).then_some(()));

    // Console du serveur
    assert_eq!(server.console_command("list"), vec!["2 player(s) online: Alice, Bob".to_string()]);
    assert_eq!(server.console_command("seed"), vec!["Seed: 4242".to_string()]);
    server.console_command("say Bonjour");
    assert_eq!(pump(&mut server, &bob, chat), ("Server".to_string(), "Bonjour".to_string()));
}

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use herocraft_common::DEFAULT_PORT;
use herocraft_server::{Server, ServerConfig, TICK_RATE};
use log::{error, info, warn};

const USAGE: &str = "Usage : herocraft_server [--bind ADRESSE:PORT] [--port PORT] [--seed GRAINE] [--name NOM] [--no-lan]";

fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let mut bind = format!("0.0.0.0:{DEFAULT_PORT}");
    let mut seed = 2026_u64;
    let mut name = "Serveur HeroCraft".to_string();
    let mut lan = true;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| exit_with_usage(&format!("valeur manquante après {arg}")));
        match arg.as_str() {
            "--bind" => bind = value(),
            "--port" => {
                let port: u16 = value().parse().unwrap_or_else(|_| exit_with_usage("port invalide"));
                bind = format!("0.0.0.0:{port}");
            }
            "--seed" => seed = value().parse().unwrap_or_else(|_| exit_with_usage("graine invalide")),
            "--name" => name = value(),
            "--no-lan" => lan = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            other => exit_with_usage(&format!("argument inconnu : {other}")),
        }
    }

    info!("==========================================");
    info!("   HeroCraft - Serveur Dédié Voxel RPG    ");
    info!("==========================================");

    let mut server = match Server::bind(ServerConfig::new(&bind, seed)) {
        Ok(server) => server,
        Err(e) => {
            error!("Impossible d'écouter sur {bind} : {e}");
            std::process::exit(1);
        }
    };
    if lan {
        match server.enable_lan(name.as_str()) {
            Ok(()) => info!("Partie « {name} » annoncée sur le réseau local."),
            Err(e) => warn!("Annonce sur le réseau local impossible : {e}"),
        }
    }
    info!("Planète de graine #{seed}, sauvegarde dans saves/serveur_monde_{seed}/.");
    info!("En écoute sur {} à {TICK_RATE} TPS. Tapez « stop » pour arrêter le serveur proprement, « help » pour les autres commandes.", server.local_addr().map(|a| a.to_string()).unwrap_or(bind));

    // Console : « stop » arrête la boucle, qui enregistre le monde avant de rendre la main
    let running = Arc::new(AtomicBool::new(true));
    let console_running = running.clone();
    let (console_tx, console_rx) = std::sync::mpsc::channel();
    server.set_console(console_rx);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match line.trim() {
                "stop" | "quit" | "exit" => {
                    console_running.store(false, Ordering::Relaxed);
                    break;
                }
                "" => {}
                // Les autres commandes (help, list, seed, say) passent par la boucle du serveur
                other => {
                    if console_tx.send(other.to_string()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    server.run(&running);
    info!("Serveur arrêté, monde enregistré.");
}

fn exit_with_usage(message: &str) -> ! {
    eprintln!("{message}\n{USAGE}");
    std::process::exit(2);
}

//! Découverte des parties sur le réseau local : chaque serveur annonce sa partie en UDP
//! (diffusion sur le réseau et boucle locale) toutes les secondes, et le client qui ouvre la page
//! Multijoueur écoute ces annonces.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::protocol::PROTOCOL_VERSION;

/// Port UDP des annonces
pub const LAN_DISCOVERY_PORT: u16 = 7778;
/// Une partie qui ne s'est plus annoncée depuis ce délai disparaît de la liste
const LAN_EXPIRY: Duration = Duration::from_secs(4);
const MAGIC: [u8; 8] = *b"HEROCRFT";

/// Annonce d'une partie
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanAnnounce {
    magic: [u8; 8],
    pub protocol_version: u32,
    /// Identifiant tiré au démarrage du serveur : la même partie reçue par deux chemins (réseau et
    /// boucle locale) n'apparaît qu'une fois
    pub instance: u64,
    pub name: String,
    /// Port TCP du jeu
    pub port: u16,
    pub players: u32,
    pub seed: u64,
}

impl LanAnnounce {
    pub fn new(instance: u64, name: String, port: u16, players: u32, seed: u64) -> Self {
        Self { magic: MAGIC, protocol_version: PROTOCOL_VERSION, instance, name, port, players, seed }
    }
}

/// Côté serveur : envoie les annonces
pub struct LanAnnouncer {
    socket: UdpSocket,
}

impl LanAnnouncer {
    pub fn new() -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }

    /// Diffuse l'annonce sur le réseau local et sur cette machine (les erreurs, par exemple sans
    /// réseau, sont ignorées : l'annonce suivante retentera)
    pub fn announce(&self, announce: &LanAnnounce) {
        let Ok(bytes) = bincode::serialize(announce) else { return };
        let _ = self.socket.send_to(&bytes, (Ipv4Addr::BROADCAST, LAN_DISCOVERY_PORT));
        let _ = self.socket.send_to(&bytes, (Ipv4Addr::LOCALHOST, LAN_DISCOVERY_PORT));
    }
}

/// Partie trouvée sur le réseau local
#[derive(Debug, Clone, PartialEq)]
pub struct LanServer {
    /// Adresse TCP à laquelle se connecter
    pub addr: SocketAddr,
    pub name: String,
    pub players: u32,
    pub seed: u64,
    instance: u64,
    last_seen: Instant,
}

/// Côté client : écoute les annonces, le temps que la page Multijoueur est ouverte
pub struct LanBrowser {
    socket: UdpSocket,
    servers: Vec<LanServer>,
}

impl LanBrowser {
    pub fn new() -> io::Result<Self> {
        Self::bind(LAN_DISCOVERY_PORT)
    }

    fn bind(port: u16) -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, servers: Vec::new() })
    }

    /// Relève les annonces reçues et renvoie les parties encore actives, par nom
    pub fn poll(&mut self) -> &[LanServer] {
        let mut buf = [0u8; 1024];
        let now = Instant::now();
        while let Ok((len, from)) = self.socket.recv_from(&mut buf) {
            let Ok(announce) = bincode::deserialize::<LanAnnounce>(&buf[..len]) else { continue };
            if announce.magic != MAGIC || announce.protocol_version != PROTOCOL_VERSION {
                continue;
            }
            let addr = SocketAddr::new(from.ip(), announce.port);
            match self.servers.iter_mut().find(|s| s.instance == announce.instance) {
                Some(server) => {
                    // Une partie vue par le réseau et la boucle locale garde son adresse réseau,
                    // joignable aussi depuis cette machine
                    if server.addr.ip().is_loopback() || !addr.ip().is_loopback() {
                        server.addr = addr;
                    }
                    server.name = announce.name;
                    server.players = announce.players;
                    server.last_seen = now;
                }
                None => self.servers.push(LanServer {
                    addr,
                    name: announce.name,
                    players: announce.players,
                    seed: announce.seed,
                    instance: announce.instance,
                    last_seen: now,
                }),
            }
        }
        self.servers.retain(|s| now.duration_since(s.last_seen) < LAN_EXPIRY);
        self.servers.sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
        &self.servers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_announce_is_found_once_by_the_browser() {
        // Port libre pour le test, l'annonce lui étant envoyée directement
        let mut browser = LanBrowser::bind(0).unwrap();
        let port = browser.socket.local_addr().unwrap().port();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let bytes = bincode::serialize(&LanAnnounce::new(42, "Monde de Mat".into(), 7777, 2, 2026)).unwrap();
        // Deux fois la même partie, et un paquet étranger
        sender.send_to(&bytes, (Ipv4Addr::LOCALHOST, port)).unwrap();
        sender.send_to(&bytes, (Ipv4Addr::LOCALHOST, port)).unwrap();
        sender.send_to(b"bonjour", (Ipv4Addr::LOCALHOST, port)).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while browser.poll().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        let servers = browser.poll();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Monde de Mat");
        assert_eq!(servers[0].addr, SocketAddr::from((Ipv4Addr::LOCALHOST, 7777)));
        assert_eq!(servers[0].players, 2);
    }
}

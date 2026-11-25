//! Transport réseau : des paquets postcard précédés de leur taille (u32 petit-boutiste) sur TCP.
//!
//! Une `Connection` lit et écrit chacune sur son propre fil : le fil de jeu (boucle du serveur ou
//! frame du client) ne bloque jamais sur le réseau. Il dépose les paquets à envoyer et relève ceux
//! reçus sans attendre.

use std::io::{self, BufReader, BufWriter, ErrorKind, Read, Write};
use std::marker::PhantomData;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Taille maximale d'un paquet : au-delà, la connexion est jugée corrompue
pub(crate) const MAX_PACKET_BYTES: usize = 16 * 1024 * 1024;

/// Encode un paquet dans une trame prête à envoyer (taille puis contenu) : un paquet diffusé à
/// tous les joueurs n'est encodé qu'une fois
pub fn encode_frame<T: Serialize>(packet: &T) -> Vec<u8> {
    // Le contenu s'écrit directement derrière la place réservée à la taille, remplie ensuite
    let mut frame = postcard::to_extend(packet, vec![0u8; 4]).expect("paquet sérialisable");
    let len = (frame.len() - 4) as u32;
    frame[..4].copy_from_slice(&len.to_le_bytes());
    frame
}

/// Lit une trame et la décode
pub(crate) fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<T> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_PACKET_BYTES {
        return Err(io::Error::new(ErrorKind::InvalidData, format!("paquet de {len} octets, trop gros")));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    postcard::from_bytes(&body).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
}

/// Pourquoi une connexion ne donne plus de paquet
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionError {
    #[error("délai d'attente dépassé")]
    Timeout,
    /// Le fil de jeu a lâché la file de réception
    #[error("connexion abandonnée")]
    Abandoned,
    #[error("connexion fermée par le pair")]
    PeerClosed,
    #[error("erreur de lecture : {0}")]
    Read(#[source] Arc<io::Error>),
    #[error("erreur d'écriture : {0}")]
    Write(#[source] Arc<io::Error>),
    #[error("connexion fermée")]
    Closed,
}

/// Connexion TCP vers un pair : `Out` les paquets envoyés, `In` les paquets reçus
pub struct Connection<Out, In> {
    stream: TcpStream,
    peer: SocketAddr,
    outgoing: Sender<Vec<u8>>,
    /// Paquets reçus, datés à leur arrivée par le fil de lecture
    incoming: Receiver<(Instant, In)>,
    /// Raison de la fermeture, écrite par le fil de lecture ou d'écriture qui l'a constatée
    closed: Arc<Mutex<Option<ConnectionError>>>,
    _out: PhantomData<fn(Out)>,
}

impl<Out: Serialize, In: DeserializeOwned + Send + 'static> Connection<Out, In> {
    /// Se connecte à `addr` (ex. `127.0.0.1:7777`)
    pub fn connect(addr: &str, timeout: Duration) -> io::Result<Self> {
        use std::net::ToSocketAddrs;
        let mut last_err = io::Error::new(ErrorKind::NotFound, format!("adresse introuvable : {addr}"));
        for sock_addr in addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sock_addr, timeout) {
                Ok(stream) => return Self::from_stream(stream),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// Prend en charge une connexion déjà établie (côté serveur : celle acceptée par l'écoute)
    pub fn from_stream(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(false)?;
        // Les paquets sont petits et fréquents : pas d'attente de Nagle
        stream.set_nodelay(true)?;
        let peer = stream.peer_addr()?;
        let closed = Arc::new(Mutex::new(None));

        let (in_tx, incoming) = mpsc::channel();
        let read_stream = stream.try_clone()?;
        let read_closed = closed.clone();
        thread::Builder::new().name(format!("net-lecture-{peer}")).spawn(move || {
            let mut reader = BufReader::new(read_stream);
            let reason = loop {
                match read_frame::<_, In>(&mut reader) {
                    Ok(packet) => {
                        if in_tx.send((Instant::now(), packet)).is_err() {
                            break ConnectionError::Abandoned;
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::UnexpectedEof => break ConnectionError::PeerClosed,
                    Err(e) => break ConnectionError::Read(Arc::new(e)),
                }
            };
            read_closed.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert(reason);
        })?;

        let (outgoing, out_rx) = mpsc::channel::<Vec<u8>>();
        let write_stream = stream.try_clone()?;
        let write_closed = closed.clone();
        thread::Builder::new().name(format!("net-ecriture-{peer}")).spawn(move || {
            let mut writer = BufWriter::new(write_stream);
            // Les trames en attente partent ensemble, vidées une fois quand la file est vide
            while let Ok(frame) = out_rx.recv() {
                let mut result = writer.write_all(&frame);
                while result.is_ok() {
                    match out_rx.try_recv() {
                        Ok(frame) => result = writer.write_all(&frame),
                        Err(_) => break,
                    }
                }
                if let Err(e) = result.and_then(|_| writer.flush()) {
                    write_closed.lock().unwrap_or_else(PoisonError::into_inner).get_or_insert(ConnectionError::Write(Arc::new(e)));
                    let _ = writer.get_ref().shutdown(Shutdown::Both);
                    return;
                }
            }
            // Connexion lâchée : ce qui restait est parti, le pair voit la fin de flux
            let _ = writer.flush();
            let _ = writer.get_ref().shutdown(Shutdown::Both);
        })?;

        Ok(Self { stream, peer, outgoing, incoming, closed, _out: PhantomData })
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
    }

    /// Met un paquet en file d'envoi
    pub fn send(&self, packet: &Out) {
        self.send_frame(encode_frame(packet));
    }

    /// Met en file une trame déjà encodée par `encode_frame`
    pub fn send_frame(&self, frame: Vec<u8>) {
        // Fil d'écriture arrêté : la fermeture est déjà notée et sera vue par `try_recv`
        let _ = self.outgoing.send(frame);
    }

    /// Paquet reçu suivant sans attendre : `Ok(None)` si aucun, `Err` si la connexion est fermée
    pub fn try_recv(&self) -> Result<Option<In>, ConnectionError> {
        Ok(self.try_recv_timed()?.map(|(_, packet)| packet))
    }

    /// Comme `try_recv`, avec l'instant d'arrivée du paquet : il reste exact même si le fil de jeu
    /// relève ses paquets avec retard (fenêtre en arrière-plan qui dessine peu d'images)
    pub fn try_recv_timed(&self) -> Result<Option<(Instant, In)>, ConnectionError> {
        match self.incoming.try_recv() {
            Ok(packet) => Ok(Some(packet)),
            Err(TryRecvError::Empty) => match self.closed_reason() {
                // Fermée côté écriture : la lecture peut encore attendre longtemps
                Some(reason) => Err(reason),
                None => Ok(None),
            },
            Err(TryRecvError::Disconnected) => Err(self.closed_reason().unwrap_or(ConnectionError::Closed)),
        }
    }

    /// Attend le paquet suivant au plus `timeout`
    pub fn recv_timeout(&self, timeout: Duration) -> Result<In, ConnectionError> {
        match self.incoming.recv_timeout(timeout) {
            Ok((_, packet)) => Ok(packet),
            Err(RecvTimeoutError::Timeout) => Err(ConnectionError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(self.closed_reason().unwrap_or(ConnectionError::Closed)),
        }
    }

    fn closed_reason(&self) -> Option<ConnectionError> {
        self.closed.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl<Out, In> Drop for Connection<Out, In> {
    fn drop(&mut self) {
        // Le fil d'écriture vide sa file puis ferme : on ne coupe ici que la lecture
        let _ = self.stream.shutdown(Shutdown::Read);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlanetFace;
    use crate::protocol::{BlockAddress, BlockInteractionAction, ClientPacket, ServerPacket};
    use std::net::TcpListener;

    #[test]
    fn test_frame_round_trip() {
        let packet = ClientPacket::InteractBlock {
            block: BlockAddress { face: PlanetFace::NegY, x: 12, y: 300, z: 1023 },
            action: BlockInteractionAction::Place(1),
        };
        let frame = encode_frame(&packet);
        let decoded: ClientPacket = read_frame(&mut frame.as_slice()).unwrap();
        assert!(matches!(
            decoded,
            ClientPacket::InteractBlock {
                block: BlockAddress { face: PlanetFace::NegY, x: 12, y: 300, z: 1023 },
                action: BlockInteractionAction::Place(1)
            }
        ));
    }

    #[test]
    fn test_oversized_frame_is_rejected() {
        let mut frame = ((MAX_PACKET_BYTES + 1) as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&[0; 16]);
        let err = read_frame::<_, ClientPacket>(&mut frame.as_slice()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_truncated_body_is_invalid_data() {
        let frame = encode_frame(&ClientPacket::ChatMessage("un message assez long".into()));
        // La taille annoncée correspond au contenu tronqué : le décodage manque d'octets
        let mut cut = ((frame.len() - 4 - 5) as u32).to_le_bytes().to_vec();
        cut.extend_from_slice(&frame[4..frame.len() - 5]);
        let err = read_frame::<_, ClientPacket>(&mut cut.as_slice()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_connection_exchanges_packets_and_reports_close() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let conn = Connection::<ServerPacket, ClientPacket>::from_stream(stream).unwrap();
            let packet = conn.recv_timeout(Duration::from_secs(5)).unwrap();
            let ClientPacket::ChatMessage(text) = packet else { panic!("paquet inattendu : {packet:?}") };
            conn.send(&ServerPacket::ChatMessage { sender: "serveur".into(), message: text.to_uppercase() });
            // `conn` lâchée : le client voit la fermeture
        });

        let client = Connection::<ClientPacket, ServerPacket>::connect(&addr, Duration::from_secs(5)).unwrap();
        client.send(&ClientPacket::ChatMessage("salut".into()));
        match client.recv_timeout(Duration::from_secs(5)).unwrap() {
            ServerPacket::ChatMessage { message, .. } => assert_eq!(message, "SALUT"),
            other => panic!("paquet inattendu : {other:?}"),
        }
        server.join().unwrap();
        assert!(client.recv_timeout(Duration::from_secs(5)).is_err());
        assert!(client.try_recv().is_err());
    }
}

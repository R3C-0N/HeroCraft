use crate::character::CharacterStats;
use crate::creature::CreatureState;
use crate::math::{PlanetChunkCoord, PlanetFace};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Version du protocole : un client et un serveur de versions différentes refusent de se parler
pub const PROTOCOL_VERSION: u32 = 24;
/// Port d'écoute du serveur par défaut
pub const DEFAULT_PORT: u16 = 7777;

/// Position d'un bloc sur la planète : face du cube, colonne (x le long de u, z le long de v) et
/// altitude y en blocs depuis le fond du monde (même repère que `herocraft_world::BlockPos`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockAddress {
    pub face: PlanetFace,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Messages envoyés du Client vers le Serveur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientPacket {
    /// Demande de connexion initiale avec nom du joueur et version
    Connect {
        player_name: String,
        protocol_version: u32,
        /// Empreinte des fichiers de compétences du client (`SkillCatalog::fingerprint`) : le
        /// serveur refuse un client qui ne juge pas les compétences comme lui
        skills: u64,
    },
    /// Position des pieds du joueur (repère du centre de la planète), orientation du regard et
    /// sprint en cours (il consomme l'endurance), posture accroupie (les bêtes craintives le
    /// remarquent de moins loin)
    PlayerMove {
        position: Vec3,
        yaw: f32,
        pitch: f32,
        sprinting: bool,
        crouching: bool,
        /// Ce qu'il tient en main (numéro d'`ItemId`) : sa nourriture attire les bêtes
        holding: Option<u16>,
    },
    /// Le joueur vient de retomber au sol après une chute de `blocks` blocs
    Fall { blocks: f32 },
    /// Action de minage ou de pose d'un bloc
    InteractBlock { block: BlockAddress, action: BlockInteractionAction },
    /// Le joueur frappe une créature (dégâts, recul et fuite décidés par le serveur)
    HitCreature { creature_id: crate::CreatureId },
    /// Le joueur commence à tenir chargé un tir d'arc (`skill` : `None`) ou une compétence qui se
    /// charge (`Cast::charge`) : le serveur compte le temps jusqu'au tir ou au lancement
    BeginCharge { skill: Option<String> },
    /// Le joueur tire à l'arc ou à l'arbalète dans la direction `aim`, la munition `ammo` (numéro
    /// d'`ItemId`) déjà retirée de son inventaire. Le serveur juge l'arme, la cadence, le chargeur
    /// et l'arc bandé, puis annonce le tir (`PlayerShot`) ; un tir refusé est ignoré.
    Shoot { aim: Vec3, ammo: u16 },
    /// Le joueur donne à manger à une créature l'objet `item` (numéro d'`ItemId`), déjà retiré de son
    /// inventaire : le serveur l'apprivoise, la rend amoureuse ou fait grandir le petit
    FeedCreature { creature_id: crate::CreatureId, item: u16 },
    /// Le joueur parle à un habitant sans rien à échanger (clic droit) : le serveur choisit sa
    /// réplique et l'envoie aux joueurs qui l'entendent (`ServerPacket::VillagerSays`)
    TalkToVillager { creature_id: crate::CreatureId },
    /// Le joueur dépèce une bête morte à sa portée, couteau ou dague en main : le serveur tire son
    /// butin (`ServerPacket::CreatureLoot`) et la retire
    SkinCreature { creature_id: crate::CreatureId },
    /// Le joueur investit un point dans une compétence de la roue (le serveur vérifie points et
    /// prérequis, puis renvoie ses statistiques)
    LearnSkill { id: String },
    /// Le joueur lance la compétence `skill`, en regardant dans la direction `aim` (repère du
    /// centre de la planète) et en visant le point `aimed` au sol, s'il en vise un. Le serveur
    /// vérifie rang, coût et recharge, puis l'annonce (`SkillCast`) ou la refuse (`SkillRefused`).
    CastSkill { skill: String, aim: Vec3, aimed: Option<Vec3> },
    /// Légende du héros, envoyée à la connexion et au baptême : le serveur vérifie chaque choix
    /// contre l'atlas et en prend la voie (ses jauges et ses gains de niveau en dépendent)
    Legend(Box<crate::hero::Legend>),
    /// Le joueur mange un aliment (numéro d'objet) : le serveur en lit la satiété et renvoie ses
    /// statistiques
    Eat { item: u16 },
    /// Pièces que le joueur porte (numéros d'objets), envoyées à la connexion et à chaque
    /// changement : leur Protection réduit les coups physiques qu'il reçoit
    Equipment(Vec<u16>),
    /// Envoi d'un message dans le chat
    ChatMessage(String),
    /// Commande `/summon` : `count` créatures de l'espèce `species` (nom de son fichier) aux pieds
    /// `feet`, près du joueur
    Summon { species: String, count: u32, feet: Vec3 },
    /// Commande `/heal` : jauges pleines
    Heal,
    /// Commande `/xp` : points d'expérience gagnés
    GainExperience(u64),
    /// Commandes `/time set`, `/season` et `/weather` : le ciel est celui du serveur, pour tous.
    /// `time` : nouveau temps du ciel ; `forced` : nouveau temps imposé (`Some(None)` : le temps
    /// naturel revient)
    SetSky { time: Option<f64>, forced: Option<Option<crate::WeatherKind>> },
    /// Le joueur quitte la partie
    Disconnect,
    /// Le joueur tombé réapparaît (écran de mort quitté) : une résurrection ne peut plus le relever
    Respawn,
    /// Commande de test : entrer dans la vision du temple le plus proche (`true`) ou en revenir
    Vision { enter: bool },
    /// Dans la vision, le joueur a ramassé l'éclat du tutoriel : il s'éveille en héros et les
    /// premières compétences de sa voie s'ouvrent (`vision::awaken`)
    VisionAwaken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockInteractionAction {
    Break,
    Place(u16), // BlockId
}

/// État d'un joueur tel que les autres le voient
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub player_id: crate::PlayerId,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Messages envoyés du Serveur vers le Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerPacket {
    /// Acceptation de la connexion. Suivie des `edited_chunks` chunks modifiés du monde
    /// (`ChunkData`), puis des joueurs déjà en ligne (`PlayerJoined`).
    ConnectAck {
        player_id: crate::PlayerId,
        /// Graine de la planète : le client génère le même terrain que le serveur
        world_seed: u64,
        /// Dernière position connue du joueur sur ce serveur, ou `None` pour un premier passage
        spawn_position: Option<Vec3>,
        server_tickrate: u32,
        edited_chunks: u32,
    },
    /// Chunk modifié par les joueurs, compressé en RLE (`ChunkData::compress_rle`)
    ChunkData { coord: PlanetChunkCoord, compressed_voxels: Vec<u8> },
    /// Mise à jour d'un voxel individuel
    VoxelUpdate { block: BlockAddress, block_id: u16 },
    /// Un joueur est en ligne (à la connexion : un message par joueur déjà présent)
    PlayerJoined { player_id: crate::PlayerId, name: String },
    /// Allure d'un joueur (sa légende vérifiée), pour dessiner son héros : à chaque baptême, et à
    /// la connexion pour les joueurs déjà présents
    PlayerAppearance { player_id: crate::PlayerId, appearance: Box<crate::hero::Appearance> },
    /// Un joueur a quitté la partie
    PlayerLeft { player_id: crate::PlayerId },
    /// Positions des autres joueurs, envoyées à intervalle régulier
    PlayerStates(Vec<PlayerState>),
    /// Une créature entre dans le voisinage du joueur (elle vient d'apparaître, ou il s'en approche)
    CreatureSpawned {
        /// Identifiant de l'espèce (nom de son fichier dans `assets/creatures/`)
        species: String,
        state: CreatureState,
    },
    /// Fiche de l'habitant ou du voyageur qu'incarne une créature humanoïde, envoyée juste avant
    /// son `CreatureSpawned`
    VillagerCard { creature_id: crate::CreatureId, card: Box<crate::villager::VillagerCard> },
    /// Créatures sorties du voisinage du joueur, ou disparues
    CreaturesDespawned(Vec<crate::CreatureId>),
    /// Positions des créatures du voisinage, envoyées à intervalle régulier
    CreatureStates(Vec<CreatureState>),
    /// Tirs de créatures (tourelles, bastion, liche) partis à l'instant, pour les montrer
    CreatureShots(Vec<crate::creature::CreatureShotEvent>),
    /// Projectiles partis, déviés ou arrêtés depuis le dernier envoi (joueurs et créatures)
    Projectiles(Vec<crate::creature::ProjectileEvent>),
    /// Mise à jour des statistiques RPG du joueur (chaque seconde, et aussitôt après une chute)
    StatsUpdate(CharacterStats),
    /// Le joueur vient d'être mordu ou chargé par une créature (suivi de ses nouvelles jauges)
    PlayerHurt {
        damage: f32,
        /// Nom de l'espèce qui l'a frappé, pour l'écran de mort
        cause: String,
    },
    /// Le joueur vient de dépecer une bête morte : son butin tombe au sol à côté d'elle (`position`),
    /// objets et nombres (`ItemId`), tirés par le serveur
    CreatureLoot { position: Vec3, items: Vec<(u16, u16)> },
    /// Une bête apprivoisée vient de pondre `item` (`ItemId`) à ses pieds (`position`) : il reste
    /// au sol jusqu'à ce qu'on marche dessus
    Laid { position: Vec3, item: u16 },
    /// Un habitant parle (on vient de lui adresser la parole) : seuls les joueurs proches de lui le
    /// reçoivent, et le lisent dans le chat
    VillagerSays { speaker: String, line: String },
    /// Message de chat ou notification système
    ChatMessage { sender: String, message: String },
    /// Horloge du ciel du serveur (heure, saison, météo), à la connexion puis à intervalle régulier
    Clock {
        time: f64,
        /// Temps imposé par `/weather`, ou `None` pour le temps naturel
        forced: Option<crate::WeatherKind>,
    },
    /// Un joueur lance une compétence (envoyé à tous, lanceur compris : son client a déjà commencé
    /// le geste et ne le rejoue pas)
    SkillCast {
        player_id: crate::PlayerId,
        /// La compétence vraiment lancée : le mélange, pour la Fusion
        skill: String,
        rank: u8,
        aim: Vec3,
        aimed: Option<Vec3>,
        /// Lancée par la Fusion (sa recharge est celle de la Fusion), et amplifiée
        fused: bool,
        amplified: bool,
    },
    /// Un joueur a tiré à l'arc ou à l'arbalète (envoyé à tous, tireur compris) : d'où, jusqu'où,
    /// la bête touchée, la munition spéciale qu'il portait (compétence qui l'a chargée)
    PlayerShot { player_id: crate::PlayerId, from: Vec3, to: Vec3, target: Option<crate::CreatureId>, special: Option<String> },
    /// Le lancement de `skill` est refusé (`reason` : pourquoi, en clair) ; le client arrête le geste
    SkillRefused { skill: String, reason: String },
    /// Une compétence déplace son lanceur (téléportation près de sa cible, traction vers elle) :
    /// envoyé à lui seul, qui décide de sa position
    SkillMove(crate::skills::SkillMove),
    /// La compétence frappe : centre de sa zone et cibles touchées, pour leurs effets visuels
    SkillHit {
        player_id: crate::PlayerId,
        skill: String,
        rank: u8,
        center: Vec3,
        creatures: Vec<crate::CreatureId>,
        players: Vec<crate::PlayerId>,
        /// La touche d'un projectile en vol : seuls les visuels « hit » jouent
        contact: bool,
        /// La riposte d'une garde qui vient de parer un coup
        parried: bool,
    },
    /// La garde de `skill` s'est fermée sans rien parer : sa recharge dure `seconds` de plus
    SkillCooldown { skill: String, seconds: f32 },
    /// Le coup d'une bête repousse le joueur : direction le long du sol et vitesse, en blocs par
    /// seconde
    PlayerPushed { direction: Vec3, speed: f32 },
    /// Le joueur entre dans la vision d'un temple, seul dans le passé de la ville tenue par le
    /// peuple `holder` (rang dans `Culture::ALL`), ou en revient (`None`) : le client change
    /// d'époque, oublie les blocs modifiés de l'autre monde et pose le joueur en `feet`
    Vision { holder: Option<u8>, feet: Vec3 },
    /// Erreur ou déconnexion
    Disconnect(String),
}

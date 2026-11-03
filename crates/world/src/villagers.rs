//! Habitants des villes et des villages, et voyageurs qui vont de l'un à l'autre par les chemins.
//!
//! **Habitants.** La première fois qu'un joueur approche d'une ville ou d'un village, ses habitants
//! sont tirés de la graine du monde : une à trois personnes par maison, de la culture de la ville
//! pour la plupart (`Settlement::resident_culture`), chacune avec son apparence, son nom et, sauf
//! certains, un métier (`assets/metiers/`) dont le lieu est dans le plan : forgeron à la forge,
//! marchand derrière son étal, fermier au bord de son champ, aubergiste et prêtre sur le seuil de la
//! taverne et de l'église, gardes aux portes de la ville ou près du puits du village.
//!
//! Tant qu'un joueur est près, chaque habitant est une créature humanoïde que la simulation des
//! créatures fait marcher ; ce module lui dit où aller. Le jour, il sort de chez lui, travaille,
//! flâne dans les rues, va au marché, s'arrête bavarder avec un voisin. Le soir venu, chacun à son
//! heure, il rentre chez lui et disparaît derrière sa porte jusqu'au matin. Les gardes patrouillent
//! d'une porte à l'autre ; la moitié d'entre eux veille la nuit. Ils chassent les bêtes qui
//! approchent et s'en prennent au joueur qui frappe un habitant (`defends_herd`) ; les autres fuient
//! les bêtes (`afraid_of`). Les rues sont parcourues sur une grille du plan qui contourne bâtiments,
//! champs et remparts (`NavGrid`).
//!
//! **Voyageurs.** Colporteurs, pèlerins, aventuriers suivent les chemins d'un établissement à
//! l'autre, même loin de tout joueur (leur avancée est alors simplement comptée le long du chemin).
//! Arrivés, ils restent un moment, flânent et bavardent, dorment à l'auberge, puis repartent par un
//! autre chemin. La nuit, sur la route, ils s'écartent du chemin et campent ; un voyageur armé se
//! défend contre les bêtes qui l'attaquent, les autres s'enfuient.
//!
//! Les habitants ne sont pas enregistrés : un habitant tué revient quand la partie est relancée.

use std::collections::{HashMap, VecDeque};
use std::f32::consts::TAU;
use std::sync::Arc;

use glam::Vec3;
use herocraft_common::hero::atlas;
use herocraft_common::{professions, CreatureRegistry, PlanetFace, Profession, VillagerCard, Workplace};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::creatures::{CreatureWorld, CREATURE_RANGE_BLOCKS};
use crate::culture::Culture;
use crate::edits::{BlockGeometry, TerrainQuery, WORLD_HEIGHT};
use crate::generator::PlanetWorldGenerator;
use crate::roads::Road;
use crate::structures::{face_index, hash2, mix, Piece, Rect, Settlement, SettlementKind, Use};

/// Distance du centre d'un établissement au joueur le plus proche sous laquelle ses habitants
/// sont dans les rues, en blocs
const ACTIVE_RANGE_BLOCKS: f32 = CREATURE_RANGE_BLOCKS + 40.0;
/// Marge au-delà de laquelle ils s'effacent de nouveau (pour ne pas clignoter à la limite)
const INACTIVE_MARGIN_BLOCKS: f32 = 30.0;
/// Un voyageur prend corps à moins de tant de blocs d'un joueur, s'efface au-delà de la seconde
const TRAVELLER_EMBODY_BLOCKS: f32 = CREATURE_RANGE_BLOCKS + 10.0;
const TRAVELLER_RELEASE_BLOCKS: f32 = CREATURE_RANGE_BLOCKS + 40.0;
/// Au-delà, un voyageur est oublié (personne n'ira jamais le voir)
const TRAVELLER_FORGET_BLOCKS: f32 = 3000.0;
/// Voyageurs au plus sur toute la planète
const MAX_TRAVELLERS: usize = 40;
/// Écart entre deux points de la route d'un voyageur, en blocs
const ROUTE_SPACING: f64 = 3.0;
/// Marge de la grille de déplacement autour du plan, en blocs (pour rejoindre le bout des rues)
const NAV_MARGIN: i32 = 12;
/// Temps entre deux recherches des établissements proches des joueurs, en secondes
const SCAN_INTERVAL: f32 = 1.0;
/// Hauteur du soleil sous laquelle les habitants rentrent chez eux (décalée pour chacun)
const DUSK_HEIGHT: f32 = 0.03;
/// Hauteur du soleil sous laquelle les voyageurs campent
const CAMP_HEIGHT: f32 = -0.02;
/// Distance à laquelle deux habitants désœuvrés s'aperçoivent et vont bavarder, en blocs
const CHAT_RADIUS_BLOCKS: f32 = 12.0;
/// Sans avancer vers son but pendant ce temps, un habitant passe au point suivant (en secondes)
const STUCK_TIME: f32 = 6.0;
/// Distance du chemin où un voyageur campe, en blocs
const CAMP_OFFSET_BLOCKS: f32 = 3.5;
/// Premier numéro des troupeaux des villages et des voyageurs (les bêtes ont les leurs)
const VILLAGE_HERD_BASE: u64 = 1 << 60;
const TRAVELLER_HERD_BASE: u64 = 1 << 61;

/// Un établissement : sa face et la colonne de son centre
type TownKey = (PlanetFace, i32, i32);
type Cell = (i32, i32);

/// Un poste : la colonne où se tenir et celle à regarder
#[derive(Debug, Clone, Copy, PartialEq)]
struct Spot {
    cell: Cell,
    look: Cell,
    /// Plancher du bâtiment voisin (pour poser les pieds sur son seuil plutôt que sur son toit)
    floor: Option<i32>,
}

/// Une maison : la colonne de sa porte (dans le mur) et celle juste devant
#[derive(Debug, Clone, Copy)]
struct Home {
    door: Cell,
    out: Cell,
    floor: i32,
}

/// Grille des colonnes où l'on peut marcher dans un établissement et autour
#[derive(Debug, Clone)]
struct NavGrid {
    area: Rect,
    blocked: Vec<bool>,
}

impl NavGrid {
    fn new(area: Rect) -> Self {
        Self { area, blocked: vec![false; (area.width() * area.depth()) as usize] }
    }

    fn index(&self, (x, z): Cell) -> Option<usize> {
        self.area.contains(x, z).then(|| ((z - self.area.z0) * self.area.width() + x - self.area.x0) as usize)
    }

    fn set_rect(&mut self, r: Rect, blocked: bool) {
        for z in r.z0..r.z1 {
            for x in r.x0..r.x1 {
                self.set((x, z), blocked);
            }
        }
    }

    fn set(&mut self, cell: Cell, blocked: bool) {
        if let Some(i) = self.index(cell) {
            self.blocked[i] = blocked;
        }
    }

    fn walkable(&self, cell: Cell) -> bool {
        self.index(cell).is_some_and(|i| !self.blocked[i])
    }

    /// La colonne praticable la plus proche, à moins de 8 colonnes
    fn nearest_walkable(&self, (x, z): Cell) -> Option<Cell> {
        for r in 0..=8 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if (dx as i32).abs().max((dz as i32).abs()) == r && self.walkable((x + dx, z + dz)) {
                        return Some((x + dx, z + dz));
                    }
                }
            }
        }
        None
    }

    /// Ligne droite praticable de `a` à `b`
    fn line_clear(&self, a: Cell, b: Cell) -> bool {
        let (dx, dz) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
        let steps = (dx.abs().max(dz.abs()) * 3.0).ceil() as i32;
        (0..=steps).all(|i| {
            let t = if steps == 0 { 0.0 } else { i as f32 / steps as f32 };
            let (x, z) = (a.0 as f32 + 0.5 + dx * t, a.1 as f32 + 0.5 + dz * t);
            // Le corps a une largeur : les colonnes voisines de la ligne comptent aussi
            [(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)].iter().all(|(ox, oz)| self.walkable(((x + ox).floor() as i32, (z + oz).floor() as i32)))
        })
    }

    /// Plus court chemin de `from` à `to` (déplacements en huit directions, sans couper les angles),
    /// réduit à ses tournants : les colonnes à rejoindre l'une après l'autre, `to` comprise
    fn path(&self, from: Cell, to: Cell) -> Option<Vec<Cell>> {
        let start = self.nearest_walkable(from)?;
        let goal = self.nearest_walkable(to)?;
        let (si, gi) = (self.index(start)?, self.index(goal)?);
        let mut parent = vec![u32::MAX; self.blocked.len()];
        parent[si] = si as u32;
        let mut queue = VecDeque::from([start]);
        const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
        while let Some(cell) = queue.pop_front() {
            if cell == goal {
                break;
            }
            let ci = self.index(cell)?;
            for (dx, dz) in DIRS {
                let next = (cell.0 + dx, cell.1 + dz);
                if !self.walkable(next) || (dx != 0 && dz != 0 && !(self.walkable((cell.0 + dx, cell.1)) && self.walkable((cell.0, cell.1 + dz)))) {
                    continue;
                }
                let ni = self.index(next)?;
                if parent[ni] == u32::MAX {
                    parent[ni] = ci as u32;
                    queue.push_back(next);
                }
            }
        }
        if parent[gi] == u32::MAX {
            return None;
        }
        let w = self.area.width();
        let cell_of = |i: usize| (self.area.x0 + i as i32 % w, self.area.z0 + i as i32 / w);
        let mut cells = vec![goal];
        let mut i = gi;
        while i != si {
            i = parent[i] as usize;
            cells.push(cell_of(i));
        }
        cells.reverse();
        // Ne garde que les tournants : de chaque point, le plus lointain visible en ligne droite
        let mut turns = Vec::new();
        let mut k = 0;
        while k + 1 < cells.len() {
            let mut far = k + 1;
            for j in (k + 2..cells.len()).rev().take(40) {
                if self.line_clear(cells[k], cells[j]) {
                    far = j;
                    break;
                }
            }
            turns.push(cells[far]);
            k = far;
        }
        Some(turns)
    }
}

/// Une ville ou un village et ce que ses habitants y connaissent
struct Town {
    nav: Arc<NavGrid>,
    homes: Vec<Home>,
    /// Postes de travail de chaque lieu
    work: HashMap<Workplace, Vec<Spot>>,
    /// Devant chaque étal, où se tient le client
    customers: Vec<Spot>,
    /// Colonnes des rues et des places où flâner
    wander: Vec<Cell>,
    /// Place centrale (fontaine, puits)
    plaza: Cell,
    /// Seuil de la taverne, où les voyageurs passent la nuit
    inn: Option<Home>,
    active: bool,
}

/// Ce que fait un habitant ou un voyageur
#[derive(Debug, Clone, Copy, PartialEq)]
enum Task {
    /// Chez lui (ou à l'auberge), sans corps
    Indoors,
    /// Arrêté, il attend la fin de `timer` pour choisir quoi faire
    Idle,
    /// En chemin vers un but
    Going(Goal),
    /// À son poste
    Working,
    /// Il bavarde avec quelqu'un (ou attend qu'il le rejoigne)
    Chatting,
    /// Endormi au bord du chemin
    Camping,
    /// Sur la route d'un établissement à un autre
    Travelling,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Goal {
    Home,
    Work,
    Wander,
    Shop,
    Chat,
    Patrol,
    Camp,
}

/// Le trajet d'un voyageur
#[derive(Debug, Clone)]
struct Trip {
    /// Centres de l'établissement quitté et de celui visé
    from: Cell,
    to: Cell,
    /// Points du chemin, de l'un à l'autre
    route: Vec<Cell>,
    /// Prochain point à atteindre, et avancée vers lui (entre 0 et 1) loin des joueurs
    k: usize,
    frac: f32,
    /// En visite dans un établissement : lequel, et pour combien de temps encore
    visiting: Option<(TownKey, f32)>,
    walk_speed: f32,
}

#[derive(Debug, Clone)]
enum Role {
    Resident { town: TownKey, home: usize, work: Option<Spot> },
    Traveller(Trip),
}

/// Un habitant ou un voyageur
struct Person {
    card: Arc<VillagerCard>,
    species: usize,
    face: PlanetFace,
    role: Role,
    guard: bool,
    /// Garde de nuit : il reste dehors quand les autres dorment
    night_watch: bool,
    creature: Option<u64>,
    alive: bool,
    task: Task,
    timer: f32,
    path: VecDeque<Cell>,
    /// Poste où il s'arrêtera au bout du chemin
    destination: Option<Spot>,
    /// Interlocuteur (rang dans `Villages::people`)
    partner: Option<usize>,
    chat_cooldown: f32,
    /// Distance au but au dernier progrès, et temps depuis
    progress: (f32, f32),
    /// Décalage de l'heure à laquelle il rentre (en hauteur du soleil)
    bias: f32,
    herd: u64,
}

/// Espèces des créatures qui incarnent les habitants
#[derive(Debug, Clone, Default)]
struct SpeciesIds {
    by_id: HashMap<String, usize>,
    /// Rang de l'inactivité de chaque nom, par espèce
    pauses: HashMap<(usize, &'static str), u8>,
}

/// Les habitants et les voyageurs de la planète
pub struct Villages {
    seed: u64,
    enabled: bool,
    species: SpeciesIds,
    towns: HashMap<TownKey, Town>,
    people: Vec<Person>,
    /// Fiche de chaque créature qui incarne quelqu'un
    cards: HashMap<u64, Arc<VillagerCard>>,
    rng: StdRng,
    scan_timer: f32,
}

impl Default for Villages {
    fn default() -> Self {
        Self { seed: 0, enabled: false, species: SpeciesIds::default(), towns: HashMap::new(), people: Vec::new(), cards: HashMap::new(), rng: StdRng::seed_from_u64(0), scan_timer: 0.0 }
    }
}

/// Noms tirés par culture : un début et une fin
fn random_name(culture: Culture, rng: &mut StdRng) -> String {
    let (starts, ends): (&[&str], &[&str]) = match culture {
        Culture::Humains => (
            &["Jeh", "Aud", "Thib", "Isab", "Perr", "Gaut", "Marg", "Bertr", "Clém", "Ren", "Arn", "Blanch", "Guill", "Mahi", "Ysol", "Hug", "Agn", "Colin", "Ermen", "Rob"],
            &["an", "e", "aut", "eau", "in", "ier", "ot", "and", "ence", "aud", "ette", "ine", "on", "ert", "elle"],
        ),
        Culture::Nains => (&["Thor", "Bram", "Dur", "Grim", "Kar", "Bor", "Hal", "Dag", "Brun", "Orm", "Gund", "Sto"], &["grim", "in", "dak", "rik", "na", "hild", "dun", "ra", "gar", "li", "bek"]),
        Culture::ElfesDesBois => (&["Ael", "Syl", "Fae", "Lin", "Eir", "Thal", "Nim", "Ari", "Fen", "Ilv", "Oro"], &["wen", "rien", "dil", "las", "iel", "or", "wyn", "ith", "anna", "eth"]),
        Culture::HautsElfes => (&["Cael", "Ithi", "Vael", "Elo", "Ser", "Ara", "Lumi", "Ilo", "Sae", "Tir"], &["andor", "riel", "thas", "dris", "eon", "lise", "marin", "nael", "ion", "esse"]),
    };
    format!("{}{}", starts[rng.gen_range(0..starts.len())], ends[rng.gen_range(0..ends.len())])
}

/// Apparence d'une personne de cette culture et de ce métier : tirée au hasard, puis vêtue selon
/// le métier
fn appearance_for(culture: Culture, profession: &Profession, name: &str, rng: &mut StdRng) -> herocraft_common::hero::Appearance {
    let mut draw = || rng.gen::<f64>();
    let mut look = atlas().aleatoire(&mut draw, Some(culture.espece_id()));
    look.nom = name.to_string();
    if !profession.outfits.is_empty() {
        look.tenue = profession.outfits[rng.gen_range(0..profession.outfits.len())].clone();
    } else if ["nu", "harnais", "bandes", "plastron", "cuirasse", "ecailles", "mailles", "robeMage"].contains(&look.tenue.as_str()) {
        // Un habitant sans métier s'habille simplement
        look.tenue = ["tunique", "chemise", "gilet", "robe"][rng.gen_range(0..4)].to_string();
    }
    if !profession.colours.is_empty() {
        look.couleur_tenue = profession.colours[rng.gen_range(0..profession.colours.len())].clone();
    }
    look
}

/// Quelques échanges du métier (de trois à cinq), tirés pour cette personne
fn trades_for(profession: &Profession, rng: &mut StdRng) -> Vec<herocraft_common::Trade> {
    let mut trades = profession.trades.clone();
    for i in (1..trades.len()).rev() {
        trades.swap(i, rng.gen_range(0..=i));
    }
    trades.truncate(rng.gen_range(3..=5));
    trades
}

fn card_for(culture: Culture, profession: &Profession, origin: String, rng: &mut StdRng) -> Arc<VillagerCard> {
    let name = random_name(culture, rng);
    let appearance = appearance_for(culture, profession, &name, rng);
    let greeting = if profession.lines.is_empty() { "Bonjour.".to_string() } else { profession.lines[rng.gen_range(0..profession.lines.len())].clone() };
    Arc::new(VillagerCard { name, profession: profession.name.clone(), culture: culture.name().to_string(), origin, appearance, trades: trades_for(profession, rng), greeting })
}

/// Colonne de la porte d'une maison (au milieu du mur de la porte) et celle juste devant
fn door_of(house: &crate::structures::House) -> Home {
    let f = &house.foot;
    let (cx, cz) = f.center();
    let door = match house.door {
        crate::structures::Side::North => (cx, f.z0),
        crate::structures::Side::South => (cx, f.z1 - 1),
        crate::structures::Side::West => (f.x0, cz),
        crate::structures::Side::East => (f.x1 - 1, cz),
    };
    let (dx, dz) = house.door.delta();
    Home { door, out: (door.0 + dx, door.1 + dz), floor: house.floor }
}

/// Colonne au milieu du bord `side` d'un rectangle, et celle juste au-delà
fn edge_of(r: &Rect, side: crate::structures::Side) -> (Cell, Cell) {
    let (cx, cz) = r.center();
    let edge = match side {
        crate::structures::Side::North => (cx, r.z0),
        crate::structures::Side::South => (cx, r.z1 - 1),
        crate::structures::Side::West => (r.x0, cz),
        crate::structures::Side::East => (r.x1 - 1, cz),
    };
    let (dx, dz) = side.delta();
    (edge, (edge.0 + dx, edge.1 + dz))
}

impl Town {
    /// Lit le plan : où l'on marche, les maisons, les postes de travail, les rues
    fn new(s: &Settlement) -> Town {
        use crate::structures::Side;
        let mut nav = NavGrid::new(s.bounds.expand(NAV_MARGIN));
        let mut homes = Vec::new();
        let mut work: HashMap<Workplace, Vec<Spot>> = HashMap::new();
        let mut customers = Vec::new();
        let mut wander = Vec::new();
        let mut plaza = None;
        let mut inn = None;
        let mut doors = Vec::new();
        let mut gates = Vec::new();
        for piece in &s.pieces {
            match piece {
                Piece::House(h) => {
                    nav.set_rect(h.foot, true);
                    let home = door_of(h);
                    doors.push(home.door);
                    // Au seuil, il regarde la rue
                    let (dx, dz) = h.door.delta();
                    let spot = Spot { cell: home.out, look: (home.out.0 + dx, home.out.1 + dz), floor: Some(h.floor) };
                    match h.usage {
                        Use::Home => homes.push(home),
                        Use::Tavern => {
                            inn = Some(home);
                            work.entry(Workplace::Taverne).or_default().push(spot);
                        }
                        Use::Church => work.entry(Workplace::Eglise).or_default().push(spot),
                        Use::Barn => {}
                    }
                }
                Piece::Forge(f) => {
                    nav.set_rect(f.foot, true);
                    let (_, out) = edge_of(&f.foot, f.front);
                    work.entry(Workplace::Forge).or_default().push(Spot { cell: out, look: f.foot.center(), floor: Some(f.floor) });
                }
                Piece::Stall(st) => {
                    // Le comptoir et les poteaux arrêtent ; derrière, le marchand
                    let (front, before) = edge_of(&st.rect, st.front);
                    let (dx, dz) = st.front.delta();
                    let behind = (front.0 - dx, front.1 - dz);
                    for z in st.rect.z0..st.rect.z1 {
                        for x in st.rect.x0..st.rect.x1 {
                            let on_front = match st.front {
                                Side::North => z == st.rect.z0,
                                Side::South => z == st.rect.z1 - 1,
                                Side::West => x == st.rect.x0,
                                Side::East => x == st.rect.x1 - 1,
                            };
                            let corner = (x == st.rect.x0 || x == st.rect.x1 - 1) && (z == st.rect.z0 || z == st.rect.z1 - 1);
                            nav.set((x, z), on_front || corner);
                        }
                    }
                    let stand = if st.rect.contains(behind.0, behind.1) && behind != front { behind } else { before };
                    work.entry(Workplace::Etal).or_default().push(Spot { cell: stand, look: before, floor: None });
                    customers.push(Spot { cell: before, look: stand, floor: None });
                }
                Piece::Field { rect, .. } => {
                    nav.set_rect(*rect, true);
                    // Au bord du champ, côté place
                    let (cx, cz) = s.center;
                    let side = if (rect.center().0 - cx).abs() > (rect.center().1 - cz).abs() {
                        if rect.center().0 > cx { Side::West } else { Side::East }
                    } else if rect.center().1 > cz {
                        Side::North
                    } else {
                        Side::South
                    };
                    let (edge, out) = edge_of(rect, side);
                    work.entry(Workplace::Champ).or_default().push(Spot { cell: out, look: edge, floor: None });
                }
                Piece::Well { rect, .. } | Piece::Fountain { rect, .. } => {
                    nav.set_rect(*rect, true);
                    let (_, out) = edge_of(rect, Side::South);
                    plaza.get_or_insert(out);
                }
                Piece::Tower { rect, .. } | Piece::Rampart { rect, .. } => nav.set_rect(*rect, true),
                Piece::Gate { rect, .. } => gates.push(*rect),
                Piece::Lamp { x, z, .. } => nav.set((*x, *z), true),
                Piece::Paving { rect, .. } => {
                    for z in (rect.z0..rect.z1).step_by(2) {
                        for x in (rect.x0..rect.x1).step_by(2) {
                            wander.push((x, z));
                        }
                    }
                }
                _ => {}
            }
        }
        // Les portes de la ville et des maisons se franchissent
        for gate in &gates {
            nav.set_rect(*gate, false);
        }
        for door in doors {
            nav.set(door, false);
        }
        wander.retain(|c| nav.walkable(*c));
        let plaza = plaza.unwrap_or(s.center);
        // Les gardes : aux portes de la ville, ou près du puits du village
        let away = |cell: Cell| {
            let (dx, dz) = (cell.0 - s.center.0, cell.1 - s.center.1);
            (cell.0 + dx.signum() * 4, cell.1 + dz.signum() * 4)
        };
        let posts: Vec<Spot> = if gates.is_empty() {
            vec![Spot { cell: plaza, look: away(plaza), floor: None }]
        } else {
            gates.iter().map(|g| {
                let c = g.center();
                // Juste en dedans de la porte, à regarder dehors
                let inward = ((s.center.0 - c.0).signum() * 2, (s.center.1 - c.1).signum() * 2);
                let cell = (c.0 + inward.0, c.1 + inward.1);
                Spot { cell, look: away(c), floor: None }
            }).collect()
        };
        work.insert(Workplace::Rempart, posts);
        Town { nav: Arc::new(nav), homes, work, customers, wander, plaza, inn, active: false }
    }

    fn random_street(&self, rng: &mut StdRng) -> Cell {
        if self.wander.is_empty() {
            self.plaza
        } else {
            self.wander[rng.gen_range(0..self.wander.len())]
        }
    }
}

/// Culture de la ville (en attendant celle du plan) : celle du `n`-ième habitant
fn resident_culture(s: &Settlement, n: u64) -> Culture {
    let h = mix(n ^ hash2(0xC417 ^ ((face_index(s.face) as u64) << 40), s.center.0, s.center.1));
    if h % 100 < 85 {
        Culture::Humains
    } else {
        Culture::ALL[1 + (h / 100 % 3) as usize]
    }
}

/// Direction (depuis le centre de la planète) du milieu de la colonne (x, z) d'une face
fn cell_dir(geometry: &BlockGeometry, face: PlanetFace, (x, z): Cell) -> Vec3 {
    geometry.corner(face, x as f64 + 0.5, 0.0, z as f64 + 0.5).normalize().as_vec3()
}

/// Pieds posés sur le sol de la colonne, au-dessus du plancher `floor` s'il est donné (un seuil),
/// sinon près du relief naturel (sous les avant-toits)
fn feet_at(geometry: &BlockGeometry, query: &mut TerrainQuery, face: PlanetFace, (x, z): Cell, floor: Option<i32>) -> Option<Vec3> {
    let column = query.column_sample(face, x, z);
    let from = floor.map_or(column.elevation.floor() as i32 + 3, |f| f + 1).clamp(1, WORLD_HEIGHT - 2);
    let top = query.support_top(face, x, z, from);
    (top > 0).then(|| geometry.corner(face, x as f64 + 0.5, top as f64, z as f64 + 0.5).as_vec3())
}

/// Route d'un voyageur le long d'un chemin, partant de l'établissement `from` : des points tous
/// les `ROUTE_SPACING` blocs
fn route_along(road: &Road, from: Cell) -> Vec<Cell> {
    let mut points = road.points.clone();
    if road.from != from {
        points.reverse();
    }
    let mut route: Vec<Cell> = Vec::new();
    let mut carry = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let mut t = carry;
        while t < length {
            let p = (a.0 + (b.0 - a.0) * t / length, a.1 + (b.1 - a.1) * t / length);
            route.push((p.0.floor() as i32, p.1.floor() as i32));
            t += ROUTE_SPACING;
        }
        carry = t - length;
    }
    if let Some(last) = points.last() {
        route.push((last.0.floor() as i32, last.1.floor() as i32));
    }
    route.dedup();
    route
}

/// Les chemins qui partent de l'établissement `s`
fn roads_from<'a>(generator: &'a PlanetWorldGenerator, s: &Settlement) -> Vec<&'a Road> {
    generator.roads.touching(generator, s.face, s.bounds.expand(8)).into_iter().filter(|r| r.from == s.center || r.to == s.center).collect()
}

fn other_end(road: &Road, from: Cell) -> Cell {
    if road.from == from {
        road.to
    } else {
        road.from
    }
}

impl Villages {
    pub fn new(seed: u64, registry: &CreatureRegistry) -> Self {
        let mut species = SpeciesIds::default();
        for (index, s) in registry.iter().enumerate() {
            if s.humanoid {
                species.by_id.insert(s.id.clone(), index);
                for name in ["travaille", "discute", "sommeil"] {
                    if let Some(i) = s.pauses().position(|(n, _)| n == name) {
                        species.pauses.insert((index, name), i as u8);
                    }
                }
            }
        }
        let enabled = !professions().is_empty() && species.by_id.contains_key("villageois");
        Self { seed, enabled, species, rng: StdRng::seed_from_u64(seed ^ 0x7111_A6E5), ..Self::default() }
    }

    /// Fiche de la personne qu'incarne la créature `id`
    pub fn card(&self, id: u64) -> Option<&Arc<VillagerCard>> {
        self.cards.get(&id)
    }

    /// Nombre d'habitants et de voyageurs connus (vivants)
    pub fn population(&self) -> (usize, usize) {
        let residents = self.people.iter().filter(|p| p.alive && matches!(p.role, Role::Resident { .. })).count();
        let travellers = self.people.iter().filter(|p| p.alive && matches!(p.role, Role::Traveller(_))).count();
        (residents, travellers)
    }

    fn species_of(&self, profession: &Profession) -> usize {
        self.species.by_id.get(&profession.species).or_else(|| self.species.by_id.get("villageois")).copied().unwrap_or(0)
    }

    pub(crate) fn tick(&mut self, world: &mut CreatureWorld, query: &mut TerrainQuery, generator: &PlanetWorldGenerator, players: &[Vec3], dt: f32) {
        if !self.enabled {
            return;
        }
        // Les morts : une créature disparue ou tuée
        for person in &mut self.people {
            let Some(id) = person.creature else { continue };
            let alive = world.creatures().iter().find(|c| c.id == id).map(|c| c.is_alive());
            if alive != Some(true) {
                // Tuée (ou déjà disparue après sa chute) : seul ce module retire les créatures vivantes
                person.alive = false;
                person.creature = None;
                self.cards.remove(&id);
            }
        }
        self.scan_timer -= dt;
        if self.scan_timer <= 0.0 {
            self.scan_timer = SCAN_INTERVAL;
            self.scan(world, query, generator, players);
        }
        for i in 0..self.people.len() {
            if !self.people[i].alive {
                continue;
            }
            self.people[i].chat_cooldown = (self.people[i].chat_cooldown - dt).max(0.0);
            match self.people[i].role {
                Role::Resident { .. } => self.think_resident(i, world, query, dt),
                Role::Traveller(_) => self.think_traveller(i, world, query, generator, dt),
            }
        }
        // Les morts et les voyageurs oubliés s'en vont
        let mut index = 0;
        while index < self.people.len() {
            if self.people[index].alive {
                index += 1;
                continue;
            }
            if let Some(id) = self.people[index].creature {
                world.remove(id);
                self.cards.remove(&id);
            }
            self.people.swap_remove(index);
            // Les interlocuteurs pointaient par rang : on les oublie tous
            for p in &mut self.people {
                p.partner = None;
            }
        }
    }

    /// Distance (en blocs) d'un point de la planète au joueur le plus proche
    fn nearest_player(geometry: &BlockGeometry, dir: Vec3, players: &[Vec3]) -> f32 {
        players.iter().map(|p| (dir * p.length()).distance(*p)).fold(f32::INFINITY, f32::min) / geometry.block_height as f32
    }

    /// Repère les établissements proches des joueurs, réveille ou endort leurs habitants, et fait
    /// prendre corps aux voyageurs proches
    fn scan(&mut self, world: &mut CreatureWorld, query: &mut TerrainQuery, generator: &PlanetWorldGenerator, players: &[Vec3]) {
        let geometry = *world.geometry();
        for p in players {
            let Some(block) = geometry.block_at_point(p.as_dvec3()) else { continue };
            let area = Rect::around(block.x, block.z, ACTIVE_RANGE_BLOCKS as i32 + 64);
            for s in generator.structures.touching(generator, block.face, area) {
                let key = (s.face, s.center.0, s.center.1);
                if s.kind != SettlementKind::Ruins && !self.towns.contains_key(&key) {
                    self.found_town(s, generator);
                }
            }
        }
        let keys: Vec<TownKey> = self.towns.keys().copied().collect();
        for key in keys {
            let town = &self.towns[&key];
            let distance = Self::nearest_player(&geometry, cell_dir(&geometry, key.0, (key.1, key.2)), players);
            if !town.active && distance < ACTIVE_RANGE_BLOCKS {
                self.activate(key, world, query);
            } else if town.active && distance > ACTIVE_RANGE_BLOCKS + INACTIVE_MARGIN_BLOCKS {
                self.deactivate(key, world);
            }
        }
        // Voyageurs : prendre corps près d'un joueur, s'effacer loin, être oublié très loin
        for i in 0..self.people.len() {
            let Role::Traveller(trip) = &self.people[i].role else { continue };
            let face = self.people[i].face;
            let position = match self.people[i].creature.and_then(|id| world.creatures().iter().find(|c| c.id == id)) {
                Some(c) => c.position.normalize_or_zero(),
                None => match trip.visiting {
                    Some((key, _)) => cell_dir(&geometry, face, self.towns.get(&key).map_or((key.1, key.2), |t| t.plaza)),
                    None => cell_dir(&geometry, face, trip.route[trip.k.min(trip.route.len() - 1)]),
                },
            };
            let distance = Self::nearest_player(&geometry, position, players);
            let visiting_inactive = trip.visiting.is_some_and(|(key, _)| self.towns.get(&key).is_none_or(|t| !t.active));
            let indoors = self.people[i].task == Task::Indoors;
            if distance > TRAVELLER_FORGET_BLOCKS {
                self.people[i].alive = false;
            } else if let Some(id) = self.people[i].creature {
                if distance > TRAVELLER_RELEASE_BLOCKS {
                    world.remove(id);
                    self.cards.remove(&id);
                    self.people[i].creature = None;
                    self.people[i].path.clear();
                }
            } else if distance < TRAVELLER_EMBODY_BLOCKS && !visiting_inactive && !indoors {
                let cell = match &self.people[i].role {
                    Role::Traveller(Trip { visiting: Some((key, _)), .. }) => self.towns.get(key).map_or((key.1, key.2), |t| t.random_street(&mut self.rng)),
                    Role::Traveller(trip) => trip.route[trip.k.min(trip.route.len() - 1)],
                    _ => continue,
                };
                self.embody(i, cell, None, world, query);
            }
        }
    }

    /// Fonde la connaissance d'un établissement : son plan, ses habitants, et quelques voyageurs
    fn found_town(&mut self, s: &Settlement, generator: &PlanetWorldGenerator) {
        let key = (s.face, s.center.0, s.center.1);
        let herd = VILLAGE_HERD_BASE | (self.towns.len() as u64);
        let town_seed = mix(self.seed ^ hash2(0x7A11_A6E ^ ((face_index(s.face) as u64) << 48), s.center.0, s.center.1));
        let town = Town::new(s);
        let mut rng = StdRng::seed_from_u64(town_seed);
        let registry = professions();
        let origin = match s.kind {
            SettlementKind::Town => "Habitant de la ville",
            _ => "Habitant du village",
        };
        // Les métiers dont le lieu est dans le plan, un poste après l'autre
        let mut jobs: Vec<(usize, Spot)> = Vec::new();
        for (index, profession) in registry.iter().enumerate() {
            if profession.traveller || profession.workplace == Workplace::Aucun {
                continue;
            }
            for spot in town.work.get(&profession.workplace).into_iter().flatten() {
                for _ in 0..profession.per_workplace {
                    jobs.push((index, *spot));
                }
            }
        }
        let idle: Vec<usize> = registry.iter().enumerate().filter(|(_, p)| !p.traveller && p.workplace == Workplace::Aucun).map(|(i, _)| i).collect();
        if town.homes.is_empty() || idle.is_empty() {
            self.towns.insert(key, town);
            return;
        }
        let capacity = town.homes.len() * 2 + 1;
        jobs.truncate(capacity);
        let extra: usize = (0..town.homes.len()).map(|_| rng.gen_range(0..=1)).sum();
        let count = (jobs.len() + extra).max(town.homes.len());
        let mut guards = 0;
        for n in 0..count {
            let (profession, work) = match jobs.get(n) {
                Some(&(p, spot)) => (p, Some(spot)),
                None => (idle[rng.gen_range(0..idle.len())], None),
            };
            let p = registry.get(profession).unwrap();
            let culture = resident_culture(s, n as u64);
            let card = card_for(culture, p, origin.to_string(), &mut rng);
            let guard = p.workplace == Workplace::Rempart;
            if guard {
                guards += 1;
            }
            let species = self.species_of(p);
            self.people.push(Person {
                card,
                species,
                face: s.face,
                role: Role::Resident { town: key, home: n % town.homes.len(), work },
                guard,
                night_watch: guard && guards % 2 == 0,
                creature: None,
                alive: true,
                task: Task::Idle,
                timer: 0.0,
                path: VecDeque::new(),
                destination: None,
                partner: None,
                chat_cooldown: rng.gen_range(0.0..30.0),
                progress: (f32::INFINITY, 0.0),
                bias: rng.gen_range(-0.12..0.08),
                herd,
            });
        }
        self.towns.insert(key, town);

        // Des voyageurs : quelques-uns en visite, d'autres déjà en route sur les chemins qui partent d'ici
        let travellers = self.people.iter().filter(|p| matches!(p.role, Role::Traveller(_))).count();
        if travellers >= MAX_TRAVELLERS {
            return;
        }
        let roads = roads_from(generator, s);
        let visitors = rng.gen_range(0..=2);
        for _ in 0..visitors {
            if let Some(road) = roads.get(rng.gen_range(0..roads.len().max(1))) {
                // Arrivé par ce chemin : il en vient
                let from = other_end(road, s.center);
                let route = route_along(road, from);
                let k = route.len().saturating_sub(1);
                let visiting = Some((key, rng.gen_range(30.0..240.0)));
                self.add_traveller(s.face, from, s.center, route, k, visiting, &mut rng);
            }
        }
        for road in &roads {
            if rng.gen_bool(0.4) {
                let route = route_along(road, s.center);
                if route.len() > 4 {
                    let k = rng.gen_range(2..route.len() - 1);
                    self.add_traveller(s.face, s.center, other_end(road, s.center), route, k, None, &mut rng);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_traveller(&mut self, face: PlanetFace, from: Cell, to: Cell, route: Vec<Cell>, k: usize, visiting: Option<(TownKey, f32)>, rng: &mut StdRng) {
        if route.is_empty() {
            return;
        }
        let registry = professions();
        let candidates: Vec<(usize, f32)> = registry.iter().enumerate().filter(|(_, p)| p.traveller).map(|(i, p)| (i, p.weight)).collect();
        let total: f32 = candidates.iter().map(|c| c.1).sum();
        if total <= 0.0 {
            return;
        }
        let mut roll = rng.gen_range(0.0..total);
        let index = candidates.iter().find(|c| {
            roll -= c.1;
            roll < 0.0
        }).map_or(candidates[0].0, |c| c.0);
        let p = registry.get(index).unwrap();
        // Surtout des humains, qui voyagent le plus
        let culture = if rng.gen_bool(0.55) { Culture::Humains } else { Culture::ALL[rng.gen_range(1..4)] };
        let card = card_for(culture, p, "Voyageur".to_string(), rng);
        let species = self.species_of(p);
        let herd = TRAVELLER_HERD_BASE | self.rng.gen::<u32>() as u64;
        let walk_speed = 1.5;
        self.people.push(Person {
            card,
            species,
            face,
            role: Role::Traveller(Trip { from, to, route, k, frac: 0.0, visiting, walk_speed }),
            guard: false,
            night_watch: false,
            creature: None,
            alive: true,
            task: if visiting.is_some() { Task::Idle } else { Task::Travelling },
            timer: 0.0,
            path: VecDeque::new(),
            destination: None,
            partner: None,
            chat_cooldown: rng.gen_range(0.0..20.0),
            progress: (f32::INFINITY, 0.0),
            bias: rng.gen_range(-0.05..0.05),
            herd,
        });
    }

    /// Fait prendre corps à une personne sur la colonne `cell`
    fn embody(&mut self, i: usize, cell: Cell, floor: Option<i32>, world: &mut CreatureWorld, query: &mut TerrainQuery) {
        let geometry = *world.geometry();
        let person = &mut self.people[i];
        let Some(feet) = feet_at(&geometry, query, person.face, cell, floor) else { return };
        let yaw = self.rng.gen_range(0.0..TAU);
        let id = world.spawn_managed(person.species, feet, yaw, person.herd);
        person.creature = Some(id);
        person.path.clear();
        person.destination = None;
        person.partner = None;
        person.progress = (f32::INFINITY, 0.0);
        if person.task != Task::Travelling {
            person.task = Task::Idle;
        }
        person.timer = self.rng.gen_range(0.5..3.0);
        self.cards.insert(id, person.card.clone());
    }

    fn release(&mut self, i: usize, world: &mut CreatureWorld) {
        if let Some(id) = self.people[i].creature.take() {
            world.remove(id);
            self.cards.remove(&id);
        }
        self.people[i].path.clear();
        self.people[i].partner = None;
    }

    fn activate(&mut self, key: TownKey, world: &mut CreatureWorld, query: &mut TerrainQuery) {
        let Some(town) = self.towns.get_mut(&key) else { return };
        town.active = true;
        let sun = Self::sun_height(world, cell_dir(world.geometry(), key.0, (key.1, key.2)));
        for i in 0..self.people.len() {
            let Role::Resident { town: t, home, work } = self.people[i].role else { continue };
            if t != key || !self.people[i].alive {
                continue;
            }
            let person = &self.people[i];
            let out = !self.is_night(person, sun) || person.night_watch;
            if !out {
                self.people[i].task = Task::Indoors;
                continue;
            }
            let town = &self.towns[&key];
            let (cell, floor) = match work {
                Some(spot) if self.rng.gen_bool(0.7) => (spot.cell, spot.floor),
                _ if self.rng.gen_bool(0.3) => (town.homes[home].out, Some(town.homes[home].floor)),
                _ => (town.random_street(&mut self.rng), None),
            };
            self.people[i].task = Task::Idle;
            self.embody(i, cell, floor, world, query);
        }
    }

    fn deactivate(&mut self, key: TownKey, world: &mut CreatureWorld) {
        if let Some(town) = self.towns.get_mut(&key) {
            town.active = false;
        }
        for i in 0..self.people.len() {
            if matches!(self.people[i].role, Role::Resident { town, .. } if town == key) {
                self.release(i, world);
                if self.people[i].task != Task::Indoors {
                    self.people[i].task = Task::Idle;
                }
            }
        }
    }

    fn sun_height(world: &CreatureWorld, dir: Vec3) -> f32 {
        world.sun_dir().map_or(1.0, |sun| sun.dot(dir))
    }

    fn is_night(&self, person: &Person, sun: f32) -> bool {
        sun < DUSK_HEIGHT + person.bias
    }

    /// Envoie la personne `i` vers la colonne `to` (par les rues), pour y faire `goal` ; `spot` :
    /// le poste où s'arrêter au bout. Renvoie faux s'il n'y a pas de chemin.
    fn send(&mut self, i: usize, nav: &NavGrid, from: Cell, to: Cell, goal: Goal, spot: Option<Spot>) -> bool {
        let Some(path) = nav.path(from, to) else { return false };
        let person = &mut self.people[i];
        person.path = path.into();
        person.task = Task::Going(goal);
        person.destination = spot;
        person.progress = (f32::INFINITY, 0.0);
        true
    }

    /// Colonne où se tient une créature
    fn cell_of(world: &CreatureWorld, position: Vec3) -> Option<(PlanetFace, Cell)> {
        world.geometry().block_at_point(position.as_dvec3()).map(|b| (b.face, (b.x, b.z)))
    }

    fn set_pause(&self, world: &mut CreatureWorld, id: u64, species: usize, name: Option<&'static str>, look: Option<Vec3>) {
        let pause = name.and_then(|n| self.species.pauses.get(&(species, n)).copied());
        if let Some(c) = world.creature_mut(id) {
            c.pause = pause;
            c.look = look;
            c.guide = None;
            c.walking = false;
        }
    }

    /// Fait avancer la personne `i` le long de son chemin. Renvoie vrai une fois au bout.
    fn walk(&mut self, i: usize, world: &mut CreatureWorld, dt: f32) -> bool {
        let geometry = *world.geometry();
        let person = &mut self.people[i];
        let Some(id) = person.creature else { return false };
        let face = person.face;
        let Some(c) = world.creature_mut(id) else { return false };
        if c.guide.is_none() || c.arrived {
            c.arrived = false;
            match person.path.pop_front() {
                Some(next) => {
                    c.guide = Some(cell_dir(&geometry, face, next));
                    c.guide_pace = 1.0;
                    c.look = None;
                    person.progress = (f32::INFINITY, 0.0);
                }
                None => {
                    c.guide = None;
                    return true;
                }
            }
        }
        // Coincée : elle passe au point suivant, ou s'arrête là
        let distance = c.guide.map_or(0.0, |g| (g * c.position.length()).distance(c.position));
        let (best, since) = person.progress;
        if distance < best - 0.3 * geometry.block_height as f32 {
            person.progress = (distance, 0.0);
        } else {
            person.progress.1 = since + dt;
            if person.progress.1 > STUCK_TIME {
                c.guide = None;
                person.progress = (f32::INFINITY, 0.0);
                if person.path.is_empty() {
                    return true;
                }
            }
        }
        false
    }

    fn think_resident(&mut self, i: usize, world: &mut CreatureWorld, query: &mut TerrainQuery, dt: f32) {
        let Role::Resident { town: key, home, work } = self.people[i].role else { return };
        let Some(town) = self.towns.get(&key) else { return };
        if !town.active {
            return;
        }
        let geometry = *world.geometry();
        let sun = Self::sun_height(world, cell_dir(&geometry, key.0, (key.1, key.2)));
        let night = self.is_night(&self.people[i], sun) && !self.people[i].night_watch;
        let home_spot = town.homes[home];
        let Some(id) = self.people[i].creature else {
            // Chez lui : il sort au matin
            if self.people[i].task == Task::Indoors && !night {
                self.people[i].task = Task::Idle;
                self.embody(i, home_spot.out, Some(home_spot.floor), world, query);
            }
            return;
        };
        let Some(c) = world.creatures().iter().find(|c| c.id == id) else { return };
        let (position, busy) = (c.position, c.panic > 0.0 || c.is_hunting());
        let species = self.people[i].species;
        if busy {
            // Il fuit ou se bat : il reprendra ensuite ce qu'il faisait
            let person = &mut self.people[i];
            person.path.clear();
            person.task = Task::Idle;
            person.timer = 1.0;
            person.partner = None;
            return;
        }
        let Some((face, here)) = Self::cell_of(world, position) else { return };
        if face != key.0 {
            return;
        }
        let nav = self.towns[&key].nav.clone();
        if night && self.people[i].task != Task::Going(Goal::Home) {
            // Le soir : à la maison
            self.end_chat(i, world);
            let door = Spot { cell: home_spot.door, look: home_spot.door, floor: Some(home_spot.floor) };
            if self.send(i, &nav, here, home_spot.out, Goal::Home, Some(door)) {
                self.people[i].path.push_back(home_spot.door);
            } else {
                self.release(i, world);
                self.people[i].task = Task::Indoors;
            }
            return;
        }
        match self.people[i].task {
            Task::Going(goal) => {
                if self.walk(i, world, dt) {
                    self.arrive(i, goal, world);
                }
            }
            Task::Working | Task::Chatting | Task::Idle => {
                self.people[i].timer -= dt;
                if self.people[i].timer > 0.0 {
                    return;
                }
                self.end_chat(i, world);
                self.choose(i, key, here, work, world);
            }
            Task::Indoors => {
                self.release(i, world);
            }
            Task::Camping | Task::Travelling => self.people[i].task = Task::Idle,
        }
        let _ = species;
    }

    /// Au bout du chemin : il fait ce pour quoi il est venu
    fn arrive(&mut self, i: usize, goal: Goal, world: &mut CreatureWorld) {
        let geometry = *world.geometry();
        let person = &self.people[i];
        let (Some(id), face, species) = (person.creature, person.face, person.species) else { return };
        let look = person.destination.map(|s| cell_dir(&geometry, face, s.look));
        match goal {
            Goal::Home => {
                self.release(i, world);
                self.people[i].task = Task::Indoors;
            }
            Goal::Work => {
                let pause = if self.people[i].guard { None } else { Some("travaille") };
                self.set_pause(world, id, species, pause, look);
                let person = &mut self.people[i];
                person.task = Task::Working;
                person.timer = if person.guard { self.rng.gen_range(20.0..60.0) } else { self.rng.gen_range(30.0..90.0) };
            }
            Goal::Chat | Goal::Shop => {
                // Face à face : lui et son interlocuteur bavardent
                let partner = self.people[i].partner.filter(|&p| p < self.people.len());
                let partner_creature = partner.and_then(|p| self.people[p].creature);
                let partner_pos = partner_creature.and_then(|pid| world.creatures().iter().find(|c| c.id == pid)).map(|c| c.position);
                let duration = self.rng.gen_range(5.0..14.0);
                self.set_pause(world, id, species, Some("discute"), partner_pos.or(look));
                let me = world.creatures().iter().find(|c| c.id == id).map(|c| c.position);
                if let (Some(p), Some(pid)) = (partner, partner_creature) {
                    let pspecies = self.people[p].species;
                    self.set_pause(world, pid, pspecies, Some("discute"), me);
                    let other = &mut self.people[p];
                    if other.task != Task::Working {
                        other.task = Task::Chatting;
                    }
                    other.timer = other.timer.max(duration);
                    other.partner = Some(i);
                    other.chat_cooldown = 60.0;
                }
                let person = &mut self.people[i];
                person.task = Task::Chatting;
                person.timer = duration;
                person.chat_cooldown = self.rng.gen_range(40.0..90.0);
            }
            Goal::Camp => {
                if let Some(c) = world.creature_mut(id) {
                    c.sleeping = true;
                    c.walking = false;
                    c.guide = None;
                }
                self.set_pause(world, id, species, Some("sommeil"), None);
                if let Some(c) = world.creature_mut(id) {
                    c.sleeping = true;
                }
                self.people[i].task = Task::Camping;
            }
            Goal::Wander | Goal::Patrol => {
                self.set_pause(world, id, species, None, look);
                let person = &mut self.people[i];
                person.task = Task::Idle;
                person.timer = if goal == Goal::Patrol { self.rng.gen_range(6.0..16.0) } else { self.rng.gen_range(2.0..8.0) };
            }
        }
    }

    /// Fin d'une discussion : l'interlocuteur reprend ce qu'il faisait
    fn end_chat(&mut self, i: usize, world: &mut CreatureWorld) {
        let Some(p) = self.people[i].partner.take() else { return };
        if p >= self.people.len() || self.people[p].partner != Some(i) {
            return;
        }
        self.people[p].partner = None;
        let Some(pid) = self.people[p].creature else { return };
        let geometry = *world.geometry();
        let other = &self.people[p];
        let (species, face) = (other.species, other.face);
        match other.task {
            Task::Working => {
                let look = other.destination.map(|s| cell_dir(&geometry, face, s.look));
                let pause = if other.guard { None } else { Some("travaille") };
                self.set_pause(world, pid, species, pause, look);
            }
            Task::Chatting => {
                self.set_pause(world, pid, species, None, None);
                self.people[p].task = Task::Idle;
                self.people[p].timer = self.rng.gen_range(1.0..4.0);
            }
            _ => {}
        }
    }

    /// Désœuvré : il choisit sa prochaine occupation
    fn choose(&mut self, i: usize, key: TownKey, here: Cell, work: Option<Spot>, world: &mut CreatureWorld) {
        let nav = self.towns[&key].nav.clone();
        let roll: f64 = self.rng.gen();
        let person = &self.people[i];
        let guard = person.guard;
        // Un garde va de poste en poste
        if guard {
            let posts = self.towns[&key].work.get(&Workplace::Rempart).cloned().unwrap_or_default();
            let spot = if roll < 0.5 || posts.is_empty() { work } else { Some(posts[self.rng.gen_range(0..posts.len())]) };
            if let Some(spot) = spot {
                let goal = if Some(spot) == work { Goal::Work } else { Goal::Patrol };
                if self.send(i, &nav, here, spot.cell, goal, Some(spot)) {
                    return;
                }
            }
        }
        if let Some(spot) = work.filter(|_| roll < 0.55) {
            if self.send(i, &nav, here, spot.cell, Goal::Work, Some(spot)) {
                return;
            }
        }
        if roll < 0.8 && self.people[i].chat_cooldown <= 0.0 && self.find_chat(i, key, here, world) {
            return;
        }
        let town = &self.towns[&key];
        if roll < 0.88 && !town.customers.is_empty() {
            let spot = town.customers[self.rng.gen_range(0..town.customers.len())];
            // Le marchand de cet étal, s'il est là, le servira
            let merchant = self.people.iter().position(|p| {
                matches!(p.role, Role::Resident { work: Some(w), .. } if w.cell == spot.look) && p.task == Task::Working && p.creature.is_some()
            });
            if self.send(i, &nav, here, spot.cell, Goal::Shop, Some(spot)) {
                self.people[i].partner = merchant;
                return;
            }
        }
        let town = &self.towns[&key];
        let target = if roll < 0.93 { town.plaza } else { town.random_street(&mut self.rng) };
        if !self.send(i, &nav, here, target, Goal::Wander, None) {
            self.people[i].task = Task::Idle;
            self.people[i].timer = self.rng.gen_range(2.0..6.0);
        }
    }

    /// Cherche un voisin désœuvré à qui parler ; s'il en trouve un, le rejoint
    fn find_chat(&mut self, i: usize, key: TownKey, here: Cell, world: &mut CreatureWorld) -> bool {
        let block = world.geometry().block_height as f32;
        let Some(me) = self.people[i].creature.and_then(|id| world.creatures().iter().find(|c| c.id == id)).map(|c| c.position) else { return false };
        let in_town = |p: &Person| match &p.role {
            Role::Resident { town, .. } => *town == key,
            Role::Traveller(t) => t.visiting.is_some_and(|(k, _)| k == key),
        };
        let partner = self.people.iter().enumerate().filter(|(j, p)| {
            *j != i && p.alive && in_town(p) && p.task == Task::Idle && p.partner.is_none() && p.chat_cooldown <= 0.0
        }).filter_map(|(j, p)| {
            let pos = world.creatures().iter().find(|c| Some(c.id) == p.creature)?.position;
            let d = pos.distance(me) / block;
            (d < CHAT_RADIUS_BLOCKS).then_some((j, pos, d))
        }).min_by(|a, b| a.2.total_cmp(&b.2));
        let Some((j, pos, _)) = partner else { return false };
        let Some((_, there)) = Self::cell_of(world, pos) else { return false };
        // Il s'arrête à côté de lui, du côté d'où il vient
        let (dx, dz) = ((here.0 - there.0).signum(), (here.1 - there.1).signum());
        let stand = (there.0 + if dx == 0 && dz == 0 { 1 } else { dx }, there.1 + dz);
        let nav = self.towns[&key].nav.clone();
        if !self.send(i, &nav, here, stand, Goal::Chat, Some(Spot { cell: stand, look: there, floor: None })) {
            return false;
        }
        self.people[i].partner = Some(j);
        // Il l'attend en le regardant venir
        let pid = self.people[j].creature.unwrap();
        let species = self.people[j].species;
        self.set_pause(world, pid, species, None, Some(me));
        let other = &mut self.people[j];
        other.task = Task::Chatting;
        other.partner = Some(i);
        other.timer = 25.0;
        other.chat_cooldown = 60.0;
        true
    }

    fn think_traveller(&mut self, i: usize, world: &mut CreatureWorld, query: &mut TerrainQuery, generator: &PlanetWorldGenerator, dt: f32) {
        let geometry = *world.geometry();
        let Role::Traveller(trip) = &self.people[i].role else { return };
        let trip = trip.clone();
        let face = self.people[i].face;
        let creature = self.people[i].creature.and_then(|id| world.creatures().iter().find(|c| c.id == id)).map(|c| (c.position, c.panic > 0.0 || c.is_hunting()));
        // En visite
        if let Some((key, left)) = trip.visiting {
            let left = left - dt;
            if let Role::Traveller(t) = &mut self.people[i].role {
                t.visiting = Some((key, left));
            }
            let town_active = self.towns.get(&key).is_some_and(|t| t.active);
            let sun = Self::sun_height(world, cell_dir(&geometry, face, (key.1, key.2)));
            let night = sun < DUSK_HEIGHT + self.people[i].bias;
            if left <= 0.0 && !night {
                self.depart(i, key, world, generator);
                return;
            }
            let Some((position, busy)) = creature else {
                // Loin des joueurs ou à l'auberge : il sort le matin si la ville est animée
                if self.people[i].task == Task::Indoors && !night && town_active {
                    let inn = self.towns[&key].inn;
                    self.people[i].task = Task::Idle;
                    if let Some(inn) = inn {
                        self.embody(i, inn.out, Some(inn.floor), world, query);
                    }
                }
                return;
            };
            if busy {
                self.people[i].path.clear();
                self.people[i].task = Task::Idle;
                self.people[i].timer = 1.0;
                return;
            }
            let Some((_, here)) = Self::cell_of(world, position) else { return };
            let Some(town) = self.towns.get(&key) else { return };
            let nav = town.nav.clone();
            let inn = town.inn;
            if night && !matches!(self.people[i].task, Task::Going(Goal::Home) | Task::Going(Goal::Camp) | Task::Camping) {
                self.end_chat(i, world);
                match inn {
                    // Il dort à l'auberge
                    Some(inn) => {
                        let door = Spot { cell: inn.door, look: inn.door, floor: Some(inn.floor) };
                        if self.send(i, &nav, here, inn.out, Goal::Home, Some(door)) {
                            self.people[i].path.push_back(inn.door);
                        }
                    }
                    // Ou près de la place, à la belle étoile
                    None => {
                        let spot = self.towns[&key].random_street(&mut self.rng);
                        self.send(i, &nav, here, spot, Goal::Camp, None);
                    }
                }
                return;
            }
            if !night && self.people[i].task == Task::Camping {
                self.wake(i, world);
            }
            match self.people[i].task {
                Task::Going(goal) => {
                    if self.walk(i, world, dt) {
                        self.arrive(i, goal, world);
                    }
                }
                Task::Idle | Task::Chatting | Task::Working | Task::Travelling => {
                    self.people[i].timer -= dt;
                    if self.people[i].timer > 0.0 {
                        return;
                    }
                    self.end_chat(i, world);
                    let roll: f64 = self.rng.gen();
                    if roll < 0.5 && self.people[i].chat_cooldown <= 0.0 && self.find_chat(i, key, here, world) {
                        return;
                    }
                    let town = &self.towns[&key];
                    let target = if roll < 0.75 { town.plaza } else { town.random_street(&mut self.rng) };
                    if !self.send(i, &nav, here, target, Goal::Wander, None) {
                        self.people[i].task = Task::Idle;
                        self.people[i].timer = 3.0;
                    }
                }
                Task::Indoors => self.release(i, world),
                Task::Camping => {}
            }
            return;
        }

        // Sur la route
        let here_dir = match creature {
            Some((p, _)) => p.normalize_or_zero(),
            None => cell_dir(&geometry, face, trip.route[trip.k.min(trip.route.len() - 1)]),
        };
        let night = Self::sun_height(world, here_dir) < CAMP_HEIGHT + self.people[i].bias;
        let Some((position, busy)) = creature else {
            // Loin des joueurs : il avance le long du chemin, sauf la nuit
            if !night {
                let person = &mut self.people[i];
                if let Role::Traveller(t) = &mut person.role {
                    t.frac += t.walk_speed * dt / ROUTE_SPACING as f32;
                    while t.frac >= 1.0 && t.k < t.route.len() {
                        t.frac -= 1.0;
                        t.k += 1;
                    }
                    if t.k >= t.route.len() {
                        self.arrive_at(i, generator);
                    }
                }
            }
            return;
        };
        if busy {
            self.people[i].task = Task::Travelling;
            if let Some(c) = self.people[i].creature.and_then(|id| world.creature_mut(id)) {
                c.guide = None;
            }
            return;
        }
        match self.people[i].task {
            Task::Camping => {
                if !night {
                    self.wake(i, world);
                    self.people[i].task = Task::Travelling;
                }
            }
            Task::Going(Goal::Camp) => {
                if self.walk(i, world, dt) {
                    self.arrive(i, Goal::Camp, world);
                }
            }
            _ if night => {
                // La nuit tombe : il s'écarte du chemin pour camper
                let k = trip.k.min(trip.route.len() - 1);
                let (a, b) = (trip.route[k.saturating_sub(1)], trip.route[(k + 1).min(trip.route.len() - 1)]);
                let (dx, dz) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
                let length = (dx * dx + dz * dz).sqrt().max(1e-3);
                let side = if self.rng.gen_bool(0.5) { 1.0 } else { -1.0 };
                let camp = (trip.route[k].0 + (-dz / length * CAMP_OFFSET_BLOCKS * side).round() as i32, trip.route[k].1 + (dx / length * CAMP_OFFSET_BLOCKS * side).round() as i32);
                let person = &mut self.people[i];
                person.path = VecDeque::from([camp]);
                person.task = Task::Going(Goal::Camp);
                person.destination = None;
                if let Some(c) = person.creature.and_then(|id| world.creature_mut(id)) {
                    c.guide = None;
                }
            }
            _ => {
                // En marche : d'un point du chemin au suivant
                self.people[i].task = Task::Travelling;
                let Some(id) = self.people[i].creature else { return };
                let Some(c) = world.creature_mut(id) else { return };
                let person = &mut self.people[i];
                let Role::Traveller(t) = &mut person.role else { return };
                if c.guide.is_none() || c.arrived {
                    if c.arrived {
                        t.k += 1;
                        person.progress = (f32::INFINITY, 0.0);
                    } else {
                        // Reprise après une fuite ou une halte : le point du chemin le plus proche devant lui
                        let near = (t.k.saturating_sub(12)..(t.k + 12).min(t.route.len())).min_by(|&a, &b| {
                            let da = cell_dir(&geometry, face, t.route[a]).distance(position.normalize_or_zero());
                            let db = cell_dir(&geometry, face, t.route[b]).distance(position.normalize_or_zero());
                            da.total_cmp(&db)
                        });
                        t.k = near.map_or(t.k, |n| n + 1).min(t.route.len());
                    }
                    c.arrived = false;
                    if t.k >= t.route.len() {
                        c.guide = None;
                        self.arrive_at(i, generator);
                        return;
                    }
                    c.guide = Some(cell_dir(&geometry, face, t.route[t.k]));
                    c.guide_pace = 1.0;
                    c.look = None;
                    c.pause = None;
                }
                // Coincé (un arbre, une maison posée sur le chemin) : il saute ce point
                let distance = c.guide.map_or(0.0, |g| (g * c.position.length()).distance(c.position));
                let (best, since) = person.progress;
                if distance < best - 0.3 * geometry.block_height as f32 {
                    person.progress = (distance, 0.0);
                } else {
                    person.progress.1 = since + dt;
                    if person.progress.1 > STUCK_TIME {
                        person.progress = (f32::INFINITY, 0.0);
                        c.arrived = true;
                    }
                }
            }
        }
    }

    fn wake(&mut self, i: usize, world: &mut CreatureWorld) {
        if let Some(c) = self.people[i].creature.and_then(|id| world.creature_mut(id)) {
            c.sleeping = false;
            c.pause = None;
        }
        self.people[i].task = Task::Idle;
        self.people[i].timer = self.rng.gen_range(1.0..4.0);
    }

    /// Arrivé au bout du chemin : il visite l'établissement un moment
    fn arrive_at(&mut self, i: usize, generator: &PlanetWorldGenerator) {
        let face = self.people[i].face;
        let Role::Traveller(t) = &mut self.people[i].role else { return };
        let key = (face, t.to.0, t.to.1);
        t.visiting = Some((key, self.rng.gen_range(90.0..300.0)));
        if !self.towns.contains_key(&key) {
            if let Some(s) = generator.structures.at(generator, face, t.to.0, t.to.1) {
                if s.kind != SettlementKind::Ruins {
                    self.found_town(s, generator);
                }
            }
        }
        let person = &mut self.people[i];
        person.task = Task::Idle;
        person.timer = 0.5;
        person.path.clear();
    }

    /// Fin de la visite : il repart par un autre chemin que celui d'où il vient
    fn depart(&mut self, i: usize, key: TownKey, world: &mut CreatureWorld, generator: &PlanetWorldGenerator) {
        let face = key.0;
        let Some(s) = generator.structures.at(generator, face, key.1, key.2) else {
            self.people[i].alive = false;
            return;
        };
        let roads = roads_from(generator, s);
        let Role::Traveller(t) = &self.people[i].role else { return };
        let came_from = t.from;
        let roads: Vec<&Road> = roads;
        let choices: Vec<&&Road> = roads.iter().filter(|r| other_end(r, s.center) != came_from).collect();
        let road = if choices.is_empty() { roads.first().copied() } else { Some(choices[self.rng.gen_range(0..choices.len())]).copied() };
        let Some(road) = road else {
            // Cul-de-sac : il reste encore
            if let Role::Traveller(t) = &mut self.people[i].role {
                t.visiting = Some((key, 120.0));
            }
            return;
        };
        let mut route = route_along(road, s.center);
        let to = other_end(road, s.center);
        self.end_chat(i, world);
        // Il quitte la ville par les rues jusqu'au départ du chemin, puis suit la route
        let here = self.people[i].creature.and_then(|id| world.creatures().iter().find(|c| c.id == id)).and_then(|c| Self::cell_of(world, c.position)).map(|(_, cell)| cell);
        if let (Some(here), Some(town), Some(&start)) = (here, self.towns.get(&key), route.first()) {
            if let Some(mut streets) = town.nav.path(here, start) {
                streets.extend(route);
                route = streets;
            }
        }
        let person = &mut self.people[i];
        if let Role::Traveller(t) = &mut person.role {
            *t = Trip { from: s.center, to, route, k: 0, frac: 0.0, visiting: None, walk_speed: t.walk_speed };
        }
        person.task = Task::Travelling;
        person.path.clear();
        if let Some(c) = person.creature.and_then(|id| world.creature_mut(id)) {
            c.guide = None;
            c.arrived = false;
            c.pause = None;
            c.look = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::TerrainEdits;
    use crate::cubed_sphere::PlanetDescriptor;

    fn generator() -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() })
    }

    #[test]
    fn the_nav_grid_finds_a_way_around_a_wall() {
        let mut nav = NavGrid::new(Rect::new(0, 0, 20, 20));
        nav.set_rect(Rect::new(10, 0, 11, 18), true);
        let path = nav.path((2, 2), (17, 2)).unwrap();
        assert_eq!(*path.last().unwrap(), (17, 2));
        // Il passe par le trou au bout du mur
        assert!(path.iter().any(|c| c.1 >= 18), "{path:?}");
        // Chaque tronçon est praticable en ligne droite
        let mut from = (2, 2);
        for &c in &path {
            assert!(nav.line_clear(from, c), "{from:?} → {c:?}");
            from = c;
        }
        nav.set_rect(Rect::new(10, 18, 11, 20), true);
        assert!(nav.path((2, 2), (17, 2)).is_none());
    }

    #[test]
    fn a_route_follows_the_road_from_either_end() {
        let road = Road::for_tests(PlanetFace::PosX, (0, 0), (30, 0), vec![(0.5, 0.5), (15.5, 0.5), (30.5, 0.5)]);
        let forth = route_along(&road, (0, 0));
        let back = route_along(&road, (30, 0));
        assert_eq!((forth[0], *forth.last().unwrap()), ((0, 0), (30, 0)));
        assert_eq!((back[0], *back.last().unwrap()), ((30, 0), (0, 0)));
        assert!(forth.len() >= 10);
    }

    /// Une ville ou un village de la graine 2026, et les pieds d'un joueur à son centre
    fn settlement(generator: &PlanetWorldGenerator) -> (Settlement, Vec3) {
        let face = PlanetFace::PosY;
        for rz in 0..crate::structures::REGIONS_PER_FACE {
            for rx in 0..crate::structures::REGIONS_PER_FACE {
                if let Some(s) = generator.structures.region_settlement(generator, face, rx, rz) {
                    let geometry = BlockGeometry::new(generator.base_radius());
                    let edits = TerrainEdits::new(None);
                    let mut query = TerrainQuery::new(generator, &edits);
                    let feet = feet_at(&geometry, &mut query, face, s.center, None).unwrap();
                    return (s.clone(), feet);
                }
            }
        }
        panic!("aucun établissement sur la face");
    }

    #[test]
    fn residents_have_homes_jobs_and_a_culture() {
        let generator = generator();
        let (s, _) = settlement(&generator);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut villages = Villages::new(1, &registry);
        assert!(villages.enabled);
        villages.found_town(&s, &generator);
        let key = (s.face, s.center.0, s.center.1);
        let town = &villages.towns[&key];
        assert!(!town.homes.is_empty());
        let residents: Vec<&Person> = villages.people.iter().filter(|p| matches!(p.role, Role::Resident { .. })).collect();
        assert!(residents.len() >= town.homes.len(), "{} habitants pour {} maisons", residents.len(), town.homes.len());
        // Des gardes, des gens de métier, et d'autres sans métier
        assert!(residents.iter().any(|p| p.guard));
        assert!(residents.iter().any(|p| !p.card.profession.is_empty() && !p.guard));
        assert!(residents.iter().any(|p| p.card.profession.is_empty()));
        // Chaque porte de maison mène à la place
        for home in &town.homes {
            assert!(town.nav.path(home.out, town.plaza).is_some(), "maison en {:?} coupée de la place", home.out);
        }
        // Le même établissement donne les mêmes habitants
        let mut again = Villages::new(1, &registry);
        again.found_town(&s, &generator);
        let names = |v: &Villages| v.people.iter().filter(|p| matches!(p.role, Role::Resident { .. })).map(|p| p.card.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&villages), names(&again));
    }

    #[test]
    fn residents_walk_the_streets_by_day_and_go_home_at_night() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (s, feet) = settlement(&generator);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        world.set_sun_dir(feet.normalize());
        world.tick(0.05, &generator, &edits, &[feet]);
        let key = (s.face, s.center.0, s.center.1);
        assert!(world.villages.towns[&key].active);
        let humanoids = |w: &CreatureWorld| w.creatures().iter().filter(|c| c.managed && w.villager_card(c.id).is_some()).count();
        let out = humanoids(&world);
        assert!(out > 3, "{out} habitants dehors en plein jour");
        let start: Vec<(u64, Vec3)> = world.creatures().iter().filter(|c| c.managed).map(|c| (c.id, c.position)).collect();
        for _ in 0..600 {
            world.tick(0.1, &generator, &edits, &[feet]);
        }
        let block = world.geometry().block_height as f32;
        let moved = start.iter().filter(|(id, p)| world.creatures().iter().find(|c| c.id == *id).is_some_and(|c| c.position.distance(*p) > 2.0 * block)).count();
        assert!(moved > 0, "personne n'a bougé en une minute");
        // La nuit tombe : ils rentrent (sauf les gardes de nuit et les voyageurs)
        world.set_sun_dir(-feet.normalize());
        for _ in 0..1800 {
            world.tick(0.1, &generator, &edits, &[feet]);
        }
        let residents_out = world.creatures().iter().filter(|c| c.managed).filter(|c| world.villager_card(c.id).is_some_and(|card| card.origin.starts_with("Habitant") && card.profession != "Garde")).count();
        assert!(residents_out <= out / 4, "{residents_out} habitants encore dehors la nuit (sur {out})");
        let indoors = world.villages.people.iter().filter(|p| p.task == Task::Indoors).count();
        assert!(indoors > 0);
        // Au matin, ils ressortent
        world.set_sun_dir(feet.normalize());
        for _ in 0..100 {
            world.tick(0.1, &generator, &edits, &[feet]);
        }
        assert!(humanoids(&world) > out / 2);
    }

    #[test]
    fn travellers_follow_the_roads_far_from_players() {
        let generator = generator();
        let (s, _) = settlement(&generator);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut villages = Villages::new(3, &registry);
        let roads = roads_from(&generator, &s);
        assert!(!roads.is_empty(), "établissement sans chemin");
        let mut rng = StdRng::seed_from_u64(5);
        let route = route_along(roads[0], s.center);
        let len = route.len();
        villages.add_traveller(s.face, s.center, other_end(roads[0], s.center), route, 0, None, &mut rng);
        let i = villages.people.len() - 1;
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        let edits = TerrainEdits::new(None);
        let mut query = TerrainQuery::new(&generator, &edits);
        let mut arrived = false;
        for _ in 0..(len as f32 * ROUTE_SPACING as f32 / 1.5 / 0.5) as usize + 20 {
            villages.think_traveller(i, &mut world, &mut query, &generator, 0.5);
            if let Role::Traveller(t) = &villages.people[i].role {
                if t.visiting.is_some() {
                    arrived = true;
                    break;
                }
            }
        }
        assert!(arrived, "le voyageur n'est jamais arrivé");
    }
}

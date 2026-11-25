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
//! heure, il rentre chez lui et disparaît derrière sa porte jusqu'au matin. Les gardes se tiennent
//! de part et d'autre des portes, sans les quitter ; la moitié d'entre eux veille la nuit. Ils chassent les bêtes qui
//! approchent (à dix blocs, sans les poursuivre au-delà de trente) et s'en prennent au joueur qui frappe ou tue un habitant (`defends_herd`) ; les autres fuient
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
    /// Zone de rassemblement autour de la fontaine ou du puits : des colonnes où se tenir, chacun
    /// la sienne, de la plus proche à la plus lointaine, et le centre vers lequel on se tourne
    gathering: Vec<Cell>,
    gathering_center: Cell,
    /// Seuil de la taverne, où les voyageurs passent la nuit
    inn: Option<Home>,
    /// Nom de la ville ou du village
    name: String,
    /// Du centre à la maison la plus éloignée, en blocs : une grande ville s'éveille dès qu'un
    /// joueur approche de ses murs, pas seulement de son centre
    radius: f32,
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
    /// Habitants (hors gardes) qui ont un corps, par établissement : recompté à chaque pas
    outdoors: HashMap<TownKey, usize>,
    /// Positions des joueurs au pas en cours
    players: Vec<Vec3>,
}

impl Default for Villages {
    fn default() -> Self {
        Self { seed: 0, enabled: false, species: SpeciesIds::default(), towns: HashMap::new(), people: Vec::new(), cards: HashMap::new(), rng: StdRng::seed_from_u64(0), scan_timer: 0.0, outdoors: HashMap::new(), players: Vec::new() }
    }
}

/// Habitants dehors en même temps dans un établissement, gardes non compris : les autres
/// travaillent chez eux ou à l'atelier, sans corps, et sortent quand une place se libère
fn outdoor_cap(homes: usize) -> usize {
    (12 + homes / 8).min(40)
}

/// Près d'un joueur, un habitant a toujours un corps de jour (le plafond ne compte pas pour lui) :
/// il ne rentre pas pour une tâche à l'intérieur, et celui qui est chez lui sort à son passage
const NEAR_PLAYER_BLOCKS: f32 = 32.0;

/// Durée d'une tâche à l'intérieur (atelier, cuisine, ménage), en secondes
const INDOOR_TASK: std::ops::Range<f32> = 90.0..300.0;

/// Coiffures qui se lisent comme celles d'une femme
const FEMININE_HAIR: [&str; 7] = ["chignon", "longue", "lisse", "afro", "tresse", "nattes", "couettes"];
/// Coiffures d'homme (aucune ne se lit comme celle d'une femme)
const MASCULINE_HAIR: [&str; 13] = ["rasee", "courte", "brosse", "degrade", "frange", "bol", "raie", "arriere", "herisse", "tonsure", "mi", "queue", "mulet"];

/// L'apparence se lit comme celle d'une femme : sans barbe (les naines ont la leur) et coiffée en
/// femme
pub fn looks_feminine(look: &herocraft_common::hero::Appearance) -> bool {
    let beardless = look.barbe == "aucune" || look.espece == "nain";
    beardless && FEMININE_HAIR.contains(&look.coiffure.as_str())
}

/// Noms tirés par culture : un début et une fin, au féminin ou au masculin
fn random_name(culture: Culture, feminine: bool, rng: &mut StdRng) -> String {
    let (starts, ends): (&[&str], &[&str]) = match (culture, feminine) {
        (Culture::Humains, false) => (
            &["Jeh", "Thib", "Perr", "Gaut", "Bertr", "Ren", "Arn", "Guill", "Hug", "Colin", "Rob", "Gerv", "Bald", "Eud", "Lamb"],
            &["an", "aut", "in", "ier", "ot", "and", "aud", "on", "ert", "ard", "ric"],
        ),
        (Culture::Humains, true) => (
            &["Aud", "Isab", "Marg", "Clém", "Blanch", "Mahi", "Ysol", "Agn", "Ermen", "Berth", "Ali", "Ros", "Jeann", "Perrin", "Gaut"],
            &["e", "ette", "ine", "elle", "ence", "otte", "aude", "ise", "ie", "eline"],
        ),
        (Culture::Nains, false) => (&["Thor", "Bram", "Dur", "Grim", "Kar", "Bor", "Dag", "Orm", "Gund", "Sto"], &["grim", "in", "dak", "rik", "dun", "gar", "bek", "rak", "nar"]),
        (Culture::Nains, true) => (&["Hal", "Brun", "Dis", "Hel", "Sig", "Ast", "Gunn", "Thyr", "Ingr", "Bryn"], &["na", "hild", "ra", "li", "dis", "run", "veig", "ja", "dra"]),
        (Culture::ElfesDesBois, false) => (&["Ael", "Thal", "Fen", "Oro", "Lin", "Eir", "Syl", "Ilv"], &["dil", "las", "or", "ion", "ir", "andir", "oth", "ael"]),
        (Culture::ElfesDesBois, true) => (&["Syl", "Fae", "Nim", "Ari", "Ilv", "Lin", "Eir", "Ael"], &["wen", "rien", "iel", "wyn", "ith", "anna", "eth", "ia"]),
        (Culture::HautsElfes, false) => (&["Cael", "Vael", "Ser", "Tir", "Elo", "Ilo"], &["andor", "thas", "dris", "eon", "marin", "nael", "ion"]),
        (Culture::HautsElfes, true) => (&["Ithi", "Ara", "Lumi", "Sae", "Elo", "Ser"], &["riel", "lise", "esse", "wyn", "anne", "iel", "ena"]),
        (Culture::DemiOrques, false) => (&["Gor", "Thrak", "Ug", "Mog", "Kru", "Zag", "Brug", "Dorg", "Grom"], &["ash", "uk", "nak", "rag", "gul", "dush", "mok", "gar"]),
        (Culture::DemiOrques, true) => (&["Sha", "Gra", "Ur", "Mag", "Ruu", "Zhe", "Bag", "Kha"], &["ka", "rha", "gha", "dra", "mosh", "ra", "na", "gah"]),
        (Culture::Revenants, false) => (&["Mor", "Vesp", "Ond", "Sil", "Cend", "Nocht", "Brum", "Lith"], &["ain", "rel", "ost", "ur", "ias", "orn", "ide", "al"]),
        (Culture::Revenants, true) => (&["Morw", "Vesp", "Ond", "Sil", "Cend", "Ylv", "Brum", "Lith"], &["ine", "era", "a", "ue", "ia", "ève", "ée", "ys"]),
        (Culture::Sylphes, false) => (&["Zeph", "Aer", "Ven", "Bris", "Cirr", "Al", "Fyr", "Sour"], &["ir", "el", "ion", "an", "us", "ast", "en", "is"]),
        (Culture::Sylphes, true) => (&["Zeph", "Aer", "Ven", "Bris", "Cirr", "Al", "Fyr", "Sour"], &["ire", "elle", "ia", "ane", "ée", "a", "ine", "is"]),
        (Culture::Gnomes, false) => (&["Fizz", "Bim", "Tock", "Nim", "Pip", "Gim", "Wob", "Zook", "Dabb"], &["wick", "ble", "bert", "o", "kin", "pot", "nob", "ster"]),
        (Culture::Gnomes, true) => (&["Fizz", "Bim", "Tilly", "Nim", "Pip", "Lil", "Wob", "Zook", "Dabb"], &["ette", "belle", "a", "ina", "kin", "wyn", "ie", "ra"]),
        (Culture::Aasimars, false) => (&["Aur", "Sol", "Cel", "Ser", "Lum", "Ori", "Hel", "Zar"], &["iel", "on", "as", "ael", "ian", "ius", "or", "eth"]),
        (Culture::Aasimars, true) => (&["Aur", "Sol", "Cel", "Ser", "Lum", "Ori", "Hel", "Zar"], &["iela", "ine", "ise", "aëlle", "iane", "a", "ora", "eth"]),
        (Culture::HommesBetes, false) => (&["Rakh", "Fen", "Tar", "Grif", "Kaz", "Bor", "Shir", "Vulk"], &["an", "rok", "ash", "ur", "kar", "ruk", "ix", "ok"]),
        (Culture::HommesBetes, true) => (&["Rakh", "Fen", "Tar", "Nyr", "Kaz", "Ssa", "Shir", "Vel"], &["ia", "ra", "sha", "ya", "ka", "ri", "ix", "ane"]),
    };
    format!("{}{}", starts[rng.gen_range(0..starts.len())], ends[rng.gen_range(0..ends.len())])
}

/// Apparence d'une personne de cette culture et de ce métier : tirée au hasard, accordée à son
/// sexe (barbe et coiffure), puis vêtue selon le métier
fn appearance_for(culture: Culture, profession: &Profession, feminine: bool, rng: &mut StdRng) -> herocraft_common::hero::Appearance {
    let mut look = {
        let mut draw = || rng.gen::<f64>();
        atlas().aleatoire(&mut draw, Some(culture.espece_id()))
    };
    if feminine {
        // Les naines gardent la barbe de leur peuple
        if look.espece != "nain" {
            look.barbe = "aucune".to_string();
        }
        if !FEMININE_HAIR.contains(&look.coiffure.as_str()) {
            look.coiffure = FEMININE_HAIR[rng.gen_range(0..FEMININE_HAIR.len())].to_string();
        }
    } else if !MASCULINE_HAIR.contains(&look.coiffure.as_str()) {
        look.coiffure = MASCULINE_HAIR[rng.gen_range(0..MASCULINE_HAIR.len())].to_string();
    }
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

/// « de Brumeval », « d'Ithilien »
fn of_town(name: &str) -> String {
    if name.starts_with(['A', 'E', 'I', 'O', 'U', 'Y', 'É']) {
        format!("d'{name}")
    } else {
        format!("de {name}")
    }
}

/// Nom d'une ville ou d'un village, tiré de sa place et de sa culture
pub fn town_name(s: &Settlement) -> String {
    let h = mix(hash2(0x0A3E ^ ((face_index(s.face) as u64) << 40), s.center.0, s.center.1));
    let (starts, ends): (&[&str], &[&str]) = match s.culture.unwrap_or(Culture::Humains) {
        Culture::Humains => (
            &["Brume", "Clair", "Haut", "Roche", "Beau", "Vert", "Fonte", "Mont", "Pierre", "Saule", "Bois", "Val", "Gué", "Champ", "Chêne", "Loup"],
            &["val", "fort", "mont", "bourg", "lieu", "ville", "fontaine", "pré", "combe", "brune", "rive", "croix"],
        ),
        Culture::Nains => (&["Khaz", "Dur", "Bar", "Grim", "Thor", "Kar", "Ered", "Bram", "Gund", "Orm"], &["ak", "dum", "heim", "grund", "bar", "zar", "dor", "hold", "karak"]),
        Culture::ElfesDesBois => (&["Syl", "Lor", "Fael", "Eryn", "Thal", "Nim", "Aer", "Linde"], &["wen", "lindë", "dor", "lóme", "anor", "thil", "nost", "lad"]),
        Culture::HautsElfes => (&["Ith", "Caer", "Vael", "Sil", "Aer", "Lum", "Tir", "Ela"], &["ilien", "andor", "esse", "marë", "athil", "ion", "orië", "vanyar"]),
        Culture::DemiOrques => (&["Gor", "Krag", "Thrak", "Mog", "Uruk", "Zag", "Brug", "Dush"], &["gash", "mar", "thor", "nak", "ruk", "gor", "bal", "kul"]),
        Culture::Revenants => (&["Brume", "Noir", "Cendre", "Sel", "Morte", "Grève", "Pâle", "Ombre"], &["rive", "lac", "anse", "grève", "étang", "marais", "eau", "port"]),
        Culture::Sylphes => (&["Zéph", "Aéri", "Ventu", "Cirr", "Alti", "Nuée", "Bris", "Hau"], &["mont", "pic", "aire", "cime", "flèche", "haut", "voile", "crête"]),
        Culture::Gnomes => (&["Bric", "Fouil", "Taupe", "Cuivre", "Rouage", "Butte", "Terrier", "Clou"], &["bourg", "trou", "fosse", "ville", "nid", "motte", "creux", "boule"]),
        Culture::Aasimars => (&["Aur", "Sol", "Cél", "Lumi", "Héli", "Ori", "Séra", "Zén"], &["ia", "ion", "ade", "elle", "ys", "or", "iel", "ane"]),
        Culture::HommesBetes => (&["Croc", "Griffe", "Fauve", "Crin", "Corne", "Pelage", "Rugi", "Traque"], &["roc", "val", "sente", "terre", "lande", "gîte", "tanière", "piste"]),
    };
    let start = starts[(h % starts.len() as u64) as usize];
    let end = ends[(h / 97 % ends.len() as u64) as usize];
    format!("{start}{end}")
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
    let feminine = rng.gen_bool(0.5);
    card_of_sex(culture, profession, origin, feminine, rng)
}

/// Un habitant tiré comme dans les villages, d'une culture et d'un métier pris au hasard de
/// `seed`, femme ou homme (pour les aperçus)
pub fn sample_villager(seed: u64, feminine: bool) -> Option<Arc<VillagerCard>> {
    let mut rng = StdRng::seed_from_u64(seed);
    let registry = herocraft_common::professions();
    let residents: Vec<&Profession> = registry.iter().filter(|p| !p.traveller && !p.name.is_empty()).collect();
    let profession = *residents.get(rng.gen_range(0..residents.len().max(1)))?;
    let culture = Culture::ALL[rng.gen_range(0..Culture::ALL.len())];
    Some(card_of_sex(culture, profession, "Habitant".to_string(), feminine, &mut rng))
}

fn card_of_sex(culture: Culture, profession: &Profession, origin: String, feminine: bool, rng: &mut StdRng) -> Arc<VillagerCard> {
    let mut appearance = appearance_for(culture, profession, feminine, rng);
    // Le nom suit ce que l'apparence montre
    let name = random_name(culture, looks_feminine(&appearance), rng);
    appearance.nom = name.clone();
    let greeting = if profession.lines.is_empty() { "Bonjour.".to_string() } else { profession.lines[rng.gen_range(0..profession.lines.len())].clone() };
    let profession_name = profession.name_for(looks_feminine(&appearance)).to_string();
    let origin = match origin.strip_prefix("Habitant ") {
        Some(rest) if looks_feminine(&appearance) => format!("Habitante {rest}"),
        _ => origin,
    };
    Arc::new(VillagerCard { name, profession: profession_name, culture: culture.name().to_string(), origin, appearance, trades: trades_for(profession, rng), greeting })
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
        let mut landmark: Option<Rect> = None;
        let mut doors = Vec::new();
        let mut gates = Vec::new();
        let mut passages = Vec::new();
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
                Piece::Well { rect, .. }
                | Piece::Fountain { rect, .. }
                | Piece::Arena { rect, .. }
                | Piece::Ossuary { rect, .. }
                | Piece::Obelisk { rect, .. }
                | Piece::Sanctuary { rect, .. } => {
                    nav.set_rect(*rect, true);
                    let (_, out) = edge_of(rect, Side::South);
                    plaza.get_or_insert(out);
                    landmark.get_or_insert(*rect);
                }
                Piece::Tower { rect, .. } | Piece::Rampart { rect, .. } => nav.set_rect(*rect, true),
                Piece::Gate { rect, walk: false, .. } => gates.push(*rect),
                // Sous le chemin de ronde, le passage de la porte reste ouvert
                Piece::Gate { rect, walk: true, .. } => passages.push(*rect),
                Piece::Lamp { x, z, .. } | Piece::Ladder { x, z, .. } | Piece::Totem { x, z, .. } => nav.set((*x, *z), true),
                Piece::Terrace { rect, .. } => {
                    let (cx, cz) = rect.center();
                    nav.set_rect(Rect::new(cx - 1, cz - 1, cx + 1, cz + 1), true);
                }
                Piece::Pen { rect } => nav.set_rect(*rect, true),
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
        for gate in gates.iter().chain(&passages) {
            nav.set_rect(*gate, false);
        }
        for door in doors {
            nav.set(door, false);
        }
        wander.retain(|c| nav.walkable(*c));
        let plaza = plaza.unwrap_or(s.center);
        let landmark = landmark.unwrap_or(Rect::around(plaza.0, plaza.1, 0));
        let gathering_center = landmark.center();
        let gathering = gathering_cells(&nav, landmark);
        // Les gardes : de part et d'autre de chaque porte de la ville, juste en dedans et tournés vers
        // dehors ; ou des deux côtés du puits du village. Chacun a sa colonne : ils ne s'empilent pas.
        let away = |cell: Cell| {
            let (dx, dz) = (cell.0 - s.center.0, cell.1 - s.center.1);
            (cell.0 + dx.signum() * 4, cell.1 + dz.signum() * 4)
        };
        let mut posts: Vec<Spot> = Vec::new();
        if gates.is_empty() {
            for side in [-2, 2] {
                let cell = nav.nearest_walkable((plaza.0 + side, plaza.1)).unwrap_or(plaza);
                if posts.iter().all(|p| p.cell != cell) {
                    posts.push(Spot { cell, look: away(cell), floor: None });
                }
            }
        } else {
            for g in &gates {
                let c = g.center();
                // Vers le centre (en dedans, le long de l'axe dominant), et le long du mur
                let (to_x, to_z) = (s.center.0 - c.0, s.center.1 - c.1);
                let inward = if to_x.abs() > to_z.abs() { (to_x.signum(), 0) } else { (0, to_z.signum()) };
                let along = if inward.0 == 0 { (1, 0) } else { (0, 1) };
                let half = if along == (1, 0) { g.width() } else { g.depth() } / 2 + 1;
                for side in [-1, 1] {
                    // Le premier pas libre en dedans, passé l'épaisseur du rempart et de son chemin de ronde
                    let beside = |d: i32| (c.0 + inward.0 * d + along.0 * half * side, c.1 + inward.1 * d + along.1 * half * side);
                    // (et relié à la place : un rempart en biais laisse des recoins fermés près des portes)
                    let free = (1..=6).map(beside).find(|&cell| nav.walkable(cell) && nav.path(cell, plaza).is_some());
                    let Some(cell) = free.or_else(|| nav.nearest_walkable(beside(1))) else { continue };
                    if posts.iter().all(|p| p.cell != cell) {
                        posts.push(Spot { cell, look: away(c), floor: None });
                    }
                }
            }
        }
        // Les gardes gardent leur poste libre des badauds
        let gathering = gathering.into_iter().filter(|c| posts.iter().all(|p| p.cell != *c)).collect();
        work.insert(Workplace::Rempart, posts);
        let radius = homes.iter().map(|h| (((h.door.0 - s.center.0).pow(2) + (h.door.1 - s.center.1).pow(2)) as f32).sqrt()).fold(0.0, f32::max);
        Town { nav: Arc::new(nav), homes, work, customers, wander, plaza, gathering, gathering_center, inn, name: town_name(s), radius, active: false }
    }

    fn random_street(&self, rng: &mut StdRng) -> Cell {
        if self.wander.is_empty() {
            self.plaza
        } else {
            self.wander[rng.gen_range(0..self.wander.len())]
        }
    }
}

/// Rayon de la zone de rassemblement autour de la fontaine ou du puits, en blocs
const GATHERING_RADIUS: i32 = 6;

/// Colonnes praticables autour de `landmark` (fontaine, puits) jusqu'à `GATHERING_RADIUS` blocs de
/// son bord, de la plus proche à la plus lointaine
fn gathering_cells(nav: &NavGrid, landmark: Rect) -> Vec<Cell> {
    let ring = |(x, z): Cell| {
        let dx = (landmark.x0 - x).max(x - (landmark.x1 - 1)).max(0);
        let dz = (landmark.z0 - z).max(z - (landmark.z1 - 1)).max(0);
        dx.max(dz)
    };
    let around = landmark.expand(GATHERING_RADIUS);
    let mut cells: Vec<Cell> = (around.z0..around.z1).flat_map(|z| (around.x0..around.x1).map(move |x| (x, z))).filter(|&c| ring(c) >= 1 && nav.walkable(c)).collect();
    let (cx, cz) = landmark.center();
    cells.sort_by_key(|&(x, z)| (ring((x, z)), (x - cx).abs() + (z - cz).abs(), x, z));
    cells
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
    /// Nom de la ville où vit (ou séjourne) la personne qu'incarne la créature `id` ; `None` sur
    /// les routes
    pub fn town_of(&self, id: u64) -> Option<&str> {
        let person = self.people.iter().find(|p| p.creature == Some(id))?;
        let key = match &person.role {
            Role::Resident { town, .. } => *town,
            Role::Traveller(trip) => trip.visiting?.0,
        };
        self.towns.get(&key).map(|t| t.name.as_str())
    }

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
        self.players.clear();
        self.players.extend_from_slice(players);
        self.outdoors.clear();
        for p in &self.people {
            if let (Role::Resident { town, .. }, Some(_), false, true) = (&p.role, p.creature, p.guard, p.alive) {
                *self.outdoors.entry(*town).or_insert(0) += 1;
            }
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
            let distance = Self::nearest_player(&geometry, cell_dir(&geometry, key.0, (key.1, key.2)), players) - town.radius;
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
        let origin = format!("Habitant {}", of_town(&town.name));
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
            let culture = s.resident_culture(n as u64);
            let card = card_for(culture, p, origin.clone(), &mut rng);
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
        let culture = if rng.gen_bool(0.55) { Culture::Humains } else { Culture::ALL[rng.gen_range(1..Culture::ALL.len())] };
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
        if let (Role::Resident { town, .. }, false) = (&person.role, person.guard) {
            *self.outdoors.entry(*town).or_insert(0) += 1;
        }
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
            if let (Role::Resident { town, .. }, false) = (&self.people[i].role, self.people[i].guard) {
                if let Some(n) = self.outdoors.get_mut(town) {
                    *n = n.saturating_sub(1);
                }
            }
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
            let out = (!self.is_night(person, sun) || person.night_watch) && self.room_outside(key, person.guard);
            if !out {
                // Chez lui : il sortira plus tard, quand une place se libère
                self.people[i].task = Task::Indoors;
                self.people[i].timer = self.rng.gen_range(0.0..INDOOR_TASK.end);
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

    /// La colonne `cell` d'une face est près d'un joueur
    fn near_player(&self, geometry: &BlockGeometry, face: PlanetFace, cell: Cell) -> bool {
        Self::nearest_player(geometry, cell_dir(geometry, face, cell), &self.players) < NEAR_PLAYER_BLOCKS
    }

    /// Il reste de la place dehors pour un habitant de l'établissement `key` (un garde sort toujours)
    fn room_outside(&self, key: TownKey, guard: bool) -> bool {
        guard || self.towns.get(&key).is_some_and(|t| self.outdoors.get(&key).copied().unwrap_or(0) < outdoor_cap(t.homes.len()))
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
            // La nuit chez lui, il sortira dès le matin
            if self.people[i].task == Task::Indoors && night {
                self.people[i].timer = self.people[i].timer.min(5.0);
            }
            // Chez lui : de jour, il sort sa tâche finie, s'il reste de la place dehors ; tout de
            // suite, quand un joueur passe près de chez lui
            if self.people[i].task == Task::Indoors && !night {
                let near = self.near_player(&geometry, key.0, home_spot.door);
                if near {
                    self.people[i].timer = self.people[i].timer.min(self.rng.gen_range(0.0..2.0));
                }
                self.people[i].timer -= dt;
                if self.people[i].timer > 0.0 {
                    return;
                }
                if !near && !self.room_outside(key, self.people[i].guard) {
                    self.people[i].timer = self.rng.gen_range(15.0..60.0);
                    return;
                }
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
                self.people[i].timer = self.rng.gen_range(INDOOR_TASK);
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
            Goal::Wander => {
                self.set_pause(world, id, species, None, look);
                let person = &mut self.people[i];
                person.task = Task::Idle;
                person.timer = self.rng.gen_range(2.0..8.0);
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
        // Un garde ne quitte pas l'entrée qu'il garde : il y revient (après une poursuite) et y reste
        if guard {
            if let Some(spot) = work {
                if self.send(i, &nav, here, spot.cell, Goal::Work, Some(spot)) {
                    return;
                }
            }
        }
        // Une tâche à l'intérieur : de temps en temps, et toujours quand la rue est trop peuplée
        let Role::Resident { home, .. } = self.people[i].role else { return };
        let crowded = !self.room_outside(key, false) && self.outdoors.get(&key).copied().unwrap_or(0) > outdoor_cap(self.towns[&key].homes.len());
        let near = self.near_player(world.geometry(), key.0, here);
        if !guard && !near && (crowded || self.rng.gen_bool(0.12)) {
            let home_spot = self.towns[&key].homes[home];
            let door = Spot { cell: home_spot.door, look: home_spot.door, floor: Some(home_spot.floor) };
            if self.send(i, &nav, here, home_spot.out, Goal::Home, Some(door)) {
                self.people[i].path.push_back(home_spot.door);
                return;
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
        if roll < 0.93 {
            if let Some(spot) = self.gathering_spot(i, key, world) {
                if self.send(i, &nav, here, spot.cell, Goal::Wander, Some(spot)) {
                    return;
                }
            }
        }
        let target = self.towns[&key].random_street(&mut self.rng);
        if !self.send(i, &nav, here, target, Goal::Wander, None) {
            self.people[i].task = Task::Idle;
            self.people[i].timer = self.rng.gen_range(2.0..6.0);
        }
    }

    /// Une colonne libre de la zone de rassemblement de l'établissement `key` pour la personne `i` :
    /// ni occupée par quelqu'un, ni promise à quelqu'un qui s'y rend. Tirée parmi les plus proches de
    /// la fontaine encore libres, pour que la foule se forme autour d'elle, chacun sur son bloc.
    fn gathering_spot(&mut self, i: usize, key: TownKey, world: &CreatureWorld) -> Option<Spot> {
        let town = self.towns.get(&key)?;
        let in_town = |p: &Person| match &p.role {
            Role::Resident { town, .. } => *town == key,
            Role::Traveller(t) => t.visiting.is_some_and(|(k, _)| k == key),
        };
        let mut taken: Vec<Cell> = Vec::new();
        for (j, p) in self.people.iter().enumerate() {
            if j == i || !p.alive || !in_town(p) {
                continue;
            }
            let Some(c) = p.creature.and_then(|id| world.creatures().iter().find(|c| c.id == id)) else { continue };
            if let Some((_, cell)) = Self::cell_of(world, c.position) {
                taken.push(cell);
            }
            // Le poste où il va, ou celui où il se tient
            if let Some(spot) = p.destination {
                taken.push(spot.cell);
            }
        }
        let free: Vec<Cell> = town.gathering.iter().copied().filter(|c| !taken.contains(c)).take(12).collect();
        let cell = *free.get(self.rng.gen_range(0..free.len().max(1)))?;
        Some(Spot { cell, look: town.gathering_center, floor: None })
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
        // Le voisin retenu a forcément une créature : sa position vient d'elle
        let Some(pid) = self.people[j].creature else { return false };
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
                    let spot = if roll < 0.75 { self.gathering_spot(i, key, world) } else { None };
                    let (target, spot) = match spot {
                        Some(spot) => (spot.cell, Some(spot)),
                        None => (self.towns[&key].random_street(&mut self.rng), None),
                    };
                    if !self.send(i, &nav, here, target, Goal::Wander, spot) {
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
    fn names_follow_what_the_appearance_shows() {
        let registry = herocraft_common::professions();
        let smith = registry.get(registry.find("forgeron").unwrap()).unwrap();
        let mut rng = StdRng::seed_from_u64(7);
        let (mut women, mut men) = (0, 0);
        for i in 0..200 {
            let culture = Culture::ALL[i % 4];
            let card = card_for(culture, smith, String::new(), &mut rng);
            let feminine = looks_feminine(&card.appearance);
            assert_eq!(card.appearance.nom, card.name);
            if feminine {
                women += 1;
                assert_eq!(card.profession, "Forgeronne");
                if card.appearance.espece != "nain" {
                    assert_eq!(card.appearance.barbe, "aucune", "{}", card.name);
                }
            } else {
                men += 1;
                assert_eq!(card.profession, "Forgeron");
                assert!(MASCULINE_HAIR.contains(&card.appearance.coiffure.as_str()), "{}", card.appearance.coiffure);
            }
        }
        assert!(women > 60 && men > 60, "{women} femmes, {men} hommes");
        // Les mêmes tirages donnent des noms différents selon le sexe
        let (mut a, mut b) = (StdRng::seed_from_u64(1), StdRng::seed_from_u64(1));
        let she: Vec<String> = (0..20).map(|_| random_name(Culture::Humains, true, &mut a)).collect();
        let he: Vec<String> = (0..20).map(|_| random_name(Culture::Humains, false, &mut b)).collect();
        assert_ne!(she, he);
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
        // Le plus petit établissement de la face : une capitale compterait des centaines d'habitants
        let face = PlanetFace::PosY;
        let mut found: Option<&Settlement> = None;
        for rz in 0..crate::structures::REGIONS_PER_FACE {
            for rx in 0..crate::structures::REGIONS_PER_FACE {
                if let Some(s) = generator.structures.region_settlement(generator, face, rx, rz) {
                    if found.is_none_or(|f| s.pieces.len() < f.pieces.len()) {
                        found = Some(s);
                    }
                }
            }
        }
        let s = found.expect("aucun établissement sur la face");
        let geometry = BlockGeometry::new(generator.base_radius());
        let edits = TerrainEdits::new(None);
        let mut query = TerrainQuery::new(generator, &edits);
        let feet = feet_at(&geometry, &mut query, face, s.center, None).unwrap();
        (s.clone(), feet)
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
    fn killing_a_resident_is_announced_with_the_town_and_the_guards_hunt_the_killer() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (s, feet) = settlement(&generator);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        world.set_sun_dir(feet.normalize());
        world.tick(0.05, &generator, &edits, &[feet]);
        let name = town_name(&s);
        let (id, herd, position, card) = world
            .creatures()
            .iter()
            .filter(|c| c.managed && c.is_alive())
            .find_map(|c| world.villager_card(c.id).filter(|card| card.origin.starts_with("Habitant") && card.profession != "Garde").map(|card| (c.id, c.herd, c.position, card.clone())))
            .expect("un habitant dehors");
        assert!(card.origin.ends_with(&name), "{} / {name}", card.origin);
        // Le premier coup est signalé à son auteur, une fois, avec la ville
        assert!(world.hit_by(id, position, 5));
        let assaults = world.take_assaults();
        assert_eq!(assaults.len(), 1);
        let warning = assaults[0].warning();
        assert!(warning.contains(&card.name) && warning.contains(&name) && warning.contains("gardes"), "{warning}");
        while world.hit_by(id, position, 5) {}
        assert!(world.take_assaults().is_empty(), "averti à chaque coup");
        let murders = world.take_murders();
        assert_eq!(murders.len(), 1);
        assert_eq!(murders[0].town.as_deref(), Some(name.as_str()));
        let line = murders[0].announce("Mat");
        println!("{line}");
        assert!(line.starts_with(&format!("Mat a tué {}", card.name)) && line.contains(&name) && line.contains("gardes"), "{line}");
        assert!(world.is_wanted(5, herd));
        assert!(world.take_murders().is_empty(), "annoncé deux fois");
    }

    /// Une ville fortifiée de la graine 2026
    fn walled_town(generator: &PlanetWorldGenerator) -> Settlement {
        for face in PlanetFace::ALL {
            for rz in 0..crate::structures::REGIONS_PER_FACE {
                for rx in 0..crate::structures::REGIONS_PER_FACE {
                    if let Some(s) = generator.structures.region_settlement(generator, face, rx, rz).filter(|s| s.size == Some(crate::structures::TownSize::PetiteVille)) {
                        return s.clone();
                    }
                }
            }
        }
        panic!("aucune petite ville fortifiée");
    }

    #[test]
    fn guards_each_hold_their_own_post_beside_a_gate() {
        let generator = generator();
        let s = walled_town(&generator);
        let town = Town::new(&s);
        // Les portes du rempart (pas les passages sous le chemin de ronde, qui les doublent en dedans)
        let gates: Vec<Rect> = s.pieces.iter().filter_map(|p| if let Piece::Gate { rect, walk: false, .. } = p { Some(*rect) } else { None }).collect();
        let openings: Vec<Rect> = s.pieces.iter().filter_map(|p| if let Piece::Gate { rect, .. } = p { Some(*rect) } else { None }).collect();
        let posts = &town.work[&Workplace::Rempart];
        assert!(posts.len() >= gates.len() * 2 - 1, "{} postes pour {} portes", posts.len(), gates.len());
        for (k, post) in posts.iter().enumerate() {
            assert!(town.nav.walkable(post.cell));
            // Chacun le sien, et tout près d'une porte
            assert!(posts[..k].iter().all(|p| p.cell != post.cell), "deux gardes sur {:?}", post.cell);
            let near = gates.iter().any(|g| g.expand(4).contains(post.cell.0, post.cell.1));
            assert!(near, "poste {:?} loin des portes", post.cell);
            // À côté du passage, pas dedans : les gardes ne bouchent pas la porte
            assert!(openings.iter().all(|g| !g.contains(post.cell.0, post.cell.1)), "poste {:?} dans le passage", post.cell);
        }
        // Les gardes ont chacun un poste, et un garde désœuvré y retourne au lieu de patrouiller
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut villages = Villages::new(1, &registry);
        villages.found_town(&s, &generator);
        let guards: Vec<Spot> = villages.people.iter().filter(|p| p.guard).filter_map(|p| match p.role {
            Role::Resident { work, .. } => work,
            _ => None,
        }).collect();
        assert!(!guards.is_empty());
        for (k, g) in guards.iter().enumerate() {
            assert!(guards[..k].iter().all(|o| o.cell != g.cell), "deux gardes au même poste");
        }
    }

    #[test]
    fn guards_stay_at_the_gates_by_day() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let s = walled_town(&generator);
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(&generator, &edits);
        let feet = feet_at(&geometry, &mut query, s.face, s.center, None).unwrap();
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        world.set_sun_dir(feet.normalize());
        // Une petite ville est plus grande que l'ancien bourg : les gardes ont plus de chemin à faire
        for _ in 0..1800 {
            world.tick(0.1, &generator, &edits, &[feet]);
        }
        let block = geometry.block_height as f32;
        // Presque tous à leur poste (dans une ville plus grande, l'un d'eux peut être encore en chemin)
        let (mut checked, mut at_post) = (0, 0);
        for p in world.villages.people.iter().filter(|p| p.guard) {
            let (Some(id), Role::Resident { work: Some(post), .. }) = (p.creature, &p.role) else { continue };
            let Some(c) = world.creatures().iter().find(|c| c.id == id).filter(|c| !c.is_hunting()) else { continue };
            let at = cell_dir(&geometry, s.face, post.cell) * c.position.length();
            let off = at.distance(c.position) / block;
            if off < 4.0 {
                at_post += 1;
            }
            checked += 1;
        }
        assert!(at_post >= 4 && at_post * 4 >= checked * 3, "{at_post} gardes sur {checked} à leur poste");
    }

    /// Dans une capitale de plusieurs centaines d'habitants, seuls quelques dizaines ont un corps à la
    /// fois : les autres travaillent chez eux et sortent à tour de rôle
    #[test]
    fn a_capital_animates_only_a_few_residents_at_a_time() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let capital = generator.structures.capital(&generator).expect("une capitale");
        let s = generator.structures.region_settlement(&generator, capital.face, capital.region.0, capital.region.1).expect("la capitale").clone();
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(&generator, &edits);
        let feet = feet_at(&geometry, &mut query, s.face, s.center, None).unwrap();
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        world.set_sun_dir(feet.normalize());
        let key = (s.face, s.center.0, s.center.1);
        let homes = |w: &CreatureWorld| w.villages.towns[&key].homes.len();
        let mut most = 0;
        for k in 0..600 {
            world.tick(0.1, &generator, &edits, &[feet]);
            if k == 0 {
                assert!(world.villages.towns[&key].active);
            }
            let out = world.villages.people.iter().filter(|p| !p.guard && p.creature.is_some() && matches!(p.role, Role::Resident { town, .. } if town == key)).count();
            most = most.max(out);
        }
        let residents = world.villages.people.iter().filter(|p| matches!(p.role, Role::Resident { town, .. } if town == key)).count();
        println!("{residents} habitants, au plus {most} dehors à la fois");
        assert!(residents > 400, "{residents} habitants");
        assert!(most > 10 && most <= outdoor_cap(homes(&world)), "{most} habitants dehors");
        // Et ils tournent : certains sont rentrés travailler chez eux pendant que d'autres sortaient
        let indoors_by_day = world.villages.people.iter().filter(|p| p.task == Task::Indoors && matches!(p.role, Role::Resident { town, .. } if town == key)).count();
        assert!(indoors_by_day > residents / 2);
    }

    #[test]
    fn residents_near_a_player_always_have_a_body_by_day() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let capital = generator.structures.capital(&generator).expect("une capitale");
        let s = generator.structures.region_settlement(&generator, capital.face, capital.region.0, capital.region.1).expect("la capitale").clone();
        let geometry = BlockGeometry::new(generator.base_radius());
        let mut query = TerrainQuery::new(&generator, &edits);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        let key = (s.face, s.center.0, s.center.1);
        let centre = feet_at(&geometry, &mut query, s.face, s.center, None).unwrap();
        world.set_sun_dir(centre.normalize());
        // On fonde la ville depuis le centre, puis le joueur va dans un quartier du bord, loin du centre
        world.tick(0.1, &generator, &edits, &[centre]);
        let far = |c: Cell| ((c.0 - s.center.0).pow(2) + (c.1 - s.center.1).pow(2)) as f32;
        let edge = world.villages.towns[&key].homes.iter().map(|h| h.out).max_by(|a, b| far(*a).total_cmp(&far(*b))).unwrap();
        let feet = feet_at(&geometry, &mut query, s.face, edge, None).unwrap();
        println!("joueur à {:.0} blocs du centre", far(edge).sqrt());
        assert!(far(edge).sqrt() > ACTIVE_RANGE_BLOCKS, "le bord doit être hors de portée du centre");
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry);
        world.set_sun_dir(feet.normalize());
        for _ in 0..300 {
            world.tick(0.1, &generator, &edits, &[feet]);
        }
        assert!(world.villages.towns[&key].active, "la capitale s'éveille quand on arrive par ses quartiers");
        let town = &world.villages.towns[&key];
        let near: Vec<&Person> = world.villages.people.iter().filter(|p| matches!(p.role, Role::Resident { town: t, home, .. } if t == key && far_from(town.homes[home].door, edge) < 20.0)).collect();
        let bodies = near.iter().filter(|p| p.creature.is_some()).count();
        println!("{bodies} / {} voisins du joueur ont un corps", near.len());
        assert!(!near.is_empty());
        assert!(bodies * 10 >= near.len() * 9, "{bodies} / {} voisins ont un corps", near.len());
    }

    fn far_from(a: Cell, b: Cell) -> f32 {
        (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f32).sqrt()
    }

    #[test]
    fn a_gathering_gives_everyone_a_free_block_of_their_own() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let (s, feet) = settlement(&generator);
        let registry = Arc::new(CreatureRegistry::load_default());
        let mut world = CreatureWorld::new(generator.base_radius(), 2026, registry.clone());
        world.set_sun_dir(feet.normalize());
        world.tick(0.05, &generator, &edits, &[feet]);
        let key = (s.face, s.center.0, s.center.1);
        let mut villages = std::mem::take(&mut world.villages);
        let out: Vec<usize> = (0..villages.people.len()).filter(|&i| villages.people[i].creature.is_some() && matches!(villages.people[i].role, Role::Resident { town, .. } if town == key)).collect();
        assert!(out.len() > 3, "{} habitants dehors", out.len());
        // Tous se rendent au rassemblement l'un après l'autre : chacun reçoit sa colonne
        let mut cells = Vec::new();
        for &i in &out {
            let spot = villages.gathering_spot(i, key, &world).expect("rassemblement plein");
            assert!(!cells.contains(&spot.cell), "{:?} donnée deux fois", spot.cell);
            assert!(villages.towns[&key].gathering.contains(&spot.cell));
            cells.push(spot.cell);
            villages.people[i].destination = Some(spot);
            villages.people[i].task = Task::Going(Goal::Wander);
        }
        world.villages = villages;
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

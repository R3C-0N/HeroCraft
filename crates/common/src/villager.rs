//! Métiers des habitants des villes et des villages, et des voyageurs qui vont de l'un à l'autre.
//! Chaque métier est décrit par un fichier JSON du dossier `assets/metiers/` : son nom, le lieu où
//! il s'exerce (forge, étal, champ, taverne, église, remparts), la créature qui l'incarne (un
//! habitant paisible ou un garde armé), ses tenues et les échanges qu'il propose au joueur.
//!
//! La simulation (`herocraft_world::villagers`) en tire les habitants de chaque établissement ; la
//! fiche d'un habitant (`VillagerCard`) voyage du serveur aux clients, qui le dessinent avec le corps
//! des héros et ouvrent ses échanges. Le format est décrit dans `assets/metiers/README.md`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::blocks::blocks;
use crate::hero::Appearance;
use crate::items::{items, ItemId};

/// Dossier des fiches de métiers, depuis le dossier de lancement
pub const PROFESSIONS_DIR: &str = "assets/metiers";

/// Lieu où s'exerce un métier, dans le plan d'une ville ou d'un village
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Workplace {
    /// Sans lieu : il flâne dans les rues (sans métier, enfant, voyageur)
    #[default]
    Aucun,
    /// L'appentis de la forge
    Forge,
    /// Derrière le comptoir d'un étal du marché
    Etal,
    /// Au bord d'un champ
    Champ,
    /// Dans la salle de la taverne
    Taverne,
    /// Devant l'autel de l'église
    Eglise,
    /// Aux portes et aux tours du rempart, ou au puits d'un village
    Rempart,
}

/// Un échange proposé au joueur : il donne `give`, il reçoit `get`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub give: Vec<(ItemId, u16)>,
    pub get: (ItemId, u16),
}

/// Un échange tel qu'écrit dans la fiche : identifiants d'objets ou de blocs et quantités
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TradeSheet {
    donne: BTreeMap<String, u16>,
    recoit: BTreeMap<String, u16>,
}

/// Un métier, tel que décrit par son fichier
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfessionSheet {
    name: String,
    /// Nom du métier au féminin ; vide : le même qu'au masculin
    #[serde(default)]
    nom_feminin: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    lieu: Workplace,
    /// Créature qui l'incarne (fichier de `assets/creatures/`)
    #[serde(default = "default_species")]
    espece: String,
    /// Nombre d'habitants de ce métier par lieu de travail (forge, étal, champ…)
    #[serde(default = "one")]
    par_lieu: u32,
    /// Exercé par des voyageurs plutôt que par des habitants
    #[serde(default)]
    voyageur: bool,
    /// Poids face aux autres métiers sans lieu (habitants sans métier, voyageurs)
    #[serde(default = "one_f32")]
    poids: f32,
    /// Tenues de l'atlas des héros, et leurs couleurs (`#rrggbb`) ; vides : celles du hasard
    #[serde(default)]
    tenues: Vec<String>,
    #[serde(default)]
    couleurs: Vec<String>,
    /// Échanges que le métier peut proposer ; chaque habitant en propose quelques-uns
    #[serde(default)]
    echanges: Vec<TradeSheet>,
    /// Ce qu'il dit quand on lui parle
    #[serde(default)]
    repliques: Vec<String>,
}

/// Un métier, échanges résolus en numéros d'objets
#[derive(Debug, Clone)]
pub struct Profession {
    /// Identifiant : le nom du fichier sans `.json`
    pub id: String,
    pub name: String,
    /// Nom du métier exercé par une femme
    pub name_feminine: String,
    pub description: String,
    pub workplace: Workplace,
    pub species: String,
    pub per_workplace: u32,
    pub traveller: bool,
    pub weight: f32,
    pub outfits: Vec<String>,
    pub colours: Vec<String>,
    pub trades: Vec<Trade>,
    pub lines: Vec<String>,
}

fn default_species() -> String {
    "villageois".to_string()
}

fn one() -> u32 {
    1
}

fn one_f32() -> f32 {
    1.0
}

/// Numéro d'un objet ou d'un bloc d'après son identifiant
pub fn find_item(id: &str) -> Option<ItemId> {
    blocks().find(id).map(ItemId::from).or_else(|| items().find(id))
}

impl Profession {
    /// Nom du métier accordé à qui l'exerce
    pub fn name_for(&self, feminine: bool) -> &str {
        if feminine {
            &self.name_feminine
        } else {
            &self.name
        }
    }

    /// Lit la fiche JSON d'un métier ; `find` donne le numéro d'un objet d'après son identifiant
    pub fn parse(id: &str, json: &str, find: &impl Fn(&str) -> Option<ItemId>) -> Result<Self, String> {
        let sheet: ProfessionSheet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if !(sheet.poids >= 0.0) || sheet.par_lieu == 0 {
            return Err("poids négatif ou par_lieu nul".into());
        }
        let resolve = |key: &str, count: u16| -> Result<(ItemId, u16), String> {
            if count == 0 {
                return Err(format!("« {key} » : quantité nulle"));
            }
            find(key).map(|item| (item, count)).ok_or_else(|| format!("objet « {key} » inconnu"))
        };
        let mut trades = Vec::new();
        for trade in &sheet.echanges {
            if trade.donne.is_empty() || trade.recoit.len() != 1 {
                return Err("un échange donne au moins un objet et en reçoit exactement un".into());
            }
            let give = trade.donne.iter().map(|(k, c)| resolve(k, *c)).collect::<Result<Vec<_>, _>>()?;
            let (key, count) = trade.recoit.iter().next().unwrap();
            trades.push(Trade { give, get: resolve(key, *count)? });
        }
        if sheet.couleurs.iter().any(|c| !(c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit()))) {
            return Err("couleur invalide (format #rrggbb)".into());
        }
        Ok(Profession {
            id: id.to_string(),
            name_feminine: if sheet.nom_feminin.is_empty() { sheet.name.clone() } else { sheet.nom_feminin },
            name: sheet.name,
            description: sheet.description,
            workplace: sheet.lieu,
            species: sheet.espece,
            per_workplace: sheet.par_lieu,
            traveller: sheet.voyageur,
            weight: sheet.poids,
            outfits: sheet.tenues,
            colours: sheet.couleurs,
            trades,
            lines: sheet.repliques,
        })
    }
}

/// Tous les métiers connus, rangés par identifiant
#[derive(Debug, Clone, Default)]
pub struct ProfessionRegistry {
    professions: Vec<Profession>,
}

impl ProfessionRegistry {
    /// Les métiers de `assets/metiers/`, depuis le dossier de lancement, sinon ceux livrés avec les
    /// sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(PROFESSIONS_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PROFESSIONS_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {PROFESSIONS_DIR}/ introuvable : villages sans habitants.");
            return Self::default();
        };
        match Self::load_dir(dir, &find_item) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::default()
            }
        }
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les métiers valides et les erreurs des autres.
    pub fn load_dir(dir: &Path, find: &impl Fn(&str) -> Option<ItemId>) -> io::Result<(Self, Vec<String>)> {
        let mut professions = Vec::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| Profession::parse(&id, &text, find));
            match result {
                Ok(p) => professions.push(p),
                Err(e) => errors.push(format!("Métier {} ignoré : {e}", path.display())),
            }
        }
        Ok((Self::from_professions(professions), errors))
    }

    pub fn from_professions(mut professions: Vec<Profession>) -> Self {
        professions.sort_by(|a, b| a.id.cmp(&b.id));
        Self { professions }
    }

    pub fn get(&self, index: usize) -> Option<&Profession> {
        self.professions.get(index)
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.professions.iter().position(|p| p.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Profession> {
        self.professions.iter()
    }

    pub fn len(&self) -> usize {
        self.professions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.professions.is_empty()
    }
}

/// Les métiers, lus une fois
pub fn professions() -> &'static ProfessionRegistry {
    static REGISTRY: OnceLock<ProfessionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(ProfessionRegistry::load_default)
}

/// Fiche d'un habitant ou d'un voyageur : qui il est, à quoi il ressemble, ce qu'il échange. Le
/// serveur l'envoie avec la créature qui l'incarne.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VillagerCard {
    pub name: String,
    /// Nom du métier (« Forgeronne », « Garde »…) ; vide : sans métier
    pub profession: String,
    /// Culture (« Humains », « Nains »…)
    pub culture: String,
    /// D'où il est : « du village », « de la ville », ou sa route s'il voyage
    pub origin: String,
    pub appearance: Appearance,
    /// Échanges qu'il propose (vide : il n'a rien à échanger)
    pub trades: Vec<Trade>,
    /// Ce qu'il dit quand on lui parle
    pub greeting: String,
}

/// Distance jusqu'où l'on entend un habitant parler, en blocs : seuls les joueurs aussi proches
/// lisent dans le chat ce qu'il dit
pub const VILLAGER_HEARING_BLOCKS: f32 = 16.0;

impl VillagerCard {
    /// N'a rien à échanger : on bavarde avec lui dans le chat au lieu d'ouvrir l'écran des échanges
    pub fn chats_only(&self) -> bool {
        self.trades.is_empty()
    }

    /// Nom sous lequel il parle dans le chat : « Aude » ou « Bertrand (Garde) »
    pub fn speaker(&self) -> String {
        if self.profession.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.profession)
        }
    }

    /// Ce qu'il dit la `turn`-ième fois qu'on lui parle : d'abord son bonjour, puis, à chaque fois,
    /// une autre réplique de son métier (`lines`, celles de sa fiche), et l'on y revient en boucle
    pub fn line(&self, turn: usize, lines: &[String]) -> String {
        let others: Vec<&String> = lines.iter().filter(|l| **l != self.greeting).collect();
        if turn == 0 || others.is_empty() {
            return self.greeting.clone();
        }
        // Chacun commence ailleurs dans la liste, pour que deux voisins ne se répètent pas
        let start = self.name.bytes().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
        others[(start + turn - 1) % others.len()].clone()
    }
}

impl ProfessionRegistry {
    /// Répliques du métier d'un habitant, d'après le nom de métier de sa fiche (au masculin ou au
    /// féminin ; vide : sans métier)
    pub fn lines_of(&self, card: &VillagerCard) -> &[String] {
        self.professions
            .iter()
            .find(|p| p.name == card.profession || p.name_feminine == card.profession)
            .map_or(&[], |p| &p.lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_professions_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PROFESSIONS_DIR);
        let (registry, errors) = ProfessionRegistry::load_dir(&dir, &find_item).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        for id in ["forgeron", "marchand", "fermier", "garde", "aubergiste", "pretre", "sansMetier", "colporteur", "pelerin", "aventurier"] {
            assert!(registry.find(id).is_some(), "{id} manquant");
        }
        let garde = registry.get(registry.find("garde").unwrap()).unwrap();
        assert_eq!((garde.workplace, garde.species.as_str()), (Workplace::Rempart, "garde"));
        let forgeron = registry.get(registry.find("forgeron").unwrap()).unwrap();
        assert!(!forgeron.trades.is_empty());
        assert!(registry.iter().filter(|p| p.traveller).count() >= 3);
        // Chaque tenue citée existe dans l'atlas des héros
        let atlas = crate::hero::atlas();
        for p in registry.iter() {
            for t in &p.outfits {
                assert!(atlas.tenue(t).is_some(), "{} : tenue {t} inconnue", p.id);
            }
        }
    }

    #[test]
    fn a_villager_greets_then_says_the_other_lines_of_his_trade() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(PROFESSIONS_DIR);
        let (registry, _) = ProfessionRegistry::load_dir(&dir, &find_item).unwrap();
        let idle = registry.get(registry.find("sansMetier").unwrap()).unwrap();
        let card = VillagerCard {
            name: "Aude".into(),
            profession: String::new(),
            culture: "Humains".into(),
            origin: "Habitante de Brumeval".into(),
            appearance: crate::hero::atlas().defaut("humain"),
            trades: Vec::new(),
            greeting: idle.lines[0].clone(),
        };
        assert!(card.chats_only());
        assert_eq!(card.speaker(), "Aude");
        let lines = registry.lines_of(&card);
        assert_eq!(lines, idle.lines.as_slice());
        assert_eq!(card.line(0, lines), card.greeting);
        let said: Vec<String> = (1..idle.lines.len()).map(|t| card.line(t, lines)).collect();
        // Chaque autre réplique une fois, sans redire le bonjour
        assert!(said.iter().all(|l| *l != card.greeting));
        let mut unique = said.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), idle.lines.len() - 1, "{said:?}");
        let guard = VillagerCard { name: "Bertrand".into(), profession: "Garde".into(), greeting: "Circulez.".into(), ..card };
        assert_eq!(guard.speaker(), "Bertrand (Garde)");
        assert!(registry.lines_of(&guard).iter().any(|l| l == "Rien à signaler."));
    }

    #[test]
    fn invalid_professions_are_explained() {
        let find = |id: &str| (id == "ble").then_some(ItemId(20000)).or((id == "pain").then_some(ItemId(20001)));
        let ok = r#"{ "name": "Boulanger", "lieu": "etal", "echanges": [ { "donne": { "ble": 6 }, "recoit": { "pain": 1 } } ] }"#;
        let p = Profession::parse("boulanger", ok, &find).unwrap();
        assert_eq!(p.workplace, Workplace::Etal);
        assert_eq!(p.species, "villageois");
        assert_eq!(p.trades, vec![Trade { give: vec![(ItemId(20000), 6)], get: (ItemId(20001), 1) }]);
        assert!(Profession::parse("x", &ok.replace("pain", "brioche"), &find).unwrap_err().contains("brioche"));
        assert!(Profession::parse("x", &ok.replace("etal", "moulin"), &find).is_err());
        assert!(Profession::parse("x", &ok.replace(": 6", ": 0"), &find).is_err());
        assert!(Profession::parse("x", &ok.replace("\"lieu\"", "\"couleurs\": [\"rouge\"], \"lieu\""), &find).is_err());
    }
}

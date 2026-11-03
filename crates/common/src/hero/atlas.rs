//! L'atlas du héros : le patron 64 × 64 à deux couches, toutes les options d'apparence, les
//! espèces et leurs variantes (sous-races), et les règles qui fondent une variante dans son espèce.
//!
//! Les données sont celles de la maquette de référence (`assets/heros/atlas.json`, tirée de
//! `tools/heros/maquette/hero-atlas.js` sur la branche `develop`) : les noms des champs restent
//! ceux du fichier, en français, pour qu'une ligne de l'un se retrouve dans l'autre.
//!
//! Une variante pose ses traits et ses proportions sur ceux de son espèce comme le fait
//! `Object.assign` : un champ qu'elle nomme remplace celui de l'espèce, un champ qu'elle tait le
//! laisse.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::Appearance;

/// Le fichier de l'atlas, embarqué : le serveur et les tests le lisent sans dossier d'assets
const ATLAS_JSON: &str = include_str!("../../../../assets/heros/atlas.json");

/// Rectangle de la planche, en texels : une face d'une pièce du patron
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub largeur: i32,
    pub hauteur: i32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Boite {
    pub largeur: i32,
    pub hauteur: i32,
    pub profondeur: i32,
}

/// Une boîte du patron : l'os qu'elle suit, sa couche, sa taille en texels, son gonflement
/// (couche 2) et la place de chacune de ses six faces sur la planche
#[derive(Debug, Clone, Deserialize)]
pub struct Partie {
    pub os: String,
    pub couche: u8,
    pub boite: Boite,
    pub gonflement: f32,
    pub faces: HashMap<String, Rect>,
}

impl Partie {
    /// Rectangle d'une face : dessus, dessous, droite, devant, gauche ou derriere
    pub fn face(&self, face: &str) -> Rect {
        self.faces[face]
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Patron {
    pub unite_en_blocs: f64,
    pub parties: HashMap<String, Partie>,
}

/// Rectangle de couleur sur une face, nommé par une clé de teinte : yeux `b i l c` (blanc, iris,
/// paupière, cil), bouches `l f d p` (lèvre, creux, dent, peau ombrée), tenues `fer clair sombre
/// base cuir peau`
#[derive(Debug, Clone, Deserialize)]
pub struct Touche(pub i32, pub i32, pub i32, pub i32, pub String);

/// Rectangle posé sur une pièce nommée : pour les marques `tete teteD teteG torse bras`, pour les
/// extras d'une coiffure une face de la tête de couche 2
#[derive(Debug, Clone, Deserialize)]
pub struct Pose(pub String, pub i32, pub i32, pub i32, pub i32);

/// Une option simple de la forge : son identifiant et son nom affiché
pub trait Choix {
    fn id(&self) -> &str;
    fn nom(&self) -> &str;
}

macro_rules! choix {
    ($($t:ty),*) => {$(
        impl Choix for $t {
            fn id(&self) -> &str { &self.id }
            fn nom(&self) -> &str { &self.nom }
        }
    )*};
}

/// Coiffure. Les trois profils donnent, colonne par colonne, combien de texels de cheveux
/// tombent devant, sur les côtés (de l'avant vers l'arrière) et derrière la tête de couche 2.
/// `chute` nomme la mèche pendante, s'il y en a une.
#[derive(Debug, Clone, Deserialize)]
pub struct Coiffure {
    pub id: String,
    pub nom: String,
    pub dev: Option<Vec<i32>>,
    pub cot: Option<Vec<i32>>,
    pub der: Option<Vec<i32>>,
    pub chute: Option<String>,
    #[serde(default)]
    pub extras: Vec<Pose>,
}

/// Forme d'yeux ou de bouche : des rectangles sur le visage (les yeux sur la demi-face, en miroir)
#[derive(Debug, Clone, Deserialize)]
pub struct Expression {
    pub id: String,
    pub nom: String,
    #[serde(default)]
    pub ops: Vec<Touche>,
}

/// Marque corporelle : `teinte` est une couleur, ou `peauF` pour une ombre de la peau
#[derive(Debug, Clone, Deserialize)]
pub struct Marque {
    pub id: String,
    pub nom: String,
    pub teinte: Option<String>,
    #[serde(default)]
    pub ops: Vec<Pose>,
}

/// Barbe : pour chacun de ses huit rangs, les bandes `[x, largeur]` qu'elle couvre
#[derive(Debug, Clone, Deserialize)]
pub struct Barbe {
    pub id: String,
    pub nom: String,
    pub rangs: Option<Vec<Vec<[i32; 2]>>>,
}

/// Tenue. `ton` choisit la couleur du corps dans l'étoffe (base, sombre, clair, plaque) ;
/// `manche` est le nombre des douze rangs du bras que la manche couvre ; `jambes` vaut `pantalon`
/// ou `robe`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tenue {
    pub id: String,
    pub nom: String,
    pub ton: String,
    pub manche: i32,
    pub jambes: String,
    #[serde(default)]
    pub col: bool,
    #[serde(default)]
    pub ceinture: bool,
    #[serde(default)]
    pub epaulettes: bool,
    #[serde(default)]
    pub torse_nu: bool,
    #[serde(default)]
    pub extras: Vec<Touche>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormeTete {
    pub id: String,
    pub nom: String,
}

choix!(Coiffure, Expression, Marque, Barbe, Tenue, FormeTete);

/// Proportions d'une espèce ou d'une variante ; chaque champ est facultatif
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Mods {
    pub torse: Option<f64>,
    pub jambes: Option<f64>,
    pub bras: Option<f64>,
    pub tete: Option<f64>,
    pub voute: Option<i32>,
    pub flotte: Option<bool>,
    pub ailes: Option<bool>,
    pub pointes: Option<bool>,
}

macro_rules! fusion {
    ($base:expr, $over:expr, $($f:ident),*) => {{
        let mut m = $base.clone();
        if let Some(o) = $over {
            $(if o.$f.is_some() { m.$f = o.$f.clone(); })*
        }
        m
    }};
}

impl Mods {
    /// Ces proportions avec celles de `over` posées par-dessus
    pub fn fusion(&self, over: Option<&Mods>) -> Mods {
        fusion!(self, over, torse, jambes, bras, tete, voute, flotte, ailes, pointes)
    }
}

/// Traits d'une espèce ou d'une variante, comme la table libre de la maquette : chaque champ est
/// facultatif, pour qu'une variante ne remplace que ce qu'elle nomme (un `false` explicite
/// remplace). Le peintre les lit avec la « vérité » de JavaScript : absent, zéro et faux valent non.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Traits {
    /// Profondeur des ombres du visage
    pub relief: Option<f64>,
    /// Opacité de la couche 1
    pub alpha: Option<f64>,
    /// Pelage : peau plus granuleuse, touffes sur la tête
    pub pelage: Option<f64>,
    /// Longueur des oreilles pointues, en texels
    pub oreilles: Option<i32>,
    /// Rangées de canines inférieures visibles (1 ou 2)
    pub canines: Option<i32>,
    /// Veines sombres : 1 éparses, 2 denses
    pub veines: Option<i32>,
    /// Plus petit indice de barbe que l'espèce peut choisir
    pub barbe_min: Option<i32>,
    pub motif: Option<String>,
    pub accent: Option<String>,
    pub reflet: Option<String>,
    pub lueur: Option<String>,
    /// Regard qui remplace la forme des yeux : lumineux ou vitreux
    pub regard: Option<String>,
    /// Appendice crânien : oreilles, cornes, bois, couronne
    pub appendice: Option<String>,
    pub queue: Option<String>,
    pub oreilles_animales: Option<String>,
    pub coiffure_forcee: Option<String>,
    pub barbe_forcee: Option<String>,
    pub yeux_forces: Option<String>,
    pub forme_tete_forcee: Option<String>,
    pub bouche_forcee: Option<String>,
    pub mains_massives: Option<bool>,
    pub machoire: Option<bool>,
    pub machoire_lourde: Option<bool>,
    pub craquelee: Option<bool>,
    pub chevelure_etheree: Option<bool>,
    pub halo: Option<bool>,
    pub griffes: Option<bool>,
}

impl Traits {
    /// Ces traits avec ceux de `over` posés par-dessus
    pub fn fusion(&self, over: Option<&Traits>) -> Traits {
        fusion!(
            self, over, relief, alpha, pelage, oreilles, canines, veines, barbe_min, motif, accent, reflet, lueur,
            regard, appendice, queue, oreilles_animales, coiffure_forcee, barbe_forcee, yeux_forces,
            forme_tete_forcee, bouche_forcee, mains_massives, machoire, machoire_lourde, craquelee,
            chevelure_etheree, halo, griffes
        )
    }
}

/// Une variante (sous-race) : sa palette de peau (absente : celle de l'espèce), ses proportions et
/// ses traits posés sur ceux de l'espèce
#[derive(Debug, Clone, Deserialize)]
pub struct Variante {
    pub nom: String,
    pub apparence: Option<String>,
    pub peaux: Option<Vec<String>>,
    pub mods: Option<Mods>,
    pub traits: Option<Traits>,
}

/// Une espèce jouable : tailles en mètres, palette de peau, proportions et traits de base, et ses
/// variantes
#[derive(Debug, Clone, Deserialize)]
pub struct Espece {
    pub id: String,
    pub nom: String,
    pub taille: f64,
    pub min: f64,
    pub max: f64,
    pub apparence: String,
    pub attribut: String,
    pub peaux: Vec<String>,
    #[serde(default)]
    pub traits: Traits,
    #[serde(default)]
    pub mods: Mods,
    pub variantes: Vec<Variante>,
}

/// L'atlas complet, tel que lu dans `atlas.json`
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Atlas {
    pub PATRON: Patron,
    pub FACES: Vec<String>,
    pub COIFFURES: Vec<Coiffure>,
    pub YEUX: Vec<Expression>,
    pub BOUCHES: Vec<Expression>,
    pub MARQUES: Vec<Marque>,
    pub BARBES: Vec<Barbe>,
    pub TENUES: Vec<Tenue>,
    pub FORMES_TETE: Vec<FormeTete>,
    pub PAL_CHEVEUX: Vec<String>,
    pub PAL_YEUX: Vec<String>,
    pub PAL_TENUE: Vec<String>,
    pub ESPECES: HashMap<String, Espece>,
    /// Espèces dans l'ordre où la forge les présente
    pub ORDRE_ESPECES: Vec<String>,
}

/// L'atlas, lu une fois
pub fn atlas() -> &'static Atlas {
    static ATLAS: OnceLock<Atlas> = OnceLock::new();
    ATLAS.get_or_init(|| serde_json::from_str(ATLAS_JSON).expect("assets/heros/atlas.json illisible"))
}

fn trouver<'a, T: Choix>(liste: &'a [T], id: &str) -> Option<&'a T> {
    liste.iter().find(|o| o.id() == id)
}

fn present(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

impl Atlas {
    /// L'espèce de cet identifiant, ou l'humain s'il est inconnu
    pub fn espece(&self, id: &str) -> &Espece {
        self.ESPECES.get(id).unwrap_or_else(|| &self.ESPECES["humain"])
    }

    /// Les espèces dans l'ordre de la forge
    pub fn especes(&self) -> impl Iterator<Item = &Espece> {
        self.ORDRE_ESPECES.iter().map(|id| self.espece(id))
    }

    pub fn partie(&self, id: &str) -> &Partie {
        &self.PATRON.parties[id]
    }

    pub fn coiffure(&self, id: &str) -> Option<&Coiffure> {
        trouver(&self.COIFFURES, id)
    }

    pub fn yeux(&self, id: &str) -> Option<&Expression> {
        trouver(&self.YEUX, id)
    }

    pub fn bouche(&self, id: &str) -> Option<&Expression> {
        trouver(&self.BOUCHES, id)
    }

    pub fn marque(&self, id: &str) -> Option<&Marque> {
        trouver(&self.MARQUES, id)
    }

    pub fn barbe(&self, id: &str) -> Option<&Barbe> {
        trouver(&self.BARBES, id)
    }

    pub fn tenue(&self, id: &str) -> Option<&Tenue> {
        trouver(&self.TENUES, id)
    }

    /// La variante que nomme l'apparence, ramenée à la dernière ; la première si hors bornes
    pub fn variante_de(&self, cfg: &Appearance) -> Option<&Variante> {
        let variantes = &self.espece(&cfg.espece).variantes;
        if variantes.is_empty() {
            return None;
        }
        let index = (cfg.variante.max(0) as usize).min(variantes.len() - 1);
        variantes.get(index)
    }

    /// Palette de peau : celle de la variante, sinon celle de l'espèce
    pub fn peaux_de(&self, cfg: &Appearance) -> &[String] {
        match self.variante_de(cfg).and_then(|v| v.peaux.as_ref()) {
            Some(peaux) => peaux,
            None => &self.espece(&cfg.espece).peaux,
        }
    }

    /// Proportions de l'espèce, variante posée par-dessus
    pub fn mods_de(&self, cfg: &Appearance) -> Mods {
        self.espece(&cfg.espece).mods.fusion(self.variante_de(cfg).and_then(|v| v.mods.as_ref()))
    }

    /// Traits de l'espèce, variante posée par-dessus, relief et alpha valant 1 par défaut
    pub fn traits_de(&self, cfg: &Appearance) -> Traits {
        Traits { relief: Some(1.0), alpha: Some(1.0), ..Traits::default() }
            .fusion(Some(&self.espece(&cfg.espece).traits))
            .fusion(self.variante_de(cfg).and_then(|v| v.traits.as_ref()))
    }

    /// L'apparence par défaut d'une espèce : sa première variante, avec ce qu'elle impose
    pub fn defaut(&self, espece_id: &str) -> Appearance {
        let sp = self.espece(espece_id);
        let premiere = sp.variantes.first();
        let t = sp.traits.fusion(premiere.and_then(|v| v.traits.as_ref()));
        let pal = premiere.and_then(|v| v.peaux.as_ref()).unwrap_or(&sp.peaux);
        Appearance {
            nom: "Sans nom".to_string(),
            espece: sp.id.clone(),
            variante: 0,
            taille: sp.taille,
            corpulence: 0.5,
            peau: pal.get(1).unwrap_or(&pal[0]).clone(),
            forme_tete: present(&t.forme_tete_forcee).unwrap_or("carree").to_string(),
            yeux: present(&t.yeux_forces).unwrap_or("amande").to_string(),
            couleur_yeux: self.PAL_YEUX[0].clone(),
            bouche: present(&t.bouche_forcee).unwrap_or("neutre").to_string(),
            marques: "aucune".to_string(),
            coiffure: present(&t.coiffure_forcee).unwrap_or("courte").to_string(),
            couleur_cheveux: self.PAL_CHEVEUX[1].clone(),
            barbe: present(&t.barbe_forcee).unwrap_or("aucune").to_string(),
            tenue: "tunique".to_string(),
            couleur_tenue: self.PAL_TENUE[0].clone(),
        }
    }

    /// Une apparence au hasard. Tire dans l'ordre de la maquette, un nombre de `[0, 1)` par
    /// tirage, et saute les tirages qu'un trait imposé rend inutiles.
    ///
    /// `espece_id` : l'espèce, ou `None` pour en tirer une
    pub fn aleatoire(&self, rng: &mut impl FnMut() -> f64, espece_id: Option<&str>) -> Appearance {
        fn pick<'a, T>(rng: &mut impl FnMut() -> f64, liste: &'a [T]) -> &'a T {
            &liste[(rng() * liste.len() as f64).floor() as usize]
        }
        let id = match espece_id.filter(|s| !s.is_empty()) {
            Some(id) => id.to_string(),
            None => pick(rng, &self.ORDRE_ESPECES).clone(),
        };
        let sp = self.espece(&id);
        let mut c = self.defaut(&id);
        c.variante = (rng() * sp.variantes.len() as f64).floor() as i32;
        let t = self.traits_de(&c);
        c.taille = java_round((sp.min + rng() * (sp.max - sp.min)) * 100.0) / 100.0;
        c.corpulence = java_round(rng() * 100.0) / 100.0;
        c.peau = pick(rng, self.peaux_de(&c)).clone();
        c.forme_tete = match present(&t.forme_tete_forcee) {
            Some(v) => v.to_string(),
            None => pick(rng, &self.FORMES_TETE).id.clone(),
        };
        c.yeux = match present(&t.yeux_forces) {
            Some(v) => v.to_string(),
            None => pick(rng, &self.YEUX).id.clone(),
        };
        c.couleur_yeux = pick(rng, &self.PAL_YEUX).clone();
        c.bouche = match present(&t.bouche_forcee) {
            Some(v) => v.to_string(),
            None => pick(rng, &self.BOUCHES).id.clone(),
        };
        c.marques = pick(rng, &self.MARQUES).id.clone();
        c.coiffure = match present(&t.coiffure_forcee) {
            Some(v) => v.to_string(),
            None => pick(rng, &self.COIFFURES).id.clone(),
        };
        c.couleur_cheveux = pick(rng, &self.PAL_CHEVEUX).clone();
        c.barbe = match present(&t.barbe_forcee) {
            Some(v) => v.to_string(),
            None => pick(rng, &self.BARBES).id.clone(),
        };
        c.tenue = pick(rng, &self.TENUES).id.clone();
        c.couleur_tenue = pick(rng, &self.PAL_TENUE).clone();
        c
    }

    /// Hauteur du corps en texels : tête, torse et jambes, aux proportions de l'espèce
    pub fn hauteur_texels(&self, cfg: &Appearance) -> f64 {
        let m = self.mods_de(cfg);
        let non_nul = |v: Option<f64>| v.filter(|v| *v != 0.0).unwrap_or(1.0);
        8.0 * non_nul(m.tete) + 12.0 + 12.0 * non_nul(m.jambes)
    }
}

/// `Math.round` de Java et de JavaScript : l'entier le plus proche, le plus grand à égalité
pub(crate) fn java_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

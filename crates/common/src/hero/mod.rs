//! Le créateur de légende : qui est un héros. Le corps choisi à la forge (espèce, variante,
//! taille, corpulence, visage, pilosité, tenue), la voie prise au râtelier, et le nom donné au
//! baptême. Repris du module `HeroClasses` (paquet `legende`) de la branche `develop`.
//!
//! **Scellé une fois.** Un héros arrive dans un monde sans légende, et le créateur s'ouvre ; le
//! baptême fixe tout d'un coup et pour de bon.
//!
//! L'apparence se garde champ par champ plutôt qu'en planche peinte : chacun la repeint
//! ([`paint`]), si bien qu'une planche ne voyage jamais et qu'un héros vu par deux joueurs est le
//! même par construction.

pub mod atlas;
pub mod classes;
pub mod painter;

use serde::{Deserialize, Serialize};

use crate::character::PlayerClass;
pub use atlas::{atlas, Atlas, Espece, Variante};
pub use classes::{class_sheet, ClassSheet, Family, CLASSES};
pub use painter::{paint, Sheet, SHEET_SIZE};

/// Le plus long nom qu'un héros peut porter
pub const NAME_MAX: usize = 32;

/// À quoi ressemble un héros : la configuration que le peintre change en planche, champ pour champ
/// l'objet `cfg` de la maquette. Les options sont des identifiants de l'[`Atlas`], les couleurs des
/// chaînes `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub nom: String,
    pub espece: String,
    pub variante: i32,
    /// Taille en mètres
    pub taille: f64,
    /// Carrure, de 0 (frêle) à 1 (massive)
    pub corpulence: f64,
    pub peau: String,
    pub forme_tete: String,
    pub yeux: String,
    pub couleur_yeux: String,
    pub bouche: String,
    pub marques: String,
    pub coiffure: String,
    pub couleur_cheveux: String,
    pub barbe: String,
    pub tenue: String,
    pub couleur_tenue: String,
}

impl Default for Appearance {
    fn default() -> Self {
        atlas().defaut("humain")
    }
}

impl Appearance {
    pub fn species(&self) -> &'static Espece {
        atlas().espece(&self.espece)
    }

    pub fn variant(&self) -> Option<&'static Variante> {
        atlas().variante_de(self)
    }

    /// Nom de la carrure
    pub fn build_label(&self) -> &'static str {
        if self.corpulence < 0.3 {
            "Frêle"
        } else if self.corpulence > 0.7 {
            "Massive"
        } else {
            "Moyenne"
        }
    }

    /// Élargissement du corps dû à la carrure, en travers et en profondeur
    pub fn width_scale(&self) -> f32 {
        0.92 + 0.16 * self.corpulence as f32
    }

    /// Ramène l'apparence dans ce que permettent son espèce et sa variante.
    ///
    /// `souche` : l'espèce ou la variante vient de changer. Alors seulement les yeux, la tête et la
    /// bouche imposés remplacent le choix du joueur, qui peut les changer ensuite.
    pub fn retouch(&mut self, souche: bool) {
        let a = atlas();
        let traits = a.traits_de(self);
        if let Some(c) = &traits.coiffure_forcee {
            self.coiffure = c.clone();
        }
        if let Some(b) = &traits.barbe_forcee {
            self.barbe = b.clone();
        }
        if souche {
            if let Some(y) = &traits.yeux_forces {
                self.yeux = y.clone();
            }
            if let Some(f) = &traits.forme_tete_forcee {
                self.forme_tete = f.clone();
            }
            if let Some(b) = &traits.bouche_forcee {
                self.bouche = b.clone();
            }
        }
        let peaux = a.peaux_de(self);
        if !peaux.contains(&self.peau) {
            self.peau = peaux.get(1).unwrap_or(&peaux[0]).clone();
        }
        let sp = a.espece(&self.espece);
        self.taille = self.taille.clamp(sp.min, sp.max);
    }

    /// Ce que le baptême accepte : chaque option doit être une de celles que la forge propose.
    /// Ce qu'une variante impose (la barbe d'un nain, une coiffure) est imposé de nouveau ; ce
    /// qu'elle suggère (yeux, bouche, tête) reste au joueur.
    ///
    /// Renvoie la raison du refus, en français pour le joueur ; acceptée, l'apparence est
    /// normalisée sur place (nom rogné, traits imposés appliqués).
    pub fn verify(&mut self) -> Result<(), LegendError> {
        let a = atlas();
        self.nom = self.nom.trim().to_string();
        if self.nom.is_empty() {
            return Err(LegendError::NoName);
        }
        if self.nom.chars().count() > NAME_MAX || self.nom.chars().any(char::is_control) {
            return Err(LegendError::BadName);
        }
        let Some(sp) = a.ESPECES.get(&self.espece) else { return Err(LegendError::UnknownSpecies) };
        if self.variante < 0 || self.variante as usize >= sp.variantes.len() {
            return Err(LegendError::UnknownVariant);
        }
        if !(self.taille >= sp.min - 1e-6 && self.taille <= sp.max + 1e-6) {
            return Err(LegendError::Height);
        }
        if !(0.0..=1.0).contains(&self.corpulence) {
            return Err(LegendError::Build);
        }
        let traits = a.traits_de(self);
        if let Some(c) = traits.coiffure_forcee.filter(|c| !c.is_empty()) {
            self.coiffure = c;
        }
        if let Some(b) = traits.barbe_forcee.filter(|b| !b.is_empty()) {
            self.barbe = b;
        }
        fn dans(valeur: &str, permis: impl IntoIterator<Item = impl AsRef<str>>, quoi: &'static str) -> Result<(), LegendError> {
            if permis.into_iter().any(|p| p.as_ref() == valeur) {
                Ok(())
            } else {
                Err(LegendError::UnknownChoice(quoi))
            }
        }
        use atlas::Choix;
        dans(&self.peau, a.peaux_de(self), "couleur de peau")?;
        dans(&self.forme_tete, a.FORMES_TETE.iter().map(Choix::id), "forme de tête")?;
        dans(&self.yeux, a.YEUX.iter().map(Choix::id), "yeux")?;
        dans(&self.couleur_yeux, &a.PAL_YEUX, "couleur des yeux")?;
        dans(&self.bouche, a.BOUCHES.iter().map(Choix::id), "bouche")?;
        dans(&self.marques, a.MARQUES.iter().map(Choix::id), "marques")?;
        dans(&self.coiffure, a.COIFFURES.iter().map(Choix::id), "coiffure")?;
        dans(&self.couleur_cheveux, &a.PAL_CHEVEUX, "couleur des cheveux")?;
        dans(&self.barbe, a.BARBES.iter().map(Choix::id), "barbe")?;
        dans(&self.tenue, a.TENUES.iter().map(Choix::id), "tenue")?;
        dans(&self.couleur_tenue, &a.PAL_TENUE, "couleur de tenue")
    }

    /// Ce qui se voit sur la planche : deux apparences de même clé ont la même planche
    pub fn sheet_key(&self) -> String {
        [
            &self.espece,
            &self.variante.to_string(),
            &self.peau,
            &self.forme_tete,
            &self.yeux,
            &self.couleur_yeux,
            &self.bouche,
            &self.marques,
            &self.coiffure,
            &self.couleur_cheveux,
            &self.barbe,
            &self.tenue,
            &self.couleur_tenue,
        ]
        .map(|s| s.as_str())
        .join("|")
    }
}

/// Pourquoi le baptême refuse une légende, en français pour le joueur
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LegendError {
    #[error("le héros n'a pas de nom")]
    NoName,
    #[error("ce nom ne peut pas se porter")]
    BadName,
    #[error("espèce inconnue")]
    UnknownSpecies,
    #[error("variante inconnue")]
    UnknownVariant,
    #[error("taille hors de l'espèce")]
    Height,
    #[error("corpulence hors d'échelle")]
    Build,
    /// Option absente de la forge (le nom du champ en paramètre)
    #[error("choix inconnu : {0}")]
    UnknownChoice(&'static str),
    #[error("voie inconnue")]
    UnknownClass,
}

/// La légende d'un héros baptisé : son apparence (nom compris) et sa voie
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Legend {
    pub appearance: Appearance,
    pub class: PlayerClass,
}

impl Legend {
    /// Vérifie toute la légende (voir [`Appearance::verify`]) et une voie du râtelier. Acceptée,
    /// elle est normalisée sur place.
    pub fn verify(&mut self) -> Result<(), LegendError> {
        self.appearance.verify()?;
        if class_sheet(self.class).is_none() {
            return Err(LegendError::UnknownClass);
        }
        Ok(())
    }
}

const SYL1: [&str; 14] = ["Bran", "Kae", "Tor", "El", "Var", "Mor", "Sig", "Dru", "Yl", "Gor", "Ash", "Nym", "Rhu", "Cal"];
const SYL2: [&str; 12] = ["din", "wen", "gar", "mir", "ska", "thal", "ven", "run", "dra", "lin", "vok", "nara"];
const TITRES: [&str; 10] = [
    "de Valbrume",
    "du Chêne fendu",
    "des Marches basses",
    "aux Deux Clous",
    "de Bas-Fort",
    "la Dernière",
    "le Rouge",
    "sans Terre",
    "de la Forge froide",
    "aux Corbeaux",
];

/// Un nom comme les écrivent les greffiers de garnison, avec un surnom un peu plus d'une fois sur
/// deux
pub fn random_name(rng: &mut impl FnMut() -> f64) -> String {
    fn pick<'a>(rng: &mut impl FnMut() -> f64, liste: &[&'a str]) -> &'a str {
        liste[((rng() * liste.len() as f64) as usize).min(liste.len() - 1)]
    }
    let nom = format!("{}{}", pick(rng, &SYL1), pick(rng, &SYL2));
    if rng() < 0.55 {
        format!("{nom} {}", pick(rng, &TITRES))
    } else {
        nom
    }
}

/// Un petit générateur de hasard (xorshift), semé par l'horloge : la forge n'a besoin que de dés
#[derive(Debug, Clone)]
pub struct Dice(u64);

impl Dice {
    pub fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        Self(nanos | 1)
    }

    /// Un nombre de `[0, 1)`
    pub fn roll(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legende() -> Legend {
        let mut a = atlas().defaut("nain");
        a.nom = "  Brandin de Valbrume ".into();
        Legend { appearance: a, class: PlayerClass::Soldier }
    }

    #[test]
    fn baptism_accepts_a_forged_hero_and_trims_the_name() {
        let mut l = legende();
        assert_eq!(l.verify(), Ok(()));
        assert_eq!(l.appearance.nom, "Brandin de Valbrume");
    }

    #[test]
    fn baptism_refuses_what_the_forge_does_not_offer() {
        let mut l = legende();
        l.appearance.nom = "   ".into();
        assert!(l.verify().is_err());
        let mut l = legende();
        l.appearance.taille = 1.9;
        assert_eq!(l.verify(), Err(LegendError::Height));
        let mut l = legende();
        l.appearance.peau = "#123456".into();
        assert_eq!(l.verify(), Err(LegendError::UnknownChoice("couleur de peau")));
        let mut l = legende();
        l.class = PlayerClass::Adventurer;
        assert_eq!(l.verify(), Err(LegendError::UnknownClass));
    }

    #[test]
    fn baptism_imposes_the_variant_beard() {
        // Le forgeron des profondeurs porte toujours la barbe pleine
        let mut l = legende();
        l.appearance.barbe = "aucune".into();
        l.verify().unwrap();
        assert_eq!(l.appearance.barbe, "pleine");
    }

    #[test]
    fn retouch_keeps_height_and_skin_within_the_species() {
        let mut a = atlas().defaut("gnome");
        a.taille = 2.5;
        a.peau = "#000000".into();
        a.retouch(false);
        let sp = a.species();
        assert_eq!(a.taille, sp.max);
        assert!(atlas().peaux_de(&a).contains(&a.peau));
    }

    #[test]
    fn random_names_and_heroes_are_valid() {
        let mut dice = Dice(42);
        for _ in 0..200 {
            let mut a = atlas().aleatoire(&mut || dice.roll(), None);
            a.nom = random_name(&mut || dice.roll());
            assert_eq!(a.verify(), Ok(()), "{a:?}");
        }
    }

    #[test]
    fn legend_survives_json_and_bincode() {
        let l = legende();
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"formeTete\""));
        assert_eq!(serde_json::from_str::<Legend>(&json).unwrap(), l);
        let bin = bincode::serialize(&l).unwrap();
        assert_eq!(bincode::deserialize::<Legend>(&bin).unwrap(), l);
    }
}

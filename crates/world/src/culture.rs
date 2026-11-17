//! Cultures des villes et villages : une par race jouable du créateur de personnage. Les Humains,
//! les Nains, les Elfes des bois et les Hauts Elfes bâtissent des villes et des villages ; les
//! Demi-orques, les Revenants, les Sylphes, les Gnomes, les Aasimars et les Hommes-bêtes, des
//! villages seulement. Chaque race a au moins un village sur la planète (`structures`).
//!
//! Chaque ville ou village appartient à une culture, tirée de la graine du monde selon son climat
//! et le « royaume » où il se trouve : de grands pans de planète (trois régions sur trois) penchent
//! vers une même culture, si bien que des établissements voisins se ressemblent souvent sans être
//! tous pareils. Les Nains aiment les montagnes et le froid, les Elfes des bois les forêts et les
//! jungles, les Hauts Elfes les plaines douces et les côtes, les Humains sont partout chez eux. Les
//! Demi-orques tiennent la savane, le désert et la jungle ; les Revenants les plages et la toundra ;
//! les Sylphes les montagnes ; les Gnomes les plaines et les forêts ; les Aasimars les plateaux et la
//! savane ; les Hommes-bêtes la savane, la jungle et la taïga.
//!
//! La culture choisit le style des bâtiments, des remparts et des tours (`structures`), et le
//! peuple : la plupart des habitants en sont, quelques-uns viennent d'ailleurs
//! (`Settlement::resident_culture`). Chaque culture est une espèce de l'atlas des héros
//! (`espece_id`), qui donne aux habitants leur corps et leur visage.

use serde::{Deserialize, Serialize};

use crate::generator::BiomeType;

/// Une culture majeure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Culture {
    Humains,
    Nains,
    ElfesDesBois,
    HautsElfes,
    DemiOrques,
    Revenants,
    Sylphes,
    Gnomes,
    Aasimars,
    HommesBetes,
}

/// Part des habitants d'une ville qui sont de sa culture ; les autres viennent d'ailleurs
pub const NATIVE_SHARE: f64 = 0.85;

impl Culture {
    pub const ALL: [Culture; 10] = [
        Culture::Humains,
        Culture::Nains,
        Culture::ElfesDesBois,
        Culture::HautsElfes,
        Culture::DemiOrques,
        Culture::Revenants,
        Culture::Sylphes,
        Culture::Gnomes,
        Culture::Aasimars,
        Culture::HommesBetes,
    ];

    /// Les cultures qui bâtissent des villes ; les autres n'ont que des villages
    pub const TOWN_BUILDERS: [Culture; 4] = [Culture::Humains, Culture::Nains, Culture::ElfesDesBois, Culture::HautsElfes];

    /// La culture bâtit des villes fortifiées, et pas seulement des villages
    pub fn builds_towns(self) -> bool {
        Self::TOWN_BUILDERS.contains(&self)
    }

    /// Nom du peuple, au pluriel
    pub fn name(self) -> &'static str {
        match self {
            Culture::Humains => "Humains",
            Culture::Nains => "Nains",
            Culture::ElfesDesBois => "Elfes des bois",
            Culture::HautsElfes => "Hauts Elfes",
            Culture::DemiOrques => "Demi-orques",
            Culture::Revenants => "Revenants",
            Culture::Sylphes => "Sylphes",
            Culture::Gnomes => "Gnomes",
            Culture::Aasimars => "Aasimars",
            Culture::HommesBetes => "Hommes-bêtes",
        }
    }

    /// Adjectif d'une ville de cette culture (« une ville naine »)
    pub fn adjective(self, feminine: bool) -> &'static str {
        match (self, feminine) {
            (Culture::Humains, _) => "humaine",
            (Culture::Nains, true) => "naine",
            (Culture::Nains, false) => "nain",
            (Culture::ElfesDesBois, _) => "sylvestre",
            (Culture::HautsElfes, _) => "elfique",
            (Culture::DemiOrques, _) => "orque",
            (Culture::Revenants, _) => "revenante",
            (Culture::Sylphes, _) => "sylphide",
            (Culture::Gnomes, _) => "gnome",
            (Culture::Aasimars, _) => "aasimar",
            (Culture::HommesBetes, _) => "des hommes-bêtes",
        }
    }

    /// Espèce de l'atlas des héros (`herocraft_common::hero::atlas`) des gens de cette culture
    pub fn espece_id(self) -> &'static str {
        match self {
            Culture::Humains => "humain",
            Culture::Nains => "nain",
            Culture::ElfesDesBois => "elfeBois",
            Culture::HautsElfes => "hautElfe",
            Culture::DemiOrques => "demiOrque",
            Culture::Revenants => "revenant",
            Culture::Sylphes => "sylphe",
            Culture::Gnomes => "gnome",
            Culture::Aasimars => "aasimar",
            Culture::HommesBetes => "hommeBete",
        }
    }

    /// Goût de la culture pour un climat : poids de tirage d'une ville de ce biome
    pub fn affinity(self, biome: BiomeType) -> f64 {
        use BiomeType::*;
        match self {
            Culture::Humains => match biome {
                Plains | Savanna | Desert | Beach => 3.0,
                Forest | Taiga | Jungle | Tundra | Mountains => 1.0,
                _ => 0.5,
            },
            Culture::Nains => match biome {
                Mountains => 5.0,
                Tundra => 3.0,
                Taiga => 2.0,
                Desert => 1.0,
                Plains | Savanna | Forest => 0.4,
                _ => 0.2,
            },
            Culture::ElfesDesBois => match biome {
                Forest | Jungle => 4.0,
                Taiga => 2.0,
                Plains => 0.5,
                _ => 0.15,
            },
            Culture::HautsElfes => match biome {
                Beach => 2.0,
                Plains | Forest => 1.5,
                Savanna | Tundra => 0.5,
                _ => 0.2,
            },
            // Camps de clan des terres chaudes et du basalte
            Culture::DemiOrques => match biome {
                Savanna => 3.0,
                Desert => 2.5,
                Jungle => 2.0,
                Mountains => 1.0,
                Plains => 0.5,
                _ => 0.2,
            },
            // Hameaux sur pilotis des rivages et des brumes froides
            Culture::Revenants => match biome {
                Beach => 4.0,
                Tundra => 2.5,
                Taiga => 1.0,
                Plains | Forest => 0.4,
                _ => 0.2,
            },
            // Tours perchées des hauteurs ventées
            Culture::Sylphes => match biome {
                Mountains => 4.0,
                Tundra => 2.5,
                Taiga => 1.5,
                _ => 0.2,
            },
            // Buttes des plaines et des lisières
            Culture::Gnomes => match biome {
                Plains => 3.0,
                Forest => 2.5,
                Taiga | Savanna => 0.6,
                _ => 0.2,
            },
            // Sanctuaires des plateaux et des terres ensoleillées
            Culture::Aasimars => match biome {
                Mountains => 2.5,
                Savanna => 2.0,
                Plains => 1.5,
                Desert => 0.6,
                _ => 0.3,
            },
            // Huttes sur plateformes des savanes, des jungles et des taïgas
            Culture::HommesBetes => match biome {
                Savanna | Jungle => 3.0,
                Taiga => 2.0,
                Forest => 1.5,
                Plains => 0.5,
                _ => 0.2,
            },
        }
    }

    /// Culture d'une ville (`town`) ou d'un village du biome `biome`, dans un royaume qui penche vers
    /// `realm`, `roll` étant un tirage uniforme dans [0, 1). Une ville n'est jamais d'une culture qui
    /// ne bâtit que des villages.
    pub fn choose(biome: BiomeType, realm: Culture, roll: f64, town: bool) -> Culture {
        let choices: &[Culture] = if town { &Culture::TOWN_BUILDERS } else { &Culture::ALL };
        let weight = |c: Culture| c.affinity(biome) * if c == realm { 4.0 } else { 1.0 };
        let total: f64 = choices.iter().map(|&c| weight(c)).sum();
        let mut left = roll * total;
        for &c in choices {
            left -= weight(c);
            if left < 0.0 {
                return c;
            }
        }
        choices[choices.len() - 1]
    }

    /// Culture d'un habitant d'une ville de cette culture, `roll` et `other` étant deux tirages
    /// uniformes dans [0, 1) : celle de la ville le plus souvent, sinon l'une des autres
    pub fn resident(self, roll: f64, other: f64) -> Culture {
        if roll < NATIVE_SHARE {
            return self;
        }
        let others: Vec<Culture> = Culture::ALL.into_iter().filter(|&c| c != self).collect();
        others[((other * others.len() as f64) as usize).min(others.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_culture_is_a_hero_species() {
        let atlas = herocraft_common::hero::atlas();
        for c in Culture::ALL {
            assert_eq!(atlas.espece(c.espece_id()).id, c.espece_id(), "{c:?}");
        }
    }

    #[test]
    fn the_climate_and_the_realm_steer_the_choice() {
        let count = |biome, realm| {
            let mut n = [0usize; 10];
            for i in 0..1000 {
                let c = Culture::choose(biome, realm, i as f64 / 1000.0, false);
                n[Culture::ALL.iter().position(|&x| x == c).unwrap()] += 1;
            }
            n
        };
        // Les Nains dominent les montagnes, les Elfes des bois les forêts
        let mountains = count(BiomeType::Mountains, Culture::Humains);
        assert!(mountains[1] > mountains[2] && mountains[1] > mountains[3], "{mountains:?}");
        let forest = count(BiomeType::Forest, Culture::Humains);
        assert!(forest[2] > forest[1], "{forest:?}");
        // Un royaume de Hauts Elfes en compte bien plus dans ses plaines
        let plains = count(BiomeType::Plains, Culture::Humains);
        let elven = count(BiomeType::Plains, Culture::HautsElfes);
        assert!(elven[3] > 2 * plains[3], "{plains:?} {elven:?}");
        // Chaque culture a sa chance quelque part
        for (k, _) in Culture::ALL.iter().enumerate() {
            assert!(count(BiomeType::Plains, Culture::ALL[k])[k] > 0);
        }
    }

    #[test]
    fn most_residents_are_natives() {
        let natives = (0..100).filter(|&i| Culture::Nains.resident(i as f64 / 100.0, 0.5) == Culture::Nains).count();
        assert_eq!(natives, 85);
        let strangers: std::collections::HashSet<Culture> = (0..9).map(|k| Culture::Nains.resident(0.9, k as f64 / 9.0 + 0.05)).collect();
        assert_eq!(strangers.len(), 9);
        assert!(!strangers.contains(&Culture::Nains));
    }

    #[test]
    fn towns_belong_to_town_builders_only() {
        for biome in BiomeType::ALL {
            for realm in Culture::ALL {
                for i in 0..200 {
                    let c = Culture::choose(biome, realm, i as f64 / 200.0, true);
                    assert!(c.builds_towns(), "ville {c:?} en {biome:?}");
                }
            }
        }
    }

    #[test]
    fn every_race_has_its_climate() {
        // Chaque race est la plus probable dans au moins un climat, dans son propre royaume
        for c in Culture::ALL {
            let favourite = BiomeType::ALL.into_iter().any(|biome| {
                let mut n = [0usize; 10];
                for i in 0..1000 {
                    let k = Culture::choose(biome, c, i as f64 / 1000.0, false);
                    n[Culture::ALL.iter().position(|&x| x == k).unwrap()] += 1;
                }
                let me = n[Culture::ALL.iter().position(|&x| x == c).unwrap()];
                n.iter().all(|&m| m <= me)
            });
            assert!(favourite, "{c:?}");
        }
    }
}

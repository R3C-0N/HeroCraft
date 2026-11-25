//! Identifiants typés des entités du jeu : un joueur, une créature et un projectile ne se
//! confondent plus. Chacun enveloppe un `u64` et se sérialise exactement comme lui
//! (`#[serde(transparent)]`) : le protocole et les sauvegardes ne changent pas d'octet.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! entity_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// Le nombre brut, pour le hacher ou le composer avec d'autres bits
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<$name> for u64 {
            fn from(id: $name) -> u64 {
                id.0
            }
        }
    };
}

entity_id!(
    /// Identifiant d'un joueur connecté, attribué par le serveur
    PlayerId
);
entity_id!(
    /// Identifiant d'une créature (bête, habitant, invocation), unique dans un `CreatureWorld`
    CreatureId
);
entity_id!(
    /// Identifiant d'un projectile en vol
    ProjectileId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_like_their_number() {
        let raw = postcard::to_allocvec(&42u64).unwrap();
        assert_eq!(postcard::to_allocvec(&PlayerId(42)).unwrap(), raw);
        assert_eq!(postcard::to_allocvec(&CreatureId(42)).unwrap(), raw);
        assert_eq!(postcard::to_allocvec(&ProjectileId(42)).unwrap(), raw);
        assert_eq!(serde_json::to_string(&CreatureId(7)).unwrap(), "7");
        let back: PlayerId = postcard::from_bytes(&raw).unwrap();
        assert_eq!(back, PlayerId(42));
    }
}

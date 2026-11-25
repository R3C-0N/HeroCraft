//! Briques de l'interface egui de HeroCraft : habillage (cadres, boutons, polices), palette, chat
//! et commandes, carte du monde, barre et écran des compétences, icônes des objets. L'état global
//! de l'interface (`UiState`, menus, HUD, inventaire) reste dans la crate du client, qui assemble
//! ces briques avec l'état du jeu.

pub mod chat;
pub mod commands;
pub mod item_icons;
pub mod skill_bar;
pub mod skills_screen;
pub mod theme;
pub mod ui_skin;
/// Captures de l'interface sans carte graphique, pour les tests (feature `snapshot`)
#[cfg(any(test, feature = "snapshot"))]
pub mod ui_snapshot;
pub mod world_map;

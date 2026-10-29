use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use winit::keyboard::{Key, KeyCode};

/// Fichier des contrôles du joueur, relatif au dossier de lancement (comme `saves/`)
pub const CONTROLS_FILE: &str = "config/controles.json";

/// Rotation de la vue par unité de déplacement brut de la souris, à sensibilité 1.0 (radians)
pub const LOOK_RADIANS_PER_COUNT: f32 = 0.0022;

/// Délai maximal entre deux appuis sur Saut pour basculer le vol
pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);

/// Actions du joueur attribuables à une touche
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    Jump,
    Descend,
    Sprint,
    ZoomModifier,
    ToggleView,
    ResetView,
    NewPlanet,
    ToggleTectonics,
    ToggleOceanCurrents,
    OpenInventory,
}

/// Groupes d'actions, dans l'ordre d'affichage de la page Commandes
pub const ACTION_GROUPS: [(&str, &[Action]); 4] = [
    (
        "DÉPLACEMENT",
        &[
            Action::MoveForward,
            Action::MoveBackward,
            Action::MoveLeft,
            Action::MoveRight,
            Action::Jump,
            Action::Descend,
            Action::Sprint,
        ],
    ),
    ("OBJETS", &[Action::OpenInventory]),
    ("CAMÉRA", &[Action::ZoomModifier, Action::ToggleView, Action::ResetView]),
    ("MONDE", &[Action::NewPlanet, Action::ToggleTectonics, Action::ToggleOceanCurrents]),
];

impl Action {
    pub const ALL: [Action; 14] = [
        Action::MoveForward,
        Action::MoveBackward,
        Action::MoveLeft,
        Action::MoveRight,
        Action::Jump,
        Action::Descend,
        Action::Sprint,
        Action::ZoomModifier,
        Action::ToggleView,
        Action::ResetView,
        Action::NewPlanet,
        Action::ToggleTectonics,
        Action::ToggleOceanCurrents,
        Action::OpenInventory,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::MoveForward => "Avancer",
            Action::MoveBackward => "Reculer",
            Action::MoveLeft => "Aller à gauche",
            Action::MoveRight => "Aller à droite",
            Action::Jump => "Sauter (deux fois : voler), monter en vol",
            Action::Descend => "Descendre en vol",
            Action::Sprint => "Sprinter",
            Action::ZoomModifier => "Zoom de la vue (maintenir + molette)",
            Action::ToggleView => "Basculer vue au sol / satellite",
            Action::ResetView => "Réinitialiser la vue",
            Action::NewPlanet => "Générer une nouvelle planète",
            Action::ToggleTectonics => "Filtre plaques tectoniques",
            Action::ToggleOceanCurrents => "Filtre courants marins",
            Action::OpenInventory => "Ouvrir l'inventaire",
        }
    }

    /// Touches physiques par défaut : positions ZQSD d'un clavier AZERTY (WASD en QWERTY)
    pub fn default_key(self) -> KeyCode {
        match self {
            Action::MoveForward => KeyCode::KeyW,
            Action::MoveBackward => KeyCode::KeyS,
            Action::MoveLeft => KeyCode::KeyA,
            Action::MoveRight => KeyCode::KeyD,
            Action::Jump => KeyCode::Space,
            Action::Descend => KeyCode::ShiftLeft,
            Action::Sprint => KeyCode::ControlLeft,
            Action::ZoomModifier => KeyCode::AltLeft,
            Action::ToggleView => KeyCode::KeyF,
            Action::ResetView => KeyCode::KeyR,
            Action::NewPlanet => KeyCode::KeyG,
            Action::ToggleTectonics => KeyCode::KeyT,
            Action::ToggleOceanCurrents => KeyCode::KeyC,
            Action::OpenInventory => KeyCode::KeyE,
        }
    }
}

/// Touches réservées au système et à la barre d'objets, jamais attribuables
pub fn is_reserved_key(key: KeyCode) -> bool {
    matches!(key, KeyCode::Escape | KeyCode::F9) || hotbar_slot_for(key).is_some()
}

/// Case de la barre d'objets choisie par une touche de 1 à 9 (rangée des chiffres ou pavé numérique)
pub fn hotbar_slot_for(key: KeyCode) -> Option<usize> {
    const ROW: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    const PAD: [KeyCode; 9] = [
        KeyCode::Numpad1,
        KeyCode::Numpad2,
        KeyCode::Numpad3,
        KeyCode::Numpad4,
        KeyCode::Numpad5,
        KeyCode::Numpad6,
        KeyCode::Numpad7,
        KeyCode::Numpad8,
        KeyCode::Numpad9,
    ];
    ROW.iter().position(|&k| k == key).or_else(|| PAD.iter().position(|&k| k == key))
}

/// Contrôles du joueur : touches, libellés selon la disposition du clavier, souris.
/// Sauvegardés en JSON à chaque modification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    bindings: HashMap<Action, KeyCode>,
    /// Caractère réellement imprimé sur chaque touche physique, appris à la frappe
    /// (sans lui, une touche KeyW s'afficherait « W » sur un clavier AZERTY)
    labels: HashMap<KeyCode, String>,
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            bindings: Action::ALL.iter().map(|&a| (a, a.default_key())).collect(),
            labels: HashMap::new(),
            mouse_sensitivity: 1.0,
            invert_y: false,
            path: None,
        }
    }
}

impl Controls {
    /// Charge les contrôles depuis `path`, ou les valeurs par défaut si le fichier est absent ou illisible.
    /// Les sauvegardes suivantes écrivent dans `path`.
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let mut controls = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Controls>(&text).unwrap_or_else(|e| {
                log::warn!("Contrôles illisibles dans {} ({}), touches par défaut.", path.display(), e);
                Controls::default()
            }),
            Err(_) => Controls::default(),
        };
        controls.repair();
        controls.path = Some(path.to_path_buf());
        controls
    }

    /// Complète les actions manquantes et dédoublonne les touches d'un fichier ancien ou édité à la main
    fn repair(&mut self) {
        let defaults = Controls::default();
        for action in Action::ALL {
            let key = self.key(action);
            let taken = Action::ALL
                .iter()
                .take_while(|&&other| other != action)
                .any(|&other| self.bindings.get(&other) == Some(&key));
            if !self.bindings.contains_key(&action) || taken || is_reserved_key(key) {
                self.bindings.insert(action, defaults.key(action));
            }
        }
        // Une touche par défaut remise peut encore croiser une touche personnalisée : on repart de zéro
        let unique = Action::ALL.iter().all(|&a| self.action_for(self.key(a)) == Some(a));
        if !unique {
            self.reset_bindings();
        }
        self.mouse_sensitivity = self.mouse_sensitivity.clamp(0.1, 5.0);
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            std::fs::write(path, text)
        })();
        if let Err(e) = result {
            log::warn!("Impossible d'enregistrer les contrôles dans {} : {}", path.display(), e);
        }
    }

    pub fn key(&self, action: Action) -> KeyCode {
        self.bindings.get(&action).copied().unwrap_or(action.default_key())
    }

    pub fn action_for(&self, key: KeyCode) -> Option<Action> {
        Action::ALL.into_iter().find(|&a| self.key(a) == key)
    }

    /// Attribue `key` à `action`. Si une autre action l'utilisait, elle reçoit l'ancienne touche
    /// de `action` (échange) ; elle est alors renvoyée.
    pub fn rebind(&mut self, action: Action, key: KeyCode) -> Option<Action> {
        if is_reserved_key(key) {
            return None;
        }
        let old = self.key(action);
        let swapped = self.action_for(key).filter(|&other| other != action);
        if let Some(other) = swapped {
            self.bindings.insert(other, old);
        }
        self.bindings.insert(action, key);
        swapped
    }

    pub fn reset_bindings(&mut self) {
        self.bindings = Controls::default().bindings;
    }

    /// Retient le caractère produit par une touche physique pour l'afficher ensuite
    pub fn learn_label(&mut self, key: KeyCode, logical: &Key) {
        if let Key::Character(text) = logical {
            let label = text.to_uppercase();
            if !label.trim().is_empty() && label.chars().count() <= 2 {
                self.labels.insert(key, label);
            }
        }
    }

    /// Nom affichable d'une touche physique
    pub fn key_label(&self, key: KeyCode) -> String {
        if let Some(label) = self.labels.get(&key) {
            return label.clone();
        }
        let name = match key {
            KeyCode::Space => "Espace",
            KeyCode::ShiftLeft => "Maj",
            KeyCode::ShiftRight => "Maj droite",
            KeyCode::ControlLeft => "Ctrl",
            KeyCode::ControlRight => "Ctrl droite",
            KeyCode::AltLeft => "Alt",
            KeyCode::AltRight => "Alt Gr",
            KeyCode::SuperLeft | KeyCode::SuperRight => "Super",
            KeyCode::Tab => "Tab",
            KeyCode::CapsLock => "Verr. Maj",
            KeyCode::Enter => "Entrée",
            KeyCode::NumpadEnter => "Entrée pavé",
            KeyCode::Backspace => "Retour arrière",
            KeyCode::Delete => "Suppr",
            KeyCode::Insert => "Inser",
            KeyCode::Home => "Début",
            KeyCode::End => "Fin",
            KeyCode::PageUp => "Page préc.",
            KeyCode::PageDown => "Page suiv.",
            KeyCode::ArrowUp => "Flèche haut",
            KeyCode::ArrowDown => "Flèche bas",
            KeyCode::ArrowLeft => "Flèche gauche",
            KeyCode::ArrowRight => "Flèche droite",
            _ => "",
        };
        if !name.is_empty() {
            return name.to_string();
        }
        let debug = format!("{:?}", key);
        debug
            .strip_prefix("Key")
            .or_else(|| debug.strip_prefix("Digit"))
            .map(str::to_string)
            .unwrap_or_else(|| debug.replace("Numpad", "Pavé "))
    }

    /// Nom affichable de la touche d'une action
    pub fn action_key_label(&self, action: Action) -> String {
        self.key_label(self.key(action))
    }
}

/// Détecte un double appui rapide sur une touche
#[derive(Debug, Default)]
pub struct DoubleTap {
    last_press: Option<Instant>,
}

impl DoubleTap {
    /// Enregistre un appui ; renvoie vrai s'il suit le précédent de moins de `DOUBLE_TAP_WINDOW`.
    /// Un double appui est consommé : un troisième appui rapide recommence une nouvelle série.
    pub fn press(&mut self, now: Instant) -> bool {
        let is_double = self.last_press.is_some_and(|t| now.duration_since(t) <= DOUBLE_TAP_WINDOW);
        self.last_press = if is_double { None } else { Some(now) };
        is_double
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings_are_unique() {
        let controls = Controls::default();
        for a in Action::ALL {
            assert_eq!(controls.action_for(controls.key(a)), Some(a), "{:?} partage sa touche", a);
            assert!(!is_reserved_key(controls.key(a)));
        }
        let grouped: Vec<Action> = ACTION_GROUPS.iter().flat_map(|(_, actions)| actions.iter().copied()).collect();
        assert_eq!(grouped.len(), Action::ALL.len(), "chaque action apparaît une fois dans la page Commandes");
        for a in Action::ALL {
            assert!(grouped.contains(&a));
        }
    }

    #[test]
    fn test_rebind_swaps_conflicting_action() {
        let mut controls = Controls::default();
        let swapped = controls.rebind(Action::Sprint, KeyCode::KeyW);
        assert_eq!(swapped, Some(Action::MoveForward));
        assert_eq!(controls.key(Action::Sprint), KeyCode::KeyW);
        assert_eq!(controls.key(Action::MoveForward), KeyCode::ControlLeft);

        // Touche libre : aucun échange
        assert_eq!(controls.rebind(Action::Sprint, KeyCode::KeyP), None);
        assert_eq!(controls.action_for(KeyCode::KeyW), None);

        // Touches réservées refusées
        assert_eq!(controls.rebind(Action::Jump, KeyCode::Escape), None);
        assert_eq!(controls.rebind(Action::Jump, KeyCode::Digit3), None, "les chiffres choisissent la case de la barre");
        assert_eq!(controls.key(Action::Jump), KeyCode::Space);

        controls.reset_bindings();
        assert_eq!(controls.key(Action::Sprint), KeyCode::ControlLeft);
    }

    #[test]
    fn test_labels_follow_keyboard_layout() {
        let mut controls = Controls::default();
        assert_eq!(controls.action_key_label(Action::MoveForward), "W");
        assert_eq!(controls.action_key_label(Action::Jump), "Espace");
        // Clavier AZERTY : la touche physique KeyW produit « z »
        controls.learn_label(KeyCode::KeyW, &Key::Character("z".into()));
        assert_eq!(controls.action_key_label(Action::MoveForward), "Z");
        assert_eq!(controls.key_label(KeyCode::Digit1), "1");
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("herocraft_controls_{}", std::process::id()));
        let path = dir.join("controles.json");
        let mut controls = Controls::load_or_default(&path);
        controls.rebind(Action::Jump, KeyCode::KeyJ);
        controls.mouse_sensitivity = 2.0;
        controls.invert_y = true;
        controls.save();

        let loaded = Controls::load_or_default(&path);
        assert_eq!(loaded.key(Action::Jump), KeyCode::KeyJ);
        assert_eq!(loaded.mouse_sensitivity, 2.0);
        assert!(loaded.invert_y);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_repair_fills_missing_and_duplicate_keys() {
        let json = r#"{ "bindings": { "Jump": "KeyW" }, "mouse_sensitivity": 99.0 }"#;
        let mut controls: Controls = serde_json::from_str(json).unwrap();
        controls.repair();
        // MoveForward passe avant Jump : il garde KeyW, Jump revient à sa touche par défaut
        assert_eq!(controls.key(Action::MoveForward), KeyCode::KeyW);
        assert_eq!(controls.key(Action::Jump), KeyCode::Space);
        assert_eq!(controls.mouse_sensitivity, 5.0);
    }

    #[test]
    fn test_double_tap_window() {
        let t0 = Instant::now();
        let mut tap = DoubleTap::default();
        assert!(!tap.press(t0));
        assert!(tap.press(t0 + Duration::from_millis(200)));
        // Le double appui est consommé
        assert!(!tap.press(t0 + Duration::from_millis(350)));
        // Trop lent
        assert!(!tap.press(t0 + Duration::from_millis(900)));
    }
}

//! Crochet clavier bas niveau (Windows) : en jeu, souris capturée, les touches attribuées sont lues
//! avant les autres programmes qui écoutent le clavier, et ne leur parviennent pas. Sans lui,
//! PowerToys (Accent rapide) garde Espace quand une lettre est maintenue, et le saut n'arrive jamais.
//!
//! Windows appelle d'abord le dernier crochet posé : celui du jeu, posé après PowerToys, passe
//! avant lui. Les touches de modification (Ctrl, Maj, Alt, Windows) ne sont jamais interceptées,
//! pour que Alt+Tab et les raccourcis du système restent disponibles. Ailleurs que sous Windows,
//! le module ne fait rien et le jeu lit le clavier par la fenêtre.

use winit::keyboard::KeyCode;

/// Touche lue par le crochet, à traiter comme un événement clavier de la fenêtre
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookedKey {
    pub key: KeyCode,
    pub pressed: bool,
    pub repeat: bool,
}

/// Touches que le crochet ne prend jamais
fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
            | KeyCode::CapsLock
    )
}

/// Ne garde que les touches que le crochet peut intercepter
pub(crate) fn interceptable(keys: impl IntoIterator<Item = KeyCode>) -> Vec<KeyCode> {
    keys.into_iter().filter(|&k| !is_modifier(k)).collect()
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    use log::{info, warn};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG,
        SetWindowsHookExW, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::platform::scancode::PhysicalKeyExtScancode;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::HookedKey;

    /// Le jeu a la souris capturée : les touches attribuées sont interceptées
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    /// Fenêtre du jeu : rien n'est intercepté quand une autre fenêtre est au premier plan
    static WINDOW: AtomicIsize = AtomicIsize::new(0);

    struct HookState {
        keys: Vec<KeyCode>,
        held: Vec<KeyCode>,
        events: Vec<HookedKey>,
    }

    static STATE: Mutex<HookState> = Mutex::new(HookState { keys: Vec::new(), held: Vec::new(), events: Vec::new() });

    /// Pose le crochet sur un fil dédié : il doit répondre vite même quand une image tarde
    pub fn install(window: &Window) {
        let hwnd = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => return,
        };
        WINDOW.store(hwnd, Ordering::Relaxed);
        // SAFETY: appels Win32 sur ce fil, qui garde le crochet et sa boucle de messages jusqu'à
        // la fin du jeu ; `hook_proc` a la signature attendue par `WH_KEYBOARD_LL`, `GetModuleHandleW`
        // reçoit un nom nul (le module du jeu) et `MSG` est une structure C valide à zéro
        let spawned = std::thread::Builder::new().name("crochet-clavier".into()).spawn(|| unsafe {
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(std::ptr::null()), 0);
            if hook == 0 {
                warn!("Crochet clavier impossible : les autres programmes peuvent garder certaines touches");
                return;
            }
            info!("Crochet clavier posé : les touches du jeu passent avant les autres programmes");
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {}
        });
        if let Err(e) = spawned {
            warn!("Fil du crochet clavier impossible : {}", e);
        }
    }

    pub fn set_capture(keys: Option<Vec<KeyCode>>) {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        ACTIVE.store(keys.is_some(), Ordering::Relaxed);
        state.keys = keys.unwrap_or_default();
        state.held.clear();
    }

    pub fn drain() -> Vec<HookedKey> {
        std::mem::take(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()).events)
    }

    /// # Safety
    /// Appelée seulement par Windows pour le crochet `WH_KEYBOARD_LL` : `lparam` pointe alors sur
    /// un `KBDLLHOOKSTRUCT` valide le temps de l'appel.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && ACTIVE.load(Ordering::Relaxed) && intercept(wparam, lparam) {
            return 1;
        }
        CallNextHookEx(0, code, wparam, lparam)
    }

    /// Prend la touche pour le jeu si elle lui est attribuée et que sa fenêtre est au premier plan
    ///
    /// # Safety
    /// `lparam` doit pointer sur un `KBDLLHOOKSTRUCT` valide (celui que reçoit `hook_proc`).
    unsafe fn intercept(wparam: WPARAM, lparam: LPARAM) -> bool {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Les touches simulées (caractères envoyés par d'autres programmes) passent
        if info.flags & LLKHF_INJECTED != 0 || GetForegroundWindow() != WINDOW.load(Ordering::Relaxed) {
            return false;
        }
        let extended = if info.flags & LLKHF_EXTENDED != 0 { 0xE000 } else { 0 };
        let PhysicalKey::Code(key) = PhysicalKey::from_scancode(info.scanCode | extended) else {
            return false;
        };
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if !state.keys.contains(&key) {
            return false;
        }
        let pressed = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let repeat = state.held.contains(&key);
        if pressed && !repeat {
            state.held.push(key);
        } else if !pressed {
            state.held.retain(|&k| k != key);
        }
        state.events.push(HookedKey { key, pressed, repeat: pressed && repeat });
        true
    }
}

#[cfg(not(windows))]
mod imp {
    use winit::keyboard::KeyCode;
    use winit::window::Window;

    use super::HookedKey;

    pub(crate) fn install(_window: &Window) {}

    pub(crate) fn set_capture(_keys: Option<Vec<KeyCode>>) {}

    pub(crate) fn drain() -> Vec<HookedKey> {
        Vec::new()
    }
}

/// Touches lues par le crochet depuis le dernier appel
pub(crate) use imp::drain;
/// Pose le crochet (une fois, au lancement) ; il reste inactif tant que `set_capture` ne l'arme pas
pub(crate) use imp::install;
/// Arme le crochet sur ces touches (`Some`, souris capturée en jeu) ou le désarme (`None`)
pub(crate) use imp::set_capture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifiers_are_never_intercepted() {
        let keys = interceptable([KeyCode::KeyW, KeyCode::ControlLeft, KeyCode::Space, KeyCode::AltLeft, KeyCode::ShiftLeft]);
        assert_eq!(keys, vec![KeyCode::KeyW, KeyCode::Space]);
    }
}

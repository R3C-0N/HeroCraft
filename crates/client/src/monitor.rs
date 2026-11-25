//! Choix de l'écran sur lequel s'ouvre la fenêtre, via la variable `HEROCRAFT_MONITOR`
//! (lue dans l'environnement ou dans le fichier `.env`).
//!
//! Valeurs acceptées :
//! - un nom d'écran Windows : `DISPLAY3` ou `\\.\DISPLAY3` ;
//! - le nom donné par winit : `rdp-4` sous WSLg, `\\.\DISPLAY3` sous Windows natif.
//!
//! Sous WSLg, les écrans portent des noms (`rdp-N`) sans rapport avec la numérotation
//! Windows : on demande alors à Windows (via `powershell.exe`) la position de l'écran voulu,
//! puis on retrouve l'écran winit placé au même endroit.

use log::{info, warn};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::monitor::MonitorHandle;

pub const MONITOR_VAR: &str = "HEROCRAFT_MONITOR";

/// Écran demandé par `HEROCRAFT_MONITOR`, ou `None` (choix laissé au système).
pub fn requested() -> Option<String> {
    std::env::var(MONITOR_VAR).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Crée la boucle d'événements. Sous Linux, Wayland ne laisse pas une application placer
/// sa fenêtre : si un écran est demandé, on passe par X11 (XWayland sous WSLg).
pub fn build_event_loop(wants_monitor: bool) -> anyhow::Result<EventLoop<()>> {
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "linux")]
    if wants_monitor {
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = wants_monitor;
    anyhow::Context::context(builder.build(), "Échec d'initialisation de l'EventLoop Winit")
}

/// Retrouve l'écran correspondant à `wanted` parmi ceux que voit winit.
pub fn find(event_loop: &ActiveEventLoop, wanted: &str) -> Option<MonitorHandle> {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    let key = normalize(wanted);

    if let Some(m) = monitors.iter().find(|m| m.name().is_some_and(|n| normalize(&n) == key)) {
        return Some(m.clone());
    }

    // Nom Windows introuvable tel quel : on est probablement sous WSLg
    if key.starts_with("DISPLAY")
        && let Some(m) = find_via_windows(&monitors, &key) {
        return Some(m);
    }

    let names: Vec<String> = monitors.iter().filter_map(|m| m.name()).collect();
    warn!("{MONITOR_VAR}={wanted} : écran introuvable (écrans vus : {names:?}).");
    None
}

/// Position qui centre une fenêtre de taille `window` sur l'écran `monitor`.
pub fn centered_position(monitor: &MonitorHandle, window: PhysicalSize<u32>) -> PhysicalPosition<i32> {
    let (pos, size) = (monitor.position(), monitor.size());
    PhysicalPosition::new(
        pos.x + (size.width as i32 - window.width as i32).max(0) / 2,
        pos.y + (size.height as i32 - window.height as i32).max(0) / 2,
    )
}

/// `\\.\display3` -> `DISPLAY3`
fn normalize(name: &str) -> String {
    name.trim().trim_start_matches(r"\\.\").to_ascii_uppercase()
}

/// Demande à Windows où se trouve l'écran `key`, puis prend l'écran winit le plus proche.
/// Windows et WSLg n'ont pas la même origine : on ramène les deux au coin haut-gauche
/// de l'ensemble des écrans avant de comparer.
fn find_via_windows(monitors: &[MonitorHandle], key: &str) -> Option<MonitorHandle> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; \
             [System.Windows.Forms.Screen]::AllScreens | % { '{0} {1} {2}' -f $_.DeviceName,$_.Bounds.X,$_.Bounds.Y }",
        ])
        .output()
        .ok()?;
    let screens: Vec<(String, i32, i32)> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((normalize(parts.next()?), parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .collect();

    let (_, wx, wy) = screens.iter().find(|(name, ..)| name == key)?;
    let (win_min_x, win_min_y) = min_corner(screens.iter().map(|&(_, x, y)| (x, y)))?;
    let target = (wx - win_min_x, wy - win_min_y);

    let (min_x, min_y) = min_corner(monitors.iter().map(|m| (m.position().x, m.position().y)))?;
    let found = monitors
        .iter()
        .min_by_key(|m| {
            let p = m.position();
            (p.x - min_x - target.0).abs() + (p.y - min_y - target.1).abs()
        })?
        .clone();
    info!("{MONITOR_VAR} : écran Windows {key} = écran {:?} (position {:?}).", found.name(), found.position());
    Some(found)
}

fn min_corner(points: impl Iterator<Item = (i32, i32)>) -> Option<(i32, i32)> {
    points.fold(None, |acc, (x, y)| match acc {
        None => Some((x, y)),
        Some((ax, ay)) => Some((ax.min(x), ay.min(y))),
    })
}

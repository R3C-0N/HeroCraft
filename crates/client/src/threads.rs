//! Priorité des fils de travail : la génération du monde, les cartes et les chargements de fond
//! passent après le fil principal (simulation et rendu) quand tous les cœurs sont occupés. Sans
//! cela, un pic de génération (arrivée sur une zone neuve) volait du temps au fil principal et se
//! voyait en à-coups.

use thread_priority::{set_current_thread_priority, ThreadPriority, ThreadPriorityValue};

/// Passe le fil courant en priorité basse (sous la normale : `BelowNormal` sous Windows, gentillesse
/// 7 sous Linux). Sans effet si le système refuse.
pub fn lower_current_thread_priority() {
    let Ok(value) = ThreadPriorityValue::try_from(30u8) else { return };
    if let Err(e) = set_current_thread_priority(ThreadPriority::Crossplatform(value)) {
        log::debug!("Priorité du fil {:?} inchangée : {e:?}", std::thread::current().name());
    }
}

#[cfg(test)]
mod tests {
    /// Gentillesse du fil courant (Linux : 19e champ de /proc/thread-self/stat)
    #[cfg(target_os = "linux")]
    fn niceness() -> i32 {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        // Le nom du fil, entre parenthèses, peut contenir des espaces
        let after = &stat[stat.rfind(')').unwrap() + 2..];
        after.split(' ').nth(16).unwrap().parse().unwrap()
    }

    /// Un fil de travail peut baisser sa propre priorité sans droits particuliers
    #[cfg(target_os = "linux")]
    #[test]
    fn worker_priority_can_be_lowered() {
        std::thread::spawn(|| {
            let before = niceness();
            super::lower_current_thread_priority();
            assert!(niceness() > before, "gentillesse {} -> {}", before, niceness());
        })
        .join()
        .unwrap();
    }
}

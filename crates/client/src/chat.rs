//! Chat du jeu : la touche Entrée (modifiable dans Échap > Commandes) ouvre une ligne de saisie en
//! bas à gauche. Un message part aux autres joueurs en ligne ; une ligne qui commence par « / » est
//! une commande (`commands.rs`). Les derniers messages restent affichés quelques secondes, chat fermé.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::commands::{self, CompletionSources};

/// Lignes gardées dans le chat, et lignes de saisie gardées dans l'historique (↑ / ↓)
const MAX_LINES: usize = 200;
const MAX_HISTORY: usize = 50;
/// Chat fermé : les lignes restent affichées tant de temps, et s'effacent sur la dernière seconde
const LINE_VISIBLE: Duration = Duration::from_secs(10);
const LINE_FADE: Duration = Duration::from_secs(1);
/// Lignes affichées chat fermé
const MAX_CLOSED_LINES: usize = 10;
/// Longueur maximale d'une saisie, en caractères (celle des messages que relaie le serveur)
pub const MAX_INPUT_CHARS: usize = 256;
/// Largeur du chat et hauteur de son historique déroulant, en points logiques
const CHAT_WIDTH: f32 = 520.0;
const CHAT_HISTORY_HEIGHT: f32 = 240.0;
/// Le chat se place au-dessus de la fiche du personnage, en bas à gauche
const CHAT_BOTTOM_OFFSET: f32 = 110.0;
/// Suggestions de Tab affichées au-dessus de la saisie
const MAX_SUGGESTIONS: usize = 8;

/// Nature d'une ligne : elle en tire sa couleur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Message d'un joueur
    Player,
    /// Annonce du serveur (arrivées, départs, morts)
    Server,
    /// Réponse d'une commande
    Info,
    /// Commande refusée ou en échec
    Error,
    /// Commande tapée par le joueur, rappelée en gris
    Echo,
    /// Réplique d'un habitant à qui l'on parle
    Villager,
}

#[derive(Debug, Clone)]
pub struct ChatLine {
    pub sender: Option<String>,
    pub text: String,
    pub kind: LineKind,
    pub at: Instant,
}

#[derive(Default)]
pub struct Chat {
    /// Ligne de saisie ouverte (la souris est libérée, les touches ne pilotent plus le joueur)
    pub open: bool,
    input: String,
    lines: VecDeque<ChatLine>,
    /// Lignes déjà envoyées, de la plus ancienne à la plus récente, et celle rappelée par ↑ / ↓
    history: Vec<String>,
    history_pos: Option<usize>,
    /// Saisie en cours mise de côté pendant qu'on parcourt l'historique
    draft: String,
    /// Lignes validées par Entrée, que la boucle de jeu envoie ou exécute
    submitted: Vec<String>,
    /// Suites proposées par Tab, celle affichée, et la saisie qu'elles complètent
    completion: Option<Completion>,
    /// Le champ de saisie doit prendre le focus (à l'ouverture)
    focus_pending: bool,
    /// Identifiants proposés par Tab : blocs et objets (`/give`), créatures (`/summon`), joueurs
    pub items: Vec<String>,
    pub creatures: Vec<String>,
    pub players: Vec<String>,
    /// Cinématiques (`/cinematic`)
    pub cinematics: Vec<String>,
}

struct Completion {
    candidates: Vec<String>,
    index: usize,
}

impl Chat {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_pending = true;
        self.history_pos = None;
        self.completion = None;
    }

    /// Referme la saisie ; le texte commencé est gardé pour la prochaine ouverture
    pub fn close(&mut self) {
        self.open = false;
        self.completion = None;
    }

    fn push(&mut self, sender: Option<String>, text: impl Into<String>, kind: LineKind) {
        self.lines.push_back(ChatLine { sender, text: text.into(), kind, at: Instant::now() });
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    /// Message d'un joueur
    pub fn player(&mut self, sender: &str, text: &str) {
        self.push(Some(sender.to_string()), text, LineKind::Player);
    }

    /// Réplique d'un habitant (`sender` : son nom, et son métier)
    pub fn villager(&mut self, sender: &str, text: &str) {
        self.push(Some(sender.to_string()), text, LineKind::Villager);
    }

    /// Annonce du serveur
    pub fn server(&mut self, text: &str) {
        self.push(None, text, LineKind::Server);
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(None, text, LineKind::Info);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(None, text, LineKind::Error);
    }

    pub fn echo(&mut self, text: &str) {
        self.push(None, text, LineKind::Echo);
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    #[cfg(test)]
    pub fn lines(&self) -> impl Iterator<Item = &ChatLine> {
        self.lines.iter()
    }

    /// Lignes validées depuis le dernier appel
    pub fn take_submitted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.submitted)
    }

    /// Valide la saisie : gardée dans l'historique, puis rendue par `take_submitted`. Referme le chat.
    pub fn submit(&mut self) {
        let line: String = self.input.trim().chars().filter(|c| !c.is_control()).take(MAX_INPUT_CHARS).collect();
        self.input.clear();
        self.draft.clear();
        self.close();
        if line.is_empty() {
            return;
        }
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
        self.submitted.push(line);
    }

    /// Rappelle une ligne de l'historique : `-1` la précédente (↑), `+1` la suivante (↓)
    pub fn recall(&mut self, step: i32) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        let pos = match (self.history_pos, step < 0) {
            (None, true) => {
                self.draft = self.input.clone();
                Some(last)
            }
            (None, false) => None,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i >= last => None,
            (Some(i), false) => Some(i + 1),
        };
        self.history_pos = pos;
        self.input = match pos {
            Some(i) => self.history[i].clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.completion = None;
    }

    /// Tab : complète le dernier mot de la saisie ; de nouveaux appuis passent d'une suite à l'autre
    pub fn complete(&mut self) {
        if let Some(c) = self.completion.as_mut().filter(|c| c.candidates.get(c.index) == Some(&self.input)) {
            c.index = (c.index + 1) % c.candidates.len();
            self.input = c.candidates[c.index].clone();
            return;
        }
        let sources = CompletionSources { items: &self.items, creatures: &self.creatures, players: &self.players, cinematics: &self.cinematics };
        let candidates = commands::complete(&self.input, &sources);
        let Some(first) = candidates.first() else {
            self.completion = None;
            return;
        };
        self.input = first.clone();
        // Une seule suite : le mot est fini, on passe à l'argument suivant
        if candidates.len() == 1 {
            self.input.push(' ');
            self.completion = None;
        } else {
            self.completion = Some(Completion { candidates, index: 0 });
        }
    }

    /// Dessine les dernières lignes (chat fermé) ou l'historique et la saisie (chat ouvert)
    pub fn draw(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if !self.open && self.lines.back().is_none_or(|l| now.duration_since(l.at) >= LINE_VISIBLE) {
            return;
        }
        let screen = ctx.screen_rect();
        let width = CHAT_WIDTH.min(screen.width() - 32.0);
        let input_id = egui::Id::new("chat_input");

        if self.open {
            // Touches du chat, retirées avant que le champ de saisie ne les voie
            let (tab, up, down) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                )
            });
            let before = self.input.clone();
            if tab {
                self.complete();
            }
            if up {
                self.recall(-1);
            }
            if down {
                self.recall(1);
            }
            if self.input != before {
                move_cursor_to_end(ctx, input_id, &self.input);
            }
        }

        egui::Area::new(egui::Id::new("chat"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(16.0, screen.bottom() - CHAT_BOTTOM_OFFSET))
            .interactable(self.open)
            .show(ctx, |ui| {
                ui.set_width(width);
                if self.open {
                    egui::Frame::none().fill(egui::Color32::from_black_alpha(150)).rounding(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.set_width(width - 12.0);
                        egui::ScrollArea::vertical().max_height(CHAT_HISTORY_HEIGHT).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
                            for line in &self.lines {
                                ui.label(line_text(line, 1.0));
                            }
                        });
                    });
                    self.draw_suggestions(ui);
                    let response = egui::Frame::none().fill(egui::Color32::from_black_alpha(190)).rounding(4.0).inner_margin(4.0).show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .id(input_id)
                                .desired_width(width - 8.0)
                                .char_limit(MAX_INPUT_CHARS)
                                .lock_focus(true)
                                .frame(false)
                                .hint_text("Message, or /help for the commands")
                                .font(egui::FontId::proportional(15.0)),
                        )
                    });
                    let response = response.inner;
                    if response.changed() {
                        self.history_pos = None;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.submit();
                    } else if self.focus_pending || !response.has_focus() {
                        response.request_focus();
                        self.focus_pending = false;
                    }
                } else {
                    let recent: Vec<&ChatLine> = self.lines.iter().rev().take(MAX_CLOSED_LINES).take_while(|l| now.duration_since(l.at) < LINE_VISIBLE).collect();
                    for line in recent.into_iter().rev() {
                        let left = LINE_VISIBLE.saturating_sub(now.duration_since(line.at));
                        let alpha = (left.as_secs_f32() / LINE_FADE.as_secs_f32()).min(1.0);
                        egui::Frame::none().fill(egui::Color32::from_black_alpha((110.0 * alpha) as u8)).inner_margin(egui::Margin::symmetric(5.0, 1.0)).show(ui, |ui| {
                            ui.label(line_text(line, alpha));
                        });
                    }
                    // Le fondu continue sans attendre d'autre événement
                    ctx.request_repaint();
                }
            });
    }

    /// Suites de Tab au-dessus de la saisie, celle choisie en or
    fn draw_suggestions(&self, ui: &mut egui::Ui) {
        let Some(c) = &self.completion else { return };
        let words: Vec<egui::RichText> = c
            .candidates
            .iter()
            .enumerate()
            .skip(c.index.saturating_sub(MAX_SUGGESTIONS - 1))
            .take(MAX_SUGGESTIONS)
            .map(|(i, full)| {
                let word = full.rsplit(' ').next().unwrap_or(full);
                let text = egui::RichText::new(word).size(13.0);
                if i == c.index { text.color(egui::Color32::from_rgb(255, 215, 0)) } else { text.color(egui::Color32::from_gray(200)) }
            })
            .collect();
        egui::Frame::none().fill(egui::Color32::from_black_alpha(150)).rounding(4.0).inner_margin(4.0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for word in words {
                    ui.label(word);
                }
                if c.candidates.len() > MAX_SUGGESTIONS {
                    ui.label(egui::RichText::new(format!("({} matches)", c.candidates.len())).size(12.0).color(egui::Color32::from_gray(150)));
                }
            });
        });
    }
}

/// Texte coloré d'une ligne : nom du joueur en or, annonces en jaune, erreurs en rouge
fn line_text(line: &ChatLine, alpha: f32) -> egui::text::LayoutJob {
    let fade = |c: egui::Color32| c.gamma_multiply(alpha);
    let font = egui::FontId::proportional(15.0);
    let format = |color: egui::Color32| egui::TextFormat { font_id: font.clone(), color: fade(color), ..Default::default() };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = CHAT_WIDTH - 24.0;
    if let Some(sender) = &line.sender {
        // Un habitant parle en vert, entre crochets, pour ne pas le prendre pour un joueur
        if line.kind == LineKind::Villager {
            job.append(&format!("[{sender}] "), 0.0, format(egui::Color32::from_rgb(140, 210, 120)));
        } else {
            job.append(&format!("<{sender}> "), 0.0, format(egui::Color32::from_rgb(255, 215, 0)));
        }
    }
    let color = match line.kind {
        LineKind::Player => egui::Color32::WHITE,
        LineKind::Server => egui::Color32::from_rgb(255, 235, 120),
        LineKind::Info => egui::Color32::from_rgb(170, 220, 255),
        LineKind::Error => egui::Color32::from_rgb(255, 110, 100),
        LineKind::Echo => egui::Color32::from_gray(190),
        LineKind::Villager => egui::Color32::from_rgb(225, 240, 210),
    };
    job.append(&line.text, 0.0, format(color));
    job
}

/// Place le curseur du champ de saisie au bout du texte (après un rappel ou une complétion)
fn move_cursor_to_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(chat: &mut Chat, text: &str) {
        chat.input = text.to_string();
    }

    #[test]
    fn test_submit_keeps_history_and_closes() {
        let mut chat = Chat::default();
        chat.open();
        typed(&mut chat, "  hello  ");
        chat.submit();
        assert!(!chat.open);
        assert_eq!(chat.take_submitted(), vec!["hello"]);
        assert!(chat.take_submitted().is_empty());
        // Saisie vide : rien n'est envoyé
        chat.open();
        chat.submit();
        assert!(chat.take_submitted().is_empty());
    }

    #[test]
    fn test_history_recall_restores_draft() {
        let mut chat = Chat::default();
        for line in ["/fly", "/seed"] {
            typed(&mut chat, line);
            chat.submit();
        }
        chat.open();
        typed(&mut chat, "draft");
        chat.recall(-1);
        assert_eq!(chat.input, "/seed");
        chat.recall(-1);
        assert_eq!(chat.input, "/fly");
        chat.recall(-1);
        assert_eq!(chat.input, "/fly");
        chat.recall(1);
        assert_eq!(chat.input, "/seed");
        chat.recall(1);
        assert_eq!(chat.input, "draft");
    }

    #[test]
    fn test_tab_cycles_completions() {
        let mut chat = Chat { items: vec!["stone".into(), "stoneSlab".into(), "dirt".into()], ..Default::default() };
        chat.open();
        typed(&mut chat, "/giv");
        chat.complete();
        assert_eq!(chat.input, "/give ");
        chat.input.push_str("sto");
        chat.complete();
        assert_eq!(chat.input, "/give stone");
        chat.complete();
        assert_eq!(chat.input, "/give stoneSlab");
        chat.complete();
        assert_eq!(chat.input, "/give stone");
    }

    #[test]
    fn test_lines_are_capped() {
        let mut chat = Chat::default();
        for i in 0..MAX_LINES + 10 {
            chat.info(format!("line {i}"));
        }
        assert_eq!(chat.lines().count(), MAX_LINES);
        assert_eq!(chat.lines().next().unwrap().text, "line 10");
    }
}

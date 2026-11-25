//! Commandes tapées dans le chat : une ligne qui commence par « / » (`/tp 12 -40`, `/give stone 64`…).
//! Les noms des commandes et leurs messages sont en anglais.
//!
//! Ce module lit la ligne (`parse`), propose la suite au Tab (`complete`) et décrit chaque commande
//! (`/help`). C'est la boucle de jeu qui les exécute (`main.rs`, `run_command`).

/// Une commande lue dans le chat
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Liste des commandes, ou détail de l'une d'elles
    Help(Option<String>),
    /// Téléportation au sol à cette latitude et longitude (degrés), `height` blocs au-dessus
    TeleportLatLon { lat: f32, lon: f32, height: f32 },
    /// Téléportation près d'un autre joueur (en ligne)
    TeleportPlayer(String),
    /// Retour au point d'apparition
    Spawn,
    /// Position du joueur
    Position,
    /// Heure locale
    TimeQuery,
    /// Règle l'heure locale (0 à 24 h)
    TimeSet(f32),
    /// Vitesse du ciel (1 : jour de 20 min, 0 : ciel figé)
    TimeSpeed(f32),
    /// Graine de la planète
    Seed,
    /// Nouvelle planète, de cette graine ou tirée au hasard (solo)
    Planet(Option<u64>),
    /// Donne `count` exemplaires d'un bloc ou d'un objet (identifiant de sa fiche)
    Give { item: String, count: u16 },
    /// Pose `count` exemplaires d'un bloc ou d'un objet au sol, devant le joueur (pour essayer
    /// le ramassage, une Pierre du Néant par exemple)
    Drop { item: String, count: u16 },
    /// Vole ou cesse de voler
    Fly,
    /// Recule la caméra à `distance` blocs du héros (0 : première personne), tournée vers
    /// `yaw` degrés (0 : nord, 90 : est) et inclinée de `pitch` degrés (négatif : vers le bas)
    Camera { distance: f32, yaw: Option<f32>, pitch: Option<f32> },
    /// Multiplicateur de vitesse de déplacement
    Speed(f32),
    /// Fait apparaître `count` créatures devant le joueur (identifiant de l'espèce)
    Summon { species: String, count: u32 },
    /// Jauges pleines
    Heal,
    /// Lance une compétence apprise (identifiant de son fichier)
    Cast(String),
    /// Points d'expérience gagnés
    Experience(u64),
    /// Joueurs en ligne
    List,
    /// Efface le chat
    ClearChat,
    /// Temps qu'il fait (`None`), temps imposé (`Some(Some(kind))`) ou retour au temps naturel
    /// (`Some(None)`)
    Weather(Option<Option<herocraft_common::WeatherKind>>),
    /// Saison et jour de l'année (`None`), ou passage au début d'une saison (hémisphère nord)
    Season(Option<herocraft_world::Season>),
    /// Entre dans la vision du temple le plus proche (`true`) ou en revient
    Vision(bool),
    /// Liste des cinématiques (`None`), ou lecture de l'une d'elles
    Cinematic(Option<String>),
    /// Envoie un événement du jeu (`nom clé=valeur…`), qui peut déclencher une cinématique
    Event(String),
}

/// Une commande : nom, arguments, description (affichés par `/help`)
pub struct CommandHelp {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

/// Toutes les commandes, dans l'ordre de `/help`
pub const COMMANDS: &[CommandHelp] = &[
    CommandHelp { name: "help", usage: "/help [command]", description: "List the commands, or explain one" },
    CommandHelp { name: "tp", usage: "/tp <latitude> <longitude> [height] | /tp <player>", description: "Teleport to the ground at a latitude and longitude (degrees), optionally some blocks above it, or next to a player" },
    CommandHelp { name: "spawn", usage: "/spawn", description: "Go back to the spawn point" },
    CommandHelp { name: "pos", usage: "/pos", description: "Show your latitude, longitude, altitude and block" },
    CommandHelp { name: "time", usage: "/time [set <0-24|hh:mm|sunrise|day|noon|sunset|night|midnight>] | /time speed <factor>", description: "Show or set the local time, or change how fast the sky moves (1 = 20 min day, 0 = frozen)" },
    CommandHelp { name: "weather", usage: "/weather [clear|cloudy|rain|storm|snow|auto]", description: "Show the weather, force it everywhere, or let nature decide again (auto)" },
    CommandHelp { name: "season", usage: "/season [spring|summer|autumn|winter]", description: "Show the season, or jump to the start of one (northern hemisphere; the south has the opposite)" },
    CommandHelp { name: "seed", usage: "/seed", description: "Show the planet seed" },
    CommandHelp { name: "planet", usage: "/planet [seed]", description: "Generate a new planet, random or from a seed (single player only)" },
    CommandHelp { name: "give", usage: "/give <item> [count]", description: "Put blocks or items in your inventory (Tab completes the name)" },
    CommandHelp { name: "drop", usage: "/drop <item> [count]", description: "Drop blocks or items on the ground in front of you (Tab completes the name)" },
    CommandHelp { name: "fly", usage: "/fly", description: "Toggle flight" },
    CommandHelp { name: "camera", usage: "/camera <distance> [yaw|~] [pitch|~]", description: "Put the camera some blocks behind the hero (0 = first person), facing yaw degrees (0 = north) and tilted pitch degrees (negative = down); ~ keeps the current angle" },
    CommandHelp { name: "speed", usage: "/speed <factor>", description: "Movement speed multiplier (1 = normal, up to 20)" },
    CommandHelp { name: "summon", usage: "/summon <creature> [count]", description: "Spawn creatures in front of you" },
    CommandHelp { name: "heal", usage: "/heal", description: "Restore health, mana and stamina" },
    CommandHelp { name: "vision", usage: "/vision [end]", description: "Enter the vision of the nearest temple, or leave it" },
    CommandHelp { name: "cast", usage: "/cast <skill>", description: "Cast a learned skill (Tab completes the name)" },
    CommandHelp { name: "xp", usage: "/xp <amount>", description: "Gain experience points" },
    CommandHelp { name: "list", usage: "/list", description: "List the players online" },
    CommandHelp { name: "clear", usage: "/clear", description: "Clear the chat" },
    CommandHelp { name: "cinematic", usage: "/cinematic [name]", description: "List the cinematics, or play one (Tab completes the name)" },
    CommandHelp { name: "event", usage: "/event <name> [key=value ...]", description: "Send a game event at your feet, as the game would (e.g. /event stone_obtained stone=voidStone kind=whole)" },
];

/// Exemplaires au plus d'un `/give`, de créatures d'un `/summon`, et vitesse maximale
pub const MAX_GIVE: u16 = 64 * 36;
pub const MAX_SUMMON: u32 = 20;
/// Exemplaires au plus d'un `/drop`
pub const MAX_DROP: u16 = 64;
pub const MAX_SPEED: f32 = 20.0;
pub const MAX_XP: u64 = 1_000_000;

/// Lit une ligne de chat qui commence par « / ». Renvoie le message d'erreur à afficher sinon.
pub fn parse(line: &str) -> Result<Command, String> {
    let line = line.trim();
    let body = line.strip_prefix('/').ok_or_else(|| "Commands start with /".to_string())?;
    let mut words = body.split_whitespace();
    let name = words.next().unwrap_or_default().to_lowercase();
    let args: Vec<&str> = words.collect();
    let usage = || COMMANDS.iter().find(|c| c.name == name).map_or(String::new(), |c| format!("Usage: {}", c.usage));
    let wrong = |what: &str| format!("{what}. {}", usage());

    let command = match (name.as_str(), args.as_slice()) {
        ("help" | "?", []) => Command::Help(None),
        ("help" | "?", [command]) => Command::Help(Some(command.trim_start_matches('/').to_lowercase())),
        ("tp" | "teleport", [player]) => Command::TeleportPlayer(player.to_string()),
        ("tp" | "teleport", [lat, lon, rest @ ..]) if rest.len() <= 1 => {
            let lat = number(lat).filter(|l| l.abs() <= 90.0).ok_or_else(|| wrong(&format!("Invalid latitude \"{lat}\" (-90 to 90)")))?;
            let lon = number(lon).filter(|l| l.abs() <= 180.0).ok_or_else(|| wrong(&format!("Invalid longitude \"{lon}\" (-180 to 180)")))?;
            let height = match rest.first() {
                Some(h) => number(h).filter(|h| (0.0..=10_000.0).contains(h)).ok_or_else(|| wrong(&format!("Invalid height \"{h}\" (0 to 10000 blocks)")))?,
                None => 0.0,
            };
            Command::TeleportLatLon { lat, lon, height }
        }
        ("spawn", []) => Command::Spawn,
        ("pos" | "where", []) => Command::Position,
        ("time", []) | ("time", ["query"]) => Command::TimeQuery,
        ("time", ["set", hour]) => Command::TimeSet(parse_hour(hour).ok_or_else(|| wrong(&format!("Invalid time \"{hour}\"")))?),
        ("time", ["speed", factor]) => {
            Command::TimeSpeed(number(factor).filter(|f| (0.0..=100.0).contains(f)).ok_or_else(|| wrong(&format!("Invalid speed \"{factor}\" (0 to 100)")))?)
        }
        ("seed", []) => Command::Seed,
        ("planet", []) => Command::Planet(None),
        ("planet", [seed]) => Command::Planet(Some(seed.parse().map_err(|_| wrong(&format!("Invalid seed \"{seed}\" (a positive whole number)")))?)),
        ("give", [item, rest @ ..]) if rest.len() <= 1 => {
            let count = match rest.first() {
                Some(n) => n.parse::<u16>().ok().filter(|n| (1..=MAX_GIVE).contains(n)).ok_or_else(|| wrong(&format!("Invalid count \"{n}\" (1 to {MAX_GIVE})")))?,
                None => 1,
            };
            Command::Give { item: item.to_string(), count }
        }
        ("drop", [item, rest @ ..]) if rest.len() <= 1 => {
            let count = match rest.first() {
                Some(n) => n.parse::<u16>().ok().filter(|n| (1..=MAX_DROP).contains(n)).ok_or_else(|| wrong(&format!("Invalid count \"{n}\" (1 to {MAX_DROP})")))?,
                None => 1,
            };
            Command::Drop { item: item.to_string(), count }
        }
        ("fly", []) => Command::Fly,
        ("camera" | "cam", [distance, rest @ ..]) if rest.len() <= 2 => {
            let distance = number(distance).filter(|d| (0.0..=2000.0).contains(d)).ok_or_else(|| wrong(&format!("Invalid distance \"{distance}\" (0 to 2000 blocks)")))?;
            let angle = |i: usize, max: f32| -> Result<Option<f32>, String> {
                rest.get(i).filter(|a| **a != "~").map(|a| number(a).filter(|v| v.abs() <= max).ok_or_else(|| wrong(&format!("Invalid angle \"{a}\""))) ).transpose()
            };
            Command::Camera { distance, yaw: angle(0, 360.0)?, pitch: angle(1, 89.0)? }
        }
        ("speed", [factor]) => {
            Command::Speed(number(factor).filter(|f| (0.1..=MAX_SPEED).contains(f)).ok_or_else(|| wrong(&format!("Invalid speed \"{factor}\" (0.1 to {MAX_SPEED})")))?)
        }
        ("summon", [species, rest @ ..]) if rest.len() <= 1 => {
            let count = match rest.first() {
                Some(n) => n.parse::<u32>().ok().filter(|n| (1..=MAX_SUMMON).contains(n)).ok_or_else(|| wrong(&format!("Invalid count \"{n}\" (1 to {MAX_SUMMON})")))?,
                None => 1,
            };
            Command::Summon { species: species.to_string(), count }
        }
        ("heal", []) => Command::Heal,
        ("vision", []) => Command::Vision(true),
        ("vision", ["end" | "leave" | "exit"]) => Command::Vision(false),
        ("cast", [skill]) => Command::Cast(skill.to_string()),
        ("xp" | "experience", [amount]) => {
            Command::Experience(amount.parse().ok().filter(|n| (1..=MAX_XP).contains(n)).ok_or_else(|| wrong(&format!("Invalid amount \"{amount}\" (1 to {MAX_XP})")))?)
        }
        ("list" | "who", []) => Command::List,
        ("clear", []) => Command::ClearChat,
        ("weather" | "meteo", []) => Command::Weather(None),
        ("weather" | "meteo", ["auto" | "natural" | "reset"]) => Command::Weather(Some(None)),
        ("weather" | "meteo", [kind]) => {
            Command::Weather(Some(Some(herocraft_common::WeatherKind::parse(kind).ok_or_else(|| wrong(&format!("Unknown weather \"{kind}\"")))?)))
        }
        ("season" | "saison", []) => Command::Season(None),
        ("season" | "saison", [season]) => Command::Season(Some(herocraft_world::Season::parse(season).ok_or_else(|| wrong(&format!("Unknown season \"{season}\"")))?)),
        ("cinematic" | "cine", []) => Command::Cinematic(None),
        ("cinematic" | "cine", [id]) => Command::Cinematic(Some(id.to_string())),
        ("event", [_, ..]) => {
            if args[1..].iter().any(|a| !a.contains('=') || a.starts_with('=')) {
                return Err(wrong("Tags are written key=value"));
            }
            Command::Event(args.join(" "))
        }
        ("", _) => return Err("Type /help to list the commands".into()),
        (other, _) if COMMANDS.iter().any(|c| c.name == other) => return Err(wrong("Wrong arguments")),
        (other, _) => return Err(format!("Unknown command /{other}. Type /help to list the commands")),
    };
    Ok(command)
}

/// Nombre décimal, virgule acceptée (« 45,5 »)
fn number(text: &str) -> Option<f32> {
    text.replace(',', ".").parse::<f32>().ok().filter(|n| n.is_finite())
}

/// Heure de 0 à 24 : « 14 », « 6.5 », « 18:30 » ou un mot (noon, night…)
pub fn parse_hour(text: &str) -> Option<f32> {
    let named = match text.to_lowercase().as_str() {
        "sunrise" | "dawn" => Some(6.0),
        "day" | "morning" => Some(8.0),
        "noon" | "midday" => Some(12.0),
        "sunset" | "dusk" => Some(18.0),
        "night" => Some(21.0),
        "midnight" => Some(0.0),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    let hour = match text.split_once([':', 'h']) {
        Some((h, m)) => {
            let h: u32 = h.parse().ok()?;
            let m: u32 = if m.is_empty() { 0 } else { m.parse().ok()? };
            (m < 60).then_some(h as f32 + m as f32 / 60.0)?
        }
        None => number(text)?,
    };
    (0.0..=24.0).contains(&hour).then_some(hour % 24.0)
}

/// Heure lisible (« 14:05 »)
pub fn format_hour(hour: f32) -> String {
    let minutes = (hour.rem_euclid(24.0) * 60.0).round() as u32 % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Ce que Tab peut proposer selon la commande : blocs et objets, créatures, joueurs
pub struct CompletionSources<'a> {
    pub items: &'a [String],
    pub creatures: &'a [String],
    pub players: &'a [String],
    pub cinematics: &'a [String],
}

/// Suites possibles du dernier mot de `line` (« /gi » → « /give », « /give cob » → « /give
/// cobbleStone »…), chacune étant la ligne complète. Vide si rien ne correspond.
pub fn complete(line: &str, sources: &CompletionSources) -> Vec<String> {
    let Some(body) = line.strip_prefix('/') else { return Vec::new() };
    let words: Vec<&str> = body.split(' ').collect();
    let (last, before) = words.split_last().expect("split renvoie au moins un morceau");
    let candidates: Vec<String> = match before {
        [] => COMMANDS.iter().map(|c| c.name.to_string()).collect(),
        [command] => match command.to_lowercase().as_str() {
            "give" | "drop" => sources.items.to_vec(),
            "summon" => sources.creatures.to_vec(),
            "cast" => herocraft_common::skills().iter().filter(|s| s.is_castable()).map(|s| s.id.clone()).collect(),
            "tp" | "teleport" => sources.players.to_vec(),
            "help" | "?" => COMMANDS.iter().map(|c| c.name.to_string()).collect(),
            "time" => vec!["set".into(), "speed".into()],
            "weather" | "meteo" => ["clear", "cloudy", "rain", "storm", "snow", "auto"].map(String::from).to_vec(),
            "season" | "saison" => ["spring", "summer", "autumn", "winter"].map(String::from).to_vec(),
            "cinematic" | "cine" => sources.cinematics.to_vec(),
            _ => Vec::new(),
        },
        [command, sub] if command.eq_ignore_ascii_case("time") && sub.eq_ignore_ascii_case("set") => {
            ["sunrise", "day", "noon", "sunset", "night", "midnight"].map(String::from).to_vec()
        }
        _ => Vec::new(),
    };
    let prefix = last.to_lowercase();
    let head = &line[..line.len() - last.len()];
    let mut found: Vec<String> = candidates.into_iter().filter(|c| c.to_lowercase().starts_with(&prefix)).map(|c| format!("{head}{c}")).collect();
    found.sort_by_key(|c| c.to_lowercase());
    found.dedup();
    found
}

/// Texte de `/help` : toutes les commandes, ou le détail de l'une
pub fn help(command: Option<&str>) -> Result<Vec<String>, String> {
    match command {
        None => {
            let mut lines = vec!["Commands (Tab completes, ↑/↓ recall):".to_string()];
            lines.extend(COMMANDS.iter().map(|c| format!("{} — {}", c.usage, c.description)));
            Ok(lines)
        }
        Some(name) => {
            let c = COMMANDS.iter().find(|c| c.name == name).ok_or_else(|| format!("Unknown command /{name}"))?;
            Ok(vec![format!("{}: {}", c.usage, c.description)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_commands() {
        assert_eq!(parse("/help"), Ok(Command::Help(None)));
        assert_eq!(parse("/HELP /tp"), Ok(Command::Help(Some("tp".into()))));
        assert_eq!(parse("/tp 45.5 -73,25"), Ok(Command::TeleportLatLon { lat: 45.5, lon: -73.25, height: 0.0 }));
        assert_eq!(parse("/tp 0 0 30"), Ok(Command::TeleportLatLon { lat: 0.0, lon: 0.0, height: 30.0 }));
        assert_eq!(parse("/tp Mat"), Ok(Command::TeleportPlayer("Mat".into())));
        assert_eq!(parse("/time set noon"), Ok(Command::TimeSet(12.0)));
        assert_eq!(parse("/time set 18:30"), Ok(Command::TimeSet(18.5)));
        assert_eq!(parse("/time speed 0"), Ok(Command::TimeSpeed(0.0)));
        assert_eq!(parse("/time"), Ok(Command::TimeQuery));
        assert_eq!(parse("/planet 1234"), Ok(Command::Planet(Some(1234))));
        assert_eq!(parse("/give cobbleStone 64"), Ok(Command::Give { item: "cobbleStone".into(), count: 64 }));
        assert_eq!(parse("/give torch"), Ok(Command::Give { item: "torch".into(), count: 1 }));
        assert_eq!(parse("/summon loup 3"), Ok(Command::Summon { species: "loup".into(), count: 3 }));
        assert_eq!(parse("  /speed 4 "), Ok(Command::Speed(4.0)));
        assert_eq!(parse("/fly"), Ok(Command::Fly));
        assert_eq!(parse("/camera 12 90 -30"), Ok(Command::Camera { distance: 12.0, yaw: Some(90.0), pitch: Some(-30.0) }));
        assert_eq!(parse("/camera 0"), Ok(Command::Camera { distance: 0.0, yaw: None, pitch: None }));
        assert!(parse("/camera 12 90 -95").is_err());
        assert_eq!(parse("/camera 20 ~ -40"), Ok(Command::Camera { distance: 20.0, yaw: None, pitch: Some(-40.0) }));
        assert_eq!(parse("/xp 250"), Ok(Command::Experience(250)));
        assert_eq!(parse("/cinematic"), Ok(Command::Cinematic(None)));
        assert_eq!(parse("/cinematic fin-de-la-vision"), Ok(Command::Cinematic(Some("fin-de-la-vision".into()))));
        assert_eq!(parse("/event stone_obtained stone=voidStone"), Ok(Command::Event("stone_obtained stone=voidStone".into())));
        assert!(parse("/event stone_obtained voidStone").is_err());
    }

    #[test]
    fn test_parse_errors_explain_usage() {
        assert!(parse("/tp 95 0").unwrap_err().contains("latitude"));
        assert!(parse("/give").unwrap_err().contains("Usage: /give"));
        assert!(parse("/summon loup 500").unwrap_err().contains("1 to 20"));
        assert!(parse("/speed 0").is_err());
        assert!(parse("/time set 25").is_err());
        assert!(parse("/dance").unwrap_err().contains("Unknown command /dance"));
        assert!(parse("/").is_err());
    }

    #[test]
    fn test_hours() {
        assert_eq!(parse_hour("24"), Some(0.0));
        assert_eq!(parse_hour("6h30"), Some(6.5));
        assert_eq!(parse_hour("7h"), Some(7.0));
        assert_eq!(parse_hour("12:60"), None);
        assert_eq!(format_hour(13.999), "14:00");
        assert_eq!(format_hour(-1.0), "23:00");
    }

    #[test]
    fn test_complete() {
        let items = ["cobbleStone".to_string(), "coalOre".to_string(), "stone".to_string()];
        let creatures = ["loup".to_string(), "lapin".to_string()];
        let players = ["Mat".to_string()];
        let sources = CompletionSources { items: &items, creatures: &creatures, players: &players, cinematics: &[] };
        assert_eq!(complete("/gi", &sources), vec!["/give"]);
        assert_eq!(complete("/give co", &sources), vec!["/give coalOre", "/give cobbleStone"]);
        assert_eq!(complete("/give COBB", &sources), vec!["/give cobbleStone"]);
        assert_eq!(complete("/summon lo", &sources), vec!["/summon loup"]);
        assert_eq!(complete("/tp m", &sources), vec!["/tp Mat"]);
        assert_eq!(complete("/time set n", &sources), vec!["/time set night", "/time set noon"]);
        assert!(complete("hello", &sources).is_empty());
        assert!(complete("/give stone 6", &sources).is_empty());
    }

    #[test]
    fn test_weather_and_season() {
        use herocraft_common::WeatherKind;
        assert_eq!(parse("/weather"), Ok(Command::Weather(None)));
        assert_eq!(parse("/weather storm"), Ok(Command::Weather(Some(Some(WeatherKind::Storm)))));
        assert_eq!(parse("/weather pluie"), Ok(Command::Weather(Some(Some(WeatherKind::Rain)))));
        assert_eq!(parse("/weather auto"), Ok(Command::Weather(Some(None))));
        assert!(parse("/weather hail").is_err());
        assert_eq!(parse("/drop voidStone"), Ok(Command::Drop { item: "voidStone".into(), count: 1 }));
        assert!(parse("/drop voidStone 65").unwrap_err().contains("1 to 64"));
        assert_eq!(parse("/season winter"), Ok(Command::Season(Some(herocraft_world::Season::Winter))));
        assert!(parse("/season monsoon").is_err());
        let sources = CompletionSources { items: &[], creatures: &[], players: &[], cinematics: &[] };
        assert_eq!(complete("/weather st", &sources), vec!["/weather storm".to_string()]);
    }

    #[test]
    fn test_every_command_has_help_and_parses() {
        for c in COMMANDS {
            assert!(help(Some(c.name)).is_ok());
        }
        // Chaque commande sans argument obligatoire se lit telle quelle
        for name in ["help", "spawn", "pos", "time", "seed", "planet", "fly", "heal", "vision", "list", "clear", "weather", "season", "cinematic"] {
            assert!(parse(&format!("/{name}")).is_ok(), "/{name}");
        }
    }
}

//! Sons du jeu synthétisés au lancement : bruit filtré, sinus et enveloppes. Aucun fichier audio à
//! livrer ni à licencier, et chaque son existe en plusieurs variantes pour ne pas lasser l'oreille.

use std::f32::consts::TAU;

/// Fréquence d'échantillonnage des sons (mono)
pub const SAMPLE_RATE: u32 = 44_100;

/// Matière d'un bloc, pour le timbre des pas et des blocs cassés ou posés
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    Grass,
    Dirt,
    Stone,
    Sand,
    Snow,
    Wood,
    Water,
}

impl Material {
    pub const ALL: [Material; 7] =
        [Material::Grass, Material::Dirt, Material::Stone, Material::Sand, Material::Snow, Material::Wood, Material::Water];

    /// Matière d'un bloc, d'après le son de sa fiche
    pub fn of(block: herocraft_common::BlockId) -> Material {
        use herocraft_common::BlockSound;
        match herocraft_common::blocks().get(block).map(|sheet| sheet.sound) {
            Some(BlockSound::Grass) => Material::Grass,
            Some(BlockSound::Dirt) => Material::Dirt,
            Some(BlockSound::Sand) => Material::Sand,
            Some(BlockSound::Snow) => Material::Snow,
            Some(BlockSound::Wood) => Material::Wood,
            Some(BlockSound::Water) => Material::Water,
            Some(BlockSound::Stone) | None => Material::Stone,
        }
    }
}

/// Générateur pseudo-aléatoire minimal (xorshift) : les sons sont identiques d'un lancement à l'autre
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9) | 1)
    }

    /// Valeur uniforme dans [-1, 1]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Valeur uniforme dans [0, 1]
    pub fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// Filtre passe-bas du premier ordre
struct OnePole {
    y: f32,
}

impl OnePole {
    fn new() -> Self {
        Self { y: 0.0 }
    }

    fn low(&mut self, x: f32, cutoff: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
        self.y += a * (x - self.y);
        self.y
    }
}

/// Filtre passe-bande (biquad RBJ, gain constant au sommet)
struct BandPass {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BandPass {
    fn new() -> Self {
        Self { x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    fn process(&mut self, x: f32, centre: f32, q: f32) -> f32 {
        let w = TAU * centre.clamp(20.0, SAMPLE_RATE as f32 * 0.45) / SAMPLE_RATE as f32;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b2) = (alpha / a0, -alpha / a0);
        let (a1, a2) = (-2.0 * w.cos() / a0, (1.0 - alpha) / a0);
        let y = b0 * x + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

fn samples(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

/// Enveloppe attaque linéaire puis déclin exponentiel (`decay` en secondes pour perdre ~63 %)
fn envelope(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// Ramène le pic du son à `peak` (les filtres donnent des niveaux très différents selon la matière)
fn normalize(mut buf: Vec<f32>, peak: f32) -> Vec<f32> {
    let max = buf.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if max > 1e-6 {
        let k = peak / max;
        buf.iter_mut().for_each(|s| *s *= k);
    }
    buf
}

/// Timbre d'une matière : bande de bruit, résonance, durée et coup sourd grave
struct Timbre {
    centre: f32,
    q: f32,
    decay: f32,
    length: f32,
    thump_hz: f32,
    thump: f32,
    /// Grains de bruit successifs (craquement de la neige, du gravier)
    grains: usize,
}

fn timbre(material: Material) -> Timbre {
    match material {
        Material::Grass => Timbre { centre: 1800.0, q: 0.7, decay: 0.045, length: 0.16, thump_hz: 90.0, thump: 0.25, grains: 3 },
        Material::Dirt => Timbre { centre: 700.0, q: 0.8, decay: 0.040, length: 0.14, thump_hz: 80.0, thump: 0.55, grains: 1 },
        Material::Stone => Timbre { centre: 2600.0, q: 1.4, decay: 0.020, length: 0.10, thump_hz: 160.0, thump: 0.45, grains: 1 },
        Material::Sand => Timbre { centre: 3200.0, q: 0.6, decay: 0.070, length: 0.20, thump_hz: 70.0, thump: 0.15, grains: 4 },
        Material::Snow => Timbre { centre: 2200.0, q: 1.0, decay: 0.050, length: 0.20, thump_hz: 75.0, thump: 0.20, grains: 6 },
        Material::Wood => Timbre { centre: 900.0, q: 2.5, decay: 0.030, length: 0.14, thump_hz: 210.0, thump: 0.70, grains: 1 },
        Material::Water => Timbre { centre: 1200.0, q: 0.9, decay: 0.090, length: 0.28, thump_hz: 60.0, thump: 0.10, grains: 2 },
    }
}

/// Pas sur une matière
pub fn footstep(material: Material, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x5157);
    let tb = timbre(material);
    let n = samples(tb.length);
    let mut out = vec![0.0; n];
    let mut band = BandPass::new();
    let mut low = OnePole::new();
    // Les bandes larges laissent passer les aigus : sans ce filtre, le pas siffle
    let mut soft = OnePole::new();
    // Grains décalés de quelques millisecondes : un pas n'est pas un seul impact
    let offsets: Vec<f32> = (0..tb.grains).map(|i| i as f32 * (0.012 + 0.010 * rng.unit())).collect();
    let centre = tb.centre * (0.85 + 0.3 * rng.unit());
    let thump_hz = tb.thump_hz * (0.9 + 0.2 * rng.unit());
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let mut env = 0.0;
        for (k, &o) in offsets.iter().enumerate() {
            if t >= o {
                env += envelope(t - o, 0.002, tb.decay) / (1.0 + k as f32 * 0.6);
            }
        }
        let sweep = if material == Material::Water { 1.0 + 1.5 * (t / tb.length) } else { 1.0 };
        let hiss = soft.low(band.process(rng.next(), centre * sweep, tb.q), centre * 2.5) * env;
        let body = (TAU * thump_hz * t).sin() * envelope(t, 0.003, tb.decay * 1.2) * tb.thump;
        *s = hiss + low.low(body, 400.0);
    }
    fade_tail(&mut out, 0.01);
    normalize(out, 0.8)
}

/// Bloc cassé : éclats de la matière et chute de débris
pub fn block_break(material: Material, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0xB4EA);
    let tb = timbre(material);
    let length = 0.32;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut band = BandPass::new();
    let mut low = OnePole::new();
    let mut soft = OnePole::new();
    // Impact principal puis débris de plus en plus clairsemés
    let mut hits: Vec<(f32, f32)> = vec![(0.0, 1.0)];
    let mut t = 0.03;
    while t < length - 0.05 {
        hits.push((t, 0.25 + 0.35 * rng.unit()));
        t += 0.02 + 0.05 * rng.unit();
    }
    let crack = material == Material::Stone || material == Material::Wood;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let env: f32 = hits
            .iter()
            .filter(|(o, _)| t >= *o)
            .map(|(o, a)| a * envelope(t - o, 0.001, tb.decay * if crack { 0.8 } else { 1.3 }))
            .sum::<f32>()
            * (1.0 - t / length);
        let hiss = soft.low(band.process(rng.next(), tb.centre * 0.8, tb.q * 0.8), tb.centre * 3.0) * env;
        let body = (TAU * tb.thump_hz * 0.7 * t).sin() * envelope(t, 0.002, 0.06) * (0.4 + tb.thump);
        *s = hiss + low.low(body, 500.0);
    }
    fade_tail(&mut out, 0.02);
    normalize(out, 0.85)
}

/// Bloc posé : coup bref et mat, la hauteur dépend de la matière
pub fn block_place(material: Material, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x91AC);
    let tb = timbre(material);
    let length = 0.14;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut band = BandPass::new();
    let base = tb.thump_hz * 1.4 * (0.95 + 0.1 * rng.unit());
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        // Légère chute de hauteur, comme un objet qui se cale
        let f = base * (1.0 + 0.6 * (-t / 0.01).exp());
        let knock = (TAU * f * t).sin() * envelope(t, 0.001, 0.035);
        let click = band.process(rng.next(), tb.centre, tb.q) * envelope(t, 0.0005, 0.008);
        *s = knock + 0.6 * click;
    }
    fade_tail(&mut out, 0.01);
    normalize(out, 0.8)
}

/// Souffle d'un saut : bruit dont la bande monte rapidement
pub fn jump_whoosh(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x7A3F);
    let length = 0.22;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut band = BandPass::new();
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let x = t / length;
        let env = (x * std::f32::consts::PI).sin().powf(1.5);
        *s = band.process(rng.next(), 500.0 + 1800.0 * x, 1.2) * env;
    }
    normalize(out, 0.5)
}

/// Coup sourd de la réception, ajouté au pas de la matière
pub fn landing_thud(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x1A4D);
    let length = 0.25;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut low = OnePole::new();
    let f0 = 65.0 * (0.9 + 0.2 * rng.unit());
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let f = f0 * (1.0 + 0.8 * (-t / 0.02).exp());
        let body = (TAU * f * t).sin() * envelope(t, 0.002, 0.07);
        *s = body + low.low(rng.next(), 300.0) * envelope(t, 0.001, 0.03) * 2.0;
    }
    fade_tail(&mut out, 0.02);
    normalize(out, 0.9)
}

/// Souffle coupé d'une blessure (« oumpf ») : voix grave qui retombe, à travers deux formants
pub fn hurt_grunt(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x4E27);
    let length = 0.3;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut low_formant = BandPass::new();
    let mut high_formant = BandPass::new();
    let mut breath = BandPass::new();
    let f0 = 150.0 * (0.9 + 0.2 * rng.unit());
    let mut phase = 0.0_f32;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        // La voix retombe d'un tiers en quelques dixièmes de seconde
        let f = f0 * (1.0 - 0.3 * (t / length));
        phase = (phase + f / SAMPLE_RATE as f32).fract();
        let saw = 2.0 * phase - 1.0;
        let env = envelope(t, 0.012, 0.09);
        let voice = low_formant.process(saw, 650.0, 2.0) + 0.6 * high_formant.process(saw, 1100.0, 3.0);
        let air = breath.process(rng.next(), 1400.0, 0.8) * 0.25;
        *s = (voice + air) * env;
    }
    fade_tail(&mut out, 0.03);
    normalize(out, 0.75)
}

/// Bouchées croquées : trois craquements secs et étouffés, de plus en plus faibles
pub fn eat_crunch(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0xEA7);
    let length = 0.6;
    let n = samples(length);
    let mut out = vec![0.0; n];
    let mut band = BandPass::new();
    let mut soft = OnePole::new();
    let bites: Vec<(f32, f32)> = (0..3).map(|i| (i as f32 * (0.17 + 0.03 * rng.unit()), 1.0 - 0.25 * i as f32)).collect();
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let env: f32 = bites.iter().filter(|(o, _)| t >= *o).map(|(o, a)| a * envelope(t - o, 0.002, 0.035)).sum();
        // Grain du croquant : bruit en bande médium, adouci par la bouche fermée
        let crunch = soft.low(band.process(rng.next(), 1500.0 + 600.0 * rng.unit(), 0.9), 3500.0);
        *s = crunch * env;
    }
    fade_tail(&mut out, 0.03);
    normalize(out, 0.6)
}

/// Vent en boucle : bruit filtré dont le timbre et le volume ondulent en rafales.
/// La fin se fond dans le début : la boucle n'a pas de raccord audible.
pub fn wind_loop(seconds: f32, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x3D11);
    let n = samples(seconds);
    let fade = samples(1.0).min(n / 4);
    let total = n + fade;
    let mut raw = vec![0.0; total];
    let mut low = OnePole::new();
    let mut low2 = OnePole::new();
    let mut band = BandPass::new();
    // Rafales : sommes de sinus lents aux périodes incommensurables, ramenées à la durée de la boucle
    let cycles = [1.0, 2.0, 3.0, 5.0].map(|c| c * TAU / seconds);
    let phases = [rng.unit(), rng.unit(), rng.unit(), rng.unit()].map(|p| p * TAU);
    for (i, s) in raw.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let gust = cycles.iter().zip(phases).map(|(w, p)| (w * t + p).sin()).sum::<f32>() * 0.25;
        let g = 0.55 + 0.45 * gust;
        let rumble = low2.low(low.low(rng.next(), 180.0 + 220.0 * g), 300.0) * 3.0;
        let whistle = band.process(rng.next(), 500.0 + 500.0 * g, 3.0) * 0.35 * g * g;
        *s = (rumble + whistle) * (0.4 + 0.6 * g);
    }
    // Fondu enchaîné : les `fade` échantillons qui suivent la fin recouvrent le début
    let mut out = raw[..n].to_vec();
    for i in 0..fade {
        let k = i as f32 / fade as f32;
        out[i] = out[i] * k + raw[n + i] * (1.0 - k);
    }
    normalize(out, 0.6)
}

/// Murmure de l'éclat de la Pierre du Néant, en boucle : un souffle chuchoté (bruit passé dans
/// deux formants de voyelle, haché en syllabes qui ne disent rien) sur un bourdon grave et
/// dissonant qui ondule. La fin se fond dans le début.
pub fn whisper_loop(seconds: f32, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x5EC7);
    let n = samples(seconds);
    let fade = samples(1.0).min(n / 4);
    let total = n + fade;
    let mut raw = vec![0.0; total];
    let (mut f1, mut f2, mut air) = (BandPass::new(), BandPass::new(), OnePole::new());
    // Syllabes : début, durée, formants (Hz)
    let mut syllables = Vec::new();
    let mut t = 0.2;
    while t < seconds + 1.0 {
        let length = 0.12 + 0.28 * rng.unit();
        let vowel = [(700.0, 1200.0), (350.0, 2300.0), (450.0, 900.0), (300.0, 800.0), (550.0, 1800.0)][(rng.unit() * 5.0) as usize % 5];
        syllables.push((t, length, vowel));
        // Des mots, puis des silences
        t += length + if rng.unit() < 0.25 { 0.6 + 0.9 * rng.unit() } else { 0.04 + 0.08 * rng.unit() };
    }
    let mut current = 0;
    for (i, s) in raw.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        while current + 1 < syllables.len() && syllables[current].0 + syllables[current].1 < t {
            current += 1;
        }
        let (start, length, (a, b)) = syllables[current];
        let local = ((t - start) / length).clamp(0.0, 1.0);
        let env = if t < start { 0.0 } else { (local * std::f32::consts::PI).sin().powf(0.6) };
        let noise = rng.next();
        let voice = f1.process(noise, a, 6.0) * 0.8 + f2.process(noise, b, 8.0) * 0.5 + (noise - air.low(noise, 4000.0)) * 0.15;
        // Bourdon : quinte un peu fausse, qui respire lentement (périodes ramenées à la boucle)
        let breath = 0.6 + 0.4 * (TAU * 2.0 * t / seconds).sin();
        let hum = ((TAU * 98.0 * t).sin() + 0.6 * (TAU * 146.0 * t).sin() + 0.3 * (TAU * 207.5 * t).sin()) * 0.05 * breath;
        *s = voice * env + hum;
    }
    let mut out = raw[..n].to_vec();
    for i in 0..fade {
        let k = i as f32 / fade as f32;
        out[i] = out[i] * k + raw[n + i] * (1.0 - k);
    }
    normalize(out, 0.55)
}

/// Pluie en boucle : un souffle doux (bruit filtré) semé de petits impacts de gouttes. Comme le
/// vent, la fin se fond dans le début.
pub fn rain_loop(seconds: f32, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x4A1F);
    let n = samples(seconds);
    let fade = samples(1.0).min(n / 4);
    let total = n + fade;
    let mut raw = vec![0.0; total];
    let mut hiss = BandPass::new();
    let mut body = OnePole::new();
    let mut drop_env = 0.0_f32;
    let mut drop_band = BandPass::new();
    let mut drop_centre = 3000.0_f32;
    for s in raw.iter_mut() {
        let noise = rng.next();
        let wash = hiss.process(noise, 5200.0, 0.5) * 0.5 + body.low(noise, 900.0) * 0.6;
        // Gouttes : environ 120 par seconde, chacune un claquement bref à sa hauteur
        if rng.unit() < 120.0 / SAMPLE_RATE as f32 {
            drop_env = 0.4 + 0.6 * rng.unit();
            drop_centre = 1800.0 + 4500.0 * rng.unit();
        }
        drop_env *= 0.9965;
        let drop = drop_band.process(rng.next(), drop_centre, 4.0) * drop_env;
        *s = wash + drop * 0.9;
    }
    let mut out = raw[..n].to_vec();
    for i in 0..fade {
        let k = i as f32 / fade as f32;
        out[i] = out[i] * k + raw[n + i] * (1.0 - k);
    }
    normalize(out, 0.55)
}

/// Coup de tonnerre : un craquement bref puis un grondement grave qui roule et s'éteint
pub fn thunder(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0x7D3E);
    let length = 4.5 + 1.5 * rng.unit();
    let n = samples(length);
    let mut low = OnePole::new();
    let mut low2 = OnePole::new();
    let mut crack = BandPass::new();
    // Roulements : quelques bouffées décalées
    let rolls: Vec<(f32, f32)> = (0..4).map(|i| (0.25 + i as f32 * (0.5 + 0.6 * rng.unit()), 0.5 + 0.5 * rng.unit())).collect();
    let mut out = vec![0.0; n];
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        let noise = rng.next();
        let rumble = low2.low(low.low(noise, 120.0), 70.0) * 9.0;
        let roll = rolls.iter().map(|&(at, gain)| if t >= at { gain * (-(t - at) / 0.9).exp() } else { 0.0 }).sum::<f32>();
        let swell = envelope(t, 0.15, 1.6);
        let snap = crack.process(noise, 2400.0, 0.7) * envelope(t, 0.005, 0.12) * 0.8;
        *s = rumble * (0.5 * swell + 0.6 * roll) + snap;
    }
    fade_tail(&mut out, 0.8);
    normalize(out, 0.9)
}

/// Chant d'oiseau : quelques notes sifflées qui glissent, avec un léger trémolo
pub fn bird_song(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0xB12D);
    let notes = 2 + (rng.unit() * 4.0) as usize;
    let base = 2600.0 + 1800.0 * rng.unit();
    let mut out = Vec::new();
    for _ in 0..notes {
        let length = 0.06 + 0.10 * rng.unit();
        let (from, to) = (base * (0.8 + 0.5 * rng.unit()), base * (0.8 + 0.5 * rng.unit()));
        let n = samples(length);
        let mut phase = 0.0_f32;
        for i in 0..n {
            let x = i as f32 / n as f32;
            let t = i as f32 / SAMPLE_RATE as f32;
            let f = from + (to - from) * x + 120.0 * (TAU * 30.0 * t).sin();
            phase += TAU * f / SAMPLE_RATE as f32;
            let env = (x * std::f32::consts::PI).sin();
            out.push(phase.sin() * env * env);
        }
        out.extend(std::iter::repeat_n(0.0, samples(0.03 + 0.08 * rng.unit())));
    }
    normalize(out, 0.5)
}

/// Grillons la nuit : trains d'impulsions aiguës
pub fn cricket_chirps(seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed ^ 0xC41C);
    let freq = 4200.0 + 900.0 * rng.unit();
    let trains = 2 + (rng.unit() * 3.0) as usize;
    let mut out = Vec::new();
    for _ in 0..trains {
        let pulses = 3 + (rng.unit() * 3.0) as usize;
        for _ in 0..pulses {
            let n = samples(0.018);
            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = (i as f32 / n as f32 * std::f32::consts::PI).sin();
                out.push((TAU * freq * t).sin() * env);
            }
            out.extend(std::iter::repeat_n(0.0, samples(0.012)));
        }
        out.extend(std::iter::repeat_n(0.0, samples(0.18 + 0.15 * rng.unit())));
    }
    normalize(out, 0.35)
}

/// Fondu de sortie sur les derniers `seconds` : pas de clic en fin de son
fn fade_tail(buf: &mut [f32], seconds: f32) {
    let n = samples(seconds).min(buf.len());
    let len = buf.len();
    for i in 0..n {
        buf[len - n + i] *= 1.0 - i as f32 / n as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(buf: &[f32], what: &str) {
        assert!(!buf.is_empty(), "{what} : son vide");
        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{what} : échantillon hors de [-1, 1]");
        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.01, "{what} : son quasi muet (rms {rms})");
    }

    #[test]
    fn test_every_sound_is_audible_and_bounded() {
        for m in Material::ALL {
            for seed in 0..3 {
                check(&footstep(m, seed), &format!("pas {m:?}"));
                check(&block_break(m, seed), &format!("bloc cassé {m:?}"));
                check(&block_place(m, seed), &format!("bloc posé {m:?}"));
            }
        }
        check(&jump_whoosh(1), "saut");
        check(&landing_thud(1), "réception");
        check(&bird_song(1), "oiseau");
        check(&cricket_chirps(1), "grillons");
        check(&wind_loop(4.0, 1), "vent");
    }

    #[test]
    fn test_short_sounds_end_in_silence() {
        for buf in [footstep(Material::Stone, 0), block_break(Material::Dirt, 0), block_place(Material::Wood, 0), landing_thud(0)] {
            assert!(buf.last().unwrap().abs() < 1e-3, "un son doit finir sans clic");
        }
    }

    #[test]
    fn test_wind_loop_has_no_seam() {
        let wind = wind_loop(4.0, 7);
        let (last, first) = (wind[wind.len() - 1], wind[0]);
        // Saut au raccord comparable à l'écart moyen entre deux échantillons voisins
        let mean_step = wind.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / wind.len() as f32;
        assert!((last - first).abs() < mean_step * 8.0, "raccord de la boucle audible : {} contre {}", (last - first).abs(), mean_step);
    }

    #[test]
    fn test_variants_differ_and_are_deterministic() {
        assert_eq!(footstep(Material::Grass, 3), footstep(Material::Grass, 3));
        assert_ne!(footstep(Material::Grass, 3), footstep(Material::Grass, 4));
        assert_ne!(footstep(Material::Grass, 3), footstep(Material::Stone, 3));
    }

    #[test]
    fn test_blocks_map_to_materials() {
        use herocraft_common::BlockId;
        assert_eq!(Material::of(BlockId::GRASS), Material::Grass);
        assert_eq!(Material::of(BlockId::IRON_ORE), Material::Stone);
        assert_eq!(Material::of(BlockId::WATER), Material::Water);
        assert_eq!(Material::of(BlockId::WOOD), Material::Wood);
    }
}

//! Profileur instrumenté léger (feature cargo `profiling`).
//!
//! Sans la feature, `prof_scope!`, `prof_value!` et les fonctions de ce module ne font rien
//! et sont entièrement éliminés à la compilation.
//!
//! Avec la feature, chaque portée mesurée enregistre par thread : nombre d'appels, temps
//! inclusif, temps propre (hors sous-portées), histogramme des durées (p50/p95/p99) et max.
//! Les frames sont chronométrées via `frame_mark()`, les pires frames sont décomposées,
//! et une trace chronologique (format Chrome / Perfetto) est écrite pour les portées longues.

/// Point de mesure statique (un par appel de macro).
pub struct Site {
    pub name: &'static str,
    #[cfg(feature = "profiling")]
    id: std::sync::atomic::AtomicU32,
}

impl Site {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            #[cfg(feature = "profiling")]
            id: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

/// Mesure le temps passé jusqu'à la fin du bloc courant.
#[macro_export]
macro_rules! prof_scope {
    ($name:expr) => {
        let _prof_guard = {
            static PROF_SITE: $crate::profiler::Site = $crate::profiler::Site::new($name);
            $crate::profiler::Scope::enter(&PROF_SITE)
        };
    };
    // Variante nommée, pour terminer la mesure explicitement avec `drop(guard)`
    ($name:expr => $guard:ident) => {
        let $guard = {
            static PROF_SITE: $crate::profiler::Site = $crate::profiler::Site::new($name);
            $crate::profiler::Scope::enter(&PROF_SITE)
        };
    };
}

/// Enregistre une valeur numérique (nombre de sommets, de feuilles, etc.). `$name` doit être un littéral.
#[macro_export]
macro_rules! prof_value {
    ($name:expr, $value:expr) => {{
        static PROF_SITE: $crate::profiler::Site = $crate::profiler::Site::new(concat!("#", $name));
        $crate::profiler::record_value(&PROF_SITE, $value as u64);
    }};
}

#[cfg(not(feature = "profiling"))]
mod imp {
    use super::Site;

    pub struct Scope;

    impl Scope {
        #[inline(always)]
        pub fn enter(_site: &'static Site) -> Self {
            Scope
        }
    }

    #[inline(always)]
    pub fn record_value(_site: &'static Site, _value: u64) {}

    pub fn is_enabled() -> bool {
        false
    }
    pub fn init_main_thread() {}
    pub fn set_meta(_key: &str, _value: String) {}
    #[inline(always)]
    pub fn frame_mark() {}
    pub fn write_report() -> Option<std::path::PathBuf> {
        None
    }
    pub fn write_report_to(_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(feature = "profiling")]
mod imp {
    use super::Site;
    use std::cell::Cell;
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Instant;

    /// Histogramme logarithmique : 4 sous-intervalles par puissance de 2 (précision ~±12 %).
    const BUCKETS: usize = 256;
    /// Les portées plus courtes ne vont pas dans la trace chronologique (elles restent dans les stats).
    const TRACE_MIN_NS: u64 = 50_000;
    const TRACE_MAX_EVENTS_PER_THREAD: usize = 400_000;
    /// Profondeur max et durée min des portées gardées pour décomposer les pires frames.
    const FRAME_CAPTURE_MAX_DEPTH: u32 = 4;
    const FRAME_CAPTURE_MIN_NS: u64 = 5_000;
    const WORST_FRAMES_KEPT: usize = 12;

    struct SiteStats {
        count: u64,
        total_ns: u64,
        self_ns: u64,
        max_ns: u64,
        hist: Box<[u32; BUCKETS]>,
    }

    impl SiteStats {
        fn new() -> Self {
            Self { count: 0, total_ns: 0, self_ns: 0, max_ns: 0, hist: Box::new([0; BUCKETS]) }
        }

        fn merge(&mut self, other: &SiteStats) {
            self.count += other.count;
            self.total_ns += other.total_ns;
            self.self_ns += other.self_ns;
            self.max_ns = self.max_ns.max(other.max_ns);
            for (a, b) in self.hist.iter_mut().zip(other.hist.iter()) {
                *a += *b;
            }
        }

        fn percentile(&self, p: f64) -> u64 {
            let target = ((self.count as f64) * p).ceil().max(1.0) as u64;
            let mut acc = 0u64;
            for (i, &c) in self.hist.iter().enumerate() {
                acc += c as u64;
                if acc >= target {
                    return bucket_mid(i).min(self.max_ns);
                }
            }
            self.max_ns
        }
    }

    #[inline]
    fn bucket_of(v: u64) -> usize {
        if v < 8 {
            return v as usize;
        }
        let e = 63 - v.leading_zeros() as usize; // e >= 3
        let sub = ((v >> (e - 2)) & 3) as usize;
        ((e - 1) * 4 + sub).min(BUCKETS - 1)
    }

    fn bucket_mid(i: usize) -> u64 {
        if i < 8 {
            return i as u64;
        }
        let e = i / 4 + 1;
        let sub = (i % 4) as u64;
        let low = (1u64 << e) + sub * (1u64 << (e - 2));
        low + (1u64 << (e - 2)) / 2
    }

    struct TraceEvent {
        site: u32,
        start_ns: u64,
        dur_ns: u64,
    }

    struct FrameEntry {
        site: u32,
        depth: u32,
        start_ns: u64,
        dur_ns: u64,
    }

    struct ThreadData {
        name: String,
        tid: u64,
        stats: Vec<Option<SiteStats>>,
        trace: Vec<TraceEvent>,
        trace_dropped: u64,
        frame_capture: Vec<FrameEntry>,
    }

    struct Frame {
        index: u64,
        dur_ns: u64,
        entries: Vec<FrameEntry>,
    }

    struct Global {
        start: Instant,
        sites: Mutex<Vec<&'static Site>>,
        threads: Mutex<Vec<Arc<Mutex<ThreadData>>>>,
        frames_us: Mutex<Vec<u32>>,
        worst_frames: Mutex<Vec<Frame>>,
        meta: Mutex<Vec<(String, String)>>,
        next_tid: AtomicU32,
    }

    fn global() -> &'static Global {
        static G: OnceLock<Global> = OnceLock::new();
        G.get_or_init(|| Global {
            start: Instant::now(),
            sites: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            frames_us: Mutex::new(Vec::with_capacity(1 << 16)),
            worst_frames: Mutex::new(Vec::new()),
            meta: Mutex::new(Vec::new()),
            next_tid: AtomicU32::new(1),
        })
    }

    thread_local! {
        static LOCAL: Arc<Mutex<ThreadData>> = register_thread();
        static DEPTH: Cell<u32> = const { Cell::new(0) };
        static CHILD_NS: Cell<u64> = const { Cell::new(0) };
        static IS_FRAME_THREAD: Cell<bool> = const { Cell::new(false) };
        static LAST_FRAME: Cell<Option<Instant>> = const { Cell::new(None) };
        static FRAME_INDEX: Cell<u64> = const { Cell::new(0) };
    }

    fn register_thread() -> Arc<Mutex<ThreadData>> {
        let g = global();
        let name = std::thread::current().name().unwrap_or("sans-nom").to_string();
        let data = Arc::new(Mutex::new(ThreadData {
            name,
            tid: g.next_tid.fetch_add(1, Ordering::Relaxed) as u64,
            stats: Vec::new(),
            trace: Vec::new(),
            trace_dropped: 0,
            frame_capture: Vec::new(),
        }));
        g.threads.lock().unwrap().push(data.clone());
        data
    }

    #[inline]
    fn site_id(site: &'static Site) -> u32 {
        let id = site.id.load(Ordering::Relaxed);
        if id != 0 {
            return id;
        }
        let mut sites = global().sites.lock().unwrap();
        let id = site.id.load(Ordering::Relaxed);
        if id != 0 {
            return id;
        }
        sites.push(site);
        let id = sites.len() as u32;
        site.id.store(id, Ordering::Relaxed);
        id
    }

    pub struct Scope {
        site: &'static Site,
        start: Instant,
        saved_child_ns: u64,
    }

    impl Scope {
        #[inline]
        pub fn enter(site: &'static Site) -> Self {
            DEPTH.with(|d| d.set(d.get() + 1));
            let saved_child_ns = CHILD_NS.with(|c| c.replace(0));
            Scope { site, start: Instant::now(), saved_child_ns }
        }
    }

    impl Drop for Scope {
        #[inline]
        fn drop(&mut self) {
            let end = Instant::now();
            let dur = end.duration_since(self.start).as_nanos() as u64;
            let children = CHILD_NS.with(|c| c.replace(self.saved_child_ns + dur));
            let depth = DEPTH.with(|d| {
                let v = d.get();
                d.set(v - 1);
                v
            });
            let id = site_id(self.site);
            LOCAL.with(|local| {
                let mut t = local.lock().unwrap();
                let idx = id as usize;
                if t.stats.len() <= idx {
                    t.stats.resize_with(idx + 1, || None);
                }
                let s = t.stats[idx].get_or_insert_with(SiteStats::new);
                s.count += 1;
                s.total_ns += dur;
                s.self_ns += dur.saturating_sub(children);
                s.max_ns = s.max_ns.max(dur);
                s.hist[bucket_of(dur)] += 1;

                if dur >= TRACE_MIN_NS || depth <= 1 {
                    let start_ns = self.start.duration_since(global().start).as_nanos() as u64;
                    if t.trace.len() < TRACE_MAX_EVENTS_PER_THREAD {
                        t.trace.push(TraceEvent { site: id, start_ns, dur_ns: dur });
                    } else {
                        t.trace_dropped += 1;
                    }
                }
                if depth <= FRAME_CAPTURE_MAX_DEPTH
                    && dur >= FRAME_CAPTURE_MIN_NS
                    && IS_FRAME_THREAD.with(|f| f.get())
                {
                    let start_ns = self.start.duration_since(global().start).as_nanos() as u64;
                    t.frame_capture.push(FrameEntry { site: id, depth, start_ns, dur_ns: dur });
                }
            });
        }
    }

    pub fn record_value(site: &'static Site, value: u64) {
        let id = site_id(site);
        LOCAL.with(|local| {
            let mut t = local.lock().unwrap();
            let idx = id as usize;
            if t.stats.len() <= idx {
                t.stats.resize_with(idx + 1, || None);
            }
            let s = t.stats[idx].get_or_insert_with(SiteStats::new);
            s.count += 1;
            s.total_ns += value;
            s.max_ns = s.max_ns.max(value);
            s.hist[bucket_of(value)] += 1;
        });
    }

    static VALUE_PREFIX: &str = "#";

    pub fn is_enabled() -> bool {
        true
    }

    /// À appeler au début de `main` : démarre l'horloge et désigne le thread des frames.
    pub fn init_main_thread() {
        let _ = global();
        IS_FRAME_THREAD.with(|f| f.set(true));
        set_meta("os", format!("{} {}", std::env::consts::OS, std::env::consts::ARCH));
        set_meta(
            "threads_cpu",
            std::thread::available_parallelism().map(|p| p.get().to_string()).unwrap_or_default(),
        );
        set_meta("build", if cfg!(debug_assertions) { "DEBUG (non optimisé !)".into() } else { "release".into() });
    }

    pub fn set_meta(key: &str, value: String) {
        let mut meta = global().meta.lock().unwrap();
        if let Some(entry) = meta.iter_mut().find(|(k, _)| k == key) {
            entry.1 = value;
        } else {
            meta.push((key.to_string(), value));
        }
    }

    /// À appeler une fois par frame, à la fin, sur le thread principal.
    pub fn frame_mark() {
        let now = Instant::now();
        let prev = LAST_FRAME.with(|l| l.replace(Some(now)));
        let index = FRAME_INDEX.with(|i| {
            let v = i.get();
            i.set(v + 1);
            v
        });
        let entries = LOCAL.with(|local| std::mem::take(&mut local.lock().unwrap().frame_capture));
        let Some(prev) = prev else { return };
        let dur_ns = now.duration_since(prev).as_nanos() as u64;
        let g = global();
        g.frames_us.lock().unwrap().push((dur_ns / 1000).min(u32::MAX as u64) as u32);

        let mut worst = g.worst_frames.lock().unwrap();
        let min_kept = worst.iter().map(|f| f.dur_ns).min().unwrap_or(0);
        if worst.len() < WORST_FRAMES_KEPT || dur_ns > min_kept {
            worst.push(Frame { index, dur_ns, entries });
            worst.sort_by(|a, b| b.dur_ns.cmp(&a.dur_ns));
            worst.truncate(WORST_FRAMES_KEPT);
        }
    }

    const CALIBRATION_THREAD: &str = "profiler-calibration";

    /// Coût moyen d'une portée vide (pour estimer la perturbation de la mesure).
    /// Exécuté sur un thread dédié, exclu du rapport et de la trace.
    fn calibrate() -> f64 {
        static CAL: Site = Site::new("__calibration");
        std::thread::Builder::new()
            .name(CALIBRATION_THREAD.into())
            .spawn(|| {
                let n = 200_000;
                let start = Instant::now();
                for _ in 0..n {
                    let _g = Scope::enter(&CAL);
                }
                start.elapsed().as_nanos() as f64 / n as f64
            })
            .ok()
            .and_then(|h| h.join().ok())
            .unwrap_or(f64::NAN)
    }

    fn fmt_ns(ns: f64) -> String {
        if ns >= 1e9 {
            format!("{:.2} s", ns / 1e9)
        } else if ns >= 1e6 {
            format!("{:.2} ms", ns / 1e6)
        } else if ns >= 1e3 {
            format!("{:.1} µs", ns / 1e3)
        } else {
            format!("{:.0} ns", ns)
        }
    }

    /// Regroupe les threads par nom sans le numéro final (herocraft-worker-3 -> herocraft-worker-*).
    fn thread_group(name: &str) -> String {
        let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
        if trimmed.len() != name.len() {
            format!("{}*", trimmed)
        } else {
            name.to_string()
        }
    }

    fn json_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Écrit `profiling/rapport_<horodatage>.txt` et `profiling/trace_<horodatage>.json`.
    /// Renvoie le chemin du rapport texte.
    pub fn write_report() -> Option<PathBuf> {
        write_report_to(std::path::Path::new("profiling"))
    }

    pub fn write_report_to(dir: &std::path::Path) -> Option<PathBuf> {
        let overhead_ns = calibrate();
        let g = global();
        let wall_ns = g.start.elapsed().as_nanos() as f64;
        let sites: Vec<&'static Site> = g.sites.lock().unwrap().clone();
        let site_name = |id: u32| -> &'static str { sites.get(id as usize - 1).map(|s| s.name).unwrap_or("?") };
        let threads: Vec<_> = g
            .threads
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.lock().unwrap().name != CALIBRATION_THREAD)
            .cloned()
            .collect();

        // Agrégation par (groupe de threads, site)
        let mut groups: Vec<(String, usize, Vec<Option<SiteStats>>)> = Vec::new();
        for t in &threads {
            let t = t.lock().unwrap();
            let group = thread_group(&t.name);
            let pos = match groups.iter().position(|(n, _, _)| *n == group) {
                Some(p) => p,
                None => {
                    groups.push((group, 0, Vec::new()));
                    groups.len() - 1
                }
            };
            let entry = &mut groups[pos];
            entry.1 += 1;
            if entry.2.len() < t.stats.len() {
                entry.2.resize_with(t.stats.len(), || None);
            }
            for (i, s) in t.stats.iter().enumerate() {
                if let Some(s) = s {
                    entry.2[i].get_or_insert_with(SiteStats::new).merge(s);
                }
            }
        }

        let mut out = String::new();
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(out, "=== RAPPORT DE PROFILAGE HEROCRAFT ===");
        let _ = writeln!(out, "horodatage_unix: {}", unix);
        let _ = writeln!(out, "duree_session: {}", fmt_ns(wall_ns));
        for (k, v) in g.meta.lock().unwrap().iter() {
            let _ = writeln!(out, "{}: {}", k, v);
        }
        let _ = writeln!(out, "cout_mesure_par_portee: {:.0} ns", overhead_ns);

        // Frames
        let mut frames: Vec<u32> = g.frames_us.lock().unwrap().clone();
        let _ = writeln!(out, "\n--- FRAMES (temps mur entre deux frames) ---");
        if frames.is_empty() {
            let _ = writeln!(out, "aucune frame");
        } else {
            let n = frames.len();
            let sum: u64 = frames.iter().map(|&f| f as u64).sum();
            let mean = sum as f64 / n as f64;
            let over = |ms: u32| frames.iter().filter(|&&f| f > ms * 1000).count();
            let (o16, o33, o50, o100) = (over(16), over(33), over(50), over(100));
            frames.sort_unstable();
            let pct = |p: f64| frames[((n as f64 * p) as usize).min(n - 1)] as f64 * 1000.0;
            let _ = writeln!(out, "frames: {}  fps_moyen: {:.1}", n, 1e6 / mean);
            let _ = writeln!(
                out,
                "moyenne: {}  p50: {}  p95: {}  p99: {}  max: {}",
                fmt_ns(mean * 1000.0),
                fmt_ns(pct(0.50)),
                fmt_ns(pct(0.95)),
                fmt_ns(pct(0.99)),
                fmt_ns(frames[n - 1] as f64 * 1000.0)
            );
            let _ = writeln!(
                out,
                "frames > 16.7ms: {}  > 33ms: {}  > 50ms: {}  > 100ms: {}",
                o16, o33, o50, o100
            );
        }

        // Tables par groupe de threads, triées par temps propre
        for (group, nthreads, stats) in &groups {
            let mut rows: Vec<(u32, &SiteStats)> = stats
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.as_ref().map(|s| (i as u32, s)))
                .filter(|(id, _)| !site_name(*id).starts_with(VALUE_PREFIX))
                .collect();
            if rows.is_empty() {
                continue;
            }
            rows.sort_by(|a, b| b.1.self_ns.cmp(&a.1.self_ns));
            let _ = writeln!(out, "\n--- PORTEES : threads '{}' ({} thread(s)) — tri par temps propre ---", group, nthreads);
            let _ = writeln!(
                out,
                "{:<40} {:>10} {:>11} {:>11} {:>7} {:>10} {:>10} {:>10} {:>10} {:>10}",
                "portee", "appels", "total", "propre", "%mur", "moyenne", "p50", "p95", "p99", "max"
            );
            for (id, s) in rows {
                let _ = writeln!(
                    out,
                    "{:<40} {:>10} {:>11} {:>11} {:>6.1}% {:>10} {:>10} {:>10} {:>10} {:>10}",
                    site_name(id),
                    s.count,
                    fmt_ns(s.total_ns as f64),
                    fmt_ns(s.self_ns as f64),
                    s.self_ns as f64 / wall_ns / (*nthreads as f64) * 100.0,
                    fmt_ns(s.total_ns as f64 / s.count as f64),
                    fmt_ns(s.percentile(0.50) as f64),
                    fmt_ns(s.percentile(0.95) as f64),
                    fmt_ns(s.percentile(0.99) as f64),
                    fmt_ns(s.max_ns as f64),
                );
            }
        }

        // Valeurs
        let _ = writeln!(out, "\n--- VALEURS ---");
        let _ = writeln!(out, "{:<40} {:>10} {:>14} {:>12} {:>12} {:>12}", "valeur", "mesures", "moyenne", "p50", "p95", "max");
        let mut merged_values: Vec<(u32, SiteStats)> = Vec::new();
        for (_, _, stats) in &groups {
            for (i, s) in stats.iter().enumerate() {
                let Some(s) = s else { continue };
                if !site_name(i as u32).starts_with(VALUE_PREFIX) {
                    continue;
                }
                match merged_values.iter_mut().find(|(id, _)| *id == i as u32) {
                    Some((_, m)) => m.merge(s),
                    None => {
                        let mut m = SiteStats::new();
                        m.merge(s);
                        merged_values.push((i as u32, m));
                    }
                }
            }
        }
        merged_values.sort_by_key(|(id, _)| site_name(*id));
        for (id, s) in &merged_values {
            let _ = writeln!(
                out,
                "{:<40} {:>10} {:>14.1} {:>12} {:>12} {:>12}",
                site_name(*id).trim_start_matches(VALUE_PREFIX),
                s.count,
                s.total_ns as f64 / s.count.max(1) as f64,
                s.percentile(0.50),
                s.percentile(0.95),
                s.max_ns
            );
        }

        // Pires frames
        let _ = writeln!(out, "\n--- PIRES FRAMES (décomposition thread principal, portées >= 5 µs) ---");
        for f in g.worst_frames.lock().unwrap().iter() {
            let _ = writeln!(out, "frame #{} : {}", f.index, fmt_ns(f.dur_ns as f64));
            let mut entries: Vec<&FrameEntry> = f.entries.iter().collect();
            entries.sort_by_key(|e| (e.start_ns, e.depth));
            for e in entries {
                let _ = writeln!(
                    out,
                    "  {}{:<width$} {:>10}",
                    "  ".repeat(e.depth.saturating_sub(1) as usize),
                    site_name(e.site),
                    fmt_ns(e.dur_ns as f64),
                    width = 40usize.saturating_sub(2 * e.depth.saturating_sub(1) as usize)
                );
            }
        }

        if std::fs::create_dir_all(dir).is_err() {
            return None;
        }
        let report_path = dir.join(format!("rapport_{}.txt", unix));
        std::fs::write(&report_path, &out).ok()?;

        // Trace chronologique (chrome://tracing ou https://ui.perfetto.dev)
        let trace_path = dir.join(format!("trace_{}.json", unix));
        if let Ok(file) = std::fs::File::create(&trace_path) {
            let mut w = std::io::BufWriter::new(file);
            let _ = write!(w, "{{\"traceEvents\":[");
            let mut first = true;
            for t in &threads {
                let t = t.lock().unwrap();
                let _ = write!(
                    w,
                    "{}{{\"ph\":\"M\",\"name\":\"thread_name\",\"pid\":1,\"tid\":{},\"args\":{{\"name\":\"{}\"}}}}",
                    if first { "" } else { "," },
                    t.tid,
                    json_escape(&t.name)
                );
                first = false;
                for e in &t.trace {
                    let _ = write!(
                        w,
                        ",{{\"ph\":\"X\",\"name\":\"{}\",\"pid\":1,\"tid\":{},\"ts\":{:.3},\"dur\":{:.3}}}",
                        json_escape(site_name(e.site)),
                        t.tid,
                        e.start_ns as f64 / 1000.0,
                        e.dur_ns as f64 / 1000.0
                    );
                }
                if t.trace_dropped > 0 {
                    log::warn!("Profilage : {} événements de trace ignorés sur '{}' (limite atteinte)", t.trace_dropped, t.name);
                }
            }
            let _ = write!(w, "]}}");
            let _ = w.flush();
        }

        Some(report_path)
    }
}

pub use imp::*;

#[cfg(all(test, feature = "profiling"))]
mod tests {
    use super::*;

    #[test]
    fn test_nested_scopes_report() {
        for _ in 0..3 {
            crate::prof_scope!("test::parent");
            {
                crate::prof_scope!("test::child");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            crate::prof_value!("test.valeur", 42);
        }
        let dir = std::env::temp_dir().join(format!("herocraft_prof_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_report_to(&dir).expect("rapport écrit");
        let report = std::fs::read_to_string(&path).unwrap();
        let line = |name: &str| report.lines().find(|l| l.starts_with(name)).unwrap_or_else(|| panic!("{name} absent")).to_string();
        assert!(line("test::parent").split_whitespace().nth(1) == Some("3"));
        assert!(line("test::child").split_whitespace().nth(1) == Some("3"));
        assert!(line("test.valeur").contains("42"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

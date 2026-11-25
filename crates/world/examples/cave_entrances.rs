//! Compte les entrées de grottes visibles en surface sur des faces entières de la planète.
//!
//! Une colonne est « ouverte » si une grotte y débouche : soit par le haut (les deux blocs sous la
//! surface sont creusés), soit à flanc de pente (deux blocs creusés superposés à côté d'une colonne
//! voisine plus basse, donc à l'air libre). Les colonnes ouvertes voisines sont regroupées en
//! entrées. Seules les terres émergées comptent.
//!
//! Usage : cargo run -p herocraft_world --release --example cave_entrances -- [graine…]
//! (pas d'échantillonnage : chaque colonne de terre de la face est examinée)
use herocraft_common::PlanetFace;
use herocraft_world::{PlanetDescriptor, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};

/// Taille d'une face en blocs
const SIDE: i32 = 1024;

fn main() {
    let seeds: Vec<u64> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let seeds = if seeds.is_empty() { vec![42, 1, 7, 2024] } else { seeds };
    for seed in seeds {
        let desc = PlanetDescriptor { seed, ..Default::default() };
        let generator = PlanetWorldGenerator::new(desc);
        let results: Vec<(usize, usize, usize, usize)> = std::thread::scope(|s| {
            let handles: Vec<_> = PlanetFace::ALL
                .iter()
                .map(|&face| {
                    let generator = &generator;
                    s.spawn(move || scan_face(generator, face))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let (mut land, mut open, mut entrances, mut top_only) = (0, 0, 0, 0);
        for (f, r) in results.iter().enumerate() {
            println!("graine {seed} face {f} : {} colonnes de terre, {} entrées ({} par le haut)", r.0, r.2, r.3);
            land += r.0;
            open += r.1;
            entrances += r.2;
            top_only += r.3;
        }
        // Une colonne fait un bloc de côté : 1 km² = 1 000 000 de colonnes
        let per_km2 = entrances as f64 / (land as f64 / 1.0e6);
        println!(
            "graine {seed} TOTAL : {:.2} M colonnes de terre, {} entrées ({} par le haut), {:.1} entrées / km², \
             une tous les {:.0} blocs en moyenne, {:.4}% des colonnes ouvertes",
            land as f64 / 1e6,
            entrances,
            top_only,
            per_km2,
            (1.0e6 / per_km2.max(1e-9)).sqrt(),
            open as f64 / land.max(1) as f64 * 100.0
        );
    }
}

/// (colonnes de terre, colonnes ouvertes, entrées, entrées ouvertes par le haut)
fn scan_face(generator: &PlanetWorldGenerator, face: PlanetFace) -> (usize, usize, usize, usize) {
    let n = SIDE as usize;
    let mut ground = vec![0i32; n * n];
    let mut columns = Vec::with_capacity(n * n);
    for z in 0..SIDE {
        for x in 0..SIDE {
            let column = generator.sample_block_column(face, x, z);
            ground[z as usize * n + x as usize] = column.elevation.floor() as i32;
            columns.push(column);
        }
    }
    // 0 : fermée, 1 : ouverte à flanc, 2 : ouverte par le haut
    let mut open = vec![0u8; n * n];
    let mut land = 0;
    for z in 1..SIDE - 1 {
        for x in 1..SIDE - 1 {
            let i = z as usize * n + x as usize;
            let column = &columns[i];
            if column.elevation <= SEA_LEVEL_BLOCKS + 1.0 {
                continue;
            }
            land += 1;
            let top = ground[i];
            let cave = |y: i32| generator.is_cave(column, face, x, y, z);
            if cave(top - 1) && cave(top - 2) {
                open[i] = 2;
                continue;
            }
            // À flanc : la plus basse des voisines laisse à l'air libre les blocs au-dessus d'elle
            let lowest = [i - 1, i + 1, i - n, i + n].iter().map(|&j| ground[j]).min().unwrap();
            let lowest = lowest.max(SEA_LEVEL_BLOCKS.ceil() as i32);
            let mut previous = false;
            for y in lowest..top {
                let now = cave(y);
                if now && previous {
                    open[i] = 1;
                    break;
                }
                previous = now;
            }
        }
    }
    // Regroupement des colonnes ouvertes en entrées (8-voisinage)
    let mut seen = vec![false; n * n];
    let (mut entrances, mut from_top, mut opened) = (0, 0, 0);
    let mut big = Vec::new();
    for start in 0..n * n {
        if open[start] == 0 || seen[start] {
            continue;
        }
        entrances += 1;
        let mut any_top = false;
        let mut size = 0usize;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            opened += 1;
            size += 1;
            any_top |= open[i] == 2;
            let (x, z) = ((i % n) as i32, (i / n) as i32);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (nx, nz) = (x + dx, z + dz);
                    if nx < 0 || nz < 0 || nx >= SIDE || nz >= SIDE {
                        continue;
                    }
                    let j = nz as usize * n + nx as usize;
                    if open[j] != 0 && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        from_top += any_top as usize;
        if size >= 4 {
            big.push((start % n, start / n));
        }
    }
    // Couverture : part des carrés de 128 blocs presque tout en terre qui ont une entrée d'au moins 4 colonnes
    let tile = 128usize;
    let (mut tiles, mut covered) = (0, 0);
    for tz in 0..n / tile {
        for tx in 0..n / tile {
            let mut l = 0;
            for z in (tz * tile..(tz + 1) * tile).step_by(8) {
                for x in (tx * tile..(tx + 1) * tile).step_by(8) {
                    l += (columns[z * n + x].elevation > SEA_LEVEL_BLOCKS + 1.0) as usize;
                }
            }
            if l < 200 {
                continue;
            }
            tiles += 1;
            covered += big.iter().any(|&(x, z)| x / tile == tx && z / tile == tz) as usize;
        }
    }
    eprintln!("{face:?} : {} entrées >= 4 colonnes, {covered}/{tiles} carrés de 128 de terre en ont une", big.len());
    (land, opened, entrances, from_top)
}

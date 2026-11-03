//! Le peintre de héros contre la maquette, texel par texel : les cent planches de référence de
//! `tools/heros/maquette/references.mjs` (branche `develop`) — chaque sous-race par défaut, puis
//! soixante tirages semés — avec les configurations qui les ont peintes (`tests/legende/`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use herocraft_common::hero::{atlas, paint, Appearance, SHEET_SIZE};

fn dossier() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/legende")
}

fn configs() -> BTreeMap<String, Appearance> {
    let text = std::fs::read_to_string(dossier().join("configs.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn png(nom: &str) -> Vec<u8> {
    let decoder = png::Decoder::new(std::fs::File::open(dossier().join(format!("{nom}.png"))).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "{nom} : pas une planche RGBA");
    assert_eq!((info.width, info.height), (SHEET_SIZE as u32, SHEET_SIZE as u32));
    buf.truncate(info.buffer_size());
    buf
}

#[test]
fn every_reference_sheet_is_painted_texel_for_texel() {
    let configs = configs();
    assert_eq!(configs.len(), 100);
    let mut echecs = Vec::new();
    for (nom, cfg) in &configs {
        let planche = paint(cfg);
        let attendu = png(nom);
        // Un texel transparent n'a pas de couleur : seuls comptent ceux qu'on voit
        let ecart = (0..SHEET_SIZE * SHEET_SIZE).find(|&i| {
            let (a, b) = (&attendu[i * 4..i * 4 + 4], &planche.rgba[i * 4..i * 4 + 4]);
            a[3] != b[3] || (a[3] != 0 && a != b)
        });
        if let Some(i) = ecart {
            let (x, y) = (i % SHEET_SIZE, i / SHEET_SIZE);
            echecs.push(format!("{nom} : texel ({x}, {y}) attendu {:?}, peint {:?}", &attendu[i * 4..i * 4 + 4], planche.texel(x, y)));
        }
    }
    assert!(echecs.is_empty(), "{}/{} planches diffèrent :\n{}", echecs.len(), configs.len(), echecs.join("\n"));
}

/// Le générateur semé de la maquette, à la place de `Math.random`
struct Lcg(f64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = (self.0 * 1103515245.0 + 12345.0) % 2147483648.0;
        self.0 / 2147483648.0
    }
}

#[test]
fn random_appearances_replay_the_mock_ups_draws() {
    let configs = configs();
    let mut lcg = Lcg(12345.0);
    for n in 0..60 {
        let nom = format!("tirage-{n:02}");
        let mut attendu = configs[&nom].clone();
        let vu = atlas().aleatoire(&mut || lcg.next(), None);
        attendu.nom = vu.nom.clone();
        assert_eq!(vu, attendu, "{nom}");
    }
}

#[test]
fn defaults_keep_what_the_first_sub_race_imposes() {
    let configs = configs();
    for id in &atlas().ORDRE_ESPECES {
        let c = atlas().defaut(id);
        assert!(atlas().peaux_de(&c).contains(&c.peau), "{id}");
        assert_eq!(c, configs[&format!("defaut-{id}-0")], "{id}");
    }
}

//! Captures de l'interface sans carte graphique : les triangles produits par egui sont
//! rastérisés sur le processeur et enregistrés en PNG. Sert à comparer les menus avant et après
//! un changement de style (voir le test `ui::tests::snapshot_menus`).

use std::collections::HashMap;

struct Texture {
    size: [usize; 2],
    pixels: Vec<egui::Color32>,
    nearest: bool,
}

/// Image en cours de composition, et textures egui reçues au fil des images
pub struct Snapshot {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
    textures: HashMap<egui::TextureId, Texture>,
}

impl Snapshot {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![[0.0, 0.0, 0.0, 1.0]; width * height], textures: HashMap::new() }
    }

    /// Fond de la capture : une image PNG (mise à l'échelle au plus proche) ou un gris bleuté
    pub fn set_background(&mut self, path: Option<&std::path::Path>) {
        let fallback = [0.20_f32, 0.25, 0.32, 1.0];
        let Some(path) = path else {
            self.pixels.fill(fallback);
            return;
        };
        let Ok(file) = std::fs::File::open(path) else {
            self.pixels.fill(fallback);
            return;
        };
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().expect("fond illisible");
        let mut buf = vec![0_u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("fond illisible");
        let channels = info.color_type.samples();
        let (bw, bh) = (info.width as usize, info.height as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                let sx = x * bw / self.width;
                let sy = y * bh / self.height;
                let i = (sy * bw + sx) * channels;
                let px = &buf[i..i + channels];
                let (r, g, b) = if channels >= 3 { (px[0], px[1], px[2]) } else { (px[0], px[0], px[0]) };
                self.pixels[y * self.width + x] = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
            }
        }
    }
    /// Prend en compte les textures créées, modifiées ou libérées par egui
    /// Applique les textures reçues d'egui puis vide `delta` : egui (0.37 et plus) fait avorter
    /// le programme si un `TexturesDelta` non appliqué est libéré
    pub fn apply_textures(&mut self, delta: &mut egui::TexturesDelta) {
        for (id, image_delta) in delta.set.iter().flat_map(|(id, deltas)| deltas.iter().map(move |d| (id, d))) {
            let (size, pixels): ([usize; 2], Vec<egui::Color32>) = match &image_delta.image {
                egui::ImageData::Color(image) => (image.size, image.pixels.clone()),
            };
            let nearest = image_delta.options.magnification == egui::TextureFilter::Nearest;
            match image_delta.pos {
                None => {
                    self.textures.insert(*id, Texture { size, pixels, nearest });
                }
                Some([px, py]) => {
                    let texture = self.textures.get_mut(id).expect("mise à jour partielle d'une texture inconnue");
                    for y in 0..size[1] {
                        for x in 0..size[0] {
                            texture.pixels[(py + y) * texture.size[0] + px + x] = pixels[y * size[0] + x];
                        }
                    }
                }
            }
        }
        for id in &delta.free {
            self.textures.remove(id);
        }
        delta.clear();
    }

    /// Dessine les triangles d'une image egui par-dessus le fond
    pub fn paint(&mut self, primitives: &[egui::ClippedPrimitive], pixels_per_point: f32) {
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else { continue };
            let clip = primitive.clip_rect;
            let clip = [
                (clip.min.x * pixels_per_point).max(0.0) as i64,
                (clip.min.y * pixels_per_point).max(0.0) as i64,
                ((clip.max.x * pixels_per_point).ceil() as i64).min(self.width as i64),
                ((clip.max.y * pixels_per_point).ceil() as i64).min(self.height as i64),
            ];
            for tri in mesh.indices.chunks_exact(3) {
                let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
                self.triangle(v, mesh.texture_id, clip, pixels_per_point);
            }
        }
    }

    fn triangle(&mut self, v: [&egui::epaint::Vertex; 3], texture_id: egui::TextureId, clip: [i64; 4], ppp: f32) {
        let p = v.map(|v| (v.pos.x * ppp, v.pos.y * ppp));
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-6 {
            return;
        }
        let min_x = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i64).max(clip[0]);
        let max_x = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i64).min(clip[2]);
        let min_y = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i64).max(clip[1]);
        let max_y = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i64).min(clip[3]);
        let Some(texture) = self.textures.get(&texture_id) else { return };
        let colors = v.map(|v| v.color.to_array().map(|c| c as f32 / 255.0));
        for y in min_y..max_y {
            for x in min_x..max_x {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
                let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                    continue;
                }
                let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                let t = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                let texel = sample(texture, u, t);
                let mut src = [0.0_f32; 4];
                for c in 0..4 {
                    let vertex = w0 * colors[0][c] + w1 * colors[1][c] + w2 * colors[2][c];
                    src[c] = vertex * texel[c];
                }
                let dst = &mut self.pixels[y as usize * self.width + x as usize];
                // Couleurs prémultipliées, comme dans egui
                for c in 0..3 {
                    dst[c] = src[c] + dst[c] * (1.0 - src[3]);
                }
            }
        }
    }

    pub fn save(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("dossier des captures");
        }
        let file = std::fs::File::create(path).expect("capture impossible à écrire");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("en-tête PNG");
        let data: Vec<u8> =
            self.pixels.iter().flat_map(|p| [p[0], p[1], p[2]].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
        writer.write_image_data(&data).expect("données PNG");
    }
}

fn sample(texture: &Texture, u: f32, v: f32) -> [f32; 4] {
    let [w, h] = texture.size;
    let fetch = |x: i64, y: i64| {
        let x = x.clamp(0, w as i64 - 1) as usize;
        let y = y.clamp(0, h as i64 - 1) as usize;
        texture.pixels[y * w + x].to_array().map(|c| c as f32 / 255.0)
    };
    let fx = u * w as f32 - 0.5;
    let fy = v * h as f32 - 0.5;
    if texture.nearest {
        return fetch((u * w as f32).floor() as i64, (v * h as f32).floor() as i64);
    }
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (fetch(x0, y0), fetch(x0 + 1, y0), fetch(x0, y0 + 1), fetch(x0 + 1, y0 + 1));
    let mut out = [0.0_f32; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bottom = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bottom - top) * ty;
    }
    out
}

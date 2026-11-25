//! Icônes de l'inventaire : un bloc est dessiné en petit cube isométrique couvert de ses textures
//! (ses couleurs à défaut), une plante par sa texture à plat, un objet par son icône. Les images
//! sont lues à la première apparition de l'objet, puis gardées.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use herocraft_common::{blocks, items, BlockId, ItemId};

use herocraft_render::skinned_model::Texture;

/// Images déjà envoyées à egui, par fichier (aucune si le fichier est illisible)
#[derive(Default)]
pub struct ItemIcons {
    cache: RefCell<HashMap<PathBuf, Option<egui::TextureHandle>>>,
}

impl ItemIcons {
    fn texture(&self, ctx: &egui::Context, path: PathBuf) -> Option<egui::TextureId> {
        let mut cache = self.cache.borrow_mut();
        let handle = cache.entry(path).or_insert_with_key(|path| match Texture::load(path) {
            Ok(image) => {
                let pixels = egui::ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba);
                Some(ctx.load_texture(path.display().to_string(), pixels, egui::TextureOptions::NEAREST))
            }
            Err(e) => {
                log::warn!("Icône {} illisible : {e}", path.display());
                None
            }
        });
        handle.as_ref().map(egui::TextureHandle::id)
    }

    fn block_texture(&self, ctx: &egui::Context, file: &Option<String>) -> Option<egui::TextureId> {
        file.as_ref().and_then(|file| self.texture(ctx, blocks().asset_path(file)))
    }

    /// Dessine `item` dans `rect`
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, item: ItemId) {
        match item.block() {
            Some(block) => self.paint_block(painter, rect, block),
            None => {
                let icon = items().get(item).and_then(|sheet| self.texture(painter.ctx(), items().asset_path(&sheet.icon)));
                match icon {
                    Some(texture) => paint_flat(painter, rect.shrink(rect.width() * 0.16), texture, egui::Color32::WHITE),
                    None => {
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(18.0), egui::Color32::GRAY);
                    }
                }
            }
        }
    }

    fn paint_block(&self, painter: &egui::Painter, rect: egui::Rect, block: BlockId) {
        let Some(sheet) = blocks().get(block) else {
            return paint_cube(painter, rect, [None, None], [[0.6, 0.6, 0.6, 1.0]; 2], egui::Color32::WHITE);
        };
        let ctx = painter.ctx();
        let tint = sheet.texture_tint.map_or(egui::Color32::WHITE, |t| to_color(t.0, 1.0));
        let side = self.block_texture(ctx, &sheet.textures.side);
        if block.is_plant()
            && let Some(texture) = side {
            return paint_flat(painter, rect.shrink(rect.width() * 0.14), texture, tint);
        }
        let top = self.block_texture(ctx, &sheet.textures.top).or(side);
        paint_cube(painter, rect, [top, side], [sheet.top_color(), sheet.side_color()], tint);
    }
}

fn to_color(c: [f32; 4], shade: f32) -> egui::Color32 {
    let channel = |v: f32| (v * shade * 255.0).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(channel(c[0]), channel(c[1]), channel(c[2]))
}

/// Image entière, à plat dans `rect`
fn paint_flat(painter: &egui::Painter, rect: egui::Rect, texture: egui::TextureId, tint: egui::Color32) {
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(texture, rect, uv, tint);
}

/// Petit cube isométrique : dessus, face gauche et face droite, chacune couverte de sa texture
/// (`textures` : dessus, côtés) ou de sa couleur, assombrie selon son orientation
fn paint_cube(painter: &egui::Painter, rect: egui::Rect, textures: [Option<egui::TextureId>; 2], colors: [[f32; 4]; 2], tint: egui::Color32) {
    let c = rect.center();
    let s = rect.width() * 0.62_f32;
    let (hx, hy) = (s * 0.43_f32, s * 0.25_f32);
    let p = |dx: f32, dy: f32| egui::pos2(c.x + dx, c.y + dy);
    let faces = [
        ([p(0.0, -2.0 * hy), p(hx, -hy), p(0.0, 0.0), p(-hx, -hy)], 0, 1.05_f32),
        ([p(-hx, -hy), p(0.0, 0.0), p(0.0, 2.0 * hy), p(-hx, hy)], 1, 0.82_f32),
        ([p(0.0, 0.0), p(hx, -hy), p(hx, hy), p(0.0, 2.0 * hy)], 1, 0.64_f32),
    ];
    let outline = egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(120));
    for (corners, kind, shade) in faces {
        match textures[kind] {
            Some(texture) => {
                let mut mesh = egui::Mesh::with_texture(texture);
                let shaded = |v: u8| (v as f32 * shade.min(1.0)) as u8;
                let color = egui::Color32::from_rgb(shaded(tint.r()), shaded(tint.g()), shaded(tint.b()));
                let uvs = [egui::pos2(0.0, 0.0), egui::pos2(1.0, 0.0), egui::pos2(1.0, 1.0), egui::pos2(0.0, 1.0)];
                for (pos, uv) in corners.iter().zip(uvs) {
                    mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color });
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(mesh);
                painter.add(egui::Shape::closed_line(corners.to_vec(), outline));
            }
            None => {
                painter.add(egui::Shape::convex_polygon(corners.to_vec(), to_color(colors[kind], shade), outline));
            }
        }
    }
}

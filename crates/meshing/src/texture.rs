//! Image PNG décodée en RGBA 8 bits

use std::path::Path;

/// Texture RGBA 8 bits
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Texture {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        buffer.truncate(info.buffer_size());
        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::GrayscaleAlpha => buffer.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            other => return Err(format!("format de PNG non pris en charge : {other:?}")),
        };
        Ok(Self { width: info.width, height: info.height, rgba })
    }
}

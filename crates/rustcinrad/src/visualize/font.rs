use crate::RadarError::Output;
use crate::Result;
use font_kit::{
    canvas::{Canvas, Format, RasterizationOptions},
    font::Font,
    hinting::HintingOptions,
};
use pathfinder_geometry::{transform2d::Transform2F, vector::Vector2F};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

pub(super) const FAMILY: &str = "Noto Sans CJK SC";

const FONT_BYTES: &[u8] = include_bytes!("../../data/font/NotoSansHans-Regular.otf");
static FONT: LazyLock<Arc<Vec<u8>>> = LazyLock::new(|| Arc::new(FONT_BYTES.to_vec()));
static REGISTERED: LazyLock<Result<()>> = LazyLock::new(|| {
    plotters::style::register_font(FAMILY, plotters::style::FontStyle::Normal, FONT_BYTES)
        .map_err(|_| Output("invalid embedded Noto font".into()))
});

pub(super) fn register() -> Result<()> {
    REGISTERED.as_ref().map_err(|e| Output(e.to_string())).copied()
}

struct RasterGlyph {
    left: i32,
    top: i32,
    width: usize,
    stride: usize,
    advance: f32,
    pixels: Vec<u8>,
}

impl RasterGlyph {
    fn new(font: &Font, character: char, size: u32, scale: f32) -> Result<Self> {
        let glyph = font.glyph_for_char(character).unwrap_or(0);
        let bounds = font
            .raster_bounds(
                glyph,
                size as f32,
                Transform2F::default(),
                HintingOptions::None,
                RasterizationOptions::GrayscaleAa,
            )
            .map_err(|e| Output(e.to_string()))?;
        let mut raster = Self {
            left: bounds.origin().x(),
            top: bounds.origin().y(),
            width: bounds.width().max(0) as usize,
            stride: 0,
            advance: font.advance(glyph).map_err(|e| Output(e.to_string()))?.x() * scale,
            pixels: Vec::new(),
        };
        if bounds.width() > 0 && bounds.height() > 0 {
            let mut canvas = Canvas::new(bounds.size(), Format::A8);
            font.rasterize_glyph(
                &mut canvas,
                glyph,
                size as f32,
                Transform2F::from_translation(-bounds.origin().to_f32()),
                HintingOptions::None,
                RasterizationOptions::GrayscaleAa,
            )
            .map_err(|e| Output(e.to_string()))?;
            raster.stride = canvas.stride;
            raster.pixels = canvas.pixels;
        }
        Ok(raster)
    }
}

pub(super) fn labels(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    labels: &[(String, i32, i32)],
    size: u32,
    color: [u8; 3],
) -> Result<()> {
    let font = Font::from_bytes(Arc::clone(&FONT), 0).map_err(|e| Output(e.to_string()))?;
    let metrics = font.metrics();
    let scale = size as f32 / metrics.units_per_em as f32;
    let mut cache = HashMap::new();
    for (text, left, top) in labels {
        let mut pen = *left as f32;
        let baseline = *top as f32 + metrics.ascent * scale;
        for character in text.chars() {
            let glyph = match cache.entry(character) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(RasterGlyph::new(&font, character, size, scale)?)
                }
            };
            let origin = Vector2F::new(pen, baseline).to_i32();
            for (row, pixels) in glyph.pixels.chunks(glyph.stride.max(1)).enumerate() {
                for (column, &alpha) in pixels[..glyph.width].iter().enumerate() {
                    let x = origin.x() + glyph.left + column as i32;
                    let y = origin.y() + glyph.top + row as i32;
                    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                        continue;
                    }
                    let index = (y as usize * width as usize + x as usize) * 3;
                    let alpha = u32::from(alpha);
                    for channel in 0..3 {
                        rgb[index + channel] = ((u32::from(rgb[index + channel]) * (255 - alpha)
                            + u32::from(color[channel]) * alpha
                            + 127)
                            / 255) as u8;
                    }
                }
            }
            pen += glyph.advance;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_noto_covers_latin_digits_symbols_and_chinese() -> Result<()> {
        register()?;
        let font = Font::from_bytes(Arc::clone(&FONT), 0).map_err(|e| Output(e.to_string()))?;
        let scale = 24.0 / font.metrics().units_per_em as f32;
        for character in "REF 0123456789 ABC xyz -+.°雷达大连站".chars() {
            assert_ne!(font.glyph_for_char(character), None, "{character}");
            if character != ' ' {
                let raster = RasterGlyph::new(&font, character, 24, scale)?;
                assert!(raster.pixels.iter().any(|&alpha| alpha > 0), "{character}");
            }
        }
        Ok(())
    }

    #[test]
    fn labels_use_original_ascent_baseline_and_clip_at_canvas_edges() -> Result<()> {
        let mut rgb = vec![0; 240 * 80 * 3];
        labels(&mut rgb, 240, 80, &[("REF 123 雷达".into(), 0, 0)], 24, [255; 3])?;
        assert!(rgb.iter().any(|&v| v != 0));
        let mut outside = vec![0; 240 * 80 * 3];
        labels(&mut outside, 240, 80, &[("REF 雷达".into(), -500, -500)], 24, [255; 3])?;
        assert!(outside.iter().all(|&v| v == 0));
        labels(&mut outside, 240, 80, &[("雷达 123".into(), -10, -10)], 24, [255; 3])?;
        assert!(outside.iter().any(|&v| v != 0));
        Ok(())
    }

    #[test]
    fn cached_glyphs_match_individual_original_rasterization() -> Result<()> {
        let font = Font::from_bytes(Arc::clone(&FONT), 0).map_err(|e| Output(e.to_string()))?;
        let size = 24;
        let metrics = font.metrics();
        let scale = size as f32 / metrics.units_per_em as f32;
        let text = "REF 123 REF 雷达雷达";
        let mut expected = vec![0; 500 * 100 * 3];
        let mut pen = 10.0;
        // Render each label separately to exercise the uncached path with identical pen positions.
        for character in text.chars() {
            labels(
                &mut expected,
                500,
                100,
                &[(character.to_string(), Vector2F::new(pen, 0.0).to_i32().x(), 10)],
                size,
                [255; 3],
            )?;
            let glyph = font.glyph_for_char(character).unwrap_or(0);
            pen += font.advance(glyph).map_err(|e| Output(e.to_string()))?.x() * scale;
        }
        let mut actual = vec![0; 500 * 100 * 3];
        labels(&mut actual, 500, 100, &[(text.into(), 10, 10)], size, [255; 3])?;
        assert_eq!(actual, expected);
        Ok(())
    }
}

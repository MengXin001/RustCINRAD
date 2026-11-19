use super::*;
use crate::RadarError::Unsupported;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

pub(in crate::io) fn decode(bytes: &[u8], hint: &str, options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 416)?;
    let cuts = i32_at(bytes, 336)?;
    if !(0..=1024).contains(&cuts) {
        return Err(fail(336, "invalid product cut count"));
    }
    let p = 416 + cuts as usize * 256;
    slice(bytes, p, 192)?;
    let product = i32_at(bytes, p)?;
    if ![1, 13, 14, 24, 25, 26, 27, 28, 51, 52].contains(&product) {
        return Err(Unsupported(format!("standard PUP product {product}; radial products only")));
    }
    let code = string(slice(bytes, 32, 8)?);
    let site = station::resolve(
        if code.is_empty() { filename_code(hint).unwrap_or("UNKNOWN").into() } else { code },
        string(slice(bytes, 40, 32)?),
        Some((f32_at(bytes, 76)?, f32_at(bytes, 72)?)),
        Some(i32_at(bytes, 80)? as f64),
        options,
    )?;
    let time = i64::from(i32_at(bytes, p + 40)?) * 1000;
    let elevation = if [1, 51, 52].contains(&product) { f32_at(bytes, p + 128)? } else { 0.0 };
    let mut cursor = p + 192;
    let h = slice(bytes, cursor, 64)?;
    cursor += 64;
    let dtype = i32_at(h, 0)?;
    if !(0..=65535).contains(&dtype) {
        return Err(fail(cursor - 64, "invalid product field"));
    }
    let field = FieldId::from_code(dtype as u16);
    let scale = i32_at(h, 4)?;
    let offset = i32_at(h, 8)?;
    let word = i16_at(h, 12)?;
    let spacing = f64::from(i32_at(h, 16)?);
    let start = f64::from(i32_at(h, 20)?);
    let end = f64::from(i32_at(h, 24)?);
    let count = i32_at(h, 28)?;
    if scale <= 0
        || ![1, 2].contains(&word)
        || spacing <= 0.0
        || start < 0.0
        || end <= start
        || !(1..=200000).contains(&count)
    {
        return Err(fail(cursor - 64, "invalid product scale/range/ray count"));
    }

    let first = (start / spacing).floor() * spacing + spacing;
    let mut sweep = scan(elevation, time, None, 0.0);
    let mut budget = DecodeBudget::new(options);
    for _ in 0..count {
        budget.ray()?;
        let b = slice(bytes, cursor, 32)?;
        cursor += 32;
        let azimuth = f32_at(b, 0)?;
        let angular = f32_at(b, 4)?;
        let bins = i32_at(b, 8)?;
        if !(0..=1000000).contains(&bins) || !angular.is_finite() || angular <= 0.0 || angular > 360.0 {
            return Err(fail(cursor - 32, "invalid product radial geometry"));
        }
        budget.cells(bins as usize)?;
        let raw = slice(bytes, cursor, bins as usize * word as usize)?;
        cursor += raw.len();
        let mut values = Vec::with_capacity(bins as usize);
        let mut states = Vec::with_capacity(bins as usize);
        for i in 0..bins as usize {
            let r = if word == 1 { u16::from(raw[i]) } else { u16_at(raw, i * 2)? };
            let state = if r == 1 {
                CellState::RangeFolded
            } else if r < 5 {
                CellState::Missing
            } else {
                CellState::Valid
            };
            let value = if state == CellState::Valid { (f32::from(r) - offset as f32) / scale as f32 } else { 0.0 };
            values.push(value);
            states.push(state);
        }
        let mut moments = BTreeMap::new();
        moments.insert(field, Moment { spacing_m: spacing, first_m: first, values, states });
        sweep.rays.push(Ray { azimuth: azimuth.rem_euclid(360.0), elevation, moments });
        sweep.angular_width = if sweep.angular_width == 0.0 { angular } else { sweep.angular_width.min(angular) };
    }
    if cursor != bytes.len() {
        return Err(fail(cursor, "unexpected product trailing bytes"));
    }
    sweep.info.complete = true;
    RadarDataset::finish(RadarFormat::StandardPup, site, string(slice(bytes, 160, 32)?), vec![sweep])
}

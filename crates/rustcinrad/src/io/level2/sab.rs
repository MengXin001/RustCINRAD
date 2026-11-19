use super::*;
use crate::RadarError::Invalid;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

const ANGLE: f64 = 180.0 / 4096.0 * 0.125;
pub(super) fn decode(bytes: &[u8], hint: &str, options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 128)?;
    let candidates = [(RadarFormat::Sab, 2432, 128), (RadarFormat::Cab, 4132, 128), (RadarFormat::Special, 3132, 132)];
    let (format, size, header) = if let Some(f) = options.format {
        candidates
            .into_iter()
            .find(|(k, _, _)| *k == f)
            .ok_or_else(|| Invalid("legacy override must be SAB/CAB/Special".into()))?
    } else {
        candidates
            .into_iter()
            .find(|(_, n, _)| bytes.len().is_multiple_of(*n))
            .ok_or_else(|| fail(0, "unknown format or truncated legacy record; specify legacy variant"))?
    };
    if !bytes.len().is_multiple_of(size) {
        return Err(fail(bytes.len() / size * size, "truncated legacy radial"));
    }
    let site = legacy_site(hint, options)?;
    let mut sweeps = Vec::new();
    let mut budget = DecodeBudget::new(options);
    let mut last_el = 0;
    let task = format!("VCP{}", u16_at(bytes, 72)?);
    for (i, b) in bytes.chunks_exact(size).enumerate() {
        budget.ray()?;
        let base = i * size;
        let elevation_number = u16_at(b, 44)?;
        let radial_number = u16_at(b, 38)?;
        let state = u16_at(b, 40)?;
        if elevation_number == 0 || radial_number == 0 || state > 4 {
            return Err(fail(base + 38, "invalid legacy scan identifiers/state"));
        }
        let elevation = u16_at(b, 42)? as f64 * ANGLE;
        let milliseconds = u32::from_le_bytes(slice(b, 28, 4)?.try_into().map_err(|_| fail(base + 28, "time"))?);
        let day = u16_at(b, 32)?;
        if day == 0 || milliseconds >= 86_400_000 {
            return Err(fail(base + 28, "invalid legacy timestamp"));
        }
        let time = (i64::from(day) - 1) * 86_400_000 + i64::from(milliseconds);
        let nyquist = u16_at(b, 88)? as f64 / 100.0;
        if sweeps.is_empty() || elevation_number != last_el || radial_number == 1 {
            sweeps.push(scan(elevation, time, (nyquist > 0.0).then_some(nyquist), 0.0));
        }
        last_el = elevation_number;
        let rnum = u16_at(b, 54)? as usize;
        let vnum = u16_at(b, 56)? as usize;
        if rnum + 2 * vnum > size - header - 4 {
            return Err(fail(base + 54, "gate counts exceed radial payload"));
        }
        let velocity_resolution = u16_at(b, 70)?;
        if vnum > 0 && ![2, 4].contains(&velocity_resolution) {
            return Err(fail(base + 70, "unsupported velocity resolution"));
        }
        let mut moments = BTreeMap::new();
        for (field, count, spacing_off, first_off, pointer_off, fallback) in [
            (FieldId::Reflectivity, rnum, 50, 46, 64, header),
            (FieldId::Velocity, vnum, 52, 48, 66, header + rnum),
            (FieldId::SpectrumWidth, vnum, 52, 48, 68, header + rnum + vnum),
        ] {
            if count == 0 {
                continue;
            }
            let spacing = u16_at(b, spacing_off)? as f64;
            if spacing == 0.0 {
                return Err(fail(base + spacing_off, "zero gate spacing"));
            }

            let pointer = u16_at(b, pointer_off)? as usize;
            let offset = if pointer == 0 { fallback } else { pointer + 28 };
            if offset < header || offset + count > size - 4 {
                return Err(fail(base + pointer_off, "invalid moment pointer"));
            }
            let raw = slice(b, offset, count)?;
            budget.cells(count)?;
            let mut values = Vec::with_capacity(count);
            let mut states = Vec::with_capacity(count);
            for &code in raw {
                let state = if code == 0 {
                    CellState::Missing
                } else if field == FieldId::Velocity && code == 1 {
                    CellState::RangeFolded
                } else if field != FieldId::Reflectivity && code == 1 {
                    CellState::Missing
                } else {
                    CellState::Valid
                };
                let value = if state != CellState::Valid {
                    0.0
                } else if field == FieldId::Reflectivity {
                    if code == 255 { 0.0 } else { (f32::from(code) - 2.0) / 2.0 - 32.0 }
                } else if velocity_resolution == 2 {
                    (f32::from(code) - 2.0) / 2.0 - 63.5
                } else {
                    f32::from(code) - 2.0 - 127.0
                };
                values.push(value);
                states.push(state);
            }
            let first = f64::from(u16_at(b, first_off)? as i16).max(0.0) + spacing;
            moments.insert(field, Moment { spacing_m: spacing, first_m: first, values, states });
        }
        if let Some(s) = sweeps.last_mut() {
            s.rays.push(Ray { azimuth: (u16_at(b, 36)? as f64 * ANGLE).rem_euclid(360.0), elevation, moments });
            s.info.complete = matches!(state, 2 | 4);
        }
    }
    RadarDataset::finish(format, site, task, sweeps)
}

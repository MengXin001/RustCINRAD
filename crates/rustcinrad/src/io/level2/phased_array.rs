use super::*;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

pub(super) fn decode(bytes: &[u8], hint: &str, options: &ReadOptions) -> Result<RadarDataset> {
    const RECORD: usize = 16104;
    slice(bytes, 0, RECORD)?;
    if !bytes.len().is_multiple_of(RECORD) {
        return Err(fail(bytes.len() / RECORD * RECORD, "truncated legacy PA radial"));
    }
    let code = filename_code(hint).unwrap_or("PA").to_owned();
    let site = station::resolve(
        code,
        String::new(),
        Some((f64::from(i16_at(bytes, 2)?) * 360.0 / 65535.0, f64::from(i16_at(bytes, 4)?) * 360.0 / 65535.0)),
        Some(f64::from(u16_at(bytes, 8)?) * 1000.0 / 65535.0),
        options,
    )?;
    let layers = usize::from(u16_at(bytes, 48)?);
    let az_count = usize::from(u16_at(bytes, 44)?);
    if layers == 0 || layers > 128 || az_count == 0 || bytes.len() / RECORD != layers * az_count {
        return Err(fail(44, "invalid legacy PA layer/ray counts"));
    }
    let mut scans = BTreeMap::new();
    let mut budget = DecodeBudget::new(options);
    for (index, record) in bytes.as_chunks::<RECORD>().0.iter().enumerate() {
        budget.ray()?;
        let data = &record[32..];
        let elevation = f64::from(u16_at(data, 14)?) * 360.0 / 65535.0;
        let time = i64::from(u32_at(data, 0)?) * 1000;
        let spacing = (f64::from(u16_at(data, 22)?) * 1000.0 / 65535.0).round();
        let first = (f64::from(u16_at(data, 20)?) * 1000.0 / 65535.0).round();
        let gates = match u16_at(data, 24)? {
            0 => 2000,
            n => usize::from(n),
        };
        if spacing <= 0.0 || gates > 2000 {
            return Err(fail(index * RECORD + 52, "invalid legacy PA gate geometry"));
        }
        let mut moments = BTreeMap::new();
        for (i, field, scale, offset) in [
            (0, FieldId::TotalReflectivity, 106.0, -20.0),
            (1, FieldId::Reflectivity, 106.0, -20.0),
            (2, FieldId::Velocity, 101.0, -50.0),
            (3, FieldId::SpectrumWidth, 101.0, -50.0),
            (4, FieldId::DifferentialReflectivity, 16.0, -5.0),
            (5, FieldId::DifferentialPhase, 181.0, 0.0),
            (6, FieldId::SpecificDifferentialPhase, 31.0, -10.0),
            (7, FieldId::CorrelationCoefficient, 1.1, 0.0),
        ] {
            budget.cells(gates)?;
            let raw = slice(data, 68 + i * 2000, gates)?;
            let values =
                raw.iter().map(|&v| if v == 255 { 0.0 } else { f32::from(v) * scale / 255.0 + offset }).collect();
            let states = raw.iter().map(|&v| if v == 255 { CellState::Missing } else { CellState::Valid }).collect();
            moments.insert(field, Moment { spacing_m: spacing, first_m: first + spacing, values, states });
        }
        let s = scans.entry(index % layers).or_insert_with(|| scan(elevation, time, None, 360.0 / az_count as f64));
        s.rays.push(Ray { azimuth: f64::from(u16_at(data, 10)?) * 360.0 / 65535.0, elevation, moments });
        s.info.complete = true;
    }
    RadarDataset::finish(RadarFormat::LegacyPhasedArray, site, "Legacy PA".into(), scans.into_values().collect())
}

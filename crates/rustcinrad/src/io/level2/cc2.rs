use super::*;
use crate::RadarError::Unsupported;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

pub(super) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 2060)?;
    let obs = 216;
    let count = match bytes[obs] {
        10 => 1,
        n @ 101..=132 => usize::from(n - 100),
        _ => return Err(Unsupported("CC2 RHI/unknown scan mode".into())),
    };
    let site = header_site(bytes, (102, 10), (62, 40), (164, 168, 172), 1000.0, options)?;
    let time =
        local_time(i32::from(u16_at(bytes, obs + 1)?), &bytes[obs + 3..obs + 8], u32_at(bytes, obs + 9)? % 1000)?;
    let task = string(slice(bytes, 1500, 10)?);
    let start_r = f64::from(u16_at(bytes, obs + 16)?);
    let mut sweeps = Vec::with_capacity(count);
    let mut cursor = 2060;
    let mut budget = DecodeBudget::new(options);
    for index in 0..count {
        let p = obs + 22 + index * 35;
        let rays = usize::from(u16_at(bytes, p + 26)?);
        let zr = f64::from(u16_at(bytes, p + 14)?) / 10.0;
        let zg = usize::from(u16_at(bytes, p + 20)?);
        let vg = usize::from(u16_at(bytes, p + 22)?);
        let wg = usize::from(u16_at(bytes, p + 24)?);
        let nyquist = f64::from(u16_at(bytes, p + 10)?) / 100.0;
        if rays == 0 || rays > 4096 || zr <= 0.0 || zg == 0 || zg.max(vg).max(wg) > 4096 {
            return Err(fail(p, "invalid CC2 geometry"));
        }
        let elevation = f64::from(i16_at(bytes, p + 28)?) / 100.0;
        let mut sweep = scan(elevation, time, (nyquist > 0.0).then_some(nyquist), 0.0);

        for _ in 0..rays {
            budget.ray()?;
            let record = slice(bytes, cursor, 49163)?;
            cursor += 49163;
            let mut moments = BTreeMap::new();
            for (field, offset, bins, word) in [
                (FieldId::Reflectivity, 11, zg, 1),
                (FieldId::TotalReflectivity, 4107, zg, 1),
                (FieldId::DifferentialReflectivity, 16395, zg, 2),
                (FieldId::DifferentialPhase, 24587, zg, 2),
                (FieldId::CorrelationCoefficient, 32779, zg, 2),
                (FieldId::SpecificDifferentialPhase, 40971, zg, 2),
            ] {
                if bins == 0 {
                    continue;
                }
                let spacing = zr;
                budget.cells(bins)?;
                let mut values = Vec::with_capacity(bins);
                let mut states = Vec::with_capacity(bins);
                for gate in 0..bins {
                    let raw = if word == 1 {
                        i32::from(record[offset + gate])
                    } else {
                        i32::from(i16_at(record, offset + gate * 2)?)
                    };
                    let missing =
                        if word == 1 || field == FieldId::CorrelationCoefficient { raw == 0 } else { raw == -32768 };
                    let state = if missing { CellState::Missing } else { CellState::Valid };
                    let v = if missing {
                        0.0
                    } else {
                        match field {
                            FieldId::Reflectivity | FieldId::TotalReflectivity => (raw as f32 - 64.0) / 2.0,
                            FieldId::CorrelationCoefficient => raw as f32 * 0.001,
                            _ => raw as f32 * 0.01,
                        }
                    };
                    values.push(v);
                    states.push(state);
                }
                moments
                    .insert(field, Moment { spacing_m: spacing, first_m: spacing * (1.0 + start_r), values, states });
            }
            sweep.rays.push(Ray {
                azimuth: f64::from(u16_at(record, 2)?) / 100.0,
                elevation: f64::from(i16_at(record, 0)?) / 100.0,
                moments,
            });
        }
        sweep.info.complete = true;
        sweeps.push(sweep);
    }
    if cursor != bytes.len() {
        return Err(fail(cursor, "unexpected CC2 trailing bytes"));
    }
    RadarDataset::finish(RadarFormat::Cc2, site, task, sweeps)
}

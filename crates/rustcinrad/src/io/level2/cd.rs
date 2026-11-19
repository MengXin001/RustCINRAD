use super::*;
use crate::RadarError::Unsupported;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

pub(super) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 1024)?;
    let obs = 201;
    let count = match bytes[obs] {
        10 => 1,
        n @ 101..=130 => usize::from(n - 100),
        _ => return Err(Unsupported("CD RHI/unknown scan mode".into())),
    };
    let site = header_site(bytes, (90, 10), (50, 40), (152, 156, 160), 100.0, options)?;
    let time =
        local_time(i32::from(u16_at(bytes, obs + 1)?), &bytes[obs + 3..obs + 8], u32_at(bytes, obs + 9)? % 1000)?;
    let mut budget = DecodeBudget::new(options);
    let mut sweeps = Vec::with_capacity(count);
    let mut cursor = 1024;
    for index in 0..count {
        let p = obs + 16 + index * 21;
        let nyquist = f64::from(u16_at(bytes, p + 9)?) / 100.0;
        let spacing = f64::from(u16_at(bytes, p + 13)?) / 10.0;
        let gates = match u16_at(bytes, p + 15)? {
            0 => 998,
            n => usize::from(n),
        };
        let rays = match u16_at(bytes, p + 17)? {
            0 => 360,
            n => usize::from(n),
        };
        if spacing <= 0.0 || gates > 998 || rays > 4096 || nyquist <= 0.0 {
            return Err(fail(p, "invalid CD geometry"));
        }
        let elevation = f64::from(i16_at(bytes, p + 19)?) / 100.0;
        let mut sweep = scan(elevation, time, Some(nyquist), 360.0 / rays as f64);
        for index in 0..rays {
            budget.ray()?;
            let record = slice(bytes, cursor, 4000)?;
            cursor += 4000;
            let mut moments = BTreeMap::new();
            for (field, offset) in [
                (FieldId::Reflectivity, 0),
                (FieldId::Velocity, 1),
                (FieldId::TotalReflectivity, 2),
                (FieldId::SpectrumWidth, 3),
            ] {
                budget.cells(gates)?;
                let mut values = Vec::with_capacity(gates);
                let mut states = Vec::with_capacity(gates);
                for gate in 0..gates {
                    let raw = record[8 + gate * 4 + offset];
                    let state = if raw == 0 {
                        CellState::Missing
                    } else if field == FieldId::Velocity && raw == 128 {
                        CellState::RangeFolded
                    } else {
                        CellState::Valid
                    };
                    let value = if state != CellState::Valid {
                        0.0
                    } else {
                        match field {
                            FieldId::Reflectivity | FieldId::TotalReflectivity => (f32::from(raw) - 64.0) / 2.0,
                            FieldId::Velocity => (nyquist * (f64::from(raw) - 128.0) / 128.0) as f32,
                            _ => (nyquist * f64::from(raw) / 256.0) as f32,
                        }
                    };
                    values.push(value);
                    states.push(state);
                }
                moments.insert(field, Moment { spacing_m: spacing, first_m: spacing, values, states });
            }

            let start = f64::from(u16_at(record, 0)?) * 360.0 / 65536.0;
            let end = f64::from(u16_at(record, 4)?) * 360.0 / 65536.0;
            let azimuth = if start == 0.0 && end == 0.0 {
                index as f64 * 360.0 / rays as f64
            } else {
                (start + (end - start + 180.0).rem_euclid(360.0) / 2.0 - 90.0).rem_euclid(360.0)
            };
            sweep.rays.push(Ray { azimuth, elevation, moments });
        }
        sweep.info.complete = true;
        sweeps.push(sweep);
    }
    if cursor != bytes.len() {
        return Err(fail(cursor, "unexpected CD trailing bytes"));
    }
    RadarDataset::finish(RadarFormat::Cd, site, format!("CD {count} layers"), sweeps)
}

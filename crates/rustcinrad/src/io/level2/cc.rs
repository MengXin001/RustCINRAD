use super::*;
use crate::RadarError::Unsupported;
use crate::io::base::{Moment, Ray};
use std::collections::BTreeMap;

pub(super) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 1024)?;
    let mode = bytes[199];
    if !(101..=130).contains(&mode) {
        return Err(Unsupported("CC non-volume scan".into()));
    }
    let count = usize::from(mode - 100);
    let site = header_site(bytes, (106, 10), (66, 40), (168, 172, 176), 3_600_000.0, options)?;
    let time =
        local_time(i32::from(bytes[184]) * 100 + i32::from(bytes[185]), &bytes[186..191], u32_at(bytes, 200)? % 1000)?;
    let mut budget = DecodeBudget::new(options);
    let mut sweeps = Vec::with_capacity(count);
    let mut cursor = 1024;
    for index in 0..count {
        let p = 218 + index * 22;
        let elevation = i16_at(bytes, p + 18)? as f64 / 100.0;
        let nyquist = u16_at(bytes, p)? as f64 / 100.0;
        let gates = match u16_at(bytes, p + 6)? {
            0 => 500,
            n => usize::from(n),
        };
        let rays = match u16_at(bytes, p + 8)? {
            0 => 512,
            n => usize::from(n),
        };
        if gates > 500 || rays > 4096 {
            return Err(fail(p, "invalid CC bins or rays"));
        }
        let spacing = match u16_at(bytes, p + 4)? {
            0 => 300.0,
            n => f64::from(n) * 2.0,
        };
        let mut sweep = scan(elevation, time, (nyquist > 0.0).then_some(nyquist), 360.0 / rays as f64);
        for ray_index in 0..rays {
            budget.ray()?;
            let record = slice(bytes, cursor, 3000)?;
            cursor += 3000;
            let mut moments = BTreeMap::new();
            for (field, offset) in
                [(FieldId::Reflectivity, 0), (FieldId::Velocity, 1000), (FieldId::SpectrumWidth, 2000)]
            {
                budget.cells(gates)?;
                let mut values = Vec::with_capacity(gates);
                let mut states = Vec::with_capacity(gates);
                for gate in 0..gates {
                    let raw = i16_at(record, offset + gate * 2)?;
                    values.push(if raw == i16::MIN { 0.0 } else { f32::from(raw) / 10.0 });
                    states.push(if raw == i16::MIN { CellState::Missing } else { CellState::Valid });
                }
                moments.insert(field, Moment { spacing_m: spacing, first_m: spacing, values, states });
            }

            sweep.rays.push(Ray { azimuth: ray_index as f64 * 360.0 / rays as f64, elevation, moments });
        }
        sweep.info.complete = true;
        sweeps.push(sweep);
    }
    if cursor != bytes.len() {
        return Err(fail(cursor, "unexpected CC trailing bytes"));
    }
    RadarDataset::finish(RadarFormat::Cc, site, format!("CC {count} layers"), sweeps)
}

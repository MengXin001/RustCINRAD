use super::*;
use crate::RadarError::Unsupported;
use crate::io::base::{Moment, Ray};
use binrw::BinRead;
use std::collections::BTreeMap;
use std::io::Cursor;

#[derive(BinRead)]
#[br(little)]
struct MomentHeader {
    code: i32,
    scale: i32,
    offset: i32,
    bin_length: i16,
    _flags: i16,
    length: i32,
    _reserved: [u8; 12],
}

pub(in crate::io) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 416)?;
    let generic = i32_at(bytes, 8)?;
    if ![1, 16].contains(&generic) {
        return Err(Unsupported(format!("RSTM generic type {generic} (only base data)")));
    }
    let pa = generic == 16;
    if i32_at(bytes, 324)? == 2 {
        return Err(Unsupported("native RHI reading (use geographic sections for PPI volumes)".into()));
    }
    let site = station::resolve(
        string(slice(bytes, 32, 8)?),
        string(slice(bytes, 40, 32)?),
        Some((f32_at(bytes, 76)?, f32_at(bytes, 72)?)),
        Some(i32_at(bytes, 80)? as f64),
        options,
    )?;
    let task = string(slice(bytes, 160, 32)?);
    let cut_count = i32_at(bytes, if pa { 332 } else { 336 })?;
    if !(1..=1024).contains(&cut_count) {
        return Err(fail(336, "invalid cut count"));
    }
    let beam_count = if pa { i32_at(bytes, 328)? } else { 0 };
    if !(0..=4096).contains(&beam_count) {
        return Err(fail(328, "invalid PA beam count"));
    }
    let cut_start = 416 + beam_count as usize * 640;
    let cut_size = 256;
    let mut cursor = cut_start + cut_count as usize * cut_size;
    slice(bytes, cut_start, cut_count as usize * cut_size)?;
    let mut cuts = Vec::new();
    for i in 0..cut_count as usize {
        let b = slice(bytes, cut_start + i * cut_size, cut_size)?;
        let (log, dop, start, nyquist, angular) = if pa {
            (f32_at(b, 72)?, f32_at(b, 76)?, i32_at(b, 88)? as f64, f32_at(b, 108)?, f32_at(b, 64)?)
        } else {
            (i32_at(b, 44)? as f64, i32_at(b, 48)? as f64, i32_at(b, 60)? as f64, f32_at(b, 80)?, f32_at(b, 36)?)
        };
        if log <= 0.0 || dop <= 0.0 || start < 0.0 || ![log, dop, start, nyquist, angular].iter().all(|v| v.is_finite())
        {
            return Err(fail(cut_start + i * cut_size, "invalid cut geometry"));
        }
        cuts.push((log, dop, start, nyquist, angular));
    }
    let mut scans: BTreeMap<i32, crate::io::base::Sweep> = BTreeMap::new();
    let mut budget = DecodeBudget::new(options);
    while cursor < bytes.len() {
        budget.ray()?;
        let base = cursor;
        let h = slice(bytes, cursor, if pa { 128 } else { 64 })?;
        cursor += h.len();
        let state = i32_at(h, 0)?;
        let scan_number = i32_at(h, 16)?;
        if !(0..=6).contains(&state) || scan_number <= 0 || scan_number > cut_count {
            return Err(fail(base, "invalid radial state or cut index"));
        }
        let (log, dop, start, nyquist, angular) = cuts[(scan_number - 1) as usize];
        let azi = f32_at(h, 20)?;
        let elevation = f32_at(h, 24)?;
        let secs = if pa {
            i64::from_le_bytes(slice(h, 28, 8)?.try_into().map_err(|_| fail(base + 28, "seconds"))?)
        } else {
            i32_at(h, 28)? as i64
        };
        let micros = i32_at(h, if pa { 36 } else { 32 })?;
        if !(0..1_000_000).contains(&micros) {
            return Err(fail(base + 32, "invalid microseconds"));
        }
        let time = secs
            .checked_mul(1000)
            .and_then(|s| s.checked_add(i64::from(micros) / 1000))
            .ok_or_else(|| fail(base + 28, "timestamp overflow"))?;
        let length = i32_at(h, if pa { 40 } else { 36 })?;
        let count = i32_at(h, if pa { 44 } else { 40 })?;
        if length < 0 || !(0..=64).contains(&count) {
            return Err(fail(base + 36, "invalid radial length/moment count"));
        }
        if !pa && h[50] != 0 {
            return Err(Unsupported("embedded FMT radial compression".into()));
        }
        let end = cursor.checked_add(length as usize).ok_or_else(|| fail(base, "radial length overflow"))?;
        slice(bytes, cursor, length as usize)?;
        let mut moments = BTreeMap::new();
        for _ in 0..count {
            let b = slice(bytes, cursor, 32)?;
            let m = MomentHeader::read(&mut Cursor::new(b)).map_err(|e| fail(cursor, e.to_string()))?;
            cursor += 32;
            if m.scale <= 0
                || ![1, 2].contains(&m.bin_length)
                || m.length < 0
                || m.length % m.bin_length as i32 != 0
                || !(0..=65535).contains(&m.code)
            {
                return Err(fail(cursor - 32, "invalid moment code/scale/bin length/block length"));
            }
            let data = slice(bytes, cursor, m.length as usize)?;
            cursor += data.len();
            if cursor > end {
                return Err(fail(cursor, "moment exceeds radial block"));
            }
            let field = FieldId::from_code(m.code as u16);
            let bins = data.len() / m.bin_length as usize;
            if bins == 0 {
                continue;
            }
            budget.cells(bins)?;
            let mut values = Vec::with_capacity(bins);
            let mut states = Vec::with_capacity(bins);
            for b in data.chunks_exact(m.bin_length as usize) {
                let raw = if b.len() == 1 { b[0] as u16 } else { u16::from_le_bytes([b[0], b[1]]) };
                let state = if raw == 1 && field == FieldId::Velocity {
                    CellState::RangeFolded
                } else if raw < 5 {
                    CellState::Missing
                } else {
                    CellState::Valid
                };
                let value =
                    if state == CellState::Valid { (f32::from(raw) - m.offset as f32) / m.scale as f32 } else { 0.0 };
                values.push(value);
                states.push(state);
            }
            let spacing = if matches!(field, FieldId::Velocity | FieldId::SpectrumWidth) { dop } else { log };
            if moments.insert(field, Moment { spacing_m: spacing, first_m: start + spacing, values, states }).is_some()
            {
                return Err(fail(base, "duplicate moment"));
            }
        }
        if cursor != end {
            return Err(fail(cursor, "radial data length does not match moment blocks"));
        }
        let s = scans
            .entry(scan_number)
            .or_insert_with(|| scan(elevation, time, (nyquist > 0.0).then_some(nyquist), angular));
        s.info.complete = matches!(state, 2 | 4 | 6);
        s.rays.push(Ray { azimuth: azi.rem_euclid(360.0), elevation, moments });
    }
    RadarDataset::finish(
        if pa { RadarFormat::PhasedArray } else { RadarFormat::Fmt },
        site,
        task,
        scans.into_values().collect(),
    )
}

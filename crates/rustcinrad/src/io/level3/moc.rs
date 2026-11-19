use super::*;
use crate::RadarError::Unsupported;

pub(in crate::io) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 20)?;
    if bytes.get(12) == Some(&b'Z') { single(bytes, options) } else { mosaic(bytes, options) }
}

fn dimensions(bytes: &[u8], x: usize, y: usize) -> Result<(usize, usize)> {
    let width = usize::try_from(i32_at(bytes, x)?).map_err(|_| fail(x, "negative raster width"))?;
    let height = usize::try_from(i32_at(bytes, y)?).map_err(|_| fail(y, "negative raster height"))?;
    if width < 2 || height < 2 || width.checked_mul(height).is_none_or(|n| n > 16_000_000) {
        return Err(fail(x, "invalid/oversized MOC raster"));
    }
    Ok((width, height))
}

fn date(bytes: &[u8], offset: usize, tz: i32) -> Result<i64> {
    let mut parts = [0_u32; 6];
    for (i, p) in parts.iter_mut().enumerate() {
        *p = u32::from(u16_at(bytes, offset + i * 2)?);
    }
    calendar_time(parts, tz)
}

fn bounds(bytes: &[u8], offset: usize, width: usize, height: usize) -> Result<GridSpec> {
    grid_spec(
        i32_at(bytes, offset + 4)? as f64 / 1000.0,
        i32_at(bytes, offset)? as f64 / 1000.0,
        i32_at(bytes, offset + 12)? as f64 / 1000.0,
        i32_at(bytes, offset + 8)? as f64 / 1000.0,
        width,
        height,
    )
}

fn words(bytes: &[u8]) -> Vec<i16> {
    bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

fn mosaic(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 256)?;
    if u16_at(bytes, 14)? != 0 {
        return Err(Unsupported("MOC non-geographic coordinate system".into()));
    }
    let (width, height) = dimensions(bytes, 148, 152)?;
    let spec = bounds(bytes, 124, width, height)?;
    let time = date(bytes, 100, i32_at(bytes, 96)?)?;
    let field = field_name(&string(slice(bytes, 16, 8)?))?;
    let mut budget = DecodeBudget::new(options);
    budget.cells(width * height)?;
    let body = unpack(&bytes[256..], u16_at(bytes, 166)?, options)?;
    if body.len() != width * height * 2 {
        return Err(fail(256, "MOC payload size mismatch"));
    }

    let g = raster(spec, field, &words(&body), f32::from(u16_at(bytes, 176)?), f32::from(u16_at(bytes, 164)?), time)?;
    let site = options.site.clone().unwrap_or(RadarSite {
        code: "MOC".into(),
        name: "MOC grid domain".into(),
        position: spec.point(height / 2, width / 2).ok_or_else(|| fail(124, "invalid MOC grid centre"))?,
        altitude_m: 0.0,
    });
    RadarDataset::from_rasters(RadarFormat::Moc, site, "MOC mosaic".into(), vec![g])
}

fn single(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 758)?;
    let site = header_site(bytes, (12, 8), (20, 20), (76, 72, 80), 10_000.0, options)?;
    let time = date(bytes, 88, i32_at(bytes, 84)?)?;
    let count = usize::from(u16_at(bytes, 112)?);
    if count == 0 || count > 64 {
        return Err(fail(112, "invalid MOC block count"));
    }
    let body = unpack(&bytes[758..], u16_at(bytes, 114)?, options)?;
    let mut cursor = 0;
    let mut rasters = Vec::new();
    let mut budget = DecodeBudget::new(options);
    for _ in 0..count {
        let b = slice(&body, cursor, 132)?;
        cursor += 132;
        if i32_at(b, 92)? == 0 && i32_at(b, 88)? == 0 {
            continue;
        }
        if u16_at(b, 2)? != 0 {
            return Err(Unsupported("MOC non-geographic block coordinates".into()));
        }
        let (width, height) = dimensions(b, 92, 88)?;
        budget.cells(width * height)?;
        let spec = bounds(b, 64, width, height)?;
        let data = slice(&body, cursor, width * height * 2)?;
        cursor += data.len();
        rasters.push(raster(
            spec,
            field_name(&string(slice(b, 4, 8)?))?,
            &words(data),
            f32::from(u16_at(b, 120)?),
            f32::from(u16_at(b, 110)?),
            time,
        )?);
    }
    if cursor != body.len() {
        return Err(fail(cursor, "MOC blocks do not consume payload"));
    }
    RadarDataset::from_rasters(RadarFormat::Moc, site, format!("VCP{}", u16_at(bytes, 756)?), rasters)
}

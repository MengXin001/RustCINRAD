use super::*;

pub(in crate::io) fn decode(bytes: &[u8], options: &ReadOptions) -> Result<RadarDataset> {
    slice(bytes, 0, 1024)?;
    let width = usize::from(u16_at(bytes, 78)?);
    let height = usize::from(u16_at(bytes, 80)?);
    let levels = usize::from(u16_at(bytes, 82)?);
    if levels == 0 || levels > 40 {
        return Err(fail(82, "invalid SWAN levels"));
    }
    let name = string(slice(bytes, 12, 38)?);
    let field = if name == "3DREF" {
        FieldId::Reflectivity
    } else if name.contains("REF") || name.contains("CR") || name.contains("\u{53cd}\u{5c04}\u{7387}") {
        FieldId::CompositeReflectivity
    } else {
        field_name(&name)?
    };
    let start_lon = f32_at(bytes, 88)?;
    let start_lat = f32_at(bytes, 92)?;
    let centre_delta = (f32_at(bytes, 96)? - start_lon + 180.0).rem_euclid(360.0) - 180.0;
    let end_lon = start_lon + 2.0 * centre_delta;
    let end_lat = 2.0 * f32_at(bytes, 100)? - start_lat;
    let reverse_columns = end_lon < start_lon;
    let reverse_rows = end_lat > start_lat;
    let spec = grid_spec(
        (start_lon.min(end_lon) + 180.0).rem_euclid(360.0) - 180.0,
        start_lat.min(end_lat),
        start_lon.max(end_lon),
        start_lat.max(end_lat),
        width,
        height,
    )?;
    let time = calendar_time(
        [
            u32::from(u16_at(bytes, 66)?),
            u32::from(u16_at(bytes, 68)?),
            u32::from(u16_at(bytes, 70)?),
            u32::from(u16_at(bytes, 72)?),
            u32::from(u16_at(bytes, 74)?),
            0,
        ],
        28_800,
    )?;
    let datatype = i16_at(bytes, 852)?;
    let word = match datatype {
        0 | 1 => 1,
        2..=4 => 2,
        _ => return Err(fail(852, "unknown SWAN data word")),
    };
    let cells = width * height;
    let mut budget = DecodeBudget::new(options);
    budget.cells(cells.checked_mul(levels).ok_or_else(|| fail(78, "SWAN size overflow"))?)?;
    let payload = slice(bytes, 1024, cells * levels * word)?;
    if bytes.len() != 1024 + payload.len() {
        return Err(fail(1024 + payload.len(), "unexpected SWAN trailing bytes"));
    }
    let mut rasters = Vec::with_capacity(levels);
    for layer in 0..levels {
        let mut values = Array2::zeros((height, width));
        let mut states = Array2::from_elem((height, width), CellState::Missing);
        for row in 0..height {
            for col in 0..width {
                let sr = if reverse_rows { height - 1 - row } else { row };
                let sc = if reverse_columns { width - 1 - col } else { col };
                let offset = ((layer * height + sr) * width + sc) * word;
                let raw = match datatype {
                    0 => i32::from(payload[offset]),
                    1 => i32::from(payload[offset] as i8),
                    3 => i32::from(i16_at(payload, offset)?),
                    _ => i32::from(u16_at(payload, offset)?),
                };
                if raw != 0 {
                    states[(row, col)] = CellState::Valid;
                    values[(row, col)] = if matches!(field, FieldId::CompositeReflectivity | FieldId::Reflectivity) {
                        (raw as f32 - 66.0) / 2.0
                    } else {
                        raw as f32
                    };
                }
            }
        }

        rasters.push(GeoGrid::new(
            spec,
            field,
            values,
            states,
            Array2::from_elem((height, width), (f32_at(bytes, 112 + layer * 4)? * 1000.0) as f32),
            time,
        )?);
    }
    let site = options.site.clone().unwrap_or(RadarSite {
        code: "SWAN".into(),
        name: "SWAN grid domain".into(),
        position: GeoPoint::new(f32_at(bytes, 96)?, f32_at(bytes, 100)?)?,
        altitude_m: 0.0,
    });
    RadarDataset::from_rasters(RadarFormat::Swan, site, name, rasters)
}

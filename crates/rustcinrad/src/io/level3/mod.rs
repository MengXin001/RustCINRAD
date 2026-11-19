use crate::RadarError::Invalid;

pub(super) mod moc;
pub(super) mod standard;
pub(super) mod swan;

use super::*;
use ndarray::Array2;

fn grid_spec(west: f64, south: f64, east: f64, north: f64, width: usize, height: usize) -> Result<GridSpec> {
    if width < 2 || height < 2 {
        return Err(Invalid("geographic raster needs at least 2x2 cells".into()));
    }
    let span = (east - west).rem_euclid(360.0);
    GridSpec::new(west, north, span / (width - 1) as f64, (north - south) / (height - 1) as f64, width, height)
}

fn field_name(name: &str) -> Result<FieldId> {
    match name.trim().to_ascii_uppercase().as_str() {
        "QREF" | "CAP" | "CAPPI" | "3DREF" => Ok(FieldId::Reflectivity),
        "CREF" | "CRF" | "UCR" | "FENGL" => Ok(FieldId::CompositeReflectivity),
        s => s.parse(),
    }
}

fn raster(spec: GridSpec, field: FieldId, raw: &[i16], scale: f32, height: f32, time: i64) -> Result<GeoGrid> {
    if !scale.is_finite() || scale <= 0.0 || raw.len() != spec.width() * spec.height() {
        return Err(fail(0, "invalid geographic raster scale/shape"));
    }
    let shape = (spec.height(), spec.width());
    let values =
        Array2::from_shape_vec(shape, raw.iter().map(|&v| if v < 0 { 0.0 } else { f32::from(v) / scale }).collect())
            .map_err(|e| Invalid(e.to_string()))?;
    let states = Array2::from_shape_vec(
        shape,
        raw.iter().map(|&v| if v < 0 { CellState::Missing } else { CellState::Valid }).collect(),
    )
    .map_err(|e| Invalid(e.to_string()))?;
    GeoGrid::new(spec, field, values, states, Array2::from_elem(shape, height), time)
}

fn unpack(bytes: &[u8], compression: u16, options: &ReadOptions) -> Result<Vec<u8>> {
    match compression {
        0 => bounded(bytes, options.max_bytes),
        1 => bounded(bzip2::read::BzDecoder::new(bytes), options.max_bytes),
        2 => bounded(flate2::read::MultiGzDecoder::new(bytes), options.max_bytes),
        _ => Err(fail(0, "unknown embedded compression")),
    }
}

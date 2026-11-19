use crate::RadarError::Invalid;
use crate::{
    calc::{self, Product, ProductRequest},
    projection::BeamModel,
    *,
};

#[derive(Debug)]
pub struct GeoVolume {
    pub spec: GridSpec,
    pub heights_m: Vec<f64>,
    pub layers: Vec<GeoGrid>,
}

pub fn cappi_volume(
    dataset: &RadarDataset,
    spec: GridSpec,
    heights_m: &[f64],
    max_gap_m: f64,
    beam: BeamModel,
) -> Result<GeoVolume> {
    cappi_volume_with_progress(dataset, spec, heights_m, max_gap_m, beam, |_| true)
}

pub fn cappi_volume_with_progress(
    dataset: &RadarDataset,
    spec: GridSpec,
    heights_m: &[f64],
    max_gap_m: f64,
    beam: BeamModel,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<GeoVolume> {
    if heights_m.is_empty()
        || heights_m.iter().any(|h| !h.is_finite())
        || heights_m.windows(2).any(|h| h[1] <= h[0])
        || spec
            .width()
            .checked_mul(spec.height())
            .and_then(|n| n.checked_mul(heights_m.len()))
            .is_none_or(|n| n > 16_000_000)
    {
        return Err(Invalid("invalid or oversized CAPPI volume".into()));
    }
    let mut layers = Vec::with_capacity(heights_m.len());
    for (index, &height_m) in heights_m.iter().enumerate() {
        layers.push(dataset.product_with_progress(
            ProductRequest { product: Product::Cappi { height_m, max_gap_m }, grid: spec, beam },
            |p| progress((index as f32 + p) / heights_m.len() as f32),
        )?);
    }
    Ok(GeoVolume { spec, heights_m: heights_m.to_vec(), layers })
}

pub fn mosaic_volumes(volumes: &[&GeoVolume], maximum_time_skew_ms: i64) -> Result<GeoVolume> {
    let first = volumes.first().ok_or_else(|| Invalid("empty 3D mosaic".into()))?;
    if maximum_time_skew_ms < 0
        || first.layers.is_empty()
        || first.heights_m.iter().any(|h| !h.is_finite())
        || first.heights_m.windows(2).any(|h| h[1] <= h[0])
        || first
            .spec
            .width()
            .checked_mul(first.spec.height())
            .and_then(|n| n.checked_mul(first.layers.len()))
            .is_none_or(|n| n > 16_000_000)
        || volumes.iter().any(|v| {
            v.spec != first.spec
                || v.heights_m != first.heights_m
                || v.layers.len() != v.heights_m.len()
                || v.layers.iter().enumerate().any(|(i, g)| {
                    g.spec() != v.spec
                        || g.field() != FieldId::Cappi
                        || g.heights_m()
                            .iter()
                            .zip(g.states())
                            .any(|(h, s)| *s == CellState::Valid && (f64::from(*h) - v.heights_m[i]).abs() > 0.01)
                })
        })
    {
        return Err(Invalid("3D mosaic requires aligned nonempty CAPPI volumes".into()));
    }
    let mut layers = Vec::with_capacity(first.layers.len());
    for index in 0..first.layers.len() {
        let grids: Vec<_> = volumes.iter().map(|v| &v.layers[index]).collect();
        let minimum = grids.iter().map(|g| g.time_unix_ms()).min().unwrap_or(0);
        let maximum = grids.iter().map(|g| g.time_unix_ms()).max().unwrap_or(0);
        if maximum.checked_sub(minimum).is_none_or(|dt| dt > maximum_time_skew_ms) {
            return Err(Invalid("3D mosaic observations exceed time tolerance".into()));
        }
        layers.push(calc::mosaic(&grids)?);
    }
    Ok(GeoVolume { spec: first.spec, heights_m: first.heights_m.clone(), layers })
}

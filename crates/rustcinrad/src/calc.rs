use crate::RadarError::{Cancelled, Invalid, MissingField};
use crate::{
    projection::{self, BeamModel},
    *,
};
use ndarray::Array2;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Product {
    Cappi { height_m: f64, max_gap_m: f64 },
    CompositeReflectivity,
    EchoTop { threshold_dbz: f32 },
    Vil { threshold_dbz: f32, beam_width_deg: f64 },
    VilDensity { threshold_dbz: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProductRequest {
    pub product: Product,
    pub grid: GridSpec,
    pub beam: BeamModel,
}

impl RadarDataset {
    pub fn product(&self, request: ProductRequest) -> Result<GeoGrid> {
        self.product_with_progress(request, |_| true)
    }

    pub fn product_with_progress(
        &self,
        request: ProductRequest,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        let threshold = match request.product {
            Product::CompositeReflectivity | Product::Cappi { .. } => 0.0,
            Product::EchoTop { threshold_dbz }
            | Product::Vil { threshold_dbz, .. }
            | Product::VilDensity { threshold_dbz } => threshold_dbz,
        };
        if !threshold.is_finite() {
            return Err(Invalid("non-finite product threshold".into()));
        }
        if let Product::Vil { beam_width_deg, .. } = request.product
            && (!beam_width_deg.is_finite() || !(0.0..=20.0).contains(&beam_width_deg))
        {
            return Err(Invalid("beam width must be between 0 and 20 degrees".into()));
        }
        if let Product::Cappi { height_m, max_gap_m } = request.product
            && (!height_m.is_finite() || !max_gap_m.is_finite() || max_gap_m <= 0.0)
        {
            return Err(Invalid("CAPPI needs a finite MSL height and positive vertical gap".into()));
        }
        let indices: Vec<_> = self
            .summary
            .sweeps
            .iter()
            .filter(|s| s.fields.iter().any(|f| f.field == FieldId::Reflectivity))
            .map(|s| s.index)
            .collect();
        if indices.is_empty() {
            return Err(MissingField { sweep: 0, field: FieldId::Reflectivity });
        }
        if !progress(0.0) {
            return Err(Cancelled);
        }
        let shape = (request.grid.height(), request.grid.width());
        let cells = shape.0 * shape.1;
        let mut values = vec![0.0; cells];
        let mut states = vec![CellState::OutsideCoverage; cells];
        let mut heights = vec![0.0; cells];
        for start in (0..shape.0).step_by(16) {
            let end = (start + 16).min(shape.0);
            let span = start * shape.1..end * shape.1;
            values[span.clone()]
                .par_chunks_mut(shape.1)
                .zip(states[span.clone()].par_chunks_mut(shape.1))
                .zip(heights[span].par_chunks_mut(shape.1))
                .enumerate()
                .for_each(|(row, ((values, states), heights))| {
                    let mut column = Vec::with_capacity(indices.len());
                    let mut altitude_column = Vec::with_capacity(indices.len());
                    for col in 0..shape.1 {
                        let Some(point) = request.grid.point(start + row, col) else {
                            continue;
                        };
                        let (ground, bearing) = projection::distance_bearing(self.summary.site.position, point);
                        column.clear();
                        altitude_column.clear();
                        let mut any_covered = false;
                        for &index in &indices {
                            let scan = &self.sweeps[index];
                            let observation = if self.rasters.is_empty() {
                                self.observe_ground(scan, FieldId::Reflectivity, ground, bearing, request.beam)
                            } else {
                                self.observe(scan, FieldId::Reflectivity, point, request.beam)
                            };
                            if let Some((value, state, height)) = observation {
                                any_covered |= state != CellState::OutsideCoverage;
                                if matches!(request.product, Product::Cappi { .. }) {
                                    altitude_column.push((value, height, state));
                                }
                                if state == CellState::Valid {
                                    column.push((value, height));
                                }
                            }
                        }
                        if let Product::Cappi { height_m, max_gap_m } = request.product {
                            altitude_column.sort_by(|a, b| a.1.total_cmp(&b.1));
                            (values[col], states[col]) = cappi_sample(&altitude_column, height_m, max_gap_m);
                            heights[col] = height_m as f32;
                            continue;
                        }
                        if column.is_empty() {
                            if any_covered {
                                states[col] = CellState::Missing;
                            }
                            continue;
                        }
                        column.sort_by(|a, b| a.1.total_cmp(&b.1));
                        states[col] = CellState::Valid;
                        (values[col], heights[col]) = reduce_column(&column, request.product, ground);
                    }
                });
            if !progress(end as f32 / shape.0 as f32) {
                return Err(Cancelled);
            }
        }
        let values = Array2::from_shape_vec(shape, values).map_err(|e| Invalid(e.to_string()))?;
        let states = Array2::from_shape_vec(shape, states).map_err(|e| Invalid(e.to_string()))?;
        let heights = Array2::from_shape_vec(shape, heights).map_err(|e| Invalid(e.to_string()))?;
        let field = match request.product {
            Product::Cappi { .. } => FieldId::Cappi,
            Product::CompositeReflectivity => FieldId::CompositeReflectivity,
            Product::EchoTop { .. } => FieldId::EchoTop,
            Product::Vil { .. } => FieldId::VerticallyIntegratedLiquid,
            Product::VilDensity { .. } => FieldId::VilDensity,
        };
        GeoGrid::new(request.grid, field, values, states, heights, self.sweeps[indices[0]].info.time_unix_ms)
    }
}

fn cappi_sample(column: &[(f32, f32, CellState)], height_m: f64, max_gap_m: f64) -> (f32, CellState) {
    let upper = column.partition_point(|p| f64::from(p.1) < height_m);
    if let Some(&(value, height, state)) = column.get(upper)
        && (f64::from(height) - height_m).abs() < 0.01
    {
        return (value, state);
    }
    let Some(lower) = upper.checked_sub(1).and_then(|i| column.get(i)) else {
        return (0.0, CellState::OutsideCoverage);
    };
    let Some(higher) = column.get(upper) else {
        return (0.0, CellState::OutsideCoverage);
    };
    if lower.2 != CellState::Valid {
        return (0.0, lower.2);
    }
    if higher.2 != CellState::Valid {
        return (0.0, higher.2);
    }
    let gap = f64::from(higher.1) - f64::from(lower.1);
    if gap <= 0.0 || gap > max_gap_m {
        return (0.0, CellState::Rejected);
    }
    let t = ((height_m - f64::from(lower.1)) / gap) as f32;
    (lower.0 + t * (higher.0 - lower.0), CellState::Valid)
}

fn reduce_column(c: &[(f32, f32)], product: Product, ground_m: f64) -> (f32, f32) {
    if product == Product::CompositeReflectivity {
        return c.iter().copied().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap_or((0.0, 0.0));
    }
    let threshold = match product {
        Product::EchoTop { threshold_dbz }
        | Product::Vil { threshold_dbz, .. }
        | Product::VilDensity { threshold_dbz } => threshold_dbz,
        _ => 0.0,
    };
    let Some(first) = c.iter().position(|p| p.0 >= threshold) else {
        return (0.0, 0.0);
    };
    let last = c.iter().rposition(|p| p.0 >= threshold).unwrap_or(first);
    if let Product::EchoTop { .. } = product {
        let (z, h) = c[last];
        let top = if let Some(&(z2, h2)) = c.get(last + 1) { h + (h2 - h) * (z - threshold) / (z - z2) } else { h };
        return (top, top);
    }
    let z = |dbz: f32| 10.0_f64.powf(dbz as f64 / 10.0);
    let coefficient = 3.44e-6;
    let mut integral = 0.0;
    for pair in c[..=last].windows(2) {
        let dz = f64::from(pair[1].1 - pair[0].1);
        integral += coefficient * ((z(pair[0].0) + z(pair[1].0)) * 0.5).powf(4.0 / 7.0) * dz;
    }
    let value = match product {
        Product::Vil { beam_width_deg, .. } => {
            let half = ground_m * (beam_width_deg.to_radians() / 2.0).sin();
            integral + coefficient * (z(c[first].0).powf(4.0 / 7.0) + z(c[last].0).powf(4.0 / 7.0)) * half
        }
        Product::VilDensity { .. } => {
            if first == last {
                0.0
            } else {
                integral / f64::from(c[last].1 - c[first].1) * 1000.0
            }
        }
        _ => 0.0,
    };
    (value as f32, c[last].1)
}

pub fn mosaic(grids: &[&GeoGrid]) -> Result<GeoGrid> {
    let first = grids.first().ok_or_else(|| Invalid("empty mosaic".into()))?;
    if !matches!(first.field(), FieldId::Reflectivity | FieldId::CompositeReflectivity | FieldId::Cappi)
        || grids.iter().any(|g| g.spec() != first.spec() || g.field() != first.field())
    {
        return Err(Invalid("mosaic needs identical REF/CR geographic grids".into()));
    }
    let mut values = Array2::zeros(first.values().dim());
    let mut heights = values.clone();
    let mut states = Array2::from_elem(values.dim(), CellState::OutsideCoverage);
    for g in grids {
        for (i, &s) in g.states().indexed_iter() {
            if s == CellState::Valid && (states[i] != CellState::Valid || g.values()[i] > values[i]) {
                values[i] = g.values()[i];
                heights[i] = g.heights_m()[i];
                states[i] = s;
            } else if states[i] == CellState::OutsideCoverage && s != CellState::OutsideCoverage {
                states[i] = s;
            }
        }
    }
    GeoGrid::new(
        first.spec(),
        first.field(),
        values,
        states,
        heights,
        grids.iter().map(|g| g.time_unix_ms()).max().unwrap_or(0),
    )
}

#[derive(Debug)]
pub struct GeoSection {
    pub points: Vec<GeoPoint>,
    pub distances_m: Vec<f64>,
    pub heights_m: Vec<f64>,
    pub values: Array2<f32>,
    pub states: Array2<CellState>,
    pub field: FieldId,
}

pub fn section(
    dataset: &RadarDataset,
    field: FieldId,
    path: &[GeoPoint],
    step_m: f64,
    heights_m: &[f64],
    max_vertical_error_m: f64,
    beam: BeamModel,
) -> Result<GeoSection> {
    if path.len() < 2
        || heights_m.is_empty()
        || !step_m.is_finite()
        || step_m <= 0.0
        || !max_vertical_error_m.is_finite()
        || max_vertical_error_m < 0.0
        || heights_m.iter().any(|h| !h.is_finite())
        || heights_m.windows(2).any(|h| h[1] <= h[0])
    {
        return Err(Invalid("invalid geographic section parameters".into()));
    }
    let mut points = vec![path[0]];
    let mut distances = vec![0.0];
    let mut total = 0.0;
    for segment in path.windows(2) {
        let (d, a) = projection::distance_bearing(segment[0], segment[1]);
        if d <= 1e-6 {
            continue;
        }
        let n = (d / step_m).ceil().max(1.0) as usize;
        if n > 1_000_000 || (points.len() + n).checked_mul(heights_m.len()).is_none_or(|n| n > 16_000_000) {
            return Err(Invalid("oversized section".into()));
        }
        for k in 1..=n {
            let s = d * k as f64 / n as f64;
            points.push(projection::destination(segment[0], a, s)?);
            distances.push(total + s);
        }
        total += d;
    }
    if points.len() < 2 {
        return Err(Invalid("section path has zero length".into()));
    }
    let indices: Vec<_> = dataset
        .summary()
        .sweeps
        .iter()
        .filter(|s| s.fields.iter().any(|f| f.field == field))
        .map(|s| s.index)
        .collect();
    if indices.is_empty() {
        return Err(MissingField { sweep: 0, field });
    }
    let shape = (heights_m.len(), points.len());
    let mut values = Array2::zeros(shape);
    let mut states = Array2::from_elem(shape, CellState::OutsideCoverage);
    for (col, &point) in points.iter().enumerate() {
        let mut observations = Vec::with_capacity(indices.len());
        for &i in &indices {
            if let Some(s) = dataset.sample(i, field, point, beam)? {
                observations.push(s);
            }
        }
        for (row, &height) in heights_m.iter().enumerate() {
            if let Some(&(v, s, h)) =
                observations.iter().min_by(|a, b| (a.2 as f64 - height).abs().total_cmp(&(b.2 as f64 - height).abs()))
                && (h as f64 - height).abs() <= max_vertical_error_m
            {
                values[(row, col)] = v;
                states[(row, col)] = s;
            }
        }
    }
    Ok(GeoSection { points, distances_m: distances, heights_m: heights_m.to_vec(), values, states, field })
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn reductions_have_analytic_units() {
        let column = [(30.0, 1000.0), (30.0, 2000.0), (10.0, 3000.0)];
        assert_eq!(reduce_column(&column, Product::CompositeReflectivity, 0.0), (30.0, 2000.0));
        assert_eq!(reduce_column(&column, Product::EchoTop { threshold_dbz: 20.0 }, 0.0), (2500.0, 2500.0));
        let expected = 3.44e-6 * 1000.0_f64.powf(4.0 / 7.0) * 1000.0;
        let vil = reduce_column(&column, Product::Vil { threshold_dbz: 20.0, beam_width_deg: 0.0 }, 0.0).0;
        assert!((vil as f64 - expected).abs() < 1e-5);
        assert!(
            (reduce_column(&column, Product::VilDensity { threshold_dbz: 20.0 }, 0.0).0 as f64 - expected).abs() < 1e-5
        );
    }
}

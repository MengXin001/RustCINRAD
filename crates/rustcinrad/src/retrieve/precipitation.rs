use crate::RadarError::{Cancelled, Invalid};
use crate::{projection::BeamModel, *};
use ndarray::Array2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QpeOptions {
    pub zr_a: f64,
    pub zr_b: f64,
    pub minimum_dbz: f32,
}

impl Default for QpeOptions {
    fn default() -> Self {
        Self { zr_a: 200.0, zr_b: 1.6, minimum_dbz: 0.0 }
    }
}

pub fn rain_rate(grid: &GeoGrid, options: QpeOptions) -> Result<GeoGrid> {
    rain_rate_with_progress(grid, options, |_| true)
}

pub fn rain_rate_with_progress(
    grid: &GeoGrid,
    options: QpeOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<GeoGrid> {
    if !matches!(
        grid.field(),
        FieldId::Reflectivity | FieldId::TotalReflectivity | FieldId::CompositeReflectivity | FieldId::Cappi
    ) || !options.zr_a.is_finite()
        || options.zr_a <= 0.0
        || !options.zr_b.is_finite()
        || options.zr_b <= 0.0
        || !options.minimum_dbz.is_finite()
    {
        return Err(Invalid("QPE needs reflectivity and finite positive Z-R coefficients".into()));
    }
    if !progress(0.0) {
        return Err(Cancelled);
    }
    let mut values = Array2::zeros(grid.values().dim());
    for (row, mut output) in values.outer_iter_mut().enumerate() {
        for (col, value) in output.iter_mut().enumerate() {
            if grid.states()[(row, col)] == CellState::Valid && grid.values()[(row, col)] >= options.minimum_dbz {
                let rate = (10.0_f64.powf(f64::from(grid.values()[(row, col)]) / 10.0) / options.zr_a)
                    .powf(1.0 / options.zr_b);
                if !rate.is_finite() || rate > f64::from(f32::MAX) {
                    return Err(Invalid("QPE rate overflow".into()));
                }
                *value = rate as f32;
            }
        }
        if !progress((row + 1) as f32 / grid.spec().height() as f32) {
            return Err(Cancelled);
        }
    }
    GeoGrid::new(
        grid.spec(),
        FieldId::RainRate,
        values,
        grid.states().to_owned(),
        grid.heights_m().to_owned(),
        grid.time_unix_ms(),
    )
}

impl RadarDataset {
    pub fn rain_rate_grid(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: QpeOptions,
    ) -> Result<GeoGrid> {
        self.rain_rate_grid_with_progress(sweep, spec, beam, options, |_| true)
    }

    pub fn rain_rate_grid_with_progress(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: QpeOptions,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        let reflectivity = self.grid_with_progress(sweep, FieldId::Reflectivity, spec, beam, |p| progress(0.9 * p))?;
        rain_rate_with_progress(&reflectivity, options, |p| progress(0.9 + 0.1 * p))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccumulationOptions {
    pub maximum_gap_seconds: f64,
    pub minimum_coverage_fraction: f64,
}

impl Default for AccumulationOptions {
    fn default() -> Self {
        Self { maximum_gap_seconds: 900.0, minimum_coverage_fraction: 1.0 }
    }
}

#[derive(Debug)]
pub struct RainAccumulation {
    pub depth: GeoGrid,
    pub coverage_seconds: Array2<f64>,
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
}

pub fn accumulate(rates: &[&GeoGrid], options: AccumulationOptions) -> Result<RainAccumulation> {
    accumulate_with_progress(rates, options, |_| true)
}

pub fn accumulate_with_progress(
    rates: &[&GeoGrid],
    options: AccumulationOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<RainAccumulation> {
    if rates.len() < 2
        || !options.maximum_gap_seconds.is_finite()
        || options.maximum_gap_seconds <= 0.0
        || !options.minimum_coverage_fraction.is_finite()
        || !(0.0..=1.0).contains(&options.minimum_coverage_fraction)
    {
        return Err(Invalid("invalid rainfall time integration parameters".into()));
    }
    let first = rates[0];
    if rates.iter().any(|g| {
        g.spec() != first.spec()
            || g.field() != FieldId::RainRate
            || g.values().iter().zip(g.states()).any(|(&v, &s)| s == CellState::Valid && v < 0.0)
    }) || rates.windows(2).any(|g| g[1].time_unix_ms() <= g[0].time_unix_ms())
    {
        return Err(Invalid("rainfall needs aligned nonnegative rain rates in strictly increasing time order".into()));
    }
    let end = rates[rates.len() - 1].time_unix_ms();
    let duration_ms = end.checked_sub(first.time_unix_ms()).ok_or_else(|| Invalid("time interval overflow".into()))?;
    let duration = duration_ms as f64 / 1000.0;
    let mut sums = Array2::<f64>::zeros(first.values().dim());
    let mut coverage = sums.clone();
    let mut states = Array2::from_elem(sums.dim(), CellState::OutsideCoverage);
    if !progress(0.0) {
        return Err(Cancelled);
    }
    for (index, pair) in rates.windows(2).enumerate() {
        let seconds = (pair[1].time_unix_ms() - pair[0].time_unix_ms()) as f64 / 1000.0;
        for ((row, col), sum) in sums.indexed_iter_mut() {
            let a = pair[0].states()[(row, col)];
            let b = pair[1].states()[(row, col)];
            if a != CellState::OutsideCoverage || b != CellState::OutsideCoverage {
                states[(row, col)] = CellState::Missing;
            }
            if a == CellState::Valid && b == CellState::Valid && seconds <= options.maximum_gap_seconds {
                *sum +=
                    (f64::from(pair[0].values()[(row, col)]) + f64::from(pair[1].values()[(row, col)])) * 0.5 * seconds
                        / 3600.0;
                coverage[(row, col)] += seconds;
            }
            if col == 0
                && row % 16 == 0
                && !progress((index as f32 + row as f32 / first.spec().height() as f32) / (rates.len() - 1) as f32)
            {
                return Err(Cancelled);
            }
        }
    }
    for (i, &seconds) in coverage.indexed_iter() {
        if seconds > 0.0 {
            states[i] = if seconds / duration + 1e-12 >= options.minimum_coverage_fraction {
                CellState::Valid
            } else {
                CellState::Rejected
            };
        }
        if sums[i] > f64::from(f32::MAX) {
            return Err(Invalid("rainfall accumulation overflow".into()));
        }
    }
    if !progress(1.0) {
        return Err(Cancelled);
    }
    let depth = GeoGrid::new(
        first.spec(),
        FieldId::Rainfall,
        sums.mapv(|v| v as f32),
        states,
        first.heights_m().to_owned(),
        end,
    )?;
    Ok(RainAccumulation { depth, coverage_seconds: coverage, start_unix_ms: first.time_unix_ms(), end_unix_ms: end })
}

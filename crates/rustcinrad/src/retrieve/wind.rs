use crate::RadarError::{Cancelled, Invalid, MissingField};
use crate::{
    io::base::{Ray, Sweep},
    projection::{self, BeamModel},
    *,
};
use nalgebra::{DMatrix, DVector};
use ndarray::Array2;

#[derive(Debug, Clone, Copy)]
pub struct WindOptions {
    pub minimum_samples: usize,
    pub maximum_condition: f64,
    pub maximum_rmse_mps: f64,
    pub robust_iterations: usize,
}

impl Default for WindOptions {
    fn default() -> Self {
        Self { minimum_samples: 16, maximum_condition: 1000.0, maximum_rmse_mps: 10.0, robust_iterations: 5 }
    }
}

impl WindOptions {
    fn validate(self) -> Result<()> {
        if !(4..=1_000_000).contains(&self.minimum_samples)
            || !self.maximum_condition.is_finite()
            || self.maximum_condition < 1.0
            || !self.maximum_rmse_mps.is_finite()
            || self.maximum_rmse_mps <= 0.0
            || self.robust_iterations > 10
        {
            return Err(Invalid("invalid wind fit thresholds".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HorizontalWind {
    pub u_mps: f64,
    pub v_mps: f64,
    pub radial_offset_mps: f64,
    pub rmse_mps: f64,
    pub condition: f64,
    pub samples: usize,
}

impl HorizontalWind {
    pub fn speed_mps(self) -> f64 {
        self.u_mps.hypot(self.v_mps)
    }

    pub fn direction_deg(self) -> Option<f64> {
        (self.speed_mps() > 1e-9).then(|| (-self.u_mps).atan2(-self.v_mps).to_degrees().rem_euclid(360.0))
    }
}

#[derive(Clone, Copy)]
struct Sample {
    direction: [f64; 3],
    velocity: f64,
}

struct Fit {
    coefficients: [f64; 3],
    rmse: f64,
    condition: f64,
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_unstable_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) { (values[mid - 1] + values[mid]) / 2.0 } else { values[mid] }
}

fn solve(samples: &[Sample], options: WindOptions, minimum: usize) -> Option<Fit> {
    if samples.len() < minimum {
        return None;
    }
    let mut weights = vec![1.0_f64; samples.len()];
    let mut fit = None;
    for iteration in 0..=options.robust_iterations {
        let design = DMatrix::from_fn(samples.len(), 3, |row, col| samples[row].direction[col] * weights[row].sqrt());
        let response =
            DVector::from_iterator(samples.len(), samples.iter().zip(&weights).map(|(s, w)| s.velocity * w.sqrt()));
        let svd = design.svd(true, true);
        let largest = svd.singular_values.max();
        let smallest = svd.singular_values.min();
        let condition = largest / smallest;
        if !condition.is_finite() || condition > options.maximum_condition {
            return None;
        }
        let coefficients = svd.solve(&response, largest * 1e-12).ok()?;
        if coefficients.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let mut residuals: Vec<f64> = samples
            .iter()
            .map(|s| s.velocity - (0..3).map(|i| s.direction[i] * coefficients[i]).sum::<f64>())
            .collect();
        let rmse = (residuals.iter().map(|r| r * r).sum::<f64>() / samples.len() as f64).sqrt();
        fit = Some(Fit { coefficients: [coefficients[0], coefficients[1], coefficients[2]], rmse, condition });
        if iteration == options.robust_iterations {
            break;
        }
        let center = median(&mut residuals);
        for residual in &mut residuals {
            *residual = (*residual - center).abs();
        }
        let scale = 1.4826 * median(&mut residuals);
        if scale < 1e-6 {
            break;
        }
        for (weight, sample) in weights.iter_mut().zip(samples) {
            let residual = sample.velocity - (0..3).map(|i| sample.direction[i] * coefficients[i]).sum::<f64>();
            *weight = (1.5 * scale / residual.abs()).min(1.0);
        }
    }
    fit.filter(|f| f.rmse <= options.maximum_rmse_mps)
}

fn horizontal(samples: &[Sample], options: WindOptions) -> Option<HorizontalWind> {
    solve(samples, options, options.minimum_samples).map(|fit| HorizontalWind {
        u_mps: fit.coefficients[0],
        v_mps: fit.coefficients[1],
        radial_offset_mps: fit.coefficients[2],
        rmse_mps: fit.rmse,
        condition: fit.condition,
        samples: samples.len(),
    })
}

fn scan(dataset: &RadarDataset, sweep: usize) -> Result<&Sweep> {
    dataset
        .sweeps
        .get(sweep)
        .filter(|s| s.info.fields.iter().any(|f| f.field == FieldId::Velocity))
        .ok_or(MissingField { sweep, field: FieldId::Velocity })
}

fn velocity(ray: &Ray, range: f64) -> Option<f64> {
    let moment = ray.moments.get(&FieldId::Velocity)?;
    let index = ((range - moment.first_m) / moment.spacing_m).round();
    if index < 0.0 || index >= moment.values.len() as f64 {
        return None;
    }
    (moment.states[index as usize] == CellState::Valid).then(|| f64::from(moment.values[index as usize]))
}

fn direction(azimuth: f64, elevation: f64) -> [f64; 3] {
    let (a, e) = (azimuth.to_radians(), elevation.to_radians());
    [a.sin() * e.cos(), a.cos() * e.cos(), 1.0]
}

impl RadarDataset {
    pub fn vad(
        &self,
        sweep: usize,
        ground_distance_m: f64,
        beam: BeamModel,
        options: WindOptions,
    ) -> Result<Option<HorizontalWind>> {
        options.validate()?;
        if !ground_distance_m.is_finite() || ground_distance_m <= 0.0 {
            return Err(Invalid("VAD ring distance must be positive metres".into()));
        }
        let scan = scan(self, sweep)?;
        let mut samples = Vec::with_capacity(scan.rays.len());
        let mut angles = Vec::with_capacity(scan.rays.len());
        for ray in &scan.rays {
            if let Some(range) = beam.slant(ground_distance_m, ray.elevation)
                && let Some(v) = velocity(ray, range)
            {
                samples.push(Sample { direction: direction(ray.azimuth, ray.elevation), velocity: v });
                angles.push(ray.azimuth.rem_euclid(360.0));
            }
        }
        angles.sort_unstable_by(f64::total_cmp);
        if angles.len() < options.minimum_samples {
            return Ok(None);
        }
        let max_gap =
            angles.windows(2).map(|p| p[1] - p[0]).fold(angles[0] + 360.0 - angles[angles.len() - 1], f64::max);
        if 360.0 - max_gap < 240.0 || samples.len() as f64 / (scan.rays.len().max(1) as f64) < 0.5 {
            return Ok(None);
        }
        Ok(horizontal(&samples, options))
    }

    pub fn vvp(
        &self,
        center: GeoPoint,
        radius_m: f64,
        height_m: f64,
        half_thickness_m: f64,
        beam: BeamModel,
        options: WindOptions,
    ) -> Result<Option<HorizontalWind>> {
        self.vvp_with_progress(center, radius_m, height_m, half_thickness_m, beam, options, |_| true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn vvp_with_progress(
        &self,
        center: GeoPoint,
        radius_m: f64,
        height_m: f64,
        half_thickness_m: f64,
        beam: BeamModel,
        options: WindOptions,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<Option<HorizontalWind>> {
        options.validate()?;
        if ![radius_m, height_m, half_thickness_m].iter().all(|v| v.is_finite())
            || radius_m <= 0.0
            || half_thickness_m <= 0.0
        {
            return Err(Invalid("invalid VVP geographic neighbourhood".into()));
        }
        if !self.sweeps.iter().any(|s| s.info.fields.iter().any(|f| f.field == FieldId::Velocity)) {
            return Err(MissingField { sweep: 0, field: FieldId::Velocity });
        }
        let total = self.sweeps.iter().map(|s| s.rays.len()).sum::<usize>().max(1);
        let mut done = 0;
        let mut samples = Vec::new();
        if !progress(0.0) {
            return Err(Cancelled);
        }
        let (center_range, _) = projection::distance_bearing(self.summary.site.position, center);
        for sweep in &self.sweeps {
            for ray in &sweep.rays {
                if let Some(moment) = ray.moments.get(&FieldId::Velocity) {
                    for (i, &value) in moment.values.iter().enumerate() {
                        if moment.states[i] != CellState::Valid {
                            continue;
                        }
                        let range = moment.first_m + i as f64 * moment.spacing_m;
                        let (ground, height) = beam.locate(range, ray.elevation, self.summary.site.altitude_m)?;
                        if (height - height_m).abs() > half_thickness_m || (ground - center_range).abs() > radius_m {
                            continue;
                        }
                        let location = projection::destination(self.summary.site.position, ray.azimuth, ground)?;
                        if projection::distance_bearing(center, location).0 > radius_m {
                            continue;
                        }
                        if samples.len() >= 1_000_000 {
                            return Err(Invalid("VVP sample budget exceeded".into()));
                        }
                        samples.push(Sample {
                            direction: direction(ray.azimuth, ray.elevation),
                            velocity: f64::from(value),
                        });
                    }
                }
                done += 1;
                if !progress(done as f32 / total as f32) {
                    return Err(Cancelled);
                }
            }
        }
        Ok(horizontal(&samples, options))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn vwp_with_progress(
        &self,
        center: GeoPoint,
        radius_m: f64,
        heights_m: &[f64],
        half_thickness_m: f64,
        beam: BeamModel,
        options: WindOptions,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<WindProfile> {
        if heights_m.is_empty()
            || heights_m.len() > 1000
            || heights_m.iter().any(|h| !h.is_finite())
            || heights_m.windows(2).any(|h| h[1] <= h[0])
        {
            return Err(Invalid("invalid wind-profile heights".into()));
        }
        let mut winds = Vec::with_capacity(heights_m.len());
        for (i, &height) in heights_m.iter().enumerate() {
            winds.push(self.vvp_with_progress(center, radius_m, height, half_thickness_m, beam, options, |p| {
                progress((i as f32 + p) / heights_m.len() as f32)
            })?);
        }
        Ok(WindProfile {
            position: center,
            heights_m: heights_m.to_vec(),
            winds,
            time_unix_ms: self.summary.sweeps.first().map_or(0, |s| s.time_unix_ms),
        })
    }
}

#[derive(Debug)]
pub struct WindProfile {
    pub position: GeoPoint,
    pub heights_m: Vec<f64>,
    pub winds: Vec<Option<HorizontalWind>>,
    pub time_unix_ms: i64,
}

#[derive(Debug)]
pub struct DopplerGrid {
    pub velocity: GeoGrid,
    pub look_enu: Array2<[f64; 3]>,
}

#[derive(Debug)]
pub struct WindGrid {
    pub eastward: GeoGrid,
    pub northward: GeoGrid,
    pub upward: GeoGrid,
    pub rmse_mps: Array2<f64>,
    pub condition: Array2<f64>,
}

pub fn wind_grid_with_progress(
    inputs: &[&DopplerGrid],
    maximum_time_skew_ms: i64,
    maximum_height_skew_m: f64,
    options: WindOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<WindGrid> {
    options.validate()?;
    let first = inputs.first().ok_or_else(|| Invalid("empty multi-Doppler input".into()))?;
    if inputs.len() < 3
        || inputs.len() > 64
        || maximum_time_skew_ms < 0
        || !maximum_height_skew_m.is_finite()
        || maximum_height_skew_m < 0.0
    {
        return Err(Invalid("invalid multi-Doppler limits".into()));
    }
    let earliest = inputs.iter().map(|g| g.velocity.time_unix_ms()).min().unwrap_or(0);
    let latest = inputs.iter().map(|g| g.velocity.time_unix_ms()).max().unwrap_or(0);
    if latest.checked_sub(earliest).is_none_or(|dt| dt > maximum_time_skew_ms) {
        return Err(Invalid("multi-Doppler observations exceed time tolerance".into()));
    }
    let spec = first.velocity.spec();
    let shape = (spec.height(), spec.width());
    for input in inputs {
        if input.velocity.spec() != spec
            || input.velocity.field() != FieldId::Velocity
            || input.look_enu.dim() != shape
            || input
                .velocity
                .time_unix_ms()
                .checked_sub(first.velocity.time_unix_ms())
                .is_none_or(|dt| dt.unsigned_abs() > maximum_time_skew_ms as u64)
            || input.look_enu.iter().zip(input.velocity.states()).any(|(v, s)| {
                *s == CellState::Valid
                    && (v.iter().any(|x| !x.is_finite()) || (v.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() > 1e-6)
            })
        {
            return Err(Invalid("incompatible multi-Doppler grids/time/unit vectors".into()));
        }
    }
    let mut u = Array2::zeros(shape);
    let mut v = Array2::zeros(shape);
    let mut w = Array2::zeros(shape);
    let mut states = Array2::from_elem(shape, CellState::OutsideCoverage);
    let mut heights = Array2::zeros(shape);
    let mut rmse = Array2::zeros(shape);
    let mut condition = Array2::zeros(shape);
    let mut samples = Vec::with_capacity(inputs.len());
    if !progress(0.0) {
        return Err(Cancelled);
    }
    for row in 0..spec.height() {
        for col in 0..spec.width() {
            let index = (row, col);
            samples.clear();
            let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
            for input in inputs {
                if input.velocity.states()[index] == CellState::Valid {
                    samples.push(Sample {
                        direction: input.look_enu[index],
                        velocity: f64::from(input.velocity.values()[index]),
                    });
                    low = low.min(input.velocity.heights_m()[index]);
                    high = high.max(input.velocity.heights_m()[index]);
                }
            }
            if samples.is_empty() {
                continue;
            }
            states[index] = CellState::Rejected;
            if f64::from(high) - f64::from(low) > maximum_height_skew_m {
                continue;
            }
            if let Some(fit) = solve(&samples, options, 3)
                && fit.coefficients.iter().all(|x| x.abs() <= f64::from(f32::MAX))
            {
                u[index] = fit.coefficients[0] as f32;
                v[index] = fit.coefficients[1] as f32;
                w[index] = fit.coefficients[2] as f32;
                states[index] = CellState::Valid;
                heights[index] = (f64::from(low) + (f64::from(high) - f64::from(low)) / 2.0) as f32;
                rmse[index] = fit.rmse;
                condition[index] = fit.condition;
            }
        }
        if !progress((row + 1) as f32 / spec.height() as f32) {
            return Err(Cancelled);
        }
    }
    let time = first.velocity.time_unix_ms();
    Ok(WindGrid {
        eastward: GeoGrid::new(spec, FieldId::EastwardWind, u, states.clone(), heights.clone(), time)?,
        northward: GeoGrid::new(spec, FieldId::NorthwardWind, v, states.clone(), heights.clone(), time)?,
        upward: GeoGrid::new(spec, FieldId::UpwardWind, w, states, heights, time)?,
        rmse_mps: rmse,
        condition,
    })
}

#[derive(Debug)]
pub struct WindVolume {
    pub heights_m: Vec<f64>,
    pub layers: Vec<WindGrid>,
}

#[allow(clippy::too_many_arguments)]
pub fn wind_volume_with_progress(
    heights_m: &[f64],
    inputs: &[Vec<&DopplerGrid>],
    maximum_time_skew_ms: i64,
    maximum_height_skew_m: f64,
    options: WindOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<WindVolume> {
    if heights_m.is_empty()
        || heights_m.len() != inputs.len()
        || heights_m.iter().any(|h| !h.is_finite())
        || heights_m.windows(2).any(|h| h[1] <= h[0])
    {
        return Err(Invalid("invalid 3D wind heights".into()));
    }
    let first = inputs[0].first().ok_or_else(|| Invalid("empty 3D wind layer".into()))?;
    let spec = first.velocity.spec();
    if spec
        .width()
        .checked_mul(spec.height())
        .and_then(|n| n.checked_mul(heights_m.len()))
        .is_none_or(|n| n > 4_000_000)
        || inputs.iter().flatten().any(|g| g.velocity.spec() != spec)
    {
        return Err(Invalid("misaligned or oversized 3D wind volume".into()));
    }
    let mut layers = Vec::with_capacity(inputs.len());
    for (i, layer) in inputs.iter().enumerate() {
        if layer.iter().any(|g| {
            g.velocity
                .heights_m()
                .iter()
                .zip(g.velocity.states())
                .any(|(h, s)| *s == CellState::Valid && (f64::from(*h) - heights_m[i]).abs() > maximum_height_skew_m)
        }) {
            return Err(Invalid("wind layer does not match requested MSL height".into()));
        }
        layers.push(wind_grid_with_progress(layer, maximum_time_skew_ms, maximum_height_skew_m, options, |p| {
            progress((i as f32 + p) / inputs.len() as f32)
        })?);
    }
    Ok(WindVolume { heights_m: heights_m.to_vec(), layers })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    #[test]
    fn horizontal_fit_recovers_analytic_wind_and_rejects_rank_deficiency() {
        let samples: Vec<_> = (0..360)
            .step_by(5)
            .map(|a| {
                let direction = direction(f64::from(a), 2.0);
                Sample { direction, velocity: direction[0] * 12.0 + direction[1] * (-7.0) + 0.8 }
            })
            .collect();
        let wind = horizontal(&samples, WindOptions::default()).unwrap();
        assert!((wind.u_mps - 12.0).abs() < 1e-10 && (wind.v_mps + 7.0).abs() < 1e-10);
        assert!((wind.radial_offset_mps - 0.8).abs() < 1e-10);
        assert!(wind.direction_deg().is_some_and(|d| (d - 300.2564371635).abs() < 1e-6));
        assert!(horizontal(&vec![samples[0]; 100], WindOptions::default()).is_none());
    }

    fn synthetic_velocity_volume() -> RadarDataset {
        use crate::io::base::Moment;
        use std::collections::BTreeMap;
        let info = SweepInfo {
            index: 0,
            elevation_deg: 2.0,
            ray_count: 72,
            fields: vec![FieldInfo { field: FieldId::Velocity, range_resolution_m: 1000.0, maximum_range_m: 15000.0 }],
            nyquist_velocity_mps: Some(30.0),
            time_unix_ms: 100,
            complete: true,
        };
        let rays = (0..360)
            .step_by(5)
            .map(|a| {
                let d = direction(f64::from(a), 2.0);
                let value = (d[0] * 12.0 - d[1] * 7.0 + 0.8) as f32;
                Ray {
                    azimuth: f64::from(a),
                    elevation: 2.0,
                    moments: BTreeMap::from([(
                        FieldId::Velocity,
                        Moment {
                            first_m: 10000.0,
                            spacing_m: 1000.0,
                            values: vec![value; 6],
                            states: vec![CellState::Valid; 6],
                        },
                    )]),
                }
            })
            .collect();
        RadarDataset {
            summary: DatasetSummary {
                format: RadarFormat::Fmt,
                task: "synthetic".into(),
                site: RadarSite {
                    code: "test".into(),
                    name: "synthetic".into(),
                    position: GeoPoint::new(120.0, 35.0).unwrap(),
                    altitude_m: 50.0,
                },
                sweeps: vec![info.clone()],
            },
            sweeps: vec![Sweep { info, rays, angular_width: 5.0 }],
            rasters: vec![],
            source: None,
        }
    }

    #[test]
    fn native_vad_vvp_profile_and_cancellation() {
        let data = synthetic_velocity_volume();
        let beam = BeamModel::default();
        let ground = beam.locate(12000.0, 2.0, 50.0).unwrap().0;
        let vad = data.vad(0, ground, beam, WindOptions::default()).unwrap().unwrap();
        assert!((vad.u_mps - 12.0).abs() < 1e-5 && (vad.v_mps + 7.0).abs() < 1e-5);
        let vvp = data
            .vvp(data.summary.site.position, 20000.0, 500.0, 1000.0, beam, WindOptions::default())
            .unwrap()
            .unwrap();
        assert!((vvp.u_mps - 12.0).abs() < 1e-5 && (vvp.v_mps + 7.0).abs() < 1e-5);
        let profile = data
            .vwp_with_progress(
                data.summary.site.position,
                20000.0,
                &[500.0, 1500.0],
                1000.0,
                beam,
                WindOptions::default(),
                |_| true,
            )
            .unwrap();
        assert_eq!(profile.heights_m.len(), 2);
        assert!(profile.winds[0].is_some());
        assert!(matches!(
            data.vvp_with_progress(
                data.summary.site.position,
                20000.0,
                500.0,
                1000.0,
                beam,
                WindOptions::default(),
                |_| false
            ),
            Err(Cancelled)
        ));
        assert!(data.vad(0, -1.0, beam, WindOptions::default()).is_err());
        let mut sector = synthetic_velocity_volume();
        sector.sweeps[0].rays.truncate(30);
        assert!(sector.vad(0, ground, beam, WindOptions::default()).unwrap().is_none());
    }
}

use super::RadarBand;
use crate::RadarError::{Cancelled, Invalid, MissingField, Unsupported};
use crate::{
    io::base::{Moment, Ray, Sweep},
    projection::BeamModel,
    *,
};
use std::collections::BTreeMap;

/// Conservative dual-polarization processing before geographic resampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarimetricOptions {
    /// Reject gates with lower copolar correlation.
    pub minimum_rho: f32,
    /// Reject gates with lower reflectivity, dBZ.
    pub minimum_dbz: f32,
    /// Optional five-gate circular PhiDP texture limit, degrees.
    pub maximum_phase_texture_deg: Option<f32>,
    /// Derive KDP from unwrapped, median/mean-smoothed PhiDP instead of native KDP.
    pub derive_kdp: bool,
    /// Optional KDP-driven two-way attenuation correction of REF or ZDR.
    pub attenuation_band: Option<RadarBand>,
}

impl Default for PolarimetricOptions {
    fn default() -> Self {
        Self {
            minimum_rho: 0.85,
            minimum_dbz: 0.0,
            maximum_phase_texture_deg: Some(20.0),
            derive_kdp: false,
            attenuation_band: None,
        }
    }
}

fn nearest(moment: &Moment, range_m: f64) -> Option<(f32, CellState)> {
    let gate = ((range_m - moment.first_m) / moment.spacing_m).round();
    if gate < 0.0 || gate >= moment.values.len() as f64 {
        return None;
    }
    let index = gate as usize;
    Some((moment.values[index], moment.states[index]))
}

fn valid_at(ray: &Ray, field: FieldId, range_m: f64) -> Option<f32> {
    let (value, state) = nearest(ray.moments.get(&field)?, range_m)?;
    (state == CellState::Valid).then_some(value)
}

fn coefficient(band: RadarBand, field: FieldId) -> f64 {
    match (band, field) {
        (RadarBand::S, FieldId::DifferentialReflectivity) => 0.002,
        (RadarBand::C, FieldId::DifferentialReflectivity) => 0.02,
        (RadarBand::X, FieldId::DifferentialReflectivity) => 0.04,
        (RadarBand::S, _) => 0.02,
        (RadarBand::C, _) => 0.08,
        (RadarBand::X, _) => 0.28,
    }
}

fn contiguous_window(states: &[CellState], index: usize, radius: usize) -> std::ops::Range<usize> {
    let mut left = index;
    let mut right = index + 1;
    while left > index.saturating_sub(radius) && states[left - 1] == CellState::Valid {
        left -= 1;
    }
    while right < states.len().min(index + radius + 1) && states[right] == CellState::Valid {
        right += 1;
    }
    left..right
}

fn phase_and_kdp(phase: &[f32], states: &[CellState], spacing_m: f64) -> (Vec<f32>, Vec<f32>, Vec<CellState>) {
    let mut unwrapped = vec![0.0; phase.len()];
    let mut previous: Option<(f32, f32)> = None;
    for (i, &value) in phase.iter().enumerate() {
        if states[i] != CellState::Valid {
            previous = None;
            continue;
        }
        unwrapped[i] = if let Some((last, accumulated)) = previous {
            accumulated + (value - last + 180.0_f32).rem_euclid(360.0) - 180.0
        } else {
            value
        };
        previous = Some((value, unwrapped[i]));
    }
    let mut median = vec![0.0; phase.len()];
    let mut scratch = Vec::with_capacity(5);
    for i in 0..phase.len() {
        if states[i] != CellState::Valid {
            continue;
        }
        scratch.clear();
        scratch.extend_from_slice(&unwrapped[contiguous_window(states, i, 2)]);
        scratch.sort_by(f32::total_cmp);
        let n = scratch.len();
        median[i] = if n % 2 == 0 { (scratch[n / 2 - 1] + scratch[n / 2]) * 0.5 } else { scratch[n / 2] };
    }
    let mut smoothed = vec![0.0; phase.len()];
    for (i, value) in smoothed.iter_mut().enumerate() {
        if states[i] != CellState::Valid {
            continue;
        }
        let window = contiguous_window(states, i, 3);
        *value = median[window.clone()].iter().sum::<f32>() / window.len() as f32;
    }
    let mut kdp = vec![0.0; phase.len()];
    let mut quality = states.to_vec();
    for i in 0..phase.len() {
        if states[i] != CellState::Valid {
            continue;
        }
        let window = contiguous_window(states, i, 3);
        if window.len() < 3 {
            quality[i] = CellState::Rejected;
            continue;
        }
        let mean_x = (window.start + window.end - 1) as f64 * 0.5;
        let mean_y = smoothed[window.clone()].iter().map(|&p| f64::from(p)).sum::<f64>() / window.len() as f64;
        let mut xy = 0.0;
        let mut xx = 0.0;
        for gate in window {
            let x = gate as f64 - mean_x;
            xy += x * (f64::from(smoothed[gate]) - mean_y);
            xx += x * x;
        }
        kdp[i] = (0.5 * xy / xx / (spacing_m / 1000.0)) as f32;
    }
    (smoothed, kdp, quality)
}

impl RadarDataset {
    pub fn polarimetric_grid(
        &self,
        sweep: usize,
        field: FieldId,
        spec: GridSpec,
        beam: BeamModel,
        options: PolarimetricOptions,
    ) -> Result<GeoGrid> {
        self.polarimetric_grid_with_progress(sweep, field, spec, beam, options, |_| true)
    }

    pub fn polarimetric_grid_with_progress(
        &self,
        sweep: usize,
        field: FieldId,
        spec: GridSpec,
        beam: BeamModel,
        options: PolarimetricOptions,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        if !matches!(
            field,
            FieldId::Reflectivity
                | FieldId::DifferentialReflectivity
                | FieldId::DifferentialPhase
                | FieldId::SpecificDifferentialPhase
        ) || !options.minimum_rho.is_finite()
            || !(0.0..=1.0).contains(&options.minimum_rho)
            || !options.minimum_dbz.is_finite()
            || options.maximum_phase_texture_deg.is_some_and(|p| !p.is_finite() || p < 0.0)
            || (options.attenuation_band.is_some()
                && !matches!(field, FieldId::Reflectivity | FieldId::DifferentialReflectivity))
        {
            return Err(Invalid("invalid polarimetric QC/correction request".into()));
        }
        let scan = self.sweeps.get(sweep).ok_or(MissingField { sweep, field })?;
        if scan.rays.is_empty() {
            return Err(Unsupported("polarimetric QC needs native radial observations".into()));
        }
        let source_field = if field == FieldId::SpecificDifferentialPhase && options.derive_kdp {
            FieldId::DifferentialPhase
        } else {
            field
        };
        let mut required = vec![source_field, FieldId::Reflectivity, FieldId::CorrelationCoefficient];
        if options.maximum_phase_texture_deg.is_some() || options.derive_kdp {
            required.push(FieldId::DifferentialPhase);
        }
        if options.attenuation_band.is_some() && !options.derive_kdp {
            required.push(FieldId::SpecificDifferentialPhase);
        }
        for required_field in required {
            if !scan.info.fields.iter().any(|f| f.field == required_field) {
                return Err(MissingField { sweep, field: required_field });
            }
        }
        if !progress(0.0) {
            return Err(Cancelled);
        }
        let mut rays = Vec::with_capacity(scan.rays.len());
        for (ray_index, ray) in scan.rays.iter().enumerate() {
            let Some(source) = ray.moments.get(&source_field) else {
                rays.push(Ray { azimuth: ray.azimuth, elevation: ray.elevation, moments: BTreeMap::new() });
                if !progress((ray_index + 1) as f32 / scan.rays.len() as f32 * 0.5) {
                    return Err(Cancelled);
                }
                continue;
            };
            let mut values = source.values.clone();
            let mut states = source.states.clone();
            let mut phase = vec![0.0; values.len()];
            let mut phase_states = vec![CellState::Missing; values.len()];
            for i in 0..values.len() {
                let range = source.first_m + i as f64 * source.spacing_m;
                if let Some(phi) = valid_at(ray, FieldId::DifferentialPhase, range) {
                    phase[i] = phi;
                    phase_states[i] = CellState::Valid;
                }
                if states[i] != CellState::Valid {
                    continue;
                }
                let good = valid_at(ray, FieldId::Reflectivity, range).is_some_and(|v| v >= options.minimum_dbz)
                    && valid_at(ray, FieldId::CorrelationCoefficient, range).is_some_and(|v| v >= options.minimum_rho);
                if !good {
                    states[i] = CellState::Rejected;
                }
            }
            if let Some(limit) = options.maximum_phase_texture_deg {
                for i in 0..values.len() {
                    if states[i] != CellState::Valid {
                        continue;
                    }
                    if phase_states[i] != CellState::Valid {
                        states[i] = CellState::Rejected;
                        continue;
                    }
                    let window = contiguous_window(&phase_states, i, 2);
                    let mut sum = 0.0_f64;
                    let mut squares = 0.0_f64;
                    let n = window.len() as f64;
                    for j in window {
                        let delta = f64::from((phase[j] - phase[i] + 180.0).rem_euclid(360.0) - 180.0);
                        sum += delta;
                        squares += delta * delta;
                    }
                    if (squares / n - (sum / n).powi(2)).max(0.0).sqrt() > f64::from(limit) {
                        states[i] = CellState::Rejected;
                    }
                }
            }
            for i in 0..states.len() {
                if states[i] != CellState::Valid {
                    phase_states[i] = states[i];
                }
            }
            let derived = (options.derive_kdp || field == FieldId::DifferentialPhase)
                .then(|| phase_and_kdp(&phase, &phase_states, source.spacing_m));
            if field == FieldId::SpecificDifferentialPhase
                && let Some((_, kdp, quality)) = &derived
            {
                values.clone_from(kdp);
                states.clone_from(quality);
            } else if field == FieldId::DifferentialPhase
                && let Some((smoothed, _, _)) = &derived
            {
                values.clone_from(smoothed);
                states.clone_from(&phase_states);
            }
            if let Some(band) = options.attenuation_band {
                let gamma = coefficient(band, field);
                let mut running = 0.0_f64;
                for i in 0..values.len() {
                    if states[i] != CellState::Valid {
                        running = 0.0;
                        continue;
                    }
                    let kdp = if let Some((_, kdp, quality)) = &derived {
                        (quality[i] == CellState::Valid).then_some(kdp[i])
                    } else {
                        valid_at(ray, FieldId::SpecificDifferentialPhase, source.first_m + i as f64 * source.spacing_m)
                    };
                    if let Some(kdp) = kdp {
                        running += 2.0 * gamma * f64::from(kdp.max(0.0)) * source.spacing_m / 1000.0;
                        let corrected = f64::from(values[i]) + running;
                        if !corrected.is_finite() || corrected.abs() > f64::from(f32::MAX) {
                            return Err(Invalid("attenuation overflow".into()));
                        }
                        values[i] = corrected as f32;
                    } else {
                        running = 0.0;
                        states[i] = CellState::Rejected;
                    }
                }
            }
            rays.push(Ray {
                azimuth: ray.azimuth,
                elevation: ray.elevation,
                moments: BTreeMap::from([(
                    field,
                    Moment { spacing_m: source.spacing_m, first_m: source.first_m, values, states },
                )]),
            });
            if !progress(0.2 * (ray_index + 1) as f32 / scan.rays.len() as f32) {
                return Err(Cancelled);
            }
        }
        let temporary = RadarDataset::finish(
            self.summary.format,
            self.summary.site.clone(),
            self.summary.task.clone(),
            vec![Sweep { info: scan.info.clone(), rays, angular_width: scan.angular_width }],
        )?;
        temporary.grid_with_progress(0, field, spec, beam, |p| progress(0.2 + 0.8 * p))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn linear_phase_and_wrapping_yield_analytic_kdp_without_bridging_gaps() {
        let phase: Vec<_> = (0..41).map(|i| (350.0 + i as f32 * 2.0) % 360.0).collect();
        let mut states = vec![CellState::Valid; phase.len()];
        states[20] = CellState::Missing;
        let (unwrapped, kdp, quality) = phase_and_kdp(&phase, &states, 250.0);
        assert!((kdp[10] - 4.0).abs() < 1e-5);
        assert!((kdp[30] - 4.0).abs() < 1e-5);
        assert_eq!(quality[20], CellState::Missing);
        assert!(unwrapped[10] > 360.0);
        assert!(unwrapped[30] < 100.0);
        assert!((coefficient(RadarBand::C, FieldId::Reflectivity) - 0.08).abs() < 1e-12);
    }
}

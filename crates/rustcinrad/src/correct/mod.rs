//! Native radar quality control
use crate::RadarError::{Cancelled, Invalid};

mod hca_params;
mod polarimetric;
mod unwrap;

use crate::{projection::BeamModel, *};
use ndarray::Array2;
pub use polarimetric::PolarimetricOptions;
/// Supported microwave radar band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarBand {
    /// S band.
    S,
    /// C band.
    C,
    /// X band.
    X,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HcaOptions {
    pub band: RadarBand,
    pub weights: [f64; 3],
}

impl Default for HcaOptions {
    fn default() -> Self {
        Self { band: RadarBand::S, weights: [0.8, 1.0, 0.8] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Hydrometeor {
    /// Small liquid drops.
    Drizzle = 1,
    /// Liquid rain.
    Rain,
    /// Ice crystals.
    IceCrystals,
    /// Dry aggregated snow.
    DrySnow,
    /// Wet snow.
    WetSnow,
    /// Vertically oriented ice.
    VerticalIce,
    /// Low density graupel.
    LowDensityGraupel,
    /// High density graupel.
    HighDensityGraupel,
    /// Hail.
    Hail,
    /// Large liquid drops.
    BigDrops,
}

pub fn classify(ref_dbz: f64, zdr_db: f64, kdp_deg_km: f64, rho: f64, options: HcaOptions) -> Result<Hydrometeor> {
    if ![ref_dbz, zdr_db, kdp_deg_km, rho].iter().all(|v| v.is_finite())
        || options.weights.iter().any(|w| !w.is_finite() || *w < 0.0)
        || options.weights.iter().sum::<f64>() <= 0.0
        || !options.weights.iter().sum::<f64>().is_finite()
    {
        return Err(Invalid("invalid hydrometeor observations or weights".into()));
    }
    let params = &hca_params::PARAMS[match options.band {
        RadarBand::S => 0,
        RadarBand::C => 1,
        RadarBand::X => 2,
    }];
    let observations = [ref_dbz, zdr_db, kdp_deg_km, rho];
    let mut best = 0;
    let mut maximum = -1.0;
    for (class, _) in params[0].iter().enumerate() {
        let membership: [f64; 4] = std::array::from_fn(|f| {
            let [m, a, b] = params[f][class];
            1.0 / (1.0 + ((observations[f] - m) / a).abs().powf(2.0 * b))
        });
        let score = membership[0] * (0..3).map(|i| membership[i + 1] * options.weights[i]).sum::<f64>()
            / options.weights.iter().sum::<f64>();
        if score > maximum {
            best = class;
            maximum = score;
        }
    }
    Ok([
        Hydrometeor::Drizzle,
        Hydrometeor::Rain,
        Hydrometeor::IceCrystals,
        Hydrometeor::DrySnow,
        Hydrometeor::WetSnow,
        Hydrometeor::VerticalIce,
        Hydrometeor::LowDensityGraupel,
        Hydrometeor::HighDensityGraupel,
        Hydrometeor::Hail,
        Hydrometeor::BigDrops,
    ][best])
}

pub fn hydrometeor_grid(moments: [&GeoGrid; 4], options: HcaOptions) -> Result<GeoGrid> {
    hydrometeor_grid_with_progress(moments, options, |_| true)
}

pub fn hydrometeor_grid_with_progress(
    moments: [&GeoGrid; 4],
    options: HcaOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<GeoGrid> {
    if !progress(0.0) {
        return Err(Cancelled);
    }
    classify(0.0, 0.0, 0.0, 0.0, options)?;
    let first = moments[0];
    let expected = [
        FieldId::Reflectivity,
        FieldId::DifferentialReflectivity,
        FieldId::SpecificDifferentialPhase,
        FieldId::CorrelationCoefficient,
    ];
    if moments
        .iter()
        .zip(expected)
        .any(|(g, f)| g.field() != f || g.spec() != first.spec() || g.time_unix_ms() != first.time_unix_ms())
    {
        return Err(Invalid("HCA needs aligned REF/ZDR/KDP/RHO from the same scan time".into()));
    }
    let mut values = Array2::zeros(first.values().dim());
    let mut states = Array2::from_elem(values.dim(), CellState::Valid);
    for (i, v) in values.indexed_iter_mut() {
        if i.1 == 0 && !progress(i.0 as f32 / first.spec().height() as f32) {
            return Err(Cancelled);
        }
        if let Some(s) = moments.iter().map(|g| g.states()[i]).find(|s| *s != CellState::Valid) {
            states[i] = s;
            continue;
        }
        *v = classify(
            moments[0].values()[i] as f64,
            moments[1].values()[i] as f64,
            moments[2].values()[i] as f64,
            moments[3].values()[i] as f64,
            options,
        )? as u8 as f32;
    }
    if !progress(1.0) {
        return Err(Cancelled);
    }
    GeoGrid::new(
        first.spec(),
        FieldId::HydrometeorClass,
        values,
        states,
        first.heights_m().to_owned(),
        first.time_unix_ms(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DealiasOptions {
    /// Override missing/incorrect native Nyquist speed, m/s.
    pub nyquist_velocity_mps: Option<f64>,
    /// Connect azimuth endpoints only for a genuinely covered 360-degree sweep.
    pub wrap_azimuth: bool,
}

impl Default for DealiasOptions {
    fn default() -> Self {
        Self { nyquist_velocity_mps: None, wrap_azimuth: true }
    }
}

impl RadarDataset {
    pub fn dealiased_grid(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: DealiasOptions,
    ) -> Result<GeoGrid> {
        self.dealiased_grid_with_progress(sweep, spec, beam, options, |_| true)
    }

    pub fn dealiased_grid_with_progress(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: DealiasOptions,
        progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        unwrap::grid(self, sweep, spec, beam, options, progress)
    }

    pub fn hydrometeor_grid(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: HcaOptions,
    ) -> Result<GeoGrid> {
        self.hydrometeor_grid_with_progress(sweep, spec, beam, options, |_| true)
    }

    pub fn hydrometeor_grid_with_progress(
        &self,
        sweep: usize,
        spec: GridSpec,
        beam: BeamModel,
        options: HcaOptions,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        let refg = self.grid_with_progress(sweep, FieldId::Reflectivity, spec, beam, |p| progress(0.2 * p))?;
        let zdr =
            self.grid_with_progress(sweep, FieldId::DifferentialReflectivity, spec, beam, |p| progress(0.2 + 0.2 * p))?;
        let kdp = self
            .grid_with_progress(sweep, FieldId::SpecificDifferentialPhase, spec, beam, |p| progress(0.4 + 0.2 * p))?;
        let rho =
            self.grid_with_progress(sweep, FieldId::CorrelationCoefficient, spec, beam, |p| progress(0.6 + 0.2 * p))?;
        hydrometeor_grid_with_progress([&refg, &zdr, &kdp, &rho], options, |p| progress(0.8 + 0.2 * p))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {

    use super::*;

    #[test]
    fn matches_scalar_reference_for_all_bands() {
        for (band, expected) in [(RadarBand::S, 2), (RadarBand::C, 7), (RadarBand::X, 2)] {
            assert_eq!(
                classify(30.0, 1.0, 0.1, 0.99, HcaOptions { band, ..Default::default() }).expect("valid scalar") as u8,
                expected
            );
        }
        assert!(classify(f64::NAN, 1.0, 0.1, 0.99, HcaOptions::default()).is_err());
    }
}

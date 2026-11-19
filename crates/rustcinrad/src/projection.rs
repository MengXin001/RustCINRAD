use crate::RadarError::Invalid;
use crate::{GeoPoint, GridSpec, Result};
use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use std::sync::LazyLock;

static WGS84: LazyLock<Geodesic> = LazyLock::new(Geodesic::wgs84);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeamModel {
    factor: f64,
}

impl Default for BeamModel {
    fn default() -> Self {
        Self { factor: 4.0 / 3.0 }
    }
}

impl BeamModel {
    pub fn new(factor: f64) -> Result<Self> {
        if !factor.is_finite() || !(0.1..=10.0).contains(&factor) {
            return Err(Invalid("beam factor must be finite and between 0.1 and 10".into()));
        }
        Ok(Self { factor })
    }

    pub fn factor(self) -> f64 {
        self.factor
    }

    pub fn locate(self, slant_m: f64, elevation_deg: f64, altitude_m: f64) -> Result<(f64, f64)> {
        if !slant_m.is_finite()
            || slant_m < 0.0
            || !elevation_deg.is_finite()
            || !(0.0..90.0).contains(&elevation_deg)
            || !altitude_m.is_finite()
        {
            return Err(Invalid("invalid beam geometry".into()));
        }
        let r = 6_371_008.8 * self.factor;
        let e = elevation_deg.to_radians();
        let radius = (r + slant_m * e.sin()).hypot(slant_m * e.cos());
        let h = radius - r + altitude_m;
        let ground = r * (slant_m * e.cos()).atan2(r + slant_m * e.sin());
        if !ground.is_finite() || !h.is_finite() {
            return Err(Invalid("beam geometry overflow".into()));
        }
        Ok((ground, h))
    }

    pub(crate) fn slant(self, ground_m: f64, elevation_deg: f64) -> Option<f64> {
        let r = 6_371_008.8 * self.factor;
        let theta = ground_m / r;
        let denominator = (elevation_deg.to_radians() + theta).cos();
        (denominator > 0.0).then(|| r * theta.sin() / denominator)
    }
}

pub fn distance_bearing(from: GeoPoint, to: GeoPoint) -> (f64, f64) {
    let (d, a, _, _): (f64, f64, f64, f64) =
        WGS84.inverse(from.latitude(), from.longitude(), to.latitude(), to.longitude());
    (d, a.rem_euclid(360.0))
}

pub fn destination(from: GeoPoint, bearing_deg: f64, distance_m: f64) -> Result<GeoPoint> {
    if !bearing_deg.is_finite() || !distance_m.is_finite() || distance_m < 0.0 {
        return Err(Invalid("invalid geodesic distance/bearing".into()));
    }
    let (lat, lon): (f64, f64) = WGS84.direct(from.latitude(), from.longitude(), bearing_deg, distance_m);
    GeoPoint::new(lon, lat)
}

pub fn auto_grid(site: GeoPoint, range_m: f64, resolution_m: f64) -> Result<GridSpec> {
    if !range_m.is_finite()
        || range_m <= 0.0
        || range_m > 10_000_000.0
        || !resolution_m.is_finite()
        || resolution_m <= 0.0
        || resolution_m > 10_000_000.0
        || site.latitude().abs() > 85.0
    {
        return Err(Invalid("invalid auto-grid local range/resolution or polar station; supply explicit grid".into()));
    }
    let lon = site.longitude();
    let mut west = lon;
    let mut east = lon;
    let mut north = site.latitude();
    let mut south = north;
    for bearing in (0..360).step_by(5) {
        let p = destination(site, bearing as f64, range_m)?;
        let unwrapped = lon + (p.longitude() - lon + 180.0).rem_euclid(360.0) - 180.0;
        west = west.min(unwrapped);
        east = east.max(unwrapped);
        north = north.max(p.latitude());
        south = south.min(p.latitude());
    }
    let dx = (destination(site, 90.0, resolution_m)?.longitude() - lon + 180.0).rem_euclid(360.0) - 180.0;
    let dy = destination(site, 0.0, resolution_m)?.latitude() - site.latitude();
    let width = axis_cells(east - west, dx)?;
    let height = axis_cells(north - south, dy)?;
    GridSpec::new((west + 180.0).rem_euclid(360.0) - 180.0, north, dx, dy, width, height)
}

pub fn resample_grid(spec: GridSpec, resolution_m: f64) -> Result<GridSpec> {
    let center = GeoPoint::new(
        spec.west() + spec.longitude_step() * (spec.width() - 1) as f64 / 2.0,
        spec.north() - spec.latitude_step() * (spec.height() - 1) as f64 / 2.0,
    )?;
    let local = auto_grid(center, resolution_m, resolution_m)?;
    let width = axis_cells(spec.longitude_step() * (spec.width() - 1) as f64, local.longitude_step())?;
    let height = axis_cells(spec.latitude_step() * (spec.height() - 1) as f64, local.latitude_step())?;
    GridSpec::new(spec.west(), spec.north(), local.longitude_step(), local.latitude_step(), width, height)
}

fn axis_cells(span: f64, step: f64) -> Result<usize> {
    let intervals = (span / step).ceil();
    if !span.is_finite()
        || span < 0.0
        || !step.is_finite()
        || step <= 0.0
        || !intervals.is_finite()
        || !(0.0..16_000_000.0).contains(&intervals)
    {
        return Err(Invalid("geographic grid spacing is unrepresentable or exceeds the cell budget".into()));
    }
    Ok(intervals as usize + 1)
}

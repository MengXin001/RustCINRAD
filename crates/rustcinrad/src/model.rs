use crate::RadarError::Invalid;
use ndarray::Array2;
use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error)]
pub enum RadarError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("invalid radar data at byte {offset}: {message}")]
    Decode { offset: usize, message: String },

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("invalid parameter: {0}")]
    Invalid(String),

    #[error("scan {sweep} does not contain {field:?}")]
    MissingField { sweep: usize, field: FieldId },

    #[error("operation cancelled")]
    Cancelled,

    #[error("output error: {0}")]
    Output(String),
}

pub type Result<T> = std::result::Result<T, RadarError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldId {
    TotalReflectivity,
    Reflectivity,
    Velocity,
    SpectrumWidth,
    DifferentialReflectivity,
    CorrelationCoefficient,
    DifferentialPhase,
    SpecificDifferentialPhase,
    HydrometeorClass,
    CompositeReflectivity,
    EchoTop,
    VerticallyIntegratedLiquid,
    VilDensity,
    Cappi,
    RainRate,
    Rainfall,
    EastwardWind,
    NorthwardWind,
    UpwardWind,
    Other(u16),
}

impl FieldId {
    pub fn name(self) -> String {
        match self {
            Self::TotalReflectivity => "TREF",
            Self::Reflectivity => "REF",
            Self::Velocity => "VEL",
            Self::SpectrumWidth => "SW",
            Self::DifferentialReflectivity => "ZDR",
            Self::CorrelationCoefficient => "RHO",
            Self::DifferentialPhase => "PHI",
            Self::SpecificDifferentialPhase => "KDP",
            Self::HydrometeorClass => "HCL",
            Self::CompositeReflectivity => "CR",
            Self::EchoTop => "ET",
            Self::VerticallyIntegratedLiquid => "VIL",
            Self::VilDensity => "VILD",
            Self::Cappi => "CAPPI",
            Self::RainRate => "RR",
            Self::Rainfall => "RAIN",
            Self::EastwardWind => "U",
            Self::NorthwardWind => "V",
            Self::UpwardWind => "W",
            Self::Other(n) => return format!("MOMENT_{n}"),
        }
        .into()
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Reflectivity | Self::TotalReflectivity | Self::CompositeReflectivity | Self::Cappi => "dBZ",
            Self::Velocity | Self::SpectrumWidth | Self::EastwardWind | Self::NorthwardWind | Self::UpwardWind => "m/s",
            Self::DifferentialReflectivity => "dB",
            Self::DifferentialPhase => "degree",
            Self::SpecificDifferentialPhase => "degree/km",
            Self::EchoTop => "m",
            Self::RainRate => "mm/h",
            Self::Rainfall => "mm",
            Self::VerticallyIntegratedLiquid => "kg/m2",
            Self::VilDensity => "g/m3",
            _ => "1",
        }
    }

    pub fn is_categorical(self) -> bool {
        self == Self::HydrometeorClass
    }

    pub(crate) fn from_code(n: u16) -> Self {
        match n {
            1 => Self::TotalReflectivity,
            2 => Self::Reflectivity,
            3 => Self::Velocity,
            4 => Self::SpectrumWidth,
            7 => Self::DifferentialReflectivity,
            9 => Self::CorrelationCoefficient,
            10 => Self::DifferentialPhase,
            11 => Self::SpecificDifferentialPhase,
            14 => Self::HydrometeorClass,
            _ => Self::Other(n),
        }
    }
}

impl std::str::FromStr for FieldId {
    type Err = RadarError;
    fn from_str(s: &str) -> Result<Self> {
        for f in [
            Self::TotalReflectivity,
            Self::Reflectivity,
            Self::Velocity,
            Self::SpectrumWidth,
            Self::DifferentialReflectivity,
            Self::CorrelationCoefficient,
            Self::DifferentialPhase,
            Self::SpecificDifferentialPhase,
            Self::HydrometeorClass,
            Self::CompositeReflectivity,
            Self::EchoTop,
            Self::VerticallyIntegratedLiquid,
            Self::VilDensity,
            Self::Cappi,
            Self::RainRate,
            Self::Rainfall,
            Self::EastwardWind,
            Self::NorthwardWind,
            Self::UpwardWind,
        ] {
            if s.eq_ignore_ascii_case(&f.name()) {
                return Ok(f);
            }
        }
        Err(Invalid(format!("unknown field {s}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellState {
    Valid,
    Missing,
    RangeFolded,

    #[default]
    OutsideCoverage,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    longitude: f64,
    latitude: f64,
}

impl GeoPoint {
    pub fn new(longitude: f64, latitude: f64) -> Result<Self> {
        if !longitude.is_finite() || !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
            return Err(Invalid("non-finite coordinate or latitude outside [-90,90]".into()));
        }
        Ok(Self { longitude: (longitude + 180.0).rem_euclid(360.0) - 180.0, latitude })
    }

    pub fn longitude(self) -> f64 {
        self.longitude
    }

    pub fn latitude(self) -> f64 {
        self.latitude
    }
}

#[derive(Debug, Clone)]
pub struct RadarSite {
    pub code: String,
    pub name: String,
    pub position: GeoPoint,
    pub altitude_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarFormat {
    Sab,
    Cab,
    Special,
    Cc,
    Cd,
    Cc2,
    LegacyPhasedArray,
    Swan,
    Moc,
    StandardPup,
    Fmt,
    PhasedArray,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub field: FieldId,
    pub range_resolution_m: f64,
    pub maximum_range_m: f64,
}

#[derive(Debug, Clone)]
pub struct SweepInfo {
    pub index: usize,
    pub elevation_deg: f64,
    pub ray_count: usize,
    pub fields: Vec<FieldInfo>,
    pub nyquist_velocity_mps: Option<f64>,
    pub time_unix_ms: i64,
    pub complete: bool,
}

#[derive(Debug, Clone)]
pub struct DatasetSummary {
    pub format: RadarFormat,
    pub site: RadarSite,
    pub task: String,
    pub sweeps: Vec<SweepInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpec {
    west: f64,
    north: f64,
    dx: f64,
    dy: f64,
    width: usize,
    height: usize,
}

impl GridSpec {
    pub fn new(west: f64, north: f64, dx: f64, dy: f64, width: usize, height: usize) -> Result<Self> {
        let east = west + dx * width.saturating_sub(1) as f64;
        let south = north - dy * height.saturating_sub(1) as f64;
        if ![west, north, dx, dy, east, south].iter().all(|v| v.is_finite())
            || dx <= 0.0
            || dy <= 0.0
            || width == 0
            || height == 0
            || width.checked_mul(height).is_none_or(|n| n > 16_000_000)
            || south < -90.0
            || north > 90.0
            || east - west >= 360.0
            || !(-180.0..=180.0).contains(&west)
        {
            return Err(Invalid("invalid/oversized geographic grid".into()));
        }
        Ok(Self { west, north, dx, dy, width, height })
    }

    pub fn epsg(self) -> u16 {
        4326
    }

    pub fn west(self) -> f64 {
        self.west
    }

    pub fn north(self) -> f64 {
        self.north
    }

    pub fn longitude_step(self) -> f64 {
        self.dx
    }

    pub fn latitude_step(self) -> f64 {
        self.dy
    }

    pub fn width(self) -> usize {
        self.width
    }

    pub fn height(self) -> usize {
        self.height
    }

    pub fn point(self, row: usize, col: usize) -> Option<GeoPoint> {
        if row >= self.height || col >= self.width {
            return None;
        }
        GeoPoint::new(self.west + self.dx * col as f64, self.north - self.dy * row as f64).ok()
    }

    pub fn pixel(self, point: GeoPoint) -> Option<(usize, usize)> {
        let centre = self.west + self.dx * (self.width - 1) as f64 / 2.0;
        let lon = centre + (point.longitude() - centre + 180.0).rem_euclid(360.0) - 180.0;
        let c = ((lon - self.west) / self.dx).round();
        let r = ((self.north - point.latitude()) / self.dy).round();
        if c < 0.0 || r < 0.0 || c >= self.width as f64 || r >= self.height as f64 {
            None
        } else {
            Some((r as usize, c as usize))
        }
    }
}

#[derive(Debug)]
pub struct GeoGrid {
    spec: GridSpec,
    field: FieldId,
    values: Array2<f32>,
    states: Array2<CellState>,
    heights: Array2<f32>,
    time_unix_ms: i64,
}

impl GeoGrid {
    pub fn new(
        spec: GridSpec,
        field: FieldId,
        values: Array2<f32>,
        states: Array2<CellState>,
        heights: Array2<f32>,
        time_unix_ms: i64,
    ) -> Result<Self> {
        let shape = (spec.height(), spec.width());
        if values.dim() != shape
            || states.dim() != shape
            || heights.dim() != shape
            || values
                .iter()
                .zip(states.iter())
                .zip(heights.iter())
                .any(|((&v, &s), &h)| s == CellState::Valid && (!v.is_finite() || !h.is_finite()))
        {
            return Err(Invalid("grid shapes or valid cell values/heights are invalid".into()));
        }
        Ok(Self { spec, field, values, states, heights, time_unix_ms })
    }

    pub fn spec(&self) -> GridSpec {
        self.spec
    }

    pub fn field(&self) -> FieldId {
        self.field
    }

    pub fn values(&self) -> &Array2<f32> {
        &self.values
    }

    pub fn states(&self) -> &Array2<CellState> {
        &self.states
    }

    pub fn heights_m(&self) -> &Array2<f32> {
        &self.heights
    }

    pub fn time_unix_ms(&self) -> i64 {
        self.time_unix_ms
    }

    pub fn byte_size(&self) -> usize {
        self.values.len() * (8 + std::mem::size_of::<CellState>())
    }

    pub fn sample(&self, point: GeoPoint) -> Option<(f32, CellState, f32)> {
        self.spec.pixel(point).map(|i| (self.values[i], self.states[i], self.heights[i]))
    }
}

pub fn available_fields(sweeps: &[SweepInfo]) -> BTreeSet<FieldId> {
    sweeps.iter().flat_map(|s| s.fields.iter().map(|f| f.field)).collect()
}

impl std::str::FromStr for RadarFormat {
    type Err = RadarError;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "SA" | "SB" | "SAB" => Ok(Self::Sab),
            "CA" | "CB" | "CAB" => Ok(Self::Cab),
            "SPECIAL" => Ok(Self::Special),
            "CC" => Ok(Self::Cc),
            "SC" | "CD" => Ok(Self::Cd),
            "CC2" => Ok(Self::Cc2),
            "PA" => Ok(Self::LegacyPhasedArray),
            "FMT" => Ok(Self::Fmt),
            "STANDARD-PUP" | "ROSE" => Ok(Self::StandardPup),
            "SWAN" => Ok(Self::Swan),
            "MOC" => Ok(Self::Moc),
            _ => Err(Invalid(format!("unknown radar format {s}"))),
        }
    }
}

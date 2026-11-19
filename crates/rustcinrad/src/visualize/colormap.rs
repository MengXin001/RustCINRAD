use crate::RadarError::Invalid;
use crate::*;

mod reference {
    include!(concat!(env!("OUT_DIR"), "/colormaps.rs"));
}

#[derive(Debug, Clone)]
struct Lookup {
    colors: &'static [[u8; 3]],
    min: f64,
    max: f64,
    under: [u8; 3],
    over: [u8; 3],
}

#[derive(Debug, Clone)]
pub struct Palette {
    pub(super) stops: Vec<(f32, [u8; 3])>,
    lut: Option<Lookup>,
}

impl Palette {
    pub fn new(stops: Vec<(f32, [u8; 3])>) -> Result<Self> {
        if stops.len() < 2 || stops.iter().any(|s| !s.0.is_finite()) || stops.windows(2).any(|s| s[1].0 <= s[0].0) {
            return Err(Invalid("palette needs increasing finite stops".into()));
        }
        Ok(Self { stops, lut: None })
    }

    fn reference(min: f64, max: f64, colors: &'static [[u8; 3]], under: [u8; 3], over: [u8; 3]) -> Self {
        Self {
            stops: vec![(min as f32, colors[0]), (max as f32, colors[colors.len() - 1])],
            lut: Some(Lookup { colors, min, max, under, over }),
        }
    }

    pub fn reflectivity() -> Self {
        Self::for_field(FieldId::Reflectivity)
    }

    pub fn velocity() -> Self {
        Self::for_field(FieldId::Velocity)
    }

    pub fn for_field(field: FieldId) -> Self {
        use reference::*;
        let (min, max, colors, under, over) = match field {
            FieldId::Reflectivity | FieldId::TotalReflectivity | FieldId::CompositeReflectivity | FieldId::Cappi => {
                (0.0, 75.0, REF, REF_UNDER, REF_OVER)
            }
            FieldId::Velocity | FieldId::EastwardWind | FieldId::NorthwardWind | FieldId::UpwardWind => {
                (-35.0, 28.0, VEL, VEL_UNDER, VEL_OVER)
            }
            FieldId::SpectrumWidth => (0.0, 13.0, SW, SW_UNDER, SW_OVER),
            FieldId::EchoTop => (0.0, 21000.0, ET, ET_UNDER, ET_OVER),
            FieldId::VerticallyIntegratedLiquid => (0.0, 75.0, VIL, VIL_UNDER, VIL_OVER),
            FieldId::VilDensity => (0.1, 6.0, VIL, VIL_UNDER, VIL_OVER),
            FieldId::DifferentialReflectivity => (-4.0, 5.0, ZDR, ZDR_UNDER, ZDR_OVER),
            FieldId::SpecificDifferentialPhase => (-0.8, 21.0, KDP, KDP_UNDER, KDP_OVER),
            FieldId::DifferentialPhase => (260.0, 360.0, KDP, KDP_UNDER, KDP_OVER),
            FieldId::CorrelationCoefficient => (0.0, 0.99, CC, CC_UNDER, CC_OVER),
            FieldId::HydrometeorClass => (0.0, 10.0, HCL, HCL_UNDER, HCL_OVER),
            _ => (0.0, 204.0, OHP, OHP_UNDER, OHP_OVER),
        };
        Self::reference(min, max, colors, under, over)
    }

    pub fn computed_hydrometeors() -> Self {
        const TAB10: &[[u8; 3]] = &[
            [31, 119, 180],
            [255, 127, 14],
            [44, 160, 44],
            [214, 39, 40],
            [148, 103, 189],
            [140, 86, 75],
            [227, 119, 194],
            [127, 127, 127],
            [188, 189, 34],
            [23, 190, 207],
        ];
        Self::reference(1.0, 11.0, TAB10, TAB10[0], TAB10[9])
    }

    pub(super) fn legend(&self, field: FieldId) -> Option<(&'static [[u8; 3]], &'static [&'static str])> {
        use reference::*;
        self.lut.as_ref()?;
        Some(match field {
            FieldId::Velocity => (
                VEL_REVERSE,
                &["RF", "", "27", "20", "15", "10", "5", "1", "0", "-1", "-5", "-10", "-15", "-20", "-27", "-35"],
            ),
            FieldId::SpectrumWidth => {
                (SW_REVERSE, &["RF", "", "13", "12", "11", "10", "9", "8", "7", "6", "5", "4", "3", "2", "1", "0"])
            }
            FieldId::EchoTop => {
                (ET_CBAR, &["", "21", "20", "18", "17", "15", "14", "12", "11", "9", "8", "6", "5", "3", "2", "0"])
            }
            FieldId::VerticallyIntegratedLiquid => (
                VIL_CBAR,
                &["", "70", "65", "60", "55", "50", "45", "40", "35", "30", "25", "20", "15", "10", "5", "0"],
            ),
            FieldId::VilDensity => (
                VIL_CBAR,
                &["", "6", "5", "4", "3.5", "3", "2.5", "2.1", "1.8", "1.5", "1.2", "0.9", "0.7", "0.5", "0.3", "0.1"],
            ),
            FieldId::DifferentialReflectivity => (
                ZDR_CBAR,
                &["", "5", "4", "3.5", "3", "2.5", "2", "1.5", "1", "0.8", "0.5", "0.2", "0", "-1", "-2", "-3", "-4"],
            ),
            FieldId::SpecificDifferentialPhase => (
                KDP_CBAR,
                &[
                    "", "20", "7", "3.1", "2.4", "1.7", "1.1", "0.75", "0.5", "0.33", "0.22", "0.15", "0.1", "-0.1",
                    "-0.2", "-0.4", "-0.8",
                ],
            ),
            FieldId::DifferentialPhase => (
                KDP_CBAR,
                &[
                    "360", "353.75", "347.5", "341.25", "335", "328.75", "322.5", "316.25", "310", "303.75", "297.5",
                    "291.25", "285", "278.75", "272.5", "266.25", "260",
                ],
            ),
            FieldId::CorrelationCoefficient => (
                CC_CBAR,
                &[
                    "", "0.99", "0.98", "0.97", "0.96", "0.95", "0.94", "0.92", "0.9", "0.85", "0.8", "0.7", "0.6",
                    "0.5", "0.3", "0.1", "0",
                ],
            ),
            FieldId::HydrometeorClass if self.stops[0].0 == 1.0 => (
                self.lut.as_ref()?.colors,
                &[
                    "Drizzle",
                    "Rain",
                    "Ice Crystals",
                    "Dry Snow",
                    "Wet Snow",
                    "Vertical Ice",
                    "Low-Dens Graupel",
                    "High-Dens Graupel",
                    "Hail",
                    "Big Drops",
                    "",
                ],
            ),
            FieldId::HydrometeorClass => (
                HCL_CBAR,
                &[
                    "Rain",
                    "Heavy Rain",
                    "Hail",
                    "Big Drops",
                    "Clear-Air Echo",
                    "Ground Clutter",
                    "Dry snow",
                    "Wet snow",
                    "Ice Crystals",
                    "Graupel",
                    "Unknown",
                    "",
                ],
            ),
            FieldId::Rainfall => (
                OHP_CBAR,
                &[
                    "", "203.2", "152.4", "101.6", "76.2", "63.5", "50.8", "44.45", "38.1", "31.75", "25.4", "19.05",
                    "12.7", "6.35", "2.54", "0",
                ],
            ),
            _ => return None,
        })
    }

    pub fn color(&self, value: f32, categorical: bool) -> [u8; 3] {
        if let Some(lut) = &self.lut {
            let value = f64::from(value);
            if value < lut.min {
                return lut.under;
            }
            if value > lut.max {
                return lut.over;
            }
            let i = (((value - lut.min) / (lut.max - lut.min)) * lut.colors.len() as f64) as usize;
            return lut.colors[i.min(lut.colors.len() - 1)];
        }
        let pos = self.stops.partition_point(|s| s.0 <= value);
        if pos == 0 {
            return self.stops[0].1;
        }
        if pos == self.stops.len() || categorical {
            return self.stops[pos - 1].1;
        }
        let (v0, c0) = self.stops[pos - 1];
        let (v1, c1) = self.stops[pos];
        let t = (value - v0) / (v1 - v0);
        std::array::from_fn(|i| (c0[i] as f32 + (c1[i] as f32 - c0[i] as f32) * t).round() as u8)
    }
}

pub(super) fn product_name(field: FieldId) -> &'static str {
    match field {
        FieldId::Reflectivity => "Base Reflectivity",
        FieldId::TotalReflectivity => "Total Reflectivity",
        FieldId::Velocity => "Base Velocity",
        FieldId::SpectrumWidth => "Spectrum Width",
        FieldId::CompositeReflectivity => "Composite Ref.",
        FieldId::Cappi => "Constant Altitude Ref.",
        FieldId::RainRate => "Rain Rate",
        FieldId::Rainfall => "Accumulated Precip.",
        FieldId::EchoTop => "Echo Tops",
        FieldId::VerticallyIntegratedLiquid => "V Integrated Liquid",
        FieldId::VilDensity => "VIL Density",
        FieldId::DifferentialReflectivity => "Differential Ref.",
        FieldId::DifferentialPhase => "Differential Phase",
        FieldId::CorrelationCoefficient => "Correlation Coe.",
        FieldId::SpecificDifferentialPhase => "Spec. Diff. Phase",
        FieldId::HydrometeorClass => "Hydrometeor Class",
        _ => "Radar Product",
    }
}

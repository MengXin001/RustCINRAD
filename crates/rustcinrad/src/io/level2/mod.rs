mod cc;
mod cc2;
mod cd;
mod phased_array;
mod sab;
pub(super) mod standard;

use super::*;

pub(super) fn decode(bytes: &[u8], hint: &str, options: &ReadOptions) -> Result<RadarDataset> {
    let format = options
        .format
        .or_else(|| {
            if bytes.get(116..125) == Some(b"CINRAD/CC") {
                Some(RadarFormat::Cc)
            } else if matches!(bytes.get(100..109), Some(b"CINRAD/SC" | b"CINRAD/CD")) {
                Some(RadarFormat::Cd)
            } else if bytes.get(112..122).is_some_and(|b| b.starts_with(b"CINRAD/CC2")) {
                Some(RadarFormat::Cc2)
            } else {
                hint.split(['_', '-', '.']).find_map(|s| s.parse::<RadarFormat>().ok())
            }
        })
        .or_else(|| filename_code(hint).and_then(station::format));
    match format {
        Some(RadarFormat::Cc) => cc::decode(bytes, options),
        Some(RadarFormat::Cd) => cd::decode(bytes, options),
        Some(RadarFormat::Cc2) => cc2::decode(bytes, options),
        Some(RadarFormat::LegacyPhasedArray) => phased_array::decode(bytes, hint, options),
        _ => sab::decode(bytes, hint, options),
    }
}

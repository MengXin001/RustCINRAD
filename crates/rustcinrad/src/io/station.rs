use super::*;
use crate::RadarError::Invalid;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};
use tracing::info;

#[derive(Deserialize)]
struct Entry(String, f64, f64, String, f64);
static TABLE: LazyLock<std::result::Result<BTreeMap<String, Entry>, serde_json::Error>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../data/radar_station.json")));

pub(super) fn resolve(
    code: String,
    name: String,
    coordinates: Option<(f64, f64)>,
    altitude_m: Option<f64>,
    options: &ReadOptions,
) -> Result<RadarSite> {
    let table = TABLE.as_ref().map_err(|e| Invalid(format!("station database: {e}")))?;
    let fallback = table.get(&code);
    let native = coordinates
        .filter(|&(lon, lat)| (lon != 0.0 || lat != 0.0) && lon.abs() <= 180.0 && lat.abs() <= 90.0)
        .and_then(|(lon, lat)| GeoPoint::new(lon, lat).ok());
    let position = native.or_else(|| fallback.and_then(|e| GeoPoint::new(e.1, e.2).ok()));
    let name = if name.trim().is_empty() || name == "None" {
        fallback.map_or_else(|| code.clone(), |e| e.0.clone())
    } else {
        name
    };
    let position = options
        .site
        .as_ref()
        .map(|s| s.position)
        .or(position)
        .ok_or_else(|| Invalid(format!("no station coordinates in base data or JSON for {code}")))?;
    let altitude_m = options
        .site
        .as_ref()
        .map(|s| s.altitude_m)
        .or(altitude_m.filter(|h| h.is_finite() && (native.is_some() || *h != 0.0)))
        .or_else(|| fallback.map(|e| e.4))
        .ok_or_else(|| Invalid(format!("no station altitude in base data or JSON for {code}")))?;
    if native.is_none() && options.site.is_none() {
        info!(station = %code, source = "station JSON", "using fallback station coordinates");
    }
    let mut site = RadarSite { code, name, position, altitude_m };
    if let Some(user) = &options.site {
        if !user.code.is_empty() {
            site.code.clone_from(&user.code);
        }
        if !user.name.is_empty() {
            site.name.clone_from(&user.name);
        }
    }
    Ok(site)
}

pub(super) fn format(code: &str) -> Option<RadarFormat> {
    TABLE.as_ref().ok()?.get(code)?.3.parse().ok()
}

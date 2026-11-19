use crate::RadarError::{Decode, Invalid};

pub(crate) mod base;
mod level2;
mod level3;
mod station;

use crate::*;
use std::{fs::File, io::Read, path::Path, time::Instant};
use tracing::{debug_span, info};

#[derive(Debug, Clone)]
pub struct ReadOptions {
    pub site: Option<RadarSite>,
    pub format: Option<RadarFormat>,
    pub max_bytes: usize,
    pub max_decoded_cells: usize,
    pub max_rays: usize,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            site: None,
            format: None,
            max_bytes: 512 * 1024 * 1024,
            max_decoded_cells: 32_000_000,
            max_rays: 200_000,
        }
    }
}

pub fn read_path(path: impl AsRef<Path>, options: &ReadOptions) -> Result<RadarDataset> {
    let path = path.as_ref();
    let _span = debug_span!("read_radar", path = %path.display()).entered();
    let started = Instant::now();
    let file = File::open(path)?;
    let raw = bounded(file, options.max_bytes)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut dataset = read_bytes(&raw, name, options)?;
    dataset.source = Some(path.to_path_buf());
    info!(path = %path.display(), format = ?dataset.summary.format, sweeps = dataset.sweeps.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0, "io");
    Ok(dataset)
}

fn bounded(reader: impl Read, max: usize) -> Result<Vec<u8>> {
    let limit = max.checked_add(1).ok_or_else(|| Invalid("byte limit overflow".into()))?;
    let mut bytes = Vec::new();
    reader.take(limit as u64).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(Invalid("input exceeds byte limit".into()));
    }
    Ok(bytes)
}

pub fn read_bytes(bytes: &[u8], filename_hint: &str, options: &ReadOptions) -> Result<RadarDataset> {
    let _span = debug_span!("decode_radar", input_bytes = bytes.len(), format = ?options.format).entered();
    if bytes.len() > options.max_bytes {
        return Err(Invalid("input exceeds byte limit".into()));
    }
    let unpacked;
    let data = if bytes.starts_with(&[0x1f, 0x8b]) {
        unpacked = bounded(flate2::read::MultiGzDecoder::new(bytes), options.max_bytes)?;
        &unpacked[..]
    } else if bytes.starts_with(b"BZh") {
        unpacked = bounded(bzip2::read::BzDecoder::new(bytes), options.max_bytes)?;
        &unpacked[..]
    } else {
        bytes
    };
    if data.starts_with(b"RSTM") {
        if i32_at(data, 8)? == 2 {
            level3::standard::decode(data, filename_hint, options)
        } else {
            level2::standard::decode(data, options)
        }
    } else if data.starts_with(b"MOC") {
        level3::moc::decode(data, options)
    } else if options.format == Some(RadarFormat::Swan) || data.starts_with(b"SWAN") {
        level3::swan::decode(data, options)
    } else {
        level2::decode(data, filename_hint, options)
    }
}

fn fail(offset: usize, message: impl Into<String>) -> RadarError {
    Decode { offset, message: message.into() }
}

fn slice(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(offset..offset.checked_add(len).ok_or_else(|| fail(offset, "length overflow"))?)
        .ok_or_else(|| fail(offset, "truncated input"))
}

fn u16_at(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(b, o, 2)?.try_into().map_err(|_| fail(o, "u16"))?))
}

fn i32_at(b: &[u8], o: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(slice(b, o, 4)?.try_into().map_err(|_| fail(o, "i32"))?))
}

fn f32_at(b: &[u8], o: usize) -> Result<f64> {
    Ok(f32::from_le_bytes(slice(b, o, 4)?.try_into().map_err(|_| fail(o, "f32"))?) as f64)
}

fn string(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    encoding_rs::GBK.decode(&b[..end]).0.trim().to_owned()
}

fn scan(elevation: f64, time: i64, nyquist: Option<f64>, angular_width: f64) -> crate::io::base::Sweep {
    crate::io::base::Sweep {
        info: SweepInfo {
            index: 0,
            elevation_deg: elevation,
            ray_count: 0,
            fields: Vec::new(),
            nyquist_velocity_mps: nyquist,
            time_unix_ms: time,
            complete: false,
        },
        rays: Vec::new(),
        angular_width,
    }
}

struct DecodeBudget {
    cells_left: usize,
    rays_left: usize,
}

impl DecodeBudget {
    fn new(options: &ReadOptions) -> Self {
        Self { cells_left: options.max_decoded_cells, rays_left: options.max_rays }
    }

    fn ray(&mut self) -> Result<()> {
        self.rays_left = self.rays_left.checked_sub(1).ok_or_else(|| Invalid("decoded ray limit exceeded".into()))?;
        Ok(())
    }

    fn cells(&mut self, count: usize) -> Result<()> {
        self.cells_left =
            self.cells_left.checked_sub(count).ok_or_else(|| Invalid("decoded cell limit exceeded".into()))?;
        Ok(())
    }
}

fn filename_code(hint: &str) -> Option<&str> {
    hint.split(['_', '-', '.']).find(|s| s.starts_with('Z') && s.len() == 5)
}

fn legacy_site(hint: &str, options: &ReadOptions) -> Result<RadarSite> {
    station::resolve(filename_code(hint).unwrap_or("UNKNOWN").into(), String::new(), None, None, options)
}

fn i16_at(b: &[u8], o: usize) -> Result<i16> {
    Ok(i16::from_le_bytes(slice(b, o, 2)?.try_into().map_err(|_| fail(o, "i16"))?))
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(b, o, 4)?.try_into().map_err(|_| fail(o, "u32"))?))
}

fn local_time(year: i32, parts: &[u8], millis: u32) -> Result<i64> {
    let date = chrono::NaiveDate::from_ymd_opt(year, parts[0] as u32, parts[1] as u32)
        .and_then(|d| d.and_hms_milli_opt(parts[2] as u32, parts[3] as u32, parts[4] as u32, millis))
        .ok_or_else(|| fail(0, "invalid local timestamp"))?;
    Ok(date.and_utc().timestamp_millis() - 8 * 3_600_000)
}

fn header_site(
    bytes: &[u8],
    code: (usize, usize),
    name: (usize, usize),
    coords: (usize, usize, usize),
    scale: f64,
    options: &ReadOptions,
) -> Result<RadarSite> {
    station::resolve(
        string(slice(bytes, code.0, code.1)?),
        string(slice(bytes, name.0, name.1)?),
        Some((i32_at(bytes, coords.0)? as f64 / scale, i32_at(bytes, coords.1)? as f64 / scale)),
        Some(i32_at(bytes, coords.2)? as f64 / 1000.0),
        options,
    )
}

fn calendar_time(parts: [u32; 6], offset_seconds: i32) -> Result<i64> {
    let year = i32::try_from(parts[0]).map_err(|_| fail(0, "year overflow"))?;
    let date = chrono::NaiveDate::from_ymd_opt(year, parts[1], parts[2])
        .and_then(|d| d.and_hms_opt(parts[3], parts[4], parts[5]))
        .ok_or_else(|| fail(0, "invalid calendar timestamp"))?;
    if !(-86_399..=86_399).contains(&offset_seconds) {
        return Err(fail(0, "invalid time-zone offset"));
    }
    Ok(date.and_utc().timestamp_millis() - i64::from(offset_seconds) * 1000)
}

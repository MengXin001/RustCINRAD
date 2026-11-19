use crate::RadarError::Output;
use crate::Result;
use binrw::BinRead;
use std::{io::Cursor, sync::LazyLock};

type Coordinate = [f64; 2];

struct Line {
    bounds: [f64; 4],
    points: Vec<Coordinate>,
}

fn read<T: for<'a> BinRead<Args<'a> = ()>>(cursor: &mut Cursor<&[u8]>) -> Result<T> {
    T::read_le(cursor).map_err(|e| Output(e.to_string()))
}

fn decode(bytes: &[u8]) -> Result<Vec<Line>> {
    if bytes.len() < 100 {
        return Err(Output("truncated basemap SHP header".into()));
    }
    let mut cursor = Cursor::new(bytes);
    let magic = u32::read_be(&mut cursor).map_err(|e| Output(e.to_string()))?;
    cursor.set_position(24);
    let words = u32::read_be(&mut cursor).map_err(|e| Output(e.to_string()))?;
    let version: u32 = read(&mut cursor)?;
    let shape_type: u32 = read(&mut cursor)?;
    if magic != 9994 || u64::from(words) * 2 != bytes.len() as u64 || version != 1000 || !matches!(shape_type, 3 | 5) {
        return Err(Output("invalid basemap SHP header".into()));
    }
    cursor.set_position(100);
    let mut lines = Vec::new();
    while cursor.position() < bytes.len() as u64 {
        let _number = u32::read_be(&mut cursor).map_err(|e| Output(e.to_string()))?;
        let words = u32::read_be(&mut cursor).map_err(|e| Output(e.to_string()))?;
        let start = cursor.position() as usize;
        let end = start.checked_add(words as usize * 2).ok_or_else(|| Output("SHP record length overflow".into()))?;
        let payload = bytes.get(start..end).ok_or_else(|| Output("truncated basemap SHP record".into()))?;
        cursor.set_position(end as u64);
        let mut record = Cursor::new(payload);
        let record_type: u32 = read(&mut record)?;
        if record_type == 0 {
            continue;
        }
        if record_type != shape_type {
            return Err(Output("inconsistent basemap SHP record type".into()));
        }
        let _bounds: [f64; 4] = read(&mut record)?;
        let parts: u32 = read(&mut record)?;
        let count: u32 = read(&mut record)?;
        if parts == 0 || parts > count || 44 + u64::from(parts) * 4 + u64::from(count) * 16 != payload.len() as u64 {
            return Err(Output("invalid basemap SHP part/point count".into()));
        }
        let mut starts = Vec::with_capacity(parts as usize + 1);
        for _ in 0..parts {
            starts.push(read::<u32>(&mut record)? as usize);
        }
        starts.push(count as usize);
        if starts[0] != 0 || starts.windows(2).any(|p| p[1] <= p[0]) {
            return Err(Output("invalid basemap SHP part offsets".into()));
        }
        for offsets in starts.windows(2) {
            let mut points = Vec::with_capacity(offsets[1] - offsets[0]);
            let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for _ in offsets[0]..offsets[1] {
                let point: Coordinate = read(&mut record)?;
                if !(-180.0..=180.0).contains(&point[0]) || !(-90.0..=90.0).contains(&point[1]) {
                    return Err(Output("invalid longitude/latitude basemap SHP coordinate".into()));
                }
                bounds[0] = bounds[0].min(point[0]);
                bounds[1] = bounds[1].min(point[1]);
                bounds[2] = bounds[2].max(point[0]);
                bounds[3] = bounds[3].max(point[1]);
                points.push(point);
            }
            lines.push(Line { bounds, points });
        }
    }
    Ok(lines)
}

static LAYERS: LazyLock<Result<Vec<Vec<Line>>>> = LazyLock::new(|| {
    [
        include_bytes!("../../data/map/County.shp").as_slice(),
        include_bytes!("../../data/map/City.shp").as_slice(),
        include_bytes!("../../data/map/Province.shp").as_slice(),
    ]
    .into_iter()
    .map(decode)
    .collect()
});

pub(super) fn visible(west: f64, east: f64, south: f64, north: f64) -> Result<Vec<(usize, &'static [Coordinate])>> {
    let layers = LAYERS.as_ref().map_err(|e| Output(e.to_string()))?;
    let mut selected = Vec::new();
    for (layer, lines) in layers.iter().enumerate() {
        for line in lines {
            let [left, bottom, right, top] = line.bounds;
            let overlaps = [-360.0, 0.0, 360.0].into_iter().any(|shift| left + shift <= east && right + shift >= west);
            if overlaps && bottom <= north && top >= south {
                selected.push((layer, line.points.as_slice()));
            }
        }
    }
    Ok(selected)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    fn shape(kind: u32, parts: &[&[Coordinate]]) -> Vec<u8> {
        let count = parts.iter().map(|part| part.len()).sum::<usize>();
        let payload_len = 44 + parts.len() * 4 + count * 16;
        let mut bytes = vec![0; 100];
        bytes[..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&((108 + payload_len) as u32 / 2).to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&(payload_len as u32 / 2).to_be_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&(parts.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(count as u32).to_le_bytes());
        let mut start = 0_u32;
        for part in parts {
            bytes.extend_from_slice(&start.to_le_bytes());
            start += part.len() as u32;
        }
        for point in parts.iter().flat_map(|part| part.iter()) {
            for coordinate in point {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn multipart_coordinates_order_and_duplicate_vertices_are_unchanged() {
        let outer = [[115.123456789, 35.0], [117.0, 35.0], [117.0, 35.0], [116.0, 37.0], [115.123456789, 35.0]];
        let inner = [[116.0, 35.5], [116.0, 36.0], [116.5, 35.5], [116.0, 35.5]];
        let lines = decode(&shape(5, &[&outer, &inner])).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].points, outer);
        assert_eq!(lines[1].points, inner);
    }

    #[test]
    fn open_polyline_is_not_closed_or_reordered() {
        let first = [[115.123456789, 35.0], [117.0, 35.0], [117.0, 35.0], [116.0, 37.0]];
        let second = [[116.0, 35.5], [116.0, 36.0], [116.5, 35.5]];
        let lines = decode(&shape(3, &[&first, &second])).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].points, first);
        assert_eq!(lines[1].points, second);
    }

    #[test]
    fn original_shp_parts_and_all_vertices_are_retained() {
        let layers = LAYERS.as_ref().unwrap();
        for (lines, (parts, points)) in layers.iter().zip([(8743, 932621), (4309, 833946), (1569, 344311)]) {
            assert_eq!(lines.len(), parts);
            assert_eq!(lines.iter().map(|line| line.points.len()).sum::<usize>(), points);
        }
        assert!(visible(115.0, 125.0, 35.0, 43.0).unwrap().len() > 100);
        assert!(visible(-100.0, -90.0, 20.0, 30.0).unwrap().is_empty());
        assert!(!visible(475.0, 485.0, 35.0, 43.0).unwrap().is_empty());
    }

    #[test]
    fn malformed_shp_headers_records_and_counts_are_rejected() {
        assert!(decode(b"bad map").is_err());
        let bytes = shape(5, &[&[[115.0, 35.0], [117.0, 35.0], [116.0, 37.0], [115.0, 35.0]]]);
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        let mut changed = bytes.clone();
        changed[144..148].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&changed).is_err());
        changed = bytes;
        changed[152..156].copy_from_slice(&1_u32.to_le_bytes());
        assert!(decode(&changed).is_err());
    }
}

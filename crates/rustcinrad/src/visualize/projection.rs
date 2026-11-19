use crate::{projection, *};

#[derive(Debug, Clone, Copy, Default)]
pub enum MapProjection {
    #[default]
    Automatic,
    Geographic,
    AzimuthalEquidistant(GeoPoint),
}

pub(super) struct DisplayPlane {
    spec: GridSpec,
    center: Option<GeoPoint>,
}

impl DisplayPlane {
    pub(super) fn new(spec: GridSpec, mode: MapProjection, site: Option<&RadarSite>) -> Result<Self> {
        let center = match mode {
            MapProjection::Automatic => site.map(|s| s.position),
            MapProjection::Geographic => None,
            MapProjection::AzimuthalEquidistant(p) => Some(p),
        };
        Ok(Self { spec, center })
    }

    pub(super) fn project(&self, p: GeoPoint) -> (f64, f64) {
        if let Some(center) = self.center {
            let (d, a) = projection::distance_bearing(center, p);
            (d * a.to_radians().sin() / 1000.0, d * a.to_radians().cos() / 1000.0)
        } else {
            let mid = self.spec.west() + self.spec.longitude_step() * (self.spec.width() - 1) as f64 / 2.0;
            (mid + (p.longitude() - mid + 180.0).rem_euclid(360.0) - 180.0, p.latitude())
        }
    }

    pub(super) fn unproject(&self, x: f64, y: f64) -> Option<GeoPoint> {
        if let Some(center) = self.center {
            projection::destination(center, x.atan2(y).to_degrees(), x.hypot(y) * 1000.0).ok()
        } else {
            GeoPoint::new(x, y).ok()
        }
    }

    pub(super) fn pixel_row(&self, west: f64, step: f64, y: f64, out: &mut [Option<usize>]) {
        if out.is_empty() {
            return;
        }
        let at = |i: usize| self.unproject(west + (i as f64 + 0.5) * step, y);
        for start in (0..out.len()).step_by(32) {
            let end = (start + 32).min(out.len() - 1);
            self.fill_segment(west, step, y, start, end, at(start), at(end), out);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_segment(
        &self,
        west: f64,
        step: f64,
        y: f64,
        start: usize,
        end: usize,
        left: Option<GeoPoint>,
        right: Option<GeoPoint>,
        out: &mut [Option<usize>],
    ) {
        let exact = |i: usize| self.unproject(west + (i as f64 + 0.5) * step, y);
        if start == end {
            out[start] = left.and_then(|p| self.spec.pixel(p)).map(|(r, c)| r * self.spec.width() + c);
            return;
        }
        if let (Some(a), Some(b)) = (left, right) {
            let dl = (b.longitude() - a.longitude() + 180.0).rem_euclid(360.0) - 180.0;
            let dy = b.latitude() - a.latitude();
            let mid = (start + end) / 2;
            let t = (mid - start) as f64 / (end - start) as f64;
            let control = exact(mid);
            let acceptable = control.is_some_and(|m| {
                ((m.longitude() - a.longitude() - dl * t + 180.0).rem_euclid(360.0) - 180.0).abs()
                    < 0.01 * self.spec.longitude_step()
                    && (m.latitude() - a.latitude() - dy * t).abs() < 0.01 * self.spec.latitude_step()
            });
            if acceptable || end - start <= 1 {
                for (i, cell) in out.iter_mut().enumerate().take(end + 1).skip(start) {
                    let t = (i - start) as f64 / (end - start) as f64;
                    *cell = GeoPoint::new(a.longitude() + dl * t, a.latitude() + dy * t)
                        .ok()
                        .and_then(|p| self.spec.pixel(p))
                        .map(|(r, c)| r * self.spec.width() + c);
                }
                return;
            }
            self.fill_segment(west, step, y, start, mid, left, control, out);
            self.fill_segment(west, step, y, mid + 1, end, exact(mid + 1), right, out);
        } else {
            for (i, cell) in out.iter_mut().enumerate().take(end + 1).skip(start) {
                *cell = exact(i).and_then(|p| self.spec.pixel(p)).map(|(r, c)| r * self.spec.width() + c);
            }
        }
    }

    pub(super) fn bounds(&self) -> Result<(f64, f64, f64, f64)> {
        let west = self.spec.west() - self.spec.longitude_step() / 2.0;
        let east = self.spec.west() + self.spec.longitude_step() * (self.spec.width() as f64 - 0.5);
        let north = (self.spec.north() + self.spec.latitude_step() / 2.0).min(90.0);
        let south = (self.spec.north() - self.spec.latitude_step() * (self.spec.height() as f64 - 0.5)).max(-90.0);
        if self.center.is_none() {
            return Ok((west, east, south, north));
        }
        let mut b = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..=64 {
            let t = i as f64 / 64.0;
            for (lon, lat) in [
                (west + (east - west) * t, north),
                (west + (east - west) * t, south),
                (west, south + (north - south) * t),
                (east, south + (north - south) * t),
            ] {
                let (x, y) = self.project(GeoPoint::new(lon, lat)?);
                b.0 = b.0.min(x);
                b.1 = b.1.max(x);
                b.2 = b.2.min(y);
                b.3 = b.3.max(y);
            }
        }
        Ok(b)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    #[test]
    fn accelerated_display_coordinates_stay_in_the_exact_nearest_cell_neighbourhood() {
        for (lon, lat) in [(121.6, 38.9), (179.6, 42.0), (-45.0, 75.0)] {
            let center = GeoPoint::new(lon, lat).unwrap();
            let spec = projection::auto_grid(center, 350000.0, 1000.0).unwrap();
            let plane = DisplayPlane::new(spec, MapProjection::AzimuthalEquidistant(center), None).unwrap();
            let (west, east, south, north) = plane.bounds().unwrap();
            let width = 1024;
            let step = (east - west) / width as f64;
            let mut mismatch = 0;
            let mut total = 0;
            for row in 0..80 {
                let y = north - (north - south) * (row as f64 + 0.5) / 80.0;
                let mut approximate = vec![None; width];
                plane.pixel_row(west, step, y, &mut approximate);
                for (col, approx) in approximate.iter().enumerate() {
                    let exact = plane
                        .unproject(west + (col as f64 + 0.5) * step, y)
                        .and_then(|p| spec.pixel(p))
                        .map(|(r, c)| r * spec.width() + c);
                    total += 1;
                    if *approx != exact {
                        mismatch += 1;
                        if let (Some(a), Some(b)) = (*approx, exact) {
                            assert!((a / spec.width()).abs_diff(b / spec.width()) <= 1);
                            assert!((a % spec.width()).abs_diff(b % spec.width()) <= 1);
                        }
                    }
                }
            }
            assert!((mismatch as f64) / (total as f64) < 0.02, "{lon},{lat}: {mismatch}/{total}");
        }
    }
}

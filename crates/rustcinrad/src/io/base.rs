use crate::RadarError::{Cancelled, Invalid, MissingField};
use crate::{
    projection::{self, BeamModel},
    *,
};
use ndarray::{Array2, Axis, Slice, Zip};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug)]
pub(crate) struct Moment {
    pub spacing_m: f64,
    pub first_m: f64,
    pub values: Vec<f32>,
    pub states: Vec<CellState>,
}

#[derive(Debug)]
pub(crate) struct Ray {
    pub azimuth: f64,
    pub elevation: f64,
    pub moments: BTreeMap<FieldId, Moment>,
}

#[derive(Debug)]
pub(crate) struct Sweep {
    pub info: SweepInfo,
    pub rays: Vec<Ray>,
    pub angular_width: f64,
}

#[derive(Debug)]
pub struct RadarDataset {
    pub(crate) summary: DatasetSummary,
    pub(crate) sweeps: Vec<Sweep>,
    pub(crate) source: Option<PathBuf>,
    pub(crate) rasters: Vec<GeoGrid>,
}

impl RadarDataset {
    pub fn summary(&self) -> &DatasetSummary {
        &self.summary
    }

    pub fn source(&self) -> Option<&std::path::Path> {
        self.source.as_deref()
    }

    pub fn grid_spec(&self, sweep: usize, field: FieldId, beam: BeamModel) -> Result<GridSpec> {
        if let Some(grid) = self.rasters.get(sweep) {
            if grid.field() != field {
                return Err(MissingField { sweep, field });
            }
            return Ok(grid.spec());
        }
        let scan = self.sweeps.get(sweep).ok_or(MissingField { sweep, field })?;
        let f = scan.info.fields.iter().find(|f| f.field == field).ok_or(MissingField { sweep, field })?;
        let (ground, _) = beam.locate(f.maximum_range_m, scan.info.elevation_deg, self.summary.site.altitude_m)?;
        projection::auto_grid(self.summary.site.position, ground.max(f.range_resolution_m), f.range_resolution_m)
    }

    pub fn grid(&self, sweep: usize, field: FieldId, spec: GridSpec, beam: BeamModel) -> Result<GeoGrid> {
        self.grid_with_progress(sweep, field, spec, beam, |_| true)
    }

    pub fn grid_with_progress(
        &self,
        sweep: usize,
        field: FieldId,
        spec: GridSpec,
        beam: BeamModel,
        mut progress: impl FnMut(f32) -> bool,
    ) -> Result<GeoGrid> {
        let scan = self.sweeps.get(sweep).ok_or(MissingField { sweep, field })?;
        if !scan.info.fields.iter().any(|f| f.field == field) {
            return Err(MissingField { sweep, field });
        }
        let shape = (spec.height(), spec.width());
        let mut values = Array2::zeros(shape);
        let mut states = Array2::from_elem(shape, CellState::OutsideCoverage);
        let mut heights = Array2::zeros(shape);
        if !progress(0.0) {
            return Err(Cancelled);
        }
        for start in (0..spec.height()).step_by(16) {
            let end = (start + 16).min(spec.height());
            Zip::indexed(values.slice_axis_mut(Axis(0), Slice::from(start..end)))
                .and(states.slice_axis_mut(Axis(0), Slice::from(start..end)))
                .and(heights.slice_axis_mut(Axis(0), Slice::from(start..end)))
                .par_for_each(|(row, col), v, state, height| {
                    if let Some(p) = spec.point(start + row, col)
                        && let Some((value, quality, h)) = self.observe(scan, field, p, beam)
                    {
                        *v = value;
                        *state = quality;
                        *height = h;
                    }
                });
            if !progress(end as f32 / spec.height() as f32) {
                return Err(Cancelled);
            }
        }
        GeoGrid::new(spec, field, values, states, heights, scan.info.time_unix_ms)
    }

    pub fn sample(
        &self,
        sweep: usize,
        field: FieldId,
        point: GeoPoint,
        beam: BeamModel,
    ) -> Result<Option<(f32, CellState, f32)>> {
        let scan = self.sweeps.get(sweep).ok_or(MissingField { sweep, field })?;
        if !scan.info.fields.iter().any(|f| f.field == field) {
            return Err(MissingField { sweep, field });
        }
        Ok(self.observe(scan, field, point, beam))
    }

    pub(crate) fn observe(
        &self,
        scan: &Sweep,
        field: FieldId,
        point: GeoPoint,
        beam: BeamModel,
    ) -> Option<(f32, CellState, f32)> {
        if let Some(grid) = self.rasters.get(scan.info.index) {
            if grid.field() != field {
                return None;
            }
            let pixel = grid.spec().pixel(point)?;
            return Some((grid.values()[pixel], grid.states()[pixel], grid.heights_m()[pixel]));
        }
        let (distance, bearing) = projection::distance_bearing(self.summary.site.position, point);
        self.observe_ground(scan, field, distance, bearing, beam)
    }

    pub(crate) fn observe_ground(
        &self,
        scan: &Sweep,
        field: FieldId,
        distance: f64,
        bearing: f64,
        beam: BeamModel,
    ) -> Option<(f32, CellState, f32)> {
        let rays = &scan.rays;
        let pos = rays.partition_point(|r| r.azimuth < bearing);
        let left = if pos == 0 { rays.len().checked_sub(1)? } else { pos - 1 };
        let right = pos % rays.len();
        let delta = |a: f64| (a - bearing + 180.0).rem_euclid(360.0) - 180.0;
        let ray =
            if delta(rays[left].azimuth).abs() < delta(rays[right].azimuth).abs() { &rays[left] } else { &rays[right] };
        if distance > 0.01 && delta(ray.azimuth).abs() > scan.angular_width * 0.525 {
            return None;
        }
        let moment = ray.moments.get(&field)?;
        let slant = beam.slant(distance, ray.elevation)?;
        let index = ((slant - moment.first_m) / moment.spacing_m).round();
        if index < 0.0 || index >= moment.values.len() as f64 {
            return None;
        }
        let index = index as usize;
        let (_, height) = beam
            .locate(moment.first_m + index as f64 * moment.spacing_m, ray.elevation, self.summary.site.altitude_m)
            .ok()?;
        Some((moment.values[index], moment.states[index], height as f32))
    }

    pub(crate) fn from_rasters(
        format: RadarFormat,
        site: RadarSite,
        task: String,
        rasters: Vec<GeoGrid>,
    ) -> Result<Self> {
        if rasters.is_empty() {
            return Err(Invalid("empty geographic dataset".into()));
        }
        let mut sweeps = Vec::with_capacity(rasters.len());
        for (index, grid) in rasters.iter().enumerate() {
            let spec = grid.spec();
            let origin = spec.point(0, 0).ok_or_else(|| Invalid("invalid raster extent".into()))?;
            let east = projection::destination(origin, 90.0, 1.0)?;
            let resolution = if spec.width() > 1 {
                projection::distance_bearing(origin, spec.point(0, 1).ok_or_else(|| Invalid("invalid raster".into()))?)
                    .0
            } else {
                projection::distance_bearing(origin, east).0
            };
            let info = SweepInfo {
                index,
                elevation_deg: 0.0,
                ray_count: 0,
                fields: vec![FieldInfo { field: grid.field(), range_resolution_m: resolution, maximum_range_m: 0.0 }],
                nyquist_velocity_mps: None,
                time_unix_ms: grid.time_unix_ms(),
                complete: true,
            };
            sweeps.push(Sweep { info, rays: Vec::new(), angular_width: 0.0 });
        }
        let summary = DatasetSummary { format, site, task, sweeps: sweeps.iter().map(|s| s.info.clone()).collect() };
        Ok(Self { summary, sweeps, source: None, rasters })
    }

    pub(crate) fn finish(format: RadarFormat, site: RadarSite, task: String, mut sweeps: Vec<Sweep>) -> Result<Self> {
        if !site.altitude_m.is_finite() || sweeps.is_empty() {
            return Err(Invalid("empty volume or invalid station altitude".into()));
        }
        for (index, s) in sweeps.iter_mut().enumerate() {
            s.info.index = index;
            s.rays.sort_by(|a, b| a.azimuth.total_cmp(&b.azimuth));
            let mut gaps: Vec<_> =
                s.rays.windows(2).map(|r| r[1].azimuth - r[0].azimuth).filter(|d| *d > 0.01 && *d <= 10.0).collect();
            gaps.sort_by(f64::total_cmp);
            if s.angular_width <= 0.0 {
                s.angular_width = gaps.get(gaps.len() / 2).copied().unwrap_or(1.0);
            }
            let mut fields: BTreeMap<FieldId, (f64, f64)> = BTreeMap::new();
            for ray in &s.rays {
                if !ray.azimuth.is_finite() || !ray.elevation.is_finite() || !(0.0..90.0).contains(&ray.elevation) {
                    return Err(Invalid("invalid ray angle (RHI not yet supported by grid reader)".into()));
                }
                for (&field, m) in &ray.moments {
                    let range = m.first_m + m.values.len().saturating_sub(1) as f64 * m.spacing_m;
                    fields
                        .entry(field)
                        .and_modify(|(res, r)| {
                            *res = res.min(m.spacing_m);
                            *r = r.max(range);
                        })
                        .or_insert((m.spacing_m, range));
                }
            }
            s.info.ray_count = s.rays.len();
            s.info.fields = fields
                .into_iter()
                .map(|(field, (range_resolution_m, maximum_range_m))| FieldInfo {
                    field,
                    range_resolution_m,
                    maximum_range_m,
                })
                .collect();
        }
        let summary = DatasetSummary { format, site, task, sweeps: sweeps.iter().map(|s| s.info.clone()).collect() };
        Ok(Self { summary, sweeps, source: None, rasters: Vec::new() })
    }
}

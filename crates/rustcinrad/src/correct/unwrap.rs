use super::*;
use crate::RadarError::{Cancelled, Invalid, MissingField, Unsupported};
use crate::io::base::{Moment, Ray, Sweep};
use std::collections::BTreeMap;

struct Groups {
    parent: Vec<usize>,
    size: Vec<usize>,
    offset: Vec<i64>,
}

impl Groups {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect(), size: vec![1; n], offset: vec![0; n] }
    }

    fn find(&mut self, i: usize) -> (usize, i64) {
        if self.parent[i] == i {
            return (i, 0);
        }
        let (root, offset) = self.find(self.parent[i]);
        self.offset[i] += offset;
        self.parent[i] = root;
        (root, self.offset[i])
    }

    fn connect(&mut self, a: usize, b: usize, delta: i64) {
        let (ra, wa) = self.find(a);
        let (rb, wb) = self.find(b);
        if ra == rb {
            return;
        }
        let shift = delta + wa - wb;
        if self.size[ra] >= self.size[rb] {
            self.parent[rb] = ra;
            self.offset[rb] = shift;
            self.size[ra] += self.size[rb];
        } else {
            self.parent[ra] = rb;
            self.offset[ra] = -shift;
            self.size[rb] += self.size[ra];
        }
    }
}

fn wrapped(v: f64, period: f64) -> f64 {
    (v + period / 2.0).rem_euclid(period) - period / 2.0
}

fn solve(
    values: &[f32],
    valid: &[bool],
    rows: usize,
    cols: usize,
    links: &[bool],
    period: f64,
    progress: &mut impl FnMut(f32) -> bool,
) -> Result<Vec<f32>> {
    let n = values.len();
    let mut reliability = vec![1e12; n];
    let above = |r: usize| if r == 0 { rows - 1 } else { r - 1 };
    let below = |r: usize| (r + 1) % rows;
    for r in 0..rows {
        if !progress(0.2 * r as f32 / rows as f32) {
            return Err(Cancelled);
        }
        for c in 1..cols.saturating_sub(1) {
            let i = r * cols + c;
            if !links[above(r)] || !links[r] || !valid[i] {
                continue;
            }
            let neighbours = [
                (r * cols + c - 1, r * cols + c + 1),
                (above(r) * cols + c, below(r) * cols + c),
                (above(r) * cols + c - 1, below(r) * cols + c + 1),
                (above(r) * cols + c + 1, below(r) * cols + c - 1),
            ];
            if neighbours.iter().all(|&(a, b)| valid[a] && valid[b]) {
                reliability[i] = neighbours
                    .iter()
                    .map(|&(a, b)| {
                        let second = wrapped(values[a] as f64 - values[i] as f64, period)
                            - wrapped(values[i] as f64 - values[b] as f64, period);
                        second * second
                    })
                    .sum();
            }
        }
    }
    let mut edges = Vec::with_capacity(n * 2);
    for (r, &linked) in links.iter().enumerate() {
        if !progress(0.2 + 0.2 * r as f32 / rows as f32) {
            return Err(Cancelled);
        }
        for c in 0..cols {
            let a = r * cols + c;
            if !valid[a] {
                continue;
            }
            if c + 1 < cols {
                let b = a + 1;
                if valid[b] {
                    edges.push((reliability[a] + reliability[b], a, b));
                }
            }
            if linked {
                let b = below(r) * cols + c;
                if valid[b] {
                    edges.push((reliability[a] + reliability[b], a, b));
                }
            }
        }
    }
    if !progress(0.4) {
        return Err(Cancelled);
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut groups = Groups::new(n);
    let edge_count = edges.len().max(1);
    for (i, (_, a, b)) in edges.into_iter().enumerate() {
        if i % 4096 == 0 && !progress(0.45 + 0.4 * i as f32 / edge_count as f32) {
            return Err(Cancelled);
        }
        let delta = -((values[b] as f64 - values[a] as f64) / period).round() as i64;
        groups.connect(a, b, delta);
    }
    let mut anchors = BTreeMap::new();
    let mut result = values.to_vec();
    for i in 0..n {
        if i % 4096 == 0 && !progress(0.85 + 0.15 * i as f32 / n.max(1) as f32) {
            return Err(Cancelled);
        }
        if valid[i] {
            let (root, offset) = groups.find(i);
            let anchor = *anchors.entry(root).or_insert(offset);
            result[i] = (values[i] as f64 + (offset - anchor) as f64 * period) as f32;
        }
    }
    if !progress(1.0) {
        return Err(Cancelled);
    }
    Ok(result)
}

pub(super) fn grid(
    dataset: &RadarDataset,
    index: usize,
    spec: GridSpec,
    beam: BeamModel,
    options: DealiasOptions,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<GeoGrid> {
    if !progress(0.0) {
        return Err(Cancelled);
    }
    let field = FieldId::Velocity;
    let scan = dataset.sweeps.get(index).ok_or(MissingField { sweep: index, field })?;
    let nyquist = options
        .nyquist_velocity_mps
        .or(scan.info.nyquist_velocity_mps)
        .ok_or_else(|| Invalid("missing Nyquist velocity".into()))?;
    if !nyquist.is_finite() || !(0.01..=1000.0).contains(&nyquist) {
        return Err(Invalid("Nyquist velocity must be finite and between 0.01 and 1000 m/s".into()));
    }
    let moments: Vec<_> = scan.rays.iter().filter_map(|r| r.moments.get(&field)).collect();
    let first = moments.first().ok_or(MissingField { sweep: index, field })?;
    if moments.iter().any(|m| m.spacing_m != first.spacing_m || m.first_m != first.first_m) {
        return Err(Unsupported("dealiasing rays with different native gate positions".into()));
    }
    let rows = scan.rays.len();
    let cols = moments.iter().map(|m| m.values.len()).max().unwrap_or(0);
    if rows.checked_mul(cols).is_none_or(|n| n > 4_000_000) {
        return Err(Invalid("dealias working-set limit exceeded".into()));
    }
    let mut values = vec![0.0; rows * cols];
    let mut valid = vec![false; rows * cols];
    let mut links = vec![false; rows];
    for (r, ray) in scan.rays.iter().enumerate() {
        if let Some(m) = ray.moments.get(&field) {
            for (c, (&v, &s)) in m.values.iter().zip(&m.states).enumerate() {
                values[r * cols + c] = v;
                valid[r * cols + c] = s == CellState::Valid;
            }
        }
        if r + 1 < rows {
            links[r] = scan.rays[r + 1].azimuth - ray.azimuth <= scan.angular_width * 1.5;
        }
    }
    if options.wrap_azimuth && rows > 2 {
        links[rows - 1] = 360.0 + scan.rays[0].azimuth - scan.rays[rows - 1].azimuth <= scan.angular_width * 1.5;
    }
    let corrected = solve(&values, &valid, rows, cols, &links, 2.0 * nyquist, &mut |p| progress(0.05 + 0.7 * p))?;
    let mut rays = Vec::with_capacity(rows);
    for (r, ray) in scan.rays.iter().enumerate() {
        if r % 16 == 0 && !progress(0.75 + 0.05 * r as f32 / rows as f32) {
            return Err(Cancelled);
        }
        let mut moments = BTreeMap::new();
        if let Some(m) = ray.moments.get(&field) {
            moments.insert(
                field,
                Moment {
                    spacing_m: m.spacing_m,
                    first_m: m.first_m,
                    values: corrected[r * cols..r * cols + m.values.len()].to_vec(),
                    states: m.states.clone(),
                },
            );
        }
        rays.push(Ray { azimuth: ray.azimuth, elevation: ray.elevation, moments });
    }
    let sweep = Sweep { info: scan.info.clone(), rays, angular_width: scan.angular_width };
    let d = RadarDataset::finish(
        dataset.summary.format,
        dataset.summary.site.clone(),
        dataset.summary.task.clone(),
        vec![sweep],
    )?;
    d.grid_with_progress(0, field, spec, beam, |p| progress(0.8 + 0.2 * p))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {

    use super::*;

    #[test]
    fn two_dimensional_ramp_recovers_folds() {
        let rows = 8;
        let cols = 32;
        let mut truth = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                truth.push(c as f32 * 2.0 + r as f32 * 0.2);
            }
        }
        let wrapped: Vec<_> = truth.iter().map(|&v| super::unwrap::wrapped(v as f64, 20.0) as f32).collect();
        let mut links = vec![true; rows];
        links[rows - 1] = false;
        let result =
            solve(&wrapped, &vec![true; rows * cols], rows, cols, &links, 20.0, &mut |_| true).expect("valid unwrap");
        for (a, b) in result.iter().zip(truth) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn mask_separates_unanchored_components() {
        let v = [0.0, 8.0, 0.0, -8.0, 0.0, 8.0, 0.0, -8.0];
        let mask = [true, true, false, true, true, true, false, true];
        let out = solve(&v, &mask, 2, 4, &[true, false], 20.0, &mut |_| true).expect("valid unwrap");
        assert_eq!(out[3], -8.0);
        assert_eq!(out[2], 0.0);
    }
}

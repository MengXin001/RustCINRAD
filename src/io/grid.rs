#[allow(dead_code)]
pub fn grid_raw(
    //reserve
    data: Vec<Vec<f64>>,
    azimuths: Vec<f64>,
    drange: f64,
    reso: f64,
) -> Option<Vec<Vec<f64>>> {
    let drange = drange as usize;
    let rows = drange + 1;
    let cols = drange + 1;
    let center = (((rows - 1) / 2) as f64, ((cols - 1) / 2) as f64);
    let mut grid = vec![vec![0.0; cols]; rows];
    for (az_idx, &az) in azimuths.iter().enumerate() {
        let theta = az.to_radians();
        for (range_idx, &value) in data[az_idx].iter().enumerate() {
            let r = range_idx as f64 * reso;
            let x = r * theta.sin();
            let y = r * theta.cos();

            let x_grid = (center.0 + x).round() as isize;
            let y_grid = (center.1 - y).round() as isize;

            if x_grid >= 0 && x_grid < cols as isize && y_grid >= 0 && y_grid < rows as isize {
                let xi = x_grid as usize;
                let yi = y_grid as usize;
                grid[yi][xi] = value;
            }
        }
    }

    Some(grid)
}
pub fn grid_interpolated(
    data: Vec<Vec<f64>>,
    azimuths: Vec<f64>,
    drange: f64,
    reso: f64,
) -> Option<Vec<Vec<f64>>> {
    let rows = 3000; //default use
    let cols = rows;
    let center = ((rows - 1) as f64 / 2.0, (cols - 1) as f64 / 2.0);

    let scale = drange / rows as f64 * reso; //todo -> dpi

    let mut grid = vec![vec![f64::NAN; cols]; rows];
    for az_idx in 0..azimuths.len() - 1 {
        let theta1 = azimuths[az_idx].to_radians();
        let theta2 = azimuths[az_idx + 1].to_radians();
        for range_idx in 0..data[az_idx].len() - 1 {
            let r1 = range_idx as f64 * reso;
            let r2 = (range_idx + 1) as f64 * reso;
            let corners = [
                (r1 * theta1.sin(), r1 * theta1.cos()),
                (r2 * theta1.sin(), r2 * theta1.cos()),
                (r2 * theta2.sin(), r2 * theta2.cos()),
                (r1 * theta2.sin(), r1 * theta2.cos()),
            ];
            let x_min = corners
                .iter()
                .map(|(x, _)| x)
                .fold(f64::INFINITY, |a, &b| a.min(b));
            let x_max = corners
                .iter()
                .map(|(x, _)| x)
                .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let y_min = corners
                .iter()
                .map(|(_, y)| y)
                .fold(f64::INFINITY, |a, &b| a.min(b));
            let y_max = corners
                .iter()
                .map(|(_, y)| y)
                .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let x_min_idx = ((x_min / scale) + center.0).floor() as isize;
            let x_max_idx = ((x_max / scale) + center.0).ceil() as isize;
            let y_min_idx = (center.1 - (y_max / scale)).floor() as isize;
            let y_max_idx = (center.1 - (y_min / scale)).ceil() as isize;
            for xi in x_min_idx..=x_max_idx {
                for yi in y_min_idx..=y_max_idx {
                    if xi >= 0 && xi < cols as isize && yi >= 0 && yi < rows as isize {
                        let xi = xi as usize;
                        let yi = yi as usize;
                        let cx = (xi as f64 - center.0) * scale;
                        let cy = (center.1 - yi as f64) * scale;
                        if point_in_polygon((cx, cy), &corners) {
                            let value = (data[az_idx][range_idx]
                                + data[az_idx + 1][range_idx]
                                + data[az_idx][range_idx + 1]
                                + data[az_idx + 1][range_idx + 1])
                                / 4.0;

                            grid[yi][xi] = if grid[yi][xi].is_nan() {
                                value
                            } else {
                                (grid[yi][xi] + value) / 2.0
                            };
                        }
                    }
                }
            }
        }
    }

    Some(grid)
}

fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let (px, py) = point;
    let mut is_inside = false;

    for i in 0..polygon.len() {
        let start = polygon[i];
        let end = polygon[(i + 1) % polygon.len()];
        let is_between_y = (start.1 > py) != (end.1 > py);
        if is_between_y {
            let intersection_x = (end.0 - start.0) * (py - start.1) / (end.1 - start.1) + start.0;
            if px < intersection_x {
                is_inside = !is_inside;
            }
        }
    }

    is_inside
}

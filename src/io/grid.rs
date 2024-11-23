pub fn grid(
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
    for (az_index, &az) in azimuths.iter().enumerate() {
        let theta = az.to_radians();
        for (range_index, &value) in data[az_index].iter().enumerate() {
            let r = range_index as f64 * reso;
            let x = r * theta.cos();
            let y = r * theta.sin();

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
    let rows = ((drange / reso).ceil() as usize) + 1;
    let cols = rows;
    let center = ((rows - 1) as f64 / 2.0, (cols - 1) as f64 / 2.0);
    let mut grid = vec![vec![f64::NAN; cols]; rows];
    let mut count = vec![vec![0; cols]; rows];
    for az_index in 0..azimuths.len() - 1 {
        let theta1 = azimuths[az_index].to_radians();
        let theta2 = azimuths[az_index + 1].to_radians();
        for range_index in 0..data[az_index].len() - 1 {
            let r1 = range_index as f64 * reso;
            let r2 = (range_index + 1) as f64 * reso;
            let corners = [
                (r1 * theta1.cos(), r1 * theta1.sin()),
                (r2 * theta1.cos(), r2 * theta1.sin()),
                (r2 * theta2.cos(), r2 * theta2.sin()),
                (r1 * theta2.cos(), r1 * theta2.sin()),
            ];
            let x_min = corners
                .iter()
                .map(|(x, _)| center.0 + x / reso)
                .fold(f64::INFINITY, f64::min);
            let x_max = corners
                .iter()
                .map(|(x, _)| center.0 + x / reso)
                .fold(f64::NEG_INFINITY, f64::max);
            let y_min = corners
                .iter()
                .map(|(_, y)| center.1 - y / reso)
                .fold(f64::INFINITY, f64::min);
            let y_max = corners
                .iter()
                .map(|(_, y)| center.1 - y / reso)
                .fold(f64::NEG_INFINITY, f64::max);
            for xi in x_min.floor() as isize..=x_max.ceil() as isize {
                for yi in y_min.floor() as isize..=y_max.ceil() as isize {
                    if xi >= 0 && xi < cols as isize && yi >= 0 && yi < rows as isize {
                        let xi = xi as usize;
                        let yi = yi as usize;
                        let value = (data[az_index][range_index]
                            + data[az_index + 1][range_index]
                            + data[az_index][range_index + 1]
                            + data[az_index + 1][range_index + 1])
                            / 4.0;

                        if grid[yi][xi].is_nan() {
                            grid[yi][xi] = value;
                        } else {
                            grid[yi][xi] += value;
                        }
                        count[yi][xi] += 1;
                    }
                }
            }
        }
    }
    for i in 0..rows {
        for j in 0..cols {
            if count[i][j] > 0 {
                grid[i][j] /= count[i][j] as f64;
            }
        }
    }

    Some(grid)
}

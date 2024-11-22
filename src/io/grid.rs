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
// todo interp
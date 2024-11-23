use ndarray::Array2;
use std::f64::consts::PI;
#[allow(dead_code)]
pub fn get_range(drange: f64, reso: f64) -> Vec<f64> {
    let mut rng = Vec::new();
    let valid_entry = (drange / reso) as usize;

    for i in 0..valid_entry {
        rng.push(reso * (i as f64 + 1.0));
    }

    rng
}
#[allow(dead_code)]
pub fn get_coordinate(
    distance: Vec<f64>,
    azimuth: Vec<f64>,
    elevation: f64,
    centerlon: f64,
    centerlat: f64,
    h_offset: bool,
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let deg2rad = PI / 180.0;
    let azimuth: Vec<f64> = azimuth.iter().map(|&x| (x * deg2rad)).collect();
    let elev = if h_offset { elevation } else { 0.0 };
    let mut deltav = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    let mut deltah = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    let mut deltalat = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    let mut lat = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    let mut deltalon = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    let mut lon = Array2::<f64>::zeros((azimuth.len(), distance.len()));
    for (i, &azimuth_val) in azimuth.iter().enumerate() {
        let azimuth_rad = azimuth_val;
        let cos_azimuth = azimuth_rad.cos();
        let sin_azimuth = azimuth_rad.sin();
        for (j, &distance_val) in distance.iter().enumerate() {
            deltav[[i, j]] = cos_azimuth * distance_val * (elev * deg2rad).cos();
            deltah[[i, j]] = sin_azimuth * distance_val * (elev * deg2rad).cos();
            deltalat[[i, j]] = deltav[[i, j]] / 111.0;

            lat[[i, j]] = deltalat[[i, j]] + centerlat;
            deltalon[[i, j]] = deltah[[i, j]] / (111.0 * (lat[[i, j]] * deg2rad).cos());
            lon[[i, j]] = deltalon[[i, j]] + centerlon;
        }
    }
    let lat_ncol = lat.ncols();
    let lon_ncol = lon.ncols();

    let actuallat = lat
        .into_raw_vec()
        .chunks(lat_ncol)
        .map(|chunk| chunk.to_vec())
        .collect();
    let actuallon = lon
        .into_raw_vec()
        .chunks(lon_ncol)
        .map(|chunk| chunk.to_vec())
        .collect();

    (actuallon, actuallat)
}

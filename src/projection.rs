use std::f64::consts::PI;

pub fn get_range(drange: f64, reso: f64) -> Vec<f64> {
    let mut rng = Vec::new();
    let valid_entry = (drange / reso) as usize;

    for i in 0..valid_entry {
        rng.push(reso * (i as f64 + 1.0));
    }

    rng
}

pub fn get_coordinate(
    distance: Vec<f64>,
    azimuth: Vec<f64>,
    elevation: f64,
    centerlon: f64,
    centerlat: f64,
    h_offset: bool,
) -> (Vec<f64>, Vec<f64>) {
    let deg2rad = PI / 180.0;
    let azimuth: Vec<f64> = azimuth.iter().map(|&x| (x * deg2rad)).collect();
    let elev = if h_offset { elevation } else { 0.0 };
    let deltav: Vec<f64> = azimuth
        .iter()
        .map(|&x| (x.cos() * distance.iter().sum::<f64>() * (elev * deg2rad).cos()))
        .collect();
    let deltah: Vec<f64> = azimuth
        .iter()
        .map(|&x| (x.sin() * distance.iter().sum::<f64>() * (elev * deg2rad).cos()))
        .collect();
    let deltalat: Vec<f64> = deltav.iter().map(|&x| x / 111.0).collect();
    let actuallat: Vec<f64> = deltalat.iter().map(|&x| x + centerlat).collect();
    let deltalon: Vec<f64> = deltah
        .iter()
        .zip(&actuallat)
        .map(|(&j, &k)| j / (111.0 * (k * deg2rad).cos()))
        .collect();
    let actuallon: Vec<f64> = deltalon.iter().map(|&x| x + centerlon).collect();
    (actuallon, actuallat)
}

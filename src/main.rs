mod io;
mod projection;
mod visualize;
//use projection::{get_coordinate, get_range};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //println!("输入CINRAD数据路径:");
    let file_path = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    println!("read {}", file_path);
    //stdin().read_line(&mut file_path).expect("Input error");
    let start = Instant::now();
    let f = io::level2::SAB_reader(file_path)?;

    let centerlon = &f.attributes["site_longitude"];
    let centerlat = &f.attributes["site_latitude"];
    let tilt= 0;
    let drange = 460.0;
    let elevation = f.get_tilt(tilt)?;
    let fname = "radar.png";
    let dtype = "REF";
    let reso = f.get_reso(dtype)?;
    println!(
        "\n第{}层仰角{}deg，数据范围{}km，数据分辨率{}km",
        tilt, elevation, drange, reso
    );
    //let r = get_range(drange, reso);
    let azimuth = f.get_azimuth(tilt)?;
    let data =f.get_data(tilt, drange, dtype)?;
    /*let (actuallon, actuallat) =
    get_coordinate(r, azimuth.to_vec(), elevation, centerlon, centerlat, true);*/
    let grid_data: Vec<Vec<f64>> = io::grid::grid_interpolated(data, azimuth, drange, reso).unwrap();
    visualize::ppi::ppi(grid_data, 3000, 3000, fname, dtype);
    println!(
        "站点: {}/{} {}N, {}E/{}m",
        &f.attributes["site_code"], &f.attributes["site_name"], centerlat, centerlon, f.attributes["site_altitude"]
    );
    let duration = start.elapsed();
    println!("运行时间: {:?}", duration);
    Ok(())
}

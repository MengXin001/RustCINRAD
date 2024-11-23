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
    let radardata = io::level2::SAB_reader(file_path).unwrap();

    let radarcode = radardata.site_code;
    let radarname = radardata.site_name;
    let centerlon = radardata.site_longitude;
    let centerlat = radardata.site_latitude;
    let el = 0;
    let drange = 460.0;
    let reso = 1.0;
    let elevation = 0.5;
    let fname = "radar.png";
    println!(
        "\n第{}层仰角{}deg，数据范围{}km，数据分辨率{}km",
        el, elevation, drange, reso
    );
    //let r = get_range(drange, reso);
    let azimuth = &radardata.azimuth[el];
    let data = &radardata.data[0][el];
    /*let (actuallon, actuallat) =
    get_coordinate(r, azimuth.to_vec(), elevation, centerlon, centerlat, true);*/
    let grid_data: Vec<Vec<f64>> = io::grid::grid_interpolated(data.to_vec(), azimuth.to_vec(), drange, reso).unwrap();
    visualize::ppi::ppi(grid_data.to_vec(), 3000, 3000, fname);
    println!(
        "站点: {}/{} {}N, {}E/{}m",
        radarcode, radarname, centerlat, centerlon, radardata.site_altitude
    );
    let duration = start.elapsed();
    println!("运行时间: {:?}", duration);
    Ok(())
}

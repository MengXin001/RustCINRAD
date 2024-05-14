mod io;
mod projection;
use projection:: {get_coordinate, get_range};
use std::io::stdin;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("输入CINRAD数据路径:");
    let mut filePath = String::new();
    stdin().read_line(&mut filePath).expect("Input error");
    let start = Instant::now();

    //let fileName: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    //let mut radardata = io::level2::SAB_reader(&fileName);

    let mut radardata = io::level2::SAB_reader(&filePath.trim());
    let radarcode = io::dtype::infer_type(&filePath).unwrap();
    let el = 0;
    let r = get_range(2.300, 0.1);
    let azimuth = radardata[el]["azimuth"].to_vec();
    let elevation = 0.0;

    // Read from station config data
    let radarinfo = io::base::get_radar_info();
    let station = &radarinfo[radarcode.clone()];
    let radarname = station[0].as_str().unwrap();
    let centerlon = station[1].as_f64().unwrap();
    let centerlat = station[2].as_f64().unwrap();
    let radartype = station[3].as_str().unwrap();

    let (actuallon, actuallat) = get_coordinate(r, azimuth, elevation, centerlon, centerlat, true);
    radardata[el].insert("lon".to_string(), actuallon);
    radardata[el].insert("lat".to_string(), actuallat);

    // println!("{:?}", radardata[el]["REF"]);
    println!("{} {} {}", radarcode, radarname,radartype);
    let duration = start.elapsed();
    println!("程序执行时间: {:?}", duration);
    Ok(())
}
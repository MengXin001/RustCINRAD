mod io;
mod projection;
use io::SAB_reader;
use projection:: {get_coordinate, get_range};
use std::io::stdin;
use std::time::Instant;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Input CINRAD Data path:");
    let mut filePath = String::new();
    stdin().read_line(&mut filePath).expect("Input error");
    let start = Instant::now();
    let fileName: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    //let radardata = SAB_reader(&filePath.trim());
    let mut radardata = SAB_reader(&fileName);
    let el = 0;
    let r = get_range(2.300, 0.1);
    let azimuth = radardata[el]["azimuth"].to_vec();
    let elevation = 0.0;

    // Read from station config data
    let centerlat = 33.43083;
    let centerlon = 120.20083;

    let (actuallon, actuallat) = get_coordinate(r, azimuth, elevation, centerlon, centerlat, true);

    radardata[el].insert("lon".to_string(), actuallon);
    radardata[el].insert("lat".to_string(), actuallat);
    // println!("{:?}", radardata[el]["REF"]);
    let duration = start.elapsed();
    println!("程序执行时间: {:?}", duration);
    Ok(())
}
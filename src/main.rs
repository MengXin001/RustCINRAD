mod io;
mod projection;
use projection::{get_coordinate, get_range};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("输入CINRAD数据路径:");
    let file_path = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    //stdin().read_line(&mut file_path).expect("Input error");
    let start = Instant::now();
    let radardata = io::level2::SAB_reader(file_path).unwrap();
    println!("{:?}", radardata.azimuth);
    let radarcode = radardata.site_code;
    let radarname = radardata.site_name;
    println!("\n站点: {}/{}", radarcode, radarname);
    let duration = start.elapsed();
    println!("运行时间: {:?}", duration);
    Ok(())
}

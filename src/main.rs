mod io;
use io::SAB_reader;
use std::time::Instant;

fn main() -> std::io::Result<()> {
    let start = Instant::now();
    let filePath: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    let fileName: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    let radardata = SAB_reader(filePath);
    println!("{:?}", radardata[1]);
    let duration = start.elapsed();
    println!("程序执行时间: {:?}", duration);
    Ok(())
}
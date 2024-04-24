mod io;
use io::SAB_reader;

fn main() -> Result<(), bincode::Error> {
    let filePath: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    let fileName: &str = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin";
    let mut file = std::fs::read(filePath).expect("file exists");
    let radardata = SAB_reader(&file)?;
    println!(
        "{:?}",
        radardata.data().r() // radardata.info() radardata.header()
    );
    Ok(())
}
mod io;
mod projection;
mod visualize;
//use projection::{get_coordinate, get_range};
use clap::Parser;
use std::time::Instant;
use tracing::info;
use tracing_subscriber;

#[derive(Parser)]
#[command(
    version = env!("CARGO_PKG_VERSION"),
    about = "PyCINRAD with Rust to decode CINRAD data and visualize."
)]
struct Cli {
    #[arg(
        short = 'f',
        long,
        value_name = "FILE",
        default_value = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin"
    )]
    file: String,
    #[arg(short = 'r', long, value_name = "RANGE", default_value = "460")]
    drange: f64,
    #[arg(short = 't', long, value_name = "TILT", default_value = "0")]
    tilt: usize,
    #[arg(short = 'd', long, value_name = "DTYPE", default_value = "REF")]
    dtype: String,
    #[arg(
        short = 'o',
        long,
        value_name = "FILENAME",
        default_value = "Z_RADR_I_Z9515_20160623043100_O_DOR_SA_R.png"
    )]
    output: String,
    #[arg(short = 'p', long, value_name = "PREVIEW", default_value = "true")]
    preview: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    info!(
        "---------RustCINRAD CLI Build{}---------",
        env!("CARGO_PKG_VERSION")
    );
    let (file_path, drange, tilt, dtype, output, preview) = (
        args.file,
        args.drange,
        args.tilt,
        &args.dtype,
        &args.output,
        args.preview,
    );
    if !preview {
        info!("read {}", file_path);
        let start = Instant::now();
        let f = io::level2::SAB_reader(&file_path)?;

        let elevation = f.get_tilt(tilt)?;
        let reso = f.get_reso(dtype)?;
        info!(
            "\n第{}层仰角{}deg，数据范围{}km，数据分辨率{}km",
            tilt, elevation, drange, reso
        );
        //let r = get_range(drange, reso);
        let azimuth = f.get_azimuth(tilt)?;
        let data = f.get_data(tilt, drange, dtype)?;
        /*let (actuallon, actuallat) =
        get_coordinate(r, azimuth.to_vec(), elevation, centerlon, centerlat, true);*/
        let grid_data: Vec<Vec<f64>> =
            io::grid::grid_interpolated(data, azimuth, drange, reso).unwrap();
        visualize::ppi::ppi(grid_data, 3000, 3000, output, dtype);
        info!(
            "站点: {}/{}/{} {}N, {}E/{}m",
            &f.attributes["site_code"],
            &f.attributes["site_name"],
            &f.attributes["site_type"],
            &f.attributes["site_latitude"],
            &f.attributes["site_longitude"],
            f.attributes["site_altitude"]
        );

        let duration = start.elapsed();
        info!("运行时间: {:?}", duration);
    } else {
        info!("RUNNING PREVIEW MODE");
        let fmt_file_path = "Z_RADR_I_Z9375_20241028094043_O_DOR_SA_CAP_FMT.bin";
        info!("read {}", fmt_file_path);
        let start = Instant::now();
        let f = io::level2::FMT_SAB_reader(fmt_file_path)?;
        info!(
            "站点: {}/{}/{} {}N, {}E/{}m",
            &f.attributes["site_code"],
            &f.attributes["site_name"],
            &f.attributes["site_type"],
            &f.attributes["site_latitude"],
            &f.attributes["site_longitude"],
            f.attributes["site_altitude"]
        );
        let elevation = f.get_tilt(tilt)?;
        let reso = f.get_reso(dtype)?;
        info!(
            "\n第{}层仰角{}deg，数据范围{}km，数据分辨率{}km",
            tilt, elevation, drange, reso
        );
        let azimuth = f.get_azimuth(tilt)?;
        let data = f.get_data(tilt, drange, dtype)?;
        let grid_data: Vec<Vec<f64>> =
            io::grid::grid_interpolated(data, azimuth, drange, reso).unwrap();
        visualize::ppi::ppi(grid_data, 3000, 3000, output, dtype);
        let duration = start.elapsed();
        info!("运行时间: {:?}", duration);
    }
    Ok(())
}
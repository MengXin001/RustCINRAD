use anyhow::{Context, Result};
use clap::{Args as ClapArgs, Parser, Subcommand};
use rustcinrad::{
    calc::{Product, ProductRequest},
    projection::BeamModel,
    visualize::{self, PlotOptions},
    *,
};
use rustcinrad_app::{Application, JobEvent, Operation};
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, error, info};

#[derive(Parser)]
#[command(version, about = "PyCINRAD with Rust to decode CINRAD data and visualize.")]
struct Args {
    #[command(subcommand)]
    command: Command,
    #[arg(long, global = true)]
    format: Option<RadarFormat>,
}

#[derive(ClapArgs, Clone)]
struct Processing {
    #[arg(long, default_value = "REF", help = "Radar field or product to plot")]
    dtype: FieldId,
    #[arg(long, help = "Zero-based scan index; use inspect to see elevations and available fields")]
    tilt: Option<usize>,
    #[arg(long, conflicts_with_all = ["dual_pol_qc", "attenuation", "derive_kdp"])]
    dealias: bool,
    #[arg(long, default_value = "S", value_parser = ["S", "C", "X"])]
    band: String,

    #[arg(long, default_value_t = 18.0)]
    threshold: f32,

    #[arg(long, default_value_t = 3000.0)]
    height_m: f64,

    #[arg(long)]
    dual_pol_qc: bool,

    #[arg(long)]
    attenuation: bool,

    #[arg(long)]
    derive_kdp: bool,
}

#[derive(Subcommand)]
enum Command {
    Inspect {
        file: PathBuf,
    },

    /// Plot a radar field or product as PNG, with optional GeoTIFF output.
    Plot {
        #[arg(value_name = "INPUT.bin")]
        file: PathBuf,
        #[command(flatten)]
        processing: Processing,
        #[arg(long, help = "PNG output path (default: beside input, <input stem>_<dtype>.png)")]
        png: Option<PathBuf>,
        #[arg(long, help = "Also export a geographic raster to this GeoTIFF path")]
        geotiff: Option<PathBuf>,
    },

    Batch {
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
        #[command(flatten)]
        processing: Processing,
    },

    Accumulate {
        #[arg(required = true, num_args = 2..)]
        files: Vec<PathBuf>,
        #[arg(long)]
        png: Option<PathBuf>,
        #[arg(long)]
        geotiff: Option<PathBuf>,
    },

    Volume {
        file: PathBuf,
        #[arg(long)]
        geotiff: PathBuf,

        #[arg(long, num_args = 3, default_values = ["1000", "10000", "1000"], allow_hyphen_values = true)]
        heights: Vec<f64>,
    },

    Section {
        file: PathBuf,
        #[arg(long = "dtype", default_value = "REF")]
        field: FieldId,
        #[arg(long)]
        png: PathBuf,

        #[arg(long, num_args = 4.., allow_hyphen_values = true)]
        path: Vec<f64>,

        #[arg(long, num_args = 3, default_values = ["0", "15000", "250"], allow_hyphen_values = true)]
        heights: Vec<f64>,
        #[arg(long, default_value_t = 1000.0)]
        step_m: f64,
        #[arg(long, default_value_t = 1000.0)]
        vertical_tolerance_m: f64,
    },
}

fn plot_png_path(input: &Path, dtype: FieldId, png: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = png {
        return Ok(path);
    }
    let mut filename = input.file_stem().context("input path has no file name")?.to_os_string();
    filename.push("_");
    filename.push(dtype.name());
    filename.push(".png");
    Ok(input.with_file_name(filename))
}

fn height_levels(heights: &[f64]) -> Result<Vec<f64>> {
    anyhow::ensure!(
        heights.len() == 3 && heights.iter().all(|h| h.is_finite()) && heights[1] > heights[0] && heights[2] > 0.0,
        "invalid --heights",
    );
    let count = ((heights[1] - heights[0]) / heights[2]).floor() + 1.0;
    anyhow::ensure!(count.is_finite() && (2.0..=100000.0).contains(&count), "invalid/excessive height count");
    Ok((0..count as usize).map(|i| heights[0] + i as f64 * heights[2]).collect())
}

fn init_logging() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init()
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn main() -> std::process::ExitCode {
    if let Err(error) = init_logging() {
        // No subscriber is available to report initialization failures.
        eprintln!("RustCINRAD: cannot initialize logging: {error:#}");
        return std::process::ExitCode::FAILURE;
    }
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            error!(error = %format!("{error:#}"), "command failed");
            std::process::ExitCode::FAILURE
        }
    }
}

fn band(processing: &Processing) -> rustcinrad::correct::RadarBand {
    match processing.band.as_str() {
        "C" => rustcinrad::correct::RadarBand::C,
        "X" => rustcinrad::correct::RadarBand::X,
        _ => rustcinrad::correct::RadarBand::S,
    }
}

fn operation(dataset: &RadarDataset, processing: &Processing) -> Result<(usize, bool, Operation)> {
    let field = processing.dtype;
    let selection = if field == FieldId::SpecificDifferentialPhase && processing.derive_kdp {
        FieldId::DifferentialPhase
    } else {
        field
    };
    let sweep = processing
        .tilt
        .or_else(|| {
            dataset.summary().sweeps.iter().find(|s| s.fields.iter().any(|f| f.field == selection)).map(|s| s.index)
        })
        .or_else(|| {
            dataset
                .summary()
                .sweeps
                .iter()
                .find(|s| s.fields.iter().any(|f| f.field == FieldId::Reflectivity))
                .map(|s| s.index)
        })
        .context("no usable scan")?;
    let native = dataset.summary().sweeps.get(sweep).is_some_and(|s| s.fields.iter().any(|f| f.field == field));
    let derived = matches!(
        field,
        FieldId::CompositeReflectivity
            | FieldId::EchoTop
            | FieldId::VerticallyIntegratedLiquid
            | FieldId::VilDensity
            | FieldId::HydrometeorClass
            | FieldId::Cappi
            | FieldId::RainRate
    );
    let basefield = if !native && derived {
        FieldId::Reflectivity
    } else if field == FieldId::SpecificDifferentialPhase && processing.derive_kdp {
        FieldId::DifferentialPhase
    } else {
        field
    };
    let beam = BeamModel::default();
    let spec = dataset.grid_spec(sweep, basefield, beam)?;
    anyhow::ensure!(!processing.dealias || field == FieldId::Velocity, "--dealias requires --dtype VEL");
    let op = if processing.dealias {
        Operation::Dealias { sweep, spec, beam, options: Default::default() }
    } else if processing.dual_pol_qc || processing.attenuation || processing.derive_kdp {
        Operation::Polarimetric {
            sweep,
            field,
            spec,
            beam,
            options: rustcinrad::correct::PolarimetricOptions {
                derive_kdp: processing.derive_kdp,
                attenuation_band: processing.attenuation.then(|| band(processing)),
                ..Default::default()
            },
        }
    } else if field == FieldId::HydrometeorClass && !native {
        Operation::Hydrometeors {
            sweep,
            spec,
            beam,
            options: rustcinrad::correct::HcaOptions { band: band(processing), ..Default::default() },
        }
    } else if field == FieldId::RainRate && !native {
        Operation::Qpe { sweep, spec, beam, options: Default::default() }
    } else if !native && derived {
        let product = match field {
            FieldId::CompositeReflectivity => Product::CompositeReflectivity,
            FieldId::EchoTop => Product::EchoTop { threshold_dbz: processing.threshold },
            FieldId::VerticallyIntegratedLiquid => {
                Product::Vil { threshold_dbz: processing.threshold, beam_width_deg: 0.99 }
            }
            FieldId::VilDensity => Product::VilDensity { threshold_dbz: processing.threshold },
            FieldId::Cappi => Product::Cappi { height_m: processing.height_m, max_gap_m: 3000.0 },
            _ => anyhow::bail!("unsupported derived field"),
        };
        Operation::Product(ProductRequest { product, grid: spec, beam })
    } else {
        Operation::Grid { sweep, field, spec, beam }
    };
    Ok((sweep, native, op))
}

fn output(
    grid: &GeoGrid,
    dataset: &RadarDataset,
    sweep: usize,
    native: bool,
    png: Option<&Path>,
    tiff: Option<&Path>,
) -> Result<()> {
    if let Some(path) = tiff {
        rustcinrad::export::write_geotiff(grid, path)?;
    }
    if let Some(path) = png {
        let scan = dataset.summary().sweeps.get(sweep).context("invalid output scan")?;
        visualize::write_png(
            grid,
            path,
            &PlotOptions {
                site: Some(dataset.summary().site.clone()),
                scan: Some(scan.clone()),
                task: dataset.summary().task.clone(),
                projection: if scan.ray_count == 0 {
                    visualize::MapProjection::Geographic
                } else {
                    visualize::MapProjection::Automatic
                },
                palette: if grid.field() == FieldId::HydrometeorClass && !native {
                    visualize::Palette::computed_hydrometeors()
                } else {
                    visualize::Palette::for_field(grid.field())
                },
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

fn run() -> Result<()> {
    let args = Args::parse();
    let options = ReadOptions { format: args.format, ..Default::default() };
    match args.command {
        Command::Inspect { file } => {
            let dataset = read_path(file, &options)?;
            let s = dataset.summary();
            println!(
                "{} {} | {:?} | {:.6}, {:.6} | {:.1} m MSL | {}",
                s.site.code,
                s.site.name,
                s.format,
                s.site.position.longitude(),
                s.site.position.latitude(),
                s.site.altitude_m,
                s.task
            );
            for scan in &s.sweeps {
                println!(
                    "scan {}: {:.3} deg, {} rays, UTC ms {}, Nyquist {:?} m/s, complete {}",
                    scan.index,
                    scan.elevation_deg,
                    scan.ray_count,
                    scan.time_unix_ms,
                    scan.nyquist_velocity_mps,
                    scan.complete
                );
                for field in &scan.fields {
                    println!(
                        "  {} [{}]: {:.1} m gates, {:.1} m range",
                        field.field.name(),
                        field.field.unit(),
                        field.range_resolution_m,
                        field.maximum_range_m
                    );
                }
            }
        }
        Command::Plot { file, processing, png, geotiff } => {
            let png = plot_png_path(&file, processing.dtype, png)?;
            let app = Application::new(256 * 1024 * 1024);
            app.open(file, &options)?;
            let (_, dataset) = app.dataset()?;
            let (sweep, native, op) = operation(&dataset, &processing)?;
            let mut job = app.submit(op)?;
            let raster: Arc<GeoGrid> = loop {
                match job.events.recv().context("radar worker stopped without a result")? {
                    JobEvent::Completed { session, grid, .. } => {
                        anyhow::ensure!(app.is_current(session), "session changed");
                        break grid;
                    }
                    JobEvent::Failed { message, .. } => anyhow::bail!(message),
                    JobEvent::Cancelled(_) => anyhow::bail!("cancelled"),
                    JobEvent::Stale(_) => anyhow::bail!("session replaced"),
                    JobEvent::Progress { job, session, fraction } => {
                        debug!(job = job.0, session = session.0, fraction, "radar job progress");
                    }
                }
            };
            job.join()?;
            output(&raster, &dataset, sweep, native, Some(&png), geotiff.as_deref())?;
        }
        Command::Batch { files, output_dir, processing } => {
            std::fs::create_dir_all(&output_dir)?;

            let mut index = 0;
            let outcomes = rustcinrad_app::batch(
                &files,
                &options,
                |dataset| {
                    index += 1;
                    let (sweep, native, op) = operation(dataset, &processing)?;
                    let raster = op.execute_with_progress(dataset, |_| true)?;
                    let code: String = dataset
                        .summary()
                        .site
                        .code
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect();
                    let name = format!("{index:05}_{}_{}_{}", code, raster.time_unix_ms(), raster.field().name());
                    output(
                        &raster,
                        dataset,
                        sweep,
                        native,
                        Some(&output_dir.join(format!("{name}.png"))),
                        Some(&output_dir.join(format!("{name}.tif"))),
                    )
                },
                |_, _| true,
            )?;
            let mut failures = 0;
            for outcome in outcomes {
                if let Err(error) = outcome.result {
                    failures += 1;
                    error!(path = %outcome.path.display(), error = %format!("{error:#}"), "batch input failed");
                }
            }
            anyhow::ensure!(failures == 0, "{failures} of {} input files failed; other outputs retained", files.len());
        }
        Command::Accumulate { files, png, geotiff } => {
            anyhow::ensure!(png.is_some() || geotiff.is_some(), "provide --png and/or --geotiff");
            let first = read_path(&files[0], &options)?;
            let sweep = first
                .summary()
                .sweeps
                .iter()
                .find(|s| s.fields.iter().any(|f| f.field == FieldId::Reflectivity))
                .context("no REF scan")?
                .index;
            let beam = BeamModel::default();
            let spec = first.grid_spec(sweep, FieldId::Reflectivity, beam)?;
            anyhow::ensure!(
                spec.width()
                    .checked_mul(spec.height())
                    .and_then(|n| n.checked_mul(files.len()))
                    .is_some_and(|n| n <= 16_000_000),
                "rainfall series exceeds 16 million retained cells"
            );
            let mut rates = vec![first.rain_rate_grid(sweep, spec, beam, Default::default())?];
            for path in &files[1..] {
                let dataset = read_path(path, &options).with_context(|| format!("opening {}", path.display()))?;
                anyhow::ensure!(
                    dataset.summary().site.position == first.summary().site.position
                        && dataset.summary().site.code == first.summary().site.code,
                    "rainfall series requires the same radar site"
                );
                let scan = dataset
                    .summary()
                    .sweeps
                    .iter()
                    .find(|s| s.fields.iter().any(|f| f.field == FieldId::Reflectivity))
                    .context("no REF scan")?
                    .index;
                rates.push(dataset.rain_rate_grid(scan, spec, beam, Default::default())?);
            }
            rates.sort_by_key(GeoGrid::time_unix_ms);
            let accumulation = rustcinrad::retrieve::accumulate(&rates.iter().collect::<Vec<_>>(), Default::default())?;
            output(&accumulation.depth, &first, sweep, false, png.as_deref(), geotiff.as_deref())?;
            info!(
                start_unix_ms = accumulation.start_unix_ms,
                end_unix_ms = accumulation.end_unix_ms,
                "rainfall accumulated"
            );
        }
        Command::Volume { file, geotiff, heights } => {
            let levels = height_levels(&heights)?;
            let dataset = read_path(file, &options)?;
            let sweep = dataset
                .summary()
                .sweeps
                .iter()
                .find(|s| s.fields.iter().any(|f| f.field == FieldId::Reflectivity))
                .context("no REF scan")?
                .index;
            let beam = BeamModel::default();
            let spec = dataset.grid_spec(sweep, FieldId::Reflectivity, beam)?;
            let volume = rustcinrad::retrieve::cappi_volume(&dataset, spec, &levels, 3000.0, beam)?;
            let layers: Vec<_> = volume
                .layers
                .iter()
                .zip(&volume.heights_m)
                .map(|(grid, &height)| rustcinrad::export::GeoTiffLayer { grid, height_msl_m: Some(height) })
                .collect();
            rustcinrad::export::write_geotiff_layers(&layers, geotiff)?;
            info!(layers = layers.len(), crs = "EPSG:4326", height_unit = "m MSL", "CAPPI volume ready");
        }
        Command::Section { file, field, png, path, heights, step_m, vertical_tolerance_m } => {
            anyhow::ensure!(path.len() >= 4 && path.len() % 2 == 0, "--path needs longitude/latitude pairs");
            let levels = height_levels(&heights)?;
            let points: rustcinrad::Result<Vec<_>> =
                path.as_chunks::<2>().0.iter().map(|p| GeoPoint::new(p[0], p[1])).collect();
            let dataset = read_path(file, &options)?;
            let section = rustcinrad::calc::section(
                &dataset,
                field,
                &points?,
                step_m,
                &levels,
                vertical_tolerance_m,
                BeamModel::default(),
            )?;
            visualize::write_section_png(
                &section,
                png,
                &visualize::SectionPlotOptions {
                    title: dataset.summary().site.name.clone(),
                    palette: visualize::Palette::for_field(field),
                    ..Default::default()
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn scientific_height_sampling_rejects_overflow_and_invalid_ranges() -> Result<()> {
        assert_eq!(height_levels(&[1000.0, 3000.0, 1000.0])?, [1000.0, 2000.0, 3000.0]);
        for values in [
            vec![],
            vec![0.0, f64::MAX, f64::MIN_POSITIVE],
            vec![-f64::MAX, f64::MAX, 1.0],
            vec![0.0, 1.0, 0.0],
            vec![f64::NAN, 2.0, 1.0],
        ] {
            assert!(height_levels(&values).is_err());
        }
        assert!(Args::try_parse_from(["rustcinrad", "volume", "input.bin", "--geotiff", "volume.tif"]).is_ok());
        Ok(())
    }

    #[test]
    fn plot_defaults_to_ref_and_automatic_png() -> Result<()> {
        let args = Args::try_parse_from(["rustcinrad", "plot", "input.bin"])?;
        let Command::Plot { file, processing, png, geotiff } = args.command else {
            anyhow::bail!("expected plot command");
        };
        assert_eq!(processing.dtype, FieldId::Reflectivity);
        assert_eq!(processing.tilt, None);
        assert!(geotiff.is_none());
        assert_eq!(plot_png_path(&file, processing.dtype, png)?, Path::new("input_REF.png"));
        Ok(())
    }

    #[test]
    fn plot_selects_dtype_and_zero_based_tilt() -> Result<()> {
        for sweep in ["0", "3"] {
            let args = Args::try_parse_from(["rustcinrad", "plot", "input.bin", "--dtype", "vel", "--tilt", sweep])?;
            let Command::Plot { file, processing, png, .. } = args.command else {
                anyhow::bail!("expected plot command");
            };
            assert_eq!(processing.dtype, FieldId::Velocity);
            assert_eq!(processing.tilt, Some(sweep.parse()?));
            assert_eq!(plot_png_path(&file, processing.dtype, png)?, Path::new("input_VEL.png"));
        }
        assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin", "--tilt", "0.5"]).is_err());
        assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin", "--dtype", "unknown"]).is_err());
        Ok(())
    }

    #[test]
    fn plot_keeps_explicit_png_and_optional_geotiff() -> Result<()> {
        for extra in [vec![], vec!["--png", "ppi.png"]] {
            let mut arguments = vec!["rustcinrad", "plot", "input.bin", "--geotiff", "ppi.tif"];
            arguments.extend(extra.iter().copied());
            let args = Args::try_parse_from(arguments)?;
            let Command::Plot { file, processing, png, geotiff } = args.command else {
                anyhow::bail!("expected plot command");
            };
            assert_eq!(geotiff.as_deref(), Some(Path::new("ppi.tif")));
            let expected = if extra.is_empty() { "input_REF.png" } else { "ppi.png" };
            assert_eq!(plot_png_path(&file, processing.dtype, png)?, Path::new(expected));
        }
        Ok(())
    }

    #[test]
    fn automatic_png_name_preserves_input_directory_and_filename() -> Result<()> {
        for (input, field, expected) in [
            ("data/sample.bin", FieldId::Reflectivity, "data/sample_REF.png"),
            ("data/sample.bin", FieldId::CompositeReflectivity, "data/sample_CR.png"),
            ("雷达 数据/大连.20180806.bin", FieldId::Velocity, "雷达 数据/大连.20180806_VEL.png"),
            ("sample", FieldId::EchoTop, "sample_ET.png"),
        ] {
            assert_eq!(plot_png_path(Path::new(input), field, None)?, Path::new(expected));
        }
        assert!(plot_png_path(Path::new(""), FieldId::Reflectivity, None).is_err());
        Ok(())
    }

    #[test]
    fn dtype_is_consistent_for_batch_and_section() -> Result<()> {
        let args = Args::try_parse_from(["rustcinrad", "batch", "a.bin", "b.bin", "--dtype", "ET"])?;
        assert!(matches!(args.command, Command::Batch { processing: Processing { dtype: FieldId::EchoTop, .. }, .. }));
        let args =
            Args::try_parse_from(["rustcinrad", "section", "input.bin", "--dtype", "VEL", "--png", "section.png"])?;
        assert!(matches!(args.command, Command::Section { field: FieldId::Velocity, .. }));
        assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin", "--field", "REF"]).is_err());
        assert!(Args::try_parse_from(["rustcinrad", "render", "input.bin"]).is_err());
        assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin", "--sweep", "0"]).is_err());
        Ok(())
    }

    #[test]
    fn drawing_and_station_parameters_are_not_required_or_exposed() {
        assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin"]).is_ok());
        for flag in ["--resolution-m", "--range-m", "--grid", "--site", "--width", "--height", "--earth-factor"] {
            assert!(Args::try_parse_from(["rustcinrad", "plot", "input.bin", flag, "1"]).is_err());
        }
        assert!(Args::try_parse_from(["rustcinrad", "batch", "a.bin", "b.bin"]).is_ok());
    }
}

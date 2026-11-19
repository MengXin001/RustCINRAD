// cargo run --release -p rustcinrad --example profile -- INPUT OUTPUT_DIR
use rustcinrad::{
    calc::{Product, ProductRequest},
    projection::BeamModel,
    visualize::{Palette, PlotOptions},
    *,
};
use std::{path::PathBuf, time::Instant};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let file = PathBuf::from(args.next().ok_or("missing input path")?);
    let output = PathBuf::from(args.next().ok_or("missing output directory")?);
    std::fs::create_dir_all(&output)?;
    let start = Instant::now();
    let data = read_path(file, &ReadOptions::default())?;
    println!("decode_ms,{:.3}", start.elapsed().as_secs_f64() * 1000.0);
    let beam = BeamModel::default();
    println!("field,compute_ms,png_ms,tiff_ms,width,height");
    for field in [
        FieldId::Reflectivity,
        FieldId::Velocity,
        FieldId::CompositeReflectivity,
        FieldId::EchoTop,
        FieldId::VerticallyIntegratedLiquid,
        FieldId::VilDensity,
        FieldId::Cappi,
        FieldId::RainRate,
    ] {
        let native = matches!(field, FieldId::Reflectivity | FieldId::Velocity);
        let scan = data
            .summary()
            .sweeps
            .iter()
            .find(|s| s.fields.iter().any(|f| f.field == if native { field } else { FieldId::Reflectivity }))
            .ok_or("no source scan")?;
        let spec = data.grid_spec(scan.index, if native { field } else { FieldId::Reflectivity }, beam)?;
        let start = Instant::now();
        let grid = if native {
            data.grid(scan.index, field, spec, beam)?
        } else if field == FieldId::RainRate {
            data.rain_rate_grid(scan.index, spec, beam, Default::default())?
        } else {
            let product = match field {
                FieldId::CompositeReflectivity => Product::CompositeReflectivity,
                FieldId::EchoTop => Product::EchoTop { threshold_dbz: 18.0 },
                FieldId::VerticallyIntegratedLiquid => Product::Vil { threshold_dbz: 18.0, beam_width_deg: 0.99 },
                FieldId::VilDensity => Product::VilDensity { threshold_dbz: 18.0 },
                _ => Product::Cappi { height_m: 3000.0, max_gap_m: 3000.0 },
            };
            data.product(ProductRequest { product, grid: spec, beam })?
        };
        let compute = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        rustcinrad::visualize::write_png(
            &grid,
            output.join(format!("{}.png", field.name())),
            &PlotOptions {
                site: Some(data.summary().site.clone()),
                scan: Some(scan.clone()),
                task: data.summary().task.clone(),
                palette: Palette::for_field(field),
                ..Default::default()
            },
        )?;
        let png = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        rustcinrad::export::write_geotiff(&grid, output.join(format!("{}.tif", field.name())))?;
        println!(
            "{},{compute:.3},{png:.3},{:.3},{},{}",
            field.name(),
            start.elapsed().as_secs_f64() * 1000.0,
            spec.width(),
            spec.height()
        );
    }
    Ok(())
}

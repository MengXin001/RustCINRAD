use crate::RadarError::{Invalid, Output};
use crate::*;
use std::{
    fs::File,
    io::{BufWriter, Seek, Write},
    path::Path,
    time::Instant,
};
use tiff::{
    encoder::{TiffEncoder, colortype},
    tags::Tag,
};
use tracing::{debug_span, info};

fn output(e: impl std::fmt::Display) -> RadarError {
    Output(e.to_string())
}

pub fn write_geotiff(grid: &GeoGrid, path: impl AsRef<Path>) -> Result<()> {
    write_geotiff_layers(&[GeoTiffLayer { grid, height_msl_m: None }], path)
}

#[derive(Debug, Clone, Copy)]
pub struct GeoTiffLayer<'a> {
    pub grid: &'a GeoGrid,
    pub height_msl_m: Option<f64>,
}

pub fn write_geotiff_layers(layers: &[GeoTiffLayer<'_>], path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let _span = debug_span!("export_geotiff", path = %path.display(), layers = layers.len()).entered();
    let started = Instant::now();
    let first = layers.first().ok_or_else(|| Invalid("empty GeoTIFF layers".into()))?;
    let spec = first.grid.spec();
    if spec.width().checked_mul(spec.height()).and_then(|n| n.checked_mul(layers.len())).is_none_or(|n| n > 16_000_000)
        || layers.iter().any(|layer| {
            layer.grid.spec() != spec
                || layer.height_msl_m.is_some_and(|h| {
                    !h.is_finite()
                        || layer.grid.heights_m().iter().zip(layer.grid.states()).any(|(&observed, &state)| {
                            state == CellState::Valid && (f64::from(observed) - h).abs() > 0.01
                        })
                })
        })
    {
        return Err(Invalid("invalid GeoTIFF layers or constant heights".into()));
    }
    let mut writer = BufWriter::new(File::create(path)?);
    {
        let mut encoder = TiffEncoder::new(&mut writer).map_err(output)?;
        for layer in layers {
            write_layer(&mut encoder, *layer)?;
        }
    }
    writer.flush()?;
    info!(path = %path.display(), layers = layers.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0, "visualize(geotiff)");
    Ok(())
}

fn write_layer<W: Write + Seek>(encoder: &mut TiffEncoder<W>, layer: GeoTiffLayer<'_>) -> Result<()> {
    let grid = layer.grid;
    let spec = grid.spec();
    let mut image =
        encoder.new_image::<colortype::Gray32Float>(spec.width() as u32, spec.height() as u32).map_err(output)?;
    let e = image.encoder();
    write_georeferencing(e, spec)?;
    e.write_tag(Tag::Unknown(42113), "nan").map_err(output)?;
    let height_metadata =
        layer.height_msl_m.map_or_else(String::new, |h| format!("<Item name=\"HEIGHT_MSL_M\">{h}</Item>"));
    let metadata = format!(
        r#"<GDALMetadata><Item name="UNITTYPE" sample="0" role="unittype">{}</Item><Item name="DESCRIPTION" sample="0" role="description">{}</Item>{height_metadata}</GDALMetadata>"#,
        grid.field().unit(),
        grid.field().name()
    );
    e.write_tag(Tag::Unknown(42112), metadata.as_str()).map_err(output)?;
    let description = format!(
        "rustcinrad {} [{}]; UTC Unix ms {}; EPSG:4326",
        grid.field().name(),
        grid.field().unit(),
        grid.time_unix_ms()
    );
    e.write_tag(Tag::ImageDescription, description.as_str()).map_err(output)?;
    let data: Vec<_> = grid
        .values()
        .iter()
        .zip(grid.states().iter())
        .map(|(&v, &s)| if s == CellState::Valid { v } else { f32::NAN })
        .collect();
    image.write_data(&data).map_err(output)?;
    let mut mask = encoder.new_image::<colortype::Gray8>(spec.width() as u32, spec.height() as u32).map_err(output)?;
    write_georeferencing(mask.encoder(), spec)?;
    mask.encoder()
        .write_tag(
            Tag::ImageDescription,
            "rustcinrad validity: 0=valid,1=missing,2=range_folded,3=outside_coverage,4=rejected",
        )
        .map_err(output)?;
    let data: Vec<_> = grid
        .states()
        .iter()
        .map(|s| match s {
            CellState::Valid => 0,
            CellState::Missing => 1,
            CellState::RangeFolded => 2,
            CellState::OutsideCoverage => 3,
            CellState::Rejected => 4,
        })
        .collect();
    mask.write_data(&data).map_err(output)?;
    Ok(())
}

fn write_georeferencing<W: Write + Seek, K: tiff::encoder::TiffKind>(
    e: &mut tiff::encoder::DirectoryEncoder<'_, W, K>,
    spec: GridSpec,
) -> Result<()> {
    e.write_tag(Tag::ModelPixelScaleTag, &[spec.longitude_step(), spec.latitude_step(), 0.0][..]).map_err(output)?;
    e.write_tag(
        Tag::ModelTiepointTag,
        &[0.0, 0.0, 0.0, spec.west() - spec.longitude_step() / 2.0, spec.north() + spec.latitude_step() / 2.0, 0.0][..],
    )
    .map_err(output)?;
    e.write_tag(
        Tag::GeoKeyDirectoryTag,
        &[1_u16, 1, 0, 4, 1024, 0, 1, 2, 1025, 0, 1, 1, 2048, 0, 1, 4326, 2054, 0, 1, 9102][..],
    )
    .map_err(output)?;
    Ok(())
}

use super::Palette;
use super::projection::{DisplayPlane, MapProjection};
use crate::RadarError::{Invalid, Output};
use crate::*;
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use image::{Rgba, RgbaImage};
use plotters::prelude::*;
use rayon::prelude::*;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};
use tracing::{debug_span, info};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlotStyle {
    #[default]
    Black,
    White,
    Transparent,
}

#[derive(Debug, Clone)]
pub struct MapAnnotation {
    pub position: GeoPoint,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct GeoPolyline {
    pub points: Vec<GeoPoint>,
    pub color: [u8; 3],
}

#[derive(Debug, Clone)]
pub struct PlotOptions {
    pub style: PlotStyle,
    pub scan: Option<SweepInfo>,
    pub task: String,
    pub width: u32,
    pub height: u32,
    pub palette: Palette,
    pub projection: MapProjection,
    pub site: Option<RadarSite>,
    pub range_rings_m: Vec<f64>,
    pub lines: Vec<GeoPolyline>,
    pub annotations: Vec<MapAnnotation>,
}

impl Default for PlotOptions {
    fn default() -> Self {
        Self {
            width: 3500,
            height: 2800,
            style: PlotStyle::Black,
            scan: None,
            task: "VCP21".into(),
            palette: Palette::reflectivity(),
            projection: MapProjection::Automatic,
            site: None,
            range_rings_m: vec![],
            lines: vec![],
            annotations: vec![],
        }
    }
}

fn output(e: impl std::fmt::Display) -> RadarError {
    Output(e.to_string())
}

pub fn write_png(grid: &GeoGrid, path: impl AsRef<Path>, options: &PlotOptions) -> Result<()> {
    let path = path.as_ref();
    let _span = debug_span!("plot_ppi", path = %path.display(), dtype = %grid.field().name()).entered();
    let started = Instant::now();
    if options.width < 400 || options.height < 300 || options.width as u64 * options.height as u64 > 32_000_000 {
        return Err(Invalid("Image dimensions must be >=400x300 and <=32 million pixels".into()));
    }
    let background = if options.style == PlotStyle::White { WHITE } else { BLACK };
    let foreground = if options.style == PlotStyle::White { BLACK } else { WHITE };
    let missing =
        if options.style == PlotStyle::Transparent { [1, 2, 3] } else { [background.0, background.1, background.2] };
    let mut rgb = vec![0_u8; options.width as usize * options.height as usize * 3];
    let spec = grid.spec();
    let plane = DisplayPlane::new(spec, options.projection, options.site.as_ref())?;
    let (mut west, mut east, mut south, mut north) = plane.bounds()?;
    let plot_bounds;
    let pixel_cells;
    let mut text_labels = Vec::new();
    let mut annotation_labels = Vec::new();
    let font_size = (options.width as f64 * 12.0 / 720.0).max(10.0) as u32;
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (options.width, options.height)).into_drawing_area();
        root.fill(&background).map_err(output)?;
        let (map, _) = root.split_horizontally(options.width * 4 / 5);
        let mut chart = ChartBuilder::on(&map).build_cartesian_2d(west..east, south..north).map_err(output)?;
        {
            let (x, y) = chart.plotting_area().get_pixel_range();
            let aspect = (x.end - x.start) as f64 / (y.end - y.start) as f64;
            let cx = (west + east) / 2.0;
            let cy = (north + south) / 2.0;
            let width = (east - west).max((north - south) * aspect);
            let height = width / aspect;
            west = cx - width / 2.0;
            east = cx + width / 2.0;
            south = cy - height / 2.0;
            north = cy + height / 2.0;
            chart = ChartBuilder::on(&map).build_cartesian_2d(west..east, south..north).map_err(output)?;
        }
        plot_bounds = chart.plotting_area().get_pixel_range();
        let (xr, yr) = &plot_bounds;
        let w = (xr.end - xr.start) as usize;
        let h = (yr.end - yr.start) as usize;

        let colors: Vec<_> = grid
            .values()
            .iter()
            .zip(grid.states())
            .map(|(&v, &state)| {
                if state == CellState::Valid {
                    options.palette.color(v, grid.field().is_categorical())
                } else if state == CellState::RangeFolded {
                    [102, 0, 102]
                } else {
                    missing
                }
            })
            .collect();
        let mut raster = vec![0_u8; w * h * 3];
        let render_row = |row: usize, cells: &mut [Option<usize>], pixels: &mut [u8]| {
            let y = north - (north - south) * (row as f64 + 0.5) / h as f64;
            plane.pixel_row(west, (east - west) / w as f64, y, cells);
            for (pixel, &cell) in pixels.as_chunks_mut::<3>().0.iter_mut().zip(cells.iter()) {
                pixel.copy_from_slice(&cell.map_or(missing, |i| colors[i]));
            }
        };
        if options.style == PlotStyle::Transparent {
            let mut cells = vec![None; w * h];
            cells
                .par_chunks_mut(w)
                .zip(raster.par_chunks_mut(w * 3))
                .enumerate()
                .for_each(|(row, (cells, pixels))| render_row(row, cells, pixels));
            pixel_cells = cells;
        } else {
            raster
                .par_chunks_mut(w * 3)
                .enumerate()
                .for_each_init(|| vec![None; w], |cells, (row, pixels)| render_row(row, cells, pixels));
            pixel_cells = Vec::new();
        }
        {
            let mut canvas = BitMapBackend::with_buffer(&mut raster, (w as u32, h as u32));
            let bounds = (west, east, south, north);
            let geographic_east = spec.west() + spec.longitude_step() * (spec.width() - 1) as f64;
            let geographic_south = spec.north() - spec.latitude_step() * (spec.height() - 1) as f64;
            let colors = if options.style == PlotStyle::White {
                [RGBColor(128, 128, 128), RGBColor(211, 211, 211), BLACK]
            } else {
                [RGBColor(128, 128, 128), RGBColor(211, 211, 211), WHITE]
            };
            for (layer, points) in super::map::visible(spec.west(), geographic_east, geographic_south, spec.north())? {
                let projected: Result<Vec<_>> = points
                    .par_iter()
                    .map(|&[lon, lat]| GeoPoint::new(lon, lat).map(|point| plane.project(point)))
                    .collect();
                super::canvas::path(&mut canvas, bounds, projected?, colors[layer])?;
            }
            for r in &options.range_rings_m {
                let site = options.site.as_ref().ok_or_else(|| Invalid("range rings require a station".into()))?;
                let points: Result<Vec<_>> = (0..=360)
                    .step_by(3)
                    .map(|a| crate::projection::destination(site.position, a as f64, *r).map(|p| plane.project(p)))
                    .collect();
                super::canvas::path(&mut canvas, bounds, points?, RGBColor(90, 90, 90))?;
            }
            for line in &options.lines {
                super::canvas::path(
                    &mut canvas,
                    bounds,
                    line.points.iter().map(|&p| plane.project(p)),
                    RGBColor(line.color[0], line.color[1], line.color[2]),
                )?;
            }
        }
        let bitmap = BitMapElement::with_owned_buffer((0, 0), (w as u32, h as u32), raster)
            .ok_or_else(|| Output("invalid bitmap size".into()))?;
        chart.plotting_area().strip_coord_spec().draw(&bitmap).map_err(output)?;
        for annotation in &options.annotations {
            let position = plane.project(annotation.position);
            if (west..=east).contains(&position.0) && (south..=north).contains(&position.1) {
                let (x, y) = chart.plotting_area().map_coordinate(&position);
                annotation_labels.push((annotation.label.clone(), x, y));
            }
        }
        let timestamp = chrono::DateTime::from_timestamp_millis(grid.time_unix_ms());
        let scale = if grid.field() == FieldId::EchoTop { 0.001 } else { 1.0 };
        let unit = if grid.field() == FieldId::EchoTop { "km" } else { grid.field().unit() };
        let (mut maximum, mut minimum) = (f32::NEG_INFINITY, f32::INFINITY);
        for (&value, &state) in grid.values().iter().zip(grid.states()) {
            if state == CellState::Valid {
                maximum = maximum.max(value);
                minimum = minimum.min(value);
            }
        }
        let scan = options.scan.as_ref();
        let resolution = scan
            .and_then(|s| {
                s.fields
                    .iter()
                    .find(|f| f.field == grid.field())
                    .or_else(|| s.fields.iter().find(|f| f.field == FieldId::Reflectivity))
            })
            .map(|f| f.range_resolution_m);
        let range = scan
            .and_then(|s| {
                s.fields
                    .iter()
                    .find(|f| f.field == grid.field())
                    .or_else(|| s.fields.iter().find(|f| f.field == FieldId::Reflectivity))
            })
            .map(|f| f.maximum_range_m);
        let res_text = resolution.map_or_else(
            || "N/A".into(),
            |m| {
                if m >= 100.0 { format!("{:.2}km", m / 1000.0) } else { format!("{m:.0}m") }
            },
        );
        let mut texts = vec![
            super::colormap::product_name(grid.field()).to_owned(),
            range.map_or_else(|| "Range: native domain".into(), |r| format!("Range: {:.0}km", r / 1000.0)),
            format!("Resolution: {res_text}"),
            format!("Date: {}", timestamp.map_or_else(|| "N/A".into(), |t| t.format("%Y.%m.%d").to_string())),
            format!("Time: {}", timestamp.map_or_else(|| "N/A".into(), |t| t.format("%H:%M").to_string())),
            format!("RDA: {}", options.site.as_ref().map_or("Unknown", |s| s.name.as_str())),
            format!("Task: {}", options.task),
            format!("Elev: {:.2}deg", scan.map_or(0.0, |s| s.elevation_deg)),
            if maximum.is_finite() { format!("Max: {:.1}{unit}", maximum * scale) } else { "Max: N/A".into() },
        ];
        if grid.field() == FieldId::Velocity {
            texts.push(if minimum.is_finite() { format!("Min: {:.1}{unit}", minimum) } else { "Min: N/A".into() });
        }
        let left = (options.width as f64 * 0.83) as i32;
        for (i, text) in texts.into_iter().enumerate() {
            let y = (options.height as f64 * (0.02 + i as f64 * 0.021)) as i32;
            text_labels.push((text, left, y));
        }
        let top = (options.height as f64 * 0.56) as i32;
        let bottom = (options.height as f64 * 0.94) as i32;
        let right = (options.width as f64 * 0.87) as i32;
        let min = options.palette.stops[0].0;
        let max = options.palette.stops[options.palette.stops.len() - 1].0;
        let legend = options.palette.legend(grid.field());
        for y in top..bottom {
            let t = 1.0 - (y - top) as f32 / (bottom - top) as f32;
            let c = if let Some((colors, _)) = legend {
                colors[((t * colors.len() as f32) as usize).min(colors.len() - 1)]
            } else {
                options.palette.color(min + (max - min) * t, grid.field().is_categorical())
            };
            root.draw(&Rectangle::new([(left, y), (right, y + 1)], RGBColor(c[0], c[1], c[2]).filled()))
                .map_err(output)?;
        }
        if let Some((_, labels)) = legend {
            for (k, &text) in labels.iter().enumerate() {
                let y = bottom - (bottom - top) * k as i32 / (labels.len() - 1) as i32;
                text_labels.push((text.to_owned(), right + font_size as i32 / 2, y - font_size as i32 / 2));
            }
        } else {
            for k in 0..=7 {
                let v = min + (max - min) * k as f32 / 7.5;
                let y = bottom - ((bottom - top) as f32 * (v - min) / (max - min)) as i32;
                text_labels.push((format!("{:.0}", v * scale), right + font_size as i32 / 2, y - font_size as i32 / 2));
            }
        }
        root.present().map_err(output)?;
    }
    super::font::labels(
        &mut rgb,
        options.width,
        options.height,
        &text_labels,
        font_size,
        [foreground.0, foreground.1, foreground.2],
    )?;
    super::font::labels(
        &mut rgb,
        options.width,
        options.height,
        &annotation_labels,
        14,
        [foreground.0, foreground.1, foreground.2],
    )?;
    let mut result = RgbaImage::from_fn(options.width, options.height, |x, y| {
        let i = (y as usize * options.width as usize + x as usize) * 3;
        Rgba([rgb[i], rgb[i + 1], rgb[i + 2], 255])
    });
    let (xr, yr) = plot_bounds;
    if options.style == PlotStyle::Transparent {
        for y in yr.clone() {
            for x in xr.clone() {
                let cell =
                    pixel_cells[(y - yr.start) as usize * (xr.end - xr.start) as usize + (x - xr.start) as usize];
                if cell.is_none_or(|i| {
                    !matches!(
                        grid.states()[(i / spec.width(), i % spec.width())],
                        CellState::Valid | CellState::RangeFolded
                    )
                }) && result.get_pixel(x as u32, y as u32).0[..3] == [1, 2, 3]
                {
                    result.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 0]));
                }
            }
        }
    }
    let mut writer = BufWriter::new(File::create(path)?);
    PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::Sub)
        .write_image(result.as_raw(), options.width, options.height, image::ExtendedColorType::Rgba8)
        .map_err(output)?;
    writer.flush()?;
    info!(path = %path.display(), dtype = %grid.field().name(), elapsed_ms = started.elapsed().as_secs_f64() * 1000.0, "visualize(png)");
    Ok(())
}

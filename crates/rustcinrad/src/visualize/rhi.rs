use super::{Palette, PlotStyle};
use crate::RadarError::{Invalid, Output};
use crate::{calc::GeoSection, *};
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use image::{Rgba, RgbaImage};
use plotters::prelude::*;
use rayon::prelude::*;
use std::{io::Write, path::Path, time::Instant};
use tracing::{debug_span, info};

#[derive(Debug, Clone)]
pub struct SectionPlotOptions {
    pub style: PlotStyle,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub palette: Palette,
}

impl Default for SectionPlotOptions {
    fn default() -> Self {
        Self {
            style: PlotStyle::Black,
            width: 3000,
            height: 1500,
            title: "rustcinrad section".into(),
            palette: Palette::reflectivity(),
        }
    }
}

fn output(e: impl std::fmt::Display) -> RadarError {
    Output(e.to_string())
}

fn edges(centres: &[f64]) -> Vec<f64> {
    let mut edges = Vec::with_capacity(centres.len() + 1);
    edges.push(centres[0] - (centres[1] - centres[0]) / 2.0);
    edges.extend(centres.windows(2).map(|p| (p[0] + p[1]) / 2.0));
    let n = centres.len();
    edges.push(centres[n - 1] + (centres[n - 1] - centres[n - 2]) / 2.0);
    edges
}

pub fn write_section_png(section: &GeoSection, path: impl AsRef<Path>, options: &SectionPlotOptions) -> Result<()> {
    let path = path.as_ref();
    let _span = debug_span!("plot_section", path = %path.display(), dtype = %section.field.name()).entered();
    let started = Instant::now();
    let nx = section.distances_m.len();
    let ny = section.heights_m.len();
    if nx < 2
        || ny < 2
        || nx.checked_mul(ny).is_none_or(|n| n > 16_000_000)
        || section.points.len() != nx
        || section.values.dim() != (ny, nx)
        || section.states.dim() != (ny, nx)
        || [&section.distances_m, &section.heights_m]
            .iter()
            .any(|axis| axis.iter().any(|v| !v.is_finite()) || axis.windows(2).any(|p| p[1] <= p[0]))
        || section.values.iter().zip(section.states.iter()).any(|(v, s)| *s == CellState::Valid && !v.is_finite())
        || options.width < 400
        || options.height < 300
        || u64::from(options.width) * u64::from(options.height) > 32_000_000
    {
        return Err(Invalid("invalid section shape/axes or PNG dimensions".into()));
    }
    let xedges = edges(&section.distances_m);
    let yedges = edges(&section.heights_m);
    let (xmin, xmax) = (xedges[0], xedges[nx]);
    let (ymin, ymax) = (yedges[0], yedges[ny]);
    let background = if options.style == PlotStyle::White { WHITE } else { BLACK };
    let foreground = if options.style == PlotStyle::White { BLACK } else { WHITE };
    let missing =
        if options.style == PlotStyle::Transparent { [1, 2, 3] } else { [background.0, background.1, background.2] };
    let mut rgb = vec![0; options.width as usize * options.height as usize * 3];
    super::font::register()?;
    let bounds;
    let cells;
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (options.width, options.height)).into_drawing_area();
        root.fill(&background).map_err(output)?;
        let (area, legend) = root.split_horizontally(options.width - 110);
        let mut chart = ChartBuilder::on(&area)
            .margin(12)
            .margin_top(38)
            .x_label_area_size(40)
            .y_label_area_size(55)
            .build_cartesian_2d(xmin / 1000.0..xmax / 1000.0, ymin / 1000.0..ymax / 1000.0)
            .map_err(output)?;
        bounds = chart.plotting_area().get_pixel_range();
        let (xr, yr) = &bounds;
        let w = (xr.end - xr.start) as usize;
        let h = (yr.end - yr.start) as usize;
        cells = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let x = xmin + (xmax - xmin) * (i % w) as f64 / w as f64 + (xmax - xmin) / (2.0 * w as f64);
                let y = ymax - (ymax - ymin) * (i / w) as f64 / h as f64 - (ymax - ymin) / (2.0 * h as f64);
                (yedges.partition_point(|v| *v <= y) - 1, xedges.partition_point(|v| *v <= x) - 1)
            })
            .collect::<Vec<_>>();
        let colors: Vec<_> =
            section
                .values
                .iter()
                .zip(section.states.iter())
                .map(|(&v, &s)| {
                    if s == CellState::Valid {
                        options.palette.color(v, section.field.is_categorical())
                    } else {
                        missing
                    }
                })
                .collect();
        let mut raster = vec![0; w * h * 3];
        raster.par_chunks_mut(3).zip(cells.par_iter()).for_each(|(p, &(r, c))| p.copy_from_slice(&colors[r * nx + c]));
        let bitmap = BitMapElement::with_owned_buffer((0, 0), (w as u32, h as u32), raster)
            .ok_or_else(|| Output("invalid section bitmap".into()))?;
        chart.plotting_area().strip_coord_spec().draw(&bitmap).map_err(output)?;
        chart
            .configure_mesh()
            .x_desc("Along path (km)")
            .y_desc("Height MSL (km)")
            .axis_style(foreground)
            .label_style((super::font::FAMILY, 14).into_font().color(&foreground))
            .light_line_style(foreground.mix(0.25))
            .bold_line_style(foreground.mix(0.25))
            .draw()
            .map_err(output)?;
        let min = options.palette.stops[0].0;
        let max = options.palette.stops[options.palette.stops.len() - 1].0;
        for y in 60..options.height as i32 - 60 {
            let v = max - (max - min) * (y - 60) as f32 / (options.height as f32 - 120.0);
            let c = options.palette.color(v, section.field.is_categorical());
            legend
                .draw(&Rectangle::new([(10, y), (35, y + 1)], RGBColor(c[0], c[1], c[2]).filled()))
                .map_err(output)?;
        }
        for k in 0..=5 {
            legend
                .draw(&Text::new(
                    format!("{:.1}", max - (max - min) * k as f32 / 5.0),
                    (42, 60 + (options.height as i32 - 120) * k / 5),
                    (super::font::FAMILY, 14).into_font().color(&foreground),
                ))
                .map_err(output)?;
        }
        root.present().map_err(output)?;
    }
    super::font::labels(
        &mut rgb,
        options.width,
        options.height,
        &[(format!("{} | {} [{}]", options.title, section.field.name(), section.field.unit()), 12, 8)],
        17,
        [foreground.0, foreground.1, foreground.2],
    )?;
    let mut image = RgbaImage::from_fn(options.width, options.height, |x, y| {
        let i = (y as usize * options.width as usize + x as usize) * 3;
        Rgba([rgb[i], rgb[i + 1], rgb[i + 2], 255])
    });
    let (xr, yr) = bounds;
    for y in yr.clone() {
        for x in xr.clone() {
            let cell = cells[(y - yr.start) as usize * (xr.end - xr.start) as usize + (x - xr.start) as usize];
            if options.style == PlotStyle::Transparent
                && section.states[cell] != CellState::Valid
                && image.get_pixel(x as u32, y as u32).0[..3] == [1, 2, 3]
            {
                image.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 0]));
            }
        }
    }
    let mut writer = std::io::BufWriter::new(std::fs::File::create(path)?);
    PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::Sub)
        .write_image(image.as_raw(), options.width, options.height, image::ExtendedColorType::Rgba8)
        .map_err(output)?;
    writer.flush()?;
    info!(path = %path.display(), dtype = %section.field.name(), elapsed_ms = started.elapsed().as_secs_f64() * 1000.0, "visualize(png)");
    Ok(())
}

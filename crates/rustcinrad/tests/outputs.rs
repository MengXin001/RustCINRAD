//! Integration regression tests.
#![allow(clippy::unwrap_used)]
use ndarray::array;
use rustcinrad::{
    export,
    visualize::{self, Palette, PlotOptions},
    *,
};
use std::{fs::File, io::BufReader};
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};

fn grid() -> GeoGrid {
    GeoGrid::new(
        GridSpec::new(120.0, 30.0, 0.1, 0.1, 2, 2).unwrap(),
        FieldId::Reflectivity,
        array![[0.0, 10.0], [20.0, 30.0]],
        array![[CellState::Valid, CellState::Missing], [CellState::RangeFolded, CellState::OutsideCoverage]],
        array![[100.0, 100.0], [100.0, 100.0]],
        1_700_000_000_000,
    )
    .unwrap()
}

#[test]
fn geotiff_roundtrip_tags_units_nodata_and_mask() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("radar.tif");
    let g = grid();
    export::write_geotiff(&g, &path).unwrap();
    let mut d = Decoder::new(BufReader::new(File::open(&path).unwrap())).unwrap();
    assert_eq!(d.dimensions().unwrap(), (2, 2));
    assert_eq!(d.get_tag_f64_vec(Tag::ModelPixelScaleTag).unwrap(), [0.1, 0.1, 0.0]);
    assert_eq!(d.get_tag_f64_vec(Tag::ModelTiepointTag).unwrap(), [0.0, 0.0, 0.0, 119.95, 30.05, 0.0]);
    let keys = d.get_tag_u16_vec(Tag::GeoKeyDirectoryTag).unwrap();
    assert!(keys.windows(4).any(|k| k == [2048, 0, 1, 4326]));
    assert_eq!(d.get_tag_ascii_string(Tag::Unknown(42113)).unwrap(), "nan");
    assert!(d.get_tag_ascii_string(Tag::Unknown(42112)).unwrap().contains("dBZ"));
    let DecodingResult::F32(v) = d.read_image().unwrap() else {
        panic!("expected float raster");
    };
    assert_eq!(v[0], 0.0);
    assert!(v[1..].iter().all(|v| v.is_nan()));
    d.next_image().unwrap();
    let DecodingResult::U8(mask) = d.read_image().unwrap() else {
        panic!("expected mask");
    };
    assert_eq!(mask, [0, 1, 2, 3]);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn png_has_north_up_palette_and_transparency() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("radar.png");
    visualize::write_png(
        &grid(),
        &path,
        &PlotOptions { width: 600, height: 500, style: visualize::PlotStyle::Transparent, ..Default::default() },
    )
    .unwrap();
    let image = image::open(path).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (600, 500));
    let zero = Palette::reflectivity().color(0.0, false);
    // Top-left quadrant is valid zero; bottom quadrants are invalid.
    let a = image.get_pixel(150, 150).0;
    assert_eq!(a, [zero[0], zero[1], zero[2], 255]);
    assert_eq!(image.get_pixel(350, 150).0[3], 0);
    assert_eq!(image.get_pixel(150, 350).0, [102, 0, 102, 255]);
}

#[test]
fn palette_and_grid_validation() {
    assert!(Palette::new(vec![(1.0, [0, 0, 0]), (1.0, [255, 255, 255])]).is_err());
    let palette = Palette::new(vec![(0.0, [0, 0, 0]), (10.0, [200, 100, 50])]).unwrap();
    assert_eq!(palette.color(5.0, true), [0, 0, 0]);
    assert_eq!(palette.color(5.0, false), [100, 50, 25]);
    assert!(GridSpec::new(0.0, 30.0, 0.1, 0.1, usize::MAX, 2).is_err());
}

#[test]
fn geographic_section_flat_cells_and_transparent_missing() {
    let section = rustcinrad::calc::GeoSection {
        points: vec![GeoPoint::new(120.0, 30.0).unwrap(), GeoPoint::new(120.1, 30.0).unwrap()],
        distances_m: vec![0.0, 10000.0],
        heights_m: vec![0.0, 10000.0],
        values: array![[0.0, 10.0], [20.0, 30.0]],
        states: array![[CellState::Valid, CellState::Missing], [CellState::Valid, CellState::Valid]],
        field: FieldId::Reflectivity,
    };
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("section.png");
    visualize::write_section_png(
        &section,
        &path,
        &visualize::SectionPlotOptions {
            width: 600,
            height: 500,
            style: visualize::PlotStyle::Transparent,
            ..Default::default()
        },
    )
    .unwrap();
    let im = image::open(path).unwrap().to_rgba8();
    let p = Palette::reflectivity().color(20.0, false);
    assert_eq!(im.get_pixel(150, 150).0, [p[0], p[1], p[2], 255]);
    assert_eq!(im.get_pixel(350, 350).0[3], 0);
}

#[test]
fn projected_station_map_wgs84_has_valid_zero_at_centre() {
    let d = temp_dir::TempDir::new().unwrap();
    let path = d.path().join("aeqd.png");
    visualize::write_png(
        &grid(),
        &path,
        &PlotOptions {
            width: 600,
            height: 500,
            style: visualize::PlotStyle::Transparent,
            projection: visualize::MapProjection::AzimuthalEquidistant(GeoPoint::new(120.0, 30.0).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let im = image::open(path).unwrap().to_rgba8();
    assert!(im.pixels().any(|p| p.0[3] == 0));
    let color = Palette::reflectivity().color(0.0, false);
    assert!(im.pixels().any(|p| p.0 == [color[0], color[1], color[2], 255]));
}

#[test]
fn layered_geotiff_preserves_components_heights_and_georeferences_every_page() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("volume.tif");
    let first = grid();
    let second = GeoGrid::new(
        first.spec(),
        FieldId::EastwardWind,
        array![[12.0, 13.0], [14.0, 15.0]],
        array![[CellState::Valid, CellState::Rejected], [CellState::Valid, CellState::Missing]],
        ndarray::Array2::from_elem((2, 2), 1000.0),
        first.time_unix_ms(),
    )
    .unwrap();
    export::write_geotiff_layers(
        &[
            export::GeoTiffLayer { grid: &first, height_msl_m: Some(100.0) },
            export::GeoTiffLayer { grid: &second, height_msl_m: Some(1000.0) },
        ],
        &path,
    )
    .unwrap();
    let mut d = Decoder::new(BufReader::new(File::open(&path).unwrap())).unwrap();
    for (page, unit, height) in [(0, "dBZ", "100"), (1, "", ""), (2, "m/s", "1000"), (3, "", "")] {
        assert_eq!(d.dimensions().unwrap(), (2, 2));
        assert_eq!(d.get_tag_f64_vec(Tag::ModelPixelScaleTag).unwrap(), [0.1, 0.1, 0.0]);
        assert!(d.get_tag_u16_vec(Tag::GeoKeyDirectoryTag).unwrap().windows(4).any(|k| k == [2048, 0, 1, 4326]));
        if page % 2 == 0 {
            let metadata = d.get_tag_ascii_string(Tag::Unknown(42112)).unwrap();
            assert!(metadata.contains(unit));
            assert!(metadata.contains(&format!("<Item name=\"HEIGHT_MSL_M\">{height}</Item>")));
            assert!(!metadata.contains("4979"));
            let DecodingResult::F32(values) = d.read_image().unwrap() else { panic!("expected data page") };
            assert_eq!(values[0], if page == 0 { 0.0 } else { 12.0 });
            assert!(values[1].is_nan());
        } else {
            let DecodingResult::U8(mask) = d.read_image().unwrap() else { panic!("expected mask page") };
            assert_eq!(mask, if page == 1 { vec![0, 1, 2, 3] } else { vec![0, 4, 0, 1] });
        }
        if page < 3 {
            d.next_image().unwrap();
        }
    }
    assert!(!d.more_images());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn invalid_layer_metadata_is_rejected_before_creating_file() {
    let dir = temp_dir::TempDir::new().unwrap();
    let path = dir.path().join("invalid.tif");
    let first = grid();
    assert!(export::write_geotiff_layers(&[], &path).is_err());
    for height in [f64::NAN, f64::INFINITY, 101.0] {
        assert!(
            export::write_geotiff_layers(&[export::GeoTiffLayer { grid: &first, height_msl_m: Some(height) },], &path)
                .is_err()
        );
    }
    let unaligned = GeoGrid::new(
        GridSpec::new(121.0, 30.0, 0.1, 0.1, 2, 2).unwrap(),
        first.field(),
        first.values().clone(),
        first.states().clone(),
        first.heights_m().clone(),
        first.time_unix_ms(),
    )
    .unwrap();
    assert!(
        export::write_geotiff_layers(
            &[
                export::GeoTiffLayer { grid: &first, height_msl_m: None },
                export::GeoTiffLayer { grid: &unaligned, height_msl_m: None },
            ],
            &path
        )
        .is_err()
    );
    assert!(!path.exists());
}

#[test]
fn default_row_buffer_and_transparent_full_map_preserve_valid_cell_colors() {
    let dir = temp_dir::TempDir::new().unwrap();
    let reference = Palette::reflectivity().color(0.0, false);
    for (index, style) in [visualize::PlotStyle::Black, visualize::PlotStyle::White, visualize::PlotStyle::Transparent]
        .into_iter()
        .enumerate()
    {
        let path = dir.path().join(format!("{index}.png"));
        visualize::write_png(&grid(), &path, &PlotOptions { width: 600, height: 500, style, ..Default::default() })
            .unwrap();
        let image = image::open(path).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(150, 150).0, [reference[0], reference[1], reference[2], 255]);
        assert_eq!(image.get_pixel(150, 350).0, [102, 0, 102, 255]);
    }
}

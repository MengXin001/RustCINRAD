//! Metadata, orientation, resolution and truncation for native legacy/raster formats.
#![allow(clippy::unwrap_used)]
mod support;

use rustcinrad::{
    projection::{self, BeamModel},
    *,
};
use support::*;

fn point(d: &RadarDataset, bearing: f64, range: f64) -> GeoPoint {
    let (distance, _) =
        BeamModel::default().locate(range, d.summary().sweeps[0].elevation_deg, d.summary().site.altitude_m).unwrap();
    projection::destination(d.summary().site.position, bearing, distance).unwrap()
}

#[test]
fn native_cc_cd_cc2_metadata_and_calibration() {
    for (bytes, format) in [(cc(), RadarFormat::Cc), (cd(), RadarFormat::Cd), (cc2(), RadarFormat::Cc2)] {
        let d = read_bytes(&bytes, "unknown", &ReadOptions::default()).unwrap();
        assert_eq!(d.summary().format, format);
        assert_eq!(d.summary().site.code, "Z1234");
        assert_eq!(d.summary().site.name, "Native site");
        assert!((d.summary().site.position.longitude() - 120.0).abs() < 1e-10);
        assert!((d.summary().site.position.latitude() - 30.0).abs() < 1e-10);
        assert_eq!(d.summary().site.altitude_m, 100.0);
        let value = d.sample(0, FieldId::Reflectivity, point(&d, 0.0, 600.0), BeamModel::default()).unwrap().unwrap();
        assert_eq!(value.0, 30.0);
        assert_eq!(value.1, CellState::Valid);
        assert!(read_bytes(&bytes[..bytes.len() - 1], "unknown", &ReadOptions::default()).is_err());
    }
}

#[test]
fn legacy_pa_interleaved_scans_masks_and_native_location() {
    let d = read_bytes(
        &legacy_pa(),
        "Z_RADR_I_Z1234_test_PA.bin",
        &ReadOptions { format: Some(RadarFormat::LegacyPhasedArray), ..Default::default() },
    )
    .unwrap();
    assert_eq!(d.summary().sweeps.len(), 2);
    assert_eq!(d.summary().sweeps[0].ray_count, 2);
    assert_eq!(d.summary().sweeps[1].ray_count, 2);
    assert!((d.summary().site.position.longitude() - 120.0).abs() < 1e-8);
    assert_eq!(
        d.sample(0, FieldId::Reflectivity, point(&d, 0.0, 1000.0), BeamModel::default()).unwrap().unwrap().1,
        CellState::Missing
    );
    let observed = d.sample(0, FieldId::Reflectivity, point(&d, 0.0, 2000.0), BeamModel::default()).unwrap().unwrap();
    assert!((observed.0 - (100.0 * 106.0 / 255.0 - 20.0)).abs() < 1e-5);
}

#[test]
fn swan_axes_reversed_to_north_up_and_height_units() {
    let d = read_bytes(&swan(), "swan", &ReadOptions::default()).unwrap();
    let spec = d.grid_spec(0, FieldId::CompositeReflectivity, BeamModel::default()).unwrap();
    let g = d.grid(0, FieldId::CompositeReflectivity, spec, BeamModel::default()).unwrap();
    assert_eq!(g.values()[(0, 0)], 2.0);
    assert_eq!(g.values()[(1, 0)], 0.0);
    assert_eq!(g.states()[(0, 1)], CellState::Missing);
    assert_eq!(g.heights_m()[(0, 0)], 1000.0);
    assert_eq!(spec.north(), 31.0);
    assert_eq!(spec.west(), 120.0);
}

#[test]
fn moc_north_up_does_not_reverse_height_axis() {
    for single in [false, true] {
        let d = read_bytes(&moc(single), "moc", &ReadOptions::default()).unwrap();
        let field = if single { FieldId::Reflectivity } else { FieldId::CompositeReflectivity };
        let spec = d.grid_spec(0, field, BeamModel::default()).unwrap();
        let g = d.grid(0, field, spec, BeamModel::default()).unwrap();
        assert_eq!(g.values()[(0, 0)], 10.0);
        assert_eq!(g.states()[(1, 0)], CellState::Missing);
        assert_eq!(g.heights_m()[(0, 0)], 1000.0);
        if single {
            let second = d.grid(1, field, spec, BeamModel::default()).unwrap();
            assert_eq!(second.values()[(0, 0)], 20.0);
            assert_eq!(second.heights_m()[(0, 0)], 2000.0);
            assert_eq!(d.summary().site.code, "Z1234");
        }
    }
}

#[test]
fn decoded_cell_and_ray_limits_and_legacy_metadata_error() {
    let b = cc();
    let mut o = ReadOptions { max_decoded_cells: 1, ..Default::default() };
    assert!(read_bytes(&b, "cc", &o).is_err());
    o.max_decoded_cells = 100;
    o.max_rays = 1;
    assert!(read_bytes(&b, "cc", &o).is_err());
    assert!(
        read_bytes(&sab(0.0, 1, true), "Z1234.bin", &ReadOptions::default())
            .unwrap_err()
            .to_string()
            .contains("no station coordinates")
    );
    let d = read_bytes(&sab(0.0, 1, true), "Z_RADR_I_Z9411.bin", &ReadOptions::default()).unwrap();
    assert_eq!(d.summary().site.code, "Z9411");
    assert!((d.summary().site.position.longitude() - 121.64888888888889).abs() < 1e-10);
}

#[test]
fn station_header_precedes_json_and_missing_header_falls_back() {
    let mut b = fmt(false, 1, 2);
    b[32..40].fill(0);
    b[32..37].copy_from_slice(b"Z9411");
    b[40..72].fill(0);
    b[40..46].copy_from_slice(b"Native");
    let d = read_bytes(&b, "", &ReadOptions::default()).unwrap();
    assert_eq!(d.summary().site.name, "Native");
    assert_eq!(d.summary().site.position.longitude(), 120.0);
    b[72..80].fill(0);
    b[40..72].fill(0);
    let d = read_bytes(&b, "", &ReadOptions::default()).unwrap();
    assert!((d.summary().site.position.longitude() - 121.64888888888889).abs() < 1e-10);
    assert!(!d.summary().site.name.is_empty());
}

#[test]
fn cd_native_binary_angle_and_north_crossing() {
    let mut b = cd();
    u16_at(&mut b, 1024, 16300);
    u16_at(&mut b, 1028, 16468);
    let d = read_bytes(&b, "", &ReadOptions::default()).unwrap();
    let v = d.sample(0, FieldId::Reflectivity, point(&d, 90.0, 600.0), BeamModel::default()).unwrap().unwrap();
    assert_eq!(v.0, 30.0);
}

#[test]
fn standard_pup_radial_words_masks_and_metadata() {
    for word in [1, 2] {
        let b = standard_pup(word);
        let d = read_bytes(&b, "", &ReadOptions::default()).unwrap();
        assert_eq!(d.summary().format, RadarFormat::StandardPup);
        assert_eq!(d.summary().site.name, "Native");
        for (range, state, value) in [
            (1000.0, CellState::Missing, 0.0),
            (2000.0, CellState::RangeFolded, 0.0),
            (3000.0, CellState::Valid, 0.0),
            (4000.0, CellState::Valid, 20.0),
        ] {
            let s = d.sample(0, FieldId::Reflectivity, point(&d, 90.0, range), BeamModel::default()).unwrap().unwrap();
            assert_eq!((s.1, s.0), (state, value));
        }
        assert!(read_bytes(&b[..b.len() - 1], "", &ReadOptions::default()).is_err());
    }
}

#[test]
fn swan_crosses_dateline_without_reversing_data_columns() {
    let mut b = swan();
    f32_at(&mut b, 88, 179.0);
    f32_at(&mut b, 96, -180.0);
    let d = read_bytes(&b, "", &ReadOptions::default()).unwrap();
    let spec = d.grid_spec(0, FieldId::CompositeReflectivity, BeamModel::default()).unwrap();
    assert_eq!(spec.west(), 179.0);
    assert_eq!(spec.longitude_step(), 2.0);
    assert!(spec.pixel(GeoPoint::new(-179.0, 30.0).unwrap()).is_some());
}
proptest::proptest! {
    #[test]
    fn mutated_native_headers_never_panic(which in 0usize..7,offset in 0usize..2200,value in proptest::prelude::any::<u8>()) {
        let mut b=match which {0=>cc(),1=>cd(),2=>cc2(),3=>legacy_pa(),4=>swan(),5=>moc(true),_=>standard_pup(2)};
        let n=b.len();b[offset%n]=value;
        let options=ReadOptions {format: if which==3 {Some(RadarFormat::LegacyPhasedArray)} else {None}, max_decoded_cells:100000,max_rays:1000,..Default::default()};
        let _=read_bytes(&b,"Z9411",&options);
    }
}

//! Integration regression tests.
#![allow(clippy::unwrap_used)]
mod support;

use rustcinrad::{
    projection::{self, BeamModel},
    *,
};
use support::*;

fn observed(d: &RadarDataset, field: FieldId, range: f64) -> (f32, CellState, f32) {
    let (ground, _) = BeamModel::default().locate(range, d.summary().sweeps[0].elevation_deg, 100.0).unwrap();
    let point = projection::destination(site().position, 0.0, ground).unwrap();
    d.sample(0, field, point, BeamModel::default()).unwrap().unwrap()
}

#[test]
fn sab_offsets_negative_ref_and_folding() {
    let d = read_bytes(&sab(0.0, 1, true), "test", &options()).unwrap();
    assert_eq!(observed(&d, FieldId::Reflectivity, 1000.0).1, CellState::Missing);
    assert_eq!(observed(&d, FieldId::Reflectivity, 2000.0).0, -32.5);
    assert_eq!(observed(&d, FieldId::Reflectivity, 3000.0).0, 0.0);
    assert_eq!(observed(&d, FieldId::Velocity, 500.0).1, CellState::RangeFolded);
    assert_eq!(observed(&d, FieldId::Velocity, 750.0).0, 0.0);
    assert_eq!(observed(&d, FieldId::Velocity, 1000.0).0, 20.0);
    assert_eq!(d.summary().sweeps[0].time_unix_ms, 1_533_513_601_000);
    assert!(d.summary().sweeps[0].complete);
}

#[test]
fn incomplete_sweep_and_field_resolution_preserved() {
    let mut bytes = sab(359.0, 9, false);
    bytes.extend(sab(0.0, 10, false));
    let d = read_bytes(&bytes, "test", &options()).unwrap();
    assert_eq!(d.summary().sweeps.len(), 1);
    assert!(!d.summary().sweeps[0].complete);
    let fields = &d.summary().sweeps[0].fields;
    assert_eq!(fields.iter().find(|f| f.field == FieldId::Velocity).unwrap().range_resolution_m, 250.0);
    let hole = projection::destination(site().position, 90.0, 2000.0).unwrap();
    assert!(d.sample(0, FieldId::Reflectivity, hole, BeamModel::default()).unwrap().is_none());
}

#[test]
fn truncated_bad_gate_counts_and_pointers_fail() {
    let b = sab(0.0, 1, true);
    for n in [0, 27, 127, 128, 2431] {
        assert!(read_bytes(&b[..n], "test", &options()).is_err());
    }
    for (offset, value) in [(54, 65535), (64, 65535), (50, 0), (32, 0), (38, 0)] {
        let mut corrupt = b.clone();
        u16_at(&mut corrupt, offset, value);
        assert!(read_bytes(&corrupt, "test", &options()).is_err(), "offset {offset}");
    }
}

#[test]
fn fmt_standard_pa_u8_u16_and_unknown_moments() {
    for pa in [false, true] {
        for bins in [1, 2] {
            let d = read_bytes(&fmt(pa, bins, 2), "fmt", &ReadOptions::default()).unwrap();
            assert_eq!(observed(&d, FieldId::Reflectivity, 3000.0).0, 0.0);
            assert_eq!(observed(&d, FieldId::Reflectivity, 4000.0).0, 20.0);
            assert_eq!(d.summary().sweeps[0].time_unix_ms, 1_700_000_000_000);
        }
    }
    let d = read_bytes(&fmt(false, 1, 123), "fmt", &ReadOptions::default()).unwrap();
    assert_eq!(d.summary().sweeps[0].fields[0].field, FieldId::Other(123));
}

#[test]
fn fmt_malformed_lengths_and_scales_fail() {
    let b = fmt(false, 2, 2);
    for n in [32, 415, 671, 735, b.len() - 1] {
        assert!(read_bytes(&b[..n], "fmt", &ReadOptions::default()).is_err());
    }
    for (o, v) in [(336, 0), (736 + 4, 0), (736 + 16, -1), (672 + 36, 100000)] {
        let mut b = b.clone();
        i32_at(&mut b, o, v);
        assert!(read_bytes(&b, "fmt", &ReadOptions::default()).is_err());
    }
}

#[test]
fn compressed_inputs_and_expansion_limits() {
    use std::io::Write;
    let b = sab(0.0, 1, true);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&b).unwrap();
    let gzip = gzip.finish().unwrap();
    assert!(read_bytes(&gzip, "test", &options()).is_ok());
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    bz.write_all(&b).unwrap();
    let bz = bz.finish().unwrap();
    assert!(read_bytes(&bz, "test", &options()).is_ok());
    let mut o = options();
    o.max_bytes = 1000;
    assert!(read_bytes(&gzip, "test", &o).is_err());
    assert!(read_bytes(&bz, "test", &o).is_err());
}
proptest::proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(),0..6000)) {
        let _=read_bytes(&bytes,"test",&options());
        let _=read_bytes(&bytes,"test",&ReadOptions::default());
    }
}

#[test]
#[ignore = "local 9.8 MiB SAB fixture; set RUSTCINRAD_SAB_SAMPLE to run"]
fn local_sample_baseline() {
    let path = std::env::var("RUSTCINRAD_SAB_SAMPLE").unwrap();
    let d = read_path(path, &ReadOptions::default()).unwrap();
    assert_eq!(d.summary().site.code, "Z9411");
    assert_eq!(d.summary().sweeps.iter().map(|s| s.ray_count).sum::<usize>(), 4032);
    assert!((d.summary().site.position.longitude() - 121.64888888888889).abs() < 1e-8);
    assert_eq!(d.summary().sweeps.len(), 11);
    assert_eq!(d.summary().sweeps[0].time_unix_ms, 1533574972199);
}

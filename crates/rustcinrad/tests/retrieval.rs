//! Analytic precipitation, CAPPI/3D, native-gate correction and time-series regressions.
#![allow(clippy::unwrap_used)]
mod support;

use ndarray::array;
use rustcinrad::{
    calc::{Product, ProductRequest},
    correct::PolarimetricOptions,
    projection::{self, BeamModel},
    retrieve::*,
    *,
};

fn frame(values: [f32; 2], states: [CellState; 2], time: i64, field: FieldId) -> GeoGrid {
    GeoGrid::new(
        GridSpec::new(120.0, 30.0, 0.01, 0.01, 2, 1).unwrap(),
        field,
        array![[values[0], values[1]]],
        array![[states[0], states[1]]],
        array![[100.0, 100.0]],
        time,
    )
    .unwrap()
}

fn native_volume(elevations: &[f32], moments: &[(i32, i32, i32, Vec<u16>)]) -> RadarDataset {
    let template = support::fmt(false, 2, 2);
    let mut bytes = template[..416].to_vec();
    support::i32_at(&mut bytes, 336, elevations.len() as i32);
    for _ in elevations {
        bytes.extend_from_slice(&template[416..672]);
    }
    for (scan, &elevation) in elevations.iter().enumerate() {
        let mut ray = template[672..736].to_vec();
        support::i32_at(&mut ray, 16, scan as i32 + 1);
        support::f32_at(&mut ray, 24, elevation);
        support::i32_at(&mut ray, 40, moments.len() as i32);
        let length: usize = moments.iter().map(|m| 32 + 2 * m.3.len()).sum();
        support::i32_at(&mut ray, 36, length as i32);
        bytes.extend_from_slice(&ray);
        for (code, scale, offset, data) in moments {
            let mut header = vec![0; 32];
            support::i32_at(&mut header, 0, *code);
            support::i32_at(&mut header, 4, *scale);
            support::i32_at(&mut header, 8, *offset);
            support::u16_at(&mut header, 12, 2);
            support::i32_at(&mut header, 16, 2 * data.len() as i32);
            bytes.extend(header);
            for &value in data {
                bytes.extend(value.to_le_bytes());
            }
        }
    }
    read_bytes(&bytes, "native.bin", &ReadOptions::default()).unwrap()
}

#[test]
fn qpe_units_masks_and_analytic_zr() {
    let grid = frame([30.0, 0.0], [CellState::Valid, CellState::Missing], 1, FieldId::Reflectivity);
    let result = rain_rate(&grid, Default::default()).unwrap();
    assert!((f64::from(result.values()[(0, 0)]) - 5.0_f64.powf(1.0 / 1.6)).abs() < 1e-5);
    assert_eq!(result.field().unit(), "mm/h");
    assert_eq!(result.states(), grid.states());
    assert!(rain_rate(&grid, QpeOptions { zr_a: 0.0, ..Default::default() }).is_err());
    assert!(matches!(rain_rate_with_progress(&grid, Default::default(), |_| false), Err(RadarError::Cancelled)));
    let rate = frame([1.0, 2.0], [CellState::Valid; 2], 0, FieldId::Velocity);
    assert!(rain_rate(&rate, Default::default()).is_err());
}

#[test]
fn time_integration_is_trapezoidal_gap_aware_and_not_extrapolated() {
    let a = frame([2.0, 2.0], [CellState::Valid; 2], 0, FieldId::RainRate);
    let b = frame([4.0, 4.0], [CellState::Valid, CellState::Missing], 600_000, FieldId::RainRate);
    let c = frame([6.0, 6.0], [CellState::Valid; 2], 1_200_000, FieldId::RainRate);
    let result = accumulate(&[&a, &b, &c], Default::default()).unwrap();
    assert!((result.depth.values()[(0, 0)] - 4.0 / 3.0).abs() < 1e-6);
    assert_eq!(result.coverage_seconds[(0, 0)], 1200.0);
    assert_eq!(result.coverage_seconds[(0, 1)], 0.0);
    assert_eq!(result.depth.states()[(0, 1)], CellState::Missing);
    assert_eq!(result.depth.field().unit(), "mm");
    assert!(accumulate(&[&b, &a], Default::default()).is_err());
    assert!(accumulate(&[&a, &a], Default::default()).is_err());
    let result = accumulate(&[&a, &c], Default::default()).unwrap();
    assert_eq!(result.depth.states()[(0, 0)], CellState::Missing);
    let d = frame([2.0, 2.0], [CellState::Valid; 2], 1_800_000, FieldId::RainRate);
    let partial = accumulate(&[&a, &b, &c, &d], Default::default()).unwrap();
    assert_eq!(partial.depth.states()[(0, 1)], CellState::Rejected);
    assert_eq!(partial.coverage_seconds[(0, 1)], 600.0);
    assert!(matches!(accumulate_with_progress(&[&a, &b], Default::default(), |_| false), Err(RadarError::Cancelled)));
}

#[test]
fn cappi_brackets_beam_heights_and_preserves_vertical_coverage() {
    let d = native_volume(&[1.0, 5.0], &[(2, 2, 0, vec![60; 40])]);
    let beam = BeamModel::default();
    let point = projection::destination(d.summary().site.position, 0.0, 10_000.0).unwrap();
    let spec = GridSpec::new(point.longitude(), point.latitude(), 0.001, 0.001, 1, 1).unwrap();
    let low = d.sample(0, FieldId::Reflectivity, point, beam).unwrap().unwrap();
    let high = d.sample(1, FieldId::Reflectivity, point, beam).unwrap().unwrap();
    let height = (f64::from(low.2) + f64::from(high.2)) / 2.0;
    let request = ProductRequest { product: Product::Cappi { height_m: height, max_gap_m: 3000.0 }, grid: spec, beam };
    let cappi = d.product(request).unwrap();
    assert_eq!(cappi.values()[(0, 0)], 30.0);
    assert_eq!(cappi.states()[(0, 0)], CellState::Valid);
    assert!((f64::from(cappi.heights_m()[(0, 0)]) - height).abs() < 1e-4);
    assert!(matches!(d.product_with_progress(request, |_| false), Err(RadarError::Cancelled)));
    let outside = d
        .product(ProductRequest { product: Product::Cappi { height_m: 10000.0, max_gap_m: 3000.0 }, ..request })
        .unwrap();
    assert_eq!(outside.states()[(0, 0)], CellState::OutsideCoverage);
    let volume = cappi_volume(&d, spec, &[height, height + 100.0], 3000.0, beam).unwrap();
    let combined = mosaic_volumes(&[&volume, &volume], 0).unwrap();
    assert_eq!(combined.layers[0].values(), volume.layers[0].values());
    assert!(cappi_volume(&d, spec, &[1000.0, 500.0], 3000.0, beam).is_err());
    assert!(
        d.product(ProductRequest { product: Product::Cappi { height_m: height, max_gap_m: -1.0 }, ..request }).is_err()
    );
}

#[test]
fn native_attenuation_includes_current_gate_and_resets_after_qc_rejection() {
    let mut rho = vec![99; 40];
    rho[9] = 50;
    let d = native_volume(&[0.5], &[(2, 2, 0, vec![60; 40]), (9, 100, 0, rho), (11, 10, 0, vec![20; 40])]);
    let beam = BeamModel::default();
    let (ground, _) = beam.locate(12_000.0, 0.5, 100.0).unwrap();
    let point = projection::destination(d.summary().site.position, 0.0, ground).unwrap();
    let spec = GridSpec::new(point.longitude(), point.latitude(), 0.001, 0.001, 1, 1).unwrap();
    let options = PolarimetricOptions {
        maximum_phase_texture_deg: None,
        attenuation_band: Some(rustcinrad::correct::RadarBand::C),
        ..Default::default()
    };
    let corrected = d.polarimetric_grid(0, FieldId::Reflectivity, spec, beam, options).unwrap();
    // Gate 10 rejects low RHO; gates 11 and 12 each add 2*0.08*2*1 km = 0.32 dB.
    assert!((corrected.values()[(0, 0)] - 30.64).abs() < 1e-4);
    let (ground, _) = beam.locate(10_000.0, 0.5, 100.0).unwrap();
    let point = projection::destination(d.summary().site.position, 0.0, ground).unwrap();
    let spec = GridSpec::new(point.longitude(), point.latitude(), 0.001, 0.001, 1, 1).unwrap();
    let rejected = d.polarimetric_grid(0, FieldId::Reflectivity, spec, beam, options).unwrap();
    assert_eq!(rejected.states()[(0, 0)], CellState::Rejected);
    assert!(matches!(
        d.polarimetric_grid_with_progress(0, FieldId::Reflectivity, spec, beam, options, |_| false),
        Err(RadarError::Cancelled)
    ));
}

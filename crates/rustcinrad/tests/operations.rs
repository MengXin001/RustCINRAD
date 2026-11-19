//! Integration regression tests.
#![allow(clippy::unwrap_used)]
mod support;

use rustcinrad::{
    calc::{self, Product, ProductRequest},
    projection::{self, BeamModel},
    *,
};

#[test]
fn cancellation_and_missing_field_are_results() {
    let d = support::volume();
    let spec = GridSpec::new(119.9, 30.1, 0.001, 0.001, 200, 200).unwrap();
    assert!(matches!(
        d.grid_with_progress(0, FieldId::Reflectivity, spec, BeamModel::default(), |p| p < 0.1),
        Err(RadarError::Cancelled)
    ));
    assert!(matches!(
        d.grid(999, FieldId::Reflectivity, spec, BeamModel::default()),
        Err(RadarError::MissingField { .. })
    ));
}

#[test]
fn geographic_cr_section_mosaic_and_dealias_are_callable() {
    let d = support::volume();
    let beam = BeamModel::default();
    let spec = GridSpec::new(119.98, 30.03, 0.001, 0.001, 40, 60).unwrap();
    let g = d.grid(0, FieldId::Reflectivity, spec, beam).unwrap();
    let cr = d.product(ProductRequest { product: Product::CompositeReflectivity, grid: spec, beam }).unwrap();
    assert_eq!(g.values(), cr.values());
    assert_eq!(g.states(), cr.states());
    let merged = calc::mosaic(&[&g, &g]).unwrap();
    assert_eq!(merged.values(), g.values());
    let dealiased = d.dealiased_grid(0, spec, beam, Default::default()).unwrap();
    assert_eq!(dealiased.field(), FieldId::Velocity);
    let start = projection::destination(support::site().position, 0.0, 2000.0).unwrap();
    let end = projection::destination(support::site().position, 0.0, 4000.0).unwrap();
    let section = calc::section(&d, FieldId::Reflectivity, &[start, end], 500.0, &[100.0, 500.0], 500.0, beam).unwrap();
    assert!((section.distances_m.last().unwrap() - 2000.0).abs() < 0.1);
    assert_eq!(section.points.first(), Some(&start));
    assert!(section.states.iter().any(|s| *s == CellState::Valid));
}

#[test]
fn correction_cancellation_is_checked_before_work() {
    let d = support::volume();
    let spec = GridSpec::new(119.98, 30.03, 0.001, 0.001, 40, 60).unwrap();
    let beam = BeamModel::default();
    assert!(matches!(
        d.dealiased_grid_with_progress(0, spec, beam, Default::default(), |_| false),
        Err(RadarError::Cancelled)
    ));
    assert!(matches!(
        d.hydrometeor_grid_with_progress(0, spec, beam, Default::default(), |_| false),
        Err(RadarError::Cancelled)
    ));
    assert!(
        calc::section(&d, FieldId::Reflectivity, &[support::site().position; 2], 1000.0, &[0.0, 100.0], 1000.0, beam)
            .is_err()
    );
}

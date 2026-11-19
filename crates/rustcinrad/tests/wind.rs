//! Analytic multi-Doppler regression and georeferenced component output.
#![allow(clippy::unwrap_used)]
use ndarray::Array2;
use rustcinrad::{retrieve::*, *};

fn input(direction: [f64; 3], state: CellState, height: f32, time: i64) -> DopplerGrid {
    let spec = GridSpec::new(120.0, 35.0, 0.01, 0.01, 1, 1).unwrap();
    let radial = (direction[0] * 12.0 + direction[1] * (-7.0) + direction[2] * 2.0) as f32;
    DopplerGrid {
        velocity: GeoGrid::new(
            spec,
            FieldId::Velocity,
            Array2::from_elem((1, 1), radial),
            Array2::from_elem((1, 1), state),
            Array2::from_elem((1, 1), height),
            time,
        )
        .unwrap(),
        look_enu: Array2::from_elem((1, 1), direction),
    }
}

#[test]
fn three_components_recover_analytic_wind_and_export_as_geographic_rasters() {
    let a = input([1.0, 0.0, 0.0], CellState::Valid, 1000.0, 0);
    let b = input([0.0, 1.0, 0.0], CellState::Valid, 1000.0, 0);
    let c = input([0.0, 0.0, 1.0], CellState::Valid, 1000.0, 0);
    let wind = wind_grid_with_progress(&[&a, &b, &c], 0, 0.0, WindOptions::default(), |_| true).unwrap();
    assert_eq!(wind.eastward.field(), FieldId::EastwardWind);
    assert_eq!(wind.northward.values()[(0, 0)], -7.0);
    assert_eq!(wind.upward.values()[(0, 0)], 2.0);
    assert_eq!(wind.upward.states()[(0, 0)], CellState::Valid);
    assert!(wind.rmse_mps[(0, 0)] < 1e-12);
    let dir = temp_dir::TempDir::new().unwrap();
    rustcinrad::export::write_geotiff(&wind.upward, dir.path().join("w.tif")).unwrap();
    let file = std::fs::File::open(dir.path().join("w.tif")).unwrap();
    let mut decoder = tiff::decoder::Decoder::new(file).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (1, 1));
    assert!(matches!(decoder.read_image().unwrap(),tiff::decoder::DecodingResult::F32(v) if v == [2.0]));
    let volume =
        wind_volume_with_progress(&[1000.0], &[vec![&a, &b, &c]], 0, 0.0, WindOptions::default(), |_| true).unwrap();
    assert_eq!(volume.layers.len(), 1);
    assert!(
        wind_volume_with_progress(&[2000.0], &[vec![&a, &b, &c]], 0, 0.0, WindOptions::default(), |_| true).is_err()
    );
}

#[test]
fn unobservable_vertical_wind_missing_data_and_height_time_skew_are_not_fabricated() {
    let a = input([1.0, 0.0, 0.0], CellState::Valid, 1000.0, 0);
    let b = input([0.0, 1.0, 0.0], CellState::Valid, 1000.0, 0);
    let c = input([-1.0, 0.0, 0.0], CellState::Valid, 1000.0, 0);
    let wind = wind_grid_with_progress(&[&a, &b, &c], 0, 0.0, WindOptions::default(), |_| true).unwrap();
    assert_eq!(wind.upward.states()[(0, 0)], CellState::Rejected);
    let missing = input([0.0, 0.0, 1.0], CellState::Missing, 1000.0, 0);
    let wind = wind_grid_with_progress(&[&a, &b, &missing], 0, 0.0, WindOptions::default(), |_| true).unwrap();
    assert_eq!(wind.eastward.states()[(0, 0)], CellState::Rejected);
    let skew = input([0.0, 0.0, 1.0], CellState::Valid, 1100.0, 10);
    assert!(wind_grid_with_progress(&[&a, &b, &skew], 0, 100.0, WindOptions::default(), |_| true).is_err());
    let wind = wind_grid_with_progress(&[&a, &b, &skew], 10, 1.0, WindOptions::default(), |_| true).unwrap();
    assert_eq!(wind.eastward.states()[(0, 0)], CellState::Rejected);
    assert!(matches!(
        wind_grid_with_progress(&[&a, &b, &c], 0, 0.0, WindOptions::default(), |_| false),
        Err(RadarError::Cancelled)
    ));
    let bad = input([2.0, 0.0, 0.0], CellState::Valid, 1000.0, 0);
    assert!(wind_grid_with_progress(&[&a, &b, &bad], 0, 0.0, WindOptions::default(), |_| true).is_err());
}

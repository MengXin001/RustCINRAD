//! Integration regression tests.
#![allow(clippy::unwrap_used)]
use rustcinrad::{
    projection::{self, BeamModel},
    *,
};

#[test]
fn independent_wgs84_controls() {
    let zero = GeoPoint::new(0.0, 0.0).unwrap();
    let (equator, bearing) = projection::distance_bearing(zero, GeoPoint::new(1.0, 0.0).unwrap());
    assert!((equator - 111319.49079327357).abs() < 1e-6);
    assert!((bearing - 90.0).abs() < 1e-8);
    let (meridian, bearing) = projection::distance_bearing(zero, GeoPoint::new(0.0, 1.0).unwrap());
    assert!((meridian - 110574.38855779878).abs() < 1e-6);
    assert!(bearing.abs() < 1e-8);
}

#[test]
fn beam_height_long_range_and_high_elevation() {
    let (distance, height) = BeamModel::default().locate(100_000.0, 0.0, 100.0).unwrap();
    let r = 6_371_008.8 * 4.0 / 3.0;
    assert!((height - ((r * r + 1e10_f64).sqrt() - r + 100.0)).abs() < 1e-8);
    assert!(distance < 100_000.0);
    let (d, h) = BeamModel::default().locate(100_000.0, 80.0, 100.0).unwrap();
    assert!(d < 18000.0 && h > 98000.0);
    assert!(BeamModel::new(f64::NAN).is_err());
}

#[test]
fn dateline_axes_and_auto_resolution() {
    let p = GeoPoint::new(179.9, 30.0).unwrap();
    let spec = projection::auto_grid(p, 50000.0, 1000.0).unwrap();
    assert!(spec.west() + spec.longitude_step() * spec.width() as f64 > 180.0);
    let crossed = projection::destination(p, 90.0, 30000.0).unwrap();
    assert!(crossed.longitude() < 0.0);
    assert!(spec.pixel(crossed).is_some());
    let a = spec.point(spec.height() / 2, spec.width() / 2).unwrap();
    let b = spec.point(spec.height() / 2, spec.width() / 2 + 1).unwrap();
    assert!((projection::distance_bearing(a, b).0 - 1000.0).abs() < 10.0);
}
proptest::proptest! {
    #[test]
    fn geodesic_roundtrip(lon in -179.0_f64..179.0,lat in -80.0_f64..80.0,bearing in 0.0_f64..360.0,d in 1.0_f64..1_000_000.0) {
        let a=GeoPoint::new(lon,lat).unwrap();let b=projection::destination(a,bearing,d).unwrap();
        let (actual,az)=projection::distance_bearing(a,b);
        proptest::prop_assert!((actual-d).abs()<1e-5);
        proptest::prop_assert!(((az-bearing+180.0).rem_euclid(360.0)-180.0).abs()<1e-6);
    }
}

#[test]
fn wide_dateline_grid_has_roundtrip_pixels_and_bounded_beam_factor() {
    let spec = GridSpec::new(-170.0, 40.0, 10.0, 1.0, 34, 3).unwrap();
    for row in 0..spec.height() {
        for col in 0..spec.width() {
            assert_eq!(spec.pixel(spec.point(row, col).unwrap()), Some((row, col)));
        }
    }
    assert!(BeamModel::new(f64::MAX).is_err());
    assert!(BeamModel::new(0.0001).is_err());
    assert!(BeamModel::default().locate(f64::MAX, 45.0, f64::MAX).is_err());
    let spec = projection::auto_grid(GeoPoint::new(-179.9, 30.0).unwrap(), 50000.0, 1000.0).unwrap();
    assert!(spec.pixel(GeoPoint::new(179.9, 30.0).unwrap()).is_some());
}

#[test]
fn unrepresentable_grid_spacing_returns_errors_without_integer_overflow() {
    let site = GeoPoint::new(120.0, 35.0).unwrap();
    let spec = GridSpec::new(120.0, 35.0, 0.01, 0.01, 2, 2).unwrap();
    for resolution in [f64::MIN_POSITIVE, 1e-15, f64::MAX] {
        assert!(projection::auto_grid(site, 1000.0, resolution).is_err());
        assert!(projection::resample_grid(spec, resolution).is_err());
    }
}

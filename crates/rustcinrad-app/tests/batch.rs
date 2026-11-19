#![allow(clippy::unwrap_used)]
#[path = "../../rustcinrad/tests/support/mod.rs"]
mod support;

use rustcinrad::RadarError;
use rustcinrad_app::batch;

#[test]
fn a_failed_file_does_not_prevent_later_files_and_cancellation_aborts() {
    let dir = temp_dir::TempDir::new().unwrap();
    let good = dir.path().join("good.bin");
    std::fs::write(&good, support::sab(0.0, 1, true)).unwrap();
    let files = [dir.path().join("missing.bin"), good];
    let mut calls = 0;
    let results = batch(
        &files,
        &support::options(),
        |d| {
            calls += 1;
            Ok(d.summary().sweeps.len())
        },
        |_, _| true,
    )
    .unwrap();
    assert!(results[0].result.is_err());
    assert_eq!(*results[1].result.as_ref().unwrap(), 1);
    assert_eq!(calls, 1);
    let error = batch(&files, &support::options(), |_| Ok(()), |_, _| false).unwrap_err();
    assert!(matches!(error.downcast_ref::<RadarError>(), Some(RadarError::Cancelled)));
}

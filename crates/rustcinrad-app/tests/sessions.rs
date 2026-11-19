//! Integration regression tests.
#![allow(clippy::unwrap_used)]
#[path = "../../rustcinrad/tests/support/mod.rs"]
mod support;

use rustcinrad::{projection::BeamModel, *};
use rustcinrad_app::*;
use std::{sync::Arc, time::Duration};

fn operation(width: usize, height: usize) -> Operation {
    Operation::Grid {
        sweep: 0,
        field: FieldId::Reflectivity,
        spec: GridSpec::new(119.99, 30.01, 0.00001, 0.00001, width, height).unwrap(),
        beam: BeamModel::default(),
    }
}

fn finish(job: &mut Job) -> JobEvent {
    loop {
        let e = job.events.recv_timeout(Duration::from_secs(30)).unwrap();
        if !matches!(e, JobEvent::Progress { .. }) {
            job.join().unwrap();
            return e;
        }
    }
}

#[test]
fn cache_budget_hit_and_session_invalidation() {
    let app = Application::new(10000);
    let d = Arc::new(support::volume());
    let session = app.set_dataset(Arc::clone(&d)).unwrap();
    let mut a = app.submit(operation(10, 10)).unwrap();
    let JobEvent::Completed { grid: first, .. } = finish(&mut a) else {
        panic!("expected complete");
    };
    let mut b = app.submit(operation(10, 10)).unwrap();
    let JobEvent::Completed { grid: second, .. } = finish(&mut b) else {
        panic!("expected cached complete");
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert!(app.cached_bytes().unwrap() <= 10000);
    let mut oversized = app.submit(operation(40, 40)).unwrap();
    assert!(matches!(finish(&mut oversized), JobEvent::Completed { .. }));
    assert!(app.cached_bytes().unwrap() <= 10000);
    app.set_dataset(d).unwrap();
    assert!(!app.is_current(session));
    assert_eq!(app.cached_bytes().unwrap(), 0);
}

#[test]
fn cancellation_and_stale_results() {
    let app = Application::new(0);
    app.set_dataset(Arc::new(support::volume())).unwrap();
    let mut a = app.submit(operation(500, 500)).unwrap();
    a.cancel();
    assert!(matches!(finish(&mut a), JobEvent::Cancelled(_)));
    let mut b = app.submit(operation(500, 500)).unwrap();
    app.set_dataset(Arc::new(support::volume())).unwrap();
    assert!(matches!(finish(&mut b), JobEvent::Stale(_)));
    assert_eq!(app.cached_bytes().unwrap(), 0);
}

#[test]
fn failed_open_leaves_old_session_intact() {
    let app = Application::new(0);
    let id = app.set_dataset(Arc::new(support::volume())).unwrap();
    assert!(app.open("nonexistent.radar", &ReadOptions::default()).is_err());
    assert!(app.is_current(id));
}

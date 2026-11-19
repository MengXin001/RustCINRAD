//! GUI-independent sessions, bounded grid cache and cancellable background jobs.
//! Event delivery carries session identity: obsolete results cannot replace a new file.
mod batch;

use anyhow::{Context, Result};
pub use batch::{FileOutcome, batch};
use rustcinrad::{projection::BeamModel, *};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};
use tracing::{debug, debug_span};

/// File-session identity, monotonically increasing within one application instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionId(pub u64);
/// Background operation identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobId(pub u64);
/// Supported background work. Parameters are part of cache identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operation {
    /// Geographic layer extraction.
    Grid {
        /// Layer index.
        sweep: usize,
        /// Moment.
        field: FieldId,
        /// Output grid.
        spec: GridSpec,
        /// Geometry.
        beam: BeamModel,
    },
    /// Radial-domain velocity correction and geographic output.
    Dealias {
        /// Scan index.
        sweep: usize,
        /// Output grid.
        spec: GridSpec,
        /// Beam geometry.
        beam: BeamModel,
        /// Correction parameters, included in the cache identity.
        options: rustcinrad::correct::DealiasOptions,
    },
    /// Four-moment hydrometeor classification.
    Hydrometeors {
        /// Scan index.
        sweep: usize,
        /// Output grid.
        spec: GridSpec,
        /// Beam geometry.
        beam: BeamModel,
        /// Classification parameters, included in the cache identity.
        options: rustcinrad::correct::HcaOptions,
    },
    /// Dual-polarization gate-domain QC and attenuation correction.
    Polarimetric {
        /// Scan index.
        sweep: usize,
        /// Corrected moment.
        field: FieldId,
        /// Geographic grid.
        spec: GridSpec,
        /// Beam geometry.
        beam: BeamModel,
        /// QC and correction parameters.
        options: rustcinrad::correct::PolarimetricOptions,
    },
    /// Reflectivity-only precipitation rate.
    Qpe {
        /// Scan index.
        sweep: usize,
        /// Geographic grid.
        spec: GridSpec,
        /// Beam geometry.
        beam: BeamModel,
        /// Z-R relation.
        options: rustcinrad::retrieve::QpeOptions,
    },
    /// Scientific product.
    Product(rustcinrad::calc::ProductRequest),
}

impl Operation {
    /// Execute reusable application work without installing an executor or logger.
    /// Returns core decoding/algorithm/cancellation failures unchanged.
    pub fn execute_with_progress(
        self,
        dataset: &RadarDataset,
        progress: impl FnMut(f32) -> bool,
    ) -> rustcinrad::Result<GeoGrid> {
        match self {
            Self::Grid { sweep, field, spec, beam } => dataset.grid_with_progress(sweep, field, spec, beam, progress),
            Self::Dealias { sweep, spec, beam, options } => {
                dataset.dealiased_grid_with_progress(sweep, spec, beam, options, progress)
            }
            Self::Hydrometeors { sweep, spec, beam, options } => {
                dataset.hydrometeor_grid_with_progress(sweep, spec, beam, options, progress)
            }
            Self::Polarimetric { sweep, field, spec, beam, options } => {
                dataset.polarimetric_grid_with_progress(sweep, field, spec, beam, options, progress)
            }
            Self::Qpe { sweep, spec, beam, options } => {
                dataset.rain_rate_grid_with_progress(sweep, spec, beam, options, progress)
            }
            Self::Product(request) => dataset.product_with_progress(request, progress),
        }
    }
}

/// Typed background job events for CLI or a future GUI event loop.
#[derive(Debug)]
pub enum JobEvent {
    /// Fraction in `0..=1`.
    Progress {
        /// Job identity.
        job: JobId,
        /// File identity.
        session: SessionId,
        /// Fraction completed.
        fraction: f32,
    },
    /// Accepted result; stale sessions produce `Stale` instead.
    Completed {
        /// Job identity.
        job: JobId,
        /// File identity.
        session: SessionId,
        /// Shared immutable raster.
        grid: Arc<GeoGrid>,
    },
    /// Cooperative cancellation observed.
    Cancelled(JobId),
    /// Old session result deliberately discarded.
    Stale(JobId),
    /// Recoverable computation failure.
    Failed {
        /// Job identity.
        job: JobId,
        /// Error text.
        message: String,
    },
}

/// Receives events and controls cancellation. Dropping requests cancellation without blocking.
pub struct Job {
    /// Unique job identifier.
    pub id: JobId,
    /// Event stream (single-consumer).
    pub events: mpsc::Receiver<JobEvent>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Job {
    /// Request cancellation without blocking.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Wait for worker termination, reporting a worker panic.
    pub fn join(&mut self) -> Result<()> {
        if let Some(w) = self.worker.take() {
            w.join().map_err(|_| anyhow::anyhow!("radar worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        // Detach: the worker retains only owned/Arc state and rejects obsolete sessions.
        self.worker.take();
    }
}

struct Entry {
    session: SessionId,
    operation: Operation,
    grid: Arc<GeoGrid>,
}

struct State {
    session: SessionId,
    dataset: Option<Arc<RadarDataset>>,
    cache: VecDeque<Entry>,
    bytes: usize,
    budget: usize,
}

/// Shared immutable file sessions and LRU cache with a byte budget.
pub struct Application {
    state: Arc<Mutex<State>>,
    next_job: AtomicU64,
}

impl Application {
    /// Create an application with an explicit retained-raster cache budget.
    pub fn new(cache_budget_bytes: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                session: SessionId(0),
                dataset: None,
                cache: VecDeque::new(),
                bytes: 0,
                budget: cache_budget_bytes,
            })),
            next_job: AtomicU64::new(1),
        }
    }

    /// Decode a local file and replace the session only after successful decoding.
    /// Errors leave the old session intact. Call from a loader worker in GUI.
    pub fn open(&self, path: impl AsRef<Path>, options: &ReadOptions) -> Result<SessionId> {
        let dataset =
            read_path(path.as_ref(), options).with_context(|| format!("opening {}", path.as_ref().display()))?;
        self.set_dataset(Arc::new(dataset))
    }

    /// Install an already decoded immutable dataset; invalidates previous cache/jobs.
    pub fn set_dataset(&self, dataset: Arc<RadarDataset>) -> Result<SessionId> {
        let mut s = self.state.lock().map_err(|_| anyhow::anyhow!("session lock poisoned"))?;
        s.session = SessionId(s.session.0.checked_add(1).context("session id exhausted")?);
        s.dataset = Some(dataset);
        s.cache.clear();
        s.bytes = 0;
        debug!(session = s.session.0, "radar session opened");
        Ok(s.session)
    }

    /// Current dataset and session. Errors if no file is open.
    pub fn dataset(&self) -> Result<(SessionId, Arc<RadarDataset>)> {
        let s = self.state.lock().map_err(|_| anyhow::anyhow!("session lock poisoned"))?;
        Ok((s.session, Arc::clone(s.dataset.as_ref().context("no radar file open")?)))
    }

    /// Bytes retained by the cache (external `Arc` owners are not charged).
    pub fn cached_bytes(&self) -> Result<usize> {
        Ok(self.state.lock().map_err(|_| anyhow::anyhow!("cache lock poisoned"))?.bytes)
    }

    /// Check an event's session again at consumption time, closing delivery races.
    pub fn is_current(&self, session: SessionId) -> bool {
        self.state.lock().is_ok_and(|s| s.session == session)
    }

    /// Start a background operation. Cache identity includes session and every parameter.
    /// Errors on absent files, lock poisoning or thread creation failure.
    pub fn submit(&self, operation: Operation) -> Result<Job> {
        let (session, dataset) = self.dataset()?;
        let id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let state = Arc::clone(&self.state);
        let (sender, events) = mpsc::channel();
        let span = debug_span!("radar_job", job = id.0, session = session.0, operation = ?operation);
        let worker = thread::Builder::new()
            .name(format!("radar-job-{}", id.0))
            .spawn(move || {
                let _span = span.entered();
                let started = Instant::now();
                debug!("job started");
                let cached = state.lock().ok().and_then(|mut s| {
                    let i = s.cache.iter().position(|e| e.session == session && e.operation == operation)?;
                    let e = s.cache.remove(i)?;
                    let grid = Arc::clone(&e.grid);
                    s.cache.push_back(e);
                    Some(grid)
                });
                let result = if let Some(grid) = cached {
                    debug!(bytes = grid.byte_size(), "cache hit");
                    Ok(grid)
                } else {
                    debug!("cache miss");
                    let mut last = -1.0_f32;
                    let mut progress = |fraction: f32| {
                        if flag.load(Ordering::Acquire) {
                            return false;
                        }
                        if !state.lock().is_ok_and(|s| s.session == session) {
                            return false;
                        }
                        if fraction - last >= 0.01 || fraction == 1.0 {
                            last = fraction;
                            let _ = sender.send(JobEvent::Progress { job: id, session, fraction });
                        }
                        true
                    };
                    operation.execute_with_progress(&dataset, &mut progress).map(Arc::new)
                };
                let event = match state.lock() {
                    Err(_) => JobEvent::Failed { job: id, message: "session lock poisoned".into() },
                    Ok(s) if s.session != session => JobEvent::Stale(id),
                    Ok(_) if flag.load(Ordering::Acquire) => JobEvent::Cancelled(id),
                    Ok(mut s) => match result {
                        Ok(grid) => {
                            let size = grid.byte_size();
                            if size <= s.budget
                                && !s.cache.iter().any(|e| e.session == session && e.operation == operation)
                            {
                                while s.bytes + size > s.budget {
                                    if let Some(old) = s.cache.pop_front() {
                                        let bytes = old.grid.byte_size();
                                        s.bytes -= bytes;
                                        debug!(bytes, "cache entry evicted");
                                    } else {
                                        break;
                                    }
                                }
                                s.bytes += size;
                                s.cache.push_back(Entry { session, operation, grid: Arc::clone(&grid) });
                            }
                            JobEvent::Completed { job: id, session, grid }
                        }
                        Err(RadarError::Cancelled) => JobEvent::Cancelled(id),
                        Err(e) => JobEvent::Failed { job: id, message: e.to_string() },
                    },
                };
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                match &event {
                    JobEvent::Completed { grid, .. } => debug!(elapsed_ms, bytes = grid.byte_size(), "job completed"),
                    JobEvent::Failed { message, .. } => debug!(elapsed_ms, error = %message, "job failed"),
                    JobEvent::Cancelled(_) => debug!(elapsed_ms, "job cancelled"),
                    JobEvent::Stale(_) => debug!(elapsed_ms, "stale job discarded"),
                    JobEvent::Progress { .. } => {}
                }
                let _ = sender.send(event);
            })
            .context("starting radar worker")?;
        Ok(Job { id, events, cancel, worker: Some(worker) })
    }
}

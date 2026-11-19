//! Local batch execution, independent of the interactive file session.
use anyhow::Result;
use rustcinrad::{RadarDataset, RadarError::Cancelled, ReadOptions, read_path};
use std::path::PathBuf;
use tracing::{debug, debug_span};

/// One file outcome; a failure does not discard other successful files.
#[derive(Debug)]
pub struct FileOutcome<T> {
    /// Input path.
    pub path: PathBuf,
    /// Decoder or processing result.
    pub result: Result<T>,
}

/// Process local files sequentially with bounded per-file decoded memory.
/// `progress(completed,total)` returning false cancels before the next file.
/// Decoder/processor errors are retained individually; only cancellation aborts the batch.
/// The processor can invoke cancellable core operations from a future GUI worker.
pub fn batch<T>(
    paths: &[PathBuf],
    options: &ReadOptions,
    mut process: impl FnMut(&RadarDataset) -> Result<T>,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Vec<FileOutcome<T>>> {
    let mut outcomes = Vec::with_capacity(paths.len());
    for path in paths {
        let _span = debug_span!("batch_file", path = %path.display(), index = outcomes.len()).entered();
        if !progress(outcomes.len(), paths.len()) {
            return Err(Cancelled.into());
        }
        let result = read_path(path, options).map_err(anyhow::Error::from).and_then(|dataset| process(&dataset));
        debug!(success = result.is_ok(), "batch file completed");
        outcomes.push(FileOutcome { path: path.clone(), result });
    }
    if !progress(outcomes.len(), paths.len()) {
        return Err(Cancelled.into());
    }
    Ok(outcomes)
}

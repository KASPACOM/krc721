use cliclack::ProgressBar;
use portable_atomic::AtomicF64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Progress {
    progress: Arc<AtomicF64>,
    last_progress: Arc<AtomicU64>,
    callback: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    progress_bar: Option<ProgressBar>,
}

impl Progress {
    // pub fn new<F>() -> Self
    // {
    //     Self {
    //         progress: Arc::new(AtomicF64::new(0.0)),
    //         last_progress: Arc::new(AtomicU64::new(0)),
    //         callback: None,
    //         progress_bar: None,
    //     }
    // }

    pub fn with_callback(self, callback: Arc<dyn Fn(f64) + Send + Sync>) -> Self {
        Self {
            callback: Some(callback),
            ..self
        }
    }

    pub fn with_progress_bar(self, progress_bar: ProgressBar) -> Self {
        Self {
            progress_bar: Some(progress_bar),
            ..self
        }
    }

    pub fn update(&self, progress: f64) {
        let progress = (progress * 100.0).min(100.0);
        self.progress.store(progress, Ordering::Relaxed);
        if let Some(progress_bar) = self.progress_bar.as_ref() {
            let p = progress as u64;
            let last = self.last_progress.load(Ordering::Relaxed);
            if p > last {
                progress_bar.inc(p - last);
                self.last_progress.store(p, Ordering::Relaxed);
            }
        }
        if let Some(callback) = self.callback.as_ref() {
            (callback)(progress);
        }
    }

    pub fn message(&self, message: &str) {
        if let Some(progress_bar) = self.progress_bar.as_ref() {
            progress_bar.set_message(message);
        }
    }

    pub fn progress(&self) -> f64 {
        self.progress.load(Ordering::Relaxed)
    }

    // pub fn finish(&self) {
    //     if let Some(progress_bar) = self.progress_bar.as_ref() {
    //         progress_bar.stop("Done");
    //     }
    // }
}

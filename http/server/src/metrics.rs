use crate::imports::*;
pub use crate::metrics::latency::Metrics as LatencyMetrics;
pub use crate::metrics::latency::Snapshot as LatencySnapshot;
use tower::Layer;
use tower::Service;
pub mod latency;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub requests_per_second: Arc<AtomicU64>,
    pub latency: Arc<LatencyMetrics>,
}

impl Metrics {
    pub fn new(requests_per_second: Arc<AtomicU64>, latency: Arc<LatencyMetrics>) -> Self {
        Self {
            requests_per_second,
            latency,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Snapshot {
    pub requests_per_second: u64,
    pub latency: LatencySnapshot,
}

impl From<&Metrics> for Snapshot {
    fn from(value: &Metrics) -> Self {
        Self {
            requests_per_second: value.requests_per_second.load(Ordering::Relaxed),
            latency: LatencySnapshot::from(value.latency.as_ref()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestCounterLayer {
    counter: Arc<AtomicU64>,
}

impl RequestCounterLayer {
    pub fn new(counter: Arc<AtomicU64>) -> Self {
        Self { counter }
    }
}

impl<S> Layer<S> for RequestCounterLayer {
    type Service = RequestCounter<S>;

    fn layer(&self, service: S) -> Self::Service {
        RequestCounter {
            inner: service,
            counter: self.counter.clone(),
        }
    }
}
#[derive(Clone)]
pub struct RequestCounter<S> {
    inner: S,
    counter: Arc<AtomicU64>,
}

impl<S, Request> Service<Request> for RequestCounter<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.counter.fetch_add(1, Ordering::Relaxed);
        self.inner.call(request)
    }
}

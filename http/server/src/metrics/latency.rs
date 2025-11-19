use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use futures_util::future::BoxFuture;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use strum::EnumCount;
use tower::{Layer, Service};
use tracing::log::warn;

#[derive(Debug, EnumCount)]
#[repr(u8)]
pub enum Endpoint {
    Root,
    Metrics,
    Docs,    // Added
    Sandbox, // Added
    KRC721CollectionList,
    KRC721CollectionLookup,
    KRC721TokenLookup,
    KRC721TokenList,
    KRC721TokenHistory,
    KRC721AddressNftList,
    KRC721AddressNftLookup,
    KRC721OpList,
    KRC721OpByScore,
    KRC721OpByTxid,
    KRC721DeploymentList,
    KRC721RoyaltyFee,
    KRC721RejectionByTxid,
    KRC721ReservedTokens,
    KRC721IndexerStatus,
    KRC721TokenIdRanges,
}

impl Endpoint {
    fn from_path(path: &str) -> Option<Self> {
        let network_pattern = "/api/v1/krc721/";

        match path {
            "/" => Some(Self::Root),
            "/api/metrics" => Some(Self::Metrics),
            "/docs" => Some(Self::Docs),
            "/sandbox" => Some(Self::Sandbox),
            // Handle network-specific paths
            path if path.starts_with(network_pattern) => {
                let route = &path[network_pattern.len()..];

                // Extract network-relative path
                let path_without_network = route.split('/').skip(1).collect::<Vec<_>>().join("/");

                match path_without_network.as_str() {
                    "nfts" => Some(Self::KRC721CollectionList),

                    p if p.starts_with("nfts/") && p.matches('/').count() == 1 => {
                        Some(Self::KRC721CollectionLookup)
                    }

                    p if p.starts_with("nfts/") && p.matches('/').count() == 2 => {
                        Some(Self::KRC721TokenLookup)
                    }

                    p if p.starts_with("owners/") => Some(Self::KRC721TokenList),

                    p if p.starts_with("history/") => Some(Self::KRC721TokenHistory),

                    p if p.starts_with("address/") && p.matches('/').count() == 1 => {
                        Some(Self::KRC721AddressNftList)
                    }

                    p if p.starts_with("address/") && p.matches('/').count() == 2 => {
                        Some(Self::KRC721AddressNftLookup)
                    }

                    "ops" => Some(Self::KRC721OpList),

                    p if p.starts_with("ops/score/") => Some(Self::KRC721OpByScore),

                    p if p.starts_with("ops/txid/") => Some(Self::KRC721OpByTxid),

                    "deployments" => Some(Self::KRC721DeploymentList),

                    p if p.starts_with("royalties/") => Some(Self::KRC721RoyaltyFee),

                    p if p.starts_with("rejections/txid/") => Some(Self::KRC721RejectionByTxid),

                    "reserved" => Some(Self::KRC721ReservedTokens),

                    "status" => Some(Self::KRC721IndexerStatus),

                    p if p.starts_with("ranges/") => Some(Self::KRC721TokenIdRanges),

                    _ => None,
                }
            }

            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct EndpointMetrics {
    min_nanos: AtomicU64,
    max_nanos: AtomicU64,
    sum_ms: AtomicU64,
    count: AtomicU64,
}

impl EndpointMetrics {
    fn new() -> Self {
        Self {
            min_nanos: AtomicU64::new(u64::MAX),
            max_nanos: AtomicU64::new(0),
            sum_ms: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn record(&self, duration: Duration) {
        let nanos = duration.as_nanos() as u64;

        self.min_nanos.fetch_min(nanos, Ordering::Relaxed);
        self.max_nanos.fetch_max(nanos, Ordering::Relaxed);
        self.sum_ms.fetch_add(nanos, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn stats(&self) -> LatencyStats {
        let count = self.count.load(Ordering::Relaxed);
        LatencyStats {
            min_nanos: self.min_nanos.load(Ordering::Relaxed),
            max_nanos: self.max_nanos.load(Ordering::Relaxed),
            avg_nanos: if count > 0 {
                self.sum_ms.load(Ordering::Relaxed) / count
            } else {
                0
            },
            count,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LatencyStats {
    pub min_nanos: u64,
    pub max_nanos: u64,
    pub avg_nanos: u64,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub struct Metrics {
    metrics: Arc<[EndpointMetrics; Endpoint::COUNT]>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(std::array::from_fn(|_| EndpointMetrics::new())),
        }
    }

    pub fn record(&self, endpoint: Endpoint, duration: Duration) {
        self.metrics[endpoint as usize].record(duration);
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Snapshot {
    pub root: LatencyStats,
    pub metrics: LatencyStats,
    pub sandbox: LatencyStats,
    pub docs: LatencyStats,
    pub krc721_collection_list: LatencyStats,
    pub krc721_collection_lookup: LatencyStats,
    pub krc721_token_lookup: LatencyStats,
    pub krc721_token_list: LatencyStats,
    pub krc721_token_history: LatencyStats,
    pub krc721_address_nft_list: LatencyStats,
    pub krc721_address_nft_lookup: LatencyStats,
    pub krc721_op_list: LatencyStats,
    pub krc721_op_by_score: LatencyStats,
    pub krc721_op_by_txid: LatencyStats,
    pub krc721_deployment_list: LatencyStats,
    pub krc721_royalty_fee: LatencyStats,
    pub krc721_rejection_by_txid: LatencyStats,
    pub krc721_reserved_tokens: LatencyStats,
    pub krc721_indexer_status: LatencyStats,
    pub krc721_token_id_ranges: LatencyStats,
}

impl From<&Metrics> for Snapshot {
    fn from(value: &Metrics) -> Self {
        Self {
            root: value.metrics[Endpoint::Root as usize].stats(),
            metrics: value.metrics[Endpoint::Metrics as usize].stats(),
            krc721_collection_list: value.metrics[Endpoint::KRC721CollectionList as usize].stats(),
            krc721_collection_lookup: value.metrics[Endpoint::KRC721CollectionLookup as usize]
                .stats(),
            krc721_token_lookup: value.metrics[Endpoint::KRC721TokenLookup as usize].stats(),
            krc721_token_list: value.metrics[Endpoint::KRC721TokenList as usize].stats(),
            krc721_token_history: value.metrics[Endpoint::KRC721TokenHistory as usize].stats(),
            krc721_address_nft_list: value.metrics[Endpoint::KRC721AddressNftList as usize].stats(),
            krc721_address_nft_lookup: value.metrics[Endpoint::KRC721AddressNftLookup as usize]
                .stats(),
            krc721_op_list: value.metrics[Endpoint::KRC721OpList as usize].stats(),
            krc721_op_by_score: value.metrics[Endpoint::KRC721OpByScore as usize].stats(),
            krc721_op_by_txid: value.metrics[Endpoint::KRC721OpByTxid as usize].stats(),
            krc721_deployment_list: value.metrics[Endpoint::KRC721DeploymentList as usize].stats(),
            krc721_royalty_fee: value.metrics[Endpoint::KRC721RoyaltyFee as usize].stats(),
            krc721_rejection_by_txid: value.metrics[Endpoint::KRC721RejectionByTxid as usize]
                .stats(),
            krc721_reserved_tokens: value.metrics[Endpoint::KRC721ReservedTokens as usize].stats(),
            krc721_indexer_status: value.metrics[Endpoint::KRC721IndexerStatus as usize].stats(),
            krc721_token_id_ranges: value.metrics[Endpoint::KRC721TokenIdRanges as usize].stats(),
            docs: value.metrics[Endpoint::Docs as usize].stats(),
            sandbox: value.metrics[Endpoint::Sandbox as usize].stats(),
        }
    }
}

#[derive(Clone)]
pub struct MetricsLayer {
    data: Arc<Metrics>,
}

impl MetricsLayer {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self { data: metrics }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        MetricsMiddleware {
            inner: service,
            metrics: self.data.clone(),
        }
    }
}

#[derive(Clone)]
pub struct MetricsMiddleware<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

impl<S> Service<Request<Body>> for MetricsMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let metrics = self.metrics.clone();
        if let Some(endpoint) = Endpoint::from_path(request.uri().path()) {
            let start = std::time::Instant::now();
            let future = self.inner.call(request);

            Box::pin(async move {
                let response = future.await;
                metrics.record(endpoint, start.elapsed());
                response
            })
        } else {
            warn!("Unknown endpoint: {:?}", request.uri().path());
            Box::pin(self.inner.call(request))
        }
    }
}

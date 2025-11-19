use crate::imports::*;

use crate::filter::IpFilterLayer;
use crate::limits::RateLimit;
use crate::metrics::latency::MetricsLayer;
use crate::metrics::*;
use axum::{
    body::Body,
    error_handling::HandleErrorLayer,
    extract::Query,
    http::{header, HeaderValue, Method, StatusCode},
    response::Html,
    response::{IntoResponse, Response},
    routing::get,
    BoxError, Json, Router,
};
use krc721_core::model::krc721::Response as CoreResponse;
use krc721_core::model::krc721::*;
use krc721_core::network::Network;
use krc721_core::runtime::{Runtime, Service as CoreService, ServiceResult};
use krc721_snapshot::prelude::Generator;
use std::net::IpAddr;
use tower::{buffer::BufferLayer, limit::RateLimitLayer, ServiceBuilder};
use tower_http::cors::{Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::*;

struct HttpServerInner {
    data: Arc<dyn DataT>,
    network: Network,
    listen_address: String,
    rate_limit: Option<RateLimit>,
    allowed_ips: Option<Vec<IpAddr>>,
    requests_per_second: Arc<AtomicU64>,
    index_html: String,
    sandbox_html: String,
    doc_html: String,
    // http_folder: PathBuf,
    generator: Option<Arc<Generator>>,
}

pub struct HttpServer {
    inner: Arc<HttpServerInner>,
}

impl HttpServer {
    pub fn new(
        // nexus: &Nexus,
        network: Network,
        data: Arc<dyn DataT>,
        listen_address: &str,
        rate_limit: Option<RateLimit>,
        allowed_ips: Option<Vec<IpAddr>>,
        generator: Option<Arc<Generator>>,
    ) -> Self {
        let index_html = include_str!("../html/index.html")
            .replace("{{network}}", network.to_string().as_str())
            .replace("{{version}}", env!("CARGO_PKG_VERSION"))
            .to_string();

        let test_html = include_str!("../html/sandbox.html")
            .replace("{{network}}", network.to_string().as_str())
            .replace("{{version}}", env!("CARGO_PKG_VERSION"))
            .to_string();

        let doc_html = generate_docs();

        Self {
            inner: Arc::new(HttpServerInner {
                network,
                data,
                listen_address: listen_address.to_string(),
                rate_limit,
                allowed_ips,
                requests_per_second: Arc::new(AtomicU64::new(0)),
                index_html,
                sandbox_html: test_html,
                doc_html,
                generator,
            }),
        }
    }

    #[inline(always)]
    pub fn data(&self) -> Arc<dyn DataT> {
        self.inner.data.clone()
    }

    pub fn get_requests_per_second(&self) -> u64 {
        self.inner.requests_per_second.load(Ordering::Relaxed)
    }

    async fn register_routes(self: &Arc<Self>, mut router: Router) -> Router {
        let network = self.inner.network;

        let this = self.clone();
        router = router.route(
            "/sandbox",
            get(|| async move { Html(this.inner.sandbox_html.clone()) }),
        );

        let this = self.clone();
        router = router.route(
            "/docs",
            get(|| async move { Html(this.inner.doc_html.clone()) }),
        );

        let data = self.data().clone();
        router =
            router.route(
                &format!("/api/v1/krc721/{network}/nfts"),
                get(|Query(query)| async move {
                    to_pagination(data.krc721_collection_list(query).await)
                }),
            );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/nfts/{{tick}}"),
            get(|UrlPath(path)| async move { to_json(data.krc721_collection_lookup(path).await) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/nfts/{{tick}}/{{id}}"),
            get(|UrlPath(path)| async move { to_json(data.krc721_token_lookup(path).await) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/owners/{{tick}}"),
            get(|UrlPath(path), Query(query)| async move {
                to_pagination(data.krc721_token_list(path, query).await)
            }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/history/{{tick}}/{{id}}"),
            get(|UrlPath(path), Query(query)| async move {
                to_pagination(data.krc721_token_history(path, query).await)
            }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/address/{{address}}"),
            get(|UrlPath(path), Query(query)| async move {
                to_pagination(data.krc721_address_nft_list(path, query).await)
            }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/address/{{address}}/{{tick}}"),
            get(|UrlPath(path), Query(query)| async move {
                to_pagination(data.krc721_address_nft_lookup(path, query).await)
            }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/ops"),
            get(|Query(query)| async move { to_pagination(data.krc721_op_list(query).await) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/ops/score/{{score}}"),
            get(|UrlPath(path)| async move { to_json(data.krc721_op_by_score(path).await) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/ops/txid/{{txid}}"),
            get(|UrlPath(path)| async move { to_json(data.krc721_op_by_txid(path).await) }),
        );

        let data = self.data().clone();
        router =
            router.route(
                &format!("/api/v1/krc721/{network}/deployments"),
                get(|Query(query)| async move {
                    to_pagination(data.krc721_deployment_list(query).await)
                }),
            );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/royalties/{{address}}/{{tick}}"),
            get(|UrlPath(path)| async move {
                to_json(
                    data.krc721_royalty_fee(path)
                        .await
                        .map(|v| v.map(|v| v.to_string())),
                )
            }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/rejections/txid/{{txid}}"),
            get(|UrlPath(path)| async move { to_json(data.krc721_rejection_by_txid(path).await) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/reserved"),
            get(|| async move { to_json(data.krc721_reserved_tokens().await.map(Some)) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/status"),
            get(|| async move { to_json(data.krc721_indexer_status().await.map(Some)) }),
        );

        let data = self.data().clone();
        router = router.route(
            &format!("/api/v1/krc721/{network}/ranges/{{tick}}"),
            get(|UrlPath(path)| async move {
                to_json(data.krc721_available_token_id_ranges(path).await)
            }),
        );

        router
    }
}

#[async_trait]
impl CoreService for HttpServer {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        let router = Router::new();

        let this = self.clone();
        let mut router = router.route(
            "/",
            get(|| async move { Html(this.inner.index_html.clone()) }),
        );

        router = self.register_routes(router).await;

        if let Some(generator) = self.inner.generator.as_ref() {
            router = generator.register_handlers(router);
        }

        // Add IP filter if configured
        if let Some(allowed_ips) = self.inner.allowed_ips.as_ref() {
            info!(
                "Enabling IP filtering with following IPs: {}",
                allowed_ips
                    .iter()
                    .map(|ip| ip.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
            router = router.layer(IpFilterLayer::new(allowed_ips.clone()));
        } else {
            warn!("IP filtering is disabled");
        }

        // Add request counter
        let counter_layer = RequestCounterLayer::new(self.inner.requests_per_second.clone());
        router = router.layer(counter_layer);

        // Create metrics instance
        let metrics = Arc::new(Metrics::new(
            self.inner.requests_per_second.clone(),
            Arc::new(LatencyMetrics::new()),
        ));

        let metrics_layer = MetricsLayer::new(metrics.latency.clone());
        let router = router
            .route(
                "/api/metrics",
                get(|| async move { metrics_handler(metrics).await }),
            )
            .layer(metrics_layer);

        let router = if let Some(rate_limit) = self.inner.rate_limit.as_ref() {
            info!(
                "Setting rate limit to: {} requests per {} seconds",
                rate_limit.requests, rate_limit.period
            );
            router.layer(
                ServiceBuilder::new()
                    // .layer(counter_layer)
                    .layer(HandleErrorLayer::new(|err: BoxError| async move {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Unhandled error: {}", err),
                        )
                    }))
                    .layer(BufferLayer::new(1024))
                    .layer(RateLimitLayer::new(
                        rate_limit.requests,
                        Duration::from_secs(rate_limit.period),
                    )),
            )
        } else {
            warn!("Rate limit is disabled");
            router
            // router.layer(counter_layer)
        };

        let router = router.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST]),
        );

        let router = router.layer((
            // Add CORS configuration
            TraceLayer::new_for_http(),
            // Graceful shutdown will wait for
            // outstanding requests to complete.
            // Add a timeout so that requests
            // don't hang forever.
            TimeoutLayer::new(Duration::from_secs(10)),
        ));

        info!(
            "HTTP server is listening on http://{}",
            self.inner.listen_address.as_str()
        );

        let listener = tokio::net::TcpListener::bind(self.inner.listen_address.as_str())
            .await
            .unwrap();

        let counter = self.inner.requests_per_second.clone();
        spawn(async move {
            if let Err(err) = axum::serve(listener, router)
                .with_graceful_shutdown(std::future::pending::<()>())
                .await
            {
                error!("HTTP server error (serve): {err}");
            }
        });

        spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                // println!("HTTP r/s: {}", counter.load(Ordering::Relaxed));
                counter.store(0, Ordering::Relaxed);
            }
        });

        Ok(())
    }

    fn terminate(self: Arc<Self>) {}

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        Ok(())
    }
}

pub fn to_pagination<T: Serialize + std::fmt::Debug, Offset: Serialize + std::fmt::Debug>(
    result: CoreResult<Pagination<T, Offset>>,
) -> impl IntoResponse {
    // let response = CoreResponse::new(result);
    // info!("to_pagination: {result:?}",);
    match result {
        Ok(value) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
            .header(
                header::CACHE_CONTROL,
                HeaderValue::from_static(
                    "no-cache, no-store, must-revalidate, proxy-revalidate, max-age=0",
                ),
            )
            .body(Body::from(
                serde_json::to_string(&CoreResponse::pagination(value)).unwrap(),
            ))
            .unwrap(),
        Err(err) => {
            error!("HTTP response to_pagination(): {err:?}");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    serde_json::to_string(&CoreResponse::<(), ()>::error(err)).unwrap(),
                ))
                .unwrap()
        }
    }
}

pub fn to_json<T: Serialize + std::fmt::Debug>(result: CoreResult<Option<T>>) -> impl IntoResponse {
    // info!("to_json: {result:?}",);
    match result {
        Ok(value) => {
            let response = CoreResponse::<T, ()>::single(value);

            // let (code, body, content_type) = if response.has_result() {
            //     (StatusCode::OK, serde_json::to_string(&response).unwrap(), mime::APPLICATION_JSON.as_ref())
            // } else {
            //     (StatusCode::NOT_FOUND, response.message, mime::TEXT_PLAIN.as_ref())
            // };

            let (code, body, content_type) = if response.has_result() {
                (
                    StatusCode::OK,
                    serde_json::to_string(&response).unwrap(),
                    mime::APPLICATION_JSON.as_ref(),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    response.message,
                    mime::TEXT_PLAIN.as_ref(),
                )
            };

            Response::builder()
                .status(code)
                .header(header::CONTENT_TYPE, content_type)
                .header(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(
                        "no-cache, no-store, must-revalidate, proxy-revalidate, max-age=0",
                    ),
                )
                .body(Body::from(body))
                .unwrap()
        }
        Err(err) => {
            error!("HTTP response to_json(): {err:?}");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                // .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.as_ref())
                .body(Body::from(
                    // serde_json::to_string(&CoreResponse::<(), ()>::error(err)).unwrap(),
                    err.to_string(),
                ))
                .unwrap()
        }
    }
}

fn generate_docs() -> String {
    use comrak::plugins::syntect::SyntectAdapterBuilder;
    use syntect::parsing::SyntaxSet;

    let syntax_set = SyntaxSet::load_defaults_newlines();
    let _ts = syntax_set.find_syntax_by_extension("ts");
    let _rs = syntax_set.find_syntax_by_extension("rs");
    // let adapter = SyntectAdapter::new(Some("InspiredGitHub"));
    // let adapter = SyntectAdapter::new(Some("base16-ocean.dark"));
    let adapter = SyntectAdapterBuilder::default()
        .theme("base16-ocean.dark")
        .syntax_set(syntax_set)
        .build();

    use comrak::{markdown_to_html_with_plugins, Options, Plugins};

    let krc721_md = include_str!("../../../doc/KRC-721.md").to_string();
    let rest_md = include_str!("../../../doc/REST.md").to_string();
    let overview_md = include_str!("../../../doc/OVERVIEW.md").to_string();
    let rust_md = include_str!("../../../doc/examples/RUST.md").to_string();
    let ts_md = include_str!("../../../doc/examples/TS.md").to_string();

    let doc_html = include_str!("../html/docs.html").to_string();

    let options = Options::default();
    let mut plugins = Plugins::default();

    plugins.render.codefence_syntax_highlighter = Some(&adapter);
    let krc721_html = markdown_to_html_with_plugins(&krc721_md, &options, &plugins);
    let rest_html = markdown_to_html_with_plugins(&rest_md, &options, &plugins);
    let overview_html = markdown_to_html_with_plugins(&overview_md, &options, &plugins);
    let rust_html = markdown_to_html_with_plugins(&rust_md, &options, &plugins);
    let ts_html = markdown_to_html_with_plugins(&ts_md, &options, &plugins);

    doc_html
        .replace("{krc721_html}", &krc721_html)
        .replace("{rest_html}", &rest_html)
        .replace("{overview_html}", &overview_html)
        .replace("{rust_html}", &rust_html)
        .replace("{ts_html}", &ts_html)
        .replace("<a ", "<a target=\"_blank\" ")
}

pub(crate) async fn metrics_handler(metrics: Arc<Metrics>) -> impl IntoResponse {
    let snapshot = Snapshot::from(metrics.as_ref());
    Json(snapshot)
}

use crate::imports::*;
use axum::{
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use std::net::IpAddr;
use std::task::{Context, Poll};
use tower::Service;
use tracing::*;

#[derive(Clone)]
pub struct IpFilter<S> {
    inner: S,
    allowed_ips: Arc<Vec<IpAddr>>,
}

impl<S> IpFilter<S> {
    pub fn new(inner: S, allowed_ips: Vec<IpAddr>) -> Self {
        Self {
            inner,
            allowed_ips: Arc::new(allowed_ips),
        }
    }
}

#[derive(Clone)]
pub struct IpFilterLayer {
    allowed_ips: Arc<Vec<IpAddr>>,
}

impl IpFilterLayer {
    pub fn new(allowed_ips: Vec<IpAddr>) -> Self {
        Self {
            allowed_ips: Arc::new(allowed_ips),
        }
    }
}

impl<S> tower::Layer<S> for IpFilterLayer {
    type Service = IpFilter<S>;

    fn layer(&self, service: S) -> Self::Service {
        IpFilter::new(service, (*self.allowed_ips).clone())
    }
}

impl<S, B> Service<Request<B>> for IpFilter<S>
where
    S: Service<Request<B>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: IntoResponse,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future =
        futures::future::BoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let client_ip = request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|connect_info| connect_info.0.ip());

        let allowed_ips = self.allowed_ips.clone();
        let mut inner = std::mem::replace(&mut self.inner, unsafe { std::mem::zeroed() });
        Box::pin(async move {
            if let Some(ip) = client_ip {
                if !allowed_ips.contains(&ip) {
                    warn!("Rejected request from unauthorized IP: {}", ip);
                    return Ok((
                        StatusCode::FORBIDDEN,
                        "Access denied: IP not in allowed list",
                    )
                        .into_response());
                }
            } else {
                warn!("Could not determine client IP address");
                return Ok((
                    StatusCode::BAD_REQUEST,
                    "Could not determine client IP address",
                )
                    .into_response());
            }

            let response = inner.call(request).await?;
            Ok(response.into_response())
        })
    }
}

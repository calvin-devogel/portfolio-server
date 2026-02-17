use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::time::Instant;

use crate::metrics::app_metrics::AppMetrics;

pub struct MetricsMiddleware {
    metrics: AppMetrics
}

impl MetricsMiddleware {
    pub fn new(metrics: AppMetrics) -> Self {
        Self { metrics }
    }
}

impl<S, B> Transform<S, ServiceRequest> for MetricsMiddleware
where 
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = MetricsMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(MetricsMiddlewareService {
            service: Rc::new(service),
            metrics: self.metrics.clone(),
        }))
    }
}

pub struct MetricsMiddlewareService<S> {
    service: Rc<S>,
    metrics: AppMetrics,
}

impl<S, B> Service<ServiceRequest> for MetricsMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let path = req.path().to_string();
        let metrics = self.metrics.clone();
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let res = service.call(req).await?;

            let duration = start.elapsed().as_secs_f64();
            let status = res.status().as_u16();

            let route = sanitize_route(&path);
            metrics.record_request(&method, &route, status, duration);

            if method == "GET" && !path.starts_with("/api/") && !path.starts_with("/metrics") {
                metrics.record_page_view(&route);
            }

            Ok(res)
        })
    }
}

// Sanitize route to prevent high cardinality (replace IDs with placeholders)
fn sanitize_route(path: &str) -> String {
    // Simple pattern matching for common ID patterns
    let mut sanitized = path.to_string();
    
    // Replace UUIDs
    if sanitized.contains('-') {
        let parts: Vec<&str> = sanitized.split('/').collect();
        sanitized = parts.iter().map(|&part| {
            if part.len() == 36 && part.chars().filter(|&c| c == '-').count() == 4 {
                "{id}"
            } else {
                part
            }
        }).collect::<Vec<&str>>().join("/");
    }
    
    // Replace numeric IDs
    let parts: Vec<&str> = sanitized.split('/').collect();
    sanitized = parts.iter().map(|&part| {
        if !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) {
            "{id}"
        } else {
            part
        }
    }).collect::<Vec<&str>>().join("/");
    
    sanitized
}
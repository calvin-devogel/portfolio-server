use actix_web::{HttpResponse, ResponseError, http::StatusCode, web};
use prometheus::{HistogramOpts, HistogramVec, Registry};

use crate::core::error::{error_chain_fmt, response::{AppError, build_error_response}};
use super::models::{WebVitalName, WebVitalsPayload};

#[derive(thiserror::Error)]
pub enum MetricsError {
    #[error("Too many metrics in a single request")]
    TooManyMetrics,
    #[error("Invalid metric value: {0} for metric {1:?}")]
    InvalidMetricValue(f64, WebVitalName),
}

impl std::fmt::Debug for MetricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl AppError for MetricsError {
    fn code(&self) -> &'static str {
        match self {
            MetricsError::TooManyMetrics => "too_many_metrics",
            MetricsError::InvalidMetricValue(_, name) => match name {
                WebVitalName::Lcp => "invalid_lcp_value",
                WebVitalName::Inp => "invalid_inp_value",
                WebVitalName::Cls => "invalid_cls_value",
                WebVitalName::Ttfb => "invalid_ttfb_value",
                WebVitalName::Fcp => "invalid_fcp_value",
            },
        }
    }

    fn client_message(&self) -> &str {
        match self {
            MetricsError::TooManyMetrics => "Too many metrics in a single request",
            MetricsError::InvalidMetricValue(_, name) => match name {
                WebVitalName::Lcp => "Invalid value for metric LCP",
                WebVitalName::Inp => "Invalid value for metric INP",
                WebVitalName::Cls => "Invalid value for metric CLS",
                WebVitalName::Ttfb => "Invalid value for metric TTFB",
                WebVitalName::Fcp => "Invalid value for metric FCP",
            },
        }
    }

    fn http_status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl ResponseError for MetricsError {
    fn error_response(&self) -> HttpResponse {
        build_error_response(self)
    }
}

#[derive(Debug, Clone)]
pub struct WebVitalsMetrics {
    pub histogram: HistogramVec,
}

impl WebVitalsMetrics {
    pub fn new(registry: &Registry) -> Self {
        let histogram = HistogramVec::new(
            HistogramOpts::new("web_vitals", "Core Web Vitals")
                .namespace("portfolio")
                .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.5, 4.0, 10.0]),
            &["metric", "rating", "pathname"],
        )
        .expect("Failed to create web_vitals histogram");

        registry
            .register(Box::new(histogram.clone()))
            .expect("Failed to register web_vitals histogram");

        Self { histogram }
    }
}

const MAX_METRICS_PER_REQUEST: usize = 20;

#[tracing::instrument(name = "Ingest web vitals", skip(payload, metrics))]
pub async fn post_web_vitals(
    payload: web::Json<WebVitalsPayload>,
    metrics: web::Data<WebVitalsMetrics>,
) -> Result<HttpResponse, MetricsError> {
    if payload.metrics.len() > MAX_METRICS_PER_REQUEST {
        return Err(MetricsError::TooManyMetrics);
    }

    for entry in &payload.metrics {
        if !entry.value.is_finite() || entry.value < 0.0 {
            return Err(MetricsError::InvalidMetricValue(entry.value, entry.name.clone()));
        }

        let metric_name = match entry.name {
            WebVitalName::Lcp => "LCP",
            WebVitalName::Inp => "INP",
            WebVitalName::Cls => "CLS",
            WebVitalName::Ttfb => "TTFB",
            WebVitalName::Fcp => "FCP",
        };

        let rating = format!("{:?}", entry.rating).to_lowercase();
        let pathname = sanitize_pathname(&entry.pathname);

        metrics
            .histogram
            .with_label_values(&[metric_name, &rating, &pathname])
            .observe(entry.value / 1000.0); // ms -> seconds
    }

    Ok(HttpResponse::Ok().finish())
}

fn sanitize_pathname(raw: &str) -> String {
    let without_query = raw.split('?').next().unwrap_or("/");
    let truncated: String = without_query.chars().take(100).collect();

    if truncated.starts_with("/blog/") {
        "/blog".to_string()
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_pathname() {
        let input = "/blog/portfolio-retrospective-pt-one";
        let expected = "/blog";
        assert_eq!(sanitize_pathname(input), expected);
    }
}
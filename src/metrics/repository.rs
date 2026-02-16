use crate::errors::MetricsError;
use crate::models::metrics::*;
use actix_web::{ResponseError};
use chrono::{Duration, Utc};
use bigdecimal::{BigDecimal, FromPrimitive};
use sqlx::{PgPool, Result};
use uuid::Uuid;
use url::Url;
use sha2::{Sha256, Digest};

// replace with a fast, non-cryptographic hash algorithm
fn hash_session_id(session_id: &Uuid) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    format!("{:x}", hasher.finalize())
}

//?
fn extract_domain(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }

    Url::parse(url)
        .ok()
        .and_then(|p| p.host_str().map(String::from))
}

#[tracing::instrument(
    name = "Record page visit",
    skip(pool, visit)
)]
pub async fn record_page_visit(
    pool: &PgPool,
    visit: &PageVisitRequest,
) -> Result<Uuid, actix_web::Error> {
    let session_hash = hash_session_id(&visit.session_id);
    let referrer_domain = visit.referrer.as_ref().and_then(|r| extract_domain(r));

    let record = sqlx::query!(
        r#"
        INSERT INTO page_visits (page_path, referrer_domain, session_hash, duration_ms)
        VALUES ($1, $2, $3, $4)
        RETURNING visit_id
        "#,
        visit.page_path,
        referrer_domain,
        session_hash,
        visit.duration_ms,
    ).fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!("Failed to insert page visit");
        MetricsError::UnexpectedError(anyhow::anyhow!(error))
    })?;

    Ok(record.visit_id)
}

#[tracing::instrument(
    name = "Record performance metric",
    skip(pool, metric)
)]
pub async fn record_performance_metric(
    pool: &PgPool,
    metric: &PerformanceMetricRequest
) -> Result<Uuid, actix_web::Error> {
    let value: Option<BigDecimal> = FromPrimitive::from_f64(metric.metric_value);

    if value.is_none() {
        tracing::error!("Failed to convert metric value");
        Err(MetricsError::UnexpectedError(anyhow::anyhow!("Failed to convert metric value")))
    }

    let record = sqlx::query!(
        r#"
        INSERT INTO performance_metrics (page_path, metric_type, metric_value)
        VALUES ($1, $2, $3)
        RETURNING metric_id
        "#,
        metric.page_path,
        metric.metric_type,
        value // :madge:
    )
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!("Failed to insert page visit");
        MetricsError::UnexpectedError(anyhow::anyhow!(error))
    })?;

    Ok(record.metric_id)
}
use crate::errors::MetricsError;
use crate::models::metrics::*;
use chrono::{Duration, Utc};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
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
    // what if this is no good?
    let value: BigDecimal = FromPrimitive::from_f64(metric.metric_value)
        .ok_or_else(|| {
            tracing::error!("Failed to convert metric value to BigDecimal: {}", metric.metric_value);
            MetricsError::UnexpectedError(anyhow::anyhow!("Invalid metric value"))
        })?;

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

#[tracing::instrument(
    name = "get metrics summary",
    skip(pool)
)]
pub async fn get_metrics_summary(
    pool: &PgPool
) -> Result<MetricsSummary, actix_web::Error> {
    let now = Utc::now();
    let today_start = now.date_naive().and_hms_opt(0,0,0)
    .ok_or_else(|| {
        tracing::warn!("Failed to get today_start");
        MetricsError::UnexpectedError(anyhow::anyhow!("Failed to get today_start"))
    })?.and_utc();

    let last_hour = now - Duration::hours(1);
    let last_5_minutes = now - Duration::minutes(5);

    // active users in the last 5 minutes:
    let active_users = sqlx::query!(
        r#"
        SELECT COUNT(DISTINCT session_hash) as count
        FROM page_visits
        WHERE visited_at > $1
        "#,
        last_5_minutes
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get active users");
        MetricsError::UnexpectedError(anyhow::anyhow!(e))
    })?
    .count
    .unwrap_or(0);

    // total visits today
    let total_visits_today = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM page_visits
        WHERE visited_at >= $1
        "#,
        today_start
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get average response time");
        MetricsError::UnexpectedError(anyhow::anyhow!(e))
    })?
    .count
    .unwrap_or(0);

    let avg_response = sqlx::query!(
        r#"
        SELECT AVG(response_time_ms) as avg_time
        FROM server_metrics
        WHERE recorded_at > $1
        "#,
        last_hour
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::warn!("Failed to get average response time");
        MetricsError::UnexpectedError(anyhow::anyhow!(e))
    })?
    .avg_time
    .and_then(|t| t.to_f64())
    .unwrap_or(0.0);

    let error_rate_stats = sqlx::query!(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status_code >= 400) as error_count,
            COUNT(*) as total_count
        FROM server_metrics
        WHERE recorded_at > $1
        "#,
        last_hour
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get error rate data");
        MetricsError::UnexpectedError(anyhow::anyhow!(e))
    })?;

    let error_rate = if error_rate_stats.total_count.unwrap_or(0) > 0 {
        error_rate_stats.error_count.unwrap_or(0) as f64 / error_rate_stats.total_count.unwrap_or(1) as f64
    } else {
        0.0
    };
}

#[tracing::instrument(
    name = "Metrics cleanup",
    skip(pool)
)]
pub async fn cleanup_old_metrics(pool: &PgPool) -> Result<(), actix_web::Error> {
    sqlx::query!("SELECT cleanup_old_page_visits()")
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to run page visit cleanup");
            MetricsError::UnexpectedError(anyhow::anyhow!(e))
        })?;
    
    sqlx::query!("SELECT cleanup_old_performance_metrics()")
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to run performance metrics cleanup");
            MetricsError::UnexpectedError(anyhow::anyhow!(e))
        })?;

    sqlx::query!("SELECT cleanup_old_server_metrics()")
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to run server metrics cleanup");
            MetricsError::UnexpectedError(anyhow::anyhow!(e))
        })?;

    Ok(())
}
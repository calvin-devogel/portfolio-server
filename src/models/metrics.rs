use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct PageVisit {
    pub visit_id: Uuid,
    pub page_path: String,
    pub referrer_domain: Option<String>,
    pub visited_at: DateTime<Utc>,
    pub session_hash: String,
    pub duration_ms: Option<i32>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetric {
    pub metric_id: Uuid,
    pub page_path: String,
    pub metric_type: String,
    pub metric_value: f64,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PageVisitRequest {
    pub page_path: String,
    pub referrer: Option<String>,
    pub session_id: Uuid,
    pub duration_ms: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct PerformanceMetricRequest {
    pub page_path: String,
    pub metric_type: String,
    pub metric_value: f64,
}

#[derive(Debug, Serialize)]
pub struct MetricsSummary {
    pub active_users: i64,
    pub total_visits_today: i64,
    pub avg_response_time: f64,
    pub error_rate: f64,

    // page views
    pub top_pages: Vec<PageStats>,

    // performance
    pub avg_page_load_time: f64,
    pub avg_fcp: f64,
    pub avg_lcp: f64,

    // server health
    pub requests_per_minute: f64,
    pub error_count_hour: i64,
    pub slow_requests: Vec<SlowRequest>,

    // external sources
    pub cloudflare_requests_24h: Option<i64>,
    pub digitalocean_bandwidth_24h: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PageStats {
    pub page_path: String,
    pub visit_count: i64,
    pub avg_duration_ms: f64,
    pub unique_visitors: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlowRequest {
    pub endpoint: String,
    pub method: String,
    pub response_time_ms: i32,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RealtimeStats {
    pub active_users_count: i64,
    pub current_page_views: Vec<CurrentPageView>,
    pub recent_errors: Vec<RecentError>,
}

#[derive(Debug, Serialize)]
pub struct CurrentPageView {
    pub page_path: String,
    pub user_count: i64,
}

#[derive(Debug, Serialize)]
pub struct RecentError {
    pub endpoint: String,
    pub status_code: i32,
    pub occurred_at: DateTime<Utc>,
}
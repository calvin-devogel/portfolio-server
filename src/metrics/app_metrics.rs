// to start: the metrics we are interested in are as follows
// - total requests
// - request duration
// - total errors
// - active sessions
// - page views by route
// - resource utilization (active db connections + more eventually)
use opentelemetry::{global, KeyValue};
use opentelemetry::metrics::{Counter, Histogram, UpDownCounter, Meter};
use opentelemetry_sdk::metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider};
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use std::time::Duration;

#[derive(Clone)]
pub struct AppMetrics {
    http_requests_total: Counter<u64>,
    http_request_duration: Histogram<f64>,
    http_errors_total: Counter<u64>,

    active_sessions: UpDownCounter<i64>,

    page_views: Counter<u64>,

    db_connections_active: UpDownCounter<i64>,
    redis_connections_active: UpDownCounter<i64>,
}

impl AppMetrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            http_requests_total: meter
                .u64_counter("http_requests_total")
                .with_description("Total number of HTTP requests")
                .build(),
            
            http_request_duration: meter
                .f64_histogram("http_request_duration_seconds")
                .with_description("HTTP request duration in seconds")
                .build(),
            
            http_errors_total: meter
                .u64_counter("http_errors_total")
                .with_description("Total number of HTTP errors")
                .build(),
            
            active_sessions: meter
                .i64_up_down_counter("active_sessions")
                .with_description("Number of active user sessions")
                .build(),
            
            page_views: meter
                .u64_counter("page_views_total")
                .with_description("Total page views by route")
                .build(),
            
            db_connections_active: meter
                .i64_up_down_counter("db_connections_active")
                .with_description("Active database connections")
                .build(),
            
            redis_connections_active: meter
                .i64_up_down_counter("redis_connections_active")
                .with_description("Active Redis connections")
                .build(),
        }
    }

    pub fn record_request(&self, method: &str, route: &str, status: u16, duration: f64) {
        let labels = [
            KeyValue::new("method", method.to_string()),
            KeyValue::new("route", route.to_string()),
            KeyValue::new("status", status.to_string()),
        ];

        self.http_requests_total.add(1, &labels);
        self.http_request_duration.record(duration, &labels);

        if status >= 400 {
            self.http_errors_total.add(1, &labels);
        }
    }

    pub fn record_page_view(&self, route: &str) {
        self.page_views.add(1, &[KeyValue::new("route", route.to_string())]);
    }

    pub fn increment_active_sessions(&self) {
        self.active_sessions.add(1, &[]);
    }

    pub fn decrement_active_sessions(&self) {
        self.active_sessions.add(-1, &[]);
    }

    pub fn set_db_connections(&self, count: i64) {
        self.db_connections_active.add(count, &[]);
    }

    pub fn set_redis_connections(&self, count: i64) {
        self.redis_connections_active.add(count, &[]);
    }
}

pub fn init_metrics(otlp_endpoint: String) -> Result<AppMetrics, Box<dyn std::error::Error>> {
    // create OTLP exporter
    let exporter = MetricExporter::builder()
        .with_http()
        .with_endpoint(otlp_endpoint)
        .build()?;

    let reader = PeriodicReader::builder(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_interval(Duration::from_secs(60))
        .build();

    let provider = MeterProviderBuilder::default()
        .with_reader(reader)
        .build();

    global::set_meter_provider(provider);

    let meter = global::meter("portfolio-server");
    let metrics = AppMetrics::new(&meter);

    Ok(metrics)
}

// shutdown? probably not needed
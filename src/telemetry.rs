// let's actually understand what we're doing here
use tokio::task::JoinHandle;
use tracing::{Subscriber, subscriber::set_global_default};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{EnvFilter, Registry, fmt::MakeWriter, layer::SubscriberExt};

// compose multiple layers into a tracing subscriber
// impl Sub to avoid specifying the return type (?)
// explicitly call out Send + Sync so we can pass it to init_subscriber
pub fn get_subscriber<Sink>(
    name: String,
    env_filter: String,
    sink: Sink,
) -> impl Subscriber + Send + Sync
// higher-ranked trait bound
// aka: sink implements `MakeWriter` for all choices of the lifetime parameter
// (how long the data Sink is writing lives), can be shared across threads safely
where
    Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    // logging level filter (ie. info/debug) depending on the environment
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(env_filter));
    // bunyan formats log events into JSON
    let formatting_layer = BunyanFormattingLayer::new(name, sink);

    // assemble the subscriber pipeline starting from default
    Registry::default()
        // determines what gets logged
        .with(env_filter)
        // stores span contexts
        .with(JsonStorageLayer)
        // outputs the actual logs
        .with(formatting_layer)
}

/// # Panics
/// likewise should handle subscriber failures more gracefully
pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}

// this function maintains logging context when offloading work to a background thread
// by passing the context metadata of the current span to the new thread
pub fn spawn_blocking_with_tracing<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // get current span metadata
    let current_span = tracing::Span::current();
    // transfer ownership of the current span and the closure to the new thread
    // inside said thread, .in_scope enters the captured span, runs the function/logic,
    // then exits the span. Thus, for the duration of the operation, telemetry treats the
    // logging context as active to keep consistency across thread boundaries
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}

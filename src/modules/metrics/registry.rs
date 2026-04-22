use prometheus::{Counter, CounterVec, Opts, Registry};

#[derive(Clone)]
pub struct AppMetrics {
    pub auth_login_total: CounterVec,
    pub contact_messages_total: Counter,
    pub blog_views_total: Counter,
}

pub fn init_registry() -> (Registry, AppMetrics) {
    let registry = Registry::new();

    let auth_login_total = CounterVec::new(
        Opts::new("auth_login_total", "Login attempts by result")
            .namespace("portfolio_api"),
        &["result"],
    )
    .expect("Failed to create auth_login_total metric");

    let contact_messages_total = Counter::with_opts(
        Opts::new("contact_messages_total", "Contact form submissions")
            .namespace("portfolio_api"),
    )
    .expect("Failed to create contact_messages_total metric");

    let blog_views_total = Counter::with_opts(
        Opts::new("blog_views_total", "Total blog post views")
            .namespace("portfolio_api"),
    )
    .expect("Failed to create blog_views_total metric");

    registry
        .register(Box::new(auth_login_total.clone()))
        .expect("Failed to register auth_login_total metric");
    registry
        .register(Box::new(contact_messages_total.clone()))
        .expect("Failed to register contact_messages_total metric");
    registry
        .register(Box::new(blog_views_total.clone()))
        .expect("Failed to register blog_views_total metric");

    (
        registry,
        AppMetrics {
            auth_login_total,
            contact_messages_total,
            blog_views_total,
        },
    )
}
// Middleware + prometheus exporter setup

use std::time::Instant;

use axum::{extract::MatchedPath, http::Request, middleware::Next, response::IntoResponse};
use metrics::counter;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

pub fn report_dropped_events(cause: &'static str, quantity: u64) {
    counter!("capture_events_dropped_total", quantity, "cause" => cause);
}
pub fn setup_metrics_recorder() -> PrometheusHandle {
    // Ok I broke it at the end, but the limit on our ingress is 60 and that's a nicer way of reaching it
    const EXPONENTIAL_SECONDS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ];
    const BATCH_SIZES: &[f64] = &[
        1.0, 10.0, 25.0, 50.0, 75.0, 100.0, 250.0, 500.0, 750.0, 1000.0,
    ];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_requests_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .unwrap()
        .set_buckets_for_metric(Matcher::Suffix("_batch_size".to_string()), BATCH_SIZES)
        .unwrap()
        .install_recorder()
        .unwrap()
}

/// Middleware to record some common HTTP metrics
/// Generic over B to allow for arbitrary body types (eg Vec<u8>, Streams, a deserialized thing, etc)
/// Someday tower-http might provide a metrics middleware: https://github.com/tower-rs/tower-http/issues/57
pub async fn track_metrics<B>(req: Request<B>, next: Next<B>) -> impl IntoResponse {
    let start = Instant::now();

    let path = if let Some(matched_path) = req.extensions().get::<MatchedPath>() {
        matched_path.as_str().to_owned()
    } else {
        req.uri().path().to_owned()
    };

    let method = req.method().clone();

    // Run the rest of the request handling first, so we can measure it and get response
    // codes.
    let response = next.run(req).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method.to_string()),
        ("path", path),
        ("status", status),
    ];

    metrics::increment_counter!("http_requests_total", &labels);
    metrics::histogram!("http_requests_duration_seconds", latency, &labels);

    response
}

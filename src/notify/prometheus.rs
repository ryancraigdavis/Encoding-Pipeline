//! Prometheus metrics exporter.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use prometheus::{
    Counter, CounterVec, Gauge, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::error::NotificationError;
use crate::queue::job::EncodeResultMetadata;

/// Prometheus metrics for the encoding pipeline.
pub struct Metrics {
    /// Registry for all metrics.
    registry: Registry,
    /// Number of jobs in the queue.
    pub queue_depth: Gauge,
    /// Number of jobs in the dead letter queue.
    pub dead_letter_count: Gauge,
    /// Total encodes by status.
    pub encodes_total: CounterVec,
    /// Encode duration in seconds.
    pub encode_duration_seconds: Histogram,
    /// Size reduction ratio.
    pub size_reduction_ratio: Histogram,
    /// VMAF scores.
    pub vmaf_score: Histogram,
    /// Currently encoding jobs.
    pub jobs_in_progress: Gauge,
}

impl Metrics {
    /// Creates a new metrics instance with all gauges and counters.
    pub fn new() -> Result<Self, NotificationError> {
        let registry = Registry::new();

        let queue_depth = Gauge::new("encode_queue_depth", "Number of jobs waiting in queue")
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let dead_letter_count = Gauge::new(
            "encode_dead_letter_count",
            "Number of jobs in dead letter queue",
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let encodes_total = CounterVec::new(
            Opts::new("encodes_total", "Total number of encode operations"),
            &["status"],
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let encode_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "encode_duration_seconds",
                "Time taken to encode videos in seconds",
            )
            .buckets(vec![60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 14400.0]),
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let size_reduction_ratio = Histogram::with_opts(
            HistogramOpts::new(
                "encode_size_reduction_ratio",
                "Ratio of input size to output size",
            )
            .buckets(vec![1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 10.0]),
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let vmaf_score = Histogram::with_opts(
            HistogramOpts::new("encode_vmaf_score", "VMAF scores of encoded videos")
                .buckets(vec![80.0, 85.0, 90.0, 92.0, 94.0, 95.0, 96.0, 97.0, 98.0, 99.0]),
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        let jobs_in_progress = Gauge::new(
            "encode_jobs_in_progress",
            "Number of jobs currently being encoded",
        )
        .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        // Register all metrics
        registry
            .register(Box::new(queue_depth.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(dead_letter_count.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(encodes_total.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(encode_duration_seconds.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(size_reduction_ratio.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(vmaf_score.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;
        registry
            .register(Box::new(jobs_in_progress.clone()))
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        Ok(Self {
            registry,
            queue_depth,
            dead_letter_count,
            encodes_total,
            encode_duration_seconds,
            size_reduction_ratio,
            vmaf_score,
            jobs_in_progress,
        })
    }

    /// Records a successful encode.
    pub fn record_success(&self, metadata: &EncodeResultMetadata) {
        self.encodes_total.with_label_values(&["success"]).inc();
        self.encode_duration_seconds.observe(metadata.encode_duration_secs);
        self.size_reduction_ratio.observe(metadata.compression_ratio());

        if let Some(vmaf) = metadata.vmaf_score {
            self.vmaf_score.observe(vmaf as f64);
        }
    }

    /// Records a failed encode.
    pub fn record_failure(&self) {
        self.encodes_total.with_label_values(&["failure"]).inc();
    }

    /// Records a dead letter event.
    pub fn record_dead_letter(&self) {
        self.encodes_total.with_label_values(&["dead_letter"]).inc();
    }

    /// Updates queue depth gauge.
    pub fn set_queue_depth(&self, depth: usize) {
        self.queue_depth.set(depth as f64);
    }

    /// Updates dead letter count gauge.
    pub fn set_dead_letter_count(&self, count: usize) {
        self.dead_letter_count.set(count as f64);
    }

    /// Updates jobs in progress gauge.
    pub fn set_jobs_in_progress(&self, count: usize) {
        self.jobs_in_progress.set(count as f64);
    }

    /// Returns the metrics in Prometheus text format.
    pub fn gather(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

/// HTTP server for Prometheus metrics.
pub struct MetricsServer {
    /// Metrics instance.
    metrics: Arc<Metrics>,
    /// Port to listen on.
    port: u16,
}

impl MetricsServer {
    /// Creates a new metrics server.
    pub fn new(metrics: Arc<Metrics>, port: u16) -> Self {
        Self { metrics, port }
    }

    /// Starts the metrics HTTP server.
    pub async fn start(self) -> Result<(), NotificationError> {
        use hyper::server::conn::http1;
        use hyper::service::service_fn;
        use hyper::{body::Incoming, Request, Response};
        use hyper_util::rt::TokioIo;
        use http_body_util::Full;
        use hyper::body::Bytes;

        let addr: SocketAddr = ([0, 0, 0, 0], self.port).into();
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

        info!(port = self.port, "Starting Prometheus metrics server");

        let metrics = self.metrics.clone();

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| NotificationError::PrometheusFailed(e.to_string()))?;

            let io = TokioIo::new(stream);
            let metrics = metrics.clone();

            tokio::spawn(async move {
                let service = service_fn(|req: Request<Incoming>| {
                    let metrics = metrics.clone();
                    async move {
                        if req.uri().path() == "/metrics" {
                            let body = metrics.gather();
                            Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(body))))
                        } else {
                            Ok(Response::builder()
                                .status(404)
                                .body(Full::new(Bytes::from("Not Found")))
                                .unwrap())
                        }
                    }
                });

                if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                    error!(error = %e, "Error serving connection");
                }
            });
        }
    }
}

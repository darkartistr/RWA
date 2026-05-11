use prometheus::{Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder};
use std::sync::Arc;

#[derive(Clone)]
pub struct Metrics {
    registry: Arc<Registry>,
    pub http_requests: IntCounterVec,
    pub token_ops: IntCounterVec,
    pub solana_rpc_seconds: HistogramVec,
}

impl Metrics {
    pub fn new() -> anyhow::Result<Self> {
        let registry = Registry::new();

        let http_requests = IntCounterVec::new(
            Opts::new("http_requests_total", "Total HTTP requests"),
            &["method", "path", "status"],
        )?;

        let token_ops = IntCounterVec::new(
            Opts::new("token_ops_total", "Token operations total"),
            &["op", "status"],
        )?;

        let solana_rpc_seconds = HistogramVec::new(
            HistogramOpts::new("solana_rpc_seconds", "Solana RPC call latency seconds"),
            &["op"],
        )?;

        registry.register(Box::new(http_requests.clone()))?;
        registry.register(Box::new(token_ops.clone()))?;
        registry.register(Box::new(solana_rpc_seconds.clone()))?;

        Ok(Self {
            registry: Arc::new(registry),
            http_requests,
            token_ops,
            solana_rpc_seconds,
        })
    }

    pub fn gather(&self) -> Vec<u8> {
        let mf = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut out = Vec::new();
        let _ = encoder.encode(&mf, &mut out);
        out
    }
}


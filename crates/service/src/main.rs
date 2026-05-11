mod api;
mod config;
mod db;
mod grpc;
mod kafka;
mod metrics;
mod solana;
#[cfg(test)]
mod solana_tests;

use anyhow::Context;
use axum::Router;
use config::Config;
use db::Db;
use kafka::KafkaProducer;
use metrics::Metrics;
use solana::SolanaTokenClient;
use std::net::SocketAddr;
use tokio::signal;
use tonic::transport::Server;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = Config::from_env();
    let db = Db::connect(&cfg.database_url).await?;
    let metrics = Metrics::new()?;
    let kafka = KafkaProducer::new(&cfg.kafka_brokers, &cfg.kafka_topic_token_events)?;
    let sol = SolanaTokenClient::new(
        &cfg.solana_rpc_url,
        &cfg.solana_commitment,
        &cfg.solana_keypair_path,
    )
    .context("init solana client")?;
    info!(
        authority = %sol.authority_pubkey(),
        rpc = %cfg.solana_rpc_url,
        "solana client ready"
    );

    let app_state = api::AppState {
        db,
        kafka,
        sol,
        metrics: metrics.clone(),
    };

    let http_addr: SocketAddr = cfg.http_addr.parse().context("HTTP_ADDR parse")?;
    let grpc_addr: SocketAddr = cfg.grpc_addr.parse().context("GRPC_ADDR parse")?;

    let http_app: Router = api::router(app_state.clone(), metrics.clone());

    let grpc_svc = grpc::TokenGrpcService::new(app_state.clone());

    let http_server = async move {
        info!(%http_addr, "http server listening");
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        axum::serve(listener, http_app).await?;
        Ok::<(), anyhow::Error>(())
    };

    let grpc_server = async move {
        info!(%grpc_addr, "grpc server listening");
        Server::builder()
            .add_service(rwa_proto::rwa::token::v1::token_service_server::TokenServiceServer::new(
                grpc_svc,
            ))
            .serve(grpc_addr)
            .await?;
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        res = http_server => { res?; }
        res = grpc_server => { res?; }
        _ = shutdown_signal() => {
            warn!("shutdown signal received");
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let _ = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}


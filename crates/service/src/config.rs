#[derive(Clone, Debug)]
pub struct Config {
    pub http_addr: String,
    pub grpc_addr: String,
    pub solana_rpc_url: String,
    pub solana_commitment: String,
    pub solana_keypair_path: String,
    pub database_url: String,
    pub kafka_brokers: String,
    pub kafka_topic_token_events: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            http_addr: env("HTTP_ADDR", "0.0.0.0:8080"),
            grpc_addr: env("GRPC_ADDR", "0.0.0.0:9090"),
            solana_rpc_url: env("SOLANA_RPC_URL", "https://api.devnet.solana.com"),
            solana_commitment: env("SOLANA_COMMITMENT", "confirmed"),
            solana_keypair_path: env("SOLANA_KEYPAIR_PATH", "./secrets/authority.json"),
            database_url: env("DATABASE_URL", "postgres://rwa:rwa@localhost:5432/rwa"),
            kafka_brokers: env("KAFKA_BROKERS", "localhost:9092"),
            kafka_topic_token_events: env("KAFKA_TOPIC_TOKEN_EVENTS", "rwa.token.events.v1"),
        }
    }
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}


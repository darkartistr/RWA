use anyhow::Context;
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::Duration;

#[derive(Clone)]
pub struct KafkaProducer {
    producer: FutureProducer,
    topic: String,
}

impl KafkaProducer {
    pub fn new(brokers: &str, topic: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .context("create kafka producer")?;

        Ok(Self {
            producer,
            topic: topic.to_string(),
        })
    }

    pub async fn publish_json(&self, key: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(payload).context("serialize kafka payload")?;
        self.producer
            .send(
                FutureRecord::to(&self.topic)
                    .key(key)
                    .payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!(e))?;
        Ok(())
    }
}


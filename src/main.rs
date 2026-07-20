use std::cmp;
use std::time::{Duration, Instant};

use anyhow::Context;
use paho_mqtt::Message;
use tracing::{error, info};

/// Initial delay between reconnection attempts.
const INITIAL_BACKOFF_SECS: u64 = 2;
/// Maximum delay between reconnection attempts (cap for exponential growth).
const MAX_BACKOFF_SECS: u64 = 300;
/// Maximum total time spent retrying before the process crashes.
/// Kubernetes will then restart the pod with its own backoff policy.
const MAX_RECONNECT_DURATION_SECS: u64 = 600;

use producer::amqp::AmqpClient;
use producer::config::{Env, init};
use producer::errors::message_error::MessageError;
use producer::mqtt::get_bytes_from_payload;
use producer::mqtt::mqtt_client::MqttClient;
use producer::mqtt::mqtt_config::MqttConfig;
use producer::mqtt::mqtt_options::MqttOptions;

const TOPICS: &[&str] = &[
    "sensors/+/temperature",
    "sensors/+/humidity",
    "sensors/+/light",
    "sensors/+/motion",
    "sensors/+/airquality",
    "sensors/+/airpressure",
    "sensors/+/mode",
    "sensors/+/online",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Init logger and env
    let env: Env = init();

    // 2. Init RabbitMQ
    info!(target: "app", "Initializing RabbitMQ...");
    let mut amqp_client = AmqpClient::new(
        env.amqp_uri.clone(),
        env.amqp_queue_name.clone(),
        env.amqp_hmac_secret.clone(),
    );
    amqp_client.connect(false).await.context("RabbitMQ: cannot connect")?;

    // 3. Init MQTT
    info!(target: "app", "Initializing MQTT...");
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    let mqtt_opts = MqttOptions::new(&mqtt_config).context("MQTT: cannot create MqttOptions")?;
    let mut mqtt_client = MqttClient::new(mqtt_opts).context("MQTT: cannot create client")?;

    // Retry connect with exponential backoff until the broker is reachable
    let start = Instant::now();
    let mut backoff = Duration::from_secs(INITIAL_BACKOFF_SECS);
    loop {
        match mqtt_client.connect().await {
            Ok(()) => break,
            Err(err) => {
                let elapsed = start.elapsed().as_secs();
                if start.elapsed() > Duration::from_secs(MAX_RECONNECT_DURATION_SECS) {
                    error!(target: "app", "MQTT connection failed after {elapsed} seconds, giving up");
                    return Err(anyhow::anyhow!("MQTT: connection timed out after {elapsed} seconds"));
                }
                let secs = backoff.as_secs();
                error!(target: "app", "MQTT connection error, retrying in {secs} seconds: {err:?}");
                tokio::time::sleep(backoff).await;
                backoff = cmp::min(backoff * 2, Duration::from_secs(MAX_BACKOFF_SECS));
            }
        }
    }
    mqtt_client
        .subscribe(TOPICS)
        .await
        .context("MQTT: cannot subscribe to topics")?;

    // 4. Wait for incoming MQTT messages
    info!(target: "app", "Waiting for incoming MQTT messages");
    while let Some(msg_opt) = mqtt_client.get_next_message().await {
        if let Err(err) = process_mqtt_message(
            msg_opt.as_ref(),
            &mut mqtt_client,
            &mut amqp_client,
            &env.amqp_queue_name,
            TOPICS,
        )
        .await
        {
            error!(target: "app", "listen_for_messages - failed to process MQTT message: {err:?}");
        }
    }
    Ok(())
}

async fn process_mqtt_message(
    msg_opt: Option<&Message>,
    mqtt_client: &mut MqttClient,
    amqp_client: &mut AmqpClient,
    amqp_queue_name: &str,
    topics: &[&str],
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        let msg_byte =
            get_bytes_from_payload(msg).ok_or_else(|| anyhow::Error::from(MessageError::EmptyMessageError))?;
        amqp_client.publish_message(amqp_queue_name, &msg_byte).await.map_err(|err| {
            error!(target: "app", "listen_for_messages - Cannot publish AMQP message to queue {amqp_queue_name}. Err ={err:?}");
            anyhow::Error::from(err)
        })
    } else {
        // msg_opt="None" means we were disconnected. Try to reconnect...
        error!(target: "app", "listen_for_messages - Lost connection. Attempting reconnect...");
        let start = Instant::now();
        let mut backoff = Duration::from_secs(INITIAL_BACKOFF_SECS);
        while let Err(err) = mqtt_client.reconnect().await {
            let elapsed = start.elapsed().as_secs();
            if start.elapsed() > Duration::from_secs(MAX_RECONNECT_DURATION_SECS) {
                error!(target: "app", "listen_for_messages - Reconnection failed after {elapsed} seconds, giving up");
                return Err(anyhow::anyhow!("MQTT: reconnect timed out after {elapsed} seconds"));
            }
            let secs = backoff.as_secs();
            error!(target: "app", "listen_for_messages - Error reconnecting: {err:?}, retrying in {secs} seconds...");
            tokio::time::sleep(backoff).await;
            backoff = cmp::min(backoff * 2, Duration::from_secs(MAX_BACKOFF_SECS));
        }
        // Re-subscribe to topics after reconnection, as the broker may have
        // discarded the session (e.g. session expiry or clean_session mismatch).
        if let Err(err) = mqtt_client.subscribe(topics).await {
            error!(target: "app", "listen_for_messages - Failed to re-subscribe after reconnect: {err:?}");
        }
        Ok(())
    }
}

// testing
#[cfg(test)]
mod tests_integration;

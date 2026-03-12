use std::time::Duration;

use futures::executor::block_on;
use paho_mqtt::Message;
use tracing::{debug, error, info};

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
    "sensors/+/online",
];

#[tokio::main]
async fn main() {
    // 1. Init logger and env
    let env: Env = init();

    // 2. Init RabbitMQ
    info!(target: "app", "Initializing RabbitMQ...");
    // let handle = tokio::runtime::Handle::current();
    let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    amqp_client.connect(false).await;

    // 3. Init MQTT
    info!(target: "app", "Initializing MQTT...");
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                panic!("unknown error, because MQTT cannot subscribe to TOPICS");
            }
            // 4. Wait for incoming MQTT messages
            info!(target: "app", "Waiting for incoming MQTT messages");
            while let Some(msg_opt) = mqtt_client.get_next_message().await {
                if !amqp_client.is_connected(false) {
                    error!(target: "app", "AMQP is not connected, cannot publish MQTT message, MQTT message lost.");
                    continue;
                }
                let _ = process_mqtt_message(
                    &msg_opt,
                    &mut mqtt_client,
                    &mut amqp_client,
                    env.amqp_queue_name.clone().as_str(),
                )
                .await;
            }
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("unknown error, cannot create MQTT client");
        }
    }
}

async fn process_mqtt_message(
    msg_opt: &Option<Message>,
    mqtt_client: &mut MqttClient,
    amqp_client: &mut AmqpClient,
    amqp_queue_name: &str,
) -> Result<(), anyhow::Error> {
    if let Some(msg) = msg_opt {
        debug!(target: "app", "listen_for_messages - MQTT message received");
        let msg_byte: Vec<u8> = get_bytes_from_payload(msg);
        // return this if
        if msg_byte.is_empty() {
            // msg is not valid, because empty
            info!(target: "app", "listen_for_messages - Empty msg_byte received");
            Err(anyhow::Error::from(MessageError::EmptyMessageError))
        } else {
            // return the result of block_on(...)
            block_on(async {
                // send via AMQP
                let result = amqp_client.publish_message(amqp_queue_name, msg_byte).await;
                match result {
                    Ok(_) => {
                        debug!(target: "app", "listen_for_messages - AMQP message published to queue {}", &amqp_queue_name);
                        Ok(())
                    }
                    Err(err) => {
                        error!(target: "app", "listen_for_messages - Cannot publish AMQP message to queue {}. Err ={:?}", &amqp_queue_name, err);
                        Err(anyhow::Error::from(err))
                    }
                }
            })
        }
    } else {
        // msg_opt="None" means we were disconnected. Try to reconnect...
        error!(target: "app", "listen_for_messages - Lost connection. Attempting reconnect in 5 seconds...");
        while let Err(err) = mqtt_client.reconnect().await {
            error!(target: "app", "listen_for_messages - Error reconnecting: {:?}, retrying in 5 seconds...", err);
            tokio::time::sleep(Duration::from_millis(5000)).await;
        }
        debug!(target: "app", "listen_for_messages - Connection re-established");
        Ok(())
    }
}

// testing
#[cfg(test)]
mod tests_integration;

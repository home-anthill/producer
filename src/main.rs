use dotenvy::dotenv;
use futures::{executor::block_on, stream::StreamExt};
use log::{debug, error, info, warn};
use std::process;
use std::time::Duration;

use producer::amqp::AmqpClient;
use producer::config::{print_env, Env};
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
];

#[tokio::main]
async fn main() {
    // 1. Init logger if not in testing environment
    let _ = log4rs::init_file("log4rs.yaml", Default::default());
    info!(target: "app", "Starting application...");

    // 2. Load the .env file
    dotenv().ok();
    let env = envy::from_env::<Env>().ok().unwrap();

    // 3. Print .env vars
    print_env(&env);

    // 4. Init RabbitMQ
    info!(target: "app", "Initializing RabbitMQ");
    let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    amqp_client.connect_with_retry_loop().await;

    // 5. Init MQTT
    info!(target: "app", "Initializing MQTT");
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                process::exit(1);
            }
            // 6. Wait for incoming MQTT messages
            info!(target: "app", "Waiting for incoming MQTT messages");
            listen_for_messages(&mut mqtt_client, &mut amqp_client).await;
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
        }
    }
}

async fn listen_for_messages(mqtt_client: &mut MqttClient, amqp_client: &mut AmqpClient) {
    while let Some(msg_opt) = mqtt_client.get_next_message().await {
        if let Some(msg) = msg_opt {
            debug!(target: "app", "listen_for_messages - MQTT message received");
            let msg_byte: Vec<u8> = get_bytes_from_payload(&msg);
            if msg_byte.is_empty() {
                // msg is not valid, because not parsable as utf-8
                debug!(target: "app", "listen_for_messages - Empty msg_byte received");
                continue;
            }
            block_on(async {
                if !amqp_client.is_connected() {
                    debug!(target: "app", "listen_for_messages - AMQP channel is not connected, reconnecting...");
                    amqp_client.connect_with_retry_loop().await;
                }
                debug!(target: "app", "listen_for_messages - Publishing message via AMQP...");
                // send via AMQP
                match amqp_client.publish_message(msg_byte).await {
                    Ok(_) => {
                        debug!(target: "app", "listen_for_messages - AMQP message published to queue {}", amqp_client.amqp_queue_name);
                    }
                    Err(err) => {
                        error!(target: "app", "listen_for_messages - Cannot publish AMQP message to queue {}. Err ={:?}", amqp_client.amqp_queue_name, err);
                    }
                };
            });
        } else {
            // msg_opt="None" means we were disconnected. Try to reconnect...
            warn!(target: "app", "listen_for_messages - Lost connection. Attempting reconnect in 5 seconds...");
            while let Err(err) = mqtt_client.reconnect().await {
                error!(target: "app", "listen_for_messages - Error reconnecting: {:?}, retrying in 5 seconds...", err);
                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }
    }
}

// testing
#[cfg(test)]
mod tests;

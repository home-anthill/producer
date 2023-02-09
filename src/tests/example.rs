use dotenvy::dotenv;
use log::{error, info};
use std::process;

use producer::amqp::AmqpClient;
use producer::config::{print_env, Env};
use producer::mqtt::mqtt_client::MqttClient;
use producer::mqtt::mqtt_config::MqttConfig;
use producer::mqtt::mqtt_options::MqttOptions;

use crate::{listen_for_messages, TOPICS};

// 1. emit mqtt message
// 2. check if listen_for_messages is invoked
// 3. emit data on rabbitmq
// 4. read the queue to find the message

#[tokio::test]
async fn example() {
    // init
    dotenv().ok();
    let env = envy::from_env::<Env>().ok().unwrap();
    print_env(&env);

    // let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    // amqp_client.connect_with_retry_loop().await;

    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            // mqtt_client.connect().await;
            // if let Err(err) = mqtt_client.subscribe(TOPICS).await {
            //     error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
            //     process::exit(1);
            // }
            // 6. Wait for incoming MQTT messages
            // info!(target: "app", "Waiting for incoming MQTT messages");
            // listen_for_messages(&mut mqtt_client, &mut amqp_client).await;
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
        }
    }

    // run tests for every sensor_type

    // check results

    // cleanup
}

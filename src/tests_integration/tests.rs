use std::env;
use std::process::Command;

use paho_mqtt::Message;
use pretty_assertions::assert_eq;
use tracing::{debug, error};
use uuid::Uuid;

use producer::amqp::AmqpClient;
use producer::config::{Env, init};
use producer::models::get_msg_byte;
use producer::models::topic::Topic;
use producer::mqtt::mqtt_client::MqttClient;
use producer::mqtt::mqtt_config::MqttConfig;
use producer::mqtt::mqtt_options::MqttOptions;

use crate::{TOPICS, process_mqtt_message};

const DEFAULT_MQTT_PUBLISH_USER: &str = "device_pubsub";
const DEFAULT_MQTT_PUBLISH_PASSWORD: &str = "DevicePassword1!";

fn mqtt_publish_credentials() -> (String, String) {
    let user = env::var("MQTT_PUBLISH_USER").unwrap_or_else(|_| DEFAULT_MQTT_PUBLISH_USER.to_string());
    let password = env::var("MQTT_PUBLISH_PASSWORD").unwrap_or_else(|_| DEFAULT_MQTT_PUBLISH_PASSWORD.to_string());
    (user, password)
}

fn mqtt_test_config(env: &Env) -> MqttConfig {
    let mut mqtt_config = MqttConfig::new(env);
    mqtt_config.client_id = format!("{}-test-{}", env.mqtt_client_id, Uuid::new_v4());
    mqtt_config
}

#[tokio::test]
#[test_log::test]
async fn receive_message_via_mqtt() {
    let env: Env = init();

    let mqtt_config = mqtt_test_config(&env);
    let mut mqtt_client = MqttClient::new(MqttOptions::new(&mqtt_config).expect("cannot create MqttOptions"))
        .unwrap_or_else(|err| {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("cannot create MQTT client: {err:?}")
        });
    mqtt_client.connect().await.expect("mqtt connect failed");
    mqtt_client.subscribe(TOPICS).await.unwrap_or_else(|err| {
        error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
        panic!("cannot subscribe to MQTT topics: {err:?}")
    });
    let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
    let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
    let sensor_type = "temperature";
    let value = 12.23;
    let msg_payload_str = format!(
        r#"{{"deviceUuid":"{device_uuid}", "featureUuid":"{feature_uuid}", "timestamp":1777630000, "nonce":"00112233445566778899aabbccddeeff", "signature":"aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899","payload":{{"value":{value}}}}}"#
    );

    let (mqtt_publish_user, mqtt_publish_password) = mqtt_publish_credentials();
    let status = Command::new("mosquitto_pub")
        .arg("-h")
        .arg(env.mqtt_url.as_str())
        .arg("-p")
        .arg(env.mqtt_port.to_string())
        .arg("-u")
        .arg(mqtt_publish_user.as_str())
        .arg("-P")
        .arg(mqtt_publish_password.as_str())
        .arg("-m")
        .arg(&msg_payload_str)
        .arg("-t")
        .arg(format!("sensors/{}/{}", device_uuid, sensor_type))
        .status()
        .expect("command failed to start");
    assert!(status.success(), "mosquitto_pub failed with status {status}");

    let msg_opt_opt = mqtt_client.get_next_message().await;
    let msg_mqtt = msg_opt_opt
        .expect("message stream ended unexpectedly")
        .expect("got disconnection signal instead of message");
    let message = std::str::from_utf8(msg_mqtt.payload()).expect("message payload is not valid UTF-8");
    debug!(target: "app", "message = {}", &message);

    assert_eq!(message, msg_payload_str);
}

#[tokio::test]
#[test_log::test]
async fn send_mqtt_message_via_amqp() {
    let env: Env = init();

    let mut amqp_client = AmqpClient::new(
        env.amqp_uri.clone(),
        env.amqp_queue_name.clone(),
        env.amqp_hmac_secret.clone(),
    );
    amqp_client.connect(false).await.expect("amqp connect failed");

    let mqtt_config = mqtt_test_config(&env);
    let mut mqtt_client = MqttClient::new(MqttOptions::new(&mqtt_config).expect("cannot create MqttOptions"))
        .unwrap_or_else(|err| {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("cannot create MQTT client: {err:?}")
        });
    mqtt_client.connect().await.expect("mqtt connect failed");
    mqtt_client.subscribe(TOPICS).await.unwrap_or_else(|err| {
        error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
        panic!("cannot subscribe to MQTT topics: {err:?}")
    });
    let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
    let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
    let sensor_type = "temperature";
    let value = 12.23;
    let msg_payload_str = format!(
        r#"{{"deviceUuid":"{device_uuid}", "featureUuid":"{feature_uuid}", "timestamp":1777630000, "nonce":"00112233445566778899aabbccddeeff", "signature":"aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899","payload":{{"value":{value}}}}}"#
    );
    let topic: Topic = Topic::new(format!("sensors/{}/{}", device_uuid, sensor_type).as_str()).unwrap();
    let msg_byte_arr = get_msg_byte(&topic, msg_payload_str.as_str()).expect("expected Some bytes");
    let message = Message::new(format!("sensors/{}/{}", device_uuid, sensor_type), msg_byte_arr, 0);

    let result = process_mqtt_message(Some(&message), &mut mqtt_client, &mut amqp_client, "", TOPICS).await;
    assert_eq!(result.unwrap(), ());
}

#[tokio::test]
#[test_log::test]
async fn wrong_sensor_type_for_process_mqtt_message() {
    let _env: Env = init();

    let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
    let sensor_type = "unknown_type";

    // H5: Topic::new now rejects unknown feature names at construction time,
    // so no message ever reaches process_mqtt_message with an invalid sensor type.
    let topic_result = Topic::new(format!("sensors/{}/{}", device_uuid, sensor_type).as_str());
    assert!(
        topic_result.is_err(),
        "expected Topic::new to reject unknown feature name"
    );
}

#[tokio::test]
#[test_log::test]
async fn reconnect_to_mqtt_on_message() {
    let env: Env = init();

    let mut amqp_client = AmqpClient::new(
        env.amqp_uri.clone(),
        env.amqp_queue_name.clone(),
        env.amqp_hmac_secret.clone(),
    );
    amqp_client.connect(false).await.expect("amqp connect failed");

    let mqtt_config = mqtt_test_config(&env);
    let mut mqtt_client = MqttClient::new(MqttOptions::new(&mqtt_config).expect("cannot create MqttOptions"))
        .unwrap_or_else(|err| {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("cannot create MQTT client: {err:?}")
        });
    mqtt_client.connect().await.expect("mqtt connect failed");
    mqtt_client.subscribe(TOPICS).await.unwrap_or_else(|err| {
        error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
        panic!("cannot subscribe to MQTT topics: {err:?}")
    });
    let _ = mqtt_client.disconnect().await;

    let result = process_mqtt_message(
        None,
        &mut mqtt_client,
        &mut amqp_client,
        env.amqp_queue_name.as_str(),
        TOPICS,
    )
    .await;

    assert_eq!(result.unwrap(), ());
}

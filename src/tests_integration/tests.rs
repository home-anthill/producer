use std::process::Command;

use paho_mqtt::Message;
use pretty_assertions::assert_eq;
use tracing::{debug, error};

use producer::amqp::AmqpClient;
use producer::config::{Env, init};
use producer::models::get_msg_byte;
use producer::models::topic::Topic;
use producer::mqtt::mqtt_client::MqttClient;
use producer::mqtt::mqtt_config::MqttConfig;
use producer::mqtt::mqtt_options::MqttOptions;

use crate::{TOPICS, process_mqtt_message};

#[tokio::test]
#[test_log::test]
async fn receive_message_via_mqtt() {
    let env: Env = init();

    let mqtt_config: MqttConfig = MqttConfig::new(&env);
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
    let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
    let sensor_type = "temperature";
    let value = 12.23;
    let msg_payload_str = format!(
        r#"{{"deviceUuid":"{device_uuid}", "featureUuid":"{feature_uuid}", "apiToken":"{api_token}","payload":{{"value":{value}}}}}"#
    );

    Command::new("mosquitto_pub")
        .arg("-u")
        .arg("mosquser")
        .arg("-P")
        .arg("Password1!")
        .arg("-m")
        .arg(&msg_payload_str)
        .arg("-t")
        .arg(format!("sensors/{}/{}", device_uuid, sensor_type))
        .spawn()
        .expect("command failed to start");

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

    let mqtt_config: MqttConfig = MqttConfig::new(&env);
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
    let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
    let sensor_type = "temperature";
    let value = 12.23;
    let msg_payload_str = format!(
        r#"{{"deviceUuid":"{device_uuid}", "featureUuid":"{feature_uuid}", "apiToken":"{api_token}","payload":{{"value":{value}}}}}"#
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

    let mqtt_config: MqttConfig = MqttConfig::new(&env);
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

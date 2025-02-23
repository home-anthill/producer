use std::process::Command;

use log::{debug, error};
use paho_mqtt::Message;
use pretty_assertions::assert_eq;

use producer::amqp::AmqpClient;
use producer::config::{Env, init};
use producer::errors::message_error::MessageError;
use producer::models::get_msg_byte;
use producer::models::topic::Topic;
use producer::mqtt::mqtt_client::MqttClient;
use producer::mqtt::mqtt_config::MqttConfig;
use producer::mqtt::mqtt_options::MqttOptions;

use crate::{TOPICS, process_mqtt_message};

#[tokio::test]
async fn receive_message_via_mqtt() {
    // init logger and env variables
    let env: Env = init();

    // init MQTT client
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            // connect to MQTT server and subscribe to topics
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                panic!("unknown error, because MQTT cannot subscribe to TOPICS");
            }
            // create MQTT message payload
            let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
            let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
            let sensor_type = "temperature";
            let value = 12.23;
            let msg_payload_str = r#"{"uuid":""#.to_owned()
                + uuid
                + r#"","apiToken":""#
                + api_token
                + r#"","payload":{"value":"#
                + value.to_string().as_str()
                + r#"}}"#;

            // send an MQTT message to the server via `mosquitto_pub` cli
            Command::new("mosquitto_pub")
                .arg("-u")
                .arg("mosquser")
                .arg("-P")
                .arg("Password1!")
                .arg("-m")
                .arg(&msg_payload_str)
                .arg("-t")
                .arg(format!("sensors/{}/{}", uuid, sensor_type))
                .spawn()
                .expect("command failed to start");

            // receive MQTT message
            let msg_opt_opt = mqtt_client.get_next_message().await;
            let msg_mqtt = &msg_opt_opt.unwrap().unwrap();
            let message = std::str::from_utf8(msg_mqtt.payload()).unwrap();
            debug!(target: "app", "message = {}", &message);

            // check results: received message and sent message payloads must be equal
            assert_eq!(&message, &msg_payload_str);
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("unknown error, cannot create MQTT client");
        }
    }
}

#[tokio::test]
async fn send_mqtt_message_via_amqp() {
    // init logger and env variables
    let env: Env = init();

    // init AMQP client
    let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    amqp_client.connect_with_retry_loop().await;

    // init MQTT client
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            // connect to MQTT server and subscribe to topics
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                panic!("unknown error, because MQTT cannot subscribe to TOPICS");
            }
            // create MQTT message payload
            let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
            let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
            let sensor_type = "temperature";
            let value = 12.23;
            let msg_payload_str = r#"{"uuid":""#.to_owned()
                + uuid
                + r#"","apiToken":""#
                + api_token
                + r#"","payload":{"value":"#
                + value.to_string().as_str()
                + r#"}}"#;
            let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str());
            let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, msg_payload_str.as_str());
            let message = Message::new(format!("sensors/{}/{}", uuid, sensor_type), msg_byte_arr, 0);

            // send MQTT message via AMQP
            let result = process_mqtt_message(&Some(message), &mut mqtt_client, &mut amqp_client).await;

            // check result: it should return () if `process_mqtt_message`
            // successfully sent the message via AMQP
            assert_eq!(result.unwrap(), ());
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("unknown error, cannot create MQTT client");
        }
    }
}

#[tokio::test]
async fn wrong_sensor_type_for_process_mqtt_message() {
    // init logger and env variables
    let env: Env = init();

    // create an instance of AMQP client
    let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    // create an instance of MQTT client
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    let mut mqtt_client = MqttClient::new(MqttOptions::new(&mqtt_config)).unwrap();

    // create a bad MQTT message payload with an unknown sensor type
    let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
    let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
    let sensor_type = "unknown_type";
    let value = 12.23;
    let msg_payload_str = r#"{"uuid":""#.to_owned()
        + uuid
        + r#"", "apiToken":""#
        + api_token
        + r#"","payload":{"value":"#
        + value.to_string().as_str()
        + r#"}}"#;
    let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str());
    let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, msg_payload_str.as_str());
    let message = Message::new(format!("sensors/{}/{}", uuid, sensor_type), msg_byte_arr, 0);

    // invoke `process_mqtt_message` with the bad MQTT message
    let result = process_mqtt_message(&Some(message), &mut mqtt_client, &mut amqp_client).await;

    // check result: it should return MessageError::EmptyMessageError,
    // because sensor_type is unknown
    assert_eq!(
        result.err().unwrap().to_string(),
        anyhow::Error::from(MessageError::EmptyMessageError).to_string()
    );
}

#[tokio::test]
async fn reconnect_to_mqtt_on_message() {
    // init logger and env variables
    let env: Env = init();

    // init AMQP client
    let mut amqp_client = AmqpClient::new(env.amqp_uri.clone(), env.amqp_queue_name.clone());
    amqp_client.connect_with_retry_loop().await;

    // init MQTT client
    let mqtt_config: MqttConfig = MqttConfig::new(&env);
    match MqttClient::new(MqttOptions::new(&mqtt_config)) {
        Ok(mut mqtt_client) => {
            // connect to MQTT server and subscribe to topics
            mqtt_client.connect().await;
            if let Err(err) = mqtt_client.subscribe(TOPICS).await {
                error!(target: "app", "MQTT cannot subscribe to TOPICS, err = {:?}", err);
                panic!("unknown error, because MQTT cannot subscribe to TOPICS");
            }
            // disconnect from MQTT server
            let _ = mqtt_client.disconnect().await;

            // send MQTT message via AMQP
            // this will automatically trigger a `reconnect()`
            let result = process_mqtt_message(&None, &mut mqtt_client, &mut amqp_client).await;

            // check result: it should return () if `process_mqtt_message`
            // successfully reconnected to MQTT server
            assert_eq!(result.unwrap(), ());
        }
        Err(err) => {
            error!(target: "app", "Error creating MQTT client: {:?}", err);
            panic!("unknown error, cannot create MQTT client");
        }
    }
}

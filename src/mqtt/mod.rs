pub mod mqtt_client;
pub mod mqtt_config;
pub mod mqtt_options;

use log::{debug, error};
use std::string::String;

use paho_mqtt::Message;
use thiserror::Error;

use crate::models::get_msg_byte;
use crate::models::topic::Topic;

// custom error, based on 'thiserror' library
#[derive(Error, Debug)]
pub enum MqttError {
    #[error("file {0} not found error")]
    FileNotFound(String),
}

const COMBINED_CA_FILES_PATH: &str = "./rootca_and_cert.pem";

pub fn get_bytes_from_payload(msg: &Message) -> Vec<u8> {
    let payload: String = get_string_payload(msg);
    let topic: Topic = Topic::new(msg.topic());
    debug!(target: "app", "get_bytes_from_payload - MQTT message topic = {}", &topic);
    let msg_byte: Vec<u8> = get_msg_byte(&topic, &payload);
    msg_byte
}

fn get_string_payload(msg: &Message) -> String {
    match std::str::from_utf8(msg.payload()) {
        Ok(res) => {
            debug!(target: "app", "get_string_payload - MQTT utf8 payload_str: {}", res);
            res.to_string()
        }
        Err(err) => {
            error!(target: "app", "get_string_payload - Cannot read MQTT message payload as utf8. Error = {:?}", err);
            "".to_string()
        }
    }
}

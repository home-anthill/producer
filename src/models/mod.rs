use log::{debug, error};
use serde::{Deserialize, Serialize};

use crate::models::message::Message;
use crate::models::notification::Notification;
use crate::models::payload_trait::{
    AirPressure, AirQuality, Humidity, Light, Motion, PayloadTrait, PowerOutage, Temperature,
};
use crate::models::topic::Topic;

pub mod message;
pub mod notification;
pub mod payload_trait;
pub mod topic;

pub fn get_msg_byte(topic: &Topic, payload_str: &str) -> Vec<u8> {
    debug!(target: "app", "payload_str: {}", payload_str);
    let msg_byte: Vec<u8> = match topic.feature.as_str() {
        "temperature" => message_payload_to_bytes::<Temperature>(payload_str, topic),
        "humidity" => message_payload_to_bytes::<Humidity>(payload_str, topic),
        "light" => message_payload_to_bytes::<Light>(payload_str, topic),
        "motion" => message_payload_to_bytes::<Motion>(payload_str, topic),
        "airquality" => message_payload_to_bytes::<AirQuality>(payload_str, topic),
        "airpressure" => message_payload_to_bytes::<AirPressure>(payload_str, topic),
        "poweroutage" => message_payload_to_bytes::<PowerOutage>(payload_str, topic),
        _ => vec![],
    };
    msg_byte
}

fn message_payload_to_bytes<'a, T>(payload_str: &'a str, topic: &Topic) -> Vec<u8>
where
    T: Deserialize<'a> + Serialize + Clone + PayloadTrait + Sized,
{
    // deserialize to a Notification (with turbofish operator "::<Notification>")
    let parsed_result = serde_json::from_str::<Notification<T>>(payload_str);
    match parsed_result {
        Ok(val) => {
            debug!(target: "app", "message_payload_to_bytes - parsed from JSON string, returning as byte array");
            let serialized =
                Message::<T>::new_as_json(val.uuid.clone(), val.api_token.clone(), topic.clone(), val.payload);
            serialized.into_bytes()
        }
        Err(err) => {
            error!(target: "app", "message_payload_to_bytes - cannot parse JSON from string, returning empty data. Err = {:?}", &err);
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::init;
    use crate::models::get_msg_byte;
    use crate::models::topic::Topic;
    use log::debug;
    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde_json::json;
    use std::str::from_utf8;

    fn get_expected_json_string<T: Serialize>(uuid: &str, value: T, topic: &Topic) -> String {
        let api_token = "473a4861-632b-4915-b01e-cf1d418966c6";
        json!({
            "uuid": uuid,
            "apiToken": api_token,
            "topic": {
                "family": topic.family,
                "deviceId": topic.device_id,
                "feature": topic.feature
            },
            "payload": {
                "value": value
            }
        })
        .to_string()
    }

    #[test]
    fn ok_get_msg_byte_sensors() {
        // init logger and env
        let _ = init();

        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        const FLOAT_SENSORS: &[&str] = &["temperature", "humidity", "light", "airpressure"];
        const INT_SENSORS: &[&str] = &["motion", "airquality"];
        const VALUE_FLOAT: f64 = 12.0;
        const VALUE_INT: i64 = 1;

        for sensor_type in FLOAT_SENSORS.iter() {
            let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str());
            let expected_value = get_expected_json_string::<f64>(uuid, VALUE_FLOAT, &topic);

            let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, expected_value.as_str());
            let result = from_utf8(msg_byte_arr.as_slice()).unwrap();

            debug!(target: "app", "result = {}", result);
            debug!(target: "app", "expected_value = {}", expected_value);
            assert_eq!(result.to_string(), expected_value);
        }

        for sensor_type in INT_SENSORS.iter() {
            let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str());
            let expected_value = get_expected_json_string::<i64>(uuid, VALUE_INT, &topic);

            let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, expected_value.as_str());
            let result = from_utf8(msg_byte_arr.as_slice()).unwrap();

            debug!(target: "app", "result = {}", result);
            debug!(target: "app", "expected_value = {}", expected_value);
            assert_eq!(result.to_string(), expected_value);
        }

        // unknown sensor type
        let topic: Topic = Topic::new(format!("sensors/{}/unknown", uuid).as_str());
        let expected_value = get_expected_json_string::<i64>(uuid, VALUE_INT, &topic);
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, expected_value.as_str());
        assert_eq!(msg_byte_arr.len(), 0);
    }

    #[test]
    fn wrong_get_msg_byte_unknown_sensor() {
        // init logger and env
        let _ = init();

        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        // unknown sensor type
        let topic: Topic = Topic::new(format!("sensors/{}/unknown_type", uuid).as_str());
        debug!(target: "app", "Topic = {}", &topic);
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, get_expected_json_string::<i64>(uuid, 1, &topic).as_str());
        // for unknown sensor type, get_msg_byte returns an empty Vec<u8>
        assert_eq!(msg_byte_arr.len(), 0);
    }

    #[test]
    fn wrong_get_msg_byte_bad_json_message() {
        // init logger and env
        let _ = init();

        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", uuid).as_str());
        // create a message with a bad JSON payload
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, "{\"uuid\": \"1234\", 12}");
        // for bad JSON payloads, get_msg_byte returns an empty Vec<u8>
        assert_eq!(msg_byte_arr.len(), 0);
    }

    #[test]
    fn wrong_get_msg_byte_bad_value_format() {
        // init logger and env
        let _ = init();

        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";

        let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, "motion").as_str());
        // create a message with an int value, instead of a float as required by 'temperature'
        let expected_value = get_expected_json_string::<f64>(uuid, 5.0, &topic);
        let msg_byte_arr: Vec<u8> = get_msg_byte(&topic, expected_value.as_str());
        let result = from_utf8(msg_byte_arr.as_slice()).unwrap();

        debug!(target: "app", "result = {}", result);
        // for bad JSON payloads, get_msg_byte returns an empty Vec<u8>
        assert_eq!(msg_byte_arr.len(), 0);
    }
}

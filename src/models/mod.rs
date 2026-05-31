use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::{debug, error};

use crate::models::message::Message;
use crate::models::notification::Notification;
use crate::models::payload_trait::{
    AirPressure, AirQuality, Humidity, Light, Motion, Online, PayloadTrait, Temperature,
};
use crate::models::topic::Topic;

pub mod message;
pub mod notification;
pub mod payload_trait;
pub mod topic;

const SIGNED_NONCE_HEX_LEN: usize = 32;
const SIGNED_SIGNATURE_HEX_LEN: usize = 64;

pub(crate) fn is_valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|u| !u.is_nil())
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn has_valid_signed_envelope(timestamp: i64, nonce: &str, signature: &str) -> bool {
    timestamp > 0 && is_lower_hex(nonce, SIGNED_NONCE_HEX_LEN) && is_lower_hex(signature, SIGNED_SIGNATURE_HEX_LEN)
}

pub fn get_msg_byte(topic: &Topic, payload_str: &str) -> Option<Vec<u8>> {
    match topic.feature_name.as_str() {
        "temperature" => message_payload_to_bytes::<Temperature>(payload_str, topic),
        "humidity" => message_payload_to_bytes::<Humidity>(payload_str, topic),
        "light" => message_payload_to_bytes::<Light>(payload_str, topic),
        "motion" => message_payload_to_bytes::<Motion>(payload_str, topic),
        "airquality" => message_payload_to_bytes::<AirQuality>(payload_str, topic),
        "airpressure" => message_payload_to_bytes::<AirPressure>(payload_str, topic),
        "online" => message_payload_to_bytes::<Online>(payload_str, topic),
        _ => None,
    }
}

fn message_payload_to_bytes<T>(payload_str: &str, topic: &Topic) -> Option<Vec<u8>>
where
    T: DeserializeOwned + Serialize + Clone + PayloadTrait,
{
    let parsed_result = serde_json::from_str::<Notification<T>>(payload_str);
    match parsed_result {
        Ok(val) => {
            if !is_valid_uuid(&val.device_uuid) || !is_valid_uuid(&val.feature_uuid) {
                error!(target: "app", "message_payload_to_bytes - invalid UUID in payload fields, dropping message");
                return None;
            }
            if !has_valid_signed_envelope(val.timestamp, &val.nonce, &val.signature) {
                error!(target: "app", "message_payload_to_bytes - invalid signed envelope fields, dropping message");
                return None;
            }
            debug!(target: "app", "message_payload_to_bytes - parsed from JSON string, returning as byte array");
            match Message::<T>::new_as_json(
                val.device_uuid,
                val.feature_uuid,
                val.timestamp,
                val.nonce,
                val.signature,
                topic.clone(),
                val.payload,
            ) {
                Ok(serialized) => Some(serialized.into_bytes()),
                Err(err) => {
                    error!(target: "app", "message_payload_to_bytes - cannot serialize message to JSON. Err = {:?}", &err);
                    None
                }
            }
        }
        Err(err) => {
            error!(target: "app", "message_payload_to_bytes - cannot parse JSON from string. Err = {:?}", &err);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::get_msg_byte;
    use crate::models::topic::Topic;
    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde_json::json;
    use std::str::from_utf8;
    use tracing::debug;

    fn get_expected_json_string<T: Serialize>(
        device_uuid: &str,
        feature_uuid: &str,
        value: T,
        topic: &Topic,
    ) -> String {
        json!({
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "timestamp": 1777630000i64,
            "nonce": "00112233445566778899aabbccddeeff",
            "signature": "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            "topic": {
                "family": topic.family,
                "deviceId": topic.device_id,
                "featureName": topic.feature_name,
            },
            "payload": {
                "value": value
            }
        })
        .to_string()
    }

    #[test]
    fn ok_get_msg_byte_sensors() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        const FLOAT_SENSORS: &[&str] = &["temperature", "humidity", "light", "airpressure"];
        const INT_SENSORS: &[&str] = &["motion", "airquality"];
        const VALUE_FLOAT: f64 = 12.0;
        const VALUE_INT: i64 = 1;

        for sensor_type in FLOAT_SENSORS {
            let topic: Topic = Topic::new(format!("sensors/{}/{}", device_uuid, sensor_type).as_str()).unwrap();
            let expected_value = get_expected_json_string::<f64>(device_uuid, feature_uuid, VALUE_FLOAT, &topic);

            let bytes = get_msg_byte(&topic, expected_value.as_str()).expect("expected Some bytes");
            let result = from_utf8(&bytes).unwrap();

            debug!(target: "app", "result = {}", result);
            debug!(target: "app", "expected_value = {}", expected_value);
            assert_eq!(result.to_string(), expected_value);
        }

        for sensor_type in INT_SENSORS {
            let topic: Topic = Topic::new(format!("sensors/{}/{}", device_uuid, sensor_type).as_str()).unwrap();
            let expected_value = get_expected_json_string::<i64>(device_uuid, feature_uuid, VALUE_INT, &topic);

            let bytes = get_msg_byte(&topic, expected_value.as_str()).expect("expected Some bytes");
            let result = from_utf8(&bytes).unwrap();

            debug!(target: "app", "result = {}", result);
            debug!(target: "app", "expected_value = {}", expected_value);
            assert_eq!(result.to_string(), expected_value);
        }
    }

    #[test]
    fn wrong_get_msg_byte_unknown_sensor() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        // H5: Topic::new now rejects unknown feature names at construction time.
        let topic_result = Topic::new(format!("sensors/{}/unknown_type", device_uuid).as_str());
        assert!(
            topic_result.is_err(),
            "expected Topic::new to reject unknown feature name"
        );
    }

    #[test]
    fn wrong_get_msg_byte_bad_json_message() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", device_uuid).as_str()).unwrap();
        assert!(get_msg_byte(&topic, "{\"deviceUuid\": \"1234\", 12}").is_none());
    }

    #[test]
    fn wrong_get_msg_byte_bad_value_format() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let topic: Topic = Topic::new(format!("sensors/{}/motion", device_uuid).as_str()).unwrap();
        // float value for a motion (integer) sensor → parse fails → None
        let expected_value = get_expected_json_string::<f64>(device_uuid, feature_uuid, 5.0, &topic);
        assert!(get_msg_byte(&topic, expected_value.as_str()).is_none());
    }

    #[test]
    fn wrong_get_msg_byte_bad_signed_nonce() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", device_uuid).as_str()).unwrap();
        let value = json!({
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "timestamp": 1777630000i64,
            "nonce": "00112233-4455-6677-8899-aabbccddeeff",
            "signature": "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            "payload": {
                "value": 21.0
            }
        })
        .to_string();

        assert!(get_msg_byte(&topic, value.as_str()).is_none());
    }

    #[test]
    fn wrong_get_msg_byte_bad_signed_signature() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", device_uuid).as_str()).unwrap();
        let value = json!({
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "timestamp": 1777630000i64,
            "nonce": "00112233445566778899aabbccddeeff",
            "signature": "not-hex",
            "payload": {
                "value": 21.0
            }
        })
        .to_string();

        assert!(get_msg_byte(&topic, value.as_str()).is_none());
    }

    #[test]
    fn wrong_get_msg_byte_bad_payload_uuid_fields() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", device_uuid).as_str()).unwrap();
        let cases = [
            ("not-a-uuid", feature_uuid),
            ("00000000-0000-0000-0000-000000000000", feature_uuid),
            (device_uuid, "not-a-uuid"),
            (device_uuid, "00000000-0000-0000-0000-000000000000"),
        ];

        for (payload_device_uuid, payload_feature_uuid) in cases {
            let value = json!({
                "deviceUuid": payload_device_uuid,
                "featureUuid": payload_feature_uuid,
                "timestamp": 1777630000i64,
                "nonce": "00112233445566778899aabbccddeeff",
                "signature": "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
                "payload": {
                    "value": 21.0
                }
            })
            .to_string();

            assert!(get_msg_byte(&topic, value.as_str()).is_none());
        }
    }

    #[test]
    fn wrong_get_msg_byte_bad_timestamp() {
        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let topic: Topic = Topic::new(format!("sensors/{}/temperature", device_uuid).as_str()).unwrap();
        let value = json!({
            "deviceUuid": device_uuid,
            "featureUuid": feature_uuid,
            "timestamp": 0,
            "nonce": "00112233445566778899aabbccddeeff",
            "signature": "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            "payload": {
                "value": 21.0
            }
        })
        .to_string();

        assert!(get_msg_byte(&topic, value.as_str()).is_none());
    }
}

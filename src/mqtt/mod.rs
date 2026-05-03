use paho_mqtt::Message;
use tracing::{debug, error};

use crate::models::get_msg_byte;
use crate::models::topic::Topic;

pub mod mqtt_client;
pub mod mqtt_config;
pub mod mqtt_options;

const COMBINED_CA_FILES_PATH: &str = "rootca_and_cert.pem";
const MAX_PAYLOAD_BYTES: usize = 65_536;

pub fn get_bytes_from_payload(msg: &Message) -> Option<Vec<u8>> {
    if msg.payload().len() > MAX_PAYLOAD_BYTES {
        error!(target: "app", "get_bytes_from_payload - payload too large ({} bytes), dropping message", msg.payload().len());
        return None;
    }
    let payload = get_string_payload(msg)?;
    let topic = Topic::new(msg.topic())
        .inspect_err(|err| {
            error!(target: "app", "get_bytes_from_payload - invalid MQTT topic: {}", err);
        })
        .ok()?;
    debug!(target: "app", "get_bytes_from_payload - MQTT message topic = {}", &topic);
    get_msg_byte(&topic, &payload)
}

fn get_string_payload(msg: &Message) -> Option<String> {
    std::str::from_utf8(msg.payload())
        .inspect_err(|err| {
            error!(target: "app", "get_string_payload - Cannot read MQTT message payload as utf8. Error = {:?}", err);
        })
        .ok()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use crate::config::init;
    use crate::models::get_msg_byte;
    use crate::models::topic::Topic;
    use crate::mqtt::get_bytes_from_payload;
    use paho_mqtt::Message;
    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde_json::json;
    use std::str::from_utf8;

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
    fn ok_get_bytes_from_payload() {
        let _ = init();

        let device_uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let feature_uuid = "41cb3f47-894c-45e9-90d9-a4d4de903896";
        let sensor_type = "temperature";
        let value = 12.23;
        let topic: Topic = Topic::new(format!("sensors/{}/{}", device_uuid, sensor_type).as_str()).unwrap();
        let msg_payload = format!(
            r#"{{"deviceUuid":"{device_uuid}", "featureUuid":"{feature_uuid}", "timestamp":1777630000, "nonce":"00112233445566778899aabbccddeeff", "signature":"aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899","payload":{{"value":{value}}}}}"#
        );
        let msg_byte_arr = get_msg_byte(&topic, msg_payload.as_str()).expect("expected Some bytes");
        let message = Message::new(format!("sensors/{}/{}", device_uuid, sensor_type), msg_byte_arr, 0);

        let bytes = get_bytes_from_payload(&message).expect("expected Some bytes");
        let result = from_utf8(bytes.as_slice()).unwrap();
        let expected_value = get_expected_json_string::<f64>(device_uuid, feature_uuid, value, &topic);
        assert_eq!(result.to_string(), expected_value);
    }
}

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::models::is_valid_uuid;

const KNOWN_FEATURES: &[&str] = &[
    "temperature",
    "humidity",
    "light",
    "airpressure",
    "motion",
    "airquality",
    "mode",
    "online",
];
const MAX_SEGMENT_BYTES: usize = 255;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub family: String,
    pub device_id: String,
    pub feature_name: String,
}

impl Topic {
    pub fn new(topic: &str) -> Result<Self, String> {
        let items: Vec<&str> = topic.split('/').collect();
        if items.len() != 3 {
            return Err(format!("expected 3 segments in topic '{}', got {}", topic, items.len()));
        }
        if items.iter().any(|s| s.is_empty()) {
            return Err(format!("topic '{}' contains empty segments", topic));
        }
        // H5: length cap per segment
        if items.iter().any(|s| s.len() > MAX_SEGMENT_BYTES) {
            return Err(format!(
                "topic '{}' contains a segment longer than {} bytes",
                topic, MAX_SEGMENT_BYTES
            ));
        }
        // H5: device_id must be a valid UUID
        if !is_valid_uuid(items[1]) {
            return Err(format!(
                "topic '{}' device_id '{}' is not a valid UUID",
                topic, items[1]
            ));
        }
        // H5: feature_name must be a known sensor type
        if !KNOWN_FEATURES.contains(&items[2]) {
            return Err(format!(
                "topic '{}' feature_name '{}' is not a known sensor type",
                topic, items[2]
            ));
        }
        Ok(Self {
            family: items[0].to_string(),
            device_id: items[1].to_string(),
            feature_name: items[2].to_string(),
        })
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}/{}", self.family, self.device_id, self.feature_name)
    }
}

#[cfg(test)]
mod tests {
    use crate::models::topic::Topic;
    use pretty_assertions::assert_eq;

    #[test]
    fn check_topic_display() {
        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let sensor_type = "temperature";

        let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str()).unwrap();
        let expected = topic.to_string();
        assert_eq!(format!("sensors/{}/{}", uuid, sensor_type), expected);
    }

    #[test]
    fn rejects_invalid_topics() {
        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let long_segment = "a".repeat(256);
        let cases = [
            ("sensors/temperature".to_string(), "expected 3 segments"),
            ("sensors//temperature".to_string(), "contains empty segments"),
            (
                format!("sensors/{}/{}", uuid, long_segment),
                "contains a segment longer than 255 bytes",
            ),
            ("sensors/not-a-uuid/temperature".to_string(), "is not a valid UUID"),
            (
                "sensors/00000000-0000-0000-0000-000000000000/temperature".to_string(),
                "is not a valid UUID",
            ),
            (format!("sensors/{}/unknown", uuid), "is not a known sensor type"),
        ];

        for (topic, expected_error) in cases {
            let err = Topic::new(&topic).expect_err("topic should be rejected");
            assert!(
                err.contains(expected_error),
                "expected '{err}' to contain '{expected_error}'"
            );
        }
    }
}

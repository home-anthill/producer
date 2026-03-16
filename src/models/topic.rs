use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[test_log::test]
    fn check_topic_display() {
        let uuid = "246e3256-f0dd-4fcb-82c5-ee20c2267eeb";
        let sensor_type = "temperature";

        let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str()).unwrap();
        let expected = topic.to_string();
        assert_eq!(format!("sensors/{}/{}", uuid, sensor_type), expected);
    }
}

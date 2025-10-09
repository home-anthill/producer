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
    pub fn new(topic: &str) -> Self {
        let items: Vec<&str> = topic.split('/').collect();
        Self {
            family: items.first().unwrap().to_string(),
            device_id: items.get(1).unwrap().to_string(),
            feature_name: items.last().unwrap().to_string(),
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(self.family.as_str())?;
        fmt.write_str("/")?;
        fmt.write_str(self.device_id.as_str())?;
        fmt.write_str("/")?;
        fmt.write_str(self.feature_name.as_str())?;
        Ok(())
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

        let topic: Topic = Topic::new(format!("sensors/{}/{}", uuid, sensor_type).as_str());
        let expected = topic.to_string();
        assert_eq!(format!("sensors/{}/{}", uuid, sensor_type), expected);
    }
}

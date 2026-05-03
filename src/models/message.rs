use serde::Serialize;

use crate::models::payload_trait::PayloadTrait;
use crate::models::topic::Topic;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Message<T>
where
    T: PayloadTrait + Serialize,
{
    pub device_uuid: String,
    pub feature_uuid: String,
    pub timestamp: i64,
    pub nonce: String,
    pub signature: String,
    pub topic: Topic,
    pub payload: T,
}

impl<T> Message<T>
where
    T: PayloadTrait + Serialize,
{
    fn new(
        device_uuid: String,
        feature_uuid: String,
        timestamp: i64,
        nonce: String,
        signature: String,
        topic: Topic,
        payload: T,
    ) -> Message<T> {
        Self {
            device_uuid,
            feature_uuid,
            timestamp,
            nonce,
            signature,
            topic,
            payload,
        }
    }

    pub fn new_as_json(
        device_uuid: String,
        feature_uuid: String,
        timestamp: i64,
        nonce: String,
        signature: String,
        topic: Topic,
        payload: T,
    ) -> Result<String, serde_json::Error> {
        let message = Self::new(device_uuid, feature_uuid, timestamp, nonce, signature, topic, payload);
        serde_json::to_string(&message)
    }
}

use serde::Deserialize;

use crate::models::payload_trait::PayloadTrait;

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notification<T: PayloadTrait> {
    pub device_uuid: String,
    pub feature_uuid: String,
    pub api_token: String,
    pub payload: T,
}

use serde::{Deserialize, Deserializer, Serialize};

pub trait PayloadTrait {}

fn deserialize_finite_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(d)?;
    if !v.is_finite() {
        return Err(serde::de::Error::custom(format!("value must be finite, got {v}")));
    }
    Ok(v)
}

macro_rules! payload_type {
    ($name:ident, f64) => {
        #[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
        pub struct $name {
            #[serde(deserialize_with = "deserialize_finite_f64")]
            pub value: f64,
        }
        impl PayloadTrait for $name {}
    };
    ($name:ident, $ty:ty) => {
        #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
        pub struct $name {
            pub value: $ty,
        }
        impl PayloadTrait for $name {}
    };
}

payload_type!(Temperature, f64);
payload_type!(Humidity, f64);
payload_type!(Light, f64);
payload_type!(AirPressure, f64);
payload_type!(Motion, i64);
payload_type!(AirQuality, i64);
payload_type!(Mode, i64);
payload_type!(Online, bool);

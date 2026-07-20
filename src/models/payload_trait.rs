use serde::{Deserialize, Deserializer, Serialize};

pub trait PayloadTrait {}

fn deserialize_finite_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(d)?;
    if !v.is_finite() {
        return Err(serde::de::Error::custom(format!("value must be finite, got {v}")));
    }
    Ok(v)
}

fn deserialize_mode<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let value = deserialize_finite_f64(d)?;
    if ![-1.0, 0.0, 1.0, 2.0].contains(&value) {
        return Err(serde::de::Error::custom(format!(
            "mode must be one of -1.0, 0.0, 1.0, or 2.0; got {value}"
        )));
    }
    Ok(value)
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Mode {
    #[serde(deserialize_with = "deserialize_mode")]
    pub value: f64,
}
impl PayloadTrait for Mode {}

payload_type!(Online, bool);

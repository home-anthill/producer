use serde::{Deserialize, Serialize};

pub trait PayloadTrait {}

macro_rules! payload_type {
    ($name:ident, $ty:ty) => {
        #[derive(Debug, Serialize, Deserialize, Clone)]
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
payload_type!(Online, i64);

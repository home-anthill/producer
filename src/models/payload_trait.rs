use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Temperature {
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Humidity {
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Light {
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AirPressure {
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Motion {
    pub value: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AirQuality {
    pub value: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Online {
    pub value: i64,
}

pub trait PayloadTrait {}

impl PayloadTrait for Temperature {}

impl PayloadTrait for Humidity {}

impl PayloadTrait for Light {}

impl PayloadTrait for AirPressure {}

impl PayloadTrait for Motion {}

impl PayloadTrait for AirQuality {}

impl PayloadTrait for Online {}

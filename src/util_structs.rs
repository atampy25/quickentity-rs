#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZRuntimeResourceIDPropertyValue {
	pub m_IDLow: u32,
	pub m_IDHigh: u32
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SMatrix43PropertyValue {
	pub XAxis: Vector3,
	pub YAxis: Vector3,
	pub ZAxis: Vector3,
	pub Trans: Vector3
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vector3 {
	pub x: f64,
	pub y: f64,
	pub z: f64
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZGuidPropertyValue {
	pub _a: u32,
	pub _b: u16,
	pub _c: u16,
	pub _d: u8,
	pub _e: u8,
	pub _f: u8,
	pub _g: u8,
	pub _h: u8,
	pub _i: u8,
	pub _j: u8,
	pub _k: u8
}

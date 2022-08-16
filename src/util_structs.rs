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
	pub _b: u32,
	pub _c: u32,
	pub _d: u32,
	pub _e: u32,
	pub _f: u32,
	pub _g: u32,
	pub _h: u32,
	pub _i: u32,
	pub _j: u32,
	pub _k: u32
}

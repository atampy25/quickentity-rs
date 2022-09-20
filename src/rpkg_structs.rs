use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceMeta {
	pub hash_offset: u64,
	pub hash_reference_data: Vec<ResourceDependency>,
	pub hash_reference_table_dummy: u64,
	pub hash_reference_table_size: u64,
	pub hash_resource_type: String,
	pub hash_size: u64,
	pub hash_size_final: u64,
	pub hash_size_in_memory: u64,
	pub hash_size_in_video_memory: u64,
	pub hash_value: String
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceDependency {
	pub hash: String,
	pub flag: String
}

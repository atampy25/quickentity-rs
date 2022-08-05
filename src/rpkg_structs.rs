use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ResourceMeta {
    pub hash_offset: i32,
    pub hash_reference_data: Vec<ResourceDependency>,
    pub hash_reference_table_dummy: i32,
    pub hash_reference_table_size: i32,
    pub hash_resource_type: String,
    pub hash_size: i32,
    pub hash_size_final: i32,
    pub hash_size_in_memory: i32,
    pub hash_size_in_video_memory: i32,
    pub hash_value: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResourceDependency {
    pub hash: String,
    pub flag: String,
}

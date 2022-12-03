use serde::{Deserialize, Serialize};

use crate::rt_structs::{
	SEntityTemplateEntitySubset, SEntityTemplateProperty, SEntityTemplatePropertyAlias,
	SEntityTemplatePropertyOverride, SEntityTemplateReference
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct STemplateSubEntity {
	pub logical_parent: SEntityTemplateReference,
	pub entity_type_resource_index: usize,
	pub property_values: Vec<SEntityTemplateProperty>,
	pub post_init_property_values: Vec<SEntityTemplateProperty>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RTFactory2016 {
	pub sub_type: i8,
	pub blueprint_index_in_resource_header: i32,
	pub root_entity_index: usize,
	pub entity_templates: Vec<STemplateSubEntity>,
	pub property_overrides: Vec<SEntityTemplatePropertyOverride>,
	pub external_scene_type_indices_in_resource_header: Vec<usize>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct STemplateSubEntityBlueprint {
	pub logical_parent: SEntityTemplateReference,
	pub entity_type_resource_index: usize,
	pub entity_id: u64,
	pub entity_name: String,
	pub property_aliases: Vec<SEntityTemplatePropertyAlias>,
	pub exposed_entities: Vec<(String, SEntityTemplateReference)>,
	pub exposed_interfaces: Vec<(String, usize)>,
	pub entity_subsets: Vec<(String, SEntityTemplateEntitySubset)>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RTBlueprint2016 {
	pub sub_type: i8,
	pub root_entity_index: usize,
	pub entity_templates: Vec<STemplateSubEntityBlueprint>,
	pub external_scene_type_indices_in_resource_header: Vec<usize>,
	pub pin_connections: Vec<SEntityTemplatePinConnection2016>,
	pub input_pin_forwardings: Vec<SEntityTemplatePinConnection2016>,
	pub output_pin_forwardings: Vec<SEntityTemplatePinConnection2016>,
	pub override_deletes: Vec<SEntityTemplateReference>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplatePinConnection2016 {
	#[serde(rename = "fromID")]
	pub from_id: usize,

	#[serde(rename = "toID")]
	pub to_id: usize,

	pub from_pin_name: String,
	pub to_pin_name: String
}

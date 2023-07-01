use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct STemplateBlueprintSubEntity {
    pub logical_parent: SEntityTemplateReference,
    pub entity_type_resource_index: usize,

    #[serde(rename = "entityId")]
    pub entity_id: u64,

    pub editor_only: bool,
    pub entity_name: String,
    pub property_aliases: Vec<SEntityTemplatePropertyAlias>,
    pub exposed_entities: Vec<SEntityTemplateExposedEntity>,
    pub exposed_interfaces: Vec<(String, usize)>,
    pub entity_subsets: Vec<(String, SEntityTemplateEntitySubset)>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RTBlueprint {
    pub sub_type: i8,
    pub root_entity_index: usize,
    pub sub_entities: Vec<STemplateBlueprintSubEntity>,
    pub external_scene_type_indices_in_resource_header: Vec<usize>,
    pub pin_connections: Vec<SEntityTemplatePinConnection>,
    pub input_pin_forwardings: Vec<SEntityTemplatePinConnection>,
    pub output_pin_forwardings: Vec<SEntityTemplatePinConnection>,
    pub override_deletes: Vec<SEntityTemplateReference>,
    pub pin_connection_overrides: Vec<SExternalEntityTemplatePinConnection>,
    pub pin_connection_override_deletes: Vec<SExternalEntityTemplatePinConnection>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct STemplateFactorySubEntity {
    pub logical_parent: SEntityTemplateReference,
    pub entity_type_resource_index: usize,
    pub property_values: Vec<SEntityTemplateProperty>,
    pub post_init_property_values: Vec<SEntityTemplateProperty>,

    #[serde(default = "Vec::new")] // H2 does not have this property
    pub platform_specific_property_values: Vec<SEntityTemplatePlatformSpecificProperty>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RTFactory {
    pub sub_type: i8,
    pub blueprint_index_in_resource_header: i32,
    pub root_entity_index: usize,
    pub sub_entities: Vec<STemplateFactorySubEntity>,
    pub property_overrides: Vec<SEntityTemplatePropertyOverride>,
    pub external_scene_type_indices_in_resource_header: Vec<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplateReference {
    #[serde(rename = "entityID")]
    pub entity_id: u64,

    pub external_scene_index: i32,
    pub entity_index: i32,
    pub exposed_entity: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplateExposedEntity {
    pub s_name: String,
    pub b_is_array: bool,
    pub a_targets: Vec<SEntityTemplateReference>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplatePinConnection {
    #[serde(rename = "fromID")]
    pub from_id: usize,

    #[serde(rename = "toID")]
    pub to_id: usize,

    pub from_pin_name: String,
    pub to_pin_name: String,
    pub constant_pin_value: SEntityTemplatePropertyValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplatePlatformSpecificProperty {
    pub property_value: SEntityTemplateProperty,
    pub platform: String,
    pub post_init: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplatePropertyAlias {
    pub s_alias_name: String,

    #[serde(rename = "entityID")]
    pub entity_id: usize,

    pub s_property_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplatePropertyOverride {
    pub property_owner: SEntityTemplateReference,
    pub property_value: SEntityTemplateProperty,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SEntityTemplateEntitySubset {
    pub entities: Vec<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SExternalEntityTemplatePinConnection {
    pub from_entity: SEntityTemplateReference,
    pub to_entity: SEntityTemplateReference,
    pub from_pin_name: String,
    pub to_pin_name: String,
    pub constant_pin_value: SEntityTemplatePropertyValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SEntityTemplateProperty {
    #[serde(rename = "nPropertyID")]
    pub n_property_id: PropertyID,

    #[serde(rename = "value")]
    pub value: SEntityTemplatePropertyValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SEntityTemplatePropertyValue {
    #[serde(rename = "$type")]
    pub property_type: String,

    #[serde(rename = "$val")]
    pub property_value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum PropertyID {
    Int(u64),
    String(String),
}

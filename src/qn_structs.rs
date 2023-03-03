use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};

use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum SubType {
	Brick,
	Scene,
	Template
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export)]
pub struct Entity {
	/// The hash of the TEMP file of this entity.
	#[serde(rename = "tempHash")]
	pub factory_hash: String,

	/// The hash of the TBLU file of this entity.
	#[serde(rename = "tbluHash")]
	pub blueprint_hash: String,

	/// The root sub-entity of this entity.
	#[serde(rename = "rootEntity")]
	pub root_entity: String,

	/// The sub-entities of this entity.
	#[serde(rename = "entities")]
	#[ts(type = "Record<string, SubEntity>")]
	pub entities: LinkedHashMap<String, SubEntity>,

	/// Properties on other entities (local or external) to override when this entity is loaded.
	#[serde(rename = "propertyOverrides")]
	pub property_overrides: Vec<PropertyOverride>,

	/// Entities (external or local) to delete (including their organisational children) when
	/// this entity is loaded.
	#[serde(rename = "overrideDeletes")]
	pub override_deletes: Vec<Ref>,

	/// Pin (event) connections (between entities, external or local) to add when this entity is
	/// loaded.
	#[serde(rename = "pinConnectionOverrides")]
	pub pin_connection_overrides: Vec<PinConnectionOverride>,

	/// Pin (event) connections (between entities, external or local) to delete when this entity
	/// is loaded.
	#[serde(rename = "pinConnectionOverrideDeletes")]
	pub pin_connection_override_deletes: Vec<PinConnectionOverrideDelete>,

	/// The external scenes that this entity references.
	#[serde(rename = "externalScenes")]
	pub external_scenes: Vec<String>,

	/// The type of this entity.
	#[serde(rename = "subType")]
	pub sub_type: SubType,

	/// The QuickEntity format version of this entity.
	#[serde(rename = "quickEntityVersion")]
	pub quick_entity_version: f64,

	#[serde(rename = "extraFactoryDependencies")]
	pub extra_factory_dependencies: Vec<Dependency>,

	#[serde(rename = "extraBlueprintDependencies")]
	pub extra_blueprint_dependencies: Vec<Dependency>,

	#[serde(rename = "comments")]
	pub comments: Vec<CommentEntity>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct CommentEntity {
	/// The sub-entity this comment is parented to.
	pub parent: Ref,

	/// The name of this comment.
	pub name: String,

	/// The text this comment holds.
	pub text: String
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct SubEntity {
	/// The "logical" parent of the entity.
	#[serde(rename = "parent")]
	pub parent: Ref,

	/// The name of the entity.
	#[serde(rename = "name")]
	pub name: String,

	/// The factory of the entity.
	#[serde(rename = "factory")]
	#[serde(alias = "template")]
	pub factory: String,

	/// The factory's flag.
	#[serde(rename = "factoryFlag")]
	#[serde(alias = "templateFlag")]
	pub factory_flag: Option<String>,

	/// The blueprint of the entity.
	#[serde(rename = "blueprint")]
	pub blueprint: String,

	/// Whether the entity is only loaded in IO's editor.
	#[serde(rename = "editorOnly")]
	pub editor_only: Option<bool>,

	/// Properties of the entity.
	#[serde(rename = "properties")]
	#[ts(type = "Record<string, Property>")]
	pub properties: Option<LinkedHashMap<String, Property>>,

	/// Properties to apply conditionally to the entity based on platform.
	#[serde(rename = "platformSpecificProperties")]
	#[ts(type = "Record<string, Record<string, Property>>")]
	pub platform_specific_properties:
		Option<LinkedHashMap<String, LinkedHashMap<String, Property>>>,

	/// Inputs on entities to trigger when events occur.
	#[serde(rename = "events")]
	#[ts(type = "Record<string, Record<string, Array<RefMaybeConstantValue>>>")]
	pub events: Option<LinkedHashMap<String, LinkedHashMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Inputs on entities to trigger when this entity is given inputs.
	#[serde(rename = "inputCopying")]
	#[ts(type = "Record<string, Record<string, Array<RefMaybeConstantValue>>>")]
	pub input_copying:
		Option<LinkedHashMap<String, LinkedHashMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Events to propagate on other entities.
	#[serde(rename = "outputCopying")]
	#[ts(type = "Record<string, Record<string, Array<RefMaybeConstantValue>>>")]
	pub output_copying:
		Option<LinkedHashMap<String, LinkedHashMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Properties on other entities that can be accessed from this entity.
	#[serde(rename = "propertyAliases")]
	#[ts(type = "Record<string, PropertyAlias>")]
	pub property_aliases: Option<LinkedHashMap<String, Vec<PropertyAlias>>>,

	/// Entities that can be accessed from this entity.
	#[serde(rename = "exposedEntities")]
	#[ts(type = "Record<string, ExposedEntity>")]
	pub exposed_entities: Option<LinkedHashMap<String, ExposedEntity>>,

	/// Interfaces implemented by other entities that can be accessed from this entity.
	#[serde(rename = "exposedInterfaces")]
	#[ts(type = "Record<string, String>")]
	pub exposed_interfaces: Option<LinkedHashMap<String, String>>,

	/// The subsets that this entity belongs to.
	#[serde(rename = "subsets")]
	#[ts(type = "Record<string, Array<String>>")]
	pub subsets: Option<LinkedHashMap<String, Vec<String>>>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
#[serde(untagged)]
pub enum RefMaybeConstantValue {
	RefWithConstantValue(RefWithConstantValue),
	Ref(Ref)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct RefWithConstantValue {
	/// The entity to reference's ID.
	#[serde(rename = "ref")]
	pub entity_ref: Ref,

	/// The external scene the referenced entity resides in.
	#[serde(rename = "value")]
	pub value: SimpleProperty
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct Property {
	/// The type of the property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The value of the property.
	#[serde(rename = "value")]
	#[ts(type = "any")]
	pub value: serde_json::Value,

	/// Whether the property should be (presumably) loaded/set after the entity has been initialised.
	#[serde(rename = "postInit")]
	pub post_init: Option<bool>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct SimpleProperty {
	/// The type of the simple property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The simple property's value.
	#[serde(rename = "value")]
	#[ts(type = "any")]
	pub value: serde_json::Value
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct ExposedEntity {
	/// Whether there are multiple target entities.
	#[serde(rename = "isArray")]
	pub is_array: bool,

	/// The target entity (or entities) that will be accessed.
	#[serde(rename = "refersTo")]
	pub refers_to: Vec<Ref>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct PropertyAlias {
	/// The other entity's property that should be accessed from this entity.
	#[serde(rename = "originalProperty")]
	pub original_property: String,

	/// The other entity whose property will be accessed.
	#[serde(rename = "originalEntity")]
	pub original_entity: Ref
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct PinConnectionOverride {
	/// The entity that will trigger the input on the other entity.
	#[serde(rename = "fromEntity")]
	pub from_entity: Ref,

	/// The name of the event on the fromEntity that will trigger the input on the toEntity.
	#[serde(rename = "fromPin")]
	pub from_pin: String,

	/// The entity whose input will be triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will be triggered by the event on the
	/// fromEntity.
	#[serde(rename = "toPin")]
	pub to_pin: String,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	#[ts(type = "any")]
	pub value: Option<SimpleProperty>
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct PinConnectionOverrideDelete {
	/// The entity that triggers the input on the other entity.
	#[serde(rename = "fromEntity")]
	pub from_entity: Ref,

	/// The name of the event on the fromEntity that will no longer trigger the input on the
	/// toEntity.
	#[serde(rename = "fromPin")]
	pub from_pin: String,

	/// The entity whose input is triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will no longer be triggered by the event on
	/// the fromEntity.
	#[serde(rename = "toPin")]
	pub to_pin: String,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	pub value: Option<SimpleProperty>
}

/// A set of overrides for entity properties.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct PropertyOverride {
	/// An array of references to the entities to override the properties of.
	#[serde(rename = "entities")]
	pub entities: Vec<Ref>,

	/// A set of properties to override on the entities.
	#[serde(rename = "properties")]
	#[ts(type = "Record<string, OverriddenProperty>")]
	pub properties: LinkedHashMap<String, OverriddenProperty>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct OverriddenProperty {
	/// The type of the property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The value of the property.
	#[serde(rename = "value")]
	#[ts(type = "any")]
	pub value: serde_json::Value
}

/// A full reference.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct FullRef {
	/// The entity to reference's ID.
	#[serde(rename = "ref")]
	pub entity_ref: String,

	/// The external scene the referenced entity resides in.
	#[serde(rename = "externalScene")]
	pub external_scene: Option<String>,

	/// The sub-entity to reference that is exposed by the referenced entity.
	#[serde(rename = "exposedEntity")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub exposed_entity: Option<String>
}

/// A reference to an entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
#[serde(untagged)]
pub enum Ref {
	Full(FullRef),
	Short(Option<String>)
}

/// A dependency of an entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
#[serde(untagged)]
pub enum Dependency {
	Full(DependencyWithFlag),
	Short(String)
}

/// A dependency with a flag other than the default (1F).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS, Eq)]
#[ts(export)]
pub struct DependencyWithFlag {
	pub resource: String,
	pub flag: String
}

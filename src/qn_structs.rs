use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubType {
	Brick,
	Scene,
	Template
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
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
	pub entities: IndexMap<String, SubEntity>,

	/// Properties on other entities (local or external) to override when this entity is loaded.
	///
	/// Overriding a local entity would be a rather pointless maneuver given that you could just actually change it in the entity instead of using an override.
	#[serde(rename = "propertyOverrides")]
	pub property_overrides: Vec<PropertyOverride>,

	/// Entities (external or local) to delete (including their organisational children) when
	/// this entity is loaded.
	///
	/// Deleting a local entity would be a rather pointless maneuver given that you could just actually remove it from this entity instead of using an override.
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

	/// The QuickEntity format version of this entity. The current version is 3.1.
	#[serde(rename = "quickEntityVersion")]
	pub quick_entity_version: f64,

	/// Extra resource dependencies that should be added to the entity's factory when converted to the game's format.
	#[serde(rename = "extraFactoryDependencies")]
	pub extra_factory_dependencies: Vec<Dependency>,

	/// Extra resource dependencies that should be added to the entity's blueprint when converted to the game's format.
	#[serde(rename = "extraBlueprintDependencies")]
	pub extra_blueprint_dependencies: Vec<Dependency>,

	/// Comments to be attached to sub-entities.
	///
	/// Will be displayed in QuickEntity Editor as tree items with a sticky note icon.
	#[serde(rename = "comments")]
	pub comments: Vec<CommentEntity>
}

/// A comment entity.
///
/// Will be displayed in QuickEntity Editor as a tree item with a sticky note icon.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct CommentEntity {
	/// The sub-entity this comment is parented to.
	pub parent: Ref,

	/// The name of this comment.
	pub name: String,

	/// The text this comment holds.
	pub text: String
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SubEntity {
	/// The "logical" or "organisational" parent of the entity, used for tree organisation in graphical editors.
	///
	/// Has no effect on the entity in game.
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
	///
	/// You can leave this out if it's 1F.
	#[serde(rename = "factoryFlag")]
	#[serde(alias = "templateFlag")]
	pub factory_flag: Option<String>,

	/// The blueprint of the entity.
	#[serde(rename = "blueprint")]
	pub blueprint: String,

	/// Whether the entity is only loaded in IO's editor.
	///
	/// Setting this to true will remove the entity from the game as well as all of its organisational (but not coordinate) children.
	#[serde(rename = "editorOnly")]
	pub editor_only: Option<bool>,

	/// Properties of the entity.
	#[serde(rename = "properties")]
	pub properties: Option<IndexMap<String, Property>>,

	/// Properties to apply conditionally to the entity based on platform.
	#[serde(rename = "platformSpecificProperties")]
	pub platform_specific_properties: Option<IndexMap<String, IndexMap<String, Property>>>,

	/// Inputs on entities to trigger when events occur.
	#[serde(rename = "events")]
	pub events: Option<IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Inputs on entities to trigger when this entity is given inputs.
	#[serde(rename = "inputCopying")]
	pub input_copying: Option<IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Events to propagate on other entities.
	#[serde(rename = "outputCopying")]
	pub output_copying: Option<IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>>,

	/// Properties on other entities that can be accessed from this entity.
	#[serde(rename = "propertyAliases")]
	pub property_aliases: Option<IndexMap<String, Vec<PropertyAlias>>>,

	/// Entities that can be accessed from this entity.
	#[serde(rename = "exposedEntities")]
	pub exposed_entities: Option<IndexMap<String, ExposedEntity>>,

	/// Interfaces implemented by other entities that can be accessed from this entity.
	#[serde(rename = "exposedInterfaces")]
	pub exposed_interfaces: Option<IndexMap<String, String>>,

	/// The subsets that this entity belongs to.
	#[serde(rename = "subsets")]
	pub subsets: Option<IndexMap<String, Vec<String>>>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(untagged)]
pub enum RefMaybeConstantValue {
	RefWithConstantValue(RefWithConstantValue),
	Ref(Ref)
}

/// A reference accompanied by a constant value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct RefWithConstantValue {
	/// The entity to reference's ID.
	#[serde(rename = "ref")]
	pub entity_ref: Ref,

	/// The constant value accompanying this reference.
	#[serde(rename = "value")]
	pub value: SimpleProperty
}

/// A property with a type and a value. Can be marked as post-init.
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct Property {
	/// The type of the property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The value of the property.
	#[serde(rename = "value")]
	pub value: serde_json::Value,

	/// Whether the property should be (presumably) loaded/set after the entity has been initialised.
	#[serde(rename = "postInit")]
	pub post_init: Option<bool>
}

/// A simple property.
///
/// Simple properties cannot be marked as post-init. They are used by pin connection overrides, events and input/output copying.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SimpleProperty {
	/// The type of the simple property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The simple property's value.
	#[serde(rename = "value")]
	pub value: serde_json::Value
}

/// An exposed entity.
///
/// Exposed entities are accessible when referencing this entity through a property on long-form references.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct ExposedEntity {
	/// Whether there are multiple target entities.
	#[serde(rename = "isArray")]
	pub is_array: bool,

	/// The target entity (or entities) that will be accessed.
	#[serde(rename = "refersTo")]
	pub refers_to: Vec<Ref>
}

/// A property alias.
///
/// Property aliases are used to access properties of other entities through a single entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PropertyAlias {
	/// The other entity's property that should be accessed from this entity.
	#[serde(rename = "originalProperty")]
	pub original_property: String,

	/// The other entity whose property will be accessed.
	#[serde(rename = "originalEntity")]
	pub original_entity: Ref
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PinConnectionOverride {
	/// The entity that will trigger the input on the other entity.
	///
	/// If this references a local entity, you can simply use an event on the entity itself.
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
	pub value: Option<SimpleProperty>
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PropertyOverride {
	/// An array of references to the entities to override the properties of.
	#[serde(rename = "entities")]
	pub entities: Vec<Ref>,

	/// A set of properties to override on the entities.
	#[serde(rename = "properties")]
	pub properties: IndexMap<String, OverriddenProperty>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct OverriddenProperty {
	/// The type of the property.
	#[serde(rename = "type")]
	pub property_type: String,

	/// The value of the property.
	#[serde(rename = "value")]
	pub value: serde_json::Value
}

/// A long-form reference to an entity, allowing for the specification of external scenes and/or an exposed entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(untagged)]
pub enum Ref {
	Full(FullRef),

	/// A short-form reference represents either a local reference with no exposed entity or a null reference.
	Short(Option<String>)
}

/// A dependency of an entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(untagged)]
pub enum Dependency {
	Full(DependencyWithFlag),

	/// A dependency which is flagged as "1F".
	Short(String)
}

/// A dependency with a flag other than the default (1F).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct DependencyWithFlag {
	pub resource: String,
	pub flag: String
}

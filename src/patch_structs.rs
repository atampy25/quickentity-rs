use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::qn_structs::{
	CommentEntity, Dependency, ExposedEntity, OverriddenProperty, PinConnectionOverride, PinConnectionOverrideDelete,
	Property, PropertyAlias, PropertyOverride, Ref, RefMaybeConstantValue, SubEntity, SubType
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct Patch {
	/// The hash of the TEMP file of this entity.
	#[serde(rename = "tempHash")]
	pub factory_hash: String,

	/// The hash of the TBLU file of this entity.
	#[serde(rename = "tbluHash")]
	pub blueprint_hash: String,

	/// The patch operations to apply.
	pub patch: Vec<PatchOperation>,

	/// The patch version. The current version is 6.
	#[serde(rename = "patchVersion")]
	pub patch_version: u8
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub enum PatchOperation {
	SetRootEntity(String),
	SetSubType(SubType),

	AddEntity(String, Box<SubEntity>),
	RemoveEntityByID(String),
	SubEntityOperation(String, SubEntityOperation),

	/// Should no longer be emitted by patch generators.
	AddPropertyOverride(PropertyOverride),

	/// Should no longer be emitted by patch generators.
	RemovePropertyOverride(PropertyOverride),

	AddPropertyOverrideConnection(PropertyOverrideConnection),
	RemovePropertyOverrideConnection(PropertyOverrideConnection),

	AddOverrideDelete(Ref),
	RemoveOverrideDelete(Ref),

	AddPinConnectionOverride(PinConnectionOverride),
	RemovePinConnectionOverride(PinConnectionOverride),

	AddPinConnectionOverrideDelete(PinConnectionOverrideDelete),
	RemovePinConnectionOverrideDelete(PinConnectionOverrideDelete),

	AddExternalScene(String),
	RemoveExternalScene(String),

	AddExtraFactoryDependency(Dependency),
	RemoveExtraFactoryDependency(Dependency),

	AddExtraBlueprintDependency(Dependency),
	RemoveExtraBlueprintDependency(Dependency),

	AddComment(CommentEntity),
	RemoveComment(CommentEntity)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub enum SubEntityOperation {
	SetParent(Ref),
	SetName(String),
	SetFactory(String),
	SetFactoryFlag(Option<String>),
	SetBlueprint(String),
	SetEditorOnly(Option<bool>),

	AddProperty(String, Property),
	SetPropertyType(String, String),
	SetPropertyValue {
		property_name: String,

		value: Value
	},
	PatchArrayPropertyValue(String, Vec<ArrayPatchOperation>),
	SetPropertyPostInit(String, bool),
	RemovePropertyByName(String),

	AddPlatformSpecificProperty(String, String, Property),
	SetPlatformSpecificPropertyType(String, String, String),
	SetPlatformSpecificPropertyValue {
		platform: String,

		property_name: String,

		value: Value
	},
	PatchPlatformSpecificArrayPropertyValue(String, String, Vec<ArrayPatchOperation>),
	SetPlatformSpecificPropertyPostInit(String, String, bool),
	RemovePlatformSpecificPropertyByName(String, String),
	RemovePlatformSpecificPropertiesForPlatform(String),

	AddEventConnection(String, String, RefMaybeConstantValue),
	RemoveEventConnection(String, String, RefMaybeConstantValue),
	RemoveAllEventConnectionsForTrigger(String, String),
	RemoveAllEventConnectionsForEvent(String),

	AddInputCopyConnection(String, String, RefMaybeConstantValue),
	RemoveInputCopyConnection(String, String, RefMaybeConstantValue),
	RemoveAllInputCopyConnectionsForTrigger(String, String),
	RemoveAllInputCopyConnectionsForInput(String),

	AddOutputCopyConnection(String, String, RefMaybeConstantValue),
	RemoveOutputCopyConnection(String, String, RefMaybeConstantValue),
	RemoveAllOutputCopyConnectionsForPropagate(String, String),
	RemoveAllOutputCopyConnectionsForOutput(String),

	AddPropertyAliasConnection(String, PropertyAlias),
	RemovePropertyAlias(String),
	RemoveConnectionForPropertyAlias(String, PropertyAlias),

	SetExposedEntity(String, ExposedEntity),
	RemoveExposedEntity(String),

	SetExposedInterface(String, String),
	RemoveExposedInterface(String),

	AddSubset(String, String),
	RemoveSubset(String, String),
	RemoveAllSubsetsFor(String)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub enum ArrayPatchOperation {
	RemoveItemByValue(Value),
	AddItemAfter(Value, Value),
	AddItemBefore(Value, Value),
	AddItem(Value)
}

/// A single entity-property override.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PropertyOverrideConnection {
	/// A reference to an entity to override a property on.
	#[serde(rename = "entity")]
	pub entity: Ref,

	/// The property to override.
	#[serde(rename = "propertyName")]
	pub property_name: String,

	/// The overridden property.
	#[serde(rename = "propertyOverride")]
	pub property_override: OverriddenProperty
}

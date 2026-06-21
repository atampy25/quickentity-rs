use ecow::EcoString;
use glacier_commons::metadata::{ResourceReference, RuntimeID};
use serde::{Deserialize, Serialize};
use specta::Type;
use tryvial::try_fn;

use crate::{
	entity::{
		CommentEntity, EntityID, ExposedEntity, LocalPinConnection, PinConnection, PinConnectionOverride,
		PinConnectionOverrideDelete, Property, PropertyAlias, Ref, SubEntity, SubType
	},
	variant::Variant
};

#[cfg(feature = "rune")]
pub fn rune_module() -> Result<rune::Module, rune::ContextError> {
	let mut module = rune::Module::with_crate_item("quickentity_rs", ["patch"])?;

	module.ty::<Patch>()?;
	module.ty::<PatchOperation>()?;
	module.ty::<SubEntityOperation>()?;
	module.ty::<ArrayPatchOperation>()?;
	module.ty::<PropertyOverrideConnection>()?;

	Ok(module)
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Patch {
	/// The hash of the TEMP file of this entity.
	#[serde(rename = "factory")]
	pub factory: RuntimeID,

	/// The hash of the TBLU file of this entity.
	#[serde(rename = "blueprint")]
	pub blueprint: RuntimeID,

	/// The patch operations to apply.
	pub patch: Vec<PatchOperation>,

	/// The patch version. The current version is 7.
	#[serde(rename = "patchVersion")]
	#[serde(deserialize_with = "validate_patch_version")]
	pub patch_version: u8
}

#[try_fn]
fn validate_patch_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
	let version = u8::deserialize(deserializer)?;

	if version != 7 {
		return Err(serde::de::Error::invalid_value(
			serde::de::Unexpected::Unsigned(version as u64),
			&"version 7"
		));
	}

	version
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
pub enum PatchOperation {
	#[cfg_attr(feature = "rune", rune(constructor))]
	SetRootEntity(#[cfg_attr(feature = "rune", rune(get, set))] EntityID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetSubType(#[cfg_attr(feature = "rune", rune(get, set))] SubType),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddEntity(
		#[cfg_attr(feature = "rune", rune(get, set))] EntityID,
		#[cfg_attr(feature = "rune", rune(get, set, boxed))] Box<SubEntity>
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveEntity(#[cfg_attr(feature = "rune", rune(get, set))] EntityID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	PatchEntity(
		#[cfg_attr(feature = "rune", rune(get, set))] EntityID,
		#[cfg_attr(feature = "rune", rune(get, set))] SubEntityOperation
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddPropertyOverrideConnection(#[cfg_attr(feature = "rune", rune(get, set))] PropertyOverrideConnection),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemovePropertyOverrideConnection(#[cfg_attr(feature = "rune", rune(get, set))] PropertyOverrideConnection),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] Ref),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] Ref),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddPinConnectionOverride(#[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverride),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemovePinConnectionOverride(#[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverride),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddPinConnectionOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverrideDelete),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemovePinConnectionOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverrideDelete),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddExternalScene(#[cfg_attr(feature = "rune", rune(get, set))] RuntimeID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveExternalScene(#[cfg_attr(feature = "rune", rune(get, set))] RuntimeID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddExtraFactoryReference(#[cfg_attr(feature = "rune", rune(get, set))] ResourceReference),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveExtraFactoryReference(#[cfg_attr(feature = "rune", rune(get, set))] ResourceReference),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddExtraBlueprintReference(#[cfg_attr(feature = "rune", rune(get, set))] ResourceReference),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveExtraBlueprintReference(#[cfg_attr(feature = "rune", rune(get, set))] ResourceReference),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddComment(#[cfg_attr(feature = "rune", rune(get, set))] CommentEntity),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveComment(#[cfg_attr(feature = "rune", rune(get, set))] CommentEntity)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
pub enum SubEntityOperation {
	#[cfg_attr(feature = "rune", rune(constructor))]
	SetParent(#[cfg_attr(feature = "rune", rune(get, set))] Option<Ref>),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetName(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetFactory(#[cfg_attr(feature = "rune", rune(get, set))] ResourceReference),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetBlueprint(#[cfg_attr(feature = "rune", rune(get, set))] RuntimeID),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetEditorOnly(#[cfg_attr(feature = "rune", rune(get, set))] bool),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddProperty(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] Property
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	PatchPropertyValue(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] VariantPatch,
		/// Expected value of post-init, used only if the property doesn't exist and needs to be created
		#[cfg_attr(feature = "rune", rune(get, set))]
		bool
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetPropertyPostInit(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] bool
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveProperty(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddPlatformSpecificProperty(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] Property
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	PatchPlatformSpecificPropertyValue(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] VariantPatch,
		/// Expected value of post-init, used only if the property doesn't exist and needs to be created
		#[cfg_attr(feature = "rune", rune(get, set))]
		bool
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetPlatformSpecificPropertyPostInit(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] bool
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemovePlatformSpecificProperty(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddEventConnection(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] PinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveEventConnection(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] PinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddInputForwarding(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] LocalPinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveInputForwarding(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] LocalPinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddOutputForwarding(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] LocalPinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveOutputForwarding(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] LocalPinConnection
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddPropertyAlias(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] PropertyAlias
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemovePropertyAlias(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] PropertyAlias
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetExposedEntity(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] ExposedEntity
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveExposedEntity(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	SetExposedInterface(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] EntityID
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveExposedInterface(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	AddSubset(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] EntityID
	),

	#[cfg_attr(feature = "rune", rune(constructor))]
	RemoveSubset(
		#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
		#[specta(type = String)]
		EcoString,
		#[cfg_attr(feature = "rune", rune(get, set))] EntityID
	)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
pub enum VariantPatch {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Set(#[cfg_attr(feature = "rune", rune(get, set))] Variant),

	#[cfg_attr(feature = "rune", rune(constructor))]
	ArrayPatch(#[cfg_attr(feature = "rune", rune(get, set))] Vec<ArrayPatchOperation>)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
pub struct ItemSelector(
	#[cfg_attr(feature = "rune", rune(get, set))] pub Variant,
	#[cfg_attr(feature = "rune", rune(get, set))] pub usize
);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
pub enum ArrayPatchOperation {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Add {
		/// Preferred over `after`.
		#[cfg_attr(feature = "rune", rune(get, set))]
		before: Option<ItemSelector>,

		#[cfg_attr(feature = "rune", rune(get, set))]
		after: Option<ItemSelector>,

		#[cfg_attr(feature = "rune", rune(get, set))]
		item: Variant
	},

	#[cfg_attr(feature = "rune", rune(constructor))]
	Remove {
		#[cfg_attr(feature = "rune", rune(get, set))]
		item: ItemSelector
	},

	#[cfg_attr(feature = "rune", rune(constructor))]
	Replace {
		#[cfg_attr(feature = "rune", rune(get, set))]
		item: ItemSelector,

		#[cfg_attr(feature = "rune", rune(get, set))]
		new: Variant
	}
}

/// A single entity-property override.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct PropertyOverrideConnection {
	/// A reference to an entity to override a property on.
	pub entity: Ref,

	/// The property to override.
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub property: EcoString,

	/// The overridden property.
	pub value: Variant,

	pub runtime_editable: bool
}

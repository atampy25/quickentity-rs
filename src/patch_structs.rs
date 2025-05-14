use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::qn_structs::{
    CommentEntity, Dependency, ExposedEntity, OverriddenProperty, PinConnectionOverride,
    PinConnectionOverrideDelete, Property, PropertyAlias, PropertyOverride, Ref,
    RefMaybeConstantValue, SubEntity, SubType,
};

#[cfg(feature = "rune")]
pub fn rune_module() -> Result<rune::Module, rune::ContextError> {
    let mut module = rune::Module::with_crate_item("quickentity_rs", ["patch_structs"])?;

    module.ty::<Patch>()?;
    module.ty::<PatchOperation>()?;
    module.ty::<SubEntityOperation>()?;
    module.ty::<ArrayPatchOperation>()?;
    module.ty::<PropertyOverrideConnection>()?;

    Ok(module)
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
    pub patch_version: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
pub enum PatchOperation {
    #[cfg_attr(feature = "rune", rune(constructor))]
    SetRootEntity(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetSubType(#[cfg_attr(feature = "rune", rune(get, set))] SubType),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddEntity(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] SubEntity,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveEntityByID(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SubEntityOperation(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] SubEntityOperation,
    ),

    /// Should no longer be emitted by patch generators.
    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPropertyOverride(#[cfg_attr(feature = "rune", rune(get, set))] PropertyOverride),

    /// Should no longer be emitted by patch generators.
    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePropertyOverride(#[cfg_attr(feature = "rune", rune(get, set))] PropertyOverride),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPropertyOverrideConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] PropertyOverrideConnection,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePropertyOverrideConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] PropertyOverrideConnection,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] Ref),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveOverrideDelete(#[cfg_attr(feature = "rune", rune(get, set))] Ref),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPinConnectionOverride(#[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverride),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePinConnectionOverride(
        #[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverride,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPinConnectionOverrideDelete(
        #[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverrideDelete,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePinConnectionOverrideDelete(
        #[cfg_attr(feature = "rune", rune(get, set))] PinConnectionOverrideDelete,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddExternalScene(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveExternalScene(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddExtraFactoryDependency(#[cfg_attr(feature = "rune", rune(get, set))] Dependency),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveExtraFactoryDependency(#[cfg_attr(feature = "rune", rune(get, set))] Dependency),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddExtraBlueprintDependency(#[cfg_attr(feature = "rune", rune(get, set))] Dependency),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveExtraBlueprintDependency(#[cfg_attr(feature = "rune", rune(get, set))] Dependency),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddComment(#[cfg_attr(feature = "rune", rune(get, set))] CommentEntity),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveComment(#[cfg_attr(feature = "rune", rune(get, set))] CommentEntity),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
pub enum SubEntityOperation {
    #[cfg_attr(feature = "rune", rune(constructor))]
    SetParent(#[cfg_attr(feature = "rune", rune(get, set))] Ref),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetName(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetFactory(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetFactoryFlag(#[cfg_attr(feature = "rune", rune(get, set))] Option<String>),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetBlueprint(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetEditorOnly(#[cfg_attr(feature = "rune", rune(get, set))] Option<bool>),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddProperty(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] Property,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPropertyType(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPropertyValue(#[cfg_attr(feature = "rune", rune(get, set))] SetPropertyValue),

    #[cfg_attr(feature = "rune", rune(constructor))]
    PatchArrayPropertyValue(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] Vec<ArrayPatchOperation>,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPropertyPostInit(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] bool,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePropertyByName(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPlatformSpecificProperty(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] Property,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPlatformSpecificPropertyType(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPlatformSpecificPropertyValue(
        #[cfg_attr(feature = "rune", rune(get, set))] SetPlatformSpecificPropertyValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    PatchPlatformSpecificArrayPropertyValue(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] Vec<ArrayPatchOperation>,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetPlatformSpecificPropertyPostInit(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] bool,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePlatformSpecificPropertyByName(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePlatformSpecificPropertiesForPlatform(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddEventConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveEventConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllEventConnectionsForTrigger(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllEventConnectionsForEvent(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddInputCopyConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveInputCopyConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllInputCopyConnectionsForTrigger(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllInputCopyConnectionsForInput(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddOutputCopyConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveOutputCopyConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] RefMaybeConstantValue,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllOutputCopyConnectionsForPropagate(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllOutputCopyConnectionsForOutput(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddPropertyAliasConnection(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] PropertyAlias,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemovePropertyAlias(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveConnectionForPropertyAlias(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] PropertyAlias,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetExposedEntity(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] ExposedEntity,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveExposedEntity(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    SetExposedInterface(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveExposedInterface(#[cfg_attr(feature = "rune", rune(get, set))] String),

    #[cfg_attr(feature = "rune", rune(constructor))]
    AddSubset(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveSubset(
        #[cfg_attr(feature = "rune", rune(get, set))] String,
        #[cfg_attr(feature = "rune", rune(get, set))] String,
    ),

    #[cfg_attr(feature = "rune", rune(constructor))]
    RemoveAllSubsetsFor(#[cfg_attr(feature = "rune", rune(get, set))] String),
}

/// A property name and value to set on an entity.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SetPropertyValue {
    #[cfg_attr(feature = "rune", rune(get, set))]
    pub property_name: String,

    pub value: Value,
}

#[cfg(feature = "rune")]
impl SetPropertyValue {
    fn rune_construct(property_name: String, value: rune::Value) -> Self {
        Self {
            property_name,
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
        module.field_function(&rune::runtime::Protocol::GET, "value", |s: &Self| {
            serde_json::from_value::<rune::Value>(s.value.clone()).ok()
        })?;

        module.field_function(
            &rune::runtime::Protocol::SET,
            "value",
            |s: &mut Self, value: rune::Value| {
                s.value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
            },
        )?;

        Ok(())
    }
}

/// A platform, property name, and value to set on an entity.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SetPlatformSpecificPropertyValue {
    #[cfg_attr(feature = "rune", rune(get, set))]
    pub platform: String,

    #[cfg_attr(feature = "rune", rune(get, set))]
    pub property_name: String,

    pub value: Value,
}

#[cfg(feature = "rune")]
impl SetPlatformSpecificPropertyValue {
    fn rune_construct(platform: String, property_name: String, value: rune::Value) -> Self {
        Self {
            platform,
            property_name,
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
        module.field_function(&rune::runtime::Protocol::GET, "value", |s: &Self| {
            serde_json::from_value::<rune::Value>(s.value.clone()).ok()
        })?;

        module.field_function(
            &rune::runtime::Protocol::SET,
            "value",
            |s: &mut Self, value: rune::Value| {
                s.value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
            },
        )?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
pub enum ArrayPatchOperation {
    RemoveItemByValue(Value),
    AddItemAfter(Value, Value),
    AddItemBefore(Value, Value),
    AddItem(Value),
}

/// A single entity-property override.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::patch_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
    pub property_override: OverriddenProperty,
}

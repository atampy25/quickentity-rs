use std::{
	fmt::{Debug, Display, Formatter},
	num::ParseIntError,
	str::FromStr
};

use hitman_commons::metadata::{PathedID, ResourceReference};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(feature = "rune")]
pub fn rune_module() -> Result<rune::Module, rune::ContextError> {
	let mut module = rune::Module::with_crate_item("quickentity_rs", ["qn_structs"])?;

	module.ty::<EntityId>()?;
	module.ty::<SubType>()?;
	module.ty::<Entity>()?;
	module.ty::<CommentEntity>()?;
	module.ty::<SubEntity>()?;
	module.ty::<RefMaybeConstantValue>()?;
	module.ty::<RefWithConstantValue>()?;
	module.ty::<Property>()?;
	module.ty::<SimpleProperty>()?;
	module.ty::<ExposedEntity>()?;
	module.ty::<PropertyAlias>()?;
	module.ty::<PinConnectionOverride>()?;
	module.ty::<PinConnectionOverrideDelete>()?;
	module.ty::<PropertyOverride>()?;
	module.ty::<OverriddenProperty>()?;
	module.ty::<FullRef>()?;
	module.ty::<Ref>()?;

	Ok(module)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
pub enum SubType {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Brick,

	#[cfg_attr(feature = "rune", rune(constructor))]
	Scene,

	#[cfg_attr(feature = "rune", rune(constructor))]
	Template
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DISPLAY_FMT, DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(
	feature = "rune",
	rune_functions(Self::r_from_u64, Self::r_from_str, Self::as_u64__meta)
)]
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(u64);

impl EntityId {
	#[cfg(feature = "rune")]
	#[rune::function(path = Self::from_u64)]
	pub fn r_from_u64(value: u64) -> Self {
		Self(value)
	}

	#[cfg(feature = "rune")]
	#[rune::function(path = Self::from_str)]
	pub fn r_from_str(value: &str) -> Result<Self, ParseIntError> {
		Ok(Self(u64::from_str_radix(value, 16)?))
	}

	#[cfg_attr(feature = "rune", rune::function(keep, path = Self::as_u64))]
	pub fn as_u64(&self) -> u64 {
		self.0
	}
}

impl Display for EntityId {
	fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
		write!(f, "{:0>16x}", self.as_u64())
	}
}

impl Debug for EntityId {
	fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
		write!(f, "{:0>16x}", self.as_u64())
	}
}

impl FromStr for EntityId {
	type Err = ParseIntError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		u64::from_str_radix(s, 16).map(Self)
	}
}

impl From<u64> for EntityId {
	fn from(value: u64) -> Self {
		Self(value)
	}
}

impl From<EntityId> for u64 {
	fn from(value: EntityId) -> Self {
		value.0
	}
}

impl From<EntityId> for String {
	fn from(value: EntityId) -> Self {
		value.to_string()
	}
}

impl Serialize for EntityId {
	fn serialize<S: serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_string())
	}
}

impl<'de> Deserialize<'de> for EntityId {
	fn deserialize<D: serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		Self::from_str(&s).map_err(serde::de::Error::custom)
	}
}

impl Type for EntityId {
	fn inline(_: &mut specta::TypeMap, _: &[specta::DataType]) -> specta::DataType {
		specta::DataType::Primitive(specta::datatype::PrimitiveType::String)
	}
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Entity {
	/// The hash of the TEMP file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "factory")]
	pub factory: PathedID,

	/// The hash of the TBLU file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "blueprint")]
	pub blueprint: PathedID,

	/// The root sub-entity of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "rootEntity")]
	pub root_entity: EntityId,

	/// The sub-entities of this entity.
	#[serde(rename = "entities")]
	pub entities: IndexMap<EntityId, SubEntity>,

	/// Properties on other entities (local or external) to override when this entity is loaded.
	///
	/// Overriding a local entity would be a rather pointless maneuver given that you could just actually change it in the entity instead of using an override.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "propertyOverrides")]
	pub property_overrides: Vec<PropertyOverride>,

	/// Entities (external or local) to delete (including their organisational children) when
	/// this entity is loaded.
	///
	/// Deleting a local entity would be a rather pointless maneuver given that you could just actually remove it from this entity instead of using an override.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "overrideDeletes")]
	pub override_deletes: Vec<Ref>,

	/// Pin (event) connections (between entities, external or local) to add when this entity is
	/// loaded.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "pinConnectionOverrides")]
	pub pin_connection_overrides: Vec<PinConnectionOverride>,

	/// Pin (event) connections (between entities, external or local) to delete when this entity
	/// is loaded.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "pinConnectionOverrideDeletes")]
	pub pin_connection_override_deletes: Vec<PinConnectionOverrideDelete>,

	/// The external scenes that this entity references.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "externalScenes")]
	pub external_scenes: Vec<PathedID>,

	/// The type of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "subType")]
	pub sub_type: SubType,

	/// The QuickEntity format version of this entity. The current version is 3.2.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "quickEntityVersion")]
	pub quick_entity_version: f32,

	/// Extra resource references that should be added to the entity's factory when converted to the game's format.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "extraFactoryReferences")]
	pub extra_factory_references: Vec<ResourceReference>,

	/// Extra resource references that should be added to the entity's blueprint when converted to the game's format.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "extraBlueprintReferences")]
	pub extra_blueprint_references: Vec<ResourceReference>,

	/// Comments to be attached to sub-entities.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "comments")]
	pub comments: Vec<CommentEntity>
}

/// A comment entity.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct CommentEntity {
	/// The sub-entity this comment is parented to.
	pub parent: Ref,

	/// The name of this comment.
	pub name: String,

	/// The text this comment holds.
	pub text: String
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune_functions(Self::r_new))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SubEntity {
	/// The "logical" or "organisational" parent of the entity, used for tree organisation in graphical editors.
	///
	/// Has no effect on the entity in game.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "parent")]
	pub parent: Ref,

	/// The name of the entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "name")]
	pub name: String,

	/// The factory of the entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "factory")]
	#[serde(alias = "template")]
	pub factory: ResourceReference,

	/// The blueprint of the entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "blueprint")]
	pub blueprint: PathedID,

	/// Whether the entity is only loaded in IO's editor.
	///
	/// Setting this to true will remove the entity from the game as well as all of its organisational (but not coordinate) children.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "editorOnly")]
	#[serde(default)]
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub editor_only: bool,

	/// Properties of the entity.
	#[serde(rename = "properties")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub properties: IndexMap<String, Property>,

	/// Properties to apply conditionally to the entity based on platform.
	#[serde(rename = "platformSpecificProperties")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub platform_specific_properties: IndexMap<String, IndexMap<String, Property>>,

	/// Inputs on entities to trigger when events occur.
	#[serde(rename = "events")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub events: IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>,

	/// Inputs on entities to trigger when this entity is given inputs.
	#[serde(rename = "inputCopying")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub input_copying: IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>,

	/// Events to propagate on other entities.
	#[serde(rename = "outputCopying")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub output_copying: IndexMap<String, IndexMap<String, Vec<RefMaybeConstantValue>>>,

	/// Properties on other entities that can be accessed from this entity.
	#[serde(rename = "propertyAliases")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub property_aliases: IndexMap<String, Vec<PropertyAlias>>,

	/// Entities that can be accessed from this entity.
	#[serde(rename = "exposedEntities")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub exposed_entities: IndexMap<String, ExposedEntity>,

	/// Interfaces implemented by other entities that can be accessed from this entity.
	#[serde(rename = "exposedInterfaces")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub exposed_interfaces: IndexMap<String, EntityId>,

	/// The subsets that this entity belongs to.
	#[serde(rename = "subsets")]
	#[serde(default)]
	#[serde(skip_serializing_if = "IndexMap::is_empty")]
	pub subsets: IndexMap<String, Vec<EntityId>>
}

#[cfg(feature = "rune")]
impl SubEntity {
	/// Constructor function. An actual struct constructor cannot be made as Rune only supports up to five parameters in functions.
	#[rune::function(path = Self::new)]
	fn r_new(parent: Ref, name: String, factory: ResourceReference, blueprint: PathedID) -> Self {
		Self {
			parent,
			name,
			factory,
			blueprint,
			editor_only: false,
			properties: Default::default(),
			platform_specific_properties: Default::default(),
			events: Default::default(),
			input_copying: Default::default(),
			output_copying: Default::default(),
			property_aliases: Default::default(),
			exposed_entities: Default::default(),
			exposed_interfaces: Default::default(),
			subsets: Default::default()
		}
	}

	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "properties", |s: &Self| {
			s.properties.to_owned().into_iter().collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: HashMap<String, Property>| {
				s.properties = value.into_iter().collect();
			}
		)?;

		module.field_function(
			&rune::runtime::Protocol::GET,
			"platform_specific_properties",
			|s: &Self| {
				s.platform_specific_properties
					.clone()
					.into_iter()
					.map(|(x, y)| (x, y.into_iter().collect::<HashMap<_, _>>()))
					.collect::<HashMap<_, _>>()
			}
		)?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"platform_specific_properties",
			|s: &mut Self, value: HashMap<String, HashMap<String, Property>>| {
				s.platform_specific_properties = value.into_iter().map(|(x, y)| (x, y.into_iter().collect())).collect()
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "events", |s: &Self| {
			s.events
				.to_owned()
				.into_iter()
				.map(|(x, y)| (x, y.into_iter().collect::<HashMap<_, _>>()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"events",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<RefMaybeConstantValue>>>| {
				s.events = value.into_iter().map(|(x, y)| (x, y.into_iter().collect())).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "input_copying", |s: &Self| {
			s.input_copying
				.to_owned()
				.into_iter()
				.map(|(x, y)| (x, y.into_iter().collect::<HashMap<_, _>>()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"input_copying",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<RefMaybeConstantValue>>>| {
				s.input_copying = value.into_iter().map(|(x, y)| (x, y.into_iter().collect())).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "output_copying", |s: &Self| {
			s.output_copying
				.to_owned()
				.into_iter()
				.map(|(x, y)| (x, y.into_iter().collect::<HashMap<_, _>>()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"output_copying",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<RefMaybeConstantValue>>>| {
				s.output_copying = value.into_iter().map(|(x, y)| (x, y.into_iter().collect())).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "property_aliases", |s: &Self| {
			s.property_aliases
				.clone()
				.into_iter()
				.map(|(x, y)| (x, y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"property_aliases",
			|s: &mut Self, value: HashMap<String, Vec<PropertyAlias>>| {
				s.property_aliases = value.into_iter().collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_entities", |s: &Self| {
			s.exposed_entities
				.clone()
				.into_iter()
				.map(|(x, y)| (x, y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_entities",
			|s: &mut Self, value: HashMap<String, ExposedEntity>| {
				s.exposed_entities = value.into_iter().collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_interfaces", |s: &Self| {
			s.exposed_interfaces
				.clone()
				.into_iter()
				.map(|(x, y)| (x, y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_interfaces",
			|s: &mut Self, value: HashMap<String, EntityId>| {
				s.exposed_interfaces = value.into_iter().collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "subsets", |s: &Self| {
			s.subsets
				.clone()
				.into_iter()
				.map(|(x, y)| (x, y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"subsets",
			|s: &mut Self, value: HashMap<String, Vec<EntityId>>| {
				s.subsets = value.into_iter().collect();
			}
		)?;

		Ok(())
	}
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(untagged)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
pub enum RefMaybeConstantValue {
	#[cfg_attr(feature = "rune", rune(constructor))]
	RefWithConstantValue(#[cfg_attr(feature = "rune", rune(get, set))] RefWithConstantValue),

	#[cfg_attr(feature = "rune", rune(constructor))]
	Ref(#[cfg_attr(feature = "rune", rune(get, set))] Ref)
}

/// A reference accompanied by a constant value.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct Property {
	/// The type of the property.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "type")]
	pub property_type: String,

	/// The value of the property.
	#[serde(rename = "value")]
	pub value: serde_json::Value,

	/// Whether the property should be (presumably) loaded/set after the entity has been initialised.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "postInit")]
	#[serde(default)]
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub post_init: bool
}

#[cfg(feature = "rune")]
impl Property {
	fn rune_construct(property_type: String, value: rune::Value, post_init: bool) -> Self {
		Self {
			property_type,
			value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
			post_init
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
			}
		)?;

		Ok(())
	}
}

/// A simple property.
///
/// Simple properties cannot be marked as post-init. They are used by pin connection overrides, events and input/output copying.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct SimpleProperty {
	/// The type of the simple property.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "type")]
	pub property_type: String,

	/// The simple property's value.
	#[serde(rename = "value")]
	pub value: serde_json::Value
}

#[cfg(feature = "rune")]
impl SimpleProperty {
	fn rune_construct(property_type: String, value: rune::Value) -> Self {
		Self {
			property_type,
			value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
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
			}
		)?;

		Ok(())
	}
}

/// An exposed entity.
///
/// Exposed entities are accessible when referencing this entity through a property on long-form references.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PropertyAlias {
	/// The other entity's property that should be accessed from this entity.
	#[serde(rename = "originalProperty")]
	pub original_property: String,

	/// The other entity whose property will be accessed.
	#[serde(rename = "originalEntity")]
	pub original_entity: Ref
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
	pub value: Option<SimpleProperty> // TODO: Convert simple property values to QN format
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
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
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct PropertyOverride {
	/// An array of references to the entities to override the properties of.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "entities")]
	pub entities: Vec<Ref>,

	/// A set of properties to override on the entities.
	#[serde(rename = "properties")]
	pub properties: IndexMap<String, SimpleProperty>
}

#[cfg(feature = "rune")]
impl PropertyOverride {
	fn rune_construct(entities: Vec<Ref>, properties: HashMap<String, SimpleProperty>) -> Self {
		Self {
			entities,
			properties: properties.into_iter().collect()
		}
	}

	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "properties", |s: &Self| {
			Some(s.properties.clone().into_iter().collect::<HashMap<_, _>>())
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: HashMap<String, SimpleProperty>| {
				s.properties = value.into_iter().collect();
			}
		)?;

		Ok(())
	}
}

/// A long-form reference to an entity, allowing for the specification of external scenes and/or an exposed entity.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
pub struct FullRef {
	/// The entity to reference's ID.
	#[serde(rename = "ref")]
	pub entity_ref: EntityId,

	/// The external scene the referenced entity resides in.
	#[serde(rename = "externalScene")]
	pub external_scene: Option<PathedID>,

	/// The sub-entity to reference that is exposed by the referenced entity.
	#[serde(rename = "exposedEntity")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub exposed_entity: Option<String>
}

/// A reference to an entity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq)]
#[serde(untagged)]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::qn_structs))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ))]
pub enum Ref {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Full(#[cfg_attr(feature = "rune", rune(get, set))] FullRef),

	/// A short-form reference represents either a local reference with no exposed entity or a null reference.
	#[cfg_attr(feature = "rune", rune(constructor))]
	Short(#[cfg_attr(feature = "rune", rune(get, set))] Option<EntityId>)
}

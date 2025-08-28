use std::{
	fmt::{Debug, Display, Formatter},
	hash::Hash,
	num::ParseIntError,
	str::FromStr
};

use ecow::EcoString;
use educe::Educe;
use hitman_commons::metadata::{ResourceReference, RuntimeID};
use ordermap::OrderMap;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use specta::Type;

#[cfg(feature = "rune")]
use std::collections::HashMap;

#[cfg(feature = "rune")]
pub fn rune_module() -> Result<rune::Module, rune::ContextError> {
	let mut module = rune::Module::with_crate_item("quickentity_rs", ["entity"])?;

	module.ty::<EntityID>()?;
	module.ty::<SubType>()?;
	module.ty::<Entity>()?;
	module.ty::<CommentEntity>()?;
	module.ty::<SubEntity>()?;
	module.ty::<PinConnection>()?;
	module.ty::<Property>()?;
	module.ty::<SimpleProperty>()?;
	module.ty::<ExposedEntity>()?;
	module.ty::<PropertyAlias>()?;
	module.ty::<PinConnectionOverride>()?;
	module.ty::<PinConnectionOverrideDelete>()?;
	module.ty::<PropertyOverride>()?;
	module.ty::<Ref>()?;

	Ok(module)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
pub enum SubType {
	#[cfg_attr(feature = "rune", rune(constructor))]
	Brick,

	#[cfg_attr(feature = "rune", rune(constructor))]
	Scene,

	#[cfg_attr(feature = "rune", rune(constructor))]
	Template
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DISPLAY_FMT, DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(
	feature = "rune",
	rune_functions(Self::r_from_u64, Self::r_from_str, Self::as_u64__meta)
)]
#[derive(SerializeDisplay, DeserializeFromStr, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityID(u64);

impl EntityID {
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

impl Display for EntityID {
	fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
		write!(f, "{:0>16x}", self.as_u64())
	}
}

impl Debug for EntityID {
	fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
		write!(f, "{:0>16x}", self.as_u64())
	}
}

impl FromStr for EntityID {
	type Err = ParseIntError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		u64::from_str_radix(s, 16).map(Self)
	}
}

impl From<u64> for EntityID {
	fn from(value: u64) -> Self {
		Self(value)
	}
}

impl From<EntityID> for u64 {
	fn from(value: EntityID) -> Self {
		value.0
	}
}

impl Type for EntityID {
	fn inline(_: &mut specta::TypeMap, _: specta::Generics<'_>) -> specta::DataType {
		specta::DataType::Primitive(specta::datatype::PrimitiveType::String)
	}
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(
	feature = "rune",
	rune_functions(Self::r_get_entity, Self::r_insert_entity, Self::r_remove_entity)
)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Educe)]
#[educe(Hash)]
pub struct Entity {
	/// The hash of the TEMP file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "factory")]
	pub factory: RuntimeID,

	/// The hash of the TBLU file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "blueprint")]
	pub blueprint: RuntimeID,

	/// The root sub-entity of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "rootEntity")]
	pub root_entity: EntityID,

	/// The sub-entities of this entity.
	#[serde(rename = "entities")]
	pub entities: OrderMap<EntityID, SubEntity>,

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
	pub external_scenes: Vec<RuntimeID>,

	/// The type of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "subType")]
	pub sub_type: SubType,

	/// The QuickEntity format version of this entity. The current version is 3.2.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "quickEntityVersion")]
	#[educe(Hash(ignore))]
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

#[cfg(feature = "rune")]
impl Entity {
	#[rune::function(instance, path = Self::get_entity)]
	fn r_get_entity(&self, id: EntityID) -> Option<SubEntity> {
		self.entities.get(&id).cloned()
	}

	#[rune::function(instance, path = Self::insert_entity)]
	fn r_insert_entity(&mut self, id: EntityID, entity: SubEntity) {
		self.entities.insert(id, entity);
	}

	#[rune::function(instance, path = Self::remove_entity)]
	fn r_remove_entity(&mut self, id: EntityID) {
		self.entities.remove(&id);
	}
}

/// A comment entity.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct CommentEntity {
	/// The sub-entity this comment is parented to.
	pub parent: Option<EntityID>,

	/// The name of this comment.
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub name: EcoString,

	/// The text this comment holds.
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub text: EcoString
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune_functions(Self::r_new))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubEntity {
	/// The "logical" or "organisational" parent of the entity, used for tree organisation in graphical editors.
	///
	/// Has no effect on the entity in game.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "parent")]
	pub parent: Option<Ref>,

	/// The name of the entity.
	#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
	pub name: EcoString,

	/// The factory of the entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "factory")]
	pub factory: ResourceReference,

	/// The blueprint of the entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "blueprint")]
	pub blueprint: RuntimeID,

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
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub properties: OrderMap<EcoString, Property>,

	/// Properties to apply conditionally to the entity based on platform.
	#[serde(rename = "platformSpecificProperties")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub platform_specific_properties: OrderMap<EcoString, OrderMap<EcoString, Property>>,

	/// Inputs on entities to trigger when events occur.
	#[serde(rename = "events")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub events: OrderMap<EcoString, OrderMap<EcoString, Vec<PinConnection>>>,

	/// Inputs on entities to trigger when this entity is given inputs.
	#[serde(rename = "inputCopying")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub input_copying: OrderMap<EcoString, OrderMap<EcoString, Vec<PinConnection>>>,

	/// Events to propagate on other entities.
	#[serde(rename = "outputCopying")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub output_copying: OrderMap<EcoString, OrderMap<EcoString, Vec<PinConnection>>>,

	/// Properties on other entities that can be accessed from this entity.
	#[serde(rename = "propertyAliases")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub property_aliases: OrderMap<EcoString, Vec<PropertyAlias>>,

	/// Entities that can be accessed from this entity.
	#[serde(rename = "exposedEntities")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub exposed_entities: OrderMap<EcoString, ExposedEntity>,

	/// Interfaces implemented by other entities that can be accessed from this entity.
	#[serde(rename = "exposedInterfaces")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub exposed_interfaces: OrderMap<EcoString, EntityID>,

	/// The subsets that this entity belongs to.
	#[serde(rename = "subsets")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	pub subsets: OrderMap<EcoString, Vec<EntityID>>
}

#[cfg(feature = "rune")]
impl SubEntity {
	/// Constructor function. An actual struct constructor cannot be made as Rune only supports up to five parameters in functions.
	#[rune::function(path = Self::new)]
	fn r_new(parent: Option<Ref>, name: String, factory: ResourceReference, blueprint: RuntimeID) -> Self {
		Self {
			parent,
			name: name.into(),
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
			s.properties
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: HashMap<String, Property>| {
				s.properties = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(
			&rune::runtime::Protocol::GET,
			"platform_specific_properties",
			|s: &Self| {
				s.platform_specific_properties
					.clone()
					.into_iter()
					.map(|(x, y)| {
						(
							String::from(x),
							y.into_iter()
								.map(|(x, y)| (String::from(x), y))
								.collect::<HashMap<_, _>>()
						)
					})
					.collect::<HashMap<_, _>>()
			}
		)?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"platform_specific_properties",
			|s: &mut Self, value: HashMap<String, HashMap<String, Property>>| {
				s.platform_specific_properties = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect()
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "events", |s: &Self| {
			s.events
				.clone()
				.into_iter()
				.map(|(x, y)| {
					(
						String::from(x),
						y.into_iter()
							.map(|(x, y)| (String::from(x), y))
							.collect::<HashMap<_, _>>()
					)
				})
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"events",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<PinConnection>>>| {
				s.events = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "input_copying", |s: &Self| {
			s.input_copying
				.clone()
				.into_iter()
				.map(|(x, y)| {
					(
						String::from(x),
						y.into_iter()
							.map(|(x, y)| (String::from(x), y))
							.collect::<HashMap<_, _>>()
					)
				})
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"input_copying",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<PinConnection>>>| {
				s.input_copying = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "output_copying", |s: &Self| {
			s.output_copying
				.clone()
				.into_iter()
				.map(|(x, y)| {
					(
						String::from(x),
						y.into_iter()
							.map(|(x, y)| (String::from(x), y))
							.collect::<HashMap<_, _>>()
					)
				})
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"output_copying",
			|s: &mut Self, value: HashMap<String, HashMap<String, Vec<PinConnection>>>| {
				s.output_copying = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "property_aliases", |s: &Self| {
			s.property_aliases
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"property_aliases",
			|s: &mut Self, value: HashMap<String, Vec<PropertyAlias>>| {
				s.property_aliases = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_entities", |s: &Self| {
			s.exposed_entities
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_entities",
			|s: &mut Self, value: HashMap<String, ExposedEntity>| {
				s.exposed_entities = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_interfaces", |s: &Self| {
			s.exposed_interfaces
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_interfaces",
			|s: &mut Self, value: HashMap<String, EntityID>| {
				s.exposed_interfaces = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "subsets", |s: &Self| {
			s.subsets
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"subsets",
			|s: &mut Self, value: HashMap<String, Vec<EntityID>>| {
				s.subsets = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		Ok(())
	}
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
#[serde(from = "PinConnectionProxy", into = "PinConnectionProxy")]
pub struct PinConnection {
	/// The entity being referenced.
	#[serde(rename = "ref")]
	pub entity_ref: Ref,

	/// The constant value of the pin connection.
	pub value: Option<SimpleProperty>
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PinConnectionProxy {
	Ref(Ref),
	RefWithValue {
		#[serde(rename = "ref")]
		entity_ref: Ref,
		value: SimpleProperty
	}
}

impl From<PinConnection> for PinConnectionProxy {
	fn from(pin: PinConnection) -> Self {
		if let Some(value) = pin.value {
			Self::RefWithValue {
				entity_ref: pin.entity_ref,
				value
			}
		} else {
			Self::Ref(pin.entity_ref)
		}
	}
}

impl From<PinConnectionProxy> for PinConnection {
	fn from(proxy: PinConnectionProxy) -> Self {
		match proxy {
			PinConnectionProxy::Ref(entity_ref) => PinConnection {
				entity_ref,
				value: None
			},

			PinConnectionProxy::RefWithValue { entity_ref, value } => PinConnection {
				entity_ref,
				value: Some(value)
			}
		}
	}
}

/// A property with a type and a value. Can be marked as post-init.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct Property {
	/// The type of the property.
	#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
	#[serde(rename = "type")]
	#[specta(type = String)]
	pub property_type: EcoString,

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
			property_type: property_type.into(),
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
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct SimpleProperty {
	/// The type of the simple property.
	#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
	#[serde(rename = "type")]
	#[specta(type = String)]
	pub property_type: EcoString,

	/// The simple property's value.
	#[serde(rename = "value")]
	pub value: serde_json::Value
}

#[cfg(feature = "rune")]
impl SimpleProperty {
	fn rune_construct(property_type: String, value: rune::Value) -> Self {
		Self {
			property_type: property_type.into(),
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
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
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
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct PropertyAlias {
	/// The other entity's property that should be accessed from this entity.
	#[serde(rename = "originalProperty")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub original_property: EcoString,

	/// The other entity whose property will be accessed.
	#[serde(rename = "originalEntity")]
	pub original_entity: EntityID
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct PinConnectionOverride {
	/// The entity that will trigger the input on the other entity.
	///
	/// If this references a local entity, you can simply use an event on the entity itself.
	#[serde(rename = "fromEntity")]
	pub from_entity: Ref,

	/// The name of the event on the fromEntity that will trigger the input on the toEntity.
	#[serde(rename = "fromPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub from_pin: EcoString,

	/// The entity whose input will be triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will be triggered by the event on the
	/// fromEntity.
	#[serde(rename = "toPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub to_pin: EcoString,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	pub value: Option<SimpleProperty>
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct PinConnectionOverrideDelete {
	/// The entity that triggers the input on the other entity.
	#[serde(rename = "fromEntity")]
	pub from_entity: Ref,

	/// The name of the event on the fromEntity that will no longer trigger the input on the
	/// toEntity.
	#[serde(rename = "fromPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub from_pin: EcoString,

	/// The entity whose input is triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will no longer be triggered by the event on
	/// the fromEntity.
	#[serde(rename = "toPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	pub to_pin: EcoString,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	pub value: Option<SimpleProperty>
}

/// A set of overrides for entity properties.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyOverride {
	/// An array of references to the entities to override the properties of.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "entities")]
	pub entities: Vec<Ref>,

	/// A set of properties to override on the entities.
	#[serde(rename = "properties")]
	pub properties: OrderMap<EcoString, SimpleProperty>
}

#[cfg(feature = "rune")]
impl PropertyOverride {
	fn rune_construct(entities: Vec<Ref>, properties: HashMap<String, SimpleProperty>) -> Self {
		Self {
			entities,
			properties: properties.into_iter().map(|(x, y)| (x.into(), y)).collect()
		}
	}

	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "properties", |s: &Self| {
			Some(
				s.properties
					.clone()
					.into_iter()
					.map(|(x, y)| (String::from(x), y))
					.collect::<HashMap<_, _>>()
			)
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: HashMap<String, SimpleProperty>| {
				s.properties = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		Ok(())
	}
}

/// A reference to an entity.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune_functions(Self::local__meta))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
#[serde(from = "RefProxy", into = "RefProxy")]
pub struct Ref {
	/// The entity to reference's ID.
	#[serde(rename = "ref")]
	pub entity_id: EntityID,

	/// The external scene the referenced entity resides in.
	#[serde(rename = "externalScene")]
	pub external_scene: Option<RuntimeID>,

	/// The sub-entity to reference that is exposed by the referenced entity.
	#[serde(rename = "exposedEntity")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub exposed_entity: Option<String>
}

impl Ref {
	#[cfg_attr(feature = "rune", rune::function(keep, path = Self::local))]
	pub fn local(entity_id: EntityID) -> Self {
		Self {
			entity_id,
			external_scene: None,
			exposed_entity: None
		}
	}
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RefProxy {
	Short(EntityID),
	Full {
		#[serde(rename = "ref")]
		entity_ref: EntityID,

		#[serde(rename = "externalScene")]
		#[serde(skip_serializing_if = "Option::is_none")]
		external_scene: Option<RuntimeID>,

		#[serde(rename = "exposedEntity")]
		#[serde(skip_serializing_if = "Option::is_none")]
		exposed_entity: Option<String>
	}
}

impl From<Ref> for RefProxy {
	fn from(value: Ref) -> Self {
		if value.external_scene.is_some() || value.exposed_entity.is_some() {
			Self::Full {
				entity_ref: value.entity_id,
				external_scene: value.external_scene,
				exposed_entity: value.exposed_entity
			}
		} else {
			Self::Short(value.entity_id)
		}
	}
}

impl From<RefProxy> for Ref {
	fn from(value: RefProxy) -> Self {
		match value {
			RefProxy::Short(entity_ref) => Self {
				entity_id: entity_ref,
				external_scene: None,
				exposed_entity: None
			},

			RefProxy::Full {
				entity_ref,
				external_scene,
				exposed_entity
			} => Self {
				entity_id: entity_ref,
				external_scene,
				exposed_entity
			}
		}
	}
}

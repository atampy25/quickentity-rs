use std::{
	fmt::{Debug, Display, Formatter},
	hash::Hash,
	num::ParseIntError,
	str::FromStr
};

use anyhow::{Context, Result};
use ecow::EcoString;
use fn_error_context::context;
use glacier_commons::metadata::{ResourceMetadata, ResourceReference, RuntimeID};
use identity_hash::BuildIdentityHasher;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use specta::Type;
use tryvial::try_fn;

use crate::{
	HashMap, OrderMap,
	game::{FromQuickEntity, ToQuickEntity, types as game_types},
	variant::Variant
};

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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for EntityID {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		"EntityID".into()
	}

	fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
		schemars::json_schema!({
			"type": "string",
			"pattern": "^[0-9a-fA-F]{16}$"
		})
	}
}

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
	fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
		String::definition(types)
	}
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, CLONE))]
#[cfg_attr(
	feature = "rune",
	rune_functions(Self::r_entities, Self::r_get_entity, Self::r_insert_entity, Self::r_remove_entity)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Entity {
	/// The TEMP file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "factory")]
	pub factory: RuntimeID,

	/// The TBLU file of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "blueprint")]
	pub blueprint: RuntimeID,

	/// The root sub-entity of this entity.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "rootEntity")]
	pub root_entity: EntityID,

	/// The sub-entities of this entity.
	#[serde(rename = "entities")]
	#[specta(type = std::collections::HashMap<EntityID, SubEntity>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<EntityID, SubEntity>")
	)]
	pub entities: OrderMap<EntityID, SubEntity, BuildIdentityHasher<u64>>,

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
	#[serde(deserialize_with = "validate_qn_version")]
	pub quickentity_version: f32,

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

#[try_fn]
fn validate_qn_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
	let version = f32::deserialize(deserializer)?;

	if version != 3.2 {
		return Err(serde::de::Error::invalid_value(
			serde::de::Unexpected::Float(version as f64),
			&"version 3.2"
		));
	}

	version
}

#[cfg(feature = "rune")]
impl Entity {
	#[rune::function(instance, path = Self::entities)]
	fn r_entities(&self) -> Vec<EntityID> {
		self.entities.keys().copied().collect()
	}

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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct CommentEntity {
	/// The sub-entity this comment is parented to.
	pub parent: Option<EntityID>,

	/// The name of this comment.
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub name: EcoString,

	/// The text this comment holds.
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub text: EcoString
}

#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune_functions(Self::r_new))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct SubEntity {
	/// The "logical" or "organisational" parent of the entity, used for tree organisation in graphical editors.
	///
	/// Has no effect on the entity in game.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "parent")]
	pub parent: Option<Ref>,

	/// The name of the entity.
	#[cfg_attr(feature = "rune", rune(get, set, as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
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

	/// Platforms on which the entity will not be loaded.
	#[serde(rename = "excludedPlatforms")]
	#[serde(default)]
	#[serde(skip_serializing_if = "Vec::is_empty")]
	#[specta(type = Vec<String>)]
	#[cfg_attr(feature = "schemars", schemars(with = "Vec<String>"))]
	pub excluded_platforms: Vec<EcoString>,

	/// Properties of the entity.
	#[serde(rename = "properties")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, Property>)]
	#[cfg_attr(feature = "schemars", schemars(with = "std::collections::HashMap<String, Property>"))]
	pub properties: OrderMap<EcoString, Property>,

	/// Properties to apply conditionally to the entity based on platform.
	#[serde(rename = "platformSpecificProperties")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, std::collections::HashMap<String, Property>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<String, std::collections::HashMap<String, Property>>")
	)]
	pub platform_specific_properties: OrderMap<EcoString, OrderMap<EcoString, Property>>,

	/// Inputs on entities to trigger when events occur.
	#[serde(rename = "events")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, std::collections::HashMap<String, Vec<PinConnection>>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<String, std::collections::HashMap<String, Vec<PinConnection>>>")
	)]
	pub events: OrderMap<EcoString, OrderMap<EcoString, Vec<PinConnection>>>,

	/// Inputs on entities to trigger when this entity is given inputs.
	#[serde(rename = "inputForwardings")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(
			with = "std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>"
		)
	)]
	pub input_forwardings: OrderMap<EcoString, OrderMap<EcoString, Vec<LocalPinConnection>>>,

	/// Events to propagate on other entities.
	#[serde(rename = "outputForwardings")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(
			with = "std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>"
		)
	)]
	pub output_forwardings: OrderMap<EcoString, OrderMap<EcoString, Vec<LocalPinConnection>>>,

	/// Properties on other entities that can be accessed from this entity.
	#[serde(rename = "propertyAliases")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, Vec<PropertyAlias>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<String, Vec<PropertyAlias>>")
	)]
	pub property_aliases: OrderMap<EcoString, Vec<PropertyAlias>>,

	/// Entities that can be accessed from this entity.
	#[serde(rename = "exposedEntities")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, ExposedEntity>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<String, ExposedEntity>")
	)]
	pub exposed_entities: OrderMap<EcoString, ExposedEntity>,

	/// Interfaces implemented by other entities that can be accessed from this entity.
	#[serde(rename = "exposedInterfaces")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, EntityID>)]
	#[cfg_attr(feature = "schemars", schemars(with = "std::collections::HashMap<String, EntityID>"))]
	pub exposed_interfaces: OrderMap<EcoString, EntityID>,

	/// The subsets that this entity belongs to.
	#[serde(rename = "subsets")]
	#[serde(default)]
	#[serde(skip_serializing_if = "OrderMap::is_empty")]
	#[specta(type = std::collections::HashMap<String, Vec<EntityID>>)]
	#[cfg_attr(
		feature = "schemars",
		schemars(with = "std::collections::HashMap<String, Vec<EntityID>>")
	)]
	pub subsets: OrderMap<EcoString, Vec<EntityID>>
}

impl Default for SubEntity {
	fn default() -> Self {
		Self {
			parent: None,
			name: Default::default(),
			factory: ResourceReference {
				resource: glacier_commons::rid!("[modules:/zentity.class].entitytype"),
				flags: Default::default()
			},
			blueprint: glacier_commons::rid!("[modules:/zentity.class].entityblueprint"),
			editor_only: false,
			excluded_platforms: Default::default(),
			properties: Default::default(),
			platform_specific_properties: Default::default(),
			events: Default::default(),
			input_forwardings: Default::default(),
			output_forwardings: Default::default(),
			property_aliases: Default::default(),
			exposed_entities: Default::default(),
			exposed_interfaces: Default::default(),
			subsets: Default::default()
		}
	}
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
			..Default::default()
		}
	}

	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "excluded_platforms", |s: &Self| {
			s.excluded_platforms
				.clone()
				.into_iter()
				.map(|x| String::from(x))
				.collect::<Vec<_>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"excluded_platforms",
			|s: &mut Self, value: Vec<String>| {
				s.excluded_platforms = value.into_iter().map(|x| x.into()).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "properties", |s: &Self| {
			s.properties
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y))
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: std::collections::HashMap<String, Property>| {
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
								.collect::<std::collections::HashMap<_, _>>()
						)
					})
					.collect::<std::collections::HashMap<_, _>>()
			}
		)?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"platform_specific_properties",
			|s: &mut Self, value: std::collections::HashMap<String, std::collections::HashMap<String, Property>>| {
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
							.collect::<std::collections::HashMap<_, _>>()
					)
				})
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"events",
			|s: &mut Self,
			 value: std::collections::HashMap<String, std::collections::HashMap<String, Vec<PinConnection>>>| {
				s.events = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "input_forwardings", |s: &Self| {
			s.input_forwardings
				.clone()
				.into_iter()
				.map(|(x, y)| {
					(
						String::from(x),
						y.into_iter()
							.map(|(x, y)| (String::from(x), y))
							.collect::<std::collections::HashMap<_, _>>()
					)
				})
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"input_forwardings",
			|s: &mut Self,
			 value: std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>| {
				s.input_forwardings = value
					.into_iter()
					.map(|(x, y)| (x.into(), y.into_iter().map(|(x, y)| (x.into(), y)).collect()))
					.collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "output_forwardings", |s: &Self| {
			s.output_forwardings
				.clone()
				.into_iter()
				.map(|(x, y)| {
					(
						String::from(x),
						y.into_iter()
							.map(|(x, y)| (String::from(x), y))
							.collect::<std::collections::HashMap<_, _>>()
					)
				})
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"output_forwardings",
			|s: &mut Self,
			 value: std::collections::HashMap<String, std::collections::HashMap<String, Vec<LocalPinConnection>>>| {
				s.output_forwardings = value
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
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"property_aliases",
			|s: &mut Self, value: std::collections::HashMap<String, Vec<PropertyAlias>>| {
				s.property_aliases = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_entities", |s: &Self| {
			s.exposed_entities
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_entities",
			|s: &mut Self, value: std::collections::HashMap<String, ExposedEntity>| {
				s.exposed_entities = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "exposed_interfaces", |s: &Self| {
			s.exposed_interfaces
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_interfaces",
			|s: &mut Self, value: std::collections::HashMap<String, EntityID>| {
				s.exposed_interfaces = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "subsets", |s: &Self| {
			s.subsets
				.clone()
				.into_iter()
				.map(|(x, y)| (String::from(x), y.to_owned()))
				.collect::<std::collections::HashMap<_, _>>()
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"subsets",
			|s: &mut Self, value: std::collections::HashMap<String, Vec<EntityID>>| {
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(from = "PinConnectionProxy", into = "PinConnectionProxy")]
pub struct PinConnection {
	/// The entity being referenced.
	#[serde(rename = "ref")]
	pub entity_ref: Ref,

	/// The constant value of the pin connection.
	pub value: Option<Variant>
}

impl Type for PinConnection {
	fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
		PinConnectionProxy::definition(types)
	}
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Type)]
#[serde(untagged)]
enum PinConnectionProxy {
	RefWithValue {
		#[serde(rename = "ref")]
		entity_ref: Ref,
		value: Variant
	},
	Ref(Ref)
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

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(from = "LocalPinConnectionProxy", into = "LocalPinConnectionProxy")]
pub struct LocalPinConnection {
	/// The entity being referenced.
	#[serde(rename = "ref")]
	pub entity_id: EntityID,

	/// The constant value of the pin connection.
	pub value: Option<Variant>
}

impl Type for LocalPinConnection {
	fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
		LocalPinConnectionProxy::definition(types)
	}
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Type)]
#[serde(untagged)]
enum LocalPinConnectionProxy {
	RefWithValue {
		#[serde(rename = "ref")]
		entity_id: EntityID,
		value: Variant
	},
	Ref(EntityID)
}

impl From<LocalPinConnection> for LocalPinConnectionProxy {
	fn from(pin: LocalPinConnection) -> Self {
		if let Some(value) = pin.value {
			Self::RefWithValue {
				entity_id: pin.entity_id,
				value
			}
		} else {
			Self::Ref(pin.entity_id)
		}
	}
}

impl From<LocalPinConnectionProxy> for LocalPinConnection {
	fn from(proxy: LocalPinConnectionProxy) -> Self {
		match proxy {
			LocalPinConnectionProxy::Ref(entity_id) => LocalPinConnection { entity_id, value: None },

			LocalPinConnectionProxy::RefWithValue { entity_id, value } => LocalPinConnection {
				entity_id,
				value: Some(value)
			}
		}
	}
}

/// A property with a type and a value. Can be marked as post-init.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct Property {
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(flatten)]
	pub value: Variant,

	/// Whether the property should be (presumably) loaded/set after the entity has been initialised.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "postInit")]
	#[serde(default)]
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub post_init: bool
}

/// An exposed entity.
///
/// Exposed entities are accessible when referencing this entity through a property on long-form references.
#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Type)]
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Eq, Hash)]
pub struct PropertyAlias {
	/// The other entity's property that should be accessed from this entity.
	#[serde(rename = "originalProperty")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
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
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
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
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub from_pin: EcoString,

	/// The entity whose input will be triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will be triggered by the event on the
	/// fromEntity.
	#[serde(rename = "toPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub to_pin: EcoString,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	pub value: Option<Variant>
}

#[cfg_attr(feature = "rune", serde_with::apply(_ => #[rune(get, set)]))]
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct PinConnectionOverrideDelete {
	/// The entity that triggers the input on the other entity.
	#[serde(rename = "fromEntity")]
	pub from_entity: Ref,

	/// The name of the event on the fromEntity that will no longer trigger the input on the
	/// toEntity.
	#[serde(rename = "fromPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub from_pin: EcoString,

	/// The entity whose input is triggered.
	#[serde(rename = "toEntity")]
	pub to_entity: Ref,

	/// The name of the input on the toEntity that will no longer be triggered by the event on
	/// the fromEntity.
	#[serde(rename = "toPin")]
	#[cfg_attr(feature = "rune", rune(as_into = String))]
	#[specta(type = String)]
	#[cfg_attr(feature = "schemars", schemars(with = "String"))]
	pub to_pin: EcoString,

	/// The constant value of the input to the toEntity.
	#[serde(rename = "value")]
	pub value: Option<Variant>
}

/// A set of overrides for entity properties.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct PropertyOverride {
	/// An array of references to the entities to override the properties of.
	#[cfg_attr(feature = "rune", rune(get, set))]
	#[serde(rename = "entities")]
	pub entities: Vec<Ref>,

	/// A set of properties to override on the entities.
	#[serde(rename = "properties")]
	#[specta(type = std::collections::HashMap<String, Variant>)]
	#[cfg_attr(feature = "schemars", schemars(with = "std::collections::HashMap<String, Variant>"))]
	pub properties: OrderMap<EcoString, Variant>,

	/// Which of the overridden properties can be edited at runtime.
	#[serde(rename = "runtimeEditable", default, skip_serializing_if = "Vec::is_empty")]
	#[specta(type = Vec<String>)]
	#[cfg_attr(feature = "schemars", schemars(with = "Vec<String>"))]
	pub runtime_editable: Vec<EcoString>
}

#[cfg(feature = "rune")]
impl PropertyOverride {
	fn rune_construct(
		entities: Vec<Ref>,
		properties: std::collections::HashMap<String, Variant>,
		runtime_editable: Vec<String>
	) -> Self {
		Self {
			entities,
			properties: properties.into_iter().map(|(x, y)| (x.into(), y)).collect(),
			runtime_editable: runtime_editable.into_iter().map(|x| x.into()).collect()
		}
	}

	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "properties", |s: &Self| {
			Some(
				s.properties
					.clone()
					.into_iter()
					.map(|(x, y)| (String::from(x), y))
					.collect::<std::collections::HashMap<_, _>>()
			)
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"properties",
			|s: &mut Self, value: std::collections::HashMap<String, Variant>| {
				s.properties = value.into_iter().map(|(x, y)| (x.into(), y)).collect();
			}
		)?;

		module.field_function(&rune::runtime::Protocol::GET, "runtime_editable", |s: &Self| {
			Some(
				s.runtime_editable
					.clone()
					.into_iter()
					.map(|x| String::from(x))
					.collect::<Vec<_>>()
			)
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"runtime_editable",
			|s: &mut Self, value: Vec<String>| {
				s.runtime_editable = value.into_iter().map(|x| x.into()).collect();
			}
		)?;

		Ok(())
	}
}

/// A reference to an entity.
#[cfg_attr(feature = "rune", derive(better_rune_derive::Any))]
#[cfg_attr(feature = "rune", rune(item = ::quickentity_rs::entity, install_with = Self::rune_install))]
#[cfg_attr(feature = "rune", rune_derive(DEBUG_FMT, PARTIAL_EQ, EQ, CLONE))]
#[cfg_attr(
	feature = "rune",
	rune_functions(Self::local__meta, Self::is_local__meta, Self::as_local__meta, Self::to_local__meta)
)]
#[cfg_attr(feature = "rune", rune(constructor_fn = Self::rune_construct))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(from = "RefProxy", into = "RefProxy")]
pub struct Ref {
	/// The entity to reference's ID.
	#[cfg_attr(feature = "rune", rune(get, set))]
	pub entity_id: EntityID,

	/// The external scene the referenced entity resides in.
	#[cfg_attr(feature = "rune", rune(get, set))]
	pub external_scene: Option<RuntimeID>,

	/// The sub-entity to reference that is exposed by the referenced entity.
	pub exposed_entity: Option<EcoString>
}

impl Type for Ref {
	fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
		RefProxy::definition(types)
	}
}

#[cfg(feature = "rune")]
impl Ref {
	fn rune_install(module: &mut rune::Module) -> Result<(), rune::ContextError> {
		module.field_function(&rune::runtime::Protocol::GET, "exposed_entity", |s: &Self| {
			s.exposed_entity.clone().map(|x| String::from(x))
		})?;

		module.field_function(
			&rune::runtime::Protocol::SET,
			"exposed_entity",
			|s: &mut Self, value: Option<String>| {
				s.exposed_entity = value.map(|x| x.into());
			}
		)?;

		Ok(())
	}

	fn rune_construct(entity_id: EntityID, external_scene: Option<RuntimeID>, exposed_entity: Option<String>) -> Self {
		Self {
			entity_id,
			external_scene,
			exposed_entity: exposed_entity.map(|x| x.into())
		}
	}
}

#[hotpath::measure_all]
impl Ref {
	#[cfg_attr(feature = "rune", rune::function(keep, path = Self::local))]
	pub fn local(entity_id: EntityID) -> Self {
		Self {
			entity_id,
			external_scene: None,
			exposed_entity: None
		}
	}

	#[cfg_attr(feature = "rune", rune::function(keep, instance, path = Self::is_local))]
	pub fn is_local(&self) -> bool {
		self.external_scene.is_none()
	}

	#[cfg_attr(feature = "rune", rune::function(keep, instance, path = Self::as_local))]
	pub fn as_local(&self) -> Option<EntityID> {
		if self.is_local() { Some(self.entity_id) } else { None }
	}

	#[cfg_attr(feature = "rune", rune::function(keep, instance, path = Self::to_local))]
	pub fn to_local(&self, entity_id: EntityID) -> Self {
		Self {
			entity_id,
			external_scene: None,
			exposed_entity: self.exposed_entity.to_owned()
		}
	}
}

mod ref_impl {
	use super::*;

	macro_rules! sub_entities {
		(h1, $a:expr) => {
			$a.entity_templates
		};

		($game:ident, $a:expr) => {
			$a.sub_entities
		};
	}

	macro_rules! impl_fl_others {
		(fl, $fl:expr, $others:expr) => {
			$fl
		};

		($game:ident, $fl:expr, $others:expr) => {
			$others
		};
	}

	macro_rules! impl_game {
		($game:ident) => {
			impl ToQuickEntity for glacier_bin1::game::$game::SEntityTemplateReference {
				type QuickEntity = Option<Ref>;
				type Error = anyhow::Error;

				type Factory = game_types::$game::Factory;
				type Blueprint = game_types::$game::Blueprint;

				#[try_fn]
				#[context("Failed to convert game reference to QN")]
				fn to_qn(
					&self,
					factory: &Self::Factory,
					_factory_meta: &ResourceMetadata,
					blueprint: &Self::Blueprint,
					_: &ResourceMetadata,
					_: bool
				) -> Result<Self::QuickEntity, Self::Error> {
					if self.entity_index == -1 {
						None
					} else {
						Some(Ref {
							entity_id: if self.entity_index == -2 {
								self.entity_id.into()
							} else {
								sub_entities!($game, blueprint)
									.get(self.entity_index as usize)
									.with_context(|| {
										format!("Invalid entity index {} for reference", self.entity_index)
									})?
									.entity_id
									.into()
							},
							external_scene: if self.external_scene_index == -1 {
								None
							} else {
								Some(impl_fl_others!(
									$game,
									factory
										.external_scene_runtime_resource_ids
										.get(self.external_scene_index as usize)
										.context("No such external scene in factory")?
										.as_u64()
										.try_into()
										.context("Invalid external scene ID")?,
									_factory_meta
										.references
										.get(
											factory
												.external_scene_type_indices_in_resource_header
												.get(self.external_scene_index as usize)
												.context("No such external scene in factory")?
												.to_owned() as usize
										)
										.context("External scene type index does not exist in factory metadata")?
										.resource
								))
							},
							exposed_entity: (!self.exposed_entity.is_empty()).then(|| self.exposed_entity.to_owned())
						})
					}
				}
			}

			impl FromQuickEntity<Ref> for glacier_bin1::game::$game::SEntityTemplateReference {
				type Error = anyhow::Error;

				#[try_fn]
				#[context("Invalid reference")]
				fn from_qn(
					value: &Ref,
					entity_indices: &HashMap<EntityID, usize>,
					_: &HashMap<RuntimeID, usize>,
					external_scene_indices: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					if let Some(external_scene) = &value.external_scene {
						Self {
							entity_id: value.entity_id.as_u64(),
							external_scene_index: external_scene_indices
								.get(&external_scene)
								.copied()
								.with_context(|| {
									format!("External scene {external_scene} is not listed in externalScenes")
								})?
								.try_into()?,
							entity_index: -2,
							exposed_entity: value.exposed_entity.to_owned().unwrap_or_default()
						}
					} else {
						Self {
							entity_id: impl_fl_others!($game, value.entity_id.as_u64(), u64::MAX),
							external_scene_index: -1,
							entity_index: entity_indices
								.get(&value.entity_id)
								.with_context(|| format!("Entity {} does not exist", value.entity_id))?
								.to_owned() as i32,
							exposed_entity: value.exposed_entity.to_owned().unwrap_or_default()
						}
					}
				}
			}

			impl FromQuickEntity<Option<Ref>> for glacier_bin1::game::$game::SEntityTemplateReference {
				type Error = anyhow::Error;

				#[try_fn]
				fn from_qn(
					value: &Option<Ref>,
					entity_indices: &HashMap<EntityID, usize>,
					reference_indices: &HashMap<RuntimeID, usize>,
					external_scene_indices: &HashMap<RuntimeID, usize>
				) -> Result<Self, Self::Error> {
					match value {
						None => Self {
							entity_id: u64::MAX,
							external_scene_index: -1,
							entity_index: -1,
							exposed_entity: "".into()
						},

						Some(value) => Self::from_qn(value, entity_indices, reference_indices, external_scene_indices)?
					}
				}
			}
		};
	}

	#[cfg(feature = "h1")]
	impl_game!(h1);

	#[cfg(feature = "h2")]
	impl_game!(h2);

	#[cfg(feature = "h3")]
	impl_game!(h3);

	#[cfg(feature = "fl")]
	impl_game!(fl);
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Type)]
#[serde(untagged)]
enum RefProxy {
	Short(EntityID),
	Full {
		#[serde(rename = "ref")]
		entity_id: EntityID,

		#[serde(rename = "externalScene")]
		#[serde(skip_serializing_if = "Option::is_none")]
		external_scene: Option<RuntimeID>,

		#[serde(rename = "exposedEntity")]
		#[serde(skip_serializing_if = "Option::is_none")]
		#[specta(type = Option<String>)]
		#[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
		exposed_entity: Option<EcoString>
	}
}

impl From<Ref> for RefProxy {
	fn from(value: Ref) -> Self {
		if value.external_scene.is_some() || value.exposed_entity.is_some() {
			Self::Full {
				entity_id: value.entity_id,
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
			RefProxy::Short(entity_id) => Self {
				entity_id,
				external_scene: None,
				exposed_entity: None
			},

			RefProxy::Full {
				entity_id,
				external_scene,
				exposed_entity
			} => Self {
				entity_id,
				external_scene,
				exposed_entity
			}
		}
	}
}

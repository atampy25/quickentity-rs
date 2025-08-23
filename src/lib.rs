#![feature(try_find)]

pub mod entity;
pub mod patch;

use anyhow::{Context, Error, Result, anyhow, bail};
use auto_context::auto_context;
use core::hash::Hash;
use ecow::EcoString;
use fn_error_context::context;
use hitman_bin1::{
	game::h3::{
		SColorRGB, SColorRGBA, SEntityTemplateEntitySubset, SEntityTemplateExposedEntity, SEntityTemplatePinConnection,
		SEntityTemplatePlatformSpecificProperty, SEntityTemplateProperty, SEntityTemplatePropertyAlias,
		SEntityTemplatePropertyOverride, SEntityTemplateReference, SExternalEntityTemplatePinConnection, SMatrix43,
		STemplateBlueprintSubEntity, STemplateEntityBlueprint, STemplateEntityFactory, STemplateFactorySubEntity,
		ZGuid, ZVariant
	},
	types::{property::PropertyID, repository::ZRepositoryID, resource::ZRuntimeResourceID, variant::Variant}
};
use hitman_commons::metadata::{PathedID, ResourceMetadata, ResourceReference};
use itertools::Itertools;
use ordermap::OrderMap;
use rayon::prelude::*;
use serde_json::{Value, from_value, json, to_string, to_value};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::{
	collections::{HashMap, HashSet},
	ops::Deref,
	str::FromStr
};
use tryvial::try_fn;

use entity::{
	Entity, EntityID, ExposedEntity, PinConnection, PinConnectionOverride, PinConnectionOverrideDelete, Property,
	PropertyAlias, PropertyOverride, Ref, SimpleProperty, SubEntity, SubType
};
use patch::{
	ArrayPatchOperation, Patch, PatchOperation, PropertyOverrideConnection, SetPlatformSpecificPropertyValue,
	SetPropertyValue, SubEntityOperation
};

pub const PATCH_VERSION: u8 = 7;
pub const QN_VERSION: f32 = 3.2;

pub const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;
pub const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

// TODO: Array patches for property override properties? Simple properties in general?

#[cfg(feature = "rune")]
pub fn rune_install(ctx: &mut rune::Context) -> Result<(), rune::ContextError> {
	ctx.install(entity::rune_module()?)?;
	ctx.install(patch::rune_module()?)?;

	let mut module = rune::Module::with_crate("quickentity_rs")?;
	module.function_meta(apply_patch__meta)?;
	module.function_meta(generate_patch__meta)?;
	module.function_meta(r_convert_to_qn)?;
	module.function_meta(r_convert_to_rl)?;
	ctx.install(module)?;

	Ok(())
}

// Why is this not in the standard library
trait TryAllTryPos: Iterator {
	fn try_all<F>(&mut self, f: F) -> Result<bool>
	where
		F: FnMut(Self::Item) -> Result<bool>;

	fn try_position<F>(&mut self, f: F) -> Result<Option<usize>>
	where
		F: FnMut(Self::Item) -> Result<bool>;
}

impl<T: Sized> TryAllTryPos for T
where
	T: Iterator
{
	#[context("Failure in try_all")]
	fn try_all<F>(&mut self, mut f: F) -> Result<bool>
	where
		F: FnMut(Self::Item) -> Result<bool>
	{
		for x in self {
			if !(f(x)?) {
				return Ok(false);
			}
		}

		Ok(true)
	}

	#[context("Failure in try_position")]
	fn try_position<F>(&mut self, mut f: F) -> Result<Option<usize>>
	where
		F: FnMut(Self::Item) -> Result<bool>
	{
		for (i, x) in self.enumerate() {
			if f(x)? {
				return Ok(Some(i));
			}
		}

		Ok(None)
	}
}

trait PermissiveUnwrap {
	/// Throw away the value of this Option. If it was None, return Err or Ok depending on whether permissive mode is enabled, for use with `?`. If it was Some, return Ok.
	fn permit(&self, permissive: bool, message: &str) -> Result<()>;
}

impl<T> PermissiveUnwrap for Option<T> {
	#[context("Permissive unwrap failure")]
	fn permit(&self, permissive: bool, message: &str) -> Result<()> {
		if self.is_none() {
			if permissive {
				log::warn!("QuickEntity warning: {message}");

				Ok(())
			} else {
				Err(anyhow!("Non-permissive mode error: {message}"))
			}
		} else {
			Ok(())
		}
	}
}

// A frankly terrible implementation of Hash and PartialOrd/Ord for Value
#[derive(Debug, Clone)]
struct DiffableValue {
	value: Value,
	text: String
}

impl DiffableValue {
	fn new(value: Value) -> Self {
		let text = to_string(&value).unwrap_or_default();

		Self { value, text }
	}
}

impl Hash for DiffableValue {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.text.hash(state);
	}
}

impl PartialEq for DiffableValue {
	fn eq(&self, other: &Self) -> bool {
		self.text == other.text
	}
}

impl Eq for DiffableValue {}

impl PartialOrd for DiffableValue {
	fn ge(&self, other: &Self) -> bool {
		self.text.ge(&other.text)
	}

	fn le(&self, other: &Self) -> bool {
		self.text.le(&other.text)
	}

	fn gt(&self, other: &Self) -> bool {
		self.text.gt(&other.text)
	}

	fn lt(&self, other: &Self) -> bool {
		self.text.lt(&other.text)
	}

	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for DiffableValue {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.text.cmp(&other.text)
	}
}

// TODO: Use for array patches and pin connections
#[try_fn]
#[context("Failure checking property is roughly identical")]
#[auto_context]
fn property_is_roughly_identical(p1_type: &str, p1_value: &Value, p2_type: &str, p2_value: &Value) -> Result<bool> {
	p1_type == p2_type && {
		if p1_value.is_array() {
			let mut single_ty = p1_type.chars();
			single_ty.nth(6); // discard TArray<
			single_ty.next_back(); // discard closing >
			let single_ty = single_ty.collect::<String>();

			let p1_arr = p1_value.as_array().ctx?;
			let p2_arr = p2_value.as_array().ctx?;

			p1_arr.len() == p2_arr.len()
				&& p1_arr
					.iter()
					.zip(p2_arr)
					.try_all(|(x, y)| property_is_roughly_identical(&single_ty, x, &single_ty, y))?
		} else if p1_type == "TPair<ZString,ZVariant>" {
			let p1_arr = p1_value.as_array().ctx?;
			let p2_arr = p2_value.as_array().ctx?;

			p1_arr.len() == 2
				&& p2_arr.len() == 2
				&& property_is_roughly_identical("ZString", &p1_arr[0], "ZString", &p2_arr[0])?
				&& property_is_roughly_identical(
					p1_arr[1].get("type").ctx?.as_str().ctx?,
					p1_arr[1].get("value").ctx?,
					p2_arr[1].get("type").ctx?.as_str().ctx?,
					p2_arr[1].get("value").ctx?
				)?
		} else if p1_type == "SMatrix43" {
			let p1 = p1_value.as_object().ctx?;
			let p2 = p2_value.as_object().ctx?;

			// scale X, Y and Z have the same values (to 2 decimal places) or if either scale doesn't exist assume they're the same
			let scales_roughly_identical = if p1.get("scale").is_some() && p2.get("scale").is_some() {
				let p1_scale = &p1["scale"];
				let p2_scale = &p2["scale"];

				format!("{:.2}", p1_scale.get("x").ctx?.as_f64().ctx?)
					== format!("{:.2}", p2_scale.get("x").ctx?.as_f64().ctx?)
					&& format!("{:.2}", p1_scale.get("y").ctx?.as_f64().ctx?)
						== format!("{:.2}", p2_scale.get("y").ctx?.as_f64().ctx?)
					&& format!("{:.2}", p1_scale.get("z").ctx?.as_f64().ctx?)
						== format!("{:.2}", p2_scale.get("z").ctx?.as_f64().ctx?)
			} else {
				true
			};

			p1.get("rotation").ctx? == p2.get("rotation").ctx?
				&& p1.get("position").ctx? == p2.get("position").ctx?
				&& scales_roughly_identical
		} else if p1_type == "SEntityTemplateReference" {
			from_value::<Ref>(p1_value.to_owned())? == from_value::<Ref>(p2_value.to_owned())?
		} else {
			p1_value == p2_value
		}
	}
}

#[try_fn]
#[context("Failure applying patch to entity")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[cfg_attr(feature = "rune", rune::function(keep))]
pub fn apply_patch(entity: &mut Entity, patch: Patch, permissive: bool) -> Result<()> {
	if patch.patch_version != PATCH_VERSION {
		bail!(
			"Invalid patch version; expected {}, got {}",
			PATCH_VERSION,
			patch.patch_version
		);
	}

	let patch: Vec<PatchOperation> = patch.patch;

	let pool = rayon::ThreadPoolBuilder::new().build()?;
	pool.install(|| {
		for operation in patch {
			match operation {
				PatchOperation::SetRootEntity(value) => {
					entity.root_entity = value;
				}

				PatchOperation::SetSubType(value) => {
					entity.sub_type = value;
				}

				PatchOperation::RemoveEntityByID(value) => {
					entity.entities.remove(&value).permit(
						permissive,
						"Couldn't remove entity by ID because entity did not exist in target!"
					)?;
				}

				PatchOperation::AddEntity(id, data) => {
					entity.entities.insert(id, *data);
				}

				PatchOperation::SubEntityOperation(entity_id, op) => {
					let entity = entity
						.entities
						.get_mut(&entity_id)
						.with_context(|| format!("SubEntityOperation couldn't find entity ID: {entity_id}!"))?;

					match op {
						SubEntityOperation::SetParent(value) => {
							entity.parent = value;
						}

						SubEntityOperation::SetName(value) => {
							entity.name = value;
						}

						SubEntityOperation::SetFactory(value) => {
							entity.factory = value;
						}

						SubEntityOperation::SetBlueprint(value) => {
							entity.blueprint = value;
						}

						SubEntityOperation::SetEditorOnly(value) => {
							entity.editor_only = value;
						}

						SubEntityOperation::AddProperty(name, data) => {
							entity.properties.insert(name, data);
						}

						SubEntityOperation::RemovePropertyByName(name) => {
							entity
								.properties
								.remove(&name)
								.permit(permissive, "RemovePropertyByName couldn't find expected property!")?;
						}

						SubEntityOperation::SetPropertyType(name, value) => {
							entity
								.properties
								.get_mut(&name)
								.context("SetPropertyType couldn't find expected property!")?
								.property_type = value;
						}

						SubEntityOperation::SetPropertyValue(SetPropertyValue { property_name, value }) => {
							entity
								.properties
								.get_mut(&property_name)
								.context("SetPropertyValue couldn't find expected property!")?
								.value = value;
						}

						SubEntityOperation::PatchArrayPropertyValue(property_name, array_patch) => {
							let item_to_patch = entity
								.properties
								.get_mut(&property_name)
								.context("PatchArrayPropertyValue couldn't find expected property!")?;

							apply_array_patch(
								&mut item_to_patch.value,
								array_patch,
								permissive,
								item_to_patch.property_type == "TArray<SEntityTemplateReference>"
							)?;
						}

						SubEntityOperation::SetPropertyPostInit(name, value) => {
							entity
								.properties
								.get_mut(&name)
								.context("SetPropertyPostInit couldn't find expected property!")?
								.post_init = value;
						}

						SubEntityOperation::AddPlatformSpecificProperty(platform, name, data) => {
							entity
								.platform_specific_properties
								.entry(platform)
								.or_default()
								.insert(name, data);
						}

						SubEntityOperation::RemovePlatformSpecificPropertiesForPlatform(name) => {
							entity.platform_specific_properties.remove(&name).permit(
								permissive,
								"RemovePSPropertiesForPlatform couldn't find platform to remove!"
							)?;
						}

						SubEntityOperation::RemovePlatformSpecificPropertyByName(platform, name) => {
							entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("RemovePSPropertyByName couldn't find platform!")?
								.remove(&name)
								.permit(permissive, "RemovePSPropertyByName couldn't find property to remove!")?;

							if entity.platform_specific_properties.get(&platform).ctx?.is_empty() {
								entity.platform_specific_properties.remove(&platform);
							}
						}

						SubEntityOperation::SetPlatformSpecificPropertyType(platform, name, value) => {
							entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("SetPSPropertyType couldn't find expected platform!")?
								.get_mut(&name)
								.context("SetPSPropertyType couldn't find expected property!")?
								.property_type = value;
						}

						SubEntityOperation::SetPlatformSpecificPropertyValue(SetPlatformSpecificPropertyValue {
							platform,
							property_name,
							value
						}) => {
							entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("SetPSPropertyValue couldn't find expected platform!")?
								.get_mut(&property_name)
								.context("SetPSPropertyValue couldn't find expected property!")?
								.value = value;
						}

						SubEntityOperation::PatchPlatformSpecificArrayPropertyValue(
							platform,
							property_name,
							array_patch
						) => {
							let item_to_patch = entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("PatchPSArrayPropertyValue couldn't find expected platform!")?
								.get_mut(&property_name)
								.context("PatchPSArrayPropertyValue couldn't find expected property!")?;

							apply_array_patch(
								&mut item_to_patch.value,
								array_patch,
								permissive,
								item_to_patch.property_type == "TArray<SEntityTemplateReference>"
							)?;
						}

						SubEntityOperation::SetPlatformSpecificPropertyPostInit(platform, name, value) => {
							entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("SetPSPropertyPostInit couldn't find expected platform!")?
								.get_mut(&name)
								.context("SetPSPropertyPostInit couldn't find expected property!")?
								.post_init = value;
						}

						SubEntityOperation::RemoveAllEventConnectionsForEvent(event) => {
							entity
								.events
								.remove(&event)
								.context("RemoveAllEventConnectionsForEvent couldn't find event!")?;
						}

						SubEntityOperation::RemoveAllEventConnectionsForTrigger(event, trigger) => {
							entity
								.events
								.get_mut(&event)
								.context("RemoveAllEventConnectionsForTrigger couldn't find event!")?
								.remove(&trigger)
								.context("RemoveAllEventConnectionsForTrigger couldn't find trigger!")?;

							if entity.events.get(&event).ctx?.is_empty() {
								entity.events.remove(&event);
							}
						}

						SubEntityOperation::RemoveEventConnection(event, trigger, reference) => {
							let ind = entity
								.events
								.get(&event)
								.context("RemoveEventConnection couldn't find event!")?
								.get(&trigger)
								.context("RemoveEventConnection couldn't find trigger!")?
								.iter()
								.position(|x| *x == reference)
								.context("RemoveEventConnection couldn't find reference!")?;

							entity.events.get_mut(&event).ctx?.get_mut(&trigger).ctx?.remove(ind);

							if entity.events.get(&event).ctx?.get(&trigger).ctx?.is_empty() {
								entity.events.get_mut(&event).ctx?.remove(&trigger);
							}

							if entity.events.get(&event).ctx?.is_empty() {
								entity.events.remove(&event);
							}
						}

						SubEntityOperation::AddEventConnection(event, trigger, reference) => {
							if entity.events.get(&event).is_none() {
								entity.events.insert(event.to_owned(), Default::default());
							}

							if entity.events.get(&event).ctx?.get(&trigger).is_none() {
								entity
									.events
									.get_mut(&event)
									.ctx?
									.insert(trigger.to_owned(), Default::default());
							}

							entity
								.events
								.get_mut(&event)
								.ctx?
								.get_mut(&trigger)
								.ctx?
								.push(reference);
						}

						SubEntityOperation::RemoveAllInputCopyConnectionsForInput(event) => {
							entity
								.input_copying
								.remove(&event)
								.context("RemoveAllInputCopyConnectionsForInput couldn't find input!")?;
						}

						SubEntityOperation::RemoveAllInputCopyConnectionsForTrigger(event, trigger) => {
							entity
								.input_copying
								.get_mut(&event)
								.context("RemoveAllInputCopyConnectionsForTrigger couldn't find input!")?
								.remove(&trigger)
								.context("RemoveAllInputCopyConnectionsForTrigger couldn't find trigger!")?;

							if entity.input_copying.get(&event).ctx?.is_empty() {
								entity.input_copying.remove(&event);
							}
						}

						SubEntityOperation::RemoveInputCopyConnection(event, trigger, reference) => {
							let ind = entity
								.input_copying
								.get(&event)
								.context("RemoveInputCopyConnection couldn't find input!")?
								.get(&trigger)
								.context("RemoveInputCopyConnection couldn't find trigger!")?
								.iter()
								.position(|x| *x == reference)
								.context("RemoveInputCopyConnection couldn't find reference!")?;

							entity
								.input_copying
								.get_mut(&event)
								.ctx?
								.get_mut(&trigger)
								.ctx?
								.remove(ind);

							if entity.input_copying.get(&event).ctx?.get(&trigger).ctx?.is_empty() {
								entity.input_copying.get_mut(&event).ctx?.remove(&trigger);
							}

							if entity.input_copying.get(&event).ctx?.is_empty() {
								entity.input_copying.remove(&event);
							}
						}

						SubEntityOperation::AddInputCopyConnection(event, trigger, reference) => {
							if entity.input_copying.get(&event).is_none() {
								entity.input_copying.insert(event.to_owned(), Default::default());
							}

							if entity.input_copying.get(&event).ctx?.get(&trigger).is_none() {
								entity
									.input_copying
									.get_mut(&event)
									.ctx?
									.insert(trigger.to_owned(), Default::default());
							}

							entity
								.input_copying
								.get_mut(&event)
								.ctx?
								.get_mut(&trigger)
								.ctx?
								.push(reference);
						}

						SubEntityOperation::RemoveAllOutputCopyConnectionsForOutput(event) => {
							entity
								.output_copying
								.remove(&event)
								.context("RemoveAllOutputCopyConnectionsForOutput couldn't find event!")?;
						}

						SubEntityOperation::RemoveAllOutputCopyConnectionsForPropagate(event, trigger) => {
							entity
								.output_copying
								.get_mut(&event)
								.context("RemoveAllOutputCopyConnectionsForPropagate couldn't find event!")?
								.remove(&trigger)
								.context("RemoveAllOutputCopyConnectionsForPropagate couldn't find propagate!")?;

							if entity.output_copying.get(&event).ctx?.is_empty() {
								entity.output_copying.remove(&event);
							}
						}

						SubEntityOperation::RemoveOutputCopyConnection(event, trigger, reference) => {
							let ind = entity
								.output_copying
								.get(&event)
								.context("RemoveOutputCopyConnection couldn't find event!")?
								.get(&trigger)
								.context("RemoveOutputCopyConnection couldn't find propagate!")?
								.iter()
								.position(|x| *x == reference)
								.context("RemoveOutputCopyConnection couldn't find reference!")?;

							entity
								.output_copying
								.get_mut(&event)
								.ctx?
								.get_mut(&trigger)
								.ctx?
								.remove(ind);

							if entity.output_copying.get(&event).ctx?.get(&trigger).ctx?.is_empty() {
								entity.output_copying.get_mut(&event).ctx?.remove(&trigger);
							}

							if entity.output_copying.get(&event).ctx?.is_empty() {
								entity.output_copying.remove(&event);
							}
						}

						SubEntityOperation::AddOutputCopyConnection(event, trigger, reference) => {
							if entity.output_copying.get(&event).is_none() {
								entity.output_copying.insert(event.to_owned(), Default::default());
							}

							if entity.output_copying.get(&event).ctx?.get(&trigger).is_none() {
								entity
									.output_copying
									.get_mut(&event)
									.ctx?
									.insert(trigger.to_owned(), Default::default());
							}

							entity
								.output_copying
								.get_mut(&event)
								.ctx?
								.get_mut(&trigger)
								.ctx?
								.push(reference);
						}

						SubEntityOperation::AddPropertyAliasConnection(alias, data) => {
							entity.property_aliases.entry(alias).or_default().push(data);
						}

						SubEntityOperation::RemovePropertyAlias(alias) => {
							entity
								.property_aliases
								.remove(&alias)
								.context("RemovePropertyAlias couldn't find alias!")?;
						}

						SubEntityOperation::RemoveConnectionForPropertyAlias(alias, data) => {
							let connection = entity
								.property_aliases
								.get(&alias)
								.context("RemoveConnectionForPropertyAlias couldn't find alias!")?
								.iter()
								.position(|x| *x == data)
								.context("RemoveConnectionForPropertyAlias couldn't find connection!")?;

							entity.property_aliases.get_mut(&alias).ctx?.remove(connection);

							if entity.property_aliases.get(&alias).ctx?.is_empty() {
								entity.property_aliases.remove(&alias);
							}
						}

						SubEntityOperation::SetExposedEntity(name, data) => {
							entity.exposed_entities.insert(name, data);
						}

						SubEntityOperation::RemoveExposedEntity(name) => {
							entity
								.exposed_entities
								.remove(&name)
								.context("RemoveExposedEntity couldn't find exposed entity to remove!")?;
						}

						SubEntityOperation::SetExposedInterface(name, implementor) => {
							entity.exposed_interfaces.insert(name, implementor);
						}

						SubEntityOperation::RemoveExposedInterface(name) => {
							entity
								.exposed_interfaces
								.remove(&name)
								.context("RemoveExposedInterface couldn't find exposed entity to remove!")?;
						}

						SubEntityOperation::AddSubset(name, ent) => {
							entity.subsets.entry(name).or_default().push(ent);
						}

						SubEntityOperation::RemoveSubset(name, ent) => {
							let ind = entity
								.subsets
								.get(&name)
								.context("RemoveSubset couldn't find subset to remove from!")?
								.iter()
								.position(|x| *x == ent)
								.context("RemoveSubset couldn't find the entity to remove from the subset!")?;

							entity.subsets.get_mut(&name).ctx?.remove(ind);
						}

						SubEntityOperation::RemoveAllSubsetsFor(name) => {
							entity
								.subsets
								.remove(&name)
								.context("RemoveAllSubsetsFor couldn't find subset to remove!")?;
						}
					}
				}

				#[allow(deprecated)]
				PatchOperation::AddPropertyOverride(value) => {
					entity.property_overrides.push(value);
				}

				#[allow(deprecated)]
				PatchOperation::RemovePropertyOverride(value) => {
					entity.property_overrides.remove(
						entity
							.property_overrides
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemovePropertyOverride couldn't find expected value!")?
					);
				}

				PatchOperation::AddPropertyOverrideConnection(value) => {
					let mut unravelled_overrides: Vec<PropertyOverride> = vec![];

					for property_override in &entity.property_overrides {
						for ent in &property_override.entities {
							for (prop_name, prop_override) in &property_override.properties {
								unravelled_overrides.push(PropertyOverride {
									entities: vec![ent.to_owned()],
									properties: {
										let mut x = OrderMap::new();
										x.insert(prop_name.to_owned(), prop_override.to_owned());
										x
									}
								});
							}
						}
					}

					unravelled_overrides.push(PropertyOverride {
						entities: vec![value.entity],
						properties: {
							let mut x = OrderMap::new();
							x.insert(value.property_name.to_owned(), value.property_override.to_owned());
							x
						}
					});

					let mut merged_overrides: Vec<PropertyOverride> = vec![];

					let mut pass1: Vec<PropertyOverride> = Vec::default();

					for property_override in unravelled_overrides {
						// if same entity being overridden, merge props
						if let Some(found) = pass1.iter_mut().find(|x| x.entities == property_override.entities) {
							found.properties.extend(property_override.properties);
						} else {
							pass1.push(PropertyOverride {
								entities: property_override.entities,
								properties: property_override.properties
							});
						}
					}

					// merge entities when same props being overridden
					for property_override in pass1 {
						if let Some(found) = merged_overrides.iter_mut().try_find(|x| -> Result<bool> {
							let contain_same_keys = x
								.properties
								.iter()
								.all(|(y, _)| property_override.properties.contains_key(y))
								&& property_override
									.properties
									.iter()
									.all(|(y, _)| x.properties.contains_key(y));

							// short-circuit
							if !contain_same_keys {
								return Ok(false);
							}

							let values_identical =
								x.properties.iter().try_all(|(prop_name, prop_val)| -> Result<bool> {
									property_is_roughly_identical(
										&prop_val.property_type,
										&prop_val.value,
										&property_override.properties[prop_name].property_type,
										&property_override.properties[prop_name].value
									)
								})?;

							// Properties are identical when they contain the same properties and each property's value is roughly identical
							Ok(values_identical)
						})? {
							found.entities.extend(property_override.entities);
						} else {
							merged_overrides.push(property_override);
						}
					}

					entity.property_overrides = merged_overrides;
				}

				PatchOperation::RemovePropertyOverrideConnection(value) => {
					let mut unravelled_overrides: Vec<PropertyOverride> = vec![];

					for property_override in &entity.property_overrides {
						for ent in &property_override.entities {
							for (prop_name, prop_override) in &property_override.properties {
								unravelled_overrides.push(PropertyOverride {
									entities: vec![ent.to_owned()],
									properties: {
										let mut x = OrderMap::new();
										x.insert(prop_name.to_owned(), prop_override.to_owned());
										x
									}
								});
							}
						}
					}

					let search = PropertyOverride {
						entities: vec![value.entity.to_owned()],
						properties: {
							let mut x = OrderMap::new();
							x.insert(value.property_name.to_owned(), value.property_override.to_owned());
							x
						}
					};

					let mut retain_result = Ok(());
					unravelled_overrides.retain(|x| {
						x.entities != search.entities
							|| !x.properties.contains_key(&value.property_name)
							|| !{
								match property_is_roughly_identical(
									&x.properties[&value.property_name].property_type,
									&x.properties[&value.property_name].value,
									&value.property_override.property_type,
									&value.property_override.value
								) {
									Ok(x) => x,
									Err(e) => {
										retain_result = Err(e);
										false
									}
								}
							}
					});
					retain_result?;

					let mut merged_overrides: Vec<PropertyOverride> = vec![];

					let mut pass1: Vec<PropertyOverride> = Vec::default();

					for property_override in unravelled_overrides {
						// if same entity being overridden, merge props
						if let Some(found) = pass1.iter_mut().find(|x| x.entities == property_override.entities) {
							found.properties.extend(property_override.properties);
						} else {
							pass1.push(PropertyOverride {
								entities: property_override.entities,
								properties: property_override.properties
							});
						}
					}

					// merge entities when same props being overridden
					for property_override in pass1 {
						if let Some(found) = merged_overrides.iter_mut().try_find(|x| -> Result<bool> {
							let contain_same_keys = x
								.properties
								.iter()
								.all(|(y, _)| property_override.properties.contains_key(y))
								&& property_override
									.properties
									.iter()
									.all(|(y, _)| x.properties.contains_key(y));

							// short-circuit
							if !contain_same_keys {
								return Ok(false);
							}

							let values_identical = x
								.properties
								.iter()
								.try_find(|(prop_name, prop_val)| -> Result<bool> {
									Ok(!(property_is_roughly_identical(
										&prop_val.property_type,
										&prop_val.value,
										&property_override.properties[*prop_name].property_type,
										&property_override.properties[*prop_name].value
									))?)
								})?
								.is_none();

							// Properties are identical when they contain the same properties and each property's value is roughly identical
							Ok(values_identical)
						})? {
							found.entities.extend(property_override.entities);
						} else {
							merged_overrides.push(property_override);
						}
					}

					entity.property_overrides = merged_overrides;
				}

				PatchOperation::AddOverrideDelete(value) => {
					entity.override_deletes.push(value);
				}

				PatchOperation::RemoveOverrideDelete(value) => {
					entity.override_deletes.remove(
						entity
							.override_deletes
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemoveOverrideDelete couldn't find expected value!")?
					);
				}

				PatchOperation::AddPinConnectionOverride(value) => {
					entity.pin_connection_overrides.push(value);
				}

				PatchOperation::RemovePinConnectionOverride(value) => {
					entity.pin_connection_overrides.remove(
						entity
							.pin_connection_overrides
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemovePinConnectionOverride couldn't find expected value!")?
					);
				}

				PatchOperation::AddPinConnectionOverrideDelete(value) => {
					entity.pin_connection_override_deletes.push(value);
				}

				PatchOperation::RemovePinConnectionOverrideDelete(value) => {
					entity.pin_connection_override_deletes.remove(
						entity
							.pin_connection_override_deletes
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemovePinConnectionOverrideDelete couldn't find expected value!")?
					);
				}

				PatchOperation::AddExternalScene(value) => {
					entity.external_scenes.push(value);
				}

				PatchOperation::RemoveExternalScene(value) => {
					if let Some(x) = entity.external_scenes.par_iter().position_any(|x| *x == value) {
						entity.external_scenes.remove(x);
					} else if permissive {
						log::warn!("QuickEntity warning: RemoveExternalScene couldn't find expected value!");
					} else {
						bail!("RemoveExternalScene couldn't find expected value!");
					}
				}

				PatchOperation::AddExtraFactoryReference(value) => {
					entity.extra_factory_references.push(value);
				}

				PatchOperation::RemoveExtraFactoryReference(value) => {
					entity.extra_factory_references.remove(
						entity
							.extra_factory_references
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemoveExtraFactoryDependency couldn't find expected value!")?
					);
				}

				PatchOperation::AddExtraBlueprintReference(value) => {
					entity.extra_blueprint_references.push(value);
				}

				PatchOperation::RemoveExtraBlueprintReference(value) => {
					entity.extra_blueprint_references.remove(
						entity
							.extra_blueprint_references
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemoveExtraBlueprintDependency couldn't find expected value!")?
					);
				}

				PatchOperation::AddComment(value) => {
					entity.comments.push(value);
				}

				PatchOperation::RemoveComment(value) => {
					entity.comments.remove(
						entity
							.comments
							.par_iter()
							.position_any(|x| *x == value)
							.context("RemoveComment couldn't find expected value!")?
					);
				}
			}
		}

		Ok(())
	})?;
}

#[try_fn]
#[context("Failure applying array patch")]
pub fn apply_array_patch(
	arr: &mut Value,
	patch: Vec<ArrayPatchOperation>,
	permissive: bool,
	is_ref_array: bool
) -> Result<()> {
	let arr = arr
		.as_array_mut()
		.context("Array patch was given a non-array value to patch!")?;

	if is_ref_array {
		// It's not unnecessary because what Clippy suggests causes an error due to the borrow from .iter().cloned()
		#[allow(clippy::unnecessary_to_owned)]
		for (index, elem) in arr.to_owned().into_iter().enumerate() {
			arr[index] = to_value(&from_value::<Ref>(elem)?)?;
		}
	}

	for op in patch {
		match op {
			ArrayPatchOperation::RemoveItemByValue(mut val) => {
				if is_ref_array {
					val = to_value(&from_value::<Ref>(val)?)?;
				}

				arr.retain(|x| *x != val);
			}

			ArrayPatchOperation::AddItemAfter(mut val, mut new) => {
				if is_ref_array {
					val = to_value(&from_value::<Ref>(val)?)?;
					new = to_value(&from_value::<Ref>(new)?)?;
				}

				let new = new.to_owned();

				if let Some(pos) = arr.iter().position(|x| *x == val) {
					arr.insert(pos + 1, new);
				} else if permissive {
					log::warn!("QuickEntity warning: couldn't find value to add after in array patch");
					arr.push(new);
				} else {
					bail!("Couldn't find value to add after in array patch!");
				}
			}

			ArrayPatchOperation::AddItemBefore(mut val, mut new) => {
				if is_ref_array {
					val = to_value(&from_value::<Ref>(val)?)?;
					new = to_value(&from_value::<Ref>(new)?)?;
				}

				let new = new.to_owned();

				if let Some(pos) = arr.iter().position(|x| *x == val) {
					arr.insert(pos, new);
				} else if permissive {
					log::warn!("QuickEntity warning: couldn't find value to add before in array patch");
					arr.push(new);
				} else {
					bail!("Couldn't find value to add before in array patch!");
				}
			}

			ArrayPatchOperation::AddItem(mut val) => {
				if is_ref_array {
					val = to_value(&from_value::<Ref>(val)?)?;
				}

				arr.push(val);
			}
		}
	}
}

#[try_fn]
#[context("Failure generating patch from two entities")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[cfg_attr(feature = "rune", rune::function(keep))]
pub fn generate_patch(original: &Entity, modified: &Entity) -> Result<Patch> {
	if original.quick_entity_version != modified.quick_entity_version {
		bail!("Can't create patches between differing QuickEntity versions!")
	}

	let mut patch: Vec<PatchOperation> = vec![];

	if original.root_entity != modified.root_entity {
		patch.push(PatchOperation::SetRootEntity(modified.root_entity.to_owned()));
	}

	if original.sub_type != modified.sub_type {
		patch.push(PatchOperation::SetSubType(modified.sub_type.to_owned()));
	}

	for entity_id in original.entities.keys() {
		if !modified.entities.contains_key(entity_id) {
			patch.push(PatchOperation::RemoveEntityByID(entity_id.to_owned()));
		}
	}

	for (entity_id, new_entity_data) in &modified.entities {
		if let Some(old_entity_data) = original.entities.get(entity_id) {
			if old_entity_data.parent != new_entity_data.parent {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetParent(new_entity_data.parent.to_owned())
				));
			}

			if old_entity_data.name != new_entity_data.name {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetName(new_entity_data.name.to_owned())
				));
			}

			if old_entity_data.factory != new_entity_data.factory {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetFactory(new_entity_data.factory.to_owned())
				));
			}

			if old_entity_data.blueprint != new_entity_data.blueprint {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetBlueprint(new_entity_data.blueprint.to_owned())
				));
			}

			if old_entity_data.editor_only != new_entity_data.editor_only {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetEditorOnly(new_entity_data.editor_only.to_owned())
				));
			}

			for property_name in old_entity_data.properties.keys() {
				if !new_entity_data.properties.contains_key(property_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePropertyByName(property_name.to_owned())
					));
				}
			}

			for (property_name, new_property_data) in &new_entity_data.properties {
				if let Some(old_property_data) = old_entity_data.properties.get(property_name) {
					if old_property_data.property_type != new_property_data.property_type {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::SetPropertyType(
								property_name.to_owned(),
								new_property_data.property_type.to_owned()
							)
						));
					}

					if old_property_data.value != new_property_data.value {
						if old_property_data.value.is_array()
							&& new_property_data.value.is_array()
							&& old_property_data.property_type != "ZCurve"
							&& new_property_data.property_type != "ZCurve"
						{
							let old_value = old_property_data
								.value
								.as_array()
								.ctx?
								.iter()
								.map(|x| DiffableValue::new(x.to_owned()))
								.collect::<Vec<_>>();

							let new_value = new_property_data
								.value
								.as_array()
								.ctx?
								.iter()
								.map(|x| DiffableValue::new(x.to_owned()))
								.collect::<Vec<_>>();

							let mut ops = vec![];

							for diff_result in capture_diff_slices(Algorithm::Patience, &old_value, &new_value) {
								match diff_result {
									DiffOp::Replace {
										old_index,
										new_index,
										old_len,
										new_len
									} => {
										for i in 0..old_len {
											ops.push(ArrayPatchOperation::RemoveItemByValue(
												old_value[old_index + i].value.to_owned()
											));
										}

										for i in (0..new_len).rev() {
											if let Some(prev) = old_value.get(old_index.overflowing_sub(1).0) {
												ops.push(ArrayPatchOperation::AddItemAfter(
													prev.value.to_owned(),
													new_value[new_index + i].value.to_owned()
												));
											} else if let Some(next) = old_value.get(old_index + 1) {
												ops.push(ArrayPatchOperation::AddItemBefore(
													next.value.to_owned(),
													new_value[new_index + i].value.to_owned()
												));
											} else {
												ops.push(ArrayPatchOperation::AddItem(
													new_value[new_index + i].value.to_owned()
												));
											}
										}
									}

									DiffOp::Delete { old_index, old_len, .. } => {
										for i in 0..old_len {
											ops.push(ArrayPatchOperation::RemoveItemByValue(
												old_value[old_index + i].value.to_owned()
											));
										}
									}

									DiffOp::Insert {
										old_index,
										new_index,
										new_len
									} => {
										for i in (0..new_len).rev() {
											if let Some(prev) = old_value.get(old_index.overflowing_sub(1).0) {
												ops.push(ArrayPatchOperation::AddItemAfter(
													prev.value.to_owned(),
													new_value[new_index + i].value.to_owned()
												));
											} else if let Some(next) = old_value.first() {
												ops.push(ArrayPatchOperation::AddItemBefore(
													next.value.to_owned(),
													new_value[new_index + i].value.to_owned()
												));
											} else {
												ops.push(ArrayPatchOperation::AddItem(
													new_value[new_index + i].value.to_owned()
												));
											}
										}
									}

									DiffOp::Equal { .. } => {}
								}
							}

							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::PatchArrayPropertyValue(property_name.to_owned(), ops)
							));
						} else {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::SetPropertyValue(SetPropertyValue {
									property_name: property_name.to_owned(),
									value: new_property_data.value.to_owned()
								})
							));
						}
					}

					if old_property_data.post_init != new_property_data.post_init {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::SetPropertyPostInit(
								property_name.to_owned(),
								new_property_data.post_init
							)
						));
					}
				} else {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::AddProperty(property_name.to_owned(), new_property_data.to_owned())
					));
				}
			}

			// Duplicated from above except with an extra layer for platform
			for platform_name in old_entity_data.platform_specific_properties.keys() {
				if !new_entity_data.platform_specific_properties.contains_key(platform_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePlatformSpecificPropertiesForPlatform(platform_name.to_owned())
					));
				}
			}

			for (platform_name, new_properties_data) in &new_entity_data.platform_specific_properties {
				if let Some(old_properties_data) = old_entity_data.platform_specific_properties.get(platform_name) {
					for property_name in old_properties_data.keys() {
						if !new_entity_data.properties.contains_key(property_name) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemovePlatformSpecificPropertyByName(
									platform_name.to_owned(),
									property_name.to_owned()
								)
							));
						}
					}

					for (property_name, new_property_data) in new_properties_data {
						if let Some(old_property_data) = old_properties_data.get(property_name) {
							if old_property_data.property_type != new_property_data.property_type {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyType(
										platform_name.to_owned(),
										property_name.to_owned(),
										new_property_data.property_type.to_owned()
									)
								));
							}

							if old_property_data.value != new_property_data.value {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyValue(
										SetPlatformSpecificPropertyValue {
											platform: platform_name.to_owned(),
											property_name: property_name.to_owned(),
											value: new_property_data.value.to_owned()
										}
									)
								));
							}

							if old_property_data.post_init != new_property_data.post_init {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyPostInit(
										platform_name.to_owned(),
										property_name.to_owned(),
										new_property_data.post_init
									)
								));
							}
						} else {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddPlatformSpecificProperty(
									platform_name.to_owned(),
									property_name.to_owned(),
									new_property_data.to_owned()
								)
							));
						}
					}
				} else {
					for (property_name, new_property_data) in new_properties_data {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::AddPlatformSpecificProperty(
								platform_name.to_owned(),
								property_name.to_owned(),
								new_property_data.to_owned()
							)
						))
					}
				}
			}

			// An egregious amount of code duplication
			for event_name in old_entity_data.events.keys() {
				if !new_entity_data.events.contains_key(event_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllEventConnectionsForEvent(event_name.to_owned())
					));
				}
			}

			for (event_name, new_events_data) in &new_entity_data.events {
				if let Some(old_events_data) = old_entity_data.events.get(event_name) {
					for trigger_name in old_events_data.keys() {
						if !new_events_data.contains_key(trigger_name) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveAllEventConnectionsForTrigger(
									event_name.to_owned(),
									trigger_name.to_owned()
								)
							));
						}
					}

					for (trigger_name, new_refs_data) in new_events_data {
						if let Some(old_refs_data) = old_events_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::RemoveEventConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}

							for i in new_refs_data {
								if !old_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::AddEventConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}
						} else {
							for i in new_refs_data {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::AddEventConnection(
										event_name.to_owned(),
										trigger_name.to_owned(),
										i.to_owned()
									)
								))
							}
						}
					}
				} else {
					for (trigger_name, new_refs_data) in new_events_data {
						for i in new_refs_data {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddEventConnection(
									event_name.to_owned(),
									trigger_name.to_owned(),
									i.to_owned()
								)
							))
						}
					}
				}
			}

			for event_name in old_entity_data.input_copying.keys() {
				if !new_entity_data.input_copying.contains_key(event_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllInputCopyConnectionsForInput(event_name.to_owned())
					));
				}
			}

			for (event_name, new_input_copying_data) in &new_entity_data.input_copying {
				if let Some(old_input_copying_data) = old_entity_data.input_copying.get(event_name) {
					for trigger_name in old_input_copying_data.keys() {
						if !new_input_copying_data.contains_key(trigger_name) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveAllInputCopyConnectionsForTrigger(
									event_name.to_owned(),
									trigger_name.to_owned()
								)
							));
						}
					}

					for (trigger_name, new_refs_data) in new_input_copying_data {
						if let Some(old_refs_data) = old_input_copying_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::RemoveInputCopyConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}

							for i in new_refs_data {
								if !old_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::AddInputCopyConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}
						} else {
							for i in new_refs_data {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::AddInputCopyConnection(
										event_name.to_owned(),
										trigger_name.to_owned(),
										i.to_owned()
									)
								))
							}
						}
					}
				} else {
					for (trigger_name, new_refs_data) in new_input_copying_data {
						for i in new_refs_data {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddInputCopyConnection(
									event_name.to_owned(),
									trigger_name.to_owned(),
									i.to_owned()
								)
							))
						}
					}
				}
			}

			for event_name in old_entity_data.output_copying.keys() {
				if !new_entity_data.output_copying.contains_key(event_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllOutputCopyConnectionsForOutput(event_name.to_owned())
					));
				}
			}

			for (event_name, new_output_copying_data) in &new_entity_data.output_copying {
				if let Some(old_output_copying_data) = old_entity_data.output_copying.get(event_name) {
					for trigger_name in old_output_copying_data.keys() {
						if !new_output_copying_data.contains_key(trigger_name) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveAllOutputCopyConnectionsForPropagate(
									event_name.to_owned(),
									trigger_name.to_owned()
								)
							));
						}
					}

					for (trigger_name, new_refs_data) in new_output_copying_data {
						if let Some(old_refs_data) = old_output_copying_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::RemoveOutputCopyConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									));
								}
							}

							for i in new_refs_data {
								if !old_refs_data.contains(i) {
									patch.push(PatchOperation::SubEntityOperation(
										entity_id.to_owned(),
										SubEntityOperation::AddOutputCopyConnection(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									));
								}
							}
						} else {
							for i in new_refs_data {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::AddOutputCopyConnection(
										event_name.to_owned(),
										trigger_name.to_owned(),
										i.to_owned()
									)
								));
							}
						}
					}
				} else {
					for (trigger_name, new_refs_data) in new_output_copying_data {
						for i in new_refs_data {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddOutputCopyConnection(
									event_name.to_owned(),
									trigger_name.to_owned(),
									i.to_owned()
								)
							));
						}
					}
				}
			}

			for alias_name in old_entity_data.property_aliases.keys() {
				if !new_entity_data.property_aliases.contains_key(alias_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePropertyAlias(alias_name.to_owned())
					));
				}
			}

			for (alias_name, new_alias_connections) in &new_entity_data.property_aliases {
				if let Some(old_alias_connections) = old_entity_data.property_aliases.get(alias_name) {
					for connection in new_alias_connections {
						if !old_alias_connections.contains(connection) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddPropertyAliasConnection(
									alias_name.to_owned(),
									connection.to_owned()
								)
							));
						}
					}

					for connection in old_alias_connections {
						if !new_alias_connections.contains(connection) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveConnectionForPropertyAlias(
									alias_name.to_owned(),
									connection.to_owned()
								)
							));
						}
					}
				} else {
					for connection in new_alias_connections {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::AddPropertyAliasConnection(
								alias_name.to_owned(),
								connection.to_owned()
							)
						));
					}
				}
			}

			for exposed_entity in old_entity_data.exposed_entities.keys() {
				if !new_entity_data.exposed_entities.contains_key(exposed_entity) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedEntity(exposed_entity.to_owned())
					));
				}
			}

			for (exposed_entity, data) in &new_entity_data.exposed_entities {
				if !old_entity_data.exposed_entities.contains_key(exposed_entity)
					|| old_entity_data.exposed_entities.get(exposed_entity).ctx? != data
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedEntity(exposed_entity.to_owned(), data.to_owned())
					));
				}
			}

			for exposed_interface in old_entity_data.exposed_interfaces.keys() {
				if !new_entity_data.exposed_interfaces.contains_key(exposed_interface) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedInterface(exposed_interface.to_owned())
					));
				}
			}

			for (exposed_interface, data) in &new_entity_data.exposed_interfaces {
				if !old_entity_data.exposed_interfaces.contains_key(exposed_interface)
					|| old_entity_data.exposed_interfaces.get(exposed_interface).ctx? != data
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedInterface(exposed_interface.to_owned(), data.to_owned())
					));
				}
			}

			for subset_name in old_entity_data.subsets.keys() {
				if !new_entity_data.subsets.contains_key(subset_name) {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllSubsetsFor(subset_name.to_owned())
					));
				}
			}

			for (subset_name, new_refs_data) in &new_entity_data.subsets {
				if let Some(old_refs_data) = old_entity_data.subsets.get(subset_name) {
					for i in old_refs_data {
						if !new_refs_data.contains(i) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveSubset(subset_name.to_owned(), i.to_owned())
							));
						}
					}

					for i in new_refs_data {
						if !old_refs_data.contains(i) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::AddSubset(subset_name.to_owned(), i.to_owned())
							));
						}
					}
				} else {
					for i in new_refs_data {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::AddSubset(subset_name.to_owned(), i.to_owned())
						));
					}
				}
			}
		} else {
			patch.push(PatchOperation::AddEntity(
				entity_id.to_owned(),
				new_entity_data.to_owned().into()
			));
		}
	}

	let original_unravelled_overrides: Vec<PropertyOverrideConnection> = original
		.property_overrides
		.iter()
		.flat_map(|property_override| {
			property_override
				.entities
				.iter()
				.flat_map(|ent| {
					property_override
						.properties
						.iter()
						.map(|(prop_name, prop_val)| PropertyOverrideConnection {
							entity: ent.to_owned(),
							property_name: prop_name.to_owned(),
							property_override: prop_val.to_owned()
						})
						.collect_vec()
				})
				.collect_vec()
		})
		.collect();

	let modified_unravelled_overrides: Vec<PropertyOverrideConnection> = modified
		.property_overrides
		.iter()
		.flat_map(|property_override| {
			property_override
				.entities
				.iter()
				.flat_map(|ent| {
					property_override
						.properties
						.iter()
						.map(|(prop_name, prop_val)| PropertyOverrideConnection {
							entity: ent.to_owned(),
							property_name: prop_name.to_owned(),
							property_override: prop_val.to_owned()
						})
						.collect_vec()
				})
				.collect_vec()
		})
		.collect();

	for x in &original_unravelled_overrides {
		if modified_unravelled_overrides
			.iter()
			.try_find(|val| -> Result<bool> {
				Ok(val.entity == x.entity
					&& val.property_name == x.property_name
					&& property_is_roughly_identical(
						&val.property_override.property_type,
						&val.property_override.value,
						&x.property_override.property_type,
						&x.property_override.value
					)?)
			})?
			.is_none()
		{
			patch.push(PatchOperation::RemovePropertyOverrideConnection(x.to_owned()))
		}
	}

	for x in &modified_unravelled_overrides {
		if original_unravelled_overrides
			.iter()
			.try_find(|val| -> Result<bool> {
				Ok(val.entity == x.entity
					&& val.property_name == x.property_name
					&& property_is_roughly_identical(
						&val.property_override.property_type,
						&val.property_override.value,
						&x.property_override.property_type,
						&x.property_override.value
					)?)
			})?
			.is_none()
		{
			patch.push(PatchOperation::AddPropertyOverrideConnection(x.to_owned()))
		}
	}

	for x in &original.override_deletes {
		if !modified.override_deletes.contains(x) {
			patch.push(PatchOperation::RemoveOverrideDelete(x.to_owned()))
		}
	}

	for x in &modified.override_deletes {
		if !original.override_deletes.contains(x) {
			patch.push(PatchOperation::AddOverrideDelete(x.to_owned()))
		}
	}

	for x in &original.pin_connection_overrides {
		if !modified.pin_connection_overrides.contains(x) {
			patch.push(PatchOperation::RemovePinConnectionOverride(x.to_owned()))
		}
	}

	for x in &modified.pin_connection_overrides {
		if !original.pin_connection_overrides.contains(x) {
			patch.push(PatchOperation::AddPinConnectionOverride(x.to_owned()))
		}
	}

	for x in &original.pin_connection_override_deletes {
		if !modified.pin_connection_override_deletes.contains(x) {
			patch.push(PatchOperation::RemovePinConnectionOverrideDelete(x.to_owned()))
		}
	}

	for x in &modified.pin_connection_override_deletes {
		if !original.pin_connection_override_deletes.contains(x) {
			patch.push(PatchOperation::AddPinConnectionOverrideDelete(x.to_owned()))
		}
	}

	for x in &original.external_scenes {
		if !modified.external_scenes.contains(x) {
			patch.push(PatchOperation::RemoveExternalScene(x.to_owned()))
		}
	}

	for x in &modified.external_scenes {
		if !original.external_scenes.contains(x) {
			patch.push(PatchOperation::AddExternalScene(x.to_owned()))
		}
	}

	for x in &original.extra_factory_references {
		if !modified.extra_factory_references.contains(x) {
			patch.push(PatchOperation::RemoveExtraFactoryReference(x.to_owned()))
		}
	}

	for x in &modified.extra_factory_references {
		if !original.extra_factory_references.contains(x) {
			patch.push(PatchOperation::AddExtraFactoryReference(x.to_owned()))
		}
	}

	for x in &original.extra_blueprint_references {
		if !modified.extra_blueprint_references.contains(x) {
			patch.push(PatchOperation::RemoveExtraBlueprintReference(x.to_owned()))
		}
	}

	for x in &modified.extra_blueprint_references {
		if !original.extra_blueprint_references.contains(x) {
			patch.push(PatchOperation::AddExtraBlueprintReference(x.to_owned()))
		}
	}

	for x in &original.comments {
		if !modified.comments.contains(x) {
			patch.push(PatchOperation::RemoveComment(x.to_owned()))
		}
	}

	for x in &modified.comments {
		if !original.comments.contains(x) {
			patch.push(PatchOperation::AddComment(x.to_owned()))
		}
	}

	Patch {
		factory: modified.factory.to_owned(),
		blueprint: modified.blueprint.to_owned(),
		patch,
		patch_version: PATCH_VERSION
	}
}

#[try_fn]
#[context("Failure converting reference to QN")]
fn convert_reference_to_qn(
	reference: &SEntityTemplateReference,
	factory: &STemplateEntityFactory,
	blueprint: &STemplateEntityBlueprint,
	factory_meta: &ResourceMetadata
) -> Result<Option<Ref>> {
	if reference.entity_index == -1 {
		None
	} else {
		Some(Ref {
			entity_id: if reference.entity_index == -2 {
				reference.entity_id.into()
			} else {
				blueprint
					.sub_entities
					.get(reference.entity_index as usize)
					.with_context(|| format!("Invalid entity index {} for reference", reference.entity_index))?
					.entity_id
					.into()
			},
			external_scene: if reference.external_scene_index == -1 {
				None
			} else {
				Some(
					factory_meta
						.references
						.get(
							factory
								.external_scene_type_indices_in_resource_header
								.get(reference.external_scene_index as usize)
								.context("No such external scene in factory")?
								.to_owned() as usize
						)
						.context("External scene type index does not exist in factory metadata")?
						.resource
						.to_owned()
				)
			},
			exposed_entity: (!reference.exposed_entity.is_empty()).then(|| reference.exposed_entity.to_owned().into())
		})
	}
}

#[try_fn]
#[context("Failure converting QN reference to game format")]
fn convert_qn_reference_to_game(
	reference: Option<&Ref>,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	entity_id_to_index_mapping: &HashMap<EntityID, usize>
) -> Result<SEntityTemplateReference> {
	match reference {
		None => SEntityTemplateReference {
			entity_id: u64::MAX,
			external_scene_index: -1,
			entity_index: -1,
			exposed_entity: "".into()
		},

		Some(Ref {
			entity_id,
			external_scene,
			exposed_entity
		}) => {
			if let Some(external_scene) = external_scene {
				SEntityTemplateReference {
					entity_id: entity_id.as_u64(),
					external_scene_index: factory
						.external_scene_type_indices_in_resource_header
						.iter()
						.try_position(|x| {
							Ok(factory_meta.references.get(*x as usize).unwrap().resource == *external_scene)
						})?
						.with_context(|| {
							format!(
								"Can't reference external scene {external_scene} which is not listed in externalScenes"
							)
						})?
						.try_into()?,
					entity_index: -2,
					exposed_entity: exposed_entity.to_owned().unwrap_or_default().into()
				}
			} else {
				SEntityTemplateReference {
					entity_id: u64::MAX,
					external_scene_index: -1,
					entity_index: entity_id_to_index_mapping
						.get(entity_id)
						.with_context(|| format!("Reference referred to a nonexistent entity ID: {entity_id}"))?
						.to_owned() as i32,
					exposed_entity: exposed_entity.to_owned().unwrap_or_default().into()
				}
			}
		}
	}
}

pub fn convert_matrix(value: &SMatrix43, convert_lossless: bool) -> Value {
	// this is all from three.js
	let mut n11 = value.x_axis.x as f64;
	let mut n12 = value.x_axis.y as f64;
	let mut n13 = value.x_axis.z as f64;
	let n14 = 0.0;
	let n21 = value.y_axis.x as f64;
	let mut n22 = value.y_axis.y as f64;
	let mut n23 = value.y_axis.z as f64;
	let n24 = 0.0;
	let n31 = value.z_axis.x as f64;
	let mut n32 = value.z_axis.y as f64;
	let mut n33 = value.z_axis.z as f64;
	let n34 = 0.0;
	let n41 = value.trans.x as f64;
	let n42 = value.trans.y as f64;
	let n43 = value.trans.z as f64;
	let n44 = 1.0;

	let det = n41
		* (n14 * n23 * n32 - n13 * n24 * n32 - n14 * n22 * n33 + n12 * n24 * n33 + n13 * n22 * n34 - n12 * n23 * n34)
		+ n42
			* (n11 * n23 * n34 - n11 * n24 * n33 + n14 * n21 * n33 - n13 * n21 * n34 + n13 * n24 * n31
				- n14 * n23 * n31)
		+ n43
			* (n11 * n24 * n32 - n11 * n22 * n34 - n14 * n21 * n32 + n12 * n21 * n34 + n14 * n22 * n31
				- n12 * n24 * n31)
		+ n44
			* (-n13 * n22 * n31 - n11 * n23 * n32 + n11 * n22 * n33 + n13 * n21 * n32 - n12 * n21 * n33
				+ n12 * n23 * n31);

	let mut sx = n11 * n11 + n21 * n21 + n31 * n31;
	let sy = n12 * n12 + n22 * n22 + n32 * n32;
	let sz = n13 * n13 + n23 * n23 + n33 * n33;

	if det < 0.0 {
		sx = -sx
	};

	let inv_sx = 1.0 / sx;
	let inv_sy = 1.0 / sy;
	let inv_sz = 1.0 / sz;

	n11 *= inv_sx;
	n12 *= inv_sy;
	n22 *= inv_sy;
	n32 *= inv_sy;
	n13 *= inv_sz;
	n23 *= inv_sz;
	n33 *= inv_sz;

	let rotation = json!({
		"x": (if n13.abs() < 0.9999999 { (- n23).atan2(n33) } else { (n32).atan2(n22) }) * RAD2DEG,
		"y": n13.clamp(-1.0, 1.0).asin() * RAD2DEG,
		"z": (if n13.abs() < 0.9999999 { (- n12).atan2(n11) } else { 0.0 }) * RAD2DEG
	});

	let position = json!({ "x": n41, "y": n42, "z": n43 });

	let scale_important = if convert_lossless {
		// In lossless mode, preserve exact scale
		sx != 1.0 || sy != 1.0 || sz != 1.0
	} else {
		// Otherwise only emit if scale is not equal to 1.00 (to 2 d.p.)
		(sx * 100.0).round() != 100.0 || (sy * 100.0).round() != 100.0 || (sz * 100.0).round() != 100.0
	};

	if scale_important {
		json!({
			"rotation": rotation,
			"position": position,
			"scale": json!({ "x": sx, "y": sy, "z": sz })
		})
	} else {
		json!({
			"rotation": rotation,
			"position": position
		})
	}
}

#[try_fn]
#[context("Failure converting property value to QN")]
pub fn convert_variant_to_qn(
	property_value: &dyn Variant,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	blueprint: &STemplateEntityBlueprint,
	convert_lossless: bool
) -> Result<Value> {
	if let Some(value) = property_value.as_vec() {
		to_value(
			value
				.into_iter()
				.map(|value| convert_variant_to_qn(value, factory, factory_meta, blueprint, convert_lossless))
				.collect::<Result<Vec<_>>>()?
		)?
	} else if let Some((first, second)) = property_value.as_ref::<(EcoString, ZVariant)>() {
		to_value((
			first,
			json!({
				"type": second.variant_type(),
				"value": convert_variant_to_qn(second, factory, factory_meta, blueprint, convert_lossless)?
			})
		))?
	} else if let Some(value) = property_value.as_ref::<SEntityTemplateReference>() {
		to_value(convert_reference_to_qn(value, factory, blueprint, factory_meta)?)?
	} else if let Some(value) = property_value.as_ref::<ZRuntimeResourceID>() {
		match value {
			ZRuntimeResourceID {
				id_high: u32::MAX,
				id_low: u32::MAX
			} => Value::Null,

			ZRuntimeResourceID { id_low, .. } => {
				// We ignore the id_high as no resource in the game has that many depends
				to_value(
					factory_meta
						.references
						.get(*id_low as usize)
						.context("ZRuntimeResourceID m_IDLow referred to non-existent dependency")?
				)?
			}
		}
	} else if let Some(value) = property_value.as_ref::<SMatrix43>() {
		convert_matrix(value, convert_lossless)
	} else if let Some(value) = property_value.as_ref::<ZGuid>() {
		to_value(format!(
			"{:0>8x}-{:0>4x}-{:0>4x}-{:0>2x}{:0>2x}-{:0>2x}{:0>2x}{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
			value._a,
			value._b,
			value._c,
			value._d,
			value._e,
			value._f,
			value._g,
			value._h,
			value._i,
			value._j,
			value._k
		))?
	} else if let Some(value) = property_value.as_ref::<SColorRGB>() {
		to_value(format!(
			"#{:0>2x}{:0>2x}{:0>2x}",
			(value.r * 255.0).round() as u8,
			(value.g * 255.0).round() as u8,
			(value.b * 255.0).round() as u8
		))?
	} else if let Some(value) = property_value.as_ref::<SColorRGBA>() {
		to_value(format!(
			"#{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
			(value.r * 255.0).round() as u8,
			(value.g * 255.0).round() as u8,
			(value.b * 255.0).round() as u8,
			(value.a * 255.0).round() as u8
		))?
	} else if let Some(value) = property_value.as_ref::<ZRepositoryID>() {
		to_value(String::from(*value).to_lowercase())?
	} else if let Some(value) = property_value.as_ref::<ZVariant>() {
		json!({
			"type": value.variant_type(),
			"value": convert_variant_to_qn(value, factory, factory_meta, blueprint, convert_lossless)?
		})
	} else {
		property_value.to_serde()?
	}
}

#[try_fn]
#[context("Failure converting game property to QN")]
#[auto_context]
fn convert_property_to_qn(
	property: &SEntityTemplateProperty,
	post_init: bool,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	blueprint: &STemplateEntityBlueprint,
	convert_lossless: bool
) -> Result<Property> {
	Property {
		property_type: property.value.variant_type(),
		value: convert_variant_to_qn(
			property.value.deref(),
			factory,
			factory_meta,
			blueprint,
			convert_lossless
		)?,
		post_init
	}
}

#[try_fn]
#[context("Failure converting QN property value to game format")]
#[auto_context]
pub fn convert_qn_property_value_to_game(
	property_type: &str,
	property_value: &Value,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	entity_id_to_index_mapping: &HashMap<EntityID, usize>,
	factory_dependencies_index_mapping: &HashMap<PathedID, usize>
) -> Result<Value> {
	match property_type {
		"SEntityTemplateReference" => to_value(convert_qn_reference_to_game(
			from_value::<Option<Ref>>(property_value.to_owned())
				.context("Invalid entity reference")?
				.as_ref(),
			factory,
			factory_meta,
			entity_id_to_index_mapping
		)?)?,

		"ZRuntimeResourceID" => {
			if property_value.is_null() {
				json!({
					"m_IDHigh": 4294967295u32,
					"m_IDLow": 4294967295u32
				})
			} else if property_value.is_string() {
				json!({
					"m_IDHigh": 0, // I doubt we'll ever have that many dependencies
					"m_IDLow": factory_dependencies_index_mapping.get(&PathedID::from_str(property_value.as_str().ctx?)?).ctx?
				})
			} else if property_value.is_object() {
				json!({
					"m_IDHigh": 0,
					"m_IDLow": factory_dependencies_index_mapping.get(&PathedID::from_str(property_value.get("resource").context("ZRuntimeResourceID didn't have resource despite being object")?.as_str().context("ZRuntimeResourceID resource must be string")?)?).ctx?
				})
			} else {
				bail!("ZRuntimeResourceID was not of a valid type")
			}
		}

		"SMatrix43" => {
			// this is from three.js

			let obj = property_value.as_object().context("SMatrix43 must be object")?;

			let x = obj.get("rotation").ctx?.get("x").ctx?.as_f64().ctx? * DEG2RAD;
			let y = obj.get("rotation").ctx?.get("y").ctx?.as_f64().ctx? * DEG2RAD;
			let z = obj.get("rotation").ctx?.get("z").ctx?.as_f64().ctx? * DEG2RAD;

			let c1 = (x / 2.0).cos();
			let c2 = (y / 2.0).cos();
			let c3 = (z / 2.0).cos();

			let s1 = (x / 2.0).sin();
			let s2 = (y / 2.0).sin();
			let s3 = (z / 2.0).sin();

			let quat_x = s1 * c2 * c3 + c1 * s2 * s3;
			let quat_y = c1 * s2 * c3 - s1 * c2 * s3;
			let quat_z = c1 * c2 * s3 + s1 * s2 * c3;
			let quat_w = c1 * c2 * c3 - s1 * s2 * s3;

			let x2 = quat_x + quat_x;
			let y2 = quat_y + quat_y;
			let z2 = quat_z + quat_z;
			let xx = quat_x * x2;
			let xy = quat_x * y2;
			let xz = quat_x * z2;
			let yy = quat_y * y2;
			let yz = quat_y * z2;
			let zz = quat_z * z2;
			let wx = quat_w * x2;
			let wy = quat_w * y2;
			let wz = quat_w * z2;

			let sx = if let Some(scale) = obj.get("scale") {
				scale
					.get("x")
					.context("Scale must have x value")?
					.as_f64()
					.context("Scale must be number")?
			} else {
				1.0
			};

			let sy = if let Some(scale) = obj.get("scale") {
				scale
					.get("y")
					.context("Scale must have y value")?
					.as_f64()
					.context("Scale must be number")?
			} else {
				1.0
			};

			let sz = if let Some(scale) = obj.get("scale") {
				scale
					.get("z")
					.context("Scale must have z value")?
					.as_f64()
					.context("Scale must be number")?
			} else {
				1.0
			};

			json!({
				"XAxis": {
					"x": (1.0 - (yy + zz)) * sx,
					"y": (xy - wz) * sy,
					"z": (xz + wy) * sz
				},
				"YAxis": {
					"x": (xy + wz) * sx,
					"y": (1.0 - (xx + zz)) * sy,
					"z": (yz - wx) * sz
				},
				"ZAxis": {
					"x": (xz - wy) * sx,
					"y": (yz + wx) * sy,
					"z": (1.0 - (xx + yy)) * sz
				},
				"Trans": {
					"x": obj.get("position").ctx?.get("x").ctx?.as_f64().ctx?,
					"y": obj.get("position").ctx?.get("y").ctx?.as_f64().ctx?,
					"z": obj.get("position").ctx?.get("z").ctx?.as_f64().ctx?
				}
			})
		}

		"ZGuid" => json!({
			"_a": u32::from_str_radix(property_value.as_str().ctx?.split('-').next().ctx?, 16).ctx?,
			"_b": u16::from_str_radix(property_value.as_str().ctx?.split('-').nth(1).ctx?, 16).ctx?,
			"_c": u16::from_str_radix(property_value.as_str().ctx?.split('-').nth(2).ctx?, 16).ctx?,
			"_d": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(3).ctx?.chars().take(2).collect::<String>(), 16).ctx?,
			"_e": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(3).ctx?.chars().skip(2).take(2).collect::<String>(), 16).ctx?,
			"_f": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().take(2).collect::<String>(), 16).ctx?,
			"_g": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().skip(2).take(2).collect::<String>(), 16).ctx?,
			"_h": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().skip(4).take(2).collect::<String>(), 16).ctx?,
			"_i": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().skip(6).take(2).collect::<String>(), 16).ctx?,
			"_j": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().skip(8).take(2).collect::<String>(), 16).ctx?,
			"_k": u8::from_str_radix(&property_value.as_str().ctx?.split('-').nth(4).ctx?.chars().skip(10).take(2).collect::<String>(), 16).ctx?
		}),

		"SColorRGB" => json!({
			"r": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).take(2).collect::<String>(), 16).ctx?) / 255.0,
			"g": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).skip(2).take(2).collect::<String>(), 16).ctx?) / 255.0,
			"b": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).skip(4).take(2).collect::<String>(), 16).ctx?) / 255.0
		}),

		"SColorRGBA" => json!({
			"r": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).take(2).collect::<String>(), 16).ctx?) / 255.0,
			"g": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).skip(2).take(2).collect::<String>(), 16).ctx?) / 255.0,
			"b": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).skip(4).take(2).collect::<String>(), 16).ctx?) / 255.0,
			"a": f64::from(u8::from_str_radix(&property_value.as_str().ctx?.chars().skip(1).skip(6).take(2).collect::<String>(), 16).ctx?) / 255.0
		}),

		"ZRepositoryID" => to_value(
			ZRepositoryID::try_from(property_value.as_str().ctx?.to_uppercase().as_str())
				.context("Invalid ZRepositoryID")?
		)?,

		"TPair<ZString,ZVariant>" => {
			let mut elements = property_value.as_array().context("TPair value was not array")?.iter();
			let first = elements.next().context("TPair must have two elements")?;
			let second = elements.next().context("TPair must have two elements")?;

			to_value((
				convert_qn_property_value_to_game(
					"ZString",
					first,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping
				)?,
				convert_qn_property_value_to_game(
					"ZVariant",
					second,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping
				)?
			))?
		}

		"ZVariant" => {
			let ty = property_value
				.get("type")
				.context("ZVariant must have type key")?
				.as_str()
				.context("ZVariant type must be string")?;

			let value = property_value.get("value").context("ZVariant must have value key")?;

			json!({
				"$type": ty,
				"$val": convert_qn_property_value_to_game(
					ty,
					value,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping
				)?
			})
		}

		property_type if property_type.starts_with("TArray<") => {
			let mut single_type = property_type.chars();
			single_type.nth(6); // discard TArray<
			single_type.next_back(); // discard closing >
			let single_type = single_type.collect::<String>();

			property_value
				.as_array()
				.context("TArray value was not array")?
				.iter()
				.map(|value| {
					convert_qn_property_value_to_game(
						&single_type,
						value,
						factory,
						factory_meta,
						entity_id_to_index_mapping,
						factory_dependencies_index_mapping
					)
				})
				.collect::<Result<_>>()?
		}

		_ => property_value.to_owned()
	}
}

#[try_fn]
#[context("Failure converting QN property to game format")]
fn convert_qn_property_to_game(
	property_name: &str,
	property_type: String,
	property_value: &Value,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	entity_id_to_index_mapping: &HashMap<EntityID, usize>,
	factory_dependencies_index_mapping: &HashMap<PathedID, usize>
) -> Result<SEntityTemplateProperty> {
	let value = convert_qn_property_value_to_game(
		&property_type,
		property_value,
		factory,
		factory_meta,
		entity_id_to_index_mapping,
		factory_dependencies_index_mapping
	)?;

	SEntityTemplateProperty {
		property_id: convert_string_property_name_to_id(property_name)?,
		value: from_value(json!({
			"$type": property_type,
			"$val": value
		}))?
	}
}

#[try_fn]
#[context("Failure converting string property name to ID")]
#[auto_context]
fn convert_string_property_name_to_id(property_name: &str) -> Result<PropertyID> {
	if let Ok(i) = property_name.parse::<u32>() {
		let is_crc_length = {
			let x = format!("{i:x}").chars().count();

			x == 8 || x == 7
		};

		if is_crc_length {
			PropertyID::from(i)
		} else {
			PropertyID::from(crc32fast::hash(property_name.as_bytes()))
		}
	} else {
		PropertyID::from(crc32fast::hash(property_name.as_bytes()))
	}
}

#[try_fn]
#[context("Failure getting factory dependencies")]
#[auto_context]
fn get_factory_dependencies(entity: &Entity) -> Result<Vec<ResourceReference>> {
	vec![
		// blueprint first
		vec![ResourceReference {
			resource: entity.blueprint.to_owned(),
			flags: Default::default()
		}],
		// then external scenes
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceReference {
				resource: scene.to_owned(),
				flags: Default::default()
			})
			.collect(),
		// then factories of sub-entities
		entity
			.entities
			.par_iter()
			.map(|(_, sub_entity)| sub_entity.factory.to_owned())
			.collect(),
		// then sub-entity ZRuntimeResourceIDs
		entity
			.entities
			.par_iter()
			.map(|(_, sub_entity)| -> Result<_> {
				Ok(vec![
					sub_entity
						.properties
						.iter()
						.filter(|(_, prop)| prop.property_type == "ZRuntimeResourceID" && !prop.value.is_null())
						.map(|(_, prop)| -> Result<_> {
							from_value::<ResourceReference>(prop.value.to_owned())
								.context("ZRuntimeResourceID must be valid ResourceReference")
						})
						.collect::<Result<Vec<_>>>()?,
					sub_entity
						.properties
						.iter()
						.filter(|(_, prop)| prop.property_type == "TArray<ZRuntimeResourceID>" && !prop.value.is_null())
						.map(|(_, prop)| -> Result<_> {
							prop.value
								.as_array()
								.context("TArray<ZRuntimeResourceID> must be array")?
								.iter()
								.map(|value| -> Result<_> {
									from_value::<ResourceReference>(value.to_owned())
										.context("ZRuntimeResourceID must be valid ResourceReference")
								})
								.collect::<Result<Vec<_>>>()
						})
						.collect::<Result<Vec<_>>>()?
						.into_iter()
						.flatten()
						.collect(),
					sub_entity
						.platform_specific_properties
						.iter()
						.map(|(_, props)| -> Result<_> {
							Ok([
								props
									.iter()
									.filter(|(_, prop)| {
										prop.property_type == "ZRuntimeResourceID" && !prop.value.is_null()
									})
									.map(|(_, prop)| -> Result<_> {
										from_value::<ResourceReference>(prop.value.to_owned())
											.context("ZRuntimeResourceID must be valid ResourceReference")
									})
									.collect::<Result<Vec<_>>>()?,
								props
									.iter()
									.filter(|(_, prop)| {
										prop.property_type == "TArray<ZRuntimeResourceID>" && !prop.value.is_null()
									})
									.map(|(_, prop)| -> Result<_> {
										prop.value
											.as_array()
											.context("TArray<ZRuntimeResourceID> must be array")?
											.iter()
											.map(|value| -> Result<_> {
												from_value::<ResourceReference>(value.to_owned())
													.context("ZRuntimeResourceID must be valid ResourceReference")
											})
											.collect::<Result<Vec<_>>>()
									})
									.collect::<Result<Vec<_>>>()?
									.into_iter()
									.flatten()
									.collect()
							]
							.concat())
						})
						.collect::<Result<Vec<_>>>()?
						.into_iter()
						.flatten()
						.collect(),
				]
				.into_iter()
				.concat())
			})
			.collect::<Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect(),
		// then property override ZRuntimeResourceIDs
		entity
			.property_overrides
			.par_iter()
			.map(|PropertyOverride { properties, .. }| -> Result<_> {
				Ok([
					properties
						.iter()
						.filter(|(_, prop)| prop.property_type == "ZRuntimeResourceID" && !prop.value.is_null())
						.map(|(_, prop)| -> Result<_> {
							from_value::<ResourceReference>(prop.value.to_owned())
								.context("ZRuntimeResourceID must be valid ResourceReference")
						})
						.collect::<Result<Vec<_>>>()?,
					properties
						.iter()
						.filter(|(_, prop)| prop.property_type == "TArray<ZRuntimeResourceID>" && !prop.value.is_null())
						.map(|(_, prop)| -> Result<_> {
							prop.value
								.as_array()
								.context("TArray<ZRuntimeResourceID> must be array")?
								.iter()
								.map(|value| -> Result<_> {
									from_value::<ResourceReference>(value.to_owned())
										.context("ZRuntimeResourceID must be valid ResourceReference")
								})
								.collect::<Result<Vec<_>>>()
						})
						.collect::<Result<Vec<_>>>()?
						.into_iter()
						.flatten()
						.collect()
				]
				.concat())
			})
			.collect::<Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

fn get_blueprint_dependencies(entity: &Entity) -> Vec<ResourceReference> {
	vec![
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceReference {
				resource: scene.to_owned(),
				flags: Default::default()
			})
			.collect::<Vec<_>>(),
		entity
			.entities
			.iter()
			.map(|(_, sub_entity)| ResourceReference {
				resource: sub_entity.blueprint.to_owned(),
				flags: Default::default()
			})
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

#[try_fn]
#[context("Failure converting RL entity to QN")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
pub fn convert_to_qn(
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	blueprint: &STemplateEntityBlueprint,
	blueprint_meta: &ResourceMetadata,
	convert_lossless: bool
) -> Result<Entity> {
	let pool = rayon::ThreadPoolBuilder::new().build()?;
	pool.install(|| {
		{
			let mut ids = blueprint.sub_entities.iter().map(|x| x.entity_id).collect_vec();
			ids.sort_unstable();
			ids.dedup();

			if ids.len() != blueprint.sub_entities.len() {
				bail!("Cannot convert entity with duplicate IDs");
			}
		}

		let mut entity = Entity {
			factory: factory_meta.id.to_owned(),
			blueprint: blueprint_meta.id.to_owned(),
			root_entity: blueprint
				.sub_entities
				.get(blueprint.root_entity_index as usize)
				.context("Root entity index referred to nonexistent entity")?
				.entity_id
				.into(),
			entities: factory
				.sub_entities
				.par_iter() // rayon automatically makes this run in parallel for s p e e d
				.enumerate()
				.map(|(index, sub_entity_factory)| -> Result<(EntityID, SubEntity)> {
					let sub_entity_blueprint = blueprint
						.sub_entities
						.get(index)
						.context("Factory entity had no equivalent by index in blueprint")?;

					Ok((
						sub_entity_blueprint.entity_id.into(),
						SubEntity {
							name: sub_entity_blueprint.entity_name.to_owned().into(),
							factory: factory_meta
								.references
								.get(sub_entity_factory.entity_type_resource_index as usize)
								.context("Entity resource index referred to nonexistent dependency")?
								.to_owned(),
							blueprint: blueprint_meta
								.references
								.get(sub_entity_blueprint.entity_type_resource_index as usize)
								.context("Entity resource index referred to nonexistent dependency")?
								.resource
								.to_owned(),
							parent: convert_reference_to_qn(
								&sub_entity_factory.logical_parent,
								factory,
								blueprint,
								factory_meta
							)?,
							editor_only: sub_entity_blueprint.editor_only,
							properties: sub_entity_factory
								.property_values
								.iter()
								.map(|property| -> Result<_> {
									Ok((
										property
											.property_id
											.as_name()
											.map(|x| x.to_owned())
											.unwrap_or_else(|| property.property_id.0.to_string()), // key
										convert_property_to_qn(
											property,
											false,
											factory,
											factory_meta,
											blueprint,
											convert_lossless
										)? // value
									))
								})
								.chain(sub_entity_factory.post_init_property_values.iter().map(
									|property| -> Result<_> {
										Ok((
											// we do a little code duplication
											property
												.property_id
												.as_name()
												.map(|x| x.to_owned())
												.unwrap_or_else(|| property.property_id.0.to_string()),
											convert_property_to_qn(
												property,
												true,
												factory,
												factory_meta,
												blueprint,
												convert_lossless
											)?
										))
									}
								))
								.collect::<Result<_>>()?,
							// Group props by platform, then convert them all and turn into a nested OrderMap structure
							platform_specific_properties: sub_entity_factory
								.platform_specific_property_values
								.iter()
								.into_group_map_by(|property| property.platform.to_owned())
								.into_iter()
								.map(|(platform, properties)| -> Result<_> {
									Ok((
										<&str>::from(platform).into(),
										properties
											.into_iter()
											.map(|property| -> Result<(String, Property)> {
												Ok((
													// we do a little code duplication
													property
														.property_value
														.property_id
														.as_name()
														.map(|x| x.to_owned())
														.unwrap_or_else(|| {
															property.property_value.property_id.0.to_string()
														}),
													convert_property_to_qn(
														&property.property_value,
														property.post_init.to_owned(),
														factory,
														factory_meta,
														blueprint,
														convert_lossless
													)?
												))
											})
											.collect::<Result<_>>()?
									))
								})
								.collect::<Result<_>>()?,
							events: Default::default(),         // will be mutated later
							input_copying: Default::default(),  // will be mutated later
							output_copying: Default::default(), // will be mutated later
							property_aliases: sub_entity_blueprint
								.property_aliases
								.iter()
								.into_group_map_by(|alias| alias.property_name.to_owned())
								.into_iter()
								.map(|(property_name, aliases)| {
									Ok({
										(
											property_name.into(),
											aliases
												.into_iter()
												.map(|alias| {
													Ok(PropertyAlias {
														original_property: alias.alias_name.to_owned().into(),
														original_entity: blueprint
															.sub_entities
															.get(alias.entity_id as usize)
															.context(
																"Property alias referred to nonexistent sub-entity"
															)?
															.entity_id
															.into()
													})
												})
												.collect::<Result<_>>()?
										)
									})
								})
								.collect::<Result<_>>()?,
							exposed_entities: sub_entity_blueprint
								.exposed_entities
								.iter()
								.map(|exposed_entity| -> Result<_> {
									Ok((
										exposed_entity.name.to_owned().into(),
										ExposedEntity {
											is_array: exposed_entity.is_array.to_owned(),
											refers_to: exposed_entity
												.targets
												.iter()
												.map(|target| {
													convert_reference_to_qn(target, factory, blueprint, factory_meta)?
														.context("Exposed entity references must not be null")
												})
												.collect::<Result<_>>()?
										}
									))
								})
								.collect::<Result<_>>()?,
							exposed_interfaces: sub_entity_blueprint
								.exposed_interfaces
								.iter()
								.map(|(interface, entity_index)| {
									Ok((
										interface.to_owned().into(),
										blueprint
											.sub_entities
											.get(*entity_index as usize)
											.context("Exposed interface referred to nonexistent sub-entity")?
											.entity_id
											.into()
									))
								})
								.collect::<Result<_>>()?,
							subsets: Default::default() // will be mutated later
						}
					))
				})
				.collect::<Result<OrderMap<EntityID, SubEntity>>>()?,
			external_scenes: factory
				.external_scene_type_indices_in_resource_header
				.par_iter()
				.map(|scene_index| {
					Ok(factory_meta
						.references
						.get(*scene_index as usize)
						.ctx?
						.resource
						.to_owned())
				})
				.collect::<Result<_>>()?,
			override_deletes: blueprint
				.override_deletes
				.par_iter()
				.map(|x| {
					convert_reference_to_qn(x, factory, blueprint, factory_meta)?
						.context("Override delete references must not be null")
				})
				.collect::<Result<_>>()?,
			pin_connection_override_deletes: blueprint
				.pin_connection_override_deletes
				.par_iter()
				.map(|x| {
					Ok(PinConnectionOverrideDelete {
						from_entity: convert_reference_to_qn(&x.from_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override delete references must not be null")?,
						to_entity: convert_reference_to_qn(&x.to_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override delete references must not be null")?,
						from_pin: x.from_pin_name.to_owned().into(),
						to_pin: x.to_pin_name.to_owned().into(),
						value: if x.constant_pin_value.is::<()>() {
							None
						} else {
							Some(SimpleProperty {
								property_type: x.constant_pin_value.variant_type(),
								value: convert_variant_to_qn(
									x.constant_pin_value.deref(),
									factory,
									factory_meta,
									blueprint,
									convert_lossless
								)?
							})
						}
					})
				})
				.collect::<Result<_>>()?,
			pin_connection_overrides: blueprint
				.pin_connection_overrides
				.par_iter()
				.filter(|x| x.from_entity.external_scene_index != -1)
				.map(|x| {
					Ok(PinConnectionOverride {
						from_entity: convert_reference_to_qn(&x.from_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override references must not be null")?,
						to_entity: convert_reference_to_qn(&x.to_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override references must not be null")?,
						from_pin: x.from_pin_name.to_owned().into(),
						to_pin: x.to_pin_name.to_owned().into(),
						value: if x.constant_pin_value.is::<()>() {
							None
						} else {
							Some(SimpleProperty {
								property_type: x.constant_pin_value.variant_type(),
								value: convert_variant_to_qn(
									x.constant_pin_value.deref(),
									factory,
									factory_meta,
									blueprint,
									convert_lossless
								)?
							})
						}
					})
				})
				.collect::<Result<_>>()?,
			property_overrides: vec![],
			sub_type: match blueprint.sub_type {
				2 => SubType::Brick,
				1 => SubType::Scene,
				0 => SubType::Template,
				_ => bail!("Invalid subtype {}", blueprint.sub_type)
			},
			quick_entity_version: 3.2,
			extra_factory_references: vec![],
			extra_blueprint_references: vec![],
			comments: vec![]
		};

		{
			let depends = get_factory_dependencies(&entity)?.into_iter().collect::<HashSet<_>>();

			entity.extra_factory_references = factory_meta
				.references
				.iter()
				.filter(|x| !depends.contains(x))
				.cloned()
				.collect();
		}

		{
			let depends = get_blueprint_dependencies(&entity).into_iter().collect::<HashSet<_>>();

			entity.extra_blueprint_references = blueprint_meta
				.references
				.iter()
				.filter(|x| !depends.contains(x))
				.cloned()
				.collect();
		}

		for pin in &blueprint.pin_connections {
			let relevant_sub_entity = entity
				.entities
				.get_mut(&EntityID::from(
					blueprint
						.sub_entities
						.get(pin.from_id as usize)
						.context("Pin referred to nonexistent sub-entity")?
						.entity_id
				))
				.ctx?;

			relevant_sub_entity
				.events
				.entry(pin.from_pin_name.to_owned().into())
				.or_default()
				.entry(pin.to_pin_name.to_owned().into())
				.or_default()
				.push(PinConnection {
					entity_ref: Ref::local(
						blueprint
							.sub_entities
							.get(pin.to_id as usize)
							.context("Pin referred to nonexistent sub-entity")?
							.entity_id
							.into()
					),
					value: if pin.constant_pin_value.is::<()>() {
						None
					} else {
						Some(SimpleProperty {
							property_type: pin.constant_pin_value.variant_type(),
							value: convert_variant_to_qn(
								pin.constant_pin_value.deref(),
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?
						})
					}
				});
		}

		for pin_connection_override in blueprint
			.pin_connection_overrides
			.iter()
			.filter(|x| x.from_entity.external_scene_index == -1)
		{
			let relevant_sub_entity = entity
				.entities
				.get_mut(&EntityID::from(
					blueprint
						.sub_entities
						.get(pin_connection_override.from_entity.entity_index as usize)
						.context("Pin connection override referred to nonexistent sub-entity")?
						.entity_id
				))
				.ctx?;

			relevant_sub_entity
				.events
				.entry(pin_connection_override.from_pin_name.to_owned().into())
				.or_default()
				.entry(pin_connection_override.to_pin_name.to_owned().into())
				.or_default()
				.push(PinConnection {
					entity_ref: convert_reference_to_qn(
						&pin_connection_override.to_entity,
						factory,
						blueprint,
						factory_meta
					)?
					.context("Pin connection references must not be null")?,
					value: if pin_connection_override.constant_pin_value.is::<()>() {
						None
					} else {
						Some(SimpleProperty {
							property_type: pin_connection_override.constant_pin_value.variant_type(),
							value: convert_variant_to_qn(
								pin_connection_override.constant_pin_value.deref(),
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?
						})
					}
				});
		}

		// cheeky bit of code duplication right here
		for forwarding in &blueprint.input_pin_forwardings {
			let relevant_sub_entity = entity
				.entities
				.get_mut(&EntityID::from(
					blueprint
						.sub_entities
						.get(forwarding.from_id as usize)
						.context("Pin referred to nonexistent sub-entity")?
						.entity_id
				))
				.ctx?;

			relevant_sub_entity
				.input_copying
				.entry(forwarding.from_pin_name.to_owned().into())
				.or_default()
				.entry(forwarding.to_pin_name.to_owned().into())
				.or_default()
				.push(PinConnection {
					entity_ref: Ref::local(
						blueprint
							.sub_entities
							.get(forwarding.to_id as usize)
							.context("Pin referred to nonexistent sub-entity")?
							.entity_id
							.into()
					),
					value: if forwarding.constant_pin_value.is::<()>() {
						None
					} else {
						Some(SimpleProperty {
							property_type: forwarding.constant_pin_value.variant_type(),
							value: convert_variant_to_qn(
								forwarding.constant_pin_value.deref(),
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?
						})
					}
				});
		}

		for forwarding in &blueprint.output_pin_forwardings {
			let relevant_sub_entity = entity
				.entities
				.get_mut(&EntityID::from(
					blueprint
						.sub_entities
						.get(forwarding.from_id as usize)
						.context("Pin referred to nonexistent sub-entity")?
						.entity_id
				))
				.ctx?;

			relevant_sub_entity
				.output_copying
				.entry(forwarding.from_pin_name.to_owned().into())
				.or_default()
				.entry(forwarding.to_pin_name.to_owned().into())
				.or_default()
				.push(PinConnection {
					entity_ref: Ref::local(
						blueprint
							.sub_entities
							.get(forwarding.to_id as usize)
							.context("Pin referred to nonexistent sub-entity")?
							.entity_id
							.into()
					),
					value: if forwarding.constant_pin_value.is::<()>() {
						None
					} else {
						Some(SimpleProperty {
							property_type: forwarding.constant_pin_value.variant_type(),
							value: convert_variant_to_qn(
								forwarding.constant_pin_value.deref(),
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?
						})
					}
				});
		}

		for sub_entity in &blueprint.sub_entities {
			for (subset, data) in &sub_entity.entity_subsets {
				for subset_entity in &data.entities {
					let relevant_qn = entity
						.entities
						.get_mut(&EntityID::from(
							blueprint
								.sub_entities
								.get(*subset_entity as usize)
								.context("Entity subset referred to nonexistent sub-entity")?
								.entity_id
						))
						.ctx?;

					relevant_qn
						.subsets
						.entry(subset.to_owned().into())
						.or_default()
						.push(sub_entity.entity_id.into());
				}
			}
		}

		let mut pass1: Vec<PropertyOverride> = Vec::default();

		for property_override in &factory.property_overrides {
			let ents = vec![
				convert_reference_to_qn(&property_override.property_owner, factory, blueprint, factory_meta)?
					.context("Property override references must not be null")?,
			];

			let props = [(
				property_override
					.property_value
					.property_id
					.as_name()
					.map(|x| x.to_owned())
					.unwrap_or_else(|| property_override.property_value.property_id.0.to_string()),
				{
					let prop = convert_property_to_qn(
						&property_override.property_value,
						false,
						factory,
						factory_meta,
						blueprint,
						convert_lossless
					)?;

					SimpleProperty {
						value: prop.value,
						property_type: prop.property_type
					}
				}
			)]
			.into_iter()
			.collect();

			// if same entity being overridden, merge props
			if let Some(found) = pass1.iter_mut().find(|x| x.entities == ents) {
				found.properties.extend(props);
			} else {
				pass1.push(PropertyOverride {
					entities: ents,
					properties: props
				});
			}
		}

		// merge entities when same props being overridden
		for property_override in pass1 {
			if let Some(found) = entity
				.property_overrides
				.iter_mut()
				.find(|x| x.properties == property_override.properties)
			{
				found.entities.extend(property_override.entities);
			} else {
				entity.property_overrides.push(property_override);
			}
		}

		Ok(entity)
	})?
}

#[cfg(feature = "rune")]
#[rune::function]
pub fn r_convert_to_qn(
	factory: rune::Value,
	factory_meta: &ResourceMetadata,
	blueprint: rune::Value,
	blueprint_meta: &ResourceMetadata,
	convert_lossless: bool
) -> Result<Entity> {
	let factory = from_value(to_value(factory)?)?;
	let blueprint = from_value(to_value(blueprint)?)?;
	convert_to_qn(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
}

#[try_fn]
#[context("Failure converting QN entity to RL")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
pub fn convert_to_rl(
	entity: &Entity
) -> Result<(
	STemplateEntityFactory,
	ResourceMetadata,
	STemplateEntityBlueprint,
	ResourceMetadata
)> {
	if entity.quick_entity_version != QN_VERSION {
		bail!(
			"Invalid QuickEntity version; expected {}, got {}",
			QN_VERSION,
			entity.quick_entity_version
		);
	}

	let pool = rayon::ThreadPoolBuilder::new().build()?;
	pool.install(|| {
		let entity_id_to_index_mapping: HashMap<EntityID, usize> =
			entity.entities.keys().enumerate().map(|(x, y)| (*y, x)).collect();

		let mut factory = STemplateEntityFactory {
			sub_type: match entity.sub_type {
				SubType::Brick => 2,
				SubType::Scene => 1,
				SubType::Template => 0
			},
			blueprint_index_in_resource_header: 0,
			root_entity_index: *entity_id_to_index_mapping
				.get(&entity.root_entity)
				.context("Root entity was non-existent")? as i32,
			sub_entities: Vec::with_capacity(entity.entities.len()),
			property_overrides: vec![],
			external_scene_type_indices_in_resource_header: (1..entity.external_scenes.len() as i32 + 1).collect()
		};

		let factory_meta = ResourceMetadata {
			id: entity.factory.to_owned(),
			resource_type: "TEMP".try_into()?,
			compressed: ResourceMetadata::infer_compressed("TEMP".try_into()?),
			scrambled: ResourceMetadata::infer_scrambled("TEMP".try_into()?),
			references: [
				get_factory_dependencies(entity)?,
				entity.extra_factory_references.to_owned()
			]
			.concat()
		};

		let factory_dependencies_index_mapping: HashMap<PathedID, usize> = factory_meta
			.references
			.par_iter()
			.enumerate()
			.map(|(x, y)| (y.resource.to_owned(), x.to_owned()))
			.collect();

		let mut blueprint = STemplateEntityBlueprint {
			sub_type: match entity.sub_type {
				SubType::Brick => 2,
				SubType::Scene => 1,
				SubType::Template => 0
			},
			root_entity_index: *entity_id_to_index_mapping
				.get(&entity.root_entity)
				.context("Root entity was non-existent")? as i32,
			sub_entities: vec![],
			pin_connections: vec![],
			input_pin_forwardings: vec![],
			output_pin_forwardings: vec![],
			override_deletes: entity
				.override_deletes
				.par_iter()
				.map(|override_delete| {
					convert_qn_reference_to_game(
						Some(override_delete),
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					)
				})
				.collect::<Result<_>>()?,
			pin_connection_overrides: [
				entity
					.pin_connection_overrides
					.par_iter()
					.map(|pin_connection_override| {
						Ok(SExternalEntityTemplatePinConnection {
							from_entity: convert_qn_reference_to_game(
								Some(&pin_connection_override.from_entity),
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping
							)?,
							to_entity: convert_qn_reference_to_game(
								Some(&pin_connection_override.to_entity),
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping
							)?,
							from_pin_name: pin_connection_override.from_pin.to_owned().into(),
							to_pin_name: pin_connection_override.to_pin.to_owned().into(),
							constant_pin_value: {
								if let Some(property) = pin_connection_override.value.as_ref() {
									from_value(json!({
										"$type": property.property_type,
										"$val": convert_qn_property_value_to_game(
											&property.property_type,
											&property.value,
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping,
											&factory_dependencies_index_mapping
										)?
									}))?
								} else {
									ZVariant::new(())
								}
							}
						})
					})
					.collect::<Result<_>>()?,
				entity
					.entities
					.par_iter()
					.map(|(entity_id, sub_entity)| {
						Ok(sub_entity
							.events
							.iter()
							.map(|(event, pin)| {
								Ok(pin
									.iter()
									.map(|(trigger, entities)| {
										entities
											.iter()
											.filter(|&trigger_entity| {
												trigger_entity.entity_ref.external_scene.is_some()
											})
											.map(|trigger_entity| {
												Ok(SExternalEntityTemplatePinConnection {
													from_entity: convert_qn_reference_to_game(
														Some(&Ref::local(*entity_id)),
														&factory,
														&factory_meta,
														&entity_id_to_index_mapping
													)?,
													to_entity: convert_qn_reference_to_game(
														Some(&trigger_entity.entity_ref),
														&factory,
														&factory_meta,
														&entity_id_to_index_mapping
													)?,
													from_pin_name: event.to_owned().into(),
													to_pin_name: trigger.to_owned().into(),
													constant_pin_value: if let Some(value) = &trigger_entity.value {
														from_value(json!({
															"$type": value.property_type,
															"$val": convert_qn_property_value_to_game(
																&value.property_type,
																&value.value,
																&factory,
																&factory_meta,
																&entity_id_to_index_mapping,
																&factory_dependencies_index_mapping
															)?
														}))?
													} else {
														ZVariant::new(())
													}
												})
											})
											.collect::<Result<Vec<SExternalEntityTemplatePinConnection>>>()
									})
									.collect::<Result<Vec<_>>>()?
									.into_iter()
									.flatten()
									.collect::<Vec<SExternalEntityTemplatePinConnection>>())
							})
							.collect::<Result<Vec<_>>>()?
							.into_iter()
							.flatten()
							.collect::<Vec<_>>())
					})
					.collect::<Result<Vec<_>>>()?
					.into_iter()
					.flatten()
					.collect::<Vec<SExternalEntityTemplatePinConnection>>()
			]
			.concat(),
			pin_connection_override_deletes: entity
				.pin_connection_override_deletes
				.par_iter()
				.map(|pin_connection_override_delete| {
					Ok(SExternalEntityTemplatePinConnection {
						from_entity: convert_qn_reference_to_game(
							Some(&pin_connection_override_delete.from_entity),
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						)?,
						to_entity: convert_qn_reference_to_game(
							Some(&pin_connection_override_delete.to_entity),
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						)?,
						from_pin_name: pin_connection_override_delete.from_pin.to_owned().into(),
						to_pin_name: pin_connection_override_delete.to_pin.to_owned().into(),
						constant_pin_value: {
							if let Some(property) = pin_connection_override_delete.value.as_ref() {
								from_value(json!({
									"$type": property.property_type,
									"$val": convert_qn_property_value_to_game(
										&property.property_type,
										&property.value,
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping,
										&factory_dependencies_index_mapping
									)?
								}))?
							} else {
								ZVariant::new(())
							}
						}
					})
				})
				.collect::<Result<_>>()?,
			external_scene_type_indices_in_resource_header: (0..entity.external_scenes.len() as i32).collect()
		};

		let blueprint_meta = ResourceMetadata {
			id: entity.blueprint.to_owned(),
			resource_type: "TBLU".try_into()?,
			compressed: ResourceMetadata::infer_compressed("TBLU".try_into()?),
			scrambled: ResourceMetadata::infer_scrambled("TBLU".try_into()?),
			references: [
				get_blueprint_dependencies(entity),
				entity.extra_blueprint_references.to_owned()
			]
			.concat()
		};

		let blueprint_dependencies_index_mapping: HashMap<PathedID, usize> = blueprint_meta
			.references
			.par_iter()
			.enumerate()
			.map(|(x, y)| (y.resource.to_owned(), x.to_owned()))
			.collect();

		factory.property_overrides = entity
			.property_overrides
			.par_iter()
			.flat_map(|property_override| {
				property_override
					.entities
					.iter()
					.flat_map(|ext_entity| {
						property_override
							.properties
							.iter()
							.map(|(property, overridden)| {
								Ok(SEntityTemplatePropertyOverride {
									property_owner: convert_qn_reference_to_game(
										Some(ext_entity),
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping
									)?,
									property_value: convert_qn_property_to_game(
										property,
										overridden.property_type.to_owned(),
										&overridden.value,
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping,
										&factory_dependencies_index_mapping
									)?
								})
							})
							.collect_vec()
					})
					.collect_vec()
			})
			.collect::<Result<_>>()?;

		factory.sub_entities = entity
			.entities
			.par_iter()
			.map(|(_, sub_entity)| {
				Ok(STemplateFactorySubEntity {
					logical_parent: convert_qn_reference_to_game(
						sub_entity.parent.as_ref(),
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					)?,
					entity_type_resource_index: *factory_dependencies_index_mapping
						.get(&sub_entity.factory.resource)
						.ctx? as i32,
					property_values: sub_entity
						.properties
						.iter()
						.filter(|(_, x)| !x.post_init)
						.map(|(x, y)| {
							convert_qn_property_to_game(
								x,
								y.property_type.to_owned(),
								&y.value,
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping,
								&factory_dependencies_index_mapping
							)
						})
						.collect::<Result<_>>()?,
					post_init_property_values: sub_entity
						.properties
						.iter()
						.filter(|(_, y)| y.post_init)
						.map(|(x, y)| {
							convert_qn_property_to_game(
								x,
								y.property_type.to_owned(),
								&y.value,
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping,
								&factory_dependencies_index_mapping
							)
						})
						.collect::<Result<_>>()?,
					platform_specific_property_values: sub_entity
						.platform_specific_properties
						.iter()
						.flat_map(|(platform, props)| {
							props
								.iter()
								.map(|(x, y)| {
									Ok(SEntityTemplatePlatformSpecificProperty {
										platform: platform
											.as_str()
											.try_into()
											.map_err(|_| anyhow!("Invalid platform ID: {platform}"))?,
										post_init: y.post_init,
										property_value: convert_qn_property_to_game(
											x,
											y.property_type.to_owned(),
											&y.value,
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping,
											&factory_dependencies_index_mapping
										)?
									})
								})
								.collect_vec()
						})
						.collect::<Result<_>>()?
				})
			})
			.collect::<Result<_>>()?;

		blueprint.sub_entities = entity
			.entities
			.par_iter()
			.map(|(entity_id, sub_entity)| {
				Ok(STemplateBlueprintSubEntity {
					logical_parent: convert_qn_reference_to_game(
						sub_entity.parent.as_ref(),
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					)?,
					entity_type_resource_index: *blueprint_dependencies_index_mapping.get(&sub_entity.blueprint).ctx?
						as i32,
					entity_id: (*entity_id).into(),
					editor_only: sub_entity.editor_only,
					entity_name: sub_entity.name.to_owned().into(),
					property_aliases: sub_entity
						.property_aliases
						.iter()
						.map(|(aliased_name, aliases)| -> Result<_> {
							aliases
								.iter()
								.map(|alias| -> Result<_> {
									Ok(SEntityTemplatePropertyAlias {
										entity_id: entity_id_to_index_mapping
											.get(&alias.original_entity)
											.with_context(|| {
												format!(
													"Property alias referred to nonexistent entity ID: {}",
													alias.original_entity
												)
											})?
											.to_owned() as i32,
										alias_name: alias.original_property.to_owned().into(),
										property_name: aliased_name.to_owned().into()
									})
								})
								.collect::<Result<Vec<_>>>()
						})
						.collect::<Result<Vec<_>>>()?
						.into_iter()
						.flatten()
						.collect(),
					exposed_entities: sub_entity
						.exposed_entities
						.iter()
						.map(|(exposed_name, exposed_entity)| {
							Ok(SEntityTemplateExposedEntity {
								name: exposed_name.to_owned().into(),
								is_array: exposed_entity.is_array,
								targets: exposed_entity
									.refers_to
									.iter()
									.map(|target| {
										convert_qn_reference_to_game(
											Some(target),
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping
										)
									})
									.collect::<Result<_>>()?
							})
						})
						.collect::<Result<_>>()?,
					exposed_interfaces: sub_entity
						.exposed_interfaces
						.iter()
						.map(|(interface, implementor)| -> Result<_> {
							Ok((
								interface.to_owned().into(),
								entity_id_to_index_mapping
									.get(implementor)
									.context("Exposed interface referenced nonexistent local entity")?
									.to_owned() as i32
							))
						})
						.collect::<Result<Vec<_>>>()?,
					entity_subsets: vec![] // will be mutated later
				})
			})
			.collect::<Result<_>>()?;

		for (entity_index, (_, sub_entity)) in entity.entities.iter().enumerate() {
			for (subset, ents) in sub_entity.subsets.iter() {
				for ent in ents.iter() {
					let ent_subs = &mut blueprint
						.sub_entities
						.get_mut(
							*entity_id_to_index_mapping
								.get(ent)
								.context("Entity subset referenced nonexistent local entity")?
						)
						.ctx?
						.entity_subsets;

					if let Some((_, subset_entities)) = ent_subs.iter_mut().find(|(s, _)| s == subset) {
						subset_entities.entities.push(entity_index as i32);
					} else {
						ent_subs.push((
							subset.to_owned().into(),
							SEntityTemplateEntitySubset {
								entities: vec![entity_index as i32]
							}
						));
					};
				}
			}
		}

		blueprint.pin_connections = entity
			.entities
			.par_iter()
			.map(|(&entity_id, sub_entity)| -> Result<_> {
				Ok(sub_entity
					.events
					.iter()
					.map(|(evt, triggers)| {
						pin_connections_for_event(
							entity_id,
							evt,
							triggers,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping,
							&factory_dependencies_index_mapping
						)
					})
					.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
					.into_iter()
					.flatten()
					.collect::<Vec<_>>())
			})
			.collect::<Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		// slightly less code duplication than there used to be
		blueprint.input_pin_forwardings = entity
			.entities
			.par_iter()
			.map(|(&entity_id, sub_entity)| -> Result<_> {
				Ok(sub_entity
					.input_copying
					.iter()
					.map(|(evt, triggers)| {
						pin_connections_for_event(
							entity_id,
							evt,
							triggers,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping,
							&factory_dependencies_index_mapping
						)
					})
					.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
					.into_iter()
					.flatten()
					.collect::<Vec<_>>())
			})
			.collect::<Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		blueprint.output_pin_forwardings = entity
			.entities
			.par_iter()
			.map(|(&entity_id, sub_entity)| -> Result<_> {
				Ok(sub_entity
					.output_copying
					.iter()
					.map(|(evt, triggers)| {
						pin_connections_for_event(
							entity_id,
							evt,
							triggers,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping,
							&factory_dependencies_index_mapping
						)
					})
					.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
					.into_iter()
					.flatten()
					.collect::<Vec<_>>())
			})
			.collect::<Result<Vec<_>>>()?
			.into_iter()
			.flatten()
			.collect();

		Ok::<_, Error>((factory, factory_meta, blueprint, blueprint_meta))
	})?
}

#[cfg(feature = "rune")]
#[try_fn]
#[rune::function]
pub fn r_convert_to_rl(entity: &Entity) -> Result<(rune::Value, ResourceMetadata, rune::Value, ResourceMetadata)> {
	let (fac, fac_meta, blu, blu_meta) = convert_to_rl(entity)?;

	(
		from_value(to_value(fac)?)?,
		fac_meta,
		from_value(to_value(blu)?)?,
		blu_meta
	)
}

#[try_fn]
#[context("Failure getting pin connections for event")]
#[auto_context]
fn pin_connections_for_event(
	entity_id: EntityID,
	event: &str,
	triggers: &OrderMap<String, Vec<PinConnection>>,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	entity_id_to_index_mapping: &HashMap<EntityID, usize>,
	factory_dependencies_index_mapping: &HashMap<PathedID, usize>
) -> Result<Vec<SEntityTemplatePinConnection>> {
	triggers
		.iter()
		.map(|(trigger, entities)| -> Result<_> {
			entities
				.iter()
				.filter(|&trigger_entity| trigger_entity.entity_ref.external_scene.is_none())
				.map(|trigger_entity| {
					if trigger_entity.entity_ref.exposed_entity.is_some() {
						bail!("Pin connections cannot refer to exposed entities")
					}

					Ok(SEntityTemplatePinConnection {
						from_id: *entity_id_to_index_mapping.get(&entity_id).ctx? as i32,
						to_id: *entity_id_to_index_mapping
							.get(&trigger_entity.entity_ref.entity_id)
							.with_context(|| {
								format!(
									"Pin connection referred to nonexistent entity ID: {}",
									trigger_entity.entity_ref.entity_id
								)
							})? as i32,
						from_pin_name: event.to_owned().into(),
						to_pin_name: trigger.to_owned().into(),
						constant_pin_value: if let Some(value) = &trigger_entity.value {
							from_value(json!({
								"$type": value.property_type,
								"$val": convert_qn_property_value_to_game(
									&value.property_type,
									&value.value,
									factory,
									factory_meta,
									entity_id_to_index_mapping,
									factory_dependencies_index_mapping
								)?
							}))
							.context("Invalid pin value")?
						} else {
							ZVariant::new(())
						}
					})
				})
				.collect::<Result<Vec<SEntityTemplatePinConnection>>>()
		})
		.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
		.into_iter()
		.flatten()
		.collect_vec()
}

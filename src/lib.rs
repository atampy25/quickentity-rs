#![feature(try_find)]

pub mod entity;
pub mod patch;
pub mod variant;

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Error, Result, anyhow, bail};
use auto_context::auto_context;
use ecow::EcoString;
use entity::{
	Entity, EntityID, ExposedEntity, PinConnection, PinConnectionOverride, PinConnectionOverrideDelete, PropertyAlias,
	PropertyOverride, Ref, SubEntity, SubType
};
use fn_error_context::context;
use hitman_bin1::{
	game::h3::{
		SEntityTemplateEntitySubset, SEntityTemplateExposedEntity, SEntityTemplatePinConnection,
		SEntityTemplatePlatformSpecificProperty, SEntityTemplateProperty, SEntityTemplatePropertyAlias,
		SEntityTemplatePropertyOverride, SExternalEntityTemplatePinConnection, STemplateBlueprintSubEntity,
		STemplateEntityBlueprint, STemplateEntityFactory, STemplateFactorySubEntity, ZVariant
	},
	types::property::PropertyID
};
use hitman_commons::{
	game::GameVersion,
	metadata::{ResourceMetadata, ResourceReference, RuntimeID}
};
use itertools::Itertools;
use ordermap::OrderMap;
use patch::{
	ArrayPatchOperation, Patch, PatchOperation, PropertyOverrideConnection, SetPropertyValue, SubEntityOperation
};
use rayon::prelude::*;
use thiserror::Error;
use tryvial::try_fn;

use crate::{entity::Property, patch::ItemSelector, variant::Variant};

pub const PATCH_VERSION: u8 = 7;
pub const ENTITY_VERSION: f32 = 3.2;

// TODO: Array patches for property override properties? Simple properties in general?

/// The apply_patch function is not exposed to Rune because of the `emit` argument.
#[cfg(feature = "rune")]
pub fn rune_install(ctx: &mut rune::Context) -> Result<(), rune::ContextError> {
	ctx.install(entity::rune_module()?)?;
	ctx.install(patch::rune_module()?)?;
	ctx.install(variant::rune_module()?)?;

	let mut module = rune::Module::with_crate("quickentity_rs")?;
	module.function_meta(generate_patch__meta)?;
	module.function_meta(r_convert_to_qn)?;
	module.function_meta(r_convert_to_game)?;
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

#[derive(Error, Debug)]
pub enum Diagnostic {
	#[error("couldn't remove entity {entity} because it did not exist")]
	EntityAlreadyNonexistent { entity: EntityID },

	#[error("couldn't remove property {property} on {entity} because it did not exist")]
	PropertyAlreadyNonexistent { entity: EntityID, property: EcoString },

	#[error("couldn't remove platform specific properties for {platform} on {entity} because it did not exist")]
	PlatformAlreadyNonexistent { entity: EntityID, platform: EcoString },

	#[error("couldn't remove platform specific property {platform}/{property} on {entity} because it did not exist")]
	PlatformSpecificPropertyAlreadyNonexistent {
		entity: EntityID,
		platform: EcoString,
		property: EcoString
	},

	#[error("couldn't remove external scene {scene} because it did not exist")]
	ExternalSceneAlreadyNonexistent { scene: RuntimeID },

	#[error("in patching array {identifier}: {diagnostic}")]
	ArrayPatch {
		identifier: EcoString,
		diagnostic: ArrayPatchDiagnostic
	}
}

#[derive(Error, Debug)]
pub enum ArrayPatchDiagnostic {
	#[error("can't find element {element:?} to add before")]
	NoSuchElementBefore { element: ItemSelector },

	#[error("can't find element {element:?} to add after")]
	NoSuchElementAfter { element: ItemSelector },

	#[error("can't find element {element:?} to remove")]
	NoSuchElementToRemove { element: ItemSelector },

	#[error("can't find element {element:?} to replace")]
	NoSuchElementToReplace { element: ItemSelector }
}

#[try_fn]
#[context("Failure applying patch to entity")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[hotpath::measure]
pub fn apply_patch(entity: &mut Entity, patch: Patch, mut emit: impl FnMut(Diagnostic) + Send + Sync) -> Result<()> {
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
					let removed = entity.entities.remove(&value);

					if removed.is_none() {
						emit(Diagnostic::EntityAlreadyNonexistent { entity: value });
					}
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
							let removed = entity.properties.remove(&name);

							if removed.is_none() {
								emit(Diagnostic::PropertyAlreadyNonexistent {
									entity: entity_id,
									property: name
								});
							}
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

							let Variant::Array(_, value) = &mut item_to_patch.value else {
								bail!("PatchArrayPropertyValue expected property to be an array!");
							};

							apply_array_patch(value, array_patch, property_name, &mut emit)?;
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
							let removed = entity.platform_specific_properties.remove(&name);

							if removed.is_none() {
								emit(Diagnostic::PlatformAlreadyNonexistent {
									entity: entity_id,
									platform: name
								});
							}
						}

						SubEntityOperation::RemovePlatformSpecificPropertyByName(platform, name) => {
							let removed = entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("RemovePSPropertyByName couldn't find platform!")?
								.remove(&name);

							if removed.is_none() {
								emit(Diagnostic::PlatformSpecificPropertyAlreadyNonexistent {
									entity: entity_id,
									platform,
									property: name
								});
							} else if entity.platform_specific_properties.get(&platform).ctx?.is_empty() {
								entity.platform_specific_properties.remove(&platform);
							}
						}

						SubEntityOperation::SetPlatformSpecificPropertyValue(platform, property_name, value) => {
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

							let Variant::Array(_, value) = &mut item_to_patch.value else {
								bail!("PatchArrayPropertyValue expected property to be an array!");
							};

							apply_array_patch(value, array_patch, property_name, &mut emit)?;
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

				PatchOperation::AddPropertyOverrideConnection(connection) => {
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
						entities: vec![connection.entity],
						properties: {
							let mut x = OrderMap::new();
							x.insert(connection.property.to_owned(), connection.value.to_owned());
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

							let values_identical = x.properties.iter().all(|(prop_name, prop_val)| {
								prop_val.rough_eq(&property_override.properties[prop_name])
							});

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

				PatchOperation::RemovePropertyOverrideConnection(connection) => {
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
						entities: vec![connection.entity.to_owned()],
						properties: {
							let mut x = OrderMap::new();
							x.insert(connection.property.to_owned(), connection.value.to_owned());
							x
						}
					};

					unravelled_overrides.retain(|x| {
						x.entities != search.entities
							|| !x.properties.contains_key(&connection.property)
							|| !{ x.properties[&connection.property].rough_eq(&connection.value) }
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

							let values_identical = x.properties.iter().all(|(prop_name, prop_val)| {
								prop_val.rough_eq(&property_override.properties[prop_name])
							});

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
					} else {
						emit(Diagnostic::ExternalSceneAlreadyNonexistent { scene: value });
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

		anyhow::Ok(())
	})?;
}

#[try_fn]
#[context("Failure applying array patch")]
#[hotpath::measure]
pub fn apply_array_patch(
	arr: &mut Vec<Variant>,
	patch: Vec<ArrayPatchOperation>,
	identifier: EcoString,
	mut emit: impl FnMut(Diagnostic)
) -> Result<()> {
	#[hotpath::measure]
	fn find_selector_index(arr: &[Variant], selector: &ItemSelector) -> Option<usize> {
		let ItemSelector(wanted, occurrence) = selector;
		let mut seen = 0usize;

		for (idx, value) in arr.iter().enumerate() {
			if value.rough_eq(wanted) {
				if seen == *occurrence {
					return Some(idx);
				}

				seen += 1;
			}
		}

		None
	}

	for operation in patch {
		match operation {
			ArrayPatchOperation::Add { before, after, item } => {
				let mut missing_before: Option<ItemSelector> = None;
				let mut missing_after: Option<ItemSelector> = None;

				if let Some(selector) = before.as_ref() {
					if let Some(idx) = find_selector_index(arr, selector) {
						arr.insert(idx, item);
						continue;
					} else {
						missing_before = Some(selector.to_owned());
					}
				}

				if let Some(selector) = after.as_ref() {
					if let Some(idx) = find_selector_index(arr, selector) {
						arr.insert(idx + 1, item);
						continue;
					} else {
						missing_after = Some(selector.to_owned());
					}
				}

				if before.is_none() && after.is_none() {
					arr.push(item);
					continue;
				}

				if let Some(element) = missing_before {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementBefore { element }
					});
					continue;
				}

				if let Some(element) = missing_after {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementAfter { element }
					});
					continue;
				}
			}

			ArrayPatchOperation::Remove { item } => {
				if let Some(idx) = find_selector_index(arr, &item) {
					arr.remove(idx);
				} else {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementToRemove { element: item }
					});
				}
			}

			ArrayPatchOperation::Replace { item, new } => {
				if let Some(idx) = find_selector_index(arr, &item) {
					arr[idx] = new;
				} else {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementToReplace { element: item }
					});

					arr.push(new);
				}
			}
		}
	}
}

#[hotpath::measure]
fn generate_array_patch(original: &[Variant], modified: &[Variant]) -> Vec<ArrayPatchOperation> {
	#[hotpath::measure]
	fn selector_for_index(arr: &[Variant], index: usize) -> ItemSelector {
		let item = arr
			.get(index)
			.expect("selector_for_index called with out-of-bounds index")
			.to_owned();

		let occurrence = arr
			.iter()
			.take(index + 1)
			.filter(|candidate| candidate.rough_eq(&item))
			.count()
			.saturating_sub(1);

		ItemSelector(item, occurrence)
	}

	#[derive(Debug)]
	enum Action {
		Insert { index: usize, new_index: usize },
		Remove { index: usize },
		Replace { index: usize, new_index: usize }
	}

	let n = original.len();
	let m = modified.len();
	let mut dp = vec![vec![0usize; m + 1]; n + 1];

	for i in 0..=n {
		dp[i][0] = i;
	}

	for j in 0..=m {
		dp[0][j] = j;
	}

	for i in 1..=n {
		for j in 1..=m {
			if original[i - 1].rough_eq(&modified[j - 1]) {
				dp[i][j] = dp[i - 1][j - 1];
			} else {
				dp[i][j] = 1 + dp[i - 1][j - 1].min(dp[i - 1][j].min(dp[i][j - 1]));
			}
		}
	}

	let mut actions: Vec<Action> = Vec::new();
	let mut i = n;
	let mut j = m;

	while i > 0 || j > 0 {
		if i > 0 && j > 0 && original[i - 1].rough_eq(&modified[j - 1]) && dp[i][j] == dp[i - 1][j - 1] {
			i -= 1;
			j -= 1;
			continue;
		}

		if i > 0 && j > 0 && dp[i][j] == dp[i - 1][j - 1] + 1 {
			actions.push(Action::Replace {
				index: i - 1,
				new_index: j - 1
			});
			i -= 1;
			j -= 1;
			continue;
		}

		if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
			actions.push(Action::Remove { index: i - 1 });
			i -= 1;
			continue;
		}

		if j > 0 && dp[i][j] == dp[i][j - 1] + 1 {
			actions.push(Action::Insert {
				index: i,
				new_index: j - 1
			});
			j -= 1;
			continue;
		}
	}

	let mut ops: Vec<ArrayPatchOperation> = Vec::new();
	let mut working = original.to_vec();

	for action in actions {
		match action {
			Action::Insert { index, new_index } => {
				let before = if index < working.len() {
					Some(selector_for_index(&working, index))
				} else {
					None
				};

				let after = if index > 0 {
					Some(selector_for_index(&working, index - 1))
				} else {
					None
				};

				let item = modified
					.get(new_index)
					.expect("insert referencing out-of-bounds new_index")
					.to_owned();

				ops.push(ArrayPatchOperation::Add {
					before,
					after,
					item: item.to_owned()
				});
				working.insert(index, item);
			}

			Action::Remove { index } => {
				let selector = selector_for_index(&working, index);
				ops.push(ArrayPatchOperation::Remove {
					item: selector.to_owned()
				});
				working.remove(index);
			}

			Action::Replace { index, new_index } => {
				let selector = selector_for_index(&working, index);
				let new_item = modified
					.get(new_index)
					.expect("replace referencing out-of-bounds new_index")
					.to_owned();

				ops.push(ArrayPatchOperation::Replace {
					item: selector.to_owned(),
					new: new_item.to_owned()
				});

				working[index] = new_item;
			}
		}
	}

	ops
}

#[try_fn]
#[context("Failure generating patch from two entities")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[cfg_attr(feature = "rune", rune::function(keep))]
#[hotpath::measure]
pub fn generate_patch(original: &Entity, modified: &Entity) -> Result<Patch> {
	if original.quickentity_version != modified.quickentity_version {
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
					if !old_property_data.value.rough_eq(&new_property_data.value) {
						if let Variant::Array(_, old_value) = &old_property_data.value
							&& let Variant::Array(_, new_value) = &new_property_data.value
						{
							let ops = generate_array_patch(old_value, new_value);

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
							if old_property_data.value != new_property_data.value {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyValue(
										platform_name.to_owned(),
										property_name.to_owned(),
										new_property_data.value.to_owned()
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
							property: prop_name.to_owned(),
							value: prop_val.to_owned()
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
							property: prop_name.to_owned(),
							value: prop_val.to_owned()
						})
						.collect_vec()
				})
				.collect_vec()
		})
		.collect();

	for x in &original_unravelled_overrides {
		if !modified_unravelled_overrides
			.iter()
			.any(|val| val.entity == x.entity && val.property == x.property && val.value.rough_eq(&x.value))
		{
			patch.push(PatchOperation::RemovePropertyOverrideConnection(x.to_owned()))
		}
	}

	for x in &modified_unravelled_overrides {
		if !original_unravelled_overrides
			.iter()
			.any(|val| val.entity == x.entity && val.property == x.property && val.value.rough_eq(&x.value))
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
#[context("Failure converting string property name to ID")]
#[auto_context]
#[hotpath::measure]
fn convert_string_property_name_to_id(property_name: &str) -> Result<PropertyID> {
	if let Ok(i) = property_name.parse::<u32>()
		&& !PropertyID::is_known(property_name)
	{
		PropertyID::from(i)
	} else {
		PropertyID::from(property_name)
	}
}

#[try_fn]
#[context("Failure getting factory dependencies")]
#[auto_context]
#[hotpath::measure]
fn get_factory_references(entity: &Entity) -> Result<Vec<ResourceReference>> {
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
						.filter_map(|(_, prop)| {
							if let Variant::Resource(res) = &prop.value {
								res.to_owned()
							} else {
								None
							}
						})
						.collect_vec(),
					sub_entity
						.properties
						.iter()
						.flat_map(|(_, prop)| match &prop.value {
							Variant::Array(ty, items) if ty == "ZRuntimeResourceID" => items
								.iter()
								.filter_map(|item| {
									let Variant::Resource(res) = item else { unreachable!() };
									res.to_owned()
								})
								.collect_vec(),

							_ => vec![]
						})
						.collect_vec(),
					sub_entity
						.platform_specific_properties
						.iter()
						.map(|(_, props)| -> Result<_> {
							Ok([
								props
									.iter()
									.filter_map(|(_, prop)| {
										if let Variant::Resource(res) = &prop.value {
											res.to_owned()
										} else {
											None
										}
									})
									.collect_vec(),
								props
									.iter()
									.flat_map(|(_, prop)| match &prop.value {
										Variant::Array(ty, items) if ty == "ZRuntimeResourceID" => items
											.iter()
											.filter_map(|item| {
												let Variant::Resource(res) = item else { unreachable!() };
												res.to_owned()
											})
											.collect_vec(),

										_ => vec![]
									})
									.collect_vec()
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
						.filter_map(|(_, prop)| {
							if let Variant::Resource(res) = prop {
								res.to_owned()
							} else {
								None
							}
						})
						.collect_vec(),
					properties
						.iter()
						.flat_map(|(_, prop)| match prop {
							Variant::Array(ty, items) if ty == "ZRuntimeResourceID" => items
								.iter()
								.filter_map(|item| {
									let Variant::Resource(res) = item else { unreachable!() };
									res.to_owned()
								})
								.collect_vec(),

							_ => vec![]
						})
						.collect_vec()
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

#[hotpath::measure]
fn get_blueprint_references(entity: &Entity) -> Vec<ResourceReference> {
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
#[context("Failure converting game entity to QN")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[hotpath::measure]
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
				.par_iter()
				.zip(&blueprint.sub_entities)
				.map(
					|(sub_entity_factory, sub_entity_blueprint)| -> Result<(EntityID, SubEntity)> {
						Ok((
							sub_entity_blueprint.entity_id.into(),
							SubEntity {
								name: sub_entity_blueprint.entity_name.to_owned(),
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
								parent: Ref::from_game(
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
												.unwrap_or_else(|| property.property_id.0.to_string().into()),
											Property {
												value: Variant::from_game(
													&property.value,
													factory,
													factory_meta,
													blueprint,
													convert_lossless
												)?,
												post_init: false
											}
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
													.unwrap_or_else(|| property.property_id.0.to_string().into()),
												Property {
													value: Variant::from_game(
														&property.value,
														factory,
														factory_meta,
														blueprint,
														convert_lossless
													)?,
													post_init: true
												}
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
												.map(|property| -> Result<_> {
													Ok((
														// we do a little code duplication
														property
															.property_value
															.property_id
															.as_name()
															.map(|x| x.to_owned())
															.unwrap_or_else(|| {
																property.property_value.property_id.0.to_string().into()
															}),
														Property {
															value: Variant::from_game(
																&property.property_value.value,
																factory,
																factory_meta,
																blueprint,
																convert_lossless
															)?,
															post_init: property.post_init
														}
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
												property_name,
												aliases
													.into_iter()
													.map(|alias| {
														Ok(PropertyAlias {
															original_property: alias.alias_name.to_owned(),
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
											exposed_entity.name.to_owned(),
											ExposedEntity {
												is_array: exposed_entity.is_array.to_owned(),
												refers_to: exposed_entity
													.targets
													.iter()
													.map(|target| {
														Ref::from_game(target, factory, blueprint, factory_meta)?
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
											interface.to_owned(),
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
					}
				)
				.collect::<Result<OrderMap<EntityID, SubEntity>>>()?,
			external_scenes: factory
				.external_scene_type_indices_in_resource_header
				.iter()
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
					Ref::from_game(x, factory, blueprint, factory_meta)?
						.context("Override delete references must not be null")
				})
				.collect::<Result<_>>()?,
			pin_connection_override_deletes: blueprint
				.pin_connection_override_deletes
				.par_iter()
				.map(|x| {
					Ok(PinConnectionOverrideDelete {
						from_entity: Ref::from_game(&x.from_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override delete references must not be null")?,
						to_entity: Ref::from_game(&x.to_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override delete references must not be null")?,
						from_pin: x.from_pin_name.to_owned(),
						to_pin: x.to_pin_name.to_owned(),
						value: if x.constant_pin_value.is::<()>() {
							None
						} else {
							Some(Variant::from_game(
								&x.constant_pin_value,
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?)
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
						from_entity: Ref::from_game(&x.from_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override references must not be null")?,
						to_entity: Ref::from_game(&x.to_entity, factory, blueprint, factory_meta)?
							.context("Pin connection override references must not be null")?,
						from_pin: x.from_pin_name.to_owned(),
						to_pin: x.to_pin_name.to_owned(),
						value: if x.constant_pin_value.is::<()>() {
							None
						} else {
							Some(Variant::from_game(
								&x.constant_pin_value,
								factory,
								factory_meta,
								blueprint,
								convert_lossless
							)?)
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
			quickentity_version: 3.2,
			extra_factory_references: vec![],
			extra_blueprint_references: vec![],
			comments: vec![]
		};

		{
			let depends = get_factory_references(&entity)?.into_iter().collect::<HashSet<_>>();

			entity.extra_factory_references = factory_meta
				.references
				.iter()
				.filter(|x| !depends.contains(x))
				.cloned()
				.collect();
		}

		{
			let depends = get_blueprint_references(&entity).into_iter().collect::<HashSet<_>>();

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
				.entry(pin.from_pin_name.to_owned())
				.or_default()
				.entry(pin.to_pin_name.to_owned())
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
						Some(Variant::from_game(
							&pin.constant_pin_value,
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)?)
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
				.entry(pin_connection_override.from_pin_name.to_owned())
				.or_default()
				.entry(pin_connection_override.to_pin_name.to_owned())
				.or_default()
				.push(PinConnection {
					entity_ref: Ref::from_game(&pin_connection_override.to_entity, factory, blueprint, factory_meta)?
						.context("Pin connection references must not be null")?,
					value: if pin_connection_override.constant_pin_value.is::<()>() {
						None
					} else {
						Some(Variant::from_game(
							&pin_connection_override.constant_pin_value,
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)?)
					}
				});
		}

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
				.entry(forwarding.from_pin_name.to_owned())
				.or_default()
				.entry(forwarding.to_pin_name.to_owned())
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
						Some(Variant::from_game(
							&forwarding.constant_pin_value,
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)?)
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
				.entry(forwarding.from_pin_name.to_owned())
				.or_default()
				.entry(forwarding.to_pin_name.to_owned())
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
						Some(Variant::from_game(
							&forwarding.constant_pin_value,
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)?)
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
						.entry(subset.to_owned())
						.or_default()
						.push(sub_entity.entity_id.into());
				}
			}
		}

		let mut pass1: Vec<PropertyOverride> = Vec::default();

		for property_override in &factory.property_overrides {
			let ents = vec![
				Ref::from_game(&property_override.property_owner, factory, blueprint, factory_meta)?
					.context("Property override references must not be null")?,
			];

			let props = [(
				property_override
					.property_value
					.property_id
					.as_name()
					.map(|x| x.to_owned())
					.unwrap_or_else(|| property_override.property_value.property_id.0.to_string().into()),
				{
					Variant::from_game(
						&property_override.property_value.value,
						factory,
						factory_meta,
						blueprint,
						convert_lossless
					)?
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
	use serde_json::{from_value, to_value};

	let factory = from_value(to_value(factory)?)?;
	let blueprint = from_value(to_value(blueprint)?)?;
	convert_to_qn(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
}

#[try_fn]
#[context("Failure converting QN entity to game")]
#[auto_context]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[hotpath::measure]
pub fn convert_to_game(
	entity: &Entity,
	version: GameVersion
) -> Result<(
	STemplateEntityFactory,
	ResourceMetadata,
	STemplateEntityBlueprint,
	ResourceMetadata
)> {
	if entity.quickentity_version != ENTITY_VERSION {
		bail!(
			"Invalid QuickEntity version; expected {}, got {}",
			ENTITY_VERSION,
			entity.quickentity_version
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
				get_factory_references(entity)?,
				entity.extra_factory_references.to_owned()
			]
			.concat()
		};

		let factory_dependencies_index_mapping: HashMap<RuntimeID, usize> = factory_meta
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
				.map(|override_delete| override_delete.to_game(&factory, &factory_meta, &entity_id_to_index_mapping))
				.collect::<Result<_>>()?,
			pin_connection_overrides: [
				entity
					.pin_connection_overrides
					.par_iter()
					.map(|pin_connection_override| {
						Ok(SExternalEntityTemplatePinConnection {
							from_entity: pin_connection_override.from_entity.to_game(
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping
							)?,
							to_entity: pin_connection_override.to_entity.to_game(
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping
							)?,
							from_pin_name: pin_connection_override.from_pin.to_owned(),
							to_pin_name: pin_connection_override.to_pin.to_owned(),
							constant_pin_value: {
								if let Some(property) = pin_connection_override.value.as_ref() {
									property.to_game(
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping,
										&factory_dependencies_index_mapping
									)?
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
													from_entity: Ref::local(*entity_id).to_game(
														&factory,
														&factory_meta,
														&entity_id_to_index_mapping
													)?,
													to_entity: trigger_entity.entity_ref.to_game(
														&factory,
														&factory_meta,
														&entity_id_to_index_mapping
													)?,
													from_pin_name: event.to_owned(),
													to_pin_name: trigger.to_owned(),
													constant_pin_value: if let Some(value) = &trigger_entity.value {
														value.to_game(
															&factory,
															&factory_meta,
															&entity_id_to_index_mapping,
															&factory_dependencies_index_mapping
														)?
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
						from_entity: pin_connection_override_delete.from_entity.to_game(
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						)?,
						to_entity: pin_connection_override_delete.to_entity.to_game(
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						)?,
						from_pin_name: pin_connection_override_delete.from_pin.to_owned(),
						to_pin_name: pin_connection_override_delete.to_pin.to_owned(),
						constant_pin_value: {
							if let Some(property) = pin_connection_override_delete.value.as_ref() {
								property.to_game(
									&factory,
									&factory_meta,
									&entity_id_to_index_mapping,
									&factory_dependencies_index_mapping
								)?
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
				get_blueprint_references(entity),
				entity.extra_blueprint_references.to_owned()
			]
			.concat()
		};

		let blueprint_dependencies_index_mapping: HashMap<RuntimeID, usize> = blueprint_meta
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
									property_owner: ext_entity.to_game(
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping
									)?,
									property_value: SEntityTemplateProperty {
										property_id: convert_string_property_name_to_id(property)?,
										value: overridden.to_game(
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping,
											&factory_dependencies_index_mapping
										)?
									}
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
					logical_parent: Ref::to_game_opt(
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
						.filter(|(_, property)| !property.post_init)
						.map(|(name, property)| {
							Ok(SEntityTemplateProperty {
								property_id: convert_string_property_name_to_id(name)?,
								value: property.value.to_game(
									&factory,
									&factory_meta,
									&entity_id_to_index_mapping,
									&factory_dependencies_index_mapping
								)?
							})
						})
						.collect::<Result<_>>()?,
					post_init_property_values: sub_entity
						.properties
						.iter()
						.filter(|(_, property)| property.post_init)
						.map(|(name, property)| {
							Ok(SEntityTemplateProperty {
								property_id: convert_string_property_name_to_id(name)?,
								value: property.value.to_game(
									&factory,
									&factory_meta,
									&entity_id_to_index_mapping,
									&factory_dependencies_index_mapping
								)?
							})
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
											.parse()
											.map_err(|_| anyhow!("Invalid platform ID: {platform}"))?,
										post_init: y.post_init,
										property_value: SEntityTemplateProperty {
											property_id: convert_string_property_name_to_id(x)?,
											value: y.value.to_game(
												&factory,
												&factory_meta,
												&entity_id_to_index_mapping,
												&factory_dependencies_index_mapping
											)?
										}
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
					logical_parent: Ref::to_game_opt(
						sub_entity.parent.as_ref(),
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					)?,
					entity_type_resource_index: *blueprint_dependencies_index_mapping.get(&sub_entity.blueprint).ctx?
						as i32,
					entity_id: (*entity_id).into(),
					editor_only: sub_entity.editor_only,
					entity_name: sub_entity.name.to_owned(),
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
										alias_name: alias.original_property.to_owned(),
										property_name: aliased_name.to_owned()
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
								name: exposed_name.to_owned(),
								is_array: exposed_entity.is_array,
								targets: exposed_entity
									.refers_to
									.iter()
									.map(|target| target.to_game(&factory, &factory_meta, &entity_id_to_index_mapping))
									.collect::<Result<_>>()?
							})
						})
						.collect::<Result<_>>()?,
					exposed_interfaces: sub_entity
						.exposed_interfaces
						.iter()
						.map(|(interface, implementor)| -> Result<_> {
							Ok((
								interface.to_owned(),
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
							subset.to_owned(),
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
pub fn r_convert_to_game(
	entity: &Entity,
	version: GameVersion
) -> Result<(rune::Value, ResourceMetadata, rune::Value, ResourceMetadata)> {
	use serde_json::{from_value, to_value};

	let (fac, fac_meta, blu, blu_meta) = convert_to_game(entity, version)?;

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
#[hotpath::measure]
fn pin_connections_for_event(
	entity_id: EntityID,
	event: &EcoString,
	triggers: &OrderMap<EcoString, Vec<PinConnection>>,
	factory: &STemplateEntityFactory,
	factory_meta: &ResourceMetadata,
	entity_id_to_index_mapping: &HashMap<EntityID, usize>,
	factory_dependencies_index_mapping: &HashMap<RuntimeID, usize>
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
						from_pin_name: event.to_owned(),
						to_pin_name: trigger.to_owned(),
						constant_pin_value: if let Some(value) = &trigger_entity.value {
							value.to_game(
								factory,
								factory_meta,
								entity_id_to_index_mapping,
								factory_dependencies_index_mapping
							)?
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

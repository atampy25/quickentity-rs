#![feature(try_find)]
#![feature(never_type)]

pub mod entity;
pub mod game;
pub mod patch;
pub mod variant;

use anyhow::{Context, Result, anyhow, bail};
use auto_context::auto_context;
use ecow::{EcoString, string::ToEcoString};
use entity::{Entity, EntityID, PropertyOverride};
use fn_error_context::context;
use glacier_bin1::types::{property::PropertyID, resource::ZRuntimeResourceID};
use glacier_commons::{
	game::GamePlatform,
	metadata::{ResourceMetadata, ResourceReference, RuntimeID}
};
use identity_hash::BuildIdentityHasher;
use itertools::{EitherOrBoth, Itertools};
use patch::{ArrayPatchOperation, Patch, PatchOperation, PropertyOverrideConnection, SubEntityOperation};
use rayon::prelude::*;
use thiserror::Error;
use tryvial::try_fn;

use crate::{
	entity::{
		ExposedEntity, LocalPinConnection, PinConnection, PinConnectionOverride, PinConnectionOverrideDelete, Property,
		PropertyAlias, Ref, SubEntity, SubType
	},
	game::{FromQuickEntity, ToGame, ToQuickEntity},
	patch::{ItemSelector, VariantPatch},
	variant::Variant
};

#[cfg(feature = "rune")]
use glacier_commons::game::GlacierGame;

pub const PATCH_VERSION: u8 = 7;
pub const ENTITY_VERSION: f32 = 3.2;

pub(crate) type HashMap<K, V, S = rapidhash::fast::RandomState> = std::collections::HashMap<K, V, S>;
pub(crate) type HashSet<K, S = rapidhash::fast::RandomState> = std::collections::HashSet<K, S>;
pub(crate) type OrderMap<K, V, S = rapidhash::fast::RandomState> = ordermap::OrderMap<K, V, S>;

/// The apply_patch function is not exposed to Rune because of the `emit` argument.
#[cfg(feature = "rune")]
pub fn rune_install(ctx: &mut rune::Context) -> Result<(), rune::ContextError> {
	ctx.install(entity::rune_module()?)?;
	ctx.install(patch::rune_module()?)?;
	ctx.install(variant::rune_module()?)?;

	let mut module = rune::Module::with_crate("quickentity_rs")?;
	module.function_meta(generate_patch__meta)?;
	ctx.install(module)?;

	Ok(())
}

#[derive(Error, Debug)]
pub enum Diagnostic {
	#[error("entity {entity} already existed and was replaced")]
	EntityAlreadyExisted { entity: EntityID },

	#[error("{0}")]
	AlreadyNonexistent(#[from] AlreadyNonexistentDiagnostic),

	#[error("in patching array {identifier}: {diagnostic}")]
	ArrayPatch {
		identifier: EcoString,
		diagnostic: ArrayPatchDiagnostic
	}
}

#[derive(Error, Debug)]
pub enum AlreadyNonexistentDiagnostic {
	#[error("couldn't remove entity {entity} because it did not exist")]
	Entity { entity: EntityID },

	#[error("couldn't remove property {property} on {entity} because it did not exist")]
	Property { entity: EntityID, property: EcoString },

	#[error("couldn't remove platform property {platform}/{property} on {entity} because it did not exist")]
	PlatformSpecificProperty {
		entity: EntityID,
		platform: EcoString,
		property: EcoString
	},

	#[error("couldn't remove external scene {scene} because it did not exist")]
	ExternalScene { scene: RuntimeID },

	#[error("couldn't remove extra factory reference {reference:?} because it did not exist")]
	ExtraFactoryReference { reference: ResourceReference },

	#[error("couldn't remove extra blueprint reference {reference:?} because it did not exist")]
	ExtraBlueprintReference { reference: ResourceReference }
}

#[derive(Error, Debug)]
pub enum ArrayPatchDiagnostic {
	#[error("couldn't find any elements to add before/after")]
	NoSuchElementsBeforeAfter,

	#[error("couldn't find element {element:?} to remove")]
	NoSuchElementToRemove { element: ItemSelector },

	#[error("couldn't find element {element:?} to replace")]
	NoSuchElementToReplace { element: ItemSelector }
}

/// Apply a patch to an entity. Returns whether the entity was modified.
#[try_fn]
#[context("Failure applying patch to entity")]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[hotpath::measure]
pub fn apply_patch(entity: &mut Entity, patch: Patch, mut emit: impl FnMut(Diagnostic) + Send + Sync) -> Result<bool> {
	if patch.patch_version != PATCH_VERSION {
		bail!(
			"Invalid patch version; expected {}, got {}",
			PATCH_VERSION,
			patch.patch_version
		);
	}

	let patch: Vec<PatchOperation> = patch.patch;

	let mut modified = false;

	for (idx, operation) in patch.into_iter().enumerate() {
		modified |=
			apply_patch_operation(entity, operation, &mut emit).context(format!("Failure applying operation {idx}"))?;
	}

	modified
}

#[try_fn]
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
#[hotpath::measure]
fn apply_patch_operation(
	entity: &mut Entity,
	operation: PatchOperation,
	mut emit: impl FnMut(Diagnostic)
) -> Result<bool> {
	let mut modified = false;

	match operation {
		PatchOperation::SetRootEntity(value) => {
			modified = entity.root_entity != value;
			entity.root_entity = value;
		}

		PatchOperation::SetSubType(value) => {
			modified = entity.sub_type != value;
			entity.sub_type = value;
		}

		PatchOperation::RemoveEntity(value) => {
			let removed = entity.sub_entities.remove(&value);
			modified = removed.is_some();

			if !modified {
				emit(AlreadyNonexistentDiagnostic::Entity { entity: value }.into());
			}
		}

		PatchOperation::AddEntity(id, data) => {
			if let Some(existing) = entity.sub_entities.get(&id) {
				emit(Diagnostic::EntityAlreadyExisted { entity: id });
				modified = *data != *existing;
			} else {
				modified = true;
			}

			entity.sub_entities.insert(id, *data);
		}

		PatchOperation::PatchEntity(entity_id, op) => {
			let entity = entity
				.sub_entities
				.get_mut(&entity_id)
				.with_context(|| format!("SubEntityOperation couldn't find entity ID: {entity_id}!"))?;

			match op {
				SubEntityOperation::SetParent(value) => {
					modified = entity.parent != value;
					entity.parent = value;
				}

				SubEntityOperation::SetName(value) => {
					modified = entity.name != value;
					entity.name = value;
				}

				SubEntityOperation::SetFactory(value) => {
					modified = entity.factory != value;
					entity.factory = value;
				}

				SubEntityOperation::SetBlueprint(value) => {
					modified = entity.blueprint != value;
					entity.blueprint = value;
				}

				SubEntityOperation::SetEditorOnly(value) => {
					modified = entity.editor_only != value;
					entity.editor_only = value;
				}

				SubEntityOperation::AddExcludedPlatform(value) => {
					if !entity.excluded_platforms.contains(&value) {
						modified = true;
						entity.excluded_platforms.push(value);
					}
				}

				SubEntityOperation::RemoveExcludedPlatform(value) => {
					if entity.excluded_platforms.contains(&value) {
						modified = true;
						entity.excluded_platforms.retain(|x| *x != value);
					}
				}

				SubEntityOperation::AddProperty(name, data) => {
					if let Some(existing) = entity.properties.get(&name) {
						modified = data != *existing;
					} else {
						modified = true;
					}

					entity.properties.insert(name, data);
				}

				SubEntityOperation::RemoveProperty(name) => {
					let removed = entity.properties.remove(&name);
					modified = removed.is_some();

					if !modified {
						emit(
							AlreadyNonexistentDiagnostic::Property {
								entity: entity_id,
								property: name
							}
							.into()
						);
					}
				}

				SubEntityOperation::PatchPropertyValue(property_name, patch, post_init) => match patch {
					VariantPatch::Set(value) => {
						if let Some(existing) = entity.properties.get(&property_name) {
							modified = value != existing.value;
						} else {
							modified = true;
						}

						entity
							.properties
							.entry(property_name)
							.or_insert_with(|| Property {
								value: Variant::Ref(None),
								post_init
							})
							.value = value;
					}

					VariantPatch::ArrayPatch(patch) => {
						let property = entity
							.properties
							.get_mut(&property_name)
							.context("PatchPropertyValue couldn't find expected property!")?;

						let Variant::Array(_, value) = &mut property.value else {
							bail!("PatchPropertyValue expected property to be an array!");
						};

						modified = apply_array_patch(value, patch, property_name, &mut emit)?;
					}
				},

				SubEntityOperation::SetPropertyPostInit(name, value) => {
					let ent = entity
						.properties
						.get_mut(&name)
						.context("SetPropertyPostInit couldn't find expected property!")?;

					modified = value != ent.post_init;
					ent.post_init = value;
				}

				SubEntityOperation::AddPlatformSpecificProperty(platform, name, data) => {
					let entry = entity.platform_specific_properties.entry(platform).or_default();

					if let Some(existing) = entry.get(&name) {
						modified = data != *existing;
					} else {
						modified = true;
					}

					entry.insert(name, data);
				}

				SubEntityOperation::RemovePlatformSpecificProperty(platform, name) => {
					let removed = entity
						.platform_specific_properties
						.get_mut(&platform)
						.context("RemovePSPropertyByName couldn't find platform!")?
						.remove(&name);

					modified = removed.is_some();

					if !modified {
						emit(
							AlreadyNonexistentDiagnostic::PlatformSpecificProperty {
								entity: entity_id,
								platform,
								property: name
							}
							.into()
						);
					} else if entity.platform_specific_properties.get(&platform).unwrap().is_empty() {
						entity.platform_specific_properties.remove(&platform);
					}
				}

				SubEntityOperation::PatchPlatformSpecificPropertyValue(platform, property_name, patch, post_init) => {
					match patch {
						VariantPatch::Set(value) => {
							let entry = entity.platform_specific_properties.entry(platform).or_default();

							if let Some(existing) = entry.get(&property_name) {
								modified = value != existing.value;
							} else {
								modified = true;
							}

							entry
								.entry(property_name)
								.or_insert_with(|| Property {
									value: Variant::Ref(None),
									post_init
								})
								.value = value;
						}

						VariantPatch::ArrayPatch(patch) => {
							let property = entity
								.platform_specific_properties
								.get_mut(&platform)
								.context("PatchPlatformSpecificPropertyValue couldn't find expected platform!")?
								.get_mut(&property_name)
								.context("PatchPlatformSpecificPropertyValue couldn't find expected property!")?;

							let Variant::Array(_, value) = &mut property.value else {
								bail!("PatchPlatformSpecificPropertyValue expected property to be an array!");
							};

							modified = apply_array_patch(value, patch, property_name, &mut emit)?;
						}
					}
				}

				SubEntityOperation::SetPlatformSpecificPropertyPostInit(platform, name, value) => {
					let ent = entity
						.platform_specific_properties
						.get_mut(&platform)
						.context("SetPSPropertyPostInit couldn't find expected platform!")?
						.get_mut(&name)
						.context("SetPSPropertyPostInit couldn't find expected property!")?;

					modified = value != ent.post_init;
					ent.post_init = value;
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

					entity
						.events
						.get_mut(&event)
						.unwrap()
						.get_mut(&trigger)
						.unwrap()
						.remove(ind);

					modified = true;

					if entity.events.get(&event).unwrap().get(&trigger).unwrap().is_empty() {
						entity.events.get_mut(&event).unwrap().remove(&trigger);
					}

					if entity.events.get(&event).unwrap().is_empty() {
						entity.events.remove(&event);
					}
				}

				SubEntityOperation::AddEventConnection(event, trigger, reference) => {
					entity
						.events
						.entry(event)
						.or_default()
						.entry(trigger)
						.or_default()
						.push(reference);

					modified = true;
				}

				SubEntityOperation::RemoveInputForwarding(event, trigger, reference) => {
					let ind = entity
						.input_forwardings
						.get(&event)
						.context("RemoveInputCopyConnection couldn't find input!")?
						.get(&trigger)
						.context("RemoveInputCopyConnection couldn't find trigger!")?
						.iter()
						.position(|x| *x == reference)
						.context("RemoveInputCopyConnection couldn't find reference!")?;

					entity
						.input_forwardings
						.get_mut(&event)
						.unwrap()
						.get_mut(&trigger)
						.unwrap()
						.remove(ind);

					modified = true;

					if entity
						.input_forwardings
						.get(&event)
						.unwrap()
						.get(&trigger)
						.unwrap()
						.is_empty()
					{
						entity.input_forwardings.get_mut(&event).unwrap().remove(&trigger);
					}

					if entity.input_forwardings.get(&event).unwrap().is_empty() {
						entity.input_forwardings.remove(&event);
					}
				}

				SubEntityOperation::AddInputForwarding(event, trigger, reference) => {
					entity
						.input_forwardings
						.entry(event)
						.or_default()
						.entry(trigger)
						.or_default()
						.push(reference);

					modified = true;
				}

				SubEntityOperation::RemoveOutputForwarding(event, trigger, reference) => {
					let ind = entity
						.output_forwardings
						.get(&event)
						.context("RemoveOutputCopyConnection couldn't find event!")?
						.get(&trigger)
						.context("RemoveOutputCopyConnection couldn't find propagate!")?
						.iter()
						.position(|x| *x == reference)
						.context("RemoveOutputCopyConnection couldn't find reference!")?;

					entity
						.output_forwardings
						.get_mut(&event)
						.unwrap()
						.get_mut(&trigger)
						.unwrap()
						.remove(ind);

					modified = true;

					if entity
						.output_forwardings
						.get(&event)
						.unwrap()
						.get(&trigger)
						.unwrap()
						.is_empty()
					{
						entity.output_forwardings.get_mut(&event).unwrap().remove(&trigger);
					}

					if entity.output_forwardings.get(&event).unwrap().is_empty() {
						entity.output_forwardings.remove(&event);
					}
				}

				SubEntityOperation::AddOutputForwarding(event, trigger, reference) => {
					entity
						.output_forwardings
						.entry(event)
						.or_default()
						.entry(trigger)
						.or_default()
						.push(reference);

					modified = true;
				}

				SubEntityOperation::AddPropertyAlias(alias, data) => {
					entity.property_aliases.entry(alias).or_default().push(data);
					modified = true;
				}

				SubEntityOperation::RemovePropertyAlias(alias, data) => {
					let connection = entity
						.property_aliases
						.get(&alias)
						.context("RemoveConnectionForPropertyAlias couldn't find alias!")?
						.iter()
						.position(|x| *x == data)
						.context("RemoveConnectionForPropertyAlias couldn't find connection!")?;

					entity.property_aliases.get_mut(&alias).unwrap().remove(connection);

					modified = true;

					if entity.property_aliases.get(&alias).unwrap().is_empty() {
						entity.property_aliases.remove(&alias);
					}
				}

				SubEntityOperation::SetExposedEntity(name, data) => {
					modified = entity.exposed_entities.get(&name) != Some(&data);
					entity.exposed_entities.insert(name, data);
				}

				SubEntityOperation::RemoveExposedEntity(name) => {
					entity
						.exposed_entities
						.remove(&name)
						.context("RemoveExposedEntity couldn't find exposed entity to remove!")?;

					modified = true;
				}

				SubEntityOperation::SetExposedInterface(name, implementor) => {
					modified = entity.exposed_interfaces.get(&name) != Some(&implementor);
					entity.exposed_interfaces.insert(name, implementor);
				}

				SubEntityOperation::RemoveExposedInterface(name) => {
					entity
						.exposed_interfaces
						.remove(&name)
						.context("RemoveExposedInterface couldn't find exposed entity to remove!")?;

					modified = true;
				}

				SubEntityOperation::AddSubset(name, ent) => {
					entity.subsets.entry(name).or_default().push(ent);
					modified = true;
				}

				SubEntityOperation::RemoveSubset(name, ent) => {
					let ind = entity
						.subsets
						.get(&name)
						.context("RemoveSubset couldn't find subset to remove from!")?
						.iter()
						.position(|x| *x == ent)
						.context("RemoveSubset couldn't find the entity to remove from the subset!")?;

					entity.subsets.get_mut(&name).unwrap().remove(ind);

					modified = true;
				}
			}
		}

		PatchOperation::AddPropertyOverrideConnection(connection) => {
			let previous_overrides = entity.property_overrides.to_owned();
			let mut unravelled_overrides: Vec<PropertyOverride> = vec![];

			for property_override in &entity.property_overrides {
				for ent in &property_override.entities {
					for (prop_name, prop_override) in &property_override.properties {
						unravelled_overrides.push(PropertyOverride {
							entities: vec![ent.to_owned()],
							properties: {
								let mut x = OrderMap::default();
								x.insert(prop_name.to_owned(), prop_override.to_owned());
								x
							},
							runtime_editable: if property_override.runtime_editable.contains(prop_name) {
								vec![prop_name.to_owned()]
							} else {
								vec![]
							}
						});
					}
				}
			}

			unravelled_overrides.push(PropertyOverride {
				entities: vec![connection.entity],
				properties: {
					let mut x = OrderMap::default();
					x.insert(connection.property.to_owned(), connection.value.to_owned());
					x
				},
				runtime_editable: if connection.runtime_editable {
					vec![connection.property.to_owned()]
				} else {
					vec![]
				}
			});

			let mut merged_overrides: Vec<PropertyOverride> = vec![];

			let mut pass1: Vec<PropertyOverride> = Vec::default();

			for property_override in unravelled_overrides {
				// if same entity being overridden, merge props
				if let Some(found) = pass1.iter_mut().find(|x| x.entities == property_override.entities) {
					found.properties.extend(property_override.properties);
					found.runtime_editable.extend(property_override.runtime_editable);
				} else {
					pass1.push(PropertyOverride {
						entities: property_override.entities,
						properties: property_override.properties,
						runtime_editable: property_override.runtime_editable
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

					if x.runtime_editable != property_override.runtime_editable {
						return Ok(false);
					}

					let values_identical = x
						.properties
						.iter()
						.all(|(prop_name, prop_val)| prop_val.rough_eq(&property_override.properties[prop_name]));

					// Properties are identical when they contain the same properties and each property's value is roughly identical
					Ok(values_identical)
				})? {
					found.entities.extend(property_override.entities);
				} else {
					merged_overrides.push(property_override);
				}
			}

			entity.property_overrides = merged_overrides;
			modified = entity.property_overrides != previous_overrides;
		}

		PatchOperation::RemovePropertyOverrideConnection(connection) => {
			let previous_overrides = entity.property_overrides.to_owned();
			let mut unravelled_overrides: Vec<PropertyOverride> = vec![];

			for property_override in &entity.property_overrides {
				for ent in &property_override.entities {
					for (prop_name, prop_override) in &property_override.properties {
						unravelled_overrides.push(PropertyOverride {
							entities: vec![ent.to_owned()],
							properties: {
								let mut x = OrderMap::default();
								x.insert(prop_name.to_owned(), prop_override.to_owned());
								x
							},
							runtime_editable: if property_override.runtime_editable.contains(prop_name) {
								vec![prop_name.to_owned()]
							} else {
								vec![]
							}
						});
					}
				}
			}

			let search = PropertyOverride {
				entities: vec![connection.entity.to_owned()],
				properties: {
					let mut x = OrderMap::default();
					x.insert(connection.property.to_owned(), connection.value.to_owned());
					x
				},
				runtime_editable: if connection.runtime_editable {
					vec![connection.property.to_owned()]
				} else {
					vec![]
				}
			};

			unravelled_overrides.retain(|x| {
				x.entities != search.entities
					|| !x.properties.contains_key(&connection.property)
					|| !{ x.properties[&connection.property].rough_eq(&connection.value) }
					|| x.runtime_editable.contains(&connection.property) != connection.runtime_editable
			});

			let mut merged_overrides: Vec<PropertyOverride> = vec![];

			let mut pass1: Vec<PropertyOverride> = Vec::default();

			for property_override in unravelled_overrides {
				// if same entity being overridden, merge props
				if let Some(found) = pass1.iter_mut().find(|x| x.entities == property_override.entities) {
					found.properties.extend(property_override.properties);
					found.runtime_editable.extend(property_override.runtime_editable);
				} else {
					pass1.push(PropertyOverride {
						entities: property_override.entities,
						properties: property_override.properties,
						runtime_editable: property_override.runtime_editable
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

					if x.runtime_editable != property_override.runtime_editable {
						return Ok(false);
					}

					let values_identical = x
						.properties
						.iter()
						.all(|(prop_name, prop_val)| prop_val.rough_eq(&property_override.properties[prop_name]));

					// Properties are identical when they contain the same properties and each property's value is roughly identical
					Ok(values_identical)
				})? {
					found.entities.extend(property_override.entities);
				} else {
					merged_overrides.push(property_override);
				}
			}

			entity.property_overrides = merged_overrides;
			modified = entity.property_overrides != previous_overrides;
		}

		PatchOperation::AddOverrideDelete(value) => {
			entity.override_deletes.push(value);
			modified = true;
		}

		PatchOperation::RemoveOverrideDelete(value) => {
			entity.override_deletes.remove(
				entity
					.override_deletes
					.par_iter()
					.position_any(|x| *x == value)
					.context("RemoveOverrideDelete couldn't find expected value!")?
			);

			modified = true;
		}

		PatchOperation::AddPinConnectionOverride(value) => {
			entity.pin_connection_overrides.push(value);
			modified = true;
		}

		PatchOperation::RemovePinConnectionOverride(value) => {
			entity.pin_connection_overrides.remove(
				entity
					.pin_connection_overrides
					.par_iter()
					.position_any(|x| *x == value)
					.context("RemovePinConnectionOverride couldn't find expected value!")?
			);

			modified = true;
		}

		PatchOperation::AddPinConnectionOverrideDelete(value) => {
			entity.pin_connection_override_deletes.push(value);
			modified = true;
		}

		PatchOperation::RemovePinConnectionOverrideDelete(value) => {
			entity.pin_connection_override_deletes.remove(
				entity
					.pin_connection_override_deletes
					.par_iter()
					.position_any(|x| *x == value)
					.context("RemovePinConnectionOverrideDelete couldn't find expected value!")?
			);

			modified = true;
		}

		PatchOperation::AddExternalScene(value) => {
			entity.external_scenes.push(value);
			modified = true;
		}

		PatchOperation::RemoveExternalScene(value) => {
			if let Some(x) = entity.external_scenes.par_iter().position_any(|x| *x == value) {
				entity.external_scenes.remove(x);
				modified = true;
			} else {
				emit(AlreadyNonexistentDiagnostic::ExternalScene { scene: value }.into());
			}
		}

		PatchOperation::AddExtraFactoryReference(value) => {
			entity.extra_factory_references.push(value);
			modified = true;
		}

		PatchOperation::RemoveExtraFactoryReference(value) => {
			if let Some(x) = entity.extra_factory_references.par_iter().position_any(|x| *x == value) {
				entity.extra_factory_references.remove(x);
				modified = true;
			} else {
				emit(AlreadyNonexistentDiagnostic::ExtraFactoryReference { reference: value }.into());
			}
		}

		PatchOperation::AddExtraBlueprintReference(value) => {
			entity.extra_blueprint_references.push(value);
			modified = true;
		}

		PatchOperation::RemoveExtraBlueprintReference(value) => {
			if let Some(x) = entity
				.extra_blueprint_references
				.par_iter()
				.position_any(|x| *x == value)
			{
				entity.extra_blueprint_references.remove(x);
				modified = true;
			} else {
				emit(AlreadyNonexistentDiagnostic::ExtraBlueprintReference { reference: value }.into());
			}
		}

		PatchOperation::AddComment(value) => {
			entity.comments.push(value);
			modified = true;
		}

		PatchOperation::RemoveComment(value) => {
			entity.comments.remove(
				entity
					.comments
					.par_iter()
					.position_any(|x| *x == value)
					.context("RemoveComment couldn't find expected value!")?
			);

			modified = true;
		}
	}

	modified
}

#[try_fn]
#[context("Failure applying array patch")]
#[hotpath::measure]
pub fn apply_array_patch(
	arr: &mut Vec<Variant>,
	patch: Vec<ArrayPatchOperation>,
	identifier: EcoString,
	mut emit: impl FnMut(Diagnostic)
) -> Result<bool> {
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

	let mut modified = false;

	'op: for operation in patch {
		match operation {
			ArrayPatchOperation::Add { before, after, item } => {
				let has_selectors = before.is_empty() || after.is_empty();
				for pair in before.into_iter().zip_longest(after.into_iter().rev()) {
					let (before, after) = pair.left_and_right();

					if let Some(before) = before
						&& let Some(idx) = find_selector_index(arr, &before)
					{
						modified = true;
						arr.insert(idx, item);
						continue 'op;
					}

					if let Some(after) = after
						&& let Some(idx) = find_selector_index(arr, &after)
					{
						modified = true;
						arr.insert(idx + 1, item);
						continue 'op;
					}
				}

				modified = true;
				arr.push(item);

				if has_selectors {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementsBeforeAfter
					});
					continue;
				}
			}

			ArrayPatchOperation::Remove { item } => {
				if let Some(idx) = find_selector_index(arr, &item) {
					modified = true;
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
					modified = true;
					arr[idx] = new;
				} else {
					emit(Diagnostic::ArrayPatch {
						identifier: identifier.to_owned(),
						diagnostic: ArrayPatchDiagnostic::NoSuchElementToReplace { element: item }
					});

					modified = true;
					arr.push(new);
				}
			}
		}
	}

	modified
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

	for (i, row) in dp.iter_mut().enumerate() {
		row[0] = i;
	}

	for (j, el) in dp[0].iter_mut().enumerate() {
		*el = j;
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
					(index..working.len())
						.map(|i| selector_for_index(&working, i))
						.collect()
				} else {
					vec![]
				};

				let after = if index > 0 {
					(0..index).map(|i| selector_for_index(&working, i)).collect()
				} else {
					vec![]
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

	for entity_id in original.sub_entities.keys() {
		if !modified.sub_entities.contains_key(entity_id) {
			patch.push(PatchOperation::RemoveEntity(entity_id.to_owned()));
		}
	}

	for (entity_id, new_entity_data) in &modified.sub_entities {
		if let Some(old_entity_data) = original.sub_entities.get(entity_id) {
			if old_entity_data.parent != new_entity_data.parent {
				patch.push(PatchOperation::PatchEntity(
					entity_id.to_owned(),
					SubEntityOperation::SetParent(new_entity_data.parent.to_owned())
				));
			}

			if old_entity_data.name != new_entity_data.name {
				patch.push(PatchOperation::PatchEntity(
					entity_id.to_owned(),
					SubEntityOperation::SetName(new_entity_data.name.to_owned())
				));
			}

			if old_entity_data.factory != new_entity_data.factory {
				patch.push(PatchOperation::PatchEntity(
					entity_id.to_owned(),
					SubEntityOperation::SetFactory(new_entity_data.factory.to_owned())
				));
			}

			if old_entity_data.blueprint != new_entity_data.blueprint {
				patch.push(PatchOperation::PatchEntity(
					entity_id.to_owned(),
					SubEntityOperation::SetBlueprint(new_entity_data.blueprint.to_owned())
				));
			}

			if old_entity_data.editor_only != new_entity_data.editor_only {
				patch.push(PatchOperation::PatchEntity(
					entity_id.to_owned(),
					SubEntityOperation::SetEditorOnly(new_entity_data.editor_only.to_owned())
				));
			}

			for excluded_platform in &old_entity_data.excluded_platforms {
				if !new_entity_data.excluded_platforms.contains(excluded_platform) {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExcludedPlatform(excluded_platform.to_owned())
					));
				}
			}

			for excluded_platform in &new_entity_data.excluded_platforms {
				if !old_entity_data.excluded_platforms.contains(excluded_platform) {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::AddExcludedPlatform(excluded_platform.to_owned())
					));
				}
			}

			for property_name in old_entity_data.properties.keys() {
				if !new_entity_data.properties.contains_key(property_name) {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::RemoveProperty(property_name.to_owned())
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

							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::PatchPropertyValue(
									property_name.to_owned(),
									VariantPatch::ArrayPatch(ops),
									new_property_data.post_init
								)
							));
						} else {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::PatchPropertyValue(
									property_name.to_owned(),
									VariantPatch::Set(new_property_data.value.to_owned()),
									new_property_data.post_init
								)
							));
						}
					}

					if old_property_data.post_init != new_property_data.post_init {
						patch.push(PatchOperation::PatchEntity(
							entity_id.to_owned(),
							SubEntityOperation::SetPropertyPostInit(
								property_name.to_owned(),
								new_property_data.post_init
							)
						));
					}
				} else {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::AddProperty(property_name.to_owned(), new_property_data.to_owned())
					));
				}
			}

			// Duplicated from above except with an extra layer for platform
			for (platform_name, properties) in &old_entity_data.platform_specific_properties {
				if !new_entity_data.platform_specific_properties.contains_key(platform_name) {
					patch.extend(properties.keys().map(|property_name| {
						PatchOperation::PatchEntity(
							entity_id.to_owned(),
							SubEntityOperation::RemovePlatformSpecificProperty(
								platform_name.to_owned(),
								property_name.to_owned()
							)
						)
					}));
				}
			}

			for (platform_name, new_properties_data) in &new_entity_data.platform_specific_properties {
				if let Some(old_properties_data) = old_entity_data.platform_specific_properties.get(platform_name) {
					for property_name in old_properties_data.keys() {
						if !new_entity_data.properties.contains_key(property_name) {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemovePlatformSpecificProperty(
									platform_name.to_owned(),
									property_name.to_owned()
								)
							));
						}
					}

					for (property_name, new_property_data) in new_properties_data {
						if let Some(old_property_data) = old_properties_data.get(property_name) {
							if !old_property_data.value.rough_eq(&new_property_data.value) {
								if let Variant::Array(_, old_value) = &old_property_data.value
									&& let Variant::Array(_, new_value) = &new_property_data.value
								{
									let ops = generate_array_patch(old_value, new_value);

									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::PatchPlatformSpecificPropertyValue(
											platform_name.to_owned(),
											property_name.to_owned(),
											VariantPatch::ArrayPatch(ops),
											new_property_data.post_init
										)
									));
								} else {
									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::PatchPlatformSpecificPropertyValue(
											platform_name.to_owned(),
											property_name.to_owned(),
											VariantPatch::Set(new_property_data.value.to_owned()),
											new_property_data.post_init
										)
									));
								}
							}

							if old_property_data.post_init != new_property_data.post_init {
								patch.push(PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyPostInit(
										platform_name.to_owned(),
										property_name.to_owned(),
										new_property_data.post_init
									)
								));
							}
						} else {
							patch.push(PatchOperation::PatchEntity(
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
						patch.push(PatchOperation::PatchEntity(
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
			for (event_name, triggers) in &old_entity_data.events {
				if !new_entity_data.events.contains_key(event_name) {
					patch.extend(triggers.iter().flat_map(|(trigger_name, connections)| {
						connections.iter().map(move |connection| {
							PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemoveEventConnection(
									event_name.to_owned(),
									trigger_name.to_owned(),
									connection.to_owned()
								)
							)
						})
					}));
				}
			}

			for (event_name, new_events_data) in &new_entity_data.events {
				if let Some(old_events_data) = old_entity_data.events.get(event_name) {
					for (trigger_name, connections) in old_events_data {
						if !new_events_data.contains_key(trigger_name) {
							patch.extend(connections.iter().map(|connection| {
								PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::RemoveEventConnection(
										event_name.to_owned(),
										trigger_name.to_owned(),
										connection.to_owned()
									)
								)
							}));
						}
					}

					for (trigger_name, new_refs_data) in new_events_data {
						if let Some(old_refs_data) = old_events_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::PatchEntity(
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
									patch.push(PatchOperation::PatchEntity(
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
								patch.push(PatchOperation::PatchEntity(
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
							patch.push(PatchOperation::PatchEntity(
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

			for (event_name, triggers) in &old_entity_data.input_forwardings {
				if !new_entity_data.input_forwardings.contains_key(event_name) {
					patch.extend(triggers.iter().flat_map(|(trigger_name, connections)| {
						connections.iter().map(move |connection| {
							PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemoveInputForwarding(
									event_name.to_owned(),
									trigger_name.to_owned(),
									connection.to_owned()
								)
							)
						})
					}));
				}
			}

			for (event_name, new_input_forwarding_data) in &new_entity_data.input_forwardings {
				if let Some(old_input_forwarding_data) = old_entity_data.input_forwardings.get(event_name) {
					for (trigger_name, connections) in old_input_forwarding_data {
						if !new_input_forwarding_data.contains_key(trigger_name) {
							patch.extend(connections.iter().map(|connection| {
								PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::RemoveInputForwarding(
										event_name.to_owned(),
										trigger_name.to_owned(),
										connection.to_owned()
									)
								)
							}));
						}
					}

					for (trigger_name, new_refs_data) in new_input_forwarding_data {
						if let Some(old_refs_data) = old_input_forwarding_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::RemoveInputForwarding(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}

							for i in new_refs_data {
								if !old_refs_data.contains(i) {
									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::AddInputForwarding(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}
						} else {
							for i in new_refs_data {
								patch.push(PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::AddInputForwarding(
										event_name.to_owned(),
										trigger_name.to_owned(),
										i.to_owned()
									)
								))
							}
						}
					}
				} else {
					for (trigger_name, new_refs_data) in new_input_forwarding_data {
						for i in new_refs_data {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::AddInputForwarding(
									event_name.to_owned(),
									trigger_name.to_owned(),
									i.to_owned()
								)
							))
						}
					}
				}
			}

			for (event_name, triggers) in &old_entity_data.output_forwardings {
				if !new_entity_data.output_forwardings.contains_key(event_name) {
					patch.extend(triggers.iter().flat_map(|(trigger_name, connections)| {
						connections.iter().map(move |connection| {
							PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemoveOutputForwarding(
									event_name.to_owned(),
									trigger_name.to_owned(),
									connection.to_owned()
								)
							)
						})
					}));
				}
			}

			for (event_name, new_output_forwarding_data) in &new_entity_data.output_forwardings {
				if let Some(old_output_forwarding_data) = old_entity_data.output_forwardings.get(event_name) {
					for (trigger_name, connections) in old_output_forwarding_data {
						if !new_output_forwarding_data.contains_key(trigger_name) {
							patch.extend(connections.iter().map(|connection| {
								PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::RemoveOutputForwarding(
										event_name.to_owned(),
										trigger_name.to_owned(),
										connection.to_owned()
									)
								)
							}));
						}
					}

					for (trigger_name, new_refs_data) in new_output_forwarding_data {
						if let Some(old_refs_data) = old_output_forwarding_data.get(trigger_name) {
							for i in old_refs_data {
								if !new_refs_data.contains(i) {
									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::RemoveOutputForwarding(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}

							for i in new_refs_data {
								if !old_refs_data.contains(i) {
									patch.push(PatchOperation::PatchEntity(
										entity_id.to_owned(),
										SubEntityOperation::AddOutputForwarding(
											event_name.to_owned(),
											trigger_name.to_owned(),
											i.to_owned()
										)
									))
								}
							}
						} else {
							for i in new_refs_data {
								patch.push(PatchOperation::PatchEntity(
									entity_id.to_owned(),
									SubEntityOperation::AddOutputForwarding(
										event_name.to_owned(),
										trigger_name.to_owned(),
										i.to_owned()
									)
								))
							}
						}
					}
				} else {
					for (trigger_name, new_refs_data) in new_output_forwarding_data {
						for i in new_refs_data {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::AddOutputForwarding(
									event_name.to_owned(),
									trigger_name.to_owned(),
									i.to_owned()
								)
							))
						}
					}
				}
			}

			for (alias_name, connections) in &old_entity_data.property_aliases {
				if !new_entity_data.property_aliases.contains_key(alias_name) {
					patch.extend(connections.iter().map(|connection| {
						PatchOperation::PatchEntity(
							entity_id.to_owned(),
							SubEntityOperation::RemovePropertyAlias(alias_name.to_owned(), connection.to_owned())
						)
					}));
				}
			}

			for (alias_name, new_alias_connections) in &new_entity_data.property_aliases {
				if let Some(old_alias_connections) = old_entity_data.property_aliases.get(alias_name) {
					for connection in new_alias_connections {
						if !old_alias_connections.contains(connection) {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::AddPropertyAlias(alias_name.to_owned(), connection.to_owned())
							));
						}
					}

					for connection in old_alias_connections {
						if !new_alias_connections.contains(connection) {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemovePropertyAlias(alias_name.to_owned(), connection.to_owned())
							));
						}
					}
				} else {
					for connection in new_alias_connections {
						patch.push(PatchOperation::PatchEntity(
							entity_id.to_owned(),
							SubEntityOperation::AddPropertyAlias(alias_name.to_owned(), connection.to_owned())
						));
					}
				}
			}

			for exposed_entity in old_entity_data.exposed_entities.keys() {
				if !new_entity_data.exposed_entities.contains_key(exposed_entity) {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedEntity(exposed_entity.to_owned())
					));
				}
			}

			for (exposed_entity, data) in &new_entity_data.exposed_entities {
				if !old_entity_data.exposed_entities.contains_key(exposed_entity)
					|| old_entity_data.exposed_entities.get(exposed_entity).ctx? != data
				{
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedEntity(exposed_entity.to_owned(), data.to_owned())
					));
				}
			}

			for exposed_interface in old_entity_data.exposed_interfaces.keys() {
				if !new_entity_data.exposed_interfaces.contains_key(exposed_interface) {
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedInterface(exposed_interface.to_owned())
					));
				}
			}

			for (exposed_interface, data) in &new_entity_data.exposed_interfaces {
				if !old_entity_data.exposed_interfaces.contains_key(exposed_interface)
					|| old_entity_data.exposed_interfaces.get(exposed_interface).ctx? != data
				{
					patch.push(PatchOperation::PatchEntity(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedInterface(exposed_interface.to_owned(), data.to_owned())
					));
				}
			}

			for (subset_name, subsets) in &old_entity_data.subsets {
				if !new_entity_data.subsets.contains_key(subset_name) {
					patch.extend(subsets.iter().map(|i| {
						PatchOperation::PatchEntity(
							entity_id.to_owned(),
							SubEntityOperation::AddSubset(subset_name.to_owned(), i.to_owned())
						)
					}));
				}
			}

			for (subset_name, new_refs_data) in &new_entity_data.subsets {
				if let Some(old_refs_data) = old_entity_data.subsets.get(subset_name) {
					for i in old_refs_data {
						if !new_refs_data.contains(i) {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::RemoveSubset(subset_name.to_owned(), i.to_owned())
							));
						}
					}

					for i in new_refs_data {
						if !old_refs_data.contains(i) {
							patch.push(PatchOperation::PatchEntity(
								entity_id.to_owned(),
								SubEntityOperation::AddSubset(subset_name.to_owned(), i.to_owned())
							));
						}
					}
				} else {
					for i in new_refs_data {
						patch.push(PatchOperation::PatchEntity(
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
							value: prop_val.to_owned(),
							runtime_editable: property_override.runtime_editable.contains(prop_name)
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
							value: prop_val.to_owned(),
							runtime_editable: property_override.runtime_editable.contains(prop_name)
						})
						.collect_vec()
				})
				.collect_vec()
		})
		.collect();

	for x in &original_unravelled_overrides {
		if !modified_unravelled_overrides.iter().any(|val| {
			val.entity == x.entity
				&& val.property == x.property
				&& val.value.rough_eq(&x.value)
				&& val.runtime_editable == x.runtime_editable
		}) {
			patch.push(PatchOperation::RemovePropertyOverrideConnection(x.to_owned()))
		}
	}

	for x in &modified_unravelled_overrides {
		if !original_unravelled_overrides.iter().any(|val| {
			val.entity == x.entity
				&& val.property == x.property
				&& val.value.rough_eq(&x.value)
				&& val.runtime_editable == x.runtime_editable
		}) {
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
		if entity.sub_type != SubType::Template {
			entity
				.external_scenes
				.par_iter()
				.map(|scene| ResourceReference {
					resource: scene.to_owned(),
					flags: Default::default()
				})
				.collect()
		} else {
			vec![]
		},
		// then factories of sub-entities
		entity
			.sub_entities
			.par_iter()
			.map(|(_, sub_entity)| sub_entity.factory.to_owned())
			.collect(),
		// then sub-entity resources
		entity
			.sub_entities
			.par_iter()
			.map(|(_, sub_entity)| -> Result<_> {
				Ok(vec![
					sub_entity
						.properties
						.iter()
						.filter_map(|(_, prop)| {
							if let Variant::Resource(_, res) = &prop.value {
								res.to_owned()
							} else if let Variant::EnumValue(val) = &prop.value {
								val.resource.to_owned()
							} else {
								None
							}
						})
						.collect_vec(),
					sub_entity
						.properties
						.iter()
						.flat_map(|(_, prop)| match &prop.value {
							Variant::Array(ty, items)
								if ty == "ZResourceID" || ty == "ZRuntimeResourceID" || ty == "ZEditorEnumValue" =>
							{
								items
									.iter()
									.filter_map(|item| {
										if let Variant::Resource(_, res) = item {
											res.to_owned()
										} else if let Variant::EnumValue(val) = item {
											val.resource.to_owned()
										} else {
											None
										}
									})
									.collect_vec()
							}

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
										if let Variant::Resource(_, res) = &prop.value {
											res.to_owned()
										} else if let Variant::EnumValue(val) = &prop.value {
											val.resource.to_owned()
										} else {
											None
										}
									})
									.collect_vec(),
								props
									.iter()
									.flat_map(|(_, prop)| match &prop.value {
										Variant::Array(ty, items)
											if ty == "ZResourceID"
												|| ty == "ZRuntimeResourceID" || ty == "ZEditorEnumValue" =>
										{
											items
												.iter()
												.filter_map(|item| {
													if let Variant::Resource(_, res) = item {
														res.to_owned()
													} else if let Variant::EnumValue(val) = item {
														val.resource.to_owned()
													} else {
														None
													}
												})
												.collect_vec()
										}

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
		// then property override resources
		entity
			.property_overrides
			.par_iter()
			.map(|PropertyOverride { properties, .. }| -> Result<_> {
				Ok([
					properties
						.iter()
						.filter_map(|(_, prop)| {
							if let Variant::Resource(_, res) = prop {
								res.to_owned()
							} else if let Variant::EnumValue(val) = prop {
								val.resource.to_owned()
							} else {
								None
							}
						})
						.collect_vec(),
					properties
						.iter()
						.flat_map(|(_, prop)| match prop {
							Variant::Array(ty, items)
								if ty == "ZResourceID" || ty == "ZRuntimeResourceID" || ty == "ZEditorEnumValue" =>
							{
								items
									.iter()
									.filter_map(|item| {
										if let Variant::Resource(_, res) = item {
											res.to_owned()
										} else if let Variant::EnumValue(val) = item {
											val.resource.to_owned()
										} else {
											None
										}
									})
									.collect_vec()
							}

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
		if entity.sub_type != SubType::Template {
			entity
				.external_scenes
				.par_iter()
				.map(|scene| ResourceReference {
					resource: scene.to_owned(),
					flags: Default::default()
				})
				.collect::<Vec<_>>()
		} else {
			vec![]
		},
		entity
			.sub_entities
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

macro_rules! impl_psproperties {
	(
		fl,
		$sub_entity_factory:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {{
		let mut properties: OrderMap<EcoString, OrderMap<EcoString, Property>> = Default::default();

		for item in $sub_entity_factory
			.platform_specific_property_values
			.iter()
			.map(|property| {
				anyhow::Ok((
					property.platform,
					property
						.property_value
						.property_id
						.as_name()
						.map(|x| x.to_owned())
						.unwrap_or_else(|| property.property_value.property_id.0.to_string().into()),
					Property {
						value: property.property_value.value.to_qn(
							$factory,
							$factory_meta,
							$blueprint,
							$blueprint_meta,
							$convert_lossless
						)?,
						post_init: property.post_init
					}
				))
			}) {
			let (platform, property_name, property) = item?;
			properties
				.entry(<&str>::from(platform).into())
				.or_default()
				.insert(property_name, property);
		}

		properties
	}};

	(
		h3,
		$sub_entity_factory:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		impl_psproperties!(
			fl,
			$sub_entity_factory,
			$factory,
			$factory_meta,
			$blueprint,
			$blueprint_meta,
			$convert_lossless
		)
	};

	(
		$_:ident,
		$sub_entity_factory:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		Default::default()
	};
}

macro_rules! impl_editor_only {
	(fl, $sub_entity_blueprint:expr) => {
		$sub_entity_blueprint.editor_only
	};

	(h3, $sub_entity_blueprint:expr) => {
		impl_editor_only!(fl, $sub_entity_blueprint)
	};

	(h2, $sub_entity_blueprint:expr) => {
		impl_editor_only!(fl, $sub_entity_blueprint)
	};

	($_:ident, $sub_entity_blueprint:expr) => {
		false
	};
}

macro_rules! impl_exposed_entity {
	(
		fl,
		$exposed_entity:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		Ok((
			$exposed_entity.name.to_owned(),
			ExposedEntity {
				is_array: $exposed_entity.is_array.to_owned(),
				refers_to: $exposed_entity
					.targets
					.iter()
					.map(|target| {
						target
							.to_qn($factory, $factory_meta, $blueprint, $blueprint_meta, $convert_lossless)?
							.context("Exposed entity references must not be null")
					})
					.collect::<Result<_>>()?
			}
		))
	};

	(
		h3,
		$exposed_entity:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		impl_exposed_entity!(
			fl,
			$exposed_entity,
			$factory,
			$factory_meta,
			$blueprint,
			$blueprint_meta,
			$convert_lossless
		)
	};

	(
		h2,
		$exposed_entity:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		impl_exposed_entity!(
			fl,
			$exposed_entity,
			$factory,
			$factory_meta,
			$blueprint,
			$blueprint_meta,
			$convert_lossless
		)
	};

	(
		h1,
		$exposed_entity:expr,
		$factory:expr,
		$factory_meta:expr,
		$blueprint:expr,
		$blueprint_meta:expr,
		$convert_lossless:expr
	) => {
		Ok((
			$exposed_entity.0.to_owned(),
			ExposedEntity {
				is_array: false,
				refers_to: vec![
					$exposed_entity
						.1
						.to_qn($factory, $factory_meta, $blueprint, $blueprint_meta, $convert_lossless)?
						.context("Exposed entity references must not be null")?,
				]
			}
		))
	};
}

macro_rules! impl_h1_others {
	(fl, $h1:expr, $others:expr) => {
		$others
	};

	(h3, $h1:expr, $others:expr) => {
		$others
	};

	(h2, $h1:expr, $others:expr) => {
		$others
	};

	(h1, $h1:expr, $others:expr) => {
		$h1
	};
}

macro_rules! impl_fl_others {
	(fl, $fl:expr, $others:expr) => {
		$fl
	};

	(h3, $fl:expr, $others:expr) => {
		$others
	};

	(h2, $fl:expr, $others:expr) => {
		$others
	};

	(h1, $fl:expr, $others:expr) => {
		$others
	};
}

macro_rules! impl_h3_fl_others {
	(fl, $h3_fl:expr, $others:expr) => {
		$h3_fl
	};

	(h3, $h3_fl:expr, $others:expr) => {
		$h3_fl
	};

	(h2, $h3_fl:expr, $others:expr) => {
		$others
	};

	(h1, $h3_fl:expr, $others:expr) => {
		$others
	};
}

macro_rules! impl_game {
	($game:ident, $factory:ident, $sub_entities:ident) => {
		impl ToQuickEntity for glacier_bin1::game::$game::$factory {
			type QuickEntity = Entity;
			type Error = anyhow::Error;

			type Factory = glacier_bin1::game::$game::$factory;
			type Blueprint = glacier_bin1::game::$game::STemplateEntityBlueprint;

			#[try_fn]
			#[context("Failure converting game entity to QN")]
			#[auto_context]
			#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
			#[hotpath::measure]
			fn to_qn(
				&self,
				factory: &Self::Factory,
				factory_meta: &ResourceMetadata,
				blueprint: &Self::Blueprint,
				blueprint_meta: &ResourceMetadata,
				convert_lossless: bool
			) -> Result<Self::QuickEntity> {
				if factory.$sub_entities.len() != blueprint.$sub_entities.len() {
					bail!("Factory and blueprint have different sub-entity counts");
				}

				let mut entity = Entity {
					factory: factory_meta.id,
					blueprint: blueprint_meta.id,
					root_entity: blueprint
						.$sub_entities
						.get(blueprint.root_entity_index as usize)
						.context("Root entity index referred to nonexistent entity")?
						.entity_id
						.into(),
					sub_entities: factory
						.$sub_entities
						.par_iter()
						.zip(&blueprint.$sub_entities)
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
											.resource,
										parent: sub_entity_factory.logical_parent.to_qn(
											factory,
											factory_meta,
											blueprint,
											blueprint_meta,
											convert_lossless
										)?,
										editor_only: impl_editor_only!($game, sub_entity_blueprint),
										excluded_platforms: impl_fl_others!(
											$game,
											sub_entity_blueprint
												.excluded_platforms
												.iter()
												.map(|&x| <&str>::from(x).into())
												.collect(),
											Default::default()
										),
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
														value: property.value.to_qn(
															factory,
															factory_meta,
															blueprint,
															blueprint_meta,
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
															.unwrap_or_else(|| {
																property.property_id.0.to_string().into()
															}),
														Property {
															value: property.value.to_qn(
																factory,
																factory_meta,
																blueprint,
																blueprint_meta,
																convert_lossless
															)?,
															post_init: true
														}
													))
												}
											))
											.collect::<Result<_>>()?,
										platform_specific_properties: impl_psproperties!(
											$game,
											sub_entity_factory,
											factory,
											factory_meta,
											blueprint,
											blueprint_meta,
											convert_lossless
										),
										events: Default::default(),             // will be mutated later
										input_forwardings: Default::default(),  // will be mutated later
										output_forwardings: Default::default(), // will be mutated later
										property_aliases: {
											let mut aliases: OrderMap<EcoString, Vec<PropertyAlias>> =
												Default::default();

											for item in sub_entity_blueprint.property_aliases.iter().map(|alias| {
												anyhow::Ok({
													(
														alias.property_name.to_owned(),
														PropertyAlias {
															original_property: alias.alias_name.to_owned(),
															original_entity: blueprint
																.$sub_entities
																.get(alias.entity_id as usize)
																.context(
																	"Property alias referred to nonexistent sub-entity"
																)?
																.entity_id
																.into()
														}
													)
												})
											}) {
												let (property_name, alias) = item?;
												aliases.entry(property_name).or_default().push(alias);
											}

											aliases
										},
										exposed_entities: sub_entity_blueprint
											.exposed_entities
											.iter()
											.map(|exposed_entity| -> Result<_> {
												impl_exposed_entity!(
													$game,
													exposed_entity,
													factory,
													factory_meta,
													blueprint,
													blueprint_meta,
													convert_lossless
												)
											})
											.collect::<Result<_>>()?,
										exposed_interfaces: sub_entity_blueprint
											.exposed_interfaces
											.iter()
											.map(|(interface, entity_index)| {
												Ok((
													interface.to_owned(),
													blueprint
														.$sub_entities
														.get(*entity_index as usize)
														.context(
															"Exposed interface referred to nonexistent sub-entity"
														)?
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
						.collect::<Result<_>>()?,
					external_scenes: impl_fl_others!(
						$game,
						factory
							.external_scene_runtime_resource_ids
							.iter()
							.map(|scene| scene.as_u64().try_into().context("Invalid external scene ID"))
							.collect::<Result<_>>()?,
						factory
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
							.collect::<Result<_>>()?
					),
					override_deletes: blueprint
						.override_deletes
						.par_iter()
						.map(|x| {
							x.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
								.context("Override delete references must not be null")
						})
						.collect::<Result<_>>()?,
					pin_connection_override_deletes: impl_h1_others!($game, Default::default(), {
						blueprint
							.pin_connection_override_deletes
							.par_iter()
							.map(|x| {
								Ok(PinConnectionOverrideDelete {
									from_entity: x
										.from_entity
										.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
										.context("Pin connection override delete references must not be null")?,
									to_entity: x
										.to_entity
										.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
										.context("Pin connection override delete references must not be null")?,
									from_pin: x.from_pin_name.to_owned(),
									to_pin: x.to_pin_name.to_owned(),
									value: if x.constant_pin_value.is::<()>() {
										None
									} else {
										Some(x.constant_pin_value.to_qn(
											factory,
											factory_meta,
											blueprint,
											blueprint_meta,
											convert_lossless
										)?)
									}
								})
							})
							.collect::<Result<_>>()?
					}),
					pin_connection_overrides: impl_h1_others!(
						$game,
						Default::default(),
						blueprint
							.pin_connection_overrides
							.par_iter()
							.filter(|x| x.from_entity.external_scene_index != -1)
							.map(|x| {
								Ok(PinConnectionOverride {
									from_entity: x
										.from_entity
										.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
										.context("Pin connection override references must not be null")?,
									to_entity: x
										.to_entity
										.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
										.context("Pin connection override references must not be null")?,
									from_pin: x.from_pin_name.to_owned(),
									to_pin: x.to_pin_name.to_owned(),
									value: if x.constant_pin_value.is::<()>() {
										None
									} else {
										Some(x.constant_pin_value.to_qn(
											factory,
											factory_meta,
											blueprint,
											blueprint_meta,
											convert_lossless
										)?)
									}
								})
							})
							.collect::<Result<_>>()?
					),
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

				let (a, b) = rayon::join(
					|| {
						let depends = get_factory_references(&entity)?
							.into_iter()
							.collect::<HashSet<_>>();

						anyhow::Ok(
							factory_meta
								.references
								.iter()
								.filter(|x| !depends.contains(x))
								.cloned()
								.collect()
						)
					},
					|| {
						let depends = get_blueprint_references(&entity)
							.into_iter()
							.collect::<HashSet<_>>();

						anyhow::Ok(
							blueprint_meta
								.references
								.iter()
								.filter(|x| !depends.contains(x))
								.cloned()
								.collect()
						)
					}
				);

				entity.extra_factory_references = a?;
				entity.extra_blueprint_references = b?;

				for pin in &blueprint.pin_connections {
					let relevant_sub_entity = entity
						.sub_entities
						.get_mut(&EntityID::from(
							blueprint
								.$sub_entities
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
									.$sub_entities
									.get(pin.to_id as usize)
									.context("Pin referred to nonexistent sub-entity")?
									.entity_id
									.into()
							),
							value: impl_h1_others!(
								$game,
								None,
								if pin.constant_pin_value.is::<()>() {
									None
								} else {
									Some(pin.constant_pin_value.to_qn(
										factory,
										factory_meta,
										blueprint,
										blueprint_meta,
										convert_lossless
									)?)
								}
							)
						});
				}

				impl_h1_others!($game, {}, {
					for pin_connection_override in blueprint
						.pin_connection_overrides
						.iter()
						.filter(|x| x.from_entity.external_scene_index == -1)
					{
						let relevant_sub_entity = entity
							.sub_entities
							.get_mut(&EntityID::from(
								blueprint
									.$sub_entities
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
								entity_ref: pin_connection_override
									.to_entity
									.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
									.context("Pin connection references must not be null")?,
								value: if pin_connection_override.constant_pin_value.is::<()>() {
									None
								} else {
									Some(pin_connection_override.constant_pin_value.to_qn(
										factory,
										factory_meta,
										blueprint,
										blueprint_meta,
										convert_lossless
									)?)
								}
							});
					}
				});

				for forwarding in &blueprint.input_pin_forwardings {
					let relevant_sub_entity = entity
						.sub_entities
						.get_mut(&EntityID::from(
							blueprint
								.$sub_entities
								.get(forwarding.from_id as usize)
								.context("Pin referred to nonexistent sub-entity")?
								.entity_id
						))
						.ctx?;

					relevant_sub_entity
						.input_forwardings
						.entry(forwarding.from_pin_name.to_owned())
						.or_default()
						.entry(forwarding.to_pin_name.to_owned())
						.or_default()
						.push(LocalPinConnection {
							entity_id: blueprint
								.$sub_entities
								.get(forwarding.to_id as usize)
								.context("Pin referred to nonexistent sub-entity")?
								.entity_id
								.into(),
							value: impl_h1_others!(
								$game,
								None,
								if forwarding.constant_pin_value.is::<()>() {
									None
								} else {
									Some(forwarding.constant_pin_value.to_qn(
										factory,
										factory_meta,
										blueprint,
										blueprint_meta,
										convert_lossless
									)?)
								}
							)
						});
				}

				for forwarding in &blueprint.output_pin_forwardings {
					let relevant_sub_entity = entity
						.sub_entities
						.get_mut(&EntityID::from(
							blueprint
								.$sub_entities
								.get(forwarding.from_id as usize)
								.context("Pin referred to nonexistent sub-entity")?
								.entity_id
						))
						.ctx?;

					relevant_sub_entity
						.output_forwardings
						.entry(forwarding.from_pin_name.to_owned())
						.or_default()
						.entry(forwarding.to_pin_name.to_owned())
						.or_default()
						.push(LocalPinConnection {
							entity_id: blueprint
								.$sub_entities
								.get(forwarding.to_id as usize)
								.context("Pin referred to nonexistent sub-entity")?
								.entity_id
								.into(),
							value: impl_h1_others!(
								$game,
								None,
								if forwarding.constant_pin_value.is::<()>() {
									None
								} else {
									Some(forwarding.constant_pin_value.to_qn(
										factory,
										factory_meta,
										blueprint,
										blueprint_meta,
										convert_lossless
									)?)
								}
							)
						});
				}

				impl_fl_others!($game, {}, {
					for sub_entity in &blueprint.$sub_entities {
						for (subset, data) in &sub_entity.entity_subsets {
							for subset_entity in &data.entities {
								let relevant_qn = entity
									.sub_entities
									.get_mut(&EntityID::from(
										blueprint
											.$sub_entities
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
				});

				let mut pass1: Vec<PropertyOverride> = Vec::default();

				for property_override in &factory.property_overrides {
					let ents = vec![
						property_override
							.property_owner
							.to_qn(factory, factory_meta, blueprint, blueprint_meta, convert_lossless)?
							.context("Property override references must not be null")?,
					];

					let prop_name = property_override
						.property_value
						.property_id
						.as_name()
						.map(|x| x.to_owned())
						.unwrap_or_else(|| property_override.property_value.property_id.0.to_string().into());

					let props = [(prop_name.to_owned(), {
						property_override.property_value.value.to_qn(
							factory,
							factory_meta,
							blueprint,
							blueprint_meta,
							convert_lossless
						)?
					})]
					.into_iter()
					.collect();

					// if same entity being overridden, merge props
					if let Some(found) = pass1.iter_mut().find(|x| x.entities == ents) {
						found.properties.extend(props);
						impl_fl_others!(
							$game,
							if property_override.is_rt_editable {
								found.runtime_editable.push(prop_name)
							},
							{}
						)
					} else {
						pass1.push(PropertyOverride {
							entities: ents,
							properties: props,
							runtime_editable: impl_fl_others!(
								$game,
								if property_override.is_rt_editable {
									vec![prop_name]
								} else {
									vec![]
								},
								vec![]
							)
						});
					}
				}

				// merge entities when same props being overridden
				for property_override in pass1 {
					if let Some(found) = entity.property_overrides.iter_mut().find(|x| {
						x.properties == property_override.properties
							&& x.runtime_editable == property_override.runtime_editable
					}) {
						found.entities.extend(property_override.entities);
					} else {
						entity.property_overrides.push(property_override);
					}
				}

				entity
			}
		}

		impl FromQuickEntity<Entity>
			for (
				glacier_bin1::game::$game::$factory,
				ResourceMetadata,
				glacier_bin1::game::$game::STemplateEntityBlueprint,
				ResourceMetadata
			)
		{
			type Error = anyhow::Error;

			#[try_fn]
			#[context("Failure converting QN entity to game")]
			#[auto_context]
			#[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
			#[hotpath::measure]
			fn from_qn(
				entity: &Entity,
				_: &HashMap<EntityID, usize>,
				_: &HashMap<RuntimeID, usize>,
				_: &HashMap<RuntimeID, usize>
			) -> Result<Self> {
				use glacier_bin1::game::$game::*;

				if entity.quickentity_version != ENTITY_VERSION {
					bail!(
						"Invalid QuickEntity version; expected {}, got {}",
						ENTITY_VERSION,
						entity.quickentity_version
					);
				}

				let entity_indices: HashMap<EntityID, usize> = entity
					.sub_entities
					.keys()
					.enumerate()
					.map(|(x, y)| (*y, x))
					.collect();

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

				let reference_indices: HashMap<RuntimeID, usize> = factory_meta
					.references
					.par_iter()
					.enumerate()
					.map(|(x, y)| (y.resource.to_owned(), x.to_owned()))
					.collect();

				let mut factory = impl_fl_others!(
					$game,
					$factory {
						sub_type: match entity.sub_type {
							SubType::Brick => 2,
							SubType::Scene => 1,
							SubType::Template => 0
						},
						blueprint_index_in_resource_header: 0,
						root_entity_index: *entity_indices
							.get(&entity.root_entity)
							.context("Root entity was non-existent")? as i32,
						$sub_entities: Vec::with_capacity(entity.sub_entities.len()),
						property_overrides: vec![],
						external_scene_type_indices_in_resource_header: factory_meta
							.references
							.iter()
							.enumerate()
							.filter_map(|(idx, reference)| entity
								.external_scenes
								.contains(&reference.resource)
								.then_some(idx as i32))
							.collect(),
						external_scene_runtime_resource_ids: entity
							.external_scenes
							.iter()
							.map(|scene| ZRuntimeResourceID::from_u64(
								scene.as_u64() | ((GamePlatform::PC.tag().unwrap() as u64) << 56)
							))
							.collect(),
						source_resource_id: entity.factory.to_eco_string()
					},
					$factory {
						sub_type: match entity.sub_type {
							SubType::Brick => 2,
							SubType::Scene => 1,
							SubType::Template => 0
						},
						blueprint_index_in_resource_header: 0,
						root_entity_index: *entity_indices
							.get(&entity.root_entity)
							.context("Root entity was non-existent")? as i32,
						$sub_entities: Vec::with_capacity(entity.sub_entities.len()),
						property_overrides: vec![],
						external_scene_type_indices_in_resource_header: (1..entity.external_scenes.len() as i32 + 1)
							.collect()
					}
				);

				let external_scene_indices: HashMap<RuntimeID, usize> = impl_fl_others!(
					$game,
					entity
						.external_scenes
						.iter()
						.copied()
						.enumerate()
						.map(|(x, y)| (y, x))
						.collect(),
					factory
						.external_scene_type_indices_in_resource_header
						.iter()
						.enumerate()
						.map(|(idx, scene_index)| {
							anyhow::Ok((
								factory_meta
									.references
									.get(*scene_index as usize)
									.context("External scene index referred to nonexistent dependency")?
									.resource,
								idx
							))
						})
						.collect::<Result<_>>()?
				);

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

				let mut blueprint = impl_h1_others!(
					$game,
					STemplateEntityBlueprint {
						sub_type: match entity.sub_type {
							SubType::Brick => 2,
							SubType::Scene => 1,
							SubType::Template => 0
						},
						root_entity_index: *entity_indices
							.get(&entity.root_entity)
							.context("Root entity was non-existent")? as i32,
						$sub_entities: vec![],
						pin_connections: vec![],
						input_pin_forwardings: vec![],
						output_pin_forwardings: vec![],
						override_deletes: entity
							.override_deletes
							.par_iter()
							.map(|override_delete| {
								override_delete.to_game(&entity_indices, &reference_indices, &external_scene_indices)
							})
							.collect::<Result<_>>()?,
						external_scene_type_indices_in_resource_header: (0..entity.external_scenes.len() as i32)
							.collect()
					},
					{
						let pin_connection_overrides = [
							entity
								.pin_connection_overrides
								.par_iter()
								.map(|pin_connection_override| {
									Ok(SExternalEntityTemplatePinConnection {
										from_entity: pin_connection_override.from_entity.to_game(
											&entity_indices,
											&reference_indices,
											&external_scene_indices
										)?,
										to_entity: pin_connection_override.to_entity.to_game(
											&entity_indices,
											&reference_indices,
											&external_scene_indices
										)?,
										from_pin_name: pin_connection_override.from_pin.to_owned(),
										to_pin_name: pin_connection_override.to_pin.to_owned(),
										constant_pin_value: {
											if let Some(property) = pin_connection_override.value.as_ref() {
												property.to_game(
													&entity_indices,
													&reference_indices,
													&external_scene_indices
												)?
											} else {
												ZVariant::new(())
											}
										}
									})
								})
								.collect::<Result<_>>()?,
							entity
								.sub_entities
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
																	&entity_indices,
																	&reference_indices,
																	&external_scene_indices
																)?,
																to_entity: trigger_entity.entity_ref.to_game(
																	&entity_indices,
																	&reference_indices,
																	&external_scene_indices
																)?,
																from_pin_name: event.to_owned(),
																to_pin_name: trigger.to_owned(),
																constant_pin_value: if let Some(value) =
																	&trigger_entity.value
																{
																	value.to_game(
																		&entity_indices,
																		&reference_indices,
																		&external_scene_indices
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
						.concat();

						let pin_connection_override_deletes = entity
							.pin_connection_override_deletes
							.par_iter()
							.map(|pin_connection_override_delete| {
								Ok(SExternalEntityTemplatePinConnection {
									from_entity: pin_connection_override_delete.from_entity.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									to_entity: pin_connection_override_delete.to_entity.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									from_pin_name: pin_connection_override_delete.from_pin.to_owned(),
									to_pin_name: pin_connection_override_delete.to_pin.to_owned(),
									constant_pin_value: {
										if let Some(property) = pin_connection_override_delete.value.as_ref() {
											property.to_game(
												&entity_indices,
												&reference_indices,
												&external_scene_indices
											)?
										} else {
											ZVariant::new(())
										}
									}
								})
							})
							.collect::<Result<_>>()?;

						impl_fl_others!(
							$game,
							STemplateEntityBlueprint {
								sub_type: match entity.sub_type {
									SubType::Brick => 2,
									SubType::Scene => 1,
									SubType::Template => 0
								},
								root_entity_index: *entity_indices
									.get(&entity.root_entity)
									.context("Root entity was non-existent")? as i32,
								$sub_entities: vec![],
								pin_connections: vec![],
								input_pin_forwardings: vec![],
								output_pin_forwardings: vec![],
								override_deletes: entity
									.override_deletes
									.par_iter()
									.map(|override_delete| {
										override_delete.to_game(
											&entity_indices,
											&reference_indices,
											&external_scene_indices
										)
									})
									.collect::<Result<_>>()?,
								external_scene_type_indices_in_resource_header: blueprint_meta
									.references
									.iter()
									.enumerate()
									.filter_map(|(idx, reference)| entity
										.external_scenes
										.contains(&reference.resource)
										.then_some(idx as i32))
									.collect(),
								pin_connection_overrides,
								pin_connection_override_deletes,
								external_scene_runtime_resource_ids: entity
									.external_scenes
									.iter()
									.map(|scene| ZRuntimeResourceID::from_u64(
										scene.as_u64() | ((GamePlatform::PC.tag().unwrap() as u64) << 56)
									))
									.collect(),
								source_resource_id: entity.factory.to_eco_string()
							},
							STemplateEntityBlueprint {
								sub_type: match entity.sub_type {
									SubType::Brick => 2,
									SubType::Scene => 1,
									SubType::Template => 0
								},
								root_entity_index: *entity_indices
									.get(&entity.root_entity)
									.context("Root entity was non-existent")? as i32,
								$sub_entities: vec![],
								pin_connections: vec![],
								input_pin_forwardings: vec![],
								output_pin_forwardings: vec![],
								override_deletes: entity
									.override_deletes
									.par_iter()
									.map(|override_delete| {
										override_delete.to_game(
											&entity_indices,
											&reference_indices,
											&external_scene_indices
										)
									})
									.collect::<Result<_>>()?,
								external_scene_type_indices_in_resource_header: (0..entity.external_scenes.len()
									as i32)
									.collect(),
								pin_connection_overrides,
								pin_connection_override_deletes
							}
						)
					}
				);

				let blueprint_dependencies_index_mapping: HashMap<RuntimeID, usize, BuildIdentityHasher<u64>> =
					blueprint_meta
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
										Ok(impl_fl_others!(
											$game,
											SEntityTemplatePropertyOverride {
												property_owner: ext_entity.to_game(
													&entity_indices,
													&reference_indices,
													&external_scene_indices
												)?,
												property_value: SEntityTemplateProperty {
													property_id: convert_string_property_name_to_id(property)?,
													value: overridden.to_game(
														&entity_indices,
														&reference_indices,
														&external_scene_indices
													)?
												},
												is_rt_editable: property_override.runtime_editable.contains(property)
											},
											SEntityTemplatePropertyOverride {
												property_owner: ext_entity.to_game(
													&entity_indices,
													&reference_indices,
													&external_scene_indices
												)?,
												property_value: SEntityTemplateProperty {
													property_id: convert_string_property_name_to_id(property)?,
													value: overridden.to_game(
														&entity_indices,
														&reference_indices,
														&external_scene_indices
													)?
												}
											}
										))
									})
									.collect_vec()
							})
							.collect_vec()
					})
					.collect::<Result<_>>()?;

				factory.$sub_entities = entity
					.sub_entities
					.par_iter()
					.map(|(_, sub_entity)| {
						Ok(impl_h1_others!(
							$game,
							STemplateSubEntity {
								logical_parent: sub_entity.parent.to_game(
									&entity_indices,
									&reference_indices,
									&external_scene_indices
								)?,
								entity_type_resource_index: *reference_indices.get(&sub_entity.factory.resource).ctx?
									as i32,
								property_values: sub_entity
									.properties
									.iter()
									.filter(|(_, property)| !property.post_init)
									.map(|(name, property)| {
										Ok(SEntityTemplateProperty {
											property_id: convert_string_property_name_to_id(name)?,
											value: property.value.to_game(
												&entity_indices,
												&reference_indices,
												&external_scene_indices
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
												&entity_indices,
												&reference_indices,
												&external_scene_indices
											)?
										})
									})
									.collect::<Result<_>>()?
							},
							impl_h3_fl_others!(
								$game,
								STemplateFactorySubEntity {
									logical_parent: sub_entity.parent.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									entity_type_resource_index: *reference_indices
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
													&entity_indices,
													&reference_indices,
													&external_scene_indices
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
													&entity_indices,
													&reference_indices,
													&external_scene_indices
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
																&entity_indices,
																&reference_indices,
																&external_scene_indices
															)?
														}
													})
												})
												.collect_vec()
										})
										.collect::<Result<_>>()?
								},
								STemplateFactorySubEntity {
									logical_parent: sub_entity.parent.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									entity_type_resource_index: *reference_indices
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
													&entity_indices,
													&reference_indices,
													&external_scene_indices
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
													&entity_indices,
													&reference_indices,
													&external_scene_indices
												)?
											})
										})
										.collect::<Result<_>>()?
								}
							)
						))
					})
					.collect::<Result<_>>()?;

				blueprint.$sub_entities = entity
					.sub_entities
					.par_iter()
					.map(|(entity_id, sub_entity)| {
						Ok(impl_h1_others!(
							$game,
							STemplateSubEntityBlueprint {
								logical_parent: sub_entity.parent.to_game(
									&entity_indices,
									&reference_indices,
									&external_scene_indices
								)?,
								entity_type_resource_index: *blueprint_dependencies_index_mapping
									.get(&sub_entity.blueprint)
									.ctx? as i32,
								entity_id: (*entity_id).into(),
								entity_name: sub_entity.name.to_owned(),
								property_aliases: sub_entity
									.property_aliases
									.iter()
									.map(|(aliased_name, aliases)| -> Result<_> {
										aliases
											.iter()
											.map(|alias| -> Result<_> {
												Ok(SEntityTemplatePropertyAlias {
													entity_id: entity_indices
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
									.flat_map(|(exposed_name, exposed_entity)| {
										exposed_entity.refers_to.iter().map(|target| {
											Ok((
												exposed_name.to_owned(),
												target.to_game(
													&entity_indices,
													&reference_indices,
													&external_scene_indices
												)?
											))
										})
									})
									.collect::<Result<_>>()?,
								exposed_interfaces: sub_entity
									.exposed_interfaces
									.iter()
									.map(|(interface, implementor)| -> Result<_> {
										Ok((
											interface.to_owned(),
											entity_indices
												.get(implementor)
												.context("Exposed interface referenced nonexistent local entity")?
												.to_owned() as i32
										))
									})
									.collect::<Result<Vec<_>>>()?,
								entity_subsets: vec![] // will be mutated later
							},
							impl_fl_others!(
								$game,
								STemplateBlueprintSubEntity {
									logical_parent: sub_entity.parent.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									entity_type_resource_index: *blueprint_dependencies_index_mapping
										.get(&sub_entity.blueprint)
										.ctx? as i32,
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
														entity_id: entity_indices
															.get(&alias.original_entity)
															.with_context(|| {
																format!(
																	"Property alias referred to nonexistent entity \
																	 ID: {}",
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
													.map(|target| {
														target.to_game(
															&entity_indices,
															&reference_indices,
															&external_scene_indices
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
												interface.to_owned(),
												entity_indices
													.get(implementor)
													.context("Exposed interface referenced nonexistent local entity")?
													.to_owned() as i32
											))
										})
										.collect::<Result<Vec<_>>>()?,
									excluded_platforms: sub_entity
										.excluded_platforms
										.iter()
										.map(|platform| {
											platform
												.as_str()
												.parse()
												.map_err(|_| anyhow!("Invalid platform ID: {platform}"))
										})
										.collect::<Result<Vec<_>>>()?
								},
								STemplateBlueprintSubEntity {
									logical_parent: sub_entity.parent.to_game(
										&entity_indices,
										&reference_indices,
										&external_scene_indices
									)?,
									entity_type_resource_index: *blueprint_dependencies_index_mapping
										.get(&sub_entity.blueprint)
										.ctx? as i32,
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
														entity_id: entity_indices
															.get(&alias.original_entity)
															.with_context(|| {
																format!(
																	"Property alias referred to nonexistent entity \
																	 ID: {}",
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
													.map(|target| {
														target.to_game(
															&entity_indices,
															&reference_indices,
															&external_scene_indices
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
												interface.to_owned(),
												entity_indices
													.get(implementor)
													.context("Exposed interface referenced nonexistent local entity")?
													.to_owned() as i32
											))
										})
										.collect::<Result<Vec<_>>>()?,
									entity_subsets: vec![] // will be mutated later
								}
							)
						))
					})
					.collect::<Result<_>>()?;

				impl_fl_others!($game, {}, {
					for (entity_index, (_, sub_entity)) in entity.sub_entities.iter().enumerate() {
						for (subset, ents) in sub_entity.subsets.iter() {
							for ent in ents.iter() {
								let ent_subs = &mut blueprint
									.$sub_entities
									.get_mut(
										*entity_indices
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
				});

				#[try_fn]
				#[context("Failure getting pin connections for event")]
				#[auto_context]
				#[hotpath::measure]
				fn pin_connections_for_event(
					entity_id: EntityID,
					event: &EcoString,
					triggers: &OrderMap<EcoString, Vec<PinConnection>>,
					entity_indices: &HashMap<EntityID, usize>,
					_reference_indices: &HashMap<RuntimeID, usize>,
					_external_scene_indices: &HashMap<RuntimeID, usize>
				) -> Result<Vec<SEntityTemplatePinConnection>> {
					triggers
						.iter()
						.map(|(trigger, entities)| -> Result<_> {
							entities
								.iter()
								.filter(|&trigger_entity| trigger_entity.entity_ref.external_scene.is_none())
								.map(|trigger_entity| {
									if trigger_entity.entity_ref.exposed_entity.is_some() {
										bail!("Local pin connections cannot refer to exposed entities")
									}

									Ok(impl_h1_others!(
										$game,
										SEntityTemplatePinConnection {
											from_id: *entity_indices.get(&entity_id).ctx? as i32,
											to_id: *entity_indices
												.get(&trigger_entity.entity_ref.entity_id)
												.with_context(|| {
													format!(
														"Pin connection referred to nonexistent entity ID: {}",
														trigger_entity.entity_ref.entity_id
													)
												})? as i32,
											from_pin_name: event.to_owned(),
											to_pin_name: trigger.to_owned()
										},
										SEntityTemplatePinConnection {
											from_id: *entity_indices.get(&entity_id).ctx? as i32,
											to_id: *entity_indices
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
													entity_indices,
													_reference_indices,
													_external_scene_indices
												)?
											} else {
												ZVariant::new(())
											}
										}
									))
								})
								.collect::<Result<Vec<SEntityTemplatePinConnection>>>()
						})
						.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
						.into_iter()
						.flatten()
						.collect_vec()
				}

				#[try_fn]
				#[context("Failure getting local pin connections for event")]
				#[auto_context]
				#[hotpath::measure]
				fn local_pin_connections_for_event(
					entity_id: EntityID,
					event: &EcoString,
					triggers: &OrderMap<EcoString, Vec<LocalPinConnection>>,
					entity_indices: &HashMap<EntityID, usize>,
					_reference_indices: &HashMap<RuntimeID, usize>,
					_external_scene_indices: &HashMap<RuntimeID, usize>
				) -> Result<Vec<SEntityTemplatePinConnection>> {
					triggers
						.iter()
						.map(|(trigger, entities)| -> Result<_> {
							entities
								.iter()
								.map(|trigger_entity| {
									Ok(impl_h1_others!(
										$game,
										SEntityTemplatePinConnection {
											from_id: *entity_indices.get(&entity_id).ctx? as i32,
											to_id: *entity_indices.get(&trigger_entity.entity_id).with_context(
												|| {
													format!(
														"Pin connection referred to nonexistent entity ID: {}",
														trigger_entity.entity_id
													)
												}
											)? as i32,
											from_pin_name: event.to_owned(),
											to_pin_name: trigger.to_owned()
										},
										SEntityTemplatePinConnection {
											from_id: *entity_indices.get(&entity_id).ctx? as i32,
											to_id: *entity_indices.get(&trigger_entity.entity_id).with_context(
												|| {
													format!(
														"Pin connection referred to nonexistent entity ID: {}",
														trigger_entity.entity_id
													)
												}
											)? as i32,
											from_pin_name: event.to_owned(),
											to_pin_name: trigger.to_owned(),
											constant_pin_value: if let Some(value) = &trigger_entity.value {
												value.to_game(
													entity_indices,
													_reference_indices,
													_external_scene_indices
												)?
											} else {
												ZVariant::new(())
											}
										}
									))
								})
								.collect::<Result<Vec<SEntityTemplatePinConnection>>>()
						})
						.collect::<Result<Vec<Vec<SEntityTemplatePinConnection>>>>()?
						.into_iter()
						.flatten()
						.collect_vec()
				}

				blueprint.pin_connections = entity
					.sub_entities
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
									&entity_indices,
									&reference_indices,
									&external_scene_indices
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
					.sub_entities
					.par_iter()
					.map(|(&entity_id, sub_entity)| -> Result<_> {
						Ok(sub_entity
							.input_forwardings
							.iter()
							.map(|(evt, triggers)| {
								local_pin_connections_for_event(
									entity_id,
									evt,
									triggers,
									&entity_indices,
									&reference_indices,
									&external_scene_indices
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
					.sub_entities
					.par_iter()
					.map(|(&entity_id, sub_entity)| -> Result<_> {
						Ok(sub_entity
							.output_forwardings
							.iter()
							.map(|(evt, triggers)| {
								local_pin_connections_for_event(
									entity_id,
									evt,
									triggers,
									&entity_indices,
									&reference_indices,
									&external_scene_indices
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

				(factory, factory_meta, blueprint, blueprint_meta)
			}
		}
	};
}

#[cfg(feature = "h1")]
impl_game!(h1, STemplateEntity, entity_templates);

#[cfg(feature = "h2")]
impl_game!(h2, STemplateEntityFactory, sub_entities);

#[cfg(feature = "h3")]
impl_game!(h3, STemplateEntityFactory, sub_entities);

#[cfg(feature = "fl")]
impl_game!(fl, STemplateEntityFactory, sub_entities);

impl Entity {
	#[cfg(feature = "rune")]
	#[rune::function(path = Self::from_game)]
	fn r_from_game(
		factory: &str,
		factory_meta: &ResourceMetadata,
		blueprint: &str,
		blueprint_meta: &ResourceMetadata,
		(version, convert_lossless): (GlacierGame, bool)
	) -> Result<Self> {
		match version {
			GlacierGame::H1 => {
				let factory: glacier_bin1::game::h1::STemplateEntity = serde_json::from_str(factory)?;
				let blueprint = serde_json::from_str(blueprint)?;
				Self::from_game(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
			}

			GlacierGame::H2 => {
				let factory: glacier_bin1::game::h2::STemplateEntityFactory = serde_json::from_str(factory)?;
				let blueprint = serde_json::from_str(blueprint)?;
				Self::from_game(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
			}

			GlacierGame::H3 => {
				let factory: glacier_bin1::game::h3::STemplateEntityFactory = serde_json::from_str(factory)?;
				let blueprint = serde_json::from_str(blueprint)?;
				Self::from_game(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
			}

			GlacierGame::FL => {
				let factory: glacier_bin1::game::fl::STemplateEntityFactory = serde_json::from_str(factory)?;
				let blueprint = serde_json::from_str(blueprint)?;
				Self::from_game(&factory, factory_meta, &blueprint, blueprint_meta, convert_lossless)
			}
		}
	}

	pub fn from_game<T: ToQuickEntity>(
		factory: &T,
		factory_meta: &ResourceMetadata,
		blueprint: &T::Blueprint,
		blueprint_meta: &ResourceMetadata,
		convert_lossless: bool
	) -> Result<Self, T::Error>
	where
		for<'a> &'a T: Into<&'a T::Factory>,
		T::QuickEntity: Into<Self>
	{
		factory
			.to_qn(
				factory.into(),
				factory_meta,
				blueprint,
				blueprint_meta,
				convert_lossless
			)
			.map(Into::into)
	}

	#[cfg(feature = "rune")]
	#[try_fn]
	#[rune::function(path = Self::to_game, instance)]
	fn r_to_game(&self, version: GlacierGame) -> Result<(String, ResourceMetadata, String, ResourceMetadata)> {
		match version {
			GlacierGame::H1 => {
				let (fac, fac_meta, blu, blu_meta): (glacier_bin1::game::h1::STemplateEntity, _, _, _) =
					self.to_game()?;

				(
					serde_json::to_string(&fac)?,
					fac_meta,
					serde_json::to_string(&blu)?,
					blu_meta
				)
			}

			GlacierGame::H2 => {
				let (fac, fac_meta, blu, blu_meta): (glacier_bin1::game::h2::STemplateEntityFactory, _, _, _) =
					self.to_game()?;

				(
					serde_json::to_string(&fac)?,
					fac_meta,
					serde_json::to_string(&blu)?,
					blu_meta
				)
			}

			GlacierGame::H3 => {
				let (fac, fac_meta, blu, blu_meta): (glacier_bin1::game::h3::STemplateEntityFactory, _, _, _) =
					self.to_game()?;

				(
					serde_json::to_string(&fac)?,
					fac_meta,
					serde_json::to_string(&blu)?,
					blu_meta
				)
			}

			GlacierGame::FL => {
				let (fac, fac_meta, blu, blu_meta): (glacier_bin1::game::fl::STemplateEntityFactory, _, _, _) =
					self.to_game()?;

				(
					serde_json::to_string(&fac)?,
					fac_meta,
					serde_json::to_string(&blu)?,
					blu_meta
				)
			}
		}
	}

	pub fn to_game<T: FromQuickEntity<Entity>>(&self) -> Result<T, T::Error> {
		T::from_qn(self, &Default::default(), &Default::default(), &Default::default())
	}
}

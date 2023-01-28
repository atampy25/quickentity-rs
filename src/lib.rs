pub mod patch_structs;
pub mod qn_structs;
pub mod rpkg_structs;
pub mod rt_2016_structs;
pub mod rt_structs;
pub mod util_structs;

use linked_hash_map::LinkedHashMap;
use patch_structs::{PatchOperation, SubEntityOperation};
use rt_2016_structs::{
	RTBlueprint2016, RTFactory2016, SEntityTemplatePinConnection2016, STemplateSubEntity,
	STemplateSubEntityBlueprint
};
use std::collections::HashMap;

use itertools::Itertools;
use rayon::prelude::*;
use serde_json::{from_value, json, to_value, Value};

use qn_structs::{
	Dependency, DependencyWithFlag, Entity, ExposedEntity, FullRef, OverriddenProperty,
	PinConnectionOverride, PinConnectionOverrideDelete, Property, PropertyAlias, PropertyOverride,
	Ref, RefMaybeConstantValue, RefWithConstantValue, SimpleProperty, SubEntity, SubType
};
use rpkg_structs::{ResourceDependency, ResourceMeta};
use rt_structs::{
	PropertyID, RTBlueprint, RTFactory, SEntityTemplateEntitySubset, SEntityTemplateExposedEntity,
	SEntityTemplatePinConnection, SEntityTemplatePlatformSpecificProperty, SEntityTemplateProperty,
	SEntityTemplatePropertyAlias, SEntityTemplatePropertyOverride, SEntityTemplatePropertyValue,
	SEntityTemplateReference, SExternalEntityTemplatePinConnection, STemplateBlueprintSubEntity,
	STemplateFactorySubEntity
};
use util_structs::{SMatrix43PropertyValue, ZGuidPropertyValue, ZRuntimeResourceIDPropertyValue};

const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

#[time_graph::instrument]
pub fn apply_patch(entity: &mut Entity, patch: &Value) {
	let patch: Vec<PatchOperation> = from_value(
		patch
			.get("patch")
			.expect("Patch didn't define a patch!")
			.to_owned()
	)
	.unwrap();

	for operation in patch {
		match operation {
			PatchOperation::SetRootEntity(value) => {
				entity.root_entity = value;
			}

			PatchOperation::SetSubType(value) => {
				entity.sub_type = value;
			}

			PatchOperation::RemoveEntityByID(value) => {
				entity.entities.remove(&value);
			}

			PatchOperation::AddEntity(id, data) => {
				entity.entities.insert(id, *data);
			}

			PatchOperation::SubEntityOperation(entity_id, op) => {
				let entity = entity
					.entities
					.get_mut(&entity_id)
					.expect("SubEntityOperation couldn't find entity ID!");

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

					SubEntityOperation::SetFactoryFlag(value) => {
						entity.factory_flag = value;
					}

					SubEntityOperation::SetBlueprint(value) => {
						entity.blueprint = value;
					}

					SubEntityOperation::SetEditorOnly(value) => {
						entity.editor_only = value;
					}

					SubEntityOperation::AddProperty(name, data) => {
						entity
							.properties
							.get_or_insert(Default::default())
							.insert(name, data);
					}

					SubEntityOperation::RemovePropertyByName(name) => {
						entity
							.properties
							.as_mut()
							.expect("RemovePropertyByName couldn't find properties!")
							.remove(&name)
							.expect("RemovePropertyByName couldn't find expected property!");

						if entity.properties.as_ref().unwrap().is_empty() {
							entity.properties = None;
						}
					}

					SubEntityOperation::SetPropertyType(name, value) => {
						entity
							.properties
							.get_or_insert(Default::default())
							.get_mut(&name)
							.expect("SetPropertyType couldn't find expected property!")
							.property_type = value;
					}

					SubEntityOperation::SetPropertyValue {
						property_name,
						value
					} => {
						entity
							.properties
							.get_or_insert(Default::default())
							.get_mut(&property_name)
							.expect("SetPropertyValue couldn't find expected property!")
							.value = value;
					}

					SubEntityOperation::SetPropertyPostInit(name, value) => {
						entity
							.properties
							.get_or_insert(Default::default())
							.get_mut(&name)
							.expect("SetPropertyPostInit couldn't find expected property!")
							.post_init = if value { Some(true) } else { None };
					}

					SubEntityOperation::AddPlatformSpecificProperty(platform, name, data) => {
						entity
							.platform_specific_properties
							.get_or_insert(Default::default())
							.entry(platform)
							.or_default()
							.insert(name, data);
					}

					SubEntityOperation::RemovePlatformSpecificPropertiesForPlatform(name) => {
						entity
							.platform_specific_properties
							.as_mut()
							.expect("RemovePSPropertiesForPlatform couldn't find properties!")
							.remove(&name)
							.expect(
								"RemovePSPropertiesForPlatform couldn't find platform to remove!"
							);

						if entity
							.platform_specific_properties
							.as_ref()
							.unwrap()
							.is_empty()
						{
							entity.platform_specific_properties = None;
						}
					}

					SubEntityOperation::RemovePlatformSpecificPropertyByName(platform, name) => {
						entity
							.platform_specific_properties
							.as_mut()
							.expect("RemovePSPropertyByName couldn't find properties!")
							.get_mut(&platform)
							.expect("RemovePSPropertyByName couldn't find platform!")
							.remove(&name);

						if entity
							.platform_specific_properties
							.as_ref()
							.unwrap()
							.get(&platform)
							.unwrap()
							.is_empty()
						{
							entity
								.platform_specific_properties
								.as_mut()
								.unwrap()
								.remove(&platform);
						}

						if entity
							.platform_specific_properties
							.as_ref()
							.unwrap()
							.is_empty()
						{
							entity.platform_specific_properties = None;
						}
					}

					SubEntityOperation::SetPlatformSpecificPropertyType(platform, name, value) => {
						entity
							.platform_specific_properties
							.as_mut()
							.expect("SetPSPropertyType couldn't find properties!")
							.get_mut(&platform)
							.expect("SetPSPropertyType couldn't find expected platform!")
							.get_mut(&name)
							.expect("SetPSPropertyType couldn't find expected property!")
							.property_type = value;
					}

					SubEntityOperation::SetPlatformSpecificPropertyValue {
						platform,
						property_name,
						value
					} => {
						entity
							.platform_specific_properties
							.as_mut()
							.expect("SetPSPropertyValue couldn't find properties!")
							.get_mut(&platform)
							.expect("SetPSPropertyValue couldn't find expected platform!")
							.get_mut(&property_name)
							.expect("SetPSPropertyValue couldn't find expected property!")
							.value = value;
					}

					SubEntityOperation::SetPlatformSpecificPropertyPostInit(
						platform,
						name,
						value
					) => {
						entity
							.platform_specific_properties
							.as_mut()
							.expect("SetPSPropertyPostInit couldn't find properties!")
							.get_mut(&platform)
							.expect("SetPSPropertyPostInit couldn't find expected platform!")
							.get_mut(&name)
							.expect("SetPSPropertyPostInit couldn't find expected property!")
							.post_init = if value { Some(true) } else { None };
					}

					SubEntityOperation::RemoveAllEventConnectionsForEvent(event) => {
						entity
							.events
							.as_mut()
							.expect("RemoveAllEventConnectionsForEvent couldn't find events!")
							.remove(&event)
							.expect("RemoveAllEventConnectionsForEvent couldn't find event!");

						if entity.events.as_ref().unwrap().is_empty() {
							entity.events = None;
						}
					}

					SubEntityOperation::RemoveAllEventConnectionsForTrigger(event, trigger) => {
						entity
							.events
							.as_mut()
							.expect("RemoveAllEventConnectionsForTrigger couldn't find events!")
							.get_mut(&event)
							.expect("RemoveAllEventConnectionsForTrigger couldn't find event!")
							.remove(&trigger)
							.expect("RemoveAllEventConnectionsForTrigger couldn't find trigger!");

						if entity
							.events
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.events.as_mut().unwrap().remove(&event);
						}

						if entity.events.as_ref().unwrap().is_empty() {
							entity.events = None;
						}
					}

					SubEntityOperation::RemoveEventConnection(event, trigger, reference) => {
						let ind = entity
							.events
							.as_ref()
							.expect("RemoveEventConnection couldn't find events!")
							.get(&event)
							.expect("RemoveEventConnection couldn't find event!")
							.get(&trigger)
							.expect("RemoveEventConnection couldn't find trigger!")
							.iter()
							.position(|x| *x == reference)
							.expect("RemoveEventConnection couldn't find reference!");

						entity
							.events
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.remove(ind);

						if entity
							.events
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.unwrap()
							.is_empty()
						{
							entity
								.events
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.remove(&trigger);
						}

						if entity
							.events
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.events.as_mut().unwrap().remove(&event);
						}

						if entity.events.as_ref().unwrap().is_empty() {
							entity.events = None;
						}
					}

					SubEntityOperation::AddEventConnection(event, trigger, reference) => {
						if entity.events.is_none() {
							entity.events = Some(Default::default());
						}

						if entity.events.as_ref().unwrap().get(&event).is_none() {
							entity
								.events
								.as_mut()
								.unwrap()
								.insert(event.to_owned(), Default::default());
						}

						if entity
							.events
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.is_none()
						{
							entity
								.events
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.insert(trigger.to_owned(), Default::default());
						}

						entity
							.events
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.push(reference);
					}

					SubEntityOperation::RemoveAllInputCopyConnectionsForInput(event) => {
						entity
							.input_copying
							.as_mut()
							.expect("RemoveAllInputCopyConnectionsForInput couldn't find input copying!")
							.remove(&event)
							.expect("RemoveAllInputCopyConnectionsForInput couldn't find input!");

						if entity.input_copying.as_ref().unwrap().is_empty() {
							entity.input_copying = None;
						}
					}

					SubEntityOperation::RemoveAllInputCopyConnectionsForTrigger(event, trigger) => {
						entity
							.input_copying
							.as_mut()
							.expect("RemoveAllInputCopyConnectionsForTrigger couldn't find input copying!")
							.get_mut(&event)
							.expect("RemoveAllInputCopyConnectionsForTrigger couldn't find input!")
							.remove(&trigger)
							.expect("RemoveAllInputCopyConnectionsForTrigger couldn't find trigger!");

						if entity
							.input_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.input_copying.as_mut().unwrap().remove(&event);
						}

						if entity.input_copying.as_ref().unwrap().is_empty() {
							entity.input_copying = None;
						}
					}

					SubEntityOperation::RemoveInputCopyConnection(event, trigger, reference) => {
						let ind = entity
							.input_copying
							.as_ref()
							.expect("RemoveInputCopyConnection couldn't find input copying!")
							.get(&event)
							.expect("RemoveInputCopyConnection couldn't find input!")
							.get(&trigger)
							.expect("RemoveInputCopyConnection couldn't find trigger!")
							.iter()
							.position(|x| *x == reference)
							.expect("RemoveInputCopyConnection couldn't find reference!");

						entity
							.input_copying
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.remove(ind);

						if entity
							.input_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.unwrap()
							.is_empty()
						{
							entity
								.input_copying
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.remove(&trigger);
						}

						if entity
							.input_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.input_copying.as_mut().unwrap().remove(&event);
						}

						if entity.input_copying.as_ref().unwrap().is_empty() {
							entity.input_copying = None;
						}
					}

					SubEntityOperation::AddInputCopyConnection(event, trigger, reference) => {
						if entity.input_copying.is_none() {
							entity.input_copying = Some(Default::default());
						}

						if entity.input_copying.as_ref().unwrap().get(&event).is_none() {
							entity
								.input_copying
								.as_mut()
								.unwrap()
								.insert(event.to_owned(), Default::default());
						}

						if entity
							.input_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.is_none()
						{
							entity
								.input_copying
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.insert(trigger.to_owned(), Default::default());
						}

						entity
							.input_copying
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.push(reference);
					}

					SubEntityOperation::RemoveAllOutputCopyConnectionsForOutput(event) => {
						entity
							.output_copying
							.as_mut()
							.expect("RemoveAllOutputCopyConnectionsForOutput couldn't find output copying!")
							.remove(&event)
							.expect("RemoveAllOutputCopyConnectionsForOutput couldn't find event!");

						if entity.output_copying.as_ref().unwrap().is_empty() {
							entity.output_copying = None;
						}
					}

					SubEntityOperation::RemoveAllOutputCopyConnectionsForPropagate(
						event,
						trigger
					) => {
						entity
							.output_copying
							.as_mut()
							.expect("RemoveAllOutputCopyConnectionsForPropagate couldn't find output copying!")
							.get_mut(&event)
							.expect("RemoveAllOutputCopyConnectionsForPropagate couldn't find event!")
							.remove(&trigger)
							.expect("RemoveAllOutputCopyConnectionsForPropagate couldn't find propagate!");

						if entity
							.output_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.output_copying.as_mut().unwrap().remove(&event);
						}

						if entity.output_copying.as_ref().unwrap().is_empty() {
							entity.output_copying = None;
						}
					}

					SubEntityOperation::RemoveOutputCopyConnection(event, trigger, reference) => {
						let ind = entity
							.output_copying
							.as_ref()
							.expect("RemoveOutputCopyConnection couldn't find output copying!")
							.get(&event)
							.expect("RemoveOutputCopyConnection couldn't find event!")
							.get(&trigger)
							.expect("RemoveOutputCopyConnection couldn't find propagate!")
							.iter()
							.position(|x| *x == reference)
							.expect("RemoveOutputCopyConnection couldn't find reference!");

						entity
							.output_copying
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.remove(ind);

						if entity
							.output_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.unwrap()
							.is_empty()
						{
							entity
								.output_copying
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.remove(&trigger);
						}

						if entity
							.output_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.is_empty()
						{
							entity.output_copying.as_mut().unwrap().remove(&event);
						}

						if entity.output_copying.as_ref().unwrap().is_empty() {
							entity.output_copying = None;
						}
					}

					SubEntityOperation::AddOutputCopyConnection(event, trigger, reference) => {
						if entity.output_copying.is_none() {
							entity.output_copying = Some(Default::default());
						}

						if entity
							.output_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.is_none()
						{
							entity
								.output_copying
								.as_mut()
								.unwrap()
								.insert(event.to_owned(), Default::default());
						}

						if entity
							.output_copying
							.as_ref()
							.unwrap()
							.get(&event)
							.unwrap()
							.get(&trigger)
							.is_none()
						{
							entity
								.output_copying
								.as_mut()
								.unwrap()
								.get_mut(&event)
								.unwrap()
								.insert(trigger.to_owned(), Default::default());
						}

						entity
							.output_copying
							.as_mut()
							.unwrap()
							.get_mut(&event)
							.unwrap()
							.get_mut(&trigger)
							.unwrap()
							.push(reference);
					}

					SubEntityOperation::AddPropertyAliasConnection(alias, data) => {
						entity
							.property_aliases
							.get_or_insert(Default::default())
							.entry(alias)
							.or_default()
							.push(data);
					}

					SubEntityOperation::RemovePropertyAlias(alias) => {
						entity
							.property_aliases
							.get_or_insert(Default::default())
							.remove(&alias)
							.expect("RemovePropertyAlias couldn't find alias!");

						if entity.property_aliases.as_ref().unwrap().is_empty() {
							entity.property_aliases = None;
						}
					}

					SubEntityOperation::RemoveConnectionForPropertyAlias(alias, data) => {
						let connection = entity
							.property_aliases
							.as_ref()
							.expect("RemoveConnectionForPropertyAlias had no aliases to remove!")
							.get(&alias)
							.expect("RemoveConnectionForPropertyAlias couldn't find alias!")
							.iter()
							.position(|x| *x == data)
							.expect("RemoveConnectionForPropertyAlias couldn't find connection!");

						entity
							.property_aliases
							.as_mut()
							.unwrap()
							.get_mut(&alias)
							.unwrap()
							.remove(connection);

						if entity
							.property_aliases
							.as_ref()
							.unwrap()
							.get(&alias)
							.unwrap()
							.is_empty()
						{
							entity.property_aliases.as_mut().unwrap().remove(&alias);
						}

						if entity.property_aliases.as_ref().unwrap().is_empty() {
							entity.property_aliases = None;
						}
					}

					SubEntityOperation::SetExposedEntity(name, data) => {
						entity
							.exposed_entities
							.get_or_insert(Default::default())
							.insert(name, data);
					}

					SubEntityOperation::RemoveExposedEntity(name) => {
						entity
							.exposed_entities
							.as_mut()
							.expect("RemoveExposedEntity had no exposed entities to remove!")
							.remove(&name)
							.expect("RemoveExposedEntity couldn't find exposed entity to remove!");
					}

					SubEntityOperation::SetExposedInterface(name, implementor) => {
						entity
							.exposed_interfaces
							.get_or_insert(Default::default())
							.insert(name, implementor);
					}

					SubEntityOperation::RemoveExposedInterface(name) => {
						entity
							.exposed_interfaces
							.as_mut()
							.expect("RemoveExposedInterface had no exposed entities to remove!")
							.remove(&name)
							.expect(
								"RemoveExposedInterface couldn't find exposed entity to remove!"
							);
					}

					SubEntityOperation::AddSubset(name, ent) => {
						entity
							.subsets
							.get_or_insert(Default::default())
							.entry(name)
							.or_default()
							.push(ent);
					}

					SubEntityOperation::RemoveSubset(name, ent) => {
						let ind = entity
							.subsets
							.as_ref()
							.expect("RemoveSubset had no subsets to remove!")
							.get(&name)
							.expect("RemoveSubset couldn't find subset to remove from!")
							.iter()
							.position(|x| *x == ent);

						entity
							.subsets
							.as_mut()
							.unwrap()
							.get_mut(&name)
							.unwrap()
							.remove(ind.expect(
								"RemoveSubset couldn't find the entity to remove from the subset!"
							));
					}

					SubEntityOperation::RemoveAllSubsetsFor(name) => {
						entity
							.subsets
							.as_mut()
							.expect("RemoveAllSubsetsFor had no subsets to remove!")
							.remove(&name)
							.expect("RemoveAllSubsetsFor couldn't find subset to remove!");
					}
				}
			}

			PatchOperation::AddPropertyOverride(value) => {
				entity.property_overrides.push(value);
			}

			PatchOperation::RemovePropertyOverride(value) => {
				entity.property_overrides.remove(
					entity
						.property_overrides
						.par_iter()
						.position_any(|x| *x == value)
						.expect("RemovePropertyOverride couldn't find expected value!")
				);
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
						.expect("RemoveOverrideDelete couldn't find expected value!")
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
						.expect("RemovePinConnectionOverride couldn't find expected value!")
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
						.expect("RemovePinConnectionOverrideDelete couldn't find expected value!")
				);
			}

			PatchOperation::AddExternalScene(value) => {
				entity.external_scenes.push(value);
			}

			PatchOperation::RemoveExternalScene(value) => {
				entity.external_scenes.remove(
					entity
						.external_scenes
						.par_iter()
						.position_any(|x| {
							*x == value
								|| value
									== format!(
										"00{}",
										format!("{:X}", md5::compute(x))
											.chars()
											.skip(2)
											.take(14)
											.collect::<String>()
									) || *x
								== format!(
									"00{}",
									format!("{:X}", md5::compute(&value))
										.chars()
										.skip(2)
										.take(14)
										.collect::<String>()
								)
						})
						.expect("RemoveExternalScene couldn't find expected value!")
				);
			}

			PatchOperation::AddExtraFactoryDependency(value) => {
				entity.extra_factory_dependencies.push(value);
			}

			PatchOperation::RemoveExtraFactoryDependency(value) => {
				entity.extra_factory_dependencies.remove(
					entity
						.extra_factory_dependencies
						.par_iter()
						.position_any(|x| *x == value)
						.expect("RemoveExtraFactoryDependency couldn't find expected value!")
				);
			}

			PatchOperation::AddExtraBlueprintDependency(value) => {
				entity.extra_blueprint_dependencies.push(value);
			}

			PatchOperation::RemoveExtraBlueprintDependency(value) => {
				entity.extra_blueprint_dependencies.remove(
					entity
						.extra_blueprint_dependencies
						.par_iter()
						.position_any(|x| *x == value)
						.expect("RemoveExtraBlueprintDependency couldn't find expected value!")
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
						.expect("RemoveComment couldn't find expected value!")
				);
			}
		}
	}
}

#[time_graph::instrument]
pub fn generate_patch(original: &Entity, modified: &Entity) -> Value {
	if original.quick_entity_version != modified.quick_entity_version {
		panic!("Can't create patches between differing QuickEntity versions!")
	}

	let mut patch: Vec<PatchOperation> = vec![];

	let mut original = original.clone();
	let mut modified = modified.clone();

	if original.root_entity != modified.root_entity {
		patch.push(PatchOperation::SetRootEntity(
			modified.root_entity.to_owned()
		));
	}

	if original.sub_type != modified.sub_type {
		patch.push(PatchOperation::SetSubType(modified.sub_type.to_owned()));
	}

	for entity_id in original.entities.keys() {
		if !modified.entities.contains_key(entity_id) {
			patch.push(PatchOperation::RemoveEntityByID(entity_id.to_owned()));
		}
	}

	for (entity_id, new_entity_data) in &mut modified.entities {
		if let Some(old_entity_data) = original.entities.get_mut(entity_id) {
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

			if old_entity_data.factory_flag != new_entity_data.factory_flag {
				patch.push(PatchOperation::SubEntityOperation(
					entity_id.to_owned(),
					SubEntityOperation::SetFactoryFlag(new_entity_data.factory_flag.to_owned())
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

			if old_entity_data.properties.is_none() {
				old_entity_data.properties = Some(LinkedHashMap::new());
			}

			if new_entity_data.properties.is_none() {
				new_entity_data.properties = Some(LinkedHashMap::new());
			}

			for property_name in old_entity_data.properties.as_ref().unwrap().keys() {
				if !new_entity_data
					.properties
					.as_ref()
					.unwrap()
					.contains_key(property_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePropertyByName(property_name.to_owned())
					));
				}
			}

			for (property_name, new_property_data) in new_entity_data.properties.as_ref().unwrap() {
				if let Some(old_property_data) = old_entity_data
					.properties
					.as_ref()
					.unwrap()
					.get(property_name)
				{
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
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::SetPropertyValue {
								property_name: property_name.to_owned(),
								value: new_property_data.value.to_owned()
							}
						));
					}

					if old_property_data.post_init != new_property_data.post_init {
						patch.push(PatchOperation::SubEntityOperation(
							entity_id.to_owned(),
							SubEntityOperation::SetPropertyPostInit(
								property_name.to_owned(),
								new_property_data.post_init.unwrap_or(false)
							)
						));
					}
				} else {
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::AddProperty(
							property_name.to_owned(),
							new_property_data.to_owned()
						)
					));
				}
			}

			// Duplicated from above except with an extra layer for platform
			if old_entity_data.platform_specific_properties.is_none() {
				old_entity_data.platform_specific_properties = Some(LinkedHashMap::new());
			}

			if new_entity_data.platform_specific_properties.is_none() {
				new_entity_data.platform_specific_properties = Some(LinkedHashMap::new());
			}

			for platform_name in old_entity_data
				.platform_specific_properties
				.as_ref()
				.unwrap()
				.keys()
			{
				if !new_entity_data
					.platform_specific_properties
					.as_ref()
					.unwrap()
					.contains_key(platform_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePlatformSpecificPropertiesForPlatform(
							platform_name.to_owned()
						)
					));
				}
			}

			for (platform_name, new_properties_data) in new_entity_data
				.platform_specific_properties
				.as_ref()
				.unwrap()
			{
				if let Some(old_properties_data) = old_entity_data
					.platform_specific_properties
					.as_ref()
					.unwrap()
					.get(platform_name)
				{
					for property_name in old_properties_data.keys() {
						if !new_entity_data
							.properties
							.as_ref()
							.unwrap()
							.contains_key(property_name)
						{
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
									SubEntityOperation::SetPlatformSpecificPropertyValue {
										platform: platform_name.to_owned(),
										property_name: property_name.to_owned(),
										value: new_property_data.value.to_owned()
									}
								));
							}

							if old_property_data.post_init != new_property_data.post_init {
								patch.push(PatchOperation::SubEntityOperation(
									entity_id.to_owned(),
									SubEntityOperation::SetPlatformSpecificPropertyPostInit(
										platform_name.to_owned(),
										property_name.to_owned(),
										new_property_data.post_init.unwrap_or(false)
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
			if old_entity_data.events.is_none() {
				old_entity_data.events = Some(LinkedHashMap::new());
			}

			if new_entity_data.events.is_none() {
				new_entity_data.events = Some(LinkedHashMap::new());
			}

			for event_name in old_entity_data.events.as_ref().unwrap().keys() {
				if !new_entity_data
					.events
					.as_ref()
					.unwrap()
					.contains_key(event_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllEventConnectionsForEvent(
							event_name.to_owned()
						)
					));
				}
			}

			for (event_name, new_events_data) in new_entity_data.events.as_ref().unwrap() {
				if let Some(old_events_data) =
					old_entity_data.events.as_ref().unwrap().get(event_name)
				{
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

			if old_entity_data.input_copying.is_none() {
				old_entity_data.input_copying = Some(LinkedHashMap::new());
			}

			if new_entity_data.input_copying.is_none() {
				new_entity_data.input_copying = Some(LinkedHashMap::new());
			}

			for event_name in old_entity_data.input_copying.as_ref().unwrap().keys() {
				if !new_entity_data
					.input_copying
					.as_ref()
					.unwrap()
					.contains_key(event_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllInputCopyConnectionsForInput(
							event_name.to_owned()
						)
					));
				}
			}

			for (event_name, new_input_copying_data) in
				new_entity_data.input_copying.as_ref().unwrap()
			{
				if let Some(old_input_copying_data) = old_entity_data
					.input_copying
					.as_ref()
					.unwrap()
					.get(event_name)
				{
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

			if old_entity_data.output_copying.is_none() {
				old_entity_data.output_copying = Some(LinkedHashMap::new());
			}

			if new_entity_data.output_copying.is_none() {
				new_entity_data.output_copying = Some(LinkedHashMap::new());
			}

			for event_name in old_entity_data.output_copying.as_ref().unwrap().keys() {
				if !new_entity_data
					.output_copying
					.as_ref()
					.unwrap()
					.contains_key(event_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllOutputCopyConnectionsForOutput(
							event_name.to_owned()
						)
					));
				}
			}

			for (event_name, new_output_copying_data) in
				new_entity_data.output_copying.as_ref().unwrap()
			{
				if let Some(old_output_copying_data) = old_entity_data
					.output_copying
					.as_ref()
					.unwrap()
					.get(event_name)
				{
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

			if old_entity_data.property_aliases.is_none() {
				old_entity_data.property_aliases = Some(LinkedHashMap::new());
			}

			if new_entity_data.property_aliases.is_none() {
				new_entity_data.property_aliases = Some(LinkedHashMap::new());
			}

			for alias_name in old_entity_data.property_aliases.as_ref().unwrap().keys() {
				if !new_entity_data
					.property_aliases
					.as_ref()
					.unwrap()
					.contains_key(alias_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemovePropertyAlias(alias_name.to_owned())
					));
				}
			}

			for (alias_name, new_alias_connections) in
				new_entity_data.property_aliases.as_ref().unwrap()
			{
				if let Some(old_alias_connections) = old_entity_data
					.property_aliases
					.as_ref()
					.unwrap()
					.get(alias_name)
				{
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

			if old_entity_data.exposed_entities.is_none() {
				old_entity_data.exposed_entities = Some(LinkedHashMap::new());
			}

			if new_entity_data.exposed_entities.is_none() {
				new_entity_data.exposed_entities = Some(LinkedHashMap::new());
			}

			for exposed_entity in old_entity_data.exposed_entities.as_ref().unwrap().keys() {
				if !new_entity_data
					.exposed_entities
					.as_ref()
					.unwrap()
					.contains_key(exposed_entity)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedEntity(exposed_entity.to_owned())
					));
				}
			}

			for (exposed_entity, data) in new_entity_data.exposed_entities.as_ref().unwrap() {
				if !old_entity_data
					.exposed_entities
					.as_ref()
					.unwrap()
					.contains_key(exposed_entity)
					|| old_entity_data
						.exposed_entities
						.as_ref()
						.unwrap()
						.get(exposed_entity)
						.unwrap() != data
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedEntity(
							exposed_entity.to_owned(),
							data.to_owned()
						)
					));
				}
			}

			if old_entity_data.exposed_interfaces.is_none() {
				old_entity_data.exposed_interfaces = Some(LinkedHashMap::new());
			}

			if new_entity_data.exposed_interfaces.is_none() {
				new_entity_data.exposed_interfaces = Some(LinkedHashMap::new());
			}

			for exposed_interface in old_entity_data.exposed_interfaces.as_ref().unwrap().keys() {
				if !new_entity_data
					.exposed_interfaces
					.as_ref()
					.unwrap()
					.contains_key(exposed_interface)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveExposedInterface(exposed_interface.to_owned())
					));
				}
			}

			for (exposed_interface, data) in new_entity_data.exposed_interfaces.as_ref().unwrap() {
				if !old_entity_data
					.exposed_interfaces
					.as_ref()
					.unwrap()
					.contains_key(exposed_interface)
					|| old_entity_data
						.exposed_interfaces
						.as_ref()
						.unwrap()
						.get(exposed_interface)
						.unwrap() != data
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::SetExposedInterface(
							exposed_interface.to_owned(),
							data.to_owned()
						)
					));
				}
			}

			if old_entity_data.subsets.is_none() {
				old_entity_data.subsets = Some(LinkedHashMap::new());
			}

			if new_entity_data.subsets.is_none() {
				new_entity_data.subsets = Some(LinkedHashMap::new());
			}

			for subset_name in old_entity_data.subsets.as_ref().unwrap().keys() {
				if !new_entity_data
					.subsets
					.as_ref()
					.unwrap()
					.contains_key(subset_name)
				{
					patch.push(PatchOperation::SubEntityOperation(
						entity_id.to_owned(),
						SubEntityOperation::RemoveAllSubsetsFor(subset_name.to_owned())
					));
				}
			}

			for (subset_name, new_refs_data) in new_entity_data.subsets.as_ref().unwrap() {
				if let Some(old_refs_data) =
					old_entity_data.subsets.as_ref().unwrap().get(subset_name)
				{
					for i in old_refs_data {
						if !new_refs_data.contains(i) {
							patch.push(PatchOperation::SubEntityOperation(
								entity_id.to_owned(),
								SubEntityOperation::RemoveSubset(
									subset_name.to_owned(),
									i.to_owned()
								)
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
				Box::new(new_entity_data.to_owned())
			));
		}
	}

	for x in &original.property_overrides {
		if !modified.property_overrides.contains(x) {
			patch.push(PatchOperation::RemovePropertyOverride(x.to_owned()))
		}
	}

	for x in &modified.property_overrides {
		if !original.property_overrides.contains(x) {
			patch.push(PatchOperation::AddPropertyOverride(x.to_owned()))
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
			patch.push(PatchOperation::RemovePinConnectionOverrideDelete(
				x.to_owned()
			))
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

	for x in &original.extra_factory_dependencies {
		if !modified.extra_factory_dependencies.contains(x) {
			patch.push(PatchOperation::RemoveExtraFactoryDependency(x.to_owned()))
		}
	}

	for x in &modified.extra_factory_dependencies {
		if !original.extra_factory_dependencies.contains(x) {
			patch.push(PatchOperation::AddExtraFactoryDependency(x.to_owned()))
		}
	}

	for x in &original.extra_blueprint_dependencies {
		if !modified.extra_blueprint_dependencies.contains(x) {
			patch.push(PatchOperation::RemoveExtraBlueprintDependency(x.to_owned()))
		}
	}

	for x in &modified.extra_blueprint_dependencies {
		if !original.extra_blueprint_dependencies.contains(x) {
			patch.push(PatchOperation::AddExtraBlueprintDependency(x.to_owned()))
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

	json!({
		"tempHash": modified.factory_hash,
		"tbluHash": modified.blueprint_hash,
		"patch": patch,
		"patchVersion": 6
	})
}

#[time_graph::instrument]
fn convert_rt_reference_to_qn(
	reference: &SEntityTemplateReference,
	factory: &RTFactory,
	blueprint: &RTBlueprint,
	factory_meta: &ResourceMeta
) -> Ref {
	if !reference.exposed_entity.is_empty() || reference.external_scene_index != -1 {
		Ref::Full(FullRef {
            entity_ref: match reference.entity_index {
                -2 => format!("{:x}", reference.entity_id),
                index if index >= 0 => {
                    format!(
                        "{:x}",
                        blueprint
                            .sub_entities
                            .get(index as usize)
                            .expect("Expected an entity at the index when converting ref to QN")
                            .entity_id
                    )
                }
                _ => panic!("Uhh you can't reference nothing and then ask for an external scene or exposed entity"),
            },
            external_scene: match reference.external_scene_index {
                -1 => None,
                index if index >= 0 => Some(
                    factory_meta
                        .hash_reference_data
                        .get(
                            factory
                                .external_scene_type_indices_in_resource_header
                                .get(index as usize)
                                .expect("Expected an external scene to be in the TEMP").to_owned()
                        )
                        .expect("Expected an external scene to be in the TEMP meta")
                        .hash.to_owned()
                ),
                _ => panic!("Uhh this external scene is not valid at all"),
            },
            exposed_entity: if reference.exposed_entity.is_empty() {
                None
            } else {
                Some(reference.exposed_entity.to_owned())
            },
        })
	} else {
		Ref::Short(match reference.entity_index {
			-1 => None,
			index if index >= 0 => Some(format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(index as usize)
					.expect("Expected an entity at the index when converting ref to QN")
					.entity_id
			)),
			_ => panic!("Uhh you can't have a -2 entity index and then not provide the entity id")
		})
	}
}

#[time_graph::instrument]
fn convert_qn_reference_to_rt(
	reference: &Ref,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>
) -> SEntityTemplateReference {
	match reference {
		Ref::Short(None) => SEntityTemplateReference {
			entity_id: 18446744073709551615,
			external_scene_index: -1,
			entity_index: -1,
			exposed_entity: "".to_string()
		},
		Ref::Short(Some(ent)) => SEntityTemplateReference {
			entity_id: 18446744073709551615,
			external_scene_index: -1,
			entity_index: entity_id_to_index_mapping
				.get(ent)
				.expect("Short ref referred to a nonexistent entity ID")
				.to_owned() as i32,
			exposed_entity: "".to_string()
		},
		Ref::Full(fullref) => SEntityTemplateReference {
			entity_id: match &fullref.external_scene {
				None => 18446744073709551615,
				Some(_) => u64::from_str_radix(fullref.entity_ref.as_str(), 16)
					.expect("Full ref had invalid hex ref")
			},
			external_scene_index: match &fullref.external_scene {
				None => -1,
				Some(extscene) => factory
					.external_scene_type_indices_in_resource_header
					.iter()
					.position(|x| {
						factory_meta.hash_reference_data.get(*x).expect(
                            "TEMP referenced external scene not found in meta in externalScenes",
                        ).hash == *extscene
					})
					.expect(
						"TEMP referenced external scene not found in externalScenes in sub-entity"
					)
					.try_into()
					.unwrap()
			},
			entity_index: match &fullref.external_scene {
				None => entity_id_to_index_mapping
					.get(&fullref.entity_ref)
					.expect("Full ref referred to a nonexistent entity ID")
					.to_owned() as i32,
				Some(_) => -2
			},
			exposed_entity: fullref.exposed_entity.to_owned().unwrap_or_default()
		}
	}
}

#[time_graph::instrument]
fn convert_rt_property_value_to_qn(
	property: &SEntityTemplatePropertyValue,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
	convert_lossless: bool
) -> Value {
	match property.property_type.as_str() {
		"SEntityTemplateReference" => to_value(convert_rt_reference_to_qn(
			&from_value::<SEntityTemplateReference>(property.property_value.to_owned())
				.expect("Converting RT ref to QN in property value returned error in parsing"),
			factory,
			blueprint,
			factory_meta
		))
		.expect("Converting RT ref to QN in property value returned error in serialisation"),

		"ZRuntimeResourceID" => {
			match from_value::<ZRuntimeResourceIDPropertyValue>(property.property_value.to_owned())
				.expect("ZRuntimeResourceID did not have a valid format")
			{
				ZRuntimeResourceIDPropertyValue {
					m_IDHigh: 4294967295,
					m_IDLow: 4294967295
				} => Value::Null,

				ZRuntimeResourceIDPropertyValue {
					m_IDHigh: _, // We ignore the id_high as no resource in the game has that many depends
					m_IDLow: id_low
				} => {
					let depend_data = factory_meta
						.hash_reference_data
						.get(id_low as usize)
						.expect("ZRuntimeResourceID m_IDLow referred to non-existent dependency");

					if depend_data.flag != "1F" {
						json!({
							"resource": depend_data.hash,
							"flag": depend_data.flag
						})
					} else {
						to_value(depend_data.hash.to_owned()).unwrap()
					}
				}
			}
		}

		"SMatrix43" => {
			let mut matrix =
				from_value::<SMatrix43PropertyValue>(property.property_value.to_owned())
					.expect("SMatrix43 did not have a valid format");

			// this is all from three.js

			let n11 = matrix.XAxis.x;
			let n12 = matrix.XAxis.y;
			let n13 = matrix.XAxis.z;
			let n14 = 0.0;
			let n21 = matrix.YAxis.x;
			let n22 = matrix.YAxis.y;
			let n23 = matrix.YAxis.z;
			let n24 = 0.0;
			let n31 = matrix.ZAxis.x;
			let n32 = matrix.ZAxis.y;
			let n33 = matrix.ZAxis.z;
			let n34 = 0.0;
			let n41 = matrix.Trans.x;
			let n42 = matrix.Trans.y;
			let n43 = matrix.Trans.z;
			let n44 = 1.0;

			let det =
				n41 * (n14 * n23 * n32 - n13 * n24 * n32 - n14 * n22 * n33
					+ n12 * n24 * n33 + n13 * n22 * n34
					- n12 * n23 * n34) + n42
					* (n11 * n23 * n34 - n11 * n24 * n33 + n14 * n21 * n33 - n13 * n21 * n34
						+ n13 * n24 * n31 - n14 * n23 * n31)
					+ n43
						* (n11 * n24 * n32 - n11 * n22 * n34 - n14 * n21 * n32
							+ n12 * n21 * n34 + n14 * n22 * n31
							- n12 * n24 * n31) + n44
					* (-n13 * n22 * n31 - n11 * n23 * n32 + n11 * n22 * n33 + n13 * n21 * n32
						- n12 * n21 * n33 + n12 * n23 * n31);

			let mut sx = n11 * n11 + n21 * n21 + n31 * n31;
			let sy = n12 * n12 + n22 * n22 + n32 * n32;
			let sz = n13 * n13 + n23 * n23 + n33 * n33;

			if det < 0.0 {
				sx = -sx
			};

			let pos = json!({ "x": n41, "y": n42, "z": n43 });
			let scale = json!({ "x": sx, "y": sy, "z": sz });

			let inv_sx = 1.0 / sx;
			let inv_sy = 1.0 / sy;
			let inv_sz = 1.0 / sz;

			matrix.XAxis.x *= inv_sx;
			matrix.YAxis.x *= inv_sx;
			matrix.ZAxis.x *= inv_sx;
			matrix.XAxis.y *= inv_sy;
			matrix.YAxis.y *= inv_sy;
			matrix.ZAxis.y *= inv_sy;
			matrix.XAxis.z *= inv_sz;
			matrix.YAxis.z *= inv_sz;
			matrix.ZAxis.z *= inv_sz;

			if if convert_lossless {
				scale.get("x").unwrap().as_f64().unwrap() != 1.0
					|| scale.get("y").unwrap().as_f64().unwrap() != 1.0
					|| scale.get("z").unwrap().as_f64().unwrap() != 1.0
			} else {
				format!("{:.2}", scale.get("x").unwrap().as_f64().unwrap()) != "1.00"
					|| format!("{:.2}", scale.get("y").unwrap().as_f64().unwrap()) != "1.00"
					|| format!("{:.2}", scale.get("z").unwrap().as_f64().unwrap()) != "1.00"
			} {
				json!({
					"rotation": {
						"x": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.YAxis.z).atan2(matrix.ZAxis.z) } else { (matrix.ZAxis.y).atan2(matrix.YAxis.y) }) * RAD2DEG,
						"y": matrix.XAxis.z.clamp(-1.0, 1.0).asin() * RAD2DEG,
						"z": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.XAxis.y).atan2(matrix.XAxis.x) } else { 0.0 }) * RAD2DEG
					},
					"position": pos,
					"scale": scale
				})
			} else {
				json!({
					"rotation": {
						"x": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.YAxis.z).atan2(matrix.ZAxis.z) } else { (matrix.ZAxis.y).atan2(matrix.YAxis.y) }) * RAD2DEG,
						"y": matrix.XAxis.z.clamp(-1.0, 1.0).asin() * RAD2DEG,
						"z": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.XAxis.y).atan2(matrix.XAxis.x) } else { 0.0 }) * RAD2DEG
					},
					"position": pos
				})
			}
		}

		"ZGuid" => {
			let guid = from_value::<ZGuidPropertyValue>(property.property_value.to_owned())
				.expect("ZGuid did not have a valid format");

			to_value(format!(
				"{:0>8x}-{:0>4x}-{:0>4x}-{:0>2x}{:0>2x}-{:0>2x}{:0>2x}{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
				guid._a,
				guid._b,
				guid._c,
				guid._d,
				guid._e,
				guid._f,
				guid._g,
				guid._h,
				guid._i,
				guid._j,
				guid._k
			))
			.unwrap()
		}

		"SColorRGB" => {
			let map = property
				.property_value
				.as_object()
				.expect("SColorRGB was not an object");

			to_value(format!(
				"#{:0>2x}{:0>2x}{:0>2x}",
				(map.get("r")
					.expect("Colour did not have required key r")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8,
				(map.get("g")
					.expect("Colour did not have required key g")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8,
				(map.get("b")
					.expect("Colour did not have required key b")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			))
			.unwrap()
		}

		"SColorRGBA" => {
			let map = property
				.property_value
				.as_object()
				.expect("SColorRGBA was not an object");

			to_value(format!(
				"#{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
				(map.get("r")
					.expect("Colour did not have required key r")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8,
				(map.get("g")
					.expect("Colour did not have required key g")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8,
				(map.get("b")
					.expect("Colour did not have required key b")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8,
				(map.get("a")
					.expect("Colour did not have required key a")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			))
			.unwrap()
		}

		_ => property.property_value.to_owned()
	}
}

#[time_graph::instrument]
fn convert_rt_property_to_qn(
	property: &SEntityTemplateProperty,
	post_init: bool,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
	convert_lossless: bool
) -> Property {
	Property {
		property_type: property.value.property_type.to_owned(),
		value: if property.value.property_value.is_array() {
			to_value(
				property
					.value
					.property_value
					.as_array()
					.unwrap()
					.iter()
					.map(|x| {
						let mut y = property.value.property_type.chars();
						y.nth(6); // discard TArray<
						y.next_back(); // discard closing >

						convert_rt_property_value_to_qn(
							&SEntityTemplatePropertyValue {
								property_type: y.collect::<String>(), // mock a single value for each array element
								property_value: x.to_owned()
							},
							factory,
							factory_meta,
							blueprint,
							convert_lossless
						)
					})
					.collect::<Vec<Value>>()
			)
			.unwrap()
		} else {
			convert_rt_property_value_to_qn(
				&property.value,
				factory,
				factory_meta,
				blueprint,
				convert_lossless
			)
		},
		post_init: if post_init { Some(true) } else { None }
	}
}

#[time_graph::instrument]
fn convert_qn_property_value_to_rt(
	property: &Property,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>,
	factory_dependencies_index_mapping: &HashMap<String, usize>
) -> Value {
	match property.property_type.as_str() {
		"SEntityTemplateReference" => to_value(convert_qn_reference_to_rt(
			&from_value::<Ref>(property.value.to_owned())
				.expect("Converting RT ref to QN in property value returned error in parsing"),
			factory,
			factory_meta,
			entity_id_to_index_mapping
		))
		.expect("Converting RT ref to QN in property value returned error in serialisation"),

		"ZRuntimeResourceID" => {
			if property.value.is_null() {
				json!({
					"m_IDHigh": 4294967295u32,
					"m_IDLow": 4294967295u32
				})
			} else if property.value.is_string() {
				json!({
					"m_IDHigh": 0, // I doubt we'll ever have that many dependencies
					"m_IDLow": factory_dependencies_index_mapping.get(property.value.as_str().unwrap()).unwrap()
				})
			} else if property.value.is_object() {
				json!({
					"m_IDHigh": 0,
					"m_IDLow": factory_dependencies_index_mapping.get(property.value.get("resource").expect("ZRuntimeResourceID didn't have resource despite being object").as_str().expect("ZRuntimeResourceID resource must be string")).unwrap()
				})
			} else {
				panic!("ZRuntimeResourceID was not of a valid type")
			}
		}

		"SMatrix43" => {
			// this is from three.js

			let obj = property
				.value
				.as_object()
				.expect("SMatrix43 must be object");

			let x = obj
				.get("rotation")
				.unwrap()
				.get("x")
				.unwrap()
				.as_f64()
				.unwrap() * DEG2RAD;
			let y = obj
				.get("rotation")
				.unwrap()
				.get("y")
				.unwrap()
				.as_f64()
				.unwrap() * DEG2RAD;
			let z = obj
				.get("rotation")
				.unwrap()
				.get("z")
				.unwrap()
				.as_f64()
				.unwrap() * DEG2RAD;

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
					.expect("Scale must have x value")
					.as_f64()
					.expect("Scale must be number")
			} else {
				1.0
			};

			let sy = if let Some(scale) = obj.get("scale") {
				scale
					.get("y")
					.expect("Scale must have y value")
					.as_f64()
					.expect("Scale must be number")
			} else {
				1.0
			};

			let sz = if let Some(scale) = obj.get("scale") {
				scale
					.get("z")
					.expect("Scale must have z value")
					.as_f64()
					.expect("Scale must be number")
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
					"x": obj.get("position").unwrap().get("x").unwrap().as_f64().unwrap(),
					"y": obj.get("position").unwrap().get("y").unwrap().as_f64().unwrap(),
					"z": obj.get("position").unwrap().get("z").unwrap().as_f64().unwrap()
				}
			})
		}

		"ZGuid" => json!({
			"_a": i64::from_str_radix(property.value.as_str().unwrap().split('-').next().unwrap(), 16).unwrap(),
			"_b": i64::from_str_radix(property.value.as_str().unwrap().split('-').nth(1).unwrap(), 16).unwrap(),
			"_c": i64::from_str_radix(property.value.as_str().unwrap().split('-').nth(2).unwrap(), 16).unwrap(),
			"_d": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(3).unwrap().chars().skip(0).take(2).collect::<String>(), 16).unwrap(),
			"_e": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(3).unwrap().chars().skip(2).take(2).collect::<String>(), 16).unwrap(),
			"_f": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(0).take(2).collect::<String>(), 16).unwrap(),
			"_g": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(2).take(2).collect::<String>(), 16).unwrap(),
			"_h": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(4).take(2).collect::<String>(), 16).unwrap(),
			"_i": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(6).take(2).collect::<String>(), 16).unwrap(),
			"_j": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(8).take(2).collect::<String>(), 16).unwrap(),
			"_k": i64::from_str_radix(&property.value.as_str().unwrap().split('-').nth(4).unwrap().chars().skip(10).take(2).collect::<String>(), 16).unwrap()
		}),

		"SColorRGB" => json!({
			"r": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(0).take(2).collect::<String>(), 16).unwrap()) / 255.0,
			"g": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(2).take(2).collect::<String>(), 16).unwrap()) / 255.0,
			"b": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(4).take(2).collect::<String>(), 16).unwrap()) / 255.0
		}),

		"SColorRGBA" => json!({
			"r": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(0).take(2).collect::<String>(), 16).unwrap()) / 255.0,
			"g": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(2).take(2).collect::<String>(), 16).unwrap()) / 255.0,
			"b": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(4).take(2).collect::<String>(), 16).unwrap()) / 255.0,
			"a": f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(6).take(2).collect::<String>(), 16).unwrap()) / 255.0
		}),

		_ => property.value.to_owned()
	}
}

#[time_graph::instrument]
fn convert_qn_property_to_rt(
	property_name: &str,
	property_value: &Property,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>,
	factory_dependencies_index_mapping: &HashMap<String, usize>
) -> SEntityTemplateProperty {
	SEntityTemplateProperty {
		n_property_id: convert_string_property_name_to_rt_id(property_name),
		value: SEntityTemplatePropertyValue {
			property_type: property_value.property_type.to_owned(),
			property_value: if property_value.value.is_array() {
				to_value(
					property_value
						.value
						.as_array()
						.unwrap()
						.iter()
						.map(|x| {
							let mut y = property_value.property_type.chars();
							y.nth(6); // discard TArray<
							y.next_back(); // discard closing >

							convert_qn_property_value_to_rt(
								&Property {
									property_type: y.collect(),
									post_init: property_value.post_init,
									value: x.to_owned()
								},
								factory,
								factory_meta,
								entity_id_to_index_mapping,
								factory_dependencies_index_mapping
							)
						})
						.collect::<Vec<Value>>()
				)
				.unwrap()
			} else {
				convert_qn_property_value_to_rt(
					property_value,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping
				)
			}
		}
	}
}

#[time_graph::instrument]
fn convert_string_property_name_to_rt_id(property_name: &str) -> PropertyID {
	if property_name.parse::<u64>().is_ok() && {
		let x = format!("{:x}", property_name.parse::<u64>().unwrap())
			.chars()
			.count();

		x == 8 || x == 7
	} {
		PropertyID::Int(property_name.parse().unwrap())
	} else {
		PropertyID::String(property_name.to_owned())
	}
}

#[time_graph::instrument]
fn get_factory_dependencies(entity: &Entity) -> Vec<ResourceDependency> {
	vec![
		// blueprint first
		vec![ResourceDependency {
			hash: entity.blueprint_hash.to_owned(),
			flag: "1F".to_string()
		}],
		// then external scenes
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceDependency {
				hash: scene.to_owned(),
				flag: "1F".to_string()
			})
			.collect(),
		// then factories of sub-entities
		entity
			.entities
			.iter()
			.collect_vec()
			.par_iter()
			.map(|(_, sub_entity)| ResourceDependency {
				hash: sub_entity.factory.to_owned(),
				flag: sub_entity
					.factory_flag
					.to_owned()
					.unwrap_or_else(|| "1F".to_string()) // this is slightly more efficient
			})
			.collect(),
		// then sub-entity ZRuntimeResourceIDs
		entity
			.entities
			.iter()
			.collect_vec()
			.par_iter()
			.flat_map(|(_, sub_entity)| {
				vec![
					if let Some(props) = &sub_entity.properties {
						vec![
							props
								.iter()
								.filter(|(_, prop)| {
									prop.property_type == "ZRuntimeResourceID"
										&& !prop.value.is_null()
								})
								.map(|(_, prop)| {
									if prop.value.is_string() {
										ResourceDependency {
											hash: prop.value.as_str().unwrap().to_string(),
											flag: "1F".to_string()
										}
									} else {
										ResourceDependency {
											hash: prop
												.value
												.get("resource")
												.expect("ZRuntimeResourceID must have resource")
												.as_str()
												.expect(
													"ZRuntimeResourceID resource must be string"
												)
												.to_string(),
											flag: prop
												.value
												.get("flag")
												.expect("ZRuntimeResourceID must have flag")
												.as_str()
												.expect("ZRuntimeResourceID flag must be string")
												.to_string()
										}
									}
								})
								.collect_vec(),
							props
								.iter()
								.filter(|(_, prop)| {
									prop.property_type == "TArray<ZRuntimeResourceID>"
										&& !prop.value.is_null()
								})
								.map(|(_, prop)| {
									prop.value
										.as_array()
										.expect("TArray<ZRuntimeResourceID> must be array")
										.iter()
										.map(|value| {
											if value.is_string() {
												ResourceDependency {
													hash: value.as_str().unwrap().to_string(),
													flag: "1F".to_string()
												}
											} else {
												ResourceDependency {
													hash: value
														.get("resource")
														.expect("ZRuntimeResourceID must have resource")
														.as_str()
														.expect(
															"ZRuntimeResourceID resource must be string"
														)
														.to_string(),
													flag: value
														.get("flag")
														.expect("ZRuntimeResourceID must have flag")
														.as_str()
														.expect("ZRuntimeResourceID flag must be string")
														.to_string()
												}
											}
										})
										.collect()
								})
								.concat(),
						]
						.concat()
					} else {
						vec![]
					},
					if let Some(platforms) = &sub_entity.platform_specific_properties {
						platforms
							.iter()
							.map(|(_, props)| {
								vec![
									props
										.iter()
										.filter(|(_, prop)| {
											prop.property_type == "ZRuntimeResourceID"
												&& !prop.value.is_null()
										})
										.map(|(_, prop)| {
											if prop.value.is_string() {
												ResourceDependency {
													hash: prop.value.as_str().unwrap().to_string(),
													flag: "1F".to_string()
												}
											} else {
												ResourceDependency {
													hash: prop
														.value
														.get("resource")
														.expect("ZRuntimeResourceID must have resource")
														.as_str()
														.expect(
															"ZRuntimeResourceID resource must be string"
														)
														.to_string(),
													flag: prop
														.value
														.get("flag")
														.expect("ZRuntimeResourceID must have flag")
														.as_str()
														.expect("ZRuntimeResourceID flag must be string")
														.to_string()
												}
											}
										})
										.collect_vec(),
									props
										.iter()
										.filter(|(_, prop)| {
											prop.property_type == "TArray<ZRuntimeResourceID>"
												&& !prop.value.is_null()
										})
										.map(|(_, prop)| {
											prop.value
												.as_array()
												.expect("TArray<ZRuntimeResourceID> must be array")
												.iter()
												.map(|value| {
													if value.is_string() {
														ResourceDependency {
															hash: value
																.as_str()
																.unwrap()
																.to_string(),
															flag: "1F".to_string()
														}
													} else {
														ResourceDependency {
															hash: value
																.get("resource")
																.expect("ZRuntimeResourceID must have resource")
																.as_str()
																.expect(
																	"ZRuntimeResourceID resource must be string"
																)
																.to_string(),
															flag: value
																.get("flag")
																.expect("ZRuntimeResourceID must have flag")
																.as_str()
																.expect("ZRuntimeResourceID flag must be string")
																.to_string()
														}
													}
												})
												.collect()
										})
										.concat(),
								]
								.concat()
							})
							.into_iter()
							.concat()
					} else {
						vec![]
					},
				]
				.into_iter()
				.concat()
			})
			.collect(),
		// then property override ZRuntimeResourceIDs
		entity
			.property_overrides
			.par_iter()
			.flat_map(|PropertyOverride { properties, .. }| {
				vec![
					properties
						.iter()
						.filter(|(_, prop)| {
							prop.property_type == "ZRuntimeResourceID" && !prop.value.is_null()
						})
						.map(|(_, prop)| {
							if prop.value.is_string() {
								ResourceDependency {
									hash: prop.value.as_str().unwrap().to_string(),
									flag: "1F".to_string()
								}
							} else {
								ResourceDependency {
									hash: prop
										.value
										.get("resource")
										.expect("ZRuntimeResourceID must have resource")
										.as_str()
										.expect("ZRuntimeResourceID resource must be string")
										.to_string(),
									flag: prop
										.value
										.get("flag")
										.expect("ZRuntimeResourceID must have flag")
										.as_str()
										.expect("ZRuntimeResourceID flag must be string")
										.to_string()
								}
							}
						})
						.collect_vec(),
					properties
						.iter()
						.filter(|(_, prop)| {
							prop.property_type == "TArray<ZRuntimeResourceID>"
								&& !prop.value.is_null()
						})
						.map(|(_, prop)| {
							prop.value
								.as_array()
								.expect("TArray<ZRuntimeResourceID> must be array")
								.iter()
								.map(|value| {
									if value.is_string() {
										ResourceDependency {
											hash: value.as_str().unwrap().to_string(),
											flag: "1F".to_string()
										}
									} else {
										ResourceDependency {
											hash: value
												.get("resource")
												.expect("ZRuntimeResourceID must have resource")
												.as_str()
												.expect(
													"ZRuntimeResourceID resource must be string"
												)
												.to_string(),
											flag: value
												.get("flag")
												.expect("ZRuntimeResourceID must have flag")
												.as_str()
												.expect("ZRuntimeResourceID flag must be string")
												.to_string()
										}
									}
								})
								.collect()
						})
						.concat(),
				]
				.concat()
			})
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

#[time_graph::instrument]
fn get_blueprint_dependencies(entity: &Entity) -> Vec<ResourceDependency> {
	vec![
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceDependency {
				hash: scene.to_owned(),
				flag: "1F".to_string()
			})
			.collect::<Vec<ResourceDependency>>(),
		entity
			.entities
			.iter()
			.map(|(_, sub_entity)| ResourceDependency {
				hash: sub_entity.blueprint.to_owned(),
				flag: "1F".to_string()
			})
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

#[time_graph::instrument]
pub fn convert_to_qn(
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
	blueprint_meta: &ResourceMeta,
	convert_lossless: bool
) -> Entity {
	if {
		let mut unique = blueprint.sub_entities.to_owned();
		unique.dedup_by_key(|x| x.entity_id);

		unique.len() != blueprint.sub_entities.len()
	} {
		panic!("Cannot convert entity with duplicate IDs");
	}

	let mut entity =
		Entity {
			factory_hash: factory_meta.hash_value.to_owned(),
			blueprint_hash: blueprint_meta.hash_value.to_owned(),
			root_entity: format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(blueprint.root_entity_index)
					.expect("Root entity index referred to nonexistent entity")
					.entity_id
			),
			entities: {
				let vec: Vec<(String, SubEntity)> =
					factory
						.sub_entities
						.par_iter() // rayon automatically makes this run in parallel for s p e e d
						.enumerate()
						.map(|(index, sub_entity_factory)| {
							let sub_entity_blueprint = blueprint
								.sub_entities
								.get(index)
								.expect("Factory entity had no equivalent by index in blueprint");

							let factory_dependency = factory_meta
								.hash_reference_data
								.get(sub_entity_factory.entity_type_resource_index)
								.expect("Entity resource index referred to nonexistent dependency");

							(
						format!("{:x}", sub_entity_blueprint.entity_id),
						SubEntity {
							name: sub_entity_blueprint.entity_name.to_owned(),
							factory: factory_dependency.hash.to_owned(),
							blueprint: blueprint_meta
								.hash_reference_data
								.get(sub_entity_blueprint.entity_type_resource_index)
								.expect("Entity resource index referred to nonexistent dependency")
								.hash
								.to_owned(),
							parent: convert_rt_reference_to_qn(
								&sub_entity_factory.logical_parent,
								factory,
								blueprint,
								factory_meta
							),
							factory_flag: match factory_dependency.flag.as_str() {
								"1F" => None,
								flag => Some(flag.to_owned())
							},
							editor_only: if sub_entity_blueprint.editor_only {
								Some(true)
							} else {
								None
							},
							properties: {
								let x: LinkedHashMap<String, Property> = sub_entity_factory
									.property_values
									.iter()
									.map(|property| {
										(
											match &property.n_property_id {
												PropertyID::Int(id) => id.to_string(),
												PropertyID::String(id) => id.to_owned()
											}, // key
											convert_rt_property_to_qn(
												property,
												false,
												factory,
												factory_meta,
												blueprint,
												convert_lossless
											) // value
										)
									})
									.chain(sub_entity_factory.post_init_property_values.iter().map(
										|property| {
											(
												// we do a little code duplication
												match &property.n_property_id {
													PropertyID::Int(id) => id.to_string(),
													PropertyID::String(id) => id.to_owned()
												},
												convert_rt_property_to_qn(
													property,
													true,
													factory,
													factory_meta,
													blueprint,
													convert_lossless
												)
											)
										}
									))
									.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							platform_specific_properties: {
								// group props by platform, then convert them all and turn into a nested Linkedhashmap structure
								let x: LinkedHashMap<String, LinkedHashMap<String, Property>> =
									sub_entity_factory
										.platform_specific_property_values
										.iter()
										.group_by(|property| property.platform.to_owned())
										.into_iter()
										.map(|(platform, properties)| {
											(
												platform,
												properties
													.map(|property| {
														(
															// we do a little code duplication
															match &property
																.property_value
																.n_property_id
															{
																PropertyID::Int(id) => {
																	id.to_string()
																}
																PropertyID::String(id) => {
																	id.to_owned()
																}
															},
															convert_rt_property_to_qn(
																&property.property_value,
																property.post_init.to_owned(),
																factory,
																factory_meta,
																blueprint,
																convert_lossless
															)
														)
													})
													.collect::<LinkedHashMap<String, Property>>()
											)
										})
										.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							events: None,         // will be mutated later
							input_copying: None,  // will be mutated later
							output_copying: None, // will be mutated later
							property_aliases: {
								let x: LinkedHashMap<String, Vec<PropertyAlias>> = sub_entity_blueprint
									.property_aliases
									.iter()
									.group_by(|alias| alias.s_property_name.to_owned()).into_iter()
									.map(|(property_name, aliases)| {
										(
											property_name,
											aliases.map(|alias| PropertyAlias {
												original_property: alias.s_alias_name.to_owned(),
												original_entity: Ref::Short(Some(format!(
													"{:x}",
													blueprint
														.sub_entities
														.get(alias.entity_id)
														.expect(
															"Property alias referred to nonexistent sub-entity",
														)
														.entity_id
												)))
											}).collect()
										)
									})
									.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							exposed_entities: {
								let x: LinkedHashMap<String, ExposedEntity> = sub_entity_blueprint
									.exposed_entities
									.iter()
									.map(|exposed_entity| {
										(
											exposed_entity.s_name.to_owned(),
											ExposedEntity {
												is_array: exposed_entity.b_is_array.to_owned(),
												refers_to: exposed_entity
													.a_targets
													.iter()
													.map(|target| {
														convert_rt_reference_to_qn(
															target,
															factory,
															blueprint,
															factory_meta
														)
													})
													.collect()
											}
										)
									})
									.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							exposed_interfaces: {
								let x: LinkedHashMap<String, String> = sub_entity_blueprint
									.exposed_interfaces
									.iter()
									.map(|(interface, entity_index)| {
										(
											interface.to_owned(),
											format!(
                                    "{:x}",
                                    blueprint
                                        .sub_entities
                                        .get(*entity_index as usize)
                                        .expect(
                                            "Exposed interface referred to nonexistent sub-entity"
                                        )
                                        .entity_id
                                )
										)
									})
									.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							subsets: None // will be mutated later
						}
					)
						})
						.collect();

				vec.into_iter().collect() // yes this is inefficient, but LinkedHashMap doesn't support rayon collect(), so I have to make it non-parallel first
			},
			external_scenes: factory
				.external_scene_type_indices_in_resource_header
				.par_iter()
				.map(|scene_index| {
					factory_meta
						.hash_reference_data
						.get(*scene_index)
						.unwrap()
						.hash
						.to_owned()
				})
				.collect(),
			override_deletes: blueprint
				.override_deletes
				.par_iter()
				.map(|x| convert_rt_reference_to_qn(x, factory, blueprint, factory_meta))
				.collect(),
			pin_connection_override_deletes: blueprint
				.pin_connection_override_deletes
				.par_iter()
				.map(|x| PinConnectionOverrideDelete {
					from_entity: convert_rt_reference_to_qn(
						&x.from_entity,
						factory,
						blueprint,
						factory_meta
					),
					to_entity: convert_rt_reference_to_qn(
						&x.to_entity,
						factory,
						blueprint,
						factory_meta
					),
					from_pin: x.from_pin_name.to_owned(),
					to_pin: x.to_pin_name.to_owned(),
					value: match x.constant_pin_value.property_type.as_str() {
						"void" => None,
						_ => Some(SimpleProperty {
							property_type: x.constant_pin_value.property_type.to_owned(),
							value: x.constant_pin_value.property_value.to_owned()
						})
					}
				})
				.collect(),
			pin_connection_overrides: blueprint
				.pin_connection_overrides
				.par_iter()
				.filter(|x| x.from_entity.external_scene_index != -1)
				.map(|x| PinConnectionOverride {
					from_entity: convert_rt_reference_to_qn(
						&x.from_entity,
						factory,
						blueprint,
						factory_meta
					),
					to_entity: convert_rt_reference_to_qn(
						&x.to_entity,
						factory,
						blueprint,
						factory_meta
					),
					from_pin: x.from_pin_name.to_owned(),
					to_pin: x.to_pin_name.to_owned(),
					value: match x.constant_pin_value.property_type.as_str() {
						"void" => None,
						_ => Some(SimpleProperty {
							property_type: x.constant_pin_value.property_type.to_owned(),
							value: x.constant_pin_value.property_value.to_owned()
						})
					}
				})
				.collect(),
			property_overrides: vec![],
			sub_type: match blueprint.sub_type {
				2 => SubType::Brick,
				1 => SubType::Scene,
				0 => SubType::Template,
				_ => panic!("Invalid subtype")
			},
			quick_entity_version: 3.1,
			extra_factory_dependencies: vec![],
			extra_blueprint_dependencies: vec![],
			comments: vec![]
		};

	{
		let depends = get_factory_dependencies(&entity);

		entity.extra_factory_dependencies = factory_meta
			.hash_reference_data
			.iter()
			.filter(|x| {
				if x.hash.contains(':') {
					!depends.contains(&ResourceDependency {
						hash: format!(
							"00{}",
							format!("{:X}", md5::compute(&x.hash))
								.chars()
								.skip(2)
								.take(14)
								.collect::<String>()
						),
						flag: x.flag.to_owned()
					}) && !depends.contains(x)
				} else {
					!depends.contains(x)
				}
			})
			.map(|x| match x {
				ResourceDependency { hash, flag } if flag == "1F" => {
					Dependency::Short(hash.to_owned())
				}
				ResourceDependency { hash, flag } => Dependency::Full(DependencyWithFlag {
					resource: hash.to_owned(),
					flag: flag.to_owned()
				})
			})
			.collect();
	}

	{
		let depends = get_blueprint_dependencies(&entity);

		entity.extra_blueprint_dependencies = blueprint_meta
			.hash_reference_data
			.iter()
			.filter(|x| {
				if x.hash.contains(':') {
					!depends.contains(&ResourceDependency {
						hash: format!(
							"00{}",
							format!("{:X}", md5::compute(&x.hash))
								.chars()
								.skip(2)
								.take(14)
								.collect::<String>()
						),
						flag: x.flag.to_owned()
					}) && !depends.contains(x)
				} else {
					!depends.contains(x)
				}
			})
			.map(|x| match x {
				ResourceDependency { hash, flag } if flag == "1F" => {
					Dependency::Short(hash.to_owned())
				}
				ResourceDependency { hash, flag } => Dependency::Full(DependencyWithFlag {
					resource: hash.to_owned(),
					flag: flag.to_owned()
				})
			})
			.collect();
	}

	for pin in &blueprint.pin_connections {
		let mut relevant_sub_entity = entity
			.entities
			.get_mut(&format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(pin.from_id as usize)
					.expect("Pin referred to nonexistent sub-entity")
					.entity_id
			))
			.unwrap();

		if relevant_sub_entity.events.is_none() {
			relevant_sub_entity.events = Some(LinkedHashMap::new());
		}

		relevant_sub_entity
			.events
			.as_mut()
			.unwrap()
			.entry(pin.from_pin_name.to_owned())
			.or_insert(LinkedHashMap::default())
			.entry(pin.to_pin_name.to_owned())
			.or_insert(Vec::default())
			.push(if pin.constant_pin_value.property_type == "void" {
				RefMaybeConstantValue::Ref(Ref::Short(Some(format!(
					"{:x}",
					blueprint
						.sub_entities
						.get(pin.to_id as usize)
						.expect("Pin referred to nonexistent sub-entity")
						.entity_id
				))))
			} else {
				RefMaybeConstantValue::RefWithConstantValue(RefWithConstantValue {
					entity_ref: Ref::Short(Some(format!(
						"{:x}",
						blueprint
							.sub_entities
							.get(pin.to_id as usize)
							.expect("Pin referred to nonexistent sub-entity")
							.entity_id
					))),
					value: SimpleProperty {
						property_type: pin.constant_pin_value.property_type.to_owned(),
						value: pin.constant_pin_value.property_value.to_owned()
					}
				})
			});
	}

	for pin_connection_override in blueprint
		.pin_connection_overrides
		.iter()
		.filter(|x| x.from_entity.external_scene_index == -1)
	{
		let relevant_sub_entity = entity
			.entities
			.get_mut(&format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(pin_connection_override.from_entity.entity_index as usize)
					.expect("Pin connection override referred to nonexistent sub-entity")
					.entity_id
			))
			.unwrap();

		if relevant_sub_entity.events.is_none() {
			relevant_sub_entity.events = Some(LinkedHashMap::new());
		}

		relevant_sub_entity
			.events
			.as_mut()
			.unwrap()
			.entry(pin_connection_override.from_pin_name.to_owned())
			.or_insert(LinkedHashMap::default())
			.entry(pin_connection_override.to_pin_name.to_owned())
			.or_insert(Vec::default())
			.push(
				if pin_connection_override.constant_pin_value.property_type == "void" {
					RefMaybeConstantValue::Ref(convert_rt_reference_to_qn(
						&pin_connection_override.to_entity,
						factory,
						blueprint,
						factory_meta
					))
				} else {
					RefMaybeConstantValue::RefWithConstantValue(RefWithConstantValue {
						entity_ref: convert_rt_reference_to_qn(
							&pin_connection_override.to_entity,
							factory,
							blueprint,
							factory_meta
						),
						value: SimpleProperty {
							property_type: pin_connection_override
								.constant_pin_value
								.property_type
								.to_owned(),
							value: pin_connection_override
								.constant_pin_value
								.property_value
								.to_owned()
						}
					})
				}
			);
	}

	// cheeky bit of code duplication right here
	for forwarding in &blueprint.input_pin_forwardings {
		let mut relevant_sub_entity = entity
			.entities
			.get_mut(&format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(forwarding.from_id as usize)
					.expect("Pin referred to nonexistent sub-entity")
					.entity_id
			))
			.unwrap();

		if relevant_sub_entity.input_copying.is_none() {
			relevant_sub_entity.input_copying = Some(LinkedHashMap::new());
		}

		relevant_sub_entity
			.input_copying
			.as_mut()
			.unwrap()
			.entry(forwarding.from_pin_name.to_owned())
			.or_insert(LinkedHashMap::default())
			.entry(forwarding.to_pin_name.to_owned())
			.or_insert(Vec::default())
			.push(if forwarding.constant_pin_value.property_type == "void" {
				RefMaybeConstantValue::Ref(Ref::Short(Some(format!(
					"{:x}",
					blueprint
						.sub_entities
						.get(forwarding.to_id as usize)
						.expect("Pin referred to nonexistent sub-entity")
						.entity_id
				))))
			} else {
				RefMaybeConstantValue::RefWithConstantValue(RefWithConstantValue {
					entity_ref: Ref::Short(Some(format!(
						"{:x}",
						blueprint
							.sub_entities
							.get(forwarding.to_id as usize)
							.expect("Pin referred to nonexistent sub-entity")
							.entity_id
					))),
					value: SimpleProperty {
						property_type: forwarding.constant_pin_value.property_type.to_owned(),
						value: forwarding.constant_pin_value.property_value.to_owned()
					}
				})
			});
	}

	for forwarding in &blueprint.output_pin_forwardings {
		let mut relevant_sub_entity = entity
			.entities
			.get_mut(&format!(
				"{:x}",
				blueprint
					.sub_entities
					.get(forwarding.from_id as usize)
					.expect("Pin referred to nonexistent sub-entity")
					.entity_id
			))
			.unwrap();

		if relevant_sub_entity.output_copying.is_none() {
			relevant_sub_entity.output_copying = Some(LinkedHashMap::new());
		}

		relevant_sub_entity
			.output_copying
			.as_mut()
			.unwrap()
			.entry(forwarding.from_pin_name.to_owned())
			.or_insert(LinkedHashMap::default())
			.entry(forwarding.to_pin_name.to_owned())
			.or_insert(Vec::default())
			.push(if forwarding.constant_pin_value.property_type == "void" {
				RefMaybeConstantValue::Ref(Ref::Short(Some(format!(
					"{:x}",
					blueprint
						.sub_entities
						.get(forwarding.to_id as usize)
						.expect("Pin referred to nonexistent sub-entity")
						.entity_id
				))))
			} else {
				RefMaybeConstantValue::RefWithConstantValue(RefWithConstantValue {
					entity_ref: Ref::Short(Some(format!(
						"{:x}",
						blueprint
							.sub_entities
							.get(forwarding.to_id as usize)
							.expect("Pin referred to nonexistent sub-entity")
							.entity_id
					))),
					value: SimpleProperty {
						property_type: forwarding.constant_pin_value.property_type.to_owned(),
						value: forwarding.constant_pin_value.property_value.to_owned()
					}
				})
			});
	}

	for sub_entity in &blueprint.sub_entities {
		sub_entity.entity_subsets.iter().for_each(|(subset, data)| {
			data.entities.iter().for_each(|subset_entity| {
				let mut relevant_qn = entity
					.entities
					.get_mut(&format!(
						"{:x}",
						blueprint
							.sub_entities
							.get(*subset_entity as usize)
							.expect("Entity subset referred to nonexistent sub-entity")
							.entity_id
					))
					.unwrap();

				if relevant_qn.subsets.is_none() {
					relevant_qn.subsets = Some(LinkedHashMap::new());
				}

				relevant_qn
					.subsets
					.as_mut()
					.unwrap()
					.entry(subset.to_owned())
					.or_insert(Vec::default())
					.push(format!("{:x}", sub_entity.entity_id));
			});
		});
	}

	// to clean up pass1
	{
		let mut pass1: Vec<PropertyOverride> = Vec::default();

		for property_override in &factory.property_overrides {
			let ents = vec![convert_rt_reference_to_qn(
				&property_override.property_owner,
				factory,
				blueprint,
				factory_meta
			)];

			let props = [(
				match &property_override.property_value.n_property_id {
					PropertyID::Int(id) => id.to_string(),
					PropertyID::String(id) => id.to_owned()
				},
				{
					let prop = convert_rt_property_to_qn(
						&property_override.property_value,
						false,
						factory,
						factory_meta,
						blueprint,
						convert_lossless
					);

					OverriddenProperty {
						value: prop.value,
						property_type: prop.property_type
					} // no post-init
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
	}

	entity
}

#[time_graph::instrument]
pub fn convert_to_rt(entity: &Entity) -> (RTFactory, ResourceMeta, RTBlueprint, ResourceMeta) {
	let entity_id_to_index_mapping: HashMap<String, usize> = entity
		.entities
		.keys()
		.enumerate()
		.map(|(x, y)| (y.to_owned(), x))
		.collect();

	let mut factory = RTFactory {
		sub_type: match entity.sub_type {
			SubType::Brick => 2,
			SubType::Scene => 1,
			SubType::Template => 0
		},
		blueprint_index_in_resource_header: 0,
		root_entity_index: *entity_id_to_index_mapping
			.get(&entity.root_entity)
			.expect("Root entity was non-existent"),
		sub_entities: vec![],
		property_overrides: vec![],
		external_scene_type_indices_in_resource_header: (1..entity.external_scenes.len() + 1)
			.collect()
	};

	let factory_meta = ResourceMeta {
		hash_offset: 1367, // none of this data actually matters except for dependencies and resource type
		hash_reference_data: vec![
			get_factory_dependencies(entity),
			entity
				.extra_factory_dependencies
				.iter()
				.map(|x| match x {
					Dependency::Short(hash) => ResourceDependency {
						hash: hash.to_owned(),
						flag: "1F".to_string()
					},
					Dependency::Full(DependencyWithFlag { resource, flag }) => ResourceDependency {
						hash: resource.to_owned(),
						flag: flag.to_owned()
					}
				})
				.collect(),
		]
		.concat(),
		hash_reference_table_dummy: 0,
		hash_reference_table_size: 193,
		hash_resource_type: "TEMP".to_string(),
		hash_size: 2147484657,
		hash_size_final: 2377,
		hash_size_in_memory: 1525,
		hash_size_in_video_memory: 4294967295,
		hash_value: entity.factory_hash.to_owned()
	};

	let mut blueprint = RTBlueprint {
		sub_type: match entity.sub_type {
			SubType::Brick => 2,
			SubType::Scene => 1,
			SubType::Template => 0
		},
		root_entity_index: *entity_id_to_index_mapping
			.get(&entity.root_entity)
			.expect("Root entity was non-existent"),
		sub_entities: vec![],
		pin_connections: vec![],
		input_pin_forwardings: vec![],
		output_pin_forwardings: vec![],
		override_deletes: entity
			.override_deletes
			.par_iter()
			.map(|override_delete| {
				convert_qn_reference_to_rt(
					override_delete,
					&factory,
					&factory_meta,
					&entity_id_to_index_mapping
				)
			})
			.collect(),
		pin_connection_overrides: vec![
			entity
				.pin_connection_overrides
				.par_iter()
				.map(
					|pin_connection_override| SExternalEntityTemplatePinConnection {
						from_entity: convert_qn_reference_to_rt(
							&pin_connection_override.from_entity,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						),
						to_entity: convert_qn_reference_to_rt(
							&pin_connection_override.to_entity,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping
						),
						from_pin_name: pin_connection_override.from_pin.to_owned(),
						to_pin_name: pin_connection_override.to_pin.to_owned(),
						constant_pin_value: {
							let x = pin_connection_override.value.as_ref();
							let default = SimpleProperty {
								property_type: "void".to_string(),
								value: Value::Null
							};
							let y = x.unwrap_or(&default);

							SEntityTemplatePropertyValue {
								property_type: y.property_type.to_owned(),
								property_value: y.value.to_owned()
							}
						}
					}
				)
				.collect::<Vec<SExternalEntityTemplatePinConnection>>(),
			entity
				.entities
				.iter()
				.collect_vec()
				.par_iter()
				.flat_map(|(entity_id, sub_entity)| {
					if sub_entity.events.is_some() {
						sub_entity
							.events
							.as_ref()
							.unwrap()
							.iter()
							.flat_map(|(event, pin)| {
								pin.iter()
									.flat_map(|(trigger, entities)| {
										entities
											.iter()
											.filter(|trigger_entity| {
												matches!(
											trigger_entity,
											RefMaybeConstantValue::Ref(Ref::Full(_))
												| RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Full(_),
														value: _
													}
												)
										)
											})
											.map(|trigger_entity| {
												SExternalEntityTemplatePinConnection {
													from_entity: convert_qn_reference_to_rt(&Ref::Short(Some(entity_id.to_owned().to_owned())), &factory, &factory_meta, &entity_id_to_index_mapping),
													to_entity: convert_qn_reference_to_rt(match &trigger_entity {
															RefMaybeConstantValue::Ref(entity_ref) => entity_ref,

															RefMaybeConstantValue::RefWithConstantValue(
																RefWithConstantValue {
																	entity_ref,
																	value: _
																}
															) => entity_ref
														}, &factory, &factory_meta, &entity_id_to_index_mapping),
													from_pin_name: event.to_owned(),
													to_pin_name: trigger.to_owned(),
													constant_pin_value: match &trigger_entity {
														RefMaybeConstantValue::RefWithConstantValue(
															RefWithConstantValue {
																entity_ref: _,
																value
															}
														) => SEntityTemplatePropertyValue {
															property_type: value.property_type.to_owned(),
															property_value: value.value.to_owned()
														},

														_ => SEntityTemplatePropertyValue {
															property_type: "void".to_owned(),
															property_value: Value::Null
														}
													}
												}
											})
											.collect_vec()
									})
									.collect_vec()
							})
							.collect()
					} else {
						vec![]
					}
				})
				.collect::<Vec<SExternalEntityTemplatePinConnection>>(),
		]
		.concat(),
		pin_connection_override_deletes: entity
			.pin_connection_override_deletes
			.par_iter()
			.map(
				|pin_connection_override_delete| SExternalEntityTemplatePinConnection {
					from_entity: convert_qn_reference_to_rt(
						&pin_connection_override_delete.from_entity,
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					),
					to_entity: convert_qn_reference_to_rt(
						&pin_connection_override_delete.to_entity,
						&factory,
						&factory_meta,
						&entity_id_to_index_mapping
					),
					from_pin_name: pin_connection_override_delete.from_pin.to_owned(),
					to_pin_name: pin_connection_override_delete.to_pin.to_owned(),
					constant_pin_value: {
						let x = pin_connection_override_delete.value.as_ref();
						let default = SimpleProperty {
							property_type: "void".to_string(),
							value: Value::Null
						};
						let y = x.unwrap_or(&default);

						SEntityTemplatePropertyValue {
							property_type: y.property_type.to_owned(),
							property_value: y.value.to_owned()
						}
					}
				}
			)
			.collect(),
		external_scene_type_indices_in_resource_header: (0..entity.external_scenes.len()).collect()
	};

	let blueprint_meta = ResourceMeta {
		hash_offset: 1367,
		hash_reference_data: vec![
			get_blueprint_dependencies(entity),
			entity
				.extra_blueprint_dependencies
				.iter()
				.map(|x| match x {
					Dependency::Short(hash) => ResourceDependency {
						hash: hash.to_owned(),
						flag: "1F".to_string()
					},
					Dependency::Full(DependencyWithFlag { resource, flag }) => ResourceDependency {
						hash: resource.to_owned(),
						flag: flag.to_owned()
					}
				})
				.collect(),
		]
		.concat(),
		hash_reference_table_dummy: 0,
		hash_reference_table_size: 193,
		hash_resource_type: "TBLU".to_string(),
		hash_size: 2147484657,
		hash_size_final: 2377,
		hash_size_in_memory: 1525,
		hash_size_in_video_memory: 4294967295,
		hash_value: entity.blueprint_hash.to_owned()
	};

	let factory_dependencies_index_mapping: HashMap<String, usize> = factory_meta
		.hash_reference_data
		.par_iter()
		.enumerate()
		.map(|(x, y)| (y.hash.to_owned(), x.to_owned()))
		.collect();

	let blueprint_dependencies_index_mapping: HashMap<String, usize> = blueprint_meta
		.hash_reference_data
		.par_iter()
		.enumerate()
		.map(|(x, y)| (y.hash.to_owned(), x.to_owned()))
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
						.map(|(property, overridden)| SEntityTemplatePropertyOverride {
							property_owner: convert_qn_reference_to_rt(
								ext_entity,
								&factory,
								&factory_meta,
								&entity_id_to_index_mapping
							),
							property_value: SEntityTemplateProperty {
								n_property_id: convert_string_property_name_to_rt_id(property),
								value: SEntityTemplatePropertyValue {
									property_type: overridden.property_type.to_owned(),
									property_value: to_value(
										convert_qn_property_to_rt(
											property,
											&Property {
												property_type: overridden.property_type.to_owned(),
												value: overridden.value.to_owned(),
												post_init: None
											},
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping,
											&factory_dependencies_index_mapping
										)
										.value
										.property_value
									)
									.unwrap()
								}
							}
						})
						.collect_vec()
				})
				.collect_vec()
		})
		.collect();

	factory.sub_entities = entity
		.entities
		.iter()
		.collect_vec()
		.par_iter()
		.map(|(_, sub_entity)| STemplateFactorySubEntity {
			logical_parent: convert_qn_reference_to_rt(
				&sub_entity.parent,
				&factory,
				&factory_meta,
				&entity_id_to_index_mapping
			),
			entity_type_resource_index: *factory_dependencies_index_mapping
				.get(&sub_entity.factory)
				.unwrap(),
			property_values: if let Some(props) = sub_entity.properties.to_owned() {
				props
					.iter()
					.filter(|(_, x)| !x.post_init.unwrap_or(false))
					.map(|(x, y)| {
						convert_qn_property_to_rt(
							x,
							y,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping,
							&factory_dependencies_index_mapping
						)
					})
					.collect()
			} else {
				vec![]
			},
			post_init_property_values: if let Some(props) = sub_entity.properties.to_owned() {
				props
					.iter()
					.filter(|(_, y)| y.post_init.unwrap_or(false))
					.map(|(x, y)| {
						convert_qn_property_to_rt(
							x,
							y,
							&factory,
							&factory_meta,
							&entity_id_to_index_mapping,
							&factory_dependencies_index_mapping
						)
					})
					.collect()
			} else {
				vec![]
			},
			platform_specific_property_values: if let Some(p_s_props) =
				sub_entity.platform_specific_properties.to_owned()
			{
				p_s_props
					.iter()
					.flat_map(|(platform, props)| {
						props
							.iter()
							.map(|(x, y)| SEntityTemplatePlatformSpecificProperty {
								platform: platform.to_owned(),
								post_init: y.post_init.unwrap_or(false),
								property_value: convert_qn_property_to_rt(
									x,
									y,
									&factory,
									&factory_meta,
									&entity_id_to_index_mapping,
									&factory_dependencies_index_mapping
								)
							})
							.collect::<Vec<SEntityTemplatePlatformSpecificProperty>>()
					})
					.collect()
			} else {
				vec![]
			}
		})
		.collect();

	blueprint.sub_entities = entity
		.entities
		.iter()
		.collect_vec()
		.par_iter()
		.map(|(entity_id, sub_entity)| STemplateBlueprintSubEntity {
			logical_parent: convert_qn_reference_to_rt(
				&sub_entity.parent,
				&factory,
				&factory_meta,
				&entity_id_to_index_mapping
			),
			entity_type_resource_index: *blueprint_dependencies_index_mapping
				.get(&sub_entity.blueprint)
				.unwrap(),
			entity_id: u64::from_str_radix(entity_id, 16).expect("entity_id must be valid hex"),
			editor_only: sub_entity.editor_only.unwrap_or(false),
			entity_name: sub_entity.name.to_owned(),
			property_aliases: if sub_entity.property_aliases.is_some() {
				sub_entity
					.property_aliases
					.as_ref()
					.unwrap()
					.iter()
					.flat_map(|(aliased_name, aliases)| {
						aliases.iter().map(|alias| {
							SEntityTemplatePropertyAlias {
								entity_id: match &alias.original_entity {
									Ref::Short(r) => match r {
										Some(r) => entity_id_to_index_mapping.get(r).expect(
											"Property alias short ref referred to nonexistent entity ID"
										).to_owned(),

										_ => panic!("Null references are not permitted in property aliases")
									},

									_ => panic!(
										"External references are not permitted in property aliases"
									)
								},
								s_alias_name: alias.original_property.to_owned(),
								s_property_name: aliased_name.to_owned()
							}
						})
					})
					.collect()
			} else {
				vec![]
			},
			exposed_entities: if sub_entity.exposed_entities.is_some() {
				sub_entity
					.exposed_entities
					.as_ref()
					.unwrap()
					.iter()
					.map(
						|(exposed_name, exposed_entity)| SEntityTemplateExposedEntity {
							s_name: exposed_name.to_owned(),
							b_is_array: exposed_entity.is_array,
							a_targets: exposed_entity
								.refers_to
								.iter()
								.map(|target| {
									convert_qn_reference_to_rt(
										target,
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping
									)
								})
								.collect()
						}
					)
					.collect()
			} else {
				vec![]
			},
			exposed_interfaces: if sub_entity.exposed_interfaces.is_some() {
				sub_entity
					.exposed_interfaces
					.as_ref()
					.unwrap()
					.iter()
					.map(|(interface, implementor)| {
						(
							interface.to_owned(),
							entity_id_to_index_mapping
								.get(implementor)
								.expect("Exposed interface referenced nonexistent local entity")
								.to_owned()
						)
					})
					.collect()
			} else {
				vec![]
			},
			entity_subsets: vec![] // will be mutated later
		})
		.collect();

	for (entity_index, (_, sub_entity)) in entity.entities.iter().enumerate() {
		if sub_entity.subsets.is_some() {
			for (subset, ents) in sub_entity.subsets.as_ref().unwrap().iter() {
				for ent in ents.iter() {
					let ent_subs = &mut blueprint
						.sub_entities
						.get_mut(
							*entity_id_to_index_mapping
								.get(ent)
								.expect("Entity subset referenced nonexistent local entity")
						)
						.unwrap()
						.entity_subsets;

					if let Some((_, subset_entities)) =
						ent_subs.iter_mut().find(|(s, _)| s == subset)
					{
						subset_entities.entities.push(entity_index);
					} else {
						ent_subs.push((
							subset.to_owned(),
							SEntityTemplateEntitySubset {
								entities: vec![entity_index]
							}
						));
					};
				}
			}
		}
	}

	blueprint.pin_connections = entity
		.entities
		.iter()
		.collect_vec()
		.par_iter()
		.flat_map(|(entity_id, sub_entity)| {
			if sub_entity.events.is_some() {
				sub_entity
					.events
					.as_ref()
					.unwrap()
					.iter()
					.flat_map(|(event, pin)| {
						pin.iter()
							.flat_map(|(trigger, entities)| {
								entities
									.iter()
									.filter(|trigger_entity| {
										matches!(
											trigger_entity,
											RefMaybeConstantValue::Ref(Ref::Short(_))
												| RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(_),
														value: _
													}
												)
										)
									})
									.map(|trigger_entity| SEntityTemplatePinConnection {
										from_id: *entity_id_to_index_mapping
											.get(*entity_id)
											.unwrap(),
										to_id: *entity_id_to_index_mapping
											.get(match &trigger_entity {
												RefMaybeConstantValue::Ref(Ref::Short(Some(
													id
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _
													}
												) => id,

												_ => panic!("Invalid to_id for trigger on events")
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value
												}
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned()
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null
											}
										}
									})
									.collect_vec()
							})
							.collect_vec()
					})
					.collect()
			} else {
				vec![]
			}
		})
		.collect();

	// more code duplication
	blueprint.input_pin_forwardings = entity
		.entities
		.iter()
		.collect_vec()
		.par_iter()
		.flat_map(|(entity_id, sub_entity)| {
			if sub_entity.input_copying.is_some() {
				sub_entity
					.input_copying
					.as_ref()
					.unwrap()
					.iter()
					.flat_map(|(event, pin)| {
						pin.iter()
							.flat_map(|(trigger, entities)| {
								entities
									.iter()
									.filter(|trigger_entity| {
										matches!(
											trigger_entity,
											RefMaybeConstantValue::Ref(Ref::Short(_))
												| RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(_),
														value: _
													}
												)
										)
									})
									.map(|trigger_entity| SEntityTemplatePinConnection {
										from_id: *entity_id_to_index_mapping
											.get(*entity_id)
											.unwrap(),
										to_id: *entity_id_to_index_mapping
											.get(match &trigger_entity {
												RefMaybeConstantValue::Ref(Ref::Short(Some(
													id
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _
													}
												) => id,

												_ => panic!(
													"Invalid to_id for trigger on input copying"
												)
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value
												}
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned()
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null
											}
										}
									})
									.collect_vec()
							})
							.collect_vec()
					})
					.collect()
			} else {
				vec![]
			}
		})
		.collect();

	blueprint.output_pin_forwardings = entity
		.entities
		.iter()
		.collect_vec()
		.par_iter()
		.flat_map(|(entity_id, sub_entity)| {
			if sub_entity.output_copying.is_some() {
				sub_entity
					.output_copying
					.as_ref()
					.unwrap()
					.iter()
					.flat_map(|(event, pin)| {
						pin.iter()
							.flat_map(|(trigger, entities)| {
								entities
									.iter()
									.filter(|trigger_entity| {
										matches!(
											trigger_entity,
											RefMaybeConstantValue::Ref(Ref::Short(_))
												| RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(_),
														value: _
													}
												)
										)
									})
									.map(|trigger_entity| SEntityTemplatePinConnection {
										from_id: *entity_id_to_index_mapping
											.get(*entity_id)
											.unwrap(),
										to_id: *entity_id_to_index_mapping
											.get(match &trigger_entity {
												RefMaybeConstantValue::Ref(Ref::Short(Some(
													id
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _
													}
												) => id,

												_ => panic!(
													"Invalid to_id for trigger on output copying"
												)
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value
												}
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned()
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null
											}
										}
									})
									.collect_vec()
							})
							.collect_vec()
					})
					.collect()
			} else {
				vec![]
			}
		})
		.collect();

	(factory, factory_meta, blueprint, blueprint_meta)
}

pub fn convert_2016_factory_to_modern(factory: &RTFactory2016) -> RTFactory {
	RTFactory {
		sub_type: factory.sub_type,
		blueprint_index_in_resource_header: factory.blueprint_index_in_resource_header,
		root_entity_index: factory.root_entity_index,
		sub_entities: factory
			.entity_templates
			.iter()
			.map(|x| STemplateFactorySubEntity {
				entity_type_resource_index: x.entity_type_resource_index,
				logical_parent: x.logical_parent.to_owned(),
				platform_specific_property_values: Vec::with_capacity(0),
				property_values: x.property_values.to_owned(),
				post_init_property_values: x.post_init_property_values.to_owned()
			})
			.collect(),
		property_overrides: factory.property_overrides.to_owned(),
		external_scene_type_indices_in_resource_header: factory
			.external_scene_type_indices_in_resource_header
			.to_owned()
	}
}

pub fn convert_modern_factory_to_2016(factory: &RTFactory) -> RTFactory2016 {
	RTFactory2016 {
		sub_type: factory.sub_type,
		blueprint_index_in_resource_header: factory.blueprint_index_in_resource_header,
		root_entity_index: factory.root_entity_index,
		entity_templates: factory
			.sub_entities
			.iter()
			.map(|x| STemplateSubEntity {
				entity_type_resource_index: x.entity_type_resource_index,
				logical_parent: x.logical_parent.to_owned(),
				property_values: x.property_values.to_owned(),
				post_init_property_values: x.post_init_property_values.to_owned()
			})
			.collect(),
		property_overrides: factory.property_overrides.to_owned(),
		external_scene_type_indices_in_resource_header: factory
			.external_scene_type_indices_in_resource_header
			.to_owned()
	}
}

pub fn convert_2016_blueprint_to_modern(blueprint: &RTBlueprint2016) -> RTBlueprint {
	RTBlueprint {
		sub_type: blueprint.sub_type,
		root_entity_index: blueprint.root_entity_index,
		sub_entities: blueprint
			.entity_templates
			.iter()
			.map(|x| STemplateBlueprintSubEntity {
				entity_id: x.entity_id,
				editor_only: false,
				entity_name: x.entity_name.to_owned(),
				entity_subsets: x.entity_subsets.to_owned(),
				entity_type_resource_index: x.entity_type_resource_index,
				exposed_entities: x
					.exposed_entities
					.iter()
					.map(|(x, y)| SEntityTemplateExposedEntity {
						b_is_array: false,
						a_targets: vec![y.to_owned()],
						s_name: x.to_owned()
					})
					.collect(),
				exposed_interfaces: x.exposed_interfaces.to_owned(),
				logical_parent: x.logical_parent.to_owned(),
				property_aliases: x.property_aliases.to_owned()
			})
			.collect(),
		external_scene_type_indices_in_resource_header: blueprint
			.external_scene_type_indices_in_resource_header
			.to_owned(),
		pin_connections: blueprint
			.pin_connections
			.iter()
			.map(|x| SEntityTemplatePinConnection {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned(),
				constant_pin_value: SEntityTemplatePropertyValue {
					property_type: "void".to_string(),
					property_value: Value::Null
				}
			})
			.collect(),
		input_pin_forwardings: blueprint
			.input_pin_forwardings
			.iter()
			.map(|x| SEntityTemplatePinConnection {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned(),
				constant_pin_value: SEntityTemplatePropertyValue {
					property_type: "void".to_string(),
					property_value: Value::Null
				}
			})
			.collect(),
		output_pin_forwardings: blueprint
			.output_pin_forwardings
			.iter()
			.map(|x| SEntityTemplatePinConnection {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned(),
				constant_pin_value: SEntityTemplatePropertyValue {
					property_type: "void".to_string(),
					property_value: Value::Null
				}
			})
			.collect(),
		override_deletes: blueprint.override_deletes.to_owned(),
		pin_connection_overrides: Vec::with_capacity(0),
		pin_connection_override_deletes: Vec::with_capacity(0)
	}
}

pub fn convert_modern_blueprint_to_2016(blueprint: &RTBlueprint) -> RTBlueprint2016 {
	RTBlueprint2016 {
		sub_type: blueprint.sub_type,
		root_entity_index: blueprint.root_entity_index,
		entity_templates: blueprint
			.sub_entities
			.iter()
			.map(|x| STemplateSubEntityBlueprint {
				entity_id: x.entity_id,
				entity_name: x.entity_name.to_owned(),
				entity_subsets: x.entity_subsets.to_owned(),
				entity_type_resource_index: x.entity_type_resource_index,
				exposed_entities: x
					.exposed_entities
					.iter()
					.map(|x| (x.s_name.to_owned(), x.a_targets[0].to_owned()))
					.collect(),
				exposed_interfaces: x.exposed_interfaces.to_owned(),
				logical_parent: x.logical_parent.to_owned(),
				property_aliases: x.property_aliases.to_owned()
			})
			.collect(),
		external_scene_type_indices_in_resource_header: blueprint
			.external_scene_type_indices_in_resource_header
			.to_owned(),
		pin_connections: blueprint
			.pin_connections
			.iter()
			.map(|x| SEntityTemplatePinConnection2016 {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned()
			})
			.collect(),
		input_pin_forwardings: blueprint
			.input_pin_forwardings
			.iter()
			.map(|x| SEntityTemplatePinConnection2016 {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned()
			})
			.collect(),
		output_pin_forwardings: blueprint
			.output_pin_forwardings
			.iter()
			.map(|x| SEntityTemplatePinConnection2016 {
				from_id: x.from_id,
				from_pin_name: x.from_pin_name.to_owned(),
				to_id: x.to_id,
				to_pin_name: x.to_pin_name.to_owned()
			})
			.collect(),
		override_deletes: blueprint.override_deletes.to_owned()
	}
}

use linked_hash_map::LinkedHashMap;
use std::collections::HashMap;

use itertools::Itertools;
use json_patch::{diff, from_value as json_patch_from_value, patch as apply_rfc_patch};
use rayon::prelude::*;
use serde_json::{from_value, json, to_value, Value};
use std::fmt::Write;

use crate::{
	qn_structs::{
		Entity, ExposedEntity, FullRef, OverriddenProperty, PinConnectionOverride,
		PinConnectionOverrideDelete, Property, PropertyAlias, PropertyOverride, Ref,
		RefMaybeConstantValue, RefWithConstantValue, SimpleProperty, SubEntity, SubType,
	},
	rpkg_structs::ResourceMeta,
	rt_structs::{
		PropertyID, RTBlueprint, RTFactory, SEntityTemplateProperty, SEntityTemplatePropertyValue,
		SEntityTemplateReference,
	},
	util_structs::{SMatrix43PropertyValue, ZGuidPropertyValue, ZRuntimeResourceIDPropertyValue},
};

const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;

pub enum Game {
	HM1,
	HM2,
	HM3,
}

pub fn apply_patch(entity: &mut Value, patch: &Value) {
	apply_rfc_patch(
		entity,
		&json_patch_from_value(
			patch
				.get("patch")
				.expect("Failed to get patch from file")
				.to_owned(),
		)
		.expect("Failed to convert patch to RFC6902"),
	)
	.expect("Failed to apply patch");
}

pub fn generate_patch(original: &Value, modified: &Value) -> Value {
	let mut rfcpatch = json!(diff(original, modified));

	if let Some(pos) =
		rfcpatch
			.as_array_mut()
			.unwrap()
			.iter()
			.position(|value| match value.get("path") {
				Some(path) => path == "/quickEntityVersion",
				_ => false,
			}) {
		rfcpatch.as_array_mut().unwrap().remove(pos);
	}

	json!({
		"tempHash": modified.get("tempHash").expect("Failed to get tempHash"),
		"tbluHash": modified.get("tbluHash").expect("Failed to get tbluHash"),
		"patch": rfcpatch,
		"patchVersion": 4
	})
}

fn convert_rt_reference_to_qn(
	reference: &SEntityTemplateReference,
	factory: &RTFactory,
	blueprint: &RTBlueprint,
	factory_meta: &ResourceMeta,
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
			_ => panic!("Uhh you can't have a -2 entity index and then not provide the entity id"),
		})
	}
}

fn convert_qn_reference_to_rt(
	reference: Ref,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>,
) -> SEntityTemplateReference {
	match reference {
		Ref::Short(None) => SEntityTemplateReference {
			entity_id: 18446744073709551615,
			external_scene_index: -1,
			entity_index: -1,
			exposed_entity: "".to_string(),
		},
		Ref::Short(Some(ent)) => SEntityTemplateReference {
			entity_id: 18446744073709551615,
			external_scene_index: -1,
			entity_index: entity_id_to_index_mapping
				.get(&ent)
				.expect("Short ref referred to a nonexistent entity ID")
				.to_owned() as i32,
			exposed_entity: "".to_string(),
		},
		Ref::Full(fullref) => SEntityTemplateReference {
			entity_id: match &fullref.external_scene {
				None => 18446744073709551615,
				Some(_) => u64::from_str_radix(fullref.entity_ref.as_str(), 16)
					.expect("Full ref had invalid hex ref"),
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
						"TEMP referenced external scene not found in externalScenes in sub-entity",
					)
					.try_into()
					.unwrap(),
			},
			entity_index: match &fullref.external_scene {
				None => entity_id_to_index_mapping
					.get(&fullref.entity_ref)
					.expect("Full ref referred to a nonexistent entity ID")
					.to_owned() as i32,
				Some(_) => -2,
			},
			exposed_entity: fullref.exposed_entity.unwrap_or_default(),
		},
	}
}

fn convert_rt_property_value_to_qn(
	property: &SEntityTemplatePropertyValue,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
) -> Value {
	match property.property_type.as_str() {
		"SEntityTemplateReference" => to_value(convert_rt_reference_to_qn(
			&from_value::<SEntityTemplateReference>(property.property_value.to_owned())
				.expect("Converting RT ref to QN in property value returned error in parsing"),
			factory,
			blueprint,
			factory_meta,
		))
		.expect("Converting RT ref to QN in property value returned error in serialisation"),

		"ZRuntimeResourceID" => {
			match from_value::<ZRuntimeResourceIDPropertyValue>(property.property_value.to_owned())
				.expect("ZRuntimeResourceID did not have a valid format")
			{
				ZRuntimeResourceIDPropertyValue {
					m_IDHigh: 4294967295,
					m_IDLow: 4294967295,
				} => Value::Null,

				ZRuntimeResourceIDPropertyValue {
					m_IDHigh: _id_high, // We ignore the id_high as no resource in the game has that many depends
					m_IDLow: id_low,
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
						to_value(depend_data.hash.to_owned()).expect("Hash of ZRuntimeResourceID depend was not a valid JSON value (should have been string)")
					}
				}
			}
		}

		"SMatrix43" => {
			let matrix = from_value::<SMatrix43PropertyValue>(property.property_value.to_owned())
				.expect("SMatrix43 did not have a valid format");

			json!({
				"rotation": {
					"x": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.YAxis.z).atan2(matrix.ZAxis.z) } else { (matrix.ZAxis.y).atan2(matrix.YAxis.y) }) * RAD2DEG,
					"y": matrix.XAxis.z.clamp(-1.0, 1.0).asin() * RAD2DEG,
					"z": (if matrix.XAxis.z.abs() < 0.9999999 { (- matrix.XAxis.y).atan2(matrix.XAxis.x) } else { 0.0 }) * RAD2DEG
				},
				"position": matrix.Trans
			})
		}

		"ZGuid" => {
			let guid = from_value::<ZGuidPropertyValue>(property.property_value.to_owned())
				.expect("ZGuid did not have a valid format");

			let mut val = String::from("");
			write!(val, "{:0>8x}", guid._a).unwrap();
			write!(val, "-").unwrap();
			write!(val, "{:0>4x}", guid._b).unwrap();
			write!(val, "-").unwrap();
			write!(val, "{:0>4x}", guid._c).unwrap();
			write!(val, "-").unwrap();
			write!(val, "{:0>2x}", guid._d).unwrap();
			write!(val, "{:0>2x}", guid._e).unwrap();
			write!(val, "-").unwrap();
			write!(val, "{:0>2x}", guid._f).unwrap();
			write!(val, "{:0>2x}", guid._g).unwrap();
			write!(val, "{:0>2x}", guid._h).unwrap();
			write!(val, "{:0>2x}", guid._i).unwrap();
			write!(val, "{:0>2x}", guid._j).unwrap();
			write!(val, "{:0>2x}", guid._k).unwrap();

			to_value(val).unwrap()
		}

		"SColorRGB" => {
			let map = property
				.property_value
				.as_object()
				.expect("SColorRGB was not an object");

			let mut val = String::from("#");
			write!(
				val,
				"{:0>2x}",
				(map.get("r")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("g")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("b")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();

			to_value(val).unwrap()
		}

		"SColorRGBA" => {
			let map = property
				.property_value
				.as_object()
				.expect("SColorRGBA was not an object");

			let mut val = String::from("#");
			write!(
				val,
				"{:0>2x}",
				(map.get("r")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("g")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("b")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("a")
					.expect("Colour did not have required key")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();

			to_value(val).unwrap()
		}

		_ => property.property_value.to_owned(),
	}
}

fn convert_rt_property_to_qn(
	property: &SEntityTemplateProperty,
	post_init: bool,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
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
							&from_value({
								json!({
									"$type": y.collect::<String>(), // mock a single value for each array element
									"$val": x
								})
							})
							.expect("RT property array value was invalid"),
							factory,
							factory_meta,
							blueprint,
						)
					})
					.collect::<Vec<Value>>(),
			)
			.unwrap()
		} else {
			convert_rt_property_value_to_qn(&property.value, factory, factory_meta, blueprint)
		},
		post_init: if post_init { Some(true) } else { None },
	}
}

pub fn convert_to_qn(
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
	blueprint_meta: &ResourceMeta,
	game: Game,
) -> Entity {
	if {
		let mut unique = blueprint.sub_entities.clone();
		unique.dedup_by_key(|x| x.entity_id);

		unique.len() != blueprint.sub_entities.len()
	} {
		panic!("Cannot convert entity with duplicate IDs");
	}

	let mut entity = Entity {
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
			let vec: Vec<(String, SubEntity)> = factory
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
								factory_meta,
							),
							factory_flag: match factory_dependency.flag.as_str() {
								"1F" => None,
								flag => Some(flag.to_owned()),
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
												PropertyID::String(id) => id.to_owned(),
											}, // key
											convert_rt_property_to_qn(
												property,
												false,
												factory,
												factory_meta,
												blueprint,
											), // value
										)
									})
									.chain(sub_entity_factory.post_init_property_values.iter().map(
										|property| {
											(
												// we do a little code duplication
												match &property.n_property_id {
													PropertyID::Int(id) => id.to_string(),
													PropertyID::String(id) => id.to_owned(),
												},
												convert_rt_property_to_qn(
													property,
													true,
													factory,
													factory_meta,
													blueprint,
												),
											)
										},
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
															),
														)
													})
													.collect::<LinkedHashMap<String, Property>>(),
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
								let x: LinkedHashMap<String, PropertyAlias> = sub_entity_blueprint
									.property_aliases
									.iter()
									.map(|alias| {
										(
											alias.s_property_name.to_owned(),
											PropertyAlias {
												original_property: alias.s_alias_name.to_owned(),
												original_entity: Ref::Short(Some(format!(
                                        "{:x}",
                                        blueprint
                                            .sub_entities
                                            .get(alias.entity_id as usize)
                                            .expect(
                                                "Property alias referred to nonexistent sub-entity",
                                            )
                                            .entity_id
                                    ))),
											},
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
												targets: exposed_entity
													.a_targets
													.iter()
													.map(|target| {
														convert_rt_reference_to_qn(
															target,
															factory,
															blueprint,
															factory_meta,
														)
													})
													.collect(),
											},
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
                                ),
										)
									})
									.collect();

								if !x.is_empty() {
									Some(x)
								} else {
									None
								}
							},
							subsets: None, // will be mutated later
						},
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
					factory_meta,
				),
				to_entity: convert_rt_reference_to_qn(
					&x.from_entity,
					factory,
					blueprint,
					factory_meta,
				),
				from_pin_name: x.from_pin_name.to_owned(),
				to_pin_name: x.to_pin_name.to_owned(),
				value: match x.constant_pin_value.property_type.as_str() {
					"void" => None,
					_ => Some(SimpleProperty {
						value_type: x.constant_pin_value.property_type.to_owned(),
						value: x.constant_pin_value.property_value.to_owned(),
					}),
				},
			})
			.collect(),
		pin_connection_overrides: blueprint
			.pin_connection_overrides
			.par_iter()
			.map(|x| PinConnectionOverride {
				from_entity: convert_rt_reference_to_qn(
					&x.from_entity,
					factory,
					blueprint,
					factory_meta,
				),
				to_entity: convert_rt_reference_to_qn(
					&x.from_entity,
					factory,
					blueprint,
					factory_meta,
				),
				from_pin_name: x.from_pin_name.to_owned(),
				to_pin_name: x.to_pin_name.to_owned(),
				value: match x.constant_pin_value.property_type.as_str() {
					"void" => None,
					_ => Some(SimpleProperty {
						value_type: x.constant_pin_value.property_type.to_owned(),
						value: x.constant_pin_value.property_value.to_owned(),
					}),
				},
			})
			.collect(),
		property_overrides: vec![],
		sub_type: match blueprint.sub_type {
			2 => SubType::Brick,
			1 => SubType::Scene,
			0 => SubType::Template,
			_ => panic!("Invalid subtype"),
		},
		quick_entity_version: 2.2,
	}; // this statement is 311 lines long

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
						value_type: pin.constant_pin_value.property_type.to_owned(),
						value: pin.constant_pin_value.property_value.to_owned(),
					},
				})
			});
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
						value_type: forwarding.constant_pin_value.property_type.to_owned(),
						value: forwarding.constant_pin_value.property_value.to_owned(),
					},
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
						value_type: forwarding.constant_pin_value.property_type.to_owned(),
						value: forwarding.constant_pin_value.property_value.to_owned(),
					},
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
				factory_meta,
			)];

			let props = [(
				match &property_override.property_value.n_property_id {
					PropertyID::Int(id) => id.to_string(),
					PropertyID::String(id) => id.to_owned(),
				},
				{
					let prop = convert_rt_property_to_qn(
						&property_override.property_value,
						false,
						factory,
						factory_meta,
						blueprint,
					);

					OverriddenProperty {
						value: prop.value,
						property_type: prop.property_type,
					} // no post-init
				},
			)]
			.into_iter()
			.collect();

			// if same entity being overridden, merge props
			if let Some(found) = pass1.iter_mut().find(|x| x.entities == ents) {
				found.properties.extend(props);
			} else {
				pass1.push(PropertyOverride {
					entities: ents,
					properties: props,
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

pub fn convert_to_rt(
	entity: &Entity,
	game: Game,
) -> (RTFactory, ResourceMeta, RTBlueprint, ResourceMeta) {
	let entity_id_to_index_mapping: HashMap<String, usize> = entity
		.entities
		.keys()
		.enumerate()
		.map(|(x, y)| (y.to_owned(), x))
		.collect();

	let factory = RTFactory {
		sub_type: match entity.sub_type {
			SubType::Brick => 2,
			SubType::Scene => 1,
			SubType::Template => 0,
		},
		blueprint_index_in_resource_header: 0,
		root_entity_index: *entity_id_to_index_mapping
			.get(&entity.root_entity)
			.expect("Root entity was non-existent"),
		sub_entities: todo!(),
		property_overrides: todo!(),
		external_scene_type_indices_in_resource_header: todo!(),
	};

	let factory_meta = ResourceMeta {
		hash_offset: 1367, // none of this data actually matters except for dependencies and resource type
		hash_reference_data: vec![],
		hash_reference_table_dummy: 0,
		hash_reference_table_size: 193,
		hash_resource_type: "TEMP".to_string(),
		hash_size: 2147484657,
		hash_size_final: 2377,
		hash_size_in_memory: 1525,
		hash_size_in_video_memory: 4294967295,
		hash_value: entity.factory_hash.to_owned(),
	};

	let blueprint = RTBlueprint {
		sub_type: match entity.sub_type {
			SubType::Brick => 2,
			SubType::Scene => 1,
			SubType::Template => 0,
		},
		root_entity_index: todo!(),
		sub_entities: todo!(),
		external_scene_type_indices_in_resource_header: todo!(),
		pin_connections: todo!(),
		input_pin_forwardings: todo!(),
		output_pin_forwardings: todo!(),
		override_deletes: todo!(),
		pin_connection_overrides: todo!(),
		pin_connection_override_deletes: todo!(),
	};

	let blueprint_meta = ResourceMeta {
		hash_offset: 1367,
		hash_reference_data: vec![],
		hash_reference_table_dummy: 0,
		hash_reference_table_size: 193,
		hash_resource_type: "TBLU".to_string(),
		hash_size: 2147484657,
		hash_size_final: 2377,
		hash_size_in_memory: 1525,
		hash_size_in_video_memory: 4294967295,
		hash_value: entity.factory_hash.to_owned(),
	};

	(factory, factory_meta, blueprint, blueprint_meta)
}

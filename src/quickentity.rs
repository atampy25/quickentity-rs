use linked_hash_map::LinkedHashMap;
use std::collections::HashMap;

use itertools::Itertools;
use json_patch::{diff, from_value as json_patch_from_value, patch as apply_rfc_patch};
use rayon::prelude::*;
use serde_json::{from_value, json, to_value, Value};
use std::fmt::Write;

use crate::{
	qn_structs::{
		Dependency, DependencyWithFlag, Entity, ExposedEntity, FullRef, OverriddenProperty,
		PinConnectionOverride, PinConnectionOverrideDelete, Property, PropertyAlias,
		PropertyOverride, Ref, RefMaybeConstantValue, RefWithConstantValue, SimpleProperty,
		SubEntity, SubType,
	},
	rpkg_structs::{ResourceDependency, ResourceMeta},
	rt_structs::{
		PropertyID, RTBlueprint, RTFactory, SEntityTemplateEntitySubset,
		SEntityTemplateExposedEntity, SEntityTemplatePinConnection,
		SEntityTemplatePlatformSpecificProperty, SEntityTemplateProperty,
		SEntityTemplatePropertyAlias, SEntityTemplatePropertyOverride,
		SEntityTemplatePropertyValue, SEntityTemplateReference,
		SExternalEntityTemplatePinConnection, STemplateBlueprintSubEntity,
		STemplateFactorySubEntity,
	},
	util_structs::{SMatrix43PropertyValue, ZGuidPropertyValue, ZRuntimeResourceIDPropertyValue},
};

const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

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
	reference: &Ref,
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
				.get(ent)
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
			exposed_entity: fullref.exposed_entity.to_owned().unwrap_or_default(),
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
					.expect("Colour did not have required key r")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("g")
					.expect("Colour did not have required key g")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("b")
					.expect("Colour did not have required key b")
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
					.expect("Colour did not have required key r")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("g")
					.expect("Colour did not have required key g")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("b")
					.expect("Colour did not have required key b")
					.as_f64()
					.unwrap() * 255.0)
					.round() as u8
			)
			.unwrap();
			write!(
				val,
				"{:0>2x}",
				(map.get("a")
					.expect("Colour did not have required key a")
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

fn convert_qn_property_value_to_rt(
	property: &Property,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>,
	factory_dependencies_index_mapping: &HashMap<String, usize>,
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

			let a = x.cos();
			let b = x.sin();
			let c = y.cos();
			let d = y.sin();
			let e = z.cos();
			let f = z.sin();

			let ae = a * e;
			let af = a * f;
			let be = b * e;
			let bf = b * f;

			json!({
				"XAxis": {
					"x": c * e,
					"y": -c * f,
					"z": d
				},
				"YAxis": {
					"x": af + be * d,
					"y": ae - bf * d,
					"z": -b * c
				},
				"ZAxis": {
					"x": bf - ae * d,
					"y": be + af * d,
					"z": a * c
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
			"r": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(0).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string(),
			"g": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(2).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string(),
			"b": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(4).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string()
		}),

		"SColorRGBA" => json!({
			"r": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(0).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string(),
			"g": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(2).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string(),
			"b": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(4).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string(),
			"a": (f64::from(u8::from_str_radix(&property.value.as_str().unwrap().chars().skip(1).skip(6).nth(1).unwrap().to_string(), 16).unwrap()) / 255.0).to_string()
		}),

		_ => property.value.to_owned()
	}
}

fn convert_qn_property_to_rt(
	property_name: &str,
	property_value: &Property,
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	entity_id_to_index_mapping: &HashMap<String, usize>,
	factory_dependencies_index_mapping: &HashMap<String, usize>,
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
									value: x.to_owned(),
								},
								factory,
								factory_meta,
								entity_id_to_index_mapping,
								factory_dependencies_index_mapping,
							)
						})
						.collect::<Vec<Value>>(),
				)
				.unwrap()
			} else {
				convert_qn_property_value_to_rt(
					property_value,
					factory,
					factory_meta,
					entity_id_to_index_mapping,
					factory_dependencies_index_mapping,
				)
			},
		},
	}
}

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

fn get_factory_dependencies(entity: &Entity) -> Vec<ResourceDependency> {
	vec![
		// blueprint first
		vec![ResourceDependency {
			hash: entity.blueprint_hash.to_owned(),
			flag: "1F".to_string(),
		}],
		// then external scenes
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceDependency {
				hash: scene.to_owned(),
				flag: "1F".to_string(),
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
					.unwrap_or_else(|| "1F".to_string()), // this is slightly more efficient
			})
			.collect(),
		// then ZRuntimeResourceIDs
		entity
			.entities
			.iter()
			.collect_vec()
			.par_iter()
			.flat_map(|(_, sub_entity)| {
				if let Some(props) = &sub_entity.properties {
					props
						.iter()
						.filter(|(_, prop)| {
							prop.property_type == "ZRuntimeResourceID" && !prop.value.is_null()
						})
						.map(|(_, prop)| {
							if prop.value.is_string() {
								ResourceDependency {
									hash: prop.value.as_str().unwrap().to_string(),
									flag: "1F".to_string(),
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
										.to_string(),
								}
							}
						})
						.collect()
				} else {
					vec![]
				}
			})
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

fn get_blueprint_dependencies(entity: &Entity) -> Vec<ResourceDependency> {
	vec![
		entity
			.external_scenes
			.par_iter()
			.map(|scene| ResourceDependency {
				hash: scene.to_owned(),
				flag: "1F".to_string(),
			})
			.collect::<Vec<ResourceDependency>>(),
		entity
			.entities
			.iter()
			.map(|(_, sub_entity)| ResourceDependency {
				hash: sub_entity.blueprint.to_owned(),
				flag: "1F".to_string(),
			})
			.collect(),
	]
	.into_iter()
	.concat()
	.into_iter()
	.unique()
	.collect()
}

pub fn convert_to_qn(
	factory: &RTFactory,
	factory_meta: &ResourceMeta,
	blueprint: &RTBlueprint,
	blueprint_meta: &ResourceMeta,
) -> Entity {
	if {
		let mut unique = blueprint.sub_entities.to_owned();
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
                                            .get(alias.entity_id)
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
					&x.to_entity,
					factory,
					blueprint,
					factory_meta,
				),
				from_pin: x.from_pin_name.to_owned(),
				to_pin: x.to_pin_name.to_owned(),
				value: match x.constant_pin_value.property_type.as_str() {
					"void" => None,
					_ => Some(SimpleProperty {
						property_type: x.constant_pin_value.property_type.to_owned(),
						value: x.constant_pin_value.property_value.to_owned(),
					}),
				},
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
					factory_meta,
				),
				to_entity: convert_rt_reference_to_qn(
					&x.to_entity,
					factory,
					blueprint,
					factory_meta,
				),
				from_pin: x.from_pin_name.to_owned(),
				to_pin: x.to_pin_name.to_owned(),
				value: match x.constant_pin_value.property_type.as_str() {
					"void" => None,
					_ => Some(SimpleProperty {
						property_type: x.constant_pin_value.property_type.to_owned(),
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
		extra_factory_dependencies: vec![],
		extra_blueprint_dependencies: vec![],
		comments: vec![],
	};

	{
		let depends = get_factory_dependencies(&entity);
		entity.extra_factory_dependencies = factory_meta
			.hash_reference_data
			.iter()
			.filter(|x| !depends.contains(x))
			.map(|x| match x {
				ResourceDependency { hash, flag } if flag == "1F" => {
					Dependency::Short(hash.to_owned())
				}
				ResourceDependency { hash, flag } => Dependency::Full(DependencyWithFlag {
					resource: hash.to_owned(),
					flag: flag.to_owned(),
				}),
			})
			.collect();
	}

	{
		let depends = get_blueprint_dependencies(&entity);
		entity.extra_blueprint_dependencies = blueprint_meta
			.hash_reference_data
			.iter()
			.filter(|x| !depends.contains(x))
			.map(|x| match x {
				ResourceDependency { hash, flag } if flag == "1F" => {
					Dependency::Short(hash.to_owned())
				}
				ResourceDependency { hash, flag } => Dependency::Full(DependencyWithFlag {
					resource: hash.to_owned(),
					flag: flag.to_owned(),
				}),
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
						value: pin.constant_pin_value.property_value.to_owned(),
					},
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
						factory_meta,
					))
				} else {
					RefMaybeConstantValue::RefWithConstantValue(RefWithConstantValue {
						entity_ref: convert_rt_reference_to_qn(
							&pin_connection_override.to_entity,
							factory,
							blueprint,
							factory_meta,
						),
						value: SimpleProperty {
							property_type: pin_connection_override
								.constant_pin_value
								.property_type
								.to_owned(),
							value: pin_connection_override
								.constant_pin_value
								.property_value
								.to_owned(),
						},
					})
				},
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
						property_type: forwarding.constant_pin_value.property_type.to_owned(),
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
			SubType::Template => 0,
		},
		blueprint_index_in_resource_header: 0,
		root_entity_index: *entity_id_to_index_mapping
			.get(&entity.root_entity)
			.expect("Root entity was non-existent"),
		sub_entities: vec![],
		property_overrides: vec![],
		external_scene_type_indices_in_resource_header: (1..entity.external_scenes.len() + 1)
			.collect(),
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
						flag: "1F".to_string(),
					},
					Dependency::Full(DependencyWithFlag { resource, flag }) => ResourceDependency {
						hash: resource.to_owned(),
						flag: flag.to_owned(),
					},
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
		hash_value: entity.factory_hash.to_owned(),
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
						flag: "1F".to_string(),
					},
					Dependency::Full(DependencyWithFlag { resource, flag }) => ResourceDependency {
						hash: resource.to_owned(),
						flag: flag.to_owned(),
					},
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
		hash_value: entity.factory_hash.to_owned(),
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
								&entity_id_to_index_mapping,
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
												post_init: None,
											},
											&factory,
											&factory_meta,
											&entity_id_to_index_mapping,
											&factory_dependencies_index_mapping,
										)
										.value
										.property_value,
									)
									.unwrap(),
								},
							},
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
				&entity_id_to_index_mapping,
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
							&factory_dependencies_index_mapping,
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
							&factory_dependencies_index_mapping,
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
									&factory_dependencies_index_mapping,
								),
							})
							.collect::<Vec<SEntityTemplatePlatformSpecificProperty>>()
					})
					.collect()
			} else {
				vec![]
			},
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
				&entity_id_to_index_mapping,
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
					.map(|(aliased_name, alias)| {
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
								.targets
								.iter()
								.map(|target| {
									convert_qn_reference_to_rt(
										target,
										&factory,
										&factory_meta,
										&entity_id_to_index_mapping,
									)
								})
								.collect(),
						},
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
								.to_owned(),
						)
					})
					.collect()
			} else {
				vec![]
			},
			entity_subsets: vec![], // will be mutated later
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
								.expect("Entity subset referenced nonexistent local entity"),
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
								entities: vec![entity_index],
							},
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
													id,
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _,
													},
												) => id,

												_ => panic!("Invalid to_id for trigger on events"),
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value,
												},
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned(),
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null,
											},
										},
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
													id,
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _,
													},
												) => id,

												_ => panic!(
													"Invalid to_id for trigger on input copying"
												),
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value,
												},
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned(),
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null,
											},
										},
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
													id,
												))) => id,

												RefMaybeConstantValue::RefWithConstantValue(
													RefWithConstantValue {
														entity_ref: Ref::Short(Some(id)),
														value: _,
													},
												) => id,

												_ => panic!(
													"Invalid to_id for trigger on output copying"
												),
											})
											.unwrap(),
										from_pin_name: event.to_owned(),
										to_pin_name: trigger.to_owned(),
										constant_pin_value: match &trigger_entity {
											RefMaybeConstantValue::RefWithConstantValue(
												RefWithConstantValue {
													entity_ref: _,
													value,
												},
											) => SEntityTemplatePropertyValue {
												property_type: value.property_type.to_owned(),
												property_value: value.value.to_owned(),
											},

											_ => SEntityTemplatePropertyValue {
												property_type: "void".to_owned(),
												property_value: Value::Null,
											},
										},
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

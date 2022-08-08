use std::collections::HashMap;

use json_patch::{diff, from_value as json_patch_from_value, patch as apply_rfc_patch};
use serde_json::{from_value, json, to_value, Map, Value};

use crate::{
    qn_structs::{FullRef, Property, Ref},
    rpkg_structs::ResourceMeta,
    rt_structs::{
        RTBlueprint, RTFactory, SEntityTemplateProperty, SEntityTemplatePropertyValue,
        SEntityTemplateReference,
    },
    util_structs::{SMatrix43PropertyValue, ZGuidPropertyValue, ZRuntimeResourceIDPropertyValue},
};

const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;

pub fn apply_patch<'a>(entity: &mut Value, patch: &Value) {
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

pub fn generate_patch<'a>(original: &Value, modified: &Value) -> Value {
    let mut rfcpatch = json!(diff(&original, &modified));

    if let Some(pos) =
        rfcpatch
            .as_array_mut()
            .unwrap()
            .iter()
            .position(|value| match value.get("path") {
                Some(path) => path == "/quickEntityVersion",
                _ => false,
            })
    {
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
    if reference.exposed_entity != "" || reference.external_scene_index != -1 {
        Ref::FullRef(FullRef {
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
                                as usize,
                        )
                        .expect("Expected an external scene to be in the TEMP meta")
                        .hash.to_owned(),
                ),
                _ => panic!("Uhh this external scene is not valid at all"),
            },
            exposed_entity: if reference.exposed_entity == "" {
                None
            } else {
                Some(reference.exposed_entity.to_owned())
            },
        })
    } else {
        Ref::ShortRef(match reference.entity_index {
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
    entity_id_to_index_mapping: HashMap<String, u32>,
) -> SEntityTemplateReference {
    match reference {
        Ref::ShortRef(None) => SEntityTemplateReference {
            entity_id: 18446744073709551615,
            external_scene_index: -1,
            entity_index: -1,
            exposed_entity: "".to_string(),
        },
        Ref::ShortRef(Some(ent)) => SEntityTemplateReference {
            entity_id: 18446744073709551615,
            external_scene_index: -1,
            entity_index: entity_id_to_index_mapping
                .get(&ent)
                .expect("Short ref referred to a nonexistent entity ID")
                .to_owned() as i32,
            exposed_entity: "".to_string(),
        },
        Ref::FullRef(fullref) => SEntityTemplateReference {
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
                        factory_meta.hash_reference_data.get(*x as usize).expect(
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
            exposed_entity: fullref.exposed_entity.unwrap_or("".to_string()),
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
            val.push_str(&format!("{:0>8x}", guid._a)); // this is insane and I have no idea why it is like this
            val.push_str("-");
            val.push_str(&format!("{:0>4x}", guid._b));
            val.push_str("-");
            val.push_str(&format!("{:0>4x}", guid._c));
            val.push_str("-");
            val.push_str(&format!("{:0>2x}", guid._d));
            val.push_str(&format!("{:0>2x}", guid._e));
            val.push_str("-");
            val.push_str(&format!("{:0>2x}", guid._f));
            val.push_str(&format!("{:0>2x}", guid._g));
            val.push_str(&format!("{:0>2x}", guid._h));
            val.push_str(&format!("{:0>2x}", guid._i));
            val.push_str(&format!("{:0>2x}", guid._j));
            val.push_str(&format!("{:0>2x}", guid._k));

            to_value(val).unwrap()
        }

        "SColorRGB" => {
            let map = property
                .property_value
                .as_object()
                .expect("SColorRGB was not an object");

            let mut val = String::from("#"); // TODO: pending QN poll bb5874 SColor string formatting
            val.push_str(&format!("{:0>2x}", map.get("r").unwrap().as_u64().unwrap()));
            val.push_str(&format!("{:0>2x}", map.get("g").unwrap().as_u64().unwrap()));
            val.push_str(&format!("{:0>2x}", map.get("b").unwrap().as_u64().unwrap()));

            to_value(val).unwrap()
        }

        "SColorRGBA" => {
            let map = property
                .property_value
                .as_object()
                .expect("SColorRGBA was not an object");

            let mut val = String::from("#"); // TODO: pending QN poll bb5874 SColor string formatting
            val.push_str(&format!("{:0>2x}", map.get("r").unwrap().as_u64().unwrap()));
            val.push_str(&format!("{:0>2x}", map.get("g").unwrap().as_u64().unwrap()));
            val.push_str(&format!("{:0>2x}", map.get("b").unwrap().as_u64().unwrap()));
            val.push_str(&format!("{:0>2x}", map.get("a").unwrap().as_u64().unwrap()));

            to_value(val).unwrap()
        }

        _ => property.property_value.clone(),
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
                        convert_rt_property_value_to_qn(
                            &from_value(x.to_owned()).expect("RT property value was not valid"),
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

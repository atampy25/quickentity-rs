use std::collections::HashMap;

use json_patch::{diff, from_value, patch as apply_rfc_patch};
use serde_json::{json, Value};

use crate::{
    qn_structs::{FullRef, Ref},
    rpkg_structs::ResourceMeta,
    rt_structs::{RTBlueprint, RTFactory, SEntityTemplateReference},
};

pub fn apply_patch<'a>(entity: &mut Value, patch: &Value) {
    apply_rfc_patch(
        entity,
        &from_value(
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
            exposed_entity: if let Some(exposedent) = fullref.exposed_entity {
                exposedent
            } else {
                "".to_string()
            },
        },
    }
}

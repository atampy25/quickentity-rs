use json_patch::{diff, from_value, patch as apply_rfc_patch};
use serde_json::{json, Value};

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

    match rfcpatch
        .as_array_mut()
        .unwrap()
        .iter()
        .position(|value| match value.get("path") {
            Some(path) => path == "/quickEntityVersion",
            _ => false,
        }) {
        Some(pos) => {
            rfcpatch.as_array_mut().unwrap().remove(pos);
        }
        _ => {}
    }

    json!({
        "tempHash": modified.get("tempHash").expect("Failed to get tempHash"),
        "tbluHash": modified.get("tbluHash").expect("Failed to get tbluHash"),
        "patch": rfcpatch,
        "patchVersion": 4
    })
}

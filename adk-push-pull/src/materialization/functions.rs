use super::{FlowImportPathMaps, insert_content_resource, replace_flow_import_ids_with_names};
use crate::{CommandGenError, clean_name, command_gen};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_function_resources(
    map: &mut ResourceMap,
    projection: &Value,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    for (id, function) in command_gen::functions::function_entries(projection) {
        if function
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id.as_str())
            .to_string();
        let file_name = clean_name(&name).to_lowercase();
        let file_path = format!("functions/{file_name}.py");
        let content = replace_flow_import_ids_with_names(
            &command_gen::functions::function_raw_content(&function),
            flow_import_path_maps,
        );
        insert_content_resource(map, &file_path, &id, &name, content)?;
    }
    for kind in [
        command_gen::functions::SpecialFunctionKind::Start,
        command_gen::functions::SpecialFunctionKind::End,
    ] {
        if let Some((id, function)) =
            command_gen::functions::special_function_entry(projection, kind)
        {
            let name = command_gen::functions::special_function_name(kind).to_string();
            let file_path = format!("functions/{name}.py");
            let content = replace_flow_import_ids_with_names(
                &command_gen::functions::function_raw_content(&function),
                flow_import_path_maps,
            );
            insert_content_resource(map, &file_path, &id, &name, content)?;
        }
    }

    Ok(())
}

use super::{FlowImportPathMaps, insert_content_resource, replace_flow_import_ids_with_names};
use crate::command_gen::per_resource_files::functions;
use crate::{CommandGenError, clean_name};
use adk_types::ResourceMap;
use serde_json::Value;

pub(super) fn insert_function_resources(
    map: &mut ResourceMap,
    projection: &Value,
    flow_import_path_maps: &FlowImportPathMaps,
) -> Result<(), CommandGenError> {
    for (id, function) in functions::function_entries(projection) {
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
        let file_name = clean_name(&name, true);
        let file_path = format!("functions/{file_name}.py");
        let content = replace_flow_import_ids_with_names(
            &functions::function_raw_content(&function),
            flow_import_path_maps,
        );
        insert_content_resource(map, &file_path, &id, &name, content)?;
    }
    for kind in [
        functions::SpecialFunctionKind::Start,
        functions::SpecialFunctionKind::End,
    ] {
        if let Some((id, function)) = functions::special_function_entry(projection, kind) {
            let name = functions::special_function_name(kind).to_string();
            let file_path = format!("functions/{name}.py");
            let content = replace_flow_import_ids_with_names(
                &functions::function_raw_content(&function),
                flow_import_path_maps,
            );
            insert_content_resource(map, &file_path, &id, &name, content)?;
        }
    }

    Ok(())
}

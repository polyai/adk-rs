use super::*;
use adk_types::Resource;
use indexmap::IndexMap;

fn map_with(resources: Vec<(String, Resource)>) -> ResourceMap {
    let mut map: ResourceMap = IndexMap::new();
    for (path, resource) in resources {
        map.insert(path, resource);
    }
    map
}

fn flatten(groups: CommandGroups) -> Vec<adk_protobuf::Command> {
    groups
        .deletes
        .into_iter()
        .chain(groups.creates)
        .chain(groups.updates)
        .chain(groups.post_updates)
        .collect()
}

#[test]
fn variable_create_and_delete_roundtrip_types() {
    let mut resources = map_with(vec![(
        "variables/OrderId".into(),
        Resource {
            resource_id: "local".into(),
            name: "OrderId".into(),
            file_path: "variables/OrderId".into(),
            payload: serde_json::json!({ "content": "" }),
        },
    )]);
    let projection = serde_json::json!({});
    let commands = flatten(variable_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "variable_create");
    assert!(matches!(
        commands[0].payload,
        Some(CommandPayload::VariableCreate(_))
    ));

    resources.clear();
    let projection = serde_json::json!({
        "variables": { "variables": { "entities": {
            "vrbl-x": { "name": "OrderId" }
        }}}
    });
    let commands = flatten(variable_resource_command_groups(
        &resources,
        &projection,
        &None,
    ));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].r#type, "variable_delete");
}

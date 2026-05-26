use adk_resources::DISCOVER_DISPATCH;
use adk_types::{ORDERED_TYPE_NAMES, RESOURCE_TYPE_REGISTRY};

#[test]
fn resource_type_metadata_roundtrips_between_python_and_rust_names() {
    let metadata = RESOURCE_TYPE_REGISTRY;
    assert!(!metadata.is_empty(), "metadata should not be empty");

    for item in metadata {
        assert_eq!(
            adk_types::descriptor_by_status_name(item.status_resource_name).map(|d| d.type_name),
            Some(item.type_name)
        );
        assert_eq!(
            adk_types::descriptor_by_type_name(item.type_name).map(|d| d.status_resource_name),
            Some(item.status_resource_name)
        );
    }
}

#[test]
fn ordered_type_names_matches_metadata_order() {
    let metadata_names: Vec<&str> = RESOURCE_TYPE_REGISTRY.iter().map(|m| m.type_name).collect();
    assert_eq!(ORDERED_TYPE_NAMES.as_slice(), metadata_names.as_slice());
}

#[test]
fn discover_dispatch_matches_registry_order() {
    let registry_names: Vec<&str> = RESOURCE_TYPE_REGISTRY.iter().map(|d| d.type_name).collect();
    let dispatch_names: Vec<&str> = DISCOVER_DISPATCH
        .iter()
        .map(|entry| entry.type_name)
        .collect();
    assert_eq!(
        dispatch_names, registry_names,
        "DISCOVER_DISPATCH must cover every entry in RESOURCE_TYPE_REGISTRY in registry order"
    );
}

#[test]
fn unknown_metadata_lookups_return_none() {
    assert_eq!(adk_types::descriptor_by_status_name("does_not_exist"), None);
    assert_eq!(adk_types::descriptor_by_type_name("DoesNotExist"), None);
}

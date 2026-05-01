use adk_core::discover::{
    ordered_type_names, resource_name_to_type_name, resource_type_metadata,
    type_name_to_resource_name,
};

#[test]
fn resource_type_metadata_roundtrips_between_python_and_rust_names() {
    let metadata = resource_type_metadata();
    assert!(!metadata.is_empty(), "metadata should not be empty");

    for item in metadata {
        assert_eq!(
            resource_name_to_type_name(item.status_resource_name),
            Some(item.type_name)
        );
        assert_eq!(
            type_name_to_resource_name(item.type_name),
            Some(item.status_resource_name)
        );
    }
}

#[test]
fn ordered_type_names_matches_metadata_order() {
    let ordered = ordered_type_names();
    let metadata_names: Vec<&str> = resource_type_metadata()
        .iter()
        .map(|m| m.type_name)
        .collect();
    assert_eq!(ordered, metadata_names.as_slice());
}

#[test]
fn unknown_metadata_lookups_return_none() {
    assert_eq!(resource_name_to_type_name("does_not_exist"), None);
    assert_eq!(type_name_to_resource_name("DoesNotExist"), None);
}

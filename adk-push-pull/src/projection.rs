use serde_json::Value;
use std::collections::HashSet;

/// Path to a projection collection stored as `{ ids, entities }`.
///
/// Python materialization and command generation both respect backend `ids`
/// ordering when present, then fall back to sorted entity IDs for anything not
/// listed. Keeping that traversal here prevents file materialization and push
/// command generation from drifting apart.
#[derive(Clone, Copy)]
pub(crate) struct ProjectionCollection {
    path: &'static [&'static str],
}

impl ProjectionCollection {
    pub(crate) const fn new(path: &'static [&'static str]) -> Self {
        Self { path }
    }

    pub(crate) fn entries<'a>(&self, root: &'a Value) -> Vec<(String, &'a Value)> {
        projection_entity_refs(root, self.path)
    }

    pub(crate) fn owned_entries(&self, root: &Value) -> Vec<(String, Value)> {
        projection_entity_values(root, self.path)
    }
}

pub(crate) fn projection_entity_refs<'a>(
    root: &'a Value,
    path: &[&str],
) -> Vec<(String, &'a Value)> {
    let Some(value) = value_at_path(root, path) else {
        return Vec::new();
    };
    projection_entity_refs_at(value)
}

pub(crate) fn projection_entity_values(root: &Value, path: &[&str]) -> Vec<(String, Value)> {
    projection_entity_refs(root, path)
        .into_iter()
        .map(|(id, value)| (id, value.clone()))
        .collect()
}

pub(crate) fn projection_entity_refs_at(value: &Value) -> Vec<(String, &Value)> {
    let Some(entities) = value.get("entities").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    if let Some(ids) = value.get("ids").and_then(Value::as_array) {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(entity) = entities.get(id) {
                ordered.push((id.to_string(), entity));
                seen.insert(id.to_string());
            }
        }
    }

    let mut remaining = entities
        .iter()
        .filter(|(id, _)| !seen.contains(*id))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|(left, _)| *left);
    ordered.extend(
        remaining
            .into_iter()
            .map(|(id, entity)| (id.clone(), entity)),
    );
    ordered
}

pub(crate) fn projection_entity_values_at(value: &Value) -> Vec<(String, Value)> {
    projection_entity_refs_at(value)
        .into_iter()
        .map(|(id, value)| (id, value.clone()))
        .collect()
}

fn value_at_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

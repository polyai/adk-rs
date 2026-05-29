use super::{
    function_parameter_decorator_names, function_signature_parameter_list,
    function_signature_parameters, is_python_function_resource,
};
use adk_io::parse_multi_resource_path;
use adk_types::ResourceMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionValidationFailure {
    path: String,
    detail: String,
}

impl FunctionValidationFailure {
    fn new(path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            detail: detail.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for FunctionValidationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.detail)
    }
}

impl std::error::Error for FunctionValidationFailure {}

pub fn validate_python_function_resources(
    resources: &ResourceMap,
) -> Result<(), FunctionValidationFailure> {
    let mut paths = resources
        .keys()
        .filter(|path| is_python_function_resource(path))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let Some(content) = resource_content(resources, &path) else {
            continue;
        };
        validate_python_function_resource(&path, content)?;
    }
    Ok(())
}

pub fn validate_python_function_resource(
    path: &str,
    content: &str,
) -> Result<(), FunctionValidationFailure> {
    validate_python_resource_syntax(path, content)?;
    validate_function_parameter_decorators(path, content)
}

pub fn validate_flow_scoped_function_resource(
    path: &str,
    content: &str,
    allow_user_parameters: bool,
) -> Result<Vec<String>, FunctionValidationFailure> {
    validate_python_function_resource(path, content)?;
    let Some(file_name) = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".py"))
    else {
        return Ok(Vec::new());
    };
    let expected_typed = format!("def {file_name}(conv: Conversation, flow: Flow)");
    let valid_signature = if allow_user_parameters {
        flow_scoped_signature_has_receiver_prefix(content, file_name)
    } else {
        content.contains(&expected_typed)
            || content.contains(&format!("def {file_name}(conv, flow)"))
    };
    if valid_signature {
        Ok(Vec::new())
    } else {
        Ok(vec![format!(
            "Validation error in {path}: Function definition '{expected_typed}' not found in code."
        )])
    }
}

fn validate_function_parameter_decorators(
    path: &str,
    content: &str,
) -> Result<(), FunctionValidationFailure> {
    let function_name = reference_name_from_logical_path(path);
    let Some(parameters) = function_signature_parameters(content, &function_name) else {
        return Ok(());
    };
    for parameter_name in function_parameter_decorator_names(content) {
        let Some(annotation) = parameters.get(&parameter_name) else {
            return Err(FunctionValidationFailure::new(
                path,
                format!(
                    "Parameter '{parameter_name}' has no type annotation. Supported types: str, int, float, bool."
                ),
            ));
        };
        let Some(annotation) = annotation else {
            return Err(FunctionValidationFailure::new(
                path,
                format!(
                    "Parameter '{parameter_name}' has no type annotation. Supported types: str, int, float, bool."
                ),
            ));
        };
        if !matches!(annotation.as_str(), "str" | "int" | "float" | "bool") {
            return Err(FunctionValidationFailure::new(
                path,
                format!(
                    "Parameter '{parameter_name}' has an unsupported type annotation. Supported types: str, int, float, bool."
                ),
            ));
        }
    }
    Ok(())
}

fn flow_scoped_signature_has_receiver_prefix(content: &str, function_name: &str) -> bool {
    let Some(parameters) = function_signature_parameter_list(content, function_name) else {
        return false;
    };
    let Some(conv) = parameters.first() else {
        return false;
    };
    let Some(flow) = parameters.get(1) else {
        return false;
    };
    conv.name == "conv"
        && conv
            .annotation
            .as_deref()
            .is_none_or(|annotation| annotation == "Conversation")
        && flow.name == "flow"
        && flow
            .annotation
            .as_deref()
            .is_none_or(|annotation| annotation == "Flow")
}

fn validate_python_resource_syntax(
    path: &str,
    content: &str,
) -> Result<(), FunctionValidationFailure> {
    if let Err(error) = validate_python_module(content) {
        return Err(FunctionValidationFailure::new(path, error.to_string()));
    }
    Ok(())
}

fn validate_python_module(source: &str) -> Result<(), PythonSyntaxError> {
    ruff_python_parser::parse_module(source)
        .map(|_| ())
        .map_err(|error| {
            let offset = usize::from(error.location.start());
            let (line, column) = line_column(source, offset);
            PythonSyntaxError {
                message: error.error.to_string(),
                line,
                column,
            }
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonSyntaxError {
    message: String,
    line: usize,
    column: usize,
}

impl fmt::Display for PythonSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.message, self.line, self.column
        )
    }
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut line_start = 0;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + ch.len_utf8();
        }
    }
    (line, offset.saturating_sub(line_start) + 1)
}

fn reference_name_from_logical_path(logical_path: &str) -> String {
    let (_, resource_suffix) = parse_multi_resource_path(logical_path);
    let source = resource_suffix.as_deref().unwrap_or(logical_path);
    let leaf = source.rsplit('/').next().unwrap_or(source);
    Path::new(leaf)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(leaf)
        .to_string()
}

fn resource_content<'a>(resources: &'a ResourceMap, path: &str) -> Option<&'a str> {
    resources.get(path)?.payload.get("content")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_valid_modules() {
        assert!(validate_python_module("def handler(conv):\n    return None\n").is_ok());
    }

    #[test]
    fn reports_line_and_column_for_invalid_modules() {
        let error =
            validate_python_module("def handler(conv):\n    if True\n        return None\n")
                .expect_err("invalid syntax");

        assert_eq!(error.line, 2);
        assert!(error.column > 0);
    }

    #[test]
    fn parameter_decorator_requires_supported_annotation() {
        let error = validate_python_function_resource(
            "functions/handoff.py",
            "@func_parameter('customer', 'Customer')\ndef handoff(conv, customer: list):\n    return None\n",
        )
        .expect_err("unsupported annotation");

        assert_eq!(error.path(), "functions/handoff.py");
        assert!(error.detail().contains("unsupported type annotation"));
    }

    #[test]
    fn flow_transition_functions_allow_user_parameters_after_receivers() {
        let errors = validate_flow_scoped_function_resource(
            "flows/sales/functions/route.py",
            "def route(conv: Conversation, flow: Flow, customer: str):\n    return None\n",
            true,
        )
        .expect("valid function");

        assert!(errors.is_empty());
    }
}

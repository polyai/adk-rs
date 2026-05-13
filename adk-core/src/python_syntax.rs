use std::fmt;

pub(crate) fn validate_python_module(source: &str) -> Result<(), PythonSyntaxError> {
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
pub(crate) struct PythonSyntaxError {
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
}

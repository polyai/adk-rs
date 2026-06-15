use crate::CommandGenError;
use crate::languages::local::LanguagesFile;
use crate::materialization::{insert_content_resource, to_yaml_string};
use crate::push_command_inputs::json_str;
use crate::specs::{ADDITIONAL_LANGUAGES, LANGUAGES_FILE};
use adk_types::ResourceMap;
use serde_json::Value;

pub(crate) fn insert_language_resources(
    map: &mut ResourceMap,
    projection: &Value,
) -> Result<(), CommandGenError> {
    let default_language = projection
        .pointer("/languages/defaultLanguageCode")
        .or_else(|| projection.pointer("/languages/defaultLanguage"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let additional_languages = ADDITIONAL_LANGUAGES
        .entries(projection)
        .into_iter()
        .map(|(id, language)| {
            let code = json_str(language, &["code"]);
            if code.is_empty() { id } else { code }
        })
        .collect::<Vec<_>>();

    if default_language.is_none() && additional_languages.is_empty() {
        return Ok(());
    }

    let content = to_yaml_string(&LanguagesFile::new(default_language, additional_languages))
        .map_err(|error| CommandGenError::InvalidData(error.to_string()))?;
    insert_content_resource(
        map,
        LANGUAGES_FILE.file_path,
        LANGUAGES_FILE.resource_id,
        LANGUAGES_FILE.name,
        content,
    )
}

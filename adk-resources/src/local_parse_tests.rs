use crate::asr_settings::AsrSettings;
use crate::entities::Entity;
use crate::handoffs::Handoff;
use crate::local_parse::ParseLocalResource;
use crate::transcript_corrections::TranscriptCorrection;
use crate::variants::{Variant, VariantAttribute};

fn parse_errors<R: ParseLocalResource>(path: &str, content: &str) -> Vec<String> {
    match R::parse_local_content(path, content) {
        Ok(_) => panic!("fixture should fail parsing"),
        Err(errors) => errors.into_validation_errors(),
    }
}

fn assert_parse_error<R: ParseLocalResource>(path: &str, content: &str, needle: &str) {
    let errors = parse_errors::<R>(path, content);
    assert!(
        errors.iter().any(|error| error.contains(needle)),
        "expected parse error containing {needle:?}, got {errors:?}"
    );
}

#[test]
fn local_parse_matrix_covers_resource_scoped_validation_rules() {
    assert_parse_error::<AsrSettings>(
        "voice/speech_recognition/asr_settings.yaml",
        "barge_in: false\ninteraction_style: warp\n",
        "unknown variant `warp`",
    );

    assert_parse_error::<Handoff>(
        "config/handoffs.yaml",
        r#"
handoffs:
  - name: Primary
    is_default: true
    sip_config:
      method: transfer
"#,
        "Invalid SIP method 'transfer'",
    );
    assert_parse_error::<Handoff>(
        "config/handoffs.yaml",
        r#"
handoffs:
  - name: Primary
    is_default: false
"#,
        "Multiple or zero default handoffs detected",
    );

    assert_parse_error::<TranscriptCorrection>(
        "voice/speech_recognition/transcript_corrections.yaml",
        r#"
corrections:
  - name: Fix alpha
    regular_expressions: []
"#,
        "At least one regular expression rule is required",
    );
    assert_parse_error::<TranscriptCorrection>(
        "voice/speech_recognition/transcript_corrections.yaml",
        r#"
corrections:
  - name: Fix alpha
    regular_expressions:
      - regular_expression: abc
        replacement_type: typo
"#,
        "unknown variant `typo`",
    );

    assert_parse_error::<Entity>(
        "config/entities.yaml",
        r#"
entities:
  - name: Amount
    entity_type: numeric
    config:
      has_decimal: "yes"
"#,
        "invalid type: string \"yes\", expected a boolean",
    );
    assert_parse_error::<Entity>(
        "config/entities.yaml",
        r#"
entities:
  - name: Bad type
    entity_type: unsupported
"#,
        "unknown variant `unsupported`",
    );

    assert_parse_error::<Variant>(
        "config/variant_attributes.yaml",
        r#"
variants:
  - name: Control
attributes: []
"#,
        "Multiple or zero default variants detected",
    );
    assert_parse_error::<VariantAttribute>(
        "config/variant_attributes.yaml",
        r#"
variants:
  - name: Control
    is_default: true
  - name: Treatment
attributes:
  - name: Channel
    values:
      Control: primary
"#,
        "Missing variants for variant attribute",
    );
}

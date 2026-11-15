//! Admission of source-provided presentation values before they enter trusted configuration.
//!
//! These fields are interpolated as individual CSS values by several renderers. This boundary
//! accepts values, never declarations or stylesheets. Host configuration and output-consumer CSS
//! policies have separate owners; this is deliberately not a stylesheet sanitizer.

use crate::{OperationControl, OperationControlResult};
use cssparser::{BasicParseErrorKind, ParseError, Parser, Token};
use serde_json::Value;

pub(super) fn filter(value: &mut Value, control: &OperationControl) -> OperationControlResult<()> {
    let mut stack = vec![(value, false)];
    while let Some((current, presentation)) = stack.pop() {
        control.checkpoint()?;
        match current {
            Value::Object(map) => {
                let rejected = map
                    .iter()
                    .filter(|(key, child)| {
                        (presentation || is_presentation_field(key))
                            && child.as_str().is_some_and(|value| !is_css_value(value))
                    })
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                for key in rejected {
                    if let Some(old) = map.remove(&key) {
                        super::drop_value_nonrecursive(old);
                    }
                }
                for (key, child) in map.iter_mut() {
                    stack.push((child, presentation || is_presentation_field(key)));
                }
            }
            Value::Array(items) => {
                // Keep array positions: a rejected font candidate must not authorize another
                // candidate by shifting an index in a family-specific configuration array.
                for child in items {
                    if presentation && child.as_str().is_some_and(|value| !is_css_value(value)) {
                        *child = Value::Null;
                    } else {
                        stack.push((child, presentation));
                    }
                }
            }
            _ => {}
        }
    }
    control.checkpoint()
}

pub(crate) fn is_presentation_field(key: &str) -> bool {
    key == "themeVariables"
        || [
            "fontFamily",
            "fontWeight",
            "fontSize",
            "FontFamily",
            "FontWeight",
            "FontSize",
        ]
        .iter()
        .any(|suffix| key.ends_with(suffix))
}

fn is_css_value(value: &str) -> bool {
    if value.chars().any(|ch| {
        matches!(ch, '<' | '>' | '&') || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    }) || crate::entities::restore_mermaid_entity_spelling(value).as_ref() != value
    {
        return false;
    }

    // CSS tokenizers recover EOF by closing strings and functions. A real delimiter following
    // the value must remain at the top level, or the original value could consume renderer CSS.
    let terminated = format!("{value}\n;");
    let mut parser = Parser::new(&terminated);
    components(&mut parser, 0, value.len() + 1).is_ok()
}

fn components(
    parser: &mut Parser<'_>,
    depth: usize,
    boundary: usize,
) -> Result<(), ParseError<()>> {
    loop {
        let start = parser.position().byte_index();
        let token = match parser.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if depth > 0 && matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        match token {
            Token::Semicolon if depth == 0 && start == boundary => {
                return parser.expect_exhausted().map_err(Into::into);
            }
            Token::Ident(_)
            | Token::Hash(_)
            | Token::IDHash(_)
            | Token::QuotedString(_)
            | Token::Number { .. }
            | Token::Percentage { .. }
            | Token::Dimension { .. }
            | Token::WhiteSpace(_)
            | Token::Comma
            | Token::Delim('+' | '-' | '*' | '/') => {}
            Token::Function(name) if depth < 32 && is_value_function(&name) => {
                parser.parse_nested_block(|nested| components(nested, depth + 1, boundary))?;
            }
            Token::ParenthesisBlock if depth > 0 && depth < 32 => {
                parser.parse_nested_block(|nested| components(nested, depth + 1, boundary))?;
            }
            _ => return Err(ParseError::custom(())),
        }
    }
}

fn is_value_function(name: &str) -> bool {
    // Resource loading and host CSS substitution are not source presentation capabilities.
    // cssparser decodes escaped names before this comparison, including escaped `url` spellings.
    [
        "rgb",
        "rgba",
        "hsl",
        "hsla",
        "hwb",
        "lab",
        "lch",
        "oklab",
        "oklch",
        "color",
        "color-mix",
        "light-dark",
        "calc",
        "min",
        "max",
        "clamp",
        "drop-shadow",
    ]
    .iter()
    .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_complete_literal_presentation_values() {
        for value in [
            "#181818",
            "hsl(-120, 0%, 9.4%)",
            "rgb(1 2 3 / 50%)",
            "18px",
            "600",
            "Inter, sans-serif",
            "\"Noto Sans CJK SC\", sans-serif",
            "宋体",
            "calc(100% - (2 * 1px))",
            "drop-shadow(1px 2px 2px rgba(185,185,185,1))",
        ] {
            assert!(is_css_value(value), "{value}");
        }
    }

    #[test]
    fn rejects_css_escape_resource_and_entity_injection() {
        for value in [
            "red;stroke:blue",
            "x;a{b} :not(&){background:green !important} c{d}",
            "</style><script>alert(1)</script>",
            "url(https://example.com/x)",
            "URL('#x')",
            r"u\72l('#x')",
            "image-set('https://example.com/x' 1x)",
            "var(--host-value)",
            "attr(data-value)",
            "rgb(1,2,3",
            "calc((1px)",
            "'unterminated",
            "'escaped\\'",
            "red/*",
            "red!important",
            "red#59;stroke:blue",
            "'font#34;'",
            "'font&#34;'",
            "'fontﬂ°quot¶ß'",
            "'font¶ß'",
            "red\0",
        ] {
            assert!(!is_css_value(value), "{value}");
        }
        assert!(!is_css_value(&format!(
            "{}1px{}",
            "calc(".repeat(33),
            ")".repeat(33)
        )));
    }

    #[test]
    fn value_boundary_rejects_nested_resources_and_unclosed_or_extra_delimiters() {
        for value in [
            "rgb(1 2 3))",
            "calc(1px + url(x))",
            "rgb(1 2 3)/*",
            "red\\",
            "red]",
            "red}",
            "rgb(1 2 3)/* closed */",
        ] {
            assert!(!is_css_value(value), "{value}");
        }
        assert!(is_css_value(r#""A \"Quoted\" Font", serif"#));
        assert!(is_css_value("rgb(1\r\n2\t3)"));
    }

    #[test]
    fn nested_arrays_cannot_bypass_value_admission_or_shift_positions() {
        let mut config = serde_json::json!({
            "sequence": { "actorFontFamily": ["red;stroke:blue", "Inter"] },
            "themeVariables": { "nested": [["url(x)", "#123456"]] }
        });
        filter(&mut config, &OperationControl::new()).unwrap();
        assert_eq!(
            config["sequence"]["actorFontFamily"],
            serde_json::json!([null, "Inter"])
        );
        assert_eq!(
            config["themeVariables"]["nested"],
            serde_json::json!([[null, "#123456"]])
        );
    }
}

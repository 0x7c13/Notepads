use crate::MermaidConfig;
#[cfg(any(
    feature = "diagram-cynefin",
    feature = "diagram-event-modeling",
    feature = "diagram-git-graph",
    feature = "diagram-packet",
    feature = "diagram-pie",
    feature = "diagram-radar",
    feature = "diagram-wardley"
))]
use crate::diagrams::langium_common::{LangiumCommonFacts, LangiumCommonField};
use crate::diagrams::scan::is_ecmascript_whitespace;
use crate::sanitize::sanitize_text;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg(any(
    feature = "diagram-cynefin",
    feature = "diagram-event-modeling",
    feature = "diagram-git-graph",
    feature = "diagram-packet",
    feature = "diagram-pie",
    feature = "diagram-radar",
    feature = "diagram-wardley"
))]
pub(crate) struct LangiumCommonDbFields {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
}

#[cfg(any(
    feature = "diagram-cynefin",
    feature = "diagram-event-modeling",
    feature = "diagram-git-graph",
    feature = "diagram-packet",
    feature = "diagram-pie",
    feature = "diagram-radar",
    feature = "diagram-wardley"
))]
impl LangiumCommonDbFields {
    pub(crate) fn from_facts(facts: &LangiumCommonFacts) -> Self {
        // `populateCommonDb` observes the final AST assignments in this order and ignores empty
        // values. Looking up each field independently also preserves Langium's last-assignment
        // semantics when a common fragment appears more than once.
        let acc_descr = nonempty_last(facts, LangiumCommonField::AccDescr);
        let acc_title = nonempty_last(facts, LangiumCommonField::AccTitle);
        let title = nonempty_last(facts, LangiumCommonField::Title);
        Self {
            title,
            acc_title,
            acc_descr,
        }
    }
}

#[cfg(any(
    feature = "diagram-cynefin",
    feature = "diagram-event-modeling",
    feature = "diagram-git-graph",
    feature = "diagram-packet",
    feature = "diagram-pie",
    feature = "diagram-radar",
    feature = "diagram-wardley"
))]
fn nonempty_last(facts: &LangiumCommonFacts, field: LangiumCommonField) -> Option<String> {
    facts
        .last(field)
        .map(|fact| fact.value.clone())
        .filter(|value| !value.is_empty())
}

fn strip_leading_whitespace(s: &str) -> String {
    s.trim_start_matches(is_ecmascript_whitespace).to_string()
}

fn collapse_newline_whitespace(s: &str) -> String {
    // Mermaid's commonDb.ts: `sanitizeText(txt).replace(/\n\s+/g, '\n')`
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' {
            while chars.peek().is_some_and(|c| is_ecmascript_whitespace(*c)) {
                chars.next();
            }
        }
    }
    out
}

fn sanitize_string_field<F>(obj: &mut Map<String, Value>, key: &'static str, transform: F)
where
    F: FnOnce(&str) -> String,
{
    let Some(v) = obj.get_mut(key) else {
        return;
    };
    let Value::String(s) = v else {
        return;
    };
    *s = transform(s);
}

pub fn apply_common_db_sanitization(model: &mut Value, config: &MermaidConfig) {
    let Value::Object(obj) = model else {
        return;
    };

    sanitize_string_field(obj, "title", |s| sanitize_text(s, config));
    sanitize_string_field(obj, "accTitle", |s| sanitize_acc_title(s, config));
    sanitize_string_field(obj, "accDescr", |s| sanitize_acc_descr(s, config));
}

pub(crate) fn sanitize_acc_title(s: &str, config: &MermaidConfig) -> String {
    strip_leading_whitespace(&sanitize_text(s, config))
}

pub(crate) fn sanitize_acc_descr(s: &str, config: &MermaidConfig) -> String {
    collapse_newline_whitespace(&sanitize_text(s, config))
}

#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) fn sanitize_optional_title(value: &mut Option<String>, config: &MermaidConfig) {
    if let Some(s) = value.as_deref() {
        *value = Some(sanitize_text(s, config));
    }
}

#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) fn sanitize_optional_acc_title(value: &mut Option<String>, config: &MermaidConfig) {
    if let Some(s) = value.as_deref() {
        *value = Some(sanitize_acc_title(s, config));
    }
}

#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) fn sanitize_optional_acc_descr(value: &mut Option<String>, config: &MermaidConfig) {
    if let Some(s) = value.as_deref() {
        *value = Some(sanitize_acc_descr(s, config));
    }
}

#[cfg(test)]
mod tests {
    use super::{collapse_newline_whitespace, strip_leading_whitespace};

    #[test]
    fn accessibility_whitespace_matches_common_db_javascript_regexes() {
        assert_eq!(strip_leading_whitespace("\u{feff}\u{00a0}Title "), "Title ");
        assert_eq!(strip_leading_whitespace("\u{0085}Title"), "\u{0085}Title");
        assert_eq!(
            collapse_newline_whitespace("First\n\u{feff}\u{00a0}Second"),
            "First\nSecond"
        );
        assert_eq!(
            collapse_newline_whitespace("First\n\u{0085}Second"),
            "First\n\u{0085}Second"
        );
    }
}

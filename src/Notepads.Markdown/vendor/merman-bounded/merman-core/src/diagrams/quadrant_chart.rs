use crate::diagrams::scan::LineCursor;
use crate::sanitize::sanitize_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, MermaidConfig, OperationControl, OperationControlResult,
    ParseMetadata, Result, SourceSpan, family::CombinedSemanticFailure,
};
use serde_json::{Map, Value, json};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

#[cfg(test)]
thread_local! {
    static QUADRANT_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_quadrant_syntax_construction_count() {
    QUADRANT_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
fn quadrant_syntax_construction_count() -> usize {
    QUADRANT_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuadrantChartStyles {
    pub radius: Option<i64>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuadrantChartPointModel {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub class_name: Option<String>,
    pub styles: QuadrantChartStyles,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuadrantChartQuadrantsModel {
    pub quadrant1_text: String,
    pub quadrant2_text: String,
    pub quadrant3_text: String,
    pub quadrant4_text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuadrantChartAxesModel {
    pub x_axis_left_text: String,
    pub x_axis_right_text: String,
    pub y_axis_bottom_text: String,
    pub y_axis_top_text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuadrantChartRenderModel {
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    pub quadrants: QuadrantChartQuadrantsModel,
    pub axes: QuadrantChartAxesModel,
    pub points: Vec<QuadrantChartPointModel>,
    pub classes: BTreeMap<String, QuadrantChartStyles>,
}

impl QuadrantChartRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Default)]
struct QuadrantDb {
    quadrant1_text: String,
    quadrant2_text: String,
    quadrant3_text: String,
    quadrant4_text: String,
    x_axis_left_text: String,
    x_axis_right_text: String,
    y_axis_bottom_text: String,
    y_axis_top_text: String,
    points: Vec<QuadrantChartPointModel>,
    classes: HashMap<String, QuadrantChartStyles>,
}

impl QuadrantDb {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set_quadrant_text(&mut self, idx: u8, text: &str, config: &MermaidConfig) {
        let t = sanitize_text(text.trim(), config);
        match idx {
            1 => self.quadrant1_text = t,
            2 => self.quadrant2_text = t,
            3 => self.quadrant3_text = t,
            4 => self.quadrant4_text = t,
            _ => {}
        }
    }

    fn set_x_axis_left(&mut self, text: &str, config: &MermaidConfig) {
        self.x_axis_left_text = sanitize_text(text.trim(), config);
    }

    fn set_x_axis_right(&mut self, text: &str, config: &MermaidConfig) {
        self.x_axis_right_text = sanitize_text(text.trim(), config);
    }

    fn set_y_axis_bottom(&mut self, text: &str, config: &MermaidConfig) {
        self.y_axis_bottom_text = sanitize_text(text.trim(), config);
    }

    fn set_y_axis_top(&mut self, text: &str, config: &MermaidConfig) {
        self.y_axis_top_text = sanitize_text(text.trim(), config);
    }

    fn add_class(&mut self, class_name: &str, styles: &[String]) -> Result<()> {
        let parsed = parse_styles(styles)?;
        self.classes.insert(class_name.to_string(), parsed);
        Ok(())
    }

    fn add_point(
        &mut self,
        text: &str,
        class_name: Option<String>,
        x: f64,
        y: f64,
        styles: &[String],
        config: &MermaidConfig,
    ) -> Result<()> {
        let styles_obj = parse_styles(styles)?;
        let text = sanitize_text(text.trim(), config);
        let p = QuadrantChartPointModel {
            text,
            x,
            y,
            class_name,
            styles: styles_obj,
        };
        self.points.insert(0, p);
        Ok(())
    }
}

struct QuadrantSemanticSource {
    model: QuadrantChartRenderModel,
    editor_facts: EditorSemanticFacts,
}

fn parse_styles(styles: &[String]) -> Result<QuadrantChartStyles> {
    let mut out = QuadrantChartStyles::default();
    for raw in styles {
        let style = raw.trim();
        if style.is_empty() {
            continue;
        }
        let (key, value) = style.split_once(':').ok_or_else(|| {
            Error::diagram_parse_fallback(
                "quadrantChart".to_string(),
                format!("style named {style} is not supported."),
            )
        })?;
        let key = key.trim();
        let value = value.trim();

        match key {
            "radius" => {
                if !value.chars().all(|c| c.is_ascii_digit()) {
                    return Err(Error::diagram_parse_fallback(
                        "quadrantChart".to_string(),
                        format!("value for {key} {value} is invalid, please use a valid number"),
                    ));
                }
                out.radius = Some(value.parse::<i64>().map_err(|e| {
                    Error::diagram_parse_fallback("quadrantChart".to_string(), e.to_string())
                })?);
            }
            "color" => {
                if !is_valid_hex_code(value) {
                    return Err(Error::diagram_parse_fallback(
                        "quadrantChart".to_string(),
                        format!("value for {key} {value} is invalid, please use a valid hex code"),
                    ));
                }
                out.color = Some(value.to_string());
            }
            "stroke-color" => {
                if !is_valid_hex_code(value) {
                    return Err(Error::diagram_parse_fallback(
                        "quadrantChart".to_string(),
                        format!("value for {key} {value} is invalid, please use a valid hex code"),
                    ));
                }
                out.stroke_color = Some(value.to_string());
            }
            "stroke-width" => {
                if !is_valid_px(value) {
                    return Err(Error::diagram_parse_fallback(
                        "quadrantChart".to_string(),
                        format!(
                            "value for {key} {value} is invalid, please use a valid number of pixels (eg. 10px)"
                        ),
                    ));
                }
                out.stroke_width = Some(value.to_string());
            }
            _ => {
                return Err(Error::diagram_parse_fallback(
                    "quadrantChart".to_string(),
                    format!("style named {key} is not supported."),
                ));
            }
        }
    }
    Ok(out)
}

fn is_valid_hex_code(value: &str) -> bool {
    let v = value.strip_prefix('#').unwrap_or(value);
    (v.len() == 3 || v.len() == 6) && v.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_valid_px(value: &str) -> bool {
    let Some(num) = value.strip_suffix("px") else {
        return false;
    };
    !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
}

fn next_char_at(s: &str, idx: usize) -> Option<char> {
    s.get(idx..)?.chars().next()
}

fn strip_inline_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut brace_depth = 0usize;
    let mut i = 0usize;
    while i + 1 < line.len() {
        let Some(ch) = next_char_at(line, i) else {
            break;
        };
        if ch == '"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && ch == '{' {
            brace_depth += 1;
        } else if !in_quotes && ch == '}' {
            brace_depth = brace_depth.saturating_sub(1);
        }
        if !in_quotes && brace_depth == 0 && line[i..].starts_with("%%") {
            return &line[..i];
        }
        i += ch.len_utf8();
    }
    line
}

fn is_axis_delim_at(s: &str, idx: usize) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    if idx >= bytes.len() {
        return None;
    }
    if bytes[idx] != b'-' {
        return None;
    }
    let mut j = idx;
    let mut dash_count = 0usize;
    while j < bytes.len() && bytes[j] == b'-' {
        dash_count += 1;
        j += 1;
    }
    if dash_count < 2 {
        return None;
    }
    if j < bytes.len() && bytes[j] == b'>' {
        Some((idx, j + 1))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceSlice<'source> {
    text: &'source str,
    start: usize,
}

impl<'source> SourceSlice<'source> {
    fn new(text: &'source str, start: usize) -> Self {
        Self { text, start }
    }

    fn end(self) -> usize {
        self.start + self.text.len()
    }

    fn span(self) -> SourceSpan {
        SourceSpan::new(self.start, self.end())
    }

    fn trim_start(self) -> Self {
        let text = self.text.trim_start();
        Self::new(text, self.end().saturating_sub(text.len()))
    }

    fn trim_end(self) -> Self {
        Self::new(self.text.trim_end(), self.start)
    }

    fn trim(self) -> Self {
        self.trim_start().trim_end()
    }

    fn subslice(self, start: usize, end: usize) -> Self {
        Self::new(&self.text[start..end], self.start + start)
    }
}

#[derive(Debug, Clone)]
struct ParsedText<'source> {
    value: String,
    content: SourceSlice<'source>,
}

fn parse_text_slice(input: SourceSlice<'_>) -> Result<ParsedText<'_>> {
    let input = input.trim();
    if input.text.starts_with("\"`") {
        if input.text.len() < 4 || !input.text.ends_with("`\"") {
            return Err(Error::diagram_parse_fallback(
                "quadrantChart".to_string(),
                "unterminated markdown string".to_string(),
            ));
        }
        let content = input.subslice(2, input.text.len() - 2);
        return Ok(ParsedText {
            value: content.text.to_string(),
            content,
        });
    }
    if input.text.starts_with('"') {
        if input.text.len() < 2 || !input.text.ends_with('"') {
            return Err(Error::diagram_parse_fallback(
                "quadrantChart".to_string(),
                "unterminated string".to_string(),
            ));
        }
        let content = input.subslice(1, input.text.len() - 1);
        return Ok(ParsedText {
            value: content.text.to_string(),
            content,
        });
    }
    Ok(ParsedText {
        value: input.text.to_string(),
        content: input,
    })
}

struct ParsedAxis<'source> {
    left: ParsedText<'source>,
    right: Option<ParsedText<'source>>,
    operator: Option<SourceSpan>,
}

fn parse_axis_text(input: SourceSlice<'_>) -> Result<ParsedAxis<'_>> {
    let mut in_quotes = false;
    let mut index = 0usize;
    while index < input.text.len() {
        let Some(ch) = next_char_at(input.text, index) else {
            break;
        };
        if ch == '"' {
            in_quotes = !in_quotes;
            index += ch.len_utf8();
            continue;
        }
        if !in_quotes && let Some((start, end)) = is_axis_delim_at(input.text, index) {
            let left = parse_text_slice(input.subslice(0, start))?;
            let right = input.subslice(end, input.text.len()).trim();
            return Ok(ParsedAxis {
                left,
                right: (!right.text.is_empty())
                    .then(|| parse_text_slice(right))
                    .transpose()?,
                operator: Some(SourceSpan::new(input.start + start, input.start + end)),
            });
        }
        index += ch.len_utf8();
    }
    Ok(ParsedAxis {
        left: parse_text_slice(input)?,
        right: None,
        operator: None,
    })
}

fn parse_unit_interval_token(raw: &str) -> Result<f64> {
    let s = raw.trim();
    if s == "1" {
        return Ok(1.0);
    }
    if s == "0" {
        return Ok(0.0);
    }
    if let Some(rest) = s.strip_prefix("0.")
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return s.parse::<f64>().map_err(|e| {
            Error::diagram_parse_fallback("quadrantChart".to_string(), e.to_string())
        });
    }
    Err(Error::diagram_parse_fallback(
        "quadrantChart".to_string(),
        "invalid point coordinate".to_string(),
    ))
}

struct ParsedStyleList<'source> {
    items: Vec<SourceSlice<'source>>,
}

fn split_style_slices(input: SourceSlice<'_>) -> ParsedStyleList<'_> {
    let mut styles = Vec::new();
    let mut start = 0usize;
    for (index, ch) in input.text.char_indices() {
        if ch == ',' {
            let style = input.subslice(start, index).trim();
            if !style.text.is_empty() {
                styles.push(style);
            }
            start = index + ch.len_utf8();
        }
    }
    let style = input.subslice(start, input.text.len()).trim();
    if !style.text.is_empty() {
        styles.push(style);
    }
    ParsedStyleList { items: styles }
}

fn find_point_colon(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut i = 0usize;
    while i < s.len() {
        let Some(ch) = next_char_at(s, i) else {
            break;
        };
        if ch == '"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && ch == ':' {
            let mut j = i + 1;
            while j < s.len() {
                let Some(c2) = next_char_at(s, j) else {
                    break;
                };
                if c2.is_whitespace() {
                    j += c2.len_utf8();
                    continue;
                }
                if c2 == '[' {
                    return Some(i);
                }
                break;
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn find_class_marker(input: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut last = None;
    let mut index = 0usize;
    while index < input.len() {
        let Some(ch) = next_char_at(input, index) else {
            break;
        };
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && input[index..].starts_with(":::") {
            last = Some(index);
            index += 3;
            continue;
        }
        index += ch.len_utf8();
    }
    last
}

struct ParsedPoint<'source> {
    label: ParsedText<'source>,
    class_name: Option<SourceSlice<'source>>,
    class_marker: Option<SourceSpan>,
    x: f64,
    y: f64,
    styles: Vec<SourceSlice<'source>>,
}

fn parse_point_statement(statement: SourceSlice<'_>) -> Result<Option<ParsedPoint<'_>>> {
    let Some(colon_idx) = find_point_colon(statement.text) else {
        return Ok(None);
    };
    let head = statement.subslice(0, colon_idx).trim_end();
    let tail = statement
        .subslice(colon_idx + 1, statement.text.len())
        .trim_start();

    let (class_name, class_marker, label_input) = if let Some(marker) = find_class_marker(head.text)
    {
        let class = head.subslice(marker + 3, head.text.len()).trim();
        if class
            .text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            (
                (!class.text.is_empty()).then_some(class),
                Some(SourceSpan::new(
                    head.start + marker,
                    head.start + marker + 3,
                )),
                head.subslice(0, marker).trim_end(),
            )
        } else {
            (None, None, head)
        }
    } else {
        (None, None, head)
    };
    let label = parse_text_slice(label_input)?;

    if !tail.text.starts_with('[') {
        return Err(Error::diagram_parse_fallback(
            "quadrantChart".to_string(),
            "expected '[' after ':'".to_string(),
        ));
    }
    let after_bracket = tail.subslice(1, tail.text.len());
    let Some(close_rel) = after_bracket.text.find(']') else {
        return Err(Error::diagram_parse_fallback(
            "quadrantChart".to_string(),
            "unterminated point coordinate; missing ']'".to_string(),
        ));
    };
    let inside = after_bracket.subslice(0, close_rel);
    let after = after_bracket.subslice(close_rel + 1, after_bracket.text.len());
    let Some(comma_rel) = inside.text.find(',') else {
        return Err(Error::diagram_parse_fallback(
            "quadrantChart".to_string(),
            "invalid point coordinate".to_string(),
        ));
    };
    if inside.text[comma_rel + 1..].contains(',') {
        return Err(Error::diagram_parse_fallback(
            "quadrantChart".to_string(),
            "invalid point coordinate".to_string(),
        ));
    }
    let x_token = inside.subslice(0, comma_rel).trim();
    let y_token = inside.subslice(comma_rel + 1, inside.text.len()).trim();
    let x = parse_unit_interval_token(x_token.text)?;
    let y = parse_unit_interval_token(y_token.text)?;

    let styles = split_style_slices(after);
    Ok(Some(ParsedPoint {
        label,
        class_name,
        class_marker,
        x,
        y,
        styles: styles.items,
    }))
}

struct KeywordRest<'source> {
    rest: SourceSlice<'source>,
}

fn parse_keyword_rest_ci<'source>(
    input: SourceSlice<'source>,
    key: &str,
) -> Option<KeywordRest<'source>> {
    let input = input.trim_start();
    let keyword = input.text.get(..key.len())?;
    if !keyword.eq_ignore_ascii_case(key) {
        return None;
    }
    Some(KeywordRest {
        rest: input.subslice(key.len(), input.text.len()).trim_start(),
    })
}

struct ColonDirective<'source> {
    value: SourceSlice<'source>,
}

fn parse_colon_value_ci<'source>(
    input: SourceSlice<'source>,
    key: &str,
) -> Option<ColonDirective<'source>> {
    let matched = parse_keyword_rest_ci(input, key)?;
    if !matched.rest.text.starts_with(':') {
        return None;
    }
    Some(ColonDirective {
        value: matched.rest.subslice(1, matched.rest.text.len()).trim(),
    })
}

struct QuadrantStatement<'source> {
    source: SourceSlice<'source>,
}

fn split_semicolons_spanned(line: SourceSlice<'_>) -> Vec<QuadrantStatement<'_>> {
    let mut out = Vec::new();
    let mut in_quotes = false;
    let mut brace_depth = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < line.text.len() {
        let Some(ch) = next_char_at(line.text, i) else {
            break;
        };
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && ch == '{' {
            brace_depth += 1;
        } else if !in_quotes && ch == '}' {
            brace_depth = brace_depth.saturating_sub(1);
        } else if !in_quotes && brace_depth == 0 && ch == ';' {
            out.push(QuadrantStatement {
                source: line.subslice(start, i),
            });
            start = i + 1;
        }
        i += ch.len_utf8();
    }
    out.push(QuadrantStatement {
        source: line.subslice(start, line.text.len()),
    });
    out
}

fn push_quadrant_payload_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    start: usize,
    end: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    let span = SourceSpan::new(start, end);
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        text.to_string(),
        Some(detail.to_string()),
        kind,
        span,
        span,
    ));
}

fn push_quadrant_outline_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    start: usize,
    end: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    let span = SourceSpan::new(start, end);
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::outline(
        text.to_string(),
        Some(detail.to_string()),
        kind,
        span,
        span,
    ));
}

struct QuadrantAccDescrBlock {
    text: String,
    source_start: usize,
}

fn quadrant_error_at(error: Error, meta: &ParseMetadata, span: SourceSpan) -> Error {
    let message = match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
        error => error.to_string(),
    };
    Error::diagram_parse_exact(meta.diagram_type.clone(), message, span)
}

fn push_quadrant_class_fact(
    facts: &mut EditorSemanticFacts,
    statement_span: SourceSpan,
    name: SourceSlice<'_>,
) {
    let span = name.span();
    facts.push_directive_prefix("classDef");
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::class_definition(
        name.text.to_string(),
        Some("quadrant chart class".to_string()),
        EditorSemanticKind::Class,
        statement_span,
        span,
    ));
}

fn push_quadrant_point_facts(
    facts: &mut EditorSemanticFacts,
    statement_span: SourceSpan,
    point: &ParsedPoint<'_>,
) {
    if !point.label.content.text.is_empty() {
        let span = point.label.content.span();
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            span,
        ));
        facts.push_symbol(EditorSemanticSymbol::outline(
            point.label.value.clone(),
            Some("quadrant chart point".to_string()),
            EditorSemanticKind::Object,
            statement_span,
            span,
        ));
    }

    let Some(class_marker) = point.class_marker else {
        return;
    };
    let class_span = point
        .class_name
        .map(SourceSlice::span)
        .unwrap_or_else(|| SourceSpan::new(class_marker.end, class_marker.end));
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::ClassName,
        class_span,
    ));

    let Some(class_name) = point.class_name else {
        return;
    };
    facts.push_symbol(EditorSemanticSymbol::payload(
        class_name.text.to_string(),
        Some("quadrant chart class".to_string()),
        EditorSemanticKind::Class,
        statement_span,
        class_name.span(),
    ));
}

fn construct_quadrant_chart_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<QuadrantSemanticSource, CombinedSemanticFailure> {
    construct_quadrant_chart_semantic_source_controlled(code, meta, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_quadrant_chart_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<QuadrantSemanticSource, CombinedSemanticFailure>> {
    control.checkpoint()?;
    #[cfg(test)]
    QUADRANT_SYNTAX_CONSTRUCTION_COUNT.set(QUADRANT_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    parse_quadrant_chart_semantic_source(code, meta, control)
}

fn parse_quadrant_chart_semantic_source(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<QuadrantSemanticSource, CombinedSemanticFailure>> {
    control.checkpoint()?;
    let mut db = QuadrantDb::default();
    db.clear();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut title = None;
    let mut acc_title = None;
    let mut acc_descr = None;
    let mut saw_header = false;
    let mut acc_descr_block: Option<QuadrantAccDescrBlock> = None;
    let mut first_error = None;
    let mut lines = LineCursor::new(code);

    while let Some((segment, line_start)) = lines.next_line() {
        control.checkpoint()?;
        let line = SourceSlice::new(segment, line_start);

        if let Some(mut block) = acc_descr_block.take() {
            if let Some(end) = line.text.find('}') {
                block.text.push_str(&line.text[..end]);
                let text = block.text.trim().to_string();
                if !text.is_empty() {
                    push_quadrant_payload_fact(
                        &mut editor_facts,
                        &text,
                        block.source_start,
                        line.start + end,
                        "quadrant chart accessibility description",
                        EditorSemanticKind::String,
                    );
                }
                lines.resume_same_line_at(line.start + end + 1);
                acc_descr = Some(text);
            } else {
                block.text.push_str(line.text);
                block.text.push('\n');
                acc_descr_block = Some(block);
            }
            continue;
        }

        let stripped_text = strip_inline_comment(line.text);
        let stripped = line.subslice(0, stripped_text.len());

        if stripped.text.trim().is_empty() {
            continue;
        }

        for statement in split_semicolons_spanned(stripped) {
            control.checkpoint()?;
            let statement = statement.source;
            let statement_trimmed = statement.trim();
            if statement_trimmed.text.is_empty() || statement_trimmed.text.starts_with("%%") {
                continue;
            }
            let statement_span = statement_trimmed.span();
            let semantic_end = statement_span.end;

            if !saw_header {
                if statement_trimmed.text.eq_ignore_ascii_case("quadrantChart") {
                    saw_header = true;
                    continue;
                }
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_exact(
                        meta.diagram_type.clone(),
                        "expected quadrantChart",
                        statement_span,
                    )
                });
                continue;
            }

            if let Some(directive) = parse_colon_value_ci(statement_trimmed, "accTitle") {
                editor_facts.push_directive_prefix("accTitle");
                if let Ok(value) = parse_text_slice(directive.value) {
                    push_quadrant_payload_fact(
                        &mut editor_facts,
                        &value.value,
                        value.content.start,
                        value.content.end(),
                        "quadrant chart accessibility title",
                        EditorSemanticKind::String,
                    );
                }
                acc_title = Some(directive.value.text.to_string());
                continue;
            }

            if let Some(directive) = parse_keyword_rest_ci(statement_trimmed, "accDescr") {
                let rest = directive.rest;
                if rest.text.starts_with('{') {
                    editor_facts.push_directive_prefix("accDescr");
                    let content_start = rest.start + 1;
                    let after_brace = rest.subslice(1, rest.text.len());
                    if let Some(end) = after_brace.text.find('}') {
                        let content = after_brace.subslice(0, end);
                        let text = content.text.trim().to_string();
                        if !text.is_empty() {
                            push_quadrant_payload_fact(
                                &mut editor_facts,
                                &text,
                                content_start,
                                content.end(),
                                "quadrant chart accessibility description",
                                EditorSemanticKind::String,
                            );
                        }
                        let trailing = after_brace.subslice(end + 1, after_brace.text.len()).trim();
                        if !trailing.text.is_empty() {
                            first_error.get_or_insert_with(|| {
                                Error::diagram_parse_exact(
                                    meta.diagram_type.clone(),
                                    "expected ';' or newline after accDescr block",
                                    trailing.span(),
                                )
                            });
                        }
                        acc_descr = Some(text);
                    } else {
                        let mut text = after_brace.text.trim_start().to_string();
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        acc_descr_block = Some(QuadrantAccDescrBlock {
                            text,
                            source_start: content_start,
                        });
                    }
                    continue;
                }
                if rest.text.starts_with(':') {
                    let value = rest.subslice(1, rest.text.len()).trim();
                    editor_facts.push_directive_prefix("accDescr");
                    if let Ok(parsed) = parse_text_slice(value) {
                        push_quadrant_payload_fact(
                            &mut editor_facts,
                            &parsed.value,
                            parsed.content.start,
                            parsed.content.end(),
                            "quadrant chart accessibility description",
                            EditorSemanticKind::String,
                        );
                    }
                    acc_descr = Some(value.text.to_string());
                    continue;
                }
            }

            if let Some(directive) = parse_keyword_rest_ci(statement_trimmed, "title") {
                let value = directive.rest;
                editor_facts.push_directive_prefix("title");
                if let Ok(parsed) = parse_text_slice(value) {
                    push_quadrant_payload_fact(
                        &mut editor_facts,
                        &parsed.value,
                        parsed.content.start,
                        parsed.content.end(),
                        "quadrant chart title",
                        EditorSemanticKind::String,
                    );
                }
                title = Some(value.text.to_string());
                continue;
            }

            if let Some(directive) = parse_keyword_rest_ci(statement_trimmed, "x-axis") {
                let axis = match parse_axis_text(directive.rest) {
                    Ok(axis) => axis,
                    Err(error) => {
                        first_error
                            .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                        continue;
                    }
                };
                push_quadrant_outline_fact(
                    &mut editor_facts,
                    &axis.left.value,
                    axis.left.content.start,
                    axis.left.content.end(),
                    "quadrant chart x-axis",
                    EditorSemanticKind::String,
                );
                if let Some(right) = &axis.right {
                    push_quadrant_outline_fact(
                        &mut editor_facts,
                        &right.value,
                        right.content.start,
                        right.content.end(),
                        "quadrant chart x-axis",
                        EditorSemanticKind::String,
                    );
                }
                let mut left = axis.left.value;
                if axis.operator.is_some() && axis.right.is_none() {
                    left.push_str(" ⟶");
                }
                db.set_x_axis_left(&left, &meta.effective_config);
                if let Some(right) = axis.right {
                    db.set_x_axis_right(&right.value, &meta.effective_config);
                }
                continue;
            }

            if let Some(directive) = parse_keyword_rest_ci(statement_trimmed, "y-axis") {
                let axis = match parse_axis_text(directive.rest) {
                    Ok(axis) => axis,
                    Err(error) => {
                        first_error
                            .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                        continue;
                    }
                };
                push_quadrant_outline_fact(
                    &mut editor_facts,
                    &axis.left.value,
                    axis.left.content.start,
                    axis.left.content.end(),
                    "quadrant chart y-axis",
                    EditorSemanticKind::String,
                );
                if let Some(top) = &axis.right {
                    push_quadrant_outline_fact(
                        &mut editor_facts,
                        &top.value,
                        top.content.start,
                        top.content.end(),
                        "quadrant chart y-axis",
                        EditorSemanticKind::String,
                    );
                }
                let mut bottom = axis.left.value;
                if axis.operator.is_some() && axis.right.is_none() {
                    bottom.push_str(" ⟶");
                }
                db.set_y_axis_bottom(&bottom, &meta.effective_config);
                if let Some(top) = axis.right {
                    db.set_y_axis_top(&top.value, &meta.effective_config);
                }
                continue;
            }

            let mut matched_quadrant = false;
            for (index, keyword) in [
                (1u8, "quadrant-1"),
                (2, "quadrant-2"),
                (3, "quadrant-3"),
                (4, "quadrant-4"),
            ] {
                let Some(directive) = parse_keyword_rest_ci(statement_trimmed, keyword) else {
                    continue;
                };
                let text = match parse_text_slice(directive.rest) {
                    Ok(text) => text,
                    Err(error) => {
                        first_error
                            .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                        matched_quadrant = true;
                        break;
                    }
                };
                push_quadrant_outline_fact(
                    &mut editor_facts,
                    &text.value,
                    text.content.start,
                    text.content.end(),
                    "quadrant chart quadrant",
                    EditorSemanticKind::String,
                );
                db.set_quadrant_text(index, &text.value, &meta.effective_config);
                matched_quadrant = true;
                break;
            }
            if matched_quadrant {
                continue;
            }

            if let Some(directive) = parse_keyword_rest_ci(statement_trimmed, "classDef") {
                let rest = directive.rest;
                let name_end = rest
                    .text
                    .char_indices()
                    .find_map(|(index, ch)| ch.is_whitespace().then_some(index))
                    .unwrap_or(rest.text.len());
                let name = rest.subslice(0, name_end).trim();
                if name.text.is_empty() {
                    first_error.get_or_insert_with(|| {
                        Error::diagram_parse_insertion_point(
                            meta.diagram_type.clone(),
                            "expected classDef name",
                            semantic_end,
                        )
                    });
                    continue;
                }
                let style_text = rest.subslice(name_end, rest.text.len()).trim();
                let styles = split_style_slices(style_text);
                push_quadrant_class_fact(&mut editor_facts, statement_span, name);
                let style_values = styles
                    .items
                    .iter()
                    .map(|style| style.text.to_string())
                    .collect::<Vec<_>>();
                if let Err(error) = db.add_class(name.text, &style_values) {
                    first_error
                        .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                }
                continue;
            }

            match parse_point_statement(statement_trimmed) {
                Ok(Some(point)) => {
                    push_quadrant_point_facts(&mut editor_facts, statement_span, &point);
                    let class_name = point.class_name.map(|class| class.text.to_string());
                    let styles = point
                        .styles
                        .iter()
                        .map(|style| style.text.to_string())
                        .collect::<Vec<_>>();
                    if let Err(error) = db.add_point(
                        &point.label.value,
                        class_name,
                        point.x,
                        point.y,
                        &styles,
                        &meta.effective_config,
                    ) {
                        first_error
                            .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    first_error
                        .get_or_insert_with(|| quadrant_error_at(error, meta, statement_span));
                    continue;
                }
            }

            first_error.get_or_insert_with(|| {
                Error::diagram_parse_exact(
                    meta.diagram_type.clone(),
                    format!("Unrecognized statement: {}", statement_trimmed.text),
                    statement_span,
                )
            });
        }
    }

    if let Some(block) = acc_descr_block {
        let text = block.text.trim().to_string();
        if !text.is_empty() {
            push_quadrant_payload_fact(
                &mut editor_facts,
                &text,
                block.source_start,
                code.len(),
                "quadrant chart accessibility description",
                EditorSemanticKind::String,
            );
        }
        first_error.get_or_insert_with(|| {
            Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "unterminated accDescr block",
                code.len(),
            )
        });
    }

    if !saw_header {
        first_error.get_or_insert_with(|| {
            Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected quadrantChart",
                code.len(),
            )
        });
    }

    if let Some(error) = first_error {
        return Ok(Err(CombinedSemanticFailure::parser_recovery(
            "quadrant chart",
            error,
            editor_facts,
        )));
    }

    control.checkpoint()?;
    Ok(Ok(QuadrantSemanticSource {
        model: QuadrantChartRenderModel {
            title,
            acc_title,
            acc_descr,
            quadrants: QuadrantChartQuadrantsModel {
                quadrant1_text: db.quadrant1_text,
                quadrant2_text: db.quadrant2_text,
                quadrant3_text: db.quadrant3_text,
                quadrant4_text: db.quadrant4_text,
            },
            axes: QuadrantChartAxesModel {
                x_axis_left_text: db.x_axis_left_text,
                x_axis_right_text: db.x_axis_right_text,
                y_axis_bottom_text: db.y_axis_bottom_text,
                y_axis_top_text: db.y_axis_top_text,
            },
            points: db.points,
            classes: db.classes.into_iter().collect(),
        },
        editor_facts,
    }))
}

pub(crate) fn parse_quadrant_chart(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = construct_quadrant_chart_semantic_source(code, meta)
        .map_err(CombinedSemanticFailure::into_error)?;
    render_model_to_compat_json(&source.model, meta)
}

pub(crate) fn parse_quadrant_chart_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_quadrant_chart_semantic_source_controlled(code, meta, control)?;
    Ok(crate::family::CombinedSemanticParse::from_construction(
        construction,
        |QuadrantSemanticSource {
             model,
             editor_facts,
         }| (render_model_to_compat_json(&model, meta), editor_facts),
        CombinedSemanticFailure::into_parts,
    ))
}

pub(crate) fn render_model_to_compat_json(
    model: &QuadrantChartRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut out = Map::with_capacity(9);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("quadrants".to_string(), json!(&model.quadrants));
    out.insert("axes".to_string(), json!(&model.axes));
    out.insert("points".to_string(), json!(&model.points));
    out.insert("classes".to_string(), json!(&model.classes));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
pub(crate) fn parse_quadrant_chart_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<QuadrantChartRenderModel> {
    construct_quadrant_chart_semantic_source(code, meta)
        .map(|source| source.model)
        .map_err(CombinedSemanticFailure::into_error)
}

pub(crate) fn parse_quadrant_chart_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<QuadrantChartRenderModel>> {
    let construction = construct_quadrant_chart_semantic_source_controlled(code, meta, control)?;
    match construction {
        Ok(source) => Ok(Ok(source.model)),
        Err(failure) => Ok(Err(failure.into_error())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated;
    use crate::{
        EditorSemanticCompleteness, EditorSemanticDiagnosticKind, EditorSemanticRole, Engine,
        ParseOptions, RenderSemanticModel,
    };
    use futures::executor::block_on;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn parse_err(text: &str) -> String {
        let engine = Engine::new();
        match block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err() {
            Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
            other => other.to_string(),
        }
    }

    fn axes(model: &Value) -> &Value {
        &model["axes"]
    }

    fn quadrants(model: &Value) -> &Value {
        &model["quadrants"]
    }

    fn points(model: &Value) -> Vec<Value> {
        model["points"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn errors_without_header() {
        let meta = ParseMetadata {
            diagram_type: "quadrantChart".to_string(),
            config: MermaidConfig::default(),
            effective_config: generated::default_site_config(),
            title: None,
        };
        let err = parse_quadrant_chart("quadrant-1 do\n", &meta)
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected quadrantChart"));
    }

    #[test]
    fn header_only_is_allowed() {
        let model = parse("quadrantChart\n");
        assert_eq!(model["type"].as_str().unwrap(), "quadrantChart");
        assert!(model["title"].is_null());
    }

    #[test]
    fn parses_x_axis_text_and_missing_right_side() {
        let model = parse("quadrantChart\nx-axis urgent --> not urgent\n");
        assert_eq!(axes(&model)["xAxisLeftText"].as_str().unwrap(), "urgent");
        assert_eq!(
            axes(&model)["xAxisRightText"].as_str().unwrap(),
            "not urgent"
        );

        let model = parse("quadrantChart\nx-AxIs \"Urgent(* +=[?\"  --> \n");
        assert_eq!(
            axes(&model)["xAxisLeftText"].as_str().unwrap(),
            "Urgent(* +=[? ⟶"
        );
        assert_eq!(axes(&model)["xAxisRightText"].as_str().unwrap(), "");
    }

    #[test]
    fn parses_y_axis_text_and_missing_top_side() {
        let model = parse("quadrantChart\ny-axis urgent --> not urgent\n");
        assert_eq!(axes(&model)["yAxisBottomText"].as_str().unwrap(), "urgent");
        assert_eq!(axes(&model)["yAxisTopText"].as_str().unwrap(), "not urgent");

        let model = parse("quadrantChart\ny-AxIs \"Urgent(* +=[?\"  --> \n");
        assert_eq!(
            axes(&model)["yAxisBottomText"].as_str().unwrap(),
            "Urgent(* +=[? ⟶"
        );
        assert_eq!(axes(&model)["yAxisTopText"].as_str().unwrap(), "");
    }

    #[test]
    fn parses_quadrant_text_and_title() {
        let model = parse("quadrantChart\nquadrant-1 Plan\nquadrant-2 \"Do(* +=[?\"\n");
        assert_eq!(quadrants(&model)["quadrant1Text"].as_str().unwrap(), "Plan");
        assert_eq!(
            quadrants(&model)["quadrant2Text"].as_str().unwrap(),
            "Do(* +=[?"
        );

        let model = parse("quadrantChart\ntitle \"this is title (* +=[?\"\n");
        assert_eq!(
            model["title"].as_str().unwrap(),
            "\"this is title (* +=[?\""
        );
    }

    #[test]
    fn parses_points_and_validates_coordinate_range() {
        let model = parse("quadrantChart\npoint1: [0.1, 0.4]\n");
        let pts = points(&model);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0]["text"].as_str().unwrap(), "point1");
        assert_eq!(pts[0]["x"].as_f64().unwrap(), 0.1);
        assert_eq!(pts[0]["y"].as_f64().unwrap(), 0.4);

        let model = parse("quadrantChart\n\"Point1 : (* +=[?\": [1, 0]\n");
        let pts = points(&model);
        assert_eq!(pts[0]["text"].as_str().unwrap(), "Point1 : (* +=[?");
        assert_eq!(pts[0]["x"].as_f64().unwrap(), 1.0);
        assert_eq!(pts[0]["y"].as_f64().unwrap(), 0.0);

        let err = parse_err("quadrantChart\nPoint1 : [1.2, 0.4]\n");
        assert!(err.contains("invalid point coordinate"));

        let err = parse_err("quadrantChart\nPoint1 : [0.2, 0.4, 0.6]\n");
        assert!(err.contains("invalid point coordinate"));
    }

    #[test]
    fn parses_point_styles_and_classes() {
        let model = parse(
            "quadrantChart\nclassDef class1 color: #109060, radius : 10, stroke-color: #310085, stroke-width: 10px\nPoint A:::class1: [0.9, 0.0]\n",
        );
        let classes = model["classes"].as_object().unwrap();
        let class1 = classes.get("class1").unwrap();
        assert_eq!(class1["color"].as_str().unwrap(), "#109060");
        assert_eq!(class1["radius"].as_i64().unwrap(), 10);
        assert_eq!(class1["strokeColor"].as_str().unwrap(), "#310085");
        assert_eq!(class1["strokeWidth"].as_str().unwrap(), "10px");

        let pts = points(&model);
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0]["className"].as_str().unwrap(), "class1");

        let model = parse(
            "quadrantChart\nIncorta: [0.20, 0.30] radius: 10 ,color: #ff0000 ,stroke-color: #ff00ff ,stroke-width: 10px\n",
        );
        let pts = points(&model);
        let styles = &pts[0]["styles"];
        assert_eq!(styles["radius"].as_i64().unwrap(), 10);
        assert_eq!(styles["color"].as_str().unwrap(), "#ff0000");
        assert_eq!(styles["strokeColor"].as_str().unwrap(), "#ff00ff");
        assert_eq!(styles["strokeWidth"].as_str().unwrap(), "10px");
    }

    #[test]
    fn quadrant_class_definitions_and_uses_have_typed_roles() {
        let text = concat!(
            "quadrantChart\n",
            "classDef priority color: #109060\n",
            "Project A:::priority: [0.2, 0.8]\n",
        );
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("quadrantChart", text)
            .unwrap()
            .expect("quadrant editor facts");

        let class_definition = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "priority" && symbol.role == EditorSemanticRole::ClassDefinition
            })
            .expect("quadrant class definition");
        assert_eq!(class_definition.kind, EditorSemanticKind::Class);
        assert!(class_definition.role.contributes_completion());
        assert!(class_definition.role.contributes_outline());
        assert!(!class_definition.role.contributes_references());

        let class_use = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == "priority" && symbol.role == EditorSemanticRole::Payload)
            .expect("quadrant class use");
        assert_eq!(class_use.kind, EditorSemanticKind::Class);
        assert!(!class_use.role.contributes_completion());
        assert!(!class_use.role.contributes_outline());
        assert!(!class_use.role.contributes_references());

        let class_names = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.role == EditorSemanticRole::ClassDefinition)
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(class_names, vec!["priority"]);

        let node_ids = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.role == EditorSemanticRole::Entity)
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            !node_ids.contains(&"priority"),
            "class names must not enter node-id completion"
        );

        let outline_names = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.role.contributes_outline())
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert!(outline_names.contains(&"priority"));
        assert!(outline_names.contains(&"Project A"));
        assert_eq!(
            outline_names
                .iter()
                .filter(|name| **name == "priority")
                .count(),
            1,
            "class use must not create a second outline entry"
        );
    }

    #[test]
    fn parses_whole_chart_example() {
        let model = parse(
            "quadrantChart\n\
title Analytics and Business Intelligence Platforms\n\
x-axis \"Completeness of Vision ?\" --> \"x-axis-2\"\n\
y-axis Ability to Execute --> \"y-axis-2\"\n\
quadrant-1 Leaders\n\
quadrant-2 Challengers\n\
quadrant-3 Niche\n\
quadrant-4 Visionaries\n\
Microsoft: [0.75, 0.75]\n\
Salesforce: [0.55, 0.60]\n\
IBM: [0.51, 0.40]\n\
Incorta: [0.20, 0.30]\n",
        );
        assert_eq!(
            axes(&model)["xAxisLeftText"].as_str().unwrap(),
            "Completeness of Vision ?"
        );
        assert_eq!(axes(&model)["xAxisRightText"].as_str().unwrap(), "x-axis-2");
        assert_eq!(
            axes(&model)["yAxisBottomText"].as_str().unwrap(),
            "Ability to Execute"
        );
        assert_eq!(axes(&model)["yAxisTopText"].as_str().unwrap(), "y-axis-2");
        assert_eq!(
            quadrants(&model)["quadrant1Text"].as_str().unwrap(),
            "Leaders"
        );
        assert_eq!(
            quadrants(&model)["quadrant4Text"].as_str().unwrap(),
            "Visionaries"
        );
        assert_eq!(points(&model).len(), 4);
    }

    #[test]
    fn parse_quadrant_chart_render_model_uses_typed_variant_without_changing_json_parse() {
        let engine = Engine::new();
        let input = r##"
quadrantChart
title Typed Quadrant
accTitle: Quadrant accTitle
accDescr: Quadrant accDescription
x-axis Low --> High
y-axis Bottom --> Top
quadrant-1 Expand
quadrant-2 Maintain
quadrant-3 Evaluate
quadrant-4 Retire
classDef priority color: #109060, radius : 10, stroke-color: #310085, stroke-width: 10px
Project A:::priority : [0.2, 0.8]
"##;

        let parsed = engine
            .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, "quadrantChart");
        match parsed.model() {
            RenderSemanticModel::QuadrantChart(model) => {
                assert_eq!(model.title.as_deref(), Some("Typed Quadrant"));
                assert_eq!(model.acc_title.as_deref(), Some("Quadrant accTitle"));
                assert_eq!(model.acc_descr.as_deref(), Some("Quadrant accDescription"));
                assert_eq!(model.axes.x_axis_left_text, "Low");
                assert_eq!(model.axes.x_axis_right_text, "High");
                assert_eq!(model.quadrants.quadrant1_text, "Expand");
                assert_eq!(model.points.len(), 1);
                assert_eq!(model.points[0].text, "Project A");
                assert_eq!(model.points[0].class_name.as_deref(), Some("priority"));
                assert_eq!(model.classes["priority"].radius, Some(10));
            }
            other => panic!("quadrantChart render parse should return typed model, got {other:?}"),
        }

        let parsed_json = engine
            .parse_diagram_sync(input, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(parsed_json.model["type"], json!("quadrantChart"));
        assert_eq!(parsed_json.model["title"], json!("Typed Quadrant"));
        assert_eq!(parsed_json.model["accTitle"], json!("Quadrant accTitle"));
        assert_eq!(parsed_json.model["axes"]["xAxisLeftText"], json!("Low"));
        assert_eq!(
            parsed_json.model["quadrants"]["quadrant1Text"],
            json!("Expand")
        );
        assert_eq!(parsed_json.model["points"][0]["text"], json!("Project A"));
        assert_eq!(
            parsed_json.model["classes"]["priority"]["radius"],
            json!(10)
        );
        assert!(parsed_json.model.get("config").is_some());
    }

    #[test]
    fn parse_styles_matches_quadrantdb_spec() {
        let styles = vec![
            "radius: 10".to_string(),
            "color: #ff0000".to_string(),
            "stroke-color: #ff00ff".to_string(),
            "stroke-width: 10px".to_string(),
        ];
        let obj = parse_styles(&styles).unwrap();
        assert_eq!(obj.radius, Some(10));
        assert_eq!(obj.color.as_deref(), Some("#ff0000"));
        assert_eq!(obj.stroke_color.as_deref(), Some("#ff00ff"));
        assert_eq!(obj.stroke_width.as_deref(), Some("10px"));

        let err = parse_styles(&["test_name: value".to_string()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("style named test_name is not supported."));

        let obj = parse_styles(&[]).unwrap();
        assert_eq!(obj.radius, None);
        assert!(obj.color.is_none());

        let err = parse_styles(&["radius: f".to_string()])
            .unwrap_err()
            .to_string();
        assert!(err.contains("value for radius f is invalid, please use a valid number"));
    }

    #[test]
    fn entrypoints_and_combined_projection_construct_once() {
        let engine = Engine::new();
        let text = concat!(
            "quadrantChart\n",
            "title Delivery portfolio\n",
            "accTitle: Portfolio\n",
            "x-axis Low --> High\n",
            "y-axis Bottom --> Top\n",
            "quadrant-1 Invest\n",
            "classDef priority color: #109060, radius: 10\n",
            "Project A:::priority: [0.2, 0.8]\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("standalone Quadrant JSON parse succeeds")
            .expect("standalone Quadrant JSON parse returns a diagram");
        reset_quadrant_syntax_construction_count();
        parse_quadrant_chart(text, &parsed.meta).expect("Quadrant JSON projection succeeds");
        assert_eq!(quadrant_syntax_construction_count(), 1);

        reset_quadrant_syntax_construction_count();
        let typed = parse_quadrant_chart_model_for_render(text, &parsed.meta)
            .expect("Quadrant typed projection succeeds");
        assert_eq!(quadrant_syntax_construction_count(), 1);

        reset_quadrant_syntax_construction_count();
        let (combined_json, combined_editor) =
            crate::family::test_support::into_result(parse_quadrant_chart_json_and_editor_facts(
                text,
                &parsed.meta,
                &OperationControl::new(),
            ))
            .expect("Quadrant combined projection succeeds");
        assert_eq!(quadrant_syntax_construction_count(), 1);
        assert_eq!(combined_json, parsed.model);
        assert!(!combined_editor.symbols.is_empty());
        assert_eq!(
            render_model_to_compat_json(&typed, &parsed.meta).unwrap(),
            combined_json
        );
        assert_eq!(combined_json["type"], json!("quadrantChart"));
        assert!(combined_json["config"].is_object());
        assert_eq!(combined_json["accDescr"], Value::Null);

        let typed = serde_json::to_value(typed).expect("Quadrant typed model serializes");
        for field in [
            "title",
            "accTitle",
            "accDescr",
            "quadrants",
            "axes",
            "points",
            "classes",
        ] {
            assert_eq!(typed[field], combined_json[field], "Quadrant {field} drift");
        }
    }

    #[test]
    fn quoted_semicolons_and_inline_acc_descr_block_share_statement_scanner() {
        let engine = Engine::new();
        let text = concat!(
            "quadrantChart\n",
            "title \"Plan; Execute\"; accDescr {first %% literal; second}; quadrant-1 \"Build; Learn\"\n",
            "\"Point; A\": [0.2, 0.8]\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("quoted semicolons parse")
            .expect("quadrant model");
        let facts = crate::family::test_support::editor_facts(
            parse_quadrant_chart_json_and_editor_facts,
            text,
            &parsed.meta,
        );

        assert_eq!(parsed.model["title"], json!("\"Plan; Execute\""));
        assert_eq!(parsed.model["accDescr"], json!("first %% literal; second"));
        assert_eq!(
            parsed.model["quadrants"]["quadrant1Text"],
            json!("Build; Learn")
        );
        assert_eq!(parsed.model["points"][0]["text"], json!("Point; A"));
        for expected in [
            "Plan; Execute",
            "first %% literal; second",
            "Build; Learn",
            "Point; A",
        ] {
            assert!(
                facts.symbols.iter().any(|symbol| symbol.name == expected),
                "missing source-backed fact for {expected}"
            );
        }
    }

    #[test]
    fn multiline_acc_descr_preserves_original_source_span() {
        let engine = Engine::new();
        let text = concat!(
            "quadrantChart\n",
            "accDescr {\n",
            "  First line\n",
            "  Second line\n",
            "}\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("multiline accDescr parses")
            .expect("quadrant model");
        let facts = crate::family::test_support::editor_facts(
            parse_quadrant_chart_json_and_editor_facts,
            text,
            &parsed.meta,
        );
        let description = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.detail.as_deref() == Some("quadrant chart accessibility description")
            })
            .expect("multiline accessibility description fact");
        let content_start = text.find('{').expect("opening brace") + 1;
        let content_end = text.rfind('}').expect("closing brace");

        assert_eq!(parsed.model["accDescr"], json!("First line\nSecond line"));
        assert_eq!(
            description.selection,
            SourceSpan::new(content_start, content_end)
        );
    }

    #[test]
    fn malformed_point_recovers_prior_parser_facts() {
        let engine = Engine::new();
        let text = "quadrantChart\nx-axis Low --> High\nBroken: [1.2, 0.4]\n";
        let statement_start = text.find("Broken").expect("malformed statement");
        let statement_end = statement_start + "Broken: [1.2, 0.4]".len();
        reset_quadrant_syntax_construction_count();
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("quadrantChart", text)
            .expect("quadrant editor recovery succeeds")
            .expect("quadrant editor facts are available");

        assert_eq!(quadrant_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(
            facts.symbols.iter().any(|symbol| {
                symbol.name == "Low" && symbol.role == EditorSemanticRole::Outline
            })
        );
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(statement_start, statement_end))
        }));
    }

    #[test]
    fn eof_rejects_unterminated_multiline_acc_descr_like_pinned_jison() {
        let text = "quadrantChart\naccDescr {\npartial description\n";

        reset_quadrant_syntax_construction_count();
        let snapshot = Engine::new()
            .parse_diagram_snapshot_with_type_sync("quadrantChart", text)
            .expect("Quadrant snapshot operation")
            .expect("Quadrant snapshot");
        assert_eq!(quadrant_syntax_construction_count(), 1);
        assert!(matches!(
            snapshot.outcome(),
            crate::DiagramParseOutcome::Failed(_)
        ));
        let crate::ParsedEditorFacts::Available(facts) = snapshot.editor_facts() else {
            panic!("Quadrant recovery facts");
        };
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(!facts.diagnostics.is_empty());
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "partial description" && symbol.role == EditorSemanticRole::Payload
        }));
    }
}

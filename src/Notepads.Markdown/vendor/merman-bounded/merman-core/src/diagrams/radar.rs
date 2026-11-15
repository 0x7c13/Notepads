use crate::common_db::LangiumCommonDbFields;
use crate::diagrams::langium_common::{
    LangiumCommonFacts, parse_langium_common, parse_langium_string,
    push_langium_common_editor_fact, push_langium_common_recovery, strip_langium_inline_comment,
};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
    editor::{editor_recovery_fallback_span, ensure_editor_recovery_from_error},
    family,
};
use serde_json::{Map, Value, json};

const MAX_TICKS: i64 = 32;

#[derive(Debug, Clone)]
struct AxisAst {
    name: SpannedText,
    label: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct EntryAst {
    axis: Option<SpannedText>,
    value: SpannedRadarValue,
}

#[derive(Debug, Clone)]
struct CurveAst {
    name: SpannedText,
    label: Option<SpannedText>,
    entries: Vec<EntryAst>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
enum OptionValueAst {
    Bool(bool),
    Number(Value),
    Graticule(String),
}

#[derive(Debug, Clone)]
struct OptionAst {
    name: String,
    value: OptionValueAst,
    value_span: SourceSpan,
}

#[derive(Debug, Clone)]
struct SpannedRadarValue {
    value: Value,
    span: SourceSpan,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RadarRenderAxis {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RadarRenderCurve {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub entries: Vec<Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RadarRenderOptions {
    #[serde(rename = "showLegend")]
    pub show_legend: bool,
    pub ticks: Value,
    pub max: Option<Value>,
    pub min: Value,
    pub graticule: String,
}

impl Default for RadarRenderOptions {
    fn default() -> Self {
        Self {
            show_legend: true,
            ticks: json!(5),
            max: None,
            min: json!(0),
            graticule: "circle".to_string(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RadarDiagramRenderModel {
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub axes: Vec<RadarRenderAxis>,
    #[serde(default)]
    pub curves: Vec<RadarRenderCurve>,
    #[serde(default)]
    pub options: RadarRenderOptions,
    #[serde(skip)]
    compatibility: RadarCompatibilityOutputState,
}

#[derive(Debug, Clone, Copy, Default)]
enum RadarCompatibilityOutputState {
    Empty,
    #[default]
    Model,
}

impl RadarDiagramRenderModel {
    fn empty_compatibility_output() -> Self {
        Self {
            compatibility: RadarCompatibilityOutputState::Empty,
            ..Self::default()
        }
    }

    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct RadarDb {
    title: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    axes: Vec<RadarRenderAxis>,
    curves: Vec<RadarRenderCurve>,
    options: RadarRenderOptions,
}

struct RadarSemanticSource {
    db: Option<RadarDb>,
    editor_facts: EditorSemanticFacts,
}

impl RadarDb {
    fn new() -> Self {
        Self {
            title: None,
            acc_title: None,
            acc_descr: None,
            axes: Vec::new(),
            curves: Vec::new(),
            options: RadarRenderOptions::default(),
        }
    }

    fn set_axes_controlled(
        &mut self,
        axes: Vec<AxisAst>,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        let mut rendered = Vec::with_capacity(axes.len());
        for axis in axes {
            control.checkpoint()?;
            rendered.push(RadarRenderAxis {
                label: axis
                    .label
                    .map(|label| label.text)
                    .unwrap_or_else(|| axis.name.text.clone()),
                name: axis.name.text,
            });
        }
        self.axes = rendered;
        Ok(())
    }

    fn set_curves_controlled(
        &mut self,
        curves: Vec<CurveAst>,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<()>> {
        let mut rendered = Vec::with_capacity(curves.len());
        for curve in curves {
            control.checkpoint()?;
            let curve_span = curve.span;
            let label = curve
                .label
                .as_ref()
                .map(|label| label.text.clone())
                .unwrap_or_else(|| curve.name.text.clone());
            let entries =
                match compute_curve_entries_controlled(&self.axes, &curve.entries, control)? {
                    Ok(entries) => entries,
                    Err(error) => {
                        return Ok(Err(error.with_exact_span_if_missing(curve_span)));
                    }
                };
            rendered.push(RadarRenderCurve {
                name: curve.name.text,
                label,
                entries,
            });
        }
        self.curves = rendered;
        Ok(Ok(()))
    }

    fn set_options_controlled(
        &mut self,
        options: Vec<OptionAst>,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        let mut last: std::collections::HashMap<String, OptionValueAst> =
            std::collections::HashMap::new();
        for opt in options {
            control.checkpoint()?;
            last.insert(opt.name, opt.value);
        }

        if let Some(OptionValueAst::Bool(v)) = last.get("showLegend") {
            self.options.show_legend = *v;
        }
        if let Some(OptionValueAst::Number(v)) = last.get("ticks") {
            self.options.ticks = if v.as_f64().is_some_and(|ticks| ticks > MAX_TICKS as f64) {
                json!(MAX_TICKS)
            } else {
                v.clone()
            };
        }
        if let Some(OptionValueAst::Number(v)) = last.get("max") {
            self.options.max = Some(v.clone());
        }
        if let Some(OptionValueAst::Number(v)) = last.get("min") {
            self.options.min = v.clone();
        }
        if let Some(OptionValueAst::Graticule(v)) = last.get("graticule") {
            self.options.graticule = v.clone();
        }
        Ok(())
    }

    #[inline]
    fn into_render_model(self) -> RadarDiagramRenderModel {
        RadarDiagramRenderModel {
            title: self.title,
            acc_title: self.acc_title,
            acc_descr: self.acc_descr,
            axes: self.axes,
            curves: self.curves,
            options: self.options,
            compatibility: RadarCompatibilityOutputState::Model,
        }
    }
}

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RadarEntityOccurrence {
    Definition,
    Reference,
}

fn push_radar_entity(
    facts: &mut EditorSemanticFacts,
    text: SpannedText,
    detail: &str,
    kind: EditorSemanticKind,
    occurrence: RadarEntityOccurrence,
) {
    if text.text.is_empty() {
        return;
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        text.span,
    ));
    let detail = Some(detail.to_string());
    let symbol = match occurrence {
        RadarEntityOccurrence::Definition => {
            EditorSemanticSymbol::new(text.text, detail, kind, text.span, text.span)
        }
        RadarEntityOccurrence::Reference => {
            EditorSemanticSymbol::reference(text.text, detail, kind, text.span, text.span)
        }
    };
    facts.push_symbol(symbol);
}

fn push_radar_payload(
    facts: &mut EditorSemanticFacts,
    text: SpannedText,
    detail: &str,
    kind: EditorSemanticKind,
) {
    if text.text.is_empty() {
        return;
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        text.span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        text.text,
        Some(detail.to_string()),
        kind,
        text.span,
        text.span,
    ));
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

enum RadarStatementEvent {
    Axes(Vec<AxisAst>),
    Curves(Vec<CurveAst>),
    Options(Vec<OptionAst>),
}

struct RadarParsedStatement {
    event: RadarStatementEvent,
}

fn parse_radar_statement(
    stmt: &str,
    stmt_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<RadarParsedStatement, String>> {
    control.checkpoint()?;
    let (trimmed, trimmed_start) = trim_start_with_source_offset(stmt, stmt_start);
    if let Some(rest) = trimmed.strip_prefix("axis") {
        let (rest, rest_start) = trim_start_with_source_offset(rest, trimmed_start + "axis".len());
        if rest.is_empty() {
            return Ok(Err(
                "axis statement must include at least one axis".to_string()
            ));
        }
        let axes = match parse_axes_list(rest, rest_start, control)? {
            Ok(axes) => axes,
            Err(error) => return Ok(Err(error)),
        };
        return Ok(Ok(RadarParsedStatement {
            event: RadarStatementEvent::Axes(axes),
        }));
    }

    if trimmed.starts_with("curve") {
        let curves = match parse_curves_stmt(trimmed, trimmed_start, control)? {
            Ok(curves) => curves,
            Err(error) => return Ok(Err(error)),
        };
        return Ok(Ok(RadarParsedStatement {
            event: RadarStatementEvent::Curves(curves),
        }));
    }

    let options = match parse_option_list_stmt(trimmed, trimmed_start, control)? {
        Ok(options) => options,
        Err(error) => return Ok(Err(error)),
    };
    if let Some(options) = options {
        return Ok(Ok(RadarParsedStatement {
            event: RadarStatementEvent::Options(options),
        }));
    }

    let option = match parse_option_stmt(trimmed, trimmed_start) {
        Ok(option) => option,
        Err(error) => return Ok(Err(error)),
    };
    if let Some(option) = option {
        control.checkpoint()?;
        return Ok(Ok(RadarParsedStatement {
            event: RadarStatementEvent::Options(vec![option]),
        }));
    }

    Ok(Err(format!("unexpected radar statement: {}", stmt.trim())))
}

fn push_radar_statement_facts(
    facts: &mut EditorSemanticFacts,
    event: &RadarStatementEvent,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<()> {
    match event {
        RadarStatementEvent::Axes(axes) => {
            for axis in axes {
                control.checkpoint()?;
                push_radar_entity(
                    facts,
                    axis.name.clone(),
                    "radar axis",
                    EditorSemanticKind::Variable,
                    RadarEntityOccurrence::Definition,
                );
                if let Some(label) = &axis.label {
                    push_radar_payload(
                        facts,
                        label.clone(),
                        "radar axis label",
                        EditorSemanticKind::String,
                    );
                }
            }
        }
        RadarStatementEvent::Curves(curves) => {
            for curve in curves {
                control.checkpoint()?;
                push_radar_entity(
                    facts,
                    curve.name.clone(),
                    "radar curve",
                    EditorSemanticKind::Variable,
                    RadarEntityOccurrence::Definition,
                );
                if let Some(label) = &curve.label {
                    push_radar_payload(
                        facts,
                        label.clone(),
                        "radar curve label",
                        EditorSemanticKind::String,
                    );
                }

                for entry in &curve.entries {
                    control.checkpoint()?;
                    if let Some(axis) = &entry.axis {
                        push_radar_entity(
                            facts,
                            axis.clone(),
                            "radar curve axis reference",
                            EditorSemanticKind::Variable,
                            RadarEntityOccurrence::Reference,
                        );
                    }
                    push_radar_payload(
                        facts,
                        SpannedText {
                            text: value_text(&entry.value.value),
                            span: entry.value.span,
                        },
                        "radar curve entry",
                        EditorSemanticKind::String,
                    );
                }
            }
        }
        RadarStatementEvent::Options(options) => {
            for option in options {
                control.checkpoint()?;
                let token = match &option.value {
                    OptionValueAst::Bool(value) => value.to_string(),
                    OptionValueAst::Number(value) => value_text(value),
                    OptionValueAst::Graticule(value) => value.clone(),
                };
                push_radar_payload(
                    facts,
                    SpannedText {
                        text: token,
                        span: option.value_span,
                    },
                    "radar option",
                    EditorSemanticKind::String,
                );
            }
        }
    }
    Ok(())
}

fn apply_radar_statement_controlled(
    event: RadarStatementEvent,
    axes: &mut Vec<AxisAst>,
    curves: &mut Vec<CurveAst>,
    options: &mut Vec<OptionAst>,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<()> {
    match event {
        RadarStatementEvent::Axes(parsed) => {
            for axis in parsed {
                control.checkpoint()?;
                axes.push(axis);
            }
        }
        RadarStatementEvent::Curves(parsed) => {
            for curve in parsed {
                control.checkpoint()?;
                curves.push(curve);
            }
        }
        RadarStatementEvent::Options(parsed) => {
            for option in parsed {
                control.checkpoint()?;
                options.push(option);
            }
        }
    }
    Ok(())
}

fn compute_curve_entries_controlled(
    axes: &[RadarRenderAxis],
    entries: &[EntryAst],
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<Vec<Value>>> {
    control.checkpoint()?;
    if entries.is_empty() {
        return Ok(Ok(Vec::new()));
    }

    if entries[0].axis.is_none() {
        let mut values = Vec::with_capacity(entries.len());
        for entry in entries {
            control.checkpoint()?;
            values.push(entry.value.value.clone());
        }
        return Ok(Ok(values));
    }

    if axes.is_empty() {
        return Ok(Err(Error::diagram_parse_fallback(
            "radar".to_string(),
            "Axes must be populated before curves for reference entries".to_string(),
        )));
    }

    let mut entries_by_axis = std::collections::HashMap::with_capacity(entries.len());
    for entry in entries {
        control.checkpoint()?;
        if let Some(axis) = &entry.axis {
            entries_by_axis.entry(axis.text.as_str()).or_insert(entry);
        }
    }

    let mut values = Vec::with_capacity(axes.len());
    for axis in axes {
        control.checkpoint()?;
        let Some(found) = entries_by_axis.get(axis.name.as_str()) else {
            return Ok(Err(Error::diagram_parse_fallback(
                "radar".to_string(),
                format!("Missing entry for axis {}", axis.label),
            )));
        };
        values.push(found.value.value.clone());
    }
    Ok(Ok(values))
}

pub(crate) fn parse_radar(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let model = parse_radar_semantic_source(code, meta)?
        .db
        .map(RadarDb::into_render_model)
        .unwrap_or_else(RadarDiagramRenderModel::empty_compatibility_output);
    render_model_to_compat_json(&model, meta)
}

pub(crate) fn parse_radar_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<family::CombinedSemanticParse> {
    control.checkpoint()?;
    let parsed = family::CombinedSemanticParse::from_construction(
        construct_radar_semantic_source_controlled(code, meta, control)?,
        |source| {
            let model = source
                .db
                .map(RadarDb::into_render_model)
                .unwrap_or_else(RadarDiagramRenderModel::empty_compatibility_output);
            (
                render_model_to_compat_json(&model, meta),
                source.editor_facts,
            )
        },
        family::CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_radar_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<RadarDiagramRenderModel> {
    parse_radar_semantic_source(code, meta).map(|source| {
        source
            .db
            .map(RadarDb::into_render_model)
            .unwrap_or_else(RadarDiagramRenderModel::empty_compatibility_output)
    })
}

pub(crate) fn parse_radar_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<RadarDiagramRenderModel>> {
    let construction = construct_radar_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(failure.into_error())),
    };
    control.checkpoint()?;
    Ok(Ok(source
        .db
        .map(RadarDb::into_render_model)
        .unwrap_or_else(
            RadarDiagramRenderModel::empty_compatibility_output,
        )))
}

pub(crate) fn render_model_to_compat_json(
    model: &RadarDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    if matches!(model.compatibility, RadarCompatibilityOutputState::Empty) {
        return Ok(json!({}));
    }

    let mut out = Map::with_capacity(8);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("axes".to_string(), json!(&model.axes));
    out.insert("curves".to_string(), json!(&model.curves));
    out.insert("options".to_string(), json!(&model.options));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

#[inline]
fn parse_radar_semantic_source(code: &str, meta: &ParseMetadata) -> Result<RadarSemanticSource> {
    construct_radar_semantic_source(code, meta).map_err(family::CombinedSemanticFailure::into_error)
}

#[inline]
fn construct_radar_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<RadarSemanticSource, family::CombinedSemanticFailure> {
    construct_radar_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_radar_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<
    std::result::Result<RadarSemanticSource, family::CombinedSemanticFailure>,
> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("radar");

    let body = match radar_body_start(code, control)? {
        Ok(Some(body)) => body,
        Ok(None) => {
            return Ok(Ok(RadarSemanticSource {
                db: None,
                editor_facts: EditorSemanticFacts::new(),
            }));
        }
        Err(error) => {
            return Ok(Err(radar_parse_failure(
                error,
                EditorSemanticFacts::new(),
                code,
            )));
        }
    };
    let mut offset = body.offset;

    let mut common = LangiumCommonFacts::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut axes: Vec<AxisAst> = Vec::new();
    let mut curves: Vec<CurveAst> = Vec::new();
    let mut options: Vec<OptionAst> = Vec::new();
    let mut first_error = None;

    while offset < code.len() {
        control.checkpoint()?;
        if let Some(parsed) = parse_langium_common(code, offset) {
            if let Some(diagnostic) = &parsed.diagnostic {
                push_langium_common_recovery(&mut editor_facts, diagnostic);
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        diagnostic.message.clone(),
                        diagnostic.span.start,
                    )
                });
            }
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "radar");
            common.push(parsed.fact);
            offset += parsed.consumed;
            continue;
        }

        let (statement, statement_start, next_offset) = radar_statement_at(code, offset, control)?;
        offset = next_offset;
        if statement.is_empty() {
            continue;
        }

        match parse_radar_statement(&statement, statement_start, control)? {
            Ok(parsed) => {
                push_radar_statement_facts(&mut editor_facts, &parsed.event, control)?;
                apply_radar_statement_controlled(
                    parsed.event,
                    &mut axes,
                    &mut curves,
                    &mut options,
                    control,
                )?;
            }
            Err(message) => {
                let invalid = statement.trim_end();
                editor_facts.mark_recovered_from_parse_error(
                    message.clone(),
                    Some(SourceSpan::new(
                        statement_start,
                        statement_start + invalid.len(),
                    )),
                );
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_fallback("radar".to_string(), message)
                });
            }
        }
    }

    let common = LangiumCommonDbFields::from_facts(&common);
    let mut db = RadarDb::new();
    db.title = common.title;
    db.acc_title = common.acc_title;
    db.acc_descr = common.acc_descr;
    db.set_axes_controlled(axes, control)?;
    if let Err(error) = db.set_curves_controlled(curves, control)? {
        editor_facts = ensure_editor_recovery_from_error(
            editor_facts,
            &error,
            editor_recovery_fallback_span(code),
        );
        if first_error.is_none() {
            first_error = Some(error);
        }
    }
    db.set_options_controlled(options, control)?;
    control.checkpoint()?;

    if let Some(error) = first_error {
        return Ok(Err(family::CombinedSemanticFailure::new(
            error,
            editor_facts,
        )));
    }

    Ok(Ok(RadarSemanticSource {
        db: Some(db),
        editor_facts,
    }))
}

fn radar_parse_failure(
    error: Error,
    editor_facts: EditorSemanticFacts,
    code: &str,
) -> family::CombinedSemanticFailure {
    let editor_facts = ensure_editor_recovery_from_error(
        editor_facts,
        &error,
        editor_recovery_fallback_span(code),
    );
    family::CombinedSemanticFailure::new(error, editor_facts)
}

fn strip_inline_comment(line: &str) -> &str {
    strip_langium_inline_comment(line)
}

#[derive(Debug, Clone, Copy)]
struct RadarBodyStart {
    offset: usize,
}

fn radar_body_start(
    code: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<Option<RadarBodyStart>>> {
    let mut offset = 0usize;
    while offset < code.len() {
        control.checkpoint()?;
        let (line, next_offset) = physical_line(code, offset);
        let visible = strip_inline_comment(line);
        let trimmed = visible.trim_start();
        if trimmed.trim().is_empty() {
            offset = next_offset;
            continue;
        }
        let Some(after_keyword) = trimmed.strip_prefix("radar-beta") else {
            return Ok(Err(Error::diagram_parse_fallback(
                "radar".to_string(),
                "expected radar-beta".to_string(),
            )));
        };
        if after_keyword
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace() && ch != ':' && !after_keyword.starts_with("%%"))
        {
            return Ok(Err(Error::diagram_parse_fallback(
                "radar".to_string(),
                "expected radar-beta".to_string(),
            )));
        }

        let leading = visible.len() - trimmed.len();
        let header_start = offset + leading;
        let mut body_start = header_start + "radar-beta".len();
        let whitespace = code[body_start..]
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
        let colon_start = body_start + whitespace;
        if code.as_bytes().get(colon_start) == Some(&b':') {
            body_start = colon_start + 1;
        }
        return Ok(Ok(Some(RadarBodyStart { offset: body_start })));
    }
    Ok(Ok(None))
}

fn radar_statement_at(
    code: &str,
    offset: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<(String, usize, usize)> {
    control.checkpoint()?;
    let (line, mut next_offset) = physical_line(code, offset);
    let visible = strip_inline_comment(line);
    let trimmed = visible.trim();
    let stmt_start = offset + visible.find(trimmed).unwrap_or(0);
    if trimmed.is_empty() {
        return Ok((String::new(), stmt_start, next_offset));
    }
    let mut stmt = mask_radar_inline_comments(&code[stmt_start..next_offset], control)?;
    let mut brace_balance = RadarBraceBalance::default();
    brace_balance.scan(&stmt, control)?;

    if stmt.trim_start().starts_with("curve")
        && brace_balance.saw_opening_brace
        && !brace_balance.is_balanced()
    {
        while next_offset < code.len() {
            control.checkpoint()?;
            let segment_start = next_offset;
            let (_, after_next) = physical_line(code, next_offset);
            next_offset = after_next;
            let segment = mask_radar_inline_comments(&code[segment_start..next_offset], control)?;
            brace_balance.scan(&segment, control)?;
            stmt.push_str(&segment);
            if brace_balance.is_balanced() {
                break;
            }
        }
    }

    Ok((stmt, stmt_start, next_offset))
}

fn mask_radar_inline_comments(
    source: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<String> {
    let mut masked = source.as_bytes().to_vec();
    let mut line_start = 0usize;
    while line_start < source.len() {
        control.checkpoint()?;
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |newline| line_start + newline);
        let content_end = if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
            line_end - 1
        } else {
            line_end
        };
        let line = &source[line_start..content_end];
        let visible = strip_langium_inline_comment(line);
        if visible.len() < line.len() {
            masked[line_start + visible.len()..content_end].fill(b' ');
        }
        line_start = (line_end + usize::from(line_end < source.len())).min(source.len());
    }
    Ok(String::from_utf8(masked)
        .expect("comment masking preserves UTF-8 before the comment marker"))
}

fn trim_start_with_source_offset(input: &str, input_start: usize) -> (&str, usize) {
    let trimmed = input.trim_start();
    (trimmed, input_start + input.len() - trimmed.len())
}

fn physical_line(source: &str, offset: usize) -> (&str, usize) {
    let rest = &source[offset..];
    if let Some(newline) = rest.find('\n') {
        let line = rest[..newline]
            .strip_suffix('\r')
            .unwrap_or(&rest[..newline]);
        (line, offset + newline + 1)
    } else {
        (rest, source.len())
    }
}

#[derive(Default)]
struct RadarBraceBalance {
    in_quote: Option<char>,
    escaped: bool,
    depth: i64,
    saw_opening_brace: bool,
}

impl RadarBraceBalance {
    fn scan(
        &mut self,
        source: &str,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        for (offset, ch) in source.char_indices() {
            if offset % 4096 < ch.len_utf8() {
                control.checkpoint()?;
            }
            if self.escaped {
                self.escaped = false;
                continue;
            }
            if ch == '\\' && self.in_quote.is_some() {
                self.escaped = true;
                continue;
            }
            if let Some(quote) = self.in_quote {
                if ch == quote {
                    self.in_quote = None;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                self.in_quote = Some(ch);
                continue;
            }
            if ch == '{' {
                self.saw_opening_brace = true;
                self.depth += 1;
            } else if ch == '}' {
                self.depth -= 1;
            }
        }
        Ok(())
    }

    fn is_balanced(&self) -> bool {
        self.depth == 0
    }
}

fn parse_axes_list(
    input: &str,
    input_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<Vec<AxisAst>, String>> {
    let mut p = TokenParser::new(input, input_start);
    let mut out = Vec::new();
    loop {
        control.checkpoint()?;
        p.skip_ws();
        if p.eof() {
            break;
        }
        let Some(name) = p.parse_id() else {
            return Ok(Err("expected axis id".to_string()));
        };
        p.skip_ws();
        let label = if p.try_consume('[') {
            p.skip_ws();
            let Some(s) = p.parse_quoted_string() else {
                return Ok(Err("expected quoted axis label".to_string()));
            };
            p.skip_ws();
            if !p.try_consume(']') {
                return Ok(Err("expected ']'".to_string()));
            }
            Some(s)
        } else {
            None
        };
        out.push(AxisAst { name, label });
        p.skip_ws();
        if p.try_consume(',') {
            // Upstream Mermaid rejects a trailing comma at the end of an `axis` list.
            p.skip_ws();
            if p.eof() {
                return Ok(Err("unexpected trailing ',' in axis list".to_string()));
            }
            continue;
        }
        if p.eof() {
            break;
        }
        return Ok(Err("expected ',' or end of axis list".to_string()));
    }
    Ok(Ok(out))
}

fn parse_curves_stmt(
    input: &str,
    input_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<Vec<CurveAst>, String>> {
    control.checkpoint()?;
    let (input, input_start) = trim_start_with_source_offset(input, input_start);
    let Some(rest) = input.strip_prefix("curve") else {
        return Ok(Err("expected curve".to_string()));
    };
    let (rest, rest_start) = trim_start_with_source_offset(rest, input_start + "curve".len());
    if rest.trim().is_empty() {
        return Ok(Err("expected curve id".to_string()));
    }

    let chunks = split_top_level(rest, ',', control)?;
    let mut curves = Vec::new();
    for (chunk_offset, chunk) in chunks {
        control.checkpoint()?;
        let (chunk, chunk_start) = trim_start_with_source_offset(chunk, rest_start + chunk_offset);
        let chunk = chunk.trim_end();
        if chunk.is_empty() {
            return Ok(Err("expected curve after ','".to_string()));
        }
        match parse_curve(chunk, chunk_start, control)? {
            Ok(curve) => curves.push(curve),
            Err(error) => return Ok(Err(error)),
        }
    }
    Ok(Ok(curves))
}

fn parse_curve(
    input: &str,
    input_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<CurveAst, String>> {
    control.checkpoint()?;
    let (name, label, entries_str, entries_start) = {
        let mut p = TokenParser::new(input, input_start);
        p.skip_ws();
        let Some(name) = p.parse_id() else {
            return Ok(Err("expected curve id".to_string()));
        };
        p.skip_ws();
        let label = if p.try_consume('[') {
            p.skip_ws();
            let Some(s) = p.parse_quoted_string() else {
                return Ok(Err("expected quoted curve label".to_string()));
            };
            p.skip_ws();
            if !p.try_consume(']') {
                return Ok(Err("expected ']'".to_string()));
            }
            p.skip_ws();
            Some(s)
        } else {
            None
        };

        if !p.try_consume('{') {
            return Ok(Err("expected '{'".to_string()));
        }

        let (entries_str, entries_start) = match p.take_until_matching_brace(control)? {
            Ok(entries) => entries,
            Err(error) => return Ok(Err(error)),
        };

        p.skip_ws();
        if !p.eof() {
            return Ok(Err("unexpected trailing tokens after curve".to_string()));
        }
        (name, label, entries_str, entries_start)
    };
    let entries = match parse_entries(entries_str, entries_start, control)? {
        Ok(entries) => entries,
        Err(error) => return Ok(Err(error)),
    };

    let has_detailed = entries.iter().any(|e| e.axis.is_some());
    let has_numeric = entries.iter().any(|e| e.axis.is_none());
    if has_detailed && has_numeric {
        return Ok(Err(
            "mixed detailed and numeric entries are not supported".to_string()
        ));
    }

    Ok(Ok(CurveAst {
        name,
        label,
        entries,
        span: SourceSpan::new(input_start, input_start + input.len()),
    }))
}

fn parse_entries(
    input: &str,
    input_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<Vec<EntryAst>, String>> {
    let items = split_top_level(input, ',', control)?;
    let mut out = Vec::new();
    for (item_offset, item) in items {
        control.checkpoint()?;
        let (item, item_start) = trim_start_with_source_offset(item, input_start + item_offset);
        let item = item.trim_end();
        if item.is_empty() {
            return Ok(Err("expected curve entry".to_string()));
        }

        // Try detailed first: <ID> ':'? <NUMBER>
        let mut p = TokenParser::new(item, item_start);
        p.skip_ws();
        let checkpoint = p.checkpoint();
        if let Some(axis) = p.parse_id() {
            p.skip_ws();
            p.try_consume(':');
            p.skip_ws();
            if let Some(num) = p.parse_number_value() {
                p.skip_ws();
                if p.eof() {
                    out.push(EntryAst {
                        axis: Some(axis),
                        value: num,
                    });
                    continue;
                }
            }
        }
        p.rollback(checkpoint);

        // Otherwise numeric: <NUMBER>
        p.skip_ws();
        let Some(num) = p.parse_number_value() else {
            return Ok(Err("expected entry number".to_string()));
        };
        p.skip_ws();
        if !p.eof() {
            return Ok(Err("unexpected trailing tokens in entry".to_string()));
        }
        out.push(EntryAst {
            axis: None,
            value: num,
        });
    }
    Ok(Ok(out))
}

fn parse_option_stmt(
    input: &str,
    input_start: usize,
) -> std::result::Result<Option<OptionAst>, String> {
    let mut p = TokenParser::new(input, input_start);
    p.skip_ws();
    let parsed_name = match p.parse_id() {
        Some(name) => name,
        None => return Ok(None),
    };
    let name = match parsed_name.text.as_str() {
        "showLegend" => "showLegend",
        "ticks" => "ticks",
        "max" => "max",
        "min" => "min",
        "graticule" => "graticule",
        _ => return Ok(None),
    }
    .to_string();
    p.skip_ws();

    if name == "showLegend" {
        let (value, value_span) = p
            .parse_bool()
            .ok_or_else(|| "expected boolean".to_string())?;
        p.skip_ws();
        if !p.eof() {
            return Err("unexpected trailing tokens after option".to_string());
        }
        return Ok(Some(OptionAst {
            name,
            value: OptionValueAst::Bool(value),
            value_span,
        }));
    }

    if name == "graticule" {
        let value = p
            .parse_id()
            .ok_or_else(|| "expected graticule".to_string())?;
        if value.text != "circle" && value.text != "polygon" {
            return Err("expected graticule".to_string());
        }
        p.skip_ws();
        if !p.eof() {
            return Err("unexpected trailing tokens after option".to_string());
        }
        return Ok(Some(OptionAst {
            name,
            value: OptionValueAst::Graticule(value.text),
            value_span: value.span,
        }));
    }

    let value = p
        .parse_number_value()
        .ok_or_else(|| "expected number".to_string())?;
    p.skip_ws();
    if !p.eof() {
        return Err("unexpected trailing tokens after option".to_string());
    }
    Ok(Some(OptionAst {
        name,
        value: OptionValueAst::Number(value.value),
        value_span: value.span,
    }))
}

fn parse_option_list_stmt(
    input: &str,
    input_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<Option<Vec<OptionAst>>, String>> {
    let chunks = split_top_level(input, ',', control)?;
    if chunks.len() == 1 {
        return Ok(Ok(None));
    }
    let mut out = Vec::new();
    for (chunk_offset, chunk) in chunks {
        control.checkpoint()?;
        let (chunk, chunk_start) = trim_start_with_source_offset(chunk, input_start + chunk_offset);
        let chunk = chunk.trim_end();
        if chunk.is_empty() {
            return Ok(Err("expected option after ','".to_string()));
        }
        let opt = match parse_option_stmt(chunk, chunk_start) {
            Ok(opt) => opt,
            Err(error) => return Ok(Err(error)),
        };
        let Some(opt) = opt else {
            return Ok(Ok(None));
        };
        out.push(opt);
    }
    if out.is_empty() {
        return Ok(Ok(None));
    }
    Ok(Ok(Some(out)))
}

fn split_top_level<'a>(
    input: &'a str,
    delim: char,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Vec<(usize, &'a str)>> {
    let mut out = Vec::new();
    let mut chunk_start = 0usize;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut brace_depth = 0i64;
    let mut bracket_depth = 0i64;
    for (offset, ch) in input.char_indices() {
        if offset % 4096 < ch.len_utf8() {
            control.checkpoint()?;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = in_quote {
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
            continue;
        }
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            _ => {}
        }
        if ch == delim && brace_depth == 0 && bracket_depth == 0 {
            out.push((chunk_start, &input[chunk_start..offset]));
            chunk_start = offset + ch.len_utf8();
            continue;
        }
    }
    out.push((chunk_start, &input[chunk_start..]));
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
struct TokenParserCheckpoint {
    pos: usize,
}

struct TokenParser<'input> {
    input: &'input str,
    pos: usize,
    base_offset: usize,
}

impl<'input> TokenParser<'input> {
    fn new(input: &'input str, base_offset: usize) -> Self {
        Self {
            input,
            pos: 0,
            base_offset,
        }
    }

    fn checkpoint(&self) -> TokenParserCheckpoint {
        TokenParserCheckpoint { pos: self.pos }
    }

    fn rollback(&mut self, checkpoint: TokenParserCheckpoint) {
        self.pos = checkpoint.pos;
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
                continue;
            }
            break;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn try_consume(&mut self, ch: char) -> bool {
        if self.input[self.pos..].starts_with(ch) {
            self.pos += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn parse_id(&mut self) -> Option<SpannedText> {
        let start = self.pos;
        let s = &self.input[self.pos..];
        let mut chars = s.chars();
        let first = chars.next()?;
        if !(first.is_ascii_alphanumeric() || first == '_') {
            return None;
        }
        let mut idx = first.len_utf8();
        let mut last = first;
        for ch in chars {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                idx += ch.len_utf8();
                last = ch;
            } else {
                break;
            }
        }
        if last == '-' {
            return None;
        }
        let raw = &s[..idx];
        self.pos += idx;
        Some(SpannedText {
            text: raw.to_string(),
            span: SourceSpan::new(self.base_offset + start, self.base_offset + self.pos),
        })
    }

    fn parse_bool(&mut self) -> Option<(bool, SourceSpan)> {
        let start = self.pos;
        if self.input[self.pos..].starts_with("true") {
            self.pos += 4;
            let span = SourceSpan::new(self.base_offset + start, self.base_offset + self.pos);
            return Some((true, span));
        }
        if self.input[self.pos..].starts_with("false") {
            self.pos += 5;
            let span = SourceSpan::new(self.base_offset + start, self.base_offset + self.pos);
            return Some((false, span));
        }
        None
    }

    fn parse_number_value(&mut self) -> Option<SpannedRadarValue> {
        let start = self.pos;
        let s = &self.input[self.pos..];
        if !s.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            return None;
        }
        let mut idx = 0usize;
        let mut saw_dot = false;
        for ch in s.chars() {
            if ch.is_ascii_digit() {
                idx += ch.len_utf8();
                continue;
            }
            if ch == '.' && !saw_dot {
                saw_dot = true;
                idx += 1;
                continue;
            }
            break;
        }
        if idx == 0 {
            return None;
        }
        let token = &s[..idx];

        if saw_dot {
            if token.ends_with('.') {
                return None;
            }
            let v: f64 = token.parse().ok()?;
            self.pos += idx;
            let n = serde_json::Number::from_f64(v)?;
            let span = SourceSpan::new(self.base_offset + start, self.base_offset + self.pos);
            return Some(SpannedRadarValue {
                value: Value::Number(n),
                span,
            });
        }

        if token.len() > 1 && token.starts_with('0') {
            return None;
        }
        let v: i64 = token.parse().ok()?;
        self.pos += idx;
        let span = SourceSpan::new(self.base_offset + start, self.base_offset + self.pos);
        Some(SpannedRadarValue {
            value: Value::Number(serde_json::Number::from(v)),
            span,
        })
    }

    fn parse_quoted_string(&mut self) -> Option<SpannedText> {
        let parsed = parse_langium_string(&self.input[self.pos..], self.base_offset + self.pos)?;
        self.pos += parsed.consumed;
        Some(SpannedText {
            text: parsed.value,
            span: parsed.value_span,
        })
    }

    fn take_until_matching_brace(
        &mut self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<std::result::Result<(&'input str, usize), String>> {
        let mut depth = 1i64;
        let mut in_quote: Option<char> = None;
        let mut escaped = false;
        let content_start = self.pos;
        while let Some(ch) = self.peek_char() {
            let char_start = self.pos;
            if char_start % 4096 < ch.len_utf8() {
                control.checkpoint()?;
            }
            self.pos += ch.len_utf8();
            if escaped {
                escaped = false;
                continue;
            }
            if let Some(q) = in_quote {
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == q {
                    in_quote = None;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_quote = Some(ch);
                continue;
            }
            if ch == '{' {
                depth += 1;
                continue;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Ok(Ok((
                        &self.input[content_start..char_start],
                        self.base_offset + content_start,
                    )));
                }
            }
        }
        Ok(Err("unterminated '{' in curve".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorSemanticRole, Engine, ParseOptions};
    use futures::executor::block_on;
    use serde_json::json;

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

    #[test]
    fn radar_parser_can_cancel_inside_an_axis_list() {
        let axes = (0..512)
            .map(|index| format!("axis_{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("radar-beta\naxis {axes}\n");
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(20);
        let meta = ParseMetadata {
            diagram_type: "radar".to_string(),
            config: crate::MermaidConfig::empty_object(),
            effective_config: crate::MermaidConfig::empty_object(),
            title: None,
        };

        assert!(matches!(
            construct_radar_semantic_source_controlled(&text, &meta, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn radar_projection_can_cancel_while_indexing_detailed_entries() {
        let axes = (0..512)
            .map(|index| AxisAst {
                name: SpannedText {
                    text: format!("axis_{index}"),
                    span: SourceSpan::new(index, index + 1),
                },
                label: None,
            })
            .collect();
        let entries = (0..512)
            .map(|index| EntryAst {
                axis: Some(SpannedText {
                    text: format!("axis_{index}"),
                    span: SourceSpan::new(index, index + 1),
                }),
                value: SpannedRadarValue {
                    value: json!(index),
                    span: SourceSpan::new(index, index + 1),
                },
            })
            .collect();
        let mut db = RadarDb::new();
        db.set_axes_controlled(axes, &crate::OperationControl::new())
            .unwrap();
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(10);

        assert!(matches!(
            db.set_curves_controlled(
                vec![CurveAst {
                    name: SpannedText {
                        text: "current".to_string(),
                        span: SourceSpan::new(0, 1),
                    },
                    label: None,
                    entries,
                    span: SourceSpan::new(0, 1),
                }],
                &control,
            ),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn radar_detailed_entries_keep_the_first_duplicate_value() {
        let axes = vec![RadarRenderAxis {
            name: "cost".to_string(),
            label: "Cost".to_string(),
        }];
        let entries = [1, 2].map(|value| EntryAst {
            axis: Some(SpannedText {
                text: "cost".to_string(),
                span: SourceSpan::new(0, 1),
            }),
            value: SpannedRadarValue {
                value: json!(value),
                span: SourceSpan::new(0, 1),
            },
        });

        assert_eq!(
            compute_curve_entries_controlled(&axes, &entries, &crate::OperationControl::new())
                .unwrap()
                .unwrap(),
            vec![json!(1)]
        );
    }

    #[test]
    fn radar_parses_simple_definition() {
        let _ = parse(
            r#"radar-beta
axis A,B,C
curve mycurve{1,2,3}"#,
        );
    }

    #[test]
    fn radar_errors_on_empty_axis_statement() {
        assert_eq!(
            parse_err(
                r#"radar-beta
axis"#,
            ),
            "axis statement must include at least one axis"
        );
    }

    #[test]
    fn radar_parses_title_and_data() {
        let model = parse(
            r#"radar-beta
title Radar diagram
accTitle: Radar accTitle
accDescr: Radar accDescription
axis A["Axis A"], B["Axis B"] ,C["Axis C"]
curve mycurve["My Curve"]{1,2,3}
"#,
        );
        assert_eq!(model["title"], json!("Radar diagram"));
        assert_eq!(model["accTitle"], json!("Radar accTitle"));
        assert_eq!(model["accDescr"], json!("Radar accDescription"));
        assert_eq!(
            model["axes"],
            json!([
                {"name": "A", "label": "Axis A"},
                {"name": "B", "label": "Axis B"},
                {"name": "C", "label": "Axis C"},
            ])
        );
        assert_eq!(
            model["curves"],
            json!([
                {"name": "mycurve", "label": "My Curve", "entries": [1,2,3]},
            ])
        );
        assert_eq!(
            model["options"],
            json!({"showLegend": true, "ticks": 5, "max": Value::Null, "min": 0, "graticule": "circle"})
        );
    }

    #[test]
    fn radar_uses_langium_string_escapes_and_quote_aware_inline_comments() {
        let model = parse(
            r#"radar-beta
axis A["A\n100%% complete"], B['B\tlabel'] %% outside comment
curve sample{1,2}
"#,
        );

        assert_eq!(model["axes"][0]["label"], "An100%% complete");
        assert_eq!(model["axes"][1]["label"], "Btlabel");
    }

    #[test]
    fn radar_parses_options() {
        let model = parse(
            r#"radar-beta
ticks 10
showLegend false
graticule polygon
min 1
max 10
"#,
        );
        assert_eq!(
            model["options"],
            json!({"showLegend": false, "ticks": 10, "max": 10, "min": 1, "graticule": "polygon"})
        );
    }

    #[test]
    fn radar_clamps_ticks_to_the_mermaid_limit() {
        for (ticks, expected) in [(12, 12), (32, 32), (33, 32)] {
            let model = parse(&format!("radar-beta\nticks {ticks}\n"));
            assert_eq!(model["options"]["ticks"], json!(expected));
        }
    }

    #[test]
    fn radar_parses_comma_separated_option_list_like_langium_grammar() {
        let model = parse(
            r#"radar-beta
ticks 5, max 10, min 1, graticule polygon, showLegend false
"#,
        );
        assert_eq!(
            model["options"],
            json!({"showLegend": false, "ticks": 5, "max": 10, "min": 1, "graticule": "polygon"})
        );
    }

    #[test]
    fn radar_editor_facts_preserve_every_repeated_occurrence_span() {
        let text = concat!(
            "radar-beta\r\n",
            "axis A[\"dup\"], A[\"dup\"]\r\n",
            "curve C[\"dup\"]{ A: 1,\r\n",
            "  A: 1 }, C[\"dup\"]{1,1}\r\n",
            "ticks 5\r\n",
            "ticks 5\r\n",
        );
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("radar", text)
            .unwrap()
            .unwrap();

        let occurrences = |needle: &str| {
            text.match_indices(needle)
                .map(|(start, value)| SourceSpan::new(start, start + value.len()))
                .collect::<Vec<_>>()
        };
        let spans_for = |detail: &str| {
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.detail.as_deref() == Some(detail))
                .map(|symbol| symbol.span)
                .collect::<Vec<_>>()
        };

        let axis_occurrences = occurrences("A");
        assert_eq!(spans_for("radar axis"), axis_occurrences[..2]);
        assert_eq!(
            spans_for("radar curve axis reference"),
            axis_occurrences[2..]
        );
        let axis_roles = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "A")
            .map(|symbol| symbol.role)
            .collect::<Vec<_>>();
        assert_eq!(
            axis_roles,
            vec![
                EditorSemanticRole::Entity,
                EditorSemanticRole::Entity,
                EditorSemanticRole::Reference,
                EditorSemanticRole::Reference,
            ]
        );

        let label_occurrences = occurrences("dup");
        assert_eq!(spans_for("radar axis label"), label_occurrences[..2]);
        assert_eq!(spans_for("radar curve label"), label_occurrences[2..]);
        assert_eq!(spans_for("radar curve"), occurrences("C"));
        assert_eq!(spans_for("radar curve entry"), occurrences("1"));
        assert_eq!(spans_for("radar option"), occurrences("5"));
    }

    #[test]
    fn radar_forward_axis_occurrence_is_a_reference_before_its_definition() {
        let text = concat!("radar-beta\n", "curve Forward{Later: 1}\n", "axis Later\n",);
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("radar", text)
            .unwrap()
            .unwrap();
        let later = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Later")
            .collect::<Vec<_>>();

        assert_eq!(later.len(), 2);
        assert_eq!(later[0].role, EditorSemanticRole::Reference);
        assert_eq!(later[1].role, EditorSemanticRole::Entity);
        assert_eq!(later[0].kind, later[1].kind);
    }

    #[test]
    fn radar_entity_occurrence_role_does_not_depend_on_display_detail() {
        let span = SourceSpan::new(0, "axis".len());
        let mut facts = EditorSemanticFacts::new();
        push_radar_entity(
            &mut facts,
            SpannedText {
                text: "axis".to_string(),
                span,
            },
            "reference-looking definition",
            EditorSemanticKind::Variable,
            RadarEntityOccurrence::Definition,
        );
        push_radar_entity(
            &mut facts,
            SpannedText {
                text: "axis".to_string(),
                span,
            },
            "definition-looking reference",
            EditorSemanticKind::Variable,
            RadarEntityOccurrence::Reference,
        );

        assert_eq!(facts.symbols[0].role, EditorSemanticRole::Entity);
        assert_eq!(facts.symbols[1].role, EditorSemanticRole::Reference);
    }

    #[test]
    fn radar_errors_on_empty_curve_stmt() {
        let err = parse_err(
            r#"radar-beta
axis my-axis
curve
"#,
        );
        assert_eq!(err, "expected curve id");
    }

    #[test]
    fn radar_rejects_empty_curve_and_entry_list_elements_like_langium_grammar() {
        for (body, expected) in [
            ("curve C{}", "expected curve entry"),
            ("curve C{1,}", "expected curve entry"),
            ("curve C{,1}", "expected curve entry"),
            ("curve C{1},", "expected curve after ','"),
        ] {
            let err = parse_err(&format!("radar-beta\n{body}\n"));
            assert_eq!(err, expected, "unexpected result for {body:?}");
        }
    }

    #[test]
    fn radar_number_and_id_terminals_match_common_langium() {
        for body in ["ticks .5", "curve C{.5}"] {
            let _ = parse_err(&format!("radar-beta\n{body}\n"));
        }

        let model = parse("radar-beta\naxis 1A, 2B\ncurve 3C{1A: 1, 2B: 2}\n");
        assert_eq!(model["axes"][0]["name"], "1A");
        assert_eq!(model["axes"][1]["name"], "2B");
        assert_eq!(model["curves"][0]["name"], "3C");
        assert_eq!(model["curves"][0]["entries"], json!([1, 2]));
    }

    #[test]
    fn radar_recovery_reports_parser_diagnostic_with_exact_crlf_span() {
        let text = concat!(
            "radar-beta\r\n",
            "axis A\r\n",
            "  curve C{.5} %% hidden\r\n",
            "axis B\r\n",
        );
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("radar", text)
            .unwrap()
            .unwrap();
        let invalid = "curve C{.5}";
        let start = text.find(invalid).unwrap();

        assert!(facts.symbols.iter().any(|symbol| symbol.name == "B"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(start, start + invalid.len()))
                && diagnostic.message.contains("expected entry number")
        }));
    }

    #[test]
    fn radar_editor_recovery_reports_curve_validation_errors() {
        let text = concat!(
            "radar-beta\r\n",
            "axis A[\"Axis A\"], B[\"Axis B\"]\r\n",
            "  curve sample{A: 1}  \r\n",
        );
        let invalid = "sample{A: 1}";
        let start = text.find(invalid).unwrap();
        let expected_span = SourceSpan::new(start, start + invalid.len());
        let engine = Engine::new();

        let error = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect_err("a detailed curve missing an axis must fail strict parsing");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected radar parse diagnostic");
        };
        assert_eq!(diagnostic.span(), Some(expected_span));

        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("radar", text)
            .unwrap()
            .expect("radar editor recovery facts");
        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "sample"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(expected_span)
                && diagnostic.message.contains("Missing entry for axis Axis B")
        }));
    }

    #[test]
    fn radar_errors_on_mixed_numeric_and_detailed_curve_entries() {
        let err = parse_err(
            r#"radar-beta
axis ax1, ax2
curve my-curve { 1, ax1 2 }"#,
        );
        assert_eq!(err, "mixed detailed and numeric entries are not supported");
    }

    #[test]
    fn radar_orders_detailed_curve_entries_by_axes() {
        let model = parse(
            r#"radar-beta
axis A,B,C
curve mycurve{ C: 3, A: 1, B: 2 }"#,
        );
        assert_eq!(
            model["curves"],
            json!([
                {"name": "mycurve", "label": "mycurve", "entries": [1,2,3]},
            ])
        );
    }

    #[test]
    fn radar_accepts_header_with_colon() {
        let _ = parse(
            r#"radar-beta:
axis A,B,C
curve mycurve{1,2,3}"#,
        );
    }

    #[test]
    fn radar_ignores_comment_lines() {
        let _ = parse(
            r#"radar-beta
%% This is a comment
axis A,B,C
%% This is another comment
curve mycurve{1,2,3}
"#,
        );
    }

    #[test]
    fn radar_errors_on_missing_axis_entry() {
        let err = parse_err(
            r#"radar-beta
axis A["Axis A"], B["Axis B"], C["Axis C"]
curve mycurve{ C: 3, A: 1 }"#,
        );
        assert_eq!(err, "Missing entry for axis Axis B");
    }

    #[test]
    fn radar_parses_config_override_directive() {
        let model = parse(
            r#"
%%{init: {'radar': {'marginTop': 80, 'axisLabelFactor': 1.25}}}%%
radar-beta
axis A,B,C
curve mycurve{1,2,3}
"#,
        );
        assert_eq!(model["config"]["radar"]["marginTop"], json!(80));
    }

    #[test]
    fn radar_typed_projection_matches_complete_and_empty_compat_json() {
        let text = concat!(
            "radar-beta\n",
            "title Delivery risk\n",
            "axis Cost,Time\n",
            "curve Current{2,3}\n",
        );
        let engine = Engine::new();
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let typed = parse_radar_model_for_render(text, &parsed.meta).unwrap();
        let projection = render_model_to_compat_json(&typed, &parsed.meta).unwrap();

        assert_eq!(projection, parsed.model);
        assert_eq!(projection["type"], json!("radar"));
        assert!(projection["config"].is_object());
        assert_eq!(projection["accTitle"], Value::Null);
        assert_eq!(projection["accDescr"], Value::Null);

        let empty_meta = ParseMetadata {
            diagram_type: "radar".to_string(),
            config: crate::MermaidConfig::empty_object(),
            effective_config: crate::MermaidConfig::empty_object(),
            title: None,
        };
        let empty = parse_radar_model_for_render("", &empty_meta).unwrap();
        assert_eq!(
            render_model_to_compat_json(&empty, &empty_meta).unwrap(),
            json!({})
        );
        assert_eq!(
            render_model_to_compat_json(&RadarDiagramRenderModel::default(), &empty_meta).unwrap()
                ["type"],
            json!("radar")
        );
    }
}

use crate::diagrams::scan::leading_whitespace_len;
use crate::sanitize::sanitize_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, MermaidConfig, ParseMetadata, Result, SourceSpan,
    family::CombinedSemanticFailure,
};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Number, Value};
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static XYCHART_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_xychart_syntax_construction_count() {
    XYCHART_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
fn xychart_syntax_construction_count() -> usize {
    XYCHART_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum XyChartAxisRenderModel {
    #[serde(rename = "band")]
    Band {
        #[serde(default)]
        title: String,
        #[serde(default)]
        categories: Vec<String>,
    },
    #[serde(rename = "linear")]
    Linear {
        #[serde(default)]
        title: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XyChartPlotType {
    #[serde(rename = "line")]
    Line,
    #[serde(rename = "bar")]
    Bar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XyChartPlotRenderModel {
    #[serde(rename = "type")]
    pub plot_type: XyChartPlotType,
    #[serde(default, serialize_with = "serialize_xychart_plot_title")]
    pub title: Option<String>,
    pub values: Vec<f64>,
    pub data: Vec<(String, Option<f64>)>,
    #[serde(rename = "pointLabels", default, skip_serializing_if = "Vec::is_empty")]
    pub point_labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct XyChartAxisDisplayPolicy {
    pub show_label: bool,
    pub show_title: bool,
    pub show_tick: bool,
    pub show_axis_line: bool,
}

impl Default for XyChartAxisDisplayPolicy {
    fn default() -> Self {
        Self {
            show_label: true,
            show_title: true,
            show_tick: true,
            show_axis_line: true,
        }
    }
}

impl XyChartAxisDisplayPolicy {
    fn from_config(config: &MermaidConfig, axis_key: &str) -> Self {
        let default = Self::default();
        Self {
            show_label: config
                .get_bool(&format!("xyChart.{axis_key}.showLabel"))
                .unwrap_or(default.show_label),
            show_title: config
                .get_bool(&format!("xyChart.{axis_key}.showTitle"))
                .unwrap_or(default.show_title),
            show_tick: config
                .get_bool(&format!("xyChart.{axis_key}.showTick"))
                .unwrap_or(default.show_tick),
            show_axis_line: config
                .get_bool(&format!("xyChart.{axis_key}.showAxisLine"))
                .unwrap_or(default.show_axis_line),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct XyChartDisplayPolicy {
    pub show_title: bool,
    pub show_data_label: bool,
    pub show_data_label_outside_bar: bool,
    pub x_axis: XyChartAxisDisplayPolicy,
    pub y_axis: XyChartAxisDisplayPolicy,
}

impl Default for XyChartDisplayPolicy {
    fn default() -> Self {
        Self {
            show_title: true,
            show_data_label: false,
            show_data_label_outside_bar: false,
            x_axis: XyChartAxisDisplayPolicy::default(),
            y_axis: XyChartAxisDisplayPolicy::default(),
        }
    }
}

impl XyChartDisplayPolicy {
    fn from_config(config: &MermaidConfig) -> Self {
        let default = Self::default();
        Self {
            show_title: config
                .get_bool("xyChart.showTitle")
                .unwrap_or(default.show_title),
            show_data_label: config
                .get_bool("xyChart.showDataLabel")
                .unwrap_or(default.show_data_label),
            show_data_label_outside_bar: config
                .get_bool("xyChart.showDataLabelOutsideBar")
                .unwrap_or(default.show_data_label_outside_bar),
            x_axis: XyChartAxisDisplayPolicy::from_config(config, "xAxis"),
            y_axis: XyChartAxisDisplayPolicy::from_config(config, "yAxis"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XyChartDiagramRenderModel {
    #[serde(default)]
    pub orientation: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(rename = "xAxis")]
    pub x_axis: XyChartAxisRenderModel,
    #[serde(rename = "yAxis")]
    pub y_axis: XyChartAxisRenderModel,
    #[serde(default)]
    pub plots: Vec<XyChartPlotRenderModel>,
    #[serde(skip, default)]
    pub display: XyChartDisplayPolicy,
}

impl XyChartDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }

    pub(crate) fn to_compat_json(&self, meta: &ParseMetadata) -> Value {
        let mut out = Map::with_capacity(10);
        out.insert(
            "orientation".to_string(),
            Value::String(self.orientation.clone()),
        );
        out.insert("title".to_string(), option_string_value(&self.title));
        out.insert("accTitle".to_string(), option_string_value(&self.acc_title));
        out.insert("accDescr".to_string(), option_string_value(&self.acc_descr));
        out.insert("xAxis".to_string(), axis_value(&self.x_axis));
        out.insert("yAxis".to_string(), axis_value(&self.y_axis));
        out.insert("plots".to_string(), plots_value(&self.plots));
        out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
        out.insert(
            "config".to_string(),
            crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
        );
        Value::Object(out)
    }
}

fn option_string_value(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|value| Value::String(value.clone()))
        .unwrap_or(Value::Null)
}

fn serialize_xychart_plot_title<S>(
    title: &Option<String>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(title.as_deref().unwrap_or_default())
}

fn optional_f64_value(value: Option<f64>) -> Value {
    value
        .and_then(Number::from_f64)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn f64_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn axis_value(axis: &XyChartAxisRenderModel) -> Value {
    let mut out = Map::new();
    match axis {
        XyChartAxisRenderModel::Band { title, categories } => {
            out.insert("type".to_string(), Value::String("band".to_string()));
            out.insert("title".to_string(), Value::String(title.clone()));
            out.insert("categories".to_string(), string_array_value(categories));
        }
        XyChartAxisRenderModel::Linear { title, min, max } => {
            out.insert("type".to_string(), Value::String("linear".to_string()));
            out.insert("title".to_string(), Value::String(title.clone()));
            out.insert("min".to_string(), optional_f64_value(*min));
            out.insert("max".to_string(), optional_f64_value(*max));
        }
    }
    Value::Object(out)
}

fn plots_value(plots: &[XyChartPlotRenderModel]) -> Value {
    Value::Array(plots.iter().map(plot_value).collect())
}

fn plot_value(plot: &XyChartPlotRenderModel) -> Value {
    let mut out = Map::new();
    out.insert(
        "type".to_string(),
        Value::String(plot_type_name(plot.plot_type)),
    );
    out.insert(
        "title".to_string(),
        Value::String(plot.title.clone().unwrap_or_default()),
    );
    out.insert(
        "values".to_string(),
        Value::Array(plot.values.iter().copied().map(f64_value).collect()),
    );
    out.insert("data".to_string(), plot_data_value(&plot.data));
    if !plot.point_labels.is_empty() {
        out.insert(
            "pointLabels".to_string(),
            Value::Array(
                plot.point_labels
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    Value::Object(out)
}

fn plot_type_name(plot_type: XyChartPlotType) -> String {
    match plot_type {
        XyChartPlotType::Line => "line".to_string(),
        XyChartPlotType::Bar => "bar".to_string(),
    }
}

fn plot_data_value(data: &[(String, Option<f64>)]) -> Value {
    Value::Array(
        data.iter()
            .map(|(category, value)| {
                Value::Array(vec![
                    Value::String(category.clone()),
                    optional_f64_value(*value),
                ])
            })
            .collect(),
    )
}

#[derive(Debug, Clone)]
enum AxisData {
    Band {
        title: String,
        categories: Vec<String>,
    },
    Linear {
        title: String,
        min: f64,
        max: f64,
    },
}

#[derive(Debug, Clone)]
struct Plot {
    plot_type: XyChartPlotType,
    title: Option<String>,
    values: Vec<f64>,
    data: Vec<(String, Option<f64>)>,
    point_labels: Vec<String>,
}

#[derive(Debug, Clone)]
struct ParsedDataPoint {
    value: f64,
    label: String,
    label_span: Option<SourceSpan>,
}

struct XyChartSemanticSource {
    model: XyChartDiagramRenderModel,
    editor_facts: EditorSemanticFacts,
}

struct XyChartParseOutcome {
    source: XyChartSemanticSource,
    first_error: Option<Error>,
}

impl XyChartParseOutcome {
    fn into_strict_result(
        self,
    ) -> std::result::Result<XyChartSemanticSource, CombinedSemanticFailure> {
        match self.first_error {
            Some(error) => Err(CombinedSemanticFailure::parser_recovery(
                "xychart",
                error,
                self.source.editor_facts,
            )),
            None => Ok(self.source),
        }
    }
}

enum ParsedAxisData {
    None,
    Band {
        categories: Vec<String>,
        source: SpannedText,
    },
    Range {
        min: f64,
        max: f64,
        source: SpannedText,
    },
}

struct ParsedAxisStatement {
    title: Option<SpannedText>,
    data: ParsedAxisData,
}

struct ParsedPlotStatement {
    title: Option<SpannedText>,
    data: Vec<ParsedDataPoint>,
    data_source: SpannedText,
}

enum ParsedXyChartStatement {
    Title(SpannedText),
    AccessibilityTitle(SpannedText),
    AccessibilityDescription(SpannedText),
    XAxis(ParsedAxisStatement),
    YAxis(ParsedAxisStatement),
    Line(ParsedPlotStatement),
    Bar(ParsedPlotStatement),
}

#[derive(Debug, Clone)]
struct XyChartState {
    orientation: String,
    x_axis: AxisData,
    y_axis: AxisData,
    plots: Vec<Plot>,
    has_set_x_axis: bool,
    has_set_y_axis: bool,
}

impl XyChartState {
    fn new(meta: &ParseMetadata) -> Self {
        let orientation = meta
            .effective_config
            .get_str("xyChart.chartOrientation")
            .unwrap_or("vertical")
            .to_string();
        Self {
            orientation,
            x_axis: AxisData::Band {
                title: String::new(),
                categories: Vec::new(),
            },
            y_axis: AxisData::Linear {
                title: String::new(),
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
            },
            plots: Vec::new(),
            has_set_x_axis: false,
            has_set_y_axis: false,
        }
    }

    fn set_orientation(&mut self, o: &str) {
        if o.eq_ignore_ascii_case("horizontal") {
            self.orientation = "horizontal".to_string();
        } else {
            self.orientation = "vertical".to_string();
        }
    }

    fn set_x_axis_title(&mut self, title: &str, meta: &ParseMetadata) {
        let t = sanitize_text(title.trim(), &meta.effective_config);
        match &mut self.x_axis {
            AxisData::Band { title, .. } => *title = t,
            AxisData::Linear { title, .. } => *title = t,
        }
    }

    fn set_y_axis_title(&mut self, title: &str, meta: &ParseMetadata) {
        let t = sanitize_text(title.trim(), &meta.effective_config);
        match &mut self.y_axis {
            AxisData::Linear { title, .. } => *title = t,
            AxisData::Band { title, .. } => *title = t,
        }
    }

    fn set_x_axis_range(&mut self, min: f64, max: f64) {
        let title = match &self.x_axis {
            AxisData::Band { title, .. } => title.clone(),
            AxisData::Linear { title, .. } => title.clone(),
        };
        self.x_axis = AxisData::Linear { title, min, max };
        self.has_set_x_axis = true;
    }

    fn set_x_axis_band(&mut self, categories: Vec<String>, meta: &ParseMetadata) {
        let title = match &self.x_axis {
            AxisData::Band { title, .. } => title.clone(),
            AxisData::Linear { title, .. } => title.clone(),
        };
        let categories = categories
            .into_iter()
            .map(|c| sanitize_text(c.trim(), &meta.effective_config))
            .collect::<Vec<_>>();
        self.x_axis = AxisData::Band { title, categories };
        self.has_set_x_axis = true;
    }

    fn set_y_axis_range(&mut self, min: f64, max: f64) {
        let title = match &self.y_axis {
            AxisData::Linear { title, .. } => title.clone(),
            AxisData::Band { title, .. } => title.clone(),
        };
        self.y_axis = AxisData::Linear { title, min, max };
        self.has_set_y_axis = true;
    }

    fn set_y_axis_range_from_plot_data(&mut self, data: &[f64]) {
        let min_value = data.iter().copied().fold(f64::INFINITY, |a, b| a.min(b));
        let max_value = data
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, |a, b| a.max(b));

        let (prev_min, prev_max, title) = match &self.y_axis {
            AxisData::Linear { min, max, title } => (*min, *max, title.clone()),
            AxisData::Band { title, .. } => (f64::INFINITY, f64::NEG_INFINITY, title.clone()),
        };

        self.y_axis = AxisData::Linear {
            title,
            min: prev_min.min(min_value),
            max: prev_max.max(max_value),
        };
    }

    fn transform_data_without_category(
        &mut self,
        mut data: Vec<f64>,
    ) -> (Vec<f64>, Vec<(String, Option<f64>)>) {
        if data.is_empty() {
            return (data, Vec::new());
        }

        if !self.has_set_x_axis {
            let (prev_min, prev_max) = match &self.x_axis {
                AxisData::Linear { min, max, .. } => (*min, *max),
                AxisData::Band { .. } => (f64::INFINITY, f64::NEG_INFINITY),
            };
            self.set_x_axis_range(prev_min.min(1.0), prev_max.max(data.len() as f64));
        }

        if let AxisData::Band { categories, .. } = &self.x_axis {
            data.truncate(categories.len());
        }

        if !self.has_set_y_axis {
            self.set_y_axis_range_from_plot_data(&data);
        }

        let plot_data = match &self.x_axis {
            AxisData::Band { categories, .. } => categories
                .iter()
                .enumerate()
                .map(|(i, c)| (c.clone(), data.get(i).copied()))
                .collect(),
            AxisData::Linear { min, max, .. } => {
                if data.len() == 1 {
                    vec![(format!("{min}"), data.first().copied())]
                } else {
                    let step = (*max - *min) / ((data.len() as f64) - 1.0);
                    data.iter()
                        .enumerate()
                        .map(|(index, datum)| {
                            (format!("{}", *min + (index as f64) * step), Some(*datum))
                        })
                        .collect()
                }
            }
        };
        (data, plot_data)
    }

    fn add_line_data(
        &mut self,
        title: Option<String>,
        data: Vec<ParsedDataPoint>,
        meta: &ParseMetadata,
    ) {
        let values = data.iter().map(|point| point.value).collect::<Vec<_>>();
        let mut point_labels = data
            .iter()
            .map(|point| {
                if point.label.is_empty() {
                    String::new()
                } else {
                    sanitize_text(&point.label, &meta.effective_config)
                }
            })
            .collect::<Vec<_>>();
        let (values, pairs) = self.transform_data_without_category(values);
        if point_labels.iter().all(String::is_empty) {
            point_labels.clear();
        }
        self.plots.push(Plot {
            plot_type: XyChartPlotType::Line,
            title,
            values,
            data: pairs,
            point_labels,
        });
    }

    fn add_bar_data(&mut self, title: Option<String>, data: Vec<ParsedDataPoint>) {
        let values = data.iter().map(|point| point.value).collect::<Vec<_>>();
        let (values, pairs) = self.transform_data_without_category(values);
        self.plots.push(Plot {
            plot_type: XyChartPlotType::Bar,
            title,
            values,
            data: pairs,
            point_labels: Vec::new(),
        });
    }

    fn into_render_model(
        self,
        title: Option<String>,
        acc_title: Option<String>,
        acc_descr: Option<String>,
        meta: &ParseMetadata,
    ) -> XyChartDiagramRenderModel {
        XyChartDiagramRenderModel {
            orientation: self.orientation,
            title,
            acc_title,
            acc_descr,
            x_axis: axis_data_to_render_model(self.x_axis),
            y_axis: axis_data_to_render_model(self.y_axis),
            plots: self
                .plots
                .into_iter()
                .map(|p| XyChartPlotRenderModel {
                    plot_type: p.plot_type,
                    title: p.title,
                    values: p.values,
                    data: p.data,
                    point_labels: p.point_labels,
                })
                .collect(),
            display: XyChartDisplayPolicy::from_config(&meta.effective_config),
        }
    }
}

fn normalize_xychart_error(error: Error, meta: &ParseMetadata, fallback_span: SourceSpan) -> Error {
    match error {
        Error::DiagramParse {
            diagram_type,
            diagnostic,
        } if diagnostic.span().is_some() => Error::DiagramParse {
            diagram_type,
            diagnostic,
        },
        Error::DiagramParse { diagnostic, .. } => Error::diagram_parse_exact(
            meta.diagram_type.clone(),
            diagnostic.message(),
            fallback_span,
        ),
        error => {
            Error::diagram_parse_exact(meta.diagram_type.clone(), error.to_string(), fallback_span)
        }
    }
}

fn record_xychart_error(first_error: &mut Option<Error>, error: Error) {
    first_error.get_or_insert(error);
}

fn push_xychart_axis_facts(
    facts: &mut EditorSemanticFacts,
    axis: &ParsedAxisStatement,
    detail: &'static str,
) {
    if let Some(title) = axis.title.as_ref() {
        push_xychart_payload_fact(
            facts,
            title.as_str(),
            SourceSpan::new(title.start, title.end),
            detail,
            EditorSemanticKind::String,
        );
    }
    let source = match &axis.data {
        ParsedAxisData::None => None,
        ParsedAxisData::Band { source, .. } | ParsedAxisData::Range { source, .. } => Some(source),
    };
    if let Some(source) = source {
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            SourceSpan::new(source.start, source.end),
        ));
    }
}

fn apply_x_axis_statement(
    axis: ParsedAxisStatement,
    state: &mut XyChartState,
    meta: &ParseMetadata,
) {
    state.set_x_axis_title(axis.title.as_ref().map_or("", SpannedText::as_str), meta);
    match axis.data {
        ParsedAxisData::None => {}
        ParsedAxisData::Band { categories, .. } => state.set_x_axis_band(categories, meta),
        ParsedAxisData::Range { min, max, .. } => state.set_x_axis_range(min, max),
    }
}

fn apply_y_axis_statement(
    axis: ParsedAxisStatement,
    state: &mut XyChartState,
    meta: &ParseMetadata,
) {
    state.set_y_axis_title(axis.title.as_ref().map_or("", SpannedText::as_str), meta);
    if let ParsedAxisData::Range { min, max, .. } = axis.data {
        state.set_y_axis_range(min, max);
    }
}

fn push_xychart_plot_statement_facts(
    facts: &mut EditorSemanticFacts,
    plot: &ParsedPlotStatement,
    detail: &'static str,
    data_label_detail: &'static str,
) {
    if let Some(title) = plot.title.as_ref() {
        push_xychart_payload_fact(
            facts,
            title.as_str(),
            SourceSpan::new(title.start, title.end),
            detail,
            EditorSemanticKind::String,
        );
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        SourceSpan::new(plot.data_source.start, plot.data_source.end),
    ));
    for point in &plot.data {
        let Some(span) = point.label_span else {
            continue;
        };
        push_xychart_payload_fact(
            facts,
            &point.label,
            span,
            data_label_detail,
            EditorSemanticKind::String,
        );
    }
}

fn parse_xychart_statement(
    statement_text: &str,
    statement_start: usize,
    diagram_type: &str,
) -> Result<ParsedXyChartStatement> {
    let statement_span = SourceSpan::new(statement_start, statement_start + statement_text.len());

    macro_rules! parse_keyword {
        ($keyword:literal, $parse:expr, $variant:path) => {
            if let Some(rest) = strip_keyword(statement_text, $keyword) {
                let rest_start = statement_start + statement_text.len().saturating_sub(rest.len());
                return $parse(rest, rest_start).map($variant);
            }
        };
    }

    parse_keyword!("title", parse_text_source, ParsedXyChartStatement::Title);
    if let Some(rest) = strip_keyword(statement_text, "accTitle") {
        let rest_start = statement_start + statement_text.len().saturating_sub(rest.len());
        return parse_colon_value_source(rest, rest_start, "accTitle")
            .map(ParsedXyChartStatement::AccessibilityTitle);
    }
    parse_keyword!(
        "accDescr",
        parse_acc_descr_source,
        ParsedXyChartStatement::AccessibilityDescription
    );
    parse_keyword!("x-axis", parse_x_axis_source, ParsedXyChartStatement::XAxis);
    parse_keyword!("y-axis", parse_y_axis_source, ParsedXyChartStatement::YAxis);
    parse_keyword!("line", parse_plot_stmt_source, ParsedXyChartStatement::Line);
    parse_keyword!("bar", parse_plot_stmt_source, ParsedXyChartStatement::Bar);

    Err(Error::diagram_parse_exact(
        diagram_type.to_string(),
        format!("unexpected xychart statement: {statement_text}"),
        statement_span,
    ))
}

fn construct_xychart_semantic_source(code: &str, meta: &ParseMetadata) -> XyChartParseOutcome {
    construct_xychart_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_xychart_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<XyChartParseOutcome> {
    #[cfg(test)]
    XYCHART_SYNTAX_CONSTRUCTION_COUNT.set(XYCHART_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    let syntax = split_statements_spanned_controlled(code, control)?;
    let mut statements = syntax
        .statements
        .into_iter()
        .filter(|statement| !statement.text.trim().is_empty());
    let Some(header) = statements.next() else {
        return Ok(XyChartParseOutcome {
            source: XyChartSemanticSource {
                model: XyChartState::new(meta).into_render_model(None, None, None, meta),
                editor_facts: EditorSemanticFacts::new(),
            },
            first_error: None,
        });
    };

    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_error = None;
    let mut state = XyChartState::new(meta);
    let header_trimmed = header.text.trim();
    let header_start = header.trimmed_start();
    let header_span = SourceSpan::new(header_start, header_start + header_trimmed.len());
    if let Err(error) = parse_header(&header.text, header_start, &mut state) {
        let error = normalize_xychart_error(error, meta, header_span);
        first_error = Some(error);
    }
    if let Some((prefix_len, _)) = header_token_len_and_rest(header_trimmed) {
        editor_facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            SourceSpan::new(header_start, header_start + prefix_len),
        ));
    }

    let mut title = None;
    let mut acc_title = None;
    let mut acc_descr = None;

    for (index, statement) in statements.enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let statement_start = statement.trimmed_start();
        let statement_text = statement.text.trim();
        if statement_text.is_empty() {
            continue;
        }
        let statement_span =
            SourceSpan::new(statement_start, statement_start + statement_text.len());
        let parsed =
            match parse_xychart_statement(statement_text, statement_start, &meta.diagram_type) {
                Ok(parsed) => parsed,
                Err(error) => {
                    record_xychart_error(
                        &mut first_error,
                        normalize_xychart_error(error, meta, statement_span),
                    );
                    continue;
                }
            };

        match parsed {
            ParsedXyChartStatement::Title(value) => {
                editor_facts.push_directive_prefix("title");
                push_xychart_payload_fact(
                    &mut editor_facts,
                    value.as_str(),
                    SourceSpan::new(value.start, value.end),
                    "xychart title",
                    EditorSemanticKind::String,
                );
                title = Some(value.text.trim().to_string());
            }
            ParsedXyChartStatement::AccessibilityTitle(value) => {
                editor_facts.push_directive_prefix("accTitle");
                if !value.text.is_empty() {
                    push_xychart_payload_fact(
                        &mut editor_facts,
                        value.as_str(),
                        SourceSpan::new(value.start, value.end),
                        "xychart accessibility title",
                        EditorSemanticKind::String,
                    );
                }
                acc_title = Some(value.text);
            }
            ParsedXyChartStatement::AccessibilityDescription(value) => {
                editor_facts.push_directive_prefix("accDescr");
                if !value.text.is_empty() {
                    push_xychart_payload_fact(
                        &mut editor_facts,
                        value.as_str(),
                        SourceSpan::new(value.start, value.end),
                        "xychart accessibility description",
                        EditorSemanticKind::String,
                    );
                }
                acc_descr = Some(value.text);
            }
            ParsedXyChartStatement::XAxis(axis) => {
                push_xychart_axis_facts(&mut editor_facts, &axis, "xychart x-axis");
                apply_x_axis_statement(axis, &mut state, meta);
            }
            ParsedXyChartStatement::YAxis(axis) => {
                push_xychart_axis_facts(&mut editor_facts, &axis, "xychart y-axis");
                apply_y_axis_statement(axis, &mut state, meta);
            }
            ParsedXyChartStatement::Line(plot) => {
                push_xychart_plot_statement_facts(
                    &mut editor_facts,
                    &plot,
                    "xychart line",
                    "xychart line data label",
                );
                state.add_line_data(
                    plot.title
                        .as_ref()
                        .and_then(|title| plot_title_value(title.as_str(), meta)),
                    plot.data,
                    meta,
                );
            }
            ParsedXyChartStatement::Bar(plot) => {
                push_xychart_plot_statement_facts(
                    &mut editor_facts,
                    &plot,
                    "xychart bar",
                    "xychart bar data label",
                );
                state.add_bar_data(
                    plot.title
                        .as_ref()
                        .and_then(|title| plot_title_value(title.as_str(), meta)),
                    plot.data,
                );
            }
        }
    }

    control.checkpoint()?;
    Ok(XyChartParseOutcome {
        source: XyChartSemanticSource {
            model: state.into_render_model(title, acc_title, acc_descr, meta),
            editor_facts,
        },
        first_error,
    })
}

pub(crate) fn parse_xychart(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = construct_xychart_semantic_source(code, meta)
        .into_strict_result()
        .map_err(CombinedSemanticFailure::into_error)?;
    render_model_to_compat_json(&source.model, meta)
}

pub(crate) fn parse_xychart_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construct_xychart_semantic_source_controlled(code, meta, control)?.into_strict_result(),
        |XyChartSemanticSource {
             model,
             editor_facts,
         }| {
            let model = render_model_to_compat_json(&model, meta);
            (model, editor_facts)
        },
        CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_xychart_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<XyChartDiagramRenderModel> {
    construct_xychart_semantic_source(code, meta)
        .into_strict_result()
        .map(|source| source.model)
        .map_err(CombinedSemanticFailure::into_error)
}

pub(crate) fn parse_xychart_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<XyChartDiagramRenderModel>> {
    let outcome = construct_xychart_semantic_source_controlled(code, meta, control)?;
    let source = match outcome.into_strict_result() {
        Ok(source) => source,
        Err(failure) => return Ok(Err(failure.into_error())),
    };
    control.checkpoint()?;
    Ok(Ok(source.model))
}

pub(crate) fn render_model_to_compat_json(
    model: &XyChartDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    Ok(model.to_compat_json(meta))
}

fn plot_title_value(title: &str, meta: &ParseMetadata) -> Option<String> {
    let title = sanitize_text(title.trim(), &meta.effective_config);
    (!title.is_empty()).then_some(title)
}

fn axis_data_to_render_model(axis: AxisData) -> XyChartAxisRenderModel {
    match axis {
        AxisData::Band { title, categories } => XyChartAxisRenderModel::Band { title, categories },
        AxisData::Linear { title, min, max } => {
            let min = min.is_finite().then_some(min);
            let max = max.is_finite().then_some(max);
            XyChartAxisRenderModel::Linear { title, min, max }
        }
    }
}

fn parse_header(stmt: &str, statement_start: usize, state: &mut XyChartState) -> Result<()> {
    let t = stmt.trim();
    let trimmed_start = statement_start;
    let lower = t.to_ascii_lowercase();
    let (prefix, rest) = if lower.starts_with("xychart-beta") {
        ("xychart-beta", &t["xychart-beta".len()..])
    } else if lower.starts_with("xychart") {
        ("xychart", &t["xychart".len()..])
    } else {
        return Err(Error::diagram_parse_fallback(
            "xychart".to_string(),
            "expected xychart".to_string(),
        ));
    };
    let rem = rest.trim();
    if rem.is_empty() {
        return Ok(());
    }
    if !rest.starts_with(char::is_whitespace) {
        let rem_start = trimmed_start + prefix.len();
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            format!("unexpected token after {prefix}: {rem}"),
            SourceSpan::new(rem_start, trimmed_start + t.len()),
        ));
    }

    if rem.eq_ignore_ascii_case("vertical") || rem.eq_ignore_ascii_case("horizontal") {
        state.set_orientation(rem);
        return Ok(());
    }

    let rem_start = trimmed_start + t.len().saturating_sub(rem.len());
    Err(Error::diagram_parse_exact(
        "xychart".to_string(),
        format!("invalid chart orientation: {rem}"),
        SourceSpan::new(rem_start, rem_start + rem.len()),
    ))
}

fn header_token_len_and_rest(stmt: &str) -> Option<(usize, &str)> {
    let t = stmt.trim_start();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("xychart-beta") {
        return Some(("xychart-beta".len(), &t["xychart-beta".len()..]));
    }
    if lower.starts_with("xychart") {
        return Some(("xychart".len(), &t["xychart".len()..]));
    }
    None
}

fn strip_keyword<'a>(stmt: &'a str, kw: &str) -> Option<&'a str> {
    let s = stmt.trim_start();
    let lower = s.to_ascii_lowercase();
    let kw_lower = kw.to_ascii_lowercase();
    if !lower.starts_with(&kw_lower) {
        return None;
    }
    Some(&s[kw.len()..])
}

fn push_xychart_payload_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    span: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
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

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    start: usize,
    end: usize,
}

impl SpannedText {
    fn as_str(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Clone, Copy)]
struct SpannedSlice<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

impl<'a> SpannedSlice<'a> {
    fn new(text: &'a str, start: usize, end: usize) -> Self {
        Self { text, start, end }
    }

    fn trim(self) -> Self {
        let leading = self.text.len().saturating_sub(self.text.trim_start().len());
        let text = &self.text[leading..];
        let trimmed_len = text.trim_end().len();
        Self {
            text: &text[..trimmed_len],
            start: self.start + leading,
            end: self.start + leading + trimmed_len,
        }
    }

    fn to_text(self) -> SpannedText {
        SpannedText {
            text: self.text.to_string(),
            start: self.start,
            end: self.end,
        }
    }
}

fn parse_text_source(input: &str, input_start: usize) -> Result<SpannedText> {
    let (value, tail) = parse_text_prefix_source(input, input_start)?;
    let trailing = tail.trim();
    if !trailing.text.is_empty() {
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "unexpected trailing tokens after text".to_string(),
            SourceSpan::new(trailing.start, trailing.end),
        ));
    }
    Ok(value)
}

fn parse_text_prefix_source<'a>(
    input: &'a str,
    input_start: usize,
) -> Result<(SpannedText, SpannedSlice<'a>)> {
    let leading = leading_whitespace_len(input);
    let text = &input[leading..];
    let text_start = input_start + leading;
    if text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected text".to_string(),
            text_start,
        ));
    }

    if let Some(body) = text.strip_prefix("\"`") {
        let Some(end) = body.find("`\"") else {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "unterminated markdown string".to_string(),
                input_start + input.len(),
            ));
        };
        let value_start = text_start + 2;
        let tail_start = 2 + end + 2;
        return Ok((
            SpannedText {
                text: body[..end].to_string(),
                start: value_start,
                end: value_start + end,
            },
            SpannedSlice::new(
                &text[tail_start..],
                text_start + tail_start,
                input_start + input.len(),
            ),
        ));
    }

    if let Some(body) = text.strip_prefix('"') {
        let Some(end) = body.find('"') else {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "unterminated string".to_string(),
                input_start + input.len(),
            ));
        };
        let value_start = text_start + 1;
        let tail_start = 1 + end + 1;
        return Ok((
            SpannedText {
                text: body[..end].to_string(),
                start: value_start,
                end: value_start + end,
            },
            SpannedSlice::new(
                &text[tail_start..],
                text_start + tail_start,
                input_start + input.len(),
            ),
        ));
    }

    let bracket_start = text.find('[');
    let range_start = range_suffix_start(text);
    let arrow_start = text.find("-->").filter(|offset| {
        *offset == 0
            || text[..*offset]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
    });
    let tail_rel = [bracket_start, range_start, arrow_start]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(text.len());
    let raw_value = &text[..tail_rel];
    let value_len = raw_value.trim_end().len();
    let raw_value = &raw_value[..value_len];
    if raw_value.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected text".to_string(),
            text_start,
        ));
    }

    let mut value = String::new();
    for (offset, ch) in raw_value.char_indices() {
        if ch.is_whitespace() {
            continue;
        }
        if !is_text_token_char(ch) {
            return Err(Error::diagram_parse_exact(
                "xychart".to_string(),
                format!("unexpected token in text: {ch}"),
                SourceSpan::new(text_start + offset, text_start + offset + ch.len_utf8()),
            ));
        }
        value.push(ch);
    }
    if value.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected text".to_string(),
            text_start,
        ));
    }
    Ok((
        SpannedText {
            text: value,
            start: text_start,
            end: text_start + raw_value.len(),
        },
        SpannedSlice::new(
            &text[tail_rel..],
            text_start + tail_rel,
            input_start + input.len(),
        ),
    ))
}

fn is_text_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '&' | '+' | '=' | '*' | '.' | '#' | '-' | '_')
}

fn range_suffix_start(input: &str) -> Option<usize> {
    input.char_indices().find_map(|(offset, _)| {
        if offset == 0
            || !input[..offset]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            return None;
        }
        let (_, tail) = take_number_token(&input[offset..])?;
        tail.trim_start().starts_with("-->").then_some(offset)
    })
}

fn parse_colon_value_source(input: &str, input_start: usize, keyword: &str) -> Result<SpannedText> {
    let input = SpannedSlice::new(input, input_start, input_start + input.len()).trim();
    let Some(value) = input.text.strip_prefix(':') else {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            format!("expected ':' after {keyword}"),
            input.start,
        ));
    };
    let value = SpannedSlice::new(value, input.start + 1, input.end)
        .trim()
        .to_text();
    Ok(value)
}

fn parse_acc_descr_source(input: &str, input_start: usize) -> Result<SpannedText> {
    let input = SpannedSlice::new(input, input_start, input_start + input.len()).trim();
    if input.text.starts_with(':') {
        let value = SpannedSlice::new(&input.text[1..], input.start + 1, input.end)
            .trim()
            .to_text();
        return Ok(value);
    }

    let Some(body) = input.text.strip_prefix('{') else {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected ':' or '{' after accDescr".to_string(),
            input.start,
        ));
    };
    let Some(close) = body.find('}') else {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "unterminated accDescr block".to_string(),
            input.end,
        ));
    };
    let body_source =
        SpannedSlice::new(&body[..close], input.start + 1, input.start + 1 + close).trim();
    let trailing =
        SpannedSlice::new(&body[close + 1..], input.start + 1 + close + 1, input.end).trim();
    if !trailing.text.is_empty() {
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "unexpected trailing tokens after accDescr block".to_string(),
            SourceSpan::new(trailing.start, trailing.end),
        ));
    }
    Ok(body_source.to_text())
}

fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let unsigned = t
        .strip_prefix('+')
        .or_else(|| t.strip_prefix('-'))
        .unwrap_or(t);
    let valid = if let Some(fraction) = unsigned.strip_prefix('.') {
        !fraction.is_empty() && fraction.chars().all(|ch| ch.is_ascii_digit())
    } else if let Some((integer, fraction)) = unsigned.split_once('.') {
        !integer.is_empty()
            && !fraction.is_empty()
            && integer.chars().all(|ch| ch.is_ascii_digit())
            && fraction.chars().all(|ch| ch.is_ascii_digit())
    } else {
        !unsigned.is_empty() && unsigned.chars().all(|ch| ch.is_ascii_digit())
    };
    if !valid {
        return None;
    }
    t.parse::<f64>().ok()
}

fn parse_x_axis_source(rest: &str, rest_start: usize) -> Result<ParsedAxisStatement> {
    let input = SpannedSlice::new(rest, rest_start, rest_start + rest.len()).trim();
    if input.text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "x-axis requires data".to_string(),
            input.start,
        ));
    }

    if input.text.starts_with('[') {
        let (categories, source) = parse_text_list_source(input.text, input.start)?;
        return Ok(ParsedAxisStatement {
            title: None,
            data: ParsedAxisData::Band { categories, source },
        });
    }
    if let Some((min, max, source)) = try_parse_range_source(input.text, input.start)? {
        return Ok(ParsedAxisStatement {
            title: None,
            data: ParsedAxisData::Range { min, max, source },
        });
    }

    let (title, tail) = parse_text_prefix_source(input.text, input.start)?;
    let tail = tail.trim();
    if tail.text.is_empty() {
        return Ok(ParsedAxisStatement {
            title: Some(title),
            data: ParsedAxisData::None,
        });
    }
    if tail.text.starts_with('[') {
        let (categories, source) = parse_text_list_source(tail.text, tail.start)?;
        return Ok(ParsedAxisStatement {
            title: Some(title),
            data: ParsedAxisData::Band { categories, source },
        });
    }
    if let Some((min, max, source)) = try_parse_range_source(tail.text, tail.start)? {
        return Ok(ParsedAxisStatement {
            title: Some(title),
            data: ParsedAxisData::Range { min, max, source },
        });
    }
    Err(Error::diagram_parse_exact(
        "xychart".to_string(),
        "invalid x-axis data".to_string(),
        SourceSpan::new(tail.start, tail.end),
    ))
}

fn parse_y_axis_source(rest: &str, rest_start: usize) -> Result<ParsedAxisStatement> {
    let input = SpannedSlice::new(rest, rest_start, rest_start + rest.len()).trim();
    if input.text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "y-axis requires data".to_string(),
            input.start,
        ));
    }
    if input.text.starts_with('[') {
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "y-axis does not support band data".to_string(),
            SourceSpan::new(input.start, input.end),
        ));
    }
    if let Some((min, max, source)) = try_parse_range_source(input.text, input.start)? {
        return Ok(ParsedAxisStatement {
            title: None,
            data: ParsedAxisData::Range { min, max, source },
        });
    }

    let (title, tail) = parse_text_prefix_source(input.text, input.start)?;
    let tail = tail.trim();
    if tail.text.is_empty() {
        return Ok(ParsedAxisStatement {
            title: Some(title),
            data: ParsedAxisData::None,
        });
    }
    if tail.text.starts_with('[') {
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "y-axis does not support band data".to_string(),
            SourceSpan::new(tail.start, tail.end),
        ));
    }
    if let Some((min, max, source)) = try_parse_range_source(tail.text, tail.start)? {
        return Ok(ParsedAxisStatement {
            title: Some(title),
            data: ParsedAxisData::Range { min, max, source },
        });
    }
    Err(Error::diagram_parse_exact(
        "xychart".to_string(),
        "invalid y-axis data".to_string(),
        SourceSpan::new(tail.start, tail.end),
    ))
}

fn parse_plot_stmt_source(rest: &str, rest_start: usize) -> Result<ParsedPlotStatement> {
    let input = SpannedSlice::new(rest, rest_start, rest_start + rest.len()).trim();
    if input.text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "plot requires data".to_string(),
            input.start,
        ));
    }

    let (title, data_input) = if input.text.starts_with('[') {
        (None, input)
    } else {
        let (title, tail) = parse_text_prefix_source(input.text, input.start)?;
        let tail = tail.trim();
        if tail.text.is_empty() {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "plot data missing".to_string(),
                input.end,
            ));
        }
        if !tail.text.starts_with('[') {
            return Err(Error::diagram_parse_exact(
                "xychart".to_string(),
                "plot data missing".to_string(),
                SourceSpan::new(tail.start, tail.end),
            ));
        }
        (Some(title), tail)
    };
    let (data, data_source) =
        parse_data_point_list_in_brackets_spanned(data_input.text, data_input.start)?;
    Ok(ParsedPlotStatement {
        title,
        data,
        data_source,
    })
}

fn try_parse_range_source(
    input: &str,
    input_start: usize,
) -> Result<Option<(f64, f64, SpannedText)>> {
    let input = SpannedSlice::new(input, input_start, input_start + input.len()).trim();
    let Some((first, after_first)) = take_number_token(input.text) else {
        return Ok(None);
    };
    let after_first_start = input.start + first.len();
    let after_first = SpannedSlice::new(after_first, after_first_start, input.end).trim();
    if !after_first.text.starts_with("-->") {
        return Ok(None);
    }

    let first_value = parse_number(first).ok_or_else(|| {
        Error::diagram_parse_exact(
            "xychart".to_string(),
            format!("invalid number: {first}"),
            SourceSpan::new(input.start, input.start + first.len()),
        )
    })?;
    let second_input = SpannedSlice::new(
        &after_first.text[3..],
        after_first.start + 3,
        after_first.end,
    )
    .trim();
    let Some((second, trailing)) = take_number_token(second_input.text) else {
        if second_input.text.is_empty() {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "expected number".to_string(),
                second_input.start,
            ));
        }
        let token_len = second_input
            .text
            .find(char::is_whitespace)
            .unwrap_or(second_input.text.len());
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            format!("invalid number: {}", &second_input.text[..token_len]),
            SourceSpan::new(second_input.start, second_input.start + token_len),
        ));
    };
    let trailing_start = second_input.start + second.len();
    let trailing = SpannedSlice::new(trailing, trailing_start, second_input.end).trim();
    if !trailing.text.is_empty() {
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "unexpected trailing tokens after range".to_string(),
            SourceSpan::new(trailing.start, trailing.end),
        ));
    }

    let second_value = parse_number(second).ok_or_else(|| {
        Error::diagram_parse_exact(
            "xychart".to_string(),
            format!("invalid number: {second}"),
            SourceSpan::new(second_input.start, second_input.start + second.len()),
        )
    })?;
    let source_end = second_input.start + second.len();
    Ok(Some((
        first_value,
        second_value,
        SpannedText {
            text: input.text[..source_end - input.start].to_string(),
            start: input.start,
            end: source_end,
        },
    )))
}

fn take_number_token(input: &str) -> Option<(&str, &str)> {
    let mut idx = 0usize;
    for (i, ch) in input.char_indices() {
        if i == 0 && (ch == '+' || ch == '-') {
            idx = i + ch.len_utf8();
            continue;
        }
        if ch.is_ascii_digit() || ch == '.' {
            idx = i + ch.len_utf8();
            continue;
        }
        break;
    }
    if idx == 0 {
        return None;
    }
    Some((&input[..idx], &input[idx..]))
}

fn parse_text_list_source(input: &str, input_start: usize) -> Result<(Vec<String>, SpannedText)> {
    let (inner, inner_start) = extract_bracket_inner_spanned(input, input_start)?;
    let source = SpannedSlice::new(inner, inner_start, inner_start + inner.len()).trim();
    if source.text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "empty category".to_string(),
            source.start,
        ));
    }

    let mut categories = Vec::new();
    let parts = split_top_level_commas_spanned(inner, inner_start);
    for part in parts {
        let part = part.trim();
        if part.text.is_empty() {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "empty category".to_string(),
                part.start,
            ));
        }
        categories.push(parse_text_source(part.text, part.start)?.text);
    }
    Ok((categories, source.to_text()))
}

fn parse_data_point_list_in_brackets_spanned(
    input: &str,
    input_start: usize,
) -> Result<(Vec<ParsedDataPoint>, SpannedText)> {
    let leading = input.len().saturating_sub(input.trim_start().len());
    let t = input.trim_start();
    let t_start = input_start + leading;
    let (inner, inner_start) = extract_bracket_inner_spanned(t, t_start)?;
    let source = SpannedSlice::new(inner, inner_start, inner_start + inner.len()).trim();
    if source.text.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "plot data cannot be empty".to_string(),
            source.start,
        ));
    }
    let parts = split_top_level_commas_spanned(inner, inner_start);
    let mut out = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.text.is_empty() {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "empty number".to_string(),
                trimmed.start,
            ));
        }
        out.push(parse_data_point_spanned(trimmed)?);
    }
    Ok((out, source.to_text()))
}

fn parse_data_point_spanned(part: SpannedSlice<'_>) -> Result<ParsedDataPoint> {
    if let Some(quote_rel) = part.text.find('"') {
        let number_source =
            SpannedSlice::new(&part.text[..quote_rel], part.start, part.start + quote_rel).trim();
        let number_part = number_source.text;
        if number_part.is_empty() {
            return Err(Error::diagram_parse_insertion_point(
                "xychart".to_string(),
                "expected number".to_string(),
                part.start,
            ));
        }
        let number_start = number_source.start;
        let value = parse_number(number_part).ok_or_else(|| {
            Error::diagram_parse_exact(
                "xychart".to_string(),
                format!("invalid number: {number_part}"),
                SourceSpan::new(number_start, number_start + number_part.len()),
            )
        })?;
        let label =
            parse_data_point_label_spanned(&part.text[quote_rel..], part.start + quote_rel)?;
        return Ok(ParsedDataPoint {
            value,
            label: label.text,
            label_span: Some(SourceSpan::new(label.start, label.end)),
        });
    }

    let value = parse_number(part.text).ok_or_else(|| {
        Error::diagram_parse_exact(
            "xychart".to_string(),
            format!("invalid number: {}", part.text),
            SourceSpan::new(part.start, part.end),
        )
    })?;
    Ok(ParsedDataPoint {
        value,
        label: String::new(),
        label_span: None,
    })
}

fn parse_data_point_label_spanned(input: &str, input_start: usize) -> Result<SpannedText> {
    let Some(body) = input.strip_prefix('"') else {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected data label".to_string(),
            input_start,
        ));
    };
    let Some(end) = body.find('"') else {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "unterminated data label".to_string(),
            input_start,
        ));
    };
    let rest = &body[end + 1..];
    if !rest.trim().is_empty() {
        let leading = rest.len().saturating_sub(rest.trim_start().len());
        return Err(Error::diagram_parse_exact(
            "xychart".to_string(),
            "unexpected trailing tokens after data label".to_string(),
            SourceSpan::new(
                input_start + 1 + end + 1 + leading,
                input_start + input.len(),
            ),
        ));
    }
    Ok(SpannedText {
        text: body[..end].to_string(),
        start: input_start + 1,
        end: input_start + 1 + end,
    })
}

fn extract_bracket_inner_spanned(input: &str, input_start: usize) -> Result<(&str, usize)> {
    let t = input.trim_start();
    let t_start = input_start + input.len().saturating_sub(t.len());
    if !t.starts_with('[') {
        return Err(Error::diagram_parse_insertion_point(
            "xychart".to_string(),
            "expected '['".to_string(),
            t_start,
        ));
    }
    let mut in_quote = false;
    let mut in_md = false;
    let mut idx = 1usize;
    while idx < t.len() {
        let rest = &t[idx..];
        let ch = rest.chars().next().unwrap();
        if in_md {
            if rest.starts_with("`\"") {
                in_md = false;
                idx += 2;
                continue;
            }
            idx += ch.len_utf8();
            continue;
        }
        if in_quote {
            if ch == '"' {
                in_quote = false;
            }
            idx += ch.len_utf8();
            continue;
        }
        if rest.starts_with("\"`") {
            in_md = true;
            idx += 2;
            continue;
        }
        if ch == '"' {
            in_quote = true;
            idx += ch.len_utf8();
            continue;
        }
        if ch == '[' {
            return Err(Error::diagram_parse_exact(
                "xychart".to_string(),
                "unbalanced '['".to_string(),
                SourceSpan::new(t_start + idx, t_start + idx + ch.len_utf8()),
            ));
        }
        if ch == ']' {
            let inner = &t[1..idx];
            let rest = &t[idx + 1..];
            if !rest.trim().is_empty() {
                let trailing_start = rest.len().saturating_sub(rest.trim_start().len());
                let trailing = rest.trim();
                let start = t_start + idx + 1 + trailing_start;
                return Err(Error::diagram_parse_exact(
                    "xychart".to_string(),
                    "unexpected trailing tokens after ']'".to_string(),
                    SourceSpan::new(start, start + trailing.len()),
                ));
            }
            return Ok((inner, t_start + 1));
        }
        idx += ch.len_utf8();
    }

    Err(Error::diagram_parse_insertion_point(
        "xychart".to_string(),
        "unbalanced ']'".to_string(),
        t_start + t.len(),
    ))
}

fn split_top_level_commas_spanned(input: &str, input_start: usize) -> Vec<SpannedSlice<'_>> {
    let mut out = Vec::new();
    let mut in_quote = false;
    let mut in_md = false;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        let rest = &input[i..];
        let ch = rest.chars().next().unwrap();
        if in_md {
            if rest.starts_with("`\"") {
                in_md = false;
                i += 2;
                continue;
            }
            i += ch.len_utf8();
            continue;
        }
        if in_quote {
            if ch == '"' {
                in_quote = false;
            }
            i += ch.len_utf8();
            continue;
        }
        if rest.starts_with("\"`") {
            in_md = true;
            i += 2;
            continue;
        }
        if ch == '"' {
            in_quote = true;
            i += ch.len_utf8();
            continue;
        }
        if ch == ',' {
            out.push(SpannedSlice::new(
                &input[start..i],
                input_start + start,
                input_start + i,
            ));
            i += ch.len_utf8();
            start = i;
            continue;
        }
        i += ch.len_utf8();
    }
    out.push(SpannedSlice::new(
        &input[start..],
        input_start + start,
        input_start + input.len(),
    ));
    out
}

#[derive(Debug, Clone)]
struct SpannedStatement {
    text: String,
    start: usize,
}

struct XyChartSyntaxFragments {
    statements: Vec<SpannedStatement>,
}

impl SpannedStatement {
    fn trimmed_start(&self) -> usize {
        self.start + self.text.len().saturating_sub(self.text.trim_start().len())
    }
}

fn split_statements_spanned_controlled(
    input: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<XyChartSyntaxFragments> {
    control.checkpoint()?;
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_start = 0usize;
    let mut in_quote = false;
    let mut in_md = false;
    let mut in_acc_descr_block = false;
    let mut iter = input.char_indices().peekable();
    let mut next_checkpoint = 0usize;

    while let Some((idx, ch)) = iter.next() {
        if idx >= next_checkpoint {
            control.checkpoint()?;
            next_checkpoint = idx.saturating_add(4096);
        }
        if cur.is_empty() {
            cur_start = idx;
        }

        if in_md {
            cur.push(ch);
            if ch == '`'
                && iter.peek().is_some_and(|(_, next)| *next == '"')
                && let Some((_quote_idx, quote)) = iter.next()
            {
                cur.push(quote);
                in_md = false;
            }
            continue;
        }
        if in_quote {
            cur.push(ch);
            if ch == '"' {
                in_quote = false;
            }
            continue;
        }

        if ch == '"' && iter.peek().is_some_and(|(_, next)| *next == '`') {
            cur.push(ch);
            if let Some((_tick_idx, tick)) = iter.next() {
                cur.push(tick);
                in_md = true;
            }
            continue;
        }

        if ch == '"' {
            cur.push(ch);
            in_quote = true;
            continue;
        }

        if !in_acc_descr_block
            && ch == '%'
            && iter.peek().is_some_and(|(_, next)| *next == '%')
            && !input[idx..].starts_with("%%{")
        {
            let mut next_statement_start = input.len();
            for (comment_idx, comment_ch) in iter.by_ref() {
                if comment_ch == '\n' {
                    next_statement_start = comment_idx + comment_ch.len_utf8();
                    break;
                }
            }
            if !cur.trim().is_empty() {
                out.push(SpannedStatement {
                    text: std::mem::take(&mut cur),
                    start: cur_start,
                });
            } else {
                cur.clear();
            }
            cur_start = next_statement_start;
            continue;
        }

        match ch {
            '{' if !in_acc_descr_block && is_xychart_accessibility_block_prefix(&cur) => {
                in_acc_descr_block = true;
            }
            '}' if in_acc_descr_block => {
                in_acc_descr_block = false;
                cur.push(ch);
                out.push(SpannedStatement {
                    text: std::mem::take(&mut cur),
                    start: cur_start,
                });
                cur_start = idx + ch.len_utf8();
                continue;
            }
            _ => {}
        }

        if (ch == '\n' || ch == ';') && !in_acc_descr_block {
            out.push(SpannedStatement {
                text: std::mem::take(&mut cur),
                start: cur_start,
            });
            cur_start = idx + ch.len_utf8();
            continue;
        }

        cur.push(ch);
    }

    if !in_acc_descr_block && !cur.is_empty() {
        out.push(SpannedStatement {
            text: cur,
            start: cur_start,
        });
    }
    control.checkpoint()?;
    Ok(XyChartSyntaxFragments { statements: out })
}

fn is_xychart_accessibility_block_prefix(statement: &str) -> bool {
    let statement = statement.trim_start();
    statement
        .get(.."accDescr".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("accDescr"))
        && statement["accDescr".len()..]
            .chars()
            .all(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, EditorSemanticDiagnosticKind, EditorSemanticRole, Engine,
        ParseDiagnosticSpanKind, ParseOptions,
    };
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
    fn xychart_header_only_is_accepted() {
        let model = parse("xychart");
        assert_eq!(model["plots"], json!([]));
    }

    #[test]
    fn xychart_empty_sources_share_the_configured_default_model_and_editor_facts() {
        let engine = Engine::new();
        let configured = concat!(
            "%%{init: {\"xyChart\": {\"chartOrientation\": \"horizontal\", ",
            "\"showTitle\": false, \"xAxis\": {\"showLabel\": false}}}}%%\nxychart",
        );
        let parsed = engine
            .parse_diagram_sync(configured, ParseOptions::strict())
            .unwrap()
            .unwrap();
        for source in ["", " \t\r\n", "%% no statements\n", ";\n"] {
            let typed = parse_xychart_model_for_render(source, &parsed.meta).unwrap();
            assert_eq!(typed.orientation, "horizontal");
            assert!(!typed.display.show_title);
            assert!(!typed.display.x_axis.show_label);
            assert!(typed.plots.is_empty());
            let projection = render_model_to_compat_json(&typed, &parsed.meta).unwrap();
            assert_eq!(projection, parsed.model);
            assert_eq!(parse_xychart(source, &parsed.meta).unwrap(), projection);
            let (combined, facts) =
                crate::family::test_support::into_result(parse_xychart_json_and_editor_facts(
                    source,
                    &parsed.meta,
                    &crate::OperationControl::new(),
                ))
                .unwrap();
            assert_eq!(combined, projection);
            assert!(facts.symbols.is_empty());
            assert!(facts.diagnostics.is_empty());
        }
    }

    #[test]
    fn xychart_invalid_header_throws() {
        let err = parse_err("xychart-1");
        assert!(err.contains("unexpected"));
    }

    #[test]
    fn xychart_orientation_is_parsed() {
        let model = parse("xychart horizontal");
        assert_eq!(model["orientation"], json!("horizontal"));
    }

    #[test]
    fn xychart_orientation_invalid_throws() {
        let err = parse_err("xychart abc");
        assert!(err.contains("invalid chart orientation"));
    }

    #[test]
    fn xychart_title_parses_quoted_and_unquoted() {
        let model = parse("xychart\ntitle \"This is a title\"");
        assert_eq!(model["title"], json!("This is a title"));

        let model = parse("xychart\ntitle oneLinertitle");
        assert_eq!(model["title"], json!("oneLinertitle"));
    }

    #[test]
    fn xychart_parses_axis_band_and_range_and_plots() {
        let model = parse(
            r#"xychart horizontal
title "Basic xychart"
x-axis "this is x axis" [category1, "category 2", category3]
y-axis yaxisText 10 --> 150
bar barTitle1 [23, 45, 56.6]
line lineTitle1 [11, 45.5, 67, 23]
"#,
        );
        assert_eq!(model["orientation"], json!("horizontal"));
        assert_eq!(model["xAxis"]["type"], json!("band"));
        assert_eq!(
            model["xAxis"]["categories"],
            json!(["category1", "category 2", "category3"])
        );
        assert_eq!(model["yAxis"]["min"], json!(10.0));
        assert_eq!(model["yAxis"]["max"], json!(150.0));
        assert_eq!(model["plots"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn xychart_line_points_accept_optional_labels() {
        let model = parse(
            r#"xychart
x-axis [A, B, C]
y-axis 0 --> 10
line [1 "low", 5, 9 "high"]
bar [2 "ignored", 4, 6]
"#,
        );

        assert_eq!(model["plots"][0]["type"], json!("line"));
        assert_eq!(model["plots"][0]["values"], json!([1.0, 5.0, 9.0]));
        assert_eq!(model["plots"][0]["pointLabels"], json!(["low", "", "high"]));
        assert!(model["plots"][1].get("pointLabels").is_none());
    }

    #[test]
    fn xychart_equal_linear_axis_bounds_map_every_point_by_data_index() {
        let model = parse(
            r#"xychart
x-axis 5 --> 5
line [10, 20, 30]
"#,
        );

        assert_eq!(
            model["plots"][0]["data"],
            json!([["5", 10.0], ["5", 20.0], ["5", 30.0]])
        );
    }

    #[test]
    fn xychart_single_point_on_linear_axis_remains_mapped_to_minimum() {
        let model = parse(
            r#"xychart
x-axis 5 --> 9
line [10]
"#,
        );

        assert_eq!(model["plots"][0]["data"], json!([["5", 10.0]]));
    }

    #[test]
    fn xychart_band_axis_truncates_points_but_preserves_labels_before_auto_y_range() {
        let model = parse(
            r#"xychart
x-axis [Q1, Q2]
line [10 "first", 50 "second", 999 "orphan", 800 "ignored"]
"#,
        );

        assert_eq!(model["yAxis"]["min"], json!(10.0));
        assert_eq!(model["yAxis"]["max"], json!(50.0));
        assert_eq!(model["plots"][0]["values"], json!([10.0, 50.0]));
        assert_eq!(
            model["plots"][0]["data"],
            json!([["Q1", 10.0], ["Q2", 50.0]])
        );
        assert_eq!(
            model["plots"][0]["pointLabels"],
            json!(["first", "second", "orphan", "ignored"])
        );
    }

    #[test]
    fn xychart_unquoted_multibyte_categories_do_not_panic() {
        let model = parse(
            r#"xychart
x-axis [東京, 大阪]
y-axis "値" 0 --> 10
bar [1, 2]
"#,
        );

        assert_eq!(model["xAxis"]["categories"], json!(["東京", "大阪"]));
    }

    #[test]
    fn xychart_plot_requires_nonempty_data() {
        let err = parse_err("xychart\nline \"t\" [ ]");
        assert!(err.contains("empty"));
        let err = parse_err("xychart\nline \"t\"");
        assert!(err.contains("missing") || err.contains("requires"));
    }

    #[test]
    fn xychart_accepts_line_without_whitespace_after_keyword() {
        let model = parse("xychart\nline[1,2,3]");
        assert_eq!(model["plots"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn xychart_comment_after_plot_does_not_merge_next_statement() {
        let model = parse("xychart\nbar [1] %% keep next line separate\nline [2]\n");
        assert_eq!(model["plots"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn xychart_acc_title_requires_colon() {
        let err = parse_err("xychart\naccTitle hello");
        assert!(err.contains("accTitle"));
    }

    #[test]
    fn xychart_rejects_invalid_x_axis_range_like_upstream() {
        let err = parse_err("xychart\nx-axis xAxisName aaa --> 33\n");
        assert!(err.contains("invalid"));
    }

    #[test]
    fn xychart_rejects_unbalanced_x_axis_brackets_like_upstream() {
        let err = parse_err("xychart\nx-axis xAxisName [ \"cat1\" [ cat2a ]\n");
        assert!(err.contains("unbalanced"));
        let err = parse_err("xychart\nx-axis xAxisName [ \"cat1\" , cat2a ] ]\n");
        assert!(err.contains("unexpected") || err.contains("unbalanced"));
    }

    #[test]
    fn xychart_rejects_invalid_y_axis_range_like_upstream() {
        let err = parse_err("xychart\ny-axis yAxisName 45.5 --> abc\n");
        assert!(err.contains("expected number") || err.contains("invalid"));
    }

    #[test]
    fn xychart_rejects_y_axis_band_data_like_upstream() {
        let err = parse_err("xychart\ny-axis yAxisName [ 45.3, 33 ]\n");
        assert!(err.contains("does not support") || err.contains("band"));
    }

    #[test]
    fn xychart_rejects_unbalanced_plot_brackets_like_upstream() {
        let err = parse_err("xychart\nline \"t\" [  +23 [ -45  , 56.6 ]\n");
        assert!(err.contains("unbalanced") || err.contains("expected"));
        let err = parse_err("xychart\nbar \"t\" [  +23 , -45  ] 56.6 ]\n");
        assert!(err.contains("unexpected") || err.contains("unbalanced"));
    }

    #[test]
    fn xychart_rejects_invalid_plot_commas_and_numbers_like_upstream() {
        let err = parse_err("xychart\nline \"t\" [  +23 ,  , -45  , 56.6 ]\n");
        assert!(err.contains("empty") || err.contains("invalid"));
        let err = parse_err("xychart\nbar \"t\" [  +23 , -4aa5  , 56.6 ]\n");
        assert!(err.contains("invalid number"));
    }

    #[test]
    fn xychart_invalid_plot_number_reports_exact_token_span() {
        let text = "xychart\nbar \"t\" [  +23 , -4aa5  , 56.6 ]\n";
        let engine = Engine::new();
        let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = err else {
            panic!("expected xychart parse error");
        };

        let token_start = text.find("-4aa5").unwrap();
        assert_eq!(diagnostic.message(), "invalid number: -4aa5");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(token_start, token_start + "-4aa5".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn xychart_rejects_trailing_decimal_point_like_pinned_jison() {
        let text = "xychart\nbar [1.]\n";
        let engine = Engine::new();
        let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = err else {
            panic!("expected xychart parse error");
        };
        let token_start = text.find("1.").expect("invalid number token");

        assert_eq!(diagnostic.message(), "invalid number: 1.");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(token_start, token_start + "1.".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn xychart_entrypoints_and_combined_projection_construct_once() {
        let engine = Engine::new();
        let text = concat!(
            "xychart horizontal\n",
            "title \"Revenue; Growth\"; accTitle: Revenue chart\n",
            "accDescr {\n",
            "  Quarterly %% literal\n",
            "  projection\n",
            "}\n",
            "x-axis \"Fiscal quarter\" [Q1, \"Q 2\"]\n",
            "y-axis Revenue 0 --> 100\n",
            "line \"Forecast line\" [23 \"Low\", 45 \"High\"]\n",
            "bar \"Actual bars\" [20, 40]\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("standalone XYChart JSON parse succeeds")
            .expect("standalone XYChart JSON parse returns a diagram");
        reset_xychart_syntax_construction_count();
        let standalone_json =
            parse_xychart(text, &parsed.meta).expect("XYChart JSON projection succeeds");
        assert_eq!(xychart_syntax_construction_count(), 1);

        reset_xychart_syntax_construction_count();
        let typed = parse_xychart_model_for_render(text, &parsed.meta)
            .expect("XYChart typed projection succeeds");
        assert_eq!(xychart_syntax_construction_count(), 1);

        reset_xychart_syntax_construction_count();
        let (combined_json, combined_editor) =
            crate::family::test_support::into_result(parse_xychart_json_and_editor_facts(
                text,
                &parsed.meta,
                &crate::OperationControl::new(),
            ))
            .expect("XYChart combined projection succeeds");
        assert_eq!(xychart_syntax_construction_count(), 1);
        for field in [
            "orientation",
            "title",
            "accTitle",
            "accDescr",
            "xAxis",
            "yAxis",
            "plots",
            "type",
            "config",
        ] {
            assert_eq!(
                combined_json[field], standalone_json[field],
                "XYChart combined {field} drift"
            );
        }
        assert!(!combined_editor.symbols.is_empty());

        let typed = serde_json::to_value(typed).expect("XYChart typed model serializes");
        for field in [
            "orientation",
            "title",
            "accTitle",
            "accDescr",
            "xAxis",
            "yAxis",
        ] {
            assert_eq!(typed[field], combined_json[field], "XYChart {field} drift");
        }
        let typed_plots = typed["plots"].as_array().expect("typed plots");
        let compat_plots = combined_json["plots"].as_array().expect("compat plots");
        assert_eq!(typed_plots.len(), compat_plots.len());
        for (typed, compat) in typed_plots.iter().zip(compat_plots) {
            for field in ["type", "title", "values", "data", "pointLabels"] {
                assert_eq!(
                    typed.get(field),
                    compat.get(field),
                    "XYChart plot {field} drift"
                );
            }
        }
    }

    #[test]
    fn xychart_typed_projection_matches_complete_compat_json() {
        let text = "xychart\nx-axis [Q1, Q2]\nline [1, 2]\n";
        let engine = Engine::new();
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("XYChart compatibility parse succeeds")
            .expect("XYChart compatibility parse returns a diagram");
        let typed = parse_xychart_model_for_render(text, &parsed.meta)
            .expect("XYChart typed parse succeeds");

        let projection = render_model_to_compat_json(&typed, &parsed.meta)
            .expect("XYChart compatibility projection succeeds");

        assert_eq!(projection, parsed.model);
        assert_eq!(projection["type"], json!("xychart"));
        assert!(projection["config"].is_object());
        assert_eq!(projection["title"], Value::Null);
        assert_eq!(projection["accTitle"], Value::Null);
        assert_eq!(projection["accDescr"], Value::Null);
        assert_eq!(projection["plots"][0]["title"], json!(""));
    }

    #[test]
    fn xychart_plot_titles_are_sanitized_and_projected_for_legends() {
        let text = "xychart\nx-axis [Q1, Q2]\nline \" avg \" [1, 2]\nbar \" p95 \" [2, 3]\n";
        let engine = Engine::new();
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("XYChart compatibility parse succeeds")
            .expect("XYChart compatibility parse returns a diagram");
        let typed = parse_xychart_model_for_render(text, &parsed.meta)
            .expect("XYChart typed parse succeeds");

        assert_eq!(typed.plots[0].title.as_deref(), Some("avg"));
        assert_eq!(typed.plots[1].title.as_deref(), Some("p95"));
        assert_eq!(parsed.model["plots"][0]["title"], json!("avg"));
        assert_eq!(parsed.model["plots"][1]["title"], json!("p95"));
    }

    #[test]
    fn xychart_editor_projection_preserves_multiline_and_nested_payload_spans() {
        let engine = Engine::new();
        let text = concat!(
            "xychart\n",
            "title \"Revenue plan\"\n",
            "accTitle: Revenue chart\n",
            "accDescr {\n",
            "  Quarterly %% literal\n",
            "  projection\n",
            "}\n",
            "x-axis \"Fiscal quarter\" [Q1, \"Q 2\"]\n",
            "y-axis Revenue 0 --> 100\n",
            "line \"Forecast line\" [23 \"Low\", 45 \"High\"]\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("XYChart parse succeeds")
            .expect("XYChart model");
        let facts = crate::family::test_support::editor_facts(
            parse_xychart_json_and_editor_facts,
            text,
            &parsed.meta,
        );

        let assert_payload = |name: &str, detail: &str, start: usize| {
            let symbol = facts
                .symbols
                .iter()
                .find(|symbol| {
                    symbol.name == name
                        && symbol.detail.as_deref() == Some(detail)
                        && symbol.selection.start == start
                })
                .unwrap_or_else(|| panic!("missing {detail} payload {name:?} at {start}"));
            assert_eq!(symbol.role, EditorSemanticRole::Payload);
            assert_eq!(symbol.selection, SourceSpan::new(start, start + name.len()));
            assert!(facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::Payload
                    && expected.span == symbol.selection
            }));
        };

        for (name, detail, start) in [
            (
                "Revenue plan",
                "xychart title",
                text.find("Revenue plan").expect("title source"),
            ),
            (
                "Revenue chart",
                "xychart accessibility title",
                text.find("Revenue chart")
                    .expect("accessibility title source"),
            ),
            (
                "Quarterly %% literal\n  projection",
                "xychart accessibility description",
                text.find("Quarterly %% literal")
                    .expect("accessibility description source"),
            ),
            (
                "Fiscal quarter",
                "xychart x-axis",
                text.find("Fiscal quarter").expect("x-axis title source"),
            ),
            (
                "Revenue",
                "xychart y-axis",
                text.find("y-axis Revenue").expect("y-axis source") + "y-axis ".len(),
            ),
            (
                "Forecast line",
                "xychart line",
                text.find("Forecast line").expect("plot title source"),
            ),
            (
                "Low",
                "xychart line data label",
                text.find("Low").expect("low label source"),
            ),
            (
                "High",
                "xychart line data label",
                text.find("High").expect("high label source"),
            ),
        ] {
            assert_payload(name, detail, start);
        }

        for structured in ["Q1, \"Q 2\"", "0 --> 100", "23 \"Low\", 45 \"High\""] {
            let start = text.find(structured).expect("structured payload source");
            assert!(facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::Payload
                    && expected.span == SourceSpan::new(start, start + structured.len())
            }));
            assert!(!facts.symbols.iter().any(|symbol| {
                symbol.name == structured && symbol.role == EditorSemanticRole::Payload
            }));
        }
    }

    #[test]
    fn xychart_malformed_plot_recovers_prior_parser_facts() {
        let engine = Engine::new();
        let text = concat!(
            "xychart\n",
            "title Revenue\n",
            "x-axis [Q1, Q2]\n",
            "bar [23, -4aa5]\n",
        );
        let invalid_start = text.find("-4aa5").expect("invalid number token");
        reset_xychart_syntax_construction_count();
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("xychart", text)
            .expect("XYChart editor recovery succeeds")
            .expect("XYChart editor facts are available");

        assert_eq!(xychart_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Revenue" && symbol.role == EditorSemanticRole::Payload
        }));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span
                    == Some(SourceSpan::new(
                        invalid_start,
                        invalid_start + "-4aa5".len(),
                    ))
        }));
    }

    #[test]
    fn xychart_recovery_keeps_confirmed_prefix_and_safe_later_statements() {
        let text = concat!(
            "xychart\n",
            "title Before\n",
            "bar [1, -4aa5]\n",
            "y-axis [0, 10]\n",
            "line \"After 后来\" [2 \"后来\"]\n",
        );
        let invalid_start = text.find("-4aa5").expect("invalid token");

        let strict = Engine::new()
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect_err("strict XYChart parse must return its first error");
        let Error::DiagramParse { diagnostic, .. } = strict else {
            panic!("expected diagram parse error");
        };
        assert_eq!(diagnostic.message(), "invalid number: -4aa5");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(
                invalid_start,
                invalid_start + "-4aa5".len()
            ))
        );

        reset_xychart_syntax_construction_count();
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("xychart", text)
            .expect("XYChart editor recovery")
            .expect("XYChart recovered facts");
        assert_eq!(xychart_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "After 后来"
                && symbol.detail.as_deref() == Some("xychart line")
                && symbol.role == EditorSemanticRole::Payload
        }));
    }

    #[test]
    fn xychart_unterminated_eof_string_reports_insertion_point() {
        let text = "xychart\ntitle \"未完成 🤓";
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("xychart", text)
            .expect("XYChart editor recovery")
            .expect("XYChart recovered facts");

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(text.len(), text.len()))
        }));
    }

    #[test]
    fn xychart_missing_bracket_recovers_at_the_next_statement_boundary() {
        let text = "xychart\nbar [1, 2\nline [3]\n";
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("xychart", text)
            .expect("XYChart editor recovery")
            .expect("XYChart recovered facts");

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        let later_value_start = text.rfind('3').expect("later line value");
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(later_value_start, later_value_start + 1)
        }));
    }
}

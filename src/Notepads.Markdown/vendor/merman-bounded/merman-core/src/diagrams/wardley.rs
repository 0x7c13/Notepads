use crate::common_db::LangiumCommonDbFields;
use crate::diagrams::langium_common::{
    LangiumCommonFacts, parse_langium_common, parse_langium_string,
    push_langium_common_editor_fact, strip_langium_inline_comment,
};
use crate::diagrams::scan::physical_line_at;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorRenamePolicy, EditorSemanticFacts,
    EditorSemanticKind, EditorSemanticSymbol, Error, OperationControl, OperationControlResult,
    ParseMetadata, Result, SourceSpan, family::CombinedSemanticFailure,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const HEADER: &str = "wardley-beta";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleySourceStrategy {
    Build,
    Buy,
    Outsource,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleyFlowDirection {
    Forward,
    Backward,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WardleyPointRenderModel {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyNodeRenderModel {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, rename = "className", skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(
        default,
        rename = "labelOffsetX",
        skip_serializing_if = "Option::is_none"
    )]
    pub label_offset_x: Option<i64>,
    #[serde(
        default,
        rename = "labelOffsetY",
        skip_serializing_if = "Option::is_none"
    )]
    pub label_offset_y: Option<i64>,
    #[serde(default, rename = "inPipeline", skip_serializing_if = "is_false")]
    pub in_pipeline: bool,
    #[serde(default, rename = "isPipelineParent", skip_serializing_if = "is_false")]
    pub is_pipeline_parent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inertia: Option<bool>,
    #[serde(
        default,
        rename = "sourceStrategy",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_strategy: Option<WardleySourceStrategy>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyLinkRenderModel {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub dashed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<WardleyFlowDirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyTrendRenderModel {
    #[serde(rename = "nodeId")]
    pub node_id: String,
    #[serde(rename = "targetX")]
    pub target_x: f64,
    #[serde(rename = "targetY")]
    pub target_y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardleyPipelineRenderModel {
    #[serde(rename = "nodeId")]
    pub node_id: String,
    #[serde(default, rename = "componentIds")]
    pub component_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyAnnotationRenderModel {
    pub number: u64,
    #[serde(default)]
    pub coordinates: Vec<WardleyPointRenderModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyNoteRenderModel {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyAcceleratorRenderModel {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardleyDeacceleratorRenderModel {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WardleyAxesRenderModel {
    #[serde(default, rename = "xLabel", skip_serializing_if = "Option::is_none")]
    pub x_label: Option<String>,
    #[serde(default, rename = "yLabel", skip_serializing_if = "Option::is_none")]
    pub y_label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<String>,
    #[serde(
        default,
        rename = "stageBoundaries",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub stage_boundaries: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WardleySizeRenderModel {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WardleyDiagramRenderModel {
    #[serde(default, rename = "accTitle", skip_serializing_if = "Option::is_none")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr", skip_serializing_if = "Option::is_none")]
    pub acc_descr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub nodes: Vec<WardleyNodeRenderModel>,
    #[serde(default)]
    pub links: Vec<WardleyLinkRenderModel>,
    #[serde(default)]
    pub trends: Vec<WardleyTrendRenderModel>,
    #[serde(default)]
    pub pipelines: Vec<WardleyPipelineRenderModel>,
    #[serde(default)]
    pub annotations: Vec<WardleyAnnotationRenderModel>,
    #[serde(default)]
    pub notes: Vec<WardleyNoteRenderModel>,
    #[serde(default)]
    pub accelerators: Vec<WardleyAcceleratorRenderModel>,
    #[serde(default)]
    pub deaccelerators: Vec<WardleyDeacceleratorRenderModel>,
    #[serde(
        default,
        rename = "annotationsBox",
        skip_serializing_if = "Option::is_none"
    )]
    pub annotations_box: Option<WardleyPointRenderModel>,
    #[serde(default)]
    pub axes: WardleyAxesRenderModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WardleySizeRenderModel>,
}

impl WardleyDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

pub(crate) fn parse_wardley(code: &str, meta: &ParseMetadata) -> Result<Value> {
    construct_wardley_semantic_source(code, meta)
        .map_err(CombinedSemanticFailure::into_error)?
        .into_compat_json(meta)
}

pub(crate) fn parse_wardley_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    control.checkpoint()?;
    let construction = match construct_wardley_semantic_source_controlled(code, meta, control)? {
        Ok(source) => Ok(source.into_combined_parts_controlled(meta, control)?),
        Err(error) => Err(error),
    };
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |parts| parts,
        CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_wardley_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<WardleyDiagramRenderModel> {
    Ok(construct_wardley_semantic_source(code, meta)
        .map_err(CombinedSemanticFailure::into_error)?
        .into_render_model(meta))
}

pub(crate) fn parse_wardley_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<WardleyDiagramRenderModel>> {
    let construction = construct_wardley_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(failure.into_error())),
    };
    Ok(Ok(source.into_render_model_controlled(meta, control)?))
}

pub(crate) fn render_model_to_compat_json(
    model: &WardleyDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let control = OperationControl::new();
    render_model_to_compat_json_controlled(model, meta, &control)
        .expect("a private parse control cannot be cancelled")
}

pub(crate) fn render_model_to_compat_json_controlled(
    model: &WardleyDiagramRenderModel,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<Value>> {
    control.checkpoint()?;
    let nodes = serialize_wardley_items_controlled(&model.nodes, control)?;
    let links = serialize_wardley_items_controlled(&model.links, control)?;
    let trends = serialize_wardley_items_controlled(&model.trends, control)?;
    let annotations = serialize_wardley_items_controlled(&model.annotations, control)?;
    let notes = serialize_wardley_items_controlled(&model.notes, control)?;
    let accelerators = serialize_wardley_items_controlled(&model.accelerators, control)?;
    let deaccelerators = serialize_wardley_items_controlled(&model.deaccelerators, control)?;

    let mut pipelines = Vec::with_capacity(model.pipelines.len());
    for (pipeline_index, pipeline) in model.pipelines.iter().enumerate() {
        if pipeline_index % 128 == 0 {
            control.checkpoint()?;
        }
        let mut component_ids = Vec::with_capacity(pipeline.component_ids.len());
        for (component_index, component_id) in pipeline.component_ids.iter().enumerate() {
            if component_index % 128 == 0 {
                control.checkpoint()?;
            }
            component_ids.push(Value::String(component_id.clone()));
        }
        pipelines.push(json!({
            "nodeId": pipeline.node_id,
            "componentIds": component_ids,
        }));
    }

    let mut axes = Map::new();
    if let Some(x_label) = &model.axes.x_label {
        axes.insert("xLabel".to_string(), Value::String(x_label.clone()));
    }
    if let Some(y_label) = &model.axes.y_label {
        axes.insert("yLabel".to_string(), Value::String(y_label.clone()));
    }
    if !model.axes.stages.is_empty() {
        axes.insert(
            "stages".to_string(),
            Value::Array(strings_to_json_controlled(&model.axes.stages, control)?),
        );
    }
    if !model.axes.stage_boundaries.is_empty() {
        let mut boundaries = Vec::with_capacity(model.axes.stage_boundaries.len());
        for (index, boundary) in model.axes.stage_boundaries.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            boundaries.push(json!(boundary));
        }
        axes.insert("stageBoundaries".to_string(), Value::Array(boundaries));
    }

    let mut out = Map::with_capacity(14);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("nodes".to_string(), Value::Array(nodes));
    out.insert("links".to_string(), Value::Array(links));
    out.insert("trends".to_string(), Value::Array(trends));
    out.insert("pipelines".to_string(), Value::Array(pipelines));
    out.insert("annotations".to_string(), Value::Array(annotations));
    out.insert("notes".to_string(), Value::Array(notes));
    out.insert("accelerators".to_string(), Value::Array(accelerators));
    out.insert("deaccelerators".to_string(), Value::Array(deaccelerators));
    out.insert("annotationsBox".to_string(), json!(&model.annotations_box));
    out.insert("axes".to_string(), Value::Object(axes));
    out.insert("size".to_string(), json!(&model.size));
    control.checkpoint()?;
    Ok(Ok(Value::Object(out)))
}

fn serialize_wardley_items_controlled<T: Serialize>(
    items: &[T],
    control: &OperationControl,
) -> OperationControlResult<Vec<Value>> {
    let mut values = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        values.push(json!(item));
    }
    Ok(values)
}

fn strings_to_json_controlled(
    strings: &[String],
    control: &OperationControl,
) -> OperationControlResult<Vec<Value>> {
    let mut values = Vec::with_capacity(strings.len());
    for (index, value) in strings.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        values.push(Value::String(value.clone()));
    }
    Ok(values)
}

#[derive(Debug)]
struct WardleySemanticSource {
    model: WardleyDiagramRenderModel,
    editor_facts: EditorSemanticFacts,
}

impl WardleySemanticSource {
    #[cfg(test)]
    fn into_render_model(mut self, meta: &ParseMetadata) -> WardleyDiagramRenderModel {
        self.model.sanitize_common_db_fields(&meta.effective_config);
        self.model
    }

    fn into_render_model_controlled(
        mut self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<WardleyDiagramRenderModel> {
        control.checkpoint()?;
        self.model.sanitize_common_db_fields(&meta.effective_config);
        control.checkpoint()?;
        Ok(self.model)
    }

    fn into_compat_json(self, meta: &ParseMetadata) -> Result<Value> {
        let control = OperationControl::new();
        self.into_compat_json_controlled(meta, &control)
            .expect("a private parse control cannot be cancelled")
    }

    fn into_compat_json_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Value>> {
        let model = self.into_render_model_controlled(meta, control)?;
        render_model_to_compat_json_controlled(&model, meta, control)
    }

    fn into_combined_parts_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<(Result<Value>, EditorSemanticFacts)> {
        let Self {
            mut model,
            editor_facts,
        } = self;
        control.checkpoint()?;
        model.sanitize_common_db_fields(&meta.effective_config);
        let json = render_model_to_compat_json_controlled(&model, meta, control)?;
        control.checkpoint()?;
        Ok((json, editor_facts))
    }
}

#[derive(Debug)]
struct WardleyParseOutcome {
    ast: WardleyAst,
    editor_facts: EditorSemanticFacts,
    first_problem: Option<WardleyParseProblem>,
}

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    span: SourceSpan,
    selection: SourceSpan,
    quoted: bool,
}

#[derive(Debug, Clone, Copy)]
struct SpannedNumber {
    value: f64,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct WardleySizeAst {
    width: SpannedNumber,
    height: SpannedNumber,
}

#[derive(Debug, Clone)]
struct WardleyEvolutionStageAst {
    name: SpannedText,
    boundary: Option<SpannedNumber>,
    second_name: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct WardleyEvolutionAst {
    stages: Vec<WardleyEvolutionStageAst>,
}

#[derive(Debug, Clone)]
struct WardleyPositionedNodeAst {
    name: SpannedText,
    visibility: SpannedNumber,
    evolution: SpannedNumber,
}

#[derive(Debug, Clone)]
struct WardleyLabelAst {
    offset_x: i64,
    offset_x_span: SourceSpan,
    offset_y: i64,
    offset_y_span: SourceSpan,
}

#[derive(Debug, Clone)]
struct WardleyComponentAst {
    positioned: WardleyPositionedNodeAst,
    label: Option<WardleyLabelAst>,
    source_strategy: Option<WardleySourceStrategy>,
    source_strategy_span: Option<SourceSpan>,
    inertia: bool,
    inertia_span: Option<SourceSpan>,
}

#[derive(Debug, Clone)]
struct WardleyPipelineComponentAst {
    name: SpannedText,
    evolution: SpannedNumber,
    label: Option<WardleyLabelAst>,
}

#[derive(Debug, Clone)]
struct WardleyPipelineAst {
    parent: SpannedText,
    components: Vec<WardleyPipelineComponentAst>,
}

#[derive(Debug, Clone)]
struct WardleyLinkAst {
    from: SpannedText,
    to: SpannedText,
    arrow: Option<String>,
    from_port: Option<String>,
    to_port: Option<String>,
    label: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct WardleyEvolveAst {
    component: SpannedText,
    target: SpannedNumber,
}

#[derive(Debug, Clone)]
struct WardleyNoteAst {
    text: SpannedText,
    visibility: SpannedNumber,
    evolution: SpannedNumber,
}

#[derive(Debug, Clone)]
struct WardleyAnnotationsBoxAst {
    x: SpannedNumber,
    y: SpannedNumber,
}

#[derive(Debug, Clone)]
struct WardleyAnnotationAst {
    number: u64,
    number_span: SourceSpan,
    x: SpannedNumber,
    y: SpannedNumber,
    text: SpannedText,
}

#[derive(Debug, Clone)]
struct WardleyForceAst {
    name: SpannedText,
    x: SpannedNumber,
    y: SpannedNumber,
}

#[derive(Debug, Default)]
struct WardleyAst {
    common: LangiumCommonFacts,
    size: Option<WardleySizeAst>,
    evolution: Option<WardleyEvolutionAst>,
    anchors: Vec<WardleyPositionedNodeAst>,
    components: Vec<WardleyComponentAst>,
    links: Vec<WardleyLinkAst>,
    evolves: Vec<WardleyEvolveAst>,
    pipelines: Vec<WardleyPipelineAst>,
    notes: Vec<WardleyNoteAst>,
    annotations_boxes: Vec<WardleyAnnotationsBoxAst>,
    annotations: Vec<WardleyAnnotationAst>,
    accelerators: Vec<WardleyForceAst>,
    deaccelerators: Vec<WardleyForceAst>,
}

#[derive(Debug, Clone)]
struct PendingWardleyNode {
    id: String,
    label: String,
    x: f64,
    y: f64,
    class_name: Option<String>,
    label_offset_x: Option<i64>,
    label_offset_y: Option<i64>,
    in_pipeline: bool,
    is_pipeline_parent: bool,
    inertia: Option<bool>,
    source_strategy: Option<WardleySourceStrategy>,
}

#[derive(Debug, Default)]
struct WardleyBuilder {
    nodes: IndexMap<String, PendingWardleyNode>,
    links: Vec<WardleyLinkRenderModel>,
    trends: IndexMap<String, WardleyTrendRenderModel>,
    pipelines: IndexMap<String, WardleyPipelineRenderModel>,
    annotations: Vec<WardleyAnnotationRenderModel>,
    notes: Vec<WardleyNoteRenderModel>,
    accelerators: Vec<WardleyAcceleratorRenderModel>,
    deaccelerators: Vec<WardleyDeacceleratorRenderModel>,
    annotations_box: Option<WardleyPointRenderModel>,
    axes: WardleyAxesRenderModel,
    size: Option<WardleySizeRenderModel>,
}

impl WardleyBuilder {
    fn add_node(&mut self, node: PendingWardleyNode) {
        if let Some(existing) = self.nodes.get_mut(&node.id) {
            existing.label = node.label;
            existing.x = node.x;
            existing.y = node.y;
            existing.class_name = node.class_name.or_else(|| existing.class_name.clone());
            existing.label_offset_x = node.label_offset_x.or(existing.label_offset_x);
            existing.label_offset_y = node.label_offset_y.or(existing.label_offset_y);
            existing.inertia = node.inertia;
            existing.source_strategy = node.source_strategy;
        } else {
            self.nodes.insert(node.id.clone(), node);
        }
    }

    fn start_pipeline(&mut self, node_id: &str) {
        self.pipelines.insert(
            node_id.to_string(),
            WardleyPipelineRenderModel {
                node_id: node_id.to_string(),
                component_ids: Vec::new(),
            },
        );
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.is_pipeline_parent = true;
        }
    }

    fn add_pipeline_component(&mut self, pipeline_node_id: &str, component_id: &str) {
        if let Some(pipeline) = self.pipelines.get_mut(pipeline_node_id) {
            pipeline.component_ids.push(component_id.to_string());
        }
        if let Some(node) = self.nodes.get_mut(component_id) {
            node.in_pipeline = true;
        }
    }

    fn resolve_node_id_controlled(
        &self,
        name: &str,
        control: &OperationControl,
    ) -> OperationControlResult<String> {
        if self.nodes.contains_key(name) {
            return Ok(name.to_string());
        }
        for (index, (id, node)) in self.nodes.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if node.label == name {
                return Ok(id.clone());
            }
        }
        Ok(name.to_string())
    }

    fn finish_controlled(
        self,
        common: LangiumCommonDbFields,
        control: &OperationControl,
    ) -> OperationControlResult<WardleyDiagramRenderModel> {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.into_values().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            nodes.push(WardleyNodeRenderModel {
                id: node.id,
                label: node.label,
                x: node.x,
                y: node.y,
                class_name: node.class_name,
                label_offset_x: node.label_offset_x,
                label_offset_y: node.label_offset_y,
                in_pipeline: node.in_pipeline,
                is_pipeline_parent: node.is_pipeline_parent,
                inertia: node.inertia,
                source_strategy: node.source_strategy,
            });
        }
        let mut trends = Vec::with_capacity(self.trends.len());
        for (index, trend) in self.trends.into_values().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            trends.push(trend);
        }
        let mut pipelines = Vec::with_capacity(self.pipelines.len());
        for (index, pipeline) in self.pipelines.into_values().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            pipelines.push(pipeline);
        }
        control.checkpoint()?;
        Ok(WardleyDiagramRenderModel {
            title: common.title,
            acc_title: common.acc_title,
            acc_descr: common.acc_descr,
            nodes,
            links: self.links,
            trends,
            pipelines,
            annotations: self.annotations,
            notes: self.notes,
            accelerators: self.accelerators,
            deaccelerators: self.deaccelerators,
            annotations_box: self.annotations_box,
            axes: self.axes,
            size: self.size,
        })
    }
}

#[derive(Debug)]
struct WardleyParseProblem {
    message: String,
    span: SourceSpan,
}

impl WardleyParseProblem {
    fn new(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

fn construct_wardley_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<WardleySemanticSource, CombinedSemanticFailure> {
    construct_wardley_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_wardley_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<WardleySemanticSource, CombinedSemanticFailure>> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("wardley");

    let WardleyParseOutcome {
        ast,
        editor_facts,
        first_problem,
    } = parse_wardley_ast(code, control)?;
    if let Some(problem) = first_problem {
        return Ok(Err(wardley_failure(meta, problem, editor_facts)));
    }
    control.checkpoint()?;
    let model = match build_wardley_model_controlled(&ast, control)? {
        Ok(model) => model,
        Err(problem) => {
            return Ok(Err(wardley_failure(meta, problem, editor_facts)));
        }
    };

    control.checkpoint()?;
    Ok(Ok(WardleySemanticSource {
        model,
        editor_facts,
    }))
}

fn wardley_failure(
    meta: &ParseMetadata,
    problem: WardleyParseProblem,
    editor_facts: EditorSemanticFacts,
) -> CombinedSemanticFailure {
    CombinedSemanticFailure::parser_recovery(
        "wardley",
        Error::diagram_parse_exact(meta.diagram_type.clone(), problem.message, problem.span),
        editor_facts,
    )
}

fn parse_wardley_ast(
    code: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<WardleyParseOutcome> {
    control.checkpoint()?;
    let mut ast = WardleyAst::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_problem = None;
    let mut offset = 0usize;
    let mut header_decided = false;
    let mut saw_header = false;

    while offset < code.len() {
        control.checkpoint()?;
        let line_start = offset;
        let (line, next_offset) = physical_line_at(code, offset);
        offset = next_offset;
        let visible = strip_langium_inline_comment(line);
        let trimmed = visible.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (statement, statement_start) = if !header_decided {
            header_decided = true;
            let leading = visible.len() - visible.trim_start().len();
            let header_start = line_start + leading;
            let Some(rest) = visible.trim_start().strip_prefix(HEADER) else {
                remember_wardley_problem(
                    &mut first_problem,
                    WardleyParseProblem::new(
                        "expected wardley-beta header",
                        trimmed_span(visible, line_start),
                    ),
                );
                continue;
            };
            if rest
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_ascii_whitespace())
            {
                remember_wardley_problem(
                    &mut first_problem,
                    WardleyParseProblem::new(
                        "expected whitespace or end of line after wardley-beta header",
                        SourceSpan::new(header_start, header_start + HEADER.len()),
                    ),
                );
                continue;
            }
            saw_header = true;
            editor_facts.push_directive_prefix(HEADER);
            let body_start = header_start + HEADER.len();
            (rest, body_start)
        } else {
            let leading = visible.len() - visible.trim_start().len();
            (&visible[leading..], line_start + leading)
        };

        if statement.trim().is_empty() {
            continue;
        }

        let leading = statement.len() - statement.trim_start().len();
        let statement_start = statement_start + leading;
        let statement = &statement[leading..];
        if let Some(parsed) = parse_langium_common(code, statement_start) {
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "wardley");
            if let Some(diagnostic) = parsed.diagnostic {
                remember_wardley_problem(
                    &mut first_problem,
                    WardleyParseProblem::new(diagnostic.message, diagnostic.span),
                );
            } else {
                ast.common.push(parsed.fact);
            }
            offset = statement_start + parsed.consumed;
            continue;
        }

        if keyword_body(statement, "pipeline").is_some() {
            editor_facts.push_directive_prefix("pipeline");
            let pipeline =
                parse_pipeline(code, statement, statement_start, offset, &mut editor_facts);
            offset = pipeline.next_offset;
            if let Some(problem) = pipeline.first_problem {
                remember_wardley_problem(&mut first_problem, problem);
            } else if let Some(pipeline) = pipeline.pipeline {
                ast.pipelines.push(pipeline);
            }
            continue;
        }

        if let Err(problem) =
            parse_wardley_statement(statement, statement_start, &mut ast, &mut editor_facts)
        {
            remember_wardley_problem(&mut first_problem, problem);
        }
    }

    if !header_decided || !saw_header {
        remember_wardley_problem(
            &mut first_problem,
            WardleyParseProblem::new("expected wardley-beta header", SourceSpan::new(0, 0)),
        );
    }

    control.checkpoint()?;
    Ok(WardleyParseOutcome {
        ast,
        editor_facts,
        first_problem,
    })
}

fn remember_wardley_problem(
    first_problem: &mut Option<WardleyParseProblem>,
    problem: WardleyParseProblem,
) {
    if first_problem.is_none() {
        *first_problem = Some(problem);
    }
}

fn parse_wardley_statement(
    statement: &str,
    statement_start: usize,
    ast: &mut WardleyAst,
    editor_facts: &mut EditorSemanticFacts,
) -> std::result::Result<(), WardleyParseProblem> {
    if let Some(body) = keyword_body(statement, "size") {
        editor_facts.push_directive_prefix("size");
        let size = parse_size(body, body_start(statement, statement_start, body))?;
        push_number_fact(editor_facts, size.width, "wardley canvas width");
        push_number_fact(editor_facts, size.height, "wardley canvas height");
        ast.size = Some(size);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "evolution") {
        editor_facts.push_directive_prefix("evolution");
        let evolution = parse_evolution(body, body_start(statement, statement_start, body))?;
        for stage in &evolution.stages {
            push_payload_fact(editor_facts, &stage.name, "wardley evolution stage");
            if let Some(boundary) = stage.boundary {
                push_number_fact(editor_facts, boundary, "wardley evolution boundary");
            }
            if let Some(second_name) = &stage.second_name {
                push_payload_fact(
                    editor_facts,
                    second_name,
                    "wardley evolution stage secondary label",
                );
            }
        }
        ast.evolution = Some(evolution);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "anchor") {
        editor_facts.push_directive_prefix("anchor");
        let anchor = parse_positioned_node(body, body_start(statement, statement_start, body))?;
        push_entity_fact(editor_facts, &anchor.name, "wardley anchor");
        push_number_fact(editor_facts, anchor.visibility, "wardley anchor visibility");
        push_number_fact(editor_facts, anchor.evolution, "wardley anchor evolution");
        ast.anchors.push(anchor);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "component") {
        editor_facts.push_directive_prefix("component");
        let component = parse_component(body, body_start(statement, statement_start, body))?;
        push_component_facts(editor_facts, &component, "wardley component");
        ast.components.push(component);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "evolve") {
        editor_facts.push_directive_prefix("evolve");
        let evolve = parse_evolve(body, body_start(statement, statement_start, body))?;
        push_reference_fact(editor_facts, &evolve.component, "wardley evolved component");
        push_number_fact(editor_facts, evolve.target, "wardley evolution target");
        ast.evolves.push(evolve);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "note") {
        editor_facts.push_directive_prefix("note");
        let note = parse_note(body, body_start(statement, statement_start, body))?;
        push_payload_fact(editor_facts, &note.text, "wardley note");
        push_number_fact(editor_facts, note.visibility, "wardley note visibility");
        push_number_fact(editor_facts, note.evolution, "wardley note evolution");
        ast.notes.push(note);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "annotations") {
        editor_facts.push_directive_prefix("annotations");
        let annotations =
            parse_annotations_box(body, body_start(statement, statement_start, body))?;
        push_number_fact(
            editor_facts,
            annotations.x,
            "wardley annotations visibility",
        );
        push_number_fact(editor_facts, annotations.y, "wardley annotations evolution");
        ast.annotations_boxes.push(annotations);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "annotation") {
        editor_facts.push_directive_prefix("annotation");
        let annotation = parse_annotation(body, body_start(statement, statement_start, body))?;
        push_integer_fact(
            editor_facts,
            annotation.number,
            annotation.number_span,
            "wardley annotation number",
        );
        push_number_fact(editor_facts, annotation.x, "wardley annotation visibility");
        push_number_fact(editor_facts, annotation.y, "wardley annotation evolution");
        push_payload_fact(editor_facts, &annotation.text, "wardley annotation text");
        ast.annotations.push(annotation);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "accelerator") {
        editor_facts.push_directive_prefix("accelerator");
        let force = parse_force(body, body_start(statement, statement_start, body))?;
        push_outline_fact(editor_facts, &force.name, "wardley accelerator");
        push_number_fact(editor_facts, force.x, "wardley accelerator visibility");
        push_number_fact(editor_facts, force.y, "wardley accelerator evolution");
        ast.accelerators.push(force);
        return Ok(());
    }
    if let Some(body) = keyword_body(statement, "deaccelerator") {
        editor_facts.push_directive_prefix("deaccelerator");
        let force = parse_force(body, body_start(statement, statement_start, body))?;
        push_outline_fact(editor_facts, &force.name, "wardley deaccelerator");
        push_number_fact(editor_facts, force.x, "wardley deaccelerator visibility");
        push_number_fact(editor_facts, force.y, "wardley deaccelerator evolution");
        ast.deaccelerators.push(force);
        return Ok(());
    }

    let link = parse_link(statement, statement_start)?;
    push_reference_fact(editor_facts, &link.from, "wardley link source");
    push_reference_fact(editor_facts, &link.to, "wardley link target");
    if let Some(label) = &link.label {
        push_payload_fact(editor_facts, label, "wardley link label");
    }
    ast.links.push(link);
    Ok(())
}

struct WardleyPipelineOutcome {
    pipeline: Option<WardleyPipelineAst>,
    next_offset: usize,
    first_problem: Option<WardleyParseProblem>,
}

fn parse_pipeline(
    code: &str,
    statement: &str,
    statement_start: usize,
    offset: usize,
    editor_facts: &mut EditorSemanticFacts,
) -> WardleyPipelineOutcome {
    let mut offset = offset;
    let mut first_problem = None;
    let body = keyword_body(statement, "pipeline").expect("pipeline dispatch checked keyword");
    let pipeline_body_start = body_start(statement, statement_start, body);
    let trimmed = body.trim();
    let Some(open_brace) = find_unquoted_char(trimmed, '{') else {
        return WardleyPipelineOutcome {
            pipeline: None,
            next_offset: offset,
            first_problem: Some(WardleyParseProblem::new(
                "expected '{' after wardley pipeline parent",
                trimmed_span(body, pipeline_body_start),
            )),
        };
    };
    let leading = body.len() - body.trim_start().len();
    let opening_start = pipeline_body_start + leading + open_brace;
    if !trimmed[open_brace + 1..].trim().is_empty() {
        remember_wardley_problem(
            &mut first_problem,
            WardleyParseProblem::new(
                "unexpected tokens after wardley pipeline opening brace",
                SourceSpan::new(opening_start + 1, pipeline_body_start + body.len()),
            ),
        );
    }
    let parent_raw = &trimmed[..open_brace];
    let parent = match parse_name(parent_raw, pipeline_body_start + leading) {
        Ok(parent) => {
            push_reference_fact(editor_facts, &parent, "wardley pipeline parent");
            Some(parent)
        }
        Err(problem) => {
            remember_wardley_problem(&mut first_problem, problem);
            None
        }
    };
    let mut components = Vec::new();

    loop {
        if offset >= code.len() {
            remember_wardley_problem(
                &mut first_problem,
                WardleyParseProblem::new(
                    "unterminated wardley pipeline block",
                    SourceSpan::new(code.len(), code.len()),
                ),
            );
            return WardleyPipelineOutcome {
                pipeline: None,
                next_offset: offset,
                first_problem,
            };
        }
        let line_start = offset;
        let (line, next_offset) = physical_line_at(code, offset);
        offset = next_offset;
        let visible = strip_langium_inline_comment(line);
        let trimmed_line = visible.trim();
        if trimmed_line.is_empty() {
            continue;
        }
        let leading = visible.len() - visible.trim_start().len();
        let absolute = line_start + leading;
        if let Some(after_closing) = trimmed_line.strip_prefix('}') {
            if !after_closing.is_empty() {
                let trailing_start = absolute + 1 + after_closing.len()
                    - after_closing.trim_start_matches([' ', '\t']).len();
                remember_wardley_problem(
                    &mut first_problem,
                    WardleyParseProblem::new(
                        "unexpected tokens after wardley pipeline closing brace",
                        SourceSpan::new(trailing_start, absolute + trimmed_line.len()),
                    ),
                );
            }
            if components.is_empty() {
                remember_wardley_problem(
                    &mut first_problem,
                    WardleyParseProblem::new(
                        "wardley pipeline requires at least one component",
                        SourceSpan::new(absolute, absolute + 1),
                    ),
                );
            }
            let pipeline = if first_problem.is_none() {
                parent.map(|parent| parent_with_components(parent, components))
            } else {
                None
            };
            return WardleyPipelineOutcome {
                pipeline,
                next_offset: offset,
                first_problem,
            };
        }
        let Some(component_body) = keyword_body(trimmed_line, "component") else {
            let span = SourceSpan::new(absolute, absolute + trimmed_line.len());
            remember_wardley_problem(
                &mut first_problem,
                WardleyParseProblem::new("expected component or '}' inside wardley pipeline", span),
            );
            continue;
        };
        editor_facts.push_directive_prefix("component");
        let component_start = body_start(trimmed_line, absolute, component_body);
        match parse_pipeline_component(component_body, component_start) {
            Ok(component) => {
                push_pipeline_component_facts(editor_facts, &component);
                components.push(component);
            }
            Err(problem) => remember_wardley_problem(&mut first_problem, problem),
        }
    }
}

fn parent_with_components(
    parent: SpannedText,
    components: Vec<WardleyPipelineComponentAst>,
) -> WardleyPipelineAst {
    WardleyPipelineAst { parent, components }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WardleyNumberKind {
    Decimal,
    IntegerOrDecimal,
    Integer,
}

struct WardleyCursor<'input> {
    input: &'input str,
    base: usize,
    pos: usize,
}

impl<'input> WardleyCursor<'input> {
    fn new(input: &'input str, base: usize) -> Self {
        Self {
            input,
            base,
            pos: 0,
        }
    }

    fn remaining(&self) -> &'input str {
        &self.input[self.pos..]
    }

    fn absolute(&self) -> usize {
        self.base + self.pos
    }

    fn skip_ws(&mut self) {
        self.pos += self
            .remaining()
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
    }

    fn consume_char(
        &mut self,
        expected: char,
        context: &str,
    ) -> std::result::Result<(), WardleyParseProblem> {
        self.skip_ws();
        let start = self.absolute();
        if self.remaining().starts_with(expected) {
            self.pos += expected.len_utf8();
            Ok(())
        } else {
            Err(WardleyParseProblem::new(
                format!("expected '{expected}' {context}"),
                SourceSpan::new(start, start),
            ))
        }
    }

    fn take_number(
        &mut self,
        kind: WardleyNumberKind,
        context: &str,
    ) -> std::result::Result<SpannedNumber, WardleyParseProblem> {
        self.skip_ws();
        let token_start = self.absolute();
        let bytes = self.remaining().as_bytes();
        let integer_len = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if integer_len == 0 {
            return Err(WardleyParseProblem::new(
                format!("expected {context}"),
                SourceSpan::new(token_start, token_start),
            ));
        }

        let mut consumed = integer_len;
        let has_dot = bytes.get(consumed) == Some(&b'.');
        if has_dot {
            consumed += 1;
            let fractional = bytes[consumed..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if fractional == 0 {
                return Err(WardleyParseProblem::new(
                    format!("expected digits after decimal point in {context}"),
                    SourceSpan::new(token_start, token_start + consumed),
                ));
            }
            consumed += fractional;
        }

        if kind == WardleyNumberKind::Decimal && !has_dot {
            return Err(WardleyParseProblem::new(
                format!("expected decimal {context}"),
                SourceSpan::new(token_start, token_start + consumed),
            ));
        }
        if kind == WardleyNumberKind::Integer && has_dot {
            return Err(WardleyParseProblem::new(
                format!("expected integer {context}"),
                SourceSpan::new(token_start, token_start + consumed),
            ));
        }
        let token = &self.remaining()[..consumed];
        let token_span = SourceSpan::new(token_start, token_start + consumed);
        if !has_dot && token.len() > 1 && token.starts_with('0') {
            return Err(WardleyParseProblem::new(
                format!("invalid leading zero in {context}"),
                SourceSpan::new(token_start, token_start + consumed),
            ));
        }
        if self.remaining()[consumed..]
            .chars()
            .next()
            .is_some_and(|ch| ch == '.' || ch.is_ascii_digit())
        {
            return Err(WardleyParseProblem::new(
                format!("invalid {context}"),
                SourceSpan::new(token_start, token_start + consumed + 1),
            ));
        }

        let value = token.parse::<f64>().map_err(|_| {
            WardleyParseProblem::new(
                format!("invalid {context}"),
                SourceSpan::new(token_start, token_start + consumed),
            )
        })?;
        self.pos += consumed;
        Ok(SpannedNumber {
            value,
            span: token_span,
        })
    }

    fn take_signed_integer(
        &mut self,
        context: &str,
    ) -> std::result::Result<(i64, SourceSpan), WardleyParseProblem> {
        self.skip_ws();
        let start = self.absolute();
        let negative = self.remaining().starts_with('-');
        if negative {
            self.pos += 1;
        }
        let number = self.take_number(WardleyNumberKind::Integer, context)?;
        let magnitude = number.value as i64;
        Ok((
            if negative { -magnitude } else { magnitude },
            SourceSpan::new(start, number.span.end),
        ))
    }

    fn expect_end(&mut self, context: &str) -> std::result::Result<(), WardleyParseProblem> {
        self.skip_ws();
        if self.remaining().is_empty() {
            Ok(())
        } else {
            let span = SourceSpan::new(self.absolute(), self.base + self.input.len());
            Err(WardleyParseProblem::new(
                format!("unexpected trailing tokens in {context}"),
                span,
            ))
        }
    }
}

fn parse_size(body: &str, base: usize) -> std::result::Result<WardleySizeAst, WardleyParseProblem> {
    let mut cursor = WardleyCursor::new(body, base);
    cursor.consume_char('[', "after wardley size")?;
    let width = cursor.take_number(WardleyNumberKind::Integer, "wardley canvas width")?;
    cursor.consume_char(',', "between wardley canvas dimensions")?;
    let height = cursor.take_number(WardleyNumberKind::Integer, "wardley canvas height")?;
    cursor.consume_char(']', "after wardley canvas dimensions")?;
    cursor.expect_end("wardley size")?;
    Ok(WardleySizeAst { width, height })
}

fn parse_evolution(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyEvolutionAst, WardleyParseProblem> {
    let ranges = split_unquoted_token(body, "->", base)?;
    if ranges.len() < 2 {
        return Err(WardleyParseProblem::new(
            "wardley evolution requires at least two stages",
            trimmed_span(body, base),
        ));
    }
    let mut stages = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        stages.push(parse_evolution_stage(&body[start..end], base + start)?);
    }
    Ok(WardleyEvolutionAst { stages })
}

fn parse_evolution_stage(
    input: &str,
    base: usize,
) -> std::result::Result<WardleyEvolutionStageAst, WardleyParseProblem> {
    let (trimmed, trimmed_start) = trim_horizontal(input, base);
    if trimmed.is_empty() {
        return Err(WardleyParseProblem::new(
            "expected wardley evolution stage",
            SourceSpan::new(trimmed_start, trimmed_start),
        ));
    }
    let slash = find_unquoted_char(trimmed, '/');
    let primary_end = slash.unwrap_or(trimmed.len());
    let primary = &trimmed[..primary_end];
    let boundary_at = find_unquoted_char(primary, '@');
    let name_end = boundary_at.unwrap_or(primary.len());
    let name = parse_name(&primary[..name_end], trimmed_start)?;
    let boundary = if let Some(at) = boundary_at {
        let mut cursor = WardleyCursor::new(&primary[at + 1..], trimmed_start + at + 1);
        let number = cursor.take_number(
            WardleyNumberKind::Decimal,
            "wardley evolution stage boundary",
        )?;
        cursor.expect_end("wardley evolution stage boundary")?;
        Some(number)
    } else {
        None
    };
    let second_name = if let Some(slash) = slash {
        Some(parse_name(
            &trimmed[slash + 1..],
            trimmed_start + slash + 1,
        )?)
    } else {
        None
    };
    Ok(WardleyEvolutionStageAst {
        name,
        boundary,
        second_name,
    })
}

fn parse_positioned_node(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyPositionedNodeAst, WardleyParseProblem> {
    let bracket = find_unquoted_char(body, '[').ok_or_else(|| {
        WardleyParseProblem::new(
            "expected coordinates after wardley node name",
            trimmed_span(body, base),
        )
    })?;
    let name = parse_name(&body[..bracket], base)?;
    let mut cursor = WardleyCursor::new(&body[bracket..], base + bracket);
    let (visibility, evolution) = take_position_pair(
        &mut cursor,
        WardleyNumberKind::Decimal,
        "wardley node coordinates",
    )?;
    cursor.expect_end("wardley node")?;
    Ok(WardleyPositionedNodeAst {
        name,
        visibility,
        evolution,
    })
}

fn parse_component(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyComponentAst, WardleyParseProblem> {
    let bracket = find_unquoted_char(body, '[').ok_or_else(|| {
        WardleyParseProblem::new(
            "expected coordinates after wardley component name",
            trimmed_span(body, base),
        )
    })?;
    let name = parse_name(&body[..bracket], base)?;
    let mut cursor = WardleyCursor::new(&body[bracket..], base + bracket);
    let (visibility, evolution) = take_position_pair(
        &mut cursor,
        WardleyNumberKind::Decimal,
        "wardley component coordinates",
    )?;
    let label = parse_optional_label(&mut cursor)?;
    let source_strategy = parse_optional_strategy(&mut cursor)?;
    let inertia_span = parse_optional_inertia(&mut cursor)?;
    cursor.expect_end("wardley component")?;
    Ok(WardleyComponentAst {
        positioned: WardleyPositionedNodeAst {
            name,
            visibility,
            evolution,
        },
        label,
        source_strategy: source_strategy.map(|(strategy, _)| strategy),
        source_strategy_span: source_strategy.map(|(_, span)| span),
        inertia: inertia_span.is_some(),
        inertia_span,
    })
}

fn parse_pipeline_component(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyPipelineComponentAst, WardleyParseProblem> {
    let bracket = find_unquoted_char(body, '[').ok_or_else(|| {
        WardleyParseProblem::new(
            "expected evolution coordinate after wardley pipeline component name",
            trimmed_span(body, base),
        )
    })?;
    let name = parse_name(&body[..bracket], base)?;
    let mut cursor = WardleyCursor::new(&body[bracket..], base + bracket);
    cursor.consume_char('[', "before wardley pipeline component evolution")?;
    let evolution = cursor.take_number(
        WardleyNumberKind::Decimal,
        "wardley pipeline component evolution",
    )?;
    cursor.consume_char(']', "after wardley pipeline component evolution")?;
    let label = parse_optional_label(&mut cursor)?;
    cursor.expect_end("wardley pipeline component")?;
    Ok(WardleyPipelineComponentAst {
        name,
        evolution,
        label,
    })
}

fn take_position_pair(
    cursor: &mut WardleyCursor<'_>,
    kind: WardleyNumberKind,
    context: &str,
) -> std::result::Result<(SpannedNumber, SpannedNumber), WardleyParseProblem> {
    cursor.consume_char('[', &format!("before {context}"))?;
    let first = cursor.take_number(kind, context)?;
    cursor.consume_char(',', &format!("between {context}"))?;
    let second = cursor.take_number(kind, context)?;
    cursor.consume_char(']', &format!("after {context}"))?;
    Ok((first, second))
}

fn parse_optional_label(
    cursor: &mut WardleyCursor<'_>,
) -> std::result::Result<Option<WardleyLabelAst>, WardleyParseProblem> {
    cursor.skip_ws();
    let Some(rest) = keyword_body(cursor.remaining(), "label") else {
        return Ok(None);
    };
    cursor.pos += cursor.remaining().len() - rest.len();
    cursor.consume_char('[', "after wardley label")?;
    let (offset_x, offset_x_span) = cursor.take_signed_integer("wardley label X offset")?;
    cursor.consume_char(',', "between wardley label offsets")?;
    let (offset_y, offset_y_span) = cursor.take_signed_integer("wardley label Y offset")?;
    cursor.consume_char(']', "after wardley label offsets")?;
    Ok(Some(WardleyLabelAst {
        offset_x,
        offset_x_span,
        offset_y,
        offset_y_span,
    }))
}

fn parse_optional_strategy(
    cursor: &mut WardleyCursor<'_>,
) -> std::result::Result<Option<(WardleySourceStrategy, SourceSpan)>, WardleyParseProblem> {
    cursor.skip_ws();
    if !cursor.remaining().starts_with('(') {
        return Ok(None);
    }
    let start = cursor.absolute();
    let Some(close) = cursor.remaining().find(')') else {
        return Err(WardleyParseProblem::new(
            "unterminated wardley component decorator",
            SourceSpan::new(start, cursor.base + cursor.input.len()),
        ));
    };
    let token = cursor.remaining()[1..close].trim();
    let token_leading =
        cursor.remaining()[1..close].len() - cursor.remaining()[1..close].trim_start().len();
    let token_span = SourceSpan::new(
        start + 1 + token_leading,
        start + 1 + token_leading + token.len(),
    );
    if token == "inertia" {
        return Ok(None);
    }
    let strategy = match token {
        "build" => WardleySourceStrategy::Build,
        "buy" => WardleySourceStrategy::Buy,
        "outsource" => WardleySourceStrategy::Outsource,
        "market" => WardleySourceStrategy::Market,
        _ => {
            return Err(WardleyParseProblem::new(
                "expected build, buy, outsource, or market wardley source strategy",
                SourceSpan::new(start + 1, start + close),
            ));
        }
    };
    cursor.pos += close + 1;
    Ok(Some((strategy, token_span)))
}

fn parse_optional_inertia(
    cursor: &mut WardleyCursor<'_>,
) -> std::result::Result<Option<SourceSpan>, WardleyParseProblem> {
    cursor.skip_ws();
    let start = cursor.absolute();
    if let Some(rest) = keyword_body(cursor.remaining(), "inertia") {
        cursor.pos += cursor.remaining().len() - rest.len();
        let span = SourceSpan::new(start, start + "inertia".len());
        return Ok(Some(span));
    }
    if !cursor.remaining().starts_with('(') {
        return Ok(None);
    }

    cursor.pos += 1;
    cursor.skip_ws();
    let keyword_start = cursor.absolute();
    let Some(after_keyword) = cursor.remaining().strip_prefix("inertia") else {
        return Err(WardleyParseProblem::new(
            "expected inertia inside wardley component inertia annotation",
            SourceSpan::new(keyword_start, keyword_start),
        ));
    };
    if after_keyword
        .chars()
        .next()
        .is_some_and(|ch| !matches!(ch, ' ' | '\t' | ')'))
    {
        return Err(WardleyParseProblem::new(
            "expected inertia inside wardley component inertia annotation",
            SourceSpan::new(keyword_start, keyword_start + "inertia".len()),
        ));
    }
    cursor.pos += "inertia".len();
    let keyword = SourceSpan::new(keyword_start, keyword_start + "inertia".len());
    cursor.skip_ws();
    let close = cursor.absolute();
    if !cursor.remaining().starts_with(')') {
        return Err(WardleyParseProblem::new(
            "expected ')' after wardley component inertia annotation",
            SourceSpan::new(close, close),
        ));
    }
    cursor.pos += 1;
    Ok(Some(keyword))
}

fn parse_evolve(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyEvolveAst, WardleyParseProblem> {
    let (trimmed, trimmed_start) = trim_horizontal(body, base);
    let target_start = trimmed
        .rfind([' ', '\t'])
        .map(|index| {
            index
                + trimmed[index..]
                    .bytes()
                    .take_while(|byte| matches!(byte, b' ' | b'\t'))
                    .count()
        })
        .ok_or_else(|| {
            WardleyParseProblem::new(
                "expected component name and target after wardley evolve",
                trimmed_span(body, base),
            )
        })?;
    let component = parse_name(&trimmed[..target_start], trimmed_start)?;
    let mut cursor = WardleyCursor::new(&trimmed[target_start..], trimmed_start + target_start);
    let target = cursor.take_number(
        WardleyNumberKind::Decimal,
        "wardley component evolution target",
    )?;
    cursor.expect_end("wardley evolve")?;
    Ok(WardleyEvolveAst { component, target })
}

fn parse_note(body: &str, base: usize) -> std::result::Result<WardleyNoteAst, WardleyParseProblem> {
    let (trimmed, trimmed_start) = trim_horizontal(body, base);
    let (text, consumed) =
        parse_wardley_quoted_text(trimmed, trimmed_start, "expected quoted wardley note text")?;
    let mut cursor = WardleyCursor::new(&trimmed[consumed..], trimmed_start + consumed);
    let (visibility, evolution) = take_position_pair(
        &mut cursor,
        WardleyNumberKind::Decimal,
        "wardley note coordinates",
    )?;
    cursor.expect_end("wardley note")?;
    Ok(WardleyNoteAst {
        text,
        visibility,
        evolution,
    })
}

fn parse_annotations_box(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyAnnotationsBoxAst, WardleyParseProblem> {
    let mut cursor = WardleyCursor::new(body, base);
    let (x, y) = take_position_pair(
        &mut cursor,
        WardleyNumberKind::IntegerOrDecimal,
        "wardley annotations box coordinates",
    )?;
    cursor.expect_end("wardley annotations box")?;
    Ok(WardleyAnnotationsBoxAst { x, y })
}

fn parse_annotation(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyAnnotationAst, WardleyParseProblem> {
    let mut cursor = WardleyCursor::new(body, base);
    let number = cursor.take_number(WardleyNumberKind::Integer, "wardley annotation number")?;
    cursor.consume_char(',', "after wardley annotation number")?;
    let (x, y) = take_position_pair(
        &mut cursor,
        WardleyNumberKind::IntegerOrDecimal,
        "wardley annotation coordinates",
    )?;
    cursor.skip_ws();
    let text_start = cursor.absolute();
    let (text, consumed) = parse_wardley_quoted_text(
        cursor.remaining(),
        text_start,
        "expected quoted wardley annotation text",
    )?;
    cursor.pos += consumed;
    cursor.expect_end("wardley annotation")?;
    Ok(WardleyAnnotationAst {
        number: number.value as u64,
        number_span: number.span,
        x,
        y,
        text,
    })
}

fn parse_force(
    body: &str,
    base: usize,
) -> std::result::Result<WardleyForceAst, WardleyParseProblem> {
    let positioned = parse_positioned_node(body, base)?;
    Ok(WardleyForceAst {
        name: positioned.name,
        x: positioned.visibility,
        y: positioned.evolution,
    })
}

fn parse_link(
    statement: &str,
    statement_start: usize,
) -> std::result::Result<WardleyLinkAst, WardleyParseProblem> {
    let (link_body, label) = split_link_label(statement, statement_start)?;
    let Some((operator_start, first_operator)) = find_link_operator(link_body) else {
        let (from, to, to_port) = parse_operatorless_link(link_body, statement_start)?;
        return Ok(WardleyLinkAst {
            from,
            to,
            arrow: None,
            from_port: None,
            to_port,
            label,
        });
    };
    let from = parse_name(&link_body[..operator_start], statement_start)?;
    let mut cursor = operator_start + first_operator.len();
    while link_body[cursor..].starts_with([' ', '\t']) {
        cursor += 1;
    }

    let mut from_port = None;
    let mut arrow = None;
    if is_link_port(first_operator) {
        from_port = Some(first_operator.to_string());
        if let Some(second) = link_operator_at(link_body, cursor)
            && !is_link_port(second)
        {
            arrow = Some(second.to_string());
            cursor += second.len();
        }
    } else {
        arrow = Some(first_operator.to_string());
    }

    let target_raw = &link_body[cursor..];
    let (target_without_port, to_port) = split_trailing_link_port(target_raw);
    let target_base = statement_start + cursor;
    let to = parse_name(target_without_port, target_base)?;

    Ok(WardleyLinkAst {
        from,
        to,
        arrow,
        from_port,
        to_port: to_port.map(|(port, _)| port.to_string()),
        label,
    })
}

fn split_link_label(
    statement: &str,
    statement_start: usize,
) -> std::result::Result<(&str, Option<SpannedText>), WardleyParseProblem> {
    let Some(index) = find_unquoted_char(statement, ';') else {
        return Ok((statement, None));
    };
    let raw = &statement[index + 1..];
    if raw.is_empty() {
        return Err(WardleyParseProblem::new(
            "expected wardley link label after ';'",
            SourceSpan::new(statement_start + index, statement_start + index + 1),
        ));
    }
    let (trimmed, start) = trim_horizontal(raw, statement_start + index + 1);
    let label = SpannedText {
        text: trimmed.to_string(),
        span: SourceSpan::new(statement_start + index, statement_start + statement.len()),
        selection: SourceSpan::new(start, start + trimmed.len()),
        quoted: false,
    };
    Ok((&statement[..index], Some(label)))
}

fn find_link_operator(input: &str) -> Option<(usize, &str)> {
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..].chars().next()?;
        if escaped {
            escaped = false;
            index += ch.len_utf8();
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            index += ch.len_utf8();
            continue;
        }
        if matches!(ch, '\'' | '"') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                _ => {}
            }
            index += ch.len_utf8();
            continue;
        }
        if quote.is_none()
            && let Some(operator) = link_operator_at(input, index)
        {
            if is_link_port(operator) && input[index + operator.len()..].trim().is_empty() {
                index += operator.len();
                continue;
            }
            return Some((index, operator));
        }
        index += ch.len_utf8();
    }
    None
}

fn link_operator_at(input: &str, index: usize) -> Option<&str> {
    let rest = input.get(index..)?;
    if let Some(label_body) = rest.strip_prefix("+'")
        && let Some(close) = label_body.find('\'')
    {
        let suffix_start = 2 + close + 1;
        let suffix = &rest[suffix_start..];
        let suffix_len = if suffix.starts_with("<>") {
            2
        } else if suffix.starts_with(['<', '>']) {
            1
        } else {
            0
        };
        if suffix_len > 0 {
            return Some(&rest[..suffix_start + suffix_len]);
        }
    }
    ["+<>", "-.->", "-->", "->", "+>", "+<", ">"]
        .into_iter()
        .find(|operator| rest.starts_with(operator))
}

fn is_link_port(operator: &str) -> bool {
    matches!(operator, "+<>" | "+>" | "+<")
}

fn split_trailing_link_port(input: &str) -> (&str, Option<(&str, usize)>) {
    let trimmed_end = input.trim_end_matches([' ', '\t']);
    for port in ["+<>", "+>", "+<"] {
        if let Some(prefix) = trimmed_end.strip_suffix(port)
            && !prefix.trim().is_empty()
        {
            return (prefix, Some((port, prefix.len())));
        }
    }
    (input, None)
}

fn parse_operatorless_link(
    input: &str,
    base: usize,
) -> std::result::Result<(SpannedText, SpannedText, Option<String>), WardleyParseProblem> {
    let (trimmed, start) = trim_horizontal(input, base);
    let Some(first_len) = wardley_lexical_name_len(trimmed) else {
        let span = trimmed_span(input, base);
        return Err(WardleyParseProblem::new(
            "expected wardley statement or link operator",
            span,
        ));
    };
    let from = parse_name(&trimmed[..first_len], start)?;
    if first_len == trimmed.len() {
        return Err(WardleyParseProblem::new(
            "expected a second wardley link endpoint",
            SourceSpan::new(start + first_len, start + first_len),
        ));
    }
    let remainder = &trimmed[first_len..];
    let remainder_leading = remainder.len() - remainder.trim_start_matches([' ', '\t']).len();
    let target_start = start + first_len + remainder_leading;
    let target_raw = &remainder[remainder_leading..];
    let (target, to_port) = split_trailing_link_port(target_raw);
    let to = parse_name(target, target_start)?;
    Ok((from, to, to_port.map(|(port, _)| port.to_string())))
}

fn wardley_lexical_name_len(input: &str) -> Option<usize> {
    let first = input.chars().next()?;
    if matches!(first, '\'' | '"') {
        return parse_langium_string(input, 0).map(|parsed| parsed.consumed);
    }
    if first.is_ascii_alphabetic() {
        let mut cursor = first.len_utf8();
        cursor += input[cursor..]
            .bytes()
            .take_while(|byte| is_name_continuation(*byte as char))
            .count();
        loop {
            let whitespace = input[cursor..]
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            if whitespace == 0 {
                break;
            }
            let word_start = cursor + whitespace;
            let Some(next) = input[word_start..].chars().next() else {
                break;
            };
            if !next.is_ascii_alphabetic() && next != '(' {
                break;
            }
            cursor = word_start + next.len_utf8();
            cursor += input[cursor..]
                .bytes()
                .take_while(|byte| is_name_continuation(*byte as char))
                .count();
        }
        return Some(cursor);
    }
    if first.is_ascii_digit() || first == '_' {
        let mut len = input
            .bytes()
            .take_while(|byte| is_id_continuation(*byte as char))
            .count();
        while input[..len].ends_with('-') {
            len -= 1;
        }
        return (len > 0).then_some(len);
    }
    None
}

fn parse_name(input: &str, base: usize) -> std::result::Result<SpannedText, WardleyParseProblem> {
    let (trimmed, start) = trim_horizontal(input, base);
    if trimmed.is_empty() {
        return Err(WardleyParseProblem::new(
            "expected wardley name",
            SourceSpan::new(start, start),
        ));
    }
    if matches!(trimmed.chars().next(), Some('\'' | '"')) {
        let Some(parsed) = parse_langium_string(trimmed, start) else {
            return Err(WardleyParseProblem::new(
                "unterminated quoted wardley name",
                SourceSpan::new(start, start + trimmed.len()),
            ));
        };
        let text = SpannedText {
            text: parsed.value,
            span: parsed.raw_span,
            selection: parsed.value_span,
            quoted: true,
        };
        if parsed.consumed != trimmed.len() {
            return Err(WardleyParseProblem::new(
                "unexpected tokens after quoted wardley name",
                SourceSpan::new(start + parsed.consumed, start + trimmed.len()),
            ));
        }
        return Ok(text);
    }
    if !is_valid_wardley_bare_name(trimmed) {
        return Err(WardleyParseProblem::new(
            format!("invalid unquoted wardley name: {trimmed}"),
            SourceSpan::new(start, start + trimmed.len()),
        ));
    }
    let text = SpannedText {
        text: trimmed.to_string(),
        span: SourceSpan::new(start, start + trimmed.len()),
        selection: SourceSpan::new(start, start + trimmed.len()),
        quoted: false,
    };
    Ok(text)
}

fn parse_wardley_quoted_text(
    input: &str,
    start: usize,
    context: &str,
) -> std::result::Result<(SpannedText, usize), WardleyParseProblem> {
    let Some(_) = input
        .chars()
        .next()
        .filter(|quote| matches!(quote, '\'' | '"'))
    else {
        return Err(WardleyParseProblem::new(
            context,
            SourceSpan::new(start, start + input.len()),
        ));
    };
    let Some(parsed) = parse_langium_string(input, start) else {
        return Err(WardleyParseProblem::new(
            context,
            SourceSpan::new(start, start + input.len()),
        ));
    };
    let consumed = parsed.consumed;
    let text = SpannedText {
        text: parsed.value,
        span: parsed.raw_span,
        selection: parsed.value_span,
        quoted: true,
    };
    Ok((text, consumed))
}

fn is_valid_wardley_bare_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "wardley-beta",
        "size",
        "evolution",
        "anchor",
        "component",
        "label",
        "inertia",
        "evolve",
        "pipeline",
        "note",
        "annotations",
        "annotation",
        "accelerator",
        "deaccelerator",
        "build",
        "buy",
        "outsource",
        "market",
        "title",
        "accTitle",
        "accDescr",
    ];
    if RESERVED.contains(&name) {
        return false;
    }
    if name.contains([' ', '\t'])
        && ["title", "accTitle", "accDescr"].into_iter().any(|prefix| {
            name.strip_prefix(prefix)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|ch| matches!(ch, ' ' | '\t'))
        })
    {
        return false;
    }
    if !name.contains([' ', '\t']) {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        let id = (first.is_ascii_alphanumeric() || first == '_')
            && chars.clone().all(is_id_continuation)
            && !name.ends_with('-');
        let spaced_name_atom = first.is_ascii_alphabetic() && chars.all(is_name_continuation);
        return id || spaced_name_atom;
    }

    let mut words = name.split([' ', '\t']).filter(|word| !word.is_empty());
    let Some(first_word) = words.next() else {
        return false;
    };
    if first_word.is_empty()
        || !first_word
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        || !first_word.chars().all(is_name_continuation)
    {
        return false;
    }
    words.all(|word| {
        word.chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '(')
            && word.chars().all(is_name_continuation)
    })
}

fn is_id_continuation(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn is_name_continuation(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '(' | ')' | '&' | '-')
}

fn split_unquoted_token(
    input: &str,
    token: &str,
    base: usize,
) -> std::result::Result<Vec<(usize, usize)>, WardleyParseProblem> {
    let mut ranges = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut segment_start = 0usize;
    let mut index = 0usize;
    while index < input.len() {
        let ch = input[index..]
            .chars()
            .next()
            .expect("index stays on char boundary");
        if escaped {
            escaped = false;
            index += ch.len_utf8();
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            index += ch.len_utf8();
            continue;
        }
        if matches!(ch, '\'' | '"') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                _ => {}
            }
            index += ch.len_utf8();
            continue;
        }
        if quote.is_none() && input[index..].starts_with(token) {
            ranges.push((segment_start, index));
            index += token.len();
            segment_start = index;
            continue;
        }
        index += ch.len_utf8();
    }
    if quote.is_some() {
        return Err(WardleyParseProblem::new(
            "unterminated quoted wardley value",
            SourceSpan::new(base, base + input.len()),
        ));
    }
    ranges.push((segment_start, input.len()));
    Ok(ranges)
}

fn find_unquoted_char(input: &str, needle: char) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            continue;
        }
        if matches!(ch, '\'' | '"') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                _ => {}
            }
            continue;
        }
        if quote.is_none() && ch == needle {
            return Some(index);
        }
    }
    None
}

fn keyword_body<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(keyword)?;
    (rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '[' | '{')))
    .then_some(rest)
}

fn body_start(statement: &str, statement_start: usize, body: &str) -> usize {
    statement_start + statement.len() - body.len()
}

fn trim_horizontal(input: &str, base: usize) -> (&str, usize) {
    let leading = input.len() - input.trim_start_matches([' ', '\t']).len();
    (input.trim_matches([' ', '\t']), base + leading)
}

fn trimmed_span(input: &str, base: usize) -> SourceSpan {
    let (trimmed, start) = trim_horizontal(input, base);
    SourceSpan::new(start, start + trimmed.len())
}

fn build_wardley_model_controlled(
    ast: &WardleyAst,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<WardleyDiagramRenderModel, WardleyParseProblem>> {
    macro_rules! wardley_try {
        ($expression:expr) => {
            match $expression {
                Ok(value) => value,
                Err(problem) => return Ok(Err(problem)),
            }
        };
    }

    control.checkpoint()?;
    let mut builder = WardleyBuilder::default();
    if let Some(size) = &ast.size {
        builder.size = Some(WardleySizeRenderModel {
            width: size.width.value,
            height: size.height.value,
        });
    }
    if let Some(evolution) = &ast.evolution {
        builder.axes.stages.reserve(evolution.stages.len());
        builder
            .axes
            .stage_boundaries
            .reserve(evolution.stages.len());
        for (index, stage) in evolution.stages.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            builder.axes.stages.push(match &stage.second_name {
                Some(second) => format!("{} / {}", stage.name.text.trim(), second.text.trim()),
                None => stage.name.text.trim().to_string(),
            });
            if let Some(boundary) = stage.boundary {
                builder.axes.stage_boundaries.push(boundary.value);
            }
        }
    }

    for (index, anchor) in ast.anchors.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            anchor.visibility,
            anchor.evolution,
            &format!("Anchor \"{}\"", anchor.name.text),
        ));
        builder.add_node(PendingWardleyNode {
            id: anchor.name.text.clone(),
            label: anchor.name.text.clone(),
            x: point.x,
            y: point.y,
            class_name: Some("anchor".to_string()),
            label_offset_x: None,
            label_offset_y: None,
            in_pipeline: false,
            is_pipeline_parent: false,
            inertia: None,
            source_strategy: None,
        });
    }

    for (index, component) in ast.components.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            component.positioned.visibility,
            component.positioned.evolution,
            &format!("Component \"{}\"", component.positioned.name.text),
        ));
        builder.add_node(PendingWardleyNode {
            id: component.positioned.name.text.clone(),
            label: component.positioned.name.text.clone(),
            x: point.x,
            y: point.y,
            class_name: Some("component".to_string()),
            label_offset_x: component.label.as_ref().map(|label| label.offset_x),
            label_offset_y: component.label.as_ref().map(|label| label.offset_y),
            in_pipeline: false,
            is_pipeline_parent: false,
            inertia: Some(component.inertia),
            source_strategy: component.source_strategy,
        });
    }

    for (index, note) in ast.notes.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            note.visibility,
            note.evolution,
            &format!("Note \"{}\"", note.text.text),
        ));
        builder.notes.push(WardleyNoteRenderModel {
            text: note.text.text.clone(),
            x: point.x,
            y: point.y,
        });
    }

    for (pipeline_index, pipeline) in ast.pipelines.iter().enumerate() {
        if pipeline_index % 128 == 0 {
            control.checkpoint()?;
        }
        let Some(parent_y) = builder.nodes.get(&pipeline.parent.text).map(|node| node.y) else {
            return Ok(Err(WardleyParseProblem::new(
                format!(
                    "Pipeline \"{}\" must reference an existing component with coordinates.",
                    pipeline.parent.text
                ),
                pipeline.parent.span,
            )));
        };
        builder.start_pipeline(&pipeline.parent.text);
        for (component_index, component) in pipeline.components.iter().enumerate() {
            if component_index % 128 == 0 {
                control.checkpoint()?;
            }
            let component_id = format!("{}_{}", pipeline.parent.text, component.name.text);
            let x = wardley_try!(to_percent(
                component.evolution,
                &format!("Pipeline component \"{}\" evolution", component.name.text),
            ));
            builder.add_node(PendingWardleyNode {
                id: component_id.clone(),
                label: component.name.text.clone(),
                x,
                y: parent_y,
                class_name: Some("pipeline-component".to_string()),
                label_offset_x: component.label.as_ref().map(|label| label.offset_x),
                label_offset_y: component.label.as_ref().map(|label| label.offset_y),
                in_pipeline: false,
                is_pipeline_parent: false,
                inertia: None,
                source_strategy: None,
            });
            builder.add_pipeline_component(&pipeline.parent.text, &component_id);
        }
    }

    for (index, link) in ast.links.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let dashed = link
            .arrow
            .as_deref()
            .is_some_and(|arrow| arrow.contains("-.->") || arrow.contains(".-."));
        let mut flow = link
            .from_port
            .as_deref()
            .and_then(flow_from_port)
            .or_else(|| link.to_port.as_deref().and_then(flow_from_port));
        let (arrow_flow, flow_label) = link
            .arrow
            .as_deref()
            .map(flow_from_arrow)
            .unwrap_or_default();
        if flow.is_none() {
            flow = arrow_flow;
        }
        let label = flow_label.or_else(|| link.label.as_ref().map(|label| label.text.clone()));
        builder.links.push(WardleyLinkRenderModel {
            source: builder.resolve_node_id_controlled(&link.from.text, control)?,
            target: builder.resolve_node_id_controlled(&link.to.text, control)?,
            dashed,
            label,
            flow,
        });
    }

    for (index, evolve) in ast.evolves.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let Some(node_y) = builder.nodes.get(&evolve.component.text).map(|node| node.y) else {
            continue;
        };
        let target_x = wardley_try!(to_percent(
            evolve.target,
            &format!("Evolve target for \"{}\"", evolve.component.text),
        ));
        builder.trends.insert(
            evolve.component.text.clone(),
            WardleyTrendRenderModel {
                node_id: evolve.component.text.clone(),
                target_x,
                target_y: node_y,
            },
        );
    }

    if let Some(annotations_box) = ast.annotations_boxes.first() {
        builder.annotations_box = Some(wardley_try!(to_coordinates(
            annotations_box.x,
            annotations_box.y,
            "Annotations box",
        )));
    }
    for (index, annotation) in ast.annotations.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            annotation.x,
            annotation.y,
            &format!("Annotation {}", annotation.number),
        ));
        builder.annotations.push(WardleyAnnotationRenderModel {
            number: annotation.number,
            coordinates: vec![point],
            text: Some(annotation.text.text.clone()),
        });
    }
    for (index, accelerator) in ast.accelerators.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            accelerator.x,
            accelerator.y,
            &format!("Accelerator \"{}\"", accelerator.name.text),
        ));
        builder.accelerators.push(WardleyAcceleratorRenderModel {
            name: accelerator.name.text.clone(),
            x: point.x,
            y: point.y,
        });
    }
    for (index, deaccelerator) in ast.deaccelerators.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let point = wardley_try!(to_coordinates(
            deaccelerator.x,
            deaccelerator.y,
            &format!("Deaccelerator \"{}\"", deaccelerator.name.text),
        ));
        builder
            .deaccelerators
            .push(WardleyDeacceleratorRenderModel {
                name: deaccelerator.name.text.clone(),
                x: point.x,
                y: point.y,
            });
    }

    let common = LangiumCommonDbFields::from_facts(&ast.common);
    Ok(Ok(builder.finish_controlled(common, control)?))
}

fn to_coordinates(
    visibility: SpannedNumber,
    evolution: SpannedNumber,
    context: &str,
) -> std::result::Result<WardleyPointRenderModel, WardleyParseProblem> {
    Ok(WardleyPointRenderModel {
        x: to_percent(evolution, &format!("{context} evolution"))?,
        y: to_percent(visibility, &format!("{context} visibility"))?,
    })
}

fn to_percent(
    value: SpannedNumber,
    context: &str,
) -> std::result::Result<f64, WardleyParseProblem> {
    let normalized = if value.value <= 1.0 {
        value.value * 100.0
    } else {
        value.value
    };
    if !(0.0..=100.0).contains(&normalized) {
        return Err(WardleyParseProblem::new(
            format!(
                "{context} must be between 0-1 (decimal) or 0-100 (percentage). Received: {}",
                value.value
            ),
            value.span,
        ));
    }
    Ok(normalized)
}

fn flow_from_port(port: &str) -> Option<WardleyFlowDirection> {
    match port {
        "+<>" => Some(WardleyFlowDirection::Bidirectional),
        "+<" => Some(WardleyFlowDirection::Backward),
        "+>" => Some(WardleyFlowDirection::Forward),
        _ => None,
    }
}

fn flow_from_arrow(arrow: &str) -> (Option<WardleyFlowDirection>, Option<String>) {
    if !arrow.starts_with('+') {
        return (None, None);
    }
    let label = arrow
        .strip_prefix("+'")
        .and_then(|rest| rest.find('\'').map(|end| rest[..end].to_string()));
    let flow = if arrow.contains("<>") {
        Some(WardleyFlowDirection::Bidirectional)
    } else if arrow.contains('<') {
        Some(WardleyFlowDirection::Backward)
    } else if arrow.contains('>') {
        Some(WardleyFlowDirection::Forward)
    } else {
        None
    };
    (flow, label)
}

fn push_entity_fact(facts: &mut EditorSemanticFacts, value: &SpannedText, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        value.selection,
    ));
    facts.push_symbol(
        EditorSemanticSymbol::new(
            value.text.clone(),
            Some(detail.to_string()),
            EditorSemanticKind::Object,
            value.span,
            value.selection,
        )
        .with_rename_policy(wardley_rename_policy(value)),
    );
}

fn push_reference_fact(facts: &mut EditorSemanticFacts, value: &SpannedText, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        value.selection,
    ));
    facts.push_symbol(
        EditorSemanticSymbol::reference(
            value.text.clone(),
            Some(detail.to_string()),
            EditorSemanticKind::Object,
            value.span,
            value.selection,
        )
        .with_rename_policy(wardley_rename_policy(value)),
    );
}

fn wardley_rename_policy(value: &SpannedText) -> EditorRenamePolicy {
    if !value.quoted
        && value
            .text
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-'))
    {
        EditorRenamePolicy::Identifier
    } else {
        EditorRenamePolicy::None
    }
}

fn push_outline_fact(facts: &mut EditorSemanticFacts, value: &SpannedText, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        value.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::outline(
        value.text.clone(),
        Some(detail.to_string()),
        EditorSemanticKind::Object,
        value.span,
        value.selection,
    ));
}

fn push_payload_fact(facts: &mut EditorSemanticFacts, value: &SpannedText, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        value.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.text.clone(),
        Some(detail.to_string()),
        EditorSemanticKind::String,
        value.span,
        value.selection,
    ));
}

fn push_number_fact(facts: &mut EditorSemanticFacts, value: SpannedNumber, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        value.span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.value.to_string(),
        Some(detail.to_string()),
        EditorSemanticKind::Property,
        value.span,
        value.span,
    ));
}

fn push_integer_fact(facts: &mut EditorSemanticFacts, value: u64, span: SourceSpan, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.to_string(),
        Some(detail.to_string()),
        EditorSemanticKind::Property,
        span,
        span,
    ));
}

fn push_signed_integer_fact(
    facts: &mut EditorSemanticFacts,
    value: i64,
    span: SourceSpan,
    detail: &str,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.to_string(),
        Some(detail.to_string()),
        EditorSemanticKind::Property,
        span,
        span,
    ));
}

fn push_component_facts(
    facts: &mut EditorSemanticFacts,
    component: &WardleyComponentAst,
    detail: &str,
) {
    push_entity_fact(facts, &component.positioned.name, detail);
    push_number_fact(
        facts,
        component.positioned.visibility,
        "wardley component visibility",
    );
    push_number_fact(
        facts,
        component.positioned.evolution,
        "wardley component evolution",
    );
    if let Some(label) = &component.label {
        facts.push_directive_prefix("label");
        push_signed_integer_fact(
            facts,
            label.offset_x,
            label.offset_x_span,
            "wardley label X offset",
        );
        push_signed_integer_fact(
            facts,
            label.offset_y,
            label.offset_y_span,
            "wardley label Y offset",
        );
    }
    if let (Some(strategy), Some(span)) =
        (component.source_strategy, component.source_strategy_span)
    {
        push_payload_text(
            facts,
            match strategy {
                WardleySourceStrategy::Build => "build",
                WardleySourceStrategy::Buy => "buy",
                WardleySourceStrategy::Outsource => "outsource",
                WardleySourceStrategy::Market => "market",
            },
            span,
            "wardley source strategy",
        );
    }
    if let Some(span) = component.inertia_span {
        facts.push_directive_prefix("inertia");
        push_payload_text(facts, "inertia", span, "wardley inertia");
    }
}

fn push_pipeline_component_facts(
    facts: &mut EditorSemanticFacts,
    component: &WardleyPipelineComponentAst,
) {
    push_entity_fact(facts, &component.name, "wardley pipeline component");
    push_number_fact(
        facts,
        component.evolution,
        "wardley pipeline component evolution",
    );
    if let Some(label) = &component.label {
        facts.push_directive_prefix("label");
        push_signed_integer_fact(
            facts,
            label.offset_x,
            label.offset_x_span,
            "wardley label X offset",
        );
        push_signed_integer_fact(
            facts,
            label.offset_y,
            label.offset_y_span,
            "wardley label Y offset",
        );
    }
}

fn push_payload_text(facts: &mut EditorSemanticFacts, text: &str, span: SourceSpan, detail: &str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        text.to_string(),
        Some(detail.to_string()),
        EditorSemanticKind::String,
        span,
        span,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, EditorSemanticDiagnosticKind, Engine, MermaidConfig,
        ParseOptions, RenderSemanticModel,
    };

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "wardley".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    fn parse(source: &str) -> WardleyDiagramRenderModel {
        parse_wardley_model_for_render(source, &meta()).unwrap()
    }

    #[test]
    fn wardley_model_construction_observes_cancellation_inside_large_ast_collections() {
        let span = SourceSpan::new(0, 1);
        let ast = WardleyAst {
            components: (0..512)
                .map(|index| WardleyComponentAst {
                    positioned: WardleyPositionedNodeAst {
                        name: SpannedText {
                            text: format!("component-{index}"),
                            span,
                            selection: span,
                            quoted: false,
                        },
                        visibility: SpannedNumber { value: 0.5, span },
                        evolution: SpannedNumber { value: 0.5, span },
                    },
                    label: None,
                    source_strategy: None,
                    source_strategy_span: None,
                    inertia: false,
                    inertia_span: None,
                })
                .collect(),
            ..WardleyAst::default()
        };
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(build_wardley_model_controlled(&ast, &control).is_err());
    }

    #[test]
    fn wardley_json_projection_observes_cancellation_inside_large_model_collections() {
        let model = WardleyDiagramRenderModel {
            nodes: (0..512)
                .map(|index| WardleyNodeRenderModel {
                    id: format!("node-{index}"),
                    label: format!("Node {index}"),
                    x: 50.0,
                    y: 50.0,
                    class_name: None,
                    label_offset_x: None,
                    label_offset_y: None,
                    in_pipeline: false,
                    is_pipeline_parent: false,
                    inertia: None,
                    source_strategy: None,
                })
                .collect(),
            ..WardleyDiagramRenderModel::default()
        };
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(render_model_to_compat_json_controlled(&model, &meta(), &control).is_err());
    }

    #[test]
    fn parses_official_basic_map_with_upstream_coordinate_and_build_order() {
        let model = parse(
            r#"wardley-beta
title Tea Shop Value Chain

anchor Business [0.95, 0.63]
component Cup of Tea [0.79, 0.61]
component Tea [0.63, 0.81]
component Hot Water [0.52, 0.80]
component Kettle [0.43, 0.35]
component Power [0.10, 0.70]

Business -> Cup of Tea
Cup of Tea -> Tea
Cup of Tea -> Hot Water
Hot Water -> Kettle
Kettle -> Power

evolve Kettle 0.62
evolve Power 0.89

note "Standardising power allows Kettles to evolve faster" [0.30, 0.49]
"#,
        );

        assert_eq!(model.title.as_deref(), Some("Tea Shop Value Chain"));
        assert_eq!(
            model
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [
                "Business",
                "Cup of Tea",
                "Tea",
                "Hot Water",
                "Kettle",
                "Power"
            ]
        );
        assert_eq!(model.nodes[0].class_name.as_deref(), Some("anchor"));
        assert_eq!((model.nodes[0].x, model.nodes[0].y), (63.0, 95.0));
        assert_eq!(model.links.len(), 5);
        assert_eq!(model.trends[0].node_id, "Kettle");
        assert_eq!(model.trends[0].target_x, 62.0);
        assert_eq!(model.notes[0].x, 49.0);
        assert_eq!(model.notes[0].y, 30.0);
    }

    #[test]
    fn parses_feature_rich_wardley_1116_semantics_without_renderer_reconstruction() {
        let model = parse(
            r#"wardley-beta
title Software Platform Strategy
accTitle: Platform map
accDescr: Strategic platform evolution
size [1100, 800]
evolution Genesis@0.25 / Concept -> Custom@0.5 / Emerging -> Product@0.75 / Converging -> Commodity@1.0 / Accepted

anchor Customer [0.90, 0.95]
component "Mobile App" [0.80, 0.85] (build)
component "Web App" [0.75, 0.80] label [-60, 10] (build)
component "API Gateway" [0.70, 0.65] (buy)
component Database [0.50, 0.45] (buy) (inertia)
component Cache [0.55, 0.50]

pipeline Database {
  component "File System" [0.25] label [-40, 20]
  component SQL DB [0.50]
}

Customer -> "Mobile App"; entry
"Mobile App" +> "API Gateway"
"API Gateway" +<> Cache
Cache +'backup'> Database
"API Gateway" -.-> Database
Database -> "File System"
evolve "API Gateway" 0.85
note "Build mobile-first experience" [0.85, 0.90]
annotations [0.10, 0.20]
annotation 1,[0.78, 0.82] "User touchpoints"
accelerator "Cloud Native" [0.20, 0.85]
deaccelerator "Legacy Data" [0.45, 0.35]
"#,
        );

        assert_eq!(model.acc_title.as_deref(), Some("Platform map"));
        assert_eq!(
            model.size,
            Some(WardleySizeRenderModel {
                width: 1100.0,
                height: 800.0
            })
        );
        assert_eq!(
            model.axes.stages,
            [
                "Genesis / Concept",
                "Custom / Emerging",
                "Product / Converging",
                "Commodity / Accepted",
            ]
        );
        assert_eq!(model.axes.stage_boundaries, [0.25, 0.5, 0.75, 1.0]);

        let database = model
            .nodes
            .iter()
            .find(|node| node.id == "Database")
            .unwrap();
        assert!(database.is_pipeline_parent);
        assert_eq!(database.source_strategy, Some(WardleySourceStrategy::Buy));
        assert_eq!(database.inertia, Some(true));
        let file_system = model
            .nodes
            .iter()
            .find(|node| node.label == "File System")
            .unwrap();
        assert_eq!(file_system.id, "Database_File System");
        assert!(file_system.in_pipeline);
        assert_eq!((file_system.x, file_system.y), (25.0, 50.0));
        assert_eq!(
            (file_system.label_offset_x, file_system.label_offset_y),
            (Some(-40), Some(20))
        );
        assert_eq!(
            model.pipelines[0].component_ids,
            ["Database_File System", "Database_SQL DB"]
        );

        assert_eq!(model.links[0].label.as_deref(), Some("entry"));
        assert_eq!(model.links[1].flow, Some(WardleyFlowDirection::Forward));
        assert_eq!(
            model.links[2].flow,
            Some(WardleyFlowDirection::Bidirectional)
        );
        assert_eq!(model.links[3].flow, Some(WardleyFlowDirection::Forward));
        assert_eq!(model.links[3].label.as_deref(), Some("backup"));
        assert!(model.links[4].dashed);
        assert_eq!(model.links[5].target, "Database_File System");
        assert_eq!(
            model.annotations_box,
            Some(WardleyPointRenderModel { x: 20.0, y: 10.0 })
        );
        assert_eq!(
            model.annotations[0].coordinates[0],
            WardleyPointRenderModel { x: 82.0, y: 78.0 }
        );
        assert_eq!(
            (model.accelerators[0].x, model.accelerators[0].y),
            (85.0, 20.0)
        );
        assert_eq!(
            (model.deaccelerators[0].x, model.deaccelerators[0].y),
            (35.0, 45.0)
        );
    }

    #[test]
    fn preserves_javascript_map_insertion_and_update_semantics() {
        let model = parse(
            r#"wardley-beta
component Shared [0.20, 0.20]
anchor Shared [0.90, 0.90]
anchor First [0.80, 0.80]
component Shared [0.40, 0.40] (market)
component Last [0.30, 0.30]
evolve Shared 0.60
evolve Shared 0.70
"#,
        );

        assert_eq!(
            model
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            ["Shared", "First", "Last"]
        );
        assert_eq!(model.nodes[0].class_name.as_deref(), Some("component"));
        assert_eq!((model.nodes[0].x, model.nodes[0].y), (40.0, 40.0));
        assert_eq!(
            model.nodes[0].source_strategy,
            Some(WardleySourceStrategy::Market)
        );
        assert_eq!(model.trends.len(), 1);
        assert_eq!(model.trends[0].target_x, 70.0);
    }

    #[test]
    fn covers_upstream_hyphen_no_space_and_pipeline_label_regressions() {
        let model = parse(
            r#"wardley-beta
component real-time processing [0.5, 0.5]
component end-user [0.8, 0.9]
component foo--bar [0.3, 0.4]
component foo- [0.2, 0.3]
real-time processing->end-user
foo--bar->foo-
pipeline real-time processing {
  component batch-loader [0.7] label [-40, 20]
}
batch-loader->end-user
"#,
        );

        assert!(model.nodes.iter().any(|node| node.id == "foo--bar"));
        assert!(model.nodes.iter().any(|node| node.id == "foo-"));
        let batch = model
            .nodes
            .iter()
            .find(|node| node.label == "batch-loader")
            .unwrap();
        assert_eq!(batch.id, "real-time processing_batch-loader");
        assert_eq!(
            (batch.label_offset_x, batch.label_offset_y),
            (Some(-40), Some(20))
        );
        assert_eq!(model.links[0].source, "real-time processing");
        assert_eq!(model.links[0].target, "end-user");
        assert_eq!(model.links[2].source, batch.id);
    }

    #[test]
    fn allows_blank_and_comment_lines_between_pipeline_components() {
        let model = parse(
            r#"wardley-beta
component Platform [0.5, 0.5]
pipeline Platform {
  component First [0.2]

  %% components may be separated by hidden comments
  component Second [0.8]
}
"#,
        );

        assert_eq!(
            model.pipelines[0].component_ids,
            ["Platform_First", "Platform_Second"]
        );
    }

    #[test]
    fn preserves_consecutive_horizontal_whitespace_in_unquoted_names() {
        let model = parse("wardley-beta\ncomponent Data  \t Platform [0.5, 0.5]\n");

        assert_eq!(model.nodes[0].id, "Data  \t Platform");
        assert_eq!(model.nodes[0].label, "Data  \t Platform");
    }

    #[test]
    fn allows_hidden_whitespace_inside_parenthesized_inertia() {
        let model = parse("wardley-beta\ncomponent API [0.5, 0.5] ( \t inertia \t )\n");

        assert_eq!(model.nodes[0].inertia, Some(true));
    }

    #[test]
    fn accepts_decimal_percentage_coordinates_but_rejects_integer_node_coordinates() {
        let model = parse("wardley-beta\ncomponent API [20.0, 75.0]\n");
        assert_eq!((model.nodes[0].x, model.nodes[0].y), (75.0, 20.0));

        let source = "wardley-beta\ncomponent API [1, 1]\n";
        let error = parse_wardley_model_for_render(source, &meta()).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected wardley parse error");
        };
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(
                source.find("1, 1").unwrap(),
                source.find("1, 1").unwrap() + 1,
            ))
        );
    }

    #[test]
    fn follows_optional_arrow_grammar_without_splitting_greedy_space_names() {
        let model = parse(
            "wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.2, 0.2]\n\"A\" \"B\"\nA->B+>\n",
        );
        assert_eq!(model.links.len(), 2);
        assert_eq!(
            (
                model.links[0].source.as_str(),
                model.links[0].target.as_str()
            ),
            ("A", "B")
        );
        assert_eq!(model.links[1].flow, Some(WardleyFlowDirection::Forward));

        let error = parse_wardley_model_for_render("wardley-beta\nA B\n", &meta()).unwrap_err();
        assert!(error.to_string().contains("second wardley link endpoint"));
    }

    #[test]
    fn rejects_out_of_range_coordinates_with_exact_source_span() {
        let source = "wardley-beta\ncomponent API [0.5, 101.0]\n";
        let error = parse_wardley_model_for_render(source, &meta()).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected wardley parse error");
        };
        let start = source.find("101.0").unwrap();
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(start, start + 5)));
        assert!(diagnostic.message().contains("Component \"API\" evolution"));
    }

    #[test]
    fn rejects_pipeline_without_existing_parent_and_recovers_same_parser_facts() {
        let source = "wardley-beta\npipeline Missing {\n  component Child [0.5]\n}\n";
        let error = parse_wardley_model_for_render(source, &meta()).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected wardley parse error");
        };
        let start = source.find("Missing").unwrap();
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(start, start + 7)));

        let facts = crate::family::test_support::editor_facts(
            parse_wardley_json_and_editor_facts,
            source,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Missing"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Child"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(start, start + 7))
        }));
    }

    #[test]
    fn combined_engine_projection_constructs_once_and_exposes_typed_editor_contracts() {
        let source = "wardley-beta\ncomponent API [0.6, 0.7]\ncomponent DB [0.4, 0.5]\nAPI -> DB\n";
        crate::diagrams::langium_common::reset_family_syntax_construction_count("wardley");
        let engine = Engine::new();
        let combined = engine.parse_diagram_snapshot_sync(source).unwrap().unwrap();
        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count("wardley"),
            1
        );
        let crate::ParsedEditorFacts::Available(facts) = combined.editor_facts() else {
            panic!("expected wardley editor facts");
        };
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert_eq!(
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == "API")
                .count(),
            2
        );
        let api: Vec<_> = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "API")
            .collect();
        assert_eq!(api[0].role, crate::EditorSemanticRole::Entity);
        assert_eq!(api[1].role, crate::EditorSemanticRole::Reference);

        let typed = engine
            .parse_diagram_for_render_model_with_type_sync(
                "wardley",
                source,
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Wardley(model) = typed.model() else {
            panic!("expected typed wardley model");
        };
        assert_eq!(model.nodes.len(), 2);
        assert_eq!(
            &render_model_to_compat_json(model, typed.metadata()).unwrap(),
            combined
                .outcome()
                .parsed_model()
                .expect("expected parsed snapshot")
        );
    }

    #[test]
    fn wardley_reference_roles_cover_links_evolve_and_pipeline_parents() {
        let source = concat!(
            "wardley-beta\n",
            "API -> DB\n",
            "component API [0.6, 0.7]\n",
            "component DB [0.4, 0.5]\n",
            "API -> DB\n",
            "evolve API 0.8\n",
            "pipeline DB {\n",
            "  component Storage [0.4]\n",
            "}\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_wardley_json_and_editor_facts,
            source,
            &meta(),
        );

        for name in ["API", "DB"] {
            let roles: Vec<_> = facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == name)
                .map(|symbol| symbol.role)
                .collect();
            assert_eq!(roles[0], crate::EditorSemanticRole::Reference);
            assert_eq!(
                roles
                    .iter()
                    .filter(|role| **role == crate::EditorSemanticRole::Entity)
                    .count(),
                1
            );
            assert_eq!(
                roles
                    .iter()
                    .filter(|role| **role == crate::EditorSemanticRole::Reference)
                    .count(),
                roles.len() - 1
            );
        }
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Storage" && symbol.role == crate::EditorSemanticRole::Entity
        }));
    }

    #[test]
    fn recovery_keeps_confirmed_prefix_and_pipeline_suffix() {
        let source = concat!(
            "wardley-beta\r\n",
            "component Before [0.10, 0.20]\r\n",
            "component Broken [0.30, 1]\r\n",
            "pipeline Before {\r\n",
            "  component Child [0.40]\r\n",
            "  invalid pipeline syntax\r\n",
            "  component \"后续 子项\" [0.50]\r\n",
            "}\r\n",
            "component \"后来 🤓\" [0.60, 0.70]\r\n",
            "note \"未结束 🤓",
        );
        let invalid_number = source.find(", 1]").unwrap() + 2;
        let error = parse_wardley_model_for_render(source, &meta()).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected wardley parse error");
        };
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(invalid_number, invalid_number + 1))
        );

        crate::diagrams::langium_common::reset_family_syntax_construction_count("wardley");
        let facts = crate::family::test_support::editor_facts(
            parse_wardley_json_and_editor_facts,
            source,
            &meta(),
        );
        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count("wardley"),
            1
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        for later in ["后续 子项", "后来 🤓"] {
            assert!(
                facts.symbols.iter().any(|symbol| symbol.name == later),
                "missing recovered symbol {later:?}: {:?}",
                facts.symbols
            );
        }
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(invalid_number, invalid_number + 1))
        }));
    }
}

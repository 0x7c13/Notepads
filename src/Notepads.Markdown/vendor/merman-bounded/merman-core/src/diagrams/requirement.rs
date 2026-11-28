use crate::diagrams::scan::{LineCursor, leading_whitespace_len};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticRole, EditorSemanticSymbol, Error, OperationControl, OperationControlResult,
    ParseMetadata, Result, SourceSpan, family::CombinedSemanticFailure,
};
use serde_json::{Map, Value, json};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

#[cfg(test)]
thread_local! {
    static REQUIREMENT_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_requirement_syntax_construction_count() {
    REQUIREMENT_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn requirement_syntax_construction_count() -> usize {
    REQUIREMENT_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementRenderNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default, rename = "requirementId")]
    pub requirement_id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default, rename = "verifyMethod")]
    pub verify_method: String,
    #[serde(default)]
    pub css_styles: Vec<String>,
    #[serde(default)]
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementRenderElement {
    pub name: String,
    #[serde(rename = "type")]
    pub element_type: String,
    #[serde(default, rename = "docRef")]
    pub doc_ref: String,
    #[serde(default)]
    pub css_styles: Vec<String>,
    #[serde(default)]
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequirementRenderRelationship {
    #[serde(rename = "type")]
    pub rel_type: String,
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementRenderClass {
    pub id: String,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default, rename = "textStyles")]
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementDiagramRenderModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub requirements: Vec<RequirementRenderNode>,
    #[serde(default)]
    pub elements: Vec<RequirementRenderElement>,
    #[serde(default)]
    pub relationships: Vec<RequirementRenderRelationship>,
    #[serde(default)]
    pub classes: BTreeMap<String, RequirementRenderClass>,
}

impl RequirementDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct RequirementBuilder {
    requirement_id: String,
    text: String,
    risk: String,
    verify_method: String,
}

impl RequirementBuilder {
    fn new() -> Self {
        Self {
            requirement_id: String::new(),
            text: String::new(),
            risk: String::new(),
            verify_method: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ElementBuilder {
    element_type: String,
    doc_ref: String,
}

impl ElementBuilder {
    fn new() -> Self {
        Self {
            element_type: String::new(),
            doc_ref: String::new(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct RequirementDb {
    direction: String,
    relations: Vec<RequirementRenderRelationship>,

    requirements: HashMap<String, RequirementRenderNode>,
    requirement_order: Vec<String>,

    elements: HashMap<String, RequirementRenderElement>,
    element_order: Vec<String>,

    classes: HashMap<String, RequirementRenderClass>,
}

impl RequirementDb {
    fn new() -> Self {
        Self {
            direction: "TB".to_string(),
            relations: Vec::new(),
            requirements: HashMap::new(),
            requirement_order: Vec::new(),
            elements: HashMap::new(),
            element_order: Vec::new(),
            classes: HashMap::new(),
        }
    }

    fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    fn add_requirement(&mut self, name: &str, requirement_type: &str, b: RequirementBuilder) {
        if self.requirements.contains_key(name) {
            return;
        }
        self.requirement_order.push(name.to_string());
        self.requirements.insert(
            name.to_string(),
            RequirementRenderNode {
                name: name.to_string(),
                node_type: requirement_type.to_string(),
                requirement_id: b.requirement_id,
                text: b.text,
                risk: b.risk,
                verify_method: b.verify_method,
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            },
        );
    }

    fn add_element(&mut self, name: &str, b: ElementBuilder) {
        if self.elements.contains_key(name) {
            return;
        }
        self.element_order.push(name.to_string());
        self.elements.insert(
            name.to_string(),
            RequirementRenderElement {
                name: name.to_string(),
                element_type: b.element_type,
                doc_ref: b.doc_ref,
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            },
        );
    }

    fn add_relationship(&mut self, relationship_type: &str, src: &str, dst: &str) {
        self.relations.push(RequirementRenderRelationship {
            rel_type: relationship_type.to_string(),
            src: src.to_string(),
            dst: dst.to_string(),
        });
    }

    fn editor_entity_kind(&self, name: &str) -> Option<EditorSemanticKind> {
        if self.requirements.contains_key(name) {
            Some(EditorSemanticKind::Struct)
        } else if self.elements.contains_key(name) {
            Some(EditorSemanticKind::Object)
        } else {
            None
        }
    }

    fn set_css_style(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let node_req = self.requirements.get_mut(id);
            if let Some(node) = node_req {
                push_styles(&mut node.css_styles, styles);
                continue;
            }
            let node_el = self.elements.get_mut(id);
            if let Some(node) = node_el {
                push_styles(&mut node.css_styles, styles);
                continue;
            }
        }
    }

    fn set_class(&mut self, ids: &[String], class_names: &[String]) {
        for id in ids {
            if let Some(node) = self.requirements.get_mut(id) {
                for cls in class_names {
                    node.classes.push(cls.clone());
                    if let Some(def) = self.classes.get(cls) {
                        node.css_styles.extend(def.styles.iter().cloned());
                    }
                }
                continue;
            }
            if let Some(node) = self.elements.get_mut(id) {
                for cls in class_names {
                    node.classes.push(cls.clone());
                    if let Some(def) = self.classes.get(cls) {
                        node.css_styles.extend(def.styles.iter().cloned());
                    }
                }
            }
        }
    }

    fn define_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let style_class =
                self.classes
                    .entry(id.to_string())
                    .or_insert_with(|| RequirementRenderClass {
                        id: id.to_string(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    });

            for s in styles {
                if s.contains("color") {
                    let new_style = s.replacen("fill", "bgFill", 1);
                    style_class.text_styles.push(new_style);
                }
                style_class.styles.push(s.clone());
            }

            for req_name in &self.requirement_order {
                if let Some(req) = self.requirements.get_mut(req_name)
                    && req.classes.iter().any(|c| c == id)
                {
                    req.css_styles.extend(
                        styles
                            .iter()
                            .flat_map(|s| s.split(','))
                            .map(|s| s.to_string()),
                    );
                }
            }
            for el_name in &self.element_order {
                if let Some(el) = self.elements.get_mut(el_name)
                    && el.classes.iter().any(|c| c == id)
                {
                    el.css_styles.extend(
                        styles
                            .iter()
                            .flat_map(|s| s.split(','))
                            .map(|s| s.to_string()),
                    );
                }
            }
        }
    }

    fn to_render_model(
        &self,
        acc_title: Option<String>,
        acc_descr: Option<String>,
    ) -> RequirementDiagramRenderModel {
        let requirements = self
            .requirement_order
            .iter()
            .filter_map(|k| self.requirements.get(k))
            .cloned()
            .collect::<Vec<_>>();

        let elements = self
            .element_order
            .iter()
            .filter_map(|k| self.elements.get(k))
            .cloned()
            .collect::<Vec<_>>();

        let mut classes = BTreeMap::new();
        for (k, c) in &self.classes {
            classes.insert(k.clone(), c.clone());
        }

        RequirementDiagramRenderModel {
            acc_title,
            acc_descr,
            direction: self.direction.clone(),
            requirements,
            elements,
            relationships: self.relations.clone(),
            classes,
        }
    }
}

fn push_styles(out: &mut Vec<String>, styles: &[String]) {
    for s in styles {
        if s.contains(',') {
            out.extend(s.split(',').map(|p| p.to_string()));
        } else {
            out.push(s.to_string());
        }
    }
}

pub(crate) fn parse_requirement(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let model = parse_requirement_semantic_source(code, meta)?.model;
    render_model_to_compat_json(&model, meta)
}

#[cfg(test)]
pub(crate) fn parse_requirement_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<RequirementDiagramRenderModel> {
    Ok(parse_requirement_semantic_source(code, meta)?.model)
}

pub(crate) fn parse_requirement_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RequirementDiagramRenderModel>> {
    let construction = construct_requirement_semantic_source_controlled(code, meta, control)?;
    match construction {
        Ok(source) => Ok(Ok(source.model)),
        Err(failure) => Ok(Err(failure.into_error())),
    }
}

pub(crate) fn parse_requirement_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_requirement_semantic_source_controlled(code, meta, control)?;
    Ok(crate::family::CombinedSemanticParse::from_construction(
        construction,
        |RequirementSemanticSource {
             model,
             editor_facts,
         }| (render_model_to_compat_json(&model, meta), editor_facts),
        CombinedSemanticFailure::into_parts,
    ))
}

struct RequirementSemanticSource {
    model: RequirementDiagramRenderModel,
    editor_facts: EditorSemanticFacts,
}

#[derive(Debug, Clone, Copy)]
struct SpannedValue<'a> {
    text: &'a str,
    start: usize,
}

fn requirement_statement_start(line: &str, line_start: usize) -> usize {
    line_start + leading_whitespace_len(line)
}

fn trim_spanned_value(raw: &str, raw_start: usize) -> Option<SpannedValue<'_>> {
    let leading = raw.len() - raw.trim_start().len();
    let without_leading = &raw[leading..];
    let trailing = without_leading.len() - without_leading.trim_end().len();
    let end = raw.len().saturating_sub(trailing);
    if leading >= end {
        return None;
    }
    Some(SpannedValue {
        text: &raw[leading..end],
        start: raw_start + leading,
    })
}

fn parse_keyword_rest_ci<'a>(line: &'a str, key: &str) -> Option<(&'a str, usize)> {
    let leading = line.len() - line.trim_start().len();
    let t = &line[leading..];
    if !t
        .get(..key.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(key))
    {
        return None;
    }
    let rest_start = leading + key.len();
    let rest = &line[rest_start..];
    let rest_leading = rest.len() - rest.trim_start().len();
    let rest_start = rest_start + rest_leading;
    let rest = &line[rest_start..];
    if rest.starts_with(':') || rest.starts_with('{') {
        Some((rest, rest_start))
    } else {
        None
    }
}

fn push_requirement_payload_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    start: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let span = SourceSpan::new(start, start + text.len());
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

fn emit_parsed_requirement_ids(
    facts: &mut EditorSemanticFacts,
    ids: &ParsedRequirementIdList,
    statement_span: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
    role: EditorSemanticRole,
) {
    for id in &ids.items {
        facts.push_symbol(EditorSemanticSymbol::with_role(
            id.value.clone(),
            Some(detail.to_string()),
            kind,
            role,
            statement_span,
            id.selection,
        ));
    }
}

fn emit_parsed_requirement_styles(
    facts: &mut EditorSemanticFacts,
    styles: &ParsedRequirementStyles,
    detail: &'static str,
) {
    for style in &styles.items {
        facts.push_symbol(EditorSemanticSymbol::payload(
            style.value.clone(),
            Some(detail.to_string()),
            EditorSemanticKind::Property,
            style.span,
            style.span,
        ));
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &RequirementDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut out = Map::with_capacity(9);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("direction".to_string(), json!(&model.direction));
    out.insert("requirements".to_string(), json!(&model.requirements));
    out.insert("elements".to_string(), json!(&model.elements));
    out.insert("relationships".to_string(), json!(&model.relationships));
    out.insert("classes".to_string(), json!(&model.classes));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

fn parse_requirement_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<RequirementSemanticSource> {
    construct_requirement_semantic_source(code, meta).map_err(CombinedSemanticFailure::into_error)
}

fn construct_requirement_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<RequirementSemanticSource, CombinedSemanticFailure> {
    construct_requirement_semantic_source_controlled(code, meta, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_requirement_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<RequirementSemanticSource, CombinedSemanticFailure>>
{
    control.checkpoint()?;
    #[cfg(test)]
    REQUIREMENT_SYNTAX_CONSTRUCTION_COUNT.set(REQUIREMENT_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    parse_requirement_semantic_source_once(code, meta, control)
}

fn parse_requirement_semantic_source_once(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<RequirementSemanticSource, CombinedSemanticFailure>>
{
    control.checkpoint()?;
    let mut db = RequirementDb::new();
    let mut acc_title: Option<String> = None;
    let mut acc_descr: Option<String> = None;
    let mut editor_facts = EditorSemanticFacts::new();
    let mut lines = LineCursor::new(code);
    let mut saw_header = false;
    let mut first_error = None;

    while let Some((raw, line_start)) = lines.next_line() {
        control.checkpoint()?;
        let stripped = split_requirement_line(raw);
        let t = stripped.trim();
        if t.is_empty() {
            continue;
        }
        let statement_start = requirement_statement_start(stripped, line_start);

        if let Some((rest, rest_start)) = parse_keyword_rest_ci(stripped, "accTitle")
            && let Some(raw_value) = rest.strip_prefix(':')
        {
            let value = trim_spanned_value(raw_value, rest_start + 1);
            editor_facts.push_directive_prefix("accTitle");
            if let Some(value) = value {
                push_requirement_payload_fact(
                    &mut editor_facts,
                    value.text,
                    line_start + value.start,
                    "requirement accessibility title",
                    EditorSemanticKind::String,
                );
            }
            acc_title = Some(
                value
                    .map(|value| value.text)
                    .unwrap_or_default()
                    .to_string(),
            );
            continue;
        }
        if let Some(parsed) =
            parse_requirement_acc_descr(stripped, raw, line_start, &mut lines, control)?
        {
            editor_facts.push_directive_prefix("accDescr");
            parsed.emit_editor_fact(&mut editor_facts);
            if !parsed.complete {
                editor_facts.mark_recovered_from_parse_error(
                    "requirement parser recovered from unterminated accDescr block",
                    Some(parsed.statement_span),
                );
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        "unterminated accDescr block",
                        parsed.statement_span.end,
                    )
                });
            }
            acc_descr = Some(parsed.value);
            continue;
        }

        if !saw_header {
            if t.eq_ignore_ascii_case("requirementDiagram") {
                saw_header = true;
                continue;
            }
            first_error.get_or_insert(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                "expected requirementDiagram",
                requirement_statement_span(raw, line_start),
            ));
            continue;
        }

        if let Some(direction) = parse_direction(t, statement_start) {
            direction.emit_editor_fact(
                &mut editor_facts,
                requirement_statement_span(raw, line_start),
            );
            if let Some(value) = direction.value {
                db.set_direction(value);
            } else {
                first_error.get_or_insert(Error::diagram_parse_exact(
                    meta.diagram_type.clone(),
                    "invalid requirement direction",
                    direction.selection,
                ));
            }
            continue;
        }

        let requirement_open = match parse_requirement_def_open(t, statement_start) {
            Ok(open) => open,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(RequirementDefOpen {
            name,
            requirement_type: ty,
            classes,
        }) = requirement_open
        {
            let statement_span = requirement_statement_span(raw, line_start);
            emit_requirement_definition(
                &mut editor_facts,
                &name,
                classes.as_ref(),
                statement_span,
                &ty.to_lowercase(),
                EditorSemanticKind::Struct,
            );
            let body = parse_requirement_body(&mut lines, meta, &mut editor_facts, control)?;
            if let Some(error) = body.error {
                first_error.get_or_insert(error);
            }
            db.add_requirement(&name.value, &ty, body.value);
            if let Some(classes) = classes.as_ref().map(ParsedRequirementIdList::values) {
                db.set_class(std::slice::from_ref(&name.value), &classes);
            }
            continue;
        }

        let element_open = match parse_element_def_open(t, statement_start) {
            Ok(open) => open,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(ElementDefOpen { name, classes }) = element_open {
            let statement_span = requirement_statement_span(raw, line_start);
            emit_requirement_definition(
                &mut editor_facts,
                &name,
                classes.as_ref(),
                statement_span,
                "requirement element",
                EditorSemanticKind::Object,
            );
            let body = parse_element_body(&mut lines, meta, &mut editor_facts, control)?;
            if let Some(error) = body.error {
                first_error.get_or_insert(error);
            }
            db.add_element(&name.value, body.value);
            if let Some(classes) = classes.as_ref().map(ParsedRequirementIdList::values) {
                db.set_class(std::slice::from_ref(&name.value), &classes);
            }
            continue;
        }

        let shorthand = match parse_shorthand_class_stmt(t, statement_start) {
            Ok(statement) => statement,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(statement) = shorthand {
            emit_requirement_shorthand_class(
                &mut editor_facts,
                &statement,
                requirement_statement_span(raw, line_start),
            );
            db.set_class(
                std::slice::from_ref(&statement.target.value),
                &statement.classes.values(),
            );
            continue;
        }

        let style = match parse_style_stmt(t, statement_start) {
            Ok(statement) => statement,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(statement) = style {
            emit_requirement_style(
                &mut editor_facts,
                &statement,
                requirement_statement_span(raw, line_start),
            );
            db.set_css_style(&statement.ids.values(), &statement.styles.values());
            continue;
        }

        let class_def = match parse_classdef_stmt(t, statement_start) {
            Ok(statement) => statement,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(statement) = class_def {
            emit_requirement_class_def(
                &mut editor_facts,
                &statement,
                requirement_statement_span(raw, line_start),
            );
            db.define_class(&statement.ids.values(), &statement.styles.values());
            continue;
        }

        let class = match parse_class_stmt(t, statement_start) {
            Ok(statement) => statement,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(statement) = class {
            emit_requirement_class(
                &mut editor_facts,
                &statement,
                requirement_statement_span(raw, line_start),
            );
            db.set_class(&statement.targets.values(), &statement.classes.values());
            continue;
        }

        let relationship = match parse_relationship_stmt(t, statement_start) {
            Ok(statement) => statement,
            Err(error) => {
                first_error.get_or_insert(requirement_exact_error(
                    error,
                    meta,
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        if let Some(relationship) = relationship {
            emit_requirement_relationship(
                &mut editor_facts,
                &relationship,
                requirement_statement_span(raw, line_start),
            );
            db.add_relationship(
                &relationship.kind,
                &relationship.source.value,
                &relationship.target.value,
            );
            continue;
        }

        first_error.get_or_insert(Error::diagram_parse_exact(
            meta.diagram_type.clone(),
            format!("unexpected requirement statement: {t}"),
            requirement_statement_span(raw, line_start),
        ));
    }

    if !saw_header {
        first_error.get_or_insert(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected requirementDiagram",
            code.len(),
        ));
    }

    resolve_requirement_reference_kinds(&mut editor_facts, &db);

    if let Some(error) = first_error {
        return Ok(Err(CombinedSemanticFailure::parser_recovery(
            "requirement",
            error,
            editor_facts,
        )));
    }

    control.checkpoint()?;
    Ok(Ok(RequirementSemanticSource {
        model: db.to_render_model(acc_title, acc_descr),
        editor_facts,
    }))
}

fn requirement_exact_error(error: Error, meta: &ParseMetadata, span: SourceSpan) -> Error {
    let message = match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
        error => error.to_string(),
    };
    Error::diagram_parse_exact(meta.diagram_type.clone(), message, span)
}

fn requirement_statement_span(line: &str, line_start: usize) -> SourceSpan {
    let leading = leading_whitespace_len(line);
    let end = line.trim_end().len();
    SourceSpan::new(line_start + leading, line_start + end.max(leading))
}

struct RequirementAccDescr {
    value: String,
    statement_span: SourceSpan,
    selection: Option<SourceSpan>,
    complete: bool,
}

impl RequirementAccDescr {
    fn emit_editor_fact(&self, facts: &mut EditorSemanticFacts) {
        let Some(selection) = self.selection else {
            return;
        };
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            selection,
        ));
        facts.push_symbol(EditorSemanticSymbol::payload(
            self.value.clone(),
            Some("requirement accessibility description".to_string()),
            EditorSemanticKind::String,
            self.statement_span,
            selection,
        ));
    }
}

fn parse_requirement_acc_descr(
    line: &str,
    raw_line: &str,
    line_start: usize,
    cursor: &mut LineCursor<'_>,
    control: &OperationControl,
) -> OperationControlResult<Option<RequirementAccDescr>> {
    control.checkpoint()?;
    let Some((rest, rest_start)) = parse_keyword_rest_ci(line, "accDescr") else {
        return Ok(None);
    };
    let statement_start = requirement_statement_span(raw_line, line_start).start;

    if let Some(raw_value) = rest.strip_prefix(':') {
        let value = trim_spanned_value(raw_value, rest_start + 1);
        return Ok(Some(RequirementAccDescr {
            value: value
                .map(|value| value.text)
                .unwrap_or_default()
                .to_string(),
            statement_span: requirement_statement_span(raw_line, line_start),
            selection: value.map(|value| {
                SourceSpan::new(
                    line_start + value.start,
                    line_start + value.start + value.text.len(),
                )
            }),
            complete: true,
        }));
    }

    let Some(after_brace) = rest.strip_prefix('{') else {
        return Ok(None);
    };
    let mut value_lines = Vec::new();
    let mut first_content_start = None;
    let mut last_content_end = None;
    let mut statement_end = line_start + raw_line.len();
    let mut complete = false;

    let first_start = line_start + rest_start + 1;
    if let Some(close) = after_brace.find('}') {
        append_requirement_acc_descr_line(
            &after_brace[..close],
            first_start,
            &mut value_lines,
            &mut first_content_start,
            &mut last_content_end,
        );
        statement_end = first_start + close + 1;
        complete = true;
        cursor.resume_same_line_at(statement_end);
    } else {
        append_requirement_acc_descr_line(
            after_brace,
            first_start,
            &mut value_lines,
            &mut first_content_start,
            &mut last_content_end,
        );
        while let Some((next, next_start)) = cursor.next_line() {
            control.checkpoint()?;
            statement_end = next_start + next.len();
            if let Some(close) = next.find('}') {
                append_requirement_acc_descr_line(
                    &next[..close],
                    next_start,
                    &mut value_lines,
                    &mut first_content_start,
                    &mut last_content_end,
                );
                statement_end = next_start + close + 1;
                complete = true;
                cursor.resume_same_line_at(statement_end);
                break;
            }
            append_requirement_acc_descr_line(
                next,
                next_start,
                &mut value_lines,
                &mut first_content_start,
                &mut last_content_end,
            );
        }
    }

    let parsed = RequirementAccDescr {
        value: value_lines.join("\n").trim().to_string(),
        statement_span: SourceSpan::new(statement_start, statement_end),
        selection: first_content_start
            .zip(last_content_end)
            .map(|(start, end)| SourceSpan::new(start, end)),
        complete,
    };
    Ok(Some(parsed))
}

fn append_requirement_acc_descr_line(
    raw: &str,
    start: usize,
    lines: &mut Vec<String>,
    first_content_start: &mut Option<usize>,
    last_content_end: &mut Option<usize>,
) {
    let leading = leading_whitespace_len(raw);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        return;
    }
    *first_content_start = Some(first_content_start.unwrap_or(start + leading));
    *last_content_end = Some(start + raw.trim_end().len());
    lines.push(trimmed.to_string());
}

fn emit_requirement_definition(
    facts: &mut EditorSemanticFacts,
    name: &ParsedRequirementId,
    classes: Option<&ParsedRequirementIdList>,
    statement_span: SourceSpan,
    detail: &str,
    kind: EditorSemanticKind,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        name.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::new(
        name.value.clone(),
        Some(detail.to_string()),
        kind,
        statement_span,
        name.selection,
    ));
    if let Some(classes) = classes {
        emit_parsed_requirement_ids(
            facts,
            classes,
            statement_span,
            "requirement class",
            EditorSemanticKind::Property,
            EditorSemanticRole::Payload,
        );
    }
}

fn emit_requirement_shorthand_class(
    facts: &mut EditorSemanticFacts,
    statement: &ParsedRequirementClassShorthand,
    statement_span: SourceSpan,
) {
    facts.push_symbol(EditorSemanticSymbol::reference(
        statement.target.value.clone(),
        Some("requirement class target".to_string()),
        EditorSemanticKind::Struct,
        statement_span,
        statement.target.selection,
    ));
    emit_parsed_requirement_ids(
        facts,
        &statement.classes,
        statement_span,
        "requirement class",
        EditorSemanticKind::Property,
        EditorSemanticRole::Payload,
    );
}

fn emit_requirement_style(
    facts: &mut EditorSemanticFacts,
    statement: &ParsedRequirementStyleStatement,
    statement_span: SourceSpan,
) {
    emit_parsed_requirement_ids(
        facts,
        &statement.ids,
        statement_span,
        "requirement style target",
        EditorSemanticKind::Struct,
        EditorSemanticRole::Reference,
    );
    emit_parsed_requirement_styles(facts, &statement.styles, "requirement style");
}

fn emit_requirement_class_def(
    facts: &mut EditorSemanticFacts,
    statement: &ParsedRequirementStyleStatement,
    statement_span: SourceSpan,
) {
    emit_parsed_requirement_ids(
        facts,
        &statement.ids,
        statement_span,
        "requirement class definition",
        EditorSemanticKind::Property,
        EditorSemanticRole::ClassDefinition,
    );
    emit_parsed_requirement_styles(facts, &statement.styles, "requirement class style");
}

fn emit_requirement_class(
    facts: &mut EditorSemanticFacts,
    statement: &ParsedRequirementClassStatement,
    statement_span: SourceSpan,
) {
    emit_parsed_requirement_ids(
        facts,
        &statement.classes,
        statement_span,
        "requirement class",
        EditorSemanticKind::Property,
        EditorSemanticRole::Payload,
    );
    emit_parsed_requirement_ids(
        facts,
        &statement.targets,
        statement_span,
        "requirement class target",
        EditorSemanticKind::Struct,
        EditorSemanticRole::Reference,
    );
}

fn emit_requirement_relationship(
    facts: &mut EditorSemanticFacts,
    relationship: &ParsedRequirementRelationship,
    statement_span: SourceSpan,
) {
    facts.push_symbol(EditorSemanticSymbol::payload(
        relationship.kind.clone(),
        Some("requirement relationship".to_string()),
        EditorSemanticKind::String,
        statement_span,
        relationship.kind_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::reference(
        relationship.source.value.clone(),
        Some("requirement relationship source".to_string()),
        EditorSemanticKind::Struct,
        statement_span,
        relationship.source.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::reference(
        relationship.target.value.clone(),
        Some("requirement relationship target".to_string()),
        EditorSemanticKind::Struct,
        statement_span,
        relationship.target.selection,
    ));
}

fn resolve_requirement_reference_kinds(facts: &mut EditorSemanticFacts, db: &RequirementDb) {
    for symbol in &mut facts.symbols {
        if symbol.role == EditorSemanticRole::Reference
            && let Some(kind) = db.editor_entity_kind(&symbol.name)
        {
            symbol.kind = kind;
        }
    }
}

fn split_requirement_line(line: &str) -> &str {
    let lowered = line.trim_start().to_ascii_lowercase();
    if lowered.starts_with("style")
        || lowered.starts_with("classdef")
        || lowered.starts_with("class ")
        || lowered == "class"
    {
        return line;
    }

    let mut in_quotes = false;
    let mut idx = 0usize;
    let bytes = line.as_bytes();
    while idx < bytes.len() {
        let b = bytes[idx];
        if b == b'"' {
            in_quotes = !in_quotes;
            idx += 1;
            continue;
        }
        if !in_quotes {
            if b == b'#' {
                return &line[..idx];
            }
            if b == b'%' && idx + 1 < bytes.len() && bytes[idx + 1] == b'%' {
                return &line[..idx];
            }
        }
        idx += 1;
    }
    line
}

struct ParsedRequirementDirection {
    value: Option<&'static str>,
    selection: SourceSpan,
}

impl ParsedRequirementDirection {
    fn emit_editor_fact(&self, facts: &mut EditorSemanticFacts, statement_span: SourceSpan) {
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::CardinalDirectionValue,
            self.selection,
        ));
        if let Some(value) = self.value {
            facts.push_symbol(EditorSemanticSymbol::payload(
                value,
                Some("requirement direction".to_string()),
                EditorSemanticKind::String,
                statement_span,
                self.selection,
            ));
        }
    }
}

fn parse_direction(t: &str, statement_start: usize) -> Option<ParsedRequirementDirection> {
    let (keyword, rest) = split_first_word(t)?;
    if !keyword.eq_ignore_ascii_case("direction") {
        return None;
    }
    let rest_start = t.len() - rest.len();
    let value_start = statement_start + rest_start + leading_whitespace_len(rest);
    let dir = split_first_word(rest).map_or("", |(dir, _)| dir);
    let value = match dir.to_ascii_uppercase().as_str() {
        "TB" => Some("TB"),
        "BT" => Some("BT"),
        "LR" => Some("LR"),
        "RL" => Some("RL"),
        _ => None,
    };
    Some(ParsedRequirementDirection {
        value,
        selection: SourceSpan::new(value_start, value_start + dir.len()),
    })
}

fn parse_requirement_def_open(
    t: &str,
    statement_start: usize,
) -> Result<Option<RequirementDefOpen>> {
    let t = t.trim();
    if !t.ends_with('{') {
        return Ok(None);
    }

    let without_brace = t[..t.len() - 1].trim_end();
    let (ty_raw, rest) = split_first_word(without_brace).ok_or_else(|| {
        Error::diagram_parse_fallback(
            "requirement".to_string(),
            "invalid requirement definition".to_string(),
        )
    })?;

    let requirement_type = match ty_raw.to_ascii_lowercase().as_str() {
        "requirement" => "Requirement",
        "functionalrequirement" => "Functional Requirement",
        "interfacerequirement" => "Interface Requirement",
        "performancerequirement" => "Performance Requirement",
        "physicalrequirement" => "Physical Requirement",
        "designconstraint" => "Design Constraint",
        _ => return Ok(None),
    }
    .to_string();

    let rest_start = without_brace.len() - rest.len();
    let name_and_classes = split_name_and_classes(rest, statement_start + rest_start)?;
    if name_and_classes.name.value.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            "requirement name is empty".to_string(),
        ));
    }
    Ok(Some(RequirementDefOpen {
        name: name_and_classes.name,
        requirement_type,
        classes: name_and_classes.class_tokens,
    }))
}

struct RequirementDefOpen {
    name: ParsedRequirementId,
    requirement_type: String,
    classes: Option<ParsedRequirementIdList>,
}

fn parse_element_def_open(t: &str, statement_start: usize) -> Result<Option<ElementDefOpen>> {
    let t = t.trim();
    if !t.ends_with('{') {
        return Ok(None);
    }

    let without_brace = t[..t.len() - 1].trim_end();
    let (kw, rest) = split_first_word(without_brace).ok_or_else(|| {
        Error::diagram_parse_fallback(
            "requirement".to_string(),
            "invalid element definition".to_string(),
        )
    })?;
    if !kw.eq_ignore_ascii_case("element") {
        return Ok(None);
    }

    let rest_start = without_brace.len() - rest.len();
    let name_and_classes = split_name_and_classes(rest, statement_start + rest_start)?;
    if name_and_classes.name.value.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            "element name is empty".to_string(),
        ));
    }
    Ok(Some(ElementDefOpen {
        name: name_and_classes.name,
        classes: name_and_classes.class_tokens,
    }))
}

struct ElementDefOpen {
    name: ParsedRequirementId,
    classes: Option<ParsedRequirementIdList>,
}

fn split_first_word(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let mut iter = input.splitn(2, char::is_whitespace);
    let first = iter.next()?;
    let rest = iter.next().unwrap_or("");
    Some((first, rest))
}

fn split_name_and_classes(input: &str, input_start: usize) -> Result<DefinitionName> {
    let leading = leading_whitespace_len(input);
    let input = input.trim();
    if input.is_empty() {
        return Ok(DefinitionName {
            name: ParsedRequirementId {
                value: String::new(),
                selection: SourceSpan::new(input_start + leading, input_start + leading),
            },
            class_tokens: None,
        });
    }

    if let Some(pos) = input.find(":::") {
        let name_raw = input[..pos].trim_end();
        let classes_raw = &input[pos + 3..];
        let name = parse_requirement_id_spanned(name_raw, input_start + leading)?;
        let classes_start = input_start + leading + pos + 3;
        let class_tokens = parse_id_list_all_spanned(classes_raw, classes_start)?;
        return Ok(DefinitionName {
            name,
            class_tokens: Some(class_tokens),
        });
    }

    let name = parse_requirement_id_spanned(input, input_start + leading)?;
    Ok(DefinitionName {
        name,
        class_tokens: None,
    })
}

struct DefinitionName {
    name: ParsedRequirementId,
    class_tokens: Option<ParsedRequirementIdList>,
}

fn parse_id_or_name(input: &str) -> Result<(String, &str)> {
    let input = input.trim_start();
    if input.starts_with('"') {
        if let Some((val, rest)) = parse_quoted_prefix(input) {
            return Ok((val, rest));
        }
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            "unterminated string".to_string(),
        ));
    }
    Ok((input.trim().to_string(), ""))
}

fn parse_quoted_prefix(input: &str) -> Option<(String, &str)> {
    let mut chars = input.chars();
    if chars.next()? != '"' {
        return None;
    }
    let mut out = String::new();
    let mut idx = 1usize;
    for c in chars {
        idx += c.len_utf8();
        if c == '"' {
            return Some((out, &input[idx..]));
        }
        out.push(c);
    }
    None
}

struct RequirementBodyParse<T> {
    value: T,
    error: Option<Error>,
}

fn parse_requirement_body(
    lines: &mut LineCursor<'_>,
    meta: &ParseMetadata,
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<RequirementBodyParse<RequirementBuilder>> {
    control.checkpoint()?;
    let mut b = RequirementBuilder::new();
    let mut error = None;
    while let Some((raw, line_start)) = lines.next_line() {
        control.checkpoint()?;
        let line = split_requirement_line(raw);
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t == "}" {
            return Ok(RequirementBodyParse { value: b, error });
        }

        let Some(key_value) = split_key_value_spanned(line) else {
            error.get_or_insert(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                format!("invalid requirement body line: {t}"),
                requirement_statement_span(raw, line_start),
            ));
            continue;
        };
        let k = key_value.key.text;
        let key = k.to_ascii_lowercase();
        let Some(value) = key_value.value else {
            error.get_or_insert(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                format!("invalid requirement body line: {t}"),
                requirement_statement_span(raw, line_start),
            ));
            continue;
        };
        let value_span = SourceSpan::new(
            line_start + value.start,
            line_start + value.start + value.text.len(),
        );
        let detail = match key.as_str() {
            "id" => "requirement id",
            "text" => "requirement text",
            "risk" => "requirement risk",
            "verifymethod" => "requirement verify method",
            _ => {
                error.get_or_insert(Error::diagram_parse_exact(
                    meta.diagram_type.clone(),
                    format!("unexpected requirement body key: {k}"),
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        push_requirement_payload_fact(
            facts,
            value.text,
            value_span.start,
            detail,
            EditorSemanticKind::String,
        );
        let parsed = match parse_simple_value(value.text) {
            Ok(parsed) => parsed,
            Err(parse_error) => {
                error.get_or_insert(requirement_exact_error(parse_error, meta, value_span));
                continue;
            }
        };
        match key.as_str() {
            "id" => b.requirement_id = parsed,
            "text" => b.text = parsed,
            "risk" => match normalize_risk(&parsed) {
                Ok(risk) => b.risk = risk,
                Err(parse_error) => {
                    error.get_or_insert(requirement_exact_error(parse_error, meta, value_span));
                }
            },
            "verifymethod" => match normalize_verify_method(&parsed) {
                Ok(method) => b.verify_method = method,
                Err(parse_error) => {
                    error.get_or_insert(requirement_exact_error(parse_error, meta, value_span));
                }
            },
            _ => unreachable!("body key was validated above"),
        }
    }

    error.get_or_insert(Error::diagram_parse_insertion_point(
        meta.diagram_type.clone(),
        "unterminated requirement block",
        lines.offset(),
    ));
    Ok(RequirementBodyParse { value: b, error })
}

fn parse_element_body(
    lines: &mut LineCursor<'_>,
    meta: &ParseMetadata,
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<RequirementBodyParse<ElementBuilder>> {
    control.checkpoint()?;
    let mut b = ElementBuilder::new();
    let mut error = None;
    while let Some((raw, line_start)) = lines.next_line() {
        control.checkpoint()?;
        let line = split_requirement_line(raw);
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t == "}" {
            return Ok(RequirementBodyParse { value: b, error });
        }

        let Some(key_value) = split_key_value_spanned(line) else {
            error.get_or_insert(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                format!("invalid element body line: {t}"),
                requirement_statement_span(raw, line_start),
            ));
            continue;
        };
        let k = key_value.key.text;
        let key = k.to_ascii_lowercase();
        let Some(value) = key_value.value else {
            error.get_or_insert(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                format!("invalid element body line: {t}"),
                requirement_statement_span(raw, line_start),
            ));
            continue;
        };
        let value_span = SourceSpan::new(
            line_start + value.start,
            line_start + value.start + value.text.len(),
        );
        let detail = match key.as_str() {
            "type" => "requirement element type",
            "docref" => "requirement doc ref",
            _ => {
                error.get_or_insert(Error::diagram_parse_exact(
                    meta.diagram_type.clone(),
                    format!("unexpected element body key: {k}"),
                    requirement_statement_span(raw, line_start),
                ));
                continue;
            }
        };
        push_requirement_payload_fact(
            facts,
            value.text,
            value_span.start,
            detail,
            EditorSemanticKind::String,
        );
        let parsed = match parse_simple_value(value.text) {
            Ok(parsed) => parsed,
            Err(parse_error) => {
                error.get_or_insert(requirement_exact_error(parse_error, meta, value_span));
                continue;
            }
        };
        match key.as_str() {
            "type" => b.element_type = parsed,
            "docref" => b.doc_ref = parsed,
            _ => unreachable!("element key was validated above"),
        }
    }

    error.get_or_insert(Error::diagram_parse_insertion_point(
        meta.diagram_type.clone(),
        "unterminated element block",
        lines.offset(),
    ));
    Ok(RequirementBodyParse { value: b, error })
}

struct SpannedKeyValue<'a> {
    key: SpannedValue<'a>,
    value: Option<SpannedValue<'a>>,
}

fn split_key_value_spanned(input: &str) -> Option<SpannedKeyValue<'_>> {
    let idx = input.find(':')?;
    let key = trim_spanned_value(&input[..idx], 0)?;
    let value = trim_spanned_value(&input[idx + 1..], idx + 1);
    Some(SpannedKeyValue { key, value })
}

fn parse_simple_value(input: &str) -> Result<String> {
    let input = input.trim();
    if input.starts_with('"') {
        if let Some((val, rest)) = parse_quoted_prefix(input) {
            if !rest.trim().is_empty() {
                return Err(Error::diagram_parse_fallback(
                    "requirement".to_string(),
                    format!("unexpected trailing tokens after string: {}", rest.trim()),
                ));
            }
            return Ok(val.trim().to_string());
        }
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            "unterminated string".to_string(),
        ));
    }
    Ok(input.trim().to_string())
}

fn normalize_risk(input: &str) -> Result<String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "low" => Ok("Low".to_string()),
        "medium" => Ok("Medium".to_string()),
        "high" => Ok("High".to_string()),
        other => Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid risk level: {other}"),
        )),
    }
}

fn normalize_verify_method(input: &str) -> Result<String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "analysis" => Ok("Analysis".to_string()),
        "demonstration" => Ok("Demonstration".to_string()),
        "inspection" => Ok("Inspection".to_string()),
        "test" => Ok("Test".to_string()),
        other => Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid verify method: {other}"),
        )),
    }
}

fn parse_requirement_id_spanned(input: &str, input_start: usize) -> Result<ParsedRequirementId> {
    let leading = leading_whitespace_len(input);
    let input = input.trim();
    let token_start = input_start + leading;
    let (value, rest) = parse_id_or_name(input)?;
    if !rest.trim().is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!(
                "unexpected trailing tokens after identifier: {}",
                rest.trim()
            ),
        ));
    }
    if input.starts_with('"') {
        let selection = SourceSpan::new(token_start + 1, token_start + 1 + value.len());
        return Ok(ParsedRequirementId { value, selection });
    }
    Ok(ParsedRequirementId {
        selection: SourceSpan::new(token_start, token_start + input.len()),
        value,
    })
}

#[derive(Debug)]
struct ParsedRequirementStyle {
    value: String,
    span: SourceSpan,
}

#[derive(Debug)]
struct ParsedRequirementStyles {
    items: Vec<ParsedRequirementStyle>,
}

impl ParsedRequirementStyles {
    fn values(&self) -> Vec<String> {
        self.items.iter().map(|item| item.value.clone()).collect()
    }
}

fn parse_requirement_styles(input: &str, input_start: usize) -> ParsedRequirementStyles {
    let mut items = Vec::new();
    let mut cursor = 0usize;
    for part in input.split_inclusive(',') {
        let raw = part.strip_suffix(',').unwrap_or(part);
        let leading = leading_whitespace_len(raw);
        let value = raw.trim();
        if !value.is_empty() {
            items.push(ParsedRequirementStyle {
                value: value.to_string(),
                span: SourceSpan::new(
                    input_start + cursor + leading,
                    input_start + cursor + leading + value.len(),
                ),
            });
        }
        cursor += part.len();
    }
    ParsedRequirementStyles { items }
}

struct ParsedRequirementClassShorthand {
    target: ParsedRequirementId,
    classes: ParsedRequirementIdList,
}

struct ParsedRequirementStyleStatement {
    ids: ParsedRequirementIdList,
    styles: ParsedRequirementStyles,
}

struct ParsedRequirementClassStatement {
    targets: ParsedRequirementIdList,
    classes: ParsedRequirementIdList,
}

struct ParsedRequirementRelationship {
    kind: String,
    kind_span: SourceSpan,
    source: ParsedRequirementId,
    target: ParsedRequirementId,
}

fn parse_shorthand_class_stmt(
    t: &str,
    statement_start: usize,
) -> Result<Option<ParsedRequirementClassShorthand>> {
    let t = t.trim();
    if t.is_empty() || t.ends_with('{') {
        return Ok(None);
    }
    let Some(pos) = t.find(":::") else {
        return Ok(None);
    };
    let left = t[..pos].trim_end();
    let right_raw = &t[pos + 3..];
    let right_leading = leading_whitespace_len(right_raw);
    let right = right_raw.trim_start();
    if left.is_empty() || right.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid class shorthand statement: {t}"),
        ));
    }
    let target = parse_requirement_id_spanned(left, statement_start)?;
    let classes = parse_id_list_all_spanned(right, statement_start + pos + 3 + right_leading)?;
    Ok(Some(ParsedRequirementClassShorthand { target, classes }))
}

fn parse_style_stmt(
    t: &str,
    statement_start: usize,
) -> Result<Option<ParsedRequirementStyleStatement>> {
    let t = t.trim_start();
    let Some((keyword, rest)) = split_first_word(t) else {
        return Ok(None);
    };
    if !keyword.eq_ignore_ascii_case("style") {
        return Ok(None);
    }
    let rest_leading = leading_whitespace_len(rest);
    let rest_start = t.len() - rest.len() + rest_leading;
    let rest = rest.trim_start();
    let (id_tokens, styles_str, styles_start) =
        split_list_and_rest_spanned(rest, statement_start + rest_start)?;
    let styles = parse_requirement_styles(styles_str, styles_start);
    if id_tokens.items.is_empty() || styles.items.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid style statement: {t}"),
        ));
    }
    Ok(Some(ParsedRequirementStyleStatement {
        ids: id_tokens,
        styles,
    }))
}

fn parse_classdef_stmt(
    t: &str,
    statement_start: usize,
) -> Result<Option<ParsedRequirementStyleStatement>> {
    let t = t.trim_start();
    let Some((keyword, rest)) = split_first_word(t) else {
        return Ok(None);
    };
    if !keyword.eq_ignore_ascii_case("classdef") {
        return Ok(None);
    }
    let rest_leading = leading_whitespace_len(rest);
    let rest_start = t.len() - rest.len() + rest_leading;
    let rest = rest.trim_start();
    let (id_tokens, styles_str, styles_start) =
        split_list_and_rest_spanned(rest, statement_start + rest_start)?;
    let styles = parse_requirement_styles(styles_str, styles_start);
    if id_tokens.items.is_empty() || styles.items.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid classDef statement: {t}"),
        ));
    }
    Ok(Some(ParsedRequirementStyleStatement {
        ids: id_tokens,
        styles,
    }))
}

fn parse_class_stmt(
    t: &str,
    statement_start: usize,
) -> Result<Option<ParsedRequirementClassStatement>> {
    let t = t.trim_start();
    let Some((keyword, rest)) = split_first_word(t) else {
        return Ok(None);
    };
    if !keyword.eq_ignore_ascii_case("class") {
        return Ok(None);
    }
    let rest_leading = leading_whitespace_len(rest);
    let rest_start = t.len() - rest.len() + rest_leading;
    let rest = rest.trim_start();
    let (id_tokens, classes_str, classes_start) =
        split_list_and_rest_spanned(rest, statement_start + rest_start)?;
    let class_tokens = parse_id_list_all_spanned(classes_str, classes_start)?;
    if id_tokens.items.is_empty() || class_tokens.items.is_empty() {
        return Err(Error::diagram_parse_fallback(
            "requirement".to_string(),
            format!("invalid class statement: {t}"),
        ));
    }
    Ok(Some(ParsedRequirementClassStatement {
        targets: id_tokens,
        classes: class_tokens,
    }))
}

fn parse_relationship_stmt(
    t: &str,
    statement_start: usize,
) -> Result<Option<ParsedRequirementRelationship>> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(None);
    }

    if let Some(pos) = t.find("<-") {
        let left = t[..pos].trim_end();
        let after_arrow = &t[pos + 2..];
        let rest_leading = leading_whitespace_len(after_arrow);
        let rest_start = pos + 2 + rest_leading;
        let rest = after_arrow.trim_start();
        let Some(dash) = rest.find('-') else {
            return Err(Error::diagram_parse_fallback(
                "requirement".to_string(),
                format!("invalid relationship statement: {rest}"),
            ));
        };
        let rel = rest[..dash].trim();
        let right_raw = &rest[dash + 1..];
        let right_leading = leading_whitespace_len(right_raw);
        let right = right_raw.trim();
        let rel_leading = leading_whitespace_len(&rest[..dash]);
        let rel_start = rest_start + rel_leading;
        let right_start = rest_start + dash + 1 + right_leading;
        let relationship = normalize_relationship(rel)?;
        let mut left_token = parse_requirement_id_spanned(left, statement_start)?;
        let mut right_token = parse_requirement_id_spanned(right, statement_start + right_start)?;
        left_token.value = left_token.value.trim().to_string();
        right_token.value = right_token.value.trim().to_string();
        if relationship.is_empty() {
            return Ok(None);
        }
        return Ok(Some(ParsedRequirementRelationship {
            kind: relationship,
            kind_span: SourceSpan::new(
                statement_start + rel_start,
                statement_start + rel_start + rel.len(),
            ),
            source: right_token,
            target: left_token,
        }));
    }

    if let Some(pos) = t.find("->") {
        let right_raw = &t[pos + 2..];
        let right_leading = leading_whitespace_len(right_raw);
        let right_start = pos + 2 + right_leading;
        let right = right_raw.trim();
        let left_part = t[..pos].trim_end();
        let Some(dash) = left_part.find('-') else {
            return Err(Error::diagram_parse_fallback(
                "requirement".to_string(),
                format!("invalid relationship statement: {left_part}"),
            ));
        };
        let src_raw = left_part[..dash].trim_end();
        let rel_raw = &left_part[dash + 1..];
        let rel_leading = leading_whitespace_len(rel_raw);
        let rel = rel_raw.trim();
        let rel_start = dash + 1 + rel_leading;
        let relationship = normalize_relationship(rel)?;
        let mut src_token = parse_requirement_id_spanned(src_raw, statement_start)?;
        let mut dst_token = parse_requirement_id_spanned(right, statement_start + right_start)?;
        src_token.value = src_token.value.trim().to_string();
        dst_token.value = dst_token.value.trim().to_string();
        if relationship.is_empty() {
            return Ok(None);
        }
        return Ok(Some(ParsedRequirementRelationship {
            kind: relationship,
            kind_span: SourceSpan::new(
                statement_start + rel_start,
                statement_start + rel_start + rel.len(),
            ),
            source: src_token,
            target: dst_token,
        }));
    }

    Ok(None)
}

fn normalize_relationship(input: &str) -> Result<String> {
    let rel = input.trim().to_ascii_lowercase();
    match rel.as_str() {
        "contains" | "copies" | "derives" | "satisfies" | "verifies" | "refines" | "traces" => {
            Ok(rel)
        }
        _ => Ok(String::new()),
    }
}

fn split_list_and_rest_spanned(
    input: &str,
    input_start: usize,
) -> Result<(ParsedRequirementIdList, &str, usize)> {
    let mut cur = input;
    let mut cursor = 0usize;
    let mut items = Vec::new();

    loop {
        let leading = leading_whitespace_len(cur);
        cursor += leading;
        cur = cur.trim_start();
        if cur.is_empty() {
            break;
        }

        let (item, rest) = if cur.starts_with('"') {
            let (value, rest) = parse_quoted_prefix(cur).ok_or_else(|| {
                Error::diagram_parse_fallback(
                    "requirement".to_string(),
                    "unterminated string".to_string(),
                )
            })?;
            let consumed = cur.len() - rest.len();
            (
                ParsedRequirementId {
                    value,
                    selection: SourceSpan::new(
                        input_start + cursor + 1,
                        input_start + cursor + consumed - 1,
                    ),
                },
                rest,
            )
        } else {
            let mut end = 0usize;
            for (i, c) in cur.char_indices() {
                if c == ',' || c.is_whitespace() {
                    break;
                }
                end = i + c.len_utf8();
            }
            if end == 0 {
                return Err(Error::diagram_parse_fallback(
                    "requirement".to_string(),
                    "expected identifier".to_string(),
                ));
            }
            (
                ParsedRequirementId {
                    value: cur[..end].to_string(),
                    selection: SourceSpan::new(input_start + cursor, input_start + cursor + end),
                },
                &cur[end..],
            )
        };

        items.push(item);
        cursor += cur.len() - rest.len();
        cur = rest;
        let separator_leading = leading_whitespace_len(cur);
        cursor += separator_leading;
        cur = cur.trim_start();
        if cur.starts_with(',') {
            cursor += 1;
            cur = &cur[1..];
            continue;
        }
        break;
    }

    let rest_leading = leading_whitespace_len(cur);
    cursor += rest_leading;
    let rest = cur.trim_start();
    Ok((
        ParsedRequirementIdList { items },
        rest,
        input_start + cursor,
    ))
}

#[derive(Debug, Clone)]
struct ParsedRequirementId {
    value: String,
    selection: SourceSpan,
}

#[derive(Debug, Clone)]
struct ParsedRequirementIdList {
    items: Vec<ParsedRequirementId>,
}

impl ParsedRequirementIdList {
    fn values(&self) -> Vec<String> {
        self.items.iter().map(|item| item.value.clone()).collect()
    }
}

fn parse_id_list_all_spanned(input: &str, input_start: usize) -> Result<ParsedRequirementIdList> {
    let mut out = Vec::new();
    let mut cur = input.trim_start();
    let mut cursor = input.len() - cur.len();
    while !cur.is_empty() {
        let leading = cur.len() - cur.trim_start().len();
        cursor += leading;
        cur = cur.trim_start();
        let (item, rest, selection) = if cur.starts_with('"') {
            let (item, rest) = parse_quoted_prefix(cur).ok_or_else(|| {
                Error::diagram_parse_fallback(
                    "requirement".to_string(),
                    "unterminated string".to_string(),
                )
            })?;
            let consumed = cur.len() - rest.len();
            (
                item,
                rest,
                SourceSpan::new(
                    input_start + cursor + 1,
                    input_start + cursor + consumed - 1,
                ),
            )
        } else {
            let mut end = cur.len();
            for (i, c) in cur.char_indices() {
                if c == ',' {
                    end = i;
                    break;
                }
            }
            let raw = &cur[..end];
            let item_leading = leading_whitespace_len(raw);
            let item = raw.trim().to_string();
            let selection = SourceSpan::new(
                input_start + cursor + item_leading,
                input_start + cursor + item_leading + item.len(),
            );
            (item, &cur[end..], selection)
        };
        if !item.is_empty() {
            out.push(ParsedRequirementId {
                value: item,
                selection,
            });
        }
        cursor += cur.len() - rest.len();
        cur = rest;
        let separator_leading = cur.len() - cur.trim_start().len();
        cursor += separator_leading;
        cur = cur.trim_start();
        if cur.starts_with(',') {
            cursor += 1;
            cur = &cur[1..];
            continue;
        }
        break;
    }
    Ok(ParsedRequirementIdList { items: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, Engine, MermaidConfig, ParseDiagnosticSpanKind, ParseOptions,
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
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap_err()
            .to_string()
    }

    fn test_meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "requirement".to_string(),
            config: MermaidConfig::default(),
            effective_config: MermaidConfig::default(),
            title: None,
        }
    }

    fn payload_selection(facts: &EditorSemanticFacts, detail: &str, name: &str) -> SourceSpan {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.detail.as_deref() == Some(detail) && symbol.name == name)
            .unwrap_or_else(|| panic!("missing payload symbol {detail:?} {name:?}"))
            .selection
    }

    fn statement_occurrences(source: &str, statement: &str, needle: &str) -> Vec<SourceSpan> {
        let statement_start = source
            .find(statement)
            .unwrap_or_else(|| panic!("missing statement {statement:?}"));
        statement
            .match_indices(needle)
            .map(|(start, value)| {
                SourceSpan::new(
                    statement_start + start,
                    statement_start + start + value.len(),
                )
            })
            .collect()
    }

    fn statement_symbol_selections(
        facts: &EditorSemanticFacts,
        source: &str,
        statement: &str,
        detail: &str,
    ) -> Vec<SourceSpan> {
        let statement_start = source
            .find(statement)
            .unwrap_or_else(|| panic!("missing statement {statement:?}"));
        let statement_span = SourceSpan::new(statement_start, statement_start + statement.len());
        let mut selections = facts
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.detail.as_deref() == Some(detail)
                    && symbol.selection.start >= statement_span.start
                    && symbol.selection.end <= statement_span.end
            })
            .map(|symbol| symbol.selection)
            .collect::<Vec<_>>();
        selections.sort_by_key(|span| (span.start, span.end));
        selections
    }

    #[test]
    fn requirement_semantic_symbols_use_parser_spans_when_values_repeat() {
        let source = concat!(
            "requirementDiagram\n",
            "requirement same:::same,same {\n",
            "}\n",
            "short:::short,short\n",
            "style paint,paint paint:paint,paint:paint\n",
            "classDef defc,defc defc:defc,defc:defc\n",
            "class cls,cls cls,cls\n",
            "verifies - verifies -> verifies\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_requirement_json_and_editor_facts,
            source,
            &test_meta(),
        );

        let definition = "requirement same:::same,same {";
        let definition_same = statement_occurrences(source, definition, "same");
        assert_eq!(
            statement_symbol_selections(&facts, source, definition, "requirement"),
            vec![definition_same[0]],
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, definition, "requirement class"),
            definition_same[1..].to_vec(),
        );

        let shorthand = "short:::short,short";
        let shorthand_same = statement_occurrences(source, shorthand, "short");
        assert_eq!(
            statement_symbol_selections(&facts, source, shorthand, "requirement class target"),
            vec![shorthand_same[0]],
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, shorthand, "requirement class"),
            shorthand_same[1..].to_vec(),
        );

        let style = "style paint,paint paint:paint,paint:paint";
        let style_same = statement_occurrences(source, style, "paint");
        assert_eq!(
            statement_symbol_selections(&facts, source, style, "requirement style target"),
            style_same[..2].to_vec(),
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, style, "requirement style"),
            statement_occurrences(source, style, "paint:paint"),
        );

        let class_def = "classDef defc,defc defc:defc,defc:defc";
        let class_def_same = statement_occurrences(source, class_def, "defc");
        assert_eq!(
            statement_symbol_selections(&facts, source, class_def, "requirement class definition",),
            class_def_same[..2].to_vec(),
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, class_def, "requirement class style",),
            statement_occurrences(source, class_def, "defc:defc"),
        );

        let class = "class cls,cls cls,cls";
        let class_same = statement_occurrences(source, class, "cls");
        assert_eq!(
            statement_symbol_selections(&facts, source, class, "requirement class target"),
            class_same[..2].to_vec(),
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, class, "requirement class"),
            class_same[2..].to_vec(),
        );

        let relationship = "verifies - verifies -> verifies";
        let verifies = statement_occurrences(source, relationship, "verifies");
        assert_eq!(
            statement_symbol_selections(
                &facts,
                source,
                relationship,
                "requirement relationship source",
            ),
            vec![verifies[0]],
        );
        assert_eq!(
            statement_symbol_selections(&facts, source, relationship, "requirement relationship",),
            vec![verifies[1]],
        );
        assert_eq!(
            statement_symbol_selections(
                &facts,
                source,
                relationship,
                "requirement relationship target",
            ),
            vec![verifies[2]],
        );
    }

    #[test]
    fn requirement_targets_use_reference_roles_and_resolve_forward_definition_kinds() {
        let source = concat!(
            "requirementDiagram\n",
            "future - verifies -> later\n",
            "style future,later fill:#fff\n",
            "class future,later important\n",
            "future:::important\n",
            "element future {\n",
            "}\n",
            "requirement later {\n",
            "}\n",
            "classDef important fill:#fff\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_requirement_json_and_editor_facts,
            source,
            &test_meta(),
        );
        let symbols_for = |name: &str| {
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == name)
                .collect::<Vec<_>>()
        };

        let future = symbols_for("future");
        assert_eq!(future.len(), 5);
        assert!(
            future[..4]
                .iter()
                .all(|symbol| symbol.role == EditorSemanticRole::Reference)
        );
        assert!(
            future
                .iter()
                .all(|symbol| symbol.kind == EditorSemanticKind::Object)
        );
        assert_eq!(future[4].role, EditorSemanticRole::Entity);

        let later = symbols_for("later");
        assert_eq!(later.len(), 4);
        assert!(
            later[..3]
                .iter()
                .all(|symbol| symbol.role == EditorSemanticRole::Reference)
        );
        assert!(
            later
                .iter()
                .all(|symbol| symbol.kind == EditorSemanticKind::Struct)
        );
        assert_eq!(later[3].role, EditorSemanticRole::Entity);
    }

    #[test]
    fn requirement_reference_kind_resolution_does_not_inspect_display_detail() {
        let mut db = RequirementDb::new();
        db.add_element("node", ElementBuilder::new());
        let span = SourceSpan::new(0, "node".len());
        let mut facts = EditorSemanticFacts::new();
        facts.push_symbol(EditorSemanticSymbol::reference(
            "node",
            Some("definition-looking display text".to_string()),
            EditorSemanticKind::Struct,
            span,
            span,
        ));

        resolve_requirement_reference_kinds(&mut facts, &db);

        assert_eq!(facts.symbols[0].role, EditorSemanticRole::Reference);
        assert_eq!(facts.symbols[0].kind, EditorSemanticKind::Object);
    }

    #[test]
    fn requirement_entrypoints_construct_one_semantic_source() {
        let engine = Engine::new();
        let text = concat!(
            "requirementDiagram\n",
            "requirement login {\n",
            "  id: REQ-1\n",
            "  risk: high\n",
            "  verifymethod: test\n",
            "}\n",
        );

        reset_requirement_syntax_construction_count();
        engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(requirement_syntax_construction_count(), 1);

        reset_requirement_syntax_construction_count();
        engine
            .parse_diagram_for_render_model_sync(text, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(requirement_syntax_construction_count(), 1);

        reset_requirement_syntax_construction_count();
        engine
            .parse_editor_semantic_facts_with_type_sync("requirement", text)
            .unwrap()
            .unwrap();
        assert_eq!(requirement_syntax_construction_count(), 1);
    }

    #[test]
    fn requirement_combined_projection_constructs_once_and_matches_standalone_entrypoints() {
        let text = concat!(
            "requirementDiagram\n",
            "accTitle: Login requirements\n",
            "direction LR\n",
            "requirement login:::critical {\n",
            "  id: REQ-1\n",
            "  text: Login must work\n",
            "  risk: high\n",
            "  verifymethod: test\n",
            "}\n",
            "element api {\n",
            "  type: service\n",
            "}\n",
            "classDef critical fill:#f9f\n",
            "login - verifies -> api\n",
        );
        let mut meta = test_meta();
        meta.effective_config = MermaidConfig::from_value(json!({
            "theme": "forest",
            "securityLevel": "strict"
        }));
        let standalone_json = parse_requirement(text, &meta).unwrap();
        reset_requirement_syntax_construction_count();
        let (combined_json, combined_editor) = crate::family::test_support::into_result(
            parse_requirement_json_and_editor_facts(text, &meta, &OperationControl::new()),
        )
        .unwrap();

        assert_eq!(requirement_syntax_construction_count(), 1);
        assert_eq!(combined_json, standalone_json);
        assert!(!combined_editor.symbols.is_empty());
    }

    #[test]
    fn requirement_typed_projection_matches_compatibility_json() {
        let text = concat!(
            "requirementDiagram\n",
            "accTitle: Login requirements\n",
            "direction LR\n",
            "requirement login {\n",
            "  id: REQ-1\n",
            "  text: Login must work\n",
            "  risk: high\n",
            "  verifymethod: test\n",
            "}\n",
            "element api {\n",
            "  type: service\n",
            "}\n",
            "login - verifies -> api\n",
        );
        let meta = test_meta();
        let compat = parse_requirement(text, &meta).unwrap();
        let typed = parse_requirement_model_for_render(text, &meta).unwrap();

        assert_eq!(render_model_to_compat_json(&typed, &meta).unwrap(), compat);
        assert_eq!(compat["type"], "requirement");
        assert_eq!(compat["config"], meta.effective_config.as_value().clone());
        assert!(compat["accDescr"].is_null());
    }

    #[test]
    fn requirement_malformed_recovery_reuses_partial_statement_facts_and_exact_span() {
        let engine = Engine::new();
        let text = concat!(
            "requirementDiagram\n",
            "requirement login {\n",
            "  id: REQ-1\n",
            "  risk: impossible\n",
            "}\n",
        );
        let bad_value_start = text.find("impossible").unwrap();

        reset_requirement_syntax_construction_count();
        let error = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect_err("invalid risk must fail strict parsing");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("invalid risk returned a non-parse error");
        };
        assert_eq!(requirement_syntax_construction_count(), 1);
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(
                bad_value_start,
                bad_value_start + "impossible".len(),
            ))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);

        reset_requirement_syntax_construction_count();
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("requirement", text)
            .unwrap()
            .unwrap();
        assert_eq!(requirement_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "login"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "REQ-1"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.span
                == Some(SourceSpan::new(
                    bad_value_start,
                    bad_value_start + "impossible".len(),
                ))
        }));
    }

    #[test]
    fn requirement_full_requirement_definition_is_parsed() {
        let model = parse(
            r#"requirementDiagram

requirement test_req {
  id: test_id
  text: the test text.
  risk: high
  verifymethod: analysis
}
"#,
        );

        assert_eq!(model["requirements"].as_array().unwrap().len(), 1);
        assert_eq!(model["requirements"][0]["name"], json!("test_req"));
        assert_eq!(model["requirements"][0]["type"], json!("Requirement"));
        assert_eq!(model["requirements"][0]["requirementId"], json!("test_id"));
        assert_eq!(model["requirements"][0]["text"], json!("the test text."));
        assert_eq!(model["requirements"][0]["risk"], json!("High"));
        assert_eq!(model["requirements"][0]["verifyMethod"], json!("Analysis"));
        assert_eq!(model["elements"].as_array().unwrap().len(), 0);
        assert_eq!(model["relationships"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn requirement_full_element_definition_is_parsed() {
        let model = parse(
            r#"requirementDiagram

element test_el {
  type: test_type
  docref: test_ref
}
"#,
        );

        assert_eq!(model["requirements"].as_array().unwrap().len(), 0);
        assert_eq!(model["elements"].as_array().unwrap().len(), 1);
        assert_eq!(model["elements"][0]["name"], json!("test_el"));
        assert_eq!(model["elements"][0]["type"], json!("test_type"));
        assert_eq!(model["elements"][0]["docRef"], json!("test_ref"));
        assert_eq!(model["relationships"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn requirement_editor_payload_spans_point_to_values_when_values_match_keys() {
        let text = r#"requirementDiagram
accTitle: accTitle
accDescr: accDescr
requirement req {
  id: id
  text: text
  risk: risk
  verifymethod: verifymethod
}
element el {
  type: type
  docref: docref
}
"#;
        let facts = crate::family::test_support::editor_facts(
            parse_requirement_json_and_editor_facts,
            text,
            &test_meta(),
        );

        for (detail, name, needle) in [
            (
                "requirement accessibility title",
                "accTitle",
                "accTitle: accTitle",
            ),
            (
                "requirement accessibility description",
                "accDescr",
                "accDescr: accDescr",
            ),
            ("requirement id", "id", "id: id"),
            ("requirement text", "text", "text: text"),
            ("requirement risk", "risk", "risk: risk"),
            (
                "requirement verify method",
                "verifymethod",
                "verifymethod: verifymethod",
            ),
            ("requirement element type", "type", "type: type"),
            ("requirement doc ref", "docref", "docref: docref"),
        ] {
            let value_start = text.find(needle).unwrap() + needle.rfind(name).unwrap();
            assert_eq!(
                payload_selection(&facts, detail, name),
                SourceSpan::new(value_start, value_start + name.len()),
                "wrong span for {detail}"
            );
        }
    }

    #[test]
    fn requirement_definition_spans_point_to_name_tokens_when_names_prefix_keywords() {
        let text = concat!(
            "requirementDiagram\n",
            "  requirement require {\n",
            "  }\n",
            "  element elem {\n",
            "  }\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_requirement_json_and_editor_facts,
            text,
            &test_meta(),
        );

        for (detail, name, declaration) in [
            ("requirement", "require", "  requirement require {"),
            ("requirement element", "elem", "  element elem {"),
        ] {
            let declaration_start = text.find(declaration).unwrap();
            let name_start = declaration_start + declaration.rfind(name).unwrap();
            let expected = SourceSpan::new(name_start, name_start + name.len());
            let symbol = facts
                .symbols
                .iter()
                .find(|symbol| symbol.detail.as_deref() == Some(detail) && symbol.name == name)
                .unwrap_or_else(|| panic!("missing definition symbol {detail:?} {name:?}"));

            assert_eq!(symbol.role, EditorSemanticRole::Entity);
            assert_eq!(symbol.rename_policy, crate::EditorRenamePolicy::Identifier);
            assert_eq!(symbol.selection, expected);
            assert_eq!(&text[symbol.selection.start..symbol.selection.end], name);
            assert!(facts.expected_syntax.iter().any(|syntax| {
                syntax.kind == EditorExpectedSyntaxKind::NodeIdentifier && syntax.span == expected
            }));
        }
    }

    #[test]
    fn requirement_acc_title_and_acc_descr_are_parsed() {
        let model = parse(
            r#"requirementDiagram
accTitle: test title
accDescr: my chart description
element test_name {
  type: test_type
  docref: test_ref
}
"#,
        );

        assert_eq!(model["accTitle"], json!("test title"));
        assert_eq!(model["accDescr"], json!("my chart description"));
    }

    #[test]
    fn requirement_multiline_acc_descr_is_parsed() {
        let model = parse(
            r#"requirementDiagram
accTitle: test title
accDescr {
  my chart description
line 2
}
element test_name {
  type: test_type
  docref: test_ref
}
"#,
        );

        assert_eq!(model["accTitle"], json!("test title"));
        assert_eq!(model["accDescr"], json!("my chart description\nline 2"));
    }

    #[test]
    fn requirement_relationship_is_parsed() {
        let model = parse(
            r#"requirementDiagram

a - contains -> b
"#,
        );
        assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
        assert_eq!(model["relationships"][0]["type"], json!("contains"));
        assert_eq!(model["relationships"][0]["src"], json!("a"));
        assert_eq!(model["relationships"][0]["dst"], json!("b"));
    }

    #[test]
    fn requirement_relationship_left_arrow_is_parsed() {
        let model = parse(
            r#"requirementDiagram

a <- contains - b
"#,
        );
        assert_eq!(model["relationships"].as_array().unwrap().len(), 1);
        assert_eq!(model["relationships"][0]["type"], json!("contains"));
        assert_eq!(model["relationships"][0]["src"], json!("b"));
        assert_eq!(model["relationships"][0]["dst"], json!("a"));
    }

    #[test]
    fn requirement_proto_and_constructor_ids_are_accepted() {
        for id in ["__proto__", "constructor"] {
            let model = parse(&format!(
                r#"requirementDiagram
requirement {id} {{
  id: 1
  text: the test text.
  risk: high
  verifymethod: test
}}
"#
            ));
            assert_eq!(model["requirements"].as_array().unwrap().len(), 1);
        }

        for id in ["__proto__", "constructor"] {
            let model = parse(&format!(
                r#"requirementDiagram
element {id} {{
  type: simulation
}}
"#
            ));
            assert_eq!(model["elements"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn requirement_style_statement_applies_to_requirement() {
        let model = parse(
            r#"requirementDiagram

requirement test_req {
}
style test_req fill:#f9f,stroke:#333,stroke-width:4px
"#,
        );

        assert_eq!(
            model["requirements"][0]["cssStyles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
    }

    #[test]
    fn requirement_style_statement_applies_to_element() {
        let model = parse(
            r#"requirementDiagram

element test_element {
}
style test_element fill:#f9f,stroke:#333,stroke-width:4px
"#,
        );

        assert_eq!(
            model["elements"][0]["cssStyles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
    }

    #[test]
    fn requirement_style_statement_applies_to_multiple_things() {
        let model = parse(
            r#"requirementDiagram

requirement test_requirement {
}
element test_element {
}
style test_requirement,test_element fill:#f9f,stroke:#333,stroke-width:4px
"#,
        );

        assert_eq!(
            model["requirements"][0]["cssStyles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
        assert_eq!(
            model["elements"][0]["cssStyles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
    }

    #[test]
    fn requirement_classdef_and_class_statement_are_parsed() {
        let model = parse(
            r#"requirementDiagram

requirement myReq {
}
classDef myClass fill:#f9f,stroke:#333,stroke-width:4px
class myReq myClass
"#,
        );

        assert_eq!(
            model["requirements"][0]["classes"],
            json!(["default", "myClass"])
        );
        assert_eq!(
            model["requirements"][0]["cssStyles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
        assert_eq!(model["classes"]["myClass"]["id"], json!("myClass"));
        assert_eq!(
            model["classes"]["myClass"]["styles"],
            json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
        );
        assert_eq!(model["classes"]["myClass"]["textStyles"], json!([]));
    }

    #[test]
    fn requirement_shorthand_class_statement_is_supported() {
        let model = parse(
            r#"requirementDiagram

requirement myReq {
}
classDef myClass fill:#f9f,stroke:#333,stroke-width:4px
myReq:::myClass
"#,
        );
        assert_eq!(
            model["requirements"][0]["classes"],
            json!(["default", "myClass"])
        );
    }

    #[test]
    fn requirement_shorthand_is_supported_in_definition() {
        let model = parse(
            r#"requirementDiagram

requirement myReq:::class1 {
}
element myElem:::class1,class2 {
}

classDef class1 fill:#f9f,stroke:#333,stroke-width:4px
classDef class2 color:blue
"#,
        );
        assert_eq!(
            model["requirements"][0]["classes"],
            json!(["default", "class1"])
        );
        assert_eq!(
            model["elements"][0]["classes"],
            json!(["default", "class1", "class2"])
        );
    }

    #[test]
    fn requirement_direction_is_parsed() {
        for dir in ["TB", "BT", "LR", "RL"] {
            let model = parse(&format!("requirementDiagram\n\ndirection {dir}\n"));
            assert_eq!(model["direction"], json!(dir));
        }
    }

    #[test]
    fn requirement_top_level_directives_require_exact_first_word() {
        for statement in [
            "styleNode fill:#f00",
            "classify myReq myClass",
            "foo direction LR",
        ] {
            let message = parse_err(&format!("requirementDiagram\n{statement}\n"));
            assert!(
                message.contains("unexpected requirement statement"),
                "unexpected error for {statement:?}: {message}"
            );
        }
    }
}

use crate::diagrams::scan::{consume_line_ending, is_ecmascript_whitespace};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, OperationControl, OperationControlResult, ParseMetadata, Result,
    SourceSpan,
    editor::{
        format_lalrpop_parse_error, has_ascii_separator, lalrpop_parse_diagnostic,
        lalrpop_recovery_span, line_content_end, source_value_span, trailing_ascii_whitespace_slot,
    },
};
use indexmap::IndexMap;
use serde_json::{Value, json};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};

#[cfg(test)]
thread_local! {
    static ER_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_er_syntax_construction_count() {
    ER_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn er_syntax_construction_count() -> usize {
    ER_SYNTAX_CONSTRUCTION_COUNT.get()
}

include_checked_in_lalrpop_parser!(
    #[allow(clippy::empty_line_after_outer_attr)]
    er_grammar,
    "er_grammar.rs"
);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErDiagramRenderModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    pub direction: String,
    #[serde(default)]
    pub classes: IndexMap<String, ErClassDefRenderModel>,
    #[serde(default)]
    pub entities: IndexMap<String, ErEntityRenderModel>,
    #[serde(default)]
    pub relationships: Vec<ErRelationshipRenderModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subgraphs: Vec<ErSubgraphRenderModel>,
}

impl ErDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
        for subgraph in &mut self.subgraphs {
            subgraph.title = crate::sanitize::sanitize_text(&subgraph.title, config);
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErAttributeRenderModel {
    #[serde(rename = "type")]
    pub ty: String,
    pub name: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErRelSpecRenderModel {
    #[serde(rename = "cardA")]
    pub card_a: String,
    #[serde(rename = "cardB")]
    pub card_b: String,
    #[serde(rename = "relType")]
    pub rel_type: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErRelationshipRenderModel {
    #[serde(rename = "entityA")]
    pub entity_a: String,
    #[serde(default, rename = "roleA")]
    pub role_a: String,
    #[serde(rename = "entityB")]
    pub entity_b: String,
    #[serde(default, rename = "relSpec")]
    pub rel_spec: ErRelSpecRenderModel,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErClassDefRenderModel {
    pub id: String,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default, rename = "textStyles")]
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErSubgraphRenderModel {
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default, rename = "cssStyles")]
    pub css_styles: Vec<String>,
    #[serde(default)]
    pub dir: Option<String>,
    pub id: String,
    #[serde(default, rename = "labelType")]
    pub label_type: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    pub title: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErEntityRenderModel {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub attributes: Vec<ErAttributeRenderModel>,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub shape: String,
    #[serde(default, rename = "cssClasses")]
    pub css_classes: String,
    #[serde(default, rename = "cssStyles")]
    pub css_styles: Vec<String>,
}

pub(crate) type Attribute = ErAttributeRenderModel;
pub(crate) type RelSpec = ErRelSpecRenderModel;
type Relationship = ErRelationshipRenderModel;
type EntityClass = ErClassDefRenderModel;
type EntityNode = ErEntityRenderModel;

#[derive(Debug, Clone)]
enum Action {
    AddEntity {
        name: String,
        alias: Option<String>,
    },
    AddAttributes {
        entity: String,
        attributes: Vec<Attribute>,
    },
    AddRelationship {
        a: String,
        role: String,
        b: String,
        spec: RelSpec,
    },
    SetClass {
        entities: Vec<String>,
        classes: Vec<String>,
    },
    AddClassDef {
        classes: Vec<String>,
        raw: String,
    },
    AddCssStyles {
        entities: Vec<String>,
        raw: String,
    },
    AddSubgraph {
        id: String,
        title: String,
        label_type: String,
        body: ErActionList,
    },
    SetDirection(String),
    SetAccTitle(String),
    SetAccDescr(String),
}

// Grammar symbols carry scalar links; the arena owns every action exactly once.
#[derive(Debug, Clone, Copy, Default)]
struct ErActionList {
    first: Option<usize>,
    last: Option<usize>,
}

#[derive(Debug)]
struct ErActionRecord {
    action: Option<Action>,
    next: Option<usize>,
}

#[derive(Debug, Default)]
struct ErActionArena {
    records: Vec<ErActionRecord>,
}

impl ErActionArena {
    fn push(&mut self, action: Action) -> ErActionList {
        let index = self.records.len();
        self.records.push(ErActionRecord {
            action: Some(action),
            next: None,
        });
        ErActionList {
            first: Some(index),
            last: Some(index),
        }
    }

    fn actions(&mut self, actions: Vec<Action>) -> ErActionList {
        let mut list = ErActionList::default();
        for action in actions {
            let item = self.push(action);
            list = self.append(list, item);
        }
        list
    }

    fn append(&mut self, left: ErActionList, right: ErActionList) -> ErActionList {
        let (Some(last), Some(first)) = (left.last, right.first) else {
            return if left.first.is_some() { left } else { right };
        };
        self.records[last].next = Some(first);
        ErActionList {
            first: left.first,
            last: right.last,
        }
    }
}

struct ErSubgraphCompletion {
    id: String,
    title: String,
    label_type: String,
    members: Vec<String>,
    direction: Option<String>,
}

struct ErReplayFrame {
    next: Option<usize>,
    subgraph: Option<ErSubgraphCompletion>,
}

#[derive(Debug, Clone)]
struct SpannedId {
    name: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct SpannedIdList {
    ids: Vec<SpannedId>,
}

impl SpannedIdList {
    fn into_names(self) -> Vec<String> {
        self.ids.into_iter().map(|id| id.name).collect()
    }

    fn span(&self) -> SourceSpan {
        let start = self.ids.first().map(|id| id.span.start).unwrap_or(0);
        let end = self.ids.last().map(|id| id.span.end).unwrap_or(start);
        SourceSpan::new(start, end)
    }
}

#[derive(Debug, Default)]
struct ErDb {
    entities: IndexMap<String, EntityNode>,
    relationships: Vec<Relationship>,
    classes: IndexMap<String, EntityClass>,
    direction: String,
    entity_counter: usize,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    subgraphs: Vec<ErSubgraphRenderModel>,
    subgraph_lookup: IndexMap<String, usize>,
    subgraph_counter: usize,
    completed_members: HashSet<String>,
    #[cfg(test)]
    membership_candidates: usize,
}

impl ErDb {
    fn new() -> Self {
        Self {
            direction: "TB".to_string(),
            ..Default::default()
        }
    }

    fn add_entity(&mut self, name: &str, alias: Option<&str>) {
        let Some(existing) = self.entities.get_mut(name) else {
            let id = format!("entity-{name}-{}", self.entity_counter);
            self.entity_counter += 1;
            self.entities.insert(
                name.to_string(),
                EntityNode {
                    id,
                    label: name.to_string(),
                    attributes: Vec::new(),
                    alias: alias.unwrap_or("").to_string(),
                    shape: "erBox".to_string(),
                    css_classes: "default".to_string(),
                    css_styles: Vec::new(),
                },
            );
            return;
        };

        if existing.alias.is_empty()
            && let Some(a) = alias
            && !a.is_empty()
        {
            existing.alias = a.to_string();
        }
    }

    fn add_attributes(&mut self, entity: &str, attributes: Vec<Attribute>) {
        self.add_entity(entity, None);
        let Some(e) = self.entities.get_mut(entity) else {
            return;
        };
        for a in attributes {
            e.attributes.push(a);
        }
    }

    fn add_relationship(&mut self, a: &str, role: &str, b: &str, spec: RelSpec) {
        let entity_a = if self.subgraph_lookup.contains_key(a) {
            a.to_string()
        } else {
            self.add_entity(a, None);
            let Some(entity) = self.entities.get(a) else {
                return;
            };
            entity.id.clone()
        };
        let entity_b = if self.subgraph_lookup.contains_key(b) {
            b.to_string()
        } else {
            self.add_entity(b, None);
            let Some(entity) = self.entities.get(b) else {
                return;
            };
            entity.id.clone()
        };
        self.relationships.push(Relationship {
            entity_a,
            role_a: role.to_string(),
            entity_b,
            rel_spec: spec,
        });
    }

    fn set_class(&mut self, entities: &[String], classes: &[String]) {
        for e in entities {
            if let Some(node) = self.entities.get_mut(e) {
                for cls in classes {
                    node.css_classes.push(' ');
                    node.css_classes.push_str(cls);
                }
            }
            if let Some(index) = self.subgraph_lookup.get(e).copied()
                && let Some(subgraph) = self.subgraphs.get_mut(index)
            {
                subgraph.classes.extend(classes.iter().cloned());
            }
        }
    }

    fn add_class_def(&mut self, classes: &[String], styles: &[String]) {
        for id in classes {
            let entry = self
                .classes
                .entry(id.to_string())
                .or_insert_with(|| EntityClass {
                    id: id.to_string(),
                    ..Default::default()
                });

            for s in styles {
                if s.contains("color") {
                    entry.text_styles.push(s.replace("fill", "bgFill"));
                }
                entry.styles.push(s.to_string());
            }
        }
    }

    fn add_css_styles(&mut self, entities: &[String], styles: &[String]) {
        for id in entities {
            if let Some(entity) = self.entities.get_mut(id) {
                for style in styles {
                    entity.css_styles.push(style.to_string());
                }
            }
            if let Some(index) = self.subgraph_lookup.get(id).copied()
                && let Some(subgraph) = self.subgraphs.get_mut(index)
            {
                subgraph.css_styles.extend(styles.iter().cloned());
            }
        }
    }

    fn complete_subgraph(
        &mut self,
        completion: ErSubgraphCompletion,
        control: &OperationControl,
    ) -> OperationControlResult<String> {
        let ErSubgraphCompletion {
            id: raw_id,
            title,
            label_type,
            members,
            direction,
        } = completion;
        let mut seen = HashSet::new();
        let mut retained = Vec::new();
        for (index, member) in members.into_iter().enumerate() {
            #[cfg(test)]
            {
                self.membership_candidates += 1;
            }
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let trimmed = member.trim();
            if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
                continue;
            }
            // ER appends repeated IDs and keeps the union of all completed memberships.
            if self.completed_members.insert(member.clone()) {
                retained.push(member);
            }
        }

        let id = if raw_id.trim().is_empty() {
            format!("subGraph{}", self.subgraph_counter)
        } else {
            raw_id.trim().to_string()
        };
        self.subgraph_counter = self.subgraph_counter.saturating_add(1);
        let subgraph = ErSubgraphRenderModel {
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir: direction,
            id: id.clone(),
            label_type: match label_type.as_str() {
                "markdown" | "string" | "text" => label_type,
                _ => "markdown".to_string(),
            },
            nodes: retained,
            title: title.trim().to_string(),
        };
        let index = self.subgraphs.len();
        self.subgraphs.push(subgraph);
        self.subgraph_lookup.insert(id.clone(), index);
        Ok(id)
    }

    fn replay(
        &mut self,
        mut arena: ErActionArena,
        actions: ErActionList,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let mut frames = vec![ErReplayFrame {
            next: actions.first,
            subgraph: None,
        }];
        let mut replayed = 0usize;
        while let Some(frame) = frames.last_mut() {
            if replayed.is_multiple_of(128) {
                control.checkpoint()?;
            }
            replayed = replayed.saturating_add(1);
            let Some(index) = frame.next else {
                let frame = frames.pop().expect("the current replay frame exists");
                if let Some(completion) = frame.subgraph {
                    let id = self.complete_subgraph(completion, control)?;
                    if let Some(parent) =
                        frames.last_mut().and_then(|frame| frame.subgraph.as_mut())
                    {
                        parent.members.push(id);
                    }
                }
                continue;
            };
            let record = &mut arena.records[index];
            frame.next = record.next;
            let action = record.action.take().expect("an action is replayed once");
            match action {
                Action::AddEntity { name, alias } => {
                    if let Some(subgraph) = &mut frame.subgraph {
                        subgraph.members.push(name.clone());
                    }
                    self.add_entity(&name, alias.as_deref());
                }
                Action::AddAttributes { entity, attributes } => {
                    if let Some(subgraph) = &mut frame.subgraph {
                        subgraph.members.push(entity.clone());
                    }
                    self.add_attributes(&entity, attributes);
                }
                Action::AddRelationship { a, role, b, spec } => {
                    if let Some(subgraph) = &mut frame.subgraph {
                        subgraph.members.push(a.clone());
                        subgraph.members.push(b.clone());
                    }
                    self.add_relationship(&a, &role, &b, spec);
                }
                Action::SetClass { entities, classes } => self.set_class(&entities, &classes),
                Action::AddClassDef { classes, raw } => {
                    let styles = split_styles(&raw);
                    self.add_class_def(&classes, &styles);
                }
                Action::AddCssStyles { entities, raw } => {
                    let styles = split_styles(&raw);
                    self.add_css_styles(&entities, &styles);
                }
                Action::AddSubgraph {
                    id,
                    title,
                    label_type,
                    body,
                } => frames.push(ErReplayFrame {
                    next: body.first,
                    subgraph: Some(ErSubgraphCompletion {
                        id,
                        title,
                        label_type,
                        members: Vec::new(),
                        direction: None,
                    }),
                }),
                Action::SetDirection(dir) => {
                    if let Some(subgraph) = &mut frame.subgraph {
                        subgraph.direction = Some(dir);
                    } else {
                        self.direction = dir;
                    }
                }
                Action::SetAccTitle(t) => {
                    self.acc_title = Some(t.trim_matches(is_ecmascript_whitespace).to_string());
                }
                Action::SetAccDescr(t) => {
                    // Mermaid's commonDb.ts: `sanitizeText(txt).replace(/\n\s+/g, '\n')`
                    let trimmed = t.trim_matches(is_ecmascript_whitespace);
                    let mut out = String::with_capacity(trimmed.len());
                    let mut chars = trimmed.chars().peekable();
                    while let Some(ch) = chars.next() {
                        out.push(ch);
                        if ch == '\n' {
                            while chars.peek().is_some_and(|c| is_ecmascript_whitespace(*c)) {
                                chars.next();
                            }
                        }
                    }
                    self.acc_descr = Some(out);
                }
            }
        }
        control.checkpoint()?;
        Ok(())
    }

    fn into_render_model(self) -> ErDiagramRenderModel {
        ErDiagramRenderModel {
            acc_title: self.acc_title,
            acc_descr: self.acc_descr,
            direction: self.direction,
            classes: self.classes,
            entities: self.entities,
            relationships: self.relationships,
            subgraphs: self.subgraphs,
        }
    }

    fn into_model(self, meta: &ParseMetadata) -> Result<Value> {
        let model = self.into_render_model();
        render_model_to_compat_json(&model, meta)
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &ErDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut value = serde_json::to_value(model)
        .map_err(|e| Error::diagram_parse_fallback(meta.diagram_type.clone(), e.to_string()))?;
    let Value::Object(obj) = &mut value else {
        return Ok(value);
    };

    obj.insert("type".to_string(), json!(meta.diagram_type));
    obj.insert(
        "constants".to_string(),
        json!({
            "cardinality": {
                "zeroOrOne": "ZERO_OR_ONE",
                "zeroOrMore": "ZERO_OR_MORE",
                "oneOrMore": "ONE_OR_MORE",
                "onlyOne": "ONLY_ONE",
                "mdParent": "MD_PARENT",
            },
            "identification": {
                "nonIdentifying": "NON_IDENTIFYING",
                "identifying": "IDENTIFYING",
            }
        }),
    );

    Ok(value)
}

fn split_styles(raw: &str) -> Vec<String> {
    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    compact
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

type ErLexicalEvent = std::result::Result<(usize, Tok, usize), LexError>;
type ErGrammarError = lalrpop_util::ParseError<usize, Tok, LexError>;
type ErParsedActions = std::result::Result<(ErActionArena, ErActionList), ErGrammarError>;

struct ErSyntax {
    events: Vec<ErLexicalEvent>,
}

impl ErSyntax {
    fn lex(code: &str, control: &OperationControl) -> OperationControlResult<Self> {
        let lexer = Lexer::new(code);
        let mut events = Vec::new();
        for event in lexer {
            if events.len() % 128 == 0 {
                control.checkpoint()?;
            }
            events.push(event);
        }
        control.checkpoint()?;
        Ok(Self { events })
    }

    fn into_editor_facts_and_actions(
        self,
        code: &str,
        control: &OperationControl,
    ) -> OperationControlResult<(EditorSemanticFacts, ErParsedActions)> {
        let mut facts = EditorSemanticFacts::new();
        let mut collector = ErEditorFactCollector::default();
        for (index, event) in self.events.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            match event {
                Ok((start, token, end)) => collector.accept(code, token, *start, *end, &mut facts),
                Err(_) => facts.mark_recovered(),
            }
        }
        collector.finish(code, &mut facts);
        control.checkpoint()?;
        let mut emitted = 0usize;
        let controlled_events = self.events.into_iter().take_while(|_| {
            let active = !emitted.is_multiple_of(128) || control.checkpoint().is_ok();
            emitted = emitted.saturating_add(1);
            active
        });
        let mut arena = ErActionArena::default();
        let actions = er_grammar::ActionsParser::new()
            .parse(&mut arena, controlled_events)
            .map(|actions| (arena, actions));
        control.checkpoint()?;
        Ok((facts, actions))
    }
}

struct ErSemanticSource {
    db: ErDb,
    editor_facts: EditorSemanticFacts,
}

struct ErSemanticFailure {
    error: ErGrammarError,
    editor_facts: EditorSemanticFacts,
}

impl ErSemanticFailure {
    fn recovery_span(&self, fallback_offset: usize) -> SourceSpan {
        match &self.error {
            lalrpop_util::ParseError::User { error } => error.span,
            _ => lalrpop_recovery_span(&self.error, fallback_offset),
        }
    }

    fn into_parse_error(self, meta: &ParseMetadata, fallback_offset: usize) -> Error {
        self.into_error_and_editor_facts(meta, fallback_offset).0
    }

    fn into_error_and_editor_facts(
        self,
        meta: &ParseMetadata,
        fallback_offset: usize,
    ) -> (Error, EditorSemanticFacts) {
        self.into_error_and_editor_facts_for_type(&meta.diagram_type, fallback_offset)
    }

    fn into_error_and_editor_facts_for_type(
        mut self,
        diagram_type: &str,
        fallback_offset: usize,
    ) -> (Error, EditorSemanticFacts) {
        let error = Error::diagram_parse_diagnostic(
            diagram_type.to_string(),
            lalrpop_parse_diagnostic(&self.error, fallback_offset),
        );
        let span = self.recovery_span(fallback_offset);
        self.editor_facts.mark_recovered_from_parse_error(
            format!(
                "er parser recovered after parse error: {}",
                format_lalrpop_parse_error(&self.error)
            ),
            Some(span),
        );
        (error, self.editor_facts)
    }
}

fn construct_er_semantic_source(
    code: &str,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<ErSemanticSource, Box<ErSemanticFailure>>> {
    let syntax = ErSyntax::lex(code, control)?;
    let (editor_facts, actions) = syntax.into_editor_facts_and_actions(code, control)?;
    let (arena, actions) = match actions {
        Ok(actions) => actions,
        Err(error) => {
            return Ok(Err(Box::new(ErSemanticFailure {
                error,
                editor_facts,
            })));
        }
    };

    let mut db = ErDb::new();
    db.replay(arena, actions, control)?;
    Ok(Ok(ErSemanticSource { db, editor_facts }))
}

fn parse_er_semantic_source(code: &str, meta: &ParseMetadata) -> Result<ErSemanticSource> {
    construct_er_semantic_source(code, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
        .map_err(|failure| (*failure).into_parse_error(meta, code.len()))
}

#[cfg(test)]
pub(crate) fn parse_er_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<ErDiagramRenderModel> {
    Ok(parse_er_semantic_source(code, meta)?.db.into_render_model())
}

pub(crate) fn parse_er_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<ErDiagramRenderModel>> {
    let construction = construct_er_semantic_source(code, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err((*failure).into_parse_error(meta, code.len()))),
    };
    control.checkpoint()?;
    Ok(Ok(source.db.into_render_model()))
}

pub(crate) fn parse_er(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_er_semantic_source(code, meta)?.db.into_model(meta)
}

pub(crate) fn parse_er_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_er_semantic_source(code, control)?;
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |ErSemanticSource { db, editor_facts }| (db.into_model(meta), editor_facts),
        |failure| (*failure).into_error_and_editor_facts(meta, code.len()),
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[derive(Debug, Default)]
struct ErEditorFactCollector {
    pending_entity: Option<ErTokenSymbol>,
    deferred_styled_entity: Option<ErTokenSymbol>,
    deferred_styled_ids: Option<SpannedIdList>,
    relationship_started: bool,
    known_entities: HashSet<String>,
    expected_id_list: Option<PendingErExpectation>,
    directive_start: Option<usize>,
    style_payload_pending: bool,
    style_payload_anchor_end: Option<usize>,
    in_attribute_block: bool,
    in_alias: bool,
    in_relationship_role: bool,
    attr_word_index: usize,
}

impl ErEditorFactCollector {
    fn finish(&mut self, code: &str, facts: &mut EditorSemanticFacts) {
        self.finish_line(code, line_content_end(code, code.len()), facts);
        self.push_deferred_styled_entity(facts, ErEntityOccurrence::Definition);
        self.push_pending_entity(facts, ErEntityOccurrence::Definition);
    }

    fn finish_line(&mut self, code: &str, line_end: usize, facts: &mut EditorSemanticFacts) {
        if let Some(start) = self.directive_start {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::Directive,
                SourceSpan::new(start, line_end),
            ));
        }
        if let Some(expected) = self.expected_id_list {
            let kind = er_expected_id_list_kind(expected.kind);
            let separator_ready = expected
                .anchor_end
                .is_none_or(|anchor_end| has_ascii_separator(code, anchor_end, line_end));
            if !matches!(expected.kind, ExpectedErIdList::ClassDef)
                && (!matches!(kind, EditorExpectedSyntaxKind::ClassName) || separator_ready)
            {
                facts.push_expected_syntax(EditorExpectedSyntax::new(
                    kind,
                    SourceSpan::new(line_end, line_end),
                ));
            }
        } else if self.style_payload_pending
            && self
                .style_payload_anchor_end
                .is_some_and(|anchor_end| has_ascii_separator(code, anchor_end, line_end))
        {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::StyleValue,
                SourceSpan::new(line_end, line_end),
            ));
        }
        if !self.in_alias && !self.in_relationship_role {
            let occurrence = if self.relationship_started {
                ErEntityOccurrence::Relationship
            } else {
                ErEntityOccurrence::Definition
            };
            self.push_deferred_styled_entity(facts, occurrence);
            self.push_pending_entity(facts, occurrence);
        }
    }
}

#[derive(Debug, Clone)]
struct ErTokenSymbol {
    name: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErEntityOccurrence {
    Definition,
    Relationship,
}

#[derive(Debug, Clone, Copy)]
enum ExpectedErIdList {
    StyleEntities,
    ClassDef,
    ClassEntities,
    ClassNames,
    InlineClasses,
}

#[derive(Debug, Clone, Copy)]
struct PendingErExpectation {
    kind: ExpectedErIdList,
    anchor_end: Option<usize>,
}

impl ErEditorFactCollector {
    fn accept(
        &mut self,
        code: &str,
        token: &Tok,
        start: usize,
        end: usize,
        facts: &mut EditorSemanticFacts,
    ) {
        if self.deferred_styled_entity.is_some() && !matches!(token, Tok::IdList(_)) {
            let occurrence = if matches!(
                token,
                Tok::ZeroOrOne
                    | Tok::ZeroOrMore
                    | Tok::OneOrMore
                    | Tok::OnlyOne
                    | Tok::MdParent
                    | Tok::Identifying
                    | Tok::NonIdentifying
            ) {
                ErEntityOccurrence::Relationship
            } else {
                ErEntityOccurrence::Definition
            };
            self.push_deferred_styled_entity(facts, occurrence);
        }

        match token {
            Tok::ErDiagram => self.reset_line_state(),
            Tok::Newline => {
                self.finish_line(code, line_content_end(code, start), facts);
                self.reset_line_state();
            }
            Tok::StyleKw => {
                facts.push_directive_prefix("style");
                self.directive_start = Some(start);
                self.style_payload_pending = true;
                self.expect_id_list(ExpectedErIdList::StyleEntities);
            }
            Tok::ClassDefKw => {
                facts.push_directive_prefix("classDef");
                self.directive_start = Some(start);
                self.style_payload_pending = true;
                self.expect_id_list_after(ExpectedErIdList::ClassDef, end);
            }
            Tok::ClassKw => {
                facts.push_directive_prefix("class");
                self.directive_start = Some(start);
                self.expect_id_list(ExpectedErIdList::ClassEntities);
            }
            Tok::SubgraphKw => {
                facts.push_directive_prefix("subgraph");
                self.directive_start = Some(start);
            }
            Tok::EndKw => {
                self.directive_start = None;
            }
            Tok::StyleSeparator => {
                if self.relationship_started {
                    self.push_pending_entity(facts, ErEntityOccurrence::Relationship);
                } else {
                    self.deferred_styled_entity = self.pending_entity.take();
                }
                self.directive_start = Some(start);
                self.expect_id_list(ExpectedErIdList::InlineClasses);
            }
            Tok::IdList(ids) => {
                if self.deferred_styled_entity.is_some() {
                    self.deferred_styled_ids = Some(ids.clone());
                } else {
                    self.push_id_list(ids.clone(), facts);
                }
            }
            Tok::Name(name) => {
                if self.in_attribute_block {
                    return;
                }
                if self.in_alias {
                    self.push_context_payload(facts, name.clone(), "er entity alias", start, end);
                    return;
                }
                if self.in_relationship_role {
                    self.push_context_payload(
                        facts,
                        name.clone(),
                        "er relationship role",
                        start,
                        end,
                    );
                    self.in_relationship_role = false;
                    return;
                }
                let span = if end.saturating_sub(start) == name.len().saturating_add(2) {
                    SourceSpan::new(start + 1, end - 1)
                } else {
                    SourceSpan::new(start, end)
                };
                let symbol = ErTokenSymbol {
                    name: name.clone(),
                    span,
                };
                if let Some(entity) = self.pending_entity.replace(symbol) {
                    let occurrence = if self.relationship_started {
                        ErEntityOccurrence::Relationship
                    } else {
                        ErEntityOccurrence::Definition
                    };
                    self.push_entity_symbol(facts, entity, "er entity reference", occurrence);
                }
            }
            Tok::ZeroOrOne
            | Tok::ZeroOrMore
            | Tok::OneOrMore
            | Tok::OnlyOne
            | Tok::MdParent
            | Tok::Identifying
            | Tok::NonIdentifying => {
                self.push_pending_entity(facts, ErEntityOccurrence::Relationship);
                self.relationship_started = true;
            }
            Tok::Colon => {
                let occurrence = if self.relationship_started {
                    ErEntityOccurrence::Relationship
                } else {
                    ErEntityOccurrence::Definition
                };
                self.push_deferred_styled_entity(facts, occurrence);
                self.push_pending_entity(facts, occurrence);
                self.in_relationship_role = true;
            }
            Tok::BlockStart => {
                self.push_deferred_styled_entity(facts, ErEntityOccurrence::Definition);
                self.push_pending_entity(facts, ErEntityOccurrence::Definition);
                self.in_attribute_block = true;
                self.attr_word_index = 0;
            }
            Tok::BlockStop => {
                self.in_attribute_block = false;
                self.attr_word_index = 0;
            }
            Tok::AttrWord(word) => {
                if !self.in_attribute_block {
                    return;
                }
                let span = SourceSpan::new(start, end);
                if self.attr_word_index.is_multiple_of(2) {
                    self.push_payload_symbol(
                        facts,
                        word.clone(),
                        "er attribute type",
                        EditorSemanticKind::String,
                        span,
                        span,
                    );
                } else {
                    self.push_attribute_symbol(facts, word.clone(), span);
                }
                self.attr_word_index += 1;
            }
            Tok::Comma => {
                if self.in_attribute_block && self.attr_word_index > 2 {
                    self.attr_word_index = 2;
                }
            }
            Tok::Question => {}
            Tok::AttrKey(key) => {
                if self.in_attribute_block {
                    self.push_payload_symbol(
                        facts,
                        key.clone(),
                        "er attribute key",
                        EditorSemanticKind::Property,
                        SourceSpan::new(start, end),
                        SourceSpan::new(start, end),
                    );
                }
            }
            Tok::Comment(comment) => {
                if self.in_attribute_block {
                    let span = SourceSpan::new(start, end);
                    let selection = if end.saturating_sub(start) >= 2 {
                        SourceSpan::new(start + 1, end - 1)
                    } else {
                        span
                    };
                    self.push_payload_symbol(
                        facts,
                        comment.clone(),
                        "er attribute comment",
                        EditorSemanticKind::String,
                        span,
                        selection,
                    );
                }
            }
            Tok::AccTitle(_) => facts.push_directive_prefix("accTitle"),
            Tok::AccDescr(_) | Tok::AccDescrMultiline(_) => facts.push_directive_prefix("accDescr"),
            Tok::SquareStart => {
                self.push_deferred_styled_entity(facts, ErEntityOccurrence::Definition);
                self.push_pending_entity(facts, ErEntityOccurrence::Definition);
                self.in_alias = true;
            }
            Tok::SquareStop => self.in_alias = false,
            Tok::Str(value) => {
                let detail = if self.in_alias {
                    Some("er entity alias")
                } else if self.in_relationship_role {
                    self.in_relationship_role = false;
                    Some("er relationship role")
                } else {
                    None
                };
                if let Some(detail) = detail {
                    self.push_context_payload(facts, value.clone(), detail, start, end);
                }
            }
            Tok::Direction(direction) => {
                facts.push_expected_syntax(EditorExpectedSyntax::new(
                    EditorExpectedSyntaxKind::CardinalDirectionValue,
                    direction.selection,
                ));
            }
            Tok::RestOfLine(raw) => {
                if self.style_payload_pending {
                    let line_end = line_content_end(code, end);
                    let trailing_slot =
                        trailing_ascii_whitespace_slot(code, start, end).or_else(|| {
                            (raw.is_empty()
                                && self.style_payload_anchor_end.is_some_and(|anchor_end| {
                                    has_ascii_separator(code, anchor_end, line_end)
                                }))
                            .then_some(SourceSpan::new(line_end, line_end))
                        });
                    if let Some(span) = trailing_slot {
                        facts.push_expected_syntax(EditorExpectedSyntax::new(
                            EditorExpectedSyntaxKind::StyleValue,
                            span,
                        ));
                    }
                    let payload = raw.trim();
                    if !payload.is_empty()
                        && let Some(selection) =
                            source_value_span(code, SourceSpan::new(start, end), payload)
                    {
                        facts.push_expected_syntax(EditorExpectedSyntax::new(
                            EditorExpectedSyntaxKind::Payload,
                            selection,
                        ));
                    }
                    self.style_payload_pending = false;
                }
            }
        }
    }

    fn reset_line_state(&mut self) {
        self.pending_entity = None;
        self.deferred_styled_entity = None;
        self.deferred_styled_ids = None;
        self.expected_id_list = None;
        self.directive_start = None;
        self.style_payload_pending = false;
        self.style_payload_anchor_end = None;
        self.in_alias = false;
        self.in_relationship_role = false;
        self.relationship_started = false;
        if !self.in_attribute_block {
            self.attr_word_index = 0;
        }
    }

    fn push_pending_entity(
        &mut self,
        facts: &mut EditorSemanticFacts,
        occurrence: ErEntityOccurrence,
    ) {
        if let Some(entity) = self.pending_entity.take() {
            let detail = match occurrence {
                ErEntityOccurrence::Definition => "er entity",
                ErEntityOccurrence::Relationship => "er relationship entity",
            };
            self.push_entity_symbol(facts, entity, detail, occurrence);
        }
    }

    fn push_deferred_styled_entity(
        &mut self,
        facts: &mut EditorSemanticFacts,
        occurrence: ErEntityOccurrence,
    ) {
        if let Some(entity) = self.deferred_styled_entity.take() {
            let detail = match occurrence {
                ErEntityOccurrence::Definition => "er entity",
                ErEntityOccurrence::Relationship => "er relationship entity",
            };
            self.push_entity_symbol(facts, entity, detail, occurrence);
        }
        if let Some(ids) = self.deferred_styled_ids.take() {
            self.push_id_list(ids, facts);
        }
    }

    fn push_id_list(&mut self, ids: SpannedIdList, facts: &mut EditorSemanticFacts) {
        let expected = self.expected_id_list.take().map(|pending| pending.kind);
        let span = ids.span();
        if matches!(
            expected,
            Some(ExpectedErIdList::StyleEntities | ExpectedErIdList::ClassDef)
        ) {
            self.style_payload_anchor_end = Some(span.end);
        }
        let detail = match expected {
            Some(ExpectedErIdList::StyleEntities) => "er style target",
            Some(ExpectedErIdList::ClassDef) => "er class definition",
            Some(ExpectedErIdList::ClassEntities) => "er class target",
            Some(ExpectedErIdList::ClassNames) => "er class name",
            Some(ExpectedErIdList::InlineClasses) => "er inline class",
            None => "er id",
        };
        let kind = match expected {
            Some(ExpectedErIdList::ClassDef)
            | Some(ExpectedErIdList::ClassNames)
            | Some(ExpectedErIdList::InlineClasses) => EditorSemanticKind::Property,
            _ => EditorSemanticKind::Struct,
        };

        if let Some(expected) = expected {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                er_expected_id_list_kind(expected),
                span,
            ));
        }

        for id in ids.ids {
            if id.name.is_empty() {
                continue;
            }
            let symbol = match expected {
                Some(ExpectedErIdList::ClassDef) => EditorSemanticSymbol::class_definition(
                    id.name,
                    Some(detail.to_string()),
                    kind,
                    id.span,
                    id.span,
                ),
                Some(ExpectedErIdList::StyleEntities | ExpectedErIdList::ClassEntities) => {
                    EditorSemanticSymbol::reference(
                        id.name,
                        Some(detail.to_string()),
                        kind,
                        id.span,
                        id.span,
                    )
                }
                Some(ExpectedErIdList::ClassNames | ExpectedErIdList::InlineClasses) => {
                    EditorSemanticSymbol::payload(
                        id.name,
                        Some(detail.to_string()),
                        kind,
                        id.span,
                        id.span,
                    )
                }
                None => EditorSemanticSymbol::payload(
                    id.name,
                    Some(detail.to_string()),
                    kind,
                    id.span,
                    id.span,
                ),
            };
            facts.push_symbol(symbol);
        }

        if matches!(expected, Some(ExpectedErIdList::ClassEntities)) {
            self.expect_id_list_after(ExpectedErIdList::ClassNames, span.end);
        }
    }

    fn expect_id_list(&mut self, expected: ExpectedErIdList) {
        self.expected_id_list = Some(PendingErExpectation {
            kind: expected,
            anchor_end: None,
        });
    }

    fn expect_id_list_after(&mut self, expected: ExpectedErIdList, anchor_end: usize) {
        self.expected_id_list = Some(PendingErExpectation {
            kind: expected,
            anchor_end: Some(anchor_end),
        });
    }

    fn push_entity_symbol(
        &mut self,
        facts: &mut EditorSemanticFacts,
        symbol: ErTokenSymbol,
        detail: &'static str,
        occurrence: ErEntityOccurrence,
    ) {
        if symbol.name.is_empty() {
            return;
        }
        let is_implicit_definition = matches!(occurrence, ErEntityOccurrence::Relationship)
            && !self.known_entities.contains(&symbol.name);
        if matches!(occurrence, ErEntityOccurrence::Definition) || is_implicit_definition {
            self.known_entities.insert(symbol.name.clone());
        }
        let detail = Some(if is_implicit_definition {
            "er implicit entity".to_string()
        } else {
            detail.to_string()
        });
        let entity =
            if matches!(occurrence, ErEntityOccurrence::Relationship) && !is_implicit_definition {
                EditorSemanticSymbol::reference(
                    symbol.name,
                    detail,
                    EditorSemanticKind::Struct,
                    symbol.span,
                    symbol.span,
                )
            } else {
                EditorSemanticSymbol::new(
                    symbol.name,
                    detail,
                    EditorSemanticKind::Struct,
                    symbol.span,
                    symbol.span,
                )
            };
        facts.push_symbol(entity);
    }

    fn push_attribute_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        name: String,
        span: SourceSpan,
    ) {
        if name.is_empty() {
            return;
        }
        facts.push_symbol(EditorSemanticSymbol::outline(
            name,
            Some("er attribute".to_string()),
            EditorSemanticKind::Property,
            span,
            span,
        ));
    }

    fn push_payload_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        name: String,
        detail: &'static str,
        kind: EditorSemanticKind,
        span: SourceSpan,
        selection: SourceSpan,
    ) {
        if name.is_empty() {
            return;
        }
        facts.push_symbol(EditorSemanticSymbol::payload(
            name,
            Some(detail.to_string()),
            kind,
            span,
            selection,
        ));
    }

    fn push_context_payload(
        &self,
        facts: &mut EditorSemanticFacts,
        name: String,
        detail: &'static str,
        start: usize,
        end: usize,
    ) {
        let span = SourceSpan::new(start, end);
        let selection = if end.saturating_sub(start) == name.len().saturating_add(2) {
            SourceSpan::new(start + 1, end - 1)
        } else {
            span
        };
        self.push_payload_symbol(
            facts,
            name,
            detail,
            EditorSemanticKind::String,
            span,
            selection,
        );
    }
}

fn er_expected_id_list_kind(expected: ExpectedErIdList) -> EditorExpectedSyntaxKind {
    match expected {
        ExpectedErIdList::ClassDef
        | ExpectedErIdList::ClassNames
        | ExpectedErIdList::InlineClasses => EditorExpectedSyntaxKind::ClassName,
        ExpectedErIdList::StyleEntities | ExpectedErIdList::ClassEntities => {
            EditorExpectedSyntaxKind::IdList
        }
    }
}

#[derive(Debug, Clone)]
struct SpannedDirection {
    value: String,
    selection: SourceSpan,
}

#[derive(Debug, Clone)]
enum Tok {
    ErDiagram,
    Newline,

    Name(String),
    Str(String),
    IdList(SpannedIdList),
    RestOfLine(String),

    AccTitle(String),
    AccDescr(String),
    AccDescrMultiline(String),

    BlockStart,
    BlockStop,
    SquareStart,
    SquareStop,
    StyleSeparator,
    Colon,
    Comma,
    Question,

    StyleKw,
    ClassDefKw,
    ClassKw,
    SubgraphKw,
    EndKw,
    Direction(SpannedDirection),

    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
    Identifying,
    NonIdentifying,

    AttrWord(String),
    AttrKey(String),
    Comment(String),
}

#[derive(Debug, Clone)]
struct LexError {
    message: String,
    span: SourceSpan,
}

impl LexError {
    fn new(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LexError {}

impl crate::error::ParseErrorSourceSpan for LexError {
    fn source_span(&self) -> Option<crate::SourceSpan> {
        Some(self.span)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Default,
    Block,
    NeedIdListOnly,
    NeedIdListThenLineRest,
    NeedClassFirstIdList,
    NeedClassSecondIdList,
    LineRest,
}

struct Lexer<'input> {
    input: &'input str,
    pos: usize,
    pending: VecDeque<(usize, Tok, usize)>,
    mode: Mode,
    direction_scan_end: usize,
}

impl<'input> Lexer<'input> {
    fn new(input: &'input str) -> Self {
        #[cfg(test)]
        ER_SYNTAX_CONSTRUCTION_COUNT.set(ER_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

        Self {
            input,
            pos: 0,
            pending: VecDeque::new(),
            mode: Mode::Default,
            direction_scan_end: 0,
        }
    }

    fn emit_token(
        &mut self,
        token: (usize, Tok, usize),
    ) -> std::result::Result<(usize, Tok, usize), LexError> {
        Ok(token)
    }

    fn emit_result(
        &mut self,
        result: std::result::Result<(usize, Tok, usize), LexError>,
    ) -> std::result::Result<(usize, Tok, usize), LexError> {
        result
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn skip_ws_default(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' {
                self.pos += 1;
                continue;
            }
            break;
        }
    }

    fn skip_ws_block(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                self.pos += 1;
                continue;
            }
            break;
        }
    }

    fn starts_with_ci(&self, s: &str) -> bool {
        self.input[self.pos..]
            .get(..s.len())
            .is_some_and(|h| h.eq_ignore_ascii_case(s))
    }

    fn starts_with_word_ci(&self, s: &str) -> bool {
        self.starts_with_ci(s) && self.ends_word_at(s.len())
    }

    fn read_to_newline(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b'\r' | b'\n') {
                break;
            }
            self.pos += 1;
        }
        self.input[start..self.pos].to_string()
    }

    fn lex_comment(&mut self) -> bool {
        if self.input[self.pos..].starts_with("%%") {
            let _ = self.read_to_newline();
            return true;
        }
        false
    }

    fn lex_newline(&mut self) -> Option<(usize, Tok, usize)> {
        if self.mode == Mode::Block {
            return None;
        }
        let start = self.pos;
        self.pos = consume_line_ending(self.input, self.pos)?;
        while let Some(end) = consume_line_ending(self.input, self.pos) {
            self.pos = end;
        }
        if self.mode == Mode::LineRest {
            self.mode = Mode::Default;
        }
        Some((start, Tok::Newline, self.pos))
    }

    fn lex_acc_title(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_ci("accTitle") {
            return None;
        }
        let after = self.pos + "accTitle".len();
        let rest = &self.input[after..];
        let rest_trim = rest.trim_start_matches(is_ecmascript_whitespace);
        if !rest_trim.starts_with(':') {
            return None;
        }
        let consumed_ws = rest.len() - rest_trim.len();
        let body =
            self.input[after + consumed_ws + 1..].trim_start_matches(is_ecmascript_whitespace);
        self.pos = self.input.len() - body.len();
        let s = self.read_to_newline();
        Some(Ok((
            start,
            Tok::AccTitle(s.trim_matches(is_ecmascript_whitespace).to_string()),
            self.pos,
        )))
    }

    fn lex_acc_descr(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_ci("accDescr") {
            return None;
        }
        let after = self.pos + "accDescr".len();
        let rest = &self.input[after..];
        let rest_trim = rest.trim_start_matches(is_ecmascript_whitespace);
        if let Some(body) = rest_trim.strip_prefix('{') {
            let body = body.trim_start_matches(is_ecmascript_whitespace);
            self.pos = self.input.len() - body.len();
            let Some(end_rel) = self.input[self.pos..].find('}') else {
                self.pos = self.input.len();
                return Some(Err(LexError::new(
                    "Unterminated accDescr block; missing '}'",
                    SourceSpan::new(start, self.input.len()),
                )));
            };
            let body = self.input[self.pos..self.pos + end_rel].to_string();
            self.pos = self.pos + end_rel + 1;
            return Some(Ok((
                start,
                Tok::AccDescrMultiline(body.trim_matches(is_ecmascript_whitespace).to_string()),
                self.pos,
            )));
        }
        let body = rest_trim
            .strip_prefix(':')?
            .trim_start_matches(is_ecmascript_whitespace);
        self.pos = self.input.len() - body.len();
        let s = self.read_to_newline();
        Some(Ok((
            start,
            Tok::AccDescr(s.trim_matches(is_ecmascript_whitespace).to_string()),
            self.pos,
        )))
    }

    fn lex_direction(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if start < self.direction_scan_end {
            return None;
        }
        let rest = &self.input[start..];
        // Mermaid tries `.*direction\s+TB[^\n]*` (then BT, RL, LR) before names.
        // Match the whole rule before consuming anything: `direction` alone is an entity.
        let prefix_end = rest
            .find(['\n', '\r', '\u{2028}', '\u{2029}'])
            .unwrap_or(rest.len());
        for direction in ["TB", "BT", "RL", "LR"] {
            let value_end = rest[..prefix_end]
                .char_indices()
                .rev()
                .find_map(|(offset, _)| {
                    let candidate = &rest[offset..];
                    if !candidate
                        .get(.."direction".len())?
                        .eq_ignore_ascii_case("direction")
                    {
                        return None;
                    }
                    let suffix = &candidate["direction".len()..];
                    let value = suffix.trim_start_matches(is_ecmascript_whitespace);
                    if value.len() == suffix.len()
                        || !value.get(..2)?.eq_ignore_ascii_case(direction)
                    {
                        return None;
                    }
                    Some(rest.len() - value.len() + 2)
                });
            if let Some(value_end) = value_end {
                self.pos = start + value_end;
                // Unlike the leading `.`, the trailing `[^\n]*` includes carriage returns.
                let tail = &self.input[self.pos..];
                self.pos += tail.find('\n').unwrap_or(tail.len());
                return Some((
                    start,
                    Tok::Direction(SpannedDirection {
                        value: direction.to_string(),
                        selection: SourceSpan::new(start + value_end - 2, start + value_end),
                    }),
                    self.pos,
                ));
            }
        }
        // A failed whole-line lookup also proves every remaining token prefix cannot match.
        // Remember it so a line of entity names does not require quadratic rescanning.
        self.direction_scan_end = start + prefix_end;
        None
    }

    fn lex_keyword(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.starts_with_word_ci("erDiagram") {
            self.pos += "erDiagram".len();
            return Some((start, Tok::ErDiagram, self.pos));
        }
        if self.starts_with_word_ci("style") {
            self.pos += "style".len();
            self.mode = Mode::NeedIdListThenLineRest;
            return Some((start, Tok::StyleKw, self.pos));
        }
        if self.starts_with_word_ci("classDef") {
            self.pos += "classDef".len();
            self.mode = Mode::NeedIdListThenLineRest;
            return Some((start, Tok::ClassDefKw, self.pos));
        }
        if self.starts_with_word_ci("class") {
            self.pos += "class".len();
            self.mode = Mode::NeedClassFirstIdList;
            return Some((start, Tok::ClassKw, self.pos));
        }
        if self.starts_with_word_ci("subgraph") {
            self.pos += "subgraph".len();
            return Some((start, Tok::SubgraphKw, self.pos));
        }
        if self.starts_with_word_ci("end") {
            self.pos += "end".len();
            return Some((start, Tok::EndKw, self.pos));
        }
        None
    }

    fn lex_id_list(&mut self) -> Option<(usize, Tok, usize)> {
        if !matches!(
            self.mode,
            Mode::NeedIdListOnly
                | Mode::NeedIdListThenLineRest
                | Mode::NeedClassFirstIdList
                | Mode::NeedClassSecondIdList
        ) {
            return None;
        }
        let start = self.pos;
        self.skip_ws_default();
        let mut ids: Vec<SpannedId> = Vec::new();
        loop {
            let id_start = self.pos;
            let mut id_end = self.pos;
            for (rel, ch) in self.input[self.pos..].char_indices() {
                let ok = !ch.is_ascii()
                    || ch.is_ascii_alphanumeric()
                    || matches!(ch, '_' | '-' | '*' | '.');
                if !ok {
                    break;
                }
                id_end = self.pos + rel + ch.len_utf8();
            }
            if id_end == id_start {
                break;
            }
            ids.push(SpannedId {
                name: self.input[id_start..id_end].to_string(),
                span: SourceSpan::new(id_start, id_end),
            });
            self.pos = id_end;

            self.skip_ws_default();
            if self.peek() != Some(b',') {
                break;
            }
            self.pos += 1;
            self.skip_ws_default();
        }
        if ids.is_empty() {
            return None;
        }
        self.mode = match self.mode {
            Mode::NeedIdListOnly => Mode::Default,
            Mode::NeedIdListThenLineRest => Mode::LineRest,
            Mode::NeedClassFirstIdList => Mode::NeedClassSecondIdList,
            Mode::NeedClassSecondIdList => Mode::Default,
            _ => Mode::Default,
        };
        Some((start, Tok::IdList(SpannedIdList { ids }), self.pos))
    }

    fn lex_rest_of_line(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        if self.mode != Mode::LineRest {
            return None;
        }
        let start = self.pos;
        self.skip_ws_default();
        let s = self.read_to_newline();
        self.mode = Mode::Default;
        let styles = s.trim().trim_end_matches(';').trim_end();
        if styles.is_empty() {
            return Some(Err(LexError::new(
                "Expected ER style payload",
                SourceSpan::new(start, self.pos),
            )));
        }
        Some(Ok((start, Tok::RestOfLine(styles.to_string()), self.pos)))
    }

    /// Whether the word of `len` bytes at the cursor ends at a word boundary, like Mermaid's `\b`
    /// after a word. Mermaid's regexes are not Unicode-aware, so only ASCII `[A-Za-z0-9_]`
    /// continues a word: `tokens` is a name, while `to-do` and `to注文` start with `to`.
    fn ends_word_at(&self, len: usize) -> bool {
        self.input
            .as_bytes()
            .get(self.pos + len)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
    }

    fn lex_rel_tokens(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let s = &self.input[self.pos..];

        // Word cardinalities only match at a word boundary (`/^(?:one or zero\b)/i`,
        // `/^(?:many\b)/i`, `/^(?:to\b)/i`, ...), so entity names such as `tokens`, `oneshot`
        // or `manyToMany` fall through to `lex_name_or_str`.
        let lower = s.to_ascii_lowercase();
        for (pat, tok) in [
            ("optionally to", Tok::NonIdentifying),
            ("one or zero", Tok::ZeroOrOne),
            ("zero or one", Tok::ZeroOrOne),
            ("one or more", Tok::OneOrMore),
            ("one or many", Tok::OneOrMore),
            ("zero or more", Tok::ZeroOrMore),
            ("zero or many", Tok::ZeroOrMore),
            ("only one", Tok::OnlyOne),
        ] {
            if lower.starts_with(pat) && self.ends_word_at(pat.len()) {
                self.pos += pat.len();
                return Some((start, tok, self.pos));
            }
        }

        if lower.starts_with("many(0)") {
            self.pos += "many(0)".len();
            return Some((start, Tok::ZeroOrMore, self.pos));
        }
        if lower.starts_with("many(1)") {
            self.pos += "many(1)".len();
            return Some((start, Tok::OneOrMore, self.pos));
        }
        if lower.starts_with("0+") {
            self.pos += "0+".len();
            return Some((start, Tok::ZeroOrMore, self.pos));
        }
        if lower.starts_with("1+") {
            self.pos += "1+".len();
            return Some((start, Tok::OneOrMore, self.pos));
        }
        if lower.starts_with("many") && self.ends_word_at("many".len()) {
            self.pos += "many".len();
            return Some((start, Tok::ZeroOrMore, self.pos));
        }
        if lower.starts_with("one") && self.ends_word_at("one".len()) {
            self.pos += "one".len();
            return Some((start, Tok::OnlyOne, self.pos));
        }
        if self.one_starts_numeric_cardinality() {
            self.pos += 1;
            return Some((start, Tok::OnlyOne, self.pos));
        }
        if lower.starts_with("to") && self.ends_word_at("to".len()) {
            self.pos += "to".len();
            return Some((start, Tok::Identifying, self.pos));
        }

        for (pat, tok) in [
            ("||", Tok::OnlyOne),
            ("|o", Tok::ZeroOrOne),
            ("o|", Tok::ZeroOrOne),
            ("|{", Tok::OneOrMore),
            ("o{", Tok::ZeroOrMore),
            ("}|", Tok::OneOrMore),
            ("}o", Tok::ZeroOrMore),
        ] {
            // Only `|o` and `}o` end with a word character, so only they carry `\b` in Mermaid.
            let needs_boundary = pat.ends_with('o');
            if s.starts_with(pat) && (!needs_boundary || self.ends_word_at(pat.len())) {
                self.pos += pat.len();
                return Some((start, tok, self.pos));
            }
        }

        if s.starts_with("..") || s.starts_with(".-") || s.starts_with("-.") {
            self.pos += 2;
            return Some((start, Tok::NonIdentifying, self.pos));
        }
        if s.starts_with("--") {
            self.pos += 2;
            return Some((start, Tok::Identifying, self.pos));
        }

        if self.starts_with_ci("u")
            && matches!(
                self.input.as_bytes().get(self.pos + 1),
                Some(b'.' | b'-' | b'|')
            )
        {
            self.pos += 1;
            return Some((start, Tok::MdParent, self.pos));
        }

        None
    }

    fn one_starts_numeric_cardinality(&self) -> bool {
        let Some(after_one) = self.input[self.pos..].strip_prefix('1') else {
            return false;
        };

        if ["--", "..", ".-", "-."]
            .iter()
            .any(|operator| after_one.starts_with(operator))
        {
            return true;
        }

        let Some(first) = after_one.chars().next() else {
            return false;
        };
        if !first.is_whitespace() {
            return false;
        }

        after_one
            .trim_start_matches(char::is_whitespace)
            .chars()
            .next()
            .is_some_and(|next| {
                next.is_ascii_alphabetic()
                    || next.is_ascii_digit()
                    || matches!(next, '_' | '"' | '\'')
            })
    }

    fn lex_punct(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        match self.peek()? {
            b'{' => {
                self.pos += 1;
                self.mode = Mode::Block;
                Some((start, Tok::BlockStart, self.pos))
            }
            b'}' => {
                if self.mode != Mode::Block {
                    return None;
                }
                self.pos += 1;
                self.mode = Mode::Default;
                Some((start, Tok::BlockStop, self.pos))
            }
            b'[' => {
                self.pos += 1;
                Some((start, Tok::SquareStart, self.pos))
            }
            b']' => {
                self.pos += 1;
                Some((start, Tok::SquareStop, self.pos))
            }
            b':' => {
                if self.input[self.pos..].starts_with(":::") {
                    self.pos += 3;
                    self.mode = Mode::NeedIdListOnly;
                    return Some((start, Tok::StyleSeparator, self.pos));
                }
                self.pos += 1;
                Some((start, Tok::Colon, self.pos))
            }
            b',' => {
                self.pos += 1;
                Some((start, Tok::Comma, self.pos))
            }
            _ => None,
        }
    }

    fn lex_block_token(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        if self.mode != Mode::Block {
            return None;
        }
        let start = self.pos;
        self.skip_ws_block();
        if self.pos >= self.input.len() {
            self.mode = Mode::Default;
            return Some(Err(LexError::new(
                "EOF inside attribute block",
                SourceSpan::new(self.pos, self.pos),
            )));
        }
        if self.peek() == Some(b'}') {
            return None;
        }
        if self.peek() == Some(b',') {
            self.pos += 1;
            return Some(Ok((start, Tok::Comma, self.pos)));
        }
        if self.peek() == Some(b'?') {
            self.pos += 1;
            return Some(Ok((start, Tok::Question, self.pos)));
        }
        if self.peek() == Some(b'"') {
            self.pos += 1;
            let Some(rel_end) = self.input[self.pos..].find('"') else {
                return Some(Err(LexError::new(
                    "Unterminated comment string; missing '\"'",
                    SourceSpan::new(start, self.input.len()),
                )));
            };
            let s = self.input[self.pos..self.pos + rel_end].to_string();
            self.pos = self.pos + rel_end + 1;
            return Some(Ok((start, Tok::Comment(s), self.pos)));
        }
        if self.peek() == Some(b'`') {
            let delimiter_start = self.pos;
            self.pos += 1;
            let body_start = self.pos;
            let Some(rel_end) = self.input[self.pos..].find('`') else {
                return Some(Err(LexError::new(
                    "Unterminated attribute word; missing '`'",
                    SourceSpan::new(start, self.input.len()),
                )));
            };
            let body_end = self.pos + rel_end;
            if body_end == body_start {
                self.pos = body_end + 1;
                return Some(Err(LexError::new(
                    "Empty backtick attribute word",
                    SourceSpan::new(delimiter_start, self.pos),
                )));
            }
            let s = self.input[body_start..body_end].to_string();
            self.pos = body_end + 1;
            return Some(Ok((body_start, Tok::AttrWord(s), body_end)));
        }
        if let Some(two) = self.input[self.pos..].get(..2) {
            let two_upper = two.to_ascii_uppercase();
            if matches!(two_upper.as_str(), "PK" | "FK" | "UK") {
                let prev_ok = self.pos == 0
                    || matches!(
                        self.input.as_bytes()[self.pos - 1],
                        b' ' | b'\t' | b'\r' | b'\n' | b','
                    );
                let next_ok = self
                    .input
                    .as_bytes()
                    .get(self.pos + 2)
                    .copied()
                    .map(|b| b.is_ascii_whitespace() || matches!(b, b',' | b'"' | b'}'))
                    .unwrap_or(true);
                if prev_ok && next_ok {
                    self.pos += 2;
                    return Some(Ok((start, Tok::AttrKey(two_upper), self.pos)));
                }
            }
        }

        let start_word = self.pos;
        let mut end = self.pos;
        for (rel, ch) in self.input[self.pos..].char_indices() {
            if ch.is_whitespace() || matches!(ch, '"' | '}' | '?') {
                break;
            }
            end = self.pos + rel + ch.len_utf8();
        }
        if end == start_word {
            self.pos += self.peek().map(|_| 1).unwrap_or(0);
            return Some(Err(LexError::new(
                format!("Unexpected character inside attribute block at {start_word}"),
                SourceSpan::new(start_word, self.pos),
            )));
        }
        self.pos = end;
        let raw = &self.input[start_word..end];
        let tilde_count = raw.chars().filter(|&c| c == '~').count();
        if tilde_count >= 2 {
            return Some(Ok((start, Tok::AttrWord(raw.to_string()), self.pos)));
        }

        let mut chars = raw.chars();
        let first = chars.next()?;
        let first_ok = first == '*' || first == '_' || first.is_alphabetic() || !first.is_ascii();
        let rest_ok = chars.all(|c| {
            c == '*'
                || c == '-'
                || c == '_'
                || c == '.'
                || c == ','
                || c.is_ascii_digit()
                || c.is_alphabetic()
                || matches!(c, '[' | ']' | '(' | ')')
                || !c.is_ascii()
        });
        if !first_ok || !rest_ok {
            return Some(Err(LexError::new(
                "Invalid attribute word",
                SourceSpan::new(start_word, end),
            )));
        }
        Some(Ok((start, Tok::AttrWord(raw.to_string()), self.pos)))
    }

    fn lex_name_or_str(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        if self.mode == Mode::Block {
            return None;
        }
        let start = self.pos;
        if self.peek()? == b'"' {
            self.pos += 1;
            let Some(rel_end) = self.input[self.pos..].find('"') else {
                return Some(Err(LexError::new(
                    "Unterminated string literal; missing '\"'",
                    SourceSpan::new(start, self.input.len()),
                )));
            };
            let s = self.input[self.pos..self.pos + rel_end].to_string();
            self.pos = self.pos + rel_end + 1;
            let is_entity_name = !s.is_empty()
                && !s.contains('%')
                && !s.contains('\\')
                && !s.contains('\r')
                && !s.contains('\n')
                && !s.contains('\u{0008}')
                && !s.contains('\u{000B}');
            if is_entity_name {
                return Some(Ok((start, Tok::Name(s), self.pos)));
            }
            return Some(Ok((start, Tok::Str(s), self.pos)));
        }

        let mut end = self.pos;
        for (rel, ch) in self.input[self.pos..].char_indices() {
            let ok =
                !ch.is_ascii() || ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '*' | '.');
            if !ok {
                break;
            }
            end = self.pos + rel + ch.len_utf8();
        }
        if end == self.pos {
            return None;
        }
        let s = self.input[self.pos..end].to_string();
        self.pos = end;
        Some(Ok((start, Tok::Name(s), self.pos)))
    }
}

impl Iterator for Lexer<'_> {
    type Item = std::result::Result<(usize, Tok, usize), LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tok) = self.pending.pop_front() {
            return Some(self.emit_token(tok));
        }

        loop {
            // INITIAL accessibility rules precede directions, which in turn
            // precede whitespace. Exclusive attribute/style modes bypass them.
            if matches!(
                self.mode,
                Mode::Default
                    | Mode::NeedIdListOnly
                    | Mode::NeedClassFirstIdList
                    | Mode::NeedClassSecondIdList
            ) {
                if let Some(tok) = self.lex_acc_title() {
                    return Some(self.emit_result(tok));
                }
                if let Some(tok) = self.lex_acc_descr() {
                    return Some(self.emit_result(tok));
                }
                if let Some(tok) = self.lex_direction() {
                    return Some(self.emit_token(tok));
                }
            }
            let before_whitespace = self.pos;
            match self.mode {
                Mode::Block => self.skip_ws_block(),
                _ => self.skip_ws_default(),
            }
            if self.pos != before_whitespace {
                continue;
            }

            if self.pos >= self.input.len() {
                if self.mode == Mode::Block {
                    self.mode = Mode::Default;
                    return Some(Err(LexError::new(
                        "EOF inside attribute block",
                        SourceSpan::new(self.pos, self.pos),
                    )));
                }
                return None;
            }

            if self.lex_comment() {
                continue;
            }

            if let Some(tok) = self.lex_block_token() {
                return Some(self.emit_result(tok));
            }

            if let Some(tok) = self.lex_rest_of_line() {
                return Some(self.emit_result(tok));
            }

            if let Some(tok) = self.lex_newline() {
                return Some(self.emit_token(tok));
            }

            if let Some(tok) = self.lex_keyword() {
                return Some(self.emit_token(tok));
            }

            if let Some(tok) = self.lex_id_list() {
                return Some(self.emit_token(tok));
            }

            if let Some(tok) = self.lex_punct() {
                return Some(self.emit_token(tok));
            }

            if let Some(tok) = self.lex_rel_tokens() {
                return Some(self.emit_token(tok));
            }

            if let Some(tok) = self.lex_name_or_str() {
                return Some(self.emit_result(tok));
            }

            let start = self.pos;
            self.pos += 1;
            return Some(Err(LexError::new(
                format!("Unexpected character at {start}"),
                SourceSpan::new(start, self.pos),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorSemanticRole, MermaidConfig};

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "er".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn er_direction_lexer_preserves_ecmascript_line_terminators() {
        // Test raw lexer rules before Engine preprocessing normalizes line endings.
        for separator in ['\r', '\u{2028}', '\u{2029}'] {
            let source = format!("direction RL{separator}direction TB");
            let (start, token, end) = Lexer::new(&source).next().unwrap().unwrap();
            let Tok::Direction(direction) = token else {
                panic!("expected direction");
            };
            assert_eq!(direction.value, "RL");
            assert_eq!(direction.selection, SourceSpan::new(10, 12));
            assert_eq!((start, end), (0, source.len()));
        }
        let tokens = Lexer::new("A B C\ndirection BT")
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            matches!(&tokens.last().unwrap().1, Tok::Direction(direction) if direction.value == "BT")
        );
    }

    #[test]
    fn er_parent_marker_uses_only_the_following_character() {
        for source in ["u-table", "U.table", "u|", "\"A\"u--"] {
            let mut lexer = Lexer::new(source);
            if source.starts_with('"') {
                assert!(matches!(lexer.next().unwrap().unwrap().1, Tok::Name(_)));
            }
            assert!(
                matches!(lexer.next().unwrap().unwrap().1, Tok::MdParent),
                "{source}"
            );
        }
        for source in ["u", "user", "u_table"] {
            assert!(
                matches!(Lexer::new(source).next().unwrap().unwrap().1, Tok::Name(_)),
                "{source}"
            );
        }
    }

    #[test]
    fn er_typed_projection_matches_complete_compat_json() {
        let text = concat!(
            "erDiagram\n",
            "accTitle: Orders\n",
            "CUSTOMER ||--o{ ORDER : places\n",
        );
        let meta = meta();
        let compat = parse_er(text, &meta).unwrap();
        let typed = parse_er_model_for_render(text, &meta).unwrap();
        let projection = render_model_to_compat_json(&typed, &meta).unwrap();

        assert_eq!(projection, compat);
        assert_eq!(projection["type"], json!("er"));
        assert_eq!(projection["accDescr"], Value::Null);
        assert_eq!(
            projection["constants"]["cardinality"]["onlyOne"],
            json!("ONLY_ONE")
        );
    }

    #[test]
    fn er_accepts_decimal_and_dotted_entity_names() {
        let model = parse_er("erDiagram\n1.5 ||--|| Sales.Order : owns\n", &meta())
            .expect("ER entities with dots and decimal names should parse");

        assert!(model["entities"].get("1.5").is_some());
        assert!(model["entities"].get("Sales.Order").is_some());
        assert_eq!(model["relationships"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn er_typed_model_and_compat_json_preserve_declaration_order() {
        let text = concat!(
            "erDiagram\n",
            "ZETA\n",
            "ALPHA\n",
            "MIDDLE\n",
            "classDef zeta fill:#fff\n",
            "classDef alpha fill:#000\n",
            "classDef middle fill:#ccc\n",
        );
        let meta = meta();
        let typed = parse_er_model_for_render(text, &meta).expect("ER model should parse");
        assert!(
            typed
                .entities
                .values()
                .map(|entity| entity.label.as_str())
                .eq(["ZETA", "ALPHA", "MIDDLE"])
        );
        assert!(
            typed
                .classes
                .keys()
                .map(String::as_str)
                .eq(["zeta", "alpha", "middle"])
        );

        let compat = render_model_to_compat_json(&typed, &meta)
            .expect("ER compatibility projection should serialize");
        assert!(
            compat["entities"]
                .as_object()
                .expect("ER entities should be an object")
                .keys()
                .map(String::as_str)
                .eq(["ZETA", "ALPHA", "MIDDLE"])
        );
        assert!(
            compat["classes"]
                .as_object()
                .expect("ER classes should be an object")
                .keys()
                .map(String::as_str)
                .eq(["zeta", "alpha", "middle"])
        );

        let round_trip: ErDiagramRenderModel = serde_json::from_value(
            serde_json::to_value(&typed).expect("ER typed model should serialize"),
        )
        .expect("ER typed model should deserialize");
        assert!(
            round_trip
                .entities
                .values()
                .map(|entity| entity.label.as_str())
                .eq(["ZETA", "ALPHA", "MIDDLE"])
        );
        assert!(
            round_trip
                .classes
                .keys()
                .map(String::as_str)
                .eq(["zeta", "alpha", "middle"])
        );
    }

    #[test]
    fn er_entity_occurrence_role_does_not_depend_on_display_detail() {
        let span = SourceSpan::new(0, "CUSTOMER".len());
        let symbol = || ErTokenSymbol {
            name: "CUSTOMER".to_string(),
            span,
        };
        let mut collector = ErEditorFactCollector::default();
        let mut facts = EditorSemanticFacts::new();
        collector.push_entity_symbol(
            &mut facts,
            symbol(),
            "reference-looking definition",
            ErEntityOccurrence::Definition,
        );
        collector.push_entity_symbol(
            &mut facts,
            symbol(),
            "definition-looking relationship occurrence",
            ErEntityOccurrence::Relationship,
        );

        assert_eq!(facts.symbols[0].role, EditorSemanticRole::Entity);
        assert_eq!(facts.symbols[1].role, EditorSemanticRole::Reference);
    }

    #[test]
    fn er_subgraphs_preserve_membership_and_local_direction() {
        let text = concat!(
            "erDiagram\n",
            "direction LR\n",
            "subgraph Orders [Order Domain]\n",
            "  direction TB\n",
            "  CUSTOMER ||--o{ ORDER : places\n",
            "end\n",
            "WAREHOUSE ||--o{ Orders : ships\n",
        );
        let model = parse_er_model_for_render(text, &meta()).expect("ER subgraphs should parse");

        assert_eq!(model.direction, "LR");
        assert_eq!(model.subgraphs.len(), 1);
        let subgraph = &model.subgraphs[0];
        assert_eq!(subgraph.id, "Orders");
        assert_eq!(subgraph.title, "Order Domain");
        assert_eq!(subgraph.dir.as_deref(), Some("TB"));
        assert_eq!(subgraph.nodes, ["CUSTOMER", "ORDER"]);
        assert_eq!(model.entities.len(), 3);
        assert_eq!(model.relationships.len(), 2);
        assert_eq!(model.relationships[1].entity_b, "Orders");
    }

    #[test]
    fn er_subgraphs_support_nested_and_quoted_headers() {
        let text = concat!(
            "erDiagram\n",
            "subgraph \"Customer Domain\" [\"Customer %% Domain\"]\n",
            "  subgraph Billing [Billing Area]\n",
            "    INVOICE\n",
            "  end\n",
            "  CUSTOMER\n",
            "end\n",
        );
        let model = parse_er_model_for_render(text, &meta()).expect("nested ER subgraphs parse");

        assert_eq!(model.subgraphs.len(), 2);
        assert_eq!(model.subgraphs[0].id, "Billing");
        assert_eq!(model.subgraphs[0].title, "Billing Area");
        assert_eq!(model.subgraphs[1].id, "Customer Domain");
        assert_eq!(model.subgraphs[1].title, "Customer %% Domain");
        assert_eq!(model.subgraphs[1].nodes, ["Billing", "CUSTOMER"]);
        assert_eq!(model.subgraphs[0].nodes, ["INVOICE"]);
    }

    #[test]
    fn er_subgraph_styles_apply_after_the_subgraph_is_registered() {
        let text = concat!(
            "erDiagram\n",
            "subgraph Orders\n",
            "  ORDER\n",
            "end\n",
            "class Orders important\n",
            "style Orders fill:#eee,stroke:#333\n",
        );
        let model = parse_er_model_for_render(text, &meta()).expect("styled ER subgraph parses");
        let subgraph = &model.subgraphs[0];
        assert_eq!(subgraph.classes, ["important"]);
        assert_eq!(subgraph.css_styles, ["fill:#eee", "stroke:#333"]);
    }

    #[test]
    fn er_subgraph_header_may_follow_er_diagram_on_same_line() {
        let model = parse_er_model_for_render(
            "erDiagram subgraph WithRL\ndirection RL\nA\nB\nend\n",
            &meta(),
        )
        .expect("same-line ER subgraph parses");
        assert_eq!(model.subgraphs.len(), 1);
        assert_eq!(model.subgraphs[0].id, "WithRL");
        assert_eq!(model.subgraphs[0].dir.as_deref(), Some("RL"));
        assert_eq!(model.subgraphs[0].nodes, ["A", "B"]);
    }

    fn nested_subgraphs(depth: usize) -> String {
        let mut source = String::from("erDiagram\n");
        for index in 0..depth {
            source.push_str(&format!("subgraph G{index}\n"));
        }
        source.push_str("LEAF\n");
        for _ in 0..depth {
            source.push_str("end\n");
        }
        source
    }

    #[test]
    fn er_flat_action_storage_and_membership_work_grow_with_records() {
        for depth in [32, 128, 512] {
            let source = nested_subgraphs(depth);
            let control = OperationControl::new();
            let syntax = ErSyntax::lex(&source, &control).unwrap();
            let (_, parsed) = syntax
                .into_editor_facts_and_actions(&source, &control)
                .unwrap();
            let (arena, actions) = parsed.unwrap();
            assert_eq!(arena.records.len(), depth + 1);
            assert_eq!(actions.first, actions.last, "one top-level subgraph");

            let mut db = ErDb::new();
            db.replay(arena, actions, &control).unwrap();
            assert_eq!(db.subgraphs.len(), depth);
            assert_eq!(db.membership_candidates, depth);
            assert_eq!(db.completed_members.len(), depth);
            assert_eq!(db.subgraphs[0].nodes, ["LEAF"]);
            assert_eq!(db.subgraphs[depth - 1].nodes, ["G1"]);
        }
    }

    #[test]
    fn er_cancellation_during_one_deep_action_preserves_completed_inner_subgraphs() {
        let depth = 1024;
        let source = nested_subgraphs(depth);
        let parsing = OperationControl::new();
        let syntax = ErSyntax::lex(&source, &parsing).unwrap();
        let (_, parsed) = syntax
            .into_editor_facts_and_actions(&source, &parsing)
            .unwrap();
        let (arena, actions) = parsed.unwrap();
        assert_eq!(actions.first, actions.last, "one top-level action");

        let control = OperationControl::new();
        control.cancel_after_checkpoints(12);
        let mut db = ErDb::new();
        let cancelled = db.replay(arena, actions, &control).unwrap_err();
        assert_eq!(control.checkpoint(), Err(cancelled));
        assert!(
            !db.subgraphs.is_empty(),
            "inner completion started before cancellation"
        );
        assert!(
            db.subgraphs.len() < depth,
            "the outer action was interrupted"
        );
        assert_eq!(db.subgraphs[0].id, "G1023");
        assert_eq!(db.subgraphs[0].nodes, ["LEAF"]);
        drop(db);
        assert!(parse_er_model_for_render("erDiagram\nAFTER\n", &meta()).is_ok());
    }

    #[test]
    fn er_cancellation_during_token_delivery_retains_the_terminal_outcome() {
        let source = nested_subgraphs(64);
        let control = OperationControl::new();
        let syntax = ErSyntax::lex(&source, &control).unwrap();
        let fact_checkpoints = syntax.events.len().div_ceil(128) + 1;
        // Cancel during the third token batch, after inner grammar fragments have completed.
        control.cancel_after_checkpoints(fact_checkpoints + 2);
        let Err(cancelled) = syntax.into_editor_facts_and_actions(&source, &control) else {
            panic!("token delivery observes cancellation");
        };
        assert_eq!(control.checkpoint(), Err(cancelled));
    }
}

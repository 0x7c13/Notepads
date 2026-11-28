use super::lexer::{Token, TokenKind as K};
use super::*;
use crate::{
    EditorRenamePolicy, EditorSemanticKind, EditorSemanticRole, EditorSemanticSymbol,
    diagrams::scan::is_ecmascript_whitespace,
};

#[derive(Debug, Clone)]
pub(super) struct Label {
    pub text: String,
    pub kind: UsecaseLabelType,
    pub span: SourceSpan,
}
#[derive(Debug, Clone)]
pub(super) struct Property {
    pub key: String,
    pub value: Value,
    pub span: SourceSpan,
}
#[derive(Debug, Clone)]
pub(super) struct Entity {
    pub id: String,
    pub label: Label,
    pub span: SourceSpan,
    pub full_span: SourceSpan,
    pub generated: bool,
    pub declaration: bool,
    pub actor: bool,
    pub shape: Option<UsecaseNodeKind>,
    pub parent: Option<String>,
    pub metadata: Vec<Property>,
    pub stereotype: Option<String>,
    pub classes: Vec<String>,
}
#[derive(Debug)]
pub(super) struct Boundary {
    pub entity: Entity,
}
#[derive(Debug)]
pub(super) struct Relation {
    pub source: Entity,
    pub target: Entity,
    pub edge: UsecaseRelationship,
    pub id_span: Option<SourceSpan>,
    pub span: SourceSpan,
}
#[derive(Debug)]
pub(super) struct JsonDraft {
    pub node: UsecaseJsonNode,
    pub span: SourceSpan,
}
#[derive(Debug)]
pub(super) struct Assignment {
    pub target: String,
    pub span: SourceSpan,
    pub metadata: Vec<Property>,
}
#[derive(Debug)]
pub(super) struct ClassAssignment {
    pub targets: Vec<(String, SourceSpan)>,
    pub classes: Vec<String>,
}
#[derive(Debug)]
pub(super) struct StyleAssignment {
    pub target: String,
    pub span: SourceSpan,
    pub styles: Vec<String>,
}
#[derive(Debug)]
pub(super) struct Note {
    pub target: String,
    pub span: SourceSpan,
    pub label: Label,
}
#[derive(Default)]
pub(super) struct Draft {
    pub entities: Vec<Entity>,
    pub boundaries: Vec<Boundary>,
    pub relationships: Vec<Relation>,
    pub json: Vec<JsonDraft>,
    pub metadata: Vec<Assignment>,
    pub classes: Vec<ClassAssignment>,
    pub styles: Vec<StyleAssignment>,
    pub class_defs: Vec<UsecaseClassDef>,
    pub notes: Vec<Note>,
    pub direction: Option<String>,
    pub acc_title: Option<String>,
    pub acc_description: Option<String>,
}

type PResult<T> = std::result::Result<T, ParseIssue>;

pub(super) struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    at: usize,
    control: &'a OperationControl,
    pub draft: Draft,
    pub facts: EditorSemanticFacts,
    anonymous: usize,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str, tokens: Vec<Token>, control: &'a OperationControl) -> Self {
        let mut facts = EditorSemanticFacts::new();
        for keyword in [
            "actor",
            "systemBoundary",
            "direction",
            "note for",
            "json",
            "classDef",
            "class",
            "style",
            "accTitle:",
            "accDescr:",
        ] {
            facts.push_directive_prefix(keyword);
        }
        Self {
            source,
            tokens,
            at: 0,
            control,
            draft: Draft::default(),
            facts,
            anonymous: 0,
        }
    }
    pub fn parse(&mut self) -> OperationControlResult<PResult<()>> {
        if let Err(issue) = self.expect(K::Usecase).and_then(|_| self.line_end()) {
            return Ok(Err(issue));
        }
        while self.kind() != K::Eof {
            self.control.checkpoint()?;
            let result = self.statement();
            self.control.checkpoint()?;
            if let Err(issue) = result {
                return Ok(Err(issue));
            }
        }
        Ok(Ok(()))
    }
    fn checkpoint(&self) -> PResult<()> {
        self.control
            .checkpoint()
            .map_err(|_| ParseIssue::new("Operation cancelled", self.current().span))
    }
    fn current(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn kind(&self) -> K {
        self.current().kind
    }
    fn peek(&self, distance: usize) -> K {
        self.tokens
            .get(self.at + distance)
            .map_or(K::Eof, |token| token.kind)
    }
    fn take(&mut self) -> Token {
        let token = *self.current();
        if token.kind != K::Eof {
            self.at += 1;
        }
        token
    }
    fn text(&self, token: &Token) -> &'a str {
        token.text(self.source)
    }
    fn eat(&mut self, kind: K) -> bool {
        if self.kind() == kind {
            self.take();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, kind: K) -> PResult<Token> {
        if self.kind() == kind {
            Ok(self.take())
        } else {
            Err(ParseIssue::new(
                format!("Expected {kind:?}, found {:?}", self.text(self.current())),
                self.current().span,
            ))
        }
    }
    fn line_end(&mut self) -> PResult<()> {
        if self.kind() == K::Eof || self.eat(K::NewLine) {
            Ok(())
        } else {
            Err(ParseIssue::new(
                "Expected end of Usecase statement",
                self.current().span,
            ))
        }
    }
    fn statement(&mut self) -> PResult<()> {
        match self.kind() {
            K::NewLine => {
                self.take();
                Ok(())
            }
            K::Comment => {
                self.take();
                self.line_end()
            }
            K::AccTitleLine => {
                let token = self.take();
                self.draft.acc_title = Some(
                    self.text(&token)
                        .split_once(':')
                        .map_or("", |(_, value)| value)
                        .trim()
                        .to_string(),
                );
                self.line_end()
            }
            K::AccDescrLine | K::AccDescrBlock => {
                let token = self.take();
                let raw = self.text(&token);
                let value = if token.kind == K::AccDescrLine {
                    raw.split_once(':').map_or("", |(_, value)| value)
                } else {
                    raw.split_once('{')
                        .map_or("", |(_, value)| value.strip_suffix('}').unwrap_or(value))
                };
                self.draft.acc_description = Some(value.trim().to_string());
                self.line_end()
            }
            K::Direction => {
                self.take();
                let token = self.take();
                if !matches!(token.kind, K::Td | K::Tb | K::Bt | K::Lr | K::Rl) {
                    return Err(ParseIssue::new(
                        "Expected Usecase direction TD, TB, BT, LR, or RL",
                        token.span,
                    ));
                }
                self.draft.direction = Some(
                    if token.kind == K::Td {
                        "TB"
                    } else {
                        self.text(&token)
                    }
                    .to_string(),
                );
                self.line_end()
            }
            K::Actor => self.actor_statement(None, true),
            K::SystemBoundary => self.boundary(),
            K::Note => self.note(),
            K::JsonDeclarationStart => self.json(),
            K::ClassDef => self.class_def(),
            K::Class => self.class_assignment(),
            K::Style => self.style_assignment(),
            _ if matches!(
                self.kind(),
                K::Identifier | K::PlainString | K::MarkdownString
            ) && self.peek(1) == K::MetadataStart =>
            {
                let token = self.take();
                let (id, _, generated) = self.name(&token);
                let metadata = self.metadata()?;
                self.reference(&id, self.content_span(&token), generated);
                self.draft.metadata.push(Assignment {
                    target: id,
                    span: self.content_span(&token),
                    metadata,
                });
                self.line_end()
            }
            _ => {
                if self.kind() == K::Identifier
                    && super::is_forbidden_statement_identifier(self.text(self.current()))
                {
                    return Err(ParseIssue::new(
                        "PlantUML statements are not part of the Usecase grammar",
                        self.current().span,
                    ));
                }
                let mut source = self.entity(false, None)?;
                if matches!(self.kind(), K::Eof | K::NewLine) {
                    source.declaration = true;
                    source.shape.get_or_insert(UsecaseNodeKind::UseCaseEllipse);
                    self.record_entity(&source);
                    self.draft.entities.push(source);
                    self.line_end()
                } else {
                    self.relation(source)?;
                    self.line_end()
                }
            }
        }
    }
    fn actor_statement(&mut self, parent: Option<&str>, allow_relation: bool) -> PResult<()> {
        self.expect(K::Actor)?;
        let source = self.entity(true, parent)?;
        if allow_relation && !matches!(self.kind(), K::Comma | K::Eof | K::NewLine) {
            self.relation(source)?;
        } else {
            self.record_entity(&source);
            self.draft.entities.push(source);
            while self.eat(K::Comma) {
                let entity = self.entity(true, parent)?;
                self.record_entity(&entity);
                self.draft.entities.push(entity);
            }
        }
        self.line_end()
    }
    fn name(&self, token: &Token) -> (String, Label, bool) {
        let label = self.token_label(token);
        let generated = token.kind != K::Identifier;
        // JavaScript /\W/g operates on UTF-16 code units, including surrogate pairs.
        let id = if generated {
            label
                .text
                .encode_utf16()
                .map(|unit| {
                    if unit <= 127 && ((unit as u8).is_ascii_alphanumeric() || unit == 95) {
                        char::from(unit as u8)
                    } else {
                        '_'
                    }
                })
                .collect()
        } else {
            label.text.clone()
        };
        (id, label, generated)
    }
    fn content_span(&self, token: &Token) -> SourceSpan {
        let trim = match token.kind {
            K::MarkdownString => 2,
            K::PlainString => 1,
            _ => 0,
        };
        SourceSpan::new(token.span.start + trim, token.span.end - trim)
    }
    fn token_label(&self, token: &Token) -> Label {
        let span = self.content_span(token);
        Label {
            text: self.source[span.start..span.end].to_string(),
            kind: if token.kind == K::MarkdownString {
                UsecaseLabelType::Markdown
            } else {
                UsecaseLabelType::Text
            },
            span,
        }
    }
    fn label(&mut self) -> PResult<Label> {
        if matches!(self.kind(), K::PlainString | K::MarkdownString) {
            let token = self.take();
            return Ok(self.token_label(&token));
        }
        if !self.kind().is_label_text() {
            return Err(ParseIssue::new(
                "Expected Usecase label",
                self.current().span,
            ));
        }
        let start = self.take().span.start;
        while self.kind().is_label_text() {
            if self.at.is_multiple_of(128) {
                self.checkpoint()?;
            }
            self.take();
        }
        let span = SourceSpan::new(start, self.tokens[self.at - 1].span.end);
        Ok(Label {
            text: self.source[span.start..span.end].to_string(),
            kind: UsecaseLabelType::Text,
            span,
        })
    }
    fn entity(&mut self, actor: bool, parent: Option<&str>) -> PResult<Entity> {
        self.entity_name(actor, parent, true)
    }
    fn entity_name(
        &mut self,
        actor: bool,
        parent: Option<&str>,
        allow_stereotype: bool,
    ) -> PResult<Entity> {
        if !matches!(
            self.kind(),
            K::Identifier | K::PlainString | K::MarkdownString
        ) {
            return Err(ParseIssue::new(
                "Expected a Usecase identifier or quoted name",
                self.current().span,
            ));
        }
        let token = self.take();
        let (id, mut label, generated) = self.name(&token);
        let span = self.content_span(&token);
        let mut shape = None;
        if token.kind == K::Identifier
            && (self.kind() == K::LeftParen || (!actor && self.kind() == K::LeftBracket))
        {
            let open = self.take();
            label = self.label()?;
            self.expect(if open.kind == K::LeftParen {
                K::RightParen
            } else {
                K::RightBracket
            })?;
            shape = Some(if open.kind == K::LeftParen {
                UsecaseNodeKind::UseCaseEllipse
            } else {
                UsecaseNodeKind::UseCaseRect
            });
        }
        let has_metadata = self.kind() == K::MetadataStart;
        let metadata = if has_metadata {
            self.metadata()?
        } else {
            Vec::new()
        };
        let stereotype = if allow_stereotype && self.eat(K::StereotypeStart) {
            let token = self.expect(K::StereotypeText)?;
            let value = self
                .text(&token)
                .trim_matches(is_ecmascript_whitespace)
                .to_string();
            self.expect(K::StereotypeEnd)?;
            Some(value)
        } else {
            None
        };
        let classes = self.class_suffix()?;
        Ok(Entity {
            id,
            label,
            span,
            full_span: SourceSpan::new(token.span.start, self.tokens[self.at - 1].span.end),
            generated,
            declaration: actor || shape.is_some() || has_metadata || stereotype.is_some(),
            actor,
            shape,
            parent: parent
                .filter(|parent| !parent.is_empty())
                .map(str::to_owned),
            metadata,
            stereotype,
            classes,
        })
    }
    fn metadata(&mut self) -> PResult<Vec<Property>> {
        self.expect(K::MetadataStart)?;
        while self.eat(K::NewLine) {}
        let mut properties = Vec::new();
        if self.eat(K::RightBrace) {
            return Ok(properties);
        }
        loop {
            self.checkpoint()?;
            let key = self.take();
            if !matches!(key.kind, K::Identifier | K::PlainString) {
                return Err(ParseIssue::new("Expected metadata property name", key.span));
            }
            self.expect(K::Colon)?;
            let value = self.take();
            let decoded = match value.kind {
                K::True => Value::Bool(true),
                K::False => Value::Bool(false),
                K::Identifier | K::PlainString => Value::String(self.token_label(&value).text),
                _ => {
                    return Err(ParseIssue::new(
                        "Expected metadata identifier, string, or boolean",
                        value.span,
                    ));
                }
            };
            let property = Property {
                key: self.token_label(&key).text,
                value: decoded,
                span: self.content_span(&key),
            };
            self.facts.push_symbol(EditorSemanticSymbol::payload(
                property.key.clone(),
                None,
                EditorSemanticKind::Property,
                SourceSpan::new(key.span.start, value.span.end),
                self.content_span(&key),
            ));
            properties.push(property);
            if self.eat(K::RightBrace) {
                break;
            }
            let comma = self.eat(K::Comma);
            let mut newline = false;
            while self.eat(K::NewLine) {
                newline = true;
            }
            if !comma && newline {
                self.eat(K::Comma);
                while self.eat(K::NewLine) {}
            }
            if !comma && !newline {
                return Err(ParseIssue::new(
                    "Expected metadata separator or closing brace",
                    self.current().span,
                ));
            }
            if self.eat(K::RightBrace) {
                break;
            }
        }
        Ok(properties)
    }
    fn class_suffix(&mut self) -> PResult<Vec<String>> {
        let mut classes = Vec::new();
        if !self.eat(K::ClassSeparator) {
            return Ok(classes);
        }
        loop {
            let token = self.expect(K::Identifier)?;
            classes.push(self.text(&token).to_string());
            self.facts.push_symbol(EditorSemanticSymbol::payload(
                self.text(&token),
                Some("class".to_string()),
                EditorSemanticKind::Class,
                token.span,
                token.span,
            ));
            if !self.eat(K::Comma) {
                break;
            }
        }
        Ok(classes)
    }
    fn boundary(&mut self) -> PResult<()> {
        self.expect(K::SystemBoundary)?;
        // Boundary names have the entity name forms but no stereotype suffix.
        let mut entity = self.entity_name(false, None, false)?;
        entity.declaration = true;
        self.facts.push_symbol(
            EditorSemanticSymbol::new(
                entity.id.clone(),
                Some(entity.label.text.clone()),
                EditorSemanticKind::Namespace,
                entity.full_span,
                entity.span,
            )
            .with_rename_policy(if entity.generated {
                EditorRenamePolicy::None
            } else {
                EditorRenamePolicy::UsecaseIdentifier
            }),
        );
        if !entity.metadata.is_empty() {
            self.draft.metadata.push(Assignment {
                target: entity.id.clone(),
                span: entity.span,
                metadata: std::mem::take(&mut entity.metadata),
            });
        }
        self.line_end()?;
        let parent = entity.id.clone();
        self.draft.boundaries.push(Boundary { entity });
        while !matches!(self.kind(), K::End | K::Eof) {
            self.checkpoint()?;
            match self.kind() {
                K::NewLine => {
                    self.take();
                }
                K::Comment => {
                    self.take();
                    self.line_end()?;
                }
                K::Actor => self.actor_statement(Some(&parent), false)?,
                _ => {
                    let mut child = self.entity(false, Some(&parent))?;
                    child.declaration = true;
                    child.shape.get_or_insert(UsecaseNodeKind::UseCaseEllipse);
                    self.record_entity(&child);
                    self.draft.entities.push(child);
                    self.line_end()?;
                }
            }
        }
        self.expect(K::End)?;
        self.line_end()
    }
    fn has_label_right(&self, allowed: &[K]) -> bool {
        let label_token =
            |kind: K| kind.is_label_text() || matches!(kind, K::PlainString | K::MarkdownString);
        if !label_token(self.kind()) {
            return false;
        }
        for token in self.tokens.iter().skip(self.at + 1) {
            if allowed.contains(&token.kind) {
                return true;
            }
            if !label_token(token.kind) {
                return false;
            }
        }
        false
    }
    fn relation(&mut self, source: Entity) -> PResult<()> {
        let start = self.current().span.start;
        let explicit = if self.kind() == K::Identifier && self.peek(1) == K::At {
            let token = self.take();
            self.take();
            Some(token)
        } else {
            None
        };
        let first = self.take();
        let mut last = first;
        let mut label = None;
        let mut relation_type = UsecaseRelationshipType::Association;
        let mut arrow = match first.kind {
            K::ForwardSolid | K::Generalization | K::DependencyArrow => UsecaseArrowType::Point,
            K::BackwardSolid => UsecaseArrowType::ReversedPoint,
            K::MarkerlessSolid => UsecaseArrowType::Line,
            K::ForwardCircle => UsecaseArrowType::Circle,
            K::BackwardCircle => UsecaseArrowType::ReversedCircle,
            K::ForwardCross => UsecaseArrowType::Cross,
            K::BackwardCross => UsecaseArrowType::ReversedCross,
            _ => {
                return Err(ParseIssue::new(
                    "Expected a Usecase relationship operator",
                    first.span,
                ));
            }
        };
        if first.kind == K::DependencyArrow {
            self.expect(K::Colon)?;
            let token = self.take();
            relation_type = match token.kind {
                K::Include => UsecaseRelationshipType::Include,
                K::Extend => UsecaseRelationshipType::Extend,
                _ => {
                    return Err(ParseIssue::new(
                        "Dependency arrows require :include or :extend",
                        token.span,
                    ));
                }
            };
            label = Some(Label {
                text: self.text(&token).to_ascii_lowercase(),
                kind: UsecaseLabelType::Text,
                span: token.span,
            });
        } else if first.kind == K::Generalization {
            relation_type = UsecaseRelationshipType::Generalization;
        } else {
            let allowed: &[K] = if first.kind == K::MarkerlessSolid && self.text(&first) == "--" {
                &[
                    K::ForwardSolid,
                    K::MarkerlessSolid,
                    K::ForwardCircle,
                    K::ForwardCross,
                ]
            } else if (first.kind == K::BackwardSolid && self.text(&first) == "<--")
                || matches!(first.kind, K::BackwardCircle | K::BackwardCross)
            {
                &[K::MarkerlessSolid]
            } else {
                &[]
            };
            if !allowed.is_empty() && self.has_label_right(allowed) {
                label = Some(self.label()?);
                last = self.take();
                if !allowed.contains(&last.kind) {
                    return Err(ParseIssue::new(
                        "Invalid labeled relationship operator",
                        last.span,
                    ));
                }
                if first.kind == K::MarkerlessSolid {
                    arrow = match last.kind {
                        K::ForwardSolid => UsecaseArrowType::Point,
                        K::ForwardCircle => UsecaseArrowType::Circle,
                        K::ForwardCross => UsecaseArrowType::Cross,
                        _ => UsecaseArrowType::Line,
                    };
                }
            }
        }
        let target = self.entity(false, None)?;
        let minlen = if relation_type == UsecaseRelationshipType::Association
            && matches!(
                arrow,
                UsecaseArrowType::Point | UsecaseArrowType::ReversedPoint | UsecaseArrowType::Line
            ) {
            self.text(&last)
                .bytes()
                .filter(|byte| *byte == b'-')
                .count()
                .saturating_sub(1)
                .max(1)
        } else {
            1
        };
        let id = explicit
            .as_ref()
            .map(|token| self.text(token).to_string())
            .unwrap_or_else(|| {
                let id = format!("edge-{}", self.anonymous);
                self.anonymous += 1;
                id
            });
        if let Some(token) = &explicit {
            self.facts.push_symbol(
                EditorSemanticSymbol::new(
                    id.clone(),
                    Some("relationship".to_string()),
                    EditorSemanticKind::Event,
                    SourceSpan::new(start, target.full_span.end),
                    token.span,
                )
                .with_rename_policy(EditorRenamePolicy::UsecaseIdentifier),
            );
        }
        for entity in [&source, &target] {
            self.record_entity(entity);
            if entity.declaration {
                self.draft.entities.push(entity.clone());
            }
        }
        let edge = UsecaseRelationship {
            id,
            explicit_id: explicit.is_some(),
            source: source.id.clone(),
            target: target.id.clone(),
            relationship_type: relation_type,
            arrow_type: arrow,
            label: label.as_ref().map(|label| label.text.clone()),
            label_type: label.map(|label| label.kind),
            dotted: matches!(
                relation_type,
                UsecaseRelationshipType::Include | UsecaseRelationshipType::Extend
            ),
            minlen,
            classes: Vec::new(),
            styles: Vec::new(),
            animate: false,
            animation: None,
        };
        let span = SourceSpan::new(start, target.full_span.end);
        self.draft.relationships.push(Relation {
            source,
            target,
            edge,
            id_span: explicit.map(|token| token.span),
            span,
        });
        Ok(())
    }
    fn note(&mut self) -> PResult<()> {
        self.expect(K::Note)?;
        self.expect(K::For)?;
        let target = self.expect(K::Identifier)?;
        let label = self.label()?;
        self.reference(self.text(&target), target.span, false);
        self.draft.notes.push(Note {
            target: self.text(&target).to_string(),
            span: target.span,
            label,
        });
        self.line_end()
    }
    fn json(&mut self) -> PResult<()> {
        let opening = self.take();
        let raw = self.text(&opening);
        let rest = raw
            .strip_prefix("json")
            .unwrap_or(raw)
            .trim_start_matches([' ', '\t']);
        let len = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let id = rest[..len].to_string();
        let offset = opening.span.start + raw.len() - rest.len();
        let span = SourceSpan::new(offset, offset + len);
        let literal = self.expect(K::JsonObjectLiteral)?;
        let parsed_json = super::ordered_json::parse(self.text(&literal), literal.span.start)?;
        let classes = self.class_suffix()?;
        self.facts.push_symbol(
            EditorSemanticSymbol::new(
                id.clone(),
                Some("JSON".to_string()),
                EditorSemanticKind::Object,
                SourceSpan::new(opening.span.start, literal.span.end),
                span,
            )
            .with_rename_policy(EditorRenamePolicy::UsecaseIdentifier),
        );
        self.draft.json.push(JsonDraft {
            node: UsecaseJsonNode {
                id,
                value: parsed_json.value,
                string_encoding: parsed_json.string_encoding,
                property_order: parsed_json.property_order,
                non_finite_numbers: parsed_json.non_finite_numbers,
                classes,
                styles: Vec::new(),
            },
            span,
        });
        self.line_end()
    }
    fn identifier_list(&mut self) -> PResult<Vec<Token>> {
        let mut tokens = vec![self.expect(K::Identifier)?];
        while self.eat(K::Comma) {
            tokens.push(self.expect(K::Identifier)?);
        }
        Ok(tokens)
    }
    fn class_def(&mut self) -> PResult<()> {
        self.take();
        let names = self.identifier_list()?;
        let styles = self.styles()?;
        for token in names {
            self.facts
                .push_symbol(EditorSemanticSymbol::class_definition(
                    self.text(&token),
                    None,
                    EditorSemanticKind::Class,
                    token.span,
                    token.span,
                ));
            self.draft.class_defs.push(UsecaseClassDef {
                id: self.text(&token).to_string(),
                styles: styles.clone(),
            });
        }
        self.line_end()
    }
    fn class_assignment(&mut self) -> PResult<()> {
        self.take();
        let tokens = self.identifier_list()?;
        let names = self.identifier_list()?;
        let mut targets = Vec::new();
        for token in tokens {
            self.reference(self.text(&token), token.span, false);
            targets.push((self.text(&token).to_string(), token.span));
        }
        let classes = names
            .iter()
            .map(|token| self.text(token).to_string())
            .collect();
        self.draft
            .classes
            .push(ClassAssignment { targets, classes });
        self.line_end()
    }
    fn style_assignment(&mut self) -> PResult<()> {
        self.take();
        let target = self.expect(K::Identifier)?;
        let styles = self.styles()?;
        self.reference(self.text(&target), target.span, false);
        self.draft.styles.push(StyleAssignment {
            target: self.text(&target).to_string(),
            span: target.span,
            styles,
        });
        self.line_end()
    }
    fn styles(&mut self) -> PResult<Vec<String>> {
        let mut styles = Vec::new();
        loop {
            let token = self.take();
            let start = token.span.start;
            if token.kind == K::MarkerlessSolid {
                if !self.kind().is_word() {
                    return Err(ParseIssue::new(
                        "Expected custom CSS property",
                        self.current().span,
                    ));
                }
                self.take();
            } else if !token.kind.is_word() && token.kind != K::CssIdentifier {
                return Err(ParseIssue::new("Expected CSS property", token.span));
            }
            self.expect(K::Colon)?;
            if !self.kind().is_style_component() {
                return Err(ParseIssue::new("Expected CSS value", self.current().span));
            }
            while self.kind().is_style_component() {
                if self.at.is_multiple_of(128) {
                    self.checkpoint()?;
                }
                self.take();
            }
            let end = self.tokens[self.at - 1].span.end;
            styles.push(self.source[start..end].replace("\\,", ","));
            if !self.eat(K::Comma) {
                break;
            }
        }
        Ok(styles)
    }
    fn reference(&mut self, id: &str, span: SourceSpan, generated: bool) {
        self.facts.push_symbol(
            EditorSemanticSymbol::reference(id, None, EditorSemanticKind::Object, span, span)
                .with_rename_policy(if generated {
                    EditorRenamePolicy::None
                } else {
                    EditorRenamePolicy::UsecaseIdentifier
                }),
        );
    }
    fn record_entity(&mut self, entity: &Entity) {
        let role = if entity.declaration {
            EditorSemanticRole::Entity
        } else {
            EditorSemanticRole::Reference
        };
        self.facts.push_symbol(
            EditorSemanticSymbol::with_role(
                entity.id.clone(),
                Some(entity.label.text.clone()),
                if entity.actor {
                    EditorSemanticKind::Variable
                } else {
                    EditorSemanticKind::Function
                },
                role,
                entity.full_span,
                entity.span,
            )
            .with_rename_policy(if entity.generated {
                EditorRenamePolicy::None
            } else {
                EditorRenamePolicy::UsecaseIdentifier
            }),
        );
        if entity.label.span != entity.span {
            self.facts.push_symbol(EditorSemanticSymbol::payload(
                entity.label.text.clone(),
                Some("label".to_string()),
                EditorSemanticKind::String,
                entity.label.span,
                entity.label.span,
            ));
        }
    }
    pub fn finalize_editor_facts(&mut self) -> OperationControlResult<()> {
        use std::collections::{HashMap, HashSet};

        // Completed declarations own identity even if a later statement is incomplete. Do not
        // require a renderable model to bind references from the parser's recovery journal.
        let declarations =
            self.draft
                .entities
                .iter()
                .map(|entity| {
                    (
                        entity.id.as_str(),
                        if entity.actor {
                            EditorSemanticKind::Variable
                        } else {
                            EditorSemanticKind::Function
                        },
                    )
                })
                .chain(
                    self.draft.boundaries.iter().map(|boundary| {
                        (boundary.entity.id.as_str(), EditorSemanticKind::Namespace)
                    }),
                )
                .chain(
                    self.draft
                        .json
                        .iter()
                        .map(|json| (json.node.id.as_str(), EditorSemanticKind::Object)),
                )
                .chain(self.draft.relationships.iter().filter_map(|relation| {
                    relation
                        .edge
                        .explicit_id
                        .then_some((relation.edge.id.as_str(), EditorSemanticKind::Event))
                }));
        let mut kinds: HashMap<&str, Option<EditorSemanticKind>> = HashMap::new();
        for (id, kind) in declarations {
            self.control.checkpoint()?;
            kinds
                .entry(id)
                .and_modify(|previous| {
                    if *previous != Some(kind)
                        || matches!(kind, EditorSemanticKind::Object | EditorSemanticKind::Event)
                    {
                        *previous = None;
                    }
                })
                .or_insert(Some(kind));
        }
        // Only completed relationships can introduce implicit use cases. A dangling style or
        // note target must not become a declaration merely because parsing stopped later.
        for relation in &self.draft.relationships {
            self.control.checkpoint()?;
            for entity in [&relation.source, &relation.target] {
                kinds
                    .entry(entity.id.as_str())
                    .or_insert(Some(EditorSemanticKind::Function));
            }
        }
        let mut defined = HashSet::new();
        for symbol in &self.facts.symbols {
            self.control.checkpoint()?;
            if symbol.role == EditorSemanticRole::Entity {
                defined.insert(symbol.name.clone());
            }
        }
        for symbol in &mut self.facts.symbols {
            self.control.checkpoint()?;
            if !matches!(
                symbol.role,
                EditorSemanticRole::Entity | EditorSemanticRole::Reference
            ) {
                continue;
            }
            match kinds.get(symbol.name.as_str()) {
                Some(Some(kind)) => {
                    symbol.kind = *kind;
                    if symbol.role == EditorSemanticRole::Reference
                        && defined.insert(symbol.name.clone())
                    {
                        symbol.role = EditorSemanticRole::Entity;
                    }
                }
                Some(None) => {
                    // Conflicting kinds and duplicate JSON/edge declarations cannot be renamed together.
                    symbol.rename_policy = EditorRenamePolicy::None;
                }
                None => {}
            }
        }
        Ok(())
    }
}

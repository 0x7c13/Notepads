//! Deferred symbol resolution; implicit endpoints are created only after declarations.
use super::parse::{Draft, Entity, Property};
use super::*;
use std::collections::HashMap;

type PResult<T> = std::result::Result<T, ParseIssue>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Actor,
    Usecase,
    Boundary,
    Json,
    Edge,
}
#[derive(Clone)]
struct Origin {
    kind: Kind,
    span: SourceSpan,
    generated: bool,
}
struct State {
    entity: Entity,
    actor_type: Option<UsecaseActorType>,
    icon: Option<String>,
    business: Option<bool>,
}

pub(super) fn resolve(
    draft: &Draft,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<PResult<UsecaseDiagramRenderModel>> {
    control.checkpoint()?;
    let result = build(draft, meta, control);
    control.checkpoint()?;
    Ok(result)
}
fn checkpoint(control: &OperationControl, span: SourceSpan) -> PResult<()> {
    control
        .checkpoint()
        .map_err(|_| ParseIssue::new("Operation cancelled", span))
}
fn conflict(id: &str, issue: &str, span: SourceSpan, previous: SourceSpan) -> ParseIssue {
    ParseIssue::new(
        format!(
            "ID '{id}' {issue}; previous declaration at [{},{})",
            previous.start, previous.end
        ),
        span,
    )
}
fn unique(values: &mut Vec<String>, additions: &[String]) {
    for value in additions {
        if !values.contains(value) {
            values.push(value.clone());
        }
    }
}
fn kind(entity: &Entity) -> Kind {
    if entity.actor {
        Kind::Actor
    } else {
        Kind::Usecase
    }
}
fn register(
    symbols: &mut HashMap<String, Origin>,
    id: &str,
    kind: Kind,
    span: SourceSpan,
    generated: bool,
    merge: bool,
) -> PResult<()> {
    if let Some(previous) = symbols.get(id) {
        if previous.kind != kind {
            return Err(conflict(
                id,
                "is declared with conflicting kinds",
                span,
                previous.span,
            ));
        }
        if previous.generated || generated {
            return Err(conflict(
                id,
                "has a generated identifier collision",
                span,
                previous.span,
            ));
        }
        if !merge {
            return Err(conflict(
                id,
                "is declared more than once",
                span,
                previous.span,
            ));
        }
    } else {
        symbols.insert(
            id.to_string(),
            Origin {
                kind,
                span,
                generated,
            },
        );
    }
    Ok(())
}
fn metadata(state: &mut State, properties: &[Property], replace: bool) -> PResult<()> {
    for property in properties {
        let invalid = || {
            ParseIssue::new(
                format!(
                    "Metadata property '{}' is invalid for {} '{}'",
                    property.key,
                    if state.entity.actor {
                        "actor"
                    } else {
                        "usecase"
                    },
                    state.entity.id
                ),
                property.span,
            )
        };
        match property.key.as_str() {
            "business" if property.value.is_boolean() => {
                let value = property.value.as_bool().unwrap_or(false);
                if !replace && state.business.is_some_and(|old| old != value) {
                    return Err(conflict(
                        &state.entity.id,
                        "has conflicting business metadata",
                        property.span,
                        state.entity.span,
                    ));
                }
                state.business = Some(value);
            }
            "type" if state.entity.actor => {
                let value = match property.value.as_str() {
                    Some("normal") => UsecaseActorType::Normal,
                    Some("hollow") => UsecaseActorType::Hollow,
                    Some("awesome") => UsecaseActorType::Awesome,
                    _ => return Err(invalid()),
                };
                if !replace && state.actor_type.is_some_and(|old| old != value) {
                    return Err(conflict(
                        &state.entity.id,
                        "has conflicting type metadata",
                        property.span,
                        state.entity.span,
                    ));
                }
                state.actor_type = Some(value);
            }
            "icon" if state.entity.actor && property.value.is_string() => {
                let value = property.value.as_str().unwrap_or_default();
                if !replace && state.icon.as_deref().is_some_and(|old| old != value) {
                    return Err(conflict(
                        &state.entity.id,
                        "has conflicting icon metadata",
                        property.span,
                        state.entity.span,
                    ));
                }
                state.icon = Some(value.to_string());
            }
            _ => return Err(invalid()),
        }
    }
    Ok(())
}
fn collect(state: &mut State, entity: &Entity) -> PResult<()> {
    if state.entity.label.text != entity.label.text || state.entity.label.kind != entity.label.kind
    {
        return Err(conflict(
            &entity.id,
            "has conflicting labels",
            entity.span,
            state.entity.span,
        ));
    }
    if entity.shape.is_some() && state.entity.shape.is_some() && entity.shape != state.entity.shape
    {
        return Err(conflict(
            &entity.id,
            "has conflicting shapes",
            entity.span,
            state.entity.span,
        ));
    }
    if entity.parent.is_some()
        && state.entity.parent.is_some()
        && entity.parent != state.entity.parent
    {
        return Err(conflict(
            &entity.id,
            "belongs to more than one system boundary",
            entity.span,
            state.entity.span,
        ));
    }
    if entity.stereotype.is_some()
        && state.entity.stereotype.is_some()
        && entity.stereotype != state.entity.stereotype
    {
        return Err(conflict(
            &entity.id,
            "has conflicting stereotypes",
            entity.span,
            state.entity.span,
        ));
    }
    state.entity.shape = state.entity.shape.or(entity.shape);
    if state.entity.parent.is_none() {
        state.entity.parent.clone_from(&entity.parent);
    }
    if state.entity.stereotype.is_none() {
        state.entity.stereotype.clone_from(&entity.stereotype);
    }
    unique(&mut state.entity.classes, &entity.classes);
    metadata(state, &entity.metadata, false)
}

fn build(
    draft: &Draft,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> PResult<UsecaseDiagramRenderModel> {
    let mut symbols = HashMap::<String, Origin>::new();
    let mut elements = HashMap::<String, State>::new();
    let mut boundaries = Vec::<UsecaseBoundary>::new();
    let mut boundary_index = HashMap::<String, usize>::new();
    let mut first = HashMap::<String, usize>::new();
    let mut record_first = |id: &str, offset: usize| {
        first
            .entry(id.to_string())
            .and_modify(|old| *old = (*old).min(offset))
            .or_insert(offset);
    };
    for entity in &draft.entities {
        record_first(&entity.id, entity.span.start);
    }
    for boundary in &draft.boundaries {
        record_first(&boundary.entity.id, boundary.entity.span.start);
    }
    for json in &draft.json {
        record_first(&json.node.id, json.span.start);
    }
    for relation in &draft.relationships {
        record_first(&relation.source.id, relation.source.span.start);
        record_first(&relation.target.id, relation.target.span.start);
    }

    enum Declaration<'a> {
        Entity(&'a Entity),
        Boundary(&'a Entity),
        Json(&'a parse::JsonDraft),
        Edge(&'a parse::Relation),
    }
    let mut declarations: Vec<(usize, Declaration<'_>)> = draft
        .entities
        .iter()
        .map(|entity| (entity.span.start, Declaration::Entity(entity)))
        .collect();
    declarations.extend(draft.boundaries.iter().map(|boundary| {
        (
            boundary.entity.span.start,
            Declaration::Boundary(&boundary.entity),
        )
    }));
    declarations.extend(
        draft
            .json
            .iter()
            .map(|json| (json.span.start, Declaration::Json(json))),
    );
    declarations.extend(draft.relationships.iter().filter_map(|relation| {
        relation
            .id_span
            .map(|span| (span.start, Declaration::Edge(relation)))
    }));
    declarations.sort_by_key(|(offset, _)| *offset);
    for (_, declaration) in declarations {
        match declaration {
            Declaration::Entity(entity) => {
                checkpoint(control, entity.span)?;
                register(
                    &mut symbols,
                    &entity.id,
                    kind(entity),
                    entity.span,
                    entity.generated,
                    true,
                )?;
                if let Some(state) = elements.get_mut(&entity.id) {
                    collect(state, entity)?;
                } else {
                    let mut state = State {
                        entity: entity.clone(),
                        actor_type: None,
                        icon: None,
                        business: None,
                    };
                    metadata(&mut state, &entity.metadata, false)?;
                    elements.insert(entity.id.clone(), state);
                }
            }
            Declaration::Boundary(entity) => {
                checkpoint(control, entity.span)?;
                register(
                    &mut symbols,
                    &entity.id,
                    Kind::Boundary,
                    entity.span,
                    entity.generated,
                    true,
                )?;
                if let Some(index) = boundary_index.get(&entity.id).copied() {
                    let boundary = &mut boundaries[index];
                    if boundary.label != entity.label.text
                        || boundary.label_type != entity.label.kind
                    {
                        return Err(conflict(
                            &entity.id,
                            "has conflicting boundary titles",
                            entity.span,
                            symbols[&entity.id].span,
                        ));
                    }
                    unique(&mut boundary.classes, &entity.classes);
                } else {
                    boundary_index.insert(entity.id.clone(), boundaries.len());
                    boundaries.push(UsecaseBoundary {
                        id: entity.id.clone(),
                        label: entity.label.text.clone(),
                        label_type: entity.label.kind,
                        members: Vec::new(),
                        package: false,
                        classes: entity.classes.clone(),
                        styles: Vec::new(),
                    });
                }
            }
            Declaration::Json(json) => {
                checkpoint(control, json.span)?;
                register(
                    &mut symbols,
                    &json.node.id,
                    Kind::Json,
                    json.span,
                    false,
                    false,
                )?;
            }
            Declaration::Edge(relation) => {
                let span = relation.id_span.unwrap_or(relation.span);
                checkpoint(control, span)?;
                register(
                    &mut symbols,
                    &relation.edge.id,
                    Kind::Edge,
                    span,
                    false,
                    false,
                )?;
            }
        }
    }
    let mut relationships = Vec::new();
    let mut edge_index = HashMap::<String, usize>::new();
    for relation in &draft.relationships {
        checkpoint(control, relation.span)?;
        for endpoint in [&relation.source, &relation.target] {
            if !endpoint.declaration && !endpoint.classes.is_empty() {
                return Err(ParseIssue::new(
                    format!(
                        "Relationship endpoint '{}' uses ::: without declaring the node",
                        endpoint.id
                    ),
                    endpoint.span,
                ));
            }
            if !symbols.contains_key(&endpoint.id) {
                symbols.insert(
                    endpoint.id.clone(),
                    Origin {
                        kind: Kind::Usecase,
                        span: endpoint.span,
                        generated: endpoint.generated,
                    },
                );
                let mut entity = endpoint.clone();
                entity.shape = Some(UsecaseNodeKind::UseCaseEllipse);
                elements.insert(
                    endpoint.id.clone(),
                    State {
                        entity,
                        actor_type: None,
                        icon: None,
                        business: None,
                    },
                );
            }
        }
        let source = symbols[&relation.source.id].kind;
        let target = symbols[&relation.target.id].kind;
        if !matches!(source, Kind::Actor | Kind::Usecase | Kind::Json) {
            return Err(ParseIssue::new(
                format!(
                    "Relationship source '{}' cannot be {source:?}",
                    relation.source.id
                ),
                relation.source.span,
            ));
        }
        if !matches!(target, Kind::Actor | Kind::Usecase | Kind::Json) {
            return Err(ParseIssue::new(
                format!(
                    "Relationship target '{}' cannot be {target:?}",
                    relation.target.id
                ),
                relation.target.span,
            ));
        }
        match relation.edge.relationship_type {
            UsecaseRelationshipType::Include | UsecaseRelationshipType::Extend
                if source != Kind::Usecase || target != Kind::Usecase =>
            {
                return Err(ParseIssue::new(
                    "Include and extend relationships require use-case endpoints",
                    relation.span,
                ));
            }
            UsecaseRelationshipType::Generalization
                if !matches!(source, Kind::Actor | Kind::Usecase) || source != target =>
            {
                return Err(ParseIssue::new(
                    "Generalization requires actor-to-actor or use-case-to-use-case endpoints",
                    relation.span,
                ));
            }
            UsecaseRelationshipType::Association
                if (source == Kind::Json || target == Kind::Json)
                    && !matches!(
                        relation.edge.arrow_type,
                        UsecaseArrowType::Point
                            | UsecaseArrowType::ReversedPoint
                            | UsecaseArrowType::Line
                    ) =>
            {
                return Err(ParseIssue::new(
                    "JSON relationships permit only point, reversed-point, or markerless solid associations",
                    relation.span,
                ));
            }
            _ => {}
        }
        edge_index.insert(relation.edge.id.clone(), relationships.len());
        relationships.push(relation.edge.clone());
    }
    for assignment in &draft.metadata {
        checkpoint(control, assignment.span)?;
        let Some(origin) = symbols.get(&assignment.target) else {
            return Err(ParseIssue::new(
                format!("Metadata target '{}' is unresolved", assignment.target),
                assignment.span,
            ));
        };
        match origin.kind {
            Kind::Actor | Kind::Usecase => {
                if let Some(state) = elements.get_mut(&assignment.target) {
                    metadata(state, &assignment.metadata, true)?;
                }
            }
            Kind::Boundary => {
                let boundary = &mut boundaries[boundary_index[&assignment.target]];
                for property in &assignment.metadata {
                    match (property.key.as_str(), property.value.as_str()) {
                        ("type", Some("rect")) => boundary.package = false,
                        ("type", Some("package")) => boundary.package = true,
                        _ => {
                            return Err(ParseIssue::new(
                                format!(
                                    "Metadata property '{}' is invalid for boundary '{}'",
                                    property.key, assignment.target
                                ),
                                property.span,
                            ));
                        }
                    }
                }
            }
            Kind::Edge => {
                let edge = &mut relationships[edge_index[&assignment.target]];
                for property in &assignment.metadata {
                    match property.key.as_str() {
                        "animate" if property.value.is_boolean() => {
                            edge.animate = property.value.as_bool().unwrap_or(false)
                        }
                        "animation" if matches!(property.value.as_str(), Some("fast" | "slow")) => {
                            edge.animation = Some(if property.value.as_str() == Some("fast") {
                                UsecaseAnimation::Fast
                            } else {
                                UsecaseAnimation::Slow
                            });
                            edge.animate = true;
                        }
                        _ => {
                            return Err(ParseIssue::new(
                                format!(
                                    "Metadata property '{}' is invalid for edge '{}'",
                                    property.key, assignment.target
                                ),
                                property.span,
                            ));
                        }
                    }
                }
            }
            Kind::Json => {
                if let Some(property) = assignment.metadata.first() {
                    return Err(ParseIssue::new(
                        format!(
                            "Metadata property '{}' is invalid for JSON '{}'",
                            property.key, assignment.target
                        ),
                        property.span,
                    ));
                }
            }
        }
    }
    for edge in &mut relationships {
        if edge.animation.is_some() {
            edge.animate = true;
        }
    }
    let mut states: Vec<_> = elements.into_values().collect();
    states.sort_by_key(|state| {
        (
            !state.entity.actor,
            first.get(&state.entity.id).copied().unwrap_or(0),
        )
    });
    let mut nodes = Vec::with_capacity(states.len());
    for state in states {
        let entity = state.entity;
        checkpoint(control, entity.span)?;
        let has_icon = state.icon.as_ref().is_some_and(|icon| !icon.is_empty());
        let actor_type = if has_icon {
            UsecaseActorType::Icon
        } else {
            state.actor_type.unwrap_or_default()
        };
        let business = state.business.unwrap_or(false);
        if entity.actor {
            if has_icon
                && state
                    .actor_type
                    .is_some_and(|kind| kind != UsecaseActorType::Normal)
            {
                return Err(ParseIssue::new(
                    format!(
                        "Actor '{}' cannot combine icon with a non-normal type",
                        entity.id
                    ),
                    entity.span,
                ));
            }
            if business
                && matches!(
                    actor_type,
                    UsecaseActorType::Icon | UsecaseActorType::Awesome
                )
            {
                return Err(ParseIssue::new(
                    format!(
                        "Business actor '{}' must use normal or hollow geometry",
                        entity.id
                    ),
                    entity.span,
                ));
            }
        } else if business && entity.shape == Some(UsecaseNodeKind::UseCaseRect) {
            return Err(ParseIssue::new(
                format!(
                    "Rectangular use case '{}' cannot be a business use case",
                    entity.id
                ),
                entity.span,
            ));
        }
        nodes.push(UsecaseNode {
            id: entity.id,
            label: entity.label.text,
            label_type: entity.label.kind,
            kind: if entity.actor {
                UsecaseNodeKind::Actor
            } else {
                entity.shape.unwrap_or(UsecaseNodeKind::UseCaseEllipse)
            },
            actor_type,
            icon: state.icon.filter(|icon| !icon.is_empty()),
            parent_id: entity.parent,
            business,
            stereotype: entity.stereotype,
            classes: entity.classes,
            styles: Vec::new(),
        });
    }
    // Membership follows declarations, not hash order or first relationship references.
    for entity in &draft.entities {
        if let Some(parent) = &entity.parent {
            let Some(index) = boundary_index.get(parent).copied() else {
                return Err(ParseIssue::new(
                    format!("Parent boundary '{parent}' is unresolved"),
                    entity.span,
                ));
            };
            unique(
                &mut boundaries[index].members,
                std::slice::from_ref(&entity.id),
            );
        }
    }
    boundaries.sort_by_key(|boundary| first.get(&boundary.id).copied().unwrap_or(0));
    let mut json_nodes: Vec<_> = draft.json.iter().map(|json| json.node.clone()).collect();
    json_nodes.sort_by_key(|json| first.get(&json.id).copied().unwrap_or(0));
    let mut notes = Vec::with_capacity(draft.notes.len());
    for note in &draft.notes {
        checkpoint(control, note.span)?;
        match symbols.get(&note.target).map(|origin| origin.kind) {
            Some(Kind::Actor | Kind::Usecase) => {}
            None => {
                return Err(ParseIssue::new(
                    format!("Note target '{}' is unresolved", note.target),
                    note.span,
                ));
            }
            Some(_) => {
                return Err(ParseIssue::new(
                    format!("Note target '{}' must be an actor or use case", note.target),
                    note.span,
                ));
            }
        }
        notes.push(UsecaseNote {
            id: format!("note-{}", notes.len()),
            target: note.target.clone(),
            label: note.label.text.clone(),
            label_type: note.label.kind,
        });
    }
    let mut class_defs = Vec::<UsecaseClassDef>::new();
    let mut classes = HashMap::<String, usize>::new();
    for definition in &draft.class_defs {
        if let Some(index) = classes.get(&definition.id).copied() {
            class_defs[index].styles.clone_from(&definition.styles);
        } else {
            classes.insert(definition.id.clone(), class_defs.len());
            class_defs.push(definition.clone());
        }
    }
    let mut model = UsecaseDiagramRenderModel {
        direction: draft.direction.clone().unwrap_or_else(|| "LR".to_string()),
        nodes,
        boundaries,
        relationships,
        notes,
        json_nodes,
        class_defs,
        title: meta.title.clone(),
        acc_title: draft.acc_title.clone(),
        acc_description: draft.acc_description.clone(),
    };
    // Separate symbol and target maps keep anonymous edges out of class/style resolution.
    let targets = target_index(&model);
    for assignment in &draft.classes {
        for (id, span) in &assignment.targets {
            checkpoint(control, *span)?;
            let (classes, _) = stylable(&mut model, &targets, id, *span)?;
            unique(classes, &assignment.classes);
        }
    }
    for assignment in &draft.styles {
        checkpoint(control, assignment.span)?;
        let (_, styles) = stylable(&mut model, &targets, &assignment.target, assignment.span)?;
        styles.extend(assignment.styles.iter().cloned());
    }
    Ok(model)
}
fn target_index(model: &UsecaseDiagramRenderModel) -> HashMap<String, (Kind, usize)> {
    let mut targets: HashMap<_, _> = model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), (Kind::Usecase, index)))
        .collect();
    targets.extend(
        model
            .boundaries
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), (Kind::Boundary, index))),
    );
    targets.extend(
        model
            .json_nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), (Kind::Json, index))),
    );
    targets.extend(
        model
            .relationships
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.explicit_id)
            .map(|(index, edge)| (edge.id.clone(), (Kind::Edge, index))),
    );
    targets
}
fn stylable<'a>(
    model: &'a mut UsecaseDiagramRenderModel,
    targets: &HashMap<String, (Kind, usize)>,
    id: &str,
    span: SourceSpan,
) -> PResult<(&'a mut Vec<String>, &'a mut Vec<String>)> {
    let Some((kind, index)) = targets.get(id).copied() else {
        return Err(ParseIssue::new(
            format!("Class/style target '{id}' is unresolved or anonymous"),
            span,
        ));
    };
    Ok(match kind {
        Kind::Actor | Kind::Usecase => {
            let node = &mut model.nodes[index];
            (&mut node.classes, &mut node.styles)
        }
        Kind::Boundary => {
            let node = &mut model.boundaries[index];
            (&mut node.classes, &mut node.styles)
        }
        Kind::Json => {
            let node = &mut model.json_nodes[index];
            (&mut node.classes, &mut node.styles)
        }
        Kind::Edge => {
            let edge = &mut model.relationships[index];
            (&mut edge.classes, &mut edge.styles)
        }
    })
}

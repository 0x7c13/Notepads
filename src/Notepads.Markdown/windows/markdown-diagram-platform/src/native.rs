// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! The private, synchronous whole-diagram path. The outer native worker owns
//! thread/stack admission, captured COM capability and final artifact transport.

use crate::{NativeTheme, WindowsText};
use markdown_memory::{MemoryScope, NativeRegion};
use markdown_mermaid::{
    DiagramError,
    accessibility::{MAX_DIAGRAM_SOURCE_BYTES, mask_metadata},
    canonical::{
        ArtifactFacts, CanonicalDocument, CanonicalError, CanonicalLimits, accept_artifact,
    },
    layout::{LayoutPolicy, ThemePolicy, render_flowchart_native_controlled},
    native_pie::render_pie_native_controlled,
    native_relationships::{RelationshipDiagram, render_relationship_native_controlled},
    native_sequence::render_sequence_native_controlled,
    native_state::render_state_native_controlled,
    scene::{Color, Paint},
};
use merman_core::{
    CancelReason, NativeParseError, NativeParsePolicy, OperationControl,
    parse_flowchart_native_controlled,
};

pub fn render_native(
    source: &str,
    scope: &MemoryScope,
    control: &OperationControl,
    fonts: &WindowsText,
    theme: NativeTheme,
) -> Result<CanonicalDocument, DiagramError> {
    let result = render(source, scope, control, fonts, theme);
    if result.is_err() {
        control.cancel();
    }
    result
}

fn render(
    source: &str,
    scope: &MemoryScope,
    control: &OperationControl,
    fonts: &WindowsText,
    theme: NativeTheme,
) -> Result<CanonicalDocument, DiagramError> {
    if !fonts.same_scope(scope)
        || control
            .memory_scope()
            .is_none_or(|bound| !bound.same_root(scope))
    {
        return Err(DiagramError::InvalidFunding);
    }
    active(scope, control)?;
    fonts
        .checkpoint()
        .map_err(|e| canonical(e, scope, control))?;
    if source.len() > MAX_DIAGRAM_SOURCE_BYTES {
        return Err(DiagramError::WorkLimited);
    }
    // Receipt slots themselves are exact funded storage. This is a hard limit,
    // not an estimate of hidden payload bytes and not another authority.
    let region = NativeRegion::try_new(scope, 16384).map_err(DiagramError::Memory)?;
    let control = control.with_memory_region(region);
    let source = source.trim_start_matches('\u{feff}');
    let masked = mask_metadata(source, &control)?;
    let source = masked.as_ref().map_or(source, |text| text.as_str());
    let mut policy = LayoutPolicy::default();
    policy.font.epoch = fonts.font_epoch();
    let theme = ThemePolicy {
        node: Paint {
            fill: Some(color(theme.panel)),
            stroke: Some(color(theme.border)),
            ..Paint::default()
        },
        group: Paint {
            fill: Some(color(theme.background)),
            stroke: Some(color(theme.border)),
            ..Paint::default()
        },
        edge: Paint {
            fill: None,
            stroke: Some(color(theme.link)),
            ..Paint::default()
        },
        text: color(theme.foreground),
        background: color(theme.background),
    };
    let family = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .and_then(|line| line.split(|c: char| c.is_whitespace() || c == ';').next())
        .unwrap_or("");
    let scene = match family {
        "sequenceDiagram" => {
            render_sequence_native_controlled(source, policy, theme, fonts, &control)?
        }
        "stateDiagram" | "stateDiagram-v2" => {
            render_state_native_controlled(source, policy, theme, fonts, &control)?
        }
        "pie" => render_pie_native_controlled(source, policy, theme, fonts, &control)?,
        "classDiagram" | "classDiagram-v2" => render_relationship_native_controlled(
            source,
            RelationshipDiagram::Class,
            policy,
            theme,
            fonts,
            &control,
        )?,
        "erDiagram" => render_relationship_native_controlled(
            source,
            RelationshipDiagram::EntityRelationship,
            policy,
            theme,
            fonts,
            &control,
        )?,
        _ => {
            let input =
                parse_flowchart_native_controlled(source, &NativeParsePolicy::default(), &control)
                    .map_err(|error| match error {
                        NativeParseError::Memory(error) => DiagramError::Memory(error),
                        NativeParseError::Cancelled => stopped(scope, &control),
                        NativeParseError::Syntax { .. } => DiagramError::InvalidInput,
                        NativeParseError::Unsupported { .. } => DiagramError::Unsupported,
                        NativeParseError::Model => DiagramError::Backend,
                    })?;
            render_flowchart_native_controlled(&input, policy, theme, fonts, &control)
                .map_err(|error| canonical(error, scope, &control))?
        }
    };
    active(scope, &control)?;
    let facts = ArtifactFacts {
        bounds: scene.bounds,
        paths: scene.paths,
        commands: scene.commands,
    };
    let document = accept_artifact(
        scene.svg,
        facts,
        scope,
        &control,
        CanonicalLimits::default(),
    )
    .map_err(|e| canonical(e, scope, &control))?;
    active(scope, &control)?;
    Ok(document)
}

fn color(rgba: [u8; 4]) -> Color {
    Color::rgba(
        (rgba[0] as u32) << 16 | (rgba[1] as u32) << 8 | rgba[2] as u32,
        rgba[3],
    )
}
fn active(scope: &MemoryScope, control: &OperationControl) -> Result<(), DiagramError> {
    if let Some(error) = scope.failure() {
        return Err(DiagramError::Memory(error));
    }
    control.checkpoint().map_err(|_| stopped(scope, control))
}
fn stopped(scope: &MemoryScope, control: &OperationControl) -> DiagramError {
    if let Some(error) = scope.failure() {
        return DiagramError::Memory(error);
    }
    match control.checkpoint() {
        Err(error) if error.reason == CancelReason::DeadlineExceeded => DiagramError::Deadline,
        _ => DiagramError::Cancelled,
    }
}
fn canonical(
    error: CanonicalError,
    scope: &MemoryScope,
    control: &OperationControl,
) -> DiagramError {
    match error {
        CanonicalError::Memory(error) => DiagramError::Memory(error),
        CanonicalError::Cancelled => stopped(scope, control),
        CanonicalError::Limited => DiagramError::WorkLimited,
        CanonicalError::UnsupportedElement
        | CanonicalError::UnsupportedProperty
        | CanonicalError::UnsupportedCss => DiagramError::Unsupported,
        CanonicalError::FontUnavailable => DiagramError::FontUnavailable,
        CanonicalError::InvalidGeometry => DiagramError::InvalidGeometry,
        CanonicalError::MalformedXml | CanonicalError::InvalidReference => DiagramError::Backend,
    }
}

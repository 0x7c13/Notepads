use super::pipeline::{
    ScopedCssPostprocessor, SvgPipeline, SvgPostprocessExecution, SvgPostprocessMetadata,
};
use crate::environment::{RenderSession, RoutedTextMeasurer, TextMeasurementPhase};
#[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
use crate::model::ArchitectureDiagramLayout;
use crate::model::*;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use crate::{Error, Result};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use base64::Engine as _;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use indexmap::IndexMap;
use merman_core::OperationPhase;
use std::fmt::Write as _;

pub(crate) const C4_PERSON_IMG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADAAAAAwCAIAAADYYG7QAAACD0lEQVR4Xu2YoU4EMRCGT+4j8Ai8AhaH4QHgAUjQuFMECUgMIUgwJAgMhgQsAYUiJCiQIBBY+EITsjfTdme6V24v4c8vyGbb+ZjOtN0bNcvjQXmkH83WvYBWto6PLm6v7p7uH1/w2fXD+PBycX1Pv2l3IdDm/vn7x+dXQiAubRzoURa7gRZWd0iGRIiJbOnhnfYBQZNJjNbuyY2eJG8fkDE3bbG4ep6MHUAsgYxmE3nVs6VsBWJSGccsOlFPmLIViMzLOB7pCVO2AtHJMohH7Fh6zqitQK7m0rJvAVYgGcEpe//PLdDz65sM4pF9N7ICcXDKIB5Nv6j7tD0NoSdM2QrU9Gg0ewE1LqBhHR3BBdvj2vapnidjHxD/q6vd7Pvhr31AwcY8eXMTXAKECZZJFXuEq27aLgQK5uLMohCenGGuGewOxSjBvYBqeG6B+Nqiblggdjnc+ZXDy+FNFpFzw76O3UBAROuXh6FoiAcf5g9eTvUgzy0nWg6I8cXHRUpg5bOVBCo+KDpFajOf23GgPme7RSQ+lacIENUgJ6gg1k6HjgOlqnLqip4tEuhv0hNEMXUD0clyXE3p6pZA0S2nnvTlXwLJEZWlb7cTQH1+USgTN4VhAenm/wea1OCAOmqo6fE1WCb9WSKBah+rbUWPWAmE2Rvk0ApiB45eOyNAzU8xcTvj8KvkKEoOaIYeHNA3ZuygAvFMUO0AAAAASUVORK5CYII=";
pub(crate) const C4_EXTERNAL_PERSON_IMG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADAAAAAwCAIAAADYYG7QAAAB6ElEQVR4Xu2YLY+EMBCG9+dWr0aj0Wg0Go1Go0+j8Xdv2uTCvv1gpt0ebHKPuhDaeW4605Z9mJvx4AdXUyTUdd08z+u6flmWZRnHsWkafk9DptAwDPu+f0eAYtu2PEaGWuj5fCIZrBAC2eLBAnRCsEkkxmeaJp7iDJ2QMDdHsLg8SxKFEJaAo8lAXnmuOFIhTMpxxKATebo4UiFknuNo4OniSIXQyRxEA3YsnjGCVEjVXD7yLUAqxBGUyPv/Y4W2beMgGuS7kVQIBycH0fD+oi5pezQETxdHKmQKGk1eQEYldK+jw5GxPfZ9z7Mk0Qnhf1W1m3w//EUn5BDmSZsbR44QQLBEqrBHqOrmSKaQAxdnLArCrxZcM7A7ZKs4ioRq8LFC+NpC3WCBJsvpVw5edm9iEXFuyNfxXAgSwfrFQ1c0iNda8AdejvUgnktOtJQQxmcfFzGglc5WVCj7oDgFqU18boeFSs52CUh8LE8BIVQDT1ABrB0HtgSEYlX5doJnCwv9TXocKCaKbnwhdDKPq4lf3SwU3HLq4V/+WYhHVMa/3b4IlfyikAduCkcBc7mQ3/z/Qq/cTuikhkzB12Ae/mcJC9U+Vo8Ej1gWAtgbeGgFsAMHr50BIWOLCbezvhpBFUdY6EJuJ/QDW0XoMX60zZ0AAAAASUVORK5CYII=";

#[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
mod architecture;
#[cfg(feature = "diagram-block")]
mod block;
#[cfg(feature = "diagram-c4")]
mod c4;
#[cfg(feature = "diagram-class")]
mod class;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod css;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod curve;
#[cfg(feature = "diagram-cynefin")]
mod cynefin;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod edge_label_geometry;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod edge_path;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod emitted_bounds;
#[cfg(feature = "diagram-er")]
mod er;
mod error;
#[cfg(feature = "diagram-event-modeling")]
mod eventmodeling;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
mod flowchart;
#[cfg(feature = "diagram-gantt")]
mod gantt;
#[cfg(feature = "diagram-git-graph")]
mod gitgraph;
#[cfg(feature = "diagram-info")]
mod info;
#[cfg(feature = "diagram-ishikawa")]
mod ishikawa;
#[cfg(feature = "diagram-journey")]
mod journey;
#[cfg(feature = "diagram-kanban")]
mod kanban;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod label;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod layout_debug;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod line_hops;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod look_defs;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod markers;
#[cfg(feature = "diagram-mindmap")]
mod mindmap;
#[cfg(feature = "diagram-packet")]
mod packet;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod path_bounds;
#[cfg(feature = "diagram-pie")]
mod pie;
#[cfg(feature = "diagram-quadrant-chart")]
mod quadrantchart;
#[cfg(feature = "diagram-radar")]
mod radar;
#[cfg(feature = "diagram-railroad")]
mod railroad;
#[cfg(feature = "diagram-requirement")]
mod requirement;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod root_svg;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod roughjs_common;
#[cfg(feature = "diagram-sankey")]
mod sankey;
#[cfg(feature = "diagram-sequence")]
mod sequence;
#[cfg(feature = "diagram-state")]
mod state;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod style;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
pub(crate) mod theme;
#[cfg(feature = "diagram-timeline")]
mod timeline;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod timing;
#[cfg(feature = "diagram-tree-view")]
mod tree_view;
#[cfg(feature = "diagram-treemap")]
mod treemap;
#[cfg(feature = "diagram-usecase")]
mod usecase;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod util;
#[cfg(feature = "diagram-venn")]
mod venn;
#[cfg(feature = "diagram-wardley")]
mod wardley;
#[cfg(feature = "diagram-xychart")]
mod xychart;
#[cfg(feature = "diagram-zenuml")]
mod zenuml;
#[cfg(feature = "diagram-pie")]
use css::PieCss;
#[cfg(feature = "diagram-er")]
use css::er_css;
#[cfg(feature = "diagram-gantt")]
use css::gantt_css;
#[cfg(feature = "diagram-xychart")]
use css::push_xychart_css;
#[cfg(feature = "diagram-requirement")]
use css::requirement_css;
#[cfg(feature = "diagram-sankey")]
use css::sankey_css;
#[cfg(feature = "diagram-treemap")]
use css::treemap_css;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use css::{
    info_css_parts_with_config, info_css_parts_with_theme_font_size_only, info_css_with_config,
};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use path_bounds::{svg_path_bounds_from_d, svg_path_length_from_d};
#[cfg(feature = "diagram-mindmap")]
pub(crate) fn mindmap_cloud_rendered_bbox_size_px(w: f64, h: f64) -> Option<(f64, f64)> {
    mindmap::mindmap_cloud_rendered_bbox_size_px(w, h)
}

pub use emitted_bounds::{
    SvgEmittedBoundsContributor, SvgEmittedBoundsDebug, debug_svg_emitted_bounds,
};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use emitted_bounds::{svg_emitted_bounds_from_svg, svg_emitted_bounds_from_svg_inner};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use roughjs_common::{ops_to_svg_path_d as roughjs_ops_to_svg_path_d, roughjs_paths_for_rect};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use style::{is_rect_style_key, is_text_style_key, parse_style_decl};
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use theme::PresentationTheme;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        unused_imports,
        reason = "Common SVG imports are consumed by the selected family emitters."
    )
)]
use util::{
    SvgTheme, config_bool, config_diagram_look, config_f64, config_f64_css_px, config_string,
    css_rgba_fade, decode_mermaid_entities_for_render_text, escape_attr, escape_attr_display,
    escape_attr_into, escape_xml, escape_xml_display, escape_xml_into, fmt, fmt_display, fmt_into,
    fmt_path, fmt_path_into, fmt_points, fmt_string, json_stringify_points,
    json_stringify_points_into, normalize_css_font_family, scoped_drop_shadow, scoped_svg_id,
    scoped_svg_url, theme_token,
};

/// Converts arbitrary host input into the conservative SVG/CSS identifier grammar used by every
/// family renderer.
///
/// The result is safe to interpolate directly after `#` in generated stylesheets without CSS
/// escaping changing its selector meaning.
pub fn sanitize_svg_id(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "m-untitled".to_string();
    }

    let mut iter = raw.chars();
    let Some(first_raw) = iter.next() else {
        return "m-untitled".to_string();
    };

    let sanitize_char = |ch: char| {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            ch
        } else {
            '-'
        }
    };
    let first = sanitize_char(first_raw);
    let mut out = String::with_capacity(raw.len() + 2);
    let mut previous_was_dash = false;

    if !first.is_ascii_alphabetic() {
        out.push('m');
        if first != '-' {
            out.push('-');
            previous_was_dash = true;
        }
    }

    let push = |ch: char, out: &mut String, previous_was_dash: &mut bool| {
        if ch == '-' {
            if *previous_was_dash {
                return;
            }
            *previous_was_dash = true;
        } else {
            *previous_was_dash = false;
        }
        out.push(ch);
    };

    push(first, &mut out, &mut previous_was_dash);
    for ch in iter {
        push(sanitize_char(ch), &mut out, &mut previous_was_dash);
    }
    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() || out == "m" {
        "m-untitled".to_string()
    } else {
        out
    }
}

#[derive(Debug, Clone)]
pub struct SvgRenderOptions {
    /// Adds extra space around the computed viewBox.
    pub viewbox_padding: f64,
    /// Optional diagram id used for Mermaid-like marker ids.
    pub diagram_id: Option<String>,
}

impl Default for SvgRenderOptions {
    fn default() -> Self {
        Self {
            viewbox_padding: 8.0,
            diagram_id: None,
        }
    }
}

const SVG_DIAGRAM_ID_SCAN_CHECKPOINT_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy)]
struct SvgDiagramIdSanitizationPlan<'a> {
    trimmed: &'a str,
    output_bytes: usize,
    uses_fallback: bool,
}

fn checkpoint_svg_diagram_id_scan(
    session: &RenderSession,
    next_checkpoint: &mut usize,
    scanned_bytes: usize,
) -> Result<()> {
    if scanned_bytes < *next_checkpoint {
        return Ok(());
    }
    session.checkpoint(OperationPhase::Emit)?;
    *next_checkpoint = scanned_bytes
        .checked_add(SVG_DIAGRAM_ID_SCAN_CHECKPOINT_BYTES)
        .ok_or_else(|| {
            Error::from(session.work_meter().terminate_svg_byte_count_overflow(
                crate::resources::ResourceLimitPhase::SvgOutput,
                OperationPhase::Emit,
            ))
        })?;
    Ok(())
}

fn controlled_trim_svg_diagram_id<'a>(raw: &'a str, session: &RenderSession) -> Result<&'a str> {
    let mut first = None;
    let mut end = 0usize;
    let mut next_checkpoint = 0usize;
    for (offset, ch) in raw.char_indices() {
        checkpoint_svg_diagram_id_scan(session, &mut next_checkpoint, offset)?;
        if !ch.is_whitespace() {
            first.get_or_insert(offset);
            end = offset + ch.len_utf8();
        }
    }
    session.checkpoint(OperationPhase::Emit)?;
    Ok(first.map_or("", |first| &raw[first..end]))
}

fn plan_svg_diagram_id_sanitization<'a>(
    raw: &'a str,
    session: &RenderSession,
) -> Result<SvgDiagramIdSanitizationPlan<'a>> {
    let trimmed = controlled_trim_svg_diagram_id(raw, session)?;
    if trimmed.is_empty() {
        return Ok(SvgDiagramIdSanitizationPlan {
            trimmed,
            output_bytes: "m-untitled".len(),
            uses_fallback: true,
        });
    }

    let sanitize_char = |ch: char| {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            ch
        } else {
            '-'
        }
    };
    let mut chars = trimmed.char_indices();
    let (_, first_raw) = chars.next().expect("non-empty trimmed diagram id");
    let first = sanitize_char(first_raw);
    let mut output_bytes = 0usize;
    let mut first_output = None;
    let mut previous_was_dash = false;
    let mut append = |ch: char| -> Result<()> {
        if ch == '-' && previous_was_dash {
            return Ok(());
        }
        previous_was_dash = ch == '-';
        first_output.get_or_insert(ch);
        output_bytes = output_bytes.checked_add(1).ok_or_else(|| {
            Error::from(session.work_meter().terminate_svg_byte_count_overflow(
                crate::resources::ResourceLimitPhase::SvgOutput,
                OperationPhase::Emit,
            ))
        })?;
        Ok(())
    };

    if !first.is_ascii_alphabetic() {
        append('m')?;
        if first != '-' {
            append('-')?;
        }
    }
    append(first)?;

    let mut next_checkpoint = SVG_DIAGRAM_ID_SCAN_CHECKPOINT_BYTES;
    for (offset, ch) in chars {
        checkpoint_svg_diagram_id_scan(session, &mut next_checkpoint, offset)?;
        append(sanitize_char(ch))?;
    }
    if previous_was_dash {
        output_bytes -= 1;
    }
    let uses_fallback = output_bytes == 0 || (output_bytes == 1 && first_output == Some('m'));
    if uses_fallback {
        output_bytes = "m-untitled".len();
    }
    session.checkpoint(OperationPhase::Emit)?;
    Ok(SvgDiagramIdSanitizationPlan {
        trimmed,
        output_bytes,
        uses_fallback,
    })
}

fn materialize_svg_diagram_id(
    plan: SvgDiagramIdSanitizationPlan<'_>,
    session: &RenderSession,
) -> Result<String> {
    if session
        .resource_policy()
        .value(crate::resources::ResourceLimitId::MaxSvgBytes)
        .is_some()
    {
        session.work_meter().preflight_svg_byte_count(
            plan.output_bytes,
            crate::resources::ResourceLimitPhase::SvgOutput,
            OperationPhase::Emit,
        )?;
    }
    let mut output = String::new();
    output
        .try_reserve_exact(plan.output_bytes)
        .map_err(|error| Error::InvalidModel {
            message: format!("failed to allocate normalized SVG diagram id: {error}"),
        })?;
    if plan.uses_fallback {
        output.push_str("m-untitled");
        return Ok(output);
    }

    let sanitize_char = |ch: char| {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            ch
        } else {
            '-'
        }
    };
    let mut chars = plan.trimmed.char_indices();
    let (_, first_raw) = chars.next().expect("non-fallback diagram id is non-empty");
    let first = sanitize_char(first_raw);
    let mut previous_was_dash = false;
    let push = |ch: char, output: &mut String, previous_was_dash: &mut bool| {
        if ch == '-' && *previous_was_dash {
            return;
        }
        *previous_was_dash = ch == '-';
        output.push(ch);
    };

    if !first.is_ascii_alphabetic() {
        output.push('m');
        if first != '-' {
            output.push('-');
            previous_was_dash = true;
        }
    }
    push(first, &mut output, &mut previous_was_dash);
    let mut next_checkpoint = SVG_DIAGRAM_ID_SCAN_CHECKPOINT_BYTES;
    for (offset, ch) in chars {
        checkpoint_svg_diagram_id_scan(session, &mut next_checkpoint, offset)?;
        push(sanitize_char(ch), &mut output, &mut previous_was_dash);
    }
    while output.ends_with('-') {
        output.pop();
    }
    session.checkpoint(OperationPhase::Emit)?;
    if output.len() != plan.output_bytes {
        return Err(Error::InvalidModel {
            message: format!(
                "SVG diagram id byte projection drifted: projected {} bytes but materialized {}",
                plan.output_bytes,
                output.len()
            ),
        });
    }
    Ok(output)
}

pub(crate) fn normalize_svg_render_options(
    request: &SvgRenderOptions,
    session: &RenderSession,
) -> Result<SvgRenderOptions> {
    let diagram_id = request
        .diagram_id
        .as_deref()
        .map(|raw| plan_svg_diagram_id_sanitization(raw, session))
        .transpose()?
        .map(|plan| materialize_svg_diagram_id(plan, session))
        .transpose()?;
    Ok(SvgRenderOptions {
        viewbox_padding: request.viewbox_padding,
        diagram_id,
    })
}

/// A point captured while diagnosing one flowchart edge route.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FlowchartEdgeTracePoint {
    pub x: f64,
    pub y: f64,
}

/// An in-memory diagnostic record for one flowchart edge route.
///
/// The renderer never serializes this value or writes it to a host filesystem. Callers that need
/// a file own that I/O boundary and can serialize a drained record themselves.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FlowchartEdgeTrace {
    pub fixture_diagram_id: String,
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub layout_from: String,
    pub layout_to: String,
    pub from_cluster: Option<String>,
    pub to_cluster: Option<String>,
    pub origin_x: f64,
    pub origin_y: f64,
    pub tx: f64,
    pub ty: f64,
    pub base_points: Vec<FlowchartEdgeTracePoint>,
    pub points_after_intersect: Vec<FlowchartEdgeTracePoint>,
    pub points_for_render: Vec<FlowchartEdgeTracePoint>,
    pub points_for_data_points: Vec<FlowchartEdgeTracePoint>,
}

/// Explicit, caller-owned storage for flowchart route diagnostics.
///
/// Clones share one collection so a caller can retain a handle while a render owns a clone. A
/// poisoned lock is recovered because trace collection is diagnostic-only and must not turn a
/// completed render into an ambient host failure.
#[derive(Debug, Clone, Default)]
pub struct FlowchartEdgeTraceCollector(std::sync::Arc<std::sync::Mutex<Vec<FlowchartEdgeTrace>>>);

impl FlowchartEdgeTraceCollector {
    pub(crate) fn record(&self, trace: FlowchartEdgeTrace) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(trace);
    }

    /// Returns a snapshot without consuming the collected records.
    pub fn snapshot(&self) -> Vec<FlowchartEdgeTrace> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Drains all records collected so far.
    pub fn drain(&self) -> Vec<FlowchartEdgeTrace> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

/// An opaque flowchart trace request created by [`SvgDebugOptions::with_flowchart_edge_trace`].
#[derive(Debug, Clone)]
pub struct FlowchartEdgeTraceRequest {
    edge_id: String,
    collector: FlowchartEdgeTraceCollector,
}

/// Diagnostic visibility controls kept separate from production render requests.
#[derive(Debug, Clone)]
pub struct SvgDebugOptions {
    pub include_edges: bool,
    pub include_nodes: bool,
    pub include_clusters: bool,
    pub include_cluster_debug_markers: bool,
    pub include_edge_id_labels: bool,
    pub include_timing_diagnostics: bool,
    /// Optional caller-owned trace collection request.
    ///
    /// Use [`SvgDebugOptions::with_flowchart_edge_trace`] to construct a non-empty request. The
    /// field remains publicly updateable so existing `SvgDebugOptions { ..Default::default() }`
    /// callers keep their normal Rust struct-update ergonomics.
    pub flowchart_edge_trace: Option<FlowchartEdgeTraceRequest>,
}

impl Default for SvgDebugOptions {
    fn default() -> Self {
        Self {
            include_edges: true,
            include_nodes: true,
            include_clusters: true,
            include_cluster_debug_markers: false,
            include_edge_id_labels: false,
            include_timing_diagnostics: false,
            flowchart_edge_trace: None,
        }
    }
}

impl SvgDebugOptions {
    /// Captures diagnostics for one edge in caller-owned memory.
    ///
    /// This replaces the former implicit current-working-directory trace file. Hosts that want a
    /// file must drain `collector` after rendering and perform their own checked I/O.
    pub fn with_flowchart_edge_trace(
        mut self,
        edge_id: impl Into<String>,
        collector: FlowchartEdgeTraceCollector,
    ) -> Self {
        self.flowchart_edge_trace = Some(FlowchartEdgeTraceRequest {
            edge_id: edge_id.into(),
            collector,
        });
        self
    }

    pub(crate) fn flowchart_edge_trace(&self) -> Option<(&str, &FlowchartEdgeTraceCollector)> {
        self.flowchart_edge_trace
            .as_ref()
            .map(|trace| (trace.edge_id.as_str(), &trace.collector))
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
pub(crate) struct SvgExecution<'a> {
    request: &'a SvgRenderOptions,
    session: &'a RenderSession,
    text_measurer: RoutedTextMeasurer<'a>,
    diagram_id_projection: SvgDiagramIdProjection<'a>,
    timing: timing::RenderTiming,
    pub(crate) debug: &'a SvgDebugOptions,
}

struct SvgDiagramIdProjection<'a> {
    work_meter: &'a crate::resources::OperationWorkMeter,
    projected_bytes: std::cell::Cell<usize>,
    error: std::cell::RefCell<Option<Error>>,
}

impl SvgDiagramIdProjection<'_> {
    fn write(&self, value: &str, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.error.borrow().is_some() {
            return Ok(());
        }
        let Some(projected_bytes) = self.projected_bytes.get().checked_add(value.len()) else {
            self.error.replace(Some(
                self.work_meter
                    .terminate_svg_byte_count_overflow(
                        crate::resources::ResourceLimitPhase::SvgOutput,
                        OperationPhase::Emit,
                    )
                    .into(),
            ));
            return Ok(());
        };
        match self.work_meter.preflight_svg_byte_count(
            projected_bytes,
            crate::resources::ResourceLimitPhase::SvgOutput,
            OperationPhase::Emit,
        ) {
            Ok(()) => {
                self.projected_bytes.set(projected_bytes);
                formatter.write_str(value)
            }
            Err(error) => {
                self.error.replace(Some(error.into()));
                Ok(())
            }
        }
    }

    fn finish(&self) -> Result<()> {
        match self.error.borrow_mut().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// A normalized diagram identifier whose output occurrences are admitted before they are written.
///
/// The raw string is available only for semantic inputs such as deterministic seeds and ownership
/// checks. SVG, CSS, URL, ARIA, and marker output must format this value so the operation observes
/// the cumulative identifier contribution before caller-controlled fanout can grow the document.
#[derive(Clone, Copy)]
pub(super) struct SvgDiagramId<'a> {
    value: &'a str,
    projection: &'a SvgDiagramIdProjection<'a>,
}

impl<'a> SvgDiagramId<'a> {
    pub(super) fn semantic_str(self) -> &'a str {
        self.value
    }
}

impl std::fmt::Debug for SvgDiagramId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("SvgDiagramId")
            .field(&self.value)
            .finish()
    }
}

impl std::fmt::Display for SvgDiagramId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.projection.write(self.value, formatter)
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
pub(super) trait SvgDiagramIdValue: Copy + std::fmt::Display {
    fn semantic_value(&self) -> &str;
}

impl SvgDiagramIdValue for SvgDiagramId<'_> {
    fn semantic_value(&self) -> &str {
        self.value
    }
}

impl SvgDiagramIdValue for &str {
    fn semantic_value(&self) -> &str {
        self
    }
}

#[derive(Default)]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
struct SvgComponentByteCounter {
    bytes: usize,
    overflowed: bool,
}

impl std::fmt::Write for SvgComponentByteCounter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let Some(bytes) = self.bytes.checked_add(value.len()) else {
            self.overflowed = true;
            return Err(std::fmt::Error);
        };
        self.bytes = bytes;
        Ok(())
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
impl<'a> SvgExecution<'a> {
    fn new(
        request: &'a SvgRenderOptions,
        debug: &'a SvgDebugOptions,
        session: &'a RenderSession,
    ) -> Result<Self> {
        let timing = if debug.include_timing_diagnostics {
            timing::RenderTiming::enabled(
                session
                    .operation_context()
                    .require_timing()
                    .map_err(Error::from)?,
            )
        } else {
            timing::RenderTiming::disabled()
        };
        Ok(Self {
            request,
            session,
            text_measurer: session
                .controlled_text_measurer(TextMeasurementPhase::SvgBBox, OperationPhase::Emit),
            diagram_id_projection: SvgDiagramIdProjection {
                work_meter: session.work_meter().as_ref(),
                projected_bytes: std::cell::Cell::new(0),
                error: std::cell::RefCell::new(None),
            },
            timing,
            debug,
        })
    }

    pub(super) fn diagram_id_or<'execution>(
        &'execution self,
        fallback: &'static str,
    ) -> SvgDiagramId<'execution> {
        SvgDiagramId {
            value: self.request.diagram_id.as_deref().unwrap_or(fallback),
            projection: &self.diagram_id_projection,
        }
    }

    pub(super) fn has_explicit_diagram_id(&self) -> bool {
        self.request.diagram_id.is_some()
    }

    fn finish_diagram_id_projection(&self) -> Result<()> {
        self.diagram_id_projection.finish()
    }

    pub(crate) fn text_measurer(&self) -> &dyn TextMeasurer {
        &self.text_measurer
    }

    pub(crate) fn text_measurer_for(&self, phase: TextMeasurementPhase) -> RoutedTextMeasurer<'_> {
        self.session
            .controlled_text_measurer(phase, OperationPhase::Emit)
    }

    pub(crate) fn math_renderer(&self) -> Option<&(dyn crate::math::MathRenderer + Send + Sync)> {
        self.session.math_renderer()
    }

    pub(crate) fn icon_registry(&self) -> Option<&super::icon_registry::IconRegistry> {
        self.session.icon_registry()
    }

    pub(crate) fn unix_ms(&self) -> i64 {
        self.session.unix_millis()
    }

    pub(crate) fn local_time_zone(&self) -> &merman_core::time::LocalTimeZone {
        self.session.local_time_zone()
    }

    pub(crate) fn seed(&self) -> u64 {
        self.session.render_seed().get()
    }

    pub(crate) fn rough_randomness(
        &self,
        configured_seed: f64,
        owner_domain: &str,
    ) -> roughr::core::RoughRandomness {
        let resolved_seed = if configured_seed == 0.0 {
            self.seed() as f64
        } else {
            configured_seed
        };
        let operation = self.session.operation_context();
        roughr::core::RoughRandomness::new(
            roughr::core::RoughJsSeed::new(resolved_seed),
            roughr::core::RoughMathRandom::new(operation.derive_u64(owner_domain, 0)),
        )
    }

    pub(crate) fn timing(&self) -> timing::RenderTiming {
        self.timing
    }

    pub(crate) fn work_meter(&self) -> &crate::resources::OperationWorkMeter {
        self.session.work_meter().as_ref()
    }

    /// Replays a terminal observed while an SVG ID was being projected.
    ///
    /// Family emitters call this at bounded loop boundaries so an early SVG-byte rejection or
    /// cancellation stops the tail of a high-fanout render instead of waiting for finalization.
    pub(crate) fn checkpoint_emit(&self) -> Result<()> {
        self.work_meter()
            .checkpoint(OperationPhase::Emit)
            .map_err(Into::into)
    }

    /// Counts a retained SVG component through its production writer, admits the exact byte count,
    /// and only then allocates and materializes it.
    ///
    /// The counting pass owns no output buffer, so authored/config-sized content is not cloned just
    /// to establish the bound. The final whole-document check remains the authoritative
    /// `MaxSvgBytes` admission; this earlier absolute preflight prevents one amplified component
    /// from allocating beyond the same ceiling and is not accumulated a second time.
    fn materialize_counted_svg_component(
        &self,
        component_name: &'static str,
        count_component: impl Fn(&mut dyn std::fmt::Write) -> std::fmt::Result,
        write_component: impl Fn(&mut dyn std::fmt::Write) -> std::fmt::Result,
    ) -> Result<String> {
        if self
            .work_meter()
            .policy()
            .value(crate::resources::ResourceLimitId::MaxSvgBytes)
            .is_none()
        {
            let mut output = String::new();
            write_component(&mut output).map_err(|_| Error::InvalidModel {
                message: format!("failed to materialize {component_name}"),
            })?;
            return Ok(output);
        }

        let mut counter = SvgComponentByteCounter::default();
        let projection = count_component(&mut counter);
        if counter.overflowed {
            return Err(self
                .work_meter()
                .terminate_svg_byte_count_overflow(
                    crate::resources::ResourceLimitPhase::SvgOutput,
                    OperationPhase::Emit,
                )
                .into());
        }
        projection.map_err(|_| Error::InvalidModel {
            message: format!("failed to count {component_name}"),
        })?;
        let projected_bytes = counter.bytes;
        self.work_meter()
            .preflight_svg_byte_count(
                projected_bytes,
                crate::resources::ResourceLimitPhase::SvgOutput,
                OperationPhase::Emit,
            )
            .map_err(Error::from)?;

        let mut output = String::new();
        output
            .try_reserve_exact(projected_bytes)
            .map_err(|error| Error::InvalidModel {
                message: format!("failed to allocate {component_name}: {error}"),
            })?;
        write_component(&mut output).map_err(|_| Error::InvalidModel {
            message: format!("failed to materialize {component_name}"),
        })?;
        if output.len() != projected_bytes {
            return Err(Error::InvalidModel {
                message: format!(
                    "{component_name} byte projection drifted: projected {projected_bytes} bytes but materialized {}",
                    output.len()
                ),
            });
        }
        Ok(output)
    }
}

impl std::ops::Deref for SvgExecution<'_> {
    type Target = SvgRenderOptions;

    fn deref(&self) -> &Self::Target {
        self.request
    }
}

#[cfg(test)]
pub(crate) fn with_test_svg_execution<T>(
    request: &SvgRenderOptions,
    run: impl FnOnce(&SvgExecution<'_>) -> T,
) -> T {
    let session = crate::environment::RenderEnvironment::deterministic()
        .begin_session()
        .expect("create test render session");
    let debug = SvgDebugOptions::default();
    let execution = SvgExecution::new(request, &debug, &session)
        .expect("default test SVG execution does not request timing");
    run(&execution)
}

pub(crate) fn render_builtin_family_artifact(
    family: &crate::family::BuiltinFamilyArtifact,
    metadata: &merman_core::ParseMetadata,
    session: &RenderSession,
    options: &SvgRenderOptions,
    debug: &SvgDebugOptions,
    edge_paint_geometry: Option<&mut Vec<crate::model::EdgePaintGeometry>>,
) -> Result<String> {
    let execution = SvgExecution::new(options, debug, session)?;
    let rooted_svg =
        render_builtin_family_artifact_raw(family, metadata, &execution, edge_paint_geometry);
    execution.finish_diagram_id_projection()?;
    let rooted_svg = rooted_svg?;
    let svg = rooted_svg.into_string_for(family.kind())?;
    apply_theme_css(svg, metadata.effective_config.as_value(), session)
}

#[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
#[inline(never)]
pub(crate) fn render_architecture_family_artifact(
    pair: &crate::family::FamilyPair<
        merman_core::diagrams::architecture::ArchitectureDiagramRenderModel,
        ArchitectureDiagramLayout,
    >,
    effective_config: &merman_core::MermaidConfig,
    session: &RenderSession,
    options: &SvgRenderOptions,
    debug: &SvgDebugOptions,
) -> Result<String> {
    // Keep the deep-group Architecture path out of the heterogeneous dispatcher so it fits in
    // the renderer's supported low-stack worker budget.
    let execution = SvgExecution::new(options, debug, session)?;
    let rooted_svg = architecture::render_architecture_diagram_svg_typed_with_config(
        pair.layout(),
        pair.semantic(),
        effective_config,
        &execution,
    );
    execution.finish_diagram_id_projection()?;
    let rooted_svg = rooted_svg?;
    let svg = rooted_svg.into_string_for(crate::family::RenderFamilyKind::Architecture)?;
    apply_theme_css(svg, effective_config.as_value(), session)
}

#[allow(
    unused_variables,
    reason = "Dispatch inputs depend on the selected diagram families."
)]
fn render_builtin_family_artifact_raw(
    family: &crate::family::BuiltinFamilyArtifact,
    metadata: &merman_core::ParseMetadata,
    options: &SvgExecution<'_>,
    edge_paint_geometry: Option<&mut Vec<crate::model::EdgePaintGeometry>>,
) -> Result<root_svg::RootedSvg> {
    use crate::family::BuiltinFamilyArtifact;

    let measurer = options.text_measurer();
    let effective_config = &metadata.effective_config;
    let effective_config_value = effective_config.as_value();
    let title = metadata.title.as_deref();

    match family {
        BuiltinFamilyArtifact::Error(pair) => error::render_error_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            options,
        ),
        #[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
        #[cfg(feature = "diagram-architecture")]
        BuiltinFamilyArtifact::Architecture(pair) => {
            architecture::render_architecture_diagram_svg_typed_with_config(
                pair.layout(),
                pair.semantic(),
                effective_config,
                options,
            )
        }
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        BuiltinFamilyArtifact::Flowchart(artifact) => flowchart::render_flowchart_svg_artifact(
            artifact,
            metadata,
            options,
            edge_paint_geometry,
        ),
        #[cfg(feature = "diagram-agentflow")]
        BuiltinFamilyArtifact::Agentflow { flow, .. } => {
            flowchart::render_flowchart_svg_artifact(flow, metadata, options, edge_paint_geometry)
        }
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        BuiltinFamilyArtifact::Swimlane(artifact) => flowchart::render_swimlane_svg_artifact(
            artifact,
            metadata,
            options,
            edge_paint_geometry,
        ),
        #[cfg(feature = "diagram-cynefin")]
        BuiltinFamilyArtifact::Cynefin(pair) => cynefin::render_cynefin_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            options,
        ),
        #[cfg(feature = "diagram-wardley")]
        BuiltinFamilyArtifact::Wardley(pair) => wardley::render_wardley_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config,
            title,
            options,
        ),
        #[cfg(feature = "diagram-railroad")]
        BuiltinFamilyArtifact::Railroad(pair) => railroad::render_railroad_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-mindmap")]
        BuiltinFamilyArtifact::Mindmap(pair) => {
            mindmap::render_mindmap_diagram_svg_model_with_config(
                pair.layout(),
                pair.semantic(),
                effective_config,
                options,
            )
        }
        #[cfg(feature = "diagram-state")]
        BuiltinFamilyArtifact::State(pair) => state::render_state_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-class")]
        BuiltinFamilyArtifact::Class(pair) => class::render_class_diagram_svg_model_with_config(
            pair.layout(),
            pair.semantic(),
            effective_config,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-sequence")]
        BuiltinFamilyArtifact::Sequence(pair) => {
            sequence::render_sequence_diagram_svg_model_with_config(
                pair.layout(),
                pair.semantic(),
                effective_config,
                title,
                measurer,
                options,
            )
        }
        #[cfg(feature = "diagram-zenuml")]
        BuiltinFamilyArtifact::Zenuml(pair) => zenuml::render_zenuml_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config,
            title,
            options,
        ),
        #[cfg(feature = "diagram-kanban")]
        BuiltinFamilyArtifact::Kanban(pair) => {
            kanban::render_kanban_diagram_svg(pair.layout(), effective_config, options)
        }
        #[cfg(feature = "diagram-gantt")]
        BuiltinFamilyArtifact::Gantt(pair) => gantt::render_gantt_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            options,
        ),
        #[cfg(feature = "diagram-pie")]
        BuiltinFamilyArtifact::Pie(pair) => pie::render_pie_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            options,
        ),
        #[cfg(feature = "diagram-packet")]
        BuiltinFamilyArtifact::Packet(pair) => packet::render_packet_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            options,
        ),
        #[cfg(feature = "diagram-timeline")]
        BuiltinFamilyArtifact::Timeline(pair) => timeline::render_timeline_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-journey")]
        BuiltinFamilyArtifact::Journey(pair) => journey::render_journey_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-requirement")]
        BuiltinFamilyArtifact::Requirement(pair) => {
            requirement::render_requirement_diagram_svg_model(
                pair.layout(),
                pair.semantic(),
                effective_config,
                title,
                measurer,
                options,
            )
        }
        #[cfg(feature = "diagram-usecase")]
        BuiltinFamilyArtifact::Usecase(pair) => usecase::render_usecase_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-sankey")]
        BuiltinFamilyArtifact::Sankey(pair) => {
            sankey::render_sankey_diagram_svg(pair.layout(), effective_config_value, options)
        }
        #[cfg(feature = "diagram-radar")]
        BuiltinFamilyArtifact::Radar(pair) => radar::render_radar_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            options,
        ),
        #[cfg(feature = "diagram-info")]
        BuiltinFamilyArtifact::Info(pair) => {
            info::render_info_diagram_svg(pair.layout(), effective_config_value, options)
        }
        #[cfg(feature = "diagram-treemap")]
        BuiltinFamilyArtifact::Treemap(pair) => {
            treemap::render_treemap_diagram_svg(pair.layout(), effective_config_value, options)
        }
        #[cfg(feature = "diagram-venn")]
        BuiltinFamilyArtifact::Venn(pair) => venn::render_venn_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            options,
        ),
        #[cfg(feature = "diagram-block")]
        BuiltinFamilyArtifact::Block(pair) => block::render_block_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            options,
        ),
        #[cfg(feature = "diagram-er")]
        BuiltinFamilyArtifact::Er(pair) => er::render_er_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-quadrant-chart")]
        BuiltinFamilyArtifact::QuadrantChart(pair) => {
            quadrantchart::render_quadrantchart_diagram_svg(
                pair.layout(),
                pair.semantic(),
                effective_config_value,
                options,
            )
        }
        #[cfg(feature = "diagram-xychart")]
        BuiltinFamilyArtifact::XyChart(pair) => xychart::render_xychart_diagram_svg(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            options,
        ),
        #[cfg(feature = "diagram-git-graph")]
        BuiltinFamilyArtifact::GitGraph(pair) => gitgraph::render_gitgraph_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config_value,
            title,
            measurer,
            options,
        ),
        #[cfg(feature = "diagram-tree-view")]
        BuiltinFamilyArtifact::TreeView(pair) => tree_view::render_tree_view_diagram_svg_model(
            pair.layout(),
            pair.semantic(),
            effective_config,
            options,
        ),
        #[cfg(feature = "diagram-ishikawa")]
        BuiltinFamilyArtifact::Ishikawa(pair) => {
            ishikawa::render_ishikawa_diagram_svg(pair.layout(), effective_config_value, options)
        }
        #[cfg(feature = "diagram-event-modeling")]
        BuiltinFamilyArtifact::EventModeling(pair) => {
            eventmodeling::render_eventmodeling_diagram_svg(
                pair.layout(),
                pair.semantic(),
                effective_config_value,
                options,
            )
        }
        #[cfg(feature = "diagram-c4")]
        BuiltinFamilyArtifact::C4(pair) => c4::render_c4_diagram_svg_typed(
            pair.layout(),
            pair.semantic(),
            effective_config,
            title,
            measurer,
            options,
        ),
    }
}

fn apply_theme_css(
    svg: String,
    effective_config: &serde_json::Value,
    session: &RenderSession,
) -> Result<String> {
    const UNBALANCED_CSS_ERROR: &str = "{ /* ERROR: Unbalanced CSS */ }";

    let Some(theme_css) = effective_config
        .get("themeCSS")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|css| !css.is_empty() && *css != UNBALANCED_CSS_ERROR)
    else {
        return Ok(svg);
    };

    let metadata = SvgPostprocessMetadata::from_svg_with_execution(
        &svg,
        SvgPostprocessExecution::new(session),
    )?;
    let pipeline = SvgPipeline::parity()
        .with_postprocessor(ScopedCssPostprocessor::new(theme_css).with_existing_style_merge());
    pipeline.process_to_string_with_metadata(&svg, &metadata, session)
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
fn curve_basis_path_d(points: &[crate::model::LayoutPoint]) -> String {
    curve::curve_basis_path_d(points)
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared SVG execution support is used by the selected family emitters."
    )
)]
fn compute_layout_bounds(
    clusters: &[LayoutCluster],
    nodes: &[LayoutNode],
    edges: &[crate::model::LayoutEdge],
) -> Option<Bounds> {
    layout_debug::compute_layout_bounds(clusters, nodes, edges)
}

#[cfg(test)]
mod operation_time_tests {
    use super::*;

    #[test]
    fn svg_execution_uses_the_operation_session_time() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_runtime_policy(
                merman_core::runtime::RuntimePolicy::deterministic().with_fixed_unix_millis(1_000),
            )
            .begin_session()
            .expect("begin render session");
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");

        assert_eq!(execution.unix_ms(), session.unix_millis());
    }

    #[test]
    fn svg_execution_preserves_truthy_javascript_number_seeds() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin render session");
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");

        for seed in [
            serde_json::json!(-1),
            serde_json::json!(1.75),
            serde_json::json!(4_294_967_297_u64),
        ] {
            let seed = seed.as_f64().expect("numeric seed");
            assert_eq!(
                execution
                    .rough_randomness(seed, "render.test.roughjs")
                    .seed()
                    .number(),
                seed
            );
        }
    }

    #[test]
    fn svg_execution_resolves_falsy_seed_and_shared_math_random_stream() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin render session");
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");

        for seed in [0.0, -0.0] {
            assert_eq!(
                execution
                    .rough_randomness(seed, "render.test.roughjs")
                    .seed()
                    .number(),
                execution.seed() as f64
            );
        }

        let randomness = execution.rough_randomness(0.0, "render.test.roughjs");
        assert_eq!(
            randomness.math_random().initial_seed(),
            session
                .operation_context()
                .derive_u64("render.test.roughjs", 0)
        );
        assert_ne!(
            randomness.math_random().initial_seed(),
            execution
                .rough_randomness(0.0, "render.other.roughjs")
                .math_random()
                .initial_seed()
        );
    }
}

#[cfg(test)]
mod diagram_id_projection_tests {
    use super::*;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    fn bounded_execution(maximum: usize) -> (RenderSession, SvgDebugOptions) {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, maximum)
            .expect("valid SVG byte ceiling");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .expect("begin render session");
        let debug = SvgDebugOptions::default();
        (session, debug)
    }

    #[test]
    fn controlled_diagram_id_normalization_matches_the_public_sanitizer() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin render session");
        for raw in [
            "",
            "   ",
            "diagram",
            "1diagram",
            "--",
            "a.b:c",
            "éclair",
            "  a---b  ",
            "m",
            "\u{2003}a:b\u{2003}",
        ] {
            let request = SvgRenderOptions {
                diagram_id: Some(raw.to_string()),
                ..SvgRenderOptions::default()
            };
            let normalized = normalize_svg_render_options(&request, &session)
                .expect("controlled normalization succeeds");
            assert_eq!(
                normalized.diagram_id.as_deref(),
                Some(sanitize_svg_id(raw).as_str()),
                "raw diagram id {raw:?}"
            );
        }
    }

    #[test]
    fn controlled_diagram_id_scan_observes_preexisting_cancellation() {
        let control = merman_core::OperationControl::new();
        control.cancel();
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session_with_control(control)
            .expect("begin render session");
        let request = SvgRenderOptions {
            diagram_id: Some("a".repeat(SVG_DIAGRAM_ID_SCAN_CHECKPOINT_BYTES * 2)),
            ..SvgRenderOptions::default()
        };

        let error = normalize_svg_render_options(&request, &session)
            .expect_err("cancelled normalization must stop before allocation");
        let Error::Cancelled(cancelled) = error else {
            panic!("expected structured cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Emit);
    }

    #[test]
    fn diagram_id_occurrences_enforce_exact_n_and_n_minus_one() {
        let request = SvgRenderOptions {
            diagram_id: Some("abc".to_string()),
            ..SvgRenderOptions::default()
        };

        let (session, debug) = bounded_execution(6);
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let diagram_id = execution.diagram_id_or("fallback");
        assert_eq!(format!("{diagram_id}{diagram_id}"), "abcabc");
        execution
            .finish_diagram_id_projection()
            .expect("exact diagram-id contribution is admitted");

        let (session, debug) = bounded_execution(5);
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let diagram_id = execution.diagram_id_or("fallback");
        assert_eq!(format!("{diagram_id}"), "abc");
        assert_eq!(format!("{diagram_id}"), "");
        let error = execution
            .finish_diagram_id_projection()
            .expect_err("N-1 must reject the second occurrence");
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection");
        };
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, 6);
        assert_eq!(details.max, 5);
        assert_eq!(
            details.phase,
            crate::resources::ResourceLimitPhase::SvgOutput
        );
    }

    #[test]
    fn diagram_id_projection_terminal_is_replayed_at_emit_boundaries() {
        let request = SvgRenderOptions {
            diagram_id: Some("abc".to_string()),
            ..SvgRenderOptions::default()
        };
        let (session, debug) = bounded_execution(2);
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let diagram_id = execution.diagram_id_or("fallback");
        assert_eq!(format!("{diagram_id}"), "");

        let error = execution
            .checkpoint_emit()
            .expect_err("a failed ID projection must stop the next emit boundary");
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection");
        };
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, 3);
        assert_eq!(details.max, 2);
        assert_eq!(
            details.phase,
            crate::resources::ResourceLimitPhase::SvgOutput
        );
    }

    #[test]
    fn reusable_scoped_id_adapter_projects_each_output_occurrence() {
        let request = SvgRenderOptions {
            diagram_id: Some("abc".to_string()),
            ..SvgRenderOptions::default()
        };
        let (session, debug) = bounded_execution(5);
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let marker_url = scoped_svg_url(execution.diagram_id_or("fallback"), "arrowhead");

        assert_eq!(format!("{marker_url}"), "url(#abc-arrowhead)");
        assert_eq!(format!("{marker_url}"), "url(#-arrowhead)");
        let error = execution
            .finish_diagram_id_projection()
            .expect_err("reusing a derived ID must charge the diagram ID again");
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection");
        };
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, 6);
        assert_eq!(details.max, 5);
    }

    #[test]
    fn repeated_drop_shadow_references_project_each_diagram_id_occurrence() {
        let request = SvgRenderOptions {
            diagram_id: Some("abc".to_string()),
            ..SvgRenderOptions::default()
        };
        let (session, debug) = bounded_execution(5);
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let drop_shadow = scoped_drop_shadow(
            execution.diagram_id_or("fallback"),
            "url(#drop-shadow) url(#drop-shadow)",
        );

        assert_eq!(
            format!("{drop_shadow}"),
            "url(#abc-drop-shadow) url(#-drop-shadow)"
        );
        let error = execution
            .finish_diagram_id_projection()
            .expect_err("N-1 must reject the repeated themed URL reference");
        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected SVG byte resource rejection");
        };
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.actual, 6);
        assert_eq!(details.max, 5);
    }
}

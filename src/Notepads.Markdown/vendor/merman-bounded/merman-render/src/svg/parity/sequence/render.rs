use super::super::*;
use super::SequenceEmitCheckpoints;
use super::actor_man::{render_sequence_actor_man_bottoms, render_sequence_actor_man_tops};
use super::actor_popup::{SequenceActorPopupOptions, render_sequence_actor_popup_menus};
use super::actors::{
    SequenceActorRenderContext, render_sequence_bottom_actors,
    render_sequence_top_actors_and_lifelines,
};
use super::frames::{SequenceFrameRenderOptions, render_sequence_box_frames_and_rect_blocks};
use super::interactions::{SequenceInteractionRenderContext, render_sequence_interaction_overlays};
use super::messages::{SequenceMessageRenderContext, render_sequence_messages};
use super::root::write_sequence_svg_root_open;
use super::settings::SequenceRenderSettings;
use rustc_hash::FxHashMap;

use super::css::sequence_css;
use super::model::*;

const PINNED_MERMAID_SEQUENCE_BASE_DEFS: &str = include_str!("sequence_base_defs_11_16_0.svgfrag");
const MERMAID_SEQUENCE_EXTRA_MARKER_DEFS_PINNED: &str = r#"<defs><marker id="solidTopArrowHead" refX="7.9" refY="7.25" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto-start-reverse"><path d="M 0 0 L 10 8 L 0 8 z"/></marker></defs><defs><marker id="solidBottomArrowHead" refX="7.9" refY="0.75" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto-start-reverse"><path d="M 0 0 L 10 0 L 0 8 z"/></marker></defs><defs><marker id="stickTopArrowHead" refX="7.5" refY="7" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto-start-reverse"><path d="M 0 0 L 7 7" stroke="black" stroke-width="1.5" fill="none"/></marker></defs><defs><marker id="stickBottomArrowHead" refX="7.5" refY="0" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto-start-reverse"><path d="M 0 7 L 7 0" stroke="black" stroke-width="1.5" fill="none"/></marker></defs>"#;

const SEQUENCE_SCOPED_BASE_DEF_IDS: [&str; 11] = [
    "computer",
    "database",
    "clock",
    "arrowhead",
    "crosshead",
    "filled-head",
    "sequencenumber",
    "solidTopArrowHead",
    "solidBottomArrowHead",
    "stickTopArrowHead",
    "stickBottomArrowHead",
];

fn write_scoped_sequence_base_defs_fragment(
    out: &mut String,
    fragment: &str,
    diagram_id: impl Copy + std::fmt::Display,
) {
    const ID_ATTRIBUTE_START: &str = "id=\"";

    let mut cursor = 0usize;
    while let Some(relative_start) = fragment[cursor..].find(ID_ATTRIBUTE_START) {
        let attribute_start = cursor + relative_start;
        let value_start = attribute_start + ID_ATTRIBUTE_START.len();
        let Some(relative_end) = fragment[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + relative_end;
        let local_id = &fragment[value_start..value_end];

        out.push_str(&fragment[cursor..value_start]);
        if SEQUENCE_SCOPED_BASE_DEF_IDS.contains(&local_id) {
            let _ = write!(out, "{diagram_id}");
            out.push('-');
            out.push_str(local_id);
        } else {
            out.push_str(local_id);
        }
        cursor = value_end;
    }
    out.push_str(&fragment[cursor..]);
}

fn write_scoped_sequence_base_defs(out: &mut String, diagram_id: impl Copy + std::fmt::Display) {
    write_scoped_sequence_base_defs_fragment(out, PINNED_MERMAID_SEQUENCE_BASE_DEFS, diagram_id);
    write_scoped_sequence_base_defs_fragment(
        out,
        MERMAID_SEQUENCE_EXTRA_MARKER_DEFS_PINNED,
        diagram_id,
    );
}

pub(in crate::svg::parity) fn render_sequence_diagram_svg_model_with_config(
    prepared: &crate::sequence::SequencePreparedArtifact,
    model: &SequenceSvgModel,
    effective_config: &merman_core::MermaidConfig,
    diagram_title: Option<&str>,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    render_sequence_diagram_svg_inner(
        prepared,
        model,
        effective_config.as_value(),
        effective_config,
        diagram_title,
        measurer,
        options,
    )
}

fn render_sequence_diagram_svg_inner(
    prepared: &crate::sequence::SequencePreparedArtifact,
    model: &SequenceSvgModel,
    effective_config: &serde_json::Value,
    sanitize_config: &merman_core::MermaidConfig,
    diagram_title: Option<&str>,
    measurer: &dyn TextMeasurer,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let checkpoints = SequenceEmitCheckpoints::new(options.work_meter());
    checkpoints.checkpoint()?;
    let layout = prepared.layout();
    let effective_title =
        crate::sequence::sequence_render_title(model.title.as_deref(), diagram_title);

    let diagram_id = options.diagram_id_or("merman");
    let settings = SequenceRenderSettings::from_effective_config(effective_config);

    let mut nodes_by_id: FxHashMap<&str, &LayoutNode> =
        FxHashMap::with_capacity_and_hasher(layout.nodes.len(), Default::default());
    for (node_index, node) in layout.nodes.iter().enumerate() {
        checkpoints.checkpoint_loop(node_index)?;
        nodes_by_id.insert(node.id.as_str(), node);
    }

    let mut edges_by_id: FxHashMap<&str, &crate::model::LayoutEdge> =
        FxHashMap::with_capacity_and_hasher(layout.edges.len(), Default::default());
    for (edge_index, edge) in layout.edges.iter().enumerate() {
        checkpoints.checkpoint_loop(edge_index)?;
        edges_by_id.insert(edge.id.as_str(), edge);
    }
    checkpoints.checkpoint()?;

    let mut out = String::new();
    checkpoints.checkpoint()?;
    let root_metrics = write_sequence_svg_root_open(&mut out, layout, model, diagram_id)?;
    checkpoints.checkpoint()?;

    render_sequence_box_frames_and_rect_blocks(
        &mut out,
        model,
        &nodes_by_id,
        SequenceFrameRenderOptions {
            actor_text_style: &settings.actor_text_style,
            box_layouts: prepared.box_layouts(),
            box_title_height: prepared.box_title_height(),
            box_height: prepared.box_height(),
            box_margin: settings.box_margin,
            box_text_margin: settings.box_text_margin,
            rect_default_fill: &settings.rect_default_fill,
        },
        checkpoints,
    )?;

    let actor_diagram_id = diagram_id.to_string();
    let actor_ctx = SequenceActorRenderContext {
        model,
        nodes_by_id: &nodes_by_id,
        edges_by_id: &edges_by_id,
        diagram_id: &actor_diagram_id,
        sanitize_config,
        math_renderer: options.math_renderer(),
        actor_wrap_width: settings.actor_wrap_width,
        actor_height: settings.actor_height,
        label_box_height: settings.label_box_height,
        measurer,
        actor_text_style: &settings.actor_text_style,
        checkpoints,
    };

    if settings.mirror_actors {
        render_sequence_bottom_actors(&mut out, &actor_ctx)?;
    }

    // Top actors + lifelines.
    render_sequence_top_actors_and_lifelines(&mut out, &actor_ctx)?;

    checkpoints.checkpoint()?;
    let _ = write!(
        &mut out,
        r#"<style>{}</style><g/>"#,
        sequence_css(diagram_id, effective_config)
    );

    // Mermaid's sequence output includes a shared set of <defs> for icons/markers.
    checkpoints.checkpoint()?;
    write_scoped_sequence_base_defs(&mut out, diagram_id);
    if crate::config::config_diagram_look(effective_config).as_str() == "neo" {
        let theme = effective_config
            .get("theme")
            .and_then(serde_json::Value::as_str);
        let flood_color = if matches!(theme, Some("redux" | "redux-color")) {
            "#000000"
        } else {
            "#FFFFFF"
        };
        let _ = write!(
            out,
            r#"<defs><filter id="{diagram_id}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="{flood_color}"/></filter></defs>"#
        );
    }
    checkpoints.checkpoint()?;

    let block_widths_by_id = crate::sequence::sequence_block_widths_for_render(
        model,
        prepared,
        &nodes_by_id,
        sanitize_config,
        measurer,
        options.math_renderer(),
        options.work_meter(),
    )?;

    let interaction_ctx = SequenceInteractionRenderContext {
        model,
        block_widths_by_id: &block_widths_by_id,
        block_layouts_by_id: &layout.block_layouts_by_id,
        nodes_by_id: &nodes_by_id,
        edges_by_id: &edges_by_id,
        sanitize_config,
        math_renderer: options.math_renderer(),
        settings: &settings,
        measurer,
        checkpoints,
    };
    render_sequence_interaction_overlays(&mut out, &interaction_ctx)?;

    // Mermaid appends glyph actors after overlays, before drawing messages.
    render_sequence_actor_man_tops(&mut out, &actor_ctx, diagram_id)?;

    let message_ctx = SequenceMessageRenderContext {
        model,
        nodes_by_id: &nodes_by_id,
        edges_by_id: &edges_by_id,
        sanitize_config,
        math_renderer: options.math_renderer(),
        measurer,
        message_align: settings.message_align.as_str(),
        diagram_id,
        actor_height: settings.actor_height,
        sequence_width: settings.sequence_width,
        activation_width: settings.activation_width,
        wrap_padding: settings.wrap_padding,
        right_angles: settings.right_angles,
        loop_text_style: &settings.loop_text_style,
        checkpoints,
    };
    render_sequence_messages(&mut out, &message_ctx)?;

    if settings.mirror_actors {
        render_sequence_actor_man_bottoms(&mut out, &actor_ctx, diagram_id)?;
    }

    render_sequence_actor_popup_menus(
        &mut out,
        model,
        &nodes_by_id,
        sanitize_config,
        SequenceActorPopupOptions {
            force_menus: settings.force_menus,
            mirror_actors: settings.mirror_actors,
            actor_height: settings.actor_height,
            wrap_padding: settings.wrap_padding,
            box_margin: settings.box_margin,
            actor_text_style: &settings.actor_text_style,
            measurer,
        },
        checkpoints,
    )?;

    if let Some(title) = effective_title {
        // Mermaid sequence titles are currently emitted as a plain `<text>` node.
        // Mermaid positions the title using the inner (content) box width:
        // `x = (box.stopx - box.startx) / 2 - 2 * diagramMarginX`.
        let title_x = ((root_metrics.viewbox_width - 2.0 * settings.diagram_margin_x) / 2.0)
            - 2.0 * settings.diagram_margin_x;
        checkpoints.checkpoint()?;
        let _ = write!(
            &mut out,
            r#"<text x="{x}" y="-25">{text}</text>"#,
            x = fmt(title_x),
            text = escape_xml(title)
        );
    }

    checkpoints.checkpoint()?;
    out.push_str("</svg>\n");
    checkpoints.checkpoint()?;
    let rooted = root_metrics.document.complete(out)?;
    checkpoints.checkpoint()?;
    Ok(rooted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_defs_are_scoped_in_one_output_pass() {
        let mut defs = String::new();
        write_scoped_sequence_base_defs(&mut defs, "sequence-root");

        for local_id in SEQUENCE_SCOPED_BASE_DEF_IDS {
            assert!(
                defs.contains(&format!(r#"id="sequence-root-{local_id}""#)),
                "missing scoped definition {local_id}"
            );
            assert!(
                !defs.contains(&format!(r#"id="{local_id}""#)),
                "bare definition {local_id} survived scoping"
            );
        }
    }
}

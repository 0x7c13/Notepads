use super::super::*;
use super::SequenceEmitCheckpoints;
use super::activation::{build_sequence_activation_plan, render_sequence_activation_group};
use super::block_collection::{SequenceBlock, collect_sequence_blocks};
use super::block_geometry::frame_x_from_actors;
use super::blocks::{
    SequenceBlockRenderContext, SimpleSequenceBlock, render_critical_sequence_block,
    render_sectioned_sequence_block, render_simple_sequence_block,
};
use super::model::*;
use super::notes::{SequenceNoteRenderContext, render_sequence_note};
use super::settings::SequenceRenderSettings;
use rustc_hash::FxHashMap;

pub(super) struct SequenceInteractionRenderContext<'a> {
    pub(super) model: &'a SequenceSvgModel,
    pub(super) block_widths_by_id: &'a FxHashMap<String, f64>,
    pub(super) block_layouts_by_id: &'a FxHashMap<String, crate::model::SequenceBlockLayout>,
    pub(super) nodes_by_id: &'a FxHashMap<&'a str, &'a LayoutNode>,
    pub(super) edges_by_id: &'a FxHashMap<&'a str, &'a crate::model::LayoutEdge>,
    pub(super) sanitize_config: &'a merman_core::MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(super) settings: &'a SequenceRenderSettings,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) checkpoints: SequenceEmitCheckpoints<'a>,
}

pub(super) fn render_sequence_interaction_overlays(
    out: &mut String,
    ctx: &SequenceInteractionRenderContext<'_>,
) -> Result<()> {
    // Mermaid creates activation placeholders at ACTIVE_START and inserts the `<rect>` once the
    // corresponding ACTIVE_END is encountered. We store the final rect geometry during this
    // first pass and remember which message id should emit which activation group.
    ctx.checkpoints.checkpoint()?;
    let activation_plan = build_sequence_activation_plan(
        ctx.model,
        ctx.nodes_by_id,
        ctx.edges_by_id,
        ctx.settings.activation_width,
        ctx.checkpoints,
    )?;
    ctx.checkpoints.checkpoint()?;

    let Some((frame_x1, frame_x2)) =
        frame_x_from_actors(ctx.model, ctx.nodes_by_id, ctx.checkpoints)?
    else {
        return Ok(());
    };

    let mut actor_nodes_by_id: FxHashMap<&str, &LayoutNode> =
        FxHashMap::with_capacity_and_hasher(ctx.model.actors.len(), Default::default());
    for (actor_index, actor_id) in ctx.model.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(actor_index)?;
        let node_id = format!("actor-top-{actor_id}");
        let Some(n) = ctx.nodes_by_id.get(node_id.as_str()).copied() else {
            continue;
        };
        actor_nodes_by_id.insert(actor_id.as_str(), n);
    }

    let (blocks_by_end_index, blocks) = collect_sequence_blocks(
        ctx.model,
        &actor_nodes_by_id,
        ctx.edges_by_id,
        ctx.nodes_by_id,
        ctx.block_layouts_by_id,
        ctx.checkpoints,
    )?;
    ctx.checkpoints.checkpoint()?;

    let block_ctx = SequenceBlockRenderContext {
        default_frame_x1: frame_x1,
        default_frame_x2: frame_x2,
        block_widths_by_id: ctx.block_widths_by_id,
        actor_nodes_by_id: &actor_nodes_by_id,
        label_box_width: ctx.settings.label_box_width,
        label_box_height: ctx.settings.label_box_height,
        box_margin: ctx.settings.box_margin,
        box_text_margin: ctx.settings.box_text_margin,
        wrap_padding: ctx.settings.wrap_padding,
        measurer: ctx.measurer,
        loop_text_style: &ctx.settings.loop_text_style,
        sanitize_config: ctx.sanitize_config,
        math_renderer: ctx.math_renderer,
        checkpoints: ctx.checkpoints,
    };
    let note_ctx = SequenceNoteRenderContext {
        nodes_by_id: ctx.nodes_by_id,
        measurer: ctx.measurer,
        note_margin: ctx.settings.note_margin,
        wrap_padding: ctx.settings.wrap_padding,
        note_text_style: &ctx.settings.note_text_style,
        sanitize_config: ctx.sanitize_config,
        math_renderer: ctx.math_renderer,
        checkpoints: ctx.checkpoints,
    };

    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        render_sequence_activation_group(out, &activation_plan, &msg.id, ctx.sanitize_config);
        render_sequence_note(out, msg, &note_ctx)?;

        let Some(block_index) = blocks_by_end_index.get(message_index).copied().flatten() else {
            continue;
        };
        let Some(block) = blocks.get(block_index) else {
            continue;
        };
        match block {
            SequenceBlock::Alt {
                control_id,
                sections,
                layout,
            } => {
                render_sectioned_sequence_block(
                    out, control_id, "alt", sections, *layout, &block_ctx,
                )?;
            }
            SequenceBlock::Par {
                control_id,
                sections,
                layout,
            } => {
                render_sectioned_sequence_block(
                    out, control_id, "par", sections, *layout, &block_ctx,
                )?;
            }
            SequenceBlock::Loop {
                control_id,
                label_id,
                raw_label,
                geometry,
                layout,
            } => {
                render_simple_sequence_block(
                    out,
                    SimpleSequenceBlock {
                        control_id,
                        label_id,
                        block_label: "loop",
                        raw_label,
                        geometry: *geometry,
                        layout: *layout,
                    },
                    &block_ctx,
                )?;
            }
            SequenceBlock::Opt {
                control_id,
                label_id,
                raw_label,
                geometry,
                layout,
            } => {
                render_simple_sequence_block(
                    out,
                    SimpleSequenceBlock {
                        control_id,
                        label_id,
                        block_label: "opt",
                        raw_label,
                        geometry: *geometry,
                        layout: *layout,
                    },
                    &block_ctx,
                )?;
            }
            SequenceBlock::Break {
                control_id,
                label_id,
                raw_label,
                geometry,
                layout,
            } => {
                render_simple_sequence_block(
                    out,
                    SimpleSequenceBlock {
                        control_id,
                        label_id,
                        block_label: "break",
                        raw_label,
                        geometry: *geometry,
                        layout: *layout,
                    },
                    &block_ctx,
                )?;
            }
            SequenceBlock::Critical {
                control_id,
                sections,
                layout,
            } => {
                render_critical_sequence_block(out, control_id, sections, *layout, &block_ctx)?;
            }
        }
    }
    ctx.checkpoints.checkpoint()
}

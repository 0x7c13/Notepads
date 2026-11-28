use super::super::*;
use super::actor_man_glyphs::{ActorManGlyphPlacement, write_actor_man_glyph};
use super::actor_shapes::{ActorLabelContext, is_actor_man_variant};
use super::actors::SequenceActorRenderContext;

pub(super) fn render_sequence_actor_man_tops(
    out: &mut String,
    ctx: &SequenceActorRenderContext<'_>,
    diagram_id: SvgDiagramId<'_>,
) -> Result<()> {
    render_sequence_actor_man(out, ctx, diagram_id, false)
}

pub(super) fn render_sequence_actor_man_bottoms(
    out: &mut String,
    ctx: &SequenceActorRenderContext<'_>,
    diagram_id: SvgDiagramId<'_>,
) -> Result<()> {
    render_sequence_actor_man(out, ctx, diagram_id, true)
}

fn render_sequence_actor_man(
    out: &mut String,
    ctx: &SequenceActorRenderContext<'_>,
    diagram_id: SvgDiagramId<'_>,
    footer: bool,
) -> Result<()> {
    let label_ctx = ActorLabelContext::new(
        ctx.actor_wrap_width,
        ctx.diagram_id,
        ctx.measurer,
        ctx.actor_text_style,
        ctx.sanitize_config,
        ctx.math_renderer,
        ctx.checkpoints,
    );
    // These groups are appended by drawActor, unlike the box groups that are lowered.
    for (actor_index, actor_id) in ctx.model.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(actor_index)?;
        let Some(actor) = ctx.model.actors.get(actor_id) else {
            continue;
        };
        if !is_actor_man_variant(&actor.actor_type) {
            continue;
        }
        let placement = if footer { "bottom" } else { "top" };
        let node_id = format!("actor-{placement}-{actor_id}");
        let Some(node) = ctx.nodes_by_id.get(node_id.as_str()).copied() else {
            continue;
        };
        write_actor_man_glyph(
            out,
            actor_id,
            actor,
            node,
            ActorManGlyphPlacement {
                footer,
                line_index: if footer {
                    ctx.model.actor_order.len().saturating_sub(1)
                } else {
                    actor_index
                },
                actor_index,
                actor_height: ctx.actor_height,
                label_box_height: ctx.label_box_height,
                diagram_id,
            },
            &label_ctx,
        )?;
    }
    ctx.checkpoints.checkpoint()
}

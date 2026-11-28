use super::super::*;
use super::SequenceEmitCheckpoints;
use super::geometry::node_left_top;
use super::model::SequenceSvgModel;
use merman_core::diagrams::sequence::{SequenceControlKind, SequenceControlRole};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy)]
pub(super) struct SequenceFrameRenderOptions<'a> {
    pub(super) actor_text_style: &'a TextStyle,
    pub(super) box_layouts: &'a [crate::sequence::SequenceBoxLayout],
    pub(super) box_title_height: f64,
    pub(super) box_height: f64,
    pub(super) box_margin: f64,
    pub(super) box_text_margin: f64,
    pub(super) rect_default_fill: &'a str,
}

pub(super) fn render_sequence_box_frames_and_rect_blocks(
    out: &mut String,
    model: &SequenceSvgModel,
    nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    options: SequenceFrameRenderOptions<'_>,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<()> {
    // Mermaid renders "box" frames as root-level `<g><rect class="rect"/>...</g>` nodes before actors.
    // Mermaid renders boxes "behind" other elements; multiple boxes end up reversed in DOM order.
    let max_box_title_height = options.box_title_height;

    checkpoints.checkpoint()?;
    for (emission_index, (b, box_layout)) in model
        .boxes
        .iter()
        .zip(options.box_layouts)
        .rev()
        .enumerate()
    {
        checkpoints.checkpoint_loop(emission_index)?;
        let Some(box_x) = box_layout.x else {
            continue;
        };
        let padding = options.box_margin * 2.0;
        let x = box_x - padding;
        let w = box_layout.width + padding * 2.0;
        let y = -padding * 0.25;
        let h = options.box_height + padding * 0.75;

        out.push_str("<g>");
        let _ = write!(
            out,
            r#"<rect x="{x}" y="{y}" fill="{fill}" stroke="rgb(0,0,0, 0.5)" width="{w}" height="{h}" class="rect"/>"#,
            x = fmt(x),
            y = fmt(y),
            w = fmt(w),
            h = fmt(h),
            fill = escape_xml_display(&b.fill),
        );
        if let Some(name) = box_layout.label.as_deref().filter(|name| !name.is_empty()) {
            let cx = x + (w / 2.0);
            // Mermaid's `drawBox(...)` places the title at `box.y + boxTextMargin + textMaxHeight/2`.
            // In upstream, `box.y` is the `verticalPos` passed to `addActorRenderingData`, i.e. 0.
            let text_y = options.box_text_margin + max_box_title_height / 2.0;
            super::text::write_centered_tspan_text(
                out,
                cx,
                text_y,
                name,
                "text",
                options.actor_text_style,
                checkpoints,
            )?;
        }
        out.push_str("</g>");
    }

    // Mermaid renders `rect` blocks as root-level `<rect class="rect"/>` nodes before actors.
    {
        #[derive(Debug, Clone, Copy)]
        struct RectBlock<'a> {
            fill: &'a str,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        }

        fn contains(a: &RectBlock<'_>, b: &RectBlock<'_>) -> bool {
            const EPS: f64 = 1e-9;
            a.x <= b.x + EPS
                && a.y <= b.y + EPS
                && (a.x + a.w) >= (b.x + b.w) - EPS
                && (a.y + a.h) >= (b.y + b.h) - EPS
        }

        let mut rects: Vec<RectBlock<'_>> = Vec::with_capacity(model.messages.len());
        for (message_index, msg) in model.messages.iter().enumerate() {
            checkpoints.checkpoint_loop(message_index)?;
            if !msg.control_semantics().is_some_and(|semantics| {
                semantics.kind == SequenceControlKind::Rect
                    && semantics.role == SequenceControlRole::Start
            }) {
                continue;
            }
            let explicit_fill = msg.message_text();
            let fill = if explicit_fill.is_empty() {
                options.rect_default_fill
            } else {
                explicit_fill
            };
            let node_id = format!("rect-{}", msg.id);
            let Some(n) = nodes_by_id.get(node_id.as_str()).copied() else {
                continue;
            };
            let (x, y) = node_left_top(n);
            rects.push(RectBlock {
                fill,
                x,
                y,
                w: n.width,
                h: n.height,
            });
        }

        // Mermaid's emitted order for nested `rect` blocks is not strictly tied to parse order.
        // Match its DOM ordering semantics by keeping parents before contained children and
        // sorting unrelated rectangles by vertical position (lower blocks first).
        checkpoints.checkpoint()?;
        rects.sort_by(|a, b| {
            if contains(a, b) && !contains(b, a) {
                return std::cmp::Ordering::Less;
            }
            if contains(b, a) && !contains(a, b) {
                return std::cmp::Ordering::Greater;
            }
            b.y.partial_cmp(&a.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        });

        checkpoints.checkpoint()?;
        for (rect_index, r) in rects.into_iter().enumerate() {
            checkpoints.checkpoint_loop(rect_index)?;
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" fill="{fill}" width="{w}" height="{h}" class="rect"/>"#,
                x = fmt(r.x),
                y = fmt(r.y),
                w = fmt(r.w),
                h = fmt(r.h),
                fill = escape_xml_display(r.fill)
            );
        }
    }
    checkpoints.checkpoint()
}

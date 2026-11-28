use super::super::*;
use super::actor_shapes::ActorLabelContext;
use super::geometry::node_left_top;
use crate::sequence::SEQUENCE_GLYPH_BAND_HEIGHT;
use merman_core::diagrams::sequence::SequenceActor;

pub(super) struct ActorManGlyphPlacement<'a> {
    pub(super) footer: bool,
    pub(super) line_index: usize,
    pub(super) actor_index: usize,
    pub(super) actor_height: f64,
    pub(super) label_box_height: f64,
    pub(super) diagram_id: SvgDiagramId<'a>,
}

pub(super) fn write_actor_man_glyph(
    out: &mut String,
    actor_id: &str,
    actor: &SequenceActor,
    node: &LayoutNode,
    placement: ActorManGlyphPlacement<'_>,
    label_ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    let (_, actor_y) = node_left_top(node);
    let cx = node.x;
    let actor_type = actor.actor_type.as_str();
    let bands = label_ctx.bands(node, placement.footer);
    let height = if bands.is_some() {
        node.height
    } else if placement.footer {
        match actor_type {
            "boundary" | "entity" => 44.0 + placement.label_box_height,
            "control" => 44.0 + 2.0 * placement.label_box_height,
            _ => placement.actor_height,
        }
    } else {
        placement.actor_height
    };
    let placement_class = if placement.footer {
        "actor-bottom"
    } else {
        "actor-top"
    };
    let group_class = if actor_type == "entity" {
        "actor"
    } else {
        "actor-man"
    };
    let _ = write!(
        out,
        r#"<g class="{group_class} {placement_class}" name="{}""#,
        escape_attr(actor_id)
    );
    if bands.is_none() {
        match actor_type {
            "boundary" => out.push_str(r#" transform="translate(0,21)""#),
            "entity" => {
                let offset = if placement.footer { 22 } else { 6 };
                let _ = write!(out, r#" transform="translate(0, {offset})""#);
            }
            _ => {}
        }
    }
    if matches!(actor_type, "boundary" | "entity") {
        label_ctx.write_shadow_attr(out);
    }
    label_ctx.write_style_attr(out, actor_type, placement.actor_index);
    if !placement.footer {
        let _ = write!(
            out,
            r#" data-et="participant" data-type="{}" data-id="{}""#,
            escape_attr(actor_type),
            escape_attr(actor_id)
        );
    }
    out.push('>');

    let idx = placement.line_index;
    let cx_text = fmt(cx);
    let cy = bands
        .as_ref()
        .map(|band| band.glyph_bottom_y - 22.0)
        .unwrap_or_else(|| {
            actor_y
                + match actor_type {
                    "boundary" => 12.0,
                    "entity" if placement.footer => 10.0,
                    "entity" => 25.0,
                    _ => 32.0,
                }
        });
    match actor_type {
        "actor" => {
            let scale = if bands.is_some() {
                SEQUENCE_GLYPH_BAND_HEIGHT / 65.0
            } else {
                1.0
            };
            let gy = |offset: f64| {
                bands.as_ref().map_or(actor_y + offset, |band| {
                    band.glyph_bottom_y - (60.0 - offset) * scale
                })
            };
            let gx = |offset: f64| cx + offset * scale;
            let _ = write!(
                out,
                r#"<line id="actor-man-torso{idx}" x1="{cx_text}" y1="{torso_top}" x2="{cx_text}" y2="{torso_bottom}"/><line id="actor-man-arms{idx}" x1="{left}" y1="{arms}" x2="{right}" y2="{arms}"/><line x1="{left}" y1="{feet}" x2="{cx_text}" y2="{torso_bottom}"/><line x1="{cx_text}" y1="{torso_bottom}" x2="{right_leg}" y2="{feet}"/><circle cx="{cx_text}" cy="{head}" r="{radius}" width="{width}" height="{height}"/>"#,
                torso_top = fmt(gy(25.0)),
                torso_bottom = fmt(gy(45.0)),
                left = fmt(gx(-18.0)),
                right = fmt(gx(18.0)),
                arms = fmt(gy(33.0)),
                feet = fmt(gy(60.0)),
                right_leg = fmt(gx(16.0)),
                head = fmt(gy(10.0)),
                radius = fmt(15.0 * scale),
                width = fmt(node.width),
                height = fmt(height)
            );
        }
        "boundary" => {
            let _ = write!(
                out,
                r#"<line id="actor-man-torso{idx}" x1="{left}" y1="{cy}" x2="{right}" y2="{cy}"/><line id="actor-man-arms{idx}" x1="{left}" y1="{top}" x2="{left}" y2="{bottom}"/><circle cx="{cx_text}" cy="{cy}" r="22"/>"#,
                left = fmt(cx - 55.0),
                right = fmt(cx - 15.0),
                cy = fmt(cy),
                top = fmt(cy - 10.0),
                bottom = fmt(cy + 10.0)
            );
        }
        "control" => {
            let marker_id = scoped_svg_id(placement.diagram_id, "filled-head-control");
            let marker_url = scoped_svg_url(placement.diagram_id, "filled-head-control");
            let _ = write!(
                out,
                r#"<defs><marker id="{marker_id}" refX="11" refY="5.8" markerWidth="20" markerHeight="28" orient="172.5" stroke-width="1.2"><path d="M 14.4 5.6 L 7.2 10.4 L 8.8 5.6 L 7.2 0.8 Z"/></marker></defs><circle cx="{cx_text}" cy="{cy}" r="22""#,
                marker_id = escape_attr_display(marker_id),
                cy = fmt(cy)
            );
            if bands.is_some() {
                label_ctx.write_shadow_attr(out);
            } else {
                out.push_str(r#" filter="""#);
            }
            let _ = write!(
                out,
                r#"/><line marker-end="{marker_url}" transform="translate({cx_text}, {top})"/>"#,
                marker_url = escape_attr_display(marker_url),
                top = fmt(cy - 22.0)
            );
        }
        "entity" => {
            let _ = write!(
                out,
                r#"<circle cx="{cx_text}" cy="{cy}" r="22" width="{width}" height="{height}"/><line x1="{left}" x2="{right}" y1="{bottom}" y2="{bottom}" stroke-width="2"/>"#,
                cy = fmt(cy),
                width = fmt(node.width),
                height = fmt(height),
                left = fmt(cx - 22.0),
                right = fmt(cx + 22.0),
                bottom = fmt(cy + 22.0)
            );
        }
        _ => {}
    }
    let label_y = bands
        .as_ref()
        .map(|band| band.label_center_y)
        .unwrap_or_else(|| {
            actor_y
                + height / 2.0
                + match actor_type {
                    "actor" => 35.0,
                    "boundary" => 15.0,
                    "control" if placement.footer => 27.0,
                    "control" => 34.0,
                    "entity" if placement.footer => 15.0,
                    "entity" => 30.0,
                    _ => 0.0,
                }
        });
    label_ctx.write_actor_man(out, cx, label_y, actor)?;
    out.push_str("</g>");
    Ok(())
}

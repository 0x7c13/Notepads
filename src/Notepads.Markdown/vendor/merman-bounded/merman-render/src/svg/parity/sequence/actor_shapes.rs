use super::super::*;
use super::SequenceEmitCheckpoints;
use super::geometry::node_left_top;
use super::math_label::sequence_katex_label;
use crate::sequence::{SEQUENCE_GLYPH_BAND_HEIGHT, SequenceActorBands, SequenceMathHeightMode};
use merman_core::diagrams::sequence::SequenceActor;

pub(super) struct ActorLifelineIdentity<'a> {
    pub(super) actor_id: &'a str,
    pub(super) actor_type: &'a str,
}

pub(super) struct ActorLabelContext<'a> {
    wrap_width_px: f64,
    diagram_id: &'a str,
    measurer: &'a dyn TextMeasurer,
    style: &'a TextStyle,
    config: &'a merman_core::MermaidConfig,
    math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    checkpoints: SequenceEmitCheckpoints<'a>,
}

impl<'a> ActorLabelContext<'a> {
    pub(super) fn new(
        wrap_width_px: f64,
        diagram_id: &'a str,
        measurer: &'a dyn TextMeasurer,
        style: &'a TextStyle,
        config: &'a merman_core::MermaidConfig,
        math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
        checkpoints: SequenceEmitCheckpoints<'a>,
    ) -> Self {
        Self {
            wrap_width_px,
            diagram_id,
            measurer,
            style,
            config,
            math_renderer,
            checkpoints,
        }
    }

    fn write_actor(&self, out: &mut String, cx: f64, cy: f64, actor: &SequenceActor) -> Result<()> {
        write_actor_label(
            out,
            cx,
            cy,
            &actor.description,
            actor.wrap,
            "actor-box",
            self,
        )
    }

    pub(super) fn write_actor_man(
        &self,
        out: &mut String,
        cx: f64,
        cy: f64,
        actor: &SequenceActor,
    ) -> Result<()> {
        write_actor_label(
            out,
            cx,
            cy,
            &actor.description,
            actor.wrap,
            "actor-man",
            self,
        )
    }

    pub(super) fn is_neo(&self) -> bool {
        crate::config::config_diagram_look(self.config.as_value()).is_neo()
    }

    pub(super) fn bands(&self, node: &LayoutNode, footer: bool) -> Option<SequenceActorBands> {
        self.is_neo().then(|| {
            let (_, top) = node_left_top(node);
            // Neo row height is the shared glyph band plus two 6px gaps and the label block.
            // Derive the label block from that datum so host bbox differences cannot move glyphs.
            let text_height =
                (node.height - crate::sequence::SEQUENCE_GLYPH_BAND_HEIGHT - 12.0).max(1.0);
            SequenceActorBands::new(top, node.height, text_height, footer)
        })
    }

    pub(super) fn write_shadow_attr(&self, out: &mut String) {
        if self.is_neo() {
            let _ = write!(
                out,
                r#" filter="url(#{}-drop-shadow)""#,
                escape_attr(self.diagram_id)
            );
        }
    }

    pub(super) fn write_style_attr(&self, out: &mut String, actor_type: &str, actor_index: usize) {
        let config = self.config.as_value();
        let mut style = String::new();
        let mut declaration = |property: &str, color: &str| {
            if !style.is_empty() {
                style.push(' ');
            }
            let color = super::super::util::cssom_color_value(color);
            let _ = write!(style, "{property}: {color};");
        };
        if matches!(
            config.get("theme").and_then(serde_json::Value::as_str),
            Some("redux-color" | "redux-dark-color")
        ) {
            for (property, key) in [("stroke", "borderColorArray"), ("fill", "bkgColorArray")] {
                if let Some(palette) = config
                    .get("themeVariables")
                    .and_then(|theme| theme.get(key))
                    .and_then(serde_json::Value::as_array)
                    && !palette.is_empty()
                    && let Some(color) = palette[actor_index % palette.len()].as_str()
                {
                    declaration(property, color);
                }
            }
        } else {
            let theme = PresentationTheme::new(config).sequence_diagram();
            if matches!(actor_type, "actor" | "boundary" | "control" | "database") {
                declaration("stroke", theme.actor_border.as_str());
            }
            if actor_type == "control" {
                declaration("fill", theme.actor_fill.as_str());
            }
        }
        if !style.is_empty() {
            let _ = write!(out, r#" style="{}""#, escape_attr(&style));
        }
    }
}

pub(super) fn is_actor_man_variant(actor_type: &str) -> bool {
    matches!(actor_type, "actor" | "boundary" | "control" | "entity")
}

pub(super) fn write_actor_man_lifeline(
    out: &mut String,
    idx: usize,
    cx: f64,
    y1: f64,
    y2: f64,
    actor_id: &str,
) {
    let _ = write!(
        out,
        r##"<g><line id="actor{idx}" x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" class="actor-line 200" stroke-width="0.5px" stroke="#999" name="{name}" data-et="life-line" data-id="{data_id}"/></g>"##,
        idx = idx,
        cx = fmt(cx),
        y1 = fmt(y1),
        y2 = fmt(y2),
        name = escape_xml(actor_id),
        data_id = escape_attr(actor_id),
    );
}

impl ActorLabelContext<'_> {
    pub(super) fn write_lifeline_root_open(
        &self,
        out: &mut String,
        idx: usize,
        cx: f64,
        y1: f64,
        y2: f64,
        identity: ActorLifelineIdentity<'_>,
    ) {
        let ActorLifelineIdentity {
            actor_id,
            actor_type,
        } = identity;
        out.push_str("<g>");
        let root_class = if actor_type == "queue" {
            r#" class="actor actor-top""#
        } else {
            ""
        };
        let _ = write!(
            out,
            r##"<line id="actor{idx}" x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" class="actor-line 200" stroke-width="0.5px" stroke="#999" name="{name}" data-et="life-line" data-id="{data_id}"/><g id="root-{idx}"{root_class} data-et="participant" data-type="{actor_type}" data-id="{data_id}"{look_attr}"##,
            idx = idx,
            cx = fmt(cx),
            y1 = fmt(y1),
            y2 = fmt(y2),
            name = escape_xml(actor_id),
            data_id = escape_attr(actor_id),
            root_class = root_class,
            actor_type = escape_attr(actor_type),
            look_attr = if self.is_neo() {
                r#" data-look="neo""#
            } else {
                ""
            },
        );
        if actor_type == "collections" {
            self.write_shadow_attr(out);
        }
        out.push('>');
    }
}

pub(super) fn write_collection_actor_shape(
    out: &mut String,
    n: &LayoutNode,
    actor_id: &str,
    actor: &SequenceActor,
    placement_class: &str,
    actor_index: usize,
    label_ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    const OFFSET: f64 = 6.0;
    let (x, y) = node_left_top(n);
    let height = n.height - if label_ctx.is_neo() { OFFSET } else { 0.0 };
    for (rect_x, rect_y, class) in [
        (x, y, format!("actor {placement_class}")),
        (x - OFFSET, y + OFFSET, "actor".to_string()),
    ] {
        let _ = write!(
            out,
            r##"<rect x="{x}" y="{y}" fill="#eaeaea" stroke="#666" width="{w}" height="{h}" name="{name}" class="{class}""##,
            x = fmt(rect_x),
            y = fmt(rect_y),
            w = fmt(n.width),
            h = fmt(height),
            name = escape_attr(actor_id)
        );
        if label_ctx.is_neo() {
            out.push_str(r#" data-look="neo""#);
        }
        label_ctx.write_style_attr(out, "collections", actor_index);
        out.push_str("/>");
    }
    label_ctx.write_actor(out, n.x - OFFSET, y + OFFSET + height / 2.0, actor)
}

pub(super) fn write_queue_actor_shape(
    out: &mut String,
    n: &LayoutNode,
    actor: &SequenceActor,
    actor_index: usize,
    label_ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    let (x, y) = node_left_top(n);
    let ry = n.height / 2.0;
    let rx = ry / (2.5 + n.height / 50.0);
    let y_mid = y + ry;
    for (index, tx) in [rx, n.width - rx].into_iter().enumerate() {
        let _ = write!(
            out,
            r#"<g transform="translate({}, {})""#,
            fmt(tx),
            fmt(-n.height / 2.0)
        );
        if index == 0 {
            label_ctx.write_shadow_attr(out);
        }
        label_ctx.write_style_attr(out, "queue", actor_index);
        let _ = write!(
            out,
            r#"><path d="M {x},{y_mid} a {rx},{ry} 0 0 0 0,{h}"#,
            x = fmt(x),
            y_mid = fmt(y_mid),
            rx = fmt(rx),
            ry = fmt(ry),
            h = fmt(n.height)
        );
        if index == 0 {
            let _ = write!(
                out,
                " h {} a {},{} 0 0 0 0,-{} Z",
                fmt(n.width - 2.0 * rx),
                fmt(rx),
                fmt(ry),
                fmt(n.height)
            );
        }
        out.push_str("\"/></g>");
    }
    label_ctx.write_actor(out, n.x, y_mid, actor)
}

pub(super) fn write_database_actor_shape(
    out: &mut String,
    n: &LayoutNode,
    actor: &SequenceActor,
    actor_index: usize,
    placement_class: &str,
    legacy_actor_height: f64,
    label_ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    let (x, y) = node_left_top(n);
    let w = n.width / 3.0;
    let bands = label_ctx.bands(n, placement_class == "actor-bottom");
    let h = if bands.is_some() {
        SEQUENCE_GLYPH_BAND_HEIGHT
    } else {
        w
    };
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let cylinder_y = bands
        .as_ref()
        .map_or(y, |band| band.glyph_bottom_y - h - ry);
    let _ = write!(
        out,
        r#"<g class="actor {placement_class}" transform="translate({}, {})""#,
        fmt(w),
        fmt(ry)
    );
    label_ctx.write_shadow_attr(out);
    label_ctx.write_style_attr(out, "database", actor_index);
    let _ = write!(
        out,
        r#"><path d="M {x},{y} a {rx},{ry} 0 0 0 {w},0 a {rx},{ry} 0 0 0 -{w},0 l 0,{h2} a {rx},{ry} 0 0 0 {w},0 l 0,-{h2}"/></g>"#,
        x = fmt(x),
        y = fmt(cylinder_y + ry),
        rx = fmt(rx),
        ry = fmt(ry),
        w = fmt(w),
        h2 = fmt(h - 2.0 * ry)
    );
    let y_text = bands
        .as_ref()
        .map_or(y + 35.0 + legacy_actor_height / 2.0, |band| {
            band.label_center_y
        });
    label_ctx.write_actor(out, n.x, y_text, actor)
}

pub(super) fn write_rect_actor_shape(
    out: &mut String,
    n: &LayoutNode,
    actor_id: &str,
    actor: &SequenceActor,
    placement_class: &str,
    actor_index: usize,
    label_ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    let (x, y) = node_left_top(n);
    let custom_class = actor_custom_class(actor);
    let fill = if custom_class.is_some() {
        "#EDF2AE"
    } else {
        "#eaeaea"
    };
    let class = custom_class
        .map(|c| format!("{c} {placement_class}"))
        .unwrap_or_else(|| format!("actor {placement_class}"));
    let is_neo = crate::config::config_diagram_look(label_ctx.config.as_value()).as_str() == "neo";
    let _ = write!(
        out,
        r##"<rect x="{x}" y="{y}" fill="{fill}" stroke="#666" width="{w}" height="{h}" name="{name}" rx="{radius}" ry="{radius}" class="{class}""##,
        x = fmt(x),
        y = fmt(y),
        w = fmt(n.width),
        h = fmt(n.height),
        name = escape_xml(actor_id),
        fill = escape_xml_display(fill),
        class = escape_attr(&class),
        radius = if is_neo { 6 } else { 3 },
    );
    if is_neo {
        let _ = write!(
            out,
            r#" data-look="neo" filter="url(#{}-drop-shadow)""#,
            escape_attr(label_ctx.diagram_id)
        );
    }
    label_ctx.write_style_attr(out, "participant", actor_index);
    out.push_str("/>");
    label_ctx.write_actor(out, n.x, n.y, actor)
}

fn actor_custom_class(actor: &SequenceActor) -> Option<&str> {
    actor
        .properties
        .get("class")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn write_actor_label(
    out: &mut String,
    cx: f64,
    cy: f64,
    label: &str,
    wrap: bool,
    label_class: &str,
    ctx: &ActorLabelContext<'_>,
) -> Result<()> {
    ctx.checkpoints.checkpoint()?;
    let wrapped_label = if wrap {
        Some(
            crate::sequence::wrap_sequence_label_like_mermaid_lines(
                label,
                ctx.measurer,
                ctx.style,
                ctx.wrap_width_px,
                ctx.checkpoints.text(),
            )?
            .join("<br>"),
        )
    } else {
        None
    };
    let rendered_label = wrapped_label.as_deref().unwrap_or(label);

    if let Some(katex) = sequence_katex_label(
        rendered_label,
        ctx.measurer,
        ctx.style,
        ctx.config,
        ctx.math_renderer,
        SequenceMathHeightMode::Actor,
        ctx.checkpoints,
    )? {
        let x = cx - katex.width / 2.0;
        let y = cy - katex.height / 2.0;
        out.push_str("<switch>");
        let _ = write!(
            out,
            r#"<foreignObject x="{x}" y="{y}" width="{w}" height="{h}"><div class="actor {label_class}" xmlns="http://www.w3.org/1999/xhtml" style="height: 100%; width: 100%;"><div style="text-align: center; vertical-align: middle;">{html}</div></div></foreignObject>"#,
            x = fmt(x),
            y = fmt(y),
            w = fmt(katex.width),
            h = fmt(katex.height),
            html = katex.html,
        );
        super::text::write_centered_tspan_text(
            out,
            cx,
            cy,
            rendered_label,
            &format!("actor {label_class}"),
            ctx.style,
            ctx.checkpoints,
        )?;
        out.push_str("</switch>");
        return ctx.checkpoints.checkpoint();
    }

    super::text::write_centered_tspan_text(
        out,
        cx,
        cy,
        rendered_label,
        &format!("actor {label_class}"),
        ctx.style,
        ctx.checkpoints,
    )
}

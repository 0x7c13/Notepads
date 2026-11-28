use super::super::*;
use super::SequenceEmitCheckpoints;
use super::geometry::node_left_top;
use super::model::SequenceSvgModel;
use merman_core::svg_security::{MermaidNavigationSecurity, prepare_mermaid_navigation_href};
use rustc_hash::FxHashMap;

use crate::text::{TextMeasurer, TextStyle};

#[derive(Clone, Copy)]
pub(super) struct SequenceActorPopupOptions<'a> {
    pub(super) force_menus: bool,
    pub(super) mirror_actors: bool,
    pub(super) actor_height: f64,
    pub(super) wrap_padding: f64,
    pub(super) box_margin: f64,
    pub(super) actor_text_style: &'a TextStyle,
    pub(super) measurer: &'a dyn TextMeasurer,
}

pub(super) fn render_sequence_actor_popup_menus(
    out: &mut String,
    model: &SequenceSvgModel,
    nodes_by_id: &FxHashMap<&str, &LayoutNode>,
    sanitize_config: &merman_core::MermaidConfig,
    options: SequenceActorPopupOptions,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<()> {
    // Mermaid emits actor popup menus (links/link directives) as root-level
    // `<g class="actorPopupMenu">` groups after messages.
    for (actor_cnt, actor_id) in model.actor_order.iter().enumerate() {
        checkpoints.checkpoint_loop(actor_cnt)?;
        let Some(actor) = model.actors.get(actor_id) else {
            continue;
        };
        if actor.links.is_empty() {
            continue;
        }
        let actor_custom_class = actor
            .properties
            .get("class")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let popup_display = if options.force_menus {
            "block !important"
        } else {
            "none"
        };
        let popup_fill = if actor_custom_class.is_some() {
            "#EDF2AE"
        } else {
            "#eaeaea"
        };
        let popup_actor_pos_class = if options.mirror_actors {
            "actor-bottom"
        } else {
            "actor-top"
        };
        let popup_panel_class = actor_custom_class
            .map(|c| format!("actorPopupMenuPanel {c} {popup_actor_pos_class}"))
            .unwrap_or_else(|| format!("actorPopupMenuPanel actor {popup_actor_pos_class}"));

        let node_id = if options.mirror_actors {
            format!("actor-bottom-{actor_id}")
        } else {
            format!("actor-top-{actor_id}")
        };
        let Some(n) = nodes_by_id.get(node_id.as_str()).copied() else {
            continue;
        };
        let (x, _y) = node_left_top(n);

        let is_neo = crate::config::config_diagram_look(sanitize_config.as_value()).is_neo();
        let rect_height = crate::sequence::sequence_actor_popup_rect_height(
            &actor.actor_type,
            n.height,
            options.actor_height,
            is_neo,
            options.mirror_actors,
        );
        let radius = match actor.actor_type.as_str() {
            "collections" | "queue" | "database" => 0,
            _ if is_neo => 6,
            _ => 3,
        };
        let mut link_y: f64 = 20.0;
        let min_menu_width = crate::sequence::sequence_actor_popup_min_width(
            actor,
            options.measurer,
            options.actor_text_style,
            options.wrap_padding,
            options.box_margin,
        );
        let panel_width = n.width.max(min_menu_width);
        let panel_height = crate::sequence::sequence_actor_popup_panel_height(actor.links.len());
        let text_style = super::settings::sequence_text_style_attribute(options.actor_text_style);

        let _ = write!(
            out,
            r##"<g id="actor{idx}_popup" class="actorPopupMenu" display="{display}">"##,
            idx = actor_cnt,
            display = escape_attr(popup_display),
        );
        let _ = write!(
            out,
            r##"<rect class="{class}" x="{x}" y="{y}" fill="{fill}" stroke="#666" width="{w}" height="{h}" rx="{radius}" ry="{radius}"/>"##,
            class = escape_attr(&popup_panel_class),
            x = fmt(x),
            y = fmt(rect_height),
            w = fmt(panel_width),
            h = fmt(panel_height),
            fill = escape_xml_display(popup_fill),
        );

        for (link_index, (label, url)) in actor.links.iter().enumerate() {
            checkpoints.checkpoint_loop(link_index)?;
            let Some(href) = url.as_str() else {
                continue;
            };
            let security_level_loose = sanitize_config.get_str("securityLevel") == Some("loose");
            let href = merman_core::utils::sanitize_url(href);
            let href = prepare_mermaid_navigation_href(
                &href,
                MermaidNavigationSecurity::from_security_level_loose(security_level_loose),
            );
            let target_attr = if security_level_loose {
                r#" target="_blank""#
            } else {
                ""
            };
            let text_x = x + 10.0;
            let text_y = rect_height + link_y + 10.0;
            if let Some(href) = href {
                let _ = write!(
                    out,
                    r##"<a xlink:href="{href}"{target}><text x="{x}" y="{y}" dominant-baseline="central" alignment-baseline="central" class="actor" style="text-anchor: start; {style}"><tspan x="{x}" dy="0">{label}</tspan></text></a>"##,
                    href = href.as_serialized_str(),
                    target = target_attr,
                    x = fmt(text_x),
                    y = fmt(text_y),
                    style = escape_attr(&text_style),
                    label = escape_xml(label)
                );
            } else {
                let _ = write!(
                    out,
                    r##"<a><text x="{x}" y="{y}" dominant-baseline="central" alignment-baseline="central" class="actor" style="text-anchor: start; {style}"><tspan x="{x}" dy="0">{label}</tspan></text></a>"##,
                    x = fmt(text_x),
                    y = fmt(text_y),
                    style = escape_attr(&text_style),
                    label = escape_xml(label)
                );
            }
            link_y += 30.0;
        }

        out.push_str("</g>");
    }
    checkpoints.checkpoint()
}

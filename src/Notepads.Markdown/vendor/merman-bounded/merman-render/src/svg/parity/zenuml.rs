use super::*;
use crate::zenuml::{
    ZENUML_FRAME_TITLE_CLASS, ZenumlArrowStyle, ZenumlCommentLayout, ZenumlCreationLayout,
    ZenumlDiagramLayout, ZenumlFragmentLayout, ZenumlLayoutFragmentKind, ZenumlMessageLayout,
    ZenumlParticipantLayout, ZenumlReturnLayout, ZenumlSelfCallLayout, ZenumlSourceStyle,
    resolve_zenuml_emojis_in_text, resolve_zenuml_title, zenuml_emoji_unicode,
};
use merman_core::diagrams::zenuml::ZenumlDiagramRenderModel;

const FRAME_HEADER_HEIGHT: f64 = 28.0;
const CONTENT_PADDING: f64 = 10.0;

pub(super) fn render_zenuml_diagram_svg_model(
    layout: &ZenumlDiagramLayout,
    model: &ZenumlDiagramRenderModel,
    effective_config: &merman_core::MermaidConfig,
    diagram_title: Option<&str>,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("zenuml");
    let title = resolve_zenuml_title(model, diagram_title);
    let source_style = ZenumlSourceStyle::new(effective_config);
    let theme = &source_style;
    let vertical_scale = theme.typography.vertical_scale();
    let content_left = 1.0 + CONTENT_PADDING + layout.frame_border_left;
    let view_width =
        layout.width + content_left + CONTENT_PADDING + layout.frame_border_right + 1.0;
    let view_height =
        layout.height + (CONTENT_PADDING * 2.0 + FRAME_HEADER_HEIGHT - 1.0) * vertical_scale;
    let use_max_width = effective_config
        .as_value()
        .get("sequence")
        .and_then(|sequence| sequence.get("useMaxWidth"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let bounds = root_svg::DiagramBounds::from_view_box(0.0, 0.0, view_width, view_height);
    let root_spec = root_svg::RootViewportSpec::mermaid(bounds, use_max_width);
    let mut out = String::new();
    let mut chrome = root_svg::RootChrome::new(diagram_id, "zenuml");
    chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Zenuml, diagram_id)
            .write_open(&mut out, root_spec, chrome)?;
    options.checkpoint_emit()?;

    out.push_str("<defs><style>");
    let css = zenuml_css(
        diagram_id,
        theme,
        theme.color("titleTextColor", ZENUML_DEFAULT_TITLE_FILL),
    );
    out.push_str(&crate::svg::pipeline::escape_xml_text_with_checkpoints(
        &css,
        &mut || options.checkpoint_emit(),
    )?);
    out.push_str("</style></defs>");
    options.checkpoint_emit()?;
    let _ = write!(
        &mut out,
        r#"<rect class="frame-border-outer" x="0" y="0" width="{}" height="{}" rx="4"/><rect class="frame-border-inner" x="1" y="1" width="{}" height="{}" rx="3"/>"#,
        fmt(view_width),
        fmt(view_height),
        fmt(view_width - 2.0),
        fmt(view_height - 2.0),
    );
    let header_y = (FRAME_HEADER_HEIGHT + 6.0) * vertical_scale;
    let _ = write!(
        &mut out,
        r#"<line class="frame-header-line" x1="1" y1="{}" x2="{}" y2="{}"/>"#,
        fmt(header_y - 0.5),
        fmt(view_width - 1.0),
        fmt(header_y - 0.5),
    );
    if let Some(title) = title {
        let _ = write!(
            &mut out,
            r#"<text x="5" y="{}" dominant-baseline="central" class="{}">{}</text>"#,
            fmt((header_y - 0.5) / 2.0),
            ZENUML_FRAME_TITLE_CLASS,
            escape_xml(&resolve_emoji_in_text(title)),
        );
    }
    let _ = write!(
        &mut out,
        r#"<g class="zenuml-content" transform="translate({}, {})">"#,
        fmt(content_left),
        fmt(header_y),
    );
    let creation_names = layout
        .creations
        .iter()
        .map(|creation| creation.participant.name.as_str())
        .collect::<std::collections::HashSet<_>>();

    for group in &layout.groups {
        let rect_x = group.x - 0.5;
        let rect_y = group.y - 2.0 * vertical_scale;
        let title = (!group.name.is_empty()).then(|| escape_xml(&group.name));
        let _ = write!(
            &mut out,
            r#"<g class="participant-group"><rect x="{}" y="{}" width="{}" height="{}" class="group-outline" stroke-width="1" stroke-dasharray="4 3"/>"#,
            fmt(rect_x),
            fmt(rect_y),
            fmt(group.width + 1.0),
            fmt(group.height + 1.5 * vertical_scale),
        );
        if let Some(title) = title {
            let _ = write!(
                &mut out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" class="group-title-bg"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="middle" class="group-title-text">{}</text>"#,
                fmt(group.x + 0.5),
                fmt(rect_y + 0.5),
                fmt(group.width - 1.0),
                fmt(19.5 * vertical_scale),
                fmt(group.x + group.width / 2.0),
                fmt(group.y + 10.0 * vertical_scale),
                title,
            );
        }
        out.push_str("</g>");
    }
    for lifeline in &layout.lifelines {
        let _ = write!(
            &mut out,
            r#"<line class="lifeline" data-participant="{}" x1="{}" y1="{}" x2="{}" y2="{}" stroke-dasharray="5,5" stroke-dashoffset="5" shape-rendering="crispEdges"/>"#,
            escape_attr(&lifeline.participant_name),
            fmt(lifeline.x + 0.5),
            fmt(lifeline.top_y),
            fmt(lifeline.x + 0.5),
            fmt(lifeline.bottom_y),
        );
    }
    for participant in layout
        .participants
        .iter()
        .filter(|participant| !creation_names.contains(participant.name.as_str()))
    {
        render_participant(&mut out, participant, theme);
    }
    for occurrence in &layout.occurrences {
        let _ = write!(
            &mut out,
            r#"<rect class="occurrence" data-statement="{}" data-participant="{}" x="{}" y="{}" width="{}" height="{}" rx="1"/>"#,
            escape_attr(&occurrence.statement_id),
            escape_attr(&occurrence.participant_name),
            fmt(occurrence.x + 1.0),
            fmt(occurrence.y + 1.0),
            fmt(occurrence.width - 2.0),
            fmt(occurrence.height - 2.0),
        );
    }
    for creation in &layout.creations {
        render_participant(&mut out, &creation.participant, theme);
    }
    for message in &layout.messages {
        render_message(&mut out, message, theme);
    }
    for self_call in &layout.self_calls {
        render_self_call(&mut out, self_call, theme);
    }
    for creation in &layout.creations {
        render_creation(&mut out, creation, theme);
    }
    for returned in &layout.returns {
        render_return(&mut out, returned, theme);
    }
    for fragment in &layout.fragments {
        render_fragment(&mut out, fragment, theme);
    }
    for divider in &layout.dividers {
        let center_x = divider.width / 2.0;
        let label = divider
            .label
            .trim()
            .trim_start_matches('=')
            .trim_end_matches('=')
            .trim();
        let label = resolve_emoji_in_text(label);
        let rect_width = divider.label_width + 17.0;
        let rect_height = 27.0 * vertical_scale;
        let rect_x = center_x - rect_width / 2.0;
        let rect_y = divider.y - rect_height / 2.0;
        let outer_left = rect_x - 0.5;
        let outer_right = rect_x + rect_width + 0.5;
        let _ = write!(
            &mut out,
            r#"<g class="divider" data-statement="{}"><line x1="0" y1="{}" x2="{}" y2="{}" class="divider-line"/><line x1="{}" y1="{}" x2="{}" y2="{}" class="divider-line"/><rect x="{}" y="{}" width="{}" height="{}" rx="2" class="divider-bg"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" class="divider-label">{}</text></g>"#,
            escape_attr(&divider.statement_id),
            fmt(divider.y),
            fmt(outer_left),
            fmt(divider.y),
            fmt(outer_right),
            fmt(divider.y),
            fmt(divider.width),
            fmt(divider.y),
            fmt(rect_x),
            fmt(rect_y),
            fmt(rect_width),
            fmt(rect_height),
            fmt(center_x),
            fmt(divider.y),
            escape_xml(&label),
        );
    }
    for comment in &layout.comments {
        render_comment(&mut out, comment, theme);
    }
    out.push_str("</g></svg>");
    options.checkpoint_emit()?;
    let rooted_svg = root_document.complete(out)?;
    Ok(rooted_svg)
}

fn render_participant(
    out: &mut String,
    participant: &ZenumlParticipantLayout,
    theme: &ZenumlSourceStyle,
) {
    let vertical_scale = theme.typography.vertical_scale();
    let y = participant.y;
    let x = participant.x - participant.width / 2.0 + 1.0;
    let fill = participant
        .color
        .as_deref()
        .map(|color| {
            let color = if color.starts_with('#') {
                color.to_string()
            } else {
                format!("#{color}")
            };
            format!(r#" style="fill:{};""#, escape_attr(&color))
        })
        .unwrap_or_default();
    let _ = write!(
        out,
        r#"<g class="participant" data-participant="{}"><rect class="participant-box" x="{}" y="{}" width="{}" height="{}" rx="3"{}/>"#,
        escape_attr(&participant.name),
        fmt(x),
        fmt(y + 1.0),
        fmt(participant.width - 2.0),
        fmt(participant.height - 2.0),
        fill,
    );
    if participant.is_starter {
        render_participant_icon(
            out,
            "actor",
            participant.x - 14.0 * vertical_scale,
            y + 8.0 * vertical_scale,
            vertical_scale,
        );
        out.push_str("</g>");
        return;
    }
    let text_y = y + participant.height / 2.0 - 0.25 * vertical_scale;
    let label_y = if participant.stereotype.is_some() {
        text_y + 8.0 * vertical_scale
    } else {
        text_y
    };
    let icon = participant
        .participant_type
        .as_deref()
        .and_then(crate::zenuml::zenuml_participant_icon_key);
    let emoji = participant
        .emoji
        .as_deref()
        .map(|shortcode| zenuml_emoji_unicode(shortcode).unwrap_or(shortcode));
    let mut text_x = participant.x;
    let mut text_anchor = "middle";
    if let Some(icon) = icon {
        let emoji_extra = if emoji.is_some() {
            20.0 * vertical_scale
        } else {
            0.0
        };
        let group_width = 44.0 * vertical_scale + participant.label_width + emoji_extra;
        let group_x = participant.x - group_width / 2.0;
        let icon_x = group_x + 4.0 * vertical_scale;
        let icon_y = y
            + (participant.height - 24.0 * vertical_scale) / 2.0
            + if icon == "boundary" {
                2.75 * vertical_scale
            } else {
                0.0
            };
        render_participant_icon(out, icon, icon_x, icon_y, vertical_scale);
        if let Some(emoji) = emoji {
            let emoji_x = icon_x + 28.0 * vertical_scale;
            render_participant_emoji(out, emoji, emoji_x, label_y);
            text_x = emoji_x + 24.0 * vertical_scale;
        } else {
            text_x = group_x + 36.0 * vertical_scale;
        }
        text_anchor = "start";
    } else if let Some(emoji) = emoji {
        let group_width = 28.0 * vertical_scale + participant.label_width;
        let inner_width = participant
            .stereotype_width
            .unwrap_or_default()
            .max(group_width);
        let group_x = participant.x - inner_width / 2.0;
        render_participant_emoji(out, emoji, group_x, label_y);
        text_x = group_x + 24.0 * vertical_scale;
        text_anchor = "start";
    }
    if let Some(stereotype) = &participant.stereotype {
        let stereotype_x = if icon.is_some() {
            text_x + participant.label_width / 2.0
        } else {
            participant.x
        };
        let _ = write!(
            out,
            r#"<text class="stereotype-label" x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}" style="fill:{};">«{}»</text>"#,
            fmt(stereotype_x),
            fmt(text_y - 8.0 * vertical_scale),
            fmt(theme.typography.size(16.0)),
            escape_attr(theme.color("actorTextColor", "#222")),
            escape_xml(stereotype),
        );
    }
    let text_length = if participant.name.contains(':') {
        format!(
            r#" textLength="{}" lengthAdjust="spacing""#,
            fmt(participant.label_width)
        )
    } else {
        String::new()
    };
    let _ = write!(
        out,
        r#"<text class="participant-label" x="{}" y="{}" text-anchor="{}" dominant-baseline="central"{}>{}</text></g>"#,
        fmt(text_x),
        fmt(label_y),
        text_anchor,
        text_length,
        escape_xml(&participant.label),
    );
}

fn render_participant_emoji(out: &mut String, emoji: &str, x: f64, y: f64) {
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" dominant-baseline="central" font-family="'Apple Color Emoji','Segoe UI Emoji','Noto Color Emoji','Twemoji Mozilla',sans-serif" class="participant-emoji">{}</text>"#,
        fmt(x),
        fmt(y),
        escape_xml(emoji),
    );
}

struct ParticipantIcon {
    svg: &'static str,
    view_box_width: f64,
    view_box_height: f64,
    attributes: &'static str,
}

fn render_participant_icon(out: &mut String, key: &str, x: f64, y: f64, typography_scale: f64) {
    let Some(icon) = participant_icon(key) else {
        return;
    };
    let scale = 24.0 * typography_scale / icon.view_box_width.max(icon.view_box_height);
    let content = svg_asset_content(icon.svg);
    let attributes = if icon.attributes.is_empty() {
        String::new()
    } else {
        format!(" {}", icon.attributes)
    };
    let _ = write!(
        out,
        r#"<g class="participant-icon" data-icon="{}" transform="translate({}, {}) scale({})"{}>{}</g>"#,
        escape_attr(key),
        fmt(x),
        fmt(y),
        fmt(scale),
        attributes,
        content,
    );
}

fn participant_icon(key: &str) -> Option<ParticipantIcon> {
    Some(match key {
        "actor" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/actor.svg"),
            view_box_width: 24.0,
            view_box_height: 24.0,
            attributes: r#"fill="none""#,
        },
        "database" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/database.svg"),
            view_box_width: 24.0,
            view_box_height: 24.0,
            attributes: r#"fill="none""#,
        },
        "boundary" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/boundary.svg"),
            view_box_width: 101.0,
            view_box_height: 78.0,
            attributes: r#"fill="none""#,
        },
        "control" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/control.svg"),
            view_box_width: 77.0,
            view_box_height: 86.0,
            attributes: r#"fill="none""#,
        },
        "entity" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/entity.svg"),
            view_box_width: 77.0,
            view_box_height: 80.0,
            attributes: r#"fill="none""#,
        },
        "ec2" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/ec2.svg"),
            view_box_width: 48.0,
            view_box_height: 48.0,
            attributes: "",
        },
        "lambda" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/lambda.svg"),
            view_box_width: 48.0,
            view_box_height: 48.0,
            attributes: "",
        },
        "sqs" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/sqs.svg"),
            view_box_width: 48.0,
            view_box_height: 48.0,
            attributes: "",
        },
        "sns" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/sns.svg"),
            view_box_width: 48.0,
            view_box_height: 48.0,
            attributes: "",
        },
        "iam" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/iam.svg"),
            view_box_width: 48.0,
            view_box_height: 48.0,
            attributes: "",
        },
        "azurefunction" => ParticipantIcon {
            svg: include_str!("../../../assets/zenuml/azurefunction.svg"),
            view_box_width: 18.0,
            view_box_height: 18.0,
            attributes: "",
        },
        _ => return None,
    })
}

fn render_message(out: &mut String, message: &ZenumlMessageLayout, theme: &ZenumlSourceStyle) {
    let left_to_right = message.from_x < message.to_x;
    let from_x = if left_to_right {
        message.from_x + 1.0
    } else {
        message.from_x
    };
    let to_x = if left_to_right {
        message.to_x
    } else {
        message.to_x + 1.0
    };
    let direction = if left_to_right { 1.0 } else { -1.0 };
    let label_x = (message.from_x + message.to_x) / 2.0 - direction * 3.5 + 0.5;
    let label_y = message.y - 3.5;
    let line_y = message.y - 0.5;
    let dash = if message.arrow_style == ZenumlArrowStyle::Dashed {
        r#" stroke-dasharray="6,4""#
    } else {
        ""
    };
    let _ = write!(
        out,
        r#"<g class="message" data-statement="{}"><line class="message-line" x1="{}" y1="{}" x2="{}" y2="{}"{} />"#,
        escape_attr(&message.statement_id),
        fmt(from_x),
        fmt(line_y),
        fmt(to_x),
        fmt(line_y),
        dash,
    );
    render_arrow_head(
        out,
        to_x,
        line_y,
        message.is_reverse,
        message.arrow_style,
        theme,
    );
    let _ = write!(
        out,
        r#"<text class="message-label" x="{}" y="{}" text-anchor="middle"{}>{}</text><text class="seq-number" x="{}" y="{}" text-anchor="end">{}</text></g>"#,
        fmt(label_x),
        fmt(label_y),
        style_attr(&message.style),
        escape_xml(&resolve_emoji_in_text(&message.label)),
        fmt(from_x.min(to_x) - 4.0),
        fmt(label_y),
        escape_xml(&message.number),
    );
}

fn render_self_call(out: &mut String, call: &ZenumlSelfCallLayout, theme: &ZenumlSourceStyle) {
    let vertical_scale = theme.typography.vertical_scale();
    let asynchronous = call.arrow_style == ZenumlArrowStyle::Open;
    let label_y = call.y + (if asynchronous { 15.0 } else { 12.0 }) * vertical_scale;
    let svg_y = call.y + (if asynchronous { 20.0 } else { 14.0 }) * vertical_scale;
    let path_end = if asynchronous { "L1,15" } else { "L14,15" };
    let arrow_tx = if asynchronous { 0.0 } else { 7.0 };
    let arrow_path = if asynchronous {
        "M1 1.25 L6.15 4.5 L1 7.75"
    } else {
        "M1 1.25 L6.15 4.5 L1 7.75 Z"
    };
    let line_color = theme.color("messageLineColor", theme.color("lineColor", "#000"));
    let arrow_fill = if asynchronous { "none" } else { line_color };
    let view_box = if vertical_scale == 1.0 {
        ""
    } else {
        r#" viewBox="0 0 30 24""#
    };
    let _ = write!(
        out,
        r##"<g class="message self-call" data-statement="{}"><svg x="{}" y="{}" width="{}" height="{}"{}><path d="M0,2 L26,2 Q28,2 28,4 L28,13 Q28,15 26,15 {}" fill="none" stroke="{}" stroke-width="2"/><g transform="translate({}, 10)"><svg height="10" width="7" viewBox="0 0 7 9"><g transform="scale(-1, 1) translate(-7, 0)"><path d="{}" stroke="{}" stroke-linecap="round" fill="{}" stroke-width="2"/></g></svg></g></svg><text x="{}" y="{}" text-anchor="start" class="message-label"{}>{}</text><text x="{}" y="{}" text-anchor="end" class="seq-number">{}</text></g>"##,
        escape_attr(&call.statement_id),
        fmt(call.x + 1.0),
        fmt(svg_y),
        fmt(30.0 * vertical_scale),
        fmt(24.0 * vertical_scale),
        view_box,
        path_end,
        escape_attr(line_color),
        fmt(arrow_tx),
        arrow_path,
        escape_attr(line_color),
        escape_attr(arrow_fill),
        fmt(call.x + 6.0),
        fmt(label_y),
        style_attr(&call.style),
        escape_xml(&resolve_emoji_in_text(call.label.trim())),
        fmt(call.x - 3.0),
        fmt(call.y + 12.0 * vertical_scale),
        escape_xml(&call.number),
    );
}

fn render_creation(out: &mut String, creation: &ZenumlCreationLayout, theme: &ZenumlSourceStyle) {
    let message = &creation.message;
    let participant = &creation.participant;
    let reverse = message.to_x < message.from_x;
    let to_x = if reverse {
        participant.x + participant.width / 2.0
    } else {
        participant.x - participant.width / 2.0
    };
    let from_x = if reverse {
        message.from_x
    } else {
        message.from_x + 1.0
    };
    let label_x = from_x + (to_x - from_x) / 2.0 + if reverse { 3.5 } else { -3.0 };
    let label_y = message.y - 3.0 * theme.typography.vertical_scale();
    let _ = write!(
        out,
        r#"<g class="creation" data-statement="{}"><line x1="{}" y1="{}" x2="{}" y2="{}" class="message-line" stroke-dasharray="6,4"/>"#,
        escape_attr(&creation.statement_id),
        fmt(from_x),
        fmt(message.y),
        fmt(to_x),
        fmt(message.y),
    );
    render_open_polyline(out, to_x, message.y, reverse, "arrow-head arrow-open");
    let rendered_label = render_creation_label(&message.label, &message.style);
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" text-anchor="middle" class="message-label">{}</text><text x="{}" y="{}" text-anchor="end" class="seq-number">{}</text></g>"#,
        fmt(label_x),
        fmt(label_y),
        rendered_label,
        fmt(from_x.min(to_x) - 4.0),
        fmt(label_y),
        escape_xml(&message.number),
    );
}

fn render_return(out: &mut String, returned: &ZenumlReturnLayout, theme: &ZenumlSourceStyle) {
    if returned.is_self {
        let icon_x = returned.from_x + 4.0;
        let icon_y = returned.y - 12.0 * theme.typography.vertical_scale();
        let _ = write!(
            out,
            r#"<g class="return return-self" data-statement="{}"><g transform="translate({},{}) scale(0.0234375)"><path d="M256 0C114.84 0 0 114.84 0 256s114.84 256 256 256 256-114.84 256-256S397.16 0 256 0Zm0 469.33c-117.63 0-213.33-95.7-213.33-213.33S138.37 42.67 256 42.67 469.33 138.37 469.33 256 373.63 469.33 256 469.33Z" class="return-icon"/><path d="M288 192h-87.16l27.58-27.58a21.33 21.33 0 1 0-30.17-30.17l-64 64a21.33 21.33 0 0 0 0 30.17l64 64a21.33 21.33 0 0 0 30.17-30.17l-27.58-27.58H288a53.33 53.33 0 0 1 0 106.67h-32a21.33 21.33 0 0 0 0 42.66h32a96 96 0 0 0 0-192Z" class="return-icon"/></g><text x="{}" y="{}" text-anchor="start" class="return-label">{}</text></g>"#,
            escape_attr(&returned.statement_id),
            fmt(icon_x),
            fmt(icon_y),
            fmt(icon_x + 16.0),
            fmt(returned.y - 1.0),
            escape_xml(&resolve_emoji_in_text(&returned.label)),
        );
        return;
    }
    let line_y = returned.y.floor();
    let label_x = returned.from_x.min(returned.to_x)
        + (returned.to_x - returned.from_x).abs() / 2.0
        + if returned.is_reverse { 3.5 } else { -3.5 };
    let label_y = line_y - 3.0;
    let _ = write!(
        out,
        r#"<g class="return" data-statement="{}"><line x1="{}" y1="{}" x2="{}" y2="{}" class="return-line"/>"#,
        escape_attr(&returned.statement_id),
        fmt(returned.from_x),
        fmt(line_y),
        fmt(returned.to_x),
        fmt(line_y),
    );
    render_open_polyline(
        out,
        returned.to_x,
        line_y,
        returned.is_reverse,
        "return-arrow",
    );
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" text-anchor="middle" class="return-label">{}</text><text x="{}" y="{}" text-anchor="end" class="seq-number">{}</text></g>"#,
        fmt(label_x),
        fmt(label_y),
        escape_xml(&resolve_emoji_in_text(&returned.label)),
        fmt(returned.from_x.min(returned.to_x) - 4.0),
        fmt(label_y),
        escape_xml(&returned.number),
    );
}

fn render_arrow_head(
    out: &mut String,
    tip_x: f64,
    tip_y: f64,
    points_left: bool,
    style: ZenumlArrowStyle,
    theme: &ZenumlSourceStyle,
) {
    let filled = style == ZenumlArrowStyle::Solid;
    let x = if points_left { tip_x } else { tip_x - 7.0 };
    let transform = if points_left {
        r#" transform="scale(-1, 1) translate(-7, 0)""#
    } else {
        ""
    };
    let close = if filled { " Z" } else { "" };
    let _ = write!(
        out,
        r##"<svg x="{}" y="{}" width="7" height="10" viewBox="0 0 7 9" overflow="visible" class="arrow-head{}"><g{}><path d="M1 1.25 L6.15 4.5 L1 7.75{}" stroke="{}" stroke-linecap="round" stroke-width="2" fill="{}"/></g></svg>"##,
        fmt(x),
        fmt(tip_y - 5.0),
        if filled { "" } else { " arrow-open" },
        transform,
        close,
        escape_attr(theme.color("messageLineColor", theme.color("lineColor", "#000"))),
        escape_attr(if filled {
            theme.color("messageLineColor", theme.color("lineColor", "#000"))
        } else {
            "none"
        }),
    );
}

fn render_open_polyline(out: &mut String, tip_x: f64, tip_y: f64, points_left: bool, class: &str) {
    let direction = if points_left { 1.0 } else { -1.0 };
    let base_x = tip_x + direction * 5.15;
    let _ = write!(
        out,
        r#"<polyline points="{},{} {},{} {},{}" fill="none" stroke-linecap="round" class="{}"/>"#,
        fmt(base_x),
        fmt(tip_y - 3.25),
        fmt(tip_x),
        fmt(tip_y),
        fmt(base_x),
        fmt(tip_y + 3.25),
        class,
    );
}

fn render_creation_label(
    label: &str,
    style: &std::collections::BTreeMap<String, String>,
) -> String {
    let style = style_attr(style);
    if let Some(inner) = label
        .strip_prefix('«')
        .and_then(|label| label.strip_suffix('»'))
    {
        if inner == "create" {
            return format!("<tspan{style}>{}</tspan>", escape_xml(label));
        }
        return format!(
            "<tspan{style}>«</tspan><tspan dx=\"4\"{style}>{}</tspan><tspan dx=\"4\"{style}>»</tspan>",
            escape_xml(inner)
        );
    }
    format!("<tspan{style}>{}</tspan>", escape_xml(label))
}

fn render_fragment(out: &mut String, fragment: &ZenumlFragmentLayout, theme: &ZenumlSourceStyle) {
    let vertical_scale = theme.typography.vertical_scale();
    let (class, kind_label) = match fragment.kind {
        ZenumlLayoutFragmentKind::Loop => ("loop", "Loop"),
        ZenumlLayoutFragmentKind::Alternative => ("alt", "Alt"),
        ZenumlLayoutFragmentKind::Parallel => ("par", "Par"),
        ZenumlLayoutFragmentKind::Optional => ("opt", "Opt"),
        ZenumlLayoutFragmentKind::Critical => ("critical", "Critical"),
        ZenumlLayoutFragmentKind::Section => ("section", "Section"),
        ZenumlLayoutFragmentKind::TryCatchFinally => ("tcf", "Try"),
        ZenumlLayoutFragmentKind::Reference => ("ref", "Ref"),
    };
    let header_x = fragment.x + 1.0;
    let header_y = fragment.header_y;
    let _ = write!(
        out,
        r#"<g class="fragment fragment-{}" data-statement="{}"><rect class="fragment-border" x="{}" y="{}" width="{}" height="{}" rx="4"/><rect class="fragment-header" x="{}" y="{}" width="{}" height="{}"/>"#,
        class,
        escape_attr(&fragment.statement_id),
        fmt(fragment.x + 0.5),
        fmt(fragment.y + 0.5),
        fmt(fragment.width - 1.0),
        fmt(fragment.height - 1.0),
        fmt(header_x),
        fmt(header_y),
        fmt(fragment.width - 2.0),
        fmt(25.0 * vertical_scale),
    );
    render_fragment_icon(
        out,
        fragment.kind,
        header_x + 4.0 * vertical_scale,
        header_y,
        theme,
    );
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" dominant-baseline="central" class="fragment-label">{}</text><text x="{}" y="{}" text-anchor="end" dominant-baseline="central" class="seq-number">{}</text>"#,
        fmt(header_x + 26.0 * vertical_scale),
        fmt(header_y + 12.0 * vertical_scale),
        kind_label,
        fmt(fragment.x - 3.0),
        fmt(header_y + 8.0 * vertical_scale),
        escape_xml(&fragment.number),
    );
    if !fragment.label.is_empty() {
        render_bracketed_label(
            out,
            header_x,
            header_y + 40.0 * vertical_scale,
            &fragment.label,
            fragment.label_width,
            "fragment-condition",
        );
    }
    for section in fragment.sections.iter().skip(1) {
        let separator_y = section.y + 0.5;
        let _ = write!(
            out,
            r#"<line class="fragment-separator" x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
            fmt(fragment.x + 1.0 + section.content_inset_left.unwrap_or_default()),
            fmt(separator_y),
            fmt(fragment.x + fragment.width - 1.0),
            fmt(separator_y),
        );
        if let Some(inner) = &section.inner_label {
            render_bracketed_label(
                out,
                fragment.x + 1.0,
                section.y + 16.0 * vertical_scale,
                inner,
                section.inner_label_width,
                "fragment-section-label",
            );
        } else if let (Some(keyword), Some(detail)) = (&section.keyword, &section.detail) {
            let keyword_width = section.keyword_width.unwrap_or(0.0);
            let background_width = keyword_width + section.detail_width.unwrap_or(0.0) + 16.0;
            let _ = write!(
                out,
                r##"<g opacity="0.65"><rect x="{}" y="{}" width="{}" height="{}" fill="{}"/><text x="{}" y="{}" class="fragment-section-label" fill="{}">{}</text><text x="{}" y="{}" class="fragment-section-label" fill="{}">{}</text></g>"##,
                fmt(fragment.x + 1.0),
                fmt(section.y + 1.0),
                fmt(background_width),
                fmt(20.0 * vertical_scale),
                escape_attr(theme.color("backgroundColor", "#fff")),
                fmt(fragment.x + 5.0),
                fmt(section.y + 16.0 * vertical_scale),
                escape_attr(theme.color("textColor", "#222")),
                escape_xml(keyword),
                fmt(fragment.x + 13.0 + keyword_width),
                fmt(section.y + 16.0 * vertical_scale),
                escape_attr(theme.color("textColor", "#222")),
                escape_xml(detail),
            );
        } else if !section.label.is_empty() {
            let _ = write!(
                out,
                r##"<g opacity="0.65"><rect x="{}" y="{}" width="{}" height="{}" fill="{}"/><text x="{}" y="{}" class="fragment-section-label">{}</text></g>"##,
                fmt(fragment.x + 1.0),
                fmt(section.y + 1.0),
                fmt(section.label_width.unwrap_or(0.0) + 8.0),
                fmt(20.0 * vertical_scale),
                escape_attr(theme.color("backgroundColor", "#fff")),
                fmt(fragment.x + 5.0),
                fmt(section.y + 16.0 * vertical_scale),
                escape_xml(&section.label),
            );
        }
    }
    out.push_str("</g>");
}

fn render_bracketed_label(
    out: &mut String,
    x: f64,
    y: f64,
    inner: &str,
    inner_width: Option<f64>,
    class: &str,
) {
    let inner_x = x + 7.89;
    let close_x = inner_x + inner_width.unwrap_or(0.0) + 4.0;
    let _ = write!(
        out,
        r#"<g><text x="{}" y="{}" class="{}">[</text><text x="{}" y="{}" class="{}" opacity="0.65">{}</text><text x="{}" y="{}" class="{}">]</text></g>"#,
        fmt(x),
        fmt(y),
        class,
        fmt(inner_x),
        fmt(y),
        class,
        escape_xml(&resolve_emoji_in_text(inner)),
        fmt(close_x),
        fmt(y),
        class,
    );
}

fn render_fragment_icon(
    out: &mut String,
    kind: ZenumlLayoutFragmentKind,
    x: f64,
    y: f64,
    theme: &ZenumlSourceStyle,
) {
    let (key, raw, view_box, attributes) = match kind {
        ZenumlLayoutFragmentKind::Alternative => (
            "alt",
            include_str!("../../../assets/zenuml/fragment-alt.svg"),
            "0 0 24 24",
            r#" fill="none""#,
        ),
        ZenumlLayoutFragmentKind::Optional => (
            "opt",
            include_str!("../../../assets/zenuml/fragment-opt.svg"),
            "0 0 24 24",
            r#" fill="none""#,
        ),
        ZenumlLayoutFragmentKind::Parallel => (
            "par",
            include_str!("../../../assets/zenuml/fragment-par.svg"),
            "0 0 24 24",
            r#" fill="none""#,
        ),
        ZenumlLayoutFragmentKind::Critical => (
            "critical",
            include_str!("../../../assets/zenuml/fragment-critical.svg"),
            "0 0 24 24",
            r#" fill="none""#,
        ),
        ZenumlLayoutFragmentKind::Loop => (
            "loop",
            include_str!("../../../assets/zenuml/fragment-loop.svg"),
            "0 0 1024 1024",
            r##" fill="#000" stroke="none""##,
        ),
        ZenumlLayoutFragmentKind::TryCatchFinally => (
            "tcf",
            include_str!("../../../assets/zenuml/fragment-tcf.svg"),
            "0 0 76 76",
            r##" fill="#000" stroke="none""##,
        ),
        ZenumlLayoutFragmentKind::Section => (
            "section",
            include_str!("../../../assets/zenuml/fragment-section.svg"),
            "0 0 15 15",
            r##" fill="#000" stroke="none""##,
        ),
        ZenumlLayoutFragmentKind::Reference => (
            "ref",
            include_str!("../../../assets/zenuml/fragment-ref.svg"),
            "0 0 24 24",
            r#" fill="none""#,
        ),
    };
    let content = svg_asset_content(raw);
    let attributes = if matches!(
        kind,
        ZenumlLayoutFragmentKind::Loop
            | ZenumlLayoutFragmentKind::TryCatchFinally
            | ZenumlLayoutFragmentKind::Section
    ) {
        format!(
            r#" fill="{}" stroke="none""#,
            escape_attr(theme.color("fragmentTextColor", "#000"))
        )
    } else {
        attributes.to_string()
    };
    let _ = write!(
        out,
        r#"<svg class="fragment-icon" data-icon="{}" x="{}" y="{}" width="{}" height="{}" viewBox="{}"{}>{}</svg>"#,
        key,
        fmt(x),
        fmt(y),
        fmt(20.0 * theme.typography.vertical_scale()),
        fmt(24.0 * theme.typography.vertical_scale()),
        view_box,
        attributes,
        content,
    );
}

fn svg_asset_content(svg: &'static str) -> &'static str {
    let Some(svg_start) = svg.find("<svg") else {
        return svg;
    };
    let Some(open_end) = svg[svg_start..].find('>') else {
        return svg;
    };
    let content_start = svg_start + open_end + 1;
    let Some(content_end) = svg.rfind("</svg>") else {
        return svg;
    };
    svg[content_start..content_end].trim()
}

fn render_comment(out: &mut String, comment: &ZenumlCommentLayout, theme: &ZenumlSourceStyle) {
    let lines = markdown_comment_lines(&comment.text);
    let _ = write!(
        out,
        r#"<text class="comment-text" data-statement="{}"{}>"#,
        escape_attr(&comment.statement_id),
        style_attr(&comment.style),
    );
    for (index, line) in lines.iter().enumerate() {
        if index == 0 {
            let _ = write!(
                out,
                r#"<tspan x="{}" y="{}">{}</tspan>"#,
                fmt(comment.x),
                fmt(comment.y),
                if line.is_empty() { " " } else { line },
            );
        } else {
            let _ = write!(
                out,
                r#"<tspan x="{}" dy="{}">{}</tspan>"#,
                fmt(comment.x),
                fmt(20.0 * theme.typography.vertical_scale()),
                if line.is_empty() { " " } else { line },
            );
        }
    }
    out.push_str("</text>");
}

fn markdown_comment_lines(markdown: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut lines = vec![String::new()];
    let mut style = MarkdownRunStyle::default();
    let parser = Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH);
    for event in parser {
        match event {
            Event::Start(Tag::Strong) => style.strong = true,
            Event::End(TagEnd::Strong) => style.strong = false,
            Event::Start(Tag::Emphasis) => style.emphasis = true,
            Event::End(TagEnd::Emphasis) => style.emphasis = false,
            Event::Start(Tag::CodeBlock(_)) => style.code_block = true,
            Event::End(TagEnd::CodeBlock) => style.code_block = false,
            Event::Text(text) => append_markdown_text(&mut lines, &text, style),
            Event::Code(text) => append_markdown_text(&mut lines, &text, style),
            Event::SoftBreak | Event::HardBreak => lines.push(String::new()),
            Event::End(TagEnd::Paragraph) if lines.last().is_some_and(|line| !line.is_empty()) => {
                lines.push(String::new());
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                append_markdown_text(&mut lines, &html, style);
            }
            _ => {}
        }
    }
    while lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

#[derive(Clone, Copy, Default)]
struct MarkdownRunStyle {
    strong: bool,
    emphasis: bool,
    code_block: bool,
}

fn append_markdown_text(lines: &mut Vec<String>, text: &str, style: MarkdownRunStyle) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        let escaped = escape_xml(line);
        if escaped.is_empty() {
            continue;
        }
        if !style.strong && !style.emphasis && !style.code_block {
            lines.last_mut().unwrap().push_str(&escaped);
            continue;
        }
        lines.last_mut().unwrap().push_str("<tspan");
        if style.strong {
            lines.last_mut().unwrap().push_str(r#" font-weight="bold""#);
        }
        if style.emphasis {
            lines
                .last_mut()
                .unwrap()
                .push_str(r#" font-style="italic""#);
        }
        if style.code_block {
            lines
                .last_mut()
                .unwrap()
                .push_str(r#" font-family="monospace""#);
        }
        lines.last_mut().unwrap().push('>');
        lines.last_mut().unwrap().push_str(&escaped);
        lines.last_mut().unwrap().push_str("</tspan>");
    }
}

fn style_attr(style: &std::collections::BTreeMap<String, String>) -> String {
    if style.is_empty() {
        return String::new();
    }
    let declarations = style
        .iter()
        .map(|(name, value)| format!("{name}:{}", escape_attr(value)))
        .collect::<Vec<_>>()
        .join(";");
    format!(r#" style="{declarations}""#)
}

fn resolve_emoji_in_text(text: &str) -> String {
    resolve_zenuml_emojis_in_text(text)
}

fn zenuml_css(
    diagram_id: impl std::fmt::Display,
    theme: &ZenumlSourceStyle,
    title_fill: &str,
) -> String {
    let mut css = format!(
        r#"
#{diagram_id} .frame-border-outer{{fill:{outer_paint}}}#{diagram_id} .frame-border-inner,#{diagram_id} .frame-header-bg{{fill:{background}}}#{diagram_id} .frame-header-line{{stroke:{border};stroke-width:1;shape-rendering:crispEdges}}#{diagram_id} .frame-title{{font-family:{font_family};font-size:{size_16}px;font-weight:600;fill:{title_fill}}}#{diagram_id} .participant-box{{fill:{actor_background};stroke:{actor_border};stroke-width:2}}#{diagram_id} .participant-label{{font-family:{font_family};font-size:{size_16}px;fill:{actor_text}}}#{diagram_id} .participant-icon{{color:{actor_text}}}#{diagram_id} .participant-icon [fill="currentColor"]:not([stroke]){{stroke:{actor_border};stroke-width:1}}#{diagram_id} .participant-emoji{{font-size:{size_16}px}}#{diagram_id} .stereotype-label{{font-family:{font_family};font-size:{size_16}px;fill:{actor_text}}}#{diagram_id} .lifeline{{stroke:{line};stroke-width:1}}#{diagram_id} .message-line{{stroke:{message_line};stroke-width:2;shape-rendering:crispEdges}}#{diagram_id} .message-label{{font-family:{font_family};font-size:{size_14}px;fill:{text}}}#{diagram_id} .arrow-head{{fill:{message_line};stroke:{message_line};stroke-width:2}}#{diagram_id} .arrow-open{{fill:none}}#{diagram_id} .occurrence{{fill:{activation_background};stroke:{activation_border};stroke-width:2;shape-rendering:crispEdges}}#{diagram_id} .fragment-border{{fill:none;stroke:{fragment_border};stroke-width:1;shape-rendering:crispEdges}}#{diagram_id} .fragment-header{{fill:{muted_surface};fill-opacity:.498;stroke:none;shape-rendering:crispEdges}}#{diagram_id} .fragment-label{{font-family:{font_family};font-size:{size_14}px;font-weight:600;fill:{fragment_header_text}}}#{diagram_id} .fragment-condition{{font-family:{font_family};font-size:{size_14}px;fill:{fragment_text}}}#{diagram_id} .fragment-separator{{stroke:{separator};stroke-width:1;shape-rendering:crispEdges}}#{diagram_id} .fragment-section-label{{font-family:{font_family};font-size:{size_14}px;fill:{fragment_text}}}#{diagram_id} .return-line{{stroke:{message_line};stroke-width:2;stroke-dasharray:6,4;shape-rendering:crispEdges}}#{diagram_id} .return-arrow{{stroke:{message_line};stroke-width:2;fill:none}}#{diagram_id} .return-label{{font-family:{font_family};font-size:{size_14}px;fill:{text}}}#{diagram_id} .return-icon{{fill:{text}}}#{diagram_id} .divider-line{{stroke:{note_border};stroke-width:1}}#{diagram_id} .divider-bg{{fill:{note_background};stroke:{note_border};stroke-width:1}}#{diagram_id} .divider-label{{font-family:{font_family};font-size:{size_14}px;fill:{note_text}}}#{diagram_id} .comment-text{{font-family:{font_family};font-size:{size_14}px;fill:{comment_text};opacity:.5}}#{diagram_id} .seq-number{{font-family:{font_family};font-size:{size_12}px;font-weight:100;fill:{subtle_text}}}#{diagram_id} .group-outline{{fill:none;stroke:{group_border}}}#{diagram_id} .group-title-bg{{fill:{group_background};stroke:none}}#{diagram_id} .group-title-text{{font-family:{font_family};font-size:{size_13}px;font-weight:400;fill:{group_text}}}
"#,
        font_family = theme.typography.css_family(),
        size_16 = fmt(theme.typography.size(16.0)),
        size_14 = fmt(theme.typography.size(14.0)),
        size_12 = fmt(theme.typography.size(12.0)),
        size_13 = fmt(theme.typography.size(13.0)),
        outer_paint = theme.color("borderColor", "#666"),
        border = theme.color("borderColor", "#666"),
        background = theme.color("backgroundColor", "#fff"),
        text = theme.color("messageTextColor", theme.color("textColor", "#222")),
        actor_background = theme.color("actorBackgroundColor", "#fff"),
        actor_border = theme.color("actorBorderColor", "#666"),
        actor_text = theme.color("actorTextColor", "#222"),
        line = theme.color("lifelineColor", theme.color("lineColor", "#666")),
        message_line = theme.color("messageLineColor", theme.color("lineColor", "#000")),
        activation_background = theme.color("activationBackgroundColor", "#dedede"),
        activation_border = theme.color("activationBorderColor", "#666"),
        muted_surface = theme.color("mutedSurfaceColor", "#dedede"),
        fragment_border = theme.color("fragmentBorderColor", theme.color("borderColor", "#666")),
        fragment_text = theme.color("fragmentTextColor", theme.color("textColor", "#000")),
        fragment_header_text = theme.color("fragmentTextColor", "#000"),
        group_text = theme.color("groupTextColor", "#222"),
        separator = theme.color("borderColor", "#e5e7eb"),
        note_border = theme.color("noteBorderColor", "#aa3"),
        note_background = theme.color("noteBackgroundColor", "#fff5ad"),
        note_text = theme.color("noteTextColor", "#333"),
        comment_text = theme.color("subtleTextColor", "#333"),
        subtle_text = theme.color(
            "sequenceNumberTextColor",
            theme.color("subtleTextColor", "#6b7280")
        ),
        group_border = theme.color("groupBorderColor", "#666"),
        group_background = theme.color("groupBackgroundColor", "#fff"),
    );
    if let Some(color) = theme
        .configured_color("fragmentTextColor")
        .or_else(|| theme.configured_color("textColor"))
    {
        let _ = write!(&mut css, "#{diagram_id} .fragment-icon{{color:{}}}", color);
    }
    css
}

const ZENUML_DEFAULT_TITLE_FILL: &str = "#222";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_comment_code_closes_monospace_runs_per_svg_line() {
        let lines = markdown_comment_lines("```text\nfirst\nsecond\n```");

        assert_eq!(
            lines,
            [
                r#"<tspan font-family="monospace">first</tspan>"#,
                r#"<tspan font-family="monospace">second</tspan>"#,
            ]
        );
    }

    #[test]
    fn vendored_icon_content_does_not_add_nested_svg_viewports() {
        for raw in [
            include_str!("../../../assets/zenuml/actor.svg"),
            include_str!("../../../assets/zenuml/ec2.svg"),
            include_str!("../../../assets/zenuml/fragment-loop.svg"),
        ] {
            let content = svg_asset_content(raw);
            assert!(!content.contains("<svg"));
            assert!(!content.contains("</svg>"));
            assert!(content.contains('<'));
        }
    }
}

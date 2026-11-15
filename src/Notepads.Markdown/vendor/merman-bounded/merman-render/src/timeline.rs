use crate::Result;
use crate::model::{
    Bounds, TimelineDiagramLayout, TimelineLineLayout, TimelineNodeLayout, TimelineSectionLayout,
    TimelineTaskLayout,
};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::timeline::{
    TimelineDiagramRenderModel, TimelineDirection, TimelineRenderTask,
};
use std::borrow::Cow;

mod config;

pub(crate) use config::TimelineConfigView;

const MAX_SECTIONS: i64 = 12;

const BASE_MARGIN: f64 = 50.0;
const NODE_PADDING: f64 = 20.0;
const TASK_STEP_X: f64 = 200.0;
const TASK_CONTENT_WIDTH_DEFAULT: f64 = 150.0;
const EVENT_VERTICAL_OFFSET_FROM_TASK_Y: f64 = 200.0;
const EVENT_GAP_Y: f64 = 10.0;

// Mermaid 11.16 uses a separate renderer for `timeline TD`. These values mirror that renderer's
// block geometry rather than transposing the horizontal layout.
const VERTICAL_NODE_CONTENT_WIDTH: f64 = 200.0;
const VERTICAL_EVENT_CONTENT_WIDTH: f64 = 300.0;
const VERTICAL_NODE_PADDING: f64 = 5.0;
const VERTICAL_EVENT_SPACING: f64 = 10.0;
const VERTICAL_SECTION_TASK_GAP: f64 = 20.0;
const VERTICAL_TASK_AXIS_GAP: f64 = 20.0;
const VERTICAL_TASK_GAP: f64 = 30.0;
const VERTICAL_EVENT_AXIS_GAP: f64 = 50.0;

const TITLE_Y: f64 = 20.0;

pub(crate) fn default_use_max_width() -> bool {
    false
}

fn section_index(full_section: i64) -> i64 {
    (full_section % MAX_SECTIONS) - 1
}

fn section_class(full_section: i64) -> String {
    format!("section-{}", section_index(full_section))
}

fn next_char_at(text: &str, idx: usize) -> Option<char> {
    text.get(idx..)?.chars().next()
}

fn wrap_tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some(ch) = next_char_at(text, i) else {
            break;
        };
        if ch.is_whitespace() {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            // Coalesce any whitespace run into a single token.
            while i < bytes.len() {
                let Some(c) = next_char_at(text, i) else {
                    break;
                };
                if !c.is_whitespace() {
                    break;
                }
                i += c.len_utf8();
            }
            out.push(" ".to_string());
            continue;
        }

        let Some(rest) = text.get(i..) else {
            break;
        };
        if rest.starts_with("<br>") || rest.starts_with("<br/>") || rest.starts_with("<br />") {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            if rest.starts_with("<br>") {
                i += "<br>".len();
            } else if rest.starts_with("<br/>") {
                i += "<br/>".len();
            } else {
                i += "<br />".len();
            }
            out.push("<br>".to_string());
            continue;
        }

        buf.push(ch);
        i += ch.len_utf8();
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn join_trim(tokens: &[String]) -> String {
    tokens.join(" ").trim().to_string()
}

fn svg_collapse_whitespace_for_measure(s: &str) -> Cow<'_, str> {
    // Mermaid timeline wrap decisions use `tspan.getComputedTextLength()`, which measures the
    // rendered text. SVG text collapses whitespace runs unless `xml:space="preserve"` is set.
    // Mermaid does not set `xml:space`, so we mirror that collapsing here.
    let mut out: Option<String> = None;
    let mut last_space = false;
    let mut saw_non_space = false;

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !saw_non_space || last_space {
                continue;
            }
            out.get_or_insert_with(|| String::with_capacity(s.len()))
                .push(' ');
            last_space = true;
        } else {
            saw_non_space = true;
            out.get_or_insert_with(|| String::with_capacity(s.len()))
                .push(ch);
            last_space = false;
        }
    }

    let Some(mut out) = out else {
        return Cow::Borrowed(s.trim());
    };
    if out.ends_with(' ') {
        out.pop();
    }
    Cow::Owned(out)
}

fn wrap_lines(
    text: &str,
    max_width: f64,
    style: &TextStyle,
    measurer: &dyn TextMeasurer,
) -> Vec<String> {
    let tokens = wrap_tokens(text);
    if tokens.is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for tok in tokens {
        cur.push(tok.clone());
        let candidate = join_trim(&cur);
        let candidate = svg_collapse_whitespace_for_measure(&candidate);
        // Mermaid Timeline's family-local `wrap()` helper probes
        // `<tspan>.getComputedTextLength()`. Keep this distinct from `getBBox().width`: glyph
        // overhang can otherwise move a word to an extra line at a wrapping boundary.
        let candidate_width = measurer.measure_svg_text_computed_length_px(&candidate, style);
        if tok == "<br>" {
            cur.pop();
            lines.push(join_trim(&cur));
            cur = vec![String::new()];
        } else if candidate_width > max_width {
            cur.pop();
            let previous = join_trim(&cur);
            if !previous.is_empty() {
                lines.push(previous);
            }
            // An unbreakable token wider than the whole line must occupy that line directly.
            // Emitting the empty candidate first would create a phantom leading row for CJK and
            // other scripts that do not contain ASCII whitespace.
            cur = vec![tok];
        }
    }

    lines.push(join_trim(&cur));
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn text_bbox_height(lines: &[String], style: &TextStyle, measurer: &dyn TextMeasurer) -> f64 {
    // Mermaid timeline measures SVG `<text>.getBBox().height` (see upstream `svgDraw.js`).
    //
    // The first visible line is measured through the operation-selected DOM-shape route. Mermaid
    // places subsequent tspans at `dy=1.1em`, so their contribution is source-derived.
    let font_size = style.font_size.max(1.0);
    let mut visible_lines = lines.iter().filter(|line| !line.trim().is_empty());
    let Some(first_line) = visible_lines.next() else {
        return 0.0;
    };
    let additional_lines = visible_lines.count();
    let first = measurer.measure_svg_tspan_text_bbox_height_px(first_line, style);
    let additional = additional_lines as f64 * font_size * 1.1;
    first + additional
}

fn virtual_node_height(
    text: &str,
    content_width: f64,
    style: &TextStyle,
    layout_font_size: f64,
    padding: f64,
    measurer: &dyn TextMeasurer,
) -> (f64, Vec<String>) {
    // Mermaid timeline `wrap()` compares `tspan.getComputedTextLength()` against `node.width`
    // (the configured inner width, excluding padding).
    let lines = wrap_lines(text, content_width.max(1.0), style, measurer);
    let bbox_h = text_bbox_height(&lines, style, measurer);
    // Mermaid timeline uses `conf.fontSize` (top-level `config.fontSize`) for the extra vertical
    // offset, even when the actual rendered font size comes from `themeVariables.fontSize`.
    let h = bbox_h + layout_font_size.max(1.0) * 1.1 * 0.5 + padding;
    (h, lines)
}

#[derive(Debug, Clone, Copy)]
struct TimelineNodeRequest<'a> {
    kind: &'a str,
    label: &'a str,
    full_section: i64,
    x: f64,
    y: f64,
    content_width: f64,
    padding: f64,
    max_height: f64,
    style: &'a TextStyle,
    layout_font_size: f64,
}

fn compute_node(
    request: TimelineNodeRequest<'_>,
    measurer: &dyn TextMeasurer,
) -> TimelineNodeLayout {
    let TimelineNodeRequest {
        kind,
        label,
        full_section,
        x,
        y,
        content_width,
        padding,
        max_height,
        style,
        layout_font_size,
    } = request;
    let (h0, label_lines) = virtual_node_height(
        label,
        content_width,
        style,
        layout_font_size,
        padding,
        measurer,
    );
    let height = h0.max(max_height).max(1.0);
    let width = (content_width + padding * 2.0).max(1.0);
    TimelineNodeLayout {
        x,
        y,
        width,
        height,
        content_width: content_width.max(1.0),
        padding,
        section_class: section_class(full_section),
        label: label.to_string(),
        label_lines,
        kind: kind.to_string(),
    }
}

fn bounds_from_nodes_and_lines<'a, 'b>(
    nodes: impl IntoIterator<Item = &'a TimelineNodeLayout>,
    lines: impl IntoIterator<Item = &'b TimelineLineLayout>,
) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut any = false;
    for n in nodes {
        any = true;
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.width);
        max_y = max_y.max(n.y + n.height);
    }
    for l in lines {
        any = true;
        min_x = min_x.min(l.x1.min(l.x2));
        min_y = min_y.min(l.y1.min(l.y2));
        max_x = max_x.max(l.x1.max(l.x2));
        max_y = max_y.max(l.y1.max(l.y2));
    }

    if any {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

fn expand_bounds_for_node_text(
    min_x: &mut f64,
    _min_y: &mut f64,
    max_x: &mut f64,
    _max_y: &mut f64,
    nodes: &[TimelineNodeLayout],
    style: &TextStyle,
    measurer: &dyn TextMeasurer,
) {
    for n in nodes {
        if n.kind == "title-bounds" {
            continue;
        }

        let anchor_x = n.x + n.width / 2.0;
        for line in &n.label_lines {
            if line.trim().is_empty() {
                continue;
            }
            // `wrap()` replaces the direct text run with child `<tspan>` rows before Mermaid
            // reads the root SVG bbox. Preserve that DOM shape in the measurement route. The
            // rendered text is middle-anchored at the node center, so each row contributes half
            // of its tspan bbox width on either side of that anchor.
            let half_width = measurer
                .measure_svg_tspan_text_bbox_width_px(line, style)
                .max(0.0)
                / 2.0;
            *min_x = (*min_x).min(anchor_x - half_width);
            *max_x = (*max_x).max(anchor_x + half_width);
        }
    }
}

pub(crate) fn layout_timeline_diagram_typed(
    model: &TimelineDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<TimelineDiagramLayout> {
    match model.direction {
        TimelineDirection::LeftToRight => {
            layout_timeline_horizontal(model, effective_config, measurer)
        }
        TimelineDirection::TopDown => layout_timeline_vertical(model, effective_config, measurer),
    }
}

fn layout_timeline_horizontal(
    model: &TimelineDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<TimelineDiagramLayout> {
    let _ = (model.acc_title.as_deref(), model.acc_descr.as_deref());

    let cfg = TimelineConfigView::new(effective_config).layout_settings();
    let text_style = cfg.text_style;
    let render_font_size = text_style.font_size;
    let layout_font_size = cfg.layout_font_size;

    let left_margin = cfg.left_margin;
    let disable_multicolor = cfg.disable_multicolor;
    // Mermaid's Timeline renderer hardcodes the text-wrap width to `150` (see upstream
    // `drawTasks`/`drawEvents`: node objects use `width: 150` and `wrap(..., node.width)`),
    // even though the config schema exposes a `timeline.width` field.
    //
    // For upstream parity, treat `timeline.width` as a no-op and keep the wrap width constant.
    let task_content_width = TASK_CONTENT_WIDTH_DEFAULT;

    let mut max_section_height: f64 = 0.0;
    for section in &model.sections {
        let (h, _lines) = virtual_node_height(
            section,
            task_content_width,
            &text_style,
            layout_font_size,
            NODE_PADDING,
            measurer,
        );
        max_section_height = max_section_height.max(h + 20.0);
    }

    let mut max_task_height: f64 = 0.0;
    let mut max_event_line_length: f64 = 0.0;
    for task in &model.tasks {
        // Upstream Mermaid's Timeline renderer computes `maxTaskHeight` by passing the entire
        // task object into `getVirtualNodeHeight(...)`, which stringifies to `"[object Object]"`.
        // This inflates `maxTaskHeight` when all task labels are short; replicate for parity.
        let virtual_task_label = "[object Object]";
        let (h, _lines) = virtual_node_height(
            virtual_task_label,
            task_content_width,
            &text_style,
            layout_font_size,
            NODE_PADDING,
            measurer,
        );
        max_task_height = max_task_height.max(h + 20.0);

        let mut task_event_len: f64 = 0.0;
        for ev in &task.events {
            let (eh, _lines) = virtual_node_height(
                ev,
                task_content_width,
                &text_style,
                layout_font_size,
                NODE_PADDING,
                measurer,
            );
            task_event_len += eh;
        }
        if !task.events.is_empty() {
            task_event_len += (task.events.len().saturating_sub(1) as f64) * EVENT_GAP_Y;
        }
        max_event_line_length = max_event_line_length.max(task_event_len);
    }

    let base_x = BASE_MARGIN + left_margin;
    let base_y = BASE_MARGIN;

    let mut sections: Vec<TimelineSectionLayout> = Vec::new();
    let mut orphan_tasks: Vec<TimelineTaskLayout> = Vec::new();

    let mut all_nodes_pre_title: Vec<TimelineNodeLayout> = Vec::new();
    let mut all_lines_pre_title: Vec<TimelineLineLayout> = Vec::new();

    let has_sections = !model.sections.is_empty();

    if has_sections {
        let mut master_x = base_x;
        let section_y = base_y;

        for (section_number, section_label) in model.sections.iter().enumerate() {
            let section_number = section_number as i64;
            let tasks_for_section: Vec<&TimelineRenderTask> = model
                .tasks
                .iter()
                .filter(|t| t.section == *section_label)
                .collect();
            let tasks_for_section_count = tasks_for_section.len().max(1);

            let content_width = TASK_STEP_X * (tasks_for_section_count as f64) - 50.0;
            let section_node = compute_node(
                TimelineNodeRequest {
                    kind: "section",
                    label: section_label,
                    full_section: section_number,
                    x: master_x,
                    y: section_y,
                    content_width,
                    padding: NODE_PADDING,
                    max_height: max_section_height,
                    style: &text_style,
                    layout_font_size,
                },
                measurer,
            );
            all_nodes_pre_title.push(section_node.clone());

            let mut tasks: Vec<TimelineTaskLayout> = Vec::new();
            let mut task_x = master_x;
            let task_y = section_y + max_section_height + 50.0;

            for task in &tasks_for_section {
                let full_section = section_number;
                let task_node = compute_node(
                    TimelineNodeRequest {
                        kind: "task",
                        label: &task.task,
                        full_section,
                        x: task_x,
                        y: task_y,
                        content_width: task_content_width,
                        padding: NODE_PADDING,
                        max_height: max_task_height,
                        style: &text_style,
                        layout_font_size,
                    },
                    measurer,
                );
                all_nodes_pre_title.push(task_node.clone());

                let connector = TimelineLineLayout {
                    kind: "task-events".to_string(),
                    x1: task_x + (task_node.width / 2.0),
                    y1: task_y + max_task_height,
                    x2: task_x + (task_node.width / 2.0),
                    y2: task_y + max_task_height + 100.0 + max_event_line_length + 100.0,
                };
                all_lines_pre_title.push(connector.clone());

                let mut events: Vec<TimelineNodeLayout> = Vec::new();
                let mut event_y = task_y + EVENT_VERTICAL_OFFSET_FROM_TASK_Y;
                for ev in &task.events {
                    let event_node = compute_node(
                        TimelineNodeRequest {
                            kind: "event",
                            label: ev,
                            full_section,
                            x: task_x,
                            y: event_y,
                            content_width: task_content_width,
                            padding: NODE_PADDING,
                            max_height: 50.0,
                            style: &text_style,
                            layout_font_size,
                        },
                        measurer,
                    );
                    event_y += event_node.height + EVENT_GAP_Y;
                    all_nodes_pre_title.push(event_node.clone());
                    events.push(event_node);
                }

                tasks.push(TimelineTaskLayout {
                    node: task_node,
                    connectors: vec![connector],
                    events,
                });

                task_x += TASK_STEP_X;
            }

            sections.push(TimelineSectionLayout {
                node: section_node,
                tasks,
            });

            master_x += TASK_STEP_X * (tasks_for_section_count as f64);
        }
    } else {
        let mut master_x = base_x;
        let master_y = base_y;
        let mut section_color: i64 = 0;

        for task in &model.tasks {
            let task_node = compute_node(
                TimelineNodeRequest {
                    kind: "task",
                    label: &task.task,
                    full_section: section_color,
                    x: master_x,
                    y: master_y,
                    content_width: task_content_width,
                    padding: NODE_PADDING,
                    max_height: max_task_height,
                    style: &text_style,
                    layout_font_size,
                },
                measurer,
            );
            all_nodes_pre_title.push(task_node.clone());

            let connector = TimelineLineLayout {
                kind: "task-events".to_string(),
                x1: master_x + (task_node.width / 2.0),
                y1: master_y + max_task_height,
                x2: master_x + (task_node.width / 2.0),
                y2: master_y + max_task_height + 100.0 + max_event_line_length + 100.0,
            };
            all_lines_pre_title.push(connector.clone());

            let mut events: Vec<TimelineNodeLayout> = Vec::new();
            let mut event_y = master_y + EVENT_VERTICAL_OFFSET_FROM_TASK_Y;
            for ev in &task.events {
                let event_node = compute_node(
                    TimelineNodeRequest {
                        kind: "event",
                        label: ev,
                        full_section: section_color,
                        x: master_x,
                        y: event_y,
                        content_width: task_content_width,
                        padding: NODE_PADDING,
                        max_height: 50.0,
                        style: &text_style,
                        layout_font_size,
                    },
                    measurer,
                );
                event_y += event_node.height + EVENT_GAP_Y;
                all_nodes_pre_title.push(event_node.clone());
                events.push(event_node);
            }

            orphan_tasks.push(TimelineTaskLayout {
                node: task_node,
                connectors: vec![connector],
                events,
            });

            master_x += TASK_STEP_X;
            if !disable_multicolor {
                section_color += 1;
            }
        }
    }

    let pre_title_bounds = bounds_from_nodes_and_lines(&all_nodes_pre_title, &all_lines_pre_title);
    let has_pre_title_content = pre_title_bounds.is_some();
    let (mut pre_min_x, mut pre_min_y, mut pre_max_x, mut pre_max_y) =
        pre_title_bounds.unwrap_or((0.0, 0.0, 0.0, 0.0));
    if has_pre_title_content {
        expand_bounds_for_node_text(
            &mut pre_min_x,
            &mut pre_min_y,
            &mut pre_max_x,
            &mut pre_max_y,
            &all_nodes_pre_title,
            &text_style,
            measurer,
        );
    }
    let pre_title_box_width = if has_pre_title_content {
        (pre_max_x - pre_min_x).max(1.0)
    } else {
        0.0
    };

    let title = model
        .title
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let title_x = pre_title_box_width / 2.0 - left_margin;

    let depth_y = if has_sections {
        max_section_height + max_task_height + 150.0
    } else {
        max_task_height + 100.0
    };

    let activity_line = TimelineLineLayout {
        kind: "activity".to_string(),
        x1: left_margin,
        y1: depth_y,
        x2: pre_title_box_width + 3.0 * left_margin,
        y2: depth_y,
    };

    let mut all_nodes_full: Vec<TimelineNodeLayout> = all_nodes_pre_title.clone();
    let mut all_lines_full: Vec<TimelineLineLayout> = all_lines_pre_title.clone();
    all_lines_full.push(activity_line.clone());

    if let Some(t) = title.as_deref() {
        // Approximate the title bounds so the viewBox can include it (Mermaid uses a bold 4ex).
        //
        // Note: `ex` depends on the font x-height; for Mermaid's default theme at 16px, `4ex`
        // resolves to ~31px in upstream fixtures.
        let title_font_size = render_font_size * 1.9375;
        let title_style = TextStyle {
            font_family: text_style.font_family.clone(),
            font_size: title_font_size,
            font_weight: Some("bold".to_string()),
            font_style: None,
        };
        let metrics = measurer.measure(t, &title_style);
        all_nodes_full.push(TimelineNodeLayout {
            x: title_x,
            y: TITLE_Y - title_style.font_size,
            width: metrics.width.max(1.0),
            height: title_style.font_size.max(1.0),
            content_width: metrics.width.max(1.0),
            padding: 0.0,
            section_class: "section-root".to_string(),
            label: t.to_string(),
            label_lines: vec![t.to_string()],
            kind: "title-bounds".to_string(),
        });
    }

    let (mut full_min_x, mut full_min_y, mut full_max_x, mut full_max_y) =
        bounds_from_nodes_and_lines(&all_nodes_full, &all_lines_full)
            .unwrap_or((pre_min_x, pre_min_y, pre_max_x, pre_max_y));
    expand_bounds_for_node_text(
        &mut full_min_x,
        &mut full_min_y,
        &mut full_max_x,
        &mut full_max_y,
        &all_nodes_full,
        &text_style,
        measurer,
    );

    let viewbox_padding = cfg.viewbox_padding;
    let vb_min_x = full_min_x - viewbox_padding;
    let vb_min_y = full_min_y - viewbox_padding;
    let vb_max_x = full_max_x + viewbox_padding;
    let vb_max_y = full_max_y + viewbox_padding;

    Ok(TimelineDiagramLayout {
        direction: model.direction,
        bounds: Some(Bounds {
            min_x: vb_min_x,
            min_y: vb_min_y,
            max_x: vb_max_x,
            max_y: vb_max_y,
        }),
        left_margin,
        base_x,
        base_y,
        pre_title_box_width,
        sections,
        orphan_tasks,
        activity_line,
        title,
        title_x,
        title_y: TITLE_Y,
        use_max_width: cfg.use_max_width,
    })
}

#[allow(clippy::too_many_arguments)]
fn layout_vertical_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a TimelineRenderTask>,
    timeline_x: f64,
    start_y: f64,
    task_spacing: f64,
    max_task_height: f64,
    mut section_color: i64,
    advance_section_color: bool,
    text_style: &TextStyle,
    layout_font_size: f64,
    measurer: &dyn TextMeasurer,
    all_nodes: &mut Vec<TimelineNodeLayout>,
    all_lines: &mut Vec<TimelineLineLayout>,
) -> Vec<TimelineTaskLayout> {
    let mut layouts = Vec::new();
    let mut task_y = start_y;
    let task_width = VERTICAL_NODE_CONTENT_WIDTH + VERTICAL_NODE_PADDING * 2.0;
    let task_x = timeline_x - VERTICAL_TASK_AXIS_GAP - task_width;
    let events_x = timeline_x + VERTICAL_EVENT_AXIS_GAP;

    for task in tasks {
        let task_node = compute_node(
            TimelineNodeRequest {
                kind: "task",
                label: &task.task,
                full_section: section_color,
                x: task_x,
                y: task_y,
                content_width: VERTICAL_NODE_CONTENT_WIDTH,
                padding: VERTICAL_NODE_PADDING,
                max_height: max_task_height,
                style: text_style,
                layout_font_size,
            },
            measurer,
        );
        all_nodes.push(task_node.clone());

        let mut events = Vec::new();
        let mut connectors = Vec::new();
        let mut event_y = task_y;
        for event in &task.events {
            let event_node = compute_node(
                TimelineNodeRequest {
                    kind: "event",
                    label: event,
                    full_section: section_color,
                    x: events_x,
                    y: event_y,
                    content_width: VERTICAL_EVENT_CONTENT_WIDTH,
                    padding: VERTICAL_NODE_PADDING,
                    max_height: 0.0,
                    style: text_style,
                    layout_font_size,
                },
                measurer,
            );
            let line_y = event_y + event_node.height / 2.0;
            let connector = TimelineLineLayout {
                kind: "task-events".to_string(),
                x1: timeline_x,
                y1: line_y,
                x2: events_x,
                y2: line_y,
            };
            all_lines.push(connector.clone());
            connectors.push(connector);
            event_y += event_node.height + VERTICAL_EVENT_SPACING;
            all_nodes.push(event_node.clone());
            events.push(event_node);
        }

        layouts.push(TimelineTaskLayout {
            node: task_node,
            connectors,
            events,
        });
        task_y += task_spacing;
        if advance_section_color {
            section_color += 1;
        }
    }

    layouts
}

fn layout_timeline_vertical(
    model: &TimelineDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<TimelineDiagramLayout> {
    let _ = (model.acc_title.as_deref(), model.acc_descr.as_deref());

    let cfg = TimelineConfigView::new(effective_config).layout_settings();
    let text_style = cfg.text_style;
    let render_font_size = text_style.font_size;
    let layout_font_size = cfg.layout_font_size;
    let left_margin = cfg.left_margin;

    let node_total_width = VERTICAL_NODE_CONTENT_WIDTH + VERTICAL_NODE_PADDING * 2.0;
    let event_total_width = VERTICAL_EVENT_CONTENT_WIDTH + VERTICAL_NODE_PADDING * 2.0;
    let left_width = node_total_width + VERTICAL_TASK_AXIS_GAP;
    let right_width = event_total_width + VERTICAL_EVENT_AXIS_GAP;
    let section_content_width = (left_width + right_width - VERTICAL_NODE_PADDING * 2.0).max(50.0);

    let mut max_section_height: f64 = 0.0;
    for section in &model.sections {
        let (height, _) = virtual_node_height(
            section,
            section_content_width,
            &text_style,
            layout_font_size,
            VERTICAL_NODE_PADDING,
            measurer,
        );
        max_section_height = max_section_height.max(height);
    }

    let mut max_task_height: f64 = 0.0;
    let mut max_event_stack_height: f64 = 0.0;
    for task in &model.tasks {
        let (height, _) = virtual_node_height(
            "[object Object]",
            VERTICAL_NODE_CONTENT_WIDTH,
            &text_style,
            layout_font_size,
            VERTICAL_NODE_PADDING,
            measurer,
        );
        max_task_height = max_task_height.max(height);

        let mut event_stack_height = 0.0;
        for event in &task.events {
            let (height, _) = virtual_node_height(
                event,
                VERTICAL_EVENT_CONTENT_WIDTH,
                &text_style,
                layout_font_size,
                VERTICAL_NODE_PADDING,
                measurer,
            );
            event_stack_height += height;
        }
        if !task.events.is_empty() {
            event_stack_height +=
                task.events.len().saturating_sub(1) as f64 * VERTICAL_EVENT_SPACING;
        }
        max_event_stack_height = max_event_stack_height.max(event_stack_height);
    }

    let task_spacing = max_task_height.max(max_event_stack_height) + VERTICAL_TASK_GAP;
    let base_x = BASE_MARGIN + left_margin;
    let base_y = BASE_MARGIN;
    let has_sections = !model.sections.is_empty();
    let timeline_x = base_x + left_width;

    let mut sections = Vec::new();
    let mut orphan_tasks = Vec::new();
    let mut all_nodes_pre_title = Vec::new();
    let mut all_lines_pre_title = Vec::new();

    if has_sections {
        let mut master_y = base_y;
        for (section_number, section_label) in model.sections.iter().enumerate() {
            let section_number = section_number as i64;
            let tasks_for_section = model
                .tasks
                .iter()
                .filter(|task| task.section == *section_label)
                .collect::<Vec<_>>();
            let section_node = compute_node(
                TimelineNodeRequest {
                    kind: "section",
                    label: section_label,
                    full_section: section_number,
                    x: timeline_x - left_width,
                    y: master_y,
                    content_width: section_content_width,
                    padding: VERTICAL_NODE_PADDING,
                    max_height: max_section_height,
                    style: &text_style,
                    layout_font_size,
                },
                measurer,
            );
            all_nodes_pre_title.push(section_node.clone());
            let task_start_y = master_y + section_node.height + VERTICAL_SECTION_TASK_GAP;
            let tasks = layout_vertical_tasks(
                tasks_for_section.iter().copied(),
                timeline_x,
                task_start_y,
                task_spacing,
                max_task_height,
                section_number,
                false,
                &text_style,
                layout_font_size,
                measurer,
                &mut all_nodes_pre_title,
                &mut all_lines_pre_title,
            );
            let task_count = tasks_for_section.len();
            let section_height = section_node.height
                + VERTICAL_SECTION_TASK_GAP
                + task_spacing * task_count.max(1) as f64
                - if task_count > 0 {
                    VERTICAL_TASK_GAP * 2.0
                } else {
                    0.0
                };
            master_y += section_height;
            sections.push(TimelineSectionLayout {
                node: section_node,
                tasks,
            });
        }
    } else {
        orphan_tasks = layout_vertical_tasks(
            model.tasks.iter(),
            timeline_x,
            base_y,
            task_spacing,
            max_task_height,
            0,
            !cfg.disable_multicolor,
            &text_style,
            layout_font_size,
            measurer,
            &mut all_nodes_pre_title,
            &mut all_lines_pre_title,
        );
    }

    let pre_title_bounds = bounds_from_nodes_and_lines(&all_nodes_pre_title, &all_lines_pre_title);
    let has_pre_title_content = pre_title_bounds.is_some();
    let (mut pre_min_x, mut pre_min_y, mut pre_max_x, mut pre_max_y) =
        pre_title_bounds.unwrap_or((0.0, 0.0, 0.0, 0.0));
    if has_pre_title_content {
        expand_bounds_for_node_text(
            &mut pre_min_x,
            &mut pre_min_y,
            &mut pre_max_x,
            &mut pre_max_y,
            &all_nodes_pre_title,
            &text_style,
            measurer,
        );
    }
    let pre_title_box_width = if has_pre_title_content {
        (pre_max_x - pre_min_x).max(1.0)
    } else {
        0.0
    };
    let title = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    let title_x = pre_title_box_width / 2.0 - left_margin;

    let mut all_nodes_full = all_nodes_pre_title.clone();
    if let Some(title) = title.as_deref() {
        let title_font_size = render_font_size * 1.9375;
        let title_style = TextStyle {
            font_family: text_style.font_family.clone(),
            font_size: title_font_size,
            font_weight: Some("bold".to_string()),
            font_style: None,
        };
        let metrics = measurer.measure(title, &title_style);
        all_nodes_full.push(TimelineNodeLayout {
            x: title_x,
            y: TITLE_Y - title_font_size,
            width: metrics.width.max(1.0),
            height: title_font_size.max(1.0),
            content_width: metrics.width.max(1.0),
            padding: 0.0,
            section_class: "section-root".to_string(),
            label: title.to_string(),
            label_lines: vec![title.to_string()],
            kind: "title-bounds".to_string(),
        });
    }

    let (_, _, _, content_max_y) =
        bounds_from_nodes_and_lines(&all_nodes_full, &all_lines_pre_title)
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let activity_line = TimelineLineLayout {
        kind: "activity".to_string(),
        x1: timeline_x,
        y1: base_y - render_font_size * 2.0,
        x2: timeline_x,
        y2: content_max_y + render_font_size * 0.5 + 20.0,
    };
    let mut all_lines_full = all_lines_pre_title;
    all_lines_full.push(activity_line.clone());

    let (mut full_min_x, mut full_min_y, mut full_max_x, mut full_max_y) =
        bounds_from_nodes_and_lines(&all_nodes_full, &all_lines_full)
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
    expand_bounds_for_node_text(
        &mut full_min_x,
        &mut full_min_y,
        &mut full_max_x,
        &mut full_max_y,
        &all_nodes_full,
        &text_style,
        measurer,
    );
    let padding = cfg.viewbox_padding;

    Ok(TimelineDiagramLayout {
        direction: model.direction,
        bounds: Some(Bounds {
            min_x: full_min_x - padding,
            min_y: full_min_y - padding,
            max_x: full_max_x + padding,
            max_y: full_max_y + padding,
        }),
        left_margin,
        base_x,
        base_y,
        pre_title_box_width,
        sections,
        orphan_tasks,
        activity_line,
        title,
        title_x,
        title_y: TITLE_Y,
        use_max_width: cfg.use_max_width,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{RenderEnvironment, TextMeasurementPhase};
    use crate::text::DeterministicTextMeasurer;
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
    }

    fn layout_timeline(source: &str) -> TimelineDiagramLayout {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::default())
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Timeline(model) = parsed.model() else {
            panic!("expected timeline render model");
        };
        let session = RenderEnvironment::deterministic().begin_session().unwrap();
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        layout_timeline_diagram_typed(
            model,
            parsed.metadata().effective_config.as_value(),
            &measurer,
        )
        .expect("layout ok")
    }

    #[test]
    fn timeline_bbox_height_uses_tspan_measurement_for_first_line() {
        struct TspanHeightMeasurer;

        impl TextMeasurer for TspanHeightMeasurer {
            fn measure(&self, _text: &str, _style: &TextStyle) -> crate::text::TextMetrics {
                crate::text::TextMetrics {
                    width: 1.0,
                    height: 1.0,
                    line_count: 1,
                }
            }

            fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, _style: &TextStyle) -> f64 {
                assert_eq!(text, "first");
                25.0
            }
        }

        let style = TextStyle {
            font_size: 17.0,
            ..TextStyle::default()
        };
        let lines = vec!["first".to_string(), "second".to_string()];

        let height = text_bbox_height(&lines, &style, &TspanHeightMeasurer);

        assert_eq!(height, 25.0 + 17.0 * 1.1);
    }

    #[test]
    fn oversized_unbreakable_token_does_not_create_a_phantom_leading_row() {
        let text = "日本語のテキストも対応";
        let lines = wrap_lines(
            text,
            150.0,
            &TextStyle {
                font_size: 16.0,
                ..TextStyle::default()
            },
            &DeterministicTextMeasurer::default(),
        );

        assert_eq!(lines, [text]);
    }

    #[test]
    fn node_text_bounds_measure_the_rendered_tspan_dom_shape() {
        struct TspanOnlyMeasurer;

        impl TextMeasurer for TspanOnlyMeasurer {
            fn measure(&self, _text: &str, _style: &TextStyle) -> crate::text::TextMetrics {
                crate::text::TextMetrics {
                    width: 1.0,
                    height: 1.0,
                    line_count: 1,
                }
            }

            fn measure_svg_text_bbox_x_with_ascii_overhang(
                &self,
                _text: &str,
                _style: &TextStyle,
            ) -> (f64, f64) {
                panic!("timeline labels are rendered as child tspans")
            }

            fn measure_svg_tspan_text_bbox_width_px(&self, text: &str, _style: &TextStyle) -> f64 {
                assert_eq!(text, "an overflowing label");
                320.0
            }
        }

        let node = TimelineNodeLayout {
            x: 200.0,
            y: 50.0,
            width: 190.0,
            height: 50.0,
            content_width: 150.0,
            padding: 20.0,
            section_class: "section-0".to_string(),
            label: "an overflowing label".to_string(),
            label_lines: vec!["an overflowing label".to_string()],
            kind: "event".to_string(),
        };
        let mut min_x = node.x;
        let mut min_y = node.y;
        let mut max_x = node.x + node.width;
        let mut max_y = node.y + node.height;

        expand_bounds_for_node_text(
            &mut min_x,
            &mut min_y,
            &mut max_x,
            &mut max_y,
            &[node],
            &TextStyle::default(),
            &TspanOnlyMeasurer,
        );

        assert_eq!(min_x, 135.0);
        assert_eq!(max_x, 455.0);
    }

    #[test]
    fn long_word_wrap_keeps_the_activity_line_aligned_with_content_bounds() {
        let path = workspace_root()
            .join("fixtures")
            .join("timeline")
            .join("upstream_long_word_wrap.mmd");
        let text = std::fs::read_to_string(&path).expect("fixture");

        let layout = layout_timeline(&text);

        let actual = layout.activity_line.x2;
        assert_eq!(
            actual,
            layout.pre_title_box_width + 3.0 * layout.left_margin
        );
        assert!(actual > layout.activity_line.x1);
        assert!(layout.bounds.expect("bounds").max_x >= actual);
    }

    #[test]
    fn empty_timeline_does_not_invent_pre_title_width() {
        let layout = layout_timeline("timeline");

        assert_eq!(layout.pre_title_box_width, 0.0);
        assert_eq!(layout.activity_line.x1, 150.0);
        assert_eq!(layout.activity_line.x2, 450.0);
        let bounds = layout.bounds.expect("bounds");
        assert_eq!(bounds.min_x, 100.0);
        assert_eq!(bounds.min_y, 50.0);
        assert_eq!(bounds.max_x, 500.0);
        assert_eq!(bounds.max_y, 150.0);
    }

    #[test]
    fn top_down_timeline_uses_vertical_axis_and_one_connector_per_event() {
        let layout = layout_timeline(concat!(
            "timeline TD\n",
            "section Delivery\n",
            "Plan : Event A : Event B\n",
            "Ship\n",
        ));

        assert_eq!(layout.direction, TimelineDirection::TopDown);
        assert_eq!(layout.activity_line.x1, layout.activity_line.x2);
        assert!(layout.activity_line.y2 > layout.activity_line.y1);
        let section = &layout.sections[0];
        assert_eq!(section.tasks.len(), 2);
        assert_eq!(section.tasks[0].connectors.len(), 2);
        assert!(section.tasks[1].connectors.is_empty());
        assert!(section.tasks[0].node.x < layout.activity_line.x1);
        assert!(
            section.tasks[0]
                .events
                .iter()
                .all(|event| event.x > layout.activity_line.x1)
        );
        assert!(section.tasks[1].node.y > section.tasks[0].node.y);
        for connector in &section.tasks[0].connectors {
            assert_eq!(connector.y1, connector.y2);
            assert_eq!(connector.x1, layout.activity_line.x1);
            assert!(connector.x2 > connector.x1);
        }
    }
}

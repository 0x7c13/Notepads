use crate::Result;
use crate::model::{Bounds, KanbanDiagramLayout, KanbanItemLayout, KanbanSectionLayout};
#[cfg(test)]
use crate::resources::RenderResourcePolicy;
use crate::resources::{ModelComplexity, OperationWorkMeter};
use crate::text::{TextMeasurer, TextMetrics, TextStyle, WrapMode};
use merman_core::diagrams::kanban::{KanbanDiagramRenderModel, KanbanRenderNode};
use merman_core::svg_security::{
    MermaidNavigationSecurity, SerializedMermaidNavigationHref, prepare_mermaid_navigation_href,
};
use std::collections::HashMap;

pub(crate) const KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX: f64 = 25.0;
pub(crate) const KANBAN_SECTION_PADDING_PX: f64 = 10.0;
pub(crate) const KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX: f64 = 24.0;
const KANBAN_ITEM_ONE_ROW_HEIGHT_PX: f64 = 44.0;
const KANBAN_ITEM_TWO_ROW_HEIGHT_PX: f64 = 56.0;

pub(crate) struct KanbanMarkdown<'a> {
    sanitize_config: &'a merman_core::MermaidConfig,
    auto_wrap: bool,
}

impl<'a> KanbanMarkdown<'a> {
    pub(crate) fn new(effective_config: &'a merman_core::MermaidConfig) -> Self {
        Self {
            sanitize_config: effective_config,
            auto_wrap: crate::config::config_bool(
                effective_config.as_value(),
                &["markdownAutoWrap"],
            )
            .unwrap_or(true),
        }
    }

    pub(crate) fn render(&self, raw: &str) -> String {
        let sanitized = merman_core::sanitize::sanitize_text(raw, self.sanitize_config);
        crate::text::mermaid_markdown_to_xhtml_label_fragment(&sanitized, self.auto_wrap)
    }

    pub(crate) fn measure_html(
        &self,
        measurer: &dyn TextMeasurer,
        html: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> TextMetrics {
        crate::text::measure_xhtml_label_fragment(
            measurer,
            html,
            style,
            max_width,
            WrapMode::HtmlLike,
        )
    }
}

mod config;

pub(crate) use config::{KanbanConfigView, default_use_max_width};

#[derive(Debug)]
pub(crate) struct KanbanPreparedArtifact {
    layout: KanbanDiagramLayout,
    sections: Vec<KanbanPreparedMarkdownLabel>,
    items: Vec<KanbanPreparedItem>,
}

impl KanbanPreparedArtifact {
    pub(crate) fn layout(&self) -> &KanbanDiagramLayout {
        &self.layout
    }

    pub(crate) fn render_parts(
        &self,
    ) -> (
        &KanbanDiagramLayout,
        &[KanbanPreparedMarkdownLabel],
        &[KanbanPreparedItem],
    ) {
        (&self.layout, &self.sections, &self.items)
    }
}

#[derive(Debug)]
pub(crate) struct KanbanPreparedMarkdownLabel {
    pub(crate) html: String,
    pub(crate) geometry: KanbanPreparedLabelGeometry,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KanbanPreparedLabelGeometry {
    pub(crate) content_height: f64,
    pub(crate) foreign_object_width: f64,
    pub(crate) wrapped: bool,
}

impl KanbanPreparedLabelGeometry {
    pub(crate) fn empty() -> Self {
        Self {
            content_height: 0.0,
            foreign_object_width: 0.0,
            wrapped: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct KanbanPreparedItem {
    pub(crate) title: KanbanPreparedMarkdownLabel,
    pub(crate) ticket_link: Option<KanbanPreparedTicketLink>,
}

#[derive(Debug)]
pub(crate) struct KanbanPreparedTicketLink {
    pub(crate) href: Option<SerializedMermaidNavigationHref>,
}

fn replace_first_like_javascript(source: &str, search: &str, replacement: &str) -> String {
    let Some(match_start) = source.find(search) else {
        return source.to_string();
    };
    let match_end = match_start + search.len();
    let prefix = &source[..match_start];
    let suffix = &source[match_end..];
    let mut result = String::with_capacity(source.len() + replacement.len());
    result.push_str(prefix);

    let mut cursor = 0usize;
    while let Some(relative_dollar) = replacement[cursor..].find('$') {
        let dollar = cursor + relative_dollar;
        result.push_str(&replacement[cursor..dollar]);
        match replacement.as_bytes().get(dollar + 1).copied() {
            Some(b'$') => result.push('$'),
            Some(b'&') => result.push_str(search),
            Some(b'`') => result.push_str(prefix),
            Some(b'\'') => result.push_str(suffix),
            _ => {
                result.push('$');
                cursor = dollar + 1;
                continue;
            }
        }
        cursor = dollar + 2;
    }
    result.push_str(&replacement[cursor..]);
    result.push_str(suffix);
    result
}

fn prepare_kanban_ticket_link(
    ticket_base_url: Option<&str>,
    ticket: Option<&str>,
    effective_config: &merman_core::MermaidConfig,
) -> Option<KanbanPreparedTicketLink> {
    let ticket = ticket.filter(|ticket| !ticket.is_empty())?;
    let ticket_url = replace_first_like_javascript(ticket_base_url?, "#TICKET#", ticket);
    let href = prepare_mermaid_navigation_href(
        &ticket_url,
        MermaidNavigationSecurity::from_security_level_loose(
            effective_config.get_str("securityLevel") == Some("loose"),
        ),
    );
    Some(KanbanPreparedTicketLink { href })
}

fn prepare_kanban_markdown_label(
    markdown: &KanbanMarkdown<'_>,
    measurer: &dyn TextMeasurer,
    raw: &str,
    style: &TextStyle,
    max_width: f64,
) -> (KanbanPreparedMarkdownLabel, TextMetrics) {
    let html = markdown.render(raw);
    let raw_metrics = markdown.measure_html(measurer, &html, style, None);
    let wrapped = max_width > 0.0 && raw_metrics.width > max_width;
    let metrics = if wrapped {
        markdown.measure_html(measurer, &html, style, Some(max_width))
    } else {
        raw_metrics
    };

    (
        KanbanPreparedMarkdownLabel {
            html,
            geometry: KanbanPreparedLabelGeometry {
                content_height: metrics.height,
                foreign_object_width: if wrapped {
                    max_width.max(0.0)
                } else {
                    metrics.width.max(0.0)
                },
                wrapped,
            },
        },
        metrics,
    )
}

fn prepare_kanban_title_label(
    markdown: &KanbanMarkdown<'_>,
    measurer: &dyn TextMeasurer,
    raw: &str,
    style: &TextStyle,
    max_width: f64,
) -> (KanbanPreparedMarkdownLabel, TextMetrics) {
    let (mut prepared, metrics) =
        prepare_kanban_markdown_label(markdown, measurer, raw, style, max_width);
    if raw.is_empty() {
        prepared.geometry.foreign_object_width = 0.0;
    }
    (prepared, metrics)
}

fn kanban_layout_work_units(model: &KanbanDiagramRenderModel) -> usize {
    let sections = model.nodes.iter().filter(|node| node.is_group).count();
    let items = model
        .nodes
        .iter()
        .filter(|node| node.parent_id.is_some())
        .count();
    model
        .nodes
        .len()
        .saturating_mul(2)
        .saturating_add(sections.saturating_mul(2))
        .saturating_add(items.saturating_mul(3))
}

#[cfg(test)]
pub(crate) fn layout_kanban_diagram_typed(
    model: &KanbanDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<KanbanDiagramLayout> {
    let effective_config = merman_core::MermaidConfig::from_value(effective_config.clone());
    let work_meter = OperationWorkMeter::new(RenderResourcePolicy::interactive());
    prepare_kanban_diagram_typed_with_work_meter(model, &effective_config, measurer, &work_meter)
        .map(|prepared| prepared.layout)
}

/// Prepares a Kanban model under the cumulative work meter owned by the render operation.
pub(crate) fn prepare_kanban_diagram_typed_with_work_meter(
    model: &KanbanDiagramRenderModel,
    effective_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
    work_meter: &OperationWorkMeter,
) -> Result<KanbanPreparedArtifact> {
    work_meter
        .policy()
        .check_model_complexity(ModelComplexity::from_kanban(model))?;
    work_meter.charge(kanban_layout_work_units(model))?;
    let config_view = KanbanConfigView::new(effective_config.as_value());
    let cfg = config_view.layout_settings();
    let ticket_base_url = config_view.ticket_base_url();
    let section_width = cfg.section_width;
    let viewbox_padding = cfg.viewbox_padding;
    let padding = KANBAN_SECTION_PADDING_PX;
    let section_rect_y = -(section_width * 3.0) / 2.0;

    let legend_style = cfg.text_style;
    let font_scale = legend_style.font_size / 16.0;
    let section_label_height_baseline = KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX * font_scale;
    let label_foreign_object_height = KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX * font_scale;
    let item_one_row_height = KANBAN_ITEM_ONE_ROW_HEIGHT_PX * font_scale;
    let item_two_row_height = KANBAN_ITEM_TWO_ROW_HEIGHT_PX * font_scale;
    let markdown = KanbanMarkdown::new(effective_config);

    let section_nodes: Vec<&KanbanRenderNode> = model.nodes.iter().filter(|n| n.is_group).collect();
    let item_capacity = model
        .nodes
        .iter()
        .filter(|node| node.parent_id.is_some())
        .count();
    let mut max_label_height = section_label_height_baseline;
    let mut sections: Vec<KanbanSectionLayout> = Vec::with_capacity(section_nodes.len());
    let mut items: Vec<KanbanItemLayout> = Vec::with_capacity(item_capacity);
    let mut prepared_sections = Vec::with_capacity(section_nodes.len());
    let mut prepared_items = Vec::with_capacity(item_capacity);

    let mut items_by_section: HashMap<&str, Vec<&KanbanRenderNode>> =
        HashMap::with_capacity(section_nodes.len());
    for node in &model.nodes {
        if let Some(parent_id) = node.parent_id.as_deref() {
            items_by_section.entry(parent_id).or_default().push(node);
        }
    }
    for (i, section) in section_nodes.iter().enumerate() {
        let index = (i + 1) as i64;
        let center_x = section_width * (index as f64) + ((index - 1) as f64 * padding) / 2.0;
        let center_y = 0.0;

        let (prepared_label, label_metrics) = prepare_kanban_markdown_label(
            &markdown,
            measurer,
            &section.label,
            &legend_style,
            section_width,
        );
        let label_height = label_metrics.height.max(label_foreign_object_height);
        max_label_height = max_label_height.max(label_height);

        sections.push(KanbanSectionLayout {
            id: section.id.clone(),
            label: section.label.clone(),
            index,
            center_x,
            center_y,
            width: section_width,
            rect_y: section_rect_y,
            rect_height: (section_width * 3.0).max(1.0),
            rx: 5.0,
            ry: 5.0,
            label_width: label_metrics.width.max(0.0),
            label_height,
        });
        prepared_sections.push(prepared_label);
    }

    for section in sections.iter_mut() {
        let top = section_rect_y + max_label_height;
        let mut y = top;

        for &item in items_by_section
            .get(section.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let width = (section_width - 1.5 * padding).max(1.0);
            let inner_max_w = (width - padding).max(0.0);

            // Mermaid's kanban items are rendered via `kanbanItem.ts`, which uses HTML labels for
            // the title and applies `max-width` clamping when the content needs wrapping. Mirror
            // that behavior so item heights match the upstream bbox-based layout.
            let (prepared_title, title_metrics) = prepare_kanban_title_label(
                &markdown,
                measurer,
                &item.label,
                &legend_style,
                inner_max_w,
            );

            let has_details_row = item.ticket.is_some() || item.assigned.is_some();
            let base_height = if has_details_row {
                item_two_row_height
            } else {
                item_one_row_height
            };
            let extra_title_height = (title_metrics.height - label_foreign_object_height).max(0.0);
            let height = base_height + extra_title_height;

            let center_x = section.center_x;
            let center_y = y + height / 2.0;

            items.push(KanbanItemLayout {
                id: item.id.clone(),
                label: item.label.clone(),
                parent_id: section.id.clone(),
                center_x,
                center_y,
                width,
                height: height.max(1.0),
                rx: 5.0,
                ry: 5.0,
                ticket: item.ticket.clone(),
                assigned: item.assigned.clone(),
                priority: item.priority.clone(),
                icon: item.icon.clone(),
            });
            prepared_items.push(KanbanPreparedItem {
                title: prepared_title,
                ticket_link: prepare_kanban_ticket_link(
                    ticket_base_url.as_deref(),
                    item.ticket.as_deref(),
                    effective_config,
                ),
            });

            y = center_y + height / 2.0 + padding / 2.0;
        }

        let min_section_height = 50.0 * font_scale;
        let height = (y - top + 3.0 * padding).max(min_section_height)
            + (max_label_height - section_label_height_baseline);
        section.rect_height = height.max(1.0);
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for s in &sections {
        let left = s.center_x - s.width / 2.0;
        let right = left + s.width;
        let top = s.rect_y;
        let bottom = s.rect_y + s.rect_height;
        min_x = min_x.min(left);
        min_y = min_y.min(top);
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }
    for n in &items {
        let left = n.center_x - n.width / 2.0;
        let right = n.center_x + n.width / 2.0;
        let top = n.center_y - n.height / 2.0;
        let bottom = n.center_y + n.height / 2.0;
        min_x = min_x.min(left);
        min_y = min_y.min(top);
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    let bounds = if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()
    {
        Some(Bounds {
            min_x: min_x - viewbox_padding,
            min_y: min_y - viewbox_padding,
            max_x: max_x + viewbox_padding,
            max_y: max_y + viewbox_padding,
        })
    } else {
        None
    };

    let layout = KanbanDiagramLayout {
        bounds,
        section_width,
        padding,
        max_label_height,
        viewbox_padding,
        use_max_width: cfg.use_max_width,
        sections,
        items,
    };
    Ok(KanbanPreparedArtifact {
        layout,
        sections: prepared_sections,
        items: prepared_items,
    })
}

#[cfg(test)]
pub(crate) fn prepare_kanban_artifact_from_layout_for_test(
    layout: &KanbanDiagramLayout,
    effective_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
) -> KanbanPreparedArtifact {
    let config_view = KanbanConfigView::new(effective_config.as_value());
    let settings = config_view.layout_settings();
    let ticket_base_url = config_view.ticket_base_url();
    let markdown = KanbanMarkdown::new(effective_config);
    let sections = layout
        .sections
        .iter()
        .map(|section| {
            prepare_kanban_markdown_label(
                &markdown,
                measurer,
                &section.label,
                &settings.text_style,
                section.width,
            )
            .0
        })
        .collect();
    let items = layout
        .items
        .iter()
        .map(|item| KanbanPreparedItem {
            title: prepare_kanban_title_label(
                &markdown,
                measurer,
                &item.label,
                &settings.text_style,
                (item.width - KANBAN_SECTION_PADDING_PX).max(0.0),
            )
            .0,
            ticket_link: prepare_kanban_ticket_link(
                ticket_base_url.as_deref(),
                item.ticket.as_deref(),
                effective_config,
            ),
        })
        .collect();

    KanbanPreparedArtifact {
        layout: layout.clone(),
        sections,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::{layout_kanban_diagram_typed, replace_first_like_javascript};
    use crate::text::DeterministicTextMeasurer;
    use merman_core::diagrams::kanban::{KanbanDiagramRenderModel, KanbanRenderNode};
    use serde_json::json;

    fn section(id: &str, label: &str) -> KanbanRenderNode {
        let mut node = KanbanRenderNode::new(id, label);
        node.is_group = true;
        node
    }

    fn item(id: &str, label: &str, parent_id: &str) -> KanbanRenderNode {
        let mut node = KanbanRenderNode::new(id, label);
        node.parent_id = Some(parent_id.to_string());
        node
    }

    #[test]
    fn kanban_geometry_constants_match_mermaid() {
        assert_eq!(super::KANBAN_SECTION_LABEL_HEIGHT_BASELINE_PX, 25.0);
        assert_eq!(super::KANBAN_SECTION_PADDING_PX, 10.0);
        assert_eq!(super::KANBAN_LABEL_FOREIGN_OBJECT_HEIGHT_PX, 24.0);
        assert_eq!(super::KANBAN_ITEM_ONE_ROW_HEIGHT_PX, 44.0);
        assert_eq!(super::KANBAN_ITEM_TWO_ROW_HEIGHT_PX, 56.0);
    }

    #[test]
    fn kanban_ticket_placeholder_replacement_matches_javascript_string_replace() {
        assert_eq!(
            replace_first_like_javascript("pre#TICKET#post#TICKET#", "#TICKET#", "MC-2038"),
            "preMC-2038post#TICKET#"
        );
        assert_eq!(
            replace_first_like_javascript("left#TICKET#right", "#TICKET#", "$$"),
            "left$right"
        );
        assert_eq!(
            replace_first_like_javascript("left#TICKET#right", "#TICKET#", "$&"),
            "left#TICKET#right"
        );
        assert_eq!(
            replace_first_like_javascript("left#TICKET#right", "#TICKET#", "$`"),
            "leftleftright"
        );
        assert_eq!(
            replace_first_like_javascript("left#TICKET#right", "#TICKET#", "$'"),
            "leftrightright"
        );
        assert_eq!(
            replace_first_like_javascript("https://example.test/tickets", "#TICKET#", "$&"),
            "https://example.test/tickets"
        );
    }

    #[test]
    fn kanban_layout_uses_mermaid_padding() {
        let model = KanbanDiagramRenderModel {
            nodes: vec![
                section("todo", "Todo"),
                section("doing", "Doing"),
                item("task-1", "Task", "todo"),
            ],
        };
        let measurer = DeterministicTextMeasurer {
            char_width_factor: 8.0,
            line_height_factor: 16.0,
        };

        let layout = layout_kanban_diagram_typed(&model, &json!({}), &measurer).unwrap();

        assert_eq!(layout.padding, super::KANBAN_SECTION_PADDING_PX);
        assert!(layout.use_max_width);
        assert_eq!(
            layout.items[0].width,
            layout.section_width - 1.5 * super::KANBAN_SECTION_PADDING_PX
        );
    }

    #[test]
    fn kanban_layout_measures_rendered_markdown_instead_of_source_markers() {
        let markdown_model = KanbanDiagramRenderModel {
            nodes: vec![
                section("todo", "Todo"),
                item("task-1", "*aaaa aaaa aaaaaaa*", "todo"),
                item("task-2", "Next", "todo"),
            ],
        };
        let plain_model = KanbanDiagramRenderModel {
            nodes: vec![
                section("todo", "Todo"),
                item("task-1", "aaaa aaaa aaaaaaa", "todo"),
                item("task-2", "Next", "todo"),
            ],
        };
        let measurer = DeterministicTextMeasurer::default();

        let markdown_layout =
            layout_kanban_diagram_typed(&markdown_model, &json!({}), &measurer).unwrap();
        let plain_layout =
            layout_kanban_diagram_typed(&plain_model, &json!({}), &measurer).unwrap();

        assert_eq!(
            markdown_layout.items[0].height,
            plain_layout.items[0].height
        );
        assert_eq!(
            markdown_layout.items[1].center_y,
            plain_layout.items[1].center_y
        );
        assert_eq!(
            markdown_layout.sections[0].rect_height,
            plain_layout.sections[0].rect_height
        );
        let markdown_bounds = markdown_layout.bounds.as_ref().unwrap();
        let plain_bounds = plain_layout.bounds.as_ref().unwrap();
        assert_eq!(markdown_bounds.min_x, plain_bounds.min_x);
        assert_eq!(markdown_bounds.min_y, plain_bounds.min_y);
        assert_eq!(markdown_bounds.max_x, plain_bounds.max_x);
        assert_eq!(markdown_bounds.max_y, plain_bounds.max_y);
    }

    #[test]
    fn kanban_layout_uses_mermaid_mindmap_viewport_config_precedence() {
        let model = KanbanDiagramRenderModel {
            nodes: vec![section("todo", "Todo")],
        };
        let measurer = DeterministicTextMeasurer {
            char_width_factor: 8.0,
            line_height_factor: 16.0,
        };

        let layout = layout_kanban_diagram_typed(
            &model,
            &json!({
                "mindmap": {
                    "padding": 3,
                    "useMaxWidth": false
                },
                "kanban": {
                    "padding": 12,
                    "useMaxWidth": true
                }
            }),
            &measurer,
        )
        .unwrap();

        assert_eq!(layout.viewbox_padding, 3.0);
        assert!(!layout.use_max_width);
    }
}

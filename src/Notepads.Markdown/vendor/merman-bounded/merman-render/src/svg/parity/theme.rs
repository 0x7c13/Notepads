use super::util::SvgTheme;
#[cfg(feature = "diagram-xychart")]
use crate::chart_palette::resolve_xychart_plot_palette;
use merman_core::theme_color::{darken, lighten};
use serde_json::Value;

mod families;
mod helpers;

use helpers::*;

#[derive(Debug, Clone)]
pub(super) struct CommonCssTheme {
    pub(super) theme_name: String,
    pub(super) look: String,
    pub(super) font_family_css: String,
    pub(super) font_size_px: f64,
    pub(super) text_color: String,
    pub(super) line_color: String,
    pub(super) error_bkg: String,
    pub(super) error_text: String,
}

impl CommonCssTheme {
    pub(super) fn is_dark_theme(&self) -> bool {
        self.theme_name.contains("dark")
    }

    pub(super) fn is_neo(&self) -> bool {
        self.look == "neo"
    }
}

#[derive(Debug, Clone)]
#[cfg(any(
    feature = "diagram-agentflow",
    feature = "diagram-mindmap",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-block"
))]
pub(super) struct NodeDiagramTheme {
    pub(super) common: CommonCssTheme,
    pub(super) node_text_color: String,
    pub(super) title_color: String,
    pub(super) main_bkg: String,
    pub(super) node_border: String,
    pub(super) arrowhead_color: String,
    pub(super) stroke_width: String,
    pub(super) edge_label_background: String,
    pub(super) tertiary: String,
    pub(super) cluster_bkg: String,
    pub(super) cluster_border: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-class")]
pub(super) struct ClassDiagramTheme {
    pub(super) common: CommonCssTheme,
    pub(super) class_text: String,
    pub(super) note_text: String,
    pub(super) class_group_text: String,
    pub(super) title_color: String,
    pub(super) text_color: String,
    pub(super) main_bkg: String,
    pub(super) node_border: String,
    pub(super) cluster_bkg: String,
    pub(super) cluster_border: String,
    pub(super) stroke_width: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-sequence")]
pub(super) struct SequenceDiagramTheme {
    pub(super) common: CommonCssTheme,
    pub(super) actor_border: String,
    pub(super) actor_fill: String,
    pub(super) stroke_width: String,
    pub(super) drop_shadow: String,
    pub(super) note_border: String,
    pub(super) note_fill: String,
    pub(super) actor_text: String,
    pub(super) actor_line: String,
    pub(super) signal_color: String,
    pub(super) sequence_number: String,
    pub(super) signal_text: String,
    pub(super) label_box_border: String,
    pub(super) label_box_fill: String,
    pub(super) label_text: String,
    pub(super) loop_text: String,
    pub(super) note_text: String,
    pub(super) activation_fill: String,
    pub(super) activation_border: String,
    pub(super) node_border: String,
    pub(super) label_box_filter: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-state")]
pub(super) struct StateDiagramTheme {
    pub(super) common: CommonCssTheme,
    pub(super) transition_color: String,
    pub(super) node_border: String,
    pub(super) background: String,
    pub(super) main_bkg: String,
    pub(super) alt_background: String,
    pub(super) stroke_width: String,
    pub(super) stroke_width_px: String,
    pub(super) rough_stroke_width_value: f64,
    pub(super) note_border: String,
    pub(super) note_bkg: String,
    pub(super) note_text: String,
    pub(super) label_background: String,
    pub(super) edge_label_background: String,
    pub(super) transition_label_color: String,
    pub(super) special_state_color: String,
    pub(super) inner_end_background: String,
    pub(super) composite_background: String,
    pub(super) state_bkg: String,
    pub(super) state_border: String,
    pub(super) composite_title_background: String,
    pub(super) state_label_color: String,
    pub(super) drop_shadow: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-xychart")]
pub(crate) struct XyChartTheme {
    pub(crate) background_color: String,
    pub(crate) title_color: String,
    pub(crate) legend_text_color: String,
    pub(crate) x_axis_title_color: String,
    pub(crate) x_axis_label_color: String,
    pub(crate) x_axis_tick_color: String,
    pub(crate) x_axis_line_color: String,
    pub(crate) y_axis_title_color: String,
    pub(crate) y_axis_label_color: String,
    pub(crate) y_axis_tick_color: String,
    pub(crate) y_axis_line_color: String,
    pub(crate) plot_color_palette: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-quadrant-chart")]
pub(crate) struct QuadrantChartTheme {
    pub(crate) quadrant1_fill: String,
    pub(crate) quadrant2_fill: String,
    pub(crate) quadrant3_fill: String,
    pub(crate) quadrant4_fill: String,
    pub(crate) quadrant1_text_fill: String,
    pub(crate) quadrant2_text_fill: String,
    pub(crate) quadrant3_text_fill: String,
    pub(crate) quadrant4_text_fill: String,
    pub(crate) quadrant_point_fill: String,
    pub(crate) quadrant_point_text_fill: String,
    pub(crate) quadrant_x_axis_text_fill: String,
    pub(crate) quadrant_y_axis_text_fill: String,
    pub(crate) quadrant_title_fill: String,
    pub(crate) quadrant_internal_border_stroke_fill: String,
    pub(crate) quadrant_external_border_stroke_fill: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-tree-view")]
pub(crate) struct TreeViewTheme {
    pub(crate) label_font_size: f64,
    pub(crate) label_font_size_css: String,
    pub(crate) label_color: String,
    pub(crate) line_color: String,
    pub(crate) icon_color: String,
    pub(crate) description_color: String,
    pub(crate) highlight_bg: String,
    pub(crate) highlight_stroke: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-treemap")]
pub(crate) struct TreemapTheme {
    pub(crate) title_color: String,
    pub(crate) label_color: String,
    pub(crate) value_color: String,
    pub(crate) section_stroke_color: String,
    pub(crate) section_stroke_width: String,
    pub(crate) section_fill_color: String,
    pub(crate) leaf_stroke_color: String,
    pub(crate) leaf_stroke_width: String,
    pub(crate) leaf_fill_color: String,
    pub(crate) label_font_size: String,
    pub(crate) value_font_size: String,
    pub(crate) title_font_size: String,
    pub(crate) color_scale: Vec<String>,
    pub(crate) color_scale_peer: Vec<String>,
    pub(crate) color_scale_label: Vec<String>,
    text_color: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-gantt")]
pub(crate) struct GanttTheme {
    pub(crate) font_family: String,
    pub(crate) text_color: String,
    pub(crate) exclude_bkg_color: String,
    pub(crate) section_bkg_color: String,
    pub(crate) section_bkg_color2: String,
    pub(crate) alt_section_bkg_color: String,
    pub(crate) title_color: String,
    pub(crate) title_text_color: String,
    pub(crate) grid_color: String,
    pub(crate) today_line_color: String,
    pub(crate) task_text_dark_color: String,
    pub(crate) task_text_clickable_color: String,
    pub(crate) task_text_color: String,
    pub(crate) task_bkg_color: String,
    pub(crate) task_border_color: String,
    pub(crate) task_text_outside_color: String,
    pub(crate) active_task_bkg_color: String,
    pub(crate) active_task_border_color: String,
    pub(crate) done_task_border_color: String,
    pub(crate) done_task_bkg_color: String,
    pub(crate) crit_border_color: String,
    pub(crate) crit_bkg_color: String,
    pub(crate) vert_line_color: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-kanban")]
pub(crate) struct KanbanSectionTheme {
    pub(crate) section_fill: String,
    pub(crate) c_scale: String,
    pub(crate) c_scale_label: String,
    pub(crate) c_scale_inv: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-kanban")]
pub(crate) struct KanbanTheme {
    pub(crate) text_color: String,
    pub(crate) background: String,
    pub(crate) node_border: String,
    pub(crate) root_fill: String,
    pub(crate) root_label: String,
    pub(crate) sections: Vec<KanbanSectionTheme>,
}

#[cfg(feature = "diagram-treemap")]
impl TreemapTheme {
    pub(crate) fn readable_leaf_label_fill(
        &self,
        leaf_fill: &str,
        leaf_rect_style: &str,
        leaf_label_fill: String,
    ) -> String {
        if css_color_is_transparent(leaf_fill)
            && !style_has_non_empty_decl(leaf_rect_style, "fill")
            && css_color_is_white_like(&leaf_label_fill)
        {
            self.text_color.clone()
        } else {
            leaf_label_fill
        }
    }
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-event-modeling")]
pub(crate) struct EventModelingTheme {
    pub(crate) font_family_css: String,
    pub(crate) text_color: String,
    pub(crate) ui_fill: String,
    pub(crate) ui_stroke: String,
    pub(crate) processor_fill: String,
    pub(crate) processor_stroke: String,
    pub(crate) read_model_fill: String,
    pub(crate) read_model_stroke: String,
    pub(crate) command_fill: String,
    pub(crate) command_stroke: String,
    pub(crate) event_fill: String,
    pub(crate) event_stroke: String,
    pub(crate) swimlane_background_fill: String,
    pub(crate) swimlane_background_stroke: String,
    pub(crate) relation_stroke: String,
    pub(crate) arrowhead_fill: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-ishikawa")]
pub(crate) struct IshikawaTheme {
    pub(crate) line_color: String,
    pub(crate) main_bkg: String,
    pub(crate) text_color: String,
    pub(crate) font_family: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-venn")]
pub(crate) struct VennTheme {
    pub(crate) font_family_css: String,
    pub(crate) title_color: String,
    pub(crate) set_text_color: String,
    pub(crate) circle_colors: Vec<String>,
    pub(crate) primary_color: String,
    pub(crate) is_dark_theme: bool,
}

#[cfg(feature = "diagram-venn")]
impl VennTheme {
    pub(crate) fn circle_text_color(&self, base_color: &str) -> crate::Result<String> {
        if self.is_dark_theme {
            Ok(lighten(base_color, 30.0)?)
        } else {
            Ok(darken(base_color, 30.0)?)
        }
    }
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-journey")]
pub(crate) struct JourneyTheme {
    pub(crate) font_family_css: String,
    pub(crate) text_color: String,
    pub(crate) line_color: String,
    pub(crate) face_color: String,
    pub(crate) main_bkg: String,
    pub(crate) node_border: String,
    pub(crate) arrowhead_color: String,
    pub(crate) edge_label_background: String,
    pub(crate) title_color: String,
    pub(crate) tertiary_color: String,
    pub(crate) border2: String,
    pub(crate) fill_types: Vec<String>,
    pub(crate) actor_colors: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-radar")]
pub(crate) struct RadarTheme {
    pub(crate) font_family_css: String,
    pub(crate) base_font_size_css: String,
    pub(crate) text_color: String,
    pub(crate) line_color: String,
    pub(crate) error_bkg_color: String,
    pub(crate) error_text_color: String,
    pub(crate) title_font_size_css: String,
    pub(crate) title_color: String,
    pub(crate) axis_color: String,
    pub(crate) axis_stroke_width: f64,
    pub(crate) axis_label_font_size: f64,
    pub(crate) graticule_color: String,
    pub(crate) graticule_opacity: f64,
    pub(crate) graticule_stroke_width: f64,
    pub(crate) legend_font_size: f64,
    pub(crate) curve_opacity: f64,
    pub(crate) curve_stroke_width: f64,
    pub(crate) series_colors: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-timeline")]
pub(crate) struct TimelineSectionTheme {
    pub(crate) c_scale: String,
    pub(crate) c_scale_label: String,
    pub(crate) c_scale_inv: String,
}

#[derive(Debug, Clone)]
#[cfg(feature = "diagram-timeline")]
pub(crate) struct TimelineTheme {
    pub(crate) is_redux_theme: bool,
    pub(crate) is_dark_theme: bool,
    pub(crate) is_color_theme: bool,
    pub(crate) stroke_width: String,
    pub(crate) font_weight: String,
    pub(crate) main_bkg: String,
    pub(crate) node_border: String,
    pub(crate) disabled_fill: String,
    pub(crate) disabled_text_fill: String,
    pub(crate) root_fill: String,
    pub(crate) root_label: String,
    pub(crate) border_colors: Vec<String>,
    pub(crate) sections: Vec<TimelineSectionTheme>,
}

pub(crate) struct PresentationTheme<'a> {
    raw: SvgTheme<'a>,
    common: CommonCssTheme,
}

impl<'a> PresentationTheme<'a> {
    pub(crate) fn new(effective_config: &'a Value) -> Self {
        let raw = SvgTheme::new(effective_config);
        let common = CommonCssTheme {
            theme_name: raw.theme_name(),
            look: raw.look(),
            font_family_css: raw.font_family_css(),
            font_size_px: raw.font_size_px(),
            text_color: raw.color("textColor", "#333"),
            line_color: raw.color("lineColor", "#333333"),
            error_bkg: raw.color("errorBkgColor", "#552222"),
            error_text: raw.color("errorTextColor", "#552222"),
        };

        Self { raw, common }
    }
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests;

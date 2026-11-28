use crate::config::{config_bool, config_f64_or, config_font_family_css};
use crate::text::{TextMeasurer, TextStyle};
use crate::{Error, Result};
use merman_core::diagrams::wardley::{
    WardleyDiagramRenderModel, WardleyFlowDirection, WardleySourceStrategy,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const DEFAULT_WIDTH: f64 = 900.0;
const DEFAULT_HEIGHT: f64 = 600.0;
const DEFAULT_PADDING: f64 = 48.0;
const DEFAULT_NODE_RADIUS: f64 = 6.0;
const DEFAULT_NODE_LABEL_OFFSET: f64 = 8.0;
const DEFAULT_AXIS_FONT_SIZE: f64 = 12.0;
const DEFAULT_LABEL_FONT_SIZE: f64 = 10.0;
const DEFAULT_SHOW_GRID: bool = false;
const DEFAULT_USE_MAX_WIDTH: bool = true;
const DEFAULT_STAGES: [&str; 4] = ["Genesis", "Custom Built", "Product", "Commodity"];

#[derive(Debug, Clone, Copy)]
struct WardleySettings {
    width: f64,
    height: f64,
    padding: f64,
    node_radius: f64,
    node_label_offset: f64,
    axis_font_size: f64,
    label_font_size: f64,
    show_grid: bool,
    use_max_width: bool,
}

impl WardleySettings {
    fn from_config(effective_config: &Value) -> Self {
        let wardley = effective_config.get("wardley-beta").unwrap_or(&Value::Null);
        Self {
            width: config_f64_or(wardley, &["width"], DEFAULT_WIDTH),
            height: config_f64_or(wardley, &["height"], DEFAULT_HEIGHT),
            padding: config_f64_or(wardley, &["padding"], DEFAULT_PADDING),
            node_radius: config_f64_or(wardley, &["nodeRadius"], DEFAULT_NODE_RADIUS),
            node_label_offset: config_f64_or(
                wardley,
                &["nodeLabelOffset"],
                DEFAULT_NODE_LABEL_OFFSET,
            ),
            axis_font_size: config_f64_or(wardley, &["axisFontSize"], DEFAULT_AXIS_FONT_SIZE),
            label_font_size: config_f64_or(wardley, &["labelFontSize"], DEFAULT_LABEL_FONT_SIZE),
            show_grid: config_bool(wardley, &["showGrid"]).unwrap_or(DEFAULT_SHOW_GRID),
            use_max_width: config_bool(wardley, &["useMaxWidth"]).unwrap_or(DEFAULT_USE_MAX_WIDTH),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WardleyPointLayout {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WardleyLineLayout {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl WardleyLineLayout {
    fn between(start: WardleyPointLayout, end: WardleyPointLayout) -> Self {
        Self {
            x1: start.x,
            y1: start.y,
            x2: end.x,
            y2: end.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WardleyCircleLayout {
    pub center: WardleyPointLayout,
    pub radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WardleyRectLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub corner_radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WardleyRotationLayout {
    pub degrees: f64,
    pub cx: f64,
    pub cy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleyTextAnchor {
    Start,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleyFontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleyDominantBaseline {
    Auto,
    Middle,
    Central,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyTextLayout {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub font_size: f64,
    pub font_weight: WardleyFontWeight,
    pub text_anchor: WardleyTextAnchor,
    pub dominant_baseline: Option<WardleyDominantBaseline>,
    pub rotation: Option<WardleyRotationLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyAxesLayout {
    pub x_axis: WardleyLineLayout,
    pub y_axis: WardleyLineLayout,
    pub x_label: WardleyTextLayout,
    pub y_label: WardleyTextLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyStageLayout {
    pub name: String,
    pub start_x: f64,
    pub end_x: f64,
    pub label: WardleyTextLayout,
    pub divider: Option<WardleyLineLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyGridLayout {
    pub ratio: f64,
    pub vertical: WardleyLineLayout,
    pub horizontal: WardleyLineLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyPipelineBoxLayout {
    pub node_id: String,
    pub component_ids: Vec<String>,
    pub rect: WardleyRectLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyPipelineLinkLayout {
    pub source: String,
    pub target: String,
    pub line: WardleyLineLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WardleyFlowMarkersLayout {
    pub start: bool,
    pub end: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyLinkLayout {
    pub source: String,
    pub target: String,
    pub line: WardleyLineLayout,
    pub dashed: bool,
    pub markers: WardleyFlowMarkersLayout,
    pub label: Option<WardleyTextLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyTrendLayout {
    pub node_id: String,
    pub origin: WardleyPointLayout,
    pub target: WardleyPointLayout,
    pub line: WardleyLineLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WardleyNodeShapeLayout {
    Anchor,
    None,
    Circle { circle: WardleyCircleLayout },
    PipelineSquare { rect: WardleyRectLayout },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WardleySourceOverlayLayout {
    Build {
        circle: WardleyCircleLayout,
    },
    Buy {
        circle: WardleyCircleLayout,
    },
    Outsource {
        circle: WardleyCircleLayout,
    },
    Market {
        outer_circle: WardleyCircleLayout,
        connectors: Vec<WardleyLineLayout>,
        dots: Vec<WardleyCircleLayout>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyNodeLayout {
    pub id: String,
    pub label: String,
    pub class_name: Option<String>,
    pub position: WardleyPointLayout,
    pub in_pipeline: bool,
    pub is_pipeline_parent: bool,
    pub source_strategy: Option<WardleySourceStrategy>,
    pub shape: WardleyNodeShapeLayout,
    pub source_overlay: Option<WardleySourceOverlayLayout>,
    pub inertia: Option<WardleyLineLayout>,
    pub label_layout: WardleyTextLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyAnnotationPointLayout {
    pub center: WardleyPointLayout,
    pub radius: f64,
    pub label: WardleyTextLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyAnnotationLayout {
    pub number: u64,
    pub points: Vec<WardleyAnnotationPointLayout>,
    pub segments: Vec<WardleyLineLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyAnnotationsBoxLayout {
    pub rect: Option<WardleyRectLayout>,
    pub lines: Vec<WardleyTextLayout>,
    pub padding: f64,
    pub line_height: f64,
    pub font_size: f64,
    pub max_text_width: f64,
    pub max_text_height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyNoteLayout {
    pub text: WardleyTextLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WardleyArrowDirection {
    Right,
    Left,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyArrowLayout {
    pub name: String,
    pub direction: WardleyArrowDirection,
    pub origin: WardleyPointLayout,
    pub width: f64,
    pub height: f64,
    pub head_width: f64,
    pub path: Vec<WardleyPointLayout>,
    pub label: WardleyTextLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WardleyDiagramLayout {
    pub width: f64,
    pub height: f64,
    pub use_max_width: bool,
    pub padding: f64,
    pub chart_width: f64,
    pub chart_height: f64,
    pub node_radius: f64,
    pub node_label_offset: f64,
    pub axis_font_size: f64,
    pub label_font_size: f64,
    pub square_size: f64,
    pub title: Option<WardleyTextLayout>,
    pub axes: WardleyAxesLayout,
    pub stages: Vec<WardleyStageLayout>,
    pub grid: Vec<WardleyGridLayout>,
    pub pipeline_boxes: Vec<WardleyPipelineBoxLayout>,
    pub pipeline_links: Vec<WardleyPipelineLinkLayout>,
    pub links: Vec<WardleyLinkLayout>,
    pub trends: Vec<WardleyTrendLayout>,
    pub nodes: Vec<WardleyNodeLayout>,
    pub annotations: Vec<WardleyAnnotationLayout>,
    pub annotations_box: Option<WardleyAnnotationsBoxLayout>,
    pub notes: Vec<WardleyNoteLayout>,
    pub accelerators: Vec<WardleyArrowLayout>,
    pub deaccelerators: Vec<WardleyArrowLayout>,
}

fn text_layout(
    text: impl Into<String>,
    x: f64,
    y: f64,
    font_size: f64,
    font_weight: WardleyFontWeight,
    text_anchor: WardleyTextAnchor,
    dominant_baseline: Option<WardleyDominantBaseline>,
) -> WardleyTextLayout {
    WardleyTextLayout {
        text: text.into(),
        x,
        y,
        font_size,
        font_weight,
        text_anchor,
        dominant_baseline,
        rotation: None,
    }
}

fn project(
    x: f64,
    y: f64,
    height: f64,
    padding: f64,
    chart_width: f64,
    chart_height: f64,
) -> WardleyPointLayout {
    WardleyPointLayout {
        x: padding + (x / 100.0) * chart_width,
        y: height - padding - (y / 100.0) * chart_height,
    }
}

fn build_axes(
    model: &WardleyDiagramRenderModel,
    settings: WardleySettings,
    width: f64,
    height: f64,
    chart_width: f64,
    chart_height: f64,
) -> WardleyAxesLayout {
    let x_label = model.axes.x_label.as_deref().unwrap_or("Evolution");
    let y_label = model.axes.y_label.as_deref().unwrap_or("Visibility");
    let y_label_x = settings.padding / 3.0;
    let y_label_y = settings.padding + chart_height / 2.0;
    let mut y_label_layout = text_layout(
        y_label,
        y_label_x,
        y_label_y,
        settings.axis_font_size,
        WardleyFontWeight::Bold,
        WardleyTextAnchor::Middle,
        None,
    );
    y_label_layout.rotation = Some(WardleyRotationLayout {
        degrees: -90.0,
        cx: y_label_x,
        cy: y_label_y,
    });

    WardleyAxesLayout {
        x_axis: WardleyLineLayout {
            x1: settings.padding,
            y1: height - settings.padding,
            x2: width - settings.padding,
            y2: height - settings.padding,
        },
        y_axis: WardleyLineLayout {
            x1: settings.padding,
            y1: settings.padding,
            x2: settings.padding,
            y2: height - settings.padding,
        },
        x_label: text_layout(
            x_label,
            settings.padding + chart_width / 2.0,
            height - settings.padding / 4.0,
            settings.axis_font_size,
            WardleyFontWeight::Bold,
            WardleyTextAnchor::Middle,
            None,
        ),
        y_label: y_label_layout,
    }
}

fn build_stages(
    model: &WardleyDiagramRenderModel,
    settings: WardleySettings,
    height: f64,
    chart_width: f64,
) -> Vec<WardleyStageLayout> {
    let stages: Vec<String> = if model.axes.stages.is_empty() {
        DEFAULT_STAGES
            .iter()
            .map(|stage| (*stage).to_string())
            .collect()
    } else {
        model.axes.stages.clone()
    };
    let stage_count = stages.len();
    let custom_boundaries = model.axes.stage_boundaries.len() == stages.len();
    let mut previous_boundary = 0.0;

    stages
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let (start, end) = if custom_boundaries {
                let end = model.axes.stage_boundaries[index];
                let start = previous_boundary;
                previous_boundary = end;
                (start, end)
            } else {
                let stage_width = 1.0 / stage_count as f64;
                (index as f64 * stage_width, (index + 1) as f64 * stage_width)
            };
            let start_x = settings.padding + start * chart_width;
            let end_x = settings.padding + end * chart_width;
            WardleyStageLayout {
                name: name.clone(),
                start_x,
                end_x,
                label: text_layout(
                    name,
                    (start_x + end_x) / 2.0,
                    height - settings.padding / 1.5,
                    settings.axis_font_size - 2.0,
                    WardleyFontWeight::Normal,
                    WardleyTextAnchor::Middle,
                    None,
                ),
                divider: (index > 0).then_some(WardleyLineLayout {
                    x1: start_x,
                    y1: settings.padding,
                    x2: start_x,
                    y2: height - settings.padding,
                }),
            }
        })
        .collect()
}

fn build_grid(
    settings: WardleySettings,
    width: f64,
    height: f64,
    chart_width: f64,
    chart_height: f64,
) -> Vec<WardleyGridLayout> {
    if !settings.show_grid {
        return Vec::new();
    }
    (1..4)
        .map(|index| {
            let ratio = index as f64 / 4.0;
            let x = settings.padding + chart_width * ratio;
            let y = height - settings.padding - chart_height * ratio;
            WardleyGridLayout {
                ratio,
                vertical: WardleyLineLayout {
                    x1: x,
                    y1: settings.padding,
                    x2: x,
                    y2: height - settings.padding,
                },
                horizontal: WardleyLineLayout {
                    x1: settings.padding,
                    y1: y,
                    x2: width - settings.padding,
                    y2: y,
                },
            }
        })
        .collect()
}

fn source_overlay(
    strategy: Option<WardleySourceStrategy>,
    position: WardleyPointLayout,
    node_radius: f64,
) -> Option<WardleySourceOverlayLayout> {
    let circle = WardleyCircleLayout {
        center: position,
        radius: node_radius * 2.0,
    };
    match strategy? {
        WardleySourceStrategy::Build => Some(WardleySourceOverlayLayout::Build { circle }),
        WardleySourceStrategy::Buy => Some(WardleySourceOverlayLayout::Buy { circle }),
        WardleySourceStrategy::Outsource => Some(WardleySourceOverlayLayout::Outsource { circle }),
        WardleySourceStrategy::Market => {
            let triangle_radius = node_radius * 1.2;
            let dot_radius = node_radius * 0.7;
            let top = WardleyPointLayout {
                x: position.x,
                y: position.y - triangle_radius,
            };
            let bottom_dx = triangle_radius * std::f64::consts::FRAC_PI_6.cos();
            let bottom_dy = triangle_radius * std::f64::consts::FRAC_PI_6.sin();
            let bottom_left = WardleyPointLayout {
                x: position.x - bottom_dx,
                y: position.y + bottom_dy,
            };
            let bottom_right = WardleyPointLayout {
                x: position.x + bottom_dx,
                y: position.y + bottom_dy,
            };
            Some(WardleySourceOverlayLayout::Market {
                outer_circle: circle,
                connectors: vec![
                    WardleyLineLayout::between(top, bottom_left),
                    WardleyLineLayout::between(bottom_left, bottom_right),
                    WardleyLineLayout::between(bottom_right, top),
                ],
                dots: vec![
                    WardleyCircleLayout {
                        center: top,
                        radius: dot_radius,
                    },
                    WardleyCircleLayout {
                        center: bottom_left,
                        radius: dot_radius,
                    },
                    WardleyCircleLayout {
                        center: bottom_right,
                        radius: dot_radius,
                    },
                ],
            })
        }
    }
}

fn build_arrow(
    name: &str,
    origin: WardleyPointLayout,
    direction: WardleyArrowDirection,
) -> WardleyArrowLayout {
    let width = 60.0;
    let height = 30.0;
    let head_width = 20.0;
    let half_height = height / 2.0;
    let head_base_x = match direction {
        WardleyArrowDirection::Right => origin.x + width - head_width,
        WardleyArrowDirection::Left => origin.x + head_width,
    };
    let path = match direction {
        WardleyArrowDirection::Right => vec![
            WardleyPointLayout {
                x: origin.x,
                y: origin.y - half_height,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y - half_height,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y - half_height - 8.0,
            },
            WardleyPointLayout {
                x: origin.x + width,
                y: origin.y,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y + half_height + 8.0,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y + half_height,
            },
            WardleyPointLayout {
                x: origin.x,
                y: origin.y + half_height,
            },
        ],
        WardleyArrowDirection::Left => vec![
            WardleyPointLayout {
                x: origin.x + width,
                y: origin.y - half_height,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y - half_height,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y - half_height - 8.0,
            },
            WardleyPointLayout {
                x: origin.x,
                y: origin.y,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y + half_height + 8.0,
            },
            WardleyPointLayout {
                x: head_base_x,
                y: origin.y + half_height,
            },
            WardleyPointLayout {
                x: origin.x + width,
                y: origin.y + half_height,
            },
        ],
    };
    WardleyArrowLayout {
        name: name.to_string(),
        direction,
        origin,
        width,
        height,
        head_width,
        path,
        label: text_layout(
            name,
            origin.x + width / 2.0,
            origin.y + half_height + 15.0,
            10.0,
            WardleyFontWeight::Bold,
            WardleyTextAnchor::Middle,
            None,
        ),
    }
}

pub(crate) fn layout_wardley_diagram_typed(
    model: &WardleyDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
) -> Result<WardleyDiagramLayout> {
    let settings = WardleySettings::from_config(effective_config);
    let width = model.size.map_or(settings.width, |size| size.width);
    let height = model.size.map_or(settings.height, |size| size.height);
    let chart_width = width - settings.padding * 2.0;
    let chart_height = height - settings.padding * 2.0;
    let square_size = settings.node_radius * 1.6;
    let project_point =
        |x: f64, y: f64| project(x, y, height, settings.padding, chart_width, chart_height);

    let title_text = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        });
    let title = title_text.map(|title| {
        text_layout(
            title,
            width / 2.0,
            settings.padding / 2.0,
            settings.axis_font_size * 1.05,
            WardleyFontWeight::Bold,
            WardleyTextAnchor::Middle,
            Some(WardleyDominantBaseline::Middle),
        )
    });

    let axes = build_axes(model, settings, width, height, chart_width, chart_height);
    let stages = build_stages(model, settings, height, chart_width);
    let grid = build_grid(settings, width, height, chart_width, chart_height);

    // JavaScript uses a Map here: duplicate ids keep their first insertion order but their last
    // projected value. The output still follows `data.nodes` order below.
    let mut positions: HashMap<&str, WardleyPointLayout> = HashMap::new();
    let mut nodes_by_id = HashMap::new();
    for node in &model.nodes {
        positions.insert(node.id.as_str(), project_point(node.x, node.y));
        nodes_by_id.entry(node.id.as_str()).or_insert(node);
    }

    let mut pipeline_boxes = Vec::new();
    let mut pipeline_links = Vec::new();
    for pipeline in &model.pipelines {
        if pipeline.component_ids.is_empty() {
            continue;
        }

        let mut sorted_components: Vec<(&str, WardleyPointLayout, f64)> = pipeline
            .component_ids
            .iter()
            .filter_map(|id| {
                let position = *positions.get(id.as_str())?;
                let node = *nodes_by_id.get(id.as_str())?;
                Some((id.as_str(), position, node.x))
            })
            .collect();
        sorted_components.sort_by(|left, right| left.2.total_cmp(&right.2));
        for pair in sorted_components.windows(2) {
            pipeline_links.push(WardleyPipelineLinkLayout {
                source: pair[0].0.to_string(),
                target: pair[1].0.to_string(),
                line: WardleyLineLayout::between(pair[0].1, pair[1].1),
            });
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut last_y = None;
        for component_id in &pipeline.component_ids {
            if let Some(position) = positions.get(component_id.as_str()).copied() {
                min_x = min_x.min(position.x);
                max_x = max_x.max(position.x);
                last_y = Some(position.y);
            }
        }
        let Some(y) = last_y else {
            continue;
        };

        let box_height = settings.node_radius * 4.0;
        let box_top = y - box_height / 2.0;
        if let Some(parent) = positions.get_mut(pipeline.node_id.as_str()) {
            parent.x = (min_x + max_x) / 2.0;
            parent.y = box_top - square_size / 6.0;
        }
        pipeline_boxes.push(WardleyPipelineBoxLayout {
            node_id: pipeline.node_id.clone(),
            component_ids: pipeline.component_ids.clone(),
            rect: WardleyRectLayout {
                x: min_x - 15.0,
                y: box_top,
                width: max_x - min_x + 30.0,
                height: box_height,
                corner_radius: 4.0,
            },
        });
    }

    let mut pipeline_map: HashMap<&str, HashSet<&str>> = HashMap::new();
    for pipeline in &model.pipelines {
        pipeline_map.insert(
            pipeline.node_id.as_str(),
            pipeline.component_ids.iter().map(String::as_str).collect(),
        );
    }

    let mut links = Vec::new();
    for link in &model.links {
        let (Some(source), Some(target)) = (
            positions.get(link.source.as_str()).copied(),
            positions.get(link.target.as_str()).copied(),
        ) else {
            continue;
        };
        if pipeline_map
            .get(link.target.as_str())
            .is_some_and(|components| components.contains(link.source.as_str()))
        {
            continue;
        }
        let Some(source_node) = nodes_by_id.get(link.source.as_str()).copied() else {
            continue;
        };
        let Some(target_node) = nodes_by_id.get(link.target.as_str()).copied() else {
            continue;
        };
        let dx = target.x - source.x;
        let dy = target.y - source.y;
        let distance = dx.hypot(dy);
        if distance == 0.0 || !distance.is_finite() {
            return Err(Error::InvalidModel {
                message: format!(
                    "wardley link `{}` -> `{}` has coincident or non-finite projected endpoints",
                    link.source, link.target
                ),
            });
        }
        let source_radius = if source_node.is_pipeline_parent {
            square_size / 2.0_f64.sqrt()
        } else {
            settings.node_radius
        };
        let target_radius = if target_node.is_pipeline_parent {
            square_size / 2.0_f64.sqrt()
        } else {
            settings.node_radius
        };
        let line = WardleyLineLayout {
            x1: source.x + dx / distance * source_radius,
            y1: source.y + dy / distance * source_radius,
            x2: target.x - dx / distance * target_radius,
            y2: target.y - dy / distance * target_radius,
        };
        let markers = match link.flow {
            Some(WardleyFlowDirection::Forward) => WardleyFlowMarkersLayout {
                start: false,
                end: true,
            },
            Some(WardleyFlowDirection::Backward) => WardleyFlowMarkersLayout {
                start: true,
                end: false,
            },
            Some(WardleyFlowDirection::Bidirectional) => WardleyFlowMarkersLayout {
                start: true,
                end: true,
            },
            None => WardleyFlowMarkersLayout {
                start: false,
                end: false,
            },
        };
        let label = link
            .label
            .as_deref()
            .filter(|label| !label.is_empty())
            .map(|label| {
                let x = (source.x + target.x) / 2.0 + dy / distance * 8.0;
                let y = (source.y + target.y) / 2.0 - dx / distance * 8.0;
                let mut angle = dy.atan2(dx).to_degrees();
                if !(-90.0..=90.0).contains(&angle) {
                    angle += 180.0;
                }
                let mut layout = text_layout(
                    label,
                    x,
                    y,
                    settings.label_font_size,
                    WardleyFontWeight::Normal,
                    WardleyTextAnchor::Middle,
                    Some(WardleyDominantBaseline::Middle),
                );
                layout.rotation = Some(WardleyRotationLayout {
                    degrees: angle,
                    cx: x,
                    cy: y,
                });
                layout
            });
        links.push(WardleyLinkLayout {
            source: link.source.clone(),
            target: link.target.clone(),
            line,
            dashed: link.dashed,
            markers,
            label,
        });
    }

    let trends = model
        .trends
        .iter()
        .filter_map(|trend| {
            let origin = positions.get(trend.node_id.as_str()).copied()?;
            let target = project_point(trend.target_x, trend.target_y);
            let dx = target.x - origin.x;
            let dy = target.y - origin.y;
            let distance = dx.hypot(dy);
            let shorten_by = settings.node_radius + 2.0;
            let end = if distance > shorten_by {
                WardleyPointLayout {
                    x: target.x - dx / distance * shorten_by,
                    y: target.y - dy / distance * shorten_by,
                }
            } else {
                target
            };
            Some(WardleyTrendLayout {
                node_id: trend.node_id.clone(),
                origin,
                target,
                line: WardleyLineLayout::between(origin, end),
            })
        })
        .collect();

    let nodes = model
        .nodes
        .iter()
        .filter_map(|node| {
            let position = positions.get(node.id.as_str()).copied()?;
            let is_anchor = node.class_name.as_deref() == Some("anchor");
            let shape = if node.is_pipeline_parent {
                WardleyNodeShapeLayout::PipelineSquare {
                    rect: WardleyRectLayout {
                        x: position.x - square_size / 2.0,
                        y: position.y - square_size / 2.0,
                        width: square_size,
                        height: square_size,
                        corner_radius: 0.0,
                    },
                }
            } else if is_anchor {
                WardleyNodeShapeLayout::Anchor
            } else if node.source_strategy == Some(WardleySourceStrategy::Market) {
                WardleyNodeShapeLayout::None
            } else {
                WardleyNodeShapeLayout::Circle {
                    circle: WardleyCircleLayout {
                        center: position,
                        radius: settings.node_radius,
                    },
                }
            };

            let (label_x, label_y, weight, anchor, baseline) = if is_anchor {
                (
                    position.x + node.label_offset_x.unwrap_or(0) as f64,
                    position.y + node.label_offset_y.map_or(-3.0, |offset| offset as f64),
                    WardleyFontWeight::Bold,
                    WardleyTextAnchor::Middle,
                    Some(WardleyDominantBaseline::Middle),
                )
            } else {
                let mut default_x = settings.node_label_offset;
                let mut default_y = -settings.node_label_offset;
                if node.source_strategy.is_some() {
                    if node.label_offset_x.is_none() {
                        default_x += 10.0;
                    }
                    if node.label_offset_y.is_none() {
                        default_y -= 10.0;
                    }
                }
                (
                    position.x
                        + node
                            .label_offset_x
                            .map_or(default_x, |offset| offset as f64),
                    position.y
                        + node
                            .label_offset_y
                            .map_or(default_y, |offset| offset as f64),
                    WardleyFontWeight::Normal,
                    WardleyTextAnchor::Start,
                    Some(WardleyDominantBaseline::Auto),
                )
            };
            let inertia = (node.inertia == Some(true)).then(|| {
                let mut offset = if node.is_pipeline_parent {
                    square_size / 2.0 + 15.0
                } else {
                    settings.node_radius + 15.0
                };
                if node.source_strategy.is_some() {
                    offset += settings.node_radius + 10.0;
                }
                let line_height = if node.is_pipeline_parent {
                    square_size
                } else {
                    settings.node_radius * 2.0
                };
                WardleyLineLayout {
                    x1: position.x + offset,
                    y1: position.y - line_height / 2.0,
                    x2: position.x + offset,
                    y2: position.y + line_height / 2.0,
                }
            });
            Some(WardleyNodeLayout {
                id: node.id.clone(),
                label: node.label.clone(),
                class_name: node.class_name.clone(),
                position,
                in_pipeline: node.in_pipeline,
                is_pipeline_parent: node.is_pipeline_parent,
                source_strategy: node.source_strategy,
                shape,
                source_overlay: source_overlay(
                    node.source_strategy,
                    position,
                    settings.node_radius,
                ),
                inertia,
                label_layout: text_layout(
                    &node.label,
                    label_x,
                    label_y,
                    settings.label_font_size,
                    weight,
                    anchor,
                    baseline,
                ),
            })
        })
        .collect();

    let annotations = model
        .annotations
        .iter()
        .map(|annotation| {
            let projected: Vec<_> = annotation
                .coordinates
                .iter()
                .map(|coordinate| project_point(coordinate.x, coordinate.y))
                .collect();
            let segments = projected
                .windows(2)
                .map(|pair| WardleyLineLayout::between(pair[0], pair[1]))
                .collect();
            let points = projected
                .into_iter()
                .map(|center| WardleyAnnotationPointLayout {
                    center,
                    radius: 10.0,
                    label: text_layout(
                        annotation.number.to_string(),
                        center.x,
                        center.y,
                        10.0,
                        WardleyFontWeight::Bold,
                        WardleyTextAnchor::Middle,
                        Some(WardleyDominantBaseline::Central),
                    ),
                })
                .collect();
            WardleyAnnotationLayout {
                number: annotation.number,
                points,
                segments,
            }
        })
        .collect();

    let annotations_box = if model.annotations.is_empty() {
        None
    } else {
        model.annotations_box.map(|position| {
            let padding = 10.0;
            let line_height = 16.0;
            let font_size = 11.0;
            let mut sorted_annotations: Vec<_> = model
                .annotations
                .iter()
                // JavaScript's `filter((a) => a.text)` also excludes the empty string.
                .filter(|annotation| {
                    annotation
                        .text
                        .as_deref()
                        .is_some_and(|text| !text.is_empty())
                })
                .collect();
            sorted_annotations.sort_by_key(|annotation| annotation.number);

            if sorted_annotations.is_empty() {
                return WardleyAnnotationsBoxLayout {
                    rect: None,
                    lines: Vec::new(),
                    padding,
                    line_height,
                    font_size,
                    max_text_width: 0.0,
                    max_text_height: 0.0,
                };
            }

            let font_family = config_font_family_css(effective_config);
            let style = TextStyle {
                font_family: Some(font_family),
                font_size,
                font_weight: None,
                font_style: None,
            };
            let line_texts: Vec<String> = sorted_annotations
                .iter()
                .map(|annotation| {
                    format!(
                        "{}. {}",
                        annotation.number,
                        annotation.text.as_deref().unwrap_or_default()
                    )
                })
                .collect();
            let mut max_text_width: f64 = 0.0;
            let mut max_text_height: f64 = 0.0;
            for text in &line_texts {
                max_text_width =
                    max_text_width.max(measurer.measure_svg_text_computed_length_px(text, &style));
                max_text_height =
                    max_text_height.max(measurer.measure_svg_raw_text_bbox_height_px(text, &style));
            }

            // `+ 105` is pinned Mermaid 11.16 renderer behavior, not a local sizing heuristic.
            let box_width = max_text_width + padding * 2.0 + 105.0;
            let box_height =
                line_texts.len() as f64 * line_height + padding * 2.0 + max_text_height / 2.0;
            let projected = project_point(position.x, position.y);
            let min_x = settings.padding;
            let max_x = width - settings.padding - box_width;
            let min_y = settings.padding;
            let max_y = height - settings.padding - box_height;
            let box_x = projected.x.min(max_x).max(min_x);
            let box_y = projected.y.min(max_y).max(min_y);
            let lines = line_texts
                .into_iter()
                .enumerate()
                .map(|(index, text)| {
                    text_layout(
                        text,
                        box_x + padding,
                        box_y + padding + (index + 1) as f64 * line_height,
                        font_size,
                        WardleyFontWeight::Normal,
                        WardleyTextAnchor::Start,
                        Some(WardleyDominantBaseline::Middle),
                    )
                })
                .collect();
            WardleyAnnotationsBoxLayout {
                rect: Some(WardleyRectLayout {
                    x: box_x,
                    y: box_y,
                    width: box_width,
                    height: box_height,
                    corner_radius: 4.0,
                }),
                lines,
                padding,
                line_height,
                font_size,
                max_text_width,
                max_text_height,
            }
        })
    };

    let notes = model
        .notes
        .iter()
        .map(|note| {
            let position = project_point(note.x, note.y);
            WardleyNoteLayout {
                text: text_layout(
                    &note.text,
                    position.x,
                    position.y,
                    11.0,
                    WardleyFontWeight::Bold,
                    WardleyTextAnchor::Start,
                    None,
                ),
            }
        })
        .collect();
    let accelerators = model
        .accelerators
        .iter()
        .map(|accelerator| {
            build_arrow(
                &accelerator.name,
                project_point(accelerator.x, accelerator.y),
                WardleyArrowDirection::Right,
            )
        })
        .collect();
    let deaccelerators = model
        .deaccelerators
        .iter()
        .map(|deaccelerator| {
            build_arrow(
                &deaccelerator.name,
                project_point(deaccelerator.x, deaccelerator.y),
                WardleyArrowDirection::Left,
            )
        })
        .collect();

    Ok(WardleyDiagramLayout {
        width,
        height,
        use_max_width: settings.use_max_width,
        padding: settings.padding,
        chart_width,
        chart_height,
        node_radius: settings.node_radius,
        node_label_offset: settings.node_label_offset,
        axis_font_size: settings.axis_font_size,
        label_font_size: settings.label_font_size,
        square_size,
        title,
        axes,
        stages,
        grid,
        pipeline_boxes,
        pipeline_links,
        links,
        trends,
        nodes,
        annotations,
        annotations_box,
        notes,
        accelerators,
        deaccelerators,
    })
}

#[cfg(all(test, feature = "diagram-wardley"))]
#[path = "wardley_layout_tests.rs"]
mod wardley_layout_tests;

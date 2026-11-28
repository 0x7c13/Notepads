use crate::Result;
use crate::model::{
    Bounds, JourneyActorLegendItemLayout, JourneyActorLegendLineLayout, JourneyDiagramLayout,
    JourneyLineLayout, JourneyMouthKind, JourneySectionLayout, JourneyTaskActorCircleLayout,
    JourneyTaskLayout,
};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::journey::{JourneyDiagramRenderModel, JourneyRenderTask};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod config;

pub(crate) use config::{JourneyConfigView, default_use_max_width};

const JOURNEY_LEGEND_CIRCLE_R_PX: f64 = 7.0;
pub(crate) const JOURNEY_VIEWBOX_TOP_PAD_PX: f64 = 25.0;
pub(crate) const JOURNEY_TITLE_EXTRA_HEIGHT_PX: f64 = 70.0;
pub(crate) const JOURNEY_FACE_RADIUS_PX: f64 = 15.0;
const JOURNEY_FACE_BASE_Y_PX: f64 = 300.0;
const JOURNEY_FACE_SCORE_STEP_Y_PX: f64 = 30.0;

fn actors_from_tasks(tasks: &[JourneyRenderTask]) -> Vec<String> {
    let mut set = BTreeSet::<String>::new();
    for t in tasks {
        for p in &t.people {
            set.insert(p.to_string());
        }
    }
    set.into_iter().collect()
}

fn wrap_actor_label_lines(
    person: &str,
    max_label_width: f64,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
) -> Vec<String> {
    let max_label_width = max_label_width.max(1.0);
    let full_text_width =
        journey_actor_legend_text_bounding_client_rect_width_px(person, measurer, style);
    if full_text_width <= max_label_width {
        return vec![person.to_string()];
    }

    let words = person.split(' ').collect::<Vec<_>>();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in words {
        let test_line = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{current_line} {word}")
        };

        let test_width =
            journey_actor_legend_text_bounding_client_rect_width_px(&test_line, measurer, style);
        if test_width > max_label_width {
            if !current_line.is_empty() {
                lines.push(std::mem::take(&mut current_line));
            }
            current_line = word.to_string();

            let word_width =
                journey_actor_legend_text_bounding_client_rect_width_px(word, measurer, style);
            if word_width > max_label_width {
                let mut broken_word = String::new();
                for ch in word.chars() {
                    broken_word.push(ch);
                    let candidate = format!("{broken_word}-");
                    let candidate_width = journey_actor_legend_text_bounding_client_rect_width_px(
                        &candidate, measurer, style,
                    );
                    if candidate_width > max_label_width {
                        let mut head = broken_word.clone();
                        head.pop();
                        if !head.is_empty() {
                            lines.push(format!("{head}-"));
                        }
                        broken_word = ch.to_string();
                    }
                }
                current_line = broken_word;
            }
        } else {
            current_line = test_line;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        vec![person.to_string()]
    } else {
        lines
    }
}

fn journey_actor_legend_text_style(effective_config: &Value) -> TextStyle {
    TextStyle {
        font_family: Some(crate::config::config_font_family_css(effective_config)),
        font_size: crate::config::config_theme_font_size_css_or_root_number_px(
            effective_config,
            16.0,
        )
        .max(1.0),
        font_weight: None,
        font_style: None,
    }
}

fn journey_actor_legend_text_bounding_client_rect_width_px(
    line: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
) -> f64 {
    measurer
        .measure_svg_text_bounding_client_rect_width_px(line, style)
        .max(0.0)
}

fn journey_actor_legend_line_width_px(
    line: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
) -> f64 {
    let width = journey_actor_legend_text_bounding_client_rect_width_px(line, measurer, style);
    if width.is_finite() && width > 0.0 {
        width
    } else {
        0.0
    }
}

pub(crate) fn layout_journey_diagram_typed(
    model: &JourneyDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<JourneyDiagramLayout> {
    let _ = (
        model.acc_title.as_deref(),
        model.acc_descr.as_deref(),
        model.sections.as_slice(),
    );

    let cfg = JourneyConfigView::new(effective_config).layout_settings();

    let actors = if model.actors.is_empty() {
        actors_from_tasks(&model.tasks)
    } else {
        model.actors.clone()
    };

    let mut actor_map: BTreeMap<String, (i64, String)> = BTreeMap::new();
    for (i, actor) in actors.iter().enumerate() {
        let pos = i as i64;
        let color = cfg
            .actor_colours
            .get(i % cfg.actor_colours.len().max(1))
            .cloned()
            .unwrap_or_else(|| "#8FBC8F".to_string());
        actor_map.insert(actor.clone(), (pos, color));
    }

    let legend_style = journey_actor_legend_text_style(effective_config);
    let mut max_actor_label_width: f64 = 0.0;
    let mut actor_legend: Vec<JourneyActorLegendItemLayout> = Vec::new();

    let legend_circle_r = JOURNEY_LEGEND_CIRCLE_R_PX;
    let legend_line_step_y = 20.0;
    let mut y_pos = 60.0;
    for actor in actors.iter() {
        let (pos, color) = actor_map
            .get(actor)
            .cloned()
            .unwrap_or((0_i64, "#8FBC8F".to_string()));

        let lines = wrap_actor_label_lines(actor, cfg.max_label_width, measurer, &legend_style);
        let mut label_lines: Vec<JourneyActorLegendLineLayout> = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let x = 40.0;
            let y = y_pos + legend_circle_r + (index as f64) * legend_line_step_y;
            let tspan_x = x + cfg.box_text_margin * 2.0;
            label_lines.push(JourneyActorLegendLineLayout {
                text: line.to_string(),
                x,
                y,
                tspan_x,
                text_margin: cfg.box_text_margin,
            });

            let line_width = journey_actor_legend_line_width_px(line, measurer, &legend_style);
            if line_width > max_actor_label_width && line_width > cfg.left_margin_base - line_width
            {
                max_actor_label_width = line_width;
            }
        }

        actor_legend.push(JourneyActorLegendItemLayout {
            actor: actor.to_string(),
            pos,
            color,
            circle_cx: 20.0,
            circle_cy: y_pos,
            circle_r: legend_circle_r,
            label_lines,
        });

        y_pos += legend_line_step_y * (lines.len().max(1) as f64);
    }

    let left_margin = cfg.left_margin_base + max_actor_label_width;
    let section_v_height = cfg.cell_height * 2.0 + cfg.diagram_margin_y;
    let task_y = section_v_height;

    let mut sections: Vec<JourneySectionLayout> = Vec::new();
    let mut tasks: Vec<JourneyTaskLayout> = Vec::new();

    let mut last_section = String::new();
    let mut section_number: i64 = 0;
    let mut current_fill = "#CCC".to_string();
    let mut current_num: i64 = 0;

    let mut stopx = left_margin;
    for (i, task) in model.tasks.iter().enumerate() {
        let x = (i as f64) * cfg.task_margin + (i as f64) * cfg.cell_width + left_margin;
        let is_new_section = last_section != task.section;

        if is_new_section {
            let fills_len = cfg.section_fills.len().max(1) as i64;
            current_num = section_number % fills_len;
            current_fill = cfg
                .section_fills
                .get(current_num as usize)
                .cloned()
                .unwrap_or_else(|| "#CCC".to_string());

            let mut count: i64 = 0;
            for t in model.tasks.iter().skip(i) {
                if t.section == task.section {
                    count += 1;
                } else {
                    break;
                }
            }
            let section_width =
                cfg.cell_width * (count as f64) + cfg.diagram_margin_x * ((count - 1) as f64);

            sections.push(JourneySectionLayout {
                section: task.section.to_string(),
                num: current_num,
                x,
                y: 50.0,
                width: section_width.max(1.0),
                height: cfg.cell_height,
                fill: current_fill.clone(),
                task_count: count,
            });

            last_section = task.section.to_string();
            section_number += 1;
        }

        let center_x = x + cfg.cell_width / 2.0;
        let max_height = JOURNEY_FACE_BASE_Y_PX + 5.0 * JOURNEY_FACE_SCORE_STEP_Y_PX;
        let face_cy = if task.score_is_nan {
            None
        } else {
            Some(
                JOURNEY_FACE_BASE_Y_PX
                    + (5_i64.saturating_sub(task.score) as f64) * JOURNEY_FACE_SCORE_STEP_Y_PX,
            )
        };
        let mouth = if task.score_is_nan {
            JourneyMouthKind::Ambivalent
        } else if task.score > 3 {
            JourneyMouthKind::Smile
        } else if task.score < 3 {
            JourneyMouthKind::Sad
        } else {
            JourneyMouthKind::Ambivalent
        };

        let mut actor_circles = Vec::new();
        let mut cx = x + 14.0;
        for p in &task.people {
            let Some((pos, color)) = actor_map.get(p).cloned() else {
                continue;
            };
            actor_circles.push(JourneyTaskActorCircleLayout {
                actor: p.to_string(),
                pos,
                color,
                cx,
                cy: task_y,
                r: JOURNEY_LEGEND_CIRCLE_R_PX,
            });
            cx += 10.0;
        }

        let line_id = format!("task{i}");
        tasks.push(JourneyTaskLayout {
            index: i as i64,
            section: task.section.to_string(),
            task: task.task.to_string(),
            score: task.score,
            x,
            y: task_y,
            width: cfg.cell_width,
            height: cfg.cell_height,
            fill: current_fill.clone(),
            num: current_num,
            people: task.people.clone(),
            actor_circles,
            line_id,
            line_x1: center_x,
            line_y1: task_y,
            line_x2: center_x,
            line_y2: max_height,
            face_cx: center_x,
            face_cy,
            mouth,
        });

        stopx = stopx.max(x + cfg.diagram_margin_x + cfg.task_margin);
    }

    let stopy = (actors.len() as f64 * 50.0).max(if tasks.is_empty() {
        0.0
    } else {
        JOURNEY_FACE_BASE_Y_PX + 5.0 * JOURNEY_FACE_SCORE_STEP_Y_PX
    });

    let height = (stopy - 0.0 + 2.0 * cfg.diagram_margin_y).max(1.0);
    let width = (left_margin + stopx + 2.0 * cfg.diagram_margin_x).max(1.0);

    let title = model
        .title
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let extra_vert_for_title = if title.is_some() {
        JOURNEY_TITLE_EXTRA_HEIGHT_PX
    } else {
        0.0
    };

    let viewbox_top_pad = JOURNEY_VIEWBOX_TOP_PAD_PX;
    let bounds = Bounds {
        min_x: 0.0,
        min_y: -viewbox_top_pad,
        max_x: width,
        max_y: -viewbox_top_pad + height + extra_vert_for_title,
    };

    let svg_height = height + extra_vert_for_title + viewbox_top_pad;

    let activity_line = JourneyLineLayout {
        x1: left_margin,
        y1: cfg.cell_height * 4.0,
        x2: width - left_margin - 4.0,
        y2: cfg.cell_height * 4.0,
    };

    Ok(JourneyDiagramLayout {
        bounds: Some(bounds),
        left_margin,
        max_actor_label_width,
        width,
        height,
        svg_height,
        use_max_width: cfg.use_max_width,
        title,
        title_x: left_margin,
        title_y: viewbox_top_pad,
        actor_legend,
        sections,
        tasks,
        activity_line,
    })
}

#[cfg(test)]
mod tests {
    use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle};
    use merman_core::diagrams::journey::JourneyDiagramRenderModel;
    use serde_json::json;

    #[test]
    fn journey_layout_carries_use_max_width_config() {
        let model = JourneyDiagramRenderModel::default();
        let measurer = DeterministicTextMeasurer::default();
        let layout = super::layout_journey_diagram_typed(
            &model,
            &json!({"journey": {"useMaxWidth": false}}),
            &measurer,
        )
        .expect("layout");

        assert!(!layout.use_max_width);
    }

    #[test]
    fn journey_geometry_constants_match_mermaid() {
        assert_eq!(super::JOURNEY_VIEWBOX_TOP_PAD_PX, 25.0);
        assert_eq!(super::JOURNEY_TITLE_EXTRA_HEIGHT_PX, 70.0);
        assert_eq!(super::JOURNEY_LEGEND_CIRCLE_R_PX, 7.0);
        assert_eq!(super::JOURNEY_FACE_RADIUS_PX, 15.0);
        assert_eq!(super::JOURNEY_FACE_BASE_Y_PX, 300.0);
        assert_eq!(super::JOURNEY_FACE_SCORE_STEP_Y_PX, 30.0);
    }

    #[test]
    fn journey_actor_legend_width_preserves_profile_bounding_client_rect_result() {
        let measurer = DeterministicTextMeasurer::default();
        let style = super::journey_actor_legend_text_style(&json!({}));

        for line in [
            "Giancarlo Esposito and is a",
            "split into multiple lines to test the wrapping",
        ] {
            assert_eq!(
                super::journey_actor_legend_line_width_px(line, &measurer, &style),
                measurer.measure_svg_text_bounding_client_rect_width_px(line, &style)
            );
        }
    }

    struct BoundingClientRectMeasurer;

    impl TextMeasurer for BoundingClientRectMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            panic!("journey actor labels must use the source-backed browser primitive")
        }

        fn measure_svg_raw_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            panic!("getBBox must not stand in for getBoundingClientRect")
        }

        fn measure_svg_text_bounding_client_rect_width_px(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> f64 {
            123.456_789
        }
    }

    #[test]
    fn journey_actor_legend_width_uses_exact_bounding_client_rect_result() {
        let style = super::journey_actor_legend_text_style(&json!({}));

        assert_eq!(
            super::journey_actor_legend_line_width_px("actor", &BoundingClientRectMeasurer, &style,),
            123.456_789
        );
    }

    #[test]
    fn journey_actor_legend_wraps_with_the_configured_width() {
        let measurer = DeterministicTextMeasurer::default();
        let style = super::journey_actor_legend_text_style(&json!({}));

        let lines = super::wrap_actor_label_lines(
            "This is a long label that will be split into multiple lines to test the wrapping functionality",
            320.0,
            &measurer,
            &style,
        );

        assert!(lines.len() > 1, "{lines:?}");
        assert_eq!(
            lines.join(" "),
            "This is a long label that will be split into multiple lines to test the wrapping functionality"
        );
        assert!(
            lines.iter().all(|line| {
                super::journey_actor_legend_line_width_px(line, &measurer, &style) <= 320.0
            }),
            "{lines:?}"
        );
    }
}

use super::*;
use merman_core::theme_color::{darken, invert, is_dark, lighten};

impl<'a> PresentationTheme<'a> {
    #[cfg(feature = "diagram-xychart")]
    pub(crate) fn xychart(&self) -> XyChartTheme {
        let background = self
            .raw
            .optional_nested_color("xyChart", "backgroundColor")
            .or_else(|| self.raw.optional_color("background"))
            .unwrap_or_else(|| "white".to_string());
        let primary_text = self
            .raw
            .optional_color("primaryTextColor")
            .unwrap_or_else(|| "#131300".to_string());

        XyChartTheme {
            background_color: background.clone(),
            title_color: self
                .raw
                .optional_nested_color("xyChart", "titleColor")
                .unwrap_or_else(|| primary_text.clone()),
            legend_text_color: self
                .raw
                .optional_nested_color("xyChart", "legendTextColor")
                .unwrap_or_else(|| primary_text.clone()),
            x_axis_title_color: self
                .raw
                .optional_nested_color("xyChart", "xAxisTitleColor")
                .unwrap_or_else(|| primary_text.clone()),
            x_axis_label_color: self
                .raw
                .optional_nested_color("xyChart", "xAxisLabelColor")
                .unwrap_or_else(|| primary_text.clone()),
            x_axis_tick_color: self
                .raw
                .optional_nested_color("xyChart", "xAxisTickColor")
                .unwrap_or_else(|| primary_text.clone()),
            x_axis_line_color: self
                .raw
                .optional_nested_color("xyChart", "xAxisLineColor")
                .unwrap_or_else(|| primary_text.clone()),
            y_axis_title_color: self
                .raw
                .optional_nested_color("xyChart", "yAxisTitleColor")
                .unwrap_or_else(|| primary_text.clone()),
            y_axis_label_color: self
                .raw
                .optional_nested_color("xyChart", "yAxisLabelColor")
                .unwrap_or_else(|| primary_text.clone()),
            y_axis_tick_color: self
                .raw
                .optional_nested_color("xyChart", "yAxisTickColor")
                .unwrap_or_else(|| primary_text.clone()),
            y_axis_line_color: self
                .raw
                .optional_nested_color("xyChart", "yAxisLineColor")
                .unwrap_or_else(|| primary_text.clone()),
            plot_color_palette: resolve_xychart_plot_palette(
                &self.common.theme_name,
                self.raw
                    .optional_nested_color("xyChart", "plotColorPalette")
                    .as_deref(),
            ),
        }
    }

    #[cfg(feature = "diagram-quadrant-chart")]
    pub(crate) fn quadrantchart(&self) -> QuadrantChartTheme {
        let value = |key: &str, fallback: &str| self.raw.color(key, fallback);
        let primary_text = self.raw.color("primaryTextColor", "#131300");
        QuadrantChartTheme {
            quadrant1_fill: value("quadrant1Fill", "#ECECFF"),
            quadrant2_fill: value("quadrant2Fill", "#f1f1ff"),
            quadrant3_fill: value("quadrant3Fill", "#f6f6ff"),
            quadrant4_fill: value("quadrant4Fill", "#fbfbff"),
            quadrant1_text_fill: value("quadrant1TextFill", &primary_text),
            quadrant2_text_fill: value("quadrant2TextFill", "#0e0e00"),
            quadrant3_text_fill: value("quadrant3TextFill", "#090900"),
            quadrant4_text_fill: value("quadrant4TextFill", "#040400"),
            quadrant_point_fill: value("quadrantPointFill", "hsl(240, 100%, NaN%)"),
            quadrant_point_text_fill: value("quadrantPointTextFill", &primary_text),
            quadrant_x_axis_text_fill: value("quadrantXAxisTextFill", &primary_text),
            quadrant_y_axis_text_fill: value("quadrantYAxisTextFill", &primary_text),
            quadrant_title_fill: value("quadrantTitleFill", &primary_text),
            quadrant_internal_border_stroke_fill: value(
                "quadrantInternalBorderStrokeFill",
                "hsl(240, 60%, 86.2745098039%)",
            ),
            quadrant_external_border_stroke_fill: value(
                "quadrantExternalBorderStrokeFill",
                "hsl(240, 60%, 86.2745098039%)",
            ),
        }
    }

    #[cfg(feature = "diagram-tree-view")]
    pub(crate) fn tree_view(&self) -> TreeViewTheme {
        TreeViewTheme {
            label_font_size: self
                .raw
                .optional_nested_css_px("treeView", "labelFontSize")
                .unwrap_or(16.0)
                .max(1.0),
            label_font_size_css: self
                .raw
                .optional_nested_css_value("treeView", "labelFontSize")
                .unwrap_or_else(|| "16px".to_string()),
            label_color: self
                .raw
                .optional_nested_color("treeView", "labelColor")
                .unwrap_or_else(|| "black".to_string()),
            line_color: self
                .raw
                .optional_nested_color("treeView", "lineColor")
                .unwrap_or_else(|| "black".to_string()),
            icon_color: self
                .raw
                .optional_nested_color("treeView", "iconColor")
                .unwrap_or_else(|| "#546e7a".to_string()),
            description_color: self
                .raw
                .optional_nested_color("treeView", "descriptionColor")
                .unwrap_or_else(|| "#6a9955".to_string()),
            highlight_bg: self
                .raw
                .optional_nested_color("treeView", "highlightBg")
                .unwrap_or_else(|| "rgba(255, 193, 7, 0.15)".to_string()),
            highlight_stroke: self
                .raw
                .optional_nested_color("treeView", "highlightStroke")
                .unwrap_or_else(|| "#ffc107".to_string()),
        }
    }

    #[cfg(feature = "diagram-treemap")]
    pub(crate) fn treemap(&self) -> crate::Result<TreemapTheme> {
        let text_color = self.raw.color("textColor", "#333");
        let title_color = self
            .raw
            .optional_root_scoped_string("treemap", "titleColor")
            .or_else(|| self.raw.optional_color("titleColor"))
            .unwrap_or_else(|| text_color.clone());
        let raw_theme_name = self.common.theme_name.clone();
        let default_theme = raw_theme_name == "default";
        let theme_name = raw_theme_name.trim().to_ascii_lowercase();
        let label_text_color = self.raw.color("labelTextColor", "black");
        let label_text_is_calculated = label_text_color.trim() == "calculated";
        let scale_label_color = self.raw.color("scaleLabelColor", &label_text_color);
        let neutral_special_label_color = self.raw.color("cScale1", default_c_scale(1));

        let color_scale = (0..12)
            .map(|i| {
                if default_theme {
                    default_c_scale(i).to_string()
                } else {
                    self.raw.color(&format!("cScale{i}"), default_c_scale(i))
                }
            })
            .collect();
        let color_scale_peer = (0..12)
            .map(|i| {
                if default_theme {
                    default_c_scale_peer(i).to_string()
                } else {
                    self.raw
                        .color(&format!("cScalePeer{i}"), default_c_scale_peer(i))
                }
            })
            .collect();
        let color_scale_label = (0..12)
            .map(|i| -> crate::Result<String> {
                if let Some(color) = self.raw.optional_color(&format!("cScaleLabel{i}")) {
                    return Ok(color);
                }
                Ok(match theme_name.as_str() {
                    "dark" | "forest" => scale_label_color.clone(),
                    "neutral" => {
                        if i == 0 || i == 2 {
                            neutral_special_label_color.clone()
                        } else {
                            scale_label_color.clone()
                        }
                    }
                    _ => {
                        if label_text_is_calculated {
                            scale_label_color.clone()
                        } else if i == 0 || i == 3 {
                            invert(&label_text_color)?
                        } else {
                            label_text_color.clone()
                        }
                    }
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(TreemapTheme {
            title_color,
            label_color: self
                .raw
                .optional_root_scoped_string("treemap", "labelColor")
                .unwrap_or_else(|| text_color.clone()),
            value_color: self
                .raw
                .optional_root_scoped_string("treemap", "valueColor")
                .unwrap_or_else(|| text_color.clone()),
            section_stroke_color: self.treemap_style_option("sectionStrokeColor", "black"),
            section_stroke_width: self.treemap_style_option("sectionStrokeWidth", "1"),
            section_fill_color: self.treemap_style_option("sectionFillColor", "#efefef"),
            leaf_stroke_color: self.treemap_style_option("leafStrokeColor", "black"),
            leaf_stroke_width: self.treemap_style_option("leafStrokeWidth", "1"),
            leaf_fill_color: self.treemap_style_option("leafFillColor", "#efefef"),
            label_font_size: self.treemap_style_option("labelFontSize", "12px"),
            value_font_size: self.treemap_style_option("valueFontSize", "10px"),
            title_font_size: self.treemap_style_option("titleFontSize", "14px"),
            color_scale,
            color_scale_peer,
            color_scale_label,
            text_color,
        })
    }

    #[cfg(feature = "diagram-gantt")]
    pub(crate) fn gantt(&self) -> GanttTheme {
        let option = |key: &str, default_value: &str| -> String {
            self.raw
                .optional_color(key)
                .unwrap_or_else(|| default_value.to_string())
        };

        let text_color = self.common.text_color.clone();
        let title_color = option("titleColor", "#333");
        let title_text_color = if title_color.trim().is_empty() {
            text_color.clone()
        } else {
            title_color.clone()
        };

        GanttTheme {
            font_family: self.common.font_family_css.clone(),
            text_color,
            exclude_bkg_color: option("excludeBkgColor", "#eeeeee"),
            section_bkg_color: option("sectionBkgColor", "rgba(102, 102, 255, 0.49)"),
            section_bkg_color2: option("sectionBkgColor2", "#fff400"),
            alt_section_bkg_color: option("altSectionBkgColor", "white"),
            title_color,
            title_text_color,
            grid_color: option("gridColor", "lightgrey"),
            today_line_color: option("todayLineColor", "red"),
            task_text_dark_color: option("taskTextDarkColor", "black"),
            task_text_clickable_color: option("taskTextClickableColor", "#003163"),
            task_text_color: option("taskTextColor", "white"),
            task_bkg_color: option("taskBkgColor", "#8a90dd"),
            task_border_color: option("taskBorderColor", "#534fbc"),
            task_text_outside_color: option("taskTextOutsideColor", "black"),
            active_task_bkg_color: option("activeTaskBkgColor", "#bfc7ff"),
            active_task_border_color: option("activeTaskBorderColor", "#534fbc"),
            done_task_border_color: option("doneTaskBorderColor", "grey"),
            done_task_bkg_color: option("doneTaskBkgColor", "lightgrey"),
            crit_border_color: option("critBorderColor", "#ff8888"),
            crit_bkg_color: option("critBkgColor", "red"),
            vert_line_color: option("vertLineColor", "navy"),
        }
    }

    #[cfg(feature = "diagram-kanban")]
    pub(crate) fn kanban(&self) -> crate::Result<KanbanTheme> {
        let dark_mode = self.raw.bool_root_or_theme("darkMode").unwrap_or(false);
        let sections = (0..12)
            .map(|i| {
                let c_scale = self.raw.color(&format!("cScale{i}"), default_c_scale(i));
                let section_fill = if dark_mode {
                    darken(&c_scale, 10.0)?
                } else {
                    lighten(&c_scale, 10.0)?
                };
                Ok(KanbanSectionTheme {
                    section_fill,
                    c_scale,
                    c_scale_label: self
                        .raw
                        .color(&format!("cScaleLabel{i}"), default_c_scale_label(i)),
                    c_scale_inv: self
                        .raw
                        .color(&format!("cScaleInv{i}"), default_c_scale_inv(i)),
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(KanbanTheme {
            text_color: self.common.text_color.clone(),
            background: self.raw.color("background", "white"),
            node_border: self.raw.color("nodeBorder", "#9370DB"),
            root_fill: self.raw.color("git0", "hsl(240, 100%, 46.2745098039%)"),
            root_label: self.raw.color("gitBranchLabel0", "#ffffff"),
            sections,
        })
    }

    #[cfg(feature = "diagram-event-modeling")]
    pub(crate) fn eventmodeling(&self) -> EventModelingTheme {
        EventModelingTheme {
            font_family_css: self.raw.font_family_css_root_first(),
            text_color: self.raw.color("textColor", "#333"),
            ui_fill: self.raw.color("emUiFill", "white"),
            ui_stroke: self.raw.color("emUiStroke", "#dbdada"),
            processor_fill: self.raw.color("emProcessorFill", "#edb3f6"),
            processor_stroke: self.raw.color("emProcessorStroke", "#b88cbf"),
            read_model_fill: self.raw.color("emReadModelFill", "#d3f1a2"),
            read_model_stroke: self.raw.color("emReadModelStroke", "#a3b732"),
            command_fill: self.raw.color("emCommandFill", "#bcd6fe"),
            command_stroke: self.raw.color("emCommandStroke", "#679ac3"),
            event_fill: self.raw.color("emEventFill", "#ffb778"),
            event_stroke: self.raw.color("emEventStroke", "#c19a0f"),
            swimlane_background_fill: self
                .raw
                .optional_color("emSwimlaneBackgroundOdd")
                .or_else(|| self.raw.optional_color("emSwimlaneBackground"))
                .unwrap_or_else(|| "rgb(250,250,250)".to_string()),
            swimlane_background_stroke: self
                .raw
                .optional_color("emSwimlaneBackgroundStroke")
                .or_else(|| self.raw.optional_color("emSwimlaneBorder"))
                .unwrap_or_else(|| "rgb(240,240,240)".to_string()),
            relation_stroke: self.raw.color("emRelationStroke", "#000"),
            arrowhead_fill: self.raw.color("emArrowhead", "#000000"),
        }
    }

    #[cfg(feature = "diagram-ishikawa")]
    pub(crate) fn ishikawa(&self) -> IshikawaTheme {
        IshikawaTheme {
            line_color: self.raw.color("lineColor", "#333"),
            main_bkg: self.raw.color("mainBkg", "#fff"),
            text_color: self.raw.color("textColor", "#333"),
            font_family: self
                .raw
                .root_or_theme_string("fontFamily", "trebuchet ms, verdana, arial, sans-serif"),
        }
    }

    #[cfg(feature = "diagram-venn")]
    pub(crate) fn venn(&self) -> crate::Result<VennTheme> {
        let background = self.raw.color("background", "#f4f4f4");
        let is_dark_theme = is_dark(&background)?;

        Ok(VennTheme {
            font_family_css: self.raw.font_family_css_root_first(),
            title_color: self
                .raw
                .optional_color("vennTitleTextColor")
                .or_else(|| self.raw.optional_color("titleColor"))
                .unwrap_or_else(|| "#333".to_string()),
            set_text_color: self
                .raw
                .optional_color("vennSetTextColor")
                .or_else(|| self.raw.optional_color("primaryTextColor"))
                .or_else(|| self.raw.optional_color("textColor"))
                .unwrap_or_else(|| "#333".to_string()),
            circle_colors: (1..=8)
                .filter_map(|index| self.raw.optional_color(&format!("venn{index}")))
                .collect(),
            primary_color: self.raw.color("primaryColor", "#ECECFF"),
            is_dark_theme,
        })
    }

    #[cfg(feature = "diagram-journey")]
    pub(crate) fn journey(&self) -> JourneyTheme {
        let text_color = self.raw.color("textColor", "#333");

        JourneyTheme {
            font_family_css: self.raw.font_family_css(),
            text_color: text_color.clone(),
            line_color: self.raw.color("lineColor", "#333333"),
            face_color: self.raw.color("faceColor", "#FFF8DC"),
            main_bkg: self.raw.color("mainBkg", "#ECECFF"),
            node_border: self.raw.color("nodeBorder", "#9370DB"),
            arrowhead_color: self.raw.color("arrowheadColor", "#333333"),
            edge_label_background: self
                .raw
                .color("edgeLabelBackground", "rgba(232,232,232, 0.8)"),
            title_color: self.raw.color("titleColor", text_color.as_str()),
            tertiary_color: self
                .raw
                .color("tertiaryColor", "hsl(80, 100%, 96.2745098039%)"),
            border2: self.raw.color("border2", "#aaaa33"),
            fill_types: (0..8)
                .map(|index| {
                    self.raw.color(
                        &format!("fillType{index}"),
                        journey_default_fill_type(index),
                    )
                })
                .collect(),
            actor_colors: (0..6)
                .map(|index| self.raw.optional_color(&format!("actor{index}")))
                .collect(),
        }
    }

    #[cfg(feature = "diagram-radar")]
    pub(crate) fn radar(&self) -> RadarTheme {
        let font_family_css = self
            .raw
            .optional_color("fontFamily")
            .map(|font_family| crate::config::normalize_css_font_family(&font_family))
            .unwrap_or_else(|| crate::config::MERMAID_DEFAULT_FONT_FAMILY_CSS.to_string());
        let base_font_size_css = self
            .raw
            .optional_value("fontSize")
            .unwrap_or_else(|| "16px".to_string());
        let scoped_string = |key: &str, fallback: &str| {
            self.raw
                .optional_scoped_string("radar", key)
                .unwrap_or_else(|| fallback.to_string())
        };
        let scoped_f64 = |key: &str, fallback: f64| {
            self.raw
                .optional_scoped_f64("radar", key)
                .unwrap_or(fallback)
        };

        RadarTheme {
            font_family_css,
            base_font_size_css: base_font_size_css.clone(),
            text_color: self.raw.color("textColor", "#333"),
            line_color: self.raw.color("lineColor", "#333333"),
            error_bkg_color: self.raw.color("errorBkgColor", "#552222"),
            error_text_color: self.raw.color("errorTextColor", "#552222"),
            title_font_size_css: base_font_size_css,
            title_color: self.raw.color("titleColor", "#333"),
            axis_color: scoped_string("axisColor", "#333333"),
            axis_stroke_width: scoped_f64("axisStrokeWidth", 2.0),
            axis_label_font_size: scoped_f64("axisLabelFontSize", 12.0),
            graticule_color: scoped_string("graticuleColor", "#DEDEDE"),
            graticule_opacity: scoped_f64("graticuleOpacity", 0.3),
            graticule_stroke_width: scoped_f64("graticuleStrokeWidth", 1.0),
            legend_font_size: scoped_f64("legendFontSize", 12.0),
            curve_opacity: scoped_f64("curveOpacity", 0.5),
            curve_stroke_width: scoped_f64("curveStrokeWidth", 2.0),
            series_colors: (0..12)
                .map(|index| {
                    self.raw
                        .color(&format!("cScale{index}"), default_c_scale(index))
                })
                .collect(),
        }
    }

    #[cfg(feature = "diagram-timeline")]
    pub(crate) fn timeline(&self) -> TimelineTheme {
        let theme_name = self.common.theme_name.clone();
        let theme_color_limit = self
            .raw
            .optional_f64("THEME_COLOR_LIMIT")
            .map(|value| {
                let value = if value.is_nan() {
                    1.0
                } else {
                    value.clamp(1.0, 64.0)
                };
                value as usize
            })
            .unwrap_or(12);
        let sections = (0..theme_color_limit)
            .map(|i| {
                let c_scale = self.raw.color(&format!("cScale{i}"), default_c_scale(i));
                let c_scale_label = self
                    .raw
                    .optional_color(&format!("cScaleLabel{i}"))
                    .unwrap_or_else(|| default_c_scale_label(i).to_string());
                let c_scale_inv = self
                    .raw
                    .optional_color(&format!("cScaleInv{i}"))
                    .unwrap_or_else(|| default_c_scale_inv(i).to_string());

                TimelineSectionTheme {
                    c_scale,
                    c_scale_label,
                    c_scale_inv,
                }
            })
            .collect();

        TimelineTheme {
            is_redux_theme: theme_name.contains("redux"),
            is_dark_theme: theme_name.contains("dark"),
            is_color_theme: theme_name.contains("color"),
            stroke_width: self.raw.css_value("strokeWidth", "1"),
            font_weight: self.raw.css_value("fontWeight", "normal"),
            main_bkg: self.raw.color("mainBkg", "#ECECFF"),
            node_border: self.raw.color("nodeBorder", "#9370DB"),
            disabled_fill: self.raw.color("tertiaryColor", "lightgray"),
            disabled_text_fill: self.raw.color("clusterBorder", "#efefef"),
            root_fill: self.raw.color("git0", "hsl(240, 100%, 46.2745098039%)"),
            root_label: self.raw.color("gitBranchLabel0", "#ffffff"),
            border_colors: self.raw.string_array("borderColorArray"),
            sections,
        }
    }

    pub(in crate::svg::parity) fn common(&self) -> &CommonCssTheme {
        &self.common
    }

    #[cfg(feature = "diagram-treemap")]
    fn treemap_style_option(&self, key: &str, default_value: &str) -> String {
        self.raw
            .optional_root_scoped_css_value("treemap", key)
            .unwrap_or_else(|| default_value.to_string())
    }

    #[cfg(any(
        feature = "diagram-agentflow",
        feature = "diagram-mindmap",
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-block"
    ))]
    pub(in crate::svg::parity) fn node_diagram(&self) -> NodeDiagramTheme {
        let node_border = self.raw.color("nodeBorder", "#9370DB");
        let main_bkg = self.raw.color("mainBkg", "#ECECFF");

        NodeDiagramTheme {
            common: self.common.clone(),
            node_text_color: self
                .raw
                .color("nodeTextColor", self.common.text_color.as_str()),
            title_color: self
                .raw
                .color("titleColor", self.common.text_color.as_str()),
            main_bkg,
            node_border,
            arrowhead_color: self
                .raw
                .color("arrowheadColor", self.common.line_color.as_str()),
            stroke_width: self.raw.css_value("strokeWidth", "1"),
            edge_label_background: self
                .raw
                .color("edgeLabelBackground", "rgba(232,232,232, 0.8)"),
            tertiary: self
                .raw
                .color("tertiaryColor", "hsl(80, 100%, 96.2745098039%)"),
            cluster_bkg: self.raw.color("clusterBkg", "#ffffde"),
            cluster_border: self.raw.color("clusterBorder", "#aaaa33"),
        }
    }

    #[cfg(feature = "diagram-class")]
    pub(in crate::svg::parity) fn class_diagram(&self) -> ClassDiagramTheme {
        let class_text = self.raw.color(
            "classText",
            &self
                .raw
                .color("primaryTextColor", self.common.text_color.as_str()),
        );

        ClassDiagramTheme {
            common: self.common.clone(),
            class_text: class_text.clone(),
            note_text: self.raw.color("noteTextColor", "#333"),
            class_group_text: self
                .raw
                .optional_color("nodeBorder")
                .unwrap_or_else(|| class_text.clone()),
            title_color: self.raw.color("titleColor", "#333"),
            text_color: self.raw.color("textColor", class_text.as_str()),
            main_bkg: self.raw.color("mainBkg", "#ECECFF"),
            node_border: self.raw.color("nodeBorder", "#9370DB"),
            cluster_bkg: self.raw.color("clusterBkg", "#ffffde"),
            cluster_border: self.raw.color("clusterBorder", "#aaaa33"),
            stroke_width: self.raw.css_value("strokeWidth", "1"),
        }
    }

    #[cfg(feature = "diagram-sequence")]
    pub(in crate::svg::parity) fn sequence_diagram(&self) -> SequenceDiagramTheme {
        let actor_border = self.raw.color("actorBorder", "#9370DB");
        let actor_fill = self.raw.color("actorBkg", "#ECECFF");
        let actor_text = self.raw.color("actorTextColor", "black");

        SequenceDiagramTheme {
            common: self.common.clone(),
            actor_border: actor_border.clone(),
            actor_fill: actor_fill.clone(),
            stroke_width: self.raw.css_value("strokeWidth", "1"),
            drop_shadow: self.raw.css_value("dropShadow", "none"),
            note_border: self.raw.color("noteBorderColor", "#aaaa33"),
            note_fill: self.raw.color("noteBkgColor", "#fff5ad"),
            actor_text: actor_text.clone(),
            actor_line: self.raw.color("actorLineColor", actor_border.as_str()),
            signal_color: self.raw.color("signalColor", "#333"),
            sequence_number: self.raw.color("sequenceNumberColor", "white"),
            signal_text: self.raw.color("signalTextColor", "#333"),
            label_box_border: self.raw.color("labelBoxBorderColor", actor_border.as_str()),
            label_box_fill: self.raw.color("labelBoxBkgColor", actor_fill.as_str()),
            label_text: self.raw.color("labelTextColor", actor_text.as_str()),
            loop_text: self.raw.color("loopTextColor", actor_text.as_str()),
            note_text: self.raw.color("noteTextColor", "black"),
            activation_fill: self.raw.color("activationBkgColor", "#f4f4f4"),
            activation_border: self.raw.color("activationBorderColor", "#666"),
            node_border: self.raw.color("nodeBorder", actor_border.as_str()),
            label_box_filter: if self.common.is_neo() {
                self.raw.css_value("dropShadow", "none")
            } else {
                "none".to_string()
            },
        }
    }

    #[cfg(feature = "diagram-state")]
    pub(in crate::svg::parity) fn state_diagram(&self) -> StateDiagramTheme {
        let node_border = self.raw.color("nodeBorder", "#9370DB");
        let main_bkg = self.raw.color("mainBkg", "#ECECFF");
        let background = self.raw.color("background", "white");
        let stroke_width = self.raw.css_value("strokeWidth", "1");
        let stroke_width_px = if stroke_width.trim_end().ends_with("px") {
            stroke_width.clone()
        } else {
            format!("{stroke_width}px")
        };
        let stroke_width_value = stroke_width
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f64>()
            .unwrap_or(1.0)
            .max(0.0);
        let rough_stroke_width_value = if (stroke_width_value - 1.0).abs() <= 1e-9 {
            1.3
        } else {
            stroke_width_value
        };
        let transition_color = self
            .raw
            .color("transitionColor", self.common.line_color.as_str());
        let special_state_color = self
            .raw
            .color("specialStateColor", self.common.line_color.as_str());
        let inner_end_background = self.raw.color("innerEndBackground", node_border.as_str());

        StateDiagramTheme {
            common: self.common.clone(),
            transition_color,
            node_border: node_border.clone(),
            background: background.clone(),
            main_bkg: main_bkg.clone(),
            alt_background: self.raw.color("altBackground", "#efefef"),
            stroke_width,
            stroke_width_px,
            rough_stroke_width_value,
            note_border: self.raw.color("noteBorderColor", "#aaaa33"),
            note_bkg: self.raw.color("noteBkgColor", "#fff5ad"),
            note_text: self.raw.color("noteTextColor", "black"),
            label_background: self.raw.color("labelBackgroundColor", main_bkg.as_str()),
            edge_label_background: self
                .raw
                .color("edgeLabelBackground", "rgba(232,232,232, 0.8)"),
            transition_label_color: self
                .raw
                .optional_color("transitionLabelColor")
                .or_else(|| self.raw.optional_color("tertiaryTextColor"))
                .unwrap_or_else(|| self.common.text_color.clone()),
            special_state_color,
            inner_end_background,
            composite_background: self
                .raw
                .optional_color("compositeBackground")
                .unwrap_or_else(|| background.to_string()),
            state_bkg: self
                .raw
                .optional_color("stateBkg")
                .unwrap_or_else(|| main_bkg.clone()),
            state_border: self
                .raw
                .optional_color("stateBorder")
                .unwrap_or_else(|| node_border.clone()),
            composite_title_background: self
                .raw
                .color("compositeTitleBackground", main_bkg.as_str()),
            state_label_color: self.raw.color("stateLabelColor", "#131300"),
            drop_shadow: self
                .raw
                .optional_value("dropShadow")
                .unwrap_or_else(|| "none".to_string()),
        }
    }
}

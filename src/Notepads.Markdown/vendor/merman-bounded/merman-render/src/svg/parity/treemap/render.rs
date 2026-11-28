use super::super::*;
use crate::treemap::{TREEMAP_SECTION_HEADER_HEIGHT_PX, TREEMAP_SECTION_INNER_PADDING_PX};

// Treemap diagram SVG renderer implementation (split from parity.rs).

pub(crate) fn render_treemap_diagram_svg(
    layout: &crate::model::TreemapDiagramLayout,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    #[derive(Default)]
    struct OrdinalScale {
        range: Vec<String>,
        domain: std::collections::HashMap<String, usize>,
    }

    impl OrdinalScale {
        fn get(&mut self, key: &str) -> String {
            let idx = if let Some(idx) = self.domain.get(key).copied() {
                idx
            } else {
                let idx = self.domain.len();
                self.domain.insert(key.to_string(), idx);
                idx
            };
            if self.range.is_empty() {
                return String::new();
            }
            self.range[idx % self.range.len()].clone()
        }
    }

    fn replace_first(haystack: &str, needle: &str, replacement: &str) -> String {
        if needle.is_empty() {
            return haystack.to_string();
        }
        let Some(idx) = haystack.find(needle) else {
            return haystack.to_string();
        };
        let mut out = String::with_capacity(haystack.len() - needle.len() + replacement.len());
        out.push_str(&haystack[..idx]);
        out.push_str(replacement);
        out.push_str(&haystack[idx + needle.len()..]);
        out
    }

    #[derive(Default)]
    struct OrderedMap {
        order: Vec<(String, String)>,
        idx: std::collections::HashMap<String, usize>,
    }

    impl OrderedMap {
        fn set(&mut self, k: &str, v: &str) {
            if k.is_empty() {
                return;
            }
            if let Some(&i) = self.idx.get(k) {
                self.order[i].1 = v.to_string();
                return;
            }
            self.idx.insert(k.to_string(), self.order.len());
            self.order.push((k.to_string(), v.to_string()));
        }
    }

    fn treemap_is_label_style(key: &str) -> bool {
        matches!(
            key.trim(),
            "color"
                | "font-size"
                | "font-family"
                | "font-weight"
                | "font-style"
                | "text-decoration"
                | "text-align"
                | "text-transform"
                | "line-height"
                | "letter-spacing"
                | "word-spacing"
                | "text-shadow"
                | "text-overflow"
                | "white-space"
                | "word-wrap"
                | "word-break"
                | "overflow-wrap"
                | "hyphens"
        )
    }

    #[derive(Default)]
    struct TreemapCompiledStyles {
        label_styles: String,
        node_styles: String,
        border_styles: Vec<String>,
    }

    fn treemap_styles2_string(css_compiled_styles: &[String]) -> TreemapCompiledStyles {
        // Ported from Mermaid `handDrawnShapeStyles.compileStyles()` / `styles2String()`:
        // - preserve insertion order of the first occurrence of a key
        // - later occurrences override values, without changing order
        // - tolerate tokens without `:` (JS `split(':')` yields `value = undefined`)
        let mut m = OrderedMap::default();

        for entry in css_compiled_styles {
            for raw in entry.split(';') {
                let s = raw.trim();
                if s.is_empty() {
                    continue;
                }
                let (k, v) = if let Some((k, v)) = s.split_once(':') {
                    (k.trim(), v.trim())
                } else {
                    (s.trim(), "")
                };
                m.set(k, v);
            }
        }

        let mut label_styles: Vec<String> = Vec::new();
        let mut node_styles: Vec<String> = Vec::new();
        let mut border_styles: Vec<String> = Vec::new();

        for (k, v) in &m.order {
            if v.is_empty() {
                continue;
            }
            let decl = format!("{k}:{v}");
            let decl_imp = format!("{decl} !important");
            if treemap_is_label_style(k) {
                label_styles.push(decl_imp);
            } else {
                node_styles.push(decl_imp.clone());
                if k.contains("stroke") {
                    border_styles.push(decl_imp);
                }
            }
        }

        TreemapCompiledStyles {
            label_styles: label_styles.join(";"),
            node_styles: node_styles.join(";"),
            border_styles,
        }
    }

    fn normalize_dom_style_color(color: &str) -> String {
        // Upstream mutates this style through D3 after setting the attribute, so preserve the
        // browser CSSOM serialization boundary while sharing the color parser.
        super::super::util::cssom_color_value(color)
    }

    fn format_int_with_commas(n: i64) -> String {
        let mut s = n.abs().to_string();
        let mut out = String::new();
        while s.len() > 3 {
            let split_at = s.len() - 3;
            let tail = &s[split_at..];
            if out.is_empty() {
                out = tail.to_string();
            } else {
                out = format!("{tail},{out}");
            }
            s.truncate(split_at);
        }
        if out.is_empty() {
            out = s;
        } else {
            out = format!("{s},{out}");
        }
        if n < 0 { format!("-{out}") } else { out }
    }

    fn format_value(value: f64, format_str: &str) -> String {
        let format_str = format_str.trim();
        let uses_commas = format_str.is_empty() || format_str == ",";
        if uses_commas {
            if (value - value.round()).abs() < 1e-9 {
                return format_int_with_commas(value.round() as i64);
            }
            let raw = format!("{value}");
            let Some((head, tail)) = raw.split_once('.') else {
                return raw;
            };
            let int_part = head
                .parse::<i64>()
                .ok()
                .map(format_int_with_commas)
                .unwrap_or_else(|| head.to_string());
            if tail.is_empty() {
                return int_part;
            }
            format!("{int_part}.{tail}")
        } else if format_str == "$0,0" {
            let v = value.round() as i64;
            format!("${}", format_int_with_commas(v))
        } else if format_str.starts_with('$') {
            let v = format_value(value, ",");
            format!("${v}")
        } else {
            // Fallback: approximate D3 `format()` behavior.
            format_value(value, ",")
        }
    }

    let diagram_id = options.diagram_id_or("treemap");

    let theme = PresentationTheme::new(effective_config).treemap()?;

    let mut color_scale = OrdinalScale::default();
    color_scale.range.push("transparent".to_string());
    color_scale.range.extend(theme.color_scale.iter().cloned());

    let mut color_scale_peer = OrdinalScale::default();
    color_scale_peer.range.push("transparent".to_string());
    color_scale_peer
        .range
        .extend(theme.color_scale_peer.iter().cloned());

    let mut color_scale_label = OrdinalScale::default();
    color_scale_label
        .range
        .extend(theme.color_scale_label.iter().cloned());

    let has_acc_title = layout
        .acc_title
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_acc_descr = layout
        .acc_descr
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());

    let measurer = options.text_measurer();
    let title = layout.title.as_deref().filter(|t| !t.trim().is_empty());
    let title_shift_y = layout.title_height;
    let title_bbox = title.map(|t| {
        let style = crate::text::TextStyle {
            font_family: Some(r#""trebuchet ms",verdana,arial,sans-serif"#.to_string()),
            font_size: 14.0,
            font_weight: None,
            font_style: None,
        };
        let w = measurer
            .measure_svg_simple_text_bbox_width_px(t, &style)
            .max(0.0);
        let h = measurer
            .measure_svg_simple_text_bbox_height_px(t, &style)
            .max(0.0);
        (w, h)
    });

    #[derive(Debug, Clone, Copy)]
    struct TreemapRect {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    }

    #[derive(Debug, Clone, Copy)]
    struct TreemapViewBoxBounds {
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    }

    impl TreemapViewBoxBounds {
        const fn empty() -> Self {
            Self {
                min_x: f64::INFINITY,
                min_y: f64::INFINITY,
                max_x: f64::NEG_INFINITY,
                max_y: f64::NEG_INFINITY,
            }
        }

        fn include_rect(&mut self, rect: TreemapRect) {
            let TreemapRect { x0, y0, x1, y1 } = rect;
            let w = x1 - x0;
            let h = y1 - y0;
            if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
                return;
            }
            self.min_x = self.min_x.min(x0);
            self.min_y = self.min_y.min(y0);
            self.max_x = self.max_x.max(x1);
            self.max_y = self.max_y.max(y1);
        }

        fn has_rects(self) -> bool {
            self.min_x.is_finite()
                && self.min_y.is_finite()
                && self.max_x.is_finite()
                && self.max_y.is_finite()
        }
    }

    let mut viewbox_bounds = TreemapViewBoxBounds::empty();

    for s in &layout.sections {
        if s.depth == 0 {
            continue;
        }
        viewbox_bounds.include_rect(TreemapRect {
            x0: s.x0,
            y0: s.y0,
            x1: s.x1,
            y1: s.y1,
        });
    }
    for l in &layout.leaves {
        viewbox_bounds.include_rect(TreemapRect {
            x0: l.x0,
            y0: l.y0,
            x1: l.x1,
            y1: l.y1,
        });
    }

    // Treemap sections/leaves are rendered under `<g class="treemapContainer" transform="translate(0, title_height)">`.
    // Include that translation when computing the root viewport. Also include the title text's
    // bbox (dominant-baseline="middle") so `parity-root` matches the upstream getBBox-derived
    // viewBox w/h.
    if title_shift_y > 0.0 && viewbox_bounds.min_y.is_finite() && viewbox_bounds.max_y.is_finite() {
        viewbox_bounds.min_y += title_shift_y;
        viewbox_bounds.max_y += title_shift_y;
    }
    if let (Some(title), Some(&(w, h))) = (title, title_bbox.as_ref()) {
        let cx = layout.width / 2.0;
        let cy = layout.title_height / 2.0;
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            if !title.trim().is_empty() {
                // If measurement is unexpectedly degenerate, still ensure we don't ignore the title
                // region entirely.
                viewbox_bounds.min_y = viewbox_bounds.min_y.min(0.0);
                viewbox_bounds.max_y = viewbox_bounds.max_y.max(layout.title_height);
            }
        } else {
            viewbox_bounds.include_rect(TreemapRect {
                x0: cx - (w / 2.0),
                y0: cy - (h / 2.0),
                x1: cx + (w / 2.0),
                y1: cy + (h / 2.0),
            });
        }
    }

    let vb_x;
    let vb_y;
    let vb_w;
    let vb_h;
    if viewbox_bounds.has_rects() {
        vb_x = viewbox_bounds.min_x - layout.diagram_padding;
        vb_y = viewbox_bounds.min_y - layout.diagram_padding;
        vb_w = (viewbox_bounds.max_x - viewbox_bounds.min_x) + layout.diagram_padding * 2.0;
        vb_h = (viewbox_bounds.max_y - viewbox_bounds.min_y) + layout.diagram_padding * 2.0;
    } else {
        vb_x = -layout.diagram_padding;
        vb_y = -layout.diagram_padding;
        vb_w = layout.diagram_padding * 2.0;
        vb_h = layout.diagram_padding * 2.0;
    }

    let css = treemap_css(diagram_id, effective_config)?;

    let mut out = String::new();
    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));
    let extra_attrs: [(&str, &str); 1] = [("class", "flowchart")];
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "treemap");
    root_chrome.extra_attrs = &extra_attrs;
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom = root_svg::RootDomProfile {
        style_viewbox_order: root_svg::SvgRootStyleViewBoxOrder::ViewBoxThenStyle,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Treemap, diagram_id)
            .write_open(
                &mut out,
                root_svg::RootViewportSpec::responsive(root_svg::DiagramBounds::from_view_box(
                    vb_x, vb_y, vb_w, vb_h,
                )),
                root_chrome,
            )?;
    options.checkpoint_emit()?;

    if let (Some(title), true) = (layout.acc_title.as_deref(), has_acc_title) {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{diagram_id}">{}</title>"#,
            escape_xml(title)
        );
    }
    if let (Some(descr), true) = (layout.acc_descr.as_deref(), has_acc_descr) {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{diagram_id}">{}</desc>"#,
            escape_xml(descr.trim_end_matches('\n'))
        );
    }

    let _ = write!(&mut out, "<style>{}</style>", css);
    out.push_str("<g/>");
    options.checkpoint_emit()?;

    if let Some(title) = layout.title.as_deref().filter(|t| !t.trim().is_empty()) {
        let _ = write!(
            &mut out,
            r#"<text x="{x}" y="{y}" class="treemapTitle" text-anchor="middle" dominant-baseline="middle">{text}</text>"#,
            x = fmt(layout.width / 2.0),
            y = fmt(layout.title_height / 2.0),
            text = escape_xml(title)
        );
    }

    let _ = write!(
        &mut out,
        r#"<g transform="translate(0, {ty})" class="treemapContainer">"#,
        ty = fmt(layout.title_height)
    );

    let computed_length_measurer = options.text_measurer_for(TextMeasurementPhase::ComputedLength);
    let font_family = r#""trebuchet ms",verdana,arial,sans-serif"#.to_string();
    let section_header_height = TREEMAP_SECTION_HEADER_HEIGHT_PX;
    let section_header_center_y = section_header_height / 2.0;
    let section_label_inset_x: f64 = 6.0;
    let section_label_font_size: f64 = 12.0;
    let section_value_font_size: f64 = 10.0;
    let section_inner_padding = TREEMAP_SECTION_INNER_PADDING_PX;
    let section_label_reserved_value_width: f64 = 30.0;
    let section_label_min_visible_width: f64 = 15.0;

    for (i, section) in layout.sections.iter().enumerate() {
        let section_clip_id = format!("clip-section-{diagram_id}-{i}");
        options.checkpoint_emit()?;
        let w = section.x1 - section.x0;
        let h = section.y1 - section.y0;
        let _ = write!(
            &mut out,
            r#"<g class="treemapSection" transform="translate({x},{y})">"#,
            x = fmt(section.x0),
            y = fmt(section.y0)
        );

        let header_style = if section.depth == 0 {
            "display: none;"
        } else {
            ""
        };
        let _ = write!(
            &mut out,
            r#"<rect width="{w}" height="{hh}" class="treemapSectionHeader" fill="none" fill-opacity="0.6" stroke-width="0.6" style="{style}"/>"#,
            w = fmt(w),
            hh = fmt(section_header_height),
            style = header_style
        );

        let _ = write!(
            &mut out,
            r#"<clipPath id="{id}"><rect width="{w}" height="{h}"/></clipPath>"#,
            id = section_clip_id.as_str(),
            w = fmt((w - 2.0 * section_label_inset_x).max(0.0)),
            h = fmt(section_header_height)
        );

        let fill = color_scale.get(&section.name);
        let stroke = color_scale_peer.get(&section.name);
        let section_css: &[String] = section.css_compiled_styles.as_deref().unwrap_or(&[]);
        let compiled = treemap_styles2_string(section_css);
        let section_style = if section.depth == 0 {
            "display: none;".to_string()
        } else {
            format!(
                "{};{}",
                compiled.node_styles,
                compiled.border_styles.join(";")
            )
        };
        let _ = write!(
            &mut out,
            r#"<rect width="{w}" height="{h}" class="treemapSection section{i}" fill="{fill}" fill-opacity="0.6" stroke="{stroke}" stroke-width="2" stroke-opacity="0.4" style="{style}"/>"#,
            w = fmt(w),
            h = fmt(h),
            i = i,
            fill = escape_attr(&fill),
            stroke = escape_attr(&stroke),
            style = escape_attr(&section_style)
        );

        let mut label_text = if section.depth == 0 {
            String::new()
        } else {
            section.name.clone()
        };

        let label_fill = if section.depth == 0 {
            String::new()
        } else {
            color_scale_label.get(&section.name)
        };
        let label_styles_suffix = replace_first(&compiled.label_styles, "color:", "fill:");

        if label_text.is_empty() {
            let _ = write!(
                &mut out,
                r#"<text class="treemapSectionLabel" x="{x}" y="{y}" dominant-baseline="middle" font-weight="bold" clip-path="url(#{id})" style="display: none;"/>"#,
                x = fmt(section_label_inset_x),
                y = fmt(section_header_center_y),
                id = section_clip_id.as_str(),
            );
        } else {
            // Mirror Mermaid's truncation loop in `renderer.ts` (uses `getComputedTextLength()`).
            let total_header_width = w;
            let label_x_position = section_label_inset_x;
            let mut space_for_text_content =
                total_header_width - label_x_position - section_label_inset_x;
            if layout.show_values && section.value != 0.0 {
                let value_ends_at_x_relative = total_header_width - section_inner_padding;
                let estimated_value_text_actual_width = section_label_reserved_value_width;
                let gap_between_label_and_value = section_inner_padding;
                let label_must_end_before_x = value_ends_at_x_relative
                    - estimated_value_text_actual_width
                    - gap_between_label_and_value;
                space_for_text_content = label_must_end_before_x - label_x_position;
            }
            let actual_available_width =
                section_label_min_visible_width.max(space_for_text_content);

            let style = crate::text::TextStyle {
                font_family: Some(font_family.clone()),
                font_size: section_label_font_size,
                font_weight: Some("bold".to_string()),
                font_style: None,
            };

            if computed_length_measurer.measure_svg_text_computed_length_px(&label_text, &style)
                > actual_available_width
            {
                let ellipsis = "...";
                let original = label_text.clone();
                let mut current = original.clone();
                while !current.is_empty() {
                    current.pop();
                    if current.is_empty() {
                        if computed_length_measurer
                            .measure_svg_text_computed_length_px(ellipsis, &style)
                            > actual_available_width
                        {
                            label_text.clear();
                        } else {
                            label_text = ellipsis.to_string();
                        }
                        break;
                    }
                    let candidate = format!("{current}{ellipsis}");
                    if computed_length_measurer
                        .measure_svg_text_computed_length_px(&candidate, &style)
                        <= actual_available_width
                    {
                        label_text = candidate;
                        break;
                    }
                }
            }

            let section_label_style = format!(
                "dominant-baseline: middle; font-size: {}px; fill:{fill}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;{suffix}",
                fmt(section_label_font_size),
                fill = escape_attr(&label_fill),
                suffix = label_styles_suffix
            );
            let _ = write!(
                &mut out,
                r#"<text class="treemapSectionLabel" x="{x}" y="{y}" dominant-baseline="middle" font-weight="bold" clip-path="url(#{id})" style="{style}">{text}</text>"#,
                x = fmt(section_label_inset_x),
                y = fmt(section_header_center_y),
                id = section_clip_id.as_str(),
                style = escape_attr(&section_label_style),
                text = escape_xml(&label_text)
            );
        }

        if layout.show_values {
            let value_text = if section.value != 0.0 {
                format_value(section.value, &layout.value_format)
            } else {
                String::new()
            };
            let section_value_style = if section.depth == 0 {
                "display: none;".to_string()
            } else {
                format!(
                    "text-anchor: end; dominant-baseline: middle; font-size: {}px; fill:{fill}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;{suffix}",
                    fmt(section_value_font_size),
                    fill = escape_attr(&label_fill),
                    suffix = label_styles_suffix
                )
            };
            if value_text.is_empty() {
                let _ = write!(
                    &mut out,
                    r#"<text class="treemapSectionValue" x="{x}" y="{y}" text-anchor="end" dominant-baseline="middle" font-style="italic" style="{style}"/>"#,
                    x = fmt(w - section_inner_padding),
                    y = fmt(section_header_center_y),
                    style = escape_attr(&section_value_style)
                );
            } else {
                let _ = write!(
                    &mut out,
                    r#"<text class="treemapSectionValue" x="{x}" y="{y}" text-anchor="end" dominant-baseline="middle" font-style="italic" style="{style}">{text}</text>"#,
                    x = fmt(w - section_inner_padding),
                    y = fmt(section_header_center_y),
                    style = escape_attr(&section_value_style),
                    text = escape_xml(&value_text)
                );
            }
        }

        out.push_str("</g>");
    }

    let is_complex_treemap = layout.leaves.len() > 20;
    let base_label_font_size = if is_complex_treemap { 16.0 } else { 38.0 };
    let base_value_font_size = if is_complex_treemap { 14.0 } else { 28.0 };
    let min_label_font_size = if is_complex_treemap { 4.0 } else { 8.0 };
    let min_value_font_size = if is_complex_treemap { 4.0 } else { 6.0 };
    let label_padding = if is_complex_treemap { 2.0 } else { 4.0 };
    let min_display_threshold = if is_complex_treemap { 8.0 } else { 10.0 };
    let spacing_between_label_and_value = if is_complex_treemap { 1.0 } else { 2.0 };

    for (i, leaf) in layout.leaves.iter().enumerate() {
        let leaf_clip_id = format!("clip-{diagram_id}-{i}");
        options.checkpoint_emit()?;
        let w = leaf.x1 - leaf.x0;
        let h = leaf.y1 - leaf.y0;

        let group_class = if let Some(cls) = leaf
            .class_selector
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            format!("treemapNode treemapLeafGroup leaf{i} {cls}x")
        } else {
            format!("treemapNode treemapLeafGroup leaf{i}x")
        };

        let fill_key = leaf.parent_name.as_deref().unwrap_or(leaf.name.as_str());
        let fill = color_scale.get(fill_key);

        let leaf_css: &[String] = leaf.css_compiled_styles.as_deref().unwrap_or(&[]);
        let compiled = treemap_styles2_string(leaf_css);
        let leaf_rect_style = compiled.node_styles.clone();
        let label_styles_suffix = replace_first(&compiled.label_styles, "color:", "fill:");
        let leaf_label_fill = theme.readable_leaf_label_fill(
            &fill,
            &leaf_rect_style,
            color_scale_label.get(&leaf.name),
        );

        let _ = write!(
            &mut out,
            r#"<g class="{class}" transform="translate({x},{y})">"#,
            class = escape_attr(&group_class),
            x = fmt(leaf.x0),
            y = fmt(leaf.y0)
        );

        let _ = write!(
            &mut out,
            r#"<rect width="{w}" height="{h}" class="treemapLeaf" fill="{fill}" style="{style}" fill-opacity="0.3" stroke="{fill}" stroke-width="3"/>"#,
            w = fmt(w),
            h = fmt(h),
            fill = escape_attr(&fill),
            style = escape_attr(&leaf_rect_style)
        );

        let _ = write!(
            &mut out,
            r#"<clipPath id="{id}"><rect width="{w}" height="{h}"/></clipPath>"#,
            id = leaf_clip_id.as_str(),
            w = fmt((w - 4.0).max(0.0)),
            h = fmt((h - 4.0).max(0.0))
        );

        let available_w = w - 2.0 * label_padding;
        let available_h = h - 2.0 * label_padding;

        let mut label_font_size = base_label_font_size;
        let value_scale_factor = 0.6;

        let mut label_hidden = false;
        if available_w < min_display_threshold || available_h < min_display_threshold {
            label_hidden = true;
        } else {
            let mut style = crate::text::TextStyle {
                font_family: Some(font_family.clone()),
                font_size: label_font_size,
                font_weight: None,
                font_style: None,
            };

            loop {
                if computed_length_measurer.measure_svg_text_computed_length_px(&leaf.name, &style)
                    <= available_w
                    || label_font_size <= min_label_font_size
                {
                    break;
                }
                label_font_size -= 1.0;
                style.font_size = label_font_size;
            }

            let mut prospective_value_font_size = (label_font_size * value_scale_factor)
                .round()
                .min(base_value_font_size)
                .max(min_value_font_size);
            let mut combined_h =
                label_font_size + spacing_between_label_and_value + prospective_value_font_size;

            while combined_h > available_h && label_font_size > min_label_font_size {
                label_font_size -= 1.0;
                style.font_size = label_font_size;
                prospective_value_font_size = (label_font_size * value_scale_factor)
                    .round()
                    .min(base_value_font_size)
                    .max(min_value_font_size);
                combined_h =
                    label_font_size + spacing_between_label_and_value + prospective_value_font_size;
            }

            style.font_size = label_font_size;
            if is_complex_treemap {
                if label_font_size < min_label_font_size || available_h < min_label_font_size {
                    label_hidden = true;
                }
            } else {
                if computed_length_measurer.measure_svg_text_computed_length_px(&leaf.name, &style)
                    > available_w
                    || label_font_size < min_label_font_size
                    || available_h < label_font_size
                {
                    label_hidden = true;
                }
            }
        }

        let label_style = if !label_hidden && (label_font_size - base_label_font_size).abs() < 1e-9
        {
            // Preserve Mermaid's "raw attr('style', ...)" formatting when the label isn't
            // modified by the `.each()` loop.
            format!(
                "text-anchor: middle; dominant-baseline: middle; font-size: {font_size}px;fill:{fill};{suffix}",
                font_size = fmt(base_label_font_size),
                fill = escape_attr(&leaf_label_fill),
                suffix = label_styles_suffix
            )
        } else {
            let fill = normalize_dom_style_color(&leaf_label_fill);
            let mut s = format!(
                "text-anchor: middle; dominant-baseline: middle; font-size: {fs}px; fill: {fill};",
                fs = fmt(label_font_size),
                fill = escape_attr(&fill),
            );
            if label_hidden {
                s.push_str(" display: none;");
            }
            if !label_styles_suffix.is_empty() {
                s.push_str(&label_styles_suffix);
            }
            s
        };

        let _ = write!(
            &mut out,
            r#"<text class="treemapLabel" x="{x}" y="{y}" style="{style}" clip-path="url(#{id})">{text}</text>"#,
            x = fmt(w / 2.0),
            y = fmt(h / 2.0),
            style = escape_attr(&label_style),
            id = leaf_clip_id.as_str(),
            text = escape_xml(&leaf.name)
        );

        if layout.show_values {
            let value_text = if leaf.value != 0.0 {
                format_value(leaf.value, &layout.value_format)
            } else {
                String::new()
            };
            let mut value_font_size = base_value_font_size;
            let mut value_y = h / 2.0; // placeholder (overwritten when label is visible)
            let mut value_hidden = true;

            if !label_hidden {
                let actual_value_font_size = (label_font_size * value_scale_factor)
                    .round()
                    .min(base_value_font_size)
                    .max(min_value_font_size);
                value_font_size = actual_value_font_size;

                let label_center_y = h / 2.0;
                value_y =
                    label_center_y + (label_font_size / 2.0) + spacing_between_label_and_value;

                let cell_bottom_padding = 4.0;
                let max_value_bottom_y = h - cell_bottom_padding;
                let available_w_for_value = w - 2.0 * label_padding;

                let style = crate::text::TextStyle {
                    font_family: Some(font_family.clone()),
                    font_size: value_font_size,
                    font_weight: None,
                    font_style: None,
                };
                let value_w_px = computed_length_measurer
                    .measure_svg_text_computed_length_px(&value_text, &style);
                if value_w_px <= available_w_for_value
                    && value_y + value_font_size <= max_value_bottom_y
                    && value_font_size >= min_value_font_size
                {
                    value_hidden = false;
                }
            }

            let fill = normalize_dom_style_color(&leaf_label_fill);
            let mut value_style = format!(
                "text-anchor: middle; dominant-baseline: hanging; font-size: {fs}px; fill: {fill};",
                fs = fmt(value_font_size),
                fill = escape_attr(&fill)
            );
            if value_hidden {
                value_style.push_str(" display: none;");
            }
            if !label_styles_suffix.is_empty() {
                value_style.push_str(&label_styles_suffix);
            }

            if value_text.is_empty() {
                let _ = write!(
                    &mut out,
                    r#"<text class="treemapValue" x="{x}" y="{y}" style="{style}" clip-path="url(#{id})"/>"#,
                    x = fmt(w / 2.0),
                    y = fmt(value_y),
                    style = escape_attr(&value_style),
                    id = leaf_clip_id.as_str(),
                );
            } else {
                let _ = write!(
                    &mut out,
                    r#"<text class="treemapValue" x="{x}" y="{y}" style="{style}" clip-path="url(#{id})">{text}</text>"#,
                    x = fmt(w / 2.0),
                    y = fmt(value_y),
                    style = escape_attr(&value_style),
                    id = leaf_clip_id.as_str(),
                    text = escape_xml(&value_text)
                );
            }
        }

        out.push_str("</g>");
    }

    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TreemapDiagramLayout, TreemapLeafLayout, TreemapSectionLayout};

    fn leaf(name: impl Into<String>, value: f64, x0: f64, x1: f64, y1: f64) -> TreemapLeafLayout {
        TreemapLeafLayout {
            name: name.into(),
            value,
            parent_name: None,
            x0,
            y0: 0.0,
            x1,
            y1,
            class_selector: None,
            css_compiled_styles: None,
        }
    }

    fn leaf_group(svg: &str, index: usize) -> &str {
        let class = format!(r#"class="treemapNode treemapLeafGroup leaf{index}x""#);
        let class_start = svg.find(&class).expect("leaf group class");
        let start = svg[..class_start].rfind("<g").expect("leaf group start");
        let end = start + svg[start..].find("</g>").expect("leaf group end") + 4;
        &svg[start..end]
    }

    fn opening_tag_by_class<'a>(fragment: &'a str, class_name: &str) -> &'a str {
        let needle = format!(r#"<text class="{class_name}""#);
        let start = fragment.find(&needle).expect("text tag by class");
        let end = start + fragment[start..].find('>').expect("text tag end") + 1;
        &fragment[start..end]
    }

    fn attr_f64(tag: &str, name: &str) -> f64 {
        let prefix = format!(r#"{name}=""#);
        let start = tag.find(&prefix).expect("attribute") + prefix.len();
        let end = start + tag[start..].find('"').expect("attribute end");
        tag[start..end].parse().expect("numeric attribute")
    }

    fn font_size_px(tag: &str) -> f64 {
        let (_, suffix) = tag.split_once("font-size:").expect("font-size style");
        let value = suffix.trim_start();
        let end = value.find("px").expect("font-size px suffix");
        value[..end].trim().parse().expect("font-size number")
    }

    #[test]
    fn treemap_complex_leaf_text_and_section_clipping_match_mermaid_11_16() {
        let mut leaves = vec![
            leaf("Wide", 100.0, 0.0, 200.0, 100.0),
            leaf("A label much wider than its cell", 1.0, 210.0, 222.0, 40.0),
            leaf("Tiny", 1.0, 230.0, 236.0, 40.0),
        ];
        for index in 3..21 {
            leaves.push(leaf(format!("Leaf {index}"), 1.0, 0.0, 100.0, 60.0));
        }
        let layout = TreemapDiagramLayout {
            title_height: 0.0,
            width: 500.0,
            height: 200.0,
            use_max_width: true,
            diagram_padding: 8.0,
            show_values: true,
            value_format: ",".to_string(),
            acc_title: None,
            acc_descr: None,
            title: None,
            sections: vec![TreemapSectionLayout {
                name: "Section label wider than its header".to_string(),
                depth: 1,
                value: 102.0,
                x0: 0.0,
                y0: 0.0,
                x1: 40.0,
                y1: 100.0,
                class_selector: None,
                css_compiled_styles: None,
            }],
            leaves,
        };

        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).expect("SVG execution");
        let svg = render_treemap_diagram_svg(&layout, &serde_json::json!({}), &execution).unwrap();

        let section_label = opening_tag_by_class(&svg, "treemapSectionLabel");
        assert!(
            section_label.contains(r#"clip-path="url(#clip-section-treemap-0)""#),
            "section label must use its emitted clipping path: {section_label}"
        );

        let wide = leaf_group(&svg, 0);
        let wide_label = opening_tag_by_class(wide, "treemapLabel");
        let wide_value = opening_tag_by_class(wide, "treemapValue");
        assert!(!wide_label.contains("display: none"), "{wide_label}");
        assert!(!wide_value.contains("display: none"), "{wide_value}");
        assert!(font_size_px(wide_label) > font_size_px(wide_value));
        assert!(attr_f64(wide_value, "y") > attr_f64(wide_label, "y"));

        let narrow = leaf_group(&svg, 1);
        let narrow_label = opening_tag_by_class(narrow, "treemapLabel");
        let narrow_value = opening_tag_by_class(narrow, "treemapValue");
        assert!(
            narrow.contains(">A label much wider than its cell</text>"),
            "{narrow}"
        );
        assert!(font_size_px(narrow_label) <= font_size_px(wide_label));
        assert!(font_size_px(narrow_label) > 0.0);
        assert!(!narrow_label.contains("display: none"), "{narrow_label}");
        assert!(
            narrow_label.contains(r#"clip-path="url(#clip-treemap-1)""#),
            "narrow complex labels remain present and rely on clipping: {narrow_label}"
        );
        assert!(font_size_px(narrow_value) <= font_size_px(narrow_label));
        assert!(attr_f64(narrow_value, "y") > attr_f64(narrow_label, "y"));
        assert!(!narrow_value.contains("display: none"), "{narrow_value}");

        let tiny = leaf_group(&svg, 2);
        let tiny_label = opening_tag_by_class(tiny, "treemapLabel");
        let tiny_value = opening_tag_by_class(tiny, "treemapValue");
        assert!(tiny_label.contains("display: none"), "{tiny_label}");
        assert!(tiny_value.contains("display: none"), "{tiny_value}");
    }
}

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
//! Flowchart style compilation helpers.

use super::*;

#[derive(Debug, Clone)]
pub(in crate::svg::parity) struct FlowchartCompiledStyles {
    pub(super) node_style: String,
    pub(super) label_style: String,
    pub(super) label_div_decls: Vec<(String, String)>,
    pub(super) fill: Option<String>,
    pub(super) stroke: Option<String>,
    pub(super) stroke_width: Option<String>,
    pub(super) stroke_dasharray: Option<String>,
}

pub(in crate::svg::parity) fn flowchart_compile_styles(
    class_defs: &merman_core::native_collections::OrderedMap<String, Vec<String>>,
    classes: &[String],
    inline_styles_a: &[String],
    inline_styles_b: &[String],
) -> FlowchartCompiledStyles {
    // Ported from Mermaid `handDrawnShapeStyles.compileStyles()` / `styles2String()`:
    // - preserve insertion order of the first occurrence of a key
    // - later occurrences override values, without changing order
    #[derive(Default)]
    struct OrderedMap<'a> {
        order: Vec<(&'a str, &'a str)>,
        idx: FxHashMap<&'a str, usize>,
    }
    impl<'a> OrderedMap<'a> {
        fn set(&mut self, k: &'a str, v: &'a str) {
            if let Some(&i) = self.idx.get(k) {
                self.order[i].1 = v;
                return;
            }
            self.idx.insert(k, self.order.len());
            self.order.push((k, v));
        }
    }

    let mut m: OrderedMap<'_> = OrderedMap::default();

    for c in classes {
        let Some(decls) = class_defs.get(c) else {
            continue;
        };
        for d in decls {
            for d in crate::flowchart::flowchart_split_mermaid_style_decls(d) {
                let Some((k, v)) = parse_style_decl(d) else {
                    continue;
                };
                m.set(k, v);
            }
        }
    }

    for d in inline_styles_a.iter().chain(inline_styles_b.iter()) {
        for d in crate::flowchart::flowchart_split_mermaid_style_decls(d) {
            let Some((k, v)) = parse_style_decl(d) else {
                continue;
            };
            m.set(k, v);
        }
    }

    let mut node_style = String::new();
    let mut label_style = String::new();

    let mut label_div_decls: Vec<(String, String)> = Vec::new();

    let mut fill: Option<String> = None;
    let mut stroke: Option<String> = None;
    let mut stroke_width: Option<String> = None;
    let mut stroke_dasharray: Option<String> = None;

    for (k, v) in &m.order {
        let k = *k;
        let v = *v;
        if is_text_style_key(k) {
            if !label_style.is_empty() {
                label_style.push(';');
            }
            let _ = write!(&mut label_style, "{k}:{v} !important");
            label_div_decls.push((k.to_string(), v.to_string()));
        } else {
            if !node_style.is_empty() {
                node_style.push(';');
            }
            let _ = write!(&mut node_style, "{k}:{v} !important");
        }
        match k {
            "fill" => fill = Some(v.to_string()),
            "stroke" => stroke = Some(v.to_string()),
            "stroke-width" => stroke_width = Some(v.to_string()),
            "stroke-dasharray" => stroke_dasharray = Some(v.to_string()),
            _ => {}
        }
    }

    FlowchartCompiledStyles {
        node_style,
        label_style,
        label_div_decls,
        fill,
        stroke,
        stroke_width,
        stroke_dasharray,
    }
}

pub(in crate::svg::parity) fn flowchart_compile_node_styles(
    class_defs: &merman_core::native_collections::OrderedMap<String, Vec<String>>,
    classes: &[String],
    inline_styles_a: &[String],
    inline_styles_b: &[String],
) -> FlowchartCompiledStyles {
    let effective_classes =
        crate::flowchart::flowchart_effective_node_class_names(class_defs, classes)
            .into_iter()
            .map(|class| class.to_string())
            .collect::<Vec<_>>();
    flowchart_compile_styles(
        class_defs,
        &effective_classes,
        inline_styles_a,
        inline_styles_b,
    )
}

pub(in crate::svg::parity) fn flowchart_label_div_style_prefix(
    styles: &FlowchartCompiledStyles,
    color_as_rgb: bool,
) -> String {
    fn div_style_survives_mermaid_overrides(key: &str) -> bool {
        !matches!(key, "line-height" | "text-align" | "white-space")
    }

    let mut out = String::new();
    for (key, value) in &styles.label_div_decls {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() || !div_style_survives_mermaid_overrides(key) {
            continue;
        }
        if key == "color" {
            if color_as_rgb {
                let color = super::super::util::cssom_color_value(value);
                let _ = write!(&mut out, "color: {color} !important; ");
            } else {
                let _ = write!(
                    &mut out,
                    "color: {} !important; ",
                    value.to_ascii_lowercase()
                );
            }
        } else {
            let _ = write!(&mut out, "{key}: {value} !important; ");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_style(value: &str) -> FlowchartCompiledStyles {
        FlowchartCompiledStyles {
            node_style: String::new(),
            label_style: String::new(),
            label_div_decls: vec![("color".to_string(), value.to_string())],
            fill: None,
            stroke: None,
            stroke_width: None,
            stroke_dasharray: None,
        }
    }

    #[test]
    fn flowchart_html_label_color_uses_the_shared_cssom_boundary() {
        assert_eq!(
            flowchart_label_div_style_prefix(&color_style("#12345680"), true),
            "color: rgba(18, 52, 86, 0.502) !important; "
        );
        assert_eq!(
            flowchart_label_div_style_prefix(&color_style("hsl(210 50% 40%)"), true),
            "color: rgb(51, 102, 153) !important; "
        );
        assert_eq!(
            flowchart_label_div_style_prefix(&color_style("var(--LabelColor)"), true),
            "color: var(--LabelColor) !important; "
        );
    }
}

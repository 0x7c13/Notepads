use crate::Result;
use crate::config::{config_bool, config_f64, config_string};
use crate::model::{
    Bounds, CynefinDiagramLayout, CynefinDomainLayout, CynefinItemLayout, CynefinTransitionLayout,
};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::cynefin::CynefinDiagramRenderModel;

const DOMAIN_COMPLEX: &str = "complex";
const DOMAIN_COMPLICATED: &str = "complicated";
const DOMAIN_CHAOTIC: &str = "chaotic";
const DOMAIN_CLEAR: &str = "clear";
const DOMAIN_CONFUSION: &str = "confusion";
const QUADRANT_DOMAINS: &[&str] = &[
    DOMAIN_COMPLEX,
    DOMAIN_COMPLICATED,
    DOMAIN_CHAOTIC,
    DOMAIN_CLEAR,
];
const ALL_DOMAINS: &[&str] = &[
    DOMAIN_COMPLEX,
    DOMAIN_COMPLICATED,
    DOMAIN_CHAOTIC,
    DOMAIN_CLEAR,
    DOMAIN_CONFUSION,
];
const ITEM_HEIGHT: f64 = 26.0;
const ITEM_GAP: f64 = 4.0;
const ITEM_PADDING_X: f64 = 10.0;
const MAX_CONFUSION_ITEMS: usize = 3;

#[derive(Debug, Clone)]
pub(crate) struct CynefinLayoutSettings {
    pub width: f64,
    pub height: f64,
    pub padding: f64,
    pub show_domain_descriptions: bool,
    pub boundary_amplitude: f64,
    pub seed: Option<f64>,
    pub use_max_width: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CynefinTheme {
    pub font_family: String,
    pub complex_bg: String,
    pub complicated_bg: String,
    pub chaotic_bg: String,
    pub clear_bg: String,
    pub confusion_bg: String,
    pub domain_font_size: f64,
    pub item_font_size: f64,
    pub boundary_color: String,
    pub boundary_width: f64,
    pub cliff_color: String,
    pub cliff_width: f64,
    pub arrow_color: String,
    pub arrow_width: f64,
    pub text_color: String,
    pub label_color: String,
}

pub(crate) fn cynefin_layout_settings(
    effective_config: &serde_json::Value,
) -> CynefinLayoutSettings {
    // Mermaid only accepts a JavaScript `number` here. Do not reuse the general
    // config-number helper: it intentionally accepts YAML string numbers, while
    // upstream `resolveSeed` ignores strings based on `typeof configuredSeed`.
    let seed = effective_config
        .get("cynefin")
        .and_then(|config| config.get("seed"))
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value != 0.0);
    CynefinLayoutSettings {
        width: config_f64(effective_config, &["cynefin", "width"])
            .unwrap_or(800.0)
            .max(1.0),
        height: config_f64(effective_config, &["cynefin", "height"])
            .unwrap_or(600.0)
            .max(1.0),
        padding: config_f64(effective_config, &["cynefin", "padding"])
            .unwrap_or(40.0)
            .max(0.0),
        show_domain_descriptions: config_bool(
            effective_config,
            &["cynefin", "showDomainDescriptions"],
        )
        .unwrap_or(true),
        boundary_amplitude: config_f64(effective_config, &["cynefin", "boundaryAmplitude"])
            .filter(|value| value.is_finite())
            .unwrap_or(8.0),
        seed,
        use_max_width: config_bool(effective_config, &["cynefin", "useMaxWidth"]).unwrap_or(true),
    }
}

pub(crate) fn cynefin_theme(effective_config: &serde_json::Value) -> CynefinTheme {
    fn color(cfg: &serde_json::Value, key: &str, fallback: &str) -> String {
        cfg.get("themeVariables")
            .and_then(|value| value.get("cynefin"))
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    }
    fn themed_color(
        cfg: &serde_json::Value,
        key: &str,
        fallback_key: &str,
        fallback: &str,
    ) -> String {
        cfg.get("themeVariables")
            .and_then(|value| value.get("cynefin"))
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| config_string(cfg, &["themeVariables", fallback_key]))
            .unwrap_or_else(|| fallback.to_string())
    }
    fn number(cfg: &serde_json::Value, key: &str, fallback: f64) -> f64 {
        config_f64(cfg, &["themeVariables", "cynefin", key]).unwrap_or(fallback)
    }

    CynefinTheme {
        font_family: crate::config::config_font_family_css(effective_config),
        complex_bg: color(effective_config, "complexBg", "#E8F5E9"),
        complicated_bg: color(effective_config, "complicatedBg", "#E3F2FD"),
        chaotic_bg: color(effective_config, "chaoticBg", "#FBE9E7"),
        clear_bg: color(effective_config, "clearBg", "#FFF8E1"),
        confusion_bg: color(effective_config, "confusionBg", "#F3E5F5"),
        domain_font_size: number(effective_config, "domainFontSize", 16.0),
        item_font_size: number(effective_config, "itemFontSize", 12.0),
        boundary_color: themed_color(effective_config, "boundaryColor", "lineColor", "#333333"),
        boundary_width: number(effective_config, "boundaryWidth", 2.0),
        cliff_color: color(effective_config, "cliffColor", "#8B0000"),
        cliff_width: number(effective_config, "cliffWidth", 4.0),
        arrow_color: themed_color(effective_config, "arrowColor", "lineColor", "#333333"),
        arrow_width: number(effective_config, "arrowWidth", 2.0),
        text_color: themed_color(effective_config, "textColor", "textColor", "#333"),
        label_color: themed_color(
            effective_config,
            "labelColor",
            "primaryTextColor",
            "#131300",
        ),
    }
}

pub(crate) fn layout_cynefin_diagram_typed(
    model: &CynefinDiagramRenderModel,
    effective_config: &serde_json::Value,
    measurer: &dyn TextMeasurer,
) -> Result<CynefinDiagramLayout> {
    let settings = cynefin_layout_settings(effective_config);
    let theme = cynefin_theme(effective_config);
    let domain_layouts = build_domain_layouts(settings.width, settings.height);
    let items = layout_items(model, &domain_layouts, &settings, &theme, measurer);
    let transitions = layout_transitions(model, &domain_layouts);
    let total_width = settings.width + settings.padding * 2.0;
    let total_height = settings.height + settings.padding * 2.0;

    Ok(CynefinDiagramLayout {
        bounds: Some(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: total_width,
            max_y: total_height,
        }),
        width: settings.width,
        height: settings.height,
        padding: settings.padding,
        total_width,
        total_height,
        use_max_width: settings.use_max_width,
        show_domain_descriptions: settings.show_domain_descriptions,
        boundary_amplitude: settings.boundary_amplitude,
        seed: settings.seed,
        domain_layouts,
        items,
        transitions,
    })
}

pub(crate) fn quadrant_domains() -> &'static [&'static str] {
    QUADRANT_DOMAINS
}

pub(crate) fn domain_title(name: &str) -> &str {
    match name {
        DOMAIN_COMPLEX => "Complex",
        DOMAIN_COMPLICATED => "Complicated",
        DOMAIN_CHAOTIC => "Chaotic",
        DOMAIN_CLEAR => "Clear",
        DOMAIN_CONFUSION => "Confusion",
        _ => name,
    }
}

pub(crate) fn domain_model_and_practice(name: &str) -> (&'static str, &'static str) {
    match name {
        DOMAIN_COMPLEX => (
            "Probe \u{2192} Sense \u{2192} Respond",
            "Emergent Practices",
        ),
        DOMAIN_COMPLICATED => ("Sense \u{2192} Analyse \u{2192} Respond", "Good Practices"),
        DOMAIN_CLEAR => (
            "Sense \u{2192} Categorise \u{2192} Respond",
            "Best Practices",
        ),
        DOMAIN_CHAOTIC => ("Act \u{2192} Sense \u{2192} Respond", "Novel Practices"),
        DOMAIN_CONFUSION => ("", "Disorder"),
        _ => ("", ""),
    }
}

pub(crate) fn domain_fill<'a>(theme: &'a CynefinTheme, name: &str) -> &'a str {
    match name {
        DOMAIN_COMPLEX => &theme.complex_bg,
        DOMAIN_COMPLICATED => &theme.complicated_bg,
        DOMAIN_CHAOTIC => &theme.chaotic_bg,
        DOMAIN_CLEAR => &theme.clear_bg,
        DOMAIN_CONFUSION => &theme.confusion_bg,
        _ => &theme.confusion_bg,
    }
}

pub(crate) fn resolve_seed(configured_seed: Option<f64>, id: &str) -> f64 {
    configured_seed.unwrap_or_else(|| f64::from(hash_string(id)))
}

pub(crate) fn generate_fold_path(
    width: f64,
    height: f64,
    seed: f64,
    amplitude_override: Option<f64>,
) -> String {
    let cx = width / 2.0;
    let amplitude = amplitude_override.unwrap_or(width * 0.015);
    let segments = 7usize;
    let seg_height = height / segments as f64;
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let jitter = seeded_random(seed + i as f64 * 17.0) * amplitude * 2.0 - amplitude;
        points.push((cx + jitter, i as f64 * seg_height));
    }
    let mut d = format!("M{},{}", fmt_number(points[0].0), fmt_number(points[0].1));
    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let mid_y = (p0.1 + p1.1) / 2.0;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * dir * seeded_random(seed + i as f64 * 31.0 + 7.0);
        d.push_str(&format!(
            " C{},{} {},{} {},{}",
            fmt_number(p0.0 + offset),
            fmt_number(mid_y),
            fmt_number(p1.0 - offset),
            fmt_number(mid_y),
            fmt_number(p1.0),
            fmt_number(p1.1)
        ));
    }
    d
}

pub(crate) fn generate_horizontal_boundary(
    width: f64,
    height: f64,
    seed: f64,
    amplitude_override: Option<f64>,
) -> String {
    let cy = height / 2.0;
    let amplitude = amplitude_override.unwrap_or(height * 0.015);
    let segments = 7usize;
    let seg_width = width / segments as f64;
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let jitter = seeded_random(seed + i as f64 * 23.0) * amplitude * 2.0 - amplitude;
        points.push((i as f64 * seg_width, cy + jitter));
    }
    let mut d = format!("M{},{}", fmt_number(points[0].0), fmt_number(points[0].1));
    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let mid_x = (p0.0 + p1.0) / 2.0;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        let offset = amplitude * 1.5 * dir * seeded_random(seed + i as f64 * 37.0 + 11.0);
        d.push_str(&format!(
            " C{},{} {},{} {},{}",
            fmt_number(mid_x),
            fmt_number(p0.1 + offset),
            fmt_number(mid_x),
            fmt_number(p1.1 - offset),
            fmt_number(p1.0),
            fmt_number(p1.1)
        ));
    }
    d
}

pub(crate) fn generate_cliff_path(width: f64, height: f64) -> String {
    let cx = width / 2.0;
    let top_y = height * 0.5;
    let bottom_y = height;
    let amplitude = width * 0.03;
    format!(
        "M{},{} C{},{} {},{} {},{} C{},{} {},{} {},{}",
        fmt_number(cx),
        fmt_number(top_y),
        fmt_number(cx + amplitude),
        fmt_number(top_y + (bottom_y - top_y) * 0.2),
        fmt_number(cx - amplitude * 1.5),
        fmt_number(top_y + (bottom_y - top_y) * 0.55),
        fmt_number(cx + amplitude * 0.5),
        fmt_number(top_y + (bottom_y - top_y) * 0.75),
        fmt_number(cx - amplitude),
        fmt_number(top_y + (bottom_y - top_y) * 0.85),
        fmt_number(cx + amplitude * 0.3),
        fmt_number(top_y + (bottom_y - top_y) * 0.95),
        fmt_number(cx),
        fmt_number(bottom_y)
    )
}

pub(crate) fn generate_confusion_path(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    format!(
        "M{},{} A{},{} 0 1,1 {},{} A{},{} 0 1,1 {},{} Z",
        fmt_number(cx - rx),
        fmt_number(cy),
        fmt_number(rx),
        fmt_number(ry),
        fmt_number(cx + rx),
        fmt_number(cy),
        fmt_number(rx),
        fmt_number(ry),
        fmt_number(cx - rx),
        fmt_number(cy)
    )
}

fn build_domain_layouts(width: f64, height: f64) -> Vec<CynefinDomainLayout> {
    let hw = width / 2.0;
    let hh = height / 2.0;
    vec![
        CynefinDomainLayout {
            name: DOMAIN_COMPLEX.to_string(),
            cx: hw / 2.0,
            cy: hh / 2.0,
            x: 0.0,
            y: 0.0,
            width: hw,
            height: hh,
        },
        CynefinDomainLayout {
            name: DOMAIN_COMPLICATED.to_string(),
            cx: hw + hw / 2.0,
            cy: hh / 2.0,
            x: hw,
            y: 0.0,
            width: hw,
            height: hh,
        },
        CynefinDomainLayout {
            name: DOMAIN_CHAOTIC.to_string(),
            cx: hw / 2.0,
            cy: hh + hh / 2.0,
            x: 0.0,
            y: hh,
            width: hw,
            height: hh,
        },
        CynefinDomainLayout {
            name: DOMAIN_CLEAR.to_string(),
            cx: hw + hw / 2.0,
            cy: hh + hh / 2.0,
            x: hw,
            y: hh,
            width: hw,
            height: hh,
        },
        CynefinDomainLayout {
            name: DOMAIN_CONFUSION.to_string(),
            cx: hw,
            cy: hh,
            x: hw * 0.7,
            y: hh * 0.7,
            width: hw * 0.6,
            height: hh * 0.6,
        },
    ]
}

fn layout_items(
    model: &CynefinDiagramRenderModel,
    domain_layouts: &[CynefinDomainLayout],
    settings: &CynefinLayoutSettings,
    theme: &CynefinTheme,
    measurer: &dyn TextMeasurer,
) -> Vec<CynefinItemLayout> {
    let style = TextStyle {
        font_family: Some(theme.font_family.clone()),
        font_size: theme.item_font_size,
        ..Default::default()
    };
    let mut out = Vec::new();
    for domain_name in ALL_DOMAINS {
        let Some(domain) = model
            .domains
            .iter()
            .find(|domain| domain.name == *domain_name)
        else {
            continue;
        };
        if domain.items.is_empty() {
            continue;
        }
        let Some(layout) = domain_layouts
            .iter()
            .find(|layout| layout.name == *domain_name)
        else {
            continue;
        };
        let is_confusion = *domain_name == DOMAIN_CONFUSION;
        let visible_count = if is_confusion {
            domain.items.len().min(MAX_CONFUSION_ITEMS)
        } else {
            domain.items.len()
        };
        let start_y = if is_confusion {
            layout.cy
                + if settings.show_domain_descriptions {
                    22.0
                } else {
                    14.0
                }
        } else {
            layout.cy
                + if settings.show_domain_descriptions {
                    25.0
                } else {
                    15.0
                }
        };
        for (idx, item) in domain.items.iter().take(visible_count).enumerate() {
            push_item_layout(
                &mut out,
                layout,
                domain_name,
                &item.label,
                idx,
                start_y,
                &style,
                measurer,
                false,
            );
        }
        if is_confusion && domain.items.len() > MAX_CONFUSION_ITEMS {
            let label = format!("+{} more", domain.items.len() - MAX_CONFUSION_ITEMS);
            push_item_layout(
                &mut out,
                layout,
                domain_name,
                &label,
                visible_count,
                start_y,
                &style,
                measurer,
                true,
            );
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn push_item_layout(
    out: &mut Vec<CynefinItemLayout>,
    domain_layout: &CynefinDomainLayout,
    domain_name: &str,
    label: &str,
    idx: usize,
    start_y: f64,
    style: &TextStyle,
    measurer: &dyn TextMeasurer,
    overflow: bool,
) {
    let bbox_width = measurer.measure(label, style).width;
    let measured_width = if bbox_width > 0.0 {
        bbox_width
    } else {
        // Mermaid uses JavaScript `String.length`, so preserve UTF-16 code-unit semantics in the
        // browser-only fallback instead of replacing a valid bbox with a character estimate.
        label.encode_utf16().count() as f64 * 7.0
    };
    let width = measured_width + ITEM_PADDING_X * 2.0;
    let x = domain_layout.cx - width / 2.0;
    let y = start_y + idx as f64 * (ITEM_HEIGHT + ITEM_GAP);
    out.push(CynefinItemLayout {
        domain: domain_name.to_string(),
        label: label.to_string(),
        x,
        y,
        width,
        height: ITEM_HEIGHT,
        text_x: width / 2.0,
        text_y: ITEM_HEIGHT / 2.0,
        overflow,
    });
}

fn layout_transitions(
    model: &CynefinDiagramRenderModel,
    domain_layouts: &[CynefinDomainLayout],
) -> Vec<CynefinTransitionLayout> {
    let mut out = Vec::new();
    for transition in &model.transitions {
        if transition.from == transition.to {
            continue;
        }
        let Some(from) = domain_layouts
            .iter()
            .find(|layout| layout.name == transition.from)
        else {
            continue;
        };
        let Some(to) = domain_layouts
            .iter()
            .find(|layout| layout.name == transition.to)
        else {
            continue;
        };
        let dx = to.cx - from.cx;
        let dy = to.cy - from.cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= f64::EPSILON {
            continue;
        }
        let mx = (from.cx + to.cx) / 2.0;
        let my = (from.cy + to.cy) / 2.0;
        let offset_amount = len * 0.15;
        let nx = -dy / len;
        let ny = dx / len;
        out.push(CynefinTransitionLayout {
            from: transition.from.clone(),
            to: transition.to.clone(),
            label: transition.label.clone(),
            x1: from.cx,
            y1: from.cy,
            x2: to.cx,
            y2: to.cy,
            cpx: mx + nx * offset_amount,
            cpy: my + ny * offset_amount,
        });
    }
    out
}

fn seeded_random(seed: f64) -> f64 {
    let mut t = javascript_to_uint32(seed + f64::from(0x6d2b_79f5_u32));
    t = (t ^ (t >> 15)).wrapping_mul(t | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
    ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
}

fn javascript_to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    value.trunc().rem_euclid(4_294_967_296.0) as u32
}

fn hash_string(value: &str) -> i32 {
    let mut hash: i32 = 0;
    for ch in value.encode_utf16() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(ch as i32);
    }
    hash
}

fn fmt_number(value: f64) -> String {
    if !value.is_finite() || value.abs() < 0.0005 {
        return "0".to_string();
    }
    let mut rounded = (value * 1000.0).round() / 1000.0;
    if rounded.abs() < 0.0005 {
        rounded = 0.0;
    }
    let mut s = format!("{rounded:.3}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{DeterministicTextMeasurer, TextMetrics};

    struct FixedWidthMeasurer(f64);

    impl TextMeasurer for FixedWidthMeasurer {
        fn measure(&self, _text: &str, style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: self.0,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    #[test]
    fn cynefin_boundary_seed_is_stable() {
        assert_eq!(seeded_random(42.0), seeded_random(42.0));
        assert_ne!(hash_string("cynefin-1"), hash_string("cynefin-2"));
        assert_eq!(resolve_seed(Some(7.0), "a"), resolve_seed(Some(7.0), "b"));
    }

    #[test]
    fn cynefin_seed_uses_ecmascript_to_uint32_semantics() {
        assert_eq!(seeded_random(4_294_967_297.0), seeded_random(1.0));
        assert_eq!(javascript_to_uint32(-1.0), u32::MAX);
        assert_eq!(javascript_to_uint32(-1.5), u32::MAX);
        assert_eq!(javascript_to_uint32(f64::INFINITY), 0);
    }

    #[test]
    fn cynefin_seed_requires_a_json_number_like_mermaid() {
        let string_seed = cynefin_layout_settings(&serde_json::json!({
            "cynefin": {"seed": "4294967297"}
        }));
        assert_eq!(string_seed.seed, None);
        assert_eq!(
            resolve_seed(string_seed.seed, "cynefin-string-seed"),
            f64::from(hash_string("cynefin-string-seed")),
            "a string seed must fall back to Mermaid's diagram-id hash"
        );

        let numeric_seed = cynefin_layout_settings(&serde_json::json!({
            "cynefin": {"seed": 4294967297_u64}
        }));
        assert_eq!(numeric_seed.seed, Some(4_294_967_297.0));
        assert_eq!(
            seeded_random(numeric_seed.seed.unwrap()),
            seeded_random(1.0)
        );
    }

    #[test]
    fn cynefin_layout_caps_confusion_overflow() {
        let model = CynefinDiagramRenderModel {
            domains: vec![merman_core::diagrams::cynefin::CynefinDomainModel {
                name: DOMAIN_CONFUSION.to_string(),
                items: vec![
                    merman_core::diagrams::cynefin::CynefinItemModel { label: "A".into() },
                    merman_core::diagrams::cynefin::CynefinItemModel { label: "B".into() },
                    merman_core::diagrams::cynefin::CynefinItemModel { label: "C".into() },
                    merman_core::diagrams::cynefin::CynefinItemModel { label: "D".into() },
                ],
            }],
            ..Default::default()
        };
        let layout = layout_cynefin_diagram_typed(
            &model,
            &serde_json::json!({}),
            &DeterministicTextMeasurer::default(),
        )
        .unwrap();

        assert_eq!(layout.items.len(), 4);
        assert!(layout.items.last().unwrap().overflow);
        assert_eq!(layout.items.last().unwrap().label, "+1 more");
    }

    #[test]
    fn cynefin_item_width_prefers_a_positive_bbox_over_the_character_fallback() {
        let model = CynefinDiagramRenderModel {
            domains: vec![merman_core::diagrams::cynefin::CynefinDomainModel {
                name: DOMAIN_COMPLEX.to_string(),
                items: vec![merman_core::diagrams::cynefin::CynefinItemModel {
                    label: "a deliberately long label".into(),
                }],
            }],
            ..Default::default()
        };
        let layout =
            layout_cynefin_diagram_typed(&model, &serde_json::json!({}), &FixedWidthMeasurer(3.5))
                .unwrap();

        assert_eq!(layout.items[0].width, 3.5 + ITEM_PADDING_X * 2.0);
    }

    #[test]
    fn cynefin_item_width_uses_the_utf16_character_fallback_for_a_zero_bbox() {
        let label = "A\u{1f642}";
        let model = CynefinDiagramRenderModel {
            domains: vec![merman_core::diagrams::cynefin::CynefinDomainModel {
                name: DOMAIN_COMPLEX.to_string(),
                items: vec![merman_core::diagrams::cynefin::CynefinItemModel {
                    label: label.into(),
                }],
            }],
            ..Default::default()
        };
        let layout =
            layout_cynefin_diagram_typed(&model, &serde_json::json!({}), &FixedWidthMeasurer(0.0))
                .unwrap();

        assert_eq!(
            layout.items[0].width,
            label.encode_utf16().count() as f64 * 7.0 + ITEM_PADDING_X * 2.0
        );
    }
}

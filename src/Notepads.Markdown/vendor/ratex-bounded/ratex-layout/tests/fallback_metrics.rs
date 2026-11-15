use std::{collections::HashMap, sync::Arc};

use ratex_layout::{
    layout, layout_with_fallback_metrics, to_display_list, validate_layout, FallbackGlyphMetrics,
    LayoutBox, LayoutOptions,
};
use ratex_parser::parse;
use ratex_types::{color::Color, math_style::MathStyle};

fn metrics() -> Arc<HashMap<char, FallbackGlyphMetrics>> {
    Arc::new(HashMap::from([
        (
            'ш',
            FallbackGlyphMetrics {
                width: 0.82,
                height: 0.53,
                depth: 0.12,
            },
        ),
        (
            'і',
            FallbackGlyphMetrics {
                width: 0.27,
                height: 0.74,
                depth: 0.0,
            },
        ),
    ]))
}

fn output(tree: &LayoutBox) -> serde_json::Value {
    validate_layout(tree).unwrap();
    serde_json::to_value(to_display_list(tree)).unwrap()
}

#[test]
fn measured_metrics_replace_only_missing_glyphs() {
    let options = LayoutOptions::default();
    let metrics = metrics();
    let ast = parse(r"\text{ш}").unwrap();
    let measured = layout_with_fallback_metrics(&ast, &options, metrics.clone());
    assert!((layout(&ast, &options).width - 0.5).abs() < 1e-9);
    assert!((measured.width - 0.82).abs() < 1e-9);
    assert!((measured.height - 0.53).abs() < 1e-9);
    assert!((measured.depth - 0.12).abs() < 1e-9);
    let pair = parse(r"\text{ші}").unwrap();
    assert!(
        (layout_with_fallback_metrics(&pair, &options, metrics.clone()).width - 1.09).abs() < 1e-9
    );

    let mut with_katex = (*metrics).clone();
    with_katex.insert(
        'x',
        FallbackGlyphMetrics {
            width: 3.0,
            height: 3.0,
            depth: 3.0,
        },
    );
    // Neither a KaTeX glyph nor an unlisted fallback glyph may be overridden.
    for source in ["x", r"\text{x}", r"\text{Ж}"] {
        let ast = parse(source).unwrap();
        assert_eq!(
            output(&layout_with_fallback_metrics(
                &ast,
                &options,
                Arc::new(with_katex.clone())
            )),
            output(&layout(&ast, &options)),
            "{source}",
        );
    }
}

#[test]
fn empty_map_matches_legacy_entry_across_layout_contexts() {
    let empty = Arc::new(HashMap::new());
    for style in [
        MathStyle::Display,
        MathStyle::Text,
        MathStyle::Script,
        MathStyle::ScriptScript,
    ] {
        let options = LayoutOptions::default()
            .with_style(style)
            .with_color(Color::new(0.2, 0.3, 0.4, 0.8))
            .with_inter_glyph_kern(0.03);
        for source in [
            "",
            r"\text{шіЖ} + x^2",
            r"\frac{\text{ш}}{\sqrt{y}}",
            r"\left(\text{ш}\middle|\left(x\middle|y\right)\right)",
            r"\begin{array}{c:c}\text{ш}&x\\y&\Huge z\end{array}",
            r"\pmb{\textcolor{red}{\text{ш}}}",
        ] {
            let ast = parse(source).unwrap();
            assert_eq!(
                output(&layout_with_fallback_metrics(&ast, &options, empty.clone())),
                output(&layout(&ast, &options)),
                "{style:?}: {source}",
            );
        }
    }
}

#[test]
fn metrics_survive_groups_color_style_and_explicit_size_changes() {
    let options = LayoutOptions::default();
    let metrics = metrics();
    for (source, scale) in [
        (r"{{\text{ш}}}", 1.0),
        (r"\textcolor{red}{\text{ш}}", 1.0),
        (r"{\scriptstyle\text{ш}}", 0.7),
        (r"{\scriptscriptstyle\text{ш}}", 0.5),
        (r"{\Huge\textcolor{blue}{\text{ш}}}", 2.488),
        (r"{\Huge{\tiny\text{ш}}}", 0.5),
    ] {
        let tree = layout_with_fallback_metrics(&parse(source).unwrap(), &options, metrics.clone());
        validate_layout(&tree).unwrap();
        assert!(
            (tree.width - 0.82 * scale).abs() < 1e-9,
            "{source}: {}",
            tree.width
        );
        assert!(
            (tree.height - 0.53 * scale).abs() < 1e-9,
            "{source}: {}",
            tree.height
        );
        assert!(
            (tree.depth - 0.12 * scale).abs() < 1e-9,
            "{source}: {}",
            tree.depth
        );
    }
}

#[test]
fn separate_calls_do_not_retain_metrics_or_change_legacy_output() {
    let options = LayoutOptions::default();
    let ast = parse(r"\left(\text{ш}\middle|\frac{\text{ш}}{x}\right)").unwrap();
    let baseline = output(&layout(&ast, &options));
    let metrics = metrics();
    let measured = output(&layout_with_fallback_metrics(
        &ast,
        &options,
        metrics.clone(),
    ));
    assert_ne!(measured, baseline);
    assert_eq!(Arc::strong_count(&metrics), 1);
    assert_eq!(output(&layout(&ast, &options)), baseline);
    let mut larger = (*metrics).clone();
    larger.get_mut(&'ш').unwrap().height = 3.0;
    assert_ne!(
        output(&layout_with_fallback_metrics(
            &ast,
            &options,
            Arc::new(larger)
        )),
        measured,
    );
    assert_eq!(
        output(&layout_with_fallback_metrics(&ast, &options, metrics)),
        measured
    );
}

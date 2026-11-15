use ratex_layout::to_display_list;
/// Integration tests comparing ratex-layout box dimensions against KaTeX.
///
/// These expected values were extracted from KaTeX 0.16.38 using
/// `tools/layout_compare/katex_layout.mjs`. They represent the strut
/// ascent (height) and depth in em units for display mode.
///
/// Tolerance: 0.001em (well within the 0.02em threshold from the plan).
use ratex_layout::{layout, LayoutOptions};
use ratex_parser::parser::parse;
use ratex_types::color::Color;
use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::path_command::PathCommand;
use ratex_types::MathStyle;

const TOLERANCE: f64 = 0.002;

fn check(input: &str, expected_height: f64, expected_depth: f64) {
    let ast = parse(input).unwrap_or_else(|e| panic!("Parse error for `{input}`: {e}"));
    let options = LayoutOptions::default();
    let lbox = layout(&ast, &options);

    let h_diff = (lbox.height - expected_height).abs();
    let d_diff = (lbox.depth - expected_depth).abs();

    assert!(
        h_diff < TOLERANCE,
        "`{input}` height: expected {expected_height:.5}, got {:.5} (Δ={h_diff:.5})",
        lbox.height
    );
    assert!(
        d_diff < TOLERANCE,
        "`{input}` depth: expected {expected_depth:.5}, got {:.5} (Δ={d_diff:.5})",
        lbox.depth
    );
}

fn layout_with_style(input: &str, style: MathStyle) -> ratex_layout::LayoutBox {
    let ast = parse(input).unwrap_or_else(|e| panic!("Parse error for `{input}`: {e}"));
    let options = LayoutOptions::default().with_style(style);
    layout(&ast, &options)
}

fn max_path_x(display: &DisplayList) -> f64 {
    let mut max_x = f64::NEG_INFINITY;

    for item in &display.items {
        let DisplayItem::Path { x, commands, .. } = item else {
            continue;
        };

        for command in commands {
            match command {
                PathCommand::MoveTo { x: cx, .. } | PathCommand::LineTo { x: cx, .. } => {
                    max_x = max_x.max(*x + *cx);
                }
                PathCommand::CubicTo { x1, x2, x: cx, .. } => {
                    max_x = max_x.max(*x + *x1).max(*x + *x2).max(*x + *cx);
                }
                PathCommand::QuadTo { x1, x: cx, .. } => {
                    max_x = max_x.max(*x + *x1).max(*x + *cx);
                }
                PathCommand::Close => {}
            }
        }
    }

    max_x
}

#[test]
fn single_char_x() {
    check("x", 0.43056, 0.0);
}

#[test]
fn htmlstyle_applies_supported_css() {
    let ast = parse("\\htmlStyle{color: blue; font-size: 20px; font-weight: bold; font-style: italic; background-color: yellow; text-decoration: underline;}{x}").unwrap();
    let options = LayoutOptions::default();
    let lbox = layout(&ast, &options);
    let display = to_display_list(&lbox);

    assert!(display.width > layout(&parse("x").unwrap(), &options).width);
    assert!(display.items.iter().any(|item| matches!(
        item,
        DisplayItem::Rect { color, .. } if *color == Color::from_name("yellow").unwrap()
    )));
    assert!(display.items.iter().any(|item| matches!(
        item,
        DisplayItem::Line { color, .. } if *color == Color::from_name("blue").unwrap()
    )));
    assert!(display.items.iter().any(|item| matches!(
        item,
        DisplayItem::GlyphPath { color, font, .. }
            if *color == Color::from_name("blue").unwrap() && font == "Main-BoldItalic"
    )));
}

#[test]
fn htmlstyle_nested_leftright_middle_keeps_inner_metrics() {
    let options = LayoutOptions::default();
    let inner = layout(&parse("\\left( x \\middle| y \\right)").unwrap(), &options);
    let wrapped = layout(
        &parse("\\htmlStyle{background-color: yellow;}{\\left( x \\middle| y \\right)}").unwrap(),
        &options,
    );

    assert!(
        (wrapped.height - inner.height).abs() < TOLERANCE,
        "HTML wrapper should not reserve current-pass \\middle height for nested LeftRight: inner height {:.5}, wrapped height {:.5}",
        inner.height,
        wrapped.height,
    );
    assert!(
        (wrapped.depth - inner.depth).abs() < TOLERANCE,
        "HTML wrapper should not reserve current-pass \\middle depth for nested LeftRight: inner depth {:.5}, wrapped depth {:.5}",
        inner.depth,
        wrapped.depth,
    );
}

#[test]
fn htmlstyle_mathchoice_ignores_middle_in_unselected_branch() {
    let options = LayoutOptions::default();
    let plain = layout(
        &parse("\\left( \\htmlStyle{background-color: yellow;}{x} \\right)").unwrap(),
        &options,
    );
    let with_unselected_middle = layout(
        &parse("\\left( \\htmlStyle{background-color: yellow;}{\\mathchoice{x}{x \\middle| y}{x}{x}} \\right)").unwrap(),
        &options,
    );

    assert!(
        (with_unselected_middle.height - plain.height).abs() < TOLERANCE,
        "HTML wrapper should not reserve \\middle height from unselected \\mathchoice branch: plain height {:.5}, wrapped height {:.5}",
        plain.height,
        with_unselected_middle.height,
    );
    assert!(
        (with_unselected_middle.depth - plain.depth).abs() < TOLERANCE,
        "HTML wrapper should not reserve \\middle depth from unselected \\mathchoice branch: plain depth {:.5}, wrapped depth {:.5}",
        plain.depth,
        with_unselected_middle.depth,
    );
    assert!(
        (with_unselected_middle.width - plain.width).abs() < TOLERANCE,
        "HTML wrapper should not choose larger delimiters from unselected \\mathchoice branch: plain width {:.5}, wrapped width {:.5}",
        plain.width,
        with_unselected_middle.width,
    );
}

#[test]
fn htmlmathml_leftright_middle_renders_html_branch_delimiter() {
    let options = LayoutOptions::default();
    let plain = layout(&parse("\\left( x \\middle| y \\right)").unwrap(), &options);
    let wrapped = layout(
        &parse("\\left( \\html@mathml{x \\middle| y}{x} \\right)").unwrap(),
        &options,
    );
    let plain_display = to_display_list(&plain);
    let wrapped_display = to_display_list(&wrapped);

    assert_eq!(
        wrapped_display.items.len(),
        plain_display.items.len(),
        "\\html@mathml html branch should render the current-pass \\middle delimiter"
    );
}

#[test]
fn widetilde_path_stays_inside_display_width_after_previous_glyph() {
    let ast = parse("x\\widetilde{x}").unwrap();
    let options = LayoutOptions::default();
    let display = to_display_list(&layout(&ast, &options));
    let max_path_x = max_path_x(&display);

    assert!(
        max_path_x.is_finite(),
        "Expected \\widetilde to emit an SVG path"
    );
    assert!(
        max_path_x <= display.width + TOLERANCE,
        "\\widetilde path should stay within display width: max path x {max_path_x:.5}, display width {:.5}",
        display.width,
    );
}

#[test]
fn href_non_typewriter_body_keeps_link_underline() {
    let ast = parse("\\href{https://example.com}{x}").unwrap();
    let options = LayoutOptions::default();
    let lbox = layout(&ast, &options);
    let display = to_display_list(&lbox);

    assert!(display.items.iter().any(|item| matches!(
        item,
        DisplayItem::Line { color, .. } if *color == Color::from_name("blue").unwrap()
    )));
}

#[test]
fn prooftree_binary_emits_inference_rule() {
    let ast =
        parse("\\begin{prooftree}\\AxiomC{P}\\AxiomC{Q}\\BinaryInfC{R}\\end{prooftree}").unwrap();
    let options = LayoutOptions::default();
    let lbox = layout(&ast, &options);
    let display = to_display_list(&lbox);

    assert!(display.width > 0.0);
    assert!(display.height > 0.0);
    assert!(display
        .items
        .iter()
        .any(|item| matches!(item, DisplayItem::Line { dashed: false, .. })));
}

#[test]
fn prooftree_dashed_and_noline_rules() {
    let dashed_ast =
        parse("\\begin{prooftree}\\AxiomC{P}\\dashedLine\\UnaryInfC{Q}\\end{prooftree}").unwrap();
    let options = LayoutOptions::default();
    let dashed_display = to_display_list(&layout(&dashed_ast, &options));
    assert!(dashed_display
        .items
        .iter()
        .any(|item| matches!(item, DisplayItem::Line { dashed: true, .. })));

    let noline_ast =
        parse("\\begin{prooftree}\\AxiomC{P}\\noLine\\UnaryInfC{Q}\\end{prooftree}").unwrap();
    let noline_display = to_display_list(&layout(&noline_ast, &options));
    assert!(!noline_display
        .items
        .iter()
        .any(|item| matches!(item, DisplayItem::Line { .. })));
}

#[test]
fn prooftree_root_at_top_flips_layout() {
    let normal_ast = parse("\\begin{prooftree}\\AxiomC{P}\\UIC{Q}\\end{prooftree}").unwrap();
    let root_at_top_ast =
        parse("\\begin{prooftree}\\AxiomC{P}\\rootAtTop\\UIC{Q}\\end{prooftree}").unwrap();
    let options = LayoutOptions::default();
    let normal_display = to_display_list(&layout(&normal_ast, &options));
    let root_at_top_display = to_display_list(&layout(&root_at_top_ast, &options));
    // Both should produce valid non-empty output
    assert!(normal_display.width > 0.0);
    assert!(normal_display.height > 0.0);
    assert!(root_at_top_display.width > 0.0);
    assert!(root_at_top_display.height > 0.0);
    // rootAtTop places conclusion at top, so height should differ from normal
    // (normal has premises above conclusion; rootAtTop has conclusion at top)
    assert!(
        (normal_display.height - root_at_top_display.height).abs() > 0.001,
        "rootAtTop should change the vertical layout"
    );
}

#[test]
fn single_char_uppercase() {
    check("A", 0.68333, 0.0);
}

#[test]
fn single_digit() {
    check("1", 0.64444, 0.0);
}

#[test]
fn binary_op_a_plus_b() {
    check("a+b", 0.69444, 0.08333);
}

#[test]
fn unicode_white_circle_matches_square_metric_box() {
    let circle = layout_with_style("○", MathStyle::Display);
    let square = layout_with_style("□", MathStyle::Display);
    let bigcirc = layout_with_style("\\bigcirc", MathStyle::Display);
    let circle_expr = layout_with_style("○\\div□=5", MathStyle::Display);
    let square_expr = layout_with_style("□\\div□=5", MathStyle::Display);
    let bold_circle = layout_with_style("\\mathbf{○}", MathStyle::Display);

    assert!((circle.width - square.width).abs() < TOLERANCE);
    assert!((circle.height - square.height).abs() < TOLERANCE);
    assert!((circle.depth - square.depth).abs() < TOLERANCE);
    assert!(bigcirc.width > circle.width);
    assert!((circle_expr.width - square_expr.width).abs() < TOLERANCE);
    assert!((bold_circle.width - square.width).abs() < TOLERANCE);
    assert!((bold_circle.height - square.height).abs() < TOLERANCE);
    assert!((bold_circle.depth - square.depth).abs() < TOLERANCE);

    let circle_display = to_display_list(&circle);
    let square_display = to_display_list(&square);
    let glyph_bounds = |display: &DisplayList| {
        display
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::GlyphPath {
                    x,
                    y,
                    scale,
                    font,
                    char_code,
                    ..
                } => {
                    let font_id = ratex_font::FontId::parse(font).unwrap();
                    let metrics = ratex_font::get_char_metrics(font_id, *char_code).unwrap();
                    Some((
                        *x,
                        x + metrics.width * scale,
                        y - metrics.height * scale,
                        y + metrics.depth * scale,
                        *scale,
                        font_id,
                        *char_code,
                    ))
                }
                _ => None,
            })
            .unwrap()
    };
    let circle_glyph = glyph_bounds(&circle_display);
    let square_glyph = glyph_bounds(&square_display);

    assert_eq!(circle_glyph.5, ratex_font::FontId::MainRegular);
    assert_eq!(circle_glyph.6, '◯' as u32);
    assert!(circle_glyph.4 < 1.0);
    assert!((circle_glyph.0 + circle_glyph.1 - square_glyph.0 - square_glyph.1).abs() < TOLERANCE);
    assert!((circle_glyph.2 - square_glyph.2).abs() < TOLERANCE);
    assert!((circle_glyph.3 - square_glyph.3).abs() < TOLERANCE);

    let bold_display = to_display_list(&bold_circle);
    assert!(bold_display.items.iter().any(|item| matches!(
        item,
        DisplayItem::GlyphPath {
            font,
            char_code,
            scale,
            ..
        } if font == "Main-Bold" && *char_code == '◯' as u32 && *scale < 1.0
    )));
}

#[test]
fn relational_eq() {
    check("a+b=c", 0.69444, 0.08333);
}

#[test]
fn superscript_x_squared() {
    check("x^2", 0.86411, 0.0);
}

#[test]
fn subscript_x_i() {
    check("x_i", 0.43056, 0.15);
}

#[test]
fn both_sup_and_sub() {
    check("x^2_i", 0.86411, 0.247);
}

#[test]
fn fraction_a_over_b() {
    check("\\frac{a}{b}", 1.10756, 0.686);
}

#[test]
fn fraction_1_over_2() {
    check("\\frac{1}{2}", 1.32144, 0.686);
}

#[test]
fn fraction_with_ops() {
    check("\\frac{x+y}{z}", 1.26033, 0.686);
}

#[test]
fn fraction_with_scripts() {
    check("\\frac{a^2}{b^2}", 1.49111, 0.686);
}

#[test]
fn sqrt_x() {
    check("\\sqrt{x}", 0.84916, 0.19084);
}

#[test]
fn sqrt_2() {
    check("\\sqrt{2}", 0.95610, 0.08390);
}

#[test]
fn sqrt_a_plus_b() {
    check("\\sqrt{a+b}", 0.93943, 0.10057);
}

#[test]
fn nested_frac_in_sqrt() {
    check("\\sqrt{\\frac{a}{b}}", 1.54466, 0.89535);
}

#[test]
fn nested_sqrt_in_frac() {
    check("\\frac{\\sqrt{a}}{b}", 1.47728, 0.686);
}

#[test]
fn nested_superscripts() {
    check("x^{x^2}", 1.03688, 0.0);
}

#[test]
fn quadratic_formula() {
    check("\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}", 1.59044, 0.686);
}

// ============================================================================
// Phase 4.2: Operator layout tests
// ============================================================================

#[test]
fn sum_with_limits() {
    check("\\sum_{i=1}^{n} x_i", 1.7314, 1.3577);
}

#[test]
fn int_nolimits() {
    check("\\int_0^1 f(x)\\,dx", 1.5641, 0.9119);
}

#[test]
fn prod_with_limits() {
    check("\\prod_{i=1}^{n} x_i", 1.7314, 1.3577);
}

#[test]
fn sum_standalone() {
    check("\\sum x", 1.05, 0.55);
}

#[test]
fn int_standalone() {
    check("\\int x", 1.36, 0.8622);
}

#[test]
fn lim_with_limits() {
    check("\\lim_{x\\to 0} \\frac{\\sin x}{x}", 1.3449, 0.7971);
}

#[test]
fn sum_infinity_series() {
    check("\\sum_{n=0}^{\\infty} a_n x^n", 1.7314, 1.3471);
}

#[test]
fn coprod_sub_only() {
    check("\\coprod_{i} A_i", 1.05, 1.3577);
}

#[test]
fn bigcap_sub_only() {
    check("\\bigcap_{i} A_i", 1.05, 1.3577);
}

#[test]
fn bigcup_sub_only() {
    check("\\bigcup_{i} A_i", 1.05, 1.3577);
}

#[test]
fn sin_text_op() {
    check("\\sin x", 0.6679, 0.0);
}

#[test]
fn cos_with_sup() {
    check("\\cos^2 \\theta", 0.8641, 0.0);
}

#[test]
fn det_text_op() {
    check("\\det A", 0.6944, 0.0);
}

#[test]
fn max_text_op() {
    check("\\max S", 0.6833, 0.0);
}

#[test]
fn int_with_explicit_limits() {
    check("\\int\\limits_2^2 3", 2.1922, 1.6582);
}

#[test]
fn explicit_limits_apply_in_text_style() {
    let default = layout_with_style("\\sum_{n=1}^{\\infty}", MathStyle::Text);
    let explicit = layout_with_style("\\sum\\limits_{n=1}^{\\infty}", MathStyle::Text);

    assert!(
        explicit.height > default.height + 0.5,
        "explicit limits should place superscript above in text style: default height {:.5}, explicit height {:.5}",
        default.height,
        explicit.height
    );
    assert!(
        explicit.depth > default.depth + 0.5,
        "explicit limits should place subscript below in text style: default depth {:.5}, explicit depth {:.5}",
        default.depth,
        explicit.depth
    );
}

// ============================================================================
// Phase 4.3: Accent layout tests
// ============================================================================

#[test]
fn accent_hat_x() {
    check("\\hat{x}", 0.78056, 0.0);
}

#[test]
fn accent_keeps_skew_for_multi_glyph_font_run() {
    use ratex_layout::layout_box::{BoxContent, LayoutBox};

    fn find_accent_skew(lbox: &LayoutBox) -> Option<f64> {
        match &lbox.content {
            BoxContent::Accent { skew, .. } => Some(*skew),
            BoxContent::HBox(children) => children.iter().find_map(find_accent_skew),
            BoxContent::Scaled { body, .. } | BoxContent::RaiseBox { body, .. } => {
                find_accent_skew(body)
            }
            _ => None,
        }
    }

    let options = LayoutOptions::default();
    let single = layout(&parse(r"\hat{\mathit{M}}").unwrap(), &options);
    let run = layout(&parse(r"\hat{\mathit{MM}}").unwrap(), &options);
    let single_skew = find_accent_skew(&single).expect("single-glyph accent");
    let run_skew = find_accent_skew(&run).expect("multi-glyph accent");

    assert!(single_skew > 0.0, "test glyph must have a non-zero skew");
    assert!(
        (run_skew - single_skew).abs() < f64::EPSILON,
        "font run lost the final glyph skew: single={single_skew}, run={run_skew}"
    );
}

#[test]
fn accent_bar_a() {
    check("\\bar{a}", 0.78056, 0.0);
}

#[test]
fn accent_tilde_n() {
    check("\\tilde{n}", 0.66786, 0.0);
}

#[test]
fn accent_nested_tilde_x() {
    // KaTeX 0.16.38 strut from tools/layout_compare/katex_layout.mjs
    check("\\tilde{\\tilde{x}}", 0.9047, 0.0);
}

#[test]
fn accent_nested_hat_x() {
    check("\\hat{\\hat{x}}", 0.9579, 0.0);
}

#[test]
fn accent_dot_y() {
    check("\\dot{y}", 0.78056, 0.1944);
}

#[test]
fn accent_ddot_x() {
    check("\\ddot{x}", 0.78056, 0.0);
}

// ============================================================================
// Phase 4.3: Left/Right delimiter tests
// ============================================================================

#[test]
fn left_right_simple() {
    check("\\left( x \\right)", 0.75, 0.25);
}

#[test]
fn left_right_frac() {
    check("\\left( \\frac{a}{b} \\right)", 1.15, 0.686);
}

#[test]
fn left_right_brackets() {
    check("\\left[ x^2 \\right]", 0.8641, 0.35);
}

#[test]
fn left_right_braces() {
    check("\\left\\{ a + b \\right\\}", 0.75, 0.25);
}

#[test]
fn left_right_bars() {
    check("\\left| x \\right|", 0.75, 0.25);
}

#[test]
fn left_right_sum() {
    // RaTeX clamps `\left`/`\right` height to `inner_height + inner_depth` after the
    // TeX formula; KaTeX 0.16.38 `makeLeftRightDelim` does not, so this differs from HTML parity.
    check("\\left( \\sum_{i=1}^{n} x_i \\right)", 1.79453, 1.3577);
}

// ============================================================================
// Phase 4.3: Delimiter sizing tests
// ============================================================================

#[test]
fn bigl_bigr() {
    check("\\bigl( x \\bigr)", 0.85, 0.35);
}

#[test]
fn big_bigl() {
    check("\\Bigl( x \\Bigr)", 1.15, 0.65);
}

#[test]
fn biggl_biggr() {
    check("\\biggl( x \\biggr)", 1.45, 0.95);
}

#[test]
fn biggest_biggl() {
    check("\\Biggl( x \\Biggr)", 1.75, 1.25);
}

// ============================================================================
// Phase 4.3: Array/Matrix tests
// ============================================================================

#[test]
fn pmatrix_2x2() {
    check(
        "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
        1.45,
        0.95,
    );
}

#[test]
fn bmatrix_identity() {
    check(
        "\\begin{bmatrix} 1 & 0 \\\\ 0 & 1 \\end{bmatrix}",
        1.45,
        0.95,
    );
}

#[test]
fn matrix_2x2() {
    check("\\begin{matrix} a & b \\\\ c & d \\end{matrix}", 1.45, 0.95);
}

// ============================================================================
// Phase 4.3: Text/Font tests
// ============================================================================

#[test]
fn text_hello() {
    check("\\text{hello}", 0.6944, 0.0);
}

#[test]
fn continuous_text_and_textrm_use_glyph_runs() {
    use ratex_layout::layout_box::{BoxContent, LayoutBox};

    fn longest_run(lbox: &LayoutBox) -> usize {
        match &lbox.content {
            BoxContent::GlyphRun { glyphs } => glyphs.len(),
            BoxContent::HBox(children) => children.iter().map(longest_run).max().unwrap_or(0),
            BoxContent::Scaled { body, .. } | BoxContent::RaiseBox { body, .. } => {
                longest_run(body)
            }
            _ => 0,
        }
    }

    for input in [r"\text{hello}", r"\textrm{hello}"] {
        let lbox = layout(&parse(input).unwrap(), &LayoutOptions::default());
        assert_eq!(
            longest_run(&lbox),
            5,
            "{input} should be laid out as one five-glyph run"
        );
    }
}

#[test]
fn inter_glyph_kern_changes_font_run_positions_and_width() {
    use ratex_layout::layout_box::{BoxContent, LayoutBox};

    fn glyph_positions(lbox: &LayoutBox) -> Option<Vec<f64>> {
        match &lbox.content {
            BoxContent::GlyphRun { glyphs } => Some(glyphs.iter().map(|glyph| glyph.x).collect()),
            BoxContent::HBox(children) => children.iter().find_map(glyph_positions),
            BoxContent::Scaled { body, .. } | BoxContent::RaiseBox { body, .. } => {
                glyph_positions(body)
            }
            _ => None,
        }
    }

    let ast = parse(r"\textrm{ab}").unwrap();
    let plain = layout(&ast, &LayoutOptions::default());
    let tracked = layout(&ast, &LayoutOptions::default().with_inter_glyph_kern(0.05));
    let plain_positions = glyph_positions(&plain).expect("plain glyph run");
    let tracked_positions = glyph_positions(&tracked).expect("tracked glyph run");

    assert!((tracked.width - plain.width - 0.05).abs() < TOLERANCE);
    assert!((tracked_positions[1] - plain_positions[1] - 0.05).abs() < TOLERANCE);
}

#[test]
fn mathrm_sin() {
    check("\\mathrm{sin}", 0.6679, 0.0);
}

#[test]
fn href_does_not_add_fixed_tracking_to_text_run() {
    let options = LayoutOptions::default();
    let linked = layout(
        &parse(r"\href{https://example.com}{\texttt{AaBb123}}").unwrap(),
        &options,
    );
    let plain = layout(&parse(r"\texttt{AaBb123}").unwrap(), &options);

    assert!(
        (linked.width - plain.width).abs() < TOLERANCE,
        "href changed text width: linked={:.5}, plain={:.5}",
        linked.width,
        plain.width
    );
}

/// `\mathrm{mm^{2}}` (e.g. mhchem `\pu{123 mm2}`): base of superscript must stay roman, not math italic.
#[test]
fn mathrm_mm_squared_both_m_upright() {
    use ratex_font::FontId;
    use ratex_layout::layout_box::{BoxContent, LayoutBox};

    fn collect_m_fonts(lb: &LayoutBox) -> Vec<FontId> {
        let mut v = Vec::new();
        match &lb.content {
            BoxContent::Glyph { font_id, char_code } if *char_code == 'm' as u32 => {
                v.push(*font_id);
            }
            BoxContent::GlyphRun { glyphs } => {
                v.extend(
                    glyphs
                        .iter()
                        .filter(|glyph| glyph.char_code == 'm' as u32)
                        .map(|glyph| glyph.font_id),
                );
            }
            BoxContent::HBox(children) => {
                for c in children {
                    v.extend(collect_m_fonts(c));
                }
            }
            BoxContent::SupSub { base, sup, sub, .. } => {
                v.extend(collect_m_fonts(base));
                if let Some(s) = sup {
                    v.extend(collect_m_fonts(s));
                }
                if let Some(s) = sub {
                    v.extend(collect_m_fonts(s));
                }
            }
            BoxContent::Scaled { body, .. } => v.extend(collect_m_fonts(body)),
            _ => {}
        }
        v
    }

    let ast = parse(r"\mathrm{mm^{2}}").expect("parse");
    let options = LayoutOptions::default();
    let lbox = layout(&ast, &options);
    let m_fonts = collect_m_fonts(&lbox);
    assert_eq!(m_fonts.len(), 2, "expected exactly two 'm' glyphs");
    assert!(
        m_fonts.iter().all(|&f| f == FontId::MainRegular),
        "both m should be MainRegular, got {m_fonts:?}"
    );
}

#[test]
fn subscript_italic_kern_only_applies_to_symbol_nodes() {
    use ratex_layout::layout_box::{BoxContent, LayoutBox};

    fn sub_h_kern(lb: &LayoutBox) -> Option<f64> {
        match &lb.content {
            BoxContent::SupSub { sub_h_kern, .. } => Some(*sub_h_kern),
            BoxContent::HBox(children) => children.iter().find_map(sub_h_kern),
            _ => None,
        }
    }

    fn kern_for(expr: &str) -> f64 {
        let ast = parse(expr).expect("parse");
        let lbox = layout(&ast, &LayoutOptions::default());
        sub_h_kern(&lbox).expect("supsub box")
    }

    assert!(
        kern_for(r"f_i") < 0.0,
        "a direct math symbol keeps its italic kern"
    );
    assert!(
        kern_for(r"\mathit{f}_i") < 0.0,
        "a single-glyph font node builds a direct symbol"
    );
    assert!(
        kern_for(r"\oiint_i") < 0.0,
        "KaTeX special-cases synthetic multi-integral operators"
    );

    for expr in [
        r"{f}_i",
        r"\mathit{ff}_i",
        r"\text{\textit{CPI}}_t",
        r"\color{red}{f}_i",
    ] {
        assert_eq!(
            kern_for(expr),
            0.0,
            "span/group base must not inherit a nested glyph's italic correction: {expr}"
        );
    }
}

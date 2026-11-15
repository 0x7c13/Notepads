// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Finite fixup only. Does not rerun or grant the original80 to changed sources.
use super::*;
use ratex_types::execution::{ExecutionError, Observer, Resource};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[test]
fn fixup_all_zero_resource_limits_propagate_the_original_typed_failure_without_panic() {
    for resource in [
        Resource::InputBytes,
        Resource::Tokens,
        Resource::ParserSteps,
        Resource::MacroExpansions,
        Resource::LayoutNodes,
        Resource::DisplayItems,
        Resource::OutlineSegments,
        Resource::GeneratedBytes,
    ] {
        let cancel = AtomicBool::new(false);
        let mut limits = MathLimits.execution();
        limits.values[resource as usize] = 0;
        let control = ExecutionControl::new(&cancel, None, limits);
        let storage = StorageLedger::new(&control);
        let input = if matches!(
            resource,
            Resource::MacroExpansions | Resource::GeneratedBytes
        ) {
            "~"
        } else {
            "x"
        };
        let result = pipeline(input, MathMode::Display, &storage, &FontAssets::new());
        let error = result.err().unwrap();
        let recorded = match error {
            MathError::Execution(error) => Some(error),
            MathError::Storage(ratex_types::storage::StorageError::Execution(error)) => Some(error),
            _ => None,
        };
        assert!(
            matches!(recorded,Some(ExecutionError::ResourceLimit{resource:actual,limit:0,..}) if actual==resource),
            "{resource:?}: {error:?}"
        );
    }
}

#[test]
fn fixup_fixed_neq_and_notin_aliases_keep_original_semantics_dimensions_and_real_outlines() {
    // Frozen before execution in evidence/fixup-1/INPUTS.json. Derived from the unmodified
    // KaTeX metric rows and original builtin expansion/hbox semantics, never Actual.
    for (tex, height, depth, width, glyphs) in [
        (r"\neq", 0.69444, 0.19444, 0.77778, [57376, 61]),
        (r"\ne", 0.69444, 0.19444, 0.77778, [57376, 61]),
        ("≠", 0.69444, 0.19444, 0.77778, [57376, 61]),
        (r"\notin", 0.75, 0.25, 0.66667, [8712, 47]),
        ("∉", 0.75, 0.25, 0.66667, [8712, 47]),
    ] {
        for style in [MathStyle::Display, MathStyle::Text] {
            let cancel = AtomicBool::new(false);
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage)
                .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
            let options = ratex_layout::LayoutOptions {
                style,
                ..Default::default()
            };
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            let codes = display
                .items
                .iter()
                .filter_map(|item| match item {
                    ratex_types::DisplayItem::GlyphPath { char_code, .. } => Some(*char_code),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(codes, glyphs, "{tex} {style:?}");
            assert!(
                (display.height - height).abs() < 1e-10,
                "{tex}: {}",
                display.height
            );
            assert!(
                (display.depth - depth).abs() < 1e-10,
                "{tex}: {}",
                display.depth
            );
            assert!(
                (display.width - width).abs() < 1e-10,
                "{tex}: {}",
                display.width
            );
            let artifact = outline_drawing(&display, &FontAssets::new(), &storage).unwrap();
            assert_eq!(artifact.primitives.len(), 2, "{tex}");
            assert!(!artifact.commands.is_empty(), "{tex}");
        }
    }
}

#[test]
fn fixup_user_commands_and_substituted_arguments_never_gain_builtin_trust() {
    for tex in [
        r"\html@mathml{x}{y}",
        r"\mathchoice{x}{y}{z}{w}",
        r"\mathrel{x}",
        r"\char`a",
        r"\@char{65}",
        r"\mskip1mu",
        r"\htmlStyle{color:red}{x}",
        r"\hphantom{\html@mathml{x}{y}}",
    ] {
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let error = ratex_parser::parse_with_storage(tex, &storage).unwrap_err();
        assert!(
            matches!(
                error,
                ratex_parser::ControlledParseError::Execution(ExecutionError::Unsupported { .. })
                    | ratex_parser::ControlledParseError::Storage(
                        ratex_types::storage::StorageError::Execution(
                            ExecutionError::Unsupported { .. }
                        )
                    )
            ),
            "{tex}: {error:?}"
        );
    }
}

struct Gate {
    phase: Phase,
    hit: Cell<bool>,
    reached: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}
impl Observer for Gate {
    fn checkpoint(&self, phase: Phase, ordinal: u64) {
        if phase == self.phase && ordinal == 4 && !self.hit.replace(true) {
            let _ = self.reached.send(());
            let _ = self.release.recv_timeout(Duration::from_secs(5));
        }
    }
}
#[test]
fn fixup_real_external_phase_cancellation_returns_after_all_pipeline_buffers_drop() {
    for phase in [
        Phase::Lexer,
        Phase::Parser,
        Phase::Macro,
        Phase::Layout,
        Phase::Validate,
        Phase::Display,
        Phase::Outline,
    ] {
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (reached_tx, reached_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                let gate = Gate {
                    phase,
                    hit: Cell::new(false),
                    reached: reached_tx,
                    release: release_rx,
                };
                let control = ExecutionControl::new(
                    &worker_cancel,
                    Some(Instant::now() + Duration::from_secs(5)),
                    MathLimits.execution(),
                )
                .with_observer(&gate);
                let storage = StorageLedger::new(&control);
                let result = pipeline(
                    r"\frac{x^2+1}{\sqrt{y}}+\sum_{i=0}^{n}i",
                    MathMode::Display,
                    &storage,
                    &FontAssets::new(),
                );
                let error = result.err();
                (Instant::now(), error)
            })
            .unwrap();
        let reached = reached_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        let requested = Instant::now();
        cancel.store(true, Ordering::Release);
        let _ = release_tx.send(());
        let (returned, error) = worker.join().unwrap();
        let tail = returned.checked_duration_since(requested);
        assert!(reached, "{phase:?} not reached");
        assert!(
            tail.is_some_and(|duration| duration <= Duration::from_millis(750)),
            "{phase:?}: {tail:?}"
        );
        let cancelled_phase = match error {
            Some(MathError::Execution(ExecutionError::Cancelled { phase }))
            | Some(MathError::Storage(ratex_types::storage::StorageError::Execution(
                ExecutionError::Cancelled { phase },
            ))) => Some(phase),
            _ => None,
        };
        assert_eq!(cancelled_phase, Some(phase), "{phase:?}: {error:?}");
        println!(
            "external_phase={phase:?} cancel_to_buffer_return_us={} scope=phase_gate_not_midopaque_or_UWP",
            tail.unwrap().as_micros()
        );
    }
}

#[test]
fn fixup3_original_integral_and_text_mode_thinspace_reach_real_outline() {
    // Independent original registry3-mode oracle was frozen before source edits.
    // KaTeX's display strut is a separate <0.002em oracle, not full drawing parity.
    for (tex, mode, height, depth, width, primitives) in [
        (
            r"\int_0^1 f(x)\,dx",
            MathMode::Display,
            1.5651329999999999,
            0.910825,
            4.771903333333333,
            9,
        ),
        (
            r"\int_0^1 f(x)\,dx",
            MathMode::Inline,
            1.0090080000000001,
            0.35582,
            4.438563333333334,
            9,
        ),
        (r"\text{\,}", MathMode::Display, 0.0, 0.0, 0.1667, 0),
    ] {
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let result = pipeline(tex, mode, &storage, &FontAssets::new()).unwrap();
        assert!((result.metrics.ascent - height).abs() < 1e-10, "{tex}");
        assert!((result.metrics.descent - depth).abs() < 1e-10, "{tex}");
        assert!((result.metrics.advance - width).abs() < 1e-10, "{tex}");
        assert_eq!(result.primitives.len(), primitives, "{tex}");
        if primitives != 0 {
            assert!(!result.commands.is_empty());
        }
        if tex.starts_with(r"\int") && matches!(mode, MathMode::Display) {
            assert!((result.metrics.ascent - 1.5641).abs() < 0.002);
            assert!((result.metrics.descent - 0.9119).abs() < 0.002);
        }
    }
}

#[test]
fn fixed_spacing_commands_reach_real_outlines_with_their_tex_widths() {
    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for mode in [MathMode::Inline, MathMode::Display] {
        let render = |tex: &str| {
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            pipeline(tex, mode, &storage, &fonts).unwrap_or_else(|error| panic!("{tex}: {error:?}"))
        };
        let baseline = render("ab");
        for (tex, width) in [
            (r"a\enskip b", 0.5),
            (r"a\quad b", 1.0),
            (r"a\qquad b", 2.0),
        ] {
            let result = render(tex);
            assert!(
                (result.metrics.advance - baseline.metrics.advance - width).abs() < 1e-10,
                "{tex}"
            );
            assert_eq!(result.primitives.len(), baseline.primitives.len(), "{tex}");
        }
        let tex = r"\int_0^1 f(x)\,dx\qquad\sum_{n=1}^{\infty}\frac{1}{n^2}";
        let result = render(tex);
        assert!(!result.commands.is_empty(), "{tex}");

        for tex in [
            r"a\hskip1em b",
            r"a\kern1em b",
            r"a\relax b",
            r"\hphantom{a\relax b}",
        ] {
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let result = pipeline(tex, mode, &storage, &fonts);
            assert!(
                matches!(
                    result,
                    Err(MathError::Execution(ExecutionError::Unsupported {
                        phase: ratex_types::execution::Phase::Parser,
                    }))
                ),
                "{tex}"
            );
        }
    }
}

#[test]
fn operator_names_preserve_roman_ink_spacing_and_script_placement() {
    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for tex in [
            r"\operatorname{Cov}(X,Y)",
            r"\operatorname{A\&\Gamma\alpha}",
            r"x+\operatorname{rank}_{n}A",
            r"\operatorname*{argmax}_{x}^{n} f(x)",
            r"\operatorname{lim}\limits_{x} x",
            r"\operatorname*{lim}\nolimits_{x} x",
            r"\operatorname{A\frac{1}{2}} x",
            r"\operatorname x",
            r"\argmax_{x} f(x)",
        ] {
            let original = ratex_parser::parse(tex).unwrap();
            let expected =
                ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage)
                .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            assert_eq!(display.items, expected.items, "{style:?}: {tex}");
            assert_eq!(
                (display.width, display.height, display.depth),
                (expected.width, expected.height, expected.depth),
                "{style:?}: {tex}"
            );
            if tex.starts_with(r"\operatorname{Cov}") {
                for (item, letter) in display.items.iter().zip("Cov".chars()) {
                    assert!(
                        matches!(item, ratex_types::DisplayItem::GlyphPath { font, char_code, .. }
                            if font == "Main-Regular" && *char_code == u32::from(letter)),
                        "{item:?}"
                    );
                }
            }
            if tex == r"\operatorname{A\&\Gamma\alpha}" {
                let glyphs: Vec<_> = display
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        ratex_types::DisplayItem::GlyphPath { char_code, .. } => Some(*char_code),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    glyphs,
                    ['A', '&', 'Γ', 'α'].map(u32::from),
                    "{style:?}: {tex}"
                );
            }
            let artifact = outline_drawing(&display, &fonts, &storage).unwrap();
            assert!(!artifact.commands.is_empty(), "{tex}");
        }
    }
}

#[test]
fn operator_name_internals_and_budget_stay_bounded() {
    use ratex_types::{execution::Phase, storage::StorageFamily};

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for (tex, phase) in [
        (r"\@ifstar{x}{y}*", Phase::Macro),
        (r"\operatorname@{Cov}", Phase::Parser),
        (r"\operatornamewithlimits{lim}", Phase::Parser),
        (r"\operatorname{\hskip1em}", Phase::Parser),
    ] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(
            matches!(
                pipeline(tex, MathMode::Display, &storage, &fonts),
                Err(MathError::Execution(ExecutionError::Unsupported { phase: actual }))
                    if actual == phase
            ),
            "{tex}"
        );
    }

    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(r"\operatorname{", MathMode::Display, &storage, &fonts),
        Err(MathError::ParseFailure)
    ));

    let mut limits = MathLimits.execution();
    limits.values[Resource::LayoutNodes as usize] = 32;
    let control = ExecutionControl::new(&cancel, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(
            r"\operatorname{abcdefghijklmnopqrstuvwxyz}",
            MathMode::Display,
            &storage,
            &fonts
        ),
        Err(MathError::Storage(
            ratex_types::storage::StorageError::Execution(ExecutionError::ResourceLimit {
                phase: Phase::Layout,
                resource: Resource::LayoutNodes,
                limit: 32,
                ..
            })
        ))
    ));
    assert!(storage.used_bytes()[StorageFamily::Layout as usize] > 0);
}

#[test]
fn plain_matrices_preserve_grid_geometry_in_the_bounded_native_outline() {
    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for tex in [
            r"\begin{matrix}1&22\\33&4\end{matrix}",
            r"\begin{matrix}a&\frac{1}{2}\\x_1\end{matrix}",
            r"x_{\begin{matrix}a&b\\c&d\end{matrix}}",
            r"\begin{matrix}\begin{matrix}1\\2\end{matrix}&3\end{matrix}",
        ] {
            let original = ratex_parser::parse(tex).unwrap();
            let expected =
                ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage)
                .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let actual = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            assert!((actual.width - expected.width).abs() < 1e-10, "{tex}");
            assert!((actual.height - expected.height).abs() < 1e-10, "{tex}");
            assert!((actual.depth - expected.depth).abs() < 1e-10, "{tex}");
            assert_eq!(actual.items, expected.items, "{style:?}: {tex}");
            let artifact = outline_drawing(&actual, &fonts, &storage).unwrap();
            assert_eq!(artifact.primitives.len(), expected.items.len(), "{tex}");
        }
    }
}

#[test]
fn plain_matrix_separators_and_work_are_bounded() {
    use ratex_types::execution::Phase;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let render = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        pipeline(tex, MathMode::Display, &storage, &fonts)
    };
    let matrix = render(r"\begin{matrix}a\&b&c\\d&\end{matrix}").unwrap();
    assert_eq!(matrix.primitives.len(), 5);
    assert!((matrix.metrics.ascent + matrix.metrics.descent - 2.4).abs() < 1e-10);
    let trailing = render(r"\begin{matrix}a\&b&c\\d&\\\end{matrix}").unwrap();
    assert_eq!(trailing.metrics, matrix.metrics);
    assert_eq!(trailing.commands, matrix.commands);
    let raw_break = render(r"\begin{matrix}a\\ [b]\end{matrix}").unwrap();
    let builtin_break = render(r"\begin{matrix}a\newline[b]\end{matrix}").unwrap();
    assert_eq!(raw_break.metrics, builtin_break.metrics);
    assert_eq!(raw_break.commands, builtin_break.commands);
    for tex in [
        r"\begin{matrix}a&b\end{pmatrix}",
        r"\begin{matrix}a&b",
        r"\begin{matrix}a&b}",
        r"\begin{matrix",
    ] {
        assert!(matches!(render(tex), Err(MathError::ParseFailure)), "{tex}");
    }
    for tex in [
        r"\begin{pmatrix*}a&b\end{pmatrix*}",
        r"\begin{matrix}a\\[1pt]b\end{matrix}",
    ] {
        assert!(
            matches!(
                render(tex),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Parser
                }))
            ),
            "{tex}"
        );
    }
    let oversized = format!(
        r"\begin{{matrix}}{}a{}\end{{matrix}}",
        "a&".repeat(100),
        r"\\a".repeat(100)
    );
    assert!(oversized.len() < MathLimits::INPUT_BYTES);
    assert!(matches!(
        render(&oversized),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Validate,
            resource: Resource::LayoutNodes,
            ..
        }))
    ));
    let nested = format!(
        "{}x{}",
        r"\begin{matrix}a\\".repeat(17),
        r"\end{matrix}".repeat(17)
    );
    assert!(matches!(
        render(&nested),
        Err(MathError::Storage(
            ratex_types::storage::StorageError::DepthLimit {
                phase: Phase::Parser,
                limit: 16,
                ..
            }
        ))
    ));

    let mut limits = MathLimits.execution();
    limits.values[Resource::ParserSteps as usize] = 1;
    let control = ExecutionControl::new(&cancel, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(
            r"\begin{matrix}&&\end{matrix}",
            MathMode::Display,
            &storage,
            &fonts
        ),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Parser,
            resource: Resource::ParserSteps,
            limit: 1,
            requested: 2,
        }))
    ));
}

#[test]
fn delimited_matrices_keep_small_reference_geometry_and_stretch_tall_rows() {
    use ratex_types::display_item::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for environment in ["pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix"] {
            for rows in [1, 2, 3, 6] {
                let cells = vec!["a&b"; rows].join(r"\\");
                let tex = format!(r"\begin{{{environment}}}{cells}\end{{{environment}}}");
                let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
                let storage = StorageLedger::new(&control);
                let ast = ratex_parser::parse_with_storage(&tex, &storage).unwrap();
                let layout = ratex_layout::layout_with_storage(&ast, &options, &storage)
                    .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
                ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
                let actual = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
                let artifact = outline_drawing(&actual, &fonts, &storage).unwrap();
                assert!(artifact.primitives.len() > 2 * rows, "{tex}");
                assert!(actual.height + actual.depth >= 1.2 * rows as f64 - 1e-10, "{tex}");
                let ink = artifact.metrics.ink;
                assert!(ink.left >= -1e-10, "{tex}: {ink:?}");
                assert!(ink.right <= artifact.metrics.advance + 1e-10, "{tex}: {ink:?}");
                let plain = ratex_parser::parse(&format!(r"\begin{{matrix}}{cells}\end{{matrix}}"))
                    .unwrap();
                let grid = ratex_layout::to_display_list(&ratex_layout::layout(&plain, &options));
                let left_advance = (actual.width - grid.width) / 2.0;
                let cell_positions = |items: &[DisplayItem]| {
                    items
                        .iter()
                        .filter_map(|item| match item {
                            DisplayItem::GlyphPath { x, y, char_code, .. }
                                if matches!(*char_code, 97 | 98) =>
                            {
                                Some((*char_code, *x, *y))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let positions = cell_positions(&actual.items);
                assert_eq!(positions.len(), 2 * rows, "{tex}");
                for ((code, x, y), (expected_code, expected_x, expected_y)) in
                    positions.into_iter().zip(cell_positions(&grid.items))
                {
                    assert_eq!(code, expected_code, "{tex}");
                    assert!((x - left_advance - expected_x).abs() < 1e-10, "{tex}");
                    assert!(((y - actual.height) - (expected_y - grid.height)).abs() < 1e-10, "{tex}");
                }
                let stems = actual.items.iter()
                    .filter(|item| matches!(item, DisplayItem::Rect { .. }))
                    .count();
                let expected_stems = match environment {
                    "vmatrix" => 2,
                    "Vmatrix" => 4,
                    "Bmatrix" if rows >= 3 => 4,
                    _ if rows >= 3 => 2,
                    _ => 0,
                };
                assert_eq!(stems, expected_stems, "{tex}");
                if rows <= 2 && matches!(environment, "pmatrix" | "bmatrix" | "Bmatrix") {
                    let original = ratex_parser::parse(&tex).unwrap();
                    let expected =
                        ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
                    assert_eq!(actual.items, expected.items, "{style:?}: {tex}");
                    assert!((actual.width - expected.width).abs() < 1e-10, "{tex}");
                    assert!((actual.height - expected.height).abs() < 1e-10, "{tex}");
                    assert!((actual.depth - expected.depth).abs() < 1e-10, "{tex}");
                }
            }
        }
    }
}

#[test]
fn delimited_matrix_names_nesting_and_height_keep_typed_boundaries() {
    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let render = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        pipeline(tex, MathMode::Display, &storage, &fonts)
    };
    let explicit = render(r"\left(\begin{matrix}a&b\\c&d\\e&f\end{matrix}\right)").unwrap();
    let environment = render(r"\begin{pmatrix}a&b\\c&d\\e&f\end{pmatrix}").unwrap();
    assert_eq!(explicit.metrics, environment.metrics);
    assert_eq!(explicit.commands, environment.commands);
    let nested = render(
        r"x_{\begin{Bmatrix}\begin{vmatrix}\frac{1}{2}\\x\end{vmatrix}&y\end{Bmatrix}}",
    );
    assert!(nested.is_ok(), "{:?}", nested.err());
    for tex in [
        r"\begin{pmatrix}a\end{bmatrix}",
        r"\begin{Vmatrix}a\end{vmatrix}",
        r"\begin{Bmatrix}a\end{Bmat",
    ] {
        assert!(matches!(render(tex), Err(MathError::ParseFailure)), "{tex}");
    }
    let rows = vec!["a"; 54].join(r"\\");
    let oversized = format!(r"\begin{{vmatrix}}{rows}\end{{vmatrix}}");
    let result = render(&oversized);
    assert!(
        matches!(
            result,
            Err(MathError::Storage(
                ratex_types::storage::StorageError::Execution(ExecutionError::Unsupported {
                    phase: ratex_types::execution::Phase::Layout,
                })
            ))
        ),
        "{:?}",
        result.err()
    );
}

#[test]
fn fixup3_thinspace_preserves_typed_zero_resource_first_failure() {
    for resource in [
        Resource::Tokens,
        Resource::MacroExpansions,
        Resource::GeneratedBytes,
    ] {
        let cancel = AtomicBool::new(false);
        let mut limits = MathLimits.execution();
        limits.values[resource as usize] = 0;
        let control = ExecutionControl::new(&cancel, None, limits);
        let storage = StorageLedger::new(&control);
        let error = pipeline(r"\,", MathMode::Display, &storage, &FontAssets::new())
            .err()
            .unwrap();
        let recorded = match error {
            MathError::Execution(error) => Some(error),
            MathError::Storage(ratex_types::storage::StorageError::Execution(error)) => Some(error),
            _ => None,
        };
        assert!(
            matches!(recorded, Some(ExecutionError::ResourceLimit { resource: actual, limit: 0, .. }) if actual == resource),
            "{resource:?}: {error:?}"
        );
    }
}

#[test]
fn fixup3_user_text_or_math_and_parameter_injection_remain_unsupported() {
    for tex in [
        r"\TextOrMath{x}{y}",
        r"\hphantom{\TextOrMath{x}{y}}",
        r"\mskip{3mu}",
    ] {
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let error = pipeline(tex, MathMode::Display, &storage, &FontAssets::new())
            .err()
            .unwrap();
        assert!(
            matches!(
                error,
                MathError::Execution(ExecutionError::Unsupported { .. })
                    | MathError::Storage(ratex_types::storage::StorageError::Execution(
                        ExecutionError::Unsupported { .. }
                    ))
            ),
            "{tex}: {error:?}"
        );
    }
}

#[test]
fn cases_rows_match_reference_spacing_and_align_the_first_two_columns_left() {
    use ratex_types::display_item::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for (cells, reference_cells, tall) in [
            (
                r"\frac{1}{2}&\text{if }x>0",
                r"\frac{1}{2}&\text{if }x>0",
                false,
            ),
            (r"a", r"a", false),
            (r"a&bc\\de&f", r"a&bc\\de&f", false),
            (r"a&", r"a&{}", false),
            (r"a\\b&cc\\ddd&e", r"a\\b&cc\\ddd&e", true),
            (r"a&b&c\\dd&ee&ff", r"a&b&c\\dd&ee&ff", false),
        ] {
            let tex = format!(r"\begin{{cases}}{cells}\end{{cases}}");
            let reference = format!(r"\begin{{cases}}{reference_cells}\end{{cases}}");
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(&tex, &storage).unwrap();
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let mut actual = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            let original = ratex_parser::parse(&reference).unwrap();
            let expected =
                ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
            let artifact = outline_drawing(&actual, &fonts, &storage).unwrap();
            let count_stems = actual
                .items
                .iter()
                .filter(|item| matches!(item, DisplayItem::Rect { .. }))
                .count();
            assert_eq!(count_stems, if tall { 2 } else { 0 }, "{tex}");
            if tall {
                assert!(actual.height + actual.depth >= 3.0 * 1.44 - 1e-10);
                let cell_positions = |items: &[DisplayItem], height| {
                    items
                        .iter()
                        .filter_map(|item| match item {
                            DisplayItem::GlyphPath {
                                char_code, x, y, ..
                            } if (b'a' as u32..=b'z' as u32).contains(char_code) => {
                                Some((*char_code, *x, *y - height))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let cells = cell_positions(&actual.items, actual.height);
                let reference_cells = cell_positions(&expected.items, expected.height);
                let x_offset = cells[0].1 - reference_cells[0].1;
                assert_eq!(cells.len(), reference_cells.len(), "{tex}");
                for ((code, x, y), (reference_code, reference_x, reference_y)) in
                    cells.into_iter().zip(reference_cells)
                {
                    assert_eq!(code, reference_code, "{tex}");
                    assert!((x - x_offset - reference_x).abs() < 1e-10, "{tex}");
                    assert!((y - reference_y).abs() < 1e-10, "{tex}");
                }
                assert!(artifact.metrics.ink.right <= artifact.metrics.advance + 1e-10);
            } else {
                assert_eq!(actual.items.len(), expected.items.len(), "{tex}");
                for (item, reference) in actual.items.iter_mut().zip(&expected.items) {
                    if let (
                        DisplayItem::GlyphPath { x, y, .. },
                        DisplayItem::GlyphPath {
                            x: reference_x,
                            y: reference_y,
                            ..
                        },
                    ) = (item, reference)
                    {
                        assert!((*x - reference_x).abs() < 1e-10, "{tex}");
                        assert!((*y - reference_y).abs() < 1e-10, "{tex}");
                        *x = *reference_x;
                        *y = *reference_y;
                    }
                }
                assert_eq!(actual.items, expected.items, "{style:?}: {tex}");
                assert!((actual.width - expected.width).abs() < 1e-10, "{tex}");
                assert!((actual.height - expected.height).abs() < 1e-10, "{tex}");
                assert!((actual.depth - expected.depth).abs() < 1e-10, "{tex}");
            }
        }
    }
}

#[test]
fn cases_environment_keeps_name_and_height_boundaries() {
    use ratex_types::execution::Phase;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let render = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        pipeline(tex, MathMode::Display, &storage, &fonts)
    };
    let nested = render(r"x_{\begin{cases}\begin{pmatrix}a\\b\end{pmatrix}&x\\c&y\end{cases}}");
    assert!(nested.is_ok(), "{:?}", nested.err());
    let trailing = render(r"\begin{cases}a&\\\end{cases}").unwrap();
    let explicit = render(r"\begin{cases}a&\end{cases}").unwrap();
    assert_eq!(trailing.metrics, explicit.metrics);
    assert_eq!(trailing.commands, explicit.commands);
    for tex in [
        r"\begin{cases}a\end{matrix}",
        r"\begin{Bmatrix}a\end{cases}",
        r"\begin{cases}a\end{case",
    ] {
        assert!(matches!(render(tex), Err(MathError::ParseFailure)), "{tex}");
    }
    for tex in [
        r"\begin{cases*}a\end{cases*}",
        r"\begin{dcases}a\end{dcases}",
        r"\begin{rcases}a\end{rcases}",
    ] {
        assert!(
            matches!(
                render(tex),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Parser
                }))
            ),
            "{tex}"
        );
    }
    let rows = vec!["a"; 45].join(r"\\");
    let oversized = format!(r"\begin{{cases}}{rows}\end{{cases}}");
    let explicit = format!(r"\left\{{\begin{{matrix}}{rows}\end{{matrix}}\right.");
    for tex in [&oversized, &explicit] {
        assert!(
            matches!(
                render(tex),
                Err(MathError::Storage(
                    ratex_types::storage::StorageError::Execution(ExecutionError::Unsupported {
                        phase: Phase::Layout,
                    })
                ))
            ),
            "{tex}"
        );
    }
}

#[test]
fn aligned_matches_reference_pair_spacing_and_includes_the_final_jot() {
    use ratex_types::display_item::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for (cells, reference_cells, height, depth) in [
            (r"a&=b", r"a&=b", 1.0, 0.5),
            (r"a&=b\\c+d&=e", r"a&=b\\c+d&=e", 1.75, 1.25),
            (
                r"a&=b&cd&=-e\\fg&=h&i&=j",
                r"a&=b&cd&=-e\\fg&=h&i&=j",
                1.75,
                1.25,
            ),
            (
                r"a&=b&cd&=e\\fg&=h\\i&",
                r"a&=b&cd&=e\\fg&=h\\i&{}",
                2.5,
                2.0,
            ),
            (r"a&=b\\", r"a&=b", 1.0, 0.5),
            (r"a&+b", r"a&+b", 1.0, 0.5),
            ("", "", 1.0, 0.5),
        ] {
            let tex = format!(r"\begin{{aligned}}{cells}\end{{aligned}}");
            let reference = format!(r"\begin{{aligned}}{reference_cells}\end{{aligned}}");
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(&tex, &storage).unwrap();
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let mut actual = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            assert!((actual.height - height).abs() < 1e-10, "{tex}: {}", actual.height);
            assert!((actual.depth - depth).abs() < 1e-10, "{tex}: {}", actual.depth);
            let original = ratex_parser::parse(&reference).unwrap();
            let expected =
                ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
            assert!((actual.width - expected.width).abs() < 1e-10, "{tex}");
            assert_eq!(actual.items.len(), expected.items.len(), "{tex}");
            if !actual.items.is_empty() {
                let artifact = outline_drawing(&actual, &fonts, &storage).unwrap();
                assert_eq!(artifact.primitives.len(), actual.items.len(), "{tex}");
            }
            for (item, reference) in actual.items.iter_mut().zip(&expected.items) {
                if let (
                    DisplayItem::GlyphPath { x, y, .. },
                    DisplayItem::GlyphPath {
                        x: reference_x,
                        y: reference_y,
                        ..
                    },
                ) = (item, reference)
                {
                    assert!((*x - reference_x).abs() < 1e-10, "{tex}");
                    assert!((*y - reference_y).abs() < 1e-10, "{tex}");
                    *x = *reference_x;
                    *y = *reference_y;
                }
            }
            assert_eq!(actual.items, expected.items, "{style:?}: {tex}");
        }
    }
}

#[test]
fn aligned_supports_nested_cells_and_preserves_typed_environment_boundaries() {
    use ratex_types::execution::Phase;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let render = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        pipeline(tex, MathMode::Display, &storage, &fonts)
    };
    for tex in [
        r"\begin{aligned}a\&b&=c\\d&=\frac{1}{2}\end{aligned}",
        r"x_{\begin{aligned}a&=\begin{pmatrix}1\\2\end{pmatrix}\\b&=0\end{aligned}}",
    ] {
        assert!(render(tex).is_ok(), "{tex}");
    }
    for tex in [
        r"\begin{aligned}a&=b\end{matrix}",
        r"\begin{matrix}a\end{aligned}",
        r"\begin{aligned}a\end{aligne",
    ] {
        assert!(matches!(render(tex), Err(MathError::ParseFailure)), "{tex}");
    }
    for tex in [
        r"\begin{alignedat}{1}a&=b\end{alignedat}",
        r"\begin{align}a&=b\end{align}",
        r"\begin{split}a&=b\end{split}",
        r"\begin{aligned}a&=b\\[1pt]c&=d\end{aligned}",
    ] {
        assert!(
            matches!(
                render(tex),
                Err(MathError::Execution(ExecutionError::Unsupported { phase: Phase::Parser }))
            ),
            "{tex}"
        );
    }
}

#[test]
fn vector_accent_uses_the_fixed_outline_and_katex_placement() {
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        for (tex, ascent, skew) in [
            (r"\vec{x}", 0.714, 0.02778),
            (r"\vec{AB}", 0.96633, 0.0),
            (r"\vec{\mathbf{x}}", 0.72744, 0.02778),
            (r"\mathbf{\vec{x}}", 0.72744, 0.0),
            (r"\vec{=}", 0.714, 0.0),
            (r"\vec{}", 0.714, 0.0),
        ] {
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            assert!(
                (display.height - ascent).abs() < 1e-5,
                "{tex}: {}",
                display.height
            );
            assert!(display.depth.abs() < 1e-10, "{tex}: {}", display.depth);
            let Some(DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code: 0x20D7,
                ..
            }) = display.items.last()
            else {
                panic!("missing vector glyph: {tex}");
            };
            assert_eq!(font, "Main-Regular", "{tex}");
            assert_eq!(*scale, 1.0, "{tex}");
            assert!(
                (x - (display.width / 2.0 + 0.2645 + skew)).abs() < 1e-10,
                "{tex}: {x}"
            );
            assert!((y - 0.714).abs() < 1e-10, "{tex}: {y}");
            let artifact = outline_drawing(&display, &fonts, &storage).unwrap();
            assert!(
                artifact.metrics.ink.top <= 1e-6,
                "{tex}: {:?}",
                artifact.metrics.ink
            );
            if tex == r"\vec{}" {
                assert_eq!(artifact.metrics.advance, 0.0);
                assert_eq!(artifact.primitives.len(), 1);
                assert!((artifact.metrics.ink.left + 0.2065).abs() < 1e-6);
                assert!((artifact.metrics.ink.right - 0.2355).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn vector_accent_preserves_character_scripts_and_bounded_fallback() {
    use ratex_types::execution::Phase;
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let display = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
        let layout = ratex_layout::layout_with_storage(
            &ast,
            &ratex_layout::LayoutOptions::default(),
            &storage,
        )
        .unwrap();
        ratex_layout::to_display_list_with_control(&layout, &control).unwrap()
    };
    let glyph = |result: &ratex_types::DisplayList, code| {
        result
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::GlyphPath {
                    char_code,
                    x,
                    y,
                    scale,
                    ..
                } if *char_code == code => Some((*x, *y - result.height, *scale)),
                _ => None,
            })
            .unwrap()
    };
    for (accented, bare) in [
        (r"\vec{f}_i^2", "f_i^2"),
        (r"\vec{{f}}_i^2", "{f}_i^2"),
        (r"\mathbf{\vec{f}_i^2}", r"\mathbf{f_i^2}"),
    ] {
        let actual = display(accented);
        let reference = display(bare);
        for code in [u32::from('i'), u32::from('2')] {
            let (x, y, scale) = glyph(&actual, code);
            let (expected_x, expected_y, expected_scale) = glyph(&reference, code);
            assert!((x - expected_x).abs() < 1e-10, "{accented}");
            assert!((y - expected_y).abs() < 1e-10, "{accented}");
            assert_eq!(scale, expected_scale, "{accented}");
        }
    }
    assert!(
        glyph(&display(r"\vec{AB}^2"), u32::from('2')).1
            < glyph(&display("{AB}^2"), u32::from('2')).1
    );
    assert_eq!(glyph(&display(r"x^{\vec{v}}"), 0x20D7).2, 0.7);
    assert!((display(r"\vec{\vec{x}}").height - 0.997).abs() < 1e-10);
    let render = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        pipeline(tex, MathMode::Display, &storage, &fonts)
    };
    for tex in [
        r"\vec{i}",
        r"\vec{\imath}",
        r"\vec{\frac{1}{2}}",
        r"\vec{\sum_{k=1}^{n}a_k}",
    ] {
        assert!(render(tex).is_ok(), "{tex}");
    }
    for tex in [r"\hat{x}", r"\overrightarrow{AB}", r"\underrightarrow{AB}"] {
        assert!(
            matches!(
                render(tex),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Parser
                }))
            ),
            "{tex}"
        );
    }
    let nested = format!("{}x{}", r"\vec{".repeat(17), "}".repeat(17));
    let result = render(&nested);
    assert!(
        matches!(
            &result,
            Err(MathError::Storage(
                ratex_types::storage::StorageError::DepthLimit {
                    phase: Phase::Macro,
                    ..
                }
            ))
        ),
        "{:?}",
        result.err()
    );
    assert!(render(r"x\!\!\!\!").is_ok());
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    let result = pipeline(r"\vec{}", MathMode::Inline, &storage, &fonts).unwrap();
    assert_eq!(result.metrics.advance, 0.0);
    assert!(result.metrics.ink.left < 0.0 && result.metrics.ink.right > 0.0);
    let mut limits = MathLimits.execution();
    limits.values[Resource::LayoutNodes as usize] = 0;
    let control = ExecutionControl::new(&cancel, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(r"\vec{x}", MathMode::Display, &storage, &fonts),
        Err(MathError::Storage(
            ratex_types::storage::StorageError::Execution(ExecutionError::ResourceLimit {
                resource: Resource::LayoutNodes,
                limit: 0,
                ..
            })
        )) | Err(MathError::Execution(ExecutionError::ResourceLimit {
            resource: Resource::LayoutNodes,
            limit: 0,
            ..
        }))
    ));
}

#[test]
fn bounded_zero_advance_keeps_ink_while_other_display_callers_keep_their_metrics() {
    use ratex_layout::layout_box::{BoxContent, LayoutBox};
    use ratex_types::color::Color;

    let cancel = AtomicBool::new(false);
    for width in [0.0, -0.2] {
        let glyph = LayoutBox {
            width,
            height: 0.43056,
            depth: 0.0,
            content: BoxContent::Glyph {
                font_id: ratex_font::FontId::MathItalic,
                char_code: u32::from('x'),
            },
            color: Color::BLACK,
        };
        let legacy = ratex_layout::to_display_list(&glyph);
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let controlled = ratex_layout::to_display_list_with_control(&glyph, &control).unwrap();
        assert_eq!(controlled.width, legacy.width);
        assert_eq!(controlled.items, legacy.items);
        let storage = StorageLedger::new(&control);
        let bounded = ratex_layout::to_display_list_with_control(&glyph, &control).unwrap();
        assert_eq!(bounded.items, legacy.items);
        if width == 0.0 {
            assert_eq!(bounded.width, 0.0);
            let artifact = outline_drawing(&bounded, &FontAssets::new(), &storage).unwrap();
            assert_eq!(artifact.metrics.advance, 0.0);
            assert!(artifact.metrics.ink.right > 0.0);
        } else {
            assert_eq!(bounded.width, legacy.width);
        }
    }
}


#[test]
fn horizontal_braces_match_katex_extents_and_emit_three_filled_pieces() {
    use ratex_types::{DisplayItem, PathCommand};

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for style in [MathStyle::Text, MathStyle::Display] {
        let options = ratex_layout::LayoutOptions::default().with_style(style);
        // Measured in the installed VS Code KaTeX 0.16.21 bundle.
        for (tex, height, depth) in [
            (r"\underbrace{x+y}", 0.58333, 0.84244),
            (r"\underbrace{x+y}_{\text{sum}}", 0.58333, 1.343832),
            (r"\overbrace{x+y}^{\text{sum}}", 1.732722, 0.19444),
            (
                r"\underbrace{\frac{1}{2}}_{\text{fraction}}",
                1.32144,
                2.020108,
            ),
            (
                r"\underbrace{x_1+x_2+\cdots+x_n}_{n\;\text{terms}}",
                0.58333,
                1.428556,
            ),
            (r"a+\underbrace{b+c}_{\text{part}}=d", 0.69444, 1.497994),
            (r"x^{\underbrace{a+b}_{c}}", 1.461069, 0.0),
            (r"\underbrace{}_{x}", 0.0, 1.149392),
            (r"\overbrace{}", 0.648, 0.0),
        ] {
            let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage)
                .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            assert!(
                (display.height - height).abs() < 0.002,
                "{tex}: height {}",
                display.height
            );
            assert!(
                (display.depth - depth).abs() < 0.002,
                "{tex}: depth {}",
                display.depth
            );

            let pieces: Vec<_> = display
                .items
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Path {
                        x,
                        y,
                        commands,
                        fill: true,
                        ..
                    } => Some((x, y, commands)),
                    _ => None,
                })
                .collect();
            assert_eq!(pieces.len(), 3, "{tex}");
            assert!(
                pieces.iter().all(|(x, y, commands)| {
                    x.is_finite()
                        && y.is_finite()
                        && matches!(commands.first(), Some(PathCommand::MoveTo { .. }))
                        && matches!(commands.last(), Some(PathCommand::Close))
                        && commands
                            .iter()
                            .any(|cmd| matches!(cmd, PathCommand::CubicTo { .. }))
                }),
                "{tex}"
            );
            let drawing = outline_drawing(&display, &fonts, &storage).unwrap();
            assert!(drawing.primitives.len() >= 3, "{tex}");
        }
    }
}

#[test]
fn horizontal_brace_annotations_center_without_stretching_the_brace() {
    use ratex_types::{DisplayItem, PathCommand};

    let cancel = AtomicBool::new(false);
    let display = |tex: &str| {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let ast = ratex_parser::parse_with_storage(tex, &storage)
            .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
        let options = ratex_layout::LayoutOptions::default();
        let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
        ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
        ratex_layout::to_display_list_with_control(&layout, &control).unwrap()
    };
    let brace = |display: &ratex_types::DisplayList| {
        let paths: Vec<_> = display
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Path { x, y, commands, .. } => Some((*x, *y, commands)),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 3);
        let right = paths[2]
            .2
            .iter()
            .fold(0.0_f64, |right, command| match *command {
                PathCommand::MoveTo { x, .. }
                | PathCommand::LineTo { x, .. }
                | PathCommand::CubicTo { x, .. } => right.max(x),
                _ => right,
            });
        (paths[0].0, paths[0].1, right)
    };
    let glyph = |display: &ratex_types::DisplayList, ch| {
        display
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::GlyphPath {
                    x,
                    y,
                    scale,
                    char_code,
                    ..
                } if *char_code == u32::from(ch) => Some((*x, *y, *scale)),
                _ => None,
            })
            .unwrap()
    };

    for (tex, is_over) in [
        (r"\overbrace{x}^{\text{long annotation}}_{j}", true),
        (r"\underbrace{x}_{\text{long annotation}}^{j}", false),
    ] {
        let result = display(tex);
        let (brace_x, brace_y, width) = brace(&result);
        assert!((width - 1.6).abs() < 1e-9);
        let (body_x, body_y, body_scale) = glyph(&result, 'x');
        let (label_x, label_y, label_scale) = glyph(&result, 'l');
        let (side_x, side_y, side_scale) = glyph(&result, 'j');
        assert!(
            label_x < brace_x && body_x > brace_x && side_x > brace_x + width,
            "{tex}"
        );
        assert_eq!((body_scale, label_scale, side_scale), (1.0, 0.7, 0.7));
        if is_over {
            assert!(
                label_y < brace_y && brace_y < body_y && body_y < side_y,
                "{tex}"
            );
        } else {
            assert!(
                side_y < body_y && body_y < brace_y && brace_y + 0.548 < label_y,
                "{tex}"
            );
        }
    }
    let plain = display(r"\underbrace{x}");
    let opposite = display(r"\underbrace{x}^{j}");
    assert_eq!(brace(&plain).2, brace(&opposite).2);
    assert!(glyph(&opposite, 'j').0 >= plain.width);
    let wide = display(r"\underbrace{x_1+x_2+\cdots+x_n}");
    assert!(brace(&wide).2 > 6.0);
    let bold = display(r"\mathbf{\overbrace{x+y}^{z}}");
    let (x, _, _) = glyph(&bold, 'x');
    let (plus, _, _) = glyph(&bold, '+');
    let bold_x = display(r"\mathbf{x}");
    assert!((plus - x - bold_x.width - 4.0 / 18.0).abs() < 1e-9);

    for (tex, expected) in [
        (
            r"\mathbf{\underbrace{\frac{x+y}{z}}}",
            [('x', "Main-Bold"), ('y', "Main-Bold"), ('z', "Main-Bold")],
        ),
        (
            r"\underbrace{\mathbf{\frac{x}{\mathnormal{y}}}}_{z}",
            [
                ('x', "Main-Bold"),
                ('y', "Math-Italic"),
                ('z', "Math-Italic"),
            ],
        ),
        (
            r"\mathbf{\frac{\underbrace{x+y}_{z}}{w}}",
            [('x', "Main-Bold"), ('y', "Main-Bold"), ('z', "Main-Bold")],
        ),
        (
            r"\mathbf{\sum_{\underbrace{x+y}_{n}}}",
            [('x', "Main-Bold"), ('y', "Main-Bold"), ('n', "Main-Bold")],
        ),
        (
            r"\mathbf{\overbrace{x}^{\text{sum}}_z}",
            [
                ('x', "Main-Bold"),
                ('s', "Main-Regular"),
                ('z', "Main-Bold"),
            ],
        ),
    ] {
        let result = display(tex);
        for (ch, expected_font) in expected {
            assert!(
                result.items.iter().any(|item| matches!(item,
                    DisplayItem::GlyphPath { font, char_code, .. }
                        if *char_code == u32::from(ch) && font == expected_font
                )),
                "{tex}: {ch} expected in {expected_font}"
            );
        }
    }
    for tex in [
        r"\mathbf{\overbrace{\displaystyle x}}",
        r"\mathbf{\displaystyle\overbrace{x}}",
    ] {
        assert!(
            display(tex).items.iter().any(|item| matches!(item,
                DisplayItem::GlyphPath { font, char_code: 120, .. } if font == "Math-Italic"
            )),
            "{tex}"
        );
    }
    for (tex, codepoint, expected_font) in [
        (r"\mathbf{\overbrace{𝑥}}", 0x1d465, "Math-Italic"),
        (r"\mathit{\overbrace{x}}", u32::from('x'), "Main-Italic"),
        (
            r"\mathnormal{\underbrace{1+\Gamma}}",
            u32::from('1'),
            "Math-Italic",
        ),
    ] {
        assert!(
            display(tex).items.iter().any(|item| matches!(item,
                DisplayItem::GlyphPath { font, char_code, .. }
                    if *char_code == codepoint && font == expected_font
            )),
            "{tex}"
        );
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(
            pipeline(tex, MathMode::Inline, &storage, &FontAssets::new()).is_ok(),
            "{tex}"
        );
    }
    let script = display(r"\mathbf{\overbrace{x}^{a\sin b}}");
    let (a_x, _, scale) = glyph(&script, 'a');
    let (sin_x, _, _) = glyph(&script, 's');
    let bold_a = display(r"\mathbf{a}");
    let script_quad = ratex_font::get_global_metrics(MathStyle::Script.size_index()).quad;
    assert!((sin_x - a_x - scale * (bold_a.width + 3.0 * script_quad / 18.0)).abs() < 1e-9);
    let close = display(r"\cdots,");
    let binary = display(r"\cdots+");
    assert_eq!(glyph(&close, '\u{22EF}').0, 0.0);
    assert_eq!(glyph(&binary, '\u{22EF}').0, 0.0);
    let centered_dots = display(r"\cdots");
    // The macro adds a thin space before the comma, plus the ordinary Inner-to-Punct glue.
    assert!((glyph(&close, ',').0 - centered_dots.width - 6.0 / 18.0).abs() < 1e-9);
    assert!(bold.items.iter().any(|item| matches!(item,
        DisplayItem::GlyphPath { font, char_code: 120, .. } if font == "Main-Bold"
    )));
}

#[test]
fn horizontal_braces_keep_typed_resource_limits_and_reject_brackets() {
    use ratex_types::execution::Phase;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for tex in [r"\overbracket{x}", r"\underbracket{x}"] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(
            matches!(
                pipeline(tex, MathMode::Inline, &storage, &fonts),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Parser
                }))
            ),
            "{tex}"
        );
    }

    let mut limits = MathLimits.execution();
    limits.values[Resource::DisplayItems as usize] = 2;
    let control = ExecutionControl::new(&cancel, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(r"\overbrace{}", MathMode::Inline, &storage, &fonts),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Validate,
            resource: Resource::DisplayItems,
            limit: 2,
            requested: 3,
        }))
    ));

    let mut limits = MathLimits.execution();
    limits.values[Resource::OutlineSegments as usize] = 2;
    let control = ExecutionControl::new(&cancel, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(r"\underbrace{}", MathMode::Inline, &storage, &fonts),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Outline,
            resource: Resource::OutlineSegments,
            limit: 2,
            ..
        }))
    ));
}

#[test]
fn r13_display_parentheses_assemble_real_caps_and_preserve_the_complete_lab_cov() {
    use ratex_font::{FontId, get_char_metrics};
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let options = ratex_layout::LayoutOptions::default().with_style(MathStyle::Display);
    let minimal = r"\left(\sum_{i=1}^{n}x_i\right)";
    // Fixed copy of the authored Lab formula; this test never reads user data.
    let cov = r"\operatorname{Cov}\!\left(\sum_{i=1}^{n} a_i X_i,\;\sum_{j=1}^{m} b_j Y_j\right)
=\sum_{i=1}^{n}\sum_{j=1}^{m} a_i b_j\,\operatorname{Cov}(X_i,Y_j)
+\frac{\prod_{k=1}^{p}(1+\lambda_k^2)}{1+\sqrt{1+\gamma^2}}";
    for tex in [minimal, cov, r"\sum_{i=1}^{n}x_i", r"\left(\sum x\right)"] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
        let layout = ratex_layout::layout_with_storage(&ast, &options, &storage)
            .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
        ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
        let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
        let artifact = outline_drawing(&display, &fonts, &storage).unwrap();
        assert!(!artifact.commands.is_empty(), "{tex}");
        let metrics = artifact.metrics;
        assert!(
            [
                metrics.advance,
                metrics.ascent,
                metrics.descent,
                metrics.ink.left,
                metrics.ink.top,
                metrics.ink.right,
                metrics.ink.bottom,
            ]
            .iter()
            .all(|value| value.is_finite()),
            "{tex}"
        );
        assert!(metrics.ink.left >= -1e-10, "{tex}: {:?}", metrics.ink);
        assert!(
            metrics.ink.right <= metrics.advance + 1e-10,
            "{tex}: {:?}",
            metrics.ink
        );
        if tex == minimal {
            let glyph = |code| {
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
                        } if *char_code == code && font == "Size4-Regular" => {
                            Some((*x, *y, *scale))
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            let stems: Vec<_> = display
                .items
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Rect {
                        x,
                        y,
                        width,
                        height,
                        ..
                    } if (*width - 0.111).abs() < 1e-10 => Some((*x, *y, *height)),
                    _ => None,
                })
                .collect();
            assert_eq!(stems.len(), 2);
            for ((top, bottom), stem) in [(0x239b, 0x239d), (0x239e, 0x23a0)].into_iter().zip(stems)
            {
                let (_, top_y, scale) = glyph(top);
                let (_, bottom_y, bottom_scale) = glyph(bottom);
                let top_metrics = get_char_metrics(FontId::Size4Regular, top).unwrap();
                let bottom_metrics = get_char_metrics(FontId::Size4Regular, bottom).unwrap();
                let upper = top_y - top_metrics.height * scale;
                let lower = bottom_y + bottom_metrics.depth * bottom_scale;
                assert!(((upper + lower) / 2.0 - (display.height - 0.25)).abs() < 1e-10);
                assert!(stem.1 < top_y + top_metrics.depth * scale);
                assert!(stem.1 + stem.2 > bottom_y - bottom_metrics.height * bottom_scale);
                assert!(upper >= -1e-10 && lower <= display.height + display.depth + 1e-10);
            }
        }
    }
}

#[test]
fn r13_nested_text_styles_compose_restore_and_shape_chinese_with_the_same_state() {
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for (tex, expected) in [
        (r"\textbf{\textit{Aa}}", vec!["Main-BoldItalic"; 2]),
        (r"\textit{\textbf{Aa}}", vec!["Main-BoldItalic"; 2]),
        (
            r"\textbf{A\textit{a}B\textnormal{C}\textmd{D}}E",
            vec![
                "Main-Bold",
                "Main-BoldItalic",
                "Main-Bold",
                "Main-Bold",
                "Main-Regular",
                "Math-Italic",
            ],
        ),
        (
            r"\textit{A\textup{B}C}",
            vec!["Main-Italic", "Main-Regular", "Main-Italic"],
        ),
        (r"\textsf{\textbf{A}}", vec!["SansSerif-Bold"]),
        (r"\textsf{\textit{A}}", vec!["SansSerif-Italic"]),
    ] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
        let layout =
            ratex_layout::layout_with_storage(&ast, &Default::default(), &storage).unwrap();
        ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
        let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
        let actual: Vec<_> = display
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::GlyphPath { font, .. } => Some(font.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(actual, expected, "{tex}");
        assert!(
            !outline_drawing(&display, &fonts, &storage)
                .unwrap()
                .commands
                .is_empty()
        );
    }

    struct Shaper(Vec<chinese_text::ChineseTextStyle>);
    impl chinese_text::ChineseTextShaper for Shaper {
        fn shape(
            &mut self,
            _: &[u16],
            style: chinese_text::ChineseTextStyle,
            outline: &mut chinese_text::ChineseTextOutline<'_, '_>,
        ) -> Result<Option<chinese_text::ChineseTextMetrics>, MathError> {
            use chinese_text::ChineseTextCommand as Command;
            self.0.push(style);
            for command in [
                Command::MoveTo { x: 0.0, y: -0.8 },
                Command::LineTo { x: 2.0, y: -0.8 },
                Command::LineTo { x: 2.0, y: 0.1 },
                Command::LineTo { x: 0.0, y: 0.1 },
                Command::Close,
            ] {
                outline.push(command)?;
            }
            Ok(Some(chinese_text::ChineseTextMetrics {
                width: 2.0,
                height: 0.8,
                depth: 0.1,
            }))
        }
    }
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    let mut shaper = Shaper(Vec::new());
    let artifact = pipeline_with_chinese_text(
        r"\textbf{\textit{Aa中文}}+\textit{\textbf{Aa中文}}",
        MathMode::Inline,
        &storage,
        &fonts,
        Some(&mut shaper),
    )
    .unwrap();
    assert_eq!(
        shaper.0,
        [chinese_text::ChineseTextStyle {
            bold: true,
            italic: true,
            ..Default::default()
        }]
    );
    assert!(artifact.primitives.len() >= 7 && !artifact.commands.is_empty());
}

#[test]
fn r13_missing_text_faces_metrics_and_new_paths_keep_typed_limits_and_cancellation() {
    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    for tex in [
        r"\textsf{\textbf{\textit{A}}}",
        r"\texttt{\textbf{A}}",
        r"\texttt{\textit{A}}",
        r"\texttt{æ}",
    ] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(
            matches!(
                pipeline(tex, MathMode::Inline, &storage, &fonts),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Layout
                })) | Err(MathError::Storage(
                    ratex_types::storage::StorageError::Execution(ExecutionError::Unsupported {
                        phase: Phase::Layout
                    })
                ))
            ),
            "{tex}"
        );
    }
    let rows = vec!["x"; 54].join(r"\\");
    let oversized = format!(r"\left(x+\begin{{matrix}}{rows}\end{{matrix}}\right)");
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(&oversized, MathMode::Display, &storage, &fonts),
        Err(MathError::Execution(ExecutionError::Unsupported {
            phase: Phase::Layout
        })) | Err(MathError::Storage(
            ratex_types::storage::StorageError::Execution(ExecutionError::Unsupported {
                phase: Phase::Layout
            })
        ))
    ));

    struct CancelOutline<'a>(&'a AtomicBool);
    impl Observer for CancelOutline<'_> {
        fn checkpoint(&self, phase: Phase, ordinal: u64) {
            if phase == Phase::Outline && ordinal == 10 {
                self.0.store(true, Ordering::Release);
            }
        }
    }
    for tex in [r"\left(\sum_{i=1}^{n}x_i\right)", r"\textbf{\textit{Aa}}"] {
        for resource in [Resource::LayoutNodes, Resource::OutlineSegments] {
            let mut limits = MathLimits.execution();
            limits.values[resource as usize] = 0;
            let control = ExecutionControl::new(&cancel, None, limits);
            let storage = StorageLedger::new(&control);
            let result = pipeline(tex, MathMode::Display, &storage, &fonts);
            let actual = match result {
                Err(MathError::Execution(ExecutionError::ResourceLimit {
                    resource,
                    limit: 0,
                    ..
                }))
                | Err(MathError::Storage(ratex_types::storage::StorageError::Execution(
                    ExecutionError::ResourceLimit {
                        resource, limit: 0, ..
                    },
                ))) => resource,
                other => panic!("{tex}: {:?}", other.err()),
            };
            assert_eq!(actual, resource);
        }
        let observer = CancelOutline(&cancel);
        let control =
            ExecutionControl::new(&cancel, None, MathLimits.execution()).with_observer(&observer);
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline(tex, MathMode::Display, &storage, &fonts),
            Err(MathError::Execution(ExecutionError::Cancelled {
                phase: Phase::Outline
            })) | Err(MathError::Storage(
                ratex_types::storage::StorageError::Execution(ExecutionError::Cancelled {
                    phase: Phase::Outline
                })
            ))
        ));
        assert!(cancel.swap(false, Ordering::AcqRel));
    }
}

#[test]
fn r13_encoded_text_keeps_its_font_and_styled_missing_glyph_stays_typed() {
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let tex = r"\text{𝑥}";
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
    let options = ratex_layout::LayoutOptions::default();
    let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
    ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
    let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
    assert!(matches!(&display.items[..],
        [DisplayItem::GlyphPath { font, char_code: 0x1d465, .. }] if font == "Math-Italic"));
    let original = ratex_parser::parse(tex).unwrap();
    let expected = ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
    assert_eq!(display.items, expected.items);
    assert!(
        !outline_drawing(&display, &fonts, &storage)
            .unwrap()
            .commands
            .is_empty()
    );

    // The bold metrics for x exist, but its TTF does not own this scalar's
    // encoded italic glyph. That remains a visible MissingGlyph, not a reset.
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        pipeline(r"\textbf{𝑥}", MathMode::Inline, &storage, &fonts),
        Err(MathError::MissingGlyph {
            codepoint: 0x1d465,
            ..
        })
    ));
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    assert!(pipeline(r"\textbf{\textit{x}}", MathMode::Inline, &storage, &fonts).is_ok());
}

#[test]
fn r13_default_text_symbols_and_legacy_fonts_preserve_their_original_producers() {
    use ratex_types::DisplayItem;

    let cancel = AtomicBool::new(false);
    let fonts = FontAssets::new();
    let options = ratex_layout::LayoutOptions::default();
    for (tex, expected) in [
        (r"\text{\maltese}", vec![("AMS-Regular", 0x2720)]),
        (r"\text{ℂ}", vec![("AMS-Regular", u32::from('C'))]),
        (r"\overbrace{\text{𝑥}}", vec![("Math-Italic", 0x1d465)]),
        (
            r"\text{\cal ABC}",
            vec![
                ("Caligraphic-Regular", u32::from('A')),
                ("Caligraphic-Regular", u32::from('B')),
                ("Caligraphic-Regular", u32::from('C')),
            ],
        ),
        (
            r"\text{\cal A\textit{B}C}",
            vec![
                ("Caligraphic-Regular", u32::from('A')),
                ("Main-Italic", u32::from('B')),
                ("Caligraphic-Regular", u32::from('C')),
            ],
        ),
        (
            r"\text{\cal \underline{A}}",
            vec![("Main-Regular", u32::from('A'))],
        ),
        (r"\text{\it A}", vec![("Math-Italic", u32::from('A'))]),
    ] {
        let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
        let layout = ratex_layout::layout_with_storage(&ast, &options, &storage)
            .unwrap_or_else(|error| panic!("{tex}: {error:?}"));
        ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
        let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
        let actual: Vec<_> = display
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::GlyphPath {
                    font, char_code, ..
                } => Some((font.as_str(), *char_code)),
                _ => None,
            })
            .collect();
        assert_eq!(actual, expected, "{tex}");
        assert!(
            !outline_drawing(&display, &fonts, &storage)
                .unwrap()
                .commands
                .is_empty(),
            "{tex}"
        );
    }

    let tex = r"\text{\textunderscore}";
    let control = ExecutionControl::new(&cancel, None, MathLimits.execution());
    let storage = StorageLedger::new(&control);
    let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
    let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
    assert_eq!(
        (layout.width, layout.height, layout.depth),
        (0.65, 0.0, 0.31)
    );
    ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
    let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
    let original = ratex_parser::parse(tex).unwrap();
    let expected = ratex_layout::to_display_list(&ratex_layout::layout(&original, &options));
    assert_eq!(display.items, expected.items);
    assert!(
        matches!(&display.items[..], [DisplayItem::Line { width, thickness, .. }]
        if *width == 0.65 && *thickness == 0.08)
    );
    assert!(
        !outline_drawing(&display, &fonts, &storage)
            .unwrap()
            .commands
            .is_empty()
    );
}

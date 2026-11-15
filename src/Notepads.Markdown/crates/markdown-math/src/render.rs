// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Synchronous, funded math pipeline. The host owns admission, source identity,
//! worker retirement and stale-result publication.

use crate::{
    ExecutionControl, Phase, StorageLedger,
    chinese_text::{self, ChineseTextShaper},
    fonts::FontAssets,
    limits::{MathLimits, WorkerStackContract},
    outline::outline_drawing,
    types::*,
};
use ratex_types::{
    math_style::MathStyle,
    storage::{JOB_BYTES, StorageSnapshot},
};
const READONLY_TABLE_BYTES: usize =
    ratex_font::bounded_readonly_bytes() + ratex_parser::bounded_readonly_bytes();
const FIXED_ASSET_BYTES: usize = crate::fonts::FONT_BYTES + READONLY_TABLE_BYTES;
const _: () = {
    assert!(READONLY_TABLE_BYTES <= 2 * 1024 * 1024);
    assert!(FIXED_ASSET_BYTES <= 4 * 1024 * 1024);
};

/// The integration owner supplies real already-acquired source/root and long-lived policy
/// leases. No UI callbacks are permitted. The complete 64-MiB reservation stays held until all
/// transient owners are gone, then split_result transfers only the actual retained payload.
pub trait JobFunding {
    type RetainedLease;
    /// The minimum of the independently held source/root and derived-policy reservations,
    /// never their sum. Each pool must really own at least the complete 64-MiB admission.
    fn reserved_bytes(&self) -> u64;
    fn split_result(
        &mut self,
        actual_retained_bytes: u64,
    ) -> Result<Self::RetainedLease, MathError>;
}

pub struct OwnedMathArtifact<L> {
    // Declaration order is deliberate: actual owned buffers drop before the retained lease.
    data: MathArtifactData,
    _lease: L,
    cumulative_requested_bytes: [u64; ratex_types::storage::FAMILY_COUNT],
}
impl<L> OwnedMathArtifact<L> {
    pub fn data(&self) -> &MathArtifactData {
        &self.data
    }
    pub fn cumulative_requested_bytes(&self) -> &[u64; ratex_types::storage::FAMILY_COUNT] {
        &self.cumulative_requested_bytes
    }
}

pub fn render<F: JobFunding>(
    tex: &str,
    mode: MathMode,
    control: &ExecutionControl<'_>,
    limits: &MathLimits,
    fonts: &FontAssets,
    funding: F,
    stack: WorkerStackContract,
) -> Result<OwnedMathArtifact<F::RetainedLease>, MathError> {
    render_seeded(
        tex,
        mode,
        control,
        limits,
        fonts,
        funding,
        stack,
        None,
        &mut StorageSnapshot::default(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "One pipeline owns the explicit funded stage handoff"
)]
pub(crate) fn render_seeded<F: JobFunding>(
    tex: &str,
    mode: MathMode,
    control: &ExecutionControl<'_>,
    limits: &MathLimits,
    fonts: &FontAssets,
    mut funding: F,
    stack: WorkerStackContract,
    shaper: Option<&mut dyn ChineseTextShaper>,
    cumulative: &mut StorageSnapshot,
) -> Result<OwnedMathArtifact<F::RetainedLease>, MathError> {
    control.checkpoint(Phase::Admission)?;
    limits.validate_control(control.limits())?;
    let latest = std::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(5))
        .ok_or(MathError::InvalidLimits)?;
    if control.deadline().is_none_or(|deadline| deadline > latest) {
        return Err(MathError::InvalidLimits);
    }
    stack.admit()?;
    if funding.reserved_bytes() < JOB_BYTES {
        return Err(MathError::InvalidFunding);
    }
    let storage = StorageLedger::new(control);
    let result = (|| {
        storage.seed(*cumulative)?;
        // Immutable assets are finite and admitted independently of dynamic producer storage.
        // No LazyLock registry/font cache is constructed by this elementary path.
        storage.charge(
            Phase::Admission,
            ratex_types::storage::StorageFamily::Assets,
            FIXED_ASSET_BYTES as u64,
            0,
        )?;
        let data = pipeline_with_chinese_text(tex, mode, &storage, fonts, shaper)?;
        // pipeline returned only final numerical buffers; AST/Layout/DisplayList have dropped.
        let bytes = data
            .commands
            .capacity()
            .checked_mul(size_of::<MathPathCommand>())
            .and_then(|value| {
                data.primitives
                    .capacity()
                    .checked_mul(size_of::<Primitive>())
                    .and_then(|other| value.checked_add(other))
            })
            .and_then(|value| value.checked_add(size_of::<OwnedMathArtifact<F::RetainedLease>>()))
            .ok_or(MathError::InternalInvariant)? as u64;
        if bytes > 32 * 1024 * 1024 {
            return Err(MathError::InvalidFunding);
        }
        let lease = funding.split_result(bytes)?;
        Ok(OwnedMathArtifact {
            data,
            _lease: lease,
            cumulative_requested_bytes: storage.used_bytes(),
        })
    })();
    *cumulative = storage.snapshot();
    result
}

#[cfg(test)]
fn pipeline(
    tex: &str,
    mode: MathMode,
    storage: &StorageLedger<'_>,
    fonts: &FontAssets,
) -> Result<MathArtifactData, MathError> {
    pipeline_with_chinese_text(tex, mode, storage, fonts, None)
}

fn pipeline_with_chinese_text(
    tex: &str,
    mode: MathMode,
    storage: &StorageLedger<'_>,
    fonts: &FontAssets,
    shaper: Option<&mut dyn ChineseTextShaper>,
) -> Result<MathArtifactData, MathError> {
    use ratex_parser::ControlledParseError;
    let nodes = ratex_parser::parse_with_storage(tex, storage).map_err(|error| match error {
        ControlledParseError::Storage(error) => MathError::Storage(error),
        ControlledParseError::Execution(error) => MathError::Execution(error),
        ControlledParseError::Parse(_) => MathError::ParseFailure,
    })?;
    let options = ratex_layout::LayoutOptions {
        style: match mode {
            MathMode::Inline => MathStyle::Text,
            MathMode::Display => MathStyle::Display,
        },
        ..Default::default()
    };
    let chinese_text = match shaper {
        Some(shaper) if tex.chars().any(ratex_parser::parser::is_chinese_text_char) => {
            Some(chinese_text::prepare(&nodes, storage, shaper)?)
        }
        _ => None,
    };
    let layout = if let Some(text) = &chinese_text {
        ratex_layout::engine::layout_with_chinese_text(&nodes, &options, storage, &text.runs)?
    } else {
        ratex_layout::layout_with_storage(&nodes, &options, storage)?
    };
    ratex_layout::validate_layout_with_control(&layout, storage.control()).map_err(|error| {
        match error {
            ratex_layout::ControlledValidationError::Execution(error) => {
                MathError::Execution(error)
            }
            ratex_layout::ControlledValidationError::Storage(error) => MathError::Storage(error),
            ratex_layout::ControlledValidationError::InvalidGeometry => MathError::InvalidGeometry,
            ratex_layout::ControlledValidationError::Invalid(_) => MathError::InvalidGeometry,
        }
    })?;
    let display = match &chinese_text {
        Some(text) => ratex_layout::to_display::to_display_list_with_chinese_text(
            &layout,
            storage.control(),
            &text.runs,
        ),
        None => ratex_layout::to_display_list_with_control(&layout, storage.control()),
    }
    .map_err(|error| match storage.failure() {
        Some(error) => MathError::Storage(error),
        None => MathError::Execution(error),
    })?;
    let artifact = match &chinese_text {
        Some(text) => crate::outline::outline_drawing_with_chinese_text(
            &display,
            fonts,
            storage,
            &text.paths,
        )?,
        None => outline_drawing(&display, fonts, storage)?,
    };
    storage.check(Phase::Outline)?;
    Ok(artifact)
}

#[cfg(test)]
#[path = "fixup_tests.rs"]
mod fixup_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    struct TestChineseText {
        seen: Vec<String>,
        missing: bool,
    }

    impl ChineseTextShaper for TestChineseText {
        fn shape(
            &mut self,
            utf16: &[u16],
            _: chinese_text::ChineseTextStyle,
            outline: &mut chinese_text::ChineseTextOutline<'_, '_>,
        ) -> Result<Option<chinese_text::ChineseTextMetrics>, MathError> {
            use chinese_text::ChineseTextCommand::{Close, LineTo, MoveTo};

            self.seen.push(String::from_utf16(utf16).unwrap());
            if self.missing {
                return Ok(None);
            }
            // Opposite windings make the interior contour a hole.
            for command in [
                MoveTo { x: 0.1, y: -0.8 },
                LineTo { x: 2.4, y: -0.8 },
                LineTo { x: 2.4, y: 0.1 },
                LineTo { x: 0.1, y: 0.1 },
                Close,
                MoveTo { x: 0.2, y: -0.7 },
                LineTo { x: 0.2, y: 0.0 },
                LineTo { x: 2.3, y: 0.0 },
                LineTo { x: 2.3, y: -0.7 },
                Close,
            ] {
                outline.push(command)?;
            }
            Ok(Some(chinese_text::ChineseTextMetrics {
                width: 2.6,
                height: 0.82,
                depth: 0.14,
            }))
        }
    }

    #[test]
    fn chinese_text_uses_measured_advance_and_emits_all_contours() {
        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let mut shaper = TestChineseText {
            seen: Vec::new(),
            missing: false,
        };
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let actual = pipeline_with_chinese_text(
            r"\text{平均值}",
            MathMode::Inline,
            &storage,
            &fonts,
            Some(&mut shaper),
        )
        .unwrap();
        assert_eq!(shaper.seen, ["平均值"]);
        assert!((actual.metrics.advance - 2.6).abs() < 1e-10);
        assert!((actual.metrics.ascent - 0.82).abs() < 1e-10);
        assert!((actual.metrics.descent - 0.14).abs() < 1e-10);
        assert!((actual.metrics.ink.top - 0.02).abs() < 1e-10);
        assert!((actual.metrics.ink.bottom - 0.92).abs() < 1e-10);
        assert_eq!(actual.primitives.len(), 1);
        assert!(actual.primitives[0].fill);
        assert_eq!(actual.primitives[0].command_len, 10);
        assert!(matches!(actual.commands[4], MathPathCommand::Close));
        assert!(matches!(actual.commands[9], MathPathCommand::Close));

        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let actual = pipeline_with_chinese_text(
            r"x_{\text{测试}}+\text{测试}",
            MathMode::Inline,
            &storage,
            &fonts,
            Some(&mut shaper),
        )
        .unwrap();
        assert_eq!(shaper.seen, ["平均值", "测试"]);
        let rendered: Vec<_> = actual
            .primitives
            .iter()
            .filter(|item| item.command_len == 10)
            .collect();
        assert_eq!(rendered.len(), 2);
        let first = rendered[0].command_start as usize;
        let second = rendered[1].command_start as usize;
        let width = |start| match (actual.commands[start], actual.commands[start + 1]) {
            (MathPathCommand::MoveTo { x: left, .. }, MathPathCommand::LineTo { x: right, .. }) => {
                right - left
            }
            _ => panic!("expected measured horizontal edge"),
        };
        assert!((width(first) / width(second) - 0.7).abs() < 1e-10);
    }

    #[test]
    fn bare_han_uses_the_same_measured_upright_run_beside_math() {
        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let render = |tex: &str| {
            let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let mut shaper = TestChineseText {
                seen: Vec::new(),
                missing: false,
            };
            let artifact = pipeline_with_chinese_text(
                tex,
                MathMode::Inline,
                &storage,
                &fonts,
                Some(&mut shaper),
            )
            .unwrap();
            assert_eq!(shaper.seen, ["汉"]);
            artifact
        };
        let bare = render("汉+x");
        let wrapped = render(r"\text{汉}+x");
        assert_eq!(bare.metrics, wrapped.metrics);
        assert_eq!(bare.primitives, wrapped.primitives);
        assert_eq!(bare.commands, wrapped.commands);
        assert!(bare.metrics.advance > 2.6);
        assert!(bare.primitives.len() > 1);
    }

    #[test]
    fn han_joiner_reaches_the_shaper_unchanged_and_uses_measured_geometry() {
        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let tex = "\\text{汉\u{200d}字}";
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        let mut shaper = TestChineseText {
            seen: Vec::new(),
            missing: false,
        };
        let artifact =
            pipeline_with_chinese_text(tex, MathMode::Inline, &storage, &fonts, Some(&mut shaper))
                .unwrap();
        assert_eq!(shaper.seen, ["汉\u{200d}字"]);
        assert_eq!(
            shaper.seen[0].encode_utf16().collect::<Vec<_>>(),
            [0x6c49, 0x200d, 0x5b57]
        );
        assert!((artifact.metrics.advance - 2.6).abs() < 1e-10);
        assert!((artifact.metrics.ascent - 0.82).abs() < 1e-10);
        assert!((artifact.metrics.descent - 0.14).abs() < 1e-10);
        assert_eq!(artifact.primitives.len(), 1);
        assert_eq!(artifact.primitives[0].command_len, 10);
    }

    #[test]
    fn incomplete_han_joiners_and_other_controls_never_reach_the_shaper() {
        use ratex_types::execution::ExecutionError;

        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        for tex in [
            "\\text{\u{200d}汉}",
            "\\text{汉\u{200d}}",
            "\\text{汉\u{200d}\u{200d}字}",
            "\\text{汉\u{200d}a}",
            "\\text{汉\u{200d}，字}",
            "\\text{汉\u{200d}{字}}",
            "\\text{汉\u{200c}字}",
            "\\text{汉\u{2060}字}",
            "\\text{汉\u{202e}字}",
        ] {
            let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let mut shaper = TestChineseText {
                seen: Vec::new(),
                missing: false,
            };
            assert!(matches!(
                pipeline_with_chinese_text(
                    tex,
                    MathMode::Inline,
                    &storage,
                    &fonts,
                    Some(&mut shaper),
                ),
                Err(MathError::Execution(ExecutionError::Unsupported {
                    phase: Phase::Parser,
                }))
            ));
            assert!(shaper.seen.is_empty(), "{tex}");
        }
    }

    #[test]
    fn chinese_text_keeps_missing_font_and_provisional_outline_limits() {
        use ratex_types::execution::{ExecutionError, Resource};

        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let mut shaper = TestChineseText {
            seen: Vec::new(),
            missing: true,
        };
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(
            pipeline_with_chinese_text(
                r"\text{mean}",
                MathMode::Inline,
                &storage,
                &fonts,
                Some(&mut shaper),
            )
            .is_ok()
        );
        assert!(shaper.seen.is_empty());
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline_with_chinese_text(
                r"\text{平均值}",
                MathMode::Inline,
                &storage,
                &fonts,
                Some(&mut shaper),
            ),
            Err(MathError::MissingFont { .. })
        ));

        shaper.missing = false;
        let mut limits = MathLimits.execution();
        limits.values[Resource::OutlineSegments as usize] = 3;
        let control = ExecutionControl::new(&cancelled, None, limits);
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline_with_chinese_text(
                r"\text{平均值}",
                MathMode::Inline,
                &storage,
                &fonts,
                Some(&mut shaper),
            ),
            Err(MathError::Execution(ExecutionError::ResourceLimit {
                phase: Phase::Layout,
                resource: Resource::OutlineSegments,
                limit: 3,
                requested: 4,
            }))
        ));
    }

    #[test]
    fn quoted_vector_norm_renders_real_double_bars_and_preserves_aliases() {
        use ratex_types::{display_item::DisplayItem, math_style::MathStyle};

        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let tex = r"\left\lVert \vec v \right\rVert_2=\sqrt{\sum_{i=1}^d v_i^2}";
        let canonical = r"\left\Vert \vec v \right\Vert_2=\sqrt{\sum_{i=1}^d v_i^2}";
        for mode in [MathMode::Inline, MathMode::Display] {
            let render = |input: &str| {
                let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
                let storage = StorageLedger::new(&control);
                pipeline(input, mode, &storage, &fonts)
                    .unwrap_or_else(|error| panic!("{mode:?}: {input}: {error:?}"))
            };
            for component in [
                r"\vec v",
                r"\left\lVert \vec v \right\rVert",
                r"\sum_{i=1}^d v_i^2",
                r"\sqrt{\sum_{i=1}^d v_i^2}",
            ] {
                assert!(!render(component).commands.is_empty());
            }
            let actual = render(tex);
            let reference = render(canonical);
            assert!(!actual.commands.is_empty());
            assert_eq!(actual.metrics, reference.metrics);
            assert_eq!(actual.primitives, reference.primitives);
            assert_eq!(actual.commands, reference.commands);

            let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let ast = ratex_parser::parse_with_storage(tex, &storage).unwrap();
            let options = ratex_layout::LayoutOptions::default().with_style(match mode {
                MathMode::Inline => MathStyle::Text,
                MathMode::Display => MathStyle::Display,
            });
            let layout = ratex_layout::layout_with_storage(&ast, &options, &storage).unwrap();
            ratex_layout::validate_layout_with_control(&layout, &control).unwrap();
            let display = ratex_layout::to_display_list_with_control(&layout, &control).unwrap();
            let mut stems: Vec<_> = display
                .items
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Rect {
                        x,
                        y,
                        width,
                        height,
                        ..
                    } if (*width - 0.043).abs() < 1e-10 && *height > 1.0 => Some((*x, *y, *height)),
                    _ => None,
                })
                .collect();
            stems.sort_by(|left, right| left.0.total_cmp(&right.0));
            let natural_double_bars = display
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        DisplayItem::GlyphPath {
                            char_code: 0x2225,
                            ..
                        }
                    )
                })
                .count();
            if natural_double_bars == 2 {
                assert!(stems.is_empty());
            } else {
                // Locked doublevert geometry has two .043em stems per side
                // with a .179em gap. Single absolute-value bars fail this check.
                assert_eq!(natural_double_bars, 0);
                assert_eq!(stems.len(), 4, "{mode:?}");
                for pair in stems.chunks_exact(2) {
                    assert!((pair[1].0 - pair[0].0 - 0.222).abs() < 1e-10);
                    assert_eq!(pair[0].1, pair[1].1);
                    assert_eq!(pair[0].2, pair[1].2);
                }
            }
            if mode == MathMode::Display {
                let hook = display
                    .items
                    .iter()
                    .find_map(|item| match item {
                        DisplayItem::GlyphPath {
                            x,
                            y,
                            char_code: 0x23b7,
                            ..
                        } => Some((*x, *y)),
                        _ => None,
                    })
                    .expect("display sum needs the original tall radical hook");
                let stem = display
                    .items
                    .iter()
                    .find_map(|item| match item {
                        DisplayItem::Rect {
                            x,
                            y,
                            width,
                            height,
                            ..
                        } if (*width - 0.04).abs() < 1e-10 && *height > 1.0 => {
                            Some((*x, *y, *height))
                        }
                        _ => None,
                    })
                    .expect("tall radical needs its finite upright");
                assert!((stem.0 - hook.0 - 0.702).abs() < 1e-10);
                assert!((stem.1 + stem.2 - (hook.1 - 0.915) - 0.008).abs() < 1e-10);
                assert!(display.items.iter().any(|item| matches!(
                    item,
                    DisplayItem::Line { x, y, thickness, .. }
                        if (*x - stem.0).abs() < 1e-10
                            && (*y - stem.1 - 0.02).abs() < 1e-10
                            && (*thickness - 0.04).abs() < 1e-10
                )));
            }
        }
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline(
                r"\thisCommandDoesNotExist{1}",
                MathMode::Inline,
                &storage,
                &fonts
            ),
            Err(MathError::ParseFailure)
        ));
    }

    #[test]
    fn tall_radicals_keep_index_nested_height_quota_and_cancellation_boundaries() {
        use ratex_types::{
            execution::{ExecutionError, Observer, Resource},
            storage::StorageError,
        };
        use std::sync::atomic::Ordering;

        let cancelled = AtomicBool::new(false);
        let fonts = FontAssets::new();
        let simple = r"\sqrt{\sum_{i=1}^d v_i^2}";
        let render = |input: &str| {
            let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            pipeline(input, MathMode::Display, &storage, &fonts)
        };
        let root = render(simple).unwrap();
        let indexed = render(r"\sqrt[3]{\sum_{i=1}^d v_i^2}").unwrap();
        assert!(indexed.metrics.advance > root.metrics.advance);
        let nested = render(r"\sqrt{\sqrt{\sum_{i=1}^d v_i^2}}").unwrap();
        assert!(nested.metrics.advance > root.metrics.advance);
        assert!(nested.metrics.ascent > root.metrics.ascent);
        for (rows, accepted) in [(53, true), (54, false)] {
            let cells = vec!["x"; rows].join(r"\\");
            let tex = format!(r"\sqrt{{\begin{{matrix}}{cells}\end{{matrix}}}}");
            let result = render(&tex);
            if accepted {
                assert!(result.is_ok(), "{rows} rows: {:?}", result.err());
            } else {
                assert!(matches!(
                    result,
                    Err(MathError::Storage(StorageError::Execution(
                        ExecutionError::Unsupported {
                            phase: Phase::Layout
                        }
                    ))) | Err(MathError::Execution(ExecutionError::Unsupported {
                        phase: Phase::Layout
                    }))
                ));
            }
        }
        let mut limits = MathLimits.execution();
        limits.values[Resource::DisplayItems as usize] = 2;
        let control = ExecutionControl::new(&cancelled, None, limits);
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline(simple, MathMode::Display, &storage, &fonts),
            Err(MathError::Execution(ExecutionError::ResourceLimit {
                phase: Phase::Validate,
                resource: Resource::DisplayItems,
                limit: 2,
                requested: 3,
            }))
        ));
        struct CancelDisplay<'a>(&'a AtomicBool);
        impl Observer for CancelDisplay<'_> {
            fn checkpoint(&self, phase: Phase, ordinal: u64) {
                if phase == Phase::Display && ordinal == 10 {
                    self.0.store(true, Ordering::Release);
                }
            }
        }
        let observer = CancelDisplay(&cancelled);
        let control = ExecutionControl::new(&cancelled, None, MathLimits.execution())
            .with_observer(&observer);
        let storage = StorageLedger::new(&control);
        assert!(matches!(
            pipeline(simple, MathMode::Display, &storage, &fonts),
            Err(MathError::Execution(ExecutionError::Cancelled {
                phase: Phase::Display
            })) | Err(MathError::Storage(StorageError::Execution(
                ExecutionError::Cancelled {
                    phase: Phase::Display
                }
            )))
        ));
        assert!(cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn actual_parse_layout_outline_keeps_the_original_single_x_strut() {
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(
            &cancelled,
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5)),
            MathLimits.execution(),
        );
        let storage = StorageLedger::new(&control);
        let fonts = FontAssets::new();
        let artifact = pipeline("x", MathMode::Display, &storage, &fonts).unwrap();
        // Original KaTeX 0.16.38 expected, upstream/layout_vs_katex.rs:73; not copied from Actual.
        assert!((artifact.metrics.ascent - 0.43056).abs() < 0.002);
        assert!((artifact.metrics.descent - 0.0).abs() < 0.002);
        assert!(!artifact.commands.is_empty());
    }
    #[test]
    fn unknown_worker_stack_cannot_start_the_candidate_pipeline() {
        assert!(matches!(
            WorkerStackContract::unknown().admit(),
            Err(MathError::StackUnavailable { .. })
        ));
    }
    #[test]
    fn common_formula_outlines_preserve_pinned_original_dimensions() {
        // Unchanged KaTeX0.16.38 dimensions from frozen original cases 019/023/024/055/056.
        // These exercise generated operator RaiseBox and text/font paths, not only layout.
        for (tex, height, depth) in [
            (r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}", 1.59044, 0.686),
            (r"\sum x", 1.05, 0.55),
            (r"\int x", 1.36, 0.8622),
            (r"\text{hello}", 0.6944, 0.0),
            (r"\mathrm{sin}", 0.6679, 0.0),
        ] {
            let cancelled = AtomicBool::new(false);
            let control = ExecutionControl::new(&cancelled, None, MathLimits.execution());
            let storage = StorageLedger::new(&control);
            let actual = pipeline(tex, MathMode::Display, &storage, &FontAssets::new()).unwrap();
            assert!((actual.metrics.ascent - height).abs() < 0.002, "{tex}");
            assert!((actual.metrics.descent - depth).abs() < 0.002, "{tex}");
            assert!(!actual.commands.is_empty(), "{tex}");
        }
    }
    #[test]
    fn final_actual_payload_keeps_its_real_lease_until_the_last_artifact_drops() {
        use std::{cell::Cell, rc::Rc};
        let held = Rc::new(Cell::new(JOB_BYTES));
        struct Lease {
            held: Rc<Cell<u64>>,
            bytes: u64,
        }
        impl Drop for Lease {
            fn drop(&mut self) {
                self.held.set(self.held.get() - self.bytes);
            }
        }
        struct Fund(Lease);
        impl JobFunding for Fund {
            type RetainedLease = Lease;
            fn reserved_bytes(&self) -> u64 {
                self.0.bytes
            }
            fn split_result(&mut self, bytes: u64) -> Result<Lease, MathError> {
                if bytes > self.0.bytes {
                    return Err(MathError::InvalidFunding);
                }
                self.0.bytes -= bytes;
                Ok(Lease {
                    held: Rc::clone(&self.0.held),
                    bytes,
                })
            }
        }
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(
            &cancelled,
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5)),
            MathLimits.execution(),
        );
        // A stub exercises ownership/admission mechanics only. It is expressly not a
        // measured/audited Windows or UWP stack profile and grants no production Ready.
        let stub = WorkerStackContract {
            usable_headroom_bytes: 2 * 1024 * 1024,
            platform_margin_bytes: 64 * 1024,
            audited_required_bytes: Some(128 * 1024),
        };
        let artifact = render(
            "x",
            MathMode::Display,
            &control,
            &MathLimits,
            &FontAssets::new(),
            Fund(Lease {
                held: Rc::clone(&held),
                bytes: JOB_BYTES,
            }),
            stub,
        )
        .unwrap();
        let actual = artifact.data.commands.capacity() * size_of::<MathPathCommand>()
            + artifact.data.primitives.capacity() * size_of::<Primitive>()
            + size_of_val(&artifact);
        assert_eq!(held.get(), actual as u64);
        assert!(!artifact.data.commands.is_empty());
        assert!(held.get() < JOB_BYTES);
        drop(artifact);
        assert_eq!(held.get(), 0);
    }
}

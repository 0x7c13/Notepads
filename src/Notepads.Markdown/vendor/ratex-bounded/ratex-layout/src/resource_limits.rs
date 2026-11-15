// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Validation of output work in completed layouts, without creating drawing items.
use crate::layout_box::{BoxContent, LayoutBox, VBoxChildKind};
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase};
use ratex_types::path_command::PathCommand;

/// A brace piece is a single closed, filled contour. Its longest source path
/// has 19 edges, each generating at most two commands during clipping.
pub(crate) fn is_bounded_fill_path(commands: &[PathCommand], fill: bool) -> bool {
    let [PathCommand::MoveTo { x, y }, edges @ .., PathCommand::Close] = commands else {
        return false;
    };
    fill && (2..=38).contains(&edges.len())
        && x.is_finite()
        && y.is_finite()
        && edges.iter().all(|command| match *command {
            PathCommand::LineTo { x, y } => x.is_finite() && y.is_finite(),
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => [x1, y1, x2, y2, x, y].into_iter().all(f64::is_finite),
            _ => false,
        })
}

/// Maximum estimated drawing items accepted by [`validate_layout`].
pub const MAX_LAYOUT_OUTPUT_ITEMS: usize = 65_536;

/// The count is independent of translation and uniform scaling. DisplayList
/// conversion uses this same count, avoiding an unbounded floating-point loop.
/// Layout validation checks dimensions before using it; conversion itself
/// does not enforce an output budget.
pub(crate) fn vertical_dash_count(height: f64, thickness: f64) -> usize {
    (height / (8.0 * thickness)).ceil().max(0.0) as usize
}

fn own_cost(box_: &LayoutBox) -> Result<usize, &'static str> {
    Ok(match &box_.content {
        BoxContent::Glyph { .. } | BoxContent::Rule { .. } | BoxContent::SvgPath { .. } => 1,
        BoxContent::GlyphRun { glyphs } => glyphs.len(),
        BoxContent::Fraction { bar_thickness, .. } => usize::from(*bar_thickness > 0.0),
        BoxContent::Radical { .. } => 2, // Legacy surd glyph and overbar.
        BoxContent::Angl { .. } | BoxContent::Overline { .. } | BoxContent::Underline { .. } => 1,
        BoxContent::Framed {
            has_border,
            bg_color,
            ..
        } => 4 * usize::from(*has_border) + usize::from(bg_color.is_some()),
        BoxContent::ProofTree { rules, .. } => rules.len(),
        BoxContent::Array {
            col_separators,
            hlines_before_row,
            rule_thickness,
            ..
        } => {
            let mut cost = hlines_before_row
                .iter()
                .fold(0usize, |sum, row| sum.saturating_add(row.len()));
            let dashed = col_separators
                .iter()
                .filter(|sep| **sep == Some(true))
                .count();
            let solid = col_separators
                .iter()
                .filter(|sep| **sep == Some(false))
                .count();
            cost = cost.saturating_add(solid);
            if dashed > 0 {
                let height = box_.height + box_.depth;
                let period = 8.0 * rule_thickness;
                if !height.is_finite() || height < 0.0 || !period.is_finite() || period <= 0.0 {
                    return Err("Invalid dashed separator geometry".into());
                }
                cost = cost.saturating_add(
                    vertical_dash_count(height, *rule_thickness).saturating_mul(dashed),
                );
            }
            cost
        }
        BoxContent::HBox(_)
        | BoxContent::VBox(_)
        | BoxContent::SupSub { .. }
        | BoxContent::OpLimits { .. }
        | BoxContent::Accent { .. }
        | BoxContent::LeftRight { .. }
        | BoxContent::RaiseBox { .. }
        | BoxContent::Scaled { .. }
        | BoxContent::Kern
        | BoxContent::Empty => 0,
    })
}

// Exhaustive traversal: new box variants must explicitly define their children
// and own cost. The input tree stays alive and immutable throughout the walk.
fn children<'a>(box_: &'a LayoutBox, mut visit: impl FnMut(&'a LayoutBox)) {
    match &box_.content {
        BoxContent::HBox(boxes) => boxes.iter().for_each(visit),
        BoxContent::VBox(boxes) => {
            for child in boxes {
                if let VBoxChildKind::Box(body) = &child.kind {
                    visit(body);
                }
            }
        }
        BoxContent::Fraction { numer, denom, .. } => {
            visit(numer);
            visit(denom);
        }
        BoxContent::SupSub { base, sup, sub, .. } | BoxContent::OpLimits { base, sup, sub, .. } => {
            visit(base);
            if let Some(sup) = sup {
                visit(sup);
            }
            if let Some(sub) = sub {
                visit(sub);
            }
        }
        BoxContent::Radical { body, index, .. } => {
            visit(body);
            if let Some(index) = index {
                visit(index);
            }
        }
        BoxContent::Accent { base, accent, .. } => {
            visit(base);
            visit(accent);
        }
        BoxContent::LeftRight { left, right, inner } => {
            visit(left);
            visit(right);
            visit(inner);
        }
        BoxContent::Array {
            cells,
            row_tags,
            tag_col_width,
            ..
        } => {
            for row in cells {
                for cell in row {
                    visit(cell);
                }
            }
            if *tag_col_width > 0.0 {
                for tag in row_tags.iter().flatten() {
                    visit(tag);
                }
            }
        }
        BoxContent::Framed { body, .. }
        | BoxContent::RaiseBox { body, .. }
        | BoxContent::Scaled { body, .. }
        | BoxContent::Angl { body, .. }
        | BoxContent::Overline { body, .. }
        | BoxContent::Underline { body, .. } => visit(body),
        BoxContent::ProofTree { children, .. } => {
            for child in children {
                visit(&child.box_);
            }
        }
        BoxContent::Glyph { .. }
        | BoxContent::GlyphRun { .. }
        | BoxContent::Rule { .. }
        | BoxContent::SvgPath { .. }
        | BoxContent::Kern
        | BoxContent::Empty => {}
    }
}

/// Validate the output-work budget of a completed layout without creating drawing items.
///
/// Call after [`crate::layout`] and before [`crate::to_display_list`] when handling
/// untrusted input. This does not modify the layout. The check covers drawing-item
/// costs and dashed separator dimensions, not arbitrary hand-built tree validity.
/// Layouts derived from parser-produced ASTs are the supported input contract.
pub fn validate_layout(root: &LayoutBox) -> Result<(), String> {
    output_cost(root).map(|_| ())
}

#[derive(Debug)]
pub enum ControlledValidationError {
    Execution(ExecutionError),
    Storage(ratex_types::storage::StorageError),
    InvalidGeometry,
    Invalid(String),
}
impl std::fmt::Display for ControlledValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(e) => e.fmt(f),
            Self::Storage(e) => e.fmt(f),
            Self::InvalidGeometry => f.write_str("Invalid layout geometry"),
            Self::Invalid(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for ControlledValidationError {}
pub fn validate_layout_with_control(
    root: &LayoutBox,
    control: &ExecutionControl<'_>,
) -> Result<(), ControlledValidationError> {
    if let Some(storage) = control.storage() {
        return bounded_output_cost(root, &storage);
    }
    let result = output_cost_impl(root, Some(control));
    if let Some(error) = control.failure() {
        return Err(ControlledValidationError::Execution(error));
    }
    result
        .map(|_| ())
        .map_err(ControlledValidationError::Invalid)
}

fn bounded_output_cost(
    root: &LayoutBox,
    storage: &ratex_types::storage::StorageLedger<'_>,
) -> Result<(), ControlledValidationError> {
    use ratex_types::{
        execution::Resource,
        storage::{StorageFamily, WorkVec},
    };
    let mut pending = WorkVec::new(
        Some(storage.control()),
        Phase::Validate,
        StorageFamily::Auxiliary,
    );
    storage
        .check(Phase::Validate)
        .map_err(ControlledValidationError::Storage)?;
    pending.push(root);
    let mut cost = 0u64;
    while let Some(box_) = pending.pop() {
        storage
            .check(Phase::Validate)
            .map_err(ControlledValidationError::Storage)?;
        if !box_.width.is_finite() || !box_.height.is_finite() || !box_.depth.is_finite() {
            return Err(ControlledValidationError::InvalidGeometry);
        }
        if matches!(
            &box_.content,
            BoxContent::Array { .. }
                | BoxContent::Framed { .. }
                | BoxContent::Angl { .. }
                | BoxContent::ProofTree { .. }
                | BoxContent::Accent { .. }
        ) || matches!(
            &box_.content,
            BoxContent::SvgPath { commands, fill } if !is_bounded_fill_path(commands, *fill)
        ) {
            return Err(ControlledValidationError::Execution(
                storage.control().fail(ExecutionError::Unsupported {
                    phase: Phase::Validate,
                }),
            ));
        }
        let count = if matches!(
            &box_.content,
            BoxContent::Radical { inner_height, .. } if *inner_height > 3.0
        ) {
            // Only bounded conversion adds the finite tall-root stem.
            3
        } else {
            own_cost(box_).map_err(|_| ControlledValidationError::InvalidGeometry)? as u64
        };
        cost = cost
            .checked_add(count)
            .ok_or(ControlledValidationError::InvalidGeometry)?;
        let limit = storage.control().limits().values[Resource::DisplayItems as usize];
        if cost > limit {
            return Err(ControlledValidationError::Execution(
                storage.control().fail(ExecutionError::ResourceLimit {
                    phase: Phase::Validate,
                    resource: Resource::DisplayItems,
                    limit,
                    requested: cost,
                }),
            ));
        }
        children(box_, |child| pending.push(child));
    }
    storage
        .check(Phase::Validate)
        .map_err(ControlledValidationError::Storage)
}
fn output_cost(root: &LayoutBox) -> Result<usize, String> {
    output_cost_impl(root, None)
}
fn output_cost_impl(
    root: &LayoutBox,
    execution: Option<&ExecutionControl<'_>>,
) -> Result<usize, String> {
    if let Some(control) = execution {
        control
            .checkpoint(Phase::Validate)
            .map_err(|e| e.to_string())?;
    }
    let mut pending = vec![root];
    let mut cost = 0usize;
    while let Some(box_) = pending.pop() {
        if let Some(control) = execution {
            control
                .checkpoint(Phase::Validate)
                .map_err(|e| e.to_string())?;
        }
        cost = cost.saturating_add(own_cost(box_)?);
        if cost > MAX_LAYOUT_OUTPUT_ITEMS {
            return Err("Layout resource limit exceeded".into());
        }
        children(box_, |child| pending.push(child));
    }
    Ok(cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayoutOptions, layout, to_display_list};
    use ratex_types::math_style::MathStyle;

    #[test]
    fn bounded_path_requires_a_short_finite_closed_contour() {
        let start = PathCommand::MoveTo { x: 0.0, y: 0.0 };
        let edge = PathCommand::LineTo { x: 1.0, y: 0.5 };
        let contour = [start, edge, edge, PathCommand::Close];
        assert!(is_bounded_fill_path(&contour, true));
        assert!(!is_bounded_fill_path(&contour, false));
        assert!(!is_bounded_fill_path(&contour[..3], true));
        assert!(!is_bounded_fill_path(
            &[start, edge, start, PathCommand::Close],
            true
        ));
        assert!(!is_bounded_fill_path(
            &[
                start,
                edge,
                PathCommand::LineTo {
                    x: f64::NAN,
                    y: 0.5
                },
                PathCommand::Close,
            ],
            true
        ));
        let mut too_many_edges = vec![start];
        too_many_edges.extend([edge; 39]);
        too_many_edges.push(PathCommand::Close);
        assert!(!is_bounded_fill_path(&too_many_edges, true));
    }

    #[test]
    fn layout_cost_matches_output_across_primitives_and_scales() {
        let formulas = [
            r"x^2_i+\frac{a}{b}+\binom{n}{k}",
            r"\sqrt[3]{x}+\sum_{i=0}^n i+\hat{x}+\overline{x}+\underline{y}",
            r"\sqrt{\sum_{i=1}^d v_i^2}",
            r"\text{abc}+\colorbox{red}{x}+\fbox{x}+\angl{x}",
            r"\left(\frac{x}{y}\middle|z\right)",
            r"\pmb{\pmb{\begin{array}{:c|r:}\hline x&y\\z\end{array}}}",
            r"\begin{align}x&=y\tag{one}\\z&=0\tag{two}\end{align}",
            r"\begin{prooftree}\AxiomC{P}\UnaryInfC{Q}\end{prooftree}",
            r"\tiny\begin{array}{:c:}\rule{1em}{10em}\end{array}",
            r"\htmlStyle{font-size:2em;}{\begin{array}{:c:}x\end{array}}",
            r"\raisebox{10000000000000000em}{\begin{array}{:c:}x\end{array}}",
            r"\phantom{x}+\vphantom{y}+\smash{z}+\mathllap{w}",
        ];
        for style in [
            MathStyle::Display,
            MathStyle::Text,
            MathStyle::Script,
            MathStyle::ScriptScript,
        ] {
            for input in formulas {
                let ast =
                    ratex_parser::parse(input).unwrap_or_else(|err| panic!("{input}: {err:?}"));
                let boxes = layout(&ast, &LayoutOptions::default().with_style(style));
                assert_eq!(
                    output_cost(&boxes).unwrap(),
                    to_display_list(&boxes).items.len(),
                    "{input} ({style:?})"
                );
            }
        }
    }

    #[test]
    fn invalid_dashed_dimensions_are_rejected_without_conversion() {
        let ast = ratex_parser::parse(r"\begin{array}{:c}x\end{array}").unwrap();
        let BoxContent::HBox(mut boxes) = layout(&ast, &LayoutOptions::default()).content else {
            panic!("expected expression");
        };
        let mut array = boxes.remove(0);
        assert!(matches!(array.content, BoxContent::Array { .. }));
        for height in [f64::NAN, f64::INFINITY, -1.0] {
            array.height = height;
            array.depth = 0.0;
            assert_eq!(
                output_cost(&array).unwrap_err(),
                "Invalid dashed separator geometry"
            );
        }
        array.height = 1.0;
        for thickness in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            if let BoxContent::Array { rule_thickness, .. } = &mut array.content {
                *rule_thickness = thickness;
            }
            assert_eq!(
                output_cost(&array).unwrap_err(),
                "Invalid dashed separator geometry"
            );
        }
    }

    #[test]
    fn dash_count_handles_full_period_and_partial_tail() {
        assert_eq!(vertical_dash_count(0.0, 0.0625), 0);
        assert_eq!(vertical_dash_count(2.0, 0.0625), 4);
        assert_eq!(vertical_dash_count(2.01, 0.0625), 5);
    }
}

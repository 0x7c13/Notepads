// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase, Resource};
use ratex_types::storage::{StorageFamily, WorkVec};

use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::path_command::PathCommand;

use crate::layout_box::{BoxContent, LayoutBox, VBoxChildKind};
use crate::resource_limits::vertical_dash_count;
use crate::surd::surd_font_for_inner_height;

use crate::layout_options::{ChineseTextRun, chinese_text_run_index};

/// Unicode √ (U+221A), same glyph KaTeX uses for `\sqrt` surd.
const SURD_CHAR: u32 = 0x221A;

/// Convert a LayoutBox tree into a flat DisplayList with absolute coordinates.
///
/// The coordinate system:
/// - x increases to the right
/// - y increases downward (screen coordinates)
/// - The origin (0, 0) is at the top-left of the bounding box
/// - The baseline is at y = height
pub fn to_display_list(root: &LayoutBox) -> DisplayList {
    to_display_impl(root, None, &[]).expect("legacy uncontrolled display conversion")
}

pub fn to_display_list_with_control(
    root: &LayoutBox,
    control: &ExecutionControl<'_>,
) -> Result<DisplayList, ExecutionError> {
    to_display_impl(root, Some(control), &[])
}

#[doc(hidden)]
pub fn to_display_list_with_chinese_text(
    root: &LayoutBox,
    control: &ExecutionControl<'_>,
    runs: &[ChineseTextRun<'_>],
) -> Result<DisplayList, ExecutionError> {
    to_display_impl(root, Some(control), runs)
}

fn check(control: Option<&ExecutionControl<'_>>) -> Result<(), ExecutionError> {
    match control {
        Some(control) => control.checkpoint(Phase::Display),
        None => Ok(()),
    }
}
fn charge_display_item(control: Option<&ExecutionControl<'_>>) -> Result<(), ExecutionError> {
    match control {
        Some(control) => control.consume(Phase::Display, Resource::DisplayItems, 1),
        None => Ok(()),
    }
}
fn storage_error(
    error: ratex_types::storage::StorageError,
    control: &ExecutionControl<'_>,
) -> ExecutionError {
    match error {
        ratex_types::storage::StorageError::Execution(error) => error,
        _ => {
            let _ = control.storage().map(|storage| storage.fail(error));
            ExecutionError::StorageFailure {
                phase: Phase::Display,
            }
        }
    }
}
fn font_text(
    font: ratex_font::FontId,
    control: Option<&ExecutionControl<'_>>,
) -> Result<String, ExecutionError> {
    if let Some((control, storage)) =
        control.and_then(|control| control.storage().map(|storage| (control, storage)))
    {
        storage
            .try_string(Phase::Display, font.as_str())
            .map_err(|error| storage_error(error, control))
    } else {
        Ok(font.as_str().to_owned())
    }
}
fn push_item(
    items: &mut Vec<DisplayItem>,
    item: DisplayItem,
    control: Option<&ExecutionControl<'_>>,
) -> Result<(), ExecutionError> {
    if let Some((control, storage)) =
        control.and_then(|control| control.storage().map(|storage| (control, storage)))
    {
        storage
            .try_push(Phase::Display, StorageFamily::Display, items, item)
            .map_err(|error| storage_error(error, control))
    } else {
        items.push(item);
        Ok(())
    }
}
fn to_display_impl(
    root: &LayoutBox,
    control: Option<&ExecutionControl<'_>>,
    runs: &[ChineseTextRun<'_>],
) -> Result<DisplayList, ExecutionError> {
    check(control)?;
    let mut items = Vec::new();
    let baseline_y = root.height;
    emit_box(root, 0.0, baseline_y, 1.0, &mut items, control)?;

    if items.is_empty() {
        return Ok(DisplayList {
            items,
            width: root.width,
            height: root.height,
            depth: root.depth,
        });
    }

    // Compute visual bounding box from actual display items.
    // This handles cases like \smash (zero height/depth) and \mathllap (zero width)
    // where content extends beyond the nominal box dimensions.
    // Horizontal: expand only genuinely zero-width roots. Nested zero-width
    // constructs such as `\mathclap` intentionally keep their overflow outside
    // the parent advance; the fixed output viewport clips that overflow exactly
    // like KaTeX's measured element box.
    let (min_x, max_x, min_y, max_y) = compute_visual_bounds(&items, control, runs)?;

    let mut width = root.width;
    let mut height = root.height;
    let mut depth = root.depth;
    let total_h = root.height + root.depth;

    // Expand vertical dimensions only when nominal total height is near-zero (e.g. \smash)
    if total_h < 0.01 {
        if min_y < -0.001 {
            let extra = -min_y;
            height += extra;
            for item in &mut items {
                check(control)?;
                shift_item_y(item, extra);
            }
        }
        let nominal_bottom = height + depth;
        let shifted_max_y = if min_y < -0.001 { max_y - min_y } else { max_y };
        if shifted_max_y > nominal_bottom + 0.001 {
            depth = shifted_max_y - height;
        }
    }

    // Legacy bitmap output expands a zero-width root to fit its ink. The bounded
    // native renderer retains the logical advance and measures actual ink separately.
    if root.width < 0.01
        && (root.width < 0.0 || !control.is_some_and(|control| control.storage().is_some()))
    {
        if min_x < -0.001 {
            let extra = -min_x;
            width += extra;
            for item in &mut items {
                check(control)?;
                shift_item_x(item, extra);
            }
        }
        let shifted_max_x = if min_x < -0.001 { max_x - min_x } else { max_x };
        if shifted_max_x > width + 0.001 {
            width = shifted_max_x;
        }
    }

    // Filled SVG paths (e.g. KaTeX `tallDelim` for `\vert`) can extend slightly above y=0 from
    // curve overshoot; the pixmap uses nominal height/depth only and would clip. Shift down and
    // grow depth when needed so all path ink fits. (Skip when `total_h < 0.01`: that case already
    // adjusted vertical bounds using the same `min_y`.)
    if total_h >= 0.01 && min_y < -0.001 {
        let extra = -min_y;
        height += extra;
        for item in &mut items {
            check(control)?;
            shift_item_y(item, extra);
        }
        let new_bottom = height + depth;
        let adjusted_max_y = max_y + extra;
        if adjusted_max_y > new_bottom + 0.001 {
            depth = adjusted_max_y - height;
        }
    }

    // Expand depth when content extends below the nominal bottom but nothing went above the top.
    // This handles e.g. \smash[b] which zeroes the layout depth while content still renders below
    // the baseline — the pixmap would otherwise be too short and clip the denominator.
    if total_h >= 0.01 && min_y >= -0.001 && max_y > height + depth + 0.001 {
        depth = max_y - height;
    }

    Ok(DisplayList {
        items,
        width,
        height,
        depth,
    })
}

/// Iteratively emit DisplayItems for a LayoutBox at the given position.
///
/// `x`, `y` are the position of the box's baseline-left corner in absolute coordinates.
/// `scale` is the cumulative size multiplier (1.0 at root, 0.7 in script, 0.5 in scriptscript).
fn emit_box(
    root: &LayoutBox,
    root_x: f64,
    root_y: f64,
    root_scale: f64,
    items: &mut Vec<DisplayItem>,
    control: Option<&ExecutionControl<'_>>,
) -> Result<(), ExecutionError> {
    enum EmitAction<'a> {
        Box {
            lbox: &'a LayoutBox,
            x: f64,
            y: f64,
            scale: f64,
        },
        Item(DisplayItem),
    }

    let mut pending = WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
    pending.push(EmitAction::Box {
        lbox: root,
        x: root_x,
        y: root_y,
        scale: root_scale,
    });

    while let Some(action) = pending.pop() {
        check(control)?;
        let EmitAction::Box { lbox, x, y, scale } = action else {
            if let EmitAction::Item(item) = action {
                charge_display_item(control)?;
                push_item(items, item, control)?;
            }
            continue;
        };

        // Controlled elementary conversion never enters unclosed SVG/array/proof paths.
        // Rejection precedes path collect, row vectors or other branch-local allocations.
        if let Some(control) = control.filter(|control| control.storage().is_some()) {
            if matches!(
                &lbox.content,
                BoxContent::Array { .. }
                    | BoxContent::Framed { .. }
                    | BoxContent::Angl { .. }
                    | BoxContent::ProofTree { .. }
                    | BoxContent::Accent { .. }
            ) {
                return Err(control.fail(ExecutionError::Unsupported {
                    phase: Phase::Display,
                }));
            }
        }
        match &lbox.content {
            BoxContent::HBox(children) => {
                let mut cur_x = x;
                let mut child_actions =
                    WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
                for child in children {
                    check(control)?;
                    child_actions.push(EmitAction::Box {
                        lbox: child,
                        x: cur_x,
                        y,
                        scale,
                    });
                    cur_x += child.width * scale;
                }
                for action in child_actions.into_vec().into_iter().rev() {
                    pending.push(action);
                }
            }

            BoxContent::VBox(children) => {
                let mut cur_y = y - lbox.height * scale;
                let mut child_actions =
                    WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
                for child in children {
                    check(control)?;
                    match &child.kind {
                        VBoxChildKind::Box(b) => {
                            cur_y += b.height * scale;
                            child_actions.push(EmitAction::Box {
                                lbox: b,
                                x: x + child.shift * scale,
                                y: cur_y,
                                scale,
                            });
                            cur_y += b.depth * scale;
                        }
                        VBoxChildKind::Kern(k) => {
                            cur_y += k * scale;
                        }
                    }
                }
                for action in child_actions.into_vec().into_iter().rev() {
                    pending.push(action);
                }
            }

            BoxContent::Glyph { font_id, char_code } => {
                charge_display_item(control)?;
                push_item(
                    items,
                    DisplayItem::GlyphPath {
                        x,
                        y,
                        scale,
                        font: font_text(*font_id, control)?,
                        char_code: *char_code,
                        color: lbox.color,
                    },
                    control,
                )?;
            }

            BoxContent::GlyphRun { glyphs } => {
                for glyph in glyphs {
                    check(control)?;
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::GlyphPath {
                            x: x + glyph.x * scale,
                            y,
                            scale,
                            font: font_text(glyph.font_id, control)?,
                            char_code: glyph.char_code,
                            color: lbox.color,
                        },
                        control,
                    )?;
                }
            }

            BoxContent::Rule { thickness, raise } => {
                // Baseline at `y` (downward screen coords); bottom of ink is `raise` em above baseline.
                let cy = y - (raise + thickness / 2.0) * scale;
                charge_display_item(control)?;
                push_item(
                    items,
                    DisplayItem::Line {
                        x,
                        y: cy,
                        width: lbox.width * scale,
                        thickness: thickness * scale,
                        color: lbox.color,
                        dashed: false,
                    },
                    control,
                )?;
            }

            BoxContent::Fraction {
                numer,
                denom,
                numer_shift,
                denom_shift,
                bar_thickness,
                numer_scale: n_sc,
                denom_scale: d_sc,
            } => {
                let child_numer_scale = scale * n_sc;
                let child_denom_scale = scale * d_sc;

                if *bar_thickness > 0.0 {
                    let metrics = ratex_font::get_global_metrics(0);
                    pending.push(EmitAction::Item(DisplayItem::Line {
                        x,
                        y: y - metrics.axis_height * scale,
                        width: lbox.width * scale,
                        thickness: bar_thickness * scale,
                        color: lbox.color,
                        dashed: false,
                    }));
                }
                let denom_x = x + (lbox.width * scale - denom.width * child_denom_scale) / 2.0;
                pending.push(EmitAction::Box {
                    lbox: denom,
                    x: denom_x,
                    y: y + denom_shift * scale,
                    scale: child_denom_scale,
                });
                let numer_x = x + (lbox.width * scale - numer.width * child_numer_scale) / 2.0;
                pending.push(EmitAction::Box {
                    lbox: numer,
                    x: numer_x,
                    y: y - numer_shift * scale,
                    scale: child_numer_scale,
                });
            }

            BoxContent::SupSub {
                base,
                sup,
                sub,
                sup_shift,
                sub_shift,
                sup_scale: ss,
                sub_scale: bs,
                center_scripts,
                italic_correction,
                sub_h_kern,
            } => {
                let base_x = if *center_scripts {
                    x + (lbox.width - base.width) * scale / 2.0
                } else {
                    x
                };
                let mut child_actions =
                    WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
                child_actions.push(EmitAction::Box {
                    lbox: base,
                    x: base_x,
                    y,
                    scale,
                });
                if let Some(sup_box) = sup {
                    let child_scale = scale * ss;
                    let sup_x = if *center_scripts {
                        x + (lbox.width * scale - sup_box.width * child_scale) / 2.0
                    } else {
                        base_x + (base.width + italic_correction) * scale
                    };
                    child_actions.push(EmitAction::Box {
                        lbox: sup_box,
                        x: sup_x,
                        y: y - sup_shift * scale,
                        scale: child_scale,
                    });
                }
                if let Some(sub_box) = sub {
                    let child_scale = scale * bs;
                    let sub_x = if *center_scripts {
                        x + (lbox.width * scale - sub_box.width * child_scale) / 2.0
                    } else {
                        base_x + base.width * scale + sub_h_kern * scale
                    };
                    child_actions.push(EmitAction::Box {
                        lbox: sub_box,
                        x: sub_x,
                        y: y + sub_shift * scale,
                        scale: child_scale,
                    });
                }
                for action in child_actions.into_vec().into_iter().rev() {
                    pending.push(action);
                }
            }

            BoxContent::Radical {
                body,
                index,
                index_offset,
                index_scale,
                rule_thickness,
                inner_height,
            } => {
                let radical_width = lbox.width - index_offset - body.width;

                let surd_x = x + index_offset * scale;

                let index_action = if let Some(index_box) = index {
                    // Root index (scriptscript): KaTeX `htmlBuilder` builds a vlist with
                    // `positionType: "shift", positionData: -toShift` where
                    // `toShift = 0.6 * (body.height - body.depth)` and `body` is the sqrt
                    // inner+vlist (same height/depth as this radical box, excluding the index).
                    // That aligns the root span with the sqrt body on the math baseline, then
                    // shifts the glyph upward by `toShift` — not pinned to the top of the surd.
                    //
                    // Horizontal: KaTeX places the index at `\mkern 5mu` (5/18 em) from the LEFT
                    // of the surd glyph, matching `.sqrt > .root { margin-left: 5/18em }`.
                    // Using surd_x as the reference makes this scale correctly for all surd sizes
                    // and for nested radicals where each level has a different index_offset.
                    let to_shift = 0.6 * (lbox.height - lbox.depth);
                    let index_baseline_y = y - to_shift * scale;
                    let child_scale = scale * index_scale;
                    Some(EmitAction::Box {
                        lbox: index_box,
                        x: surd_x + (5.0 / 18.0) * scale,
                        y: index_baseline_y,
                        scale: child_scale,
                    })
                } else {
                    None
                };
                let surd_font = surd_font_for_inner_height(*inner_height);
                let tall = *inner_height > 3.0
                    && control.is_some_and(|control| control.storage().is_some());
                let surd_char = if tall { 0x23B7 } else { SURD_CHAR };
                let surd_metrics = ratex_font::get_char_metrics(surd_font, surd_char);
                let (gh, gd) = match surd_metrics {
                    Some(metrics) => (metrics.height, metrics.depth),
                    None if tall => {
                        let error = ExecutionError::Unsupported {
                            phase: Phase::Display,
                        };
                        return Err(control.map_or(error, |control| control.fail(error)));
                    }
                    None => (lbox.height - rule_thickness, 0.0),
                };
                let surd_y = if tall {
                    y + (lbox.depth - gd) * scale
                } else {
                    y - (lbox.height - rule_thickness - gh) * scale
                };
                let surd_action = EmitAction::Item(DisplayItem::GlyphPath {
                    x: surd_x,
                    y: surd_y,
                    scale,
                    font: font_text(surd_font, control)?,
                    char_code: surd_char,
                    color: lbox.color,
                });

                // Vinculum: a horizontal rule extending the glyph's top bar over the body.
                // Center it on the glyph's top bar position.
                let bar_top = if tall {
                    y - (lbox.height - rule_thickness) * scale
                } else {
                    surd_y - gh * scale
                };
                // KaTeX v0.16.38 svgGeometry.ts sqrtTall (MIT; tag commit
                // eda872c939f78f4c999ea67560463ab954a5306c) uses the original
                // U+23B7 hook with its upright at x=.702..742em. Original notice:
                // Copyright (c) 2013-2020 Khan Academy and other contributors;
                // retained in markdown-math/assets/licenses/KaTeX-LICENSE.txt.
                // A .008em lap joins the existing glyph to the finite stem.
                // The overbar spans the hook's advance too, so no gap remains.
                let line_left = if tall { 0.702 } else { radical_width };
                let line_center_y = bar_top + (rule_thickness * scale) / 2.0;
                let line_action = EmitAction::Item(DisplayItem::Line {
                    x: surd_x + line_left * scale,
                    y: line_center_y,
                    width: (radical_width - line_left + body.width) * scale,
                    thickness: rule_thickness * scale,
                    color: lbox.color,
                    dashed: false,
                });

                pending.push(EmitAction::Box {
                    lbox: body,
                    x: surd_x + radical_width * scale,
                    y,
                    scale,
                });
                pending.push(line_action);
                if tall {
                    pending.push(EmitAction::Item(DisplayItem::Rect {
                        x: surd_x + 0.702 * scale,
                        y: bar_top,
                        width: 0.04 * scale,
                        height: surd_y - gh * scale - bar_top + 0.008 * scale,
                        color: lbox.color,
                    }));
                }
                pending.push(surd_action);
                if let Some(action) = index_action {
                    pending.push(action);
                }
            }

            BoxContent::OpLimits {
                base,
                sup,
                sub,
                base_shift,
                sup_kern,
                sub_kern,
                slant,
                sup_scale: ss,
                sub_scale: bs,
            } => {
                let base_x = x + (lbox.width - base.width) * scale / 2.0;
                let mut child_actions =
                    WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
                child_actions.push(EmitAction::Box {
                    lbox: base,
                    x: base_x,
                    y: y + base_shift * scale,
                    scale,
                });
                if let Some(sup_box) = sup {
                    let child_scale = scale * ss;
                    let sup_x = x
                        + (lbox.width * scale - sup_box.width * child_scale) / 2.0
                        + slant * scale / 2.0;
                    let sup_y = y
                        - (base.height - base_shift) * scale
                        - sup_kern * scale
                        - sup_box.depth * child_scale;
                    child_actions.push(EmitAction::Box {
                        lbox: sup_box,
                        x: sup_x,
                        y: sup_y,
                        scale: child_scale,
                    });
                }
                if let Some(sub_box) = sub {
                    let child_scale = scale * bs;
                    let sub_x = x + (lbox.width * scale - sub_box.width * child_scale) / 2.0
                        - slant * scale / 2.0;
                    let sub_y = y
                        + (base.depth + base_shift) * scale
                        + sub_kern * scale
                        + sub_box.height * child_scale;
                    child_actions.push(EmitAction::Box {
                        lbox: sub_box,
                        x: sub_x,
                        y: sub_y,
                        scale: child_scale,
                    });
                }
                for action in child_actions.into_vec().into_iter().rev() {
                    pending.push(action);
                }
            }

            BoxContent::Accent {
                base,
                accent,
                clearance,
                skew,
                is_below,
                under_gap_em,
            } => {
                let accent_action = if *is_below {
                    let accent_x = x + (base.width - accent.width) * scale / 2.0;
                    let accent_y = y + (base.depth + under_gap_em) * scale + accent.height * scale;
                    EmitAction::Box {
                        lbox: accent,
                        x: accent_x,
                        y: accent_y,
                        scale,
                    }
                } else {
                    let accent_x = x + (base.width - accent.width) * scale / 2.0 + skew * scale;
                    // Position accent so its TOP is at (clearance + effective_accent_height) above baseline.
                    // For SVG accents (height=0, depth=h), position at clearance above baseline.
                    // For glyph accents (height>0, depth=0), the visual mark is at the top of the glyph.
                    // Place the glyph so its top aligns with clearance + small_mark_height above baseline.
                    let is_svg_accent = accent.height <= 0.001;
                    let accent_y = if is_svg_accent {
                        y - clearance * scale - accent.depth * scale
                    } else {
                        // Glyph accent: position baseline so top of glyph = clearance above text baseline
                        // accent_y - accent.height = y - (lbox.height) where lbox.height = clearance + eff_h
                        // => accent_y = y - clearance - eff_h + accent.height
                        // Simpler: shift glyph so top is at y - clearance - small_gap
                        y - clearance * scale
                            + (accent.height - 0.35_f64.min(accent.height)) * scale
                    };
                    EmitAction::Box {
                        lbox: accent,
                        x: accent_x,
                        y: accent_y,
                        scale,
                    }
                };
                pending.push(accent_action);
                pending.push(EmitAction::Box {
                    lbox: base,
                    x,
                    y,
                    scale,
                });
            }

            BoxContent::LeftRight { left, right, inner } => {
                let inner_x = x + left.width * scale;
                let right_x = inner_x + inner.width * scale;
                pending.push(EmitAction::Box {
                    lbox: right,
                    x: right_x,
                    y,
                    scale,
                });
                pending.push(EmitAction::Box {
                    lbox: inner,
                    x: inner_x,
                    y,
                    scale,
                });
                pending.push(EmitAction::Box {
                    lbox: left,
                    x,
                    y,
                    scale,
                });
            }

            BoxContent::Array {
                cells,
                col_widths,
                col_aligns,
                row_heights,
                row_depths,
                col_gaps,
                offset,
                content_x_offset,
                content_x_end_offset: _content_x_end_offset,
                col_separators,
                hlines_before_row,
                rule_thickness,
                double_rule_sep,
                array_inner_width,
                tag_gap_em,
                tag_col_width,
                row_tags,
                tags_left: _tags_left,
            } => {
                let y_top = y - offset * scale;
                let array_total_height = (lbox.height + lbox.depth) * scale;
                let line_thickness = rule_thickness * scale;

                // Compute y positions of row boundaries (0 = top of array, num_rows = bottom).
                let mut boundary_ys: Vec<f64> = Vec::with_capacity(cells.len() + 1);
                let mut cur_boundary = y_top;
                boundary_ys.push(cur_boundary);
                for r in 0..cells.len() {
                    check(control)?;
                    cur_boundary += (row_heights[r] + row_depths[r]) * scale;
                    boundary_ys.push(cur_boundary);
                }

                // Draw horizontal lines (hlines) before/after each row.
                // Each entry in hlines vec is one \hline (false) or \hdashline (true).
                // Consecutive rules are separated by double_rule_sep (= \doublerulesep = 2pt).
                //
                // Extra space for n > 1 hlines was already added to the layout:
                //   - r == 0: added to row_heights[0], so lines start at boundary_ys[0] going down.
                //   - r >= 1: added to row_depths[r-1], so lines occupy the range
                //             [boundary_ys[r] - (n-1)*rule_step, boundary_ys[r]], above row r.
                let rule_step = line_thickness + double_rule_sep * scale;
                for (r, hlines) in hlines_before_row.iter().enumerate() {
                    check(control)?;
                    if r < boundary_ys.len() {
                        let n = hlines.len();
                        let start_y = if r == 0 {
                            boundary_ys[0]
                        } else {
                            boundary_ys[r] - (n.saturating_sub(1)) as f64 * rule_step
                        };
                        for (i, &is_dashed) in hlines.iter().enumerate() {
                            check(control)?;
                            charge_display_item(control)?;
                            push_item(
                                items,
                                DisplayItem::Line {
                                    x,
                                    y: start_y + i as f64 * rule_step,
                                    width: array_inner_width * scale,
                                    thickness: line_thickness,
                                    color: lbox.color,
                                    dashed: is_dashed,
                                },
                                control,
                            )?;
                        }
                    }
                }

                // Draw vertical column separator lines ('|' = solid, ':' = dashed).
                // Put a separator at the midpoint of its column boundary gap.
                for (i, sep) in col_separators.iter().enumerate() {
                    check(control)?;
                    if let Some(is_dashed) = sep {
                        let prefix_w: f64 = col_widths[..i].iter().sum();
                        let preceding_gaps: f64 = col_gaps[..i.saturating_sub(1)].iter().sum();
                        let local_x = if i == 0 {
                            0.0
                        } else if i >= col_widths.len() {
                            *array_inner_width
                        } else {
                            content_x_offset + prefix_w + preceding_gaps + col_gaps[i - 1] / 2.0
                        };
                        let abs_x = x + local_x * scale - line_thickness / 2.0;
                        if *is_dashed {
                            // Use the same local segment count as checked layout.
                            // Indexed positions cannot stall at large coordinates,
                            // and scaling never changes the number of segments.
                            let height = lbox.height + lbox.depth;
                            let dash = 4.0 * rule_thickness;
                            let period = 2.0 * dash;
                            for i in 0..vertical_dash_count(height, *rule_thickness) {
                                check(control)?;
                                let offset = i as f64 * period;
                                let seg_h = dash.min((height - offset).max(0.0));
                                charge_display_item(control)?;
                                push_item(
                                    items,
                                    DisplayItem::Rect {
                                        x: abs_x,
                                        y: y_top + offset * scale,
                                        width: line_thickness,
                                        height: seg_h * scale,
                                        color: lbox.color,
                                    },
                                    control,
                                )?;
                            }
                        } else {
                            charge_display_item(control)?;
                            push_item(
                                items,
                                DisplayItem::Rect {
                                    x: abs_x,
                                    y: y_top,
                                    width: line_thickness,
                                    height: array_total_height,
                                    color: lbox.color,
                                },
                                control,
                            )?;
                        }
                    }
                }

                // Render cells.
                let mut cur_y = y_top;
                let mut child_actions =
                    WorkVec::new(control, Phase::Display, StorageFamily::Auxiliary);
                for (r, row) in cells.iter().enumerate() {
                    check(control)?;
                    let rh = row_heights[r];
                    cur_y += rh * scale;
                    let mut cur_x = x + content_x_offset * scale;
                    for (c, cell) in row.iter().enumerate() {
                        check(control)?;
                        let cw = col_widths[c];
                        let align = col_aligns.get(c).copied().unwrap_or(b'c');
                        let cell_x = match align {
                            b'l' => cur_x,
                            b'r' => cur_x + (cw - cell.width) * scale,
                            _ => cur_x + (cw - cell.width) * scale / 2.0,
                        };
                        child_actions.push(EmitAction::Box {
                            lbox: cell,
                            x: cell_x,
                            y: cur_y,
                            scale,
                        });
                        cur_x += cw * scale;
                        if c + 1 < row.len() {
                            cur_x += col_gaps.get(c).copied().unwrap_or(0.0) * scale;
                        }
                    }
                    if *tag_col_width > 0.0 {
                        if let Some(tb) = row_tags.get(r).and_then(|o| o.as_ref()) {
                            let tag_start_em = array_inner_width + tag_gap_em;
                            let tag_x =
                                x + tag_start_em * scale + (tag_col_width - tb.width) * scale;
                            child_actions.push(EmitAction::Box {
                                lbox: tb,
                                x: tag_x,
                                y: cur_y,
                                scale,
                            });
                        }
                    }
                    cur_y += row_depths[r] * scale;
                }
                for action in child_actions.into_vec().into_iter().rev() {
                    pending.push(action);
                }
            }

            BoxContent::SvgPath { commands, fill } => {
                let controlled =
                    control.and_then(|control| control.storage().map(|storage| (control, storage)));
                if let Some((control, _)) = &controlled {
                    if !crate::resource_limits::is_bounded_fill_path(commands, *fill) {
                        return Err(control.fail(ExecutionError::Unsupported {
                            phase: Phase::Display,
                        }));
                    }
                }
                charge_display_item(control)?;
                // A contour covering the box exactly can go straight to the rectangle renderer.
                if controlled.is_some()
                    && *fill
                    && commands.len() == 5
                    && commands.as_slice()
                        == LayoutBox::full_rect_path(lbox.width, lbox.height, lbox.depth)
                {
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x,
                            y: y - lbox.height * scale,
                            width: lbox.width * scale,
                            height: lbox.total_height() * scale,
                            color: lbox.color,
                        },
                        control,
                    )?;
                    continue;
                }
                let scaled = if let Some((control, storage)) = controlled {
                    let mut scaled = storage
                        .try_vec_with_capacity(
                            Phase::Display,
                            StorageFamily::Display,
                            commands.len(),
                        )
                        .map_err(|error| storage_error(error, control))?;
                    for command in commands {
                        check(Some(control))?;
                        scaled.push(scale_path_command(command, scale));
                    }
                    scaled
                } else {
                    commands
                        .iter()
                        .map(|command| scale_path_command(command, scale))
                        .collect()
                };
                push_item(
                    items,
                    DisplayItem::Path {
                        x,
                        y,
                        commands: scaled,
                        fill: *fill,
                        color: lbox.color,
                    },
                    control,
                )?;
            }

            BoxContent::Framed {
                body,
                padding,
                border_thickness,
                has_border,
                bg_color,
                border_color,
            } => {
                let outer_w = lbox.width * scale;
                let outer_h = lbox.height * scale;
                let outer_d = lbox.depth * scale;
                let top_y = y - outer_h;

                // Background fill
                if let Some(bg) = bg_color {
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x,
                            y: top_y,
                            width: outer_w,
                            height: outer_h + outer_d,
                            color: *bg,
                        },
                        control,
                    )?;
                }

                // Border (4 sides as Rect strips)
                if *has_border {
                    let bt = border_thickness * scale;
                    // Top
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x,
                            y: top_y,
                            width: outer_w,
                            height: bt,
                            color: *border_color,
                        },
                        control,
                    )?;
                    // Bottom
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x,
                            y: y + outer_d - bt,
                            width: outer_w,
                            height: bt,
                            color: *border_color,
                        },
                        control,
                    )?;
                    // Left
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x,
                            y: top_y,
                            width: bt,
                            height: outer_h + outer_d,
                            color: *border_color,
                        },
                        control,
                    )?;
                    // Right
                    charge_display_item(control)?;
                    push_item(
                        items,
                        DisplayItem::Rect {
                            x: x + outer_w - bt,
                            y: top_y,
                            width: bt,
                            height: outer_h + outer_d,
                            color: *border_color,
                        },
                        control,
                    )?;
                }

                // Body content (shifted by padding + border from left baseline)
                let inner_offset = (padding + border_thickness) * scale;
                pending.push(EmitAction::Box {
                    lbox: body,
                    x: x + inner_offset,
                    y,
                    scale,
                });
            }

            BoxContent::RaiseBox { body, shift } => {
                pending.push(EmitAction::Box {
                    lbox: body,
                    x,
                    y: y - shift * scale,
                    scale,
                });
            }

            BoxContent::Scaled { body, child_scale } => {
                pending.push(EmitAction::Box {
                    lbox: body,
                    x,
                    y,
                    scale: scale * child_scale,
                });
            }

            BoxContent::Angl {
                path_commands,
                body,
            } => {
                let scaled: Vec<PathCommand> = path_commands
                    .iter()
                    .map(|c| scale_path_command(c, scale))
                    .collect();
                charge_display_item(control)?;
                push_item(
                    items,
                    DisplayItem::Path {
                        x,
                        y,
                        commands: scaled,
                        fill: false,
                        color: lbox.color,
                    },
                    control,
                )?;
                pending.push(EmitAction::Box {
                    lbox: body,
                    x,
                    y,
                    scale,
                });
            }

            BoxContent::Overline {
                body,
                rule_thickness,
                offset,
            } => {
                let rule_center_y = y - (body.height + offset) * scale;
                pending.push(EmitAction::Item(DisplayItem::Line {
                    x,
                    y: rule_center_y,
                    width: lbox.width * scale,
                    thickness: rule_thickness * scale,
                    color: lbox.color,
                    dashed: false,
                }));
                pending.push(EmitAction::Box {
                    lbox: body,
                    x,
                    y,
                    scale,
                });
            }

            BoxContent::Underline {
                body,
                rule_thickness,
                offset,
            } => {
                let rule_center_y = y + (body.depth + offset) * scale;
                pending.push(EmitAction::Item(DisplayItem::Line {
                    x,
                    y: rule_center_y,
                    width: lbox.width * scale,
                    thickness: rule_thickness * scale,
                    color: lbox.color,
                    dashed: false,
                }));
                pending.push(EmitAction::Box {
                    lbox: body,
                    x,
                    y,
                    scale,
                });
            }

            BoxContent::ProofTree { children, rules } => {
                let top_y = y - lbox.height * scale;
                for rule in rules.iter().rev() {
                    check(control)?;
                    pending.push(EmitAction::Item(DisplayItem::Line {
                        x: x + rule.x * scale,
                        y: top_y + rule.y * scale,
                        width: rule.width * scale,
                        thickness: rule.thickness * scale,
                        color: lbox.color,
                        dashed: rule.dashed,
                    }));
                }
                for child in children.iter().rev() {
                    check(control)?;
                    pending.push(EmitAction::Box {
                        lbox: &child.box_,
                        x: x + child.x * scale,
                        y: top_y + child.baseline_y * scale,
                        scale,
                    });
                }
            }

            BoxContent::Kern | BoxContent::Empty => {}
        }
    }
    check(control)?;
    Ok(())
}

fn scale_path_command(cmd: &PathCommand, scale: f64) -> PathCommand {
    match *cmd {
        PathCommand::MoveTo { x, y } => PathCommand::MoveTo {
            x: x * scale,
            y: y * scale,
        },
        PathCommand::LineTo { x, y } => PathCommand::LineTo {
            x: x * scale,
            y: y * scale,
        },
        PathCommand::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => PathCommand::CubicTo {
            x1: x1 * scale,
            y1: y1 * scale,
            x2: x2 * scale,
            y2: y2 * scale,
            x: x * scale,
            y: y * scale,
        },
        PathCommand::QuadTo { x1, y1, x, y } => PathCommand::QuadTo {
            x1: x1 * scale,
            y1: y1 * scale,
            x: x * scale,
            y: y * scale,
        },
        PathCommand::Close => PathCommand::Close,
    }
}

/// Compute the visual bounding box from line, rect, and path items (paths only when
/// coordinates are within a sane em range — skips huge KaTeX `viewBox` artifacts).
/// Returns (min_x, max_x, min_y, max_y) in em coordinates.
fn compute_visual_bounds(
    items: &[DisplayItem],
    control: Option<&ExecutionControl<'_>>,
    runs: &[ChineseTextRun<'_>],
) -> Result<(f64, f64, f64, f64), ExecutionError> {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for item in items {
        check(control)?;
        match item {
            // Compute glyph extent from font metrics so edge cases like \smash
            // (zero nominal height) and \mathllap (zero nominal width) are
            // correctly sized: the pixmap is dimensioned from nominal bounds,
            // but smashed/llap boxes have near-zero nominal dimensions.
            DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                ..
            } => {
                let font_id =
                    ratex_font::FontId::parse(font).unwrap_or(ratex_font::FontId::MainRegular);
                let (w, h, d) = chinese_text_run_index(font, *char_code)
                    .and_then(|index| runs.get(index)?.metrics)
                    .map(|m| (m.width, m.height, m.depth))
                    .or_else(|| {
                        ratex_font::get_char_metrics(font_id, *char_code)
                            .map(|m| (m.width, m.height, m.depth))
                    })
                    .unwrap_or((0.0, 0.0, 0.0));
                min_x = min_x.min(*x);
                max_x = max_x.max(x + w * scale);
                min_y = min_y.min(y - h * scale);
                max_y = max_y.max(y + d * scale);
            }
            DisplayItem::Line {
                x,
                y,
                width,
                thickness,
                ..
            } => {
                min_x = min_x.min(*x);
                max_x = max_x.max(x + width);
                min_y = min_y.min(y - thickness / 2.0);
                max_y = max_y.max(y + thickness / 2.0);
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                ..
            } => {
                min_x = min_x.min(*x);
                max_x = max_x.max(x + width);
                min_y = min_y.min(*y);
                max_y = max_y.max(y + height);
            }
            // Paths in document em space (delimiters, accents): include bbox so tall `tallDelim`
            // curves are not clipped at the pixmap edge. Skip astronomical KaTeX coords (e.g. \phase).
            DisplayItem::Path {
                x: px,
                y: py,
                commands,
                ..
            } => {
                const MAX_EM: f64 = 50.0;
                for cmd in commands {
                    check(control)?;
                    let mut consider = |cx: f64, cy: f64| {
                        if cx.abs() <= MAX_EM && cy.abs() <= MAX_EM {
                            let abs_x = px + cx;
                            let abs_y = py + cy;
                            min_x = min_x.min(abs_x);
                            max_x = max_x.max(abs_x);
                            min_y = min_y.min(abs_y);
                            max_y = max_y.max(abs_y);
                        }
                    };
                    match cmd {
                        PathCommand::MoveTo { x: cx, y: cy }
                        | PathCommand::LineTo { x: cx, y: cy } => {
                            consider(*cx, *cy);
                        }
                        PathCommand::CubicTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => {
                            consider(*x1, *y1);
                            consider(*x2, *y2);
                            consider(*x, *y);
                        }
                        PathCommand::QuadTo { x1, y1, x, y } => {
                            consider(*x1, *y1);
                            consider(*x, *y);
                        }
                        PathCommand::Close => {}
                    }
                }
            }
        }
    }

    Ok((min_x, max_x, min_y, max_y))
}

fn shift_item_y(item: &mut DisplayItem, dy: f64) {
    match item {
        DisplayItem::GlyphPath { y, .. } => *y += dy,
        DisplayItem::Line { y, .. } => *y += dy,
        DisplayItem::Rect { y, .. } => *y += dy,
        DisplayItem::Path { y, .. } => *y += dy,
    }
}

fn shift_item_x(item: &mut DisplayItem, dx: f64) {
    match item {
        DisplayItem::GlyphPath { x, .. } => *x += dx,
        DisplayItem::Line { x, .. } => *x += dx,
        DisplayItem::Rect { x, .. } => *x += dx,
        DisplayItem::Path { x, .. } => *x += dx,
    }
}

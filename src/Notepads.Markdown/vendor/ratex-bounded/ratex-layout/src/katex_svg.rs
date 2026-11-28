use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use ratex_types::PathCommand;
use ratex_types::execution::{ExecutionControl, ExecutionError, Phase};
use ratex_types::storage::{StorageFamily, WorkVec};

/// Returns (commands, width, height, fill) for a KaTeX SVG accent, or `None` if unhandled.
///
/// `group_len` is KaTeX’s `groupLength(base)` (`ordgroup.body.length`) for `\\widehat` /
/// `\\widecheck` / `\\widetilde` / `\\utilde` — variant choice uses this, not base width.
pub fn katex_accent_path(
    label: &str,
    base_width_em: f64,
    group_len: usize,
) -> Option<(Vec<PathCommand>, f64, f64, bool)> {
    match label {
        "\\vec" => {
            // KaTeX `svgGeometry.path.vec` (glyph U+20D7) + `svgData.vec` fixed size — not stretched to base width.
            const W_EM: f64 = 0.471;
            const H_EM: f64 = 0.714;
            let raw = parsed_static_path(KATEX_VEC_PATH);
            let cmds = scale_svg_path_thousandths(&raw);
            Some((cmds, W_EM, H_EM, true))
        }
        "\\widehat" | "\\widecheck" => {
            let is_hat = label == "\\widehat";
            let (path, vb_w, vb_h, h_em) = select_hat_check(is_hat, group_len);
            let cmds = parse_and_fit_nonuniform(path, vb_w, vb_h, base_width_em, h_em);
            Some((cmds, base_width_em, h_em, true))
        }
        "\\widetilde" | "\\utilde" => {
            let (path, vb_w, vb_h, h_em) = select_tilde(group_len);
            let cmds = parse_and_fit_nonuniform(path, vb_w, vb_h, base_width_em, h_em);
            Some((cmds, base_width_em, h_em, true))
        }
        "\\overgroup" => {
            // KaTeX: viewBoxHeight=342, height=0.342
            let h_em = 0.342;
            let cmds = build_overgroup(base_width_em, h_em, true);
            Some((cmds, base_width_em, h_em, true))
        }
        "\\undergroup" => {
            let h_em = 0.342;
            let cmds = build_overgroup(base_width_em, h_em, false);
            Some((cmds, base_width_em, h_em, true))
        }
        _ => None,
    }
}

/// Returns path commands for KaTeX stretchy arrows. Delegates to `katex_stretchy_path`.
/// Kept for backward compatibility with existing callers.
pub fn katex_stretchy_arrow_path(
    label: &str,
    width_em: f64,
    _height_em: f64,
) -> Option<Vec<PathCommand>> {
    katex_stretchy_path(label, width_em).map(|(cmds, _)| cmds)
}

fn scale_cmd_twohead_uniform(cmd: &PathCommand, s: f64, vb_cy: f64, x_shift: f64) -> PathCommand {
    match *cmd {
        PathCommand::MoveTo { x, y } => PathCommand::MoveTo {
            x: x * s + x_shift,
            y: (y - vb_cy) * s,
        },
        PathCommand::LineTo { x, y } => PathCommand::LineTo {
            x: x * s + x_shift,
            y: (y - vb_cy) * s,
        },
        PathCommand::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => PathCommand::CubicTo {
            x1: x1 * s + x_shift,
            y1: (y1 - vb_cy) * s,
            x2: x2 * s + x_shift,
            y2: (y2 - vb_cy) * s,
            x: x * s + x_shift,
            y: (y - vb_cy) * s,
        },
        PathCommand::QuadTo { x1, y1, x, y } => PathCommand::QuadTo {
            x1: x1 * s + x_shift,
            y1: (y1 - vb_cy) * s,
            x: x * s + x_shift,
            y: (y - vb_cy) * s,
        },
        PathCommand::Close => PathCommand::Close,
    }
}

/// Clip path to rectangle [x_min, x_max] x [y_min, y_max]. Curves are flattened then clipped.
/// For filled closed contours, the clip boundary edges are implicitly added by emitting
/// LineTo(a) whenever the start of a newly-visible segment doesn't match the current position.
fn clip_path_to_rect(
    commands: &[PathCommand],
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Vec<PathCommand> {
    let contours = flatten_path_to_contours(commands);
    let mut out = Vec::new();
    for contour in contours {
        if contour.len() < 2 {
            continue;
        }
        let mut need_move = true;
        let mut last_pt: (f64, f64) = (f64::NAN, f64::NAN);

        let n = contour.len();
        // Process all n edges: 0→1, 1→2, …, (n-1)→0 (closing edge).
        for i in 0..n {
            let p0 = contour[i];
            let p1 = contour[(i + 1) % n];
            let segments = clip_segment_to_rect(p0, p1, x_min, x_max, y_min, y_max);
            for (a, b) in segments {
                if need_move {
                    out.push(PathCommand::MoveTo { x: a.0, y: a.1 });
                    need_move = false;
                } else if (a.0 - last_pt.0).abs() > 1e-9 || (a.1 - last_pt.1).abs() > 1e-9 {
                    // Gap due to clipped-away segments: emit the clip-boundary start point
                    // so the contour remains closed and the clip edge is explicitly drawn.
                    out.push(PathCommand::LineTo { x: a.0, y: a.1 });
                }
                out.push(PathCommand::LineTo { x: b.0, y: b.1 });
                last_pt = b;
            }
        }
    }
    out
}

fn flatten_path_to_contours(commands: &[PathCommand]) -> Vec<Vec<(f64, f64)>> {
    let mut contours = Vec::new();
    let mut current = Vec::new();
    let mut last = (0.0_f64, 0.0_f64);
    const N: usize = 16; // steps per curve

    for cmd in commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                if !current.is_empty() {
                    contours.push(std::mem::take(&mut current));
                }
                last = (x, y);
                current.push(last);
            }
            PathCommand::LineTo { x, y } => {
                last = (x, y);
                current.push(last);
            }
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x0, y0) = last;
                for k in 1..=N {
                    let t = k as f64 / N as f64;
                    let u = 1.0 - t;
                    let x = u * u * u * x0
                        + 3.0 * u * u * t * x1
                        + 3.0 * u * t * t * x2
                        + t * t * t * x;
                    let y = u * u * u * y0
                        + 3.0 * u * u * t * y1
                        + 3.0 * u * t * t * y2
                        + t * t * t * y;
                    last = (x, y);
                    current.push(last);
                }
            }
            PathCommand::QuadTo { x1, y1, x, y } => {
                let (x0, y0) = last;
                for k in 1..=N {
                    let t = k as f64 / N as f64;
                    let u = 1.0 - t;
                    let x = u * u * x0 + 2.0 * u * t * x1 + t * t * x;
                    let y = u * u * y0 + 2.0 * u * t * y1 + t * t * y;
                    last = (x, y);
                    current.push(last);
                }
            }
            PathCommand::Close => {
                if !current.is_empty() {
                    contours.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        contours.push(current);
    }
    contours
}

/// Clip line segment (p0,p1) to rectangle. Returns 0 or 1 segment (as (start,end)) inside the rect.
/// Uses Liang-Barsky.
fn clip_segment_to_rect(
    p0: (f64, f64),
    p1: (f64, f64),
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Vec<((f64, f64), (f64, f64))> {
    let (x0, y0) = p0;
    let (x1, y1) = p1;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;

    let mut clip = |p: f64, q: f64| -> bool {
        if p.abs() < 1e-12 {
            return q >= 0.0;
        }
        let t = q / p;
        if p < 0.0 {
            if t > t1 {
                return false;
            }
            if t > t0 {
                t0 = t;
            }
        } else {
            if t < t0 {
                return false;
            }
            if t < t1 {
                t1 = t;
            }
        }
        true
    };

    if !clip(-dx, x0 - x_min) {
        return vec![];
    }
    if !clip(dx, x_max - x0) {
        return vec![];
    }
    if !clip(-dy, y0 - y_min) {
        return vec![];
    }
    if !clip(dy, y_max - y0) {
        return vec![];
    }

    if t0 >= t1 - 1e-9 {
        return vec![];
    }
    let a = (x0 + t0 * dx, y0 + t0 * dy);
    let b = (x0 + t1 * dx, y0 + t1 * dy);
    vec![(a, b)]
}

/// KaTeX `svgGeometry.js` path `vec` (from Main U+20D7); viewBox `0 0 471 714` (= 1000×0.471 by 1000×0.714).
const KATEX_VEC_PATH: &str = "M377 20c0-5.333 1.833-10 5.5-14S391 0 397 0c4.667 0 8.667 1.667 12 5 3.333 2.667 6.667 9 10 19 6.667 24.667 20.333 43.667 41 57 7.333 4.667 11 10.667 11 18 0 6-1 10-3 12s-6.667 5-14 9c-28.667 14.667-53.667 35.667-75 63 -1.333 1.333-3.167 3.5-5.5 6.5s-4 4.833-5 5.5c-1 .667-2.5 1.333-4.5 2s-4.333 1 -7 1c-4.667 0-9.167-1.833-13.5-5.5S337 184 337 178c0-12.667 15.667-32.333 47-59 H213l-171-1c-8.667-6-13-12.333-13-19 0-4.667 4.333-11.333 13-20h359 c-16-25.333-24-45-24-59z";

fn scale_svg_path_thousandths(cmds: &[PathCommand]) -> Vec<PathCommand> {
    let s = 0.001;
    cmds.iter()
        .map(|c| match *c {
            PathCommand::MoveTo { x, y } => PathCommand::MoveTo { x: x * s, y: y * s },
            PathCommand::LineTo { x, y } => PathCommand::LineTo { x: x * s, y: y * s },
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => PathCommand::CubicTo {
                x1: x1 * s,
                y1: y1 * s,
                x2: x2 * s,
                y2: y2 * s,
                x: x * s,
                y: y * s,
            },
            PathCommand::QuadTo { x1, y1, x, y } => PathCommand::QuadTo {
                x1: x1 * s,
                y1: y1 * s,
                x: x * s,
                y: y * s,
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect()
}

/// KaTeX `svgSpan` / `svgGeometry.js`: `imgIndex = [1,1,2,2,3,3][numChars]` for `numChars` ≤ 5;
/// `numChars > 5` uses the fourth (widest/tallest) asset.
fn wide_accent_img_index(group_len: usize) -> usize {
    if group_len > 5 {
        4
    } else {
        [1, 1, 2, 2, 3, 3][group_len]
    }
}

fn select_hat_check(is_hat: bool, group_len: usize) -> (&'static str, f64, f64, f64) {
    let img_index = wide_accent_img_index(group_len);
    let prefix = if is_hat { &WIDEHAT } else { &WIDECHECK };
    let vb_w = [0.0, 1062.0, 2364.0, 2364.0, 2364.0][img_index];
    let vb_h = [0.0, 239.0, 300.0, 360.0, 420.0][img_index];
    // KaTeX: `_height = [0, 0.24, 0.3, 0.3, 0.36, 0.42][imgIndex]`; `numChars > 5` forces 0.42.
    let h_em = if group_len > 5 {
        0.42
    } else {
        [0.0, 0.24, 0.3, 0.3, 0.36, 0.42][img_index]
    };
    (prefix[img_index - 1], vb_w, vb_h, h_em)
}

fn select_tilde(group_len: usize) -> (&'static str, f64, f64, f64) {
    let img_index = wide_accent_img_index(group_len);
    let vb_w = [0.0, 600.0, 1033.0, 2339.0, 2340.0][img_index];
    let vb_h = [0.0, 260.0, 286.0, 306.0, 312.0][img_index];
    let h_em = if group_len > 5 {
        0.34
    } else {
        [0.0, 0.26, 0.286, 0.3, 0.306, 0.34][img_index]
    };
    (TILDE[img_index - 1], vb_w, vb_h, h_em)
}

/// Parse SVG path and scale with independent X/Y factors (preserveAspectRatio="none").
fn parse_and_fit_nonuniform(
    svg_path: &'static str,
    vb_width: f64,
    vb_height: f64,
    target_width_em: f64,
    target_height_em: f64,
) -> Vec<PathCommand> {
    let raw = parsed_static_path(svg_path);
    let sx = target_width_em / vb_width;
    let sy = target_height_em / vb_height;
    raw.iter().map(|c| scale_cmd_xy(c, sx, sy)).collect()
}

/// Build the overgroup/undergroup filled path from KaTeX leftgroup/rightgroup data.
fn build_overgroup(width_em: f64, h_em: f64, is_over: bool) -> Vec<PathCommand> {
    // KaTeX overgroup: left half = leftgroup, right half = rightgroup
    // viewBox: 400000 × 342, height = 0.342em
    // We combine both halves into a single filled path.
    //
    // leftgroup outer curve (in viewBox units):
    //   (435, 80) → C(64,80)(168.3,229.4)(21,260) → small curves → (0,257) → V(0,219)
    //   Inner: (0,219) → C(76,61)(257,0)(435,0) → H to right → close
    //
    // rightgroup: mirror at the right end.
    let vb_h = 342.0;
    let sy = h_em / vb_h;

    // KaTeX's curves span 0..435 viewBox units at each end.
    // At 1000:1 scale, 435 units = 0.435em. Keep this natural size.
    // If the target is too narrow, shrink corners proportionally.
    let natural_corner: f64 = 0.435;
    let corner_em = natural_corner.min(width_em / 2.0);
    // cx converts curve x-coordinates (0..435) to em
    let cx = corner_em / 435.0;

    let ly = |y: f64| y * sy;

    if is_over {
        vec![
            // Start at top-left, after corner
            PathCommand::MoveTo {
                x: corner_em,
                y: ly(80.0),
            },
            // Left outer curve down
            PathCommand::CubicTo {
                x1: 64.0 * cx,
                y1: ly(80.0),
                x2: 168.3 * cx,
                y2: ly(229.4),
                x: 21.0 * cx,
                y: ly(260.0),
            },
            PathCommand::CubicTo {
                x1: 15.1 * cx,
                y1: ly(261.2),
                x2: 3.0 * cx,
                y2: ly(260.0),
                x: 3.0 * cx,
                y: ly(260.0),
            },
            PathCommand::CubicTo {
                x1: 1.0 * cx,
                y1: ly(260.0),
                x2: 0.0,
                y2: ly(259.0),
                x: 0.0,
                y: ly(257.0),
            },
            PathCommand::LineTo {
                x: 0.0,
                y: ly(219.0),
            },
            // Left inner curve up
            PathCommand::CubicTo {
                x1: 76.0 * cx,
                y1: ly(61.0),
                x2: 257.0 * cx,
                y2: ly(0.0),
                x: corner_em,
                y: ly(0.0),
            },
            // Bottom flat section
            PathCommand::LineTo {
                x: width_em - corner_em,
                y: ly(0.0),
            },
            // Right inner curve down
            PathCommand::CubicTo {
                x1: width_em - 257.0 * cx,
                y1: ly(0.0),
                x2: width_em - 76.0 * cx,
                y2: ly(61.0),
                x: width_em,
                y: ly(219.0),
            },
            PathCommand::LineTo {
                x: width_em,
                y: ly(257.0),
            },
            // Right outer curves up
            PathCommand::CubicTo {
                x1: width_em,
                y1: ly(259.0),
                x2: width_em - 1.0 * cx,
                y2: ly(260.0),
                x: width_em - 3.0 * cx,
                y: ly(260.0),
            },
            PathCommand::CubicTo {
                x1: width_em - 3.0 * cx,
                y1: ly(260.0),
                x2: width_em - 15.1 * cx,
                y2: ly(261.2),
                x: width_em - 21.0 * cx,
                y: ly(260.0),
            },
            PathCommand::CubicTo {
                x1: width_em - 168.3 * cx,
                y1: ly(229.4),
                x2: width_em - 64.0 * cx,
                y2: ly(80.0),
                x: width_em - corner_em,
                y: ly(80.0),
            },
            PathCommand::Close,
        ]
    } else {
        // undergroup: KaTeX leftgroupunder / rightgroupunder
        // leftgroupunder:
        //   (435, 262) → C(64,262)(168.3,112.6)(21,82) → curves → (0,85) → V(0,123)
        //   Inner: (0,123) → C(76,281)(257,342)(435,342) → H → close
        vec![
            PathCommand::MoveTo {
                x: corner_em,
                y: ly(262.0),
            },
            PathCommand::CubicTo {
                x1: 64.0 * cx,
                y1: ly(262.0),
                x2: 168.3 * cx,
                y2: ly(112.6),
                x: 21.0 * cx,
                y: ly(82.0),
            },
            PathCommand::CubicTo {
                x1: 15.1 * cx,
                y1: ly(80.8),
                x2: 3.0 * cx,
                y2: ly(82.0),
                x: 3.0 * cx,
                y: ly(82.0),
            },
            PathCommand::CubicTo {
                x1: 1.0 * cx,
                y1: ly(82.0),
                x2: 0.0,
                y2: ly(83.0),
                x: 0.0,
                y: ly(85.0),
            },
            PathCommand::LineTo {
                x: 0.0,
                y: ly(123.0),
            },
            PathCommand::CubicTo {
                x1: 76.0 * cx,
                y1: ly(281.0),
                x2: 257.0 * cx,
                y2: ly(342.0),
                x: corner_em,
                y: ly(342.0),
            },
            PathCommand::LineTo {
                x: width_em - corner_em,
                y: ly(342.0),
            },
            PathCommand::CubicTo {
                x1: width_em - 257.0 * cx,
                y1: ly(342.0),
                x2: width_em - 76.0 * cx,
                y2: ly(281.0),
                x: width_em,
                y: ly(123.0),
            },
            PathCommand::LineTo {
                x: width_em,
                y: ly(85.0),
            },
            PathCommand::CubicTo {
                x1: width_em,
                y1: ly(83.0),
                x2: width_em - 1.0 * cx,
                y2: ly(82.0),
                x: width_em - 3.0 * cx,
                y: ly(82.0),
            },
            PathCommand::CubicTo {
                x1: width_em - 3.0 * cx,
                y1: ly(82.0),
                x2: width_em - 15.1 * cx,
                y2: ly(80.8),
                x: width_em - 21.0 * cx,
                y: ly(82.0),
            },
            PathCommand::CubicTo {
                x1: width_em - 168.3 * cx,
                y1: ly(112.6),
                x2: width_em - 64.0 * cx,
                y2: ly(262.0),
                x: width_em - corner_em,
                y: ly(262.0),
            },
            PathCommand::Close,
        ]
    }
}

fn scale_cmd_xy(cmd: &PathCommand, sx: f64, sy: f64) -> PathCommand {
    match *cmd {
        PathCommand::MoveTo { x, y } => PathCommand::MoveTo {
            x: x * sx,
            y: y * sy,
        },
        PathCommand::LineTo { x, y } => PathCommand::LineTo {
            x: x * sx,
            y: y * sy,
        },
        PathCommand::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => PathCommand::CubicTo {
            x1: x1 * sx,
            y1: y1 * sy,
            x2: x2 * sx,
            y2: y2 * sy,
            x: x * sx,
            y: y * sy,
        },
        PathCommand::QuadTo { x1, y1, x, y } => PathCommand::QuadTo {
            x1: x1 * sx,
            y1: y1 * sy,
            x: x * sx,
            y: y * sy,
        },
        PathCommand::Close => PathCommand::Close,
    }
}

// ---------------------------------------------------------------------------
// Minimal SVG path data parser
// ---------------------------------------------------------------------------

/// Cache for SVG path strings that are static for the lifetime of the process.
/// Parsing these on every accent/stretchy-arrow layout is wasteful because the
/// same KaTeX path constants are used repeatedly across formulas.
type StaticPathCache = RwLock<HashMap<&'static str, Arc<[PathCommand]>>>;

static STATIC_PATH_CACHE: LazyLock<StaticPathCache> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Parse a KaTeX-style SVG path `d` string into [`PathCommand`]s (for delimiters, accents).
pub(crate) fn parse_svg_path_data(d: &str) -> Vec<PathCommand> {
    parse_svg_path(d)
}

/// Return the parsed form of a static SVG path string, caching the result.
///
/// The returned slice is shared; callers must not mutate it. This keeps the
/// existing `parse_svg_path_data` API unchanged while avoiding repeated
/// tokenize/parse work for KaTeX's built-in path constants.
fn parsed_static_path(d: &'static str) -> Arc<[PathCommand]> {
    {
        let cache = STATIC_PATH_CACHE.read().unwrap();
        if let Some(parsed) = cache.get(d) {
            return Arc::clone(parsed);
        }
    }

    let parsed: Arc<[PathCommand]> = parse_svg_path(d).into();
    let mut cache = STATIC_PATH_CACHE.write().unwrap();
    // Another thread may have inserted the same path while we were parsing.
    cache
        .entry(d)
        .or_insert_with(|| Arc::clone(&parsed))
        .clone()
}

fn parse_svg_path(d: &str) -> Vec<PathCommand> {
    let tokens = tokenize_svg(d);
    let mut cmds = Vec::new();
    let mut i = 0;
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut subpath_start = (0.0_f64, 0.0_f64);
    let mut last_cmd = b'M';

    while i < tokens.len() {
        let cmd_byte = match &tokens[i] {
            Token::Cmd(c) => {
                i += 1;
                *c
            }
            Token::Num(_) => last_cmd,
        };

        match cmd_byte {
            b'M' => {
                let (x, y) = read2(&tokens, &mut i);
                cx = x;
                cy = y;
                subpath_start = (cx, cy);
                cmds.push(PathCommand::MoveTo { x, y });
                last_cmd = b'L';
            }
            b'm' => {
                let (dx, dy) = read2(&tokens, &mut i);
                cx += dx;
                cy += dy;
                subpath_start = (cx, cy);
                cmds.push(PathCommand::MoveTo { x: cx, y: cy });
                last_cmd = b'l';
            }
            b'L' => {
                let (x, y) = read2(&tokens, &mut i);
                cx = x;
                cy = y;
                cmds.push(PathCommand::LineTo { x, y });
                last_cmd = b'L';
            }
            b'l' => {
                let (dx, dy) = read2(&tokens, &mut i);
                cx += dx;
                cy += dy;
                cmds.push(PathCommand::LineTo { x: cx, y: cy });
                last_cmd = b'l';
            }
            b'H' => {
                let x = read1(&tokens, &mut i);
                cx = x;
                cmds.push(PathCommand::LineTo { x: cx, y: cy });
                last_cmd = b'H';
            }
            b'h' => {
                let dx = read1(&tokens, &mut i);
                cx += dx;
                cmds.push(PathCommand::LineTo { x: cx, y: cy });
                last_cmd = b'h';
            }
            b'V' => {
                let y = read1(&tokens, &mut i);
                cy = y;
                cmds.push(PathCommand::LineTo { x: cx, y: cy });
                last_cmd = b'V';
            }
            b'v' => {
                let dy = read1(&tokens, &mut i);
                cy += dy;
                cmds.push(PathCommand::LineTo { x: cx, y: cy });
                last_cmd = b'v';
            }
            b'C' => {
                let (x1, y1) = read2(&tokens, &mut i);
                let (x2, y2) = read2(&tokens, &mut i);
                let (x, y) = read2(&tokens, &mut i);
                cx = x;
                cy = y;
                cmds.push(PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
                last_cmd = b'C';
            }
            b'c' => {
                let (dx1, dy1) = read2(&tokens, &mut i);
                let (dx2, dy2) = read2(&tokens, &mut i);
                let (dx, dy) = read2(&tokens, &mut i);
                let x1 = cx + dx1;
                let y1 = cy + dy1;
                let x2 = cx + dx2;
                let y2 = cy + dy2;
                cx += dx;
                cy += dy;
                cmds.push(PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x: cx,
                    y: cy,
                });
                last_cmd = b'c';
            }
            b'S' => {
                let (x2, y2) = read2(&tokens, &mut i);
                let (x, y) = read2(&tokens, &mut i);
                let x1 = cx;
                let y1 = cy;
                cx = x;
                cy = y;
                cmds.push(PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                });
                last_cmd = b'S';
            }
            b's' => {
                let (dx2, dy2) = read2(&tokens, &mut i);
                let (dx, dy) = read2(&tokens, &mut i);
                let x1 = cx;
                let y1 = cy;
                let x2 = cx + dx2;
                let y2 = cy + dy2;
                cx += dx;
                cy += dy;
                cmds.push(PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x: cx,
                    y: cy,
                });
                last_cmd = b's';
            }
            b'Q' => {
                let (x1, y1) = read2(&tokens, &mut i);
                let (x, y) = read2(&tokens, &mut i);
                cx = x;
                cy = y;
                cmds.push(PathCommand::QuadTo { x1, y1, x, y });
                last_cmd = b'Q';
            }
            b'q' => {
                let (dx1, dy1) = read2(&tokens, &mut i);
                let (dx, dy) = read2(&tokens, &mut i);
                let x1 = cx + dx1;
                let y1 = cy + dy1;
                cx += dx;
                cy += dy;
                cmds.push(PathCommand::QuadTo {
                    x1,
                    y1,
                    x: cx,
                    y: cy,
                });
                last_cmd = b'q';
            }
            b'Z' | b'z' => {
                cmds.push(PathCommand::Close);
                // Per SVG spec: after closepath, current point returns to subpath start.
                cx = subpath_start.0;
                cy = subpath_start.1;
                last_cmd = b'M';
            }
            _ => {
                i += 1;
            }
        }
    }
    cmds
}

#[derive(Debug)]
enum Token {
    Cmd(u8),
    Num(f64),
}

fn tokenize_svg(d: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_alphabetic() {
            tokens.push(Token::Cmd(b));
            i += 1;
        } else if b == b'-' || b == b'.' || b.is_ascii_digit() {
            let start = i;
            if b == b'-' {
                i += 1;
            }
            let mut has_dot = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !has_dot)) {
                if bytes[i] == b'.' {
                    has_dot = true;
                }
                i += 1;
            }
            if let Ok(n) = std::str::from_utf8(&bytes[start..i])
                .unwrap_or("0")
                .parse::<f64>()
            {
                tokens.push(Token::Num(n));
            }
        } else {
            i += 1;
        }
    }
    tokens
}

fn read1(tokens: &[Token], i: &mut usize) -> f64 {
    if *i < tokens.len() {
        if let Token::Num(n) = tokens[*i] {
            *i += 1;
            return n;
        }
    }
    0.0
}

fn read2(tokens: &[Token], i: &mut usize) -> (f64, f64) {
    let a = read1(tokens, i);
    let b = read1(tokens, i);
    (a, b)
}

// ---------------------------------------------------------------------------
// KaTeX raw SVG path data (from svgGeometry.js)
// ---------------------------------------------------------------------------

// twoheadleftarrow from glyph U+219E in font KaTeX AMS Regular (viewBox 400000×334)
const TWOHEADLEFTARROW: &str = "M0 167c68 40 115.7 95.7 143 167h22c15.3 0 23-.3 23-1 0-1.3-5.3-13.7-16-37-18-35.3-41.3-69-70-101l-7-8h125l9 7c50.7 39.3 85 86 103 140h46c0-4.7-6.3-18.7-19-42-18-35.3-40-67.3-66-96l-9-9h399716v-40H284l9-9c26-28.7 48-60.7 66-96 12.7-23.333 19-37.333 19-42h-46c-18 54-52.3 100.7-103 140l-9 7H95l7-8c28.7-32 52-65.7 70-101 10.7-23.333 16-35.7 16-37 0-.7-7.7-1-23-1h-22C115.7 71.3 68 127 0 167z";

// rightarrow from glyph U+2192 in font KaTeX Main (viewBox 400000×534, scale 1000:1)
const RIGHTARROW: &str = "M0 241v40h399891c-47.3 35.3-84 78-110 128 -16.7 32-27.7 63.7-33 95 0 1.3-.2 2.7-.5 4-.3 1.3-.5 2.3-.5 3 0 7.3 6.7 11 20 11 8 0 13.2-.8 15.5-2.5 2.3-1.7 4.2-5.5 5.5-11.5 2-13.3 5.7-27 11-41 14.7-44.7 39-84.5 73-119.5s73.7-60.2 119-75.5c6-2 9-5.7 9-11s-3-9-9-11c-45.3-15.3-85 -40.5-119-75.5s-58.3-74.8-73-119.5c-4.7-14-8.3-27.3-11-40-1.3-6.7-3.2-10.8-5.5 -12.5-2.3-1.7-7.5-2.5-15.5-2.5-14 0-21 3.7-21 11 0 2 2 10.3 6 25 20.7 83.3 67 151.7 139 205zm0 0v40h399900v-40z";

// leftarrow from glyph U+2190 in font KaTeX Main (viewBox 400000×534)
const LEFTARROW: &str = "M400000 241H110l3-3c68.7-52.7 113.7-120 135-202 4-14.7 6-23 6-25 0-7.3-7-11-21-11-8 0-13.2.8-15.5 2.5-2.3 1.7-4.2 5.8 -5.5 12.5-1.3 4.7-2.7 10.3-4 17-12 48.7-34.8 92-68.5 130S65.3 228.3 18 247 c-10 4-16 7.7-18 11 0 8.7 6 14.3 18 17 47.3 18.7 87.8 47 121.5 85S196 441.3 208 490c.7 2 1.3 5 2 9s1.2 6.7 1.5 8c.3 1.3 1 3.3 2 6s2.2 4.5 3.5 5.5c1.3 1 3.3 1.8 6 2.5s6 1 10 1c14 0 21-3.7 21-11 0-2-2-10.3-6-25-20-79.3-65-146.7-135-202 l-3-3h399890zM100 241v40h399900v-40z";

// leftmapsto: vertical bar at left (x=0..40) + shaft, for \xmapsto (viewBox 400000×534)
const LEFTMAPSTO: &str = "M40 281 V448H0V74H40V241H400000v40z";

// twoheadrightarrow from glyph U+21A0 in font KaTeX AMS Regular (viewBox 400000×334)
const TWOHEADRIGHTARROW: &str = "M400000 167c-68-40-115.7-95.7-143-167h-22c-15.3 0-23 .3-23 1 0 1.3 5.3 13.7 16 37 18 35.3 41.3 69 70 101l7 8h-125l-9-7c-50.7-39.3-85-86-103-140h-46c0 4.7 6.3 18.7 19 42 18 35.3 40 67.3 66 96l9 9H0v40h399716l-9 9c-26 28.7-48 60.7-66 96-12.7 23.333-19 37.333-19 42h46c18-54 52.3-100.7 103-140l9-7h125l-7 8c-28.7 32-52 65.7-70 101-10.7 23.333-16 35.7-16 37 0 .7 7.7 1 23 1h22c27.3-71.3 75-127 143-167z";

const WIDEHAT: [&str; 4] = [
    // widehat1
    "M529 0h5l519 115c5 1 9 5 9 10 0 1-1 2-1 3l-4 22\
c-1 5-5 9-11 9h-2L532 67 19 159h-2c-5 0-9-4-11-9l-5-22c-1-6 2-12 8-13z",
    // widehat2
    "M1181 0h2l1171 176c6 0 10 5 10 11l-2 23c-1 6-5 10\
-11 10h-1L1182 67 15 220h-1c-6 0-10-4-11-10l-2-23c-1-6 4-11 10-11z",
    // widehat3
    "M1181 0h2l1171 236c6 0 10 5 10 11l-2 23c-1 6-5 10\
-11 10h-1L1182 67 15 280h-1c-6 0-10-4-11-10l-2-23c-1-6 4-11 10-11z",
    // widehat4
    "M1181 0h2l1171 296c6 0 10 5 10 11l-2 23c-1 6-5 10\
-11 10h-1L1182 67 15 340h-1c-6 0-10-4-11-10l-2-23c-1-6 4-11 10-11z",
];

const WIDECHECK: [&str; 4] = [
    "M529,159h5l519,-115c5,-1,9,-5,9,-10c0,-1,-1,-2,-1,-3l-4,-22c-1,\
-5,-5,-9,-11,-9h-2l-512,92l-513,-92h-2c-5,0,-9,4,-11,9l-5,22c-1,6,2,12,8,13z",
    "M1181,220h2l1171,-176c6,0,10,-5,10,-11l-2,-23c-1,-6,-5,-10,\
-11,-10h-1l-1168,153l-1167,-153h-1c-6,0,-10,4,-11,10l-2,23c-1,6,4,11,10,11z",
    "M1181,280h2l1171,-236c6,0,10,-5,10,-11l-2,-23c-1,-6,-5,-10,\
-11,-10h-1l-1168,213l-1167,-213h-1c-6,0,-10,4,-11,10l-2,23c-1,6,4,11,10,11z",
    "M1181,340h2l1171,-296c6,0,10,-5,10,-11l-2,-23c-1,-6,-5,-10,\
-11,-10h-1l-1168,273l-1167,-273h-1c-6,0,-10,4,-11,10l-2,23c-1,6,4,11,10,11z",
];

const TILDE: [&str; 4] = [
    "M200 55.538c-77 0-168 73.953-177 73.953-3 0-7
-2.175-9-5.437L2 97c-1-2-2-4-2-6 0-4 2-7 5-9l20-12C116 12 171 0 207 0c86 0
 114 68 191 68 78 0 168-68 177-68 4 0 7 2 9 5l12 19c1 2.175 2 4.35 2 6.525 0
 4.35-2 7.613-5 9.788l-19 13.05c-92 63.077-116.937 75.308-183 76.128
-68.267.847-113-73.952-191-73.952z",
    "M344 55.266c-142 0-300.638 81.316-311.5 86.418
-8.01 3.762-22.5 10.91-23.5 5.562L1 120c-1-2-1-3-1-4 0-5 3-9 8-10l18.4-9C160.9
 31.9 283 0 358 0c148 0 188 122 331 122s314-97 326-97c4 0 8 2 10 7l7 21.114
c1 2.14 1 3.21 1 4.28 0 5.347-3 9.626-7 10.696l-22.3 12.622C852.6 158.372 751
 181.476 676 181.476c-149 0-189-126.21-332-126.21z",
    "M786 59C457 59 32 175.242 13 175.242c-6 0-10-3.457
-11-10.37L.15 138c-1-7 3-12 10-13l19.2-6.4C378.4 40.7 634.3 0 804.3 0c337 0
 411.8 157 746.8 157 328 0 754-112 773-112 5 0 10 3 11 9l1 14.075c1 8.066-.697
 16.595-6.697 17.492l-21.052 7.31c-367.9 98.146-609.15 122.696-778.15 122.696
 -338 0-409-156.573-744-156.573z",
    "M786 58C457 58 32 177.487 13 177.487c-6 0-10-3.345
-11-10.035L.15 143c-1-7 3-12 10-13l22-6.7C381.2 35 637.15 0 807.15 0c337 0 409
 177 744 177 328 0 754-127 773-127 5 0 10 3 11 9l1 14.794c1 7.805-3 13.38-9
 14.495l-20.7 5.574c-366.85 99.79-607.3 139.372-776.3 139.372-338 0-409
 -175.236-744-175.236z",
];

// doubleleftarrow from glyph U+21D0 in font KaTeX Main (viewBox 400000×560)
const DOUBLELEFTARROW: &str = "M262 157 l10-10c34-36 62.7-77 86-123 3.3-8 5-13.3 5-16 0-5.3-6.7-8-20-8-7.3 0-12.2.5-14.5 1.5-2.3 1-4.8 4.5-7.5 10.5-49.3 97.3-121.7 169.3-217 216-28 14-57.3 25-88 33-6.7 2-11 3.8-13 5.5-2 1.7-3 4.2-3 7.5s1 5.8 3 7.5 c2 1.7 6.3 3.5 13 5.5 68 17.3 128.2 47.8 180.5 91.5 52.3 43.7 93.8 96.2 124.5 157.5 9.3 8 15.3 12.3 18 13h6c12-.7 18-4 18-10 0-2-1.7-7-5-15-23.3-46-52-87-86-123l-10-10h399738v-40H218c328 0 0 0 0 0l-10-8c-26.7-20-65.7-43-117-69 2.7-2 6-3.7 10-5 36.7-16 72.3-37.3 107-64l10-8h399782v-40z m8 0v40h399730v-40zm0 194v40h399730v-40z";

// doublerightarrow from glyph U+21D2 in font KaTeX Main (viewBox 400000×560)
const DOUBLERIGHTARROW: &str = "M399738 392l -10 10c-34 36-62.7 77-86 123-3.3 8-5 13.3-5 16 0 5.3 6.7 8 20 8 7.3 0 12.2-.5 14.5-1.5 2.3-1 4.8-4.5 7.5-10.5 49.3-97.3 121.7-169.3 217-216 28-14 57.3-25 88-33 6.7-2 11-3.8 13-5.5 2-1.7 3-4.2 3-7.5s-1-5.8-3-7.5c-2-1.7-6.3-3.5-13-5.5-68-17.3-128.2-47.8-180.5-91.5-52.3-43.7-93.8-96.2-124.5-157.5-9.3-8-15.3-12.3-18-13h-6c-12 .7-18 4-18 10 0 2 1.7 7 5 15 23.3 46 52 87 86 123l10 10H0v40h399782 c-328 0 0 0 0 0l10 8c26.7 20 65.7 43 117 69-2.7 2-6 3.7-10 5-36.7 16-72.3 37.3-107 64l-10 8H0v40zM0 157v40h399730v-40zm0 194v40h399730v-40z";

// leftharpoon from glyph U+21BD in font KaTeX Main (viewBox 400000×522)
const LEFTHARPOON: &str = "M0 267c.7 5.3 3 10 7 14h399993v-40H93c3.3-3.3 10.2-9.5 20.5-18.5s17.8-15.8 22.5-20.5c50.7-52 88-110.3 112-175 4-11.3 5-18.3 3-21-1.3-4-7.3-6-18-6-8 0-13 .7-15 2s-4.7 6.7-8 16c-42 98.7-107.3 174.7-196 228-6.7 4.7-10.7 8-12 10-1.3 2-2 5.7-2 11zm100-26v40h399900v-40z";

// leftharpoonplus (viewBox 400000×716)
const LEFTHARPOONPLUS: &str = "M0 267c.7 5.3 3 10 7 14h399993v-40H93c3.3-3.3 10.2-9.5 20.5-18.5s17.8-15.8 22.5-20.5c50.7-52 88-110.3 112-175 4-11.3 5-18.3 3-21-1.3-4-7.3-6-18-6-8 0-13 .7-15 2s-4.7 6.7-8 16c-42 98.7-107.3 174.7-196 228-6.7 4.7-10.7 8-12 10-1.3 2-2 5.7-2 11zm100-26v40h399900v-40zM0 435v40h400000v-40z m0 0v40h400000v-40z";

// leftharpoondown (viewBox 400000×522)
const LEFTHARPOONDOWN: &str = "M7 241c-4 4-6.333 8.667-7 14 0 5.333.667 9 2 11s5.333 5.333 12 10c90.667 54 156 130 196 228 3.333 10.667 6.333 16.333 9 17 2 .667 5 1 9 1h5c10.667 0 16.667-2 18-6 2-2.667 1-9.667-3-21-32-87.333-82.667-157.667-152-211l-3-3h399907v-40zM93 281 H400000 v-40L7 241z";

// leftharpoondownplus (viewBox 400000×716)
const LEFTHARPOONDOWNPLUS: &str = "M7 435c-4 4-6.3 8.7-7 14 0 5.3.7 9 2 11s5.3 5.3 12 10c90.7 54 156 130 196 228 3.3 10.7 6.3 16.3 9 17 2 .7 5 1 9 1h5c10.7 0 16.7-2 18-6 2-2.7 1-9.7-3-21-32-87.3-82.7-157.7-152-211l-3-3h399907v-40H7zm93 0 v40h399900v-40zM0 241v40h399900v-40zm0 0v40h399900v-40z";

// rightharpoon from glyph U+21C0 in font KaTeX Main (viewBox 400000×522)
const RIGHTHARPOON: &str = "M0 241v40h399993c4.7-4.7 7-9.3 7-14 0-9.3-3.7-15.3-11-18-92.7-56.7-159-133.7-199-231-3.3-9.3-6-14.7-8-16-2-1.3-7-2-15-2-10.7 0-16.7 2-18 6-2 2.7-1 9.7 3 21 15.3 42 36.7 81.8 64 119.5 27.3 37.7 58 69.2 92 94.5zm0 0v40h399900v-40z";

// rightharpoonplus (viewBox 400000×716)
const RIGHTHARPOONPLUS: &str = "M0 241v40h399993c4.7-4.7 7-9.3 7-14 0-9.3-3.7-15.3-11-18-92.7-56.7-159-133.7-199-231-3.3-9.3-6-14.7-8-16-2-1.3-7-2-15-2-10.7 0-16.7 2-18 6-2 2.7-1 9.7 3 21 15.3 42 36.7 81.8 64 119.5 27.3 37.7 58 69.2 92 94.5z m0 0v40h399900v-40z m100 194v40h399900v-40zm0 0v40h399900v-40z";

// rightharpoondown (viewBox 400000×522)
const RIGHTHARPOONDOWN: &str = "M399747 511c0 7.3 6.7 11 20 11 8 0 13-.8 15-2.5s4.7-6.8 8-15.5c40-94 99.3-166.3 178-217 13.3-8 20.3-12.3 21-13 5.3-3.3 8.5-5.8 9.5-7.5 1-1.7 1.5-5.2 1.5-10.5s-2.3-10.3-7-15H0v40h399908c-34 25.3-64.7 57-92 95-27.3 38-48.7 77.7-64 119-3.3 8.7-5 14-5 16zM0 241v40h399900v-40z";

// rightharpoondownplus (viewBox 400000×716)
const RIGHTHARPOONDOWNPLUS: &str = "M399747 705c0 7.3 6.7 11 20 11 8 0 13-.8 15-2.5s4.7-6.8 8-15.5c40-94 99.3-166.3 178-217 13.3-8 20.3-12.3 21-13 5.3-3.3 8.5-5.8 9.5-7.5 1-1.7 1.5-5.2 1.5-10.5s-2.3-10.3-7-15H0v40h399908c-34 25.3-64.7 57-92 95-27.3 38-48.7 77.7-64 119-3.3 8.7-5 14-5 16zM0 435v40h399900v-40z m0-194v40h400000v-40zm0 0v40h400000v-40z";

// mhchem \\xrightequilibrium / \\xleftequilibrium (KaTeX katexImagesData, viewBox 400000×716)
const BARABOVESHORTLEFTHARPOON: &str = "M507,435c-4,4,-6.3,8.7,-7,14c0,5.3,0.7,9,2,11c1.3,2,5.3,5.3,12,10c90.7,54,156,130,196,228c3.3,10.7,6.3,16.3,9,17c2,0.7,5,1,9,1c0,0,5,0,5,0c10.7,0,16.7,-2,18,-6c2,-2.7,1,-9.7,-3,-21c-32,-87.3,-82.7,-157.7,-152,-211c0,0,-3,-3,-3,-3l399351,0l0,-40c-398570,0,-399437,0,-399437,0z M593 435 v40 H399500 v-40zM0 281 v-40 H399908 v40z M0 281 v-40 H399908 v40z";
const RIGHTHARPOONABOVESHORTBAR: &str = "M0,241 l0,40c399126,0,399993,0,399993,0c4.7,-4.7,7,-9.3,7,-14c0,-9.3,-3.7,-15.3,-11,-18c-92.7,-56.7,-159,-133.7,-199,-231c-3.3,-9.3,-6,-14.7,-8,-16c-2,-1.3,-7,-2,-15,-2c-10.7,0,-16.7,2,-18,6c-2,2.7,-1,9.7,3,21c15.3,42,36.7,81.8,64,119.5c27.3,37.7,58,69.2,92,94.5zM0 241 v40 H399908 v-40z M0 475 v-40 H399500 v40z M0 475 v-40 H399500 v40z";
const SHORTBARABOVELEFTHARPOON: &str = "M7,435c-4,4,-6.3,8.7,-7,14c0,5.3,0.7,9,2,11c1.3,2,5.3,5.3,12,10c90.7,54,156,130,196,228c3.3,10.7,6.3,16.3,9,17c2,0.7,5,1,9,1c0,0,5,0,5,0c10.7,0,16.7,-2,18,-6c2,-2.7,1,-9.7,-3,-21c-32,-87.3,-82.7,-157.7,-152,-211c0,0,-3,-3,-3,-3l399907,0l0,-40c-399126,0,-399993,0,-399993,0zM93 435 v40 H400000 v-40z M500 241 v40 H400000 v-40z M500 241 v40 H400000 v-40z";
const SHORTRIGHTHARPOONABOVEBAR: &str = "M53,241l0,40c398570,0,399437,0,399437,0c4.7,-4.7,7,-9.3,7,-14c0,-9.3,-3.7,-15.3,-11,-18c-92.7,-56.7,-159,-133.7,-199,-231c-3.3,-9.3,-6,-14.7,-8,-16c-2,-1.3,-7,-2,-15,-2c-10.7,0,-16.7,2,-18,6c-2,2.7,-1,9.7,3,21c15.3,42,36.7,81.8,64,119.5c27.3,37.7,58,69.2,92,94.5zM500 241 v40 H399408 v-40z M500 435 v40 H400000 v-40z";

// lefthook from glyph U+21A9 in font KaTeX Main (viewBox 400000×522)
const LEFTHOOK: &str = "M400000 281 H103s-33-11.2-61-33.5S0 197.3 0 164s14.2-61.2 42.5-83.5C70.8 58.2 104 47 142 47 c16.7 0 25 6.7 25 20 0 12-8.7 18.7-26 20-40 3.3-68.7 15.7-86 37-10 12-15 25.3-15 40 0 22.7 9.8 40.7 29.5 54 19.7 13.3 43.5 21 71.5 23h399859zM103 281v-40h399897v40z";

// righthook (viewBox 400000×522)
const RIGHTHOOK: &str = "M399859 241c-764 0 0 0 0 0 40-3.3 68.7-15.7 86-37 10-12 15-25.3 15-40 0-22.7-9.8-40.7-29.5-54-19.7-13.3-43.5-21-71.5-23-17.3-1.3-26-8-26-20 0-13.3 8.7-20 26-20 38 0 71 11.2 99 33.5 0 0 7 5.6 21 16.7 14 11.2 21 33.5 21 66.8s-14 61.2-42 83.5c-28 22.3-61 33.5-99 33.5L0 241z M0 281v-40h399859v40z";

// leftbrace from glyphs U+23A9/23A8/23A7 in font KaTeX_Size4-Regular (viewBox 400000×548)
const LEFTBRACE: &str = "M6 548l-6-6v-35l6-11c56-104 135.3-181.3 238-232 57.3-28.7 117-45 179-50h399577v120H403c-43.3 7-81 15-113 26-100.7 33-179.7 91-237 174-2.7 5-6 9-10 13-.7 1-7.3 1-20 1H6z";

// midbrace (viewBox 400000×548)
const MIDBRACE: &str = "M200428 334 c-100.7-8.3-195.3-44-280-108-55.3-42-101.7-93-139-153l-9-14c-2.7 4-5.7 8.7-9 14-53.3 86.7-123.7 153-211 199-66.7 36-137.3 56.3-212 62H0V214h199568c178.3-11.7 311.7-78.3 403-201 6-8 9.7-12 11-12 .7-.7 6.7-1 18-1s17.3.3 18 1c1.3 0 5 4 11 12 44.7 59.3 101.3 106.3 170 141s145.3 54.3 229 60h199572v120z";

// rightbrace (viewBox 400000×548)
const RIGHTBRACE: &str = "M400000 542l -6 6h-17c-12.7 0-19.3-.3-20-1-4-4-7.3-8.3-10-13-35.3-51.3-80.8-93.8-136.5-127.5s-117.2-55.8-184.5-66.5c-.7 0-2-.3-4-1-18.7-2.7-76-4.3-172-5H0V214h399571l6 1 c124.7 8 235 61.7 331 161 31.3 33.3 59.7 72.7 85 118l7 13v35z";

// leftbraceunder (viewBox 400000×548)
const LEFTBRACEUNDER: &str = "M0 6l6-6h17c12.688 0 19.313.3 20 1 4 4 7.313 8.3 10 13 35.313 51.3 80.813 93.8 136.5 127.5 55.688 33.7 117.188 55.8 184.5 66.5.688 0 2 .3 4 1 18.688 2.7 76 4.3 172 5h399450v120H429l-6-1c-124.688-8-235-61.7-331-161C60.687 138.7 32.312 99.3 7 54L0 41V6z";

// midbraceunder (viewBox 400000×548)
const MIDBRACEUNDER: &str = "M199572 214 c100.7 8.3 195.3 44 280 108 55.3 42 101.7 93 139 153l9 14c2.7-4 5.7-8.7 9-14 53.3-86.7 123.7-153 211-199 66.7-36 137.3-56.3 212-62h199568v120H200432c-178.3 11.7-311.7 78.3-403 201-6 8-9.7 12-11 12-.7.7-6.7 1-18 1s-17.3-.3-18-1c-1.3 0-5-4-11-12-44.7-59.3-101.3-106.3-170-141s-145.3-54.3-229-60H0V214z";

// rightbraceunder (viewBox 400000×548)
const RIGHTBRACEUNDER: &str = "M399994 0l6 6v35l-6 11c-56 104-135.3 181.3-238 232-57.3 28.7-117 45-179 50H-300V214h399897c43.3-7 81-15 113-26 100.7-33 179.7-91 237-174 2.7-5 6-9 10-13 .7-1 7.3-1 20-1h17z";

// Fixed KaTeX brace outlines, normalized from the six SVG paths above.
// Coordinates are in em relative to the left edge, center, or right edge.
// Keeping these paths static avoids runtime SVG parsing and a shared path cache.
pub(crate) const BRACE_MIN_WIDTH: f64 = 1.6;
pub(crate) const BRACE_HEIGHT: f64 = 0.548;

type BracePoint = [f64; 2];

enum BraceEdge {
    Line(BracePoint),
    Cubic([BracePoint; 3]),
}

struct BraceOutline {
    start: BracePoint,
    edges: &'static [BraceEdge],
}

const LEFTBRACE_OUTLINE: BraceOutline = BraceOutline {
    start: [0.006, 0.548],
    edges: &[
        BraceEdge::Line([0.0, 0.542]),
        BraceEdge::Line([0.0, 0.507]),
        BraceEdge::Line([0.006, 0.496]),
        BraceEdge::Cubic([[0.062, 0.392], [0.1413, 0.3147], [0.244, 0.264]]),
        BraceEdge::Cubic([[0.3013, 0.2353], [0.361, 0.219], [0.423, 0.214]]),
        BraceEdge::Line([400.0, 0.214]),
        BraceEdge::Line([400.0, 0.334]),
        BraceEdge::Line([0.403, 0.334]),
        BraceEdge::Cubic([[0.3597, 0.341], [0.322, 0.349], [0.29, 0.36]]),
        BraceEdge::Cubic([[0.1893, 0.393], [0.1103, 0.451], [0.053, 0.534]]),
        BraceEdge::Cubic([[0.0503, 0.539], [0.047, 0.543], [0.043, 0.547]]),
        BraceEdge::Cubic([[0.0423, 0.548], [0.0357, 0.548], [0.023, 0.548]]),
        BraceEdge::Line([0.006, 0.548]),
    ],
};

const MIDBRACE_OUTLINE: BraceOutline = BraceOutline {
    start: [0.428, 0.334],
    edges: &[
        BraceEdge::Cubic([[0.3273, 0.3257], [0.2327, 0.29], [0.148, 0.226]]),
        BraceEdge::Cubic([[0.0927, 0.184], [0.0463, 0.133], [0.009, 0.073]]),
        BraceEdge::Line([0.0, 0.059]),
        BraceEdge::Cubic([[-0.0027, 0.063], [-0.0057, 0.0677], [-0.009, 0.073]]),
        BraceEdge::Cubic([[-0.0623, 0.1597], [-0.1327, 0.226], [-0.22, 0.272]]),
        BraceEdge::Cubic([[-0.2867, 0.308], [-0.3573, 0.3283], [-0.432, 0.334]]),
        BraceEdge::Line([-200.0, 0.334]),
        BraceEdge::Line([-200.0, 0.214]),
        BraceEdge::Line([-0.432, 0.214]),
        BraceEdge::Cubic([[-0.2537, 0.2023], [-0.1203, 0.1357], [-0.029, 0.013]]),
        BraceEdge::Cubic([[-0.023, 0.005], [-0.0193, 0.001], [-0.018, 0.001]]),
        BraceEdge::Cubic([[-0.0173, 0.0003], [-0.0113, 0.0], [0.0, 0.0]]),
        BraceEdge::Cubic([[0.0113, 0.0], [0.0173, 0.0003], [0.018, 0.001]]),
        BraceEdge::Cubic([[0.0193, 0.001], [0.023, 0.005], [0.029, 0.013]]),
        BraceEdge::Cubic([[0.0737, 0.0723], [0.1303, 0.1193], [0.199, 0.154]]),
        BraceEdge::Cubic([[0.2677, 0.1887], [0.3443, 0.2083], [0.428, 0.214]]),
        BraceEdge::Line([200.0, 0.214]),
        BraceEdge::Line([200.0, 0.334]),
        BraceEdge::Line([0.428, 0.334]),
    ],
};

const RIGHTBRACE_OUTLINE: BraceOutline = BraceOutline {
    start: [0.0, 0.542],
    edges: &[
        BraceEdge::Line([-0.006, 0.548]),
        BraceEdge::Line([-0.023, 0.548]),
        BraceEdge::Cubic([[-0.0357, 0.548], [-0.0423, 0.5477], [-0.043, 0.547]]),
        BraceEdge::Cubic([[-0.047, 0.543], [-0.0503, 0.5387], [-0.053, 0.534]]),
        BraceEdge::Cubic([[-0.0883, 0.4827], [-0.1338, 0.4402], [-0.1895, 0.4065]]),
        BraceEdge::Cubic([[-0.2452, 0.3728], [-0.3067, 0.3507], [-0.374, 0.34]]),
        BraceEdge::Cubic([[-0.3747, 0.34], [-0.376, 0.3397], [-0.378, 0.339]]),
        BraceEdge::Cubic([[-0.3967, 0.3363], [-0.454, 0.3347], [-0.55, 0.334]]),
        BraceEdge::Line([-400.0, 0.334]),
        BraceEdge::Line([-400.0, 0.214]),
        BraceEdge::Line([-0.429, 0.214]),
        BraceEdge::Line([-0.423, 0.215]),
        BraceEdge::Cubic([[-0.2983, 0.223], [-0.188, 0.2767], [-0.092, 0.376]]),
        BraceEdge::Cubic([[-0.0607, 0.4093], [-0.0323, 0.4487], [-0.007, 0.494]]),
        BraceEdge::Line([0.0, 0.507]),
        BraceEdge::Line([0.0, 0.542]),
    ],
};

const LEFTBRACEUNDER_OUTLINE: BraceOutline = BraceOutline {
    start: [0.0, 0.006],
    edges: &[
        BraceEdge::Line([0.006, 0.0]),
        BraceEdge::Line([0.023, 0.0]),
        BraceEdge::Cubic([[0.035688, 0.0], [0.042313, 0.0003], [0.043, 0.001]]),
        BraceEdge::Cubic([[0.047, 0.005], [0.050313, 0.0093], [0.053, 0.014]]),
        BraceEdge::Cubic([[0.088313, 0.0653], [0.133813, 0.1078], [0.1895, 0.1415]]),
        BraceEdge::Cubic([[0.245188, 0.1752], [0.306688, 0.1973], [0.374, 0.208]]),
        BraceEdge::Cubic([[0.374688, 0.208], [0.376, 0.2083], [0.378, 0.209]]),
        BraceEdge::Cubic([[0.396688, 0.2117], [0.454, 0.2133], [0.55, 0.214]]),
        BraceEdge::Line([400.0, 0.214]),
        BraceEdge::Line([400.0, 0.334]),
        BraceEdge::Line([0.429, 0.334]),
        BraceEdge::Line([0.423, 0.333]),
        BraceEdge::Cubic([[0.298312, 0.325], [0.188, 0.2713], [0.092, 0.172]]),
        BraceEdge::Cubic([[0.060687, 0.1387], [0.032312, 0.0993], [0.007, 0.054]]),
        BraceEdge::Line([0.0, 0.041]),
        BraceEdge::Line([0.0, 0.006]),
    ],
};

const MIDBRACEUNDER_OUTLINE: BraceOutline = BraceOutline {
    start: [-0.428, 0.214],
    edges: &[
        BraceEdge::Cubic([[-0.3273, 0.2223], [-0.2327, 0.258], [-0.148, 0.322]]),
        BraceEdge::Cubic([[-0.0927, 0.364], [-0.0463, 0.415], [-0.009, 0.475]]),
        BraceEdge::Line([0.0, 0.489]),
        BraceEdge::Cubic([[0.0027, 0.485], [0.0057, 0.4803], [0.009, 0.475]]),
        BraceEdge::Cubic([[0.0623, 0.3883], [0.1327, 0.322], [0.22, 0.276]]),
        BraceEdge::Cubic([[0.2867, 0.24], [0.3573, 0.2197], [0.432, 0.214]]),
        BraceEdge::Line([200.0, 0.214]),
        BraceEdge::Line([200.0, 0.334]),
        BraceEdge::Line([0.432, 0.334]),
        BraceEdge::Cubic([[0.2537, 0.3457], [0.1203, 0.4123], [0.029, 0.535]]),
        BraceEdge::Cubic([[0.023, 0.543], [0.0193, 0.547], [0.018, 0.547]]),
        BraceEdge::Cubic([[0.0173, 0.5477], [0.0113, 0.548], [0.0, 0.548]]),
        BraceEdge::Cubic([[-0.0113, 0.548], [-0.0173, 0.5477], [-0.018, 0.547]]),
        BraceEdge::Cubic([[-0.0193, 0.547], [-0.023, 0.543], [-0.029, 0.535]]),
        BraceEdge::Cubic([[-0.0737, 0.4757], [-0.1303, 0.4287], [-0.199, 0.394]]),
        BraceEdge::Cubic([[-0.2677, 0.3593], [-0.3443, 0.3397], [-0.428, 0.334]]),
        BraceEdge::Line([-200.0, 0.334]),
        BraceEdge::Line([-200.0, 0.214]),
        BraceEdge::Line([-0.428, 0.214]),
    ],
};

const RIGHTBRACEUNDER_OUTLINE: BraceOutline = BraceOutline {
    start: [-0.006, 0.0],
    edges: &[
        BraceEdge::Line([0.0, 0.006]),
        BraceEdge::Line([0.0, 0.041]),
        BraceEdge::Line([-0.006, 0.052]),
        BraceEdge::Cubic([[-0.062, 0.156], [-0.1413, 0.2333], [-0.244, 0.284]]),
        BraceEdge::Cubic([[-0.3013, 0.3127], [-0.361, 0.329], [-0.423, 0.334]]),
        BraceEdge::Line([-400.3, 0.334]),
        BraceEdge::Line([-400.3, 0.214]),
        BraceEdge::Line([-0.403, 0.214]),
        BraceEdge::Cubic([[-0.3597, 0.207], [-0.322, 0.199], [-0.29, 0.188]]),
        BraceEdge::Cubic([[-0.1893, 0.155], [-0.1103, 0.097], [-0.053, 0.014]]),
        BraceEdge::Cubic([[-0.0503, 0.009], [-0.047, 0.005], [-0.043, 0.001]]),
        BraceEdge::Cubic([[-0.0423, 0.0], [-0.0357, 0.0], [-0.023, 0.0]]),
        BraceEdge::Line([-0.006, 0.0]),
    ],
};

#[derive(Clone, Copy)]
pub(crate) enum BracePiece {
    Left,
    Center,
    Right,
}

impl BracePiece {
    pub(crate) const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    fn outline(self, is_over: bool) -> &'static BraceOutline {
        match (self, is_over) {
            (Self::Left, true) => &LEFTBRACE_OUTLINE,
            (Self::Center, true) => &MIDBRACE_OUTLINE,
            (Self::Right, true) => &RIGHTBRACE_OUTLINE,
            (Self::Left, false) => &LEFTBRACEUNDER_OUTLINE,
            (Self::Center, false) => &MIDBRACEUNDER_OUTLINE,
            (Self::Right, false) => &RIGHTBRACEUNDER_OUTLINE,
        }
    }

    fn clip(self, width: f64) -> (f64, f64, f64) {
        // KaTeX CSS overlaps the 25.1%-width ends with the 50%-width center.
        match self {
            Self::Left => (0.0, 0.0, 0.251 * width),
            Self::Center => (width / 2.0, -0.25 * width, 0.25 * width),
            Self::Right => (width, -0.251 * width, 0.0),
        }
    }
}

pub(crate) fn bounded_brace_piece(
    piece: BracePiece,
    is_over: bool,
    width: f64,
    control: Option<&ExecutionControl<'_>>,
) -> Result<Vec<PathCommand>, ExecutionError> {
    let outline = piece.outline(is_over);
    let (origin, left, right) = piece.clip(width);
    let transform = |[x, y]: BracePoint| [x + origin, y];
    let mut commands = WorkVec::new(control, Phase::Layout, StorageFamily::Layout);
    let mut from = outline.start;
    let mut last = None;
    for edge in outline.edges {
        if let Some(control) = control {
            control.checkpoint(Phase::Layout)?;
        }
        let to = match *edge {
            BraceEdge::Line(to) | BraceEdge::Cubic([_, _, to]) => to,
        };
        let (start, command, end) = match *edge {
            BraceEdge::Line(_) => {
                if !brace_edge_visible(from[0], to[0], left, right) {
                    from = to;
                    continue;
                }
                let start = trim_brace_line(from, to, from[0].clamp(left, right));
                let end = trim_brace_line(from, to, to[0].clamp(left, right));
                let [x, y] = transform(end);
                (start, PathCommand::LineTo { x, y }, end)
            }
            BraceEdge::Cubic([a, b, _]) => {
                let Some([start, a, b, end]) = trim_brace_curve([from, a, b, to], left, right)
                else {
                    from = to;
                    continue;
                };
                let [x1, y1] = transform(a);
                let [x2, y2] = transform(b);
                let [x, y] = transform(end);
                (
                    start,
                    PathCommand::CubicTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    },
                    end,
                )
            }
        };
        let [x, y] = transform(start);
        match last {
            None => commands.push(PathCommand::MoveTo { x, y }),
            // Closing across a clipped shaft segment follows the vertical clip edge.
            Some(previous) if previous != start => commands.push(PathCommand::LineTo { x, y }),
            _ => {}
        }
        commands.push(command);
        last = Some(end);
        from = to;
    }
    commands.push(PathCommand::Close);
    if let Some(control) = control {
        control.checkpoint(Phase::Layout)?;
    }
    Ok(commands.into_vec())
}

fn brace_edge_visible(from_x: f64, to_x: f64, left: f64, right: f64) -> bool {
    from_x.min(to_x) <= right && from_x.max(to_x) >= left
}

fn trim_brace_line(from: BracePoint, to: BracePoint, x: f64) -> BracePoint {
    if x == from[0] {
        from
    } else if x == to[0] {
        to
    } else {
        let t = (x - from[0]) / (to[0] - from[0]);
        [x, from[1] + (to[1] - from[1]) * t]
    }
}

fn trim_brace_curve(mut curve: [BracePoint; 4], left: f64, right: f64) -> Option<[BracePoint; 4]> {
    if curve.iter().all(|p| p[0] >= left && p[0] <= right) {
        return Some(curve);
    }
    // For the fixed outlines, curves touching a clip boundary at the
    // minimum width or wider are monotone in x. A cut can only remove a prefix
    // or a suffix, so its remaining contour is still a single cubic.
    let start_x = curve[0][0];
    let end_x = curve[3][0];
    if !brace_edge_visible(start_x, end_x, left, right) {
        return None;
    }
    let start = start_x.clamp(left, right);
    let end = end_x.clamp(left, right);
    if start != start_x {
        curve = split_brace_curve_at_x(curve, start).1;
        curve[0][0] = start;
    }
    if end != end_x {
        curve = split_brace_curve_at_x(curve, end).0;
        curve[3][0] = end;
    }
    Some(curve)
}

fn split_brace_curve_at_x(curve: [BracePoint; 4], x: f64) -> ([BracePoint; 4], [BracePoint; 4]) {
    // A fixed number of bisections locates the intersection. De Casteljau
    // subdivision preserves the curved contour near the minimum width.
    let increasing = curve[0][0] < curve[3][0];
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..32 {
        let t = (lower + upper) / 2.0;
        let (prefix, _) = split_brace_curve(curve, t);
        if (prefix[3][0] < x) == increasing {
            lower = t;
        } else {
            upper = t;
        }
    }
    split_brace_curve(curve, (lower + upper) / 2.0)
}

fn split_brace_curve([a, b, c, d]: [BracePoint; 4], t: f64) -> ([BracePoint; 4], [BracePoint; 4]) {
    let mix = |[x, y]: BracePoint, [u, v]: BracePoint| [x + (u - x) * t, y + (v - y) * t];
    let ab = mix(a, b);
    let bc = mix(b, c);
    let cd = mix(c, d);
    let abc = mix(ab, bc);
    let bcd = mix(bc, cd);
    let middle = mix(abc, bcd);
    ([a, ab, abc, middle], [middle, bcd, cd, d])
}

// leftToFrom from glyph U+21C4 in font KaTeX AMS Regular (viewBox 400000×528)
const LEFTTOFROM: &str = "M0 147h400000v40H0zm0 214c68 40 115.7 95.7 143 167h22c15.3 0 23-.3 23-1 0-1.3-5.3-13.7-16-37-18-35.3-41.3-69-70-101l-7-8h399905v-40H95l7-8 c28.7-32 52-65.7 70-101 10.7-23.3 16-35.7 16-37 0-.7-7.7-1-23-1h-22C115.7 265.3 68 321 0 361zm0-174v-40h399900v40zm100 154v40h399900v-40z";

// rightToFrom (viewBox 400000×528)
const RIGHTTOFROM: &str = "M400000 167c-70.7-42-118-97.7-142-167h-23c-15.3 0-23 .3-23 1 0 1.3 5.3 13.7 16 37 18 35.3 41.3 69 70 101l7 8H0v40h399905l-7 8c-28.7 32-52 65.7-70 101-10.7 23.3-16 35.7-16 37 0 .7 7.7 1 23 1h23c24-69.3 71.3-125 142-167z M100 147v40h399900v-40zM0 341v40h399900v-40z";

// baraboveleftarrow from KaTeX mhchem (viewBox 400000×620)
const BARABOVELEFTARROW: &str = "M400000 620h-399890l3-3c68.7-52.7 113.7-120 135-202 c4-14.7 6-23 6-25c0-7.3-7-11-21-11c-8 0-13.2 0.8-15.5 2.5c-2.3 1.7-4.2 5.8-5.5 12.5c-1.3 4.7-2.7 10.3-4 17c-12 48.7-34.8 92-68.5 130 s-74.2 66.3-121.5 85c-10 4-16 7.7-18 11c0 8.7 6 14.3 18 17c47.3 18.7 87.8 47 121.5 85s56.5 81.3 68.5 130c0.7 2 1.3 5 2 9s1.2 6.7 1.5 8c0.3 1.3 1 3.3 2 6 s2.2 4.5 3.5 5.5c1.3 1 3.3 1.8 6 2.5s6 1 10 1c14 0 21-3.7 21-11 c0-2-2-10.3-6-25c-20-79.3-65-146.7-135-202l-3-3h399890z M100 620v40h399900v-40z M0 241v40h399900v-40zM0 241v40h399900v-40z";

// rightarrowabovebar from KaTeX mhchem (viewBox 400000×620)
const RIGHTARROWABOVEBAR: &str = "M0 241v40h399891c-47.3 35.3-84 78-110 128-16.7 32-27.7 63.7-33 95 0 1.3-.2 2.7-.5 4-.3 1.3-.5 2.3-.5 3 0 7.3 6.7 11 20 11 8 0 13.2-.8 15.5-2.5 2.3-1.7 4.2-5.5 5.5-11.5 2-13.3 5.7-27 11-41 14.7-44.7 39-84.5 73-119.5s73.7-60.2 119-75.5c6-2 9-5.7 9-11s-3-9-9-11c-45.3-15.3-85-40.5-119-75.5s-58.3-74.8-73-119.5c-4.7-14-8.3-27.3-11-40-1.3-6.7-3.2-10.8-5.5-12.5-2.3-1.7-7.5-2.5-15.5-2.5-14 0-21 3.7-21 11 0 2 2 10.3 6 25 20.7 83.3 67 151.7 139 205zm96 379h399894v40H0zm0 0h399904v40H0z";

// longequal (viewBox 400000×334)
const LONGEQUAL: &str =
    "M0 50 h400000 v40H0z m0 194h400000v40H0z M0 50 h400000 v40H0z m0 194h400000v40H0z";

// leftlinesegment (viewBox 400000×522)
const LEFTLINESEGMENT: &str =
    "M40 281 V428 H0 V94 H40 V241 H400000 v40z M40 281 V428 H0 V94 H40 V241 H400000 v40z";

// rightlinesegment (viewBox 400000×522)
const RIGHTLINESEGMENT: &str =
    "M399960 241 V94 h40 V428 h-40 V281 H0 v-40z M399960 241 V94 h40 V428 h-40 V281 H0 v-40z";

// leftgroup (for \overgroup, viewBox 400000×342)
const LEFTGROUP: &str = "M400000 80 H435C64 80 168.3 229.4 21 260c-5.9 1.2-18 0-18 0-2 0-3-1-3-3v-38C76 61 257 0 435 0h399565z";

// leftgroupunder (for \undergroup, viewBox 400000×342)
const LEFTGROUPUNDER: &str = "M400000 262 H435C64 262 168.3 112.6 21 82c-5.9-1.2-18 0-18 0-2 0-3 1-3 3v38c76 158 257 219 435 219h399565z";

// rightgroup (for \overgroup, viewBox 400000×342)
const RIGHTGROUP: &str = "M0 80h399565c371 0 266.7 149.4 414 180 5.9 1.2 18 0 18 0 2 0 3-1 3-3v-38c-76-158-257-219-435-219H0z";

// rightgroupunder (for \undergroup, viewBox 400000×342)
const RIGHTGROUPUNDER: &str = "M0 262h399565c371 0 266.7-149.4 414-180 5.9-1.2 18 0 18 0 2 0 3 1 3 3v38c-76 158-257 219-435 219H0z";

// Horizontal brackets (KaTeX `svgGeometry.ts`, viewBox 400000×440 / 410)
const LEFTBRACKETOVER: &str = "M0 440 h120 V150 H399995 v-120 H0z";
const RIGHTBRACKETOVER: &str = "M399995 440 h-120 V150 H0 v-120 H399995z";
const LEFTBRACKETUNDER: &str = "M0 0 h120 V290 H399995 v120 H0z";
const RIGHTBRACKETUNDER: &str = "M399995 0 h-120 V290 H0 v120 H400000z";

// vec from glyph U+20D7 in font KaTeX Main
const _VEC_KATEX: &str = "M377 20c0-5.333 1.833-10 5.5-14S391 0 397 0c4.667 0 8.667 1.667 12 5 3.333 2.667 6.667 9 10 19 6.667 24.667 20.333 43.667 41 57 7.333 4.667 11 10.667 11 18 0 6-1 10-3 12s-6.667 5-14 9c-28.667 14.667-53.667 35.667-75 63-1.333 1.333-3.167 3.5-5.5 6.5s-4 4.833-5 5.5c-1 .667-2.5 1.333-4.5 2s-4.333 1-7 1c-4.667 0-9.167-1.833-13.5-5.5S337 184 337 178c0-12.667 15.667-32.333 47-59 H213l-171-1c-8.667-6-13-12.333-13-19 0-4.667 4.333-11.333 13-20h359 c-16-25.333-24-45-24-59z";

// ---------------------------------------------------------------------------
// KaTeX images data lookup table (from katexImagesData in katex.js)
// ---------------------------------------------------------------------------

struct KatexImageData {
    paths: &'static [&'static str],
    /// KaTeX `katexImagesData` minimum arrow shaft width (em).
    min_width: f64,
    vb_height: f64,
    align: Option<&'static str>,
}

fn katex_image_data(label: &str) -> Option<KatexImageData> {
    let key = label.trim_start_matches('\\');
    match key {
        "overrightarrow" => Some(KatexImageData {
            paths: &["rightarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        "overleftarrow" => Some(KatexImageData {
            paths: &["leftarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "underrightarrow" => Some(KatexImageData {
            paths: &["rightarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        "underleftarrow" => Some(KatexImageData {
            paths: &["leftarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "xrightarrow" => Some(KatexImageData {
            paths: &["rightarrow"],
            min_width: 1.469,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        // KaTeX `stretchy.js` `katexImagesData`: `\\cdrightarrow` / `\\cdlongequal` use minWidth 3.0em
        // (comment “CD minwwidth2.5pc” — amscd `\minCDarrowwidth`, ~2.5pc ≈ 3em at 10pt).
        "cdrightarrow" => Some(KatexImageData {
            paths: &["rightarrow"],
            min_width: 3.0,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        "cdleftarrow" => Some(KatexImageData {
            paths: &["leftarrow"],
            min_width: 3.0,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "xleftarrow" => Some(KatexImageData {
            paths: &["leftarrow"],
            min_width: 1.469,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "Overrightarrow" => Some(KatexImageData {
            paths: &["doublerightarrow"],
            min_width: 0.888,
            vb_height: 560.0,
            align: Some("xMaxYMin"),
        }),
        "xRightarrow" => Some(KatexImageData {
            paths: &["doublerightarrow"],
            min_width: 1.526,
            vb_height: 560.0,
            align: Some("xMaxYMin"),
        }),
        "xLeftarrow" => Some(KatexImageData {
            paths: &["doubleleftarrow"],
            min_width: 1.526,
            vb_height: 560.0,
            align: Some("xMinYMin"),
        }),
        "overleftharpoon" | "xleftharpoonup" => Some(KatexImageData {
            paths: &["leftharpoon"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "xleftharpoondown" => Some(KatexImageData {
            paths: &["leftharpoondown"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMinYMin"),
        }),
        "overrightharpoon" | "xrightharpoonup" => Some(KatexImageData {
            paths: &["rightharpoon"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        "xrightharpoondown" => Some(KatexImageData {
            paths: &["rightharpoondown"],
            min_width: 0.888,
            vb_height: 522.0,
            align: Some("xMaxYMin"),
        }),
        "xlongequal" => Some(KatexImageData {
            paths: &["longequal"],
            min_width: 0.888,
            vb_height: 334.0,
            align: Some("xMinYMin"),
        }),
        "cdlongequal" => Some(KatexImageData {
            paths: &["longequal"],
            min_width: 3.0,
            vb_height: 334.0,
            align: Some("xMinYMin"),
        }),
        "xtwoheadleftarrow" => Some(KatexImageData {
            paths: &["twoheadleftarrow"],
            min_width: 0.888,
            vb_height: 334.0,
            align: Some("xMinYMin"),
        }),
        "xtwoheadrightarrow" => Some(KatexImageData {
            paths: &["twoheadrightarrow"],
            min_width: 0.888,
            vb_height: 334.0,
            align: Some("xMaxYMin"),
        }),
        "overleftrightarrow" => Some(KatexImageData {
            paths: &["leftarrow", "rightarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: None,
        }),
        "underleftrightarrow" => Some(KatexImageData {
            paths: &["leftarrow", "rightarrow"],
            min_width: 0.888,
            vb_height: 522.0,
            align: None,
        }),
        "xleftrightarrow" => Some(KatexImageData {
            paths: &["leftarrow", "rightarrow"],
            min_width: 1.75,
            vb_height: 522.0,
            align: None,
        }),
        "xLeftrightarrow" => Some(KatexImageData {
            paths: &["doubleleftarrow", "doublerightarrow"],
            min_width: 1.75,
            vb_height: 560.0,
            align: None,
        }),
        "xrightleftharpoons" => Some(KatexImageData {
            paths: &["leftharpoondownplus", "rightharpoonplus"],
            min_width: 1.75,
            vb_height: 716.0,
            align: None,
        }),
        "xleftrightharpoons" => Some(KatexImageData {
            paths: &["leftharpoonplus", "rightharpoondownplus"],
            min_width: 1.75,
            vb_height: 716.0,
            align: None,
        }),
        "xrightequilibrium" => Some(KatexImageData {
            paths: &["baraboveshortleftharpoon", "rightharpoonaboveshortbar"],
            min_width: 1.75,
            vb_height: 716.0,
            align: None,
        }),
        "xleftequilibrium" => Some(KatexImageData {
            paths: &["shortbaraboveleftharpoon", "shortrightharpoonabovebar"],
            min_width: 1.75,
            vb_height: 716.0,
            align: None,
        }),
        "xhookleftarrow" => Some(KatexImageData {
            paths: &["leftarrow", "righthook"],
            min_width: 1.08,
            vb_height: 522.0,
            align: None,
        }),
        "xhookrightarrow" => Some(KatexImageData {
            paths: &["lefthook", "rightarrow"],
            min_width: 1.08,
            vb_height: 522.0,
            align: None,
        }),
        "overlinesegment" => Some(KatexImageData {
            paths: &["leftlinesegment", "rightlinesegment"],
            min_width: 0.888,
            vb_height: 522.0,
            align: None,
        }),
        "underlinesegment" => Some(KatexImageData {
            paths: &["leftlinesegment", "rightlinesegment"],
            min_width: 0.888,
            vb_height: 522.0,
            align: None,
        }),
        "overgroup" => Some(KatexImageData {
            paths: &["leftgroup", "rightgroup"],
            min_width: 0.888,
            vb_height: 342.0,
            align: None,
        }),
        "undergroup" => Some(KatexImageData {
            paths: &["leftgroupunder", "rightgroupunder"],
            min_width: 0.888,
            vb_height: 342.0,
            align: None,
        }),
        "xmapsto" => Some(KatexImageData {
            paths: &["leftmapsto", "rightarrow"],
            min_width: 1.5,
            vb_height: 522.0,
            align: None,
        }),
        "xtofrom" => Some(KatexImageData {
            paths: &["leftToFrom", "rightToFrom"],
            min_width: 1.75,
            vb_height: 528.0,
            align: None,
        }),
        "xrightleftarrows" => Some(KatexImageData {
            paths: &["baraboveleftarrow", "rightarrowabovebar"],
            min_width: 1.75,
            vb_height: 901.0,
            align: None,
        }),
        // Overbrace/underbrace: KaTeX Size4 glyphs (viewBox 400000×548), same 3-piece horizontal joining as stretchy arrows.
        "overbrace" => Some(KatexImageData {
            paths: &["leftbrace", "midbrace", "rightbrace"],
            min_width: 1.6,
            vb_height: 548.0,
            align: None,
        }),
        "underbrace" => Some(KatexImageData {
            paths: &["leftbraceunder", "midbraceunder", "rightbraceunder"],
            min_width: 1.6,
            vb_height: 548.0,
            align: None,
        }),
        // mathtools `\overbracket` / `\underbracket`: 2-piece KaTeX SVG (same minWidth/height as KaTeX `stretchy.ts`).
        "overbracket" => Some(KatexImageData {
            paths: &["leftbracketover", "rightbracketover"],
            min_width: 1.6,
            vb_height: 440.0,
            align: None,
        }),
        "underbracket" => Some(KatexImageData {
            paths: &["leftbracketunder", "rightbracketunder"],
            min_width: 1.6,
            vb_height: 410.0,
            align: None,
        }),
        _ => None,
    }
}

/// KaTeX [`katexImagesData`](https://github.com/KaTeX/KaTeX/blob/main/src/katexImagesData.js) `minWidth`
/// for `\\xrightarrow`, `\\xleftrightarrow`, mhchem `\\xrightequilibrium`, etc.
pub(crate) fn katex_stretchy_min_width_em(label: &str) -> Option<f64> {
    katex_image_data(label).map(|d| d.min_width)
}

/// Look up the SVG path data for a named path key.
fn path_for_name(name: &str) -> Option<&'static str> {
    match name {
        "rightarrow" => Some(RIGHTARROW),
        "leftarrow" => Some(LEFTARROW),
        "doubleleftarrow" => Some(DOUBLELEFTARROW),
        "doublerightarrow" => Some(DOUBLERIGHTARROW),
        "leftharpoon" => Some(LEFTHARPOON),
        "leftharpoonplus" => Some(LEFTHARPOONPLUS),
        "leftharpoondown" => Some(LEFTHARPOONDOWN),
        "leftharpoondownplus" => Some(LEFTHARPOONDOWNPLUS),
        "rightharpoon" => Some(RIGHTHARPOON),
        "rightharpoonplus" => Some(RIGHTHARPOONPLUS),
        "rightharpoondown" => Some(RIGHTHARPOONDOWN),
        "rightharpoondownplus" => Some(RIGHTHARPOONDOWNPLUS),
        "baraboveshortleftharpoon" => Some(BARABOVESHORTLEFTHARPOON),
        "rightharpoonaboveshortbar" => Some(RIGHTHARPOONABOVESHORTBAR),
        "shortbaraboveleftharpoon" => Some(SHORTBARABOVELEFTHARPOON),
        "shortrightharpoonabovebar" => Some(SHORTRIGHTHARPOONABOVEBAR),
        "lefthook" => Some(LEFTHOOK),
        "righthook" => Some(RIGHTHOOK),
        "leftbrace" => Some(LEFTBRACE),
        "midbrace" => Some(MIDBRACE),
        "rightbrace" => Some(RIGHTBRACE),
        "leftbraceunder" => Some(LEFTBRACEUNDER),
        "midbraceunder" => Some(MIDBRACEUNDER),
        "rightbraceunder" => Some(RIGHTBRACEUNDER),
        "leftToFrom" => Some(LEFTTOFROM),
        "rightToFrom" => Some(RIGHTTOFROM),
        "baraboveleftarrow" => Some(BARABOVELEFTARROW),
        "rightarrowabovebar" => Some(RIGHTARROWABOVEBAR),
        "longequal" => Some(LONGEQUAL),
        "leftlinesegment" => Some(LEFTLINESEGMENT),
        "rightlinesegment" => Some(RIGHTLINESEGMENT),
        "leftmapsto" => Some(LEFTMAPSTO),
        "twoheadleftarrow" => Some(TWOHEADLEFTARROW),
        "twoheadrightarrow" => Some(TWOHEADRIGHTARROW),
        "leftgroup" => Some(LEFTGROUP),
        "leftgroupunder" => Some(LEFTGROUPUNDER),
        "rightgroup" => Some(RIGHTGROUP),
        "rightgroupunder" => Some(RIGHTGROUPUNDER),
        "leftbracketover" => Some(LEFTBRACKETOVER),
        "rightbracketover" => Some(RIGHTBRACKETOVER),
        "leftbracketunder" => Some(LEFTBRACKETUNDER),
        "rightbracketunder" => Some(RIGHTBRACKETUNDER),
        _ => None,
    }
}

/// Generalized KaTeX stretchy element renderer.
///
/// Returns `(commands, height_em)` where commands are centered at y=0
/// (shaft/midline at y≈0, extending ±height_em/2 above/below), or `None` if
/// the label is not in the lookup table.
pub fn katex_stretchy_path(label: &str, width_em: f64) -> Option<(Vec<PathCommand>, f64)> {
    let data = katex_image_data(label)?;
    let s = 1.0 / 1000.0;
    let height_em = data.vb_height * s;
    let vb_cy = data.vb_height / 2.0;
    let y_min = -height_em / 2.0;
    let y_max = height_em / 2.0;

    let make_cmds = |path_name: &str, x_shift: f64| -> Option<Vec<PathCommand>> {
        let svg_str = path_for_name(path_name)?;
        let raw = parsed_static_path(svg_str);
        Some(
            raw.iter()
                .map(|c| scale_cmd_twohead_uniform(c, s, vb_cy, x_shift))
                .collect(),
        )
    };

    match data.paths.len() {
        1 => {
            let x_shift = if data.align == Some("xMaxYMin") {
                width_em - 400_000.0 * s
            } else {
                0.0
            };
            let cmds = make_cmds(data.paths[0], x_shift)?;
            Some((
                clip_path_to_rect(&cmds, 0.0, width_em, y_min, y_max),
                height_em,
            ))
        }
        2 => {
            let x_r = width_em - 400_000.0 * s;
            let lc = make_cmds(data.paths[0], 0.0)?;
            let rc = make_cmds(data.paths[1], x_r)?;
            let is_xmapsto = data.paths[0] == "leftmapsto" && data.paths[1] == "rightarrow";
            let out = if is_xmapsto {
                // xmapsto: draw as one path so the horizontal shaft is guaranteed. KaTeX LEFTMAPSTO
                // has vertical bar at x=0..40 and shaft 40..400000; we draw left bar only, then
                // explicit shaft rect (from KaTeX viewBox 400000×522: y 241–281), then right arrow.
                // Shaft must not extend left of the vertical bar: use shaft_x_end >= left_bar_x_max.
                // Start the shaft slightly left of left_bar_x_max (~1px) so the intersection pixel
                // with the vertical bar is not clipped away by the boundary.
                let left_bar_x_max = 40.0 * s; // 0.04 em, right edge of left vertical bar
                let shaft_overlap = 0.1_f64; // ~1px in em to keep the corner pixel
                let shaft_x_start = (left_bar_x_max - shaft_overlap).max(0.0);
                let shaft_y_top = (241.0 - vb_cy) * s;
                let shaft_y_bot = (281.0 - vb_cy) * s;
                let shaft_x_end = x_r.max(left_bar_x_max);
                let mut cmds = clip_path_to_rect(&lc, 0.0, left_bar_x_max, y_min, y_max);
                if shaft_x_end > shaft_x_start {
                    cmds.push(PathCommand::MoveTo {
                        x: shaft_x_start,
                        y: shaft_y_top,
                    });
                    cmds.push(PathCommand::LineTo {
                        x: shaft_x_end,
                        y: shaft_y_top,
                    });
                    cmds.push(PathCommand::LineTo {
                        x: shaft_x_end,
                        y: shaft_y_bot,
                    });
                    cmds.push(PathCommand::LineTo {
                        x: shaft_x_start,
                        y: shaft_y_bot,
                    });
                    cmds.push(PathCommand::Close);
                }
                cmds.extend(clip_path_to_rect(&rc, 0.0, width_em, y_min, y_max));
                cmds
            } else {
                // Split at the midpoint so each half-arrow's shaft bars don't extend
                // into the opposite arrowhead region.  The left path owns [0, mid]
                // and the right path owns [mid, width_em].  This prevents shaft bar
                // rectangles from covering the opposite arrowhead (e.g. doubleleftarrow
                // shaft bars covering the doublerightarrow arrowhead area).
                let mid = width_em / 2.0;
                let mut cmds = clip_path_to_rect(&lc, 0.0, mid, y_min, y_max);
                cmds.extend(clip_path_to_rect(&rc, mid, width_em, y_min, y_max));
                cmds
            };
            Some((out, height_em))
        }
        3 => {
            let x_m = width_em / 2.0 - 200_000.0 * s;
            let x_r = width_em - 400_000.0 * s;
            let lc = make_cmds(data.paths[0], 0.0)?;
            let mc = make_cmds(data.paths[1], x_m)?;
            let rc = make_cmds(data.paths[2], x_r)?;
            // KaTeX `stretchy.ts` + `katex.scss`: three stacked spans with overflow hidden —
            // `.brace-left` 25.1%, `.brace-center` 50% at left 25%, `.brace-right` 25.1% from the
            // right. Each child SVG is 400em wide with `preserveAspectRatio` slice so only the
            // correct horizontal band shows. Clipping all three to `[0, width]` exposes the
            // middle glyph’s full 400-unit shaft on the entire width → thick bar past the curls.
            let key = label.trim_start_matches('\\');
            let w = width_em;
            let out = if key == "overbrace" || key == "underbrace" {
                let left_max = w * 0.251;
                let center_min = w * 0.25;
                let center_max = w * 0.75;
                let right_min = w * (1.0 - 0.251);
                let mut o = clip_path_to_rect(&lc, 0.0, left_max, y_min, y_max);
                o.extend(clip_path_to_rect(&mc, center_min, center_max, y_min, y_max));
                o.extend(clip_path_to_rect(&rc, right_min, w, y_min, y_max));
                o
            } else {
                let mut o = clip_path_to_rect(&lc, 0.0, w, y_min, y_max);
                o.extend(clip_path_to_rect(&mc, 0.0, w, y_min, y_max));
                o.extend(clip_path_to_rect(&rc, 0.0, w, y_min, y_max));
                o
            };
            Some((out, height_em))
        }
        _ => None,
    }
}

fn map_path_xy_horizontal_to_vertical_cd<F>(cmds: &[PathCommand], map: F) -> Vec<PathCommand>
where
    F: Fn(f64, f64) -> (f64, f64) + Copy,
{
    cmds.iter()
        .map(|c| match *c {
            PathCommand::MoveTo { x, y } => {
                let (nx, ny) = map(x, y);
                PathCommand::MoveTo { x: nx, y: ny }
            }
            PathCommand::LineTo { x, y } => {
                let (nx, ny) = map(x, y);
                PathCommand::LineTo { x: nx, y: ny }
            }
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (n1x, n1y) = map(x1, y1);
                let (n2x, n2y) = map(x2, y2);
                let (nx, ny) = map(x, y);
                PathCommand::CubicTo {
                    x1: n1x,
                    y1: n1y,
                    x2: n2x,
                    y2: n2y,
                    x: nx,
                    y: ny,
                }
            }
            PathCommand::QuadTo { x1, y1, x, y } => {
                let (n1x, n1y) = map(x1, y1);
                let (nx, ny) = map(x, y);
                PathCommand::QuadTo {
                    x1: n1x,
                    y1: n1y,
                    x: nx,
                    y: ny,
                }
            }
            PathCommand::Close => PathCommand::Close,
        })
        .collect()
}

/// Vertical `{CD}` ↑ / ↓: same filled KaTeX SVG as horizontal `\cdrightarrow`, rotated into the column.
///
/// Horizontal stretchy path uses x ∈ [0, `total_height_em`] (shaft length) and thin y-extent; we map
/// `(x,y) → (y_lateral + y, x - row_height)` (down) or mirrored for up so the arrowhead matches the
/// proven horizontal renderer in [`katex_stretchy_path`].
pub fn katex_cd_vert_arrow_from_rightarrow(
    down: bool,
    total_height_em: f64,
    axis_height_em: f64,
) -> Option<(Vec<PathCommand>, f64)> {
    let row_depth = (total_height_em / 2.0 - axis_height_em).max(0.0);
    let row_height = total_height_em - row_depth;
    let (cmds_h, lateral_em) = katex_stretchy_path("\\cdrightarrow", total_height_em)?;
    let w_lat = lateral_em;
    let cmds = if down {
        map_path_xy_horizontal_to_vertical_cd(&cmds_h, |xh, yh| (w_lat / 2.0 + yh, xh - row_height))
    } else {
        map_path_xy_horizontal_to_vertical_cd(&cmds_h, |xh, yh| (w_lat / 2.0 + yh, row_depth - xh))
    };
    Some((cmds, lateral_em))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_width_braces_clip_the_original_curves_at_the_seams() {
        // Intersections of the source SVG cubics with the CSS viewport edges.
        // The minimum width cuts curved segments rather than only the shafts.
        for (piece, is_over, x, expected) in [
            (BracePiece::Left, true, 0.4016, [0.216187571, 0.334226996]),
            (BracePiece::Center, true, 0.4, [0.211259430, 0.330628231]),
            (BracePiece::Center, true, 1.2, [0.211544409, 0.330963731]),
            (BracePiece::Right, true, 1.1984, [0.216836723, 0.337049140]),
            (BracePiece::Left, false, 0.4016, [0.210951138, 0.331163079]),
            (BracePiece::Center, false, 0.4, [0.217036269, 0.336455591]),
            (BracePiece::Center, false, 1.2, [0.217371769, 0.336740570]),
            (BracePiece::Right, false, 1.1984, [0.213773004, 0.331812429]),
        ] {
            let commands = bounded_brace_piece(piece, is_over, BRACE_MIN_WIDTH, None).unwrap();
            for y in expected {
                assert!(
                    commands.iter().any(|command| {
                        let (cx, cy) = match *command {
                            PathCommand::MoveTo { x, y }
                            | PathCommand::LineTo { x, y }
                            | PathCommand::CubicTo { x, y, .. } => (x, y),
                            _ => return false,
                        };
                        (cx - x).abs() < 1e-9 && (cy - y).abs() < 1e-8
                    }),
                    "missing brace intersection ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn test_parse_simple_path() {
        let cmds = parse_svg_path("M0 0 L10 20 Z");
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_parse_katex_vec_path() {
        let cmds = scale_svg_path_thousandths(&parse_svg_path(KATEX_VEC_PATH));
        assert!(
            cmds.len() >= 8,
            "vec path should parse to multiple segments"
        );
        match cmds[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 0.377).abs() < 0.001);
                assert!((y - 0.02).abs() < 0.001);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    #[test]
    fn test_parse_relative() {
        let cmds = parse_svg_path("M10 10 l5 5");
        assert_eq!(cmds.len(), 2);
        match cmds[1] {
            PathCommand::LineTo { x, y } => {
                assert!((x - 15.0).abs() < 0.01);
                assert!((y - 15.0).abs() < 0.01);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn test_parse_widehat1() {
        let cmds = parse_svg_path(WIDEHAT[0]);
        assert!(cmds.len() > 3);
    }

    #[test]
    fn test_katex_accent_widehat() {
        let result = katex_accent_path("\\widehat", 1.5, 1);
        assert!(result.is_some());
    }

    #[test]
    fn test_katex_accent_overgroup() {
        let result = katex_accent_path("\\overgroup", 1.5, 1);
        assert!(result.is_some());
    }

    #[test]
    fn test_tilde_path_coordinates() {
        let raw = parse_svg_path(TILDE[2]);
        assert_eq!(raw.len(), 16);
        // Verify the critical cubic that was previously broken by `\` line continuation
        match raw[7] {
            PathCommand::CubicTo { x1, y1, .. } => {
                assert!((x1 - 1141.3).abs() < 0.1, "x1 should be 1141.3, got {}", x1);
                assert!((y1 - 0.0).abs() < 0.1, "y1 should be 0, got {}", y1);
            }
            _ => panic!("expected CubicTo at index 7"),
        }
    }

    #[test]
    fn test_katex_stretchy_arrow_xtwohead() {
        let r = katex_stretchy_arrow_path("\\xtwoheadrightarrow", 2.0, 0.3);
        assert!(r.is_some());
        let cmds = r.unwrap();
        assert!(!cmds.is_empty());
        let l = katex_stretchy_arrow_path("\\xtwoheadleftarrow", 2.0, 0.3);
        assert!(l.is_some());
        assert!(!l.unwrap().is_empty());
        assert!(katex_stretchy_arrow_path("\\xrightarrow", 2.0, 0.3).is_some());
        assert!(katex_stretchy_arrow_path("\\xleftarrow", 2.0, 0.3).is_some());
    }

    #[test]
    fn test_katex_cd_vert_arrow_from_rightarrow() {
        let axis = 0.25_f64;
        for down in [true, false] {
            let r = katex_cd_vert_arrow_from_rightarrow(down, 2.5, axis);
            assert!(r.is_some(), "down={}", down);
            let (cmds, w) = r.unwrap();
            assert!(!cmds.is_empty());
            assert!(
                w > 0.45 && w < 0.58,
                "lateral extent should match horizontal arrow ink height ~0.522em, got {w}"
            );
        }
    }
}

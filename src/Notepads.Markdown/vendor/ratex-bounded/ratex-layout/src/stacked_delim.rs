//! Stacked stretchy delimiters for `\left`/`\right` when a single Size4 glyph is too short.
//! Mirrors KaTeX `delimiter.ts`: `stackLargeDelimiters` use `makeStackedDelim` past the large-font stage.

use ratex_font::{get_char_metrics, CharMetrics, FontId};
use ratex_types::path_command::PathCommand;
use ratex_types::storage::StorageFamily;

use crate::katex_svg::parse_svg_path_data;
use crate::layout_box::{BoxContent, LayoutBox, VBoxChild, VBoxChildKind};
use crate::layout_options::LayoutOptions;
use crate::scoped_options::ScopedOptions;
use crate::vbox::make_vbox;

const LAP: f64 = 0.008;

/// Build a tall matrix delimiter from sealed font caps and a constant number
/// of funded rectangular stems. Stem positions match the joins in the Size4 outlines.
pub(crate) fn make_bounded_matrix_stack(
    delim: &str,
    total_height: f64,
    options: &ScopedOptions<'_>,
) -> LayoutBox {
    let (top, middle, bottom, x, width) = match delim {
        "(" => (0x239b, None, 0x239d, 0.298, 0.111),
        ")" => (0x239e, None, 0x23a0, 0.464, 0.111),
        "[" => (0x23a1, None, 0x23a3, 0.319, 0.084),
        "]" => (0x23a4, None, 0x23a6, 0.263, 0.084),
        "\\{" => (0x23a7, Some(0x23a8), 0x23a9, 0.384, 0.120),
        "\\}" => (0x23ab, Some(0x23ac), 0x23ad, 0.384, 0.120),
        _ => {
            options.unsupported();
            return LayoutBox::new_empty();
        }
    };
    let top = size4_glyph(top, options);
    let middle = middle.map(|code| size4_glyph(code, options));
    let bottom = size4_glyph(bottom, options);
    let caps_height = top.total_height()
        + middle.as_ref().map_or(0.0, LayoutBox::total_height)
        + bottom.total_height();
    let joins = if middle.is_some() { 2.0 } else { 1.0 };
    let stem_height = (total_height - caps_height).max(0.0) / joins + 2.0 * LAP;

    let lap = || VBoxChild {
        kind: VBoxChildKind::Kern(-LAP),
        shift: 0.0,
    };
    let push_box = |children: &mut Vec<_>, item, shift| {
        let Ok(item) = options.boxed(item) else {
            return false;
        };
        options.push(
            StorageFamily::Layout,
            children,
            VBoxChild {
                kind: VBoxChildKind::Box(item),
                shift,
            },
        )
    };
    let mut children = Vec::new();
    if !push_box(&mut children, top, 0.0) {
        return LayoutBox::new_empty();
    }
    for piece in middle.into_iter().chain(std::iter::once(bottom)) {
        let Ok(stem) = LayoutBox::new_filled_rect(width, stem_height, 0.0, options) else {
            return LayoutBox::new_empty();
        };
        if !options.push(StorageFamily::Layout, &mut children, lap())
            || !push_box(&mut children, stem, x)
            || !options.push(StorageFamily::Layout, &mut children, lap())
            || !push_box(&mut children, piece, 0.0)
        {
            return LayoutBox::new_empty();
        }
    }
    let mut result = make_vbox(children);
    let span = result.total_height();
    result.height = span / 2.0 + options.metrics().axis_height;
    result.depth = span - result.height;
    result.color = options.color;
    result
}

#[derive(Clone, Copy)]
enum StackDelimKind {
    Brace { open: bool },
    Bracket { open: bool },
    Paren { open: bool },
    LFloor,
    RFloor,
    LCeil,
    RCeil,
    Group { open: bool },
}

fn normalize_for_stack(delim: &str) -> &str {
    match delim {
        "{" => "\\{",
        "}" => "\\}",
        _ => delim,
    }
}

fn classify_stackable(delim: &str) -> Option<StackDelimKind> {
    let d = normalize_for_stack(delim);
    match d {
        "\\{" | "\\lbrace" => Some(StackDelimKind::Brace { open: true }),
        "\\}" | "\\rbrace" => Some(StackDelimKind::Brace { open: false }),
        "[" | "\\lbrack" => Some(StackDelimKind::Bracket { open: true }),
        "]" | "\\rbrack" => Some(StackDelimKind::Bracket { open: false }),
        "(" | "\\lparen" => Some(StackDelimKind::Paren { open: true }),
        ")" | "\\rparen" => Some(StackDelimKind::Paren { open: false }),
        "\\lfloor" | "\u{230a}" => Some(StackDelimKind::LFloor),
        "\\rfloor" | "\u{230b}" => Some(StackDelimKind::RFloor),
        "\\lceil" | "\u{2308}" => Some(StackDelimKind::LCeil),
        "\\rceil" | "\u{2309}" => Some(StackDelimKind::RCeil),
        "\\lgroup" | "\u{27ee}" => Some(StackDelimKind::Group { open: true }),
        "\\rgroup" | "\u{27ef}" => Some(StackDelimKind::Group { open: false }),
        _ => None,
    }
}

/// KaTeX `stackNeverDelimiters`.
pub(crate) fn is_stack_never_delim(delim: &str) -> bool {
    matches!(
        delim,
        "<" | ">"
            | "\\langle"
            | "\\rangle"
            | "/"
            | "\\backslash"
            | "\\lt"
            | "\\gt"
            | "\u{27e8}"
            | "\u{27e9}"
    )
}

fn size4_glyph(char_code: u32, options: &LayoutOptions) -> LayoutBox {
    let m = get_char_metrics(FontId::Size4Regular, char_code).expect("stacked delim piece");
    LayoutBox {
        width: m.width,
        height: m.height,
        depth: m.depth,
        content: BoxContent::Glyph {
            font_id: FontId::Size4Regular,
            char_code,
        },
        color: options.color,
    }
}

fn height_depth(m: &CharMetrics) -> f64 {
    m.height + m.depth
}

fn brace_repeat_svg(inner_height_em: f64, options: &LayoutOptions) -> LayoutBox {
    let m = get_char_metrics(FontId::Size4Regular, 0x23aa).expect("brace repeat");
    let x0 = 384.0 / 1000.0;
    let x1 = 504.0 / 1000.0;
    let cmds = vec![
        PathCommand::MoveTo {
            x: x0,
            y: -inner_height_em,
        },
        PathCommand::LineTo {
            x: x1,
            y: -inner_height_em,
        },
        PathCommand::LineTo { x: x1, y: 0.0 },
        PathCommand::LineTo { x: x0, y: 0.0 },
        PathCommand::Close,
    ];
    LayoutBox {
        width: m.width,
        height: inner_height_em,
        depth: 0.0,
        content: BoxContent::SvgPath {
            commands: cmds,
            fill: true,
        },
        color: options.color,
    }
}

fn axis_center_stacked_vbox(body: LayoutBox, options: &LayoutOptions) -> LayoutBox {
    let h_vis = body.height + body.depth;
    let axis = options.metrics().axis_height * options.size_multiplier();
    let shift = axis - h_vis / 2.0;
    let depth = (h_vis / 2.0 - axis).max(0.0);
    LayoutBox {
        width: body.width,
        height: h_vis / 2.0 + axis,
        depth,
        content: BoxContent::RaiseBox {
            body: Box::new(body),
            shift,
        },
        color: options.color,
    }
}

fn tall_delim_svg_path(label: &str, mid_th: i64) -> String {
    let m = mid_th;
    match label {
        "lbrack" => format!(
            "M403 1759 V84 H666 V0 H319 V1759 v{m} v1759 h347 v-84 H403z M403 1759 V0 H319 V1759 v{m} v1759 h84z"
        ),
        "rbrack" => format!(
            "M347 1759 V0 H0 V84 H263 V1759 v{m} v1759 H0 v84 H347z M347 1759 V0 H263 V1759 v{m} v1759 h84z"
        ),
        "lfloor" => format!(
            "M319 602 V0 H403 V602 v{m} v1715 h263 v84 H319z M319 602 V0 H403 V602 v{m} v1715 H319z"
        ),
        "rfloor" => format!(
            "M319 602 V0 H403 V602 v{m} v1799 H0 v-84 H319z M319 602 V0 H403 V602 v{m} v1715 H319z"
        ),
        "lceil" => format!(
            "M403 1759 V84 H666 V0 H319 V1759 v{m} v602 h84z M403 1759 V0 H319 V1759 v{m} v602 h84z"
        ),
        "rceil" => format!(
            "M347 1759 V0 H0 V84 H263 V1759 v{m} v602 h84z M347 1759 V0 h-84 V1759 v{m} v602 h84z"
        ),
        "lparen" => {
            let m84 = m + 84;
            let m92 = m + 92;
            format!(
                "M863,9c0,-2,-2,-5,-6,-9c0,0,-17,0,-17,0c-12.7,0,-19.3,0.3,-20,1c-5.3,5.3,-10.3,11,-15,17c-242.7,294.7,-395.3,682,-458,1162c-21.3,163.3,-33.3,349,-36,557 l0,{m84}c0.2,6,0,26,0,60c2,159.3,10,310.7,24,454c53.3,528,210,949.7,470,1265c4.7,6,9.7,11.7,15,17c0.7,0.7,7,1,19,1c0,0,18,0,18,0c4,-4,6,-7,6,-9c0,-2.7,-3.3,-8.7,-10,-18c-135.3,-192.7,-235.5,-414.3,-300.5,-665c-65,-250.7,-102.5,-544.7,-112.5,-882c-2,-104,-3,-167,-3,-189 l0,-{m92}c0,-162.7,5.7,-314,17,-454c20.7,-272,63.7,-513,129,-723c65.3,-210,155.3,-396.3,270,-559c6.7,-9.3,10,-15.3,10,-18z"
            )
        }
        "rparen" => {
            let m9 = m + 9;
            let m144 = m + 144;
            format!(
                "M76,0c-16.7,0,-25,3,-25,9c0,2,2,6.3,6,13c21.3,28.7,42.3,60.3,63,95c96.7,156.7,172.8,332.5,228.5,527.5c55.7,195,92.8,416.5,111.5,664.5c11.3,139.3,17,290.7,17,454c0,28,1.7,43,3.3,45l0,{m9}c-3,4,-3.3,16.7,-3.3,38c0,162,-5.7,313.7,-17,455c-18.7,248,-55.8,469.3,-111.5,664c-55.7,194.7,-131.8,370.3,-228.5,527c-20.7,34.7,-41.7,66.3,-63,95c-2,3.3,-4,7,-6,11c0,7.3,5.7,11,17,11c0,0,11,0,11,0c9.3,0,14.3,-0.3,15,-1c5.3,-5.3,10.3,-11,15,-17c242.7,-294.7,395.3,-681.7,458,-1161c21.3,-164.7,33.3,-350.7,36,-558 l0,-{m144}c-2,-159.3,-10,-310.7,-24,-454c-53.3,-528,-210,-949.7,-470,-1265c-4.7,-6,-9.7,-11.7,-15,-17c-0.7,-0.7,-6.7,-1,-18,-1z"
            )
        }
        _ => String::new(),
    }
}

fn svg_label_for(kind: StackDelimKind) -> &'static str {
    match kind {
        StackDelimKind::Bracket { open: true } => "lbrack",
        StackDelimKind::Bracket { open: false } => "rbrack",
        StackDelimKind::Paren { open: true } => "lparen",
        StackDelimKind::Paren { open: false } => "rparen",
        StackDelimKind::LFloor => "lfloor",
        StackDelimKind::RFloor => "rfloor",
        StackDelimKind::LCeil => "lceil",
        StackDelimKind::RCeil => "rceil",
        _ => "",
    }
}

fn width_em_for_label(label: &str) -> f64 {
    match label {
        "lparen" | "rparen" => 0.875,
        _ => 0.667,
    }
}

fn shift_path_y(cmds: Vec<PathCommand>, dy: f64) -> Vec<PathCommand> {
    cmds.into_iter()
        .map(|c| match c {
            PathCommand::MoveTo { x, y } => PathCommand::MoveTo { x, y: y + dy },
            PathCommand::LineTo { x, y } => PathCommand::LineTo { x, y: y + dy },
            PathCommand::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => PathCommand::CubicTo {
                x1,
                y1: y1 + dy,
                x2,
                y2: y2 + dy,
                x,
                y: y + dy,
            },
            PathCommand::QuadTo { x1, y1, x, y } => PathCommand::QuadTo {
                x1,
                y1: y1 + dy,
                x,
                y: y + dy,
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect()
}

fn scale_path_to_em(cmds: &[PathCommand]) -> Vec<PathCommand> {
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

fn make_tall_svg_delim(
    kind: StackDelimKind,
    height_total: f64,
    options: &LayoutOptions,
) -> Option<LayoutBox> {
    let label = svg_label_for(kind);
    if label.is_empty() {
        return None;
    }

    let (top_c, bot_c) = match kind {
        StackDelimKind::Bracket { open: true } => (0x23a1u32, 0x23a3),
        StackDelimKind::Bracket { open: false } => (0x23a4, 0x23a6),
        StackDelimKind::Paren { open: true } => (0x239b, 0x239d),
        StackDelimKind::Paren { open: false } => (0x239e, 0x23a0),
        StackDelimKind::LFloor => (0x23a2, 0x23a3),
        StackDelimKind::RFloor => (0x23a5, 0x23a6),
        StackDelimKind::LCeil => (0x23a1, 0x23a2),
        StackDelimKind::RCeil => (0x23a4, 0x23a5),
        _ => return None,
    };

    let top_m = get_char_metrics(FontId::Size4Regular, top_c)
        .map(|m| height_depth(&m))
        .unwrap_or(0.0);
    let bot_m = get_char_metrics(FontId::Size4Regular, bot_c)
        .map(|m| height_depth(&m))
        .unwrap_or(0.0);
    let min_h = top_m + bot_m;
    let mid_em = (height_total - min_h).max(0.0);
    let mid_th = (mid_em * 1000.0).round() as i64;

    let d = tall_delim_svg_path(label, mid_th);
    let raw = parse_svg_path_data(&d);
    if raw.is_empty() {
        return None;
    }
    // The KaTeX SVG paths use a top-origin coordinate system (y=0 at top, y=height_total
    // at bottom).  RaTeX places SvgPath items at the box baseline, so y=0 maps to the
    // baseline and positive y extends downward.  Shift every y by -height_total so the
    // path spans [-height_total, 0] (above baseline to baseline), matching the declared
    // LayoutBox dimensions (height=height_total, depth=0).
    let cmds_raw = scale_path_to_em(&raw);
    let cmds = shift_path_y(cmds_raw, -height_total);
    let w = width_em_for_label(label);
    let inner = LayoutBox {
        width: w,
        height: height_total,
        depth: 0.0,
        content: BoxContent::SvgPath {
            commands: cmds,
            fill: true,
        },
        color: options.color,
    };
    Some(axis_center_stacked_vbox(inner, options))
}

fn make_glyph_stack_delim(
    kind: StackDelimKind,
    height_total: f64,
    options: &LayoutOptions,
) -> Option<LayoutBox> {
    let lap_kern = |k: f64| VBoxChild {
        kind: VBoxChildKind::Kern(-k),
        shift: 0.0,
    };
    let bx = |lb: LayoutBox| VBoxChild {
        kind: VBoxChildKind::Box(Box::new(lb)),
        shift: 0.0,
    };

    match kind {
        StackDelimKind::Brace { open } => {
            let (top_c, mid_c, bot_c, _rep_c) = if open {
                (0x23a7u32, 0x23a8, 0x23a9, 0x23aa)
            } else {
                (0x23abu32, 0x23ac, 0x23ad, 0x23aa)
            };
            let top = get_char_metrics(FontId::Size4Regular, top_c)?;
            let mid = get_char_metrics(FontId::Size4Regular, mid_c)?;
            let bot = get_char_metrics(FontId::Size4Regular, bot_c)?;
            let top_ht = height_depth(&top);
            let mid_ht = height_depth(&mid);
            let bot_ht = height_depth(&bot);
            let min_h = top_ht + mid_ht + bot_ht;
            let inner_h = ((height_total - min_h) / 2.0).max(0.0) + 2.0 * LAP;

            let ch: Vec<VBoxChild> = vec![
                bx(size4_glyph(top_c, options)),
                lap_kern(LAP),
                bx(brace_repeat_svg(inner_h, options)),
                lap_kern(LAP),
                bx(size4_glyph(mid_c, options)),
                lap_kern(LAP),
                bx(brace_repeat_svg(inner_h, options)),
                lap_kern(LAP),
                bx(size4_glyph(bot_c, options)),
            ];

            Some(axis_center_stacked_vbox(make_vbox(ch), options))
        }
        StackDelimKind::Group { open } => {
            let (top_c, bot_c, _) = if open {
                (0x23a7u32, 0x23a9, 0x23aa)
            } else {
                (0x23abu32, 0x23ad, 0x23aa)
            };
            let top = get_char_metrics(FontId::Size4Regular, top_c)?;
            let bot = get_char_metrics(FontId::Size4Regular, bot_c)?;
            let top_ht = height_depth(&top);
            let bot_ht = height_depth(&bot);
            let min_h = top_ht + bot_ht;
            let inner_h = (height_total - min_h).max(0.0) + 2.0 * LAP;

            let ch: Vec<VBoxChild> = vec![
                bx(size4_glyph(top_c, options)),
                lap_kern(LAP),
                bx(brace_repeat_svg(inner_h, options)),
                lap_kern(LAP),
                bx(size4_glyph(bot_c, options)),
            ];

            Some(axis_center_stacked_vbox(make_vbox(ch), options))
        }
        _ => None,
    }
}

/// If fixed-size glyphs cannot reach `total_height`, build a stacked / SVG delimiter.
pub(crate) fn make_stacked_delim_if_needed(
    delim: &str,
    total_height: f64,
    best_glyph_total: f64,
    options: &LayoutOptions,
) -> Option<LayoutBox> {
    if is_stack_never_delim(delim) {
        return None;
    }
    if best_glyph_total + 1e-6 >= total_height {
        return None;
    }
    let kind = classify_stackable(delim)?;

    match kind {
        StackDelimKind::Brace { .. } | StackDelimKind::Group { .. } => {
            make_glyph_stack_delim(kind, total_height, options)
        }
        StackDelimKind::Bracket { .. }
        | StackDelimKind::Paren { .. }
        | StackDelimKind::LFloor
        | StackDelimKind::RFloor
        | StackDelimKind::LCeil
        | StackDelimKind::RCeil => make_tall_svg_delim(kind, total_height, options),
    }
}

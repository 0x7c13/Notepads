//! SVG emitted bounds scanner used for Mermaid parity.

use super::svg_path_bounds_from_d;
use crate::model::Bounds;

#[derive(Debug, Clone)]
pub struct SvgEmittedBoundsContributor {
    pub tag: String,
    pub id: Option<String>,
    pub class: Option<String>,
    pub d: Option<String>,
    pub points: Option<String>,
    pub transform: Option<String>,
    pub bounds: Bounds,
}

#[derive(Debug, Clone)]
pub struct SvgEmittedBoundsDebug {
    pub bounds: Bounds,
    pub min_x: Option<SvgEmittedBoundsContributor>,
    pub min_y: Option<SvgEmittedBoundsContributor>,
    pub max_x: Option<SvgEmittedBoundsContributor>,
    pub max_y: Option<SvgEmittedBoundsContributor>,
}

#[doc(hidden)]
pub fn debug_svg_emitted_bounds(svg: &str) -> Option<SvgEmittedBoundsDebug> {
    let mut dbg = SvgEmittedBoundsDebug {
        bounds: Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        },
        min_x: None,
        min_y: None,
        max_x: None,
        max_y: None,
    };
    let b = svg_emitted_bounds_from_svg_inner(svg, Some(&mut dbg))?;
    dbg.bounds = b;
    Some(dbg)
}

pub(in crate::svg::parity) fn svg_emitted_bounds_from_svg(svg: &str) -> Option<Bounds> {
    svg_emitted_bounds_from_svg_inner(svg, None)
}

pub(in crate::svg::parity) fn svg_emitted_bounds_from_svg_inner(
    svg: &str,
    mut dbg: Option<&mut SvgEmittedBoundsDebug>,
) -> Option<Bounds> {
    #[derive(Clone, Copy, Debug)]
    struct AffineTransform {
        // SVG 2D affine matrix in the same form as `matrix(a b c d e f)`:
        //   [a c e]
        //   [b d f]
        //   [0 0 1]
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
    }

    impl AffineTransform {
        fn apply_point(self, x: f64, y: f64) -> (f64, f64) {
            let ox = (self.a * x + self.c * y) + self.e;
            let oy = (self.b * x + self.d * y) + self.f;
            (ox, oy)
        }
    }

    fn parse_f64(raw: &str) -> Option<f64> {
        let s = raw.trim().trim_end_matches("px").trim();
        s.parse::<f64>().ok()
    }

    fn deg_to_rad(deg: f64) -> f64 {
        deg * std::f64::consts::PI / 180.0
    }

    fn attr_value<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
        // Assumes our generated SVG uses `key="value"` quoting and that attributes are separated
        // by at least one whitespace character.
        //
        // Important: the naive `attrs.find(r#"{key}=""#)` is *not* safe for 1-letter keys like
        // `d` because it can match inside other attribute names (e.g. `id="..."` contains `d="`).
        // That would break path bbox parsing and, in turn, root viewBox parity.
        let bytes = attrs.as_bytes();
        let mut from = 0usize;
        while from < attrs.len() {
            let rel = attrs[from..].find(key)?;
            let pos = from + rel;
            let ok_prefix = pos == 0 || bytes[pos.saturating_sub(1)].is_ascii_whitespace();
            if ok_prefix {
                let after_key = pos + key.len();
                if after_key + 1 < attrs.len()
                    && bytes[after_key] == b'='
                    && bytes[after_key + 1] == b'"'
                {
                    let start = after_key + 2;
                    let rest = &attrs[start..];
                    let end = rest.find('"')?;
                    return Some(&rest[..end]);
                }
            }
            from = pos + 1;
        }
        None
    }

    fn parse_transform_ops_into(transform: &str, ops: &mut Vec<AffineTransform>) {
        // Mermaid output routinely uses rotated elements (e.g. gitGraph commit labels,
        // Architecture edge labels). For parity-root viewport computations we need to support
        // a reasonably complete SVG transform subset.
        let mut s = transform.trim();

        while !s.is_empty() {
            let ws = s
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
            s = &s[ws..];
            if s.is_empty() {
                break;
            }

            let Some(paren) = s.find('(') else {
                break;
            };
            let name = s[..paren].trim();
            let rest = &s[paren + 1..];
            let Some(end) = rest.find(')') else {
                break;
            };
            let inner = rest[..end].replace(',', " ");
            let mut parts = inner.split_whitespace().filter_map(parse_f64);

            match name {
                "translate" => {
                    let x = parts.next().unwrap_or(0.0);
                    let y = parts.next().unwrap_or(0.0);
                    ops.push(AffineTransform {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: x,
                        f: y,
                    });
                }
                "scale" => {
                    let sx = parts.next().unwrap_or(1.0);
                    let sy = parts.next().unwrap_or(sx);
                    ops.push(AffineTransform {
                        a: sx,
                        b: 0.0,
                        c: 0.0,
                        d: sy,
                        e: 0.0,
                        f: 0.0,
                    });
                }
                "rotate" => {
                    let angle_deg = parts.next().unwrap_or(0.0);
                    let cx = parts.next();
                    let cy = parts.next();
                    let rad = deg_to_rad(angle_deg);
                    let cos = rad.cos();
                    let sin = rad.sin();

                    match (cx, cy) {
                        (Some(cx), Some(cy)) => {
                            // T(cx,cy) * R(angle) * T(-cx,-cy), represented as one matrix.
                            let e = cx - (cx * cos) + (cy * sin);
                            let f = cy - (cx * sin) - (cy * cos);
                            ops.push(AffineTransform {
                                a: cos,
                                b: sin,
                                c: -sin,
                                d: cos,
                                e,
                                f,
                            });
                        }
                        _ => {
                            ops.push(AffineTransform {
                                a: cos,
                                b: sin,
                                c: -sin,
                                d: cos,
                                e: 0.0,
                                f: 0.0,
                            });
                        }
                    }
                }
                "skewX" | "skewx" => {
                    let angle_deg = parts.next().unwrap_or(0.0);
                    let k = deg_to_rad(angle_deg).tan();
                    ops.push(AffineTransform {
                        a: 1.0,
                        b: 0.0,
                        c: k,
                        d: 1.0,
                        e: 0.0,
                        f: 0.0,
                    });
                }
                "skewY" | "skewy" => {
                    let angle_deg = parts.next().unwrap_or(0.0);
                    let k = deg_to_rad(angle_deg).tan();
                    ops.push(AffineTransform {
                        a: 1.0,
                        b: k,
                        c: 0.0,
                        d: 1.0,
                        e: 0.0,
                        f: 0.0,
                    });
                }
                "matrix" => {
                    // matrix(a b c d e f)
                    let a = parts.next().unwrap_or(1.0);
                    let b = parts.next().unwrap_or(0.0);
                    let c = parts.next().unwrap_or(0.0);
                    let d = parts.next().unwrap_or(1.0);
                    let e = parts.next().unwrap_or(0.0);
                    let f = parts.next().unwrap_or(0.0);
                    ops.push(AffineTransform { a, b, c, d, e, f });
                }
                _ => {}
            };

            s = &rest[end + 1..];
        }

        // Caller owns `ops`.
    }

    fn parse_view_box(view_box: &str) -> Option<(f64, f64, f64, f64)> {
        let buf = view_box.replace(',', " ");
        let mut parts = buf.split_whitespace().filter_map(parse_f64);
        let x = parts.next()?;
        let y = parts.next()?;
        let w = parts.next()?;
        let h = parts.next()?;
        if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some((x, y, w, h))
    }

    fn svg_viewport_transform(attrs: &str) -> AffineTransform {
        // Nested <svg> establishes a new viewport. Map its internal user coordinates into the
        // parent coordinate system via x/y + viewBox scaling.
        //
        // Equivalent to: translate(x,y) * scale(width/vbw, height/vbh) * translate(-vbx, -vby)
        // when viewBox is present. When viewBox is absent, treat it as a 1:1 user unit space.
        let x = attr_value(attrs, "x").and_then(parse_f64).unwrap_or(0.0);
        let y = attr_value(attrs, "y").and_then(parse_f64).unwrap_or(0.0);

        let Some((vb_x, vb_y, vb_w, vb_h)) = attr_value(attrs, "viewBox").and_then(parse_view_box)
        else {
            return AffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: x,
                f: y,
            };
        };

        let w = attr_value(attrs, "width")
            .and_then(parse_f64)
            .unwrap_or(vb_w);
        let h = attr_value(attrs, "height")
            .and_then(parse_f64)
            .unwrap_or(vb_h);
        if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
            return AffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: x,
                f: y,
            };
        }

        let sx = w / vb_w;
        let sy = h / vb_h;
        AffineTransform {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: x - sx * vb_x,
            f: y - sy * vb_y,
        }
    }

    fn maybe_record_dbg(
        dbg: &mut Option<&mut SvgEmittedBoundsDebug>,
        tag: &str,
        attrs: &str,
        b: Bounds,
    ) {
        let Some(dbg) = dbg.as_deref_mut() else {
            return;
        };
        let id = attr_value(attrs, "id").map(|s| s.to_string());
        let class = attr_value(attrs, "class").map(|s| s.to_string());
        let d = attr_value(attrs, "d").map(|s| s.to_string());
        let points = attr_value(attrs, "points").map(|s| s.to_string());
        let transform = attr_value(attrs, "transform").map(|s| s.to_string());
        let c = SvgEmittedBoundsContributor {
            tag: tag.to_string(),
            id,
            class,
            d,
            points,
            transform,
            bounds: b.clone(),
        };

        if dbg
            .min_x
            .as_ref()
            .map(|cur| b.min_x < cur.bounds.min_x)
            .unwrap_or(true)
        {
            dbg.min_x = Some(c.clone());
        }
        if dbg
            .min_y
            .as_ref()
            .map(|cur| b.min_y < cur.bounds.min_y)
            .unwrap_or(true)
        {
            dbg.min_y = Some(c.clone());
        }
        if dbg
            .max_x
            .as_ref()
            .map(|cur| b.max_x > cur.bounds.max_x)
            .unwrap_or(true)
        {
            dbg.max_x = Some(c.clone());
        }
        if dbg
            .max_y
            .as_ref()
            .map(|cur| b.max_y > cur.bounds.max_y)
            .unwrap_or(true)
        {
            dbg.max_y = Some(c);
        }
    }

    #[derive(Default)]
    struct BoundsAccumulator {
        bounds: Option<Bounds>,
    }

    impl BoundsAccumulator {
        fn include(&mut self, candidate: Bounds) {
            // Empty placeholder geometry does not expand an SVG bbox. Non-empty Mermaid
            // placeholders, including 0.1 by 0.1 rects, remain part of emitted geometry.
            let width = (candidate.max_x - candidate.min_x).abs();
            let height = (candidate.max_y - candidate.min_y).abs();
            if width < 1e-9 && height < 1e-9 {
                return;
            }

            if let Some(bounds) = self.bounds.as_mut() {
                bounds.min_x = bounds.min_x.min(candidate.min_x);
                bounds.min_y = bounds.min_y.min(candidate.min_y);
                bounds.max_x = bounds.max_x.max(candidate.max_x);
                bounds.max_y = bounds.max_y.max(candidate.max_y);
            } else {
                self.bounds = Some(candidate);
            }
        }

        fn finish(self) -> Option<Bounds> {
            self.bounds
        }
    }

    fn include_points(
        bounds: &mut BoundsAccumulator,
        points: &str,
        cur_ops: &[AffineTransform],
        el_ops: &[AffineTransform],
    ) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut have = false;

        let buf = points.replace(',', " ");
        let mut nums = buf.split_whitespace().filter_map(parse_f64);
        while let Some(x) = nums.next() {
            let Some(y) = nums.next() else { break };
            have = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        if have {
            let b = apply_ops_bounds(
                cur_ops,
                el_ops,
                Bounds {
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                },
            );
            bounds.include(b);
        }
    }

    let mut bounds = BoundsAccumulator::default();

    fn apply_ops_point(
        cur_ops: &[AffineTransform],
        el_ops: &[AffineTransform],
        mut x: f64,
        mut y: f64,
    ) -> (f64, f64) {
        for op in el_ops.iter().rev() {
            (x, y) = op.apply_point(x, y);
        }
        for op in cur_ops.iter().rev() {
            (x, y) = op.apply_point(x, y);
        }
        (x, y)
    }

    fn apply_ops_bounds(
        cur_ops: &[AffineTransform],
        el_ops: &[AffineTransform],
        b: Bounds,
    ) -> Bounds {
        let (x0, y0) = apply_ops_point(cur_ops, el_ops, b.min_x, b.min_y);
        let (x1, y1) = apply_ops_point(cur_ops, el_ops, b.min_x, b.max_y);
        let (x2, y2) = apply_ops_point(cur_ops, el_ops, b.max_x, b.min_y);
        let (x3, y3) = apply_ops_point(cur_ops, el_ops, b.max_x, b.max_y);
        Bounds {
            min_x: x0.min(x1).min(x2).min(x3),
            min_y: y0.min(y1).min(y2).min(y3),
            max_x: x0.max(x1).max(x2).max(x3),
            max_y: y0.max(y1).max(y2).max(y3),
        }
    }

    // Elements under `<defs>` and other non-rendered containers (e.g. `<marker>`) must be ignored
    // for `getBBox()`-like computations; they do not contribute to the rendered content bbox.
    let mut defs_depth: usize = 0;
    let mut tf_stack: Vec<usize> = Vec::new();
    let mut cur_ops: Vec<AffineTransform> = Vec::new();
    let mut el_ops_buf: Vec<AffineTransform> = Vec::new();
    let mut seen_root_svg = false;
    let mut nested_svg_depth = 0usize;

    let mut i = 0usize;
    while i < svg.len() {
        let Some(rel) = svg[i..].find('<') else {
            break;
        };
        i += rel;

        // Comments.
        if svg[i..].starts_with("<!--") {
            if let Some(end_rel) = svg[i + 4..].find("-->") {
                i = i + 4 + end_rel + 3;
                continue;
            }
            break;
        }

        // Processing instructions.
        if svg[i..].starts_with("<?") {
            if let Some(end_rel) = svg[i + 2..].find("?>") {
                i = i + 2 + end_rel + 2;
                continue;
            }
            break;
        }

        let close = svg[i..].starts_with("</");
        let tag_start = if close { i + 2 } else { i + 1 };
        let Some(tag_end_rel) =
            svg[tag_start..].find(|c: char| c == '>' || c.is_whitespace() || c == '/')
        else {
            break;
        };
        let tag = &svg[tag_start..tag_start + tag_end_rel];

        // Find end of tag.
        let Some(gt_rel) = svg[tag_start + tag_end_rel..].find('>') else {
            break;
        };
        let gt = tag_start + tag_end_rel + gt_rel;
        let raw = &svg[i..=gt];
        let self_closing = raw.ends_with("/>");

        if close {
            match tag {
                "defs" | "marker" | "symbol" | "clipPath" | "mask" | "pattern"
                | "linearGradient" | "radialGradient" => {
                    defs_depth = defs_depth.saturating_sub(1);
                }
                "g" | "a" => {
                    if let Some(len) = tf_stack.pop() {
                        cur_ops.truncate(len);
                    } else {
                        cur_ops.clear();
                    }
                }
                "svg" if nested_svg_depth > 0 => {
                    nested_svg_depth -= 1;
                    if let Some(len) = tf_stack.pop() {
                        cur_ops.truncate(len);
                    } else {
                        cur_ops.clear();
                    }
                }
                _ => {}
            }
            i = gt + 1;
            continue;
        }

        // Attributes substring (excluding `<tag` and trailing `>`/`/>`).
        let attrs_start = tag_start + tag_end_rel;
        let attrs_end = if self_closing {
            gt.saturating_sub(1)
        } else {
            gt
        };
        let attrs = if attrs_start < attrs_end {
            &svg[attrs_start..attrs_end]
        } else {
            ""
        };

        if matches!(
            tag,
            "defs"
                | "marker"
                | "symbol"
                | "clipPath"
                | "mask"
                | "pattern"
                | "linearGradient"
                | "radialGradient"
        ) {
            defs_depth += 1;
        }

        el_ops_buf.clear();
        if let Some(transform) = attr_value(attrs, "transform") {
            parse_transform_ops_into(transform, &mut el_ops_buf);
        }
        let el_ops: &[AffineTransform] = &el_ops_buf;

        if tag == "g" || tag == "a" {
            tf_stack.push(cur_ops.len());
            cur_ops.extend_from_slice(el_ops);
            if self_closing {
                // Balance a self-closing group.
                if let Some(len) = tf_stack.pop() {
                    cur_ops.truncate(len);
                } else {
                    cur_ops.clear();
                }
            }
            i = gt + 1;
            continue;
        }

        if tag == "svg" {
            if !seen_root_svg {
                // Root <svg> defines the user coordinate system we are already parsing in; do not
                // apply its viewBox mapping again.
                seen_root_svg = true;
            } else {
                tf_stack.push(cur_ops.len());
                nested_svg_depth += 1;
                let vp_tf = svg_viewport_transform(attrs);
                cur_ops.extend_from_slice(el_ops);
                cur_ops.push(vp_tf);
                if self_closing {
                    nested_svg_depth = nested_svg_depth.saturating_sub(1);
                    if let Some(len) = tf_stack.pop() {
                        cur_ops.truncate(len);
                    } else {
                        cur_ops.clear();
                    }
                }
            }
            i = gt + 1;
            continue;
        }

        if defs_depth == 0 {
            match tag {
                "rect" => {
                    let x = attr_value(attrs, "x").and_then(parse_f64).unwrap_or(0.0);
                    let y = attr_value(attrs, "y").and_then(parse_f64).unwrap_or(0.0);
                    let w = attr_value(attrs, "width")
                        .and_then(parse_f64)
                        .unwrap_or(0.0);
                    let h = attr_value(attrs, "height")
                        .and_then(parse_f64)
                        .unwrap_or(0.0);
                    let b = apply_ops_bounds(
                        &cur_ops,
                        el_ops,
                        Bounds {
                            min_x: x,
                            min_y: y,
                            max_x: x + w,
                            max_y: y + h,
                        },
                    );
                    if w != 0.0 || h != 0.0 {
                        maybe_record_dbg(&mut dbg, tag, attrs, b.clone());
                    }
                    bounds.include(b);
                }
                "circle" => {
                    let cx = attr_value(attrs, "cx").and_then(parse_f64).unwrap_or(0.0);
                    let cy = attr_value(attrs, "cy").and_then(parse_f64).unwrap_or(0.0);
                    let r = attr_value(attrs, "r").and_then(parse_f64).unwrap_or(0.0);
                    let b = apply_ops_bounds(
                        &cur_ops,
                        el_ops,
                        Bounds {
                            min_x: cx - r,
                            min_y: cy - r,
                            max_x: cx + r,
                            max_y: cy + r,
                        },
                    );
                    if r != 0.0 {
                        maybe_record_dbg(&mut dbg, tag, attrs, b.clone());
                    }
                    bounds.include(b);
                }
                "ellipse" => {
                    let cx = attr_value(attrs, "cx").and_then(parse_f64).unwrap_or(0.0);
                    let cy = attr_value(attrs, "cy").and_then(parse_f64).unwrap_or(0.0);
                    let rx = attr_value(attrs, "rx").and_then(parse_f64).unwrap_or(0.0);
                    let ry = attr_value(attrs, "ry").and_then(parse_f64).unwrap_or(0.0);
                    let b = apply_ops_bounds(
                        &cur_ops,
                        el_ops,
                        Bounds {
                            min_x: cx - rx,
                            min_y: cy - ry,
                            max_x: cx + rx,
                            max_y: cy + ry,
                        },
                    );
                    if rx != 0.0 || ry != 0.0 {
                        maybe_record_dbg(&mut dbg, tag, attrs, b.clone());
                    }
                    bounds.include(b);
                }
                "line" => {
                    let x1 = attr_value(attrs, "x1").and_then(parse_f64).unwrap_or(0.0);
                    let y1 = attr_value(attrs, "y1").and_then(parse_f64).unwrap_or(0.0);
                    let x2 = attr_value(attrs, "x2").and_then(parse_f64).unwrap_or(0.0);
                    let y2 = attr_value(attrs, "y2").and_then(parse_f64).unwrap_or(0.0);
                    let (tx1, ty1) = apply_ops_point(&cur_ops, el_ops, x1, y1);
                    let (tx2, ty2) = apply_ops_point(&cur_ops, el_ops, x2, y2);
                    let b = Bounds {
                        min_x: tx1.min(tx2),
                        min_y: ty1.min(ty2),
                        max_x: tx1.max(tx2),
                        max_y: ty1.max(ty2),
                    };
                    maybe_record_dbg(&mut dbg, tag, attrs, b.clone());
                    bounds.include(b);
                }
                "path" => {
                    if let Some(d) = attr_value(attrs, "d")
                        && let Some(pb) = svg_path_bounds_from_d(d)
                    {
                        let b0 = apply_ops_bounds(
                            &cur_ops,
                            el_ops,
                            Bounds {
                                min_x: pb.min_x,
                                min_y: pb.min_y,
                                max_x: pb.max_x,
                                max_y: pb.max_y,
                            },
                        );
                        maybe_record_dbg(&mut dbg, tag, attrs, b0.clone());
                        bounds.include(b0);
                    }
                }
                "polygon" | "polyline" => {
                    if let Some(pts) = attr_value(attrs, "points") {
                        include_points(&mut bounds, pts, &cur_ops, el_ops);
                    }
                }
                "foreignObject" => {
                    let x = attr_value(attrs, "x").and_then(parse_f64).unwrap_or(0.0);
                    let y = attr_value(attrs, "y").and_then(parse_f64).unwrap_or(0.0);
                    let w = attr_value(attrs, "width")
                        .and_then(parse_f64)
                        .unwrap_or(0.0);
                    let h = attr_value(attrs, "height")
                        .and_then(parse_f64)
                        .unwrap_or(0.0);
                    let b = apply_ops_bounds(
                        &cur_ops,
                        el_ops,
                        Bounds {
                            min_x: x,
                            min_y: y,
                            max_x: x + w,
                            max_y: y + h,
                        },
                    );
                    if w != 0.0 || h != 0.0 {
                        maybe_record_dbg(&mut dbg, tag, attrs, b.clone());
                    }
                    bounds.include(b);
                }
                _ => {}
            }
        }

        i = gt + 1;
    }

    bounds.finish()
}

#[cfg(test)]
mod svg_bbox_tests {
    use super::*;

    fn parse_root_viewbox(svg: &str) -> Option<(f64, f64, f64, f64)> {
        let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
        let rest = &svg[start..];
        let end = rest.find('"')?;
        let raw = &rest[..end];
        let nums: Vec<f64> = raw
            .split_whitespace()
            .filter_map(|v| v.parse::<f64>().ok())
            .collect();
        if nums.len() != 4 {
            return None;
        }
        Some((nums[0], nums[1], nums[2], nums[3]))
    }

    #[test]
    fn svg_bbox_matches_upstream_state_concurrent_viewbox() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../fixtures/upstream-svgs/state/upstream_stateDiagram_concurrent_state_spec.svg",
        );
        let svg = std::fs::read_to_string(p).expect("read upstream state svg");

        let (vb_x, vb_y, vb_w, vb_h) = parse_root_viewbox(&svg).expect("parse viewBox");
        let b = svg_emitted_bounds_from_svg(&svg).expect("bbox");

        let pad = 8.0;
        let got_x = b.min_x - pad;
        let got_y = b.min_y - pad;
        let got_w = (b.max_x - b.min_x) + 2.0 * pad;
        let got_h = (b.max_y - b.min_y) + 2.0 * pad;

        fn close(a: f64, b: f64) -> bool {
            (a - b).abs() <= 1e-6
        }

        assert!(close(got_x, vb_x), "viewBox x: got {got_x}, want {vb_x}");
        assert!(close(got_y, vb_y), "viewBox y: got {got_y}, want {vb_y}");
        assert!(close(got_w, vb_w), "viewBox w: got {got_w}, want {vb_w}");
        assert!(close(got_h, vb_h), "viewBox h: got {got_h}, want {vb_h}");
    }

    #[test]
    fn svg_path_bounds_architecture_service_node_bkg_matches_mermaid_bbox() {
        // Mermaid architecture service fallback background path (no icon / no iconText), as of
        // Mermaid 11.15:
        // `M0,${iconSize} V5 Q0,0 5,0 H${iconSize - 5} Q${iconSize},0 ${iconSize},5 V${iconSize} Z`
        //
        // With iconSize=80, Chromium getBBox() yields:
        //   x=0, y=0, width=80, height=80
        let d = "M0,80 V5 Q0,0 5,0 H75 Q80,0 80,5 V80 Z";
        let b = svg_path_bounds_from_d(d).expect("path bounds");
        assert!((b.min_x - 0.0).abs() < 1e-9, "min_x: got {}", b.min_x);
        assert!((b.min_y - 0.0).abs() < 1e-9, "min_y: got {}", b.min_y);
        assert!((b.max_x - 80.0).abs() < 1e-9, "max_x: got {}", b.max_x);
        assert!((b.max_y - 80.0).abs() < 1e-9, "max_y: got {}", b.max_y);
    }

    #[test]
    fn svg_emitted_bounds_attr_lookup_d_does_not_match_id() {
        // Regression test: naive attribute lookup for `d="..."` can match inside `id="..."`.
        // That would cause `<path>` bboxes to be skipped, breaking root viewBox/max-width parity.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><path class="node-bkg" id="node-db" d="M0,80 V5 Q0,0 5,0 H75 Q80,0 80,5 V80 Z"/></svg>"#;
        let dbg = debug_svg_emitted_bounds(svg).expect("emitted bounds");
        assert!((dbg.bounds.min_x - 0.0).abs() < 1e-9);
        assert!((dbg.bounds.min_y - 0.0).abs() < 1e-9);
        assert!((dbg.bounds.max_x - 80.0).abs() < 1e-9);
        assert!((dbg.bounds.max_y - 80.0).abs() < 1e-9);
    }

    #[test]
    fn svg_emitted_bounds_supports_rotate_transform() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect x="0" y="0" width="10" height="20" transform="rotate(90)"/></svg>"#;
        let dbg = debug_svg_emitted_bounds(svg).expect("emitted bounds");
        assert!(
            (dbg.bounds.min_x - (-20.0)).abs() < 1e-9,
            "min_x: {}",
            dbg.bounds.min_x
        );
        assert!(
            (dbg.bounds.min_y - 0.0).abs() < 1e-9,
            "min_y: {}",
            dbg.bounds.min_y
        );
        assert!(
            (dbg.bounds.max_x - 0.0).abs() < 1e-9,
            "max_x: {}",
            dbg.bounds.max_x
        );
        assert!(
            (dbg.bounds.max_y - 10.0).abs() < 1e-9,
            "max_y: {}",
            dbg.bounds.max_y
        );
    }

    #[test]
    fn svg_emitted_bounds_supports_rotate_about_center() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect x="0" y="0" width="10" height="20" transform="rotate(90, 5, 10)"/></svg>"#;
        let dbg = debug_svg_emitted_bounds(svg).expect("emitted bounds");
        assert!(
            (dbg.bounds.min_x - (-5.0)).abs() < 1e-9,
            "min_x: {}",
            dbg.bounds.min_x
        );
        assert!(
            (dbg.bounds.min_y - 5.0).abs() < 1e-9,
            "min_y: {}",
            dbg.bounds.min_y
        );
        assert!(
            (dbg.bounds.max_x - 15.0).abs() < 1e-9,
            "max_x: {}",
            dbg.bounds.max_x
        );
        assert!(
            (dbg.bounds.max_y - 15.0).abs() < 1e-9,
            "max_y: {}",
            dbg.bounds.max_y
        );
    }

    #[test]
    fn svg_emitted_bounds_preserves_f64_affine_precision() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect x="0.1" y="0.2" width="1.3" height="2.4" transform="translate(0.2, 0.3)"/></svg>"#;
        let bounds = svg_emitted_bounds_from_svg(svg).expect("emitted bounds");

        assert!(
            (bounds.min_x - 0.3).abs() < 1e-12,
            "min_x: {}",
            bounds.min_x
        );
        assert!(
            (bounds.min_y - 0.5).abs() < 1e-12,
            "min_y: {}",
            bounds.min_y
        );
        assert!(
            (bounds.max_x - 1.6).abs() < 1e-12,
            "max_x: {}",
            bounds.max_x
        );
        assert!(
            (bounds.max_y - 2.9).abs() < 1e-12,
            "max_y: {}",
            bounds.max_y
        );
    }

    #[test]
    fn svg_emitted_bounds_preserves_transform_list_and_group_order() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(10, 20)"><rect x="1" y="2" width="3" height="4" transform="scale(2, 3) translate(5, 7)"/></g></svg>"#;
        let bounds = svg_emitted_bounds_from_svg(svg).expect("emitted bounds");

        assert!(
            (bounds.min_x - 22.0).abs() < 1e-12,
            "min_x: {}",
            bounds.min_x
        );
        assert!(
            (bounds.min_y - 47.0).abs() < 1e-12,
            "min_y: {}",
            bounds.min_y
        );
        assert!(
            (bounds.max_x - 28.0).abs() < 1e-12,
            "max_x: {}",
            bounds.max_x
        );
        assert!(
            (bounds.max_y - 59.0).abs() < 1e-12,
            "max_y: {}",
            bounds.max_y
        );
    }

    #[test]
    fn svg_emitted_bounds_maps_nested_svg_viewport() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(3, 4)"><svg x="10" y="20" width="200" height="100" viewBox="0 0 100 50"><rect x="5" y="6" width="10" height="8"/></svg></g></svg>"#;
        let bounds = svg_emitted_bounds_from_svg(svg).expect("emitted bounds");

        assert!(
            (bounds.min_x - 23.0).abs() < 1e-12,
            "min_x: {}",
            bounds.min_x
        );
        assert!(
            (bounds.min_y - 36.0).abs() < 1e-12,
            "min_y: {}",
            bounds.min_y
        );
        assert!(
            (bounds.max_x - 43.0).abs() < 1e-12,
            "max_x: {}",
            bounds.max_x
        );
        assert!(
            (bounds.max_y - 52.0).abs() < 1e-12,
            "max_y: {}",
            bounds.max_y
        );
    }
}

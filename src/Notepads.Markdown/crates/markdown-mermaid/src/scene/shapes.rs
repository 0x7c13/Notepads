// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Rounded,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Diamond,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    Asymmetric,
}
impl Shape {
    pub fn parse(value: Option<&str>) -> Result<Self, CanonicalError> {
        Ok(match value.unwrap_or("rect") {
            "rect" | "rectangle" | "square" | "squareRect" => Self::Rectangle,
            "rounded" | "round" | "roundedRect" => Self::Rounded,
            "stadium" | "pill" | "terminal" => Self::Stadium,
            "subroutine" | "fr-rect" | "framed-rectangle" => Self::Subroutine,
            "cylinder" | "cyl" | "database" | "db" => Self::Cylinder,
            "circle" | "circ" => Self::Circle,
            "doublecircle" | "double-circle" | "dbl-circ" => Self::DoubleCircle,
            "diamond" | "diam" | "decision" => Self::Diamond,
            "hexagon" | "hex" | "prepare" => Self::Hexagon,
            "lean_right" | "lean-r" | "parallelogram" | "in-out" => Self::Parallelogram,
            "lean_left" | "lean-l" | "parallelogram-alt" | "out-in" => Self::ParallelogramAlt,
            "trapezoid" | "trap-b" | "priority" => Self::Trapezoid,
            "inv_trapezoid" | "trapezoid-alt" | "trap-t" | "manual" => Self::TrapezoidAlt,
            "odd" | "asymmetric" | "notch-rect" => Self::Asymmetric,
            _ => return Err(CanonicalError::UnsupportedElement),
        })
    }
    pub fn dimensions(self, text_width: f64, text_height: f64, padding: f64) -> (f64, f64) {
        let (mut w, mut h) = (
            (text_width + 2.0 * padding).max(40.0),
            (text_height + 2.0 * padding).max(32.0),
        );
        match self {
            Self::Diamond => {
                w *= 1.65;
                h *= 1.65;
            }
            Self::Circle | Self::DoubleCircle => {
                w = (w * w + h * h).sqrt();
                h = w;
            }
            Self::Hexagon
            | Self::Parallelogram
            | Self::ParallelogramAlt
            | Self::Trapezoid
            | Self::TrapezoidAlt
            | Self::Asymmetric => w += h * 0.6,
            Self::Cylinder => h += 16.0,
            Self::Subroutine => w += 20.0,
            _ => {}
        }
        (w, h)
    }
}

fn polygon(points: &[Point], sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
    if let Some(first) = points.first() {
        sink.push(PathCommand::move_to(first.x, first.y))?;
        for p in &points[1..] {
            sink.push(PathCommand::line_to(p.x, p.y))?;
        }
        sink.push(PathCommand::close())?;
    }
    Ok(())
}
fn ellipse(r: Rect, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
    let (cx, cy, rx, ry) = (
        (r.left + r.right) / 2.0,
        (r.top + r.bottom) / 2.0,
        (r.right - r.left) / 2.0,
        (r.bottom - r.top) / 2.0,
    );
    let k = 0.5522847498307936;
    sink.push(PathCommand::move_to(cx + rx, cy))?;
    sink.push(PathCommand::cubic(
        cx + rx,
        cy + k * ry,
        cx + k * rx,
        cy + ry,
        cx,
        cy + ry,
    ))?;
    sink.push(PathCommand::cubic(
        cx - k * rx,
        cy + ry,
        cx - rx,
        cy + k * ry,
        cx - rx,
        cy,
    ))?;
    sink.push(PathCommand::cubic(
        cx - rx,
        cy - k * ry,
        cx - k * rx,
        cy - ry,
        cx,
        cy - ry,
    ))?;
    sink.push(PathCommand::cubic(
        cx + k * rx,
        cy - ry,
        cx + rx,
        cy - k * ry,
        cx + rx,
        cy,
    ))?;
    sink.push(PathCommand::close())
}

pub fn outline(shape: Shape, r: Rect, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
    outer(shape, r, sink)?;
    if shape == Shape::DoubleCircle {
        ellipse(
            Rect {
                left: r.left + 5.0,
                top: r.top + 5.0,
                right: r.right - 5.0,
                bottom: r.bottom - 5.0,
            },
            sink,
        )?;
    } else if shape == Shape::Subroutine {
        for x in [r.left + 9.0, r.right - 9.0] {
            sink.push(PathCommand::move_to(x, r.top))?;
            sink.push(PathCommand::line_to(x, r.bottom))?;
        }
    } else if shape == Shape::Cylinder {
        let ry = ((r.bottom - r.top) / 8.0).min(10.0);
        let (cx, rx) = ((r.left + r.right) / 2.0, (r.right - r.left) / 2.0);
        sink.push(PathCommand::move_to(r.left, r.top + ry))?;
        sink.push(PathCommand::cubic(
            r.left,
            r.top + 2.0 * ry,
            cx - rx / 2.0,
            r.top + 2.0 * ry,
            cx,
            r.top + 2.0 * ry,
        ))?;
        sink.push(PathCommand::cubic(
            cx + rx / 2.0,
            r.top + 2.0 * ry,
            r.right,
            r.top + 2.0 * ry,
            r.right,
            r.top + ry,
        ))?;
    }
    Ok(())
}

fn outer(shape: Shape, r: Rect, sink: &mut dyn PathSink) -> Result<(), CanonicalError> {
    let (l, t, rr, b) = (r.left, r.top, r.right, r.bottom);
    let (w, h, cx, cy) = (rr - l, b - t, (l + rr) / 2.0, (t + b) / 2.0);
    if ![l, t, rr, b].into_iter().all(valid) || w <= 0.0 || h <= 0.0 {
        return Err(CanonicalError::InvalidGeometry);
    }
    let p = |x, y| Point { x, y };
    match shape {
        Shape::Circle | Shape::DoubleCircle => ellipse(r, sink),
        Shape::Rounded | Shape::Stadium => {
            let radius = if shape == Shape::Stadium {
                h / 2.0
            } else {
                8.0f64.min(h / 2.0).min(w / 2.0)
            };
            let radius = radius.min(w / 2.0);
            let k = 0.5522847498307936;
            sink.push(PathCommand::move_to(l + radius, t))?;
            sink.push(PathCommand::line_to(rr - radius, t))?;
            sink.push(PathCommand::cubic(
                rr - radius + k * radius,
                t,
                rr,
                t + radius - k * radius,
                rr,
                t + radius,
            ))?;
            sink.push(PathCommand::line_to(rr, b - radius))?;
            sink.push(PathCommand::cubic(
                rr,
                b - radius + k * radius,
                rr - radius + k * radius,
                b,
                rr - radius,
                b,
            ))?;
            sink.push(PathCommand::line_to(l + radius, b))?;
            sink.push(PathCommand::cubic(
                l + radius - k * radius,
                b,
                l,
                b - radius + k * radius,
                l,
                b - radius,
            ))?;
            sink.push(PathCommand::line_to(l, t + radius))?;
            sink.push(PathCommand::cubic(
                l,
                t + radius - k * radius,
                l + radius - k * radius,
                t,
                l + radius,
                t,
            ))?;
            sink.push(PathCommand::close())
        }
        Shape::Cylinder => {
            let ry = (h / 8.0).min(10.0);
            sink.push(PathCommand::move_to(l, t + ry))?;
            sink.push(PathCommand::cubic(
                l,
                t - ry / 3.0,
                rr,
                t - ry / 3.0,
                rr,
                t + ry,
            ))?;
            sink.push(PathCommand::line_to(rr, b - ry))?;
            sink.push(PathCommand::cubic(
                rr,
                b + ry / 3.0,
                l,
                b + ry / 3.0,
                l,
                b - ry,
            ))?;
            sink.push(PathCommand::close())
        }
        Shape::Diamond => polygon(&[p(cx, t), p(rr, cy), p(cx, b), p(l, cy)], sink),
        Shape::Hexagon => polygon(
            &[
                p(l + h / 4.0, t),
                p(rr - h / 4.0, t),
                p(rr, cy),
                p(rr - h / 4.0, b),
                p(l + h / 4.0, b),
                p(l, cy),
            ],
            sink,
        ),
        Shape::Parallelogram => polygon(
            &[p(l + h / 4.0, t), p(rr, t), p(rr - h / 4.0, b), p(l, b)],
            sink,
        ),
        Shape::ParallelogramAlt => polygon(
            &[p(l, t), p(rr - h / 4.0, t), p(rr, b), p(l + h / 4.0, b)],
            sink,
        ),
        Shape::Trapezoid => polygon(
            &[p(l + h / 4.0, t), p(rr - h / 4.0, t), p(rr, b), p(l, b)],
            sink,
        ),
        Shape::TrapezoidAlt => polygon(
            &[p(l, t), p(rr, t), p(rr - h / 4.0, b), p(l + h / 4.0, b)],
            sink,
        ),
        Shape::Asymmetric => polygon(
            &[
                p(l + h / 4.0, t),
                p(rr, t),
                p(rr, b),
                p(l + h / 4.0, b),
                p(l, cy),
            ],
            sink,
        ),
        _ => polygon(&[p(l, t), p(rr, t), p(rr, b), p(l, b)], sink),
    }
}

/// Intersects a center ray with the exact emitted polygon/cubic contour.
/// Cubics use a fixed, bounded 32-segment flattening; no heap or recursive walk.
pub fn port(shape: Shape, rect: Rect, toward: Point) -> Result<Point, CanonicalError> {
    let center = Point {
        x: (rect.left + rect.right) / 2.0,
        y: (rect.top + rect.bottom) / 2.0,
    };
    let d = Point {
        x: toward.x - center.x,
        y: toward.y - center.y,
    };
    if d.x.abs() + d.y.abs() < 1e-9 {
        return Err(CanonicalError::InvalidGeometry);
    }
    struct Intersector {
        center: Point,
        d: Point,
        first: Point,
        last: Point,
        best: f64,
    }
    impl Intersector {
        fn line(&mut self, p: Point) {
            let e = Point {
                x: p.x - self.last.x,
                y: p.y - self.last.y,
            };
            let cross = self.d.x * e.y - self.d.y * e.x;
            if cross.abs() > 1e-12 {
                let q = Point {
                    x: self.last.x - self.center.x,
                    y: self.last.y - self.center.y,
                };
                let t = (q.x * e.y - q.y * e.x) / cross;
                let u = (q.x * self.d.y - q.y * self.d.x) / cross;
                if t >= 0.0 && (0.0..=1.0).contains(&u) {
                    self.best = self.best.min(t);
                }
            }
            self.last = p;
        }
    }
    impl PathSink for Intersector {
        fn push(&mut self, c: PathCommand) -> Result<(), CanonicalError> {
            match c.verb {
                PathVerb::Move => {
                    self.first = Point { x: c.x1, y: c.y1 };
                    self.last = self.first;
                }
                PathVerb::Line => self.line(Point { x: c.x1, y: c.y1 }),
                PathVerb::Cubic => {
                    let start = self.last;
                    for i in 1..=32 {
                        let t = i as f64 / 32.0;
                        let u = 1.0 - t;
                        self.line(Point {
                            x: u * u * u * start.x
                                + 3.0 * u * u * t * c.x1
                                + 3.0 * u * t * t * c.x2
                                + t * t * t * c.x3,
                            y: u * u * u * start.y
                                + 3.0 * u * u * t * c.y1
                                + 3.0 * u * t * t * c.y2
                                + t * t * t * c.y3,
                        });
                    }
                }
                PathVerb::Close => self.line(self.first),
                PathVerb::Quad => return Err(CanonicalError::InvalidGeometry),
            }
            Ok(())
        }
    }
    let mut intersection = Intersector {
        center,
        d,
        first: center,
        last: center,
        best: f64::INFINITY,
    };
    outer(shape, rect, &mut intersection)?;
    if !intersection.best.is_finite() {
        return Err(CanonicalError::InvalidGeometry);
    }
    Ok(Point {
        x: center.x + d.x * intersection.best,
        y: center.y + d.y * intersection.best,
    })
}

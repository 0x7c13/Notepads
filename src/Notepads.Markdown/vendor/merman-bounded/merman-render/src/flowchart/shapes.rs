use crate::{Error, Result};
use kurbo::{Arc, Point, SvgArc, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowchartShape {
    Anchor,
    Bang,
    Browser,
    BowTieRectangle,
    BraceLeft,
    BraceRight,
    Braces,
    Bucket,
    Choice,
    Circle,
    Cloud,
    CollapsedGroup,
    Console,
    CrossedCircle,
    CurvedTrapezoid,
    Cylinder,
    Datastore,
    Delay,
    Diamond,
    Folder,
    DividedRectangle,
    Document,
    DoubleCircle,
    Ellipse,
    FilledCircle,
    ForkJoin,
    FramedCircle,
    Hexagon,
    HorizontalCylinder,
    Hourglass,
    Icon,
    IconCircle,
    IconRounded,
    IconSquare,
    ImageSquare,
    InvertedTrapezoid,
    LeanLeft,
    LeanRight,
    LightningBolt,
    LinedCylinder,
    LinedDocument,
    ManualFile,
    ManualInput,
    NotchedPentagon,
    NotchedRectangle,
    Note,
    Odd,
    PaperTape,
    Person,
    Process,
    RoundedRectangle,
    ShadedProcess,
    SmallCircle,
    StackedDocument,
    StackedRectangle,
    Stadium,
    Subroutine,
    TaggedDocument,
    TaggedRectangle,
    Text,
    Trapezoid,
    Triangle,
    WindowPane,
}

impl FlowchartShape {
    pub(crate) fn resolve(name: &str) -> Result<Self> {
        let shape = match name {
            "anchor" => Self::Anchor,
            "bang" => Self::Bang,
            "browser" => Self::Browser,
            "bow-rect" | "stored-data" | "bow-tie-rectangle" => Self::BowTieRectangle,
            "comment" | "brace" | "brace-l" => Self::BraceLeft,
            "brace-r" => Self::BraceRight,
            "braces" => Self::Braces,
            "bucket" => Self::Bucket,
            "choice" => Self::Choice,
            "circle" | "circ" => Self::Circle,
            "cloud" => Self::Cloud,
            "collapsedGroup" | "collapsed-group" => Self::CollapsedGroup,
            "console" => Self::Console,
            "cross-circ" | "summary" | "crossed-circle" => Self::CrossedCircle,
            "curv-trap" | "display" | "curved-trapezoid" => Self::CurvedTrapezoid,
            "cylinder" | "cyl" | "db" | "database" => Self::Cylinder,
            "datastore" | "data-store" => Self::Datastore,
            "delay" | "half-rounded-rectangle" => Self::Delay,
            "diamond" | "question" | "diam" | "decision" => Self::Diamond,
            "folder" | "directory" => Self::Folder,
            "div-rect" | "div-proc" | "divided-rectangle" | "divided-process" => {
                Self::DividedRectangle
            }
            "doc" | "document" => Self::Document,
            "doublecircle" | "dbl-circ" | "double-circle" => Self::DoubleCircle,
            "ellipse" => Self::Ellipse,
            "f-circ" | "junction" | "filled-circle" => Self::FilledCircle,
            "fork" | "join" => Self::ForkJoin,
            "fr-circ" | "framed-circle" | "stop" => Self::FramedCircle,
            "hexagon" | "hex" | "prepare" => Self::Hexagon,
            "h-cyl" | "das" | "horizontal-cylinder" => Self::HorizontalCylinder,
            "hourglass" | "collate" => Self::Hourglass,
            "icon" => Self::Icon,
            "iconCircle" => Self::IconCircle,
            "iconRounded" => Self::IconRounded,
            "iconSquare" => Self::IconSquare,
            "imageSquare" => Self::ImageSquare,
            "inv_trapezoid" | "inv-trapezoid" | "trap-t" | "manual" | "trapezoid-top" => {
                Self::InvertedTrapezoid
            }
            "lean_left" | "lean-l" | "lean-left" | "out-in" => Self::LeanLeft,
            "lean_right" | "lean-r" | "lean-right" | "in-out" => Self::LeanRight,
            "bolt" | "com-link" | "lightning-bolt" => Self::LightningBolt,
            "lin-cyl" | "disk" | "lined-cylinder" => Self::LinedCylinder,
            "lin-doc" | "lined-document" => Self::LinedDocument,
            "manual-file" | "flipped-triangle" | "flip-tri" => Self::ManualFile,
            "manual-input" | "sloped-rectangle" | "sl-rect" => Self::ManualInput,
            "notch-pent" | "loop-limit" | "notched-pentagon" => Self::NotchedPentagon,
            "notch-rect" | "notched-rectangle" | "card" => Self::NotchedRectangle,
            "note" => Self::Note,
            "odd" | "rect_left_inv_arrow" => Self::Odd,
            "paper-tape" | "flag" => Self::PaperTape,
            "person" => Self::Person,
            "squareRect" | "rect" | "proc" | "process" | "rectangle" => Self::Process,
            "roundedRect" | "rounded" | "event" => Self::RoundedRectangle,
            "lin-rect" | "lined-rectangle" | "lined-process" | "lin-proc" | "shaded-process" => {
                Self::ShadedProcess
            }
            "sm-circ" | "small-circle" | "start" => Self::SmallCircle,
            "docs" | "documents" | "st-doc" | "stacked-document" => Self::StackedDocument,
            "st-rect" | "procs" | "processes" | "stacked-rectangle" => Self::StackedRectangle,
            "stadium" | "terminal" | "pill" => Self::Stadium,
            "subroutine" | "fr-rect" | "subproc" | "subprocess" | "framed-rectangle" => {
                Self::Subroutine
            }
            "tag-doc" | "tagged-document" => Self::TaggedDocument,
            "tag-rect" | "tagged-rectangle" | "tag-proc" | "tagged-process" => {
                Self::TaggedRectangle
            }
            "text" => Self::Text,
            "trapezoid" | "trap-b" | "priority" | "trapezoid-bottom" => Self::Trapezoid,
            "tri" | "extract" | "triangle" => Self::Triangle,
            "win-pane" | "internal-storage" | "window-pane" => Self::WindowPane,
            _ => {
                return Err(Error::InvalidModel {
                    message: format!("No such shape: {name}. Please check your syntax."),
                });
            }
        };
        Ok(shape)
    }
}

pub(crate) fn validate_flowchart_model_shapes(
    model: &merman_core::diagrams::flowchart::FlowchartModel,
) -> Result<()> {
    for node in &model.nodes {
        FlowchartShape::resolve(node.layout_shape.as_deref().unwrap_or("squareRect"))?;
    }
    Ok(())
}

pub(crate) fn is_flowchart_process_shape(name: &str) -> bool {
    matches!(FlowchartShape::resolve(name), Ok(FlowchartShape::Process))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RelativeArc {
    pub(crate) rx: f64,
    pub(crate) ry: f64,
    pub(crate) x_axis_rotation_deg: f64,
    pub(crate) large_arc: bool,
    pub(crate) sweep: bool,
    pub(crate) dx: f64,
    pub(crate) dy: f64,
}

#[derive(Debug, Clone, Copy)]
struct PathBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl PathBounds {
    fn origin() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }
    }

    fn include(&mut self, point: Point) {
        self.min_x = self.min_x.min(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_x = self.max_x.max(point.x);
        self.max_y = self.max_y.max(point.y);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OrganicShapeGeometry {
    pub(crate) arcs: Vec<RelativeArc>,
    pub(crate) translate_x: f64,
    pub(crate) translate_y: f64,
    bounds: PathBounds,
}

impl OrganicShapeGeometry {
    pub(crate) fn width(&self) -> f64 {
        (self.bounds.max_x - self.bounds.min_x).max(0.0)
    }

    pub(crate) fn height(&self) -> f64 {
        (self.bounds.max_y - self.bounds.min_y).max(0.0)
    }

    pub(crate) fn rendered_min_x(&self) -> f64 {
        self.bounds.min_x + self.translate_x
    }

    pub(crate) fn rendered_min_y(&self) -> f64 {
        self.bounds.min_y + self.translate_y
    }

    pub(crate) fn rendered_max_x(&self) -> f64 {
        self.bounds.max_x + self.translate_x
    }

    pub(crate) fn rendered_max_y(&self) -> f64 {
        self.bounds.max_y + self.translate_y
    }
}

fn normalize_angle(mut angle: f64) -> f64 {
    let turn = 2.0 * std::f64::consts::PI;
    angle %= turn;
    if angle < 0.0 {
        angle += turn;
    }
    angle
}

fn angle_is_on_arc(angle: f64, start: f64, sweep: f64) -> bool {
    let turn = 2.0 * std::f64::consts::PI;
    let offset = normalize_angle(angle - start);
    if sweep >= 0.0 {
        offset <= sweep + 1.0e-12
    } else {
        offset >= turn + sweep - 1.0e-12
    }
}

fn include_arc_bounds(bounds: &mut PathBounds, svg_arc: &SvgArc) {
    bounds.include(svg_arc.from);
    bounds.include(svg_arc.to);
    let Some(arc) = Arc::from_svg_arc(svg_arc) else {
        return;
    };

    let point_at = |angle: f64| {
        let (sin_angle, cos_angle) = angle.sin_cos();
        let (sin_rotation, cos_rotation) = arc.x_rotation.sin_cos();
        Point::new(
            arc.center.x + arc.radii.x * cos_angle * cos_rotation
                - arc.radii.y * sin_angle * sin_rotation,
            arc.center.y
                + arc.radii.x * cos_angle * sin_rotation
                + arc.radii.y * sin_angle * cos_rotation,
        )
    };

    bounds.include(point_at(arc.start_angle));
    bounds.include(point_at(arc.start_angle + arc.sweep_angle));

    let (sin_rotation, cos_rotation) = arc.x_rotation.sin_cos();
    let x_extreme = (-arc.radii.y * sin_rotation).atan2(arc.radii.x * cos_rotation);
    let y_extreme = (arc.radii.y * cos_rotation).atan2(arc.radii.x * sin_rotation);
    for angle in [
        x_extreme,
        x_extreme + std::f64::consts::PI,
        y_extreme,
        y_extreme + std::f64::consts::PI,
    ] {
        if angle_is_on_arc(angle, arc.start_angle, arc.sweep_angle) {
            bounds.include(point_at(angle));
        }
    }
}

fn organic_geometry(
    arcs: Vec<RelativeArc>,
    translate_x: f64,
    translate_y: f64,
) -> OrganicShapeGeometry {
    let mut bounds = PathBounds::origin();
    let mut current = Point::ORIGIN;
    for segment in &arcs {
        let next = Point::new(current.x + segment.dx, current.y + segment.dy);
        include_arc_bounds(
            &mut bounds,
            &SvgArc {
                from: current,
                to: next,
                radii: Vec2::new(segment.rx, segment.ry),
                x_rotation: segment.x_axis_rotation_deg.to_radians(),
                large_arc: segment.large_arc,
                sweep: segment.sweep,
            },
        );
        current = next;
    }
    bounds.include(Point::ORIGIN);

    OrganicShapeGeometry {
        arcs,
        translate_x,
        translate_y,
        bounds,
    }
}

pub(crate) fn bang_geometry(
    label_width: f64,
    label_height: f64,
    padding: f64,
) -> OrganicShapeGeometry {
    let half_padding = padding.max(0.0) / 2.0;
    let width = label_width.max(0.0) + 10.0 * half_padding;
    let height = label_height.max(0.0) + 8.0 * half_padding;
    let radius = 0.15 * width;
    let effective_width = width.max(label_width.max(0.0) + 20.0);
    let effective_height = height.max(label_height.max(0.0) + 20.0);
    let arc = |scale: f64, dx: f64, dy: f64| RelativeArc {
        rx: radius * scale,
        ry: radius * scale,
        x_axis_rotation_deg: 1.0,
        large_arc: false,
        sweep: false,
        dx,
        dy,
    };

    organic_geometry(
        vec![
            arc(1.0, effective_width * 0.25, -effective_height * 0.1),
            arc(1.0, effective_width * 0.25, 0.0),
            arc(1.0, effective_width * 0.25, 0.0),
            arc(1.0, effective_width * 0.25, effective_height * 0.1),
            arc(1.0, effective_width * 0.15, effective_height * 0.33),
            arc(0.8, 0.0, effective_height * 0.34),
            arc(1.0, -effective_width * 0.15, effective_height * 0.33),
            arc(1.0, -effective_width * 0.25, effective_height * 0.15),
            arc(1.0, -effective_width * 0.25, 0.0),
            arc(1.0, -effective_width * 0.25, 0.0),
            arc(1.0, -effective_width * 0.25, -effective_height * 0.15),
            arc(1.0, -effective_width * 0.1, -effective_height * 0.33),
            arc(0.8, 0.0, -effective_height * 0.34),
            arc(1.0, effective_width * 0.1, -effective_height * 0.33),
        ],
        -effective_width / 2.0,
        -effective_height / 2.0,
    )
}

pub(crate) fn cloud_geometry(
    label_width: f64,
    label_height: f64,
    padding: f64,
) -> OrganicShapeGeometry {
    let half_padding = padding.max(0.0) / 2.0;
    let width = label_width.max(0.0) + 2.0 * half_padding;
    let height = label_height.max(0.0) + 2.0 * half_padding;
    let r1 = 0.15 * width;
    let r2 = 0.25 * width;
    let r3 = 0.35 * width;
    let r4 = 0.2 * width;
    let arc = |rx: f64, ry: f64, rotation: f64, dx: f64, dy: f64| RelativeArc {
        rx,
        ry,
        x_axis_rotation_deg: rotation,
        large_arc: false,
        sweep: true,
        dx,
        dy,
    };

    organic_geometry(
        vec![
            arc(r1, r1, 0.0, width * 0.25, -width * 0.1),
            arc(r3, r3, 1.0, width * 0.4, -width * 0.1),
            arc(r2, r2, 1.0, width * 0.35, width * 0.2),
            arc(r1, r1, 1.0, width * 0.15, height * 0.35),
            arc(r4, r4, 1.0, -width * 0.15, height * 0.65),
            arc(r2, r1, 1.0, -width * 0.25, width * 0.15),
            arc(r3, r3, 1.0, -width * 0.5, 0.0),
            arc(r1, r1, 1.0, -width * 0.25, -width * 0.15),
            arc(r1, r1, 1.0, -width * 0.1, -height * 0.35),
            arc(r4, r4, 1.0, width * 0.1, -height * 0.65),
        ],
        -width / 2.0,
        -height / 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organic_shape_bounds_are_deterministic_for_source_probe_inputs() {
        let bang = bang_geometry(34.234_375, 19.0, 15.0);
        assert!((bang.width() - 136.542_968_75).abs() < 1.0e-4);
        assert!((bang.height() - 98.75).abs() < 1.0e-4);

        let cloud = cloud_geometry(40.531_25, 19.0, 15.0);
        assert!(
            (cloud.width() - 85.674_982_252_576_95).abs() < 1.0e-9,
            "cloud width: {}",
            cloud.width()
        );
        assert!(
            (cloud.height() - 60.691_277_982_195_26).abs() < 1.0e-9,
            "cloud height: {}",
            cloud.height()
        );
    }
}

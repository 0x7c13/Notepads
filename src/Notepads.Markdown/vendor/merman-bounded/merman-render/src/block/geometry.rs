//! Geometry owned by the Block family.
//!
//! Block layout has two different rectangles for many nodes: the rectangular slot allocated by
//! the grid and the actual outline emitted for the node.  Keeping those values in one artifact
//! prevents routing from accidentally treating a stretched slot as a circle, polygon, or
//! cylinder.

use crate::model::{LayoutNode, LayoutPoint};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockAllocatedBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockRectangleKind {
    Basic,
    Composite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum BlockShapeBoundary {
    Rectangle {
        width: f64,
        height: f64,
        radius: f64,
        kind: BlockRectangleKind,
    },
    Circle {
        radius: f64,
        width_attribute: f64,
        height_attribute: f64,
    },
    DoubleCircle {
        outer_radius: f64,
        inner_radius: f64,
        inner_width_attribute: f64,
        inner_height_attribute: f64,
    },
    Stadium {
        width: f64,
        height: f64,
    },
    Cylinder {
        width: f64,
        body_height: f64,
        radius_x: f64,
        radius_y: f64,
    },
    Polygon {
        points: Vec<LayoutPoint>,
        translation: LayoutPoint,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockShapeGeometry {
    pub id: String,
    pub allocated: BlockAllocatedBounds,
    pub boundary: BlockShapeBoundary,
}

#[derive(Debug, Clone, Copy)]
struct BlockShapeInputs<'a> {
    block_type: &'a str,
    directions: &'a [String],
    label_width: f64,
    label_height: f64,
    padding: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct BlockShapeAllocation {
    width: f64,
    height: f64,
    width_in_columns: i64,
}

impl BlockShapeGeometry {
    pub(crate) fn from_layout_node(
        node: &LayoutNode,
        block_type: &str,
        directions: &[String],
        padding: f64,
        width_in_columns: i64,
    ) -> Option<Self> {
        // Mermaid renders a block twice: the first pass measures the label and shape, while the
        // second pass supplies the grid-allocated slot.  Keep the final outline calculation
        // separate from the slot itself so routing never intersects the slot by accident.
        let boundary = rendered_boundary_for(
            BlockShapeInputs {
                block_type,
                directions,
                label_width: node.label_width.unwrap_or_default().max(0.0),
                label_height: node.label_height.unwrap_or_default().max(0.0),
                padding,
            },
            BlockShapeAllocation {
                width: node.width,
                height: node.height,
                width_in_columns,
            },
        )?;

        Some(Self {
            id: node.id.clone(),
            allocated: BlockAllocatedBounds {
                x: node.x,
                y: node.y,
                width: node.width.max(0.0),
                height: node.height.max(0.0),
            },
            boundary,
        })
    }

    pub(crate) fn intersect(&self, target: &LayoutPoint) -> LayoutPoint {
        let center = LayoutPoint {
            x: self.allocated.x,
            y: self.allocated.y,
        };
        match &self.boundary {
            BlockShapeBoundary::Rectangle {
                width,
                height,
                radius,
                ..
            } => intersect_rounded_rect(&center, *width, *height, *radius, *radius, target),
            BlockShapeBoundary::Circle { radius, .. }
            | BlockShapeBoundary::DoubleCircle {
                outer_radius: radius,
                ..
            } => intersect_circle(&center, *radius, target),
            BlockShapeBoundary::Stadium { width, height } => intersect_rounded_rect(
                &center,
                *width,
                *height,
                *height / 2.0,
                *height / 2.0,
                target,
            ),
            BlockShapeBoundary::Cylinder {
                width,
                body_height,
                radius_x,
                radius_y,
            } => intersect_cylinder(&center, *width, *body_height, *radius_x, *radius_y, target),
            BlockShapeBoundary::Polygon {
                points,
                translation,
            } => intersect_polygon(&center, points, translation, target),
        }
    }

    /// Returns the geometric bounds of the emitted outline in diagram coordinates.
    ///
    /// Block layout stores the grid slot separately from the shape that Mermaid actually draws.
    /// The latter is the source of truth for the SVG root viewport, because several shapes may
    /// intentionally extend beyond their allocated slot.
    pub(crate) fn rendered_extents(&self) -> (f64, f64, f64, f64) {
        let (min_x, min_y, max_x, max_y) = boundary_extents(&self.boundary);
        (
            self.allocated.x + min_x,
            self.allocated.y + min_y,
            self.allocated.x + max_x,
            self.allocated.y + max_y,
        )
    }
}

/// Computes the intrinsic shape size used while the grid is choosing a common slot.
///
/// This is the equivalent of Mermaid's first `insertNode(..., positioned = false)` pass.  The
/// returned dimensions are shape dimensions, not the eventual grid slot dimensions.
pub(crate) fn natural_shape_size(
    block_type: &str,
    directions: &[String],
    label_width: f64,
    label_height: f64,
    padding: f64,
    has_label: bool,
) -> Option<(f64, f64)> {
    let boundary = intrinsic_boundary_for(BlockShapeInputs {
        block_type,
        directions,
        label_width,
        label_height,
        padding,
    })?;
    if matches!(block_type, "composite" | "group") && !has_label {
        return None;
    }
    Some(boundary_size(&boundary))
}

impl BlockShapeAllocation {
    fn is_slot(self) -> bool {
        self.width > 0.0 || self.height > 0.0
    }
}

fn intrinsic_boundary_for(inputs: BlockShapeInputs<'_>) -> Option<BlockShapeBoundary> {
    // Keep these formulas aligned with Mermaid 11.17.2's rendering-util shape modules.  This is
    // deliberately the unpositioned/intrinsic pass; slot-aware adjustments live below.
    let BlockShapeInputs {
        block_type,
        directions,
        label_width,
        label_height,
        padding,
    } = inputs;
    let rect_w = (label_width + 4.0 * padding).max(1.0);
    let rect_h = (label_height + padding).max(1.0);
    let block_arrow_height = label_height + 2.0 * padding;
    let natural_block_arrow_width = label_width + block_arrow_height + padding;

    Some(match block_type {
        "composite" => BlockShapeBoundary::Rectangle {
            width: label_width.max(1.0),
            height: label_height.max(1.0),
            radius: 0.0,
            kind: BlockRectangleKind::Composite,
        },
        "group" => BlockShapeBoundary::Rectangle {
            width: rect_w,
            height: rect_h,
            radius: 0.0,
            kind: BlockRectangleKind::Basic,
        },
        "space" => return None,
        "circle" => BlockShapeBoundary::Circle {
            // `circle.ts` uses half the label padding on each side.  Circle's positioned pass
            // still derives its radius from the measured label, so the grid slot is not used.
            radius: (label_width + padding).max(1.0) / 2.0,
            width_attribute: (label_width + padding).max(1.0),
            height_attribute: (label_width + padding).max(1.0),
        },
        "doublecircle" => {
            let outer_radius = (label_width / 2.0 + padding).max(1.0);
            BlockShapeBoundary::DoubleCircle {
                outer_radius,
                inner_radius: (outer_radius - 5.0).max(0.0),
                inner_width_attribute: (outer_radius - 5.0).max(0.0) * 2.0,
                inner_height_attribute: (outer_radius - 5.0).max(0.0) * 2.0,
            }
        }
        "stadium" => BlockShapeBoundary::Stadium {
            width: (label_width + rect_h / 4.0 + padding).max(1.0),
            height: rect_h,
        },
        "cylinder" => {
            let width = (label_width + padding).max(1.0);
            let radius_x = width / 2.0;
            let radius_y = radius_x / (2.5 + width / 50.0);
            let body_height = (label_height + padding + radius_y).max(1.0);
            BlockShapeBoundary::Cylinder {
                width,
                body_height,
                radius_x,
                radius_y,
            }
        }
        "diamond" => {
            let side = (label_width + padding + label_height + padding).max(1.0);
            BlockShapeBoundary::Polygon {
                points: vec![
                    LayoutPoint {
                        x: side / 2.0,
                        y: 0.0,
                    },
                    LayoutPoint {
                        x: side,
                        y: -side / 2.0,
                    },
                    LayoutPoint {
                        x: side / 2.0,
                        y: -side,
                    },
                    LayoutPoint {
                        x: 0.0,
                        y: -side / 2.0,
                    },
                ],
                translation: LayoutPoint {
                    x: -side / 2.0 + 0.5,
                    y: side / 2.0,
                },
            }
        }
        "hexagon" => {
            let height = rect_h;
            let shoulder = height / 4.0;
            let width = (label_width + 2.0 * shoulder + padding).max(1.0);
            let shoulder = height / 4.0;
            polygon_boundary(
                vec![
                    LayoutPoint {
                        x: shoulder,
                        y: 0.0,
                    },
                    LayoutPoint {
                        x: width - shoulder,
                        y: 0.0,
                    },
                    LayoutPoint {
                        x: width,
                        y: -height / 2.0,
                    },
                    LayoutPoint {
                        x: width - shoulder,
                        y: -height,
                    },
                    LayoutPoint {
                        x: shoulder,
                        y: -height,
                    },
                    LayoutPoint {
                        x: 0.0,
                        y: -height / 2.0,
                    },
                ],
                width,
                height,
            )
        }
        "rect_left_inv_arrow" => odd_boundary(label_width, label_height, padding),
        "subroutine" => subroutine_boundary(label_width, label_height, padding),
        "lean_right" => lean_boundary("lean_right", label_width, label_height, padding),
        "lean_left" => lean_boundary("lean_left", label_width, label_height, padding),
        "trapezoid" => lean_boundary("trapezoid", label_width, label_height, padding),
        "inv_trapezoid" => inverted_trapezoid_boundary(label_width, label_height, padding),
        "block_arrow" => {
            let width = natural_block_arrow_width.max(1.0);
            let points =
                super::block_arrow_points_for_width(directions, label_height, padding, width)
                    .into_iter()
                    .map(|point| LayoutPoint {
                        x: point.x,
                        y: point.y,
                    })
                    .collect();
            polygon_boundary(points, width, block_arrow_height)
        }
        "round" => BlockShapeBoundary::Rectangle {
            width: rect_w,
            height: rect_h,
            radius: 5.0,
            kind: BlockRectangleKind::Basic,
        },
        _ => BlockShapeBoundary::Rectangle {
            width: rect_w,
            height: rect_h,
            radius: 0.0,
            kind: BlockRectangleKind::Basic,
        },
    })
}

fn rendered_boundary_for(
    inputs: BlockShapeInputs<'_>,
    allocation: BlockShapeAllocation,
) -> Option<BlockShapeBoundary> {
    let intrinsic = intrinsic_boundary_for(inputs)?;
    if !allocation.is_slot() {
        return Some(intrinsic);
    }

    let BlockShapeInputs {
        block_type,
        directions,
        label_width,
        label_height,
        padding,
    } = inputs;
    let slot_width = allocation.width.max(0.0);
    let slot_height = allocation.height.max(0.0);
    let intrinsic_width = boundary_size(&intrinsic).0;
    let intrinsic_height = boundary_size(&intrinsic).1;

    Some(match block_type {
        // drawRect() uses max(label-derived dimensions, node.width/node.height) in the positioned
        // pass.  This is the only ordinary shape where both dimensions are directly slot-aware.
        "composite" => BlockShapeBoundary::Rectangle {
            width: slot_width.max(1.0),
            height: slot_height.max(1.0),
            radius: 0.0,
            kind: BlockRectangleKind::Composite,
        },
        "group" => BlockShapeBoundary::Rectangle {
            width: slot_width.max(intrinsic_width).max(1.0),
            height: slot_height.max(intrinsic_height).max(1.0),
            radius: 0.0,
            kind: BlockRectangleKind::Basic,
        },
        "square" | "default" | "round" => BlockShapeBoundary::Rectangle {
            width: slot_width.max(intrinsic_width).max(1.0),
            height: slot_height.max(intrinsic_height).max(1.0),
            radius: if block_type == "round" { 5.0 } else { 0.0 },
            kind: BlockRectangleKind::Basic,
        },
        // circle.ts intentionally ignores node.width/node.height when computing the circle.  A
        // wider common grid slot only changes the label wrapping width, not the outline radius.
        "circle" | "stadium" | "diamond" => intrinsic,
        "doublecircle" => {
            let outer_radius = (slot_width / 2.0 + padding).max(1.0);
            let inner_radius = (outer_radius - 5.0).max(0.0);
            BlockShapeBoundary::DoubleCircle {
                outer_radius,
                inner_radius,
                inner_width_attribute: inner_radius * 2.0,
                inner_height_attribute: inner_radius * 2.0,
            }
        }
        "cylinder" => {
            cylinder_boundary(label_width, label_height, padding, slot_width, slot_height)
        }
        "hexagon" => hexagon_boundary(label_width, label_height, padding, slot_width, slot_height),
        "rect_left_inv_arrow" => {
            odd_boundary_slot(label_width, label_height, padding, slot_width, slot_height)
        }
        "subroutine" => {
            subroutine_boundary_slot(label_width, label_height, padding, slot_width, slot_height)
        }
        "lean_right" | "lean_left" | "trapezoid" => lean_boundary_slot(
            block_type,
            label_width,
            label_height,
            padding,
            slot_width,
            slot_height,
        ),
        "inv_trapezoid" => inverted_trapezoid_boundary_slot(
            label_width,
            label_height,
            padding,
            slot_width,
            slot_height,
        ),
        "block_arrow" => {
            let natural_width = label_width + label_height + 3.0 * padding;
            let height = label_height + 2.0 * padding;
            let width = if allocation.width_in_columns > 1 && slot_width > natural_width {
                slot_width
            } else {
                natural_width.max(1.0)
            };
            let points =
                super::block_arrow_points_for_width(directions, label_height, padding, width)
                    .into_iter()
                    .map(|point| LayoutPoint {
                        x: point.x,
                        y: point.y,
                    })
                    .collect();
            polygon_boundary(points, width, height)
        }
        // Unknown block shapes have the default rectangular renderer in Mermaid.
        _ => BlockShapeBoundary::Rectangle {
            width: slot_width.max(intrinsic_width).max(1.0),
            height: slot_height.max(intrinsic_height).max(1.0),
            radius: 0.0,
            kind: BlockRectangleKind::Basic,
        },
    })
}

// Kept as a small compatibility seam for geometry unit tests and callers that still want to
// provide an explicit allocation.  New code should use `intrinsic_boundary_for` for measurement
// and `rendered_boundary_for` for the final slot-aware outline.
#[cfg(test)]
fn boundary_for(
    inputs: BlockShapeInputs<'_>,
    allocation: BlockShapeAllocation,
) -> Option<BlockShapeBoundary> {
    rendered_boundary_for(inputs, allocation)
}

fn cylinder_boundary(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    // cylinder.ts mutates node.width/node.height before labelHelper(), then adds the cap geometry
    // back after measuring.  Reproduce those mutations analytically without conflating the slot
    // with the final path body height.
    let original_width = slot_width.max(0.0);
    let adjusted_width = if original_width > 0.0 {
        (original_width - padding).max(8.0)
    } else {
        0.0
    };
    let original_radius_x = original_width / 2.0;
    let original_radius_y = if original_width > 0.0 {
        original_radius_x / (2.5 + original_width / 50.0)
    } else {
        0.0
    };
    let adjusted_height = if slot_height > 0.0 {
        (slot_height - padding - original_radius_y * 3.0).max(8.0)
    } else {
        0.0
    };
    let width = (adjusted_width.max(label_width) + padding).max(1.0);
    let radius_x = width / 2.0;
    let radius_y = radius_x / (2.5 + width / 50.0);
    let body_height = (adjusted_height.max(label_height) + padding + radius_y).max(1.0);
    BlockShapeBoundary::Cylinder {
        width,
        body_height,
        radius_x,
        radius_y,
    }
}

fn hexagon_boundary(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    let original_height = slot_height.max(0.0);
    let m = original_height / 4.0;
    let adjusted_width = if original_height > 0.0 {
        (slot_width - 2.0 * m - padding).max(0.0)
    } else {
        0.0
    };
    let adjusted_height = if original_height > 0.0 {
        (slot_height - padding).max(0.0)
    } else {
        0.0
    };
    let height = (adjusted_height.max(label_height) + padding).max(1.0);
    let shoulder = height / 4.0;
    let width = (adjusted_width.max(label_width) + 2.0 * shoulder + padding).max(1.0);
    polygon_boundary(
        vec![
            LayoutPoint {
                x: shoulder,
                y: 0.0,
            },
            LayoutPoint {
                x: width - shoulder,
                y: 0.0,
            },
            LayoutPoint {
                x: width,
                y: -height / 2.0,
            },
            LayoutPoint {
                x: width - shoulder,
                y: -height,
            },
            LayoutPoint {
                x: shoulder,
                y: -height,
            },
            LayoutPoint {
                x: 0.0,
                y: -height / 2.0,
            },
        ],
        width,
        height,
    )
}

fn odd_boundary(label_width: f64, label_height: f64, padding: f64) -> BlockShapeBoundary {
    odd_boundary_with_slot(label_width, label_height, padding, 0.0, 0.0)
}

fn odd_boundary_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    odd_boundary_with_slot(label_width, label_height, padding, slot_width, slot_height)
}

fn odd_boundary_with_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    let height = (label_height + padding).max(slot_height).max(1.0);
    let notch_width = height / 4.0;
    let width = (label_width + padding)
        .max((slot_width - notch_width).max(0.0))
        .max(1.0);
    let x = -width / 2.0;
    let y = -height / 2.0;
    let notch = y / 2.0;
    polygon_with_translation(
        vec![
            LayoutPoint { x: x + notch, y },
            LayoutPoint { x, y: 0.0 },
            LayoutPoint {
                x: x + notch,
                y: -y,
            },
            LayoutPoint { x: -x, y: -y },
            LayoutPoint { x: -x, y },
        ],
        LayoutPoint {
            x: -notch / 2.0,
            y: 0.0,
        },
    )
}

fn subroutine_boundary(label_width: f64, label_height: f64, padding: f64) -> BlockShapeBoundary {
    subroutine_boundary_with_slot(label_width, label_height, padding, 0.0, 0.0)
}

fn subroutine_boundary_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    subroutine_boundary_with_slot(label_width, label_height, padding, slot_width, slot_height)
}

fn subroutine_boundary_with_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    const FRAME_WIDTH: f64 = 8.0;
    let total_width = (label_width + 2.0 * FRAME_WIDTH + padding)
        .max(slot_width)
        .max(1.0);
    let total_height = (label_height + padding).max(slot_height).max(1.0);
    let width = (total_width - 2.0 * FRAME_WIDTH).max(0.0);
    let points = vec![
        LayoutPoint { x: 0.0, y: 0.0 },
        LayoutPoint { x: width, y: 0.0 },
        LayoutPoint {
            x: width,
            y: -total_height,
        },
        LayoutPoint {
            x: 0.0,
            y: -total_height,
        },
        LayoutPoint { x: 0.0, y: 0.0 },
        LayoutPoint {
            x: -FRAME_WIDTH,
            y: 0.0,
        },
        LayoutPoint {
            x: width + FRAME_WIDTH,
            y: 0.0,
        },
        LayoutPoint {
            x: width + FRAME_WIDTH,
            y: -total_height,
        },
        LayoutPoint {
            x: -FRAME_WIDTH,
            y: -total_height,
        },
        LayoutPoint {
            x: -FRAME_WIDTH,
            y: 0.0,
        },
    ];
    polygon_boundary(points, width, total_height)
}

fn lean_boundary(
    kind: &str,
    label_width: f64,
    label_height: f64,
    padding: f64,
) -> BlockShapeBoundary {
    lean_boundary_with_slot(kind, label_width, label_height, padding, 0.0, 0.0)
}

fn lean_boundary_slot(
    kind: &str,
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    lean_boundary_with_slot(
        kind,
        label_width,
        label_height,
        padding,
        slot_width,
        slot_height,
    )
}

fn lean_boundary_with_slot(
    kind: &str,
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    let height = (label_height + padding).max(slot_height).max(1.0);
    let width = (label_width + padding)
        .max((slot_width - height).max(0.0))
        .max(1.0);
    let points = match kind {
        "lean_right" => vec![
            LayoutPoint {
                x: -3.0 * height / 6.0,
                y: 0.0,
            },
            LayoutPoint { x: width, y: 0.0 },
            LayoutPoint {
                x: width + 3.0 * height / 6.0,
                y: -height,
            },
            LayoutPoint { x: 0.0, y: -height },
        ],
        "lean_left" => vec![
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint {
                x: width + 3.0 * height / 6.0,
                y: 0.0,
            },
            LayoutPoint {
                x: width,
                y: -height,
            },
            LayoutPoint {
                x: -3.0 * height / 6.0,
                y: -height,
            },
        ],
        "trapezoid" => vec![
            LayoutPoint {
                x: -3.0 * height / 6.0,
                y: 0.0,
            },
            LayoutPoint {
                x: width + 3.0 * height / 6.0,
                y: 0.0,
            },
            LayoutPoint {
                x: width,
                y: -height,
            },
            LayoutPoint { x: 0.0, y: -height },
        ],
        _ => vec![LayoutPoint { x: 0.0, y: 0.0 }],
    };
    polygon_boundary(points, width, height)
}

fn inverted_trapezoid_boundary(
    label_width: f64,
    label_height: f64,
    padding: f64,
) -> BlockShapeBoundary {
    inverted_trapezoid_boundary_with_slot(label_width, label_height, padding, 0.0, 0.0)
}

fn inverted_trapezoid_boundary_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    inverted_trapezoid_boundary_with_slot(
        label_width,
        label_height,
        padding,
        slot_width,
        slot_height,
    )
}

fn inverted_trapezoid_boundary_with_slot(
    label_width: f64,
    label_height: f64,
    padding: f64,
    slot_width: f64,
    slot_height: f64,
) -> BlockShapeBoundary {
    let height = (label_height + 2.0 * padding).max(slot_height).max(1.0);
    let width = (label_width + 2.0 * padding)
        .max((slot_width - height).max(0.0))
        .max(1.0);
    polygon_boundary(
        vec![
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint { x: width, y: 0.0 },
            LayoutPoint {
                x: width + 3.0 * height / 6.0,
                y: -height,
            },
            LayoutPoint {
                x: -3.0 * height / 6.0,
                y: -height,
            },
        ],
        width,
        height,
    )
}

fn polygon_with_translation(
    points: Vec<LayoutPoint>,
    translation: LayoutPoint,
) -> BlockShapeBoundary {
    BlockShapeBoundary::Polygon {
        points,
        translation,
    }
}

fn polygon_boundary(
    points: Vec<LayoutPoint>,
    base_width: f64,
    base_height: f64,
) -> BlockShapeBoundary {
    BlockShapeBoundary::Polygon {
        points,
        translation: LayoutPoint {
            x: -base_width / 2.0,
            y: base_height / 2.0,
        },
    }
}

fn boundary_size(boundary: &BlockShapeBoundary) -> (f64, f64) {
    let (min_x, min_y, max_x, max_y) = boundary_extents(boundary);
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

fn boundary_extents(boundary: &BlockShapeBoundary) -> (f64, f64, f64, f64) {
    match boundary {
        BlockShapeBoundary::Rectangle { width, height, .. } => {
            (-width / 2.0, -height / 2.0, width / 2.0, height / 2.0)
        }
        BlockShapeBoundary::Circle { radius, .. }
        | BlockShapeBoundary::DoubleCircle {
            outer_radius: radius,
            ..
        } => (-radius, -radius, *radius, *radius),
        BlockShapeBoundary::Stadium { width, height } => {
            (-width / 2.0, -height / 2.0, width / 2.0, height / 2.0)
        }
        BlockShapeBoundary::Cylinder {
            width,
            body_height,
            radius_y,
            ..
        } => (
            -width / 2.0,
            -(body_height / 2.0 + radius_y),
            width / 2.0,
            body_height / 2.0 + radius_y,
        ),
        BlockShapeBoundary::Polygon {
            points,
            translation,
        } => {
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for point in points {
                min_x = min_x.min(point.x + translation.x);
                max_x = max_x.max(point.x + translation.x);
                min_y = min_y.min(point.y + translation.y);
                max_y = max_y.max(point.y + translation.y);
            }
            if points.is_empty() {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                (min_x, min_y, max_x, max_y)
            }
        }
    }
}

fn intersect_rect(
    center: &LayoutPoint,
    width: f64,
    height: f64,
    target: &LayoutPoint,
) -> LayoutPoint {
    let dx = target.x - center.x;
    let dy = target.y - center.y;
    if dx.abs() <= f64::EPSILON && dy.abs() <= f64::EPSILON {
        return center.clone();
    }
    let half_width = width.max(0.0) / 2.0;
    let half_height = height.max(0.0) / 2.0;
    let scale = if dx.abs() * half_height > dy.abs() * half_width {
        half_width / dx.abs().max(f64::EPSILON)
    } else {
        half_height / dy.abs().max(f64::EPSILON)
    };
    LayoutPoint {
        x: center.x + dx * scale,
        y: center.y + dy * scale,
    }
}

fn intersect_circle(center: &LayoutPoint, radius: f64, target: &LayoutPoint) -> LayoutPoint {
    let dx = target.x - center.x;
    let dy = target.y - center.y;
    let distance = dx.hypot(dy);
    if distance <= f64::EPSILON {
        return center.clone();
    }
    LayoutPoint {
        x: center.x + dx / distance * radius.max(0.0),
        y: center.y + dy / distance * radius.max(0.0),
    }
}

fn intersect_rounded_rect(
    center: &LayoutPoint,
    width: f64,
    height: f64,
    radius_x: f64,
    radius_y: f64,
    target: &LayoutPoint,
) -> LayoutPoint {
    let dx = target.x - center.x;
    let dy = target.y - center.y;
    if dx.abs() <= f64::EPSILON && dy.abs() <= f64::EPSILON {
        return center.clone();
    }
    let half_width = width.max(0.0) / 2.0;
    let half_height = height.max(0.0) / 2.0;
    // SVG clamps rx/ry independently to half of the rendered width/height. In particular, a
    // short-label stadium can be narrower than it is tall and therefore renders as an ellipse.
    let radius_x = radius_x.max(0.0).min(half_width);
    let radius_y = radius_y.max(0.0).min(half_height);
    if radius_x <= f64::EPSILON || radius_y <= f64::EPSILON {
        return intersect_rect(center, width, height, target);
    }

    let straight_half_width = half_width - radius_x;
    let straight_half_height = half_height - radius_y;
    let mut candidate = f64::INFINITY;

    if dy.abs() > f64::EPSILON {
        let t = half_height / dy.abs();
        let x = t * dx;
        if x.abs() <= straight_half_width + 1e-12 {
            candidate = candidate.min(t);
        }
    }

    if dx.abs() > f64::EPSILON {
        let t = half_width / dx.abs();
        let y = t * dy;
        if y.abs() <= straight_half_height + 1e-12 {
            candidate = candidate.min(t);
        }
    }

    let side_x = if dx < 0.0 { -1.0 } else { 1.0 };
    let side_y = if dy < 0.0 { -1.0 } else { 1.0 };
    let center_x = side_x * straight_half_width;
    let center_y = side_y * straight_half_height;
    let normalized_dx = dx / radius_x;
    let normalized_dy = dy / radius_y;
    let normalized_center_x = center_x / radius_x;
    let normalized_center_y = center_y / radius_y;
    let a = normalized_dx * normalized_dx + normalized_dy * normalized_dy;
    let b = -2.0 * (normalized_dx * normalized_center_x + normalized_dy * normalized_center_y);
    let c =
        normalized_center_x * normalized_center_x + normalized_center_y * normalized_center_y - 1.0;
    let discriminant = (b * b - 4.0 * a * c).max(0.0);
    let root = discriminant.sqrt();
    for t in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
        if t < 0.0 {
            continue;
        }
        let x = t * dx;
        let y = t * dy;
        if x * side_x + 1e-12 >= straight_half_width && y * side_y + 1e-12 >= straight_half_height {
            candidate = candidate.min(t);
        }
    }

    if !candidate.is_finite() {
        return intersect_rect(center, width, height, target);
    }
    LayoutPoint {
        x: center.x + dx * candidate,
        y: center.y + dy * candidate,
    }
}

fn intersect_cylinder(
    center: &LayoutPoint,
    width: f64,
    body_height: f64,
    radius_x: f64,
    radius_y: f64,
    target: &LayoutPoint,
) -> LayoutPoint {
    let total_height = body_height + 2.0 * radius_y;
    let mut position = intersect_rect(center, width, total_height, target);
    let x = position.x - center.x;
    if radius_x > f64::EPSILON
        && (x.abs() < width / 2.0
            || ((x.abs() - width / 2.0).abs() < 1e-12
                && (position.y - center.y).abs() > total_height / 2.0 - radius_y))
    {
        let mut y = radius_y * radius_y * (1.0 - (x * x) / (radius_x * radius_x));
        y = y.max(0.0).sqrt();
        y = radius_y - y;
        if target.y - center.y > 0.0 {
            y = -y;
        }
        position.y += y;
    }
    position
}

fn intersect_polygon(
    center: &LayoutPoint,
    points: &[LayoutPoint],
    translation: &LayoutPoint,
    target: &LayoutPoint,
) -> LayoutPoint {
    let dx = target.x - center.x;
    let dy = target.y - center.y;
    if dx.abs() <= f64::EPSILON && dy.abs() <= f64::EPSILON {
        return center.clone();
    }

    let mut best_t: Option<f64> = None;
    for index in 0..points.len() {
        let a = LayoutPoint {
            x: points[index].x + translation.x + center.x,
            y: points[index].y + translation.y + center.y,
        };
        let b = LayoutPoint {
            x: points[(index + 1) % points.len()].x + translation.x + center.x,
            y: points[(index + 1) % points.len()].y + translation.y + center.y,
        };
        let ex = b.x - a.x;
        let ey = b.y - a.y;
        let denominator = dx * ey - dy * ex;
        if denominator.abs() <= 1e-12 {
            continue;
        }
        let acx = a.x - center.x;
        let acy = a.y - center.y;
        let t = (acx * ey - acy * ex) / denominator;
        let u = (acx * dy - acy * dx) / denominator;
        let segment = -1e-12..=1.0 + 1e-12;
        if segment.contains(&t) && segment.contains(&u) {
            best_t = Some(best_t.map_or(t, |best| best.max(t)));
        }
    }

    let t = best_t.unwrap_or(0.0);
    LayoutPoint {
        x: center.x + dx * t,
        y: center.y + dy * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point_segment_distance(point: &LayoutPoint, start: &LayoutPoint, end: &LayoutPoint) -> f64 {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_sq = dx * dx + dy * dy;
        if length_sq <= f64::EPSILON {
            return (point.x - start.x).hypot(point.y - start.y);
        }
        let projection =
            (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_sq).clamp(0.0, 1.0);
        (point.x - (start.x + projection * dx)).hypot(point.y - (start.y + projection * dy))
    }

    fn rounded_rect_boundary_residual(
        width: f64,
        height: f64,
        radius_x: f64,
        radius_y: f64,
        point: &LayoutPoint,
    ) -> f64 {
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        let radius_x = radius_x.min(half_width);
        let radius_y = radius_y.min(half_height);
        if radius_x <= f64::EPSILON || radius_y <= f64::EPSILON {
            let horizontal = if point.x.abs() <= half_width + 1e-9 {
                (point.y.abs() - half_height).abs()
            } else {
                f64::INFINITY
            };
            let vertical = if point.y.abs() <= half_height + 1e-9 {
                (point.x.abs() - half_width).abs()
            } else {
                f64::INFINITY
            };
            return horizontal.min(vertical);
        }

        let straight_half_width = half_width - radius_x;
        let straight_half_height = half_height - radius_y;
        let horizontal = if point.x.abs() <= straight_half_width + 1e-9 {
            (point.y.abs() - half_height).abs()
        } else {
            f64::INFINITY
        };
        let vertical = if point.y.abs() <= straight_half_height + 1e-9 {
            (point.x.abs() - half_width).abs()
        } else {
            f64::INFINITY
        };
        let corner = (((point.x.abs() - straight_half_width) / radius_x).powi(2)
            + ((point.y.abs() - straight_half_height) / radius_y).powi(2)
            - 1.0)
            .abs();
        horizontal.min(vertical).min(corner)
    }

    fn distance_to_boundary(boundary: &BlockShapeBoundary, point: &LayoutPoint) -> f64 {
        match boundary {
            BlockShapeBoundary::Rectangle {
                width,
                height,
                radius,
                ..
            } => rounded_rect_boundary_residual(*width, *height, *radius, *radius, point),
            BlockShapeBoundary::Circle { radius, .. }
            | BlockShapeBoundary::DoubleCircle {
                outer_radius: radius,
                ..
            } => (point.x.hypot(point.y) - radius).abs(),
            BlockShapeBoundary::Stadium { width, height } => {
                rounded_rect_boundary_residual(*width, *height, *height / 2.0, *height / 2.0, point)
            }
            BlockShapeBoundary::Cylinder {
                width,
                body_height,
                radius_x,
                radius_y,
            } => {
                let side = if point.y.abs() <= body_height / 2.0 + 1e-9 {
                    (point.x.abs() - width / 2.0).abs()
                } else {
                    f64::INFINITY
                };
                let cap_center_y = point.y.signum() * body_height / 2.0;
                let cap = ((point.x / radius_x).powi(2)
                    + ((point.y - cap_center_y) / radius_y).powi(2)
                    - 1.0)
                    .abs();
                side.min(cap)
            }
            BlockShapeBoundary::Polygon {
                points,
                translation,
            } => points
                .iter()
                .enumerate()
                .map(|(index, start)| {
                    let end = &points[(index + 1) % points.len()];
                    point_segment_distance(
                        point,
                        &LayoutPoint {
                            x: start.x + translation.x,
                            y: start.y + translation.y,
                        },
                        &LayoutPoint {
                            x: end.x + translation.x,
                            y: end.y + translation.y,
                        },
                    )
                })
                .fold(f64::INFINITY, f64::min),
        }
    }

    fn assert_intersections_reach_boundary(boundary: BlockShapeBoundary) {
        let geometry = BlockShapeGeometry {
            id: "node".to_string(),
            allocated: BlockAllocatedBounds {
                x: 0.0,
                y: 0.0,
                width: 240.0,
                height: 180.0,
            },
            boundary,
        };
        for target in [
            LayoutPoint { x: 300.0, y: 0.0 },
            LayoutPoint { x: -300.0, y: 0.0 },
            LayoutPoint { x: 0.0, y: 300.0 },
            LayoutPoint { x: 0.0, y: -300.0 },
            LayoutPoint { x: 300.0, y: 240.0 },
            LayoutPoint {
                x: -300.0,
                y: -240.0,
            },
        ] {
            let intersection = geometry.intersect(&target);
            let distance = distance_to_boundary(&geometry.boundary, &intersection);
            assert!(
                distance <= 1e-7,
                "intersection {intersection:?} misses {:?} by {distance}",
                geometry.boundary
            );
        }
    }

    #[test]
    fn every_boundary_representation_uses_its_rendered_outline_for_intersection() {
        assert_intersections_reach_boundary(BlockShapeBoundary::Rectangle {
            width: 80.0,
            height: 40.0,
            radius: 0.0,
            kind: BlockRectangleKind::Basic,
        });
        assert_intersections_reach_boundary(BlockShapeBoundary::Circle {
            radius: 24.0,
            width_attribute: 48.0,
            height_attribute: 32.0,
        });
        assert_intersections_reach_boundary(BlockShapeBoundary::DoubleCircle {
            outer_radius: 29.0,
            inner_radius: 24.0,
            inner_width_attribute: 48.0,
            inner_height_attribute: 32.0,
        });
        assert_intersections_reach_boundary(BlockShapeBoundary::Stadium {
            width: 96.0,
            height: 36.0,
        });
        assert_intersections_reach_boundary(BlockShapeBoundary::Cylinder {
            width: 72.0,
            body_height: 40.0,
            radius_x: 36.0,
            radius_y: 9.0,
        });
        assert_intersections_reach_boundary(polygon_boundary(
            vec![
                LayoutPoint { x: 30.0, y: 0.0 },
                LayoutPoint { x: 60.0, y: -30.0 },
                LayoutPoint { x: 30.0, y: -60.0 },
                LayoutPoint { x: 0.0, y: -30.0 },
            ],
            60.0,
            60.0,
        ));
    }

    #[test]
    fn narrow_stadium_uses_the_svg_clamped_ellipse_boundary() {
        let geometry = BlockShapeGeometry {
            id: "stadium".to_string(),
            allocated: BlockAllocatedBounds {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 80.0,
            },
            boundary: BlockShapeBoundary::Stadium {
                width: 40.0,
                height: 80.0,
            },
        };

        let horizontal = geometry.intersect(&LayoutPoint { x: 100.0, y: 0.0 });
        assert!((horizontal.x - 20.0).abs() <= 1e-9);
        assert!(horizontal.y.abs() <= 1e-9);

        let diagonal = geometry.intersect(&LayoutPoint { x: 100.0, y: 100.0 });
        let ellipse_residual = (diagonal.x / 20.0).powi(2) + (diagonal.y / 40.0).powi(2);
        assert!((ellipse_residual - 1.0).abs() <= 1e-9);
    }

    #[test]
    fn rounded_rectangle_intersection_follows_the_corner_arc() {
        let geometry = BlockShapeGeometry {
            id: "round".to_string(),
            allocated: BlockAllocatedBounds {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 40.0,
            },
            boundary: BlockShapeBoundary::Rectangle {
                width: 80.0,
                height: 40.0,
                radius: 5.0,
                kind: BlockRectangleKind::Basic,
            },
        };

        let intersection = geometry.intersect(&LayoutPoint { x: 80.0, y: 40.0 });
        assert!(intersection.x < 40.0 && intersection.y < 20.0);
        let corner_residual =
            ((intersection.x - 35.0) / 5.0).powi(2) + ((intersection.y - 15.0) / 5.0).powi(2);
        assert!((corner_residual - 1.0).abs() <= 1e-9);
    }

    #[test]
    fn all_block_shapes_materialize_a_closed_boundary_from_one_shape_factory() {
        let directions = vec!["right".to_string(), "down".to_string()];
        let cases = [
            "square",
            "round",
            "composite",
            "group",
            "circle",
            "doublecircle",
            "stadium",
            "cylinder",
            "diamond",
            "hexagon",
            "rect_left_inv_arrow",
            "subroutine",
            "lean_right",
            "lean_left",
            "trapezoid",
            "inv_trapezoid",
            "block_arrow",
        ];
        for block_type in cases {
            let boundary = boundary_for(
                BlockShapeInputs {
                    block_type,
                    directions: &directions,
                    label_width: 64.0,
                    label_height: 24.0,
                    padding: 8.0,
                },
                BlockShapeAllocation {
                    width: 120.0,
                    height: 72.0,
                    width_in_columns: 2,
                },
            )
            .unwrap_or_else(|| panic!("{block_type} must own a visible boundary"));
            let (width, height) = boundary_size(&boundary);
            assert!(
                width.is_finite() && width > 0.0,
                "invalid {block_type} width"
            );
            assert!(
                height.is_finite() && height > 0.0,
                "invalid {block_type} height"
            );
            assert_intersections_reach_boundary(boundary);
        }
        assert!(
            boundary_for(
                BlockShapeInputs {
                    block_type: "space",
                    directions: &[],
                    label_width: 0.0,
                    label_height: 0.0,
                    padding: 8.0,
                },
                BlockShapeAllocation::default(),
            )
            .is_none(),
            "space is allocation-only and must not fabricate a visible boundary"
        );
    }
}

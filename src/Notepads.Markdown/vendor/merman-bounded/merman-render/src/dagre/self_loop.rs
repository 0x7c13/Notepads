use crate::model::LayoutPoint;
use dugong::RankDir;
use merman_core::geom::Size;

pub(crate) struct CompactSelfLoopGeometry {
    pub(crate) points: Vec<LayoutPoint>,
    pub(crate) label_center: LayoutPoint,
}

#[derive(Clone, Copy)]
enum SelfLoopSide {
    Top,
    Bottom,
    Left,
    Right,
}

fn default_side(rankdir: RankDir) -> SelfLoopSide {
    match rankdir {
        RankDir::BT => SelfLoopSide::Bottom,
        RankDir::LR => SelfLoopSide::Right,
        RankDir::RL => SelfLoopSide::Left,
        RankDir::TB => SelfLoopSide::Top,
    }
}

fn side_from_layout_hints(
    node_center: &LayoutPoint,
    rankdir: RankDir,
    hints: &[LayoutPoint],
) -> SelfLoopSide {
    if hints.is_empty() {
        return default_side(rankdir);
    }

    let (sum_x, sum_y) = hints
        .iter()
        .fold((0.0, 0.0), |(x, y), point| (x + point.x, y + point.y));
    let center_x = sum_x / hints.len() as f64;
    let center_y = sum_y / hints.len() as f64;
    let dx = center_x - node_center.x;
    let dy = center_y - node_center.y;

    if dx.abs() > dy.abs() {
        if dx > 0.0 {
            SelfLoopSide::Right
        } else {
            SelfLoopSide::Left
        }
    } else if dy.abs() > 0.0 {
        if dy > 0.0 {
            SelfLoopSide::Bottom
        } else {
            SelfLoopSide::Top
        }
    } else {
        default_side(rankdir)
    }
}

// Port of Mermaid 11.16.0's getSelfLoopSide, getSelfLoopPoints, and
// getSelfLoopLabelPosition helpers in the Dagre rendering adapter.
pub(crate) fn compact_self_loop_geometry(
    node_center: &LayoutPoint,
    node_size: Size,
    rankdir: RankDir,
    hints: &[LayoutPoint],
    y_offset: f64,
    label_size: Size,
) -> CompactSelfLoopGeometry {
    let side = side_from_layout_hints(node_center, rankdir, hints);
    let x = node_center.x;
    let y = node_center.y - y_offset;
    let half_width = node_size.width / 2.0;
    let half_height = node_size.height / 2.0;
    let max_span = (node_size.width * 0.8).clamp(36.0, 100.0);
    let span = label_size
        .width
        .max(node_size.width * 0.35)
        .clamp(36.0, max_span);
    let depth = (node_size.width.min(node_size.height) * 0.45).clamp(24.0, 48.0);

    let points = match side {
        SelfLoopSide::Bottom => {
            let bottom = y + half_height;
            vec![
                LayoutPoint {
                    x: x - span / 2.0,
                    y: bottom,
                },
                LayoutPoint {
                    x: x - span / 2.0,
                    y: bottom + depth,
                },
                LayoutPoint {
                    x: x + span / 2.0,
                    y: bottom + depth,
                },
                LayoutPoint {
                    x: x + span / 2.0,
                    y: bottom,
                },
            ]
        }
        SelfLoopSide::Right => {
            let right = x + half_width;
            vec![
                LayoutPoint {
                    x: right,
                    y: y - span / 2.0,
                },
                LayoutPoint {
                    x: right + depth,
                    y: y - span / 2.0,
                },
                LayoutPoint {
                    x: right + depth,
                    y: y + span / 2.0,
                },
                LayoutPoint {
                    x: right,
                    y: y + span / 2.0,
                },
            ]
        }
        SelfLoopSide::Left => {
            let left = x - half_width;
            vec![
                LayoutPoint {
                    x: left,
                    y: y - span / 2.0,
                },
                LayoutPoint {
                    x: left - depth,
                    y: y - span / 2.0,
                },
                LayoutPoint {
                    x: left - depth,
                    y: y + span / 2.0,
                },
                LayoutPoint {
                    x: left,
                    y: y + span / 2.0,
                },
            ]
        }
        SelfLoopSide::Top => {
            let top = y - half_height;
            vec![
                LayoutPoint {
                    x: x - span / 2.0,
                    y: top,
                },
                LayoutPoint {
                    x: x - span / 2.0,
                    y: top - depth,
                },
                LayoutPoint {
                    x: x + span / 2.0,
                    y: top - depth,
                },
                LayoutPoint {
                    x: x + span / 2.0,
                    y: top,
                },
            ]
        }
    };

    let gap = 4.0;
    let label_center = match side {
        SelfLoopSide::Bottom => LayoutPoint {
            x,
            y: points[1].y + label_size.height / 2.0 + gap,
        },
        SelfLoopSide::Right => LayoutPoint {
            x: points[1].x + label_size.width / 2.0 + gap,
            y,
        },
        SelfLoopSide::Left => LayoutPoint {
            x: points[1].x - label_size.width / 2.0 - gap,
            y,
        },
        SelfLoopSide::Top => LayoutPoint {
            x,
            y: points[1].y - label_size.height / 2.0 - gap,
        },
    };

    CompactSelfLoopGeometry {
        points,
        label_center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
    }

    fn assert_point(actual: &LayoutPoint, expected: (f64, f64)) {
        assert_eq!((actual.x, actual.y), expected);
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_point(actual, *expected);
        }
    }

    #[test]
    fn creates_mermaid_compact_self_loop_geometry() {
        let geometry = compact_self_loop_geometry(
            &point(10.0, 10.0),
            Size::new(20.0, 20.0),
            RankDir::TB,
            &[],
            0.0,
            Size::new(0.0, 0.0),
        );

        assert_points(
            &geometry.points,
            &[(-8.0, 0.0), (-8.0, -24.0), (28.0, -24.0), (28.0, 0.0)],
        );
        assert_point(&geometry.label_center, (10.0, -28.0));
    }

    #[test]
    fn layout_hints_choose_the_mermaid_self_loop_side() {
        let geometry = compact_self_loop_geometry(
            &point(10.0, 10.0),
            Size::new(20.0, 20.0),
            RankDir::TB,
            &[point(80.0, 5.0), point(80.0, 15.0)],
            0.0,
            Size::new(20.0, 10.0),
        );

        assert_points(
            &geometry.points,
            &[(20.0, -8.0), (44.0, -8.0), (44.0, 28.0), (20.0, 28.0)],
        );
        assert_point(&geometry.label_center, (58.0, 10.0));
    }

    #[test]
    fn rankdir_selects_all_four_default_sides() {
        let cases = [
            (
                RankDir::TB,
                [(-8.0, 0.0), (-8.0, -24.0), (28.0, -24.0), (28.0, 0.0)],
                (10.0, -33.0),
            ),
            (
                RankDir::BT,
                [(-8.0, 20.0), (-8.0, 44.0), (28.0, 44.0), (28.0, 20.0)],
                (10.0, 53.0),
            ),
            (
                RankDir::LR,
                [(20.0, -8.0), (44.0, -8.0), (44.0, 28.0), (20.0, 28.0)],
                (58.0, 10.0),
            ),
            (
                RankDir::RL,
                [(0.0, -8.0), (-24.0, -8.0), (-24.0, 28.0), (0.0, 28.0)],
                (-38.0, 10.0),
            ),
        ];

        for (rankdir, expected_points, expected_label_center) in cases {
            let geometry = compact_self_loop_geometry(
                &point(10.0, 10.0),
                Size::new(20.0, 20.0),
                rankdir,
                &[],
                0.0,
                Size::new(20.0, 10.0),
            );
            assert_points(&geometry.points, &expected_points);
            assert_point(&geometry.label_center, expected_label_center);
        }
    }

    #[test]
    fn span_and_depth_follow_mermaid_clamp_boundaries() {
        let lower = compact_self_loop_geometry(
            &point(0.0, 0.0),
            Size::new(20.0, 20.0),
            RankDir::TB,
            &[],
            0.0,
            Size::new(0.0, 0.0),
        );
        assert_points(
            &lower.points,
            &[(-18.0, -10.0), (-18.0, -34.0), (18.0, -34.0), (18.0, -10.0)],
        );

        let upper = compact_self_loop_geometry(
            &point(0.0, 0.0),
            Size::new(200.0, 200.0),
            RankDir::TB,
            &[],
            0.0,
            Size::new(500.0, 20.0),
        );
        assert_points(
            &upper.points,
            &[
                (-50.0, -100.0),
                (-50.0, -148.0),
                (50.0, -148.0),
                (50.0, -100.0),
            ],
        );
    }

    #[test]
    fn y_offset_matches_mermaid_recursive_render_coordinates() {
        let geometry = compact_self_loop_geometry(
            &point(10.0, 30.0),
            Size::new(20.0, 20.0),
            RankDir::TB,
            &[],
            20.0,
            Size::new(0.0, 0.0),
        );

        assert_points(
            &geometry.points,
            &[(-8.0, 0.0), (-8.0, -24.0), (28.0, -24.0), (28.0, 0.0)],
        );
        assert_point(&geometry.label_center, (10.0, -28.0));
    }
}

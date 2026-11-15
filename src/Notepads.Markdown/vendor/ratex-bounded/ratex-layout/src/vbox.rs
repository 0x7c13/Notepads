use crate::layout_box::{BoxContent, LayoutBox, VBoxChild, VBoxChildKind};

/// Lay out boxes vertically: stack top-to-bottom.
///
/// The first child sits at the top. The baseline of the resulting
/// vbox is at `depth` below the top of the first child.
///
/// `baseline_index`: which child's baseline becomes the vbox baseline.
/// If None, the baseline is at the bottom of the last child.
pub fn make_vbox(children: Vec<VBoxChild>) -> LayoutBox {
    if children.is_empty() {
        return LayoutBox::new_empty();
    }

    let mut width = 0.0_f64;
    let mut total_height = 0.0_f64;

    for child in &children {
        match &child.kind {
            VBoxChildKind::Box(b) => {
                width = width.max(b.width);
                total_height += b.height + b.depth;
            }
            VBoxChildKind::Kern(k) => {
                total_height += k;
            }
        }
    }

    // Default: baseline at the bottom of the last element
    let height = total_height;
    let depth = 0.0;

    LayoutBox {
        width,
        height,
        depth,
        content: BoxContent::VBox(children),
        color: ratex_types::color::Color::BLACK,
    }
}

/// Create a VBox with the baseline positioned so that a specific
/// vertical position is at the baseline.
///
/// `depth_below_baseline`: how much of the vbox extends below the baseline.
pub fn make_vbox_with_depth(children: Vec<VBoxChild>, depth_below_baseline: f64) -> LayoutBox {
    let mut vbox = make_vbox(children);
    let total = vbox.height + vbox.depth;
    vbox.depth = depth_below_baseline;
    vbox.height = total - depth_below_baseline;
    vbox
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_box::BoxContent;
    use ratex_types::color::Color;

    fn make_test_box(w: f64, h: f64, d: f64) -> LayoutBox {
        LayoutBox {
            width: w,
            height: h,
            depth: d,
            content: BoxContent::Empty,
            color: Color::BLACK,
        }
    }

    #[test]
    fn test_empty_vbox() {
        let vbox = make_vbox(vec![]);
        assert_eq!(vbox.width, 0.0);
        assert_eq!(vbox.height, 0.0);
    }

    #[test]
    fn test_single_element_vbox() {
        let child = VBoxChild {
            kind: VBoxChildKind::Box(Box::new(make_test_box(1.0, 0.5, 0.2))),
            shift: 0.0,
        };
        let vbox = make_vbox(vec![child]);
        assert!((vbox.width - 1.0).abs() < 1e-10);
        assert!((vbox.height - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_vbox_with_kern() {
        let child1 = VBoxChild {
            kind: VBoxChildKind::Box(Box::new(make_test_box(1.0, 0.5, 0.2))),
            shift: 0.0,
        };
        let kern = VBoxChild {
            kind: VBoxChildKind::Kern(0.1),
            shift: 0.0,
        };
        let child2 = VBoxChild {
            kind: VBoxChildKind::Box(Box::new(make_test_box(0.8, 0.3, 0.1))),
            shift: 0.0,
        };
        let vbox = make_vbox(vec![child1, kern, child2]);
        assert!((vbox.width - 1.0).abs() < 1e-10);
        // total = (0.5+0.2) + 0.1 + (0.3+0.1) = 1.2
        assert!((vbox.height - 1.2).abs() < 1e-10);
    }

    #[test]
    fn test_vbox_with_depth() {
        let child = VBoxChild {
            kind: VBoxChildKind::Box(Box::new(make_test_box(1.0, 0.8, 0.4))),
            shift: 0.0,
        };
        let vbox = make_vbox_with_depth(vec![child], 0.5);
        assert!((vbox.height - 0.7).abs() < 1e-10);
        assert!((vbox.depth - 0.5).abs() < 1e-10);
    }
}

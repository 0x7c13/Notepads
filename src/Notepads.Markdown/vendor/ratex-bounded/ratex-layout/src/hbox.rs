use crate::layout_box::{BoxContent, LayoutBox};

/// Lay out boxes horizontally: x accumulates, take max height/depth.
///
/// This corresponds to TeX's \hbox operation: children are placed
/// side by side from left to right. The resulting box's height is
/// the maximum child height, and its depth is the maximum child depth.
pub fn make_hbox(children: Vec<LayoutBox>) -> LayoutBox {
    let mut width = 0.0_f64;
    let mut height = 0.0_f64;
    let mut depth = 0.0_f64;

    for child in &children {
        width += child.width;
        height = height.max(child.height);
        depth = depth.max(child.depth);
    }

    LayoutBox {
        width,
        height,
        depth,
        content: BoxContent::HBox(children),
        color: ratex_types::color::Color::BLACK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_hbox() {
        let hbox = make_hbox(vec![]);
        assert_eq!(hbox.width, 0.0);
        assert_eq!(hbox.height, 0.0);
        assert_eq!(hbox.depth, 0.0);
    }

    #[test]
    fn test_single_child_hbox() {
        let child = LayoutBox {
            width: 0.5,
            height: 0.43,
            depth: 0.0,
            content: BoxContent::Empty,
            color: ratex_types::color::Color::BLACK,
        };
        let hbox = make_hbox(vec![child]);
        assert!((hbox.width - 0.5).abs() < 1e-10);
        assert!((hbox.height - 0.43).abs() < 1e-10);
    }

    #[test]
    fn test_two_children_width_sums() {
        let a = LayoutBox {
            width: 0.5,
            height: 0.43,
            depth: 0.0,
            content: BoxContent::Empty,
            color: ratex_types::color::Color::BLACK,
        };
        let b = LayoutBox {
            width: 0.6,
            height: 0.69,
            depth: 0.1,
            content: BoxContent::Empty,
            color: ratex_types::color::Color::BLACK,
        };
        let hbox = make_hbox(vec![a, b]);
        assert!((hbox.width - 1.1).abs() < 1e-10);
        assert!((hbox.height - 0.69).abs() < 1e-10);
        assert!((hbox.depth - 0.1).abs() < 1e-10);
    }
}

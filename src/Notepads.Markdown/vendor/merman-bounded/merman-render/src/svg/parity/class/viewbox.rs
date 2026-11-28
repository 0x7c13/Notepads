use crate::model::Bounds;

pub(super) struct ClassViewBoxContext<'a> {
    pub content_bounds: Option<Bounds>,
    pub viewport_padding: f64,
    pub diagram_title: Option<&'a str>,
    pub diagram_title_bbox_x: Option<(f64, f64)>,
}

pub(super) struct ClassViewBox<'a> {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
    pub title: Option<ClassViewBoxTitle<'a>>,
}

pub(super) struct ClassViewBoxTitle<'a> {
    pub text: &'a str,
    pub x: f64,
    pub y: f64,
}

pub(super) fn class_viewbox(ctx: ClassViewBoxContext<'_>) -> ClassViewBox<'_> {
    let mut bounds = ctx.content_bounds.unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    });
    let content_center_x = bounds.min_x + (bounds.max_x - bounds.min_x) / 2.0;
    let has_title = ctx
        .diagram_title
        .is_some_and(|title| !title.trim().is_empty());
    if has_title && let Some((left, right)) = ctx.diagram_title_bbox_x {
        bounds.min_x = bounds.min_x.min(content_center_x - left.max(0.0));
        bounds.max_x = bounds.max_x.max(content_center_x + right.max(0.0));
    }
    let min_x = bounds.min_x - ctx.viewport_padding;
    let mut min_y = bounds.min_y - ctx.viewport_padding;
    let width = ((bounds.max_x - bounds.min_x) + 2.0 * ctx.viewport_padding).max(1.0);
    let mut height = ((bounds.max_y - bounds.min_y) + 2.0 * ctx.viewport_padding).max(1.0);

    // Mermaid renders the title outside the content wrapper and reserves a fixed block above it.
    const TITLE_BLOCK_HEIGHT_PX: f64 = 48.0;
    const TITLE_Y_OFFSET_FROM_VIEWBOX_TOP_PX: f64 = 23.0;
    if has_title {
        min_y -= TITLE_BLOCK_HEIGHT_PX;
        height += TITLE_BLOCK_HEIGHT_PX;
    }

    let title = has_title.then(|| {
        let text = ctx.diagram_title.unwrap_or_default().trim();
        ClassViewBoxTitle {
            text,
            x: content_center_x,
            y: min_y + TITLE_Y_OFFSET_FROM_VIEWBOX_TOP_PX,
        }
    });

    ClassViewBox {
        min_x,
        min_y,
        width,
        height,
        title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn right(viewbox: &ClassViewBox<'_>) -> f64 {
        viewbox.min_x + viewbox.width
    }

    fn bottom(viewbox: &ClassViewBox<'_>) -> f64 {
        viewbox.min_y + viewbox.height
    }

    #[test]
    fn viewbox_is_derived_from_content_bounds_and_padding() {
        let bounds = Bounds {
            min_x: -12.0,
            min_y: 7.0,
            max_x: 83.0,
            max_y: 149.0,
        };
        let padding = 11.0;

        let viewbox = class_viewbox(ClassViewBoxContext {
            content_bounds: Some(bounds.clone()),
            viewport_padding: padding,
            diagram_title: None,
            diagram_title_bbox_x: None,
        });

        assert_eq!(bounds.min_x - viewbox.min_x, padding);
        assert_eq!(bounds.min_y - viewbox.min_y, padding);
        assert_eq!(right(&viewbox) - bounds.max_x, padding);
        assert_eq!(bottom(&viewbox) - bounds.max_y, padding);
        assert!(viewbox.title.is_none());
    }

    #[test]
    fn title_expands_only_the_top_and_stays_centered() {
        let context = |title| ClassViewBoxContext {
            content_bounds: Some(Bounds {
                min_x: 20.0,
                min_y: 30.0,
                max_x: 220.0,
                max_y: 130.0,
            }),
            viewport_padding: 8.0,
            diagram_title: title,
            diagram_title_bbox_x: title.map(|_| (100.0, 100.0)),
        };
        let without_title = class_viewbox(context(None));
        let with_title = class_viewbox(context(Some("  Diagram title  ")));
        let title = with_title.title.as_ref().expect("title geometry");

        assert_eq!(with_title.min_x, without_title.min_x);
        assert_eq!(with_title.width, without_title.width);
        assert_eq!(bottom(&with_title), bottom(&without_title));
        assert!(with_title.min_y < without_title.min_y);
        assert!(with_title.height > without_title.height);
        assert_eq!(title.text, "Diagram title");
        assert_eq!(title.x, 120.0);
        assert!(title.y > with_title.min_y && title.y < without_title.min_y);
    }

    #[test]
    fn empty_svg_uses_zero_bbox_plus_padding() {
        let viewbox = class_viewbox(ClassViewBoxContext {
            content_bounds: None,
            viewport_padding: 8.0,
            diagram_title: None,
            diagram_title_bbox_x: None,
        });

        assert_eq!(viewbox.min_x, -8.0);
        assert_eq!(viewbox.min_y, -8.0);
        assert_eq!(viewbox.width, 16.0);
        assert_eq!(viewbox.height, 16.0);
    }

    #[test]
    fn title_bbox_expands_horizontally_around_original_content_center() {
        let viewbox = class_viewbox(ClassViewBoxContext {
            content_bounds: Some(Bounds {
                min_x: 80.0,
                min_y: 8.0,
                max_x: 160.0,
                max_y: 92.0,
            }),
            viewport_padding: 8.0,
            diagram_title: Some("A much wider title"),
            diagram_title_bbox_x: Some((75.0, 73.0)),
        });
        let title = viewbox.title.expect("title geometry");

        assert_eq!(title.x, 120.0);
        assert_eq!(viewbox.min_x, 37.0);
        assert_eq!(viewbox.width, 164.0);
    }
}

use crate::model::Bounds;

use super::super::{root_svg, svg_emitted_bounds_from_svg};

pub(super) struct ArchitectureRootViewportContext<'a, 'id> {
    pub(super) out: String,
    pub(super) root_viewport: &'a root_svg::RootViewportContext<'id>,
    pub(super) root_document: root_svg::RootDocument,
    pub(super) content_bounds: Option<Bounds>,
    pub(super) padding_px: f64,
    pub(super) half_icon: f64,
    pub(super) icon_size_px: f64,
    pub(super) use_max_width: bool,
    pub(super) is_empty: bool,
    pub(super) trust_content_bounds: bool,
}

fn architecture_root_bbox_from_svg(
    out: &str,
    content_bounds: Option<Bounds>,
    icon_size_px: f64,
    trust_content_bounds: bool,
) -> Bounds {
    let content_bounds_fallback = content_bounds.as_ref().cloned().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: icon_size_px,
        max_y: icon_size_px,
    });

    if trust_content_bounds && content_bounds.is_some() {
        return content_bounds_fallback;
    }

    let mut bounds = svg_emitted_bounds_from_svg(out).unwrap_or(content_bounds_fallback);

    // Architecture labels are rendered as `<text>` without explicit bbox geometry. Our emitted SVG
    // bbox pass cannot see those label extents, so union the headless label bounds before applying
    // Mermaid's root `getBBox() + padding` behavior.
    if let Some(content_bounds) = content_bounds {
        bounds.min_x = bounds.min_x.min(content_bounds.min_x);
        bounds.min_y = bounds.min_y.min(content_bounds.min_y);
        bounds.max_x = bounds.max_x.max(content_bounds.max_x);
        bounds.max_y = bounds.max_y.max(content_bounds.max_y);
    }

    bounds
}

pub(super) fn finalize_architecture_root_viewport(
    ctx: ArchitectureRootViewportContext<'_, '_>,
) -> crate::Result<root_svg::RootedSvg> {
    let ArchitectureRootViewportContext {
        mut out,
        root_viewport,
        root_document,
        content_bounds,
        padding_px,
        half_icon,
        icon_size_px,
        use_max_width,
        is_empty,
        trust_content_bounds,
    } = ctx;

    let root_bounds = if is_empty {
        root_svg::DiagramBounds::from_view_box(-half_icon, -half_icon, icon_size_px, icon_size_px)
    } else {
        let bounds = architecture_root_bbox_from_svg(
            &out,
            content_bounds,
            icon_size_px,
            trust_content_bounds,
        );
        root_svg::DiagramBounds::from_extents(
            bounds.min_x,
            bounds.min_y,
            bounds.max_x,
            bounds.max_y,
            padding_px,
        )
    };
    let root_spec = root_svg::RootViewportSpec::mermaid_or_intrinsic(root_bounds, use_max_width);
    let root_document = root_viewport.finish_document(&mut out, root_document, root_spec)?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Bounds {
        Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    #[test]
    fn architecture_root_bbox_uses_content_bounds_when_svg_bbox_is_unavailable() {
        let content = bounds(-10.0, -20.0, 30.0, 40.0);

        let b = architecture_root_bbox_from_svg("<not-svg", Some(content.clone()), 80.0, false);

        assert_eq!(b.min_x, content.min_x);
        assert_eq!(b.min_y, content.min_y);
        assert_eq!(b.max_x, content.max_x);
        assert_eq!(b.max_y, content.max_y);
    }

    #[test]
    fn architecture_root_bbox_can_trust_accumulated_content_bounds() {
        let content = bounds(1.0, 2.0, 3.0, 4.0);
        let svg = r#"<svg><rect x="-100" y="-200" width="300" height="400"/></svg>"#;

        let b = architecture_root_bbox_from_svg(svg, Some(content.clone()), 80.0, true);

        assert_eq!(b.min_x, content.min_x);
        assert_eq!(b.min_y, content.min_y);
        assert_eq!(b.max_x, content.max_x);
        assert_eq!(b.max_y, content.max_y);
    }

    #[test]
    fn architecture_root_bbox_scans_svg_when_content_bounds_are_not_trusted() {
        let content = bounds(1.0, 2.0, 3.0, 4.0);
        let svg = r#"<svg><rect x="-100" y="-200" width="300" height="400"/></svg>"#;

        let b = architecture_root_bbox_from_svg(svg, Some(content), 80.0, false);

        assert_eq!(b.min_x, -100.0);
        assert_eq!(b.min_y, -200.0);
        assert_eq!(b.max_x, 200.0);
        assert_eq!(b.max_y, 200.0);
    }

    #[test]
    fn architecture_root_viewport_preserves_f64_bbox_and_padding() {
        let diagram_id = "architecture-f64-root";
        let root_viewport = root_svg::RootViewportContext::new(
            crate::family::RenderFamilyKind::Architecture,
            diagram_id,
        );
        let mut out = String::new();
        let root_document = root_viewport
            .begin_document(
                &mut out,
                root_svg::DeferredRootSpec::mermaid_or_intrinsic(true),
                root_svg::RootChrome::new(diagram_id, "architecture"),
            )
            .unwrap();
        out.push_str("</svg>");
        let content = bounds(
            1.123_456_789,
            2.123_456_789,
            111.987_654_321,
            222.987_654_321,
        );
        let padding = 40.0;

        let svg = finalize_architecture_root_viewport(ArchitectureRootViewportContext {
            out,
            root_viewport: &root_viewport,
            root_document,
            content_bounds: Some(content.clone()),
            padding_px: padding,
            half_icon: 40.0,
            icon_size_px: 80.0,
            use_max_width: true,
            is_empty: false,
            trust_content_bounds: true,
        })
        .unwrap()
        .into_string_for(crate::family::RenderFamilyKind::Architecture)
        .unwrap();
        let view_box = svg
            .split_once("viewBox=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0
            .split_whitespace()
            .map(|part| part.parse::<f64>().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            view_box,
            vec![
                content.min_x - padding,
                content.min_y - padding,
                content.max_x - content.min_x + 2.0 * padding,
                content.max_y - content.min_y + 2.0 * padding,
            ]
        );
    }
}

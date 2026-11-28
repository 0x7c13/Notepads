use super::super::*;

pub(super) fn class_marker_name(ty: i32, is_start: bool) -> Option<&'static str> {
    // Mermaid class diagram relationType constants.
    // -1 = none, 0 = aggregation, 1 = extension, 2 = composition, 3 = dependency, 4 = lollipop
    match ty {
        0 => Some(if is_start {
            "aggregationStart"
        } else {
            "aggregationEnd"
        }),
        1 => Some(if is_start {
            "extensionStart"
        } else {
            "extensionEnd"
        }),
        2 => Some(if is_start {
            "compositionStart"
        } else {
            "compositionEnd"
        }),
        3 => Some(if is_start {
            "dependencyStart"
        } else {
            "dependencyEnd"
        }),
        4 => Some(if is_start {
            "lollipopStart"
        } else {
            "lollipopEnd"
        }),
        _ => None,
    }
}

pub(super) fn class_markers(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    diagram_marker_class: &str,
    include_margin_markers: bool,
) {
    // Match Mermaid unified output: multiple <defs> wrappers, one marker each.
    struct MarkerContext<'a> {
        out: &'a mut String,
        diagram_id: SvgDiagramId<'a>,
        diagram_marker_class: &'a str,
    }

    enum MarkerShape<'a> {
        Path(&'a str),
        StyledPath {
            d: &'a str,
            view_box: Option<&'a str>,
            stroke_width: &'a str,
        },
        Polygon(&'a str),
        Circle {
            stroke: Option<&'a str>,
            stroke_width: Option<&'a str>,
        },
    }

    struct MarkerSpec<'a> {
        name: &'a str,
        kind: &'a str,
        ref_x: &'a str,
        ref_y: &'a str,
        marker_w: &'a str,
        marker_h: &'a str,
        marker_units: Option<&'a str>,
        view_box: Option<&'a str>,
        wrap_defs: bool,
        shape: MarkerShape<'a>,
    }

    fn marker(ctx: &mut MarkerContext<'_>, spec: MarkerSpec<'_>) {
        if spec.wrap_defs {
            ctx.out.push_str("<defs>");
        }
        match spec.shape {
            MarkerShape::Path(d) | MarkerShape::StyledPath { d, .. } => {
                let _ = write!(
                    ctx.out,
                    r#"<marker id="{}_{}-{}" class="marker {} {}" refX="{}" refY="{}" markerWidth="{}" markerHeight="{}" orient="auto""#,
                    ctx.diagram_id,
                    escape_xml_display(ctx.diagram_marker_class),
                    escape_xml_display(spec.name),
                    escape_xml_display(spec.kind),
                    escape_xml_display(ctx.diagram_marker_class),
                    spec.ref_x,
                    spec.ref_y,
                    spec.marker_w,
                    spec.marker_h,
                );
                if let Some(marker_units) = spec.marker_units {
                    let _ = write!(ctx.out, r#" markerUnits="{}""#, marker_units);
                }
                if let Some(view_box) = spec.view_box {
                    let _ = write!(ctx.out, r#" viewBox="{}""#, view_box);
                }
                let _ = write!(ctx.out, r#"><path d="{}""#, escape_xml_display(d));
                if let MarkerShape::StyledPath {
                    view_box,
                    stroke_width,
                    ..
                } = spec.shape
                {
                    if let Some(view_box) = view_box {
                        let _ = write!(ctx.out, r#" viewBox="{}""#, view_box);
                    }
                    let _ = write!(ctx.out, r#" style="stroke-width: {};""#, stroke_width);
                }
                ctx.out.push_str("/></marker>");
            }
            MarkerShape::Polygon(points) => {
                let _ = write!(
                    ctx.out,
                    r#"<marker id="{}_{}-{}" class="marker {} {}" refX="{}" refY="{}" markerWidth="{}" markerHeight="{}" orient="auto""#,
                    ctx.diagram_id,
                    escape_xml_display(ctx.diagram_marker_class),
                    escape_xml_display(spec.name),
                    escape_xml_display(spec.kind),
                    escape_xml_display(ctx.diagram_marker_class),
                    spec.ref_x,
                    spec.ref_y,
                    spec.marker_w,
                    spec.marker_h,
                );
                if let Some(marker_units) = spec.marker_units {
                    let _ = write!(ctx.out, r#" markerUnits="{}""#, marker_units);
                }
                if let Some(view_box) = spec.view_box {
                    let _ = write!(ctx.out, r#" viewBox="{}""#, view_box);
                }
                let _ = write!(
                    ctx.out,
                    r#"><polygon points="{}" style="stroke-width: 2; stroke-dasharray: 0;"/></marker>"#,
                    escape_xml_display(points)
                );
            }
            MarkerShape::Circle {
                stroke,
                stroke_width,
            } => {
                let _ = write!(
                    ctx.out,
                    r#"<marker id="{}_{}-{}" class="marker {} {}" refX="{}" refY="{}" markerWidth="{}" markerHeight="{}" orient="auto""#,
                    ctx.diagram_id,
                    escape_xml_display(ctx.diagram_marker_class),
                    escape_xml_display(spec.name),
                    escape_xml_display(spec.kind),
                    escape_xml_display(ctx.diagram_marker_class),
                    spec.ref_x,
                    spec.ref_y,
                    spec.marker_w,
                    spec.marker_h,
                );
                if let Some(marker_units) = spec.marker_units {
                    let _ = write!(ctx.out, r#" markerUnits="{}""#, marker_units);
                }
                if let Some(view_box) = spec.view_box {
                    let _ = write!(ctx.out, r#" viewBox="{}""#, view_box);
                }
                ctx.out
                    .push_str(r#"><circle fill="transparent" cx="7" cy="7" r="6""#);
                if let Some(stroke) = stroke {
                    let _ = write!(ctx.out, r#" stroke="{}""#, escape_xml_display(stroke));
                }
                if let Some(stroke_width) = stroke_width {
                    let _ = write!(ctx.out, r#" stroke-width="{}""#, stroke_width);
                }
                ctx.out.push_str("/></marker>");
            }
        }
        if spec.wrap_defs {
            ctx.out.push_str("</defs>");
        }
    }

    let mut ctx = MarkerContext {
        out,
        diagram_id,
        diagram_marker_class,
    };

    marker(
        &mut ctx,
        MarkerSpec {
            name: "aggregationStart",
            kind: "aggregation",
            ref_x: "18",
            ref_y: "7",
            marker_w: "190",
            marker_h: "240",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 18,7 L9,13 L1,7 L9,1 Z"),
        },
    );
    marker(
        &mut ctx,
        MarkerSpec {
            name: "aggregationEnd",
            kind: "aggregation",
            ref_x: "1",
            ref_y: "7",
            marker_w: "20",
            marker_h: "28",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 18,7 L9,13 L1,7 L9,1 Z"),
        },
    );
    if include_margin_markers {
        marker(
            &mut ctx,
            MarkerSpec {
                name: "aggregationStart-margin",
                kind: "aggregation",
                ref_x: "15",
                ref_y: "7",
                marker_w: "190",
                marker_h: "240",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 18,7 L9,13 L1,7 L9,1 Z",
                    view_box: None,
                    stroke_width: "2",
                },
            },
        );
        marker(
            &mut ctx,
            MarkerSpec {
                name: "aggregationEnd-margin",
                kind: "aggregation",
                ref_x: "1",
                ref_y: "7",
                marker_w: "20",
                marker_h: "28",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 18,7 L9,13 L1,7 L9,1 Z",
                    view_box: None,
                    stroke_width: "2",
                },
            },
        );
    }

    marker(
        &mut ctx,
        MarkerSpec {
            name: "extensionStart",
            kind: "extension",
            ref_x: "18",
            ref_y: "7",
            marker_w: "20",
            marker_h: "28",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 1,7 L18,13 V 1 Z"),
        },
    );
    marker(
        &mut ctx,
        MarkerSpec {
            name: "extensionEnd",
            kind: "extension",
            ref_x: "1",
            ref_y: "7",
            marker_w: "20",
            marker_h: "28",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 1,1 V 13 L18,7 Z"),
        },
    );
    if include_margin_markers {
        marker(
            &mut ctx,
            MarkerSpec {
                name: "extensionStart-margin",
                kind: "extension",
                ref_x: "18",
                ref_y: "7",
                marker_w: "20",
                marker_h: "28",
                marker_units: Some("userSpaceOnUse"),
                view_box: Some("0 0 20 14"),
                wrap_defs: false,
                shape: MarkerShape::Polygon("10,7 18,13 18,1"),
            },
        );
        marker(
            &mut ctx,
            MarkerSpec {
                name: "extensionEnd-margin",
                kind: "extension",
                ref_x: "9",
                ref_y: "7",
                marker_w: "20",
                marker_h: "28",
                marker_units: Some("userSpaceOnUse"),
                view_box: Some("0 0 20 14"),
                wrap_defs: true,
                shape: MarkerShape::Polygon("10,1 10,13 18,7"),
            },
        );
    }

    marker(
        &mut ctx,
        MarkerSpec {
            name: "compositionStart",
            kind: "composition",
            ref_x: "18",
            ref_y: "7",
            marker_w: "190",
            marker_h: "240",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 18,7 L9,13 L1,7 L9,1 Z"),
        },
    );
    marker(
        &mut ctx,
        MarkerSpec {
            name: "compositionEnd",
            kind: "composition",
            ref_x: "1",
            ref_y: "7",
            marker_w: "20",
            marker_h: "28",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 18,7 L9,13 L1,7 L9,1 Z"),
        },
    );
    if include_margin_markers {
        marker(
            &mut ctx,
            MarkerSpec {
                name: "compositionStart-margin",
                kind: "composition",
                ref_x: "15",
                ref_y: "7",
                marker_w: "190",
                marker_h: "240",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 18,7 L9,13 L1,7 L9,1 Z",
                    view_box: Some("0 0 15 15"),
                    stroke_width: "0",
                },
            },
        );
        marker(
            &mut ctx,
            MarkerSpec {
                name: "compositionEnd-margin",
                kind: "composition",
                ref_x: "3.5",
                ref_y: "7",
                marker_w: "20",
                marker_h: "28",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 18,7 L9,13 L1,7 L9,1 Z",
                    view_box: None,
                    stroke_width: "0",
                },
            },
        );
    }

    marker(
        &mut ctx,
        MarkerSpec {
            name: "dependencyStart",
            kind: "dependency",
            ref_x: "6",
            ref_y: "7",
            marker_w: "190",
            marker_h: "240",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 5,7 L9,13 L1,7 L9,1 Z"),
        },
    );
    marker(
        &mut ctx,
        MarkerSpec {
            name: "dependencyEnd",
            kind: "dependency",
            ref_x: "13",
            ref_y: "7",
            marker_w: "20",
            marker_h: "28",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Path("M 18,7 L9,13 L14,7 L9,1 Z"),
        },
    );
    if include_margin_markers {
        marker(
            &mut ctx,
            MarkerSpec {
                name: "dependencyStart-margin",
                kind: "dependency",
                ref_x: "4",
                ref_y: "7",
                marker_w: "190",
                marker_h: "240",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 5,7 L9,13 L1,7 L9,1 Z",
                    view_box: None,
                    stroke_width: "0",
                },
            },
        );
        marker(
            &mut ctx,
            MarkerSpec {
                name: "dependencyEnd-margin",
                kind: "dependency",
                ref_x: "16",
                ref_y: "7",
                marker_w: "20",
                marker_h: "28",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::StyledPath {
                    d: "M 18,7 L9,13 L14,7 L9,1 Z",
                    view_box: None,
                    stroke_width: "0",
                },
            },
        );
    }

    marker(
        &mut ctx,
        MarkerSpec {
            name: "lollipopStart",
            kind: "lollipop",
            ref_x: "13",
            ref_y: "7",
            marker_w: "190",
            marker_h: "240",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Circle {
                stroke: (!include_margin_markers).then_some("black"),
                stroke_width: None,
            },
        },
    );
    marker(
        &mut ctx,
        MarkerSpec {
            name: "lollipopEnd",
            kind: "lollipop",
            ref_x: "1",
            ref_y: "7",
            marker_w: "190",
            marker_h: "240",
            marker_units: Some("userSpaceOnUse"),
            view_box: None,
            wrap_defs: true,
            shape: MarkerShape::Circle {
                stroke: (!include_margin_markers).then_some("black"),
                stroke_width: None,
            },
        },
    );
    if include_margin_markers {
        marker(
            &mut ctx,
            MarkerSpec {
                name: "lollipopStart-margin",
                kind: "lollipop",
                ref_x: "13",
                ref_y: "7",
                marker_w: "190",
                marker_h: "240",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::Circle {
                    stroke: None,
                    stroke_width: Some("2"),
                },
            },
        );
        marker(
            &mut ctx,
            MarkerSpec {
                name: "lollipopEnd-margin",
                kind: "lollipop",
                ref_x: "1",
                ref_y: "7",
                marker_w: "190",
                marker_h: "240",
                marker_units: Some("userSpaceOnUse"),
                view_box: None,
                wrap_defs: true,
                shape: MarkerShape::Circle {
                    stroke: None,
                    stroke_width: Some("2"),
                },
            },
        );
    }
}

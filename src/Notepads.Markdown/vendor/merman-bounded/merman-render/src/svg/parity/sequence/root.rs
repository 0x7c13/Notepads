use super::super::*;
use super::model::SequenceSvgModel;

pub(super) struct SequenceRootMetrics {
    pub(super) viewbox_width: f64,
    pub(super) document: root_svg::RootDocument,
}

pub(super) fn write_sequence_svg_root_open(
    out: &mut String,
    layout: &SequenceDiagramLayout,
    model: &SequenceSvgModel,
    diagram_id: SvgDiagramId<'_>,
) -> Result<SequenceRootMetrics> {
    let bounds = layout.bounds.clone().unwrap_or(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 100.0,
        max_y: 100.0,
    });
    let root_bounds = root_svg::DiagramBounds::from_extents(
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
        0.0,
    );

    let aria_labelledby = model
        .acc_title
        .as_deref()
        .map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = model
        .acc_descr
        .as_deref()
        .map(|_| format!("chart-desc-{diagram_id}"));
    let root_spec = root_svg::RootViewportSpec::responsive(root_bounds);
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "sequence");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    let document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Sequence, diagram_id)
            .write_open(out, root_spec, root_chrome)?;

    if let Some(title) = model.acc_title.as_deref() {
        let _ = write!(
            out,
            r#"<title id="chart-title-{id}">{text}</title>"#,
            id = diagram_id,
            text = escape_xml_display(title)
        );
    }
    if let Some(desc) = model.acc_descr.as_deref() {
        let _ = write!(
            out,
            r#"<desc id="chart-desc-{id}">{text}</desc>"#,
            id = diagram_id,
            text = escape_xml_display(desc)
        );
    }

    Ok(SequenceRootMetrics {
        viewbox_width: root_bounds.width,
        document,
    })
}

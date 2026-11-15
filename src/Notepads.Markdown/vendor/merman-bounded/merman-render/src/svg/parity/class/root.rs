use super::*;

pub(super) const CLASS_GRAPH_MARGIN_PX: f64 = 8.0;

pub(super) struct ClassSvgDocument {
    pub(super) root: root_svg::RootDocument,
}

pub(super) fn begin_class_svg_document(
    out: &mut String,
    model: &ClassSvgModel,
    diagram_id: SvgDiagramId<'_>,
    aria_roledescription: &str,
    root_context: &root_svg::RootViewportContext<'_>,
) -> crate::Result<ClassSvgDocument> {
    let has_acc_title = model
        .acc_title
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_acc_descr = model
        .acc_descr
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());

    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, aria_roledescription);
    root_chrome.class = Some("classDiagram");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    root_chrome.dom.aria_attr_order = root_svg::SvgRootAriaAttrOrder::LabelledbyThenDescribedby;
    let document =
        root_context.begin_document(out, root_svg::DeferredRootSpec::responsive(), root_chrome)?;

    if has_acc_title {
        let _ = write!(
            out,
            r#"<title id="chart-title-{}">{}"#,
            diagram_id,
            escape_xml_display(model.acc_title.as_deref().unwrap_or_default())
        );
        out.push_str("</title>");
    }
    if has_acc_descr {
        let _ = write!(
            out,
            r#"<desc id="chart-desc-{}">{}"#,
            diagram_id,
            escape_xml_display(model.acc_descr.as_deref().unwrap_or_default())
        );
        out.push_str("</desc>");
    }

    Ok(ClassSvgDocument { root: document })
}

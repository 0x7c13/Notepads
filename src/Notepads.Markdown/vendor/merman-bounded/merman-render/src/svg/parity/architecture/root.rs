use std::fmt::Write as _;

use super::super::{SvgDiagramId, escape_xml_display, root_svg};

pub(super) struct ArchitectureA11y {
    pub(super) aria_labelledby: Option<String>,
    pub(super) aria_describedby: Option<String>,
    pub(super) nodes: String,
}

pub(super) fn architecture_a11y_nodes(
    diagram_id: SvgDiagramId<'_>,
    acc_title: Option<&str>,
    acc_descr: Option<&str>,
) -> ArchitectureA11y {
    let aria_labelledby = acc_title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|_| format!("chart-desc-{diagram_id}"));

    let mut nodes = String::new();
    if let Some(t) = acc_title.map(str::trim).filter(|t| !t.is_empty()) {
        let _ = write!(
            &mut nodes,
            r#"<title id="chart-title-{}">{}</title>"#,
            diagram_id,
            escape_xml_display(t)
        );
    }
    if let Some(d) = acc_descr.map(str::trim).filter(|t| !t.is_empty()) {
        let _ = write!(
            &mut nodes,
            r#"<desc id="chart-desc-{}">{}</desc>"#,
            diagram_id,
            escape_xml_display(d)
        );
    }

    ArchitectureA11y {
        aria_labelledby,
        aria_describedby,
        nodes,
    }
}

pub(super) fn begin_architecture_document(
    out: &mut String,
    root_viewport: &root_svg::RootViewportContext<'_>,
    diagram_id: SvgDiagramId<'_>,
    css: &str,
    a11y: &ArchitectureA11y,
    use_max_width: bool,
) -> crate::Result<root_svg::RootDocument> {
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "architecture");
    root_chrome.aria_labelledby = a11y.aria_labelledby.as_deref();
    root_chrome.aria_describedby = a11y.aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;
    let document = root_viewport.begin_document(
        out,
        root_svg::DeferredRootSpec::mermaid_or_intrinsic(use_max_width),
        root_chrome,
    )?;

    out.push_str(a11y.nodes.as_str());
    let _ = write!(out, "<style>{}</style>", css);
    out.push_str("<g/><g class=\"architecture-edges\">");
    Ok(document)
}

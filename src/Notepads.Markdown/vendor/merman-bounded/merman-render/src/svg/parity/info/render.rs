use super::super::*;
pub(crate) fn render_info_diagram_svg(
    layout: &InfoDiagramLayout,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("merman");

    let mut out = String::new();
    let root_spec = root_svg::RootViewportSpec::responsive_without_view_box(400.0);
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "info");
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Info, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;
    let css = info_css_with_config(diagram_id, effective_config);
    let _ = write!(&mut out, r#"<style>{}</style>"#, css);
    out.push_str(r#"<g/>"#);
    options.checkpoint_emit()?;
    let _ = write!(
        &mut out,
        r#"<g><text x="100" y="40" class="version" font-size="32" style="text-anchor: middle;">{}</text></g>"#,
        escape_xml(&layout.version)
    );
    out.push_str("</svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

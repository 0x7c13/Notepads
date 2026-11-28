use super::super::*;

// Error diagram SVG renderer implementation (split from parity.rs).

pub(crate) fn render_error_diagram_svg_model(
    layout: &ErrorDiagramLayout,
    semantic: &merman_core::diagrams::error_diagram::ErrorDiagramRenderModel,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    render_error_diagram_svg_inner(layout, semantic, effective_config, options)
}

fn render_error_diagram_svg_inner(
    layout: &ErrorDiagramLayout,
    semantic: &merman_core::diagrams::error_diagram::ErrorDiagramRenderModel,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("merman");

    let mut out = String::new();
    let root_bounds = root_svg::DiagramBounds::from_view_box(
        0.0,
        0.0,
        layout.viewbox_width,
        layout.viewbox_height,
    );
    let root_spec = root_svg::RootViewportSpec::responsive(root_bounds)
        .with_max_width(root_svg::RootMaxWidth::SvgNumber(layout.max_width_px));
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "error");
    root_chrome.dom.style_viewbox_order = root_svg::SvgRootStyleViewBoxOrder::ViewBoxThenStyle;
    root_chrome.dom.trailing_newline = false;
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Error, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    options.checkpoint_emit()?;
    let css = info_css_with_config(diagram_id, effective_config);
    let _ = write!(&mut out, "<style>{}</style>", css);
    out.push_str(r#"<g/>"#);
    options.checkpoint_emit()?;
    out.push_str(r#"<g>"#);
    out.push_str(r#"<path class="error-icon" d="m411.313,123.313c6.25-6.25 6.25-16.375 0-22.625s-16.375-6.25-22.625,0l-32,32-9.375,9.375-20.688-20.688c-12.484-12.5-32.766-12.5-45.25,0l-16,16c-1.261,1.261-2.304,2.648-3.31,4.051-21.739-8.561-45.324-13.426-70.065-13.426-105.867,0-192,86.133-192,192s86.133,192 192,192 192-86.133 192-192c0-24.741-4.864-48.327-13.426-70.065 1.402-1.007 2.79-2.049 4.051-3.31l16-16c12.5-12.492 12.5-32.758 0-45.25l-20.688-20.688 9.375-9.375 32.001-31.999zm-219.313,100.687c-52.938,0-96,43.063-96,96 0,8.836-7.164,16-16,16s-16-7.164-16-16c0-70.578 57.422-128 128-128 8.836,0 16,7.164 16,16s-7.164,16-16,16z"/>"#);
    out.push_str(r#"<path class="error-icon" d="m459.02,148.98c-6.25-6.25-16.375-6.25-22.625,0s-6.25,16.375 0,22.625l16,16c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688 6.25-6.25 6.25-16.375 0-22.625l-16.001-16z"/>"#);
    out.push_str(r#"<path class="error-icon" d="m340.395,75.605c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688 6.25-6.25 6.25-16.375 0-22.625l-16-16c-6.25-6.25-16.375-6.25-22.625,0s-6.25,16.375 0,22.625l15.999,16z"/>"#);
    out.push_str(r#"<path class="error-icon" d="m400,64c8.844,0 16-7.164 16-16v-32c0-8.836-7.156-16-16-16-8.844,0-16,7.164-16,16v32c0,8.836 7.156,16 16,16z"/>"#);
    out.push_str(r#"<path class="error-icon" d="m496,96.586h-32c-8.844,0-16,7.164-16,16 0,8.836 7.156,16 16,16h32c8.844,0 16-7.164 16-16 0-8.836-7.156-16-16-16z"/>"#);
    out.push_str(r#"<path class="error-icon" d="m436.98,75.605c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688l32-32c6.25-6.25 6.25-16.375 0-22.625s-16.375-6.25-22.625,0l-32,32c-6.251,6.25-6.251,16.375-0.001,22.625z"/>"#);
    out.push_str(r#"<text class="error-text" x="1440" y="250" font-size="150px" style="text-anchor: middle;">Syntax error in text</text>"#);
    let _ = write!(
        &mut out,
        r#"<text class="error-text" x="1250" y="400" font-size="100px" style="text-anchor: middle;">mermaid version {}</text>"#,
        crate::error::UPSTREAM_MERMAID_VERSION
    );
    for (index, line) in
        crate::error::wrap_error_message(semantic.error_message.as_deref().unwrap_or_default())
            .iter()
            .enumerate()
    {
        let _ = write!(
            &mut out,
            r#"<text class="error-text" x="1440" y="{}" font-size="42px" style="text-anchor: middle;">"#,
            510 + index * 56,
        );
        util::escape_xml_serialized_text_into(&mut out, line);
        out.push_str("</text>");
    }
    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use merman_core::diagrams::error_diagram::ErrorDiagramRenderModel;

    fn render_message(message: Option<&str>) -> String {
        let model = ErrorDiagramRenderModel {
            diagram_type: "error".to_string(),
            error_message: message.map(str::to_string),
        };
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let request = SvgRenderOptions::default();
        let debug = SvgDebugOptions::default();
        let execution = SvgExecution::new(&request, &debug, &session).unwrap();
        let config = serde_json::json!({});
        let layout =
            crate::error::layout_error_diagram_typed(&model, &config, execution.text_measurer())
                .unwrap();
        render_error_diagram_svg_model(&layout, &model, &config, &execution)
            .unwrap()
            .into_string_for(crate::family::RenderFamilyKind::Error)
            .unwrap()
    }

    #[test]
    fn error_svg_without_a_message_preserves_the_original_graphic() {
        let svg = render_message(None);
        assert!(svg.contains(r#"viewBox="0 0 2412 512""#));
        assert!(svg.contains("Syntax error in text"));
        assert_eq!(svg.matches(r#"font-size="42px""#).count(), 0);
    }

    #[test]
    fn error_svg_shows_literal_error_text_and_grows_the_viewport() {
        let svg = render_message(Some("Unexpected <bad>& #abcdef; #60;"));
        assert!(svg.contains(r#"viewBox="0 0 2412 556""#));
        assert!(svg.contains(r#"max-width: 556px"#));
        assert!(svg.contains("Unexpected &lt;bad&gt;&amp; #abcdef; #60;</text>"));
        assert!(svg.contains(r#"x="1440" y="510" font-size="42px""#));
    }

    #[test]
    fn error_svg_emits_at_most_four_message_lines() {
        let svg = render_message(Some(&"x".repeat(500)));
        assert!(svg.contains(r#"viewBox="0 0 2412 724""#));
        assert_eq!(svg.matches(r#"font-size="42px""#).count(), 4);
        assert!(svg.contains(r#"y="678" font-size="42px""#));
        assert!(svg.contains(&format!("{}...</text>", "x".repeat(72))));
    }
}

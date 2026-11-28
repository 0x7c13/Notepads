use super::{SvgPostprocessExecution, SvgPostprocessMetadata, checkpoint_loop};
use crate::family::RenderFamilyKind;
use crate::{Error, Result};
use std::borrow::Cow;

/// Removes the redundant background ID emitted by Mermaid's default Mindmap node shape.
///
/// Raw parity retains the upstream duplicate. Only renderer-certified Mindmap output receives
/// this narrowly shaped repair before embedding admission; arbitrary duplicate IDs still fail.
pub(super) fn normalize_renderer_ids<'a>(
    svg: Cow<'a, str>,
    metadata: &SvgPostprocessMetadata,
    execution: SvgPostprocessExecution<'_>,
) -> Result<Cow<'a, str>> {
    if metadata.family_kind() != Some(RenderFamilyKind::Mindmap) {
        return Ok(svg);
    }
    execution.checkpoint()?;
    let document = roxmltree::Document::parse(&svg)
        .map_err(|error| Error::svg_postprocess("normalize-embedding-ids", error.to_string()))?;
    let mut removed = Vec::new();
    for (index, node) in document.descendants().enumerate() {
        checkpoint_loop(index, &mut || execution.checkpoint())?;
        if !node.has_tag_name(("http://www.w3.org/2000/svg", "path"))
            || !node.attribute("class").is_some_and(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class| class == "node-bkg")
            })
        {
            continue;
        }
        let Some(parent) = node.parent_element() else {
            continue;
        };
        if !parent.has_tag_name(("http://www.w3.org/2000/svg", "g")) {
            continue;
        }
        let classes = parent.attribute("class").unwrap_or_default();
        if !classes
            .split_ascii_whitespace()
            .any(|class| class == "node")
            || !classes
                .split_ascii_whitespace()
                .any(|class| class == "mindmap-node")
        {
            continue;
        }
        let Some(attribute) = node
            .attributes()
            .find(|attribute| attribute.namespace().is_none() && attribute.name() == "id")
        else {
            continue;
        };
        if parent.attribute("id") == Some(attribute.value()) {
            removed.push(attribute.range());
        }
    }
    if removed.is_empty() {
        return Ok(svg);
    }
    execution.checkpoint()?;
    let mut output = String::with_capacity(svg.len());
    let mut cursor = 0;
    for (index, range) in removed.into_iter().enumerate() {
        checkpoint_loop(index, &mut || execution.checkpoint())?;
        output.push_str(&svg[cursor..range.start]);
        cursor = range.end;
    }
    output.push_str(&svg[cursor..]);
    execution.checkpoint()?;
    Ok(Cow::Owned(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::SvgPipeline;

    const MINDMAP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" id="diagram"><g class="node mindmap-node section-root" id="node"><path class="node-7 node-bkg" id="node"/></g><use href="#node"/></svg>"##;

    #[test]
    fn renderer_certified_mindmap_ids_are_isolated_for_every_embedding_mode() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let metadata =
            SvgPostprocessMetadata::from_svg(MINDMAP).with_family_kind(RenderFamilyKind::Mindmap);
        let raw = SvgPipeline::parity()
            .process_with_metadata(MINDMAP, &metadata, &session)
            .unwrap();
        assert_eq!(raw, MINDMAP);

        for pipeline in [
            SvgPipeline::parity().with_browser_inline_contract("embed"),
            SvgPipeline::parity().with_static_inline_contract("embed"),
            SvgPipeline::parity().with_rebased_ids("embed"),
        ] {
            let output = pipeline
                .process_with_metadata(MINDMAP, &metadata, &session)
                .unwrap();
            let document = roxmltree::Document::parse(&output).unwrap();
            let target = document
                .descendants()
                .filter(|node| node.attribute("id") == Some("embed-node"))
                .collect::<Vec<_>>();
            assert_eq!(target.len(), 1);
            assert!(target[0].has_tag_name("g"));
            assert!(output.contains("href=\"#embed-node\""));
        }
    }

    #[test]
    fn inferred_family_and_unrelated_duplicates_still_fail_closed() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let inferred = SvgPostprocessMetadata::from_svg(MINDMAP).with_diagram_type("mindmap");
        let certified = inferred.clone().with_family_kind(RenderFamilyKind::Mindmap);
        for (source, metadata) in [
            (MINDMAP.to_string(), inferred),
            (
                MINDMAP.replace("node-7 node-bkg", "unrelated"),
                certified.clone(),
            ),
            (
                MINDMAP.replace("</svg>", "<rect id=\"node\"/></svg>"),
                certified,
            ),
        ] {
            let error = SvgPipeline::parity()
                .with_browser_inline_contract("embed")
                .process_with_metadata(&source, &metadata, &session)
                .unwrap_err();
            assert!(error.to_string().contains("duplicate id"), "{error}");
        }
    }

    #[test]
    fn identity_repair_does_not_hide_active_content_from_admission() {
        let source = MINDMAP.replace("<path class", "<path onclick=\"run()\" class");
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let metadata =
            SvgPostprocessMetadata::from_svg(&source).with_family_kind(RenderFamilyKind::Mindmap);
        let error = SvgPipeline::parity()
            .with_browser_inline_contract("embed")
            .process_with_metadata(&source, &metadata, &session)
            .unwrap_err();
        assert!(error.to_string().contains("event attribute"), "{error}");
    }
}

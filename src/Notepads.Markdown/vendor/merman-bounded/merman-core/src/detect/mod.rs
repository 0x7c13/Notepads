use crate::{MermaidConfig, OperationControl, OperationControlResult, Result};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
#[error("No diagram type detected matching given configuration for text: {text}")]
pub struct DetectTypeError {
    /// Input after front-matter, directives, and Mermaid comments have been removed.
    pub text: String,
}

/// Predicate used by [`DetectorRegistry`] to recognize one Mermaid diagram family.
pub type DetectorFn = fn(text: &str, config: &mut MermaidConfig) -> bool;

/// One diagram detector entry.
#[derive(Debug, Clone)]
pub struct Detector {
    /// Mermaid diagram type id returned when the detector matches.
    pub id: &'static str,
    /// Detection predicate. It may read and update Mermaid config, matching upstream behavior.
    pub detector: DetectorFn,
}

/// Ordered registry that detects Mermaid diagram types.
///
/// Detector order is semantically significant because Mermaid registers overlapping diagram
/// syntaxes in a fixed order.
#[derive(Debug, Clone)]
pub struct DetectorRegistry {
    detectors: Arc<Vec<Detector>>,
}

impl DetectorRegistry {
    /// Creates an empty detector registry.
    pub fn new() -> Self {
        Self {
            detectors: Arc::new(Vec::new()),
        }
    }

    /// Adds a detector entry to the end of the ordered registry.
    pub fn add(&mut self, detector: Detector) {
        Arc::make_mut(&mut self.detectors).push(detector);
    }

    /// Adds a detector function to the end of the ordered registry.
    pub fn add_fn(&mut self, id: &'static str, detector: DetectorFn) {
        self.add(Detector { id, detector });
    }

    /// Detects a Mermaid diagram type after stripping front-matter, directives, and comments.
    pub fn detect_type(&self, text: &str, config: &mut MermaidConfig) -> Result<&'static str> {
        let control = OperationControl::new();
        self.detect_type_controlled(text, config, &control)
            .expect("a private parse control cannot be cancelled")
    }

    pub(crate) fn detect_type_controlled(
        &self,
        text: &str,
        config: &mut MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<Result<&'static str>> {
        control.checkpoint()?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let no_frontmatter = crate::preprocess::locate_frontmatter_block_controlled(text, control)?
            .map_or(text, |block| block.stripped);
        control.checkpoint()?;
        let no_directives = remove_directives_controlled(no_frontmatter, control)?;
        control.checkpoint()?;
        let cleaned = crate::utils::cleanup_mermaid_comments(no_directives.as_ref());
        control.checkpoint()?;

        for (index, det) in self.detectors.iter().enumerate() {
            if index % 16 == 0 {
                control.checkpoint()?;
            }
            if (det.detector)(cleaned.as_ref(), config) {
                return Ok(Ok(det.id));
            }
        }

        control.checkpoint()?;
        Ok(Err(DetectTypeError {
            text: cleaned.into_owned(),
        }
        .into()))
    }

    /// Detects a diagram type assuming the input is already pre-cleaned:
    /// no front-matter, no directives, and no Mermaid `%%` comments.
    pub fn detect_type_precleaned(
        &self,
        text: &str,
        config: &mut MermaidConfig,
    ) -> Result<&'static str> {
        let control = OperationControl::new();
        self.detect_type_precleaned_controlled(text, config, &control)
            .expect("a private parse control cannot be cancelled")
    }

    pub(crate) fn detect_type_precleaned_controlled(
        &self,
        text: &str,
        config: &mut MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<Result<&'static str>> {
        control.checkpoint()?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        for (index, det) in self.detectors.iter().enumerate() {
            if index % 16 == 0 {
                control.checkpoint()?;
            }
            if (det.detector)(text, config) {
                return Ok(Ok(det.id));
            }
        }

        control.checkpoint()?;
        Ok(Err(DetectTypeError {
            text: text.to_string(),
        }
        .into()))
    }

    /// Builds the detector registry for the pinned Mermaid baseline.
    pub fn pinned_mermaid_baseline() -> Self {
        let mut reg = Self::new();
        for fact in crate::family::detector_facts() {
            reg.add_fn(fact.id, fact.detector);
            if fact.id == "error" {
                reg.add_fn("---", detector_frontmatter_unparsed);
            }
        }

        reg
    }
    #[cfg(test)]
    pub(crate) fn detector_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.detectors.iter().map(|detector| detector.id)
    }
}

#[cfg(test)]
fn remove_directives(text: &str) -> Cow<'_, str> {
    let control = OperationControl::new();
    remove_directives_controlled(text, &control)
        .expect("a private parse control cannot be cancelled")
}

fn remove_directives_controlled<'a>(
    text: &'a str,
    control: &OperationControl,
) -> OperationControlResult<Cow<'a, str>> {
    control.checkpoint()?;
    let ranges = crate::preprocess::directive_removal_ranges_controlled(text, control)?;
    if ranges.is_empty() {
        return Ok(Cow::Borrowed(text));
    }

    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for (index, range) in ranges.into_iter().enumerate() {
        if index % 32 == 0 {
            control.checkpoint()?;
        }
        out.push_str(&text[pos..range.start]);
        pos = range.end;
    }
    out.push_str(&text[pos..]);
    control.checkpoint()?;
    Ok(Cow::Owned(out))
}

impl Default for DetectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn detector_frontmatter_unparsed(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("---")
}

pub(crate) fn detector_error(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim().eq_ignore_ascii_case("error")
}

pub(crate) fn detector_c4(txt: &str, _config: &mut MermaidConfig) -> bool {
    // Matches Mermaid's upstream regex exactly (note the missing grouping in JS).
    txt.trim_start_matches(char::is_whitespace)
        .starts_with("C4Context")
        || txt.contains("C4Container")
        || txt.contains("C4Component")
        || txt.contains("C4Dynamic")
        || txt.contains("C4Deployment")
}

pub(crate) fn detector_kanban(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("kanban")
}

pub(crate) fn detector_class_v2(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("classDiagram")
}

pub(crate) fn detector_er(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("erDiagram")
}

pub(crate) fn detector_gantt(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("gantt")
}

pub(crate) fn detector_info(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("info")
}

pub(crate) fn detector_pie(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("pie")
}

pub(crate) fn detector_requirement(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("requirement")
}

pub(crate) fn detector_sequence(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("sequenceDiagram")
}

pub(crate) fn detector_swimlane(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_js_word_boundary(txt.trim_start(), "swimlane-beta")
}

pub(crate) fn detector_flowchart_elk(txt: &str, config: &mut MermaidConfig) -> bool {
    let trimmed = txt.trim_start();
    if trimmed.starts_with("flowchart-elk") {
        config.set_value("layout", serde_json::Value::String("elk".to_string()));
        return true;
    }
    false
}

pub(crate) fn detector_flowchart_v2(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("graph") || txt.trim_start().starts_with("flowchart")
}

pub(crate) fn detector_timeline(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("timeline")
}

pub(crate) fn detector_git_graph(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("gitGraph")
}

pub(crate) fn detector_usecase(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start()
        .strip_prefix("usecase-beta")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

pub(crate) fn detector_agentflow(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_js_word_boundary(txt.trim_start(), "agentflow-beta")
}

pub(crate) fn detector_state_v2(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("stateDiagram")
}

pub(crate) fn detector_journey(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("journey")
}

pub(crate) fn detector_quadrant(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("quadrantChart")
}

pub(crate) fn detector_sankey(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("sankey")
}

pub(crate) fn detector_packet(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("packet")
}

pub(crate) fn detector_xychart(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("xychart")
}

pub(crate) fn detector_block(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("block")
}

pub(crate) fn detector_tree_view(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("treeView-beta")
}

pub(crate) fn detector_ishikawa(txt: &str, _config: &mut MermaidConfig) -> bool {
    let t = txt.trim_start();
    starts_with_header_case_insensitive(t, "ishikawa-beta")
        || starts_with_header_case_insensitive(t, "ishikawa")
}

pub(crate) fn detector_eventmodeling(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("eventmodeling")
}

pub(crate) fn detector_railroad(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_case_insensitive_prefix(txt.trim_start(), "railroad-beta")
}

pub(crate) fn detector_railroad_ebnf(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_case_insensitive_prefix(txt.trim_start(), "railroad-ebnf-beta")
}

pub(crate) fn detector_railroad_abnf(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_case_insensitive_prefix(txt.trim_start(), "railroad-abnf-beta")
}

pub(crate) fn detector_railroad_peg(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_case_insensitive_prefix(txt.trim_start(), "railroad-peg-beta")
}

pub(crate) fn detector_wardley(txt: &str, _config: &mut MermaidConfig) -> bool {
    starts_with_case_insensitive_prefix(txt.trim_start(), "wardley-beta")
}

pub(crate) fn detector_cynefin(txt: &str, _config: &mut MermaidConfig) -> bool {
    let Some(rest) = txt.trim_start().strip_prefix("cynefin-beta") else {
        return false;
    };
    rest.chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || c == ':')
}

fn starts_with_header_case_insensitive(text: &str, header: &str) -> bool {
    let Some(actual) = text.get(..header.len()) else {
        return false;
    };
    if !actual.eq_ignore_ascii_case(header) {
        return false;
    }
    text[header.len()..]
        .chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || c == ';')
}

fn starts_with_case_insensitive_prefix(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(prefix))
}

fn starts_with_js_word_boundary(text: &str, header: &str) -> bool {
    text.strip_prefix(header).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
    })
}

pub(crate) fn detector_radar(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("radar-beta")
}

pub(crate) fn detector_treemap(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("treemap")
}

pub(crate) fn detector_venn(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("venn-beta")
}

pub(crate) fn detector_mindmap(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("mindmap")
}

pub(crate) fn detector_architecture(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("architecture")
}

pub(crate) fn detector_zenuml(txt: &str, _config: &mut MermaidConfig) -> bool {
    txt.trim_start().starts_with("zenuml")
}

#[cfg(test)]
mod remove_directives_tests {
    use super::{DetectorRegistry, remove_directives};
    use crate::{MermaidConfig, OperationCancelled, OperationControl};
    use std::borrow::Cow;

    #[test]
    fn controlled_detection_stops_before_iterating_detectors() {
        let control = OperationControl::new();
        control.cancel();

        let result = DetectorRegistry::pinned_mermaid_baseline().detect_type_controlled(
            "flowchart TD",
            &mut MermaidConfig::empty_object(),
            &control,
        );

        assert!(matches!(result, Err(OperationCancelled { .. })));
    }

    #[test]
    fn detection_uses_the_greedy_frontmatter_match_without_parsing_yaml() {
        let registry = DetectorRegistry::pinned_mermaid_baseline();
        let mut config = MermaidConfig::empty_object();
        let source = "---\n\n---\n\nMORE\n---\nerror";
        assert_eq!(registry.detect_type(source, &mut config).unwrap(), "error");
    }

    #[test]
    fn detection_observes_cancellation_inside_frontmatter_whitespace() {
        let source = format!("---\n{}", " \n".repeat(16_000));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(3);
        let result = DetectorRegistry::pinned_mermaid_baseline().detect_type_controlled(
            &source,
            &mut MermaidConfig::empty_object(),
            &control,
        );
        assert!(result.is_err());
    }

    #[test]
    fn no_directives_is_borrowed() {
        let s = "flowchart TD; A-->B;";
        assert!(matches!(remove_directives(s), Cow::Borrowed(_)));
    }

    #[test]
    fn removes_directive_block() {
        let s = "%%{init: {\"theme\": \"dark\"}}%%\nflowchart TD; A-->B;";
        let out = remove_directives(s);
        assert!(out.as_ref().contains("flowchart TD"));
        assert!(!out.as_ref().contains("init"));
    }

    #[test]
    fn unterminated_directive_truncates_following_source_like_mermaid() {
        let s = "flowchart\n%%{init: {\"theme\": \"dark\"}}\nA-->B;";
        let out = remove_directives(s);
        assert_eq!(out.as_ref(), "flowchart\n");
    }
}

#[cfg(test)]
mod registry_clone_tests {
    use super::*;
    use std::sync::Arc;

    fn always_detects(_text: &str, _config: &mut MermaidConfig) -> bool {
        true
    }

    #[test]
    fn detector_registry_clone_uses_copy_on_write_storage() {
        let original = DetectorRegistry::pinned_mermaid_baseline();
        let mut cloned = original.clone();

        assert!(Arc::ptr_eq(&original.detectors, &cloned.detectors));

        cloned.add_fn("copy-on-write-test", always_detects);

        assert!(!Arc::ptr_eq(&original.detectors, &cloned.detectors));
        assert!(!original.detector_ids().any(|id| id == "copy-on-write-test"));
        assert!(cloned.detector_ids().any(|id| id == "copy-on-write-test"));
    }
}

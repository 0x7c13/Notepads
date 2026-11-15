mod create_text;
mod deterministic;
mod flowchart_parity;
mod heuristic;
pub(crate) mod html_wrap;
mod icons;
mod line_break;
mod markdown;
mod markdown_label;
mod measure;
mod metrics;
mod svg_metrics;
mod types;
mod whitespace;
mod wrap;

pub(crate) use create_text::non_markdown_svg_words;
pub use deterministic::{
    DeterministicTextMeasurer, WrappedTextLayout, try_measure_wrapped_text,
    try_measure_wrapped_text_with_line_metrics,
};
pub use flowchart_parity::{flowchart_html_has_inline_style_tags, flowchart_html_line_height_px};
pub(crate) use heuristic::{append_text_width_em, estimate_text_width_em};
pub(crate) use html_wrap::HtmlWhiteSpace;
pub use icons::replace_fontawesome_icons;
pub(crate) use line_break::{html_has_soft_break_opportunity, mermaid_html_breaks_to_newlines};
pub(crate) use markdown::{
    MermaidMarkdownAnalysis, MermaidMarkdownWordType, analyze_mermaid_markdown,
    mermaid_markdown_contains_html_tags, mermaid_markdown_to_lines,
};
pub(crate) use markdown_label::{
    mermaid_markdown_contains_raw_blocks, mermaid_markdown_to_html_label_fragment,
    mermaid_markdown_to_xhtml_label_fragment, mermaid_markdown_wants_paragraph_wrap,
    mermaid_xhtml_label_plain_text, mermaid_xhtml_label_text_content,
};
pub use measure::TextMeasurer;
pub(crate) use measure::{
    MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX, measure_mermaid_text_dimensions,
    measure_mermaid_text_dimensions_with_checkpoint,
};
#[doc(hidden)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub use metrics::unsupported_html_label_style;
pub use metrics::{measure_html_with_inline_styles, measure_markdown_with_inline_styles};
pub(crate) use metrics::{
    measure_wrapped_markdown_with_inline_styles, measure_xhtml_label_fragment,
    mermaid_markdown_to_wrapped_word_lines,
};
pub(crate) use svg_metrics::{
    svg_text_vertical_extents_px, svg_title_bbox_vertical_extents_px,
    svg_wrapped_first_line_bbox_height_px,
};
pub use types::{TextMetrics, TextStyle, WrapMode};
pub(crate) use whitespace::{
    is_ecmascript_whitespace, is_html_collapsible_ascii_whitespace, trim_ecmascript_whitespace,
    trim_end_html_collapsible_ascii_whitespace, trim_html_collapsible_ascii_whitespace,
    trim_start_ecmascript_whitespace,
};
pub(crate) use wrap::wrap_label_like_mermaid_lines_with_checkpoint;
pub use wrap::{
    ceil_to_1_64_px, round_to_1_64_px, round_to_1_64_px_ties_to_even, split_html_br_lines,
    wrap_label_like_mermaid_lines, wrap_text_lines_measurer,
};

#[cfg(all(
    test,
    any(
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-mindmap"
    )
))]
mod tests;

// Style parsing helpers (split from parity.rs).

pub(super) fn parse_style_decl(s: &str) -> Option<(&str, &str)> {
    crate::mermaid_style::parse_safe_style_decl(s)
}

pub(super) fn is_rect_style_key(key: &str) -> bool {
    matches!(
        key,
        "fill"
            | "stroke"
            | "stroke-width"
            | "stroke-dasharray"
            | "opacity"
            | "fill-opacity"
            | "stroke-opacity"
    )
}

pub(super) fn is_text_style_key(key: &str) -> bool {
    crate::mermaid_style::is_label_style_key(key)
}

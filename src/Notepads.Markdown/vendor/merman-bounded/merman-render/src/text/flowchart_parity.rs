//! Flowchart-specific HTML label measurement parity helpers.

pub fn flowchart_html_line_height_px(font_size_px: f64) -> f64 {
    (font_size_px.max(1.0) * 1.5).max(1.0)
}

pub fn flowchart_html_has_inline_style_tags(lower_html: &str) -> bool {
    // Detect Mermaid HTML inline styling tags in a way that avoids false positives like
    // `<br>` matching `<b`.
    //
    // We keep this intentionally lightweight (no full HTML parser); for our purposes we only
    // need to decide whether the label needs the special inline-style measurement path.
    let bytes = lower_html.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'!' || bytes[i] == b'?' {
            continue;
        }
        if bytes[i] == b'/' {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if start == i {
            continue;
        }
        let name = &lower_html[start..i];
        if matches!(name, "strong" | "b" | "em" | "i") {
            return true;
        }
    }
    false
}

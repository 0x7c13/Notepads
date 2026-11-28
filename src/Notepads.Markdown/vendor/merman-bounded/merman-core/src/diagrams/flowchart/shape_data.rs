use super::{Node, TitleKind};
use crate::diagrams::shapes::is_valid_pinned_shape;
use serde_json::Value;

pub(super) fn apply_shape_data_value_to_node(
    node: &mut Node,
    value: &Value,
) -> std::result::Result<(), String> {
    apply_shape_data_document_to_node(node, value)
}

pub(super) fn value_to_string(v: &Value) -> Option<String> {
    crate::inline_config::value_to_string(v)
}

pub(super) fn value_to_bool(v: &Value) -> Option<bool> {
    crate::inline_config::value_to_bool(v)
}

fn value_to_f64(v: &Value) -> Option<f64> {
    crate::inline_config::value_to_f64(v)
}

fn sanitize_shape_label_type(label_type: Option<&str>) -> TitleKind {
    match label_type {
        Some("text") => TitleKind::Text,
        Some("string") => TitleKind::String,
        Some("markdown") => TitleKind::Markdown,
        _ => TitleKind::Markdown,
    }
}

fn apply_shape_data_document_to_node(
    node: &mut Node,
    document: &Value,
) -> std::result::Result<(), String> {
    let map = match document.as_object() {
        Some(m) => m,
        None => return Ok(()),
    };

    let mut provided_label: Option<String> = None;
    let mut provided_label_type: Option<TitleKind> = None;
    for (k, v) in map {
        match k.as_str() {
            "shape" => {
                let Some(shape) = v.as_str() else { continue };
                // Mermaid rejects camelCase and underscore shapeData before shape-map lookup.
                if shape != shape.to_lowercase() || shape.contains('_') {
                    return Err(format!(
                        "No such shape: {shape}. Shape names should be lowercase."
                    ));
                }
                if !is_valid_pinned_shape(shape) {
                    return Err(format!("No such shape: {shape}."));
                }
                node.shape = Some(shape.to_string());
            }
            "label" => {
                if let Some(label) = value_to_string(v) {
                    provided_label = Some(label.clone());
                    node.label = Some(label);
                    node.label_span = None;
                    node.label_selection = None;
                }
            }
            "labelType" => {
                provided_label_type =
                    Some(sanitize_shape_label_type(value_to_string(v).as_deref()));
            }
            "icon" => {
                if let Some(icon) = value_to_string(v) {
                    node.icon = Some(icon);
                }
            }
            "form" => {
                if let Some(form) = value_to_string(v) {
                    node.form = Some(form);
                }
            }
            "pos" => {
                if let Some(pos) = value_to_string(v) {
                    node.pos = Some(pos);
                }
            }
            "img" => {
                if let Some(img) = value_to_string(v) {
                    node.img = Some(img);
                }
            }
            "constraint" => {
                if let Some(constraint) = value_to_string(v) {
                    node.constraint = Some(constraint);
                }
            }
            "w" => {
                if let Some(w) = value_to_f64(v) {
                    node.asset_width = Some(w);
                }
            }
            "h" => {
                if let Some(h) = value_to_f64(v) {
                    node.asset_height = Some(h);
                }
            }
            _ => {}
        }
    }
    if provided_label.is_some() {
        node.label_type = provided_label_type.unwrap_or(TitleKind::Markdown);
    }

    // Mermaid clears the default label when an icon or img is set without an explicit label.
    let has_visual = node.icon.is_some() || node.img.is_some();
    let label_is_empty_or_missing = provided_label
        .as_deref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true);
    if has_visual && label_is_empty_or_missing {
        let current_text = node.label.as_deref().unwrap_or(node.id.as_str());
        if current_text == node.id {
            node.label = Some(String::new());
            node.label_type = TitleKind::Text;
            node.label_span = None;
            node.label_selection = None;
        }
    }

    Ok(())
}

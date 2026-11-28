use crate::{
    Error, ParseMetadata, Result, common_db,
    diagram::{ParsedDiagram, ParsedDiagramRender, RenderSemanticModel},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Typed semantic model for Merman's built-in suppressed-error diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDiagramRenderModel {
    #[serde(rename = "type")]
    pub diagram_type: String,
    #[serde(
        default,
        rename = "errorMessage",
        skip_serializing_if = "Option::is_none"
    )]
    pub error_message: Option<String>,
}

impl ErrorDiagramRenderModel {
    fn new(meta: &ParseMetadata) -> Self {
        Self {
            diagram_type: meta.diagram_type.clone(),
            error_message: None,
        }
    }

    fn compatibility_json(&self) -> Value {
        let mut model = json!({ "type": self.diagram_type });
        if let Some(message) = &self.error_message {
            model["errorMessage"] = json!(message);
        }
        model
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &ErrorDiagramRenderModel,
    _meta: &ParseMetadata,
) -> Result<Value> {
    Ok(model.compatibility_json())
}

pub(crate) fn parse_error(_code: &str, meta: &ParseMetadata) -> Result<Value> {
    render_model_to_compat_json(&ErrorDiagramRenderModel::new(meta), meta)
}

pub(crate) fn parse_error_model_for_render(
    _code: &str,
    meta: &ParseMetadata,
) -> Result<ErrorDiagramRenderModel> {
    Ok(ErrorDiagramRenderModel::new(meta))
}

pub(crate) fn suppressed_error_diagram(
    source_meta: &ParseMetadata,
    error: &Error,
) -> ParsedDiagram {
    let meta = suppressed_error_metadata(source_meta);
    let mut typed = ErrorDiagramRenderModel::new(&meta);
    typed.error_message = Some(error.to_string());
    let mut model = render_model_to_compat_json(&typed, &meta)
        .expect("Error typed model must remain JSON-serializable");
    common_db::apply_common_db_sanitization(&mut model, &meta.effective_config);
    ParsedDiagram {
        meta,
        model: model.into(),
    }
}

pub(crate) fn suppressed_error_render_diagram(
    source_meta: &ParseMetadata,
    error: &Error,
) -> ParsedDiagramRender {
    let meta = suppressed_error_metadata(source_meta);
    let mut model = ErrorDiagramRenderModel::new(&meta);
    model.error_message = Some(error.to_string());
    ParsedDiagramRender::new(meta, RenderSemanticModel::Error(model))
}

fn suppressed_error_metadata(source_meta: &ParseMetadata) -> ParseMetadata {
    let mut meta = source_meta.clone();
    meta.diagram_type = "error".to_string();
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_error_model_projects_exact_compatibility_json() {
        let meta = ParseMetadata {
            diagram_type: "error".to_string(),
            config: crate::MermaidConfig::default(),
            effective_config: crate::MermaidConfig::default(),
            title: None,
        };
        let compat = parse_error("", &meta).unwrap();
        let typed = parse_error_model_for_render("", &meta).unwrap();

        assert_eq!(render_model_to_compat_json(&typed, &meta).unwrap(), compat);
    }
}

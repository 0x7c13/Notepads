use crate::Result;
use crate::model::{Bounds, InfoDiagramLayout};
use crate::text::TextMeasurer;
use merman_core::baseline::PINNED_MERMAID_BASELINE_VERSION;
use merman_core::diagrams::info::InfoDiagramRenderModel;

pub(crate) fn layout_info_diagram_typed(
    model: &InfoDiagramRenderModel,
    _effective_config: &serde_json::Value,
    _measurer: &dyn TextMeasurer,
) -> Result<InfoDiagramLayout> {
    let _ = model.show_info;
    Ok(InfoDiagramLayout {
        // Mermaid configures the info renderer with `height = 100`, `width = 400`. Responsive
        // max-width mode omits the height attribute from the emitted SVG, but the layout model
        // still records the configured canvas dimensions.
        bounds: Some(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 400.0,
            max_y: 100.0,
        }),
        version: format!("v{PINNED_MERMAID_BASELINE_VERSION}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::DeterministicTextMeasurer;

    #[test]
    fn info_layout_records_upstream_configured_canvas_size() {
        let measurer = DeterministicTextMeasurer::default();
        let layout = layout_info_diagram_typed(
            &InfoDiagramRenderModel::default(),
            &serde_json::Value::Null,
            &measurer,
        )
        .expect("info layout");

        assert_eq!(
            layout.bounds,
            Some(Bounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 400.0,
                max_y: 100.0,
            })
        );
    }
}

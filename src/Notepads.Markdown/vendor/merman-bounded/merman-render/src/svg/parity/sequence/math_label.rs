use super::super::*;
use super::SequenceEmitCheckpoints;
use crate::sequence::{SequenceMathHeightMode, measure_sequence_math_label};

pub(super) struct SequenceKatexLabel {
    pub(super) html: String,
    pub(super) width: f64,
    pub(super) height: f64,
}

pub(super) fn sequence_katex_label(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
    height_mode: SequenceMathHeightMode,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<Option<SequenceKatexLabel>> {
    checkpoints.checkpoint()?;
    if !text.contains("$$") {
        return Ok(None);
    }
    let Some(renderer) = math_renderer else {
        return Ok(None);
    };
    let Some((width, height)) = measure_sequence_math_label(
        measurer,
        text,
        style,
        config,
        Some(renderer),
        height_mode,
        checkpoints.text(),
    )?
    else {
        return Ok(None);
    };
    checkpoints.checkpoint()?;
    let html = renderer.render_sequence_html_label(text, config);
    checkpoints.checkpoint()?;
    let Some(html) = html else {
        return Ok(None);
    };
    let html = xhtml_fix_fragment(&merman_core::sanitize::sanitize_text(&html, config));
    Ok(Some(SequenceKatexLabel {
        html,
        width,
        height,
    }))
}

pub(super) fn write_sequence_katex_foreign_object(
    out: &mut String,
    label: &SequenceKatexLabel,
    x: f64,
    y: f64,
) {
    let _ = write!(
        out,
        r#"<foreignObject height="{h}" width="{w}" x="{x}" y="{y}"><div style="width: fit-content;" xmlns="http://www.w3.org/1999/xhtml">{html}</div></foreignObject>"#,
        h = fmt(label.height),
        w = fmt(label.width),
        x = fmt(x),
        y = fmt(y),
        html = label.html,
    );
}

fn xhtml_fix_fragment(input: &str) -> String {
    input
        .replace("<br>", "<br />")
        .replace("<br/>", "<br />")
        .replace("<br >", "<br />")
        .replace("</br>", "<br />")
        .replace("</br/>", "<br />")
        .replace("</br />", "<br />")
        .replace("</br >", "<br />")
}

#[cfg(test)]
mod tests {
    use super::sequence_katex_label;
    use crate::math::MathRenderer;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::sequence::SequenceMathHeightMode;
    use crate::text::{DeterministicTextMeasurer, TextMetrics, TextStyle};
    use merman_core::{MermaidConfig, OperationControl, OperationPhase};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CancellingMathRenderer {
        control: OperationControl,
        measurement_calls: AtomicUsize,
        render_calls: AtomicUsize,
    }

    impl MathRenderer for CancellingMathRenderer {
        fn render_html_label(&self, text: &str, _config: &MermaidConfig) -> Option<String> {
            self.render_calls.fetch_add(1, Ordering::Relaxed);
            Some(text.to_string())
        }

        fn measure_sequence_html_label(
            &self,
            _text: &str,
            _config: &MermaidConfig,
        ) -> Option<TextMetrics> {
            let calls = self.measurement_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if calls == 1 {
                self.control.cancel();
            }
            Some(TextMetrics {
                width: 20.0,
                height: 20.0,
                line_count: 1,
            })
        }
    }

    #[test]
    fn sequence_math_emit_stops_after_measurement_callback_cancels() {
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control.clone(),
        );
        let renderer = CancellingMathRenderer {
            control,
            measurement_calls: AtomicUsize::new(0),
            render_calls: AtomicUsize::new(0),
        };
        let text = std::iter::repeat_n("$$x$$", 130)
            .collect::<Vec<_>>()
            .join(" ");

        let result = sequence_katex_label(
            &text,
            &DeterministicTextMeasurer::default(),
            &TextStyle::default(),
            &MermaidConfig::default(),
            Some(&renderer),
            SequenceMathHeightMode::Draw,
            super::SequenceEmitCheckpoints::new(&meter),
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("expected Sequence emit cancellation"),
        };
        let crate::Error::Cancelled(error) = error else {
            panic!("expected Sequence emit cancellation");
        };

        assert_eq!(error.phase, OperationPhase::Emit);
        assert_eq!(renderer.measurement_calls.load(Ordering::Relaxed), 1);
        assert_eq!(renderer.render_calls.load(Ordering::Relaxed), 0);
    }
}

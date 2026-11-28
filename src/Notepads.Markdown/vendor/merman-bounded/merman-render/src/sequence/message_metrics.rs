use crate::environment::{BuiltinTextMeasurementOperationCarrier, TextMeasurementOperation};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::diagrams::sequence::{SequenceDiagramRenderModel, SequenceMessage};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SequenceMessageBoundMetrics {
    width: f64,
    height: f64,
}

impl SequenceMessageBoundMetrics {
    pub(super) fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub(super) const fn width(self) -> f64 {
        self.width
    }

    pub(super) const fn height(self) -> f64 {
        self.height
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SequenceMessageMeasurementBinding {
    font_family: Option<String>,
    font_size_bits: u64,
    font_weight: Option<String>,
    font_style: Option<String>,
    carrier: BuiltinTextMeasurementOperationCarrier,
}

impl SequenceMessageMeasurementBinding {
    fn for_measurer(style: &TextStyle, measurer: &dyn TextMeasurer) -> Option<Self> {
        Some(Self {
            font_family: style.font_family.clone(),
            font_size_bits: style.font_size.to_bits(),
            font_weight: style.font_weight.clone(),
            font_style: style.font_style.clone(),
            carrier: measurer.builtin_operation_carrier(
                TextMeasurementOperation::MermaidCalculateTextDimensions,
            )?,
        })
    }

    fn matches(&self, style: &TextStyle, measurer: &dyn TextMeasurer) -> bool {
        self.font_family == style.font_family
            && self.font_size_bits == style.font_size.to_bits()
            && self.font_weight == style.font_weight
            && self.font_style == style.font_style
            && measurer
                .builtin_operation_carrier(TextMeasurementOperation::MermaidCalculateTextDimensions)
                == Some(self.carrier)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SequenceMessageOwner(usize);

impl SequenceMessageOwner {
    pub(super) const fn from_model_index(model_index: usize) -> Self {
        Self(model_index)
    }
}

/// Private operation sidecar keyed by immutable Sequence message owner.
///
/// The semantic model and sidecar remain paired in one private family artifact. The model's stable
/// message allocation and each view's exact owner/message check bind the original text, wrap,
/// note, and math context without copying source strings. The binding proves the same style and
/// built-in operation route. Host and custom measurers cannot construct the carrier and retain
/// every callback.
#[derive(Debug)]
pub(super) struct SequenceMessageMetricSidecar {
    model_messages_identity: usize,
    binding: Option<SequenceMessageMeasurementBinding>,
    entries: Vec<Option<SequenceMessageBoundMetrics>>,
}

impl SequenceMessageMetricSidecar {
    pub(super) fn new(
        model: &SequenceDiagramRenderModel,
        style: &TextStyle,
        measurer: &dyn TextMeasurer,
    ) -> Self {
        let binding = SequenceMessageMeasurementBinding::for_measurer(style, measurer);
        let entries = binding
            .as_ref()
            .map_or_else(Vec::new, |_| vec![None; model.messages.len()]);
        Self {
            model_messages_identity: model.messages.as_ptr() as usize,
            binding,
            entries,
        }
    }

    pub(super) fn record(
        &mut self,
        owner: SequenceMessageOwner,
        metrics: SequenceMessageBoundMetrics,
    ) {
        if self.binding.is_none() {
            return;
        }
        let slot = self
            .entries
            .get_mut(owner.0)
            .expect("Sequence metric owner must come from the prepared model");
        debug_assert!(slot.is_none(), "Sequence message metrics are prepared once");
        *slot = Some(metrics);
    }

    pub(super) fn view<'a>(
        &'a self,
        model: &'a SequenceDiagramRenderModel,
        style: &TextStyle,
        measurer: &dyn TextMeasurer,
    ) -> SequenceMessageMetricView<'a> {
        if self.entries.len() == model.messages.len()
            && self.model_messages_identity == model.messages.as_ptr() as usize
            && self
                .binding
                .as_ref()
                .is_some_and(|binding| binding.matches(style, measurer))
        {
            SequenceMessageMetricView {
                model_messages: &model.messages,
                entries: &self.entries,
            }
        } else {
            SequenceMessageMetricView::empty()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SequenceMessageMetricView<'a> {
    model_messages: &'a [SequenceMessage],
    entries: &'a [Option<SequenceMessageBoundMetrics>],
}

impl SequenceMessageMetricView<'_> {
    pub(super) const fn empty() -> Self {
        Self {
            model_messages: &[],
            entries: &[],
        }
    }

    pub(super) fn get(
        self,
        owner: SequenceMessageOwner,
        message: &SequenceMessage,
    ) -> Option<SequenceMessageBoundMetrics> {
        let expected_message = self.model_messages.get(owner.0)?;
        if !std::ptr::eq(expected_message, message) {
            return None;
        }
        self.entries.get(owner.0).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::{SequenceMessageBoundMetrics, SequenceMessageMetricSidecar, SequenceMessageOwner};
    use crate::environment::{RenderEnvironment, TextMeasurementPhase};
    use crate::text::{DeterministicTextMeasurer, TextStyle};
    use merman_core::diagrams::sequence::{
        SequenceDiagramRenderModel, SequenceMessage, SequenceMessagePayload,
    };
    use std::collections::BTreeMap;

    fn metric_test_model(text: &str) -> SequenceDiagramRenderModel {
        SequenceDiagramRenderModel {
            acc_title: None,
            acc_descr: None,
            title: None,
            actor_order: Vec::new(),
            actors: BTreeMap::new(),
            boxes: Vec::new(),
            messages: vec![SequenceMessage {
                id: "message-0".to_string(),
                from: Some("A".to_string()),
                to: Some("B".to_string()),
                message_type: 0,
                message: SequenceMessagePayload::Text(text.to_string()),
                wrap: false,
                activate: false,
                placement: None,
                central_connection: 0,
            }],
            notes: Vec::new(),
            created_actors: BTreeMap::new(),
            destroyed_actors: BTreeMap::new(),
            actor_lifecycles: None,
        }
    }

    #[test]
    fn message_metric_sidecar_requires_exact_model_style_and_builtin_route() {
        let model = metric_test_model("operation-bound");
        let style = TextStyle::default();
        let session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin deterministic session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);
        let owner = SequenceMessageOwner::from_model_index(0);
        let metrics = SequenceMessageBoundMetrics::new(42.0, 17.0);
        let mut sidecar = SequenceMessageMetricSidecar::new(&model, &style, &measurer);
        sidecar.record(owner, metrics);

        assert_eq!(
            sidecar
                .view(&model, &style, &measurer)
                .get(owner, &model.messages[0]),
            Some(metrics)
        );

        let equal_message = model.messages[0].clone();
        assert_eq!(
            sidecar
                .view(&model, &style, &measurer)
                .get(owner, &equal_message),
            None,
            "equal text must not turn the owner-indexed sidecar into a value cache"
        );

        let equal_model = model.clone();
        assert_eq!(
            sidecar
                .view(&equal_model, &style, &measurer)
                .get(owner, &equal_model.messages[0]),
            None,
            "a separately allocated semantic model must not inherit operation-owned metrics"
        );

        let mut changed_style = style.clone();
        changed_style.font_size += 1.0;
        assert_eq!(
            sidecar
                .view(&model, &changed_style, &measurer)
                .get(owner, &model.messages[0]),
            None
        );
    }

    #[test]
    fn message_metric_sidecar_stays_empty_for_opaque_measurers() {
        let model = metric_test_model("host-visible");
        let style = TextStyle::default();
        let measurer = DeterministicTextMeasurer::default();
        let owner = SequenceMessageOwner::from_model_index(0);
        let mut sidecar = SequenceMessageMetricSidecar::new(&model, &style, &measurer);
        sidecar.record(owner, SequenceMessageBoundMetrics::new(42.0, 17.0));

        assert_eq!(
            sidecar
                .view(&model, &style, &measurer)
                .get(owner, &model.messages[0]),
            None
        );
    }
}

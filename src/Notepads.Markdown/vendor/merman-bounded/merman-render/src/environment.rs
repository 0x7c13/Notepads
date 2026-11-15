//! Operation-owned render services and deterministic policy.

use crate::math::MathRenderer;
use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
use crate::svg::IconRegistry;
use crate::text::{
    DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle, WrapMode,
    append_text_width_em, estimate_text_width_em,
};
use crate::{RenderCapability, RenderCapabilityPolicy};
use merman_core::runtime::{OperationContext, OperationTiming, RuntimePolicy, RuntimePolicyError};
use merman_core::time::LocalTimeZoneProvenance;
use merman_core::{OperationControl, OperationPhase};
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use unicode_segmentation::UnicodeSegmentation;

/// A render phase that may select a distinct complete text-measurement profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextMeasurementPhase {
    Layout,
    Wrap,
    SvgBBox,
    ComputedLength,
}

impl TextMeasurementPhase {
    pub const ALL: [Self; 4] = [
        Self::Layout,
        Self::Wrap,
        Self::SvgBBox,
        Self::ComputedLength,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Layout => 0,
            Self::Wrap => 1,
            Self::SvgBBox => 2,
            Self::ComputedLength => 3,
        }
    }
}

/// Stable name for one complete [`TextMeasurer`] profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasurementProfileId(Arc<str>);

impl MeasurementProfileId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidMeasurementProfileIdentity> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(InvalidMeasurementProfileIdentity::EmptyProfile);
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MeasurementProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Observable identity for a measurer and its ordered decorator chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextMeasurementProfileIdentity {
    profile: MeasurementProfileId,
    version: Arc<str>,
    decorators: Arc<[Arc<str>]>,
}

impl TextMeasurementProfileIdentity {
    pub fn new(
        profile: MeasurementProfileId,
        version: impl Into<String>,
    ) -> Result<Self, InvalidMeasurementProfileIdentity> {
        let version = version.into();
        let version = version.trim();
        if version.is_empty() {
            return Err(InvalidMeasurementProfileIdentity::EmptyVersion);
        }
        Ok(Self {
            profile,
            version: Arc::from(version),
            decorators: Arc::from([]),
        })
    }

    pub fn with_decorators<I, S>(
        mut self,
        decorators: I,
    ) -> Result<Self, InvalidMeasurementProfileIdentity>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut validated = Vec::new();
        for decorator in decorators {
            let decorator = decorator.into();
            let decorator = decorator.trim();
            if decorator.is_empty() {
                return Err(InvalidMeasurementProfileIdentity::EmptyDecorator);
            }
            validated.push(Arc::from(decorator));
        }
        self.decorators = validated.into();
        Ok(self)
    }

    pub fn profile(&self) -> &MeasurementProfileId {
        &self.profile
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn decorators(&self) -> &[Arc<str>] {
        &self.decorators
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidMeasurementProfileIdentity {
    #[error("text measurement profile name cannot be empty")]
    EmptyProfile,
    #[error("text measurement profile version cannot be empty")]
    EmptyVersion,
    #[error("text measurement decorator identity cannot be empty")]
    EmptyDecorator,
}

/// A named, complete measurer profile. Specialized trait methods remain part of the profile.
#[derive(Clone)]
pub struct TextMeasurementProfile {
    identity: TextMeasurementProfileIdentity,
    backend: Arc<dyn TextMeasurer + Send + Sync>,
    builtin: Option<BuiltinTextMeasurementProfile>,
}

impl TextMeasurementProfile {
    pub fn new(
        identity: TextMeasurementProfileIdentity,
        backend: Arc<dyn TextMeasurer + Send + Sync>,
    ) -> Self {
        Self {
            identity,
            backend,
            builtin: None,
        }
    }

    pub fn identity(&self) -> &TextMeasurementProfileIdentity {
        &self.identity
    }

    fn new_builtin(
        identity: TextMeasurementProfileIdentity,
        backend: Arc<dyn TextMeasurer + Send + Sync>,
        builtin: BuiltinTextMeasurementProfile,
    ) -> Self {
        Self {
            identity,
            backend,
            builtin: Some(builtin),
        }
    }
}

impl fmt::Debug for TextMeasurementProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextMeasurementProfile")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinTextMeasurementProfile {
    Deterministic,
}

/// Crate-private proof that one operation resolves to a concrete built-in profile route.
///
/// The public `TextMeasurer` extension surface cannot construct or name this value. Sequence may
/// carry it between two stages of the same render operation, but a custom or host-backed measurer
/// cannot replay built-in authority to validate cached measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuiltinTextMeasurementOperationCarrier {
    profile: BuiltinTextMeasurementProfile,
    phase: TextMeasurementPhase,
    operation: TextMeasurementOperation,
}

impl BuiltinTextMeasurementOperationCarrier {
    pub(crate) const fn into_inline_html(self) -> Option<InlineHtmlMeasurementCarrier> {
        match (self.phase, self.operation) {
            (TextMeasurementPhase::Wrap, TextMeasurementOperation::WrappedWithRawWidth) => {
                Some(InlineHtmlMeasurementCarrier::builtin(self.profile))
            }
            _ => None,
        }
    }

    pub(crate) fn into_svg_computed_length(
        self,
        style: &TextStyle,
    ) -> Option<BuiltinSvgComputedLength> {
        match (self.phase, self.operation) {
            (TextMeasurementPhase::ComputedLength, TextMeasurementOperation::ComputedLength) => {
                Some(BuiltinSvgComputedLength::new(style))
            }
            _ => None,
        }
    }

    pub(crate) fn into_html_line_width(self, style: &TextStyle) -> Option<BuiltinHtmlLineWidth> {
        match (self.profile, self.phase, self.operation) {
            (
                BuiltinTextMeasurementProfile::Deterministic,
                TextMeasurementPhase::Wrap,
                TextMeasurementOperation::CanvasMeasureTextWidth,
            ) => Some(BuiltinHtmlLineWidth::deterministic(style, 0.0)),
            _ => None,
        }
    }
}

/// Private authority for one complete rich HTML measurement operation.
///
/// Only [`RoutedTextMeasurer`] can attach a built-in profile after resolving the operation's
/// owning phase. Arbitrary `TextMeasurer` implementations receive [`Self::opaque`], so custom and
/// host routes cannot copy or replay built-in authority through the public trait surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineHtmlMeasurementCarrier {
    builtin: Option<BuiltinTextMeasurementProfile>,
}

impl InlineHtmlMeasurementCarrier {
    pub(crate) const fn opaque() -> Self {
        Self { builtin: None }
    }

    const fn builtin(profile: BuiltinTextMeasurementProfile) -> Self {
        Self {
            builtin: Some(profile),
        }
    }

    pub(crate) const fn is_builtin(self) -> bool {
        self.builtin.is_some()
    }

    pub(crate) fn begin_inline_html_width(self, style: &TextStyle) -> Option<BuiltinHtmlLineWidth> {
        self.builtin
            .map(|_| BuiltinHtmlLineWidth::deterministic(style, 0.0))
    }
}

#[derive(Debug, Clone)]
struct BuiltinInlineRawLineWidth {
    font_size: f64,
    committed_em: f64,
    pending_grapheme: String,
}

/// Streaming width of a decoded, single HTML line for a qualified built-in wrapping route.
///
/// Retain only the final extendable grapheme rather than remeasure every growing candidate.
/// The input is already decoded text: literal `<br>` spellings are never interpreted as markup.
/// U+00AD retains its shaping boundary with zero advance until a discretionary break is selected.
/// Other scalar-count semantics stay unchanged for uniform-advance profiles.
#[derive(Debug, Clone)]
pub(crate) struct BuiltinHtmlLineWidth {
    source: BuiltinHtmlLineWidthSource,
    #[cfg(test)]
    pub(crate) pushed_chunks: usize,
}

#[derive(Debug, Clone)]
enum BuiltinHtmlLineWidthSource {
    Heuristic(BuiltinInlineRawLineWidth),
    Uniform {
        font_size: f64,
        char_width_factor: f64,
        char_count: usize,
    },
}

impl BuiltinHtmlLineWidth {
    pub(crate) fn deterministic(style: &TextStyle, char_width_factor: f64) -> Self {
        let source = if char_width_factor == 0.0 {
            BuiltinHtmlLineWidthSource::Heuristic(BuiltinInlineRawLineWidth::new(style))
        } else {
            BuiltinHtmlLineWidthSource::Uniform {
                font_size: style.font_size.max(1.0),
                char_width_factor,
                char_count: 0,
            }
        };
        Self {
            source,
            #[cfg(test)]
            pushed_chunks: 0,
        }
    }

    pub(crate) fn push_text(&mut self, text: &str) {
        #[cfg(test)]
        {
            self.pushed_chunks = self.pushed_chunks.saturating_add(1);
        }
        match &mut self.source {
            BuiltinHtmlLineWidthSource::Heuristic(line) => line.push_text(text),
            BuiltinHtmlLineWidthSource::Uniform { char_count, .. } => {
                *char_count =
                    char_count.saturating_add(text.chars().filter(|ch| *ch != '\u{00ad}').count());
            }
        }
    }

    pub(crate) fn width_px(&self) -> f64 {
        let width = match &self.source {
            BuiltinHtmlLineWidthSource::Heuristic(line) => line.width_px(),
            BuiltinHtmlLineWidthSource::Uniform {
                font_size,
                char_width_factor,
                char_count,
            } => *char_count as f64 * *font_size * *char_width_factor,
        };
        // An unwrapped built-in HTML measurement starts its maximum-width fold at zero.
        width.max(0.0)
    }

    pub(crate) fn reset(&mut self) {
        #[cfg(test)]
        {
            self.pushed_chunks = 0;
        }
        match &mut self.source {
            BuiltinHtmlLineWidthSource::Heuristic(line) => line.reset(),
            BuiltinHtmlLineWidthSource::Uniform { char_count, .. } => *char_count = 0,
        }
    }
}

/// Streaming `getComputedTextLength()` state for a qualified built-in SVG text route.
///
/// Flowchart's createText wrapper probes every growing word prefix. Retaining completed grapheme
/// width plus only the extendable final grapheme avoids rescanning the complete prefix while
/// preserving sequence-aware Unicode width across arbitrary chunk boundaries. Host-backed and
/// opaque custom measurers cannot construct this state, so their observable callback sequence
/// remains unchanged.
#[derive(Debug, Clone)]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
pub(crate) struct BuiltinSvgComputedLength {
    line: BuiltinInlineRawLineWidth,
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
impl BuiltinSvgComputedLength {
    fn new(style: &TextStyle) -> Self {
        Self {
            line: BuiltinInlineRawLineWidth::new(style),
        }
    }

    pub(crate) fn deterministic(style: &TextStyle) -> Self {
        Self::new(style)
    }

    pub(crate) fn push_text(&mut self, text: &str) {
        self.line.push_text(text);
    }

    pub(crate) fn width_px(&self) -> f64 {
        let width = self.line.width_px();
        if width.is_finite() && width >= 0.0 {
            width
        } else {
            0.0
        }
    }

    pub(crate) fn reset(&mut self) {
        self.line.reset();
    }
}

impl BuiltinInlineRawLineWidth {
    fn new(style: &TextStyle) -> Self {
        Self {
            font_size: style.font_size.max(1.0),
            committed_em: 0.0,
            pending_grapheme: String::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.pending_grapheme.push_str(text);
        let last_grapheme_start = self
            .pending_grapheme
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index);
        if last_grapheme_start > 0 {
            append_text_width_em(
                &mut self.committed_em,
                &self.pending_grapheme[..last_grapheme_start],
            );
            self.pending_grapheme.drain(..last_grapheme_start);
        }
    }

    fn width_px(&self) -> f64 {
        (self.committed_em + estimate_text_width_em(&self.pending_grapheme)) * self.font_size
    }

    fn reset(&mut self) {
        self.committed_em = 0.0;
        self.pending_grapheme.clear();
    }
}

fn deterministic_profile() -> TextMeasurementProfile {
    let profile = MeasurementProfileId::new("merman.deterministic-text")
        .expect("static deterministic profile id is valid");
    let identity = TextMeasurementProfileIdentity::new(
        profile,
        concat!("merman-render@", env!("CARGO_PKG_VERSION")),
    )
    .expect("static deterministic profile version is valid");
    TextMeasurementProfile::new_builtin(
        identity,
        Arc::new(DeterministicTextMeasurer::default()),
        BuiltinTextMeasurementProfile::Deterministic,
    )
}

/// Why a configured host attempt used its named fallback profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostFallbackReason {
    Missing,
    Invalid,
    Error,
}

/// The exact [`TextMeasurer`] operation performed through a phase facade and its required host
/// result shape. Both types are generated from the independently versioned host
/// text-measurement protocol shared by every binding.
pub use crate::generated::text_measurement_abi::{
    TEXT_MEASUREMENT_PROTOCOL_VERSION, TextMeasurementOperation, TextMeasurementResultKind,
};

/// The concrete backend kind that produced one result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextMeasurementSource {
    Profile,
    Host,
}

/// Actual provenance recorded after one measurement completes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextMeasurementProvenance {
    pub phase: TextMeasurementPhase,
    pub operation: TextMeasurementOperation,
    pub source: TextMeasurementSource,
    pub identity: TextMeasurementProfileIdentity,
    pub fallback_reason: Option<HostFallbackReason>,
}

/// One distinct provenance key and its total call count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMeasurementSummary {
    provenance: TextMeasurementProvenance,
    count: u64,
}

impl TextMeasurementSummary {
    pub fn provenance(&self) -> &TextMeasurementProvenance {
        &self.provenance
    }

    pub const fn count(&self) -> u64 {
        self.count
    }
}

/// Bounded snapshot of measurement provenance aggregated by distinct route outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextMeasurementReport {
    entries: Vec<TextMeasurementSummary>,
}

impl TextMeasurementReport {
    pub fn entries(&self) -> &[TextMeasurementSummary] {
        &self.entries
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextMeasurementRouteOutcome {
    Profile,
    Host,
    Fallback(HostFallbackReason),
}

impl TextMeasurementRouteOutcome {
    const ALL: [Self; 5] = [
        Self::Profile,
        Self::Host,
        Self::Fallback(HostFallbackReason::Missing),
        Self::Fallback(HostFallbackReason::Invalid),
        Self::Fallback(HostFallbackReason::Error),
    ];

    const fn index(self) -> usize {
        match self {
            Self::Profile => 0,
            Self::Host => 1,
            Self::Fallback(HostFallbackReason::Missing) => 2,
            Self::Fallback(HostFallbackReason::Invalid) => 3,
            Self::Fallback(HostFallbackReason::Error) => 4,
        }
    }
}

#[derive(Debug)]
struct TextMeasurementRecorder {
    counts: [AtomicU64;
        TextMeasurementPhase::ALL.len()
            * TextMeasurementOperation::ALL.len()
            * TextMeasurementRouteOutcome::ALL.len()],
}

impl Default for TextMeasurementRecorder {
    fn default() -> Self {
        Self {
            counts: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

impl TextMeasurementRecorder {
    const fn slot(
        phase: TextMeasurementPhase,
        operation: TextMeasurementOperation,
        outcome: TextMeasurementRouteOutcome,
    ) -> usize {
        (phase.index() * TextMeasurementOperation::ALL.len() + operation.index())
            * TextMeasurementRouteOutcome::ALL.len()
            + outcome.index()
    }

    fn record(
        &self,
        phase: TextMeasurementPhase,
        operation: TextMeasurementOperation,
        outcome: TextMeasurementRouteOutcome,
    ) {
        let counter = &self.counts[Self::slot(phase, operation, outcome)];
        let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            Some(count.saturating_add(1))
        });
    }

    fn report(&self, policy: &TextMeasurementPolicy) -> TextMeasurementReport {
        let mut entries = Vec::new();
        for phase in TextMeasurementPhase::ALL {
            for operation in TextMeasurementOperation::ALL {
                for outcome in TextMeasurementRouteOutcome::ALL {
                    let count =
                        self.counts[Self::slot(phase, operation, outcome)].load(Ordering::Relaxed);
                    if count == 0 {
                        continue;
                    }

                    let provenance = match (&policy.routes[phase.index()], outcome) {
                        (
                            TextMeasurementRouteConfig::Profile(profile),
                            TextMeasurementRouteOutcome::Profile,
                        ) => TextMeasurementProvenance {
                            phase,
                            operation,
                            source: TextMeasurementSource::Profile,
                            identity: profile.identity.clone(),
                            fallback_reason: None,
                        },
                        (
                            TextMeasurementRouteConfig::Host { identity, .. },
                            TextMeasurementRouteOutcome::Host,
                        ) => TextMeasurementProvenance {
                            phase,
                            operation,
                            source: TextMeasurementSource::Host,
                            identity: identity.clone(),
                            fallback_reason: None,
                        },
                        (
                            TextMeasurementRouteConfig::Host { fallback, .. },
                            TextMeasurementRouteOutcome::Fallback(reason),
                        ) => TextMeasurementProvenance {
                            phase,
                            operation,
                            source: TextMeasurementSource::Profile,
                            identity: fallback.identity.clone(),
                            fallback_reason: Some(reason),
                        },
                        _ => unreachable!(
                            "recorded text measurement outcome does not match the configured route"
                        ),
                    };
                    entries.push(TextMeasurementSummary { provenance, count });
                }
            }
        }
        TextMeasurementReport { entries }
    }
}

/// A host callback failure or invalid result converted to explicit fallback provenance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HostTextMeasurementError {
    message: Arc<str>,
    fallback_reason: HostFallbackReason,
}

impl HostTextMeasurementError {
    /// Creates an error reported by the host callback or its transport.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Arc::from(message.into()),
            fallback_reason: HostFallbackReason::Error,
        }
    }

    /// Creates an error for a callback value that violates the measurement contract.
    #[doc(hidden)]
    pub fn invalid_value(message: impl Into<String>) -> Self {
        Self {
            message: Arc::from(message.into()),
            fallback_reason: HostFallbackReason::Invalid,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn fallback_reason(&self) -> HostFallbackReason {
        self.fallback_reason
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HostTextMeasurementRequest<'a> {
    pub operation: TextMeasurementOperation,
    pub phase: TextMeasurementPhase,
    pub text: &'a str,
    pub style: &'a TextStyle,
    pub max_width: Option<f64>,
    pub wrap_mode: WrapMode,
}

#[derive(Debug, Clone, Copy)]
pub enum HostTextMeasurement {
    Metrics(TextMetrics),
    Length(f64),
    HorizontalExtents {
        left: f64,
        right: f64,
    },
    WrappedWithRawWidth {
        metrics: TextMetrics,
        raw_width: Option<f64>,
    },
}

pub type HostMeasurementResult = Result<Option<HostTextMeasurement>, HostTextMeasurementError>;

/// Fallible, operation-aware host counterpart of [`TextMeasurer`].
///
/// Returning `Ok(None)` declines exactly the requested operation. Returning a result variant that
/// does not match `request.operation`, or an invalid value, uses the configured fallback and is
/// recorded as [`HostFallbackReason::Invalid`].
pub trait HostTextMeasurer: Send + Sync {
    fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult;

    /// Measures within the active render operation. Existing host adapters may ignore control;
    /// native adapters use it to checkpoint bounded shaping and wrapping work.
    fn measure_with_control(
        &self,
        request: HostTextMeasurementRequest<'_>,
        _control: &OperationControl,
    ) -> HostMeasurementResult {
        self.measure(request)
    }
}

#[derive(Clone)]
enum TextMeasurementRouteConfig {
    Profile(TextMeasurementProfile),
    Host {
        identity: TextMeasurementProfileIdentity,
        backend: Arc<dyn HostTextMeasurer>,
        fallback: TextMeasurementProfile,
    },
}

/// Observable configured route for one phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMeasurementRoute {
    pub phase: TextMeasurementPhase,
    pub primary_source: TextMeasurementSource,
    pub primary: TextMeasurementProfileIdentity,
    pub fallback: Option<TextMeasurementProfileIdentity>,
}

/// Immutable routing policy for all text-measurement phases in one environment.
#[derive(Clone)]
pub struct TextMeasurementPolicy {
    routes: [TextMeasurementRouteConfig; 4],
}

impl fmt::Debug for TextMeasurementPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let routes = TextMeasurementPhase::ALL.map(|phase| self.route(phase));
        f.debug_struct("TextMeasurementPolicy")
            .field("routes", &routes)
            .finish()
    }
}

impl TextMeasurementPolicy {
    pub fn deterministic() -> Self {
        Self::uniform(deterministic_profile())
    }

    pub fn uniform(profile: TextMeasurementProfile) -> Self {
        Self {
            routes: std::array::from_fn(|_| TextMeasurementRouteConfig::Profile(profile.clone())),
        }
    }

    pub fn with_profile_for_phase(
        mut self,
        phase: TextMeasurementPhase,
        profile: TextMeasurementProfile,
    ) -> Self {
        self.routes[phase.index()] = TextMeasurementRouteConfig::Profile(profile);
        self
    }

    pub fn host_display(
        identity: TextMeasurementProfileIdentity,
        host: Arc<dyn HostTextMeasurer>,
        host_phases: impl IntoIterator<Item = TextMeasurementPhase>,
    ) -> Self {
        Self::host_display_with_fallback(identity, host, host_phases, deterministic_profile())
    }

    pub fn host_display_with_fallback(
        identity: TextMeasurementProfileIdentity,
        host: Arc<dyn HostTextMeasurer>,
        host_phases: impl IntoIterator<Item = TextMeasurementPhase>,
        fallback: TextMeasurementProfile,
    ) -> Self {
        let mut policy = Self::uniform(fallback.clone());
        for phase in host_phases {
            policy.routes[phase.index()] = TextMeasurementRouteConfig::Host {
                identity: identity.clone(),
                backend: Arc::clone(&host),
                fallback: fallback.clone(),
            };
        }
        policy
    }

    pub fn route(&self, phase: TextMeasurementPhase) -> TextMeasurementRoute {
        match &self.routes[phase.index()] {
            TextMeasurementRouteConfig::Profile(profile) => TextMeasurementRoute {
                phase,
                primary_source: TextMeasurementSource::Profile,
                primary: profile.identity.clone(),
                fallback: None,
            },
            TextMeasurementRouteConfig::Host {
                identity, fallback, ..
            } => TextMeasurementRoute {
                phase,
                primary_source: TextMeasurementSource::Host,
                primary: identity.clone(),
                fallback: Some(fallback.identity.clone()),
            },
        }
    }

    pub fn routes(&self) -> [TextMeasurementRoute; 4] {
        TextMeasurementPhase::ALL.map(|phase| self.route(phase))
    }
}

impl Default for TextMeasurementPolicy {
    fn default() -> Self {
        Self::deterministic()
    }
}

/// Session-aware facade that routes specialized operations to their named phases.
pub struct RoutedTextMeasurer<'a> {
    default_phase: TextMeasurementPhase,
    policy: &'a TextMeasurementPolicy,
    recorder: &'a TextMeasurementRecorder,
    work_meter: &'a OperationWorkMeter,
    controlled_operation_phase: Option<OperationPhase>,
}

trait CancelledTextMeasurement: Sized {
    fn cancelled() -> Self;
}

impl CancelledTextMeasurement for f64 {
    fn cancelled() -> Self {
        0.0
    }
}

impl CancelledTextMeasurement for Option<f64> {
    fn cancelled() -> Self {
        None
    }
}

impl CancelledTextMeasurement for (f64, f64) {
    fn cancelled() -> Self {
        (0.0, 0.0)
    }
}

impl CancelledTextMeasurement for TextMetrics {
    fn cancelled() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            line_count: 1,
        }
    }
}

impl CancelledTextMeasurement for (TextMetrics, Option<f64>) {
    fn cancelled() -> Self {
        (TextMetrics::cancelled(), None)
    }
}

impl RoutedTextMeasurer<'_> {
    fn controlled_cancellation_value<T: CancelledTextMeasurement>(&self) -> Option<T> {
        let phase = self.controlled_operation_phase?;
        self.work_meter
            .checkpoint(phase)
            .is_err()
            .then(T::cancelled)
    }

    fn phase_for(&self, operation: TextMeasurementOperation) -> TextMeasurementPhase {
        match operation {
            TextMeasurementOperation::ComputedLength => TextMeasurementPhase::ComputedLength,
            TextMeasurementOperation::BBoxX
            | TextMeasurementOperation::BBoxXWithAsciiOverhang
            | TextMeasurementOperation::TitleBBoxX
            | TextMeasurementOperation::SimpleBBoxWidth
            | TextMeasurementOperation::RawBBoxWidth
            | TextMeasurementOperation::RawBBoxHeight
            | TextMeasurementOperation::RawBBoxYOffset
            | TextMeasurementOperation::RawMiddleBBoxYOffset
            | TextMeasurementOperation::RawHangingBBoxYOffset
            | TextMeasurementOperation::BoundingClientRectWidth
            | TextMeasurementOperation::TspanBBoxWidth
            | TextMeasurementOperation::TspanBBoxHeight
            | TextMeasurementOperation::CreateTextBBoxYOffset
            | TextMeasurementOperation::CreateTextMiddleBBoxYOffset
            | TextMeasurementOperation::MermaidCalculateTextDimensions
            | TextMeasurementOperation::SimpleBBoxHeight => TextMeasurementPhase::SvgBBox,
            TextMeasurementOperation::CanvasMeasureTextWidth => TextMeasurementPhase::Layout,
            TextMeasurementOperation::WrapProbeBBoxWidth => TextMeasurementPhase::Wrap,
            TextMeasurementOperation::Wrapped | TextMeasurementOperation::WrappedWithRawWidth => {
                TextMeasurementPhase::Wrap
            }
            TextMeasurementOperation::Measure => self.default_phase,
        }
    }

    pub(crate) fn builtin_operation_carrier(
        &self,
        operation: TextMeasurementOperation,
    ) -> Option<BuiltinTextMeasurementOperationCarrier> {
        self.builtin_operation_carrier_for_phase(self.phase_for(operation), operation)
    }

    fn builtin_operation_carrier_for_phase(
        &self,
        phase: TextMeasurementPhase,
        operation: TextMeasurementOperation,
    ) -> Option<BuiltinTextMeasurementOperationCarrier> {
        match &self.policy.routes[phase.index()] {
            TextMeasurementRouteConfig::Profile(profile) => {
                profile
                    .builtin
                    .map(|profile| BuiltinTextMeasurementOperationCarrier {
                        profile,
                        phase,
                        operation,
                    })
            }
            // A host-routed operation remains observable even when its fallback is built-in. It
            // must stay opaque so callback order, failure position, and provenance cannot be
            // predicted away.
            TextMeasurementRouteConfig::Host { .. } => None,
        }
    }

    fn resolve<T: CancelledTextMeasurement>(
        &self,
        request: HostTextMeasurementRequest<'_>,
        decode_host: impl FnOnce(HostTextMeasurement) -> Option<T>,
        profile_call: impl FnOnce(&(dyn TextMeasurer + Send + Sync)) -> T,
    ) -> T {
        let phase = request.phase;
        let operation = request.operation;
        if let Some(cancelled) = self.controlled_cancellation_value() {
            return cancelled;
        }
        match &self.policy.routes[phase.index()] {
            TextMeasurementRouteConfig::Profile(profile) => {
                let value = profile_call(profile.backend.as_ref());
                self.recorder
                    .record(phase, operation, TextMeasurementRouteOutcome::Profile);
                if let Some(cancelled) = self.controlled_cancellation_value() {
                    return cancelled;
                }
                value
            }
            TextMeasurementRouteConfig::Host {
                backend, fallback, ..
            } => {
                let attempt = backend.measure_with_control(request, self.work_meter.control());
                if let Some(cancelled) = self.controlled_cancellation_value() {
                    return cancelled;
                }
                let decoded = match &attempt {
                    Ok(Some(value)) if validate_host_text_measurement(&request, value).is_ok() => {
                        decode_host(*value)
                    }
                    Ok(None) | Err(_) => None,
                    Ok(Some(_)) => None,
                };
                if let Some(value) = decoded {
                    self.recorder
                        .record(phase, operation, TextMeasurementRouteOutcome::Host);
                    return value;
                }

                let reason = match attempt {
                    Ok(Some(_)) => HostFallbackReason::Invalid,
                    Ok(None) => HostFallbackReason::Missing,
                    Err(error) => error.fallback_reason(),
                };
                // `TextMeasurer` is intentionally infallible. A controlled route therefore
                // observes the canonical operation before entering another opaque backend and
                // returns a neutral value only to unwind toward the caller's fallible boundary.
                if let Some(cancelled) = self.controlled_cancellation_value() {
                    return cancelled;
                }
                let value = profile_call(fallback.backend.as_ref());
                self.recorder.record(
                    phase,
                    operation,
                    TextMeasurementRouteOutcome::Fallback(reason),
                );
                if let Some(cancelled) = self.controlled_cancellation_value() {
                    return cancelled;
                }
                value
            }
        }
    }

    fn request<'a>(
        &self,
        operation: TextMeasurementOperation,
        text: &'a str,
        style: &'a TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> HostTextMeasurementRequest<'a> {
        HostTextMeasurementRequest {
            operation,
            phase: self.phase_for(operation),
            text,
            style,
            max_width,
            wrap_mode,
        }
    }
}

impl TextMeasurer for RoutedTextMeasurer<'_> {
    fn cancellation_requested(&self) -> bool {
        self.controlled_operation_phase
            .is_some_and(|phase| self.work_meter.checkpoint(phase).is_err())
    }

    #[allow(private_interfaces)]
    fn builtin_operation_carrier(
        &self,
        operation: TextMeasurementOperation,
    ) -> Option<BuiltinTextMeasurementOperationCarrier> {
        RoutedTextMeasurer::builtin_operation_carrier(self, operation)
    }

    #[allow(private_interfaces)]
    fn begin_svg_text_computed_length(
        &self,
        style: &TextStyle,
    ) -> Option<BuiltinSvgComputedLength> {
        self.builtin_operation_carrier(TextMeasurementOperation::ComputedLength)
            .and_then(|carrier| carrier.into_svg_computed_length(style))
    }

    #[allow(private_interfaces)]
    fn begin_html_line_width(&self, style: &TextStyle) -> Option<BuiltinHtmlLineWidth> {
        self.builtin_operation_carrier_for_phase(
            TextMeasurementPhase::Wrap,
            TextMeasurementOperation::CanvasMeasureTextWidth,
        )
        .and_then(|carrier| carrier.into_html_line_width(style))
    }

    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        // A candidate is already decoded, single-line text. Use its raw advance without
        // normalizing it as a complete HTML label, while retaining the Wrap route and callbacks.
        self.resolve(
            HostTextMeasurementRequest {
                operation: TextMeasurementOperation::CanvasMeasureTextWidth,
                phase: TextMeasurementPhase::Wrap,
                text,
                style,
                max_width: None,
                wrap_mode: WrapMode::HtmlLike,
            },
            decode_host_length,
            |profile| profile.measure_html_line_width_px(text, style),
        )
    }

    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.resolve(
            self.request(
                TextMeasurementOperation::Measure,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_metrics,
            |profile| profile.measure(text, style),
        )
    }

    fn measure_svg_text_computed_length_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::ComputedLength,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_text_computed_length_px(text, style),
        )
    }

    fn measure_svg_text_bbox_x(&self, text: &str, style: &TextStyle) -> (f64, f64) {
        self.resolve(
            self.request(
                TextMeasurementOperation::BBoxX,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_extents,
            |profile| profile.measure_svg_text_bbox_x(text, style),
        )
    }

    fn measure_svg_text_bbox_x_with_ascii_overhang(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> (f64, f64) {
        self.resolve(
            self.request(
                TextMeasurementOperation::BBoxXWithAsciiOverhang,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_extents,
            |profile| profile.measure_svg_text_bbox_x_with_ascii_overhang(text, style),
        )
    }

    fn measure_svg_title_bbox_x(&self, text: &str, style: &TextStyle) -> (f64, f64) {
        self.resolve(
            self.request(
                TextMeasurementOperation::TitleBBoxX,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_extents,
            |profile| profile.measure_svg_title_bbox_x(text, style),
        )
    }

    fn measure_svg_simple_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::SimpleBBoxWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_simple_text_bbox_width_px(text, style),
        )
    }

    fn measure_svg_raw_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::RawBBoxWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_raw_text_bbox_width_px(text, style),
        )
    }

    fn measure_svg_raw_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::RawBBoxHeight,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_raw_text_bbox_height_px(text, style),
        )
    }

    fn measure_svg_text_bounding_client_rect_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::BoundingClientRectWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_text_bounding_client_rect_width_px(text, style),
        )
    }

    fn measure_svg_raw_text_bbox_y_offset_px(&self, text: &str, style: &TextStyle) -> Option<f64> {
        self.resolve(
            self.request(
                TextMeasurementOperation::RawBBoxYOffset,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            |value| decode_host_length(value).map(Some),
            |profile| profile.measure_svg_raw_text_bbox_y_offset_px(text, style),
        )
    }

    fn measure_svg_raw_text_middle_bbox_y_offset_px(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> Option<f64> {
        self.resolve(
            self.request(
                TextMeasurementOperation::RawMiddleBBoxYOffset,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            |value| decode_host_length(value).map(Some),
            |profile| profile.measure_svg_raw_text_middle_bbox_y_offset_px(text, style),
        )
    }

    fn measure_svg_raw_text_hanging_bbox_y_offset_px(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> Option<f64> {
        self.resolve(
            self.request(
                TextMeasurementOperation::RawHangingBBoxYOffset,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            |value| decode_host_length(value).map(Some),
            |profile| profile.measure_svg_raw_text_hanging_bbox_y_offset_px(text, style),
        )
    }

    fn measure_svg_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::TspanBBoxWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_tspan_text_bbox_width_px(text, style),
        )
    }

    fn measure_svg_tspan_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::TspanBBoxHeight,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_tspan_text_bbox_height_px(text, style),
        )
    }

    fn measure_svg_preserved_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::TspanBBoxWidth,
                text,
                style,
                None,
                WrapMode::HtmlLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_preserved_tspan_text_bbox_width_px(text, style),
        )
    }

    fn measure_svg_create_text_bbox_y_offset_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::CreateTextBBoxYOffset,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_create_text_bbox_y_offset_px(text, style),
        )
    }

    fn measure_svg_create_text_middle_bbox_y_offset_px(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::CreateTextMiddleBBoxYOffset,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_create_text_middle_bbox_y_offset_px(text, style),
        )
    }

    fn measure_svg_simple_text_bbox_width_for_wrap_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::WrapProbeBBoxWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_simple_text_bbox_width_for_wrap_px(text, style),
        )
    }

    fn measure_mermaid_calculate_text_dimensions(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> TextMetrics {
        self.resolve(
            self.request(
                TextMeasurementOperation::MermaidCalculateTextDimensions,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_metrics,
            |profile| profile.measure_mermaid_calculate_text_dimensions(text, style),
        )
    }

    fn measure_canvas_text_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::CanvasMeasureTextWidth,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_canvas_text_width_px(text, style),
        )
    }

    fn measure_svg_simple_text_bbox_height_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.resolve(
            self.request(
                TextMeasurementOperation::SimpleBBoxHeight,
                text,
                style,
                None,
                WrapMode::SvgLike,
            ),
            decode_host_length,
            |profile| profile.measure_svg_simple_text_bbox_height_px(text, style),
        )
    }

    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> TextMetrics {
        self.resolve(
            self.request(
                TextMeasurementOperation::Wrapped,
                text,
                style,
                max_width,
                wrap_mode,
            ),
            decode_host_metrics,
            |profile| profile.measure_wrapped(text, style, max_width, wrap_mode),
        )
    }

    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        self.resolve(
            self.request(
                TextMeasurementOperation::WrappedWithRawWidth,
                text,
                style,
                max_width,
                wrap_mode,
            ),
            decode_host_wrapped_with_raw_width,
            |profile| profile.measure_wrapped_with_raw_width(text, style, max_width, wrap_mode),
        )
    }
}

fn decode_host_metrics(measurement: HostTextMeasurement) -> Option<TextMetrics> {
    match measurement {
        HostTextMeasurement::Metrics(metrics) => Some(metrics),
        _ => None,
    }
}

fn decode_host_length(measurement: HostTextMeasurement) -> Option<f64> {
    match measurement {
        HostTextMeasurement::Length(length) => Some(length),
        _ => None,
    }
}

fn decode_host_extents(measurement: HostTextMeasurement) -> Option<(f64, f64)> {
    match measurement {
        HostTextMeasurement::HorizontalExtents { left, right } => Some((left, right)),
        _ => None,
    }
}

fn decode_host_wrapped_with_raw_width(
    measurement: HostTextMeasurement,
) -> Option<(TextMetrics, Option<f64>)> {
    match measurement {
        HostTextMeasurement::WrappedWithRawWidth { metrics, raw_width } => {
            Some((metrics, raw_width))
        }
        _ => None,
    }
}

/// Checks a host callback value against the complete operation request.
///
/// The validator is the single authority for result shape and numeric bounds across direct
/// renderer hosts and every binding transport.
pub fn validate_host_text_measurement(
    request: &HostTextMeasurementRequest<'_>,
    measurement: &HostTextMeasurement,
) -> Result<(), HostTextMeasurementError> {
    let result_kind = match measurement {
        HostTextMeasurement::Metrics(_) => TextMeasurementResultKind::Metrics,
        HostTextMeasurement::Length(_) => TextMeasurementResultKind::Length,
        HostTextMeasurement::HorizontalExtents { .. } => {
            TextMeasurementResultKind::HorizontalExtents
        }
        HostTextMeasurement::WrappedWithRawWidth { .. } => {
            TextMeasurementResultKind::WrappedWithRawWidth
        }
    };
    let required_kind = request.operation.required_result_kind();
    if result_kind != required_kind {
        return Err(HostTextMeasurementError::invalid_value(format!(
            "host text measurement operation `{}` requires `{}` but returned `{}`",
            request.operation.external_name(),
            required_kind.external_name(),
            result_kind.external_name(),
        )));
    }

    let valid = match measurement {
        HostTextMeasurement::Metrics(metrics) => valid_metrics(request, metrics),
        HostTextMeasurement::Length(value) => {
            value.is_finite() && (request.operation.accepts_signed_length() || *value >= 0.0)
        }
        HostTextMeasurement::HorizontalExtents { left, right } => valid_extents(*left, *right),
        HostTextMeasurement::WrappedWithRawWidth { metrics, raw_width } => {
            valid_metrics(request, metrics)
                && raw_width.is_none_or(|value| value.is_finite() && value >= 0.0)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(HostTextMeasurementError::invalid_value(format!(
            "host text measurement operation `{}` returned an invalid `{}` value",
            request.operation.external_name(),
            result_kind.external_name(),
        )))
    }
}

fn valid_metrics(request: &HostTextMeasurementRequest<'_>, metrics: &TextMetrics) -> bool {
    metrics.width.is_finite()
        && metrics.height.is_finite()
        && metrics.width >= 0.0
        && metrics.height >= 0.0
        && metrics.line_count > 0
        && metrics.line_count <= request.text.len().saturating_add(1)
}

fn valid_extents(left: f64, right: f64) -> bool {
    left.is_finite()
        && right.is_finite()
        && left >= 0.0
        && right >= 0.0
        && (left + right).is_finite()
}

#[cfg(feature = "math")]
fn default_math_renderer() -> Option<Arc<dyn MathRenderer + Send + Sync>> {
    Some(Arc::new(crate::math::RatexMathRenderer))
}

#[cfg(not(feature = "math"))]
fn default_math_renderer() -> Option<Arc<dyn MathRenderer + Send + Sync>> {
    None
}

/// Immutable render services and the policy used to capture one operation context.
#[derive(Clone)]
pub struct RenderEnvironment {
    text_measurement: TextMeasurementPolicy,
    capability_policy: RenderCapabilityPolicy,
    math_renderer: Option<Arc<dyn MathRenderer + Send + Sync>>,
    icon_registry: Option<IconRegistry>,
    runtime_policy: RuntimePolicy,
    resource_policy: RenderResourcePolicy,
}

impl fmt::Debug for RenderEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderEnvironment")
            .field("text_measurement", &self.text_measurement)
            .field("capability_policy", &self.capability_policy)
            .field(
                "has_math_renderer",
                &(self.capability_policy.allows(RenderCapability::Math)
                    && self.math_renderer.is_some()),
            )
            .field("has_icon_registry", &self.icon_registry.is_some())
            .field("runtime_policy", &self.runtime_policy)
            .field("resource_policy", &self.resource_policy)
            .finish_non_exhaustive()
    }
}

impl RenderEnvironment {
    /// Creates a target-independent environment with fixed time, UTC, and a fixed seed.
    ///
    /// When the `math` capability is compiled, the environment also installs its built-in math
    /// renderer. Builds without that capability leave the service absent so family admission can
    /// return a typed missing-capability error.
    pub fn deterministic() -> Self {
        Self {
            text_measurement: TextMeasurementPolicy::deterministic(),
            capability_policy: RenderCapabilityPolicy::unrestricted(),
            math_renderer: default_math_renderer(),
            icon_registry: None,
            runtime_policy: RuntimePolicy::deterministic(),
            resource_policy: RenderResourcePolicy::interactive(),
        }
    }

    /// Creates an environment backed by native clock, timezone, and randomness adapters.
    ///
    /// Timing remains an explicit opt-in because it adds work and observable diagnostics.
    pub fn try_native() -> Result<Self, RuntimePolicyError> {
        Ok(Self::deterministic().with_runtime_policy(RuntimePolicy::try_native()?))
    }

    pub fn with_text_measurement_policy(mut self, policy: TextMeasurementPolicy) -> Self {
        self.text_measurement = policy;
        self
    }

    /// Restricts optional renderer capabilities for every operation begun by this environment.
    ///
    /// This is primarily useful to artifact owners whose public feature contract can be narrower
    /// than Cargo's resolved dependency feature union.
    pub const fn with_capability_policy(mut self, policy: RenderCapabilityPolicy) -> Self {
        self.capability_policy = policy;
        self
    }

    /// Installs the math renderer compiled into this renderer, if present.
    ///
    /// Facades use this to select the canonical compiled capability instead of duplicating Cargo
    /// feature checks in each transport layer.
    pub fn with_compiled_math_renderer(mut self) -> Self {
        self.math_renderer = default_math_renderer();
        self
    }

    pub fn with_math_renderer(mut self, renderer: Arc<dyn MathRenderer + Send + Sync>) -> Self {
        self.math_renderer = Some(renderer);
        self
    }

    pub fn without_math_renderer(mut self) -> Self {
        self.math_renderer = None;
        self
    }

    pub fn with_icon_registry(mut self, registry: IconRegistry) -> Self {
        self.icon_registry = Some(registry);
        self
    }

    pub fn with_runtime_policy(mut self, policy: RuntimePolicy) -> Self {
        self.runtime_policy = policy;
        self
    }

    pub fn runtime_policy(&self) -> &RuntimePolicy {
        &self.runtime_policy
    }

    pub const fn with_resource_policy(mut self, policy: RenderResourcePolicy) -> Self {
        self.resource_policy = policy;
        self
    }

    /// Captures time, timezone rules, random seed, and provenance exactly once.
    pub fn begin_session(&self) -> Result<RenderSession, RuntimePolicyError> {
        self.begin_session_with_control(OperationControl::new())
    }

    /// Captures one render session using caller-owned cancellation/deadline state.
    ///
    /// The control is cloned into the operation work meter so layout adapters, SVG emission, and
    /// postprocessing observe the same operation scope. This method does not create another
    /// control or expose a second cancellation state.
    pub fn begin_session_with_control(
        &self,
        control: OperationControl,
    ) -> Result<RenderSession, RuntimePolicyError> {
        let operation_context = self.runtime_policy.begin_operation()?;
        Ok(self.begin_session_in_context(operation_context, control))
    }

    /// Begins one render session from caller-captured operation state.
    ///
    /// This entry point deliberately does not call [`RuntimePolicy::begin_operation`]. Facades
    /// that already own the source-to-output operation use it to keep parsing and rendering on
    /// the same runtime context and cancellation/deadline control.
    pub fn begin_session_in_context(
        &self,
        operation_context: OperationContext,
        control: OperationControl,
    ) -> RenderSession {
        RenderSession {
            text_measurement: self.text_measurement.clone(),
            measurement_recorder: Box::default(),
            capability_policy: self.capability_policy,
            math_renderer: self.math_renderer.clone(),
            icon_registry: self.icon_registry.clone(),
            operation_context,
            resource_policy: self.resource_policy,
            work_meter: Arc::new(OperationWorkMeter::new_with_control(
                self.resource_policy,
                control,
            )),
        }
    }
}

impl Default for RenderEnvironment {
    fn default() -> Self {
        Self::deterministic()
    }
}

/// Opaque operation session. Family code receives only the narrow projection it needs.
pub struct RenderSession {
    text_measurement: TextMeasurementPolicy,
    // Keep movable family artifacts compact for bounded worker stacks.
    measurement_recorder: Box<TextMeasurementRecorder>,
    capability_policy: RenderCapabilityPolicy,
    math_renderer: Option<Arc<dyn MathRenderer + Send + Sync>>,
    icon_registry: Option<IconRegistry>,
    operation_context: OperationContext,
    resource_policy: RenderResourcePolicy,
    work_meter: Arc<OperationWorkMeter>,
}

impl RenderSession {
    pub fn text_measurer(&self, default_phase: TextMeasurementPhase) -> RoutedTextMeasurer<'_> {
        self.routed_text_measurer(default_phase, None)
    }

    pub(crate) fn controlled_text_measurer(
        &self,
        default_phase: TextMeasurementPhase,
        operation_phase: OperationPhase,
    ) -> RoutedTextMeasurer<'_> {
        self.routed_text_measurer(default_phase, Some(operation_phase))
    }

    fn routed_text_measurer(
        &self,
        default_phase: TextMeasurementPhase,
        controlled_operation_phase: Option<OperationPhase>,
    ) -> RoutedTextMeasurer<'_> {
        RoutedTextMeasurer {
            default_phase,
            policy: &self.text_measurement,
            recorder: &self.measurement_recorder,
            work_meter: self.work_meter.as_ref(),
            controlled_operation_phase,
        }
    }

    pub fn text_measurement_route(&self, phase: TextMeasurementPhase) -> TextMeasurementRoute {
        self.text_measurement.route(phase)
    }

    pub fn text_measurement_report(&self) -> TextMeasurementReport {
        self.measurement_recorder.report(&self.text_measurement)
    }

    pub fn operation_context(&self) -> &OperationContext {
        &self.operation_context
    }

    pub fn operation_timing(&self) -> Option<OperationTiming> {
        self.operation_context.timing()
    }

    pub const fn unix_millis(&self) -> i64 {
        self.operation_context.unix_millis()
    }

    pub const fn local_date(&self) -> merman_core::time::CivilDate {
        self.operation_context.today_local()
    }

    pub fn local_time_zone(&self) -> &merman_core::time::LocalTimeZone {
        self.operation_context.local_time_zone()
    }

    pub fn render_seed(&self) -> NonZeroU64 {
        self.operation_context.derive_nonzero_u64("render.root", 0)
    }

    pub const fn resource_policy(&self) -> RenderResourcePolicy {
        self.resource_policy
    }

    /// Reports effective operation availability after policy and backend/service resolution.
    pub(crate) fn supports_capability(&self, capability: RenderCapability) -> bool {
        if !self.capability_policy.allows(capability) {
            return false;
        }
        match capability {
            RenderCapability::LayoutCytoscape => crate::layout_cytoscape_available(),
            RenderCapability::LayoutElk => crate::layout_elk_available(),
            RenderCapability::Math => self.math_renderer.is_some(),
        }
    }

    pub(crate) fn work_meter(&self) -> &Arc<OperationWorkMeter> {
        &self.work_meter
    }

    /// Checks the operation-owned control at an SVG/render phase boundary.
    pub(crate) fn checkpoint(&self, phase: OperationPhase) -> crate::Result<()> {
        self.work_meter().checkpoint(phase).map_err(Into::into)
    }

    pub fn math_renderer(&self) -> Option<&(dyn MathRenderer + Send + Sync)> {
        if self.supports_capability(RenderCapability::Math) {
            self.math_renderer.as_deref()
        } else {
            None
        }
    }

    pub fn icon_registry(&self) -> Option<&IconRegistry> {
        self.icon_registry.as_ref()
    }

    /// Freezes the observable policy and provenance accumulated so far.
    pub fn report(&self) -> RenderSessionReport {
        RenderSessionReport {
            measurement_routes: self.text_measurement.routes(),
            measurement: self.measurement_recorder.report(&self.text_measurement),
            operation_context: self.operation_context.clone(),
            local_time_zone: self
                .operation_context
                .local_time_zone()
                .provenance()
                .clone(),
            resource_policy: self.resource_policy,
            layout_work_units: self.work_meter.used(),
        }
    }
}

/// Immutable environment evidence accumulated by an operation session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSessionReport {
    measurement_routes: [TextMeasurementRoute; 4],
    measurement: TextMeasurementReport,
    operation_context: OperationContext,
    local_time_zone: LocalTimeZoneProvenance,
    resource_policy: RenderResourcePolicy,
    layout_work_units: usize,
}

impl RenderSessionReport {
    pub fn measurement_routes(&self) -> &[TextMeasurementRoute; 4] {
        &self.measurement_routes
    }

    pub fn measurement(&self) -> &TextMeasurementReport {
        &self.measurement
    }

    pub fn operation_context(&self) -> &OperationContext {
        &self.operation_context
    }

    pub const fn unix_millis(&self) -> i64 {
        self.operation_context.unix_millis()
    }

    pub const fn local_date(&self) -> merman_core::time::CivilDate {
        self.operation_context.today_local()
    }

    pub fn local_time_zone(&self) -> &LocalTimeZoneProvenance {
        &self.local_time_zone
    }

    pub fn render_seed(&self) -> NonZeroU64 {
        self.operation_context.derive_nonzero_u64("render.root", 0)
    }

    pub const fn resource_policy(&self) -> RenderResourcePolicy {
        self.resource_policy
    }

    /// Returns the deterministic owner-accounted layout and geometry work consumed so far.
    ///
    /// This value is useful for resource-policy calibration. It is not elapsed time, an
    /// instruction count, or a portable latency estimate.
    pub const fn layout_work_units(&self) -> usize {
        self.layout_work_units
    }
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn inline_html_carrier<M: TextMeasurer + ?Sized>(measurer: &M) -> InlineHtmlMeasurementCarrier {
        measurer
            .builtin_operation_carrier(TextMeasurementOperation::WrappedWithRawWidth)
            .and_then(BuiltinTextMeasurementOperationCarrier::into_inline_html)
            .unwrap_or_else(InlineHtmlMeasurementCarrier::opaque)
    }

    #[test]
    fn deterministic_profile_identity_tracks_the_render_crate() {
        let profile = deterministic_profile();

        assert_eq!(
            profile.identity().profile().as_str(),
            "merman.deterministic-text"
        );
        assert_eq!(
            profile.identity().version(),
            concat!("merman-render@", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn text_measurement_operations_have_stable_external_mappings() {
        let mappings = TextMeasurementOperation::ALL
            .map(|operation| (operation.external_code(), operation.external_name()));

        assert_eq!(
            mappings,
            [
                (0, "measure"),
                (1, "computed-length"),
                (2, "bbox-x"),
                (3, "bbox-x-with-ascii-overhang"),
                (4, "title-bbox-x"),
                (5, "simple-bbox-width"),
                (6, "raw-bbox-width"),
                (7, "tspan-bbox-width"),
                (8, "tspan-bbox-height"),
                (9, "wrap-probe-bbox-width"),
                (10, "simple-bbox-height"),
                (11, "wrapped"),
                (12, "wrapped-with-raw-width"),
                (13, "bounding-client-rect-width"),
                (14, "create-text-bbox-y-offset"),
                (15, "mermaid-calculate-text-dimensions"),
                (16, "canvas-measure-text-width"),
                (17, "create-text-middle-bbox-y-offset"),
                (18, "raw-bbox-height"),
                (19, "raw-bbox-y-offset"),
                (20, "raw-middle-bbox-y-offset"),
                (21, "raw-hanging-bbox-y-offset"),
            ]
        );
    }

    #[test]
    fn deterministic_environment_projects_one_operation_context_into_the_report() {
        let runtime_policy = RuntimePolicy::deterministic()
            .with_fixed_unix_millis(1_704_067_200_000)
            .try_with_fixed_local_offset_minutes(480)
            .expect("valid fixed offset")
            .with_fixed_seed(77);
        let environment = RenderEnvironment::deterministic().with_runtime_policy(runtime_policy);

        let session = environment.begin_session().expect("render session");
        let captured = session.operation_context().clone();
        let report = session.report();

        assert_eq!(captured.unix_millis(), 1_704_067_200_000);
        assert_eq!(captured.seed(), 77);
        assert_eq!(captured.local_time_zone().fixed_offset_minutes(), Some(480));
        assert_eq!(report.operation_context(), &captured);
        assert_eq!(report.unix_millis(), captured.unix_millis());
        assert_eq!(report.operation_context().seed(), captured.seed());
        assert_eq!(
            report.render_seed(),
            captured.derive_nonzero_u64("render.root", 0)
        );
        assert_eq!(
            report.local_time_zone(),
            captured.local_time_zone().provenance()
        );
    }

    #[test]
    fn caller_captured_context_and_control_define_one_render_session() {
        let operation_context = RuntimePolicy::deterministic()
            .with_fixed_unix_millis(1_704_067_200_123)
            .with_fixed_seed(91)
            .begin_operation()
            .expect("caller operation context");
        let control = OperationControl::new();
        let session = RenderEnvironment::deterministic()
            .begin_session_in_context(operation_context.clone(), control.clone());

        assert_eq!(session.operation_context(), &operation_context);

        control.cancel();
        let error = session
            .checkpoint(OperationPhase::Layout)
            .expect_err("shared control should cancel the render session");
        let crate::Error::Cancelled(cancelled) = error else {
            panic!("expected structured cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Layout);
        assert_eq!(cancelled.reason, merman_core::CancelReason::Requested);
    }

    #[test]
    fn descriptor_drives_host_result_validation_for_every_operation() {
        let style = TextStyle::default();
        let request = |operation| HostTextMeasurementRequest {
            operation,
            phase: TextMeasurementPhase::Layout,
            text: "contract",
            style: &style,
            max_width: None,
            wrap_mode: WrapMode::SvgLike,
        };
        let valid_metrics_value = HostTextMeasurement::Metrics(metrics(10.0));
        let valid_length = HostTextMeasurement::Length(10.0);
        let negative_length = HostTextMeasurement::Length(-10.0);
        let invalid_length = HostTextMeasurement::Length(f64::NAN);
        let valid_extents = HostTextMeasurement::HorizontalExtents {
            left: 1.0,
            right: 2.0,
        };
        let valid_wrapped = HostTextMeasurement::WrappedWithRawWidth {
            metrics: metrics(10.0),
            raw_width: Some(11.0),
        };

        for operation in TextMeasurementOperation::ALL {
            let required = operation.required_result_kind();
            assert_eq!(
                validate_host_text_measurement(&request(operation), &valid_metrics_value).is_ok(),
                required == TextMeasurementResultKind::Metrics,
                "{} metrics contract",
                operation.external_name()
            );
            assert_eq!(
                validate_host_text_measurement(&request(operation), &valid_length).is_ok(),
                required == TextMeasurementResultKind::Length,
                "{} length contract",
                operation.external_name()
            );
            assert_eq!(
                validate_host_text_measurement(&request(operation), &negative_length).is_ok(),
                required == TextMeasurementResultKind::Length && operation.accepts_signed_length(),
                "{} signed-length contract",
                operation.external_name()
            );
            assert!(
                validate_host_text_measurement(&request(operation), &invalid_length).is_err(),
                "{} accepted a non-finite length",
                operation.external_name()
            );
            assert_eq!(
                validate_host_text_measurement(&request(operation), &valid_extents).is_ok(),
                required == TextMeasurementResultKind::HorizontalExtents,
                "{} extents contract",
                operation.external_name()
            );
            assert_eq!(
                validate_host_text_measurement(&request(operation), &valid_wrapped).is_ok(),
                required == TextMeasurementResultKind::WrappedWithRawWidth,
                "{} wrapped contract",
                operation.external_name()
            );
        }
    }

    #[test]
    fn checked_host_measurement_rejects_malformed_numeric_boundaries() {
        let style = TextStyle::default();
        let request = |operation, text| HostTextMeasurementRequest {
            operation,
            phase: TextMeasurementPhase::Layout,
            text,
            style: &style,
            max_width: None,
            wrap_mode: WrapMode::SvgLike,
        };
        let metrics_value = |width, height, line_count| {
            HostTextMeasurement::Metrics(TextMetrics {
                width,
                height,
                line_count,
            })
        };

        let metrics_request = request(TextMeasurementOperation::Measure, "abc");
        assert!(
            validate_host_text_measurement(&metrics_request, &metrics_value(1.0, 2.0, 4)).is_ok()
        );
        for invalid in [
            metrics_value(f64::NAN, 2.0, 1),
            metrics_value(f64::INFINITY, 2.0, 1),
            metrics_value(-1.0, 2.0, 1),
            metrics_value(1.0, f64::NAN, 1),
            metrics_value(1.0, f64::INFINITY, 1),
            metrics_value(1.0, -2.0, 1),
            metrics_value(1.0, 2.0, 0),
            metrics_value(1.0, 2.0, 5),
        ] {
            assert!(validate_host_text_measurement(&metrics_request, &invalid).is_err());
        }

        let length_request = request(TextMeasurementOperation::ComputedLength, "abc");
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert!(
                validate_host_text_measurement(
                    &length_request,
                    &HostTextMeasurement::Length(value),
                )
                .is_err()
            );
        }
        for operation in [
            TextMeasurementOperation::CreateTextBBoxYOffset,
            TextMeasurementOperation::CreateTextMiddleBBoxYOffset,
        ] {
            assert!(
                validate_host_text_measurement(
                    &request(operation, "abc"),
                    &HostTextMeasurement::Length(-1.0),
                )
                .is_ok()
            );
        }

        let extents_request = request(TextMeasurementOperation::BBoxX, "abc");
        for (left, right) in [
            (f64::NAN, 1.0),
            (1.0, f64::INFINITY),
            (-1.0, 1.0),
            (1.0, -1.0),
            (f64::MAX, f64::MAX),
        ] {
            assert!(
                validate_host_text_measurement(
                    &extents_request,
                    &HostTextMeasurement::HorizontalExtents { left, right },
                )
                .is_err()
            );
        }

        let wrapped_request = request(TextMeasurementOperation::WrappedWithRawWidth, "abc");
        assert!(
            validate_host_text_measurement(
                &wrapped_request,
                &HostTextMeasurement::WrappedWithRawWidth {
                    metrics: TextMetrics {
                        width: 10.0,
                        height: 20.0,
                        line_count: 1,
                    },
                    raw_width: Some(1.0),
                },
            )
            .is_ok(),
            "raw width may be smaller than wrapped width"
        );
        for raw_width in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(
                validate_host_text_measurement(
                    &wrapped_request,
                    &HostTextMeasurement::WrappedWithRawWidth {
                        metrics: metrics(10.0),
                        raw_width: Some(raw_width),
                    },
                )
                .is_err()
            );
        }
    }

    struct OperationAwareHost {
        operations: Arc<Mutex<Vec<TextMeasurementOperation>>>,
    }

    #[test]
    fn preserved_tspan_width_reuses_operation_seven_with_explicit_whitespace_mode() {
        struct PreservedTspanHost;

        impl HostTextMeasurer for PreservedTspanHost {
            fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
                assert_eq!(request.operation, TextMeasurementOperation::TspanBBoxWidth);
                assert_eq!(request.operation.external_code(), 7);
                assert_eq!(request.phase, TextMeasurementPhase::SvgBBox);
                assert_eq!(request.text, "a  b");
                assert_eq!(request.max_width, None);
                assert_eq!(request.wrap_mode, WrapMode::HtmlLike);
                Ok(Some(HostTextMeasurement::Length(37.5)))
            }
        }

        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.preserved-tspan-host", "v1", &[]),
            Arc::new(PreservedTspanHost),
            [TextMeasurementPhase::SvgBBox],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");
        assert_eq!(
            session
                .text_measurer(TextMeasurementPhase::Layout)
                .measure_svg_preserved_tspan_text_bbox_width_px("a  b", &TextStyle::default()),
            37.5,
        );
    }

    impl HostTextMeasurer for OperationAwareHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            self.operations
                .lock()
                .expect("operation probe lock")
                .push(request.operation);
            match request.operation {
                TextMeasurementOperation::ComputedLength => {
                    Ok(Some(HostTextMeasurement::Length(73.25)))
                }
                TextMeasurementOperation::BoundingClientRectWidth => {
                    Ok(Some(HostTextMeasurement::Length(91.875)))
                }
                TextMeasurementOperation::CreateTextBBoxYOffset => {
                    Ok(Some(HostTextMeasurement::Length(-1.25)))
                }
                TextMeasurementOperation::MermaidCalculateTextDimensions => {
                    Ok(Some(HostTextMeasurement::Metrics(metrics(82.5))))
                }
                TextMeasurementOperation::CanvasMeasureTextWidth => {
                    Ok(Some(HostTextMeasurement::Length(94.25)))
                }
                TextMeasurementOperation::CreateTextMiddleBBoxYOffset => {
                    Ok(Some(HostTextMeasurement::Length(-2.5)))
                }
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn host_computed_length_receives_exact_operation_and_is_authoritative() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.operation-aware-host", "v1", &[]),
            Arc::new(OperationAwareHost {
                operations: Arc::clone(&operations),
            }),
            [TextMeasurementPhase::ComputedLength],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");

        let length = session
            .text_measurer(TextMeasurementPhase::Layout)
            .measure_svg_text_computed_length_px("operation", &TextStyle::default());

        assert_eq!(length, 73.25);
        assert_eq!(
            *operations.lock().expect("operation probe lock"),
            [TextMeasurementOperation::ComputedLength]
        );
    }

    #[test]
    fn host_bounding_client_rect_width_receives_exact_operation_and_is_authoritative() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.operation-aware-host", "v1", &[]),
            Arc::new(OperationAwareHost {
                operations: Arc::clone(&operations),
            }),
            [TextMeasurementPhase::SvgBBox],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");

        let length = session
            .text_measurer(TextMeasurementPhase::Layout)
            .measure_svg_text_bounding_client_rect_width_px("operation", &TextStyle::default());

        assert_eq!(length, 91.875);
        assert_eq!(
            *operations.lock().expect("operation probe lock"),
            [TextMeasurementOperation::BoundingClientRectWidth]
        );
    }

    #[test]
    fn host_create_text_bbox_y_offset_accepts_signed_authoritative_values() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.operation-aware-host", "v1", &[]),
            Arc::new(OperationAwareHost {
                operations: Arc::clone(&operations),
            }),
            [TextMeasurementPhase::SvgBBox],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");

        let offset = session
            .text_measurer(TextMeasurementPhase::Layout)
            .measure_svg_create_text_bbox_y_offset_px("operation", &TextStyle::default());

        assert_eq!(offset, -1.25);
        assert_eq!(
            *operations.lock().expect("operation probe lock"),
            [TextMeasurementOperation::CreateTextBBoxYOffset]
        );
    }

    #[test]
    fn host_create_text_middle_bbox_y_offset_is_a_distinct_signed_operation() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.operation-aware-host", "v1", &[]),
            Arc::new(OperationAwareHost {
                operations: Arc::clone(&operations),
            }),
            [TextMeasurementPhase::SvgBBox],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");

        let offset = session
            .text_measurer(TextMeasurementPhase::Layout)
            .measure_svg_create_text_middle_bbox_y_offset_px("operation", &TextStyle::default());

        assert_eq!(offset, -2.5);
        assert_eq!(
            *operations.lock().expect("operation probe lock"),
            [TextMeasurementOperation::CreateTextMiddleBBoxYOffset]
        );
    }

    #[test]
    fn host_source_specific_width_operations_are_authoritative() {
        let operations = Arc::new(Mutex::new(Vec::new()));
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.operation-aware-host", "v1", &[]),
            Arc::new(OperationAwareHost {
                operations: Arc::clone(&operations),
            }),
            [TextMeasurementPhase::SvgBBox, TextMeasurementPhase::Layout],
            deterministic_profile(),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session()
            .expect("begin render session");
        let measurer = session.text_measurer(TextMeasurementPhase::Layout);

        assert_eq!(
            measurer
                .measure_mermaid_calculate_text_dimensions("operation", &TextStyle::default())
                .width,
            82.5,
        );
        assert_eq!(
            measurer.measure_canvas_text_width_px("operation", &TextStyle::default()),
            94.25
        );
        assert_eq!(
            *operations.lock().expect("operation probe lock"),
            [
                TextMeasurementOperation::MermaidCalculateTextDimensions,
                TextMeasurementOperation::CanvasMeasureTextWidth,
            ]
        );
    }

    fn identity(
        profile: &str,
        version: &str,
        decorators: &[&str],
    ) -> TextMeasurementProfileIdentity {
        TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new(profile).expect("valid test profile"),
            version,
        )
        .expect("valid test version")
        .with_decorators(decorators.iter().copied())
        .expect("valid test decorators")
    }

    fn metrics(width: f64) -> TextMetrics {
        TextMetrics {
            width,
            height: width + 1.0,
            line_count: 1,
        }
    }

    #[derive(Debug, Default)]
    struct SpecializedProfile;

    impl TextMeasurer for SpecializedProfile {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            metrics(1.0)
        }

        fn measure_svg_text_computed_length_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            2.0
        }

        fn measure_svg_text_bbox_x(&self, _text: &str, _style: &TextStyle) -> (f64, f64) {
            (3.0, 4.0)
        }

        fn measure_svg_text_bbox_x_with_ascii_overhang(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> (f64, f64) {
            (5.0, 6.0)
        }

        fn measure_svg_title_bbox_x(&self, _text: &str, _style: &TextStyle) -> (f64, f64) {
            (7.0, 8.0)
        }

        fn measure_svg_simple_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            9.0
        }

        fn measure_svg_raw_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            10.0
        }

        fn measure_svg_tspan_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            10.5
        }

        fn measure_svg_tspan_text_bbox_height_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            11.5
        }

        fn measure_svg_create_text_bbox_y_offset_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            -1.25
        }

        fn measure_svg_create_text_middle_bbox_y_offset_px(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> f64 {
            -2.5
        }

        fn measure_svg_simple_text_bbox_width_for_wrap_px(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> f64 {
            11.0
        }

        fn measure_mermaid_calculate_text_dimensions(
            &self,
            _text: &str,
            _style: &TextStyle,
        ) -> TextMetrics {
            metrics(11.25)
        }

        fn measure_canvas_text_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            11.75
        }

        fn measure_svg_simple_text_bbox_height_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            12.0
        }

        fn measure_wrapped(
            &self,
            _text: &str,
            _style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> TextMetrics {
            metrics(13.0)
        }

        fn measure_wrapped_with_raw_width(
            &self,
            _text: &str,
            _style: &TextStyle,
            _max_width: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> (TextMetrics, Option<f64>) {
            (metrics(14.0), Some(15.0))
        }
    }

    struct DecliningHost;

    impl HostTextMeasurer for DecliningHost {
        fn measure(&self, _request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            Ok(None)
        }
    }

    struct ForgedCarrierProfile;

    impl TextMeasurer for ForgedCarrierProfile {
        #[allow(private_interfaces)]
        fn builtin_operation_carrier(
            &self,
            operation: TextMeasurementOperation,
        ) -> Option<BuiltinTextMeasurementOperationCarrier> {
            Some(BuiltinTextMeasurementOperationCarrier {
                profile: BuiltinTextMeasurementProfile::Deterministic,
                phase: TextMeasurementPhase::SvgBBox,
                operation,
            })
        }

        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            metrics(1.0)
        }
    }

    #[test]
    fn private_operation_carriers_only_qualify_builtin_profile_routes() {
        assert!(
            BuiltinTextMeasurementOperationCarrier {
                profile: BuiltinTextMeasurementProfile::Deterministic,
                phase: TextMeasurementPhase::SvgBBox,
                operation: TextMeasurementOperation::WrappedWithRawWidth,
            }
            .into_inline_html()
            .is_none(),
            "the inline planner requires both the wrapped operation and its Wrap owner phase"
        );

        let builtin_session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin built-in session");
        let builtin = builtin_session.text_measurer(TextMeasurementPhase::Layout);
        let builtin_carrier = inline_html_carrier(&builtin);
        assert!(builtin_carrier.is_builtin());
        let sequence_carrier = builtin
            .builtin_operation_carrier(TextMeasurementOperation::MermaidCalculateTextDimensions)
            .expect("sequence measurement route is built-in");
        assert_eq!(sequence_carrier.phase, TextMeasurementPhase::SvgBBox);
        assert_eq!(
            sequence_carrier.operation,
            TextMeasurementOperation::MermaidCalculateTextDimensions
        );
        assert!(
            builtin
                .builtin_operation_carrier(TextMeasurementOperation::MermaidCalculateTextDimensions)
                .is_some()
        );

        let custom_profile = TextMeasurementProfile::new(
            identity("test.custom", "v1", &[]),
            Arc::new(ForgedCarrierProfile),
        );
        let custom_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::uniform(custom_profile))
            .begin_session()
            .expect("begin custom profile session");
        let custom = custom_session.text_measurer(TextMeasurementPhase::Layout);
        assert!(!inline_html_carrier(&custom).is_builtin());
        assert!(
            custom
                .builtin_operation_carrier(TextMeasurementOperation::MermaidCalculateTextDimensions)
                .is_none()
        );

        let host_policy = TextMeasurementPolicy::host_display(
            identity("test.host", "v1", &[]),
            Arc::new(DecliningHost),
            [TextMeasurementPhase::Wrap, TextMeasurementPhase::SvgBBox],
        );
        let host_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(host_policy)
            .begin_session()
            .expect("begin host session");
        let host = host_session.text_measurer(TextMeasurementPhase::Layout);
        assert!(
            !inline_html_carrier(&host).is_builtin(),
            "host routes remain opaque even when their fallback is deterministic"
        );
        assert!(
            host.builtin_operation_carrier(
                TextMeasurementOperation::MermaidCalculateTextDimensions
            )
            .is_none()
        );
    }

    #[test]
    fn builtin_inline_stream_preserves_decoded_candidates_and_backend_order() {
        fn assert_cases(
            backend: &dyn TextMeasurer,
            carrier: InlineHtmlMeasurementCarrier,
            style: &TextStyle,
            cases: &[(&str, &[&str])],
        ) {
            for (text, chunks) in cases {
                assert_eq!(chunks.concat(), *text);
                let expected = backend.measure_html_line_width_px(text, style);
                let mut streamed = carrier
                    .begin_inline_html_width(style)
                    .expect("built-in carrier starts a streaming width");
                for chunk in *chunks {
                    streamed.push_text(chunk);
                }
                assert_eq!(
                    streamed.width_px().to_bits(),
                    expected.to_bits(),
                    "text={text:?}, chunks={chunks:?}"
                );
            }
        }

        let cases: &[(&str, &[&str])] = &[
            ("AVATAR office", &["A", "VAT", "AR ", "office"]),
            (
                "alpha<br   />omega",
                &["alpha<", "b", "r ", "  /", ">", "omega"],
            ),
            ("<b<br/>tail", &["<b<", "br", "/>tail"]),
            ("wide<br / >literal", &["wide<br ", "/ ", ">literal"]),
            // ECMAScript `\s` also includes form feed. The headless built-in intentionally keeps
            // that spelling literal until browser-grade HTML parsing is available.
            (
                "wide<br\u{000C}/>literal",
                &["wide<br", "\u{000C}", "/>literal"],
            ),
            (
                "wide\n                         ",
                &["wide\n", "             ", "            "],
            ),
            (" \n  ", &[" ", "\n", "  "]),
            ("i\n\u{00a0}", &["i\n", "\u{00a0}"]),
            ("A\u{301}👩‍💻مرحبا世界", &["A\u{301}", "👩‍💻", "مرحبا", "世界"]),
            ("👩‍🔬", &["👩", "\u{200d}", "🔬"]),
            ("👨‍👩‍👧‍👦", &["👨‍", "👩", "\u{200d}👧‍", "👦"]),
            ("👍🏽", &["👍", "🏽"]),
            ("🇨🇳", &["🇨", "🇳"]),
            ("1️⃣", &["1", "\u{fe0f}", "\u{20e3}"]),
            ("😀\u{fe0e}", &["😀", "\u{fe0e}"]),
            ("k中A+", &["k中", "A+"]),
        ];
        let style = TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: Some("700".to_string()),
            font_style: Some("italic".to_string()),
        };

        let deterministic_session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin deterministic session");
        let deterministic_carrier =
            inline_html_carrier(&deterministic_session.text_measurer(TextMeasurementPhase::Wrap));
        assert_cases(
            &DeterministicTextMeasurer::default(),
            deterministic_carrier,
            &style,
            cases,
        );
        let long_whitespace = " ".repeat(8_192);
        let long_break = format!("wide<br{long_whitespace}/>tail");
        let expected =
            DeterministicTextMeasurer::default().measure_html_line_width_px(&long_break, &style);
        let mut streamed = deterministic_carrier
            .begin_inline_html_width(&style)
            .expect("deterministic carrier starts a streaming width");
        streamed.push_text("wide<br");
        streamed.push_text(&long_whitespace);
        streamed.push_text("/>tail");
        assert_eq!(streamed.width_px().to_bits(), expected.to_bits());

        let mut unknown_font = style.clone();
        unknown_font.font_family = Some("fixture-private-font".to_string());
        assert_cases(
            &DeterministicTextMeasurer::default(),
            deterministic_carrier,
            &unknown_font,
            cases,
        );

        let combining_tail = format!("A{}", "\u{0301}".repeat(1_024));
        assert_cases(
            &DeterministicTextMeasurer::default(),
            deterministic_carrier,
            &style,
            &[(combining_tail.as_str(), &[combining_tail.as_str()])],
        );
    }

    #[test]
    fn builtin_svg_computed_length_stream_is_sequence_aware_and_reversible() {
        let backend = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let cases: &[(&str, &[&str])] = &[
            ("👩‍🔬", &["👩", "\u{200d}", "🔬"]),
            ("👨‍👩‍👧‍👦", &["👨", "\u{200d}👩‍", "👧", "\u{200d}👦"]),
            ("👍🏽", &["👍", "🏽"]),
            ("🇨🇳", &["🇨", "🇳"]),
            ("1️⃣", &["1", "\u{fe0f}", "\u{20e3}"]),
            ("😀\u{fe0e}", &["😀", "\u{fe0e}"]),
            ("k中A+", &["k中", "A+"]),
        ];

        for (text, chunks) in cases {
            let mut streamed = BuiltinSvgComputedLength::deterministic(&style);
            let mut prefix = String::new();
            for chunk in *chunks {
                prefix.push_str(chunk);
                streamed.push_text(chunk);
                let expected = backend.measure_svg_text_computed_length_px(&prefix, &style);
                assert_eq!(
                    streamed.width_px().to_bits(),
                    expected.to_bits(),
                    "text={text:?}, prefix={prefix:?}"
                );
            }

            let checkpoint = streamed.clone();
            let expected = backend.measure_svg_text_computed_length_px(text, &style);
            assert_eq!(checkpoint.width_px().to_bits(), expected.to_bits());
            streamed.push_text("A");
            assert_ne!(
                streamed.width_px().to_bits(),
                checkpoint.width_px().to_bits()
            );
            streamed = checkpoint;
            assert_eq!(streamed.width_px().to_bits(), expected.to_bits());
            streamed.reset();
            assert_eq!(streamed.width_px(), 0.0);
        }
    }

    #[test]
    fn raw_soft_hyphens_bound_the_pending_shaping_cluster() {
        let style = TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        };
        let mut width = BuiltinInlineRawLineWidth::new(&style);
        for index in 1..=8192 {
            width.push_text("👩\u{200d}\u{00ad}");
            assert!(width.pending_grapheme.len() <= '\u{00ad}'.len_utf8());
            assert_eq!(width.width_px(), index as f64 * 10.0);
            let snapshot = width.clone();
            assert!(snapshot.pending_grapheme.len() <= '\u{00ad}'.len_utf8());
        }
        width.push_text("👩");
        assert_eq!(width.width_px(), 81930.0);
    }

    #[test]
    fn builtin_html_line_stream_preserves_decoded_unicode_width_and_rollback() {
        let style = TextStyle {
            font_size: 16.0,
            ..TextStyle::default()
        };
        let cases: &[(&str, &[&str])] = &[
            ("AVATAR office", &["A", "VAT", "AR ", "office"]),
            ("A\u{301}مرحبا世界", &["A", "\u{301}", "مرحبا", "世", "界"]),
            ("👩‍🔬", &["👩", "\u{200d}", "🔬"]),
            ("👨‍👩‍👧‍👦", &["👨", "\u{200d}👩‍", "👧", "\u{200d}👦"]),
            ("👍🏽", &["👍", "🏽"]),
            ("🇨🇳", &["🇨", "🇳"]),
            ("1️⃣", &["1", "\u{fe0f}", "\u{20e3}"]),
            ("😀\u{fe0e}", &["😀", "\u{fe0e}"]),
            ("alphabeta\u{200b}", &["alpha", "beta", "\u{200b}"]),
            (
                " a  b\t\u{00a0}\u{2002}",
                &[" a", "  b", "\t", "\u{00a0}", "\u{2002}"],
            ),
        ];

        for char_width_factor in [0.0, 0.6] {
            let backend = DeterministicTextMeasurer {
                char_width_factor,
                ..DeterministicTextMeasurer::default()
            };
            for (text, chunks) in cases {
                assert_eq!(chunks.concat(), *text);
                let mut streamed = BuiltinHtmlLineWidth::deterministic(&style, char_width_factor);
                let mut prefix = String::new();
                for chunk in *chunks {
                    prefix.push_str(chunk);
                    streamed.push_text(chunk);
                    let expected = backend
                        .measure_wrapped(&prefix, &style, None, WrapMode::HtmlLike)
                        .width;
                    assert_eq!(
                        streamed.width_px().to_bits(),
                        expected.to_bits(),
                        "factor={char_width_factor}, text={text:?}, prefix={prefix:?}"
                    );
                }

                let checkpoint = streamed.clone();
                streamed.push_text("A");
                assert!(streamed.width_px() > checkpoint.width_px());
                streamed = checkpoint;
                let expected = backend
                    .measure_wrapped(text, &style, None, WrapMode::HtmlLike)
                    .width;
                assert_eq!(streamed.width_px().to_bits(), expected.to_bits());
                streamed.reset();
                assert_eq!(streamed.width_px(), 0.0);
                streamed.push_text("A");
                assert_eq!(
                    streamed.width_px().to_bits(),
                    backend
                        .measure_wrapped("A", &style, None, WrapMode::HtmlLike)
                        .width
                        .to_bits()
                );
            }
        }

        // HTML has already been decoded before a line reaches this accumulator. A label such
        // as `&lt;br&gt;` must keep its literal spelling rather than introduce another line.
        let mut literal = BuiltinHtmlLineWidth::deterministic(&style, 0.0);
        literal.push_text("a<br>b");
        assert_eq!(
            literal.width_px().to_bits(),
            (estimate_text_width_em("a<br>b") * style.font_size).to_bits()
        );
        assert_ne!(
            literal.width_px().to_bits(),
            DeterministicTextMeasurer::default()
                .measure_wrapped("a<br>b", &style, None, WrapMode::HtmlLike)
                .width
                .to_bits()
        );
    }

    #[test]
    fn builtin_html_line_stream_uses_wrap_canvas_route_and_keeps_opaque_routes_observable() {
        let style = TextStyle::default();
        for (phase, operation) in [
            (
                TextMeasurementPhase::Layout,
                TextMeasurementOperation::CanvasMeasureTextWidth,
            ),
            (
                TextMeasurementPhase::Wrap,
                TextMeasurementOperation::Wrapped,
            ),
            (
                TextMeasurementPhase::Wrap,
                TextMeasurementOperation::ComputedLength,
            ),
            (
                TextMeasurementPhase::Wrap,
                TextMeasurementOperation::WrappedWithRawWidth,
            ),
        ] {
            assert!(
                BuiltinTextMeasurementOperationCarrier {
                    profile: BuiltinTextMeasurementProfile::Deterministic,
                    phase,
                    operation,
                }
                .into_html_line_width(&style)
                .is_none()
            );
        }

        let builtin_session = RenderEnvironment::deterministic()
            .begin_session()
            .expect("begin built-in session");
        for default_phase in TextMeasurementPhase::ALL {
            assert!(
                builtin_session
                    .text_measurer(default_phase)
                    .begin_html_line_width(&style)
                    .is_some(),
                "decoded HTML candidates always belong to the Wrap phase"
            );
        }

        // Even a deterministic backend remains opaque when admitted as an ordinary custom
        // profile; concrete operation authority comes from the route, not its backend type.
        let custom = TextMeasurementProfile::new(
            identity("test.custom-deterministic", "v1", &[]),
            Arc::new(DeterministicTextMeasurer::default()),
        );
        let custom_wrap_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(
                TextMeasurementPolicy::deterministic()
                    .with_profile_for_phase(TextMeasurementPhase::Wrap, custom.clone()),
            )
            .begin_session()
            .expect("begin custom Wrap session");
        let custom_wrap = custom_wrap_session.text_measurer(TextMeasurementPhase::ComputedLength);
        assert!(custom_wrap.begin_html_line_width(&style).is_none());
        assert!(custom_wrap.begin_svg_text_computed_length(&style).is_some());

        let custom_layout_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(
                TextMeasurementPolicy::deterministic()
                    .with_profile_for_phase(TextMeasurementPhase::Layout, custom.clone()),
            )
            .begin_session()
            .expect("begin custom Layout session");
        assert!(
            custom_layout_session
                .text_measurer(TextMeasurementPhase::Layout)
                .begin_html_line_width(&style)
                .is_some(),
            "the ordinary Canvas Layout route does not own decoded HTML candidates"
        );

        let custom_computed_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(
                TextMeasurementPolicy::deterministic()
                    .with_profile_for_phase(TextMeasurementPhase::ComputedLength, custom),
            )
            .begin_session()
            .expect("begin custom ComputedLength session");
        let custom_computed = custom_computed_session.text_measurer(TextMeasurementPhase::Layout);
        assert!(custom_computed.begin_html_line_width(&style).is_some());
        assert!(
            custom_computed
                .begin_svg_text_computed_length(&style)
                .is_none()
        );

        let host_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::host_display(
                identity("test.host", "v1", &[]),
                Arc::new(DecliningHost),
                [TextMeasurementPhase::Wrap],
            ))
            .begin_session()
            .expect("begin host Wrap session");
        assert!(
            host_session
                .text_measurer(TextMeasurementPhase::ComputedLength)
                .begin_html_line_width(&style)
                .is_none(),
            "a built-in fallback cannot bypass host measurement attempts"
        );
        assert!(host_session.text_measurement_report().entries().is_empty());

        let callback = DeterministicTextMeasurer::default()
            .with_width_callback(|text, _style| text.chars().count() as f64);
        assert!(callback.begin_html_line_width(&style).is_none());
        assert!(SpecializedProfile.begin_html_line_width(&style).is_none());
    }

    #[test]
    fn decoded_html_line_probe_keeps_wrap_canvas_host_routing_and_fallback_provenance() {
        struct DecodedLineHost {
            calls: Arc<AtomicUsize>,
            measurement: Option<HostTextMeasurement>,
        }

        impl HostTextMeasurer for DecodedLineHost {
            fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
                self.calls.fetch_add(1, Ordering::Relaxed);
                assert_eq!(
                    request.operation,
                    TextMeasurementOperation::CanvasMeasureTextWidth
                );
                assert_eq!(request.phase, TextMeasurementPhase::Wrap);
                assert_eq!(request.text, "a<br>b");
                assert_eq!(request.max_width, None);
                assert_eq!(request.wrap_mode, WrapMode::HtmlLike);
                Ok(self.measurement)
            }
        }

        let style = TextStyle::default();
        let text = "a<br>b";
        let fallback_width =
            DeterministicTextMeasurer::default().measure_html_line_width_px(text, &style);
        for (measurement, expected, outcome) in [
            (
                Some(HostTextMeasurement::Length(73.25)),
                73.25,
                TextMeasurementRouteOutcome::Host,
            ),
            (
                None,
                fallback_width,
                TextMeasurementRouteOutcome::Fallback(HostFallbackReason::Missing),
            ),
            (
                Some(HostTextMeasurement::Metrics(metrics(100.0))),
                fallback_width,
                TextMeasurementRouteOutcome::Fallback(HostFallbackReason::Invalid),
            ),
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let session = RenderEnvironment::deterministic()
                .with_text_measurement_policy(TextMeasurementPolicy::host_display(
                    identity("test.decoded-line-host", "v1", &[]),
                    Arc::new(DecodedLineHost {
                        calls: Arc::clone(&calls),
                        measurement,
                    }),
                    [TextMeasurementPhase::Wrap],
                ))
                .begin_session()
                .expect("begin decoded line host session");
            let measurer = session.text_measurer(TextMeasurementPhase::ComputedLength);
            assert_eq!(measurer.measure_html_line_width_px(text, &style), expected);
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            let report = session.text_measurement_report();
            assert_eq!(report.entries().len(), 1);
            let provenance = report.entries()[0].provenance();
            assert_eq!(provenance.phase, TextMeasurementPhase::Wrap);
            assert_eq!(
                provenance.operation,
                TextMeasurementOperation::CanvasMeasureTextWidth
            );
            match outcome {
                TextMeasurementRouteOutcome::Host => {
                    assert_eq!(provenance.source, TextMeasurementSource::Host);
                    assert_eq!(provenance.fallback_reason, None);
                }
                TextMeasurementRouteOutcome::Fallback(reason) => {
                    assert_eq!(provenance.source, TextMeasurementSource::Profile);
                    assert_eq!(provenance.fallback_reason, Some(reason));
                }
                TextMeasurementRouteOutcome::Profile => unreachable!("host-only test cases"),
            }
        }

        let custom_session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(
                TextMeasurementPolicy::deterministic().with_profile_for_phase(
                    TextMeasurementPhase::Wrap,
                    TextMeasurementProfile::new(
                        identity("test.specialized-decoded-line", "v1", &[]),
                        Arc::new(SpecializedProfile),
                    ),
                ),
            )
            .begin_session()
            .expect("begin custom decoded line session");
        assert_eq!(
            custom_session
                .text_measurer(TextMeasurementPhase::Layout)
                .measure_html_line_width_px(text, &style),
            13.0,
            "a custom profile retains its complete wrapped-measurement default"
        );
    }

    #[test]
    fn decoded_html_line_cancellation_skips_fallback_and_future_wrap_probes() {
        struct CancellingDecodedLineHost(Arc<AtomicUsize>);

        impl HostTextMeasurer for CancellingDecodedLineHost {
            fn measure(&self, _request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
                panic!("decoded line requests must carry the active session control");
            }

            fn measure_with_control(
                &self,
                request: HostTextMeasurementRequest<'_>,
                control: &OperationControl,
            ) -> HostMeasurementResult {
                assert_eq!(request.phase, TextMeasurementPhase::Wrap);
                assert_eq!(
                    request.operation,
                    TextMeasurementOperation::CanvasMeasureTextWidth
                );
                self.0.fetch_add(1, Ordering::Relaxed);
                control.cancel();
                Ok(None)
            }
        }

        let control = OperationControl::new();
        let host_calls = Arc::new(AtomicUsize::new(0));
        let fallback_calls = Arc::new(AtomicUsize::new(0));
        let fallback = TextMeasurementProfile::new(
            identity("test.cancelled-html-line-fallback", "v1", &[]),
            Arc::new(CountingFallback(Arc::clone(&fallback_calls))),
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::host_display_with_fallback(
                identity("test.cancelling-html-line", "v1", &[]),
                Arc::new(CancellingDecodedLineHost(Arc::clone(&host_calls))),
                [TextMeasurementPhase::Wrap],
                fallback,
            ))
            .begin_session_with_control(control)
            .expect("begin controlled decoded line session");
        let measurer = session
            .controlled_text_measurer(TextMeasurementPhase::SvgBBox, OperationPhase::Postprocess);
        let style = TextStyle::default();
        assert_eq!(measurer.measure_html_line_width_px("a<br>b", &style), 0.0);
        assert_eq!(
            measurer.measure_html_line_width_px("next candidate", &style),
            0.0
        );
        assert_eq!(host_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
        assert!(session.text_measurement_report().entries().is_empty());
        assert!(matches!(
            session.checkpoint(OperationPhase::Postprocess),
            Err(crate::Error::Cancelled(_))
        ));
    }

    #[test]
    fn named_complete_profile_preserves_every_specialized_method_and_identity() {
        let profile_identity = identity(
            "test.specialized",
            "v3",
            &["fixture-map@v2", "host-adjustment@v1"],
        );
        let profile =
            TextMeasurementProfile::new(profile_identity.clone(), Arc::new(SpecializedProfile));
        let environment = RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::uniform(profile));
        let session = environment.begin_session().expect("begin render session");
        let measurer = session.text_measurer(TextMeasurementPhase::SvgBBox);
        let style = TextStyle::default();

        assert_eq!(measurer.measure("x", &style).width, 1.0);
        assert_eq!(
            measurer.measure_svg_text_computed_length_px("x", &style),
            2.0
        );
        assert_eq!(measurer.measure_svg_text_bbox_x("x", &style), (3.0, 4.0));
        assert_eq!(
            measurer.measure_svg_text_bbox_x_with_ascii_overhang("x", &style),
            (5.0, 6.0)
        );
        assert_eq!(measurer.measure_svg_title_bbox_x("x", &style), (7.0, 8.0));
        assert_eq!(
            measurer.measure_svg_simple_text_bbox_width_px("x", &style),
            9.0
        );
        assert_eq!(
            measurer.measure_svg_raw_text_bbox_width_px("x", &style),
            10.0
        );
        assert_eq!(
            measurer.measure_svg_tspan_text_bbox_width_px("x", &style),
            10.5
        );
        assert_eq!(
            measurer.measure_svg_tspan_text_bbox_height_px("x", &style),
            11.5
        );
        assert_eq!(
            measurer.measure_svg_create_text_bbox_y_offset_px("x", &style),
            -1.25
        );
        assert_eq!(
            measurer.measure_svg_create_text_middle_bbox_y_offset_px("x", &style),
            -2.5
        );
        assert_eq!(
            measurer.measure_svg_simple_text_bbox_width_for_wrap_px("x", &style),
            11.0
        );
        assert_eq!(
            measurer
                .measure_mermaid_calculate_text_dimensions("x", &style)
                .width,
            11.25
        );
        assert_eq!(measurer.measure_canvas_text_width_px("x", &style), 11.75);
        assert_eq!(
            measurer.measure_svg_simple_text_bbox_height_px("x", &style),
            12.0
        );
        assert_eq!(
            measurer
                .measure_wrapped("x", &style, Some(10.0), WrapMode::HtmlLike)
                .width,
            13.0
        );
        assert_eq!(
            measurer
                .measure_wrapped_with_raw_width("x", &style, Some(10.0), WrapMode::HtmlLike,)
                .1,
            Some(15.0)
        );
        let route = session.text_measurement_route(TextMeasurementPhase::SvgBBox);
        assert_eq!(route.primary, profile_identity);
        assert_eq!(session.text_measurement_report().entries().len(), 17);
    }

    #[test]
    fn repeated_measurements_are_aggregated_into_one_bounded_summary() {
        let policy = TextMeasurementPolicy::deterministic();
        let recorder = TextMeasurementRecorder::default();

        for _ in 0..10_000 {
            recorder.record(
                TextMeasurementPhase::Layout,
                TextMeasurementOperation::Measure,
                TextMeasurementRouteOutcome::Profile,
            );
        }

        let report = recorder.report(&policy);
        assert_eq!(report.entries().len(), 1);
        assert_eq!(report.entries()[0].count(), 10_000);
        assert_eq!(
            report.entries()[0].provenance().operation,
            TextMeasurementOperation::Measure
        );
        assert_eq!(
            report.entries()[0].provenance().phase,
            TextMeasurementPhase::Layout
        );
    }

    #[derive(Clone)]
    enum HostOutcome {
        Measured(TextMetrics),
        Length(f64),
        Missing,
        Invalid,
        Error,
    }

    struct CountingHost {
        calls: Arc<AtomicUsize>,
        outcome: HostOutcome,
    }

    impl HostTextMeasurer for CountingHost {
        fn measure(&self, _request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            match self.outcome {
                HostOutcome::Measured(metrics) => Ok(Some(HostTextMeasurement::Metrics(metrics))),
                HostOutcome::Length(length) => Ok(Some(HostTextMeasurement::Length(length))),
                HostOutcome::Missing => Ok(None),
                HostOutcome::Invalid => Err(HostTextMeasurementError::invalid_value(
                    "invalid host value",
                )),
                HostOutcome::Error => Err(HostTextMeasurementError::new("host failed")),
            }
        }
    }

    struct CountingFallback(Arc<AtomicUsize>);

    impl TextMeasurer for CountingFallback {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            self.0.fetch_add(1, Ordering::Relaxed);
            metrics(41.0)
        }

        fn measure_svg_text_computed_length_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            self.0.fetch_add(1, Ordering::Relaxed);
            42.0
        }
    }

    fn host_policy(
        outcome: HostOutcome,
        host_calls: &Arc<AtomicUsize>,
        fallback_calls: &Arc<AtomicUsize>,
    ) -> TextMeasurementPolicy {
        let fallback = TextMeasurementProfile::new(
            identity("test.fallback", "v1", &[]),
            Arc::new(CountingFallback(Arc::clone(fallback_calls))),
        );
        TextMeasurementPolicy::host_display_with_fallback(
            identity("test.host", "v2", &["browser@stable"]),
            Arc::new(CountingHost {
                calls: Arc::clone(host_calls),
                outcome,
            }),
            [
                TextMeasurementPhase::Layout,
                TextMeasurementPhase::ComputedLength,
            ],
            fallback,
        )
    }

    struct CancellingMissingHost {
        calls: Arc<AtomicUsize>,
        control: OperationControl,
    }

    struct ActiveOperationHost;

    impl HostTextMeasurer for ActiveOperationHost {
        fn measure(&self, _request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            panic!("session must provide its canonical control");
        }

        fn measure_with_control(
            &self,
            _request: HostTextMeasurementRequest<'_>,
            control: &OperationControl,
        ) -> HostMeasurementResult {
            control.cancel();
            Ok(None)
        }
    }

    #[test]
    fn host_adapter_receives_the_active_session_control() {
        let control = OperationControl::new();
        let policy = TextMeasurementPolicy::host_display(
            identity("test.active-operation", "v1", &[]),
            Arc::new(ActiveOperationHost),
            TextMeasurementPhase::ALL,
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session_with_control(control.clone())
            .expect("begin session");
        let metrics = session
            .controlled_text_measurer(TextMeasurementPhase::Layout, OperationPhase::Layout)
            .measure("label", &TextStyle::default());
        assert_eq!(metrics.width, 0.0);
        assert!(
            control
                .terminal_checkpoint_at(OperationPhase::Layout)
                .is_err()
        );
    }

    impl HostTextMeasurer for CancellingMissingHost {
        fn measure(&self, _request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.control.cancel();
            Ok(None)
        }
    }

    #[test]
    fn host_cancellation_skips_fallback_and_future_primary_calls() {
        let control = OperationControl::new();
        let host_calls = Arc::new(AtomicUsize::new(0));
        let fallback_calls = Arc::new(AtomicUsize::new(0));
        let fallback = TextMeasurementProfile::new(
            identity("test.cancelled-fallback", "v1", &[]),
            Arc::new(CountingFallback(Arc::clone(&fallback_calls))),
        );
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.cancelling-host", "v1", &[]),
            Arc::new(CancellingMissingHost {
                calls: Arc::clone(&host_calls),
                control: control.clone(),
            }),
            [TextMeasurementPhase::Layout],
            fallback,
        );
        let session = RenderEnvironment::deterministic()
            .with_text_measurement_policy(policy)
            .begin_session_with_control(control)
            .expect("begin controlled render session");

        let measurer =
            session.controlled_text_measurer(TextMeasurementPhase::Layout, OperationPhase::Layout);
        let measured = measurer.measure("label", &TextStyle::default());
        let measured_after_cancellation = measurer.measure("second label", &TextStyle::default());

        assert_eq!(measured.width, 0.0);
        assert_eq!(measured.height, 0.0);
        assert_eq!(measured.line_count, 1);
        assert_eq!(measured_after_cancellation.width, 0.0);
        assert_eq!(measured_after_cancellation.height, 0.0);
        assert_eq!(measured_after_cancellation.line_count, 1);
        assert_eq!(host_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
        assert!(matches!(
            session.checkpoint(OperationPhase::Layout),
            Err(crate::Error::Cancelled(_))
        ));
    }

    #[test]
    fn host_success_and_each_fallback_reason_are_recorded_from_actual_calls() {
        let scenarios = [
            (HostOutcome::Measured(metrics(73.0)), None, 73.0),
            (
                HostOutcome::Length(73.0),
                Some(HostFallbackReason::Invalid),
                41.0,
            ),
            (
                HostOutcome::Missing,
                Some(HostFallbackReason::Missing),
                41.0,
            ),
            (
                HostOutcome::Measured(TextMetrics {
                    width: f64::NAN,
                    height: 10.0,
                    line_count: 1,
                }),
                Some(HostFallbackReason::Invalid),
                41.0,
            ),
            (
                HostOutcome::Invalid,
                Some(HostFallbackReason::Invalid),
                41.0,
            ),
            (HostOutcome::Error, Some(HostFallbackReason::Error), 41.0),
        ];

        for (outcome, expected_reason, expected_width) in scenarios {
            let host_calls = Arc::new(AtomicUsize::new(0));
            let fallback_calls = Arc::new(AtomicUsize::new(0));
            let environment = RenderEnvironment::deterministic()
                .with_text_measurement_policy(host_policy(outcome, &host_calls, &fallback_calls));
            let session = environment.begin_session().expect("begin render session");
            let measured = session
                .text_measurer(TextMeasurementPhase::Layout)
                .measure("label", &TextStyle::default());

            assert_eq!(measured.width, expected_width);
            assert_eq!(host_calls.load(Ordering::Relaxed), 1);
            assert_eq!(
                fallback_calls.load(Ordering::Relaxed),
                usize::from(expected_reason.is_some())
            );
            let report = session.text_measurement_report();
            assert_eq!(report.entries().len(), 1);
            assert_eq!(
                report.entries()[0].provenance().fallback_reason,
                expected_reason
            );
        }
    }

    struct StyleCapturingHost {
        observed_font_style: Arc<Mutex<Option<String>>>,
    }

    impl HostTextMeasurer for StyleCapturingHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            if request.operation != TextMeasurementOperation::Wrapped {
                return Ok(None);
            }
            *self
                .observed_font_style
                .lock()
                .expect("font style probe lock") = request.style.font_style.clone();
            Ok(Some(HostTextMeasurement::Metrics(metrics(73.25))))
        }
    }

    #[test]
    fn host_success_receives_italic_style_without_fallback_adjustment() {
        let observed_font_style = Arc::new(Mutex::new(None));
        let fallback_calls = Arc::new(AtomicUsize::new(0));
        let fallback = TextMeasurementProfile::new(
            identity("test.italic-fallback", "v1", &[]),
            Arc::new(CountingFallback(Arc::clone(&fallback_calls))),
        );
        let policy = TextMeasurementPolicy::host_display_with_fallback(
            identity("test.italic-host", "v1", &[]),
            Arc::new(StyleCapturingHost {
                observed_font_style: Arc::clone(&observed_font_style),
            }),
            [TextMeasurementPhase::Wrap],
            fallback,
        );
        let environment = RenderEnvironment::deterministic().with_text_measurement_policy(policy);
        let session = environment.begin_session().expect("begin render session");
        let style = TextStyle {
            font_style: Some("italic".to_string()),
            ..TextStyle::default()
        };

        let measured = session
            .text_measurer(TextMeasurementPhase::Layout)
            .measure_wrapped("italic label", &style, Some(200.0), WrapMode::HtmlLike);

        assert_eq!(measured.width, 73.25);
        assert_eq!(
            observed_font_style
                .lock()
                .expect("font style probe lock")
                .as_deref(),
            Some("italic")
        );
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn operation_specific_length_is_authoritative_and_invalid_lengths_fallback() {
        let host_calls = Arc::new(AtomicUsize::new(0));
        let fallback_calls = Arc::new(AtomicUsize::new(0));
        let environment = RenderEnvironment::deterministic().with_text_measurement_policy(
            host_policy(HostOutcome::Length(73.0), &host_calls, &fallback_calls),
        );
        let session = environment.begin_session().expect("begin render session");

        assert_eq!(
            session
                .text_measurer(TextMeasurementPhase::Layout)
                .measure_svg_text_computed_length_px("label", &TextStyle::default()),
            73.0
        );
        assert_eq!(host_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
        let report = session.text_measurement_report();
        assert_eq!(
            report.entries()[0].provenance().operation,
            TextMeasurementOperation::ComputedLength
        );
        assert_eq!(
            report.entries()[0].provenance().source,
            TextMeasurementSource::Host
        );
        assert_eq!(report.entries()[0].provenance().fallback_reason, None);

        for invalid_length in [-1.0, f64::NAN] {
            let host_calls = Arc::new(AtomicUsize::new(0));
            let fallback_calls = Arc::new(AtomicUsize::new(0));
            let environment =
                RenderEnvironment::deterministic().with_text_measurement_policy(host_policy(
                    HostOutcome::Length(invalid_length),
                    &host_calls,
                    &fallback_calls,
                ));
            let session = environment.begin_session().expect("render session");

            assert_eq!(
                session
                    .text_measurer(TextMeasurementPhase::ComputedLength)
                    .measure_svg_text_computed_length_px("label", &TextStyle::default()),
                42.0
            );
            assert_eq!(host_calls.load(Ordering::Relaxed), 1);
            assert_eq!(fallback_calls.load(Ordering::Relaxed), 1);
            assert_eq!(
                session.text_measurement_report().entries()[0]
                    .provenance()
                    .fallback_reason,
                Some(HostFallbackReason::Invalid)
            );
        }
    }

    struct ExtentHost {
        calls: Arc<AtomicUsize>,
        value: (f64, f64),
    }

    impl HostTextMeasurer for ExtentHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            if request.operation != TextMeasurementOperation::BBoxX {
                return Ok(None);
            }
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(Some(HostTextMeasurement::HorizontalExtents {
                left: self.value.0,
                right: self.value.1,
            }))
        }
    }

    struct ExtentFallback(Arc<AtomicUsize>);

    impl TextMeasurer for ExtentFallback {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            metrics(1.0)
        }

        fn measure_svg_text_bbox_x(&self, _text: &str, _style: &TextStyle) -> (f64, f64) {
            self.0.fetch_add(1, Ordering::Relaxed);
            (3.0, 4.0)
        }
    }

    #[test]
    fn host_bbox_accepts_non_negative_extents_and_rejects_invalid_values() {
        for (host_value, expected, expected_source, expected_reason) in [
            ((1.5, 12.0), (1.5, 12.0), TextMeasurementSource::Host, None),
            (
                (-1.5, 12.0),
                (3.0, 4.0),
                TextMeasurementSource::Profile,
                Some(HostFallbackReason::Invalid),
            ),
            (
                (12.0, -1.5),
                (3.0, 4.0),
                TextMeasurementSource::Profile,
                Some(HostFallbackReason::Invalid),
            ),
            (
                (f64::NAN, 12.0),
                (3.0, 4.0),
                TextMeasurementSource::Profile,
                Some(HostFallbackReason::Invalid),
            ),
        ] {
            let host_calls = Arc::new(AtomicUsize::new(0));
            let fallback_calls = Arc::new(AtomicUsize::new(0));
            let fallback = TextMeasurementProfile::new(
                identity("test.extent-fallback", "v1", &[]),
                Arc::new(ExtentFallback(Arc::clone(&fallback_calls))),
            );
            let policy = TextMeasurementPolicy::host_display_with_fallback(
                identity("test.extent-host", "v1", &[]),
                Arc::new(ExtentHost {
                    calls: Arc::clone(&host_calls),
                    value: host_value,
                }),
                [TextMeasurementPhase::SvgBBox],
                fallback,
            );
            let session = RenderEnvironment::deterministic()
                .with_text_measurement_policy(policy)
                .begin_session()
                .expect("render session");

            assert_eq!(
                session
                    .text_measurer(TextMeasurementPhase::SvgBBox)
                    .measure_svg_text_bbox_x("A", &TextStyle::default()),
                expected
            );
            assert_eq!(host_calls.load(Ordering::Relaxed), 1);
            assert_eq!(
                fallback_calls.load(Ordering::Relaxed),
                usize::from(expected_reason.is_some())
            );
            let report = session.text_measurement_report();
            assert_eq!(report.entries().len(), 1);
            assert_eq!(report.entries()[0].provenance().source, expected_source);
            assert_eq!(
                report.entries()[0].provenance().fallback_reason,
                expected_reason
            );
        }
    }

    #[test]
    fn session_exposes_services_and_derives_a_nonzero_render_seed() {
        let limits = RenderResourcePolicy::trusted_native();
        let environment = RenderEnvironment::deterministic()
            .with_runtime_policy(RuntimePolicy::deterministic().with_fixed_seed(0))
            .with_math_renderer(Arc::new(crate::math::NoopMathRenderer))
            .with_icon_registry(crate::svg::IconRegistryBuilder::new().build().unwrap())
            .with_resource_policy(limits);

        let session = environment.begin_session().expect("begin render session");
        assert_eq!(session.operation_context().seed(), 0);
        assert_eq!(
            session.render_seed(),
            session
                .operation_context()
                .derive_nonzero_u64("render.root", 0)
        );
        assert_eq!(session.resource_policy(), limits);
        assert!(session.math_renderer().is_some());
        assert!(session.icon_registry().is_some());
        assert_eq!(session.report().operation_context().seed(), 0);
    }

    #[cfg(feature = "math")]
    #[test]
    fn without_math_renderer_disables_the_compiled_default() {
        let session = RenderEnvironment::deterministic()
            .without_math_renderer()
            .begin_session()
            .expect("begin render session");

        assert!(session.math_renderer().is_none());
    }

    #[test]
    fn capability_policy_masks_installed_services_and_compiled_backends() {
        let session = RenderEnvironment::deterministic()
            .with_math_renderer(Arc::new(crate::math::NoopMathRenderer))
            .with_capability_policy(RenderCapabilityPolicy::deny_all())
            .begin_session()
            .expect("begin render session");

        assert!(!session.supports_capability(RenderCapability::LayoutCytoscape));
        assert!(!session.supports_capability(RenderCapability::LayoutElk));
        assert!(!session.supports_capability(RenderCapability::Math));
        assert!(session.math_renderer().is_none());

        let session = RenderEnvironment::deterministic()
            .with_math_renderer(Arc::new(crate::math::NoopMathRenderer))
            .with_capability_policy(
                RenderCapabilityPolicy::deny_all().with_allowed(RenderCapability::Math),
            )
            .begin_session()
            .expect("begin render session");
        assert!(session.supports_capability(RenderCapability::Math));
        assert!(session.math_renderer().is_some());
    }
}

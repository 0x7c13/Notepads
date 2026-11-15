use crate::time::CivilDate;
#[cfg(feature = "diagram-gantt")]
use crate::time::CivilDateTime;
#[cfg(any(test, feature = "diagram-gantt"))]
use crate::time::OffsetDateTime;
#[cfg(test)]
use crate::time::UtcOffset;
use std::cell::RefCell;
use std::num::NonZeroU64;

const DETERMINISTIC_UNIX_MILLIS: i64 = 0;
const DETERMINISTIC_OPERATION_SEED: u64 = 0x6D65_726D_616E_0001;

thread_local! {
    static OPERATION_CONTEXT: RefCell<Option<OperationContext>> = const { RefCell::new(None) };
}

/// Optional system adapters that a runtime policy can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapability {
    SystemClock,
    SystemTimeZone,
    SystemRandom,
    SystemTiming,
}

impl RuntimeCapability {
    pub const ALL: [Self; 4] = [
        Self::SystemClock,
        Self::SystemTimeZone,
        Self::SystemRandom,
        Self::SystemTiming,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::SystemClock => "system-clock",
            Self::SystemTimeZone => "system-timezone",
            Self::SystemRandom => "system-random",
            Self::SystemTiming => "system-timing",
        }
    }
}

const COMPILED_SYSTEM_ADAPTER_IDS: &[&str] = &[
    #[cfg(feature = "system-clock")]
    RuntimeCapability::SystemClock.id(),
    #[cfg(feature = "system-timezone")]
    RuntimeCapability::SystemTimeZone.id(),
    #[cfg(feature = "system-random")]
    RuntimeCapability::SystemRandom.id(),
    #[cfg(feature = "system-timing")]
    RuntimeCapability::SystemTiming.id(),
];

/// Returns the system adapters actually compiled by the owning core crate after Cargo feature
/// unification.
pub const fn compiled_system_adapter_ids() -> &'static [&'static str] {
    COMPILED_SYSTEM_ADAPTER_IDS
}

/// Failure to materialize an explicitly requested runtime policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimePolicyError {
    #[error("runtime capability `{}` is not compiled into this artifact", .0.id())]
    MissingCapability(RuntimeCapability),
    #[error("local UTC offset must be between -1439 and 1439 minutes, got {0}")]
    InvalidFixedOffset(i32),
    #[error("system clock instant is outside the supported millisecond range")]
    SystemClockOutOfRange,
    #[error("system time-zone adapter failed: {0}")]
    SystemTimeZone(String),
    #[error("runtime instant {0} is outside the supported calendar range")]
    InstantOutOfRange(i64),
    #[error("fixed_today local datetime {0}T00:00:00 cannot be resolved in the selected time zone")]
    FixedLocalMidnightOutOfRange(CivilDate),
    #[error("system random adapter failed: {0}")]
    SystemRandom(String),
}

impl RuntimePolicyError {
    pub const fn missing_capability(&self) -> Option<RuntimeCapability> {
        match self {
            Self::MissingCapability(capability) => Some(*capability),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockPolicy {
    Fixed(i64),
    System,
    Captured(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedPolicy {
    Fixed(u64),
    System,
    Captured(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingPolicy {
    Disabled,
    System,
}

#[cfg(feature = "system-timing")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SystemTimingAuthority;

#[cfg(not(feature = "system-timing"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemTimingAuthority {}

/// Runtime choices used to create one immutable operation context.
///
/// The default policy is deterministic. System state is consulted only after callers explicitly
/// select a system adapter, either individually or through [`RuntimePolicy::try_native`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePolicy {
    clock: ClockPolicy,
    local_time_zone: crate::time::LocalTimeZone,
    fixed_today_local: Option<CivilDate>,
    seed: SeedPolicy,
    timing: TimingPolicy,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self::deterministic()
    }
}

impl RuntimePolicy {
    /// System adapters selected by [`Self::try_native`].
    ///
    /// This list is a policy contract, not a report of what Cargo compiled. Consumers that
    /// advertise a `native` policy should intersect it with [`compiled_system_adapter_ids`].
    pub const NATIVE_SYSTEM_ADAPTER_IDS: &'static [&'static str] = &[
        RuntimeCapability::SystemClock.id(),
        RuntimeCapability::SystemTimeZone.id(),
        RuntimeCapability::SystemRandom.id(),
    ];

    /// Returns the deterministic, target-independent core policy.
    pub fn deterministic() -> Self {
        Self {
            clock: ClockPolicy::Fixed(DETERMINISTIC_UNIX_MILLIS),
            local_time_zone: crate::time::LocalTimeZone::utc(),
            fixed_today_local: None,
            seed: SeedPolicy::Fixed(DETERMINISTIC_OPERATION_SEED),
            timing: TimingPolicy::Disabled,
        }
    }

    /// Selects the native clock, complete local-time rules, and random source.
    ///
    /// Timing instrumentation remains opt-in through [`RuntimePolicy::try_with_system_timing`]
    /// because enabling it changes observable diagnostics and adds work to every operation.
    pub fn try_native() -> Result<Self, RuntimePolicyError> {
        Self::deterministic()
            .try_with_system_clock()?
            .try_with_system_time_zone()?
            .try_with_system_random()
    }

    /// Replays an already captured operation without consulting system state again.
    pub fn from_operation_context(context: OperationContext) -> Self {
        let timing = if context.timing.is_some() {
            TimingPolicy::System
        } else {
            TimingPolicy::Disabled
        };
        Self {
            clock: ClockPolicy::Captured(context.unix_millis),
            local_time_zone: context.local_time_zone,
            fixed_today_local: context.today_is_fixed.then_some(context.today_local),
            seed: SeedPolicy::Captured(context.seed),
            timing,
        }
    }

    pub fn with_fixed_unix_millis(mut self, unix_millis: i64) -> Self {
        self.clock = ClockPolicy::Fixed(unix_millis);
        self
    }

    pub fn try_with_system_clock(mut self) -> Result<Self, RuntimePolicyError> {
        require_system_adapter(
            RuntimeCapability::SystemClock,
            cfg!(feature = "system-clock"),
        )?;
        self.clock = ClockPolicy::System;
        Ok(self)
    }

    pub fn with_fixed_today(mut self, today: Option<CivilDate>) -> Self {
        self.fixed_today_local = today;
        self
    }

    /// Freezes both the local calendar day and the operation clock at that day's local midnight.
    ///
    /// Use this when a host accepts a date-only configuration value such as `fixed_today`.
    /// Resolving it through the selected time zone preserves target-date DST behavior and returns
    /// a typed error when the date cannot be represented instead of overflowing at an offset
    /// boundary.
    pub fn try_with_fixed_today_at_local_midnight(
        mut self,
        today: CivilDate,
    ) -> Result<Self, RuntimePolicyError> {
        let local = self
            .local_time_zone
            .resolve_local(today.at_midnight())
            .ok_or(RuntimePolicyError::FixedLocalMidnightOutOfRange(today))?;
        self.clock = ClockPolicy::Fixed(local.timestamp_millis());
        self.fixed_today_local = Some(today);
        Ok(self)
    }

    /// Selects a fixed UTC offset without overloading a sentinel value to mean "system".
    pub fn try_with_fixed_local_offset_minutes(
        mut self,
        offset_minutes: i32,
    ) -> Result<Self, RuntimePolicyError> {
        self.local_time_zone = crate::time::LocalTimeZone::fixed(offset_minutes)?;
        Ok(self)
    }

    pub fn with_local_time_zone(mut self, time_zone: crate::time::LocalTimeZone) -> Self {
        self.local_time_zone = time_zone;
        self
    }

    pub fn try_with_system_time_zone(mut self) -> Result<Self, RuntimePolicyError> {
        self.local_time_zone = crate::time::LocalTimeZone::try_system()?;
        Ok(self)
    }

    pub fn with_fixed_seed(mut self, seed: u64) -> Self {
        self.seed = SeedPolicy::Fixed(seed);
        self
    }

    pub fn try_with_system_random(mut self) -> Result<Self, RuntimePolicyError> {
        require_system_adapter(
            RuntimeCapability::SystemRandom,
            cfg!(feature = "system-random"),
        )?;
        self.seed = SeedPolicy::System;
        Ok(self)
    }

    /// Enables the compiled system timing adapter.
    pub fn try_with_system_timing(mut self) -> Result<Self, RuntimePolicyError> {
        require_system_adapter(
            RuntimeCapability::SystemTiming,
            cfg!(feature = "system-timing"),
        )?;
        self.timing = TimingPolicy::System;
        Ok(self)
    }

    pub fn fixed_local_offset_minutes(&self) -> Option<i32> {
        self.local_time_zone.fixed_offset_minutes()
    }

    pub fn fixed_today(&self) -> Option<CivilDate> {
        self.fixed_today_local
    }

    pub fn local_time_zone(&self) -> &crate::time::LocalTimeZone {
        &self.local_time_zone
    }

    /// Freezes all selected adapters for one parse or render operation.
    pub fn begin_operation(&self) -> Result<OperationContext, RuntimePolicyError> {
        let (unix_millis, clock_source) = match self.clock {
            ClockPolicy::Fixed(unix_millis) => (unix_millis, RuntimeValueSource::Fixed),
            ClockPolicy::System => (system_unix_millis()?, RuntimeValueSource::System),
            ClockPolicy::Captured(unix_millis) => (unix_millis, RuntimeValueSource::Captured),
        };
        let local_time_zone = self.local_time_zone.clone();
        let today_local = match self.fixed_today_local {
            Some(today) => today,
            None => local_date_at(unix_millis, &local_time_zone)?,
        };
        let (seed, random_source) = match self.seed {
            SeedPolicy::Fixed(seed) => (seed, RuntimeValueSource::Fixed),
            SeedPolicy::System => (system_seed()?, RuntimeValueSource::System),
            SeedPolicy::Captured(seed) => (seed, RuntimeValueSource::Captured),
        };
        let timing = match self.timing {
            TimingPolicy::Disabled => None,
            TimingPolicy::System => Some(system_timing_authority()?),
        };

        Ok(OperationContext {
            unix_millis,
            clock_source,
            today_local,
            today_is_fixed: self.fixed_today_local.is_some(),
            local_time_zone,
            seed,
            random_source,
            timing,
        })
    }
}

/// Immutable environment captured at the start of one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationContext {
    unix_millis: i64,
    clock_source: RuntimeValueSource,
    today_local: CivilDate,
    today_is_fixed: bool,
    local_time_zone: crate::time::LocalTimeZone,
    seed: u64,
    random_source: RuntimeValueSource,
    timing: Option<SystemTimingAuthority>,
}

/// An operation-derived authority to read the monotonic system clock.
///
/// The token has no public constructor. Code can only obtain one from an operation whose policy
/// explicitly enabled system timing.
#[derive(Debug, Clone, Copy)]
pub struct OperationTiming {
    authority: SystemTimingAuthority,
}

/// A monotonic timer started through [`OperationTiming`].
#[derive(Debug)]
pub struct OperationTimer {
    #[cfg(feature = "system-timing")]
    started_at: web_time::Instant,
    #[cfg(not(feature = "system-timing"))]
    unavailable: std::convert::Infallible,
}

/// Failure to request timing authority from an operation that did not enable it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("operation timing was not enabled for this operation")]
pub struct OperationTimingUnavailable;

/// Provenance of a runtime value captured for one operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeValueSource {
    Fixed,
    System,
    Captured,
}

impl RuntimeValueSource {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::System => "system",
            Self::Captured => "captured",
        }
    }
}

impl OperationContext {
    pub const fn unix_millis(&self) -> i64 {
        self.unix_millis
    }

    pub const fn clock_source(&self) -> RuntimeValueSource {
        self.clock_source
    }

    pub const fn today_local(&self) -> CivilDate {
        self.today_local
    }

    pub const fn today_is_fixed(&self) -> bool {
        self.today_is_fixed
    }

    pub fn local_time_zone(&self) -> &crate::time::LocalTimeZone {
        &self.local_time_zone
    }

    pub const fn seed(&self) -> u64 {
        self.seed
    }

    pub const fn random_source(&self) -> RuntimeValueSource {
        self.random_source
    }

    pub fn timing(&self) -> Option<OperationTiming> {
        self.timing.map(|authority| OperationTiming { authority })
    }

    pub fn require_timing(&self) -> Result<OperationTiming, OperationTimingUnavailable> {
        self.timing().ok_or(OperationTimingUnavailable)
    }

    /// Derives a stable value from this operation's random key without sharing mutable PRNG state.
    ///
    /// Callers must use a durable, owner-qualified domain such as `block.generated-id`. The
    /// ordinal is local to that domain, so adding random consumers in another family cannot shift
    /// existing output.
    pub fn derive_u64(&self, domain: &str, ordinal: u64) -> u64 {
        derive_random_u64(self.seed, domain, ordinal)
    }

    pub fn derive_nonzero_u64(&self, domain: &str, ordinal: u64) -> NonZeroU64 {
        NonZeroU64::new(self.derive_u64(domain, ordinal)).unwrap_or(NonZeroU64::MIN)
    }

    pub fn derive_hex(&self, domain: &str, ordinal: u64, len: usize) -> String {
        derive_random_hex(self.seed, domain, ordinal, len)
    }
}

impl OperationTiming {
    /// Starts a timer using the system adapter authorized for this operation.
    pub fn start(self) -> OperationTimer {
        #[cfg(feature = "system-timing")]
        {
            let _ = self.authority;
            OperationTimer {
                started_at: web_time::Instant::now(),
            }
        }

        #[cfg(not(feature = "system-timing"))]
        {
            match self.authority {}
        }
    }
}

impl OperationTimer {
    /// Returns the time elapsed since this timer was started.
    pub fn elapsed(self) -> std::time::Duration {
        #[cfg(feature = "system-timing")]
        {
            self.started_at.elapsed()
        }

        #[cfg(not(feature = "system-timing"))]
        {
            match self.unavailable {}
        }
    }
}

fn require_system_adapter(
    capability: RuntimeCapability,
    available: bool,
) -> Result<(), RuntimePolicyError> {
    if available {
        Ok(())
    } else {
        Err(RuntimePolicyError::MissingCapability(capability))
    }
}

#[cfg(feature = "system-timing")]
fn system_timing_authority() -> Result<SystemTimingAuthority, RuntimePolicyError> {
    Ok(SystemTimingAuthority)
}

#[cfg(not(feature = "system-timing"))]
fn system_timing_authority() -> Result<SystemTimingAuthority, RuntimePolicyError> {
    Err(RuntimePolicyError::MissingCapability(
        RuntimeCapability::SystemTiming,
    ))
}

fn local_date_at(
    unix_millis: i64,
    time_zone: &crate::time::LocalTimeZone,
) -> Result<CivilDate, RuntimePolicyError> {
    time_zone
        .at_instant(unix_millis)
        .map(|local| local.local_datetime().date())
        .ok_or(RuntimePolicyError::InstantOutOfRange(unix_millis))
}

#[cfg(feature = "system-clock")]
fn system_unix_millis() -> Result<i64, RuntimePolicyError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_millis())
            .map_err(|_| RuntimePolicyError::SystemClockOutOfRange)?,
        Err(error) => -i128::try_from(error.duration().as_millis())
            .map_err(|_| RuntimePolicyError::SystemClockOutOfRange)?,
    };
    millis
        .try_into()
        .map_err(|_| RuntimePolicyError::SystemClockOutOfRange)
}

#[cfg(not(feature = "system-clock"))]
fn system_unix_millis() -> Result<i64, RuntimePolicyError> {
    Err(RuntimePolicyError::MissingCapability(
        RuntimeCapability::SystemClock,
    ))
}

#[cfg(feature = "system-random")]
fn system_seed() -> Result<u64, RuntimePolicyError> {
    let mut bytes = [0_u8; size_of::<u64>()];
    getrandom::fill(&mut bytes)
        .map_err(|error| RuntimePolicyError::SystemRandom(error.to_string()))?;
    Ok(u64::from_ne_bytes(bytes))
}

#[cfg(not(feature = "system-random"))]
fn system_seed() -> Result<u64, RuntimePolicyError> {
    Err(RuntimePolicyError::MissingCapability(
        RuntimeCapability::SystemRandom,
    ))
}

pub(crate) fn with_operation_context<R>(context: &OperationContext, f: impl FnOnce() -> R) -> R {
    OPERATION_CONTEXT.with(|cell| {
        let previous = cell.replace(Some(context.clone()));
        struct Restore<'a> {
            cell: &'a RefCell<Option<OperationContext>>,
            previous: Option<OperationContext>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.cell.replace(self.previous.take());
            }
        }
        let _restore = Restore { cell, previous };
        f()
    })
}

#[cfg(any(test, feature = "diagram-gantt"))]
pub(crate) fn today_local() -> CivilDate {
    active_operation_context().today_local
}

#[cfg(feature = "diagram-gantt")]
pub(crate) fn resolve_local_datetime(local: CivilDateTime) -> Option<OffsetDateTime> {
    active_operation_context()
        .local_time_zone
        .resolve_local(local)
}

#[cfg(feature = "diagram-gantt")]
pub(crate) fn datetime_to_local(datetime: OffsetDateTime) -> OffsetDateTime {
    active_operation_context()
        .local_time_zone
        .at_instant(datetime.timestamp_millis())
        .unwrap_or(datetime)
}

#[cfg(feature = "diagram-gantt")]
pub(crate) fn datetime_to_local_civil(datetime: OffsetDateTime) -> CivilDateTime {
    datetime_to_local(datetime).local_datetime()
}

#[cfg(any(
    test,
    feature = "diagram-block",
    feature = "diagram-git-graph",
    feature = "diagram-mindmap"
))]
pub(crate) fn generated_id_hex(domain: &str, counter: u64, len: usize) -> String {
    let context = active_operation_context();
    context.derive_hex(domain, counter, len)
}

fn derive_random_hex(seed: u64, domain: &str, ordinal: u64, len: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(len);
    let mut state = derive_random_u64(seed, domain, ordinal);
    while out.len() < len {
        state = splitmix64(state);
        for shift in (0..16).rev() {
            if out.len() == len {
                break;
            }
            let idx = ((state >> (shift * 4)) & 0xF) as usize;
            out.push(HEX[idx] as char);
        }
    }
    out
}

fn derive_random_u64(seed: u64, domain: &str, ordinal: u64) -> u64 {
    const OPERATION_RANDOM_DOMAIN: u64 = 0x6D65_726D_616E_2D72;
    let domain_hash = domain
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    splitmix64(
        seed ^ OPERATION_RANDOM_DOMAIN
            ^ domain_hash.rotate_left(17)
            ^ ordinal.wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
}

fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(any(
    test,
    feature = "diagram-gantt",
    feature = "diagram-block",
    feature = "diagram-git-graph",
    feature = "diagram-mindmap"
))]
fn active_operation_context() -> OperationContext {
    OPERATION_CONTEXT
        .with(|cell| cell.borrow().clone())
        .unwrap_or_else(|| {
            RuntimePolicy::deterministic()
                .begin_operation()
                .expect("the deterministic runtime policy is infallible")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> CivilDate {
        CivilDate::new(year, month, day).expect("valid test date")
    }

    #[test]
    fn operation_context_restores_after_panic() {
        let outer = RuntimePolicy::deterministic()
            .with_fixed_today(Some(date(2026, 7, 18)))
            .begin_operation()
            .unwrap();
        let inner = RuntimePolicy::deterministic()
            .with_fixed_today(Some(date(2030, 1, 2)))
            .begin_operation()
            .unwrap();

        with_operation_context(&outer, || {
            let panic = std::panic::catch_unwind(|| {
                with_operation_context(&inner, || panic!("test panic"));
            });
            assert!(panic.is_err());
            assert_eq!(today_local(), outer.today_local());
        });
    }

    #[test]
    fn deterministic_policy_uses_epoch_utc_and_fixed_seed() {
        let context = RuntimePolicy::deterministic().begin_operation().unwrap();

        assert_eq!(context.unix_millis(), 0);
        assert_eq!(context.today_local(), date(1970, 1, 1));
        assert_eq!(context.local_time_zone().fixed_offset_minutes(), Some(0));
        assert_eq!(context.seed(), DETERMINISTIC_OPERATION_SEED);
        assert_eq!(context.clock_source(), RuntimeValueSource::Fixed);
        assert_eq!(context.random_source(), RuntimeValueSource::Fixed);
        assert!(context.timing().is_none());
    }

    #[test]
    fn fixed_today_local_midnight_rejects_unrepresentable_offset_boundary() {
        let earliest_day = OffsetDateTime::from_unix_millis(i64::MIN, UtcOffset::UTC)
            .utc_datetime()
            .date();
        let error = RuntimePolicy::deterministic()
            .try_with_fixed_local_offset_minutes(1439)
            .expect("valid fixed offset")
            .try_with_fixed_today_at_local_midnight(earliest_day)
            .expect_err("minimum date at eastern boundary must be rejected");

        assert_eq!(
            error,
            RuntimePolicyError::FixedLocalMidnightOutOfRange(earliest_day)
        );
    }

    #[test]
    fn replayed_context_is_attested_as_captured() {
        let original = RuntimePolicy::deterministic().begin_operation().unwrap();
        let replayed = RuntimePolicy::from_operation_context(original)
            .begin_operation()
            .unwrap();

        assert_eq!(replayed.clock_source(), RuntimeValueSource::Captured);
        assert_eq!(replayed.random_source(), RuntimeValueSource::Captured);
        assert!(!replayed.today_is_fixed());
    }

    #[test]
    fn replayed_context_preserves_computed_and_fixed_today_semantics() {
        let computed = RuntimePolicy::deterministic().begin_operation().unwrap();
        let recomputed = RuntimePolicy::from_operation_context(computed)
            .try_with_fixed_local_offset_minutes(-60)
            .unwrap()
            .begin_operation()
            .unwrap();
        assert_eq!(recomputed.today_local(), date(1969, 12, 31));
        assert!(!recomputed.today_is_fixed());

        let fixed_today = date(2026, 7, 22);
        let fixed = RuntimePolicy::deterministic()
            .with_fixed_today(Some(fixed_today))
            .begin_operation()
            .unwrap();
        let replayed_fixed = RuntimePolicy::from_operation_context(fixed)
            .try_with_fixed_local_offset_minutes(-60)
            .unwrap()
            .begin_operation()
            .unwrap();
        assert_eq!(replayed_fixed.today_local(), fixed_today);
        assert!(replayed_fixed.today_is_fixed());
    }

    #[test]
    fn operation_seed_is_domain_separated_and_context_owned() {
        let first = RuntimePolicy::deterministic()
            .with_fixed_seed(1)
            .begin_operation()
            .unwrap();
        let second = RuntimePolicy::deterministic()
            .with_fixed_seed(2)
            .begin_operation()
            .unwrap();

        let first_id = with_operation_context(&first, || generated_id_hex("test.first", 7, 12));
        let repeated = with_operation_context(&first, || generated_id_hex("test.first", 7, 12));
        let second_id = with_operation_context(&second, || generated_id_hex("test.first", 7, 12));
        let other_domain =
            with_operation_context(&first, || generated_id_hex("test.second", 7, 12));

        assert_eq!(first_id, repeated);
        assert_ne!(first_id, second_id);
        assert_ne!(first_id, other_domain);
    }

    #[test]
    fn native_policy_reports_the_first_missing_required_adapter() {
        let expected = RuntimePolicy::NATIVE_SYSTEM_ADAPTER_IDS
            .iter()
            .map(|id| {
                RuntimeCapability::ALL
                    .into_iter()
                    .find(|capability| capability.id() == *id)
                    .expect("native policy adapter IDs must use the runtime capability vocabulary")
            })
            .map(|capability| {
                (
                    capability,
                    compiled_system_adapter_ids().contains(&capability.id()),
                )
            })
            .into_iter()
            .find_map(|(capability, available)| (!available).then_some(capability));

        match expected {
            Some(capability) => assert_eq!(
                RuntimePolicy::try_native().unwrap_err(),
                RuntimePolicyError::MissingCapability(capability)
            ),
            None => assert!(RuntimePolicy::try_native().is_ok()),
        }
    }

    #[test]
    fn native_policy_adapter_contract_excludes_timing() {
        assert_eq!(
            RuntimePolicy::NATIVE_SYSTEM_ADAPTER_IDS,
            ["system-clock", "system-timezone", "system-random"]
        );
        assert!(
            !RuntimePolicy::NATIVE_SYSTEM_ADAPTER_IDS
                .contains(&RuntimeCapability::SystemTiming.id())
        );
    }

    #[cfg(all(
        feature = "system-clock",
        feature = "system-timezone",
        feature = "system-random"
    ))]
    #[test]
    fn native_policy_does_not_enable_timing_instrumentation() {
        let context = RuntimePolicy::try_native()
            .unwrap()
            .begin_operation()
            .unwrap();

        assert!(context.timing().is_none());
    }

    #[test]
    fn compiled_system_adapter_ids_follow_the_canonical_order() {
        let expected = RuntimeCapability::ALL
            .into_iter()
            .filter(|capability| match capability {
                RuntimeCapability::SystemClock => cfg!(feature = "system-clock"),
                RuntimeCapability::SystemTimeZone => cfg!(feature = "system-timezone"),
                RuntimeCapability::SystemRandom => cfg!(feature = "system-random"),
                RuntimeCapability::SystemTiming => cfg!(feature = "system-timing"),
            })
            .map(RuntimeCapability::id)
            .collect::<Vec<_>>();

        assert_eq!(compiled_system_adapter_ids(), expected);
    }

    #[test]
    fn fixed_offset_changes_the_local_date_without_changing_the_instant() {
        let west = RuntimePolicy::deterministic()
            .try_with_fixed_local_offset_minutes(-60)
            .unwrap()
            .begin_operation()
            .unwrap();
        let utc = RuntimePolicy::deterministic()
            .try_with_fixed_local_offset_minutes(0)
            .unwrap()
            .begin_operation()
            .unwrap();

        assert_eq!(west.unix_millis(), utc.unix_millis());
        assert_eq!(west.today_local(), date(1969, 12, 31));
        assert_eq!(utc.today_local(), date(1970, 1, 1));
    }

    #[cfg(feature = "system-timing")]
    #[test]
    fn explicitly_enabled_system_timing_is_issued_by_the_operation_context() {
        let context = RuntimePolicy::deterministic()
            .try_with_system_timing()
            .unwrap()
            .begin_operation()
            .unwrap();

        let timing = context.require_timing().unwrap();
        let _elapsed = timing.start().elapsed();
    }

    #[cfg(not(feature = "system-timing"))]
    #[test]
    fn deterministic_context_cannot_forge_timing_without_system_timing() {
        let context = RuntimePolicy::deterministic().begin_operation().unwrap();

        assert_eq!(
            context.require_timing().unwrap_err(),
            OperationTimingUnavailable
        );
    }
}

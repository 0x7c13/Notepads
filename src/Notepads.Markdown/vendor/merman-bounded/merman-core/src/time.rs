#[cfg(feature = "system-timezone")]
use sha2::{Digest, Sha256};
#[cfg(feature = "system-timezone")]
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

mod civil;

pub use civil::{
    CivilDate, CivilDateTime, IsoWeek, OffsetDateTime, ParseCivilDateError, UtcOffset, Weekday,
    days_in_month, is_leap_year,
};

/// Stable evidence for the local-time rules used by an engine or render operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTimeZoneProvenance {
    source: LocalTimeZoneSource,
    identifier: String,
    rules_sha256: Option<String>,
}

impl LocalTimeZoneProvenance {
    pub const fn source(&self) -> LocalTimeZoneSource {
        self.source
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// SHA-256 of the complete offset-transition behavior supported by the resolver.
    pub fn rules_sha256(&self) -> Option<&str> {
        self.rules_sha256.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTimeZoneSource {
    FixedOffset,
    System,
}

/// An immutable local-time resolver.
///
/// Materialized system resolvers own the complete time-zone rule set rather than the offset sampled
/// at "now": another target date may be on the other side of a daylight-saving transition. The
/// captured rules and their lazily computed provenance are shared by every clone of a resolver.
///
/// Wide civil dates remain project-owned. Jiff is used only as the system time-zone rule engine;
/// dates outside Jiff's civil range are projected onto a Gregorian-equivalent year before rules
/// are queried and are then mapped back without narrowing the represented instant.
#[derive(Debug, Clone)]
pub struct LocalTimeZone {
    data: Arc<LocalTimeZoneData>,
}

#[derive(Debug)]
struct LocalTimeZoneData {
    inner: LocalTimeZoneInner,
    provenance: OnceLock<LocalTimeZoneProvenance>,
}

#[derive(Debug)]
enum LocalTimeZoneInner {
    Fixed(UtcOffset),
    #[cfg(feature = "system-timezone")]
    System(jiff::tz::TimeZone),
}

impl LocalTimeZone {
    pub fn fixed(offset_minutes: i32) -> Result<Self, crate::runtime::RuntimePolicyError> {
        let offset = UtcOffset::from_minutes(offset_minutes).ok_or(
            crate::runtime::RuntimePolicyError::InvalidFixedOffset(offset_minutes),
        )?;
        Ok(Self {
            data: Arc::new(LocalTimeZoneData {
                inner: LocalTimeZoneInner::Fixed(offset),
                provenance: OnceLock::from(LocalTimeZoneProvenance {
                    source: LocalTimeZoneSource::FixedOffset,
                    identifier: offset.to_string(),
                    rules_sha256: None,
                }),
            }),
        })
    }

    pub fn utc() -> Self {
        Self::fixed(0).expect("UTC is a valid fixed offset")
    }

    /// Captures the system time-zone rules or reports that the adapter is unavailable.
    pub fn try_system() -> Result<Self, crate::runtime::RuntimePolicyError> {
        #[cfg(feature = "system-timezone")]
        {
            thread_local! {
                static CACHE: RefCell<Option<LocalTimeZone>> = const { RefCell::new(None) };
            }

            let time_zone = jiff::tz::TimeZone::try_system().map_err(|error| {
                crate::runtime::RuntimePolicyError::SystemTimeZone(error.to_string())
            })?;
            CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(cached) = cache.as_ref()
                    && cached.materialized_system_time_zone() == Some(&time_zone)
                {
                    return Ok(cached.clone());
                }
                let captured = Self {
                    data: Arc::new(LocalTimeZoneData {
                        inner: LocalTimeZoneInner::System(time_zone),
                        provenance: OnceLock::new(),
                    }),
                };
                *cache = Some(captured.clone());
                Ok(captured)
            })
        }
        #[cfg(not(feature = "system-timezone"))]
        {
            Err(crate::runtime::RuntimePolicyError::MissingCapability(
                crate::runtime::RuntimeCapability::SystemTimeZone,
            ))
        }
    }

    pub fn provenance(&self) -> &LocalTimeZoneProvenance {
        self.data.provenance.get_or_init(|| match &self.data.inner {
            LocalTimeZoneInner::Fixed(offset) => LocalTimeZoneProvenance {
                source: LocalTimeZoneSource::FixedOffset,
                identifier: offset.to_string(),
                rules_sha256: None,
            },
            #[cfg(feature = "system-timezone")]
            LocalTimeZoneInner::System(time_zone) => system_provenance(time_zone),
        })
    }

    pub fn is_system(&self) -> bool {
        #[cfg(feature = "system-timezone")]
        {
            matches!(self.data.inner, LocalTimeZoneInner::System(_))
        }
        #[cfg(not(feature = "system-timezone"))]
        {
            false
        }
    }

    pub fn fixed_offset_minutes(&self) -> Option<i32> {
        match &self.data.inner {
            LocalTimeZoneInner::Fixed(offset) => Some(offset.minutes()),
            #[cfg(feature = "system-timezone")]
            LocalTimeZoneInner::System(_) => None,
        }
    }

    /// Resolves a local civil date-time using this resolver's compatible DST policy.
    ///
    /// Gaps move forward by the gap duration and folds select the earlier instant, matching
    /// Mermaid's JavaScript date behavior and Jiff's compatible disambiguation.
    pub fn resolve_local(&self, local: CivilDateTime) -> Option<OffsetDateTime> {
        match &self.data.inner {
            LocalTimeZoneInner::Fixed(offset) => OffsetDateTime::from_local(local, *offset),
            #[cfg(feature = "system-timezone")]
            LocalTimeZoneInner::System(time_zone) => resolve_local_system(time_zone, local),
        }
    }

    /// Applies this resolver's offset rules to an absolute millisecond instant.
    pub fn at_instant(&self, unix_millis: i64) -> Option<OffsetDateTime> {
        let instant = OffsetDateTime::from_unix_millis(unix_millis, UtcOffset::UTC);
        match &self.data.inner {
            LocalTimeZoneInner::Fixed(offset) => Some(instant.to_offset(*offset)),
            #[cfg(feature = "system-timezone")]
            LocalTimeZoneInner::System(time_zone) => {
                let timestamp = utc_timestamp_candidates(instant.utc_datetime()).next()?;
                let offset = UtcOffset::from_seconds(time_zone.to_offset(timestamp).seconds())?;
                Some(instant.to_offset(offset))
            }
        }
    }

    #[cfg(feature = "system-timezone")]
    fn materialized_system_time_zone(&self) -> Option<&jiff::tz::TimeZone> {
        match &self.data.inner {
            LocalTimeZoneInner::Fixed(_) => None,
            LocalTimeZoneInner::System(time_zone) => Some(time_zone),
        }
    }
}

impl PartialEq for LocalTimeZone {
    fn eq(&self, other: &Self) -> bool {
        match (&self.data.inner, &other.data.inner) {
            (LocalTimeZoneInner::Fixed(left), LocalTimeZoneInner::Fixed(right)) => left == right,
            #[cfg(feature = "system-timezone")]
            (LocalTimeZoneInner::System(left), LocalTimeZoneInner::System(right)) => left == right,
            #[cfg(feature = "system-timezone")]
            _ => false,
        }
    }
}

impl Eq for LocalTimeZone {}

#[cfg(feature = "system-timezone")]
const JIFF_MIN_YEAR: i32 = -9999;

#[cfg(feature = "system-timezone")]
const JIFF_MAX_YEAR: i32 = 9999;

#[cfg(feature = "system-timezone")]
const GREGORIAN_CYCLE_YEARS: i32 = 400;

#[cfg(feature = "system-timezone")]
fn jiff_year_candidates(year: i32) -> [Option<i32>; 2] {
    if (JIFF_MIN_YEAR..=JIFF_MAX_YEAR).contains(&year) {
        let fallback = if year > JIFF_MAX_YEAR - GREGORIAN_CYCLE_YEARS {
            Some(year - GREGORIAN_CYCLE_YEARS)
        } else if year < JIFF_MIN_YEAR + GREGORIAN_CYCLE_YEARS {
            Some(year + GREGORIAN_CYCLE_YEARS)
        } else {
            None
        };
        return [Some(year), fallback];
    }

    let projected = if year > JIFF_MAX_YEAR {
        // TZif's POSIX tail repeats on the proleptic Gregorian 400-year cycle. Keep the
        // projected year in the final supported cycle so the query uses that tail.
        JIFF_MAX_YEAR - GREGORIAN_CYCLE_YEARS + 1 + year.rem_euclid(GREGORIAN_CYCLE_YEARS)
    } else {
        // Before the first TZif transition the initial offset is constant. Keep the projected
        // year in the first supported cycle so that historical rule remains stable.
        let distance = i64::from(year) - i64::from(JIFF_MIN_YEAR);
        JIFF_MIN_YEAR + distance.rem_euclid(i64::from(GREGORIAN_CYCLE_YEARS)) as i32
    };
    [Some(projected), None]
}

#[cfg(feature = "system-timezone")]
fn jiff_civil_datetime(local: CivilDateTime, year: i32) -> Option<jiff::civil::DateTime> {
    jiff::civil::DateTime::new(
        year.try_into().ok()?,
        local.month().try_into().ok()?,
        local.day().try_into().ok()?,
        local.hour().try_into().ok()?,
        local.minute().try_into().ok()?,
        local.second().try_into().ok()?,
        (local.millisecond() * 1_000_000).try_into().ok()?,
    )
    .ok()
}

#[cfg(feature = "system-timezone")]
fn resolve_local_system(
    time_zone: &jiff::tz::TimeZone,
    local: CivilDateTime,
) -> Option<OffsetDateTime> {
    for year in jiff_year_candidates(local.year()).into_iter().flatten() {
        let Some(civil) = jiff_civil_datetime(local, year) else {
            continue;
        };
        let Ok(timestamp) = time_zone.to_timestamp(civil) else {
            continue;
        };
        let offset = UtcOffset::from_seconds(time_zone.to_offset(timestamp).seconds())?;
        let resolved = OffsetDateTime::from_unix_millis(timestamp.as_millisecond(), offset);
        if year == local.year() {
            return Some(resolved);
        }

        let projected = CivilDate::new(year, local.month(), local.day())?.at_hms_milli(
            local.hour(),
            local.minute(),
            local.second(),
            local.millisecond(),
        )?;
        let compatible_shift = resolved
            .local_datetime()
            .signed_duration_millis_since(projected)?;
        let original_resolved = local.checked_add_millis(compatible_shift)?;
        return OffsetDateTime::from_local(original_resolved, offset);
    }
    None
}

#[cfg(feature = "system-timezone")]
fn utc_timestamp_candidates(utc: CivilDateTime) -> impl Iterator<Item = jiff::Timestamp> {
    jiff_year_candidates(utc.year())
        .into_iter()
        .flatten()
        .filter_map(move |year| {
            let mapped = CivilDate::new(year, utc.month(), utc.day())?.at_hms_milli(
                utc.hour(),
                utc.minute(),
                utc.second(),
                utc.millisecond(),
            )?;
            jiff::Timestamp::from_millisecond(mapped.naive_unix_millis()?).ok()
        })
}

#[cfg(feature = "system-timezone")]
fn system_provenance(time_zone: &jiff::tz::TimeZone) -> LocalTimeZoneProvenance {
    let identifier = time_zone.iana_name().map(str::to_owned).unwrap_or_else(|| {
        if time_zone.is_unknown() {
            "Etc/Unknown".to_string()
        } else {
            format!("{time_zone:?}")
        }
    });

    let mut digest = Sha256::new();
    digest.update(b"merman-local-time-zone-offset-rules-v1\0");
    let minimum = jiff::Timestamp::MIN;
    digest.update(time_zone.to_offset(minimum).seconds().to_be_bytes());
    for transition in time_zone.following(minimum) {
        let timestamp = transition.timestamp();
        digest.update(timestamp.as_second().to_be_bytes());
        digest.update(timestamp.subsec_nanosecond().to_be_bytes());
        digest.update(transition.offset().seconds().to_be_bytes());
    }
    let hash = digest.finalize();
    let mut rules_sha256 = String::with_capacity(hash.len() * 2);
    use std::fmt::Write as _;
    for byte in hash {
        write!(rules_sha256, "{byte:02x}").expect("writing to a String cannot fail");
    }

    LocalTimeZoneProvenance {
        source: LocalTimeZoneSource::System,
        identifier,
        rules_sha256: Some(rules_sha256),
    }
}

#[cfg(all(test, feature = "system-timezone"))]
mod system_timezone_tests {
    use super::*;

    fn date_time(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> CivilDateTime {
        CivilDate::new(year, month, day)
            .expect("valid date")
            .at_hms(hour, minute, 0)
            .expect("valid time")
    }

    #[test]
    fn deterministic_resolver_construction_uses_bounded_stack() {
        let handle = std::thread::Builder::new()
            .name("merman-core-deterministic-time-zone-small-stack".to_string())
            .stack_size(256 * 1024)
            .spawn(|| std::hint::black_box(LocalTimeZone::utc()))
            .expect("spawn deterministic time-zone construction test");

        handle
            .join()
            .expect("deterministic time-zone construction should not overflow a 256 KiB stack");
    }

    #[test]
    fn system_resolver_supports_mermaid_year_boundary_beyond_jiff_civil_range() {
        let resolver = LocalTimeZone::try_system().expect("system time-zone adapter");
        for year in [-10000, 10000] {
            let local = date_time(year, 1, 1, 0, 0);
            let resolved = resolver
                .resolve_local(local)
                .expect("system TZif rules should project the boundary year");
            assert_eq!(resolved.local_datetime(), local);
        }
    }

    #[test]
    fn system_resolver_maps_out_of_range_utc_instants_without_losing_the_instant() {
        let resolver = LocalTimeZone::try_system().expect("system time-zone adapter");
        let utc = OffsetDateTime::from_local(date_time(10000, 1, 1, 0, 0), UtcOffset::UTC)
            .expect("boundary UTC instant");
        let local = resolver
            .at_instant(utc.timestamp_millis())
            .expect("system TZif rules should resolve the boundary instant");
        assert_eq!(local.timestamp_millis(), utc.timestamp_millis());
    }

    #[test]
    fn system_resolver_preserves_compatible_dst_gap_resolution() {
        let time_zone = jiff::tz::TimeZone::posix("EST5EDT,M3.2.0,M11.1.0")
            .expect("valid deterministic US Eastern POSIX rule");
        let resolver = LocalTimeZone {
            data: Arc::new(LocalTimeZoneData {
                inner: LocalTimeZoneInner::System(time_zone),
                provenance: OnceLock::new(),
            }),
        };
        let resolved = resolver
            .resolve_local(date_time(2026, 3, 8, 2, 30))
            .expect("compatible DST gap resolution should succeed");
        assert_eq!(resolved.local_datetime().hour(), 3);
        assert_eq!(resolved.local_datetime().minute(), 30);
    }

    #[test]
    fn system_resolver_preserves_compatible_dst_fold_resolution() {
        let time_zone = jiff::tz::TimeZone::posix("EST5EDT,M3.2.0,M11.1.0")
            .expect("valid deterministic US Eastern POSIX rule");
        let resolver = LocalTimeZone {
            data: Arc::new(LocalTimeZoneData {
                inner: LocalTimeZoneInner::System(time_zone),
                provenance: OnceLock::new(),
            }),
        };
        let repeated = date_time(2026, 11, 1, 1, 30);

        let resolved = resolver
            .resolve_local(repeated)
            .expect("compatible DST fold resolution should succeed");

        assert_eq!(resolved.local_datetime(), repeated);
        assert_eq!(resolved.offset().seconds(), -4 * 60 * 60);
    }
}

use super::*;

pub(super) fn today_midnight_local() -> Option<OffsetDateTime> {
    local_from_civil(crate::runtime::today_local().at_midnight())
}

pub(super) fn local_from_civil(local: CivilDateTime) -> Option<OffsetDateTime> {
    crate::runtime::resolve_local_datetime(local)
}

pub(super) fn add_days_local(dt: OffsetDateTime, days: i64) -> Option<OffsetDateTime> {
    let local = crate::runtime::datetime_to_local_civil(dt);
    local_from_civil(local.checked_add_days(days)?)
}

pub(super) fn add_months_local(dt: OffsetDateTime, months: i64) -> Option<OffsetDateTime> {
    let local = crate::runtime::datetime_to_local_civil(dt);
    local_from_civil(local.checked_add_months(months)?)
}

pub(super) fn add_years_local(dt: OffsetDateTime, years: i64) -> Option<OffsetDateTime> {
    let local = crate::runtime::datetime_to_local_civil(dt);
    local_from_civil(local.checked_add_years(years)?)
}

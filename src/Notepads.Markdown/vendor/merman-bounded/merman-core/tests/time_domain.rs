use merman_core::time::{CivilDate, CivilDateTime, OffsetDateTime, UtcOffset, Weekday};

#[test]
fn civil_dates_support_mermaid_year_boundaries() {
    let upper = CivilDate::new(10_000, 1, 1).expect("Mermaid permits year 10000");
    let lower = CivilDate::new(-10_000, 1, 1).expect("Mermaid permits year -10000");

    assert_eq!(upper.to_string(), "+10000-01-01");
    assert_eq!(lower.to_string(), "-10000-01-01");
    assert_eq!(
        upper.weekday(),
        CivilDate::new(9600, 1, 1).unwrap().weekday()
    );
    assert_eq!(
        lower.weekday(),
        CivilDate::new(-9600, 1, 1).unwrap().weekday()
    );
}

#[test]
fn civil_date_validation_uses_proleptic_gregorian_rules() {
    assert!(CivilDate::new(2000, 2, 29).is_some());
    assert!(CivilDate::new(1900, 2, 29).is_none());
    assert!(CivilDate::new(2026, 13, 1).is_none());
    assert!(CivilDate::new(2026, 4, 31).is_none());
    assert_eq!(
        CivilDate::new(1970, 1, 1).unwrap().weekday(),
        Weekday::Thursday
    );
}

#[test]
fn civil_date_parsing_is_strict_and_round_trips() {
    for value in ["2026-08-03", "+10000-01-01", "-10000-12-31"] {
        let parsed: CivilDate = value.parse().expect("valid ISO civil date");
        assert_eq!(parsed.to_string(), value);
    }

    for invalid in [
        "2026-8-03",
        "2026-02-30",
        "10000-01-01",
        "+2026-08-03",
        "+010000-01-01",
        "-0000-01-01",
        "-010000-01-01",
        "2026/08/03",
    ] {
        assert!(invalid.parse::<CivilDate>().is_err(), "{invalid}");
    }
}

#[test]
fn offset_datetimes_round_trip_wide_years_and_negative_instants() {
    let offset = UtcOffset::from_minutes(480).expect("valid offset");
    let local = CivilDateTime::new(CivilDate::new(10_000, 1, 2).unwrap(), 3, 4, 5, 678).unwrap();
    let datetime = OffsetDateTime::from_local(local, offset).expect("representable instant");

    assert_eq!(datetime.local_datetime(), local);
    assert_eq!(
        datetime.to_offset(UtcOffset::UTC).local_datetime(),
        datetime.utc_datetime()
    );

    let before_epoch = OffsetDateTime::from_unix_millis(-1, UtcOffset::UTC);
    assert_eq!(
        before_epoch.local_datetime().to_string(),
        "1969-12-31T23:59:59.999"
    );
    assert_eq!(before_epoch.timestamp_seconds(), -1);
    assert_eq!(before_epoch.local_datetime().millisecond(), 999);
}

#[test]
fn calendar_arithmetic_clamps_months_and_preserves_clock_fields() {
    let leap_day =
        CivilDateTime::new(CivilDate::new(2024, 2, 29).unwrap(), 12, 34, 56, 789).unwrap();

    assert_eq!(
        leap_day.checked_add_years(1).unwrap().to_string(),
        "2025-02-28T12:34:56.789"
    );
    assert_eq!(
        leap_day.checked_add_months(12).unwrap().to_string(),
        "2025-02-28T12:34:56.789"
    );
    assert_eq!(
        leap_day.checked_add_days(1).unwrap().to_string(),
        "2024-03-01T12:34:56.789"
    );
}

#[test]
fn full_i64_instant_range_round_trips_without_panicking() {
    for unix_millis in [i64::MIN, i64::MAX] {
        let datetime = OffsetDateTime::from_unix_millis(unix_millis, UtcOffset::UTC);
        let utc = datetime.utc_datetime();

        assert_eq!(
            OffsetDateTime::from_local(utc, UtcOffset::UTC)
                .expect("an i64 instant must round trip")
                .timestamp_millis(),
            unix_millis
        );
    }
}

#[test]
fn iso_week_is_total_at_signed_year_boundaries() {
    for date in [
        CivilDate::new(i32::MIN, 1, 1).unwrap(),
        CivilDate::new(i32::MAX, 12, 31).unwrap(),
    ] {
        let week = date.iso_week();
        assert!((1..=53).contains(&week.week()));
        assert!((i64::from(i32::MIN) - 1..=i64::from(i32::MAX) + 1).contains(&week.year()));
    }
}

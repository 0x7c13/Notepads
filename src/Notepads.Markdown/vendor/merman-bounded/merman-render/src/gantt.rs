use crate::Result;
use crate::config::{config_bool as cfg_bool, config_f64 as cfg_f64};
use crate::model::{
    Bounds, GanttAxisTickLayout, GanttDiagramLayout, GanttExcludeRangeLayout, GanttRowLayout,
    GanttSectionTitleLayout, GanttTaskBarLayout, GanttTaskLabelLayout, GanttTaskLayout,
};
use crate::text::{TextMeasurer, TextStyle, mermaid_html_breaks_to_newlines};
use merman_core::time::{CivilDate, CivilDateTime, LocalTimeZone, OffsetDateTime, Weekday};
use std::collections::{HashMap, hash_map::Entry};
use std::fmt::Write as _;

use merman_core::diagrams::gantt::{GanttDiagramRenderModel, GanttRenderTask};

// Mermaid falls back to 1200 only when the parent element exposes no `offsetWidth`.
const DEFAULT_CONTAINER_WIDTH: f64 = 1200.0;
const MS_PER_DAY: i64 = 86_400_000;

fn instant_to_local(ms: i64, local_time_zone: &LocalTimeZone) -> Option<OffsetDateTime> {
    local_time_zone.at_instant(ms)
}

fn cfg_i64(cfg: &serde_json::Value, path: &[&str]) -> Option<i64> {
    let mut cur = cfg;
    for k in path {
        cur = cur.get(*k)?;
    }
    cur.as_i64()
}

fn month_name_short(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

fn month_name_long(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn ordinal_suffix(n: u32) -> &'static str {
    let nn = n % 100;
    if (11..=13).contains(&nn) {
        return "th";
    }
    match n % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

fn format_dayjs_like(ms: i64, fmt: &str, local_time_zone: &LocalTimeZone) -> Option<String> {
    let dt = instant_to_local(ms, local_time_zone)?;
    let civil = dt.local_datetime();
    let fmt = fmt.trim();

    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '[' {
            i += 1;
            while i < chars.len() && chars[i] != ']' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && chars[i] == ']' {
                i += 1;
            }
            continue;
        }

        let rest: String = chars[i..].iter().collect();
        let token = [
            "YYYY", "MMMM", "MMM", "dddd", "ddd", "YY", "MM", "DD", "Do", "HH", "hh", "mm", "ss",
            "SSS", "ZZ", "Z", "A", "a", "x", "X", "M", "D", "H", "h", "m", "s",
        ]
        .into_iter()
        .find(|t| rest.starts_with(t));

        if let Some(t) = token {
            match t {
                "YYYY" => out.push_str(&format!("{:04}", civil.year())),
                "YY" => out.push_str(&format!("{:02}", (civil.year() % 100).abs())),
                "MMMM" => out.push_str(month_name_long(civil.month())),
                "MMM" => out.push_str(month_name_short(civil.month())),
                "MM" => out.push_str(&format!("{:02}", civil.month())),
                "M" => out.push_str(&format!("{}", civil.month())),
                "DD" => out.push_str(&format!("{:02}", civil.day())),
                "D" => out.push_str(&format!("{}", civil.day())),
                "Do" => out.push_str(&format!("{}{}", civil.day(), ordinal_suffix(civil.day()))),
                "dddd" => out.push_str(civil.weekday().full_name()),
                "ddd" => out.push_str(civil.weekday().short_name()),
                "HH" => out.push_str(&format!("{:02}", civil.hour())),
                "H" => out.push_str(&format!("{}", civil.hour())),
                "hh" => {
                    let h = civil.hour() % 12;
                    let h = if h == 0 { 12 } else { h };
                    out.push_str(&format!("{:02}", h));
                }
                "h" => {
                    let h = civil.hour() % 12;
                    let h = if h == 0 { 12 } else { h };
                    out.push_str(&format!("{}", h));
                }
                "mm" => out.push_str(&format!("{:02}", civil.minute())),
                "m" => out.push_str(&format!("{}", civil.minute())),
                "ss" => out.push_str(&format!("{:02}", civil.second())),
                "s" => out.push_str(&format!("{}", civil.second())),
                "SSS" => out.push_str(&format!("{:03}", civil.millisecond())),
                "A" => out.push_str(if civil.hour() < 12 { "AM" } else { "PM" }),
                "a" => out.push_str(if civil.hour() < 12 { "am" } else { "pm" }),
                "Z" => {
                    let off = dt.offset().seconds();
                    let sign = if off >= 0 { '+' } else { '-' };
                    let off = off.abs();
                    let hh = off / 3600;
                    let mm = (off % 3600) / 60;
                    out.push_str(&format!("{sign}{:02}:{:02}", hh, mm));
                }
                "ZZ" => {
                    let off = dt.offset().seconds();
                    let sign = if off >= 0 { '+' } else { '-' };
                    let off = off.abs();
                    let hh = off / 3600;
                    let mm = (off % 3600) / 60;
                    out.push_str(&format!("{sign}{:02}{:02}", hh, mm));
                }
                "x" => out.push_str(&format!("{ms}")),
                "X" => out.push_str(&format!("{}", dt.timestamp_seconds())),
                _ => {}
            }
            i += t.len();
            continue;
        }

        out.push(c);
        i += 1;
    }

    Some(out)
}

fn format_yyyy_mm_dd(ms: i64, local_time_zone: &LocalTimeZone) -> Option<String> {
    format_dayjs_like(ms, "YYYY-MM-DD", local_time_zone)
}

fn weekend_start_day(weekend: &str) -> u32 {
    match weekend {
        "friday" => 5,
        _ => 6,
    }
}

fn is_invalid_date(
    ms: i64,
    date_format: &str,
    excludes: &[String],
    includes: &[String],
    weekend: &str,
    local_time_zone: &LocalTimeZone,
) -> bool {
    let Some(formatted_date) = format_dayjs_like(ms, date_format, local_time_zone) else {
        return false;
    };
    let Some(date_only) = format_yyyy_mm_dd(ms, local_time_zone) else {
        return false;
    };

    if includes
        .iter()
        .any(|t| t == &formatted_date || t == &date_only)
    {
        return false;
    }

    let Some(dt) = instant_to_local(ms, local_time_zone) else {
        return false;
    };
    let iso_weekday = dt.weekday().number_from_monday();

    if excludes.iter().any(|t| t == "weekends") {
        let start = weekend_start_day(weekend);
        if iso_weekday == start || iso_weekday == start + 1 {
            return true;
        }
    }

    let weekday_lower = dt.weekday().full_name().to_lowercase();
    if excludes.iter().any(|t| t == &weekday_lower) {
        return true;
    }

    excludes
        .iter()
        .any(|t| t == &formatted_date || t == &date_only)
}

pub(crate) fn start_of_day_ms(ms: i64, local_time_zone: &LocalTimeZone) -> Option<i64> {
    let date = instant_to_local(ms, local_time_zone)?.date();
    Some(
        local_time_zone
            .resolve_local(date.at_midnight())?
            .timestamp_millis(),
    )
}

fn add_local_days_ms(ms: i64, days: i64, local_time_zone: &LocalTimeZone) -> Option<i64> {
    let local = instant_to_local(ms, local_time_zone)?.local_datetime();
    Some(
        local_time_zone
            .resolve_local(local.checked_add_days(days)?)?
            .timestamp_millis(),
    )
}

fn end_of_day_ms(ms: i64, local_time_zone: &LocalTimeZone) -> Option<i64> {
    let start = start_of_day_ms(ms, local_time_zone)?;
    add_local_days_ms(start, 1, local_time_zone)?.checked_sub(1)
}

fn absolute_millis_between(a: i64, b: i64) -> i128 {
    (i128::from(a) - i128::from(b)).abs()
}

fn scale_time(ms: i64, min_ms: i64, max_ms: i64, range: f64) -> f64 {
    if max_ms <= min_ms {
        // D3 scaleTime returns the midpoint of the range for degenerate domains.
        // This matters for fixtures where parsing fails and `startTime == endTime` (width=0).
        return (range / 2.0).round();
    }
    let elapsed = i128::from(ms) - i128::from(min_ms);
    let span = i128::from(max_ms) - i128::from(min_ms);
    let t = elapsed as f64 / span as f64;
    (t * range).round()
}

fn collect_categories(tasks: &[GanttRenderTask]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tasks {
        if t.vert {
            continue;
        }
        if !out.iter().any(|x| x == &t.task_type) {
            out.push(t.task_type.clone());
        }
    }
    out
}

fn get_max_intersections(tasks: &mut [GanttRenderTask], order_offset: i64) -> i64 {
    let mut timeline: Vec<i64> = vec![i64::MIN; tasks.len()];
    let mut sorted: Vec<usize> = (0..tasks.len()).collect();
    sorted.sort_by(|&a, &b| {
        let ta = tasks[a].start_ms;
        let tb = tasks[b].start_ms;
        ta.cmp(&tb)
            .then_with(|| tasks[a].order.cmp(&tasks[b].order))
    });

    let mut max_i: i64 = 0;
    for idx in sorted {
        for (j, slot) in timeline.iter_mut().enumerate() {
            if tasks[idx].start_ms >= *slot {
                *slot = tasks[idx].end_ms;
                tasks[idx].order = j as i64 + order_offset;
                max_i = max_i.max(j as i64);
                break;
            }
        }
    }
    max_i
}

fn tick_step(start: f64, stop: f64, count: f64) -> i64 {
    if !start.is_finite() || !stop.is_finite() || !count.is_finite() || count <= 0.0 {
        return 1;
    }
    let span = (stop - start).abs();
    if span <= 0.0 {
        return 1;
    }
    let step0 = span / count;
    let power = 10f64.powf(step0.log10().floor());
    let error = step0 / power;
    let factor = if error >= 7.5 {
        10.0
    } else if error >= 3.5 {
        5.0
    } else if error >= 1.5 {
        2.0
    } else {
        1.0
    };
    (factor * power).round().max(1.0) as i64
}

fn auto_tick_interval(min_ms: i64, max_ms: i64) -> (i64, &'static str) {
    // Matches the shape of d3-time's default tick interval selection (used by Mermaid when no
    // custom tickInterval is specified). The key properties we need for SVG DOM parity are:
    // - choosing from the same "nice" interval set (e.g. 1h/3h/6h/12h, not 2h)
    // - aligning ticks to interval boundaries (handled in build_ticks)
    const TARGET_TICKS: f64 = 10.0;
    const MS: f64 = 1.0;
    const SEC: f64 = 1_000.0;
    const MIN: f64 = 60_000.0;
    const HOUR: f64 = 3_600_000.0;
    const DAY: f64 = MS_PER_DAY as f64;
    const WEEK: f64 = (MS_PER_DAY * 7) as f64;
    const MONTH: f64 = (MS_PER_DAY * 30) as f64;
    const YEAR: f64 = (MS_PER_DAY * 365) as f64;

    let span_ms = absolute_millis_between(max_ms, min_ms).max(1) as f64;
    let target = span_ms / TARGET_TICKS;

    let mut intervals: Vec<(f64, i64, &'static str)> = Vec::new();
    for (every, unit_ms) in [
        (1, MS),
        (2, MS),
        (5, MS),
        (10, MS),
        (20, MS),
        (50, MS),
        (100, MS),
        (200, MS),
        (500, MS),
    ] {
        intervals.push((unit_ms * every as f64, every, "millisecond"));
    }
    for (every, unit_ms, unit) in [
        (1, SEC, "second"),
        (5, SEC, "second"),
        (15, SEC, "second"),
        (30, SEC, "second"),
        (1, MIN, "minute"),
        (5, MIN, "minute"),
        (15, MIN, "minute"),
        (30, MIN, "minute"),
        (1, HOUR, "hour"),
        (3, HOUR, "hour"),
        (6, HOUR, "hour"),
        (12, HOUR, "hour"),
        (1, DAY, "day"),
        (2, DAY, "day"),
        (1, WEEK, "week"),
        (1, MONTH, "month"),
        (3, MONTH, "month"),
        (1, YEAR, "year"),
    ] {
        intervals.push((unit_ms * (every as f64), every, unit));
    }
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut i = 0usize;
    while i < intervals.len() && intervals[i].0 < target {
        i += 1;
    }

    if i == 0 {
        let (_dur, every, unit) = intervals[0];
        return (every, unit);
    }

    if i >= intervals.len() {
        let years = tick_step(min_ms as f64 / YEAR, max_ms as f64 / YEAR, TARGET_TICKS);
        return (years, "year");
    }

    let (d0, e0, u0) = intervals[i - 1];
    let (d1, e1, u1) = intervals[i];
    if target / d0 < d1 / target {
        (e0, u0)
    } else {
        (e1, u1)
    }
}

fn parse_tick_interval(s: &str) -> Option<(i64, &str)> {
    let s = s.trim();
    let mut num = String::new();
    let mut idx = 0;
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
            idx += 1;
        } else {
            break;
        }
    }
    let every = num.parse::<i64>().ok()?;
    if every <= 0 {
        return None;
    }
    let unit = &s[idx..];
    match unit {
        "millisecond" | "second" | "minute" | "hour" | "day" | "week" | "month" => {
            Some((every, unit))
        }
        _ => None,
    }
}

fn add_interval(ms: i64, every: i64, unit: &str, local_time_zone: &LocalTimeZone) -> Option<i64> {
    let elapsed_millis = match unit {
        "millisecond" => Some(every),
        "second" => every.checked_mul(1_000),
        "minute" => every.checked_mul(60_000),
        "hour" => every.checked_mul(3_600_000),
        _ => None,
    };
    if let Some(elapsed_millis) = elapsed_millis {
        return ms.checked_add(elapsed_millis);
    }

    let local = instant_to_local(ms, local_time_zone)?.local_datetime();
    let next = match unit {
        "day" => local.checked_add_days(every)?,
        "week" => local.checked_add_days(every.checked_mul(7)?)?,
        "month" => local.checked_add_months(every)?,
        "year" => local.checked_add_years(every)?,
        _ => return None,
    };
    Some(local_time_zone.resolve_local(next)?.timestamp_millis())
}

fn weekday_from_str(s: &str) -> Option<Weekday> {
    match s.trim().to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        "saturday" => Some(Weekday::Saturday),
        "sunday" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn civil_with_time(date: CivilDate, time: CivilDateTime) -> Option<CivilDateTime> {
    date.at_hms_milli(
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond(),
    )
}

#[derive(Clone, Copy)]
struct ElapsedTickState {
    lower_millis: i64,
    field: i64,
}

fn ceil_elapsed_tick_start_with(
    min_ms: i64,
    every: i64,
    unit_millis: i64,
    mut state_at: impl FnMut(i64) -> Option<ElapsedTickState>,
) -> Option<i64> {
    let initial = state_at(min_ms)?;
    let floor = min_ms.checked_sub(initial.lower_millis)?;
    let mut candidate = if floor < min_ms {
        floor.checked_add(unit_millis)?
    } else {
        floor
    };
    let every = every.max(1);

    loop {
        if state_at(candidate)?.field.rem_euclid(every) == 0 {
            return Some(candidate);
        }
        candidate = candidate.checked_add(unit_millis)?;
    }
}

fn elapsed_tick_state(
    ms: i64,
    unit: &str,
    local_time_zone: &LocalTimeZone,
) -> Option<ElapsedTickState> {
    let local = instant_to_local(ms, local_time_zone)?.local_datetime();
    match unit {
        "second" => Some(ElapsedTickState {
            lower_millis: i64::from(local.millisecond()),
            field: i64::from(local.second()),
        }),
        "minute" => Some(ElapsedTickState {
            lower_millis: i64::from(local.second()) * 1_000 + i64::from(local.millisecond()),
            field: i64::from(local.minute()),
        }),
        "hour" => Some(ElapsedTickState {
            lower_millis: i64::from(local.minute()) * 60_000
                + i64::from(local.second()) * 1_000
                + i64::from(local.millisecond()),
            field: i64::from(local.hour()),
        }),
        _ => None,
    }
}

fn ceil_tick_start(
    min_ms: i64,
    every: i64,
    unit: &str,
    week_start: Option<&str>,
    local_time_zone: &LocalTimeZone,
) -> Option<i64> {
    if let Some(unit_millis) = match unit {
        "second" => Some(1_000),
        "minute" => Some(60_000),
        "hour" => Some(3_600_000),
        _ => None,
    } {
        return ceil_elapsed_tick_start_with(min_ms, every, unit_millis, |ms| {
            elapsed_tick_state(ms, unit, local_time_zone)
        });
    }

    let start = match unit {
        "millisecond" => {
            let e = every.max(1);
            // D3's `millisecond.every(e)` aligns using `Math.floor(date / e) * e`, and `range`
            // starts at `ceil(start)`. Use Euclidean division so negative timestamps match D3.
            let r = min_ms.rem_euclid(e);
            let aligned = if r == 0 {
                min_ms
            } else {
                min_ms.checked_add(e.checked_sub(r)?)?
            };
            return Some(aligned);
        }
        "day" => {
            let local = instant_to_local(min_ms, local_time_zone)?.local_datetime();
            let mut cur = local.date().at_midnight();
            if cur < local {
                cur = cur.checked_add_days(1)?;
            }
            let e = every.max(1);
            while i64::from(cur.day0()).rem_euclid(e) != 0 {
                cur = cur.checked_add_days(1)?;
            }
            cur
        }
        "week" => {
            let local = instant_to_local(min_ms, local_time_zone)?.local_datetime();
            let epoch = CivilDate::new(1970, 1, 4)?; // Sunday
            let start = week_start
                .and_then(weekday_from_str)
                .unwrap_or(Weekday::Sunday);

            let mut d = local.date();
            let cur_wd = i64::from(d.weekday().number_from_sunday() - 1);
            let start_wd = i64::from(start.number_from_sunday() - 1);
            let delta = (cur_wd - start_wd).rem_euclid(7);
            d = d.checked_sub_days(delta)?;
            let mut cur = d.at_midnight();
            if cur < local {
                cur = cur.checked_add_days(7)?;
            }

            let e = every.max(1);
            if e > 1 {
                let mut ws = cur.date();
                let weeks = ws.days_since(epoch).div_euclid(7);
                let rem = weeks.rem_euclid(e);
                if rem != 0 {
                    let delta_days = e.checked_sub(rem)?.checked_mul(7)?;
                    ws = ws.checked_add_days(delta_days)?;
                }
                cur = ws.at_midnight();
            }
            cur
        }
        "month" => {
            let local = instant_to_local(min_ms, local_time_zone)?.local_datetime();
            let month_index = |y: i32, m: u32| (y as i64) * 12 + (m as i64 - 1);

            let mut y = local.year();
            let mut m = local.month();
            let mut cur = CivilDate::new(y, m, 1)?.at_midnight();
            if cur < local {
                if m == 12 {
                    m = 1;
                    y = y.checked_add(1)?;
                } else {
                    m = m.checked_add(1)?;
                }
                cur = CivilDate::new(y, m, 1)?.at_midnight();
            }

            let e = every.max(1);
            if e > 1 {
                let mut idx = month_index(y, m);
                let rem = idx.rem_euclid(e);
                if rem != 0 {
                    idx = idx.checked_add(e.checked_sub(rem)?)?;
                    y = idx.div_euclid(12).try_into().ok()?;
                    m = u32::try_from(idx.rem_euclid(12)).ok()?.checked_add(1)?;
                    cur = CivilDate::new(y, m, 1)?.at_midnight();
                }
            }
            cur
        }
        "year" => {
            let local = instant_to_local(min_ms, local_time_zone)?.local_datetime();
            let mut y = i64::from(local.year());
            let initial_year: i32 = y.try_into().ok()?;
            let mut cur = CivilDate::new(initial_year, 1, 1)?.at_midnight();
            if cur < local {
                y = y.checked_add(1)?;
                let year: i32 = y.try_into().ok()?;
                cur = CivilDate::new(year, 1, 1)?.at_midnight();
            }
            let e = every.max(1);
            if e > 1 {
                let rem = y.rem_euclid(e);
                if rem != 0 {
                    y = y.checked_add(e.checked_sub(rem)?)?;
                    let year: i32 = y.try_into().ok()?;
                    cur = CivilDate::new(year, 1, 1)?.at_midnight();
                }
            }
            cur
        }
        _ => return None,
    };

    Some(local_time_zone.resolve_local(start)?.timestamp_millis())
}

fn add_d3_time_day_every(ms: i64, every: i64, local_time_zone: &LocalTimeZone) -> Option<i64> {
    let local = instant_to_local(ms, local_time_zone)?.local_datetime();

    let e = every.max(1);
    if e <= 1 {
        return add_interval(ms, 1, "day", local_time_zone);
    }

    // D3's `timeDay.every(e)` uses a filtered interval based on `(date.getDate() - 1) % e`.
    // This means the modulus resets at month boundaries, and the "next tick" is not simply
    // `+e days` for months with non-multiple-of-e lengths.
    let mut next_date = local.date().checked_add_days(1)?;
    while i64::from(next_date.day0()).rem_euclid(e) != 0 {
        next_date = next_date.checked_add_days(1)?;
    }
    let next = civil_with_time(next_date, local)?;
    Some(local_time_zone.resolve_local(next)?.timestamp_millis())
}

fn axis_format_to_strftime(axis_format: &str, date_format: &str, cfg_axis_format: &str) -> String {
    if !axis_format.trim().is_empty() {
        // Mermaid preserves any leading/trailing whitespace in `axisFormat` (it is treated as
        // literal text by d3-time-format). Keep the raw string for DOM parity.
        return axis_format.to_string();
    }
    if date_format.trim() == "D" {
        return "%d".to_string();
    }
    if !cfg_axis_format.trim().is_empty() {
        return cfg_axis_format.to_string();
    }
    "%Y-%m-%d".to_string()
}

fn write_d3_padded(out: &mut String, value: i64, fill: Option<char>, width: usize) {
    if value < 0 {
        out.push('-');
    }
    let absolute = value.unsigned_abs();
    let digits = if absolute == 0 {
        1
    } else {
        absolute.ilog10() as usize + 1
    };
    if let Some(fill) = fill {
        for _ in 0..width.saturating_sub(digits) {
            out.push(fill);
        }
    }
    let _ = write!(out, "{absolute}");
}

fn d3_week_number(date: CivilDate, week_start: Weekday) -> u32 {
    let january_first = CivilDate::new(date.year(), 1, 1).expect("January 1 is always valid");
    let january_index = january_first.weekday().number_from_sunday() - 1;
    let start_index = week_start.number_from_sunday() - 1;
    let first_start = (i64::from(start_index) - i64::from(january_index)).rem_euclid(7) as u32;
    let ordinal0 = date.ordinal() - 1;
    if ordinal0 < first_start {
        0
    } else {
        1 + (ordinal0 - first_start) / 7
    }
}

fn format_axis_tick_label(datetime: OffsetDateTime, axis_format: &str) -> String {
    let mut out = String::new();
    format_axis_tick_label_into(datetime, datetime.local_datetime(), axis_format, &mut out);
    out
}

fn format_axis_tick_label_into(
    datetime: OffsetDateTime,
    local: CivilDateTime,
    axis_format: &str,
    out: &mut String,
) {
    let mut it = axis_format.chars().peekable();

    while let Some(ch) = it.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }

        let Some(next) = it.next() else {
            // d3-time-format drops an incomplete trailing directive.
            break;
        };

        let (fill, directive) = if matches!(next, '-' | '_' | '0') {
            let Some(directive) = it.next() else {
                break;
            };
            (
                match next {
                    '-' => None,
                    '_' => Some(' '),
                    _ => Some('0'),
                },
                directive,
            )
        } else {
            (Some(if next == 'e' { ' ' } else { '0' }), next)
        };

        match directive {
            'a' => out.push_str(local.weekday().short_name()),
            'A' => out.push_str(local.weekday().full_name()),
            'b' => out.push_str(month_name_short(local.month())),
            'B' => out.push_str(month_name_long(local.month())),
            'c' => format_axis_tick_label_into(datetime, local, "%x, %X", out),
            'd' | 'e' => write_d3_padded(out, i64::from(local.day()), fill, 2),
            'f' => {
                write_d3_padded(out, i64::from(local.millisecond()), fill, 3);
                out.push_str("000");
            }
            'g' => write_d3_padded(out, local.date().iso_week().year() % 100, fill, 2),
            'G' => write_d3_padded(out, local.date().iso_week().year() % 10_000, fill, 4),
            'H' => write_d3_padded(out, i64::from(local.hour()), fill, 2),
            'I' => {
                let hour = local.hour() % 12;
                write_d3_padded(out, i64::from(if hour == 0 { 12 } else { hour }), fill, 2);
            }
            'j' => write_d3_padded(out, i64::from(local.date().ordinal()), fill, 3),
            'L' => write_d3_padded(out, i64::from(local.millisecond()), fill, 3),
            'm' => write_d3_padded(out, i64::from(local.month()), fill, 2),
            'M' => write_d3_padded(out, i64::from(local.minute()), fill, 2),
            'p' => out.push_str(if local.hour() >= 12 { "PM" } else { "AM" }),
            'q' => {
                let _ = write!(out, "{}", (local.month0() / 3) + 1);
            }
            'Q' => {
                let _ = write!(out, "{}", datetime.timestamp_millis());
            }
            's' => {
                let _ = write!(out, "{}", datetime.timestamp_seconds());
            }
            'S' => write_d3_padded(out, i64::from(local.second()), fill, 2),
            'u' => {
                let _ = write!(out, "{}", local.weekday().number_from_monday());
            }
            'U' => write_d3_padded(
                out,
                i64::from(d3_week_number(local.date(), Weekday::Sunday)),
                fill,
                2,
            ),
            'V' => write_d3_padded(out, i64::from(local.date().iso_week().week()), fill, 2),
            'w' => {
                let _ = write!(out, "{}", local.weekday().number_from_sunday() - 1);
            }
            'W' => write_d3_padded(
                out,
                i64::from(d3_week_number(local.date(), Weekday::Monday)),
                fill,
                2,
            ),
            'x' => format_axis_tick_label_into(datetime, local, "%-m/%-d/%Y", out),
            'X' => format_axis_tick_label_into(datetime, local, "%-I:%M:%S %p", out),
            'y' => write_d3_padded(out, i64::from(local.year()) % 100, fill, 2),
            'Y' => write_d3_padded(out, i64::from(local.year()) % 10_000, fill, 4),
            'Z' => {
                let seconds = datetime.offset().seconds();
                let sign = if seconds < 0 { '-' } else { '+' };
                let minutes = seconds.unsigned_abs() / 60;
                let _ = write!(out, "{sign}{:02}{:02}", minutes / 60, minutes % 60);
            }
            '%' => out.push('%'),
            // d3-time-format emits the unknown directive character without the leading `%`.
            unknown => out.push(unknown),
        }
    }
}

struct GanttTickRequest<'a> {
    min_ms: i64,
    max_ms: i64,
    range: f64,
    left_padding: f64,
    axis_format: &'a str,
    tick_interval: Option<&'a str>,
    week_start: Option<&'a str>,
    local_time_zone: &'a LocalTimeZone,
}

fn build_ticks(request: GanttTickRequest<'_>) -> Vec<GanttAxisTickLayout> {
    let GanttTickRequest {
        min_ms,
        max_ms,
        range,
        left_padding,
        axis_format,
        tick_interval,
        week_start,
        local_time_zone,
    } = request;
    const MAX_TICK_COUNT: f64 = 10_000.0;

    fn estimate_ticks(min_ms: i64, max_ms: i64, every: i64, unit: &str) -> f64 {
        if every <= 0 || min_ms > max_ms {
            return f64::INFINITY;
        }

        let time_diff_ms = absolute_millis_between(max_ms, min_ms).max(1) as f64;
        let interval_ms = match unit {
            "millisecond" => every as f64,
            "second" => (every as f64) * 1_000.0,
            "minute" => (every as f64) * 60_000.0,
            "hour" => (every as f64) * 3_600_000.0,
            "day" => (every as f64) * (MS_PER_DAY as f64),
            "week" => (every as f64) * (MS_PER_DAY as f64) * 7.0,
            // dayjs.duration({ month: n }).asMilliseconds() uses a fixed 30-day lattice.
            "month" => (every as f64) * (MS_PER_DAY as f64) * 30.0,
            _ => return f64::INFINITY,
        };
        if interval_ms <= 0.0 {
            return f64::INFINITY;
        }

        (time_diff_ms / interval_ms).ceil()
    }

    // Mermaid skips applying custom ticks when the interval would generate an excessive amount of
    // tick marks (it falls back to d3's automatic tick selection instead).
    let parsed = tick_interval
        .and_then(parse_tick_interval)
        .filter(|(every, unit)| estimate_ticks(min_ms, max_ms, *every, unit) <= MAX_TICK_COUNT);
    let (every, unit) = parsed.unwrap_or_else(|| auto_tick_interval(min_ms, max_ms));
    let week_start = if parsed.is_some() && unit == "week" {
        week_start
    } else {
        None
    };

    let mut ticks = Vec::new();
    let mut cur =
        ceil_tick_start(min_ms, every, unit, week_start, local_time_zone).unwrap_or(min_ms);
    let max_ticks = 2000;
    for _ in 0..max_ticks {
        if cur > max_ms {
            break;
        }
        let x = scale_time(cur, min_ms, max_ms, range) + left_padding;
        let label = instant_to_local(cur, local_time_zone)
            .map(|datetime| format_axis_tick_label(datetime, axis_format))
            .unwrap_or_default();
        ticks.push(GanttAxisTickLayout {
            time_ms: cur,
            x,
            label,
        });
        let next = if unit == "day" && every > 1 {
            add_d3_time_day_every(cur, every, local_time_zone)
        } else {
            add_interval(cur, every, unit, local_time_zone)
        };
        let Some(next) = next else {
            break;
        };
        if next <= cur {
            break;
        }
        cur = next;
    }
    ticks
}

/// Mirrors JavaScript remainder stringification for Mermaid's generated Gantt class names.
///
/// In particular, `index % 0` is `NaN` in JavaScript rather than a thrown division error. The
/// class suffix is observable in Mermaid SVG, so preserve that behavior at the language boundary
/// instead of relying on Rust's integer remainder operator at every call site.
pub(crate) fn gantt_section_class_suffix(
    task_type: &str,
    categories: &[String],
    number_section_styles: i64,
) -> String {
    let Some(index) = categories.iter().position(|category| category == task_type) else {
        return "0".to_string();
    };
    if number_section_styles == 0 {
        return "NaN".to_string();
    }
    ((index as i64) % number_section_styles).to_string()
}

pub(crate) fn layout_gantt_diagram_typed(
    model: &GanttDiagramRenderModel,
    diagram_title: Option<&str>,
    config: &serde_json::Value,
    text_measurer: &dyn TextMeasurer,
    container_width: f64,
    local_time_zone: &merman_core::time::LocalTimeZone,
) -> Result<GanttDiagramLayout> {
    let mut m = model.clone();
    let title = m.title.as_deref().or(diagram_title).map(str::to_owned);

    let gantt_cfg = config.get("gantt").unwrap_or(config);
    let bar_gap = cfg_f64(gantt_cfg, &["barGap"]).unwrap_or(4.0);
    let bar_height = cfg_f64(gantt_cfg, &["barHeight"]).unwrap_or(20.0);
    let top_padding = cfg_f64(gantt_cfg, &["topPadding"]).unwrap_or(50.0);
    let left_padding = cfg_f64(gantt_cfg, &["leftPadding"]).unwrap_or(75.0);
    let right_padding = cfg_f64(gantt_cfg, &["rightPadding"]).unwrap_or(75.0);
    let grid_line_start_padding = cfg_f64(gantt_cfg, &["gridLineStartPadding"]).unwrap_or(35.0);
    let title_top_margin = cfg_f64(gantt_cfg, &["titleTopMargin"]).unwrap_or(25.0);
    let font_size = cfg_f64(gantt_cfg, &["fontSize"]).unwrap_or(11.0);
    let section_font_size = cfg_f64(gantt_cfg, &["sectionFontSize"]).unwrap_or(11.0);
    let number_section_styles = cfg_i64(gantt_cfg, &["numberSectionStyles"]).unwrap_or(4);

    let cfg_display_mode = gantt_cfg
        .get("displayMode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cfg_top_axis = cfg_bool(gantt_cfg, &["topAxis"]).unwrap_or(false);
    let cfg_axis_format = gantt_cfg
        .get("axisFormat")
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d");

    let container_width = if container_width.is_finite() && container_width > 0.0 {
        container_width
    } else {
        DEFAULT_CONTAINER_WIDTH
    };
    let width = gantt_cfg
        .get("useWidth")
        .and_then(|v| v.as_f64())
        .unwrap_or(container_width);
    let gap = bar_height + bar_gap;

    let row_task_count = m.tasks.iter().filter(|task| !task.vert).count();
    let categories = collect_categories(&m.tasks);
    let is_compact = m.display_mode == "compact" || cfg_display_mode == "compact";

    let mut category_heights: Vec<(String, i64)> = Vec::new();
    if is_compact {
        let mut section_order: Vec<String> = Vec::new();
        let mut section_map: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, t) in m.tasks.iter().enumerate() {
            if t.vert {
                continue;
            }
            match section_map.entry(t.section.clone()) {
                Entry::Occupied(mut entry) => entry.get_mut().push(idx),
                Entry::Vacant(entry) => {
                    section_order.push(entry.key().clone());
                    entry.insert(vec![idx]);
                }
            }
        }

        let mut order_offset: i64 = 0;
        for sec in section_order {
            let idxs = section_map.get(&sec).cloned().unwrap_or_default();
            let mut subset: Vec<GanttRenderTask> =
                idxs.iter().map(|&i| m.tasks[i].clone()).collect();
            let max_i = get_max_intersections(&mut subset, order_offset);
            for (pos, &orig_idx) in idxs.iter().enumerate() {
                m.tasks[orig_idx].order = subset[pos].order;
            }
            let height = max_i + 1;
            order_offset += height;
            category_heights.push((sec, height));
        }
    } else {
        for c in &categories {
            let count = m
                .tasks
                .iter()
                .filter(|t| !t.vert && &t.task_type == c)
                .count() as i64;
            category_heights.push((c.clone(), count));
        }
    }

    let mut height = 2.0 * top_padding;
    if is_compact {
        for (_k, h) in &category_heights {
            height += *h as f64 * gap;
        }
    } else {
        height += row_task_count as f64 * gap;
    }

    let has_tasks = !m.tasks.is_empty();
    let (min_ms, max_ms) = if has_tasks {
        let min_ms = m.tasks.iter().map(|t| t.start_ms).min().unwrap_or(0);
        let max_ms = m.tasks.iter().map(|t| t.end_ms).max().unwrap_or(min_ms);
        (min_ms, max_ms)
    } else {
        (0, 0)
    };
    let range = (width - left_padding - right_padding).max(1.0);
    let span_ms = absolute_millis_between(max_ms, min_ms);
    let max_exclude_span_ms = i128::from(MS_PER_DAY) * 365 * 5;
    let has_excludes_layer = has_tasks
        && (!m.excludes.is_empty() || !m.includes.is_empty())
        && span_ms <= max_exclude_span_ms;

    // Sort by start time for rendering.
    m.tasks.sort_by_key(|a| a.start_ms);

    // Exclude day ranges.
    let mut excludes_layout: Vec<GanttExcludeRangeLayout> = Vec::new();
    if has_excludes_layer {
        let mut cur = start_of_day_ms(min_ms, local_time_zone).unwrap_or(min_ms);
        let max_day = start_of_day_ms(max_ms, local_time_zone).unwrap_or(max_ms);
        let mut range_start: Option<i64> = None;
        let mut range_end: Option<i64> = None;
        let mut append_range = |start_ms: i64, last_day_ms: i64| {
            let id = format!(
                "exclude-{}",
                format_yyyy_mm_dd(start_ms, local_time_zone)
                    .unwrap_or_else(|| "invalid".to_string())
            );
            let x0 = scale_time(start_ms, min_ms, max_ms, range) + left_padding;
            let end_ms = end_of_day_ms(last_day_ms, local_time_zone).unwrap_or(last_day_ms);
            let x1 = scale_time(end_ms, min_ms, max_ms, range) + left_padding;
            excludes_layout.push(GanttExcludeRangeLayout {
                id,
                start_ms,
                end_ms,
                x: x0,
                y: grid_line_start_padding,
                width: (x1 - x0).max(0.0),
                height: (height - top_padding - grid_line_start_padding).max(0.0),
            });
        };

        while cur <= max_day {
            let invalid = is_invalid_date(
                cur,
                &m.date_format,
                &m.excludes,
                &m.includes,
                &m.weekend,
                local_time_zone,
            );
            if invalid {
                if range_start.is_none() {
                    range_start = Some(cur);
                    range_end = Some(cur);
                } else {
                    range_end = Some(cur);
                }
            } else if let (Some(start_ms), Some(last_day_ms)) =
                (range_start.take(), range_end.take())
            {
                append_range(start_ms, last_day_ms);
            }
            let Some(next) = add_local_days_ms(cur, 1, local_time_zone) else {
                break;
            };
            if next <= cur {
                break;
            }
            cur = next;
        }
        // Mermaid only materializes a range when a later valid day closes it.
        // A trailing invalid range therefore remains absent from the SVG.
    }

    // Background rows.
    //
    // Mermaid draws the row rectangles by iterating the tasks in their render order (sorted by
    // `startTime`). This means the row insertion order is *not* necessarily ascending by `order`
    // (e.g. forward references can cause `order=0` to have the latest start date).
    let mut row_orders: Vec<i64> = Vec::new();
    for t in &m.tasks {
        if t.vert {
            continue;
        }
        if !row_orders.contains(&t.order) {
            row_orders.push(t.order);
        }
    }

    let mut rows: Vec<GanttRowLayout> = Vec::new();
    for order in &row_orders {
        let ttype = m
            .tasks
            .iter()
            .find(|t| t.order == *order)
            .map(|t| t.task_type.clone())
            .unwrap_or_default();

        let sec_num = gantt_section_class_suffix(&ttype, &categories, number_section_styles);

        let y = *order as f64 * gap + top_padding - 2.0;
        rows.push(GanttRowLayout {
            index: *order,
            x: 0.0,
            y,
            width: width - right_padding / 2.0,
            height: gap,
            class: format!("section section{sec_num}"),
        });
    }

    // Tasks (bars + labels).
    // Mermaid gantt task labels inherit the diagram font family (defaulting to
    // `"trebuchet ms", verdana, arial, sans-serif`), not the axis group's `sans-serif`.
    // Use the effective Mermaid font family here so `getBBox().width`-derived `width-*` class
    // values match upstream SVG baselines.
    let task_font_family = gantt_cfg
        .get("fontFamily")
        .and_then(|v| v.as_str())
        .or_else(|| config.get("fontFamily").and_then(|v| v.as_str()))
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif")
        .to_string();
    let text_style = TextStyle {
        font_family: Some(task_font_family.clone()),
        font_size,
        font_weight: None,
        font_style: None,
    };

    let mut tasks: Vec<GanttTaskLayout> = Vec::new();
    for t in &m.tasks {
        let start_x = scale_time(t.start_ms, min_ms, max_ms, range);
        let end_x = scale_time(t.end_ms, min_ms, max_ms, range);
        let render_end_x = scale_time(t.render_end_ms.unwrap_or(t.end_ms), min_ms, max_ms, range);

        let mut bar_x = start_x + left_padding;
        if t.milestone {
            bar_x = start_x + left_padding + 0.5 * (end_x - start_x) - 0.5 * bar_height;
        }

        let bar_y = if t.vert {
            grid_line_start_padding
        } else {
            t.order as f64 * gap + top_padding
        };
        let bar_width = if t.milestone {
            bar_height
        } else if t.vert {
            0.08 * bar_height
        } else {
            (render_end_x - start_x).max(0.0)
        };
        let bar_height_actual = if t.vert {
            row_task_count as f64 * gap + bar_height * 2.0
        } else {
            bar_height
        };

        let sec_num = gantt_section_class_suffix(&t.task_type, &categories, number_section_styles);

        let mut task_class = String::new();
        if t.active {
            if t.crit {
                task_class.push_str(" activeCrit");
            } else {
                task_class.push_str(" active");
            }
        } else if t.done {
            if t.crit {
                task_class.push_str(" doneCrit");
            } else {
                task_class.push_str(" done");
            }
        } else if t.crit {
            task_class.push_str(" crit");
        }
        if task_class.is_empty() {
            task_class.push_str(" task");
        }
        if t.milestone {
            task_class = format!(" milestone{task_class}");
        }
        if t.vert {
            task_class = format!(" vert{task_class}");
        }
        task_class.push_str(&sec_num.to_string());
        if !t.classes.is_empty() {
            task_class.push(' ');
            task_class.push_str(&t.classes.join(" "));
        }

        let bar = GanttTaskBarLayout {
            id: t.id.clone(),
            x: bar_x,
            y: bar_y,
            width: bar_width,
            height: bar_height_actual,
            rx: 3.0,
            ry: 3.0,
            class: format!("task{task_class}"),
        };

        // Mermaid measures `textWidth` via `this.getBBox().width`, which does not include trailing
        // whitespace. Preserve the original task text for rendering, but trim it for measurement.
        let text_width = text_measurer
            .measure_svg_raw_text_bbox_width_px(t.task.trim_end(), &text_style)
            .max(0.0);

        // Mermaid uses `renderEndTime` for the X-position calculation but `endTime` for the class
        // overflow check. Mirror this quirk for DOM parity.
        let mut start_x_for_label = start_x;
        let mut end_x_for_label = render_end_x;
        if t.milestone {
            start_x_for_label += 0.5 * (end_x - start_x) - 0.5 * bar_height;
            end_x_for_label = start_x_for_label + bar_height;
        }
        let start_x_for_class = start_x;
        let end_x_for_class = if t.milestone {
            start_x + bar_height
        } else {
            end_x
        };

        let label_x = if t.vert {
            start_x + left_padding
        } else if text_width > (end_x_for_label - start_x_for_label).abs() {
            if end_x_for_label + text_width + 1.5 * left_padding > width {
                start_x_for_label + left_padding - 5.0
            } else {
                end_x_for_label + left_padding + 5.0
            }
        } else {
            (end_x_for_label - start_x_for_label) / 2.0 + start_x_for_label + left_padding
        };

        let label_y = if t.vert {
            grid_line_start_padding + row_task_count as f64 * gap + 60.0
        } else {
            t.order as f64 * gap + bar_height / 2.0 + (font_size / 2.0 - 2.0) + top_padding
        };

        let base_classes = if t.classes.is_empty() {
            String::new()
        } else {
            format!("{} ", t.classes.join(" "))
        };

        // Mermaid checks overflow for both horizontal and vertical labels:
        // `if (textWidth > endX - startX) { ... }` (Mermaid@11.12.2 ganttRenderer.js).
        let class_overflows = text_width > (end_x_for_class - start_x_for_class).abs();
        let outside_left =
            class_overflows && (end_x_for_class + text_width + 1.5 * left_padding > width);
        let outside_right = class_overflows && !outside_left;

        let label_class = if outside_left {
            format!("{base_classes}taskTextOutsideLeft taskTextOutside{sec_num}")
        } else if outside_right {
            format!(
                "{base_classes}taskTextOutsideRight taskTextOutside{sec_num} width-{text_width}"
            )
        } else {
            format!("{base_classes}taskText taskText{sec_num} width-{text_width}")
        };

        let label = GanttTaskLabelLayout {
            id: format!("{}-text", t.id),
            text: t.task.clone(),
            font_size,
            width: text_width,
            x: label_x,
            y: label_y,
            class: label_class.trim().to_string(),
        };

        tasks.push(GanttTaskLayout {
            id: t.id.clone(),
            task: t.task.clone(),
            section: t.section.clone(),
            task_type: t.task_type.clone(),
            order: t.order,
            start_ms: t.start_ms,
            end_ms: t.end_ms,
            render_end_ms: t.render_end_ms,
            milestone: t.milestone,
            vert: t.vert,
            bar,
            label,
        });
    }

    // Section titles.
    let mut section_titles: Vec<GanttSectionTitleLayout> = Vec::new();
    let mut prev_gap: i64 = 0;
    for (idx, (sec, h)) in category_heights.iter().enumerate() {
        let lines = mermaid_html_breaks_to_newlines(sec)
            .split('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let dy_em = -((lines.len().saturating_sub(1)) as f64) / 2.0;

        let sec_num = gantt_section_class_suffix(sec, &categories, number_section_styles);

        let y = if idx == 0 {
            (*h as f64 * gap) / 2.0 + top_padding
        } else {
            prev_gap += category_heights[idx - 1].1;
            (*h as f64 * gap) / 2.0 + prev_gap as f64 * gap + top_padding
        };

        section_titles.push(GanttSectionTitleLayout {
            section: sec.clone(),
            index: idx as i64,
            x: 10.0,
            y,
            dy_em,
            lines,
            class: format!("sectionTitle sectionTitle{sec_num}"),
        });
    }

    let axis_format = axis_format_to_strftime(&m.axis_format, &m.date_format, cfg_axis_format);
    let tick_interval = m.tick_interval.as_deref();
    let week_start = if m.weekday.trim().is_empty() {
        gantt_cfg.get("weekday").and_then(|v| v.as_str())
    } else {
        Some(m.weekday.as_str())
    };
    let bottom_ticks = if has_tasks {
        build_ticks(GanttTickRequest {
            min_ms,
            max_ms,
            range,
            left_padding,
            axis_format: &axis_format,
            tick_interval,
            week_start,
            local_time_zone,
        })
    } else {
        Vec::new()
    };
    let top_axis_enabled = m.top_axis || cfg_top_axis;
    let top_ticks = if has_tasks && top_axis_enabled {
        build_ticks(GanttTickRequest {
            min_ms,
            max_ms,
            range,
            left_padding,
            axis_format: &axis_format,
            tick_interval,
            week_start,
            local_time_zone,
        })
    } else {
        Vec::new()
    };

    let bounds = Some(Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: width,
        max_y: height,
    });

    Ok(GanttDiagramLayout {
        bounds,
        width,
        height,
        left_padding,
        right_padding,
        top_padding,
        grid_line_start_padding,
        bar_height,
        bar_gap,
        title_top_margin,
        font_size,
        section_font_size,
        number_section_styles,
        display_mode: if m.display_mode.is_empty() {
            cfg_display_mode
        } else {
            m.display_mode.clone()
        },
        date_format: m.date_format.clone(),
        axis_format: m.axis_format.clone(),
        tick_interval: m.tick_interval.clone(),
        top_axis: top_axis_enabled,
        today_marker: m.today_marker.clone(),
        categories,
        rows,
        section_titles,
        tasks,
        excludes: excludes_layout,
        has_excludes_layer,
        bottom_ticks,
        top_ticks,
        title,
        title_x: width / 2.0,
        title_y: title_top_margin,
    })
}

#[cfg(all(test, feature = "diagram-gantt"))]
mod tests {
    use super::{
        ElapsedTickState, ceil_elapsed_tick_start_with, ceil_tick_start, format_axis_tick_label,
        format_dayjs_like, layout_gantt_diagram_typed,
    };
    use crate::text::DeterministicTextMeasurer;
    use merman_core::diagrams::gantt::{GanttDiagramRenderModel, GanttRenderTask};
    use merman_core::time::{CivilDate, OffsetDateTime, UtcOffset};

    #[test]
    fn zero_section_styles_preserves_javascript_nan_class_suffix() {
        let categories = vec!["first".to_string(), "second".to_string()];

        assert_eq!(
            super::gantt_section_class_suffix("second", &categories, 0),
            "NaN"
        );
        assert_eq!(
            super::gantt_section_class_suffix("missing", &categories, 0),
            "0"
        );
    }

    #[test]
    fn maximum_utc_date_layout_terminates_without_panicking() {
        let max_ms = i64::MAX;
        let mut model = GanttDiagramRenderModel::default();
        model.date_format = "x".to_string();
        model.axis_format = "%Y-%m-%d".to_string();
        model.tick_interval = Some("1day".to_string());
        model.excludes = vec!["weekends".to_string()];
        model.weekend = "saturday".to_string();
        model.tasks.push(GanttRenderTask {
            id: "boundary".to_string(),
            task: "Boundary".to_string(),
            section: "Boundary".to_string(),
            task_type: "Boundary".to_string(),
            start_ms: max_ms,
            end_ms: max_ms,
            ..GanttRenderTask::default()
        });

        let utc = merman_core::time::LocalTimeZone::utc();
        let layout = layout_gantt_diagram_typed(
            &model,
            None,
            &serde_json::json!({}),
            &DeterministicTextMeasurer::default(),
            800.0,
            &utc,
        )
        .expect("maximum-date layout should terminate successfully");

        assert_eq!(layout.tasks.len(), 1);
        assert_eq!(layout.tasks[0].start_ms, max_ms);
        assert_eq!(layout.bottom_ticks.len(), 1);
    }

    #[test]
    fn axis_formatter_uses_d3_time_format_semantics() {
        let local = CivilDate::new(2026, 8, 3)
            .unwrap()
            .at_hms_milli(14, 5, 6, 7)
            .unwrap();
        let datetime = OffsetDateTime::from_local(
            local,
            UtcOffset::from_minutes(8 * 60).expect("valid offset"),
        )
        .unwrap();

        assert_eq!(
            format_axis_tick_label(
                datetime,
                "%a %A %b %B %d %e %f %H %I %j %L %m %M %p %q %S %u %U %V %w %W %x %X %y %Y %Z %%",
            ),
            "Mon Monday Aug August 03  3 007000 14 02 215 007 08 05 PM 3 06 1 31 32 1 31 8/3/2026 2:05:06 PM 26 2026 +0800 %"
        );
    }

    #[test]
    fn axis_formatter_preserves_d3_epoch_and_unknown_directive_rules() {
        let before_epoch = OffsetDateTime::from_unix_millis(-1, UtcOffset::UTC);
        assert_eq!(
            format_axis_tick_label(before_epoch, "%Q|%s|%L|%K|tail%"),
            "-1|-1|999|K|tail"
        );

        let wide = OffsetDateTime::from_local(
            CivilDate::new(10_000, 1, 1).unwrap().at_midnight(),
            UtcOffset::UTC,
        )
        .unwrap();
        assert_eq!(format_axis_tick_label(wide, "%Y"), "0000");
    }

    #[test]
    fn dayjs_unix_seconds_floor_negative_milliseconds() {
        let utc = merman_core::time::LocalTimeZone::utc();
        for (milliseconds, expected) in [
            (-1_001, "-2"),
            (-1_000, "-1"),
            (-999, "-1"),
            (-1, "-1"),
            (0, "0"),
            (999, "0"),
            (1_000, "1"),
        ] {
            assert_eq!(
                format_dayjs_like(milliseconds, "X", &utc).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn millisecond_tick_ceil_uses_euclidean_alignment_before_epoch() {
        let utc = merman_core::time::LocalTimeZone::utc();

        assert_eq!(ceil_tick_start(-1, 250, "millisecond", None, &utc), Some(0));
        assert_eq!(
            ceil_tick_start(-251, 250, "millisecond", None, &utc),
            Some(-250)
        );
        assert_eq!(
            ceil_tick_start(-250, 250, "millisecond", None, &utc),
            Some(-250)
        );
    }

    #[test]
    fn elapsed_hour_ceil_preserves_the_second_tick_in_a_fall_back_fold() {
        const FIRST_ONE_OCLOCK: i64 = 3_600_000;
        const FIRST_ONE_THIRTY: i64 = FIRST_ONE_OCLOCK + 1_800_000;
        const SECOND_ONE_OCLOCK: i64 = 7_200_000;

        let next =
            ceil_elapsed_tick_start_with(FIRST_ONE_THIRTY, 1, 3_600_000, |instant| match instant {
                FIRST_ONE_THIRTY => Some(ElapsedTickState {
                    lower_millis: 1_800_000,
                    field: 1,
                }),
                SECOND_ONE_OCLOCK => Some(ElapsedTickState {
                    lower_millis: 0,
                    field: 1,
                }),
                _ => None,
            });

        assert_eq!(next, Some(SECOND_ONE_OCLOCK));
    }
}

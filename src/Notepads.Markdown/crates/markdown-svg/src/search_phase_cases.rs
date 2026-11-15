// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual OwnedJob -> adapter -> shaping/projection searches; existing real font,
//! mocked Native plan/metrics/geometry capabilities, original resource account.
//! SAFETY: callbacks run synchronously on this fixture's thread. The engine owns
//! initialized ABI inputs and exact writable output capacities; FONT/GEOMETRY
//! retain returned arrays until their matching retirement and never expose them
//! to another thread. Every raw write stays inside the requested count.
use super::*;
use crate::search_work::phase::{self, Event as PhaseEvent, Stage};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Observe,
    Cancel,
    Work,
    Native,
}
#[derive(Default)]
struct Geometry {
    generation: u64,
    count: u32,
    prepared: usize,
    retired: usize,
}
thread_local! { static GEOMETRY: RefCell<Geometry> = RefCell::new(Geometry::default()); }

unsafe extern "C" fn plan(session: u64, draft: *const abi::Draft, output: *mut abi::Plan) -> i32 {
    let hr = unsafe { real_plan(session, draft, output) };
    if hr != 0 {
        return hr;
    }
    let draft = unsafe { &*draft };
    let spans = unsafe {
        core::slice::from_raw_parts(draft.logical_spans, draft.logical_span_count as usize)
    };
    let styles = unsafe { core::slice::from_raw_parts(draft.styles, draft.style_count as usize) };
    FONT.with_borrow_mut(|slot| {
        let fixture = slot.as_mut().unwrap();
        fixture.runs.clear();
        let mut style = 0;
        for span in spans {
            let mut start = span.utf8_start;
            let end = start + span.utf8_length;
            while start < end {
                while styles[style].utf8_start + styles[style].utf8_length <= start {
                    style += 1;
                }
                let next = end.min(styles[style].utf8_start + styles[style].utf8_length);
                fixture.runs.push(abi::LogicalRun {
                    utf8_start: start,
                    utf8_length: next - start,
                    face_ticket: 1,
                    script_be: span.script_be,
                    language_start: span.language_start,
                    language_length: span.language_length,
                    bidi_level: span.bidi_level,
                    orientation: span.orientation,
                    fallback_scale: 1.0,
                });
                start = next;
            }
        }
        unsafe {
            (*output).runs = fixture.runs.as_ptr();
            (*output).run_count = fixture.runs.len() as u32;
        }
    });
    0
}
unsafe extern "C" fn metrics(
    _: u64,
    _: u32,
    _: u32,
    output: *mut abi::GlyphMetrics,
    count: u32,
    written: *mut u32,
) -> i32 {
    for index in 0..count as usize {
        unsafe {
            output.add(index).write(abi::GlyphMetrics {
                advance_width: 500,
                advance_height: 1000,
                vertical_origin_y: 800,
                ..Default::default()
            });
        }
    }
    unsafe {
        written.write(count);
    }
    0
}
unsafe extern "C" fn prepare(
    _: u64,
    _: *const abi::GlyphKey,
    count: u32,
    output: *mut abi::GeometryReceipt,
) -> i32 {
    GEOMETRY.with_borrow_mut(|g| {
        assert_eq!(g.count, 0);
        assert!((1..=4096).contains(&count));
        g.generation += 1;
        g.prepared += 1;
        g.count = count;
        unsafe {
            output.write(abi::GeometryReceipt {
                size: size_of::<abi::GeometryReceipt>() as u32,
                version: abi::ABI_VERSION,
                generation: g.generation,
                glyph_count: count,
                command_count: count * 5,
                retained_native_bytes: 0,
            });
        }
    });
    0
}
unsafe extern "C" fn fill(
    _: u64,
    generation: u64,
    paths: *mut abi::GlyphPath,
    path_count: u32,
    commands: *mut abi::Command,
    command_count: u32,
    paths_written: *mut u32,
    commands_written: *mut u32,
) -> i32 {
    GEOMETRY.with_borrow(|g| {
        assert_eq!(generation, g.generation);
        assert_eq!(path_count, g.count);
    });
    assert_eq!(command_count, path_count * 5);
    for index in 0..path_count as usize {
        let rectangle = [
            abi::Command {
                kind: 0,
                ..Default::default()
            },
            abi::Command {
                kind: 1,
                x1: 500.0,
                ..Default::default()
            },
            abi::Command {
                kind: 1,
                x1: 500.0,
                y1: -700.0,
                ..Default::default()
            },
            abi::Command {
                kind: 1,
                y1: -700.0,
                ..Default::default()
            },
            abi::Command {
                kind: 3,
                ..Default::default()
            },
        ];
        unsafe {
            paths.add(index).write(abi::GlyphPath {
                key_index: index as u32,
                command_start: index as u32 * 5,
                command_count: 5,
                fill_rule: 0,
            });
            core::ptr::copy_nonoverlapping(rectangle.as_ptr(), commands.add(index * 5), 5);
        }
    }
    unsafe {
        paths_written.write(path_count);
        commands_written.write(command_count);
    }
    0
}
unsafe extern "C" fn retire(_: u64, generation: u64) -> i32 {
    GEOMETRY.with_borrow_mut(|g| {
        assert_eq!(generation, g.generation);
        assert_ne!(g.count, 0);
        g.count = 0;
        g.retired += 1;
    });
    0
}

struct Observation {
    stage: Stage,
    mode: Mode,
    ordinal: usize,
    entries: usize,
    armed: bool,
    active: bool,
    exited: bool,
    completed: bool,
    comparisons: usize,
    geometry_at_arm: usize,
    cancel: CancelHandle,
    host: Arc<TestHost>,
}
thread_local! { static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) }; }
fn hook(event: PhaseEvent, resources: &ResourcesRef) {
    OBSERVATION.with_borrow_mut(|slot| {
        let state = slot.as_mut().unwrap();
        match event {
            PhaseEvent::Enter(stage) if stage == state.stage => {
                state.entries += 1;
                if state.armed || state.entries != state.ordinal {
                    return;
                }
                state.armed = true;
                state.active = true;
                state.geometry_at_arm = GEOMETRY.with_borrow(|g| g.prepared);
                match state.mode {
                    Mode::Observe => {}
                    Mode::Cancel => state.cancel.cancel(),
                    Mode::Work => resources
                        .checkpoint(resources.limits().work - resources.statistics().work)
                        .unwrap(),
                    Mode::Native => state
                        .host
                        .checkpoint_hr
                        .store((-19071i32) as u32, Ordering::Relaxed),
                }
            }
            PhaseEvent::Exit(stage, count, completed) if stage == state.stage && state.active => {
                state.active = false;
                state.exited = true;
                state.completed = completed;
                state.comparisons = count;
            }
            _ => {}
        }
    });
}
struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        phase::install(None);
        OBSERVATION.with_borrow_mut(|slot| {
            slot.take();
        });
        FONT.with_borrow_mut(|slot| {
            slot.take();
        });
    }
}
fn numeric_text(codes: &[u32], positioned: bool) -> String {
    let mut output = String::from("<text y='30' font-family='fixture'>");
    for (index, code) in codes.iter().enumerate() {
        let x = if positioned {
            format!(" x='{}'", index + 1)
        } else {
            String::new()
        };
        output.push_str(&format!(
            "<tspan{x} font-size='{}'>&#x{code:X};</tspan>",
            16 + index % 2
        ));
    }
    output.push_str("</text>");
    output
}
fn unique_codes() -> Vec<u32> {
    use crate::grapheme_roles::{SourceGraphemeRole, source_role};
    use read_fonts::TableProvider;
    use unicode_script::UnicodeScript;
    FONT.with_borrow(|slot| {
        let fixture = slot.as_ref().unwrap();
        let font = harfrust::FontRef::from_index(&fixture.bytes, 0).unwrap();
        let cmap = font.cmap().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        (33..=65534)
            .filter(|&code| {
                let Some(ch) = char::from_u32(code) else {
                    return false;
                };
                !ch.is_whitespace()
                    && ch.script() != unicode_script::Script::Unknown
                    && source_role(ch) == SourceGraphemeRole::Anchor
                    && cmap
                        .map_codepoint(code)
                        .is_some_and(|gid| gid.to_u32() != 0 && seen.insert(gid.to_u32()))
            })
            .collect()
    })
}
fn exercise(stage: Stage, mode: Mode) {
    let mut callbacks = install_font();
    callbacks.plan = Some(plan);
    callbacks.fill_metrics = Some(metrics);
    callbacks.prepare_geometry = Some(prepare);
    callbacks.fill_geometry = Some(fill);
    callbacks.retire_geometry = Some(retire);
    let synthetic = matches!(stage, Stage::RetainedKey | Stage::PendingKey | Stage::OutlineLookup);
    if synthetic {
        FONT.with_borrow_mut(|slot| {
            let face = &mut slot.as_mut().unwrap().faces[0];
            face.simulations = 1;
            face.metrics.ascent = 800;
            face.metrics.descent = 200;
            face.metrics.cap_height = 700;
            face.metrics.x_height = 500;
            face.metrics.underline_position = -100;
            face.metrics.underline_thickness = 50;
            face.metrics.strike_position = 250;
            face.metrics.strike_thickness = 50;
        });
    }
    GEOMETRY.with_borrow_mut(|g| *g = Geometry::default());
    let codes = if synthetic {
        unique_codes()
    } else {
        vec![65; 320]
    };
    assert!(
        codes.len() >= 260,
        "actual fixture must supply enough distinct synthetic glyphs"
    );
    let positioned = matches!(stage, Stage::RelevantRuns | Stage::IncludedRuns);
    let mut body = numeric_text(&codes, positioned);
    if matches!(stage, Stage::RetainedKey | Stage::OutlineLookup) {
        body.push_str(&numeric_text(&vec![*codes.last().unwrap(); 16], false));
    }
    let source =
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='8000' height='200'>{body}</svg>");
    let ordinal = match stage {
        Stage::RunStyle | Stage::SourceSpan | Stage::StyleFace | Stage::PendingKey => 260,
        Stage::RetainedKey | Stage::OutlineLookup => codes.len() + 1,
        Stage::RelevantRuns | Stage::IncludedRuns => 1,
    };
    let host = Arc::new(TestHost::default());
    let mut job = OwnedJob::new(identity(), callbacks, host.clone(), Limits::default()).unwrap();
    OBSERVATION.with_borrow_mut(|slot| {
        *slot = Some(Observation {
            stage,
            mode,
            ordinal,
            entries: 0,
            armed: false,
            active: false,
            exited: false,
            completed: false,
            comparisons: 0,
            geometry_at_arm: 0,
            cancel: job.cancellation(),
            host: host.clone(),
        })
    });
    assert!(phase::install(Some(hook)).is_none());
    let installed = Installed;
    let result = job.prepare(source.as_bytes());
    phase::install(None);
    let observation = OBSERVATION.with_borrow_mut(|slot| slot.take().unwrap());
    assert!(
        observation.armed && observation.exited,
        "actual {stage:?} stage must run: {result:?}"
    );
    let expected = match mode {
        Mode::Observe => None,
        Mode::Cancel => Some(Error::new(FailureKind::Cancelled, LimitKind::None)),
        Mode::Work => Some(Error::new(FailureKind::Quota, LimitKind::Work)),
        Mode::Native => Some(Error::native(-19071)),
    };
    if let Some(expected) = expected {
        assert_eq!(
            result,
            Err(JobError::Failure(expected)),
            "{stage:?}/{mode:?}"
        );
        assert!(!observation.completed);
        assert!(
            observation.comparisons <= 256,
            "{stage:?}: {} comparisons after request rejection",
            observation.comparisons
        );
        assert_eq!(
            GEOMETRY.with_borrow(|g| g.prepared),
            observation.geometry_at_arm,
            "no later native geometry call"
        );
    } else {
        assert!(result.is_ok(), "{stage:?}: {result:?}");
        assert!(
            observation.completed && observation.comparisons >= 259,
            "{stage:?}: {} actual comparisons",
            observation.comparisons
        );
        if synthetic {
            assert!(GEOMETRY.with_borrow(|g| g.prepared) > 0);
        }
    }
    let retirement = job.finish_on_worker().unwrap();
    assert_eq!(retirement.first_fault, expected);
    assert_eq!(retirement.cleanup_status, CleanupStatus::Succeeded);
    assert!(host.live.lock().unwrap().is_empty());
    GEOMETRY.with_borrow(|g| {
        assert_eq!(g.count, 0);
        assert_eq!(g.prepared, g.retired);
    });
    println!(
        "OWNED_SEARCH stage={stage:?} mode={mode:?} comparisons={} exact_fault={expected:?} retired=true",
        observation.comparisons
    );
    drop(installed);
}

#[test]
fn actual_owned_job_search_phases_observe_cancel_work_and_original_backend_failure() {
    for stage in [
        Stage::RunStyle,
        Stage::SourceSpan,
        Stage::RelevantRuns,
        Stage::IncludedRuns,
        Stage::StyleFace,
        Stage::PendingKey,
        Stage::RetainedKey,
        Stage::OutlineLookup,
    ] {
        for mode in [Mode::Observe, Mode::Cancel, Mode::Work, Mode::Native] {
            exercise(stage, mode);
        }
    }
}

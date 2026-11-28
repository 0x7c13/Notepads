// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual Tree and renderer delegation tests for the geometry scope only.
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    reserves: AtomicUsize,
    commits: AtomicUsize,
    releases: AtomicUsize,
    checkpoints: AtomicUsize,
    fail_reserve: AtomicUsize,
    fail_commit: AtomicUsize,
    fail_release: AtomicUsize,
    fail_checkpoint: AtomicUsize,
    used: AtomicUsize,
    entries: Mutex<Vec<(usize, usize)>>,
}
struct Host(Arc<State>);
impl Backend for Host {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let call = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_reserve.load(Ordering::Relaxed) {
            return Err(Error::native(-911));
        }
        self.0.entries.lock().unwrap().push((call, bytes));
        self.0.used.fetch_add(bytes, Ordering::Relaxed);
        Ticket::new(1, call as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let call = self.0.commits.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(
            self.0
                .entries
                .lock()
                .unwrap()
                .contains(&(ticket.slot() as usize, bytes))
        );
        if call == self.0.fail_commit.load(Ordering::Relaxed) {
            return Err(Error::native(-912));
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let call = self.0.releases.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_release.load(Ordering::Relaxed) {
            return Err(Error::native(-913));
        }
        let mut entries = self.0.entries.lock().unwrap();
        let index = entries
            .iter()
            .position(|entry| entry.0 == ticket.slot() as usize)
            .unwrap();
        let (_, bytes) = entries.swap_remove(index);
        self.0.used.fetch_sub(bytes, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let call = self.0.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_checkpoint.load(Ordering::Relaxed) {
            return Err(Error::new(FailureKind::Cancelled, LimitKind::Work));
        }
        Ok(())
    }
}
fn request() -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        entries: Mutex::new(Vec::with_capacity(4096)),
        ..State::default()
    });
    let resources = ResourcesRef::new_prepaid(
        Host(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (resources, state)
}
fn scenes() -> Vec<(&'static str, String)> {
    let shapes = r##"<path d="M4 12C11 1 35 2 42 18Q40 42 8 38Z" fill="#439ac9"/><rect x="44" y="44" width="10" height="10" fill="#37ad52"/>"##;
    let stroke = r##"<path d="M5 8C12 1 41 3 43 19L15 41" fill="none" stroke="#b34061" stroke-width="3" stroke-linecap="round" stroke-linejoin="miter-clip" stroke-dasharray="3 2"/>"##;
    let mask = r##"<defs><mask id="m"><rect width="64" height="64" fill="white"/><circle cx="25" cy="25" r="12" fill="black"/></mask></defs><g mask="url(#m)"><path d="M5 5H47V47H5Z" fill="#67a5d8"/></g>"##;
    let clip = r##"<defs><clipPath id="c"><path d="M2 4H43V44H2Z"/></clipPath></defs><g clip-path="url(#c)"><rect x="8" y="9" width="43" height="43" fill="#dba640"/></g>"##;
    let isolate =
        format!(r##"<g opacity="0.72" transform="matrix(.95 .1 -.06 .9 2 1)">{shapes}</g>"##);
    let pattern = r##"<defs><pattern id="p" width="8" height="9" patternUnits="userSpaceOnUse"><rect width="6" height="8" fill="#8a65bd"/></pattern></defs><path d="M5 6H47V46H5Z" fill="url(#p)"/>"##;
    let filter = r##"<defs><filter id="f"><feGaussianBlur stdDeviation=".7"/><feOffset dx="1.5" dy="1.25"/></filter></defs><g filter="url(#f)"><rect x="8" y="8" width="29" height="27" fill="#b36e48"/></g>"##;
    let nested = r##"<image x="7" y="9" width="40" height="35" href="data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='20'%20height='20'%3E%3Cpath%20d='M2%202H18V18H2Z'%20fill='red'/%3E%3C/svg%3E"/>"##;
    [
        ("fill", shapes.to_owned()), ("stroke", stroke.to_owned()), ("mask", mask.to_owned()),
        ("clip", clip.to_owned()), ("isolate", isolate), ("pattern", pattern.to_owned()),
        ("filter", filter.to_owned()), ("nested-svg", nested.to_owned()),
    ].into_iter().map(|(name, body)| (name, format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">{body}<rect x="58" y="58" width="5" height="5" fill="lime"/></svg>"#))).collect()
}

#[test]
fn actual_controlled_parse_and_renderer_match_ordinary_full_pixels_for_eight_scenes() {
    for (name, svg) in scenes() {
        let ordinary = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut expected = tiny_skia::Pixmap::new(64, 64).unwrap();
        crate::render(
            &ordinary,
            tiny_skia::Transform::identity(),
            &mut expected.as_mut(),
        )
        .unwrap();
        assert!(expected.data().iter().any(|byte| *byte != 0), "{name}");

        let (resources, state) = request();
        let processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
        let mut options = usvg::Options::default();
        options.processing = processing.clone();
        let controlled = usvg::Tree::from_data_controlled(svg.as_bytes(), &options).unwrap();
        let mut actual = tiny_skia::Pixmap::new(64, 64).unwrap();
        crate::render_in_request(
            &controlled,
            tiny_skia::Transform::identity(),
            &mut actual.as_mut(),
        )
        .unwrap();
        assert_eq!(actual.data(), expected.data(), "{name}");
        assert_eq!(processing.counts().2, 0);
        assert_eq!(processing.counts().3, 0);
        drop((controlled, options, processing));
        assert_eq!(state.used.load(Ordering::Relaxed), 0, "{name}");
        resources.validate_retired().unwrap();
        println!(
            "RENDER_TREE_POSITIVE {name} fullBytes={}",
            actual.data().len()
        );
    }
}

#[derive(Default)]
struct Fault {
    reserve: usize,
    commit: usize,
    release: usize,
    checkpoint: usize,
}
struct Counts {
    reserve: usize,
    commit: usize,
    release: usize,
    checkpoint: usize,
}
fn render_trial(svg: &str, fault: Fault, expected: Option<Error>) -> Counts {
    let (resources, state) = request();
    let processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    let mut options = usvg::Options::default();
    options.processing = processing.clone();
    // Renderer faults start only after this actual same-request controlled
    // tree owns its geometry and style lists. An ordinary Tree's dash metadata
    // cannot be passed through the paid-list factory as though it were paid.
    let tree = usvg::Tree::from_data_controlled(svg.as_bytes(), &options).unwrap();
    let before = Counts {
        reserve: state.reserves.load(Ordering::Relaxed),
        commit: state.commits.load(Ordering::Relaxed),
        release: state.releases.load(Ordering::Relaxed),
        checkpoint: state.checkpoints.load(Ordering::Relaxed),
    };
    let source_bytes = state.used.load(Ordering::Relaxed);
    state.fail_reserve.store(
        if fault.reserve == 0 {
            0
        } else {
            before.reserve + fault.reserve
        },
        Ordering::Relaxed,
    );
    state.fail_commit.store(
        if fault.commit == 0 {
            0
        } else {
            before.commit + fault.commit
        },
        Ordering::Relaxed,
    );
    state.fail_release.store(
        if fault.release == 0 {
            0
        } else {
            before.release + fault.release
        },
        Ordering::Relaxed,
    );
    state.fail_checkpoint.store(
        if fault.checkpoint == 0 {
            0
        } else {
            before.checkpoint + fault.checkpoint
        },
        Ordering::Relaxed,
    );
    let mut pixels = tiny_skia::Pixmap::new(64, 64).unwrap();
    let result = crate::render_in_context(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
        processing.clone(),
    );
    assert_eq!(result.err().map(|error| error.typed_failure), expected);
    assert_eq!(processing.resource_failure(), expected);
    assert_eq!(processing.counts().2, 0);
    assert_eq!(processing.counts().3, 0);
    if expected.is_some() {
        assert_eq!(processing.counts().1, processing.counts_at_fault().1);
    }
    let rendered = Counts {
        reserve: state.reserves.load(Ordering::Relaxed) - before.reserve,
        commit: state.commits.load(Ordering::Relaxed) - before.commit,
        release: state.releases.load(Ordering::Relaxed) - before.release,
        checkpoint: state.checkpoints.load(Ordering::Relaxed) - before.checkpoint,
    };
    if fault.release == 0 {
        assert_eq!(state.used.load(Ordering::Relaxed), source_bytes);
    }
    drop((tree, options, processing));
    if fault.release == 0 {
        assert_eq!(state.used.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    } else {
        assert_eq!(resources.statistics().release_failures, 1);
        assert_eq!(resources.statistics().live_leases, 1);
        assert_eq!(state.entries.lock().unwrap().len(), 1);
        assert_eq!(resources.validate_retired(), Err(Error::native(-913)));
    }
    rendered
}

#[test]
fn real_renderer_geometry_allocation_and_commit_faults_stop_before_next_render_entry() {
    for (name, svg) in scenes() {
        let baseline = render_trial(&svg, Fault::default(), None);
        for at in 1..=baseline.reserve {
            render_trial(
                &svg,
                Fault {
                    reserve: at,
                    ..Fault::default()
                },
                Some(Error::native(-911)),
            );
        }
        for at in 1..=baseline.commit {
            render_trial(
                &svg,
                Fault {
                    commit: at,
                    ..Fault::default()
                },
                Some(Error::native(-912)),
            );
        }
        println!(
            "RENDER_TREE_ALLOC {name} reserve={} commit={}",
            baseline.reserve, baseline.commit
        );
    }
}

#[test]
fn real_renderer_checkpoint_and_release_faults_keep_original_typed_error_and_no_late_entry() {
    for (name, svg) in scenes() {
        let baseline = render_trial(&svg, Fault::default(), None);
        for at in 1..=baseline.checkpoint {
            render_trial(
                &svg,
                Fault {
                    checkpoint: at,
                    ..Fault::default()
                },
                Some(Error::new(FailureKind::Cancelled, LimitKind::Work)),
            );
        }
        for at in 1..=baseline.release {
            render_trial(
                &svg,
                Fault {
                    release: at,
                    ..Fault::default()
                },
                Some(Error::native(-913)),
            );
        }
        println!(
            "RENDER_TREE_RETIRE {name} checkpoint={} release={}",
            baseline.checkpoint, baseline.release
        );
    }
}

#[test]
fn paid_raster_wrappers_reject_existing_geometry_failure_without_touching_pixels() {
    for mode in 0..3 {
        let (resources, state) = request();
        let mut pixels = super::Raster::try_create(8, 8, Some(&resources)).unwrap();
        pixels.fill(tiny_skia::Color::from_rgba8(10, 90, 170, 255));
        let mut source = tiny_skia::Pixmap::new(2, 2).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(190, 30, 45, 255));
        let mask = super::RasterMask::try_new(8, 8, Some(&resources)).unwrap();
        let before = pixels.data().to_vec();
        resources.record(Error::native(-914));
        match mode {
            0 => pixels.fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, 8.0, 8.0).unwrap(),
                &tiny_skia::Paint::default(),
                tiny_skia::Transform::identity(),
                None,
            ),
            1 => pixels.draw_pixmap(
                0,
                0,
                source.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            ),
            _ => pixels.apply_mask(mask.as_ref()),
        }
        assert_eq!(pixels.data(), before.as_slice());
        assert_eq!(resources.failure(), Some(Error::native(-914)));
        drop((pixels, mask));
        assert_eq!(state.used.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual allocation/free observations use the parent's sole TLS System observer.
use super::*;
use crate::memory::{Context, RetainedPool};
use markdown_svg_budget::FirstFailureRef;

fn owners() -> (Arc<TestHost>, Context, ResourcesRef, Arc<RetainedPool>) {
    let host = Arc::new(TestHost::default());
    let context =
        Context::new(callbacks(), host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    (host, context, resources, retained)
}
fn arc_base<T>(owner: &Arc<T>) -> (usize, usize) {
    let (layout, offset) = std::alloc::Layout::new::<(usize, usize)>()
        .extend(std::alloc::Layout::new::<T>())
        .unwrap();
    (
        Arc::as_ptr(owner) as usize - offset,
        layout.pad_to_align().size(),
    )
}
fn sequence(events: &[Event], pointer: usize, bytes: usize) -> (usize, usize, usize, usize, usize) {
    let allocated = events
        .iter()
        .position(|e| e.tag == 1 && e.identity == pointer && e.bytes == bytes)
        .unwrap();
    let admitted = events
        .iter()
        .enumerate()
        .skip(allocated + 1)
        .find(|(_, e)| e.tag == 4 && e.bytes == bytes)
        .map(|(i, _)| i)
        .unwrap();
    let slot = events[admitted].identity;
    let reserved = events
        .iter()
        .position(|e| e.tag == 3 && e.identity == slot && e.bytes == bytes)
        .unwrap();
    let freed = events
        .iter()
        .enumerate()
        .skip(admitted + 1)
        .find(|(_, e)| e.tag == 2 && e.identity == pointer && e.bytes == bytes)
        .map(|(i, _)| i)
        .unwrap();
    let released = events
        .iter()
        .enumerate()
        .skip(freed + 1)
        .find(|(_, e)| e.tag == 5 && e.identity == slot)
        .map(|(i, _)| i)
        .unwrap();
    assert!(reserved < allocated && allocated < admitted && admitted < freed && freed < released);
    (reserved, allocated, admitted, freed, released)
}

#[repr(align(128))]
struct AlignedPayload(u8);

#[test]
fn replay_allowances_stay_pending_and_reject_the_first_fault_within_paid_slack() {
    let (host, context, resources, retained) = owners();
    let committed = host.committed.load(Ordering::Relaxed);
    let allowance = retained.retain(1).unwrap();
    let live = host.live.lock().unwrap().clone();
    assert_eq!(host.committed.load(Ordering::Relaxed), committed);
    assert!(
        live.values()
            .any(|(bytes, admitted)| *bytes == 64 * 1024 && !admitted)
    );
    // The second request fits the already reserved quantum, so no host reserve
    // callback can supply the missing first-failure barrier on its behalf.
    assert_eq!(
        context.record_failure(abi::Failure::Native(-778)),
        abi::Failure::Native(-778)
    );
    assert!(matches!(
        retained.retain(1),
        Err(abi::Failure::Native(-778))
    ));
    assert_eq!(*host.live.lock().unwrap(), live);
    drop(allowance);
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}

#[test]
fn shared_arc_last_strong_then_private_last_weak_physically_free_before_refund() {
    let (host, _context, resources, retained) = owners();
    start_events();
    let owner = retained.share(AlignedPayload(7)).unwrap();
    let (pointer, bytes) = arc_base(&owner);
    assert_eq!(bytes, 256);
    let alias = owner.clone();
    drop(owner);
    assert_eq!(alias.0, 7);
    drop(alias); // Payload is retired; the sole private Weak keeps its control block.
    assert!(
        host.live
            .lock()
            .unwrap()
            .values()
            .any(|(size, admitted)| *size == bytes && *admitted)
    );
    drop(retained);
    let events = stop_events();
    sequence(&events, pointer, bytes);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}

#[test]
fn shared_arc_commit_failure_frees_the_real_arc_before_pending_ticket_refund() {
    let (host, _context, resources, retained) = owners();
    // Pay the registry capacity first so the injected error applies to this Arc.
    drop(retained.share(0u8).unwrap());
    host.commit_hr.store((-778i32) as u32, Ordering::Relaxed);
    start_events();
    assert!(retained.share(AlignedPayload(8)).is_err());
    let events = stop_events();
    let allocated = events
        .iter()
        .position(|e| e.tag == 1 && e.bytes == 256)
        .unwrap();
    let admitted = events
        .iter()
        .enumerate()
        .skip(allocated + 1)
        .find(|(_, e)| e.tag == 4 && e.bytes == 256)
        .unwrap()
        .0;
    let slot = events[admitted].identity;
    let freed = events
        .iter()
        .enumerate()
        .skip(admitted + 1)
        .find(|(_, e)| e.tag == 2 && e.identity == events[allocated].identity)
        .unwrap()
        .0;
    let released = events
        .iter()
        .enumerate()
        .skip(freed + 1)
        .find(|(_, e)| e.tag == 5 && e.identity == slot)
        .unwrap()
        .0;
    assert!(allocated < admitted && admitted < freed && freed < released);
    assert_eq!(resources.failure().unwrap().native_hr, Some(-778));
    host.commit_hr.store(0, Ordering::Relaxed);
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}

#[test]
fn live_strong_owner_at_pool_retirement_is_quarantined_without_false_refund() {
    let (host, _context, resources, retained) = owners();
    let owner = retained.share(AlignedPayload(9)).unwrap();
    let (_, bytes) = arc_base(&owner);
    drop(retained);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
    assert!(
        host.live
            .lock()
            .unwrap()
            .values()
            .any(|(size, admitted)| *size == bytes && *admitted)
    );
    assert!(resources.validate_retired().is_err());
    drop(owner);
    // The original quarantined Weak/fee are intentionally preserved. Later
    // payload destruction does not invent a successful retirement receipt.
    assert!(
        host.live
            .lock()
            .unwrap()
            .values()
            .any(|(size, _)| *size == bytes)
    );
}

#[test]
fn exact_arc_variation_slices_cover_zero_and_nonzero_actual_allocations() {
    for axes in [
        vec![],
        vec![
            abi::Axis {
                tag_be: u32::from_be_bytes(*b"wght"),
                value: 650.0,
            },
            abi::Axis {
                tag_be: u32::from_be_bytes(*b"wdth"),
                value: 90.0,
            },
        ],
    ] {
        let (host, _context, resources, retained) = owners();
        start_events();
        let owner = retained.variations(&axes).unwrap();
        let (layout, offset) = std::alloc::Layout::new::<(usize, usize)>()
            .extend(std::alloc::Layout::array::<usvg::FontVariation>(axes.len()).unwrap())
            .unwrap();
        let pointer = Arc::as_ptr(&owner).cast::<u8>() as usize - offset;
        assert_eq!(owner.len(), axes.len());
        for (actual, wanted) in owner.iter().zip(&axes) {
            assert_eq!(actual.tag, wanted.tag_be.to_be_bytes());
            assert_eq!(actual.value.to_bits(), wanted.value.to_bits());
        }
        let alias = owner.clone();
        drop(owner);
        drop(alias);
        drop(retained);
        let events = stop_events();
        sequence(&events, pointer, layout.pad_to_align().size());
        assert!(host.live.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
}

#[test]
fn canonical_and_ordinary_contexts_check_exact_first_fault_owner_counts() {
    let host = Arc::new(TestHost::default());
    let original =
        Context::new(callbacks(), host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    original.validate_scope_retired().unwrap();
    let slot = FirstFailureRef::new_prepaid();
    let shared = Context::new_with_first_failure(
        callbacks(),
        host,
        Arc::new(AtomicBool::new(false)),
        slot.clone(),
    )
    .unwrap();
    shared.validate_scope_retired().unwrap();
    let third = slot.clone();
    assert!(shared.validate_scope_retired().is_err());
    drop(third);
    assert_eq!(slot.get().unwrap().kind, FailureKind::Invariant);
    // Restoring the exact owner count does not clear the immutable first fault.
    shared.validate_scope_retired().unwrap();
    drop(shared);
    slot.validate_unique().unwrap();
}

unsafe extern "C" fn geometry_plan(
    _: u64,
    _: *const abi::GlyphKey,
    count: u32,
    output: *mut abi::GeometryReceipt,
) -> i32 {
    if count != 1 {
        return -900;
    }
    unsafe {
        output.write(abi::GeometryReceipt {
            size: size_of::<abi::GeometryReceipt>() as u32,
            version: abi::ABI_VERSION,
            generation: 1,
            glyph_count: 1,
            command_count: 3,
            retained_native_bytes: 0,
        });
    }
    0
}
unsafe extern "C" fn geometry_fill(
    _: u64,
    _: u64,
    paths: *mut abi::GlyphPath,
    path_capacity: u32,
    commands: *mut abi::Command,
    command_capacity: u32,
    paths_written: *mut u32,
    commands_written: *mut u32,
) -> i32 {
    if path_capacity != 1 || command_capacity != 3 {
        return -901;
    }
    unsafe {
        paths.write(abi::GlyphPath {
            key_index: 0,
            command_start: 0,
            command_count: 3,
            fill_rule: 0,
        });
        commands.write(abi::Command {
            kind: 0,
            ..Default::default()
        });
        commands.add(1).write(abi::Command {
            kind: 1,
            x1: 2.0,
            y1: 2.0,
            ..Default::default()
        });
        commands.add(2).write(abi::Command {
            kind: 3,
            ..Default::default()
        });
        paths_written.write(1);
        commands_written.write(3);
    }
    0
}
unsafe extern "C" fn geometry_retire(_: u64, _: u64) -> i32 {
    0
}

#[test]
fn simulated_geometry_preserves_exact_two_upstream_vec_layouts_and_real_arc_retirement() {
    let host = Arc::new(TestHost::default());
    let mut fonts = callbacks();
    fonts.prepare_geometry = Some(geometry_plan);
    fonts.fill_geometry = Some(geometry_fill);
    fonts.retire_geometry = Some(geometry_retire);
    let context = Context::new(fonts, host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    start_events();
    let geometry = crate::geometry::OwnedGeometry::fetch(
        &context,
        &resources,
        &retained,
        &[abi::GlyphKey {
            face_ticket: 1,
            glyph_id: 1,
            orientation: 0,
            reserved: 0,
        }],
    )
    .unwrap();
    let path = geometry.paths[0].path.as_ref().unwrap().clone();
    let (pointer, bytes) = arc_base(&path);
    assert_eq!(path.verbs().len(), 3);
    assert_eq!(path.points().len(), 2);
    drop(geometry);
    drop(path);
    drop(retained);
    let events = stop_events();
    sequence(&events, pointer, bytes);
    for layout in [
        std::alloc::Layout::array::<tiny_skia_path::PathVerb>(3).unwrap(),
        std::alloc::Layout::array::<tiny_skia::Point>(9).unwrap(),
    ] {
        let allocations: Vec<_> = events
            .iter()
            .filter(|e| e.tag == 1 && e.bytes == layout.size())
            .collect();
        assert_eq!(
            allocations.len(),
            1,
            "actual upstream requested layout {}",
            layout.size()
        );
        sequence(&events, allocations[0].identity, layout.size());
    }
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
}

fn registration_plan(
    context: &Context,
    files: &[abi::File],
    faces: &[abi::Face],
    requests: &[(u32, u32)],
) -> crate::font::OwnedPlan {
    let runs: Vec<_> = requests
        .iter()
        .enumerate()
        .map(|(at, &(ticket, orientation))| abi::LogicalRun {
            utf8_start: at as u32,
            utf8_length: 1,
            face_ticket: ticket,
            script_be: u32::from_be_bytes(*b"Latn"),
            language_start: 0,
            language_length: 2,
            orientation,
            fallback_scale: 1.0,
            ..Default::default()
        })
        .collect();
    // SAFETY: each input slice remains readable for the entire synchronous copy.
    unsafe {
        crate::font::OwnedPlan {
            generation: 1,
            files: AdmittedSlice::copy_native(context, files.as_ptr(), files.len()).unwrap(),
            faces: AdmittedSlice::copy_native(context, faces.as_ptr(), faces.len()).unwrap(),
            axes: AdmittedSlice::copy_native(context, core::ptr::null(), 0).unwrap(),
            runs: AdmittedSlice::copy_native(context, runs.as_ptr(), runs.len()).unwrap(),
            strings: AdmittedSlice::copy_native(context, b"en".as_ptr(), 2).unwrap(),
        }
    }
}
fn registration_metadata() -> (abi::File, abi::Face) {
    FONT.with_borrow(|slot| {
        let fixture = slot.as_ref().unwrap();
        (fixture.files[0], fixture.faces[0])
    })
}
#[allow(unreachable_patterns)]
fn assert_same_binary(left: &fontdb::Database, right: &fontdb::Database, id: fontdb::ID) {
    match (
        &left.face(id).unwrap().source,
        &right.face(id).unwrap().source,
    ) {
        (fontdb::Source::Binary(left), fontdb::Source::Binary(right)) => {
            assert!(Arc::ptr_eq(left, right));
        }
        _ => panic!("Native fixture must remain Binary"),
    }
}

#[test]
fn repeated_registration_preserves_database_arc_and_allocates_no_metadata() {
    let host = Arc::new(TestHost::default());
    let context = Context::new(
        install_font(),
        host.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let (file, face) = registration_metadata();
    let plan = registration_plan(&context, &[file], &[face], &[(1, 0), (1, 0)]);
    let (fonts, database) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &plan, &bootstrap)
        .unwrap();
    let database = database.unwrap();
    assert!(database.is_funded());
    let id = fonts.id(1, 0).unwrap();
    let output = database.clone();
    let admissions = host.committed.load(Ordering::Relaxed);
    start_events();
    let (repeated, replacement) = fonts
        .stage_registration(&context, &retained, &plan, &database)
        .unwrap();
    let events = stop_events();
    assert!(replacement.is_none());
    assert!(Arc::ptr_eq(&database, &output));
    assert_eq!(repeated.id(1, 0).unwrap(), id);
    assert_eq!(host.committed.load(Ordering::Relaxed), admissions);
    assert!(events.iter().all(|event| event.tag != 1));
    let mut options = usvg::Options::default();
    options.fontdb = database.clone();
    start_events();
    assert!(matches!(
        options.try_fontdb_mut(),
        Err(fontdb::CompatCloneError::ControlledReadOnly)
    ));
    let events = stop_events();
    assert!(events.iter().all(|event| event.tag != 1));
    assert!(Arc::ptr_eq(&options.fontdb, &database));
    assert_eq!(database.len(), 1);
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 1);
    drop((options, repeated, fonts, plan, database, output, bootstrap));
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

#[test]
fn batch_registration_keeps_old_snapshot_ids_sources_and_physical_release_order() {
    let host = Arc::new(TestHost::default());
    let context = Context::new(
        install_font(),
        host.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let (file, face) = registration_metadata();
    let first = registration_plan(&context, &[file], &[face], &[(1, 0)]);
    let mut second_face = face;
    second_face.ticket = u32::MAX;
    let second = registration_plan(
        &context,
        &[file],
        &[face, second_face],
        &[(1, 0), (u32::MAX, 0), (1, 1), (u32::MAX, 0)],
    );
    start_events();
    let (fonts_v1, database_v1) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database_v1 = database_v1.unwrap();
    let original = fonts_v1.id(1, 0).unwrap();
    let original_name = database_v1.face(original).unwrap().families[0].0.clone();
    let (fonts_v2, database_v2) = fonts_v1
        .stage_registration(&context, &retained, &second, &database_v1)
        .unwrap();
    let database_v2 = database_v2.unwrap();
    assert!(!Arc::ptr_eq(&database_v1, &database_v2));
    assert_eq!(database_v1.len(), 1);
    assert_eq!(database_v2.len(), 3);
    assert_eq!(fonts_v2.id(1, 0).unwrap(), original);
    let other = fonts_v2.id(u32::MAX, 0).unwrap();
    let vertical = fonts_v2.id(1, 1).unwrap();
    assert_ne!(original, other);
    assert_ne!(original, vertical);
    assert_ne!(other, vertical);
    assert_eq!(
        database_v1.face(original).unwrap().families[0].0,
        original_name
    );
    assert!(database_v1.face(other).is_none());
    assert!(database_v1.face(vertical).is_none());
    assert_eq!(
        database_v2.face(other).unwrap().families[0].0,
        "actual-face-4294967295-0"
    );
    assert_same_binary(&database_v1, &database_v2, original);
    #[allow(unreachable_patterns)]
    match (
        &database_v2.face(original).unwrap().source,
        &database_v2.face(other).unwrap().source,
    ) {
        (fontdb::Source::Binary(left), fontdb::Source::Binary(right)) => {
            assert!(Arc::ptr_eq(left, right));
        }
        _ => panic!("Native fixture must remain Binary"),
    }
    let face_v1 = database_v1.face(original).unwrap();
    let face_v2 = database_v2.face(original).unwrap();
    let family_v1 = (
        face_v1.families[0].0.as_ptr() as usize,
        face_v1.families[0].0.len(),
    );
    let family_v2 = (
        face_v2.families[0].0.as_ptr() as usize,
        face_v2.families[0].0.len(),
    );
    assert_ne!(family_v1.0, family_v2.0);
    let families_v1 = (
        face_v1.families.as_ptr() as usize,
        face_v1.families.len() * size_of::<(String, fontdb::Language)>(),
    );
    let families_v2 = (
        face_v2.families.as_ptr() as usize,
        face_v2.families.len() * size_of::<(String, fontdb::Language)>(),
    );
    let database_allocation_v1 = arc_base(&database_v1);
    let database_allocation_v2 = arc_base(&database_v2);
    drop((
        fonts_v1,
        fonts_v2,
        database_v1,
        database_v2,
        first,
        second,
        bootstrap,
    ));
    // Payloads have retired; each last private Weak still holds its real Arc.
    assert!(
        host.live
            .lock()
            .unwrap()
            .values()
            .any(|(bytes, admitted)| *bytes == database_allocation_v2.1 && *admitted)
    );
    drop(retained);
    let events = stop_events();
    for (pointer, bytes) in [
        family_v1,
        family_v2,
        families_v1,
        families_v2,
        database_allocation_v1,
        database_allocation_v2,
    ] {
        sequence(&events, pointer, bytes);
    }
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 1);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

unsafe extern "C" fn identical_file_fill(
    session: u64,
    _: u32,
    output: *mut u8,
    capacity: u64,
    written: *mut u64,
) -> i32 {
    // Both test file identities contain the same immutable real TTF bytes.
    unsafe { real_fill(session, 1, output, capacity, written) }
}

#[test]
fn no_new_faces_still_register_all_files_and_reject_changed_file_identity() {
    let host = Arc::new(TestHost::default());
    let mut callbacks = install_font();
    callbacks.fill_font = Some(identical_file_fill);
    let context = Context::new(callbacks, host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let (file, face) = registration_metadata();
    let first = registration_plan(&context, &[file], &[face], &[(1, 0)]);
    let (fonts, database) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database = database.unwrap();
    let mut extra = file;
    extra.ticket = 2;
    let second = registration_plan(&context, &[file, extra], &[face], &[(1, 0)]);
    let (fonts_with_extra, replacement) = fonts
        .stage_registration(&context, &retained, &second, &database)
        .unwrap();
    assert!(replacement.is_none());
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 2);
    let (_, replacement) = fonts_with_extra
        .stage_registration(&context, &retained, &second, &database)
        .unwrap();
    assert!(replacement.is_none());
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 2);
    extra.sha256[0] ^= 1;
    let wrong = registration_plan(&context, &[file, extra], &[face], &[(1, 0)]);
    assert!(
        fonts_with_extra
            .stage_registration(&context, &retained, &wrong, &database)
            .is_err()
    );
    assert_eq!(database.len(), 1);
    assert_eq!(fonts.id(1, 0).unwrap(), database.faces().next().unwrap().id);
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 2);
    drop((
        fonts_with_extra,
        fonts,
        database,
        first,
        second,
        wrong,
        bootstrap,
    ));
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

#[derive(Default)]
struct RegistrationHost {
    inner: TestHost,
    admissions: AtomicU64,
    checkpoints: AtomicU64,
    fail_admission: AtomicU64,
    cancel_checkpoint: AtomicU64,
}
impl Host for RegistrationHost {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.inner.reserve(kind, bytes)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let at = self.admissions.fetch_add(1, Ordering::Relaxed) + 1;
        if at == self.fail_admission.load(Ordering::Relaxed) {
            self.inner
                .commit_hr
                .store((-778i32) as u32, Ordering::Relaxed);
        }
        let result = self.inner.allocation_committed(ticket, bytes);
        self.inner.commit_hr.store(0, Ordering::Relaxed);
        result
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.inner.release_after_free(ticket)
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let at = self.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        if at == self.cancel_checkpoint.load(Ordering::Relaxed) {
            return Err(Error::new(
                FailureKind::Cancelled,
                markdown_svg_budget::LimitKind::None,
            ));
        }
        self.inner.checkpoint(work)
    }
}

fn registration_failure_case(fail_admission: u64, cancel_checkpoint: u64) -> (u64, u64) {
    let host = Arc::new(RegistrationHost::default());
    let context = Context::new(
        install_font(),
        host.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let (file, face) = registration_metadata();
    let first = registration_plan(&context, &[file], &[face], &[(1, 0)]);
    let (fonts, database) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database = database.unwrap();
    let output = database.clone();
    let original = fonts.id(1, 0).unwrap();
    let original_family = database.face(original).unwrap().families[0].0.clone();
    let mut second_face = face;
    second_face.ticket = 2;
    let second = registration_plan(
        &context,
        &[file],
        &[face, second_face],
        &[(1, 0), (2, 0), (1, 1), (2, 0)],
    );
    let before_admission = host.admissions.load(Ordering::Relaxed);
    let before_checkpoint = host.checkpoints.load(Ordering::Relaxed);
    if fail_admission != 0 {
        host.fail_admission
            .store(before_admission + fail_admission, Ordering::Relaxed);
    }
    if cancel_checkpoint != 0 {
        host.cancel_checkpoint
            .store(before_checkpoint + cancel_checkpoint, Ordering::Relaxed);
    }
    start_events();
    let result = fonts.stage_registration(&context, &retained, &second, &database);
    if fail_admission != 0 || cancel_checkpoint != 0 {
        assert!(result.is_err());
        if fail_admission != 0 {
            assert_eq!(resources.failure(), Some(Error::native(-778)));
        } else {
            assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
        }
    } else {
        assert!(result.is_ok());
    }
    drop(result);
    let events = stop_events();
    let admissions = host.admissions.load(Ordering::Relaxed) - before_admission;
    let checkpoints = host.checkpoints.load(Ordering::Relaxed) - before_checkpoint;
    // The previous committed store and caller output remain usable at every
    // rejection, including cancellation at the final publication checkpoint.
    assert!(Arc::ptr_eq(&database, &output));
    assert_eq!(database.len(), 1);
    assert_eq!(
        database.face(original).unwrap().families[0].0,
        original_family
    );
    assert_eq!(fonts.id(1, 0).unwrap(), original);
    assert!(fonts.id(2, 0).is_err());
    assert!(fonts.id(1, 1).is_err());
    if fail_admission != 0 {
        let (at, failed) = events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.tag == 4)
            .nth(fail_admission as usize - 1)
            .unwrap();
        let allocated = events[..at]
            .iter()
            .rev()
            .find(|event| event.tag == 1 && event.bytes == failed.bytes)
            .unwrap();
        sequence(&events, allocated.identity, allocated.bytes);
    }
    drop((fonts, database, output, first, second, bootstrap));
    drop(retained);
    assert!(host.inner.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
    (admissions, checkpoints)
}

#[test]
fn every_registration_admission_and_checkpoint_failure_preserves_previous_version() {
    let (admissions, checkpoints) = registration_failure_case(0, 0);
    assert!(
        admissions > 10,
        "exercise copied strings, families, slots, fee table and Arc"
    );
    assert!(checkpoints > 5, "include the final commit checkpoint");
    for at in 1..=admissions {
        registration_failure_case(at, 0);
    }
    for at in 1..=checkpoints {
        registration_failure_case(0, at);
    }
}

#[test]
fn specified_ttc_index_is_validated_without_changing_the_previous_registration() {
    let host = Arc::new(TestHost::default());
    let context = Context::new(
        install_font(),
        host.clone(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let (file, face) = registration_metadata();
    let first = registration_plan(&context, &[file], &[face], &[(1, 0)]);
    let (fonts, database) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database = database.unwrap();
    let original = fonts.id(1, 0).unwrap();
    let mut nonexistent = face;
    nonexistent.ticket = 2;
    nonexistent.face_index = 1; // A real single-face TTF cannot supply TTC face 1.
    let wrong = registration_plan(&context, &[file], &[nonexistent], &[(2, 0)]);
    assert!(
        fonts
            .stage_registration(&context, &retained, &wrong, &database)
            .is_err()
    );
    assert_eq!(database.len(), 1);
    assert_eq!(fonts.id(1, 0).unwrap(), original);
    assert!(fonts.id(2, 0).is_err());
    drop((fonts, database, first, wrong, bootstrap));
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

fn install_two_face_collection() -> (abi::Callbacks, abi::File, abi::Face, abi::Face) {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/svg-fonts/Notepads-Fixture-RegularBold.ttc"),
    )
    .unwrap();
    assert_eq!(bytes.len(), 105208);
    let file = abi::File {
        ticket: 1,
        collection_faces: 2,
        byte_length: 105208,
        sha256: core::array::from_fn(|i| {
            u8::from_str_radix(
                &"0EC59A653501E5DFFF4D8C6046C4DB0D9B0A824CC17A6906094F4A9A60BD4D63"
                    [i * 2..i * 2 + 2],
                16,
            )
            .unwrap()
        }),
    };
    // Independent expectations from the two original TTFs are recorded in
    // tests/fixtures/svg-fonts/PROVENANCE-V1.json. Do not derive these values
    // from the collection under test and then compare them with themselves.
    let bold = abi::Face {
        ticket: 11,
        file_ticket: 1,
        face_index: 1,
        glyph_count: 280,
        units_per_em: 1000,
        metrics: abi::GlobalMetrics {
            units_per_em: 1000,
            ..Default::default()
        },
        ..Default::default()
    };
    let regular = abi::Face {
        ticket: 12,
        face_index: 0,
        glyph_count: 286,
        ..bold
    };
    FONT.with_borrow_mut(|slot| {
        *slot = Some(FontFixture {
            bytes,
            files: [file],
            faces: [bold],
            runs: Vec::new(),
            strings: Vec::new(),
            generation: 0,
            fills: 0,
        });
    });
    let mut callbacks = callbacks();
    callbacks.fill_font = Some(real_fill);
    (callbacks, file, bold, regular)
}

#[test]
fn real_two_face_ttc_selects_index_one_first_and_shares_one_file_across_versions() {
    use read_fonts::TableProvider;
    let host = Arc::new(TestHost::default());
    let (callbacks, file, bold, regular) = install_two_face_collection();
    let context = Context::new(callbacks, host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let first = registration_plan(&context, &[file], &[bold], &[(11, 0)]);
    let second = registration_plan(&context, &[file], &[bold, regular], &[(11, 0), (12, 0)]);
    start_events();
    let (fonts_v1, database_v1) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database_v1 = database_v1.unwrap();
    let bold_id = fonts_v1.id(11, 0).unwrap();
    assert_eq!(
        database_v1.len(),
        1,
        "only the selected TTC face is registered"
    );
    assert_eq!(database_v1.face(bold_id).unwrap().index, 1);
    let (bold_bytes, bold_index) = fonts_v1.bytes(bold_id).unwrap();
    assert_eq!(bold_index, 1);
    let bold_font =
        harfrust::FontRef::from_index(bold_bytes.as_ref().as_ref(), bold_index).unwrap();
    assert_eq!(bold_font.maxp().unwrap().num_glyphs(), 280);
    assert_eq!(bold_font.head().unwrap().units_per_em(), 1000);
    let (fonts_v2, database_v2) = fonts_v1
        .stage_registration(&context, &retained, &second, &database_v1)
        .unwrap();
    let database_v2 = database_v2.unwrap();
    let regular_id = fonts_v2.id(12, 0).unwrap();
    assert_ne!(bold_id, regular_id);
    assert_eq!(database_v2.len(), 2);
    assert_eq!(fonts_v2.id(11, 0).unwrap(), bold_id);
    assert_eq!(database_v2.face(bold_id).unwrap().index, 1);
    assert_eq!(database_v2.face(regular_id).unwrap().index, 0);
    assert_eq!(database_v1.len(), 1);
    assert!(database_v1.face(regular_id).is_none());
    assert!(fonts_v1.id(12, 0).is_err());
    assert_same_binary(&database_v1, &database_v2, bold_id);
    let (regular_bytes, regular_index) = fonts_v2.bytes(regular_id).unwrap();
    assert_eq!(regular_index, 0);
    assert!(Arc::ptr_eq(&bold_bytes, &regular_bytes));
    let regular_font =
        harfrust::FontRef::from_index(regular_bytes.as_ref().as_ref(), regular_index).unwrap();
    assert_eq!(regular_font.maxp().unwrap().num_glyphs(), 286);
    assert_eq!(regular_font.head().unwrap().units_per_em(), 1000);
    #[allow(unreachable_patterns)]
    match (
        &database_v2.face(bold_id).unwrap().source,
        &database_v2.face(regular_id).unwrap().source,
    ) {
        (fontdb::Source::Binary(left), fontdb::Source::Binary(right)) => {
            assert!(Arc::ptr_eq(left, right));
        }
        _ => panic!("TTC fixture must remain one Binary source"),
    }
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 1);
    let payload = bold_bytes.as_ref().as_ref().as_ptr() as usize;
    // FontRef values borrow these owners and are no longer used below.
    drop((
        bold_bytes,
        regular_bytes,
        fonts_v1,
        fonts_v2,
        database_v1,
        database_v2,
        first,
        second,
        bootstrap,
    ));
    drop(retained);
    let events = stop_events();
    assert!(
        events.len() < 4096,
        "the TLS event window must not truncate"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.tag == 1 && event.bytes == 105208)
            .count(),
        1
    );
    sequence(&events, payload, 105208);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

#[test]
fn real_two_face_ttc_rejects_index_two_without_changing_the_committed_index_one() {
    let host = Arc::new(TestHost::default());
    let (callbacks, file, bold, _) = install_two_face_collection();
    let context = Context::new(callbacks, host.clone(), Arc::new(AtomicBool::new(false))).unwrap();
    let resources = context.make_resources(Limits::default());
    let retained = RetainedPool::new(&context, &resources).unwrap();
    let bootstrap = Arc::new(fontdb::Database::new());
    let first = registration_plan(&context, &[file], &[bold], &[(11, 0)]);
    let (fonts, database) = crate::font::FontStore::new(&resources)
        .stage_registration(&context, &retained, &first, &bootstrap)
        .unwrap();
    let database = database.unwrap();
    let output = database.clone();
    let bold_id = fonts.id(11, 0).unwrap();
    let (bytes, index) = fonts.bytes(bold_id).unwrap();
    assert_eq!(index, 1);
    assert!(harfrust::FontRef::from_index(bytes.as_ref().as_ref(), 2).is_err());
    let nonexistent = abi::Face {
        ticket: 13,
        face_index: 2,
        ..bold
    };
    let wrong = registration_plan(&context, &[file], &[nonexistent], &[(13, 0)]);
    let admissions = host.committed.load(Ordering::Relaxed);
    start_events();
    assert!(
        fonts
            .stage_registration(&context, &retained, &wrong, &database)
            .is_err()
    );
    let events = stop_events();
    assert!(
        events.len() < 4096,
        "the TLS event window must not truncate"
    );
    assert!(events.iter().all(|event| event.tag != 1));
    assert_eq!(host.committed.load(Ordering::Relaxed), admissions);
    assert!(Arc::ptr_eq(&database, &output));
    assert_eq!(database.len(), 1);
    assert_eq!(database.face(bold_id).unwrap().index, 1);
    assert_eq!(fonts.id(11, 0).unwrap(), bold_id);
    assert!(fonts.id(13, 0).is_err());
    assert_eq!(FONT.with_borrow(|slot| slot.as_ref().unwrap().fills), 1);
    drop((bytes, fonts, database, output, first, wrong, bootstrap));
    drop(retained);
    assert!(host.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    context.validate_released().unwrap();
    FONT.with_borrow_mut(|slot| *slot = None);
}

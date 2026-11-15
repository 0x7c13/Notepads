// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#[test]
fn ownership_union_parent_layout_and_page_widths() {
    use std::mem::{align_of, size_of};

    fn emit<T>(width: Option<usize>) {
        println!(
            "UNIFIED_NATIVE_LAYOUT\t{}\t{}\t{}\t{width:?}",
            std::any::type_name::<T>(),
            size_of::<T>(),
            align_of::<T>(),
        );
    }
    fn rows<T>() {
        emit::<T>(Some(rows::NativeRows::<T, ChargedLease>::width()));
        println!(
            "UNIFIED_NATIVE_DESCRIPTOR\t{}\t{}",
            std::any::type_name::<T>(),
            rows::NativeRows::<T, ChargedLease>::descriptor_bytes(),
        );
    }

    // Existing x64 Release baseline, not a newly inferred old layout:
    // quota-cut-investigation-v1/final-layout-release-v1.log.
    println!(
        "UNIFIED_NATIVE_LAYOUT_BASELINE\t170a4173b225d1899cc72799241a5cb8f2de8397c3e2c0afa2b9ddbf239e7e86"
    );
    println!(
        "UNIFIED_NATIVE_TARGET\t{}\t{}",
        std::env::consts::ARCH,
        usize::BITS,
    );
    rows::<mapped::MappedValue>();
    rows::<mapped::BufferMapping>();
    rows::<mapped::PayloadBinding>();
    rows::<mapped::Proof>();
    emit::<crate::nodes::BlockLines>(None);
    emit::<crate::nodes::Ast>(None);
    emit::<crate::nodes::AstNode<'static>>(None);
    emit::<crate::parser::Spx>(None);
    emit::<small_value::SmallValue<mapped::Atom>>(None);
    emit::<small_value::SmallValue<mapped::BufferPiece>>(None);
    emit::<smallvec::SmallVec<[usize; 4]>>(None);
    emit::<smallvec::SmallVec<[(crate::nodes::Sourcepos, usize); 4]>>(None);
    emit::<ChargedLease>(None);
    println!(
        "UNIFIED_NATIVE_LAYOUT_UNAVAILABLE\tparser::inlines::Subject/Bracket\tprivate module; no production export added"
    );

    // Require real parent shrinkage, but print the actual sizes rather than
    // treating a guessed Rust layout as a measured result. Other targets do
    // not inherit the x64 baseline. ChargedLease is this vendor test's owner,
    // not proof that the core's NativeLease has the same target layout.
    if usize::BITS == 64 {
        assert!(size_of::<mapped::MappedValue>() < 112);
        assert!(size_of::<mapped::BufferMapping>() < 104);
        assert!(size_of::<mapped::PayloadBinding>() < 200);
        assert!(size_of::<mapped::Proof>() < 192);
    }
    #[cfg(all(target_pointer_width = "64", not(feature = "attributes")))]
    {
        // BlockLines is boxed: shrinking its offsets cannot shrink every AST
        // arena slot. Preserve the upstream/current actual AST size contract.
        assert_eq!(size_of::<crate::nodes::Ast>(), 88);
        assert_eq!(size_of::<crate::nodes::AstNode<'static>>(), 136);
    }
}

#[test]
fn ownership_union_parent_rows_cross_real_pages_and_directory_growth() {
    use mapped::*;

    fn check<T>(make: impl Fn(usize, &mut MemoryHost) -> T, verify: impl Fn(&T, usize)) {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        let width = rows::NativeRows::<T, ChargedLease>::width();
        assert!(width > 1);
        {
            let mut rows = rows::NativeRows::<T, ChargedLease>::default();
            // Five real pages also grow the original four-entry directory.
            // Element sizes come from the actual parent, not a proxy padding
            // record or the previous layout's page width.
            for index in 0..(4 * width + 1) {
                let value = make(index, &mut host);
                rows.push(value, &mut host).unwrap();
                assert_eq!(rows.len(), index + 1);
                if index.is_multiple_of(width) || (index + 1).is_multiple_of(width) {
                    for previous in 0..=index {
                        let value = rows
                            .get(previous, |bytes| rows::charge_host(&mut host, bytes))
                            .unwrap()
                            .unwrap();
                        verify(value, previous);
                    }
                    assert!(
                        rows.get(index + 1, |bytes| rows::charge_host(&mut host, bytes))
                            .unwrap()
                            .is_none()
                    );
                }
            }
            // Original permits: five data pages and two directory owners
            // admitted in succession. No additional backing lives in these
            // empty/inline nested values, so these counts identify the real rows.
            assert_eq!(host.reserve_calls, 7);
            assert_eq!(host.actual_calls, 7);
            assert!(gauge.borrow().current > 0);
        }
        assert_eq!(gauge.borrow().current, 0);
    }

    check(
        |index, host| {
            let mut ctx = Context::new(host, index + 1);
            MappedValue::raw(
                RSpan {
                    buffer: BufferId::from_raw(1).unwrap(),
                    start: index,
                    end: index + 1,
                },
                &mut ctx.mapping_access(),
            )
            .unwrap()
        },
        |value, index| {
            assert_eq!(value.bytes, 1);
            assert_eq!(value.atoms.len(), 1);
            assert_eq!(value.atoms[0].source.start, index);
        },
    );
    check(
        |index, _| BufferMapping {
            owner: NativeNodeId::from_raw(1).unwrap(),
            len: index,
            sealed: index.is_multiple_of(2),
            pieces: small_value::SmallValue::new(),
        },
        |value, index| {
            assert_eq!(value.len, index);
            assert_eq!(value.sealed, index.is_multiple_of(2));
        },
    );
    let raw = RSpan {
        buffer: BufferId::from_raw(1).unwrap(),
        start: 0,
        end: 0,
    };
    check(
        |index, _| PayloadBinding {
            role: PayloadKind::Destination,
            prefix_kind: GeneratedPrefix::Http,
            raw,
            value: MappedValue::default(),
            stage: 0,
            stage_len: index,
            prefix: None,
        },
        |value, index| assert_eq!(value.stage_len, index),
    );
    check(
        |index, _| Proof {
            kind: TransformKind::SoftBreakToSpace,
            stage: 0,
            host: HostSnapshot {
                source: ClaimSource::R(raw),
                utf8_len: 0,
                consumed_start: 0,
                consumed_end: 0,
                payload_role: None,
                role: HostRole::Inline,
                phase: NativePhase::Inline,
            },
            input: small_value::SmallValue::new(),
            local_start: index,
            local_end: index,
            result: String::new(),
        },
        |value, index| {
            assert_eq!(value.local_start, index);
            assert_eq!(value.local_end, index);
        },
    );
}

#[test]
fn ownership_inline_one_spill_growth_and_empty_backing_keep_exact_permit() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 0);
        let mut values = small_value::SmallValue::<u64>::new();
        values.push(7, &mut ctx.mapping_access()).unwrap();
        assert_eq!(ctx.host.reserve_calls, 0);
        assert_eq!(gauge.borrow().current, 0);
        for n in 1..19 {
            values.push(n, &mut ctx.mapping_access()).unwrap();
        }
        assert_eq!(values.len(), 19);
        assert_eq!(values[0], 7);
        assert_eq!(ctx.registry.test_counts().0, 1);
        // Truncation retains physical capacity, which must still be marked.
        values.truncate(0);
        ctx.registry.begin_mark(ctx.host).unwrap();
        values.mark(&mut ctx.registry, ctx.host).unwrap();
        ctx.registry.finish_mark(ctx.host).unwrap();
        ctx.registry.retire(ctx.host).unwrap();
        assert_eq!(ctx.registry.test_counts().0, 1);
        assert_eq!(ctx.registry.test_counts().1, 0);
        drop(values); // real backing precedes Context's original permit
        assert!(gauge.borrow().current > 0);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_slot_reuse_rejects_old_generation_even_with_new_pointer() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut registry = allocation::Registry::default();
        registry.prepare(false, &mut host).unwrap();
        let first = Box::new(11_u64);
        let pointer = std::ptr::from_ref(&*first) as usize;
        let lease = host.reserve(8).unwrap();
        let old = registry.retain(pointer, lease, false, &mut host).unwrap();
        drop(first);
        registry.release(old, pointer, &mut host).unwrap();
        registry.prepare(false, &mut host).unwrap();
        let second = Box::new(23_u64);
        let pointer = std::ptr::from_ref(&*second) as usize;
        let lease = host.reserve(8).unwrap();
        let new = registry.retain(pointer, lease, false, &mut host).unwrap();
        assert_ne!(old, new);
        assert_eq!(registry.test_counts().0, 1);
        registry.begin_mark(&mut host).unwrap();
        let error = registry.mark(old, pointer, &mut host).unwrap_err();
        host.assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        assert_eq!(*second, 23);
        drop(second);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_unique_index_rejects_duplicate_pointer_without_releasing_original() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 0);
        let value = ctx.string("live owner").unwrap();
        let pointer = value.as_ptr() as usize;
        ctx.ensure_slot().unwrap();
        let duplicate = ctx.host.reserve(1).unwrap();
        let before = gauge.borrow().current;
        let error = ctx.retain(pointer, duplicate).unwrap_err();
        assert_eq!(gauge.borrow().current, before);
        assert_eq!(ctx.registry.test_counts().0, 1);
        assert_eq!(value, "live owner");
        ctx.host
            .assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        drop(value);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_retirement_marks_empty_retained_strings_and_releases_only_dead_owners() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 0);
        let dead = ctx.string("AST-only backing").unwrap();
        let mut key = ctx.string("empty retained key capacity").unwrap();
        key.clear();
        let url = ctx.string("url").unwrap();
        let mut title = ctx.string("empty retained title capacity").unwrap();
        title.clear();
        let id = DefinitionId::from_raw(1).unwrap();
        let buffer = BufferId::from_raw(1).unwrap();
        let span = RSpan {
            buffer,
            start: 0,
            end: 1,
        };
        // The AST/reference slots now retain identities. Preserve the same
        // actual URL/title String allocations as proof-result owners instead;
        // this is a retained-heap fixture, not a grammar/provenance oracle.
        let url_payload = PayloadId::from_raw(1).unwrap();
        let title_payload = PayloadId::from_raw(2).unwrap();
        for (payload, role, result) in [
            (url_payload, PayloadKind::Destination, url),
            (title_payload, PayloadKind::Title, title),
        ] {
            let proof = ctx.mappings.proofs.len();
            let bytes = result.len();
            ctx.mappings
                .proofs
                .push(
                    mapped::Proof {
                        kind: mapped::TransformKind::Payload(StageKind::Entity),
                        stage: 1,
                        host: mapped::HostSnapshot {
                            source: mapped::ClaimSource::R(span),
                            utf8_len: 1,
                            consumed_start: 0,
                            consumed_end: 1,
                            payload_role: Some(role),
                            role: mapped::HostRole::Inline,
                            phase: mapped::NativePhase::Inline,
                        },
                        input: small_value::SmallValue::new(),
                        local_start: 0,
                        local_end: 1,
                        result,
                    },
                    ctx.host,
                )
                .unwrap();
            let mut value = mapped::MappedValue::default();
            value.bytes = bytes;
            value
                .atoms
                .push(
                    mapped::Atom {
                        source: span,
                        bytes,
                        proof: Some(proof),
                        hidden_syntax: false,
                    },
                    &mut ctx.mapping_access(),
                )
                .unwrap();
            assert_eq!(payload.get() as usize, ctx.mappings.payloads.len() + 1);
            ctx.mappings
                .payloads
                .push(
                    mapped::PayloadBinding {
                        role,
                        raw: span,
                        value,
                        stage: 1,
                        stage_len: bytes,
                        prefix: None,
                        prefix_kind: GeneratedPrefix::Http,
                    },
                    ctx.host,
                )
                .unwrap();
        }
        ctx.definitions
            .push(
                definitions::DefinitionRecord {
                    metadata: definitions::PendingDefinition {
                        id,
                        syntax: span,
                        label: span,
                    },
                    key,
                    value: crate::parser::ResolvedReference {
                        url: value::ValueText::accepted(value::AcceptedValueRef::Payload {
                            payload: url_payload,
                            role: PayloadKind::Destination,
                        }),
                        title: value::ValueText::accepted(value::AcceptedValueRef::Payload {
                            payload: title_payload,
                            role: PayloadKind::Title,
                        }),
                        native: Some(ReferenceIdentity {
                            definition: id,
                            destination: url_payload,
                            title: Some(title_payload),
                        }),
                    },
                    winner: false,
                },
                ctx.host,
            )
            .unwrap();
        assert_eq!(ctx.registry.test_counts().0, 4);
        let before = gauge.borrow().current;
        ctx.prepare_ast_retirement().unwrap();
        assert_eq!(gauge.borrow().current, before); // marking retires nothing
        drop(dead); // driver obligation: actual AST-only buffer first
        ctx.retire_ast().unwrap();
        assert_eq!(ctx.registry.test_counts(), (3, 0, None));
        assert!(gauge.borrow().current < before);
        let error = ctx.string("must not allocate after seal").unwrap_err();
        ctx.host
            .assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_mark_fault_retains_all_permits_and_stops_retirement() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 0);
        let value = ctx.string("still owned").unwrap();
        ctx.registry.begin_mark(ctx.host).unwrap();
        let before = gauge.borrow().current;
        ctx.host.event_fault_at = Some(ctx.host.event_calls + 1);
        let error = ctx.registry.mark_string(&value, ctx.host).unwrap_err();
        assert_eq!(gauge.borrow().current, before);
        ctx.host.assert_original(&error, Fault::Work);
        assert!(ctx.retire_ast().is_err());
        drop(value);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_box_actual_success_then_unwind_drops_buffer_before_permit() {
    struct Forward {
        inner: MemoryHost,
        armed: bool,
    }
    impl Host for Forward {
        type Error = Arc<Fault>;
        type Lease = ChargedLease;
        fn work(&mut self, d: WorkDelta) -> Result<(), Self::Error> {
            self.inner.work(d)
        }
        fn admit(&mut self, d: NativeAdmission) -> Result<(), Self::Error> {
            self.inner.admit(d)
        }
        fn reserve(&mut self, n: u64) -> Result<Self::Lease, Self::Error> {
            self.inner.reserve(n)
        }
        fn admit_actual(&mut self, l: &mut Self::Lease, n: u64) -> Result<(), Self::Error> {
            self.inner.admit_actual(l, n)?;
            assert!(!self.armed, "forwarding Host after original actual success");
            Ok(())
        }
        fn fact(&mut self, f: AcceptedFact<'_>) -> Result<(), Self::Error> {
            self.inner.fact(f)
        }
        fn reject(&mut self, v: NativeViolation) -> Self::Error {
            self.inner.reject(v)
        }
    }
    struct BufferWitness {
        gauge: Rc<RefCell<Gauge>>,
        minimum: u64,
        dropped: Rc<std::cell::Cell<bool>>,
    }
    impl Drop for BufferWitness {
        fn drop(&mut self) {
            assert!(self.gauge.borrow().current >= self.minimum);
            self.dropped.set(true);
        }
    }
    let mut host = Forward {
        inner: MemoryHost::new(),
        armed: false,
    };
    let gauge = host.inner.gauge.clone();
    let dropped = Rc::new(std::cell::Cell::new(false));
    {
        let mut ctx = Context::new(&mut host, 0);
        ctx.ensure_slot().unwrap();
        let minimum = gauge.borrow().current + std::mem::size_of::<BufferWitness>() as u64;
        ctx.host.armed = true;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = ctx.boxed(BufferWitness {
                gauge: gauge.clone(),
                minimum,
                dropped: dropped.clone(),
            });
        }));
        assert!(caught.is_err());
        assert!(dropped.get());
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_small_spill_reserve_and_actual_rejections_preserve_inline_value() {
    for actual in [false, true] {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        {
            let mut ctx = Context::new(&mut host, 0);
            let mut value = small_value::SmallValue::new();
            value.push(91_u64, &mut ctx.mapping_access()).unwrap();
            ctx.registry.prepare(false, ctx.host).unwrap();
            if actual {
                ctx.host.actual_fault_at = Some(ctx.host.actual_calls + 1);
            } else {
                ctx.host.reserve_fault_at = Some(ctx.host.reserve_calls + 1);
            }
            let error = value.push(92, &mut ctx.mapping_access()).unwrap_err();
            assert_eq!(&value[..], &[91]);
            assert_eq!(ctx.registry.test_counts().0, 0);
            ctx.host.assert_original(
                &error,
                if actual {
                    Fault::Actual
                } else {
                    Fault::Reserve
                },
            );
        }
        assert_eq!(gauge.borrow().current, 0);
    }
}

#[test]
fn ownership_retained_heap_enumeration_covers_all_mapped_nested_fields() {
    use mapped::*;
    fn map(ctx: &mut Context<'_, MemoryHost>, raw: RSpan) -> MappedValue {
        let mut value = MappedValue::raw(raw, &mut ctx.mapping_access()).unwrap();
        let second = MappedValue::raw(raw, &mut ctx.mapping_access()).unwrap();
        value
            .atoms
            .push(second.atoms[0], &mut ctx.mapping_access())
            .unwrap();
        value
    }
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 1);
        let raw = RSpan {
            buffer: BufferId::from_raw(1).unwrap(),
            start: 0,
            end: 1,
        };
        let value = map(&mut ctx, raw);
        ctx.mappings.values.push(value, ctx.host).unwrap();
        let mut pieces = small_value::SmallValue::new();
        for _ in 0..2 {
            pieces
                .push(
                    BufferPiece {
                        r: raw,
                        p: PSpan { start: 0, end: 1 },
                        kind: AppendKind::Copy,
                    },
                    &mut ctx.mapping_access(),
                )
                .unwrap();
        }
        pieces.truncate(0);
        ctx.mappings
            .buffers
            .push(
                BufferMapping {
                    owner: NativeNodeId::from_raw(1).unwrap(),
                    len: 0,
                    sealed: true,
                    pieces,
                },
                ctx.host,
            )
            .unwrap();
        let value = map(&mut ctx, raw);
        let mut prefix = ctx.string("retained zero-length prefix").unwrap();
        prefix.clear();
        ctx.mappings
            .payloads
            .push(
                PayloadBinding {
                    role: PayloadKind::Destination,
                    prefix_kind: GeneratedPrefix::Mailto,
                    raw,
                    value,
                    stage: 0,
                    stage_len: 1,
                    prefix: Some((raw, prefix)),
                },
                ctx.host,
            )
            .unwrap();
        let mut input = map(&mut ctx, raw).atoms;
        input.truncate(0);
        let mut result = ctx.string("retained zero-output result").unwrap();
        result.clear();
        ctx.mappings
            .proofs
            .push(
                Proof {
                    kind: TransformKind::SoftBreakToSpace,
                    stage: 1,
                    host: HostSnapshot {
                        source: ClaimSource::R(raw),
                        utf8_len: 1,
                        consumed_start: 0,
                        consumed_end: 1,
                        payload_role: None,
                        role: HostRole::Inline,
                        phase: NativePhase::Inline,
                    },
                    input,
                    local_start: 0,
                    local_end: 1,
                    result,
                },
                ctx.host,
            )
            .unwrap();
        assert_eq!(ctx.registry.test_counts().0, 6);
        ctx.prepare_ast_retirement().unwrap();
        ctx.retire_ast().unwrap();
        assert_eq!(ctx.registry.test_counts(), (6, 0, None));
        assert!(gauge.borrow().current > 0);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_missing_permit_and_retirement_stage_misuse_are_typed() {
    for misuse in 0..3 {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        {
            let mut ctx = Context::new(&mut host, 0);
            let error = match misuse {
                0 => ctx.retire_ast().unwrap_err(),
                1 => {
                    ctx.prepare_ast_retirement().unwrap();
                    ctx.prepare_ast_retirement().unwrap_err()
                }
                _ => {
                    ctx.ensure_slot().unwrap();
                    ctx.registry.begin_mark(ctx.host).unwrap();
                    let unregistered = String::from("not a registered physical owner");
                    ctx.registry
                        .mark_string(&unregistered, ctx.host)
                        .unwrap_err()
                }
            };
            ctx.host
                .assert_original(&error, Fault::Violation(NativeViolation::InvalidProvenance));
        }
        assert_eq!(gauge.borrow().current, 0);
    }
}

#[test]
fn ownership_multi_page_index_preserves_owners_across_rehash_and_tombstones() {
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut ctx = Context::new(&mut host, 0);
        let mut owners = Vec::new();
        for at in 0..1100 {
            owners.push(Some(
                ctx.string(&format!("retained physical owner {at}"))
                    .unwrap(),
            ));
        }
        assert_eq!(ctx.registry.test_counts().0, 1100);
        assert!(
            ctx.registry.test_counts().1
                > rows::NativeRows::<u32, <MemoryHost as Host>::Lease>::width()
        );
        for at in (0..1100).step_by(3) {
            let value = owners[at].take().unwrap();
            let pointer = value.as_ptr() as usize;
            drop(value);
            ctx.release(pointer).unwrap();
        }
        for at in (0..1100).step_by(3) {
            owners[at] = Some(
                ctx.string(&format!("retained physical owner {at}"))
                    .unwrap(),
            );
        }
        ctx.registry.begin_mark(ctx.host).unwrap();
        for (at, value) in owners.iter().enumerate() {
            let value = value.as_ref().unwrap();
            assert_eq!(value, &format!("retained physical owner {at}"));
            ctx.registry.mark_string(value, ctx.host).unwrap();
        }
        ctx.registry.finish_mark(ctx.host).unwrap();
        ctx.registry.retire(ctx.host).unwrap();
        assert_eq!(ctx.registry.test_counts(), (1100, 0, None));
        drop(owners);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_last_clean_entry_clears_but_colliding_chain_keeps_tombstones() {
    type Ledger = allocation::Registry<<MemoryHost as Host>::Lease>;
    let mut host = MemoryHost::new();
    let gauge = host.gauge.clone();
    {
        let mut registry = Ledger::default();
        let value = Box::new(71_u64);
        let pointer = std::ptr::from_ref(&*value) as usize;
        registry.prepare(true, &mut host).unwrap();
        let lease = host.reserve(8).unwrap();
        registry.retain(pointer, lease, true, &mut host).unwrap();
        assert_eq!(registry.test_index_counts(), (1, 0));
        drop(value);
        registry.release_pointer(pointer, &mut host).unwrap();
        assert_eq!(registry.test_index_counts(), (0, 0));

        // Forty-nine real live allocations guarantee four in one of the
        // sixteen initial buckets, independently of allocator address layout.
        let mut buckets: [Vec<Box<u64>>; 16] = std::array::from_fn(|_| Vec::new());
        for n in 0..49 {
            let value = Box::new(n as u64);
            let pointer = std::ptr::from_ref(&*value) as usize;
            buckets[Ledger::test_bucket(pointer, 16)].push(value);
        }
        let bucket = buckets.iter_mut().find(|values| values.len() >= 4).unwrap();
        let mut owners = Vec::new();
        for _ in 0..3 {
            let value = bucket.pop().unwrap();
            let pointer = std::ptr::from_ref(&*value) as usize;
            registry.prepare(true, &mut host).unwrap();
            let lease = host.reserve(8).unwrap();
            registry.retain(pointer, lease, true, &mut host).unwrap();
            owners.push((pointer, value));
        }
        assert_eq!(registry.test_counts().1, 16);
        for (removed, (pointer, value)) in owners.into_iter().enumerate() {
            drop(value);
            // Every later lookup must cross the previously removed chain head.
            registry.release_pointer(pointer, &mut host).unwrap();
            assert_eq!(registry.test_index_counts(), (2 - removed, removed + 1));
        }
        // Reinsert in the same real bucket: the first tombstone returned by
        // locate must be reused, including its already-read cell classification.
        let value = bucket.pop().unwrap();
        let pointer = std::ptr::from_ref(&*value) as usize;
        let old_tombstones = registry.test_index_counts().1;
        registry.prepare(true, &mut host).unwrap();
        let lease = host.reserve(8).unwrap();
        registry.retain(pointer, lease, true, &mut host).unwrap();
        assert_eq!(registry.test_index_counts().0, 1);
        assert_eq!(registry.test_index_counts().1, old_tombstones - 1);
        drop(value);
        registry.release_pointer(pointer, &mut host).unwrap();
        assert_eq!(registry.test_index_counts().0, 0);
    }
    assert_eq!(gauge.borrow().current, 0);
}

#[test]
fn ownership_retire_page_walk_keeps_marked_owners_through_partial_failure() {
    for fail in [false, true] {
        let mut host = MemoryHost::new();
        let gauge = host.gauge.clone();
        {
            let mut ctx = Context::new(&mut host, 0);
            let mut retained = Vec::new();
            let mut dead = Vec::new();
            for at in 0..1100 {
                let value = ctx.string(&format!("physical owner {at}")).unwrap();
                if at % 2 == 0 {
                    retained.push(value);
                } else {
                    dead.push(value);
                }
            }
            ctx.registry.begin_mark(ctx.host).unwrap();
            for value in &retained {
                ctx.registry.mark_string(value, ctx.host).unwrap();
            }
            ctx.registry.finish_mark(ctx.host).unwrap();
            drop(dead);
            if fail {
                ctx.host.event_fault_at = Some(ctx.host.event_calls + 17);
            }
            let outcome = ctx.retire_ast();
            if fail {
                let error = outcome.unwrap_err();
                ctx.host.assert_original(&error, Fault::Work);
                assert!(ctx.registry.test_counts().0 >= retained.len());
                assert!(ctx.registry.test_counts().0 < 1100);
                assert!(ctx.string("closed").is_err());
            } else {
                outcome.unwrap();
                assert_eq!(ctx.registry.test_counts(), (retained.len(), 0, None));
            }
            for (at, value) in retained.iter().enumerate() {
                assert_eq!(value, &format!("physical owner {}", at * 2));
            }
            drop(retained);
        }
        assert_eq!(gauge.borrow().current, 0);
    }
}

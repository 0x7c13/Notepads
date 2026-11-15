// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use comrak::native::direct as n;
use comrak::native::{
    AcceptedFact, DecodeKind, Host, NativeAdmission, NativeKind, NativeViolation, WorkDelta,
};
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Work,
    Allocation,
    Native(NativeViolation),
}

// Field presence is read from actual value bindings, not made-up payload IDs.
type LinkFact = (bool, Option<()>, Option<()>, Option<u32>);

#[derive(Default)]
struct Facts {
    atoms: Vec<(u32, usize, usize, DecodeKind, String)>,
    nodes: Vec<(u32, NativeKind)>,
    texts: Vec<String>,
    winners: Vec<(u32, bool)>,
    references: Vec<u32>,
    payloads: Vec<((), String)>,
    links: Vec<LinkFact>,
    breaks: Vec<(comrak::native::BreakKind, usize, usize, usize, usize)>,
    syntax: Vec<comrak::native::SyntaxRole>,
}

#[derive(Default)]
struct Capture {
    facts: Facts,
    work: u64,
    stop_at: Option<u64>,
    fail_reserve: bool,
    maximum_scan_unit: u64,
    stop_scan_start: Option<u64>,
    finalizer_entered: bool,
    fail_reference_bytes: bool,
    fault: Option<Fault>,
    checkpoints: usize,
}
impl std::ops::Deref for Capture {
    type Target = Facts;
    fn deref(&self) -> &Facts {
        &self.facts
    }
}
impl Capture {
    fn check(&self) -> Result<(), Fault> {
        self.fault.map_or(Ok(()), Err)
    }
    fn fail(&mut self, error: Fault) -> Fault {
        *self.fault.get_or_insert(error)
    }
}

struct CaptureHost<'input, 'capture> {
    capture: &'capture mut Capture,
    input: PhantomData<fn(&'input ()) -> &'input ()>,
}

impl Host for CaptureHost<'_, '_> {
    type Error = Fault;
    type Lease = u64;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Fault> {
        self.capture.check()?;
        if let WorkDelta::Scan(bytes) = delta {
            let before = self.capture.work;
            self.capture.maximum_scan_unit = self.capture.maximum_scan_unit.max(bytes);
            self.capture.work += bytes;
            if self
                .capture
                .stop_at
                .is_some_and(|threshold| self.capture.work >= threshold)
            {
                self.capture.stop_scan_start = Some(before);
                return Err(self.capture.fail(Fault::Work));
            }
        }
        Ok(())
    }
    fn admit(&mut self, admission: NativeAdmission) -> Result<(), Fault> {
        self.capture.check()?;
        if self.capture.fail_reference_bytes
            && matches!(admission, NativeAdmission::ReferenceBytes(_))
        {
            return Err(self.capture.fail(Fault::Work));
        }
        Ok(())
    }
    fn reserve(&mut self, bytes: u64) -> Result<u64, Fault> {
        self.capture.check()?;
        if self.capture.fail_reserve {
            Err(self.capture.fail(Fault::Allocation))
        } else {
            Ok(bytes)
        }
    }
    fn admit_actual(&mut self, lease: &mut u64, bytes: u64) -> Result<(), Fault> {
        self.capture.check()?;
        *lease = bytes;
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Fault> {
        // The migrated tests consume the real view. Production must not replay
        // either accepted facts or the old Final transport to this host.
        Err(self.reject(NativeViolation::InvalidProvenance))
    }
    fn reject(&mut self, violation: NativeViolation) -> Fault {
        self.capture.fail(Fault::Native(violation))
    }
}
impl<'input> n::DirectHost<'input> for CaptureHost<'input, '_> {
    type ReadBrand = PhantomData<fn(&'input ()) -> &'input ()>;
    fn finish<T>(&mut self, result: Result<T, Fault>) -> Result<T, Fault> {
        let result = result.map_err(|error| self.capture.fail(error));
        match self.capture.fault {
            Some(error) => Err(error),
            None => result,
        }
    }
    fn checkpoint_now(&mut self) -> Result<(), Fault> {
        self.capture.checkpoints += 1;
        self.capture.check()
    }
}

fn value_text<'store, 'view, 'arena, 'input>(
    view: &mut n::FinalView<'store, 'view, 'arena, 'input, CaptureHost<'input, '_>>,
    token: n::ValueToken<'view, 'arena, 'input>,
) -> Result<String, Fault> {
    let len = view.value_len(token)?;
    let mut cursor = view.value_cursor(token, 0, len)?;
    let mut text = String::new();
    while text.len() < len {
        let (_, next) = view.with_next_value_chunk(cursor, 256, |chunk, _| {
            text.push_str(chunk);
            Ok(())
        })?;
        cursor = next;
    }
    Ok(text)
}

fn read_proofs<'store, 'view, 'arena, 'input>(
    view: &mut n::FinalView<'store, 'view, 'arena, 'input, CaptureHost<'input, '_>>,
    map: n::MappingToken<'view, 'input>,
    owner: u32,
    facts: &mut Facts,
) -> Result<(), Fault> {
    let mut pending = Vec::new();
    for index in (0..view.mapping_len(map)?).rev() {
        if let Some(proof) = view.mapping_atom(map, index)?.proof {
            pending.push(proof);
        }
    }
    while let Some(token) = pending.pop() {
        let proof = view.proof(token)?;
        let mut input_bytes = 0;
        for index in (0..view.mapping_len(proof.input)?).rev() {
            let atom = view.mapping_atom(proof.input, index)?;
            input_bytes += atom.bytes;
            if let Some(ancestor) = atom.proof {
                pending.push(ancestor);
            }
        }
        if let n::TransformKind::Decode(kind) = proof.kind {
            let end = proof.output_len - (input_bytes - proof.local_end);
            let mut output = String::new();
            while proof.local_start + output.len() < end {
                let at = proof.local_start + output.len();
                view.with_proof_chunk_next(token, at, end, 256, |chunk, _| {
                    output.push_str(chunk);
                    Ok(())
                })?;
            }
            facts.atoms.push((
                owner,
                proof.host.consumed_start,
                proof.host.consumed_end,
                kind,
                output,
            ));
        }
    }
    Ok(())
}

fn read_facts<'store, 'view, 'arena, 'input>(
    view: &mut n::FinalView<'store, 'view, 'arena, 'input, CaptureHost<'input, '_>>,
    input: &str,
) -> Result<Facts, Fault> {
    let mut facts = Facts::default();
    // Actual definition acceptance order includes losers and unused values.
    for index in 0..view.definition_count() {
        let definition = view.definition(index)?;
        facts.winners.push((definition.id.get(), definition.winner));
        facts
            .payloads
            .push(((), value_text(view, definition.destination)?));
        if let Some(title) = definition.title {
            facts.payloads.push(((), value_text(view, title)?));
        }
    }
    for index in 0..view.claim_count() {
        let claim = view.claim(index)?;
        if let n::ClaimKind::Syntax(role) = claim.kind {
            // Syntax facts have P domains; R Cut claims are a different action.
            if view.source_owner(claim.source)?.is_none() {
                facts.syntax.push(role);
            }
        }
    }
    let mut pending = vec![view.root()?];
    while let Some(node) = pending.pop() {
        let info = view.node(node)?;
        facts.nodes.push((info.id.get(), info.kind));
        if let Some(next) = info.next_sibling {
            pending.push(next);
        }
        if let Some(child) = info.first_child {
            pending.push(child);
        }
        let lineage = view.lineage(node)?;
        let binding = view.binding(lineage)?;
        read_proofs(view, binding.value, info.id.get(), &mut facts)?;
        match info.attributes {
            n::NodeAttributes::Text(value) => facts.texts.push(value_text(view, value)?),
            n::NodeAttributes::Code { info, .. } => {
                if view.value_mapping(info)?.is_some() {
                    facts.payloads.push(((), value_text(view, info)?));
                }
            }
            n::NodeAttributes::Link { destination, title } => {
                let has_destination = view.value_mapping(destination)?.is_some();
                let has_title = view.value_mapping(title)?.is_some();
                facts.links.push((
                    info.kind == NativeKind::Image,
                    has_destination.then_some(()),
                    has_title.then_some(()),
                    binding.definition.map(|id| id.get()),
                ));
                if binding.reference_use {
                    facts.references.push(
                        binding
                            .definition
                            .ok_or(Fault::Native(NativeViolation::InvalidProvenance))?
                            .get(),
                    );
                } else {
                    if has_destination {
                        facts.payloads.push(((), value_text(view, destination)?));
                    }
                    if has_title {
                        facts.payloads.push(((), value_text(view, title)?));
                    }
                }
            }
            _ => {}
        }
        if matches!(info.kind, NativeKind::SoftBreak | NativeKind::HardBreak) {
            let ending = view.mapping_atom(binding.value, 0)?.source;
            let ending = view.source_domain(ending)?;
            let mut prefix = (ending.consumed_end, ending.consumed_end);
            let mut marker = String::new();
            for index in 0..view.claim_count() {
                let claim = view.claim(index)?;
                if view.binding(claim.owner)?.id != info.id {
                    continue;
                }
                match claim.kind {
                    n::ClaimKind::Whitespace => {
                        let domain = view.source_domain(claim.source)?;
                        prefix = (domain.consumed_start, domain.consumed_end);
                    }
                    n::ClaimKind::Break => {
                        let mut cursor = view.source_cursor(claim.source)?;
                        while let Some(piece) = view.next_source(&mut cursor)? {
                            marker.push_str(&input[piece.p.start..piece.p.end]);
                        }
                    }
                    _ => {}
                }
            }
            // The real HardBreak plus its accepted marker claim distinguishes
            // spaces/backslash; this does not re-run the grammar on raw text.
            let kind = if info.kind == NativeKind::SoftBreak {
                comrak::native::BreakKind::Soft
            } else if marker == "\\" {
                comrak::native::BreakKind::Backslash
            } else {
                assert!(!marker.is_empty() && marker.bytes().all(|byte| byte == b' '));
                comrak::native::BreakKind::Spaces
            };
            facts.breaks.push((
                kind,
                ending.consumed_start,
                ending.consumed_end,
                prefix.0,
                prefix.1,
            ));
        }
    }
    Ok(facts)
}

fn parse_capture(
    input: &str,
    options: &comrak::Options,
    capture: &mut Capture,
) -> Result<(), Fault> {
    n::with_input(input, |native_input| {
        let mut host = CaptureHost {
            capture,
            input: PhantomData,
        };
        let entered = std::cell::Cell::new(false);
        let result = n::parse(native_input, options, &mut host, |mut view| {
            entered.set(true);
            let facts = read_facts(&mut view, input)?;
            view.with_host(|host| {
                host.capture.facts = facts;
                Ok(())
            })
        });
        host.capture.finalizer_entered = entered.get();
        result
    })
}
#[test]
fn native_accepts_strong_and_entity_closures_from_actual_r() {
    let mut capture = Capture::default();
    parse_capture("**中&amp;😀**", &comrak::Options::default(), &mut capture).unwrap();
    assert!(
        capture
            .nodes
            .iter()
            .any(|(_, kind)| *kind == NativeKind::Strong)
    );
    assert_eq!(capture.texts, ["中&😀"]);
    assert_eq!(
        (
            capture.atoms[0].1,
            capture.atoms[0].2,
            capture.atoms[0].3,
            capture.atoms[0].4.as_str()
        ),
        (5, 10, DecodeKind::Entity, "&")
    );
}

#[test]
fn native_entity_multi_and_private_use_are_preserved() {
    let mut capture = Capture::default();
    parse_capture(
        "&NotEqualTilde; &#xE000;",
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    assert_eq!(capture.texts, ["≂\u{0338} \u{E000}"]);
    assert_eq!((capture.atoms[0].1, capture.atoms[0].2), (0, 15));
    assert_eq!(capture.atoms[0].4.encode_utf16().count(), 2);
}

#[test]
fn native_long_plain_input_faults_inside_bounded_scan() {
    let mut capture = Capture {
        stop_at: Some(131_072),
        ..Capture::default()
    };
    let input = "x".repeat(1_048_576);
    assert_eq!(
        parse_capture(&input, &comrak::Options::default(), &mut capture),
        Err(Fault::Work)
    );
    assert!(capture.maximum_scan_unit <= 65_536);
    // A real directory read also consumes Scan work. Rejection occurs on the
    // first bounded charge crossing the same threshold, not at an assumed
    // grammar-only multiple that depends on the storage representation.
    assert!(capture.stop_scan_start.unwrap() < 131_072);
    assert!((131_072..131_072 + capture.maximum_scan_unit).contains(&capture.work));
    assert_eq!(capture.fault, Some(Fault::Work));
    assert!(!capture.finalizer_entered);
    assert!(capture.texts.is_empty());
}

#[test]
fn native_arena_reserve_failure_is_the_original_host_fault() {
    let mut capture = Capture {
        fail_reserve: true,
        ..Capture::default()
    };
    assert_eq!(
        parse_capture("text", &comrak::Options::default(), &mut capture),
        Err(Fault::Allocation)
    );
}

#[test]
fn native_unimplemented_g4_is_explicit() {
    let mut options = comrak::Options::default();
    options.extension.autolink = true;
    assert_eq!(
        parse_capture("www.example.com", &options, &mut Capture::default()),
        Err(Fault::Native(NativeViolation::UnsupportedConfiguration))
    );
}

#[test]
fn native_crlf_break_and_continuation_preserve_executed_r() {
    let mut capture = Capture::default();
    parse_capture("a  \r\n  b", &comrak::Options::default(), &mut capture).unwrap();
    assert_eq!(
        capture.breaks,
        [(comrak::native::BreakKind::Spaces, 3, 5, 5, 7)]
    );
    assert_eq!(capture.texts, ["a", "b"]);
}

#[test]
fn native_image_payloads_use_accepted_decoder_stages() {
    let mut capture = Capture::default();
    parse_capture(
        r#"![a&amp;b](<u\&amp;x> "t&amp;\!")"#,
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    assert!(
        capture
            .nodes
            .iter()
            .any(|(_, kind)| *kind == NativeKind::Image)
    );
    assert_eq!(capture.texts, ["a&b"]);
    assert_eq!(
        capture
            .payloads
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        ["u&x", "t&!"]
    );
    assert_eq!(capture.links.len(), 1);
    assert!(capture.links[0].0);
}

#[test]
fn native_reference_winner_and_lookup_are_one_table() {
    let mut capture = Capture::default();
    parse_capture(
        "[x]: <u&amp;x> 't\\!'\n[x]: /later\n\n[a][x]",
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    assert_eq!(capture.winners, [(1, true), (2, false)]);
    assert_eq!(capture.references, [1]);
    assert_eq!(capture.links[0].3, Some(1));
    assert_eq!(capture.texts, ["a"]);
}

#[test]
fn native_reference_fuel_fault_does_not_fall_back_to_text() {
    let mut capture = Capture {
        fail_reference_bytes: true,
        ..Capture::default()
    };
    assert_eq!(
        parse_capture(
            "[x]: /url\n\n[x]",
            &comrak::Options::default(),
            &mut capture
        ),
        Err(Fault::Work)
    );
    assert!(capture.texts.is_empty());
}

#[test]
fn native_abandoned_reference_title_is_not_attached() {
    let mut capture = Capture::default();
    parse_capture(
        "[foo]: /url\n'[foo]'bar\n\n[foo]",
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    assert!(!capture.links.is_empty());
    assert!(capture.links.iter().all(|link| link.2.is_none()));
    assert_eq!(capture.winners, [(1, true)]);
}

#[test]
fn native_c0_block_syntax_and_fence_info_have_live_origins() {
    use comrak::native::SyntaxRole as Role;
    let mut capture = Capture::default();
    parse_capture(
        "\u{feff}# h ##\r\n\r\n> p\n\n- q\n\n``` lang&amp;\\!\ncode\n```\n",
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    for role in [
        Role::LeadingBom,
        Role::HeadingPrefix,
        Role::HeadingSuffix,
        Role::QuotePrefix,
        Role::ListPrefix,
        Role::FenceOpening,
        Role::FenceClosing,
    ] {
        assert!(capture.syntax.contains(&role), "{role:?}");
    }
    assert!(capture.payloads.iter().any(|(_, value)| value == "lang&!"));
}

#[test]
fn native_multiline_html_preserves_paragraph_continuation_indent() {
    let mut capture = Capture::default();
    parse_capture(
        "<foo bar =\n \"hi\"> \n",
        &comrak::Options::default(),
        &mut capture,
    )
    .unwrap();
    assert!(
        capture
            .nodes
            .iter()
            .any(|(_, kind)| *kind == NativeKind::HtmlInline)
    );
}

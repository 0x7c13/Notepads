// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Finite actual direct-session fault sweep. Counts select operation boundaries,
//! never an expected grammar/model. Full IR/query oracles live in g3_models.

use super::*;

struct SweepHost<'input> {
    inner: TestHost<'input>,
    calls: usize,
    stop: Option<usize>,
    cancel: bool,
    late_cancel: bool,
}

impl<'input> SweepHost<'input> {
    fn new(input: &NativeInput<'input>, stop: Option<usize>, cancel: bool) -> Self {
        Self {
            inner: TestHost::new(input),
            calls: 0,
            stop,
            cancel,
            late_cancel: false,
        }
    }

    fn gate(&mut self) -> Result<(), Error> {
        self.inner.check()?;
        self.calls += 1;
        if self.stop == Some(self.calls) {
            self.inner.fault = Some(if self.cancel {
                Error::Cancelled
            } else {
                Error::Raw
            });
        } else if self.late_cancel {
            self.inner.fault = Some(Error::Cancelled);
        }
        self.inner.check()
    }
}

impl Host for SweepHost<'_> {
    type Error = Error;
    type Lease = Lease;

    fn work(&mut self, delta: WorkDelta) -> Result<(), Error> {
        self.gate()?;
        self.inner.work(delta)
    }
    fn admit(&mut self, request: NativeAdmission) -> Result<(), Error> {
        self.gate()?;
        self.inner.admit(request)
    }
    fn reserve(&mut self, bytes: u64) -> Result<Lease, Error> {
        self.gate()?;
        self.inner.reserve(bytes)
    }
    fn admit_actual(&mut self, lease: &mut Lease, bytes: u64) -> Result<(), Error> {
        self.gate()?;
        self.inner.admit_actual(lease, bytes)
    }
    fn fact(&mut self, fact: AcceptedFact<'_>) -> Result<(), Error> {
        self.inner.fact(fact)
    }
    fn reject(&mut self, violation: NativeViolation) -> Error {
        self.inner.reject(violation)
    }
}

impl<'input> DirectHost<'input> for SweepHost<'input> {
    type ReadBrand = Invariant<'input>;

    fn finish<T>(&mut self, result: Result<T, Error>) -> Result<T, Error> {
        self.inner.finish(result)
    }
    fn checkpoint_now(&mut self) -> Result<(), Error> {
        self.gate()?;
        self.inner.checkpoint_now()
    }
}

fn run(text: &str, stop: Option<usize>, cancel: bool) -> (Result<u32, Error>, usize) {
    with_input(text, |input| {
        let mut host = SweepHost::new(&input, stop, cancel);
        let mut options = crate::Options::default();
        options.extension.table = true;
        options.extension.tasklist = true;
        options.extension.strikethrough = true;
        options.extension.autolink = false;
        let result = parse(input, &options, &mut host, |mut view| {
            let mut stack = vec![view.root()?];
            let mut kinds = 0;
            while let Some(node) = stack.pop() {
                let info = view.node(node)?;
                if let Some(next) = info.next_sibling {
                    stack.push(next);
                }
                if let Some(child) = info.first_child {
                    stack.push(child);
                }
                kinds |= match info.kind {
                    NativeKind::Table => 1,
                    NativeKind::TableCell => 2,
                    NativeKind::TaskCheckbox => 4,
                    NativeKind::Strike => 8,
                    NativeKind::Link => 16,
                    _ => 0,
                };
            }
            Ok(kinds)
        });
        assert_eq!(
            host.inner.live.get(),
            0,
            "actual leases after direct return"
        );
        assert_eq!(host.inner.old_transport, 0);
        if let Some(at) = stop.filter(|_| result.is_err()) {
            let expected = if cancel { Error::Cancelled } else { Error::Raw };
            assert_eq!(host.calls, at, "requested boundary must be reached");
            assert_eq!(result, Err(expected));
            assert_eq!(host.inner.fault, Some(expected));
            host.late_cancel = true;
            assert_eq!(host.work(WorkDelta::Events(0)), Err(expected));
            assert_eq!(host.reject(NativeViolation::InvalidProvenance), expected);
            assert_eq!(host.inner.live.get(), 0);
        } else {
            assert_eq!(host.inner.fault, None);
            if let Some(at) = stop {
                assert!(
                    host.calls < at,
                    "successful invocation did not reach the cut"
                );
            }
        }
        (result, host.calls)
    })
}

#[test]
fn g3_actual_direct_faults_preserve_first_error_and_release_every_native_owner() {
    for (name, text, expected_kinds) in [
        (
            "table",
            "prefix\n| a\\|b | c |\n| --- | --- |\n| x |\n| y | z | ignored |\n",
            3,
        ),
        ("task", "- [x] ~~label~~\n- [ ] \n\nlater\n", 12),
        ("title", "~~a~~ [r]\n\n[r]: /u \"a\n    bcd  \"\n", 24),
    ] {
        let (result, count) = run(text, None, false);
        assert_eq!(result, Ok(expected_kinds), "{name}");
        assert!(
            count > 0 && count < 20_000,
            "finite small family: {name} {count}"
        );
        for cancel in [false, true] {
            let mut failures = 0;
            // Registry probe counts depend on real allocation addresses. A
            // later invocation cannot inherit the first one's exact call count.
            // Stop at the first actually successful bounded invocation; each
            // reported failure must itself prove it reached its requested cut.
            for at in 1..=20_000 {
                let (result, _) = run(text, Some(at), cancel);
                if result.is_ok() {
                    assert_eq!(result, Ok(expected_kinds));
                    break;
                }
                failures += 1;
            }
            assert!(failures > 0 && failures < 20_000);
            println!(
                "G3 direct {name}: cancel={cancel}, {failures} actual reached cuts, then successful termination"
            );
        }
        println!("G3 direct {name}: first positive invocation {count} boundaries");
    }
}

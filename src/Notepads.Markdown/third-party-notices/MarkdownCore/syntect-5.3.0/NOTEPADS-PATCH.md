# Notepads scoped execution patch

Upstream: syntect 5.3.0, MIT, tag commit `e4670846ecf16d8832db6c43d531bec466214e27`.
Crates.io archive SHA256: `656B45C05D95A5704399AEEF6BD0DDEC7B2B3531B7C9E900ABBF7C4D2190C925`.

The original crate files, licenses, serialized syntax assets and Cargo manifests are preserved.
`NOTEPADS-UPSTREAM.json` lists every original file/hash; `NOTEPADS.patch` records the
four modified parsing files and one added parsing file. Modified files retain upstream
copyright/license and add 2026 modification attribution. `NOTEPADS-SYNTAX-NOTICES.txt`
preserves the active syntax dump's Packages root license and per-file/directory exceptions.
It does not describe the entire Sublime Packages repository as MIT.

This patch is limited to `src/parsing/{execution,mod,regex,parser,syntax_definition}.rs`.
No serialized fields, grammar patterns, default dumps, syntax priorities, or original
stack-depth behavior are changed. The crate version and dependency constraints remain 5.3.0.

The additive Rust API is:

```rust
let mut control = ExecutionControl::new(&mut monitor);
let ops = parse_state.parse_line_with_control(line, syntax_set, &mut control)?;
```

Use one borrowed control across all lines of one block. `ExecutionMonitor::checkpoint(stage,
units)` borrows the owner's cancellation, actual clock-based deadline and checked accounting.
Zero units still require an actual deadline/cancellation observation. Units are parser events,
not regex VM instructions. The product owner must use a separately funded highlight Operation
whose failure does not poison the Markdown/body Operation, and must own aggregate execution,
cache and allocation bounds. This library does not create a second budget or publication owner.

`ParsingError::Execution` carries `ExecutionError`: cancellation/deadline/work/resource stops,
the first regex compile/search error with its actual backend `Error` source, or an attempt to
reuse a terminated control. `termination()` retains only a compact first terminal marker.
Any parser error poisons that block control. Reuse returns immediately without monitor calls
or parser mutations. Discard ParseState, ScopeStack and all temporary tokens after any error.

The controlled regex path uses `OnceCell::get_or_try_init`; successful shared regex compilation
stores no borrowed control. A failed compile is not cached as success. Backend runtime errors
are propagated before post-call cancellation observations. Parser searches propagate Result,
do not cache fatal as a negative match, and stop before visiting further patterns. The controlled
MatchIter path checks its own include traversal loop and uses iteration rather than recursive
skipping. Token, context, cached-pattern, capture, scope-operation and backreference work also
has cooperative checkpoints. Legacy bool regex methods and the original parse_line entry
remain available; the product must use the new controlled path.

Opaque regex compile/search calls, lazy context decoding, OnceCell waiting and allocation
cannot be preempted. The patch provides no hard wall-time, instruction-count or RSS bound and
does not meter third-party allocations. The known upstream C# Strings 38-position fancy-regex
difference is unchanged. First product allowlist remains C++, Rust, JSON, JavaScript and Python;
the dump's 75 syntaxes do not imply 75 qualified languages.

Current validation entry: `scripts/Test-Rust.ps1 -Layer Core` from the module root.
The retained parsing tests and Notepads highlight tests cover the controlled path.
Full AOT/MSIX/UI qualification is recorded separately in the task verification.

# Notepads private native parser integration

Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
The original Comrak copyright and BSD terms in `COPYING` remain applicable.

The current source accepts explicit C0/G3 and the separately selected strict-G4
inline policy. G3 adds the approved table, task-list and double-tilde strike
policies. Native dollar/LaTeX math is opt-in and records the same acceptance
boundary described below; native code-math remains rejected. Source correctness,
package delivery and resource/performance qualification are separate. See the
current handoff for the exact qualified source/package and known limitations.

## Direct compilation and lifetime

`native/direct.rs` consumes one generative `NativeInput`. The existing parser
runs through `parse_in_context`; no second ordinary grammar is maintained.
`DirectCompiler::ast` reads actual arena nodes, typed accepted value slots,
facts and definitions through short branded reader borrows. Node tokens have
both the arena lifetime and invariant input/view identities; integers and AST
addresses cannot create a public source capability.

The AST callback completes Core graph/metadata capture before arena retirement.
The native driver then marks every retained physical owner, drops the real AST
and delimiter arenas, releases unmarked permits and destroys the address index.
The provenance callback can read retained maps, proofs, claims and definitions;
it has no AST/value capability and reconstructs no winner or source relation.
Its result cannot borrow native storage. The private builder can freeze only
after native storage has physically dropped and the original Host's final
clock/cancellation checkpoint has succeeded. Core validation and index building
remain independent of the producer. This source retains the accepted
measure/target organization; the failed isolated P DFS candidate is not merged.

## Source, claims and payloads

`native/mapped.rs` updates R/P relationships at actual accepted mutations:
append/cut, text acceptance/join, delimiter crop, decoder output, code/break
normalization and payload stages. Atoms move with their values. Connected proof
ancestry retains original inputs, accepted output, complete host domains,
consumed ranges, roles and phases. Claim order follows grammar actions rather
than final model IDs. Birth lineage survives detached donors and cannot grant
final-tree membership. S/N01 composition belongs to Core's one prepared binding.

Sequential decoder edits use a charged position cursor. Crop, hide, join and
stage restarts invalidate or reposition it. Transform replacement preserves
zero-output atoms in boundary order. Actual suffix moves and simultaneous
old/new allocation capacities remain charged; the cursor does not change fee
units or resource limits.

`native/definitions.rs` owns each original normalized definition key and cleaned
value, including unused and losing definitions. The one original RefMap decides
the winner. Its direct index stores the definition ID and borrows the one actual
normalized record key for hashing and comparison. Each accepted reference use
shares typed destination/title identities while paying its original lookup,
reference-byte and URL-then-title decoded-byte admissions. UTF-8 logical chunks
retain their original 64 KiB boundary. The store survives until native
finalization ends; no occurrence string is materialized.

Successful reference titles use Trim, ReferenceTitleIndent, Entity and Escape
in that order. The producer checks the real normalized LF followed by maximal
SP/TAB runs before decoding; a generated LF cannot trigger indentation removal.
The single synchronous borrowed descriptor uses old-stage coordinates. The map
transport checks identity/phase/geometry, preserves real R pieces and records
removed indentation as Hidden. It restores its taken map directly into the
original row before propagating failure. This preprocessing stage is never a
decoder proof. Inline titles keep their original pipeline.

## Current value storage and reads

`native/value.rs` uses the existing accepted maps, R/P pieces, proof results,
payload prefixes and code terminal records as the current value authority.
Direct AST fields carry `AcceptedValueRef` identities or explicit absent Empty;
Owned and Pending slots are rejected at final traversal. Explicit zero-length
payloads retain their identities. Legacy parsing retains an explicit Owned
representation; ValueText has no implicit Deref, Display or string fallback.

A consuming, nonclone `ValueCursor` validates both UTF-8 endpoints and lends
at most one actual backing per read. Reads do not run a decoder, resolver or
scanner and do not allocate a whole-value cache. The callback and the original
read-finish check must both succeed before the replacement cursor is returned.
Core installs the returned cursor only after its enclosing read/finish succeeds.
Brand and arena lifetimes prevent
using it across input, view or AST ownership scopes.

BlockLines, Subject and decoder/stage builders remain actual temporary String
owners until their last use. The accepted raw/proof storage, one normalized key,
Core IR copies, independent validator and indexes remain in the physical budget.
This representation does not establish a time or memory improvement by itself.

## G3 acceptance

Table records retain actual header/delimiter/alignment/row/cell facts. A split
keeps the prefix Paragraph and derives new buffers from accepted pieces; the
new BlockStart comes from the real single-Copy pipe capability. Failed probes
are not permanently cached. Reference-definition residual indentation and table
delimiter indentation retain distinct accepted hidden source claims. Cross-buffer
reads validate lineage, kind and source owner and do not reuse another buffer's
host cache. Escaped pipes retain the real two-byte source and one-byte output
with the minimum accepted Table/Block authority; missing cells use real row-end
anchors and extra cells retain their consumption facts.

Task classification uses the actual first item Paragraph before ordinary
inline decoding or trimming, a raw marker and the approved ASCII spacer. Empty
labels create no fake Paragraph. The Checkbox is an item child; only the original
three-byte markers can grant a task action. Strike uses the original delimiter
stack and accepts only a complete run of two tildes. Core measure and emission
share separator rules; Image Main/Alt and metadata remain separate targets.

## Physical ownership, errors and checks

NativeRows pages and directories each own their original Host lease once.
Small values use the locked SmallVec union representation and spill through the
same actual-capacity admission. Stable nonclone permit rows carry checked slot
generations; the String/Box address index stores narrow slot codes. Direct-ID
owners have no second index permit. Rehash and partial moves charge old and new
owners while both actually exist. Empty values with nonzero capacity still count.
Backing storage drops before its permit, including growth/admission failures.

Mutation, session and reader callback guards latch through the same original
Host; no second error, budget or cancellation flag is stored. A typed first
failure survives later cancellation, cleanup and rejection. An abandoned scope
cannot restart the operation. Rust errors/panics remain inside their specified
native/FFI boundaries.

The legacy FinalWorkspace/Final-message transport and parse_with_host wrappers
are cfg(test) regressions, absent from Core's normal dependency. Normal capture
cannot re-enter that transport. Upstream noncapturing parse_document is a separate
compatibility entry and is not the Notepads production path.

Correctness evidence covers fixed upstream oracles, independent complete small
models and real queries, lifetime/first-fault tests and finite native fault cuts.
Their scope and unknown layers are recorded in the handoff; none proves complete
joint actual memory, natural cancellation latency or product performance.
Workspace Clippy with warnings denied passes for all targets. Standalone vendor
library Clippy also passes with warnings denied after removal of the original
eight redundant style expressions and the new unused unit branch. Formatting
checks cover the workspace and changed vendor Rust files.

The original G3 legacy library run failed 70 of 637 tests. The first aligned
candidate run failed 71 of 651, including one email case that passed alone.
Repairs keep the allocation registry strict: actual legacy Text/Subject/reference
owners now enter through the existing Context factories, and replaced owners
are explicitly released. Noncapturing inline parsing uses the actual AST start
line; captured C0/G3 retains its original accepted-buffer line. The intermediate
633/18 source-position failure and all first failures remain recorded. With
unchanged original expectations, the corrected candidate passes the complete
x64 suite (651) and x86 suite (650); one wide-coordinate/acceptance-snapshot test
is explicitly x64-only.

The independently reviewed candidate, now integrated into this source, also passed Core/model/query/FFI, applicable original
C0/G3 baselines and the existing nine-family set. Those earlier source seals are
retained with their actual scopes; later legacy/style repairs do not silently
rewrite them. Controlled tests and compiler capability checks establish no
product package, natural cancellation or performance qualification. Final
integration must validate one final source and configuration.

## Opt-in native math facts

The actual successful dollar/LaTeX scanners emit one `AcceptedFact::Math` with
complete syntax, exact consumed literal R span and one of four `MathSyntax`
variants. The two dollar and two LaTeX forms remain distinct; display is derived
from syntax, never reconstructed from diagnostic source positions. Native
`math_code` is explicitly rejected at the existing option-validation boundary;
legacy parsing and ordinary upstream code-math formatting keep their behavior.

`NodeMath.literal` uses the existing `ValueText` authority. Its actual slot carries
`AcceptedValueRef::Node` with `MathInput`; `Field::Math` reads the same canonical
map, not an owned duplicate or raw AST string. Full syntax has one
`ClaimKind::Math(MathSyntax)` with Inline phase and MathInput host role. Sparse
syntax binding retains the actual syntax variant and survives arena retirement.
AST reads check the real value identity, node owner and display agreement.
A Math-only sparse `NativeRows<MathBinding>` retains the exact literal R span
from the accepted fact. Common syntax records carry only an optional nonzero
index; they do not duplicate that span for ordinary Markdown. `binding.math_input`
and `mapping_source` for a real Math node issue the same raw-source capability,
including bytes removed by later normalization. Other node mappings keep their
previous absent raw-source result; current atoms do not reconstruct a full host.

Single-dollar math calls the same funded `normalize_code_with_host` execution
used by inline code. Its actual line-ending/edge-trim transformations emit their
proofs against consumed R spans and MathInput host; other math forms do not gain
these transformation permissions. Double-dollar/LaTeX literal bytes bind raw
accepted maps and preserve upstream literal behavior. TeX backslashes and HTML
entity-looking input are not Markdown metadata-cleaned. Existing R/P transport
continues to describe container-prefix removal, CRLF and table escaped pipes.

`native/direct/math_tests.rs` checks four actual syntax forms, scalar chunks,
CRLF, real table/container transport, complete-syntax claims, wrong role/owner
and unaccepted slot rejection, post-arena provenance reads and cancellation at
real retirement/final checkpoints. Ordinary upstream mathematical expected
results remain unchanged. These are native producer tests; they do not qualify
Core objects, a math renderer, UWP resource safety or a delivered product package.

### Finite implementation-review repair

The first independent implementation review found two P1 issues in the opt-in
Math source: uncharged/unbounded LaTeX closing search and omitted Math shadow
protection in the explicit StrictG4 candidate route. Its v1 source and original
699-test observations remain frozen outside the working vendor tree.

LaTeX closing search now uses the existing charged `Context::find` batches and
checks the second closing byte only inside the actual active gap. Its success,
failed/repeated opener and long-tail paths are charged and can return the first
Host scan fault/cancellation. Real scanner tests cover a backslash at the gap
edge and a closing character outside the gap. They do not measure natural
cancellation latency or establish UWP performance.

StrictG4 shadow acceptance now records an explicit Math owner from the actual
full-syntax Math fact. The selector recognizes that excluding owner exhaustively,
as it does Code/Html/Link/Image. A finite four-mode test checks internal URL
protection, real outside links, adjacent formulas and Link/Image label nesting.
This combination's producer behavior is tested; general G4 product admission is
still a separate, unpassed qualification.

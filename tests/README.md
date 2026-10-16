# Notepads tests

GitHub CI runs the unit, source, headless and native checks automatically, and
publishes/validates Production Native AOT packages for x86, x64 and ARM64.
The required hosted E2E job executes the x64 Release AOT App groups, external UI
Automation and cross-process drag tests. Resilience Tests runs the dedicated
Debug allocation-failure checks concurrently on a separate VM. An additional
manual desktop workflow supports dedicated x64/ARM64 machines. The x64 package
job additionally installs and drives the actual Production application.
See [CI and local reproduction](../CI.md) for commands, artifacts and runner setup.

Two test projects serve different iteration loops. `UnitTests` runs actual pure
production logic without files, native libraries, Windows UI or external services.
`IntegrationTests` runs its portable Headless host, isolated Windows App host, or
external scripts under `PackagedApp`. The native C++ executables and drag peer
under `Support` are helpers.

```powershell
# Fast default: pure unit tests (independent xUnit fixtures run in parallel).
./scripts/Test.ps1
./scripts/Test.ps1 -Filter 'FullyQualifiedName~DocumentOperation'

# Discover integration groups, then run only the feature being changed.
./scripts/Test.ps1 -Suite Integration -List |
    Format-Table Group,Profile,Default,Configuration,Performance -AutoSize
./scripts/Test.ps1 -Suite Integration -Group Documents,Clipboard -Platform ARM64

# Reuse a verified build. Source/configuration/platform/artifact changes fail closed.
./scripts/Test.ps1 -Suite Integration -Group Documents -Platform ARM64 -NoBuild

# Default correctness matrix, including Release AOT and a separate Debug fault lane.
./scripts/Test.ps1 -Suite All -Platform ARM64

# Only optional native allocation failures; this group always builds Debug.
./scripts/Test.ps1 -Suite Integration -Group NativeFaults -Platform ARM64
```

`tests/Tests.slnx` contains both projects. `IntegrationHost=Headless` is the default;
`IntegrationHost=App` selects the UWP host. Their NuGet, generated source and build
outputs are isolated. App builds require Visual Studio 2026 UWP/native tools.
The runner chooses the matching compiled product assembly for architecture checks.

Unit filters and integration groups must select real tests. Unknown groups and
empty filters fail. Named group results, durations and unexecuted groups are
written to `artifacts/test-runs`; UWP phase reports and preservation evidence are
under `artifacts/isolated-uwp`. PASS counts in free-form logs do not define coverage.

Pure unit fixtures parallelize. App/desktop tests serialize behind a shared named
mutex because package registration, foreground input and UI state are exclusive
resources. Headless metrics failure scenarios use separate processes so poisoning
a static metrics observer cannot affect another scenario. Performance/stress,
physical UI Automation/drag and package checks are explicit groups; `-List`
identifies the defaults. Ordinary App phases and the interactive editor host must
exit cleanly after reporting success. The drag peer is currently an exception:
cleanup terminates the owned helper; its natural zero-code exit is not verified.
The physical drag fixture requires a desktop-hosted UWP window. It waits for the
owned CoreWindow's actual frame before arranging windows, then checks unchanged
HWND/PID ancestry, foreground and intended hit targets around gestures. Initial
mouse-down currently lacks the actual-cursor recheck used for mouse-up, so this
does not guarantee safe injection during concurrent physical mouse input. A
different hosting model fails setup rather than relaxing ownership checks.

Before the first physical case, both drivers acquire their owned test windows'
foreground. The drag driver initializes both editor and peer and verifies that
ordinary activation can switch between them. If normal activation is denied, a
shared helper clicks a verified
non-client caption, temporarily raising only that window when necessary. It
restores temporary window/input state before returning; unrelated windows are
not minimized. Ownership, idle input and the existing per-gesture foreground
and hit-target checks remain required. This bootstrap does not recover a test
that loses foreground during a gesture.

## Isolation and the meaning of a pass

Local App/desktop tests register only `Notepads.Editor.Tests`. The runner restores the
exact prior loose registration, compares pre-existing test LocalState files, and
checks future-access token/metadata preservation. It verifies executable path and
process creation time before terminating an owned process. Production/Dev Notepads,
Visual Studio and user document sessions are outside the runner's scope.

The explicit `PackagedApp` group is a separate hosted-only boundary: it installs a
temporarily signed copy of the actual Production bundle, retaining its identity
and checking the installed executable hash. It refuses existing product
registrations/processes/data, and uninstalls its package and temporary certificate
afterward. The driver observes UIA text, actual file bytes, process/window identity
and OS execution state. It drives the shipped entry points without an in-process
test host.
Results live under `artifacts/packaged-app`; see [CI.md](../CI.md) for its scenarios
and the distinctions between Windows file activation, Explorer association and
Microsoft Store deployment. This group never installs the product locally.

Each run uses a private GUID directory and a real LocalSettings subcontainer.
Explicit restart phases retain that container; final cleanup removes it. The App
host compiles actual production pages, resources, preferences, document services,
native editor and workspace workflows. Test partials expose access to real
commands and controls; they do not reimplement the operations being asserted.

Some outbound boundaries intentionally stop at the operating system:

| Boundary | Exercised in normal integration tests | Separate external validation |
| --- | --- | --- |
| Clipboard | Actual copy/cut/paste, selection, undo and Windows DataPackage; rejected/throwing writes | System clipboard history/roaming and cross-app paste in a disposable Windows user/session |
| File picker | Actual factory, cancel/result/error handling and real file writes | OS chooser UI and arbitrary user folder permissions |
| Recent files | Actual menu ordering/filtering, open/reuse and clear commands against an in-memory MRU boundary | Windows MRU persistence, permission expiry and access-cache failure handling |
| Launcher | Actual command-line parsing, protocol policy, URL construction, result/error handling; hosted Explorer default-file association | Store activation and browser dispatch |
| Printing | Actual print content, limits, commands and lifetime management | OS chooser, pagination against printer capabilities and spooler |
| Theme | Actual Light/Dark resources, controls, accent and current contrast palette | Physically changing the Windows contrast theme |
| Input | Native/editor commands, real XAML handlers; separate UIA and drag drivers | Physical IME and assistive-technology interaction |
| App lifecycle | Actual MainPage close, session decisions, fresh-process restore and writer leases; hosted `PackagedApp` covers OS file activation, normal window close, suspension/resume and V1 data import | Microsoft Store deployment/update service; other Windows builds and shipping architectures |

These distinctions are deliberate. A passing payload or policy test is not a
claim that the corresponding operating-system interaction was exercised.

Hosted CI executes the x64 Release Native AOT App groups and a separate Debug
native-fault job, plus the external UIA and drag drivers in the Release job.
Ordinary syntax and native storage checks run in the Release `Editor` group;
`NativeFaults` only injects failures and verifies editing, recovery and journal
integrity. It does not repeat the general syntax/storage suites. Earlier local validation
used Windows ARM64 Release Native AOT and Debug; the two C++ helpers ran as x64.
This does not establish runtime coverage for every shipping architecture. The
future Markdown renderer is outside the current feature map.
Historical receipts remain under `artifacts/test-runs`, `artifacts/isolated-uwp`
and `artifacts/test-overhaul`; those local artifacts are not checked into Git.

## Coverage map

| Feature family | Primary tests |
| --- | --- |
| Decoding, BOM/EOL, Unicode, streaming bounds | Unit `DocumentTextCodecTests`, `LineEndingTests`; App native storage |
| File language, indentation precedence, filenames, search URLs | Unit language/indentation/filename/search tests |
| Document operation ordering and cancellation | Unit `DocumentOperationTests`; actual save/admission integration |
| Recovery schema, integrity, authority and selection | Unit schema/selector; Headless real storage and metrics faults |
| Open/save/save-as/save-all, metadata-only changes, failure preservation | `DocumentWorkflowsTests` with fixed byte oracles |
| Reload/revert/undo, rename/share, external modification/deletion/move | `DocumentWorkflowsTests` |
| Find/replace, Go To, focus, query options | `EditingUiTests`, existing editor search tests |
| Copy/cut/paste, smart copy and failures | `ClipboardContractTests` |
| Tab/menu/context commands, pending-state admission | `WorkspaceCommandFeatureTests`, actual workspace lifecycle tests |
| Preferences, settings navigation, persisted restart | `PreferenceFeatureTests` |
| Theme/accent/slider, zoom geometry, language, view modes | `AppearanceFeatureTests` |
| Command-line/protocol, picker, recent-file menu, web search, printing | `ShellWorkflowTests` |
| Session restore, backup, close/save, import/transfer and recovery | Existing controller/storage/recovery tests plus actual MainPage phases |
| Cold/warm external activation ordering, selected startup tab, close during activation | `WorkspaceActivationTests`, `SessionControllerFileLookupTests`; hosted `PackagedApp` OS activation |
| Restored viewport on the first paint, RTL historical canvas/panning, wrapping, focus, tab reattachment and long `.LOG` append | `ScrollRestoration` App group |
| Process leases and abrupt exit | Two packaged processes in the Leases group |
| Installed Production startup, OS activation, window preference, shutdown, suspension and V1 import | Hosted-only `PackagedApp` external driver |
| Native indentation, syntax, diff geometry, regex | Editor suite and native C++ helpers; Debug-only fault lane |
| RTL/LTR paragraph shaping, visual navigation, mixed selections and wrapping | `NativeBidi` independent DirectWrite geometry/raster; real-control `Bidi` App group |
| Per-document reading direction, commands and persisted view state | `ReadingOrder` App group; external UIAutomation physical input |
| Accessibility/input/drag | Native UIA navigation; separate desktop UI Automation and drag groups |
| Localization, package content, dependency architecture | Dedicated integration groups and architecture negative probes |
| Source copyright headers and third-party notices | `Headers` source group and private Git rejection fixtures |

Tests should protect observable contracts with independent expected results.
Add a reproducer before changing product logic. Avoid duplicate getter checks,
arbitrary sleeps, tests that simply restate an implementation, and performance
thresholds tied to one developer's current machine load.

`NativeBidi` builds the production DirectWrite paragraph implementation in a
headless x64 executable. Its separate whole-paragraph DirectWrite oracle checks
UTF-8/UTF-16 mapping, hit positions, wrapping and offscreen WIC pixels on the same
machine; fixed Hebrew/combining/emoji cases check exact visual caret traversal.
Color/font segmentation, disjoint selection rectangles, direction controls,
uniform fallback-font rows and fixed caret/EOL gutters have dedicated cases.
The production clipped renderer is also compared directly with Windows
`DrawTextLayout` pixels for colour glyphs, drawing effects, italic overhang,
wrapped rows, transformed clips and adjacent tiles. COLRv1 coverage requires
real paint-tree glyphs and chromatic pixels; unavailable OS/font capabilities
are explicitly reported as skipped. A failing inline object verifies error
propagation without retrying a partially drawn paragraph.
The runner saves `native-bidi.json` beside the aggregate test report, with the
assertion count and each optional capability marked `verified`, `skipped` (with
a reason), or `not_run`. Unsupported capabilities do not imply verified coverage.
Optional capability skips are recorded in this artifact; they do not fail the
Native Tests job or CI Gate. Failed assertions still fail the native executable.
All Unicode noncharacters and malformed UTF-8 fixtures also check that font,
representation and caret mappings agree with Scintilla's drawing byte boundaries.
Real Document undo/redo notifications verify restoration edges through insertion,
deletion, coalesced deletes and container actions. The AOT editor tests separately
check actual mixed-boundary caret geometry and subsequent arrow navigation.
Row-specific rendering rejects adjacent Arabic overhang and processes no off-row
drawing effects. A test-only capability facade forces the Context4 branch;
original, privately loaded COLRv0, SVG and PNG fonts verify actual colour pixels,
foreground, opacity, DPI and bitmap snapping against Windows rendering. This
tests the legacy branch on the current OS, not an older Windows installation.
Foreground changes use an immutable layout. Independent Windows pixel references
verify partial Arabic ligatures, combining characters and emoji sequences without
moving glyphs or changing wrapping. The matrix includes affine transforms, GDI
measurement, overlapping translucent colour glyphs, wrapped rows and clipped
italic ink. Same-layout redraws and invalid brush spans cover resource lifetime
and error boundaries; no renderer observer or test switch is compiled into the app.
The small checked-in fonts never require installation or Python to run tests;
regenerate them with `uv run --no-project --with fonttools==4.60.1 python
tests/IntegrationTests/Support/NativeBidi.Tests/GenerateFonts.py`.
The App group checks actual editing, rectangular selection, undo, scrolling and
UIA geometry. First-paint checks cover detached loads, restored selections and
distant diff hunks; synchronous font/zoom changes must also preserve rectangle
collapse corners before any intervening paint can repair the geometry.
Live RTL viewport checks start with unmeasured distant long rows and verify
each painted frame through width growth, panning, resize, gutter and font changes.
They reject geometry queries that repair an already-painted frame, and separately
cover immediate offscreen queries, explicit physical offsets and LTR controls.
Close-hit APIs reject gutters on both the ASCII LTR fast path and mixed-direction
paragraphs; vertical arrows preserve their column across cold canvas growth.
Immediate editing/query checks run before any paint or lexer reset; separate
pending-tokenization fixtures check the same observable results. Actual-control
byte-boundary tests also paint noncharacters, truncated UTF-8 and styles split
inside scalars without altering the original bytes.
Command regressions pair plain and mixed paragraphs to preserve newline and word
stops in both reading directions, and cover subword stops in LTR text.
Rectangle Tab checks one insertion
per visual row across logical islands, reversed selections and wrapping, with
tabs/spaces, read-only mode, subsequent typing and exact undo/redo results.
Read-only Tab preserves text, every oriented selection and the rectangle corners;
the empty undo/redo history is checked after restoring write access.
Plain LTR soft-wrap checks retain the insertion's caret state while verifying
UIA geometry, single-row scrolling, rectangle extension and exact undo/redo.
External UIAutomation drives real keys and clicks, then compares
screen pixels for Latin orientation, Arabic joining and selection islands. Its
regular CI cases verify that UIA point hits return an empty range at the exact
character, including line-interior hits with either gutter position.
Physical command cases also cover cross-line Ctrl+Right/Shift and rectangle
Tab/Undo, plus overwrite input across selected rows, empty tab rows and mixed
islands. Repeated input checks that overwrite resumes after selection replacement;
exact row carets before EOF also reject stale text-store selection corrections.
Physical soft-wrap controls compare typed affinity with an explicitly placed caret.
The App fixtures' `TypeText` uses native paste, bypassing `InsertCharacter` and
TSF `SetText`. Physical typing supplies the direct bridge regressions; NativeBidi
does not compile the text-store bridge and cannot provide that coverage.
Mid-case child focus requires owned foreground and idle input before
any mutation; runner contracts reject interrupted focus without desktop input.
Keyboard helpers also reject held input before typing or dismissing a context
menu. Only menu dismissal permits focus outside the editor; owned foreground
is still required. Inert runner fixtures verify rejection without sending keys.
The right-click cases check both visual homes of wrapped and mixed-direction caret
boundaries, as well as preserving an existing selection. The drag matrix also
moves/copies a selected Hebrew glyph to a separate process and checks actual
insertion-caret pixels when a held drag moves between the two visual homes of
one byte position, followed by exact insertion and single-step undo.
Optional long-paragraph timings are available by running the built helper with
`--performance`; normal correctness tests have no performance pass/fail thresholds.
DPI verification has two independent layers. NativeBidi compares the production
renderer with Windows at 120/144/192 DPI and changes DPI on the same targets and
layouts through 96 -> 120 -> 144 -> 192 -> 96. Returning to 96 DPI must reproduce
the exact original pixels. This validates rendering, not XAML/system events.

The optional `DpiLayout` desktop group runs six Release AOT fixtures at the current
system scale: LTR/RTL, wrapped/unwrapped editing and actual Diff panes. It checks
native gutter/reveal geometry, scrollbar side, retained viewport/selection/text,
and external UIA range geometry and hit testing. It changes no display setting.
`DisplayDpi` runs those fixtures while changing the Windows Settings scale combo
through 100/125/150/200% and back to the original. It requires exactly one active
display and enabled standard scaling; custom scaling is rejected before mutation.
The runner waits for system/native scale agreement, DPI/geometry events and paint,
keeps UIA ranges alive across transitions, and restores the original scale in
`finally`. Original settings, binary hashes and observations are saved with the
isolated session. An interrupted/killed runner may require manual restoration
using `display-settings.json`; cleanup failure is always a test failure.
Discovery failures close only a Settings window opened by the run; pre-existing
windows are preserved. Runner contracts exercise this cleanup and require held
input to be rejected before the DPI driver changes keyboard focus.

```powershell
./scripts/Test.ps1 -Suite Integration -Group DpiLayout -Platform ARM64
./scripts/Test-EditorDpi.ps1 -PreflightOnly
# Explicitly changes the desktop scale; excluded from defaults and hosted CI.
./scripts/Test.ps1 -Suite Integration -Group DisplayDpi -Platform ARM64
```

Neither a current-scale pass nor a native DPI pass claims a real Windows scale
transition. Single-display scale changes also do not cover monitor-origin or
topology changes; that requires a separate multi-monitor environment.

The optional `BidiPerformance` App group records five open/close cycles for ASCII,
CJK and mixed-direction documents, including 32K paragraphs and 30 long RTL
lines that exercise the visible-page cache. A separately labelled 256K stress
case runs last, with one cycle and fewer actions. `RTL-PERF` CSV
lines contain construction, native load, first paint, first/repeated arrow call time,
insert/undo, selection/clear-selection in wrapped and unwrapped views,
scroll/wrap/resize/direction measurements and private/working-set/
managed/app memory, including observations after disposal and collection.
The selection fixture explicitly sets contrasting active/inactive text colours
before first paint and verifies them afterwards, so machine theme defaults cannot
accidentally bypass the multicolour drawing path.
The probe source also compiles on the pre-RTL baseline; explicit R2L scenarios
measure feature cost, since that baseline does not implement equivalent behavior.
Call times are synchronous elapsed time, not CPU accounting or physical input
latency; native paint completion is distinct from compositor presentation.
The first wrapped selection also includes any pending rewrap caused by XAML
arranging a newly visible scrollbar. Its cost remains recorded; a second
selection/clear pair separately measures the already painted viewport.
Use matched architecture/configuration and interleaved runs for comparisons.

Startup presentation tests observe the native document at its first paint and
the tab headers at their first three XAML frames after Loaded. Native surface
painting can precede XAML Loaded, so it cannot establish the header layout's
readiness. The header checks never wait for the expected scroll offset to appear.

## Copyright header checks

```powershell
# Fast repository-only check; Git and PowerShell 7.4+, no build or UI.
./scripts/Test-Headers.ps1

# Also test the checker against private Git fixtures and record a test report.
./scripts/Test.ps1 -Suite Integration -Group Headers
```

`Headers` is included in the default integration matrix. It checks tracked files
and new, nonignored files, so forgetting `git add` cannot hide a missing header.
Tracked files deleted from the working tree are omitted; missing policy entries
and licenses still fail. Gitignored build outputs are not inspected. A tracked
generated wrapper is inspected like any other checked-in source.

C#, C/C++ (`.c`, `.cpp`, `.cxx`, `.h`, `.hpp`), IDL, `.inc`, and PowerShell
(`.ps1`, `.psm1`) files use the existing 93-hyphen banner, with no leading indent,
two spaces before the copyright/license text and exactly one empty line afterward:

```csharp
// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

```

PowerShell uses `#` instead of `//`. Keep original years/ranges; the checker
validates their syntax and ordering, without forcing the current year. A BOM and
LF/CRLF are accepted. Directives such as `#nullable` follow the header. A file
containing only a banner must still end with the one empty separator line.
This guard checks headers, not body formatting.

[HeaderPolicy.json](IntegrationTests/Support/HeaderPolicy.json) registers precise
third-party paths, reasons and licenses. Existing notices, including modification
credits and source URLs, are verified by SHA-256 over the first specified lines,
joined with LF without a final line terminator; BOM and CRLF differences alone
do not invalidate them. License hashes cover their full LF-normalized text,
including the final newline. Code below the protected header may change freely.
The 16 WinUIEdit files originally lacking a per-file notice have explicit
exceptions tied to their preserved license; adding a leading comment invalidates
that exception. New files in these directories receive no automatic exemption.
Renaming/removing an exception, changing attribution or upgrading upstream sources
requires a deliberate policy review. There is no automatic hash-refresh command.

XML/XAML/resources have not generally required copyright banners in this repo.
The two existing first-party XML notices and four upstream XML notices are
explicitly registered, including the upstream project with an XML declaration.
The guard does not impose new headers on all other project/resource files.

The private fixtures exercise actual Git enumeration and rejection behavior,
including untracked/ignored files, header damage, stale/duplicate exceptions,
license removal, preserved-notice changes and failure propagation through the
test runner. They do not build, register packages or interact with the desktop.

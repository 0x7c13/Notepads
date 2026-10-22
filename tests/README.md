# Notepads tests

Use `scripts/Test.ps1` from the repository root. [CI.md](../CI.md) owns workflow
configuration, runner setup, Production package checks and hosted reproduction.

| Project / host | Purpose |
| --- | --- |
| `UnitTests` | Pure production logic; no files, native libraries, UI or services; independent xUnit fixtures parallelize. |
| `IntegrationTests` / Headless | Real storage, process-isolated metrics faults and architecture checks. |
| `IntegrationTests` / App | Actual production pages, resources, native editor and workspace workflows in an isolated Windows package. |
| `IntegrationTests` / Lifecycle | Real instance router, App/bootstrap and OS prelaunch/suspend/activation; same isolated package and storage boundary. |
| Integration `Support` and scripts | Native C++ oracles, external keyboard/UIA/drag drivers and the hosted installed-app driver. |

## Run the relevant checks

```powershell
./scripts/Test.ps1                              # Fast default: Unit
./scripts/Test.ps1 -Filter 'FullyQualifiedName~DocumentOperation'
./scripts/Test.ps1 -Suite Integration -List |
    Format-Table Group,Profile,Default,Configuration,Performance -AutoSize
./scripts/Test.ps1 -Suite Integration -Group Documents,Clipboard -Platform ARM64
./scripts/Test.ps1 -Suite Integration -Group Documents -Platform ARM64 -NoBuild
./scripts/Test.ps1 -Suite All -Platform ARM64    # Default correctness matrix
./scripts/Test.ps1 -Suite Integration -Group NativeFaults -Platform ARM64
```

Release Native AOT is the normal App/E2E configuration; `NativeFaults` always
uses Debug. App builds need 2026 UWP/native build tools and the SDK in
`global.json`; [source setup](../CONTRIBUTING.md#how-to-build-and-run-notepads-from-source)
also supports Build Tools without the IDE. `tests/Tests.slnx` contains both projects; Headless and App outputs
are separate. The runner selects matching product assemblies for architecture checks.

Unknown groups and empty Unit filters fail. `-NoBuild` verifies source,
configuration, platform and binary identity rather than accepting stale outputs.
Results/durations live in `artifacts/test-runs`; App phase, exit and preservation
receipts live in `artifacts/isolated-uwp`. Free-form PASS counts are not coverage.
Use `-List` for defaults: desktop, package and performance groups are explicit.
App/desktop runs serialize behind a shared mutex; run physical input and A/B
measurements on an idle desktop, with no concurrent builds during A/B.

## Coverage map

| Change area | Primary checks |
| --- | --- |
| Encoding/BOM/EOL, language, indentation policy, filenames and search URLs | Unit; `Editor`, `NativeStorage` |
| Document admission/cancellation, recovery schema and authority | Unit; Headless `Storage` and metrics-failure groups |
| Open/save/save-as/save-all, reload/revert, external file changes | `Documents`, `FileOpening` |
| Find/replace, Go To, clipboard, native edit commands | `EditingUI`, `Clipboard`, `ClipboardRejectedCut`, `Editor`, `Search` |
| Syntax, native Diff, ICU regex and failures | `Editor`; focused `Syntax`, `Diff`, `NativeDiff`, `RegexText`, `NativeFaults` |
| Tabs, content-derived titles, selected reorder attachment, transfer admission and pending availability | `WorkspaceCommands`, `Activation`, `Workspace`; Unit title/drag geometry |
| Prelaunch, first-paint restore, new-window protocol, process routing, live window preference, delayed activation/shutdown foreground | `AppLifecycle` (explicit locally; E2E CI) |
| Preferences (fresh defaults, preserved explicit values, real Tab), themes/accent, zoom/language, view modes and accessible actions | `Preferences*`, `Appearance`, `ZoomLanguage`, `ViewModes`, `EditingUI` |
| Session restart/import/close, transfer, persistence, recovery and leases | `Workspace*`, `Persistence`, `Leases`, `SessionResilience` |
| First-paint restore, two-axis pixel scrolling/animation, wrap, Diff sync and tab reattachment | `ScrollRestoration` |
| RTL/LTR shaping, geometry, navigation, selection, undo and direction persistence | `NativeBidi`, `Bidi`, `ReadingOrder`, `AutomationNavigation` |
| Real keys/clicks, TSF typing, UIA ranges, raster and cross-process dragging | `UIAutomation`, `DragDrop`; `TabDragging` for rail, modifier clicks, keyboard focus, release continuity/stacking, drag-back, cancellation, RTL/Equal overflow and actual workspace moves |
| DPI geometry / actual Windows scale transitions | `DpiLayout` / opt-in `DisplayDpi` |
| Installed Production launch, file association, windows, suspension and V1 data import | Hosted-only `PackagedApp` |
| Localization, notices, package content and dependency boundaries | `Localization`, `Headers`, `Package`, `Architecture` |
| Runner ownership, toolchain/offline integrity, CI export without rebuilding and registration boundaries | `Runner` (inert OS/install fixtures) |

`*` denotes a family for orientation, not a selectable wildcard; use exact group
names from `-List`. Markdown preview is not implemented and has no completion claim.

NativeBidi uses production native code with independent whole-paragraph DirectWrite
geometry and WIC pixel references. It covers Unicode byte/UTF-16 boundaries,
shaping/fallback, wrapping, visual navigation, rectangles, immutable foregrounds,
color glyphs, clipping/transforms, DPI, gutter geometry, viewport bounds/animation clocks and undo notifications.
Regular CI also requires an optimized **262,144 UTF-16-unit** dense LTR paragraph:
bounded beginning/middle/end samples check navigation, selection and panned pixels.
The complete lazy navigation index is built; native fallback remains separately tested.

App tests exercise the real adapter/control, including immediate post-edit queries,
first paint, restore, rectangles, language, accessibility and focus. External tests
add physical input and independent UIA/pixel observations. These layers complement
each other: NativeBidi does not compile `Editor.cxx` or the TSF bridge; App `TypeText`
uses native paste, so neither is proof of TSF keyboard handling.
Physical typing also queues a selection shortcut and character together, checking
exact text, caret and one-step undo without relying on paint/idle to synchronize TSF.

## Isolation and the meaning of a pass

- Local App/desktop tests register only `Notepads.Editor.Tests`, use private GUID
  data/settings, restore prior registration, verify existing test data/grants and
  validate process identity before cleanup. Lifecycle decline additionally requires
  the framework shutdown event and actual exit matching an independent framework
  control; pending-presentation checks use owned windows and the real App helper.
  Pending broker calls stay session-owned through cleanup. If one never settles,
  its isolated registration stays fenced against replacement by later runs.
  A private test protocol contract permits direct AUMID activation without
  registering the product URI scheme; protocol entry requires an explicit run marker.
  Production/Dev apps and user sessions
  stay outside this boundary. Ordinary hosts must exit cleanly; the drag peer is
  deliberately terminated by cleanup, so its natural exit is not verified.
- Foreground bootstrap acts only on verified owned windows, with guarded caption
  fallback. It never minimizes unrelated apps. Liveness, idle input, foreground,
  child focus and hit targets remain required; interruption stops input rather
  than reacquiring focus mid-gesture. Initial drag mouse-down still lacks the
  actual-cursor recheck used at release; concurrent user input is not supported.
- Hosted-only `PackagedApp` tests the actual temporarily signed Production bundle,
  installed executable and OS entry points. It refuses existing product state
  and removes its package/certificate afterward. Store deployment/update and an
  actual installed V1-binary upgrade are not covered by V1 data fixtures.
- Clipboard/picker/MRU/launcher/printing tests exercise production policy and
  controlled boundaries; they do not certify every OS dialog, permission/provider,
  external application, printer, clipboard history or MRU persistence behavior.
- `native-bidi.json` records optional capabilities as `verified`, `skipped` with
  reason, or `not_run`. A capability skip does not fail CI Gate or imply coverage;
  failed assertions still fail the job. Forced Context4 rendering on a newer OS
  does not establish old-Windows runtime support.
- Native DPI simulation and `DpiLayout` do not change Windows display settings.
  `DisplayDpi` does: it requires one display and standard scaling, rejects custom
  scaling, waits for events/paint and restores in `finally`. Interruption may
  require manual restoration from `display-settings.json`; cleanup failure fails.
  None of these establishes multi-monitor behavior, actual IME or Narrator usability.

```powershell
./scripts/Test.ps1 -Suite Integration -Group UIAutomation,DragDrop -Platform ARM64
./scripts/Test.ps1 -Suite Integration -Group DpiLayout -Platform ARM64
./scripts/Test-EditorDpi.ps1 -PreflightOnly
# Explicitly changes the desktop scale; excluded from defaults and hosted CI.
./scripts/Test.ps1 -Suite Integration -Group DisplayDpi -Platform ARM64
```

## Performance and test design

`BidiPerformance` records load/first-paint, input, navigation, selection, scroll,
wrap/resize and memory through disposal. `SmoothScrollPerformance` measures retained-row
pixel paints, animation CPU and disposal memory. NativeBidi `--performance` adds the broader
256K matrix and scaling observations. Compare matched builds with interleaved A/B
runs. Prepared-document timings exclude activation/disk and are not key-to-photon
latency; sampled memory is not an allocation profile or proof of leak freedom.
External benchmark drivers can request `PerformanceTestWindow`'s isolated readiness
handshake to acquire foreground before timers start; ordinary runs need no handshake.
`Test-AppLifecycle.ps1 -PerformanceRounds 4` alternates normal process startup and
forced prelaunch/suspend/reuse on one Release AOT build. Preparation CPU/memory are
reported separately. CPU admission reads language-independent CIM processor utility;
provider errors or invalid samples fail explicitly. This measures native paint/next render, not GPU presentation,
true cold disk caches or Windows' automatic prelaunch frequency.
Correctness tests have no machine-dependent performance pass/fail threshold.

Protect observable contracts with independent expectations and a reproducer for
product fixes. Avoid mirrored implementation tests, arbitrary settling sleeps,
weakened guards and coverage-only assertions. Observe first-paint state without
queries that repair it; equal-value setters must not be awaited as new paints.

## Copyright header checks

```powershell
./scripts/Test-Headers.ps1                      # No build or UI
./scripts/Test.ps1 -Suite Integration -Group Headers
```

`Headers` checks tracked and new nonignored C#/C/C++/IDL/inc/PowerShell sources.
The first-party banner has 93 hyphens, no leading indent, two spaces before the
copyright/license text, and one blank line afterward. Preserve existing years;
BOM and LF/CRLF are accepted. This checks headers, not body formatting.

[HeaderPolicy.json](IntegrationTests/Support/HeaderPolicy.json) owns exact upstream
exceptions, protected notice hashes and license hashes. No directory-wide skip or
automatic hash refresh is allowed. Reviewed renames/upstream upgrades update the
policy deliberately. XML/XAML/resources require notices only where registered.
Private Git fixtures verify real enumeration and rejection, including ignored/new
files, damaged notices, stale exceptions and missing licenses, without UI or builds.

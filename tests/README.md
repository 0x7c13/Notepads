# Notepads tests

GitHub CI runs the unit, source, headless and native checks automatically, and
publishes/validates Production Native AOT packages for x86, x64 and ARM64.
The required hosted E2E job executes the x64 Release AOT App groups, Debug native
fault tests, external UI Automation and cross-process drag tests. An additional
manual desktop workflow supports dedicated x64/ARM64 machines. Package validation
is not a substitute for executing an AOT app.
See [CI and local reproduction](../CI.md) for commands, artifacts and runner setup.

Two test projects serve different iteration loops. `UnitTests` runs actual pure
production logic without files, native libraries, Windows UI or external services.
`IntegrationTests` runs either its portable Headless host or the isolated Windows
App host. The native C++ executables and drag peer under `Support` are helpers.

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

## Isolation and the meaning of a pass

Only the `Notepads.Editor.Tests` package is registered. The runner restores the
exact prior loose registration, compares pre-existing test LocalState files, and
checks future-access token/metadata preservation. It verifies executable path and
process creation time before terminating an owned process. Production/Dev Notepads,
Visual Studio and user document sessions are outside the runner's scope.

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
| Launcher | Actual command-line parsing, protocol policy, URL construction, result/error handling | Store activation, file associations and browser dispatch |
| Printing | Actual print content, limits, commands and lifetime management | OS chooser, pagination against printer capabilities and spooler |
| Theme | Actual Light/Dark resources, controls, accent and current contrast palette | Physically changing the Windows contrast theme |
| Input | Native/editor commands, real XAML handlers; separate UIA and drag drivers | Physical IME and assistive-technology interaction |
| App lifecycle | Actual MainPage close, session decisions, fresh-process restore and writer leases | Store activation, normal UWP shutdown, OS suspension/resume and desktop window-manager behavior |

These distinctions are deliberate. A passing payload or policy test is not a
claim that the corresponding operating-system interaction was exercised.

Hosted CI executes the x64 Release Native AOT App groups and a separate Debug
native-fault lane, plus the external UIA and drag drivers. Earlier local validation
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
| Restored viewport on the first paint, wrapping, focus, tab reattachment and long `.LOG` append | `ScrollRestoration` App group |
| Process leases and abrupt exit | Two packaged processes in the Leases group |
| Native indentation, syntax, diff geometry, regex | Editor suite and native C++ helpers; Debug-only fault lane |
| Accessibility/input/drag | Native UIA navigation; separate desktop UI Automation and drag groups |
| Localization, package content, dependency architecture | Dedicated integration groups and architecture negative probes |
| Source copyright headers and third-party notices | `Headers` source group and private Git rejection fixtures |

Tests should protect observable contracts with independent expected results.
Add a reproducer before changing product logic. Avoid duplicate getter checks,
arbitrary sleeps, tests that simply restate an implementation, and performance
thresholds tied to one developer's current machine load.

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

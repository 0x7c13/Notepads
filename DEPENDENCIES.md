# V2 dependencies and platform scope

Audit baseline: `c93d3268`, 2026-10-28. Versions below are a dated inventory;
`Directory.Packages.props`, native `packages.config`, and the selected .NET SDK
remain the build inputs. V2 is unreleased.

## Runtime dependencies

| Dependency | Retained version | Actual use / decision |
| --- | --- | --- |
| Microsoft.UI.Xaml | 2.8.7 | WinUI 2 controls, styles and resources. Keep the UWP product on WinUI 2. |
| Microsoft.Web.WebView2 | 1.0.4314.35 | WinUI dependency. Explicitly pinned through Notepads.Controls to fix ARM64 cross-compilation; see below. The editor does not instantiate WebView2. |
| Win2D.uwp | 1.28.3 | HostBackdropAcrylicBrush composition effects and localized system-font enumeration. It is not an unused Markdown dependency. |
| CommunityToolkit.Uwp.Extensions | 8.2.251219 | Visual-tree lookup, tab ancestor bindings, cursor attached properties and color conversion. |
| CommunityToolkit.Uwp.Converters | 8.2.251219 | SetsView visibility and size bindings. |
| CommunityToolkit.Uwp.Helpers | 8.2.251219 | Theme color serialization. |
| CommunityToolkit.Common | 8.2.1, transitive | Shared Toolkit code; resolved once in the product dependency graph. |
| UTF.Unknown | 2.8.0 | Stream-based encoding detection in DocumentEncodingPolicy; upgraded from 2.7.0 with matched stream-detector samples and production decoding regression coverage. |
| Microsoft.Services.Store.Engagement | 10.2307.3001 | Best-effort custom diagnostic events in AnalyticsService. Retained as a package framework; this is not the Microsoft Store client application. |
| Microsoft.Windows.CsWinRT | 2.3.1 | Native editor/Engagement projections, WinRT interop and AOT code generation. |

The first-party project references are Notepads.Controls and
Notepads.Native.Projection. Multiple references to the same NuGet package across
these projects resolve to one version, not multiple runtime copies. There was no
confirmed unused runtime package to remove in this audit. The old Markdown source
is excluded from compilation and is not evidence that its old packages still ship.

WinUI, Win2D, Toolkit, CsWinRT and Engagement matched the latest stable versions
checked on NuGet. Preview Toolkit/CsWinRT packages were not selected.

### ARM64 packaging repair

WinUI 2.8.7 resolves WebView2 1.0.2849.39 without an explicit override. That SDK's
`build/Common.targets` overwrites `EffectivePlatform` with the build host's
`NETCoreSdkRuntimeIdentifier` and only restores x86/x64 from `PlatformTarget`.
An x64 host targeting ARM64 therefore copies the x64
`Microsoft.Web.WebView2.Core.dll` into the ARM64 package.

The actual ARM64 offline artifact from
[CI run 40603847042](https://github.com/0x7c13/Notepads/actions/runs/40603847042)
contains that mismatch; its app, editor, Win2D and loader are ARM64. That run built
the pull request merge (`732a055`) containing the audited branch head. Current
editing does not use WebView2, explaining why the application tests passed.

The explicitly pinned SDK includes the upstream `PlatformTarget == arm64` fix.
This changes the SDK/projection/native wrapper, not the installed Edge browser
runtime. No browser runtime download or new Markdown renderer was added.
`Test-AppPackage.ps1` now checks every packaged EXE/DLL's PE machine and optional
header against the package architecture, as well as all three required framework
dependencies. A matching manifest alone is insufficient. The check deliberately
targets the current pure x86/x64/ARM64 payloads; introducing hybrid images or
out-of-process helper binaries of another architecture requires an explicit
packaging design and corresponding validation.

## Native and build dependencies

| Dependency | Retained version | Decision |
| --- | --- | --- |
| WinUIEdit | imported commit `d9f60dbca65c2af54ced590d873d9438de32ce77` | Maintained source fork; preserve local scrolling, input, rendering and document-state extensions. |
| Scintilla | 5.6.7 | Recently upgraded and validated; keep the vendored version. |
| Lexilla | 5.5.4 | Only 13 lexer source files / 14 modules and shared lexlib are compiled, with the documented CR-newline fix. |
| Windows ICU | OS component | Regex uses `icuuc.dll` and `icuin.dll`, not the newer combined `icu.dll`; servicing follows Windows. |
| DirectWrite / Direct2D / D3D11 | OS components | Text layout, painting and compositing. No separately bundled rendering engine. |
| Microsoft.Windows.CppWinRT | 3.0.260818.1 | Upgraded with cancellation/lifetime AOT regression coverage. Retain the existing C++20 projection headers/PCH; modules remain disabled. |
| Microsoft.Windows.SDK.BuildTools, native project | 10.0.28000.2705 | Updated build tools; packages.config, imports and missing-package guards use the same version. SDK headers and the runtime OS floor are unchanged. |
| Microsoft.Windows.SDK.BuildTools.MSIX | 1.7.260930100 | Override the UWP SDK's implicit 1.7.20250806.1 through Directory.Packages.props for product and test hosts. Other SDK/build/runtime packages retain their existing selection. |
| .NET SDK | global.json: 10.0.401, latestPatch | Local installed patches can roll forward; CI bootstraps the explicit version. Runtime/ILCompiler versions follow that SDK through NativeAot.targets. |
| Windows SDK headers / managed target | 10.0.26100.0 | Compilation target, not the minimum runtime OS. |
| MSVC / UWP libraries | VS or Build Tools 2026, v145 | Required to build; not an end-user installation requirement. |

The managed SDK also injects its own Windows SDK BuildTools/MSIX tasks and
ILLink/ILCompiler packs. These are build inputs, not a second runtime UI stack.
Keep their provenance separate from the native project's packages.config pins.
Do not manually pin runtime packs away from the selected .NET SDK.

Test-only direct packages are xunit.v3 4.0.2, xunit.runner.visualstudio 4.0.1 and
Microsoft.NET.Test.Sdk 18.11.0. The Test SDK update includes a TRX non-BMP
character fix; the existing VSTest/xUnit adapter configuration is retained.
These packages do not ship in Notepads.

### MSIX BuildTools validation (2026-10-31)

The MSIX-only upgrade from 1.7.20250806.1 to 1.7.260930100 passed Production
Native AOT packaging for ARM64, x64 and x86. The side-edge shadow fix was then
included in a second complete packaging matrix. Package checks covered native
payload architectures, framework dependencies, resources/symbols and the absence
of managed assemblies/CoreCLR; regenerated offline distributions passed integrity
and dependency checks. No other dependency or minimum OS version changed.

On the ARM64 Windows 11 host, each architecture's unchanged Production binaries
also passed isolated loose registration, Unicode input, normal close/cold restart
and a version update preserving an unsaved draft. x64/x86 used Windows emulation;
the private package identities avoided replacing installed user applications.
This does not establish signed-bundle installation under the original identity,
physical x64/x86 execution or Windows 10 17763 runtime compatibility. Hosted CI
must still validate the pushed revision.

Installation testing also exposed an existing offline installer defect: supplying
unrelated framework packages through `DependencyPath` causes AppX deployment to
fail when multiple frameworks are missing. `Register-DevLayout` now supplies only
declared framework dependencies. Regression fixtures cover both independent and
dependent frameworks and pass in PowerShell 5.1 and 7.

Local evidence (ignored artifacts, before the bottom-shadow follow-up): CI builds
`ARM64/529d511b68714ca4a39d0fd613f7d6f2`,
`x64/a793ab2b11b74500897f48323d21a840`, and
`x86/2ba82392d7f64d459dc92b84a9fb0d51` under `artifacts/ci/`;
installation reports under `artifacts/package-install-smoke/`:
`ARM64-69e11c1a9cfe4efa82e96049ad5ce0a9`,
`x64-a6d436a6b8de4432a0dad456675ad350`, and
`x86-f1df7e5378494b4da9f9630940761ea5`.

## Platform decision

Preserve the current minimum of **Windows 10 version 1809, build 17763**, and
**x86, x64 and ARM64**. The audit found no demonstrated reason to raise the
minimum or remove an architecture. This preserves the existing compatibility
target; it is not a claim that every OS/architecture combination has been tested.

| Boundary | Current evidence / requirement |
| --- | --- |
| Build and package | CI publishes Production Native AOT for all three architectures. App manifests declare 17763; actual framework packages must also satisfy that floor. |
| Runtime execution | Hosted E2E and installed-Production checks run x64. Local AOT tests run ARM64; isolated Production installation smoke also covers x64/x86 emulation on ARM64. This does not replace physical-machine or memory-heavy x86 validation. |
| Oldest OS | Real Windows 10 17763 execution is still pending. API checks, package manifests and successful Windows 11 tests do not replace it. |
| OS servicing | Microsoft .NET support follows the supported Windows edition/lifecycle. A package's minimum build is not a promise of Microsoft support for every old Windows installation. |
| Installation | Offline artifacts include the required WinUI, VCLibs and Engagement framework packages. The Store client and a separate .NET runtime are not installation prerequisites. AppX deployment/UWP platform components, permitted sideloading and Developer Mode for the loose preview remain prerequisites. |
| Windows Server / stripped images | No support claim from the .NET compatibility table alone; the required UWP/AppX desktop components and actual installation must be validated. |
| x86 limits | Preserve x86, but include memory-heavy open/save/undo/recovery scenarios in its runtime validation; a successful linker does not establish headroom. |

## Follow-up decisions

1. Evaluate **C++/WinRT C++20 modules** only as a separate build-time experiment.
   The 3.0 package upgrade below retains projection headers/PCH. Modules require
   changes to PCH/import ordering and generated-XAML integration; benefits from
   sharing modules across native projects do not establish a benefit for this
   single native component. Measure before adopting them.
2. **UTF.Unknown 2.8.0** changes byte/span detection batching to match stream
   detection. The maintenance upgrade is now applied. Notepads still calls
   DetectFromStream; no file-opening speed or detection-accuracy gain is claimed.
3. Preserve OS/architecture scope while collecting the missing runtime evidence.
   Raising the minimum or dropping x86 is a product decision requiring coordinated
   project, manifest, installer and CI changes.
4. NuGet transitive resolution is not locked. Keep this limitation explicit;
   adding lock files must account for all target RIDs and the existing SDK patch
   roll-forward policy instead of freezing mismatched AOT/runtime packs.

## Validation of the packaging repair

Local validation on 2026-10-28 passed:

- Production Native AOT publish, compiled architecture checks, package checks
  and offline `-CheckOnly` for x86, x64 and ARM64. All 15 main EXE/DLL images have
  the declared architecture; framework dependencies and the OS floor are unchanged.
- The old SDK's real targets reproduce x64 selection for an ARM64 target on an
  x64 host. The pinned targets select correctly for all six x64/ARM64 host and
  x86/x64/ARM64 target combinations.
- Runner and Headers, including 21 package cases. The old validator accepts the
  archived bad ARM64 payload and the wrong-machine/header/missing-WinUI fixtures;
  the new validator rejects them. Windows PowerShell 5.1 also validates the fixed
  ARM64 upload package.
- Release ARM64 AOT Editor, Appearance, Preferences and ViewModes, with successful
  isolated-package/data preservation. These cover existing UI behavior, not an
  embedded browser feature.

Untouched native files still emit unused-parameter and numeric-conversion
warnings during full rebuilds; the new App host build had no warnings. This
change is not a performance experiment or proof of oldest-OS/x86 runtime support.

### Maintenance updates after the audit

UTF.Unknown 2.7.0 -> 2.8.0, native Windows SDK BuildTools
10.0.28000.1839 -> 10.0.28000.2705 and Microsoft.NET.Test.Sdk
18.10.1 -> 18.11.0 are now applied. The separate C++/WinRT upgrade follows below.

- New and old UTF.Unknown assemblies give identical stream results for 91
  samples, including truncated input, malformed UTF-8, random bytes and chunk
  boundaries. Compared fields include the ordered candidate encodings,
  confidence, BOM status and consumed length. All ten complete multilingual
  samples decode exactly; this is a fixed corpus, not universal encoding accuracy.
- `Persistence` now checks ten known texts through the actual AOT file-loading
  pipeline: ASCII/UTF-8, GBK, GB18030 with supplementary characters, Big5,
  Shift-JIS, EUC-JP, Korean, Windows-1251 and Windows-1252. It checks canonical
  content, retained encoding/EOL and unchanged source bytes, alongside the
  existing BOM, explicit/default encoding, empty-file and late-error fallback cases.
- The resolved product graph changes only UTF.Unknown; the upgrade adds no new
  transitive packages. The native project's evaluated tool paths use the new
  BuildTools package; SDK headers remain 26100 and the OS floor remains 17763.
- All 608 Unit tests pass using Test SDK/TestHost 18.11.0 and the existing xUnit
  VSTest adapter. Runner, Headers, Localization, storage/metrics and architecture
  self-tests pass. All three Production AOT publications and offline checks pass.
- Release ARM64 AOT Persistence, Editor, Documents, FileOpening,
  ScrollRestoration, WindowMove, Appearance, Preferences and ViewModes pass,
  including all nine isolated data-preservation checks. NativeDiff, NativeBidi
  (1,248,315 checks) and RegexText pass on the local x64 test executables.

The maintenance-update Production and App build logs have no compiler warnings
or errors. Unit compilation retains five xUnit1051 cancellation-token warnings
already present in the 2026-10-27 baseline; no analyzer suppressions were added.
No performance A/B, oldest-Windows run or x86/x64 product execution was performed.

### C++/WinRT 3.0 upgrade

Microsoft.Windows.CppWinRT 2.0.250303.1 -> 3.0.260818.1 changes only the native
package pin and its matching props/targets imports and missing-package guards.
Notepads already uses standard C++20 coroutines, so removal of pre-standard
`/await` support needs no product adaptation. CsWinRT, UWP/WinUI 2, SDK headers,
the OS minimum and supported architectures remain unchanged.

The SDK fixes races in cancellation registration/revocation and overlapping
Cancel calls. Ordinary WinRT `co_await` can use this machinery as well as
`resume_after` / `resume_on_signal`, but registration requires cancellation
propagation, which is disabled by default. The retained native source has six
explicit `enable_propagation(false)` calls and no enabling call or direct use of
those two awaiters. No current Notepads failure attributable to this SDK race
has been reproduced.

Keep the existing cancellation flags, owner-apartment cleanup and read-lease
draining. The SDK fix does not replace application ownership: a canceled load,
restore, journal or Diff/regex job must still stop using its resources before
its completion permits reuse or disposal.

Validation on 2026-10-28, using the isolated upgrade candidate:

- Production Native AOT publish, architecture/PE/framework checks and offline
  `-CheckOnly` pass for ARM64, x64 and x86.
- Release ARM64 AOT Editor, NativeStorage, Persistence, WindowMove and Search
  pass, including isolated-data preservation. Coverage includes finalization,
  cancellation/restart, blocked reads retaining leases until completion, journal
  and recovery publication, undo/selection transfer and search admission.
  WindowMove uses a test launcher; this is not a new Windows-broker input test.
- ARM64 WinRT metadata is byte-identical after excluding the build MVID. The
  native DLL adds no static system API import. The full native build retains
  exactly the same 192 warning instances as the 2.0 baseline.
- A standalone probe against each SDK's actual generated headers uses semaphore
  barriers for two cancellation interleavings. Each scenario fails all 1,000
  repetitions with 2.0 and passes all 1,000 with 3.0. This confirms the SDK fix;
  it is not an application reproduction or exhaustive race coverage.

Separate SDK review found that `Cancel()` and the canceled `await_transform()`
path call the `originate_on_cancel()` setter as a getter. This resets the optional
false policy and may introduce concurrent flag writes if those paths overlap.
Notepads does not use the opt-out. Its externally canceled SDK coroutines have
no ordinary await after their caller-visible suspension; multi-await workers
receive neither direct nor propagated cancellation. No current product path to
the overlap was identified. Reassess this upstream issue before adding later
awaits to those public operations or adopting the opt-out; disabling propagation
alone would not address this separate flag-access issue.

The adopted configuration files are identical to that candidate. After adoption,
the main worktree also passes ARM64 Production AOT/offline checks, Release AOT
Editor and NativeStorage, and Headers. Retaining the old build directories
correctly regenerates 3.0 headers in both Production and Release without a clean.

The generator's file-reading improvements concern build time; no startup,
scrolling, memory or build-speed improvement is claimed without performance A/B.
Windows 10 17763 and x86/x64 product execution remain separate validation gaps.

## WinUIEdit adapter maintenance audit

The 2026-10-29 audit covers the retained `WinUIEditor` control, Scintilla's
`winui` backend and the local CallTip drawing override. A TODO is not itself
evidence of a reachable Notepads defect. The following distinctions matter:

| Area | Notepads V2 impact and disposition |
| --- | --- |
| UIA `TextRangeProvider::FindText` | Was advertised through Text ranges but threw `E_NOTIMPL`. Implemented counted, bounded native search, including Unicode, case folding, reverse search and embedded NUL. It leaves selection, target/search settings, viewport and edit state unchanged. |
| UIA query encoding | A counted encode/decode round trip rejects lossy substitutions, best-fit mappings and malformed UTF-16 before searching. This prevents legacy-codepage queries such as `漢` or fullwidth `Ａ` from matching unrelated `?` or `A` bytes. Validation copies only the query; Notepads continues to use UTF-8 internally. |
| Native literal-search bounds | Review found that the forward, case-sensitive fast path could scan candidate starts too close to the range end, then match outside it or treat EOF as a real NUL. Restrict candidate scanning to positions that fit the entire query. Native tests compare every subrange and buffer-gap position of an ASCII/NUL fixture with standard-library search; UIA tests also cover CJK, exact-end matches and real versus artificial NUL. |
| UIA `CompareEndpoints` | A 64-bit byte difference was narrowed to 32 bits and could reverse ordering. Return only the sign, as the UIA contract permits. This is preventive library hardening, not a reproduced failure within Notepads' current Int32 document admission. |
| UIA localized control type | The English-only `code editor` override prevented platform localization. Use XAML's localized standard Edit type. Multiple-selection range text is also exercised directly; the old unspecific crash comment is removed. |
| CallTip drawing | The WinUI chevron implementation returned before the retained Win32 triangle drawing. Remove the unreachable code and its unused argument; keep the existing chevron output. Notepads does not currently invoke `CallTipShow`. |
| Autocomplete/CallTip TODOs | Font selection, images, list matching, popup placement, live theme changes and CallTip automation remain incomplete generic-control features. Notepads does not invoke `AutoCShow` or `CallTipShow`; syntax highlighting uses the independent Lexilla path. These need feature-specific work before exposing completion/help popups. |
| Win32 menu/monitor and desktop TSF stubs | These are not equivalent to missing Notepads menus or all IME input being unimplemented. The app supplies a XAML context menu and UWP uses CoreText. Remove the commented, non-compiling Win32 body of `GetACPFromPoint`; keep its explicit unsupported result. Rich embedded objects are outside the plain-text editor contract. |
| IME/focus/device-loss comments | Some describe older implementations; the current CoreText composition transaction, finalization and selection notifications have dedicated tests. Do not infer that every keyboard/IME/device scenario is solved, or remove lifecycle work merely because an old TODO looks stale. |
| Capability-query caching / WinUI 3 notes | Per-instance `ApiInformation` caching is a possible small startup cleanup, not an established performance bottleneck. WinUI 3 shutdown notes do not describe the retained UWP product. |
| Scintilla core notes | `SafeSegment` still has an upstream grapheme-boundary limitation when splitting long shaping segments; this is not an unimplemented UTF-8 decoder. `BraceMatch` does not extend pending styling itself. `UndoHistory::Text` assumes enumeration restarts at action zero; the retained state readers follow that contract. These need targeted reproductions before changing core layout, syntax or undo semantics. |

Remaining reachable accessibility work includes `FindAttribute`, honoring
`ScrollIntoView(alignToTop)`, the composition/conversion methods of the currently
advertised TextEdit pattern, and a consistent memory policy for full-document
Value/Text reads. Annotation and embedded-child behavior need explicit unsupported
contracts if those features are ever introduced. No arbitrary truncation or stale
viewport cache has been added to hide these gaps.

The clean ARM64 Release baseline emitted 192 native compiler warning instances;
the final rebuild emits 23. Clean x64 Release, Win32 Release and ARM64 Debug
builds also pass, with 23, 22 and 23 warning instances respectively. Counts
exclude repeated compiler diagnostic continuation lines.
Cleanup keeps `/W4`: unused callback/stub parameters are unnamed, shadowing locals
are renamed, WinRT array indexes use their 32-bit size type, bounded notification
lengths are narrowed in the generator, and XAML/Direct2D geometry conversions are
explicit. The generator remains the source for generated wrapper changes.
Remaining TSF position/rectangle narrowing needs a separate overflow policy;
casting away those warnings would not establish correctness. No blanket warning
suppression, API/ABI change or scrolling-performance improvement is claimed.
Win32 also retains pre-existing 64-bit WinRT-to-native-position conversion
warnings in range reads and regex/editor adapters; the architecture build is
not a claim that every narrowing site has been eliminated.

UIA search is synchronous. Candidate and suffix comparisons can take O(n * m)
time in the worst case, where n is the range length and m is the encoded query
length. Temporary search storage scales with the query, not the entire document;
this is not an asynchronous or time-bounded large-document search service.

Validation before the literal-search boundary follow-up passes ARM64 Release AOT AutomationNavigation,
Editor, Bidi and ScrollRestoration, plus the Headers suite. The added UIA tests
cover bounded/reverse search, Unicode case folding, CJK/emoji, embedded NUL,
stale ranges, multiple selections and preservation of editor state. Running the
interface generator twice leaves all three checked-in generated files unchanged.
These checks do not substitute for physical IME/Narrator testing, x86/x64 product
execution or performance measurements. Local receipts are under
`artifacts/winuiedit-audit`, with runtime results in
`artifacts/test-runs/f1ed73cbe5a74bf783ffa9955f9903cb/results.json` and Headers in
`artifacts/test-runs/1656309557c54314bd2a7c646a3c2436/results.json`.

The literal-search boundary follow-up first reproduced the old defect in the
native suite (`range=0:2`, forward/case-sensitive, expected no match but returned
position 1). After the fix, all 48,420 new native checks pass, along with the
complete x64 NativeBidi suite, ARM64 Release AOT AutomationNavigation and Search,
and Headers. Results are recorded in
`artifacts/test-runs/9e5738597dea45d2b6b5a0f1282ea8c1/results.json`; the negative
and corrected run logs are `artifacts/winuiedit-audit/search-boundary-before.log`
and `search-boundary-after.log`. Independent review found no remaining issue in
the boundary fix or its regression tests.

The query-encoding follow-up passes ARM64 Release AOT AutomationNavigation and
Search, plus Headers, recorded in
`artifacts/test-runs/5e3194099b494c3a8b3f6a976f7766b1/results.json`. The generic
control fixture uses raw CP1252 bytes to test rejected substitutions/best-fit
queries, valid accented letters, case modes, reverse search and embedded NULs.
UTF-8 coverage distinguishes malformed UTF-16 queries from an actual replacement
character. Independent review also confirmed the fix without new findings.

Contract references: [UIA text range requirements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingtextandtextrange),
[standard control-type localization](https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.automation.peers.automationpeer.getlocalizedcontroltypecore).

## Rechecking the inventory

Restore using the repository build scripts, then inspect the resolved graph,
not only direct PackageReference entries:

```powershell
dotnet list src/Notepads/Notepads.csproj package --include-transitive --no-restore
dotnet list src/Notepads/Notepads.csproj package --vulnerable --include-transitive --no-restore
dotnet list src/Notepads/Notepads.csproj package --deprecated --include-transitive --no-restore
dotnet list tests/Tests.slnx package --vulnerable --include-transitive --no-restore
```

The baseline queries on 2026-10-28 reported no known vulnerable packages in the
resolved product/test graphs and no deprecated packages in the product graph.
This is a NuGet advisory result, not a scan of vendored native source, Windows
components, unused package versions or all possible vulnerabilities. Repeat it
for release candidates. Retained upstream source licenses remain under
Notepads.Native; resolved NuGet metadata supplies package license/provenance data.

Primary references: [WinUI](https://www.nuget.org/packages/Microsoft.UI.Xaml),
[Win2D](https://www.nuget.org/packages/Win2D.uwp),
[Toolkit](https://www.nuget.org/packages/CommunityToolkit.Uwp.Helpers),
[UTF.Unknown release](https://github.com/CharsetDetector/UTF-unknown/releases/tag/v2.8),
[C++/WinRT 3.0](https://github.com/microsoft/cppwinrt/releases/tag/3.0.260520.1),
[C++/WinRT pinned release](https://github.com/microsoft/cppwinrt/releases/tag/3.0.260818.1),
[cancellation fix](https://github.com/microsoft/cppwinrt/pull/1609),
[WebView2 SDK](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4314.35),
[Windows SDK BuildTools](https://www.nuget.org/packages/Microsoft.Windows.SDK.BuildTools/10.0.28000.2705),
[Test SDK 18.11 release](https://github.com/microsoft/vstest/releases/tag/v18.11.0),
[.NET Windows support](https://learn.microsoft.com/en-us/dotnet/core/install/windows),
[Windows ICU](https://learn.microsoft.com/en-us/windows/win32/intl/international-components-for-unicode--icu-).

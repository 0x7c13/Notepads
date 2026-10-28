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

## Platform decision

Preserve the current minimum of **Windows 10 version 1809, build 17763**, and
**x86, x64 and ARM64**. The audit found no demonstrated reason to raise the
minimum or remove an architecture. This preserves the existing compatibility
target; it is not a claim that every OS/architecture combination has been tested.

| Boundary | Current evidence / requirement |
| --- | --- |
| Build and package | CI publishes Production Native AOT for all three architectures. App manifests declare 17763; actual framework packages must also satisfy that floor. |
| Runtime execution | Hosted E2E and installed-Production checks run x64. Local AOT tests run ARM64. x86 package builds are not x86 runtime coverage. |
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

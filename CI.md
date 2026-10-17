# V2 continuous integration

V2 is unreleased. GitHub Actions validates changes and produces unsigned
Production Native AOT packages. Release signing, tags, GitHub Releases and
Microsoft Store submission are deferred. Installed-app tests sign a copy with a
temporary certificate on their disposable VM; no publishing credentials are used.

## Automatic checks

[ci.yml](.github/workflows/ci.yml) runs on pull request updates, pushes to `master`,
merge queues and manual dispatch. Feature branch pushes are checked through their
PR instead of starting a duplicate run. New runs cancel superseded runs on the same
PR/ref. There are no path filters that could leave a required check pending.

| Job | What must pass |
| --- | --- |
| Managed Tests | actionlint; all pure unit tests; runner/header/localization checks; real-storage recovery tests; isolated metrics-failure processes; architecture guard negative fixtures |
| Native Tests | NativeDiff, NativeBidi and RegexText C++ executables, Release x64 |
| Native AOT / x86, x64, ARM64 | Solution restore; Production AOT publish; compiled managed-assembly dependency guard; unsigned bundle/upload-package validation; x64 also installs and exercises the Production package |
| E2E Tests | x64 Release AOT App tests, external UI Automation and cross-process drag tests on a hosted interactive desktop |
| Resilience Tests | Dedicated Debug host: optional style/lexer allocation failures, editing/undo recovery and journal integrity |
| CI Gate | Every job above succeeded, including every architecture; failure, cancellation or an unexpected skip fails the gate |

Configure **`CI Gate`** as the required branch/ruleset check. Managed Tests and
Native Tests run in parallel first. Only after both succeed do the three Native
AOT architectures, E2E Tests and Resilience Tests start in parallel. A failed prerequisite skips
these heavier jobs and fails the gate. Once started, a job's failure does not
cancel its siblings; the package matrix also continues other architectures when
one fails. Unit fixtures parallelize inside their runner; source and headless
groups share one build.
The desktop job matrix gives E2E and fault injection separate VMs and builds.
E2E runs all default Release App groups from `Test.ps1 -Suite Integration -List`,
plus `UIAutomation` and `DragDrop`. The Debug lane runs only the default fault
group, without repeating ordinary syntax or storage tests; those already run in
the Release `Editor` group. Each desktop runs its groups sequentially. The matrix
does not cancel one lane when the other fails, and CI Gate requires both.
Package jobs do not build the test host again.

The Windows image is `windows-2025-vs2026`, with Visual Studio 2026/v145 and Windows
SDK 10.0.26100.0. `global.json` selects .NET; `Initialize-BuildTools.ps1` checks the
actual compiler/UWP library paths and installs missing native components on hosted
runners. NuGet downloads are cached against project/dependency inputs. Build
outputs and test results are never reused across hosted jobs. Dependencies are
centrally versioned, but the repository does not yet lock transitive NuGet
resolution. The runner image itself receives upstream updates.

Workflow permissions are `contents: read`, checkout does not persist credentials,
and actions are pinned to full commit SHAs. actionlint's downloaded release archive
is SHA-256 verified before execution. The PR workflow does not use
`pull_request_target`, repository secrets or a self-hosted runner.

## Artifacts and package checks

Artifacts are retained for 14 days, including available diagnostics after failure:

- `test-results` and `native-test-results`: structured group results, timings,
  test logs and unit TRX results under `artifacts/test-runs`.
- `native-aot-<architecture>`: logs, publish binlog, source fingerprints,
  `results.json`, unsigned bundle/upload package and publishing symbols under
  `artifacts/ci/<architecture>/<run-id>`.
- `desktop-<architecture>-<attempt>`: desktop group reports, phase results,
  input-driver evidence, lease/peer protocol diagnostics (including crash reports),
  and package/data preservation reports. Copied application `layout/` directories
  are excluded.
- `ui-test-results`: hosted desktop environment, test/phase results, preservation
  reports and input-driver protocols. Failure diagnostics include an available
  desktop screenshot and recent application/AppModel/deployment event logs.
- `native-fault-results`: the separate Debug host's build log, fault-injection
  results, preservation reports and the same desktop failure diagnostics.

`Build-CI.ps1` creates a unique output directory and requires exactly one fresh
upload package. `Test-AppPackage.ps1` checks the manifest architecture set,
declared executable, current framework dependencies/minimum Windows version,
publishing symbol archive, and every packaged EXE/DLL for a CLR header. CoreCLR
and JIT payloads are rejected. The upload package's size and SHA-256 are recorded.
This checks symbol presence, not source indexing or successful crash symbolication.
The managed architecture guard inspects the actual compiled product assembly
before AOT rather than a separate source-only model.

The build preserves the current x86/x64/ARM64 scope and Windows 10 build 17763
minimum. A platform change must update projects, package assertions and CI
together. Generated unsigned packages are validation artifacts. The x64 job
installs a test-signed copy; this does not verify Store certification. Artifacts from a PR remain
untrusted build output and are not automatically promoted to a release.

## Hosted UI and AOT runtime validation

The x64 package job also runs the explicit `PackagedApp` integration group against
the just-built Production upload package. It installs a temporarily signed bundle,
preserving the original identity, manifest and executable. SHA-256 checks verify
that signing changed only signature footprint metadata and that the installed
executable matches the archive. A loose development registration is rejected.

An external driver exercises startup, Unicode editing/saving, cold and warm
Windows file activation, Explorer default-file association, duplicate-file reuse,
new-window preferences before and after restart, normal close/recovery, OS suspend/resume, termination after a
completed suspension, and one-time import of V1 UTF-8/UTF-16 recovery fixtures.
Assertions observe actual editor text through UI Automation, tab membership,
process/window identity, saved file bytes and OS package execution state. Product
test hooks and in-process calls to application activation handlers are not used.

The isolated App tests additionally force startup races found by this work:

- `SessionControllerFileLookupTests` holds a matching imported file while startup
  restores the saved selected tab. File lookup must not replace the saved selection
  or make first-document readiness depend on a different file's I/O.
- `WorkspaceActivationTests` holds recovery and the first activated file, then
  submits another file/command request. Arrival order and final selection must
  survive both barriers. Canceling a close during startup must resume processing;
  actual disposal must settle queued requests even with a file-error dialog open.
- A delayed default blank document must not add a tab or steal selection from a
  file opened while the blank document's journal was being created. The unused
  editor is released; ordinary explicit new-document commands are unaffected.

The page queues only external activation requests. Normal file commands retain
their existing admission/coalescing behavior. Initial navigation enqueues without
waiting, allowing `App` to activate the window; file lookup priority remains
separate from the saved session selection.

`Test-PackagedApp.ps1` refuses non-hosted machines and pre-existing Notepads
registrations, processes or data. Its cleanup removes only its installation and
temporary certificates; cleanup failures fail the test. Evidence under
`artifacts/packaged-app/<run-id>` includes per-case results, driver logs, process
exit codes and failure UIA/screenshot diagnostics, uploaded with `native-aot-x64`.
Normal-close tests reject abnormal process exits even if recovery data is intact.
UWP's close exit code `1` is accepted only for a window the driver requested to
close; that code without a close request, and crash exit codes after one, fail.
Any lingering closed processes are terminated through retained handles with an
explicit exit code; a concurrent crash cannot be accepted as that termination.
The private signing key
is non-exportable and never uploaded. No product installation occurs locally.

Windows `ActivateForFile` tests the OS file activation contract. A separate case
chooses Notepads in Windows default-app settings and verifies the association
before invoking a file's default action in Explorer (the double-click path).
The default-app picker uses keyboard navigation because its immersive window
does not expose UIA descendants on the hosted image. An unexpected initial
handler, picker layout or association readback fails the test.
This does not exercise the Microsoft Store installation/update service.
Migration uses actual V1 data contracts with the installed V2 executable,
not an installed V1 binary upgraded through the Store. These distinctions must
remain visible when reporting a pass.

The automatic **E2E Tests** job uses its own disposable GitHub-hosted Windows VM.
`Initialize-CIDesktop.ps1` verifies an interactive session, enables Developer Mode,
and registers the restored WinUI framework and SDK VC runtime packages. It refuses
to change a local or self-hosted machine. It also removes the foreground-lock delay
on its disposable desktop so the input driver can switch between verified test
windows. An adequate newer framework is retained.
`Test-CIUI.ps1 -Lane E2E` selects all default Release App groups plus the two
external input groups. `-Lane Faults` selects the default Debug group on a separate
VM. Unsupported configurations or an empty lane fail instead of silently reducing
coverage. The dedicated `--native-faults` phase rejects Release execution because
the native injection hooks are compiled only in Debug.
The hosted driver retains the runner's elevation to arrange the UWP frame. Only
the desktop drag peer uses a SAFER normal-user token with medium integrity; it
records and checks its own privilege state before opening its window. No UAC
split token is required. Retained process handles preserve exact ownership and
exit-code checks, including when observing UWP without administrator rights.
The existing isolated runner then registers and activates only `Notepads.Editor.Tests`.
UI test failures fail `CI Gate`; tests are not silently skipped or retried into
success. Superseded hosted runs may be cancelled because their VM is disposable.

This validates the x64 test host's actual AOT execution and tested UI/input
behavior. It does not certify the packaged Store app or prove ARM64/x86 runtime
behavior. Actual IME behavior, physical high contrast, screen-reader usability,
Store activation and other OS boundaries still need targeted release QA; see the
[coverage boundaries](tests/README.md#isolation-and-the-meaning-of-a-pass).

## Optional self-hosted desktop validation

[desktop.yml](.github/workflows/desktop.yml) additionally provides a **manual**
workflow for a dedicated Windows desktop, including native ARM64 runtime testing.
It is restricted to this repository's
`v2` and `master` refs and uses the `desktop-tests` environment. It is intentionally
not triggered by pull requests. Only dispatch reviewed commits; do not give this
runner access to personal files, production credentials or other repositories.

Runner setup:

1. Install PowerShell 7.4+, Git, the `global.json` SDK, Visual Studio 2026 with
   .NET UWP tools, Windows SDK 10.0.26100.0, and v145 native/UWP tools. Run
   `./scripts/Initialize-BuildTools.ps1` to verify the native toolchain.
2. Enable Windows Developer Mode and prepare the test package's framework
   dependencies. Use the [test guide](tests/README.md) to validate the isolated App
   tests locally on this runner before enabling workflow dispatch.
3. Register labels `self-hosted`, `Windows`, the actual architecture (`ARM64` or
   `x64`) and `notepads-desktop`. Run the current Actions runner interactively in
   the logged-in user session, not as a Windows service. Keep that desktop
   unlocked and exclusive while tests run.
4. Create the `desktop-tests` environment with allowed branches `v2`/`master` and
   required reviewer protection. Restrict the runner to trusted workflows where
   runner-group controls are available. Environment rules and branch protections
   are repository settings, not created automatically by this change.
5. Dispatch **Desktop integration**, choosing the runner's native architecture.
   Enable `input-tests` to include external UI Automation and cross-process drag.

The default desktop run executes `Test.ps1 -Suite All`: real Release Native AOT
App tests, headless/native checks, and a separate Debug native-fault lane. The
optional input step runs both external drivers. Desktop runs serialize, and new
dispatches do not cancel running tests midway through preservation cleanup.
An absent/offline matching runner leaves this manual job queued. The automatic
hosted UI job does not require this runner or the `desktop-tests` environment.
Both paths verify preservation of the test package's prior registration and data.

## Reproduce locally

Run from the repository root in PowerShell 7.4+:

```powershell
./scripts/Test-Workflows.ps1
./scripts/Test.ps1 -Suite All -Configuration Release -Group Runner,Headers,Localization,Storage,MetricsInitialization,MetricsCounter,MetricsDuration,ArchitectureSelfTest
./scripts/Test.ps1 -Suite Integration -Configuration Release -Group NativeDiff,NativeBidi,RegexText -Platform x64

# Production AOT, unsigned upload bundle and symbol/package checks. No deployment.
./scripts/Build-CI.ps1 -Platform ARM64
./scripts/Build-CI.ps1 -Platform x64
./scripts/Build-CI.ps1 -Platform x86

# Dedicated interactive desktop only:
./scripts/Test.ps1 -Suite All -Configuration Release -Platform ARM64
./scripts/Test.ps1 -Suite Integration -Configuration Release -Group UIAutomation,DragDrop -Platform ARM64
```

Run product builds sequentially in one checkout; GitHub matrix jobs use separate
checkouts. Do not substitute an earlier package or `-NoBuild` output for a failed
build. Use the workflow logs and structured reports to distinguish failed checks
from checks that never ran.

## Maintenance and rollout

Dependabot proposes weekly GitHub Actions and NuGet updates targeting `v2`.
The NuGet scan explicitly includes the root, product solution and standalone
test/tool project directories. The solution includes native `packages.config`,
and project imports include central versions. Action updates are grouped;
NuGet updates remain separate for focused AOT compatibility review. There is
no automatic merge. Update actionlint's version and both checksum constants
together after verifying the upstream release.

The repository's default branch is currently `master`. GitHub requires the
manual workflow and Dependabot configuration on the default branch before those
features are enabled. Shipping these files only to `v2` enables CI for its PR,
but does not complete that repository setup. When V2 becomes the default branch,
revisit Dependabot's explicit target and the allowed branch lists.

After changes to runner images or dependencies, verify all three package jobs and
the UI job's actual test reports. Set the required check after it appears.
Provisioning an optional desktop runner, configuring repository protections, signing/versioning policy,
combined release bundles and Store submission remain separate steps. No claim of
cloud execution or runtime coverage should be based solely on local validation.

The old Azure pipeline, tag/release/Store workflow, bulk alert-dismissal workflow,
release templates and screenshot-based setup guide have been removed. Existing
Azure connections, old GitHub secrets and external service settings are not
removed by deleting repository files; maintainers can retire those once no other
project uses them.

References: [Windows runner images](https://github.com/actions/runner-images),
[workflow security](https://docs.github.com/en/actions/reference/security/secure-use),
[manual workflow requirements](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow),
[Dependabot configuration](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference).

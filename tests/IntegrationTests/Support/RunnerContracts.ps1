# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Exercises the real runner guards against private fixtures. No package is
# registered; only private headless child processes exercise exit-code handling.
param([string] $EvidenceRoot = '')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/IsolatedTestPackage.ps1')
. (Join-Path $repo 'scripts/TestBuildIdentity.ps1')
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo ('artifacts/runner-contracts/' + [Guid]::NewGuid().ToString('N')) }
$fixtureRoot = [IO.Path]::GetFullPath($EvidenceRoot)
New-Item -ItemType Directory -Path $fixtureRoot -Force | Out-Null
$checks = [Collections.Generic.List[string]]::new()

function Expect-Rejection([string] $Name, [string] $Message, [scriptblock] $Action) {
    $failure = $null
    try { & $Action | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.Exception.Message -notlike $Message) {
        throw "$Name did not fail with the expected guard ($Message): $failure"
    }
    $checks.Add($Name)
}

# Scope the registration boundary to this script; no OS registration is read or
# changed. The current process is queried only; termination uses an inert guard.
function Get-AppxPackage {
    param([string] $Name)
    if ($Name -ne 'Notepads.Editor.Tests') { throw 'Unexpected package query.' }
    return $probeRegistration
}

$runId = [Guid]::NewGuid().ToString('N')
$sentinel = Join-Path $fixtureRoot 'runner-sentinel.txt'
[IO.File]::WriteAllText($sentinel, $runId)
$ownedRegistration = [pscustomobject]@{ PackageFullName = 'Synthetic.Test'; InstallLocation = $fixtureRoot }
$probeRegistration = $ownedRegistration
$session = @{
    Package = $ownedRegistration; Layout = $fixtureRoot; RunRoot = $fixtureRoot; RunId = $runId; Root = $fixtureRoot; Phase = 0
    PriorDirectories = @($fixtureRoot); Before = @{}
    Processes = [Collections.Generic.List[object]]::new()
}
$session.Before[$sentinel] = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash
Assert-IsolatedTestRunOwnership $session
Assert-IsolatedTestPreservation $session
$checks.Add('Valid owned registration, sentinel and prior data are accepted')

[IO.File]::WriteAllText($sentinel, 'changed')
Expect-Rejection 'Changed sentinel' '*owned test sentinel changed*' { Assert-IsolatedTestRunOwnership $session }
Expect-Rejection 'Changed retained data' '*Pre-existing test data changed*' { Assert-IsolatedTestPreservation $session }
[IO.File]::WriteAllText($sentinel, $runId)
$probeRegistration = [pscustomobject]@{ PackageFullName = 'Foreign.Test'; InstallLocation = $fixtureRoot }
Expect-Rejection 'Foreign registration' '*isolated test registration changed*' { Assert-IsolatedTestRunOwnership $session }
$probeRegistration = $null
Expect-Rejection 'Missing registration' '*isolated test registration changed*' { Assert-IsolatedTestRunOwnership $session }
$probeRegistration = $ownedRegistration

$currentProcess = Get-Process -Id $PID
$null = $currentProcess.Handle
$identity = @{ Id = $PID; Path = $currentProcess.Path; StartTicks = $currentProcess.StartTime.ToUniversalTime().Ticks; Verified = $true; ExpectedTermination = $false; Process = $currentProcess }
$null = Get-IsolatedTestProcess $identity
$checks.Add('Exact current process identity is accepted without mutation')
# Protect the runner itself even if a regression removes the rejection guard.
$killGuard = [pscustomobject]@{ HasExited = $false }
$killGuard | Add-Member ScriptMethod Kill { throw 'A runner contract attempted to terminate its parent.' }
$identity.Process = $killGuard
Expect-Rejection 'Ordinary process cannot be forcibly counted as PASS' '*forced termination cannot count as PASS*' { Stop-IsolatedTestProcess $session $identity }
$identity.Process = $currentProcess
$identity.Verified = $false
Expect-Rejection 'Unverified PID' '*left untouched*' { Get-IsolatedTestProcess $identity }
$identity.Verified = $true
$identity.StartTicks++
Expect-Rejection 'Reused PID with different creation time' '*left untouched*' { Get-IsolatedTestProcess $identity }
$currentProcess.Dispose()

function New-ExitedProcessFixture([int] $ExitCode) {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32/cmd.exe'))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    foreach ($argument in @('/d', '/c', 'exit', [string]$ExitCode)) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($start)
    $retained = $null
    try {
        $retained = [IsolatedTestProcess]::new($process.Id)
        $identity = @{ Id = $process.Id; Path = $retained.Path; StartTicks = $retained.StartTime.Ticks; Verified = $true; ExpectedTermination = $false; Process = $retained }
        if (!$process.WaitForExit(5000)) { $process.Kill(); throw 'Private exit-code fixture timed out.' }
        return $identity
    }
    catch { if ($retained) { $retained.Dispose() }; throw }
    finally { $process.Dispose() }
}

# A live private child verifies wait, identity and termination through the same
# limited handle. The exited-child fixtures also dispose the launcher object.
$childStart = [Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$childStart.UseShellExecute = $false
$childStart.CreateNoWindow = $true
$childStart.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$childStart.RedirectStandardInput = $true
foreach ($argument in @('-NoLogo', '-NoProfile', '-Command', '[Console]::ReadLine() | Out-Null')) { $childStart.ArgumentList.Add($argument) }
$child = [Diagnostics.Process]::Start($childStart)
$retained = $null
try {
    $retained = [IsolatedTestProcess]::new($child.Id)
    if ($retained.Path -ne $childStart.FileName -or $retained.StartTime.Ticks -ne $child.StartTime.ToUniversalTime().Ticks -or
        $retained.WaitForExit(0)) { throw 'Retained process metadata or live wait is incorrect.' }
    $checks.Add('Limited process handle preserves executable, creation time and live state')
    Expect-Rejection 'A live process has no final exit code' '*still running*' { $retained.get_ExitCode() }
    $retained.Kill()
    if (!$retained.WaitForExit(5000) -or $retained.get_ExitCode() -ne -1) { throw 'Exact-handle termination or exit-code observation failed.' }
    $checks.Add('Limited process handle terminates only its owned child and retains the exit code')
}
finally {
    if ($retained) { $retained.Dispose() }
    if (!$child.HasExited) { $child.Kill(); $null = $child.WaitForExit(5000) }
    $child.Dispose()
}

# The child has exited before the first poll: a fresh Get-Process lookup cannot
# supply its exit code. The actual phase must use the retained process handle.
& {
    function Start-IsolatedTestProcess {
        param($Session, $Argument)
        $identity = New-ExitedProcessFixture $fixtureExitCode
        $Session.Processes.Add($identity)
        [IO.File]::WriteAllText((Join-Path $Session.RunRoot 'results.txt'), 'PASS: completed assertion')
        @{ Phase = $Argument; Status = 'PASS' } | ConvertTo-Json | Set-Content (Join-Path $Session.RunRoot 'report.json')
        return $identity
    }
    $fixtureExitCode = 0
    Invoke-IsolatedTestPhase $session '--fixture' | Out-Null
    if ($session.Processes.Count) { throw 'A successful phase retained its process handle.' }
    $checks.Add('Published PASS plus zero process exit succeeds and closes the handle')
    $fixtureExitCode = 17
    try {
        Expect-Rejection 'Published PASS plus nonzero process exit' '*exited with code 17*' { Invoke-IsolatedTestPhase $session '--fixture' }
        $failedPhase = $session.Phase
        foreach ($name in @('results.txt', 'report.json', 'process.json')) {
            if (!(Test-Path -LiteralPath (Join-Path $fixtureRoot "phase-$failedPhase-$name"))) {
                throw "Failed process exit lost its $name evidence."
            }
        }
        $savedProcess = Get-Content -LiteralPath (Join-Path $fixtureRoot "phase-$failedPhase-process.json") -Raw | ConvertFrom-Json
        if ($savedProcess.ExitCode -ne 17) { throw 'Failed phase exit code was not preserved.' }
        [IO.File]::WriteAllText((Join-Path $session.RunRoot 'results.txt'), 'PASS: later cleanup')
        if ((Get-Content -LiteralPath (Join-Path $fixtureRoot "phase-$failedPhase-results.txt") -Raw) -ne 'PASS: completed assertion') {
            throw 'Cleanup overwrote failed phase evidence.'
        }
        $checks.Add('Failed phase preserves report, result and exit code before cleanup replaces protocol files')
        $failed = $session.Processes[0]
        $failed.ExpectedTermination = $true
        Assert-IsolatedTestExit $failed
        $checks.Add('Only explicitly expected termination bypasses exit-code rejection')
    }
    finally { foreach ($pending in $session.Processes.ToArray()) { Close-IsolatedTestProcess $session $pending } }
    $fixtureExitCode = 1
    try { Expect-Rejection 'Unclassified exit code one remains a failure' '*exited with code 1*' { Invoke-IsolatedTestPhase $session '--fixture' } }
    finally { foreach ($pending in $session.Processes.ToArray()) { Close-IsolatedTestProcess $session $pending } }
}

& {
    function Start-IsolatedTestProcess {
        param($Session, $Argument, [switch] $ExpectedTermination)
        $scope = $Argument.Split('=')[1]
        $state = Join-Path $Session.RunRoot "LeaseTests/$scope"
        if ($ExpectedTermination) {
            New-Item -ItemType Directory -Path $state -Force | Out-Null
            [IO.File]::WriteAllText((Join-Path $state 'holder-ready'), '4294967294')
            [IO.File]::WriteAllText((Join-Path $state 'terminate-holder'), '')
            [IO.File]::WriteAllText((Join-Path $state 'result.txt'), 'PASS: lease assertions completed')
            $identity = @{ Id = 4294967294L; ExpectedTermination = $true; Process = $null }
        }
        else { $identity = New-ExitedProcessFixture 19 }
        $Session.Processes.Add($identity)
        return $identity
    }
    function Stop-IsolatedTestProcess {
        param($Session, $Identity)
        if (!$Identity.ExpectedTermination) { throw 'Unexpected forced termination in the lease fixture.' }
        $null = $Session.Processes.Remove($Identity)
    }
    try { Expect-Rejection 'Lease probe PASS plus nonzero process exit' '*exited with code 19*' { Invoke-LeaseProbe $session } }
    finally { foreach ($pending in $session.Processes.ToArray()) { Close-IsolatedTestProcess $session $pending } }
}

& {
    . (Join-Path $repo 'scripts/EditorTestSession.ps1')
    function Complete-IsolatedTestPackage { param($Session); $Session.Completed = $true }
    $interactive = @{ ProcessIdentity = New-ExitedProcessFixture 0; Processes = [Collections.Generic.List[object]]::new() }
    try {
        Expect-Rejection 'Interactive early zero exit without shutdown acknowledgement' '*before shutdown was acknowledged*' { Stop-EditorTestSession $interactive }
    }
    finally { $interactive.ProcessIdentity.Process.Dispose() }
    # Simulate the command acknowledgement boundary; the actual shutdown code
    # still waits on and validates a real failed child process.
    function Get-IsolatedTestProcess { param($Identity); return $Identity.Process }
    function Invoke-EditorTestCommand { return @{} }
    $interactive = @{ ProcessIdentity = New-ExitedProcessFixture 23; Processes = [Collections.Generic.List[object]]::new() }
    try { Expect-Rejection 'Interactive acknowledged shutdown with nonzero exit' '*exited with code 23*' { Stop-EditorTestSession $interactive } }
    finally { $interactive.ProcessIdentity.Process.Dispose() }
}

# Publish only from the process-observation boundary: the real poll has already
# seen no response/result. Actual exited child handles still supply exit codes.
& {
    function Start-IsolatedTestProcess {
        param($Session, $Argument)
        $identity = New-ExitedProcessFixture $publication.ExitCode
        $Session.Processes.Add($identity)
        return $identity
    }
    function Get-IsolatedTestProcess {
        param($Identity)
        $publication.Checks++
        $result = Join-Path $session.RunRoot 'results.txt'
        if (Test-Path -LiteralPath $result) { throw 'Phase fixture did not reach the absent-result boundary.' }
        if ($publication.Result) {
            [IO.File]::WriteAllText($result, $publication.Result)
            if ($publication.Report) {
                $publication.Report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.RunRoot 'report.json')
            }
        }
        return $null
    }
    foreach ($case in @(
        @{ Name = 'Phase result published during exit observation succeeds'; ExitCode = 0; Result = 'PASS: late completion'; Report = @{ Phase = '--publication'; Status = 'PASS' } },
        @{ Name = 'Phase exit without its result is rejected'; ExitCode = 0; Error = '*exited without a result*' },
        @{ Name = 'Late phase result cannot hide nonzero exit'; ExitCode = 29; Result = 'PASS: late completion'; Report = @{ Phase = '--publication'; Status = 'PASS' }; Error = '*exited with code 29*' },
        @{ Name = 'Late phase result requires its completion report'; ExitCode = 0; Result = 'PASS: late completion'; Error = '*omitted its structured completion report*' },
        @{ Name = 'Late phase result rejects a report for another phase'; ExitCode = 0; Result = 'PASS: late completion'; Report = @{ Phase = '--other'; Status = 'PASS' }; Error = '*Isolated phase failed*' },
        @{ Name = 'Late phase result rejects failed report status'; ExitCode = 0; Result = 'PASS: late completion'; Report = @{ Phase = '--publication'; Status = 'FAIL' }; Error = '*Isolated phase failed*' },
        @{ Name = 'Late phase result rejects failed result content'; ExitCode = 0; Result = "PASS: earlier assertion`nFAIL: later assertion"; Report = @{ Phase = '--publication'; Status = 'PASS' }; Error = '*Isolated phase failed*' }
    )) {
        $publication = $case.Clone()
        $publication.Checks = 0
        try {
            if ($case.Error) { Expect-Rejection $case.Name $case.Error { Invoke-IsolatedTestPhase $session '--publication' } }
            else {
                Invoke-IsolatedTestPhase $session '--publication' | Out-Null
                if ($session.Processes.Count) { throw 'Late phase completion retained its process handle.' }
                $checks.Add($case.Name)
            }
            if ($publication.Checks -ne 1) { throw 'Phase fixture did not exercise exactly one exit-observation boundary.' }
        }
        finally { foreach ($pending in $session.Processes.ToArray()) { Close-IsolatedTestProcess $session $pending } }
    }
}

& {
    . (Join-Path $repo 'scripts/EditorTestSession.ps1')
    function Complete-IsolatedTestPackage { param($Session); $Session.Completed = $true }
    function Get-IsolatedTestProcess {
        param($Identity)
        $publication.Checks++
        if ($publication.Checks -lt $publication.ExitCheck) { return $Identity.Process }
        $command = Join-Path $interactive.Channel 'command-0001.txt'
        $response = Join-Path $interactive.Channel 'response-0001.txt'
        if (!(Test-Path -LiteralPath $command) -or (Test-Path -LiteralPath $response)) {
            throw 'Interactive fixture did not reach the pending current-command boundary.'
        }
        if ($publication.Response) {
            $name = if ($publication.StaleResponse) { 'response-0000.txt' } else { 'response-0001.txt' }
            [IO.File]::WriteAllText((Join-Path $interactive.Channel $name), $publication.Response)
        }
        if ($publication.Fatal) { [IO.File]::WriteAllText((Join-Path $interactive.Channel 'fatal.txt'), 'Fixture fatal failure') }
        return $null
    }
    foreach ($case in @(
        @{ Name = 'Exit ACK published during exit observation succeeds'; Command = 'exit'; ExitCode = 0; Response = "OK`n" },
        @{ Name = 'Exit observation without an ACK is rejected'; Command = 'exit'; ExitCode = 0; Error = '*Isolated editor process exited*' },
        @{ Name = 'Exit observation cannot reuse a prior command ACK'; Command = 'exit'; ExitCode = 0; Response = "OK`n"; StaleResponse = $true; Error = '*Isolated editor process exited*' },
        @{ Name = 'Late exit ACK cannot hide nonzero exit'; Command = 'exit'; ExitCode = 31; Response = "OK`n"; Error = '*exited with code 31*' },
        @{ Name = 'Late exit response must acknowledge success'; Command = 'exit'; ExitCode = 0; Response = "ERROR`nFixture command failure"; Error = '*Fixture command failure*' },
        @{ Name = 'Late exit ACK cannot hide fatal evidence'; Command = 'exit'; ExitCode = 0; Response = "OK`n"; Fatal = $true; Error = '*Fixture fatal failure*' },
        @{ Name = 'Ordinary command response cannot authorize process exit'; Command = 'snapshot'; ExitCode = 0; Response = "OK`n"; Error = '*Isolated editor process exited*' }
    )) {
        $channel = Join-Path $fixtureRoot ('interactive-publication-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $channel -Force | Out-Null
        $interactive = @{
            ProcessIdentity = New-ExitedProcessFixture $case.ExitCode
            Processes = [Collections.Generic.List[object]]::new()
            Channel = $channel; Root = $channel; Sequence = 0
        }
        $interactive.Processes.Add($interactive.ProcessIdentity)
        $publication = $case.Clone()
        $publication.Checks = 0
        $publication.ExitCheck = if ($case.Command -ceq 'exit') { 3 } else { 2 }
        try {
            $action = {
                if ($case.Command -ceq 'exit') { Stop-EditorTestSession $interactive }
                else { Invoke-EditorTestCommand $interactive $case.Command }
            }
            if ($case.Error) { Expect-Rejection $case.Name $case.Error $action }
            else {
                & $action
                if (!$interactive.Completed -or $interactive.Processes.Count) { throw 'Acknowledged shutdown did not finish cleanup and release its handle.' }
                $checks.Add($case.Name)
            }
            if ($publication.Checks -ne $publication.ExitCheck) { throw 'Interactive fixture missed its exact exit-observation boundary.' }
        }
        finally { foreach ($pending in $interactive.Processes.ToArray()) { Close-IsolatedTestProcess $interactive $pending } }
    }
}

& {
    function Get-IsolatedTestProcess {
        param($Identity)
        $publication.Checks++
        if (Test-Path -LiteralPath $publication.Marker) { throw 'Lease fixture did not reach the absent-marker boundary.' }
        if ($publication.Publish) { [IO.File]::WriteAllText($publication.Marker, 'PASS: late lease completion') }
        return $null
    }
    foreach ($case in @(
        @{ Name = 'Terminal lease marker published during exit observation succeeds'; MarkerName = 'result.txt'; Publish = $true; ExitCode = 0 },
        @{ Name = 'Lease exit without its terminal marker is rejected'; MarkerName = 'result.txt'; ExitCode = 0; Error = '*exited before result.txt*' },
        @{ Name = 'Late terminal lease marker cannot hide nonzero exit'; MarkerName = 'result.txt'; Publish = $true; ExitCode = 37; Error = '*exited with code 37*' },
        @{ Name = 'Late holder readiness still requires a live process'; MarkerName = 'holder-ready'; Publish = $true; ExitCode = 0; Error = '*exited before holder-ready*' },
        @{ Name = 'Late termination request still requires a live process'; MarkerName = 'terminate-holder'; Publish = $true; ExitCode = 0; Error = '*exited before terminate-holder*' }
    )) {
        $state = Join-Path $fixtureRoot ('lease-publication-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $state -Force | Out-Null
        $lease = @{ Processes = [Collections.Generic.List[object]]::new() }
        $identity = New-ExitedProcessFixture $case.ExitCode
        $lease.Processes.Add($identity)
        $publication = $case.Clone()
        $publication.Checks = 0
        $publication.Marker = Join-Path $state $case.MarkerName
        try {
            $action = {
                Wait-LeaseMarker $lease $state $case.MarkerName ([DateTime]::UtcNow.AddSeconds(1))
                Assert-IsolatedTestExit $identity
            }
            if ($case.Error) { Expect-Rejection $case.Name $case.Error $action }
            else { & $action; $checks.Add($case.Name) }
            if ($publication.Checks -ne 1) { throw 'Lease fixture missed its exact exit-observation boundary.' }
        }
        finally { Close-IsolatedTestProcess $lease $identity }
    }
}

$manifestSource = Join-Path $repo 'tests/IntegrationTests/Package.appxmanifest'
if ($manifestSource -notin @(Get-TestSourceFiles App)) { throw 'App build discovery omits Package.appxmanifest.' }
$checks.Add('App source discovery includes its package manifest')
& {
    $inputRepo = Join-Path $fixtureRoot 'manifest-inputs'
    foreach ($relative in @('scripts', 'src', 'tests/Shared', 'tests/IntegrationTests/App')) {
        New-Item -ItemType Directory -Path (Join-Path $inputRepo $relative) -Force | Out-Null
    }
    Copy-Item -LiteralPath (Join-Path $repo 'scripts/TestBuildIdentity.ps1') -Destination (Join-Path $inputRepo 'scripts/TestBuildIdentity.ps1')
    . (Join-Path $inputRepo 'scripts/TestBuildIdentity.ps1')
    $inputManifest = Join-Path $inputRepo 'tests/IntegrationTests/Package.appxmanifest'
    [IO.File]::WriteAllText($inputManifest, '<Package Identity="before" />')
    $generatedInputs = @(
        'src/ARM64/Release/WinUIEditor/WinUIEditor.dll',
        'src/x64/Debug/WinUIEditor/WinUIEditor.pdb',
        'src/Win32/Release/WinUIEditor/WinUIEditor.lib',
        'src/Release/WinUIEditor/WinUIEditor.dll',
        'src/Notepads.Native/WinUIEditor/ARM64/Release/Editor.obj',
        'src/Notepads.Native/WinUIEditor/Generated Files/Editor.g.h',
        'src/_buildUwp/ARM64/Release/Editor.obj',
        'src/_buildWinUI3/x64/Debug/Editor.obj',
        'src/Notepads/AppPackages/Product.msix',
        'src/Notepads/BundleArtifacts/Product.msixbundle',
        'src/Notepads/bin/Release/Product.dll',
        'src/Notepads/obj/Release/Product.g.cs')
    $realInputs = @(
        'src/Notepads.Native/scintilla/win32/ScintillaWin.cxx',
        'src/Notepads.Native/scintilla/winui/ScintillaWin.cxx',
        'src/Notepads.Native/WinUIEditor/Editor.cpp',
        'src/Notepads/Assets/ARM64/Release/icon.png',
        'src/Notepads/Assets/x64/icon.png')
    foreach ($relative in $generatedInputs + $realInputs) {
        $fixturePath = Join-Path $inputRepo $relative
        New-Item -ItemType Directory -Path (Split-Path $fixturePath) -Force | Out-Null
        [IO.File]::WriteAllText($fixturePath, 'before')
    }
    $fixtureArtifact = Join-Path $inputRepo 'app.dll'
    [IO.File]::WriteAllText($fixtureArtifact, 'fixed-output')
    function Get-TestArtifactFiles { return @($fixtureArtifact) }
    $beforeManifest = Get-TestSourceHashes App
    foreach ($relative in $generatedInputs) {
        if ($beforeManifest.Contains($relative)) { throw "Generated build/publish output entered source fingerprint: $relative" }
    }
    foreach ($relative in $realInputs) {
        if (!$beforeManifest.Contains($relative)) { throw "Real platform source/asset disappeared from source fingerprint: $relative" }
    }
    $checks.Add('Source discovery excludes build/publish outputs and retains native/platform assets')
    $null = Write-TestBuildIdentity App Release ARM64 $beforeManifest
    $null = Assert-TestBuildIdentity App Release ARM64
    foreach ($relative in $generatedInputs) { [IO.File]::WriteAllText((Join-Path $inputRepo $relative), 'rebuilt-output') }
    [IO.File]::WriteAllText((Join-Path $inputRepo 'src/Notepads/AppPackages/new-package.msix'), 'new-package')
    $null = Assert-TestBuildIdentity App Release ARM64
    $checks.Add('Rebuilt and newly published outputs do not invalidate source identity')
    foreach ($relative in @($realInputs[0], $realInputs[3])) {
        [IO.File]::WriteAllText((Join-Path $inputRepo $relative), 'changed-source')
        Expect-Rejection "Changed retained input $relative" '*Source changed since build:*' { Assert-TestBuildIdentity App Release ARM64 }
        [IO.File]::WriteAllText((Join-Path $inputRepo $relative), 'before')
    }
    [IO.File]::WriteAllText($inputManifest, '<Package Identity="after" />')
    Expect-Rejection 'Manifest-only source edit invalidates NoBuild' '*Source changed since build:*Package.appxmanifest*' { Assert-TestBuildIdentity App Release ARM64 }
}

# Build manifests use actual hashing/serialization/validation against one private
# artifact. Only source/artifact discovery and manifest location are substituted.
$identityPath = Join-Path $fixtureRoot 'build-identity.json'
$artifactPath = Join-Path $fixtureRoot 'fixture.dll'
[IO.File]::WriteAllBytes($artifactPath, [byte[]](1, 2, 3, 4))
$sourceHashes = [ordered]@{ 'fixture.cs' = 'fixed-source-hash' }
function Get-TestSourceHashes { param([string] $Profile); return $sourceHashes }
function Get-TestArtifactFiles { param([string] $Profile, [string] $Configuration, [string] $Platform); return @($artifactPath) }
function Get-TestIdentityPath { param([string] $Profile, [string] $Configuration, [string] $Platform); return $identityPath }

$null = Write-TestBuildIdentity Unit Release AnyCPU $sourceHashes
$null = Assert-TestBuildIdentity Unit Release AnyCPU
$checks.Add('Matching build identity is accepted')
Expect-Rejection 'Wrong build configuration' '*configuration*does not match*' { Assert-TestBuildIdentity Unit Debug AnyCPU }
Expect-Rejection 'Wrong build profile' '*Build profile*does not match*' { Assert-TestBuildIdentity Headless Release AnyCPU }
Expect-Rejection 'Wrong build platform' '*platform*does not match*' { Assert-TestBuildIdentity Unit Release ARM64 }
$sourceHashes = [ordered]@{ 'fixture.cs' = 'changed-source-hash' }
Expect-Rejection 'Stale source content' '*Source changed since build*' { Assert-TestBuildIdentity Unit Release AnyCPU }
$sourceHashes = [ordered]@{ 'fixture.cs' = 'fixed-source-hash'; 'new.cs' = 'new-source-hash' }
Expect-Rejection 'Changed source membership' '*source file set changed*' { Assert-TestBuildIdentity Unit Release AnyCPU }
$sourceHashes = [ordered]@{ 'fixture.cs' = 'fixed-source-hash' }
$beforeIdentity = (Get-FileHash -LiteralPath $identityPath -Algorithm SHA256).Hash
Expect-Rejection 'Source changed during build' '*Source inputs changed during the build*' {
    Write-TestBuildIdentity Unit Release AnyCPU ([ordered]@{ 'fixture.cs' = 'earlier-source-hash' })
}
if ((Get-FileHash -LiteralPath $identityPath -Algorithm SHA256).Hash -ne $beforeIdentity) { throw 'A rejected build replaced the last valid manifest.' }
[IO.File]::WriteAllBytes($artifactPath, [byte[]](4, 3, 2, 1))
Expect-Rejection 'Changed output bytes' '*Built artifact changed*' { Assert-TestBuildIdentity Unit Release AnyCPU }
[IO.File]::WriteAllBytes($artifactPath, [byte[]](1, 2, 3, 4))
$null = Assert-TestBuildIdentity Unit Release AnyCPU
Assert-IsolatedTestRunOwnership $session
Assert-IsolatedTestPreservation $session
$checks.Add('Restored fixtures accept the unchanged valid build and ownership')
$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixtureRoot 'checks.json') -Encoding utf8
Write-Output "PASS: Runner contracts ($($checks.Count) checks); private evidence: $fixtureRoot"
& (Join-Path $PSScriptRoot 'CIContracts.ps1') -EvidenceRoot (Join-Path $fixtureRoot 'ci')

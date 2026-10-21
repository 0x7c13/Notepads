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
$session.Completed = $true
Expect-Rejection 'Completed session cannot launch more work' '*already completed*' { Assert-IsolatedTestRunOwnership $session }
$session.Completed = $false

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

# Exit may occur after the retained-handle check but during the separate PID
# metadata lookup. Only the verified handle may establish that the process ended;
# live mismatches and live query failures must still be rejected.
& {
    function Get-Process { param($Id, $ErrorAction); return $observation }
    foreach ($missingPath in @($false, $true)) {
        foreach ($exitsDuringQuery in @($false, $true)) {
            $retained = [pscustomobject]@{ HasExited = $false }
            $observation = [pscustomobject]@{ Retained = $retained; Exits = $exitsDuringQuery; MissingPath = $missingPath; Disposed = $false }
            $observation | Add-Member ScriptProperty Path {
                $this.Retained.HasExited = $this.Exits
                if ($this.MissingPath) { return $null }
                return 'changed executable'
            }
            $observation | Add-Member ScriptMethod Dispose { $this.Disposed = $true }
            $identity = @{ Id = 123; Path = 'owned executable'; StartTicks = 1L; Verified = $true; Process = $retained }
            $name = "PID metadata race (path unavailable=$missingPath, process exits=$exitsDuringQuery)"
            if ($exitsDuringQuery) {
                if ($null -ne (Get-IsolatedTestProcess $identity)) { throw "$name returned an exited process." }
                $checks.Add($name)
            }
            else {
                Expect-Rejection $name '*left untouched*' { Get-IsolatedTestProcess $identity }
            }
            if (!$observation.Disposed) { throw "$name leaked its metadata probe." }
        }
    }
}

function New-ExitedProcessFixture([int] $ExitCode) {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32/cmd.exe'))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardInput = $true
    # Keep the child alive until the limited handle has captured its identity.
    # Windows can no longer query its image once an immediate exit has finished.
    foreach ($argument in @('/d', '/q', '/c', "set /p fixtureReady= & exit $ExitCode")) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($start)
    $retained = $null
    try {
        $retained = [IsolatedTestProcess]::new($process.Id)
        $identity = @{ Id = $process.Id; Path = $retained.Path; StartTicks = $retained.StartTime.Ticks; Verified = $true; ExpectedTermination = $false; Process = $retained }
        $process.StandardInput.WriteLine('ready')
        $process.StandardInput.Close()
        if (!$process.WaitForExit(5000)) { $process.Kill(); throw 'Private exit-code fixture timed out.' }
        return $identity
    }
    catch { if ($retained) { $retained.Dispose() }; throw }
    finally {
        if (!$process.HasExited) { $process.Kill(); $null = $process.WaitForExit(5000) }
        $process.Dispose()
    }
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

& {
    # Load only helper functions, never the automation script's desktop setup.
    # The inert UIA method throws before any fallback input can be reached if a
    # regression moves the entry guard after SetFocus. Native types stay unloaded.
    $tokens = $null
    $parseErrors = $null
    $automation = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $repo 'scripts/Test-EditorAutomation.ps1'), [ref] $tokens, [ref] $parseErrors)
    if ($parseErrors.Count) { throw "Automation script did not parse: $parseErrors" }
    $focus = @($automation.FindAll({
        param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Focus-Editor'
    }, $false))
    if ($focus.Count -ne 1) { throw 'Expected one production Focus-Editor function.' }
    . ([scriptblock]::Create($focus[0].Extent.Text))
    function Assert-EditorForeground {
        if (!$interruption.Foreground) { throw 'Fixture foreground was interrupted.' }
        if (!$interruption.Idle) { throw 'Fixture physical input is held.' }
    }
    function Invoke-EditorTestCommand { throw 'The interrupted focus fixture reached a command.' }
    $element = [pscustomobject]@{ FocusCalls = 0 }
    $element | Add-Member ScriptMethod SetFocus {
        $this.FocusCalls++
        throw 'The interrupted focus fixture reached UIA SetFocus.'
    }
    foreach ($interruption in @(
        @{ Foreground = $false; Idle = $true; Name = 'Lost foreground rejects child focus'; Error = '*Fixture foreground was interrupted*' },
        @{ Foreground = $true; Idle = $false; Name = 'Held physical input rejects child focus'; Error = '*Fixture physical input is held*' }
    )) {
        Expect-Rejection $interruption.Name $interruption.Error { Focus-Editor }
        if ($element.FocusCalls -ne 0) { throw 'Focus was mutated before interruption was rejected.' }
    }

    # Exercise the actual keyboard helpers with inert input endpoints. No native
    # driver is loaded: a regression can record an attempted injection, but cannot
    # send Ctrl+Escape (or any other key) to the desktop running these contracts.
    $keyboardBoundary = [pscustomobject]@{
        Alive = $true; Idle = $true; Foreground = $true; PopupCount = 1
        Inputs = [Collections.Generic.List[string]]::new()
        PopupObservations = [Collections.Generic.List[int]]::new()
    }
    $keyboardBoundary | Add-Member ScriptMethod RequireIdle {
        if (!$this.Idle) { throw 'Fixture physical input is held.' }
    }
    $keyboardBoundary | Add-Member ScriptMethod Key {
        param($ProcessId, $Key, $Control, $Shift)
        if (!$this.Foreground) { throw 'Fixture foreground was interrupted.' }
        $this.Inputs.Add("key|$ProcessId|$Key|$Control|$Shift")
        if ($Key -eq 0x1b) { $this.PopupCount = 0 }
    }
    $keyboardBoundary | Add-Member ScriptMethod Type {
        param($ProcessId, $Text)
        if (!$this.Foreground) { throw 'Fixture foreground was interrupted.' }
        $this.Inputs.Add("text|$ProcessId|$Text")
    }
    function Assert-EditorTestAlive {
        param($Session)
        if (!$keyboardBoundary.Alive) { throw 'Fixture editor exited.' }
    }
    function Wait-EditorState {
        param([scriptblock] $Condition, [string] $Label)
        Assert-EditorTestAlive $session
        $state = @{ openPopups = [string] $keyboardBoundary.PopupCount }
        $keyboardBoundary.PopupObservations.Add($keyboardBoundary.PopupCount)
        if (!(& $Condition $state)) { throw "Fixture popup state does not satisfy: $Label" }
        return $state
    }
    $session = @{ ProcessId = 42 }
    $element = [pscustomobject]@{ Current = [pscustomobject]@{ HasKeyboardFocus = $true } }
    foreach ($name in @('Assert-EditorKeyboardTarget', 'Send-EditorKey', 'Send-EditorText', 'Close-EditorContextMenu')) {
        $functions = @($automation.FindAll({
            param($node)
            $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
        }, $false))
        if ($functions.Count -ne 1) { throw "Expected one production $name function." }
        $body = $functions[0].Extent.Text.Replace('[EditorAutomationInput]::', '$keyboardBoundary.').
            Replace('[PackagedAppDriver]::', '$keyboardBoundary.')
        . ([scriptblock]::Create($body))
    }
    foreach ($case in @(
        @{ Name = 'Editor key'; Action = { Send-EditorKey 0x27 $true $true }; Expected = 'key|42|39|True|True'; EditorFocus = $true },
        @{ Name = 'Editor text'; Action = { Send-EditorText 'אX' }; Expected = 'text|42|אX'; EditorFocus = $true },
        @{ Name = 'Menu Escape'; Action = { Close-EditorContextMenu }; Expected = 'key|42|27|False|False'; EditorFocus = $false }
    )) {
        $element.Current.HasKeyboardFocus = $case.EditorFocus
        $keyboardBoundary.Idle = $false
        Expect-Rejection "$($case.Name) rejects held physical input" '*Fixture physical input is held*' $case.Action
        $keyboardBoundary.Idle = $true
        $keyboardBoundary.Foreground = $false
        Expect-Rejection "$($case.Name) rejects lost foreground" '*Fixture foreground was interrupted*' $case.Action
        $keyboardBoundary.Foreground = $true
        $keyboardBoundary.Alive = $false
        Expect-Rejection "$($case.Name) rejects an exited editor" '*Fixture editor exited*' $case.Action
        $keyboardBoundary.Alive = $true
        if ($case.EditorFocus) {
            $element.Current.HasKeyboardFocus = $false
            Expect-Rejection "$($case.Name) rejects lost child focus" '*Editor focus changed*' $case.Action
            $element.Current.HasKeyboardFocus = $true
        }
        if ($keyboardBoundary.Inputs.Count) { throw "$($case.Name) injected input before rejecting interruption." }
        $keyboardBoundary.PopupObservations.Clear()
        & $case.Action
        if ($keyboardBoundary.Inputs.Count -ne 1 -or $keyboardBoundary.Inputs[0] -cne $case.Expected) {
            throw "$($case.Name) did not forward exactly the requested input to the owned target."
        }
        if ($case.Name -eq 'Menu Escape') {
            if ($keyboardBoundary.PopupObservations.Count -ne 2 -or
                $keyboardBoundary.PopupObservations[0] -ne 1 -or $keyboardBoundary.PopupObservations[1] -ne 0) {
                throw 'Menu dismissal must observe an open popup before Escape and its removal afterward.'
            }
            $checks.Add('Menu dismissal observes popup opening and visual-tree removal around guarded Escape')
        }
        $checks.Add("$($case.Name) forwards guarded input with the appropriate child focus")
        $keyboardBoundary.Inputs.Clear()
    }

    # Prevent a new test case or finally block from bypassing these boundaries.
    foreach ($call in $automation.FindAll({
        param($node)
        $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and
        $node.Expression -is [Management.Automation.Language.TypeExpressionAst] -and
        $node.Expression.TypeName.FullName -eq 'PackagedAppDriver' -and $node.Member.Value -in @('Key', 'Type')
    }, $true)) {
        $owner = $call.Parent
        while ($owner -and $owner -isnot [Management.Automation.Language.FunctionDefinitionAst]) { $owner = $owner.Parent }
        if (!$owner -or $owner.Name -notin @('Send-EditorKey', 'Send-EditorText', 'Close-EditorContextMenu')) {
            throw "Keyboard injection bypasses the guarded helpers at line $($call.Extent.StartLineNumber)."
        }
    }
    $checks.Add('All physical keyboard call sites use the guarded helpers, including menu cleanup')
}

& {
    # Execute the actual declined-prelaunch driver with inert activation/process
    # boundaries. A receipt alone must never mask a live or failed process.
    $ast = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $repo 'scripts/Test-AppLifecycle.ps1'), [ref]$null, [ref]$null)
    foreach ($name in @('Check', 'Check-DeclinedPrelaunch', 'Send-Command')) {
        $definition = $ast.Find({ param($node)
            $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
        }, $false)
        $body = $definition.Extent.Text.Replace('[PreservingUwpTestLauncher]', '[LifecycleContractLauncher]')
        . ([scriptblock]::Create($body.Replace('[IO.File]::Move(', '[LifecycleContractFiles]::Move(')))
    }
    $activationAst = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $repo 'scripts/IsolatedTestPackage.ps1'), [ref]$null, [ref]$null)
    $startActivation = $activationAst.Find({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Start-IsolatedTestActivation'
    }, $false)
    . ([scriptblock]::Create($startActivation.Extent.Text.Replace('[PreservingUwpTestLauncher]', '[LifecycleContractLauncher]')))
    if (!('LifecycleContractLauncher' -as [type])) {
        Add-Type @'
public static class LifecycleContractLauncher {
    public static string Root, Receipt;
    public static bool Exiting;
    public static System.Threading.Tasks.Task<uint> LaunchAsync(string app, string arguments, uint options) {
        System.IO.File.WriteAllText(System.IO.Path.Combine(Root, "lifecycle-42-declined-0001.json"), Receipt);
        if (Exiting) System.IO.File.WriteAllText(System.IO.Path.Combine(Root, "framework-exiting-42.txt"), "123");
        return System.Threading.Tasks.Task.FromResult(42u);
    }
}
public static class LifecycleContractFiles {
    public static int Failures, Code, Moves;
    public static void Move(string source, string target, bool overwrite) {
        Moves++;
        if (Failures-- > 0) throw new System.IO.IOException("Command publication rejected", unchecked((int)0x80070000) | Code);
        System.IO.File.Move(source, target, overwrite);
    }
}
'@
    }
    function Assert-IsolatedTestRunOwnership { param($Session) }
    function Register-IsolatedTestProcess {
        param($Session, $ProcessId, $ExpectedStartTicks)
        $identity = @{ Id = $ProcessId; StartTicks = 123L; Verified = $true; Process = $observation; ExpectedTermination = $false }
        $Session.Processes.Add($identity)
        return $identity
    }
    foreach ($case in @(
        @{ Name = 'Natural successful decline'; Exited = $true; Framework = $true; ExpectedCode = 0; Code = 0; Error = '' },
        @{ Name = 'Framework-defined exit code'; Exited = $true; Framework = $true; ExpectedCode = 1; Code = 1; Error = '' },
        @{ Name = 'Receipt with live process'; Exited = $false; Framework = $true; ExpectedCode = 0; Code = 0; Error = '*did not exit naturally*' },
        @{ Name = 'Receipt with failed process'; Exited = $true; Framework = $true; ExpectedCode = 0; Code = 1; Error = '*differs from the framework control*' },
        @{ Name = 'Receipt without framework shutdown'; Exited = $true; Framework = $false; ExpectedCode = 0; Code = 0; Error = '*did not acknowledge framework shutdown*' }
    )) {
        $root = Join-Path $fixtureRoot $case.Name
        $null = New-Item -ItemType Directory -Path (Join-Path $root 'data') -Force
        $session = @{ Root = $root; Layout = $root; RunRoot = (Join-Path $root 'data'); RunId = 'fixture';
            Package = @{ PackageFamilyName = 'fixture' }; Processes = [Collections.Generic.List[object]]::new(); FrameworkExitCode = $case.ExpectedCode }
        $observation = [pscustomobject]@{ HasExited = $case.Exited; Code = $case.Code; Disposed = $false }
        $observation | Add-Member ScriptMethod WaitForExit { param($Timeout); return $this.HasExited }
        $observation | Add-Member ScriptMethod get_ExitCode { return $this.Code }
        $observation | Add-Member ScriptMethod Dispose { $this.Disposed = $true }
        [LifecycleContractLauncher]::Root = $session.RunRoot
        [LifecycleContractLauncher]::Exiting = $case.Framework
        [LifecycleContractLauncher]::Receipt = @{ ProcessId = 42; StartTicks = 123L; HasWorkspace = $false;
            Primary = $false; Visible = $false; ClipboardFlushes = 0; SettingsBefore = 'same'; SettingsAfter = 'same' } | ConvertTo-Json
        if ($case.Error) {
            Expect-Rejection $case.Name $case.Error { Check-DeclinedPrelaunch }
            if ($session.Processes.Count -ne 1 -or $observation.Disposed) { throw 'A failed decline escaped owned process cleanup.' }
        }
        else {
            Check-DeclinedPrelaunch
            if ($session.Processes.Count -ne 0 -or !$observation.Disposed) { throw 'A successful decline retained its process handle.' }
            $checks.Add($case.Name)
        }
    }
    function Get-IsolatedTestProcess { param($Identity); return $Identity }
    function Start-Sleep { param($Milliseconds) }
    $session = @{ RunRoot = (Join-Path $fixtureRoot 'command-replacement') }
    $null = New-Item -ItemType Directory -Path $session.RunRoot
    $identity = @{ Id = 42 }
    $command = Join-Path $session.RunRoot 'lifecycle-42-command.txt'
    [IO.File]::WriteAllText($command, 'old')
    [LifecycleContractFiles]::Failures = 1
    [LifecycleContractFiles]::Code = 32
    [LifecycleContractFiles]::Moves = 0
    Send-Command $identity new-window
    if ([IO.File]::ReadAllText($command) -ne '1:new-window' -or [LifecycleContractFiles]::Moves -ne 2) {
        throw 'A transient reader lock did not publish exactly one complete command.'
    }
    $checks.Add('Atomic lifecycle command replacement retries transient reader locks')
    [LifecycleContractFiles]::Failures = 1
    [LifecycleContractFiles]::Code = 5
    [LifecycleContractFiles]::Moves = 0
    Expect-Rejection 'Non-sharing command I/O error' '*Command publication rejected*' { Send-Command $identity exit }
    if ([IO.File]::ReadAllText($command) -ne '1:new-window' -or [LifecycleContractFiles]::Moves -ne 1) {
        throw 'A non-sharing publication error was retried or changed the previous command.'
    }
}

$manifestSource = Join-Path $repo 'tests/IntegrationTests/Package.appxmanifest'
if ($manifestSource -notin @(Get-TestSourceFiles App)) { throw 'App build discovery omits Package.appxmanifest.' }
$checks.Add('App source discovery includes its package manifest')
foreach ($relative in @('tests/IntegrationTests/Package.appxmanifest', 'tests/IntegrationTests/IntegrationTests.csproj',
    'tests/IntegrationTests/Directory.Build.props', 'src/Notepads/Composition/InstanceRouter.cs',
    'src/Notepads/App.xaml.cs', 'tests/IntegrationTests/Lifecycle/LifecycleProgram.cs')) {
    if ((Join-Path $repo $relative) -notin @(Get-TestSourceFiles Lifecycle)) {
        throw "Lifecycle build discovery omits $relative."
    }
}
$checks.Add('Lifecycle source discovery includes the real activation path and package/project inputs')
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

& {
    # Exercise DPI readiness and disabled-settings guards without loading UIA,
    # launching Settings, changing a scale or waiting for a real desktop event.
    . (Join-Path $repo 'scripts/DisplayScaleSettings.ps1')
    foreach ($label in @(@('100%', 100), @('125% (Recommended)', 125), @('300％', 300))) {
        if ((Read-DisplayScalePercent $label[0]) -ne $label[1]) { throw 'Incorrect display scale parsing.' }
        $checks.Add('Display scale label: ' + $label[0])
    }
    Expect-Rejection 'Invalid display scale label' '*Unrecognized display scale*' { Read-DisplayScalePercent 'Custom' }
    function Get-DisplayScaleCombo { param($Settings); [pscustomobject]@{ Current = [pscustomobject]@{ IsEnabled = $false } } }
    Expect-Rejection 'Disabled custom scaling cannot be changed' '*disabled standard display scaling*No setting was changed*' {
        Set-DisplayScalePercent @{} 125
    }
    $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'scripts/Test-EditorDpi.ps1'), [ref]$null, [ref]$errors)
    if ($errors.Count) { throw 'The DPI runner has parse errors.' }
    $helper = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Test-DpiReady' }, $true)
    if (!$helper) { throw 'The DPI readiness helper is missing.' }
    . ([scriptblock]::Create($helper.Extent.Text))
    $before = @{ panes = '1'; windowsScale = '1'; '0_scale' = '1'; '0_pending' = 'False'; '0_dpiEvents' = '2'; '0_geometryEvents' = '3'; '0_paints' = '4' }
    $after = @{ panes = '1'; windowsScale = '1.5'; '0_scale' = '1.5'; '0_pending' = 'False'; '0_dpiEvents' = '3'; '0_geometryEvents' = '4'; '0_paints' = '5' }
    $before['0_paintScale'] = $before['0_geometryScale'] = '1'
    $after['0_paintScale'] = $after['0_geometryScale'] = '1.5'
    if (!(Test-DpiReady $after $before 1.5)) { throw 'Completed DPI transition was rejected.' }
    $checks.Add('DPI completion requires the actual system and native transition')
    foreach ($key in @('windowsScale', '0_scale', '0_paintScale', '0_geometryScale', '0_dpiEvents', '0_geometryEvents', '0_paints')) {
        $incomplete = $after.Clone()
        $incomplete[$key] = $before[$key]
        if (Test-DpiReady $incomplete $before 1.5) { throw "DPI readiness ignored stale $key." }
        $checks.Add('DPI readiness rejects stale ' + $key)
    }
    $pending = $after.Clone()
    $pending['0_pending'] = 'True'
    if (Test-DpiReady $pending $before 1.5) { throw 'DPI readiness accepted a pending viewport.' }
    $checks.Add('DPI readiness waits for viewport restoration')
    $empty = @{ panes = '0'; windowsScale = '1.5' }
    if (Test-DpiReady $empty $empty 1.5) { throw 'No editor cannot count as DPI coverage.' }
    $checks.Add('An empty DPI fixture is rejected')
}

& {
    # Run the actual DPI setup block with inert OS boundaries. A held key must
    # be rejected before even UIA SetFocus, not just before caption injection.
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'scripts/Test-EditorDpi.ps1'), [ref]$null, [ref]$null)
    $setup = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.IfStatementAst] -and $node.Extent.Text.StartsWith('if (!$foregroundAcquired)')
    }, $true)
    if (!$setup) { throw 'DPI foreground setup is missing.' }
    $body = $setup.Extent.Text.Replace('[OwnedWindowActivation]::', '$focusBoundary.').Replace('[PackagedAppDriver]::', '$focusBoundary.')
    $focusBoundary = [pscustomobject]@{ Held = $false; Calls = [Collections.Generic.List[string]]::new() }
    $focusBoundary | Add-Member ScriptMethod RequireIdle {
        $this.Calls.Add('Idle')
        if ($this.Held) { throw 'Fixture input is held.' }
    }
    $focusBoundary | Add-Member ScriptMethod Window { param($ProcessId); return [IntPtr]42 }
    $focusBoundary | Add-Member ScriptMethod Acquire {
        param($ProcessId, $Window)
        $this.Calls.Add('Acquire')
        if ($this.Held) { throw 'Fixture input is held.' }
    }
    $element = [pscustomobject]@{ Calls = $focusBoundary.Calls }
    $element | Add-Member ScriptMethod SetFocus { $this.Calls.Add('Focus') }
    function Find-DpiEditor { param($Index); return $element }
    $session = @{ ProcessId = 42 }
    $foregroundAcquired = $false
    $focusBoundary.Held = $true
    Expect-Rejection 'DPI setup rejects held input before changing focus' '*Fixture input is held*' { & ([scriptblock]::Create($body)) }
    if (($focusBoundary.Calls -join ',') -ne 'Idle') { throw 'Held input reached a focus or foreground mutation.' }
    $focusBoundary.Held = $false
    $focusBoundary.Calls.Clear()
    & ([scriptblock]::Create($body))
    if (($focusBoundary.Calls -join ',') -ne 'Idle,Focus,Acquire') { throw 'Idle DPI setup did not execute its guarded focus path.' }
    $checks.Add('Idle DPI setup acquires focus after its input guard')
}

& {
    # Keep discovery/ownership/cleanup control flow real; replace only Settings
    # launch, process/UIA boundaries and sleep. Timeout zero still attempts one
    # discovery, so failures require no wall-clock wait or desktop mutation.
    . (Join-Path $repo 'scripts/DisplayScaleSettings.ps1')
    $settingsAst = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'scripts/DisplayScaleSettings.ps1'), [ref]$null, [ref]$null)
    foreach ($name in @('Open-DisplayScaleSettings', 'Close-DisplayScaleSettings')) {
        $function = $settingsAst.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
        $body = $function.Extent.Text.Replace('[PackagedAppDriver]::', '$settingsBoundary.').
            Replace('[Windows.Automation.AutomationElement]::FromHandle', '$settingsBoundary.FromHandle').
            Replace('([Windows.Automation.WindowPattern] $root.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)).Close()', '$settingsBoundary.Close($root)')
        . ([scriptblock]::Create($body))
    }
    function Start-Process { param($FilePath); $settingsBoundary.Launched = $true }
    function Get-Process {
        param($Name, $ErrorAction)
        if ($settingsBoundary.Existing -or $settingsBoundary.Launched) { return $settingsProcess }
    }
    function Start-Sleep { param($Milliseconds) }
    function Get-DisplayScalePercent {
        param($Settings)
        if ($settingsBoundary.ChangedIdentity) { $settingsProcess.StartTime = $settingsProcess.StartTime.AddSeconds(1) }
        if ($settingsBoundary.ReadFailure) { throw 'Fixture scale discovery failed.' }
        return 125
    }
    foreach ($case in @(
        @{ Name = 'Failed discovery closes its newly opened Settings window'; ReadFailure = $true; Closes = 1; Error = '*did not become ready*Fixture scale discovery failed*' },
        @{ Name = 'Failed discovery preserves a pre-existing Settings window'; ReadFailure = $true; Existing = $true; Closes = 0; Error = '*did not become ready*Fixture scale discovery failed*' },
        @{ Name = 'Successful discovery transfers cleanup to the caller'; Closes = 1 },
        @{ Name = 'Settings cleanup failure preserves the discovery error'; ReadFailure = $true; CloseFailure = $true; Closes = 1; Error = '*Fixture scale discovery failed*Closing the owned Settings window failed*Fixture close failed*' },
        @{ Name = 'Settings cleanup rejects a replacement process'; ReadFailure = $true; ChangedIdentity = $true; Closes = 0; Error = '*Fixture scale discovery failed*refusing to close a different process*' }
    )) {
        $settingsBoundary = [pscustomobject]@{
            Existing = [bool] $case.Existing; Launched = $false; ReadFailure = [bool] $case.ReadFailure
            CloseFailure = [bool] $case.CloseFailure; ChangedIdentity = [bool] $case.ChangedIdentity; Closes = 0
        }
        $settingsBoundary | Add-Member ScriptMethod Window { param($ProcessId); return [IntPtr]42 }
        $settingsBoundary | Add-Member ScriptMethod FromHandle { param($Window); return $Window }
        $settingsBoundary | Add-Member ScriptMethod Close {
            param($Window)
            ++$this.Closes
            if ($this.CloseFailure) { throw 'Fixture close failed.' }
        }
        $settingsProcess = [pscustomobject]@{
            Id = 42; HasExited = $false; StartTime = [DateTime]::UtcNow
            Path = Join-Path $env:SystemRoot 'ImmersiveControlPanel/SystemSettings.exe'
        }
        $settingsProcess | Add-Member ScriptMethod Refresh { }
        if ($case.Error) { Expect-Rejection $case.Name $case.Error { Open-DisplayScaleSettings -TimeoutSeconds 0 } }
        else {
            $opened = Open-DisplayScaleSettings -TimeoutSeconds 0
            if (!$opened.Opened -or $opened.Original -ne 125 -or $settingsBoundary.Closes) { throw 'Discovery lost ownership or closed the successful window.' }
            Close-DisplayScaleSettings $opened
            $checks.Add($case.Name)
        }
        if ($settingsBoundary.Closes -ne $case.Closes) { throw "Incorrect Settings window cleanup: $($case.Name)" }
    }
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
& (Join-Path $PSScriptRoot 'PackagedAppProcessContracts.ps1') -EvidenceRoot (Join-Path $fixtureRoot 'packaged-processes')
& (Join-Path $PSScriptRoot 'PackagedAppInputContracts.ps1') -EvidenceRoot (Join-Path $fixtureRoot 'packaged-input')
& (Join-Path $PSScriptRoot 'ActivationContracts.ps1') -EvidenceRoot (Join-Path $fixtureRoot 'activation')
& (Join-Path $PSScriptRoot 'LifecycleSamplingContracts.ps1') -EvidenceRoot (Join-Path $fixtureRoot 'lifecycle-sampling')

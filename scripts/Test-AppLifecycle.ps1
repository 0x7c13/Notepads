# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [int] $TimeoutSeconds = 180,
    [ValidateRange(0, 20)] [int] $PerformanceRounds = 0,
    [string] $SourceLayout = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'IsolatedTestPackage.ps1')
if (!('PackagedAppDriver' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs') }
$session = $null
$driver = $null
$samples = [Collections.Generic.List[object]]::new()
$primaryFailure = $null

function Wait-Observation($Identity, [string] $Status, [int] $Activations = 1) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $path = Join-Path $session.RunRoot "lifecycle-$($Identity.Id).json"
    do {
        foreach ($failed in Get-ChildItem -LiteralPath $session.RunRoot -Filter "lifecycle-$($Identity.Id)-failed-*.json") {
            $failure = Get-Content -LiteralPath $failed.FullName -Raw | ConvertFrom-Json
            if ($failure.StartTicks -eq $Identity.StartTicks) { throw $failure.Error }
        }
        if (Test-Path -LiteralPath $path) {
            foreach ($receipt in Get-ChildItem -LiteralPath $session.RunRoot -Filter "lifecycle-$($Identity.Id)-$Status-*.json" | Sort-Object Name -Descending) {
                $state = Get-Content -LiteralPath $receipt.FullName -Raw | ConvertFrom-Json
                if ($state.StartTicks -eq $Identity.StartTicks -and $state.Activations -ge $Activations -and
                    ($Status -in @('prelaunch', 'ready', 'seeded') -or $state.CommandId -eq $Identity.LastCommandId)) { return $state }
            }
        }
        if (!(Get-IsolatedTestProcess $Identity)) { throw 'The lifecycle process exited before the expected observation.' }
        Start-Sleep -Milliseconds 25
    } while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds)
    throw "Timed out waiting for lifecycle status '$Status': $path"
}

function Send-Command($Identity, [string] $Command) {
    Assert-IsolatedTestRunOwnership $session
    if (!(Get-IsolatedTestProcess $Identity)) { throw 'Cannot command an exited lifecycle process.' }
    $path = Join-Path $session.RunRoot "lifecycle-$($Identity.Id)-command.txt"
    $session.CommandSequence = 1 + [int]$session.CommandSequence
    $Identity.LastCommandId = $session.CommandSequence
    [IO.File]::WriteAllText($path + '.tmp', "$($Identity.LastCommandId):$Command")
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        try { [IO.File]::Move($path + '.tmp', $path, $true); break }
        catch [IO.IOException] {
            # The observer may briefly hold the previous command for reading.
            # Retry only sharing/lock violations; publish by atomic replacement.
            if (($_.Exception.GetBaseException().HResult -band 0xffff) -notin @(32, 33) -or $watch.Elapsed.TotalSeconds -gt 5) { throw }
            Start-Sleep -Milliseconds 25
        }
    }
}

function Close-LifecycleProcess($Identity) {
    Send-Command $Identity exit
    $null = Wait-Observation $Identity exiting
    if (!$Identity.Process.WaitForExit(15000)) { throw 'The lifecycle process did not exit cleanly.' }
    Assert-IsolatedTestExit $Identity
    Close-IsolatedTestProcess $session $Identity
}

function Check([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
}

function Get-WorkspaceHashes {
    $hashes = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath $session.RunRoot -Recurse -File) {
        if ($file.Name -like 'lifecycle-*.json*' -or $file.Name -like 'lifecycle-*-command.txt*' -or
            $file.Name -like 'retained-process-*.txt') { continue }
        $hashes[[IO.Path]::GetRelativePath($session.RunRoot, $file.FullName)] = (Get-FileHash -LiteralPath $file.FullName).Hash
    }
    return $hashes | ConvertTo-Json -Compress
}

function Wait-Suspended {
    Assert-IsolatedTestRunOwnership $session
    $driver.Suspend()
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($driver.State -ne 3) {
        if ($watch.Elapsed.TotalSeconds -gt 15) { throw 'The owned package did not enter PES_SUSPENDED.' }
        Start-Sleep -Milliseconds 25
    }
}

function Wait-IdleCpu {
    $history = [Collections.Generic.List[double]]::new()
    $consecutive = 0
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do {
        # CIM class/property names are language-independent. Preserve the same
        # Processor Utility metric and leave a second between provider samples.
        Start-Sleep -Seconds 1
        try {
            $samples = @(Get-CimInstance -Query "SELECT Name,PercentProcessorUtility FROM Win32_PerfFormattedData_Counters_ProcessorInformation WHERE Name='_Total'" -OperationTimeoutSec 5 -ErrorAction Stop)
            if ($samples.Count -ne 1 -or $samples[0].Name -cne '_Total' -or $null -eq $samples[0].PercentProcessorUtility) {
                throw 'The provider did not return exactly one total-processor utility sample.'
            }
            $load = [double] $samples[0].PercentProcessorUtility
            if (![double]::IsFinite($load) -or $load -lt 0) { throw 'The provider returned an invalid processor utility value.' }
        }
        catch { throw "Cannot sample total processor utility through CIM: $($_.Exception.Message)" }
        $history.Add($load)
        $consecutive = if ($load -le 25) { $consecutive + 1 } else { 0 }
        if ($watch.Elapsed.TotalSeconds -gt 120) { throw 'CPU did not settle; no performance sample was launched.' }
    } while ($consecutive -lt 3)
    return ,$history.ToArray()
}

function Measure-Activation([bool] $Prepared, [string] $ExpectedHash, [bool] $Measurement = $false) {
    $warm = $null
    $current = $null
    $preparation = $null
    $load = @()
    if ($Measurement) { $load = Wait-IdleCpu }
    if ($Prepared) {
        $before = Get-WorkspaceHashes
        $warm = Start-IsolatedTestProcess $session '--lifecycle-open' -Prelaunch
        $state = Wait-Observation $warm prelaunch
        $preparation = $state
        $state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $session.Root "prelaunch-$($warm.Id).json")
        Check (!$state.HasWorkspace -and !$state.Primary -and !$state.Visible -and $state.Tabs -eq 0 -and
            $state.ClipboardFlushes -eq 0) 'Prelaunch displayed/owned a workspace or touched the clipboard.'
        Check ($state.ActiveInstance -eq $session.LastActiveInstance) 'Prelaunch changed last-active instance routing.'
        Check ($state.SettingsBefore -ceq $state.SettingsAfter) 'Prelaunch changed application preferences or instance settings.'
        Check ([PackagedAppDriver]::Window($warm.Id) -eq [IntPtr]::Zero) 'Prelaunch created a visible native window.'
        Check ((Get-WorkspaceHashes) -ceq $before) 'Prelaunch changed persisted workspace data.'
        Wait-Suspended
        Check ((Get-WorkspaceHashes) -ceq $before) 'Suspending prelaunch changed persisted workspace data.'
        if ($Measurement) { $load = Wait-IdleCpu }
    }
    $start = [Diagnostics.Stopwatch]::GetTimestamp()
    $current = Start-IsolatedTestProcess $session '--lifecycle-open'
    $state = Wait-Observation $current ready $(if ($Prepared) { 2 } else { 1 })
    Check ($state.HasWorkspace -and $state.Primary -and $state.Visible -and $state.Tabs -eq 1 -and
        $state.TextHash -ceq $ExpectedHash -and $state.FirstVisibleLine -eq 80) 'Activation did not restore the exact selected editable document/viewport.'
    Check ($state.ActiveInstance -eq $state.Instance) 'Activation did not publish the actual active instance.'
    Check ($state.ClipboardFlushes -eq 0) 'Suspending a prepared process flushed the clipboard.'
    $session.LastActiveInstance = $state.Instance
    if ($Prepared) { Check ($current.Id -eq $warm.Id) 'Real activation did not reuse the prelaunched process.' }
    $samples.Add([pscustomobject]@{
        Prepared = $Prepared; ProcessId = $current.Id; Measurement = $Measurement; CpuBefore = $load
        ReadyMilliseconds = 1000.0 * ($state.ReadyTicks - $start) / [Diagnostics.Stopwatch]::Frequency
        PaintMilliseconds = 1000.0 * ($state.FirstPaintTicks - $start) / [Diagnostics.Stopwatch]::Frequency
        PrivateBytes = $state.PrivateBytes; CpuMilliseconds = $state.CpuMilliseconds
        Width = $state.Width; Height = $state.Height; FontSize = $state.FontSize
        IsolationMilliseconds = 1000.0 * ($state.IsolationReadyTicks - $state.EntryTicks) / [Diagnostics.Stopwatch]::Frequency
        Preparation = $preparation
    })
    $samples | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $session.Root 'lifecycle-samples.json') -Encoding utf8
    Close-LifecycleProcess $current
    $current = $null
    if ($warm) { Assert-IsolatedTestExit $warm; Close-IsolatedTestProcess $session $warm; $warm = $null }
}

function Check-MultiWindowRouting {
    $first = Start-IsolatedTestProcess $session '--lifecycle-open'
    $firstState = Wait-Observation $first ready
    Check-DeclinedPrelaunch
    $repeat = Start-IsolatedTestProcess $session '--lifecycle-open'
    Check ($repeat.Id -eq $first.Id) 'A repeated user launch created another process with window reuse enabled.'
    $null = Wait-Observation $first ready 2
    Close-IsolatedTestProcess $session $repeat
    $activation = Start-IsolatedTestActivation $session -ReceiptPattern 'lifecycle-*-ready-*.json' -Protocol
    $protocol = Wait-IsolatedTestActivation $session
    $protocolState = Get-Content -LiteralPath $activation.Receipt -Raw | ConvertFrom-Json
    Check ($protocol.Id -ne $first.Id) 'Explicit new-window protocol activation was redirected to the existing window.'
    Check (!$protocolState.Primary -and $protocolState.Visible -and $protocolState.Tabs -eq 1) 'The protocol window did not create an independent secondary workspace.'
    Close-LifecycleProcess $protocol
    Write-Output 'PASS: real new-window protocol activation creates a separate editable window with reuse enabled.'
    Send-Command $first new-window
    $null = Wait-Observation $first new-window
    $second = Start-IsolatedTestProcess $session '--lifecycle-secondary'
    Check ($second.Id -ne $first.Id) 'The new-window preference was ignored without an application restart.'
    $secondState = Wait-Observation $second ready
    Check (!$secondState.Primary -and $secondState.Tabs -eq 1) 'The second window stole the primary workspace.'
    Send-Command $second reuse-window
    $null = Wait-Observation $second reuse-window
    $activationCounts = @{}
    $activationCounts[$first.Id] = 2
    $activationCounts[$second.Id] = 1
    Check ($session.ClosedInstance -notin @($firstState.Instance, $secondState.Instance)) 'The stale-ID fixture still identifies an open window.'
    foreach ($command in @('clear-active', "stale-active-$($session.ClosedInstance)")) {
        Send-Command $second $command
        $setting = Wait-Observation $second $command
        if ($command -ne 'clear-active') {
            Check ($setting.ActiveInstance -ceq $session.ClosedInstance) 'The stale-ID fixture did not publish the closed process identity.'
        }
        $fallback = Start-IsolatedTestProcess $session '--lifecycle-open'
        Check ($fallback.Id -in @($first.Id, $second.Id)) 'Missing/stale last-active metadata created a spurious third window.'
        $activationCounts[$fallback.Id]++
        $fallbackState = Wait-Observation $fallback ready $activationCounts[$fallback.Id]
        Check ($fallbackState.ActiveInstance -eq $fallbackState.Instance) 'A real activation did not repair last-active metadata.'
        Close-IsolatedTestProcess $session $fallback
    }
    Close-LifecycleProcess $second
    Send-Command $first new-window
    $null = Wait-Observation $first new-window
    Close-LifecycleProcess $first
    Check-DeclinedPrelaunch
    $reset = Start-IsolatedTestProcess $session '--lifecycle-open'
    $resetState = Wait-Observation $reset ready
    Send-Command $reset reuse-window
    $null = Wait-Observation $reset reuse-window
    Close-LifecycleProcess $reset
    $session.LastActiveInstance = $resetState.Instance
    Write-Output 'PASS: repeated launches reuse, preferences apply immediately, secondary ownership and missing/stale last-active fallback.'
}

function Check-DeclinedPrelaunch([switch] $FrameworkControl) {
    $status = if ($FrameworkControl) { 'exit-control' } else { 'declined' }
    $phase = if ($FrameworkControl) { '--lifecycle-exit-control' } else { '--lifecycle-declined' }
    # The test observer holds the callback after App has requested Exit. Launch
    # asynchronously under the session's ownership, including failure cleanup.
    $activation = Start-IsolatedTestActivation $session $phase -ReceiptPattern "lifecycle-*-$status-*.json" -Prelaunch
    $identity = Wait-IsolatedTestActivation $session
    $state = Get-Content -LiteralPath $activation.Receipt -Raw | ConvertFrom-Json
    Check (!$state.HasWorkspace -and !$state.Primary -and !$state.Visible -and $state.ClipboardFlushes -eq 0 -and
        $state.SettingsBefore -ceq $state.SettingsAfter) 'A declined prelaunch changed workspace/window, clipboard or settings.'
    if (!$identity.Process.WaitForExit(15000)) { throw 'The declined prelaunch process did not exit naturally.' }
    $exiting = Join-Path $session.RunRoot "framework-exiting-$($identity.Id).txt"
    Check ((Test-Path -LiteralPath $exiting) -and
        (Get-Content -LiteralPath $exiting -Raw) -ceq $identity.StartTicks.ToString([Globalization.CultureInfo]::InvariantCulture)) 'The declined process did not acknowledge framework shutdown.'
    $identity.ExitCode = $identity.Process.get_ExitCode()
    if ($FrameworkControl) {
        # UWP framework Exit has no documented zero-exit-code contract.
        # Measure it independently; never accept crash NTSTATUS values.
        Check ($identity.ExitCode -in @(0, 1)) 'The framework exit control failed.'
        $session.FrameworkExitCode = $identity.ExitCode
    }
    Check ($null -ne $session.FrameworkExitCode -and $identity.ExitCode -eq $session.FrameworkExitCode) 'The declined process exit code differs from the framework control.'
    $receipt = Get-Item -LiteralPath $activation.Receipt
    [pscustomobject]@{ ProcessId = $identity.Id; StartTicks = $identity.StartTicks; ExitCode = $identity.ExitCode; Marker = $receipt.Name } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root ($receipt.BaseName + '.json'))
    Close-IsolatedTestProcess $session $identity
}

function Check-ActivationPresentation {
    $first = Start-IsolatedTestProcess $session '--lifecycle-open'
    $firstState = Wait-Observation $first ready
    $requests = $firstState.PresentationRequests
    $observations = [Collections.Generic.List[object]]::new()
    Send-Command $first new-window
    $null = Wait-Observation $first new-window
    $second = Start-IsolatedTestProcess $session '--lifecycle-secondary'
    $secondState = Wait-Observation $second ready
    foreach ($case in @(
        @{ Begin = 'begin-late-activation'; End = 'complete-activation'; Negative = $true },
        @{ Begin = 'begin-activation'; End = 'complete-activation'; Negative = $false },
        @{ Begin = 'begin-activation'; End = 'dispose-activation'; Negative = $false }
    )) {
        Focus-LifecycleProcess $first
        Send-Command $first $case.Begin
        $begin = Wait-Observation $first $case.Begin
        $expectedBegin = $requests + $(if ($case.Negative) { 0 } else { 1 })
        Check ($begin.PresentationRequests -eq $expectedBegin) 'Presentation was not requested at the expected point before pending activation.'
        Check ([PackagedAppDriver]::IsForeground($first.Id)) 'The pending-activation fixture did not start in the requesting window.'
        Focus-LifecycleProcess $second
        Check ([PackagedAppDriver]::IsForeground($second.Id)) 'The peer window did not acquire the foreground.'
        Send-Command $first $case.End
        $end = Wait-Observation $first $case.End
        Check ($end.PresentationRequests -eq $requests + 1) 'The activation did not issue exactly one presentation request.'
        $peerForeground = [PackagedAppDriver]::IsForeground($second.Id)
        if (!$case.Negative) { Check $peerForeground 'Activation completion unexpectedly changed foreground.' }
        $observations.Add([pscustomobject]@{ Begin = $case.Begin; End = $case.End; Before = $requests;
            Pending = $begin.PresentationRequests; Completed = $end.PresentationRequests; PeerRemainedForeground = $peerForeground })
        $requests = $end.PresentationRequests
    }
    Close-LifecycleProcess $first
    Send-Command $second reuse-window
    $null = Wait-Observation $second reuse-window
    Close-LifecycleProcess $second
    $session.LastActiveInstance = $secondState.Instance
    $observations | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'activation-presentation.json')
    Write-Output 'PASS: presentation occurs before pending work, with no late request or foreground change on completion/shutdown; old-order control requests presentation late.'
}

function Focus-LifecycleProcess($Identity) {
    Assert-IsolatedTestRunOwnership $session
    if (!(Get-IsolatedTestProcess $Identity)) { throw 'Cannot present an exited lifecycle process.' }
    [PackagedAppDriver]::Focus($Identity.Id)
}

try {
    $session = Start-IsolatedTestPackage -Platform $Platform -Configuration $Configuration -IntegrationHost Lifecycle -SourceLayout $SourceLayout
    $driver = [PackagedAppDriver]::new("$($session.Package.PackageFamilyName)!App", $session.Package.PackageFullName)
    $seed = Start-IsolatedTestProcess $session '--lifecycle-seed'
    $seedState = Wait-Observation $seed seeded
    $expected = (Get-Content -LiteralPath (Join-Path $session.RunRoot 'lifecycle-fixture.txt') -Raw)
    Check ($seedState.TextHash -ceq $expected) 'The real-editor fixture did not publish its expected text.'
    $session.LastActiveInstance = $seedState.Instance
    $session.ClosedInstance = $seedState.Instance
    Close-LifecycleProcess $seed
    Measure-Activation $false $expected
    Measure-Activation $true $expected
    Write-Output 'PASS: real cold launch and prelaunch/suspend/user activation restore exact text and first-paint viewport.'
    Check-DeclinedPrelaunch -FrameworkControl
    Check-MultiWindowRouting
    Check-ActivationPresentation
    for ($round = 0; $round -lt $PerformanceRounds; $round++) {
        foreach ($prepared in $(if ($round % 2 -eq 0) { @($false, $true) } else { @($true, $false) })) {
            Measure-Activation $prepared $expected $true
        }
    }
    Write-Output "Lifecycle report: $($session.Root)"
}
catch {
    $primaryFailure = $_
    if ($session) { $_ | Out-String | Set-Content -LiteralPath (Join-Path $session.Root 'lifecycle-failure.txt') }
    throw
}
finally {
    if ($driver) { $driver.Dispose() }
    if ($session) {
        try { Complete-IsolatedTestPackage $session }
        catch { if ($primaryFailure) { Write-Warning "Additional lifecycle cleanup failure: $_" } else { throw } }
    }
}

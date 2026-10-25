# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [ValidateSet('Debug', 'Release')] [string] $Configuration = 'Release',
    [int] $TimeoutSeconds = 180,
    [ValidateRange(0, 20)] [int] $PerformanceRounds = 0,
    [switch] $WindowMoveOnly,
    [string] $SourceLayout = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'IsolatedTestPackage.ps1')
if (!('PackagedAppDriver' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs') }
if ($PerformanceRounds -gt 0 -and !('ProcessorUtilitySampler' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'ProcessorUtilitySampler.cs') }
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
    $sampler = $null
    try {
        try { $sampler = [ProcessorUtilitySampler]::new() }
        catch { throw "Cannot sample total processor utility through PDH: $($_.Exception.Message)" }
        do {
            Start-Sleep -Seconds 1
            try {
                $load = $sampler.Read()
                if (![double]::IsFinite($load) -or $load -lt 0) { throw 'The provider returned an invalid processor utility value.' }
            }
            catch { throw "Cannot sample total processor utility through PDH: $($_.Exception.Message)" }
            $history.Add($load)
            $consecutive = if ($load -le 25) { $consecutive + 1 } else { 0 }
            if ($watch.Elapsed.TotalSeconds -gt 120) { throw 'CPU did not settle; no performance sample was launched.' }
        } while ($consecutive -lt 3)
        return ,$history.ToArray()
    }
    finally { if ($sampler) { $sampler.Dispose() } }
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
    Arrange-LifecycleProcesses $first $second
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
    if (!('OwnedWindowActivation' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'OwnedWindowActivation.cs') }
    [OwnedWindowActivation]::RequireIdle()
    # Ask the owned UWP view on its dispatcher before using desktop activation.
    # This is fixture preparation only, never a repair after the behavior under test.
    Send-Command $Identity focus
    $null = Wait-Observation $Identity focus
    if ([PackagedAppDriver]::IsForeground($Identity.Id)) { return }
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($Identity.Id))
    $editor = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty, 'WinUIEditor.EditorBaseControl'),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$Identity.Id)))
    if (!$editor -or $editor.Current.IsOffscreen -or !$editor.Current.IsEnabled) { throw 'No visible owned editor for lifecycle setup.' }
    [OwnedWindowActivation]::RequireIdle()
    $editor.SetFocus()
    $null = [OwnedWindowActivation]::Acquire([uint32]$Identity.Id, [PackagedAppDriver]::Window($Identity.Id))
}

function Arrange-LifecycleProcesses($First, $Second) {
    Assert-IsolatedTestRunOwnership $session
    if (!(Get-IsolatedTestProcess $First) -or !(Get-IsolatedTestProcess $Second)) { throw 'Cannot arrange an exited lifecycle process.' }
    if (!('EditorDragInput' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'EditorDragInput.cs') }
    # Keep both exact owned captions visible. The existing arrangement helper
    # validates HWND ancestry/idle input and never makes either window topmost.
    $placement = [EditorDragInput]::new([uint32]$First.Id, [uint32]$Second.Id)
    try { $placement.Arrange() }
    finally { $placement.Dispose() }
}

function Start-IndirectMoveActivation($Source, [string] $Command, $Existing = $null) {
    Assert-IsolatedTestRunOwnership $session
    $marker = Join-Path $session.Layout 'PendingTestActivation.txt'
    if ($session.PendingActivation -or (Test-Path -LiteralPath $marker)) { throw 'An activation still owns this registration.' }
    $session.MoveBrokerSequence = 1 + [int]$session.MoveBrokerSequence
    $completion = [Threading.Tasks.TaskCompletionSource[uint32]]::new([Threading.Tasks.TaskCreationOptions]::RunContinuationsAsynchronously)
    $activation = @{ Task = $completion.Task; CompletionSource = $completion; Identity = $Existing; Receipt = $null
        Marker = $marker; ReceiptPattern = 'move-entry-*.json'; Indirect = $true; Source = $Source; Existing = $Existing
        BrokerPath = (Join-Path $session.RunRoot "move-broker-$($Source.Id)-$($session.MoveBrokerSequence).txt")
        PriorReceipts = @(Get-ChildItem -LiteralPath $session.RunRoot -Filter 'move-entry-*.json' | ForEach-Object FullName) }
    if ($Existing) { $activation.Receipt = Join-Path $session.RunRoot "move-entry-$($Existing.Id).json" }
    $activation.PriorReadyReceipts = if ($Existing) {
        @(Get-ChildItem -LiteralPath $session.RunRoot -Filter "lifecycle-$($Existing.Id)-ready-*.json" | ForEach-Object FullName)
    } else { @() }
    [IO.File]::WriteAllText($marker, $session.RunId)
    $session.PendingActivation = $activation
    # Persist before commanding the process: a timeout must not unregister a
    # package while its in-package broker call can still create a receiver.
    Send-Command $Source $Command
}

function Wait-IndirectMoveActivation([int] $BudgetSeconds = $TimeoutSeconds) {
    $activation = $session.PendingActivation
    if (!$activation -or !$activation.Indirect) { throw 'No indirect move activation is pending.' }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $entries = @(Get-ChildItem -LiteralPath $session.RunRoot -Filter $activation.ReceiptPattern |
            Where-Object FullName -NotIn $activation.PriorReceipts)
        foreach ($entry in $entries) {
            $receipt = Get-Content -LiteralPath $entry.FullName -Raw | ConvertFrom-Json
            if (!$activation.Identity -or $activation.Identity.Id -ne $receipt.ProcessId -or $activation.Identity.StartTicks -ne $receipt.StartTicks) {
                if ($activation.Identity) { $activation.UnexpectedReceiver = $true }
                # A duplicate can race the old receiver exiting. Retain the actual
                # replacement before rejecting it; never clear against a stale PID.
                $activation.Identity = Register-IsolatedTestProcess $session $receipt.ProcessId -ExpectedStartTicks $receipt.StartTicks
                $activation.Receipt = $entry.FullName
            }
        }
        if ($entries.Count -gt 1) { throw 'Multiple receivers acknowledged one move activation.' }
        $currentDelivery = !$activation.Existing -or $entries.Count -gt 0
        if (!$currentDelivery) {
            foreach ($ready in Get-ChildItem -LiteralPath $session.RunRoot -Filter "lifecycle-$($activation.Existing.Id)-ready-*.json" |
                Where-Object FullName -NotIn $activation.PriorReadyReceipts) {
                $observed = Get-Content -LiteralPath $ready.FullName -Raw | ConvertFrom-Json
                if ($observed.ProcessId -eq $activation.Existing.Id -and $observed.StartTicks -eq $activation.Existing.StartTicks) {
                    $currentDelivery = $true
                }
            }
        }
        if ((Test-Path -LiteralPath $activation.BrokerPath) -and $activation.Identity -and $currentDelivery) {
            $broker = [IO.File]::ReadAllLines($activation.BrokerPath)
            $receiver = Get-Content -LiteralPath $activation.Receipt -Raw | ConvertFrom-Json
            Check ($broker.Length -eq 3 -and [long]$broker[0] -eq $activation.Source.StartTicks -and
                $broker[1] -ceq $receiver.TransferId -and $broker[2] -ceq 'True') 'The completed broker call does not match its receiving process.'
            $null = $activation.CompletionSource.TrySetResult([uint32]$activation.Identity.Id)
            Complete-IsolatedTestActivation $session
            if ($activation.UnexpectedReceiver) { throw 'Duplicate activation unexpectedly replaced its retained receiver.' }
            return $activation.Identity
        }
        Start-Sleep -Milliseconds 25
    } while ($watch.Elapsed.TotalSeconds -lt $BudgetSeconds)
    throw 'The move broker/receiver handshake is unresolved; the pending fence retains its isolated registration.'
}

function Check-WindowMove {
    [IO.File]::WriteAllText((Join-Path $session.Layout 'ProtocolTestRun.txt'), $session.RunId)
    $source = Start-IsolatedTestProcess $session '--lifecycle-open'
    $before = Wait-Observation $source ready
    if ($PerformanceRounds -gt 0) {
        $null = Wait-IdleCpu
        Send-Command $source measure-move-state
        $null = Wait-Observation $source measure-move-state
        Copy-Item -LiteralPath (Join-Path $session.RunRoot 'window-move-capture.json') -Destination $session.Root
        Start-IndirectMoveActivation $source move-large-tab
        $largeTarget = Wait-IndirectMoveActivation
        $largeReady = Wait-Observation $largeTarget ready
        $largeMove = Wait-Observation $source move-large-tab
        Send-Command $largeTarget verify-large-move
        $largeVerified = Wait-Observation $largeTarget verify-large-move
        Check ($largeMove.MoveVerified -and $largeVerified.MoveVerified -and !$largeReady.Primary) 'The large move did not preserve its document and native history.'
        Close-LifecycleProcess $largeTarget
        Send-Command $source after-large-move
        $largeClosed = Wait-Observation $source after-large-move
        @($largeReady, $largeMove, $largeVerified, $largeClosed) | ConvertTo-Json -Depth 5 |
            Set-Content -LiteralPath (Join-Path $session.Root 'window-move-large.json')
    }
    Start-IndirectMoveActivation $source move-tab
    $target = Wait-IndirectMoveActivation
    Check ($source.Id -ne $target.Id) 'The move did not create a different process.'
    $received = Wait-Observation $target ready
    $moved = Wait-Observation $source move-tab
    Check ($moved.MoveVerified -and $moved.Tabs -eq $before.Tabs -and $moved.TextHash -ceq $before.TextHash) 'The background move changed the remaining source workspace.'
    Check (!$received.Primary -and $received.Tabs -eq 1) 'The receiver restored the wrong workspace.'
    Send-Command $target verify-moved-tab
    $verified = Wait-Observation $target verify-moved-tab
    Check $verified.MoveVerified 'The receiving process did not verify live document state.'
    Arrange-LifecycleProcesses $source $target
    Focus-LifecycleProcess $target
    Send-Command $target focus-moved-input
    $null = Wait-Observation $target focus-moved-input
    [OwnedWindowActivation]::RequireIdle()
    [PackagedAppDriver]::KeyThenCharacter($target.Id, 0x23, [char]'Q', $true)
    Send-Command $target verify-move-input
    $null = Wait-Observation $target verify-move-input
    Start-IndirectMoveActivation $source repeat-move $target
    $null = Wait-IndirectMoveActivation
    $null = Wait-Observation $source repeat-move
    $repeated = Wait-Observation $target ready 2
    Check ($repeated.Tabs -eq 1 -and $repeated.TextHash -ceq $received.TextHash) 'Duplicate activation reimported or changed the document.'
    Close-LifecycleProcess $target
    Start-IndirectMoveActivation $source repeat-move
    $rejected = Wait-IndirectMoveActivation
    $null = Wait-Observation $source repeat-move
    if (!$rejected.Process.WaitForExit(15000)) { throw 'A replay receiver remained registered after failing import.' }
    $exitMarker = Join-Path $session.RunRoot "framework-exiting-$($rejected.Id).txt"
    Check ((Test-Path -LiteralPath $exitMarker) -and $rejected.Process.ExitCode -eq $session.FrameworkExitCode) 'The rejected receiver did not complete framework shutdown.'
    Close-IsolatedTestProcess $session $rejected
    # Arrangement may legitimately clamp the source viewport on a taller screen.
    # Compare the move with its current source, not with its pre-resize position.
    Send-Command $source before-last-move
    $beforeLastMove = Wait-Observation $source before-last-move
    Start-IndirectMoveActivation $source move-last-tab
    $lastTarget = Wait-IndirectMoveActivation
    $lastState = Wait-Observation $lastTarget ready
    Check (!$lastState.Primary -and $lastState.Tabs -eq 1 -and $lastState.TextHash -ceq $before.TextHash -and
        $lastState.FirstVisibleLine -eq $beforeLastMove.FirstVisibleLine) 'Last-tab move changed the document or viewport.'
    if (!$source.Process.WaitForExit(15000)) { throw 'Moving the last tab did not retire the source process.' }
    Check ((Test-Path -LiteralPath (Join-Path $session.RunRoot "framework-exiting-$($source.Id).txt")) -and
        $source.Process.ExitCode -eq $session.FrameworkExitCode) 'Last-tab source did not complete framework shutdown.'
    Close-IsolatedTestProcess $session $source
    Close-LifecycleProcess $lastTarget
    @($before, $received, $moved, $verified, $repeated, $beforeLastMove, $lastState) | ConvertTo-Json -Depth 5 |
        Set-Content -LiteralPath (Join-Path $session.Root 'window-move-broker.json')
    Write-Output 'PASS: real ValueSet broker/AOT move, background and last-tab source, RTL/selection/undo/redo, duplicate activation and closed-receiver replay refusal.'
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
    if ($WindowMoveOnly) {
        Check-DeclinedPrelaunch -FrameworkControl
        Check-WindowMove
        Write-Output "Window move report: $($session.Root)"
        return
    }
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
        if ($session.PendingActivation -and $session.PendingActivation.Indirect) {
            try { $null = Wait-IndirectMoveActivation -BudgetSeconds 15 }
            catch { Write-Warning "Indirect activation remains fenced: $_" }
        }
        try { Complete-IsolatedTestPackage $session }
        catch { if ($primaryFailure) { Write-Warning "Additional lifecycle cleanup failure: $_" } else { throw } }
    }
}

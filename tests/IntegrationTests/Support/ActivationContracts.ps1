# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Run the actual asynchronous activation/cleanup code with private files and inert
# broker, process and registration boundaries. No desktop/package mutation occurs.
param([Parameter(Mandatory)] [string] $EvidenceRoot)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/IsolatedTestPackage.ps1')
$EvidenceRoot = [IO.Path]::GetFullPath($EvidenceRoot)
$null = New-Item -ItemType Directory -Path $EvidenceRoot -Force
$checks = [Collections.Generic.List[string]]::new()

if (!('ActivationLifetimeBroker' -as [type])) {
    Add-Type @'
public static class ActivationLifetimeBroker {
    public static System.Threading.Tasks.TaskCompletionSource<uint> Pending;
    public static bool RejectQueue;
    public static uint LastOptions;
    public static string LastProtocol;
    public static uint Launch(string app, string arguments, uint options) {
        LastOptions = options;
        return 42;
    }
    public static System.Threading.Tasks.Task<uint> LaunchAsync(string app, string arguments, uint options) {
        LastOptions = options;
        if (RejectQueue) throw new System.InvalidOperationException("Fixture queue failure");
        return Pending.Task;
    }
    public static System.Threading.Tasks.Task<uint> LaunchProtocolAsync(string app, string uri) {
        LastProtocol = uri;
        if (RejectQueue) throw new System.InvalidOperationException("Fixture queue failure");
        return Pending.Task;
    }
}
public sealed class ActivationPollClock {
    public static ActivationPollClock Current;
    public long Milliseconds;
    public System.TimeSpan Elapsed => System.TimeSpan.FromMilliseconds(Milliseconds);
    public static ActivationPollClock StartNew() => Current = new ActivationPollClock();
}
'@
}
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'scripts/IsolatedTestPackage.ps1'), [ref]$null, [ref]$null)
foreach ($name in @('Start-IsolatedTestActivation', 'Start-IsolatedTestProcess')) {
    $start = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
    }, $false)
    . ([scriptblock]::Create($start.Extent.Text.Replace('[PreservingUwpTestLauncher]', '[ActivationLifetimeBroker]')))
}

# Zero wait budgets exercise timeout paths deterministically, without adding a
# 15-second delay per fixture. All handshake and cleanup decisions remain real.
$waitImplementation = ${function:Wait-IsolatedTestActivation}
function Wait-IsolatedTestActivation {
    param($Session)
    & $waitImplementation $Session -TimeoutMilliseconds 0
}

function Expect-Rejection([string] $Name, [string] $Message, [scriptblock] $Action) {
    $failure = $null
    try { & $Action | Out-Null } catch { $failure = $_ }
    if (!$failure -or $failure.ToString() -notlike $Message) { throw "$Name did not reject as expected ($Message): $failure" }
    $checks.Add($Name)
}

function New-Fixture([string] $Name) {
    $root = Join-Path $EvidenceRoot $Name
    foreach ($directory in @('layout', 'data', 'prior')) { $null = New-Item -ItemType Directory -Path (Join-Path $root $directory) }
    $script:events = [Collections.Generic.List[string]]::new()
    $script:retained = [Collections.Generic.List[object]]::new()
    $script:completeOnRetention = $false
    $script:hostCleanups = $script:removals = $script:restores = 0
    $script:prior = [pscustomobject]@{ PackageFullName = 'Fixture.Prior'; InstallLocation = (Join-Path $root 'prior'); Architecture = 'Arm64' }
    $script:registration = [pscustomobject]@{ PackageFullName = 'Fixture.Owned'; InstallLocation = (Join-Path $root 'layout'); PackageFamilyName = 'Fixture' }
    $mutex = [pscustomobject]@{ Released = $false; Disposed = $false }
    $mutex | Add-Member ScriptMethod ReleaseMutex { $this.Released = $true }
    $mutex | Add-Member ScriptMethod Dispose { $this.Disposed = $true }
    $fixture = @{ Root = $root; Layout = $registration.InstallLocation; RunRoot = (Join-Path $root 'data'); RunId = 'private-fixture'
        Package = $registration; Prior = $prior; RegisteredOwned = $true; UnregisteredPrior = $true; Completed = $false
        Processes = [Collections.Generic.List[object]]::new(); PendingActivation = $null; Before = @{}; PriorDirectories = @()
        Mutex = $mutex; MutexHeld = $true; CleanupStarted = $false }
    [IO.File]::WriteAllText((Join-Path $fixture.RunRoot 'runner-sentinel.txt'), $fixture.RunId)
    [ActivationLifetimeBroker]::RejectQueue = $false
    [ActivationLifetimeBroker]::Pending = [Threading.Tasks.TaskCompletionSource[uint32]]::new([Threading.Tasks.TaskCreationOptions]::RunContinuationsAsynchronously)
    return $fixture
}

function Assert-IsolatedTestRunOwnership { param($Session) }
function Assert-IsolatedTestPreservation { param($Session) }
function Get-AppxPackage { param($Name); return $registration }
function Remove-AppxPackage { param($Package, [switch] $PreserveApplicationData); $script:removals++; $script:registration = $null; $events.Add('remove') }
function Add-AppxPackage { param($Register); $script:restores++; $script:registration = $prior; $events.Add('restore') }
function Get-IsolatedTestProcess { param($Identity); if (!$Identity.Process.HasExited) { return $Identity.Process } }
function Register-IsolatedTestProcess {
    param($Session, $ProcessId, $ExpectedStartTicks)
    $process = [pscustomobject]@{ HasExited = $true; Disposed = $false; Events = $events }
    $process | Add-Member ScriptMethod Dispose { $this.Disposed = $true; $this.Events.Add('dispose') }
    $identity = @{ Id = $ProcessId; StartTicks = 123L; Verified = $true; ExpectedTermination = $false; Process = $process }
    $Session.Processes.Add($identity)
    $retained.Add($process)
    $events.Add("retain-$ProcessId")
    if ($completeOnRetention) { [ActivationLifetimeBroker]::Pending.SetResult($ProcessId) }
    return $identity
}
function Invoke-IsolatedTestPhase {
    param($Session, $Argument, $TimeoutSeconds)
    if ($Session.PendingActivation -or $Session.Processes.Count) { throw 'Host cleanup raced outstanding activation/process ownership.' }
    $script:hostCleanups++
    $events.Add('host-cleanup')
    foreach ($name in @('grants-before.json', 'grants-after.json')) {
        [IO.File]::WriteAllText((Join-Path $Session.RunRoot $name), '{"prior-token":"prior-metadata"}')
    }
}
function Publish-Receipt($Session, [string] $Content = '') {
    if (!$Content) { $Content = @{ ProcessId = 42; StartTicks = 123L } | ConvertTo-Json }
    [IO.File]::WriteAllText((Join-Path $Session.RunRoot 'lifecycle-42-declined-0001.json'), $Content)
}
function Begin-Activation($Session) {
    Start-IsolatedTestActivation $Session '--declined' -ReceiptPattern 'lifecycle-*-declined-*.json' -Prelaunch
}
function Assert-Restored($Session) {
    if ($Session.PendingActivation -or $Session.Processes.Count -or (Test-Path -LiteralPath (Join-Path $Session.Layout 'PendingTestActivation.txt')) -or
        $registration -ne $prior -or $hostCleanups -ne 1 -or $removals -ne 1 -or $restores -ne 1 -or
        !$Session.Completed -or !$Session.Mutex.Released -or !$Session.Mutex.Disposed -or @($retained | Where-Object { !$_.Disposed }).Count) {
        throw 'Settled activation did not release exact process handles and restore the prior registration.'
    }
    if ($events.IndexOf('restore') -lt $events.IndexOf('host-cleanup')) { throw 'Registration was restored before cleanup.' }
}

$session = New-Fixture 'queue-failure'
[ActivationLifetimeBroker]::RejectQueue = $true
Expect-Rejection 'Synchronous queue failure releases the fence' '*Fixture queue failure*' { Begin-Activation $session }
if ($session.PendingActivation -or (Test-Path -LiteralPath (Join-Path $session.Layout 'PendingTestActivation.txt'))) { throw 'Failed queue retained ownership.' }

foreach ($prelaunch in @($false, $true)) {
    foreach ($asynchronous in @($false, $true)) {
        $session = New-Fixture "activation-option-$prelaunch-$asynchronous"
        if ($asynchronous) {
            $null = Start-IsolatedTestActivation $session '--open' -ReceiptPattern 'lifecycle-*-declined-*.json' -Prelaunch:$prelaunch
            Publish-Receipt $session
            [ActivationLifetimeBroker]::Pending.SetResult(42u)
            $null = Wait-IsolatedTestActivation $session
        }
        else { $null = Start-IsolatedTestProcess $session '--open' -Prelaunch:$prelaunch }
        # Independent SDK oracle: AO_PRELAUNCH must reach the COM boundary intact.
        $expected = if ($prelaunch) { 0x02000000 } else { 0 }
        if ([ActivationLifetimeBroker]::LastOptions -ne $expected) { throw 'The prelaunch switch changed the native activation flags.' }
        Complete-IsolatedTestPackage $session | Out-Null
        Assert-Restored $session
        $checks.Add("Activation flags (prelaunch=$prelaunch, asynchronous=$asynchronous)")
    }
}

$session = New-Fixture 'protocol-activation'
$activation = Start-IsolatedTestActivation $session -ReceiptPattern 'lifecycle-*-declined-*.json' -Protocol
if ([ActivationLifetimeBroker]::LastProtocol -cne 'notepads://newinstance' -or
    (Get-Content -LiteralPath (Join-Path $session.Layout 'ProtocolTestRun.txt') -Raw) -cne $session.RunId -or
    !(Test-Path -LiteralPath $activation.Marker)) { throw 'Protocol activation lost its exact operation, explicit root or broker fence.' }
Publish-Receipt $session
[ActivationLifetimeBroker]::Pending.SetResult(42u)
$null = Wait-IsolatedTestActivation $session
Complete-IsolatedTestPackage $session | Out-Null
Assert-Restored $session
$checks.Add('Protocol activation retains the explicit run marker and fenced broker handshake')

$session = New-Fixture 'protocol-prelaunch'
Expect-Rejection 'Protocol activation refuses prelaunch before creating a fence' '*cannot be prelaunch*' {
    Start-IsolatedTestActivation $session -Protocol -Prelaunch
}
if ($session.PendingActivation -or (Test-Path -LiteralPath (Join-Path $session.Layout 'PendingTestActivation.txt'))) {
    throw 'An invalid protocol/prelaunch combination created pending work.'
}

$session = New-Fixture 'unfinished'
$activation = Begin-Activation $session
Expect-Rejection 'Timeout retains the outstanding call' '*did not finish its broker/receipt handshake*' { Wait-IsolatedTestActivation $session }
if ($session.PendingActivation -ne $activation -or $activation.Task.IsCompleted -or $session.Processes.Count -or
    !(Test-Path -LiteralPath $activation.Marker)) { throw 'Timeout lost the pending call or persistent registration fence.' }
Expect-Rejection 'Overlapping asynchronous activation is refused' '*unfinished activation*' { Begin-Activation $session }
Expect-Rejection 'Ordinary launch cannot pass an unfinished activation' '*unfinished activation*' { Start-IsolatedTestProcess $session '--open' }
Expect-Rejection 'Unfinished cleanup retains registration and blocks later runs' '*unfinished activation remains*' { Complete-IsolatedTestPackage $session }
if ($hostCleanups -or $removals -or $restores -or $registration -ne $session.Package -or !(Test-Path -LiteralPath $activation.Marker)) {
    throw 'Unfinished activation allowed cleanup or package replacement.'
}
# Exercise actual next-run admission against the fenced fixture registration.
# Even a regression cannot mutate Windows: all package/process boundaries here
# are inert, and the private fixture is not a deployable app layout.
& {
    function Get-Process { param($Name, $ErrorAction) }
    Expect-Rejection 'Next run refuses an outstanding broker registration' '*unfinished activation owns the existing test registration*' {
        Start-IsolatedTestPackage
    }
}
if ($removals -or $restores -or $registration -ne $session.Package) { throw 'Admission touched the fenced registration.' }
[ActivationLifetimeBroker]::Pending.SetResult(42u)

$session = New-Fixture 'late-callback'
$activation = Begin-Activation $session
Expect-Rejection 'Late callback starts from a genuine timeout' '*did not finish its broker/receipt handshake*' { Wait-IsolatedTestActivation $session }
Publish-Receipt $session
$completeOnRetention = $true
Complete-IsolatedTestPackage $session | Out-Null
Assert-Restored $session
if (!$activation.Task.IsCompletedSuccessfully -or $events.IndexOf('retain-42') -gt $events.IndexOf('host-cleanup')) {
    throw 'Cleanup did not retain the callback process before completing its broker handshake.'
}
$checks.Add('Cleanup finishes a late callback handshake before process/data/registration cleanup')

foreach ($receipt in @($false, $true)) {
    $session = New-Fixture "broker-failure-$receipt"
    $null = Begin-Activation $session
    if ($receipt) { Publish-Receipt $session }
    [ActivationLifetimeBroker]::Pending.SetException([InvalidOperationException]::new('Fixture broker failure'))
    Expect-Rejection "Broker failure is observed and propagated (receipt=$receipt)" '*Fixture broker failure*' { Complete-IsolatedTestPackage $session }
    Assert-Restored $session
}

$session = New-Fixture 'completed-without-receipt'
$activation = Begin-Activation $session
[ActivationLifetimeBroker]::Pending.SetResult(42u)
Expect-Rejection 'Completed broker without a receipt still fails the test' '*did not finish its broker/receipt handshake*' { Wait-IsolatedTestActivation $session }
if ($session.PendingActivation -or $session.Processes.Count -ne 1 -or $session.Processes[0].Id -ne 42) { throw 'Missing receipt lost the returned process.' }
Complete-IsolatedTestPackage $session | Out-Null
Assert-Restored $session

$session = New-Fixture 'malformed-receipt'
$null = Begin-Activation $session
Publish-Receipt $session '{bad json'
[ActivationLifetimeBroker]::Pending.SetResult(42u)
Expect-Rejection 'Malformed receipt cannot lose a completed broker process' '*JSON*' { Wait-IsolatedTestActivation $session }
if ($session.PendingActivation -or $session.Processes.Count -ne 1 -or $session.Processes[0].Id -ne 42) { throw 'Receipt parse failure lost the returned process.' }
Complete-IsolatedTestPackage $session | Out-Null
Assert-Restored $session

$session = New-Fixture 'different-processes'
$null = Begin-Activation $session
Publish-Receipt $session
[ActivationLifetimeBroker]::Pending.SetResult(43u)
Expect-Rejection 'Disagreeing broker and callback retain both processes' '*activation result and callback process differ*' { Wait-IsolatedTestActivation $session }
if ($session.Processes.Count -ne 2 -or ($session.Processes.Id -join ',') -ne '42,43') { throw 'A disagreeing process escaped cleanup ownership.' }
Complete-IsolatedTestPackage $session | Out-Null
Assert-Restored $session

& {
    # Substitute only time at the private test boundary. Exercise the actual
    # positive/default budgets and backoff without real sleeps or timing bands.
    $wait = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Wait-IsolatedTestActivation'
    }, $false)
    $body = $wait.Extent.Text.Replace('function Wait-IsolatedTestActivation', 'function Wait-PolledActivation')
    $clocked = $body.Replace('[Diagnostics.Stopwatch]::StartNew()', '[ActivationPollClock]::StartNew()')
    if ($clocked -ceq $body) { throw 'The monotonic clock boundary was not found.' }
    . ([scriptblock]::Create($clocked))
    function Start-Sleep {
        param([int] $Milliseconds)
        if ($Milliseconds -le 0 -or ++$polling.Sleeps -gt 1000) { throw 'The polling loop did not back off within its budget.' }
        [ActivationPollClock]::Current.Milliseconds += $Milliseconds
        if ($polling.AfterSleep) { & $polling.AfterSleep $polling.Sleeps }
    }
    function Assert-Polls([int] $Sleeps, [int] $Milliseconds) {
        if ($polling.Sleeps -ne $Sleeps -or [ActivationPollClock]::Current.Milliseconds -ne $Milliseconds) {
            throw 'The polling loop returned before completion or did not stop at its deadline.'
        }
    }

    $session = New-Fixture 'positive-budget'
    $activation = Begin-Activation $session
    $polling = @{ Sleeps = 0 }
    Expect-Rejection 'Nonzero budget expires after repeated backoff' '*did not finish its broker/receipt handshake*' {
        Wait-PolledActivation $session -TimeoutMilliseconds 75
    }
    Assert-Polls 3 75
    if ($session.PendingActivation -ne $activation) { throw 'Positive-budget timeout lost the outstanding call.' }
    Publish-Receipt $session
    [ActivationLifetimeBroker]::Pending.SetResult(42u)
    Complete-IsolatedTestPackage $session | Out-Null
    Assert-Restored $session

    $session = New-Fixture 'polled-completion'
    $null = Begin-Activation $session
    $polling = @{ Sleeps = 0; AfterSleep = {
        param($Count)
        if ($Count -eq 2) { Publish-Receipt $session; [ActivationLifetimeBroker]::Pending.SetResult(42u) }
    } }
    $identity = Wait-PolledActivation $session -TimeoutMilliseconds 75
    Assert-Polls 2 50
    if ($identity.Id -ne 42 -or $retained.Count -ne 1) { throw 'Polled completion lost or duplicated the process.' }
    Complete-IsolatedTestPackage $session | Out-Null
    Assert-Restored $session
    $checks.Add('A broker completed on the second backoff is received before the deadline')

    $session = New-Fixture 'polled-receipt'
    $null = Begin-Activation $session
    [ActivationLifetimeBroker]::Pending.SetResult(42u)
    $polling = @{ Sleeps = 0; AfterSleep = { param($Count); if ($Count -eq 2) { Publish-Receipt $session } } }
    $identity = Wait-PolledActivation $session -TimeoutMilliseconds 75
    Assert-Polls 2 50
    if ($identity.Id -ne 42 -or $retained.Count -ne 1) { throw 'A late receipt lost or duplicated the completed broker process.' }
    Complete-IsolatedTestPackage $session | Out-Null
    Assert-Restored $session
    $checks.Add('Completed broker keeps polling until its late receipt arrives')

    $session = New-Fixture 'polled-missing-receipt'
    $null = Begin-Activation $session
    [ActivationLifetimeBroker]::Pending.SetResult(42u)
    $polling = @{ Sleeps = 0 }
    Expect-Rejection 'Completed broker without a receipt consumes the positive budget' '*did not finish its broker/receipt handshake*' {
        Wait-PolledActivation $session -TimeoutMilliseconds 75
    }
    Assert-Polls 3 75
    if ($session.PendingActivation -or $session.Processes.Count -ne 1) { throw 'Receipt timeout lost the completed broker process.' }
    Complete-IsolatedTestPackage $session | Out-Null
    Assert-Restored $session

    $session = New-Fixture 'default-budget'
    $null = Begin-Activation $session
    $polling = @{ Sleeps = 0 }
    Expect-Rejection 'Default budget remains fifteen seconds of monotonic time' '*did not finish its broker/receipt handshake*' {
        Wait-PolledActivation $session
    }
    Assert-Polls 600 15000
    Publish-Receipt $session
    [ActivationLifetimeBroker]::Pending.SetResult(42u)
    Complete-IsolatedTestPackage $session | Out-Null
    Assert-Restored $session
}

$checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'checks.json')
Write-Output "PASS: asynchronous activation lifetime contracts ($($checks.Count) checks)."

# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Publish the isolated UWP host and desktop peer for the same architecture first.
# -Case accepts exact case names or wildcards for diagnosis, e.g. incoming-immediate-*.
# Omitting -Case runs the full matrix. Run from a 64-bit interactive PowerShell.
param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [Alias('CaseFilter')] [string[]] $Case = @('*')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
$null = Assert-TestBuildIdentity App Release $Platform
$null = Assert-TestBuildIdentity DragPeer Release $Platform
if (!('EditorDragInput' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'EditorDragInput.cs') }

$script:session = $null
$script:peer = $null
$script:peerProcess = $null
$script:editorProcess = $null
$script:inputDriver = $null
$script:activeCase = $null
$script:checks = 0
$script:reports = [Collections.Generic.List[object]]::new()
$script:caseIds = [Collections.Generic.List[string]]::new()
$script:utf8 = [Text.UTF8Encoding]::new($false)
# Deliberately include BMP, supplementary characters, and all three line endings.
$script:payload = 'A' + [char]0x4F60 + [char]0x597D + [char]::ConvertFromUtf32(0x1F600) + "`r`nB`nC`rD"
$script:targetFixture = "target`rend"
$script:receiverFixture = 'receiver|'
$script:sourceSuffix = " tail`rend"

function Encode-Text([string] $Value) { [Convert]::ToBase64String($script:utf8.GetBytes($Value)) }
function Decode-Text([string] $Value) { $script:utf8.GetString([Convert]::FromBase64String($Value)) }
function Normalize-Text([string] $Value) { $Value.Replace("`r`n", "`r").Replace("`n", "`r") }
function Text-Length([string] $Value) { $script:utf8.GetByteCount($Value) }
function State-Bool($Value) { ($Value -eq '1' -or $Value -eq 'True') }

function Assert-DragEqual($Actual, $Expected, [string] $Label) {
    if ($Actual -cne $Expected) { throw "$Label`: expected [$Expected], actual [$Actual]" }
    $script:checks++
}
function Assert-DragTrue([bool] $Condition, [string] $Label) {
    if (!$Condition) { throw $Label }
    $script:checks++
}
function Write-DragTrace([string] $Event, $Data) {
    if (!$script:session) { return }
    $entry = [ordered]@{ ticks = [DateTime]::UtcNow.Ticks; case = $script:activeCase; event = $Event; data = $Data }
    [IO.File]::AppendAllText((Join-Path $script:session.Root 'drag-driver.jsonl'), ($entry | ConvertTo-Json -Depth 8 -Compress) + "`n", $script:utf8)
}
function Assert-PeerAlive {
    if (!$script:peer) { throw 'Desktop peer has not started.' }
    if (!$script:peer.IdentityReady) { throw 'Desktop peer identity has not finished initializing.' }
    $process = Get-Process -Id $script:peer.ProcessId -ErrorAction SilentlyContinue
    if (!$process) { throw 'Owned desktop peer exited.' }
    if ([string]::IsNullOrWhiteSpace($process.Path)) { throw 'The initialized desktop peer no longer exposes its executable path.' }
    if (![string]::Equals([IO.Path]::GetFullPath($process.Path), $script:peer.Path, [StringComparison]::OrdinalIgnoreCase) -or
        $process.StartTime.ToUniversalTime().Ticks -ne $script:peer.StartTicks) {
        throw 'Desktop peer process identity changed.'
    }
    $fatal = Join-Path $script:peer.Channel 'fatal.txt'
    if (Test-Path -LiteralPath $fatal) { throw (Read-EditorTestFile $fatal) }
}
function Wait-PeerIdentity {
    # Process.Start supplies the owned process handle immediately, but Windows
    # can briefly report no main-module path while the apphost is initializing.
    # Only this startup phase permits an unavailable path; no input occurs here.
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    $lastReadError = ''
    while (!$script:peer.IdentityReady) {
        Assert-EditorTestAlive $script:session
        $script:peerProcess.Refresh()
        if ($script:peerProcess.HasExited) { throw "Desktop peer exited during initialization (exit $($script:peerProcess.ExitCode))." }
        $actualPath = $null
        try { $actualPath = $script:peerProcess.MainModule.FileName }
        catch { $lastReadError = $_.ToString() }
        if (![string]::IsNullOrWhiteSpace($actualPath)) {
            if (![string]::Equals([IO.Path]::GetFullPath($actualPath), $script:peer.Path, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'The launched desktop peer executable does not match the requested apphost.'
            }
            $script:peer.StartTicks = $script:peerProcess.StartTime.ToUniversalTime().Ticks
            $script:peer.IdentityReady = $true
            Write-DragTrace 'peer-identity-ready' $script:peer
            return
        }
        if ([DateTime]::UtcNow -ge $deadline) { throw "Desktop peer executable path did not become available within five seconds. $lastReadError" }
        Start-Sleep -Milliseconds 25
    }
}
function Assert-DragAlive {
    Assert-EditorTestAlive $script:session
    Assert-PeerAlive
    if ($script:inputDriver) { $script:inputDriver.RequireForeground() }
}
function Read-KeyValues([string] $Path) {
    $result = @{}
    foreach ($line in (Read-EditorTestFile $Path -AsLines)) {
        $parts = $line.Split(@('='), 2)
        if ($parts.Length -eq 2) { $result[$parts[0]] = $parts[1] }
    }
    return $result
}
function Write-Atomic([string] $Path, [string] $Value) {
    if (Test-Path -LiteralPath $Path) { throw "Refusing to reuse a protocol file: $Path" }
    [IO.File]::WriteAllText($Path + '.tmp', $Value, $script:utf8)
    [IO.File]::Move($Path + '.tmp', $Path)
}
function Invoke-PeerCommand([string] $Command, [int] $TimeoutSeconds = 5, [DateTime] $DeadlineUtc = [DateTime]::MaxValue) {
    Assert-PeerAlive
    $script:peer.Sequence++
    $id = '{0:D4}' -f $script:peer.Sequence
    Write-Atomic (Join-Path $script:peer.Channel "command-$id.txt") $Command
    $response = Join-Path $script:peer.Channel "response-$id.txt"
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    if ($DeadlineUtc -lt $deadline) { $deadline = $DeadlineUtc }
    while (!(Test-Path -LiteralPath $response)) {
        Assert-DragAlive
        if ([DateTime]::UtcNow -ge $deadline) { throw "Desktop peer command timed out: $Command" }
        Start-Sleep -Milliseconds 20
    }
    $lines = Read-EditorTestFile $response -AsLines
    if ($lines[0] -ne 'OK') { throw ($lines -join "`n") }
    return Read-KeyValues $response
}
function Editor-Command([string] $Command, [int] $TimeoutSeconds = 5) {
    Assert-DragAlive
    $state = Invoke-EditorTestCommand $script:session $Command -TimeoutSeconds $TimeoutSeconds
    Assert-DragAlive
    return $state
}
function Event-Path([string] $Side, [string] $Event) {
    $channel = if ($Side -eq 'editor') { $script:session.Channel } else { $script:peer.Channel }
    return Join-Path $channel "$($script:activeCase).$Event"
}
function Wait-DragEvent([string] $Side, [string] $Event, [int] $TimeoutSeconds = 5) {
    $path = Event-Path $Side $Event
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (!(Test-Path -LiteralPath $path)) {
        Assert-DragAlive
        if ([DateTime]::UtcNow -ge $deadline) {
            Write-DragTrace 'event-timeout' @{ side = $Side; event = $Event; peerFiles = @(Get-ChildItem -LiteralPath $script:peer.Channel -File | Select-Object -ExpandProperty Name) }
            throw "Real $Side event '$Event' was not observed. Delayed/preflight/cache ordering is unproven; this case does not count as coverage."
        }
        Start-Sleep -Milliseconds 20
    }
    Write-DragTrace 'event-observed' @{ side = $Side; event = $Event; contents = (Read-EditorTestFile $path) }
    return Read-KeyValues $path
}
function Release-Gate([string] $Event, [string] $Value = 'ok') {
    $path = Event-Path 'peer' $Event
    if (!(Test-Path -LiteralPath $path)) { Write-Atomic $path $Value }
    Write-DragTrace 'gate-released' @{ gate = $Event; value = $Value }
}
function Assert-DataStillBlocked($Request) {
    Assert-DragTrue ($Request.ContainsKey('requestId')) 'Blocked request has an independent request ID'
    $path = Event-Path 'peer' 'events.tsv'
    $entries = if (Test-Path -LiteralPath $path) { Read-EditorTestFile $path -AsLines -CompleteLinesOnly } else { @() }
    $entered = $false
    $finished = $false
    foreach ($line in $entries) {
        $parts = $line.Split("`t")
        if ($parts.Length -ge 5 -and $parts[4] -eq $Request.requestId) {
            if ($parts[3] -eq 'data-request-enter') { $entered = $true }
            if ($parts[3] -eq 'data-request-result') { $finished = $true }
        }
    }
    Assert-DragTrue $entered 'Timeline confirms the requested payload entered GetData'
    Assert-DragTrue (!$finished) 'Payload GetData is still blocked before mutation/release'
    Assert-DragTrue (!(Test-Path -LiteralPath (Event-Path 'peer' 'source-completed'))) 'Source has not completed while the payload is blocked'
}
function Assert-DataReleased($Request, [bool] $Failure) {
    $entries = Read-EditorTestFile (Event-Path 'peer' 'events.tsv') -AsLines -CompleteLinesOnly
    $released = $false
    $result = $null
    foreach ($line in $entries) {
        $parts = $line.Split("`t")
        if ($parts.Length -ge 6 -and $parts[4] -eq $Request.requestId) {
            Assert-DragTrue ($parts[3] -ne 'release-data-timeout') 'Provider gate was explicitly released before its timeout'
            if ($parts[3] -eq 'release-data') { $released = $true }
            if ($parts[3] -eq 'data-request-result') {
                Assert-DragTrue $released 'The blocked request completed only after its explicit release'
                $result = Decode-Text $parts[5]
            }
        }
    }
    Assert-DragEqual $result $(if ($Failure) { 'failure' } else { 'success' }) 'The originally blocked request produced the intended result'
}
function Reset-Editor([string] $Text) {
    $state = Editor-Command "reset|$(Encode-Text $Text)|False|260|200|18" 15
    Assert-DragEqual (Decode-Text $state.text64) (Normalize-Text $Text) 'Editor reset contains the complete normalized fixture'
    Assert-DragEqual (State-Bool $state.canUndo) $false 'Reset starts with an empty undo history'
    return $state
}
function New-DragCase([string] $Name, [string] $Direction, [string] $SourceMode, [string] $ReceiverMode) {
    $script:activeCase = [Guid]::NewGuid().ToString('N')
    $script:caseIds.Add($script:activeCase)
    $editorText = if ($Direction -eq 'incoming') { $script:targetFixture } else { (Normalize-Text $script:payload) + $script:sourceSuffix }
    $editor = Reset-Editor $editorText
    $null = Editor-Command "begin-case|$script:activeCase"
    $peer = Invoke-PeerCommand "reset|$script:activeCase|$(Encode-Text $script:payload)|$(Encode-Text $script:receiverFixture)|$SourceMode|$ReceiverMode|3"
    Assert-DragEqual (Decode-Text $peer.source64) $script:payload 'Peer reset retains the raw Unicode payload and mixed endings'
    Assert-DragEqual (Decode-Text $peer.target64) $script:receiverFixture 'Peer receiver starts with its sentinel'
    Assert-DragEqual ([int] $peer.allowed) 3 'Peer advertises both Copy and Move independently of the effective operation'
    Assert-DragEqual ([int] $peer.pid) ([int] $script:peer.ProcessId) 'Protocol peer PID matches the independently launched process'
    Assert-DragEqual ([long] $peer.hwnd) $script:inputDriver.PeerWindow.ToInt64() 'Peer reported HWND matches exact discovered window'
    if ($Direction -eq 'outgoing') { $editor = Editor-Command "select|0|$(Text-Length (Normalize-Text $script:payload))" }
    Write-DragTrace 'case-start' @{ name = $Name; direction = $Direction; sourceMode = $SourceMode; receiverMode = $ReceiverMode; allowedEffect = 3; editor = $editor; peer = $peer }
    return @{ Editor = $editor; Peer = $peer; EditorText = $editorText; Name = $Name; Direction = $Direction }
}
function Get-DragPoints($Fixture) {
    $incoming = $Fixture.Direction -eq 'incoming'
    $position = if ($incoming) { Text-Length $Fixture.EditorText } else { 0 }
    $geometry = Editor-Command "geometry|$position"
    $peerState = Invoke-PeerCommand 'state'
    $editorPoint = @{ X = [int][Math]::Round([double]$geometry.pointX) + 3; Y = [int][Math]::Round([double]$geometry.pointY) + 5 }
    $panel = if ($incoming) { 'source' } else { 'target' }
    $peerPoint = @{ X = [int]$peerState["${panel}X"] + [int]([int]$peerState["${panel}Width"] / 2); Y = [int]$peerState["${panel}Y"] + [int]([int]$peerState["${panel}Height"] / 2) }
    $script:inputDriver.RequirePoint($editorPoint.X, $editorPoint.Y, [uint32]$script:session.ProcessId)
    $script:inputDriver.RequirePoint($peerPoint.X, $peerPoint.Y, [uint32]$script:peer.ProcessId)
    $result = if ($incoming) {
        @{ Source = $peerPoint; Target = $editorPoint; SourcePid = $script:peer.ProcessId; TargetPid = $script:session.ProcessId }
    } else {
        @{ Source = $editorPoint; Target = $peerPoint; SourcePid = $script:session.ProcessId; TargetPid = $script:peer.ProcessId }
    }
    Write-DragTrace 'physical-points' $result
    return $result
}
function Wait-PeerTargetReady($Points, [bool] $Copy) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    $timeline = Event-Path 'peer' 'events.tsv'
    $expectedEffect = if ($Copy) { 1 } else { 2 }
    while ([DateTime]::UtcNow -lt $deadline) {
        Assert-DragAlive
        $script:inputDriver.RequireCursorAt([uint32]$Points.TargetPid, $Points.Target.X, $Points.Target.Y)
        $script:inputDriver.RequireHeldMouse()
        if (Test-Path -LiteralPath (Event-Path 'editor' 'editor-drag-completed')) {
            throw 'Editor source completed before the desktop receiver acknowledged this held drag.'
        }
        $entered = $false
        foreach ($line in (Read-EditorTestFile $timeline -AsLines -CompleteLinesOnly)) {
            $parts = $line.Split("`t")
            if ($parts.Length -lt 6 -or $parts[3] -ne 'target-drag-enter') { continue }
            Assert-DragEqual ([int]$parts[1]) ([int]$script:peer.ProcessId) 'Target-enter evidence belongs to the owned peer process'
            $entered = $true
        }
        if ($entered) {
            # The same-case enter event proves real OLE delivery; the live state
            # also excludes an old enter followed by leave/reset/drop.
            $target = Invoke-PeerCommand 'state' -DeadlineUtc $deadline
            Assert-DragEqual $target.case $script:activeCase 'Receiver readiness belongs to the current drag case'
            Assert-DragEqual (State-Bool $target.targetDropped) $false 'Mouse remains held before the receiver Drop callback'
            $script:inputDriver.RequireCursorAt([uint32]$Points.TargetPid, $Points.Target.X, $Points.Target.Y)
            $script:inputDriver.RequireHeldMouse()
            if (Test-Path -LiteralPath (Event-Path 'editor' 'editor-drag-completed')) {
                throw 'Editor source completed while checking desktop receiver readiness.'
            }
            if ((State-Bool $target.targetHovering) -and [int]$target.targetProvisional -eq $expectedEffect -and
                (([int]$target.targetAllowed -band $expectedEffect) -ne 0)) {
                if ([DateTime]::UtcNow -ge $deadline) { break }
                Write-DragTrace 'peer-target-ready-before-release' $target
                return
            }
        }
        Start-Sleep -Milliseconds 20
    }
    throw 'Desktop receiver did not acknowledge and provisionally accept the held drag within five seconds; no mouse-up was issued.'
}
function Start-RealDrag($Fixture, [bool] $Copy, [bool] $Cancel) {
    $points = Get-DragPoints $Fixture
    Assert-DragAlive
    $script:inputDriver.Begin([uint32]$points.SourcePid, $points.Source.X, $points.Source.Y, $Copy)
    # First cross the real drag threshold inside the owned source window.
    $script:inputDriver.MoveTo($points.Source.X + 24, $points.Source.Y + 8, 8)
    if ($Fixture.Direction -eq 'incoming') { $null = Wait-DragEvent 'peer' 'source-started' }
    else { $null = Wait-DragEvent 'editor' 'editor-drag-started' }
    $script:inputDriver.MoveTo($points.Target.X, $points.Target.Y, 30)
    if ($Fixture.Direction -eq 'outgoing') { Wait-PeerTargetReady $points $Copy }
    if ($Cancel) {
        $script:inputDriver.Escape()
        # Wait for the real drag loop to observe Escape before releasing the
        # mouse; an immediate button-up can otherwise win the input race.
        try {
            if ($Fixture.Direction -eq 'incoming') { $null = Wait-DragEvent 'peer' 'source-completed' }
            else { $null = Wait-DragEvent 'editor' 'editor-drag-completed' }
        } finally { $script:inputDriver.ReleaseEscape() }
    }
    $script:inputDriver.End([uint32]$points.TargetPid, $points.Target.X, $points.Target.Y)
    Write-DragTrace 'pointer-released' @{ copyModifier = $Copy; escapeAfterStart = $Cancel }
}
function Complete-RealDrag($Fixture, [bool] $Cancel = $false) {
    if ($Fixture.Direction -eq 'incoming') { $null = Wait-DragEvent 'peer' 'source-completed' 10 }
    else {
        $null = Wait-DragEvent 'editor' 'editor-drag-completed' 10
        if (!$Cancel) { $null = Wait-DragEvent 'peer' 'target-completed' }
    }
    $script:inputDriver.ReleaseControl()
    $state = @{ Editor = (Editor-Command 'state'); Peer = (Invoke-PeerCommand 'state') }
    Write-DragTrace 'case-completed' $state
    return $state
}
function Assert-EditorUndo([string] $Before, $After, [bool] $Changed) {
    Assert-DragEqual (State-Bool $After.canUndo) $Changed 'Exactly accepted editor transfers create an undo entry'
    if ($Changed) {
        $undo = Editor-Command 'undo'
        Assert-DragEqual (Decode-Text $undo.text64) $Before 'One editor undo restores the entire insertion/deletion'
        Assert-DragEqual (State-Bool $undo.canUndo) $false 'One transfer creates exactly one undo group'
    }
}
function Assert-Immediate($Fixture, $State, [int] $Effect) {
    if ($Fixture.Direction -eq 'incoming') {
        $expectedEditor = $Fixture.EditorText
        if ($Effect -ne 0) { $expectedEditor += Normalize-Text $script:payload }
        $expectedSource = if ($Effect -eq 2) { '' } else { $script:payload }
        Assert-DragEqual (Decode-Text $State.Editor.text64) $expectedEditor 'Incoming transfer changes exactly the expected editor bytes'
        Assert-DragEqual (Decode-Text $State.Peer.source64) $expectedSource 'Desktop source clears only an actual Move'
        Assert-DragEqual (Decode-Text $State.Peer.target64) $script:receiverFixture 'Incoming transfer preserves peer receiver sentinel'
        Assert-DragEqual ([int]$State.Peer.sourceEffect) $Effect 'Desktop source records the effective operation'
        Assert-DragTrue (State-Bool $State.Peer.sourceStarted) 'Desktop source really started a drag'
        Assert-DragTrue (State-Bool $State.Peer.sourceCompleted) 'Desktop source completed its drag'
        if ($Effect -eq 0) {
            Assert-DragTrue (State-Bool $State.Peer.sourceEscapeSeen) 'Desktop source observed Escape in the real drag callback'
            Assert-DragEqual ([int]$State.Editor.dropStarted) 0 'Escape cancellation never reaches editor Drop'
        }
        else { Assert-DragTrue ([int]$State.Editor.dropStarted -gt 0) 'The native editor received a real drop' }
    } else {
        $expectedEditor = if ($Effect -eq 2) { $script:sourceSuffix } else { $Fixture.EditorText }
        $expectedTarget = $script:receiverFixture
        if ($Effect -ne 0) { $expectedTarget += Normalize-Text $script:payload }
        Assert-DragEqual (Decode-Text $State.Editor.text64) $expectedEditor 'Outgoing source deletion matches the actual operation'
        Assert-DragEqual (Decode-Text $State.Peer.target64) $expectedTarget 'Desktop receiver contains the exact transferred Unicode payload'
        Assert-DragEqual (Decode-Text $State.Peer.source64) $script:payload 'Outgoing transfer preserves peer source fixture'
        Assert-DragEqual ([int]$State.Peer.targetEffect) $Effect 'Desktop receiver reports its actual acceptance'
        Assert-DragEqual ([int]$State.Editor.dragResult) $Effect 'Editor reports the actual completed operation'
        Assert-DragTrue ([int]$State.Editor.dragStarted -gt 0) 'Editor really started a drag'
        Assert-DragTrue ([int]$State.Editor.dragCompleted -gt 0) 'Editor completed its drag'
        if ($Effect -ne 0) {
            Assert-DragEqual (Decode-Text $State.Peer.received64) (Normalize-Text $script:payload) 'Receiver captured the selected source bytes'
            Assert-DragEqual (([int]$State.Peer.targetKeyState -band 8) -ne 0) ($Effect -eq 1) 'Receiver records Ctrl for Copy and no Ctrl for Move'
        } else {
            Assert-DragEqual (State-Bool $State.Peer.targetDropped) $false 'Escape cancellation never reaches receiver Drop'
            Assert-DragEqual (Decode-Text $State.Peer.received64) '' 'Cancelled receiver never retrieves payload'
        }
    }
    Assert-EditorUndo $Fixture.EditorText $State.Editor ($Effect -ne 0 -and ($Fixture.Direction -eq 'incoming' -or $Effect -eq 2))
}
function Mutate-PendingEditor([string] $Mutation, $Fixture) {
    $state = $null
    switch ($Mutation) {
        'unchanged' { $state = Editor-Command 'state' }
        'failure' { $state = Editor-Command 'state' }
        'replace-same' {
            $state = Editor-Command "replace|$(Encode-Text $Fixture.EditorText)"
            Assert-DragTrue (State-Bool $state.documentReplaced) 'Replacement publishes a distinct native document while the drag is gated'
            if ($Fixture.Direction -eq 'outgoing') {
                # Loading a new native document collapses selection. Restore it
                # so this case isolates document identity/revision invalidation.
                $state = Editor-Command "select|0|$(Text-Length (Normalize-Text $script:payload))"
                Assert-DragEqual $state.anchor $Fixture.Editor.anchor 'Replacement source restores the original selection anchor'
                Assert-DragEqual $state.caret $Fixture.Editor.caret 'Replacement source restores the original selection caret'
            }
        }
        'document' { $state = Editor-Command "set|$(Encode-Text ('changed|' + $Fixture.EditorText))" }
        'readonly' { $state = Editor-Command 'flags|True|True|True' }
        'native-disabled' { $state = Editor-Command 'flags|False|True|False' }
        'parent-disabled' { $state = Editor-Command 'flags|False|False|True' }
        'detach' { $state = Editor-Command 'detach' }
        'detach-attach' { $null = Editor-Command 'detach'; $state = Editor-Command 'attach' }
        'dispose' { $state = Editor-Command 'dispose' }
        'selection' { $state = Editor-Command "select|$(Text-Length $Fixture.EditorText)|$(Text-Length $Fixture.EditorText)" }
        'selection-restored' {
            $null = Editor-Command "select|$(Text-Length $Fixture.EditorText)|$(Text-Length $Fixture.EditorText)"
            $state = Editor-Command "select|0|$(Text-Length (Normalize-Text $script:payload))"
        }
        default { throw "Unknown mutation: $Mutation" }
    }
    Assert-DragEqual (State-Bool $state.canUndo) $false 'Pending mutation has no pre-existing undo entry'
    Write-DragTrace 'mutation-acknowledged' @{ mutation = $Mutation; state = $state }
    return $state
}
function Test-Recovery([string] $Name) {
    # Exercise the same live control again after re-enabling/reattaching it.
    # Reset recreates a disposed control because that lifecycle has ended.
    $baseline = Reset-Editor $script:targetFixture
    $null = Editor-Command "select|$(Text-Length $script:targetFixture)|$(Text-Length $script:targetFixture)"
    $edited = Editor-Command "insert|$(Encode-Text '|edit')"
    Assert-DragEqual (Decode-Text $edited.text64) ($script:targetFixture + '|edit') 'Ordinary edit works after the rejected transfer'
    $undo = Editor-Command 'undo'
    Assert-DragEqual (Decode-Text $undo.text64) $script:targetFixture 'Ordinary edit remains undoable after rejection'
    $fixture = New-DragCase "$Name-recovery" 'incoming' 'normal' 'normal'
    Start-RealDrag $fixture $false $false
    $state = Complete-RealDrag $fixture
    Assert-Immediate $fixture $state 2
    Write-DragTrace 'recovery-passed' @{ precedingCase = $Name }
}
function Test-Immediate([string] $Name, [string] $Direction, [string] $Operation) {
    $fixture = New-DragCase $Name $Direction 'normal' 'normal'
    $cancel = $Operation -eq 'cancel'
    Start-RealDrag $fixture ($Operation -eq 'copy') $cancel
    $state = Complete-RealDrag $fixture $cancel
    $effect = if ($cancel) { 0 } elseif ($Operation -eq 'copy') { 1 } else { 2 }
    Assert-Immediate $fixture $state $effect
    if ($cancel) { Test-Recovery $Name }
}
function Prepare-ReplacementFixture($Fixture) {
    $prepared = Editor-Command "prepare-replace|$(Encode-Text $Fixture.EditorText)" 15
    Assert-DragEqual $prepared.replacementCase $script:activeCase 'Replacement fixture belongs to this exact drag case'
    Assert-DragEqual (Decode-Text $prepared.replacementText64) $Fixture.EditorText 'Replacement fixture binds the exact candidate text'
    Assert-DragEqual (Decode-Text $prepared.text64) $Fixture.EditorText 'Preparing a candidate does not replace the active document'
    Assert-DragEqual $prepared.anchor $Fixture.Editor.anchor 'Preparing a candidate preserves the source selection anchor'
    Assert-DragEqual $prepared.caret $Fixture.Editor.caret 'Preparing a candidate preserves the source selection caret'
    Assert-DragEqual (State-Bool $prepared.canUndo) $false 'Preparing a candidate adds no editor undo entry'
    Write-DragTrace 'replacement-fixture-prepared' $prepared
}
function Test-IncomingDelayed([string] $Name, [string] $Mutation) {
    $mode = if ($Mutation -eq 'failure') { 'fail' } else { 'delayed' }
    $fixture = New-DragCase $Name 'incoming' $mode 'normal'
    if ($Mutation -eq 'replace-same') { Prepare-ReplacementFixture $fixture }
    Start-RealDrag $fixture $false $false
    $null = Wait-DragEvent 'editor' 'editor-drop-entered'
    $request = Wait-DragEvent 'peer' 'data-requested'
    Assert-DataStillBlocked $request
    $waiting = Editor-Command 'state'
    Assert-DragEqual (Decode-Text $waiting.text64) $fixture.EditorText 'Editor responds unchanged while real Drop is awaiting data'
    Assert-DragTrue ([int]$waiting.dropStarted -gt 0) 'Native routed Drop was observed before mutation'
    Assert-DataStillBlocked $request
    Write-DragTrace 'incoming-gate-proven' @{ request = $request; editor = $waiting }
    $mutated = Mutate-PendingEditor $Mutation $fixture
    Assert-DataStillBlocked $request
    Release-Gate 'release-data' $(if ($Mutation -eq 'failure') { 'fail' } else { 'ok' })
    $state = Complete-RealDrag $fixture
    Assert-DataReleased $request ($Mutation -eq 'failure')
    if ($Mutation -eq 'unchanged') { Assert-Immediate $fixture $state 2 }
    else {
        Assert-DragEqual (Decode-Text $state.Editor.text64) (Decode-Text $mutated.text64) 'Rejected incoming transfer preserves the acknowledged post-mutation document'
        Assert-DragEqual ([int]$state.Peer.sourceEffect) 0 'Rejected incoming transfer reports no operation'
        Assert-DragEqual (Decode-Text $state.Peer.source64) $script:payload 'Rejected incoming Move preserves source payload'
        Assert-DragEqual (Decode-Text $state.Peer.target64) $script:receiverFixture 'Rejected incoming transfer preserves peer target'
        Assert-EditorUndo (Decode-Text $mutated.text64) $state.Editor $false
        Test-Recovery $Name
    }
}
function Test-OutgoingDelayed([string] $Name, [string] $Mutation) {
    $fixture = New-DragCase $Name 'outgoing' 'normal' 'delayed'
    if ($Mutation -eq 'replace-same') { Prepare-ReplacementFixture $fixture }
    Start-RealDrag $fixture $false $false
    $captured = Wait-DragEvent 'peer' 'target-drop-entered'
    Assert-DragEqual (Decode-Text $captured.received64) (Normalize-Text $script:payload) 'Delayed receiver already captured the original payload'
    Assert-DragTrue (!(Test-Path -LiteralPath (Event-Path 'editor' 'editor-drag-completed'))) 'Editor source is still awaiting receiver completion'
    $waiting = Editor-Command 'state'
    Assert-DragEqual (Decode-Text $waiting.text64) $fixture.EditorText 'Outgoing source responds unchanged while receiver is gated'
    Assert-DragTrue ([int]$waiting.dragStarted -gt 0) 'Outgoing mutation follows a real editor drag start'
    $mutated = Mutate-PendingEditor $Mutation $fixture
    Assert-DragTrue (!(Test-Path -LiteralPath (Event-Path 'peer' 'target-completed'))) 'Receiver has not completed before mutation acknowledgement'
    Release-Gate 'release-drop'
    $state = Complete-RealDrag $fixture
    Assert-DragEqual ([int]$state.Peer.targetEffect) 2 'Delayed receiver actually accepts Move'
    Assert-DragEqual ([int]$state.Editor.dragResult) 2 'Editor observes receiver Move even when cleanup is unsafe'
    Assert-DragEqual (Decode-Text $state.Peer.received64) (Normalize-Text $script:payload) 'Receiver retains the original captured payload'
    Assert-DragEqual (Decode-Text $state.Peer.target64) ($script:receiverFixture + (Normalize-Text $script:payload)) 'Receiver inserts the captured payload exactly once'
    Assert-DragEqual (Decode-Text $state.Peer.source64) $script:payload 'Outgoing transfer leaves peer source unchanged'
    $mayDelete = $Mutation -in @('unchanged', 'selection-restored')
    $expected = if ($mayDelete) { $script:sourceSuffix } else { Decode-Text $mutated.text64 }
    Assert-DragEqual (Decode-Text $state.Editor.text64) $expected 'Source cleanup deletes only an unchanged, editable selected range'
    Assert-EditorUndo (Decode-Text $mutated.text64) $state.Editor $mayDelete
    if (!$mayDelete) { Test-Recovery $Name }
}
function Stop-OwnedPeer {
    if (!$script:peer -or !$script:peerProcess) { return }
    if ($script:peerProcess.HasExited) { return }
    if (!$script:peer.IdentityReady) {
        # A startup failure may precede path availability. Kill only the exact
        # Process.Start handle; never reopen or terminate an unverified PID.
        Write-DragTrace 'stop-initializing-peer' @{ pid = $script:peer.ProcessId; expectedPath = $script:peer.Path }
        $script:peerProcess.Kill()
        if (!$script:peerProcess.WaitForExit(5000)) { throw 'Initializing owned desktop peer did not exit.' }
        return
    }
    $process = Get-Process -Id $script:peer.ProcessId -ErrorAction SilentlyContinue
    if (!$process) { return }
    if ([string]::IsNullOrWhiteSpace($process.Path) -or
        ![string]::Equals([IO.Path]::GetFullPath($process.Path), $script:peer.Path, [StringComparison]::OrdinalIgnoreCase) -or
        $process.StartTime.ToUniversalTime().Ticks -ne $script:peer.StartTicks) { throw 'Refusing to stop a peer process whose exact identity changed.' }
    $script:peerProcess.Kill()
    if (!$script:peerProcess.WaitForExit(5000)) { throw 'Owned desktop peer did not exit.' }
}
function Stop-PressedDragSource {
    $sourceId = $script:inputDriver.SourceProcessId
    $source = if ($sourceId -eq $script:session.ProcessId) { $script:editorProcess }
        elseif ($script:peer -and $sourceId -eq $script:peer.ProcessId) { $script:peerProcess }
        else { throw 'Pressed input does not belong to a known drag source.' }
    if (!$source -or $source.Id -ne $sourceId) { throw 'The original drag source process handle is unavailable.' }
    # These handles were opened and identity-checked before any input. Never
    # reopen a PID here: an exited source's PID may already belong to someone else.
    if (!$source.HasExited) {
        $source.Kill()
        if (!$source.WaitForExit(5000)) { throw 'The exact owned drag source did not exit; emergency mouse release was refused.' }
    }
    Write-DragTrace 'drag-source-exited-before-mouse-release' @{ pid = $sourceId }
    return $source
}

$matrix = @(
    @{ Name = 'outgoing-readonly-copy'; Kind = 'readonly-copy' }
    foreach ($direction in @('incoming', 'outgoing')) {
        foreach ($operation in @('move', 'copy', 'cancel')) {
            @{ Name = "$direction-immediate-$operation"; Kind = 'immediate'; Direction = $direction; Value = $operation }
        }
    }
    foreach ($mutation in @('unchanged', 'failure', 'replace-same', 'document', 'readonly', 'native-disabled', 'parent-disabled', 'detach', 'detach-attach', 'dispose')) {
        @{ Name = "incoming-delayed-$mutation"; Kind = 'incoming'; Value = $mutation }
    }
    foreach ($mutation in @('unchanged', 'selection', 'selection-restored', 'document', 'replace-same', 'readonly', 'native-disabled', 'parent-disabled', 'detach', 'detach-attach')) {
        @{ Name = "outgoing-delayed-$mutation"; Kind = 'outgoing'; Value = $mutation }
    }
)
if (!$Case -or @($Case | Where-Object { ![string]::IsNullOrWhiteSpace($_) }).Count -eq 0) { $Case = @('*') }
$selected = @($matrix | Where-Object { $name = $_.Name; @($Case | Where-Object { $name -like $_ }).Count -gt 0 })
if (!$selected.Count) { throw "No matrix case matches '$($Case -join ', ')'. Available: $($matrix.Name -join ', ')" }
$failure = $null
try {
    $repo = Split-Path $PSScriptRoot -Parent
    $runtime = 'win-' + $Platform.ToLowerInvariant()
    $peerExe = [IO.Path]::GetFullPath((Join-Path $repo "tests\IntegrationTests\Support\DragDrop.Peer\bin\Release\net10.0-windows\$runtime\DragDrop.Peer.exe"))
    if (!(Test-Path -LiteralPath $peerExe)) { throw "Publish DragDrop.Peer for $runtime first. Missing $peerExe" }
    $script:session = Start-EditorTestSession -Platform $Platform
    Write-Output "Evidence: $($script:session.Root)"
    $script:editorProcess = [IsolatedTestProcess]::new($script:session.ProcessId)
    if ($script:editorProcess.HasExited -or
        $script:editorProcess.StartTime.ToUniversalTime().Ticks -ne $script:session.ProcessStartTicks -or
        !$script:editorProcess.Path.StartsWith($script:session.Layout + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'The retained editor handle does not match the owned session process.'
    }
    $nativeBuild = Join-Path $repo "src\_buildUwp\$Platform\Release\WinUIEditor\bin\WinUIEditor\WinUIEditor.dll"
    $nativeLayout = @(Get-ChildItem -LiteralPath $script:session.Layout -Filter WinUIEditor.dll -File -Recurse)
    Assert-DragEqual $nativeLayout.Count 1 'The owned layout contains exactly one native editor'
    $buildHash = (Get-FileHash -LiteralPath $nativeBuild -Algorithm SHA256).Hash
    Assert-DragEqual (Get-FileHash -LiteralPath $nativeLayout[0].FullName -Algorithm SHA256).Hash $buildHash 'Deployed native editor matches the current build'
    @($nativeBuild, $nativeLayout[0].FullName, $peerExe, [IO.Path]::ChangeExtension($peerExe, '.dll')) |
        ForEach-Object { Get-FileHash -LiteralPath $_ -Algorithm SHA256 } |
        Select-Object Path, Hash | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:session.Root 'binary-hashes.json') -Encoding UTF8
    $channel = Join-Path $script:session.Root 'peer'
    if (Test-Path -LiteralPath $channel) { throw 'Peer channel unexpectedly exists; refusing to reuse it.' }
    New-Item -ItemType Directory -Path $channel | Out-Null
    # CreateNoWindow hides a console without hiding the peer's intentional Form.
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $peerExe
    $start.Arguments = '"' + $channel + '"'
    $start.WorkingDirectory = Split-Path $peerExe -Parent
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    if ($env:RUNNER_ENVIRONMENT -eq 'github-hosted') {
        if (!('CiDesktopPeer' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'CiDesktopPeer.cs') }
        $script:peerProcess = [CiDesktopPeer]::Start($start.FileName, $start.Arguments, $start.WorkingDirectory)
    }
    else { $script:peerProcess = [Diagnostics.Process]::Start($start) }
    $script:peer = @{ ProcessId = $script:peerProcess.Id; Path = $peerExe; StartTicks = 0; Channel = $channel; Sequence = 0; IdentityReady = $false }
    Write-DragTrace 'peer-launched' $script:peer
    Wait-PeerIdentity
    Assert-PeerAlive
    Assert-DragTrue ($script:peer.ProcessId -ne $script:session.ProcessId) 'Drag crosses distinct independently owned processes'
    $script:peer | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:session.Root 'peer-session.json') -Encoding UTF8
    $ready = Join-Path $channel 'ready.txt'
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (!(Test-Path -LiteralPath $ready)) {
        Assert-DragAlive
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Desktop peer did not become ready.' }
        Start-Sleep -Milliseconds 25
    }
    $script:inputDriver = [EditorDragInput]::new([uint32]$script:session.ProcessId, [uint32]$script:peer.ProcessId)
    $script:inputDriver.Arrange()
    $script:inputDriver.Activate([uint32]$script:session.ProcessId)
    Write-DragTrace 'owned-windows' @{ editorPid = $script:session.ProcessId; peerPid = $script:peer.ProcessId; editorHwnd = $script:inputDriver.EditorWindow.ToInt64(); peerHwnd = $script:inputDriver.PeerWindow.ToInt64(); editorAnchor = $script:inputDriver.EditorAnchor.ToInt64(); peerAnchor = $script:inputDriver.PeerAnchor.ToInt64(); physicalWorkArea = $script:inputDriver.WorkArea; selected = $selected.Name; totalMatrix = $matrix.Count }
    foreach ($scenario in $selected) {
        $started = [DateTime]::UtcNow
        $beforeChecks = $script:checks
        Write-Output "RUN: $($scenario.Name)"
        try {
            switch ($scenario.Kind) {
                'readonly-copy' {
                    $fixture = New-DragCase $scenario.Name 'outgoing' 'normal' 'normal'
                    $null = Editor-Command 'flags|True|True|True'
                    Start-RealDrag $fixture $true $false
                    $state = Complete-RealDrag $fixture
                    Assert-Immediate $fixture $state 1
                }
                'immediate' { Test-Immediate $scenario.Name $scenario.Direction $scenario.Value }
                'incoming' { Test-IncomingDelayed $scenario.Name $scenario.Value }
                'outgoing' { Test-OutgoingDelayed $scenario.Name $scenario.Value }
            }
            $script:reports.Add([ordered]@{ name = $scenario.Name; status = 'PASS'; assertions = $script:checks - $beforeChecks; elapsedSeconds = ([DateTime]::UtcNow - $started).TotalSeconds })
            Write-Output "PASS: $($scenario.Name)"
        }
        catch {
            $script:reports.Add([ordered]@{ name = $scenario.Name; status = 'FAIL'; caseId = $script:activeCase; error = $_.ToString(); assertions = $script:checks - $beforeChecks })
            throw
        }
    }
    Write-Output "PASS: $($selected.Count)/$($matrix.Count) cross-process matrix cases selected, $script:checks assertions. Evidence: $($script:session.Root)"
}
catch { $failure = $_; Write-DragTrace 'failure' $_.ToString() }
finally {
    $cleanupErrors = [Collections.Generic.List[string]]::new()
    # Escape may not be observed when a window vanishes or an STA is blocked.
    # In that case, source exit is the completion oracle before emergency UP.
    if ($script:inputDriver) {
        try {
            if ($script:inputDriver.HasMousePress) {
                try { $script:inputDriver.Activate($script:inputDriver.SourceProcessId); $script:inputDriver.Escape() }
                catch { Write-DragTrace 'cleanup-escape-unavailable' $_.ToString() }
                $exitedSource = Stop-PressedDragSource
                $script:inputDriver.ReleaseOwnedInput($exitedSource)
            } else { $script:inputDriver.ReleaseOwnedInput() }
        } catch { $cleanupErrors.Add($_.ToString()) }
        finally {
            try { $script:inputDriver.ReleaseOwnedKeys() } catch { $cleanupErrors.Add($_.ToString()) }
        }
    }
    if ($script:peer) {
        foreach ($caseId in $script:caseIds) {
            foreach ($gate in @('release-data', 'release-drop')) {
                try {
                    $path = Join-Path $script:peer.Channel "$caseId.$gate"
                    if (!(Test-Path -LiteralPath $path)) { Write-Atomic $path 'fail' }
                } catch { $cleanupErrors.Add($_.ToString()) }
            }
        }
    }
    try { Stop-OwnedPeer } catch { $cleanupErrors.Add($_.ToString()) }
    if ($script:peerProcess) { $script:peerProcess.Dispose() }
    if ($script:inputDriver) { try { $script:inputDriver.Dispose() } catch { $cleanupErrors.Add($_.ToString()) } }
    if ($script:session) {
        $report = [ordered]@{ platform = $Platform; fullMatrix = $matrix.Count; selected = $selected.Count; assertions = $script:checks; cases = $script:reports.ToArray(); failure = $(if ($failure) { $failure.ToString() } else { $null }); cleanupErrors = $cleanupErrors.ToArray() }
        try { $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $script:session.Root 'drag-results.json') -Encoding UTF8 }
        finally { try { Stop-EditorTestSession $script:session } catch { $cleanupErrors.Add($_.ToString()) } }
        # Session cleanup retains the package channel under commands/.
        [IO.File]::WriteAllText((Join-Path $script:session.Root 'drag-cleanup-errors.txt'), ($cleanupErrors -join "`r`n"), $script:utf8)
    }
    if ($script:editorProcess) { $script:editorProcess.Dispose() }
    if ($cleanupErrors.Count -gt 0 -and !$failure) { $failure = 'Drag cleanup failed: ' + ($cleanupErrors -join '; ') }
}
if ($failure) { throw $failure }

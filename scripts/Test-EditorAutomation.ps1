# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Requires the Release AOT test project to have been published for Platform.
# This PowerShell process is an external UIA client of the real native editor.
param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [string] $SourceLayout = '',
    [string[]] $Case = @('*')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
if (!$SourceLayout) { $null = Assert-TestBuildIdentity App Release $Platform }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (!('PackagedAppDriver' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs') }
if (!('OwnedWindowActivation' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'OwnedWindowActivation.cs') }
if (!('EditorAutomationInput' -as [type])) {
    $drawingReferences = @([System.Drawing.Bitmap].Assembly.Location, [System.Drawing.Point].Assembly.Location)
    # .NET 10 split the implementation into these assemblies; older PowerShell
    # runtimes do not reference them and need no additional assembly names.
    $drawingReferences += @([System.Drawing.Bitmap].Assembly.GetReferencedAssemblies() |
        Where-Object Name -Like 'System.Private.Windows.*' | ForEach-Object { [Reflection.Assembly]::Load($_).Location })
    Add-Type -Path (Join-Path $PSScriptRoot 'EditorAutomationInput.cs') -ReferencedAssemblies $drawingReferences
}
if (!('EditorNavigationEventObserver' -as [type])) {
    $observerReferences = @([Windows.Automation.AutomationElement].Assembly.Location, [Windows.Automation.AutomationEvent].Assembly.Location)
    if ($PSEdition -eq 'Core') { $observerReferences += @('System.Threading.dll', 'System.Threading.Thread.dll') }
    Add-Type -ReferencedAssemblies $observerReferences -TypeDefinition @'
using System;
using System.Threading;
using System.Windows.Automation;

public sealed class EditorNavigationEventObserver : IDisposable {
    private readonly AutomationEventHandler handler;
    private readonly Thread worker;
    private readonly ManualResetEvent ready = new ManualResetEvent(false);
    private readonly ManualResetEvent stopping = new ManualResetEvent(false);
    private Exception workerError;
    private int textEvents, selectionEvents;
    private long lastEventTicks = DateTime.UtcNow.Ticks;

    public EditorNavigationEventObserver(int processId, string automationId) {
        handler = OnEvent;
        // Windows PowerShell and some pwsh hosts use an STA without a message
        // pump. UIA event subscriptions belong on a non-UI MTA thread. Discover
        // the element there too, rather than passing an STA-created COM proxy.
        worker = new Thread(() => Observe(processId, automationId));
        worker.IsBackground = true;
        worker.SetApartmentState(ApartmentState.MTA);
        worker.Start();
        if (!ready.WaitOne(15000)) {
            stopping.Set();
            throw new TimeoutException("UIA event subscription timed out.");
        }
        if (workerError != null) {
            stopping.Set();
            worker.Join(5000);
            throw new InvalidOperationException("UIA event subscription failed.", workerError);
        }
    }
    private void Observe(int processId, string automationId) {
        AutomationElement element = null;
        bool textSubscribed = false, selectionSubscribed = false;
        try {
            var condition = new AndCondition(
                new PropertyCondition(AutomationElement.ProcessIdProperty, processId),
                new PropertyCondition(AutomationElement.AutomationIdProperty, automationId));
            element = AutomationElement.RootElement.FindFirst(TreeScope.Descendants, condition);
            if (element == null) throw new InvalidOperationException("UIA event source was not found.");
            Automation.AddAutomationEventHandler(TextPattern.TextChangedEvent, element, TreeScope.Element, handler);
            textSubscribed = true;
            Automation.AddAutomationEventHandler(TextPattern.TextSelectionChangedEvent, element, TreeScope.Element, handler);
            selectionSubscribed = true;
            ready.Set();
            stopping.WaitOne();
        } catch (Exception ex) { workerError = ex; }
        finally {
            try {
                if (selectionSubscribed) Automation.RemoveAutomationEventHandler(TextPattern.TextSelectionChangedEvent, element, handler);
            } catch (Exception ex) { workerError = ex; }
            try {
                if (textSubscribed) Automation.RemoveAutomationEventHandler(TextPattern.TextChangedEvent, element, handler);
            } catch (Exception ex) { workerError = ex; }
            ready.Set();
        }
    }
    private void OnEvent(object sender, AutomationEventArgs args) {
        if (args.EventId == TextPattern.TextChangedEvent) Interlocked.Increment(ref textEvents);
        if (args.EventId == TextPattern.TextSelectionChangedEvent) Interlocked.Increment(ref selectionEvents);
        Interlocked.Exchange(ref lastEventTicks, DateTime.UtcNow.Ticks);
    }
    public int TextEvents { get { return Interlocked.CompareExchange(ref textEvents, 0, 0); } }
    public int SelectionEvents { get { return Interlocked.CompareExchange(ref selectionEvents, 0, 0); } }
    public long LastEventTicks { get { return Interlocked.Read(ref lastEventTicks); } }
    public void Reset() {
        Interlocked.Exchange(ref textEvents, 0);
        Interlocked.Exchange(ref selectionEvents, 0);
    }
    public void Dispose() {
        stopping.Set();
        if (!worker.Join(10000)) throw new TimeoutException("UIA event observer did not stop.");
        ready.Dispose();
        stopping.Dispose();
        if (workerError != null) throw new InvalidOperationException("UIA event observer failed.", workerError);
    }
}
'@
}
$script:checks = 0
$script:report = [Collections.Generic.List[string]]::new()
$startEndpoint = [Windows.Automation.Text.TextPatternRangeEndpoint]::Start
$endEndpoint = [Windows.Automation.Text.TextPatternRangeEndpoint]::End
$wordUnit = [Windows.Automation.Text.TextUnit]::Word
$lineUnit = [Windows.Automation.Text.TextUnit]::Line
$characterUnit = [Windows.Automation.Text.TextUnit]::Character

function Assert-Equal($Actual, $Expected, [string] $Label) {
    if ($Actual -cne $Expected) {
        throw "$Label`: expected [$Expected], actual [$Actual]"
    }
    $script:checks++
}

function Assert-True([bool] $Condition, [string] $Label) {
    if (!$Condition) { throw $Label }
    $script:checks++
}

function Reset-Document([string] $Text, [bool] $Wrap = $false, [int] $Width = 520, [int] $FontSize = 18, [int] $Height = 340) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Text))
    $state = Invoke-EditorTestCommand $session "reset|$encoded|$Wrap|$Width|$Height|$FontSize"
    Assert-Equal $pattern.DocumentRange.GetText(-1) $Text 'DocumentRange matches fixture text'
    return $state
}

function New-Caret([int] $CharacterOffset = 0) {
    $range = $pattern.DocumentRange.Clone()
    $range.MoveEndpointByRange($endEndpoint, $range, $startEndpoint)
    if ($CharacterOffset) {
        Assert-Equal $range.MoveEndpointByUnit($startEndpoint, $characterUnit, $CharacterOffset) $CharacterOffset 'Set range character position'
    }
    return $range
}

function Assert-Caret($Range, [string] $Label) {
    Assert-Equal $Range.CompareEndpoints($startEndpoint, $Range, $endEndpoint) 0 $Label
}

function Wait-EditorEventsQuiet {
    # UIA callbacks arrive on a worker thread. Allow at least 300 ms for delivery,
    # then require 250 ms without events before resetting or checking counters.
    $started = [DateTime]::UtcNow
    $deadline = $started.AddSeconds(5)
    do {
        Assert-EditorTestAlive $session
        Start-Sleep -Milliseconds 50
        $now = [DateTime]::UtcNow
        if (($now - $started).TotalMilliseconds -ge 300 -and
            ($now.Ticks - $events.LastEventTicks) -ge [TimeSpan]::FromMilliseconds(250).Ticks) { return }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'UIA events did not settle within five seconds.'
}

function Wait-EditorEvent([ValidateSet('Text', 'Selection')] [string] $Kind) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Assert-EditorTestAlive $session
        $count = if ($Kind -eq 'Text') { $events.TextEvents } else { $events.SelectionEvents }
        if ($count -gt 0) { return $true }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Assert-Traversal($Unit, [string[]] $Expected) {
    $before = Invoke-EditorTestCommand $session 'state'
    Wait-EditorEventsQuiet
    $events.Reset()
    $range = New-Caret
    $range.ExpandToEnclosingUnit($Unit)
    for ($i = 0; $i -lt $Expected.Count; $i++) {
        Assert-Equal $range.GetText(-1) $Expected[$i] "Forward $Unit unit $i"
        if ($i + 1 -lt $Expected.Count) {
            Assert-Equal $range.Move($Unit, 1) 1 "Forward $Unit move count"
        }
    }
    $last = $range.GetText(-1)
    Assert-Equal $range.Move($Unit, 1000) 0 "Nonempty $Unit stops at last unit"
    Assert-Equal $range.GetText(-1) $last "Failed $Unit move preserves range"
    for ($i = $Expected.Count - 1; $i -ge 0; $i--) {
        Assert-Equal $range.GetText(-1) $Expected[$i] "Backward $Unit unit $i"
        if ($i -gt 0) { Assert-Equal $range.Move($Unit, -1) -1 "Backward $Unit move count" }
    }
    Assert-Equal $range.Move($Unit, -1000) 0 "$Unit stops at first unit"
    Assert-Equal $range.GetText(-1) $Expected[0] "$Unit first range preserved"

    $caret = New-Caret
    Assert-Equal $caret.Move($Unit, 1000) $Expected.Count "Degenerate $Unit reaches EOF with correct count"
    Assert-Caret $caret 'Movement preserves degenerate range'
    Assert-Equal $caret.CompareEndpoints($endEndpoint, $pattern.DocumentRange, $endEndpoint) 0 "$Unit caret reaches EOF"
    Assert-Equal $caret.Move($Unit, -1000) (-$Expected.Count) "Degenerate $Unit reaches BOF with correct count"
    Assert-Caret $caret 'Backward movement preserves degenerate range'
    Assert-Equal $caret.CompareEndpoints($startEndpoint, $pattern.DocumentRange, $startEndpoint) 0 "$Unit caret reaches BOF"
    $after = Invoke-EditorTestCommand $session 'state'
    Assert-Equal $after.firstVisible $before.firstVisible "$Unit traversal does not scroll the native viewport"
    Assert-Equal $after.anchor $before.anchor "$Unit traversal does not change native anchor"
    Assert-Equal $after.caret $before.caret "$Unit traversal does not change native caret"
    Wait-EditorEventsQuiet
    Assert-Equal $events.TextEvents 0 "$Unit traversal emits no content events"
    Assert-Equal $events.SelectionEvents 0 "$Unit traversal emits no selection events"
}

function Get-VisualUnits($State) {
    $bytes = [Convert]::FromBase64String($State.text64)
    $boundaries = [Collections.Generic.List[int]]::new()
    $previousY = $null
    foreach ($sample in $State.positionY.Split(',')) {
        $pair = $sample.Split(':')
        $position = [int] $pair[0]
        $y = [int] $pair[1]
        if ($position -lt $bytes.Length -and ($null -eq $previousY -or $y -ne $previousY)) {
            $boundaries.Add($position)
        }
        $previousY = $y
    }
    $boundaries.Add($bytes.Length)
    $units = [Collections.Generic.List[string]]::new()
    for ($i = 0; $i + 1 -lt $boundaries.Count; $i++) {
        $units.Add([Text.Encoding]::UTF8.GetString($bytes, $boundaries[$i], $boundaries[$i + 1] - $boundaries[$i]))
    }
    return ,$units.ToArray()
}

function Test-Case([string] $Name, [scriptblock] $Action) {
    if (@($Case | Where-Object { $Name -like $_ }).Count -eq 0) { return }
    & $Action
    $line = "PASS: $Name"
    $script:report.Add($line)
    Write-Output $line
}

function Wait-EditorState([scriptblock] $Condition, [string] $Label) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $observed = Invoke-EditorTestCommand $session 'state'
        if (& $Condition $observed) { return $observed }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    $evidence = Join-Path $session.Root ("state-failure-" + [Guid]::NewGuid().ToString('N') + '.json')
    $observed | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $evidence -Encoding UTF8
    throw "$Label`: caret=$($observed.caret), anchor=$($observed.anchor), firstVisible=$($observed.firstVisible), rtl=$($observed.rtl), xOffset=$($observed.xOffset). State: $evidence"
}

function Wait-EditorText([string] $Expected, [string] $Label) {
    $state = Wait-EditorState {
        param($observed)
        [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $Expected
    } $Label
    Assert-Equal $pattern.DocumentRange.GetText(-1) $Expected "$Label (external UIA text)"
    return $state
}

function Assert-EditorForeground {
    Assert-EditorTestAlive $session
    $current = $element.Current
    $bounds = $current.BoundingRectangle
    if ($current.ProcessId -ne $session.ProcessId -or !$current.IsEnabled -or $current.IsOffscreen -or
        $bounds.Width -le 0 -or $bounds.Height -le 0) {
        throw 'The owned editor focus target is unavailable.'
    }
    [EditorAutomationInput]::RequireForeground([uint32] $session.ProcessId,
        [int] [Math]::Round($bounds.Left + $bounds.Width / 2),
        [int] [Math]::Round($bounds.Top + $bounds.Height / 2))
}

function Focus-Editor {
    # Startup acquires foreground separately. Later calls restore only child
    # focus inside the already-owned foreground window; interruption aborts.
    Assert-EditorForeground
    $element.SetFocus()
    Assert-EditorForeground
    if (!$element.Current.HasKeyboardFocus) {
        $before = Invoke-EditorTestCommand $session 'state'
        Assert-EditorForeground
        $bounds = $element.Current.BoundingRectangle
        [EditorAutomationInput]::Click([uint32] $session.ProcessId,
            [int] [Math]::Round($bounds.Left + $bounds.Width / 2), [int] [Math]::Round($bounds.Top + [int] $before.lineHeight / 2))
        $deadline = [DateTime]::UtcNow.AddSeconds(2)
        while (!$element.Current.HasKeyboardFocus -and [DateTime]::UtcNow -lt $deadline) {
            Assert-EditorForeground
            Start-Sleep -Milliseconds 25
        }
        Assert-EditorForeground
        if (!$element.Current.HasKeyboardFocus) { throw 'The isolated editor did not acquire keyboard focus after the owned click.' }
        $null = Invoke-EditorTestCommand $session "select|$($before.anchor)|$($before.caret)"
    }
}

function Assert-RightClickCaret($Glyph, [bool] $RightEdge, [int] $Position, [string] $Label) {
    # Hit inside a real glyph/space, close to its requested edge. Its range
    # rectangle is independent of the collapsed caret's leading/trailing home.
    $expectedX = if ($RightEdge) { $Glyph.Right } else { $Glyph.Left }
    $hitX = if ($RightEdge) { $expectedX - 2 } else { $expectedX + 2 }
    [EditorAutomationInput]::RightClick([uint32] $session.ProcessId,
        [int] [Math]::Round($hitX), [int] [Math]::Round($Glyph.Top + $Glyph.Height / 2))
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $state = Invoke-EditorTestCommand $session 'state'
            $rectangles = @($pattern.GetSelection()[0].GetBoundingRectangles())
            if ([int] $state.caret -eq $Position -and $state.anchor -eq $state.caret -and
                $rectangles.Count -eq 1 -and [Math]::Abs($rectangles[0].Left - $expectedX) -le 2 -and
                [Math]::Abs($rectangles[0].Top - $Glyph.Top) -le 1) {
                $script:checks++
                return
            }
            Start-Sleep -Milliseconds 25
        } while ([DateTime]::UtcNow -lt $deadline)
        @{ state = $state; rectangles = $rectangles; expectedX = $expectedX; expectedY = $Glyph.Top } |
            ConvertTo-Json -Depth 5 | Set-Content (Join-Path $session.Root 'right-click-affinity-failure.json')
        throw "$Label`: right-click did not reach byte $Position at the requested visual edge."
    }
    finally { Close-EditorContextMenu }
}

function Assert-EditorKeyboardTarget([switch] $AllowMenuFocus) {
    Assert-EditorTestAlive $session
    if (!$AllowMenuFocus -and !$element.Current.HasKeyboardFocus) {
        throw 'Editor focus changed before physical keyboard input.'
    }
    [EditorAutomationInput]::RequireIdle()
    # The driver checks owned foreground immediately before injecting input.
    # Menu dismissal alone may target a focused menu instead of the editor.
}

function Send-EditorKey([int] $Key, [bool] $Control = $false, [bool] $Shift = $false) {
    Assert-EditorKeyboardTarget
    [PackagedAppDriver]::Key([uint32] $session.ProcessId, [uint16] $Key, $Control, $Shift)
}

function Send-EditorText([string] $Text) {
    Assert-EditorKeyboardTarget
    [PackagedAppDriver]::Type([uint32] $session.ProcessId, $Text)
}

function Close-EditorContextMenu {
    $null = Wait-EditorState { param($state) [int] $state.openPopups -gt 0 } 'The editor context menu opens before dismissal'
    Assert-EditorKeyboardTarget -AllowMenuFocus
    [PackagedAppDriver]::Key([uint32] $session.ProcessId, 0x1b, $false, $false)
    # Escape dispatch and restored focus do not prove that the popup has left
    # the visual tree. A following raster assertion must not capture its ink.
    $null = Wait-EditorState { param($state) [int] $state.openPopups -eq 0 } 'The editor context menu leaves the visual tree'
}

function Read-StableInk([string] $Name, [int] $Rows = 1) {
    $deadline = [DateTime]::UtcNow.AddSeconds(6)
    $state = Invoke-EditorTestCommand $session 'state'
    $previous = $null
    do {
        Assert-EditorTestAlive $session
        $bounds = $element.Current.BoundingRectangle
        # Exclude scrollbar and focus chrome; a rectangle fixture needs two rows.
        $capture = [EditorAutomationInput]::Capture([uint32] $session.ProcessId,
            [int] [Math]::Ceiling($bounds.Left), [int] [Math]::Ceiling($bounds.Top),
            [int] [Math]::Floor($bounds.Width), [int] [Math]::Min($bounds.Height, $Rows * [int] $state.lineHeight + 6),
            (Join-Path $session.Root "$Name.png"))
        if ($previous -and $capture.Fingerprint -eq $previous.Fingerprint -and
            $capture.Left -eq $previous.Left -and $capture.Top -eq $previous.Top -and
            $capture.Width -eq $previous.Width -and $capture.Height -eq $previous.Height) { return $capture }
        $previous = $capture
        Start-Sleep -Milliseconds 60
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "The editor raster did not settle: $Name"
}

function Read-GutterInk([string] $Name, $State, [bool] $Rtl, [int] $Row) {
    $left = [double] $State.surfaceLeft
    if ($Rtl) { $left += [double] $State.surfaceWidth - [double] $State.gutterWidth }
    $top = [double] $State.surfaceTop + $Row * [double] $State.lineHeight
    Assert-True (($Row + 1) * [double] $State.lineHeight -le [double] $State.surfaceHeight) 'The numbered raster row is fully visible'
    $deadline = [DateTime]::UtcNow.AddSeconds(6)
    $previous = $null
    do {
        Assert-EditorTestAlive $session
        $capture = [EditorAutomationInput]::Capture([uint32] $session.ProcessId,
            [int] [Math]::Ceiling($left), [int] [Math]::Ceiling($top),
            [int] [Math]::Floor([double] $State.gutterWidth), [int] $State.lineHeight,
            (Join-Path $session.Root "$Name.png"))
        if ($previous -and $capture.Fingerprint -eq $previous.Fingerprint) { return $capture }
        $previous = $capture
        Start-Sleep -Milliseconds 60
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "The native gutter raster did not settle: $Name"
}

$session = $null
$events = $null
try {
    $session = Start-EditorTestSession -Platform $Platform -SourceLayout $SourceLayout
    Write-Output "Evidence: $($session.Root)"
    @('NotepadsIntegrationTests.exe', 'WinUIEditor.dll') | ForEach-Object {
        Get-FileHash -LiteralPath (Join-Path $session.Layout $_) -Algorithm SHA256
    } | Select-Object Path, Hash | ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $session.Root 'automation-binary-hashes.json') -Encoding UTF8
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int] $session.ProcessId),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, 'InteractiveEditor'))
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $element = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($element) { break }
        Assert-EditorTestAlive $session
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (!$element) { throw 'The external UIA client could not find the isolated editor.' }
    Assert-EditorTestAlive $session
    [EditorAutomationInput]::RequireIdle()
    $element.SetFocus()
    $activation = [OwnedWindowActivation]::Acquire([uint32]$session.ProcessId,
        [PackagedAppDriver]::Window([uint32]$session.ProcessId))
    @{ method = $activation; processId = $session.ProcessId } | ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $session.Root 'initial-foreground.json') -Encoding UTF8
    Focus-Editor
    $pattern = [Windows.Automation.TextPattern] $element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    $events = [EditorNavigationEventObserver]::new([int] $session.ProcessId, 'InteractiveEditor')

    Test-Case 'Repeated reading-order setup completes without requiring an unrelated paint' {
        $text = 'idle אבג editor'
        $null = Reset-Document $text
        # Suppress caret animation so it cannot accidentally satisfy an incorrect
        # paint wait after an unchanged direction. Real changes still await paint.
        $null = Invoke-EditorTestCommand $session 'raster|False'
        foreach ($rtl in @($false, $false, $true, $true, $true, $false, $false)) {
            $state = Invoke-EditorTestCommand $session "direction|$rtl"
            Assert-Equal $state.rtl $rtl.ToString() 'Requested direction was applied'
            Assert-Equal ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.text64))) $text 'Direction setup preserves actual document'
            Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Independent UIA confirms unchanged content'
        }
    }

    Test-Case 'External UIA event observer detects real selection and content changes' {
        $null = Reset-Document 'event listener control'
        Wait-EditorEventsQuiet
        $events.Reset()
        $range = New-Caret
        $range.ExpandToEnclosingUnit($wordUnit)
        $range.Select()
        $state = Invoke-EditorTestCommand $session 'state'
        Assert-Equal $state.anchor '0' 'Positive selection control native anchor'
        Assert-Equal $state.caret '6' 'Positive selection control native caret'
        $selectionReceived = Wait-EditorEvent Selection
        Wait-EditorEventsQuiet
        $selectionContentEvents = $events.TextEvents
        $null = Invoke-EditorTestCommand $session 'state'
        $events.Reset()
        $null = Reset-Document 'changed event listener document'
        $textReceived = Wait-EditorEvent Text
        Wait-EditorEventsQuiet
        $null = Invoke-EditorTestCommand $session 'state'
        Assert-True $selectionReceived 'Explicit Select emits selection notification'
        Assert-Equal $selectionContentEvents 0 'Select emits no content notification'
        Assert-True $textReceived 'Document mutation emits content notification'
    }

    Test-Case 'Word traversal: spaces, punctuation, tabs, Chinese and emoji' {
        $text = "  alpha,`tbeta  你好 😀 end "
        $null = Reset-Document $text
        Assert-Traversal $wordUnit @('  ', "alpha,`t", 'beta  ', '你好 😀 ', 'end ')
        $range = New-Caret 3
        $range.ExpandToEnclosingUnit($wordUnit)
        Assert-Equal $range.GetText(-1) "alpha,`t" 'Interior word expands with separators'
        $boundary = New-Caret 2
        $boundary.ExpandToEnclosingUnit($wordUnit)
        Assert-Equal $boundary.GetText(-1) "alpha,`t" 'Exact boundary expands into next word'
        $state = Invoke-EditorTestCommand $session 'state'
        Assert-Equal $state.anchor '0' 'Navigation does not change native anchor'
        Assert-Equal $state.caret '0' 'Navigation does not change native caret'
        Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Navigation does not edit document'
    }

    Test-Case 'Endpoint movement, return counts, crossing collapse and Select' {
        $null = Reset-Document 'one two three'
        $range = New-Caret
        Assert-Equal $range.MoveEndpointByUnit($endEndpoint, $wordUnit, 2) 2 'End advances two words'
        Assert-Equal $range.GetText(-1) 'one two ' 'Endpoint selects exact words'
        Assert-Equal $range.MoveEndpointByUnit($startEndpoint, $wordUnit, 3) 3 'Start crossing end returns full count'
        Assert-Caret $range 'Start crossing end collapses range'
        Assert-Equal $range.MoveEndpointByUnit($endEndpoint, $wordUnit, -2) -2 'End moves backward across start'
        Assert-Caret $range 'End crossing start collapses range'
        $range.ExpandToEnclosingUnit($wordUnit)
        Assert-Equal $range.GetText(-1) 'two ' 'Crossed range is on second word'
        $range.Select()
        $state = Invoke-EditorTestCommand $session 'state'
        Assert-Equal $state.anchor '4' 'Select reaches real native anchor'
        Assert-Equal $state.caret '8' 'Select reaches real native caret'
        Assert-Equal $pattern.GetSelection()[0].GetText(-1) 'two ' 'Selection reported to external UIA'

        $interior = New-Caret 1
        $null = $interior.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
        Assert-Equal $interior.Move($wordUnit, -1) 0 'Normalization alone is not movement'
        Assert-Equal $interior.GetText(-1) 'n' 'Failed movement retains original nonempty range'
        Assert-Equal $interior.Move($wordUnit, 1) 1 'Successful movement counts units after normalization'
        Assert-Equal $interior.GetText(-1) 'two ' 'Successful movement normalizes to one word'
        Assert-Equal $interior.Move($wordUnit, 0) 0 'Zero count does not move'
        Assert-Equal $interior.GetText(-1) 'two ' 'Zero count preserves text'
    }

    Test-Case 'Empty document, whitespace-only document and EOF expansion' {
        $null = Reset-Document ''
        foreach ($unit in @($wordUnit, $lineUnit)) {
            $range = New-Caret
            $range.ExpandToEnclosingUnit($unit)
            Assert-Equal $range.GetText(-1) '' 'Empty expansion'
            Assert-Equal $range.Move($unit, 10) 0 'Empty Move count'
            Assert-Equal $range.MoveEndpointByUnit($endEndpoint, $unit, -10) 0 'Empty endpoint count'
        }
        $null = Reset-Document " `t  "
        Assert-Traversal $wordUnit @(" `t  ")
        $eof = $pattern.DocumentRange.Clone()
        $eof.MoveEndpointByRange($startEndpoint, $eof, $endEndpoint)
        foreach ($unit in @($wordUnit, $lineUnit)) {
            $range = $eof.Clone()
            $range.ExpandToEnclosingUnit($unit)
            Assert-Equal $range.GetText(-1) " `t  " 'Unterminated EOF expands into the final unit'
        }
        $character = $eof.Clone()
        $character.ExpandToEnclosingUnit($characterUnit)
        Assert-Equal $character.GetText(-1) '' 'Character at EOF stays empty'
    }

    Test-Case 'Hard lines, blank line, trailing newline and Unicode' {
        $text = "first`r`r你好😀`rlast`r"
        $null = Reset-Document $text
        Assert-Traversal $lineUnit @("first`r", "`r", "你好😀`r", "last`r")
        $eof = $pattern.DocumentRange.Clone()
        $eof.MoveEndpointByRange($startEndpoint, $eof, $endEndpoint)
        $eof.ExpandToEnclosingUnit($lineUnit)
        Assert-Equal $eof.GetText(-1) '' 'Trailing newline has an empty final line'
        $range = New-Caret 7
        $range.ExpandToEnclosingUnit($lineUnit)
        Assert-Equal $range.GetText(-1) "你好😀`r" 'Unicode interior line expands including terminator'
        $null = Reset-Document "a`rb"
        Assert-Traversal $lineUnit @("a`r", 'b')
    }

    Test-Case 'Wrapped visual lines match native rendered rows at two widths and font sizes' {
        $text = ('alpha beta gamma 你好 😀 delta epsilon ' * 6) + "`r`rtail"
        $counts = @()
        foreach ($configuration in @(@(230, 18), @(360, 24))) {
            $state = Reset-Document $text $true $configuration[0] $configuration[1]
            $units = Get-VisualUnits $state
            Assert-True ($units.Count -gt 4) 'Fixture must produce multiple wrapped visual rows'
            Assert-Equal ($units -join '') $text 'Rendered rows cover text without loss or duplication'
            Assert-Traversal $lineUnit $units
            $counts += $units.Count
            $paragraph = New-Caret
            $paragraph.ExpandToEnclosingUnit([Windows.Automation.Text.TextUnit]::Paragraph)
            Assert-Equal $paragraph.GetText(-1) ($text.Substring(0, $text.IndexOf("`r") + 1)) 'Paragraph retains whole logical line across wraps'
            $stateAfter = Invoke-EditorTestCommand $session 'state'
            Assert-Equal $stateAfter.anchor '0' 'Wrapped traversal preserves native selection'
        }
        Assert-True ($counts[0] -ne $counts[1]) 'Different layouts must exercise different wrap boundaries'
    }

    Test-Case 'ScrollIntoView reaches an offscreen range and Select updates actual editor' {
        $text = ((0..79 | ForEach-Object { "line $_" }) -join "`r")
        $null = Reset-Document $text
        $range = New-Caret
        Assert-Equal $range.Move($lineUnit, 65) 65 'Move to offscreen line'
        $range.ExpandToEnclosingUnit($lineUnit)
        Assert-Equal $range.GetText(-1) "line 65`r" 'Offscreen line text'
        $range.ScrollIntoView($false)
        $scrolled = Invoke-EditorTestCommand $session 'state'
        Assert-Equal $scrolled.anchor '0' 'ScrollIntoView preserves native anchor before Select'
        Assert-Equal $scrolled.caret '0' 'ScrollIntoView preserves native caret before Select'
        $range.Select()
        $state = Invoke-EditorTestCommand $session 'state'
        $expectedStart = [Text.Encoding]::UTF8.GetByteCount($text.Substring(0, $text.IndexOf('line 65')))
        Assert-Equal ([int] $state.anchor) $expectedStart 'Scrolled selection native anchor'
        Assert-Equal ([int] $state.caret) ($expectedStart + 8) 'Scrolled selection native caret'
        Assert-True ([int] $state.firstVisible -gt 0) 'ScrollIntoView changed actual viewport'
        $sample = @($state.positionY.Split(',') | Where-Object { $_.StartsWith("${expectedStart}:") })
        Assert-Equal $sample.Count 1 'Native position sample exists'
        $y = [int] $sample[0].Split(':')[1]
        Assert-True ($y -ge 0 -and $y -lt ([int] $state.linesOnScreen * [int] $state.lineHeight)) 'Requested range is visible in native viewport'
        Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Scrolling and selection preserve document'
    }

    Test-Case 'Physical Ctrl+R/L, Hebrew arrows, selection, replacement and undo' {
        $initial = Reset-Document 'אבג'
        Focus-Editor
        Send-EditorKey 0x52 $true
        $rtl = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'Ctrl+R changes reading direction'
        Assert-Equal $rtl.revision $initial.revision 'Reading direction does not change document revision'
        foreach ($position in @(2, 4, 6)) {
            Send-EditorKey 0x25
            $null = Wait-EditorState { param($state) [int] $state.caret -eq $position } 'Physical Left follows Hebrew visual order'
        }
        foreach ($position in @(4, 2, 0)) {
            Send-EditorKey 0x27
            $null = Wait-EditorState { param($state) [int] $state.caret -eq $position } 'Physical Right reverses Hebrew visual order'
        }
        Send-EditorKey 0x25 $false $true
        $null = Wait-EditorState { param($state) $state.caret -eq '2' } 'First Shift+Left'
        Send-EditorKey 0x25 $false $true
        $null = Wait-EditorState { param($state) $state.caret -eq '4' } 'Second Shift+Left'
        Assert-Equal $pattern.GetSelection()[0].GetText(-1) 'אב' 'Physical selection exposes exact logical text'
        Send-EditorText 'X'
        $null = Wait-EditorState { param($state) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.text64)) -ceq 'Xג' } 'Physical replacement'
        Assert-Equal $pattern.DocumentRange.GetText(-1) 'Xג' 'Replacement reaches external UIA'
        Send-EditorKey 0x5a $true
        $null = Wait-EditorState { param($state) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.text64)) -ceq 'אבג' } 'Physical Ctrl+Z'
        Send-EditorKey 0x4c $true
        $null = Wait-EditorState { param($state) $state.rtl -eq 'False' } 'Ctrl+L restores LTR paragraph'
        Assert-Equal $pattern.DocumentRange.GetText(-1) 'אבג' 'Direction and undo preserve document bytes'
    }

    Test-Case 'Physical Ctrl+Right crosses indentation before ASCII and mixed words' {
        foreach ($text in @("abc`r    xyz def", "abc`r    אבג def")) {
            foreach ($shift in @($false, $true)) {
                $initial = Reset-Document $text
                Focus-Editor
                $null = Invoke-EditorTestCommand $session 'select|3|3'
                Send-EditorKey 0x27 $true $shift
                $expectedAnchor = if ($shift) { '3' } else { '8' }
                $state = Wait-EditorState {
                    param($observed)
                    $observed.caret -eq '8' -and $observed.anchor -eq $expectedAnchor
                } 'Ctrl+Right reaches the first word after the newline and indentation'
                $selected = @($pattern.GetSelection())
                Assert-Equal $selected.Count 1 'Word navigation exposes one logical selection'
                $expectedText = if ($shift) { "`r    " } else { '' }
                Assert-Equal $selected[0].GetText(-1) $expectedText 'Shift extends over exactly the newline and indentation'
                Assert-Equal $state.revision $initial.revision 'Word navigation leaves the document revision unchanged'
                Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Cross-line word navigation preserves all text'
            }
        }
    }

    Test-Case 'Physical Tab replaces mixed rectangle islands once per row and undoes together' {
        $text = 'abc אבג def'
        $null = Reset-Document $text
        Focus-Editor
        $null = Invoke-EditorTestCommand $session 'select|1|1'
        for ($step = 0; $step -lt 4; $step++) {
            $null = Invoke-EditorTestCommand $session 'rectangle-step|Right'
        }
        $selected = @($pattern.GetSelection())
        Assert-Equal $selected.Count 2 'The mixed rectangle has two logical islands on one visual row'
        Assert-Equal $selected[0].GetText(-1) 'bc ' 'First rectangle island covers the Latin fragment'
        Assert-Equal $selected[1].GetText(-1) 'ג' 'Second rectangle island covers the leftmost Hebrew glyph'
        Assert-Equal (Invoke-EditorTestCommand $session 'state').canUndo 'False' 'Rectangle setup has no undo entry'
        # App.Host fixes the isolated profile to four-space indentation. From
        # column one, one row replacement inserts exactly three spaces.
        Send-EditorKey 0x09
        $expected = 'a   אב def'
        $null = Wait-EditorState {
            param($observed)
            [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $expected
        } 'Physical Tab replaces both islands with one row insertion'
        Assert-Equal $pattern.DocumentRange.GetText(-1) $expected 'Tab preserves the unselected Hebrew letters without a second insertion'
        Send-EditorKey 0x5a $true
        $state = Wait-EditorState {
            param($observed)
            [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $text
        } 'One physical Undo restores both deleted islands and removes the inserted spaces'
        Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'One Undo restores the exact original mixed text'
        Assert-Equal $state.canUndo 'False' 'All rectangle Tab changes belong to one undo group'
    }

    Test-Case 'Physical rectangle overtype replaces selections once and keeps subsequent overwrite' {
        $fixtures = @(
            @{ Name = 'Stream insert'; Rectangle = $false; Overtype = $false; Rows = 1; Reverse = $false },
            @{ Name = 'Stream overwrite'; Rectangle = $false; Overtype = $true; Rows = 1; Reverse = $false },
            @{ Name = 'Rectangle insert'; Rectangle = $true; Overtype = $false; Rows = 1; Reverse = $false },
            @{ Name = 'Two-row rectangle insert'; Rectangle = $true; Overtype = $false; Rows = 2; Reverse = $false }
        )
        foreach ($rows in @(1, 2)) {
            foreach ($reverse in @($false, $true)) {
                $fixtures += @{ Name = "Rectangle overwrite rows=$rows reverse=$reverse"; Rectangle = $true; Overtype = $true; Rows = $rows; Reverse = $reverse }
            }
        }
        foreach ($fixture in $fixtures) {
            $text = ((1..$fixture.Rows | ForEach-Object { 'abc' }) -join "`r")
            $null = Reset-Document $text
            Focus-Editor
            if ($fixture.Overtype) {
                Send-EditorKey 0x2d
                $null = Wait-EditorState { param($state) $state.overtype -eq 'True' } "$($fixture.Name): Insert enables overtype"
            }
            if ($fixture.Rectangle) {
                $start = if ($fixture.Reverse) { 2 } else { 1 }
                $step = if ($fixture.Reverse) { 'Left' } else { 'Right' }
                $null = Invoke-EditorTestCommand $session "select|$start|$start"
                $null = Invoke-EditorTestCommand $session "rectangle-step|$step"
                if ($fixture.Rows -eq 2) { $null = Invoke-EditorTestCommand $session 'rectangle-step|Down' }
            } else { $null = Invoke-EditorTestCommand $session 'select|1|2' }
            $selected = @($pattern.GetSelection())
            Assert-Equal $selected.Count $fixture.Rows "$($fixture.Name): one selected range per row"
            for ($row = 0; $row -lt $fixture.Rows; $row++) {
                Assert-Equal $selected[$row].GetText(-1) 'b' "$($fixture.Name): only b is selected"
                $expectedStart = New-Caret ($row * 4 + 1)
                $expectedEnd = New-Caret ($row * 4 + 2)
                Assert-Equal $selected[$row].CompareEndpoints($startEndpoint, $expectedStart, $startEndpoint) 0 'Selected row starts at b'
                Assert-Equal $selected[$row].CompareEndpoints($endEndpoint, $expectedEnd, $startEndpoint) 0 'Selected row ends before c'
            }
            Assert-Equal (Invoke-EditorTestCommand $session 'state').canUndo 'False' 'Selection and overtype setup add no undo entry'

            # Real text input reaches InsertCharacter; the host insert command
            # uses PasteText and cannot expose the extra overtype deletion.
            Send-EditorText 'X'
            $first = ((1..$fixture.Rows | ForEach-Object { 'aXc' }) -join "`r")
            $null = Wait-EditorText $first "$($fixture.Name): replacing b preserves c"
            $carets = @($pattern.GetSelection())
            Assert-Equal $carets.Count $fixture.Rows 'Replacement retains one insertion caret per row'
            foreach ($caret in $carets) { Assert-Caret $caret 'Replacement leaves empty row selections' }
            Send-EditorText 'Y'
            $second = ((1..$fixture.Rows | ForEach-Object { if ($fixture.Overtype) { 'aXY' } else { 'aXYc' } }) -join "`r")
            $null = Wait-EditorText $second "$($fixture.Name): subsequent input honors insert or overwrite mode"
            Send-EditorKey 0x5a $true
            $null = Wait-EditorText $first "$($fixture.Name): one Undo reverses Y on every row"
            Send-EditorKey 0x5a $true
            $state = Wait-EditorText $text "$($fixture.Name): one more Undo restores the selected text on every row"
            Assert-Equal $state.canUndo 'False' 'Each physical rectangle replacement is one undo group'
        }
    }

    Test-Case 'Physical rectangle repeated overtype retains non-EOF row carets through three inputs' {
        $text = "abcd`rabcd"
        $null = Reset-Document $text
        Focus-Editor
        Send-EditorKey 0x2d
        $null = Wait-EditorState { param($state) $state.overtype -eq 'True' } 'Insert enables repeated rectangular overtype'
        $null = Invoke-EditorTestCommand $session 'select|1|1'
        $null = Invoke-EditorTestCommand $session 'rectangle-step|Right'
        $null = Invoke-EditorTestCommand $session 'rectangle-step|Down'
        $selected = @($pattern.GetSelection())
        Assert-Equal $selected.Count 2 'Repeated overtype starts with one selected range on each row'
        foreach ($range in $selected) { Assert-Equal $range.GetText(-1) 'b' 'Repeated overtype selects only b' }
        Assert-Equal (Invoke-EditorTestCommand $session 'state').canUndo 'False' 'Repeated overtype setup has no undo entry'
        $inputs = @(
            @{ Character = 'X'; Line = 'aXcd'; Caret = 2; Before = 'abcd' },
            @{ Character = 'Y'; Line = 'aXYd'; Caret = 3; Before = 'aXcd' },
            @{ Character = 'Z'; Line = 'aXYZ'; Caret = 4; Before = 'aXYd' }
        )
        foreach ($inputCase in $inputs) {
            Send-EditorText $inputCase.Character
            $expected = $inputCase.Line + "`r" + $inputCase.Line
            $null = Wait-EditorText $expected "Repeated overtype $($inputCase.Character) replaces exactly one character per row"
            $carets = @($pattern.GetSelection())
            Assert-Equal $carets.Count 2 'Repeated overtype retains both row carets'
            for ($row = 0; $row -lt 2; $row++) {
                Assert-Caret $carets[$row] 'Each repeated overwrite leaves an empty row caret'
                $expectedCaret = New-Caret ($row * 5 + $inputCase.Caret)
                Assert-Equal $carets[$row].CompareEndpoints($startEndpoint, $expectedCaret, $startEndpoint) 0 "After $($inputCase.Character), row $row caret starts immediately after the typed character"
                Assert-Equal $carets[$row].CompareEndpoints($endEndpoint, $expectedCaret, $startEndpoint) 0 "After $($inputCase.Character), row $row caret ends immediately after the typed character"
            }
            # After Y, d still follows each caret. EOF clamping cannot conceal
            # an incorrect text-service correction before the next overwrite.
        }
        for ($index = $inputs.Count - 1; $index -ge 0; $index--) {
            Send-EditorKey 0x5a $true
            $expected = $inputs[$index].Before + "`r" + $inputs[$index].Before
            $state = Wait-EditorText $expected "One Undo reverses only $($inputs[$index].Character) on both rows"
            Assert-Equal $state.canUndo ($index -gt 0).ToString() 'Each typed character is exactly one rectangle undo group'
        }
    }

    Test-Case 'Physical rectangle overtype distinguishes replaced rows from empty tab rows' {
        foreach ($overtype in @($false, $true)) {
            $text = "abc`r`tdef"
            $null = Reset-Document $text
            Focus-Editor
            if ($overtype) {
                Send-EditorKey 0x2d
                $null = Wait-EditorState { param($state) $state.overtype -eq 'True' } 'Insert enables the selected/empty-row overwrite fixture'
            }
            # The profile uses four-column tabs. The rectangle spans x=0..1:
            # it selects a on row one, but stops before the tab midpoint on row two.
            $null = Invoke-EditorTestCommand $session 'select|1|1'
            $null = Invoke-EditorTestCommand $session 'rectangle-step|Left'
            $null = Invoke-EditorTestCommand $session 'rectangle-step|Down'
            $selected = @($pattern.GetSelection())
            Assert-Equal $selected.Count 2 'The rectangle has both a selected row and an empty row'
            Assert-Equal $selected[0].GetText(-1) 'a' 'The first row replaces only a'
            Assert-Caret $selected[1] 'The second row selects no part of the tab'
            $tabStart = New-Caret 4
            Assert-Equal $selected[1].CompareEndpoints($startEndpoint, $tabStart, $startEndpoint) 0 'The empty row caret is immediately before the tab'
            Send-EditorText 'X'
            $first = if ($overtype) { "Xbc`rXdef" } else { "Xbc`rX`tdef" }
            $null = Wait-EditorText $first 'Selection replacement preserves b while the empty row honors its input mode'
            Send-EditorText 'Y'
            $second = if ($overtype) { "XYc`rXYef" } else { "XYbc`rXY`tdef" }
            $null = Wait-EditorText $second 'Subsequent input applies the normal mode independently to both thin carets'
            Send-EditorKey 0x5a $true
            $null = Wait-EditorText $first 'One Undo restores both rows before Y'
            Send-EditorKey 0x5a $true
            $state = Wait-EditorText $text 'One more Undo restores the selected character and original tab'
            Assert-Equal $state.canUndo 'False' 'The mixed selected/empty-row edit has one undo group per character'
        }
    }

    foreach ($overtype in @($false, $true)) {
        $mode = if ($overtype) { 'overtype' } else { 'insert' }
        Test-Case "Physical rectangle $mode coalesces mixed islands in both reading orders" {
            foreach ($rtl in @($false, $true)) {
                $label = "Mixed-island $mode rtl=$rtl"
                $line = if ($rtl) { 'אבג ABC' } else { 'abc אבג def' }
                $text = $line + "`r" + $line
                $null = Reset-Document $text
                $null = Invoke-EditorTestCommand $session "direction|$rtl"
                Focus-Editor
                if ($overtype) {
                    Send-EditorKey 0x2d
                    $null = Wait-EditorState { param($state) $state.overtype -eq 'True' } "$label`: Insert enables overtype"
                } else {
                    Assert-Equal (Invoke-EditorTestCommand $session 'state').overtype 'False' "$label`: control uses insert mode"
                }
                $start = if ($rtl) { 0 } else { 1 }
                $step = if ($rtl) { 'Left' } else { 'Right' }
                $steps = if ($rtl) { 6 } else { 4 }
                $null = Invoke-EditorTestCommand $session "select|$start|$start"
                for ($index = 0; $index -lt $steps; $index++) {
                    $null = Invoke-EditorTestCommand $session "rectangle-step|$step"
                }
                $null = Invoke-EditorTestCommand $session 'rectangle-step|Down'
                $selected = @($pattern.GetSelection())
                Assert-Equal $selected.Count 4 "$label`: exactly two islands on each row"
                $islands = if ($rtl) { @('אבג ', 'BC') } else { @('bc ', 'ג') }
                for ($row = 0; $row -lt 2; $row++) {
                    for ($island = 0; $island -lt 2; $island++) {
                        Assert-Equal $selected[$row * 2 + $island].GetText(-1) $islands[$island] "$label`: intended logical island on row $row"
                    }
                }
                Send-EditorText 'X'
                $firstLine = if ($rtl) { 'XA' } else { 'aXאב def' }
                $first = $firstLine + "`r" + $firstLine
                $null = Wait-EditorText $first "$label`: X replaces each row once and preserves unselected text"
                $carets = @($pattern.GetSelection())
                Assert-Equal $carets.Count 2 "$label`: one insertion caret remains on each row"
                $column = if ($rtl) { 1 } else { 2 }
                for ($row = 0; $row -lt 2; $row++) {
                    Assert-Caret $carets[$row] "$label`: replacement leaves an empty caret on row $row"
                    # UIA offsets count UTF-16 characters, not native UTF-8 bytes.
                    # Pin the logical home immediately after X before sending Y;
                    # an echoed TSF selection must not reproject this thin rectangle.
                    $expectedCaret = New-Caret ($row * ($firstLine.Length + 1) + $column)
                    Assert-Equal $carets[$row].CompareEndpoints($startEndpoint, $expectedCaret, $startEndpoint) 0 "$label`: row $row caret starts immediately after X"
                    Assert-Equal $carets[$row].CompareEndpoints($endEndpoint, $expectedCaret, $startEndpoint) 0 "$label`: row $row caret ends immediately after X"
                }
                Send-EditorText 'Y'
                $secondLine = if ($rtl) {
                    if ($overtype) { 'XY' } else { 'XYA' }
                } else {
                    if ($overtype) { 'aXYב def' } else { 'aXYאב def' }
                }
                $second = $secondLine + "`r" + $secondLine
                $null = Wait-EditorText $second "$label`: Y honors the input mode at both retained logical carets"
                Send-EditorKey 0x5a $true
                $null = Wait-EditorText $first "$label`: one Undo restores both rows before Y"
                Send-EditorKey 0x5a $true
                $state = Wait-EditorText $text "$label`: one more Undo restores every original island"
                Assert-Equal $state.canUndo 'False' "$label`: each typed rectangle edit has one undo group"
            }
        }
    }

    Test-Case 'Physical Ctrl+A followed immediately by typing replaces the complete document' {
        foreach ($wrap in @($false, $true)) {
            foreach ($text in @('whole document selection', 'ABC אבג 中文 😀 tail')) {
                $null = Reset-Document $text -Wrap $wrap -Width 260 -FontSize 20
                Focus-Editor
                Assert-EditorKeyboardTarget
                [PackagedAppDriver]::KeyThenCharacter([uint32] $session.ProcessId, 0x41, [char] '|', $true)
                $typed = Wait-EditorText '|' 'Queued Ctrl+A and character replace the complete document'
                Assert-Equal $typed.caret '1' 'Immediate replacement leaves the caret after the character'
                Assert-Equal $typed.anchor '1' 'Immediate replacement collapses the selection'
                Send-EditorKey 0x5a $true
                $state = Wait-EditorText $text 'One Undo restores the complete document after immediate Ctrl+A typing'
                Assert-Equal $state.canUndo 'False' 'Immediate select-all replacement creates one undo action'
            }
        }
    }

    Test-Case 'Physical typing immediately after programmatic selection uses the new insertion range' {
        # Distinct characters expose a stale TSF insertion range even when the
        # document length is correct. Do not wait for a paint after selecting.
        $text = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'
        foreach ($selectionCommand in @('select', 'select-on-key')) {
            foreach ($wrap in @($false, $true)) {
                foreach ($position in @(7, 22)) {
                    foreach ($replaceLength in @(0, 3)) {
                        $null = Reset-Document $text -Wrap $wrap -Width 260 -FontSize 20
                        Focus-Editor
                        $end = $position + $replaceLength
                        $label = "$selectionCommand wrap=$wrap range=$position..$end"
                        $null = Invoke-EditorTestCommand $session "$selectionCommand|$position|$end"
                        if ($selectionCommand -eq 'select-on-key') {
                            Assert-EditorKeyboardTarget
                            [PackagedAppDriver]::KeyThenCharacter([uint32] $session.ProcessId, 0x7e, [char] '|')
                        }
                        else { Send-EditorText '|' }
                        $expected = $text.Substring(0, $position) + '|' + $text.Substring($end)
                        $typed = Wait-EditorText $expected "$label`: physical typing uses the selected interval before paint"
                        Assert-Equal ([int] $typed.caret) ($position + 1) "$label`: caret follows the inserted character"
                        Assert-Equal ([int] $typed.anchor) ($position + 1) "$label`: selection collapses after insertion"
                        Send-EditorKey 0x5a $true
                        $state = Wait-EditorText $text "$label`: one Undo restores the exact original text"
                        Assert-Equal $state.canUndo 'False' "$label`: typing creates one undo action"
                    }
                }
            }
        }
    }

    Test-Case 'Physical typing at a plain soft wrap keeps rectangle edits on the current row' {
        foreach ($width in @(180, 260)) {
            $text = 'a' * 200
            # The first paint can change scrollbar visibility and the text width.
            # Warm that layout before each case resets and measures the same geometry.
            $null = Reset-Document $text -Wrap $true -Width $width -FontSize 20
            foreach ($explicitSelection in @($false, $true)) {
                $null = Reset-Document $text -Wrap $true -Width $width -FontSize 20
                Focus-Editor
                $state = Invoke-EditorTestCommand $session 'state'
                $samples = @($state.positionY.Split(',') | ForEach-Object {
                    $pair = $_.Split(':'); @{ Position = [int] $pair[0]; Y = [int] $pair[1] }
                })
                $boundary = [int] ($samples | Where-Object Y -GT $samples[0].Y | Select-Object -First 1).Position
                Assert-True ($boundary -ge 2 -and $boundary -lt 100) 'The plain-text fixture has a measured soft-wrap boundary'
                $following = New-Caret $boundary
                Assert-Equal $following.MoveEndpointByUnit($endEndpoint, $characterUnit, 1) 1 'Measure the first character on the next visual row'
                $followingBox = @($following.GetBoundingRectangles())
                Assert-Equal $followingBox.Count 1 'The following character has one visible glyph rectangle'
                $before = $boundary - 1
                $null = Invoke-EditorTestCommand $session "select|$before|$before"
                Send-EditorText 'a'
                $inserted = 'a' * 201
                $typed = Wait-EditorText $inserted 'Physical typing ends exactly at the measured soft wrap'
                Assert-Equal ([int] $typed.caret) $boundary 'The typed character leaves the caret at the wrap boundary'
                $afterSamples = @($typed.positionY.Split(',') | ForEach-Object {
                    $pair = $_.Split(':'); @{ Position = [int] $pair[0]; Y = [int] $pair[1] }
                })
                $afterBoundary = [int] ($afterSamples | Where-Object Y -GT $afterSamples[0].Y | Select-Object -First 1).Position
                Assert-Equal $afterBoundary $boundary 'Typing the same-width glyph leaves the measured wrap boundary unchanged'
                # The control resets only affinity at the same byte. The main
                # case retains the caret produced by actual character input.
                if ($explicitSelection) { $null = Invoke-EditorTestCommand $session "select|$boundary|$boundary" }
                $null = Invoke-EditorTestCommand $session 'rectangle-step|Right'
                $selected = @($pattern.GetSelection())
                Assert-Equal $selected.Count 1 'Rectangle Right selects one range only on the current visual row'
                Assert-Equal $selected[0].GetText(-1) 'a' 'Rectangle Right selects exactly one following character'
                $expectedStart = New-Caret $boundary
                $expectedEnd = New-Caret ($boundary + 1)
                Assert-Equal $selected[0].CompareEndpoints($startEndpoint, $expectedStart, $startEndpoint) 0 'Rectangle selection starts at the typed caret'
                Assert-Equal $selected[0].CompareEndpoints($endEndpoint, $expectedEnd, $startEndpoint) 0 'Rectangle selection ends after the first character of the next row'
                $selectedBox = @($selected[0].GetBoundingRectangles())
                Assert-Equal $selectedBox.Count 1 'The rectangle selection has one visible glyph rectangle'
                Assert-True ([Math]::Abs($selectedBox[0].Top - $followingBox[0].Top) -le 1) 'Rectangle selection stays on the measured next visual row'
                Send-EditorText 'X'
                $expected = ('a' * $boundary) + 'X' + ('a' * (200 - $boundary))
                $null = Wait-EditorText $expected 'Rectangle replacement changes only the next-row character and preserves the entire previous row'
                Send-EditorKey 0x5a $true
                $null = Wait-EditorText $inserted 'One Undo restores only the next-row rectangle replacement'
                Send-EditorKey 0x5a $true
                $state = Wait-EditorText $text 'One more Undo removes the character typed at the wrap boundary'
                Assert-Equal $state.canUndo 'False' 'Wrap-boundary typing and rectangle replacement are two undo actions'
            }
        }
    }

    Test-Case 'Physical mixed-direction click inserts into the visible Hebrew character' {
        $null = Reset-Document 'ABC אבג 123 DEF'
        Focus-Editor
        Send-EditorKey 0x52 $true
        $null = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'RTL mouse fixture'
        # The native helper separately checks these positions against whole-line
        # DirectWrite. This layer verifies real screen mapping and mouse delivery.
        $bet = Invoke-EditorTestCommand $session 'geometry|6'
        $gimel = Invoke-EditorTestCommand $session 'geometry|8'
        $clickX = [int] [Math]::Round((3 * [double] $bet.pointX + [double] $gimel.pointX) / 4)
        $clickY = [int] [Math]::Round([double] $bet.pointY + 8)
        [EditorAutomationInput]::Click([uint32] $session.ProcessId, $clickX, $clickY)
        $null = Wait-EditorState { param($state) $state.caret -eq '6' -and $state.anchor -eq '6' } 'Mouse hit reaches exact UTF-8 position'
        Send-EditorText '|'
        $null = Wait-EditorState { param($state) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.text64)) -ceq 'ABC א|בג 123 DEF' } 'Physical mouse insertion'
        Assert-Equal $pattern.DocumentRange.GetText(-1) 'ABC א|בג 123 DEF' 'Mouse insertion matches visible target'
    }

    Test-Case 'Physical right-click preserves a selected Hebrew glyph' {
        $null = Reset-Document 'אבג' -FontSize 28
        Focus-Editor
        Send-EditorKey 0x52 $true
        $null = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'Right-click RTL fixture'
        $null = Invoke-EditorTestCommand $session 'select|0|2'
        $start = Invoke-EditorTestCommand $session 'geometry|0'
        $end = Invoke-EditorTestCommand $session 'geometry|2'
        [EditorAutomationInput]::RightClick([uint32] $session.ProcessId,
            [int] [Math]::Round(([double] $start.pointX + [double] $end.pointX) / 2),
            [int] [Math]::Round([double] $start.pointY + 10))
        # A context menu can hold focus; Escape is still restricted to the
        # owned application and restores the editor's actual keyboard target.
        $selected = Wait-EditorState { param($state) $state.nativeFocus -eq 'False' } 'The Hebrew selection context menu takes keyboard focus'
        Close-EditorContextMenu
        Focus-Editor
        Assert-Equal $selected.anchor '0' 'Right-click inside Hebrew keeps the selection anchor'
        Assert-Equal $selected.caret '2' 'Right-click inside Hebrew keeps the selection caret'
        Assert-Equal $pattern.GetSelection()[0].GetText(-1) 'א' 'Context menu preserves the selected Hebrew text'
    }

    Test-Case 'Physical right-click retains mixed-direction and wrapped caret affinity' {
        # The end of the LTR space and start of the Hebrew run share byte 4
        # but have distinct screen coordinates. Test both relocation from a
        # different byte and an affinity-only relocation of an empty caret.
        $mixed = 'abc אבג def'
        $null = Reset-Document $mixed -FontSize 28
        $space = New-Caret 3
        Assert-Equal $space.MoveEndpointByUnit($endEndpoint, $characterUnit, 1) 1 'Select the boundary space'
        $spaceBox = @($space.GetBoundingRectangles())
        Assert-Equal $spaceBox.Count 1 'The space has one visible rectangle'
        foreach ($initial in @(0, 4)) {
            $null = Invoke-EditorTestCommand $session "select|$initial|$initial"
            $before = @($pattern.GetSelection()[0].GetBoundingRectangles())
            Assert-True ([Math]::Abs($before[0].Left - $spaceBox[0].Right) -gt 4) 'The right-click must change the visible caret position'
            Assert-RightClickCaret $spaceBox[0] $true 4 "Mixed boundary from byte $initial"
        }
        Assert-Equal $pattern.DocumentRange.GetText(-1) $mixed 'Mixed right-click preserves document text'

        foreach ($rtl in @($false, $true)) {
            $text = 'א' * 64
            $state = Reset-Document $text -Wrap $true -Width 260 -FontSize 24
            $state = Invoke-EditorTestCommand $session "direction|$rtl"
            $rows = @($state.positionY.Split(',') | ForEach-Object {
                $sample = $_.Split(':'); @{ Position = [int] $sample[0]; Y = [int] $sample[1] }
            })
            $boundary = [int] ($rows | Where-Object Y -GT $rows[0].Y | Select-Object -First 1).Position
            Assert-True ($boundary -ge 4 -and $boundary -lt [int] $state.length) 'The fixture has a real wrapped boundary'
            $previous = New-Caret ($boundary / 2 - 1)
            $null = $previous.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
            $following = New-Caret ($boundary / 2)
            $null = $following.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
            $previousBox = @($previous.GetBoundingRectangles())
            $followingBox = @($following.GetBoundingRectangles())
            Assert-Equal $previousBox.Count 1 'Last glyph of the first row is visible'
            Assert-Equal $followingBox.Count 1 'First glyph of the next row is visible'
            Assert-True ($followingBox[0].Top -gt $previousBox[0].Top) 'The same byte has two different display rows'
            foreach ($initial in @(0, $boundary)) {
                $null = Invoke-EditorTestCommand $session "select|$initial|$initial"
                Assert-RightClickCaret $previousBox[0] $false $boundary "Wrapped trailing edge rtl=$rtl, initial=$initial"
                Assert-RightClickCaret $followingBox[0] $true $boundary "Wrapped leading edge rtl=$rtl"
            }
            Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Wrapped right-click preserves document text'
            Assert-Equal (Invoke-EditorTestCommand $session 'state').canUndo 'False' 'Right-click creates no undo entry'
        }
    }

    Test-Case 'Physical right-click preserves coincident carets in a zero-width rectangle' {
        foreach ($rtl in @($false, $true)) {
            $text = "אבג`rאבג`rאבג"
            $null = Reset-Document $text -FontSize 28
            $null = Invoke-EditorTestCommand $session "direction|$rtl"
            $null = Invoke-EditorTestCommand $session 'select|2|2'
            $null = Invoke-EditorTestCommand $session 'rectangle-step|Down'
            $null = Invoke-EditorTestCommand $session 'rectangle-step|Down'
            $before = @($pattern.GetSelection())
            Assert-Equal $before.Count 3 'The zero-width rectangle has a caret on all three rows'
            foreach ($range in $before) { Assert-Caret $range 'The rectangle contains only empty ranges' }
            $glyph = New-Caret
            $null = $glyph.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
            $box = @($glyph.GetBoundingRectangles())[0]
            # Both adjacent Hebrew glyph halves hit the same visible caret,
            # one as preceding trailing edge, the other as following leading.
            foreach ($side in @(2, -2)) {
                [EditorAutomationInput]::RightClick([uint32] $session.ProcessId,
                    [int] [Math]::Round($box.Left + $side), [int] [Math]::Round($box.Top + $box.Height / 2))
                try {
                    $null = Wait-EditorState { param($state) $state.nativeFocus -eq 'False' } 'The real context menu has taken keyboard focus'
                    $after = @($pattern.GetSelection())
                    Assert-Equal $after.Count 3 'Right-click on a coincident visual home preserves every rectangle caret'
                    for ($i = 0; $i -lt $before.Count; $i++) {
                        Assert-Caret $after[$i] 'Right-click keeps each rectangle range empty'
                        Assert-Equal $after[$i].CompareEndpoints($startEndpoint, $before[$i], $startEndpoint) 0 'Right-click preserves the logical caret position'
                    }
                }
                finally { Close-EditorContextMenu }
                Assert-EditorForeground
                $element.SetFocus()
                Assert-EditorForeground
                $null = Wait-EditorState { param($state) $state.nativeFocus -eq 'True' } 'Context menu returns keyboard focus'
            }
            Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Right-click leaves rectangle fixture text unchanged'
        }
    }

    Test-Case 'Physical right-click beyond line end does not introduce virtual padding' {
        foreach ($rtl in @($false, $true)) {
            $text = if ($rtl) { 'אבג' } else { 'abc' }
            $state = Reset-Document $text -FontSize 24
            $null = Invoke-EditorTestCommand $session "direction|$rtl"
            $end = Invoke-EditorTestCommand $session "geometry|$($state.length)"
            $hitX = [double] $end.pointX + $(if ($rtl) { -60 } else { 60 })
            [EditorAutomationInput]::RightClick([uint32] $session.ProcessId,
                [int] [Math]::Round($hitX), [int] [Math]::Round([double] $end.pointY + [int] $state.lineHeight / 2))
            try {
                $null = Wait-EditorState { param($observed) $observed.caret -eq $state.length -and $observed.anchor -eq $observed.caret } 'Right-click clamps to the logical line end'
            }
            finally { Close-EditorContextMenu }
            # Restore keyboard focus without a click that could reset the
            # very caret/virtual-space state this regression is observing.
            Assert-EditorForeground
            $element.SetFocus()
            Assert-EditorForeground
            $null = Wait-EditorState { param($observed) $observed.nativeFocus -eq 'True' } 'Editor regains keyboard focus'
            Send-EditorText 'X'
            $expected = $text + 'X'
            $null = Wait-EditorState { param($observed) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $expected } 'Typing at the clamped caret adds no spaces'
            Assert-Equal $pattern.DocumentRange.GetText(-1) $expected 'Right-click outside text retains the normal insertion contract'
            Send-EditorKey 0x5a $true
            $null = Wait-EditorState { param($observed) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $text } 'One undo removes only the typed character'
        }
    }

    Test-Case 'Empty RTL overstrike caret is painted inside the right insertion edge' {
        $null = Reset-Document '' -Width 620 -FontSize 28
        Focus-Editor
        Send-EditorKey 0x52 $true
        $state = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'Empty RTL caret fixture'
        $null = Invoke-EditorTestCommand $session 'raster|False|Caret'
        $point = Invoke-EditorTestCommand $session 'geometry|0'
        Send-EditorKey 0x2d
        try {
            $state = Wait-EditorState { param($state) $state.overtype -eq 'True' } 'Physical Insert enables overstrike'
            Assert-Equal $state.nativeFocus 'True' 'The native caret owns keyboard focus'
            $caretInk = Read-StableInk 'bidi-empty-overstrike-caret'
            $bounds = $element.Current.BoundingRectangle
            $insertionX = [double] $point.pointX - [Math]::Ceiling($bounds.Left)
            Assert-True ($caretInk.Width -ge 8 -and $caretInk.Height -le 3) 'Empty overstrike mode paints the horizontal caret bar'
            Assert-True ($caretInk.Left -gt $bounds.Width / 2 -and $caretInk.Left + $caretInk.Width -le $insertionX + 1) 'The entire caret bar lies inward from the RTL insertion edge'
            Assert-Equal $pattern.DocumentRange.GetText(-1) '' 'Changing caret mode leaves the document empty'
        }
        finally { Send-EditorKey 0x2d }
    }

    Test-Case 'Actual rectangular carets follow the internal visual corner on two long mixed rows' {
        $characters = 40
        $line = 'abc ' + ('א' * $characters) + ' def'
        $hebrewEnd = 4 + 2 * $characters
        $selectedEnd = $hebrewEnd + 2
        $secondLine = [Text.Encoding]::UTF8.GetByteCount($line) + 1
        $null = Reset-Document ($line + "`r" + $line) -Width 260 -FontSize 28
        Focus-Editor
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $null = Invoke-EditorTestCommand $session "select|$selectedEnd|$selectedEnd"
        $initial = Invoke-EditorTestCommand $session 'scroll-caret'
        Assert-True ([int] $initial.xOffset -gt 0) 'Long rectangle fixture starts outside the left viewport'
        for ($step = 0; $step -lt $characters + 2; $step++) {
            $state = Invoke-EditorTestCommand $session 'rectangle-step|Left'
        }
        Assert-Equal ([int] $state.rectangleCaret) $hebrewEnd 'Rectangle reaches the Hebrew run internal visual edge'
        $state = Invoke-EditorTestCommand $session 'rectangle-step|Down'
        # The adjacent space and Hebrew run have equivalent trailing edges.
        # Pixel and island assertions below distinguish these valid nodes from
        # the same byte position with the opposite, visually distant affinity.
        Assert-True ([int] $state.rectangleCaret -in @(($secondLine + 4), ($secondLine + $hebrewEnd))) 'Second row reaches the shared visual boundary'
        $selected = @($pattern.GetSelection())
        Assert-Equal $selected.Count 2 'Rectangle has exactly one complete logical island per row'
        for ($row = 0; $row -lt 2; $row++) {
            $expectedStart = New-Caret ($row * ($line.Length + 1) + 4)
            $expectedEnd = New-Caret ($row * ($line.Length + 1) + 4 + $characters + 2)
            Assert-Equal $selected[$row].CompareEndpoints($startEndpoint, $expectedStart, $startEndpoint) 0 'Rectangle island starts at the first Hebrew character'
            Assert-Equal $selected[$row].CompareEndpoints($endEndpoint, $expectedEnd, $startEndpoint) 0 'Rectangle island includes the complete Hebrew run and adjacent Latin text'
        }
        $offset = $state.xOffset
        $state = Invoke-EditorTestCommand $session 'scroll-caret'
        Assert-Equal $state.xOffset $offset 'ScrollCaret does not jump to the logical island endpoint'
        $null = Invoke-EditorTestCommand $session 'caret-visible|True'
        $ink = Read-StableInk 'bidi-rectangle-carets' 2
        $bounds = $element.Current.BoundingRectangle
        # Independent external UIA geometry identifies the leftmost Hebrew
        # glyph; its left edge is the physical rectangle column.
        $range = New-Caret (4 + $characters - 1)
        $null = $range.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
        $rectangles = @($range.GetBoundingRectangles())
        Assert-Equal $rectangles.Count 1 'The true rectangle edge remains inside the visible viewport'
        $expectedX = $rectangles[0].Left - [Math]::Ceiling($bounds.Left)
        for ($row = 0; $row -lt 2; $row++) {
            $pixels = 0
            for ($y = $row * [int] $state.lineHeight + 4; $y -lt ($row + 1) * [int] $state.lineHeight - 3; $y++) {
                for ($x = 3; $x -lt [int] [Math]::Floor($bounds.Width) - 3; $x++) {
                    if ($ink.PixelRgb($x, $y) -eq 0xff00ff) {
                        Assert-True ([Math]::Abs($x - $expectedX) -le 2) 'Every rectangle caret pixel follows the physical edge'
                        $pixels++
                    }
                }
            }
            Assert-True ($pixels -ge 10) "Rectangle row $row paints a complete visible caret"
        }
        Assert-Equal $pattern.DocumentRange.GetText(-1) ($line + "`r" + $line) 'Caret projection leaves document bytes unchanged'
        Send-EditorKey 0x25
        $null = Wait-EditorState {
            param($state)
            [int] $state.caret -in @(4, $hebrewEnd) -and $state.anchor -eq $state.caret
        } 'Left collapses to the top row shared visual edge'
        Send-EditorKey 0x27
        $null = Wait-EditorState {
            param($state)
            [int] $state.caret -eq ($hebrewEnd - 2) -and $state.anchor -eq $state.caret
        } 'Right enters the Hebrew run from the preserved visual edge'
    }

    Test-Case 'Long mixed paragraph physical typing and undo retain distant UIA selections' {
        $unit = 'abc אבג def '
        $text = $unit * 2000
        $character = $unit.Length * 1800 + 4
        $expected = $text.Remove($character, 3).Insert($character, 'X')
        foreach ($rtl in @($false, $true)) {
            $null = Reset-Document $text -Width 620 -FontSize 24
            Focus-Editor
            $null = Invoke-EditorTestCommand $session "direction|$rtl"
            $range = New-Caret $character
            Assert-Equal $range.MoveEndpointByUnit($endEndpoint, $characterUnit, 3) 3 'Distant selection reaches three characters'
            Assert-Equal $range.GetText(-1) 'אבג' 'Distant selection contains only the intended Hebrew word'
            $range.Select()
            $null = Invoke-EditorTestCommand $session 'scroll-caret'
            Assert-True (@($range.GetBoundingRectangles()).Count -gt 0) 'Distant selected text is physically visible'
            Send-EditorText 'X'
            $null = Wait-EditorText $expected "Long paragraph RTL=$rtl replaces exactly one distant word through physical input"
            Send-EditorKey 0x5a $true
            $state = Wait-EditorText $text "Long paragraph RTL=$rtl restores all text in one physical undo"
            Assert-Equal $state.canUndo 'False' 'Distant physical replacement has exactly one undo action'
            Send-EditorKey 0x59 $true
            $null = Wait-EditorText $expected "Long paragraph RTL=$rtl physical redo restores the same replacement"
            Send-EditorKey 0x5a $true
            $null = Wait-EditorText $text "Long paragraph RTL=$rtl final undo restores the exact original"
        }
    }

    Test-Case 'Partial emoji selection preserves neighboring glyph pixels and geometry' {
        # Strong RTL text ensures both base directions use paragraph shaping.
        # The wrapped fixture also exercises row filtering during selection.
        $fixtures = @(
            @{ Name = 'line'; Text = 'א 👩‍💻X'; Wrap = $false; Width = 620 },
            @{ Name = 'wrapped'; Text = 'א 👩‍💻X ' + ('word ' * 20); Wrap = $true; Width = 220 }
        )
        foreach ($fixture in $fixtures) {
            foreach ($rtl in @($false, $true)) {
                $null = Reset-Document $fixture.Text -Wrap $fixture.Wrap -Width $fixture.Width -FontSize 28
                Focus-Editor
                $null = Invoke-EditorTestCommand $session "direction|$rtl"
                $state = Invoke-EditorTestCommand $session 'raster|False'
                if ($fixture.Wrap) {
                    $rowPositions = @($state.positionY.Split(',') | ForEach-Object { [int] $_.Split(':')[1] })
                    Assert-True (($rowPositions | Measure-Object -Maximum).Maximum -gt 0) 'The emoji fixture actually wraps onto another display row'
                }
                $name = "bidi-emoji-selection-$($fixture.Name)-$rtl"
                $before = Read-StableInk "$name-before" 2
                # Hebrew, space, woman, ZWJ and laptop are five Unicode scalars.
                $neighbor = New-Caret 5
                $null = $neighbor.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
                Assert-Equal $neighbor.GetText(-1) 'X' 'Independent UIA range identifies the unselected neighboring glyph'
                $neighborBounds = @($neighbor.GetBoundingRectangles())
                Assert-Equal $neighborBounds.Count 1 'Neighbor glyph has one visible rectangle before selection'
                $bounds = $element.Current.BoundingRectangle
                # Keep a fixed region inside X's own cell. Do not move the
                # expected region if selection causes the renderer to move X.
                $left = [int] [Math]::Ceiling($neighborBounds[0].Left - [Math]::Ceiling($bounds.Left)) + 2
                $right = [int] [Math]::Floor($neighborBounds[0].Right - [Math]::Ceiling($bounds.Left)) - 2
                $top = [int] [Math]::Ceiling($neighborBounds[0].Top - [Math]::Ceiling($bounds.Top)) + 2
                $bottom = [int] [Math]::Floor($neighborBounds[0].Bottom - [Math]::Ceiling($bounds.Top)) - 2
                Assert-True ($right - $left -ge 8 -and $bottom - $top -ge 12 -and $top -ge 0 -and
                    $bottom -lt 2 * [int] $state.lineHeight + 6) 'Neighbor glyph region is visible and nonempty'
                $inkPixels = 0
                for ($y = $top; $y -lt $bottom; $y++) {
                    for ($x = $left; $x -lt $right; $x++) {
                        $rgb = $before.PixelRgb($x, $y)
                        if (($rgb -band 0xff) -lt 192 -and (($rgb -shr 8) -band 0xff) -lt 192 -and
                            (($rgb -shr 16) -band 0xff) -lt 192) { $inkPixels++ }
                    }
                }
                Assert-True ($inkPixels -ge 20) 'The fixed neighbor region contains real dark glyph pixels'
                # Hebrew + space consume three bytes; the woman scalar uses
                # four more and is only part of the joined emoji cluster.
                $null = Invoke-EditorTestCommand $session 'select|3|7'
                $selected = @($pattern.GetSelection())
                Assert-Equal $selected.Count 1 'Partial emoji selection remains one logical range'
                Assert-Equal $selected[0].GetText(-1) '👩' 'Selection contains exactly the first emoji scalar'
                $highlighted = Read-StableInk "$name-selected" 2
                $selectedNeighborBounds = @($neighbor.GetBoundingRectangles())
                Assert-Equal $selectedNeighborBounds.Count 1 'Neighbor glyph remains visible after partial selection'
                Assert-Equal $selectedNeighborBounds[0] $neighborBounds[0] 'Selection does not change neighboring glyph geometry'
                for ($y = $top; $y -lt $bottom; $y++) {
                    for ($x = $left; $x -lt $right; $x++) {
                        Assert-Equal $highlighted.PixelRgb($x, $y) $before.PixelRgb($x, $y) 'Partial emoji selection preserves every neighboring glyph pixel'
                    }
                }
                $null = Invoke-EditorTestCommand $session 'select|0|0'
                $restored = Read-StableInk "$name-cleared" 2
                Assert-Equal $restored.Fingerprint $before.Fingerprint 'Clearing partial emoji selection restores the original pixels'
                Assert-Equal $pattern.DocumentRange.GetText(-1) $fixture.Text 'Foreground painting leaves the logical document unchanged'
            }
        }
    }

    Test-Case 'Native line numbers move sides without mirroring and physical gutter clicks select exact lines' {
        $lines = @(1..40 | ForEach-Object { ('F12 אבג {0:D2} ' -f $_) + ('א' * 100) })
        $text = $lines -join "`r"
        $null = Reset-Document $text -Width 620 -Height 440
        $null = Invoke-EditorTestCommand $session 'line-numbers|True'
        $null = Invoke-EditorTestCommand $session 'raster|False'
        Focus-Editor
        $reference = $null
        $iteration = 0
        foreach ($rtl in @($false, $true, $false)) {
            Send-EditorKey $(if ($rtl) { 0x52 } else { 0x4c }) $true
            $null = Wait-EditorState { param($state) $state.rtl -eq $rtl.ToString() } 'Physical reading-order command moves the gutter'
            $null = Invoke-EditorTestCommand $session 'scroll-caret'
            $state = Invoke-EditorTestCommand $session 'gutter-state'
            Assert-True ([int] $state.gutterWidth -gt 20) 'The real native number margin is enabled'
            Assert-True ([double] $state.scrollbarWidth -gt 0) 'The fixture has a visible vertical scrollbar'
            $scrollbarEdge = [double] $state.scrollbarLeft
            $surfaceEdge = [double] $state.surfaceLeft + [double] $state.surfaceWidth
            if ($rtl) {
                $scrollbarEdge += [double] $state.scrollbarWidth
                $surfaceEdge = [double] $state.surfaceLeft
            }
            Assert-True ([Math]::Abs($scrollbarEdge - $surfaceEdge) -lt 2) 'The vertical scrollbar occupies the outer left in RTL and outer right in LTR'
            Assert-Equal $state.firstVisible '0' 'Line-number fixture retains its first physical row'
            $ink = Read-GutterInk "bidi-gutter-line12-$iteration" $state $rtl 11
            if ($null -eq $reference) {
                $reference = $ink
                Assert-True (!$reference.SameShape($reference, $true)) 'Line 12 raster rejects mirrored or reversed digit shapes'
            } else {
                Assert-True ($reference.SameShape($ink, $false)) 'Moving the native gutter preserves the original line 12 glyph shapes'
            }
            foreach ($buffered in @($true, $false)) {
                $null = Invoke-EditorTestCommand $session "buffered-draw|$buffered"
                $painted = Read-GutterInk "bidi-gutter-buffered-$iteration-$buffered" $state $rtl 11
                Assert-True ($ink.Fingerprint -eq $painted.Fingerprint) 'Buffered and direct drawing preserve the exact gutter raster'
            }
            $offset = if ([int] $state.xOffset -eq 0) { 100 } else { 0 }
            $scrolled = Invoke-EditorTestCommand $session "x-offset|$offset"
            Assert-True ($scrolled.xOffset -ne $state.xOffset) 'The gutter raster fixture actually scrolls horizontally'
            $painted = Read-GutterInk "bidi-gutter-scroll-$iteration" $state $rtl 11
            Assert-True ($ink.Fingerprint -eq $painted.Fingerprint) 'Horizontal scrolling does not move or overwrite line numbers'
            $length = [Text.Encoding]::UTF8.GetByteCount($text)
            $null = Invoke-EditorTestCommand $session "select|0|$length"
            $painted = Read-GutterInk "bidi-gutter-selected-$iteration" $state $rtl 11
            Assert-True ($ink.Fingerprint -eq $painted.Fingerprint) 'A full-document selection paints no foreground or background in the gutter'
            $null = Invoke-EditorTestCommand $session 'select|0|0'
            $null = Invoke-EditorTestCommand $session 'scroll-caret'

            # Compute the click from the actual surface bounds and configured
            # width, independently of native hit testing or a gutter-position API.
            $gutterX = [double] $state.surfaceLeft + [double] $state.gutterWidth / 2
            if ($rtl) { $gutterX = [double] $state.surfaceLeft + [double] $state.surfaceWidth - [double] $state.gutterWidth / 2 }
            [EditorAutomationInput]::Click([uint32] $session.ProcessId,
                [int] [Math]::Round($gutterX),
                [int] [Math]::Round([double] $state.surfaceTop + 1.5 * [double] $state.lineHeight))
            $start = [Text.Encoding]::UTF8.GetByteCount($lines[0]) + 1
            $end = $start + [Text.Encoding]::UTF8.GetByteCount($lines[1]) + 1
            $null = Wait-EditorState {
                param($observed)
                [Math]::Min([long] $observed.anchor, [long] $observed.caret) -eq $start -and
                    [Math]::Max([long] $observed.anchor, [long] $observed.caret) -eq $end
            } 'Physical margin click selects the second logical line including its EOL'
            Assert-Equal $pattern.GetSelection()[0].GetText(-1) ($lines[1] + "`r") 'Gutter selection exposes the exact logical row'
            # A click captures/releases the pointer; observe a separate real
            # hover move after release instead of relying on capture's events.
            [EditorAutomationInput]::Move([uint32] $session.ProcessId,
                [int] [Math]::Round($gutterX),
                [int] [Math]::Round([double] $state.surfaceTop + 1.5 * [double] $state.lineHeight + 4))
            $hoverDeadline = [DateTime]::UtcNow.AddSeconds(2)
            do {
                $hover = Invoke-EditorTestCommand $session 'gutter-state'
                if ($hover.glowLeft -eq $(if ($rtl) { '1' } else { '0' }) -and
                    $hover.glowRight -eq $(if ($rtl) { '0' } else { '1' })) { break }
                Start-Sleep -Milliseconds 30
            } while ([DateTime]::UtcNow -lt $hoverDeadline)
            Assert-Equal $hover.glowLeft $(if ($rtl) { '1' } else { '0' }) 'Hover edge is on the text side of the gutter'
            Assert-Equal $hover.glowRight $(if ($rtl) { '0' } else { '1' }) 'Hover does not leave a border on the outer edge'

            if ($rtl) {
                # ScrollCaret can leave the LTR prefix of an RTL line clipped
                # to a one-pixel bidi boundary. Expose the entire F before an
                # exact physical hit test, independent of endpoint affinity.
                $bodyState = Invoke-EditorTestCommand $session 'gutter-state'
                $null = Invoke-EditorTestCommand $session "x-offset|$([int] $bodyState.xOffset + [int] $bodyState.gutterWidth)"
            }
            $glyph = New-Caret ($lines[0].Length + 1)
            Assert-Equal $glyph.MoveEndpointByUnit($endEndpoint, $characterUnit, 1) 1 'Body hit fixture selects F'
            Assert-Equal $glyph.GetText(-1) 'F' 'Independent UIA range identifies the first body glyph of row two'
            $bounds = @($glyph.GetBoundingRectangles())
            Assert-Equal $bounds.Count 1 'The body glyph is visible beside the moved gutter'
            $glyphStart = Invoke-EditorTestCommand $session "geometry|$start"
            $glyphEnd = Invoke-EditorTestCommand $session "geometry|$($start + 1)"
            $advance = [double] $glyphEnd.pointX - [double] $glyphStart.pointX
            Assert-True ($advance -gt 4 -and [Math]::Abs($bounds[0].Width - $advance) -le 1) 'The physical hit fixture exposes the whole F rather than a clipped bidi boundary'
            $hitX = [Math]::Round($bounds[0].Left + $bounds[0].Width / 4)
            $hitY = [Math]::Round($bounds[0].Top + $bounds[0].Height / 2)
            $hit = $pattern.RangeFromPoint([Windows.Point]::new($hitX, $hitY))
            $expectedCaret = New-Caret ($lines[0].Length + 1)
            Assert-Equal $hit.CompareEndpoints($startEndpoint, $expectedCaret, $startEndpoint) 0 'External UIA hit testing accounts for the moved scrollbar and text surface'
            Assert-Caret $hit 'UIA point hit returns an empty range rather than the whole line'
            $interiorCaret = New-Caret ($lines[0].Length + 2)
            $interiorGlyph = $interiorCaret.Clone()
            Assert-Equal $interiorGlyph.MoveEndpointByUnit($endEndpoint, $characterUnit, 1) 1 'Interior hit fixture advances over one character'
            Assert-Equal $interiorGlyph.GetText(-1) '1' 'Interior hit fixture is beyond the logical line start'
            $interiorBounds = @($interiorGlyph.GetBoundingRectangles())
            Assert-Equal $interiorBounds.Count 1 'Interior hit fixture has one visible glyph rectangle'
            Assert-True ($interiorBounds[0].Width -gt 4) 'Interior glyph exposes a usable hit target'
            $interiorHit = $pattern.RangeFromPoint([Windows.Point]::new(
                $interiorBounds[0].Left + $interiorBounds[0].Width / 4,
                $interiorBounds[0].Top + $interiorBounds[0].Height / 2))
            Assert-Equal $interiorHit.CompareEndpoints($startEndpoint, $interiorCaret, $startEndpoint) 0 'UIA point hit preserves the exact interior character instead of snapping to line start'
            Assert-Caret $interiorHit 'UIA interior hit returns an empty range'
            [EditorAutomationInput]::Click([uint32] $session.ProcessId,
                [int] [Math]::Round($hitX), [int] [Math]::Round($hitY))
            $null = Wait-EditorState { param($observed) [long] $observed.caret -eq $start -and [long] $observed.anchor -eq $start } 'Body click remains an insertion point rather than a gutter selection'
            Send-EditorKey 0x2e
            $expected = $text.Remove($lines[0].Length + 1, 1)
            $null = Wait-EditorState { param($observed) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $expected } 'Physical Delete edits exactly the clicked glyph'
            Send-EditorKey 0x5a $true
            $null = Wait-EditorState { param($observed) [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($observed.text64)) -ceq $text } 'Undo restores the exact document after the body hit'
            [EditorAutomationInput]::Click([uint32] $session.ProcessId,
                [int] [Math]::Round([double] $state.scrollbarLeft + [double] $state.scrollbarWidth / 2),
                [int] [Math]::Round([double] $state.scrollbarTop + 0.8 * [double] $state.scrollbarHeight))
            $null = Wait-EditorState { param($observed) [long] $observed.firstVisible -gt 0 } 'The physically moved scrollbar still scrolls document rows'
            Focus-Editor
            Send-EditorKey 0x24 $true
            $null = Wait-EditorState { param($observed) $observed.firstVisible -eq '0' -and $observed.caret -eq '0' } 'Ctrl+Home restores the top after physical scrollbar input'
            $iteration++
        }
        $hidden = Invoke-EditorTestCommand $session 'line-numbers|False'
        Assert-Equal $hidden.gutterWidth '0' 'Disabling line numbers removes the native gutter'
        Assert-Equal $pattern.DocumentRange.GetText(-1) $text 'Gutter direction changes preserve all document content'
    }

    Test-Case 'Actual editor pixels preserve Latin glyphs and Arabic shaping across direction/styles' {
        $null = Reset-Document 'ABC F123' -Width 620 -FontSize 28
        Focus-Editor
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $latinLtr = Read-StableInk 'bidi-latin-ltr'
        Send-EditorKey 0x52 $true
        $null = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'RTL Latin raster'
        $latinRtl = Read-StableInk 'bidi-latin-rtl'
        Assert-True ($latinRtl.Left -gt $latinLtr.Left + 100) 'RTL changes paragraph alignment in actual rendered pixels'
        Assert-True ($latinLtr.SameShape($latinRtl, $false)) 'RTL retains the actual Latin glyph raster'
        Assert-True (!$latinLtr.SameShape($latinLtr, $true)) 'Raster fixture rejects mirrored glyphs as a negative control'

        $null = Reset-Document 'السلام عليكم' -Width 620 -FontSize 28
        Focus-Editor
        Send-EditorKey 0x52 $true
        $null = Wait-EditorState { param($state) $state.rtl -eq 'True' } 'Arabic styled raster'
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $arabicPlain = Read-StableInk 'bidi-arabic-plain'
        $null = Invoke-EditorTestCommand $session 'raster|True'
        $arabicStyled = Read-StableInk 'bidi-arabic-styled'
        Assert-True ($arabicPlain.Fingerprint -ne $arabicStyled.Fingerprint) 'The color-style fixture actually changes rendered colors'
        Assert-True ($arabicPlain.SameShape($arabicStyled, $false)) 'Color-only style boundaries preserve actual Arabic joining pixels'
        Assert-True ([Math]::Abs($arabicPlain.Left - $arabicStyled.Left) -le 1) 'Styled Arabic remains at the same physical origin'
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $arabicRestored = Read-StableInk 'bidi-arabic-restored'
        Assert-Equal $arabicRestored.Fingerprint $arabicPlain.Fingerprint 'Returning to one foreground restores the original Arabic pixels'
        Assert-Equal $pattern.DocumentRange.GetText(-1) 'السلام عليكم' 'Raster setup leaves logical document text unchanged'
    }

    Test-Case 'Actual selection pixels preserve the unselected glyph between two visual islands' {
        $null = Reset-Document 'abc אבג def' -Width 620 -FontSize 28
        Focus-Editor
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $bounds = $element.Current.BoundingRectangle
        $samples = foreach ($offset in @(1, 4, 6)) {
            $range = New-Caret $offset
            $null = $range.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
            $rectangles = @($range.GetBoundingRectangles())
            Assert-Equal $rectangles.Count 1 'Each sampled glyph occupies one visual rectangle'
            $rectangle = $rectangles[0]
            [pscustomobject] @{
                Character = $offset
                ElementLeft = $bounds.Left
                ElementTop = $bounds.Top
                RectangleLeft = $rectangle.Left
                RectangleTop = $rectangle.Top
                RectangleWidth = $rectangle.Width
                RectangleHeight = $rectangle.Height
                X = [int] [Math]::Round($rectangle.Left + $rectangle.Width / 2 - [Math]::Ceiling($bounds.Left))
                Y = [int] [Math]::Round($rectangle.Top + 3 - [Math]::Ceiling($bounds.Top))
            }
        }
        $plain = Read-StableInk 'bidi-selection-before'
        $null = Invoke-EditorTestCommand $session 'select|1|8'
        Assert-Equal $pattern.GetSelection()[0].GetText(-1) 'bc אב' 'Raster selection has the fixed logical text'
        $selected = Read-StableInk 'bidi-selection-after'
        # Prove that selection exercised a different foreground, not just a
        # background fill. The fixture sets normal black and selected white.
        $glyph = $samples[0]
        $glyphLeft = [int] [Math]::Ceiling($glyph.RectangleLeft - [Math]::Ceiling($bounds.Left))
        $glyphRight = [int] [Math]::Floor($glyph.RectangleLeft + $glyph.RectangleWidth - [Math]::Ceiling($bounds.Left))
        $contrastingInk = $false
        for ($y = $plain.Top; $y -lt $plain.Top + $plain.Height -and !$contrastingInk; $y++) {
            for ($x = $glyphLeft; $x -lt $glyphRight -and !$contrastingInk; $x++) {
                $before = $plain.PixelRgb($x, $y)
                $after = $selected.PixelRgb($x, $y)
                $dark = ($before -band 255) -le 32 -and (($before -shr 8) -band 255) -le 32 -and (($before -shr 16) -band 255) -le 32
                $light = ($after -band 255) -ge 192 -and (($after -shr 8) -band 255) -ge 192 -and (($after -shr 16) -band 255) -ge 192
                $contrastingInk = $dark -and $light
            }
        }
        Assert-True $contrastingInk 'Selected Latin ink changes from black to white before testing drawing-effect cleanup'
        $samples | ConvertTo-Json | Set-Content (Join-Path $session.Root 'bidi-selection-samples.json')
        Invoke-EditorTestCommand $session 'geometry|1' | ConvertTo-Json | Set-Content (Join-Path $session.Root 'bidi-selection-native-geometry.json')
        for ($index = 0; $index -lt 2; $index++) {
            $point = $samples[$index]
            Assert-True ($plain.PixelRgb($point.X, $point.Y) -ne $selected.PixelRgb($point.X, $point.Y)) "Selected island $index changes background at $($point.X),$($point.Y) (before=$($plain.PixelRgb($point.X, $point.Y)), after=$($selected.PixelRgb($point.X, $point.Y)))"
        }
        $gap = $samples[2]
        Assert-Equal $selected.PixelRgb($gap.X, $gap.Y) $plain.PixelRgb($gap.X, $gap.Y) 'The unselected Hebrew glyph retains its background between selected islands'
        # Selection changes repaint the cached paragraph without changing its
        # text or styles. This exercises drawing-effect cleanup before the next
        # single-foreground paint of the same layout.
        $null = Invoke-EditorTestCommand $session 'select|0|0'
        $cleared = Read-StableInk 'bidi-selection-cleared'
        Assert-Equal $cleared.Fingerprint $plain.Fingerprint 'Clearing selection restores the original foreground and background pixels'
    }

    Test-Case 'Selecting only EOL does not paint over an opposite-direction trailing run' {
        $null = Reset-Document "abc אבג`rnext" -Width 620 -FontSize 28
        Focus-Editor
        $null = Invoke-EditorTestCommand $session 'raster|False'
        $bounds = $element.Current.BoundingRectangle
        $samples = foreach ($offset in @(4, 5, 6)) {
            $range = New-Caret $offset
            $null = $range.MoveEndpointByUnit($endEndpoint, $characterUnit, 1)
            $rectangle = @($range.GetBoundingRectangles())[0]
            [pscustomobject] @{
                X = [int] [Math]::Round($rectangle.Left + $rectangle.Width / 2 - [Math]::Ceiling($bounds.Left))
                Y = [int] [Math]::Round($rectangle.Top + 3 - [Math]::Ceiling($bounds.Top))
            }
        }
        $plain = Read-StableInk 'bidi-eol-before'
        $null = Invoke-EditorTestCommand $session 'select|10|11'
        Assert-Equal $pattern.GetSelection()[0].GetText(-1) "`r" 'Only the newline is selected'
        $selected = Read-StableInk 'bidi-eol-after'
        Assert-True ($plain.Fingerprint -ne $selected.Fingerprint) 'Selecting the newline changes its painted marker'
        foreach ($point in $samples) {
            Assert-Equal $selected.PixelRgb($point.X, $point.Y) $plain.PixelRgb($point.X, $point.Y) 'EOL highlight leaves every trailing Hebrew glyph background unchanged'
        }
    }

    if ($script:report.Count -eq 0) { throw "No automation case matched '$($Case -join ', ')'." }
    $summary = "PASS: external UIA, physical RTL input and raster checks, $($script:report.Count) cases, $script:checks assertions"
    $script:report.Add($summary)
    Write-Output $summary
}
catch {
    $script:report.Add('FAIL: ' + $_.ToString())
    throw
}
finally {
    try { if ($events) { $events.Dispose() } }
    finally {
        if ($session) {
            try { $script:report | Set-Content -LiteralPath (Join-Path $session.Root 'automation-results.txt') -Encoding UTF8 }
            finally { Stop-EditorTestSession $session }
        }
    }
}

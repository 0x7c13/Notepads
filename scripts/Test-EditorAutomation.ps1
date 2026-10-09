# ---------------------------------------------------------------------------------------------
# Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
# See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Requires the Release AOT test project to have been published for Platform.
# This PowerShell process is an external UIA client of the real native editor.
param([ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
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

function Reset-Document([string] $Text, [bool] $Wrap = $false, [int] $Width = 520, [int] $FontSize = 18) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Text))
    $state = Invoke-EditorTestCommand $session "reset|$encoded|$Wrap|$Width|340|$FontSize"
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
    & $Action
    $line = "PASS: $Name"
    $script:report.Add($line)
    Write-Output $line
}

$session = $null
$events = $null
try {
    $session = Start-EditorTestSession -Platform $Platform
    Write-Output "Evidence: $($session.Root)"
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
    $pattern = [Windows.Automation.TextPattern] $element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    $events = [EditorNavigationEventObserver]::new([int] $session.ProcessId, 'InteractiveEditor')

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

    $summary = "PASS: external UIA navigation, $($script:report.Count) cases, $script:checks assertions"
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

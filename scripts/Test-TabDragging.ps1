# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Requires an idle interactive desktop. Only exact owned test windows receive input.
param(
    [ValidateSet('x64', 'ARM64')][string] $Platform = 'ARM64',
    [switch] $ShadowsOnly,
    [ValidateSet(100, 125, 150, 200)][int[]] $ScalePercent = @()
)
$ErrorActionPreference = 'Stop'
if ($ScalePercent.Count -and !$ShadowsOnly) { throw 'Display scaling is only changed by the explicit ShadowsOnly run.' }
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
. (Join-Path $PSScriptRoot 'DisplayScaleSettings.ps1')
$null = Assert-TestBuildIdentity App Release $Platform
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -AssemblyName System.Drawing
foreach ($helper in @('PackagedAppDriver', 'EditorDragInput', 'OwnedWindowActivation')) {
    if (!($helper -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot "$helper.cs") }
}
if (!('EditorAutomationInput' -as [type])) {
    $references = @([System.Drawing.Bitmap].Assembly.Location, [System.Drawing.Point].Assembly.Location)
    $references += @([System.Drawing.Bitmap].Assembly.GetReferencedAssemblies() |
        Where-Object Name -Like 'System.Private.Windows.*' | ForEach-Object { [Reflection.Assembly]::Load($_).Location })
    Add-Type -Path (Join-Path $PSScriptRoot 'EditorAutomationInput.cs') -ReferencedAssemblies $references
}
if (!('TabDragRasterCapture' -as [type])) {
    $references = @((Get-ChildItem (Join-Path $PSHOME 'ref/*.dll')).FullName +
        [System.Drawing.Bitmap].Assembly.Location, [System.Drawing.Point].Assembly.Location)
    $references += @([System.Drawing.Bitmap].Assembly.GetReferencedAssemblies() |
        Where-Object Name -Like 'System.Private.Windows.*' | ForEach-Object { [Reflection.Assembly]::Load($_).Location })
    Add-Type -Path (Join-Path $PSScriptRoot 'TabDragRasterCapture.cs') -ReferencedAssemblies ($references | Select-Object -Unique)
}
$session = $peer = $driver = $null
$settings = $null
$restoreScale = $false
$raster = $null
$handles = @{}
$results = [Collections.Generic.List[object]]::new()
$checks = 0
function Check([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
    $script:checks++
}
function State($Owner = $session) { Invoke-EditorTestCommand $Owner 'tabs-state' }
function Wait-State([scriptblock] $Predicate, [string] $Message, $Owner = $session) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    do {
        $state = State $Owner
        if (& $Predicate $state) { return $state }
        Start-Sleep -Milliseconds 25
    } while ($watch.Elapsed.TotalSeconds -lt 8)
    throw "$Message.`nLast state:`n$($state | ConvertTo-Json)"
}
function Header([int] $Index, $Owner = $session) {
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($Owner.ProcessId))
    $tab = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, "DragTab$Index"))
    if (!$tab) { throw "Tab $Index is not exposed by the owned workspace." }
    $text = $tab.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Text))
    if (!$text) { throw "Tab $Index has no text peer." }
    $rect = $text.Current.BoundingRectangle
    if ($rect.Width -lt 20 -or $rect.Height -le 0) { throw "Tab $Index has no visible hit target." }
    return $rect
}
function Point($Rect) { @{ X = [int]($Rect.Left + $Rect.Width / 2); Y = [int]($Rect.Top + $Rect.Height / 2) } }
function Arrow([string] $Name) {
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $button = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Name))
    if (!$button -or $button.Current.BoundingRectangle.Width -le 0) { throw "No measured overflow arrow: $Name" }
    return $button.Current.BoundingRectangle
}
function Capture-Rail([string] $Name) {
    $rect = Header 0
    $null = [EditorAutomationInput]::Capture($session.ProcessId, [int]($rect.Left - 6), [int]($rect.Top - 4),
        [int]($rect.Width + 12), [int]($rect.Height + 8), (Join-Path $session.Root "$Name.png"))
}
function Start-Raster([string] $Name) {
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $rects = @(0..2 | ForEach-Object {
        $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, "DragTab$_")).Current.BoundingRectangle
    })
    # Headers may still be settling when a native drag returns to the rail.
    # Cropping to their transient bounds can permanently cut off a marker that
    # continues moving toward the edge. Capture the owned window's full rail
    # band instead; the driver's process/occlusion guards still check every frame.
    $window = $root.Current.BoundingRectangle
    $left = [int]($window.Left + 8)
    $right = [int]($window.Right - 8)
    $text = Header 0
    $script:rasterMinimumWidth = ($rects | Measure-Object Width -Minimum).Minimum
    $script:raster = [TabDragRasterCapture]::new($handles[[uint32]$session.ProcessId], $left,
        [int]$text.Top, $right - $left, [int]$text.Height, (Join-Path $session.Root "$Name-frames"))
}
function Finish-Motion([string] $Name, [string] $Order, [int] $Source, [bool] $Aligned, [bool] $RequireSourceResidual = $true) {
    $raster.Finish()
    $frames = @($raster.Frames)
    $release = $raster.Release
    $releaseSampleMilliseconds = $raster.ReleaseSampleMilliseconds
    $minimumWidth = $script:rasterMinimumWidth
    $script:raster = $null
    1..6 | ForEach-Object { $null = Invoke-EditorTestCommand $session 'tabs-frame' }
    $trace = State
    @{ release = $release; releaseSampleMilliseconds = $releaseSampleMilliseconds; minimumHeaderWidth = $minimumWidth; frames = $frames } | ConvertTo-Json -Depth 6 |
        Set-Content -LiteralPath (Join-Path $session.Root "$Name-raster.json")
    Check ($trace.tabOrder -eq $Order) "$Name did not commit its expected roster."
    $after = @($frames | Where-Object Milliseconds -GT $release.Milliseconds)
    Check ($after.Count -ge 3) 'Release raster must contain several actual screen samples.'
    $previous = $release
    if (!$Aligned -and $trace.tabAnimations -eq 'True') {
        if ($RequireSourceResidual) {
            Check ([Math]::Abs($after[-1].X[$Source] - $previous.X[$Source]) -gt 5) "$Name did not exercise an unfinished source settlement."
        }
        foreach ($sample in $after) {
            $elapsed = [Math]::Clamp(($sample.Milliseconds - $release.Milliseconds) / 90.0, 0.0, 1.0)
            $progress = 1 - [Math]::Pow(1 - $elapsed, 3)
            # The source was stationary under the pointer. Its new 90 ms cubic
            # settlement bounds displacement independently of frame rate. The
            # neighbors retain older preview clocks and must not use this clock.
            $allowed = [Math]::Abs($after[-1].X[$Source] - $release.X[$Source]) * $progress + 1.5
            Check ([Math]::Abs($sample.X[$Source] - $release.X[$Source]) -le $allowed) "$Name jumped instead of continuing from its current position."
        }
    }
    foreach ($sample in $after) {
        for ($index = 0; $index -lt 3; $index++) {
            if ($Aligned -or $index -eq $Source) {
                Check ([double]::IsFinite($sample.X[$index])) "$Name lost a visible tab marker."
            }
            # During a partial crossing the opaque source may cover a neighbor.
            # Its missing pixels are occlusion, not a reset; the final marker
            # must reappear, and the always-visible source is checked throughout.
            $initial = $release.X[$index]
            $final = $after[-1].X[$index]
            Check ([double]::IsFinite($final)) "$Name did not restore a final tab marker."
            if (![double]::IsFinite($initial) -or ![double]::IsFinite($sample.X[$index])) { continue }
            $minimum = [Math]::Min($initial, $final)
            $maximum = [Math]::Max($initial, $final)
            # Text-center targeting can leave a few DIPs of source residual;
            # aligned neighbors must remain exactly where the user last saw them.
            if ($Aligned -and $index -ne $Source) { $minimum = $maximum = $initial }
            Check ($sample.X[$index] -ge $minimum - 1 -and $sample.X[$index] -le $maximum + 1) "$Name replayed tab $index from its old slot."
            if ([double]::IsFinite($previous.X[$index])) {
                if ($final -gt $initial) { Check ($sample.X[$index] -ge $previous.X[$index] - 1) "$Name reversed a rightward settlement." }
                if ($final -lt $initial) { Check ($sample.X[$index] -le $previous.X[$index] + 1) "$Name reversed a leftward settlement." }
            }
        }
        $previous = $sample
    }
    if ($trace.tabAnimations -eq 'True') {
        if ($Aligned) {
            # Every aligned fixture crosses neighbor 1. A header that never
            # changed slots cannot establish coverage for an old-slot replay.
            $marker = 1
            $jump = $minimumWidth
            $tolerance = 1.0
        }
        elseif ($RequireSourceResidual) {
            $marker = $Source
            $jump = [Math]::Abs($after[-1].X[$Source] - $release.X[$Source])
            $tolerance = 1.5
        }
        else {
            # An already aligned source has no meaningful residual to animate.
            # Its full permitted band still must expose an old-slot replay.
            $marker = $Source
            $jump = $minimumWidth
            $tolerance = [Math]::Abs($after[-1].X[$Source] - $release.X[$Source]) + 1.5
        }
        [TabDragRasterCapture]::RequireSettlementSamples($release, $frames, $marker, $jump, $tolerance)
    }
    Finish $Name
}
function Ready([int] $Count = 3, [string] $Mode = 'equal') {
    $null = Invoke-EditorTestCommand $session "tabs-start|$Count|$Mode"
    $null = Invoke-EditorTestCommand $session 'tabs-frame'
    # UIA can focus the exact editor through the UWP frame when an ordinary
    # foreground request is denied. This is setup, before any gesture oracle.
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $editor = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty, 'WinUIEditor.EditorBaseControl'),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$session.ProcessId)))
    if (!$editor -or $editor.Current.IsOffscreen -or !$editor.Current.IsEnabled) { throw 'No visible owned editor for tab-drag setup.' }
    [OwnedWindowActivation]::RequireIdle()
    $editor.SetFocus()
    $null = [OwnedWindowActivation]::Acquire($session.ProcessId, [PackagedAppDriver]::Window($session.ProcessId))
    $driver.Activate($session.ProcessId)
}
function Finish([string] $Name) {
    $state = Wait-State { param($s) $s.tabTransferCompleting -eq 'False' } "$Name did not finish transfer cleanup"
    Check ($state.tabDocuments -eq 'True') "$Name changed document identity or undo history."
    $process = $handles[[uint32]$session.ProcessId]
    $process.Refresh()
    $results.Add(@{ name = $Name; state = $state; cpuMs = $process.TotalProcessorTime.TotalMilliseconds;
        privateBytes = $process.PrivateMemorySize64; workingSet = $process.WorkingSet64 })
    Write-Host "PASS: TabDragging.$Name"
}
function Check-Keyboard([string] $Name) {
    $state = Wait-State { param($s) $s.tabFocus -eq 'True' } "$Name did not retain editor focus"
    $originalText64 = $state.tabText
    $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($state.tabText))
    [EditorAutomationInput]::RequireIdle()
    [PackagedAppDriver]::Key([uint32]$session.ProcessId, 0x23, $true, $false) # Ctrl+End
    [PackagedAppDriver]::Type([uint32]$session.ProcessId, '|')
    $expected = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text + '|'))
    $null = Wait-State { param($s) $s.tabText -eq $expected } "$Name could not type into the retained editor"
    [EditorAutomationInput]::RequireIdle()
    [PackagedAppDriver]::Key([uint32]$session.ProcessId, 0x5A, $true, $false) # Ctrl+Z
    $null = Wait-State { param($s) $s.tabText -eq $originalText64 } "$Name could not undo the typed character"
    Check $true "$Name retains real keyboard input and undo."
}
function Retain-Source($Owner) {
    $process = [Diagnostics.Process]::GetProcessById($Owner.ProcessId)
    try {
        $null = $process.SafeHandle
        if ($process.StartTime.ToUniversalTime().Ticks -ne $Owner.ProcessIdentity.StartTicks -or
            ![string]::Equals($process.MainModule.FileName, $Owner.ProcessIdentity.Path, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Retained input source does not match the verified isolated process.'
        }
        $script:handles[[uint32]$Owner.ProcessId] = $process
    }
    catch { $process.Dispose(); throw }
}

function Check-EdgeShadows([string] $Left, [string] $Right, [string] $Label) {
    $null = Wait-State { param($s) $s.tabLeftEdgeShadow -eq $Left -and $s.tabRightEdgeShadow -eq $Right } $Label
    Check $true $Label
}

function Capture-Shadows([string] $Name, [int] $Index) {
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $window = $root.Current.BoundingRectangle
    $text = Header $Index
    $scale = [double](State).tabScale
    $null = [EditorAutomationInput]::Capture($session.ProcessId, [int]($window.Left + 8),
        [int]($text.Top - 5 * $scale), [int]($window.Width - 16), [int](40 * $scale),
        (Join-Path $session.Root "$Name.png"))
}

function Capture-BottomShadow([string] $Name, $Slot, [double] $Scale) {
    # Observe the vacated trailing strip of the original slot, away from text,
    # the moving selected header and both side shadows. Compare the actual DWM
    # pixels near the bottom with the same columns above the shadow's blur.
    $null = Invoke-EditorTestCommand $session 'tabs-frame'
    $path = Join-Path $session.Root "$Name-bottom.png"
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $window = $root.Current.BoundingRectangle
    $left = [int]($window.Left + 8)
    # Capture the rail (including text) through the existing guarded helper;
    # its text-fixture check intentionally rejects a blank light-theme crop.
    $null = [EditorAutomationInput]::Capture($session.ProcessId,
        $left, [int]$Slot.Top, [int]($window.Width - 16), [int]$Slot.Height, $path)
    $bitmap = [Drawing.Bitmap]::new($path)
    try {
        $above = $bottom = 0.0
        $count = 0
        for ($x = [int]($Slot.Right - 18 * $Scale) - $left; $x -le [int]($Slot.Right - 14 * $Scale) - $left; $x++) {
            $a = $bitmap.GetPixel($x, $bitmap.Height - [int](12 * $Scale))
            $b = $bitmap.GetPixel($x, $bitmap.Height - [int](2 * $Scale))
            $above += ($a.R + $a.G + $a.B) / 3.0
            $bottom += ($b.R + $b.G + $b.B) / 3.0
            $count++
        }
        $sample = @{ above = $above / $count; bottom = $bottom / $count; contrast = ($above - $bottom) / $count }
        $sample | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root "$Name-bottom.json")
        return $sample
    }
    finally { $bitmap.Dispose() }
}

function Test-EdgeShadows([string] $Mode, [int] $Count, [string] $Label) {
    Ready $Count $Mode
    if ($script:requestedScale) {
        $null = Wait-State { param($s) [Math]::Abs([double]$s.tabScale * 100 - $script:requestedScale) -lt .01 } 'The tab rail must observe the requested Windows DPI'
    }
    $last = $Count - 1
    if ($Count -gt 2) {
        # Use the actual overflow buttons to reveal the last tab.
        $arrow = Point (Arrow 'SetsScrollForwardButton')
        $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
        $forward = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, 'SetsScrollForwardButton'))
        $driver.Begin($session.ProcessId, $arrow.X, $arrow.Y, $false)
        try {
            $deadline = [DateTime]::UtcNow.AddSeconds(8)
            while ($forward.Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 25 }
            Check (!$forward.Current.IsEnabled) 'The overflow arrow reaches the actual end of the rail'
        }
        finally { $driver.End($session.ProcessId, $arrow.X, $arrow.Y) }
    }
    $start = Point (Header $last)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.End($session.ProcessId, $start.X, $start.Y)
    $null = Wait-State { param($s) $s.tabSelected -eq "$last" } 'The last tab is selected'
    Check-EdgeShadows 'Collapsed' 'Visible' 'The stationary last tab has its edge shadow'
    Capture-Shadows "$Label-selected" $last
    $scale = [double](State).tabScale
    $root = [Windows.Automation.AutomationElement]::FromHandle([PackagedAppDriver]::Window($session.ProcessId))
    $slot = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, "DragTab$last")).Current.BoundingRectangle
    $selectedShadow = Capture-BottomShadow "$Label-selected" $slot $scale
    $neighborSlot = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, "DragTab$($last - 1)")).Current.BoundingRectangle
    $neighborSample = @{ Right = $neighborSlot.Left + 30 * $scale; Top = $neighborSlot.Top; Height = $neighborSlot.Height }
    $neighborShadow = Capture-BottomShadow "$Label-neighbor" $neighborSample $scale
    # Stay before the reorder threshold so no neighbor fills the source's gap.
    $destinationX = $start.X - [int](32 * $scale)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destinationX, $start.Y, 8)
    Check-EdgeShadows 'Collapsed' 'Collapsed' 'Dragging the last tab leaves no fixed shadow at its former edge'
    Capture-Shadows "$Label-drag" $last
    $gapShadow = Capture-BottomShadow "$Label-drag" $slot $scale
    Check ($gapShadow.contrast -gt .5 -and [Math]::Abs($gapShadow.contrast - $neighborShadow.contrast) -le 1.5) 'The vacated source slot matches the neighboring bottom shadow in rendered pixels'
    $movingNeighborShadow = Capture-BottomShadow "$Label-moving-neighbor" $neighborSample $scale
    Check ([Math]::Abs($movingNeighborShadow.contrast - $neighborShadow.contrast) -le 1.5) 'The shared shadow does not double-darken stationary neighbors'
    $driver.Escape(); $driver.ReleaseEscape()
    $driver.End($session.ProcessId, $destinationX, $start.Y)
    Check-EdgeShadows 'Collapsed' 'Visible' 'Escape restores the stationary edge shadow'
    Check ((State).tabOrder -eq ((0..$last) -join ',')) 'Escape preserves tab order'
    Capture-Shadows "$Label-cancel" $last
    $restoredShadow = Capture-BottomShadow "$Label-cancel" $slot $scale
    # The upper row crosses the selected close glyph at some fractional DPIs.
    # Its antialiasing is unrelated to shadow cleanup; compare the clear bottom
    # row itself against its pre-drag value instead.
    Check ([Math]::Abs($restoredShadow.bottom - $selectedShadow.bottom) -le 1) 'Escape restores the original selected bottom edge in rendered pixels'
    $start = Point (Header $last)
    $neighbor = Point (Header ($last - 1))
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($neighbor.X, $start.Y, 8)
    Check-EdgeShadows 'Collapsed' 'Collapsed' 'Reordering has no fixed edge shadow during preview'
    $driver.End($session.ProcessId, $neighbor.X, $start.Y)
    $expected = @(0..$last)
    $expected[$last - 1] = $last
    $expected[$last] = $last - 1
    $null = Wait-State { param($s) $s.tabOrder -eq ($expected -join ',') } 'Reorder commits the last tab into its neighbor slot'
    $left = if ($Count -eq 2) { 'Visible' } else { 'Collapsed' }
    Check-EdgeShadows $left 'Collapsed' 'Settlement restores shadows at the new selected position'
    Capture-Shadows "$Label-reordered" $last
    Finish $Label
}
try {
    $session = Start-EditorTestSession -Platform $Platform
    $run = [Guid]::NewGuid().ToString('N')
    $identity = Start-IsolatedTestProcess $session "--interactive-test $run"
    $peer = @{ ProcessIdentity = $identity; ProcessId = $identity.Id; Sequence = 0;
        Channel = (Join-Path $session.RunRoot "InteractiveTests/$run"); Root = $session.Root }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while (!(Test-Path -LiteralPath (Join-Path $peer.Channel 'ready.txt'))) {
        Assert-EditorTestAlive $peer
        if ($watch.Elapsed.TotalSeconds -gt 30) { throw 'Owned peer did not become ready.' }
        Start-Sleep -Milliseconds 25
    }
    Retain-Source $session
    Retain-Source $peer
    $driver = [EditorDragInput]::new($session.ProcessId, $peer.ProcessId)

    if ($ScalePercent.Count) {
        $settings = Open-DisplayScaleSettings
        Assert-DisplayScaleAvailable $settings
    }
    foreach ($scale in $(if ($ScalePercent.Count) { $ScalePercent } else { @(0) })) {
        $script:requestedScale = $scale
        if ($scale) {
            $restoreScale = $true
            Set-DisplayScalePercent $settings $scale
        }
        foreach ($mode in @('equal', 'light')) {
            Test-EdgeShadows $mode 2 "EdgeShadows-$mode-$scale"
            Test-EdgeShadows $mode 12 "OverflowShadows-$mode-$scale"
        }
    }
    if ($ShadowsOnly) { return }

    Ready
    $start = Point (Header 2)
    $destination = Point (Header 1)
    $other = Point (Header 0)
    $targetX = $destination.X + 50
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($targetX, $start.Y, 1)
    $driver.End($session.ProcessId, $targetX, $start.Y)
    # SendInput queues events; it is not acknowledgment that UWP released the
    # previous capture. Observe the commit, then hold the new press until the
    # independently reported selection changes. No animation-settling delay.
    $null = Wait-State { param($s) $s.tabOrder -eq '0,2,1' } 'The first release did not commit before the next click'
    $driver.Begin($session.ProcessId, $other.X, $other.Y, $false)
    $null = Wait-State { param($s) $s.tabSelected -eq '0' } 'A click after reorder did not select the requested tab'
    $driver.End($session.ProcessId, $other.X, $other.Y)
    $null = Wait-State { param($s) $s.tabOrder -eq '0,2,1' -and $s.tabSelected -eq '0' } 'A click after reorder lost the committed order or selection'
    Finish 'ClickAfterReorder'

    Ready
    $before = Header 0
    $start = Point $before
    $destination = Point (Header 2)
    Write-Host "Rail input: $($start | ConvertTo-Json -Compress) -> $($destination | ConvertTo-Json -Compress)"
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destination.X, $start.Y + 9, 12)
    $state = Invoke-EditorTestCommand $session 'tabs-frame'
    $during = Header 0
    Check ([Math]::Abs($during.Top - $before.Top) -le 1 -and $during.Left -gt $before.Left + 20) 'Rail must move text horizontally while keeping its measured Y.'
    Check ($state.tabStarts -eq '0' -and $state.tabNativeStarts -eq '0' -and $state.tabOrder -eq '0,1,2') 'Preview must neither promote nor mutate the roster.'
    Check ($state.tabDragOpaque -eq 'True' -and $state.tabTheme -eq 'Dark') 'The dark rail source must have an opaque themed background.'
    Capture-Rail 'rail-dark'
    $driver.End($session.ProcessId, $destination.X, $start.Y + 9)
    $state = Wait-State { param($s) $s.tabOrder -eq '1,2,0' } 'Local release did not commit the preview'
    $state = Invoke-EditorTestCommand $session 'tabs-frame'
    $state = Wait-State { param($s) $s.tabDragOpaque -eq 'False' } 'Local settlement did not retire its opaque preview'
    Check ($state.tabUnloads -eq '0' -and $state.tabSelected -eq '0' -and $state.tabSelections -eq '0') 'Selected reorder detached the editor or emitted transient selection.'
    Check ($state.tabDragOpaque -eq 'False') 'Local commit must clear its opaque preview.'
    Check-Keyboard 'LocalReorder'
    Finish 'RailGeometryAndSelectedReorder'

    foreach ($index in @(0, 1)) {
        $null = Invoke-EditorTestCommand $session 'tabs-focus-action'
        $unfocused = Wait-State { param($s) $s.tabFocus -eq 'False' } 'The status action did not move focus outside the editor'
        Check ($unfocused.tabFocus -eq 'False') 'The click fixture must begin with focus outside the editor.'
        $p = Point (Header $index)
        $driver.Begin($session.ProcessId, $p.X, $p.Y, $false)
        $driver.End($session.ProcessId, $p.X, $p.Y)
        $null = Wait-State { param($s) $s.tabSelected -eq "$index" -and $s.tabFocus -eq 'True' } 'An ordinary tab click must select and focus its editor'
    }
    Finish 'ClickAndEditorFocus'

    foreach ($modifier in @(@{ name = 'Control'; shift = $false; alt = $false; mask = 1 },
        @{ name = 'ControlShift'; shift = $true; alt = $false; mask = 5 },
        @{ name = 'ControlAlt'; shift = $false; alt = $true; mask = 3 })) {
        Ready
        foreach ($index in @(0, 1)) {
            $before = State
            $p = Point (Header $index)
            $driver.Begin($session.ProcessId, $p.X, $p.Y, $true, $modifier.shift, $modifier.alt)
            $pressed = Wait-State { param($s) [int]$s.tabPresses -gt [int]$before.tabPresses } 'The modified press did not reach the measured tab'
            Check ($pressed.tabPressed -eq "$index" -and [int]$pressed.tabPressModifiers -eq $modifier.mask) 'The tab must observe the actual modifier chord at press time.'
            Check ($pressed.tabSelected -eq '0' -and $pressed.tabSelections -eq '0') 'Ctrl and its variants must not select or deselect tabs on press.'
            $driver.End($session.ProcessId, $p.X, $p.Y)
            $released = Wait-State { param($s) [int]$s.tabReleases -gt [int]$before.tabReleases } 'The modified release was not dispatched'
            $driver.ReleaseOwnedKeys()
            $state = Invoke-EditorTestCommand $session 'tabs-frame'
            Check ($state.tabSelected -eq '0' -and $state.tabOrder -eq '0,1,2' -and $state.tabSelections -eq '0') 'Modified clicks must preserve the selection and roster after release.'
            Check ($state.tabStarts -eq '0' -and $state.tabNativeStarts -eq '0' -and $state.tabInteractions -eq '0') 'Modified clicks must not enter the rail or start a native drag.'
            Check ($state.tabUnloads -eq '0' -and $state.tabDragOpaque -eq 'False') 'Modified clicks must retain the editor and normal header appearance.'
            Finish "$($modifier.name)Click-$index"
        }
    }

    Ready 3 'light'
    $start = Point (Header 0)
    $destination = Point (Header 1)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destination.X, $start.Y, 12)
    $state = Invoke-EditorTestCommand $session 'tabs-frame'
    Check ($state.tabDragOpaque -eq 'True' -and $state.tabTheme -eq 'Light') 'The light rail source must have an opaque themed background.'
    Capture-Rail 'rail-light'
    $driver.Escape(); $driver.ReleaseEscape()
    $driver.End($session.ProcessId, $destination.X, $start.Y)
    $state = State
    Check ($state.tabDragOpaque -eq 'False' -and $state.tabOrder -eq '0,1,2') 'Cancellation must restore the normal background and roster.'
    Finish 'OpaqueLightBackgroundAndRestore'

    Ready
    $start = Point (Header 0)
    $destination = Point (Header 2)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destination.X, $start.Y, 12)
    $driver.Escape()
    $driver.ReleaseEscape()
    $driver.End($session.ProcessId, $destination.X, $start.Y)
    $state = State
    Check ($state.tabOrder -eq '0,1,2' -and $state.tabStarts -eq '0') 'Local Escape must roll back the preview.'
    Check-Keyboard 'LocalEscape'
    Finish 'LocalEscape'

    # Sample release frames, not just final order. All three equal-width headers
    # are already at their final visual slots before this perfectly aligned drop.
    foreach ($case in @(@{ source = 2; mode = 'equal'; order = '0,2,1' },
        @{ source = 0; mode = 'equal'; order = '1,0,2' },
        @{ source = 2; mode = 'rtl'; order = '0,2,1' },
        @{ source = 2; mode = 'unequal-tail'; order = '0,2,1' },
        @{ source = 2; mode = 'adaptive'; order = '0,2,1,3,4,5,6,7,8,9,10,11' })) {
        $count = if ($case.mode -eq 'adaptive') { 12 } else { 3 }
        Ready $count $case.mode
        if ($case.mode -eq 'adaptive') { $null = Arrow 'SetsScrollForwardButton' }
        $sourceHeader = Header $case.source
        $start = Point $sourceHeader
        $destination = Point (Header 1)
        $neighbor = Header 1
        $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
        $null = Wait-State { param($s) $s.tabSelected -eq "$($case.source)" } 'The rail press did not select its source'
        # SendInput queues the press. Confirm its target and the actual cursor
        # before MoveTo interpolates from that position; do not retarget a
        # gesture silently if input or layout moved during admission.
        $driver.RequireCursorAt($session.ProcessId, $start.X, $start.Y)
        $driver.RequireHeldMouse()
        $pressedHeader = Header $case.source
        @{ mode = $case.mode; source = $case.source; start = $start; destination = $destination;
            before = $sourceHeader; after = $pressedHeader } | ConvertTo-Json -Depth 4 |
            Set-Content -LiteralPath (Join-Path $session.Root "AlignedRelease-$($case.source)-$($case.mode)-input.json")
        Check ([Math]::Abs($pressedHeader.Top - $sourceHeader.Top) -lt 1 -and
            [Math]::Abs($pressedHeader.Height - $sourceHeader.Height) -lt 1) 'Selection moved the measured rail band.'
        $driver.MoveTo($destination.X, $start.Y, 12)
        $watch = [Diagnostics.Stopwatch]::StartNew()
        do {
            $moved = Header 1
            if ([Math]::Abs($moved.Left - $neighbor.Left) -gt $neighbor.Width / 2) { break }
            if ($watch.Elapsed.TotalSeconds -gt 5) { throw 'Neighbor preview did not reach its adjacent slot.' }
        } while ($true)
        # Five render fences let the 90 ms neighbor animation finish; assertions
        # below use measured positions, never elapsed time as a correctness gate.
        1..5 | ForEach-Object { $null = Invoke-EditorTestCommand $session 'tabs-frame' }
        Start-Raster "AlignedRelease-$($case.source)-$($case.mode)"
        $raster.MarkRelease()
        $driver.End($session.ProcessId, $destination.X, $start.Y)
        Finish-Motion "AlignedRelease-$($case.source)-$($case.mode)" $case.order $case.source $true
    }

    foreach ($source in @(0, 2)) {
        Ready
        $start = Point (Header $source)
        $destination = Point (Header 1)
        # Release slightly short of the slot, while the neighbor's 90 ms slide
        # is still running. The source and neighbor both retain real residuals.
        $targetX = $destination.X + [Math]::Sign($start.X - $destination.X) * 50
        $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
        Start-Raster "ResidualRelease-$source"
        $initial = $raster.Frames[0].X[$source]
        $driver.MoveTo($targetX, $start.Y, 1)
        $raster.WaitForSource($source, $initial + $targetX - $start.X)
        $raster.MarkRelease()
        $driver.End($session.ProcessId, $targetX, $start.Y)
        $order = if ($source -eq 0) { '1,0,2' } else { '0,2,1' }
        Finish-Motion "ResidualRelease-$source" $order $source $false
    }

    Ready
    $start = Point (Header 0)
    $destination = Point (Header 1)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    Start-Raster 'QuickAlignedRelease'
    $initial = $raster.Frames[0].X[0]
    $driver.MoveTo($destination.X, $start.Y, 1)
    $raster.WaitForSource(0, $initial + $destination.X - $start.X)
    $raster.MarkRelease()
    $driver.End($session.ProcessId, $destination.X, $start.Y)
    Finish-Motion 'QuickAlignedRelease' '1,0,2' 0 $false $false


    Ready
    $start = Point (Header 0)
    $destination = Point (Header 2)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($start.X + 15, $start.Y + 210, 12)
    $null = Wait-State { param($s) $s.tabStarts -eq '1' } 'The same held press did not promote'
    $driver.MoveTo($destination.X, $start.Y, 12)
    $state = Invoke-EditorTestCommand $session 'tabs-frame'
    Check ($state.tabDragOpaque -eq 'True') 'Drag-back must restore the opaque local source preview.'
    Start-Raster 'SamePressPromotionAndDragBack'
    $raster.MarkRelease()
    $driver.End($session.ProcessId, $destination.X, $start.Y)
    $state = Wait-State { param($s) $s.tabCompletions -eq '1' } 'Drag-back did not complete'
    $state = Invoke-EditorTestCommand $session 'tabs-frame'
    $state = Wait-State { param($s) $s.tabDragOpaque -eq 'False' } 'Drag-back settlement did not retire its opaque preview'
    Check ($state.tabOrder -eq '1,2,0' -and $state.tabResult -eq 'Move' -and $state.tabPublished -eq '0' -and $state.tabNativeStarts -eq '0') 'Drag-back must commit locally without requesting recovery data or a native duplicate.'
    Check ($state.tabUnloads -eq '0' -and $state.tabDragOpaque -eq 'False') 'Drag-back must retain editor attachment and clear its preview after layout.'
    Check-Keyboard 'DragBack'
    Finish-Motion 'SamePressPromotionAndDragBack' '1,2,0' 0 $true

    Ready
    $start = Point (Header 0)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($start.X + 15, $start.Y + 210, 12)
    $null = Wait-State { param($s) $s.tabStarts -eq '1' } 'Promotion did not start'
    $driver.Escape()
    try { $state = Wait-State { param($s) $s.tabCompletions -eq '1' } 'Windows Escape did not complete' }
    finally { $driver.ReleaseEscape() }
    $driver.End($session.ProcessId, $start.X + 15, $start.Y + 210)
    Check ($state.tabOrder -eq '0,1,2' -and $state.tabResult -eq 'None' -and $state.tabPublished -eq '0') 'External cancellation must retain the draft and its order.'
    Finish 'ExternalEscape'

    Ready
    $start = Point (Header 0)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($start.X + 80, $start.Y, 8)
    $null = Invoke-EditorTestCommand $session 'tabs-close|0'
    $driver.End($session.ProcessId, $start.X + 80, $start.Y)
    $state = State
    Check ($state.tabOrder -eq '1,2' -and $state.tabStarts -eq '0') 'Source closure must retire local capture and preview without promoting.'
    Finish 'SourceClosure'

    Ready 3 'unequal'
    $start = Point (Header 0)
    $destination = Point (Header 2)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destination.X, $start.Y, 12)
    $driver.End($session.ProcessId, $destination.X, $start.Y)
    $null = Wait-State { param($s) $s.tabOrder -eq '1,2,0' } 'Wide source could not reach narrow end'
    Finish 'UnequalWidthEnd'

    Ready 3 'rtl'
    $before = Header 0
    $start = Point $before
    $destination = Point (Header 2)
    Check ($destination.X -lt $start.X) 'RTL collection order must run right to left on screen.'
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($destination.X, $start.Y + 9, 12)
    $null = Invoke-EditorTestCommand $session 'tabs-frame'
    $during = Header 0
    Check ($during.Left -lt $before.Left - 20 -and [Math]::Abs($during.Top - $before.Top) -le 1) 'RTL source must follow the physical pointer left with fixed Y.'
    $driver.End($session.ProcessId, $destination.X, $start.Y + 9)
    $null = Wait-State { param($s) $s.tabOrder -eq '1,2,0' } 'RTL release did not commit the visual destination'
    Finish 'RtlRailGeometryAndReorder'

    foreach ($mode in @('equal', 'rtl')) {
        Ready 12 $mode
        $start = Point (Header 0)
        $arrow = Arrow 'SetsScrollForwardButton'
        $edge = if ($mode -eq 'rtl') { [int]($arrow.Right + 4) } else { [int]($arrow.Left - 4) }
        $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
        $driver.MoveTo($edge, $start.Y, 15)
        $state = Wait-State { param($s) [double]::Parse($s.tabScroll, [cultureinfo]::InvariantCulture) -gt 100 } 'Edge scroll did not advance'
        $back = Arrow 'SetsScrollBackButton'
        $returnEdge = if ($mode -eq 'rtl') { [int]($back.Left - 4) } else { [int]($back.Right + 4) }
        $driver.MoveTo($returnEdge, $start.Y, 15)
        $null = Wait-State { param($s) [double]::Parse($s.tabScroll, [cultureinfo]::InvariantCulture) -lt 1 } 'Opposite edge did not return to the leading boundary'
        $driver.Escape(); $driver.ReleaseEscape()
        $driver.End($session.ProcessId, $returnEdge, $start.Y)
        Check ($state.tabStarts -eq '0') 'Horizontal edge scrolling unexpectedly promoted.'
        Finish "Overflow-$mode"
    }

    # An actual second workspace consumes the durable provider/receipt protocol.
    $null = Invoke-EditorTestCommand $peer 'tabs-start|2|equal'
    $null = Invoke-EditorTestCommand $peer 'tabs-hold-drop|True'
    $driver.Arrange()
    Ready
    $start = Point (Header 0)
    $target = Point (Header 0 $peer)
    $driver.Begin($session.ProcessId, $start.X, $start.Y, $false)
    $driver.MoveTo($start.X, $start.Y + 150, 12)
    $null = Wait-State { param($s) $s.tabStarts -eq '1' } 'Cross-window promotion did not start'
    $driver.MoveTo($target.X, $target.Y, 30)
    $driver.End($peer.ProcessId, $target.X, $target.Y)
    $received = Wait-State { param($s) $s.tabOrder -match '-1' -and $s.tabDropHeld -eq 'True' } 'The receiving workspace did not import the tab under its held drop deferral' $peer
    Check ($received.tabText -eq [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('Tab 00 Ω 😀'))) 'Transferred text must match the independent Unicode fixture.'
    $state = State
    Check ($state.tabOrder -eq '0,1,2' -and $state.tabCompletions -eq '0') 'The source must remain until the receiving window completes its drop deferral.'
    $null = Invoke-EditorTestCommand $peer 'tabs-hold-drop|False'
    $state = Wait-State { param($s) $s.tabOrder -eq '1,2' -and $s.tabCompletions -eq '1' } 'Acknowledged cross-window move did not retire the source'
    Check ($state.tabPublished -eq '1') 'Cross-window move did not exercise the durable data provider.'
    Finish 'CrossWindowReceiptMove'
}
catch {
    if ($session) {
        $_ | Out-String | Set-Content -LiteralPath (Join-Path $session.Root 'tab-dragging-error.txt')
        foreach ($owner in @($session, $peer)) {
            if (!$owner) { continue }
            try {
                State $owner | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root "failure-state-$($owner.ProcessId).json")
            }
            catch { Write-Warning "Could not capture workspace state: $_" }
        }
        $appResult = Join-Path $session.RunRoot 'results.txt'
        if (Test-Path -LiteralPath $appResult) { Copy-Item -LiteralPath $appResult -Destination (Join-Path $session.Root 'tab-failure-results.txt') }
    }
    Write-Host ($_ | Out-String)
    throw
}
finally {
    $cleanupErrors = [Collections.Generic.List[string]]::new()
    if ($raster) { try { $raster.Finish() } catch { $cleanupErrors.Add($_.ToString()) } }
    if ($driver) {
        try {
            if ($driver.HasPressedInput) {
                $source = $handles[[uint32]$driver.SourceProcessId]
                if (!$source) { throw 'No retained handle for the active input source.' }
                if (!$source.HasExited) {
                    $source.Kill()
                    if (!$source.WaitForExit(5000)) { throw 'Owned source did not exit before emergency release.' }
                }
                $driver.ReleaseOwnedInput($source)
            }
        }
        catch { $cleanupErrors.Add($_.ToString()) }
        finally { try { $driver.Dispose() } catch { $cleanupErrors.Add($_.ToString()) } }
    }
    if ($restoreScale) {
        try {
            Set-DisplayScalePercent $settings $settings.Original
            Check ((Get-DisplayScalePercent $settings) -eq $settings.Original) 'Restore the original Windows scale'
        }
        catch { $cleanupErrors.Add("Restoring Windows scale to $($settings.Original)% failed: $_") }
    }
    try { Close-DisplayScaleSettings $settings } catch { $cleanupErrors.Add($_.ToString()) }
    if ($session) {
        try {
            @{ checks = $checks; cases = $results } | ConvertTo-Json -Depth 8 |
                Set-Content -LiteralPath (Join-Path $session.Root 'tab-dragging.json') -Encoding utf8
            Write-Host "Tab dragging evidence: $($session.Root)"
            if ($peer) {
                $null = Invoke-EditorTestCommand $peer 'exit'
                if (!$peer.ProcessIdentity.Process.WaitForExit(10000)) { throw 'Owned peer did not exit.' }
                Assert-IsolatedTestExit $peer.ProcessIdentity
                Close-IsolatedTestProcess $session $peer.ProcessIdentity
            }
        }
        catch { $cleanupErrors.Add($_.ToString()) }
        finally { try { Stop-EditorTestSession $session } catch { $cleanupErrors.Add($_.ToString()) } }
    }
    foreach ($process in $handles.Values) { $process.Dispose() }
    if ($cleanupErrors.Count) { throw ($cleanupErrors -join "`n") }
}

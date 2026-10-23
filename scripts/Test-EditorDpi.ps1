# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# Explicit single-display desktop test: normal mode changes Windows scaling;
# CurrentScaleOnly validates the same fixtures without changing any setting.
param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'ARM64',
    [switch] $CurrentScaleOnly,
    [switch] $PreflightOnly
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'EditorTestSession.ps1')
. (Join-Path $PSScriptRoot 'TestBuildIdentity.ps1')
. (Join-Path $PSScriptRoot 'DisplayScaleSettings.ps1')
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (!('PackagedAppDriver' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'PackagedAppDriver.cs') }
if (!('OwnedWindowActivation' -as [type])) { Add-Type -Path (Join-Path $PSScriptRoot 'OwnedWindowActivation.cs') }
if (!('DisplayDpiEnvironment' -as [type])) {
    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class DisplayDpiEnvironment {
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    public static int MonitorCount { get { return GetSystemMetrics(80); } }
}
'@
}

function Assert-Dpi([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
    $script:checks++
}

function Find-DpiEditor([int] $Index) {
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int] $session.ProcessId),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, "DpiEditor$Index"))
    $element = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if (!$element) { throw "The owned DPI editor $Index was not found." }
    return $element
}

function Test-DpiReady($State, $Before, [double] $Scale) {
    if ([int] $State.panes -lt 1 -or $State.panes -ne $Before.panes -or
        [Math]::Abs([double] $State.windowsScale - $Scale) -ge 0.001) { return $false }
    for ($index = 0; $index -lt [int] $State.panes; $index++) {
        if ([Math]::Abs([double] $State["${index}_scale"] - $Scale) -ge 0.001 -or
            [Math]::Abs([double] $State["${index}_paintScale"] - $Scale) -ge 0.001 -or
            [Math]::Abs([double] $State["${index}_geometryScale"] - $Scale) -ge 0.001 -or
            $State["${index}_pending"] -ne 'False') { return $false }
        if ([Math]::Abs([double] $Before.windowsScale - $Scale) -gt 0.001 -and (
            [int] $State["${index}_dpiEvents"] -le [int] $Before["${index}_dpiEvents"] -or
            [int] $State["${index}_geometryEvents"] -le [int] $Before["${index}_geometryEvents"] -or
            [int] $State["${index}_paints"] -le [int] $Before["${index}_paints"])) { return $false }
    }
    return $true
}

function Wait-Dpi([double] $Scale, $Before) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $state = Invoke-EditorTestCommand $session 'dpi-state'
        if (Test-DpiReady $state $Before $Scale) { return $state }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    $state | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'dpi-timeout.json') -Encoding UTF8
    throw "Windows, XAML, native DPI/geometry events and painting did not converge to scale $Scale."
}

function Validate-Dpi($Ranges, [string] $Label) {
    $state = Invoke-EditorTestCommand $session 'dpi-validate'
    $script:observations.Add([ordered]@{ Label = $Label; State = $state })
    for ($index = 0; $index -lt [int] $state.panes; $index++) {
        $rectangles = @($Ranges[$index].GetBoundingRectangles())
        Assert-Dpi ($rectangles.Count -eq 1) "$Label`: retained UIA caret $index has exactly one visible rectangle."
        $rectangle = $rectangles[0]
        Assert-Dpi ([Math]::Abs($rectangle.Left - [double] $state["${index}_caretX"]) -le 2 -and
            [Math]::Abs($rectangle.Top - [double] $state["${index}_caretY"]) -le 2) "$Label`: retained UIA range uses the live screen transform."
        Assert-Dpi ([Math]::Abs($rectangle.Height - [double] $state["${index}_lineHeight"]) -le 1) "$Label`: UIA line height matches native pixels."
        $element = Find-DpiEditor $index
        Assert-Dpi ($element.Current.BoundingRectangle.Contains($rectangle)) "$Label`: caret is clipped to the current editor bounds."
        $pattern = [Windows.Automation.TextPattern] $element.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
        $hit = $pattern.RangeFromPoint([Windows.Point]::new($rectangle.Left, $rectangle.Top + $rectangle.Height / 2))
        $distance = $hit.CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,
            $Ranges[$index], [Windows.Automation.Text.TextPatternRangeEndpoint]::Start)
        Assert-Dpi ($distance -eq 0) "$Label`: screen-point hit testing returns the observed logical caret (endpoint difference=$distance)."
        Assert-Dpi ($hit.CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,
            $hit, [Windows.Automation.Text.TextPatternRangeEndpoint]::End) -eq 0) "$Label`: screen-point hit testing returns an empty range."
    }
    return $state
}

$session = $null
$settings = $null
$restoreNeeded = $false
$script:checks = 0
$script:observations = [Collections.Generic.List[object]]::new()
$failure = $null
$cleanupErrors = [Collections.Generic.List[string]]::new()
$displayRecord = [ordered]@{ CurrentScaleOnly = [bool] $CurrentScaleOnly; OriginalPercent = $null; LastRequestedPercent = $null; Restoration = 'NotRequired' }
try {
    Assert-Dpi ([DisplayDpiEnvironment]::MonitorCount -eq 1) 'DisplayDpi requires exactly one active display; multi-monitor topology is a separate test.'
    if (!$CurrentScaleOnly -or $PreflightOnly) {
        $settings = Open-DisplayScaleSettings
        Assert-DisplayScaleAvailable $settings
    }
    if ($PreflightOnly) { Write-Output "PASS: Windows offers standard display scaling; original=$($settings.Original)%. No setting changed." }
    else {
        $null = Assert-TestBuildIdentity App Release $Platform
        $session = Start-EditorTestSession -Platform $Platform
        Write-Output "Evidence: $($session.Root)"
        $original = if ($settings) { $settings.Original } else { $null }
        $displayRecord.OriginalPercent = $original
        $displayRecord |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'display-settings.json') -Encoding UTF8
        @('NotepadsIntegrationTests.exe', 'WinUIEditor.dll') | ForEach-Object {
            Get-FileHash -LiteralPath (Join-Path $session.Layout $_) -Algorithm SHA256
        } | Select-Object Path, Hash | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'dpi-binary-hashes.json') -Encoding UTF8
        $foregroundAcquired = $false
        foreach ($rtl in @($false, $true)) {
            foreach ($mode in @('Unwrapped', 'Wrapped', 'Diff')) {
                $wrap = $mode -eq 'Wrapped'
                $diff = $mode -eq 'Diff'
                $before = Invoke-EditorTestCommand $session "dpi-prepare|$rtl|$wrap|$diff"
                if (!$foregroundAcquired) {
                    $element = Find-DpiEditor 0
                    [OwnedWindowActivation]::RequireIdle()
                    $element.SetFocus()
                    $null = [OwnedWindowActivation]::Acquire([uint32] $session.ProcessId, [PackagedAppDriver]::Window([uint32] $session.ProcessId))
                    $foregroundAcquired = $true
                }
                $ranges = @(for ($index = 0; $index -lt [int] $before.panes; $index++) {
                    $pattern = [Windows.Automation.TextPattern] (Find-DpiEditor $index).GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
                    $range = $pattern.DocumentRange.Clone()
                    $range.MoveEndpointByRange([Windows.Automation.Text.TextPatternRangeEndpoint]::End, $range, [Windows.Automation.Text.TextPatternRangeEndpoint]::Start)
                    $moved = $range.MoveEndpointByUnit([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,
                        [Windows.Automation.Text.TextUnit]::Character, [int] $before["${index}_character"])
                    Assert-Dpi ($moved -eq [int] $before["${index}_character"]) 'Retained UIA range reaches the fixture caret.'
                    $range
                })
                $baseline = Validate-Dpi $ranges "RTL=$rtl $mode initial"
                if ($settings) {
                    Assert-Dpi ([Math]::Abs([double] $baseline.windowsScale * 100 - $original) -lt 0.01) 'Settings and the test window agree on the original scale.'
                }
                $scales = if ($CurrentScaleOnly) { @() } else { @(100, 125, 150, 200, $original) }
                foreach ($percent in $scales) {
                    Assert-Dpi ([DisplayDpiEnvironment]::MonitorCount -eq 1) 'Display topology remained unchanged.'
                    $before = Invoke-EditorTestCommand $session 'dpi-state'
                    $restoreNeeded = $true # Selection can succeed even if a later UIA acknowledgement fails.
                    $displayRecord.LastRequestedPercent = $percent
                    $displayRecord.Restoration = 'Pending'
                    $displayRecord | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'display-settings.json') -Encoding UTF8
                    Set-DisplayScalePercent $settings $percent
                    $null = Wait-Dpi ($percent / 100.0) $before
                    $state = Validate-Dpi $ranges "RTL=$rtl $mode $percent%"
                    for ($index = 0; $index -lt [int] $state.panes; $index++) {
                        $ratio = [double] $state.windowsScale / [double] $baseline.windowsScale
                        Assert-Dpi ([Math]::Abs([double] $state["${index}_fontWidth"] - [double] $baseline["${index}_fontWidth"] * $ratio) -le 3) 'Native font metrics follow system scale.'
                    }
                }
                Write-Output "PASS: RTL=$rtl $mode ($($scales.Count) requested system scales)."
            }
        }
        Write-Output "PASS: $script:checks DPI checks. CurrentScaleOnly=$CurrentScaleOnly."
    }
}
catch { $failure = $_ }
finally {
    if ($restoreNeeded -and $settings) {
        try {
            Set-DisplayScalePercent $settings $settings.Original
            Assert-Dpi ((Get-DisplayScalePercent $settings) -eq $settings.Original) 'Original Windows scale was restored.'
            $displayRecord.Restoration = 'Verified'
            $displayRecord |
                ConvertTo-Json | Set-Content -LiteralPath (Join-Path $session.Root 'display-settings.json') -Encoding UTF8
        }
        catch { $cleanupErrors.Add("Restoring Windows scale failed; original=$($settings.Original)%: $_") }
    }
    try { Close-DisplayScaleSettings $settings } catch { $cleanupErrors.Add($_.ToString()) }
    if ($session) {
        try { $script:observations | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $session.Root 'dpi-observations.json') -Encoding UTF8 }
        catch { $cleanupErrors.Add($_.ToString()) }
        try { Stop-EditorTestSession $session } catch { $cleanupErrors.Add($_.ToString()) }
    }
}
if ($cleanupErrors.Count) { throw "$failure`n$($cleanupErrors -join "`n")" }
if ($failure) { throw $failure }

# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

# System-settings adapter for the explicit DisplayDpi test. No registry writes,
# undocumented DisplayConfig packets, custom scaling, sign-out or driver install.
function Read-DisplayScalePercent([string] $Name) {
    if ($Name -notmatch '^\s*([0-9]+)\s*[%％]') { throw "Unrecognized display scale label: $Name" }
    return [int] $Matches[1]
}

function Get-DisplayScaleCombo($Settings) {
    $Settings.Process.Refresh()
    if ($Settings.Process.HasExited -or $Settings.Process.StartTime.ToUniversalTime().Ticks -ne $Settings.StartTicks) {
        throw 'The display Settings process changed; refusing a different process.'
    }
    $root = [Windows.Automation.AutomationElement]::RootElement.FindFirst(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, [int] $Settings.Process.Id))
    if (!$root) { throw 'The owned display Settings content is unavailable.' }
    $combo = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,
            'SystemSettings_Display_Scaling_ItemSizeOverride_ComboBox'))
    if (!$combo -or $combo.Current.ProcessId -ne $Settings.Process.Id) { throw 'The Windows display scale combo is unavailable.' }
    return $combo
}

function Get-DisplayScalePercent($Settings) {
    $combo = Get-DisplayScaleCombo $Settings
    $selection = ([Windows.Automation.SelectionPattern] $combo.GetCurrentPattern([Windows.Automation.SelectionPattern]::Pattern)).Current.GetSelection()
    if ($selection.Count -ne 1) { throw 'Display scaling does not expose exactly one selected value.' }
    return Read-DisplayScalePercent $selection[0].Current.Name
}

function Assert-DisplayScaleAvailable($Settings) {
    if (!(Get-DisplayScaleCombo $Settings).Current.IsEnabled) {
        throw 'Windows has disabled standard display scaling (for example, custom scaling is active). No setting was changed. Disable custom scaling and sign in again before running DisplayDpi.'
    }
}

function Open-DisplayScaleSettings {
    param([ValidateRange(0, 60)] [int] $TimeoutSeconds = 15)
    $before = @(Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object {
        [PackagedAppDriver]::Window([uint32] $_.Id) -ne [IntPtr]::Zero
    } | ForEach-Object Id)
    $settings = $null
    try {
        Start-Process 'ms-settings:display'
        $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
        do {
            $processes = @(Get-Process SystemSettings -ErrorAction SilentlyContinue)
            if ($processes.Count -eq 1) {
                $process = $processes[0]
                if (![string]::Equals($process.Path, (Join-Path $env:SystemRoot 'ImmersiveControlPanel/SystemSettings.exe'), [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'The Settings executable does not match Windows Settings.'
                }
                $settings = @{ Process = $process; StartTicks = $process.StartTime.ToUniversalTime().Ticks; Opened = $process.Id -notin $before }
                try { $settings.Original = Get-DisplayScalePercent $settings; return $settings } catch { $lastError = $_ }
            }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        throw "Windows display Settings did not become ready: $lastError"
    }
    catch {
        $failure = $_
        # Ownership transfers to the caller only after discovery succeeds.
        try { Close-DisplayScaleSettings $settings }
        catch { throw "$failure`nClosing the owned Settings window failed: $_" }
        throw $failure
    }
}

function Set-DisplayScalePercent($Settings, [int] $Percent) {
    Assert-DisplayScaleAvailable $Settings
    $current = Get-DisplayScalePercent $Settings
    if ($current -eq $Percent) { return }
    $combo = Get-DisplayScaleCombo $Settings
    $expand = [Windows.Automation.ExpandCollapsePattern] $combo.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)
    $expand.Expand()
    try {
        $items = $combo.FindAll([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem))
        $matches = @($items | Where-Object { (Read-DisplayScalePercent $_.Current.Name) -eq $Percent })
        if ($matches.Count -ne 1) { throw "The display does not offer exactly one $Percent% scale option." }
        ([Windows.Automation.SelectionItemPattern] $matches[0].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)).Select()
    }
    finally { $expand.Collapse() }
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if ((Get-DisplayScalePercent $Settings) -eq $Percent) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Windows did not select $Percent% display scaling."
}

function Close-DisplayScaleSettings($Settings) {
    if (!$Settings -or !$Settings.Opened) { return }
    # Close only the Settings window this run opened; never terminate the
    # process or a pre-existing Settings window used by someone else.
    $Settings.Process.Refresh()
    if ($Settings.Process.HasExited) { return }
    if ($Settings.Process.StartTime.ToUniversalTime().Ticks -ne $Settings.StartTicks) {
        throw 'The display Settings process changed; refusing to close a different process.'
    }
    $window = [PackagedAppDriver]::Window([uint32] $Settings.Process.Id)
    if ($window -ne [IntPtr]::Zero) {
        $root = [Windows.Automation.AutomationElement]::FromHandle($window)
        ([Windows.Automation.WindowPattern] $root.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)).Close()
    }
}

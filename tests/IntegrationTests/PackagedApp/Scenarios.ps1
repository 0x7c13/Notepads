# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE.txt in the project root for license information.
# ---------------------------------------------------------------------------------------------

param($Driver, $Installed, [string] $LocalState, [string] $EvidenceRoot,
    [Collections.Generic.List[object]] $Cases)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../../../scripts/PackagedAppProcess.ps1')
$owned = [Collections.Generic.List[IsolatedTestProcess]]::new()
$completedExits = [Collections.Generic.HashSet[object]]::new()
$exitEvidence = Join-Path $EvidenceRoot 'process-exits.jsonl'
$expectedExe = Join-Path $Installed.InstallLocation 'Notepads.exe'
$descendants = [Windows.Automation.TreeScope]::Descendants
$elementType = [Windows.Automation.AutomationElement]
$assertions = 0

function Check([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
    $script:assertions++
}

function Wait-Until([scriptblock] $Condition, [string] $Message, [int] $Seconds = 30) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $value = & $Condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 100
    } while ($timer.Elapsed.TotalSeconds -lt $Seconds)
    throw "Timed out: $Message"
}

function Run-Case([string] $Name, [scriptblock] $Body) {
    $item = [ordered]@{ Name = $Name; Status = 'RUNNING'; Assertions = 0; Milliseconds = 0; Error = $null }
    $Cases.Add($item)
    $before = $script:assertions
    $timer = [Diagnostics.Stopwatch]::StartNew()
    try { & $Body; $item.Status = 'PASS'; Write-Host "PASS: $Name" }
    catch { $item.Status = 'FAIL'; $item.Error = $_.ToString(); throw }
    finally {
        $item.Assertions = $script:assertions - $before
        $item.Milliseconds = $timer.ElapsedMilliseconds
        ConvertTo-Json -InputObject @($Cases.ToArray()) -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'cases.json')
    }
}

function Get-AppWindows {
    foreach ($process in @(Get-Process -Name Notepads -ErrorAction SilentlyContinue)) {
        $retained = @($owned | Where-Object { $_.Id -eq $process.Id -and !$_.HasExited })
        if (!$retained.Count) {
            try { $handle = [IsolatedTestProcess]::new($process.Id) }
            catch {
                if (!$process.HasExited) { throw }
                continue
            }
            if ($handle.Path -ine $expectedExe) { $handle.Dispose(); throw 'Unexpected Notepads process identity.' }
            $owned.Add($handle)
            $retained = @($handle)
        }
        if (!$retained[0].HasExited -and [PackagedAppDriver]::Window($process.Id) -ne [IntPtr]::Zero) { $retained[0] }
    }
}

function Wait-Windows([int] $Count) {
    $null = Wait-Until { @(Get-AppWindows).Count -eq $Count } "$Count visible application windows"
    # Reject delayed duplicate windows from redirected activation as well.
    Start-Sleep -Milliseconds 400
    Check (@(Get-AppWindows).Count -eq $Count) 'Window count changed after activation settled.'
}

function Get-Root($App) {
    Check (!$App.HasExited) 'The owned application process exited unexpectedly.'
    $window = [PackagedAppDriver]::Window($App.Id)
    if ($window -eq [IntPtr]::Zero) { return $null }
    return [Windows.Automation.AutomationElement]::FromHandle($window)
}

function Find-UI($App, [string] $Value, [switch] $Name) {
    $root = Get-Root $App
    if (!$root) { return $null }
    $property = if ($Name) { $elementType::NameProperty } else { $elementType::AutomationIdProperty }
    $condition = [Windows.Automation.PropertyCondition]::new($property, $Value)
    return $root.FindFirst($descendants, $condition)
}

function Get-Editor($App) {
    $root = Get-Root $App
    if (!$root) { return $null }
    $condition = [Windows.Automation.PropertyCondition]::new($elementType::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)
    foreach ($element in $root.FindAll($descendants, $condition)) {
        if ($element.Current.ProcessId -eq $App.Id -and !$element.Current.IsOffscreen -and
            $element.Current.ClassName -like '*EditorBaseControl*') { return $element }
    }
    return $null
}

function Get-Text($App) {
    $editor = Get-Editor $App
    if (!$editor) { return $null }
    return $editor.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern).DocumentRange.GetText(-1)
}

function Expect-Text($App, [string] $Text) {
    $null = Wait-Until { (Get-Text $App) -ceq $Text } "selected document text [$Text]"
    Check ((Get-Text $App) -ceq $Text) 'Selected document text differs.'
}

function Focus-Editor($App) {
    [PackagedAppDriver]::Focus($App.Id)
    $editor = Wait-Until { Get-Editor $App } 'visible editor'
    $editor.SetFocus()
    $null = Wait-Until {
        $current = (Get-Editor $App).Current
        $current.HasKeyboardFocus -and $current.IsEnabled
    } 'editor has keyboard focus and accepts input'
}

function Key($App, [int] $Key, [bool] $Control = $false, [bool] $Shift = $false) {
    Check (!$App.HasExited) 'Cannot send input to an exited application.'
    [PackagedAppDriver]::Key($App.Id, $Key, $Control, $Shift)
}

function Set-Text($App, [string] $Text) {
    Focus-Editor $App
    $editor = Get-Editor $App
    $pattern = $editor.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    if ($pattern.DocumentRange.GetText(-1).Length -gt 0) {
        Key $App 0x41 $true
        $null = Wait-Until {
            $document = $pattern.DocumentRange
            $selection = @($pattern.GetSelection())
            $selection.Count -eq 1 -and
                $selection[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::Start,
                    $document, [Windows.Automation.Text.TextPatternRangeEndpoint]::Start) -eq 0 -and
                $selection[0].CompareEndpoints([Windows.Automation.Text.TextPatternRangeEndpoint]::End,
                    $document, [Windows.Automation.Text.TextPatternRangeEndpoint]::End) -eq 0
        } 'entire document is selected before replacement input'
    }
    [PackagedAppDriver]::Type($App.Id, $Text)
    Expect-Text $App $Text
}

function Get-Tabs($App) {
    # SetsView itself is a layout-only element in the UIA control view; its
    # ListViewItem peers are exposed directly beneath the app window.
    $root = Get-Root $App
    if (!$root) { return @() }
    # Count the logical header roster, including tabs clipped by horizontal
    # scrolling. Match the tab headers' peer class as well as their ListItem role.
    $condition = [Windows.Automation.PropertyCondition]::new($elementType::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)
    return @($root.FindAll($descendants, $condition) | Where-Object {
        $_.Current.ProcessId -eq $App.Id -and $_.Current.ClassName -eq 'ListViewItem'
    })
}

function Expect-TabCount($App, [int] $Count) {
    $null = Wait-Until { @(Get-Tabs $App).Count -eq $Count } "exactly $Count tab headers"
    Check (@(Get-Tabs $App).Count -eq $Count) "Expected exactly $Count tabs."
}

function Get-TabTitle($Tab) {
    $condition = [Windows.Automation.PropertyCondition]::new($elementType::ControlTypeProperty, [Windows.Automation.ControlType]::Text)
    return $Tab.FindFirst($descendants, $condition).Current.Name
}

function Select-Tab($App, [int] $Index) {
    $tabs = @(Get-Tabs $App)
    Check ($Index -ge 0 -and $Index -lt $tabs.Count -and $Index -lt 9) 'Tab index is outside the shortcut fixture range.'
    $title = Get-TabTitle $tabs[$Index]
    Check (![string]::IsNullOrWhiteSpace($title)) 'The tab must expose a readable title.'
    # SetsView uses its own selection model; its ListViewItem peers do not
    # implement SelectionItemPattern. Drive the real Ctrl+number shortcut.
    Focus-Editor $App
    Key $App (0x31 + $Index) $true
    $null = Wait-Until {
        $path = Find-UI $App 'PathIndicatorButton'
        $path -and $path.Current.Name.EndsWith($title, [StringComparison]::Ordinal) -and (Get-Editor $App)
    } 'selected tab path and editor'
}

function Invoke-UI($Element) {
    Check $Element.Current.IsEnabled ("Control is disabled: " + $Element.Current.AutomationId + ' ' + $Element.Current.Name)
    $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Set-Preference($App, [string] $Id, [bool] $Enabled) {
    [PackagedAppDriver]::Focus($App.Id)
    Invoke-UI (Wait-Until { Find-UI $App 'MainMenuButton' } 'main menu')
    Invoke-UI (Wait-Until { Find-UI $App 'MenuSettingsButton' } 'settings menu item')
    $advanced = Wait-Until { Find-UI $App 'Advanced' -Name } 'Advanced settings navigation'
    $advanced.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
    $toggle = Wait-Until { Find-UI $App $Id } "preference $Id"
    $pattern = $toggle.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
    $expected = if ($Enabled) { [Windows.Automation.ToggleState]::On } else { [Windows.Automation.ToggleState]::Off }
    if ($pattern.Current.ToggleState -ne $expected) { $pattern.Toggle() }
    $null = Wait-Until { $pattern.Current.ToggleState -eq $expected } 'preference state'
    Check ($pattern.Current.ToggleState -eq $expected) 'Preference UI did not retain the selected value.'
    Key $App 0x1B
    Focus-Editor $App
}

function Close-All {
    $closeRequested = [Collections.Generic.HashSet[object]]::new()
    foreach ($app in @(Get-AppWindows)) {
        Check (!$app.HasExited) 'The application exited before its normal close request.'
        [PackagedAppDriver]::Close($app.Id)
        $null = $closeRequested.Add($app)
    }
    Wait-Windows 0
    # UWP may retain a suspended process after normal close. End that already
    # closed instance so the next launch proves disk recovery, not memory reuse.
    Stop-PackagedAppProcesses -Processes $owned -Completed $completedExits `
        -EvidencePath $exitEvidence -Reason 'Normal window close' -CloseRequested $closeRequested
}

function Launch-One {
    $null = $Driver.Launch()
    Wait-Windows 1
    $app = @(Get-AppWindows)[0]
    $null = Wait-Until { Get-Editor $app } 'initial editor ready'
    return $app
}

try {
    Run-Case 'Installed Production launch, Unicode editing and session preference' {
        $script:main = Launch-One
        Expect-Text $main ''
        Check (@(Get-Tabs $main).Count -eq 1) 'Fresh installation must contain one tab.'
        Set-Preference $main 'EnableSessionSnapshotToggleSwitch' $true
        Set-Text $main 'unsaved startup Ω 世界 😀'
    }

    $fixtures = Join-Path $EvidenceRoot 'files'
    $null = New-Item -ItemType Directory -Path $fixtures
    $firstFile = Join-Path $fixtures 'activation-first.txt'
    $secondFile = Join-Path $fixtures 'activation-second.txt'
    [IO.File]::WriteAllText($firstFile, 'first file Ω', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($secondFile, 'second file 世界', [Text.UTF8Encoding]::new($false))

    Run-Case 'Normal close restores dirty text, tab membership and selected tab' {
        $closeFile = Join-Path $fixtures 'close-recovery.txt'
        $diskText = 'original close-recovery file'
        [IO.File]::WriteAllText($closeFile, $diskText, [Text.UTF8Encoding]::new($false))
        $null = $Driver.OpenFile($closeFile)
        Wait-Windows 1
        Expect-Text $main $diskText
        # Build a known two-document workspace before the multiwindow cases.
        # Expected membership/content must not come from a partial startup tree.
        $script:tabCount = 2
        $script:tabTexts = @('unsaved startup Ω 世界 😀', 'close recovery é العربية 😀')
        Expect-TabCount $main $tabCount
        Set-Text $main $tabTexts[1]
        for ($index = 0; $index -lt $tabCount; $index++) {
            Select-Tab $main $index
            Expect-Text $main $tabTexts[$index]
        }
        Check ([IO.File]::ReadAllText($closeFile) -ceq $diskText) 'The close fixture must retain unsaved edits instead of saving them to disk.'
        [ordered]@{ Count = $tabCount; Texts = $tabTexts } |
            ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'normal-close-tabs.json')
        Expect-Text $main 'close recovery é العربية 😀'
        Close-All
        Check ([IO.File]::ReadAllText($closeFile) -ceq $diskText) 'Normal close must checkpoint dirty text without saving the file.'
        $script:main = Launch-One
        Expect-Text $main 'close recovery é العربية 😀'
        Expect-TabCount $main $tabCount
        for ($index = 0; $index -lt $tabCount; $index++) {
            Select-Tab $main $index
            Expect-Text $main $tabTexts[$index]
        }
    }

    Run-Case 'OS suspend/resume preserves live edits and accepts further input' {
        Set-Text $main 'suspend recovery Ω😀'
        $Driver.Suspend()
        $null = Wait-Until { $Driver.State -eq 3 } 'OS package suspended'
        Check ($Driver.State -eq 3) 'Package never entered the suspended state.'
        $Driver.Resume()
        $null = Wait-Until { $Driver.State -eq 1 } 'OS package resumed'
        Expect-Text $main 'suspend recovery Ω😀'
        Set-Text $main 'resumed edit 世界'
    }

    Run-Case 'Termination after OS suspension restores the completed suspension checkpoint' {
        $Driver.Suspend()
        $null = Wait-Until { $Driver.State -eq 3 } 'second OS suspension'
        Stop-PackagedAppProcesses -Processes $owned -Completed $completedExits `
            -EvidencePath $exitEvidence -Reason 'Termination after OS suspension'
        $script:main = Launch-One
        Expect-Text $main 'resumed edit 世界'
        Expect-TabCount $main $tabCount
        Close-All
    }

    Run-Case 'Warm Windows file activation, duplicate reuse and real file save' {
        $script:main = Launch-One
        Expect-Text $main 'resumed edit 世界'
        Expect-TabCount $main $tabCount
        $null = $Driver.OpenFile($firstFile)
        Wait-Windows 1
        Expect-Text $main 'first file Ω'
        Expect-TabCount $main 3
        $null = $Driver.OpenFile($firstFile)
        Wait-Windows 1
        Expect-Text $main 'first file Ω'
        Expect-TabCount $main 3
        Set-Text $main 'saved through packaged app 😀'
        Key $main 0x53 $true
        $null = Wait-Until { [IO.File]::ReadAllText($firstFile) -ceq 'saved through packaged app 😀' } 'file save bytes'
        Check ([IO.File]::ReadAllText($firstFile) -ceq 'saved through packaged app 😀') 'Save did not update the activated file.'
    }

    Run-Case 'New-window preference applies without restarting' {
        Set-Preference $main 'AlwaysOpenNewWindowToggleSwitch' $true
        $null = $Driver.OpenFile($secondFile)
        Wait-Windows 2
        $script:second = @(Get-AppWindows | Where-Object Id -ne $main.Id)[0]
        Expect-Text $second 'second file 世界'
        Expect-Text $main 'saved through packaged app 😀'
        Check ($second.Id -ne $main.Id) 'New-window preference reused the original instance.'
    }

    Run-Case 'New-window preference persists after normal close and cold activation' {
        Close-All
        $null = $Driver.OpenFile($firstFile)
        Wait-Windows 1
        $script:main = @(Get-AppWindows)[0]
        Expect-Text $main 'saved through packaged app 😀'
        $null = $Driver.OpenFile($secondFile)
        Wait-Windows 2
        $script:second = @(Get-AppWindows | Where-Object Id -ne $main.Id)[0]
        Expect-Text $second 'second file 世界'
        # Preferences are intentionally edited in the primary window. Activation
        # must still read the new value when redirected to a secondary window.
        Set-Preference $main 'AlwaysOpenNewWindowToggleSwitch' $false
        Focus-Editor $second
        $tabsBefore = @(Get-Tabs $second).Count
        $null = $Driver.OpenFile($firstFile)
        Wait-Windows 2
        Expect-Text $second 'saved through packaged app 😀'
        Check (@(Get-Tabs $second).Count -eq $tabsBefore + 1) 'Redirected file did not reach the last active window.'
        Close-All
    }

    Run-Case 'Explorer default-file association opens the installed application' {
        # Use Windows' own default-app UI, never forge the protected UserChoice
        # hash. This entire user profile belongs to the disposable hosted VM.
        Start-Process 'ms-settings:defaultapps'
        $settings = Wait-Until {
            foreach ($process in @(Get-Process SystemSettings -ErrorAction SilentlyContinue)) {
                if ([PackagedAppDriver]::Window($process.Id) -ne [IntPtr]::Zero) {
                    return [IsolatedTestProcess]::new($process.Id)
                }
            }
        } 'Windows default-app settings'
        try {
            [PackagedAppDriver]::Focus($settings.Id)
            $search = Wait-Until {
                $condition = [Windows.Automation.PropertyCondition]::new($elementType::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)
                $root = Get-Root $settings
                if (!$root) { return $null }
                foreach ($edit in $root.FindAll($descendants, $condition)) {
                    if ($edit.Current.Name -match 'file type|file extension') { return $edit }
                }
            } 'default-app file extension search'
            $search.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue('.txt')
            $association = Wait-Until {
                $root = Get-Root $settings
                if (!$root) { return $null }
                foreach ($element in $root.FindAll($descendants, [Windows.Automation.Condition]::TrueCondition)) {
                    if ($element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $element.Current.Name -match '\.txt') { return $element }
                }
            } '.txt default application selector'
            Check ($association.Current.Name -match '^\.txt, Notepad,') 'The disposable image must begin with Windows Notepad as .txt default.'
            Invoke-UI $association
            # OpenWith's immersive window is not enumerated by EnumWindows and
            # exposes no UIA descendants on this image. Use its real keyboard UI;
            # never synthesize UserChoice registry values or bypass its hash.
            $picker = Wait-Until {
                $foregroundPid = [PackagedAppDriver]::ForegroundProcess()
                if (!$foregroundPid) { return $null }
                $process = Get-Process -Id $foregroundPid -ErrorAction SilentlyContinue
                if ($process -and $process.Name -eq 'OpenWith') { return $process }
            } 'foreground Windows file handler picker'
            try {
                Check ($picker.Path -ieq (Join-Path $env:windir 'System32/OpenWith.exe')) 'Unexpected default-app picker executable.'
                function Wait-PickerFocus([string] $Pattern) {
                    return Wait-Until {
                        $focused = $elementType::FocusedElement
                        if ($focused -and $focused.Current.ProcessId -eq $picker.Id -and
                            $focused.Current.Name -match $Pattern) { return $focused }
                    } "Windows picker focus: $Pattern"
                }
                function Send-PickerKey([int] $Key, [bool] $Shift = $false) {
                    # OpenWith publishes focus before its keyboard transition
                    # finishes. Pace real input as well as verifying each target.
                    Start-Sleep -Milliseconds 100
                    [PackagedAppDriver]::Key($picker.Id, $Key, $false, $Shift)
                }
                $confirm = Wait-PickerFocus '^Set default$'
                Send-PickerKey 0x09 $true
                $null = Wait-PickerFocus '^Choose an app on your PC$'
                Send-PickerKey 0x09 $true
                $null = Wait-PickerFocus '^(?!Choose an app on your PC$).+'
                Send-PickerKey 0x24
                $null = Wait-PickerFocus '^Notepad$'
                Send-PickerKey 0x28
                $null = Wait-PickerFocus '^Suggested apps$'
                Send-PickerKey 0x28
                $choice = Wait-PickerFocus '^Notepads(?:, new)?$'
                # FocusedElement reaches the actual peer even though desktop
                # tree enumeration cannot traverse this immersive popup.
                $selection = $choice.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)
                $selection.Select()
                $null = Wait-Until { $selection.Current.IsSelected } 'Notepads selected in Windows picker'
                Invoke-UI $confirm
            }
            finally { $picker.Dispose() }
            # This strict readback detects image/picker layout changes before
            # Explorer is invoked, rather than silently opening a different app.
            $null = Wait-Until {
                $root = Get-Root $settings
                if (!$root) { return $null }
                foreach ($element in $root.FindAll($descendants, [Windows.Automation.Condition]::TrueCondition)) {
                    if ($element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
                        $element.Current.Name -match '^\.txt, Notepads,') { return $element }
                }
            } 'Windows confirms Notepads as .txt default'
            [PackagedAppDriver]::Close($settings.Id)

            $explorerFile = Join-Path $fixtures 'explorer-activation.txt'
            [IO.File]::WriteAllText($explorerFile, 'Explorer association Ω😀', [Text.UTF8Encoding]::new($false))
            Start-Process explorer.exe -ArgumentList ('/select,"' + $explorerFile + '"')
            $fileItem = Wait-Until {
                $condition = [Windows.Automation.PropertyCondition]::new($elementType::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)
                foreach ($element in $elementType::RootElement.FindAll($descendants, $condition)) {
                    if ($element.Current.Name -in 'explorer-activation', 'explorer-activation.txt' -and !$element.Current.IsOffscreen) { return $element }
                }
            } 'Explorer fixture file'
            # Explorer's Invoke pattern performs the item's default action,
            # following the same association as a double-click on that file.
            Invoke-UI $fileItem
            Wait-Windows 1
            $script:main = @(Get-AppWindows)[0]
            Expect-Text $main 'Explorer association Ω😀'
            Check ($main.Path -ieq $expectedExe) 'Explorer activated a different application.'
            Close-All
        }
        finally { $settings.Dispose() }
    }

    Run-Case 'V1 session fixture imports pending Unicode backups and selected tab' {
        # This LocalState belongs exclusively to the package installed by this
        # test. Keep LocalSettings (snapshot preference), replace only fixture data.
        $expectedState = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "Packages/$($Installed.PackageFamilyName)/LocalState"))
        Check ([IO.Path]::GetFullPath($LocalState) -eq $expectedState) 'Refusing an unexpected migration fixture directory.'
        Check (@(Get-AppWindows).Count -eq 0) 'Migration seed requires a stopped package.'
        Remove-Item -LiteralPath $expectedState -Recurse -Force
        $null = New-Item -ItemType Directory -Path $expectedState
        $ids = @([Guid]::NewGuid().ToString(), [Guid]::NewGuid().ToString())
        $script:legacyTexts = @('V1 pending UTF-8 Ω😀', 'V1 pending UTF-16 世界 é')
        $records = for ($index = 0; $index -lt 2; $index++) {
            $saved = Join-Path $expectedState "v1-saved-$index.txt"
            $pending = Join-Path $expectedState "v1-pending-$index.txt"
            $encoding = if ($index -eq 0) { [Text.UTF8Encoding]::new($false) } else { [Text.UnicodeEncoding]::new($false, $true) }
            [IO.File]::WriteAllText($saved, "V1 saved $index", $encoding)
            [IO.File]::WriteAllText($pending, $legacyTexts[$index], $encoding)
            @{
                Id = $ids[$index]; LastSavedBackupFilePath = $saved; PendingBackupFilePath = $pending
                StateMetaData = @{
                    FileNamePlaceholder = "legacy-$index.txt"; LastSavedEncoding = $encoding.WebName
                    LastSavedLineEnding = 'CR'; FontZoomFactor = 1.0; HasEditingFile = $false
                    IsModified = $true; RequiresSaveAs = $true
                }
            }
        }
        @{ Version = 1; SelectedTextEditor = $ids[1]; TextEditors = @($records); TabScrollViewerHorizontalOffset = 0 } |
            ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $expectedState 'NotepadsSessionData.json') -Encoding utf8
        $script:main = Launch-One
        Expect-Text $main $legacyTexts[1]
        $null = Wait-Until { @(Get-Tabs $main).Count -ge 2 } 'complete V1 imported tab roster'
        Check (@(Get-Tabs $main).Count -eq 2) 'V1 import did not restore exactly two tabs.'
        Focus-Editor $main
        Key $main 0x31 $true
        Expect-Text $main $legacyTexts[0]
        Key $main 0x32 $true
        Expect-Text $main $legacyTexts[1]
        Close-All
    }

    Run-Case 'V1 migration is consumed once across repeated restarts and tab closure' {
        $script:main = Launch-One
        Expect-Text $main $legacyTexts[1]
        Check (@(Get-Tabs $main).Count -eq 2) 'Restart duplicated V1 imported tabs.'
        Focus-Editor $main
        Key $main 0x57 $true
        Invoke-UI (Wait-Until { Find-UI $main "Don't Save" -Name } 'discard migrated tab confirmation')
        $null = Wait-Until { @(Get-Tabs $main).Count -eq 1 } 'closing migrated tab'
        Expect-Text $main $legacyTexts[0]
        Close-All
        $script:main = Launch-One
        Expect-Text $main $legacyTexts[0]
        Check (@(Get-Tabs $main).Count -eq 1) 'Consumed V1 migration resurrected an explicitly closed tab.'
        Close-All
    }

    Run-Case 'Consecutive cold file activations retain both files and session recovery' {
        $null = $Driver.OpenFile($firstFile)
        $null = $Driver.OpenFile($secondFile)
        Wait-Windows 1
        $script:main = @(Get-AppWindows)[0]
        Expect-Text $main 'second file 世界'
        Check (@(Get-Tabs $main).Count -eq 3) 'Cold activations lost or duplicated a file/recovered tab.'
        Select-Tab $main 0
        Expect-Text $main $legacyTexts[0]
        Select-Tab $main 1
        Expect-Text $main 'saved through packaged app 😀'
        Select-Tab $main 2
        Expect-Text $main 'second file 世界'
        Close-All
    }


}
catch {
    try {
        $tree = foreach ($app in @(Get-AppWindows)) {
            (Get-Root $app).FindAll($descendants, [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
                [ordered]@{ Pid = $_.Current.ProcessId; Type = $_.Current.ControlType.ProgrammaticName
                    Id = $_.Current.AutomationId; Name = $_.Current.Name; Class = $_.Current.ClassName; Offscreen = $_.Current.IsOffscreen }
            }
        }
        ConvertTo-Json -InputObject @($tree) -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'uia-failure.json')
        $foreground = $elementType::FromHandle([PackagedAppDriver]::ForegroundWindow())
        $foreground.FindAll($descendants, [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            [ordered]@{ Pid = $_.Current.ProcessId; Type = $_.Current.ControlType.ProgrammaticName;
                Id = $_.Current.AutomationId; Name = $_.Current.Name; Class = $_.Current.ClassName; Offscreen = $_.Current.IsOffscreen }
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'foreground-uia-failure.json')
        $desktop = $elementType::RootElement.FindAll($descendants, [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            [ordered]@{ Pid = $_.Current.ProcessId; Type = $_.Current.ControlType.ProgrammaticName
                Id = $_.Current.AutomationId; Name = $_.Current.Name; Class = $_.Current.ClassName; Offscreen = $_.Current.IsOffscreen }
        }
        ConvertTo-Json -InputObject @($desktop) -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'desktop-uia-failure.json')
    }
    catch { Write-Warning "UIA failure dump unavailable: $_" }
    throw
}
finally { foreach ($app in $owned) { $app.Dispose() } }
